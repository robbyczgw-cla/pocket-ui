# Android 0.2 emulator probes

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
