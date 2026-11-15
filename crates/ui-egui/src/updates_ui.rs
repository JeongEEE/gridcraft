//! Fork-signed background downloads and explicit restart through the normal save guard.
use crate::SheetApp;
use craft_update::{Release, Stop, http::Transfer};
use gridcraft_update::{download::Prepared, target::Target};
use std::sync::{
    Arc,
    mpsc::{Receiver, TryRecvError, channel},
};
use std::time::Duration;
use std::time::Instant;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const CHECK_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);

#[derive(Default)]
enum Phase {
    #[default]
    Idle,
    Checking,
    Current,
    Downloading {
        done: u64,
        total: u64,
    },
    Ready(String),
    Manual {
        version: String,
        reason: String,
    },
    Failed(String),
}

enum Message {
    Progress(Transfer),
    Manual(String, String),
    Done(Result<Option<Prepared>, String>),
}

#[derive(Default)]
pub(crate) struct Runtime {
    startup: bool,
    manual: bool,
    started: bool,
    visible: bool,
    requested: bool,
    next_check: Option<Instant>,
    pending: Option<Receiver<Message>>,
    stop: Arc<Stop>,
    phase: Phase,
    ready: Option<Prepared>,
    pub(crate) restart_requested: bool,
}

impl Drop for Runtime {
    fn drop(&mut self) {
        self.stop.cancel();
    }
}

pub(crate) fn start(app: &mut SheetApp, manual: bool) {
    if manual {
        app.updates_rt.manual = true;
    } else {
        app.updates_rt.startup = true;
    }
}

fn check_and_download(stop: &Stop, send: &dyn Fn(Message)) -> Result<Option<Prepared>, String> {
    let http = craft_update::Http::new("Grid/Update")?;
    let Some(release) = craft_update::check_for_update(&crate::UPDATE_APP, &http, VERSION)? else { return Ok(None) };
    let target = match Target::current() {
        Ok(target) => target,
        Err(reason) => {
            send(Message::Manual(release.version_label(), reason));
            return Ok(None);
        }
    };
    download(&release, target, stop, send).map(Some)
}

fn download(release: &Release, target: Target, stop: &Stop, send: &dyn Fn(Message)) -> Result<Prepared, String> {
    let mut last = gridcraft_update::instant().checked_sub(Duration::from_secs(1)).unwrap_or_else(gridcraft_update::instant);
    Prepared::download(release, target, stop, &mut |progress| {
        if last.elapsed() >= Duration::from_millis(100) {
            send(Message::Progress(progress));
            last = gridcraft_update::instant();
        }
    })
}

fn begin(rt: &mut Runtime, ctx: &egui::Context, manual: bool) {
    rt.visible |= manual;
    if rt.pending.is_some() {
        rt.requested |= manual;
        return;
    }
    if rt.ready.is_some() {
        return;
    }
    rt.requested = manual;
    rt.phase = Phase::Checking;
    rt.next_check = Some(gridcraft_update::instant() + CHECK_INTERVAL);
    rt.stop = Arc::new(Stop::default());
    let stop = rt.stop.clone();
    let (tx, rx) = channel();
    let repaint = ctx.clone();
    let spawned = std::thread::Builder::new().name("grid-auto-update".into()).spawn(move || {
        let send = |message| {
            let _ = tx.send(message);
            repaint.request_repaint();
        };
        let answer = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| check_and_download(&stop, &send)))
            .unwrap_or_else(|_| Err("Grid's update worker stopped unexpectedly.".into()));
        send(Message::Done(answer));
    });
    match spawned {
        Ok(_) => rt.pending = Some(rx),
        Err(e) => {
            rt.phase = Phase::Failed(e.to_string());
            rt.visible = true;
        }
    }
}

