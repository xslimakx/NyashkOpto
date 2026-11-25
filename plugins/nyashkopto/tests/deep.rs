//! Behaviour checks against what an optical leveling amplifier is expected to do.
use nyashkopto::dsp::{Comp, Params};
use std::f32::consts::PI;

fn par() -> Params {
    Params { peak_reduction: 50.0, gain_db: 0.0, limit: false, hf_follow: 0.0, analog: 0, mix: 1.0, bypass: false, auto_makeup: false, trim_db: 0.0, ..Params::default() }
}

fn db(x: f32) -> f32 {
    20.0 * x.max(1e-9).log10()
}

/// steady sine through the compressor; returns (output peak dB over the last half, final GR dB)
fn steady(p: &Params, amp: f32, f: f32, fs: f32, secs: f32) -> (f32, f32) {
    let mut c = Comp::new(fs);
    let n = (fs * secs) as usize;
    let mut pk = 0.0f32;
    for i in 0..n {
        let s = amp * (2.0 * PI * f * i as f32 / fs).sin();
        let mut x = [s, s];
        c.process(p, &mut x, 2);
        if i > n / 2 {
            pk = pk.max(x[0].abs());
        }
    }
    (db(pk), c.gr_db)
}

#[test]
fn peak_reduction_raises_gain_reduction_monotonically() {
    let mut last = -1.0;
    for pr in [0.0f32, 20.0, 40.0, 60.0, 80.0, 100.0] {
        let (_, gr) = steady(&Params { peak_reduction: pr, ..par() }, 0.5, 1000.0, 48000.0, 3.0);
        println!("PR {pr:5.1}: GR {gr:5.2} dB");
        assert!(gr >= last - 0.01, "GR not monotonic: {gr} after {last}");
        last = gr;
    }
    let (_, gr0) = steady(&Params { peak_reduction: 0.0, ..par() }, 0.9, 1000.0, 48000.0, 2.0);
    assert!(gr0 < 0.1, "PR 0 must not compress, GR {gr0}");
    assert!(last > 10.0, "PR 100 on a -6 dBFS sine should compress hard, GR {last}");
}

#[test]
fn static_curve_compress_vs_limit() {
    // above the threshold: compress ~3:1, limit much steeper
    let slope = |limit: bool| {
        let p = Params { peak_reduction: 70.0, limit, ..par() };
        let (o1, _) = steady(&p, 10f32.powf(-12.0 / 20.0), 1000.0, 48000.0, 3.0);
        let (o2, _) = steady(&p, 10f32.powf(-2.0 / 20.0), 1000.0, 48000.0, 3.0);
        (o2 - o1) / 10.0
    };
    let (sc, sl) = (slope(false), slope(true));
    println!("output dB per input dB: compress {sc:.3}, limit {sl:.3}");
    assert!(sc > 0.2 && sc < 0.5, "compress slope {sc}");
    assert!(sl < 0.2 && sl < sc, "limit slope {sl}");
}

