import { Component, type ErrorInfo, type ReactNode } from "react";
import { Button } from "./button";

interface Props {
  children: ReactNode;
}

interface State {
  error: Error | null;
}

function isIncompatibleResponse(error: Error): boolean {
  return (error as Error & { kind?: string }).kind === "incompatible";
}

export class ErrorBoundary extends Component<Props, State> {
  state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error("[ajax] render crash:", error, info.componentStack);
  }

  render(): ReactNode {
    const { error } = this.state;
    if (!error) return this.props.children;
    return (
      <div role="alert" className="error-boundary">
        <p>
          {isIncompatibleResponse(error)
            ? "Incompatible server response"
            : "Something went wrong rendering this view"}
        </p>
        <pre className="error-boundary-detail">{error.message}</pre>
        <Button type="button" variant="secondary" onClick={() => this.setState({ error: null })}>
          Retry
        </Button>
      </div>
    );
  }
}