fn receive(rt: &mut Runtime, message: Message) {
    match message {
        Message::Progress(Transfer::Bytes { done, total } | Transfer::Reconnecting { done, total }) => {
            rt.phase = Phase::Downloading { done, total };
        }
        Message::Manual(version, reason) => {
            rt.phase = Phase::Manual { version, reason };
            rt.visible = true;
        }
        Message::Done(result) => {
            rt.pending = None;
            match result {
                Ok(Some(ready)) => {
                    rt.phase = Phase::Ready(ready.version.clone());
                    rt.ready = Some(ready);
                    rt.visible = true;
                }
                Ok(None) if matches!(rt.phase, Phase::Manual { .. }) => {}
                Ok(None) => rt.phase = if rt.requested { Phase::Current } else { Phase::Idle },
                Err(reason) => {
                    log::warn!("Grid auto-update: {reason}");
                    rt.phase = Phase::Failed(reason);
                    rt.visible = true;
                }
            }
        }
    }
}

fn quit_is_pending(app: &SheetApp) -> bool {
    app.allow_close || app.window_with_dialog("saveChanges").is_some() || app.macros.editor.as_ref().is_some_and(|ed| ed.quit)
}

pub(crate) fn tick(app: &mut SheetApp, ctx: &egui::Context) {
    if app.updates_rt.restart_requested && !quit_is_pending(app) {
        app.updates_rt.restart_requested = false;
    }
    let rt = &mut app.updates_rt;
    let manual = std::mem::take(&mut rt.manual);
    let startup = std::mem::take(&mut rt.startup);
    let periodic = rt.next_check.is_some_and(|at| gridcraft_update::instant() >= at) && rt.ready.is_none() && rt.pending.is_none();
    if manual || (startup && !rt.started) || periodic {
        if !rt.started {
            if let Some(outcome) = gridcraft_update::helper::take_outcome() {
                app.message = Some((
                    "Grid 업데이트".into(),
                    if outcome.success {
                        format!("Grid {} 업데이트가 완료되었습니다.", outcome.version)
                    } else {
                        format!("업데이트를 설치하지 못했습니다. {}", outcome.message)
                    },
                ));
            }
            rt.started = true;
        }
        begin(rt, ctx, manual);
    }
    while let Some(rx) = &rt.pending {
        match rx.try_recv() {
            Ok(message) => receive(rt, message),
            Err(TryRecvError::Empty) => {
                ctx.request_repaint_after(Duration::from_millis(200));
                break;
            }
            Err(TryRecvError::Disconnected) => {
                receive(rt, Message::Done(Err("Grid's update worker disconnected.".into())));
                break;
            }
        }
    }
    if let Some(at) = rt.next_check {
        ctx.request_repaint_after(at.saturating_duration_since(gridcraft_update::instant()));
    }
}

pub(crate) fn cancel_restart(app: &mut SheetApp) {
    app.updates_rt.restart_requested = false;
}

fn request_restart(app: &mut SheetApp, ctx: &egui::Context) {
    if app.updates_rt.ready.is_none() {
        return;
    }
    app.updates_rt.visible = false;
    app.request_quit(ctx);
    app.updates_rt.restart_requested = quit_is_pending(app);
}

/// No ready update is installed on a normal quit or after cancelling a restart/save prompt.
pub(crate) fn start_pending_update(app: &mut SheetApp) {
    if !std::mem::take(&mut app.updates_rt.restart_requested) || !app.allow_close {
        return;
    }
    if let Some(ready) = app.updates_rt.ready.take()
        && let Err(reason) = ready.launch()
    {
        log::error!("Grid update helper: {reason}");
    }
}

