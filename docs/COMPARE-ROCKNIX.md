# pocket-ui against stock ROCKNIX

How fast pocket-ui is compared with what a Mini V2 runs without it: ROCKNIX's EmulationStation with RetroArch. The
speed guard ([LATENCY.md](LATENCY.md)) measures pocket-ui from its own trace, which EmulationStation doesn't have, so
this comparison measures both from outside, the same way: a button press goes into the Pocket's pad, and the screen is
recorded until it shows the answer.

```sh
scripts/test-rocknix-compare.sh --sim                   # no device: recorder, analysis, restore, a simulated run, the video
scripts/devlock.sh me scripts/rocknix-compare.sh run    # both UIs, every scenario (about 75 minutes, see below)
scripts/devlock.sh me scripts/rocknix-compare.sh run --ui es -- --scenarios nav,launch --reps 5
```

The results land in `compare-results/<time>/` (not tracked): `table.md`, one JSON per UI, and `out/` with the raw
recordings. The numbers of the first device run go into [LATENCY.md](LATENCY.md#pocket-ui-against-rocknix-emulationstation).

## How it measures

**The press.** `rocknix-compare.py` writes key events into the Pocket's own pad device, the "Retroid Pocket Gamepad"
that InputPlumber reads (its node is in `/dev/inputplumber/sources` once InputPlumber holds it). From there a press
takes the path a finger's takes: InputPlumber turns it into its virtual DualSense, which EmulationStation reads through
SDL, RetroArch through udev and pocket-ui and pocket-core through evdev. Only the button's own hardware (the MCU and
its serial line) is skipped, the same for both. The time of the press is `CLOCK_MONOTONIC` just before the write. A
reader on the DualSense notes when InputPlumber passed the press on (`input.inputplumber` in the results).

If InputPlumber grabbed its source device (`EVIOCGRAB`), the kernel would drop what anyone else writes into it. `probe`
finds that out before each run (nothing comes out of the DualSense); the run then writes into InputPlumber's DualSense
itself (`--inject target`: its buttons, the D-pad as a hat), which skips InputPlumber for both UIs alike and is noted
in the run's events.

A press is held 100 ms (`--hold`), like a quick tap. Every time is measured from the press going down, also where a
UI acts on the release: pocket-ui opens its pause menu when HOME comes up (HOME is also the key of the shortcuts), so
its pause-menu numbers include the 100 ms of the tap; RetroArch acts on X going down while HOME is held. Where a step
takes two presses by design, the time starts at the confirming one: RetroArch's quit (HOME+START, and again, because
ROCKNIX sets `quit_press_twice`), pocket-ui's quit (HOME, then Quit in the pause menu, then A).

**The screen.** The recorder is a Wayland client of sway written in plain Python (`zwlr_screencopy_manager_v1`, the
protocol grim uses). It asks for every frame sway commits (`copy_with_damage`: a still screen sends nothing) and keeps
a small sample of each, compressed, with the time stamp sway gives the frame (`CLOCK_MONOTONIC`, the same clock as the
presses). So there is no video, no frame rate to assume and no clock to line up afterwards.

It copies a **band across the middle of the screen**, the full width and 180 of the 1080 rows (`--region
0,450,1240,180`, every 4th pixel of every 4th row of it, `--step`). On the Mini V2 (2026-10-26) sway copied a whole
screen of EmulationStation or RetroArch in about 8 ms, but a whole screen of pocket-ui or pocket-core in about 50 ms:
the recorder then saw only every third frame of pocket-core, and sway spent those 50 ms on the copy instead of on the
next frame, on pocket-ui's side only. The cost goes with the area: a quarter of the screen took 15 ms, the band 10 ms,
at 57-60 frames a second for both. The band holds what changes in every step: the selected entry of both lists and
of both start screens, the menus, the game picture.

**The answer to a press.** For each press, from the frames:

