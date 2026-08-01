import { describe, expect, it, vi } from "vitest";
import {
  createSpeechTransport,
  encodeSpeechAudioFrame,
  floatSamplesToPcm16,
  type SpeechTransportCallbacks,
  type SpeechTransportPlatform,
  type SpeechTransportSocket,
} from "./speechTransport";

type Listener = (event: Event | MessageEvent) => void;

function fakeSocket(): SpeechTransportSocket & {
  sent: Array<string | ArrayBuffer | Uint8Array>;
  emit(type: string, event?: Event | MessageEvent): void;
} {
  const listeners = new Map<string, Set<Listener>>();
  const socket = {
    readyState: 0,
    bufferedAmount: 0,
    sent: [] as Array<string | ArrayBuffer | Uint8Array>,
    send(data: string | ArrayBuffer | Uint8Array) {
      this.sent.push(data);
    },
    close: vi.fn(),
    addEventListener(type: string, listener: Listener) {
      const set = listeners.get(type) ?? new Set<Listener>();
      set.add(listener);
      listeners.set(type, set);
    },
    removeEventListener(type: string, listener: Listener) {
      listeners.get(type)?.delete(listener);
    },
    emit(type: string, event: Event | MessageEvent = new Event(type)) {
      for (const listener of listeners.get(type) ?? []) listener(event);
    },
  };
  return socket;
}

function platformFor(socket: ReturnType<typeof fakeSocket>) {
  const track = { stop: vi.fn() };
  const stream = { getTracks: () => [track] } as unknown as MediaStream;
  const capture = {
    stop: vi.fn(),
    emit: (_samples: Float32Array) => {},
  };
  const platform: SpeechTransportPlatform = {
    getUserMedia: vi.fn(async () => stream),
    openSocket: vi.fn(() => socket),
    createAudioCapture: vi.fn((_stream, onSamples) => {
      capture.emit = onSamples;
      return capture;
    }),
    onVisibilityChange: vi.fn(() => () => {}),
  };
  return { platform, track, stream, capture };
}

function callbacks(): SpeechTransportCallbacks {
  return {
    onReady: vi.fn(),
    onPartial: vi.fn(),
    onFinal: vi.fn(),
    onSpeechStarted: vi.fn(),
    onSpeechEnded: vi.fn(),
    onError: vi.fn(),
    onClosed: vi.fn(),
  };
}

function readFrameSequence(frame: string | ArrayBuffer | Uint8Array): number {
  const bytes =
    typeof frame === "string"
      ? new TextEncoder().encode(frame)
      : frame instanceof Uint8Array
        ? frame
        : new Uint8Array(frame);
  return new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength).getUint32(0, false);
}

function audioFrameCount(sent: Array<string | ArrayBuffer | Uint8Array>): number {
  return sent.filter((item) => typeof item !== "string").length;
}

