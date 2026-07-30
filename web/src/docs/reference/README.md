# frees Reference Documentation (`src/docs/reference/`)

Per-symbol reference pages — one markdown file per built-in function,
procedure, block construct, or component. This directory is the source the Phase-1
pipeline compiles into the in-app Help → Reference section.

## Files

- `_TEMPLATE.md` — the page contract. Copy it to start a new page. Do **not** edit it as content (the leading `_` keeps it out of the manifest/compile).
- `function-manifest.json` — **generated**, do not hand-edit. The machine-readable inventory of the entire documentable surface across `functions`, `matrixFunctions`, `callProcedures`, `propertyFunctions`, `materials`, `components`, and `replCasOps` — each with a `documented` flag, reconciled against the backend by `frontend/scripts/build-doc-manifest.mjs`. The authoritative count is `coverage.documentableSurfaceTotal` (**475 unique symbols**, deduplicated across families); regenerate with `node scripts/build-doc-manifest.mjs`.
- `<category>/<name>.md` — authored reference pages, grouped by category folder (e.g. `heat-transfer/hx_effectiveness.md`).

## The contract (every page)

1. **YAML frontmatter** — `name`, `category`, `summary`, `related`, `examples` (ids in `examples.ts`), `tags`, `references`. Feeds search + the manifest coverage gate.
2. **Mathematical Formulation** — governing equations in KaTeX, plus the numerical method the backend actually uses. **Content is grounded in the standard literature**, never model memory.
3. **Examples** — bound to backend-verified entries in `examples.ts` via `[Run: id]`; progressive and multi-domain.
4. **Common Errors** — error-code table with a triggering snippet.
5. **References** — optional; cite only public standards and data sources (standard + section/equation).

## Quality gates (CI, Phase 1+)

- Every registry function (`function-manifest.json → functions[]`) has a page (`documented: true`).
- No page names a symbol absent from the backend.
- Every `examples:` id resolves and solves clean against the backend.
- Every formula page has a Mathematical Formulation section.
