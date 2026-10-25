//! Optical (opto-cell) compressor model: program-dependent attack, two-stage release with memory.

#[derive(Clone, Copy)]
pub struct Params {
    pub peak_reduction: f32, // 0..100
    pub gain_db: f32,
    pub limit: bool,
    pub hf_follow: f32, // 0..1
    pub analog: u8, // 0 = off, 1 = 50 Hz, 2 = 60 Hz (saturation + mains hum)
    pub mix: f32,   // 0..1
    pub bypass: bool,
    pub auto_makeup: bool,
    pub trim_db: f32,
}

/// tanh(1.25 x) / 1.25 with first-order antiderivative anti-aliasing (no oversampling needed for mild drive).
#[inline]
fn adaa_tanh(prev: &mut f32, x: f32) -> f32 {
    const K: f32 = 1.25;
    // F(x) = ln(cosh(Kx)) / K^2, computed without overflow
    let f = |v: f32| {
        let a = (K * v).abs();
        (a + (1.0 + (-2.0 * a).exp()).ln() - std::f32::consts::LN_2) / (K * K)
    };
    let d = x - *prev;
    let y = if d.abs() < 1e-4 { (K * 0.5 * (x + *prev)).tanh() / K } else { (f(x) - f(*prev)) / d };
    *prev = x;
    y
}

pub struct Comp {
    fs: f32,
    lvl2: f32,
    lp: f32,
    g_fast: f32,
    g_slow: f32,
    mem: f32,
    pub gr_db: f32,
    pub out_peak: f32,
    pub in_peak: f32,
    gr_avg: f32,
    hum_ph: f32,
    smooth_gain: f32,
    dc: [f32; 2],
    // first-order antiderivative anti-aliasing state of the analog stage (per channel)
    ax: [f32; 2],
    // bypass crossfade 0 = processed, 1 = bypassed
    byp: f32,
}

impl Comp {
    pub fn new(fs: f32) -> Self {
        Self { fs, lvl2: 0.0, lp: 0.0, g_fast: 0.0, g_slow: 0.0, mem: 0.0, gr_db: 0.0, out_peak: 0.0, in_peak: 0.0, gr_avg: 0.0, hum_ph: 0.0, smooth_gain: 1.0, dc: [0.0; 2], ax: [0.0; 2], byp: 0.0 }
    }

    pub fn reset(&mut self) {
        let fs = self.fs;
        *self = Self::new(fs);
    }

    #[inline]
    fn coef(&self, tau_s: f32) -> f32 {
        1.0 - (-1.0 / (tau_s.max(1e-4) * self.fs)).exp()
    }