describe("speech transport", () => {
  it("frames raw PCM with a sequence prefix and prevents duplicate starts", async () => {
    const socket = fakeSocket();
    const { platform } = platformFor(socket);
    const events = callbacks();
    const transport = createSpeechTransport("web/fix-login", events, platform);

    const first = transport.start();
    const second = transport.start();
    expect(first).toBe(second);
    expect(platform.getUserMedia).toHaveBeenCalledTimes(1);

    socket.readyState = 1;
    socket.emit("open");
    await first;

    expect(socket.sent[0]).toContain('"type":"stt.start"');
    expect(socket.sent[0]).not.toContain('"language"');
    const frame = encodeSpeechAudioFrame(42, new Int16Array([1, -2]));
    expect(new Uint8Array(frame).slice(0, 4)).toEqual(new Uint8Array([0, 0, 0, 42]));
    expect(new Uint8Array(frame).byteLength).toBe(8);
  });

  it("forwards provider events and ignores stale session messages", async () => {
    const socket = fakeSocket();
    const { platform } = platformFor(socket);
    const events = callbacks();
    const transport = createSpeechTransport("web/fix-login", events, platform);
    const started = transport.start();
    socket.readyState = 1;
    socket.emit("open");
    await started;

    socket.emit(
      "message",
      new MessageEvent("message", {
        data: JSON.stringify({
          version: 1,
          type: "stt.partial",
          sessionId: "wrong-session",
          sequence: 1,
          text: "stale",
        }),
      }),
    );
    socket.emit(
      "message",
      new MessageEvent("message", {
        data: JSON.stringify({
          version: 1,
          type: "stt.partial",
          sessionId: transport.sessionId(),
          sequence: 1,
          text: "ask cursor",
        }),
      }),
    );

    expect(events.onPartial).toHaveBeenCalledWith(1, "ask cursor");
    expect(events.onPartial).not.toHaveBeenCalledWith(1, "stale");
  });

  it("fires onSpeechStarted only from server stt.speech_started, not loud local samples", async () => {
    const socket = fakeSocket();
    const { platform, capture } = platformFor(socket);
    const events = callbacks();
    const transport = createSpeechTransport("web/fix-login", events, platform);
    const started = transport.start();
    socket.readyState = 1;
    socket.emit("open");
    await started;

    capture.emit(new Float32Array([0.3, 0.3, 0.3, 0.3]));
    expect(events.onSpeechStarted).not.toHaveBeenCalled();

    socket.emit(
      "message",
      new MessageEvent("message", {
        data: JSON.stringify({
          version: 1,
          type: "stt.speech_started",
          sessionId: transport.sessionId(),
        }),
      }),
    );
    expect(events.onSpeechStarted).toHaveBeenCalledTimes(1);

    socket.emit(
      "message",
      new MessageEvent("message", {
        data: JSON.stringify({
          version: 1,
          type: "stt.speech_ended",
          sessionId: transport.sessionId(),
        }),
      }),
    );
    expect(events.onSpeechEnded).toHaveBeenCalledTimes(1);
  });

  it("passes pauseGracePeriodMs from stt.ready to onReady", async () => {
    const socket = fakeSocket();
    const { platform } = platformFor(socket);
    const events = callbacks();
    const transport = createSpeechTransport("web/fix-login", events, platform);
    const started = transport.start();
    socket.readyState = 1;
    socket.emit("open");
    await started;

    socket.emit(
      "message",
      new MessageEvent("message", {
        data: JSON.stringify({
          version: 1,
          type: "stt.ready",
          sessionId: transport.sessionId(),
          pauseGracePeriodMs: 4000,
        }),
      }),
    );

    expect(events.onReady).toHaveBeenCalledWith({ pauseGracePeriodMs: 4000 });
  });

  it("falls back to 9000 when stt.ready omits pauseGracePeriodMs", async () => {
    const socket = fakeSocket();
    const { platform } = platformFor(socket);
    const events = callbacks();
    const transport = createSpeechTransport("web/fix-login", events, platform);
    const started = transport.start();
    socket.readyState = 1;
    socket.emit("open");
    await started;

    socket.emit(
      "message",
      new MessageEvent("message", {
        data: JSON.stringify({
          version: 1,
          type: "stt.ready",
          sessionId: transport.sessionId(),
        }),
      }),
    );

    expect(events.onReady).toHaveBeenCalledWith({ pauseGracePeriodMs: 9000 });
  });

  it("releases audio resources when visibility interrupts capture", async () => {
    const socket = fakeSocket();
    const { platform, track, capture } = platformFor(socket);
    const events = callbacks();
    let visibilityHandler = () => {};
    vi.mocked(platform.onVisibilityChange!).mockImplementation((handler) => {
      visibilityHandler = handler;
      return () => {};
    });
    const transport = createSpeechTransport("web/fix-login", events, platform);
    const started = transport.start();
    socket.readyState = 1;
    socket.emit("open");
    await started;

    visibilityHandler();

    expect(events.onError).toHaveBeenCalledWith(expect.stringContaining("background"));
    expect(capture.stop).toHaveBeenCalled();
    expect(track.stop).toHaveBeenCalled();
    expect(socket.close).toHaveBeenCalled();
  });

  it("releases the microphone when audio capture setup fails", async () => {
    const socket = fakeSocket();
    const { platform, track } = platformFor(socket);
    const events = callbacks();
    vi.mocked(platform.createAudioCapture).mockImplementation(() => {
      throw new Error("Web Audio is unavailable in this browser");
    });
    const transport = createSpeechTransport("web/fix-login", events, platform);
    const started = transport.start();
    socket.readyState = 1;
    socket.emit("open");

    await expect(started).rejects.toThrow("Web Audio is unavailable");
    expect(events.onError).toHaveBeenCalledWith("Web Audio is unavailable in this browser");
    expect(track.stop).toHaveBeenCalled();
    expect(socket.close).toHaveBeenCalled();
  });

  it("keeps the STT socket open during finalization, then closes on timeout", async () => {
    vi.useFakeTimers();
    try {
      const socket = fakeSocket();
      const { platform, capture } = platformFor(socket);
      const events = callbacks();
      const transport = createSpeechTransport("web/fix-login", events, platform);
      const started = transport.start();
      socket.readyState = 1;
      socket.emit("open");
      await started;

      transport.stop();

      expect(capture.stop).toHaveBeenCalled();
      expect(socket.close).not.toHaveBeenCalled();
      vi.advanceTimersByTime(5_001);
      expect(socket.close).toHaveBeenCalled();
    } finally {
      vi.useRealTimers();
    }
  });

  it("resolves stop immediately on stt.closed without waiting for the fallback timer", async () => {
    vi.useFakeTimers();
    try {
      const socket = fakeSocket();
      const { platform } = platformFor(socket);
      const events = callbacks();
      const transport = createSpeechTransport("web/fix-login", events, platform);
      const started = transport.start();
      socket.readyState = 1;
      socket.emit("open");
      await started;

      transport.stop();
      expect(events.onClosed).not.toHaveBeenCalled();

      const sessionId = transport.sessionId();
      socket.emit(
        "message",
        new MessageEvent("message", {
          data: JSON.stringify({
            version: 1,
            type: "stt.closed",
            sessionId,
          }),
        }),
      );

      expect(events.onClosed).toHaveBeenCalledTimes(1);
      expect(socket.close).toHaveBeenCalled();
      vi.advanceTimersByTime(5_001);
      expect(events.onClosed).toHaveBeenCalledTimes(1);
    } finally {
      vi.useRealTimers();
    }
  });

  it("fires onClosed exactly once when both stt.closed and the fallback timer would fire", async () => {
    vi.useFakeTimers();
    try {
      const socket = fakeSocket();
      const { platform } = platformFor(socket);
      const events = callbacks();
      const transport = createSpeechTransport("web/fix-login", events, platform);
      const started = transport.start();
      socket.readyState = 1;
      socket.emit("open");
      await started;

      const sessionId = transport.sessionId();
      transport.stop();
      socket.emit(
        "message",
        new MessageEvent("message", {
          data: JSON.stringify({
            version: 1,
            type: "stt.closed",
            sessionId,
          }),
        }),
      );
      vi.advanceTimersByTime(5_001);

      expect(events.onClosed).toHaveBeenCalledTimes(1);
      expect(socket.close).toHaveBeenCalledTimes(1);
    } finally {
      vi.useRealTimers();
    }
  });

  it("downsamples native audio before creating 16 kHz PCM", () => {
    const pcm = floatSamplesToPcm16(new Float32Array([0, 1, 0, -1]), 32_000);

    expect(pcm.length).toBe(2);
    expect(pcm[0]).toBeGreaterThan(0);
    expect(pcm[1]).toBeLessThan(0);
  });

  it("drops audio frames while the socket send buffer is over budget and resumes when it clears", async () => {
    const socket = fakeSocket();
    const { platform, capture } = platformFor(socket);
    const events = callbacks();
    const transport = createSpeechTransport("web/fix-login", events, platform);
    const started = transport.start();
    socket.readyState = 1;
    socket.emit("open");
    await started;

    const controlCount = socket.sent.length;
    socket.bufferedAmount = 64_001;
    capture.emit(new Float32Array([0.3, 0.3, 0.3, 0.3]));
    expect(audioFrameCount(socket.sent)).toBe(0);

    socket.bufferedAmount = 64_000;
    capture.emit(new Float32Array([0.3, 0.3, 0.3, 0.3]));
    expect(audioFrameCount(socket.sent)).toBe(1);
    expect(socket.sent.length).toBeGreaterThan(controlCount);
    expect(events.onError).not.toHaveBeenCalled();
    expect(socket.close).not.toHaveBeenCalled();
  });

  it("does not advance the audio sequence when a frame is dropped for backpressure", async () => {
    const socket = fakeSocket();
    const { platform, capture } = platformFor(socket);
    const events = callbacks();
    const transport = createSpeechTransport("web/fix-login", events, platform);
    const started = transport.start();
    socket.readyState = 1;
    socket.emit("open");
    await started;

    capture.emit(new Float32Array([0.3, 0.3, 0.3, 0.3]));
    const firstFrame = socket.sent.find((item) => typeof item !== "string");
    expect(firstFrame).toBeDefined();
    expect(readFrameSequence(firstFrame!)).toBe(0);

    socket.bufferedAmount = 64_001;
    capture.emit(new Float32Array([0.3, 0.3, 0.3, 0.3]));
    expect(audioFrameCount(socket.sent)).toBe(1);

    socket.bufferedAmount = 0;
    capture.emit(new Float32Array([0.3, 0.3, 0.3, 0.3]));
    const secondFrame = socket.sent.filter((item) => typeof item !== "string")[1];
    expect(secondFrame).toBeDefined();
    expect(readFrameSequence(secondFrame!)).toBe(1);
    expect(events.onError).not.toHaveBeenCalled();
  });
});
