import react from "@vitejs/plugin-react";
import wasm from "vite-plugin-wasm";
import topLevelAwait from "vite-plugin-top-level-await";
import dts from "vite-plugin-dts";
import { defineConfig, type Plugin } from "vite";
import { readFileSync, writeFileSync } from "fs";
import { resolve } from "path";

/**
 * After dts generates per-module .d.ts files, inline them into a
 * self-contained index.d.ts so consumers only need the single file.
 */
function bundleDtsPlugin(): Plugin {
  return {
    name: "bundle-dts",
    enforce: "post",
    closeBundle() {
      const dist = resolve(__dirname, "dist");
      const indexPath = resolve(dist, "index.d.ts");
      let content: string;
      try {
        content = readFileSync(indexPath, "utf-8");
      } catch {
        return; // no index.d.ts (e.g. dev mode)
      }

      // Collect exports from referenced local .d.ts files
      const lines = content.split("\n");
      const result: string[] = [];
      const seenImports = new Set<string>();

      for (const line of lines) {
        const m =
          /^(?:import\s+\{[^}]+\}\s+from\s+['"]((\.\/[^'"]+)\.js)['"];?\s*$)|(?:export\s+(type\s+)?\{[^}]+\}\s+from\s+['"]((\.\/[^'"]+)\.js)['"];?\s*$)/.exec(
            line.trim()
          );
        if (m) {
          // m[2] = path w/o .js from import branch; m[5] = same from export branch
          const relPath = m[2] || m[5];
          if (relPath && !seenImports.has(relPath)) {
            seenImports.add(relPath);
            try {
              const depPath = resolve(dist, relPath + ".d.ts");
              let depContent = readFileSync(depPath, "utf-8");
              // strip any internal imports the dep may have
              depContent = depContent.replace(
                /^import\s+[^'"]+['"][^'"]+['"];?\s*$/gm,
                ""
              );
              result.push(depContent.trim());
            } catch {
              // dep doesn't exist — keep the original line
              result.push(line);
            }
          }
          continue;
        }
        result.push(line);
      }

      writeFileSync(indexPath, result.join("\n").replace(/\n{3,}/g, "\n\n"));
    },
  };
}

// Library build: emits a single ES module `dist/turnstile.js` (the widget entry)
// plus a separate worker chunk and the WASM asset. rust-embed serves all of
// dist/ under /widget/* (Phase 1.10 / Wave C step C4).
export default defineConfig({
  // The widget is served from `/widget/*` by rust-embed. Setting `base` makes
  // Vite prefix all internal asset URLs (worker chunk, wasm) with `/widget/`
  // so they resolve correctly instead of 404-ing at the domain root.
  base: "/widget/",
  plugins: [
    react(),
    wasm(),
    topLevelAwait(),
    dts({
      rollupTypes: true,
      beforeWriteFile: (filePath, content) => ({
        filePath,
        content: content.replace(
          /from\s+['"](\.[^'"]+)['"]/g,
          (_, p) => p.endsWith('.js') ? `from '${p}'` : `from '${p}.js'`,
        ),
      }),
    }),
    bundleDtsPlugin(),
  ],
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
    rollupOptions: {
      external: ["react", "react-dom", "react-dom/client", "react/jsx-runtime"],
    },
  },
  worker: {
    plugins: () => [wasm()],
    format: "es",
  },
});
