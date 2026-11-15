//! The fork's update feed and install identity must never target the upstream application.
#![cfg(not(target_arch = "wasm32"))]

use gridcraft_ui_egui::UPDATE_APP;

#[test]
fn fork_updates_only_from_its_own_repository() {
    assert_eq!(UPDATE_APP.repo, "JeongEEE/gridcraft");
    assert_eq!(UPDATE_APP.releases_url(), "https://github.com/JeongEEE/gridcraft/releases");
    assert_eq!(UPDATE_APP.id, "grid");
    assert_eq!(UPDATE_APP.name, "Grid");
    assert_eq!(UPDATE_APP.developer, "JeongEEE");
    assert_ne!(UPDATE_APP.id, craft_update::suite::GRIDCRAFT.id);
}

#[test]
fn fork_bundle_and_settings_are_separate_from_upstream() {
    assert_eq!(UPDATE_APP.bundle_id(), "io.github.jeongeee.grid");
    assert_ne!(UPDATE_APP.bundle_id(), craft_update::suite::GRIDCRAFT.bundle_id());
    // craft-update derives its per-user settings/cache directory from the display name.
    let paths = craft_update::paths::Paths::for_app(&UPDATE_APP).expect("per-user folders");
    let folder = if cfg!(any(target_os = "macos", target_os = "windows")) { "Grid" } else { "grid" };
    assert!(paths.data.ends_with(folder), "{}", paths.data.display());
    assert!(paths.cache.ends_with(folder), "{}", paths.cache.display());
}

#[test]
fn fork_tag_and_packages_are_understood_by_the_existing_updater() {
    use craft_update::{Arch, Os, PackageKind, Platform, pick_asset};
    let tag = "gridcraft-v0.19.2";
    let names = ["grid-0.19.2-macos-universal.dmg", "grid-0.19.2-windows-x64.msi"];
    let assets: Vec<_> = names
        .iter()
        .map(|name| {
            serde_json::json!({
                "name": name,
                "browser_download_url": format!("https://github.com/{}/releases/download/{tag}/{name}", UPDATE_APP.repo)
            })
        })
        .collect();
    let json = serde_json::json!({"tag_name": tag, "assets": assets}).to_string();
    let release = craft_update::parse_github_release(&json, UPDATE_APP.repo).expect("fork release");
    assert_eq!(release.version.as_ref().expect("version with repository prefix").to_string(), "0.19.2");
    for (os, arch, kind, name) in [
        (Os::MacOs, Arch::Arm64, PackageKind::Dmg, names[0]),
        (Os::MacOs, Arch::X64, PackageKind::Dmg, names[0]),
        (Os::Windows, Arch::X64, PackageKind::Msi, names[1]),
    ] {
        let (asset, _) = pick_asset(&release, &[UPDATE_APP.id], Platform { os, arch }, &[kind]).expect("matching fork package");
        assert_eq!(asset.name, name);
    }
}
