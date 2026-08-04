// Browser-native continuous speech capture + authenticated STT WebSocket transport.
// No model, WebGPU, service worker, or PTY writes.

const STT_PROTOCOL_VERSION = 1;
const TARGET_SAMPLE_RATE = 16_000;
// ~2 s of 16 kHz mono PCM16; matches server max_buffered_audio_ms default (2000).
const MAX_BUFFERED_AUDIO_BYTES = 64_000;
/** Client-side queue bound: ~2 s of 20 ms frames. */
const MAX_QUEUED_AUDIO_FRAMES = 100;
/** Fail after sustained inability to drain for this long. */
const BACKPRESSURE_FAIL_MS = 1_500;
/** Server caps one audio frame at 640 PCM bytes; 320 samples of PCM16 = 20 ms. */
const MAX_AUDIO_FRAME_SAMPLES = 320;
const FINALIZATION_TIMEOUT_MS = 5_000;
const DEFAULT_PAUSE_GRACE_PERIOD_MS = 9_000;
const OPEN_READY_STATE = 1;

type SocketListener = (event: Event | MessageEvent) => void;

export interface SpeechTransportSocket {
  readyState: number;
  bufferedAmount: number;
  send(data: string | ArrayBuffer | Uint8Array): void;
  close(): void;
  addEventListener(type: string, listener: SocketListener): void;
  removeEventListener(type: string, listener: SocketListener): void;
}

export interface SpeechAudioCapture {
  stop(): void;
}

export interface SpeechTransportPlatform {
  getUserMedia(): Promise<MediaStream>;
  openSocket(url: string): SpeechTransportSocket;
  createAudioCapture(
    stream: MediaStream,
    onSamples: (samples: Float32Array, inputSampleRate?: number) => void,
  ): SpeechAudioCapture;
  onVisibilityChange?(handler: () => void): () => void;
}

export interface SpeechTransportCallbacks {
  onReady: (config: {
    pauseGracePeriodMs: number;
    finalizationTimeoutMs: number;
  }) => void;
  onPartial: (sequence: number, text: string) => void;
  onFinal: (sequence: number, text: string) => void;
  onSpeechStarted: () => void;
  onSpeechEnded: () => void;
  onError: (message: string) => void;
  onClosed: () => void;
  /** Optional visible warning before hard failure (sustained backpressure). */
  onBackpressureWarning?: (message: string) => void;
}

export interface SpeechTransport {
  start(): Promise<void>;
  stop(): void;
  cancel(): void;
  sessionId(): string | undefined;
}

/** Binary STT audio frame: big-endian u32 sequence + raw PCM16 samples. */
export function encodeSpeechAudioFrame(
  sequence: number,
  pcm: Int16Array,
): ArrayBuffer {
  const buffer = new ArrayBuffer(4 + pcm.byteLength);
  const view = new DataView(buffer);
  view.setUint32(0, sequence >>> 0, false);
  new Int16Array(buffer, 4).set(pcm);
  return buffer;
}

export function newSessionId(): string {
  if (typeof crypto !== "undefined") {
    if (typeof crypto.randomUUID === "function") {
      return crypto.randomUUID();
    }
    // randomUUID requires a secure context, and the cockpit is reachable over
    // plain http on a LAN. getRandomValues has no such requirement.
    if (typeof crypto.getRandomValues === "function") {
      const bytes = crypto.getRandomValues(new Uint8Array(16));
      const hex = Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("");
      return `session-${hex}`;
    }
  }
  return `session-${Date.now().toString(36)}`;
}

function sttSocketUrl(handle: string): string {
  const protocol =
    typeof location !== "undefined" && location.protocol === "https:" ? "wss:" : "ws:";
  const host = typeof location !== "undefined" ? location.host : "localhost";
  return `${protocol}//${host}/api/tasks/${encodeURIComponent(handle)}/stt`;
}

function wrapNativeSocket(socket: WebSocket): SpeechTransportSocket {
  return {
    get readyState() {
      return socket.readyState;
    },
    get bufferedAmount() {
      return socket.bufferedAmount;
    },
    send(data) {
      (socket as unknown as { send(value: string | ArrayBuffer | Uint8Array): void }).send(data);
    },
    close() {
      socket.close();
    },
    addEventListener(type, listener) {
      socket.addEventListener(type, listener as EventListener);
    },
    removeEventListener(type, listener) {
      socket.removeEventListener(type, listener as EventListener);
    },
  };
}

