# Audio bench: pocket-core's sound path measured off the device

Measures what a player hears from pocket-core, with numbers, without the Pocket: ROCKNIX's own PipeWire 1.2.6 and
WirePlumber run in a container of the virtual device's base image (`pocket-vdev-base`, docs/VIRTUAL-DEVICE.md),
`examples/audio_tone.rs` plays a 1 kHz sine at mGBA's 65536 Hz, one frame per 60 Hz refresh as pocket-core feeds a
GBA game, through `pocket_ui::audio::Audio` (resampler, rate control, ALSA's `pipewire` device, `audio_route`) into a
sink that drives the graph on its own timer, and the sink's monitor is recorded and analysed.

```sh
scripts/audio-bench/bench.sh            # 60 s per case, results in /tmp/audio-bench, exit 1 on a broken limit
scripts/audio-bench/bench.sh 600        # the 10-minute run
BENCH_CASES="bluetooth" scripts/audio-bench/bench.sh 30
```

`AUDIO_BENCH_HOST` is the docker host with the vdev base image (an Apple-silicon Mac with colima is aarch64 and fast;
`local` for this machine if it has the image and is aarch64), from the environment or `scripts/local.env`. This
machine needs python3 with numpy; the binary is built here with zigbuild.

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
