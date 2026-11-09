# Device tests for a person at the Mini V2

> **Status: internal checklist** from the author's test device. Boxes marked `[x]` were checked; the rest are open.
> A box can be open although a later run covered the same ground; what has been proven, and how, is in the ledger
> [STATUS.md](STATUS.md), which wins when the two disagree.
> Replace `<device-ip>` with your device's address.

Everything here was built and checked in the virtual device and, where possible, over SSH on the
real one. These are the things only someone holding the device can check: what you feel, hear and
press. Detailed procedures: docs/SLEEP.md, docs/SYSTEM-CONTROLS.md, docs/HOST.md.

## Before
- `/storage/.config/pocket-ui/no-autosuspend` is set, so the device never deep-sleeps on its own
  while nobody is there. Remove it to test automatic sleep:
  `ssh root@<device-ip> rm /storage/.config/pocket-ui/no-autosuspend`, then restart pocket-ui.
- Restart and Power off end the session. With the dual-boot install, Power + Vol Up brings pocket-ui back; after a
  RAM boot (`fastboot boot`) a restart lands in Android and ROCKNIX has to be started from the computer again.

## Where the UI is: `/tmp/pocket-ui.status`

A script or an agent that presses buttons reads where pocket-ui is first, and never presses blind: B and A mean
something different on every page, and a press on a dark screen only wakes it. pocket-ui keeps one line in
`/tmp/pocket-ui.status` (RAM, never the card; `<dir>/ui.status` under `--test-driver`), replaced whole and only when a
field changes (`src/ui_status.rs`, `src/app_where.rs`).

```
v=1	rev=42	pid=1234	place=settings:List Look	cover=none	sleep=awake	game=none
```

TAB-separated `key=value`; the first three are always there and always in that order, a reader ignores keys it doesn't
know. Values are machine codes, never translated text, and never secrets: no network names, passwords, addresses, codes or
game names.

| Key | Values |
|---|---|
| `v` | The schema, 1. A new number only when a field changes its meaning or goes away |
| `rev` | One more with every write, from 1 in each pocket-ui process |
| `pid` | pocket-ui's process: a file whose pid is gone is stale |
| `place` | The page in front, as the navigation explorer's `@where` names it: `shelf:Home`, `shelf:Games(3)`, `settings:Groups`, `settings:List Look`, `settings:Updates`, `settings:Controllers (pause)` (opened from the pause menu), `settings:Manual (setup)`, `setup:wifi`, `setup:wifi (settings)`, `pause:Main`, `pause:Options`, `pause:Big`, `card`, `card:Delete` (and `Status`, `Note`, `Choose`, `Manage`), `box`, `console`, `this game`, `runs on`, `curator:Lens`, `search`, `recent games`, `recent games (game)`, `tour`, `kids:shelf`, `kids:pin`, `game`, `app`, `powering off` |
| `cover` | What is painted over the page, comma-separated, or `none`: `where was I`, `notice`, `quick`, `time` (`time:regions`, `time:zones`, `time:search`, `time:clock`), `power`, `leave`, `slideshow` |
| `sleep` | `awake`; `waking` (just woke: presses are still swallowed for 0.6 s); `light` (dark: a press only wakes it); `deep`; `powering-off` |
| `game` | `none`, `starting`, `playing`, `paused` (the pause menu), `quitting` (saving on its way out), `app` (a tool, PC streaming) |
| `update` | Job phase: `idle`, `confirming`, `checking`, a worker phase, or the last result (`checked`, `installed`, `os-staged`, `failed`) |
| `update.job`, `update.kind`, `update.version` | Process-local job number or `none`; `check`, `app`, `system`, or `none`; public target version or `none` |
| `update.bytes`, `update.total`, `update.bytes.scope` | Bytes received and expected for the **current file**, with scope `current-file` or `none`. Brick components reset this counter; it is not a whole-package percentage |
| `update.cancel`, `update.cancel.requested` | `unavailable`, `effective`, or `too-late`; whether the player requested it (`yes` / `no`). A late request does not interrupt the existing commit sequence |
| `update.notice`, `update.notice.reason`, `update.notice.version` | `stopped`, `waiting`, `checking`, or `shown`; the suppression reason; pending or most recently consumed public version, or `none` |
| `update.notice.schedule` | The actual hint-schedule phase: `stopped`, `waiting`, or `checking`, including when a previously consumed notice is still `shown` |
| `update.milestone`, `update.health` | Read-only installation evidence (`unobserved`, `not-pending`, `pending`, `confirmed`, `stable`) and this process's health-watch observation |
| `update.pending`, `update.pending.version` | Pending marker ownership (`none`, `ours`, `other`, `unreadable`) and its public version, or `none` |
| `update.recovery`, `update.recovery.layout`, `update.stability.scope` | Read-only layout/marker classification; `rocknix`, `brick`, `host-owned`, or `unobserved`; known-good evidence applies to a `release` or `version`, or is `none` |

