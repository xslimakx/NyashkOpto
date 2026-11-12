//! NyashkOpto: optical compressor (VST3).

use nih_plug::prelude::*;
use nih_plug_egui::EguiState;
use std::num::NonZeroU32;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

pub mod dsp;
mod demo;
mod editor;
mod ui;

pub struct Meters {
    pub gr_db: AtomicU32,
    pub out_db: AtomicU32,
    pub in_db: AtomicU32,
    pub wave: WaveRing,
}

/// Columns for the scrolling waveform: input min / max, output min / max, gain reduction (dB).
pub const WAVE_COLS: usize = 320;
pub const WAVE_SECONDS: f32 = 4.0;
pub struct WaveRing {
    cols: Vec<[AtomicU32; 5]>,
    pos: std::sync::atomic::AtomicUsize,
}
impl WaveRing {
    fn new() -> Self {
        Self { cols: (0..WAVE_COLS).map(|_| std::array::from_fn(|_| AtomicU32::new(0))).collect(), pos: std::sync::atomic::AtomicUsize::new(0) }
    }
    fn push(&self, v: [f32; 5]) {
        let p = self.pos.load(Ordering::Relaxed);
        for (a, x) in self.cols[p % WAVE_COLS].iter().zip(v) {
            a.store(x.to_bits(), Ordering::Relaxed);
        }
        self.pos.store(p.wrapping_add(1), Ordering::Release);
    }
    /// Oldest first.
    pub fn read(&self) -> Vec<[f32; 5]> {
        let end = self.pos.load(Ordering::Acquire);
        (0..WAVE_COLS)
            .map(|i| {
                let c = &self.cols[end.wrapping_add(i) % WAVE_COLS];
                std::array::from_fn(|k| f32::from_bits(c[k].load(Ordering::Relaxed)))
            })
            .collect()
    }
}

/// Audio-thread accumulator for one waveform column.
#[derive(Default)]
struct WaveAcc {
    v: [f32; 5],
    n: usize,
}

impl Meters {
    fn new() -> Self {
        Self { gr_db: AtomicU32::new(0f32.to_bits()), out_db: AtomicU32::new((-90f32).to_bits()), in_db: AtomicU32::new((-90f32).to_bits()), wave: WaveRing::new() }
    }
    pub fn inp(&self) -> f32 {
        f32::from_bits(self.in_db.load(Ordering::Relaxed))
    }
    pub fn gr(&self) -> f32 {
        f32::from_bits(self.gr_db.load(Ordering::Relaxed))
    }
    pub fn out(&self) -> f32 {
        f32::from_bits(self.out_db.load(Ordering::Relaxed))
    }
}

#[derive(Enum, PartialEq, Eq, Clone, Copy, Debug)]
pub enum AnalogMode {
    #[name = "OFF"]
    Off,
    #[name = "50 Hz"]
    Hz50,
    #[name = "60 Hz"]
    Hz60,
}

#[derive(Enum, PartialEq, Eq, Clone, Copy, Debug)]
pub enum VuMode {
    #[name = "INPUT"]
    Input,
    #[name = "GR"]
    Gr,
    #[name = "OUTPUT"]
    Output,
}

#[derive(Params)]
pub struct OptoParams {
    #[persist = "editor-state"]
    pub editor_state: Arc<EguiState>,

    #[id = "bypass"]
    pub bypass: BoolParam,
    #[id = "peak"]
    pub peak: FloatParam,
    #[id = "gain"]
    pub gain: FloatParam,
    #[id = "limit"]
    pub limit: BoolParam,
    #[id = "hf"]
    pub hf: FloatParam,
    #[id = "analogm"]
    pub analog: EnumParam<AnalogMode>,
    #[id = "mix"]
    pub mix: FloatParam,
    #[id = "vu"]
    pub vu: EnumParam<VuMode>,
    #[id = "auto"]
    pub auto_makeup: BoolParam,
    #[id = "trim"]
    pub trim: FloatParam,
}

impl Default for OptoParams {
    fn default() -> Self {
        let smooth = || SmoothingStyle::Linear(30.0);
        Self {
            editor_state: EguiState::from_size(800, 470),
            bypass: BoolParam::new("Bypass", false).make_bypass(),
            peak: FloatParam::new("Peak Reduction", 40.0, FloatRange::Linear { min: 0.0, max: 100.0 }).with_step_size(0.1).with_smoother(smooth()),
            // 0..100 like the hardware-style knob; 33 = unity (like the original), 100 = +20 dB, 0 = -10 dB
            gain: FloatParam::new("Gain", 33.0, FloatRange::Linear { min: 0.0, max: 100.0 }).with_step_size(0.1).with_smoother(smooth()),
            limit: BoolParam::new("Limit Mode", false),
            hf: FloatParam::new("Hi Freq", 0.0, FloatRange::Linear { min: 0.0, max: 100.0 })
                .with_step_size(0.1)
                .with_value_to_string(Arc::new(|v| if v < 0.5 { "FLAT".to_string() } else { format!("{v:.1}") }))
                .with_string_to_value(Arc::new(|s| {
                    let t = s.trim();
                    if t.eq_ignore_ascii_case("flat") { Some(0.0) } else { t.parse().ok() }
                })),
            analog: EnumParam::new("Analog", AnalogMode::Off),
            mix: FloatParam::new("Mix", 100.0, FloatRange::Linear { min: 0.0, max: 100.0 }).with_unit(" %").with_step_size(0.1).with_smoother(smooth()),
            vu: EnumParam::new("VU Display", VuMode::Gr),
            auto_makeup: BoolParam::new("Auto Makeup", false),
            trim: FloatParam::new("Trim", 0.0, FloatRange::Linear { min: -18.0, max: 18.0 }).with_unit(" dB").with_step_size(0.1).with_smoother(smooth()),
        }
    }
}

