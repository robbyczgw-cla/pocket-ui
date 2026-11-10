// The browser demo's sound (docs/WEB-DEMO.md): an AudioWorklet that plays pocket-ui's UI mixer from the ring in shared
// memory (web/web_audio.rs: 48 kHz stereo i16, WRITE and READ counters). Linear resampling when the page's AudioContext
// doesn't run at 48 kHz. Twice a second it tells the page what it played: frames with sound, the loudest sample, and
// underruns (the ring ran dry while a sound was playing: a gap, heard as a click).
"use strict";
class PocketUiSound extends AudioWorkletProcessor {
  constructor() {
    super();
    this.ring = null;
    this.pos = 0;
    this.step = 48000 / sampleRate;
    this.stats = { frames: 0, loud: 0, peak: 0, underruns: 0 };
    this.sounding = false;
    this.since = 0;
    this.port.onmessage = (e) => {
      const { memory, buf, frames, w, r } = e.data;
      this.ring = { samples: new Int16Array(memory, buf, frames * 2), w: new Uint32Array(memory, w, 1), r: new Uint32Array(memory, r, 1), frames };
    };
  }
  process(_inputs, outputs) {
    const out = outputs[0], left = out[0], right = out[1] || out[0], n = left.length;
    const ring = this.ring;
    if (!ring) { left.fill(0); right.fill(0); return true; }
    const r = Atomics.load(ring.r, 0), w = Atomics.load(ring.w, 0);
    const avail = (w - r) >>> 0;
    // Frames this block takes from the ring (at 48 kHz), as many as there are.
    const need = Math.ceil(this.pos + n * this.step);
    const take = Math.min(avail, need);
    let loud = false;
    for (let i = 0; i < n; i++) {
      const p = this.pos + i * this.step, a = Math.floor(p), f = p - a;
      if (a + 1 >= take) { left[i] = 0; right[i] = 0; continue; }
      const i0 = ((r + a) % ring.frames) * 2, i1 = ((r + a + 1) % ring.frames) * 2;
      const l = (ring.samples[i0] * (1 - f) + ring.samples[i1] * f) / 32768;
      const rr = (ring.samples[i0 + 1] * (1 - f) + ring.samples[i1 + 1] * f) / 32768;
      left[i] = l; right[i] = rr;
      const m = Math.max(Math.abs(l), Math.abs(rr));
      if (m > 0.001) loud = true;
      if (m > this.stats.peak) this.stats.peak = m;
    }
    const used = take >= need ? Math.floor(this.pos + n * this.step) : take;
    this.pos = take >= need ? this.pos + n * this.step - used : 0;
    Atomics.store(ring.r, 0, (r + used) >>> 0);
    if (take < need && this.sounding) this.stats.underruns++;
    this.sounding = loud;
    this.stats.frames += n;
    if (loud) this.stats.loud += n;
    this.since += n;
    if (this.since >= sampleRate / 2) {
      this.port.postMessage(this.stats);
      this.stats = { frames: 0, loud: 0, peak: 0, underruns: 0 };
      this.since = 0;
    }
    return true;
  }
}
registerProcessor("pocket-ui-sound", PocketUiSound);
