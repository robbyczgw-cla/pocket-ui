# Android and platform adapters

*Part of [DEVELOPING.md](../DEVELOPING.md). Read this when you work on the Android app or the platform adapters.*

## Android feasibility branch (2026-10-28)

`android/` is an independent `cdylib` package using winit/android-activity and softbuffer. It includes selected existing
source modules by path, so it never compiles or launches the ROCKNIX app/services. `src/text_size.rs` contains the
platform-independent TextSize value (re-exported by settings); Android supplies its locale over JNI, not system.cfg.
Build, emulator setup, scope and controls: [android/README.md](../../android/README.md),
[user guide](../user/android-spike.md).
This is a feasibility experiment, not a supported release target.

## Public native preview text

The feature-preview help is linked into the handheld Android library too. The public export rewrites its sample
Tailscale address and DNS name to documentation values; source-only demo allowances do not apply to native libraries.
`scripts/test-export-public.sh` scrubs the actual preview and scans its NUL-separated native text without allowances.

## Platform adapters

`src/platform/mod.rs` selects OS adapters at I/O boundaries. `platform/linux_http.rs` owns the existing curl
transport for shared covers, pictures and ROM identification, including compressed-listing fallback. Network work
stays on the existing workers; this move changes neither curl arguments nor cache/persistence behaviour.

## Android launcher and in-app player

