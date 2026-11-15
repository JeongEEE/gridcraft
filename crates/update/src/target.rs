//! Identify only installations the fork owns, retaining the original MSI scope.
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Target {
    Mac { bundle: PathBuf },
    Msi { exe: PathBuf, product: String, machine: bool },
    Portable { exe: PathBuf },
}

impl Target {
    pub fn current() -> Result<Self, String> {
        let exe = std::env::current_exe().and_then(std::fs::canonicalize).map_err(|e| e.to_string())?;
        Self::at(&exe)
    }

    pub fn platform(&self) -> &'static str {
        match self {
            Self::Mac { .. } => "macos-universal",
            Self::Msi { .. } => "windows-x64-msi",
            Self::Portable { .. } => "windows-x64-portable",
        }
    }

    pub fn exe(&self) -> PathBuf {
        match self {
            Self::Mac { bundle } => bundle.join("Contents/MacOS/Grid"),
            Self::Msi { exe, .. } | Self::Portable { exe } => exe.clone(),
        }
    }

    pub fn running_path(&self) -> &Path {
        match self {
            Self::Mac { bundle } => bundle,
            Self::Msi { exe, .. } | Self::Portable { exe } => exe,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        // Re-discover on the helper side, rather than trusting paths or scope saved in a plan.
        if Self::at(&self.exe())? != *self {
            return Err("Grid's installation changed while the update was downloaded.".into());
        }
        Ok(())
    }

    #[cfg(target_os = "macos")]
    fn at(exe: &Path) -> Result<Self, String> {
        let bundle = exe.parent().and_then(Path::parent).and_then(Path::parent).ok_or("Grid is not installed as an app.")?;
        if bundle.extension().is_none_or(|e| e != "app")
            || exe != bundle.join("Contents/MacOS/Grid")
            || bundle.starts_with("/Volumes")
            || bundle.components().any(|p| p.as_os_str() == "AppTranslocation")
            || craft_update::detect::macos::read_bundle(&super::APP, bundle).is_none()
        {
            return Err("Move Grid.app to Applications before using automatic updates.".into());
        }
        // This only probes permission to create a sibling; no existing file is changed.
        let parent = bundle.parent().ok_or("No application folder.")?;
        let probe = tempfile::Builder::new()
            .prefix(".grid-update-probe-")
            .tempdir_in(parent)
            .map_err(|e| format!("Grid cannot write its application folder: {e}"))?;
        probe.close().map_err(|e| e.to_string())?;
        Ok(Self::Mac { bundle: bundle.to_path_buf() })
    }

    #[cfg(windows)]
    fn at(exe: &Path) -> Result<Self, String> {
        super::windows::target(exe)
    }

    #[cfg(not(any(target_os = "macos", windows)))]
    fn at(_exe: &Path) -> Result<Self, String> {
        Err("Grid automatic updates support installed macOS and Windows copies.".into())
    }
}
