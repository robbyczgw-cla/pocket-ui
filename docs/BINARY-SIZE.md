# Release binary size

The 32 MiB STORAGE image must hold all three programs before first boot grows the filesystem. Use full LTO,
keep opt-level 3 and one codegen unit, strip symbols, and abort panics. The latter three settings were already
in use. Full LTO saves 1.95 MiB without adding startup I/O, decompression, allocations or audio work.

## Size comparison

Measured on source `bbce0421588c`, Rust 1.99.0, target `aarch64-unknown-linux-gnu.2.38`, with identical features,
path remapping and source in both builds; the only code-generation difference is `thin` versus `fat` LTO.
Symbols were retained for attribution, then stripped with LLVM's `--strip-all` for the sizes below.
The manually built comparison has development version strings; release labels can change the last few bytes.

| Program | Thin LTO, bytes | Full LTO, bytes | Saved, MiB |
|---|---:|---:|---:|
| pocket-ui | 19,644,664 | 18,140,104 | 1.435 |
| pocket-core | 6,082,048 | 5,593,224 | 0.466 |
| pocket-splash | 1,996,320 | 1,949,552 | 0.045 |
| Total | 27,723,032 | 25,682,880 | 1.946 |

`scripts/image/storage-budget.sh` rounds each file up separately to 4 KiB, includes the launcher and helper files,
and conservatively counts directories. It gives 730 blocks = 2.852 MiB free after the change, versus 231 blocks =
0.902 MiB on the same source with thin LTO. Historical 0.3.1 left about 1.05 MiB in the actual images; subsequent
code growth accounts for the smaller comparison baseline here. This is a budget calculation, not a newly built image.
The release-program budget now requires 512 blocks = 2 MiB (or a larger pinned image minimum). Old release
binaries physically fit but deliberately fail that reserve for future releases. The finished-image/receipt pin
stays at 128 blocks so existing immutable 0.3.1 image attestations remain valid. The test first failed with the old
release floor; it checks the measured program sizes and both sides of the new boundary.

## Where the bytes were

`nm -S --size-sort --demangle` on the unstripped AArch64 programs supplies function/crate attribution; `readelf -SW`
supplies section sizes. This is linked output, not a count of source lines. Generic instantiations can be attributed
to `core`/`alloc` rather than their calling crate; Rust's `core` here is not the pocket-core executable.

| Program | Largest linked text contributors, bytes | Largest functions, bytes |
|---|---|---|
| UI | pocket_ui 6,380,168; core 1,907,936; std 976,072; alloc 318,412; winit 239,956 | App::paint_settings 77,652; Setup::paint 68,792; bezel::generate 57,792; snapshot 51,596 |
| Core | pocket_ui 947,400; core 435,996; std 380,380; C/other 261,408; winit 209,916 | bezel::generate 57,792; main 55,852; App::step 26,820; Wayland state::new 18,500 |
| Splash | core 115,912; std 95,604; pocket_ui 89,108; png 40,288; ttf_parser 37,592 | backtrace cache 11,028; fdeflate::read 9,792; CFF char-string parser 9,188 |

The baseline `.rodata` sizes are 6,756,460 / 2,076,208 / 1,215,600 bytes (UI/core/splash). Searching the linked bytes
for complete asset payloads confirms these opportunities:

| Embedded data | Payload bytes | Copies across programs | Decision |
|---|---:|---:|---|
| German + Spanish translations | 704,077 | 3 | Keep embedded; lossless zlib could save about 1.35 MiB total, but adds a once-per-language allocation and decompression. A prototype round-tripped every byte and passed 22 translation tests; it is not included. No runtime claim is made for it. |
| Fonts | 348,300 | 3 | Keep embedded: moving to files would add early card I/O and require installer/update/fallback work. |
| UI sounds | 709,998 | 2 | Keep embedded: no new read/decode work on sound playback. |
| Game facts + Curator databases | 1,639,526 | 1 | Already compressed by system; moving to FAT could save 1.56 MiB but needs matching data delivery and missing-file handling. Not needed for this reduction. |
| PNG art | Various | Mostly UI | Already compressed; no new decode or asset-loading path. |

`cargo tree -d --target aarch64-unknown-linux-gnu` also finds bitflags 1/2 (png versus windowing), rustix 0.38/1
(windowing generations), miniz_oxide 0.8/0.9 (our data reader versus flate2/png),
and linux-raw-sys 0.4/0.12 beneath rustix. These are transitive API generations; there is no safe manifest-only deduplication. Dev-only getrandom/tempfile and host-only
proc-macro dependencies must not be counted as shipped duplication. No dependency upgrades were made.