`src/lib.rs` selects `linux_modules.rs` or `android_modules.rs`. The Android entry depends on the normal shared
library; `platform/` supplies Android SAF/JNI, HTTPS/system queries and AAudio. `library_names.rs` and
`look_settings.rs` hold shared pure rules/values, re-exported by Linux. `audio.rs` keeps the shared queue/rate control; `platform/` selects the output sink. Android uses an AAudio data callback with the bounded atomic stereo handoff in
`platform/android_pcm.rs`; its host tests cover missing tails, restart fades, stereo order and concurrent counter wrap. The callback follows the [AAudio real-time restrictions](https://developer.android.com/ndk/guides/audio/aaudio/aaudio). The EGL Android player uses shared `libretro::Core` and save logic. The complete composition, supported subset,
intent table and build/emulator scripts are documented in [ANDROID-MVP.md](../ANDROID-MVP.md) and
[the Android build guide](../../android/README.md). The older feasibility section above records the original spike.
N64, PSP and DS remain experimental: `android/src/play_cards.rs` supplies the translated status
for phone shelves, game cards and Controls; `src/android_02_tests.rs` covers all phone looks and Continue.

The hardware cores render into a colour texture in a shared EGL context. The player draws that texture with the
same Picture shader as software cores, respecting the core's origin and RGB order. GLES fences confirm GPU writes and reads across the two contexts. A GPU copy retains the last delivered picture
even if a core writes into its framebuffer and then duplicates. If the driver cannot allocate the shared picture
or use the fence API on an otherwise working context, the session falls back to CPU capture. Mipmapped effects use a GPU copy sized to the visible area and leave the core framebuffer untouched. The
player reads pixels for CPU consumers such as pause, moment previews and screenshots. The log's
`hw_cpu_captures` counter distinguishes those reads from normal play. The player applies its CPU affinity before PPSSPP's bootstrap run so its emulation thread inherits it during boot or Continue.
A replacement window can use the same EGL share group. If the driver replaces the context itself, the
dedicated player process closes and the shelf can reopen the last saved moment.

`platform/android_audio_timing.rs` checks presentation estimates before scheduled UI sounds use
them. `AudioRoute.java` confirms the stream's local output by device ID; contradictory timestamps
on local Fast routes fall back to Written−Read, while raw values remain in diagnostics. The
microphone/frame comparison and regression commands are in the Android MVP guide.

Android portable rendering: `src/cabinet_tv_art.rs` holds the shared TV body/glass painter (Linux worker reexports it); `game_card_model` and `play_time` also provide the portable problem label and relative-time text used by box/info pages. Android target composition selects these painters without Linux services. See [ANDROID-MVP.md](../ANDROID-MVP.md#main-refresh-2026-10-30).

The private Brick build keeps its extractor and card installer in `src/untar/brick.rs` and
`src/update/brick.rs`, compiled only with `--features brick`. Standard embedded assets remain
in their original locations; Brick translations and manual variants are separate. See
[BRICK-CFG.md](../BRICK-CFG.md) for the reference comparison and outstanding verification.

### Android 0.1.14 refresh modules

`src/state_policy.rs` is the portable raw-state size exception list, used by
`hostsys::core_states_vary` and `android/src/player.rs`, through `Core::apply_state_policy`. PPSSPP is not in the
shorter-as-is list (it ignores the length it is given); it is in the padded list (`core_pads_short_states`), because its
`retro_serialize_size` is 128 MiB until its renderer is up (Android resumes before that) and later an 8 MiB-rounded
measure, while the state written at quit is 40 MiB.
`android/src/phone.rs` paints Easy and the Games Just Play action; `ui.rs` refreshes
Curator user history before the shared choice policy and launches through `Command::Launch`.
`onboarding.rs` uses separate action IDs for Animations/Appearance so adding a Look
cannot collide with those controls. See [ANDROID-MVP.md](../ANDROID-MVP.md#android-refresh-for-0114-branch-build)
for supported behavior and the remaining picture/player differences.

`scripts/android/release-apk.sh` refuses a private tree (the `scripts/check-leaks.sh` marker) before building,
creating output or accessing signing material. Build public APKs from the exported public tree, then run
`scripts/android/release-check.sh` from the private tools tree so its strict APK-member scan and reviewed
licence/contact rules are available. Embedded demo data must be scrubbed by the export too.

ClownMDEmu's pinned adapter restores CRAM on `retro_unserialize` but originally left its separate output-format
palette at the fresh process's black colours. `android/patches/clownmdemu-restore-palette.patch` rebuilds all 64 normal,
shadow and highlight colours through the selected pixel-format callback; the existing state layout stays compatible.
This is a software-core cache issue, before presentation. The Linux Mega Drive default is Genesis Plus GX; the same
ClownMDEmu library can be checked with the shared host frontend.

The Continue regression in `src/android_video_tests.rs` saves and resumes in separate processes, runs the same
bootstrap, state policy and `Core::resume` as Android, and checks the first restored frame, the GLES presentation and
Earlier's preview. The ordinary fixture covers RGB565, 0RGB1555 and XRGB8888; a same-process save/load would miss
this cache bug. To run the real-core version with a visible diagnostic or homebrew scene:

```sh
POCKETUI_TEST_CORE=/path/to/clownmdemu_libretro.so POCKETUI_TEST_ROM=/path/to/Game.bin \
  cargo test --profile gate --lib android_video_tests::a_real_core_continue_presents_a_visible_frame_and_earlier -- --ignored --exact
```

Use the same check for other software cores; a naturally black saved scene is unsuitable for this probe. The
headless GLES tests need EGL/Mesa; `POCKET_REQUIRE_GL=1` makes a missing renderer fail explicitly.

The pause-menu shot reader (`preview.rs`) accepts older one-image shots and an optional 4×1 game rectangle before the picture (the initial picture-first layout is also accepted), ignores invalid crop bounds, retries incomplete files, and bounds decoding to eight nonempty images, 4096² pixels each and 128 MiB per file. Both preview and shot reads use one open file handle so an atomic rename cannot mix requests.

A console's first start goes straight to the game (0.5: the controls card at a first start went, and a `controls-card-seen` file of older versions is ignored). `user_journeys::a_consoles_first_start_goes_straight_to_the_game_without_a_card` guards it with a stand-in game.

Android chooses the shared heavy audio reserve with `audio::needs_jitter_reserve` for N64, PSP and DS,
on initial open and after audio focus returns. The callback/endpoint policy stays separate.

DS stylus events pass through `platform/android_pointer.rs`: a complete tap between emulation
ticks reaches one tick, while pause, input-mode changes and cancellation discard pending touches.

Android periodic Earlier captures keep `retro_serialize` on the core/EGL thread and log its
cost separately. `platform/android_earlier.rs` moves the owned state and an `Arc` of the immutable
frame to one worker: pixel copying, PNG encoding and durable state/preview writes run there.
At most one capture is active and one newer capture waits; a newer waiting capture replaces the old
one. Quit finishes an active write and discards pending Earlier work (the newer Continue state is
saved separately). A worker failure preserves the previous state file and is logged; there is no synchronous
fallback in play. The Linux player already uses `timeline::Writer` and the serialize cost guard,
which defers expensive captures to a quiet pause.

## Optional systems

Since 0.5.x every core is in the APK and nothing is offered (docs/STORE.md#android-core-packages); a core downloaded with an
older APK is retired at start (`android_cores::retire_bundled`). The download path below stays for a core that may need it.
`android/src/core_downloads.rs` connects first-start selections and Extras to the shared store installer.
`CoreDownloads.java` selects an unmetered Wi-Fi network and retains partial archives; `ResumableDownload.java`
provides size-bounded HTTPS with Range continuation. Host coverage is `ResumableDownloadTest.java`,
`android_cores::tests` and the real Android controller in `android_02_tests`.
The package builder and local catalog instructions are in [STORE.md](../STORE.md#android-core-packages).

## Direct documents

`GameDirect.java` checks the current SAF document's seek/reopen capability on the content worker. The player opens its
own descriptor (Activity intents carry only URI/name), and `platform/android_content.rs::DirectGame` owns the native
file and an alias with the original suffix until after `CoreSession` drops. Its small large-document CRC cache preserves
old `GameCopy` fingerprints; no state migration is needed. `frozen_refresh` makes its own short-lived launcher alias when
there is no persistent copy. Game check uses the same probe for free-space advice. Disc sets/parent-dependent CHD and unsupported providers
keep `GameCopy`; the concrete copy reason is logged. Host tests cover suffixes, reopening/seeking, cleanup and old moments;
`GameDirectTest.java` checks routing for all pinned cores, and `core-info.c` prints their actual loader flags.
