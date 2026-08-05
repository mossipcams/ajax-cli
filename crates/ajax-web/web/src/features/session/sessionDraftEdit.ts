/** Caret-aware draft edits for the Ajax Web Session composer + hotkey bar. */

export type DraftSelection = {
  value: string;
  selectionStart: number;
  selectionEnd: number;
};

function clamp(n: number, min: number, max: number): number {
  return Math.max(min, Math.min(max, n));
}

function lineBounds(value: string, index: number): { start: number; end: number } {
  const start = value.lastIndexOf("\n", Math.max(0, index - 1)) + 1;
  const nextNl = value.indexOf("\n", index);
  const end = nextNl === -1 ? value.length : nextNl;
  return { start, end };
}

export function insertAtSelection(
  state: DraftSelection,
  text: string,
): DraftSelection {
  const start = clamp(state.selectionStart, 0, state.value.length);
  const end = clamp(state.selectionEnd, 0, state.value.length);
  const lo = Math.min(start, end);
  const hi = Math.max(start, end);
  const value = state.value.slice(0, lo) + text + state.value.slice(hi);
  const caret = lo + text.length;
  return { value, selectionStart: caret, selectionEnd: caret };
}

export function deleteBackward(state: DraftSelection): DraftSelection {
  const start = clamp(state.selectionStart, 0, state.value.length);
  const end = clamp(state.selectionEnd, 0, state.value.length);
  if (start !== end) {
    const lo = Math.min(start, end);
    const hi = Math.max(start, end);
    const value = state.value.slice(0, lo) + state.value.slice(hi);
    return { value, selectionStart: lo, selectionEnd: lo };
  }
  if (start === 0) return state;
  const value = state.value.slice(0, start - 1) + state.value.slice(start);
  return { value, selectionStart: start - 1, selectionEnd: start - 1 };
}

export function moveCaret(
  state: DraftSelection,
  direction: "left" | "right" | "up" | "down",
): DraftSelection {
  const start = clamp(state.selectionStart, 0, state.value.length);
  const end = clamp(state.selectionEnd, 0, state.value.length);
  let caret = Math.min(start, end);

  if (direction === "left") {
    caret = Math.max(0, caret - 1);
  } else if (direction === "right") {
    caret = Math.min(state.value.length, Math.max(start, end) + 1);
  } else if (direction === "up") {
    const { start: lineStart } = lineBounds(state.value, caret);
    if (lineStart === 0) {
      caret = 0;
    } else {
      const col = caret - lineStart;
      const prevEnd = lineStart - 1;
      const { start: prevStart } = lineBounds(state.value, prevEnd);
      caret = Math.min(prevStart + col, prevEnd);
    }
  } else {
    const { start: lineStart, end: lineEnd } = lineBounds(state.value, caret);
    if (lineEnd >= state.value.length) {
      caret = state.value.length;
    } else {
      const col = caret - lineStart;
      const nextStart = lineEnd + 1;
      const { end: nextEnd } = lineBounds(state.value, nextStart);
      caret = Math.min(nextStart + col, nextEnd);
    }
  }

  return { value: state.value, selectionStart: caret, selectionEnd: caret };
}
