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
cargo test -p gridcraft-ui-egui --test ime_cell_entry
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

macOS 패키징은 HFS+ DMG를 만든 후 이미지를 읽기 전용으로 마운트해 내부 `Grid.app`의
엄격한 서명 검증까지 수행한다. 앱 스테이징 폴더와 DMG 자체의 서명 검사만으로 대체하지
않는다. 기존 hybrid HFS 생성기는 파일마다 Finder 위치 정보를 추가해 이 검증을 깨뜨린다.
이미지 생성은 일시적인 CI 장치 오류에 대비해 회당 180초, 최대 세 번으로 제한한다.
완성된 파일은 `packaging/macos/check-dmg.sh <image.dmg>`로 다시 검사할 수 있다.

셀 입력 변경 후에는 선택한 셀에서 편집기를 먼저 열지 않고 한글 `한글`을 입력해
첫 음절과 다음 음절이 모두 남는지 확인한다. F2·수식 입력줄, 영문 첫 글자 중복,
Esc 취소와 시트 보호도 함께 검사한다. `ime_cell_entry`는 조합·확정 이벤트뿐 아니라
편집 시작 시 입력기에 조합 취소를 요청하지 않는지도 검사한다.

현재 macOS에서는 `crates/ui-egui/tests/arabic_editing.rs`의
`caret_after_typing_arabic_is_drawn_at_its_left_end`가 원본 main에서도 실패한다.
전체 CI가 통과했다고 보고하지 않으며, 동일 환경의 원본 기준선과 새 회귀를 구분한다.
이 알려진 실패를 이유로 테스트를 약화하거나 다른 실패까지 제외하지 않는다.

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

## 한 명령으로 배포

macOS/Linux에서 저장소 루트의 `./deploy.sh`를 실행한다. Python 3.11 이상, git,
Rust/Cargo, GitHub CLI(`gh`)와 `gh auth login`으로 로그인한 포크 쓰기 권한이 필요하다.
Mac에 Xcode가 있으면 별도 `DEVELOPER_DIR` 설정 없이 버전 도구를 실행할 수 있다.
Windows/macOS 설치 파일은 기존 GitHub Actions에서 빌드하므로 로컬에 Windows 빌드 환경은 필요 없다.

```sh
./deploy.sh                  # 다음 패치 버전 준비 → 빌드 대기 → 마지막 공개 확인
./deploy.sh 0.19.6           # 버전을 직접 지정하거나 중단한 동일 버전 이어서 진행
./deploy.sh --dry-run        # 파일·커밋·원격 변경 및 네트워크 요청 없이 절차 확인
./deploy.sh --draft          # 빌드와 초안 확인까지 진행하고 공개 질문 생략
```

스크립트는 `my-custom`의 커밋된 소스만 사용한다. 스테이징이나 추적 파일의 미완성 변경이
있으면 중단하고, 미추적 파일은 그대로 두며 포함하지 않는다. 버전 변경은 `Cargo.toml`과
`Cargo.lock`만 커밋한다. 개발 브랜치와 배포 브랜치의 원격 변경이 소스에 반영되지 않았다면
자동 병합하지 않는다. 현재 작업 브랜치를 바꾸지 않고 확정한 소스 SHA를 배포 브랜치에
fast-forward로 푸시하므로 실행 중 다른 브랜치로 이동하는 과정이 없다.

터미널에서 정확한 SHA의 Actions 진행 상태를 보여 준다. 두 OS와 draft 작업의 성공,
초안의 소스 커밋·태그 및 필수 자산 7개가 모두 맞아야 공개 질문으로 진행한다.
마지막 질문에서 `y`를 입력해야 실제 공개와 자동 업데이트 배포가 시작된다. `n`·Enter·입력
종료는 초안을 유지한다. 중간 실패나 로컬 대기 취소가 원격 빌드를 자동 취소하지는 않으며,
표시된 동일 버전 명령으로 다시 실행하면 진행 중/성공한 빌드를 이어 확인하거나 실패한
빌드를 새로 실행한다. 버전 준비 도중 실패해 미커밋 변경이 남았다면 먼저 그 변경을 확인한다.
공개된 버전을 덮어쓰거나 태그·브랜치를 강제로 되돌리지 않는다.

