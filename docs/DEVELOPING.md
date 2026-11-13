# Developing pocket-ui

How to build, run and test pocket-ui and pocket-core. For how the pieces fit together, read
[ARCHITECTURE.md](ARCHITECTURE.md) first. This page is the short core every contributor and agent reads; the long
reference is split by topic in [`docs/dev/`](dev/) and is read only when you work on that topic.

## Topic files

| Topic file | Read when you … |
|---|---|
| [Building and tools](dev/building.md) | you build, cross-build or package pocket-ui / pocket-core, or need the binaries table |
| [Real device and virtual device](dev/device-loop.md) | you deploy to or test on a handheld, or use the virtual device (vdev) |
| [Start time](dev/start-time.md) | you touch start-up, instant start or anything that could slow the first frame |
| [Tests and documentation checks](dev/tests.md) | you write or run tests, or change docs/user (the "Documentation checks" table is here) |
| [Merge gate](dev/merge-gate.md) | you queue a change, read a gate log or change gate.sh / the train |
| [Feature tour](dev/tour.md) | you add a feature that needs a tour card |
| [PortMaster ports](dev/portmaster.md) | you work on PortMaster ports |
| [Environment variables](dev/environment.md) | you add or read a POCKET* variable (doc_checks holds this list) |
| [Repository layout](dev/layout.md) | you add or move a module or file in src/ (doc_checks holds this list), or look for where code lives |
| [Releasing](dev/release.md) | you cut a release or build release assets |
| [Consoles and game data](dev/consoles.md) | you edit console fact sheets, credits, gamedb or Curator data |
| [Translations](dev/translations.md) | you add or change UI text, German/Spanish lines or the i18n snapshots |
| [Conventions, details and regressions](dev/conventions-details.md) | you touch Wi-Fi activation, text fit / long titles or small-panel layout measuring |
| [Android and platform adapters](dev/android.md) | you work on the Android app or the platform adapters |
| [Standalone Brick Bluetooth](dev/brick-bluetooth.md) | you work on Bluetooth, controller layouts or save-safety / literal-name regressions |

## Conventions

- Animation deadlines include painting in the refresh budget (`src/frame_pace.rs`, `main.rs`); still pages continue to wait for input or real deadlines. Guard: `cargo test --profile gate --lib frame_pace::` (14 ms paints keep 60 animation frames/s, 25 ms paints keep 40, fast paints remain capped).
- Panics end the process (`panic = "abort"`). Anything that can fail for a moment (a surface, a buffer, GL
  setup) returns an error value; see `src/frame.rs`. The panic hook writes `crash.log` and leaves a notice for the
  next screen (`src/crash.rs`).
- Files the user would be upset to lose are written through a temporary file and a rename (`src/fsutil.rs`).
- Pictures from files (covers, previews, screenshots, frames) are decoded only through `image::png_reader`,
  `image::load_png`/`load_jpeg`/`load_image` or `bezel::load_png_rgba`, which refuse a header larger than
  `image::MAX_EDGE` / `image::MAX_PIXELS` before allocating anything; a damaged or hostile file is then "no picture",
  not an out-of-memory abort (review N2).
- UI text uses sentence case and plain words. Buttons are called by the names printed on the Mini V2 (A is the
  right face button).
- Documents in `docs/user/` describe only what exists. Anything planned is marked "planned".
- Verification state lives in [STATUS.md](STATUS.md): change a feature's proof, change its row ([Documentation checks](dev/tests.md#documentation-checks)).

## Code shape

Counted in `doc_checks` (`the_code_shape_counts_never_go_up`); the counts may only go down. The caps are in
`tests/code-shape.txt`: whoever lowers a count lowers its cap in the same commit. `pub fn` is only printed, not
capped: a new module may bring its few `pub` items (rule 9).

1. Parse at the edge: text from files, env, argv, the FIFO or another program becomes a type in the module that reads
   it. Nothing downstream compares strings (`emulator == "pocket-core"` → `Player::Hosted`).
2. A game is one value (`Run { system, player, core, path }`), not four `&str` parameters.
3. One reader per setting or system.cfg key; a second reader calls the first.
4. Errors a caller acts on are enums. `Result<_, String>` only when the text goes straight to the screen or log. Never
   match on error text.
5. `let _ =` or `.ok()` on a write, rename or delete needs a comment saying why losing it is fine. For saves, states
   and settings it never is: return the error.
6. No `unwrap`/`expect` outside tests: with `panic = "abort"` it is a crash on the device.
7. New items are private. `pub` needs a caller in another module in the same commit.
8. No helper for a single call site unless it has its own test. On the second use, extract; don't copy.
9. A new feature is a new module with at most 5 `pub` items; `main.rs` and `App` get one field and one call.
10. Touch a file, leave its counts lower. No refactor-only branch over 300 changed lines.

## What you need

- A Rust toolchain (stable, edition 2021) and the `aarch64-unknown-linux-gnu` target:
  `rustup target add aarch64-unknown-linux-gnu`.
