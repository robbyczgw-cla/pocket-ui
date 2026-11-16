# Handheld view on Android

The Linux app (`src/main.rs` with the Linux module set) as a third native library in the APK, `libpocket_handheld.so`,
shown by `HandheldActivity` in a process of its own (`:handheld`: the app ends its process when it ends, as on Linux). The
Mobile view (`PocketActivity`, `libpocket_android.so`) and the player (`PlayerActivity`, `libpocket_core_android.so`) stay
as they are.

- **Which view opens** (`Views.java`): `PocketActivity` stays the launcher and Home entry. Opened from there while Handheld
  is the chosen view, it starts `HandheldActivity` and ends; the Mobile library returns at once (`routedAway`). At the first
  start a pad decides (built in or attached: Handheld; none: Mobile). Settings → View in either view switches. An explicit
  start (`am start -n …/.PocketActivity`, the smoke tests) is always the Mobile view.
- **Games**: the handheld view reads them where they are, `/storage/emulated/0/ROMs/<system>` (an `es_systems.cfg` made at
  its start). It asks for All files access (before Android 11: storage read) at its start, explained, with the Mobile view
  as the other way. The Mobile view keeps the folder picker (SAF).
- `Cargo.toml`: its own manifest and lock, like `web/Cargo.toml`; feature `handheld` puts `src/linux_modules.rs` in the
  library on Android (`src/lib.rs`). `main.rs` is an example built as a cdylib.
- `app_android.rs`: `android_main` (folders, then `main`), the pad's keys, a game in the APK's player by its real path
  (`launchPath`), its end when the Activity comes back, the way back from the player's `last-picture.json`.
- Java: `PocketBase` has the Activity's Android I/O and loads no library; `PocketActivity` and `HandheldActivity` each load
  their own and name their own native methods (JNI binds by class name).
- **What the player is handed** (`android_handheld.rs`, plain functions the Linux tests check): the game's moments are the
  player's `states/<key>/<stem>` (`slots::set_resolver`, `StateKeys`: the open case, Continue and the card's slots read
  what the player wrote); its `settings` are the Mobile view's `launcher.json` with this view's own language and look over
  it (`launch_settings`), the game's Options → Picture rows as the player's three (`picture_choice`: scaling, frame, effect;
  a frame the player doesn't draw is none), a same-name `.cht` as its cheats, and whether the shelf showed the case open.
  What the player's pause menu changes in those rows is this view's Options → Picture when the game ends, for the game or
  its console as the menu kept it (`keep_pause_menu_picture`; only the changed rows), as pocket-core's pause menu on Linux.
  RetroAchievements: the sign-in file and the tallies folder are the app's (`account::share_in`, `share_account`), the pages
  reach the server through Android's HTTPS (`achievements::http::system`). QA builds only: `--features ra-replay` answers
  RetroAchievements from the device (`android/src/ra_replay.rs`), as in the player.
- Linux-only settings pages and quick settings rows are hidden (`device::ANDROID_HIDES`); a category left empty is not
  listed. On Linux, `POCKETUI_ANDROID_PROFILE=1` shows the same profile (tests/nav_explorer.rs walks it).
- Android owns sleep: the Linux idle timers are off (`sleep::Sleep::build`).

Build: `scripts/android/build-apk.sh` builds it with the others. By hand (NDK 27, from the repository root):

    CC_aarch64_linux_android=$NDK/bin/aarch64-linux-android26-clang AR_aarch64_linux_android=$NDK/bin/llvm-ar \
    CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER=$NDK/bin/aarch64-linux-android26-clang \
    RUSTFLAGS='-C link-arg=-Wl,-z,max-page-size=16384' \
    cargo build --release --target aarch64-linux-android --manifest-path android/handheld/Cargo.toml --example pocket_handheld

Start it by hand: `adb shell am start -n <package>/xyz.pocketui.app.HandheldActivity` (All files access for a test:
`adb shell appops set --uid <package> MANAGE_EXTERNAL_STORAGE allow`).

Ways to start a game (`src/play_route.rs`, shared with the Mobile view): our player, a core to download (the Download row on the
game's "Runs on" page), or another emulator app (`launchApp`, `RomProvider`). See docs/ANDROID-MVP.md "Handheld view: Runs on".
The manifest has `RomProvider` with the authority `<package>.roms` (a QA package renames it with its package).
