# pocket-core audio path

The core's interleaved stereo samples are converted to 48 kHz before taking the writer queue lock. The writer
owns the output handle and blocks at its pace; emulation remains paced by video. A smoothed ±0.5% controller
corrects small clock differences. It cannot compensate for a game that consistently misses its frame budget.

## Filter and cost

The Kaiser sinc uses 72 taps, beta 11, and 512 fractional phases with interpolation. For downsampling, a wider
kernel is prepared once per core rate on a 1/32 width grid, with ±0.5% controller headroom. Rows have unity DC gain;
processing blends adjacent fractional phases with a continuous read position. The table is bounded by the maximum
4:1 step and replaces the old per-output-sample coefficient calculation and normalization. Runtime core-rate changes
can prepare a new table. Normal rate control does not allocate coefficient tables. ARM64 uses baseline NEON;
other targets retain the eight-accumulator implementation, including tails for non-multiple-of-eight tap counts.

CPU-only measurements on a Cortex-A53 at a verified 1416 MHz, 1200 stereo pushes with ±0.5% ratio changes:

| Input Hz | Previous sinc ms/frame | Prepared + NEON ms/frame |
|---:|---:|---:|
| 22050 | 0.429 | 0.349 |
| 32040 | 0.429 | 0.346 |
| 32768 | 0.434 | 0.354 |
| 44100 | 0.436 | 0.351 |
| 48000 | 0.405 | 0.320 |
| 65536 | 3.035 | 0.639 |

At 65536 Hz, preparation separately takes about 3.95 ms; steady-state p99 is 0.740 ms. These are filter timings,
not game FPS or hardware audio acceptance. Stereo is converted at the nominal core/output ratio; actual display
skew can select a nearby wider table. The 65536 Hz case saves about 2.4 ms per nominal core frame.

Host signal tests at 48 kHz output retain approximately -92 to -94 dB THD+N at 1/5/10/14 kHz where those frequencies
fit the input band. The high-rate sweep is about -97 dB at nominal ratio, close to the previous -98 dB; the 1 kHz
THD+N remains about -94 dB. The unchanged lower-rate paths are bit-identical on x86-64. ARM64's accumulation order
can round a sample differently; its filter regressions also run on the A53. Noise response, alias tones, interpolation,
chunking, rate changes and delay are measured separately by the tools below; a noise-image-band measurement includes
16-bit quantization and is not by itself filter rejection. Filter delay is about 0.75–1.6 ms across these rates. A separate coherent sine-fit grid (65 frequencies between
0.02 and 0.45 of the smaller input/output rate, each at -0.5%, nominal and +0.5% step) measures at most
0.00040 dB peak-to-peak gain variation for the prepared 65536 Hz path, versus 0.00018 dB previously.
Other rates stay below about 0.00019 dB. These are sampled gain values, not a continuous maximum guarantee.

## Buffer policy and late frames

| Output | Requested quantum | PCM buffer | Software queue target |
|---|---:|---:|---:|
| H700 speaker / wired | 1024 | 64 ms | 48 ms |
| Other Linux speaker / wired | 256 | 10 ms | 20 ms (48 ms for heavy cores) |
| Time-paced core, e.g. PAL 50 Hz on a 60 Hz display | Output's usual value | Output's usual buffer | At least 48 ms and two source frames |
| Bluetooth / no small quantum | ROCKNIX's existing value | 32 ms (64 ms on H700) | 48 ms |

These are requested buffers, not measured end-to-end latency. H700 quarters graph wakeups versus 256 and uses the cycle-buffer path with a 64 ms PCM ring.
A 32 ms ring still produced a short zero gap under QEMU with a native recorder and no observed ALSA/graph errors;
waveform measurements remain necessary alongside counters. A configured large quantum selects the cycle ring at the first open, avoiding an
undersized initial stream and unnecessary reopen before playback. The route keeper follows output changes and
restores ROCKNIX's quantum at exit. Environment overrides remain available for controlled comparisons; Android
retains its AAudio policy.

`pocket-core::sync_audio_rate` also supplies the source/display cadence to `Audio::set_frame_timing`.
A PAL 50 Hz source on a 60 Hz display repeats one picture every six display ticks, so audio bursts can be
33.3 ms apart. It needs more than the short 20 ms queue even when every core frame meets its CPU budget.
The rate controller measures fill before a delivery, while the writer starts after a delivery. Starting
at the same numeric threshold left the controller a source frame below its intended reference after
startup or resume. Both wait for `effective_target + regular_delivery`. The delivery comes from the
source/display cadence, not the unusually large first SPU buffer after a state load. This alignment also
applies to the longer PAL/cycle queue: PAL waits for 68 ms after a push to retain its intended 48 ms
before-push reserve; a 60 Hz H700 source waits for about 64.7 ms. The steady requested targets remain
20/48 ms. A video-mode change updates the shared cadence without changing resampler pitch.
An explicit test target below 20 ms retains its own target plus a delivery, so prefill remains reachable.
This extra packet is real buffering, not free latency: it can persist while the controller slowly settles.