| Number | The first frame after the press that ... |
|---|---|
| `react` | differs from the screen at the press in more than 0.3 % of the pixels, not counting pixels that were changing anyway in the second before (a running game, an animated background, a clock) |
| `done` | shows the step's end state, and every frame after it too, to 90 %: the end state is the screen at the end of the step's window (`--window`, 2 s; 6 s for a game start), compared in the pixels that stay still at that end and that changed from the screen at the press, the not-black ones first; a frame is "there" when at most 10 % of that change is still to come. A running game keeps moving, so only its still parts count: the frame around it, the borders, a score. The 10 % lets a small notice over a game that has started pass (RetroArch shows "state loaded" for about three seconds; with a stricter rule its game start read 5.3 s instead of the 1.7 s at which the game is on the screen), and ends a slide or a fade at its 90 % point. A step that ends in a running game (a game start, the last press of a switch) is done when the game's picture is there: at most half of the change still to come, from then on; a game that scrolls or animates by itself never settles (uCity's city kept moving for 5 s after it was on the screen) |

So `react` is "the UI noticed", `done` is "what I wanted is there": the end of a list move's slide, the game's
picture after a start (not the black screen before it), the list after a quit.

**Cold start** (`start`): essway stopped, then started (`systemctl start`, the same unit for both; the pocket-ui
launcher execs EmulationStation's `start_es.sh` at once when the ES-only mark is there, a few milliseconds of shell).
`start.screen` is the first frame showing the start screen (the settled screen of a first, uncounted start);
`start.input` is the first frame that answers a press: from the moment the start screen is there, `move` is pressed
every quarter second until the screen changes. These presses (and the wake's) are compared with the screen at the press
without leaving out what moved before it: the start screen has only just come.

**Waking** (`wake`): on the start screen, the RTC alarm is set (`--asleep`, 15 s), then the power key is pressed (written
into its own input device, the one logind and pocket-ui read). EmulationStation's suspend is logind's; pocket-ui's is
its own (for this run its `dev.env` gets `POCKETUI_SUSPEND_CMD` with the alarm, as for `latency.sh --wake`, because
pocket-ui's own suspend sets the alarm for switching off after sleeping). If nothing suspends within 6 s (a reader
that grabs the key), the alarm is set again and the UI's other way is taken (pocket-ui's FIFO `power`, or `systemctl
suspend`); the results say which (`how`). From the moment the system is back (the kernel's suspend counter went up):
`wake.screen_on` (the backlight is not 0), `wake.first_frame`, `wake.input` (the first frame that answers one of the
presses made every 100 ms from then on; pocket-ui takes no button in the first 600 ms after a wake, by design).
No suspend ever happens without the alarm set first.

**CPU** (`cpu`): without the recorder. 20 s windows (`--cpu-secs`) on the start screen with no input, and behind the
first game (`--systems`): the UI's process and the emulator's (all threads, utime + stime of `/proc/<pid>/stat`, % of one
core, as the speed guard) and the whole system (`/proc/stat`, also in % of one core, so 800 % is all eight).

**Playing** (`play`): each game for 5 minutes (`--play-secs`) from its power-on: the resume states of the test games are
set aside (the device script has copies and puts them back), so both frontends show the same title and attract
sequence with the same core, without input. The backlight is set to the same level for both (the run's start level,
or the plan's `brightness`). Every 5 s: battery `current_now` × `voltage_now` (or `power_now`), status, temperature,
governor, fan. Watts count only on battery (status Discharging); on the charger the column stays empty. The fan curve
and the performance profile are each UI's own (pocket-ui drives the fan itself), as a player has them: noted per
sample, not equalised.

## Fair

- **Same games, same cores.** uCity (GBC), F-Zero: Maximum Velocity (GBA), Super Mario All-Stars (SNES): `gambatte`,
  `mgba` and `snes9x` are EmulationStation's defaults for these systems (es_systems.cfg of ROCKNIX 20261001) and the
  cores pocket-ui routes them to on pocket-core (`hostsys::HANDHELD_CORES`). `device.json` keeps the device's
  system.cfg lines for these systems (a core chosen per system or game would show there).
- **Same settings as a player has them.** Stock ROCKNIX has `global.autosave=1` (RetroArch saves a state at quit and
  loads it at the next start) and `global.bezel=thebezelproject`; pocket-core resumes its own state. So a game start is
  a resume on both. With `autosave` EmulationStation asks first: A on a game opens its Savestate manager with "Start
  new game" selected and the auto save to its right, so resuming is A, Right, A; the time of a game start runs from
  the last A, and the presses are counted in [the numbers](LATENCY.md#pocket-ui-against-rocknix-emulationstation). pocket-ui: A. The first start of each game in a run is a warm-up and not counted (it writes the states and
  brings the files into the page cache).
- **Same way in.** Both get the same presses into the same device and the same recorder; the same service start.
- **The theme** is what the device has (`es_theme` in `device.json`; ROCKNIX's default is es-theme-art-book-next).

## Limits

- **One refresh.** sway commits at most once per refresh (60 Hz), so every time lies on a 16.7 ms grid, and the panel
  shows a committed frame at its next refresh, up to 16.7 ms later, the same for both. Differences under ~17 ms are not
  differences. The medians of several presses are reported.
- **Compositor, not panel.** The time is the commit's, not the light's.
- **The recorder costs something.** sway copies each frame for it (a GPU read-back of the screen) and Python samples it,
  while presses are measured. Both UIs carry the same cost. CPU and power are measured without it.
- **The band.** Only the middle sixth of the screen is compared (see above), every 4th pixel of it. A change that
  happens only outside it is not seen; every step measured here changes the band.
- **Different screens.** EmulationStation's start screen is its system view, pocket-ui's is Home; their "list" is a
  game list for both (`list_system`, GBA by default).
- **Switching** is a sum: each press of the UI's way (EmulationStation: quit, back to the systems, two to the
  right to the other system, into its list, A, Right and A in its Savestate manager; pocket-ui: HOME, SELECT, the
  other game, A), each to its end state. A player's own time between the presses is not in it; the number of presses
  is. EmulationStation's Right in the Savestate manager only moves a highlight below the band: it reads "nothing
  changed" and counts as 0 (`unseen` in the results).
- **CPU per process** is utime + stime of `/proc/<pid>/stat` (10 ms ticks, all threads): the Mini V2's kernel has no
  schedstat (`CONFIG_SCHEDSTATS` off), so the files the speed guard reads (`/proc/<pid>/task/*/schedstat`) are not
  there.
- **Different resume files.** RetroArch's `.state.auto` and pocket-core's `.pstate.auto` are not the same file, so
  the scene after a resume can differ; the `done` numbers end at the picture, whatever it shows.

### Why not wf-recorder or grim

wf-recorder encodes a video; its frame times count from its first frame, which has no stamp on the device's clock, and
encoding costs far more than sampling. grim saves a PNG per call (around 100 ms each on the device): far below 60
pictures a second. The recorder here asks sway for the same frames grim gets, keeps the commit's own time stamp and
only a sample of the pixels. (`rocknix-compare.py import-video` turns a video into a recording anyway, given a start
time; the self-test uses it on a synthetic video.)

## The plan

Both UIs remember where the cursor was, and the libraries differ from card to card, so the way to each game is
written down per device in `scripts/rocknix-compare-plan.json` (`--plan` for another file):

| Key | What |
|---|---|
| `games` | per system a part of the test game's file name (also how the run checks the right game started: the emulator's command line) |
| `move`, `move_back` | a press on the start screen that changes it, and the one that undoes it |
| `to_system`, `from_system` | start screen (cursor on its first entry) to the system's game list, and back |
| `to_game_first`, `remembers_cursor` | in a game list just entered after a start of the UI: to the test game. EmulationStation keeps the cursor in a list until it restarts (later visits find it on the game); pocket-ui opens a list at its first game every time (`"remembers_cursor": false`: the way to the game every time) |
| `launch` | from the list to the game: `A`, or EmulationStation's `A w1200 RIGHT w600 A` (its Savestate manager), measured from the last press |
| `region` | the part of the screen the recorder copies, instead of `--region` |
| `list_system`, `list_move`, `list_back` | the list of the `nav` moves |
| `pause`, `pause_close`, `quit` | in a game: the pause menu (RetroArch's menu), closing it, back to the list |
| `switch` | per `<first>><second>` of `--systems`, the presses from the first game to the second, each measured |
| `ignore` | rectangles (fractions of the recorded picture, as `thumbs` shows it) left out of every comparison: a clock |
| `brightness` | the backlight level while playing (default: the level at the start of the run) |
| `calibrated` | `run` refuses a plan that is not |

Sequences: `RIGHT RIGHT A`, `HOME+X` (hold HOME, press X), `w500` (wait), `A:300` (hold 300 ms). Buttons by their place
on the Pocket: A right, B bottom, X top, Y left. To calibrate (with the device lock):

```sh
scripts/rocknix-compare.sh probe            # which source key gives which button (no UI meanwhile); out/codes.json
scripts/rocknix-compare.sh mode es          # EmulationStation on the screen
scripts/rocknix-compare.sh shot start       # screenshots/compare-start.jpg
scripts/rocknix-compare.sh try "RIGHT A"    # press, then screenshots/compare-try.jpg
scripts/rocknix-compare.sh mode pocket-ui   # the same for pocket-ui
scripts/rocknix-compare.sh restore          # pocket-ui as the UI again, verify-device.sh
```

Then set `"calibrated": true`. A wrong sequence shows up as a failed scenario (`no retroarch with 'Maximum' in its
command line ...`); the run goes on with the next scenario after restarting the UI.

## A run

`rocknix-compare.sh run` sends the tool over and starts `scripts/rocknix-compare-device.sh` on the device as the unit
`pocket-compare` (a deep sleep may drop the SSH connection; the run goes on, and the computer asks every few seconds
until it is done). One SSH session starts it and follows its log. On the device:

1. `device.json`: ROCKNIX version, pocket-ui's md5, the ES theme, the system.cfg lines that matter, governors,
   battery, brightness, free memory, sway, kernel.
2. Copies into `/storage/.cmpbak/<time>/` (the newest three are kept): pocket-ui's data folder without its big caches
   (covers, snaps, titles, wikipedia, store, identify, logs), RetroArch's config (retroarch.cfg, the core options,
   `config/`, `playlists/`), EmulationStation's settings files, system.cfg, every save and state of the test games'
   systems and their gamelist.xml (a wrong press can start another game of the list; what the run adds there is
   deleted at the end, what it deletes or changes comes back), pocket-ui's dev.env; the speaker's mute and volume, the
   brightness (a screen dark in pocket-ui's light sleep reads 0: then its saved level comes back, not the 0).
3. The speaker muted (again before every measured press: a UI may set the volume when it starts).
4. `probe` with essway stopped: which source key gives which DualSense button (on the Mini V2: X is BTN_NORTH, Y
   BTN_WEST, A BTN_EAST, B BTN_SOUTH, the D-pad BTN_DPAD_*; InputPlumber doesn't grab its source, the presses go
   through it).
5. `drive --ui es`, then `drive --ui pocket-ui`, with a dev.env for pocket-ui: the suspend command for the wake test,
   and `POCKETUI_SLEEP_MINUTE_SECS=3600`, so that its light sleep (after 3 minutes without a press, it stops the game)
   doesn't come during the 5-minute play windows; RetroArch has no such sleep.
6. Whatever happened (an error, `systemctl stop pocket-compare`, a signal): the trap stops essway and every emulator
   (`pidof`, never `pkill -f`), puts the copies back (what the run added at the top of the data folder and new states
   of the test games go; the big caches are not touched), clears the RTC alarm, unmutes, removes the ES-only mark,
   restarts essway (pocket-ui), packs `out/`.

Back on the computer: the analysis, the table, and `scripts/verify-device.sh --no-local`, which must say `VERIFY: ok`
(exit 3 if not). Nothing touches the boot path, the bootloader, the partitions; system.cfg changes only as
EmulationStation itself writes it, and is put back.

**Time** with the defaults (5 cold starts, 30 list moves, 3 rounds of start, pause and quit per game, 3 switches, 3
wakes, CPU, 3 × 5 minutes of play) on the Mini V2 (2026-10-26): about 95 minutes for both UIs, of which CPU and
play are 42 (the long ways through the lists, 34 and 80 presses to a game, take their share). Calibrating the plan the
first time, with screenshots: about 40 minutes. The showcase: about 8 minutes for both.

## The video

A side-by-side film of the comparison (about a minute, 1920x1080): EmulationStation + RetroArch left, pocket-ui
right, the same steps. It is filmed in a run of its own, after the measurement run, because filming at full size
(wf-recorder, x264) costs the device far more than the sampling recorder:

```sh
scripts/devlock.sh me scripts/rocknix-compare.sh run -- --scenarios showcase          # both UIs, ~3 minutes
scripts/rocknix-compare.sh video compare-results/<showcase run> compare-results/<measurement run>
#   -> demo/out/pocket-ui-vs-rocknix-speed.mp4 (--out), --audio es|pocket-ui, --no-slowmo
```

**The showcase scenario** (`drive --scenarios showcase`, the same safety as every run: copies, mute, the trap,
`VERIFY: ok`): with the UI stopped, the sampling recorder and wf-recorder start (`/storage/rec-tools/rec/wf-recorder`,
60 fps, x264 crf 18, with `--showcase-audio 1` also the speaker's monitor as AAC: the speaker itself stays muted),
then essway starts; to the first game's list (`--showcase-systems`, GBA then GBC by default), four list moves, the
game start (A; EmulationStation: A, Right, A), the pause menu (HOME+X / HOME), back, quit, the second game's list, its
start, quit. The steps' presses are logged like the measured ones (`showcase.*`), but they are no measurement: the
analysis leaves them out. Before the filming, not filmed, each game is started and quit once, so that both resume a
state of their own as in the measurements (without it RetroArch had no auto save and booted F-Zero from its logo).
The way to the games goes slower than in the measurements (0.8 s between presses): with wf-recorder running,
EmulationStation dropped presses that came during a carousel slide once and ended up in the N64 list, where its
Savestate manager started Ocarina of Time.

**The cut** (`scripts/rocknix-compare-video.py`, on the computer: ffmpeg, numpy, the Barlow fonts and the caption
look of the earlier demo videos): each clip turned upright (`transpose=2`) and 900 pixels wide, each step cut from
just before its press in both clips, so that both halves show the same moment of input; a "pressed" badge on both
for half a second; the game start once more four times slower; a card with the measured medians; fades between the
parts. Lining up: wf-recorder counts its frame times from its own first frame. The sampling recorder ran alongside on
the presses' clock, so the brightness of the two, frame by frame, is compared at every shift within a few seconds of
wf-recorder's start; the best fit places the video on the presses' clock to about a frame (`fit` in its output, a
poor fit is reported).

**The numbers** under each half come from the measurement run's `<ui>.json` only (median, and "median of N measured
runs" with N from there); a number that is not there reads "not measured". The showcase's own timing is never shown
as a number.

## Files

| File | What |
|---|---|
| `scripts/rocknix-compare.sh` | on the computer: `run`, `mode`, `try`, `shot`, `probe`, `restore`, `analyze`, `selftest` |
| `scripts/rocknix-compare-device.sh` | on the device: copies, the run, the restore (trap) |
| `scripts/rocknix-compare.py` | everything else, Python standard library only: `record`, `drive`, `probe`, `press`, `analyze`, `table`, `thumbs` (JPEG strips at the press, react and done, to check what was measured; needs ffmpeg), `import-video`, `selftest` |
| `scripts/rocknix-compare-plan.json` | the plan |
| `scripts/rocknix-compare-video.py` | the side-by-side video from a showcase run and a measurement run ([The video](#the-video)) |
| `scripts/test-rocknix-compare.sh` | the tests: the self-test, a synthetic video, the device script in a made-up root (everything put back after a whole run and after SIGTERM); `--sim` also runs the simulation and cuts the video from it (lined up within a frame) |

**Output.** `out/<ui>/events.jsonl`: one line per press (`t` = the press in CLOCK_MONOTONIC ns, `t_up`, the button,
the window), per mark (service start, the UI's process seen, resumed, ...) and per CPU or play window. `out/<ui>/frames.pfr`:
the recording (`PFR1`, a JSON head, then per frame its time stamps, damage rectangles and the compressed sample;
`MARK` records for pauses). `<ui>.json` from `analyze`: `metrics` (median, n, missed, min, max per number), `samples`
(every single value with how it was found), the thresholds, `failures`. `rocknix-compare.py thumbs DIR nav launch`
writes strips to look at.

**Self-test.** `rocknix-compare.py selftest`: the Wayland client against a fake compositor (frames and their commit
times, pause and resume), the analysis on made-up recordings (a list move, a clock that must not count, a game with
the pause menu coming in, a start with and without a frame around the game, a fade). `selftest --sim`: both UIs
through every scenario but the wake (and the showcase, with a stand-in for wf-recorder), with the real driver, recorder and analysis, on a simulated Pocket that answers
30 ms after a press. `scripts/test-rocknix-compare.sh` runs these, a synthetic video (ffmpeg) and the device script's
restore in a made-up root.
