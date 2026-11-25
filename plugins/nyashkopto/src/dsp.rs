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
    /// onset guard: 0 = VINTAGE (off, peaks slip through like the hardware), 1 = SOFT, 2 = FIRM
    pub onset: u8,
    /// attack time multiplier (1 = the classic program-dependent attack)
    pub attack: f32,
    /// release time multiplier (1 = the classic two-stage release)
    pub release: f32,
    /// side-chain high-pass in Hz (0 = off): bass no longer pumps the compressor
    pub sc_hpf: f32,
}

impl Default for Params {
    fn default() -> Self {
        Self { peak_reduction: 0.0, gain_db: 0.0, limit: false, hf_follow: 0.0, analog: 0, mix: 1.0, bypass: false, auto_makeup: false, trim_db: 0.0, onset: 2, attack: 1.0, release: 1.0, sc_hpf: 0.0 }
    }
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
    /// instant-attack peak envelope for the onset guard
    pk: f32,
    /// look-ahead delay line for the audio (the detector hears it this much earlier)
    la: [Vec<f32>; 2],
    la_pos: usize,
    /// side-chain high-pass (RBJ biquad), its coefficients and the frequency they are for
    hp_z: [f32; 2],
    hp_c: [f32; 5],
    hp_f: f32,
}

impl Comp {
    pub fn new(fs: f32) -> Self {
        Self { fs, lvl2: 0.0, lp: 0.0, g_fast: 0.0, g_slow: 0.0, mem: 0.0, gr_db: 0.0, out_peak: 0.0, in_peak: 0.0, gr_avg: 0.0, hum_ph: 0.0, smooth_gain: 1.0, dc: [0.0; 2], ax: [0.0; 2], byp: 0.0, pk: 0.0, la: [vec![0.0; Self::la_len(fs)], vec![0.0; Self::la_len(fs)]], la_pos: 0, hp_z: [0.0; 2], hp_c: [1.0, 0.0, 0.0, 0.0, 0.0], hp_f: -1.0 }
    }

    /// look-ahead: 2 ms
    fn la_len(fs: f32) -> usize {
        ((fs * 0.002).round() as usize).max(1)
    }

