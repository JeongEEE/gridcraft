//! Authenticated update packages for Grid, independent of the upstream publisher.
#![cfg(not(target_arch = "wasm32"))]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

#[allow(clippy::disallowed_methods)] // This crate is used only by native desktop targets.
pub fn instant() -> std::time::Instant {
    std::time::Instant::now()
}

pub mod manifest;

pub mod download;
pub mod helper;
pub mod install;
#[cfg(target_os = "macos")]
mod macos;
pub mod target;
#[cfg(windows)]
mod windows;

pub const APP: craft_update::App = craft_update::App {
    developer: "JeongEEE",
    bundle_id_override: Some("io.github.jeongeee.grid"),
    ..craft_update::App::new("grid", "Grid", "Spreadsheets and data", manifest::REPOSITORY)
};

/// Release builds pin this key in source. No environment variable may change an installed
/// application's trust root. Empty means fail closed until the owner provisions their key.
pub fn public_key() -> Result<Vec<u8>, String> {
    let key = manifest::decode_hex(include_str!("../public-key.hex").trim())?;
    if key.len() != 32 {
        return Err("Grid's update signing key has not been configured.".into());
    }
    Ok(key)
}
