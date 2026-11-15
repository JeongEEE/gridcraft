# Grid 포크 개발·배포

Grid는 `JeongEEE/gridcraft`에서 관리하는 GridCraft 기반 개인 스프레드시트다.
사용자에게 보이는 앱 이름은 **Grid**, Cargo 패키지는 `gridcraft`와 `gridcraft-cli`다.
원본의 계산 엔진과 명령 ID를 유지해 업스트림 변경을 계속 받아들인다.

## 브랜치와 원격

| 대상 | 역할 |
|---|---|
| `upstream` | `https://github.com/storytold/gridcraft.git` |
| `origin` | `https://github.com/JeongEEE/gridcraft.git` |
| `main` | 개인 커밋 없이 `upstream/main`만 동기화 |
| `my-custom` | 개인 변경 개발·검증·커밋 |
| `my-custom-release` | 검증한 개발 버전을 배포하는 브랜치 |

업스트림을 반영하는 순서는 다음과 같다. 충돌은 `my-custom`에서 해결한다.

```sh
git fetch --no-tags upstream
git switch main
git merge --ff-only upstream/main
git push origin main
git switch my-custom
git merge main
cargo xtask ci
```

커스텀을 `main`에 역병합하지 않는다. 공개한 커스텀 브랜치는 merge로 갱신하며
일반적인 유지보수에 force push나 rebase를 사용하지 않는다.

## 빌드와 검증

Rust 1.95 이상, rustfmt, clippy, `wasm32-unknown-unknown` target이 필요하다.
macOS universal 패키지에는 `x86_64-apple-darwin` target도 설치한다.

```sh
rustup component add rustfmt clippy
rustup target add wasm32-unknown-unknown x86_64-apple-darwin
cargo build --release --locked -p gridcraft -p gridcraft-cli
cargo test -p gridcraft-ui-egui --test fork_identity
python3 packaging/test_fork_contract.py
cargo xtask ci
```

macOS에는 Xcode 또는 Command Line Tools가 필요하다. 개발 도구 경로가 불완전한
Command Line Tools를 가리키지만 Xcode가 설치돼 있다면, 전역 설정을 바꾸지 않고
빌드 명령에 `DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer`를 지정할 수 있다.
Rust가 PATH에 없다면 먼저 `. "$HOME/.cargo/env"`를 실행한다.

릴리스 workflow는 원본과 같은 `storytold/craft-fonts`의 고정 커밋
`b076417b5880bd4154bd22f00695b1d28c6bea83`을 별도 체크아웃해 글꼴과 OFL 고지를 동봉한다.
다국어 UI 테스트도 이 글꼴 입력을 사용해야 한다. 로컬에서는 해당 체크아웃의 절대 경로를
`CRAFT_FONTS_DIR`로 지정하고 `CRAFT_FONTS_REQUIRED=1`로 누락을 검사한다.

macOS 패키징은 `packaging/macos/package.sh --arch universal`, Windows는 MSVC 도구와
WiX 5.0.2를 준비한 뒤 `pwsh packaging/windows/package.ps1 -Arch x64`로 실행한다.
Apple Silicon만 빠르게 확인하려면 macOS의 `--arch aarch64`를 사용한다.

## 원본과 분리한 설치·설정

| 항목 | Grid |
|---|---|
| updater ID / 배포 파일 stem | `grid` |
| 업데이트 저장소 | `JeongEEE/gridcraft` |
| macOS 앱 / bundle ID | `Grid.app` / `io.github.jeongeee.grid` |
| macOS 설정 | `~/Library/Application Support/Grid` |
| Windows 기본 설치 | `%LOCALAPPDATA%\Programs\JeongEEE\Grid` |
| Windows 설정 | `%APPDATA%\Grid` |
| Windows registry | `Software\Grid`, `Grid.Workbook`, `Grid.TextData` |
| Windows UpgradeCode | `D0FEAEC3-C655-56AA-936E-56F8A764065E` |
| Windows 실행 파일 | `grid.exe`, `grid-cli.exe` |

설치 경로·MSI GUID·파일 연결은 원본 GridCraft와 다르다. 파일 연결은 Open With 후보만
등록하고 기존 기본 앱은 바꾸지 않는다. 설정을 원본에서 자동으로 복사하거나 삭제하지 않는다.
Windows UpgradeCode와 이미 배포한 component GUID는 이후 릴리스에서도 유지한다.