function quantizePcm16(samples: Float32Array): Int16Array {
  const pcm = new Int16Array(samples.length);
  for (let index = 0; index < samples.length; index += 1) {
    const clamped = Math.max(-1, Math.min(1, samples[index] ?? 0));
    pcm[index] = clamped < 0 ? Math.round(clamped * 0x8000) : Math.round(clamped * 0x7fff);
  }
  return pcm;
}

/** Resample mono float samples to 16 kHz PCM16 with bounded linear conversion. */
export function floatSamplesToPcm16(
  samples: Float32Array,
  inputSampleRate: number,
): Int16Array {
  if (samples.length === 0 || inputSampleRate <= 0) {
    return new Int16Array(0);
  }
  if (inputSampleRate === TARGET_SAMPLE_RATE) {
    return quantizePcm16(samples);
  }

  const ratio = inputSampleRate / TARGET_SAMPLE_RATE;
  const outputLength = Math.floor(samples.length / ratio);
  if (outputLength <= 0) {
    return new Int16Array(0);
  }

  const downsampled = new Float32Array(outputLength);
  for (let index = 0; index < outputLength; index += 1) {
    // Center each output sample in its source bin for stable decimation.
    const srcPos = index * ratio + (ratio - 1) / 2;
    const left = Math.max(0, Math.min(samples.length - 1, Math.floor(srcPos)));
    const right = Math.min(samples.length - 1, left + 1);
    const frac = srcPos - left;
    const a = samples[left] ?? 0;
    const b = samples[right] ?? 0;
    downsampled[index] = a + (b - a) * frac;
  }
  return quantizePcm16(downsampled);
}

function createBrowserAudioCapture(
  stream: MediaStream,
  onSamples: (samples: Float32Array, inputSampleRate?: number) => void,
): SpeechAudioCapture {
  const AudioContextCtor =
    window.AudioContext ||
    (window as unknown as { webkitAudioContext?: typeof AudioContext }).webkitAudioContext;
  if (!AudioContextCtor) {
    throw new Error("Web Audio is unavailable in this browser");
  }
  const context = new AudioContextCtor();
  const source = context.createMediaStreamSource(stream);
  // ScriptProcessor remains the broadest iOS Safari fallback for raw PCM taps.
  const processor = context.createScriptProcessor(4096, 1, 1);
  processor.onaudioprocess = (event) => {
    const input = event.inputBuffer.getChannelData(0);
    onSamples(new Float32Array(input), context.sampleRate);
  };
  const muteGain = context.createGain();
  muteGain.gain.value = 0;
  source.connect(processor);
  processor.connect(muteGain);
  muteGain.connect(context.destination);
  void context.resume();
  return {
    stop() {
      processor.onaudioprocess = null;
      try {
        processor.disconnect();
      } catch {
        // already disconnected
      }
      try {
        muteGain.disconnect();
      } catch {
        // already disconnected
      }
      try {
        source.disconnect();
      } catch {
        // already disconnected
      }
      void context.close();
    },
  };
}

export function createBrowserSpeechPlatform(): SpeechTransportPlatform {
  return {
    async getUserMedia() {
      if (!navigator.mediaDevices?.getUserMedia) {
        throw new Error("Microphone capture is unavailable in this browser");
      }
      return navigator.mediaDevices.getUserMedia({
        audio: {
          channelCount: 1,
          echoCancellation: true,
          noiseSuppression: true,
        },
        video: false,
      });
    },
    openSocket(url) {
      return wrapNativeSocket(new WebSocket(url));
    },
    createAudioCapture: createBrowserAudioCapture,
    onVisibilityChange(handler) {
      const listener = () => {
        if (typeof document !== "undefined" && document.visibilityState === "hidden") {
          handler();
        }
      };
      document.addEventListener("visibilitychange", listener);
      return () => document.removeEventListener("visibilitychange", listener);
    },
  };
}

export type CreateSpeechTransportOptions = {
  /** Shared with the TaskTerminal speech reducer when provided. */
  sessionId?: string;
};

