# Grid 개인 포크 배포 설계

## 목적과 승인된 요구사항

- 앱 표시 이름은 **Grid**로 한다.
- Windows와 macOS에서 다운로드하여 사용한다.
- `main`에는 개인 수정을 넣지 않고 `upstream/main`을 동기화한다.
- 개인 변경은 `my-custom`에서 개발하고 업스트림 변경을 merge로 반영한다.
- 기존 `JeongEEE/gridcraft` 포크를 계속 사용한다.

## 접근 방식

권장안은 앱의 외부 식별자와 배포 경로만 분리하는 것이다. Rust crate 이름,
내부 모듈, 기존 명령 ID와 `GRIDCRAFT_*` 개발 환경변수는 유지한다.
전체 소스에서 패키지 이름까지 바꾸면 업스트림 변경을 받을 때 충돌 범위가 커진다.
표시 이름만 바꾸면 설정과 업데이트가 원본과 섞이므로 설치 식별자도 분리한다.

## 앱 식별자와 업데이트

- 표시 이름: `Grid`; 유지관리자 표기: `JeongEEE`.
- macOS 번들 ID: `io.github.jeongeee.grid`.
- 사용자 설정은 원본 GridCraft와 별도 경로에 저장한다.
- 업데이트 조회는 `JeongEEE/gridcraft` GitHub Releases만 사용한다.
- 기존 공개 릴리스 확인과 공유 대화상자를 재사용하되, 현재 포크는 두 OS 모두
  알림·다운로드만 제공한다. 공유 macOS 설치기의 원본 개발팀 서명 고정과 Windows의
  미서명 설치 거부를 우회하지 않는다. 포크 서명 정책과 실제 업그레이드 검증은 별도 작업이다.
- Cargo 패키지와 빌드 원본 바이너리는 `gridcraft`/`gridcraft-cli`를 유지한다.
  구현 확인 결과 updater ID와 자산 stem이 결합돼 있어, 포크 ID와 배포 파일 stem은
  `grid`로 분리하고 패키징 단계에서 실행 파일 이름을 바꾼다. macOS 앱 번들은 `Grid.app`이다.
- 원본과 다른 Windows UpgradeCode, component GUID, 레지스트리 키, 설치 폴더와
  바로가기를 사용해 원본 설치를 덮어쓰지 않는다.
- ArtCraft 로고/워드마크를 배포물과 사용자 화면에서 제거하고, 원본 저작권·라이선스와
  GridCraft 기반이라는 텍스트 출처는 유지한다. 기존 MIT 라이선스 앱 아이콘은 재사용한다.

## 빌드와 릴리스

- `my-custom`은 개발용, `my-custom-release`는 검증된 배포용이다.
- 원본 릴리스 workflow는 수정하지 않고 포크 전용 workflow를 추가한다.
- workflow는 포크 저장소에서만 실행하고 Windows/macOS만 빌드한다.
- Windows x64: MSI와 portable ZIP, macOS: Apple Silicon과 Intel universal DMG와 CLI ZIP.
- 기존 플랫폼 패키징 스크립트를 가능한 한 재사용한다.
- 포크 버전은 `0.19.1` 기반에서 별도로 증가시켜 업스트림 버전과 분리 관리한다.
  앱의 실제 버전, 태그, 설치 패키지 버전은 일치시킨다. 정확한 첫 버전은 구현자가 정한다.
- 배포 브랜치 push 또는 수동 실행으로 빌드와 체크섬을 만들고 draft release를 준비한다.
- 공개 발행은 검증 결과와 실제 산출물이 준비된 뒤 사용자에게 확인한다.
- Apple/Windows 서명 자격증명이 없으면 ad-hoc/unsigned로 빌드하며, 유료 계정이나
  인증서를 구매하지 않는다. 다운로드 후 OS의 첫 실행 확인이 필요할 수 있음을 문서화한다.
- 최초 작업에서는 현재 설치된 `/Applications/GridCraft.app`이나 사용자의 문서를 덮어쓰지 않는다.

## 검증

1. 원본 상태에서 macOS release 빌드를 실행해 개발 환경을 확인한다.
2. fork identity, 업데이트 저장소, 원본과의 설치 분리를 회귀 테스트로 확인한다.
3. 저장소가 요구하는 `cargo xtask ci`를 실행하고 실패를 실제 결과대로 기록한다.
4. macOS에서 `Grid.app` 생성, plist, ad-hoc 서명, 버전, CLI 수식 계산과 XLSX 저장·재열기를 확인한다.
5. Windows는 GitHub Actions Windows runner에서 실제 MSI/ZIP을 빌드한다.
6. 설치/업데이트는 원본과 별도 경로에서 확인하며, 검증하지 못한 UI나 플랫폼은 명시한다.

## 범위 밖

계산 기능 변경, 새 테마·새 아이콘 디자인, Linux/모바일 배포, 유료 코드 서명,
업스트림 PR·이슈 등록, 저장소 이름 변경과 기존 앱/설정의 자동 마이그레이션은 포함하지 않는다.

## 구현자에게 맡기는 세부사항

공통 상수의 배치, 테스트 파일 위치, 안전한 캐시 설정, CI action의 고정 revision,
첫 포크 버전의 정확한 번호, 독립 Windows GUID 생성과 포크 전용 문서 구성은
위의 동작과 분리 조건을 만족하는 최소 변경으로 결정한다.
