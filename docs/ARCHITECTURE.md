# pocket-ui architecture

Target: Retroid Pocket Mini V2 (SM8250 / Snapdragon 865, Adreno 650) on ROCKNIX `20261001`.

This document describes how pocket-ui is put together today. The reasoning that led here is kept at the end
(section "Background"). For building and running it, see [DEVELOPING.md](DEVELOPING.md).

## Principle: replace one service, keep everything else

pocket-ui does not replace ROCKNIX. It replaces the program that ROCKNIX's `essway` unit starts
(EmulationStation) and leaves everything underneath alone.

```
kernel ─► systemd ─► sway.service ─► essway.service ─► run-pocket-ui.sh ─► pocket-ui
                                         (drop-in)                              │
                                                                                ├─► pocket-core ─► a libretro core
                                                                                └─► runemu.sh ─► RetroArch / standalone emulators
```

- pocket-ui is one Wayland client inside sway's kiosk session. It writes no compositor, input driver or
  emulator code of its own.
- The unit keeps its name `essway`, because ROCKNIX's `runemu.sh` ties game processes to it
  (`StopPropagatedFrom=essway.service`). A drop-in file
  (`deploy/essway-pocket-ui.conf`, installed by `scripts/autostart.sh`) changes only what it starts.
  Remove that one file and ROCKNIX is as before.
