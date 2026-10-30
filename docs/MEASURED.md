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
