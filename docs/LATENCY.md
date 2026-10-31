# Speed guard

pocket-ui is meant to feel instant. The speed guard is a repeatable benchmark of the moments a player notices:
starting, moving on the shelf, opening menus, starting, quitting and switching games, and how much CPU pocket-ui uses
when it should be asleep. Run it before and after a change, and in CI later, so the UI stays fast as features are
added.

```sh
scripts/vdev.sh deploy                     # the build to measure, in the virtual device
scripts/latency.sh                         # measure: table on the screen, latency.json
scripts/latency.sh --compare               # and compare with docs/latency/vdev.json; exit 1 if slower
POCKETUI_HOST=root@<device-ip> scripts/latency.sh --compare    # the same on a Mini V2 (docs/latency/device.json)
scripts/latency.sh --phases                # and where the time of each Quit, pause menu and switcher went
```

A full run takes about six minutes in the virtual device: one warm-up pass that is not counted, then three passes.

## How it measures

pocket-ui and pocket-core write a trace when `POCKETUI_TRACE=<file>` is set (`src/trace.rs`). Without the variable
nothing is written and nothing is formatted: each trace point is one atomic load and a branch. One line per event,
with a CLOCK_MONOTONIC time stamp in microseconds:

The pocket-ui that `latency.py` starts also gets `POCKETUI_NO_HISTORY=1`: the games a measurement starts and quits are
no play, so nothing of that run goes into the player's Play history ([HISTORY.md](HISTORY.md#recording); the log
says `play log: POCKETUI_NO_HISTORY is set, nothing of this run is recorded`).

It also gets `POCKETUI_RTC_ALARM=/tmp/pocket-latency-rtc-alarm`: switching off after sleeping writes the RTC's wake
alarm before every suspend and would replace the alarm `--wake` sets, so the Pocket stayed asleep until someone pressed
the power key (2026-10-26). The measured pocket-ui never switches the Pocket off.

```text
119722488791 pocket-ui 2493 input 61 Down down
119722489348 pocket-ui 2493 frame shelf in=62 sel=system:gba
119546287022 pocket-core 421 core-start
119546484128 pocket-core 421 core-frame first
```