오케스트레이터 회귀 테스트는 `python3 packaging/test_deploy.py`로 실행한다. 임시 git
저장소와 통제된 CLI 응답만 사용하며 실제 포크 배포는 실행하지 않는다.

## 버전과 릴리스

`0.19.5`는 업데이트 재시작 시 열려 있던 파일을 다시 열며 마지막 활성 파일을 표시한다.
종료의 저장 확인 중 새로 정한 파일 경로도 반영하고, 취소 시 복원 예약을 폐기한다.
복원 정보는 종료하는 앱이 전달하므로 0.19.5를 설치한 뒤 다음 업데이트부터 적용된다.
0.19.4에서 처음 업데이트할 때는 파일을 한 번 직접 열어야 한다.

기존 `0.19.4`는 좁은 셀의 숫자를 화면에서 자동 축소해 표시한다. 설정은 기본으로 켜지고
파일 → 옵션 → 일반에서 끌 수 있다. 실제 값·열 너비·저장 서식은 유지하며,
문서에 저장되는 **셀에 맞게 축소** 서식과 별개다. 회계 서식·General 숫자, 병합 셀,
확대 배율·페이지 보기와 실제 가계부의 너비 초과 셀 18개를 검증했다.
기존 `0.19.3`의 대량 DrawingML·공유 이미지·GIF 호환성 수정도 포함한다.
소유자의 사용 범위 결정에 따라 위에 명시한 기존 아랍어 커서 테스트 한 건을
배포 차단 조건에서 제외한다. 테스트 자체는 유지하며 다른 실패를 함께 제외하지 않는다.

버전의 정본은 `Cargo.toml`의 `[workspace.package].version`이다. 첫 포크 버전은 `0.19.2`다.
다음 버전은 `cargo run -p xtask --no-default-features -- version set 0.19.5`으로 변경하고
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
macOS와 Windows 빌드가 모두 성공해야 네 패키지와 체크섬·서명된 업데이트 목록을 갖춘 draft를 만든다.

- `grid-<버전>-macos-universal.dmg`: Apple Silicon·Intel용 Grid.app.
- `grid-cli-<버전>-macos-universal.zip`: macOS CLI.
- `grid-<버전>-windows-x64.msi`: Windows 설치 프로그램.
- `grid-<버전>-windows-x64-portable.zip`: 설치 없이 실행하는 Windows 파일.

draft는 저장소 협업자만 볼 수 있다. 실제 산출물과 검증 결과를 확인하고 사용자 승인 후
GitHub Releases에서 발행한다. 이미 공개한 버전을 덮어쓰지 않는다. draft 재실행은 같은
버전의 산출물과 대상 커밋을 갱신한다. 원본 전체 플랫폼 릴리스 workflow는 이 포크에서 사용하지 않는다.

## 자동 업데이트

### 업데이트 상태 표시 (2026-11-17)

백그라운드 확인·다운로드는 오른쪽 아래의 작은 카드로 표시하며 시트 작업을 막지 않는다.
크기를 알면 진행 막대·퍼센트·전송량을 보여 주고, 준비·재연결·다운로드 후 검증은 별도
문구로 표시한다. 서명과 파일 검증이 끝나야 중앙의 **업데이트 준비 완료** 모달을 연다.
기존 대화상자가 있거나 셀·텍스트 상자를 편집 중이면 준비 알림을 잠시 미루며,
**나중에**·Esc·배경 클릭으로 닫은 뒤에는
**업데이트 준비됨** 버튼에서 다시 열 수 있다. 모달의 키 입력은 시트로 전달하지 않는다.

