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
    reopen: Vec<(u64, Option<String>)>,
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
        cancel_restart(app);
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
    app.updates_rt.reopen.clear();
}

fn reopen_path(doc: &gridcraft_engine::DocState) -> Option<String> {
    // Imports and encrypted files have a source but deliberately no overwrite/save target.
    // Reopen that source through the normal open flow, including any password prompt.
    let path = doc.path.as_ref().map(std::path::PathBuf::from).or_else(|| doc.wb.source_path.clone())?;
    let path = if path.is_absolute() { path } else { std::env::current_dir().ok()?.join(path) };
    path.to_str().map(str::to_string)
}

/// The quit guard removes settled documents. Remember their final Save As path before that.
pub(crate) fn remember_restart_document(app: &mut SheetApp) {
    if !app.updates_rt.restart_requested {
        return;
    }
    if let Some(doc) = app.session.active()
        && let Some((_, path)) = app.updates_rt.reopen.iter_mut().find(|(uid, _)| *uid == doc.uid)
    {
        *path = reopen_path(doc);
    }
}

fn request_restart(app: &mut SheetApp, ctx: &egui::Context) {
    if app.updates_rt.ready.is_none() {
        return;
    }
    begin_restart(app, ctx);
}

fn begin_restart(app: &mut SheetApp, ctx: &egui::Context) {
    let active = app.session.active().map(|d| d.uid);
    let mut reopen: Vec<_> = app.session.documents().iter().map(|d| (d.uid, reopen_path(d))).collect();
    // Normal file opening activates the last one. The save guard may change the active document.
    reopen.sort_by_key(|(uid, _)| Some(*uid) == active);
    app.updates_rt.visible = false;
    app.request_quit(ctx);
    app.updates_rt.restart_requested = quit_is_pending(app);
    if app.updates_rt.restart_requested {
        app.updates_rt.reopen = reopen;
    }
}

/// No ready update is installed on a normal quit or after cancelling a restart/save prompt.
pub(crate) fn start_pending_update(app: &mut SheetApp) {
    if !std::mem::take(&mut app.updates_rt.restart_requested) || !app.allow_close {
        app.updates_rt.reopen.clear();
        return;
    }
    let mut seen = std::collections::BTreeSet::new();
    let reopen =
        std::mem::take(&mut app.updates_rt.reopen).into_iter().filter_map(|(_, path)| path).filter(|path| seen.insert(path.clone())).collect();
    if let Some(ready) = app.updates_rt.ready.take()
        && let Err(reason) = ready.launch(reopen)
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

    struct Books(std::path::PathBuf);
    impl Books {
        fn new() -> Self {
            static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
            let dir =
                std::env::temp_dir().join(format!("grid-reopen-{}-{}", std::process::id(), NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)));
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
        fn path(&self, name: &str) -> String {
            self.0.join(name).to_str().unwrap().to_string()
        }
    }
    impl Drop for Books {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn saved_app(path: &str) -> SheetApp {
        let mut app = SheetApp::new(Session::new(), Default::default());
        app.session.new_workbook();
        app.run("cell.set", serde_json::json!({"cell":"A1", "input":"보존할 내용"})).unwrap();
        app.run("file.save", serde_json::json!({"path":path})).unwrap();
        app
    }
    fn remembered(app: &SheetApp) -> Vec<&str> {
        app.updates_rt.reopen.iter().filter_map(|(_, p)| p.as_deref()).collect()
    }

    #[test]
    fn restart_remembers_all_files_and_opens_the_previously_active_one_last() {
        let books = Books::new();
        let first = books.path("한글 가계부.xlsx");
        let second = books.path("두 번째.xlsx");
        let mut app = saved_app(&first);
        app.run("file.new", serde_json::json!({})).unwrap();
        app.run("file.save", serde_json::json!({"path":second})).unwrap();
        app.session.set_active(0);
        begin_restart(&mut app, &egui::Context::default());
        assert!(app.allow_close);
        assert_eq!(remembered(&app), vec![second.as_str(), first.as_str()]);
    }

    #[test]
    fn saving_during_update_quit_keeps_the_file_after_the_document_closes() {
        let books = Books::new();
        let path = books.path("가계부.xlsx");
        let mut app = saved_app(&path);
        app.run("cell.set", serde_json::json!({"cell":"A1", "input":"수정한 내용"})).unwrap();
        let ctx = egui::Context::default();
        begin_restart(&mut app, &ctx);
        assert!(!app.allow_close);
        assert!(app.answer_save_changes(crate::SaveAnswer::Save, true, &ctx));
        assert!(app.allow_close);
        assert_eq!(remembered(&app), vec![path.as_str()]);
        assert!(app.session.documents().is_empty());
        let mut reopened = SheetApp::new(Session::new(), Default::default());
        reopened.open_path(&path);
        assert_eq!(reopened.session.doc().unwrap().wb.sheets[0].cells.get(crate::cell(0, 0)).unwrap().input_text(), "수정한 내용");
    }

    #[test]
    fn saving_a_new_workbook_uses_the_new_save_as_path() {
        let books = Books::new();
        let path = books.path("새 가계부.xlsx");
        let mut app = SheetApp::new(
            Session::new(),
            crate::Services {
                pick_save: Some(Box::new({
                    let path = path.clone();
                    move |_| Some(path.clone())
                })),
                ..Default::default()
            },
        );
        app.session.new_workbook();
        app.run("cell.set", serde_json::json!({"cell":"A1", "input":"42"})).unwrap();
        let ctx = egui::Context::default();
        begin_restart(&mut app, &ctx);
        assert!(app.answer_save_changes(crate::SaveAnswer::Save, true, &ctx));
        assert!(app.allow_close);
        assert_eq!(remembered(&app), vec![path.as_str()]);
        assert!(std::path::Path::new(&path).is_file());
    }

    #[test]
    fn discard_reopens_only_existing_files_and_cancel_clears_the_pending_session() {
        let books = Books::new();
        let path = books.path("원본.xlsx");
        let mut app = saved_app(&path);
        app.run("cell.set", serde_json::json!({"cell":"A1", "input":"버릴 변경"})).unwrap();
        app.run("file.new", serde_json::json!({})).unwrap();
        app.run("cell.set", serde_json::json!({"cell":"A1", "input":"저장 안 할 새 문서"})).unwrap();
        let ctx = egui::Context::default();
        begin_restart(&mut app, &ctx);
        assert!(app.answer_save_changes(crate::SaveAnswer::DontSave, true, &ctx));
        assert!(app.answer_save_changes(crate::SaveAnswer::DontSave, true, &ctx));
        assert!(app.allow_close);
        assert_eq!(remembered(&app), vec![path.as_str()]);

        let mut app = saved_app(&path);
        app.run("cell.set", serde_json::json!({"cell":"A1", "input":"작업 유지"})).unwrap();
        begin_restart(&mut app, &ctx);
        assert_eq!(remembered(&app), vec![path.as_str()]);
        assert!(app.answer_save_changes(crate::SaveAnswer::Cancel, true, &ctx));
        assert!(remembered(&app).is_empty());
        assert!(!app.updates_rt.restart_requested);
        assert!(!app.allow_close);
        assert!(app.session.doc().unwrap().is_dirty());
    }

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
