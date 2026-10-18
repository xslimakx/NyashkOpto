//! NyashkOpto: optical compressor (VST3).

use nih_plug::prelude::*;
use nih_plug_egui::EguiState;
use std::num::NonZeroU32;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

pub mod dsp;
mod editor;
mod ui;

pub struct Meters {
    pub gr_db: AtomicU32,
    pub out_db: AtomicU32,
    pub in_db: AtomicU32,
}
impl Meters {
    fn new() -> Self {
        Self { gr_db: AtomicU32::new(0f32.to_bits()), out_db: AtomicU32::new((-90f32).to_bits()), in_db: AtomicU32::new((-90f32).to_bits()) }
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
}

impl Default for NyashkOpto {
    fn default() -> Self {
        Self { params: Arc::new(OptoParams::default()), meters: Arc::new(Meters::new()), comp: None }
    }
}

impl Plugin for NyashkOpto {
    const NAME: &'static str = "NyashkOpto";
    const VENDOR: &'static str = "NyashkTune";
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
        true
    }

    fn reset(&mut self) {
        if let Some(c) = self.comp.as_mut() {
            c.reset();
        }
    }

    fn process(&mut self, buffer: &mut Buffer, _aux: &mut AuxiliaryBuffers, _context: &mut impl ProcessContext<Self>) -> ProcessStatus {
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
            comp.process(&dp, &mut x, n);
            for (i, s) in frame.iter_mut().enumerate().take(n) {
                *s = x[i];
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
