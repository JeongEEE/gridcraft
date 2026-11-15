use super::geo::Geo;
use super::menus::is_checkbox;
use super::{CellRef, Drag, Sheet, SheetApp, json};

/// Grid keyboard handling when no text field has focus.
pub(super) fn keyboard(app: &mut SheetApp, ctx: &egui::Context, resp: &egui::Response, geo: &Geo, sh: &Sheet) {
    // Focus left on an editor that no longer exists goes back to the grid.
    // The idle grid and the in-cell editor share an input owner. Changing owners when the
    // first preedit arrives asks egui to cancel the very native composition opening the edit.
    let input_id = egui::Id::unique("gridcraft.cell_editor");
    let stale = [egui::Id::unique("gridcraft.formula_bar"), crate::text_box::editor_id()];
    let editing = app.editor.is_some() || app.text_box_editor.is_some();
    if !editing && ctx.memory(|m| m.focused()).is_some_and(|f| stale.contains(&f)) {
        ctx.memory_mut(|m| m.request_focus(input_id));
    }
    let other_focus = ctx.memory(|m| m.focused()).is_some_and(|f| f != resp.id && (editing || (f != input_id && !stale.contains(&f))));
    // An open filter menu owns the keyboard even when a click inside it left focus on nothing (#1516).
    // So do the fill menu (its Esc closes it, not the copy) and the right-drag that opens it (an
    // arrow key would move the selection the menu fills from), and a macro's question.
    if editing
        || app.dialog.is_some()
        || other_focus
        || app.message.is_some()
        || app.grid.filter_menu.is_some()
        || app.grid.fill_menu.is_some()
        || matches!(app.grid.drag, Drag::Fill { menu: true, .. })
        || app.macros.question.is_some()
    {
        return;
    }
    if ctx.memory(|m| m.focused()).is_none_or(|f| f == resp.id) {
        ctx.memory_mut(|m| m.request_focus(input_id));
    }
    // The grid owns arrows, Tab and Escape (egui would otherwise move focus between widgets).
    ctx.memory_mut(|m| {
        m.set_focus_lock_filter(input_id, egui::EventFilter { tab: true, horizontal_arrows: true, vertical_arrows: true, escape: true });
        // A click can return focus just before this key, before the lock took effect. Cancel
        // any UI-navigation request already queued for the key the sheet is handling now.
        m.move_focus(egui::FocusDirection::None);
    });
    enable_grid_ime(app, ctx, geo, sh, input_id);
    let events = grid_events(app, ctx, sh);
    // A shortcut key that also types a character (Shift+Space sends Text(" ")) must not start an edit.
    let shortcut_pressed = events.iter().any(|e| {
        matches!(e, egui::Event::Key { pressed: true, modifiers: m, key, .. }
            if !m.alt && (m.ctrl || m.command || (m.shift && *key == egui::Key::Space)))
    });
    let page = ((geo.cells.height() / geo.z) / sh.default_row_height).floor().max(1.0) as i64;
    // Alt shortcuts type their character too on Windows and Linux (Alt+= sends Text("=")): once
    // a shortcut ran, that text must not start an edit over what it just did.
    let mut shortcut_ran = false;
    for ev in events {
        match ev {
            egui::Event::Copy => {
                app.copy_to_clipboard(ctx, "edit.copy");
            }
            egui::Event::Cut => {
                app.copy_to_clipboard(ctx, "edit.cut");
            }
            egui::Event::Paste(text) => app.paste_external(text),
            // A picture with no text on the clipboard (a screenshot, a browser image) is pasted as
            // a floating picture at the active cell, as Excel does.
            egui::Event::PasteImage(image) => app.paste_color_image(&image),
            // PRIMARY middle-click paste belongs to egui's TextEdit; the grid has no caret to
            // insert text at, so a middle click on a cell is deliberately not a paste.
            egui::Event::MiddleClickPaste { .. } => {}
            egui::Event::Text(_) if shortcut_pressed || shortcut_ran => {
                // A shortcut may itself open a focused editor (AutoSum). Its typed character
                // must not be replayed into that new draft later in the same frame.
                ctx.input_mut(|i| i.events.retain(|e| !matches!(e, egui::Event::Text(_))));
            }
            egui::Event::Ime(egui::ImeEvent::Preedit { text, .. } | egui::ImeEvent::Commit(text))
                if !text.is_empty() && text.chars().all(|c| !c.is_control()) =>
            {
                begin_ime_edit(app, ctx);
                return;
            }
            egui::Event::Text(text) if text.chars().all(|c| !c.is_control()) && !text.is_empty() => {
                begin_text_edit(app, ctx, text);
                return;
            }
            egui::Event::Key { key, physical_key, pressed: true, modifiers: m, .. } => {
                use egui::Key;
                if crate::keymap::dispatch(app, ctx, key, physical_key, m, geo, sh) {
                    shortcut_ran = true;
                    continue;
                }
                let cmd = m.command;
                let shift = m.shift;
                let mv = |app: &mut SheetApp, dr: i64, dc: i64| {
                    let from = app.session.active().map(|d| if shift { d.selection.moving_end() } else { d.selection.active });
                    let _ = app.session.run("selection.move", json!({"dr": dr, "dc": dc, "extend": shift, "jump": cmd}));
                    app.grid.ensure_visible = true;
                    app.grid.reveal_moving_end = shift;
                    app.grid.reveal_from = from;
                };
                match key {
                    Key::ArrowUp => mv(app, -1, 0),
                    Key::ArrowDown => mv(app, 1, 0),
                    // Arrows follow the screen: on a right-to-left sheet Left goes to the next column.
                    Key::ArrowLeft => mv(app, 0, if sh.right_to_left { 1 } else { -1 }),
                    Key::ArrowRight => mv(app, 0, if sh.right_to_left { -1 } else { 1 }),
                    Key::PageDown if !cmd => mv(app, page, 0),
                    Key::PageUp if !cmd => mv(app, -page, 0),
                    Key::PageDown => app.run_or_alert("sheet.next", json!({})),
                    Key::PageUp => app.run_or_alert("sheet.previous", json!({})),
                    Key::Home if cmd => {
                        let r = sh.freeze.map(|f| f.0).unwrap_or(0);
                        let c = sh.freeze.map(|f| f.1).unwrap_or(0);
                        let _ = app.session.run("selection.set", json!({"cell": CellRef::new(r, c).a1()}));
                        app.grid.ensure_visible = true;
                    }
                    Key::End if cmd => {
                        let end = sh.used_range().map(|u| u.end).unwrap_or_default();
                        let _ = app.session.run("selection.set", json!({"cell": end.a1()}));
                        app.grid.ensure_visible = true;
                    }
                    Key::Home => {
                        let a = app.session.active().map(|d| d.selection.active).unwrap_or_default();
                        let _ = app.session.run("selection.set", json!({"cell": CellRef::new(a.row, 0).a1()}));
                        app.grid.ensure_visible = true;
                    }
                    // Excel: Enter while a copied range is marked pastes it and ends copy mode.
                    Key::Enter if !m.alt && !m.ctrl && !shift && !cmd && app.session.clipboard.is_some() => {
                        app.run_or_alert("edit.paste", json!({}));
                        app.session.clipboard = None;
                        app.grid.ensure_visible = true;
                    }
                    Key::Enter => {
                        if m.alt || m.ctrl {
                            app.begin_edit(None, false);
                        } else {
                            app.move_after_enter(if shift { -1 } else { 1 }, 0);
                        }
                    }
                    Key::Tab => app.move_after_enter(0, if shift { -1 } else { 1 }),
                    Key::F2 => app.begin_edit(None, false),
                    // With an object selected, Delete and Backspace delete it, not the cells.
                    Key::Delete if app.delete_selected_object() => {}
                    Key::Delete => app.run_or_alert("edit.clearContents", json!({})),
                    Key::Backspace if app.delete_selected_object() => {}
                    Key::Backspace => {
                        // Excel for Mac: Delete clears the cell and starts editing it.
                        app.run_or_alert("edit.clearContents", json!({}));
                        if app.session.active().is_some_and(|d| d.selection.is_single_cell()) {
                            app.begin_edit(Some(String::new()), false);
                        }
                    }
                    Key::Escape if app.selected_object.is_some() => app.selected_object = None,
                    Key::Escape => {
                        app.session.clipboard = None;
                        app.session.format_painter = None;
                    }
                    Key::Space if !cmd && !shift && is_checkbox(sh, app) => app.run_or_alert("cell.toggleCheckbox", json!({})),
                    Key::F8 if m.alt => {
                        app.open_dialog("macros", json!({}));
                    }
                    Key::F9 => app.run_or_alert("formulas.calculateNow", json!({})),
                    Key::F4 if !cmd => {}
                    // Ctrl+Shift+letter (⌘⇧letter) runs the macro with that shortcut, unless GridCraft uses it.
                    _ if cmd && shift && !m.alt && crate::macro_ui::run_shortcut(app, key) => {}
                    _ if cmd && !crate::keymap::altgr_text(ctx, m, ctx.os() == egui::os::OperatingSystem::Mac) => {
                        crate::ribbon::shortcut(app, key, m)
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
}

/// A selected cell accepts text before its editor exists. Enabling the native IME only after
/// the first Text event can cancel Korean's first syllable.
fn enable_grid_ime(app: &SheetApp, ctx: &egui::Context, geo: &Geo, sh: &Sheet, input_id: egui::Id) {
    let generation_id = input_id.with("ime_generation");
    let pass = ctx.cumulative_pass_nr().max(1);
    let gained_focus = ctx.memory(|m| m.has_focus(input_id) && !m.had_focus_last_frame(input_id));
    let generation = ctx.data_mut(|d| {
        if gained_focus || d.get_temp::<u64>(generation_id).is_none() {
            d.insert_temp(generation_id, pass);
        }
        d.get_temp::<u64>(generation_id).unwrap_or(1)
    });
    if let Some(doc) = app.session.active() {
        let rect = geo.mirror_rect(geo.cell_rect(sh, doc.selection.active));
        ctx.output_mut(|o| {
            o.ime = Some(egui::output::IMEOutput::new(input_id, generation, egui::IMEPurpose::Normal, rect, rect));
        });
    }
}

fn begin_ime_edit(app: &mut SheetApp, ctx: &egui::Context) {
    app.begin_edit(Some(String::new()), false);
    if let Some(ed) = app.editor.as_mut() {
        // TextEdit owns the original IME input. Seeding text or resetting its cursor after
        // processing it would destroy the preedit selection.
        let id = egui::Id::unique("gridcraft.cell_editor");
        egui::text_edit::TextEditState::default().store(ctx, id);
        ctx.memory_mut(|m| m.request_focus(id));
        ed.request_focus = false;
        ed.sync_caret = false;
    }
}

fn begin_text_edit(app: &mut SheetApp, ctx: &egui::Context, text: String) {
    app.begin_edit(Some(text.clone()), false);
    if let Some(ed) = app.editor.as_mut() {
        ed.caret = ed.text.chars().count();
        // Preserve ordinary entry's initial completion/caret semantics, but spend the seeded
        // event: the shared input owner is already focused this frame.
        ctx.input_mut(|i| {
            if let Some(index) = i.events.iter().position(|e| matches!(e, egui::Event::Text(t) if t == &text)) {
                i.events.remove(index);
            }
        });
    }
}

/// This frame's input events, without those the focused form control took.
fn grid_events(app: &mut SheetApp, ctx: &egui::Context, sh: &Sheet) -> Vec<egui::Event> {
    let events = ctx.input(|i| i.events.clone());
    events.into_iter().filter(|ev| !form_control_event(app, sh, ev)).collect()
}

/// Whether the focused form control takes `ev`: arrows, Page Up and Down, Home, End, Space and
/// Enter without Ctrl, Cmd or Alt, and the space a Space press types when it must not start a
/// cell edit: for a button, which Space presses, and on a checkbox cell, which the Space key
/// toggles (a slider or spin button doesn't take Space, so Space on a cell does what it does with
/// no control focused: it toggles a checkbox cell, or its space starts a cell edit like any other
/// character). A key press the control doesn't take goes to the grid, and a focused button gives
/// the keyboard back with it.
fn form_control_event(app: &mut SheetApp, sh: &Sheet, ev: &egui::Event) -> bool {
    match ev {
        egui::Event::Text(text) => {
            text == " "
                && app.selected_object.is_none()
                && app.grid.focused_control.is_some()
                && (crate::form_controls::focused_button(app, sh) || is_checkbox(sh, app))
        }
        egui::Event::Key { key, pressed: true, repeat, modifiers: m, .. } => {
            if !m.command && !m.ctrl && !m.alt && crate::form_controls::key(app, sh, *key, *repeat) {
                return true;
            }
            crate::form_controls::key_to_grid(app, sh);
            false
        }
        _ => false,
    }
}
