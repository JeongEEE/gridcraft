//! macOS: dragging the empty title bar moves the window by positioning it. `StartDrag` did
//! nothing there, so the window could not be moved at all.
#![cfg(target_os = "macos")]

use egui::{Event, Modifiers, PointerButton, ViewportCommand, ViewportId, pos2, vec2};
use gridcraft_engine::Session;
use gridcraft_ui_egui::SheetApp;

type Harness = egui_kittest::Harness<'static, SheetApp>;

fn harness() -> Harness {
    let mut session = Session::new();
    session.new_workbook();
    let mut h = crate::common::builder().with_size(vec2(1200.0, 800.0)).build_ui_state(
        |ui, app: &mut SheetApp| {
            app.logic(&ui.ctx().clone());
            app.ui(ui);
        },
        crate::common::app(session),
    );
    h.run_steps(6);
    h
}

/// One frame with `events`, returning the commands sent to the window.
fn step(h: &mut Harness, events: Vec<Event>) -> Vec<ViewportCommand> {
    // The window sits at (300, 200) on the screen; the harness reports no position of its own.
    h.input_mut().viewports.entry(ViewportId::ROOT).or_default().outer_rect =
        Some(egui::Rect::from_min_size(pos2(300.0, 200.0), vec2(1200.0, 800.0)));
    h.input_mut().events.extend(events);
    h.step();
    h.output().viewport_output.get(&ViewportId::ROOT).map(|v| v.commands.clone()).unwrap_or_default()
}

fn button(pos: egui::Pos2, pressed: bool) -> Event {
    Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE }
}

#[test]
fn dragging_the_empty_title_bar_moves_the_window_under_the_pointer() {
    let mut h = harness();
    // Between the AutoSave/undo group and the centred title: nothing there but the bar.
    let start = pos2(450.0, 14.0);
    let mut commands = step(&mut h, vec![Event::PointerMoved(start)]);
    commands.extend(step(&mut h, vec![button(start, true)]));
    for i in 1..=4 {
        commands.extend(step(&mut h, vec![Event::PointerMoved(start + vec2(15.0, 10.0) * i as f32)]));
    }
    commands.extend(step(&mut h, vec![button(start + vec2(60.0, 40.0), false)]));
    assert!(!commands.contains(&ViewportCommand::StartDrag), "StartDrag does not move a macOS window: {commands:?}");
    // The pressed point stays under the pointer: the window goes where the pointer went.
    assert_eq!(commands.last(), Some(&ViewportCommand::OuterPosition(pos2(360.0, 240.0))), "{commands:?}");
}