Update fields extend this same version-1 line and revision counter. They never write a second status file or add a
durable marker. The projection reads cached worker observations; it performs no filesystem or network work on the UI
thread. Unknown or unobserved evidence stays explicit. Recovery classifications describe existing files and links;
they never choose or perform recovery.

`confirmed` records the existing first-frame/player health check. `stable` records agreement with the launcher's
existing known-good marker, not elapsed UI time. ROCKNIX records a specific release directory in a symlink; Brick's
marker contains a version and its timer does not itself wait for a frame. A same-version Brick reinstall therefore has
only version-level known-good evidence. These formats and the installer's writes, flushes, cancellation checks and
rename order stay unchanged.

Worker phases are `starting`, `preparing-layout`, `connecting`, `fallback`, `downloading`, `checking-download`,
`preparing`, `checking-assets`, `unpacking`, `validating`, `moving-assets`, `flushing`, `activating`, `pruning`,
`staging-system`. Notice reasons are `none`, `off`, `kids`, `stopped-after-kids`, `development`, `host-owned`, `mock`,
`request`, `already-said`, `game`, `job`, `scheduled`, `backoff`, `shelf-covered`, `wifi-off`, `input`, `ready`.
`already-said` explains the consumed notice while its daily check is still scheduled. A request already out when
checks are switched Off remains `checking` with reason `off` until its ignored answer arrives. Before its due tick,
a Kids Mode schedule is still `waiting` with reason `kids`. `stopped-after-kids` describes
the retained stopped schedule after Kids Mode ends; this observation does not restart it.

Health codes are `unobserved`, `watching`, `confirming`, `not-pending`, `confirmed`, `timed-out`, `broken`.
Recovery codes are `unobserved`, `host-owned`, `not-installed`, `legacy-flat`, `clean`, `staging`, `ready`,
`activation-interrupted`, `asset-recovery`, `confirmation-pending`, `confirmed-marker-retained`, `incomplete`,
`unreadable`. Inspection runs off the UI thread every five seconds and after job completion; it can lag a marker
change or sample an install in progress. It is read-only evidence and never drives boot recovery. A terminal job
retains its last file counters and job number until the next task/confirmation; cancellation is then `unavailable`.
Mock/snapshot runs leave installation evidence `unobserved`. A fatal health-watch exit can happen before another
UI turn publishes its final code; check the pid before acting on the file.

pocket-core's own `/tmp/pocket-core.status` uses the same head (`v`, `rev`, `pid`) and syntax.

`scripts/ui-status.sh` reads it on the device; `scripts/dev.sh` runs it over SSH:

```sh
scripts/dev.sh status                               # shelf:Home  sleep=awake game=none rev=7
scripts/dev.sh expect 'settings:*' 5                # waits for the page (instead of a fixed sleep), exit 1 on timeout
scripts/dev.sh send --at 'settings:Groups' down a   # waits for the page first, else presses nothing (exit 3)
scripts/dev.sh send wake                            # the screen on from light sleep, no button pressed
scripts/dev.sh send --launch a                      # A where it starts a game: only with --launch
```

`send` refuses `a` where it starts a game (a shelf, recent games, the game card or box, the child's shelf, with nothing
over them) unless `--launch` is given, and any key while the screen is dark or still waking (exit 4: `wake` first, never
`b`). The pattern is a shell glob on the page and its covers as `@where` writes them: a notice up makes `shelf:Home /
notice`, which `shelf:Home` doesn't match. `scripts/device-check.sh` wakes with `wake` and checks it is on the shelf
before a game's buttons. A pocket-ui older than the status file has none: `status` says so, `send` presses other keys
blind with a warning and refuses `a` without `--launch`.

