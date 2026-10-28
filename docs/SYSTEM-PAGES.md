# Tools, Wi-Fi, PC streaming, Backup & reset

The Settings pages that do what EmulationStation (ES) did on ROCKNIX, so nothing a player needs requires ES any more
(the gaps are listed in the analysis of 2026-10-23, packages 6 to 9 and 12). Each follows the page pattern of
Bluetooth and Performance & fan: a library module with the page's state, presses and events as plain tests, and an
`app_*.rs` file in the binary with the threads, the painting, the touch rows and the snapshots. Nothing of them runs
at start; every page reads what it shows on a thread after it opened (measured: each opens in one frame, see
[Speed](#speed)).

Sources read for this (ROCKNIX 20261001, the image the Mini V2 runs, unpacked in the virtual device):
`/usr/config/modules/*` and its `gamelist.xml`, `es_systems.cfg` (`tools`), `001-sync-modules`,
`/etc/profile.d/001-functions` (`set_setting`, `set_kill`, `sway_fullscreen`), `input_sense`, `wifictl`,
`/usr/lib/iw/setregdomain`, `/usr/bin/moonlight` (Moonlight Embedded 2.7.0), ES's `GuiMoonlight.cpp` and `GuiWifi.cpp`.

## Tools

Settings → Games → **Tools** (`src/tools.rs`, `tools_page.rs`, `app_tools.rs`).

**Why Settings and not Extras.** Extras is a first-start step about getting things (covers, PortMaster) and needs
Wi-Fi; tools are part of the system, next to the Store that installs more of them, and ES had them as a list of its
own. In Settings they are two presses away from anywhere.

**What is listed.** ROCKNIX's tools are the `.sh` files of `/storage/.config/modules`, the folder ES lists as its
system `tools`; ROCKNIX mirrors `/usr/config/modules` into it at every boot (`001-sync-modules`). pocket-ui reads
both folders (a file in the mirror wins), takes names and descriptions from its own table of the 41 tools of the
SM8250 image (`tools::KNOWN`) and from `gamelist.xml` for anything else, and adds the Store's installed tools
(`tools::store_tools`, from `installed.json`). Sorted: Emulator (the emulators' own menus, RetroArch's menu,
FEX's settings), Controls (Test gamepad, Calibrate gamepad = GPcal, Test touch screen), System (file manager,
terminal, scans, installers, cloud backup), Store, Unknown. pocket-ui's library still leaves the `tools` system
out (`library::NOT_GAMES`): the tools live on this page only.

**Risky tools.**

| Kind | Rule (`tools::hidden`, `KNOWN[..].warn`) | On the page |
|---|---|---|
| Updates the system or the bootloader, wipes or partitions a drive, resets the Pocket | File name contains factory, bootloader, flash, install/update rocknix, system update, reset; or the script contains `rocknix-update`, `updateabl`, `factoryreset`, `mkfs`, `of=/dev/`, `wipefs`, `sgdisk`, `sfdisk`, `fdisk `, `parted `, `/storage/.update`. `Install ROCKNIX.sh` (wipes the target drive) by name | Never listed |
| Uploads or replaces data, downloads gigabytes, deletes, lays a share over the games, rewrites the calibration | `cloud_backup`, `cloud_restore`, Install/Uninstall Steam and Heroic, Mount NFS, Remove ._ files, GPcal | Listed; A shows the warning in red, a second A starts it, B or moving keeps it closed |
| Not known to pocket-ui | Any other `.sh` | Listed as Unknown, with the warning that it runs as root |

None of the image's 41 tools matches the hidden rules by accident (`tools::tests`; RPCS3's script says
`dev_flash`, which is why "flash" is matched in names only).

**How a tool starts.** Like ES (`es_systems.cfg`: `<command>/usr/bin/foot %ROM%</command>`): `foot /bin/bash
<script>` in `/storage` (`tools::command`; the path is one argument, never a shell line). foot refuses to run
anything in the C locale ("'C' is not a UTF-8 locale, and failed to find a fallback"), and pocket-ui, unlike ES's
`es_settings`, exports no `LANG`; so a small `sh` in front compiles `en_US.UTF-8` once into `<data>/locale` with
ROCKNIX's `localedef` (under a second, as `es_settings` does) and exports `LOCPATH` and `LANG` for the tool. So scripts that print or
ask get a terminal, and scripts that open a window (the emulators' menus, the gamepad test) get it fullscreen
through pocket-ui's sway rule for every window but its own (`control::games_open_fullscreen`) and their own
`sway_fullscreen`. The scripts set up gptokeyb themselves where a program needs keyboard keys (Azahar and FEXConfig
map the pad to keys and a mouse), as on ROCKNIX; pocket-ui adds nothing. The tool runs like PC streaming
(`App::start_tool`, `spawn_app`): its own process group, `Running::emulator == "tool"`, an app (`Running::is_app`)
without pause menu, switcher or recents. pocket-ui shows "Starting <name>" until the tool's window covers it. The
Settings page steps aside while the tool runs (`settings_aside_for_app`): Settings input comes before a running
game's in the event loop, so the buttons pressed in the tool would otherwise have moved and started things on the
page underneath. It comes back when the tool ends (`settings_after_app`; the same for a PC game started from
Settings → Games → PC streaming). A tool (or a PC game) that ends with an error within 3 s is explained in red on the page
it came from (`settings_after_app`; a notice would only show over the shelf).

**Leaving a tool.** Its own Quit; ROCKNIX's kill combination L1 + SELECT + START for scripts that name their
process (`set_kill`, read by `input_sense`); or **HOME held for a second** (`App::tool_home`): pocket-ui sends
SIGTERM to the tool's process group and SIGKILL two seconds later (`quit::Escalation` with no grace). The page says
which applies (`Tool::leave`). Calibrate gamepad (GPcal) stops ROCKNIX's `inputplumber` and starts it again only when
its script ends normally; after every tool pocket-ui runs `systemctl start inputplumber` on a thread
(`tools::after_tool`, a no-op when it runs), so a stopped GPcal can't leave the Pocket without its controls.

**Store tools** run as before: `sh <script>` in `/tmp` on a thread, the last line of output shown on the page
(`tools::run_quiet`, shared with the Store's Run).

**Store tools are no longer in ROCKNIX's folder.** `001-sync-modules` runs `rsync -a --delete` into
`/storage/.config/modules` at every boot, so tools the Store put there were gone after the next start (confirmed on
the Mini V2). They now go to `<data>/tools`, and older installs are moved over or forgotten: [STORE.md](STORE.md#tools).

## Wi-Fi

Settings → Connections → **Wi-Fi** is the first start's Wi-Fi step (`src/setup.rs`, `Step::Wifi`, `HiddenName`, `Country`), the
nmcli calls are in `src/wifi.rs`. What the page shows of the radio, the saved networks, the address and the scan is the
shared picture of `src/net_state.rs`, the one the quick settings' Wi-Fi row reads too, and scan, connect, forget and
country run on its worker one at a time ([SYSTEM-CONTROLS.md](SYSTEM-CONTROLS.md#quick-settings-home-on-the-shelf)). The line
under the title (`Setup::wifi_status`), in this order: a switch that failed ("Couldn't turn Wi-Fi on. Press A to try
again."), a switch under way, an error note, **Wi-Fi is off. Press A to turn it on.** (radio off), other notes, a scan that
failed ("Couldn't search for networks. Try again (X)."), the network in use, a network NetworkManager is still bringing up
("Connecting to …", after a minute "… isn't letting this Pocket in. Press A to type the password again."), "Looking for networks…", "No networks found.
Press X to scan again." (a scan that worked and saw nobody), "Choose your network."; a new list drops the page's old
"Connected to …" notes and the address of a network that is gone is not kept.

| Row or button | What it does | ROCKNIX / ES equivalent |
|---|---|---|
| The list | Networks in reach and saved ones as one list (`wifi::merge`): the one in use first, then by signal, then saved ones out of reach by name. The line under the title: "Connected to … · signal … % · <address>". In use means the profile's `STATE` is `activated`; `activating` is **Connecting…** (after 60 s **Not answering**) | ES's network list and `wifictl list_saved` |
| A on a network that wants a password | The keyboard, then `nmcli device wifi connect <ssid> password …`. A refused password deletes the network's profiles (`drop_refused`: iwd's mirrored `.psk` goes with them) and opens the keyboard again with "Wrong password. Try again." | ES's connect |
| A on a network still connecting | "Still connecting to …. One moment."; after a minute the keyboard (an open network: connect again) | — |
| A on a saved network | `nmcli connection up uuid …` (`wifi::connect_saved`), no password. A password that no longer works opens the keyboard | `wifictl connect_saved` |
| Y, Y on a saved network | `nmcli connection delete uuid …` (`wifi::forget`); the row leaves the list at once (`NetState::forgotten`), and a profile that is already gone counts as forgotten | `wifictl forget_saved` |
| Other network… | Name, then password (empty: an open network), `nmcli device wifi connect <ssid> … hidden yes` | — |
| Wi-Fi country | Automatic or a country of zoneinfo's `iso3166.tab` (L2/R2 jump by letter): `wifi::set_country` | ES's "Wi-Fi country" |
| X | Scan again (with the saved networks and the country read first, `wifi::saving`); one scan at a time | |
| First row "Wi-Fi is off", A | Turns the radio on (`NetState::set_enabled(true)`; the stored `wifi.enabled` follows only once the radio is on) and scans. Shown instead of "This network wants a sign-in" whenever the radio is off; the list is empty then | ES's Wi-Fi switch |

**ROCKNIX's settings stay in step.** After a connection pocket-ui writes `wifi.enabled`, `wifi.ssid` and `wifi.key`
with ROCKNIX's `set_setting`, as before. A saved network's key is read back from its profile (`nmcli -s -g
802-11-wireless-security.psk`), so `wifi.key` is never left empty for a secured network; that empty key is what let
ES's `wifictl connect` rebuild a keyless profile. When the key can't be read, ROCKNIX's settings are left as they
were. Forgetting the network ROCKNIX's settings name removes `wifi.ssid` and `wifi.key` (`set_setting … default`),
so neither the watchdog nor ES brings it back and the key doesn't stay in system.cfg.

**The Wi-Fi country.** ROCKNIX keeps it in `wifi.country`, but applies it only when `wifictl` connects (`iw reg
set` in `wait_for_wifi`); the boot runs only `wifictl enable`. ROCKNIX's `iw` package has the boot-time way: a udev
rule runs `/usr/lib/iw/setregdomain` when the Wi-Fi chip appears, which sources `/storage/.cache/regdomain.conf`
(`REGDOMAIN=AT`). pocket-ui sets all three: `wifi.country` (for ES and `wifictl`), `regdomain.conf` (for the next
boot) and `iw reg set` (now). Automatic removes the setting and the file and sets the world domain `00`, so the
chip's own country (the Mini V2's chip is self-managed and reports AT) applies again. Whether a set country removes
the device log's "regulatory prevented using AP config, downgraded" has to be checked on the device.

**system.cfg.** Every Wi-Fi and PC-streaming write goes through one function, `wifi::set_rocknix_setting` (ROCKNIX's
`set_setting` through `sh`, the values as arguments). It is marked `SYSTEM.CFG HELPER`: when feat/rescue-path's
locked system.cfg writer is merged, that is the one place to switch over. The watchdog is unchanged.

## PC streaming

Settings → Games → **PC streaming** (`src/moonlight.rs`, `moonlight_page.rs`, `app_moonlight.rs`), user side in
[user/pc-streaming.md](user/pc-streaming.md).

**Pairing.** Moonlight Embedded 2.7.0 (ROCKNIX 20261001) has no option to be given a PIN: `moonlight pair [host]`
makes one up and prints "Please enter the following PIN on the target PC: 1234" (ES's `-pin` is ignored by this
build; its own PIN on screen never matched). pocket-ui runs `moonlight pair` with stdout and stderr line-buffered
through coreutils' `stdbuf` (`moonlight::pair_command`; without it the PIN line waits in the pipe until Moonlight
ends), reads the lines (`moonlight::parse_pair_line`: Searching, Connecting to <host>, the PIN, "Succesfully
paired" (sic), "Failed to pair", "Can't connect") and shows the PIN on a paper label at 200 px. Without an address
Moonlight looks for the PC itself (mDNS). B stops it (SIGTERM to its process group); after
`moonlight::PAIR_SECS` (3 minutes) it is stopped too.

**The paired PCs.** Moonlight Embedded keeps no list (the pairing lives on the PC; the client keys in
`/storage/.cache/moonlight`), so pocket-ui keeps `<data>/moonlight-pcs.json`. The PC that PC streaming on Home uses
is ROCKNIX's `moonlight.host`, which ES used too: pairing or A on a PC sets it. X twice unpairs (`moonlight unpair
<host>`); the PC leaves the list even when it did not answer, and the page says it may still know the Pocket.

**The PC's games.** After pairing, and with A on a PC: `moonlight list <host>` (`moonlight::list_apps`), then one
`.sh` per game and a `gamelist.xml` in `/storage/roms/moonlight`, as ES's "Update Moonlight games" wrote them for
Moonlight Embedded (`moonlight::write_games`). Unlike ES, names and the address are quoted for the shell: a PC on the
network can't put commands into a script the Pocket runs. Box art is not fetched. The page lists the default PC's
games (`moonlight::written_games`); A streams one (`moonlight::start_app`: the lookup first, then `moonlight stream
-platform sdl -app <name> <host>`), exactly like PC streaming on Home, which still streams Moonlight's default app.

## Backup & reset

Settings → System → **Backup & reset** (`src/backup.rs`, `backup_page.rs`, `app_backup.rs`).

| What | Files (of `<data>`) | Backup | Restore | Reset |
|---|---|---|---|---|
| Settings | `settings.json`, `sleep.json`, `sounds.json`, `haptics.json`, `leds.json`, `performance.json`, `curator.json` | yes | yes | back to the start (`settings.json` keeps the first start as done and the update and store addresses) |
| Options per console and game | `game-settings/`, `core-options/`, `cheats/` | yes | yes | removed |
| Services | `sync.json`, `tailscale.json`, `web.json` (never the paired browsers in `web/`) | yes | yes | kept (a reset never switches a service off) |
| Library notes | `library.json`, `favorites.json`, `recents.json`, `fallback.json`, `moonlight-pcs.json` | yes | yes | kept |
| Never | `retroachievements.json` (token), `share/` (password), `store/` (must match the files), `frames/`, `fonts/`, `tools/`, `covers/`, `snaps/`, `titles/`, `slots/`, `logs/`, `restore.json` and the other undo notes, `last-game.json`, `play-log.bin` (play history stays private: it is never copied to the games part of the card, which file sharing reaches), ROCKNIX's files (system.cfg included) | | | |

- A backup is a folder `/storage/roms/backup/pocket-ui/<YYYY-MM-DD_HHMM>` (next to ROCKNIX's own
  `roms/backup`, on the games part of the card: it outlives ROCKNIX's factory reset and is reachable over file
  sharing, which needs the password) with the files and `backup.json` (time, reason, app version, the list). It is
  written to a hidden folder and renamed when complete.
- A restore reads every listed file first (a damaged backup changes nothing), backs up the present state, removes
  the listed kinds, and writes the backup's files. Only names of the table are read (`backup::allowed`), so a list
  edited over the share can't write anywhere else.
- A reset backs up first (reason "Before reset"), then removes the settings and Options.
- After a restore or a reset pocket-ui starts again (`updates.restart_at`, as after an app update), so nothing it held
  in memory writes the old values back.
- System.cfg is not touched: the per-game options PlayStation 2, GameCube, Wii and 3DS keep there
  (`<platform>["<rom>"].<key>`) are neither backed up nor reset.

## Snapshots

| State | Screen |
|---|---|
| `tools[:<row>][:confirm\|:busy\|:done\|:empty\|:loading]` | Settings → Games → Tools with `tools::demo()` |
| `setup:wifi[:<keys>]` | Settings → Connections → Wi-Fi with a made-up neighbourhood and three saved networks (keys as for the other `setup:` states: `d` down, `a`, `y` forget, `x` scan) |
| `setup:wifi-off`, `-switch-failed`, `-save-failed`, `-scan-failed`, `-empty`, `-switching` | The same page with the radio off (the boot hooks blocked it), with a switch that did not work, with a scan that failed, with one that found nobody, and with the radio on its way on (`net_state::demo`); the picture stays as it is |
| `sys:quick-wifi-off`, `sys:quick-wifi-failed`, `sys:quick-wifi-save-failed` | Quick settings with the Wi-Fi row on Off, and with the line of a switch that did not work |
| `pc-streaming[:<row>][:pin\|:searching\|:address\|:unpair\|:done\|:none\|:missing]` | Settings → Games → PC streaming with two PCs and three games |
| `backup[:<row>][:reset\|:restore\|:done\|:empty]` | Settings → System → Backup & reset with three backups |

## Trying it in the virtual device

- **Tools:** the image's `/usr/config/modules` is there (no `/storage/.config/modules`: ROCKNIX's autostart doesn't
  run). Checked 2026-10-23: RetroArch's menu and Dolphin's own window open fullscreen, HOME held stops them and
  Settings → Games → Tools comes back; a migrated Store tool runs quietly and shows its line. Emulators the virtual device
  dropped (those over its size limit) end at once with the notice.
- **PC streaming:** `POCKETUI_MOONLIGHT=<script>` in `/storage/pocket-ui/dev.env` runs a stand-in that answers `pair`
  (prints the lines Moonlight Embedded prints), `list`, `unpair` and `stream`. Checked 2026-10-23 with one: search,
  PIN, paired, `moonlight.host` set, three games written (one named `It's "x"; reboot`, which stays one quoted word),
  a game streamed and the page back, unpair removing the PC and `moonlight.host`.
- **Wi-Fi:** the container has no Wi-Fi chip; nmcli calls fail and the page says so. Test on the device.
- **Backup & reset:** works as on the device. Checked 2026-10-23: back up, reset (backup first, restart with the
  launcher's restart code, defaults afterwards), restore of an older backup.

## Speed

None of the four adds work at start; the Store's migration of old tools runs on a thread and reads nothing when
there is none. Every page paints from what it has and reads the rest on a thread. In the virtual device
(`POCKETUI_FRAME_LOG=1`, 2026-10-23) every press on and into these pages reached the screen within 2.4 ms (the
slowest: opening Tools, paint 2.3 ms); first frame after start 7.8-8.3 ms as before.

## Device checklist

See the report of feat/tools-wifi; in short: Store tool survives a restart; each Tools group starts and returns;
HOME held stops a tool; GPcal gives the pad back; saved networks, forget, hidden network, Wi-Fi country after a
restart; pairing with a real PC (PIN on screen, Sunshine accepts it), games listed, a game streams, unpair; backup,
restore and reset with a restart.
