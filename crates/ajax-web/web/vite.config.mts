import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { fileURLToPath } from "node:url";
import { renameSync, existsSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const root = fileURLToPath(new URL(".", import.meta.url));

function renameAppHtml() {
  return {
    name: "ajax-rename-app-html",
    writeBundle() {
      const from = join(root, "dist", "app.html");
      const to = join(root, "dist", "index.html");
      if (!existsSync(from)) {
        throw new Error(`ajax-rename-app-html: expected dist/app.html but it was not produced — the build may be incomplete`);
      }
      const appJs = join(root, "dist", "app.js");
      if (existsSync(appJs)) {
        writeFileSync(
          appJs,
          readFileSync(appJs, "utf8").replaceAll(
            "import(`./terminal.js`)",
            'import("./terminal.js")',
          ),
        );
      }
      renameSync(from, to);
    },
  };
}

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
    assetsInlineLimit: 0,
    cssCodeSplit: false,
    modulePreload: false,
    rollupOptions: {
      input: join(root, "app.html"),
      output: {
        entryFileNames: "app.js",
        chunkFileNames: (chunk) => {
          if (
            chunk.name === "TaskTerminal" ||
            chunk.name === "terminal" ||
            chunk.moduleIds?.some(
              (id) =>
                typeof id === "string" &&
                (id.includes("/features/terminal/TaskTerminal") ||
                  id.includes("node_modules/@xterm/")),
            )
          ) {
            return "terminal.js";
          }
          if (chunk.name === "rolldown-runtime") return "app.js";
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
    alias: {
      "@testing-library/react": join(root, "src/test/testing-library-shim.tsx"),
      "@testing-library/react-original": join(
        root,
        "../../../node_modules/@testing-library/react",
      ),
    },
  },
});