## Where the core is

`/tmp/pocket-core.status` is a read-only observation of pocket-core, using the same v1 TAB-separated
`key=value` line and `v`, `rev`, `pid` head as `/tmp/pocket-ui.status`. The shared `ui_status::Writer`
replaces the whole file by atomic rename on tmpfs, with no `fsync`, only on change. Revisions start at 1
in each process and advance only after successful writes. Ignore unknown keys. A missing file means
unknown (including an older core); a file whose `pid` has exited is stale. It remains after exit.
Neither UI version requires this file, so updates may run old UI with new core or new UI with old core.
No game names, paths, translated text, account data or command payloads appear in it. Host runs with
`POCKET_CORE_TEST` set use `<TMPDIR>/pocket-core.status`, matching their isolated command FIFO;
normal runs always use the fixed `/tmp` path, regardless of `TMPDIR`.

| Field | Machine codes / meaning |
| --- | --- |
| `play` | `playing`, `held`, `menu-paused`, `asleep`, `dark-playing`, `exiting` |
| `quit` | `none`, `save`, `skip-resume`, `for-switch`; the existing FIFO still says `quit`, `quit nosave`, `quit handoff` |
| `menu` | `none`, `paused`; waking from sleep preserves an open pause menu |
| `asleep` | `off`, `on`; independent of the pause menu |
| `screen` | `on`, `dark`; `dark` means the core's screen-off flag, not a backlight measurement |
| `hold` | `off`, `on`; startup presentation hold; emulation may run behind the insert animation |
| `slot` | Current slot, 1–8, as on the FIFO and in the menu |
| `disc` | Last reported `none` or current/count, 1-based; current `0` means unknown |
| `save-trouble` | `none`, `full`, `unwritable`, `unreadable`; last observed battery-save health, not proof an exit save completed |

Dominant `play` precedence is exiting, asleep, menu-paused, dark-playing, held, playing. The other flags
retain combinations such as sleeping behind an open menu. This is a projection of existing flags,
not a second controller. `rev` is not a command counter or acknowledgement: repeated pause, unknown
commands and read-only requests do not change an unchanged line. A quit reports intent before the
save; the existing handoff pipe and save-error notice remain authoritative for its result.

The core publishes before its first frame, after command-driven changes, after frame-driven changes
such as releasing a startup hold, and before exit. An unchanged frame compares a small value without
formatting, file I/O or querying the emulator. Failed status writes never affect play or saving and
retry on the next state event or command. There are no status writes or cleanup after the handoff
announcement, so a retiring core cannot overwrite or remove the next game's status.

Host verification: the `core_contract_` tests run the actual FIFO dispatch against a stub libretro
core with no compositor, audio device or player files; they characterize pause/sleep/wake, permissive
slot parsing, picture defaults, strict quits and batch termination. `core_command_` tests cover the
wire grammar, including literal-space payloads and test-only commands. `core_status_` covers every
combination of the four flags, revisions, failed-write retries, and old-publisher teardown.

## Sleep and power
- [ ] Power key on the shelf: screen off, device sleeps; Power again: back, sound at the old volume.
- [ ] The same in a GBA game: it continues where it was, sound right away, no crackle.
- [ ] Idle 3 min: screen goes dark (light sleep); any button wakes it. Then 2 more min: deep sleep.
- [ ] Settings > Playing > Battery: the graph fills over time (needs ~20 min on battery for "time left").

## Buttons and display
- [ ] Vol± on the shelf and in a game: volume changes, the bar shows it.
- [ ] Start+Vol±: brightness. Select+Vol±: warmer/colder (works system-wide here: gamma ramps found).
- [ ] Settings > Look > Night light ([user/night-light.md](user/night-light.md)): `grep "display look" /var/log/pocket-ui.log`
  says whether the screen takes gamma ramps (then everything is warmed) or the software tint (menus and pocket-core
  games only; the page says so). On: the colours turn warm over ~1.5 s, no jump; Off: back. Scheduled with From/Until a
  few minutes ahead (30-minute grid: set the clock with `POCKETUI_CLOCK_SHIFT` or wait for a half hour): it switches
  on its own at the minute, also with a game running (pocket-core and, with gamma, RetroArch/PS2), and `top` shows
  pocket-ui at 0 % between the switches. Sleep across a switch time (power key, wait, wake): right colours after waking.
  Together with Select+Vol warmth: warmer than either; Night light Off: exactly the chosen temperature again.
