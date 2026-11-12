# Android launcher and pocket-core

The native ARM64 APK uses the shared pocket-ui library. The launcher and the private `:player` Activity have
separate native libraries; the player does not link winit, and its licence graph is checked independently. It has six launcher looks, search, favourites, Curator
facets, game boxes and an EGL/GLES 3 player using shared `libretro::Core` and the shared audio pipeline with AAudio.
mGBA (GB/GBC/GBA), Nestopia (NES), bsnes (SNES), Gearsystem (SMS/GG), ClownMDEmu (Mega Drive cartridges), Beetle WonderSwan, Stella (2600), optional Gambatte (GB/GBC), Beetle PCE Fast (PC Engine), Beetle NGP (NGP/Color), Handy (Lynx), PCSX ReARMed (PlayStation) and YabaSanshiro (Saturn) are built and bundled. PPSSPP (PSP), melonDS DS, Mupen64Plus-Next (N64) and Flycast (Dreamcast) are optional downloads. Saturn and Dreamcast use the in-app GLES player. The production touch-pad painter shares the controls-card table: Saturn gets A/B/C/X/Y/Z and L/R, Dreamcast gets A/B/X/Y and its trigger bits; neither offers Select. N64, PSP, DS, Saturn and Dreamcast are **experimental**: their system line and Controls page say so; real-game frame rate, sound, save/resume and DS touch are not release-qualified. Other large systems use version-sensitive external intents.
This is an incremental milestone, not feature parity with the complete Linux App or the full requested core set.

