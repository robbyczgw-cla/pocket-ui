# Android 0.2 emulator probes

## Odin recording regressions

Host checks: `cargo test --profile gate --lib android_video_tests`, `--lib player_title_tests`, `--lib vfs::tests` and
`--lib pad::tests`. The video tests run the production Earlier writer, exercise replacement without a frame, and feed
PS1/SNES resolution changes through the real libretro callback and Android picture-placement policy. Set
`POCKET_REQUIRE_GL=1` to require the software compositor checks too.

Compile `PlayerExitKeys.java` and `PlayerExitKeysTest.java` with Java 8 compatibility, then run
`xyz.pocketui.app.PlayerExitKeysTest`. No Android runtime is needed for this key policy test.

Build `--example vfs_contract --example android_core_probe --target aarch64-linux-android --release`, push the two
executables to an owned ARM64 emulator, and run `vfs_contract SCRATCH` and `android_core_probe CORE ROM SCRATCH [FRAMES]`.
Use separate scratch folders for games; the core may write SRAM there. The VFS contract must also run on Android:
glibc accepts `fflush` on read-only streams but Bionic returns EBADF. The unfixed Android probe reports `md: size=-1`;
ClownMDEmu then rejects even a plain `.md` ROM with fully readable permissions. The contract covers cartridge and disc
suffixes, Unicode/space-containing paths, buffered reads, rewind and cursor preservation. Use an actual bundled core
and known-good content for the core probe; it requires at least one video delivery and never opens an audio device.

These helpers run only in a disposable, rootable Android 35 ARM64 AVD. They are not
APK code. `02-smoke.py` rejects non-emulator serials and requires emulator root before
changing its generated games, app preferences or fixture states.

With the SDK/NDK versions used by `scripts/android/build-apk.sh`, from the checkout root:

```sh
mkdir -p android/build/qa-java android/build/qa-dex
javac --release 8 -cp "$ANDROID_HOME/platforms/android-35/android.jar" \
  -d android/build/qa-java scripts/android/test/Gestures.java
"$ANDROID_HOME/build-tools/35.0.0/d8" \
  --lib "$ANDROID_HOME/platforms/android-35/android.jar" \
  --output android/build/qa-dex android/build/qa-java/dev/pocket/qa/Gestures.class
# Select the NDK's host-specific prebuilt bin directory (darwin-x86_64 or linux-x86_64).
"$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/darwin-x86_64/bin/aarch64-linux-android26-clang" \
  -O2 scripts/android/test/gamepad.c -o android/build/qa-gamepad
adb -s emulator-5596 root
adb -s emulator-5596 push android/build/qa-dex/classes.dex /data/local/tmp/classes.dex
adb -s emulator-5596 push android/build/qa-gamepad /data/local/tmp/qa-gamepad
adb -s emulator-5596 shell chmod 755 /data/local/tmp/qa-gamepad
python3 scripts/android/02-smoke.py "$(command -v adb)" emulator-5596 android/build/evidence/02
```

`Gestures` injects actual two-pointer touch events through Android's shell input service.
`gamepad` creates a uinput gamepad and removes it on `quit`; the script owns its exact
subprocess and FIFO. No external game, network or physical controller is required for
the normal journey. Sound/system haptics are disabled on this disposable AVD.

Optional passes use the same installed APK and SAF library:

- `ANDROID02_SKIP_MATRIX=1`: lifecycle, pinch, input hot-plug, persisted toggle,
  dark/light held badges and return reminders, battery saver.
- `ANDROID02_EXTRA=1`: live font changes without process recreation, cutouts,
  actual edge Back gestures and German/Spanish pages at 200% in both appearances/orientations.
- `ANDROID02_PSP_ROM=/path/to/Psp2048.pbp`: checksum-verified fixture from
  `core-smoke-roms.json`; cached core button names must survive a launcher process
  restart, and an old short raw PSP moment must be explained and retained.
  Add `ANDROID02_LIGHT=1` for the light appearance. The fixture's own old states are reset.