| Event | Written by | When |
|---|---|---|
| `ui-start` | pocket-ui | first line of `main` |
| `input N <button> down\|up` | pocket-ui | a pad button reaches the event loop; N counts them |
| `frame <screen> in=N sel=<entry>` | pocket-ui | a frame was handed to the compositor (`src/app_trace.rs`): which screen, the last input it already shows, the selected entry |
| `paint <ms> <w>x<h>` | pocket-ui | right after each `frame`: how long painting and handing it over took, and the window's size then |
| `step <what>` | both | one step inside a Quit, the pause menu opening or a game's teardown (`trace::step`; the `switch-time` steps of `fast_switch::mark` come here too), for `latency.py phases` |
| `launch <system> <rom>` | pocket-ui | just before pocket-core is started |
| `game-exited <system> <rom>` | pocket-ui | the game has ended and pocket-ui knows (after a hand-over: its exit save is on the card, the process may still be tearing down) |
| `core-start` | pocket-core | its `main`, after the arguments are read |
| `core-frame first` | pocket-core | the game's first frame is swapped to the screen |
| `core-frame wake` | pocket-core | its first frame after a wake from sleep ([Waking](#waking)) |
| `power-key <µs>` | pocket-ui, pocket-core | the power key went down, at the kernel's time stamp of the event (CLOCK_MONOTONIC µs, as the line's own) |

`scripts/latency.py measure` runs on the device itself (ROCKNIX's Python, the standard library only). It starts
pocket-ui with the trace on, presses buttons through pocket-ui's command FIFO `/tmp/pocket-ui.in` (the one
`vdev.sh send` and `dev.sh send` use) and reads the time just before each write from the same clock. No
screenshots are compared: the screen names and the input numbers in the trace say what is on screen. When it is
done, `scripts/latency.sh` starts pocket-ui again the normal way (the launcher, without the trace).

### Nothing of the run stays behind

That pocket-ui runs outside the `essway` unit, with `POCKETUI_TRACE` and `POCKETUI_NO_HISTORY` set. Left running it
blocks the next deploy ("text busy") and keeps real play from being recorded; on 2026-10-24 one did, for about six
hours. So, on a device:

- `latency.py` ends the pocket-ui it started in `Ui.stop` (asks it to quit, kills it if it does not, waits for it) in
  every `finally`, and again at the end of `measure`; SIGTERM and SIGHUP (the SSH connection going away) end the run
  through those same `finally` blocks. `selftest` tests `Ui.stop` and `Ui.stop_all` with a real process.
- `latency.sh` stops `essway` (or the `pocketui` unit) for the whole run. After `latency.py`, whatever its outcome,
  the device side kills every `pocket-ui` and `pocket-core` still alive (they can only be the harness's), then
  restarts `essway` the way `dev.sh deploy` does (`DEVICE_RESTORE` in `latency.sh`; also in the detached `--wake` run).
- Last, `latency.sh` runs `scripts/verify-device.sh --no-local` ([DEVELOPING.md](DEVELOPING.md#after-a-deploy-or-a-measurement-scriptsverify-devicesh)):
  the unit active, exactly one pocket-ui and it in `essway.service`, no trace variables, the deployed binary running.
  If that fails, `latency.sh` exits 3 and says so (the JSON is fetched first, but no table is printed): fix the
  device before the next run or deploy. The virtual device is not checked (a container, no `essway`).

"Frame" means the frame was handed to the compositor (`wl_surface.commit` through softbuffer, `eglSwapBuffers` in
pocket-core). The panel shows it at its next refresh, up to 16.7 ms later; that part is the same in every run. A frame
smaller than the largest the window has had (its `paint` line says so) doesn't count: it was drawn before sway's
resize to the full screen came in, and is not what the screen shows. (Before 2026-10-23 such a frame counted, and
the pause menu after a Continue looked 200 ms faster than it was; pocket-ui no longer draws one, see
[ARCHITECTURE.md](ARCHITECTURE.md#the-pause-menu).)

The FIFO puts a button into pocket-ui's event loop the same way the pad thread does, but it skips the kernel and
evdev; a real button press adds the time from the switch to the read, which no software change affects.

## The numbers

All times in milliseconds, lower is better. Each pass measures every number several times and keeps the median
(`nav` keeps the 50th and 95th percentile of its moves); the reported value is the median over the passes.

| Metric | What it is |
|---|---|
| `start.cold` | pocket-ui started (process spawned) until its first shelf frame. Median of 5 starts per pass. The launcher's own work before it starts pocket-ui is not included |
| `start.process` | the same from pocket-ui's `main`: `start.cold` minus loading the program |
| `start.pads` | from pocket-ui's `main` until its pads are open (its `pad-open` trace line): a press from then on counts. It should stay below `start.process`, so that a press as soon as the shelf is there is acted on (see [First press after a start](#first-press-after-a-start)) |
| `nav.p50`, `nav.p95` | a Down/Up on Home until the frame showing the new selection. 100 moves per pass, up and down the list (`--nav-system gbc`: that system’s games; Cabinet books Right/Left), 150 ms apart (so the 120 ms slide of the previous move has ended) |
| `open.quick`, `close.quick` | HOME on the shelf until Quick Settings; B until the shelf. the FIFO sends down/up together, so no physical hold duration is included |
| `open.card`, `open.card_first`, `close.card` | X on the first `--systems` game until its card/box; the first open per process separately (cold image cache); B until the list |
| `close.pause_menu` | Continue until pocket-ui commits its frame behind the game. This measures the UI hand-off, not a new core frame or the compositor revealing it |
| `open.settings` | START on the shelf until the first Settings frame (5 per pass) |
| `open.switcher` | SELECT on the shelf until the first frame of the game switcher (5 per pass) |
| `open.pause_menu` | HOME in a game until the first frame of the pause menu. Includes pausing pocket-core (pocket-ui waits for its `paused`) and raising pocket-ui's window; for pocket-core the picture of the game comes after this frame (the `step menu: picture in` of `phases`) |
| `open.pause_menu_again` | The same after Continue and a second of play, in the first game of each cycle: the pause menu as it opens in play, after pocket-ui's window was moved behind the game once (the first menu after a start never was) |
| `open.switcher_game` | SELECT in the pause menu until the first frame of the switcher |
| `launch.<system>` | A on the game in its system's list until pocket-core's first frame. Includes the instant resume of the state the previous quit wrote |
| `quit.to_shelf` | A on Quit in the pause menu until the first shelf frame after the game ended. Includes pocket-core writing its resume state; for the cores that hand over (`fast_switch::may_hand_over`) not its teardown and exit, for the others those too |
| `switch.game` | A on the previous game in the switcher (opened from the pause menu) until that game's first frame: the running game saves and quits, the other one starts |
| `cpu.shelf_idle` | pocket-ui's CPU time on the shelf with no input, 10 s, in percent of one core (all its threads; how, below) |
| `cpu.behind_game` | the same while a game runs and the menu is closed (`scripts/idle-cpu.sh` does this with a busy player too) |

**CPU** is pocket-ui's CPU time over the window divided by the window, from the first source that can be read
(`cpu_seconds` in `latency.py`):

1. `/proc/<pid>/stat`, utime + stime: every Linux kernel has it; all threads, also those that ended; in clock ticks
   (10 ms on the Mini V2, so 10 s resolve to 0.1 %).
2. `/proc/<pid>/task/*/schedstat`: nanoseconds, only on a kernel with `CONFIG_SCHEDSTATS` (the virtual device's host
   kernel has it, the Mini V2's doesn't).

Which one it was is in the result's `meta.cpu_source`. When neither can be read the number is `n/a` (`"value": null`
with `"unmeasured"` and the reason in the JSON), never 0 %, and `compare` fails with `NOT MEASURED`, against any
baseline. Until 2026-10-26 `latency.py` read only schedstat and counted a missing file as nothing: on the Mini V2 every
CPU number came out as 0.00 %.

One pass: 5 cold starts; 100 shelf moves; Settings and the switcher opened 5 times each; 10 s idle on the shelf; then
two game cycles over the systems in `--systems` (default `gbc,gba,snes`): start each game, open the pause menu and quit,
except for the last one, from whose pause menu the switcher goes back to the game before it, which is then quit. In
the first game of each cycle the pause menu is opened twice: once, then Continue, a second of play, and again
(`open.pause_menu_again`). The CPU behind a game is measured in the first game of the first cycle.

The games are the first one of each system's list, or `--systems gba:megaman,...` picks one by part of its file name.
On a real device that means: the harness plays your games for a few seconds each, which writes their resume state
and moves them to the top of the recent games, as playing them would.

## Cold start

`start.cold` above is not cold in the page-cache sense: the harness starts pocket-ui again and again, so after the
first start its program, its libraries and the folders it reads are in RAM. At every power-on nothing is; each read
waits for the SD card, and reads at the same time queue up. On the Mini V2 (SD card, 332 games in 23 systems,
2026-10-24, before perf/cold-start) that was 319-587 ms to the first frame against about 50 ms warm, most of it the
game list (`library 205.3`: es_systems.cfg and a listing of every ROM folder) and the event loop (`event loop`
45-220 ms). What the start does since then: [ARCHITECTURE.md](ARCHITECTURE.md#start).

```sh
POCKETUI_HOST=root@<device-ip> scripts/latency.sh --cold --out cold.json            # 3 runs x 5 starts
POCKETUI_HOST=root@<device-ip> scripts/latency.sh --cold --out cold.json -- --starts 10 --runs 1
python3 scripts/latency.py compare cold-after.json cold-before.json
```

`--cold` measures only starts. Before each, `latency.py` writes the dirty pages out and empties the page cache
(`sync; echo 3 > /proc/sys/vm/drop_caches`, root), starts pocket-ui with sway running, waits for the first shelf
frame, lets it run 1.5 s (the scan after the first frame and the jobs put off until then) and stops it. Numbers:

| Metric | What it is |
|---|---|
| `start.nocache` | pocket-ui started (process spawned) until its first shelf frame, page cache empty |
| `start.nocache_process` | the same from pocket-ui's `main` |
| `step.<name>` | each step of pocket-ui's own `first frame after … (logs 0.6, …, drawn 24.1 ms)` line (`src/startup.rs`): `step.library`, `step.event_loop`, `step.configs`, `step.sleep`, … (spaces and `+` as `_`), the median over the starts |

This leaves out sway's own start, which a power-on has too (pocket-ui then also waits for sway to answer). The whole
thing as at power-on, by hand, one start at a time (the launcher starts pocket-ui again with essway):

```sh
ssh root@<device-ip> 'sync; echo 3 > /proc/sys/vm/drop_caches; systemctl restart essway; sleep 8;
  grep -E "^library:|^first frame after|needed before the first frame|^startup:" /var/log/pocket-ui.log | tail -6'
```

For an A/B on one build, `/storage/pocket-ui/dev.env` (both ways of measuring read it):

- `POCKETUI_LIBRARY_CACHE=0`: no game list cache, the folders before the first frame as before.
- `POCKETUI_STARTUP_PROBE=1`: libwayland-client and libxkbcommon are loaded before the event loop as a step of their
  own (`step.display_libs`, and `startup: libwayland-client.so.0 loaded in …` in the log), so `step.event_loop` is
  then the connection to sway and winit's setup alone.

What to look for in the log: `library: … read in … (from the cache)` at the start, `library: read again (after the
start) in …, nothing changed` (or what changed) after it, `library: cache written in …` after a start without a
usable cache, and no `startup: … needed before the first frame` (a page's state read before the first frame after
all: the move gained nothing there).

**The event loop step** (`EventLoop::build`, 45-220 ms cold, about 1 ms warm). Nothing of pocket-ui's own runs in
it: winit loads libwayland-client (with `wayland-dlopen`, not linked), connects to sway, reads the globals (one
round trip), binds them and makes a second round trip, in which sway announces the seat's keyboard and winit loads
libxkbcommon and makes an xkb context; the keymap itself is compiled later, inside the first frame's step. No EGL or
GL is involved (pocket-ui draws in software through softbuffer and wl_shm; pocket-core has the GL), no shaders,
no fonts (those are read on their own thread); the program itself links only libc, libm and libpthread (10 MB for
aarch64, `readelf -d`). What is cold in it: the two libraries (libxkbcommon is usually in
RAM already, sway maps it) and, above all, pocket-ui's own program: its winit and Wayland code is touched for the
first time here and each page of it is read from the card, in the same queue as everything the other start threads
read at that moment. Before perf/cold-start that queue held the game list's scan (hundreds of directory and inode
reads), started a millisecond earlier; with the list from the cache it holds one file. Expected: `event loop` well
below 45 ms cold; the probe above says how much of what remains is the libraries. sway's round trips cost little
(`sys`, which opens a second Wayland connection for the gamma ramps, took 5 ms in the same start).

## First press after a start

From outside ([below](#pocket-ui-against-rocknix-emulationstation)), 2026-10-26 on the Mini V2: Home was on the screen
192 ms after `systemctl start essway` (185-218), Down was pressed at 198-226 ms for 100 ms and did nothing; the next
press, about 355 ms later, moved the selection 18-35 ms after it. In all five starts.

**Why.** Looking at the recording again (`compare-results/full-20261026`, the frames between the start and the second
press): pocket-ui answered nothing to the first press. Its frames after it are the start screen unchanged, and in two
of the five starts it drew no frame at all from the press until well after its release. A press that reaches the UI
thread always asks for a frame, and nothing on a fresh shelf swallows one (the 600 ms after a wake start only with a
wake, the quick settings and the first-start setup were not open). So the press never reached pocket-ui: its pads were
not open yet. A device's events are queued for a reader only from the moment it opens the device; a press before
that is gone, and its release alone changes nothing. pocket-ui opened the pads on a thread that started only after
the event loop had been built (winit's connection to sway, `step.event_loop`), and walked every input device there
(the scan opens the nodes in name order, InputPlumber's pad `event9` last). That run kept no device log and pocket-ui
didn't log when its pads were open, so the moment itself is inferred, not measured.

**The change.** The pads and the touch screen are opened first thing in `main`, on a thread, before the logs, the type,
the game list and the event loop (`start_input` in `src/main.rs`). Their events wait in `pad::Early` until the event
loop is there, then go to the UI thread in the order they came; the Button layout is applied when they go, so a press
that waited gets the layout from the settings. Nothing is added to the UI thread and the first frame waits for nothing
new. Tests: `pad::tests` (a press before the open leaves only its release; a press while the UI thread still starts is
acted on once it is ready; the order across the hand-over).

**Checking it.** pocket-ui's log says when the pads could take presses, next to its first frame:

```
pad: <n> devices open <ms> ms after the start
first frame after <ms>ms (logs ..., event loop ..., drawn ... ms)
```

The speed guard keeps it as `start.pads` (from `main` to the pads open), next to `start.process`: it must stay below
it, and `compare` fails a run where it doesn't (`AFTER FRAME`, whatever the baseline says). From outside:
`scripts/rocknix-compare.sh run --ui pocket-ui -- --scenarios start`, where `start.input` should be the first press
after the start screen (`"presses": 1` in the samples).

**Still too late, until perf/nav-regression (2026-10-26).** On the Mini V2 the change above opened the pads 206-240 ms
after `main` (`start.pads`), the first frame came at about 60 ms: a press in between was still lost. Opening a node
is quick, closing one is not: the kernel waits for an RCU grace period, and for the driver to power the device down
when it had its last reader, 7-45 ms per node (`spmi_haptics` 37 ms, the DP jack 23 ms, the touch panel 15 ms). The
scan opens every node in name order, keeps the pads and closed the six it doesn't read on the spot, about 110 ms,
before it reached InputPlumber's pad (`event9`, the last name). The touch screen's search opened and closed the ten
other nodes once more. Now both hand what they don't keep to a thread that closes it (`pad::close_later`) and go
on at once: `pad: 5 devices open 2-5 ms after the start`, `start.pads` 3.0-5.7 ms (the medians of 12 passes, 42 starts). Tests:
`pad::tests` (the Mini V2's nodes with their close times: the pad is open within 30 ms, the others are closed in the
end; both scans use `close_later`), `latency.py selftest` (the `AFTER FRAME` rule).

## Waking

How fast a Pocket is back from sleep ([SLEEP.md](SLEEP.md#waking-timeline-instant-on-2026-10-24) has the steps and whose
they are). `--wake` measures only that, on the shelf and in the first game of `--systems`, three of each:

- **Deep sleep**: `power` through the FIFO (a short press of the power key; the harness's pocket-ui gets
  `POCKETUI_POWER_KEY=1`), the game saves, the system suspends with an RTC alarm set (`POCKETUI_SUSPEND_CMD="echo 0 >
  /sys/class/rtc/rtc0/wakealarm; echo +15 > …; systemctl suspend"`, `--asleep` seconds) and wakes by itself. Root only.
- **Light sleep**: idle until the screen goes dark (the harness gives pocket-ui `POCKETUI_SLEEP_MINUTE_SECS=4`, so
  Settings → System → Sleep & power's "Screen off after" is that many seconds per minute, and
  `POCKETUI_NO_AUTOSUSPEND=1`), then `a`. Not measured when "Screen off after" is Never.

| Metric | From | To |
|---|---|---|
| `wake.<shelf\|game>.screen_on` | the system is back: the kernel's `/sys/power/suspend_stats/success` went up | the backlight's `brightness` is no longer 0 |
| `wake.shelf.frame` | the same | pocket-ui's first shelf frame |
| `wake.game.game_on` | the same | pocket-core's `awake` in the log: the game runs again |
| `wake.game.frame` | the same | pocket-core's first frame after the wake (`core-frame wake`) |
| `wake.game.sound` | the same | pocket-core's sound fades in (`step audio: playing again`) |
| `wake.<shelf\|game>.input` | the same | the first of the D-pad presses the harness makes through InputPlumber from that moment on (`busctl … SendButtonChord`, every 100 ms or so) arrives in pocket-ui (its `input` line): the pad works |
| `wake.shelf.acts` | the same | the shelf shows such a press (the selection moved): includes the 600 ms in which the shelf takes no button after a wake (`WAKE_QUIET`) |
| `light.<shelf\|game>.screen_on`, `.frame`, `.game_on` | `a` written to the FIFO | as above |

The ends are seen by polling every 5 ms (the backlight, the log, the trace), so each number is up to 5 ms late. The
start of a deep wake is when the harness, thawed with everything else, sees the kernel's counter: the kernel's own
resume before that is not in these numbers. The power key's part of it is in the log line each wake writes
(`sleep: wake timeline, …: power key seen by the kernel -… ms`, from the key event's time stamp), which only a real
key press gives.

Everything these numbers need is there in older builds as well (the backlight, pocket-core's `awake` line,
pocket-ui's `input` lines), so the build of main gives a "before"; `frame`, `sound` and the steps below come only
from builds since feat/instant-on. Each wake also logs what it saw, including ROCKNIX's wake hooks (`hooks=`, not
ours, so not a number to compare):

```text
latency:   deep, game: game_on=2  screen_on=6  frame=21  sound=63  input=118  hooks=870
```

The trace steps of a wake: pocket-ui `sleep: light`, `sleep: suspending`, `sleep: resumed`, `sleep: game on`,
`sleep: screen on` (from the screen thread), `sleep: volume set` (twice), `sleep: wake hooks done`, `pad: lost` /
`pad: connected` if a pad's node was made again; `power-key <µs>` (when the kernel stamped the key); pocket-core
`core: awake`, `core-frame wake`, `audio: playing again`, `audio: stream reopened`.

On a device the measurement suspends it, and SSH may not survive that, so `latency.sh --wake` starts it there as the
unit `pocket-latency` and asks every 5 s whether it is done (its output: `/tmp/latency-run.log` on the device). In
the virtual device a suspend command that blocks 4 s stands in (`--mock-suspend 4`): no kernel, no backlight there, so
only the steps pocket-ui writes itself.

**Recipe for the Mini V2** (with the device lock; first once by hand that the RTC alarm wakes it from s2idle, step 1
of the device test plan in [SLEEP.md](SLEEP.md#device-test-plan-mini-v2-2026-10-22)):

```sh
# before: the build of main deployed (scripts/dev.sh deploy on main), the harness of this branch
scripts/devlock.sh me env POCKETUI_HOST=root@<device-ip> scripts/latency.sh --wake --out wake-before.json -- --systems gba:Maximum
# after: this branch deployed
scripts/devlock.sh me env POCKETUI_HOST=root@<device-ip> scripts/latency.sh --wake --phases --out wake-after.json -- --systems gba:Maximum
python3 scripts/latency.py compare wake-after.json wake-before.json
# the real power key (someone at the device): sleep with a tap, wake with a tap, a few times on the shelf and in a game
ssh root@<device-ip> 'grep -E "^sleep: (suspending|woke after|wake timeline|screen on in|awake|the screen was dark)|^pad: (lost|connected)|^audio: " /var/log/pocket-ui.log | tail -30'
```

Expected after: `wake.*.screen_on` and `wake.game.game_on` a few ms (before: ROCKNIX's hooks plus some 150 to 400
ms), `wake.game.frame` one or two frames after `game_on`, `wake.game.sound` some 40 ms after `game_on` (the queue filled
again; audible once the volume is set too, `volume set` in the log line), `wake.*.input` what InputPlumber needs after the thaw (if a `pad: connected` follows each wake, the pad's node
was made again and `input` includes the open), `light.*` as before or faster (a few ms). In the log, per wake: `sleep:
screen on in … ms` (the backlight write itself) and the timeline line, whose "power key seen by the kernel" is how long
the kernel took before pocket-ui could do anything.

## Comparing and noise

`scripts/latency.sh --compare` compares with `docs/latency/<target>.json` (`--baseline FILE` for another). A number
fails when it is both

- more than `--threshold` percent above the baseline (default 15), and
- more than `--min-ms` (default 3 ms) or, for CPU, `--min-cpu` percentage points (default 0.3) above it,

so that 2 ms becoming 3 ms is not a regression. Faster numbers are marked `faster` and never fail.

One number is checked on its own, against no baseline: `start.pads` must be below `start.process` of the same run
([First press after a start](#first-press-after-a-start)); `AFTER FRAME` fails the comparison.

Noise is handled three ways: every number is a median of several samples, the value is a median over three passes,
and a failed comparison measures again (another three passes) and judges the six passes together before it fails
(`--no-confirm` fails at once). Exit codes: 0 ok, 1 slower, 2 usage or setup, 3 the measurement failed (pocket-ui's
output is in `/tmp/latency-pocket-ui.log` on the target).

Compare only against a baseline taken the same way: the virtual device against the virtual device baseline on a
similar machine, the device against the device. In the virtual device the numbers depend on the Mac and what else
runs on it (other containers, builds). Two full runs on the same evening differed by up to 15 % for `start.cold` and
by a factor of two for numbers under 3 ms. A change that makes the UI slower by a few percent is found on the device,
not here.

When a change makes something slower on purpose (more work that is worth it), record the new baseline in the same
commit: `scripts/latency.sh --save-baseline`, and say why in the commit message.

Other options: `--wake` (only the wake from sleep, [Waking](#waking)), `--quick` (one short pass without warm-up: checks that the harness works, too noisy to compare),
`--deploy` (build and deploy to the virtual device first), `--out FILE`, and everything after `--` goes to
`latency.py measure` (`--runs`, `--moves`, `--starts`, `--opens`, `--cycles`, `--systems`, `--cpu-secs`, see
`scripts/latency.py measure --help`). `scripts/latency.py table|compare|merge` work on JSON files you already have.

## Phases: where the time went

`scripts/latency.sh --phases` (or `python3 scripts/latency.py phases <trace>` on a trace you have) takes the trace of
the run apart: for every Quit, pause menu and switcher it lists the `step` lines both programs wrote on the way, in
ms from the button, as the median over all of them (min-max, how often). Lines with `+` came after the first frame
(the pause menu's picture, the old game's teardown after a hand-over, the card writer). The trace is kept as
`latency-trace.log` next to `--out`.

```text
# quit.to_shelf: 21 times, ms from the button (median, min-max); + = after the first frame
  pocket-core: quit received                                   2.8  (0.2-18.5, 21x)
  pocket-core: save: state synced to the card                 15.9  (6.8-25.0, 21x)
  pocket-core: hand-over: saved, the next game may start      16.6  (7.1-25.4, 21x)
  pocket-ui: game-exited                                      17.7  (7.2-26.3, 21x)
  pocket-ui: first games frame                                20.5  (7.9-28.4, 21x)
  +pocket-core: torn down                                     29.9  (13.0-53.8, 21x)
  +pocket-ui: game process ended                              65.4  (36.7-82.4, 21x)
```

The steps, in order:

| Moment | Steps |
|---|---|
| Quit | pocket-ui `pause menu: quit`; pocket-core `quit received`, `save: battery save checked`, `save: state serialized`, `save: preview written`, `save: state synced to the card` (the fsyncs of the resume state), `exit save done`, `event loop left`, `achievements closed` (signed in only), `cpu, fan and rumble back`, `hand-over: saved, the next game may start` (hand-over cores), `system restored, reported`, `core unloaded`, `window and GL closed`, `torn down` (the sound stream closed too); pocket-ui `hand-over received` or `game process ended`, `game exit handled`, `game exit: shelf ready`, the first shelf frame; then `card writer: recents.json on the card` and `library.json` |
| Pause menu | pocket-ui `menu: window known` (sway's tree), `menu: screenshot taken` (grim, not for pocket-core), `menu: game paused` (pocket-core's `paused` came), `menu: in front`, `menu: built`, `frame waits for the full size` (only when sway still had the window half wide), the first menu frame; pocket-core `menu shot written`; pocket-ui `menu: picture in` |
| Switcher | the first switcher frame and how long it took to paint |

Below each moment: how long its first frame took to paint and at which window size.

## Baselines

### Virtual device

`docs/latency/vdev.json` is now the MacBook Air M4 baseline recorded in the 2026-10-29 night pass: four virtual CPUs,
5.77 GiB Docker MemTotal, one uCity GBC game, the default Shelf look, three measured passes. Every metric and its
spread are in the JSON; the before/after comparison and limitations are [below](#night-pass-cold-covers-and-large-libraries-2026-10-29).
The original M1 Max baseline is retained as `docs/latency/vdev-m1max-2026-10-23.json`; do not compare machines as if
an architecture or VM resource change were a UI regression. Neither file replaces the physical-device baseline.

#### Historical M1 Max baseline

`docs/latency/vdev-m1max-2026-10-23.json`, recorded 2026-10-23 on a MacBook Pro M1 Max, colima 6 CPUs / 8 GB, with other virtual
devices running at the same time (one of them at about 140 % CPU), commit `ca07625` plus the speed guard. Six passes
(two runs of `scripts/latency.sh` merged with `latency.py merge`), ROMs: uCity (GBC), MegaMan & Bass (GBA), Contra III
(SNES), the default Split look, 7 entries on Home (3 recent games, 4 systems).

| Metric | Value | Spread over the passes |
|---|---|---|
| `start.cold` | 67.0 ms | 47-73 ms |
| `start.process` | 65.2 ms | 47-71 ms |
| `nav.p50` | 1.7 ms | 0.9-2.7 ms |
| `nav.p95` | 5.5 ms | 4.1-8.0 ms |
| `open.settings` | 2.3 ms | 1.6-5.6 ms |
| `open.switcher` | 22.1 ms | 16-45 ms |
| `open.pause_menu` | 48.4 ms | 46-54 ms |
| `open.switcher_game` | 30.6 ms | 20-33 ms |
| `launch.gbc` | 90.1 ms | 81-102 ms |
| `launch.gba` | 125.6 ms | 109-138 ms |
| `launch.snes` | 107.7 ms | 99-201 ms |
| `quit.to_shelf` | 408.2 ms | 404-412 ms |
| `switch.game` | 360.4 ms | 334-385 ms |
| `cpu.shelf_idle` | 0.04 % | 0.02-0.18 % |
| `cpu.behind_game` | 0.03 % | 0.02-0.08 % |

What stands out (from the trace of these runs):

- Quitting takes about 400 ms in two equal halves: about 210 ms from A on Quit until pocket-core has written its
  resume state and ended (`game-exited`), then about 195 ms until pocket-ui's first shelf frame. Of the second
  half, `game_exited` itself takes about 14 ms; the rest passes before pocket-ui draws again (with temporary trace
  points; not yet explained, possibly waiting for the compositor after the focus and fullscreen change).
- Opening the switcher on the shelf (about 22 ms, up to 45) costs ten times as much as opening Settings (about 2 ms).
- `launch.gba` never resumed: mGBA's resume state is written at 388-396 KB and refused on the next start as
  "wrong size (397888 of 528448 bytes)" in every cycle, so the GBA game starts from the beginning each time.

### Mini V2

`docs/latency/device.json`: recorded on 2026-10-25 at 0eacac4 (3 runs, medians; performance profile "balanced",
fan quiet, games uCity (GBC), F-Zero: Maximum Velocity (GBA), Super Mario All-Stars & World (SNES)). To record it again
(with the device lock, see docs/DEVELOPING.md):

```sh
scripts/devlock.sh me env POCKETUI_HOST=root@<device-ip> scripts/latency.sh --save-baseline -- --systems gbc:uCity,gba:Maximum,snes:Stars
```

File-name parts in `--systems` must not contain spaces (the arguments pass through ssh).

| Metric | Value | Spread over the passes |
|---|---|---|
| start.cold | 55.4 ms | ±6.8 |
| nav.p50 / nav.p95 | 4.9 / 14.4 ms | ±1.1 / ±1.7 |
| open.settings | 10.1 ms | ±6.9 (4.5 ms in a run an hour before; the Settings categories and their transition, 2026-10-24, took it from 2.5 to 4-10 ms) |
| open.switcher | 15.3 ms | ±8.1 |
| open.pause_menu / again | 24.2 / 25.8 ms | ±4.4 / ±11.6 |
| open.switcher_game | 34.4 ms | ±14.8 |
| launch.gba / gbc / snes | 187.3 / 135.5 / 199.6 ms | ±8.5 / ±30.9 / ±6.7 |
| quit.to_shelf | 55.9 ms | ±7.0 |
| switch.game | 220.5 ms | ±31.2 |
| cpu.shelf_idle / behind_game | 0.00 / 0.00 % | not measurements: see below |

**The CPU numbers of this baseline are not measurements.** latency.py read them from schedstat, which the Mini V2's
kernel doesn't have, and took the missing files as 0 (fixed 2026-10-26, [CPU](#the-numbers)). The JSON still holds
those zeros; `compare` warns about a 0.00 % baseline without a `cpu_source`, and the next `--save-baseline` on the
device replaces them with numbers from `/proc/<pid>/stat`. What the comparison with ROCKNIX measured from
`/proc/<pid>/stat` (2026-10-26): pocket-ui 0.1 % of a core idle on the shelf and 0.1 % behind a running game
([below](#pocket-ui-against-rocknix-emulationstation)).

The first baseline (2026-10-23 at 8f158b4, before the quit, switch and pause-menu rounds below), for comparison:

| Metric | Value | Spread over the passes |
|---|---|---|
| start.cold | 72.1 ms | 66–100 |
| nav.p50 / nav.p95 | 7.7 / 14.8 ms | 4.4–9.1 / 13.5–16.0 |
| open.settings | 2.5 ms | 2.2–7.0 |
| open.switcher | 55.5 ms | 51–56 |
| open.pause_menu | 154.8 ms | 137–195 |
| open.switcher_game | 58.0 ms | 52–62 |
| launch.gba / gbc / snes | 165.7 / 189.5 / 238.1 ms | 145–187 / 159–192 / 227–246 |
| quit.to_shelf | 503.7 ms | 494–504 |
| switch.game | 709.1 ms | 613–711 |
| cpu.shelf_idle / behind_game | 0.00 / 0.00 % | not measurements either (schedstat, see above) |

Before feat/fast-switch: quit and switch are the slow ones (the switcher's sway focus change stalls a frame, and the
next game waits for the old process to end).

### Quit, pause menu and switcher (perf/round2, 2026-10-23)

The device took 462-504 ms to quit to the shelf where the virtual device took 52-92 ms, 155-175 ms to open the
pause menu (48 ms) and 55 ms to open the switcher (22-38 ms). What was changed, measured in the virtual device on the
same afternoon with the same harness (three passes each, the Mac busy with other builds; `--phases` for the steps):

| Metric | Before (142f853) | After | What did it |
|---|---|---|---|
| `quit.to_shelf` | 92.0 ms | 21.2 ms | the shelf comes back on pocket-core's `saved` line (hand-over), not after its teardown and exit; no `swaymsg` (pocket-ui is in front already); `recents.json` and `library.json` (four fsyncs each) on the card writer thread |
| `open.pause_menu` | 52.5 ms | 10.0 ms | no grim before the menu for pocket-core (it renders the picture itself, behind the menu 20-30 ms later); sway's IPC socket instead of two `swaymsg`; waits for pocket-core's `paused` instead (within a frame) |
| `open.pause_menu_again` | 43.7 ms counted, 255 ms to the first full-size frame | 11.8 ms | no frame at the old half width after a Continue: that one held sway's resize for 200 ms (the old harness counted it) |
| `open.switcher` | 38.6 ms | 8.2 ms | the glow behind the cards in integers and separable (15 ms → 3 ms on the Mac), kept between opens |
| `open.switcher_game` | 43.9 ms | 26.6 ms | the same, the frame size of the game read from the PNG header instead of decoding the picture, a sampled glow source; `blit_scaled` without a division per pixel |
| `switch.game` | 133.4 ms | 116.0 ms | (noise; the switch path is unchanged but for the IPC) |

Everything else stayed within the noise (`launch.*`, `nav.*`, `open.settings`, CPU).

**Why the device was so much slower, ranked** (hypotheses from the code and the virtual device; the device run with
`--phases` below confirms or refutes each in one go):

1. **fsync on the SD card on the UI thread before the first shelf frame** (likely the largest part of the second half).
   `game_exited` wrote `recents.json` and `library.json` with `fsutil::write_json`: per file the file, its directory,
   the `.bak` and its directory, eight card flushes in all, typically 10-50 ms each on an SD card, about nothing in
   the virtual device (the Mac's disk). Fixed: `fsutil::write_json_soon` (card writer thread, in order, every later
   `write_json` waits for it). On the device: the steps `+card writer: recents.json on the card` and
   `library.json` minus `game exit: shelf ready` are what the shelf used to wait for.
2. **pocket-core's teardown and exit on Freedreno** (the first half). After the exit save the old process unloads the
   core, destroys its GL context and EGL surface (Mesa joins its threads, the kernel waits for the GPU when the DRM
   file closes), closes its window and its PipeWire stream, whose `Audio::drop` runs ROCKNIX's `/usr/sbin/quantum`
   (a shell that sources `/etc/profile` and runs `pw-metadata` twice: 10 ms here, more on the device), and then the
   process ends. 48-67 ms in the virtual device with llvmpipe. Fixed for the hand-over cores (Gambatte, mGBA,
   Nestopia, Snes9x, Genesis Plus GX): off the path. On the device: `+torn down` − `system restored, reported` and
   `+game process ended` − `+torn down`.
3. **The exit save itself**: the resume state through `write_atomic` (fsync of the file and the directory) and the
   battery save if it changed (two more). Not changeable (the save must be on the card before the shelf says the game
   is done). On the device: `save: state synced to the card` − `save: preview written`.
4. **Helper processes on the UI thread**: `swaymsg` for the focus change after the game, `swaymsg -t get_tree`,
   `grim` (screencopy through the GLES renderer, 4 MB of PPM through a pipe, parsed) and the second `swaymsg` for the
   menu, each a process start plus `proc::wait`'s polling (1 to 10 ms naps). Fixed (IPC socket, no grim for
   pocket-core). `POCKETUI_SWAY_IPC=0` in `dev.env` brings `swaymsg` back for a comparison.
5. **The half-width frame after a Continue** (pause menu only, every menu after the first of a game): 200 ms in the
   virtual device; on the device the same sway, so presumably the same. The old `open.pause_menu` never saw it (no
   Continue in the harness; the frame counted). Fixed; `open.pause_menu_again` measures it now.
6. **CPU clocks after the game** (unconfirmed): pocket-core puts the CPU governor back (`cpu, fan and rumble back`)
   before pocket-ui's frame; on "balanced" there is nothing to undo. If a profile was set, the shelf's first frame
   may run at low clocks. The step shows how long the restore took, not the clock afterwards.
7. **The first shelf frame's own paint**: its `paint` line (1 ms here; nav frames suggest 5-10 ms on the device).

What is still in the device's way after this branch: the exit save's fsyncs (3), the first frame's paint (7), for
cores that don't hand over (3D cores, PPSSPP, Flycast, …) the teardown (2), and for the pause menu up to a frame for
pocket-core to answer `paused`.

**Recipe for the device** (one run each, with the device lock; the systems as for the baseline above):

```sh
# both runs with this branch's scripts (the harness is newer); before: the build of main deployed
scripts/devlock.sh me env POCKETUI_HOST=root@<device-ip> scripts/latency.sh --phases --out before.json -- --systems gbc:uCity,gba:Maximum,snes:Stars
# after: deploy perf/round2 (scripts/dev.sh deploy), then
scripts/devlock.sh me env POCKETUI_HOST=root@<device-ip> scripts/latency.sh --compare --phases --out after.json -- --systems gbc:uCity,gba:Maximum,snes:Stars
python3 scripts/latency.py compare after.json before.json
```

The build of main writes no `step` lines but the `switch-time` lines of its log (`grep switch-time
/tmp/latency-pocket-ui.log` on the device, and `saved … (KB, ms)`): they give the first half of its Quit. Single
changes can be switched off for an A/B in `/storage/pocket-ui/dev.env`: `POCKETUI_FAST_SWITCH=0` (Quit waits for the
process again), `POCKETUI_SWAY_IPC=0` (`swaymsg` again).

### Screen frame off the first frame, Settings' battery (perf/bezel-cache, 2026-10-23)

On the Mini V2 (`latency-device-2026-10-23-pm`, 30b763a) `launch.gba` went from 166 to 235 ms and `open.settings` from
2.5 to 7 ms. pocket-core drew the screen frame on its render thread before the first frame (`bezel: prepared in
27-72 ms`; `first frame` in its start line 57-81 ms where it was 18 ms at 8f158b4), and twice per start (the first frame
comes before sway's resize). Now the frame is read from a RAM cache or drawn on a thread and faded in once it is
there ([FRAMES.md](FRAMES.md#cache)); Handheld and Monitor are drawn on all cores. Virtual device, the same container
and afternoon, three passes each, `latency.py compare`:

| Metric | Before (30b763a) | After | |
|---|---|---|---|
| `launch.gba` | 130.0 ms | 100.9 ms | −22 % |
| `launch.gbc` | 84.5 ms | 61.0 ms | −28 % |
| `launch.snes` | 87.8 ms | 70.2 ms | −20 % |
| `switch.game` | 137.3 ms | 104.6 ms | −24 % |
| pocket-core `first frame` step (28 starts) | 17.2 ms median, 48.8 max | 8.0 ms median, 12.3 max | |
| `open.settings` | 3.0 ms | 3.7 ms | noise (the paint is 0.4-1.5 ms, see below) |

Everything else within the noise. The frame itself is unchanged pixel for pixel: `frame_fingerprints` (every style at
six geometries) gives the same hashes before and after, and at-rest screenshots of GBC, GBA and SNES differ only inside
the moving picture. In a burst of screenshots of a start the bands stay black until the frame is there and then rise
in 250 ms to their brightness at rest.

`open.settings`: profiled in the virtual device (temporary timers in `paint_settings` and around the frame), no row
computes anything slow (each value under 0.2 ms, Bluetooth's `system.cfg` read included); the page paints in 0.4-1.5 ms,
the rest is the clear of a fresh 5 MB buffer (up to 7 ms when its pages are first touched) and the Motion page
transition, a full-frame pass added after the device baseline (1616129). On the device the status pill also read
`/sys/class/power_supply/battery/capacity` in every frame of every Settings page, which on the Mini V2 is answered by
the power chip's firmware; the pill now shows the status corner's figures, which its thread reads every 3 s while the
corner or the pill is on screen (`status_corner`), never on the UI thread.

On the device (after `scripts/dev.sh deploy` of this branch, with the device lock):

```sh
scripts/devlock.sh me env POCKETUI_HOST=root@<device-ip> scripts/latency.sh --compare --phases -- --systems gbc:uCity,gba:Maximum,snes:Stars
ssh root@<device-ip> 'grep -E "^bezel:|game was started|^battery: capacity" /tmp/latency-pocket-ui.log | head -60'
```

Expected: `launch.gba` back at or under the baseline (166 ms); the start lines' `first frame` about 15-20 ms for every
system; per start two `bezel: prepared in … ms (from the cache)` (1-5 ms; `(drawn, kept in the cache)` the first time
after a boot), one `bezel: ready … ms after it was asked for` and one `bezel: uploaded in … ms` (a `prepared` line may come before
the start line: it is written on the frame's thread, while the game starts). By eye: start a GBA and a GB game twice from the shelf, the
frame fades in from black around the picture without a jump; Settings shows the battery in its pill on the first open.

### Settings' first frame (perf/settings-open, 2026-10-24)

On the Mini V2 `open.settings` (START on the shelf until the first frame of Settings) was 6.3-13.2 ms where the
baseline above has 2.5 ms. Since then Settings opens on five categories (a line under each), the top-right pill shows
time, Wi-Fi symbol and battery, every string goes through the German table at the text painter, and a page slides and
fades in (`motion.rs`, a full-frame pass the baseline did not have). Profiled on the build computer with
`pocket-ui --snapshot bench-settings` (the shelf painted first, then the first frame as `draw` makes it; 1240 x 1080,
median or minimum of many runs, the computer busy with other builds, so the figures move by a third between runs):

| Part of the first frame | Before | After | What it was |
|---|---|---|---|
| German lines, first sight | 1.4 ms (German only) | 0 | `Text::fit` gives the description back in German and `draw` looked it up again as English: all patterns of the table ran over each of the five lines (0.28 ms each, once per string). Now what `fit` and `wrap` return is marked German (`i18n::Cache::is_german`), the names of the buttons too |
| Letters drawn for the first time | 0.4 ms (73 glyphs) | 0 | The categories page is the first to paint the lines under the names at 30 px. `Text::prewarm` draws ASCII, the German letters and Settings' marks at the sizes of the menus (21, 28, 30, 36, 46; titles 50) on a thread once the first frame is on screen; the cache takes them in at its first miss |
| Page transition | 1.1 ms | 0.7 ms | A line of nothing but the page colour is read, not shifted, mixed and written again (the result is the same: `slide_fade_row` test) |
| Wi-Fi symbol | 0.1-0.2 ms | 0.01 ms | 15 000 samples with a square root each, in the frame; now a mask of 0-255 coverages per size, made by the status corner's thread before the first frame (`status_corner::paint_wifi`) |
| Text blending | 0.1 ms | 0.05 ms | Glyphs inside the picture are blended along its rows, without a bounds test per pixel, and a fully covered pixel is written as it is |
| First frame in all | 2.4 ms (English), 3.2 ms (German) | 1.5-1.7 ms (both) | The same picture, pixel for pixel: every state of `tests/i18n-states.txt`, English and German, has the same hash before and after |

What is left of the 1.5 ms is the clear of the 5 MB buffer (0.5 ms), the transition (0.7 ms) and the rows' text (0.3 ms);
the Mini V2 takes three to four times as long for each. Not a cause (measured): the Later values (the categories read
none; a category's list reads from memory), the pill's clock (5 us), measuring and wrapping the descriptions (a few us).

`tests/settings_open.rs` guards it (also in `cargo test`): in English and German the first frame draws no glyph of its
own, runs no string through the patterns, and costs at most 1.6 times the same frame again plus 0.3 ms (minimum of five
runs). To look at the numbers: `pocket-ui --snapshot bench-settings x.png` prints them.

On the device (with the device lock, after `scripts/dev.sh deploy`):

```sh
scripts/devlock.sh me env POCKETUI_HOST=root@<device-ip> scripts/latency.sh --compare -- --systems gbc:uCity,gba:Maximum,snes:Stars
```

Expected: `open.settings` back towards the baseline's 2.5 ms (3-4 ms with the page transition and the pill, which the
baseline did not have), more so with the interface in German.

### A move on Home (perf/nav-regression, 2026-10-26)

`nav.p50` on the Mini V2 was 4.6 ms at 9df5564 and 8.0 ms at b5e1a6e (two runs of `--compare` on the evening of
2026-10-26), `start.process` 51.7 against 57.0 ms. Measured again the same night, the two builds deployed in turn
(nav and starts only: `scripts/latency.sh --out f.json -- --systems= --opens 1 --cpu-secs 2`, three passes each):

| Build | nav.p50 (ms) | start.process (ms) | start.pads (ms) |
|---|---|---|---|
| 9df5564 | 5.8, 5.6, 5.5, 5.3; with 300 moves a pass 5.6, 6.7 | 59.1, 58.1, 59.1, 61.3 | - (no `pad-open` line) |
| b5e1a6e | 5.5, 5.4 | 62.7, 60.6 | 231, 217 |
| b5e1a6e + `close_later` | 6.3, 6.6, 8.8, 5.4; with 300 moves a pass 5.3, 6.0 | 54.3, 52.9, 54.2, 57.4, 56.2, 61.7 | 4.9, 4.6, 4.0, 3.8, 4.2, 4.5 |

So `nav` didn't move: its passes spread over 4.4-9.2 ms in every build, more than the difference in question, and
a whole run can sit at 8.8 ms one moment and 5.4 ms the next with the same build (the last two rows: 06:40, then
9df5564 at 5.3 ms and this build at 5.4 ms right after). Compare two builds deployed in turn, not one run against the
baseline. What
decides it on the device is not pocket-ui's work: `pocket-ui --snapshot bench-shelf:cards` (below) gives the frame
after a move 1.4 ms of CPU on the Mini V2 in both builds (Split 1.2 ms), the trace of the same run 3-14 ms from the
press to the frame for the same entry, depending on the clock the CPU is at after 150 ms idle and on the compositor.
The Home panels (Curator, Favorites, pinned shelves, a32f520) cost nothing measurable per frame: nothing is decoded,
no letter drawn, no string run through the German patterns. `start.process` was about 3 ms worse with the pads'
closes in the start and mostly better without them (median 55 ms against 59 ms for 9df5564; the closes ran beside the
type and the event loop), within the noise of a single run.

`pocket-ui --snapshot bench-shelf[:<look>] x.png` (no picture) moves down to the end of Home's list and up to its
top and prints, for the first frame `draw` makes after each move: `items moves p50_us p95_us max_us glyphs patterns
worst=<entry>`, in thread CPU time, with Curator's index there and every picture in the cache, as a moment after the
start. With a copy of a device's data folder it runs on the device too (`POCKETUI_DATA=/tmp/copy`; a snapshot leaves
the command FIFO of the pocket-ui beside it alone). `tests/shelf_moves.rs` runs it over the made-up library of the
Curator snapshots in every look (German, Split also English) and fails when frames draw letters of their own or run
strings through the patterns (not the Cabinet, whose cases are caught at sizes in between on a busy computer), or a
move costs more than 6 ms (p50) or 8 ms (p95); about 1-2 ms on the build computer, the Cabinet 3 ms.

### Main 87ffc1e against the baseline (2026-10-26)

`scripts/latency.sh --compare -- --systems gbc:uCity,gba:Maximum,snes:Stars` on the Mini V2 at `87ffc1e` (performance
balanced, fan quiet, the owner's settings: Cards, German), three passes and, as it failed, three more; the six judged
together, against `device.json` (`0eacac4`):

| Metric | 87ffc1e (6 passes) | Baseline | Verdict |
|---|---|---|---|
| start.cold / start.process | 64.2 / 59.2 ms | 55.4 / 48.7 ms | **slower** (+15.9 / +21.5 %) |
| start.pads | 3.9 ms | - | below start.process in every pass |
| nav.p50 / nav.p95 | 7.2 / 13.9 ms | 4.9 / 14.4 ms | ok (p50 under the 3 ms floor; its passes 6.0-7.9 ms) |
| open.settings / switcher | 10.3 / 12.8 ms | 10.1 / 15.3 ms | ok |
| open.pause_menu / again | 26.9 / 25.9 ms | 24.2 / 25.8 ms | ok (one pass of the first three at 33 ms) |
| open.switcher_game | 33.6 ms | 34.4 ms | ok |
| launch.gba / gbc / snes | 146.1 / 118.1 / 168.6 ms | 187.3 / 135.5 / 199.6 ms | faster |
| quit.to_shelf | 64.2 ms | 55.9 ms | ok (+14.8 %; passes 53-80 ms) |
| switch.game | 233.4 ms | 220.5 ms | ok |
| cpu.shelf_idle / behind_game | 0.35 / 0.25 % | 0.00 / 0.00 % | measured (`cpu_source` `stat`); the baseline's zeros are none |

`start.process` was 57-63 ms in all six passes: not noise against the baseline, but the level the builds since
`9df5564` already had ([above](#a-move-on-home-perfnav-regression-2026-10-26): 52-62 ms), so it came in between
`0eacac4` and `9df5564`, not with this build. Because of it the baseline was **not** saved again: a new
`device.json` would take the slower start in as the norm. Open: find where the 10 ms of `start.process` went (the
start line's steps, `--cold`), then save the baseline with the CPU numbers above.

### Night pass: cold covers and large libraries (2026-10-29)

MacBook Air M4, aarch64 virtual device, four virtual CPUs, Docker MemTotal 5.77 GiB, 1240×1080 headless sway;
only the owned `vdev-perfn` container was used. No physical device was contacted. Before: `e5a7d29` plus the
benchmark's game-list selector. After: measured `11e3ef4` (the same change rebased as `0674b21` onto `4dbc0ad`).
Exact binaries, every pass, configurations, sampled memory and warm-bench output:
[raw A/B results](latency/perf-night-2026-10-29.json).

Generated libraries contain exactly 2,000 or 5,000 GBC games and 1200×1700 RGB PNG covers, from
`scripts/perf-library.py`; uCity is the real homebrew used for launch tests. Split and Cabinet each ran three passes:
`latency.sh -- --systems= --nav-system gbc --look <look> --runs 3 --starts 3 --moves 60 --opens 2 --cycles 0
--cpu-secs 2 --no-warmup`. The fixture was regenerated and the UI deployed before each group. The other Air
container was idle (about 1% CPU). These are process-cold starts with the Linux page cache warm, not a power-on or
an SD-card-speed measurement. Medians of the three passes, milliseconds, **before / after**:

| Library / look | Start to first shelf | Navigation p50 | Navigation p95 |
|---|---|---|---|
| 2,000 / split | 31.9 / 30.3 | 23.8 / 2.4 | 25.5 / 4.1 |
| 2,000 / cabinet | 37.8 / 32.4 | 9.6 / 8.5 | 16.3 / 14.1 |
| 5,000 / split | 31.2 / 24.9 | 23.8 / 1.3 | 25.6 / 2.6 |
| 5,000 / cabinet | 37.4 / 32.5 | 8.8 / 8.0 | 17.1 / 13.0 |

The user-visible change is Split's cold-image scrolling: roughly 24 ms becomes 1–2 ms. Cabinet already loaded covers
on its own thread; its smaller differences are not attributed to this patch. Warm game-list frame CPU time stays
essentially identical: Split 0.20–0.21 ms, Cabinet 0.51–0.53 ms (both library sizes; 240 moves after two warm-up passes,
`--snapshot bench-shelf:<look>:gbc`). No measured move ran the German patterns or drew a new glyph in those runs.

Opening uCity's box in the 2,000-game Split fixture, first X per process, three passes:
**44.8 → 16.4 ms**. Repeated opens are
21.6 → 20.1 ms (the median including the first open),
so the gain is the cold open. The baseline had decoded and fitted two cover sizes on the UI thread, including the
old card's unused small cover. Now only the required cover is requested, and the existing `pictures` worker decodes
and fits it; the UI paints the images already available and redraws when one arrives. The initial live frame may show
the existing case/console placeholder while a cover loads; the settled image and controls are unchanged.

Memory: a separate, single A/B pair on the 5,000-game Split library, **500 moves 75 ms apart**, sampled UI RSS every
100 ms, including starts: **260.6 → 81.7 MiB observed peak**. The old fitted-cover cache retained every game and size
seen; live fitted covers now share the existing 16-entry image cache with screenshots and title screens. The 60-move
runs are too short for a large peak-RSS difference, and Cabinet retains its separate artwork caches: their per-run
memory is in the raw results. This is UI RSS, not the container's memory or pocket-core's memory.

Ordinary one-game vdev (default Shelf look, uCity), three passes, five starts / 60 Home moves / three menu opens /
two game cycles per pass; `--cpu-secs 3 --no-warmup`, before and after. Milliseconds:

| Moment | Before | After |
|---|---|---|
| Cold process to first shelf frame | 30.2 | 27.1 |
| From main() to first shelf frame | 28.2 | 25.9 |
| Ordinary Shelf navigation p50 / p95 | 4.5 / 7.9 | 3.3 / 5.2 |
| First box frame (ordinary one-game library) | 19.9 | 16.4 |
| Box close | 4.0 | 3.4 |
| Pause open / open again | 12.9 / 14.1 | 16.5 / 9.6 |
| Pause close (UI hand-off) | 7.3 | 2.7 |
| Quick Settings open / close | 3.6 / 2.2 | 1.4 / 1.3 |
| Settings open | 3.1 | 1.4 |
| Switcher open | 5.4 | 3.6 |
| uCity / pocket-core launch | 74.3 | 66.5 |
| Quit to shelf | 15.3 | 11.1 |

CPU behind the game and on the shelf measured 0.0% in both medians, read from `/proc/<pid>/stat`; with three-second
windows its resolution is about 0.33 percentage points, so this is not a claim of zero CPU work. The night's rule was
to leave sub-10 ms drifts alone. These small start/menu/game differences are not claimed as optimizations; no
regression exceeds that floor. The guard's normal 15% / 3 ms defaults remain unchanged. `close.pause_menu` times the
UI moving behind the game, not a new core frame or panel reveal.

`POCKETUI_FRAME_BENCH=1` on the Air repeats Home, Settings, pause and Quick Settings. The 5,000-game after build
costs 0.206 / 0.154 / 0.069 / 0.102 ms of median frame CPU respectively. Previously that benchmark painted the Quick
Settings overlay once, then timed only its background; it now includes the overlay every time. Its old Quick value
is therefore not a before/after performance comparison. PNG encoding and compositor waits are excluded.

Five sampled profiles on x86 (`perf`, gate optimization with symbols/frame pointers), locating startup, cold list
covers, Cabinet, the first box and pause/quick background work:
[profile summaries and caveat](latency/perf-night-2026-10-29-profiles.txt). Reproduce with `scripts/perf-profile.py`
and the generated fixture. Cover decode/box filtering were on the UI thread; Cabinet decode was already on
`cabinet-art`. Warm raster time was dominated by clear, text and blit, with no player-visible problem to fix there.

Validation: 88 settled PNGs (English/German × dark/light, covers present, Motion Off, old/new paired within the same
clock minute) are **byte-identical**; both views were inspected. A worker test checks real PNG alpha conversion and
box-filtered output. Latency self-tests pass, all-target Clippy exits 0 with existing warnings, and both ARM release
binaries build. The first overloaded build-host gate hit the existing Bluetooth fake pair/trust timeout; that test passed
immediately in isolation. The merge queue runs the full gate again with four test threads. No assertions were weakened.

The Mini V2 baseline is untouched. After the night restrictions are lifted, the main session should run the real-device
speed guard and briefly hold Down through a large covered library, open/turn/close a box, then launch/return from uCity;
verify that covers settle without stutter and the same pictures and controls remain.

### Launch animation (feat/box-and-launch, 2026-10-28)

The Cabinet look started a game only when its cartridge reached the console's slot, 260 ms after A
(`cabinet::LAUNCH_AT`, gone). Now every look starts the game at A, and the Cabinet and Shelf looks play their launch
animation over the shelf while the emulator loads (`app_cabinet::launch_animation`). Virtual device on the MacBook Air
M4 (`VDEV_NAME=boxl`), uCity (GBC) the one game, `latency.sh -- --look <look> --systems gbc --runs 3 --starts 2
--moves 60 --opens 2 --cycles 3` (the Cabinet with `--moves 0`: with one game there is nothing to move between; `--look
cabinet` is new: Home there is every shelf at once, so games are started from it, not from a system's list). Medians of
the three passes, main 6eb648e against the branch:

| Look | `launch.gbc` before | after | `nav.p50` before / after | `nav.p95` before / after |
|---|---|---|---|---|
| Cabinet | 332.6 ms | 67.5 ms | – | – |
| Shelf (animation on) | 67.3 ms | 68.4 ms | 4.5 / 4.5 ms | 7.5 / 7.9 ms |
| Split (no animation) | 65.0 ms | 65.7 ms | 2.9 / 3.0 ms | 3.7 / 3.8 ms |

So the animation costs the start about a millisecond (the frames pocket-ui paints while the game loads, on cores the
emulator doesn't use), and the Cabinet's starts are 265 ms faster than before. In the virtual device a GBC game is up
after 65 ms, before the cartridge is in (180 ms): there the game covers the animation, as it should.

### Launch rigs (feat/insert-animations, 2026-10-31)

Cozy + Playful, main (eeb9917) against the branch, interleaved, on the RG35XX Plus (H700, laid out at 1280x960, `bench-playful`, thread CPU of `paint`, two rounds):

| Frames | main p50 / p95 | rigs p50 / p95 |
|---|---|---|
| PS1 (lid) | 23.0 / 25.5 ms | 24.3 / 26.8 ms |
| PS2 (tray) | 23.2 / 26.1 ms | 24.9 / 27.4 ms |
| SNES (cartridge, `over`) | 23.1 / 25.9 ms | 25.5 / 28.0 ms |
| DS (behind the shell) | 22.5 / 25.5 ms | 23.6 / 26.1 ms |
| Wii (slot, `over`) | 24.3 / 27.2 ms | 26.4 / 28.9 ms |

A to the game's first frame, real starts (`launch.gbc`, uCity on a fixture, twelve each, pocket-core of the same build):

| | RG35XX Plus main → branch | Mini V2 main → branch |
|---|---|---|
| Cozy | 1065 → 1617 ms | 1072 → 1615 ms |
| Cozy, a button 200 ms after A | 1065 → 355 ms | 1070 → 212 ms |
| Quick | 410 → 412 ms | 77 → 78 ms |

Main's skip left the game's window waiting whenever pocket-core hadn't opened its pads yet (`startup::HOLD_SKIP` now tells it). First frame of the animation: 10 ms more (the rig's pictures are decoded then; the game's process is already started). Peak memory 2 to 2.5 MB more, gone with the animation.

`latency.py` puts back what a real start writes outside its fixture, `system.cfg` and `savestates/<system>/` (`Preserve`, also on a failure, Ctrl-C and SIGTERM; `python3 scripts/latency.py selftest`), and starts the pocket-core of the build it measures.

### Playful animations (feat/playful, 2026-10-30)

What a frame of each playful animation costs on the RG35XX Plus (H700, 640x480, laid out at 1280x960), the extra on and
off, from `pocket-ui --snapshot bench-playful:<system>:<what>` and `POCKETUI_BENCH_FRAMES=120 --snapshot
box:psx:castlevania` run on the device on a fixture copy (thread CPU time of `paint`, every frame 16 ms apart; two
interleaved rounds, they agree within 0.5 ms):

| Frames | Playful on p50 / p95 | off p50 / p95 |
|---|---|---|
| Shelf launch, GBA cartridge | 23.1 / 26.1 ms | 23.0 / 26.1 ms |
| Shelf launch, DS card | 22.8 / 25.6 ms | 22.8 / 25.8 ms |
| Shelf launch, PS2 tray | 23.0 / 25.8 ms | 24.0 / 26.7 ms |
| Shelf launch, GameCube lid | 23.4 / 26.4 ms | 24.5 / 27.4 ms |
| Shelf launch, PlayStation lid | 22.9 / 25.8 ms | 25.3 / 28.3 ms |
| Ten moves: spring, turn, wiggle | 6.3 / 12.4 ms | 6.3 / 12.5 ms |
| The open box, its disc turning | 20.3 / 22.0 ms | 16.8 / 18.7 ms (still) |

The pull-out's spring and wiggle cost nothing measurable and stay well inside a 60 Hz frame. The tray, lid and seat cost
nothing either: the launch's frames are as dear with them as without (the disc goes over the drive at its size, made
once, instead of being scaled smooth each frame), and the launch was already about 23 ms a frame on the H700 (about 40
fps): a pass dimming the whole 1280x960 picture behind the console and the fade's pass cost most of it (memory
bandwidth). `Canvas::fade` now skips its pass while there is nothing to fade (the first 0.6 of the animation), which
took a fifth of those frames under 16.7 ms. The box's turning disc costs 3.5 ms (nearest pixel in fixed point; the
first, bilinear version cost 29 ms); the box itself already takes 16.8 ms there, so the disc turns at every other
refresh where the box costs more than half a refresh (`BoxView::spin_wake`), at every refresh on faster handhelds.

## pocket-ui against ROCKNIX (EmulationStation)

The numbers above come from pocket-ui's own trace, which EmulationStation and RetroArch don't have. For the
comparison with what a Mini V2 runs without pocket-ui, `scripts/rocknix-compare.sh` measures both from outside, the
same way: presses written into the Pocket's pad device (the path through InputPlumber a finger's press takes), the
screen recorded from sway with each frame's commit time. Method, scenarios, the plan of presses per UI and how to run
it: [COMPARE-ROCKNIX.md](COMPARE-ROCKNIX.md).

**Method in short.** Same games and cores on both (uCity on gambatte, F-Zero: Maximum Velocity on mgba, Super Mario
All-Stars on snes9x: ROCKNIX's defaults and pocket-core's), RetroArch with ROCKNIX's defaults (resume states on,
bezels on), the theme of the device. Per press: `react` (the first frame that differs, outside what was moving
anyway) and `done` (the first frame of the step's end state). Cold start from `systemctl start essway`; deep sleep
through the power key with the RTC alarm; CPU from `/proc` without the recorder; five minutes of each game from its
power-on with the battery's current and voltage.

**Limits.** Every time is on the 16.7 ms grid of sway's commits, and the panel shows a frame up to 16.7 ms after its
commit (the same for both): differences under one refresh are not differences. The time is the compositor's, not the
light's. The recorder makes sway copy each frame while presses are measured: only a band across the middle of the
screen, because sway copies a whole screen of pocket-core in ~50 ms against ~8 ms for RetroArch (see COMPARE-ROCKNIX.md);
with the band both get 57-60 frames a second. CPU and power are measured without the recorder. EmulationStation's numbers depend on its theme (`es_theme` in the run's `device.json`).

**Numbers** (Mini V2, 2026-10-26, ROCKNIX 20261001, pocket-ui main 9df5564, EmulationStation's default theme
es-theme-art-book-next, on the charger; this card's library of 332 games). Medians, with the range over the runs in
brackets; n = presses measured (3 per game for starts, pause menus and quits). Times end at the first frame sway
committed that shows the result, so each is on a 16.7 ms grid. Files: `docs/latency/rocknix-compare-2026-10-26-es.json`
and `-pocket-ui.json` (every single value with how it was found).

| What | EmulationStation + RetroArch | pocket-ui + pocket-core |
|---|---|---|
| Cold start: `systemctl start essway` until the start screen | 2083 ms (1900-2160, n=5) | 192 ms (185-218, n=5) |
| Cold start: until the first press is acted on | 2136 ms (1960-2215) | 589 ms (587-606) |
| List move (GBA list): the screen answers | 41 ms (18-50, n=30) | 12 ms (8-35, n=30) |
| List move: the move finished | 41 ms (no animation) | 157 ms (128-167; the 120 ms slide) |
| Game start until the game is on screen, GBC / GBA / SNES | 1795 / 1687 / 2031 ms | 203 / 188 / 231 ms |
| Presses for a game start (resume) | 3: A, Right, A (Savestate manager) | 1: A |
| Pause menu until it is there, GBC / GBA / SNES | 241 / 182 / 190 ms (HOME+X) | 183 / 164 / 165 ms (HOME, incl. its 100 ms tap) |
| Quit to the list, GBC / GBA / SNES | 857 / 901 / 934 ms (HOME+START twice) | 119 / 128 / 98 ms (Quit in the pause menu) |
| Switch from uCity to F-Zero: the waits of every press | 3544 ms (3385-6395), 8 presses | 759 ms (659-4291), 4 presses |
| Deep sleep: system back until the backlight is on | 0.3 ms (its backlight value stays on) | 11 ms (6-11) |
| Deep sleep: system back until the first frame | 21 ms (20-23) | 4 ms (3-9) |
| Deep sleep: system back until a press is acted on | 55 ms (36-56) | 699 ms (689-703; 600 ms of them its no-button guard after a wake) |
| CPU on the start screen, idle: the UI / the whole system | 6.1 % / 21.0 % | 0.1 % / 10.2 % |
| CPU behind a running game (uCity, resumed): the UI / the emulator | 0.0 % / 8.2 % | 0.1 % / 10.7 % |
| Playing 5 min from power-on, the emulator's CPU: GBC / GBA / SNES | 12.0 / 44.7 / 32.5 % | 10.1 / 24.9 / 17.1 % |
| Playing 5 min from power-on, the whole system: GBC / GBA / SNES | 28.4 / 59.4 / 48.5 % | 27.6 / 40.1 / 36.7 % |
| Battery power while playing | not measured: on the charger (battery Full) | not measured |
| A press through InputPlumber to its virtual pad | 1.3 ms | 1.4 ms |

CPU in % of one core (the whole system: 800 % would be all eight cores). How to read it:
- pocket-ui's numbers here are in line with its own trace above (Mini V2 baseline: launch 136-200 ms, quit 56 ms, pause menu 24 ms
  after the button's release): from outside, the 100 ms tap and the compositor come on top.
- The slower parts of pocket-ui are design: the list's 120 ms slide (EmulationStation's list moves without one), the
  pause menu opening when HOME comes up (HOME is also the shortcut key), the 600 ms after a wake in which the shelf
  takes no button. After its cold start, the first press (at about 200 ms) was not acted on, the second (about 570 ms)
  was: pocket-ui shows Home some 400 ms before it takes a button.
- RetroArch's game start includes its "state loaded" resume; its quit writes the auto save first.
- The switch's ranges: one slow run each (RetroArch's game start in it, pocket-ui's 4.3 s once); the medians are of 3.
- CPU behind a running game is uCity at the resume point of each (the two resume files differ); from power-on (the
  play rows, the same scene for both) pocket-core needs 16 % (gambatte), 44 % (mgba) and 47 % (snes9x) less of a core than RetroArch with the same core.
- The speed guard's `cpu.shelf_idle` / `cpu.behind_game` read 0.00 % on the Mini V2 because its kernel has no
  schedstat (`CONFIG_SCHEDSTATS` off) and `latency.py` read only `/proc/<pid>/task/*/schedstat`. Fixed 2026-10-26:
  `latency.py` reads `/proc/<pid>/stat` first, as the comparison does, and says n/a (and fails) instead of 0
  ([CPU](#the-numbers)).
- The first press after the cold start: changed 2026-10-26 ([First press after a start](#first-press-after-a-start)),
  not measured on the device again yet.

Runs: `compare-results/full-20261026` (everything but CPU and play, 95 min), `cpu-20261026` (CPU and play again,
42 min, after the CPU fix), merged in `numbers-20261026`. The side-by-side video of the same steps, with these numbers:
`demo/out/pocket-ui-vs-rocknix-speed.mp4` ([COMPARE-ROCKNIX.md](COMPARE-ROCKNIX.md#the-video)).

## In CI

There is no CI yet. The steps a CI job needs, all non-interactive:

```sh
python3 scripts/latency.py selftest                    # or scripts/test-latency.sh: no device needed
scripts/vdev.sh deploy                                 # build, start the virtual device, start pocket-ui
scripts/latency.sh --compare                           # exit 1 when slower than docs/latency/vdev.json
scripts/vdev.sh down
```

The baseline has to come from the CI machine itself (run `--save-baseline` there once), since the virtual device's
numbers depend on the machine. `scripts/test-latency.sh --vdev` is a faster check that the harness still finds every
number (one short pass, about 40 s).

## Files

- `src/trace.rs`: the trace (`POCKETUI_TRACE`), `pocket_ui::mark!`, `trace::step`
- `src/app_trace.rs`: pocket-ui's `frame` and `paint` lines; the other trace points are single lines in `src/main.rs`
  and `src/bin/pocket_core.rs`, marked "Speed guard", and the `step` lines (`fast_switch::mark`, `trace::step`)
- `scripts/latency.py`: the measurement (on the target), table, comparison, merge, phases, self-test
- `scripts/latency.sh`: runs it in the virtual device or on a device, compares, re-measures on a failure; on a device restores the normal pocket-ui and runs `scripts/verify-device.sh`
- `scripts/test-latency.sh`: tests of the above
- `scripts/verify-device.sh`, `scripts/test-verify-device.sh`: the check that a device is back in its normal state, and its test
- `docs/latency/vdev.json`, `docs/latency/device.json`: the baselines
- `scripts/rocknix-compare.sh`, `scripts/rocknix-compare-device.sh`, `scripts/rocknix-compare.py`, `scripts/rocknix-compare-plan.json`, `scripts/rocknix-compare-video.py`, `scripts/test-rocknix-compare.sh`: pocket-ui against ROCKNIX, from outside ([COMPARE-ROCKNIX.md](COMPARE-ROCKNIX.md))

### First box in the Shelf and Cabinet looks (2026-10-30)

With the 0.1.4 art, the first X on a game cost ~50 ms more on the RG35XX Plus (H700, 2,000 games): the case front and the
cartridge (~25 ms each) were built on the UI thread, again when the cover arrived. `App::box_warm_tick` now builds them,
and asks for the cover, while the player rests on a game (0.3, 0.7 and 1.2 s after the last input; Shelf and Cabinet only;
nothing else open). First X, medians, before / after: Shelf 166 → 85 ms, Cabinet 144 → 82 ms; Split is unchanged (58-64 ms),
later opens stay at 47 / 40 / 23 ms. Cost: about 9 MB more RSS in the Shelf look after the first rest on a game. Also
`latency.py` no longer takes the window's first paint (800×600 on the H700, then 640×480) as its full size.

The box-front cache shared by Shelf, Cabinet and the idle warmer keeps at most 12 entries, evicting the least recently
used when inserting. This bound does not depend on drawing the Shelf look or advancing its frame counter. An active
box or launch retains its own front and undecorated cartridge-label source until that owner releases it.
