import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import { svelteTesting } from "@testing-library/svelte/vite";
import { fileURLToPath } from "node:url";
import { renameSync, existsSync } from "node:fs";
import { join } from "node:path";

const root = fileURLToPath(new URL(".", import.meta.url));

// The Svelte entry lives in `app.html` (not `index.html`) so the legacy
// `index.html` that Rust still serves stays untouched during the migration.
// We rename the built `dist/app.html` to `dist/index.html` so the eventual
// Rust embed (Phase 1.3) finds the conventional name.
function renameAppHtml() {
  return {
    name: "ajax-rename-app-html",
    closeBundle() {
      const from = join(root, "dist", "app.html");
      const to = join(root, "dist", "index.html");
      if (existsSync(from)) renameSync(from, to);
    },
  };
}

// The Rust asset adapter embeds and fingerprints the built files by exact
// name, so the build must emit deterministic, non-hashed output:
//   dist/index.html, dist/app.js, dist/app.css
// Do not enable content hashing here without updating adapters/assets.rs.
export default defineConfig({
  root,
  base: "/",
  plugins: [svelte(), svelteTesting(), renameAppHtml()],
  build: {
    outDir: "dist",
    emptyOutDir: true,
    // No hashing: filenames are part of the Rust embed contract.
    assetsInlineLimit: 0,
    rollupOptions: {
      input: join(root, "app.html"),
      output: {
        entryFileNames: "app.js",
        chunkFileNames: "app.js",
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
    include: ["src/**/*.test.ts"],
    setupFiles: ["src/test-setup.ts"],
  },
});
