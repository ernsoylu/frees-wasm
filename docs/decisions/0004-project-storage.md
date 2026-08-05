# D4 — Browser-resident project storage

**Status:** Decided · **Date:** 2026-08-05

## Decision

The Phase 11 project library is **IndexedDB**, not OPFS. Projects are stored
**keyed by name** in a `projects` object store; the workspace autosave gains a
**durable IndexedDB mirror** alongside the existing `localStorage` copy, which
stays exactly where it is and remains what the app boots from.

## Context

`PLAN.md`'s Phase 11 row says "OPFS/IndexedDB project storage" and leaves the
choice open. What exists today (`web/src/project.ts`, Story 10.10) is a single
unified `.frees` JSON document autosaved into one `localStorage` key
(`frees.project`) plus save-to-disk via the File System Access API. Two things
are wrong with it as a product:

1. **There is one slot.** A user has one implicit document; opening anything
   else means overwriting it or juggling `.frees` files by hand.
2. **`localStorage` is a ~5 MB quota**, and the project document can carry
   Excalidraw scenes with embedded images, Univer spreadsheets and digitizer
   image data-URLs. Several modules already broadcast dedicated
   `QuotaExceededError` events. When the autosave write fails it fails
   silently (by design — autosave must never interrupt), which means the
   *durable* copy of the workspace can silently stop updating.

### Why IndexedDB over OPFS

| | IndexedDB | OPFS |
|---|---|---|
| What we store | one JSON document per project | same |
| Support | universal, back to browsers older than this app supports | newer; Safari/Firefox support is recent and uneven |
| Transactionality | transactions built in | none — last write wins per file |
| Sync access | async everywhere (fine — the library is used post-boot) | fast sync handles exist **only in workers**, which is the one place this data is not needed |
| Testability | `fake-indexeddb` in vitest, established | nothing jsdom-shaped exists |
| What it's actually for | structured records | large binary streams (which measurement bytes would be — but those deliberately never persist, "template mode") |

OPFS's advantages are all about big binary files and synchronous worker-side
access. The project document is a small-to-medium JSON record read and written
on the main thread. Every axis that matters here favours IndexedDB.

### Why the boot path does not change

`App.tsx` reads the autosave **synchronously** (`bootRef.current =
loadProjectLocal()`) before every `useState` initializer. Making boot async —
suspense, a loading gate, or effect-driven state resets — is the single most
invasive change Phase 11 could make to a file the port otherwise does not own,
for zero user-visible gain on the happy path. So:

* `localStorage` remains the **boot cache**: written best-effort on the same
  debounce as today, read synchronously at boot, exactly as before.
* IndexedDB holds the **durable mirror**: the same debounced write also lands
  there (fire-and-forget). A post-boot effect compares the two `savedAt`
  stamps; only when the mirror is **strictly newer** — which happens precisely
  when the `localStorage` write started failing on quota — does the app offer
  to restore it. Same-tick writes share one `buildProject()` output and thus
  one `savedAt`, so the offer can never fire spuriously.

### Why the library is keyed by name

The app already has exactly one naming concept: `projectName`, shown in the
Project menu as `{name}.frees` and used as the save-picker suggestion. Keying
browser-resident projects by that same name means "Save to browser" has file
semantics (same name overwrites, as `showSaveFilePicker` does), the list UI
needs no identity column the user has never seen, and rename is a re-key. No
UUIDs, no reconciliation between an id-keyed store and a name-keyed UI.

## Consequences

* New module `web/src/projectStore.ts`; `project.ts` keeps ownership of the
  document shape and gains one export (`normalizeStoredProject`) so both
  storage backends share the same sanitize/migrate trust boundary
  (tssecurity:S8475 applies to IndexedDB reads exactly as to `localStorage`).
* Zero new runtime dependencies — the IDB wrapper is ~40 lines of
  promise-wrapping, not worth a crate-equivalent. `fake-indexeddb` is added as
  a **dev** dependency for vitest.
* Measurement samples stay out of storage entirely (the existing "template
  mode" rule); the library stores what a `.frees` file stores, nothing more.
* If IndexedDB is unavailable (private modes, storage-partitioned iframes),
  the library degrades to absent and the app behaves exactly as it does today
  — the `localStorage` path is untouched.
* **Revisit if** projects routinely exceed what structured-clone handles
  comfortably (~tens of MB) — that is the point at which OPFS per-project
  files become the right shape, and the `projectStore` API is async
  specifically so the backend can be swapped without touching callers.
