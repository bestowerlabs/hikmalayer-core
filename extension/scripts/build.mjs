// Builds each extension entry as a SELF-CONTAINED bundle.
//
// MV3 constrains us: content scripts cannot be ES modules and cannot resolve
// bare specifiers, so nothing may be code-split across entries. We therefore
// run one Vite build per entry with inlined dynamic imports.
import { build } from "vite";
import { cp, mkdir, rm } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const outDir = resolve(root, "dist");

const ENTRIES = [
  // The service worker is declared "type": "module" in the manifest.
  { name: "background", file: "src/background.js", format: "es" },
  // Content scripts must be classic scripts.
  { name: "content", file: "src/content.js", format: "iife" },
  { name: "inpage", file: "src/inpage.js", format: "iife" },
  // Popup script, loaded by popup.html.
  { name: "popup", file: "src/popup.js", format: "iife" },
];

await rm(outDir, { recursive: true, force: true });
await mkdir(outDir, { recursive: true });

for (const entry of ENTRIES) {
  await build({
    root,
    configFile: false,
    logLevel: "warn",
    build: {
      outDir,
      emptyOutDir: false,
      target: "chrome111",
      minify: false, // reviewable output: a wallet should be auditable
      lib: {
        entry: resolve(root, entry.file),
        formats: [entry.format],
        fileName: () => `${entry.name}.js`,
        name: `hikmalayer_${entry.name}`,
      },
      rollupOptions: {
        output: { inlineDynamicImports: true },
      },
    },
  });
  console.log(`built ${entry.name}.js`);
}

// Static assets: manifest, popup shell, icon.
await cp(resolve(root, "public"), outDir, { recursive: true });
console.log("copied static assets → dist/");
