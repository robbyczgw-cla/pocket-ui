# File and game-start rule findings

The rules start from valid files and a core that produces sound. A damaged seed that was never accepted does not establish reader coverage. Each file uses the shared policy in `tests/common/data_faults.rs`: valid, missing, empty, four truncations, three byte flips and invalid encoding. Integration probes have a per-process deadline.

The game-start rule runs Shelf, Big, Split and Cabinet through the actual hosted player, audio queue/output writer, pause, saved state and return to Continue. The Pulse boundary records delivered samples and monotonic timestamps; it accepts only the pocket-core process. After one second of startup (the common pacer’s existing panel-test boundary), two seconds of presented frame ordinals, frame count and timestamps establish that matching 60 Hz play neither repeats nor skips a frame. Startup output and the whole-session pacer counters remain in the diagnostics; initial catch-up is intentional. This is a host output-boundary check; it does not measure speakers or a device's display.

## Confirmed product findings

Missing or undecodable Android app icons left their own icon field empty. The surrounding app name hid that omission from tests that merely checked the whole page for words. The expanded file rule reproduces the failure using the same icon reader and painter as AppsPage, then checks fallback words and actual pixels inside the icon field across languages, fonts, text sizes, field sizes and light/dark contrast. The fix supplies an `App` fallback in that field.

The JVM rule found that Android save export silently skipped a missing file already named in its export plan, then returned `exported`. A valid export is a positive control; removing its planned source must instead return `export-failed` and remove the incomplete destination. The test invokes production `SaveBackup.write` with actual files and ZIP streams.

## Native save copying

The Java rule reproduced truncation of an existing destination when the provider returned no stream or failed after a valid prefix: `SaveCopy.get` opened its destination before completing the read. The native copier now replaces that destination atomically only after a complete read. The Rust caller `save_copy::bring_back` already downloads into `copy-restore.part`, validates size and SHA-256, and commits after validation. The direct native-interface test did not demonstrate loss of a player's save through that complete restore path. Android Binder/provider lifecycle behavior remains outside the host proof.

## Test-fixture findings

- The shared stub formerly alternated an extra sample: 60 frames at 48 kHz produced 48,030 sample frames. A fractional accumulator now produces the advertised count. The shared-core sound-rate rule failed before that correction.
- The Pulse test boundary used relative sleeps, accumulating host scheduling and IO overhead. The strengthened journey failed with 92,928 instead of 96,000 sounding samples in a fixed two-second window. An absolute monotonic device clock now delivers the declared 48 kHz; every look passed with exactly 96,000 samples. Audio timing is compared with the first successful presentation of a game picture.
- Nine case-knows layouts had missing or ambiguous recognised positive controls in the previous extreme-value corpus. The rule now requires a recognised save for every one of the 49 supported layouts before mutating file-backed saves. These were coverage defects, not demonstrated product-parser failures.
- The first merge-gate run passed 5,059 tests but the new launch rule required Weston, absent on that host. A Python software-EGL compositor now uses the gate's existing Mesa/Wayland libraries and records actual changing picture buffers; no system package installation is required.
- A later gate passed all 5,065 Rust tests but its minimal PATH had no `rg`. The JVM runner's unchecked process substitution then supplied no Android test sources to `javac`. A checked Python-generated source list removes that dependency. The complete 66 BIOS and 209 Java probes pass with `rg` excluded from PATH.

The integration cases and private module rules exercise production readers, with synthetic files and temporary roots. ROM-art tests use synthetic images, including checksum suffixes for the real guards; no game assets are included. The reader inventory is conservative source-change detection, not proof that each OS or Android provider operation was dynamically fault-injected. Per-boundary coverage and remaining platform limits are recorded in `tests/fixtures/reader-audit.tsv`.
