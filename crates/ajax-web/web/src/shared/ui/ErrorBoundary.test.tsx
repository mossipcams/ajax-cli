import { describe, it, expect, vi, afterEach } from "vitest";
import { render, screen } from "@testing-library/react";
import { ErrorBoundary } from "./ErrorBoundary";
import { IncompatibleResponseError } from "@/shared/lib/contracts";

function Boom({ error }: { error: Error }): never {
  throw error;
}

describe("ErrorBoundary", () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("renders children when nothing throws", () => {
    render(
      <ErrorBoundary>
        <p>content</p>
      </ErrorBoundary>,
    );
    expect(screen.getByText("content")).toBeInTheDocument();
  });

  // A render bug used to be reported as "Incompatible server response", which
  // pointed diagnosis at the server and hid the real message.
  it("shows the real message for a render crash, not a server-contract claim", () => {
    vi.spyOn(console, "error").mockImplementation(() => {});
    render(
      <ErrorBoundary>
        <Boom error={new TypeError("undefined is not an object (evaluating 'x.length')")} />
      </ErrorBoundary>,
    );
    const alert = screen.getByRole("alert");
    expect(alert).toHaveTextContent("Something went wrong rendering this view");
    expect(alert).toHaveTextContent("undefined is not an object");
    expect(alert).not.toHaveTextContent("Incompatible server response");
  });

  it("keeps the incompatible wording for a genuine contract failure", () => {
    vi.spyOn(console, "error").mockImplementation(() => {});
    render(
      <ErrorBoundary>
        <Boom error={new IncompatibleResponseError("detail.status is invalid: nope")} />
      </ErrorBoundary>,
    );
    const alert = screen.getByRole("alert");
    expect(alert).toHaveTextContent("Incompatible server response");
    expect(alert).toHaveTextContent("detail.status is invalid: nope");
  });

  it("logs the error and component stack for diagnosis", () => {
    const logged = vi.spyOn(console, "error").mockImplementation(() => {});
    render(
      <ErrorBoundary>
        <Boom error={new Error("kaboom")} />
      </ErrorBoundary>,
    );
    expect(logged).toHaveBeenCalledWith(
      "[ajax] render crash:",
      expect.objectContaining({ message: "kaboom" }),
      expect.anything(),
    );
  });
});