export function createSpeechTransport(
  handle: string,
  callbacks: SpeechTransportCallbacks,
  platform: SpeechTransportPlatform = createBrowserSpeechPlatform(),
  options: CreateSpeechTransportOptions = {},
): SpeechTransport {
  let activeSessionId: string | undefined;
  const injectedSessionId = options.sessionId;
  let startPromise: Promise<void> | undefined;
  let socket: SpeechTransportSocket | undefined;
  let mediaStream: MediaStream | undefined;
  let capture: SpeechAudioCapture | undefined;
  let unsubscribeVisibility: (() => void) | undefined;
  let finalizationTimer: ReturnType<typeof setTimeout> | undefined;
  let nextSequence = 0;
  let releasing = false;

  let messageListener: SocketListener | undefined;
  let errorListener: SocketListener | undefined;
  let closeListener: SocketListener | undefined;
  let finalizationComplete = false;
  let sessionFinalizationTimeoutMs = FINALIZATION_TIMEOUT_MS;
  let pendingAudioFrames: ArrayBuffer[] = [];
  let backpressureSinceMs: number | undefined;
  let backpressureWarned = false;
  let substantialAudioLoss = false;

  function clearFinalizationTimer() {
    if (finalizationTimer !== undefined) {
      clearTimeout(finalizationTimer);
      finalizationTimer = undefined;
    }
  }

  function clearAudioQueue() {
    pendingAudioFrames = [];
    backpressureSinceMs = undefined;
    backpressureWarned = false;
  }

  /** Shared cleanup for cancel, finalize-complete, provider error, and visibility. */
  function teardown(options: {
    closeSocket: boolean;
    invalidateSession: boolean;
    notifyClosed: boolean;
  }) {
    clearFinalizationTimer();
    clearAudioQueue();
    releaseCapture();
    detachSocketListeners();
    if (options.closeSocket && socket) {
      try {
        socket.close();
      } catch {
        // ignore close races
      }
    }
    if (options.closeSocket) {
      socket = undefined;
    }
    startPromise = undefined;
    if (options.invalidateSession) {
      activeSessionId = undefined;
    }
    if (options.notifyClosed) {
      callbacks.onClosed();
    }
  }

  function completeFinalization() {
    if (finalizationComplete) return;
    if (substantialAudioLoss) {
      fail("Speech audio was delayed under backpressure; transcript may be incomplete");
      return;
    }
    finalizationComplete = true;
    teardown({ closeSocket: true, invalidateSession: true, notifyClosed: true });
  }

  function detachSocketListeners() {
    if (!socket) return;
    if (messageListener) socket.removeEventListener("message", messageListener);
    if (errorListener) socket.removeEventListener("error", errorListener);
    if (closeListener) socket.removeEventListener("close", closeListener);
    messageListener = undefined;
    errorListener = undefined;
    closeListener = undefined;
  }

  function releaseCapture() {
    unsubscribeVisibility?.();
    unsubscribeVisibility = undefined;
    capture?.stop();
    capture = undefined;
    if (mediaStream) {
      for (const track of mediaStream.getTracks()) {
        track.stop();
      }
      mediaStream = undefined;
    }
  }

  function releaseResources(closeSocket: boolean) {
    if (releasing) return;
    releasing = true;
    teardown({
      closeSocket,
      invalidateSession: false,
      notifyClosed: false,
    });
    releasing = false;
  }

  function fail(message: string) {
    if (finalizationComplete && !activeSessionId) {
      return;
    }
    callbacks.onError(message);
    finalizationComplete = true;
    teardown({ closeSocket: true, invalidateSession: true, notifyClosed: true });
  }

  function handleServerMessage(event: Event | MessageEvent) {
    const messageEvent = event as MessageEvent;
    const raw = messageEvent.data;
    if (typeof raw !== "string") return;
    let payload: {
      type?: string;
      version?: number;
      sessionId?: string;
      pauseGracePeriodMs?: unknown;
      finalizationTimeoutMs?: unknown;
      sequence?: number;
      text?: string;
      message?: string;
      code?: string;
    };
    try {
      payload = JSON.parse(raw) as typeof payload;
    } catch {
      return;
    }
    if (payload.version !== STT_PROTOCOL_VERSION) return;
    if (!activeSessionId || payload.sessionId !== activeSessionId) return;
    switch (payload.type) {
      case "stt.ready": {
        const pauseGracePeriodMs =
          typeof payload.pauseGracePeriodMs === "number"
            ? payload.pauseGracePeriodMs
            : DEFAULT_PAUSE_GRACE_PERIOD_MS;
        sessionFinalizationTimeoutMs =
          typeof payload.finalizationTimeoutMs === "number"
            ? payload.finalizationTimeoutMs
            : FINALIZATION_TIMEOUT_MS;
        callbacks.onReady({ pauseGracePeriodMs, finalizationTimeoutMs: sessionFinalizationTimeoutMs });
        break;
      }
      case "stt.partial":
        if (typeof payload.sequence === "number" && typeof payload.text === "string") {
          callbacks.onPartial(payload.sequence, payload.text);
        }
        break;
      case "stt.final":
        if (typeof payload.sequence === "number" && typeof payload.text === "string") {
          callbacks.onFinal(payload.sequence, payload.text);
        }
        break;
      case "stt.speech_started":
        callbacks.onSpeechStarted();
        break;
      case "stt.speech_ended":
        callbacks.onSpeechEnded();
        break;
      case "stt.error":
        fail(payload.message ?? payload.code ?? "STT provider error");
        break;
      case "stt.closed":
        completeFinalization();
        break;
      default:
        break;
    }
  }

  function flushPendingAudio() {
    if (!socket || socket.readyState !== OPEN_READY_STATE) return;
    while (
      pendingAudioFrames.length > 0 &&
      socket.bufferedAmount <= MAX_BUFFERED_AUDIO_BYTES
    ) {
      const frame = pendingAudioFrames.shift();
      if (!frame) break;
      try {
        socket.send(frame);
      } catch {
        fail("Failed to send speech audio frame");
        return;
      }
    }
    if (pendingAudioFrames.length === 0) {
      backpressureSinceMs = undefined;
      backpressureWarned = false;
    }
  }

  function enqueueOrSendFrame(frame: ArrayBuffer) {
    if (!socket || socket.readyState !== OPEN_READY_STATE) return;
    flushPendingAudio();
    if (
      pendingAudioFrames.length === 0 &&
      socket.bufferedAmount <= MAX_BUFFERED_AUDIO_BYTES
    ) {
      try {
        socket.send(frame);
      } catch {
        fail("Failed to send speech audio frame");
      }
      return;
    }

    pendingAudioFrames.push(frame);
    const now =
      typeof performance !== "undefined" ? performance.now() : Date.now();
    if (backpressureSinceMs === undefined) {
      backpressureSinceMs = now;
    }
    if (
      !backpressureWarned &&
      now - backpressureSinceMs >= BACKPRESSURE_FAIL_MS / 2
    ) {
      backpressureWarned = true;
      callbacks.onBackpressureWarning?.(
        "Speech audio is delayed; speak slower or wait for the network",
      );
    }
    if (pendingAudioFrames.length > MAX_QUEUED_AUDIO_FRAMES) {
      substantialAudioLoss = true;
      console.warn(
        JSON.stringify({
          type: "stt.audio_backpressure",
          queuedFrames: pendingAudioFrames.length,
          dropped: true,
        }),
      );
      fail(
        "Speech audio backpressure: transcription integrity cannot be guaranteed",
      );
      return;
    }
    if (now - (backpressureSinceMs ?? now) >= BACKPRESSURE_FAIL_MS) {
      substantialAudioLoss = true;
      fail(
        "Speech audio backpressure: transcription integrity cannot be guaranteed",
      );
    }
  }

  function onSamples(samples: Float32Array, inputSampleRate = TARGET_SAMPLE_RATE) {
    if (!socket || socket.readyState !== OPEN_READY_STATE) return;
    const pcm = floatSamplesToPcm16(samples, inputSampleRate);
    if (pcm.length === 0) return;
    // The server rejects any frame carrying more than MAX_AUDIO_FRAME_SAMPLES of
    // PCM. One capture callback resamples to far more than that (a 4096-sample
    // buffer at 48 kHz yields 1365 samples), so split it into wire-sized frames.
    for (let offset = 0; offset < pcm.length; offset += MAX_AUDIO_FRAME_SAMPLES) {
      const chunk = pcm.subarray(offset, offset + MAX_AUDIO_FRAME_SAMPLES);
      const frame = encodeSpeechAudioFrame(nextSequence, chunk);
      enqueueOrSendFrame(frame);
      if (!activeSessionId) return;
      nextSequence = (nextSequence + 1) >>> 0;
    }
  }

  function waitForSocketOpen(target: SpeechTransportSocket): Promise<void> {
    if (target.readyState === OPEN_READY_STATE) {
      return Promise.resolve();
    }
    return new Promise((resolve, reject) => {
      const onOpen = () => {
        cleanup();
        resolve();
      };
      const onError = () => {
        cleanup();
        reject(new Error("Speech WebSocket failed to open"));
      };
      const onClose = () => {
        cleanup();
        reject(new Error("Speech WebSocket closed before open"));
      };
      const cleanup = () => {
        target.removeEventListener("open", onOpen);
        target.removeEventListener("error", onError);
        target.removeEventListener("close", onClose);
      };
      target.addEventListener("open", onOpen);
      target.addEventListener("error", onError);
      target.addEventListener("close", onClose);
    });
  }

  function sendControl(type: "stt.start" | "stt.stop" | "stt.cancel") {
    if (!socket || !activeSessionId || socket.readyState !== OPEN_READY_STATE) return;
    if (type === "stt.start") {
      socket.send(
        JSON.stringify({
          type: "stt.start",
          version: STT_PROTOCOL_VERSION,
          sessionId: activeSessionId,
          encoding: "pcm16",
          sampleRate: TARGET_SAMPLE_RATE,
          channels: 1,
        }),
      );
      return;
    }
    socket.send(
      JSON.stringify({
        type,
        version: STT_PROTOCOL_VERSION,
        sessionId: activeSessionId,
      }),
    );
  }

  function start(): Promise<void> {
    if (startPromise) return startPromise;

    startPromise = (async () => {
      activeSessionId = injectedSessionId ?? newSessionId();
      nextSequence = 0;
      finalizationComplete = false;
      substantialAudioLoss = false;
      clearAudioQueue();
      sessionFinalizationTimeoutMs = FINALIZATION_TIMEOUT_MS;

      try {
        mediaStream = await platform.getUserMedia();
      } catch (error) {
        const message =
          error instanceof Error ? error.message : "Microphone permission denied";
        fail(
          message.includes("permission") || message.includes("NotAllowed")
            ? "Microphone permission denied"
            : message,
        );
        throw error instanceof Error ? error : new Error(message);
      }

      socket = platform.openSocket(sttSocketUrl(handle));
      messageListener = handleServerMessage;
      errorListener = () => fail("Speech WebSocket error");
      closeListener = () => {
        releaseResources(false);
        callbacks.onClosed();
      };
      socket.addEventListener("message", messageListener);
      socket.addEventListener("error", errorListener);
      socket.addEventListener("close", closeListener);

      try {
        await waitForSocketOpen(socket);
      } catch (error) {
        fail(error instanceof Error ? error.message : "Speech WebSocket failed to open");
        throw error;
      }

      sendControl("stt.start");
      try {
        capture = platform.createAudioCapture(mediaStream, onSamples);
      } catch (error) {
        const message =
          error instanceof Error ? error.message : "Audio capture setup failed";
        fail(message);
        throw error instanceof Error ? error : new Error(message);
      }
      unsubscribeVisibility = platform.onVisibilityChange?.(() => {
        fail("Capture interrupted by background/visibility change");
      });
    })().catch((error) => {
      startPromise = undefined;
      throw error;
    });

    return startPromise;
  }

  function stop() {
    if (!socket && !capture && !mediaStream) return;
    sendControl("stt.stop");
    releaseCapture();
    startPromise = undefined;
    finalizationComplete = false;
    clearFinalizationTimer();
    finalizationTimer = setTimeout(() => {
      completeFinalization();
    }, sessionFinalizationTimeoutMs);
  }

  function cancel() {
    const hadSession = activeSessionId !== undefined || socket !== undefined;
    if (socket && activeSessionId && socket.readyState === OPEN_READY_STATE) {
      try {
        sendControl("stt.cancel");
      } catch {
        // ignore
      }
    }
    finalizationComplete = true;
    teardown({ closeSocket: true, invalidateSession: true, notifyClosed: hadSession });
  }

  return {
    start,
    stop,
    cancel,
    sessionId: () => activeSessionId,
  };
}