    /// latency in samples (the look-ahead)
    pub fn latency(&self) -> usize {
        self.la[0].len()
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

        // ---- optional side-chain high-pass: the detector ignores the low end
        let hpf_on = p.sc_hpf > 0.0;
        let mut det = dry;
        if hpf_on {
            if (p.sc_hpf - self.hp_f).abs() > 0.01 {
                self.hp_f = p.sc_hpf;
                let w = 2.0 * std::f32::consts::PI * p.sc_hpf.min(self.fs * 0.45) / self.fs;
                let (sn, cs) = w.sin_cos();
                let al = sn / (2.0 * std::f32::consts::FRAC_1_SQRT_2);
                let a0 = 1.0 + al;
                self.hp_c = [(1.0 + cs) * 0.5 / a0, -(1.0 + cs) / a0, (1.0 + cs) * 0.5 / a0, -2.0 * cs / a0, (1.0 - al) / a0];
            }
            let [b0, b1, b2, a1, a2] = self.hp_c;
            // transposed direct form II
            let y = b0 * mean + self.hp_z[0];
            self.hp_z[0] = b1 * mean - a1 * y + self.hp_z[1];
            self.hp_z[1] = b2 * mean - a2 * y;
            mean = y;
            for d in det.iter_mut().take(n) {
                *d = y;
            }
        } else {
            self.hp_z = [0.0; 2];
        }

        // ---- side-chain: optional high-frequency emphasis, linked stereo, RMS-ish level
        self.lp += self.coef(1.0 / (2.0 * std::f32::consts::PI * 2500.0)) * (mean - self.lp);
        let hp = mean - self.lp;
        let sc = mean + hp * p.hf_follow * 3.0;
        let pw = det.iter().take(n).map(|v| v * v).sum::<f32>() / n as f32;
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

        // ---- onset guard: a loud entry after quiet would pass the slow opto cell almost untouched
        // for its first 10-20 ms. An instant peak envelope says where the gain is heading; the cell
        // is pulled there at once, allowing `allow` dB above it, so steady material (where the
        // cell has already settled) is not touched and the opto character stays.
        let ipk = det.iter().take(n).fold(0.0f32, |m, v| m.max(v.abs()));
        let pk_rel = (-1.0 / (0.015 * self.fs)).exp();
        self.pk = ipk.max(self.pk * pk_rel);
        let pk_db = 20.0 * (self.pk + 1e-9).log10();
        let over_pk = pk_db - thr;
        let target_pk = if over_pk <= -knee * 0.5 {
            0.0
        } else if over_pk >= knee * 0.5 {
            over_pk * (1.0 - 1.0 / ratio)
        } else {
            let t = over_pk + knee * 0.5;
            (1.0 - 1.0 / ratio) * t * t / (2.0 * knee)
        };
        let allow = match (p.onset, p.limit) {
            (1, false) => 4.0,
            (1, true) => 2.0,
            (_, false) => 2.0,
            (_, true) => 1.0,
        };
        let guard = if p.onset > 0 { target_pk - allow } else { -100.0 };
        if guard > self.g_fast {
            self.g_fast += (guard - self.g_fast) * self.coef(0.0004);
        }
        if guard > self.g_slow {
            self.g_slow += (guard - self.g_slow) * self.coef(0.0004);
        }

        // ---- optical cell
        let att_tau = (0.010 / (1.0 + (target - self.g_fast).max(0.0) / 6.0)).max(0.0015) * p.attack;
        if target > self.g_fast {
            self.g_fast += (target - self.g_fast) * self.coef(att_tau);
        } else {
            self.g_fast += (target - self.g_fast) * self.coef(0.035 * p.release);
        }
        let slow_rel = (0.5 + 4.0 * self.mem) * p.release;
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
        // the audio itself comes out of the look-ahead line: the gain above already knows it
        let mut del = [0.0f32; 2];
        let ll = self.la[0].len();
        for c in 0..n {
            del[c] = self.la[c][self.la_pos];
            self.la[c][self.la_pos] = dry[c];
        }
        self.la_pos = (self.la_pos + 1) % ll;
        let mut pk = 0.0f32;
        // click-free bypass: about 5 ms crossfade
        self.byp += ((p.bypass as u8 as f32) - self.byp) * 0.004;
        if (self.byp - p.bypass as u8 as f32).abs() < 1e-4 {
            self.byp = p.bypass as u8 as f32;
        }
        for c in 0..n {
            let mut y = del[c] * self.smooth_gain;
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
            // (the soft knee starts at -0.04 dBFS, so anything that already fits is left alone)
            let ya = y.abs();
            if ya > 0.995 {
                y = y.signum() * (0.995 + 0.0049 * ((ya - 0.995) / 0.0049).tanh());
            }
            let out = del[c] * (1.0 - p.mix) + y * p.mix;
            pk = pk.max(out.abs());
            x[c] = del[c] * self.byp + out * (1.0 - self.byp);
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
    #[test]
    /// the onset guard leaves settled material alone (mean gain reduction within 0.3 dB)
    fn onset_guard_keeps_the_opto_character() {
        let fs = 48000.0f32;
        for pr in [40.0f32, 70.0] {
            let mut res = vec![];
            for onset in [0u8, 2] {
                let mut c = Comp::new(fs);
                let p = Params { peak_reduction: pr, onset, ..Params::default() };
                let mut seed = 7u32;
                let (mut gr, mut cnt) = (0.0f64, 0);
                for i in 0..(fs * 6.0) as usize {
                    let t = i as f32 / fs;
                    seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                    let nz = (seed >> 8) as f32 / 8388608.0 - 1.0;
                    // syllables at 5 Hz, a voiced tone + breath noise, level wandering slowly
                    let env = (0.5 + 0.5 * (2.0 * 3.14159 * 5.0 * t).sin()).powf(1.5) * (0.6 + 0.4 * (2.0 * 3.14159 * 0.3 * t).sin());
                    let s = env * (0.4 * (2.0 * 3.14159 * 180.0 * t).sin() + 0.15 * (2.0 * 3.14159 * 900.0 * t).sin() + 0.08 * nz);
                    let mut x = [s, s];
                    c.process(&p, &mut x, 2);
                    if t > 1.0 {
                        gr += c.gr_db as f64;
                        cnt += 1;
                    }
                }
                res.push(gr / cnt as f64);
            }
            eprintln!("PR {pr}: mean GR without guard {:.2} dB, with guard {:.2} dB", res[0], res[1]);
            assert!((res[1] - res[0]).abs() < 0.3, "PR {pr}: {res:?}");
        }
    }

    #[test]
    /// a loud entry after silence: the first 20 ms are no more than 3 dB over the settled level
    /// (was 5-21 dB before the onset guard)
    fn loud_entry_after_silence_has_no_burst() {
        let fs = 48000.0f32;
        for pr in [40.0f32, 60.0, 80.0] {
            for limit in [false, true] {
                let mut c = Comp::new(fs);
                let p = Params { peak_reduction: pr, gain_db: 0.0, limit, hf_follow: 0.0, analog: 0, mix: 1.0, bypass: false, auto_makeup: false, trim_db: 0.0, ..Params::default() };
                let n = (fs * 1.5) as usize;
                let on = (fs * 0.5) as usize;
                let (mut first, mut steady) = (0.0f32, 0.0f32);
                for i in 0..n {
                    let t = i as f32 / fs;
                    let s = if i >= on { 0.5 * ((2.0 * 3.14159 * 220.0 * t).sin() * 0.7 + (2.0 * 3.14159 * 440.0 * t).sin() * 0.3) } else { 0.0 };
                    let mut x = [s, s];
                    c.process(&p, &mut x, 2);
                    let o = on + c.latency();
                    if i >= o && i < o + (fs * 0.02) as usize { first = first.max(x[0].abs()); }
                    if i > on + (fs * 0.6) as usize { steady = steady.max(x[0].abs()); }
                }
                let over = 20.0 * (first / steady).log10();
                eprintln!("PR {pr} limit {limit}: first 20 ms peak {:.1} dBFS, steady {:.1} dBFS, overshoot {over:.1} dB", 20.0 * first.log10(), 20.0 * steady.log10());
                assert!(over < 3.0, "PR {pr} limit {limit}: {over}");
            }
        }
    }
    /// burst of a loud entry: first 20 ms over the level 0.6 s later, in dB
    fn burst(p: &Params) -> f32 {
        let fs = 48000.0f32;
        let mut c = Comp::new(fs);
        let on = (fs * 0.5) as usize;
        let (mut first, mut steady) = (0.0f32, 0.0f32);
        for i in 0..(fs * 1.5) as usize {
            let t = i as f32 / fs;
            let s = if i >= on { 0.5 * (2.0 * 3.14159 * 220.0 * t).sin() } else { 0.0 };
            let mut x = [s, s];
            c.process(p, &mut x, 2);
            let o = on + c.latency();
            if i >= o && i < o + (fs * 0.02) as usize { first = first.max(x[0].abs()); }
            if i > on + (fs * 0.6) as usize { steady = steady.max(x[0].abs()); }
        }
        20.0 * (first / steady).log10()
    }

    #[test]
    /// VINTAGE lets the entry through, SOFT a little, FIRM almost nothing; a slower attack lets more through
    fn onset_modes_and_attack_are_ordered() {
        let b = |onset: u8, attack: f32| burst(&Params { peak_reduction: 60.0, onset, attack, ..Params::default() });
        let (v, s, f) = (b(0, 1.0), b(1, 1.0), b(2, 1.0));
        eprintln!("burst: vintage {v:.1} dB, soft {s:.1} dB, firm {f:.1} dB");
        assert!(v > s + 1.0 && s > f + 0.5 && f < 3.0, "{v} {s} {f}");
        let (fast, slow) = (b(0, 0.25), b(0, 4.0));
        eprintln!("vintage burst: attack x0.25 {fast:.1} dB, x4 {slow:.1} dB");
        assert!(slow > fast + 2.0, "{fast} {slow}");
    }

    #[test]
    /// slower release = gain reduction lasts longer after the signal stops
    fn release_scales_recovery() {
        let fs = 48000.0f32;
        let after = |release: f32| {
            let mut c = Comp::new(fs);
            let p = Params { peak_reduction: 60.0, release, ..Params::default() };
            for i in 0..(fs * 1.2) as usize {
                let t = i as f32 / fs;
                let s = if t < 1.0 { 0.5 * (2.0 * 3.14159 * 220.0 * t).sin() } else { 0.0 };
                let mut x = [s, s];
                c.process(&p, &mut x, 2);
            }
            c.gr_db
        };
        let (fast, slow) = (after(0.25), after(4.0));
        eprintln!("GR 200 ms after the end: release x0.25 {fast:.2} dB, x4 {slow:.2} dB");
        assert!(slow > fast + 1.0, "{fast} {slow}");
    }

    #[test]
    /// the side-chain high-pass: a bass tone barely moves the cell, the audio is not filtered
    fn sc_hpf_ignores_bass() {
        let fs = 48000.0f32;
        let gr = |sc_hpf: f32| {
            let mut c = Comp::new(fs);
            let p = Params { peak_reduction: 60.0, sc_hpf, ..Params::default() };
            let mut out_pk = 0.0f32;
            for i in 0..(fs * 2.0) as usize {
                let s = 0.5 * (2.0 * 3.14159 * 50.0 * i as f32 / fs).sin();
                let mut x = [s, s];
                c.process(&p, &mut x, 2);
                if i > (fs * 1.5) as usize { out_pk = out_pk.max(x[0].abs()); }
            }
            (c.gr_db, out_pk)
        };
        let (off, on) = (gr(0.0), gr(200.0));
        eprintln!("50 Hz tone GR: SC HPF off {:.1} dB, 200 Hz {:.1} dB (out peak {:.2})", off.0, on.0, on.1);
        assert!(off.0 > 5.0 && on.0 < 1.0 && on.1 > 0.45, "{off:?} {on:?}");
    }

    fn par() -> Params {
        Params { peak_reduction: 0.0, gain_db: 0.0, limit: false, hf_follow: 0.0, analog: 0, mix: 1.0, bypass: false, auto_makeup: false, trim_db: 0.0, ..Params::default() }
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