- [ ] HOME on the shelf: quick settings with real values; Bluetooth on/off. (Wi-Fi off ends SSH.)
- [ ] Settings > System > Time > Date and time, Wi-Fi off, the clock made wrong first: the picker opens at the build
  day, A sets the clock and the RTC (the time is right after a reboot without Wi-Fi), the orange line goes away, and
  network time corrects it once Wi-Fi is back. Steps: [SYSTEM-CONTROLS.md](SYSTEM-CONTROLS.md#only-checkable-on-the-device-test-list), item 11.
- [ ] Are the face-button labels right? Wi-Fi scan says X, the top button should be X.

## Feel and sound
- [ ] Rumble: a game that rumbles (PS1 with DualShock, GBA rumble carts); Extras > Rumble Low/Medium/High.
  Quick check over SSH: `/storage/pocket-ui/pocket-core --rumble-test 700`.
- [ ] Settings > Sound & feel > Haptics On: a short tick while moving in menus. Pleasant or annoying?
- [x] Settings > Sound & feel > Sounds (on by default, 70 %): clicks on the shelf, in Settings, switcher, quick settings.
  2026-10-22 at 20 % system volume: loudness and sound right, slightly late (about 55 ms queued), not disturbing.
- [ ] Sounds in the pause menu (Error on Load of an empty slot), Vol± tick; none while playing, none when muted.
- [ ] Settings > Sound & feel > LEDs (the stick lights; they are off after the first start of this version). Colour: both sticks
  glow, brightness steps are visibly different, "The game's" turns the sticks the system's colour in a game and back
  to warm after it. Battery: green at a good charge. Off: dark. After a restart the same state; after sleep and wake
  the same. What to read over SSH before and after: docs/LEDS.md.
- [ ] Audio at quantum 256 (any pocket-core game): any crackle? If yes: tell me, there is a switch back.
- [ ] Smooth fan (new, on by default): the full recipe is docs/FAN-CURVE.md, "On the device". In short: the Deltarune
  port plays without the fan switching on and off; the trips read 83.5 °C while pocket-ui drives it and ROCKNIX's
  again after sleep, `kill -9`, `kill -STOP` (watchdog) and Smooth fan off; hot/critical never change; from 78 °C full
  speed at once. The trip-profile checks below are for Smooth fan **off**.
- [ ] Settings > Playing > Performance & fan (new). The readout at the bottom: plausible temperature, fan rpm, three cluster
  clocks, battery watts; it changes about once a second. With Smooth fan off, Fan outside games on Cool: within a second the fan zone's
  active trips read 40/48/55/61/66/71 °C and `/tmp/pocket-ui-fan.restore` exists; `hot`/`critical`/`passive` trips
  unchanged (commands in docs/HOST.md, "Settings > Performance & fan"). Start a GBA game: trips back at the system's
  (52 … 85 °C) before the game shows, note gone; quit: Cool again a moment later. Same around light sleep (idle 3 min)
  and deep sleep (power key). `kill -9 $(pidof pocket-ui)` on Cool: the launcher's clean-up puts the system's trips back
  ("restored the fan profile pocket-ui left behind outside games" in `/var/log/pocket-ui.log`), then the restarted
  pocket-ui applies Cool again. Games: performance on Battery, nothing saved for GBA: a GBA game runs with the capped clocks
  (`scaling_max_freq` 979200/1286400/1516800); saved Balanced for GBA in the pause menu: that wins.

## Touch screen
Touch was seen on the real panel once, with a DS game, and was wrong (taps mirrored); the mapping was turned in
`d2f0087` (2026-10-24) and no run since is recorded, so every box below is open. If a check fails, note which corner or edge is wrong; `POCKETUI_TOUCH_ROTATE=0|90|180|270` in
`/storage/pocket-ui/dev.env` (restart pocket-ui) turns the mapping.
- [ ] Shelf: tap a row: it is selected; tap it again: it opens. A tap high on the screen must select what is high, left must be left:
  this is the check of the rotation. If a tap lands mirrored or turned, that is the setting above.
- [ ] Shelf: swipe up and down: the selection follows by rows and stops at the ends. Cards look: swipe left / right.
- [ ] Settings: tap a row, tap it again; swipe; tap the B / A hints at the bottom right.
- [ ] SELECT hint on the shelf (tap): the switcher opens; swipe left / right moves between cards, tap a card, tap A.
- [ ] While a game runs (also at the pause menu) touching the screen must do nothing to pocket-ui. After the game, the shelf answers again.
- [ ] Touch kept read (`fix/touch-reopen`): after a cold boot, `grep '^touch' /var/log/pocket-ui.log` shows `touch: <name>` (and `touch: no touch screen yet` first if the panel was late) and the shelf answers a tap. Then restart the touch driver or reload it (`echo 4-0038 > /sys/bus/i2c/drivers/edt_ft5x06/unbind`, then `bind`; take the real I2C address from `ls /sys/bus/i2c/drivers/edt_ft5x06/`): the log shows `touch: lost …` and, a second or so after the node is back, `touch: … (again)`, and a tap works again without restarting pocket-ui. Each change makes one line, no stream of them.
- [ ] Dropped touches say why: touch the screen while a game runs, with the screen off (Sleep & power: Screen off) and over the pause menu: one `touch: ignored, …` line per spell in the log, not one per touch.
- [ ] Idle sleep: use only touch for longer than the sleep time on the shelf: the screen must stay on. Asleep, a touch must not wake it.
- [ ] NDS (muted: `wpctl set-mute @DEFAULT_AUDIO_SINK@ 1` first and put the old state back afterwards; end the game with
  `echo quit > /tmp/pocket-core.cmd`, none may keep running): touch the bottom screen: the DS pointer
  lands under the finger (corners too); the top screen does nothing. R3 to side by side: the same on the right half; top
  screen alone: no touch. Right stick + R2 still work. Lift a finger quickly (a short tap): it still registers.

## Games
- [ ] SELECT on the shelf: game switcher; HOME+SELECT in a game switches games and resumes.
- [ ] Fast switch (ARCHITECTURE.md, Switching games): SNES → GBA → GB → NES → MD and back in the switcher, each resumes
  where it was left; `grep switch-time /var/log/pocket-ui.log`: `hand-over received` a few ms after `switcher: A`, the
  new `process start` right after, `first frame` well under 500 ms after A. The old game's `pstate.auto` has the time
  of the switch. A switch from PSP/N64 (no hand-over) still works, as does one to RetroArch. Sound has no crackle after
  the switch (`pw-metadata -n settings | grep force-quantum`: 256 while a game runs, back after Quit to the shelf).
- [ ] Quit to the shelf (perf/round2, LATENCY.md): GBA/SNES/GB game, HOME, Quit: the shelf is there at once; start
  the same game again right away: it resumes where it was quit (it waits for the old process, `start waits for the
  old game's process` in the log when quick enough). `pw-metadata -n settings | grep force-quantum` back to ROCKNIX's
  value on the shelf. A PSP or N64 game (no hand-over) quits as before. `recents.json` has the game first after it.
- [ ] Pause menu (perf/round2): HOME in a pocket-core game: the menu is there at once, the dimmed game appears behind it
  a moment later (no flash of the shelf, no half-wide menu); Continue, play, HOME again: the same, no 200 ms wait. Quick
  settings over the game show the game. HOME + SELECT: the switcher's "Now playing" card gets the picture. RetroArch and
  a standalone emulator (PS2/GameCube): the menu still shows the game behind it, Quit still saves.
- [ ] Pictures (docs/user/extras.md): online with covers done, `grep covers: /var/log/pocket-ui.log` shows
  "N without pictures", the shelf note says "Pictures 3 of 40" and `snaps/` and `titles/` fill under
  `/storage/.config/pocket-ui/` (`du -sh` them: about 100 KB per game for 16-bit and GBA games, more for PlayStation).
  Starting a game stops it after the file it is on and it goes on after Quit. Split look: stay on a game with box art,
  screenshot and title screen: after about 2.5 s it fades to the screenshot, then the title screen, then the box art;
  scrolling the list never stalls (no decoding on the UI thread), a game without a screenshot stays on its box art, with
  Motion Off nothing fades. The game card shows both under the facts. Settings > Games > Extras > Pictures
  "Box art": all of the screenshots gone, and nothing but box art downloads; "Off": nothing downloads by itself (the
  log says "covers (...): off"), and the first start waits for that step before it downloads anything.
- [ ] Y on a game: star; the Favorites row appears.
- [ ] N64 (Mario Kart 64), PSP (Bomberman), Dreamcast (Sonic Adventure 2), NDS (New Super Mario Bros.)
  now run on pocket-core: play a bit, HOME menu, Save/Load, Quit and continue.
- [ ] NDS: right stick moves the touch pointer, R2 taps. Does a tap register in a game menu? R3 changes
  the screen layout.
- [ ] Cheats: Contra III or any game with cheats, Options > Cheats, switch one on, does it work?
- [ ] Store: Settings > Help & tools has no Store row (no catalog yet; Tools, PC streaming and the cursor moving through the list are fine). With `"store_url"` in settings.json the row is back and opens.

## DOS, ScummVM, PICO-8, TIC-80 (docs/HOST.md, checked on the virtual device only)
Muted (`wpctl set-mute @DEFAULT_AUDIO_SINK@ 1`, old state back afterwards); games quit with
`pidof pocket-core >/dev/null && timeout 3 sh -c 'echo quit > /tmp/pocket-core.cmd'`. Test content in clearly named
folders only (for example `roms/pc/zz-test-PSR Invaders/`, `roms/scummvm/zz-test-FOTAQ/`), removed afterwards. Before
the ScummVM test, copy `roms/bios/scummvm.ini` aside (pocket-core adds extrapath/themepath/savepath to it) and compare after.
- [ ] Each of the four: frame rate and CPU over 30 s against ROCKNIX's default (`scripts/syscheck.sh bench pc dosbox_pure <game>`
  and `ra`; ScummVM against standalone `scummvm`), fps line from pocket-core's exit report.
