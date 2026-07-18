import { render, screen, fireEvent, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import TestInDevPanel from "./TestInDevPanel";

const fetchDevDeploy = vi.fn();
const startDevDeploy = vi.fn();

vi.mock("../api", () => ({
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
    vi.stubGlobal("open", vi.fn());
  });

  it("shows ready state and fixed Open Dev URL", async () => {
    fetchDevDeploy.mockResolvedValue({
      ok: true,
      deploy: {
        phase: "ready_to_deploy",
        phase_label: "Ready to deploy",
        shared_slot: true,
        open_url: "https://ajaxdev.mossyhome.net:8788",
        active: false,
        error: null,
        occupant: null,
      },
    });

    render(<TestInDevPanel taskHandle="ajax-cli/demo" />);

    const panel = screen.getByTestId("test-in-dev");

    await waitFor(() => {
      expect(screen.getByTestId("test-in-dev-phase")).toHaveTextContent("Ready to deploy");
    });
    expect(within(panel).queryByText(/Shared Ajax Dev slot/)).toBeNull();
    expect(screen.queryByTestId("test-in-dev-occupant")).toBeNull();
    expect(screen.getByTestId("test-in-dev-button")).toBeEnabled();

    await fireEvent.click(screen.getByTestId("open-dev-button"));
    expect(window.open).toHaveBeenCalledWith(
      "https://ajaxdev.mossyhome.net:8788",
      "_blank",
      "noopener,noreferrer",
    );
  });

  it("disables Test in Dev while building and surfaces failure text", async () => {
    fetchDevDeploy
      .mockResolvedValueOnce({
        ok: true,
        deploy: {
          phase: "ready_to_deploy",
          phase_label: "Ready to deploy",
          shared_slot: true,
          open_url: "https://ajaxdev.mossyhome.net:8788",
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
          open_url: "https://ajaxdev.mossyhome.net:8788",
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
        open_url: "https://ajaxdev.mossyhome.net:8788",
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

    await fireEvent.click(screen.getByTestId("test-in-dev-button"));
    await waitFor(() => {
      expect(screen.getByTestId("test-in-dev-phase")).toHaveTextContent("Building");
      expect(screen.getByTestId("test-in-dev-button")).toBeDisabled();
    });
    expect(screen.queryByText(/Shared Ajax Dev slot/)).toBeNull();
    expect(screen.queryByTestId("test-in-dev-occupant")).toBeNull();
    expect(startDevDeploy).toHaveBeenCalledWith("ajax-cli/demo");
  });
});
