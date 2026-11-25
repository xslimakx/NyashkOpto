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
use crate::{AnalogMode, Meters, OnsetMode, OptoParams, VuMode};

const BASE_W: f32 = lay::W;
const BASE_H: f32 = lay::H;

/// envelope preview: 0.4 s, the loud entry at 40 ms
const ENV_COLS: usize = 160;
const ENV_SECS: f32 = 0.4;
const ENV_ON: f32 = 0.04;

struct Gui {
    hw: Option<Hw>,
    needle: f32,
    /// smooth scrolling of the waveform between audio blocks
    wave_pos: usize,
    wave_t: Instant,
    /// the envelope preview (a tone burst through the real compressor) and the settings it is for
    env: Vec<[f32; 2]>,
    env_key: [f32; 6],
}

/// Runs a tone burst through the compressor with the current settings: per column the input and
/// output peak. Cheap (a low sample rate), redone only when a setting changes.
fn envelope(p: &crate::dsp::Params) -> Vec<[f32; 2]> {
    const FS: f32 = 16000.0;
    const COLS: usize = ENV_COLS;
    let mut c = crate::dsp::Comp::new(FS);
    let lat = c.latency();
    let per = (FS * ENV_SECS) as usize / COLS;
    let total = per * COLS;
    let mut q = *p;
    q.gain_db = 0.0;
    q.auto_makeup = false;
    q.trim_db = 0.0;
    q.mix = 1.0;
    q.bypass = false;
    q.analog = 0;
    let mut out = vec![[0.0f32; 2]; COLS];
    for i in 0..total + lat {
        let t = i as f32 / FS;
        // silence, then a loud entry (a held tone) at ENV_ON
        let s = if t >= ENV_ON { 0.7 * (2.0 * std::f32::consts::PI * 400.0 * t).sin() } else { 0.0 };
        if i < total {
            let k = i / per;
            out[k][0] = out[k][0].max(s.abs());
        }
        let mut x = [s, s];
        c.process(&q, &mut x, 2);
        if i >= lat {
            let k = ((i - lat) / per).min(COLS - 1);
            out[k][1] = out[k][1].max(x[0].abs());
        }
    }
    out
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
        Gui { hw: None, needle: 0.0, wave_pos: 0, wave_t: Instant::now(), env: Vec::new(), env_key: [-1.0; 6] },
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
            // a window saved by an older version (another height): restore the proportions
            {
                let (w, h) = size_state.size();
                let want = (w as f32 * BASE_H / BASE_W).round() as u32;
                if h.abs_diff(want) > 2 {
                    size_state.set_requested_size((w, want));
                }
            }
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
                    VuMode::Input => "INPUT   0 VU = -18 dBFS",
                    VuMode::Output => "OUTPUT   0 VU = -18 dBFS",
                };
                hw::needle(&painter, hwr, vu, st.needle, Color32::from_rgb(255, 120, 60));
                hw::text(&painter, hwr, hw::find(lay::TEXTS, "vu_label", |x| x.id), label);
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
                let raw = meters.wave.read();
                // a light [1 2 1] blur along time: smooth outlines instead of jagged columns
                let cols: Vec<[f32; 5]> = (0..raw.len())
                    .map(|i| {
                        let a = raw[i.saturating_sub(1)];
                        let b = raw[(i + 1).min(raw.len() - 1)];
                        std::array::from_fn(|k| (a[k] + 2.0 * raw[i][k] + b[k]) * 0.25)
                    })
                    .collect();
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

                // ---- timing strip: onset guard, attack, release, side-chain high-pass
                let on = p.onset.value();
                for (id, m, tip) in [
                    ("on_vin", OnsetMode::Vintage, "ONSET VINTAGE: like the hardware. A loud entry after quiet slips through for the first 10-20 ms before the cell grabs it (punchy, but can sound harsh on vocals)."),
                    ("on_soft", OnsetMode::Soft, "ONSET SOFT: keeps some of the punch. The first peak of a loud entry stays within about 6 dB (3 dB in LIMIT) of the settled level."),
                    ("on_firm", OnsetMode::Firm, "ONSET FIRM: no burst. The first peak stays within about 2 dB (1 dB in LIMIT); settled material sounds the same as VINTAGE."),
                ] {
                    if hw::button(ui, hwr, b(id), on == m, tip) {
                        set_e(setter, &p.onset, m);
                    }
                }
                hw::knob(ui, hwr, setter, &p.attack, k("attack"), accent, "ATTACK: how fast the cell clamps down. 10 ms is the classic opto timing (still program-dependent); lower = faster, higher = more punch through.");
                hw::knob(ui, hwr, setter, &p.release, k("release"), accent, "RELEASE: speed of both release stages. 60 ms is the classic opto timing (fast part 60 ms, slow part 0.5-5 s); lower = faster recovery, higher = smoother, slower.");
                hw::knob(ui, hwr, setter, &p.sc_hpf, k("schpf"), accent, "SC HPF: high-pass in the detector, OFF or 20-400 Hz. The low end stops pumping the compressor; the audio itself is not filtered.");

                // envelope preview: a loud tone burst through the compressor with these settings
                let dp = crate::dsp::Params {
                    peak_reduction: p.peak.value(),
                    gain_db: 0.0,
                    limit: p.limit.value(),
                    hf_follow: p.hf.value() / 100.0,
                    analog: 0,
                    mix: 1.0,
                    bypass: false,
                    auto_makeup: false,
                    trim_db: 0.0,
                    onset: match on {
                        OnsetMode::Vintage => 0,
                        OnsetMode::Soft => 1,
                        OnsetMode::Firm => 2,
                    },
                    attack: p.attack.value() / 10.0,
                    release: p.release.value() / 60.0,
                    sc_hpf: 0.0,
                };
                let key = [dp.peak_reduction, dp.limit as u8 as f32, dp.hf_follow, dp.onset as f32, dp.attack, dp.release];
                if key != st.env_key {
                    st.env_key = key;
                    st.env = envelope(&dp);
                }
                let es = hw::find(lay::SCREENS, "env", |x| x.id);
                let er = hwr.rect(es.rect).shrink(2.0);
                let eclip = painter.with_clip_rect(er);
                if !st.env.is_empty() {
                    // levels in dB, -30..0 dBFS, input dim, output light; the dashed line is the
                    // settled output level, so the burst over it is the onset overshoot
                    let ne = st.env.len();
                    let top = er.top() + 18.0;
                    let ydb = |v: f32| {
                        let d = (20.0 * (v + 1e-6).log10()).clamp(-30.0, 0.0);
                        top + (-d / 30.0) * (er.bottom() - 4.0 - top)
                    };
                    let xat = |i: usize| er.left() + (i as f32 + 0.5) * er.width() / ne as f32;
                    let on = (ENV_ON / ENV_SECS * ne as f32) as usize;
                    let ln = |k: usize| -> Vec<Pos2> { st.env.iter().enumerate().skip(on).map(|(i, c)| Pos2::new(xat(i), ydb(c[k]))).collect() };
                    let mut fill = egui::Mesh::default();
                    for (i, c) in st.env.iter().enumerate().skip(on) {
                        let bb = fill.vertices.len() as u32;
                        let col = Color32::from_rgba_unmultiplied(0xb4, 0xb6, 0xbc, 40);
                        fill.colored_vertex(Pos2::new(xat(i), ydb(c[1])), col);
                        fill.colored_vertex(Pos2::new(xat(i), er.bottom()), col);
                        if i > on {
                            fill.add_triangle(bb - 2, bb, bb + 1);
                            fill.add_triangle(bb - 2, bb + 1, bb - 1);
                        }
                    }
                    eclip.add(egui::Shape::mesh(fill));
                    eclip.add(egui::Shape::line(ln(0), PathStroke::new(1.2_f32, Color32::from_rgb(0x5a, 0x5c, 0x62))));
                    eclip.add(egui::Shape::line(ln(1), PathStroke::new(1.6_f32, Color32::from_rgb(0xe4, 0xe2, 0xdc))));
                    // settled: the output 250-350 ms after the entry
                    let a0 = ((ENV_ON + 0.25) / ENV_SECS * ne as f32) as usize;
                    let settle = st.env[a0..ne].iter().map(|c| c[1]).sum::<f32>() / (ne - a0) as f32;
                    let sy = ydb(settle);
                    let mut x = er.left();
                    while x < er.right() {
                        eclip.line_segment([Pos2::new(x, sy), Pos2::new((x + 4.0).min(er.right()), sy)], Stroke::new(1.0_f32, accent));
                        x += 7.0;
                    }
                    let e20 = on + (0.02 / ENV_SECS * ne as f32) as usize;
                    let first = st.env[on..e20].iter().fold(0.0f32, |m, c| m.max(c[1]));
                    let over = (20.0 * ((first + 1e-6) / (settle + 1e-6)).log10()).max(0.0);
                    eclip.text(Pos2::new(er.left() + 6.0, er.top() + 4.0), egui::Align2::LEFT_TOP, format!("ATT {:.1} ms   REL {:.0} ms", p.attack.value(), p.release.value()), egui::FontId::proportional(9.0), Color32::from_rgb(200, 196, 188));
                    eclip.text(Pos2::new(er.right() - 6.0, er.top() + 4.0), egui::Align2::RIGHT_TOP, format!("burst +{over:.1} dB"), egui::FontId::proportional(9.0), if over > 3.0 { accent } else { Color32::from_rgb(200, 196, 188) });
                }
                hw::glass(&painter, hwr, es);

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
