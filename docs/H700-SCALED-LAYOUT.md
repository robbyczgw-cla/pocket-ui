# H700 scaled layouts (2026-10-28)

The live softbuffer presenter, snapshots and safe mode paint the same logical layouts and area filter.
640×480 and 720×480 use a rounded 2:1 box filter; 720×720 uses exact fractional area coverage. A native
large output bypasses the canvas and filter. The logical canvas adds 4.69 MiB at 640×480, 5.27 MiB at
720×480 and 6.25 MiB at 720×720, plus cached coverage tables. The fractional square reduction also
keeps a 7.03 MiB packed horizontal intermediate; the 2:1 path does not allocate it. There is no worker or redraw timer.

## Reproduce

Run the complete `tests/i18n-states.txt` matrix, not only hand-picked pages:

```sh
SIZES='640x480 720x480 720x720' LOOKS='dark light' TEXTS=normal KEEP=1 sh scripts/size-sweep.sh /tmp/h700-scaled
POCKETUI_SNAPSHOT_NATIVE=1 SIZES='640x480 720x480 720x720' LOOKS=dark TEXTS=normal sh scripts/size-sweep.sh /tmp/h700-native
```

The same binary generates both; the native override applies only to snapshots. Text records are taken before
reduction on the logical canvas. O is overlap, X past the right edge, E cut by an edge; T is touching text and C
intentional ellipsis. Area reduction cannot create a new geometric overlap; tiny text can still become harder to read.
The earlier research counted 617 states and included crashes; the current list has additional states and the small-size
panic fix. Compare finding counts with that difference in mind.

## CPU method and limits

`POCKETUI_FRAME_BENCH=1` on final snapshot states (`home`, `settings`, `main`) measures 101 settled frames after four
warmups, using the calling thread's CPU clock. It reports median logical paint, median paint plus reduction, and
p95 paint plus reduction. PNG encoding, startup and compositor presentation are excluded. The filter and canvas
are retained, just as in the live presenter. Use the fake library and a Wayland stub as for the snapshot sweep.
This is a single-thread path; four A53 cores do not divide its frame cost by four.

A53 times are estimates, not device measurements: multiply host CPU time by the clock ratio (host about 3.7 GHz,
H700 about 1.5 GHz), then by 2–4 for the older in-order core and lower memory throughput: approximately 5–10× in
all. No ARM instruction-level, cache or RAM calibration is claimed. Hardware testing must check sustained navigation,
Cabinet, animation, touch/rotation where present, HDMI, suspend and actual tiny-panel legibility.

Existing `App::about_to_wait` and animation settlement decide when to draw. The scaler runs only inside the existing
redraw handler, reuses its allocations and has no timer. Settled idle adds no scaling CPU. This is a code-path
property; a host or emulated container does not measure H700 system idle power.

## Full snapshot findings

The same release binary, same fake library, Normal text, English, 622 states per size. Raw findings are from
`POCKETUI_SNAPSHOT_NATIVE=1`, after the small-screen panic fix; the previous research used 617 states and aborted
before recording findings for many cards. Thus its 59% (327/555 surviving 640×480 states) is not the denominator
for this new comparison.

| Panel | Raw O / X / E | Scaled O / X / E | States with O, X or E, raw → scaled |
|---|---|---|---|
| 640×480 | 1945 / 1094 / 131 | 0 / 6 / 0 | 392/622 → 5/622 |
| 720×480 | 1867 / 1039 / 83 | 0 / 4 / 0 | 384/622 → 4/622 |
| 720×720 | 533 / 1055 / 180 | 0 / 6 / 0 | 218/622 → 5/622 |

Scaled dark and light give the same finding counts. All states render. The remaining X records are the Cards
look's deliberately clipped neighbours and first-start look previews (`shelf:gba:cards`, `curator:home:cards`,
`setup:first:a`, `setup:first:aR`, `setup:look`). These are the 4–6 right-edge records already present at the logical
reference sizes in the research. There are no overlap or vertical edge findings. Intentional ellipses are counted
separately: 193 / 174 / 176 at the three physical sizes.

## Readability inspection

JPEG quality 90, inspected at the actual pixel dimensions: Shelf (640×480 dark), Cabinet (720×480 dark), game
card (720×720 dark), Settings (640×480 light), and pause menu (640×480 dark). The settings category descriptions,
card description and pause rows remain readable; no missing panel or hint bar. The smallest hint labels are about
9–11 physical pixels and need a real-panel check. Normal text is softer than native rasterisation but a separate
text pass was not warranted by these samples: glyphs and geometry remain aligned, including the 16:9 square reduction.
Medium and Large remain available. Cabinet movement snapshots can capture the existing hint crossfade mid-motion.

Local artifacts: `/tmp/h700-scale-final-all/` (final release PNGs and records; initial sweep: `/tmp/h700-scale-after/`), `/tmp/h700-scale-native/` (raw records),
`/tmp/h700-scale-samples/` (four representative JPEGs plus the pause menu). These are development artifacts, not shipped.

## Measured CPU cost

Host: x86_64 Ryzen 5 PRO 4650G virtual CPU, nominal 3.7 GHz. Production `release` profile (opt-level 3, thin LTO, one codegen unit),
three runs per state, 101 measured frames per run. Values below are the median of the three run medians, in ms.
Thread CPU time excludes descheduling on this shared builder; frequency/cache contention can still vary between runs.
The native comparison paints the truncated raw layout, so part of the increase buys the previously missing content.

| Panel | Raw home / settings / pause | Scaled home / settings / pause | Extra total per frame | Filter alone, inferred from logical paint |
|---|---|---|---|---|
| 640×480 | 0.525 / 0.472 / 0.305 | 2.979 / 2.482 / 2.093 | 1.79–2.45 | 0.81–0.90 |
| 720×480 | 0.666 / 0.426 / 0.309 | 3.413 / 2.781 / 2.376 | 2.07–2.75 | 0.97–1.04 |
| 720×720 | 1.054 / 0.565 / 0.531 | 10.375 / 8.978 / 8.747 | 8.22–9.32 | 7.02–7.27 |

Frame p95 across these pages: 2.39–3.76 / 2.87–4.13 / 9.13–10.98 ms respectively. Applying the explicit 5–10×
A53 factor gives **10–30 / 12–34 / 44–104 ms total per changed frame**. These are estimates, not hardware results;
the square layout is the slowest and should not be assumed to sustain 60 fps on H700. Four A53 cores do not speed
this single-thread UI path fourfold. Settled idle adds no filter work or wakeups; total idle CPU has not been measured
on hardware.

The initial direct area loop cost roughly 12 ms for the square filter under that run's host load. The final path
uses exact Q6 weights in two packed RGB passes with no intermediate rounding, verified against an independent
integer-area reference. Other fractional ratios use Q14 cumulative coverage weights (constant colours preserved).
A separate synthetic copy-plus-filter benchmark is useful for repeatability but includes a copy that the live painter
does not need; the table above measures the actual live scaling function around the page painter instead.

Raw benchmark output: `/tmp/h700-scale-bench-release-final/results.txt`. The earlier no-LTO gate-profile run
(`/tmp/h700-scale-bench-optimized/results.txt`) was faster (about 1.7–7.3 ms total), illustrating why profile and
shared-host conditions must be stated rather than treating an x86 figure as a calibrated A53 prediction. No absolute timing threshold is enforced in
unit tests; filter correctness, buffer reuse, physical presentation size, output resize and logical touch hits are tested.
