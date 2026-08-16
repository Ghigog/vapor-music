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
