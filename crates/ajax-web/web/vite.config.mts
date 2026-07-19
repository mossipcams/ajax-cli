import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { fileURLToPath } from "node:url";
import { renameSync, existsSync } from "node:fs";
import { join } from "node:path";

const root = fileURLToPath(new URL(".", import.meta.url));

// The app entry lives in `app.html` so Vite uses a predictable output name.
// We rename the built `dist/app.html` to `dist/index.html` so the Rust embed
// in adapters/assets.rs finds the conventional name.
function renameAppHtml() {
  return {
    name: "ajax-rename-app-html",
    closeBundle() {
      const from = join(root, "dist", "app.html");
      const to = join(root, "dist", "index.html");
      if (!existsSync(from)) {
        throw new Error(`ajax-rename-app-html: expected dist/app.html but it was not produced — the build may be incomplete`);
      }
      renameSync(from, to);
    },
  };
}

// The Rust asset adapter embeds and fingerprints the built files by exact
// name, so the build must emit deterministic, non-hashed output:
//   dist/index.html, dist/app.js, dist/app.css, dist/terminal.js
// Do not enable content hashing here without updating adapters/assets.rs.
// terminal.js is the deferred TaskTerminal + @xterm chunk (slice 11).
export default defineConfig({
  root,
  base: "/",
  plugins: [react(), tailwindcss(), renameAppHtml()],
  resolve: {
    alias: {
      "@": join(root, "src"),
    },
  },
  build: {
    outDir: "dist",
    emptyOutDir: true,
    // No hashing: filenames are part of the Rust embed contract.
    assetsInlineLimit: 0,
    cssCodeSplit: false,
    // terminal.js must stay lazy (task-route dynamic import). Vite's default
    // modulepreload would fetch it on every boot and defeat the split.
    modulePreload: false,
    rollupOptions: {
      input: join(root, "app.html"),
      output: {
        entryFileNames: "app.js",
        // One deferred async chunk from lazy(() => import("./TaskTerminal")).
        // Do not use manualChunks for TaskTerminal/@xterm: forcing those modules
        // into a named chunk pulled shared boot modules (api.ts) into terminal.js
        // and left /api/operations out of app.js.
        chunkFileNames: (chunk) => {
          if (
            chunk.name === "TaskTerminal" ||
            chunk.name === "terminal" ||
            chunk.moduleIds?.some(
              (id) =>
                typeof id === "string" &&
                (id.includes("/features/task/TaskTerminal") ||
                  id.includes("node_modules/@xterm/")),
            )
          ) {
            return "terminal.js";
          }
          throw new Error(
            `ajax vite: unexpected chunk "${chunk.name}" — only app.js + terminal.js are allowed`,
          );
        },
        assetFileNames: (asset) => {
          const name = asset.names?.[0] ?? "";
          if (name.endsWith(".css")) return "app.css";
          return "[name][extname]";
        },
      },
    },
  },
  server: {
    proxy: {
      // Forward API calls to the locally running Rust HTTPS dev server.
      "/api": {
        target: "https://127.0.0.1:8788",
        changeOrigin: true,
        secure: false,
      },
    },
  },
  test: {
    environment: "jsdom",
    globals: true,
    include: ["src/**/*.test.{ts,tsx}"],
    setupFiles: ["src/test-setup.ts"],
  },
});
