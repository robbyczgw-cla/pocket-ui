//! The Deltarune case, simulated: ROCKNIX's quiet trips (the kernel's step_wise) against Smooth fan's Quiet curve on the
//! same thermal model (`pocket_ui::fan_curve::sim`). Prints the comparison table of docs/FAN-CURVE.md and writes a
//! chart and the samples: `cargo run --release --example fan_curve_sim -- <out dir>` (default /tmp/fan-curve-sim).

use pocket_ui::fan_curve::{sim, Controller, Shape, MIN_RUN_PWM};
use pocket_ui::gfx::Canvas;
use pocket_ui::text::{Face, Text};

const SECS: f32 = 900.0;
/// The chart shows this stretch (after the first two minutes).
const SHOW: (f32, f32) = (120.0, 420.0);

fn write_png(buf: &[u32], w: u32, h: u32, out: &std::path::Path) {
    let rgb: Vec<u8> = buf.iter().flat_map(|p| [(p >> 16) as u8, (p >> 8) as u8, *p as u8]).collect();
    let mut enc = png::Encoder::new(std::io::BufWriter::new(std::fs::File::create(out).expect("create")), w, h);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().and_then(|mut wr| wr.write_image_data(&rgb)).expect("write png");
}

/// A polyline of 3-pixel dots through `pts`.
fn plot(c: &mut Canvas, pts: &[(i32, i32)], color: u32) {
    for w in pts.windows(2) {
        let ((x0, y0), (x1, y1)) = (w[0], w[1]);
        let n = (x1 - x0).abs().max((y1 - y0).abs()).max(1);
        for k in 0..=n {
            let x = x0 + (x1 - x0) * k / n;
            let y = y0 + (y1 - y0) * k / n;
            c.rect(x - 1, y - 1, 3, 3, color);
        }
    }
}

