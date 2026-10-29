# System controls: volume, brightness, colour, quick settings, time

NextUI-level controls on the shelf and in games. Everything goes through ROCKNIX's own mechanisms (read from
the ROCKNIX 20261001 image, `docs/VIRTUAL-DEVICE.md`), behind one seam (`system_ctl::Hw`) with a mock, so the
virtual device shows a working panel although it has no backlight, radios, battery or systemd.

> **Status** (ledger: [STATUS.md](STATUS.md)): built and tested in the virtual device. On the Mini V2 the quick
> settings and the volume row were seen working (2026-10-24); the "Only checkable on the device" list at the end has
> otherwise not been worked through (see [DEVICE-TESTS.md](DEVICE-TESTS.md)): not the night light's colours, the
> brightness keys in a game, Sleep, Power off and Restart, or a real Bluetooth controller. Settings opens with START; the
> quick settings (HOME on the shelf) have rows for brightness, volume, Wi-Fi, Bluetooth, Performance, Fan and Power….

## Keys

| Keys | Where | Does | Done by |
|---|---|---|---|
| Vol+ / Vol- | everywhere | volume, 5 % steps, repeating | ROCKNIX (`input_sense` calls `volume up\|down`); pocket-ui only draws the OSD |
| Start + Vol+/- | shelf, quick settings, pocket-core games | brightness, 5 % steps, never below 5 % | pocket-ui |
| Select + Vol+/- | same | colour temperature, 10 steps each way | pocket-ui |
| HOME + Vol+/- | everywhere | brightness | ROCKNIX (modifier A of `input_sense`); pocket-ui draws the OSD |
| HOME tap | shelf (not in games: there it is the pause menu) | quick settings | pocket-ui |
| pause menu → Quick settings | any game with a pause menu | quick settings over the paused game; B/HOME back to the menu | pocket-ui |

