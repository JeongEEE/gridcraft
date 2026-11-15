//! The copied launcher finishes an authenticated update after its parent really exits.
use super::{
    APP,
    download::{Prepared, read_small, read_verified},
    manifest,
    target::Target,
};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;

const FLAG: &str = "--grid-update-helper";
const HEALTH_ENV: &str = "GRID_UPDATE_HEALTH_FILE";

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Plan {
    parent: u32,
    target: Target,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Outcome {
    pub success: bool,
    pub version: String,
    pub message: String,
}

fn helper_name() -> &'static str {
    if cfg!(windows) { "grid-update-helper.exe" } else { "grid-update-helper" }
}

pub(crate) fn launch(prepared: Prepared) -> Result<(), String> {
    prepared.target.validate()?;
    let dir = prepared.folder.path();
    // Authenticate again before creating an executable helper or recording an install plan.
    let verified = read_verified(dir)?;
    manifest::verify_package(&dir.join(&verified.asset(prepared.target.platform())?.name), verified.asset(prepared.target.platform())?)?;
    let helper = dir.join(helper_name());
    std::fs::copy(std::env::current_exe().map_err(|e| e.to_string())?, &helper).map_err(|e| e.to_string())?;
    let plan = Plan { parent: std::process::id(), target: prepared.target };
    let plan_path = dir.join("install.json");
    std::fs::write(&plan_path, serde_json::to_vec(&plan).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    craft_update::process::command(&helper)
        .arg(FLAG)
        .arg(&plan_path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("Could not start Grid's update helper: {e}"))?;
    // The helper now owns these files. Startup garbage collection removes completed helpers.
    let _ = prepared.folder.keep();
    Ok(())
}

/// Recognized helper invocations never fall through to the normal spreadsheet CLI.
pub fn finish(args: &[std::ffi::OsString]) -> Option<Result<(), String>> {
    if args.first().is_none_or(|a| a != FLAG) {
        return None;
    }
    Some((|| {
        if args.len() != 2 {
            return Err("Invalid Grid update helper arguments.".into());
        }
        let plan_path = PathBuf::from(args.get(1).ok_or("Missing Grid update plan.")?);
        let plan: Plan = serde_json::from_slice(&read_small(&plan_path, 16 * 1024)?).map_err(|e| e.to_string())?;
        let dir = validate_helper(&plan_path, &plan)?;
        let version = read_verified(&dir).map(|m| m.version).unwrap_or_else(|_| env!("CARGO_PKG_VERSION").into());
        let result = apply(&dir, &plan);
        let outcome = Outcome { success: result.is_ok(), version, message: result.as_ref().err().cloned().unwrap_or_default() };
        record(&plan.target, &outcome);
        // Never launch another copy when the parent did not exit or another Grid is still open.
        if result.is_err() && !craft_update::process::running(plan.target.running_path()).unwrap_or(true) {
            let _ = restart(&plan.target, None);
        }
        result
    })())
}

fn validate_helper(plan_path: &Path, plan: &Plan) -> Result<PathBuf, String> {
    let dir = plan_path.parent().ok_or("No update folder.")?.canonicalize().map_err(|e| e.to_string())?;
    let exe = std::env::current_exe().and_then(std::fs::canonicalize).map_err(|e| e.to_string())?;
    let paths = craft_update::Paths::for_exe(&APP, &plan.target.exe())?;
    let expected = paths.cache.join("grid-updates").canonicalize().map_err(|e| e.to_string())?;
    if dir.parent() != Some(expected.as_path())
        || !dir.file_name().is_some_and(|s| s.to_string_lossy().starts_with("grid-update-"))
        || exe != dir.join(helper_name())
        || plan_path.file_name().is_none_or(|s| s != "install.json")
        || plan.parent == 0
        || plan.parent == std::process::id()
    {
        return Err("The update helper is outside Grid's own update folder.".into());
    }
    plan.target.validate()?;
    Ok(dir)
}

fn apply(dir: &Path, plan: &Plan) -> Result<(), String> {
    let manifest = read_verified(dir)?;
    let asset = manifest.asset(plan.target.platform())?;
    let package = dir.join(&asset.name);
    manifest::verify_package(&package, asset)?;
    if !wait_for_parent(plan.parent, Duration::from_secs(90)) {
        return Err("Grid did not exit; no update was installed.".into());
    }
    if craft_update::process::running(plan.target.running_path())? {
        return Err("Another Grid window is still running; no update was installed.".into());
    }
    plan.target.validate()?;
    match &plan.target {
        #[cfg(target_os = "macos")]
        Target::Mac { bundle } => super::macos::install(&package, bundle, &manifest.version, dir),
        #[cfg(windows)]
        Target::Msi { .. } | Target::Portable { .. } => super::windows::install(&package, &plan.target, &manifest.version, dir),
        #[allow(unreachable_patterns)]
        _ => Err("This platform cannot install this Grid update.".into()),
    }
}

pub fn wait_for_parent(pid: u32, timeout: Duration) -> bool {
    if pid == 0 || pid == std::process::id() {
        return false;
    }
    #[cfg(windows)]
    {
        craft_update::process::wait_for_exit(pid, timeout)
    }
    #[cfg(unix)]
    {
        use nix::{errno::Errno, sys::signal::kill, unistd::Pid};
        let Ok(pid) = i32::try_from(pid) else { return false };
        let start = crate::instant();
        loop {
            match kill(Pid::from_raw(pid), None) {
                Err(Errno::ESRCH) => return true,
                Err(_) => return false,
                Ok(()) if start.elapsed() >= timeout => return false,
                Ok(()) => std::thread::sleep(Duration::from_millis(100)),
            }
        }
    }
    #[cfg(not(any(windows, unix)))]
    {
        let _ = timeout;
        false
    }
}

/// Signal only after the first frame has successfully rendered, not merely after spawning.
pub fn acknowledge_startup() {
    static ACKNOWLEDGED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if ACKNOWLEDGED.swap(true, std::sync::atomic::Ordering::Relaxed) {
        return;
    }
    let Some(path) = std::env::var_os(HEALTH_ENV).map(PathBuf::from) else { return };
    if path.file_name().is_none_or(|n| n != "healthy")
        || !path.parent().and_then(Path::file_name).is_some_and(|n| n.to_string_lossy().starts_with("grid-update-"))
    {
        return;
    }
    use std::io::Write;
    if let Ok(mut file) = std::fs::OpenOptions::new().write(true).create_new(true).open(path) {
        let _ = file.write_all(b"Grid ready");
    }
}

pub(crate) fn restart(target: &Target, health: Option<&Path>) -> Result<(), String> {
    let mut cmd = craft_update::process::command(target.exe());
    cmd.env_remove("CRAFT_UPDATE_DEV")
        .env_remove("GRIDCRAFT_CONTROL_TOKEN")
        .env_remove("GRIDCRAFT_CONTROL_PORT")
        .env_remove("ARTCRAFT_LAUNCHER_PRETEND_VERSION")
        .env_remove("GRID_PRETEND_VERSION");
    if let Some(path) = health {
        cmd.env(HEALTH_ENV, path);
    } else {
        cmd.env_remove(HEALTH_ENV);
    }
    let mut child = cmd
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("Could not restart Grid: {e}"))?;
    let Some(health) = health else { return Ok(()) };
    let start = crate::instant();
    while start.elapsed() < Duration::from_secs(60) {
        if health.is_file() {
            return Ok(());
        }
        if child.try_wait().map_err(|e| e.to_string())?.is_some() {
            return Err("The updated Grid exited before its first frame.".into());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let _ = child.kill();
    let _ = child.wait();
    Err("The updated Grid did not finish starting within 60 seconds.".into())
}

pub(crate) fn record(target: &Target, outcome: &Outcome) {
    let Ok(paths) = craft_update::Paths::for_exe(&APP, &target.exe()) else { return };
    if std::fs::create_dir_all(&paths.data).is_err() {
        return;
    }
    if let Ok(bytes) = serde_json::to_vec(outcome) {
        let _ = std::fs::write(paths.data.join("grid-update-result.json"), bytes);
    }
}

pub fn take_outcome() -> Option<Outcome> {
    let path = craft_update::Paths::for_app(&APP).ok()?.data.join("grid-update-result.json");
    let bytes = read_small(&path, 16 * 1024).ok()?;
    let _ = std::fs::remove_file(path);
    serde_json::from_slice(&bytes).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn never_waits_for_itself_or_pid_zero() {
        assert!(!wait_for_parent(std::process::id(), Duration::ZERO));
        assert!(!wait_for_parent(0, Duration::ZERO));
    }
    #[cfg(unix)]
    #[test]
    fn live_parent_times_out_and_exited_parent_is_accepted() {
        let mut child = std::process::Command::new("/bin/sleep").arg("5").spawn().unwrap();
        assert!(!wait_for_parent(child.id(), Duration::from_millis(20)));
        child.kill().unwrap();
        child.wait().unwrap();
        assert!(wait_for_parent(child.id(), Duration::from_millis(20)));
    }
}
