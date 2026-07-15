import { defineConfig, type ViteDevServer } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import { svelteTesting } from "@testing-library/svelte/vite";
import { fileURLToPath } from "node:url";
import { renameSync, existsSync, copyFileSync, createReadStream } from "node:fs";
import { join } from "node:path";

const root = fileURLToPath(new URL(".", import.meta.url));
const ghosttyWasm = fileURLToPath(
  new URL("../../../node_modules/ghostty-web/ghostty-vt.wasm", import.meta.url),
);

// The Svelte entry lives in `app.html` so Vite uses a predictable output name.
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

function copyGhosttyWasm() {
  return {
    name: "ajax-copy-ghostty-wasm",
    configureServer(server: ViteDevServer) {
      server.middlewares.use("/ghostty-vt.wasm", (_req, res) => {
        if (!existsSync(ghosttyWasm)) {
          res.statusCode = 404;
          res.end("ghostty-vt.wasm not found");
          return;
        }
        res.setHeader("Content-Type", "application/wasm");
        createReadStream(ghosttyWasm).pipe(res);
      });
    },
    closeBundle() {
      if (!existsSync(ghosttyWasm)) {
        throw new Error(
          `ajax-copy-ghostty-wasm: expected ${ghosttyWasm} but it was not found`,
        );
      }
      copyFileSync(ghosttyWasm, join(root, "dist", "ghostty-vt.wasm"));
    },
  };
}

// The Rust asset adapter embeds and fingerprints the built files by exact
// name, so the build must emit deterministic, non-hashed output:
//   dist/index.html, dist/app.js, dist/terminal.js, dist/app.css,
//   dist/ghostty-vt.wasm
// Do not enable content hashing here without updating adapters/assets.rs.
export default defineConfig({
  root,
  base: "/",
  plugins: [svelte(), svelteTesting(), renameAppHtml(), copyGhosttyWasm()],
  build: {
    outDir: "dist",
    emptyOutDir: true,
    // No hashing: filenames are part of the Rust embed contract.
    assetsInlineLimit: 0,
    cssCodeSplit: false,
    rollupOptions: {
      input: join(root, "app.html"),
      output: {
        entryFileNames: "app.js",
        chunkFileNames: "terminal.js",
        experimentalMinChunkSize: 1000,
        onlyExplicitManualChunks: true,
        manualChunks(id) {
          // Keep terminalSurfaceSetting (+ thin selector) in the app shell.
          // Settings imports the setting module; putting either into the deferred
          // terminal chunk creates app↔terminal cycles / terminal2.js and fails
          // web:build:check. Heavy engines stay deferred via dynamic imports.
          if (id.includes("/web/src/terminalSurfaceSetting")) return;
          if (
            id.includes("/node_modules/ghostty-web/") ||
            id.includes("/node_modules/@xterm/") ||
            id.includes("/components/TerminalRawView.svelte") ||
            id.includes("/components/XtermTerminalView.svelte") ||
            id.includes("/web/src/terminal")
          ) return "terminal";
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
    include: ["src/**/*.test.ts"],
    setupFiles: ["src/test-setup.ts"],
  },
});
