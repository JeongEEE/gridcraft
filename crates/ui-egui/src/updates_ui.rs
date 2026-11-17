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

mod view;

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
        reconnecting: bool,
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
    completed: Option<(String, Option<f64>)>,
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
    send(Message::Progress(Transfer::Bytes { done: 0, total: 0 }));
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
        Message::Progress(Transfer::Bytes { done, total }) => {
            rt.phase = Phase::Downloading { done, total, reconnecting: false };
        }
        Message::Progress(Transfer::Reconnecting { done, total }) => {
            rt.phase = Phase::Downloading { done, total, reconnecting: true };
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

fn outcome_notice(rt: &mut Runtime, outcome: gridcraft_update::helper::Outcome) -> Option<(String, String)> {
    if outcome.success {
        rt.completed = Some((format!("Grid {} 업데이트가 완료되었습니다.", outcome.version), None));
        None
    } else {
        Some(("Grid 업데이트".into(), format!("업데이트를 설치하지 못했습니다. {}", outcome.message)))
    }
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
            if let Some(outcome) = gridcraft_update::helper::take_outcome()
                && let Some(notice) = outcome_notice(rt, outcome)
            {
                app.message = Some(notice);
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
    // Finish in-place drafts before introducing a modal: outside clicks and Escape settle
    // text-box drafts before the sheet UI runs, so a prompt must not cover an active draft.
    let prompt_allowed = app.message.is_none()
        && app.dialog.is_none()
        && app.macros.question.is_none()
        && app.editor.is_none()
        && app.text_box_editor.is_none()
        && !quit_is_pending(app);
    let response = view::show(&mut app.updates_rt, ctx, prompt_allowed);
    if response.modal {
        // Sheet and ribbon shortcuts read raw events rather than egui focus. Consume keyboard
        // events after the modal has handled them so they cannot edit the workbook underneath.
        ctx.input_mut(|i| {
            i.events.retain(|e| {
                !matches!(
                    e,
                    egui::Event::Key { .. }
                        | egui::Event::Text(_)
                        | egui::Event::Ime(_)
                        | egui::Event::Copy
                        | egui::Event::Cut
                        | egui::Event::Paste(_)
                        | egui::Event::PasteImage(_)
                        | egui::Event::MiddleClickPaste { .. }
                )
            });
        });
    }
    if response.releases
        && let Some(open) = &app.services.open_url
    {
        open(&crate::UPDATE_APP.releases_url());
    }
    if response.restart {
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
        h.run_steps(3);
        assert!(!h.state().updates_rt.visible);
        assert!(!h.state().updates_rt.restart_requested);
        assert!(!h.state().allow_close);
        assert!(h.state().session.documents().iter().any(|d| d.is_dirty()));
        h.get_by_label("업데이트 준비됨").click();
        h.run_steps(2);
        assert!(h.query_by_label("재시작하여 업데이트").is_some());
        assert!(!h.state().updates_rt.restart_requested);
    }

    #[test]
    fn background_download_shows_progress_without_opening_a_dialog() {
        use egui_kittest::kittest::Queryable;
        let mut app = SheetApp::new(Session::new(), Default::default());
        receive(&mut app.updates_rt, Message::Progress(Transfer::Bytes { done: 25, total: 100 }));
        let mut h = update_harness(app);
        h.run_steps(3);
        assert!(h.query_by_label("업데이트 다운로드 중").is_some());
        assert!(h.query_by_label("25%").is_some());
        assert!(!h.state().updates_rt.visible);
        assert!(h.state().message.is_none());
    }

    #[test]
    fn successful_install_shows_a_transient_notice_without_a_grid_or_modal() {
        use egui_kittest::kittest::Queryable;
        let mut app = SheetApp::new(Session::new(), Default::default());
        let notice = outcome_notice(
            &mut app.updates_rt,
            gridcraft_update::helper::Outcome { success: true, version: "0.20.0".into(), message: String::new() },
        );
        assert!(notice.is_none(), "successful updates must not open the message box");
        let mut h = update_harness(app);
        h.run_steps(3);
        assert!(h.query_by_label("Grid 0.20.0 업데이트가 완료되었습니다.").is_some());
        assert!(h.state().grid.rect.is_none());
        h.input_mut().time = Some(h.ctx.input(|i| i.time) + 3.0);
        h.step();
        assert!(h.query_by_label("Grid 0.20.0 업데이트가 완료되었습니다.").is_some());
        h.input_mut().time = Some(h.ctx.input(|i| i.time) + 1.1);
        h.step();
        assert!(h.query_by_label("Grid 0.20.0 업데이트가 완료되었습니다.").is_none());
    }

    #[test]
    fn failed_install_keeps_an_actionable_error() {
        let notice = outcome_notice(
            &mut Runtime::default(),
            gridcraft_update::helper::Outcome { success: false, version: "0.20.0".into(), message: "disk full".into() },
        );
        assert!(notice.is_some_and(|(_, text)| text.contains("disk full")));
    }

    #[test]
    fn unknown_download_size_and_reconnect_do_not_claim_zero_or_complete() {
        use egui_kittest::kittest::Queryable;
        let mut app = SheetApp::new(Session::new(), Default::default());
        receive(&mut app.updates_rt, Message::Progress(Transfer::Bytes { done: 0, total: 0 }));
        let mut h = update_harness(app);
        h.run_steps(3);
        assert!(h.query_by_label("업데이트 다운로드 준비 중").is_some());
        assert!(h.query_by_label("0%").is_none());
        receive(&mut h.state_mut().updates_rt, Message::Progress(Transfer::Reconnecting { done: 25, total: 100 }));
        h.run_steps(2);
        assert!(h.query_by_label("연결을 다시 시도하고 있습니다").is_some());
        receive(&mut h.state_mut().updates_rt, Message::Progress(Transfer::Bytes { done: 99_999, total: 100_000 }));
        h.run_steps(2);
        assert!(h.query_by_label("99%").is_some());
        assert!(h.query_by_label("100%").is_none());
        receive(&mut h.state_mut().updates_rt, Message::Progress(Transfer::Bytes { done: 100, total: 100 }));
        h.run_steps(2);
        assert!(h.query_by_label("업데이트 검증 중").is_some());
        assert!(h.query_by_label("100%").is_none());
        assert!(!h.state().updates_rt.visible);
    }

    fn update_harness(app: SheetApp) -> egui_kittest::Harness<'static, SheetApp> {
        egui_kittest::Harness::builder()
            .with_size(egui::vec2(800.0, 480.0))
            .with_step_dt(1.0 / 60.0)
            .with_context_setup(|ctx| {
                ctx.set_fonts(crate::theme::pinned_font_definitions());
                crate::theme::apply(ctx, false);
            })
            .build_ui_state(|ui, app| show(app, ui.ctx()), app)
    }

    fn sheet_harness(mut app: SheetApp) -> egui_kittest::Harness<'static, SheetApp> {
        app.use_prepared_fonts();
        egui_kittest::Harness::builder()
            .with_size(egui::vec2(1024.0, 720.0))
            .with_step_dt(1.0 / 60.0)
            .with_context_setup(|ctx| {
                ctx.set_fonts(crate::theme::pinned_font_definitions());
                crate::theme::apply(ctx, false);
            })
            .build_ui_state(
                |ui, app| {
                    app.logic(ui.ctx());
                    app.ui(ui);
                },
                app,
            )
    }

    #[test]
    fn restart_prompt_waits_until_cell_and_text_box_drafts_are_settled() {
        use egui_kittest::kittest::Queryable;
        for text_box in [false, true] {
            let mut session = Session::new();
            session.new_workbook();
            let mut app = SheetApp::new(session, Default::default());
            if text_box {
                let id = app.run("insert.textBox", serde_json::json!({"at": "B2", "text": "original"})).unwrap()["shape"].as_u64().unwrap() as u32;
                assert!(app.begin_text_box_edit(id));
                app.text_box_editor.as_mut().unwrap().text = "draft".into();
            } else {
                app.begin_edit(Some("draft".into()), false);
            }
            app.updates_rt.phase = Phase::Ready("0.20.0".into());
            app.updates_rt.visible = true;
            let mut h = sheet_harness(app);
            h.run_steps(3);
            assert!(h.query_by_label("재시작하여 업데이트").is_none(), "an update must wait for the active draft");
            assert!(h.query_by_label("업데이트 준비됨").is_some());
            if text_box {
                assert_eq!(h.state().text_box_editor.as_ref().unwrap().text, "draft");
                h.state_mut().commit_text_box_edit().unwrap();
            } else {
                assert_eq!(h.state().editor.as_ref().unwrap().text, "draft");
                assert!(h.state_mut().commit_edit(0, 0, false, false));
            }
            h.run_steps(3);
            assert!(h.query_by_label("재시작하여 업데이트").is_some());
        }
    }

    #[test]
    fn restart_modal_blocks_sheet_keys_and_escape_only_defers() {
        use egui_kittest::kittest::Queryable;
        let mut session = Session::new();
        session.new_workbook();
        let mut app = SheetApp::new(session, Default::default());
        app.run("cell.set", serde_json::json!({"cell": "A1", "input": "keep me"})).unwrap();
        app.updates_rt.phase = Phase::Ready("0.20.0".into());
        app.updates_rt.visible = true;
        let mut h = sheet_harness(app);
        h.run_steps(4);
        if let Ok(dir) = std::env::var("GRID_UPDATE_UI_SCREENSHOTS") {
            h.render().unwrap().save(format!("{dir}/restart-in-app.png")).unwrap();
        }
        h.input_mut().events.extend([
            egui::Event::Text("oops".into()),
            egui::Event::Paste("oops".into()),
            egui::Event::PasteImage(std::sync::Arc::new(egui::ColorImage::filled([1, 1], egui::Color32::RED))),
        ]);
        h.key_press(egui::Key::Delete);
        h.run_steps(2);
        assert!(h.state().editor.is_none());
        assert_eq!(h.state_mut().run("cell.get", serde_json::json!({"cell": "A1"})).unwrap()["input"], "keep me");
        assert!(h.state().session.active().unwrap().wb.active().unwrap().images.is_empty(), "image paste must not reach the sheet behind the modal");
        h.key_press(egui::Key::Escape);
        h.run_steps(3);
        assert!(!h.state().updates_rt.visible);
        assert!(h.query_by_label("업데이트 준비됨").is_some());
        assert!(!h.state().allow_close);
        assert!(!h.state().updates_rt.restart_requested);
    }

    #[test]
    fn update_feedback_follows_light_and_dark_themes() {
        use egui_kittest::kittest::Queryable;
        for dark in [false, true] {
            let mut app = SheetApp::new(Session::new(), Default::default());
            app.updates_rt.phase = Phase::Ready("0.20.0".into());
            app.updates_rt.visible = true;
            let mut h = update_harness(app);
            crate::theme::apply(&h.ctx, dark);
            h.run_steps(3);
            assert!(h.query_by_label("재시작하여 업데이트").is_some());
            if let Ok(dir) = std::env::var("GRID_UPDATE_UI_SCREENSHOTS") {
                let theme = if dark { "dark" } else { "light" };
                h.render().unwrap().save(format!("{dir}/restart-{theme}.png")).unwrap();
                h.state_mut().updates_rt.visible = false;
                receive(&mut h.state_mut().updates_rt, Message::Progress(Transfer::Bytes { done: 29_600_000, total: 80_000_000 }));
                h.run_steps(4);
                h.render().unwrap().save(format!("{dir}/progress-{theme}.png")).unwrap();
                h.state_mut().updates_rt.phase = Phase::Idle;
                outcome_notice(
                    &mut h.state_mut().updates_rt,
                    gridcraft_update::helper::Outcome { success: true, version: "0.20.0".into(), message: String::new() },
                );
                h.run_steps(3);
                h.render().unwrap().save(format!("{dir}/completed-{theme}.png")).unwrap();
            }
        }
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
