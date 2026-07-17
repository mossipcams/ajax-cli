import { describe, it, expect } from "vitest";
import { createElement, act } from "react";
import { mountIsland } from "./mountIsland";

function TextDisplay(props: { text: string }) {
  return createElement("span", null, props.text);
}

describe("mountIsland", () => {
  it("mounts, updates props, and unmounts cleanly", () => {
    const host = document.createElement("div");
    document.body.appendChild(host);

    let island!: ReturnType<typeof mountIsland<{ text: string }>>;

    act(() => {
      island = mountIsland(host, TextDisplay, { text: "hello" });
    });

    expect(host.textContent).toBe("hello");

    act(() => {
      island.update({ text: "world" });
    });

    expect(host.textContent).toBe("world");

    act(() => {
      island.unmount();
    });

    expect(host.textContent).toBe("");

    document.body.removeChild(host);
  });

  it("contains thrown errors within the island", () => {
    const host = document.createElement("div");
    document.body.appendChild(host);

    function Thrower() {
      throw new Error("boom");
    }

    act(() => {
      mountIsland(host, Thrower, {});
    });

    expect(host.textContent).toContain("Incompatible server response");

    document.body.removeChild(host);
  });
});