#[test]
fn attack_fast_release_two_stage() {
    let fs = 48000.0;
    let p = Params { peak_reduction: 70.0, ..par() };
    let mut c = Comp::new(fs);
    let mut grs = vec![];
    // 1 s quiet, 2 s loud, 6 s quiet; the applied gain is measured on the output (peak over each
    // 2 ms of a 500 Hz tone = one period), not on the smoothed meter value
    let mut pk = 0.0f32;
    let mut ipk = 0.0f32;
    // the output is the input delayed by the look-ahead: compare like with like
    let lat = c.latency();
    let mut hist = std::collections::VecDeque::from(vec![0.0f32; lat]);
    for i in 0..(fs * 9.0) as usize {
        let t = i as f32 / fs;
        let a = if (1.0..3.0).contains(&t) { 0.7 } else { 0.01 };
        let s = a * (2.0 * PI * 500.0 * t).sin();
        let mut x = [s, s];
        c.process(&p, &mut x, 2);
        hist.push_back(s);
        let sd = hist.pop_front().unwrap();
        pk = pk.max(x[0].abs());
        ipk = ipk.max(sd.abs());
        if i % 96 == 95 {
            grs.push((t, db(ipk) - db(pk)));
            pk = 0.0;
            ipk = 0.0;
        }
    }
    let peak = grs.iter().filter(|(t, _)| *t < 3.0).map(|g| g.1).fold(0.0f32, f32::max);
    let t63 = grs.iter().find(|(t, g)| *t >= 1.0 && *g >= 0.63 * peak).unwrap().0 - 1.0;
    let at = |tt: f32| grs.iter().find(|(t, _)| *t >= tt).unwrap().1;
    let r50 = grs.iter().find(|(t, g)| *t >= 3.0 && *g <= 0.5 * at(3.0)).map(|g| g.0 - 3.0).unwrap_or(9.0);
    let r90 = grs.iter().find(|(t, g)| *t >= 3.0 && *g <= 0.1 * at(3.0)).map(|g| g.0 - 3.0).unwrap_or(9.0);
    println!("GR {peak:.1} dB, attack to 63 % {:.1} ms, release to 50 % {:.0} ms, to 90 % {:.2} s", t63 * 1000.0, r50 * 1000.0, r90);
    // opto: attack around 10 ms, release first half quick (tens of ms), the rest slow (seconds)
    assert!(t63 < 0.03, "attack {t63}");
    assert!(r50 < 0.25, "first release stage {r50}");
    assert!(r90 > 0.6, "slow release tail missing: {r90}");
}

#[test]
fn hf_follow_reacts_to_highs_only() {
    let base = |hf: f32, f: f32| steady(&Params { peak_reduction: 50.0, hf_follow: hf, ..par() }, 0.3, f, 48000.0, 2.0).1;
    let (lo0, lo1) = (base(0.0, 200.0), base(1.0, 200.0));
    let (hi0, hi1) = (base(0.0, 8000.0), base(1.0, 8000.0));
    println!("200 Hz GR {lo0:.2} -> {lo1:.2}; 8 kHz GR {hi0:.2} -> {hi1:.2}");
    assert!(hi1 > hi0 + 2.0, "HF follow does not add GR on highs");
    assert!((lo1 - lo0).abs() < 1.0, "HF follow changes the lows");
}

#[test]
fn gain_trim_mix_and_makeup() {
    let quiet = 10f32.powf(-30.0 / 20.0); // below the threshold
    let (o, _) = steady(&Params { peak_reduction: 20.0, gain_db: 6.0, ..par() }, quiet, 1000.0, 48000.0, 1.0);
    assert!((o - (-24.0)).abs() < 0.2, "gain +6: {o}");
    let (o, _) = steady(&Params { peak_reduction: 20.0, trim_db: -6.0, ..par() }, quiet, 1000.0, 48000.0, 1.0);
    assert!((o - (-36.0)).abs() < 0.2, "trim -6: {o}");
    // mix 0 = dry, whatever the settings
    let (o, _) = steady(&Params { peak_reduction: 100.0, gain_db: 10.0, mix: 0.0, ..par() }, 0.5, 1000.0, 48000.0, 1.0);
    assert!((o - db(0.5)).abs() < 0.1, "mix 0 not dry: {o}");
    // auto makeup gives part of the reduction back
    let (a, _) = steady(&Params { peak_reduction: 80.0, ..par() }, 0.5, 1000.0, 48000.0, 4.0);
    let (b, _) = steady(&Params { peak_reduction: 80.0, auto_makeup: true, ..par() }, 0.5, 1000.0, 48000.0, 4.0);
    println!("auto makeup: {a:.2} -> {b:.2} dB");
    assert!(b > a + 1.0, "auto makeup does nothing");
}

