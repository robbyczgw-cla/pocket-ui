//! CPU-only stereo resampler cost, independent of a sound device or the rest of pocket-ui.
//! Standalone: rustc --edition=2021 -O examples/resampler_cost.rs -o /tmp/resampler-cost
//! Run at a measured fixed clock. Ten warmup frames, then 1200 timed frames at each core rate;
//! ±0.5 % ratio changes exercise the same table throughout. Times include buffer management.
#[path = "../src/resample.rs"]
#[allow(dead_code)]
mod resample;
use std::{hint::black_box,time::Instant};
fn main() {
 for rate in [22050.0f64,32040.0,32768.0,44100.0,48000.0,65536.0] {
  let n=(rate/59.7275).round() as usize;
  let input:Vec<i16>=(0..n).flat_map(|i| {let s=(16000.0*(i as f64*std::f64::consts::TAU*1000.0/rate).sin()) as i16; [s,-s]}).collect();
  let mut r=resample::Resampler::new(true); let mut out=Vec::new();
  let cold=Instant::now();r.prepare(rate/48000.0);let prepare=cold.elapsed().as_secs_f64()*1000.0;
  for _ in 0..10 {out.clear();r.process(&input,rate/48000.0,&mut out);black_box(&out);}
  let mut ms=Vec::new(); let total=Instant::now();
  for i in 0..1200 {out.clear();let t=Instant::now();r.process(black_box(&input),black_box(rate/48000.0*(1.0+0.005*(i as f64*0.03).sin())),&mut out);black_box(&out);ms.push(t.elapsed().as_secs_f64()*1000.0);}
  let mean=total.elapsed().as_secs_f64()*1000.0/1200.0;ms.sort_by(f64::total_cmp);
  println!("rate={rate} frames=1200 mean_ms={mean:.6} p50_ms={:.6} p99_ms={:.6} max_ms={:.6} prepare_ms={prepare:.6} output={}",ms[600],ms[1188],ms[1199],out.len()/2);
 }
}
