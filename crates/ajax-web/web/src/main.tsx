import { createRoot } from "react-dom/client";
import App from "./components/App";
import { ErrorBoundary } from "./react/ErrorBoundary";
import "./styles.css";

const el = document.getElementById("app");
if (el) {
  createRoot(el).render(
    <ErrorBoundary>
      <App />
    </ErrorBoundary>,
  );
} else {
  console.error("[ajax] #app element not found — React app not mounted");
}