An empty resampler output never trims the queue. Previously a repeated PAL picture could hit the overflow
cap just after resume, discard valid queued audio down to 20 ms, and starve before the next source frame.
The active queue caps latency on pushes that actually supply samples. `preplay_dropped_frames` and
`active_dropped_frames` sum to the total drop count before/after the first playing block;
`min_playing_fill_frames` is the cumulative lowest remaining queue after a playing or resume pop
(null before any such pop). Intentional pause fades remain separate from starvation.
During startup, the producer feeds the complete PCM packet into the queue in restart-sized
pieces, waiting for the writer to make room. This preserves the opening sound while route selection and
PipeWire linking run, including an oversized restored SPU packet. Previously the preplay cap discarded the
prefix repeatedly during a slow start, or let a prefix play before the next push removed its middle.
Device silence is primed immediately before real PCM starts; priming it before waiting for the game queue
lets the server drain that silence and burst-read the first game samples. Priming waits for two writes
longer than 4 ms: the first may still be stream linking, followed by a fast server-buffer fill. A second
paced write keeps that burst in planned silence rather than consuming the opening PCM. The 200-write
and shutdown bounds remain. Startup backpressure is reported
as `startup_backpressure_us`. A closed output, or two seconds without accepting another piece, reports the
unplayed tail as dropped rather than trapping the game or claiming intact sound.
Flow control continues until the route is decided, PCM has played, and the queue fits the current restart reserve.
This also preserves the prefix queued for 48 ms when route selection chooses 20 ms before the first pop.
After that handover, the existing active overflow policy remains unchanged. Host regressions check the
first PCM prefix through a delayed stream, bounded stereo delivery, the startup route shrink, a linking write followed by fast server-buffer filling, the observed 2925-to-960-frame loss,
and 240 seconds of PAL with 20 pauses, two-block writer bursts and a late delivery. The PAL test checks every
sample's order and the full produced/consumed/queued/dropped balance through warmup and later playback.
The resumed video clock anchors its first deadline when pause ends. An immediately presented first picture
therefore cannot be followed by a second source frame on the same tick, accumulating an extra audio burst on
every resume until the overflow cap discards it. This also protects sources near 60 Hz.
With `POCKETUI_TRACE` enabled, `audio-push`, `audio-write`, `audio-pause` and `audio-underrun` record burst sizes, queue fill
and missing frames against the common monotonic clock; normal games do not format or write these events.

If the queue holds less than one 256-frame write, the writer consumes the available stereo prefix and zero-pads
only the missing tail. Previously it emitted an entirely silent block and retained the valid prefix for later,
expanding the gap. `playing_missing_frames` and `playing_silence_frames` now both count the actual padded tail;
`playing_underruns` counts the affected writes. Startup, pause and refill silence remain separate.

## RetroArch reference and validation

ROCKNIX 20261001 pins RetroArch [bdba046](https://github.com/libretro/RetroArch/tree/bdba046fa6766380bc2457532f38e589df769aaf).
Its H700 configuration requests `alsathread`, 44100 Hz and sinc quality 2. In the pinned
[resampler enum](https://github.com/libretro/RetroArch/blob/bdba046fa6766380bc2457532f38e589df769aaf/libretro-common/include/audio/audio_resampler.h),
2 is LOWER, not NORMAL (3). LOWER uses 8-tap Lanczos, NORMAL 16-tap Kaiser; both widen for downsampling.
The [threaded ALSA driver](https://github.com/libretro/RetroArch/blob/bdba046fa6766380bc2457532f38e589df769aaf/audio/drivers/alsathread.c)
uses a bounded FIFO, blocks the producer when full in synchronous mode, and zero-pads just the missing tail in its
worker. PCM buffering and the software FIFO are separate. Its dynamic controller uses FIFO free space around half
capacity; pocket-core uses a smoothed queue observation to avoid pitch modulation.

At equal 48 kHz output, A53 timings for 65536 Hz are 0.114 ms for RetroArch LOWER and 0.202 ms for NORMAL, versus
0.639 ms for the higher-rejection pocket-core filter. At the configured 44.1 kHz output they are 0.105/0.186 ms.
The filters have materially different passbands and rejection, so their costs cannot establish equal quality.
No code is copied from RetroArch into the production filter.

Reproduction tools and interpretation: [audio bench README](../scripts/audio-bench/README.md),
[resampler CPU bench](../examples/resampler_cost.rs), [signal generator](../examples/resampler_bench.rs), [coherent response grid](../examples/resampler_response.rs).
Use the original waveform limits and the backend/session diagnostics together. Queue-underrun zero cannot prove
an intact stream: accepted frames, write heartbeat, ALSA errors and verified PipeWire observations also matter.
A null sink or QEMU userland cannot prove the physical DAC clock, output routing, or audible hardware quality.


A 600 s H700-userland tone run under QEMU, with a native recorder on the same server, passed the unchanged
waveform limits at quantum 1024, PCM 64 ms and queue 48 ms: zero waveform events, worst 50 ms sine-fit THD+N
-71.7 dB, pitch variation 0.043 cents RMS / 0.229 max. Playback queue, ALSA recovery and observed PipeWire
errors were zero. The native recorder's final raw ERR counter was 1 (3 in the corresponding H700 Bluetooth
run, whose WAV also met the limits). These capture errors prevent a complete waveform-coverage claim;
`pipeline-matrix.py` reports them separately and returns nonzero. PipeWire playback sampling also has explicit
partial coverage at the session tail. The observations support testing this buffer policy; physical H700 output, analog distortion and audible comparison remain separate checks.
