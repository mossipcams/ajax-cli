import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import App from "./App";
import { ErrorBoundary } from "@/shared/ui/ErrorBoundary";
import "../styles.css";

const el = document.getElementById("app");
if (el) {
  createRoot(el).render(
    <StrictMode>
      <ErrorBoundary>
        <App />
      </ErrorBoundary>
    </StrictMode>,
  );
} else {
  console.error("[ajax] #app element not found — React app not mounted");
}
