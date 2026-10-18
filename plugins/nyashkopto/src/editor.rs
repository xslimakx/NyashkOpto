//! NyashkOpto GUI: same black / white / grey look as NyashkTune.

use nih_plug::prelude::*;
use nih_plug_egui::{
    create_egui_editor,
    egui::{self, epaint::PathStroke, Align2, Color32, CornerRadius, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, UiBuilder, Vec2},
};
use std::sync::Arc;
use std::time::Duration;

use crate::ui::*;
use crate::{AnalogMode, Meters, OptoParams, VuMode};

const BASE_W: f32 = 800.0;
const BASE_H: f32 = 470.0;

struct Gui {
    needle: f32,
}

pub fn create(params: Arc<OptoParams>, meters: Arc<Meters>) -> Option<Box<dyn Editor>> {
    let editor_state = params.editor_state.clone();
    let size_state = editor_state.clone();
    create_egui_editor(
        editor_state,
        Gui { needle: 0.0 },
        |ctx, _| {
            let mut v = egui::Visuals::dark();
            v.panel_fill = BG;
            v.window_fill = PANEL;
            v.extreme_bg_color = PANEL;
            v.override_text_color = Some(WHITE);
            v.selection.bg_fill = Color32::from_rgb(64, 64, 64);
            v.selection.stroke = Stroke::new(1.0, WHITE);
            ctx.set_visuals(v);
        },
        move |ctx, setter, st| {
            ctx.request_repaint_after(Duration::from_millis(33));
            let p = &*params;
            let mode = p.vu.value();
            let target = match mode {
                VuMode::Gr => meters.gr().clamp(0.0, 20.0),
                VuMode::Input => meters.inp().clamp(-30.0, 6.0),
                VuMode::Output => meters.out().clamp(-30.0, 6.0),
            };
            if (target - st.needle).abs() > 15.0 && mode != VuMode::Gr {
                st.needle = target;
            }
            st.needle += (target - st.needle) * 0.35;

            egui::CentralPanel::default().frame(egui::Frame::NONE.fill(BG)).show(ctx, |ui| {
                let full = ui.max_rect();
                let o = full.min;
                let rect = |x: f32, y: f32, w: f32, h: f32| Rect::from_min_size(Pos2::new(o.x + x, o.y + y), Vec2::new(w, h));
                let painter = ui.painter().clone();

                // ---- header
                let time = ui.input(|i| i.time);
                painter.text(Pos2::new(o.x + 18.0, o.y + 34.0), Align2::LEFT_CENTER, "Nyashk", mono(19.0), WHITE);
                painter.text(Pos2::new(o.x + 18.0 + 6.0 * 11.4, o.y + 34.0), Align2::LEFT_CENTER, "Opto", mono(19.0), DIM);
                if ((time * 2.0) as i64) % 2 == 0 {
                    painter.text(Pos2::new(o.x + 18.0 + 10.0 * 11.4, o.y + 34.0), Align2::LEFT_CENTER, "_", mono(19.0), WHITE);
                }
                painter.text(Pos2::new(o.x + 200.0, o.y + 36.0), Align2::LEFT_CENTER, "OPTICAL COMPRESSOR / LIMITER", mono(10.5), DIM);
                painter.hline(o.x + 14.0..=o.x + BASE_W - 14.0, o.y + 58.0, Stroke::new(1.0, LINE));
                if pill_tip(ui, rect(BASE_W - 14.0 - 76.0, 22.0, 76.0, 24.0), "BYPASS", p.bypass.value(), "Bypass the whole plugin.") {
                    set_b(setter, &p.bypass, !p.bypass.value());
                }

                // ---- main panel
                let mp = rect(14.0, 70.0, BASE_W - 28.0, 256.0);
                painter.rect_filled(mp, CornerRadius::same(6), PANEL);
                painter.rect_stroke(mp, CornerRadius::same(6), Stroke::new(1.0, LINE), StrokeKind::Inside);

                // left column: mode switches
                let lx = mp.left() + 12.0;
                let limit = p.limit.value();
                painter.text(Pos2::new(lx, mp.top() + 18.0), Align2::LEFT_CENTER, "MODE", mono(10.0), DIM);
                if pill_tip(ui, Rect::from_min_size(Pos2::new(lx, mp.top() + 30.0), Vec2::new(96.0, 26.0)), "COMPRESS", !limit,
                    "COMPRESS: gentle, musical compression (ratio about 3:1, soft knee). Good for vocals, bass, mix glue.") {
                    set_b(setter, &p.limit, false);
                }
                if pill_tip(ui, Rect::from_min_size(Pos2::new(lx, mp.top() + 62.0), Vec2::new(96.0, 26.0)), "LIMIT", limit,
                    "LIMIT: fast, firm limiting (ratio about 12:1). Catches peaks hard and keeps the level steady.") {
                    set_b(setter, &p.limit, true);
                }
                let am = p.auto_makeup.value();
                painter.text(Pos2::new(lx, mp.top() + 116.0), Align2::LEFT_CENTER, "AUTO MAKEUP", mono(10.0), DIM);
                if pill_tip(ui, Rect::from_min_size(Pos2::new(lx, mp.top() + 128.0), Vec2::new(96.0, 26.0)), if am { "ON" } else { "OFF" }, am,
                    "AUTO MAKEUP: automatically adds back part of the level lost to compression, so loudness stays close to the input.") {
                    set_b(setter, &p.auto_makeup, !am);
                }

                // big knobs with 0..100 scales
                let gain_cell = rect(225.0 - 85.0, 70.0 + 6.0, 170.0, 244.0);
                let peak_cell = rect(575.0 - 85.0, 70.0 + 6.0, 170.0, 244.0);
                let big_knob = |ui: &mut Ui, cell: Rect| {
                    let c = Pos2::new(cell.center().x, cell.top() + 84.0);
                    let rr = 62.0f32;
                    // ticks 0..100 (270 degree sweep, starting at 225 deg)
                    for i in 0..=10 {
                        let t = i as f32 / 10.0;
                        let a = (225.0f32 - 270.0 * t).to_radians();
                        let (ca, sa) = (a.cos(), a.sin());
                        ui.painter().line_segment([Pos2::new(c.x + (rr + 4.0) * ca, c.y - (rr + 4.0) * sa), Pos2::new(c.x + (rr + 10.0) * ca, c.y - (rr + 10.0) * sa)], Stroke::new(if i % 5 == 0 { 1.5 } else { 1.0 }, if i % 5 == 0 { WHITE } else { DIM }));
                        ui.painter().text(Pos2::new(c.x + (rr + 19.0) * ca, c.y - (rr + 19.0) * sa), Align2::CENTER_CENTER, format!("{}", i * 10), mono(9.5), DIM);
                    }
                };
                big_knob(ui, gain_cell);
                big_knob(ui, peak_cell);
                let kcell = |c: Rect| Rect::from_min_size(Pos2::new(c.left(), c.top() + 36.0), Vec2::new(c.width(), 200.0));
                knob_r(ui, setter, &p.gain, "GAIN", kcell(gain_cell), "Make-up gain after compression, 0-100. About 33 is unity (0 dB), 100 is +20 dB. The output is soft-limited so it never goes over 0 dBFS.", false, 46.0);
                knob_r(ui, setter, &p.peak, "PEAK REDUCTION", kcell(peak_cell), "How much signal feeds the optical cell, 0-100. More = more compression.", false, 46.0);

                // VU window in the centre
                let vu = Rect::from_center_size(Pos2::new(mp.center().x, mp.top() + 100.0), Vec2::new(172.0, 148.0));
                painter.rect_filled(vu, CornerRadius::same(8), Color32::from_rgb(0x05, 0x05, 0x05));
                painter.rect_stroke(vu, CornerRadius::same(8), Stroke::new(1.5, DIM), StrokeKind::Inside);
                let clip = ui.painter().with_clip_rect(vu.shrink(2.0));
                let pivot = Pos2::new(vu.center().x, vu.bottom() + 40.0);
                let rad = 142.0f32;
                let (lo, hi) = match mode {
                    VuMode::Gr => (0.0f32, 20.0f32),
                    _ => (-30.0f32, 6.0f32),
                };
                let ang = |v: f32| -> f32 {
                    let t = ((v - lo) / (hi - lo)).clamp(0.0, 1.0);
                    (125.0f32 - 70.0 * t).to_radians()
                };
                let at = |r: f32, v: f32| -> Pos2 {
                    let a = ang(v);
                    Pos2::new(pivot.x + r * a.cos(), pivot.y - r * a.sin())
                };
                let arc: Vec<Pos2> = (0..=60).map(|i| at(rad, lo + (hi - lo) * i as f32 / 60.0)).collect();
                clip.add(egui::Shape::line(arc, PathStroke::new(1.5, MID)));
                let hot: Vec<Pos2> = (0..=20).map(|i| at(rad, lo + (hi - lo) * (0.8 + 0.2 * i as f32 / 20.0))).collect();
                clip.add(egui::Shape::line(hot, PathStroke::new(3.0, WHITE)));
                let (step, major_step) = match mode {
                    VuMode::Gr => (1.0f32, 5.0f32),
                    _ => (2.0f32, 10.0f32),
                };
                let mut v = lo;
                while v <= hi + 0.001 {
                    let major = ((v - lo) % major_step).abs() < 0.01 || ((v - lo) % major_step - major_step).abs() < 0.01;
                    let len = if major { 12.0 } else { 6.0 };
                    clip.line_segment([at(rad, v), at(rad - len, v)], Stroke::new(if major { 1.5 } else { 1.0 }, if major { WHITE } else { DIM }));
                    if major {
                        let txt = if mode == VuMode::Gr { format!("{:.0}", v) } else { format!("{:+.0}", v) };
                        clip.text(at(rad - 24.0, v), Align2::CENTER_CENTER, txt, mono(10.0), DIM);
                    }
                    v += step;
                }
                clip.line_segment([at(rad - 98.0, st.needle), at(rad - 3.0, st.needle)], Stroke::new(2.0, WHITE));
                let label = match mode {
                    VuMode::Gr => "GAIN REDUCTION",
                    VuMode::Input => "INPUT  dB",
                    VuMode::Output => "OUTPUT  dB",
                };
                clip.text(Pos2::new(vu.center().x, vu.bottom() - 12.0), Align2::CENTER_CENTER, label, mono(10.0), DIM);

                // VU DISPLAY selector under the window
                painter.text(Pos2::new(mp.center().x, mp.top() + 190.0), Align2::CENTER_CENTER, "VU DISPLAY", mono(10.0), DIM);
                let pw = 64.0;
                let px = mp.center().x - pw * 1.5 - 4.0;
                let tips = [
                    "INPUT: the meter shows the level going into the plugin.",
                    "GR: the meter shows how many dB of gain reduction is being applied.",
                    "OUTPUT: the meter shows the level coming out of the plugin.",
                ];
                for (i, (name, m)) in [("INPUT", VuMode::Input), ("GR", VuMode::Gr), ("OUTPUT", VuMode::Output)].into_iter().enumerate() {
                    if pill_tip(ui, Rect::from_min_size(Pos2::new(px + (pw + 4.0) * i as f32, mp.top() + 202.0), Vec2::new(pw, 24.0)), name, mode == m, tips[i]) {
                        set_e(setter, &p.vu, m);
                    }
                }

                // ---- bottom strip
                let bp = rect(14.0, 338.0, BASE_W - 28.0, 114.0);
                painter.rect_filled(bp, CornerRadius::same(6), PANEL);
                painter.rect_stroke(bp, CornerRadius::same(6), Stroke::new(1.0, LINE), StrokeKind::Inside);
                // ANALOG
                let an = p.analog.value();
                painter.text(Pos2::new(bp.left() + 14.0, bp.top() + 18.0), Align2::LEFT_CENTER, "ANALOG", mono(10.0), DIM);
                let atips = [
                    "ANALOG OFF: clean digital signal path.",
                    "ANALOG 50 Hz: adds soft tube-style saturation and a faint 50 Hz mains hum (European power).",
                    "ANALOG 60 Hz: adds soft tube-style saturation and a faint 60 Hz mains hum (US power).",
                ];
                for (i, (name, m)) in [("OFF", AnalogMode::Off), ("50 Hz", AnalogMode::Hz50), ("60 Hz", AnalogMode::Hz60)].into_iter().enumerate() {
                    if pill_tip(ui, Rect::from_min_size(Pos2::new(bp.left() + 14.0, bp.top() + 30.0 + 26.0 * i as f32), Vec2::new(92.0, 22.0)), name, an == m, atips[i]) {
                        set_e(setter, &p.analog, m);
                    }
                }
                let ix = bp.left() + 124.0;
                let cw = (bp.right() - 10.0 - ix) / 3.0;
                let cell = |i: usize| Rect::from_min_size(Pos2::new(ix + cw * i as f32, bp.top() + 6.0), Vec2::new(cw, bp.height() - 12.0));
                knob(ui, setter, &p.hf, "HI FREQ", cell(0), "Makes the detector react more to high frequencies (sibilance, brightness). FLAT = even response.", false);
                knob(ui, setter, &p.mix, "MIX", cell(1), "Parallel blend: 0 = dry, 100 = fully compressed.", false);
                knob(ui, setter, &p.trim, "TRIM", cell(2), "Final output trim in dB.", true);

                // resize grip (keeps proportions)
                let grip = Rect::from_min_size(full.max - Vec2::splat(22.0), Vec2::splat(22.0));
                let gr_resp = ui.interact(grip, egui::Id::new("opto_resize_grip"), Sense::drag()).on_hover_text("Drag to resize");
                if gr_resp.dragged() {
                    if let Some(pos) = gr_resp.interact_pointer_pos() {
                        let ppp = ctx.pixels_per_point();
                        let w = ((pos.x - full.min.x + 11.0) * ppp).clamp(560.0, 2400.0);
                        size_state.set_requested_size((w.round() as u32, (w * BASE_H / BASE_W).round() as u32));
                    }
                }
                let gc = if gr_resp.hovered() || gr_resp.dragged() { WHITE } else { DIM };
                for k in [6.0f32, 11.0, 16.0] {
                    painter.line_segment([Pos2::new(full.max.x - 3.0, full.max.y - k), Pos2::new(full.max.x - k, full.max.y - 3.0)], Stroke::new(1.0, gc));
                }
                let _ = (UiBuilder::new(), Ui::id);
            });
        },
    )
}