Game-start QA must distinguish attract movies, titles and actual menus. Sega Rally Championship and Virtua Tennis 2
can consume the first Start by returning to the title, then need another before attract resumes. The bounded
[`title_entry.py` probe](../scripts/android/title_entry.py) records both steps; `--pause-after` holds the resulting menu
for inspection. See [the QA procedure](../docs/ANDROID-MVP.md#saturn-and-dreamcast-internal-player). This changes QA tooling,
not the player's controller, VMU, polling or core-option policy.

Build prerequisites: Rust/cargo-ndk 4.1.2 and aarch64-linux-android, JDK 17, SDK/build-tools 35.0.0, NDK 27.2.12479018,
CMake and Ninja. On the Apple Silicon build host, from the checkout root:

```sh
scripts/android/build-apk.sh
scripts/android/start-emulator.sh >android/build/emulator.log 2>&1 &
# Wait for emulator-5580: sys.boot_completed=1.
python3 scripts/android/player-smoke.py "$HOME/Library/Android/sdk/platform-tools/adb" emulator-5580 android/build/evidence/player
```

`build-cores.sh` fetches immutable source revisions, builds ARM64 API 26 and emits (with `ANDROID_RELEASE_SOURCES=1`) the corresponding-source archive,
licence and binary hash manifest. See [CORE-LICENCES.md](CORE-LICENCES.md). `build-apk.sh` packages resources, code,
core and licences with cargo-ndk/javac/d8/aapt/zipalign/apksigner. No Gradle or hosted CI is required. Outputs:
`android/build/pocket-android-mvp.apk`, its SHA-256 and `android/build/cores/corresponding-source/`. Ship the source
archive alongside an APK. A public preview is built with `scripts/android/release-apk.sh` and checked with `release-check.sh`: see
[Public preview release](../docs/ANDROID-MVP.md#public-preview-release). Lockfiles and source revisions are pinned; build timestamps/ZIP times still vary.

The release keystore defaults to `~/.local/share/pocket-ui/android-signing/release.jks`, with its private password file
beside it (override `ANDROID_SIGNING_DIR`). Both stay outside the repository, permissions 0600, directory 0700.
`build-apk.sh` stops at once, with a message, when that key is missing (a new key would be another app to Android); a QA or
test package brings its own key in `ANDROID_SIGNING_DIR`, and `ANDROID_ALLOW_NEW_KEY=1` makes a throwaway key on purpose.
The initial debug-signed spike must be uninstalled once; later versions signed with this release key update in place.
The package is `xyz.pocketui.app`; QA builds up to 0.3.1-android.1 were `dev.pocket.spike`, which Android keeps as a
separate app ([package name](../docs/ANDROID-MVP.md#package-name)).
Do not delete this identity. The APK is non-debuggable, version 0.2.0-android.1 / 20001.

Only isolated emulator serials with `ro.kernel.qemu=1` are accepted by smoke scripts. AVD name, port, display width,
height and density can be set through `ANDROID_AVD_NAME`, `ANDROID_EMULATOR_PORT`, `ANDROID_DISPLAY_WIDTH`,
`ANDROID_DISPLAY_HEIGHT`, `ANDROID_DISPLAY_DENSITY`. Defaults retain the dedicated 1280×720 / 240-dpi AVD on 5580.
For the X300-sized test, use a separate name/port, 1216×2640 and 480 dpi (assumed test density); rotate it with Android
settings and record both orientations. `-no-audio` keeps the host silent. Stop only your own emulator.

The earlier RetroArch fallback fixture remains separate: `prepare-retroarch-emulator.sh` requires an official APK and
ARM64 Gearsystem core, uses root only in the emulator and never bundles RetroArch. It is not a handheld installer.
Architecture and honest limits: [ANDROID-MVP.md](../docs/ANDROID-MVP.md). Controls: [Android preview](../docs/user/android-mvp.md).

The small bundle is pinned in `cores.lock.json`: mGBA, Nestopia, bsnes, Gearsystem, ClownMDEmu, Beetle WonderSwan, Stella, Gambatte, Beetle PCE Fast, Beetle NGP and Handy. `build-cores.sh` delegates to
`build-cores.py`, uses the NDK compilers for C/C++ cores and emits stripped ARM64 libraries, source archives and licence
file/header notices. It rejects host binaries and unbundled shared C++ runtimes. The APK copies only cores listed in
the generated manifest, verifies hashes, and enforces the 30 MB size budget. `classic-smoke.py` checks pinned starter
homebrew plus original SMS/GG/WonderSwan/2600 fixtures, manual slots, Earlier and warm resume on an emulator only.

`launcher-smoke.py` captures the five looks and shared launcher/player menus in both appearances and EN/DE, using
live accessibility bounds. The release smoke does not require a debuggable APK. `smoke-remote.sh` requires a clean,
committed source tree and exports the own sources (the cleaned, scanned tree of `scripts/source-archive.sh`, not a raw `git archive`) and the core sources alongside the APK; downloaded ROMs are not exported.

Core builds reject an NDK revision that differs from the lock and tracked local edits in a pinned core checkout.

The player keeps the initial timing/geometry run's picture. A newer picture after restoring a moment takes precedence; NULL duplicates or runs without a video callback retain the last picture. Native RGB565, 0RGB1555 and XRGB8888 conversions respect padded row pitch before the GLES upload. The Linux `android_video_tests` exercise this through the production compositor.

The player initially hides touch controls when Android reports a gamepad. Gamepad key/stick input keeps them hidden and uses the full safe viewport; a touch reveals them again. Pause/back remains available in either input mode.

The APK keeps the small cores through PS1, plus Saturn, under a 30 MB budget. PSP, DS, N64 and Dreamcast are optional store packages; their exact sizes are generated from the built archives. A like-for-like size comparison uses the unchanged 15-core APK and the full 17-core test bundle with identical signing settings. GLES cores get an
isolated EGL context and framebuffer; a native-size readback feeds the existing screenshots/pause path.
PSP has four face buttons and shoulders. N64 adds Z/C controls and maps touch arrows to its analog stick;
DS playfield touches reach the core pointer interface. Pause/resume restores the Activity EGL context.
Only quick starts are required for this development milestone:

`ANDROID_CORE_CASES=psp,nds,n64 python3 scripts/android/core-smoke.py "$ANDROID_HOME/platform-tools/adb" emulator-5580 android/build/quick-heavy`

The script downloads author-hosted homebrew fixtures by pinned checksum and captures one playing/menu
pair per core. It does not produce release bundles or PCM evidence. See CORE-LICENCES.md for the remaining
public-release licence work. Set `ANDROID_RELEASE_SOURCES=1` only when preparing that public release.

Gamepad face buttons follow libretro physical positions (Android south/east/west/north → B/A/Y/X); the pause menu retains its A/B actions.
One table turns Android's pad keys into buttons for both Activities (`pocket_ui::pad::android`): L2/R2/L3/R3 included.
Analog triggers (LTRIGGER/RTRIGGER or BRAKE/GAS) count as pressed past 0.75 and let go under 0.5, in the player
(`nativeAnalog`) and in the launcher (the vendored winit patch sends them as BUTTON_L2/R2). The launcher also has a
HOME intent filter, so Android offers it as the home screen; `singleTask` keeps one instance. Home or the app icon
during a game finishes the player after its pause save (the library opens, the game first under Continue); Recents
returns to the paused game.

The 0.1.5 refresh uses the current shared real-media/box/info/sticker/TV painters and save-medium pause row; see [ANDROID-MVP.md](../docs/ANDROID-MVP.md#main-refresh-2026-10-30) for the Android parity boundary. Internal builds skip release proof bundles.


First start offers a resumable eight-step guide in English/German: welcome,
language, ROM folders, look/animation/appearance, download consent, extra systems, controls and ready. Each choice
persists immediately; Back/Continue/Skip work with touch and a controller, and Settings → First
start reopens it. Multiple SAF roots keep their persisted read grants; scans run on a worker and
combine readable roots without changing provider document identities. No full Linux renderer or
settings parity is implied.

Opt-in browse-time cover/screenshot/English Wikipedia loading uses the existing shared sources and
requires Android's current unmetered Wi-Fi capability. Settings retains the choice; cached content
works offline. No batch downloader is enabled. Empty folders can be replaced/augmented or skipped.

Fresh-install onboarding QA, after the core checks (clears this emulator app's private data):

```sh
python3 scripts/android/onboarding-smoke.py "$ANDROID_HOME/platform-tools/adb" emulator-5582 android/build/onboarding
```

The test verifies its emulator target, step-2/3 warm restart, empty and populated real SAF roots,
English portrait/German landscape and all eight step screenshots. Add `--all-combinations` for both
languages in both orientations. UI taps come from live accessibility bounds; inspect the screenshots.

The .3 preview uses the selected cartridge logo as its adaptive/round icon, Android 13 monochrome layer and system splash icon. The supplied 432 px layers are copied unchanged into mipmap-xxxhdpi; the adaptive tile uses #3A2A1F.

Version 0.1.5-android.4 is the phone design pass: bottom tabs, scrolling shelves, dp-sized type and touch targets,
controller hints only after controller input, a touch-first pause menu, and a first start with a step indicator
and full-width buttons. Capture its screens (keeps app data, changes look/language/favourites):

```sh
python3 scripts/android/phone-design-smoke.py "$ANDROID_HOME/platform-tools/adb" emulator-5582 android/build/phone --lang de --orientation landscape
python3 scripts/android/phone-design-smoke.py "$ANDROID_HOME/platform-tools/adb" emulator-5582 android/build/phone-empty --empty
python3 scripts/android/phone-design-smoke.py "$ANDROID_HOME/platform-tools/adb" emulator-5582 android/build/phone-shelves --parity
```

Settings → Game check, from a fresh install through the real picker (clears the app's data; `--phase lost` needs a Google APIs image):

```sh
python3 scripts/android/game-check-smoke.py "$ANDROID_HOME/platform-tools/adb" emulator-5598 android/build/game-check ROMS --lang de --orientation landscape
```

The pinned PPSSPP core applies `patches/ppsspp-pause-drain.patch`: pausing drains graphics
commands until the emulation thread acknowledges the pause, rather than consuming only one
frame and polling. `cargo test --test ppsspp_pause` exercises the old and patched pause fragments
with a two-frame readback stub. Rebuild the cores to include it; it does not affect an installed
ROCKNIX core. Actual GLES/device validation remains necessary.

## Android 0.2 preview

The game card and the pause sheet offer **Controls**: the shared controls card
(`controls_card::paint`), the console's button names in the places of the gamepad's buttons. A first start on each console opens this card;
Back or A goes on, while an existing moment continues directly. **Zoom** in the pause sheet
holds the frame still: pinch from the paused picture or on Zoom, drag to pan, or use the
D-pad and A (2x/3x). Back returns to the pause sheet. It never changes the game's picture settings.

Options → Controls → **Hold to toggle** chooses one console button for this game, or Off.
One press holds it, the next lets it go. A badge names the held button. Pause, background,
controller changes and loading a state release it. The gamepad's analog stick remains analog
on analog consoles; its hat remains the D-pad.

**Text size** and Android's font setting use the larger size, including changes while the
app is open (system scale up to 200%). The two choices are not multiplied. **Haptics** is
off on a fresh install and gates touch feedback in both Rust and Java; Android's own haptic
setting also applies. Core/controller rumble remains unavailable.

After more than two weeks, a successful internal resume can show **Where was I?** over
its restored frame until a press or touch ends it. That press only dismisses the card. Easy omits total play time;
Settings → **Remind me where I was** switches it off. External emulator apps own their resumes.

Android Back opens the pause sheet during play and returns one page from Options, Controls,
Zoom or Recovery. **Save & quit** is the normal menu exit. If a Load and its rollback both fail, **Return to library** closes the protected session without saving it. Background/audio-focus loss
pauses and commits the moment; returning stays paused. Gamepad disconnection clears inputs,
pauses and restores touch controls. Both Activities use safe areas for cutouts, system bars
and gesture edges. Battery saver suppresses optional media downloads and the
player's startup animation without changing saved preferences or slowing the game.

An old short PSP moment is refused by the shared size guard. The **Recovery** page explains
that it could not load safely and where it is kept; Load still lists the preserved moment.
Recovery uses the exact raw or compressed state selected by the core, with its matching preview, even when an older
crash point from another app version is still present. A later automatic save cannot remove that retained moment.
It does not relax the pinned PPSSPP parser's safety checks.

Host regressions live in `src/android_02_tests.rs` (the Android controller/painters themselves):
`cargo test --profile gate --lib android_02_tests`. `scripts/check-platforms.sh android` checks
both native libraries. `scripts/android/02-smoke.py` drives a disposable AVD through actual SAF,
Back, the new pages in both orientations/appearances at 100%/200% fonts, pinch, background
save, a uinput gamepad, Hold to toggle, reminders and battery saver. Its helpers in
`scripts/android/test/` are shell-only probes, never packaged in the APK. Build `Gestures.java`
with the SDK and d8 to `/data/local/tmp/classes.dex`, and `gamepad.c` with the NDK to
`/data/local/tmp/qa-gamepad`, before running the script. Inspect the saved screenshots;
emulator checks cannot establish physical touch comfort, vibration, battery cost or driver quality.

## Android 0.3 internal QA

Settings calls the font choice **Style** and includes Retro, with its own saved Cocoa/Pea colours in launcher and
player. Selected game names use the shared three-line title/subtitle layout in every phone look; their reserved
band keeps system text and Play in place. The current main's shared disc/cart launch scenes and retained-moment
reader are used without a separate Android implementation.

SAF scans recognise the same foreign folder aliases as the card scanner. Artwork in `Imgs`, `.media`, `covers`,
`images`, `media/*`, listed gamelists and ES-DE `downloaded_media` is tried before cached downloads. Pictures stay
inside the granted roots, are decoded on the media worker (at most 8 MiB input, 1024 px output), and never write
back to the card. Optional XML/CHT is limited to 256 KiB per file and 8 MiB per root; unavailable/oversized optional
files do not hide the games. The listing never guesses access outside a selected tree.

A same-stem `.cht` beside a game adds **Cheats** to pause → Options for mGBA, Gambatte, bsnes and Nestopia.
The shared parser and core API apply code cheats; memory-write formats are explained, never pretended to work.
Choices start Off, persist by provider/game/core identity, and a failed setting write changes neither switch nor
core. Other bundled cores and external apps have no Android Cheats page in this preview. No cheat database is
downloaded or bundled. Nestopia's bounded per-call buffer gets split code groups just as Snes9x does.

Load saves Before load before entering the core. A refused partially applied load rolls back; a failed rollback
blocks play, automatic/manual saves and SRAM/RTC writes through core destruction. The player can load another
moment or explicitly return to the library, preserving earlier files. A successful state write remains successful
if only its optional thumbnail fails. Android **Updates** checks its signed Android APK channel and installs
through Android; Linux/SD update packages are not offered. `ANDROID_QA_VERSION=0.3.1` gives this internal APK its independent version while the main
source crate still carries the development version. Use the existing signing key to keep app data on updates.

`02-smoke.py` accepts `ANDROID_SMOKE_STYLE=retro`, `ANDROID_SMOKE_JPEG=1` (Pillow) and `ANDROID02_MATRIX_ONLY=1`.
JPEG evidence is at most 150 KB per picture. The full journey also checks that reminders wait for a press.

N64, PSP, DS, Saturn and Dreamcast use the shared 48 ms heavy-core audio queue on first open and after audio focus returns; the AAudio callback and endpoint buffer retain their low-latency policy. N64 and Dreamcast request no extra cold-start reserve. N64's former 352 ms target remained above 380 ms during continuous music after 20 seconds, so startup/load gaps remain a known limit. The Saturn candidate requests only 132 ms: Bomberman and Sega Rally return to normal latency by 20 seconds on Odin. After 15 seconds of delivered PCM it removes stereo intervals at least 50 ms long below -60 dBFS, retaining 5 ms guards and a 5 ms crossfade; a one-cent fallback never cuts sounding music. Live diagnostics record removed peak, splice step and return frame. Continue and pause do not reapply the reserve. Qualification of the full game set and post-Continue sound is still required.

DS stylus events pass through `platform/android_pointer.rs`: a complete tap between emulation
ticks reaches one tick, while pause, input-mode changes and cancellation discard pending touches.

Android periodic Earlier captures keep `retro_serialize` on the core/EGL thread and log its
cost separately from size query/buffer allocation (`save_state_with_serialize_time`); the capture log reports both.
`platform/android_earlier.rs` moves the owned state and an `Arc` of the immutable
frame to one worker: pixel copying, PNG encoding and durable state/preview writes run there.
At most one capture is active and one newer capture waits; a newer waiting capture replaces the old
one. Quit finishes an active write and discards pending Earlier work (the newer Continue state is
saved separately). A worker failure preserves the previous state file and is logged; there is no synchronous
fallback in play. The Linux player already uses `timeline::Writer` and the serialize cost guard,
which defers expensive captures to a quiet pause.

The pinned PPSSPP system assets (compatibility tables, fonts and VFPU tables) are included in its optional
core package under `system/PPSSPP`. `core.json` records every asset's hash and size. The player selects that
installed system folder before loading the core. Older APK-bundled PSP assets retain their existing fallback.
See [Android core packages](../docs/STORE.md#android-core-packages) for catalog generation and local testing.

Settings → Updates reads the separate signed Android channel, downloads over Wi-Fi and hands a
verified APK to Android's PackageInstaller session API. The first install asks for unknown-app
permission; Android confirms each update and closes the app without clearing data. Open it again;
the first opening confirms the installed version.
See [Android channel preparation](../docs/UPDATE-CHANNEL.md#android-apk-channel) for the local-only
release-check options and isolated QA channels. Nothing is published by these scripts.
Single-file SAF games now get a capability-checked direct path: `GameDirect.java` verifies descriptor reopening/seeking,
then `:player` opens the URI itself and `platform/android_content.rs` owns its original-name `/proc/self/fd` alias until
core destruction. Large documents retain their existing CRC fingerprint through tiny cached metadata; successful direct
starts remove the obsolete game copy. Providers without seekable/reopenable files, disc sets/parent-dependent CHD and unchecked cores retain
`GameCopy`. No saves move. See [Direct documents](../docs/ANDROID-MVP.md#direct-documents) for lifetime, cache and fallback rules.

Selecting a system folder directly retains its provider display name as `Document.root_name`. The shared alias resolver
uses it after nearer subfolders, so a directly chosen PSP folder can list its CHDs. Game check uses the same context;
relative set/artwork paths and provider identity stay stable. An empty library explains the parent/system-folder choices.


## Saturn and Dreamcast

Both use the existing shared disc insertion scenes, private player, pause menu, manual slots and Continue.
The pinned Flycast GLES3/AArch64 core and ROCKNIX's YabaSanshiro GLES3.1/AArch64 core are built by
`build-cores.py`, with 16 KB ELF alignment, licence notices and corresponding source. The lock lists only
the submodules these libretro builds need; standalone UI, Vulkan and audio backends are disabled.
Yaba's Android patch creates each state stream in the private save directory and immediately
unlinks it. Android's default `tmpfile()` directory is inaccessible to apps. Failed state operations
return an error, and the libretro wrapper checks the actual size before copying. Run
`python3 scripts/android/test-yaba-state.py android/build/cores/source/yabasanshiro` after building
to exercise the patched core functions with unavailable default temporary storage.
The sound-state patch also saves the Musashi sound CPU's registers, IRQ, STOP and prefetch values, excluding host pointers. New SCSP version4 states include these 240 bytes; legacy states without the sound CPU remain readable but cannot recover CPU values never saved. The YSS reader validates the saved file header and declared length, allowing state size to change with video geometry; it does not compare a 240-line saved game with the fresh 224-line boot size. Run `python3 scripts/android/test-yaba-sound-state.py android/build/cores/source/yabasanshiro` to check the actual methods across a fresh host context while retaining its callback/table pointers. Hardware confirmation of game sound after Continue remains separate from this regression.

The application explicitly opts out of Android native heap pointer tagging. Flycast's fast-memory
fault handling compares kernel fault addresses with heap RAM addresses; tagged pointers broke that
comparison on the Odin (SIGSEGV in `addrspace::write32`). This applies to the launcher and every
private player core, and gives up Android's allocator top-byte consistency check for this app.
It does not repair the core's address assumptions: Android documents this compatibility switch as
temporary, with future MTE compatibility still requiring core changes. See
[Android's tagged-pointer guidance](https://source.android.com/docs/security/test/tagged-pointers).
The release check inspects the compiled application attribute, rather than only the source XML.
Compare other cores and process memory on hardware when changing this policy.

The shared loader connects Flycast's four Maple controller ports before its first frame; the
standard controller also carries the VMU. Android's `lib<core>_libretro.so` filenames resolve to
the shared core IDs. Android extends the common profiles with `VMU A1` for the pinned core's
historical `reicast_*` option names and older `flycast_*` aliases; Linux keeps its existing profile.
Android disables Yaba's auto-frameskip/busy-wait limiter because the frontend already clocks core
ticks and presentation. Flycast's frame-rate detection is enabled so locked 30/20-Hz games report
their real cadence instead of producing PCM at twice speed. For a game with an unlocked frame
rate, an explicit disabled choice under Options wins over this default.
The audio queue receives the source frame timing on open, AV changes and focus recovery; a
30-Hz source gets at least two source frames of reserve without changing its audio sample rate.

Player diagnostics count emulation ticks (`frames`), fresh core pictures (`video_frames`) and
incoming/resampled PCM frames. Compare these with SurfaceFlinger presentation timings: a core
running at 60 ticks need not supply 60 pictures. Measure both before attributing a low visible
rate to the game or the frontend.
GLES readback explicitly selects the host framebuffer's colour attachment, then restores the
core's read-buffer selection. A scene can leave `READ_BUFFER=NONE` or another attachment even
when the colour image is valid. `read_buffer_overrides` in the player log counts these cases;
the real headless-EGL regression covers both selections and the ordinary colour-buffer path.
The Odin homebrew checks on 11 November 2026 cover Disasteroids and Sweeper Squad on Saturn,
and Xump on Dreamcast, including pause, manual save/load and Continue in a new player process.
Sweeper supplied about 60 fresh pictures/s and Xump about 29.9 while the display presented near
60 Hz. The 180-second windows added no measured audio underruns or dropped PCM; startup still
had underruns/drops. Xump's window includes level play, failed attempts and retry screens, rather
than three uninterrupted minutes in one level. Sweeper explicitly stops CD music on entering
its board; silent PCM there is not evidence of a lost music track after Continue. A fresh
post-Continue Disasteroids check also passed: a new player process supplied a separate 25-second
active-game PCM clip with nonzero sound (peak 8905), and its 35-second measurement added no audio
underruns or dropped PCM. The cold-start recording was excluded from that proof.
The read-buffer override counter stayed zero in these runs: the headless regression proves a
readback-state defect, but does not establish it as the cause of earlier black Odin captures.
Both Sega systems remain experimental.

Diagnostic audio keeps its 30-second default. A diagnostic launch can request
`--ei diagnosticAudioSeconds 240` (clamped to 1–240 seconds) to include startup and a full 180-second
game window. Only `diagnosticAudio=true` allocates or writes that bounded PCM capture; longer
captures affect diagnostic-process memory and are not comparable with the earlier 30-second PSS samples.
The diagnostic launch also logs the retained frame size and nonzero RGB-pixel count every ten seconds, to distinguish a black core readback from a composition failure. It also logs actual input callback reads/button bits and the longest continuous missing-PCM burst in startup versus later play. These are observations, not a frame-rate or latency qualification.

SAF selects BIOS by name and length across the chosen roots. `BiosFiles.java` checks the stream again
and replaces the private copy atomically; failed replacements keep the previous BIOS. Dreamcast uses
`files/bios/dc/dc_boot.bin` (2 MiB) and optional `dc_flash.bin` (128 KiB); `dc_bios.bin` is accepted as
an alias for boot. Touch arrows also steer the Dreamcast left stick; triggers retain their own bits. Saturn accepts `saturn_bios.bin`, `sega_101.bin` or `mpr-17933.bin` (512 KiB).
Saturn refuses to start without one and explains how to add it. Dreamcast explains its HLE fallback.
No BIOS or games ship in the APK; grants give read access, and all writes stay private.

Saturn supports CUE/BIN, ISO, CHD and M3U; Dreamcast GDI, CUE/BIN, CDI, CHD and M3U. CUE/GDI/M3U
references are staged together by the existing `GameCopy` path. Both cores open files themselves, so
large discs are not limited by the cartridge memory ceiling. They remain outside the direct SAF-FD
allowlist until that path has separate core evidence.

Host checks: `cargo test --profile gate --lib android_gles_tests`, the document-library tests and
`android_02_tests::unverified_android_systems_are_labelled_before_play_and_continue`. Run the pure
JDK `scripts/android/test/BiosFilesTest.java` with `BiosFiles.java` for copy-size and preservation checks.
Odin checks need an actual playing scene, 180 seconds of image/PCM statistics, pause, slot save/load
and Continue after restarting the launcher; muted runs do not qualify audible speaker quality.
