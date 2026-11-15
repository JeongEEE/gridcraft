//! Grid's display-only numeric fit keeps complete numbers inside their own columns.
use egui::{Color32, Shape};
use gridcraft_engine::{Session, core::CellRef};
use gridcraft_ui_egui::{SheetApp, UiState};
use serde_json::json;

type Harness = egui_kittest::Harness<'static, SheetApp>;
const ACCOUNTING: &str = r#"_-* #,##0_-;[Red]\-* #,##0_-;_-* "-"_-;_-@_-"#;

fn fixture(code: &str, value: serde_json::Value, shrink: bool, mode: &str, zoom: u16) -> Harness {
    let mut s = Session::new();
    s.new_workbook();
    s.execute("range.setValues", json!({"range":"A1", "values":[[value]]})).unwrap();
    s.execute("home.formatCells", json!({"range":"A1", "style":{"num_fmt":code,"font":{"size":14.0},"align":{"shrink":shrink}}})).unwrap();
    s.execute("home.columnWidth", json!({"cols":"A:A", "width":70})).unwrap();
    s.execute("view.zoom", json!({"percent":zoom})).unwrap();
    if mode == "merged" {
        s.execute("home.mergeCells", json!({"range":"A1:B1"})).unwrap();
        s.execute("home.columnWidth", json!({"cols":"A:B", "width":38})).unwrap();
    } else if mode == "page" {
        s.execute("view.pageLayout", json!({})).unwrap();
    }
    s.execute("selection.set", json!({"range":"H20"})).unwrap();
    crate::common::harness_with(crate::common::builder().with_size(egui::vec2(1200., 900.)), crate::common::app(s), 4)
}

fn cell(h: &Harness) -> egui::Rect {
    let app = h.state();
    let sh = app.session.active().unwrap().wb.active().unwrap();
    let geo = app.grid.geo.as_ref().unwrap();
    let mut r = geo.cell_rect(sh, CellRef::new(0, 0));
    if let Some(m) = sh.merges.first() {
        r.max.x = geo.cell_rect(sh, m.end).right();
    }
    r
}

fn fitting_number(h: &Harness, value: &str) {
    let rect = cell(h);
    let found: Vec<_> = h
        .output()
        .shapes
        .iter()
        .filter_map(|s| match &s.shape {
            Shape::Text(t)
                if t.galley.job.text.split_whitespace().collect::<String>().contains(value)
                    && t.pos.y >= rect.top() - 1.
                    && t.pos.y < rect.bottom() =>
            {
                Some(t)
            }
            _ => None,
        })
        .collect();
    assert!(
        !found.is_empty(),
        "complete number {value} is painted in {rect:?}; shapes={:?}",
        h.output()
            .shapes
            .iter()
            .filter_map(|s| match &s.shape {
                Shape::Text(t) if t.galley.job.text.contains('1') || t.galley.job.text.starts_with('#') => Some((&t.galley.job.text, t.pos)),
                _ => None,
            })
            .collect::<Vec<_>>()
    );
    for t in found {
        assert!(
            t.pos.x >= rect.left() && t.pos.x + t.galley.size().x <= rect.right() + 0.01,
            "whole number stays in its own column: {:?}",
            t.galley.job.text
        );
    }
}

fn has_hashes(h: &Harness) -> bool {
    h.output().shapes.iter().any(|s| matches!(&s.shape,Shape::Text(t) if t.galley.job.text.starts_with("##")))
}

#[test]
fn numbers_and_accounting_fit_by_default_at_multiple_zooms_and_views() {
    for code in ["#,##0", ACCOUNTING] {
        for mode in ["normal", "merged", "page"] {
            for zoom in [90, 100, 200] {
                let h = fixture(code, json!(123456789012_i64), false, mode, zoom);
                fitting_number(&h, "123,456,789,012");
                assert!(!has_hashes(&h));
            }
        }
    }
}

#[test]
fn general_numbers_are_not_rounded_to_the_column_before_shrinking() {
    let h = fixture("General", json!(12345.6789), false, "normal", 100);
    fitting_number(&h, "12345.6789");
}

#[test]
fn turning_it_off_restores_hashes_without_editing_the_workbook() {
    let mut h = fixture("#,##0", json!(123456789012_i64), false, "normal", 100);
    let before = serde_json::to_value(&h.state().session.active().unwrap().wb).unwrap();
    let dirty = h.state().session.active().unwrap().is_dirty();
    let undo = h.state().session.active().unwrap().undo.len();
    assert_eq!(h.state_mut().run("options.autoFitNumbers", json!({"on":false})), Ok(json!({"on":false})));
    h.run_steps(3);
    assert!(has_hashes(&h));
    h.state_mut().run("options.autoFitNumbers", json!({})).unwrap();
    h.run_steps(3);
    fitting_number(&h, "123,456,789,012");
    assert_eq!(serde_json::to_value(&h.state().session.active().unwrap().wb).unwrap(), before);
    assert_eq!(h.state().session.active().unwrap().is_dirty(), dirty);
    assert_eq!(h.state().session.active().unwrap().undo.len(), undo);
}

