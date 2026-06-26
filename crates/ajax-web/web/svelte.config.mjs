import { vitePreprocess } from "@sveltejs/vite-plugin-svelte";

// The style pass runs Vite's `preprocessCSS`, which we only need for CSS
// preprocessors (none here — all styles are plain CSS). Under Vitest that pass
// crashes (vite 6 partial-environment proxy bug) for any component carrying a
// scoped <style>, so disable it in test only. Build and svelte-check keep the
// default behavior.
const isVitest = !!process.env.VITEST;

export default {
  preprocess: vitePreprocess(isVitest ? { style: false } : undefined),
};