Inspect screenshots as well as `result.txt`; semantic bounds alone cannot establish
readability. Run the PSP pass after populating the normal SAF fixture root. The probes
change disposable app data and Android settings; destroy the task's AVD after copying
its evidence. Never run these destructive QA fixtures against a player's data.
- `ANDROID02_COMPRESSED=1`, **after the shared compressed-state change is in main**:
  assert the Android writer emits the shared compressed Auto header, resume it after a
  process restart, upgrade an authentic old raw Auto made from the player's manual slot,
  and keep that manual slot raw. Fixture copies preserve app ownership and permissions;
  a root-owned copy of a private `0600` slot would test an unreadable file instead.

The system-asset installer also has a host-only Java test (no SDK or device): compile
`android/java/xyz/pocketui/app/SystemAssets.java` and `scripts/android/test/SystemAssetsTest.java`
with `javac --release 8 -d <temporary classes folder>`, then run
`java -cp <temporary classes folder> xyz.pocketui.app.SystemAssetsTest`.
It checks interrupted copies, first-start fonts, completing a partial folder and preserving existing files.
`python3 -I scripts/android/test-release-check.py` additionally rejects missing/damaged PSP assets in APKs.

## Direct document loading

Compile `GameDirect.java` and `GameDirectTest.java` with Java 8 compatibility and the SDK's `android.jar` on the
classpath, then run `xyz.pocketui.app.GameDirectTest` with the classes folder and `android.jar` on the runtime
classpath. It checks direct attempts for all 15 pinned cores (bundled or optional) and explicit fallback reasons for disc sets, parent-dependent CHD
and unchecked loaders. CHD header tests cover standalone and parent-dependent v3/v4/v5, truncated/unknown headers,
parent flags and both checksum types. `cargo test --profile gate --lib android_content` exercises original-name alias ownership,
repeated reopen/seek and provider refusal without payload files. `cargo test --profile gate --lib frozen_phone`
also exercises old large-game moments and changed documents using Android's actual CRC/fingerprint policy.

`core-info.c`, built with NDK clang and `-ldl`, takes bundled or installed core library paths as arguments and prints their
names, extensions, `need_fullpath` and `block_extract`. It never calls core initialisation, load, run, audio or input.
Run it only in an owned device/emulator session. Device evidence must still show real SAF first/repeated starts,
copy bytes/app storage and unchanged state/SRAM/RTC hashes for PSP, PS1 CUE/BIN and GBA; host routing alone cannot
establish that a provider permits `/proc/self/fd` reopening or that every native loader accepts the alias.

`vpad.c` is a simulated gamepad for a real device (not an emulator): a uinput device with the buttons, sticks, hat and
analog triggers of a Bluetooth/USB pad, any name/vendor/product, driven line by line from stdin (`key 305 1`, `abs 9 32767`,
`quit`). The adb shell may open `/dev/uinput` on the Odin 2 Mini, so no pairing is needed. Build static without the NDK:
`zig cc -target aarch64-linux-musl -O2 -static vpad.c -o vpad`, push to `/data/local/tmp`, remove afterwards.
`StickKeysTest.java` (with `StickKeys.java`) runs on the JVM: `javac -d out android/java/xyz/pocketui/app/StickKeys.java
scripts/android/test/StickKeysTest.java && java -cp out xyz.pocketui.app.StickKeysTest`.

## Cold Continue pictures

`examples/android_core_probe.rs` runs the shared frontend headlessly on Linux or the Android emulator.
Run `android_core_probe CORE ROM SCRATCH 180 save`, then the same command with `resume` in a second process.
Use an owned scratch folder and a diagnostic/homebrew game with a non-black scene at the chosen frame count;
repeat for each software core and Game Boy core preference. This checks the first post-load picture rather than
only a successful state-load log. The same ClownMDEmu state must be black before the palette patch and visible
after it, including a state written by the old core. Linux's normal Mega Drive core is Genesis Plus GX.
For GLES presentation and the matching Earlier preview, use the host regression documented in
[Android development](../../../docs/dev/android.md). Hardware cores additionally need the player's EGL context;
this headless software probe does not create one.