- The launcher `deploy/run-pocket-ui.sh` is the unit's process. It restarts pocket-ui after a crash, goes back to an
  older version or to safe mode when that doesn't help, and cleans up what a dead pocket-ui left behind (brightness,
  CPU and fan profiles, a game's processes). While pocket-ui runs it also watches Smooth fan's heartbeat and gives the
  fan back to the kernel if it stops ([FAN-CURVE.md](FAN-CURVE.md#safety)). EmulationStation stays installed as the emergency exit: it comes only
  when the player asks for it, or when pocket-ui cannot start at all ([Rescue path](#rescue-path)). A development
  session can also start the launcher by hand (`scripts/dev.sh`).
- The launcher stands in for ROCKNIX's `start_es.sh`, so it also does what `start_es.sh` and `es_settings` do before
  the UI starts (sound driver, language, time zone files, the quirk variables for systemd, the kill chord's target, and
  on ROCKNIX main the pre-rotation and the reset of a screen scaling a killed game left), each only where the running
  system has it ([UPDATES.md](UPDATES.md#rocknixs-ui-start)).
- The system is read-only (a squashfs). Everything pocket-ui writes is under `/storage`.
- Before sway, from systemd's start, the boot screen `pocket-splash` (its own unit, `deploy/pocket-splash.service`,
  next to the drop-in in `/storage/.config/system.d`) draws on the frame buffer and fades out when sway's unit starts;
  pocket-ui's first frame after a boot fades in from black (`boot_fade`). It never holds the boot up and never
  becomes DRM master ([BOOT-SPLASH.md](BOOT-SPLASH.md)).

## The processes

| Process | Role |
|---|---|
| `pocket-ui` (`src/main.rs`) | The shelf, Settings, first-start setup, the pause menu and quick settings. Reads the gamepad (evdev) and touch screen, draws with software rendering into a Wayland surface |
| `pocket-core` (`src/bin/pocket_core.rs`) | Runs one game: loads a libretro core, draws frames with OpenGL ES, plays sound through PipeWire, reads the pad. See [HOST.md](HOST.md) |
| `runemu.sh` (ROCKNIX) | Starts RetroArch or a standalone emulator for systems pocket-core does not host |
| Standalone emulators | ARMSX2 (PlayStation 2), Dolphin (GameCube and Wii), Azahar (3DS) and others, driven through `src/standalone.rs` and `src/emu_settings.rs` ([BIG-EMULATORS.md](BIG-EMULATORS.md)) |

pocket-ui starts a game in its own process group, so pause, quit and cleanup reach everything the game starts.
While a game runs, pocket-ui stays in the background, ignores the touch screen (the game gets it) and only
reacts to the buttons that open the pause menu. pocket-core and pocket-ui talk through small files and FIFOs in
`/tmp`:

- `/tmp/pocket-ui.in` takes button names for tests (`scripts/dev.sh send`).
- `/tmp/pocket-core.cmd` takes commands for the running game: `pause`, `resume`, `save`, `load`, `slot`, `opt`,
  `ctl`, `osd`, `note`, `autosave`, `shot`, `quit` and more (header of `src/bin/pocket_core.rs`). pocket-core answers on a
  pipe of its own (`POCKET_HANDOFF_FD`): `paused` once it has taken `pause`, `saved` once a hand-over's exit save is
  on the card (`src/fast_switch.rs`).
- `/tmp/pocket-core.error` carries the reason a game could not start, which pocket-ui shows as a card.
- `/tmp/pocket-core-shot.bin` is the paused game's picture behind the pause menu (`shot <seq>`,
  [The pause menu](#the-pause-menu)).
- `/tmp/pocket-core-preview.bin` is the picture for the Picture page's live preview (`preview <w> <h> [<seq>]`,
  [FRAMES.md](FRAMES.md#live-preview-in-the-pause-menu)).
- `/tmp/pocket-core.achievements.json` is the running game's achievements for the pause menu's list
  ([HOST.md](HOST.md#retroachievements-srcachievements)).
- `/tmp/pocket-core.pixelpath.json` is pocket-core's answer to `pixelpath <seq>`, only while the pause menu's Pixel
  Path page is open ([CURATOR.md](CURATOR.md#pixel-path-as-built-featpixel-path-2026-10-23)).
- `/tmp/pocket-core.disc` is what the core's disk control says about a multi-disc game (the disc in the drive, how
  many, the game's path), for the pause menu's Disc row; written after the load, at `pause` when it changed and after
  `disc <n>` (`src/disc.rs`, [HOST.md](HOST.md#how-it-works)).
- `/tmp/pocket-core.save-trouble` says that the game's saves don't reach the card (`full` or `unwritable`): written when
  a write of the battery save or the cartridge clock fails, removed once one gets through and at the start. pocket-ui
  reads it when the pause menu opens (a line, and a question before Quit) and after the game (a notice on the shelf)
  (`src/save_trouble.rs`, `src/app_save_trouble.rs`, [HOST.md](HOST.md#when-a-save-cant-be-written)).

## Start

pocket-ui's first frame should come as fast after a power-on as after a restart. At power-on the page cache is
empty: every file pocket-ui reads (its own program included) comes from the SD card, and reads that run at the same
time wait for each other. So only what the first frame shows is read before it; everything else waits for
`App::after_first_frame` (src/main.rs), which runs once the first frame is handed to sway, or after 2 s without one.
The steps and their times are logged as `first frame after … (logs …, event loop …, library …, drawn … ms)`
(`src/startup.rs`; how to measure them cold: [LATENCY.md](LATENCY.md#cold-start)).

| Before the first frame | Why |
|---|---|
| The pads and the touch screen: opened first thing in `main`, on a thread (`start_input`); the nodes they look at and don't keep are closed on another (`pad::close_later`: a close takes the kernel 7-45 ms); their events wait in `pad::Early` until the event loop is there. The touch screen is supervised like the pads (`touch::supervise_with`, `touch::linux::supervise`): looked for until it is there (waits of 0.25 s doubling to 30 s, woken by /dev/input changing, a look skipped while the nodes are the same), opened again after a read error with a new parser; one `touch:` log line per change). `MtParser` lets go of a finger whose lift never came (a new tracking id in its slot is a new touch; one silent for `GHOST_AFTER` = 3 s while the screen reports other fingers is dropped), so a lost lift cannot hold every later finger | a press reaches a device's queue only once the device is open: a press as soon as the shelf is there must count ([LATENCY.md](LATENCY.md#first-press-after-a-start)) |
| The event loop (winit: libwayland-client, the connection to sway, libxkbcommon), the window | the frame goes there |
| `settings.json`, the display look and the night light's level by the clock (`Sys::new`: gamma ramps), sounds and haptics switches | look, appearance, screen frames, On start, hidden systems, the first input's sound |
| The type (`Text::new`, a thread) | |
| The game list: `<data>/library-cache.json` (`src/library_cache.rs`, a thread); without a usable cache es_systems.cfg and every ROM folder (`library::load`) | the shelf |
| `recents.json`, `favorites.json`, `library.json`, Curator's switches | Continue, the Favorites row, pinned stickers, the Lens and History rows |
| `fallback.json`, the fan profile, sleep's settings | a game On start continues at once needs them |

| After the first frame | How |
|---|---|
| The ROM folders read again when the list came from the cache | `app_library.rs`: a thread; a different list is swapped in like after Add games when nothing holds a game by its place (no game, game card, console page, menu, search, Curator; the other Settings pages count from the list as they open, so a game shown again in Storage is in Games and From EmulationStation at once), the selection kept on the same game or system; the cache is written only then. An equal list changes nothing. When the start began a game, after it |
| The cache written when the list was read from the folders | a thread (`fsutil::write_atomic`) |
| The state of Settings → Store, Sync, Tailscale, Web companion, Bluetooth, RetroAchievements | `startup::Later`: each read on a thread; used earlier (a game On start continues uses Sync), it is read at once |
| GameCube and Dolphin defaults in system.cfg | a thread; `launch` runs them first if they haven't run (`emu_settings::apply_start_defaults_once`) |
| `pactl` (the volume), the stick lights, the function keys (`input_sense`), the Wi-Fi watchdog (`nmcli`), Samba's shares, remote access, Store tools, the wait for Wi-Fi | `startup::after_first_frame`: each a thread |
| The quick settings' levels and radios (backlight, `pactl`, rfkill, `systemctl is-active bluetooth`) | their worker, 1.5 s after the start (`Sys::tick`, `WARM_AFTER`), so the panel opens from memory |
| The search index, Curator's index | as before (`find_tick`, `curator_tick`) |

The logind inhibitor for the power key (`systemd-inhibit`) is started on a thread at once (`sleep::InhibitorSlot`);
until it is held the power key stays logind's, as when it can't be had.

**The cache** (`library_cache.rs`): the first line is its key (cache format, pocket-ui's version, es_systems.cfg's
path, mtime and size), the second the list as JSON. The format number goes up whenever what a scan produces changes
(2 since 2026-10-24: doubled names told apart, versions off names, discs without their track left out), so an
update never shows an older scan's names. Another key, a file that doesn't parse or no file: the folders
are read before the first frame, as on a first boot. A game the cache lists but whose file is gone (deleted over the
share since the last start) is corrected by the scan after the first frame; starting it before that, or any game
whose file vanished, gives a notice ("… can't start: its file … is no longer there. The list of games is being read
again.") instead of an emulator failing on nothing, and the folders are read again. `POCKETUI_LIBRARY_CACHE=0`
switches the cache off.

## Launching a game

1. `src/library.rs` reads ROCKNIX's `es_systems.cfg` for the systems, their ROM folders, file types and the
   emulators and cores each system can run with, and lists the games on the card (at start from the cache of the
   last scan, [Start](#start)). A game whose file is gone doesn't start: pocket-ui says so and reads the list again.
   Names come from the file (`display_name`: tags and a trailing version such as "v1.02" left out, ", The" moved to
   the front); games of one system that end up with the same name get back what tells them apart (`tell_apart`:
   their own tags, else the file type, else the folder). A file name with no capital letter is cased for the shelf
   (`title_case`, after `tell_apart`; a gamelist's name is kept as it is; the cache format is 3 for it). Only `Game::name` changes: favourites, recents, play time,
   covers, `library.json` and Curator's matching go by the path or the file name, never by this name. The shown name
   reaches the recent games by path: `Recents::follow_library` (memory only, no disk) sets each entry's name to the
   library's name for that file, at start from the cached list (`library-cache.json` carries the told-apart names), and
   again whenever a list is swapped in (rescan, Add games, upload, starter pack); an entry whose file is no longer in the
   list goes back to the plain name `Recents::load` derives from the file name. So Continue, the SELECT list and the
   switcher, and a game started from them (play log, "now playing"), use the system list's name. Play history takes it
   from Curator's index (`Describe::name`), the web companion's Stats from its library (`without_paths`). A `.cue` or
   `.gdi` whose track file is missing is left out (it can't start), and the files a sheet names are part of its game.
   The text painter (`text.rs`) paints a character the font lacks from the system's Noto Sans CJK when ROCKNIX has it
   (Japanese names; `text_fallback.rs`, memory-mapped, opened once, the names' letters drawn ahead on a thread:
   [DESIGN.md](DESIGN.md#japanese-chinese-and-korean-names)), else as "?" (a letter) or not at all (a symbol), decided
   once when the glyph enters its cache.
2. On A, `launch` in `src/main.rs` picks the emulator (`App::route` → `app_emu_choice::resolve_runner` →
   `emu_choice::resolve`): the player's choice for the game, else for the console, from ROCKNIX's `system.cfg`
   (`<system>["<file>"].emulator/.core`, `<system>.emulator/.core`, the keys EmulationStation writes; read on a thread
   at start, so the launch only looks them up), else the default: pocket-core for a system it hosts
   (`src/hostsys.rs`, only systems marked `verified`), else es_systems.cfg's default. pocket-core starts as
   `pocket-core <core> <rom> --system <name>`; zip files are fine, `.7z` games and games that failed to start on
   pocket-core before (`src/fallback.rs`) go to the next choice down (not a zip that pocket-core refuses with advice: several games, a wrong checksum; `src/archive.rs`; nor a patch next to the game that doesn't apply, which RetroArch would skip and start the original: `src/softpatch.rs`). Everything else starts
   `/usr/bin/runemu.sh <rom> -P<system> --core=<core> --emulator=<emulator>`, which brings ROCKNIX's per-system
   settings (setsettings.sh: the RetroArch keys of the "Runs on" page), BIOS paths and CPU handling.
3. When the game ends, pocket-ui comes back to the same place in the shelf and adds the play time to
   `recents.json` (written by the card writer thread, `fsutil::write_json_soon`, so the shelf's first frame doesn't
   wait for the card's fsyncs).

While pocket-core starts, the shelf fades into the picture of where the game was left (`src/launch_picture.rs`), so
the start is seen as a continuation; the time from A to the first frame is logged step by step
([DEVELOPING.md](DEVELOPING.md#start-time)). Settings → Playing → On start can start the last game right at boot
(`src/on_start.rs`).

Continuing where you left off: pocket-core writes an auto state on every exit (`<game>.pstate.auto`) and loads it
at the next start, unless Settings → Playing → Remember where I quit is off, which makes the pause menu's one Quit leave it out
(`quit nosave`, `Core::keep_resume_point`; the decision is `quit_leaves_out_resume` in `src/main.rs` over
`quit::offers_quit_without_saving`, settings.json `forget_where_quit`). RetroArch does the same through its own
setting and saves on every exit either way. PlayStation 2, GameCube, Wii and 3DS save and restore through their emulators' own slots
(`src/states.rs`, `src/standalone.rs`). Other standalone emulators start fresh.

## Switching games

A in the game switcher on another game quits the running one the way Quit in the pause menu does and starts the
chosen one when the first has ended. Between two pocket-core games it doesn't wait that long (`src/fast_switch.rs`,
`src/app_fast_switch.rs`): pocket-ui sends `quit handoff`, pocket-core writes its exit save (each file through
`fsutil::write_atomic`: temp file, fsync, rename), puts the CPU, fan and rumble back, and then writes `saved` into a
pipe pocket-ui gave it at the start (`POCKET_HANDOFF_FD`). On that line pocket-ui ends the game for itself and starts
the next one, while the old process tears down its core, GL, window and sound stream next to it; pocket-ui still
reaps it and stops it with SIGTERM and SIGKILL if it hangs, as after any Quit.

What makes this safe:

- **Nothing is read before it is written.** The line comes after the exit save is on the card. A pocket-core that
  crashes or is killed earlier, mid-save included, closes the pipe without it, and pocket-ui waits for the process to
  end as it always did; a save in progress is the old file or the new one, never half of one.
- **Only cores that write nothing on their own at unload** (`fast_switch::may_hand_over`: Gambatte, mGBA, Nestopia,
  Snes9x, Genesis Plus GX but not for the Mega CD, whose backup RAM it writes at unload): their battery save is what
  pocket-core writes, already done. Every other core, RetroArch, the standalone
  emulators and a next game that isn't on pocket-core wait for the process to end, as before.
- **No shared file.** The next game must not have the same ROM, or the same name in the same folder or system
  (`fast_switch::same_saves`); a start of the same game waits for the old process (`wait_for_drained`).
- **Shared system state is handed over, not fought over.** The CPU and fan settings are put back before the line;
  PipeWire's quantum is left at the game value for the next pocket-core game instead of being restored under it;
  pocket-ui skips the `swaymsg` that would put itself in front (it is in front: the switcher) and hands
  `recents.json` and the library's play time to the card writer after the next game has started.
- `POCKETUI_FAST_SWITCH=0` turns the hand-over off.

The pause menu's **Quit** uses the same hand-over for the same cores (`App::quit_to_shelf`): the shelf comes back on
the `saved` line, while the old process tears down its core, GL, window and sound stream next to it, which on the
device takes longer than the save itself. pocket-ui is in front already (the menu), so it sends no `swaymsg`; it puts
PipeWire's quantum back itself (the old process leaves it). A game started while the old process still ends waits
for it unless the two may overlap as in the switcher (`wait_for_drained`: the same game again, RetroArch or a
standalone emulator wait). Any other core still gets `quit` and is waited for.

Both programs log the steps with the same clock: `grep switch-time /var/log/pocket-ui.log` gives
`switch-time <CLOCK_MONOTONIC ms> <program> <step>` for the switcher's A, the quit, the exit save, the hand-over, the
next process's start and its first frame, the old process's teardown (core, window and GL, sound) and end. With
`POCKETUI_TRACE` the same steps go into the latency trace to the microsecond ([LATENCY.md](LATENCY.md#phases-where-the-time-went)).

## The pause menu

HOME opens a menu that pocket-ui draws over the paused game (`QuickMenu`, `MainItem` and `main_items` in
`src/main.rs`). What it offers depends on the emulator:

| Emulator | Menu |
|---|---|
| pocket-core | Continue, Save, Load (8 slots with previews, Auto, Before load), Earlier (rolling states, `src/timeline.rs`, `app_earlier.rs`), Disc (multi-disc games), Options, Achievements (signed in), Quick settings, Quit |
| pocket-core for ScummVM, PICO-8, TIC-80 (no save states) | Continue, Options, Quick settings, Quit, and the console's line on where saving is |
| ARMSX2, Dolphin, Azahar | Continue, Save, Load (the emulators' own slots, driven by their hotkeys or PINE), Options (`src/emu_settings.rs`), Quick settings, Quit; no Save and Load while ARMSX2 hasn't named the game yet |
| RetroArch | Continue, Save, Load (one state, through RetroArch's network commands), Options ("Runs on"), Quick settings, Quit |
| Others | Continue, Options ("Runs on"), Quick settings, Quit, and the line "This emulator keeps its own saves" |

Save and Load are only there where they do something (`src/pause_saves.rs`): where they couldn't, the menu says in one
line where saving is, beside Continue, Options and Quit, instead of offering rows that close the menu and do nothing.
Save previews reserve two note lines above the hint pill; on shorter native outputs the preview moves up and gives
up height before the save's age or replacement warning can be covered (`tests/text_size.rs`).
Under Save's and Load's picture a slim row of eight blocks in the save medium's plastic (`save_media::paint_row`);
with Memory card in the pause menu on the game's save medium stands beside its blocks, styled like that console's
memory-card screen (`src/save_media.rs`, the cards PixelLab pictures in `assets/save_media`, decoded once and scaled
once per size, the cartridge from `media::game_pic`, made once per size and cover; `app_save_media.rs`); a slot's icon is the game's
cover from the picture loader's thread, so the menu reads nothing new when it opens. The info page and the box's back
show the same slots read-only (`game_card::CardSaves`, a few `stat`s when the card opens, pocket-core's games only).
The strip's unlimited-height layout caps its units before multiplication, keeping slots readable even in narrow
preview columns on native 1024×768 outputs.
The cartridge-label cache retains at most four source cover `Rc`s alongside the rendered labels, so an evicted
picture loader allocation cannot be mistaken for another game's cover at the same address. Painting hashes and
copies no cover pixels to identify it.
Cartridge labels without cover art keep their game names untranslated while measuring and wrapping, including
shortened names whose subtitle has been dropped (`media::label_title`).

With Settings → Games → Curator → Pixel Path switched on, Pixel Path comes before Quick settings in pocket-core's menu and
in ARMSX2's, Dolphin's and Azahar's when their settings files could be read (`pixel_standalone`, read on a thread; not in
RetroArch's or the others'). The player's
version of this table is in [user/pause-menu.md](user/pause-menu.md); a test keeps the two in step
([DEVELOPING.md](DEVELOPING.md#documentation-checks)).

**Opening it** (`open_menu` in `src/main.rs`, `src/app_menu_open.rs`) is meant to take about a frame:

1. The game stops drawing first. pocket-core gets `pause` and answers `paused` on its hand-over pipe
   (`fast_switch::PAUSED`); pocket-ui waits for that line (at most `control::MENU_PAUSE`, 100 ms; it comes within a
   frame) before it covers the game. A pocket-core covered in the middle of a frame would wait in `eglSwapBuffers`
   for a frame callback a hidden window never gets, and read no command after that, the Quit included. Every other
   emulator but RetroArch is stopped with SIGSTOP; RetroArch pauses when it loses focus.
2. pocket-ui puts its window in front through sway's IPC socket (`SWAYSOCK`, `control.rs`; no `swaymsg` process),
   after looking up the game's window the same way.
3. The menu goes up over black. For pocket-core the picture of the game comes a moment later: pocket-core renders
   its last frame into `/tmp/pocket-core-shot.bin` (`shot <seq>`, `preview::SHOT_FILE`), a helper thread reads it,
   and it is drawn behind the menu (darkened once per picture, not in every frame). Other emulators can't do that:
   they are captured with grim before the window goes up, within the budget of B10 (`control::MENU_HELPERS`).
4. Behind a game sway keeps pocket-ui's window tiled next to it, half the screen wide (after the first Continue).
   Right after it puts itself in front pocket-ui draws only at its full size (`app_menu_open::Raise`): a frame of
   the old size held sway's resize for 200 ms in the virtual device.

pocket-core applies an Options change to the running game at once (over `/tmp/pocket-core.cmd`); settings are stored
per console or per game (`src/options.rs`). The standalone emulators' options are stored per game in their own
files and applied when the game goes on, by a restart where needed. Details: [HOST.md](HOST.md),
[FRAMES.md](FRAMES.md), [BIG-EMULATORS.md](BIG-EMULATORS.md).

**Run-ahead** (`src/run_ahead.rs`) runs inside pocket-core's frame loop: every refresh it runs the real frame, saves
the state, runs one or two frames ahead with the same buttons and shows that picture, then loads the state back. It is
offered only for light 2D systems and switches itself off when a core is too slow, its states are slow or it doesn't
replay exactly ([HOST.md](HOST.md#run-ahead-srcrun_aheadrs-2026-10-23)).

**RetroAchievements** (`src/achievements/`, rcheevos in `vendor/rcheevos` with `src/rc_shim.c`) also run inside
pocket-core: the game is identified and its set loaded on a thread, so the game starts as fast as without an account;
memory is checked on real frames only (never the frames run ahead); unlocks are queued for offline play. pocket-ui owns the account
(Settings → Playing → RetroAchievements) and reads the list for the pause menu from pocket-core
([HOST.md](HOST.md#retroachievements-srcachievements)).

## Rescue path

ROCKNIX stays the system, pocket-ui is the interface, and EmulationStation is only an emergency exit. Nothing falls
back into EmulationStation by itself any more as long as pocket-ui can run (decided 2026-10-23; analysis in the
review notes "ES-REPLACEMENT-GAPS", sections 4 to 6).

**Exit codes.** pocket-ui tells the launcher what to do next; `src/rescue.rs` and the launcher's `CODE_*` lines hold
the same numbers, and a unit test compares them.

| Exit | Meaning | The launcher |
|---|---|---|
| 0 | Start again (the rescue page's Restart, the dialog's Restart pocket-ui, the command FIFO's `quit`, a closed window) | starts pocket-ui normally. Exit 0 before the first picture counts as a failed start |
| 75 | An update was installed (`update::RESTART_CODE`) | starts it again |
| 77 | The player asked for EmulationStation, after the warning (`rescue::OPEN_ES_CODE`) | cleans up, then `exec start_es.sh` (by hand: starts the essway unit) |
| 78 | The rescue page's "Use previous version" (`rescue::PREVIOUS_CODE`) | points `current` at `known-good`, else `previous`, then starts normally |
| anything else, or no picture | A failed start; 126/127 mean the program can't be run at all | the stages below |

**Stages of a failed start.**

1. A freshly installed version with `update-pending`: back to `previous` at once ([UPDATES.md](UPDATES.md)).
2. Restart, up to `POCKETUI_RESTARTS` (3) times, pauses 1, 2, 4 s; a run of `POCKETUI_STABLE_SECS` (120 s) starts the
   count afresh. Exit 126/127 skips the restarts.
3. The last known good version: a run with a picture that lasts `POCKETUI_STABLE_SECS` makes its release
   `known-good` (a link next to `current`). If `current` is another release, it goes back to `known-good` and
   `rolled-back` names the one that failed. `update.rs` never prunes `known-good`; `dev.sh deploy` copies a known
   good `releases/dev` to `releases/dev-good` before it overwrites it.
4. Safe mode: `pocket-ui --safe` with `POCKETUI_SAFE_REASON`, up to `POCKETUI_SAFE_RESTARTS` (2) restarts.
5. EmulationStation, with the reason in the log and in `/storage/pocket-ui/es-fallback`, which the next pocket-ui
   start shows once (`rescue::take_es_note`). Also at once when pocket-ui is missing or not executable, and when sway
   stops answering after a start without a picture. The es-only mark of `dev.sh stop` still hands over directly.

**Black screen guard.** pocket-ui creates `POCKETUI_FRAME_FILE` (`/tmp/pocket-ui.frame`) once its first picture is on
screen (`rescue::note_first_frame`, on a thread). A run without it after `POCKETUI_FIRST_FRAME_SECS` (45 s) is ended
(SIGTERM, then SIGKILL) and counts as a failed start. Every helper the launcher calls (`swaymsg`, `systemctl`,
`pocket-core --restore-system`, `quantum`) runs under `timeout` (Sol finding SOL-07), and the wait for sway at boot is
measured by the clock. The drop-in has no start limit; instead `RestartSteps`/`RestartMaxDelaySec` stretch the pause
before each restart of the unit from 2 s to a minute, so essway never ends up "failed" with only sway on screen.

**Safe mode** (`src/rescue.rs`, `src/app_rescue.rs`) is a separate, small event loop in the same binary, chosen in
`main` before anything else is read: the window, the built-in type, the pads, touch and the command FIFO, and nothing
else. No game list, no picture decoding (covers, frames, previews), no Curator index, achievements, sync, store, Wi-Fi
watchdog, search or cover threads, no theme fonts; only the appearance is read from `settings.json`. The rescue page
shows the launcher's reason, the last line of `crash.log` and the launcher's last lines, and offers Restart normally
(exit 0), Use previous version (78, only when there is one), Reset pocket-ui settings (moves the data folder's contents
except `logs/`, `crash.log`, `share/`, `remote/` to `<data>.rescue-<date>`; games and saves are elsewhere), Open
EmulationStation… (77, behind the warning) and Power off.

**EmulationStation on request.** SELECT + START held for a second, Escape on a keyboard and the command FIFO's `leave`
open a dialog (Cancel, Restart pocket-ui, Open EmulationStation) instead of leaving; Settings → Help & tools → About has "Open
EmulationStation…" with the same warning: do not use Updates there (on SM8250 a system update rewrites the ABL
bootloader Android needs) or Factory reset (it deletes pocket-ui). Before exit 77 pocket-ui sets `updates.enabled=0`
in `system.cfg`, so EmulationStation doesn't offer that update by itself.

## ROCKNIX's system.cfg

`/storage/.config/system/configs/system.cfg` is shared with ROCKNIX's scripts: `runemu.sh` writes `cooling.profile`
at the start and end of a standalone game while pocket-ui may be saving the pause menu's options. Every write from
pocket-ui goes through `src/system_cfg.rs`, which takes ROCKNIX's lock `/tmp/.system.cfg.lock` exactly as its
`wait_lock` does (create-exclusive with `noclobber` semantics, the process id inside, removed only while it is ours),
reads, changes and writes the file atomically while it holds it, waits at most 10 s, and removes a lock whose process
has ended. `system_cfg::set` is ROCKNIX's `set_setting` (all `key=` lines out, `key=value` appended, `default`
removes). A unit test runs ROCKNIX's own `set_setting` in bash against pocket-ui's writers at the same time and loses
nothing.

ROCKNIX main (after 20261001) takes the same lock with `flock` instead (`exec {fd}>>lock; flock -x`, the file stays).
`system_cfg` reads which one the running system's `wait_lock` uses from `/etc/profile.d/001-functions` once
(`LockStyle::of_device`) and takes the lock the same way; a second test runs ROCKNIX main's `set_setting` against it.
Both styles, the lock's path, `set_setting`'s steps and every other promise pocket-ui relies on in ROCKNIX are listed in
[ROCKNIX-CONTRACT.md](ROCKNIX-CONTRACT.md) and checked on an image, a source tree or a running device by
`scripts/rocknix-contract.sh`.

The Wi-Fi watchdog also keeps `wifi.key` from going empty: EmulationStation's "connect to a saved network" writes it
empty, and its next `wifictl connect` would rebuild the NetworkManager profile without a key. At pocket-ui's start the
watchdog copies the profile's key back into `system.cfg` when they differ (`wifi::key_to_restore`).

## Where the other features live

| Feature | Where | Notes |
|---|---|---|
| Search and the letter jump | `src/search.rs`, `search_page.rs`, `letters.rs`, `app_search.rs` | The index is built on a thread after the library loads; each keystroke filters it on the UI thread within a frame ([DESIGN.md](DESIGN.md#finding-a-game-srcapp_searchrs-srcsearchrs-srcsearch_pagers-srclettersrs)) |
| Fitted box art, screenshots and title screens | `src/app_pictures.rs`, `picture_loader.rs`, `App::cover` | PNG/JPEG decoding and box-filter scaling run on the `pictures` worker. Live frames use the pictures already available and redraw on arrival; the shared fitted-image cache keeps 16 entries, rather than every game and size encountered while scrolling. Snapshot mode uses the identical decoder synchronously. Case artwork uses the separate cache below |
| Idle slideshow | `src/slideshow.rs`, `app_slideshow.rs` | After a quiet minute on the shelf the games' pictures play (Ken Burns, cross-fades, title, clock). The pictures are listed on the `slides` thread and read and scaled on the `slideshow` thread; the UI thread only blends prepared pictures at 15 frames a second and reads `Sleep::quiet_for`, it sets no sleep timer ([SLEEP.md](SLEEP.md#idle-slideshow-2026-10-30-srcslideshowrs-srcapp_slideshowrs)) |
| The games' cases (Cabinet and Shelf looks) | `src/media.rs` (templates in `assets/media`), `cabinet.rs`, `cabinet_art.rs`, `app_cabinet.rs` | Box art is read on the `cabinet-art` thread (a cover's colours, a cover cut for a case's front), asked for the first time a case needs it; frames only copy pictures made once and kept: a spine per game (the Shelf look's per game and scale), the front of the case that is out (the Shelf look's per game and size), the board and the props per size; the cartridge, card or printed disc inside a case (`media::Game`, `media::disc_pic`; the disc templates decoded on the `cabinet-art` thread) is made from that front's cover, once per box opened and in the launch animation's first frame ([DESIGN.md](DESIGN.md#media-srcmediars-assetsmedia)) |
| Curator | `src/curator/` (pure logic), `curator_page.rs`, `app_curator.rs`, `curatordb.rs`, `romfacts.rs`, `connections.rs`, `app_connections.rs` | The index is built on a thread after the shelf's first frame and never while a game runs; Connections' index (a card's developer, series, genre leading to the player's games) and Rediscover's games are made on the same thread with it, so a card only looks its facts up; Home's rows say "Getting ready…" until then. A new Connections index also refreshes the open card's facts and links from its original offline facts plus the current in-memory gamelist, replacing previously inferred series and retaining selection only on a still-linked identical fact. Up next after a game is worked out by the build the session starts, on that thread. Sort orders (`curator/sort.rs`) only sort the list START or a smart shelf shows, after the Lens has picked it; "Surprise me" (`curator/surprise.rs`) is one weighted draw over the index on the UI thread when its entry or Right on its card is pressed (about 0.1 ms for 5,000 games). What Home's Curator row offers (`curator/today.rs`: the day's pick and the games waiting) comes with each index from its thread; Home's panels (`home_panel.rs`, `app_home_panel.rs`) paint covers from the image cache only and ask the picture loader's thread for the rest ([CURATOR.md](CURATOR.md), [CURATOR-DATA.md](CURATOR-DATA.md)) |
| Play history | `src/play_log.rs`, `play_history.rs`, `history_page.rs`, `app_history.rs` | The UI thread only sends a game's start and end (and sleep's pause and wake, from `sleep.rs`) into a channel; the `play-log` thread (low priority, started after the first frame) appends to `play-log.bin` and syncs, a start a few seconds later so it never meets the game's loading. History and the Year in review are counted on a thread per request; the page shows its title at once. The review never opens by itself. `play_history::stats_json` is the web companion's read API ([HISTORY.md](HISTORY.md)) |
| Console pages and Credits & links | `src/consoles.rs`, `console_page.rs`, `console_credits.rs`, `credits_page.rs`, `app_console.rs`; `assets/data/consoles.json`, `console-credits.json` | Both files are compiled in and parsed once; X on a page resolves its links against the library (a few hundred µs, the games by the makers from Curator's catalog, never a gamedb lookup on the UI thread; "Getting ready…" until the catalog is built). Data pipeline: [DEVELOPING.md](DEVELOPING.md#console-credits) |
| Screen effects and sharpness (pocket-core) | `src/video.rs`, `shaders/video.frag`, `picture_fx.rs` | One fragment pass in pocket-core's draw, a compiled variant per sharpness × effect (made when the menu changes them, never per frame); the CRT's glow reads mipmaps made by the GPU when a new picture came. No other work on pocket-core's loop; the constants live in `picture_fx` with a CPU reference, tested against the GPU on a headless GL (`tests/picture_shaders.rs`) ([HOST.md](HOST.md#picture-effects-2026-10-24)) |
| Pixel Path | `src/pixel_path.rs`, `pixel_path_page.rs`, `app_pixel_path.rs`, `App::write_pixel_path` in pocket-core | Asked for four times a second over the command FIFO, only while its page is open ([CURATOR.md](CURATOR.md#pixel-path-as-built-featpixel-path-2026-10-23)) |
| Pixel Path for PS2, GameCube, Wii and 3DS | `src/pixel_standalone.rs`, `app_pixel_path.rs` | The emulator's settings files, read on a thread 10 s after the start and when the pause menu opens; the menu only looks at the last reading ([CURATOR.md](CURATOR.md#pixel-path-for-ps2-gamecube-wii-and-3ds-featpixel-path-standalone-2026-10-24)) |
| Over the game (Off, FPS, FPS + sensors, Pixel Path) | `src/over_game.rs`, `overlay.rs`, `osd_sensors.rs`, `extras.rs`, `App::update_strip` in pocket-core; the Settings row in `main.rs` | One level: Options per game over per console over Settings → Playing (`settings.json`'s `over_game`, read by `GameSettings::load`); HOME + Y moves it on for the session. Off draws nothing, starts no reader and builds no report ([HOST.md](HOST.md), "Extras") |
| Pixel Path over the game | `src/pixel_live.rs`, the strip piece of `overlay.rs`, `App::update_strip` in pocket-core | Over the game's Pixel Path level: pocket-core builds the same report itself twice a second (the FPS counter's half-second), only while the level is on; no FIFO, no file, nothing per frame but the one quad ([CURATOR.md](CURATOR.md#pixel-path-over-the-game-featpixel-path-live-2026-10-23)) |
| Controllers and players (couch co-op) | `src/pad.rs` (`Mux`, `Controller`), `players.rs`, `players_page.rs`, `app_players.rs`, `rumble.rs` | Every device is read on its own and merged (review SOL-09): pocket-ui acts on the merged pad (`PadState::merge`), pocket-core gives each libretro port 0-3 the controller in that seat (`players::Inputs`), each player's rumble goes to their own controller. The seats live in pocket-core (`/tmp/pocket-core.players.json`), the order chosen on the Players page in `players.json` ([HOST.md](HOST.md#players-couch-co-op-srcplayersrs)) |
| Font style | `src/font_style.rs`, `Text::sync` in `src/text.rs` | Barlow, Pixel (title face only) or Rounded; one atomic holds the style, each `Text` compares it at its next glyph and only on a change swaps its faces (parsed once per process, on first use) and clears its glyph cache; alternative faces are scaled to Barlow's capital height so layouts built on Barlow keep their baselines; a store theme's own fonts and safe mode are not touched ([DESIGN.md](DESIGN.md#font-styles-srcfont_stylers)) |
| Button layout | `src/button_layout.rs`, the pad closure in `main` | Which physical face button plays A, B, X, Y in pocket-ui's menus (Nintendo as printed, or Xbox); applied once, on the pad threads, before the UI thread sees an event, so all screens (and a Bluetooth controller) follow it; games, touch, the FIFO and safe mode are untouched ([DEVELOPING.md](DEVELOPING.md)) |
| Colours, Font (Settings → Look), Buttons (Settings → Playing) | Settings rows that switch in place (`Opens::Colours`, `Opens::Font` in `main.rs`), the Buttons page `src/app_buttons.rs` (Button layout and Button symbols, the face buttons' picture), `src/button_prompts.rs`, `palette::Colours`, `src/font_style.rs`; the look previews are painted again when `app_look::style_mark` changes | Colours: `pal()` mixes the dark and light palette of the `Scheme` in use (an atomic checked per call, so a change shows in the next frame on every thread); the Cabinet's wood (`cabinet_art::wood`, cached per page colour, which differs per scheme), plates and slot light read `palette::scheme()`. Saved in settings.json's `"colours"`, Warm when unset. Button symbols: one atomic read by `kit::key_cap` (every hint key) and the manual's controls picture; the PlayStation shapes are private-use characters `Text` rasterises itself (`button_prompts::raster`). Saved in settings.json's `"prompts"`, the device's default when unset |
| Motion | `src/motion.rs`, `app_motion.rs` | Glides, page transitions and fades, Full / Reduced / Off; a press always acts at once, an animation never delays it ([DESIGN.md](DESIGN.md#motion-srcmotionrs-srcapp_motionrs)) |
| Screen frames and the power light | `src/bezel.rs`, `frames.rs`, `frames_more.rs`, `frames_tv.rs`, `frame_led.rs`, `frame_cache.rs`, `ambient.rs` | Drawn in pocket-core at the output size, cached on the GPU and in RAM between starts (`frame_cache`); at a game's start prepared on a thread and faded in, never in the way of the first frame; only the power light is painted live ([FRAMES.md](FRAMES.md)) |
| Earlier: pocket-core's rolling states | `src/timeline.rs`, `app_earlier.rs`, `src/bin/pocket_core.rs` (`timeline_play`, `timeline_quiet`) | In pocket-core only the serialize runs on the emulation thread, and only between two refreshes when it is measured to fit a quarter of one (else a second into the pause menu, or the sleep's resume state copied); the `earlier-writer` thread encodes the picture, writes the file and keeps the caps (100 MB a game, 1 GB in all). The pause menu only `stat`s nine known names when it opens ([HOST.md](HOST.md#earlier-rolling-states-srctimeliners-2026-10-25)) |
| Which device; wide and short screens | `src/device.rs`, `app_column.rs` | The device tree's `compatible` and `model`, read once on the first use (the inhibitor's thread, normally): Mini V2, the experimental Pocket 5, Flip 2 and Mini V1, or unknown with the Mini V2's numbers ([DEVICES.md](DEVICES.md)). On a screen at least 1600 px wide the pages of rows and text paint into a column buffer that is copied into the frame's middle (one row-by-row copy per frame, nothing at 1240 or 1280 px); everything else lays out from the canvas size |
| Display out (a TV through USB-C) | `src/display_out.rs`, `app_display_out.rs`, `app_touch.rs` | One thread (`display-out`) asleep on sway's IPC socket (`SUBSCRIBE ["output"]`), started after the first frame; a panel-only Pocket costs one `GET_OUTPUTS` and sends nothing to sway. A change becomes one `UserEvent::Display` (the UI stores it, drops touches while on the TV, and shows a note on the shelf); the sway commands that repair what ROCKNIX left out run on that thread ([DISPLAY-OUT.md](DISPLAY-OUT.md)) |
| Sleep, the power key, the low-battery guard, switch off after sleeping, Settings → System → Sleep & power | `src/sleep.rs`, `powerkey.rs`, `power_menu.rs`, `low_battery.rs`, `auto_off.rs`, `sleep_settings.rs` | A worker thread saves and suspends; the wake turns the game and the screen (its own thread) on as soon as the system is back and leaves the volume and the quantum to after ROCKNIX's wake hooks ([SLEEP.md](SLEEP.md#waking-timeline-instant-on-2026-10-24)). Non-power-key suspends and RTC re-suspends explicitly respect logind sleep inhibitors; the default data directory’s no-autosuspend marker also protects redirected test data. Failed suspends are retried with longer waits; at 2 % on battery the game is saved and the device switched off ([SLEEP.md](SLEEP.md)). After 1, 3 or 8 h asleep on battery the RTC alarm wakes the worker with the screen dark and the device is switched off the same way ([SLEEP.md](SLEEP.md#l3-switch-off-after-sleeping-2026-10-25)). The power button's short press can turn only the screen off with the game running (pocket-core told `screen off`: no drawing, no buttons; no sleep by the timers), and the power menu's Restart pocket-ui saves like Restart and then ends pocket-ui with exit code 0 for the launcher to start it again ([SLEEP.md](SLEEP.md#screen-off-with-the-game-running-2026-10-26)) |
| Status corner (time, Wi-Fi, battery at the top right) | `src/status_corner.rs`, `app_status_corner.rs` | Home (every look), the game card, console pages, the search and Curator's pages paint the same corner from memory: no file is read on the UI thread (the battery used to be read from sysfs in every frame). Wi-Fi (`/sys/class/net/wlan0/operstate` is `up`) and the battery (`capacity`, `status`) come from the `status-corner` thread, which reads the three files every 3 s only while the corner is on screen and parks otherwise (asleep, in a game without the pause menu or the quick settings over it); a change wakes the UI with one event. Settings, its pages, the pause menu's pages and the quick settings show the same corner in a pill (`status_corner::Pill`), from the same memory. The time is `localtime_r`, at most once a second; a frame with the corner sets one wake-up just after the next minute, none while asleep or in a game |
| Settings row values ("Home >", "Shelf · Dark >") | `src/settings_values.rs`, `app_settings_groups.rs` | A category's list paints its values from memory. Look, Add games, Performance & fan ("Auto · Quiet"), Battery, Updates and About are the App's own state; the Wi-Fi network's name (`nmcli … --rescan no`), Remote access (`systemctl is-active sshd`), the time zone and the paired PCs are read on the `settings-values` thread when a list with one of them is painted, at most every 5 s, and wake the UI with one event when they changed; "connected" is wlan0's `operstate` read afresh there (`status_corner::wifi_now`: the corner's own thread sleeps on the Wi-Fi page), and a change of the network in use or the address in the shared `net_state`, or of the corner's Wi-Fi, reads them again at once (`settings_values::watch`, `stale`) instead of after the minute's repaint; Bluetooth's switch (system.cfg) the same way on a thread of its own (`kept::Kept`, every 2 s; the Bluetooth page sets it when it saw the switch). Until they are in, the row shows ">" alone |
| Values read on a thread (`kept`) | `src/kept.rs`; Settings → About (`ABOUT` in `src/main.rs`), `app_bluetooth.rs`, `wifi::ip` | A value the UI shows but must not read itself: the About page's free space (statvfs on the card) and address (`ip`), every 5 s while it is shown, "…" until the first read; `wifi::ip` answers from memory and has the address looked at again on a thread when it is older than 3 s (the watchdog and the threads that need it fresh use `ip_now`) |
| Pages that open before their answer is in (Settings → Games → Add games, the first start's time zone step, Settings → System → Time) | `setup.rs` (`go_to_games`, `enter_zone`, `look_zone`, `SetupEvent::GameCounts` / `Zone` / `ZoneOffsets`), `quick.rs` (`open_time`, `load_zones`, `load_offsets`, `poll_all`) | Nothing walks a folder, runs a process or reads a zone file on the UI thread, when the page opens or in a frame. Add games opens with the counts the shelf knows (`ShelfSystem::games`) and a thread counts every ROM folder, then the numbers swap in and the selection stays on its tile. The same thread (`count_catalog`) lists the folders of `roms` that are no system's (`library::stray_folders`, with the folder their games probably belong in) and the disc games left out for a missing file (`library::look_at_folder`); the page names them under its title (`unread_lines`). The time zone step opens with "Looking at your time zone…" while a thread runs `iw reg get`, reads the zoneinfo tree and tables and the offsets of the zones it shows; the lists' offsets come the same way, blank until they are in. The Time page opens with "…" for the zone and its offset (`Polled::timezone`, `zone_offset`, read by `poll_all` while the page is open), and a line under Search… while the zone list (`Polled::zones`, read once) is not in; a city list's offsets are read by the worker (`want_offsets`). Date and time set by hand (`manual_time`) is set by the same worker (`Hw::set_clock`, job `set-clock`; the row says "Setting…" until it answers, `quick::tests::setting_the_clock_never_runs_on_the_ui_thread`). Tests: `quick::tests::the_time_page_never_looks_at_the_system_on_the_ui_thread`, `setup::tests::the_zone_step_opens_at_once_and_never_looks_at_the_system_on_the_ui_thread` (both with `system_ctl::spy::Spy`, which records every look made on the test's thread) and `…the_games_step_opens_with_the_known_counts_and_counts_the_folders_on_a_thread` |
| Wi-Fi watchdog | `wifi::watchdog` (`src/wifi.rs`) | A thread started with pocket-ui: repairs a saved profile that lost its key from ROCKNIX's `system.cfg` once, and writes the profile's key back into `system.cfg` when EmulationStation emptied it ([system.cfg](#rocknixs-systemcfg)), then checks the address every 15 s (kept for `wifi::ip`, so the UI has it from the first moments) and, while Wi-Fi is switched on, reconnects a dropped network after 30 s, 1 min, 2 min, then every 5 min, but only to a saved network the radio really heard since the drop (`iw dev wlan0 scan dump`, `wifi::heard_within`; nmcli and iwd go on listing an access point that went away until the next scan, and a try at it fails with "Secrets were required" and makes iwd shun it when it comes back), and at once when one is heard again (it asks for a scan at each look for the first 10 min offline); a saved connect the player started that fails that way while the network isn't heard says out of range instead of asking for the password (`wifi::gone`); scans and reconnects use the shared `net_state` lane, with fresh device-state checks and a boot/resume settle window (`wifi_activation.rs`); a connection already active or activating for the requested SSID is reused |
| The shared Wi-Fi picture: radio, connection, last scan | `src/net_state.rs` (`NetState`); read by `quick.rs` (`Sys::wire_net`) and `setup.rs` (`Setup::apply_snapshot`) | One worker thread (`net-state`) does everything that changes the radio or the connection (switch, scan, connect, forget, country), one at a time; a generation drops the looks and scans that began before a change; both surfaces are told after every change and read the snapshot from memory. The Wi-Fi page also looks at the radio every 4 s on a thread of its own while it is up (`Setup::keep_radio_watch`, ends with the page); the radio switch and `wifi::scan_checked` (40 s timeout) run only on these threads ([SYSTEM-CONTROLS.md](SYSTEM-CONTROLS.md#quick-settings-home-on-the-shelf)) |
| Sync | `src/sync.rs`, `sync_page.rs`, `app_sync.rs` | While any game runs, the battery-save and save-state folders (and the standalone emulator's) are paused in Syncthing, and resumed when it ends; folders left paused by a pocket-ui that stopped are resumed at the next start ([user/sync.md](user/sync.md), [SECURITY-HARDENING.md](SECURITY-HARDENING.md)) |
| Saves with two versions | `src/sync_conflicts.rs`, `sync_conflict_page.rs`, `app_sync_conflicts.rs` | One `sync-conflicts` thread (started after the first frame) looks for Syncthing's `.sync-conflict-` copies in the folders of the shares that are on: 20 s after the start, after every game (at once and 30 s later), when Settings → Sync opens and every 5 minutes, never while a game runs; names only (directory entries, a few levels deep, at most 40,000 per folder), a `stat` only for a copy found. It reports a changed result as an event; the UI thread ties the copies to games (same folder and name, a state's name, a memory card to every game of its system) and paints the card's row, the one-time shelf notice and the page from that list. Keeping a version runs on a thread of its own: the version in use is copied to `/storage/roms/sync-backups/<date>` and flushed before the chosen copy is renamed over it, the other copies are moved there (copied and removed across file systems); the folder is in the saves folder's `.stignore`. The copies told about are kept in `sync-conflicts.json` ([user/sync.md](user/sync.md#two-versions-of-a-save)) |
| Library data | `src/library_sync.rs`, `app_library_sync.rs` | One `library-sync` thread (low priority, started after the first frame) merges the curation with the other devices' through the Syncthing folder `<data>/library-sync/<device>/`, each device writing only its own folder: 15 s after the start, after every game (at once and 30 s later), when Settings → Sync opens or a row is switched, and when a `stat` once a minute finds a changed file; never while a game runs. A pass reads `library.json`, `favorites.json`, `curator.json` and the play log, stamps what changed since the own folder's state with its file's mtime, merges every device's values field by field (last writer wins, tombstones for removals; play history a de-duplicated union) and sends the changes as an event. The UI thread applies them with the stores' own types (skipping any the player changed meanwhile) and saves once; only then does the thread write the own folder. Other devices' sessions reach the pages through `play_log::set_extra`, never the own `play-log.bin` ([SYNC-CURATION.md](SYNC-CURATION.md)) |
| "This game" page (START on a card) | `src/this_game_page.rs`, `app_this_game.rs` | the pause menu's own rows (`options::GROUPS`, `emu_menu`, `emu_settings`) for a game that is not running; every change is written at once as the game's own settings (`game-settings/<system>/<game>.json`, system.cfg), the files pocket-core and the pause menu read ([options.md](user/options.md#this-game)) |
| Emulator choice ("Runs on") | `src/emu_choice.rs`, `emu_choice_page.rs`, `app_emu_choice.rs` | system.cfg's emulator and core keys read on a thread at start and resolved per launch (a map lookup); the page reads system.cfg when it opens and writes on a thread under ROCKNIX's lock ([HOST.md](HOST.md#choosing-the-emulator-runs-on)) |
| Settings: categories | `SETTINGS_GROUPS` and `SETTINGS` (`src/main.rs`), `app_settings_groups.rs` | START opens seven categories (Look, Sound & feel, Playing, Games, Connections, System, Help & tools), A a category's list. The grouping is data: each category's rows in `SETTINGS_GROUPS`, and `SETTINGS`, the rows one after another, is built from it at compile time. `SettingsPage::List(row)` counts in `SETTINGS`, so a page's way back (`List(settings_row(NAME))`, also after a deep link) lands in its own category, and B there on the categories (`SettingsPage::Groups`). Opening Settings reads nothing; a category's list works out the values of its own rows only |
| Settings → Playing → Performance & fan | `src/perf_fan.rs`, `perf_fan_page.rs`, `app_perf_fan.rs`, `power.rs` | Game defaults under the per-console and per-game Options; the fan outside games ([HOST.md](HOST.md#settings--performance--fan-srcperf_fanrs-srcperf_fan_pagers-srcapp_perf_fanrs)) |
| Quick settings' levels and radios | `src/quick.rs`, `quick_paint.rs`, `system_ctl.rs` (`Worker`) | One worker reads the backlight, volume and mute (`pactl`) and Bluetooth (`systemctl`) and asks `net_state` for the Wi-Fi radio (rfkill and `nmcli radio`, the one picture Settings → Wi-Fi shares): 1.5 s after the start, at every open and every 2 s while open; the panel, its power menu and the OSD paint only that memory ("…" before the first answer), a step before it is taken by the worker ([SYSTEM-CONTROLS.md](SYSTEM-CONTROLS.md#quick-settings-home-on-the-shelf)) |
| Quick settings' Performance and Fan | `src/quick_profiles.rs`, `quick.rs`, `quick_paint.rs`, `app_quick_profiles.rs` | The same choices as Settings → Playing → Performance & fan; nothing read on open (the choices, the pause menu's own settings and the port store are in memory, the device's cpufreq and fan probed once on a thread); a change saved and applied to the paused game on a worker thread through the existing paths (pocket-core's FIFO `set`, `standalone::set_performance`, Smooth fan's plan) ([SYSTEM-CONTROLS.md](SYSTEM-CONTROLS.md#performance-and-fan)) |
| A running port's pause menu | `src/standalone.rs` (`guest`, `quit_port`), `pause_saves.rs` (`Saver::Port`), `app_port_perf.rs` (`port_perf_in_menu`) | HOME → the generic menu; Options is the port's Performance & fan (`Page::PortPerf`); Quit: window close, SIGTERM then SIGKILL to this start’s private session, checking PID and start time; helpers in other groups of that session are included, other launches are untouched, ≤ 6 s |
| Installed port pictures and versions | `src/ports.rs` | One worker reads directory metadata every second and scans `port.json` only after the parent directory changes or `forget` invalidates an install, update or removal. Readers keep the last finished `Arc` snapshot with a nonblocking publication check; neither directory metadata nor the scan runs on the UI thread. An invalidation during a scan causes another pass before publication. `start_index` registers the event-loop wake before starting the worker; each published revision clears the UI's art caches and refreshes update notes. |
| A port's Performance & fan | `src/port_perf.rs`, `port_perf_page.rs`, `app_port_perf.rs`, `power.rs` (`set_port`) | Every port's choice and note in memory (`port_perf::Store`, read on a thread at the start, written by the card writer); the performance applied on the session's thread once runemu.sh has started the port's own process (also after a launcher's `exec`), holding the CPU's lease (`power::Lease`) until that thread has put it back, so the next game never notes a port's caps; two minutes of a `/proc` look every 10 s for a core kept busy, then nothing ([HOST.md](HOST.md#portmaster-ports-performance-and-fan-srcport_perfrs)) |
| Battery per game | `src/battery_games.rs`, `battery_history.rs` (`set_game`, `game_left`, `game_card`) | Read and fed by the battery sampler's thread (once a minute; read with no lock held, "Loading…" until then; a stretch with a sleep change in it counts for no game); the pause menu's line and the card's come from memory when they open (microseconds, no file or sysfs read on the UI thread) ([SLEEP.md](SLEEP.md)) |
| Night light | `src/night_light.rs`, `display.rs` (`Look::night`), `app_night_light.rs`, `quick.rs` (`Sys::set_night`) | One thread (`night-light`) asleep in `poll(2)` on a `CLOCK_REALTIME` timerfd for the next switch time (cancelled when the clock is set, fires on resume) and a socket for pokes (new choices, wake from sleep, a new time zone): no polling timer, nothing per frame. A switch fades the white point over 1.5 s in 15 steps, each one `UserEvent::Night` that `Sys::set_night` turns into the same `apply_look` as the temperature keys: the compositor's ramps (Gamma, every game) or the Soft tint (pocket-ui's frames, pocket-core's `look` command) ([SYSTEM-CONTROLS.md](SYSTEM-CONTROLS.md#night-light), [user/night-light.md](user/night-light.md)) |
| Smooth fan | `src/fan_curve.rs`, `app_perf_fan.rs`, `deploy/run-pocket-ui.sh` (`fan_handback`, `watch_fan`) | pocket-ui drives the fan's PWM along a curve on its own thread (a dozen sysfs reads every 0.5-1 s, nothing on the UI thread), with the six `active` trips of the fan's zone parked at a backstop; `hot`/`critical` trips and the CPU/GPU cooling stay the kernel's. Handed back to the kernel for sleep, power off, Smooth fan off, ROCKNIX's custom profile and a fight with the kernel; by the launcher when pocket-ui ends or the curve's heartbeat stops ([FAN-CURVE.md](FAN-CURVE.md)) |
| Starter pack (free games for an empty card) | `src/starter.rs`, `app_starter.rs`, `assets/starter-pack.json` | The list is compiled in and parsed once. Nothing is read or written on the UI thread: the page's scan (which folders, which games are there) and each download (the Store's curl, SHA-256 before anything is placed, one atomic write) run on threads of their own and report back as `UserEvent::Starter`; a game added makes `library_rescan` read the folders again, so the shelf fills by itself. Home paints the hint instead of an empty shelf while no system has a game ([STARTER-PACK.md](STARTER-PACK.md)) |
| Settings → Games → Game check | `src/game_check.rs`, `game_check_page.rs`, `app_game_check.rs` | Runs on its own thread at nice 10 with a 1 ms breath every 64 files, stopped when the page is left or a game starts. Takes the shelf's lists from the App, so what counts as a game has one truth; `hostsys`, `systems_grid`, `library` and `archive` keep their checks, and only `KNOWN_BIOS` is new. Hashes only known BIOS files. `library_rescanned` and `finish_setup` call `game_check_games_added`: every way games arrive ends in a list read again, so a new one (`feat/phone-upload`) needs nothing more |
| Settings → Help & tools → Tools | `src/tools.rs`, `tools_page.rs`, `app_tools.rs` | ROCKNIX's tools (`/storage/.config/modules`) and the Store's, started like PC streaming as an app; risky ones ask twice, system-changing ones are never listed ([SYSTEM-PAGES.md](SYSTEM-PAGES.md#tools)) |
| Settings → Connections → Wi-Fi: saved networks, hidden network, country | `src/wifi.rs`, `setup.rs` | nmcli on threads; ROCKNIX's `wifi.*` settings kept in step through one marked `set_setting` call ([SYSTEM-PAGES.md](SYSTEM-PAGES.md#wi-fi)); a typed password the network refuses takes its profile (and iwd's mirrored `.psk`) with it, and only an `activated` profile counts as connected ([SYSTEM-CONTROLS.md](SYSTEM-CONTROLS.md#quick-settings-home-on-the-shelf)) |
| Settings → Connections → Wi-Fi on a network with a sign-in page (hotel, train) | `src/net_check.rs`, `portal_page.rs`, `setup.rs` | The scan's thread asks `net_check` once; while the page or its sheet shows a sign-in, a thread of its own asks again every 15 s and ends with an answer that isn't a sign-in or when the page closes (it holds a `Weak` of the page's token). The sheet paints from memory only. "Sign in…" (`portal_form.rs`) loads and sends on threads of its own through `portal::Session` (curl, a cookie jar in a private RAM folder that goes with the page) and `html_form` (one pass over the page); a press only hands a copy of the form over ([PORTAL-SIGNIN.md](PORTAL-SIGNIN.md)) |
| Settings → Games → PC streaming | `src/moonlight.rs`, `moonlight_page.rs`, `app_moonlight.rs` | Pairing with Moonlight's own PIN read from its output, paired PCs, the PC's games written like ROCKNIX's menu ([SYSTEM-PAGES.md](SYSTEM-PAGES.md#pc-streaming)) |
| Web companion | `src/web/`, `web_page.rs`, `app_web.rs` | Off by default: no thread, no socket. On, a supervisor and three workers at the lowest CPU and I/O priority listen only on the Tailscale interface (`SO_BINDTODEVICE`, port 80). The App only stores into shared state per loop turn (comparisons; whether a game runs, Now playing, Curator's index) and applies the browsers' edits as `UserEvent::Web`, each with the function a button press uses; an upload's new library is read on the server's thread and swapped in on Home. While a game runs only Now playing and a rate-limited screenshot are answered ([WEB-COMPANION.md](WEB-COMPANION.md), [user/web-companion.md](user/web-companion.md)) |
| Settings → System → Backup & reset | `src/backup.rs`, `backup_page.rs`, `app_backup.rs` | pocket-ui's own files only, to `/storage/roms/backup/pocket-ui/<date>`; pocket-ui starts again after a restore or reset ([SYSTEM-PAGES.md](SYSTEM-PAGES.md#backup--reset)) |
| Settings → System → Storage, the low-space notice | `src/storage.rs`, `storage_page.rs`, `app_storage.rs` | The card is walked like `du` on a thread every time the page opens (`storage::measure_on_open`: not when the last walk is under 10 s old and nothing changed; the last numbers are kept in memory and shown at once with a quiet "Updating…", "Counting…" only until the first walk is in) and again after a game or a delete; the clean-ups (automatic states of games not played for some months, the picture cache, logs of earlier starts, old backups) run on a thread and check every path again before deleting (`storage::clean`); saves are on no list. The free space is a statvfs on a thread after the first frame and after every game; under 1 GB a notice on the shelf once, remembered in `storage.json` until there was 1.5 GB again. The SD card check row (`card_health`) is read on a thread each time the page opens: `/proc/mounts` (read-only?), `/sys/fs/ext4/<device>/errors_count` and `dmesg` for `EXT4-fs error`, `Remounting filesystem read-only` and `I/O error` on the card; read-only, no repair. Nothing is read on the UI thread ([user/storage.md](user/storage.md)) |
| Games recognised by their checksum (cartridges) or their serial (PlayStation 1/2); the card's "Wrong pictures? Look up again" | `src/rom_id.rs`, `disc_serial.rs`, `archive.rs` (`rom_crc`), `app_identify.rs`; used by `covers.rs` (`pick`), `game_pictures.rs`, `gamedb.rs` (`resolve_rom`), `curator/mod.rs`, `auto_texts.rs`, `web/api.rs` | One thread at the lowest CPU and I/O priority (`web::lower_priority`), started where the automatic covers start (3 s after the first frame, a changed game list, Wi-Fi up, after a game it stopped); the covers and descriptions wait for it and are asked for under its names. Cartridge systems only: a zip's CRC-32 from its directory (one or two small reads at the end of the file, nothing unpacked), a loose file up to 64 MB read once in 1 MB chunks (plus the CRC without a copier header or in N64 big-endian order). Cached by path, size and mtime in `<data>/identify/checksums.tsv`, written every 50 files; a game that starts stops it after the file it is on. The No-Intro DAT of a system is fetched from libretro-database once when online (and after 180 days), parsed to `<data>/identify/<list>.tsv`. The UI thread only keeps the names (`Identify::names`, a map); Curator's catalog thread, the cover thread and the web companion read them from the card (`rom_id::known`: two small files, no game file). A PlayStation 1/2 disc has no checksum (images are hundreds of MB) but carries its serial in its file system's `SYSTEM.CNF` (`BOOT`/`BOOT2`, "SLES_556.73"): `disc_serial` reads it with a few sector reads (the image's layout, the volume descriptor, the root directory, that one file; `.iso`, `.bin`, the `.cue`/`.m3u` that names them, MODE2/2352 included; a `.chd`/`.cso`/`.pbp` is not opened), caches it in `<data>/identify/discs.tsv` and names it from libretro-database's Redump list (`<data>/identify/<list>.serials.tsv`), fetched like the No-Intro ones. The shelf shows that name without its region tags (`library::load_with`; a gamelist's name comes first), with "(Disc 2)" kept. A changed or removed recognized name, including a manual cartridge lookup, keeps a shelf-rescan request across stopped identification runs; an unchanged resumed result still completes that refresh, and an already busy scan defers the request instead of dropping it. A game named for the first time leaves the `.missing` lists, so its pictures are asked for again. Look up again: on a thread with the cover claim, `rom_id::forget` (its pictures, `.missing` lines and checksum), a run for that game (the list fetched if missing), then `game_pictures::run_until` for it; the card's toast says how it went (`rom_id::Again`) ([CURATOR-DATA.md](CURATOR-DATA.md#identification-by-checksum), [user/library.md](user/library.md#wrong-pictures-look-up-again)) |
| EmulationStation gamelists (names, the card's description and facts, covers, the favorites and ratings offer) | `src/gamelist.rs`, `gamelist_page.rs`, `app_gamelist.rs`; the names in `library.rs` (`load_with`) | `gamelist.xml` of each system is read, never written: from the ROM folder, ES's `gamelists` folders and ES-DE's. A tolerant scanner, not an XML parser: a bare `&`, a cut-off file, Latin-1 or UTF-16 still give every complete game before the damage; over 32 MB is skipped. It runs on the scan's threads (`library::load_shown`, which is what the cache's start, the rescan, Add games and the web companion call) and on one `gamelist` thread after the first frame (`App::gamelist_refresh`: at the start, after each rescan and Add games, and when the page opens); each file is parsed once per change (path, mtime, size, ROM folder) and kept in memory and in `<data>/gamelist-cache.json`, which is written on a thread of its own and never needed for the first frame. The names are in the game list itself (so in `library-cache.json`, the search, Recent and Favorites): `gamelist::better_name` replaces a cryptic file name ("sf2ce") only, never a name that already reads as one. The card (`fill_card`), the covers (`gamelist_cover`, a lookup once per game, only for a game with no box art of its own and a picture of at most 1 MB) and the page read the `Index` the thread sent (`UserEvent::Gamelist`). Without a gamelist all of it is a few `stat`s per system on that thread |
| Screenshots gallery (Settings → Games → Screenshots, the game card's "Screenshots (N)" row) | `src/gallery.rs`, `app_gallery.rs`, `game_card.rs` (`RowKind::Screenshots`), `screenshot.rs` (the files it writes), `picture_loader.rs` | The folder `/storage/roms/screenshots/<system>/<game> <date> <time>[ (n)].png` is read on one `gallery` thread (`App::gallery_refresh`: after the first frame, after every game, when the page or the card's row opens) into `gallery::Index` in memory: the card's row and the Settings row read a count from it and touch no file. The name is read from the end, so "Game" never lists "Game 2"; other files are not listed, links are ignored. The pictures go through the shelf's picture loader thread (`App::request_picture`, `picture_loader::load_fitted`) into two caches of their own (`gallery::PictureCache`: 24 thumbnails and 3 full-size pictures, the oldest let go, at most 6 and 3 requests out at a time so a fast scroll never queues what is long off screen); the full view reads the 24-byte PNG header on the UI thread (as the card's pictures do) to show a small picture at a whole multiple of its size. Delete is one `unlink` after the second A; Y copies the file to `game_pictures::path(Kind::Snap, …)` on a thread (`gallery::use_on_shelf`, written atomically). The page opens over the card (`gallery_over_card`) or as `SettingsPage::Screenshots` ([user/playing.md](user/playing.md#looking-at-your-screenshots)). ROCKNIX's es_systems.cfg has a system `imageviewer` ("Screenshots") on the same folder with `.png` among its extensions; the library leaves it out (`library::NOT_GAMES`), so the pictures are never games on the shelf |
| The info page (X on a game in Minimal, Cards and Split) | `src/game_info.rs`, `game_card.rs` (`Card::info`, `show_info`, `paint_info`), `app_card.rs` (`show_card`, `paint_card`) | `show_card` calls `Card::show_info` in those looks instead of building a box (the `details_page` setting is not touched). `Card::press` takes Up/Down/L1/R1 (the description scrolls) and SELECT (the full page; SELECT there returns, `Card::info_look`) first, the rest goes the page's way. One screen from `CardData` as the box's back reads it (`game_box::about`); `CardData::broken` is painted as a complete Problem explanation outside the facts in all three card surfaces (the box front gets an overlay label), keeping Connections indices intact; the app fits the cover into `game_info::cover_room`. `--snapshot info:<look>:<system>:<name>[:<keys>]` |
| Case stickers (Completed, 100 %, Favourite, Playing now, Game of the year) | `src/case_stickers.rs` (kinds, `CaseStickers` in `library_meta::GameMeta::case`, painting), `app_case_stickers.rs` (`stuck_front`, `stuck_spine`, `bare_front`), `app_curator.rs` (the card's Stickers sheet), `library_sync.rs` (field `case`) | Put on from the card's Stickers sheet (its first five rows; Left/Right on Game of the year change the year, `curator_page::Sheet::adjust`) and applied in memory at once. `set_case_stickers_soon` queues `library.json` on the existing card writer without a UI-thread SD flush; checked completion reports write failures through `app_curator::Event` even after the sheet closes. Coalesced checked writes resolve older callbacks as `Superseded`, keep only the latest callback, and deliver its final I/O result; a subsequent unchecked write retains that callback. Snapshot and application exits settle pending writes. A case's pictures are made with them where the bare ones are made (`stand_front`, `shown_pics`, `spine_pic`, `stand_pic`): the stickers are part of `CaseKey`, so putting one on makes that case's pictures again and no frame paints one. A case-artwork revision in `LibraryMeta` changes after sticker edits, sync batch mutations or loading metadata anew; the Shelf transfers it to its existing artwork generation so both cached standing spines and row tiles refresh without changing lists. The cartridge inside takes its label from the front without them (`bare_front`); each bare source is retained while its decorated front is externally owned by a bounded front cache, the box or the launch animation, and released on pruning once those owners drop |
| Frozen games (Settings → System → Frozen games) | `src/frozen.rs` (`Mark` in a recent game's entry, `scan`, `at_launch`), `src/app_frozen.rs` (a thread after the start and each game, the launch's note, the spinner), `app_cabinet.rs` (`ajar_spine`, `paint_ajar_game`) | Only packaging: the state, the normal `launch` and its resume stay as they are. A frozen game's launch skips the Cozy hold; `POCKET_SKIP_RESUME` and `standalone::skip_next_resume` leave out a state made for another file, which stays for Load |
| Cozy extras (Settings → Look → Cozy extras; Sound & feel's Playful cases, Old-TV switch-on, Case sounds; Sleep & power's slideshow, `Extra::home`) | `src/cozy_extras.rs` (`Extra`, `Extra::PAGE_ROWS`, `Extras` in `settings::Settings::extras`, the atomic `on`), `app_cozy_extras.rs` (the page, `App::switch_extra` for every row that switches one) | Each little touch reads its own switch where it is drawn or heard: `ui_sound::play_after` (`is_case_sound`), the launch env (`POCKET_LAUNCH_CRT`, `POCKET_LAUNCH_CRT_SILENT`), `paint_cabinet_tv`, `case_stickers_of`, `app_cabinet::printed_disc`, `app_connections::fill_connections`, the Curator list's Rediscover row, the pause menu's memory card (`app_save_media`), the playful animations (`playful::on`, read in `app_cabinet.rs` and `game_box.rs`; its start follows Animations until switched: `Extras::playful_animations` is None then). A switch is used at once (stickers and discs: `cases_look_changed` makes the cases' pictures again) and saved with the other settings; Reset settings starts from `Extras::default()` |
| The TV on the cabinet (Cabinet look) | `src/cabinet_tv.rs` (the template, the CRT look, `source`, the `cabinet-tv` thread `Loader`), `app_cabinet_tv.rs` (size, place, crossfade), `app_cabinet.rs` (`paint_cabinet` calls it before the cases out; `paint_out` keeps the case left of it) | The UI thread asks for the selected game's screen at the glass's size; the thread finds the picture (screenshot, the resume preview or kept frame, the cover, else the console), decodes it and puts the CRT look on it; only the newest wish is made when several wait. The last 12 screens are kept. Frames go on while one is being made or a crossfade (220 ms) runs (`CabinetUi::animating`); at rest the TV with its screen is one kept picture |
| Light by time of day and decorations (Shelf look, Cabinet) | `src/room_light.rs` (the day's light, the tint), `room_deco.rs` (milestones from `library_meta`, placing), `app_room_extras.rs` (reading them, the minute wake-up, the lamp's glow, the framed "100 %"), `app_cabinet.rs` (`paint_prop`, `paint_shelf_props`, the wood's caches) | No work per frame: the clock is read at most every 20 s, the library when its case stickers changed or every 20 s; a change of light or milestones makes the tinted wood and the Shelf look's tiles again once. While a Shelf or the Cabinet shows, one wake-up a minute (none with Motion off) asks for a frame only when something changed; its deadline is independent of the optional light cache, so Decorations alone never rescans the library per input or animation frame |
| The game box (X on a game; SELECT switches to the page and back, kept in settings.json's `details_page`) | `src/game_box.rs`, `game_card.rs` (`Card::boxed`, `paint_box`), `app_card.rs` (`paint_card`), `app_cabinet.rs` (`box_front`) | X opens the card with a `game_box::BoxView` in it; `Card::press` takes Left/Right (turn over), Up/Down/L1/R1 (the back scrolls) and SELECT (the page; SELECT on the page builds a new box, both reported as `Out::Details` and saved) first, the rest goes the page's way (status, note, emulator, the sheets painted over the box). The front is the case's front made as the Shelf look makes its fronts (`stand_front` at the box's height), for a cartridge a cardboard box with the cover; the back's frame is a nine-slice PixelLab template (`assets/media/back`). While it turns, each face is painted to a scratch canvas and drawn narrowed, so a turn is ~17 frames of one face; at rest one face is painted as any page. `--snapshot box:<system>:<name>[:<keys>]` (`r` turns, `d` scrolls; `POCKETUI_SNAPSHOT_FLIP_MS`) |
| Launch animation (Settings → Sound & feel → Animations: Cozy / Quick, `settings::Animations`, default by look) | `src/app_cabinet.rs` (`launch_animation`, `paint_insert`), `cabinet.rs` (`insert_at`, `INSERT`, `IN_AT`), `ui_sound.rs` (`Sound::Insert`, `going_in`, `play_after`, `close_when_done`) | Called by `App::launch` just before the game's process is started (Cabinet and Shelf looks, only when the case is on screen): in Quick nothing waits for it; in Cozy (`cabinet::INSERT_COZY`, the case's and the medium's sounds, `motion::set_cozy` slows the other timed animations) a pocket-core game gets `POCKET_LAUNCH_HOLD_UNTIL` (`startup::HOLD_UNTIL`) and draws nothing, silently, until the fade has run or a button is pressed (`pocket_core.rs` `hold`). Its sounds are queued at A, each with how long to wait (`ui_sound::play_after`); the sound thread then closes its output once it has played, without giving back the quantum it lowered for a pocket-core game (pocket-core sets the same one; giving it back late could undo pocket-core's). Any button over the starting game skips it. Then, with Motion Full (`POCKET_LAUNCH_CRT`), pocket-core plays the old-TV power-on over the game's first 300 ms (`crt_on`, drawn by `Overlay::draw_solid` inside the picture's rectangle; no wait, the game already runs; a button cuts it; hook name `crt-on`, where pocket-core plays `Sound::CrtOn` into the game's sound through its chime, at the UI sounds' volume). `--snapshot cabinet:<keys>:<ms>:<ms since A>`, `shelf:<system>:shelf:games:<row>` with `POCKETUI_SNAPSHOT_LAUNCH_MS` |
| Hide or delete a game (the game card's last row) | `src/game_files.rs`, `game_card.rs` (`RowKind::Manage`, `Mode::Manage`, `Mode::Delete`), `library_meta.rs` (`hidden`), `library.rs` (`without_hidden`, `load_shown`), `app_storage.rs` | Hidden is a flag in `library.json`. Every list is made from the game list without hidden games: `library_cache::start`/`rescan` and the web companion read it through `library::load_shown`, and hiding takes the game out of the list on screen at once (the same swap as a rescan, `library_rescanned`); the search, Curator, Favorites and the console pages are made from that list. The recent games (Home, the switcher, On start, the Look previews) skip hidden paths (`App::recent_visible`). Play history keeps counting the game. Delete works out the files on a thread (`game_files::plan`: a sheet's tracks, a playlist's discs, a folder game's folder; saves and states found by name) and deletes on a thread after the second A, never a system's folder (`game_files::delete`); saves and states only when asked ([user/library.md](user/library.md#hide-or-delete-a-game)) |
| Speed guard | `src/trace.rs`, `app_trace.rs`, `scripts/latency.sh` | Trace lines from both programs, a benchmark with baselines ([LATENCY.md](LATENCY.md)) |
| The manual | `src/manual.rs` | `docs/user/*.md` compiled in, laid out per page when it is first opened |
| Feature registry, first-start profiles, Tour, "New in this version" | `src/features.rs`, `feature_previews.rs`, `setup.rs` (`Step::Play`, `Step::Cards`), `app_tour.rs` | One list (`features::FEATURES`) of every optional feature: its switch over a copy of the App's settings (`Switches`), what Simple and Enthusiast set, the version it came in, its card. A profile or card only changes that copy; `App::adopt_switches` uses and saves each switch as its Settings page does, so there is no state of the tour's own beyond `seen_version` and `tour_later` in settings.json. A card's preview is painted when the card is first shown and kept while the cards are open; at a normal start only `features::unseen` (two version strings) runs, after the first frame ([DEVELOPING.md](DEVELOPING.md#adding-a-feature-to-the-tour)) |

## Data on the device

| Path | Contents |
|---|---|
| `/storage/pocket-ui/` | `releases/<version>/` (binaries), `current`, `previous` and `known-good` links, the launcher, `es-fallback` (why the launcher opened EmulationStation by itself). [UPDATES.md](UPDATES.md), [Rescue path](#rescue-path) |
| `/storage/.config/pocket-ui/` | `settings.json`, `recents.json`, `favorites.json`, `library.json` (status, ratings, notes, stickers), `curator.json` (switches, smart shelves and their order), `search.json`, `performance.json`, `game-settings/`, `slots/`, `fallback.json`, `covers/`, `snaps/` and `titles/` (the collection's screenshots and title screens, `game_pictures.rs`), `identify/` (the games' checksums by path, size and mtime, and the No-Intro lists fetched for them, `rom_id.rs`; the discs' serials and the Redump lists, `disc_serial.rs`), `wikipedia/`, `store/`, `display.json`, `battery-history.bin`, `battery-games.json` (the battery per game), `port-cpu.json` (which ports keep a core busy), `play-log.bin` (play history, [HISTORY.md](HISTORY.md)), `crash.log`, `last-game.json` and `started-boot` (On start), `players.json` (who is which player, from the pause menu's Players page), `library-cache.json` (the game list of the last scan, for the first frame: [Start](#start)), `gamelist-cache.json` (what the EmulationStation gamelists said, per file and its size and mtime: not read before the first frame), `storage.json` (whether the low-space notice was shown), `web.json` and `web/browsers.json` (the web companion's switches; the paired browsers' token hashes and the CSRF key, folder 0700), fonts and frames added from the Store |
| `/storage/.config/pocket-ui/library-sync/<device>/` | Settings → Sync → Library data: each device's `library.json` (its curation, every value with time and device) and `history.json` (every session it knows), the Syncthing folder "Pocket library data"; a device writes only its own folder ([SYNC-CURATION.md](SYNC-CURATION.md)) |
| `/storage/roms/backup/pocket-ui/<date>/` | Backups of pocket-ui's settings and library notes, Settings → System → Backup & reset ([SYSTEM-PAGES.md](SYSTEM-PAGES.md#backup--reset)) |
| `/storage/.config/pocket-ui/retroachievements.json`, `achievements/` | The RetroAchievements sign-in key (0600, never the password), and per game the tally, the set kept for offline play, badges and unlocks waiting to be sent ([HOST.md](HOST.md#retroachievements-srcachievements)) |
| `/storage/roms/reports/` | Settings → Help & tools → About → Report a problem: `report-<date>-<time>.zip` with versions, logs, settings and emulators for a bug report, private details replaced first (`src/report.rs`, written on a thread of its own by `app_report.rs`; [troubleshooting.md](user/troubleshooting.md#report-a-problem)) |
| `/storage/roms/<system>/` | Games, and battery saves (`.srm`, `.sav`) and cartridge clocks (`.rtc`) in the format RetroArch uses |
| `/storage/roms/savestates/<system>/` | Save states and their previews |
| `/var/log/pocket-ui.log` | The log (in RAM). pocket-core writes into it too |
| `/storage/.config/pocket-ui/logs/` | The same log, `kernel.log`, `sensors.log`, `net.log` (the journal of NetworkManager, iwd and tailscaled from one `journalctl -f` child, plus the names of the saved connections and of iwd's known networks at the start and on change, never file contents, secrets blanked: `src/net_log.rs`) and `shutdown.log` (written by pocket-ui itself at each power off or restart: the RTC alarm's disarm and readback, the command and its end, fsynced per entry, 128 KB per boot, `src/shutdown_log.rs`) on the card, three boots, 5 MB per file, fsynced every few seconds: written by `pocket-logd` (`pocket-core --log-daemon`, started by the launcher), `src/logs.rs`. A boot without `clean-exit` is reported in `crash.log` at the next start |

## Display

The Mini V2's panel is 1080 x 1240 portrait; sway rotates it to **1240 x 1080 landscape** (about 8:7). The whole
UI is drawn at the window's size, which is that on the panel. Games keep their own aspect ratio, scaled by a whole
number where possible ([HOST.md](HOST.md)). The panel is AMOLED, so the UI background is true black.

On a TV through USB-C (DisplayPort alt mode) the window is the TV's mode, 1920 x 1080 in TV mode: the pages lay out
wider at the same type size, pocket-core scales games to 1080 lines. ROCKNIX moves the workspace and darkens the
panel; pocket-ui's `display-out` thread follows sway's output events and repairs what ROCKNIX left out
([DISPLAY-OUT.md](DISPLAY-OUT.md)).

## Other parts

| Topic | Document |
|---|---|
| Volume, brightness, quick settings, time zone, the date and time set by hand, Bluetooth pairing | [SYSTEM-CONTROLS.md](SYSTEM-CONTROLS.md) |
| Sleep and the power key | [SLEEP.md](SLEEP.md) |
| Playing on a TV through USB-C | [DISPLAY-OUT.md](DISPLAY-OUT.md) |
| Updates and the bootloader guard | [UPDATES.md](UPDATES.md) |
| The Store | [STORE.md](STORE.md) |
| Tools, Wi-Fi's saved networks and country, PC streaming pairing, Backup & reset | [SYSTEM-PAGES.md](SYSTEM-PAGES.md) |
| File sharing and SSH | [SECURITY-HARDENING.md](SECURITY-HARDENING.md) |
| The web companion: protocol, endpoints, threat model | [WEB-COMPANION.md](WEB-COMPANION.md) |
| Starting from the SD card next to Android | [INSTALL-DUAL-BOOT.md](INSTALL-DUAL-BOOT.md) |
| Look of the shelf | [DESIGN.md](DESIGN.md) |
| RetroAchievements (rcheevos in pocket-core) | [HOST.md](HOST.md#retroachievements-srcachievements) |

## Background

### Why not port NextUI

NextUI (a fork of MinUI, C + SDL2 + GLES2) is the user-experience reference: one flat list, few settings,
instant resume, a small menu, game-time tracking. It was not ported, for three reasons:

- NextUI runs games only through its own libretro frontend. The standalone emulators ROCKNIX ships for the
  Snapdragon 865 (Dolphin, PPSSPP, Flycast, ARMSX2 and others) cannot run under it.
- ROCKNIX already has one launch interface for all of its emulators (`runemu.sh`). Building on it gives low-end and
  high-end systems one interface.
- NextUI is licensed under PolyForm Noncommercial 1.0. Writing the interface from the ideas, with no code or
  assets taken over, keeps the licence of this project open.

### What ROCKNIX already provides

Checked against the ROCKNIX source and the 20261001 image.

| Layer | ROCKNIX component |
|---|---|
| Display | Mainline DRM (`msm`, freedreno), panel `ch13726a,rpminiv2` |
| GPU API | Mesa freedreno (OpenGL ES) and turnip (Vulkan) |
| Compositor | sway (wlroots), kiosk configuration |
| Input | libinput and InputPlumber, which turns the gamepad into virtual devices |
| Frontend | `essway.service` runs `/usr/bin/start_es.sh`, which starts EmulationStation |
| Game launch | `/usr/bin/runemu.sh <rom> -P<platform> --core=<core> --emulator=<emulator>`, in a `systemd-run --scope` under `games.slice` |
| Per-system settings | `get_setting` and `setsettings.sh` write RetroArch's append config at each launch |

### Language and drawing

pocket-ui is written in Rust and cross-built from a computer. It draws its own interface (hand-drawn, with a
pre-rasterised font) into a Wayland surface through `winit` and `softbuffer`; pocket-core uses OpenGL ES through
`glutin`. Input is read straight from evdev. There is no widget toolkit. The cold start target is a first frame in
well under 300 ms; since the start work moved off the UI thread, pocket-ui's own first frame comes about 10 ms after
`main` in the virtual device and about 40 ms on the Mini V2 ([DEVELOPING.md](DEVELOPING.md#start-time)).

The interface's words are English in the code and German on request (Settings → System → Language, `src/i18n.rs`): the text
painter (`src/text.rs`) looks each string up by its English wording in a table compiled into both programs
(`assets/i18n/de.tsv`) and paints the German one, so the pages themselves know nothing of it. The choice is one field
of `settings.json`; until it is made, the system language EmulationStation set (`system.language`, which the launcher
passes on as `LANG`) decides. What `Text::fit` and `Text::wrap` give back is German already and is painted as it is (`i18n::Cache::is_german`): a second look-up of it would run every pattern of the table over it. See [DEVELOPING.md](DEVELOPING.md#translations).

The text size works the same way (Settings → Look → Text size, `src/type_scale.rs`): the pages ask for the
design's sizes and the text painter paints, measures, fits and wraps each at the size of the setting, with its glyph
cache keyed by the size painted; the pages take their row heights and line steps from the same module, so a page with
Medium or Large text has taller rows and fewer of them. Normal is the design to the pixel. The value is per thread (the UI
thread paints), one field of `settings.json`; safe mode reads that field alone, and pocket-core does not scale its
overlay. See [DESIGN.md](DESIGN.md#text-size).


SM8550 experimental device profiles (`device.rs`: Pocket 6/TOP-DPAD, Nova and the Odin 2 family, [DEVICES.md](DEVICES.md))
reuse the existing layouts. `sleep_settings` normalises their buttons/timers and `sleep` keeps every path at
screen off, with an optional dark-time power-off and no RTC alarm. `power::set_fan`, `fan_curve::Engine` and
`hand_back`, plus launcher recovery, leave SM8550 PWM and thermal trips to ROCKNIX. CPU policies are discovered.

Odin 2 Mini/Portal use Xbox menu defaults (saved choices win); safe mode follows the device labels.
`pad::linux` also reads InputPlumber keyboard HOME/BACK aliases; paddle support is not claimed with the plain ds5 target.

### Small-panel UI frames

`frame::Presenter::draw_scaled` paints into `scaled_layout::ScaledFrame`'s reusable logical buffer on outputs up to
720 px tall, then area-reduces into the physical softbuffer surface. Native large frames paint directly into the
surface. Only existing redraw requests invoke this work. Layout measurements, animation buffers and touch HitMaps
use logical dimensions (`App::screen_size`); normalised touch events are mapped into that same space. Snapshots and
safe mode share the scaler. Core video and core pointer coordinates remain physical. Details: [DESIGN.md](DESIGN.md#small-panel-presentation).

## Android feasibility branch (2026-10-28)

The separate [Android spike](../android/README.md) shares the CPU Canvas, fonts, palette, image decoding, i18n and
shelf item model through source inclusion. Its NativeActivity entry and small Java bridge own the Android window
lifecycle, legacy ROM-folder selection and RetroArch intents. It does not start `src/main.rs`, ROCKNIX services or
pocket-core. `TextSize` is isolated in `src/text_size.rs` and re-exported through settings without a desktop API change.
The locale fallback skips system.cfg on Android; the Android entry sets the actual language from Java's Locale.

The `src/platform/` seam owns OS I/O at the existing call boundaries. Cover/picture/ROM metadata transport calls
`platform::http`; the Linux adapter retains the previous curl process arguments, timeouts and fallback.

## Android launcher and in-app player

`src/lib.rs` selects `linux_modules.rs` or `android_modules.rs`. The Android entry depends on the normal shared
library; `platform/` supplies Android SAF/JNI, HTTPS/system queries and AAudio. `library_names.rs` and
`look_settings.rs` hold shared pure rules/values, re-exported by Linux. The complete composition, supported subset,
intent table and build/emulator scripts are documented in [ANDROID-MVP.md](ANDROID-MVP.md) and
[the Android build guide](../android/README.md). The older feasibility section above records the original spike.

`PlayerActivity` runs in a private `:player` process with a separate native library (no winit). It owns an EGL/GLES 3 surface and keeps all `libretro::Core` calls on its GL thread. It shares Linux's SRAM/RTC, slot/resume and option code. `audio.rs` is the common queue/resampler/rate controller; `platform/` selects ALSA/Pulse or AAudio. The native launcher copies classic SAF cartridges on a worker into private files keyed by document identity, then starts the private player Activity. Heavy-core hardware negotiation is still explicitly unavailable; the small bundle contains mGBA, Nestopia, bsnes and Gearsystem. Platform-specific screenshot hints also come from `platform/`, preserving the Linux button instructions. Android details and its remaining limits are in the linked guide.
