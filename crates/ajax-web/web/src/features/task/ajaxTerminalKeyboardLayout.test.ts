import { describe, expect, it } from "vitest";
import {
  isAjaxKeyboardLayoutButton,
  mapAjaxKeyboardButton,
  nextAjaxKeyboardLayout,
} from "./ajaxTerminalKeyboardLayout";

describe("mapAjaxKeyboardButton", () => {
  it("maps enter, backspace, and space to PTY bytes", () => {
    expect(mapAjaxKeyboardButton("{enter}")).toBe("\r");
    expect(mapAjaxKeyboardButton("{bksp}")).toBe("\x7f");
    expect(mapAjaxKeyboardButton("{space}")).toBe(" ");
  });

  it("passes through single printable characters", () => {
    expect(mapAjaxKeyboardButton("a")).toBe("a");
    expect(mapAjaxKeyboardButton("A")).toBe("A");
    expect(mapAjaxKeyboardButton("$")).toBe("$");
  });

  it("maps escaped brace buttons", () => {
    expect(mapAjaxKeyboardButton("{{")).toBe("{");
    expect(mapAjaxKeyboardButton("}}")).toBe("}");
  });

  it("returns null for layout and hide controls", () => {
    expect(mapAjaxKeyboardButton("{shift}")).toBeNull();
    expect(mapAjaxKeyboardButton("{numbers}")).toBeNull();
    expect(mapAjaxKeyboardButton("{symbols}")).toBeNull();
    expect(mapAjaxKeyboardButton("{abc}")).toBeNull();
    expect(mapAjaxKeyboardButton("{hide}")).toBeNull();
    expect(mapAjaxKeyboardButton("{half}")).toBeNull();
  });
});

describe("nextAjaxKeyboardLayout", () => {
  it("toggles shift against default", () => {
    expect(nextAjaxKeyboardLayout("default", "{shift}")).toBe("shift");
    expect(nextAjaxKeyboardLayout("shift", "{shift}")).toBe("default");
  });

  it("switches to numbers, symbols, and abc", () => {
    expect(nextAjaxKeyboardLayout("default", "{numbers}")).toBe("numbers");
    expect(nextAjaxKeyboardLayout("numbers", "{symbols}")).toBe("symbols");
    expect(nextAjaxKeyboardLayout("symbols", "{abc}")).toBe("default");
  });

  it("returns null for emit buttons", () => {
    expect(nextAjaxKeyboardLayout("default", "a")).toBeNull();
    expect(nextAjaxKeyboardLayout("default", "{enter}")).toBeNull();
  });
});

describe("isAjaxKeyboardLayoutButton", () => {
  it("recognizes layout-only buttons", () => {
    expect(isAjaxKeyboardLayoutButton("{hide}")).toBe(true);
    expect(isAjaxKeyboardLayoutButton("{shift}")).toBe(true);
    expect(isAjaxKeyboardLayoutButton("{half}")).toBe(true);
    expect(isAjaxKeyboardLayoutButton("{enter}")).toBe(false);
  });
});
