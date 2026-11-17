//! Presentation only: compact background status, an explicit restart prompt, and a short toast.
use super::{Phase, Runtime, VERSION};
use crate::{
    icons::{self, Icon},
    theme::{self, Tokens},
};
use egui::{Align, Align2, RichText, Sense, Stroke, vec2};
use std::time::Duration;

#[derive(Default)]
pub(super) struct Response {
    pub restart: bool,
    pub releases: bool,
    pub modal: bool,
}

fn card_frame(ctx: &egui::Context) -> egui::Frame {
    let t = Tokens::get(ctx);
    egui::Frame::popup(&ctx.global_style()).fill(t.window).stroke(Stroke::new(1.0, t.separator)).corner_radius(12).inner_margin(16)
}

fn badge(ui: &mut egui::Ui, icon: Icon, size: f32) {
    let t = Tokens::get(ui.ctx());
    let (rect, _) = ui.allocate_exact_size(vec2(size, size), Sense::hover());
    ui.painter().rect_filled(rect, 10.0, t.accent_soft);
    icons::paint(ui.painter(), rect.shrink(size * 0.24), icon, t.accent);
}

fn progress(ui: &mut egui::Ui, phase: &Phase) {
    let t = Tokens::get(ui.ctx());
    let (title, fraction, subtitle) = match phase {
        Phase::Checking => ("업데이트 확인 중", None, "새 버전을 확인하고 있습니다".into()),
        Phase::Downloading { reconnecting: true, .. } => ("업데이트 다운로드 중", None, "연결을 다시 시도하고 있습니다".into()),
        Phase::Downloading { total: 0, .. } => ("업데이트 다운로드 준비 중", None, "작업을 계속하셔도 됩니다".into()),
        Phase::Downloading { done, total, .. } if done >= total => ("업데이트 검증 중", None, "다운로드한 파일을 확인하고 있습니다".into()),
        Phase::Downloading { done, total, .. } => (
            "업데이트 다운로드 중",
            Some((*done as f64 / *total as f64).clamp(0.0, 1.0) as f32),
            format!("{:.1} / {:.1} MB", *done as f64 / 1_000_000.0, *total as f64 / 1_000_000.0),
        ),
        _ => return,
    };
    ui.horizontal(|ui| {
        ui.add(egui::Spinner::new().size(16.0).color(t.accent));
        ui.label(RichText::new(title).strong().color(t.text));
        if let Some(fraction) = fraction {
            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                ui.label(RichText::new(format!("{}%", (fraction * 100.0).min(99.0) as u32)).color(t.accent));
            });
        }
    });
    ui.add_space(6.0);
    ui.label(RichText::new(subtitle).small().color(t.text_dim));
    if let Some(fraction) = fraction {
        ui.add_space(8.0);
        ui.add(egui::ProgressBar::new(fraction).desired_height(4.0).fill(t.accent));
    }
}

fn corner(rt: &mut Runtime, ctx: &egui::Context) {
    let ready = matches!(rt.phase, Phase::Ready(_));
    egui::Area::new(egui::Id::unique("grid.update.status"))
        .anchor(Align2::RIGHT_BOTTOM, vec2(-18.0, -44.0))
        .order(egui::Order::Foreground)
        .sense(Sense::hover())
        .interactable(ready)
        .show(ctx, |ui| {
            card_frame(ctx).show(ui, |ui| {
                if ready {
                    let t = Tokens::get(ctx);
                    ui.horizontal(|ui| {
                        badge(ui, Icon::Check, 28.0);
                        if ui.button(RichText::new("업데이트 준비됨").color(t.text)).on_hover_text("재시작 옵션 열기").clicked() {
                            rt.visible = true;
                        }
                    });
                } else {
                    ui.set_width(270.0);
                    progress(ui, &rt.phase);
                }
            });
        });
}