#[test]
fn analog_hum_only_when_on_and_at_the_right_frequency() {
    let fs = 48000.0;
    let hum = |mode: u8, f: f32| {
        let mut c = Comp::new(fs);
        let p = Params { analog: mode, ..par() };
        let n = fs as usize;
        let (mut re, mut im) = (0.0f64, 0.0f64);
        for i in 0..n {
            let mut x = [0.0f32; 2];
            c.process(&p, &mut x, 2);
            let ph = 2.0 * std::f64::consts::PI * f as f64 * i as f64 / fs as f64;
            re += x[0] as f64 * ph.cos();
            im += x[0] as f64 * ph.sin();
        }
        db((2.0 * (re * re + im * im).sqrt() / n as f64) as f32)
    };
    let (off, h50, h60, h50_60) = (hum(0, 50.0), hum(1, 50.0), hum(2, 60.0), hum(1, 60.0));
    println!("hum: off {off:.1}, 50 Hz mode @50 {h50:.1}, 60 Hz mode @60 {h60:.1}, 50 Hz mode @60 {h50_60:.1} dBFS");
    assert!(off < -140.0);
    assert!(h50 > -95.0 && h50 < -75.0 && h60 > -95.0 && h60 < -75.0);
    assert!(h50_60 < h50 - 30.0);
}

#[test]
fn stereo_is_linked_and_rates_agree() {
    // a loud left channel must reduce the right one as well (linked stereo, no image shift)
    let fs = 48000.0;
    let mut c = Comp::new(fs);
    let p = Params { peak_reduction: 70.0, ..par() };
    let mut r_pk = 0.0f32;
    for i in 0..(fs * 2.0) as usize {
        let t = i as f32 / fs;
        let mut x = [0.7 * (2.0 * PI * 500.0 * t).sin(), 0.05 * (2.0 * PI * 700.0 * t).sin()];
        c.process(&p, &mut x, 2);
        if t > 1.5 {
            r_pk = r_pk.max(x[1].abs());
        }
    }
    println!("right channel {:.2} dB (dry -26.0)", db(r_pk));
    assert!(db(r_pk) < -27.0, "right channel not reduced with the left");
    // the same setting does about the same at 44.1 and 96 kHz
    let g = |fs: f32| steady(&Params { peak_reduction: 60.0, ..par() }, 0.5, 1000.0, fs, 3.0).1;
    let (a, b) = (g(44100.0), g(96000.0));
    println!("GR 44.1k {a:.2} / 96k {b:.2}");
    assert!((a - b).abs() < 0.5);
}

#[test]
fn hot_input_is_not_coloured_without_compression() {
    // peak reduction 0, unity gain: a signal peaking near 0 dBFS must come out unchanged
    let fs = 48000.0;
    let mut c = Comp::new(fs);
    let p = Params { peak_reduction: 0.0, ..par() };
    let mut err = 0.0f32;
    for i in 0..48000 {
        let s = 0.99 * (2.0 * PI * 1000.0 * i as f32 / fs).sin();
        let mut x = [s, s];
        c.process(&p, &mut x, 2);
        if i > 4800 {
            err = err.max((x[0] - s).abs());
        }
    }
    println!("max deviation at -0.1 dBFS: {err:.5}");
    assert!(err < 2e-3, "a -0.1 dBFS signal is changed by {err}");
}

#[test]
fn recovers_after_nan_or_inf_from_the_host() {
    for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        let mut a = Comp::new(48000.0);
        let p = Params { analog: 1, peak_reduction: 80.0, hf_follow: 1.0, mix: 0.5, ..par() };
        for n in 0..96000 {
            let v = if (100..110).contains(&n) { bad } else { 0.3 * (n as f32 * 0.05).sin() };
            let mut x = [v, v];
            a.process(&p, &mut x, 2);
            if n > 48000 {
                assert!(x[0].is_finite() && x[1].is_finite(), "opto {bad}: still broken at n {n}: {x:?}");
            }
        }
    }
}
