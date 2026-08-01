// Browser-native continuous speech capture + authenticated STT WebSocket transport.
// No model, WebGPU, service worker, or PTY writes.

const STT_PROTOCOL_VERSION = 1;
const TARGET_SAMPLE_RATE = 16_000;
// ~2 s of 16 kHz mono PCM16; matches server max_buffered_audio_ms default (2000).
const MAX_BUFFERED_AUDIO_BYTES = 64_000;
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
  onReady: (config: { pauseGracePeriodMs: number }) => void;
  onPartial: (sequence: number, text: string) => void;
  onFinal: (sequence: number, text: string) => void;
  onSpeechStarted: () => void;
  onSpeechEnded: () => void;
  onError: (message: string) => void;
  onClosed: () => void;
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

function newSessionId(): string {
  if (typeof crypto !== "undefined" && typeof crypto.randomUUID === "function") {
    return crypto.randomUUID();
  }
  return `session-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 10)}`;
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
  source.connect(processor);
  processor.connect(context.destination);
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

  function clearFinalizationTimer() {
    if (finalizationTimer !== undefined) {
      clearTimeout(finalizationTimer);
      finalizationTimer = undefined;
    }
  }

  function completeFinalization() {
    if (finalizationComplete) return;
    finalizationComplete = true;
    clearFinalizationTimer();
    detachSocketListeners();
    if (socket) {
      try {
        socket.close();
      } catch {
        // ignore close races
      }
      socket = undefined;
    }
    activeSessionId = undefined;
    callbacks.onClosed();
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
    clearFinalizationTimer();
    releaseCapture();
    detachSocketListeners();
    if (closeSocket && socket) {
      try {
        socket.close();
      } catch {
        // ignore close races
      }
    }
    socket = undefined;
    startPromise = undefined;
    releasing = false;
  }

  function fail(message: string) {
    callbacks.onError(message);
    releaseResources(true);
    activeSessionId = undefined;
    callbacks.onClosed();
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
        callbacks.onReady({ pauseGracePeriodMs });
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
        callbacks.onError(payload.message ?? payload.code ?? "STT provider error");
        break;
      case "stt.closed":
        completeFinalization();
        break;
      default:
        break;
    }
  }

  function onSamples(samples: Float32Array, inputSampleRate = TARGET_SAMPLE_RATE) {
    if (!socket || socket.readyState !== OPEN_READY_STATE) return;
    if (socket.bufferedAmount > MAX_BUFFERED_AUDIO_BYTES) return;
    const pcm = floatSamplesToPcm16(samples, inputSampleRate);
    if (pcm.length === 0) return;
    const frame = encodeSpeechAudioFrame(nextSequence, pcm);
    try {
      socket.send(frame);
    } catch {
      fail("Failed to send speech audio frame");
      return;
    }
    nextSequence = (nextSequence + 1) >>> 0;
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
    }, FINALIZATION_TIMEOUT_MS);
  }

  function cancel() {
    clearFinalizationTimer();
    sendControl("stt.cancel");
    const hadSession = activeSessionId !== undefined || socket !== undefined;
    releaseResources(true);
    activeSessionId = undefined;
    if (hadSession) callbacks.onClosed();
  }

  return {
    start,
    stop,
    cancel,
    sessionId: () => activeSessionId,
  };
}
