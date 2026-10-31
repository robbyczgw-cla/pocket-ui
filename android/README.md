# Android launcher and pocket-core

The native ARM64 APK uses the shared pocket-ui library. The launcher and the private `:player` Activity have
separate native libraries; the player does not link winit, and its licence graph is checked independently. It has five launcher looks, search, favourites, Curator
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
Do not delete this identity. The APK is non-debuggable, version 0.1.6-android.8 / 10608.

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
```
