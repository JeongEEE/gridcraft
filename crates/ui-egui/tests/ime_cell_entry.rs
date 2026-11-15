//! Typing through an IME must start the selected cell's editor without losing its first syllable.

mod common;

use egui::{Event, ImeEvent, Key};
use gridcraft_engine::{
    Session,
    core::{CellRef, Value},
};
use serde_json::json;

fn selected_cell() -> common::Harness {
    let mut session = Session::new();
    session.new_workbook();
    common::os_harness(session, egui::os::OperatingSystem::Mac)
}

fn preedit(h: &mut common::Harness, text: &str) {
    h.input_mut().events.push(Event::Ime(ImeEvent::Preedit { text: text.into(), active_range_chars: None }));
    h.run_steps(2);
}

#[test]
fn selected_grid_enables_native_ime_before_the_first_keystroke() {
    let h = selected_cell();
    assert!(h.state().editor.is_none());
    assert!(h.output().platform_output.ime.is_some(), "a selected cell must accept composition before an editor exists");
}

#[test]
fn ime_commit_without_preedit_starts_a_cell_edit() {
    let mut h = selected_cell();
    h.input_mut().events.push(Event::Ime(ImeEvent::Commit("한".into())));
    h.run_steps(2);
    assert_eq!(h.state().editor.as_ref().map(|e| e.text.as_str()), Some("한"));
}

#[test]
fn opening_cell_editor_does_not_cancel_the_native_composition() {
    let mut h = selected_cell();
    h.input_mut().events.push(Event::Ime(ImeEvent::Preedit { text: "ㅎ".into(), active_range_chars: None }));
    h.run_steps(1);
    let ime = h.output().platform_output.ime.as_ref().expect("the editor keeps IME enabled");
    assert!(!ime.should_interrupt_composition, "opening the editor must preserve the native composition that opened it");
    assert_eq!(h.state().editor.as_ref().map(|e| e.text.as_str()), Some("ㅎ"));
}

#[test]
fn cancelling_cell_edit_still_cancels_the_native_composition() {
    let mut h = selected_cell();
    preedit(&mut h, "한");
    // Observe the key-down frame; Harness::key_press also runs a separate key-up frame.
    h.input_mut().events.push(Event::Key { key: Key::Escape, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::NONE });
    h.run_steps(1);
    assert!(h.state().editor.is_none());
    assert!(h.output().platform_output.ime.as_ref().unwrap().should_interrupt_composition);
}

#[test]
fn ime_entry_respects_sheet_protection() {
    let mut h = selected_cell();
    h.state_mut().run("review.protectSheet", json!({})).unwrap();
    preedit(&mut h, "한");
    assert!(h.state().editor.is_none());
    assert!(h.state().message.is_some());
    assert_eq!(h.state().session.active().unwrap().wb.active().unwrap().value(CellRef::new(0, 0)), Value::Empty);
}

#[test]
fn windows_preedit_handoff_preserves_the_native_composition() {
    let mut session = Session::new();
    session.new_workbook();
    let mut h = common::os_harness(session, egui::os::OperatingSystem::Windows);
    h.input_mut().events.push(Event::Ime(ImeEvent::Preedit { text: "한".into(), active_range_chars: None }));
    h.run_steps(1);
    assert!(!h.output().platform_output.ime.as_ref().unwrap().should_interrupt_composition);
    h.input_mut().events.push(Event::Ime(ImeEvent::Commit("한".into())));
    h.run_steps(2);
    h.key_press(Key::Enter);
    h.run_steps(2);
    assert_eq!(h.state().session.active().unwrap().wb.active().unwrap().value(CellRef::new(0, 0)), Value::text("한"));
}

#[test]
fn macos_surrounding_text_replacement_preserves_both_korean_syllables() {
    let mut h = selected_cell();
    // macOS can commit each intermediate syllable, replacing its previous UTF-8 bytes.
    for (before_bytes, text) in [(0, "ㅎ"), (3, "하"), (3, "한"), (0, "ㄱ"), (3, "그"), (3, "글")] {
        h.input_mut()
            .events
            .extend([Event::Ime(ImeEvent::DeleteSurroundingBytes { before_bytes, after_bytes: 0 }), Event::Ime(ImeEvent::Commit(text.into()))]);
        h.run_steps(2);
    }
    h.key_press(Key::Enter);
    h.run_steps(2);
    assert_eq!(h.state().session.active().unwrap().wb.active().unwrap().value(CellRef::new(0, 0)), Value::text("한글"));
}

#[test]
fn fresh_ime_edit_replaces_old_cell_and_does_not_reuse_previous_composition() {
    let mut h = selected_cell();
    preedit(&mut h, "가");
    h.key_press(Key::Escape);
    h.run_steps(2);
    h.state_mut().session.execute("cell.set", json!({"cell": "A1", "input": "old value"})).unwrap();
    h.run_steps(2);
    preedit(&mut h, "한");
    h.input_mut().events.push(Event::Ime(ImeEvent::Commit("한".into())));
    h.run_steps(2);
    h.key_press(Key::Enter);
    h.run_steps(2);
    assert_eq!(h.state().session.active().unwrap().wb.active().unwrap().value(CellRef::new(0, 0)), Value::text("한"));
}

#[test]
fn empty_ime_notifications_do_not_start_an_edit() {
    let mut h = selected_cell();
    preedit(&mut h, "");
    h.input_mut().events.push(Event::Ime(ImeEvent::Commit(String::new())));
    h.run_steps(2);
    assert!(h.state().editor.is_none());
}

#[test]
fn latin_first_character_is_not_duplicated() {
    let mut h = selected_cell();
    h.input_mut().events.push(Event::Text("hello".into()));
    h.run_steps(2);
    h.key_press(Key::Enter);
    h.run_steps(2);
    assert_eq!(h.state().session.active().unwrap().wb.active().unwrap().value(CellRef::new(0, 0)), Value::text("hello"));
}

#[test]
fn korean_composition_starts_in_selected_cell_and_keeps_first_syllable() {
    let mut h = selected_cell();
    for text in ["ㅎ", "하", "한"] {
        preedit(&mut h, text);
        assert_eq!(h.state().editor.as_ref().map(|e| e.text.as_str()), Some(text));
    }
    h.input_mut().events.push(Event::Ime(ImeEvent::Commit("한".into())));
    h.run_steps(2);
    for text in ["ㄱ", "그", "글"] {
        preedit(&mut h, text);
        assert_eq!(h.state().editor.as_ref().map(|e| e.text.as_str()), Some(format!("한{text}").as_str()));
    }
    h.input_mut().events.push(Event::Ime(ImeEvent::Commit("글".into())));
    h.run_steps(2);
    h.key_press(Key::Enter);
    h.run_steps(2);
    assert!(h.state().editor.is_none());
    assert_eq!(h.state().session.active().unwrap().wb.active().unwrap().value(CellRef::new(0, 0)), Value::text("한글"));
}
