/**
 * Unmocked Ghostty/wterm contract tests.
 *
 * These load the real @wterm/ghostty WASM from node_modules. They exist because
 * mocked unit tests previously shipped constructor/init bugs that only failed
 * on device (yellow Surface V2 banner).
 *
 * The "wterm behavioral contract" block drives the real core + DOM renderer
 * through the TERMINAL.md iPhone bake-off scenarios. Where Ghostty needed
 * Ajax-side modules (scroll-follow, snap-on-type, VT response pump, app-cursor
 * arrows, bracketed paste), wterm implements them natively — these tests pin
 * that native behavior so Surface V2 parity rests on it.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { WTerm } from "@wterm/dom";
import {
  loadWtermGhosttyCore,
  smokeInitWtermGhosttyCore,
} from "./terminalWtermGhosttyCore";
import { WTERM_GHOSTTY_WASM_URL } from "./terminalWtermWasm";

const repoRoot = join(import.meta.dirname, "../../../..");
const wtermWasmPath = join(repoRoot, "node_modules/@wterm/ghostty/wasm/ghostty-vt.wasm");
const ghosttyWebWasmPath = join(repoRoot, "node_modules/ghostty-web/ghostty-vt.wasm");

function wasmArrayBuffer(path: string): ArrayBuffer {
  const buf = readFileSync(path);
  return buf.buffer.slice(buf.byteOffset, buf.byteOffset + buf.byteLength);
}

function stubWtermWasmFetch() {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      expect(url).toContain(WTERM_GHOSTTY_WASM_URL);
      return {
        ok: true,
        status: 200,
        arrayBuffer: async () => wasmArrayBuffer(wtermWasmPath),
      };
    }),
  );
  vi.stubGlobal(
    "ResizeObserver",
    class {
      observe() {}
      disconnect() {}
      unobserve() {}
    },
  );
}

/** WTerm renders via setTimeout(0) + requestAnimationFrame. */
async function renderFlush() {
  await new Promise<void>((resolve) => setTimeout(resolve, 0));
  await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
}

describe("terminalWtermGhosttyCore integration (real WASM)", () => {
  beforeEach(() => {
    stubWtermWasmFetch();
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    document.body.innerHTML = "";
  });

  it("constructs GhosttyCore that can init and write cells", async () => {
    const core = await loadWtermGhosttyCore();
    expect(() => smokeInitWtermGhosttyCore(core)).not.toThrow();
    expect(core.getCols()).toBe(40);
    expect(core.getRows()).toBe(10);
    expect(core.getCell(0, 0).char).toBe("A".charCodeAt(0));
  });

  it("rejects the ghostty-web binary even if served at the wterm URL", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => ({
        ok: true,
        status: 200,
        arrayBuffer: async () => wasmArrayBuffer(ghosttyWebWasmPath),
      })),
    );
    await expect(loadWtermGhosttyCore()).rejects.toThrow(/missing init/);
  });

  it("WTerm.init + write paints text with the loaded core", async () => {
    const core = await loadWtermGhosttyCore();
    const host = document.createElement("div");
    host.style.width = "320px";
    host.style.height = "170px";
    // wterm measures char size from a probe in the host; give non-zero boxes.
    Object.defineProperty(host, "clientWidth", { configurable: true, value: 320 });
    Object.defineProperty(host, "clientHeight", { configurable: true, value: 170 });
    document.body.appendChild(host);

    const term = new WTerm(host, {
      core,
      cols: 40,
      rows: 10,
      autoResize: false,
    });
    await term.init();
    term.write("Hello wterm\r\n");

    await renderFlush();

    const text = host.textContent ?? "";
    expect(text).toContain("Hello wterm");
    term.destroy();
  });
});

