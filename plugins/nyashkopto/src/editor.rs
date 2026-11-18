//! NyashkOpto GUI: modern analog faceplate (pre-rendered in tools/render_ui.py, see hw.rs).

use nih_plug::prelude::*;
use nih_plug_egui::{
    create_egui_editor,
    egui::{self, epaint::PathStroke, Color32, Pos2, Rect, Sense, Stroke, Vec2},
};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::hw::{self, Hw};
use crate::hw_layout as lay;
use crate::ui::{set_b, set_e};
use crate::{AnalogMode, Meters, OptoParams, VuMode};

const BASE_W: f32 = lay::W;
const BASE_H: f32 = lay::H;

struct Gui {
    hw: Option<Hw>,
    needle: f32,
    /// smooth scrolling of the waveform between audio blocks
    wave_pos: usize,
    wave_t: Instant,
}

fn k(id: &str) -> &'static hw::KnobL {
    hw::find(lay::KNOBS, id, |x| x.id)
}
fn b(id: &str) -> &'static hw::BtnL {
    hw::find(lay::BUTTONS, id, |x| x.id)
}

pub fn create(params: Arc<OptoParams>, meters: Arc<Meters>) -> Option<Box<dyn Editor>> {
    let editor_state = params.editor_state.clone();
    let size_state = editor_state.clone();
    create_egui_editor(
        editor_state,
        Gui { hw: None, needle: 0.0, wave_pos: 0, wave_t: Instant::now() },
        |ctx, st| {
            // a new window gets a new egui context: textures must be loaded again
            st.hw = None;
            let mut v = egui::Visuals::dark();
            v.override_text_color = Some(Color32::from_rgb(232, 230, 224));
            v.selection.bg_fill = Color32::from_rgb(120, 80, 30);
            ctx.set_visuals(v);
        },
        move |ctx, setter, st| {
            ctx.request_repaint_after(Duration::from_millis(33));
            if st.hw.is_none() {
                st.hw = Some(Hw::load(ctx, lay::ASSETS));
            }
            let p = &*params;
            let mode = p.vu.value();
            // the needle reads like the original: GR swings left from 0 VU, IN / OUT read
            // the level with 0 VU = -18 dBFS
            let target = match mode {
                VuMode::Gr => hw::vu_t(-meters.gr().clamp(0.0, 30.0)),
                VuMode::Input => hw::vu_t(meters.inp() + 18.0),
                VuMode::Output => hw::vu_t(meters.out() + 18.0),
            };
            // VU ballistics: ~300 ms to 99 %
            st.needle += (target - st.needle) * 0.22;
            let accent = hw::c32(lay::ACCENT);

            egui::CentralPanel::default().frame(egui::Frame::NONE.fill(Color32::from_rgb(14, 14, 15))).show(ctx, |ui| {
                let full = ui.max_rect();
                let hwr = st.hw.as_mut().unwrap();
                hwr.o = full.min;
                let hwr = st.hw.as_ref().unwrap();
                let painter = ui.painter().clone();
                hwr.background(&painter, BASE_W, BASE_H);

                // ---- top: power, the two big knobs, the meter
                if hw::button(ui, hwr, b("power"), !p.bypass.value(), "Power: off bypasses the whole plugin.") {
                    set_b(setter, &p.bypass, !p.bypass.value());
                }
                hw::knob(ui, hwr, setter, &p.gain, k("gain"), accent,
                    "GAIN: make-up gain after compression, 0-100. About 33 is unity (0 dB), 100 is +20 dB. The output is soft-limited so it never goes over 0 dBFS.");
                hw::knob(ui, hwr, setter, &p.peak, k("peak"), accent, "PEAK REDUCTION: how much signal feeds the optical cell, 0-100. More = more compression.");
                let vu = hw::find(lay::VUS, "vu", |x| x.id);
                let label = match mode {
                    VuMode::Gr => "GAIN REDUCTION",
                    VuMode::Input => "INPUT  (0 VU = -18 dBFS)",
                    VuMode::Output => "OUTPUT  (0 VU = -18 dBFS)",
                };
                hw::text(&painter, hwr, hw::find(lay::TEXTS, "vu_label", |x| x.id), label);
                hw::needle(&painter, hwr, vu, st.needle, Color32::from_rgb(255, 120, 60));
                for (id, m, tip) in [
                    ("vu_in", VuMode::Input, "Meter shows the level going into the plugin."),
                    ("vu_gr", VuMode::Gr, "Meter shows the gain reduction."),
                    ("vu_out", VuMode::Output, "Meter shows the level coming out of the plugin."),
                ] {
                    if hw::button(ui, hwr, b(id), mode == m, tip) {
                        set_e(setter, &p.vu, m);
                    }
                }

                // ---- bottom strip
                let limit = p.limit.value();
                if hw::button(ui, hwr, b("comp"), !limit, "COMPRESS: gentle, musical compression (about 3:1, soft knee). Vocals, bass, mix glue.") {
                    set_b(setter, &p.limit, false);
                }
                if hw::button(ui, hwr, b("limit"), limit, "LIMIT: fast, firm limiting (about 12:1). Catches peaks hard.") {
                    set_b(setter, &p.limit, true);
                }
                let am = p.auto_makeup.value();
                if hw::button(ui, hwr, b("makeup"), am, "AUTO MAKEUP: adds back part of the level lost to compression.") {
                    set_b(setter, &p.auto_makeup, !am);
                }
                let an = p.analog.value();
                for (id, m, tip) in [
                    ("an_off", AnalogMode::Off, "ANALOG OFF: clean digital signal path."),
                    ("an_50", AnalogMode::Hz50, "ANALOG 50 Hz: soft tube-style saturation and a faint 50 Hz mains hum (European power)."),
                    ("an_60", AnalogMode::Hz60, "ANALOG 60 Hz: soft tube-style saturation and a faint 60 Hz mains hum (US power)."),
                ] {
                    if hw::button(ui, hwr, b(id), an == m, tip) {
                        set_e(setter, &p.analog, m);
                    }
                }
                hw::knob(ui, hwr, setter, &p.hf, k("hf"), accent, "HF FOLLOW: makes the detector react more to high frequencies (sibilance, brightness). FLAT = even response.");
                hw::knob(ui, hwr, setter, &p.mix, k("mix"), accent, "MIX: parallel blend, 0 = dry, 100 = fully compressed.");
                hw::knob(ui, hwr, setter, &p.trim, k("trim"), accent, "TRIM: final output trim in dB.");

                // live waveform: input (dark), output (light), gain reduction hanging from the top
                let sc = hw::find(lay::SCREENS, "wave", |x| x.id);
                let wr = hwr.rect(sc.rect).shrink(2.0);
                let cols = meters.wave.read();
                let wpos = meters.wave.written();
                if wpos != st.wave_pos {
                    st.wave_pos = wpos;
                    st.wave_t = Instant::now();
                }
                let col_s = crate::WAVE_SECONDS / crate::WAVE_COLS as f32;
                let frac = (st.wave_t.elapsed().as_secs_f32() / col_s).min(1.0);
                let nc = cols.len();
                let w = wr.width() / (nc as f32 - 1.0);
                let (cy, h) = (wr.center().y + 6.0, wr.height() * 0.5 - 8.0);
                let wclip = painter.with_clip_rect(wr);
                let xat = |i: usize| wr.left() + (i as f32 - frac) * w;
                let yat = |v: f32| cy - v.clamp(-1.0, 1.0) * h;
                let band = |lo: usize, hi: usize, col: Color32| {
                    let mut mesh = egui::Mesh::default();
                    let uv = egui::epaint::WHITE_UV;
                    for (i, c) in cols.iter().enumerate() {
                        let x = xat(i);
                        let (top, bot) = (yat(c[hi]) - 0.5, yat(c[lo]) + 0.5);
                        let bb = mesh.vertices.len() as u32;
                        mesh.vertices.push(egui::epaint::Vertex { pos: Pos2::new(x, top), uv, color: col });
                        mesh.vertices.push(egui::epaint::Vertex { pos: Pos2::new(x, bot), uv, color: col });
                        if i > 0 {
                            mesh.indices.extend_from_slice(&[bb - 2, bb, bb + 1, bb - 2, bb + 1, bb - 1]);
                        }
                    }
                    wclip.add(egui::Shape::mesh(mesh));
                };
                band(0, 1, Color32::from_rgb(0x3a, 0x3c, 0x40));
                band(2, 3, Color32::from_rgb(0x9a, 0x9c, 0xa2));
                let gr_pts: Vec<Pos2> = cols.iter().enumerate().map(|(i, c)| Pos2::new(xat(i), wr.top() + 2.0 + (c[4].clamp(0.0, 20.0) / 20.0) * (wr.height() - 4.0))).collect();
                wclip.add(egui::Shape::line(gr_pts, PathStroke::new(1.6_f32, accent)));
                hw::glass(&painter, hwr, sc);

                // resize grip (keeps proportions)
                let grip = Rect::from_min_size(full.max - Vec2::splat(22.0), Vec2::splat(22.0));
                let gr_resp = ui.interact(grip, egui::Id::new("opto_resize_grip"), Sense::drag()).on_hover_text("Drag to resize");
                if gr_resp.dragged() {
                    if let Some(pos) = gr_resp.interact_pointer_pos() {
                        let ppp = if cfg!(target_os = "macos") { size_state.size().0 as f32 / BASE_W } else { ctx.pixels_per_point() };
                        let w = ((pos.x - full.min.x + 11.0) * ppp).clamp(560.0, 2400.0);
                        size_state.set_requested_size((w.round() as u32, (w * BASE_H / BASE_W).round() as u32));
                    }
                }
                let gc = if gr_resp.hovered() || gr_resp.dragged() { Color32::from_white_alpha(140) } else { Color32::from_white_alpha(50) };
                for kk in [6.0f32, 11.0, 16.0] {
                    painter.line_segment([Pos2::new(full.max.x - 3.0, full.max.y - kk), Pos2::new(full.max.x - kk, full.max.y - 3.0)], Stroke::new(1.0_f32, gc));
                }
            });
        },
    )
}
