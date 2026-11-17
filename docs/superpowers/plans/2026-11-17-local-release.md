# Grid Local Release Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans. Steps use checkbox syntax for tracking.

**Goal:** Mac·Windows 로컬 빌드와 릴리스 업로드를 별도 명령으로 제공하고 Actions 의존성을 없앤다.
**Architecture:** OS별 얇은 진입점과 Python 공통 배포 로직, 네이티브 빌드, 자산 검증 모듈로 나눈다.
두 OS는 같은 SHA의 초안을 공유하며 기존 Rust 서명기는 완성된 파일 묶음만 서명한다.
**Tech Stack:** Python 3.11+, Bash, PowerShell 7+, Rust, git, GitHub CLI.
**Spec:** `docs/superpowers/specs/2026-11-17-local-release-design.md`

## Global Constraints

- `my-custom`에서 작업. 버전·bundle/updater 식별자·기존 서명 키 유지.
- Actions 호출 및 배포 브랜치 푸시 없음. 실제 릴리스 작성·발행은 이번 작업 범위 아님.
- 미추적 작업은 배포 제외. 자신의 파일만 스테이징. 작업 종료 후 커밋·푸시.
- 글꼴 고정 SHA `b076417b5880bd4154bd22f00695b1d28c6bea83` 유지.

## Review Focus

- 다른 OS에서 pull한 두 번째 실행이 같은 버전과 SHA를 사용하는가.
- 불완전한 초안·API 실패·다른 SHA·변조된 자산은 공개할 수 없는가.
- Windows에서 POSIX 전용 import·명령 인용·외부 명령 실패를 올바르게 처리하는가.
- 긴 빌드 동안 소스가 바뀌거나 다른 PC가 초안을 바꾸면 업로드·공개를 중단하는가.
- 로컬 MSI 검증이 사용자의 설치된 Grid를 변경하거나 삭제하지 않는가.

---

### Task 1: OS별 로컬 빌드·초안 업로드·서명 배포

**Files:** Modify `packaging/deploy.py`, `packaging/test_deploy.py`, `deploy.sh`, `AGENTS.md`,
`docs/custom-release.md`; create `deploy-macos.sh`, `deploy-windows.ps1`,
`packaging/local_build.py`, `packaging/release_assets.py`; delete `.github/workflows/release-custom.yml`.
**Interfaces:** `deploy.py main` consumes `--platform macos|windows`, optional version,
`--finalize`, `--draft`, `--dry-run`. `local_build.build(deployment, output)` returns verified file paths.
`release_assets` validates receipts and downloaded payloads and prepares the final file set.

- [x] Write/run failing tests for first/second OS uploads, same-version reuse, published-version bump,
  finalization with valid hashes, missing/wrong source/assets, API and build failure, concurrent source edit,
  dry-run, dirty checkout, EOF, tag mismatch, and no Actions/release-branch requests.
- [x] Replace Actions orchestration with local pipeline; add Bash/PowerShell entrypoints and portable locking.
- [x] Keep same key/manifest schema; add per-OS build receipts and fail closed before publication.
- [x] Remove fork workflow and document Windows setup, two-PC commands, key handling and safe validation limits.
- [x] Run deployment tests, fork contract, PowerShell/Bash entrypoint checks, native Mac packaging and required CI.
- [x] Request fresh read-only review; fix blocking findings with RED→GREEN tests.
- [x] Update existing Grid wiki and sync; commit explicit task paths and push `my-custom` and wiki branch.

## Verification record

- Native local-release integration tests: 19 pass; actual temporary git and file transfers,
  controlled GitHub/compiler boundaries. Includes EOF, two-PC version reuse, source/hash races.
- Fork install contract: 3 pass. Bash/Python syntax pass. Official PowerShell 7.7 parser and
  actual Windows wrapper dry-run pass on Mac; no GitHub calls. Windows native MSVC/MSI build
  is not executable on this Mac and remains a Windows-PC verification step.
- Real Mac local build of frozen SHA `13e269ce` (Rust/packaging payload code unchanged in this task):
  universal DMG/CLI ZIP, strict internal app signature, native update/rollback and CLI save/reopen pass.
  Artifacts under `/tmp/grid-local-macos-smoke` were not uploaded.
- `cargo xtask ci`: fmt/clippy pass; existing `caret_after_typing_arabic_is_drawn_at_its_left_end`
  fails with the documented baseline. Full workspace with only that exclusion: 3936 pass,
  6 ignored, 1 filtered; doctest/assets/layers/wasm/parity/keytips pass.
- Fresh read-only review: APPROVE, no blockers. EOF coverage added. Module-length advisory
  does not represent a project limit; existing separation by responsibility is retained.
- No release created/published, no Actions dispatch or release-branch push in this task.
