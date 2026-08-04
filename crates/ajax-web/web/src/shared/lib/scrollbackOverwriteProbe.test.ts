/**
 * Buffer-level lock for seed-window scrollOnErase latching.
 * Permanent scrollOnErase dumps live ED2 frames into scrollback; bootstrap-only
 * keeps attach seed preservation without polluting history.
 */
import { beforeAll, describe, expect, it } from "vitest";
import { Terminal } from "@xterm/xterm";

beforeAll(() => {
  if (!window.matchMedia) {
    window.matchMedia = ((query: string) =>
      ({
        matches: false,
        media: query,
        onchange: null,
        addListener() {},
        removeListener() {},
        addEventListener() {},
        removeEventListener() {},
        dispatchEvent() {
          return false;
        },
      })) as typeof window.matchMedia;
  }

  const proto = HTMLCanvasElement.prototype as HTMLCanvasElement & {
    getContext: typeof HTMLCanvasElement.prototype.getContext;
  };
  proto.getContext = (() =>
    ({
      fillRect() {},
      clearRect() {},
      getImageData: () => ({ data: new Uint8ClampedArray(4) }),
      putImageData() {},
      createImageData: () => ({ data: new Uint8ClampedArray(4) }),
      setTransform() {},
      drawImage() {},
      save() {},
      fillText() {},
      restore() {},
      beginPath() {},
      moveTo() {},
      lineTo() {},
      closePath() {},
      stroke() {},
      translate() {},
      scale() {},
      rotate() {},
      arc() {},
      fill() {},
      measureText: () => ({ width: 8 }),
      transform() {},
      rect() {},
      clip() {},
      canvas: document.createElement("canvas"),
    })) as typeof HTMLCanvasElement.prototype.getContext;
});

function collectNonEmpty(term: Terminal): string[] {
  const buf = term.buffer.active;
  const out: string[] = [];
  for (let y = 0; y < buf.length; y++) {
    const line = buf.getLine(y)?.translateToString(true) ?? "";
    if (line.trim()) out.push(line);
  }
  return out;
}

function write(term: Terminal, data: string): Promise<void> {
  return new Promise((resolve) => term.write(data, resolve));
}

async function seedHistory(term: Terminal, count: number) {
  let payload = "";
  for (let i = 1; i <= count; i++) {
    payload += `SEED-${String(i).padStart(2, "0")} unique-marker-line\r\n`;
  }
  await write(term, payload);
}

async function openTerm(opts: {
  scrollOnEraseInDisplay?: boolean;
}): Promise<{ term: Terminal; host: HTMLDivElement }> {
  const host = document.createElement("div");
  host.style.width = "320px";
  host.style.height = "96px";
  document.body.appendChild(host);
  const term = new Terminal({
    cols: 40,
    rows: 6,
    scrollback: 200,
    scrollOnEraseInDisplay: opts.scrollOnEraseInDisplay,
  });
  term.open(host);
  return { term, host };
}

describe("scrollOnErase bootstrap latch", () => {
  it("ED2 + scrollOnErase keeps seed markers above live screen", async () => {
    const { term, host } = await openTerm({ scrollOnEraseInDisplay: true });
    await seedHistory(term, 8);
    await write(term, "\x1b[H\x1b[2J");
    await write(term, "LIVE-SCREEN row0\r\nLIVE-SCREEN row1\r\n");
    const lines = collectNonEmpty(term);
    expect(lines.some((l) => l.includes("SEED-08"))).toBe(true);
    expect(lines.some((l) => l.includes("SEED-03"))).toBe(true);
    expect(lines.some((l) => l.includes("LIVE-SCREEN"))).toBe(true);
    term.dispose();
    host.remove();
  });

  it("bootstrap-only scrollOnErase: seed survives first ED2; later ED2 does not dump", async () => {
    const { term, host } = await openTerm({ scrollOnEraseInDisplay: true });
    await seedHistory(term, 8);
    await write(term, "\x1b[H\x1b[2J");
    await write(term, "LIVE-A\r\n");
    term.options.scrollOnEraseInDisplay = false;
    await write(term, "\x1b[H\x1b[2J");
    await write(term, "LIVE-B\r\n");
    const lines = collectNonEmpty(term);
    expect(lines.some((l) => l.includes("SEED-08"))).toBe(true);
    expect(lines.some((l) => l.includes("LIVE-A"))).toBe(false);
    expect(lines.some((l) => l.includes("LIVE-B"))).toBe(true);
    term.dispose();
    host.remove();
  });

  it("latching off before the first ED2 wipes seed instead of preserving it", async () => {
    const { term, host } = await openTerm({ scrollOnEraseInDisplay: true });
    await seedHistory(term, 8);
    // Premature reveal-time latch (the race): ED2 then clears without scrollback push.
    term.options.scrollOnEraseInDisplay = false;
    await write(term, "\x1b[H\x1b[2J");
    await write(term, "LIVE-WIPED\r\n");
    const lines = collectNonEmpty(term);
    expect(lines.some((l) => l.includes("SEED-08"))).toBe(false);
    expect(lines.some((l) => l.includes("LIVE-WIPED"))).toBe(true);
    term.dispose();
    host.remove();
  });
});
