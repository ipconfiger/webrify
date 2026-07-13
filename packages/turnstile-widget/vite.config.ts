import react from "@vitejs/plugin-react";
import wasm from "vite-plugin-wasm";
import topLevelAwait from "vite-plugin-top-level-await";
import { defineConfig } from "vite";

// Library build: emits a single ES module `dist/turnstile.js` (the widget entry)
// plus a separate worker chunk and the WASM asset. rust-embed serves all of
// dist/ under /widget/* (Phase 1.10 / Wave C step C4).
export default defineConfig({
  // The widget is served from `/widget/*` by rust-embed. Setting `base` makes
  // Vite prefix all internal asset URLs (worker chunk, wasm) with `/widget/`
  // so they resolve correctly instead of 404-ing at the domain root.
  base: "/widget/",
  plugins: [react(), wasm(), topLevelAwait()],
  // react-dom branches on `process.env.NODE_ENV`; Vite library mode doesn't
  // replace it by default, and `process` doesn't exist in the browser. Define
  // it so the production path is baked in at build time.
  define: {
    "process.env.NODE_ENV": JSON.stringify("production"),
  },
  build: {
    lib: {
      entry: "src/index.ts",
      formats: ["es"],
      fileName: "turnstile",
    },
    outDir: "dist",
    // Keep committed placeholder(s) across builds — don't wipe dist/. Stale
    // hashed chunks may accumulate (harmless, gitignored); the `build.rs`
    // rerun-if-changed tracking handles re-embedding.
    emptyOutDir: false,
    target: "es2022",
  },
  worker: {
    plugins: () => [wasm()],
    format: "es",
  },
});
