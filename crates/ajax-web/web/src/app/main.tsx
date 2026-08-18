import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { QueryClientProvider } from "@tanstack/react-query";
import App from "./App";
import { ErrorBoundary } from "@/shared/ui/ErrorBoundary";
import { queryClient } from "@/shared/lib/queryClient";
import { initTelemetry } from "@/shared/lib/telemetry";
import "../styles.css";

initTelemetry();

const el = document.getElementById("app");
if (el) {
  createRoot(el).render(
    <StrictMode>
      <ErrorBoundary>
        <QueryClientProvider client={queryClient}>
          <App />
        </QueryClientProvider>
      </ErrorBoundary>
    </StrictMode>,
  );
} else {
  console.error("[ajax] #app element not found — React app not mounted");
}
