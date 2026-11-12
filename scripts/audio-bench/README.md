# Audio bench: pocket-core's sound path measured off the device

Measures what a player hears from pocket-core, with numbers, without the Pocket: ROCKNIX's own PipeWire 1.2.6 and
WirePlumber run in a container of the virtual device's base image (`pocket-vdev-base`, docs/VIRTUAL-DEVICE.md),
`examples/audio_tone.rs` plays a 1 kHz sine at mGBA's 65536 Hz, one frame per 60 Hz refresh as pocket-core feeds a
GBA game, through `pocket_ui::audio::Audio` (resampler, rate control, ALSA's `pipewire` device, `audio_route`) into a
sink that drives the graph on its own timer, and the sink's monitor is recorded and analysed.

```sh
export AUDIO_BENCH_BIN=/path/to/prebuilt/aarch64/audio_tone
scripts/audio-bench/bench.sh            # 60 s per case, results in /tmp/audio-bench, exit 1 on a broken limit
scripts/audio-bench/bench.sh 600        # the 10-minute run
BENCH_CASES="bluetooth" scripts/audio-bench/bench.sh 30
```

`AUDIO_BENCH_HOST` is the docker host with the vdev base image (an Apple-silicon Mac with colima is aarch64 and fast;
`local` for this machine if it has the image and is aarch64), from the environment or `scripts/local.env`. This
machine needs python3 with numpy and a prebuilt aarch64 tone binary.
`AUDIO_BENCH_BIN` selects a prebuilt binary and does no build. Without it the script stops; set
`AUDIO_BENCH_BUILD=1` only when you intend to build on this host and have its build slot. That opt-in uses zigbuild,
four jobs and `CARGO_TARGET_DIR` when set (including when locating the resulting binary).

## Cases

| Case | Graph | What it stands for |
|---|---|---|
| `speaker` | the game sets 256 itself (`audio_route`), sink latency 0 | the built-in speaker |
| `bluetooth` | ROCKNIX's 1024 stays, the sink reports 140 ms of its own | Bluetooth headphones (bluez5's sink is a driver of its own, `node.driver`) |
| `bluetooth-skew` | as `bluetooth`, the "display" 0.1 % fast | the display's and the sink's clocks apart; the rate control has to make it up |
| `pauses` | both outputs, 20 s: pause menu at 5-6 s and 12-12.3 s, quit through the pause menu | clicks at start, pause, resume, quit |

