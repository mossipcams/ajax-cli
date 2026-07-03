import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import ActionBar from "./ActionBar.svelte";
import * as api from "../api";
import type { WebAction } from "../types";

const review: WebAction = {
  action: "review",
  label: "Review",
  destructive: false,
  confirmation_required: false,
};
const drop: WebAction = {
  action: "drop",
  label: "Drop",
  destructive: true,
  confirmation_required: true,
};

describe("ActionBar", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => {
    vi.useRealTimers();
    vi.restoreAllMocks();
  });

  it("renders only returned actions, first as primary", () => {
    const { container, getByText } = render(ActionBar, {
      props: { actions: [review, drop], handle: "web/x" },
    });
    expect(getByText("Review").classList.contains("primary")).toBe(true);
    expect(container.querySelectorAll("button[data-action]")).toHaveLength(2);
  });

  it("requires two taps for a destructive action", async () => {
    const spy = vi.spyOn(api, "postOperation").mockResolvedValue({ ok: true, response: {} });
    const { getByText } = render(ActionBar, { props: { actions: [drop], handle: "web/x" } });
    await fireEvent.click(getByText("Drop"));
    expect(spy).not.toHaveBeenCalled();
    expect(getByText("Tap to confirm")).toBeInTheDocument();
    await fireEvent.click(getByText("Tap to confirm"));
    expect(spy).toHaveBeenCalledOnce();
  });

  it("resubmits a confirmed destructive action with the server confirmation token", async () => {
    vi.spyOn(api, "requestId").mockReturnValue("drop-request");
    const spy = vi
      .spyOn(api, "postOperation")
      .mockResolvedValueOnce({
        ok: false,
        response: {
          ok: false,
          request_id: "drop-request",
          state_changed: false,
          error: "confirmation required",
          confirmation_token: "server-token",
        },
      })
      .mockResolvedValueOnce({ ok: true, response: { ok: true } });
    const { getByText } = render(ActionBar, { props: { actions: [drop], handle: "web/x" } });

    await fireEvent.click(getByText("Drop"));
    await fireEvent.click(getByText("Tap to confirm"));
    await vi.runAllTimersAsync();

    expect(spy).toHaveBeenCalledTimes(2);
    expect(spy).toHaveBeenNthCalledWith(1, {
      task_handle: "web/x",
      action: "drop",
      request_id: "drop-request",
    });
    expect(spy).toHaveBeenNthCalledWith(2, {
      task_handle: "web/x",
      action: "drop",
      request_id: "drop-request",
      confirmation_token: "server-token",
    });
  });

  it("expires the confirmation after the timeout", async () => {
    const spy = vi.spyOn(api, "postOperation").mockResolvedValue({ ok: true, response: {} });
    const { getByText, queryByText } = render(ActionBar, {
      props: { actions: [drop], handle: "web/x" },
    });
    await fireEvent.click(getByText("Drop"));
    expect(getByText("Tap to confirm")).toBeInTheDocument();
    vi.advanceTimersByTime(8000);
    await Promise.resolve();
    expect(queryByText("Tap to confirm")).toBeNull();
    expect(spy).not.toHaveBeenCalled();
  });

  it("runs a non-destructive action immediately and forwards the refreshed cockpit", async () => {
    const cockpit = {
      backend: { authority: "host-native", control_enabled: true },
      repos: { repos: [] },
      cards: [],
      inbox: { items: [] },
    };
    vi.spyOn(api, "postOperation").mockResolvedValue({ ok: true, response: { cockpit } });
    const onCockpit = vi.fn();
    const { getByText } = render(ActionBar, {
      props: { actions: [review], handle: "web/x", onCockpit },
    });
    await fireEvent.click(getByText("Review"));
    await vi.runAllTimersAsync();
    expect(onCockpit).toHaveBeenCalledWith(cockpit);
  });
});