재시작 설치가 성공하면 버전을 포함한 토스트를 오른쪽 위에 약 4초 표시하고 마지막에
사라진다. 클릭이나 확인이 필요 없고 시트가 없는 시작 화면에서도 보인다. 설치 실패는
기존 오류 안내를 유지한다. 이 UI 변경은 다음 포크 릴리스에 포함되며 0.19.5 공개본에는 없다.

### 확인과 설치

시작 시, Help의 업데이트 확인, 실행 중 6시간 주기로 **이 포크의 공개 릴리스만** 확인한다.
draft나 원본 저장소의 릴리스는 설치하지 않는다. `main` 동기화는 설치 앱 업데이트와 별개다.

설치된 Mac 앱, Windows MSI·포터블은 새 버전을 백그라운드 다운로드하고 검증한다.
준비되면 **재시작하여 업데이트 / 나중에**를 표시한다. 재시작은 기존 저장 확인을 거친다.

0.19.5부터 업데이트 재시작 시 열려 있던 파일을 다시 연다(0.19.4에는 미포함).
종료 전에 문서 UID와 절대 경로를 기억하고, 저장 확인 중 새로 저장한 경로를 갱신한다.
종료 확인이 끝난 뒤 로컬 설치 계획의 `reopen` 목록을 도우미에 넘긴다. Mac·Windows MSI·
포터블 모두 파일마다 별도 인자로 실행하며, 마지막으로 보던 파일을 마지막에 열어 활성화한다.
저장 안 함을 선택한 기존 파일은 디스크에 저장된 내용으로 열고, 저장하지 않은 새 문서는
제외한다. 취소하면 복원 목록도 폐기하며 일반 종료에는 적용하지 않는다. 비밀번호는 보관하지
않고 정상 파일 열기의 암호 확인을 따른다. 업데이트 실패 후 기존 앱을 다시 실행할 때도
같은 파일 목록을 전달한다. 복원 코드는 종료하는 쪽에 필요하므로 이 수정 버전을 설치한
뒤의 다음 업데이트부터 동작한다. 0.19.4에서 처음 넘어올 때는 파일을 직접 다시 열어야 한다.

저장 취소·편집 오류·매크로 닫기 취소는 업데이트 종료 예약도 취소한다. 일반 종료만으로는
설치하지 않는다. 나중에를 고른 준비 파일은 현재 실행 중 유지하고, 앱을 종료한 뒤에는
다음 실행에서 다시 확인·다운로드한다.

`crates/update`는 인증·다운로드·플랫폼 설치를, `ui-egui/src/updates_ui.rs`는 알림과
저장 확인 연결을 담당한다. 복사된 실행 파일이 helper로 시작해 부모 종료를 기다리고,
다른 Grid 프로세스가 남아 있으면 설치하지 않는다. 설정과 사용자 문서는 교체 대상이 아니다.

Mac은 DMG를 readonly로 마운트해 Grid 번들 ID·버전·엄격한 codesign을 검사한 후 같은
볼륨에서 번들을 교체한다. Windows MSI는 원래 설치 문맥과 폴더를 유지하며, 모두를 위한
설치는 OS 권한 승인이 필요할 수 있다. 포터블은 ZIP의 정해진 배포 파일만 교체한다.
Mac·포터블은 교체나 첫 화면 시작 실패 시 이전 파일을 복원하고, MSI 설치 실패는 Windows
Installer의 트랜잭션을 따른다. MSI 설치가 이미 완료된 뒤의 새 앱 시작 실패까지 자동으로
이전 MSI 버전을 재설치하는 기능은 없다. 실패 결과는 다음 시작에서 알리고 로그를 남긴다.

DMG에서 직접 실행하거나 App Translocation·패키지 관리자·개발 경로인 경우 자동 교체를
제공하지 않고 설치 링크를 안내한다. Mac은 쓰기 가능한 Applications 폴더로 복사한다.
이전 다운로드 전용 Grid를 설치했다면 updater 포함 버전을 **한 번 수동 설치**해야 한다.

## 업데이트 서명과 배포 키

