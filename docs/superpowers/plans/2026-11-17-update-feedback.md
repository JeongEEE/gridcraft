# Update feedback implementation plan

> **For agentic workers:** Use superpowers:executing-plans for this single task.

**Goal:** Make background updates visible without interrupting spreadsheet work.

**Architecture:** Keep the existing signed updater and quit/save/reopen flow. Render a small nonmodal corner card during download, a themed restart modal when ready, a compact ready button after deferral, and a self-expiring success toast after restart.

**Tech Stack:** Rust, egui, existing theme tokens and egui_kittest.

**Spec:** The user approved the three-stage proposal in this conversation and requested implementation. Success feedback lasts about four seconds; restart requires an explicit click; save confirmation and file restoration remain unchanged.

## Global constraints

- Work on `my-custom`; preserve fork identity, signing, platform installers, and wasm builds.
- Use existing theme tokens and original code-drawn icons; add no dependencies.
- No version bump or release publication in this task.

## Review focus

- Unknown size, reconnects, and complete byte counts must never display misleading percentages.
- Deferring or dismissing the modal must preserve the workbook and offer a way back.
- Modal keyboard input must not edit the workbook underneath it.
- Successful installation must never create a blocking message; failures remain actionable.
- Completion feedback must disappear even without input or a visible sheet grid.

### Task 1: Update feedback lifecycle and UI

**Files:** Modify `crates/ui-egui/src/updates_ui.rs`; create `crates/ui-egui/src/updates_ui/view.rs` for presentation; update `docs/custom-release.md` and the existing wiki `wiki/Grid/grid-release-update.md`. The existing `app.rs` integration remains sufficient.

**Interfaces:** Consume existing `Message`, `Phase`, `Prepared`, and helper `Outcome`. Keep `tick`, `show`, and quit integration signatures. Add private feedback rendering/helpers only as needed.

- [x] Add harness regressions for background progress visibility and deferral/reopening. Add outcome feedback expiry tests and unknown-size/reconnecting coverage.
- [x] Run `cargo test -p gridcraft-ui-egui --lib updates_ui::tests`; observe missing behavior failures.
- [x] Implement corner progress, themed modal, deferred ready indicator, and four-second success toast; preserve failure reporting.
- [x] Run the update regression tests, render light/dark screenshots, and inspect them.
- [x] Run `cargo xtask ci`, the fork packaging contract, and investigate any failures against the known baseline.
- [x] Review the complete diff, document the UI contract, sync the wiki, and commit/push explicit task paths after review.

## Validation record

- New feedback and save/reopen regressions: 15 passed, including real app draft and image-paste safety.
- Light/dark, small-window, and in-app screenshots rendered and inspected.
- Fresh review found draft cancellation and image-paste leaks; both reproduced before fixing and covered by passing regressions. Incomplete percentages are capped at 99% until verification starts.
- `cargo xtask ci`: fmt and clippy passed; stopped on the already documented `caret_after_typing_arabic_is_drawn_at_its_left_end` baseline failure.
- Final workspace suite with only that approved exclusion: 3,936 passed, 6 ignored, 1 filtered; no other failures.
- Final clippy, doctests, assets, layers, wasm, parity and keytips: passed. Fork packaging contract: 3 passed.
- Existing Grid wiki article updated and `sync-omc-wiki.sh` completed. Version and release channels are unchanged.
