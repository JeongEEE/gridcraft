# Grid numeric display auto fit implementation plan

> Execute inline with `superpowers:executing-plans`; the owner requests no subagents.

**Goal:** Keep overflowing numbers visible inside their columns without editing the workbook.
**Spec:** `docs/superpowers/specs/2026-11-15-grid-number-auto-fit-design.md`
**Architecture:** Persist a UI preference, pass it into both grid painting paths and reuse font
shrinking before accounting-fill expansion. Engine/model/export remain unchanged.
**Tech stack:** Rust, egui, existing kittest and serde; no new dependencies.

## Constraints and review focus

Work on `my-custom`, leave `main` and `my-custom-release` alone. No release or app replacement.
Review General pre-rounding, accounting fill, disabled-policy compatibility, explicit shrink,
invalid date hashes and non-mutating preference persistence. Include merged cells, zoom and
Page Layout, number-format colours and ordinary text overflow.

## Task: Numeric fit preference and rendering

- [x] Add `tests/it/number_auto_fit.rs` and register it: render full General/decimal/accounting
  numbers within the cell by default; off restores hashes; round-trip preference; no workbook
  mutation; preserve text overflow, colours and invalid dates; test merged/zoom/Page Layout.
- [x] Watch the rendering tests fail before implementing. Use existing tests for disabled-policy
  accounting/shrink controls and add an explicit accounting-shrink regression.
- [x] Add `UiState.auto_fit_numbers`, `options.autoFitNumbers`, File > Options checkbox and Korean
  labels; pass the preference via `QuadrantPaint` in normal and Page Layout rendering.
- [x] Adjust `display_text` General handling and `paint_cell` shrink selection; accounting repeat
  fill must use the final font. Keep non-number layout and export semantics intact.
- [x] Run regression and existing relevant render tests, the full workspace suite and fmt/clippy.
  Recheck all 18 real-workbook cases without column changes and original-file checksum.
- [x] Review the diff inline; update the existing Grid wiki, sync, and commit/push owned paths.

## Verification and review

- RED: full comma/General/accounting numbers were absent, the preference and command did not
  exist. The text overflow control already passed. Separate regressions exposed clipped partial
  amounts at the minimum font size and accounting fill consuming indentation space.
- GREEN: all 10 numeric-fit regressions pass, including 18 format/view/zoom combinations, Korean
  checkbox interaction, persistence, legacy display, explicit accounting shrink and no document
  mutation. Existing hash-display tests explicitly disable the Grid-only preference so their
  original compatibility assertions remain exercised.
- Final workspace run: 3,922 passed, 1 failed, 6 ignored. Only the already documented baseline
  `caret_after_typing_arabic_is_drawn_at_its_left_end` fails. Do not describe the full suite as green.
- All 18 original overflow cases were privately recreated with the workbook's values/styles/widths
  and the Mac's production font set. The complete formatted amount fits without widening or
  changing the workbook; switching the preference off reproduces the original hashes. The original
  XLSX checksum remains unchanged. No private fixture or QA-only source is committed.
- Native GUI/CLI development builds passed. fmt, all-target clippy with warnings denied,
  doctests, i18n report, assets, layers, WASM, parity
  and keytips passed. Clippy initially identified excessive complexity in `paint_cell`; extracting
  `NumberFit::new` resolved it without weakening the lint. The full suite and private QA were rerun
  after this extraction.
- Reviewed inline, without subagents: both on-screen painting paths receive the preference;
  sign/colour/fill layout remain intact; engine, export, workbook data and upstream identity are
  untouched. The existing Grid maintenance wiki documents UI-only versus document-level shrink.
  No new dependencies, release, release-branch update or installed-app replacement.
