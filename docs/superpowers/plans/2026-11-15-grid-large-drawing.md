# Large DrawingML repair implementation plan

> Execute inline with `superpowers:executing-plans`; the owner prefers no subagents.

**Goal:** Open, edit, save and reopen the supplied workbook without dropping its pictures.
**Spec:** `docs/superpowers/specs/2026-11-15-grid-large-drawing-design.md`
**Architecture:** Reuse quick-xml streaming and shared model media; preserve bounded imports
and context-local texture caches. No new dependencies, no user-file edits, no main changes.

## Task: Repair large drawing import and display

- [x] Add `crates/xlsx/src/tests/large_drawing.rs`, register in `tests.rs`, and demonstrate
  failure for a synthetic 37,000-picture drawing and repeated media allocations.
- [x] Add a failing shared-picture texture regression in `floating_pictures.rs` covering
  repeated object ids and replacement with equal-length, different image bytes.
- [x] Change `drawing::read_drawing` to stream anchors into bounded, atomic import,
  cache media and index relationship ids. Keep `xml::MAX_XML_NODES` unchanged.
- [x] Change the floating-picture cache to share textures by pinned media allocation.
  Preserve closed-document cleanup, context isolation, decode and cache size bounds.
- [x] Verify large-drawing edit/save/reopen, a malformed trailing element, a single oversized
  anchor, too many anchors and independent documents/pictures. Run existing format/UI tests.
- [x] Build the native GUI/CLI and inspect the real file via copies and the control API;
  verify cell edits, undo, save/reopen and 37,000 retained pictures. Never overwrite original.
- [x] Run repository quality checks, inspect the final diff and record any pre-existing
  failures honestly. Update file-format docs and existing Grid wiki maintenance reference;
  sync the wiki, then commit/push only owned paths to `my-custom` and the wiki's branch.

## Verification results

- Regression tests first reproduced the node-limit rejection, separate media allocations,
  repeated GPU textures and missing GIF decoding; all pass with this repair.
- Real workbook: 1,515 original cells preserved, one Korean edit saved and reopened,
  37,000 pictures retained with identical media/order/alt text. Original row/column data,
  merges, print settings, conditional formatting and validation remain unchanged in the
  imported model after save. One picture coordinate differs by 0.000001 px, within one EMU.
- Native macOS GUI: inspected open and saved screenshots; no open warning; Korean cell
  editing, undo/redo, save and reopen passed. Original file checksum unchanged.
- Workspace: 3,912 passed, 1 failed, 6 ignored. The only failure is the previously documented
  `caret_after_typing_arabic_is_drawn_at_its_left_end` (`arabic_editing.rs:70`).
- fmt, all-target clippy, doctests, assets, layers, WASM, parity and keytips passed.
- Reviewed inline without subagents, following the owner's preference. No new release or
  replacement of the installed app is part of this source repair.