- [`cargo-zigbuild`](https://github.com/rust-cross/cargo-zigbuild) and [Zig](https://ziglang.org), to cross-build
  for the device from macOS or Linux.
- Docker with an aarch64 engine (colima or Docker Desktop on Apple silicon), for the virtual device and for the
  Linux-only tests.
- For the virtual device: a ROCKNIX SM8250 image on your disk. It is not part of this repository.
- For a real device: a Mini V2 running ROCKNIX with SSH switched on, reachable from your computer.

Build for the handheld with [cargo-zigbuild](https://github.com/rust-cross/cargo-zigbuild):

```sh
rustup target add aarch64-unknown-linux-gnu
cargo install cargo-zigbuild
cargo zigbuild --release --target aarch64-unknown-linux-gnu.2.38 --bin pocket-ui --bin pocket-core
```

`cargo test --release` runs the tests on a Linux or macOS computer, and `scripts/vdev.sh` starts a virtual Mini V2 in
Docker.

The scripts were written on macOS. `scripts/devlock.sh` uses BSD `stat`, and `scripts/flash-sd.sh` uses
`diskutil`, so these two need changes to run on Linux.

`deploy/brick/hwroot/usr/bin/brick-suspend` owns the Brick's suspend transition (Power button: Sleep);
`scripts/test-brick-suspend.py` checks it through the real dispatcher, `scripts/brick/journey.py --phase sleep`
on the device ([TRIMUI-BRICK-SLEEP.md](TRIMUI-BRICK-SLEEP.md)).

Brick checks: `examples/brick_keys.rs` replays save/load/screenshot actions against the real pad descriptor without writing
an event to it. `scripts/brick/owner-watch.c` is a private session helper, built with
`zig cc -target aarch64-linux-gnu.2.33 -O2 -s scripts/brick/owner-watch.c -o brick-owner-watch`;
pass it to `scripts/brick/journey.py --owner-watch-binary PATH` for device-side cancellation across suspend.
`--phase lights --key-probe PATH` covers the settings choices and idle lights. The helper and probe run from session RAM,
then are removed. `scripts/audio-bench/brick-volume.py` converts measured mixer readbacks into relative dB steps and CSV.

Android update work spans shared `src/update/channel.rs`, `src/signed_data.rs`, `src/update_page.rs`,
the Android page/worker (`android/src/updates_page.rs`, `updates.rs`) and `UpdateInstaller.java`.
Run the focused `android_updates` UI tests and `scripts/android/test-release-channel.py` with a built
`STORE_TOOL`, plus the Android compile check. Channel preparation is described in
[UPDATE-CHANNEL.md](UPDATE-CHANNEL.md#android-apk-channel).

The drawer under the shelf (Android apps and the Android shelf): `android/src/apps_page.rs` and
`android/java/xyz/pocketui/app/AppDrawer.java`; `src/android_drawer_tests.rs` (in `android_02_tests::drawer`) walks it on
the Retroid screens. [The Android guide](ANDROID-MVP.md#android-apps) has the visibility choice.

Phone formats: `src/android_phones_tests.rs` (in `android_02_tests::phones`) paints every Android page at 8 phone,
foldable and tablet screens (1080x2400 at 420 dpi, 1440x3200 at 560, 720x1600, a fold open and closed, two tablets), upright
and turned, at font sizes 1.0 and 1.3, in English, German and Spanish, inside the safe area a camera hole and the gesture bar
leave (`Ui::canvas_for` is the launcher's own canvas rule, so the test and the window agree); it also measures the touch pad
over the picture and the pause sheet. `android_phone_format_sweep` and `android_phone_pad_sweep` (`--ignored`) list every fault
and write the pictures (`PHONES_SHOTS`, `PHONES_MATCH`, `PHONES_ONLY`). The insets were measured on an Android 15 emulator
with gesture navigation (`SafeInsets.java`): launcher 8 dp at the sides, 24 dp top and 32 dp bottom, a camera hole 52 dp
on its edge; the player's back-gesture zones 30 dp each side.

Android Sega disc support: `document_library::player_bios` discovers firmware in granted roots;
`android/java/xyz/pocketui/app/BiosFiles.java` validates and installs private copies.
`platform/android_hw_render.rs` owns GLES 3.x negotiation and the isolated context.
`src/android_gles_tests.rs` exercises negotiation, disc discovery and the Android touch control policy;
`scripts/android/test/BiosFilesTest.java` checks failed-copy preservation and exact BIOS lengths.
Pinned core builds and source/licence evidence are described in [the Android guide](../android/README.md).
`core_options::core_id` normalises the Android library prefix for curated settings. The shared
`Core::load` initialises all four Flycast Maple ports before running a frame; regressions for the
VMU default and connected controller are covered by `cargo test --profile gate --lib android_flycast`.
`core_options::android_curation` adds the measured Android timing defaults and pinned Flycast option
names without changing Linux's existing profiles. `Audio::set_source_timing` updates resampling and queue cadence together; the Android player
uses the Activity's display refresh rate. `cargo test --profile gate --lib android_sega` covers real
Flycast option names, the single-clock Saturn policy and a late 30-Hz audio delivery without lost PCM.
The same `android_gles` tests also exercise the actual Android `HwContext::capture` on headless
EGL: a core's `READ_BUFFER=NONE` or a different colour attachment must not turn a valid scene
black, and its selection must survive the frontend's readback. Set `POCKET_REQUIRE_GL=1` to
require a real GLES driver instead of allowing the usual explicit skip when none is available.

`POCKET_REQUIRE_GL=1 cargo test --profile gate --lib android_video_tests` runs the production
Android compositor (`android/src/player_graphics.rs`) and picture retention (`player_video.rs`)
on headless EGL. The `android_gles_tests` also run the actual touch painter in
`android/src/player_touch.rs` against its visible controls and hit areas. `tests/fixtures/video_contract.c` delivers its first picture during bootstrap,
then NULL duplicates; input changes its pixel format and padded pitch or stops callbacks.
The tests assert visible colours, retained images, post-resume precedence, cached overlays
and a cancelled CRT transition.

The Android source-archive regression runs without an NDK: `python3 scripts/android/test-core-source.py`.
It verifies corresponding source after upstream configuration moves a submodule header, and that a failed
archive write cannot become a cached release source.