`GridUpdate.json`은 저장소·태그·버전·플랫폼별 파일명·크기·SHA256을 담는다.
`GridUpdate.sig`는 JSON 원문 전체의 Ed25519 서명이다. 앱과 helper 모두 소스에 고정한
`crates/update/public-key.hex`로 서명과 패키지 해시를 확인한다. URL은 검증한 태그와
파일명으로 포크 주소 아래에서만 만든다. 같은 버전·낮은 버전·누락 서명은 설치하지 않는다.
원본 craft-update 설치기의 제작사 검증은 변경하지 않고 독립된 포크 인증 경로를 사용한다.

**도입 상태:** 운영 Ed25519 키를 생성해 공개키를 소스에 고정하고 개인키를
`GRID_UPDATE_PRIVATE_KEY` Actions secret에 등록했다. 서명된 패키지의 CI 검증과 초안
갱신은 별도 단계다. 키 설정만으로 이전 draft의 설치 파일이 바뀌지는 않는다.

개인키 정본은 저장소 밖 `~/Dev/keystores/grid/update-private-key.hex`에 보관한다.
디렉터리 권한은 `700`, 개인키 권한은 `600`으로 제한한다. 저장소를 지우거나 다시 클론해도
키를 유지하며 git에는 공개키만 커밋한다. 키 생성·등록은 소유자가 승인한 최초 한 번만 한다.
개인키 값은 명령 인자·출력·git에 넣지 않는다. 개인키를 잃으면 기존 설치본의 신뢰키를
바꿀 수 없으므로 소유자가 별도 안전한 백업을 보관해야 한다. 단순 키 교체는 자동으로
전파되지 않으며, 회전은 기존 키로 인증되는 전환 설계가 필요하다.

```sh
umask 077
mkdir -m 700 "$HOME/Dev/keystores/grid"
cargo run --locked -p gridcraft-update --bin grid-update-sign -- keygen \
  "$HOME/Dev/keystores/grid/update-private-key.hex" \
  "$HOME/Dev/keystores/grid/update-public-key.hex"
cp "$HOME/Dev/keystores/grid/update-public-key.hex" crates/update/public-key.hex
gh secret set GRID_UPDATE_PRIVATE_KEY --repo JeongEEE/gridcraft \
  < "$HOME/Dev/keystores/grid/update-private-key.hex"
```

공개키는 앱 빌드 **이전**에 커밋하고 signing secret과 일치하는지 확인한다. 배포 workflow는
모든 패키지를 만든 뒤 전용 도구로 manifest를 서명한다. 개발 테스트용 결정적 키를
운영 키로 쓰지 않는다. 공개키를 환경변수로 덮어쓰는 런타임 우회는 제공하지 않는다.

업데이트 서명은 Apple Developer ID·공증이나 Windows Authenticode와 별개다.
현재 Mac은 ad-hoc·미공증, Windows는 Authenticode 미서명이므로 OS 실행 확인이 필요할 수 있다.
OS 보안을 해제하거나 유료 계정·인증서를 자동 구매하지 않는다.

검증에는 `cargo test -p gridcraft-update`, UI 저장 취소 테스트, 그리고 패키징 후 native
bundle/MSI/portable 시험을 사용한다. Windows MSI 시험은 기존 버전·제품 ID가 다른
테스트용 MSI를 별도로 만들어 실제 major upgrade와 단일 등록·문맥 유지까지 확인한다.
실제 사용자 설치나 원본 앱을 실험 대상으로 사용하지 않는다.

## 저작권과 문서

ArtCraft 로고와 워드마크는 배포물에서 제거했다. GridCraft 기반이라는 텍스트 출처와
원본 저작권은 보존한다. 기존 앱 아이콘의 출처는 `ATTRIBUTION.md`에 있고, 앱·CLI·MSI/ZIP에
MIT/Apache 라이선스와 필요한 고지를 동봉한다.

프로젝트의 구조와 유지보수 규약은 `MyProjectsWiki/wiki/Grid/`가 사용자용 정본이다.
위키를 수정한 뒤에는 위키 저장소의 `./sync-omc-wiki.sh`를 실행한다.