- [ ] DOS: a zip and a folder from the shelf; HOME, Save 1, play on, Load 1, Quit, start again (resumed). L3 opens the
  on-screen keyboard; tap a key. A DOS game that uses the mouse: drag moves the pointer, tap clicks, two-finger tap
  right-clicks. Right stick moves the mouse.
- [ ] ScummVM: a game from ROCKNIX's scan list and one as a bare folder in `roms/scummvm`. Tap on a hotspot: the cursor
  jumps there and clicks; A clicks, B right-clicks, Y skips the intro, L1 skips a line, Start opens ScummVM's menu,
  Select its keyboard. Picture: OpenGL on the Adreno, no flicker. Save in the game, quit, start the same game in
  standalone ScummVM (ROCKNIX's ScummVM entry): the save is listed (`/storage/.local/share/scummvm/saves`; check first
  that standalone really keeps its saves there). HOME > Save: the "No save states" message.
- [ ] PICO-8 without the official binary: a cart from the shelf runs on pocket-core (fake08), A = X, B = O; HOME > Save:
  the message. With `pico8_64` in `roms/pico-8/aarch64/`: the same cart starts the official PICO-8 (start_pico8.sh).
- [ ] TIC-80: a cart from the shelf, a game that uses the mouse: a tap clicks where the finger is; HOME > Save: the message.

