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
`NetworkRuleTest.java` (with `NetworkRule.java` and the SDK's `android.jar` on both class paths) runs on the JVM: the download rule
for updates and cores (unmetered Wi-Fi or Ethernet, a VPN on it included, never mobile data).
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

The host BIOS reader rule consumes the exact Rust fault corpus from
`tests/common/data_faults.rs`: `scripts/android/test/file-reader-rule.sh` compiles a tiny
corpus exporter and the real Java BIOS installer, then checks missing, empty,
truncated, flipped and wrong-encoding streams for all six supported BIOS names.
A failed replacement must preserve the kept BIOS and remove its partial file.
Run it on the Air with `tools/air-build.sh -m arm -x <worktree> bash scripts/android/test/file-reader-rule.sh --bios-only`.
This is a native JVM companion, not an Android ContentResolver/Activity test.

## Shared Java file-reader rule

On a Linux host with a JDK, run `bash scripts/android/test/file-reader-rule.sh`.
It runs the 66 BIOS stream variants and the production Java reader companion:
19 registered case groups, each with the eleven variants exported directly from
`tests/common/data_faults.rs`. `java-reader-rule.sh` can run just the latter.
The runner compiles the actual GameCopy, GameDirect, SaveBackup, SaveCopy,
UpdateInstaller and Views sources. Each variant runs in a new JVM. The watchdog
allows up to 30 seconds for startup, ending at the flushed `READER_READY` marker.
Reader work retains a five-second budget excluding measured Linux PSI CPU/IO/memory
stall time. Overlapping pressure cannot count twice for the same interval;
the reader has an absolute 30-second wall cap even under continuous pressure.
A separate five-second CPU budget (also enforced by the kernel, with a six-second
hard backup) catches busy loops independently of load. Stricter inherited CPU
limits stay in force. Without PSI the reader keeps its five-second wall budget.
Async fixture waits use this same watchdog. The first timeout or five failed
probes stops the rule; there are no timeout retries. Temporary files
belong to that probe and are removed afterwards. Valid seeds must demonstrate
accepted view choices, copy-cache reuse, matching ZIP members/manifest, exact save
bytes, installed-version consumption and checked payload transfer.

Timeout diagnostics identify startup or reader work, elapsed wall/CPU time and
pressure allowance. A reader timeout requests a bounded HotSpot thread dump before
killing and reaping that exact child. Diagnostics have 0.5 s and reaping has 2 s;
an uninterruptible kernel wait is reported with its PID instead of blocking the
watchdog forever. Phase maxima are observations at the watchdog's
50 ms polling resolution. `test-reader-watchdog.py` covers slow startup and IO,
overlapping pressure, persistent blocked readers and CPU loops with a virtual clock;
it also proves that an actual blocked child is diagnosed, killed and reaped. It
creates no synthetic host load and runs with the Java rule.

The former five-second timeout covered JVM startup, fixture IO, `fsync` and the
reader together. Its historical `save-copy-file/empty` failure had no phase data,
so it does not establish an empty-file loop or a specific source of gate load.
The empty reader exits on EOF, then syncs and renames its replacement. A delayed
JVM startup reproduces the old false timeout on that same empty-file case; the
new watcher accepts it while retaining the independent hang limits above.

`JAVA_READER_JDK` may select a JDK installation; otherwise the scripts try PATH,
JAVA_HOME and existing `~/opt/jdk-*/bin` installations. They install nothing and
need no network. `--if-available` returns 77 with an explicit message if the host
lacks a compatible JDK or Linux `/proc/self/fd`. The ordinary merge gate invokes
this mode after successful Rust tests: a real reader failure makes the gate red,
whereas unavailable host capabilities are logged as an unexecuted JVM platform
stage. `--bios-only` keeps the original portable BIOS-only test on a non-Linux JVM.

The source inventory maps each Java reader to its registered cases and rejects
missing or unassigned case IDs. The narrow Android API doubles and the vendored,
licensed test-only Android JSON source are documented in [jvm/README.md](jvm/README.md).
The production GameDirect probe really reopens a Linux descriptor and seeks it;
it does not establish Android Binder/SAF permissions or Bionic/native-loader
behavior. APK byte transfer and installed-notice parsing are tested; Android's APK
parser, signing implementation, install acceptance and Activity/task routing are
platform boundaries. Native protocol outcomes are checked here; translated Rust
views remain covered by their own host painter/model rules.

`OrientationTest.java` (with `Orientation.java`) runs on the JVM: `javac -cp android.jar -d out android/java/xyz/pocketui/app/Orientation.java
scripts/android/test/OrientationTest.java && java -cp out:android.jar xyz.pocketui.app.OrientationTest`. A square panel (the RG Rotate's 720x720)
follows the system's turn; everything else keeps the controller-landscape rule.