pub(crate) fn show(app: &mut SheetApp, ctx: &egui::Context) {
    let rt = &mut app.updates_rt;
    if !rt.visible {
        return;
    }
    let mut open = true;
    let mut restart = false;
    let mut releases = false;
    egui::Window::new("Grid 업데이트").id(egui::Id::unique("grid.fork.update")).collapsible(false).resizable(false).open(&mut open).show(ctx, |ui| {
        ui.set_max_width(440.0);
        match &rt.phase {
            Phase::Idle => {
                ui.label("업데이트를 확인할 수 있습니다.");
            }
            Phase::Checking => {
                ui.spinner();
                ui.label("새 버전을 확인하고 있습니다…");
            }
            Phase::Current => {
                ui.label(format!("Grid {VERSION} — 최신 버전입니다."));
            }
            Phase::Downloading { done, total } => {
                ui.label("업데이트를 백그라운드에서 다운로드하고 있습니다.");
                let fraction = if *total > 0 { *done as f32 / *total as f32 } else { 0.0 };
                ui.add(egui::ProgressBar::new(fraction).show_percentage());
            }
            Phase::Ready(version) => {
                ui.label(format!("Grid {version} 업데이트 준비 완료"));
                ui.label("재시작하면 업데이트를 설치합니다. 저장하지 않은 문서는 먼저 확인합니다.");
                restart = ui.button("재시작하여 업데이트").clicked();
            }
            Phase::Manual { version, reason } => {
                ui.label(format!("Grid {version} 새 버전이 있습니다."));
                ui.label(reason);
                releases = ui.button("릴리스에서 설치 파일 받기").clicked();
            }
            Phase::Failed(reason) => {
                ui.label("업데이트를 준비하지 못했습니다.");
                ui.label(reason);
                rt.manual |= ui.button("다시 시도").clicked();
                releases = ui.button("릴리스에서 직접 받기").clicked();
            }
        }
        if ui.button(if matches!(rt.phase, Phase::Ready(_)) { "나중에" } else { "닫기" }).clicked() {
            rt.visible = false;
        }
    });
    rt.visible &= open;
    if releases && let Some(open) = &app.services.open_url {
        open(&crate::UPDATE_APP.releases_url());
    }
    if restart {
        request_restart(app, ctx);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use gridcraft_engine::Session;

    #[test]
    fn ready_notice_can_be_dismissed_without_quitting_or_changing_the_workbook() {
        use egui_kittest::kittest::Queryable;
        let mut session = Session::new();
        session.new_workbook();
        let mut app = SheetApp::new(session, Default::default());
        app.run("cell.set", serde_json::json!({"cell":"A1", "input":"keep me"})).unwrap();
        app.updates_rt.phase = Phase::Ready("0.19.3".into());
        app.updates_rt.visible = true;
        let mut h = egui_kittest::Harness::builder()
            .with_size(egui::vec2(720.0, 320.0))
            .with_context_setup(|ctx| {
                ctx.set_fonts(crate::theme::pinned_font_definitions());
                crate::theme::apply(ctx, false);
            })
            .build_ui_state(|ui, app| show(app, ui.ctx()), app);
        h.run_steps(3);
        assert!(h.query_by_label("재시작하여 업데이트").is_some());
        if let Ok(path) = std::env::var("GRID_UPDATE_UI_SCREENSHOT") {
            h.render().unwrap().save(path).unwrap();
        }
        h.get_by_label("나중에").click();
        h.run_steps(2);
        assert!(!h.state().updates_rt.visible);
        assert!(!h.state().updates_rt.restart_requested);
        assert!(!h.state().allow_close);
        assert!(h.state().session.documents().iter().any(|d| d.is_dirty()));
    }

    #[test]
    fn cancelling_save_also_cancels_update_restart_and_preserves_workbook() {
        let mut session = Session::new();
        session.new_workbook();
        let mut app = SheetApp::new(session, Default::default());
        app.run("cell.set", serde_json::json!({"cell":"A1", "input":"keep me"})).unwrap();
        app.updates_rt.restart_requested = true;
        let ctx = egui::Context::default();
        app.request_quit(&ctx);
        app.updates_rt.restart_requested = true;
        assert!(app.answer_save_changes(crate::SaveAnswer::Cancel, true, &ctx));
        assert!(!app.updates_rt.restart_requested);
        assert!(!app.allow_close);
        assert!(app.session.documents().iter().any(|d| d.is_dirty()));
    }

    #[test]
    fn manual_check_during_startup_still_reports_the_current_version() {
        let mut rt = Runtime::default();
        let (_tx, rx) = channel();
        rt.pending = Some(rx);
        begin(&mut rt, &egui::Context::default(), true);
        receive(&mut rt, Message::Done(Ok(None)));
        assert!(rt.visible);
        assert!(matches!(rt.phase, Phase::Current));
    }

    #[test]
    fn abandoning_a_quit_without_a_dialog_clears_update_intent() {
        let mut session = Session::new();
        session.new_workbook();
        let mut app = SheetApp::new(session, Default::default());
        app.updates_rt.restart_requested = true;
        tick(&mut app, &egui::Context::default());
        assert!(!app.updates_rt.restart_requested);
    }
}