`run-case.sh` is one case inside the container; `bench-sink.conf` is the sink (a `support.null-audio-sink` that is
the driver, S24_32 like the device's speaker). On Bluetooth the case takes `audio_route`'s Bluetooth branch through
`POCKET_CORE_QUANTUM=0` (no quantum of its own: the same `game_quantum == None`), since the container has no
Bluetooth link.

## Session diagnostics alongside the waveform

Every case retains its `.tone.log`, `.pwtop`, `.pwtop.end` and `.pwtop.short` beside the WAV, including diagnostic
evidence from a failed run. `audio_tone` logs `audio sample {JSON}` once per second; its normal `Audio` teardown logs
`audio session {JSON}`. `session.py CASE.tone.log` extracts the final session unchanged to `.session.json`, the
one-second snapshots to `.samples.jsonl`, and prints/writes `.diagnostics.json` with the backend, open count,
negotiated PCM rate/buffer/period, accepted frames, heartbeat age, write gaps/durations, ALSA recoveries and write/query
EPIPE observations, queue phases and PipeWire coverage. The original log preserves all session records.

The parser reports unavailable data as null. An unopened backend, no accepted frames, a heartbeat older than one
second, an unverified writer stop or missing session JSON fails the diagnostic check. Startup prefill waits, paused
silence and refill silence remain separate from playing underruns and missing frames; they do not become playing
starvation. Successful silent ALSA recovery still counts as a recovery and an observed error. A generic `pw-top` ERR
column is raw context only: on some versions a follower can display its driver's error count.

A final PipeWire snapshot intentionally covers only verified observations. Its `observed_xruns` can be zero while
`client_xruns` is null and `final_sample_age_ms` marks the unobserved tail. That remains **partial**, not a session-wide
zero-error claim. Before the first verified sample even the observed accumulator is reported as null. Unsupported
ALSA counters on Pulse/Android and failed PCM queries likewise remain explicit. Partial measurement coverage alone
does not fail the waveform check; known errors and incomplete writer evidence fail the diagnostic check. `bench.sh`
returns nonzero if artifact collection, session diagnostics or the existing waveform limits fail. Waveform thresholds
in `analyze.py` are unchanged, and these counters alone do not prove audible quality.
ALSA history survives fallback or reopening. If any Pulse/Android stream was opened, those historical ALSA values
cover only the ALSA streams; the session remains partial even if its retained ALSA counters are zero.
Run the diagnostic fixtures locally with `python3 scripts/audio-bench/test_session.py` (standard library only).

## UI sounds through a game (off the device)

`ui-guest.sh [OUT]` (from the audit branch fix/audio-after-guest-sol, `AUDIO_BENCH_HOST`): `examples/audio_ui_guest`
drives the real UI mixer (`ui_sound`) through a launch, the pause menu over a guest (a silent pulse client at
ROCKNIX's 1024, or the same with 256 forced as a pocket-core game does), the game's end (`ui_sound::game_ended`),
240 clicks every 250 ms and, after 3 s of quiet, 20 more. `analyze-ui-guest.py` aligns every click on the Nav WAV
and reports, per group, D+N against it and padded audible frames; the first click after the start, in the guest and
after the quiet are groups of their own (the moments the quantum used to change under a sound). Any click above
-40 dB D+N or with a padded frame fails.

## UI sounds after a game (on the device)

`ui-after-game.sh [GAME] [PLAY_SECONDS] [OUT]` (default `xbox:jade 20`): on a Mini V2 (`POCKETUI_HOST`, under the
device lock) it stops pocket-ui, starts it with the trace as `scripts/latency.py` does, records the speaker sink's
monitor (before volume and mute, so it works and stays silent with the speaker muted; the script never touches either) while it moves on the shelf, starts GAME, opens the pause menu and moves there, quits, goes back
to the shelf and moves at once, then again after the output has closed; it follows `clock.force-quantum` meanwhile,
puts pocket-ui back and runs `verify-device.sh`. pocket-ui plays no menu sound into a muted sink: with the speaker
muted (at night) run it with `UAG_SINK=null`, which records a null sink made the default for the run and puts
everything back after it. `analyze_ui.py OUT` counts gaps per phase: runs of
digital silence, at most a cycle long, that cut a sound off (a quiet tail passing through zero doesn't count).
Results: docs/HOST.md "UI sounds", "Buffer and quantum".

## What analyze.py measures, and the limits

| Measure | How | Limit |
|---|---|---|
| glitches | over the whole tone (fade-in, pauses, end): the sine recursion `x[n+1] + x[n-1] - 2cos(w)x[n]`, a few LSB for a clean tone, ~60 at the corner of the 5 ms pause fade; above 200 LSB is a dropout, a padded block or a click | 0 |
| THD+N | 50 ms blocks, least-squares sine fit (with slow drift terms), worst block | ≤ -60 dB |
| pitch wobble | frequency over 20 periods from zero crossings, cents from the median, RMS and max | ≤ 1 rms, ≤ 3 max |

THD+N and pitch count only where the tone has played for 1 s and goes on for 0.2 s. `--expect` also prints how far the
median is from the core's own pitch (1004.56 Hz: a 1 kHz tone of a 59.73 fps core run at 60 Hz).

## Results (2026-10-28, PipeWire 1.2.6 in the container on an M4 Mac)

Before and after the Bluetooth fix: docs/HOST.md "Bluetooth audio". In short, before: 373 dropouts in 8 s on the
Bluetooth graph, pitch wobble up to 7 cents; after, 10 minutes each: no dropout, THD+N -66 to -68 dB, wobble below
0.5 cents (1.8 with the clocks 0.1 % apart, which the rate control has to follow).

PipeWire has no real-time priority in the container (no rtkit): with several benches at once on a busy host the
speaker graph (5 ms cycles) can miss a cycle, which shows as 256 zero frames and the same ERR count in pw-top for the
recorder (`bench_rec`) as for pocket-core's stream (`*.pwtop.end` next to the recording). Run one case at a time for
a verdict.

## Resampler alone (no container)

`examples/resampler_bench.rs` runs the linear and the windowed-sinc converter (`src/resample.rs`) over tones, a sweep, noise
and an impulse at 32768, 32040, 44100 and 48000 Hz and writes raw files; `resampler.py DIR` prints the foreign-line level,
THD+N, response, latency and CPU tables (numpy). Results: AUDIO-RESAMPLER.md in the review folder.
`cargo run --release --example resampler_bench -- /tmp/rs-out && python3 -I scripts/audio-bench/resampler.py /tmp/rs-out`


## Core-rate and pipeline comparisons

`resampler_bench` also covers 22050 and 65536 Hz, including high-rate alias probes. `resampler.py` omits tones
outside the shared passband and accepts `--out-rate` for a separate 44.1 kHz reference directory. It reports the
raw measurements without treating a high noise-image band at an exact rational ratio as filter alias rejection.
`examples/resampler_cost.rs` is a standalone CPU benchmark (see its header): use identical binaries, 1200 frames,
a fixed measured clock, and report the separate preparation cost. Ordinary ratio changes keep one table.

`retroarch-reference.py SIGNAL_DIR` fetches the pinned ROCKNIX RetroArch implementation and headers with their
MIT notices into the output directory and compiles a reference library. The H700 config's quality **2** means
**LOWER** (8-tap Lanczos before downsampling expansion), while **NORMAL** is **3** (16-tap Kaiser). It writes
`ra-lower` and `ra-normal` files alongside the input signals, plus `retroarch-meta.txt`; pass those mode names to
`resampler.py`. SIMD mask zero represents ROCKNIX's aarch64 build. Use `--out-rate 44100` in a separate directory
for the configured H700 rate; 48000 compares filters at the same output rate as pocket-core.

`pipeline-matrix.py OUT --bin /path/to/audio_tone --image IMAGE --seconds 30 --cases h700 bluetooth`
compares the production profiles and optional buffer experiments in one isolated container, retaining WAVs,
writer/session counters and failures. It reuses the unchanged `analyze.py` and `session.py` limits. The optional
`--start-ms` changes only the tone example's first-push delay (default 100 ms); label any slower-host exploration.
A QEMU run measures the actual userland and filter arithmetic, but its scheduling is not evidence of hardware
real-time performance. Full audio acceptance still requires physical output and a progressing hardware clock.

`examples/resampler_response.rs` measures a coherent sine-fit gain grid from 0.02 through 0.45 of the smaller
input/output rate, at nominal and ±0.5% ratios. It reports peak-to-peak passband variation without the random
band-average uncertainty of the noise test.

For QEMU, `pipeline-matrix.py --recorder-image IMAGE` can run a native `pw-record` against the same ARM PipeWire
server over a shared Unix socket. The optional image must contain PipeWire tools and libsndfile. Only recording
moves to that image; the sender, ALSA plugin, graph and output policy remain in the target userland. The original
all-ARM recordings stay valuable failure evidence: inspect `bench_rec` errors as well as the playback node. A
recorder with its own xruns cannot establish waveform acceptance, even if playback counters are clean.
The matrix writes `.capture.json` and returns nonzero when the final raw recorder ERR is positive or its active
row is unavailable. A zero remains a partial snapshot, never a complete capture-coverage guarantee.

## Brick codec control curve

`capture-brick-volume.py --host HOST --out OUT --owner-watch-binary ARM_HELPER` holds the device lock, refuses an active game,
requires the original UI to be dark, and uses private remembered levels. Build `scripts/brick/owner-watch.c` for the Brick as
in [the development guide](../../docs/DEVELOPING.md). Both output paths and the speaker amp remain muted; it records real
codec readbacks at 5% intervals and restores the full ALSA state and amp mute exactly. It never starts a PCM stream.
The device-side watcher restores on physical input even without a network; every helper ends with the bounded session.
Run `brick-volume.py OUT/readbacks.json --csv OUT/levels.csv` to report relative gain from the inverted vendor control,
not acoustic SPL. [Measured levels and limits](../../docs/TRIMUI-BRICK-04.md#volume-real-codec-readbacks-silent).