fn main() {
    let out = std::path::PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| "/tmp/fan-curve-sim".into()));
    std::fs::create_dir_all(&out).expect("out dir");
    let old = sim::run(sim::Driver::Kernel(sim::Kernel::quiet()), SECS, 1);
    let new = sim::run(sim::Driver::Curve(Controller::new(0, MIN_RUN_PWM), Shape::Quiet), SECS, 1);

    // The samples, one row a second.
    let mut csv = String::from("t,old_hottest,old_cluster,old_pwm,new_hottest,new_cluster,new_pwm\n");
    for (a, b) in old.iter().zip(&new).step_by(10) {
        csv.push_str(&format!(
            "{:.0},{:.2},{:.2},{},{:.2},{:.2},{}\n",
            a.t, a.hottest, a.cluster, a.pwm, b.hottest, b.cluster, b.pwm
        ));
    }
    std::fs::write(out.join("fan-curve-sim.csv"), csv).expect("csv");

    let (so, sn) = (sim::summary(&old, 120.0), sim::summary(&new, 120.0));
    println!("| 13 minutes after the first two | ROCKNIX quiet trips (kernel) | Smooth fan, Quiet |");
    println!("|---|---|---|");
    println!("| fan starts / stops | {} / {} | {} / {} |", so.starts, so.stops, sn.starts, sn.stops);
    println!("| time with the fan off | {:.0} % | {:.0} % |", so.off_share * 100.0, sn.off_share * 100.0);
    println!(
        "| biggest PWM change within 1 s | {} ({} %) | {} ({} %) |",
        so.max_step_per_s,
        pct(so.max_step_per_s),
        sn.max_step_per_s,
        pct(sn.max_step_per_s)
    );
    println!(
        "| PWM mean / max | {:.0} / {} ({} %) | {:.0} / {} ({} %) |",
        so.mean_pwm,
        so.max_pwm,
        pct(so.max_pwm),
        sn.mean_pwm,
        sn.max_pwm,
        pct(sn.max_pwm)
    );
    println!(
        "| hottest core mean / max | {:.1} / {:.1} °C | {:.1} / {:.1} °C |",
        so.mean_hottest, so.max_hottest, sn.mean_hottest, sn.max_hottest
    );

    // The chart: temperature above, PWM below, old in one colour and new in the other.
    let (w, h) = (1600u32, 900u32);
    let mut buf = vec![0u32; (w * h) as usize];
    let mut c = Canvas::new(&mut buf, w, h);
    let mut text = Text::new();
    let (bg, grid, ink, dim) = (0x101418, 0x2a3038, 0xe8ecf0, 0x8a96a3);
    let (old_c, new_c) = (0xe0703c, 0x3cb4e0);
    c.clear(bg);
    let (left, right) = (110, w as i32 - 40);
    let x_of = |t: f32| left + ((t - SHOW.0) / (SHOW.1 - SHOW.0) * (right - left) as f32) as i32;
    let (t_top, t_bot, t_lo, t_hi) = (90, 430, 40.0f32, 70.0f32);
    let (p_top, p_bot) = (520, 840);
    let y_temp = |v: f32| t_bot - ((v - t_lo) / (t_hi - t_lo) * (t_bot - t_top) as f32) as i32;
    let y_pwm = |v: u8| p_bot - (v as f32 / 128.0 * (p_bot - p_top) as f32) as i32;
    for v in (40..=70).step_by(5) {
        c.rect(left, y_temp(v as f32), right - left, 1, grid);
        text.draw(&mut c, Face::Body, 20, 30, y_temp(v as f32) + 7, dim, &format!("{v} °C"));
    }
    for p in [0u8, 26, 51, 77, 102, 128] {
        c.rect(left, y_pwm(p), right - left, 1, grid);
        text.draw(&mut c, Face::Body, 20, 30, y_pwm(p) + 7, dim, &format!("{} %", pct(p)));
    }
    for s in (SHOW.0 as i32..=SHOW.1 as i32).step_by(60) {
        let x = x_of(s as f32);
        c.rect(x, t_top, 1, p_bot - t_top, grid);
        text.draw(&mut c, Face::Body, 20, x - 20, p_bot + 30, dim, &format!("{} s", s));
    }
    let shown = |s: &[sim::Sample]| -> Vec<sim::Sample> { s.iter().filter(|x| x.t >= SHOW.0 && x.t <= SHOW.1).copied().collect() };
    let (o, n) = (shown(&old), shown(&new));
    plot(&mut c, &o.iter().step_by(5).map(|x| (x_of(x.t), y_temp(x.hottest))).collect::<Vec<_>>(), old_c);
    plot(&mut c, &n.iter().step_by(5).map(|x| (x_of(x.t), y_temp(x.hottest))).collect::<Vec<_>>(), new_c);
    plot(&mut c, &o.iter().map(|x| (x_of(x.t), y_pwm(x.pwm))).collect::<Vec<_>>(), old_c);
    plot(&mut c, &n.iter().map(|x| (x_of(x.t), y_pwm(x.pwm))).collect::<Vec<_>>(), new_c);
    text.draw(
        &mut c,
        Face::Title,
        30,
        left,
        50,
        ink,
        "Deltarune case, simulated: hottest core (top) and fan PWM (bottom)",
    );
    text.draw(
        &mut c,
        Face::Body,
        22,
        left,
        t_top + 24,
        old_c,
        "ROCKNIX quiet trips (kernel step_wise on cluster1)",
    );
    text.draw(&mut c, Face::Body, 22, left + 640, t_top + 24, new_c, "Smooth fan, Quiet curve");
    text.draw(
        &mut c,
        Face::Body,
        22,
        left,
        p_top - 14,
        dim,
        "Fan PWM (the kernel's first levels: 20, 30, 40 %)",
    );
    write_png(&buf, w, h, &out.join("fan-curve-sim.png"));
    println!("\nwrote {}/fan-curve-sim.png and fan-curve-sim.csv", out.display());
}

fn pct(p: u8) -> u32 {
    (p as u32 * 100 + 127) / 255
}