    /// Process one stereo (or mono) frame in place. `n` = channel count.
    pub fn process(&mut self, p: &Params, x: &mut [f32; 2], n: usize) {
        let n = n.clamp(1, 2);
        let mut dry = [0.0f32; 2];
        let mut mean = 0.0f32;
        for c in 0..n {
            let v = if x[c].is_finite() { x[c] } else { 0.0 };
            dry[c] = v;
            mean += v;
        }
        mean /= n as f32;

        // ---- side-chain: optional high-frequency emphasis, linked stereo, RMS-ish level
        self.lp += self.coef(1.0 / (2.0 * std::f32::consts::PI * 2500.0)) * (mean - self.lp);
        let hp = mean - self.lp;
        let sc = mean + hp * p.hf_follow * 3.0;
        let mut pw = 0.0f32;
        for c in 0..n {
            let v = dry[c] + (dry[c] - mean) * 0.0;
            pw += v * v;
        }
        pw /= n as f32;
        let sc_pw = (sc * sc).max(pw * 0.25);
        self.lvl2 += self.coef(0.003) * (sc_pw - self.lvl2);
        let lvl_db = 10.0 * (self.lvl2 + 1e-12).log10() + 3.0; // sine peak-referenced

        // ---- gain computer (soft knee)
        // below ~20 the threshold slides up so that Peak Reduction 0 really does nothing, even on full-scale peaks
        let thr = -p.peak_reduction * 0.40 + 8.0 * (1.0 - p.peak_reduction / 20.0).clamp(0.0, 1.0);
        let (ratio, knee) = if p.limit { (12.0f32, 6.0f32) } else { (3.0f32, 12.0f32) };
        let over = lvl_db - thr;
        let target = if over <= -knee * 0.5 {
            0.0
        } else if over >= knee * 0.5 {
            over * (1.0 - 1.0 / ratio)
        } else {
            let t = over + knee * 0.5;
            (1.0 - 1.0 / ratio) * t * t / (2.0 * knee)
        };

        // ---- optical cell
        let att_tau = (0.010 / (1.0 + (target - self.g_fast).max(0.0) / 6.0)).max(0.0015);
        if target > self.g_fast {
            self.g_fast += (target - self.g_fast) * self.coef(att_tau);
        } else {
            self.g_fast += (target - self.g_fast) * self.coef(0.035);
        }
        let slow_rel = 0.5 + 4.0 * self.mem;
        if target > self.g_slow {
            self.g_slow += (target - self.g_slow) * self.coef(att_tau);
        } else {
            self.g_slow += (target - self.g_slow) * self.coef(slow_rel);
        }
        let gr = 0.5 * self.g_fast + 0.5 * self.g_slow;
        let mem_target = (gr / 8.0).clamp(0.0, 1.0);
        let mem_tau = if mem_target > self.mem { 3.0 } else { 12.0 };
        self.mem += (mem_target - self.mem) * self.coef(mem_tau);
        self.gr_db += (gr - self.gr_db) * self.coef(0.03);

        // ---- apply
        self.gr_avg += (gr - self.gr_avg) * self.coef(1.5);
        let auto_db = if p.auto_makeup { 0.5 * self.gr_avg } else { 0.0 };
        let makeup = 10f32.powf((p.gain_db + auto_db + p.trim_db) / 20.0);
        let g = 10f32.powf(-gr / 20.0) * makeup;
        self.smooth_gain += (g - self.smooth_gain) * 0.05;
        let mut pk = 0.0f32;
        // click-free bypass: about 5 ms crossfade
        self.byp += ((p.bypass as u8 as f32) - self.byp) * 0.004;
        if (self.byp - p.bypass as u8 as f32).abs() < 1e-4 {
            self.byp = p.bypass as u8 as f32;
        }
        for c in 0..n {
            let mut y = dry[c] * self.smooth_gain;
            if p.analog > 0 {
                // gentle tube / transformer style soft saturation (unity at small signals)
                y = adaa_tanh(&mut self.ax[c], y);
                // remove the DC that asymmetric stages would add
                let d = &mut self.dc[c];
                *d += 0.0005 * (y - *d);
                y -= *d;
            }
            if p.analog > 0 {
                // faint mains hum (50 / 60 Hz + 2nd harmonic), about -84 dBFS
                let f = if p.analog == 1 { 50.0 } else { 60.0 };
                self.hum_ph += 2.0 * std::f32::consts::PI * f / self.fs / n as f32;
                if self.hum_ph > 2.0 * std::f32::consts::PI {
                    self.hum_ph -= 2.0 * std::f32::consts::PI;
                }
                y += 6.3e-5 * (self.hum_ph.sin() + 0.5 * (2.0 * self.hum_ph).sin());
            }
            // output ceiling: gain can never push the signal over 0 dBFS
            let ya = y.abs();
            if ya > 0.95 {
                y = y.signum() * (0.95 + 0.0499 * ((ya - 0.95) / 0.0499).tanh());
            }
            let out = dry[c] * (1.0 - p.mix) + y * p.mix;
            pk = pk.max(out.abs());
            x[c] = dry[c] * self.byp + out * (1.0 - self.byp);
        }
        let rel = (-1.0 / (0.3 * self.fs)).exp();
        self.out_peak = pk.max(self.out_peak * rel);
        let ipk = dry.iter().take(n).fold(0.0f32, |m, v| m.max(v.abs()));
        self.in_peak = ipk.max(self.in_peak * rel);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn par() -> Params {
        Params { peak_reduction: 0.0, gain_db: 0.0, limit: false, hf_follow: 0.0, analog: 0, mix: 1.0, bypass: false, auto_makeup: false, trim_db: 0.0 }
    }
    fn run(p: &Params, amp: f32, f: f32, n: usize) -> Vec<f32> {
        let fs = 48000.0;
        let mut c = Comp::new(fs);
        (0..n).map(|i| { let mut x = [amp * (2.0 * std::f32::consts::PI * f * i as f32 / fs).sin(); 2]; c.process(p, &mut x, 2); x[0] }).collect()
    }
    #[test]
    fn transparent_when_nothing_to_compress() {
        // peak reduction 0 = threshold far above the signal: output equals input, even near full scale
        for amp in [0.1f32, 0.5, 0.9] {
            let y = run(&par(), amp, 1000.0, 48000);
            let fs = 48000.0;
            let e = y.iter().enumerate().skip(4800).map(|(i, v)| (v - amp * (2.0 * std::f32::consts::PI * 1000.0 * i as f32 / fs).sin()).abs()).fold(0.0, f32::max);
            assert!(e < 2e-3, "amp {amp} err {e}");
        }
    }
    #[test]
    fn bypass_is_click_free() {
        let mut p = par();
        let fs = 48000.0;
        let mut c = Comp::new(fs);
        let mut prev = 0.0f32;
        let mut maxstep = 0.0f32;
        for i in 0..48000 {
            p.bypass = i > 24000;
            p.gain_db = 6.0;
            let mut x = [0.5 * (2.0 * std::f32::consts::PI * 200.0 * i as f32 / fs).sin(); 2];
            c.process(&p, &mut x, 2);
            maxstep = maxstep.max((x[0] - prev).abs());
            prev = x[0];
        }
        assert!(maxstep < 0.05, "step {maxstep}");
    }
    #[test]
    fn adaa_matches_tanh_and_is_finite() {
        let mut st = 0.0;
        for i in 0..2000 {
            let x = (i as f32 * 0.01).sin() * 3.0;
            let y = adaa_tanh(&mut st, x);
            assert!(y.is_finite() && y.abs() <= 0.81);
        }
        let mut st = 0.1;
        assert!((adaa_tanh(&mut st, 0.1) - (1.25f32 * 0.1).tanh() / 1.25).abs() < 1e-4);
    }
}
