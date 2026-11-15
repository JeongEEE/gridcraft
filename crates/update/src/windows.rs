//! Use MSI for registered copies; never unpack a portable ZIP over an MSI installation.
use super::{
    helper, install,
    target::{Target, is_own_msi_product, msi_folder_argument},
};
use craft_update::msi_context::{self, MsiScope};
use std::path::{Path, PathBuf};
use std::time::Duration;
use winreg::{
    RegKey,
    enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_32KEY, KEY_WOW64_64KEY},
};

pub(crate) fn target(exe: &Path) -> Result<Target, String> {
    if !exe.is_absolute()
        || exe.file_name().is_none_or(|n| !n.to_string_lossy().eq_ignore_ascii_case("grid.exe"))
        || !exe.is_file()
        || exe.components().any(|c| {
            matches!(c.as_os_str().to_string_lossy().to_ascii_lowercase().as_str(), "target" | "windowsapps" | "scoop" | "chocolatey" | "winget")
        })
    {
        return Err("This Grid copy is managed by its build or package manager.".into());
    }
    let mut scopes = Vec::new();
    let mut any_registered = false;
    for (hive, machine) in [(HKEY_CURRENT_USER, false), (HKEY_LOCAL_MACHINE, true)] {
        match RegKey::predef(hive).open_subkey_with_flags(r"Software\Microsoft\Windows\CurrentVersion\App Paths\grid.exe", KEY_READ | KEY_WOW64_64KEY)
        {
            Ok(key) => {
                let path: String = key.get_value("").map_err(|e| e.to_string())?;
                any_registered = true;
                if same_path(Path::new(path.trim_matches('"')), exe) {
                    scopes.push(machine);
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("Cannot read Grid's installation registration: {e}")),
        }
    }
    if scopes.len() > 1 {
        return Err("Grid is registered for both your account and everyone; choose a copy in its installer.".into());
    }
    if let Some(&machine) = scopes.first() {
        let products = product_codes()?;
        let [product] = products.as_slice() else {
            return Err("Windows lists zero or multiple Grid MSI products; use the installer to repair the registration.".into());
        };
        let scope = msi_context::probe(product).map_err(|e| format!("Cannot confirm Grid's MSI context: {e:?}"))?;
        if (scope == MsiScope::PerMachine) != machine {
            return Err("Grid's registry and MSI context do not agree.".into());
        }
        return Ok(Target::Msi { exe: exe.to_path_buf(), product: product.clone(), machine });
    }
    // A loose copy alongside an installed copy is fine; only the exact registered executable
    // is an MSI. A folder with an unresolved Grid MSI record is never guessed to be portable.
    if !any_registered && !product_codes()?.is_empty() {
        return Err("Grid's MSI registration is incomplete; repair it with the installer.".into());
    }
    let folder = exe.parent().ok_or("No Grid folder.")?;
    let probe = tempfile::Builder::new()
        .prefix(".grid-update-probe-")
        .tempdir_in(folder)
        .map_err(|e| format!("Grid cannot write its portable folder: {e}"))?;
    probe.close().map_err(|e| e.to_string())?;
    Ok(Target::Portable { exe: exe.to_path_buf() })
}

fn product_codes() -> Result<Vec<String>, String> {
    // craft-update's generic detector pins the original publisher (Learning Machines LLC).
    // Enumerate this fork's records in both registry views instead; the MSI context probe
    // below still independently confirms the product belongs to the registered app's scope.
    let mut codes = std::collections::BTreeSet::new();
    for hive in [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE] {
        for view in [KEY_WOW64_64KEY, KEY_WOW64_32KEY] {
            let root = match RegKey::predef(hive).open_subkey_with_flags(r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall", KEY_READ | view) {
                Ok(root) => root,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => return Err(format!("Cannot read MSI product registrations: {e}")),
            };
            for code in root.enum_keys() {
                let code = code.map_err(|e| e.to_string())?;
                let key = match root.open_subkey_with_flags(&code, KEY_READ | view) {
                    Ok(key) => key,
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                    Err(e) => return Err(format!("Cannot inspect an MSI product registration: {e}")),
                };
                let name: String = key.get_value("DisplayName").unwrap_or_default();
                let publisher: String = key.get_value("Publisher").unwrap_or_default();
                if is_own_msi_product(&name, &publisher, &code) {
                    codes.insert(code.to_ascii_uppercase());
                }
            }
        }
    }
    Ok(codes.into_iter().collect())
}

fn same_path(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a.to_string_lossy().eq_ignore_ascii_case(&b.to_string_lossy()),
        _ => false,
    }
}