## 버전과 릴리스

버전의 정본은 `Cargo.toml`의 `[workspace.package].version`이다. 첫 포크 버전은 `0.19.2`다.
다음 버전은 `cargo run -p xtask --no-default-features -- version set 0.19.3`으로 변경하고
`Cargo.toml`과 `Cargo.lock`을 함께 커밋한다. 업스트림 병합 시 버전 충돌은 포크 버전을
유지하는 방향으로 해결하되, 다음 배포 번호는 이미 공개한 번호보다 높아야 한다.

태그는 **`gridcraft-v<버전>`** 형식이다. 원본의 `v<버전>` 태그와 충돌하지 않으며,
기존 updater가 저장소 이름 접두사가 붙은 버전을 해석한다. 저장소 이름 변경은 이 규약과
업데이트 주소를 함께 바꾸는 별도 작업이다.

배포 준비가 끝나면 `my-custom-release`에 개발 브랜치를 반영하고 푸시한다.
최초 한 번만 `git switch -c my-custom-release my-custom`으로 배포 브랜치를 만든다.

```sh
git switch my-custom-release
git merge --ff-only my-custom
git push origin my-custom-release
git switch my-custom
```

`.github/workflows/release-custom.yml`은 이 포크의 배포 브랜치에서만 실행된다.
macOS와 Windows 빌드가 모두 성공해야 네 파일과 `SHA256SUMS.txt`를 갖춘 draft를 만든다.

- `grid-<버전>-macos-universal.dmg`: Apple Silicon·Intel용 Grid.app.
- `grid-cli-<버전>-macos-universal.zip`: macOS CLI.
- `grid-<버전>-windows-x64.msi`: Windows 설치 프로그램.
- `grid-<버전>-windows-x64-portable.zip`: 설치 없이 실행하는 Windows 파일.

draft는 저장소 협업자만 볼 수 있다. 실제 산출물과 검증 결과를 확인하고 사용자 승인 후
GitHub Releases에서 발행한다. 이미 공개한 버전을 덮어쓰지 않는다. draft 재실행은 같은
버전의 산출물과 대상 커밋을 갱신한다. 원본 전체 플랫폼 릴리스 workflow는 이 포크에서 사용하지 않는다.

## 업데이트와 서명 제약

앱 시작 때와 Help의 업데이트 확인에서 **이 포크의 공개 릴리스만** 조회한다.
draft는 업데이트로 안내하지 않는다. 소스 `main` 동기화는 앱 업데이트와 별개이며,
개인 변경을 포함한 새 설치 파일을 배포해야 설치된 앱도 갱신된다.

현재 포크는 두 운영체제 모두 **알림·다운로드 전용**이다. 자동으로 새 버전을 확인하지만
사용자가 새 설치 파일을 내려받아 설치한다. 앱이 직접 교체되거나 재시작하지 않는다.

macOS 공유 설치기는 원본 ArtCraft 개발팀의 Apple 서명을 고정 검증하므로 이 포크의
ad-hoc 앱은 그 자동 설치 경로를 사용할 수 없다. Windows도 미서명 파일의 자동 설치를
막는다. 원본 검증을 우회하지 않으며, 포크용 자동 설치는 별도 서명 정책과 실제
구버전→신버전 시험을 갖춘 뒤 도입한다. 인증서를 추가하는 것만으로 활성화되는 기능은 아니다.

macOS는 ad-hoc 서명·미공증 배포이므로 다운로드 후 OS의 실행 확인이 필요할 수 있다.
Windows에도 미서명 앱 확인이 나타날 수 있다. 비밀키를 소스에 넣거나 유료 계정·인증서를
자동 구매하지 않는다. 기존 원본 앱과 문서는 설치·업데이트 실험 대상으로 삼지 않는다.

## 저작권과 문서

ArtCraft 로고와 워드마크는 배포물에서 제거했다. GridCraft 기반이라는 텍스트 출처와
원본 저작권은 보존한다. 기존 앱 아이콘의 출처는 `ATTRIBUTION.md`에 있고, 앱·CLI·MSI/ZIP에
MIT/Apache 라이선스와 필요한 고지를 동봉한다.

프로젝트의 구조와 유지보수 규약은 `MyProjectsWiki/wiki/Grid/`가 사용자용 정본이다.
위키를 수정한 뒤에는 위키 저장소의 `./sync-omc-wiki.sh`를 실행한다.
