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

#[cfg(any(windows, test))]
pub(crate) fn is_own_msi_product(name: &str, publisher: &str, product: &str) -> bool {
    name == super::APP.name && publisher == super::APP.developer && craft_update::msi_context::valid_product_code(product)
}

#[cfg(any(windows, test))]
pub(crate) fn msi_folder_argument(folder: &Path) -> Result<String, String> {
    let path = folder.to_str().ok_or("Windows Installer cannot represent this folder name.")?;
    if path.chars().any(|c| c == '"' || c.is_control()) {
        return Err("Invalid character in the MSI installation folder.".into());
    }
    // MSI parses property assignments itself: raw_arg must quote the value, not the whole
    // PROPERTY=value token. Canonical Win32 paths also need their ordinary DOS/UNC form.
    let value =
        if let Some(unc) = path.strip_prefix(r"\\?\UNC\") { format!(r"\\{unc}") } else { path.strip_prefix(r"\\?\").unwrap_or(path).to_string() };
    Ok(format!("INSTALLFOLDER=\"{value}\""))
}

#[cfg(test)]
mod tests {
    #[test]
    fn msi_directory_assignment_quotes_only_the_value_and_rejects_injected_properties() {
        use std::path::Path;
        assert_eq!(
            super::msi_folder_argument(Path::new(r"\\?\C:\Users\Name With Space\Grid")).unwrap(),
            r#"INSTALLFOLDER="C:\Users\Name With Space\Grid""#
        );
        assert_eq!(super::msi_folder_argument(Path::new(r"\\?\UNC\server\share\그리드")).unwrap(), r#"INSTALLFOLDER="\\server\share\그리드""#);
        assert!(super::msi_folder_argument(Path::new("C:\\bad\" TRANSFORMS=evil")).is_err());
        assert!(super::msi_folder_argument(Path::new("C:\\bad\nfolder")).is_err());
    }

    #[test]
    fn fork_msi_record_matches_its_publisher_and_valid_product_identity() {
        let code = "{D0FEAEC3-C655-56AA-936E-56F8A764065E}";
        assert!(super::is_own_msi_product("Grid", "JeongEEE", code));
        assert!(!super::is_own_msi_product("Grid", "Learning Machines LLC", code));
        assert!(!super::is_own_msi_product("GridCraft", "JeongEEE", code));
        assert!(!super::is_own_msi_product("Grid", "JeongEEE", "not-a-product-code"));
    }
}
