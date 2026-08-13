import { describe, it, expect, vi, afterEach } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import SessionStarter from "./SessionStarter";
import * as api from "@/shared/lib/api";
import cockpit from "@/fixtures/cockpit.json";

vi.mock("@/shared/lib/api", async () => {
  const actual = await vi.importActual<typeof import("@/shared/lib/api")>("@/shared/lib/api");
  return {
    ...actual,
    startTask: vi.fn(),
    requestId: vi.fn(() => "req-test"),
  };
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe("SessionStarter", () => {
  it("submits a cursor orchestration session", async () => {
    vi.mocked(api.startTask).mockResolvedValue({
      ok: true,
      response: { cockpit },
    });
    render(<SessionStarter repos={cockpit.repos.repos} />);
    fireEvent.input(screen.getByLabelText("Title"), { target: { value: "Ship it" } });
    fireEvent.submit(screen.getByRole("form", { name: "Start session" }));
    await vi.waitFor(() => expect(api.startTask).toHaveBeenCalledOnce());
    expect(api.startTask).toHaveBeenCalledWith(
      expect.objectContaining({
        agent: "cursor",
        orchestration_chat: true,
        title: "Ship it",
        request_id: "req-test",
      }),
    );
  });
});
