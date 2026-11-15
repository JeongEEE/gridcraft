# Grid 자동 업데이트 구현 계획

> **For agentic workers:** Use superpowers:executing-plans. 사용자 요청대로 직접 구현한다.

**Goal:** Windows/macOS에서 인증된 포크 업데이트를 다운로드하고 저장 확인 후 재시작 설치한다.
**Architecture:** `gridcraft-update`가 신뢰 경계와 helper를 소유하고 ui-egui가 표시와 저장 확인을 소유한다. 기존 HTTP/릴리스 파서/프로세스 도구를 재사용한다.
**Tech Stack:** Rust 1.95, ring Ed25519 (Apache-2.0 AND ISC), craft-update, zip, serde.
**Spec:** docs/superpowers/specs/2026-11-15-grid-auto-update-design.md

## Global Constraints

- repo JeongEEE/gridcraft; bundle io.github.jeongeee.grid; 앱 Grid; main 수정 금지.
- 공개 발행 금지. 개인키 소스/로그 출력 금지. 저장 취소는 설치 취소.
- 원본 OS 서명 정책을 수정하지 않고 독립된 포크 서명을 검증한다.

## Review Focus

- 악성 manifest와 ZIP 경로: 서명·동일 태그·크기·경로 검증 테스트.
- 저장 취소 후 나중에 종료: 명시 예약 해제 회귀 테스트.
- 설치 경로/문맥 충돌: 모호하면 거부, 기존 위치 보존 테스트.
- 부모가 아직 실행 중: timeout 시 파일 변경 금지 테스트.
- 교체 도중 실패: 이전 파일 복원, 실패 기록과 재실행 테스트.

### Task 1: 인증·설치 라이브러리와 서명 도구

Files: `crates/update/`, workspace Cargo.toml/Cargo.lock, xtask/src/layers.rs.
인증과 플랫폼 설치가 같은 helper 경계를 이루므로 독립적으로 검증 가능한 하나의 라이브러리 단위로 정리했다. 서명 도구는 이 경계의 실물 입력을 생성한다.
- [x] 서명·다른 repo·태그·버전·크기·해시·ZIP 경로·교체 실패·부모 종료 회귀 테스트.
- [x] `verify_manifest`·`verify_package`, `Prepared::download/launch`, helper와 플랫폼 트랜잭션 구현.
- [x] signer/keygen 도구와 private key 보호, 공개키 누락 시 fail closed.
- [x] 로컬 테스트·clippy·WASM, Mac 실제 DMG 교체·실패 복원 검증. Windows 실행은 CI에서 검증.
- [x] 라이브러리 범위 검토 완료. Task별 커밋·푸시.

### Task 2: 앱 알림과 저장 확인

Files: apps/gridcraft/{Cargo.toml,src/main.rs}, crates/ui-egui/{Cargo.toml,src/lib.rs,src/updates_ui.rs,src/app/quit.rs}, Cargo.lock.
- [x] 저장 취소 회귀 RED→GREEN. 서명 검증 제거/저장 취소 처리 제거 mutation이 각각 실패하는지도 확인.
- [x] 자동 다운로드·진행률·준비 알림·6시간 확인·on_exit 명시 예약 연결.
- [x] 일반 종료/저장 취소/매크로 취소 후에는 설치 예약 해제. 첫 UI 프레임에서 helper에 시작 확인.
- [x] 알림 화면 확인 및 저장 취소·나중에 4개 테스트 통과. Task별 커밋·푸시.

### Task 3: 서명 배포와 유지보수

Files: .github/workflows/release-custom.yml, packaging/windows/test-auto-update.ps1, docs/custom-release.md, 기존 Grid wiki.
- [x] CI에 Mac 실제 bundle 교체·복원, Windows 실제 MSI major upgrade·문맥 유지·포터블 교체 추가.
- [x] 패키지 생성 후 manifest 서명, secret 누락 시 draft 발행 작업 실패. 공개 릴리스 자동 발행 금지.
- [x] 전체 품질 검사와 직접 diff 리뷰. 전체 테스트 3900 통과, 원본 Arabic caret 한 건 실패; 나머지 게이트 별도 실행.
- [x] 기존 위키에 구조·서명키·업데이트 범위 반영하고 sync.
- [ ] 범위 커밋·푸시, Windows/macOS CI 실행 결과 확인.
- [x] 사용자 승인 후 `~/Dev/keystores/grid`에 운영 키 생성, Actions secret 등록 및 공개키 고정. 실제 운영 키 서명·검증 통과.
- [ ] 서명된 draft 재빌드·다운로드 검증과 `/Applications/Grid.app` 설치. 공개 발행은 별도.