fn completed(rt: &mut Runtime, ctx: &egui::Context) {
    let Some((text, shown_at)) = &mut rt.completed else { return };
    let now = ctx.input(|i| i.time);
    let elapsed = now - *shown_at.get_or_insert(now);
    if elapsed >= 4.0 {
        rt.completed = None;
        return;
    }
    let opacity = ((4.0 - elapsed) / 0.25).clamp(0.0, 1.0) as f32;
    egui::Area::new(egui::Id::unique("grid.update.completed"))
        .anchor(Align2::RIGHT_TOP, vec2(-18.0, 48.0))
        .order(egui::Order::Tooltip)
        .sense(Sense::hover())
        .interactable(false)
        .show(ctx, |ui| {
            ui.set_opacity(opacity);
            card_frame(ctx).show(ui, |ui| {
                ui.set_max_width(320.0);
                ui.horizontal(|ui| {
                    badge(ui, Icon::Check, 28.0);
                    ui.label(RichText::new(text.as_str()).color(Tokens::get(ctx).text));
                });
            });
        });
    ctx.request_repaint_after(Duration::from_millis(if elapsed >= 3.75 { 16 } else { 100 }));
}

pub(super) fn show(rt: &mut Runtime, ctx: &egui::Context, prompt_allowed: bool) -> Response {
    completed(rt, ctx);
    let mut response = Response::default();
    if !rt.visible || !prompt_allowed {
        if matches!(rt.phase, Phase::Checking | Phase::Downloading { .. } | Phase::Ready(_)) {
            corner(rt, ctx);
        }
        return response;
    }
    if let Phase::Ready(version) = &rt.phase {
        let t = Tokens::get(ctx);
        let mut later = false;
        let modal = egui::Modal::new(egui::Id::unique("grid.fork.update.ready")).frame(card_frame(ctx).inner_margin(24)).show(ctx, |ui| {
            ui.set_width(376.0);
            ui.horizontal(|ui| {
                badge(ui, Icon::Check, 48.0);
                ui.add_space(6.0);
                ui.vertical(|ui| {
                    ui.label(RichText::new("업데이트 준비 완료").font(theme::ui_bold(20.0)).color(t.text));
                    ui.label(RichText::new(format!("Grid {VERSION}  →  {version}")).color(t.text_dim));
                });
            });
            ui.add_space(18.0);
            ui.label(RichText::new("새 버전을 사용할 준비가 되었습니다.\n지금 Grid를 재시작할까요?").color(t.text));
            ui.add_space(12.0);
            egui::Frame::NONE.fill(t.accent_soft).corner_radius(8).inner_margin(12).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new("저장하지 않은 문서는 먼저 확인하고,\n열려 있던 파일은 재시작 후 다시 엽니다.").small().color(t.text));
            });
            ui.add_space(20.0);
            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                response.restart = ui
                    .add_sized(
                        vec2(178.0, 36.0),
                        egui::Button::new(RichText::new("재시작하여 업데이트").strong().color(t.on_accent)).fill(t.accent).corner_radius(7),
                    )
                    .clicked();
                later = ui.add_sized(vec2(84.0, 36.0), egui::Button::new("나중에").corner_radius(7)).clicked();
            });
        });
        response.modal = true;
        if later || modal.should_close() {
            rt.visible = false;
        }
        return response;
    }
    let mut open = true;
    egui::Window::new("Grid 업데이트").id(egui::Id::unique("grid.fork.update")).collapsible(false).resizable(false).open(&mut open).show(ctx, |ui| {
        ui.set_max_width(440.0);
        match &rt.phase {
            Phase::Idle => {
                ui.label("업데이트를 확인할 수 있습니다.");
            }
            Phase::Checking | Phase::Downloading { .. } => progress(ui, &rt.phase),
            Phase::Current => {
                ui.label(format!("Grid {VERSION} — 최신 버전입니다."));
            }
            Phase::Manual { version, reason } => {
                ui.label(format!("Grid {version} 새 버전이 있습니다."));
                ui.label(reason);
                response.releases = ui.button("릴리스에서 설치 파일 받기").clicked();
            }
            Phase::Failed(reason) => {
                ui.label("업데이트를 준비하지 못했습니다.");
                ui.label(reason);
                rt.manual |= ui.button("다시 시도").clicked();
                response.releases = ui.button("릴리스에서 직접 받기").clicked();
            }
            Phase::Ready(_) => {}
        }
        if ui.button("닫기").clicked() {
            rt.visible = false;
        }
    });
    rt.visible &= open;
    response
}
