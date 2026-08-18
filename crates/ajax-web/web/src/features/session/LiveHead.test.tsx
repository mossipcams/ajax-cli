import { describe, it, expect, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import LiveHead from "./LiveHead";

const noop = vi.fn();

function mountHead(
  overrides: Partial<React.ComponentProps<typeof LiveHead>> = {},
) {
  return render(
    <LiveHead
      title="Fix login"
      state="idle"
      tone="idle"
      detail={null}
      decision={null}
      tool={null}
      planStep={null}
      thoughtSnippet={null}
      status={null}
      usage={null}
      activityAgeMs={0}
      connected
      onBack={noop}
      onApprove={noop}
      onReject={noop}
      onStop={noop}
      onOpenDetails={noop}
      {...overrides}
    />,
  );
}

describe("LiveHead context usage", () => {
  it("shows reported usage in idle below 70%", () => {
    mountHead({ state: "idle", usage: { used: 25, size: 100 } });
    expect(screen.getByTestId("session-usage")).toHaveTextContent("Context 25% full");
  });

  it("shows reported usage while working below 70%", () => {
    mountHead({
      state: "working",
      tone: "running",
      usage: { used: 40, size: 100 },
      thoughtSnippet: "Checking files",
    });
    expect(screen.getByTestId("session-usage")).toHaveTextContent("Context 40% full");
  });

  it("does not render a meter when usage is absent", () => {
    mountHead({ state: "idle", usage: null });
    expect(screen.queryByTestId("session-usage")).not.toBeInTheDocument();
  });

  it("warns when context pressure is at or above 90%", () => {
    mountHead({ state: "idle", usage: { used: 92, size: 100 } });
    expect(screen.getByTestId("session-usage")).toHaveClass("is-tight");
  });
});
