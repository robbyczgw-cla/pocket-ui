# Standalone Brick Bluetooth

*Part of [DEVELOPING.md](../DEVELOPING.md). Read this when you work on Bluetooth, controller layouts or save-safety / literal-name regressions.*

## Standalone Brick Bluetooth

The existing Bluetooth page uses the card's `brick-bluetooth` service through its systemctl stand-in;
`brick-bluetoothctl` preserves pairings on the card. See [TRIMUI-BRICK-BLUETOOTH.md](../TRIMUI-BRICK-BLUETOOTH.md)
for lifecycle, firmware dependencies, host stubs and the controller-only audio boundary.

Brick button labels use `i18n::device_text` after translation and before `Text` measures or wraps them (including raw manual prose).
Learned game names bypass it; other device profiles retain HOME. The slow-load note is the shared one
(`app_loading`, every Linux device): its timer requests the due frame before advancing, so it reaches pocket-fb after the launch animation.

### Android 0.2 controller regressions

`src/android_02_tests.rs` includes `android/src/ui.rs` and `play_cards.rs` in the Linux
library-test harness, so real navigation, preference reload, control names, crop boundaries
and preservation of rejected PSP moments are checked without a device. Run
`cargo test --profile gate --lib android_02_tests`; include the focused `type_scale`,
`controls::tests` and `frozen_phone` filters after changes to the shared rules. Compile both
Android libraries with `scripts/check-platforms.sh android`. The disposable AVD journeys
and shell-only pinch/uinput probes are documented in [the Android guide](../../android/README.md#android-02-preview).

Android’s internal player holds the reminder until a button or touch; `android/src/player.rs` has no reminder deadline.

### Identity-bound Linux controller layouts

The [input corpus](../../tests/fixtures/input/README.md) records real device identities, capability bitmaps and axis ranges.
`cargo test --profile gate --lib input_corpus_tests` runs every public descriptor through the production decoder,
controller merging, game input, both menu-role layouts and shortcuts. The full gate additionally runs
`--features brick --lib input_corpus_tests`, including the built-in Brick whose USB ID collides with Xbox 360.
Never replace captured data with a generic controller descriptor or weaken expected roles to fit a new decoder.
The Brick F-key snapshot and corpus fixtures supply their verified plain-Brick identity explicitly through
`Decoder::with_caps_profile`; the host has no stock MainUI. Live readers keep `with_caps` and its firmware-identity check.
Refresh with `scripts/input-corpus-capture.py` (read-only, including already-open InputPlumber nodes) or
`scripts/input-corpus-capture.c` on devices without Python; neither reads gameplay events or grabs a device.


`src/pad_linux_decode.rs` normalises evdev using the selected SDL Linux records in `assets/controllers/`:
buttons and axes are capability-ordered indices, digital hats stay hats, trigger axes produce L2/R2 button edges
with 0.55/0.40 hysteresis. `src/pad_bindings.rs` saves device permutations to `controllers.json`; its reader runs
on the supervisor, never during painting. `src/app_controllers.rs` captures input before the merged menu pad,
undoing menu roles and the current device permutation. Held releases retain their original role. Game-specific
`ctl.*` remains a separate stage; interactive assignments in `emu_menu` swap conflicts while old files stay readable.

On the Brick, F1/F2 bypass both 16-button mappings: `brick-keys.json` chooses global system shortcuts, loaded by
pocket-core on launch. Controller capture receives these keys but ends with the Lights & buttons explanation instead of
waiting for timeout. The game Controls entry uses the same note (`brick_hw::FKEYS_REMAP_NOTE`). See
[the hardware notes](../TRIMUI-BRICK-HW.md#f1f2-assignment-scope) and `tests/brick_settings_walk.rs` (`fkeys` regressions).
The slider choice uses the same state directory for its Rust writer and the launcher's single `BRICK_SLIDER_CFG`
export, inherited by volume restore and the watcher. A legacy top-level file is migrated only when the state file is
absent; the host card probe covers this boundary with a file saved through `Slider::save`.

Each `pad::Supervisor` entry owns a reader stop signal. Retiring an entry (node removal/replacement, including
udev metadata changes) signals its old reader before opening the replacement; dropping the supervisor retires
all readers. Linux polls evdev and an eventfd together, with nonblocking evdev reads and retry on EINTR/EAGAIN.
Idle pads sleep until input or retirement; if eventfd creation fails, polling falls back to a 100-ms cancellation
check. Retirement releases held input and sends Disconnected before closing the evdev fd. No grab is taken.
The log distinguishes a node change from a reader ending. A source's late completion cannot retire a newer source.

`src/pad_bridge.rs` runs as `pocket-ui --controller-bridge <program> <args...>` for ROCKNIX SDL Dolphin and ARMSX2
(`bridged`). Not Azahar: its qt-config.ini binds each button to the built-in pad's SDL GUID, and a seat's GUID changes
with every start, so a 3DS game would have no buttons. When the seats can't be made (no uinput, no event node) the
emulator starts without them, with ROCKNIX's controller setup. Four uinput seats remain present until that game exits; SDL sees only that wrapper's reserved VID/PIDs.
A frozen game's seats cannot collide with another game's. Unchanged triggers retain analogue travel; remapped trigger buttons drive SDL trigger axes.
Sticks stay analogue. Force feedback forwards to the real controller through `rumble`, whose discovery excludes
bridge pads. External players outside these adapters retain their native input path.

Targeted verification: `cargo test --profile gate --lib pad::`, `players::`, `controls::`, `emu_menu::`, `hotkeys::`.
The ignored `uinput_modes_two_pads_and_hotplug_use_the_real_reader` test needs root and `/dev/uinput`; run the
compiled library test executable with that filter and `--ignored --test-threads=1`.
The ignored `retiring_a_silent_reader_releases_held_input_during_capture` test uses a real uinput reader and a
fake directory generation change while L2 is held: retirement must release L2 and finish without another press.
`tests/user_journeys.rs::controller_capture_survives_disconnect_and_reconnect` drives the ordinary UI loop,
including the handheld's B, the timeout, successful capture after reconnect and rendered frames. Its ignored
`uinput_disconnect_during_controller_capture_keeps_ui_alive` counterpart uses the saved hid-generic Lite 2 S-mode
key/axis capabilities (057e:2009, version 0001); events enter through the actual evdev pump, then the isolated
driver FIFO (`tdisconnect` carries Disconnected). Axis ranges are synthetic. Run that compiled integration test
as root with `/dev/uinput`; it creates/removes only its own test node. These tests cover userspace lifecycle,
not Bluetooth transport, the H700 kernel or compositor liveness during a hardware lockup.
`scripts/test-controller-bridge.py <pocket-ui-binary>` (root, SDL2) exercises two identical D-input pads through the
actual helper and SDL, including distinct maps, held input, disconnect, reconnect and right-stick isolation.
It creates only its own input nodes when udev is absent and removes them after the run.
Snapshot states: `controllers:0`, `controllers:7:capture`, `controllers:7:swapped`, `controllers:7:disconnected`.

Update transport (`src/update.rs`): signed alternate sources, per-source 30-second no-growth and five-minute total limits; strict full-file SHA-256 after every attempt. `src/update/download_tests.rs` exercises blocked, redirecting, stalled, truncated and corrupt servers with real curl. See [UPDATES.md](../UPDATES.md#bounded-downloads-and-alternate-sources-03).

Standalone card assets: `src/update/brick_assets.rs` hashes immutable installed components, downloads every missing component into RAM before card writes, and reuses unchanged files with a durable rename/recovery manifest. `src/update/brick_io.rs` caps RAM downloads and shares a 2 MiB/s, 256 KiB/fsync bulk-write budget between update extraction, web/phone transfers and save backup/restore; their worker threads use idle I/O priority. The fixed Brick boot script recovers interrupted moves and rollback before launching an app. Before UI startup, `pocket-ui --brick-seed-data SOURCE DESTINATION [RELEASE]` (`src/update/brick_seed.rs`) compares bundled engine data by content and atomically replaces only changed files through the same write budget. It invalidates the BIOS release marker before writes, so rollback repairs partial new data; unrelated player BIOS files stay untouched. The optional release marker is published by the same descriptor-relative helper with an exclusive temporary and atomic rename, refusing symlinks. `scripts/brick-update-pack.py` prepares immutable packages; `scripts/test-brick-update-pack.py` covers stable reuse and hosting-size refusal.

**Test buttons** (`src/button_test.rs`, `src/app_test_buttons.rs`): `merge_pad` hands every pad event to `test_buttons_event`
before the controllers are merged (as the capture above), with the controller of its source. The events are the
App's: the reader has applied the controller's own layout and then the Button layout, so `Mapping::physical` undoes both
(the same inversion as the capture) and a held button keeps the physical button it was pressed as. While the page is open
`user_event_inner` stops every pad event but volume and power at its top (`test_buttons_swallows`), so quick settings, the
leave dialog and the other handlers never see B, START or HOME; a press from no controller (`usize::MAX`: the touch
screen's tap on the hint pill, the FIFO) leaves at once. The way out is a hold counted in `ButtonTest` and ended in
`about_to_wait` (`test_buttons_tick`). Tests: `cargo test --profile gate --lib button_test::` (virtual pads through the
real `pad::decode::Decoder`: a trigger-as-axis Pro 2 and two pads), `--test empty_views` (no controller found).
The test driver's command FIFO has three verbs for it (`--test-driver` only): `tconnect <source> <key> <name…>`, `tbtn <source> <button> <down|up>` and `taxis <source> <axis> <value>`, which send what a controller of its own would (`tests/user_journeys.rs`, `the_button_tester_takes_every_button_and_is_left_by_a_hold`). Snapshot states: `test-buttons`, `test-buttons:idle` (nothing pressed yet: the two hints), `test-buttons:bt`, `test-buttons:two`, `test-buttons:remap`, `test-buttons:xbox`,
`test-buttons:hold`, `test-buttons:unknown`, `test-buttons:empty`.

Cartridge-clock and start-failure regressions live in `libretro.rs` (`clock_regression_*`, `full_card_regression_*`), `fallback.rs`
(moment protection and legacy migration) and `fast_switch.rs` (first-frame evidence survives SIGKILL).
See [HOST.md](../HOST.md#failed-starts-and-continue-safety-2026-11-07) for the pipe protocol and temporary retry.

### Core save-safety regressions

The `save_safety_` tests cover failed undo capture, failed exit handoff, stale Auto copies into Earlier, shortcuts collected while paused/asleep/dark, dark touch, stale standalone resume workers and a failed Continue rename. The pocket-core tests use a compiled stub core in child processes, with no display, sound server or player saves. Run the focused library and pocket-core tests before the merge gate. The explicitly ignored copy-only transit and failed-audio-write tests document known recovery failures that require separately reviewed persistence/audio changes; they are not claims of successful recovery.

The Linux player's `core_command.rs` decodes the backward-compatible FIFO into typed commands and
quit intent; `core_status.rs` projects the existing control flags into a read-only play state and
publishes it with `ui_status::Writer`. The [core status contract](../DEVICE-TESTS.md#where-the-core-is)
documents fields, revisions and stale files. Run the `core_command_`, `core_status_` library tests
and `core_contract_` pocket-core tests when changing either side of the contract.

### Literal game and system names

Use `Text::literal` around measurement and painting of a name, `Text::card_name` plus
`draw_name` for card and `title_block::name_layout` / `name_layout_in` for selected
names and Details. `Line::literal` carries this identity through positions and painting;
manual title painters use `title_block::paint_line`. A name fragment must never depend on
one-use `Cache::name_line` exemptions: measuring may run more often than drawing.
`tests/names_never_translated.rs` compares the five production painters with literal
references in DE/ES for every bundled game label/alias, console name/alias, and the snapshot
and performance fixture corpus (`tests/name-corpus.py`). UI words must still translate afterward.

Case art also prints UI entries such as Search and Favourites. Its callers use
`media::printed_title(source, item.title_is_name())` before the literal case painter:
UI wording is translated once as a whole; game/system names and player-authored shelf/sticker
names bypass translation. This explicit origin also handles a game called Search without
preventing the Search menu entry from being translated. The render regression checks both.

Compact data rows use `Text::fit_name` / `wrap_name` and `draw_name` too: the metadata
chooser, related-game names/system lines, management heading and proper-name fact values
(including Listed as). The database corpus also compares both fitting helpers with literal
references, before painting each name through the main components and the case front.

Case fronts, system bands, cartridge labels and disc-ring names use the same literal scope;
`cabinet_art::plain_label` receives final printed wording (own UI entries are translated by
`printed_title` first). This includes metadata titles that have never entered the learned-name
cache. Mixed UI headings such as Note for / Delete keep their existing raw `{s}` placeholders.
For a mixed system/year/player caption, translate the player-count wording before the
literal fit. Chooser `Choice::Entry` rows contain names; `Choice::NoEntry` contains our
translated title and explanation. The rendered UI counterchecks cover both cases in DE/ES.

Android 0.3 regressions extend `src/android_02_tests.rs` with actual Retro palette navigation, complete painted
selected names in all six phone looks/orientations at 100%/200%, provider artwork/sidecar isolation and persisted
cheat failures. `android/src/moments.rs` fault tests distinguish refused loads, failed Before load writes and failed
rollback (blocked saves). The existing bounded-core cheat regression covers Nestopia as well as Snes9x.

The Android name regression paints actual DE/ES phone rows, title bands, box headers and pause titles against a literal reference, checks raw accessibility names, and verifies that surrounding interface text still translates.

Power-key inhibitor lifecycle: `sleep_child.rs` transfers the inhibitor from its startup thread to the UI.
Its stdin pipe, rather than that short thread, owns its lifetime; inherited descriptors close at exec.
`tests/sleep_children.rs` checks that the inhibitor stays alive after the handoff and ends after stdin closes.
