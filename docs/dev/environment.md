# Environment variables

*Part of [DEVELOPING.md](../DEVELOPING.md). Read this when you add or read a POCKET* variable (doc_checks holds this list).*

## Environment variables

These are meant for development and tests; normal use needs none of them. On a device, put them one per line in
`/storage/pocket-ui/dev.env`. Every variable the code reads is listed here (checked by `doc_checks`).

**pocket-ui**

| Variable | Effect |
|---|---|
| `POCKETUI_MOCK=1` | Use in-memory mocks for brightness, Wi-Fi, battery and so on; skip the hardware GPU stock-clock helper |
| `POCKETUI_MOCK_NOW=<Unix seconds>` | With `POCKETUI_MOCK`: freeze the wall clock used by Curator, relative play history, Automatic appearance and the room/night lighting; the synthetic library generator accepts the same value for timestamps and anniversaries. Room lighting retains `POCKETUI_CLOCK_SHIFT` and the explicit `POCKETUI_ROOM_TIME` override; static cache comparisons keep byte-for-byte PNG and probe equality across real clock transitions. Snapshot fixtures pin both children to `tests/common/root.rs`’s `FIXTURE_NOW`; monotonic deadlines and cartridge RTC tests keep their own clocks. |
| `POCKETUI_MOCK_ZONE=<zone>` | With the mocks: the time zone the system starts with (`UTC` for a fresh ROCKNIX) ([VIRTUAL-DEVICE.md](../VIRTUAL-DEVICE.md)) |
| `POCKETUI_DATA` | Use another data folder instead of `/storage/.config/pocket-ui` |
| `POCKETUI_ES_SYSTEMS` | Read another `es_systems.cfg` |
| `POCKETUI_REPORTS=<dir>` | Write Settings → Help & tools → About → Report a problem's zip into this folder instead of `/storage/roms/reports` |
| `POCKETUI_SCREENSHOTS=<dir>` | Save screenshots (HOME + A) and let the web companion list them from this folder instead of `/storage/roms/screenshots` |
| `POCKETUI_PORTS_JSON=<file>` | Tests only: `cargo test` also parses this downloaded PortMaster catalog (`ports::tests`) |
| `POCKETUI_TEST_PM_LIFETIME_HELPER=<file>` | Tests only: the owned fake PortMaster helper for the child-UI lifetime regression (`ports::tests`); never set by a launcher |
| `POCKETUI_MOONLIGHT=<program>` | Start this program for PC streaming and Settings → Help & tools → For tinkerers → PC streaming (pair, list, unpair) instead of ROCKNIX's `moonlight` (virtual device, tests) |
| `POCKETUI_WEB_GAMEDB` | Build time, the browser demo only: `web/build.rs` names the game-facts file cut to the demo's systems, compiled in instead of `assets/gamedb.bin` ([WEB-DEMO.md](../WEB-DEMO.md)) |
| `POCKETUI_WIFI_IFACE=<name>` | The network interface counted as Wi-Fi (`eth0` in the virtual device; `wlan0` by default) |
| `POCKETUI_BATTERY=<dir>` | Read the battery from another sysfs directory than `/sys/class/power_supply/battery` |
| `POCKETUI_SNAPSHOT_NATIVE=1` | Snapshots only: bypass the logical canvas for raw small-panel layout comparisons |
| `POCKETUI_FRAME_BENCH=1` | Snapshots (`home`, `settings`, `main`): report settled paint and paint-plus-reduction thread CPU medians and p95, excluding startup, PNG and presentation ([H700-SCALED-LAYOUT.md](../H700-SCALED-LAYOUT.md)) |
| `POCKETUI_SNAPSHOT_WIFI=1\|0` | `--snapshot`: the status corner as if Wi-Fi were connected (`1`) or not (`0`) (the `--snapshot` table above) |
| `POCKETUI_SNAPSHOT_GAMES_CARD=1` | `--snapshot`: Add games as with a games card in (`games_card`) |
| `POCKETUI_SNAPSHOT_IMAGE_CARD=1` | `--snapshot`: Home and Add games as with a flashed system image in the second slot (`card_health::image_card`) |
| `POCKETUI_SNAPSHOT_GAME=<system>\|<name>\|<file>` | `--snapshot`: the game behind the pause menu and the slot pages (default `gbc\|uCity\|uCity.gbc`); `scripts/readme-shots.sh` sets it |
| `POCKETUI_SNAPSHOT_FLIP_MS=<ms>` | `--snapshot box:…`: the box's last turn that long after it began, instead of done (`game_box::BoxView::snapshot_settle`) |
| `POCKETUI_SNAPSHOT_OPEN_MS=<ms>` | `--snapshot box:…`: the game that long into sliding out of its case, instead of out (`game_box::BoxView::snapshot_settle`) |
| `POCKETUI_SNAPSHOT_FROZEN=<n>` | `--snapshot`: the first n recent games of `POCKETUI_DATA` stand open, whatever is on the card (`App::frozen_snapshot`) |
| `POCKETUI_SNAPSHOT_SPIN_MS=<ms>` | `--snapshot box:…`: the disc beside its case that long into its slow turn (Playful animations, `game_box::BoxView::snapshot_settle`) |
| `POCKETUI_BENCH_FRAMES=<n>` | `--snapshot box:…`: after the picture, that many frames of the box 16 ms apart (its disc turning with Playful animations), their thread CPU time on stdout as `bench-box … p50_us= p95_us= max_us= over_16ms=` |
| `POCKETUI_SNAPSHOT_HELD=<steps>:<ms>` | `--snapshot shelf:…:shelf:…`: Right held for that many steps 70 ms apart, painted `ms` after the last one: the spines leaning or swinging back (Playful animations, `App::shelf_playful_snapshot`) |
| `POCKETUI_SNAPSHOT_MOVED_MS=<ms>` | `--snapshot shelf:…:shelf:…`: the selection that long after it moved: the spine springing up, the front turning and its wiggle (`App::shelf_playful_snapshot`) |
| `POCKETUI_TEST_STALL_CHILD=<dir>`, `POCKETUI_TEST_OPERATION=<phase>`, `POCKETUI_TEST_BLOCK_PHASE=<phase>` and `POCKETUI_TEST_ENTERED=<file>` | Set by `src/bin/pocket_core/quit_regression.rs` (`stalled_player_returns_to_shelf_and_preserves_continue_in_every_phase`) for its own child process: the child blocks the core in one phase (`load`, `run`, `size`, `resume`, `retry-run`, `unload`; `ENTERED` is the file it creates once it is stuck) and the parent checks that the watchdog sends the player back to the shelf and keeps the earlier Continue moment. Not for use by hand |
| `POCKETUI_TEST_CRASH_CHILD=<dir>` and `POCKETUI_TEST_ABORT_PHASE=<phase>` | Set by `src/bin/pocket_core/quit_regression.rs` (`a_moment_that_crashes_the_core_while_loading_is_not_tried_at_every_start`) for its own child process: the stub core aborts inside `retro_unserialize` twice; the third start must come up fresh and Load must still list the moment. Not for use by hand |
| `POCKETUI_TEST_QUIT_CHILD=<dir>` and `POCKETUI_TEST_BLOCK_SERIALIZE=<file>` | Set by `src/bin/pocket_core/quit_regression.rs` for its own child process: the child loads a fake core whose `serialize` never returns and must still leave within the deadline, keeping the earlier Continue moment. Not for use by hand |
| `POCKETUI_TEST_COVERS=<dir>` | `cargo test` (`media::tests::real_covers_lose_their_side_strip`): a folder with `gba/`, `gb/` and `snes/` box art PNGs (e.g. a library's thumbnails; not in the repository) to check that a system's printed side strip is cut off the cartridge labels; without it the test says SKIPPED |
| `POCKETUI_TEST_CRASH_AT=<step>` and `POCKETUI_TEST_CRASH_SPEC=<json>` | Set by `update::crash_tests` for its own child process: the child runs `install_app` and ends with SIGKILL at the named step (`downloaded` … `linked`, also `layout-copied-*`, `layout-synced`, `layout-current`, `layout-linked-*` during flat-install conversion). Covers existing same-version release folders as well. Not for use by hand |
| `POCKETUI_SYNC_OWNERSHIP_FIXTURE=<dir>` and `POCKETUI_SYNC_OWNERSHIP_MODE=<mode>` | Tests only (`src/sync_ownership_tests.rs`): a child process runs a fake sync service with isolated wish/lock state, a temporary ownership marker, and an API success/crash, refused marker write or refused resume. No device paths are used. Not for use by hand |
| `POCKETUI_TEST_KILL_AT=<n>` and `POCKETUI_TEST_KILL_LOG=<file>` | Tests only (`fsutil::crash_point`): the process ends with SIGKILL at the n-th write step; the log lists every step reached. Set by `libretro`'s quit test for its own child. Not for use by hand |
| `POCKETUI_TEST_QUIT_DIR=<dir>` | Set by `libretro::tests::a_quit_cut_off_at_any_write_step_keeps_a_whole_moment_and_the_newest_battery_save` for its own child process (the session that quits). Not for use by hand |
| `POCKETUI_TEXT_FIT_OUTPUT=<dir>` | `cargo test` (`overlay` startup notes): also write each fitted note as a PNG into this folder, to look at them |
| `POCKETUI_ROOT`, `POCKETUI_SYSTEM_CFG`, `POCKETUI_ROCKNIX_FUNCTIONS`, `POCKETUI_ROCKNIX_PROFILE`, `POCKETUI_ES_SETTINGS`, `POCKETUI_ES_ONLY_FILE`, `POCKETUI_START_ES`, `POCKETUI_LOCALE_DIR`, `POCKETUI_TZ_CACHE`, `POCKETUI_QUANTUM`, `POCKETUI_SWAY_WAIT`, `POCKETUI_RESTART_DELAY`, `POCKETUI_UI_PID_FILE`, `POCKETUI_GAME_FILE`, `POCKETUI_DARK_FILE`, `POCKETUI_FAN_BEAT`, `POCKETUI_FAN_NOTE` | Read by the launcher `deploy/run-pocket-ui.sh`: its folders, ROCKNIX files and programs, waits and marker files moved into a test folder. `update::crash_tests` and `deploy/test-run-pocket-ui.sh` set them to start the real launcher on a computer. Not for use on a device |
| `POCKETUI_SNAPSHOT_EJECT_MS=<ms>`, `POCKETUI_SNAPSHOT_EJECT_FRAME=<png>`, `POCKETUI_SNAPSHOT_EJECT_SERIES=<dir>`, `POCKETUI_SNAPSHOT_EJECT_BENCH=1` | `--snapshot cabinet:…`, `shelf:…`, `big-shelf[:<file name>]` and `kids-shelf`: Cozy's way back from the selected game that long after it ended, with that picture as the game's last frame; every 60 Hz frame of it into `<dir>`; or the line `bench-eject …` (its start, frames and a button along the way) (`App::eject_snapshot`) |
| `POCKETUI_SNAPSHOT_LAUNCH_MS=<ms>` | `--snapshot shelf:…`: the launch animation that long after A (`App::launch_animation_snapshot`) |
| `POCKETUI_SNAPSHOT_LAUNCH_SERIES=<dir>` | `--snapshot shelf:…`: every 60 Hz frame of the launch animation from A to the end of its fade into `<dir>` (`0000.png` …) and the line `launch-series paint …` with the paint times (`App::launch_series_snapshot`) |
| `POCKETUI_SNAPSHOT_SIZE=<w>x<h>` | `--snapshot`: paint at another screen size than the device's (the Mini V2's 1240x1080 unless `POCKETUI_DEVICE` or a `:device=` says otherwise; `1920x1080` the Pocket 5 and Flip 2, `1280x960` the Mini V1, [DEVICES.md](../DEVICES.md)); a value that isn't a size from 320x240 to 4096x4096 is ignored |
| `POCKETUI_DEVICE=<name>` | Which handheld this is, instead of the device tree: `miniv2`, `mini`, `rp5`, `rp5-visionox`, `flip2`, `flip2-visionox` (a `--features brick` build also `brick` and `smartpro`, [TRIMUI-SMART-PRO.md](../TRIMUI-SMART-PRO.md)) or a device tree `model` string (`src/device.rs`, [DEVICES.md](../DEVICES.md)); a snapshot state may end in `:device=<name>` for the same |
| `POCKETUI_STATES` | Save-state directory instead of `/storage/roms/savestates` |
| `POCKETUI_STATUS=<file>` | Where the UI writes its one-line status instead of `/tmp/pocket-ui.status` (`<dir>/ui.status` under `--test-driver`): set it for every second instance (a device journey, a decoder fixture, a navigation probe) so that it never overwrites the owner's status; `scripts/ui-status.sh` honors it too ([DEVICE-TESTS.md](../DEVICE-TESTS.md), `src/ui_status.rs`) |
| `POCKETUI_GBA_CORE` | Development path to the bundled ARM64 gpSP library; enables H700 GBA core selection on a stand-in device too, retaining existing Continue ownership and saved choices ([HOST.md](../HOST.md#h700-gba)) |
| `POCKETUI_TEST_PM_LIFETIME_HELPER=<program>` | Tests only (`ports::tests::killing_the_ui_process_stops_its_portmaster_helper_with_a_checkpoint`): passes a fake PortMaster helper to its child UI process so killing that UI verifies helper shutdown and a resumable checkpoint. Not for use by hand |
| `POCKETUI_TEST_PREPARE_MS=<ms>` | With `POCKETUI_MOCK` only: delay the launch worker, capped at 2 s |
| `POCKETUI_TEST_PREPARE_HOLD=1` | With `POCKETUI_MOCK` only: the launch worker waits (at most 2 s) until `bench-launch-response` has painted five frames; `tests/launch_response.rs` checks that the first frame with the insertion came before the preparation got past that hold, and that A and that frame cost the UI thread at most 50 ms of CPU time (no wall clock) |
| `POCKETUI_TEST_TIME_FACTOR` | Tests only: an extra scheduling allowance from 1 to 64 for hang guards on a constrained runner; product deadlines and simulated clocks are unchanged (see the timing-test notes above) |
| `POCKETUI_BIOS` | BIOS directory (pocket-core's system folder) instead of `/storage/roms/bios` |
| `POCKETUI_HOST_BIN` | Path of `pocket-core` instead of `/storage/pocket-ui/pocket-core` (for custom installations) |
| `POCKETUI_HW_ROOT=<dir>` | The hardware seam (`system_ctl::RealHw`) looks below this folder instead of `/` (sysfs files, stand-in programs in `usr/bin`); only in a `--features brick` build ([TRIMUI-BRICK.md](../TRIMUI-BRICK.md)) |
| `POCKET_CORE_SAVE_DIR=<dir>` | pocket-core keeps a game's battery saves and memory cards in `<dir>/<system>` instead of next to the game; only in a `--features brick` build |
| `POCKET_CORE_SOFTWARE=1` | pocket-core without GL, the CPU copies the picture; only in a `--features brick` build ([TRIMUI-BRICK.md](../TRIMUI-BRICK.md)) |
| `POCKETUI_HW_ROOT=<dir>` | Only in a `--features brick` build: `system_ctl::RealHw` looks for sysfs files and stand-in helpers below this folder instead of `/` ([TRIMUI-BRICK-STANDALONE.md](../TRIMUI-BRICK-STANDALONE.md)) |
| `POCKETUI_BRICK_STANDALONE=1` | Only in a `--features brick` build: the Brick runs from its own card without NextUI, so the Wi-Fi page and Wi-Fi hints stay (`brick_profile::standalone`, [TRIMUI-BRICK-STANDALONE.md](../TRIMUI-BRICK-STANDALONE.md)) |
| `POCKETUI_BRICK_BLUETOOTHCTL=<program>` | Brick builds: the `bluetoothctl` pocket-ui runs (the card's `hwroot/usr/bin/brick-bluetoothctl`, set by `deploy/brick/start.sh`; [TRIMUI-BRICK-BLUETOOTH.md](../TRIMUI-BRICK-BLUETOOTH.md)) |
| `POCKETUI_UPDATE_URL=<url>` | Only in a `--features brick` build: the update channel when settings.json has no `update_url` (`update::default_url`); the Brick card's start script sets it from `app/update-url` ([TRIMUI-BRICK-REMOTE.md](../TRIMUI-BRICK-REMOTE.md)) |
| `POCKETUI_BRICK_CARD=<dir>` | Only in a `--features brick` build: the card's pocket-ui folder: an app update replaces its program folder `app/` (`update::install_brick`) instead of a release under `/storage/pocket-ui`; set by the Brick card's start script ([TRIMUI-BRICK-REMOTE.md](../TRIMUI-BRICK-REMOTE.md)) |
| `POCKETUI_TS_PROXY=<url>` | Only in a `--features brick` build: Tailscale's local proxy (`http://127.0.0.1:1055` on the Brick card): update lists and files on a `*.ts.net` address are fetched through it (`update::proxy_args`, [TRIMUI-BRICK-REMOTE.md](../TRIMUI-BRICK-REMOTE.md)) |
| `POCKETUI_TAILSCALE=<file>` | Only in a `--features brick` build on the Brick's own card: the `tailscale` command Settings › Tailscale runs, instead of `/usr/sbin/tailscale` ([TRIMUI-BRICK-REMOTE.md](../TRIMUI-BRICK-REMOTE.md)) |
| `POCKETUI_TAILSCALED_SOCKET=<file>` | tailscaled's LocalAPI socket the web companion asks (`/run/tailscale/tailscaled.sock` when unset); the Brick's own card sets its run folder's ([TRIMUI-BRICK-MORE.md](../TRIMUI-BRICK-MORE.md), `src/web/net.rs`) |
| `POCKETUI_BRICK_SYS=<dir>` | Only in a `--features brick` build: the Brick's sysfs (motor pin, LED driver) is below this folder instead of `/`; a fake tree in tests ([TRIMUI-BRICK-HW.md](../TRIMUI-BRICK-HW.md)) |
| `POCKETUI_MOTOR_VOLTAGE=<min>,<max>` | Only in a `--features brick` build: the range `/sys/class/motor/voltage` takes, to try one without a rebuild ([TRIMUI-BRICK-HW.md](../TRIMUI-BRICK-HW.md)) |
| `POCKETUI_RUNEMU` | The program that starts standalone emulators, instead of `/usr/bin/runemu.sh`  |
| `POCKETUI_CORES=<dir>:<dir>` | Folders searched for `<core>_libretro.so` after `/storage/cores` (bundled or custom cores) |
| `POCKETUI_BACKLIGHT=<dir>` | The backlight sleep dims (the first one under `/sys/class/backlight` by default); the launcher reads it too |
| `POCKETUI_LOG=<file>` | The log the logger copies to the card (`/var/log/pocket-ui.log` by default); the launcher writes it |
| `POCKETUI_SNAPSHOT_SIZE=<w>x<h>` | `--snapshot`: paint at this size instead of the panel's 1240x1080, e.g. `1920x1080` for a TV ([DISPLAY-OUT.md](../DISPLAY-OUT.md)) |
| `POCKETUI_TV_OUTPUT=<output>` | Treat this sway output as a TV too (besides `DP-*`, `HDMI-*`): a second headless output in a test sway ([DISPLAY-OUT.md](../DISPLAY-OUT.md)) |
| `POCKETUI_TOUCH_ROTATE=0\|90\|180\|270` | Turn the touch mapping if taps land in the wrong place |
| `POCKETUI_SUSPEND_CMD`, `POCKETUI_POWEROFF_CMD`, `POCKETUI_REBOOT_CMD`, `POCKETUI_BOOTLOADER_CMD` | Replace the commands sleep and the power menu run, for tests ([SLEEP.md](../SLEEP.md)) |
| `POCKETUI_NEW_IN=<version>` | Under `POCKETUI_MOCK` only: the version "New in this version" compares with instead of the build's, so a journey sees the cards of an update (`src/app_tour.rs`; the Retro card's journey in `tests/user_journeys.rs`) |
| `POCKETUI_UPDATE_HINT=<version>` | Under `POCKETUI_MOCK` only: stands in for an update list naming that pocket-ui version, so the shelf's once-per-version notice shows at once without a network, and the Updates page offers that version with notes (`app_update_hint::mock_manifest`; `src/app_update_hint.rs`, the journeys in `tests/user_journeys.rs`) |
| `POCKETUI_BUILD_UNIX=<seconds>` | Set by `build.rs` at build time (not by hand): when this was built, so the clock check knows a clock before it is wrong (`src/clock_check.rs`, [SYSTEM-CONTROLS.md](../SYSTEM-CONTROLS.md#a-wrong-clock)) |
| `POCKETUI_SLEEP_MINUTE_SECS=<n>` | A "minute" of the sleep timers lasts n seconds, so sleep can be tested quickly ([SLEEP.md](../SLEEP.md)) |
| `POCKETUI_SLEEP_DPMS=1` | Light sleep also switches the panel off through sway, not only the backlight |
| `POCKETUI_RTC_ALARM=<file>` | The RTC's wake alarm for switching off after sleeping, and the one disarmed before every power off and restart, instead of `/sys/class/rtc/rtc0/wakealarm` ([SLEEP.md](../SLEEP.md#l3-switch-off-after-sleeping-2026-10-25)) |
| `POCKETUI_RTC_PROC=<file>` | The stand-in for `/proc/driver/rtc` (`alarm_IRQ`, `alrm_pending`) read back after the disarm; with `POCKETUI_RTC_ALARM` set and this not, the proc file is not read ([SLEEP.md](../SLEEP.md#power-off-restart-and-the-rtc-alarm-2026-10-26-fixpower-off-rtc)) |
| `POCKETUI_NO_AUTOSUSPEND=1` | Light sleep never goes on to a suspend (as `no-autosuspend` in either `<data>` or `/storage/.config/pocket-ui` does); the low-battery guard still switches off |
| `POCKETUI_POWER_KEY=1` | Read the power key even without logind (the virtual device) |
| `POCKETUI_SAFE=1` | Start in safe mode, the rescue page, as `pocket-ui --safe` does ([ARCHITECTURE.md](../ARCHITECTURE.md#rescue-path)) |
| `POCKETUI_SAFE_REASON=<sentence>` | Why safe mode started, shown on the rescue page; set by the launcher |
| `POCKETUI_ANDROID_RENDERS=<folder>` | Not a check: `cargo test -- --ignored android_insert_renders` writes the start and the way back of the consoles with their own scene at moments along them as raw RGB files `<system>-<what>-<ms>.rgb` (1920x1080) into the folder, for a contact sheet (src/android_eject_tests.rs) |
| `POCKETUI_ANDROID_SYSTEMS=<list>` | With `POCKETUI_ANDROID_RENDERS`: the systems to render, comma-separated (`snes,gba`); all consoles with their own scene by default (src/android_eject_tests.rs) |
| `POCKETUI_ANDROID_SCREEN=<w>x<h>x<dp>` | With `POCKETUI_ANDROID_RENDERS`: the screen and its density (default `1920x1080x2.0`, the Odin 2 Mini) (src/android_eject_tests.rs) |
| `POCKETUI_ANDROID_STEP=<ms>` | With `POCKETUI_ANDROID_RENDERS`: the time between two pictures in ms (default 150; 17 is every 60 Hz frame) (src/android_eject_tests.rs) |
| `POCKETUI_ANDROID_THEME=cabinet` | With `POCKETUI_ANDROID_RENDERS`: renders in the Cabinet look instead of Shelf (src/android_eject_tests.rs) |
| `POCKETUI_FRAME_DUMP=<dir>` | Smooth videos of the real device (a screen recorder on the handheld keeps only 13-56 of the 60 pictures a second): every frame on a fixed 60 fps clock, so the UI thread's time stands at frame k's sixtieth (16.667 ms each, the UI's own frame) however long painting and writing take, into `<dir>/frames.pfd` (difference to the frame before, zlib), with `<dir>/events.log` (each press and UI sound with its frame). The games pocket-core runs meanwhile go into `<dir>/core-<pid>/`: every picture it puts on screen (frame, TV coming on and veil included) and the game's sound. `scripts/frame-dump-decode.py <dir> --mp4 out.mp4` makes one 60 fps track of both, with the presses, UI sounds and game sound by second ([`frame_dump.rs`](../../src/frame_dump.rs)). Developer only: unset, nothing runs and the clock is the system's |
| `POCKETUI_FRAME_DUMP_SCRIPT=<file>` | With `POCKETUI_FRAME_DUMP`: presses on the dump's own clock, `<frame> <word>` lines with the command FIFO's button words (`down:<word>`/`up:<word>` hold one, `wake`, `quit`) and `end <frame>`, where the dump and pocket-ui end (`app_frame_dump.rs`) |
| `POCKETUI_FRAME_FILE=<file>` | Created once the first picture is on screen, for the launcher's black screen guard; set by the launcher (`/tmp/pocket-ui.frame`) |
| `POCKETUI_NO_BOOT_MENU=1` | The power menu as on a device that can't restart into the boot menu |
| `POCKETUI_CLOCK_SHIFT=<seconds>` | Move the clock that Automatic appearance and the night light's schedule follow, to see the change at once ([DESIGN.md](../DESIGN.md), [SYSTEM-CONTROLS.md](../SYSTEM-CONTROLS.md#night-light)) |
| `POCKETUI_ROOM_TIME=HH:MM` | The time of day the Shelf look's and the Cabinet's light follows (Look → Cozy extras → Light by time of day), instead of the clock: snapshots, trying it without waiting ([DESIGN.md](../DESIGN.md#the-room-light-and-decorations-room_light-room_deco-app_room_extrasrs)) |
| `POCKETUI_ES_HOME=<dir>` | Where pocket-ui looks for EmulationStation's and ES-DE's `gamelists` and `downloaded_media` folders (`<dir>/.config/emulationstation`, `<dir>/.emulationstation`, `<dir>/ES-DE`; `/storage` by default): the virtual device and snapshots. The ROM folders' own `gamelist.xml` need no variable (`gamelist.rs`) |
| `POCKETUI_TEXT_SIZE=large\|medium\|normal` | The text size whatever Settings → Look → Text size says, for snapshots and tests (`type_scale.rs`, [DESIGN.md](../DESIGN.md#text-size)) |
| `POCKETUI_PROMPTS=nintendo\|xbox\|playstation` | The Button symbols whatever Settings → Look → Colours or Style says, for snapshots and tests (`button_prompts.rs`) |
| `POCKETUI_COLOURS=warm\|indigo-cube\|grey-brick\|green-x\|blue-station` | The Colours whatever Settings → Look → Colours or Style says, for snapshots and tests (`palette.rs`) |
| `POCKET_CHEATS_PARITY=<tree>:<pack>` | Release check on real data (`src/cheats.rs`, an ignored test): the unpacked `database/cht` folder and the `cht.pack` made from it must give the same cheats for every game. |
| `POCKETUI_FONT=barlow\|pixel\|retro` | The font style whatever settings.json's `"font"` says, for snapshots and tests (`font_style.rs`, [DESIGN.md](../DESIGN.md#font-styles-srcfont_stylers)); `retro` is the whole Retro style |
| `POCKETUI_RETRO=cocoa\|pea\|off` | Settings → Look → Style → Retro in that palette (or off) whatever settings.json says, for snapshots and tests (`retro.rs`, [RETRO-STYLE.md](../RETRO-STYLE.md)) |
| `POCKETUI_H700_VOLUME_OSD=1` | Set before start on an H700 handheld to keep the old display path for the volume pop-up instead of the lighter `mako_osd.rs` one; for A/B measurements only ([H700-VOLUME.md](../H700-VOLUME.md)) |
| `POCKETUI_RETRO_DIGITS=lcd\|pixelify\|plain` | Retro's digits: the LCD set (default), Pixelify's own or Barlow's, for comparing snapshots (`retro::Digits`, [RETRO-STYLE.md](../RETRO-STYLE.md)) |
| `POCKETUI_BLIP_DIR=<dir>` | The ignored test `retro_blip_table` writes the Retro menu blips as WAVs there and prints their level table (`ui_sound.rs`) |
| `POCKETUI_VISIBLE_TEXT=<file>` | A rendered snapshot writes the strings painted into its final canvas, after clipping and dialog coverage; excludes width measurements and off-screen strings; a running pocket-ui writes it for every frame it draws. Used by `tests/user_journeys.rs` and the Wi-Fi bench ([WIFI-BENCH.md](../WIFI-BENCH.md)) to assert what the player can read. This is a text transcript, not OCR; images and text painted into a separately composited layer are outside it. |
| `POCKETUI_LAYOUT_AUDIT=<file>` | A rendered snapshot writes what the text-box record cannot see because nothing overlaps (`text::boxes::layout_audit`): `N` and a column of three or more one- or two-character strings, `S` and a string painted under 14 px. Read by `scripts/size-sweep.sh`. |
| `POCKETUI_SYNC_ROOT=<dir>` | Only with `POCKETUI_MOCK`: Syncthing's `config.xml` and pocket-ui's `held` marker are read under `<dir>` instead of `/storage` (`sync::on_card`), for the navigation explorer's made-up Syncthing with paused folders ([SYNC-OWNERSHIP.md](../SYNC-OWNERSHIP.md)). Unset: `/storage` |
| `POCKETUI_NAV_PROBE=<file>` | Every frame's offer to the D-pad, for `tests/nav_explorer.rs`: each string and mark (`‹nav:row›`, `‹nav:focus›`, `‹nav:hint›`) with its place, then, under `--test-driver`, where the App is, its touch list and `@end` (`src/nav_probe.rs`, `src/app_nav_probe.rs`, [The navigation explorer](tests.md#the-navigation-explorer)). Unset: nothing recorded |
| `POCKETUI_WHERE_RECORD=<file>` | `--snapshot` writes which painter painted the state and where the App says it is, for `tests/where_states.rs` (`src/app_where.rs`). Unset: nothing written |
| `POCKETUI_SNAPSHOT_CACHE_DIR=<dir>` | Test-only fresh run directory for shared static layout renders (`tests/common/snapshot.rs`); never set by the player. Interactive and timing tests bypass it |
| `POCKETUI_SNAPSHOT_CACHE_DISABLE=1` | Execute every static request through the same staged fixture without reusing outputs, for controlled A/B measurements |
| `POCKETUI_NAV_REPORT=<dir>` | `tests/nav_explorer.rs` writes each part's pages, notes and failures to `<dir>/<profile>-<part>.txt` |
| `POCKETUI_TITLE_BLOCKS=<file>` | A snapshot writes the lines of the big game name it painted (`title_block`), one per line: size, `main` or `sub`, baseline, text. `tests/long_titles.rs` reads it to check that the name is whole |
| `POCKETUI_SPINE_LINES=<file>` | Every spine made (Shelf and Cabinet looks) appends a line to the file: title, size, its lines (two joined by a bar), the case (`media::note_spine`). `tests/spine_names.rs` reads it |
| `POCKETUI_TEXT_BOXES=<file>` | A snapshot writes to `<file>` where its text collides: `O` and two strings whose ink runs into each other (or into a hint bar, `‹hints›`; a page may record a row's band as `‹row›`, which only a hint bar may lie on), `T` and two strings on one line closer than 3 px, `E` and a string cut by the left or right edge of what it was painted into, `V` and a string the picture's own top or bottom edge cuts (a heading half off the screen). Empty when there is nothing; `tests/text_size.rs` reads it (`text::boxes`) |
| `POCKETUI_CJK_FONT=<file>\|off` | The font for the Japanese, Chinese and Korean letters Barlow lacks, instead of ROCKNIX's `/usr/share/fonts/truetype/noto-cjk/NotoSansCJKsc-Regular.otf`; `off` (or empty): none, a letter shows as "?" (`text_fallback.rs`, [DESIGN.md](../DESIGN.md#japanese-chinese-and-korean-names)). For snapshots on a computer without it: the file of an unpacked ROCKNIX image |
| `POCKETUI_TEST_CJK_FONT=<file>` | The Noto Sans CJK the tests use (`src/text.rs`, `tests/cjk_snapshots.rs`), instead of `/tmp/rocknix-root-20261001/usr/share/fonts/truetype/noto-cjk/NotoSansCJKsc-Regular.otf`; without either the tests that need it print `SKIPPED` and pass |
| `POCKETUI_LANG=de\|en` | The interface in this language whatever Settings → System → Language says, for snapshots and tests ([Translations](translations.md#translations)) |
| `POCKETUI_I18N_MISSES=<file>` | Append to `<file>`, each once: every string painted without a German line (`M<TAB>…`), a part of a translation left in English (`P`), every translation made (`O`), and by its English a string `Text::fit` cut short (`C`) or drawn past the right edge (`X`); `tests/i18n_snapshots.rs` and `scripts/i18n-shots.sh` read it |
| `POCKETUI_FRAME_LOG=1` | Log "press to frame" latency for every button press and the paint time of animation frames by kind (`app_motion.rs`), and the Cabinet look's frame times |
| `POCKET_LAUNCH_HOLD_SKIP=<path>` | Set by pocket-ui for pocket-core: once the file is there the launch animation was skipped and the window stops waiting (`startup.rs`) |
| `POCKETUI_RIG_ASSETS=<dir>` | Read a launch rig from `<dir>/<system>/` instead of the compiled-in files, for working on one without a build (`insert_rig.rs`) |
| `POCKETUI_MOTION_SCALE=<n>` | Slow the menu animations down n times (`motion.rs`), for frame sequences in the virtual device |
| `POCKETUI_SOUND_DEBUG=1`, `POCKETUI_SOUND_QUANTUM=<frames>` | Menu sounds: log how long each took to start; PipeWire's quantum on the shelf (`0` keeps ROCKNIX's) ([HOST.md](../HOST.md)) |
| `POCKETUI_H700_SPEAKER_MAX_RAW=<raw>` | Process-only lab ceiling, 1..the compiled speaker maximum; can only lower it. Raw 1 still needs an independently verified digital-zero source and running PCM clock for silent tests ([H700-VOLUME.md](../H700-VOLUME.md)) |
| `POCKETUI_FAST_SWITCH=0` | No hand-over: the game switcher waits for the old game's process to end before it starts the next one, and Quit waits for it before the shelf comes back, as before ([ARCHITECTURE.md](../ARCHITECTURE.md#switching-games)); for comparing switch and quit times |
| `POCKETUI_SWAY_IPC=0` | Start `swaymsg` for every focus change and window lookup instead of speaking sway's IPC socket (`SWAYSOCK`) directly (`control.rs`); for comparing on the device |
| `POCKETUI_FREE_BYTES=<n>` | With `POCKETUI_MOCK`: the free space on the card the low-space notice (`app_storage`) reads; without it a mocked run never warns, whatever the build machine's disk has left |
| `POCKETUI_NO_HISTORY=1` | Play history records nothing of this run (no writer thread, no migration; what is recorded stays). Set by `scripts/latency.py`, so measured game starts never land in the player's history ([HISTORY.md](../HISTORY.md#recording)) |
| `POCKETUI_LIBRARY_CACHE=0` | No game list cache: every start reads es_systems.cfg and every ROM folder before the first frame, as before 2026-10-24 (`library_cache.rs`, [Start time](start-time.md#start-time)); for comparing cold starts on the device |
| `POCKETUI_STARTUP_PROBE=1` | Load libwayland-client and libxkbcommon before the event loop as a start step of their own ("display libs" in the `first frame after` line), so their loading is told from the connection to sway ([LATENCY.md](../LATENCY.md#cold-start)) |
| `POCKETUI_TRACE=<file>` | Append latency trace lines (input, frame and its paint time and size, game start and end, the steps of a Quit and of the pause menu opening) to `<file>`; pocket-core inherits it. Audio events include `audio-push` (input/output/queued frames), `audio-write` (remaining queue and writer phase), `audio-prime` (priming write duration), `audio-pause` and `audio-underrun` for cadence/resume diagnosis. Off when unset. Set by `scripts/latency.sh`, see [LATENCY.md](../LATENCY.md) |
| `POCKETUI_SNAPSHOT_POCKET_CORE=1` | Snapshots describe games as started on pocket-core although it isn't installed (set by `--snapshot curator:…` itself) |
| `POCKET_UI_HOST_ALL=1` | Also route systems that are wired for pocket-core but not verified |
| `POCKET_HAPTIC_TICK=<ms>,<strength>` | The menu haptics' tick ([HOST.md](../HOST.md)) |
| `POCKETUI_VERSION=<version>` | At build time: the version `pocket-ui --version` and the Updates page report (see [Building](building.md#building)) |
| `POCKETUI_COMMIT=<sha>` | At build time: the commit the report's `system.txt` names (the release scripts set it; a build of your own says "not recorded") |
| `POCKETUI_FAN_ROOT=<dir>` | Smooth fan and the Performance & fan readout read and write the fan and thermal files below `<dir>/sys` instead of `/sys`: mock files for the virtual device ([FAN-CURVE.md](../FAN-CURVE.md#the-virtual-device)); the launcher's hand-back honours it too |
| `POCKETUI_FAN_MIN_PWM=<0-255>` | Smooth fan's lowest running PWM for this fan, once measured on the device (40 by default; [FAN-CURVE.md](../FAN-CURVE.md#on-the-device)) |

**pocket-core**

| Variable | Effect |
|---|---|
| `POCKET_SAVE_REFUSE=1` | Only the host tests’ stub core: refuse serialization so save-safety regressions can check that earlier saves survive; not a shipped-core option. |
| `POCKET_SAVE_SAFETY_CHILD=<dir>` | Internal save-safety/core-contract test child root: uses the scratch stub core and redirects runtime notices into that folder; set by the host test harness, not a player option. |
| `POCKET_TEST_SAVE_DELAY_MS=<ms>` | For a device test of a slow card: pocket-core's exit save takes this much longer (`quit::still_saving`: nothing stops a game while it writes). |
| `POCKET_TEST_CORE_STATE_LIMIT_S=<seconds>` | For a device test of a PSP game that hangs while saving: pocket-core waits this long for the exit save before it gives up and ends the game itself, instead of its built-in limit (`CORE_STATE_LIMIT`, `src/bin/pocket_core.rs`) |
| `POCKET_CORE_NO_SHORTCUTS=1` | Set by pocket-ui for a Kids Mode game: pocket-core's HOME shortcuts are all off, also after applying the game's settings ([KIDS-MODE.md](../KIDS-MODE.md)) |
| `POCKET_CORE_NO_OVER_GAME=1` | Set by pocket-ui for a Easy game: no FPS, CPU or Pixel Path line over the game and no HOME + Y, whatever Over the game says; the setting itself is untouched ([BIG-SHELF.md](../BIG-SHELF.md)) |
| `POCKET_CORE_TRACE=1` | Print every libretro environment call a core makes that pocket-core does not answer; a core's log has no 2 MB limit then |
| `POCKET_CORE_TEST=1` | The command FIFO also takes test input (`press A 150`, `axis lx 0.8`), `peek <offset>` and `achievement-demo` ([HOST.md](../HOST.md)) |
| `POCKET_CORE_AUDIO=alsa\|pulse` | The sound output (ALSA through PipeWire's plugin first, PulseAudio's simple API as the fallback) |
| `POCKET_CORE_AUDIO_MS=<ms>`, `POCKET_CORE_ALSA_US=<µs>`, `POCKET_CORE_ALSA_DEVICE=<name>` | The sound queue's target, the ALSA buffer, the ALSA device (`pipewire`) |
| `POCKET_CORE_AUDIO_PW=1` | Sample PipeWire's per-node errors from the stream's open for `audio session`/`audio sample` (benches; otherwise the first `audio stats` starts it) |
| `POCKET_CORE_RESAMPLER=linear` | The old linear sample-rate conversion instead of the windowed sinc (`src/resample.rs`); emergency only |
| `POCKET_CORE_QUANTUM=<frames>` | PipeWire's quantum while a game runs (`0` keeps ROCKNIX's; default 1024 on H700, 256 elsewhere) ([HOST.md](../HOST.md)) |
| `POCKET_CORE_QUANTUM_OWNER=pocket-ui` | Set by pocket-ui for the games it starts: pocket-core leaves putting ROCKNIX's quantum back at its end to pocket-ui (`ui_sound::game_ended`), which does it in order with the shelf's own and never under a sound ([HOST.md](../HOST.md)) |
| `POCKET_CORE_PREFERRED_HW=<context type>` | The OpenGL context a hardware-rendered core is told to prefer, for experiments |
| `POCKET_CORE_GPU_TIMER=1` | GL timer queries around the picture pass (scaling, sharpness, effect): pocket-core logs `gpu picture: … ms mean, median, p99` every 600 pictures ([HOST.md](../HOST.md#picture-effects-2026-10-24)) |
| `POCKET_REQUIRE_GL=1` | `tests/picture_shaders.rs` fails instead of skipping when there is no headless GLES 3 context |
| `POCKET_CORE_REWIND_CHECK=1` | Compare every rewound frame with the frame shown the first time ([HOST.md](../HOST.md)) |
| `POCKETUI_FRAME_CACHE=<dir>\|off` | Where drawn screen frames are kept between game starts (`$XDG_RUNTIME_DIR/pocket-ui-frames`, else `/tmp/pocket-ui-frames-<uid>`, both in RAM); `off` draws every frame anew ([FRAMES.md](../FRAMES.md#cache)) |
| `POCKET_CHEATS_DB=<dir>` | Read the cheat database from another folder |
| `POCKET_RUMBLE_DEVICE=/dev/input/eventN` | The device that rumbles, instead of the one found |

**Set by pocket-ui for pocket-core** (not for setting by hand)

| Variable | Effect |
|---|---|
| `POCKET_LAUNCHED_AT` | The moment the game was asked for (`CLOCK_MONOTONIC`, ms), so pocket-core logs its start against it ([Start time](start-time.md#start-time)) |
| `POCKET_LAUNCH_HOLD_UNTIL` | Cozy animations and "Where was I?": the `CLOCK_MONOTONIC` ms until which pocket-core keeps its window back (draws nothing, silent) while the launch animation plays or the card is read; a button ends it, an animation deadline more than 2 s away is ignored; `HOLD_STILL=1` waits for a button (`startup::held`) |
| `POCKET_LAUNCH_HOLD_STILL=1` | Set by pocket-ui with "Where was I?"'s hold: the game runs no frame during the hold once its moment is loaded (`startup::stands_still`) |
| `POCKET_LAUNCH_HOLD_READY` | Optional reminder-ready file: pocket-ui creates it when the decoded launch picture arrives; if it is still missing when pocket-core starts, the game plays immediately rather than waiting for an invisible card |
| `POCKET_SKIP_RESUME` | Set by pocket-ui (`frozen::at_launch`): pocket-core starts fresh and moves the auto state aside for Load: it was made for another version of the file |
| `POCKET_START_NOTE` | Set by pocket-ui: a line pocket-core shows over the game's first moments (`frozen::at_launch`) |
| `POCKET_LAUNCH_CRT` | `1`: pocket-core plays the old-TV power-on (`crt_on`) over the game's first 300 ms after the hold; `lcd`: a handheld's game (no TV, `eject::Screen::Lcd`), its display lights up instead, without the tube's sound; set by pocket-ui only for Cozy with Motion Full. pocket-core prints `crt-on` (and traces it) when it starts: the hook for the sound |
| `POCKET_LAUNCH_CRT_SILENT` | `1`: the old-TV power-on without its sound; set by pocket-ui beside `POCKET_LAUNCH_CRT` when Settings → Sound & feel → Case sounds is off (`crt_on::silent`) |
| `POCKET_HANDOFF_FD` | The pipe pocket-core writes `saved` into for the hand-over (fast switch, Quit to the shelf) and `paused` when it has taken `pause` ([ARCHITECTURE.md](../ARCHITECTURE.md#switching-games)) |
| `POCKETUI_FAN_REQUEST` | Smooth fan drives the fan: pocket-core writes the game's fan choice into this file (`/tmp/pocket-ui-fan.game`) instead of changing the trips ([FAN-CURVE.md](../FAN-CURVE.md)) |

**Scripts**

| Variable | Effect |
|---|---|
| `POCKETUI_HOST` | SSH target of the scripts (`root@<device-ip>`); alternatively `POCKETUI_HOST=...` in `scripts/local.env` |
| `VDEV_NAME`, `VDEV_ROMS` | The virtual device's container name and ROM folder ([VIRTUAL-DEVICE.md](../VIRTUAL-DEVICE.md)) |
| `VDEV_DOCKER_ARGS` | More `docker run` arguments for `vdev.sh up` (the Wi-Fi bench's `--network none`) |
| `WIFI_BENCH_HOST`, `WIFI_BENCH_RESULTS` | Where `scripts/wifi-bench/run.sh` runs the Wi-Fi bench (an SSH host with colima, or `local`; also from `scripts/local.env`) and where its results go ([WIFI-BENCH.md](../WIFI-BENCH.md)) |
| `POCKETUI_RETRO_TRACKING=wide` | Departure Mono's letters as far apart as the font sets them (two font pixels, not one), for comparing pictures (`font_style::departure_wide`) |
| `POCKETUI_RETRO_LETTERS=departure\|pixelify` | Retro's letters (`font_style::RetroLetters`) whatever settings.json's `"retro_letters"` says: Departure Mono (the default) or Pixelify Sans, for snapshots and tests ([RETRO-STYLE.md](../RETRO-STYLE.md)) |
