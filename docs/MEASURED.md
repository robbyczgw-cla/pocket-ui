# Measured: speed, sleep and CPU on the Mini V2

Moved here from the homepage (pocketui.xyz/measured.html, 2026-10-30); the site now links to this file. The numbers
are those of the runs named below; newer runs live in `docs/LATENCY.md` and `docs/latency/`.


Retroid Pocket Mini V2 · ROCKNIX 20261001 · recorded 25 October 2026 at commit 0eacac4

All times in milliseconds, lower is better. Each run measures every number several times and keeps the median; the value below is the median over three runs, after a warm-up run that isn't counted. Settings during the run: performance profile Balanced (now called Auto), fan Quiet. Games: uCity (Game Boy Color), F-Zero: Maximum Velocity (Game Boy Advance), Super Mario All-Stars & World (SNES).

**Speed guard, Mini V2, 25 October 2026**

| Moment | Median | Spread | From → to |
|---|---|---|---|
| Start | 55.4 | ±6.8 | pocket-ui started → its first shelf frame (the launcher's own work before it is not included) |
| A step on the shelf, median | 4.9 | ±1.1 | Down or Up → the frame with the new selection (100 moves per run) |
| A step on the shelf, 95th percentile | 14.4 | ±1.7 | the same |
| Settings | 10.1 | ±6.9 | START → the first Settings frame |
| Game switcher | 15.3 | ±8.1 | SELECT on the shelf → the switcher |
| Pause menu | 24.2 | ±4.4 | HOME in a game → the menu (including pausing the game) |
| Pause menu, again in play | 25.8 | ±11.6 | the same after Continue and a second of play |
| Switcher from a game | 34.4 | ±14.8 | SELECT in the pause menu → the switcher |
| Game start, Game Boy Color | 135.5 | ±30.9 | A on the game → its first frame, resumed where it was left |
| Game start, Game Boy Advance | 187.3 | ±8.5 | the same |
| Game start, SNES | 199.6 | ±6.7 | the same |
| Quit | 55.9 | ±7.0 | A on Quit → the first shelf frame (the game's moment is saved first) |
| Switch games | 220.5 | ±31.2 | A on the other game in the switcher → its first frame (the first saves and quits, the other starts) |
| pocket-ui's CPU, idle shelf | n/a |  | not measured in this run: the script read a counter this kernel doesn't have and wrote 0. Measured another way on 26 October: 0.1% of one core (below) |
| pocket-ui's CPU, behind a game | n/a |  | the same; 0.1% on 26 October |

## Before the speed work

The first baseline, 23 October 2026 at commit 8f158b4, measured the same way. Quit and switch were slow then: the next game waited for the old process to end.

**23 October → 25 October, medians**

| Moment | 23 Oct | 25 Oct |
|---|---|---|
| Start | 72.1 | 55.4 |
| A step on the shelf (median / 95%) | 7.7 / 14.8 | 4.9 / 14.4 |
| Settings | 2.5 | 10.1 |
| Game switcher | 55.5 | 15.3 |
| Pause menu | 154.8 | 24.2 |
| Switcher from a game | 58.0 | 34.4 |
| Game start, GBA / GBC / SNES | 165.7 / 189.5 / 238.1 | 187.3 / 135.5 / 199.6 |
| Quit | 503.7 | 55.9 |
| Switch games | 709.1 | 220.5 |

Not everything got faster. Settings went from 2.5 to 4–10 ms when it gained its categories and a page transition, and a Game Boy Advance start went from 166 to 187 ms.

## Waking from deep sleep

**24 October 2026, after the system is back from suspend, timer wakes**

| Moment | Before | After |
|---|---|---|
| Screen on, on the shelf | 1749 | 8 |
| Screen on, in a game | 1630 | 48 |
| The game's sound |  | 56 |
| The shelf takes a button | 3725 | 643 |

The shelf ignores buttons for 600 ms after a wake on purpose, so a press in a bag doesn't start a game; a running game gets them at once. These wakes came from the clock's alarm. A timed wake with the real power key, and the kernel's own resume before the numbers start, are not measured.

## Battery in sleep

Deep sleep: **4.8% an hour**, 27% in 5.63 hours with a PlayStation game open, two suspends, none failed (25 October 2026, one run). Other SM8250 and SM8550 handhelds report 4–5% an hour. An overnight run has not been done. Light sleep (the screen dark after a few idle minutes) has not been measured.

## The game host

**pocket-core against RetroArch, the same cores, 30 seconds each, 21 October 2026**

| System | pocket-core CPU | RetroArch CPU |
|---|---|---|
| Game Boy Advance | 22% | 42% |
| SNES | 24% | 42% |

**Screen effects on the GPU** (26 October 2026, commit 9df5564, pocket-core's own GPU timer, integer scale, no screen frame, 600-picture windows): Donkey Kong Country took 0.19 ms for the picture pass without an effect, 1.32 ms with Scanlines and 1.46 ms with CRT, so the CRT adds about 1.3 ms, some 8% of a 60 Hz frame. F-Zero on the Game Boy Advance: 0.17 ms without, 0.28 ms with the LCD grid. The graphics chip's clock was not logged, and other cores, 3D games, Crisp and the Aspect scale were not measured.

An earlier version of this page said 0.40 ms without and 0.58 ms with any effect (25 October). That measurement was wrong: all three effects read the same, most likely because the effect was never switched in the running game.

**Fan:** Burnout 3 on ARMSX2 with the Quiet profile averaged a fan level of 52 against Auto's 64 (at most 71 against 80), the CPU at 60–70 °C, without clock throttling (23 October 2026).

## Against ROCKNIX's own menu

What a Mini V2 runs without pocket-ui: ROCKNIX's EmulationStation with RetroArch. The speed guard above reads pocket-ui's own trace, which EmulationStation doesn't have, so this comparison measures both from outside, the same way. Each press is written into the Pocket's own pad device and takes the path a finger's press takes (through InputPlumber); the screen is recorded from the compositor, each frame with its time on the same clock as the press. Same games and the same cores on both (uCity on Gambatte, F-Zero: Maximum Velocity on mGBA, Super Mario All-Stars on Snes9x), RetroArch with ROCKNIX's defaults (resume states on, bezels on), EmulationStation with its default theme, the device on the charger, a library of 332 games.

**Mini V2, 26 October 2026, ROCKNIX 20261001, pocket-ui at commit 9df5564; medians, milliseconds unless marked**

| Moment | EmulationStation + RetroArch | pocket-ui + pocket-core |
|---|---|---|
| Cold start: the service started → the start screen | 2083 | 192 |
| Cold start → the first press acted on | 2136 | 589 |
| A move in the game list: the screen responds | 41 | 12 |
| A move in the game list: done | 41 (no animation) | 157 (a 120 ms slide) |
| Game start → the game on screen, GBC / GBA / SNES | 1795 / 1687 / 2031 | 203 / 188 / 231 |
| Presses to start a game where it was left | 3 (A, Right, A) | 1 (A) |
| Pause menu, GBC / GBA / SNES | 241 / 182 / 190 | 183 / 164 / 165 |
| Quit to the list, GBC / GBA / SNES | 857 / 901 / 934 | 119 / 128 / 98 |
| From uCity to F-Zero: the waits of every press | 3544 (8 presses) | 759 (4 presses) |
| Deep sleep: system back → backlight on | 0.3 | 11 |
| Deep sleep: system back → first frame | 21 | 4 |
| Deep sleep: system back → a press acted on | 55 | 699 |
| CPU on the start screen, idle: the menu / the whole system | 6.1% / 21.0% | 0.1% / 10.2% |
| CPU behind a running game: the menu / the emulator | 0.0% / 8.2% | 0.1% / 10.7% |
| Playing 5 minutes, the emulator's CPU, GBC / GBA / SNES | 12.0 / 44.7 / 32.5% | 10.1 / 24.9 / 17.1% |
| Playing 5 minutes, the whole system, GBC / GBA / SNES | 28.4 / 59.4 / 48.5% | 27.6 / 40.1 / 36.7% |
| Battery while playing | not measured | not measured |

Five cold starts, 30 list moves, three of each game start, pause menu and quit, three switches and three wakes per side. CPU is in % of one core (the whole system: 800% would be all eight cores), read from the kernel's process counters over 20 seconds, without the recorder running. The play rows start each game from its power-on, so both show the same title and demo with the same core.

Where pocket-ui is slower, it is by design or a known fault: the list slides for 120 ms; the pause menu opens when HOME comes back up, so its times include the 100 ms of the tap; the shelf takes no button for 600 ms after a wake. After its cold start pocket-ui ignored the first press (at about 200 ms) and took the second (at about 570 ms): Home was on screen some 400 ms before it took a button. A fix is being tested and is not measured yet. RetroArch's game start includes loading its resume state; its quit writes that state first.

**What this leaves out.** Every time lies on the 16.7 ms grid of the compositor's frames, and the panel shows a frame up to 16.7 ms later, the same for both: differences under one refresh are not differences. The time is the compositor's, not the light's. While presses are measured, the recorder copies only a band across the middle of the screen (a whole screen of pocket-ui cost the compositor about 50 ms a copy, of RetroArch about 8 ms). The battery was not measured: the device was on the charger with a full battery. One device, one card, one evening.

## How it is measured

pocket-ui and pocket-core write a trace when asked to, one line per event with a monotonic time stamp in microseconds; without it, each trace point costs one atomic load. A script running on the device starts pocket-ui with the trace on, presses buttons by writing to pocket-ui's command pipe, and reads the time just before each write from the same clock. A “frame” is the moment the frame is handed to the compositor. No screenshots are compared: the trace says which screen showed which input.

What that leaves out: the panel's next refresh (up to 16.7 ms, the same in every run), and the time from a real button's switch to the kernel, which no software change affects. Nobody has filmed button to picture with a camera yet. The measuring run never counts as play: nothing of it goes into the play history.

## Frozen games (2026-10-31)

A on a recent game on the shelf until the moment is back, device on the bench, muted, on links under names of their own
(`docs/user/playing.md`). Same build, three rounds a game: Frozen games on, Animations on Cozy (the setting by default), against
Frozen games off with Cozy (the start waits for the animation) and with Quick. Hardware cores (N64, PSP) resume some frames after
their first picture: the number is the resume.

| Device, game | Frozen, ms | Off, Cozy, ms | Off, Quick, ms |
|---|---|---|---|
| Mini V2, GBA Advance Wars | 115, 157, 140 | 1060, 1059 | 153, 141 |
| Mini V2, SNES Top Gear | 123, 133, 148 | 1067, 1060 | 129, 103 |
| Mini V2, GB Super Mario Land 2 | 104, 130, 145 | 1062, 1073 | 137, 144 |
| Mini V2, PS1 Crash Bandicoot 2 | 187, 250, 178 | 1074, 1074 | 203, 193 |
| Mini V2, DS Contra 4 | 333, 334, 404 | 1079, 1073 | 361, 375 |
| Mini V2, PSP Bomberman | 651, 677, 685 | 1113, 1068 | 664, 707 |
| Mini V2, N64 Super Mario 64 | 773, 832, 900 | 1079, 1073 | 820, 804 |
| RG35XX Plus, GB Super Mario Land 2 | 406, 347, 336 | 1068, 1065 | 337, 338 |
| RG35XX Plus, GBA F-Zero | 406, 400 | 1076, 1074 | 448, 405 |
| RG35XX Plus, SNES Axelay | 417, 434, 413 | 1074, 1077 | 433, 468 |
| RG35XX Plus, PS1 Crash Bandicoot 2 | 607, 669, 669 | 1078, 1073 | 750, 970 |

Big emulators on the Mini V2, Frozen games on: PS2 Ico, A until `standalone: load Auto: sent` 3339 and 2087 ms (the load itself
after that wasn't timed); GameCube Super Mario Sunshine, A until the resume thread was done 17676 and 17657 ms, which includes its
wait for Dolphin's window and a timed wait for its message. 3DS: not measured, the device has only a homebrew game and the bench
couldn't open its pause menu. The fan came on (pwm 41, 43) by the second start of each of the two.

## Frozen games on Android (emulator)

**Emulator, not a phone:** X300-sized AVD (1216x2640, 480 dpi) on a MacBook Air M4, software GPU, no sound, 2026-10-31. These numbers
compare starts on that one setup and say nothing about a real phone. From the launcher taking the tap to the player's first game
frame drawn, in ms, from their log lines (`scripts/android/frozen-smoke.py`, phase measure); the system's slide into the game
is on top (film: the moment is already in the first frame of the slide, about 0.4 s until it has settled). Homebrew test games, all small.

| Game | Frozen, 5 rounds | Frozen, app closed before | Frozen off, still from its moment | First start, no moment |
|---|---|---|---|---|
| GB Libbet | 181, 196, 280, 313, 163 | 730, 631, 1014 | 179, 262, 193 | 401 |
| GBA Voxelburg | 158, 734, 288, 271, 232 | 776, 788, 627 | 208, 271, 153 | 179 |
| SNES Elevator Madness | 293, 477, 320, 580, 293 | 1222, 2791, 904 | 237, 405, 486 | 424 |
| PSP Sliding Tiles | 515, 477, 270, 272, 338 | 1370, 2149, 1913 | 383, 353, 347 | 621 |

Measured before the insert animation came to Android's start: frozen and off are the same start in this table, and the time a frozen start saves by skipping that animation isn't in it.

## PlayStation on Android (AYN Odin 2 Mini)

2026-11-10, a real AYN Odin 2 Mini (Android 13) over wireless adb, its own test app beside the installed one, media volume 0,
the device's own buttons by key injection. PCSX ReARMed 7b0e8f8f, no BIOS (its HLE stand-in). The game is the free 240p Test
Suite for PS1 (GPL-2.0, `filipalac/240pTestSuite-PS1`), in four shapes: a cue with its bin; a cue whose data track's name is
in other capitals, with a 100 MiB audio track of our own tone; an `.m3u` of two discs in their own folders; one `.chd`. Frame
times from SurfaceFlinger, sound from the player's 10-second log line and the system mixer. Playing 90 s (first two) or 60 s.

| Game | Frames per second | Late frames | Sound underruns (app / mixer) | Play to first frame | Copy |
|---|---|---|---|---|---|
| cue + bin, 0.8 MiB | 60.00 of 60 Hz | 0 of 4997 | 0 / 0 | 8.6 s (first start: Controls card) | 32 ms |
| cue + 2 bins, 101 MiB, first start | 60.00 | 0 of 4700 | 0 / 0 | 5.0 s | 319 ms with the window |
| the same, second start | 60.00 | 0 of 3177 | 0 / 0 | 4.9 s | 29 ms (check, no copy) |
| `.m3u`, 2 discs | 60.00 | 0 of 3190 | 0 / 0 | 5.1 s | 63 ms |
| `.chd` | 60.00 | 0 of 3247 | 0 / 0 | 4.9 s | 22 ms |

Each: pause opens, Save and Load to a slot, Save & quit back to the shelf, all by the player's log. Options → Disc put in disc
2 of the `.m3u` (`player disc 2 of 2`). The insert (case, lid, disc, lid shut) and the copy window played as on other
systems. The play-to-frame time includes Cozy's insert. Sound queue median 42 to 94 ms. Not tried: `.pbp` (no free PS1
`.pbp` to hand), a real BIOS (none free to use), audio heard at the speaker.

## Big PSP games on Android (emulator)

2026-11-01, Android 15 ARM64 emulator on the MacBook Air, X300 size, 4 cores, 2 GB. `scripts/android/frozen-smoke.py ... big`, default settings from a fresh install.
PSP2048 (homebrew) in a generated ISO, padded. The emulator's disk is the Mac's SSD: a phone's own time isn't measured here.

| What | Time |
|---|---|
| Copy of 600 MiB of real bytes, page cache dropped | 3.25 s, about 185 MiB/s: about 10 s for 1.8 GB |
| Copy of 1843 MiB, mostly holes in the file (the Air's disk had no room for real bytes) | 1.6 to 2.7 s |
| Tap on Play to the game on screen, 1843 MiB with holes, first start | 6.0 to 7.2 s |
| Second start's check instead of a copy (`copy:` in the log) | 12 to 23 ms |
