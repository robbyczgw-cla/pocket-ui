# Android launcher and pocket-core

The native ARM64 APK uses the shared pocket-ui library. The launcher and the private `:player` Activity have
separate native libraries; the player does not link winit, and its licence graph is checked independently. It has six launcher looks, search, favourites, Curator
facets, game boxes and an EGL/GLES 3 player using shared `libretro::Core` and the shared audio pipeline with AAudio.
mGBA (GB/GBC/GBA), Nestopia (NES), bsnes (SNES) Gearsystem (SMS/GG), ClownMDEmu (Mega Drive cartridges), Beetle WonderSwan, Stella (2600), optional Gambatte (GB/GBC), Beetle PCE Fast (PC Engine), Beetle NGP (NGP/Color) and Handy (Lynx) are built and bundled; PPSSPP (PSP), melonDS DS and Mupen64Plus-Next (N64) are included in the internal test build. Other large systems use version-sensitive external intents.
This is an incremental milestone, not feature parity with the complete Linux App or the full requested core set.

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
archive alongside an APK. Lockfiles and source revisions are pinned; build timestamps/ZIP times still vary.

The release keystore defaults to `~/.local/share/pocket-ui/android-signing/release.jks`, with its private password file
beside it (override `ANDROID_SIGNING_DIR`). Both stay outside the repository, permissions 0600, directory 0700.
The initial debug-signed spike must be uninstalled once; later versions signed with this release key update in place.
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
the generated manifest, verifies hashes, and enforces the 100 MB size budget. `classic-smoke.py` checks pinned starter
homebrew plus original SMS/GG/WonderSwan/2600 fixtures, manual slots, Earlier and warm resume on an emulator only.

`launcher-smoke.py` captures the five looks and shared launcher/player menus in both appearances and EN/DE, using
live accessibility bounds. The release smoke does not require a debuggable APK. `smoke-remote.sh` requires a clean,
committed source tree and exports the own sources (the cleaned, scanned tree of `scripts/source-archive.sh`, not a raw `git archive`) and the core sources alongside the APK; downloaded ROMs are not exported.

Core builds reject an NDK revision that differs from the lock and tracked local edits in a pinned core checkout.

The player initially hides touch controls when Android reports a gamepad. Gamepad key/stick input keeps them hidden and uses the full safe viewport; a touch reveals them again. Pause/back remains available in either input mode.

The current 14-core test APK is about 19 MB. Heavy-core downloads are deferred. GLES cores get an
isolated EGL context and framebuffer; a native-size readback feeds the existing screenshots/pause path.
PSP has four face buttons and shoulders. N64 adds Z/C controls and maps touch arrows to its analog stick;
DS playfield touches reach the core pointer interface. Pause/resume restores the Activity EGL context.
Only quick starts are required for this development milestone:

`ANDROID_CORE_CASES=psp,nds,n64 python3 scripts/android/core-smoke.py "$ANDROID_HOME/platform-tools/adb" emulator-5580 android/build/quick-heavy`

The script downloads author-hosted homebrew fixtures by pinned checksum and captures one playing/menu
pair per core. It does not produce release bundles or PCM evidence. See CORE-LICENCES.md for the remaining
public-release licence work. Set `ANDROID_RELEASE_SOURCES=1` only when preparing that public release.

Gamepad face buttons follow libretro physical positions (Android south/east/west/north → B/A/Y/X); the pause menu retains its A/B actions.

The 0.1.5 refresh uses the current shared real-media/box/info/sticker/TV painters and save-medium pause row; see [ANDROID-MVP.md](../docs/ANDROID-MVP.md#main-refresh-2026-10-30) for the Android parity boundary. Internal builds skip release proof bundles.


Version 0.1.5-android.2 adds resumable seven-step first start in English/German: welcome,
language, ROM folders, look/animation/appearance, download consent, controls and ready. Each choice
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
English portrait/German landscape and all seven step screenshots. Add `--all-combinations` for both
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

The game card and the pause sheet offer **Controls**, with Android gamepad labels and the
buttons actually drawn on the touch pad. A first start on each console opens this card;
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
and gesture edges. Battery saver suppresses optional slideshow/media downloads and the
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
if only its optional thumbnail fails. Android **Updates** explains APK installation; it does not poll or install
Linux/SD update packages. `ANDROID_QA_VERSION=0.3.1` gives this internal APK its independent version while the main
source crate still carries the development version. Use the existing signing key to keep app data on updates.

`02-smoke.py` accepts `ANDROID_SMOKE_STYLE=retro`, `ANDROID_SMOKE_JPEG=1` (Pillow) and `ANDROID02_MATRIX_ONLY=1`.
JPEG evidence is at most 150 KB per picture. The full journey also checks that reminders wait for a press.