The OSD is a dark pill at the top centre (`src/osd.rs`): name, level bar, value. On the shelf pocket-ui draws it;
over a pocket-core game it sends `osd Name|value|fraction|centred` into pocket-core's command FIFO and the overlay
(`src/overlay.rs`) draws the same pill as a texture. RetroArch and the standalone emulators get no OSD (the
changes still apply). With no game of its own running, pocket-ui still offers the line to pocket-core's FIFO: a
pocket-core it doesn't know of (started over SSH) covers the shelf, and the FIFO refuses at once when nobody reads
it. A line a known pocket-core game didn't take is logged once (`osd: pocket-core didn't take …`).
ROCKNIX draws its own box for the same keys (`input_sense` runs `mako-notify "Volume: 25%"`: black, monospace, top centre,
over everything). `mako-notify` skips it only when EmulationStation is in front, so it used to sit on top of our display.
`src/mako_osd.rs` hides it with a mako mode: once, a block between `# BEGIN pocket-ui` and `# END pocket-ui` is appended to
`/storage/.config/mako/config` (`[mode=pocket-ui app-name=mako-notify]` with `invisible=1`; pocket-ui writes ROCKNIX's default with the block when the file is missing on a fresh card; `mako-notify` otherwise
only creates it and never rewrites it), then `makoctl mode -a pocket-ui` while pocket-ui or a pocket-core game is in front,
`-r` while RetroArch, a standalone emulator or an app is (there we draw nothing, so ROCKNIX's box stays the feedback). The mode
lives in the daemon: if mako restarts it is off until the next game start or end. "Screenshot Saved" is hidden along with it
while the mode is on. `deploy/package/uninstall.sh` removes the block and the mode.
A volume key also ends a HOME tap in a game: HOME + Vol (ROCKNIX's brightness) used to open the pause menu when
HOME was let go.

### How the volume keys reach us

`input.service` runs `/usr/bin/input_sense`: one `evtest` per input device. Vol+/Vol- alone call `volume up|down`
(`pactl set-sink-volume @DEFAULT_SINK@`, then `set_setting audio.volume`). With a function modifier held they do
something else; on SM8250 modifier A is HOME and modifier B is START (`quirks/platforms/SM8250/070-modifiers`):

| combo | ROCKNIX default |
|---|---|
| HOME + Vol | `brightness up\|down` (5 %, floor 5 %, stores `display.brightness`) |
| START + Vol | `ledcontrol` / `ledcontrol poweroff` |
| HOME + START + Vol | `wifictl enable\|disable` |

pocket-ui reads the same key events (evdev, no grab). That is the whole trick: it never competes for the keys.
- **Plain Vol and HOME + Vol:** observe. While a key is down and 450 ms after, the volume is read every 90 ms
  (`pactl get-sink-volume`) and the OSD follows.
- **START + Vol and SELECT + Vol:** ours, but `input_sense` would still react (LED toggle for START, a volume
  step for SELECT). `src/fn_keys.rs` therefore writes three keys into `system.cfg` once (as ROCKNIX's `set_setting` does, under its lock:
  `src/system_cfg.rs`, see [ARCHITECTURE.md](ARCHITECTURE.md#rocknixs-systemcfg)):

  | key | value |
  |---|---|
  | `key.function.b` | `BTN_S[TE][AL]*` (a glob: matches BTN_START and BTN_SELECT, not BTN_SOUTH) |
  | `key.function.b.up` | `true` |
  | `key.function.b.down` | `true` |

  and restarts `input.service`, because `input_sense` reads its settings only at start. Existing values are never
  overwritten. To undo, delete the three lines from `/storage/.config/system/configs/system.cfg` and
  `systemctl restart input`.
- Without `input_sense` (the virtual device) pocket-ui changes the volume itself, so the same binary can be tested.
- A key reported by two input devices counts once.

## Quick settings (HOME on the shelf)

Brightness, Volume, Wi-Fi, Bluetooth; Performance, Fan; Power…. Left/Right change sliders and the two choice rows
(repeating; the choice rows stop at their ends), A switches, goes on to the next choice (wrapping) or opens. The
status corner sits top right in a pill, as on Settings: time, Wi-Fi symbol while connected, battery (from the
`status_corner` thread's memory, no read on the UI thread). Touch: `Sys::hits` (filled by `quick_paint`, also for the power menu), routed by
`app_touch::sys_touch` while the panel or the power menu is up.

Nothing in the panel looks at the system on the UI thread. One worker (`system_ctl::Worker`) reads the backlight,
volume and mute (`pactl`), Wi-Fi (rfkill), Bluetooth (`systemctl is-active`) and, once, the RAM boot and the boot
menu (`Sys::poll`, `Polled`): 1.5 s after the start (`WARM_AFTER`, after the first frames), at every open and every
2 s while it is open; the rows show what it found last. Before its first answer every row is there with "…"; a
Left/Right or Vol step then is taken by the worker from the real level and its OSD follows the answer (`pending_osd`),
and A on a radio asks the worker too. `quick::tests::the_panel_never_looks_at_the_system_on_the_ui_thread` keeps it so.

| Row | ROCKNIX mechanism |
|---|---|
| Brightness | `/sys/class/backlight/*/brightness` for the live value (read by the worker, also while HOME + Vol is watched); `brightness set N` (stores `display.brightness`) once the level has rested 700 ms |
| Volume | `volume N` (pactl + `audio.volume`), newest value wins; the mute state is read with the volume on the poll thread (`pactl get-sink-mute`), and a muted output shows "5 %, muted" ("Muted" at 0) |
| Volume's note | Where the sound goes, beside the bar: `Hw::sound` (`audio_route::sound_now`: the default sink from `pw-metadata`, else `pactl get-default-sink`; a `bluez_output.<addr>.1` sink's device name from `bluetoothctl info`) read by the poll thread with the volume, so the panel shows "…" first and never waits. The sink name decides: `HiFi__Headphones__sink` Headphones, `bluez_output.*` Bluetooth, usb/hdmi names, any other Speaker; no answer: "Output unknown". It reports the default sink and does not switch it: ROCKNIX's SM8250 builds may not move the default when the jack is used (release 20241120's notes: "no headphone switching") |
| Wi-Fi | `net_state::NetState` (below): `Hw::set_wifi` runs `wifictl enable\|disable` (`rfkill unblock\|block wifi`), then `nmcli radio wifi on` when NetworkManager's own switch is off, and reads the radio back (`Hw::wifi_radio`: rfkill soft and hard block from `/sys/class/rfkill`, `nmcli radio wifi`, kept 8 s); `wifi.enabled` in `system.cfg` (`Hw::persist_wifi`, ROCKNIX's `set_setting` under its lock) is written only after the radio was read back in the state asked for |
| Bluetooth | `rocknix-bluetooth enable\|disable` (starts or stops bluetooth.service and the agent, `controllers.bluetooth.enabled`); state from `systemctl is-active bluetooth` |
| Battery (in the pill) | the status corner's: `/sys/class/power_supply/battery/{capacity,status}`, read by its thread |
| Performance, Fan | Settings → Playing → Performance & fan's choices (`perf_fan::Prefs`), see below |
| Power… | opens the power menu (`Sys::open_power`) over the panel; B gives the panel back. Sleep goes through `Sys::set_sleep_handler(fn)` (without one `systemctl suspend`), Restart and Power off through the shutdown handler (`systemctl poweroff\|reboot` without one), as from the power key |

**The Wi-Fi row and Settings → Wi-Fi share one picture (`src/net_state.rs`, incident of 2026-10-26).** Both used to keep a cache
each (rfkill on one side, the last scan on the other), the switch stored `wifi.enabled` whatever `wifictl` did, a scan
that failed was shown as "No networks found", and a poll that began before a switch could end after it. Now:

- `net_state::Snapshot` is the memory both read: the stored wish (`wifi.enabled`), the radio as it really is (`Radio`: On,
  or Off with its `Cause`: rfkill soft or hard block, NetworkManager's switch; Absent where there is none), a switch under
  way, the address, the saved networks, the last scan (`Scan`: Found, possibly empty, or Failed with `nmcli`'s exit code and
  last line), and a `Notice` when a switch did not work. `Sys` copies the radio into `Polled` after every change
  (`Sys::wire_net`, one subscription, copied under `polled`'s lock so a later picture always wins); the Wi-Fi page gets
  `SetupEvent::Net` and reads the snapshot (`Setup::apply_snapshot`). `Sys::net()` is handed to the page (`Setup::with_net`).
- One worker (`net_state::Lane`) does everything that changes the radio or the connection, one at a time: the switch
  (`NetState::toggle`, `set_enabled`), a scan, and the page's connect, connect saved, forget and country
  (`NetState::exclusive`). A generation (`epoch`) goes up when such an operation is asked for and again when it is done; a
  look at the system, or a scan, that began before is dropped. A scan that a newer scan replaced is dropped; a scan
  waiting behind another is replaced. A switch goes ahead of the scans that wait and stops the one under way (nmcli is
  killed, `proc::run_until`), so it never waits for a list its own result makes useless; connections are not
  interrupted, a switch asked for during one waits for it. A look that finds the radio gone on or off since the last one
  (an external block) moves the generation too, so a scan under way cannot bring its list back. A job that panics
  (`catch_unwind` in the worker) leaves the worker running and the snapshot no longer busy.
- **Connecting without colliding (hotspot incident, 2026-10-27).** The watchdog uses this same worker for its scan
  and reconnect (`NetState::reconnect`), then checks the device again after scanning. A newer player operation
  discards a queued watchdog job or stops its scan or activation (`run_until`, `-w 30`), so a slow scan cannot hold a radio switch. User connects, saved connects and reconnects all use
  `wifi_activation::prepare`: read wlan0's numeric `GENERAL.STATE`, `GENERAL.CON-UUID` and `GENERAL.REASON`, then the profile's real
  SSID by UUID (a profile label can differ). A connected target succeeds immediately; an activating target is joined
  for up to 50 s (beyond NM’s 45 s DHCP timeout), without another activation. Failure or timeout is reported without retrying that activation; its reason is preserved, so missing secrets reopen the password prompt.
  A newly typed password always disconnects an old activation first and is passed to one new connect attempt.
  A different network is disconnected first and given up to 10 s to leave. Unmanaged/unavailable/deactivating devices
  get up to 10 s to settle before a user connect; a device still unavailable is left alone. The boot/resume watchdog
  settling window ends when wlan0 settles, or after 10 s; activating, connected and unavailable states still inhibit
  reconnects afterwards. `wifi::resumed` invalidates watchdog jobs queued before suspend and restarts its offline
  timer. No changes run on the UI thread. Connection errors are logged in full and mapped to fixed, translated
  sentences (`wifi::connection_error`), including timeout, missing secrets, out of range, activation failure and an
  unknown-error fallback; raw nmcli errors are never painted. An unavailable device says "Wi-Fi isn’t ready yet",
  rather than blaming the network. Online ticks read only the IP address; device/SSID probes run only while offline.
- **A refused password, first setup on a fresh card (incident of 2026-10-27).** NetworkManager runs with iwd as its
  Wi-Fi backend and mirrors every profile into iwd's own file (`/storage/.cache/iwd/<SSID>.psk`, "Auto-generated from
  NetworkManager connection"); deleting the profile deletes the file (iwd then logs "Error loading …psk": its directory
  watch re-reads the removed file). `nmcli device wifi connect … password …` stores the typed key in the network's profile
  (a new one, or the existing one updated) whether or not the router takes it. Left there, a refused key is retried
  without end: iwd only stops auto-connecting after a refused *explicit* connect, not after its own retries
  (`network_connect_failed` sets `ask_passphrase` only in `STATION_STATE_CONNECTING`), so on the card it went through all
  six access points of a mesh every 30 s for six minutes, while the profile stayed `ACTIVE` and the page called it
  connected. iwd itself does not keep a refused key: its network settings are closed at every disconnect and read from
  the file again at the next connect, so no separate `iwctl known-networks … forget` is needed. Now:
  - `wifi::connect_with` deletes every profile of that SSID after a typed password ends in missing secrets
    (`drop_refused`), which removes iwd's file too: nothing retries in the background, the network isn't offered as
    "Saved" (where A ran "connect saved" with the refused key and collided with NetworkManager's retry: "Activation
    failed: 0 (No reason given)"), and the next password typed is the one tried. The page goes straight back to the
    keyboard with "Wrong password. Try again." above the keys. A saved profile that stops working (the router's
    password changed) is kept: one refused handshake on a weak signal must not cost a working network.
  - Only a profile whose `STATE` is `activated` (`nmcli -t -f NAME,UUID,TYPE,ACTIVE,STATE connection show`) is in use
    (`Saved::active`); `activating` is `Saved::connecting`, and the profile decides over the scan's `IN-USE` mark,
    which iwd sets from the first moment of an activation. The page says "Connecting to …" with the row
    **Connecting…**, A asks for a moment; after `setup::CONNECT_STUCK` (60 s) without the activation finishing it says
    "… isn't letting this Pocket in. Press A to type the password again." (an open network: "isn't answering. Press A to
    try again.") and the row **Not answering**. While a profile is connecting, the page's radio look every 4 s reads the
    saved list again too (`NetState::poll`), so it turns to Connected, or back, without waiting for a scan. This agrees
    with the status corner and the Remote-access/Tailscale gates, which go by the address.
  - Forget: a profile that is already gone ("cannot delete unknown connection(s)") counts as forgotten, and the
    forgotten profile leaves the shared picture at once (`NetState::forgotten`), before the scan that follows reads the
    list again, so a second Y on the old row cannot ask for it.
  Tests: `wifi::imp::activation_tests` run an nmcli stand-in that models NetworkManager's profiles, iwd's mirrored file
  and iwd's endless retry (`nm_with_iwd`): the incident's wrong-then-right sequence, a second wrong password refused at
  once, an old profile overwritten with a refused key, a double forget; `setup::tests` the Connecting/stuck states;
  `tests/user_journeys.rs` (`first_wifi_wrong_password_then_the_right_one_then_forget`) the whole first start with the
  real binary. Snapshots: `setup:wifi-connecting`, `setup:wifi-stuck`, `setup:wifi-refused`.
- **The switch** reads the radio fresh when its turn comes and goes to the opposite (a row that shows the state of two
  seconds ago can be wrong: the boot hooks change it behind everybody's back). It stores `wifi.enabled` only after the
  radio was read back in the state asked for (`Hw::set_wifi` waits up to 4 s for them to follow: rfkill is a file read every 100 ms, `nmcli radio wifi` is asked at most every 500 ms, and
  `NetState` reads once more itself). When it did not work the wish is left as it was, the row shows what the radio really
  does and a `Notice` makes the quick settings say "Couldn't turn Wi-Fi on. Try again." in the alert colour in the line under
  the rows (`Sys::wifi_failed`, in place of the row's help; a toast at the top ran into the title in German) until the next try or
  until the panel closes, and the page say so in its own line. A switch that worked but whose wish could not be stored (twice tried) is not silent
  either: a `Notice` of kind `Save` ("Wi-Fi is on, but the setting couldn't be saved yet."), and the wish is stored at the next
  look (`pending_wish`), when the notice goes.
- **The player's own choice, switched on again (decided 2026-10-27).** When the radio is blocked by the system (rfkill
  soft block: the boot and resume hooks) while `wifi.enabled` is 1, the first look switches it on once (`NetState::unblock`:
  `wifictl enable`, logged as `net: Wi-Fi is blocked while wifi.enabled is on …`), not again within 30 s of the last
  attempt, once per block. It never does so when the wish is off, after a switch off whose wish could not be stored yet,
  for a hard block, or for NetworkManager's own switch. When it does not work there is no notice: the page says "Wi-Fi is
  off. Press A to turn it on." and A tries. The reconnect watchdog (`wifi::watchdog`) does not touch the radio and
  still leaves an intentional off alone.
- **A radio that is off is not a scan.** The scan job reads the radio first; off, it searches for nothing and leaves
  `Scan::NotYet`, and the page says "Wi-Fi is off. Press A to turn it on." with a first row that does it. Only a scan that
  ran and failed is `Failed` ("Couldn't search for networks. Try again (X)."), and only one that ran and found nobody is an
  empty `Found` ("No networks found"). `wifi::scan_checked` runs `nmcli` under a 40 s timeout.
- **Retries.** When the radio comes up (a look finds it on after off, or a switch on) and somebody once asked for a list,
  a scan follows by itself. A scan that failed or found nobody is repeated once, 5 s later (`RETRY_AFTER`), until the radio
  next comes up; a scan someone asked for in between cancels the retry. The Wi-Fi page looks at the radio every 4 s
  while it is up (`Setup::keep_radio_watch`, a thread that ends with the page), so a radio released by the boot hooks
  while the page is open is noticed without a press.
- Tests: `net_state::tests` (stale results, order, retries, notices, subscribers), `system_ctl::tests` (the real
  `RealHw`/`wifi::scan_checked` against a fake sysfs and scripts for `wifictl`, `nmcli` and rfkill), `setup::tests` and
  `quick::tests` (both surfaces on one picture).

**Restart and Power off warn about the RAM boot** (`update::booted_from_ram`, `nodtbo` on the kernel command
line). When ROCKNIX was started with `fastboot boot`, a restart returns to Android and ROCKNIX only comes back
from the computer. With the dual-boot setup (`docs/INSTALL-DUAL-BOOT.md`) the command line has no `nodtbo`, so
there is no warning. The panel's Power… row explains it in red; the power menu itself has no question (it is the
question already, as from the power key).

### Performance and Fan

`src/quick_profiles.rs` (pure: `Profiles` → rows, values, notes, the footer, a step, the effects of a change, the
device probe), `src/app_quick_profiles.rs` (the App's side).

| | On the shelf | Over a paused game |
|---|---|---|
| Performance | Games: performance; note "for games" | Games: performance; note "this game too", or the game's own ("this game: Max", "this console: …", "this port: …"), "this game: Max" for a PS2/GC/Wii/3DS game on Auto, "this game: the system's" for RetroArch; a game's own in amber (`Profiles::overridden`) |
| Fan | Fan outside games; note "in menus" | Games: fan; note "this game too" or the game's own; "this game: the system's" for a non-pocket-core game with Smooth fan off |

- **Nothing read on open.** The choices are the App's (`perf_fan_sync` hands them to `Sys::set_profile_prefs`); the
  game's own choices come from what the pause menu already holds (`GameSettings::own`, `emu_settings::Menu::
  own_performance`, the port store); whether the device has cpufreq and the fan is probed once on a thread at the
  start (`quick_profiles::spawn_probe`, `caps`). `quick::tests::opening_the_panel_reads_nothing_and_is_quick` times
  a thousand opens with the rows' texts (0.5 µs each on the build machine, release).
- **A change** is memory only in `Sys` (`take_profile_change`); the App takes it in `perf_fan_sync`, keeps it as
  `perf_fan::Prefs`, and on a `system_ctl::Worker` thread saves `performance.json` (the newest save replaces a
  waiting one) and applies `quick_profiles::effects`: `set performance|fan <value>` to pocket-core's FIFO (as the
  pause menu's Options do; pocket-core's `Extras` writes sysfs or Smooth fan's request), `standalone::set_performance`
  for ARMSX2/Dolphin/Azahar (`power::Power::set_standalone`). Smooth fan's plan follows the choices in memory (a
  port without its own fan now follows Games: fan live). A port's performance is set once at its start: a change
  counts from its next start. RetroArch: no performance path.
- **A game's own choice wins** and is never changed from here: the toast "Default for games: Battery. This game keeps
  Max" says what happened.
- **Missing data:** no fan → no Fan row; no cpufreq → Performance shows "Not available here" and doesn't move.

## Settings > Bluetooth

Pairing controllers (user side: docs/user/bluetooth.md). `src/bluetooth.rs` (parsing, plain-language
errors, the `Bluetooth` seam with `Bluetoothctl` and `MockBluetooth`, `pair_trust_connect`), `src/bluetooth_page.rs`
(the page as a state machine: `press` returns an `Action`, threads answer with an `Event`; no I/O, all tested),
`src/app_bluetooth.rs` (worker threads, poller, painting, `--snapshot bluetooth[:off|:empty|:search|:found|:pairing|:connected|:error|:forget]`).

How ROCKNIX does Bluetooth (read in the ROCKNIX tree, `packages/network/bluez`, `packages/rocknix/sources/scripts`):
BlueZ 5.83 with `storagedir=/storage/.cache/bluetooth`, `AutoEnable=true`, `JustWorksRepairing=always`,
`ClassicBondedOnly=false`. `rocknix-bluetooth enable|disable` starts or stops `bluetooth.service` and
`bluetooth-agent.service` and stores `controllers.bluetooth.enabled` (the switch here is the quick settings' one,
`Hw::set_bluetooth`). `rocknix-bluetooth-agent` registers as the default agent with NoInputNoOutput, accepts every
request and answers a PIN request with 0000; its own pairing acts only on an address in `/run/bt_device`
(EmulationStation), so it never races us. `bluetooth.service` restores `/storage/roms/backups/bluetooth.tar` before
it starts and saves it after it stops, which happens on every deep sleep (ROCKNIX's `sleep.sh`).

| Step | Command (all `bluetoothctl`, non-interactive, each under its own timeout) |
|---|---|
| Page open | every 2 s: `systemctl is-active bluetooth` (via `Hw::bluetooth`); if on, `devices`, `devices Paired`, `devices Connected`, `devices Trusted` (BlueZ ≥ 5.66), and `info <mac>` once per new device (icon, class, appearance; at most 12 per look, nameless ones again after 10 s) |
| Search | `show` until `Powered: yes` (`power on` if not, up to 10 s after switching on), then `--timeout 15 scan on`; stopped early by killing it (`proc::run_until`), which ends the discovery of that client |
| A on a found device | `pair` (40 s), `trust` (10 s), `connect` (25 s, three tries 1.5 s apart; connected by itself in between counts). `AlreadyExists` is fine; "not available" (BlueZ drops unpaired devices 30 s after a search) does an 8 s search and one more `pair` |
| A on a paired device | `connect` or `disconnect` |
| X, X on a paired device | `remove`, then `rocknix-bluetooth save`, or the device would come back from the backup at the next Bluetooth start |

Non-interactive `bluetoothctl` registers no agent, so ROCKNIX's agent answers the pairing; with `--timeout` a command
only ends at the timeout, so only the search uses it. A dead `bluetoothd` makes `bluetoothctl` wait forever: every
call is bounded (`proc::run_with_timeout`) and runs on a worker thread. Errors are BlueZ names
(`org.bluez.Error.AuthenticationFailed`, `ConnectionAttemptFailed`, `br-connection-page-timeout`, ...);
`bluetooth::explain` turns them into what to do ("Put the device in pairing mode first").

The list puts controllers first (icon `input-gaming`, Class of Device peripheral/joystick or gamepad, LE appearance
0x03C3/0x03C4), paired before found; found devices without a name are counted, not listed.

**After it connects** the controller is a new `/dev/input/event*` node. `pad::linux::supervise` looks at
`/dev/input` once a second (`pad::event_nodes`, `Supervisor::tick`) and opens every new node with `BTN_SOUTH`,
`KEY_POWER`, `KEY_VOLUMEUP` or `KEY_VOLUMEDOWN` (on the Mini V2 Vol- is its own node, `pm8941_resin`), so the menus take it without a restart; a node that goes and comes back (sleep,
reconnect) has a new identity and is opened again (tests in `pad.rs`). Menus go by position: `BTN_EAST` confirms,
`BTN_SOUTH` goes back, so on an Xbox pad B confirms.

The mock (desktop, virtual device, snapshots) is a room with a paired Xbox controller and, once searched, an 8BitDo,
a DualSense, a Pro Controller that refuses the first pairing ("not in pairing mode"), a speaker and three nameless
devices; each action takes 1.2 s.

## Fan

ROCKNIX drives the SM8250's PWM fan through the kernel: it is a cooling device of the `cluster1` thermal zone with six
`active` trips, and ROCKNIX's `fancontrol` only writes their temperatures (quiet, moderate, aggressive). pocket-ui
has two ways on top of that, both on Settings → Playing → Performance & fan:
- **Smooth fan** (default): pocket-ui writes `pwm1` itself along a curve and parks the six trips at a backstop while
  it does; hand-back to the kernel for sleep, power off, a crash or a hang. [FAN-CURVE.md](FAN-CURVE.md).
- **Smooth fan off**: the trip profiles of `power.rs` (ROCKNIX's quiet and aggressive temperatures), as
  [HOST.md](HOST.md#settings--performance--fan-srcperf_fanrs-srcperf_fan_pagers-srcapp_perf_fanrs) describes.

The `hot` and `critical` trips and the CPU and GPU cooling devices are never written by either.

## Display look

wlroots/sway on ROCKNIX offers `wlr-gamma-control-unstable-v1`: a client uploads one 16-bit ramp per channel and
sway puts it into the CRTC's GAMMA_LUT. System-wide, games included, and enough for colour temperature and
contrast. It cannot do saturation or hue (a 1D ramp per channel cannot mix channels; that needs the CRTC's CTM,
which the protocol and sway don't expose). Whether the Mini V2 can use it depends on the kernel: the `msm` DPU
driver exposes the DSPP's CTM but, as far as the mainline driver goes, no gamma LUT, so `gamma_size` is probably 0
and sway answers `failed`. That could not be checked without the device; `src/gamma.rs` asks at start-up and
logs the answer (`display look: ...` in `/var/log/pocket-ui.log`). The panel's DDIC (ch13726a) has no colour
controls in its driver, and ROCKNIX ships no helper (`display.contrast/saturation/hue` in its default
`system.cfg` are read by no script on SM8250; no `wlsunset`/`gammastep` in the image).

| Backend | When | Does | Covers |
|---|---|---|---|
| Gamma | the compositor takes ramps | temperature and contrast (from `display.json`) | everything on screen |
| Soft | otherwise | temperature only (channels can only be darkened, so no contrast) | the shelf and menus (pocket-ui multiplies its frame) and all pocket-core games (a multiply layer in the overlay); **not** RetroArch, PS2, GameCube, PortMaster |

Saturation is not offered in either. The quick settings have no rows for temperature or contrast any more; Select + Vol is the one way left to change the temperature, and a contrast in `display.json` is still applied at start. The look lives in `/storage/.config/pocket-ui/display.json`
(`{warmth, contrast, soft}`); pocket-core reads it at start and gets `look <warmth> <contrast> <night>` when it changes.
Steps are 300 K from 6500 K: warm 10 = 3500 K, cool 10 = 9500 K (Tanner Helland's black-body fit, normalised so
no channel is ever boosted).

### Night light

Settings → Look & feel → Night light ([user guide](user/night-light.md)) is a second white point on top of the
temperature, not a second colour path. `Look::night` (kelvin, 0 = off; `#[serde(skip)]`, so not in `display.json`:
it follows the clock and each program works it out itself) is multiplied into `Look::gains`, the one function that
`ramps` (Gamma), `tables`/`apply_soft` (Soft) and pocket-core's overlay tint all use. The product is normalised so the
strongest channel stays at full and nothing is ever boosted. Contrast is the ramp's slope and independent of it.

| Where | How |
|---|---|
| Choices | `<data>/night-light.json` `{mode: off\|on\|scheduled, from, until, strength}`: minutes of the day on a 30-minute grid (21:00 and 07:00 at the start), strength 1-5 = 5600, 5000, 4400, 3800, 3200 K |
| Start | `Sys::new` works out the level by the clock and applies it in the very first look: no flash of plain colours, no fade. pocket-core does the same for itself at its start (`night_light::level_now`, only when `display.json` says `soft`; pocket-ui writes `soft` once, on the worker, when the night light needs the Soft backend) |
| Thread | `night_light::Runner` (`night-light`): works out the level and the seconds to the next switch (`plan`, pure, with the zone's offset at the switch time so daylight saving is right), fades with `fade_levels` (even in mireds, eased, 15 steps over 1.5 s; one step with Motion off), then sleeps in `poll(2)` on a `CLOCK_REALTIME` timerfd armed for the switch (`TFD_TIMER_ABSTIME \| TFD_TIMER_CANCEL_ON_SET`: it also fires when the clock is set, e.g. NTP after the start, and on resume from suspend) and a socket for pokes. The wake-ups are: the switch time, a poke, the next step of a fade. Nothing else. It reads the local time under `timezone::zone_lock`, the lock `apply_process_zone` takes to write `TZ` |
| Pokes | new choices (`night_light::set_prefs`), `App::night_light_tick` when the Pocket wakes from sleep, a new time zone (`Sys::set_timezone`, the setup's zone step, `apply_process_zone`) |
| Steps | `emit(level)` becomes one `UserEvent::Night(level)`; `Sys::set_night` sets `Look::night` and calls `apply_look`, the path the temperature keys use: Gamma uploads the ramps to the compositor (a memfd of 3 x 1024 entries, one flush); Soft repaints pocket-ui's frame with the tint and sends `look <warmth> <contrast> <night>` to a running pocket-core game. Nothing is painted or computed per frame |
| Not covered | Soft: RetroArch, PS2, GameCube, Wii, 3DS, PortMaster (as for the temperature); the page says so while the backend is Soft |

The gamma table belongs to the output, not to a window, so a game's own surface keeps it as long as pocket-ui's
Wayland connection (`gamma::Gamma`) lives; it does for the whole session, games included. `POCKETUI_CLOCK_SHIFT=<s>`
moves the clock the schedule reads. Screenshots: the ramps are not in the framebuffer, so `grim` shows true colours
with Gamma; with Soft the tint is in the picture, as the temperature is.

## Time (Settings > Time)

- Zone list: ROCKNIX's own `/usr/config/system/configs/tz` (593 names, the list EmulationStation offers).
- Region list with a Search row (the gamepad keyboard; words match region or city, "vie" finds Vienna), then the
  cities of a region with their UTC offset and local time right now. The offsets come from the zone's TZif file;
  ROCKNIX ships slim files (transitions up to 1980, then a POSIX rule in the footer), so `src/timezone.rs`
  evaluates the footer rule as well (tests: Vienna/New York/Sydney, DST edges).
- Nothing of the page is read on the UI thread. It opens at once: the zone and its offset say "…" until the
  worker's first look (`poll_all` with `time`, at the open and every 2 s), Time zone > shows Search… and "Looking at
  time zones…" until the list (`Polled::zones`, the walk of the zoneinfo tree, read once) is in, and a city list's
  offsets (a file each) are read by the worker when the list opens (`Sys::want_offsets`; blank until then).
  `quick::tests::the_time_page_never_looks_at_the_system_on_the_ui_thread` keeps it so.
- Setting a zone runs ROCKNIX's `steamos-set-timezone <zone>`: `system.timezone`, `/storage/.cache/timezone`,
  `tz-data.service` (re-links `/var/run/localtime`), `timedatectl set-timezone`. pocket-ui then calls `tzset`, so
  the clock on every screen changes at once.
- Network time: `timedatectl show -p NTP -p NTPSynchronized` gives Synced / Waiting for a server / Off;
  "Network time" > A restarts systemd-timesyncd to ask again.
- Date and time: the clock set by hand (below).

### Date and time by hand

The third row of the Time page (`quick::BY_HAND_ROW`), for a Pocket without Wi-Fi: a real-user run (2026-10-26) found a
first start offline with a dead clock could only read "Turn Wi-Fi on", a dead end. A opens `TimePage::SetClock`, a
picker of year, month, day, hour and minute (`manual_time::Picker`, pure and tested; painted by
`quick_paint::paint_picker`): Left/Right move between the fields, Up/Down change one (held keys repeat as on every
page), A sets, B goes back to the row. Touch: a field is `Left`/`Right` pressed as often as it is away
(`HitMap::presses`), the selected field's top and bottom halves are Up and Down, the hints B and A. The fields are the
local time of the chosen zone (libc's `localtime_r` offset, `Sys::offset_at`; the picker converts back with the
offset at the chosen moment, so summer time is right), 24 h like every clock in pocket-ui (there is no 12-hour
setting). The day is kept inside its month, leap years included; the years are 2026-2099.

- **Where it opens**: at the clock, or, when `clock_check` calls the clock wrong, at the day this version was built
  (`POCKETUI_BUILD_UNIX`) with the clock's time of day, so a Pocket at 2000 is a few presses from today. A time before
  `clock_check::earliest` is not set ("That date is before this version was made."): it would be called wrong at once.
- **How it is set** (`Hw::set_clock`, on the worker, `set-clock`): `RealHw` calls `clock_settime(CLOCK_REALTIME)`
  (pocket-ui runs as root on ROCKNIX, so it has CAP_SYS_TIME) and then writes the RTC with the `RTC_SET_TIME` ioctl on
  `/dev/rtc0`, in UTC (`manual_time::set_system_clock`, `write_rtc`). Why not `timedatectl set-time`: timedated
  refuses while NTP is on ("Automatic time synchronization is enabled"), and switching NTP off would stop network
  time from correcting the clock later. Why not `date -s` + `hwclock -w`: two processes, and BusyBox's `hwclock`
  takes the RTC as local time without `-u` (ROCKNIX has no `/etc/adjtime`); the syscall and the ioctl say exactly
  what they do. ROCKNIX 20261001's SM8250 kernel reads rtc0 as UTC at boot (`CONFIG_RTC_HCTOSYS`) and its PM8150 RTC
  keeps its offset in the PMIC's SDAM (`nvmem-cells = <&rtc_offset>`, ROCKNIX's device tree since 2025-12), so it can
  be set and the time survives a restart. An RTC that refuses (`ClockSet::ClockOnly`) leaves the clock set until the
  next start, and the toast says "Set, but only until the next restart."; a failed `clock_settime` sets nothing.
  `RealHw::set_clock` refuses on any root but `/` (a test's `with_root`), and every test goes through `MockHw`, which
  records the time (`MockState::clock_log`) and answers `MockState::clock_answer` (`ClockSet::Mock`: "Not set: this
  is a virtual device."). So no test ever changes the build computer's clock; the virtual device doesn't either.
- **Network time still wins**: systemd-timesyncd is left on. It notices the jump (a timer cancelled on a clock set),
  and once Wi-Fi reaches a server it sets the clock again; the kernel then writes the RTC every 11 minutes
  (`CONFIG_RTC_SYSTOHC`). While `NtpState::Synced` the row says "From network time" (dimmed), its line says there is
  nothing to set and A only says "Network time keeps the clock right." (the calmer choice than offering to switch
  network time off: a synced clock is right, and an off switch would be a way to break it). ROCKNIX `next` adds
  `rtc-offset.service` for RTCs that read before the build; with an RTC set here it does nothing (the RTC is not
  behind the build).
- **After a set**: a toast ("Date and time set."), the page on the row again, `Sys::clock_by_hand` (memory only), so
  `clock_check::check` no longer calls an offline clock "may be a little off"; the wrong-clock line goes because the
  clock is right; `night_light::poke` (its timerfd has `TFD_TIMER_CANCEL_ON_SET` and notices on its own too).
- **What does not move**: switching off after sleeping counts on CLOCK_BOOTTIME and arms the RTC alarm relative
  (`+N`) before each suspend, clearing it after every wake, so there is no alarm armed while the page is open; an
  alarm the RTC's jump fires early (the kernel does for an armed alarm the new time has passed) is a normal wake, not
  a switch-off (`sleep::tests::a_clock_set_by_hand_neither_switches_off_nor_sleeps`). Play time is the game's own
  monotonic seconds (`main.rs` counts on `Instant`, `play_log` stretches on its `mono`), so a jump adds none
  (`play_log::tests::a_clock_set_during_a_game_adds_no_play_time`); a session that started before the set keeps
  its old date. The suspend detector compares CLOCK_BOOTTIME with CLOCK_MONOTONIC, the idle and sleep timers run on
  `Instant`: none of them see the wall clock.
- Snapshots: `sys:time:2` (the row while synced), `sys:time-wrong[:2]` (offline, 2000), `sys:time-wrong-wifi`,
  `sys:time-set[:field]`, `sys:time-set-wrong[:field]` (opened from 2000: the build day), `sys:time-hand` (after a
  set, offline). Tests: `quick::tests::offline_with_a_wrong_clock_the_time_page_shows_a_way_forward` and the others
  beside it, `manual_time::tests`, `clock_check::tests`, `system_ctl::tests::a_clock_set_by_hand_…`.

### A wrong clock

`clock_check` decides, from the clock alone, whether it is clearly wrong: before 1 January 2026 or before the day this
version was built (`build.rs` writes `POCKETUI_BUILD_UNIX`; ignored when it is before 2026 or 20 years on, a build
computer with a broken clock; a day of slack for time zones). The Time page paints the answer in its footer in orange
(`quick_paint`, instead of the row's description), and a second, milder line, `Unsynced`, when network time hasn't set
the clock (Waiting or Off), Wi-Fi is off and the clock was not set by hand since the start. Both lines name the way
forward that works offline, the Date and time row, beside Wi-Fi (`clock_check::page_line`); the shelf's notice names
Settings › System › Time and Wi-Fi. No program runs for it: the state is the page's own poll. The shelf's
notice (`app_clock`) is only for **Wrong**, once, 45 s after the start (30 s later again while a page covers the shelf,
and no timer at all during a game), so a Pocket that is simply offline with a plausible clock is not nagged. Not
done: a pop-up when a game with achievements starts (the Time page and the shelf were judged enough for a small change).

### First start's time zone guess

`Setup` (src/setup.rs) shows a time zone step after Wi-Fi. `timezone::guess` (src/timezone.rs, pure, tested) takes the
system zone (`Hw::timezone`, used unless it is UTC), `iw reg get` (`Hw::regdomain`: the Wi-Fi chip's own domain, else the
global one, never "00"/"98"/"99"), `zone.tab` and `iso3166.tab` (`Hw::zoneinfo_file`) and the zone list the picker
offers. A country goes through a table of main zones (`COUNTRY_ZONES`, about 90 countries with one zone or one that
holds nearly everybody); any other country takes its zones from `zone.tab`: one is the guess, several open the
picker on them. Only zones on the picker's list count (ROCKNIX still has `Europe/Kiev`, so `Europe/Kyiv` is mapped
to it). Reading the domain is harmless, so `AutoHw` asks the real `iw` wherever it exists. The step opens at once saying "Looking at your
time zone…"; `iw`, the tables and the zone list are read on a thread (`setup::look_zone`) and swapped in with
`SetupEvent::Zone`, the shown cities' offsets with `SetupEvent::ZoneOffsets`. B and START work while it looks. Nothing is looked up
online. The zone is set through `Hw::set_timezone` like Settings > System > Time does, and only when it differs.

## Mock and virtual device

`AutoHw` takes the real implementation for each control the machine has (backlight dir, `pactl` + `volume`,
rfkill wlan, systemd + `rocknix-bluetooth`, power_supply, systemd + `steamos-set-timezone`) and an in-memory mock
for the rest; the start-up log lists what is mocked. On the virtual device that is brightness, Wi-Fi, Bluetooth,
battery (72 %), time zone and power; **volume is real** (PipeWire's null sink, `volume N` writes `system.cfg`).
The mock's time zone sets `TZ` for pocket-ui's own clock, so a zone change shows. Power actions are only logged;
the panel says "not on a virtual device, nothing happened". `POCKETUI_MOCK=1` forces the mock everywhere.

The virtual pad has `VOLUP` and `VOLDOWN`, so `vdev.sh pad press VOLUP`, `pad down START` etc. drive all of it.
To see the "ROCKNIX does the volume" path, a process named `input_sense` has to run
(`rocknix_handles_volume` looks in `/proc/*/comm`); then `volume 45` run by hand shows up in the OSD.

Desktop check of the layouts without any device: `pocket-ui --snapshot sys:<quick[:row]|quick-muted|power-off|restart|time|regions|zones|search|osd-volume|osd-brightness|osd-warmth> out.png`
(`POCKETUI_MOCK=1`, `POCKETUI_DATA=<scratch dir>`).

## Files

`src/system_ctl.rs` (Hw, real/mock/auto, worker), `src/quick.rs` (keys, OSD, panels, time page logic),
`src/quick_paint.rs` (drawing), `src/osd.rs`, `src/timezone.rs`, `src/manual_time.rs` (Date and time's picker, the
clock and RTC set), `src/display.rs` (look, ramps, tint),
`src/gamma.rs` (wlr-gamma-control client), `src/fn_keys.rs`, `src/mako_osd.rs`; hooks in `main.rs` (`sys_ctx`, `user_event`, `draw`,
`about_to_wait`, Settings row), `overlay.rs` + `bin/pocket_core.rs` (OSD piece, tint layer, `osd` and `look` commands).

## Only checkable on the device (test list)

1. **Start-up log** (`dev.sh log`): `system controls: mocked []` (nothing mocked on the device), a `display look:`
   line (gamma entries, or software tint and why), and on the very first start `function keys: wrote [...] and
   restarted input.service`. Then `grep key.function /storage/.config/system/configs/system.cfg` shows the three
   keys and `input_sense` has a new PID.
2. **Plain volume** on the shelf: Vol+/Vol- change the real volume (ROCKNIX), the OSD follows while held.
3. **Start + Vol**: brightness in 5 % steps with the OSD, the volume unchanged, **no LED toggling** (the old
   `ledcontrol`), nothing below 5 %. **Select + Vol**: colour temperature with the OSD, the volume unchanged, and
   releasing Select does not switch the shelf look. **HOME + Vol**: ROCKNIX's brightness, our OSD.
4. **HOME tap on the shelf**: quick settings. Real brightness, volume, battery % and "charging" with the charger
   in and out, clock. Bluetooth row: on and off (the service takes a few seconds, the row says "Switching…").
   Wi-Fi row: **switching Wi-Fi off ends the SSH session**, so test it at the device and with the screen.
5. **In a pocket-core game**: Vol, Start + Vol and Select + Vol show the pill over the picture; the warm tint is on
   the game and the pause menu's frame; back on the shelf it matches. RetroArch/PS2/GameCube: no OSD, no tint.
6. **Settings > System > Time**: pick a zone (Search "vie", then by region); the clock changes at once on the shelf and in
   the pause menu; `readlink -f /etc/localtime`, `cat /storage/.cache/timezone` and `get_setting system.timezone`
   agree; it survives a reboot of ROCKNIX; Network time shows Synced on Wi-Fi, "Sync now" works.
7. **Display**: `display look:` says gamma entries only if the compositor took a table. Then a RetroArch game is
   tinted too (there is no Contrast row any more); otherwise software tint, as above.
8. **Not tested at all**: Sleep, Power off, Restart (they run `systemctl`; Restart on a RAM boot leaves ROCKNIX,
   see above). Check the confirm questions and the red Restart line only.
9. pocket-ui idle CPU stays near 0 with the panel closed (`top`).
10. **Settings > Connections > Bluetooth with a real controller** (start-up log says `bluetooth: bluetoothctl`): Search finds it in
    pairing mode, A pairs, trusts and connects it (`bluetoothctl info <mac>`: Paired, Trusted, Connected yes), the
    menus react to it within a second (`pad: connected ...` in the log). Deep sleep and wake: it reconnects by itself
    after pressing its home button. X, X forgets it, and it stays forgotten after a deep sleep
    (`tar tf /storage/roms/backups/bluetooth.tar | grep <mac>` is empty). A controller not in pairing mode gives the
    "pairing mode first" message, not a hang.
11. **Settings > System > Time > Date and time** (Wi-Fi off). Start: `cat /proc/driver/rtc`, `hwclock -r -u`, `date -u`,
    `timedatectl show -p NTP -p NTPSynchronized` (NTP=yes). Make the clock wrong: `timedatectl set-ntp false; date -s
    2000-03-03; timedatectl set-ntp true`, restart pocket-ui: the orange line names Date and time, the shelf notice about
    45 s later. Down twice, A: the picker opens at the build day. Set today's date and the time, A: "Date and time set.",
    the line is gone, the status corner and Local time show the new time at once, `date -u` is right, `hwclock -r -u`
    (BusyBox; or `cat /sys/class/rtc/rtc0/since_epoch`) matches it in UTC, the log has `time: set by hand to …` and no
    `the RTC isn't`. Reboot without Wi-Fi: the time is still right (rtc0 read at boot). Then Wi-Fi on: Network time
    goes to Synced and `journalctl -u systemd-timesyncd` shows it set the clock (set the picker a few minutes off
    first to see the correction); the row then says "From network time", A only shows a toast. A night light on a
    schedule switches when the set time crosses its time. With Sleep & power's switch-off on (and
    `POCKETUI_SLEEP_MINUTE_SECS=5`), set the clock an hour on, then Sleep: it switches off at its own time, not at
    once; `cat /sys/class/rtc/rtc0/wakealarm` is empty while awake. A game played across a set adds its real minutes
    to History only.
