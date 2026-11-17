# Grid deployment script implementation plan

> **For agentic workers:** Use superpowers:executing-plans for this single task.

**Goal:** Provide `./deploy.sh [version]` with remote build progress and one final publication confirmation.

**Architecture:** A Bash entrypoint launches Python orchestration using git, cargo xtask, and gh. Reuse the existing release workflow unchanged; own only local preparation, exact-build tracking, draft validation and publication consent.

**Tech Stack:** Bash, Python 3.11+ stdlib, git, Rust/Cargo, GitHub CLI.

**Spec:** `docs/superpowers/specs/2026-11-17-deploy-script-design.md`

## Global constraints

- Develop on `my-custom`; fork `JeongEEE/gridcraft`, release branch `my-custom-release`, tag `gridcraft-vX.Y.Z`.
- Stage only Cargo.toml and Cargo.lock during release preparation; refuse tracked/index changes. Untracked files remain untouched and excluded.
- No workflow edits, dependencies, force pushes, or real release execution during this task.
- `--dry-run` makes no writes or network requests. Only explicit final confirmation publishes.

## Review focus

- Existing staged/unstaged source, wrong branches/remotes, and ahead/diverged remote release history must stop before version mutation.
- Published versions, authentication/network errors, duplicate workflows and stale drafts must never be mistaken for a new successful release.
- Track the exact source SHA and verify all seven release assets before offering publication.
- Cancellation/failure must preserve useful work and allow retry without another version bump.
- No false percentage progress for remote builds; show real GitHub job state and failure URL.

### Task 1: End-to-end deployment command

**Files:** Create `deploy.sh`, `packaging/deploy.py`, `packaging/test_deploy.py`; update `docs/custom-release.md` and existing wiki `wiki/Grid/grid-release-update.md`.

**Interfaces:** CLI `[version] [--dry-run] [--draft]`; subprocess argument arrays only. Existing workflow and version setter interfaces stay unchanged.

- [x] Write integration tests using real temporary git remotes and controlled gh/cargo boundaries for patch/explicit versions, safety guards, exact build tracking, retry, failure, assets, publication consent and dry run.
- [x] Run `python3 packaging/test_deploy.py`; observe RED behavior before implementing.
- [x] Implement local preparation and tracking through final confirmation; add concise Korean stage output and help/recovery commands.
- [x] Run integration tests, Bash syntax, fork contract and required repository gates; distinguish the documented Arabic-caret baseline failure.
- [x] Request read-only review, fix findings with regression evidence, update docs/wiki and sync.
- [x] Commit and push this task's explicit paths after verification. Do not run an actual deployment.

## Validation record

- Deployment integration tests: 15 passed with real isolated git remotes and controlled gh/cargo boundaries. No actual deployment ran.
- Initial missing-command RED evidence: `/tmp/grid-deploy-red.log`; GREEN: `/tmp/grid-deploy-verified-tests.log`.
- Self-review reproduced a concurrent commit slipping into the deployment SHA (`/tmp/grid-deploy-review-red.log`); freezing the version commit and checking its parent fixed it, and the full deployment suite passed.
- Fresh external review was attempted but the reviewer service returned a usage-limit error; completed a separate author review instead. No deferred findings.
- Read-only GitHub API checks against existing release 0.19.5 and run 43776890969 confirmed the actual asset and job/source response fields.
- Bash/Python syntax and fork install contract (3 tests): passed. `--help` and nonmutating `--dry-run` exercised.
- Required `cargo xtask ci`: fmt/clippy passed; stopped at documented baseline `caret_after_typing_arabic_is_drawn_at_its_left_end`.
- Full workspace suite with only that existing approved exclusion: 3,936 passed, 6 ignored, 1 filtered. Doctests, assets, layers, wasm, parity, keytips: passed.
- Existing wiki updated and sync completed. Workflow files, app version and release branch were not changed by this development task.