describe("wterm behavioral contract (real WASM, bake-off scenarios)", () => {
  let term: WTerm | undefined;

  beforeEach(() => {
    stubWtermWasmFetch();
  });

  afterEach(() => {
    term?.destroy();
    term = undefined;
    vi.unstubAllGlobals();
    document.body.innerHTML = "";
  });

  const mountRealWterm = async (onData?: (data: string) => void) => {
    const core = await loadWtermGhosttyCore();
    const host = document.createElement("div");
    Object.defineProperty(host, "clientWidth", { configurable: true, value: 320 });
    Object.defineProperty(host, "clientHeight", { configurable: true, value: 170 });
    document.body.appendChild(host);
    term = new WTerm(host, { core, cols: 40, rows: 10, autoResize: false, onData });
    await term.init();
    const textarea = host.querySelector("textarea")!;
    return { core, host, term, textarea };
  };

  it("echoes typed text and renders a backspace rubout (bake-off: type + backspace)", async () => {
    const { host, term } = await mountRealWterm();
    term.write("abc\b \b");
    await renderFlush();

    const text = host.textContent ?? "";
    expect(text).toContain("ab");
    expect(text).not.toContain("abc");
  });

  it("enters and exits the alternate screen, restoring primary content (bake-off: alt-screen program)", async () => {
    const { core, host, term } = await mountRealWterm();
    term.write("primary-content\r\n");
    await renderFlush();

    term.write("\x1b[?1049h\x1b[2J\x1b[HALT-SCREEN");
    await renderFlush();
    expect(core.usingAltScreen()).toBe(true);
    expect(host.textContent).toContain("ALT-SCREEN");

    term.write("\x1b[?1049l");
    await renderFlush();
    expect(core.usingAltScreen()).toBe(false);
    expect(host.textContent).toContain("primary-content");
    expect(host.textContent).not.toContain("ALT-SCREEN");
  });

  it("reflows on resize and keeps content (bake-off: rotate the phone)", async () => {
    const { core, host, term } = await mountRealWterm();
    term.write("0123456789");
    await renderFlush();

    term.resize(20, 10);
    await renderFlush();

    expect(core.getCols()).toBe(20);
    expect(core.getRows()).toBe(10);
    expect(host.textContent).toContain("0123456789");
  });

  it("renders multibyte UTF-8 output", async () => {
    const { host, term } = await mountRealWterm();
    term.write("héllo 世界\r\n");
    await renderFlush();
    expect(host.textContent).toContain("héllo");
    // Wide glyphs occupy two cells; the renderer emits a spacer cell after
    // each, so the row reads "世 界" — assert per-glyph.
    expect(host.textContent).toContain("世");
    expect(host.textContent).toContain("界");
  });

  it("accumulates scrollback past the viewport and flags it on the host", async () => {
    const { core, host, term } = await mountRealWterm();
    for (let i = 0; i < 15; i++) term.write(`line${i}\r\n`);
    await renderFlush();

    expect(core.getScrollbackCount()).toBeGreaterThan(0);
    expect(host.classList.contains("has-scrollback")).toBe(true);
  });

  it("does not yank the view down when output arrives while scrolled up (bake-off: scroll during output)", async () => {
    const { host, term } = await mountRealWterm();
    for (let i = 0; i < 15; i++) term.write(`line${i}\r\n`);
    await renderFlush();
    Object.defineProperty(host, "scrollHeight", { configurable: true, value: 340 });
    host.scrollTop = 0; // parked in scrollback, 170px above the bottom

    term.write("new-output\r\n");
    await renderFlush();

    expect(host.scrollTop).toBe(0);
  });

  it("keeps following the bottom when output arrives while pinned", async () => {
    const { host, term } = await mountRealWterm();
    Object.defineProperty(host, "scrollHeight", { configurable: true, value: 340 });
    host.scrollTop = 169; // within the 5px pin threshold of the 170px max scroll

    term.write("more\r\n");
    await renderFlush();

    // Re-snapped to the row-aligned bottom, not left mid-cell.
    expect(host.scrollTop).toBe(170);
  });

  it("snaps to the live output when the user types while scrolled up (bake-off: type after scrolling)", async () => {
    const data: string[] = [];
    const { host, textarea } = await mountRealWterm((d) => data.push(d));
    Object.defineProperty(host, "scrollHeight", { configurable: true, value: 340 });
    host.scrollTop = 0;

    textarea.dispatchEvent(
      new KeyboardEvent("keydown", { key: "a", cancelable: true, bubbles: true }),
    );

    expect(data).toContain("a");
    expect(host.scrollTop).toBe(170);
  });

  it("KNOWN UPSTREAM GAP: does not answer device queries (DSR/DA) that vim/tmux probe with", async () => {
    // ghostty-web answered \x1b[6n with a cursor-position report through
    // onData. @wterm/ghostty 0.3.0's read_response never yields one (probed
    // directly: DSR and DA both return null), so full-screen apps waiting on
    // a report can stall. WTerm's render loop DOES pump getResponse() into
    // onData, so this starts working — and this canary flips red — the moment
    // an upstream upgrade implements responses. Then delete this test and
    // assert the round-trip instead.
    const data: string[] = [];
    const { term } = await mountRealWterm((d) => data.push(d));
    term.write("\x1b[6n\x1b[c");
    await renderFlush();

    expect(data.join("")).toBe("");
  });

  it("switches arrow keys to application cursor sequences when DECCKM is set", async () => {
    const data: string[] = [];
    const { term, textarea } = await mountRealWterm((d) => data.push(d));
    const pressUp = () =>
      textarea.dispatchEvent(
        new KeyboardEvent("keydown", { key: "ArrowUp", cancelable: true, bubbles: true }),
      );

    pressUp();
    expect(data.at(-1)).toBe("\x1b[A");

    term.write("\x1b[?1h");
    pressUp();
    expect(data.at(-1)).toBe("\x1bOA");

    term.write("\x1b[?1l");
    pressUp();
    expect(data.at(-1)).toBe("\x1b[A");
  });

  it("wraps native paste in bracketed-paste guards and strips ESC injection (bake-off: paste)", async () => {
    const data: string[] = [];
    const { term, textarea } = await mountRealWterm((d) => data.push(d));

    const dispatchPaste = (text: string) => {
      const event = new Event("paste", { cancelable: true, bubbles: true });
      Object.defineProperty(event, "clipboardData", {
        value: { getData: () => text },
      });
      textarea.dispatchEvent(event);
    };

    dispatchPaste("plain paste");
    expect(data.at(-1)).toBe("plain paste");

    term.write("\x1b[?2004h");
    dispatchPaste("rm -rf\x1b[201~evil");
    // Payload ESC bytes are stripped so clipboard text cannot close the
    // bracketed-paste guard and smuggle commands to the PTY.
    expect(data.at(-1)).toBe("\x1b[200~rm -rf[201~evil\x1b[201~");
  });

  it("ships an iOS-safe hidden input (autocapitalize/autocorrect/spellcheck off)", async () => {
    const { textarea } = await mountRealWterm();
    expect(textarea.getAttribute("autocapitalize")).toBe("off");
    expect(textarea.getAttribute("autocorrect")).toBe("off");
    expect(textarea.getAttribute("spellcheck")).toBe("false");
  });
});
