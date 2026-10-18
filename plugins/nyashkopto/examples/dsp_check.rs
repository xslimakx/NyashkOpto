use nyashkopto::dsp::{Comp, Params};

fn run(p: &Params, fs: f32, secs: f32, amp_fn: impl Fn(f32) -> f32) -> (Vec<f32>, Vec<f32>) {
    let mut c = Comp::new(fs);
    let n = (secs * fs) as usize;
    let (mut out, mut gr) = (Vec::with_capacity(n), Vec::with_capacity(n));
    for i in 0..n {
        let t = i as f32 / fs;
        let mut x = [amp_fn(t) * (2.0 * std::f32::consts::PI * 1000.0 * t).sin(); 2];
        c.process(p, &mut x, 2);
        out.push(x[0]);
        gr.push(c.gr_db);
    }
    (out, gr)
}
fn db(a: f32) -> f32 { 20.0 * a.max(1e-9).log10() }
fn rms_db(x: &[f32]) -> f32 { db((x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt()) }

fn main() {
    let fs = 48000.0;
    let base = Params { peak_reduction: 0.0, gain_db: 0.0, limit: false, hf_follow: 0.0, analog: 0, mix: 1.0, bypass: false, auto_makeup: false, trim_db: 0.0 };
    // 1. transparency at peak reduction 0
    let (o, g) = run(&base, fs, 1.0, |_| 0.5);
    println!("PR=0, -6 dBFS sine: out rms {:.2} dB (in {:.2}), GR {:.2} dB", rms_db(&o[24000..]), 20.0 * (0.5f32 / 2f32.sqrt()).log10(), g[47999]);
    // 2. static curve
    println!("static GR (1 kHz sine), steady state after 6 s");
    for mode in [false, true] {
        for pr in [20.0, 40.0, 60.0, 80.0] {
            print!("  {} PR {:>3}: ", if mode { "LIMIT   " } else { "COMPRESS" }, pr);
            for amp_db in [-30.0f32, -20.0, -10.0, -3.0] {
                let p = Params { peak_reduction: pr, limit: mode, ..base };
                let a = 10f32.powf(amp_db / 20.0);
                let (_, g) = run(&p, fs, 6.0, |_| a);
                print!("in {:>4} dB -> GR {:>5.1} | ", amp_db, g[g.len() - 1]);
            }
            println!();
        }
    }
    // 3. attack / release: burst at -10 dBFS for 1 s, then quiet -50 dBFS
    let p = Params { peak_reduction: 60.0, ..base };
    let a_hi = 10f32.powf(-10.0 / 20.0);
    let a_lo = 10f32.powf(-50.0 / 20.0);
    let (_, g) = run(&p, fs, 8.0, |t| if t < 1.0 { a_hi } else { a_lo });
    let target = g[(1.0 * fs) as usize - 1];
    let at = |s: f32| g[(s * fs) as usize];
    let first = (0..(1.0 * fs) as usize).find(|&i| g[i] > 0.63 * target).unwrap() as f32 / fs * 1000.0;
    println!("attack: GR reaches 63% of {:.1} dB after {:.0} ms", target, first);
    for dt in [0.03f32, 0.06, 0.12, 0.3, 0.6, 1.0, 2.0, 4.0] {
        println!("  release +{:>4.0} ms: GR {:.2} dB ({:.0}% of peak)", dt * 1000.0, at(1.0 + dt), 100.0 * at(1.0 + dt) / target);
    }
    // 4. memory: longer compression -> slower release
    let (_, g2) = run(&p, fs, 20.0, |t| if t < 12.0 { a_hi } else { a_lo });
    let t2 = g2[(12.0 * fs) as usize - 1];
    println!("after 12 s of compression: GR {:.1} dB; release +1 s {:.1}, +4 s {:.1}, +8 s {:.1}", t2, g2[(13.0 * fs) as usize], g2[(16.0 * fs) as usize], g2[(20.0 * fs) as usize - 1]);
    // 5. robustness
    let mut c = Comp::new(fs);
    let mut bad = false;
    for i in 0..200000 {
        let v = if i % 5000 == 0 { f32::NAN } else if i % 7001 == 0 { f32::INFINITY } else { ((i as f32) * 0.05).sin() * 3.0 };
        let mut x = [v, v];
        c.process(&Params { peak_reduction: 100.0, analog: 1, hf_follow: 1.0, limit: true, ..base }, &mut x, 2);
        if !x[0].is_finite() || !c.gr_db.is_finite() { bad = true; }
    }
    println!("NaN/inf robustness: {}", if bad { "FAILED" } else { "ok" });
    // 6. analog saturation: THD-ish at small & big level
    for amp in [0.05f32, 0.5, 0.9] {
        let p = Params { analog: 1, ..base };
        let (o, _) = run(&p, fs, 1.0, |_| amp);
        println!("analog on: in {:.2} -> out rms gain {:.2} dB", amp, rms_db(&o[24000..]) - 20.0 * (amp / 2f32.sqrt()).log10());
    }
}
