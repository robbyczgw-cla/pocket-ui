# Start time

*Part of [DEVELOPING.md](../DEVELOPING.md). Read this when you touch start-up, instant start or anything that could slow the first frame.*

## Start time

pocket-ui should feel like a console woken from standby: the shelf as soon as sway takes a window, a game as soon as
its emulator can draw. Nothing that the first frame doesn't need runs before it on the UI thread.

- **pocket-ui** reads the type (`Text::new`) and the game list on threads of their own while it connects to sway
  and reads its settings; opens the pads, the touch screen and the command FIFO on a thread; sets the sway rules for
  game windows on a thread (`control::games_open_fullscreen_soon`; `launch` waits for them with `rules_in_place`).
- **Cold start** (every power-on: nothing of pocket-ui is in the page cache, each read waits for the SD card, and
  reads that run at the same time queue up behind each other): only what the first frame shows is read before it.
  - The game list comes from `<data>/library-cache.json`, the last scan (`library_cache.rs`): one file instead of
    es_systems.cfg and a listing of every ROM folder (205 ms of 319 ms on the Mini V2 with 332 games). After the
    first frame a thread reads the folders again (`app_library.rs`); a list that differs is swapped in like after
    Add games, at a moment nothing holds a game by its place (no game, game card, console page, menu, Settings or
    search), the selection kept on the same game or system; an equal one changes nothing (no frame, no write).
    Measured on the device with `scripts/latency.sh --cold` ([LATENCY.md](../LATENCY.md#cold-start)). The cache is
    turned away when es_systems.cfg (path, mtime, size), the cache format or pocket-ui's version differ, or when it
    doesn't parse; then the folders are read before the first frame as on a first boot, and the cache is written
    after it. Add games and web uploads write it too. A game whose file is gone (deleted while the cache still lists
    it) doesn't start: `launch` says so ("… can't start: its file … is no longer there") and reads the folders
    again.
  - Moved behind the first frame (`App::after_first_frame`, which also runs if no frame came within 2 s):
    the state of Settings → Store, Sync, Tailscale, Web companion, Bluetooth and RetroAchievements
    (`startup::Later`: read on a thread then; a use before that reads it at once, logged as `startup: … needed
    before the first frame`), the GameCube/Dolphin defaults in system.cfg (`emu_settings::apply_start_defaults_once`,
    which `launch` also calls, so a game started at once still has them), and the threads that start programs or
    read files the first frame doesn't need (`startup::after_first_frame`: `pactl` for the volume, the stick
    lights, the function keys, the Wi-Fi watchdog's `nmcli`, Samba's configuration, remote access, Store tools,
    the wait for Wi-Fi). Sleep's `systemd-inhibit` starts on a thread (`sleep::InhibitorSlot`). What stays before
    the first frame because it shows: settings.json (look, appearance, frames, On start, hidden systems),
    the display look (`Sys::new`, gamma), recent games, favourites, `library.json` (pinned stickers), Curator's
    switches (Lens and Home rows), the retired fallback list and the fan profile (a game On start continues needs it),
    sleep's settings.
- The library worker also indexes save/state file names before its result is used, so the first painted cases already know which games have saves. `play_evidence::Files` lists each shared folder once and uses name sets for game lookups; it reads no save contents. Refreshes after a library change or game run happen on workers. Device cold-start cost of this added directory pass still needs measurement.
- **pocket-core** opens the pads, the touch screen and the sound device while its window and GL come up, and makes
  its command FIFO with `mkfifo(3)` instead of the program.
- **The launcher** asks sway every 50 ms at boot (it was 250 ms).

Both programs log where the time went, step by step since `main` (`src/startup.rs`):

```
first frame after 9.9ms (logs 0.3, crash 0.1, sounds 0.3, event loop 0.8, input 0.0, settings+hw 0.2, sys 4.2, ...)
first frame after 54.3ms, 58 ms after the game was started in pocket-ui (core 9.7, input 0.2, fifo 0.3, resume 8.7, loop 0.0, graphics 26.3, audio 0.0, first frame 9.1 ms)
```

pocket-ui hands pocket-core the moment the game was asked for (`POCKET_LAUNCHED_AT`, `CLOCK_MONOTONIC` in ms), so the
second line counts from A. Measured in the virtual device (2026-10-22, M1 Max, 6 vCPU; the window counted when sway
maps it, polled from outside; old and new build run in turns on a host busy with other builds):

| | before | after |
|---|---|---|
| launcher start → shelf window, 18 systems / 844 games | 56-130 ms (median 82), outliers to 383 ms | 22-96 ms (median 39) |
| pocket-ui `main` → first frame (its own log) | 46-83 ms | 8.5-14 ms |
| A on "Continue" → game window (uCity, resumed), quiet runs | 76-155 ms | 52-91 ms |

The rest of the game start is pocket-core's GL setup (25-55 ms on llvmpipe) and the core. Take the device's own
numbers from its log; the virtual device only shows where the time goes. On the Mini V2, pocket-ui's first frame came
about 42 ms after `main` (2026-10-23), once the display look was applied at start without being saved again.

The slow-load note has its own dark backing and light type, independent of the menu appearance, so a light launch picture or black letterbox cannot hide it.

**Immediate launch response** (`src/app_launch.rs`): A starts the insertion before filesystem preparation. A worker
checks the selected file, resume compatibility, window rules and emulator settings.
The captured game and route stay fixed while it works; a list rescan waits. The process starts after a presented
response frame and a successful preparation. Errors still show their original notice. The loading spinner also covers
preparation lasting over three seconds. A button during preparation skips the insertion and any later reminder.
`tests/launch_response.rs` uses 3,000 extra games and a 300 ms mock preparation; it guards A to the first changed
animation frame at 50 ms, continued painting while waiting and the exact game eventually starting. With tracing,
`step launch …` lines bracket preparation and `launch first response painted` records the response independently
of the existing `launch` process-start marker.
On a Mini V2 with an existing resume state, the installed 0.3.2 spent 49 ms of 52 ms before `launch` in the
file and directory fsyncs of `controls-card-seen`: the first-console marker of the controls card (gone since 0.5) was
persisted even though resuming suppressed the card. An earlier trace showed 1,289 ms before `launch`; that outlier was not repeated
under step profiling. Other fsyncs on the same card took over one second. The regression injects a slow worker
rather than depending on the current SD-card cache or writeback timing. After the change, the Mini V2 presented
its response at 9.4 ms while the controls-marker check completed at 31.4 ms (same fixture, no recorder).
The 300 ms mock preparation changed from 317.7 ms before the first painted response to 24.2 ms.

**The launch picture** (`src/launch_picture.rs`): while a game starts, the shelf fades into the picture of where it
was left (the auto-resume preview, else the frame kept at Quit), placed where pocket-core will draw the game; for
pocket-core with the game's Picture setting (`video::dest_rect`), turned as its Screen orientation turns the game
(`video::turned_shape`, `turn_pixels`). pocket-core then fades the frame around the picture
in from black over 250 ms (`Overlay::draw_veil`), so the moment the game's window covers pocket-ui is not seen.
When this launch requests the old-TV switch-on (`crt_on::ENV`), pocket-ui leaves out that preview: insertion fades
to black, then pocket-core shows the line and opens the game. This applies to handheld frames and TV frames alike,
including Continue from Home. The deliberate preview behind a “Where was I?” reminder is kept so the card can be read.
`tests/user_journeys.rs::fifo::the_launch_picture_never_flashes_before_the_tv_switches_on` checks the rendered
launch for SNES, GB, GBA, PS1, N64 and Continue, with switch-on disabled and with a reminder as controls.
A known frozen Continue keeps its direct start without CRT; the Continue regression makes the moment available
after the shelf read, exercising the Cozy/CRT path for a moment that arrived later.

**Slow launch feedback** (`src/app_loading.rs`, `startup::LoadWatch`): pocket-core marks a load still busy after three seconds
on the runtime tmpfs, and removes that mark on success or error. The UI polls during a launch and paints the note with its spinner.
Each due tick requests a frame before advancing the next deadline; once the game loads, polling stops. The ordinary fast launch is unchanged.
The timer and load-watch regression tests cover the handshake; `tests/where_was_i.rs` checks the rendered message in all three languages.


**Where was I?** (`src/where_was_i.rs`, `src/app_where_was_i.rs`): a moment continued more than 14 days after the last
session gets a quiet card at the bottom of the launch picture (when it was played last, the time played, the game card's
note). Its history and note come from memory in `launch` (recent games, else the play log; `library_meta`'s note), and it is shown once
the launch picture is there and `frozen_at_launch` kept the moment. On pocket-core the start then asks for a hold
(`App::where_was_i_hold`: `HOLD_STILL=1`, until any button, with `HOLD_UNTIL` only bounding the animation): the
window stays unmapped and the core runs no frame once its moment is loaded (`startup::stands_still`; a 3D core's deferred
resume runs first), no sound, so the card can be read even where the moment is back in 0.1 s and nothing is missed. The
reminder has no timeout, including after a slow load. Any button ends it (pocket-core's own press check, and
`App::launch_button` → `startup::skip_hold`), including a press before the picture finishes loading. A preview alone
does not qualify: a matching resume state must exist. `HOLD_READY` points to a file the UI creates when its decoded
picture arrives: if the core reaches play before that, it starts immediately, so a slow read adds no invisible hold.
A missing or corrupt picture cancels the reminder and its hold;
pocket-core also drops that hold if the resume fails, is skipped, or keeps a newer battery save. Pause, sleep and screen
off end the launch hold so returning to play never waits for an obsolete card. Ordinary launches retain their 2 s
deadline safety bound; `HOLD_STILL=1` waits for a button. Other emulators can't be held: the card stays until the game covers it.
Off by default (0.5), On: settings.json `where_was_i` (only On is written; the old `no_where_was_i` is ignored); never in Kids Mode; Easy drops the time played.
The Playing category's summary includes the reminder as its own part, keeping device-specific omissions aligned with
their rows. Snapshots:
`where-was-i[:plain|long|easy|wait|press|early-press|missing]` (an optional third argument is a frame to show); tests:
`where_was_i::tests`, `startup::tests`, `tests/where_was_i.rs`, and `user_journeys::fifo::a_stale_preview_never_holds_a_fresh_start_for_a_reminder`.

**Settings → Playing → On start** (`src/on_start.rs`): `decide` has the rules and the tests. The resume note of a power off
(`sleep::take_resume`) always wins; Last game continues only on the first start of pocket-ui in a boot (boot id in
`<data>/started-boot`), without a crash notice, and when `<data>/last-game.json` says the game ended normally. In the
virtual device the boot id never changes: remove `started-boot` to try a "first start".

**Settings → Controls → Buttons → Button layout** (`src/button_layout.rs`; pure shared `ButtonLayout` in `src/button_layout_model.rs`, also used by the Android player's button names): the menus' code and hints speak in four roles on
`Button::East` (A, confirm), `South` (B, back), `North` (X) and `West` (Y). The setting (Nintendo: as printed; Xbox:
A and B swapped, X and Y swapped) decides which physical button plays which role. The one place it is applied is the
closure `main` hands to `pad::linux::supervise` (`Remap::event`, on the pad threads, before the event reaches the UI
thread), so `CONFIRM`/`CANCEL`, every page's `press`, the hints and the touch pills need no knowledge of it, and a new
screen cannot forget it. A release goes back as the role its press had (the press that switched the layout is let go
after the switch). Not remapped, on purpose: games (pocket-core and the emulators read the pads themselves, so Options →
Controls and the HOME shortcuts stay the printed buttons), touch, the command FIFO, the desktop preview's keyboard and
safe mode (`app_rescue::run` has its own loop and runs before settings.json is read; the layout is Nintendo until
`main` sets it from the settings). A new screen takes its buttons as `Button::East`/`South`/`North`/`West` roles like
the others; a new thing that must stay physical belongs outside the UI thread's event path. Tests: `button_layout.rs`
and `tests/button_layout.rs`.

**Settings → Controls → Buttons → Button symbols** (`src/button_prompts.rs`, the page `src/app_buttons.rs`, which also holds Button layout):
how a hint key looks, never which button it is. Every key is drawn by `kit::key_cap` (both hint bars, the picture of the four buttons on
the Buttons page, the manual's chips; the manual's controls picture asks `button_prompts::shown`/`colours` itself): Nintendo
draws the letter on the hint's cap, Xbox the letter on its colour (`colours`), PlayStation the shape for the role
(A ✕, B ○, X □, Y △). The shapes are private-use characters (`button_prompts::CROSS` …) that `Text::glyph` rasterises with
`button_prompts::raster` at the face's cap height, so measuring, fitting and the hint bars' label rules treat them as
one letter. Unset: `device::Device::prompt_style` (Xbox on Retroid and AYN, Nintendo on Anbernic and unknown devices);
`POCKETUI_PROMPTS=nintendo|xbox|playstation` overrides settings.json for snapshots (`buttons[:<row>][:<presses>]` is the Buttons page). Prose ("Press A to …") keeps its letters.

**Look → Colours** (`palette::Colours`, `Scheme`): five sets of the palette tokens, each with a dark and a light
`Palette` and the Cabinet's stain (`wood`, `stain`), plate (`holder`, `card`, `card_ink`) and slot light (`lamp`).
`pal()` mixes the set in use; a new token or a new set needs no other change, and `palette::tests` checks every text pair
of every set for 4.5:1 in both appearances and a page colour of its own per set (caches like the Cabinet's wood are
keyed by it). The constants (`VOID`, `CHALK` …) and `DARK` stay Warm's: overlays over a game keep them.
`POCKETUI_COLOURS=warm|indigo-cube|grey-brick|green-x|blue-station` overrides settings.json for snapshots;
`POCKETUI_RETRO=cocoa|pea` paints a snapshot in the Retro style.

## H700 game graphics and the page cache

`core_prewarm` runs only after the shelf's first frame. On H700 it reads the chosen libretro core, the exact
`POCKETUI_HOST_BIN` executable (default `/storage/pocket-ui/pocket-core`), and the installed EGL/Mesa libraries,
including the versioned Gallium driver, into the page cache. Then it follows the `DT_NEEDED` lists of these files
(read from their ELF headers, at most 64 libraries, names resolved in `/usr/lib`), so libstdc++, the X11/xcb
libraries, zlib and zstd that Mesa's EGL loads come along, and it reads glvnd's vendor list and Mesa's drirc files.
It creates no context or surface and calls no library code. The core may be up to 35 MiB; the idle I/O and nice 19
worker has one 64 MiB budget for the whole set, the core and player first, Gallium before the small libraries;
canonical paths avoid reading symlinks twice, and missing or oversized files are skipped. A launch or sleep cancels the next 256 KiB
piece. Other devices keep the single-core warm-up; Android keeps none. `POCKETUI_NO_CORE_PREWARM=1` disables
both the core and H700 runtime reads for measurement. No player preference or persistent cache is changed.

`pocket-core`'s first-frame log splits graphics into display/EGL, window, context, surface, make-current/GL entry points,
renderer shaders and overlay. The remaining `graphics` marker is the optional hardware-rendered core's setup.
Texture upload, first drawing and swap follow in `first frame`. Do not compare one substep with the older combined
`graphics` number, or a Cozy hold with Quick. Cold tests must state whether Mesa's disk cache was present and when
`drop_caches` ran; the page cache and shader cache are separate. The Plus report `PLUS-START-TIMES.md` in the
review workspace records the five-run device comparisons and the limits of the boot analysis.

On the RG35XX Plus (16 November 2026), five alternating cold Quick starts per version, with an empty private Mesa
shader cache and five seconds on the shelf: core-only warm-up 991 ms median, runtime warm-up 656 ms (33.8% less).
Renderer setup fell from 167.9 to 41.9 ms, GL entry-point setup from 34.8 to 8.8 ms. The shelf's own first-frame
medians were 1377 and 1374 ms; those small differences are within the observed spread. The warm-up read about
30.6 MiB in about 1.3–1.7 seconds after the shelf appeared. This does not measure an immediate A before the worker can
finish, nor power-on time, and the older 2.6-second game-start result was not reproduced in this setup.

With only Mesa's own libraries warm, EGL display setup and config choice still took about 287 ms of such a cold
start: glvnd loading the vendor library pulled libstdc++, X11/xcb, zstd and the drirc files from the card. A
standalone probe on the Plus (17 November 2026, `drop_caches`, then the old set read) took 296 ms for those EGL
steps, 45 ms when the `DT_NEEDED` closure was read as well, and 46 ms fully warm. In the same game-start
setup as above (10 alternating starts per version, Advance Wars on gpSP) the display/EGL step fell from 310 to 88 ms
(median; p90 328 to 118 ms) and A to the first game frame from 657 to 505 ms (p90 667 to 525 ms). The worker now
reads about 38 MiB in 34 files; the shelf's first frame comes before it starts and stayed within the spread
(1321 and 1348 ms). Faster graphics can leave the start waiting for audio to open: a test guard that flipped the
sink's mute made that wait 0.4–0.5 s in two runs, so device measurements keep the volume at 0 without mute flips.
