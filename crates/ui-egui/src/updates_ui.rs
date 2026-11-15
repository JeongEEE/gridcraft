//! Background update checks using the shared release parser and dialogs.
//! This fork downloads updates manually: the shared macOS installer pins ArtCraft's signing
//! team, and unsigned Windows builds cannot authenticate an automatic replacement.

use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::time::Duration;

use craft_update::updater::Status;
use craft_update::{CheckResultResponse, Release, StartupPrompt, StartupPromptResponse};

use crate::SheetApp;

const VERSION: &str = env!("CARGO_PKG_VERSION");
type Check = Result<Option<Release>, String>;

pub(crate) struct Runtime {
    startup: bool,
    manual: bool,
    started: bool,
    requested: bool,
    pending: Option<Receiver<Check>>,
    status: Status,
    prompt: Option<StartupPrompt>,
}

impl Default for Runtime {
    fn default() -> Self {
        Self { startup: false, manual: false, started: false, requested: false, pending: None, status: Status::Idle, prompt: None }
    }
}

pub(crate) fn start(app: &mut SheetApp, manual: bool) {
    if manual {
        app.updates_rt.manual = true;
    } else {
        app.updates_rt.startup = true;
    }
}

fn download_prompt(mut prompt: StartupPrompt) -> StartupPrompt {
    // Never offer an installation the pinned shared signer policy would reject. No installer
    // or restart worker is reachable from this fork's update UI, including on signed machines.
    prompt.is_self_updating = false;
    prompt
}

fn response_url(response: StartupPromptResponse) -> Option<String> {
    match response {
        StartupPromptResponse::Download | StartupPromptResponse::UpdateAndRestart => Some(crate::UPDATE_APP.releases_url()),
        StartupPromptResponse::Pending | StartupPromptResponse::Later => None,
    }
}

fn finish(rt: &mut Runtime, answer: Check) {
    let requested = std::mem::take(&mut rt.requested);
    match answer {
        Ok(Some(release)) => {
            rt.prompt = Some(download_prompt(StartupPrompt::new(&crate::UPDATE_APP, release, VERSION)));
            rt.status = Status::Idle;
        }
        Ok(None) => rt.status = if requested { Status::UpToDate } else { Status::Idle },
        Err(reason) => {
            log::warn!("Grid update check failed: {reason}");
            rt.status = if requested { Status::Failed(reason) } else { Status::Idle };
        }
    }
}

fn begin(rt: &mut Runtime, ctx: &egui::Context, manual: bool) {
    rt.requested |= manual;
    if manual {
        rt.status = Status::Checking;
    }
    if rt.pending.is_some() {
        return;
    }
    let (tx, rx) = channel();
    let repaint = ctx.clone();
    let worker = std::thread::Builder::new().name("grid-update-check".into()).spawn(move || {
        let answer = std::panic::catch_unwind(|| {
            craft_update::Http::new("Grid/Update").and_then(|http| craft_update::check_for_update(&crate::UPDATE_APP, &http, VERSION))
        })
        .unwrap_or_else(|_| Err("the update check failed unexpectedly".into()));
        let _ = tx.send(answer);
        repaint.request_repaint();
    });
    match worker {
        Ok(_) => rt.pending = Some(rx),
        Err(reason) => finish(rt, Err(format!("couldn't start the update check ({reason})"))),
    }
}

pub(crate) fn tick(app: &mut SheetApp, ctx: &egui::Context) {
    let rt = &mut app.updates_rt;
    let manual = std::mem::take(&mut rt.manual);
    let startup = std::mem::take(&mut rt.startup);
    if manual || (startup && !rt.started) {
        rt.started = true;
        begin(rt, ctx, manual);
    }
    if let Some(rx) = &rt.pending {
        let answer = match rx.try_recv() {
            Ok(answer) => Some(answer),
            Err(TryRecvError::Empty) => {
                ctx.request_repaint_after(Duration::from_millis(100));
                None
            }
            Err(TryRecvError::Disconnected) => Some(Err("the update check stopped unexpectedly".into())),
        };
        if let Some(answer) = answer {
            rt.pending = None;
            finish(rt, answer);
        }
    }
}

/// Compatibility with the desktop lifecycle; download-only updates never schedule an install.
pub(crate) fn start_pending_update(_app: &mut SheetApp) {}

fn result_window_shows(status: &Status, prompt_open: bool) -> bool {
    !prompt_open && !matches!(status, Status::Idle)
}

pub(crate) fn show(app: &mut SheetApp, ctx: &egui::Context) {
    let rt = &mut app.updates_rt;
    if let Some(prompt) = &mut rt.prompt {
        let response = prompt.show(ctx);
        if let Some(url) = response_url(response) {
            if let Some(open) = &app.services.open_url {
                open(&url);
            }
            rt.prompt = None;
        } else if response == StartupPromptResponse::Later {
            rt.prompt = None;
        }
    }
    if result_window_shows(&rt.status, rt.prompt.is_some()) {
        match craft_update::CheckResultDialog::new(&crate::UPDATE_APP, VERSION).show(ctx, &rt.status) {
            CheckResultResponse::Close => rt.status = Status::Idle,
            CheckResultResponse::CheckAgain => rt.manual = true,
            CheckResultResponse::RestartToUpdate | CheckResultResponse::None => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fork_update_offer_only_downloads_without_starting_an_installer() {
        let release = craft_update::Release { tag: "gridcraft-v0.19.3".into(), ..Default::default() };
        let mut offered = StartupPrompt::new(&crate::UPDATE_APP, release, VERSION);
        offered.is_self_updating = true; // Simulate a recognized installed app, independent of the test host.
        let prompt = download_prompt(offered);
        assert!(!prompt.is_self_updating);
        assert!(!prompt.is_installing);
        assert_eq!(response_url(craft_update::StartupPromptResponse::Download), Some(crate::UPDATE_APP.releases_url()));
        assert_eq!(response_url(craft_update::StartupPromptResponse::UpdateAndRestart), Some(crate::UPDATE_APP.releases_url()));
        assert_eq!(response_url(craft_update::StartupPromptResponse::Later), None);
    }

    #[test]
    fn result_window_shows_every_status_but_idle() {
        let shown = [
            Status::Checking,
            Status::UpToDate,
            Status::Failed("no network".into()),
            Status::Installing,
            Status::Ready,
            Status::Managed,
            Status::Updated("0.2.0".into()),
            Status::InstallFailed("disk full".into()),
        ];
        for status in &shown {
            assert!(result_window_shows(status, false), "{status:?}");
            assert!(!result_window_shows(status, true), "{status:?} over the prompt");
        }
        assert!(!result_window_shows(&Status::Idle, false));
    }
}
