import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL(".", import.meta.url));

// The Rust asset adapter embeds and fingerprints the built files by exact
// name, so the build must emit deterministic, non-hashed output:
//   dist/index.html, dist/app.js, dist/app.css
// Do not enable content hashing here without updating adapters/assets.rs.
export default defineConfig({
  root,
  base: "/",
  plugins: [svelte()],
  build: {
    outDir: "dist",
    emptyOutDir: true,
    // No hashing: filenames are part of the Rust embed contract.
    assetsInlineLimit: 0,
    rollupOptions: {
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
