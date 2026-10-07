const CSI_ERASE_CARRY_MAX = 16;

const CSI_ERASE_RE = /\x1b\[[0-9;]*J/;
const CSI_ERASE_COMPLETE_TAIL_RE = /\x1b\[[0-9;]*J$/;

export function detectCsiEraseInDisplay(
  carry: string,
  chunk: string,
): { sawErase: boolean; carry: string } {
  const buf = carry + chunk;
  const sawErase = CSI_ERASE_RE.test(buf);

  let newCarry = "";
  const esc = buf.lastIndexOf("\x1b");
  if (esc >= 0) {
    const tail = buf.slice(esc);
    if (tail.length <= CSI_ERASE_CARRY_MAX) {
      if (tail === "\x1b") {
        newCarry = tail;
      } else if (tail.startsWith("\x1b[") && !CSI_ERASE_COMPLETE_TAIL_RE.test(tail)) {
        newCarry = tail;
      }
    }
  }

  return { sawErase, carry: newCarry };
}
