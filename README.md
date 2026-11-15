# Grid

Grid is a personal Windows/macOS spreadsheet application maintained in
[JeongEEE/gridcraft](https://github.com/JeongEEE/gridcraft), based on
[GridCraft by the ArtCraft team](https://github.com/storytold/gridcraft).

Rust workspace package names and engine commands remain compatible with upstream.
The installed app, preferences and update feed are independent of GridCraft.

- `main`: unmodified upstream history; synchronize from `upstream/main`.
- `my-custom`: development and personal changes.
- `my-custom-release`: builds Windows x64 and universal macOS packages, then prepares a draft release.
- [Build, release and update guide](docs/custom-release.md).
- [Upstream architecture](docs/architecture.md) and [known limitations](docs/gaps.md).

## Build locally

Install Rust 1.95 or later and the platform's native build tools, then run:

```sh
cargo build --release --locked -p gridcraft -p gridcraft-cli
cargo run --release -p gridcraft -- --sample sales
cargo xtask ci
```

macOS packages: `packaging/macos/package.sh --arch universal`.
Windows packages: `pwsh packaging/windows/package.ps1 -Arch x64` (WiX 5.0.2).

## Downloads and updates

[Published releases](https://github.com/JeongEEE/gridcraft/releases) contain the installers
when a reviewed draft is published. Drafts are visible only to repository collaborators.
Grid checks this fork's published releases; it never installs upstream GridCraft releases.
On both platforms Grid automatically checks for new versions and offers a download link.
These unsigned/ad-hoc builds do not install updates or restart themselves.

## License and attribution

Code is available under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your choice.
Copyright (c) 2026 ArtCraft Team and the GridCraft contributors. See [NOTICE](NOTICE)
and [ATTRIBUTION.md](ATTRIBUTION.md) for third-party notices and the original app icon.
ArtCraft logos and wordmarks have been removed from this modified distribution.
Grid is an independent personal fork and is not an official ArtCraft product.
