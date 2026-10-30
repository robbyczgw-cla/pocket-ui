# Sleep design (research 2026-10-21)

> **Status** (the ledger is [STATUS.md](STATUS.md)): the first half is research on NextUI and ROCKNIX. "What pocket-ui
> does now" describes what is built (`src/sleep.rs`, `src/battery.rs`). The **deep sleep and its wake are measured on
> a Mini V2** (RTC-alarm wakes, 2026-10-24, see "Waking timeline" and [LATENCY.md](LATENCY.md#waking)); the drain
> of a long deep sleep once (2026-10-25: 27 % in 5.63 h, 4.8 %/h, with a PS1 game open). **Not measured:** an
> overnight sleep, a timed wake with the real power key, light sleep (idle screen-off), the power menu's Power off
> and Restart, low battery on the charger, and L3 (switch off after sleeping, built 2026-10-25, host tests only).
> Built 2026-10-26, host tests and snapshots only: the Sleep & power settings page, the power button's Screen off
> (the game running behind a dark screen) and Hold: Sleep, and the power menu's Restart pocket-ui. The device test
> plan at the end covers those.

[V] = verified in source/PR, [I] = inference. NX = research/NextUI, RX = research/distribution/projects/ROCKNIX.

## Why NextUI feels good on the TrimUI Brick
- Power **tap** = sleep; holding for ≥1 s = power off. The press used to wake is discarded. [V] NX/workspace/all/common/api.c:4171ff
- The emulator does not need to be paused: minarch calls `PWR_update()` inside the libretro input poll, so sleep simply blocks `retro_run`. [V] ma_input.c:21
- Before sleeping: SRAM/RTC flush, an auto save state (slot 9), and `auto_resume.txt`. If the device dies asleep, the next boot resumes the game. [V] ma_menu.c:224-229, nextui.c:1185ff
- Two stages: fake sleep (brightness 0, mute, SIGSTOP on helper daemons, `sync`), then **real `mem` suspend after 30 s** (Allwinner BSP kernel, which just works); power off if suspend fails. [V] api.c:4342, 4407, skeleton/SYSTEM/tg5040/bin/suspend

## ROCKNIX on SM8250 today
- The default is real suspend: no SM8250 `030-suspend_mode` quirk → `suspendmode mem`, logind `HandlePowerKey=suspend`. [V] (SM8550/SM8650 set `off`: "Sleep is currently broken")
- `rocknix-fake-suspend` exits immediately unless suspend mode is `off`. Its 900 s auto-shutdown saves **no** state. [V] rocknix-fake-suspend:369-400
- Its backlight-off only sets DCS brightness 0 on the ch13726a; the panel and DSI stay powered. [I]
- Mainline SM8250 has s2idle only (PSCI-OSI cluster states), so check `/sys/power/mem_sleep` on the device. [I]
- Comparable data (SM8550, PR #3126): real suspend costs **4–5 %/h** because RPMh rails never collapse (Wi-Fi vote, ADSP wakes, TSENS, fan tach IRQs). [V]

## pocket-ui sleep levels
| Level | When | What | Resume | Drain (est.) |
|---|---|---|---|---|
| L1 instant sleep | Power tap | SRAM flush + auto state; RetroArch `PAUSE_TOGGLE` + `systemctl freeze games.slice`; display **off** via sway `output * power off` (DPMS, test on ch13726a; fallback `bl_power`); mute, LEDs off, fan PWM 0, rfkill Wi-Fi/BT; cpu4–7 offline; policy0 + GPU devfreq powersave | < 1 s | ~4–8 %/h [I] |
| L2 s2idle | 30–60 s in L1 | `systemctl suspend` with the existing hooks; fall back to L1 if `suspend_stats/fail` rises or resume is slow | few s | measure |
| L3 auto-off | 1, 3 (default) or 8 h in L2, on battery | resume marker + power off; the next boot goes straight into the game (built 2026-10-25, see below) | boot time | ~0 |

SM8250 sysfs knobs [V] (quirks/platforms/SM8250/010-governors):
- `/sys/devices/system/cpu/cpufreq/policy{0,4,7}`
- `/sys/devices/platform/soc@0/3d00000.gpu/devfreq/3d00000.gpu/governor` (msm devfreq, not kgsl)
- `/sys/class/backlight/*/{brightness,bl_power}`
- the pwm-fan cooling device

To measure on the device: `mem_sleep`, mA in L1 vs. L2 over several hours (voltage-based; the fuel gauge is unreliable per #3126), whether the panel survives DPMS off/on, and `qcom_stats` / `wakeup_sources`.

## What pocket-ui does now (2026-10-21, `src/sleep.rs`, `src/battery.rs`)
Built and first checked on the virtual device with mocks (backlight, battery, a suspend command that blocks 4 s and
then does what ROCKNIX's wake hook did). Deep sleep and its wake were then measured on the Mini V2 (2026-10-24,
"Waking timeline" below); the rest of the test plan below is still open.

| | When | What |
|---|---|---|
| Light sleep | Settings → System → Sleep & power "Screen off after" (Never, 30 s, 1, 2, 5 min; default 2) without input | backlight 0 (sway DPMS too with `POCKETUI_SLEEP_DPMS=1`); pocket-core `sleep`, other emulators SIGSTOP; Play history's stretch of the game ends (`play_log::paused`, [HISTORY.md](HISTORY.md#recording)); any button but Vol± wakes and the game goes on (`play_log::resumed`) |
| Deep sleep | "Sleep after" (Never, 1, 2, 5, 15 min; default 2) into light sleep, or at once on Power (and the quick settings' `Sleep::sleep_now`) | pocket-core `autosave` (auto state + battery save), ARMSX2, Dolphin and Azahar write their Auto state (`standalone::save_resume_state`), `resume.json`, volume noted, `sync`, `systemctl suspend` (ROCKNIX's hooks run) |
| Wake | Power | as soon as the system is back: pocket-core `wake` (or SIGCONT), screen on (its own thread); then, while ROCKNIX's wake hooks run, the volume; after them the volume once more, `wake` again (quantum 256 again) and the backlight again if a hook turned it off ([timeline](#waking-timeline-instant-on-2026-10-24)); behind an open pause menu the game stays paused |
| Power off | `Sleep::power_off` (quick settings), FIFO `poweroff` | game saves and quits, `resume.json`, `systemctl poweroff`; the next pocket-ui start opens that game |
| Screen off, the game on | Power, short press, with "Short press: Screen off" (`Sleep::screen_off_now`) | backlight 0; the game goes on with its sound: pocket-core gets `screen off` (it draws nothing and gives the game no buttons), other emulators keep drawing; no deep sleep by the timers; any button but Vol± wakes it ([below](#screen-off-with-the-game-running-2026-10-26)) |
| Switch off after sleeping | Settings → System → Sleep & power "Switch off after sleeping" (**Off by default** since 2026-10-26, 1, 3, 8 h) in deep sleep, on battery | the RTC alarm wakes it with the screen dark, then the Power off path (`src/auto_off.rs`, [below](#l3-switch-off-after-sleeping-2026-10-25)) |

- Idle, menu and lid suspends, and every RTC/clock-step re-suspend, use `systemctl suspend --check-inhibitors=yes`.
  A logind sleep block leaves the Pocket in light sleep, logs the refusal and uses the usual retry delay. Only the
  first suspend requested by the physical power key keeps the ordinary `systemctl suspend` behaviour. Explicit
  `POCKETUI_SUSPEND_CMD` test commands are left intact (the virtual device has no systemd); the device verifier
  rejects custom command overrides in normal use.
- `no-autosuspend` in either the active data directory or `/storage/.config/pocket-ui` blocks the idle transition
  into deep sleep, even when a test redirects `POCKETUI_DATA`. Manual sleep and low-battery power-off keep working.
- The power key is ours only while a logind `handle-power-key` inhibitor is held (`systemd-inhibit … cat`);
  otherwise logind suspends as before and pocket-ui only repairs volume and quantum afterwards (it notices
  the suspend from CLOCK_BOOTTIME − CLOCK_MONOTONIC).
- A press of the power key in the 1.5 s after a wake is ignored (the wake press itself), and every other button of
  pocket-ui's own screens in the 600 ms after it (`WAKE_QUIET`: a press in a bag must not start the game Home has
  selected). Both count from the moment the screen is told to come on and never hold the picture back. pocket-core
  hides the button that woke it until it is let go and gets every other press at once; SIGSTOPped emulators get it
  from the kernel's queue when they continue.
- RetroArch writes no state before a suspend (no network command for its auto state that leaves the
  slots alone); its game stays in memory.
- A pocket-ui that ends while the screen is dark leaves `/tmp/pocket-ui.brightness`; the next one turns
  the screen back on.
- Battery: a sample a minute into `<data>/battery-history.bin` (48 hours, `src/battery_history.rs`: a checksummed
  file rewritten atomically each minute, about 46 kB, with a `.bak`). A wake takes a sample at once, on a thread of its own
  (`battery_history::sample_soon`: three reads 100 ms apart and a write the screen doesn't wait for), and the sleep is
  a gap in the history, not a line. Settings > Playing > Battery shows the graph (3/6/12/24/48 h with L1/R1, green while
  charging, hatched gaps) and a time-left estimate (`src/battery_estimate.rs`) only after 20 min on battery
  without sleep and a 2-point drop; a run of samples ends at a gap, at plugging in or out, and at a jump of the gauge.
- Battery per game (`src/battery_games.rs`): the sampler also adds each minute on battery to the game running at both
  ends of it (`battery_history::set_game`; same rules: no gap, no jump, not plugged in; minutes in light sleep,
  `set_asleep` from `sleep.rs`, and the stretch to the sample after a wake count for no game; every change into or out
  of light or deep sleep, and a suspend nobody asked for (`sleep_boundary`), starts a new generation, and two samples
  of different generations are never a pair, so a short light sleep between two samples isn't counted as play either,
  review SOL 2026-10-24 #11) in `<data>/battery-games.json`
  (per system, file and emulator: seconds and points, scaled down beyond 4 hours of play so new play moves the rate).
  The pause menu shows the time left at that rate at the right of its title, from the closest measurement that has
  20 minutes and 3 points: this game on this emulator, this game, the other games of its system, every game; else
  "measuring", and "Charging" on the charger. The card says the same from the shelf (no emulator to go by, so the
  game on any emulator first) and the game's own rate (`battery_games::card_line`). The ledger is
  read on the sampler's thread at the start (`battery_history::start` now opens the history there too) with no lock
  held and put in place whole, and the menu and the card ask it in memory: until it is there the card's Battery says
  "Loading…" and Settings → Battery says "Loading…" under its graph (review SOL 2026-10-24 #6), never a file read on
  the UI thread.
- Before pocket-ui exits, `battery_history::stop` waits for the write in progress (up to 2 s) and lets no new one
  start, so an orderly exit leaves no `battery-history.bin.<pid>-<n>.tmp`. Those of killed processes (and every other
  temporary file of `fsutil`'s atomic writer in the data folder, five folders down) go a minute after the next start
  (`fsutil::reap_temp_files_soon`): only names of exactly that shape, only when their process doesn't hold them open
  and is gone or the file is older than 15 minutes; the log says `fsutil: removed N temporary file(s) of interrupted
  writes (...)` (review SOL 2026-10-24 #12).

### Waking timeline (instant on, 2026-10-24)
The goal: the power key on a Pocket in deep sleep, and the game is back (picture moving, input, sound) in under a
second, ideally half of one; the same for the shelf. What happens after the press, in order, whose it is, and what
changed (`src/sleep.rs` `deep_sequence`, `Sleep::resumed`, `Sleep::woke`). The order is from the code; the times on the
Mini V2 come from the measurement in [LATENCY.md](LATENCY.md#waking) and from the line each wake now logs:

```
sleep: wake timeline, ms after the system was back: power key seen by the kernel -412 ms, screen on +3 ms, game on +1 ms, ROCKNIX's wake hooks done +870 ms, volume set +95 ms
```

(made-up numbers; "power key seen by the kernel" is the key event's own time stamp, so the part before it, the PMIC
and the s2idle exit, is the only part no line measures.)

| # | Step | Whose | Before (main 3d23939) | Now |
|---|---|---|---|---|
| 1 | The PMIC sees the key, s2idle ends, the kernel resumes the devices (panel and DSI, GPU, sound, Wi-Fi, SD card) and thaws the processes | kernel | | the same; `echo 1 > /sys/power/pm_debug_messages` makes the kernel log each phase |
| 2 | pocket-ui's deep sleep worker notices the system is back | ours | it napped 200 ms at a time on CLOCK_MONOTONIC, which stands still in suspend: the rest of the nap after the thaw (up to 200 ms). A suspend under a second (power pressed right after the device went down) wasn't seen at all: the screen stayed dark for the 30 s timeout and it counted as a failed suspend | the nap is on CLOCK_BOOTTIME (`boottime_nap`) and ends at the thaw; `/sys/power/suspend_stats/success` sees a short suspend, `fail` ends a failed one at once (`suspend_outcome`) |
| 3 | ROCKNIX's wake hooks (`systemd-sleep` runs `/usr/lib/systemd/system-sleep/*` with `post`: LEDs, `sleep.d/post/003-audio` sets PipeWire's quantum to 1024, …) | system | screen and game waited until `systemd-sleep` was gone (looked for every 100 ms) | they run while the game already plays; only the volume, the quantum and a second look at the backlight wait for them (every 50 ms) |
| 4 | The volume (`pactl` one point off, ROCKNIX's `volume`, `pactl` mute: three to four programs) | ours | before the screen came on, after the hooks | right after step 5 is asked for, and once more after the hooks |
| 5 | The game goes on: pocket-core `wake` (a line into its FIFO; asleep, it waits on its commands and takes the line at once), other emulators SIGCONT | ours | after the backlight | first, so its next frame is under way while the screen comes on |
| 6 | Screen on: the backlight (a DSI command), with `POCKETUI_SLEEP_DPMS` sway's output first | ours | on the UI thread, `swaymsg` for DPMS | on a thread of its own (`System::screen_on_soon`), sway's IPC socket for DPMS; a sleep right after it wins (`screen_gen`) |
| 7 | pocket-core's first frame after the wake; the sound fades in once 40 ms are queued again | ours | (behind 2 to 4) | trace `core-frame wake`, `audio: playing again` |
| 8 | PipeWire's quantum back to 256 for the game | ours | pocket-core's `wake`, after the hooks | a second `wake` after the hooks (to an awake pocket-core that is all `wake` does) |
| 9 | The pads: evdev keeps the open devices across s2idle; a node made again (InputPlumber's virtual pad, a Bluetooth pad) has to be opened | ours | the supervisor looked once a second | inotify on `/dev/input` wakes it at once (`pad::DirWatch`); `pad: lost` / `pad: connected` in the log and the trace say whether a wake re-made a node |
| 10 | The shelf's next frame (the clock and the battery in the corner) | ours | none until the corner's next-minute timer, which runs on the monotonic clock and stood still in the suspend: the time from before the sleep for up to a minute | a frame right after the wake (`Sleep::take_woke`) |
| 11 | The quiet times (power key 1.5 s, other buttons 600 ms) | ours | from the wake | unchanged lengths (reasons at `WAKE_QUIET`), counted from step 6, never in the picture's way; a game's own input isn't affected |
| 12 | Wi-Fi reconnects, Bluetooth comes back | system | nothing waits for them | the same |

Expected gain (to be measured): the screen and the game no longer wait for the hooks (their length on the Mini V2 is
what the log line's `ROCKNIX's wake hooks done` says), the rest of a 200 ms nap (about 100 ms on average), the 100 ms
look for `systemd-sleep` (about 50 ms) and the three to four volume programs (some 50 to 200 ms on the device). From
the thaw to the screen it should be a few milliseconds instead of all of that; the kernel's resume (step 1) stays.

Also new: a pocket-core whose sound stream fails (the sound server restarted, say around a sleep) opens a new one,
for up to 5 s (`audio::reopen_stream`, `audio: new stream (…) after … ms` in the log), instead of playing on without
sound. The watcher for suspends pocket-ui didn't start naps on CLOCK_BOOTTIME too, so their volume repair starts at
the wake instead of up to 2 s later.

Light sleep was immediate already (the waking button arrives on the UI thread, the backlight goes on); now the game
is told first and the backlight is written on the screen thread as well. `latency.sh --wake` measures it as well
(`light.*`).


### L3: switch off after sleeping (2026-10-25)
Deep sleep costs 4.8 %/h on the Mini V2 (s2idle with a PS1 game open: 27 % in 5.63 h, 2 suspends, 0 failed, no stray
wakes, main `7963069`): a full battery is empty after about 20 hours asleep, a weekend in a bag. So after a while in
deep sleep the Pocket saves and powers off, and the next start goes straight back into the game. The choices are Off,
1 h, 3 h and 8 h (Settings → System → Sleep & power, `sleep.json` `off_after`). **Off is the default since 2026-10-26**
(owner's decision: he picked up a Pocket he expected asleep and found it hard off; ROCKNIX and NextUI just sleep). With
Off no plan exists and no wake alarm is ever armed; a `sleep.json` that says 3 h keeps saying it. That includes every file written since the Sleep & power page (or any settings save) existed: it holds `off_after: 3` even when only another row was changed, so those Pockets go on switching off after 3 h until the row is set to Off. The other way round, a very old file with only `light_after` (no `off_after`) changes from 3 h to Off. Code cannot tell a chosen 3 h from an incidental one; a one-time migration would be needed for that and was not built. Of the choices 3 h was the default before: about 14 % of the battery,
where 1 h would cost a boot every time a Pocket is put down for an evening and 8 h would let a night take 40 %.

`src/auto_off.rs` has the parts, `sleep.rs` `deep_sequence` runs them on the deep sleep's worker thread:

| Step | What |
|---|---|
| The time | counts from the first suspend of a dark spell, on CLOCK_BOOTTIME (`auto_off::Plan`, kept by `Sleep` across failed suspends; a wake ends it) |
| Before each suspend | the RTC alarm for what is left (`echo 0`, then `echo +<s>` into `/sys/class/rtc/rtc0/wakealarm`, the path `scripts/latency.sh --wake` uses; `POCKETUI_RTC_ALARM` replaces it) |
| Back before the time | a normal wake: the only things added before screen and game are two clock reads (`Plan::due`, and the wall clock's offset to CLOCK_BOOTTIME, [below](#l3-switch-off-after-sleeping-2026-10-25)); the alarm is cleared right after `resumed` (screen and game on), so it can't switch off a Pocket that is awake or dark later |
| Back at the time (the alarm) | nothing comes on. After ROCKNIX's wake hooks and 300 ms for a key pressed at that moment (`settle`), `auto_off::decide`: the power key pressed during the suspend → a normal wake (someone is there); on the charger, or the battery can't be read → suspend again, the alarm a full period on, so a Pocket unplugged later still switches off; on battery → `Msg::SwitchOff` |
| Switching off | `shut_down_for(PowerOff, Why::Slept(h))`, the Power off path with the screen dark (no overlay): `resume.json`, the game quits (pocket-core `quit`, RetroArch's `QUIT`, the standalones' state and SIGTERM; the sink muted meanwhile so a game going on for a moment isn't heard, its mute put back after), `<data>/switched-off.json` (hours and the brightness before the dark), `sync`, `systemctl poweroff`. If that fails: back to light sleep, dark, and no second try in that dark spell |
| The clock set during the suspend | `deep_sequence` notes the wall clock against CLOCK_BOOTTIME (`auto_off::wall_offset`) before each suspend. A wake before the time with that offset moved by more than 5 s means the clock was set or stepped (by hand, NTP) after the alarm was armed and the RTC alarm may have gone off early: not a wake of a player (before 2026-10-26 the screen came on in the bag and the next sleep began a full period). After `settle`: a power key pressed → a normal wake; else asleep again, the screen never on, the alarm for what is left of the **same** period |
| Charger shows up late | the first reading at the alarm's wake that says battery is made twice more, 2 s apart, before the switch-off (`SWITCH_OFF_CONFIRMS`): ROCKNIX's wake hooks can outlast `wait_for_wake_hooks`' 15 s cap, and a charger's state arrives seconds after the wake. A charger, an unreadable battery or a power key press in that window ends it as before (asleep again for a full period, or a normal wake) |
| Light sleep after failed suspends | a Pocket whose suspends keep failing is dark in light sleep (`Timers::deep_wait`); when the time is up there and the battery, read fresh at that moment (`low_battery::read`, not the guard's reading of up to 30 s before, 2026-10-26), says battery, it switches off the same way; on the charger, or with an unreadable battery, it stays dark and looks again a minute on |
| The next start | `on_start` rule 1: the resume note wins over On start = Home. `Sleep::new` reads `switched-off.json` (one small file, removed): the backlight back to its level if it is still 0, and `app_auto_off` says "Switched off after 3 h asleep to save the battery." once: over a pocket-core game (its new `note` command, six seconds at the bottom), on the shelf when another emulator's game ends, or on the shelf at once when no game was resumed |

The low-battery guard is unchanged: at 2 % it saves and switches off with the screen on ("Battery empty. Saving…"),
whatever L3 says; a `SwitchOff` after it does nothing (the state is `PoweringOff`). The wake-time measurements are
untouched: with the harness's suspend command (its own RTC alarm) every wake comes before the switch-off time and is a
normal wake. Not covered: an RTC alarm the kernel refuses (the log says `couldn't set the RTC alarm`; the Pocket then
just stays asleep, as before L3). Tests: `sleep.rs` (`FakeDeep` with a fake clock, RTC and power supply; `Sleep` with a
power-off command that writes a file) and `auto_off.rs`.

### Power off, restart and the RTC alarm (2026-10-26, `fix/power-off-rtc`)
Incident 2026-10-26: "Power off" from the power menu rebooted the Mini V2 with no charger attached. An RTC wake alarm
still armed (or already in the past) when the system goes down would explain it, and `shut_down_for` never looked at
the alarm; only the auto-off's own switch-off cleared it first. Now **every** way to power off or restart runs one
verified disarm right before its command (`auto_off::disarm_before_power_off`, `Rtc::disarm_for_shutdown`):

| Step | What |
|---|---|
| Where | `Sleep::shut_down_for` (the power menu's Power off, Restart and Restart to boot menu, quick settings, the FIFO's `poweroff`, the low-battery guard, auto-off's switch-off from deep and from light sleep), the safe-mode dialog's Power off (`app_rescue.rs`) and the quick settings' fallback when the app has no shutdown handler (`RealHw::power`). Not Restart pocket-ui: the system stays up and so does the alarm for the next sleep |
| What | write `0` to `wakealarm`, read it back (empty or `0` = no alarm), read `alarm_IRQ` and `alrm_pending` from `/proc/driver/rtc` when the file exists; up to 3 writes (20 ms apart) until the alarm file is empty and `alarm_IRQ` is not `yes` (`alrm_pending` is only logged: the enable bit is what powers the PMIC on) |
| Never blocks | a failing write or read is in the log line (`RTC alarm disarm NOT verified (before=… write=failed after=unreadable …); powering off anyway`) and the power off goes on. No RTC at all (the virtual device) reads as `no-rtc` and counts as verified: nothing can be armed |
| Serialised with the suspend worker | `Rtc` clones share one gate (a mutex held while the alarm is written). The gate is shut at the very start of `shut_down_for` (`Rtc::close`, before the store wait and the game's quit), the disarm follows just before the command. From then `Rtc::set(Some(_))` is refused, a queued deep-sleep worker does not start, and a sequence already going round (asleep again on the charger, a clock set) ends at the top of its next round without arming or suspending (`DeepSteps::shutting_down`); `set(None)` still clears. The quick settings' fallback (`RealHw::power`) does all this for Power off and Restart only, never for Sleep (`systemctl suspend`: the process lives on and the gate would stay shut for good). A power off that fails (`false`, the command not found) opens the gate again, so the next sleep can set its alarm |
| Logged | `shutdown: <action>: RTC alarm disarm verified (before=<epoch\|none> write=ok after=none alarm_IRQ=no alrm_pending=no tries=1)` on stdout/stderr and in `shutdown.log` ([below](#shutdown-log-logs-that-survive-a-restart)) |
| At start | `Sleep::new` starts a thread (`rtc-check`, nothing waits for it) that reads the alarm and `/proc/driver/rtc` and, when an alarm is armed or `alarm_IRQ` is `yes`, clears and verifies it the same way (`auto_off::check_at_start`); the gate stays open. One line either way, `start: RTC alarm disarm verified (before=… rtc_now=… tries=0)` (`tries=0`: nothing to clear), in the log and in `shutdown.log`. Why it is always right to clear: the plan lives in memory and an alarm is only armed inside one run's deep sleep, so at a start (a crash, Restart pocket-ui, a deploy, a new boot) no switch-off can be under way; an alarm found then has nobody to answer it and the next power off would meet it. Every failed clear on the start and shutdown paths is logged, each time; the deep sleep's own clear after a wake logs a failure each time too when the Pocket has an RTC |
| Test hooks | `POCKETUI_RTC_ALARM` (the `wakealarm` file), `POCKETUI_RTC_PROC` (a stand-in for `/proc/driver/rtc`; without it and with `POCKETUI_RTC_ALARM` set the proc file is not read), `POCKETUI_POWEROFF_CMD`, `POCKETUI_REBOOT_CMD`, `POCKETUI_BOOTLOADER_CMD` |

What this cannot show: whether the PMIC of the Mini V2 treats a power off differently with an armed alarm. A
verified disarm takes the alarm out of the question; if "Power off" still reboots, `shutdown.log` of the boot before
says which command ran, how it ended and what the clock alarm and the kernel said just before.

#### Shutdown log: logs that survive a restart
`<data>/logs/shutdown.log` (written by pocket-ui itself, `src/shutdown_log.rs`; the other logs are the logger's): for
every power off and restart it records, with the uptime and the clock, `<action> asked (<why>)`, a snapshot (the
last 40 lines of pocket-ui's log, still only in RAM, and the last 25 kernel records), the RTC readback, `running
<command>` and `<command> ended: exit 0` (or the exit code, signal, or why it did not start). Each entry is written
and fsynced before the next step, so a power cut right after keeps what was there. It is one of the logger's
streams: it moves into `boot-<n>-…/` with the boot's other files, the newest three boots stay, and Report a problem
sends the current boot's and the two starts before it, all their logs (`logs/shutdown.log`, `logs/previous-boot/…`, `logs/boot-before-that/…`). Size: 128 KB
per boot (the file and its `.old` together), at most 384 KB for the three boots. The other logs
(`pocket-ui.log`, `kernel.log`, `sensors.log`, three boots, 5 MB per file) were already kept on the card by the logger
and are unchanged.

### Failed suspends and the low-battery guard (2026-10-23, review B12 / SOL-05, `src/low_battery.rs`)
- A suspend that did not happen (`slept` false) used to be tried once more and after the second failure idle stayed in
  light sleep for good: dark screen, CPU and radios on until the cell was empty. Now each failure doubles the wait
  before the next try (2, 4, 8, 16, then every 30 min, `Timers::deep_wait`); a new dark period starts at 2 min again.
- `low_battery::Guard`, fed every 30 s on its own thread (`battery-guard`; the first sysfs read is the thread's too):
  at ≤ 5 % on battery `Sleep::take_battery_warning` gives the app one warning (a notice on the shelf, the OSD level
  pill over a pocket-core game; a dark screen gets none); at ≤ 2 % in two readings in a row (one stray reading of
  the gauge is no reason) `Sleep` runs the power-off path (`shut_down_for`: "Battery empty. Saving…", the game saves
  and quits, `resume.json`, `systemctl poweroff`), from Awake or Light, or right after a suspend under way returns.
- External power stops all of it: battery `status` Charging, Full or Not charging, or any non-battery supply in
  `/sys/class/power_supply` with `online` = 1. So `no-autosuspend` on the charger overnight keeps the device up.
- Not covered: a charger too weak to keep up while plugged (it counts as external power).

### Volume at 0 after waking: cause
ROCKNIX's `/usr/lib/systemd/system-sleep/sleep post` does:
```
DEVICE_VOLUME=$(get_setting "audio.volume")
log $0 "Restoring volume to ${DEVICE_VOLUME}%."
amixer -c 0 -M set "${DEVICE_AUDIO_MIXER}" ${DEVICE_VOLUME}%
```
- `DEVICE_AUDIO_MIXER` is set by no SM8250 quirk (only the RG552 sets it, to `DAC`), so the amixer call
  gets an empty control name and does nothing on the Mini V2.
- The volume that counts is PipeWire's sink volume. Only ROCKNIX's `/usr/bin/volume` (the Vol keys, via
  `input_sense`) sets it, together with `audio.volume` in system.cfg. Nothing in the wake path writes the
  sink, and pressing Vol+ (`volume up` → `pactl set-sink-volume`) made it audible again.
- The log said 0 %, so `audio.volume` was 0 at that moment, which the Vol keys would only write when turned
  all the way down. Two causes fit and are told apart by the test below: (a) the codec comes back from s2idle
  with its gain reset while PipeWire still believes its old value, and `audio.volume` was 0 for an
  unrelated reason; (b) something wrote `audio.volume=0`/the sink to 0 (EmulationStation saves `audio.volume`
  itself).
- Workaround, whichever it is: before suspending pocket-ui reads the sink (`pactl`) and stores it through
  `volume <n>` (so the hook logs the real value); after the hooks finished it sets the sink one point off
  and back (`volume <n>`), so PipeWire writes the codec again, and restores mute. For suspends it didn't
  start it uses the volume read every 15 s.

## Power menu: short and long press (2026-10-22, `src/powerkey.rs`, `src/power_menu.rs`)
- **Short press** (released before `powerkey::LONG_PRESS`, 800 ms): sleep by default, or Screen off (below). The
  decision can only be made when the key goes up, so it acts at the release (a tap lasts 80 to 150 ms) instead of at
  the press. A short press never opens a menu.
- **Long press**: the menu opens while the key is still held (the event loop's timer, `Sleep::tick`); the
  release after that does nothing. With "Hold: Sleep" the long press sleeps instead, at the release
  (`powerkey::Step::LongEnd`), so no key event arrives while the system suspends. The press that woke the device (1.5 s
  guard) and its release are ignored as before. A second power press with the menu open closes it
  (`powerkey::Gate::menu_open`), as does B.
- The menu (`Screen::Power` in `quick.rs`, painted by `quick_paint.rs`) lists Sleep, Restart, Power off, Restart
  pocket-ui and, only where `Hw::boot_menu_supported()`, Restart to boot menu. Each entry has a drawn mark (moon, round
  arrow, power symbol, three spines, three lines: `power_menu::Mark`) and a line under it (`Choice::hint`: in a game
  "Saves the game, …"; Restart on a RAM boot "Started from a computer: this goes back to Android."). At the top right
  the battery: the status corner's level and `battery::headline` of the Battery page's estimate, asked once when the
  menu opens, from memory (`power_menu::battery_now`); "No battery found" without a level, "Loading…" before the
  history is read. Restart and Power off go through `Sleep::shut_down`: game saved and quit, resume note,
  `systemctl reboot|poweroff` (`POCKETUI_REBOOT_CMD`, `POCKETUI_POWEROFF_CMD`, `POCKETUI_BOOTLOADER_CMD` replace them
  for tests). Power off and system restart verify the RTC alarm disarm once on the worker immediately before the
  power command; Restart pocket-ui leaves it alone. Fake power
  backends (`POCKETUI_MOCK`, and unit tests) never flush the build host's filesystems. The real backend still calls
  `sync` before the command. The next start opens the game again.
- **Restart pocket-ui** (`PowerAction::RestartUi`, 2026-10-26): the same save path (store install finished, game saved
  and quit, `resume.json`, "Restarting pocket-ui…"), then instead of a `systemctl` command the worker sends
  `Msg::Restart`; `Sleep::take_restart` tells the app, which ends with exit code 0 after its usual end (battery
  history, card writes, play log, fan handed back). The launcher (`deploy/run-pocket-ui.sh`) answers exit 0 with a new
  start inside the same essway unit after its clean-up, as for the rescue page's Restart; the logind inhibitor goes
  with the old process and the new one takes it again. The next start opens the game from `resume.json` (`on_start`
  rule 1). Nothing of the system restarts, so it works on the virtual device too. `PowerAction::RestartUi` has no
  `argv`, and `Hw::power` refuses it.
- **Very long press**: the PMIC's own hold-to-reset (several seconds) is untouched (see below); pocket-ui never
  reacts to it.
- In a game the menu opens the pause menu first (game paused, same code as HOME) and puts the power menu on top.
  B closes both and the game continues; Sleep resumes the game, then sleeps it, so waking continues the game.
- virtual device: `POCKETUI_POWER_KEY=1` in `/storage/pocket-ui/dev.env` (no logind there), then
  `vdev.sh send powerdown`, a pause, `vdev.sh send powerup` is a long press; `send power` is a short one;
  `send powermenu` opens the menu directly. `POCKETUI_NO_BOOT_MENU=1` shows the menu of a device without the entry.

## Sleep & power settings (2026-10-26)
`src/sleep_settings.rs` (the choices, `sleep.json`, the page's keys), `src/app_sleep_power.rs` (painting). One page, Settings → System → Sleep & power, in the manner of NextUI's System settings (Screen timeout, Suspend
timeout): **Screen off after** (Never, 30 s, 1, 2, 5 min), **Sleep after** (Never, 1, 2, 5, 15 min), **Switch off
after sleeping** (Off, 1, 3, 8 h; moved from Settings → Playing → Battery, which keeps the graph and the estimates),
and under the heading Power button **Short press** (Sleep, Screen off) and **Hold** (Power menu, Sleep). Kept in
`sleep.json` with the keys `screen_off_secs`, `sleep_after_min` (0 = never), `off_after`, `short_press`, `hold`;
`Sleep::set_config` applies a change at once.

- **Defaults.** Screen off after 2 min: NextUI turns its screen off after 60 s and a phone after 30 s to a minute, but
  in pocket-ui the game pauses when the screen goes off, and a long cutscene, a text or a replay can go a minute
  without a button; 2 min keeps those and saves a minute against the 3 min of before. Sleep after 2 min, as it was
  fixed before (NextUI: 30 s). Switch off after sleeping Off (3 h until 2026-10-26: see [L3](#l3-switch-off-after-sleeping-2026-10-25)). Short press Sleep, Hold Power menu.
- **Before the page.** `sleep.json` had `light_after` in minutes (Off, 1, 3, 5) and `off_after`. A file without
  `screen_off_secs` takes `light_after` as it was (3 min stays 3 min and shows "3 min" until changed; Left and Right go
  to 2 and 5 min); no file at all takes the new defaults. `light_after` is still written, rounded up to minutes, so an
  older pocket-ui (the rescue page's previous version) reads the same time. Words of a newer version
  (`short_press`, `hold`) fall back to the defaults without losing the rest of the file.
- **Sleep after: Never** stays dark in light sleep until a button. A failed suspend that the power button asked for is
  still tried again (from 2 min, doubling, `Timers::deep_wait`), and the low-battery guard still saves and switches
  off at 2 %.

### What the defaults match
| | Short press | Hold | Long hold | Idle |
|---|---|---|---|---|
| pocket-ui (defaults) | sleep (game saved) | power menu at 800 ms (Sleep, Restart, Power off, Restart pocket-ui) | PMIC reset, untouched | screen off 2 min, sleep 2 min later |
| Android on the Retroid Pocket Mini [I] | screen off and sleep | power menu (Power off, Restart, …) | forced reset by the PMIC | screen timeout, Android's own setting |
| NextUI (`api.c` `PWR_update`, `config.h`) [V] | sleep | power off at 1 s, no menu | — | screen timeout 60 s, then suspend 30 s later |
| ROCKNIX (logind) [V] | `HandlePowerKey=suspend` | `HandlePowerKeyLongPress=ignore` | PMIC reset | EmulationStation's screensaver |

The only difference to Android is that pocket-ui's power menu also offers Restart pocket-ui; NextUI powers off on a
hold where pocket-ui and Android ask first. A short press never opens a menu, and the press that wakes the device
does nothing else (`WAKE_GUARD`, `WAKE_QUIET`).

## Idle slideshow (2026-10-30, `src/slideshow.rs`, `src/app_slideshow.rs`)
A quiet shelf is not left static: after a minute without input the games' pictures play, then the screen goes off as
it always did. Settings → Look & feel → Cozy extras → **Idle slideshow**. **Off in every look** until the player switches it on (not
everyone wants it; `idle_slideshow: true` under `extras` in `settings.json`, nothing is written while it is off). Read with
`cozy_extras::on(Extra::IdleSlideshow)`.

- **When.** No button or touch for `slideshow::start_after`: a minute, or half the screen-off time when that is shorter
  (Screen off after 30 s: the show starts at 15 s; a screen-off time under 16 s: no show; Never: a minute), and the shelf
  is alone: Home, no game, page, card, menu, dialog, notice or toast, the device awake, not on a TV. The show reads
  `Sleep::quiet_for` (the time since the last input of sleep's own `last_input`) and nothing else of sleep: **it sets no
  timer and holds no wake lock**, so the screen goes off at the same moment as without it, the light sleep ends the show
  (`slideshow_tick` finds the device no longer awake), and Sleep after / Switch off count as before.
- **What.** On a thread: of a shuffle of the library, one picture per game (box art, in-game screenshot or title
  screen, whichever exists, in a varying order), plus the player's own screenshots (`screenshots/<system>/`, HOME + A)
  and save-state pictures (`savestates/<system>/*.pstate*.png`: slots and the auto state, not "Before load"),
  one slide in three, 36 slides at most. Files are judged by their PNG headers (at least 64 px a side). With Game
  pictures on "Box art only" the screenshots and title screens that are on the card are left out. **No game with a real
  picture: no show**, the shelf stays (looked at again after the next input). Each picture is read and scaled for the
  screen on a second thread (`slideshow::prepare`): a picture close to the screen's shape fills it, any other (a portrait
  box) is shown whole on a dark backdrop of its own colour; three pictures at most are in memory.
- **How it looks.** Every 10 s the next picture fades in over 1.6 s (the first one over the shelf itself); each moves
  while it is there: a slow zoom (1.0 to 1.12, in and out alternately) with a drift in one of eight directions
  (`slideshow::view`; the pan never takes a filling picture's edge into the screen). The title and the system are small
  at the bottom (a player's own picture adds "Your screenshot" / "From your save"), the clock and the battery in the
  corner. Motion Reduced or Off: no zoom or pan, only the cross-fades.
- **Cheap.** 15 frames a second (`slideshow::FRAME`) while something moves or fades, none between slides when Motion is
  Reduced or Off; `App::animating` is false while it runs, so the shelf's own animations do not set the pace. The paint
  is a bilinear window from the prepared picture (`slideshow::paint`: about 3 lerps a pixel, rows and columns worked
  out once); nothing is decoded on the UI thread, nothing runs in the shelf behind it, no fan is asked for.
- **Any input ends it, and does nothing else** (`slideshow_input`): a button press (not a release, not the volume keys)
  or a touch ends the show and is swallowed; the shelf is exactly as it was (the show never changed a thing of it), key
  repeat is cancelled, the rest of a touch (move, lift) is swallowed too. Sleep has seen the input first (it is the one
  that restarts the screen-off time). Analog L2/R2 pulls become button edges before sleep and the slideshow see them;
  further travel while held cannot reach the shelf as a second press, and their release cancels key repeat.
- **Tests.** Unit: `slideshow` (timing, the camera, painting, the picture lists), `tests/user_journeys.rs` (a real
  run with `--test-driver`: idle, the show with a real title, a button press or analog R2 pull back to the same shelf
  with nothing opened, analog R2 wake without moving the shelf, no pictures: no show, off: no show, a touch). `--snapshot slideshow[:<ms>|:fade]` renders a frame;
  `POCKETUI_SLIDESHOW_AFTER_MS` (with `POCKETUI_MOCK`) shortens the minute for tests, `POCKETUI_SLIDESHOW_STATES` moves the
  save-state folder.

## Screen off with the game running (2026-10-26)
"Short press: Screen off" (`Sleep::screen_off_now`) is for a game whose music you want to keep hearing.
- With a game being played (no pause menu open): the backlight goes to 0, the game is not paused, its sound goes on.
  pocket-core gets `screen off`: it uploads and draws no frame (the frame's time is `limit`'s, as without vsync), and
  gives the game no buttons, no sticks and no shortcuts (a press in a pocket does nothing, fast-forward and rewind are
  off) until `wake`, which also hides the buttons still held until they are let go. RetroArch and the standalone
  emulators are not told: they keep drawing to the dark screen and get every press (the docs say to wake them with
  Power). The core still runs every frame (a 3D core still renders into its own texture).
- `Sleep::dark_game` is true meanwhile: the timers never put it to sleep (no deep sleep, so the music goes on); the
  battery per game doesn't count the dark minutes (`battery_history::set_asleep`), Play history goes on; the fan keeps
  the game's choice (`app_perf_fan`: not "asleep"). Any button but Vol± wakes it (`wake_light`); a power press too.
- On the shelf, in the menus, or with the pause menu open, Screen off is plain light sleep (the game stays paused,
  Sleep after counts). Light sleep from idle (Screen off after) always pauses the game, whatever the button does.
- Anything that suspends from there (`sleep_now`, `deep`) pauses the game first (`pause_dark_game`); a power off or
  the low-battery guard quits it from where it is. A game that ends by itself meanwhile leaves plain light sleep:
  Sleep after counts from then (`game_ended`).

### `reboot bootloader` on the Mini V2: not supported (read-only checks on the device, 2026-10-22, kernel 7.2.0)
- The PON block is `/soc@0/spmi@c440000/pmic@0/pon@800` (`qcom,pm8998-pon`, with `pwrkey` and `resin`). Its
  device tree node has **no `mode-*` property**: nothing under `/proc/device-tree` is named `*mode*`, and the
  only `mode-switch` is the USB-C mux. (`pmic@2` and `pmic@4` have disabled `qcom,pm8916-pon` nodes.)
- The kernel has `CONFIG_REBOOT_MODE=y`, `CONFIG_SYSCON_REBOOT_MODE=y`, `CONFIG_POWER_RESET_QCOM_PON=y` and the
  `qcom-pon` driver is bound, so `/sys/class/reboot-mode/qcom-pon/` exists, but its `reboot_modes` is empty.
  `reboot(RESTART2, "bootloader")` therefore matches nothing, no reason is written to the PMIC, and the device
  restarts into whatever the boot mode is (Loader: u-boot, GRUB, ROCKNIX again). pocket-ui hides the entry.
- systemd 255's `systemctl reboot --reboot-argument=bootloader` passes the argument to the kernel, and
  `/sbin/reboot` is a link to systemctl; ROCKNIX has no script that handles a reboot parameter.
- To get the entry: add `mode-bootloader = <N>;` to the `pon` node of the Mini V2 device tree that GRUB loads
  (`boot/pocket/miniv2-noufs.dtb`, made by `scripts/make-test-bootimg.py`). N has to be the reason code Retroid's
  ABL reads from the PMIC (on other Qualcomm boards `0x2`) and whether the ABL honours it from Loader mode is
  **not verified**: it needs a reboot of the device. pocket-ui offers the entry as soon as the kernel lists it
  in `reboot_modes`; nothing else has to change.
- Nothing reacts to a long press by itself: logind has `HandlePowerKey=suspend` and `HandlePowerKeyLongPress=ignore`
  (read from the running logind), and pocket-ui holds its `handle-power-key` inhibitor (it is in
  `systemd-inhibit --list`), which blocks logind's handling of the key altogether. ROCKNIX's `input_sense` runs
  `rocknix-fake-suspend power` on every press, which exits at once while `system.suspendmode` is `mem` (it is).
  The PMIC's own hold-to-reset (several seconds) is untouched; 800 ms is far below it.

## Lid (the Flip 2)

> **Status:** virtual device only ([STATUS.md](STATUS.md)): host tests, and a lid closed in the virtual Flip 2
> (without logind there); the Flip 2 is experimental and nobody on the project has one ([DEVICES.md](DEVICES.md)).

The Retroid Pocket Flip 2 has a hall sensor in its hinge: a `gpio-keys` input device that reports `SW_LID`, and is a
wakeup source (ROCKNIX's `sm8250-retroidpocket-flip2.dts`). ROCKNIX's logind has `HandleLidSwitch=suspend`, which
would suspend without saving the game, as the power key would.

- **The inhibitor.** On a device with a lid (`device::Device::has_lid`) pocket-ui's one `systemd-inhibit` holds
  `handle-power-key:handle-lid-switch`; on every other device it holds `handle-power-key` alone, as before. While it is
  held, logind leaves both to pocket-ui; when pocket-ui ends, the pipe closes and logind has them again.
- **Reading it.** The pad reader opens every device with `SW_LID` (`pad::linux::is_wanted`) and passes the switch on
  as `PadEvent::Lid { closed }` (`pad::linux::switch`); nothing but sleep acts on it.
- **What it does** (`Sleep::on_lid`), only while the inhibitor is held: **closed** while awake is the power button's
  short press as Settings › System › Sleep & power has it (Sleep by default: the game saves and the Pocket
  suspends; Screen off: dark, the game goes on); closed while the screen is already dark goes on into deep sleep when
  the short press is Sleep; **opened** wakes a dark screen; opened from deep sleep, the lid's own wake brings the
  system back and the usual wake path turns the screen on. A lid event never reaches the UI. Without the inhibitor
  (logind's), pocket-ui does nothing with it.
- **Not handled:** the power key pressed while the lid is closed wakes the Pocket with the lid shut (Screen off after
  and Sleep after send it back to sleep, 2 + 2 minutes by default).

Tests (`sleep::tests`): closing sleeps by default and turns only the screen off when set so, a second close changes
nothing, opening wakes; a dark screen goes to deep sleep on closing; right after waking a close still sleeps; without
the inhibitor nothing happens; the Mini V2's inhibitor stays `handle-power-key`. `pad::lid_tests`: the evdev stream
(`SW_LID` 1, a report, a headphone switch, `SW_LID` 0) reads as closed and opened.

## Device test plan (Mini V2, 2026-10-22)
Nobody may suspend the device without a wake timer while unattended. ROCKNIX has no `rtcwake`; use the RTC
alarm directly. Never `poweroff`/`reboot` (RAM boot: it comes back as Android).
```sh
# 0. Facts (read only)
cat /sys/power/mem_sleep /sys/class/rtc/rtc0/wakealarm; grep -i alarm /proc/driver/rtc
grep -n audio.volume /storage/.config/system/configs/system.cfg; pactl get-sink-volume @DEFAULT_SINK@
systemd-inhibit --list; ls /sys/class/backlight/
# 1. Does the RTC alarm wake from s2idle? (20 s; the device must come back by itself)
echo 0 >/sys/class/rtc/rtc0/wakealarm; echo +20 >/sys/class/rtc/rtc0/wakealarm; systemctl suspend
#    after it is back: journalctl -b | grep -i -E "suspend|sleep" | tail; cat /var/log/sleep.log
# 2. The volume bug, before pocket-ui's fix: note sink + audio.volume, suspend 20 s as in 1., compare
#    (pactl get-sink-volume, amixer -c 0 contents | grep -i -A2 vol, audio.volume). Silent with the sink
#    unchanged = (a); sink or audio.volume 0 = (b).
```
3. Deploy (`scripts/dev.sh deploy`) with `/storage/pocket-ui/dev.env`, so idle never suspends and every
   suspend has a wake timer:
   ```
   POCKETUI_SUSPEND_CMD="echo 0 >/sys/class/rtc/rtc0/wakealarm; echo +30 >/sys/class/rtc/rtc0/wakealarm; systemctl suspend"
   POCKETUI_POWEROFF_CMD="echo poweroff-test >>/tmp/sleep-test.log"
   POCKETUI_SLEEP_MINUTE_SECS=10
   ```
   (Remove the file and redeploy at the end.)
4. In the log: no `sleep:` errors; `systemd-inhibit --list` shows pocket-ui's handle-power-key lock.
5. Light sleep: shelf idle 30 s → screen dark (backlight 0), any button → on. In a pocket-core game
   (GBA): keep pressing for 40 s → no sleep; idle → dark, `asleep`; A → `awake`, game continues, A not
   seen by the game.
6. Deep sleep on the shelf and in a GBA game: idle 30 s + 20 s → `state saved`, `suspending`, wakes by the
   alarm after 30 s → `woke after 30 s`, volume as before (listen), `pw-metadata -n settings 0
   clock.force-quantum` = 256 in the game, 1024 on the shelf. `/var/log/sleep.log` and the journal say
   "Restoring volume to <real value>%".
7. Power key (someone at the device): tap → suspends at once; tap → on within ~1 s (since instant on: the screen
   should be on well under that; the log's `sleep: wake timeline` line says where the time went), game continues; a
   second tap right after waking does nothing. Same with the pause menu open: wakes into the menu. The numbers:
   `scripts/latency.sh --wake` ([LATENCY.md](LATENCY.md#waking)).
8. Panel: try `POCKETUI_SLEEP_DPMS=1` (sway output power off) for light sleep; check the ch13726a comes back.
9. Drain: battery page after 30+ min of play shows an estimate; compare deep-sleep drain over 1 h (start/end
   capacity in the graph) with the 4–5 %/h of PR #3126.
10. Power off with the test command (FIFO `poweroff`): `Saving…`, game quits, `resume.json` written;
    restart pocket-ui → the game opens and resumes. A real power off only when someone can fastboot it again.
11. Switch off after sleeping (L3), first without a real power off: `POCKETUI_POWEROFF_CMD="echo poweroff-test >>/tmp/sleep-test.log"`
    and `POCKETUI_SLEEP_MINUTE_SECS=10` (1 h = 10 min) in dev.env, no `POCKETUI_SUSPEND_CMD`. Settings > System >
    Sleep & power: Switch off after sleeping 1 h. In a pocket-core game, unplugged: Power (tap). The log:
    `sleep: RTC alarm in 600 s`, `suspending`; after 10 min `the switch-off time is up`, `1 h asleep on battery;
    saving the game and switching off`, `PowerOff (echo poweroff-test …)`; the screen never on (look at it), the game
    process gone, `resume.json` and `switched-off.json` in the data folder; restart pocket-ui → the game opens with
    "Switched off after 1 h asleep to save the battery." over it. Again on the charger: after 10 min `on the charger …
    asleep again`, still dark; a power tap wakes it normally. Again with a tap after 5 min: a normal wake,
    `cat /sys/class/rtc/rtc0/wakealarm` empty afterwards. Then once for real (someone at the device to fastboot it):
    the default settings, 1 h, unplugged, Power; after an hour it is off, and the next ROCKNIX boot opens the game.
12. Sleep & power (2026-10-26). Settings → System → Sleep & power on the device: the five rows, the explanation
    follows the selection; an existing `sleep.json` keeps its time (3 min shows "3 min"). Screen off after 30 s on the
    shelf: dark after 30 s; Sleep after 1 min: `suspending` a minute later (with an RTC alarm set as in 1.).
13. Short press: Screen off, in a pocket-core game with music (e.g. a GBA game with a soundtrack): Power (tap) → dark,
    the music goes on, `pocket-core: screen off, playing on` in the log; presses of A/B/D-pad do nothing in the game
    while dark; a press of B turns the screen on and does not reach the game (`screen on`); after 5 min dark the
    device has not suspended. Battery drain while dark with the music (the graph) against light sleep. Fan stays the
    game's. Again in a RetroArch game: dark, the game goes on; Power wakes it.
14. Hold: Sleep: hold Power ~1 s, let go → sleeps at the release (no menu); the power menu is still in the quick
    settings. Back to the defaults afterwards (Sleep, Power menu).
15. Power menu: hold Power on the shelf and in a game; the battery at the top right ("NN %" over "About … left", or
    "Not enough data yet" early after a start), the marks and lines under the entries (in a game "Saves the game…").
    **Restart pocket-ui** in a pocket-core game: "Saving…", "Restarting pocket-ui…", the shelf/game back within a few
    seconds, `launcher:` shows a normal restart (no "crashed"), exactly one pocket-ui in `essway.service` afterwards
    (`scripts/verify-device.sh`), and the game opens where it was (`resume.json`).


## SM8550: screen off instead of suspend

Pocket 6 and Nova are experimental (host tests only). ROCKNIX disables real suspend on SM8550;
`Device::screen_off_only`, normalised settings, the automatic timer and the final `Sleep::deep` barrier keep it
from running. The power menu says Screen off. Short press continues an active game as the existing Screen off
choice does; idle pauses it. The Sleep & power page has only Screen off after, Switch off after, and the fixed
short press. It explains why. Switch off after defaults Off on a fresh SM8550 install; an explicit saved time is
kept. With it enabled, a dark spell starts a boottime deadline, and the existing light-sleep power-off path saves
and switches off on battery only. No RTC alarm or failed suspend is involved. Wake clears the deadline.

Screen-off-only profiles also request sway output power off/on (DPMS), alongside the existing brightness save/restore.
The real panels and their wake still need hardware verification.