pub(crate) fn install(package: &Path, target: &Target, version: &str, work: &Path, reopen: &[String]) -> Result<(), String> {
    target.validate()?;
    let expected = craft_update::Version::parse(version).map_err(|e| e.to_string())?;
    let current = craft_update::detect::windows::executable_version(&target.exe()).ok_or("Cannot read installed Grid version.")?;
    if expected <= current {
        return Err("The installed Grid is already this version or newer.".into());
    }
    match target {
        Target::Portable { exe } => {
            let staged = work.join("unpacked");
            install::extract_portable(package, &staged, version)?;
            if craft_update::detect::windows::executable_version(&staged.join("grid.exe")) != Some(expected) {
                return Err("The portable update contains the wrong Grid version.".into());
            }
            install::replace_portable(&staged, exe.parent().ok_or("No Grid folder.")?, || {
                helper::restart(target, Some(&work.join("healthy")), reopen)
            })
        }
        Target::Msi { machine, exe, .. } => {
            install_msi(package, *machine, exe.parent().ok_or("No Grid folder.")?, work)?;
            if craft_update::detect::windows::executable_version(exe) != Some(expected) {
                return Err("The MSI completed without installing the expected Grid version. See the update log.".into());
            }
            helper::restart(target, Some(&work.join("healthy")), reopen)
        }
        _ => Err("A macOS update cannot install on Windows.".into()),
    }
}

fn install_msi(package: &Path, machine: bool, folder: &Path, work: &Path) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    let program = std::env::var_os("SystemRoot")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .ok_or("No Windows system directory.")?
        .join(r"System32\msiexec.exe");
    let mut command = craft_update::process::command(program);
    // This package runs FindRelatedProducts before costing normalizes ALLUSERS=2. Keep it
    // empty for per-user upgrades, or MSI can miss the old user product and register a second.
    command.arg("/i").arg(package).arg(if machine { "/passive" } else { "/qn" }).arg("/norestart").arg("REBOOT=ReallySuppress");
    if machine {
        command.arg("ALLUSERS=1");
    } else {
        command.raw_arg(r#"ALLUSERS="""#).arg("MSIINSTALLPERUSER=1");
    }
    command.raw_arg(msi_folder_argument(folder)?).arg("/l*v").arg(work.join("msi-install.log"));
    craft_update::process::run_command(command, "Grid MSI update", Duration::from_secs(10 * 60))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "Requires packaged ZIP in GRID_UPDATE_SMOKE_PACKAGE"]
    fn native_windows_portable_roundtrip_preserves_user_files_and_rolls_back() {
        let package = std::env::var_os("GRID_UPDATE_SMOKE_PACKAGE").map(PathBuf::from).expect("ZIP path");
        let root = tempfile::tempdir().unwrap();
        let folder = root.path().join("Grid");
        std::fs::create_dir(&folder).unwrap();
        std::fs::write(folder.join("grid.exe"), "old payload").unwrap();
        std::fs::write(folder.join("book.xlsx"), "personal data").unwrap();
        for failure in [true, false] {
            let stage = root.path().join(if failure { "first" } else { "second" });
            install::extract_portable(&package, &stage, env!("CARGO_PKG_VERSION")).unwrap();
            let result = install::replace_portable(&stage, &folder, || {
                let output = craft_update::process::run(folder.join("grid.exe").to_str().unwrap(), &["--version"], Duration::from_secs(30))?;
                assert!(output.contains(env!("CARGO_PKG_VERSION")));
                if failure { Err("Simulated native restart failure".into()) } else { Ok(()) }
            });
            assert_eq!(result.is_err(), failure);
            if failure {
                assert_eq!(std::fs::read_to_string(folder.join("grid.exe")).unwrap(), "old payload");
            }
        }
        assert_eq!(std::fs::read_to_string(folder.join("book.xlsx")).unwrap(), "personal data");
    }

    #[test]
    #[ignore = "Requires isolated runner MSI baseline installed by test-auto-update.ps1"]
    fn native_windows_msi_upgrade_keeps_one_product_in_the_same_scope_and_folder() {
        let package = std::env::var_os("GRID_UPDATE_SMOKE_PACKAGE").map(PathBuf::from).expect("MSI path");
        let folder = std::env::var_os("GRID_UPDATE_SMOKE_INSTALL").map(PathBuf::from).expect("installed folder");
        let work = tempfile::tempdir().unwrap();
        let before = target(&folder.join("grid.exe")).unwrap();
        assert!(matches!(before, Target::Msi { machine: false, .. }));
        install_msi(&package, false, &folder, work.path()).unwrap();
        let after = target(&folder.join("grid.exe")).unwrap();
        assert!(matches!(after, Target::Msi { machine: false, .. }));
        assert_ne!(before, after, "major upgrade must replace the old product registration");
        assert_eq!(product_codes().unwrap().len(), 1);
        assert_eq!(craft_update::detect::windows::executable_version(&folder.join("grid.exe")).unwrap().to_string(), env!("CARGO_PKG_VERSION"));
    }
}
