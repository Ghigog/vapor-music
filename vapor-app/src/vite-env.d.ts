/// <reference types="vite/client" />

/**
 * Markdown imported verbatim.
 *
 * `vite/client` declares `*?raw` already, but only for modules Vite resolves;
 * TypeScript still needs to be told what the import evaluates to. The Vibe help
 * sheet imports `docs/ai_dj_workflow.md?raw` so the help and the spec are the
 * same file — see `src/components/HelpModal.tsx`.
 */
declare module "*.md?raw" {
  const content: string;
  export default content;
}

/**
 * Stamped in by `scripts/build-stamp.mjs` through Vite's `define`, so the
 * About screen can name the build a bug report came from. There is no
 * telemetry, so this is the only way to tie a report to a tree.
 */
declare const __APP_VERSION__: string;
declare const __APP_COMMIT__: string;