## More systems (docs/HOST.md "More systems (2026-10-23)", checked on the virtual device only)
Test content in clearly named files (`zz-test-…`), removed afterwards; BIOS files only from your own hardware.
- [ ] NES resume (the Nestopia fix): a game in `roms/nes`, play, Quit, start again; `resumed` in
  `/var/log/pocket-ui.log` each time (five starts). The same once in `roms/famicom` and `roms/sfc` from the shelf.
- [ ] Famicom Disk System with `roms/bios/disksys.rom`: without the file the card names it; with it a disk game starts on
  pocket-core, L1 turns the disk when the game asks for side B, an in-game save survives Quit, the game resumes.
- [ ] SuperGrafx: a HuCard (`.pce` or `.sgx`) starts on pocket-core, Save / Load / resume; a CD game in the same folder
  starts with RetroArch. `scripts/syscheck.sh bench supergrafx beetle_supergrafx <game>` and `ra`.
- [ ] Virtual Boy: red on black, 3x; left D-pad and right stick (right D-pad) in a game that uses both; L2/R2/L3/R3;
  run-ahead 1 frame stays on (no notice) for a minute; `bench` / `ra` against RetroArch.
- [ ] NAOMI / Atomiswave, only with MAME's BIOS sets in `roms/bios/dc` and a romset of a game: pocket-core directly
  (`scripts/syscheck.sh play naomi flycast /storage/roms/naomi/<game>.zip "wait 20" "shot a" "fifo save" ... quit`, the
  shelf doesn't route unverified systems); coin (Select), start, buttons, a vertical game, Save / Load / resume, frame
  rate against standalone Flycast (`bench` / `ra`). Only then `verified: true` for them in `src/hostsys.rs`.

## Emulator choice: Runs on (docs/HOST.md "Choosing the emulator", checked on the virtual device only)
First copy system.cfg aside (`cp /storage/.config/system/configs/system.cfg /storage/system.cfg.before-runs-on`) and
look for choices made in EmulationStation: `grep -E '\.(emulator|core)=' /storage/.config/system/configs/system.cfg`.
Every line found now decides the emulator of that system or game (the log at start lists them:
`grep 'emulator choices' /var/log/pocket-ui.log`). Put the copy back at the end if anything looks wrong.
- [ ] Before touching anything: every system starts as before (no ES lines), the console page's Emulator row unchanged.
- [ ] GBA: X on the system, START: "pocket-core · mGBA". Right to "RetroArch · mGBA", Save changes → For this console.
  `grep ^gba system.cfg`: `gba.emulator=retroarch`, `gba.core=mgba`, no second line, no `/tmp/.system.cfg.lock` left.
  Start a GBA game: RetroArch (log `launch gba … (emulator retroarch, core mgba)`); HOME → pause menu has Options;
  Options opens Runs on with the RetroArch rows; Quit saves, the next start continues.
- [ ] Aspect ratio 4:3, Integer scaling On, Save for this console, start the game: RetroArch shows 4:3, whole-number
  scale (`grep -E 'aspect_ratio_index|video_scale_integer' /tmp/.retroarch.cfg`: "0", "1"). HOME + R2 fast-forward.
- [ ] Shader: the list offers presets found on the card (log `emulator choice: N shader presets`); pick crt-geom (or
  the first), save, start: the shader is on. Back to None.
- [ ] One game back on pocket-core: its card, START, pocket-core, Save → For this game. That game: pocket-core with
  instant resume; another GBA game: RetroArch. `grep 'gba\["' system.cfg`: the two lines with the game's file name.
- [ ] Open EmulationStation once (only if wanted; see ES-REPLACEMENT-GAPS 5.3 for its dangers) and check GBA's
  "Emulator" setting reads retroarch/mgba, the game's AUTO (ES doesn't know pocket-core). Not required.
- [ ] Restore defaults on the console page: GBA back on pocket-core, `gba.ratio=3/2` as shipped.
- [ ] PSP → Standalone · PPSSPP for one game (if `start_ppsspp.sh` is there): it starts in standalone PPSSPP, the pause
  menu is Continue, Options, Quick settings, Quit; Quit closes it. Then back to pocket-core.
- [ ] NDS → Standalone · melonDS for the console: games start in standalone melonDS, saves are the same `.sav`.
  Back to pocket-core.
- [ ] A game that pocket-core couldn't start (fallback.json): the page says so; choose pocket-core, save: it starts
  on pocket-core again (and goes back to the fallback if it fails again, no loop).
- [ ] Speed: `grep 'emulator choice: page' /var/log/pocket-ui.log` under 50 ms; `launch: game process started` after
  A unchanged against before (≈1 ms); first frame after boot unchanged.