## Runtime verification

The comparison uses the existing `scripts/latency.py` trace harness, `POCKETUI_FRAME_BENCH` (105 paints per process,
four warmups), and real `pocket-core --bench` runs. All three languages and UI/splash snapshots are compared.
The handheld comparison uses the same source and optimisation settings with the Brick feature and glibc 2.33;
the STORAGE sizes above are the normal ROCKNIX build, without the Brick feature.

The virtual H700 profile ran two trace passes (ten UI starts), three frame-benchmark processes for each of
Home/Settings/pause in English/German/Spanish, and ten-second GBC core windows per language. UI start was 37.1 →
36.5 ms, main-to-first-frame 34.4 → 33.8 ms, Settings open 3.6 → 3.5 ms. Frame CPU medians (including reduction)
were 164–293 → 164–294 µs; median per-process p95 values were 176–305 → 179–310 µs. Switcher opening varied
7.9 → 11.9 ms and pause opening 14.3 → 17.1 ms: recorded as small host timing variation, the comparison accepts sub-10-ms host shifts rather than treating them as a user-visible regression. No startup speedup is claimed from the first game launch: its cold initialisation
was not equally warm; second-pass GBC launch was about 70 ms in both runs.

The software-rendered VM itself presents only about 51–53 fps on the H700 profile (55–57 on the larger Mini
profile), with audio starvation already in the thin-LTO baseline. Core emulation CPU remained 0.37–0.38 ms/frame,
and p99 presentation intervals about 23 ms in both builds. These runs establish a comparison, not acceptable
hardware pacing or dropout-free audio. All pause and splash snapshots are byte-identical in three languages;
Settings differs only in the clock. Home has the clock plus tiny rendering differences (German mean absolute
channel difference 0.0066 on a 0–255 scale), without a changed layout or text.

A separate real-time `audio_tone` test through the existing audio harness ran 60 seconds per graph/build,
independently of the VM graphics. These are short regression samples, not a ten-minute audio soak.

| Audio graph | Thin → full LTO |
|---|---|
| Speaker | 0 → 0 waveform glitches; THD+N worst −70.2 → −70.1 dB, median −85.7 → −85.9 dB; pitch wobble max 1.290 → 1.722 cents |
| Bluetooth | 0 → 0 waveform glitches; THD+N worst −69.7 → −69.5 dB, median −80.6 → −80.7 dB; pitch wobble max 1.366 → 1.547 cents |

Both builds report zero playing-queue underruns and ALSA xruns in these recorded windows. PipeWire reports no
observed xruns but explicitly marks final-tail coverage **partial**; this is not a session-wide zero-error claim.
The native Brick check used four alternating 20-second GBC windows (thin/full/thin/full), the production CPU
copy path, isolated RAM programs/data/ROM/BIOS/states, and rumble off. Process-start to first frame was 292.4/242.0 → 223.8/229.7 ms. Both builds presented 60.00 fps and p99
16.67 ms. Thin had one >25-ms interval in its first window and none in its second; full LTO had none in either.
Core emulation averaged 0.86/0.84 → 0.84/0.85 ms per frame. Both reported zero audio underruns, with server plus
queue latency 84/85 → 84/85 ms. The session held the shared device lock, muted sound, restored configuration
hashes and CPU governor/limits, resumed the same installed UI, stopped and verified both guard processes,
removed its RAM scratch and ended unmuted. No installed executable or game save was replaced.

## Repeating the measurement

Build an unchanged source twice, setting `CARGO_PROFILE_RELEASE_LTO=thin` and then `fat`, with
`CARGO_PROFILE_RELEASE_STRIP=false` and the normal release path-remapping setup. Copy each output immediately
before another build can overwrite it. Use the unstripped copies for `nm` attribution and independently stripped
copies for the storage calculation. Compare the same target, version, feature flags, rustc and linker.

Run `scripts/image/test-storage-budget.sh` and `scripts/image/storage-budget.sh BIN_DIR` on the final programs.
Run the trace and frame benchmarks with the same library/settings and compare rendered snapshots. If pocket-core
code generation changes, also run the native handheld check; a headless software renderer does not establish
hardware frame pacing or audio quality. `examples/i18n_cost.rs` times the actual once-only language loader, then
cached string lookups, without embedding a separate plain-text table in the benchmark.

Do not lower the shared crate to `s`/`z`: the crate also contains the emulator, audio, image and font hot paths.
Cold UI code would need a measured separation before a per-package size profile could be justified. One codegen
unit already gives the cross-function optimisation opportunity; no codegen-unit or size-opt changes were adopted.
