import { Component, type ErrorInfo, type ReactNode } from "react";

interface Props {
  children: ReactNode;
}

interface State {
  error: Error | null;
}

function isIncompatibleResponse(error: Error): boolean {
  return (error as Error & { kind?: string }).kind === "incompatible";
}

/**
 * Whole-app boundary. It previously discarded the error and rendered
 * "Incompatible server response" for *every* crash, so a render bug was
 * indistinguishable from a real contract failure and the message actively
 * pointed diagnosis at the server. Keep that wording only for genuine
 * contract failures, show the real message otherwise, and always log.
 */
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
      </div>
    );
  }
}
