import { render, screen, fireEvent, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import TestInDevPanel from "./TestInDevPanel";

const fetchDevDeploy = vi.fn();
const startDevDeploy = vi.fn();

vi.mock("@/shared/lib/api", () => ({
  ApiError: class ApiError extends Error {
    constructor(message: string) {
      super(message);
      this.name = "ApiError";
    }
  },
  fetchDevDeploy: (...args: unknown[]) => fetchDevDeploy(...args),
  startDevDeploy: (...args: unknown[]) => startDevDeploy(...args),
}));

describe("TestInDevPanel", () => {
  beforeEach(() => {
    fetchDevDeploy.mockReset();
    startDevDeploy.mockReset();
  });

  it("shows ready state with Test in Dev button only", async () => {
    fetchDevDeploy.mockResolvedValue({
      ok: true,
      deploy: {
        phase: "ready_to_deploy",
        phase_label: "Ready to deploy",
        shared_slot: true,
        active: false,
        error: null,
        occupant: null,
      },
    });

    render(<TestInDevPanel taskHandle="ajax-cli/demo" />);

    const panel = screen.getByTestId("test-in-dev");

    await waitFor(() => {
      expect(screen.getByTestId("test-in-dev-button")).toHaveTextContent("Test in Dev");
    });
    expect(within(panel).queryByText(/Shared Ajax Dev slot/)).toBeNull();
    expect(screen.queryByTestId("test-in-dev-occupant")).toBeNull();
    expect(screen.getByTestId("test-in-dev-button")).toBeEnabled();
    expect(screen.queryByTestId("open-dev-button")).toBeNull();
  });

  it("disables Test in Dev while building and surfaces failure text", async () => {
    fetchDevDeploy
      .mockResolvedValueOnce({
        ok: true,
        deploy: {
          phase: "ready_to_deploy",
          phase_label: "Ready to deploy",
          shared_slot: true,
          active: false,
          error: null,
          occupant: null,
        },
      })
      .mockResolvedValue({
        ok: true,
        deploy: {
          phase: "failed",
          phase_label: "Failed",
          shared_slot: true,
          active: false,
          error: "cargo build failed",
          occupant: {
            task_handle: "ajax-cli/demo",
            title: "Demo",
            branch: "feat/demo",
            commit_sha: "abc123",
            dirty: true,
            deployed_at_unix_secs: 0,
          },
        },
      });

    startDevDeploy.mockResolvedValue({
      ok: true,
      deploy: {
        phase: "building",
        phase_label: "Building",
        shared_slot: true,
        active: true,
        error: null,
        occupant: {
          task_handle: "ajax-cli/demo",
          title: "Demo",
          branch: "feat/demo",
          commit_sha: "abc123",
          dirty: true,
          deployed_at_unix_secs: 0,
        },
      },
    });

    render(<TestInDevPanel taskHandle="ajax-cli/demo" />);
    await waitFor(() => expect(screen.getByTestId("test-in-dev-button")).toBeEnabled());

    fireEvent.click(screen.getByTestId("test-in-dev-button"));
    await waitFor(() => {
      expect(screen.getByTestId("test-in-dev-button")).toHaveTextContent("Building");
    });
    expect(screen.getByTestId("test-in-dev-button")).toBeDisabled();
    expect(screen.queryByText(/Shared Ajax Dev slot/)).toBeNull();
    expect(screen.queryByTestId("test-in-dev-occupant")).toBeNull();
    expect(startDevDeploy).toHaveBeenCalledWith("ajax-cli/demo");
  });

  it("does not surface a success toast when deploy starts", async () => {
    fetchDevDeploy.mockResolvedValue({
      ok: true,
      deploy: {
        phase: "ready_to_deploy",
        phase_label: "Ready to deploy",
        shared_slot: true,
        active: false,
        error: null,
        occupant: null,
      },
    });
    startDevDeploy.mockResolvedValue({
      ok: true,
      deploy: {
        phase: "building",
        phase_label: "Building",
        shared_slot: true,
        active: true,
        error: null,
        occupant: null,
      },
    });
    const onResult = vi.fn();
    render(<TestInDevPanel taskHandle="ajax-cli/demo" onResult={onResult} />);
    await waitFor(() => expect(screen.getByTestId("test-in-dev-button")).toBeEnabled());
    fireEvent.click(screen.getByTestId("test-in-dev-button"));
    await waitFor(() => expect(startDevDeploy).toHaveBeenCalled());
    expect(onResult).not.toHaveBeenCalled();
  });

  it("surfaces an error toast when deploy fails to start", async () => {
    fetchDevDeploy.mockResolvedValue({
      ok: true,
      deploy: {
        phase: "ready_to_deploy",
        phase_label: "Ready to deploy",
        shared_slot: true,
        active: false,
        error: null,
        occupant: null,
      },
    });
    startDevDeploy.mockRejectedValue(new Error("slot busy"));
    const onResult = vi.fn();
    render(<TestInDevPanel taskHandle="ajax-cli/demo" onResult={onResult} />);
    await waitFor(() => expect(screen.getByTestId("test-in-dev-button")).toBeEnabled());
    fireEvent.click(screen.getByTestId("test-in-dev-button"));
    await waitFor(() => expect(onResult).toHaveBeenCalledWith("Test in Dev failed to start", null, true));
  });

  // GitHub issue #1035: TanStack Query mount fetch must not overwrite the
  // accepted deploy-start response and hide an in-progress Test in Dev run.
  it("issue #1035 keeps building state when a stale status fetch completes after start", async () => {
    let releaseInitialFetch!: () => void;
    fetchDevDeploy.mockImplementation((signal?: AbortSignal) =>
      new Promise((resolve, reject) => {
        releaseInitialFetch = () =>
          resolve({
            ok: true,
            deploy: {
              phase: "ready_to_deploy",
              phase_label: "Ready to deploy",
              shared_slot: true,
              active: false,
              error: null,
              occupant: null,
            },
          });
        signal?.addEventListener(
          "abort",
          () => reject(new DOMException("The operation was aborted.", "AbortError")),
          { once: true },
        );
      }),
    );
    startDevDeploy.mockResolvedValue({
      ok: true,
      deploy: {
        phase: "building",
        phase_label: "Building",
        shared_slot: true,
        active: true,
        error: null,
        occupant: {
          task_handle: "ajax-cli/demo",
          title: "Demo",
          branch: "feat/demo",
          commit_sha: "abc123",
          dirty: true,
          deployed_at_unix_secs: 0,
        },
      },
    });

    render(<TestInDevPanel taskHandle="ajax-cli/demo" />);
    fireEvent.click(screen.getByTestId("test-in-dev-button"));
    await waitFor(() => expect(startDevDeploy).toHaveBeenCalledWith("ajax-cli/demo"));
    await waitFor(() =>
      expect(screen.getByTestId("test-in-dev-button")).toHaveTextContent("Building"),
    );

    releaseInitialFetch();
    await waitFor(() =>
      expect(screen.getByTestId("test-in-dev-button")).toHaveTextContent("Building"),
    );
    expect(screen.getByTestId("test-in-dev-button")).toBeDisabled();
  });

  it("starts deploy only once under same-turn double click", async () => {
    fetchDevDeploy.mockResolvedValue({
      ok: true,
      deploy: {
        phase: "ready_to_deploy",
        phase_label: "Ready to deploy",
        shared_slot: true,
        active: false,
        error: null,
        occupant: null,
      },
    });
    let release!: () => void;
    startDevDeploy.mockReturnValue(
      new Promise((resolve) => {
        release = () =>
          resolve({
            ok: true,
            deploy: {
              phase: "deploying",
              phase_label: "Deploying",
              shared_slot: true,
              active: true,
              error: null,
              occupant: null,
            },
          });
      }),
    );
    render(<TestInDevPanel taskHandle="ajax-cli/demo" />);
    await waitFor(() => expect(screen.getByTestId("test-in-dev-button")).toBeEnabled());
    const button = screen.getByTestId("test-in-dev-button");
    button.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    button.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    expect(startDevDeploy).toHaveBeenCalledOnce();
    release();
  });
});