#[test]
fn explicit_shrink_also_fits_accounting_with_the_preference_off() {
    let mut h = fixture(ACCOUNTING, json!(123456789012_i64), true, "normal", 100);
    h.state_mut().run("options.autoFitNumbers", json!({"on":false})).unwrap();
    h.run_steps(3);
    fitting_number(&h, "123,456,789,012");
    assert!(!has_hashes(&h));
}

#[test]
fn negative_sign_format_colour_and_invalid_date_hashes_survive() {
    let h = fixture(ACCOUNTING, json!(-123456789012_i64), false, "normal", 100);
    fitting_number(&h, "-123,456,789,012");
    assert!(
        h.output()
            .shapes
            .iter()
            .any(|s| matches!(&s.shape,Shape::Text(t) if t.galley.job.text.contains("123,456,789,012") && t.fallback_color==Color32::RED))
    );
    let h = fixture("m/d/yyyy", json!(-1), false, "normal", 100);
    assert!(has_hashes(&h), "invalid dates must remain invalid");
}

#[test]
fn ordinary_text_still_spills_into_empty_neighbours() {
    let text = "A label extending into an empty neighbouring column";
    let h = fixture("General", json!(text), false, "normal", 100);
    let rect = cell(&h);
    assert!(
        h.output().shapes.iter().any(
            |s| matches!(&s.shape,Shape::Text(t) if t.galley.job.text==text && s.clip_rect.right()>rect.right() && t.galley.size().x>rect.width())
        )
    );
}

#[test]
fn preference_defaults_on_and_keeps_an_explicit_off_across_restart() {
    assert_eq!(serde_json::to_value(UiState::default()).unwrap()["autoFitNumbers"], true);
    let old: UiState = serde_json::from_str("{}").unwrap();
    assert_eq!(serde_json::to_value(old).unwrap()["autoFitNumbers"], true);
    let off: UiState = serde_json::from_value(json!({"autoFitNumbers":false})).unwrap();
    let restored: UiState = serde_json::from_str(&serde_json::to_string(&off).unwrap()).unwrap();
    assert_eq!(serde_json::to_value(restored).unwrap()["autoFitNumbers"], false);
}

#[test]
fn an_impossibly_narrow_column_never_clips_a_partial_number_as_the_amount() {
    let mut h = fixture("#,##0", json!(123456789012_i64), false, "normal", 100);
    h.state_mut().session.execute("home.columnWidth", json!({"cols":"A:A","width":8})).unwrap();
    h.run_steps(3);
    let rect = cell(&h);
    let amounts: Vec<_> = h
        .output()
        .shapes
        .iter()
        .filter_map(|s| match &s.shape {
            Shape::Text(t)
                if t.pos.y >= rect.top() && t.pos.y < rect.bottom() && t.pos.x < rect.right() && t.pos.x + t.galley.size().x > rect.left() =>
            {
                Some(&t.galley.job.text)
            }
            _ => None,
        })
        .collect();
    assert!(!amounts.is_empty());
    assert!(amounts.iter().all(|s| s.chars().all(|c| c == '#')), "unreadable amounts need an overflow indicator, not clipped digits: {amounts:?}");
}

#[test]
fn the_korean_options_checkbox_changes_the_display_preference() {
    use egui_kittest::kittest::Queryable as _;
    let mut h = fixture("#,##0", json!(123456789012_i64), false, "normal", 100);
    h.state_mut().ui.language = gridcraft_ui_egui::i18n::Language::Ko;
    h.state_mut().run("ui.backstage", json!({"page":"options"})).unwrap();
    h.run_steps(3);
    h.get_by_role_and_label(egui::accesskit::Role::CheckBox, "넘치는 숫자를 셀에 맞게 자동 축소").click();
    h.run_steps(3);
    assert!(!h.state().ui.auto_fit_numbers);
    h.state_mut().run("ui.backstage", json!({"page":"close"})).unwrap();
    h.run_steps(3);
    assert!(has_hashes(&h));
}

#[test]
fn accounting_fill_reserves_the_cells_indent_before_expanding_spaces() {
    let mut h = fixture(ACCOUNTING, json!(123456789012_i64), false, "normal", 100);
    h.state_mut().session.execute("home.formatCells", json!({"range":"A1","style":{"align":{"indent":2}}})).unwrap();
    h.run_steps(3);
    fitting_number(&h, "123,456,789,012");
}