pub struct NyashkOpto {
    params: Arc<OptoParams>,
    meters: Arc<Meters>,
    comp: Option<dsp::Comp>,
    acc: WaveAcc,
    per_col: usize,
    fs: f32,
}

impl Default for NyashkOpto {
    fn default() -> Self {
        Self { params: Arc::new(OptoParams::default()), meters: Arc::new(Meters::new()), comp: None, acc: WaveAcc::default(), per_col: 600, fs: 48000.0 }
    }
}

impl Plugin for NyashkOpto {
    const NAME: &'static str = "NyashkOpto";
    const VENDOR: &'static str = "Nyashk";
    const URL: &'static str = "https://github.com/xslimakx/nyashk-plugins";
    const EMAIL: &'static str = "";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[
        AudioIOLayout { main_input_channels: NonZeroU32::new(2), main_output_channels: NonZeroU32::new(2), ..AudioIOLayout::const_default() },
        AudioIOLayout { main_input_channels: NonZeroU32::new(1), main_output_channels: NonZeroU32::new(1), ..AudioIOLayout::const_default() },
    ];
    const SAMPLE_ACCURATE_AUTOMATION: bool = false;
    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        editor::create(self.params.clone(), self.meters.clone())
    }

    fn initialize(&mut self, _layout: &AudioIOLayout, config: &BufferConfig, _context: &mut impl InitContext<Self>) -> bool {
        self.comp = Some(dsp::Comp::new(config.sample_rate));
        self.fs = config.sample_rate;
        self.per_col = ((config.sample_rate * WAVE_SECONDS) as usize / WAVE_COLS).max(1);
        true
    }

    fn reset(&mut self) {
        if let Some(c) = self.comp.as_mut() {
            c.reset();
        }
    }

    fn process(&mut self, buffer: &mut Buffer, _aux: &mut AuxiliaryBuffers, _context: &mut impl ProcessContext<Self>) -> ProcessStatus {
        demo::fill(buffer, self.fs);
        let Some(comp) = self.comp.as_mut() else { return ProcessStatus::Normal };
        let p = &self.params;
        for mut frame in buffer.iter_samples() {
            let dp = dsp::Params {
                peak_reduction: p.peak.smoothed.next(),
                gain_db: (p.gain.smoothed.next() - 33.0) * 0.3,
                limit: p.limit.value(),
                hf_follow: p.hf.value() / 100.0,
                analog: match p.analog.value() {
                    AnalogMode::Off => 0,
                    AnalogMode::Hz50 => 1,
                    AnalogMode::Hz60 => 2,
                },
                mix: p.mix.smoothed.next() / 100.0,
                bypass: p.bypass.value(),
                auto_makeup: p.auto_makeup.value(),
                trim_db: p.trim.smoothed.next(),
            };
            let n = frame.len().min(2);
            let mut x = [0.0f32; 2];
            for (i, s) in frame.iter_mut().enumerate().take(n) {
                x[i] = *s;
            }
            let xin = if n == 2 { 0.5 * (x[0] + x[1]) } else { x[0] };
            comp.process(&dp, &mut x, n);
            for (i, s) in frame.iter_mut().enumerate().take(n) {
                *s = x[i];
            }
            let xout = if n == 2 { 0.5 * (x[0] + x[1]) } else { x[0] };
            let a = &mut self.acc;
            a.v[0] = a.v[0].min(xin);
            a.v[1] = a.v[1].max(xin);
            a.v[2] = a.v[2].min(xout);
            a.v[3] = a.v[3].max(xout);
            a.v[4] = a.v[4].max(comp.gr_db);
            a.n += 1;
            if a.n >= self.per_col {
                self.meters.wave.push(a.v);
                *a = WaveAcc::default();
            }
        }
        self.meters.gr_db.store(comp.gr_db.to_bits(), Ordering::Relaxed);
        let o = 20.0 * (comp.out_peak + 1e-9).log10();
        self.meters.out_db.store(o.to_bits(), Ordering::Relaxed);
        let i = 20.0 * (comp.in_peak + 1e-9).log10();
        self.meters.in_db.store(i.to_bits(), Ordering::Relaxed);
        ProcessStatus::Normal
    }
}

impl Vst3Plugin for NyashkOpto {
    const VST3_CLASS_ID: [u8; 16] = *b"NyashkOptoVST3!!";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[Vst3SubCategory::Fx, Vst3SubCategory::Dynamics];
}

nih_export_vst3!(NyashkOpto);
