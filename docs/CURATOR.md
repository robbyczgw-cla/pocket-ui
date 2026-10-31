# Curator — Roon-style discovery for games

Design from 2026-10-23; the sections up to "Build order" are the plan. What was built, and where it differs, is in
"Pixel Path as built", "Developer notes: what is built" and "Curator 2 as built" below, and for players in
[user/curator.md](user/curator.md#curator) and [user/pause-menu.md](user/pause-menu.md#pixel-path).

Roon is a music player that treats a record collection as a web of facts
and relationships rather than a folder of files. Curator brings that idea to the games on the
device. It is one family of features, and each one can be switched on or off under
Settings → Games → Curator.

| Name | What it is | Roon counterpart |
|---|---|---|
| **Lens** | Combine filters ("GBA · Action · D-pad · under 8 h · never started"). The result count updates live. | Focus |
| **Smart shelves** | A Lens saved under a name. It sits on Home like Favorites and updates itself as the library and play status change. | Saved focus / bookmarks |
| **Stickers** | Your own labels for games, such as "Couch co-op" or "Before bed". | Tags |
| **Echoes** | "More like this": ranked games you own, each with a one-line reason. | Radio |
| **Up next** | After a game, one quiet suggestion on the shelf: its closest Echo you might play next. | Radio's continuation |
| **Versions** | One page per game across systems (SNES, PS1, DS), with the differences, your saves and a preferred version. | Album versions |
| **Pixel Path** | The live chain a frame and its sound go through, from core to screen and speaker, graded in colour. | Signal Path |

## Ground rules (from the owner, 2026-10-23)

- **Fully offline.** Every feature works without internet. The data ships in the binary, as
  `assets/gamedb.bin` does today.
- **Reacts in a fraction of a second.**
  - Every keypress in a Curator screen costs < 16 ms (one frame).
  - Opening a Curator page takes < 50 ms.
  - Nothing new runs on the UI thread at startup.
  - The index builds in the background after the library loads.
  - Until the index is ready, Curator entries show as "Getting ready…" and never block.
- **D-pad and a few buttons.**
  - No new global buttons are needed. Everything is reached through things that already exist:
    - Home rows (like Favorites)
    - the game card (X)
    - the pause menu
  - Typing only happens when you name a sticker or shelf, and a sensible name is always suggested.
- **Simple by default.**
  - A Curator feature takes no space until it has something to show.
  - Smart shelves and stickers appear on Home only once you make one.
  - Echoes and Versions are rows on the game card.
  - Pixel Path is off by default.

## What we already have (measured 2026-10-23)

`assets/gamedb.bin` holds Wikidata (CC0) facts:

| Measure | Count |
|---|---|
| Systems | 32 |
| Games | 20,674 |
| With a genre | 17,822 |
| With an English Wikipedia article | 15,409 |
| Distinct genres | 387 |
| Same Wikidata item on two or more systems | 3,273 |

The data also has series, developer, publisher, players, modes, review scores and sitelinks (how
well known a game is). `library_meta.rs` adds play status, sessions, play time and a note per game.
The game card already shows the related games you own.

**Why that is not enough for Echoes.** In Wikidata, Burnout, Burnout 2, Burnout 3, Revenge and
Dominator are all just "racing video game". Gran Turismo 4 is too. Wikidata's genres cannot tell
an arcade crash racer from a simulation. Echoes needs **feel**: pace, handling, crash mechanics,
session length, difficulty, story weight, grinding and control needs. That calls for a second
layer of data.

## Data in three layers

Built on 2026-10-23 (layers A, B and the file-name part of C): see [CURATOR-DATA.md](CURATOR-DATA.md) for what
exists, the reader APIs and the quality numbers.

### A — Facts (Wikidata, CC0, extend `scripts/build-gamedb.py`)

New properties:

| Property | Use |
|---|---|
| P4151 game mechanics | Mechanics |
| P479 input method | Light gun, touch screen, motion control |
| P144 based on, P4969 derivative work | Links ports, remakes and remasters |
| P155 follows, P156 followed by | Order within a series |
| P407 language of work | Where present |
| Platform list per item | Versions, from the same QID on several systems |

### B — Feel tags (our own data, built offline, bundled)

**Vocabulary.** About 150 tags in `assets/data/curator-tags.json`, on a few axes:

| Axis | Examples |
|---|---|
| Family | About 25 families that the 387 Wikidata genres map onto |
| Sub-genre | Arcade racing, sim racing, kart racing, metroidvania, twin-stick, … |
| Mechanics | Takedowns / crashes, drifting, open world, … |
| Pace | Calm → frantic |
| Session | Pick-up-and-play (≤ 15 min) or sit-down |
| Length | An estimate in buckets: < 5 h, 5–15 h, 15–40 h, 40 h+, endless/score attack |
| Difficulty | — |
| Story weight | — |
| Text-heavy | Needs a language you read |
| Grinding | — |
| Controls | D-pad friendly, needs analog, touch, two screens |
| Mood | Cosy, tense, funny, … |
| Multiplayer | Co-op, versus, on one screen |

Every tag has a short English label written for reasons ("crash takedowns"), and a weight of 1–3
per game.

**How the tags are made.**
- A batch labeller runs once at build time. It sees the Wikidata facts for each game plus the
  vocabulary, and answers in strict JSON. It may answer "unknown".
- Only games with sitelinks ≥ 3 get tags. Obscure games stay on layer A, so nothing is invented
  for them.
- The developer's own library comes first.
- Wikipedia text is never stored; only our tags are.

**How the tags are checked.**
- **Golden set.** About 80 games are labelled by hand, carefully, as a regression fixture. The
  labeller must reach ≥ 85 % tag precision against it, or the prompt or model is changed.
- **Cross-check.** A model from another family re-labels a sample. Tags the two models disagree
  on are dropped.

**Format.** `assets/curatordb.bin`: a header plus one zlib stream per system, keyed by QID, the
same pattern as gamedb. It is a few hundred KB, so it is bundled. A download step on first start
is only worth it later, if optional large packs (screenshots, long texts) are added.

### C — You (local, private)

- Play time, sessions, status, stars and stickers, from `library_meta` and favourites.
- Facts from the ROM file name: region and languages (No-Intro `(En,Fr,De)`), translation patches
  (`[T-En]`), revision.
- Which saves and states exist for a file.

From these the device builds a **taste profile**: a weight per tag.
- Time played counts, capped per game.
- Completed and starred count positive.
- Dropped counts negative.

The profile is recomputed in the background after each session.

## Engine (`src/curator/`, pure logic, unit-tested)

- **Index.** One bitset per facet value over the games in the library: 5,000 games make 79 `u64`
  words per value.
  - Lens intersects bitsets: AND across facets, OR within one facet.
  - The live count per value is a popcount.
  - Both take microseconds.
  - The index builds on a background thread after the library scan, and again after changes.
- **Echoes score** for a candidate *c* given a seed *s*:
  - **Base:** cosine similarity of the IDF-weighted tag vectors, using the B tags and the A genres
    and mechanics.
  - **Bonuses:** + same series 0.25, + same developer 0.15, + release years within 5 years 0.05.
  - **Taste:** × (1 + 0.3 · affinity(taste profile, c)).
  - **Status:** Not started + 0.1, Completed − 0.15, Dropped excluded.
  - **Re-ranking:** MMR, so the top 10 is not ten games of one series (at most 3 per series).
  - **Scope:** games you own by default. "Not on this device" is an optional section, off by
    default, and only names games.
- **Reasons.** The two largest contributions to the score become the reason line, from templates:
  - "Same series"
  - "Arcade racing · crash takedowns"
  - "Also by Criterion Games"
  - "Short races, fast pace"
  - "Like Burnout 3, which you finished"
- **Versions.** Games are grouped by work.
  - The same QID means the same work.
  - An item that is a port (P144 / P629 to the main item) also counts as the same work.
  - A remake stays separate and is shown as "Related: remake".
- **Stickers** live in `library.json` next to status and note: a list of sticker ids per game,
  plus a sticker list with name and order.

## Screens

- **Home.**
  - A **Curator** row after Favorites. It opens Lens, the smart shelves and the stickers. (As built since
    2026-10-26 it is the top row of Home in the list looks, with the day's pick on it: "Curator on Home" below.)
  - Every smart shelf and every sticker the user pins shows as its own row.
- **Lens.** (As built it has 22 facets, `curator::Facet::ALL`: Mechanics, Pace, Difficulty and Setting joined the
  plan below, Story, Grinding and Reading came with Curator 2, My rating with the sort orders; the user guide lists
  them and a test keeps it in step.)
  - Facets on the left:
    - System
    - Family
    - Sub-genre
    - Players
    - Length
    - Session
    - Status
    - Played (never / this month / long ago)
    - Language
    - Controls
    - Era
    - Mood
    - Stickers
    - Favourites
  - The values of the selected facet are on the right, each with a live count.
  - Buttons:
    - D-pad Up/Down moves between facets.
    - Right opens the values.
    - A toggles a value.
    - The header reads "37 games".
    - START shows the games.
    - X saves the Lens as a smart shelf, with a suggested name such as "GBA · Action · short".
    - Y clears the facet.
    - B goes back.
- **Game card (X).**
  - An **Echoes** row with the top 3 inline. A opens the full list of 12, each with a cover and
    its reason.
  - A **Versions** row, shown only when two or more versions are owned.
  - A **Stickers** row: A toggles stickers, and the last entry is "New sticker…".
- **Versions page.** One row per version with:
  - the system
  - region and language
  - whether it is a translation
  - how it runs (core or emulator, performance preset)
  - saves (count, last played, time)
  - known differences, from data only, otherwise nothing

  Y marks the preferred version. Lists that mix systems (Favorites, smart shelves, Echoes, search)
  show a work with several versions only once and start the preferred one.
- **Pixel Path** (pause menu → Pixel Path). Built after run-ahead is merged. The chain:

  | Stage | Example |
  |---|---|
  | Core | `mgba` |
  | Source | 240×160 @ 59.73 Hz |
  | Scale | 4× integer, 960×640, centred — or 4.5× smooth |
  | Filter / shader / frame | — |
  | Display | 1240×1080 @ 60 Hz, rate matched within ±0.5 % |
  | Latency | Input to photon, an estimated number of frames: core + buffering − run-ahead |
  | Audio | 32,040 Hz → 48,000 Hz, dynamic rate control, buffer, underruns |

  Live readouts: fps, dropped frames, core ms per frame.

  Each stage is graded, and the overall grade is the worst stage:

  | Grade | Colour | Meaning |
  |---|---|---|
  | **Pristine** | purple | Pixel-perfect and native |
  | **Faithful** | green | — |
  | **Enhanced** | blue | Upscaled internal resolution, run-ahead |
  | **Compromised** | orange | Drops, underruns, rate mismatch |

  Standalone emulators show what their settings files say (see "Pixel Path for PS2, GameCube, Wii and 3DS" below).

### Pixel Path as built (`feat/pixel-path`, 2026-10-23)

Files: `src/pixel_path.rs` (the report, the live window, grading, the latency estimate; pure, unit-tested),
`src/pixel_path_page.rs` (page state, the switch), `src/app_pixel_path.rs` (reader thread, painting),
`App::write_pixel_path` in `src/bin/pocket_core.rs`.

- **Switch.** Settings → Games → Curator → **Pixel Path in the pause menu**, `Prefs::pixel_path` in `<data>/curator.json`
  (off by default). pocket-ui keeps it in memory (`App::pixel_path`, read with the other Curator prefs at start and
  set again whenever the Curator page changes its switches); the pause menu only looks at that flag.
- **Data channel**, like the preview and the achievements list: the open page's thread writes `pixelpath <seq>` to
  `/tmp/pocket-core.cmd` every 250 ms, pocket-core answers with `/tmp/pocket-core.pixelpath.json` (written to a
  `.tmp` and renamed, no fsync; ~60-80 µs a report in the virtual device), the thread reads the report for its game
  and number and sends it to the UI thread only when something but the number changed (a paused game's report
  never changes, so nothing is repainted). `pixelpath` is the one command that doesn't make run-ahead forget its
  predictions. Closing the page (B, Continue, Quit, the menu going) drops it and the thread ends within 25 ms.
- **What is measured.** Picture placement from the renderer (`Renderer::placement`, `frame_size`), the Picture
  settings, the display look, the core's colour options (`colour_options`: colour correction, frame blending), the
  panel's refresh and whether frames are scheduled by time, run-ahead (wanted, in use, waiting, why off), the audio
  rates, queue target and fill, output latency and underruns. The live line covers the last 5 s of play: pictures
  and late refreshes from the frame intervals pocket-core keeps for its exit report (late = refreshes in the time
  covered that brought no new picture), core ms from its run times, emulated fps and CPU (all threads, from
  `/proc/self/stat`) from the FPS counter's half-seconds (`Hud::frame` now returns them; `pixel_path::Window`
  keeps ten), underruns per half-second. `Hud::restart` after a pause keeps the pause out of both.
- **Grades** (`pixel_path::path`):

  | Stage | Pristine | Faithful | Enhanced | Compromised |
  |---|---|---|---|---|
  | Core | the console's frames | | cheats on | |
  | Source | 2D: the core's own size; 3D: within 0.9-1.5× the console's lines | | 3D: ≥ 1.5× the console's lines | 3D: below 0.9× |
  | Scale | whole on both axes, Sharp or Crisp | whole lines, non-square pixels; whole but Soft; fractional Crisp; fractional where no whole size fits, and 3D | | stretched > 1 % out of shape; fractional Sharp or Soft where a whole size fits |
  | Look | nothing over the picture (a frame around doesn't count) | the core's colour correction / LCD blending | an effect (grid, scanlines, CRT; the LCD grid with the GBA's colours), a colour temperature or contrast | |
  | Display | core rate within 0.05 % of the panel | one frame per refresh, faster or slower (≤ 5 %) | | 50/75 Hz games scheduled by time; late pictures in the window |
  | Latency | | no run-ahead | run-ahead in use | the core takes ≥ 75 % of a refresh |
  | Audio | | resampled, rate-controlled, no gaps | | underruns in the window; no output |

  The Look stage says what each effect does (`pixel_path::effect_about`, since `feat/shaders` 2026-10-24): LCD grid "a
  thin dark gap between the game's pixels, a fixed share of each pixel, so it grows with the scale", Scanlines "drawn
  as a TV's beam … every line keeps its light", CRT "scanlines, a fine colour mask too small to see at arm's length
  and a soft glow … no curved screen"; each "looks like a TV (a handheld's screen), not like the console's own
  pixels". All three are **Enhanced**: they add to the picture on purpose, however faithful to a TV they are. The
  LCD grid with LCD colours on reads "LCD grid, GBA colours" and adds that the colours are the GBA screen's; with the
  emulator's own colour correction on, ours steps back and the value is plain "LCD grid" (the core's correction is
  listed as before). Crisp keeps its Scale grades (it is now blended in linear light: same grade, evener pixels).
  The overall grade is the worst stage in the order Pristine < Faithful < Enhanced < Compromised. Audio is always
  resampled (linear, ±0.5 % rate control), so a pocket-core game is at best Faithful overall; stages can be Pristine.
- **Latency estimate** (`pixel_path::Latency`), in refreshes: buttons read once at the start of a refresh (0.5 on
  average) + the core's measured time + 1 (swap to the compositor's next vsync) + 0.5 (scan-out to the middle of
  the panel) − run-ahead frames. The game's own reaction comes on top and is said so. Not checked against a
  camera yet: see the device checks below.
- **Standalone emulators**: ARMSX2, Dolphin and Azahar show their settings since `feat/pixel-path-standalone` (below).
  RetroArch and the other emulators have no Pixel Path in their menu (reading RetroArch's or PPSSPP's video settings
  is a follow-up); until 2026-10-24 they showed seven rows "Managed by <emulator>".
- **Not live while the menu is open**: the game is paused behind the pause menu, so the numbers are the last
  seconds before it; the page asks four times a second anyway (cheap) so a game that runs behind it would show.
  The live readout over the running game is built: see the next section.

### Pixel Path over the game (`feat/pixel-path-live`, 2026-10-23)

Files: `src/pixel_live.rs` (the strip: its items, alerts, placement, painting; pure, unit-tested), the strip piece of
`src/overlay.rs` (`Hud::set_strip`, `Overlay::draw_strip`), `App::update_strip` and `App::pixel_report` in
`src/bin/pocket_core.rs`, Y on the page (`app_pixel_path.rs`, `PixelPathPage::toggle_over_game`), HOME + Y
(`hotkeys::Action::NextOverGame`), `examples/over_game_shots.rs`. Since 2026-10-25 it is the Pixel Path level of
**Over the game** (`src/over_game.rs`, [HOST.md](HOST.md), "Extras"). User guide:
[user/pause-menu.md](user/pause-menu.md#pixel-path-over-the-game).

- **What it shows.** One line, "● mGBA · 5× sharp · 60.00 fps · ≈34 ms · 48 kHz": the dot in the overall grade's
  colour (`pixel_path::path` on the same report the page reads), the core, the scale (`fit`: whole or fractional, the
  filter; a 3D core above the console's lines `native_lines`: "2× res, crisp"; ", stretched"), pictures per second
  (time-driven 50/75 Hz games: the emulated rate), the latency estimate (`Latency`, with run-ahead "≈19 ms, 1
  ahead") and the output rate ("no sound" without output). All over the page's 5 s window.
- **When it turns orange.** Late pictures or underruns in the window ("3 late", "2 underruns"), and run-ahead switching
  itself off ("run-ahead off" for `HOLD_TICKS` = 10 updates, 5 s; a switch-off while the strip was hidden is not
  announced later, `Live::hidden`). The dot is orange while any of these is in the line; the grade itself is not
  changed, so a 50 Hz game's dot stays orange (Display is Compromised) while its text stays calm.
- **Cost.** Nothing per frame but one textured quad and a bool: `App::update_strip` runs with the FPS counter's
  half-second (`Hud::frame` returning a tick), builds the report without the core's colour options (they can't move
  the overall grade: Audio caps a pocket-core game at Faithful) and grades it; the HUD paints the strip again only
  when it changed (`Hud::strip_seq`). Measured in the virtual device (`--bench`, llvmpipe, a host shared with other
  agents; `pixel path strip:` is a bench-only line of the exit report):

  | Game | Strip | Core ms/frame | Draw ms/frame (median) | Strip update |
  |---|---|---|---|---|
  | GBA MegaMan & Bass (mGBA) | off / on | 0.89-1.38 / 0.84-1.29 | 4.3-9.0 / 3.9-9.8 | 41 µs mean, 70 µs max |
  | SNES Contra III (Snes9x) | off / on | 0.78-0.83 / 0.70-0.91 | 4.3-4.5 / 4.5-4.9 | 63-79 µs mean, 399 µs max |
  | N64 Super Mario 64 (Mupen64Plus-Next, 2× res) | off / on | 5.6-7.7 / 3.3-6.8 | 5.4-5.7 / 4.1-7.6 | 33-40 µs mean, 77 µs max |

  The on/off differences are within the run-to-run noise of the shared host; one GBA run had a single 10 ms update
  (the same run's core time peaked at 17 ms: the host, not the strip). Painting a strip (measure, place, rasterise,
  `over_game_shots pixel` prints it) takes 14 µs on the M1, plus one upload of about 640×46 RGBA (550×42 before the strip's
  type went from 24 to 28 px, `fix/contrast` 2026-10-24).
- **Placement** (`pixel_live::place`): never over the picture where there is room. The band above the picture if it
  is at least the strip's height + 16 px (GBA 5×: 140 px, SNES 4×: 92 px, N64 at Aspect: 75 px), lined up with the
  picture's left edge; else the band below; else a column (one item a line) in the left or right band (Nintendo DS
  stacked: 260 px); else small (24 px type) and see-through (alpha 0.6) in the top left corner (Game Boy 7×: 36 px
  bands; Fullscreen). The frame around the picture may lie under it: the frame isn't the picture. It is drawn under
  the toasts, the volume bar and the unlock card, which come and go over it.
- **Style.** The HUD's: the dark pill and light text of the FPS counter and the toasts, at 0.82 opacity, Barlow 28 px (24 px until 2026-10-24: too small at arm's length),
  the same in dark and light appearance (nothing over a game follows the appearance). Grade colours are the page's.
- **One setting with the FPS line: Over the game** (2026-10-25). The strip was first a third choice of Show FPS, with
  its own shortcut, a Curator default ("Pixel Path over every game") and the Curator switch gating all of it. Now it is
  the **Pixel Path** level of Over the game (Off, FPS, FPS + sensors, Pixel Path; `over_game::Level`): one value per
  game over per console over Settings → Playing → Over the game, and no state in which two pieces draw. The level
  doesn't wait for "Pixel Path in the pause menu" any more, which is the page's switch alone; pocket-core no longer
  reads `curator.json` for it. Old values are carried over (`over_game::migrate` for Options files, `adopt_curator`
  for `pixel_live`, only with `pixel_path` on, as it worked).
- **Turning it on.** Y on the page sends `set over_game pixel` (or back to what it was, `toggle_over_game`) and changes
  the menu's values, exactly like a change in Options: in effect at once, kept with Save changes. HOME + Y (free:
  HOME + R1/L1/Left/Right/R2/L2 are taken, HOME + SELECT is the switcher, HOME + Vol ROCKNIX's brightness, HOME +
  START + Vol its Wi-Fi switch) moves `Extras::over_game` on to the next level for the session, with a toast; a change
  of the option sets it again. Options > Shortcuts > Over the game switches it off.
- **Snapshots.** `over_game_shots pixel` paints a demo strip where the HUD puts it over a `vdev.sh shot`
  (DEVELOPING.md, under the snapshot states); the virtual device shows the real
  one (it runs below 60 fps on llvmpipe, so its strips are orange: the degraded case).
- **Device checks.** That the line is readable and calm on the panel at 0.82 and 0.6 opacity; that the strip turns
  orange on a real stutter (fast-forward Max on a heavy game, or a PSP game) and calms down 5 s later; that HOME + Y
  doesn't reach ROCKNIX or the game; core ms/frame and the exit report's `late` with the strip on and off on the
  device (expected: no difference).

### Pixel Path for PS2, GameCube, Wii and 3DS (`feat/pixel-path-standalone`, 2026-10-24)

The owner, on Burnout 3 in ARMSX2: seven rows "Managed by ARMSX2" said nothing. ARMSX2, Dolphin and Azahar tell pocket-ui
nothing while they run, but their settings files say how they draw, so the page now shows those values.

Files: `src/pixel_standalone.rs` (reading, the last reading per game, the stages; pure apart from the file reads,
unit-tested against `testdata/pixel-path/<armsx2|dolphin|azahar>`), `src/app_pixel_path.rs` (when it is read, the menu
item, the page), `UserEvent::PixelSetup` in `src/main.rs`.

- **What is read, per-game file first** (paths on the device; the sections and keys are the emulators' own, see
  [BIG-EMULATORS.md](BIG-EMULATORS.md#options-in-the-pause-menu-2026-10-22) for who writes them):

  | Emulator | Files | Keys |
  |---|---|---|
  | ARMSX2 | `/storage/.config/ARMSX2/gamesettings/<serial>_<CRC>.ini` (ARMSX2's own per-game INI, when the disc is known), then `/storage/.config/ARMSX2/inis/PCSX2.ini` (start_armsx2.sh writes system.cfg's game and platform values into it at every start; PINE changes from Options are saved to it) | `[EmuCore/GS]` Renderer, upscale_multiplier, AspectRatio, linear_present_mode, IntegerScaling, filter, fxaa, CASMode, CASSharpness, TVShader, ShadeBoost, MaxAnisotropy, VsyncEnable; `[EmuCore]` EnableWideScreenPatches; `[Framerate]` NominalScalar; `[SPU2/Output]` Backend, BufferMS, OutputLatencyMS, OutputLatencyMinimal |
  | Dolphin | `/storage/.config/dolphin-emu/GameSettings/<GAMEID>.ini` (the ID from the image header, `ID-<stem>` for DOL/ELF; where Options keeps its values; `Video_<section>` for GFX.ini's sections), then `GFX.ini` and `Dolphin.ini` (rewritten from `/usr/config` and system.cfg at every start) | GFX `[Settings]` InternalResolution (ROCKNIX's patch 004: half steps, 2 = 1× = 640×528), AspectRatio, wideScreenHack, MSAA, SSAA; `[Enhancements]` OutputResampling, ForceTextureFiltering, MaxAnisotropy, PostProcessingShader; `[Hardware]` VSync; `[Hacks]` VISkip; Dolphin.ini `[Core]` GFXBackend, EmulationSpeed, AudioLatency, AudioStretch; `[DSP]` Backend |
  | Azahar | `/storage/.config/azahar/qt-config.ini` alone: start_azahar.sh and pocket-ui write the game's values into it before every start and Azahar changes nothing while it runs; its own per-game properties can't be reached on the device (its menu bar is hidden) and aren't read | `[Renderer]` graphics_api, resolution_factor, texture_filter, filter_mode, use_vsync_new, frame_limit, pp_shader_name; `[Layout]` layout_option; `[Audio]` output_type, enable_audio_stretching. A key whose `\default` is true is Azahar's built-in default (known for resolution_factor 1, texture_filter 0, filter_mode true, use_vsync_new true, frame_limit 100, layout_option 0, output_type 0, enable_audio_stretching true); any other such key is left out |

  A value is shown only as the file has it, translated by the emulator's own enum (PCSX2 Renderer 14 = Vulkan, -1 =
  Auto, which is Vulkan on this GPU as measured); a number the reader doesn't know is left out, never guessed. PS2 games
  draw at sizes of their own, so ARMSX2's Source says "2× internal" and the explanation gives 640×448 → 1280×896 as an
  example; GameCube/Wii and 3DS show the size (640×528 and the 3DS's 400×240 top screen, times the multiple). Not seen
  by the files: a change made with Dolphin's own hotkeys while playing (they live in its CurrentRun layer only) and a
  PCSX2.ini key ARMSX2's Qt menu changed without saving.
- **Grades, only where a setting alone decides:**

  | Stage | Pristine | Faithful | Enhanced | Compromised | No grade |
  |---|---|---|---|---|---|
  | Core | | | | | always (how exact an emulator is can't be graded) |
  | Source | 0.9-1.5× the console's resolution | | ≥ 1.5×; a widescreen patch or hack | < 0.9× (Dolphin's 0.5×) | not set; window size |
  | Scale | | Auto; 4:3; 16:9 with the widescreen patch/hack; a 3DS layout that keeps both screens' shape (stacked, top only, large top) | | Stretch | 16:9 without the patch/hack (right for widescreen games, stretched for others); a custom shape or layout; not set |
  | Look | no effect, explicitly (a look key was read) | | FXAA, CAS, TV shader, shade boost, anisotropic filtering, MSAA/SSAA, a post-processing shader, forced or upscaled texture filtering (xBRZ, …) | textures forced to nearest | no look key in the files |
  | Display | | | | | always: the panel (`Screen`: the window's size, the output's refresh, 60 Hz without one), VSync, the limiter and Dolphin's VBI skip are shown, late pictures aren't measured |
  | Latency | | | | | always: "Not measured" |
  | Audio | | | | the emulator's output set to none | otherwise: backend → PipeWire, buffer, latency, stretching |

  The overall grade is the worst graded stage; the heading says **Partly graded** (`pixel_path::Path::partial`) in
  that grade's colour, ungraded stages have a grey dot. The line under the stages names the files read. Y (Pixel Path
  over the game) stays pocket-core's only.
- **Off the UI thread.** `pixel_standalone::prefetch` (called next to `emu_settings::launch` when the Curator switch
  is on) reads 10 s after the start, once the start script has rewritten the INIs; `App::pixel_item_at_open` starts a
  new reading on a thread each time the pause menu opens and decides from the last one (`pixel_standalone::listed`, a
  mutex and a clone, no file). The answer (`UserEvent::PixelSetup`) moves the item in or out, keeping the selected row,
  and updates an open page. Opening the page takes the last reading; nothing is read on the UI thread.
- **Hidden when nothing can be read.** A missing main file (PCSX2.ini, GFX.ini, qt-config.ini), one that isn't text,
  or one with none of the keys above: the menu doesn't list Pixel Path for that game, and the log says why once per
  change: `pixel path: not listed for ARMSX2: /storage/.config/ARMSX2/inis/PCSX2.ini is not there`. A garbled per-game
  file is left out (logged) and the base file used. RetroArch and the other emulators: `pixel path: not listed for
  RetroArch: pocket-ui doesn't read its settings` at each menu open.
- **Readability.** The game behind the Pixel Path page is darkened a little more than behind the rest of the menu
  (`app_pixel_path::PAGE_FADE` 236 against 225: 7 % of the picture's light left instead of 12 %). The owner found the
  small text over bright frames "kaum störend"; this is a small step, not a black page. `MenuShot` keeps both
  darkened copies, so going in and out of the page costs one blit the first time and copies after.
- **Snapshots.** `pixel-path:<armsx2|dolphin|azahar>[:<stage>[:about]]` and `menu:ps2[:<row>]:pixel=<armsx2|missing>`
  (DEVELOPING.md, under the snapshot states). The pause menu is dark in both appearances, so dark and light snapshots
  are the same.
- **Device checks.**
  1. Settings → Games → Curator → Pixel Path in the pause menu on. Burnout 3 (ARMSX2): HOME, the menu lists Pixel
     Path before Quick settings. Open it: Core "ARMSX2 · Vulkan", Source "2× internal" Enhanced (pocket-ui's default
     `ps2.internal_resolution=2`), Scale "4:3 · fit to 1240×1080 · …" Faithful, Display "1240×1080 @ 60 Hz · VSync off",
     Latency "Not measured", heading "Partly graded"; the bottom line "Read from PCSX2.ini." Compare with
     `grep -e Renderer -e upscale_multiplier -e AspectRatio -e linear_present_mode -e '^filter' -e fxaa -e TVShader /storage/.config/ARMSX2/inis/PCSX2.ini`
     and `grep -A6 'SPU2/Output' /storage/.config/ARMSX2/inis/PCSX2.ini`: every row matches the file, nothing more.
  2. Options → Resolution 3x, Aspect ratio Stretch, back to the game, HOME again: Source "3× internal", Scale
     "Stretch …" Compromised, heading orange. Set them back.
  3. Mario Kart: Double Dash (Dolphin): Source "1× · 640×528" Pristine with ROCKNIX's default 2; Options → Resolution
     2x → after the restart "2× · 1280×1056" Enhanced; the bottom line names `GameSettings/GM4E01.ini`.
  4. A 3DS game: Source "1× · 400×240" (or the game's Resolution), Scale "Stacked · …" Faithful.
  5. Hidden: `mv /storage/.config/ARMSX2/inis/PCSX2.ini /storage/PCSX2.ini.bak` while a PS2 game runs (ARMSX2 keeps its
     settings in memory), HOME: no Pixel Path in the menu (it may show for the first open and leave with the reading a
     frame later); `grep "pixel path" /var/log/pocket-ui.log` shows `pixel path: not listed for ARMSX2: …PCSX2.ini is not there`. Move
     the file back.
  6. A RetroArch game: no Pixel Path in the menu, log `pixel path: not listed for RetroArch: …`.
  7. Speed: `POCKETUI_TRACE` menu-open time unchanged (~24 ms) with Pixel Path on, for a PS2 game.
  8. Readability: the page over a bright scene (Burnout's sky) reads easily and doesn't look black.

## Settings → Curator (defaults)

| Setting | Default | Why |
|---|---|---|
| Lens & smart shelves | On | Costs nothing until used |
| Echoes on the game card | On | One row on the card |
| Up next after a game | Off | One line on the shelf after a game, only when something is alike enough; never starts anything; goes by itself (see "Curator 2 as built"). Off since 2026-10-25: for a few seconds A plays the suggestion, and A is the button still being pressed as a game ends. Enthusiast sets On |
| Merge versions in mixed lists | On | Only acts when two or more versions are owned |
| Stickers | On | Invisible until the first sticker |
| Games not on this device in Echoes | Off | Library first |
| Pixel Path in the pause menu | Off | For enthusiasts: the pause menu's page. The line over the game is Settings → Playing → Over the game → Pixel Path (2026-10-25), which the first start's Enthusiast and the Tour's "Pixel Path over every game" card set ([features.rs](../src/features.rs)) |
| Play history | On | Curator → History and the Year in review; no row until something was played, the review never opens by itself ([HISTORY.md](HISTORY.md)) |

## Build order

1. **Data (layers A and B) and engine (index, Lens, Echoes, Versions, Stickers) in parallel.** They
   meet in one adapter function, `curator::features_for(game)`, so neither waits on the other.
2. **UI for Home, Lens, card rows and Versions.** This comes after `feat/search` is merged, because
   both touch the shelf and Home.
3. **Pixel Path**, after `feat/run-ahead`.
4. **Motion polish** on the new screens (see `feat/motion`).

## Developer notes: what is built (2026-10-23)

Play history (Curator → History, the Year in review) has its own document: [HISTORY.md](HISTORY.md).

Lens, smart shelves, Stickers, Echoes and Versions are built, with Settings → Games → Curator, on the shipped data of
[CURATOR-DATA.md](CURATOR-DATA.md): gamedb (layer A), curatordb (layer B) and romfacts (the file name). Pixel Path is
built too (see "Pixel Path as built" above); its switch is the last row of Settings → Games → Curator. The
user guide is `docs/user/curator.md#curator`, `settings.md#curator`, `controls.md#in-the-lens`.

### Files

| File | What it is |
|---|---|
| `src/curator/features.rs` | **The adapter** `features_for(game, gamedb info, feel tags) -> Features`: facet values and the weighted terms of Echoes, all in the vocabulary's ids and labels. `FeelSource` brings layer B: `CuratorDbFeel` (the shipped file), `NoFeel`, the test fixture. The genre map (`Vocabulary::genre_tags`) gives every game with a Wikidata genre its family. Players from Wikidata's modes, era, D-pad and two screens by system |
| `src/romfacts.rs` | Regions, languages, fan translations, revisions and flags from file names (from `feat/curator-data`; Curator's own `romname.rs` was merged into it: `describe`, `language_name`, `region_name`) |
| `src/curator/bits.rs` | The bitset (one bit per game) |
| `src/curator/index.rs` | `Catalog` (data side: facet bitsets, IDF-weighted term vectors, works, series, QID → work) and `Index` (catalog + player: status, played, stickers, favourites, taste profile, preferred versions) |
| `src/curator/lens.rs` | `Lens` (facet id → value keys, serialisable), matches, live counts, merged lists, suggested names |
| `src/curator/echoes.rs` | Scoring, MMR, reasons; the constants of "Engine" above are `pub const`s there |
| `src/curator/versions.rs` | Works (gamedb's `work()`), the preferred version, `remakes` ("Related: remake"), `merge_versions` for mixed lists |
| `src/curator/prefs.rs` | `curator.json`: the switches and the smart shelves (with their order) |
| `src/curator/sort.rs` | `Order` (Name, Last played, Play time, Rating, Year, Length) and `Index::sorted` / `sort_games`: a list in an order, games without the figure last by name |
| `src/curator/surprise.rs` | "Surprise me": `Index::surprise` (weighted, seeded draw over works), `surprise_weight`, `surprise_reason`, the generator `Rng` |
| `src/curator/today.rs` | What Home's Curator row offers: `Index::today` (the day's pick, a Surprise me draw seeded by the day; the games waiting, `Waiting::Rediscover` or `NotStarted`), on the index thread |
| `src/home_panel.rs` | The picture side of Home's Curator, Favorites and pinned rows: what it shows (`curator`, `surprise`, `list`), where the covers go (`layout`), a line shortened by its parts (`fit_parts`), a title tile's lines (`title_lines`); pure, unit-tested |
| `src/app_home_panel.rs` | The App's side: the tiles from the index and the favourites, painting from cached covers only, the waiting games giving way to the line |
| `src/curator/fixture.rs` | Tests only: eight games of hand-made tags in vocabulary ids, for tests that should not move when curatordb is rebuilt |
| `src/curator/tests.rs` | A 76-game real-name library with the real gamedb and curatordb: Echoes quality, Lens, Versions, editions, remakes, the speed budgets |
| `src/curator_page.rs` | What a press does on the Lens, Echoes, Versions and the picker sheets (tested without drawing) |
| `src/app_curator.rs` | The App's side: the index thread, Home rows, the Curator list, the pages and their painting, the card rows, Settings → Games → Curator, `--snapshot curator:…` |
| `src/library_meta.rs` | `library.json` version 2: `stickers` (list) and per game `stickers` (ids), `preferred` (work → path) |
| `src/game_card.rs` | `CardData::rows`, focus on Curator rows, `Out::Row` |
| `src/shelf.rs` | `Item::{Curator, Lens, SmartShelf, Sticker}`, `Item::with_curator` |
| `scripts/curator-fake-library.py`, `scripts/curator-shots.sh` | The same 76-game library as the tests (versions, an edition, a remake, a port without data), and every screen dark and light |
| `examples/curator_check.rs` | Timings on about 5,000 real gamedb entries; with an es_systems.cfg, the report's Echoes and Lens examples |

main.rs only has hooks: the new `Item`/`Screen`/`SettingsPage` variants, one `UserEvent::Curator`, input and paint
dispatch for `curator.page`, `curator_tick()` in `about_to_wait`, `curator.touch()` after a session, and the
`curator:` snapshot; Curator 2 added three small hooks for Up next and one for the Cabinet's hints (below). The Cabinet look shows Curator as shelves
since Curator 2.

### How the data comes in

- **Feel tags.** `CuratorDbFeel::tags(system, info)` reads `curatordb::lookup(system, qid)`, or the work's tags for
  an edition filed as its own item (Chrono Trigger DS `Q507828`). Every tag keeps its shipped weight: 3 defining,
  2 clearly present, 1 minor (curatordb ships weight-1 tags only when the cross-check confirmed them). Length,
  session, difficulty and "lots of reading" become their vocabulary tags (`length-short` …) at weight 2. A game the
  labeller barely knew (confidence 1) counts at 70 %.
- **Family for every game.** Each Wikidata genre goes through `Vocabulary::genre_tags`: the family at 2.5, a
  sub-genre or setting the genre implies at 2.0. Games without tags (most below 3 sitelinks) still get a family,
  their sub-genre where the genre says it, series, developer and era.
- **Facets.** A tag on the family, sub-genre, mechanics, pace, session, length, difficulty, controls, mood, setting
  or multiplayer axis is a value of that Lens facet (multiplayer is the Players facet, next to Wikidata's Solo and
  Two or more). Keys are vocabulary ids ("arcade-racing", "couch-coop"), labels the vocabulary's ("Arcade racing",
  "Co-op on one screen"). Story, text and grinding tags are the Story, Reading and Grinding facets (Curator 2,
  below). D-pad-only systems give
  their games "Plays well on a d-pad", the DS and 3DS "Two screens", as facts of the hardware.
- **Echoes.** Terms are vocabulary tags only ("t:<id>"), so reasons read in the vocabulary's words: "crash
  takedowns", "futuristic racing", "co-op on one screen". Family tags are the plain terms reasons use last. Axis
  weights: length, session, difficulty, controls, text and grinding half, setting three quarters.
- **Versions.** Grouped by gamedb's `work()`: the same QID on several systems, and editions or ports filed as their
  own item. Remakes are their own work; `Index::remakes` finds owned remakes through gamedb's `related`
  (`RemakeOf` / `RemadeAs`, looked up from either side). They show on the Versions page as "Related: remake" (A plays,
  Y can't prefer them), make the card's Versions row appear even with one version, and are left out of Echoes.
- **File names.** `romfacts::parse`: the Language facet uses `languages_or_implied()` plus "Fan translation"; the
  preferred version without a choice prefers `readable_in("en")`; the Versions page shows `RomFacts::describe()`.

### How the index is built

`App::curator_tick` (from `about_to_wait`) starts a build when the library was read again or the player changed
something (`curator.touch()`: status, star, sticker, preferred version, a session), never before the first frame,
never while a game runs, never during the first start. The UI thread only copies paths, names and `library.json`'s
games; the thread (nice 10 on Linux) matches every file in its own `GameDb`, reads curatordb (each system unpacked
once), makes the features and the index and sends `Event::Built`. A rebuild after a change reuses the catalog (only
the player's part is new). Until the first index arrives, Curator's rows say "Getting ready…".

Measured with the shipped data (`cargo run --release --example curator_check`, `cargo test --release curator`; M1
Max with the host under load, so take the larger numbers; on the Mini V2 the main session measured 332 games at
136 ms for the catalog, before curatordb):

| | Budget | Measured |
|---|---|---|
| Catalog, 4,984 real gamedb names on 32 systems (gamedb matching + curatordb tags, every system unpacked) | background thread | 145 ms |
| Index for those 4,984 (taste, user facets, works) | background thread | 0.4 ms |
| Lens keypress (count, a facet's counts with merged versions, the picks lines) | < 1 ms | 50 µs |
| Echoes for one seed among 4,984, 64 seeds | < 5 ms | median 0.36 ms, worst 0.52 ms |
| START: the merged list of 5,000 | < 16 ms | 1.1 ms |
| Catalog of the 76-game test library | background thread | 65–87 ms (unpacking the systems dominates) |

### Choices made while building

- **Echoes reasons.** The two largest parts of the score, with an order of preference: the series first; then a
  particular tag, the developer or taste, by size; the era; a family ("racing") only when nothing more particular is
  left. "Like X" names a game of the taste profile only when it is at least 0.5 alike, at most twice per list, and
  one line is not said a third time while another part has something to say. Status adjustments change the order
  but are never a reason. Long studio names are shortened ("Nintendo EAD").
- Below a score of 0.1 (before taste and status) a game is not an Echo, so a small library gives fewer than 12, and a
  game without data (a PortMaster port) has none.
- **Counts.** With merged versions on, the Lens's header and every value's count are games as the list will show
  them, not files.
- **Values that read the same** are one Lens value; every key finds it, so saved shelves keep working.
- **Preferred version without a choice:** most play time, then last played, then a version readable in English (a
  fan translation counts), then library order.
- **Mixed lists** show the preferred version when it is in the list, else the list's own entry: a shelf filtered to
  the DS shows the DS version, Favorites the file that was starred.
- **Not on this device** (off by default): games of the seed's series or developer on the library's systems, from
  gamedb, best known first, six names; worked out on a thread when the Echoes page opens.
- **Curator list.** Home's Curator row opens a list in the current look: Lens, smart shelves, stickers. Y pins, X
  opens a small options sheet (change the filters, rename, Home, delete). A new smart shelf is pinned.
- **Data files.** Stickers and preferred versions in `library.json` (version 2; version 1 reads, and the old reader
  still reads version 2, tested). Smart shelves and the switches in `curator.json`. Shelves saved before the wiring
  with old keys ("role-playing") find nothing for that facet; the families' keys are now the vocabulary's ("rpg").

### Examples with the shipped data

`cargo run --release --example curator_check -- <es_systems.cfg of scripts/curator-fake-library.py>` (no player data,
so no taste):

| Seed | Echoes (first five) |
|---|---|
| Burnout Revenge | Burnout 3 "Same series · crash takedowns", Burnout Legends, Burnout "Same series · racing through traffic", OutRun 2006 "Racing through traffic · arcade racing", Ridge Racer (PSP) "Arcade racing · adrenaline rush" |
| Chrono Trigger | Chrono Cross "Same series · also by Square", Final Fantasy VI Advance "Also by Square · active-time battles", Ocarina of Time "Time travel · epic", Phantasy Star IV "Active-time battles · party building", Secret of Mana |
| F-Zero: Maximum Velocity | F-Zero, F-Zero GX "Same series · futuristic racing", F-Zero X, Wipeout 2097 "Futuristic racing · adrenaline rush", Burnout Legends "Adrenaline rush · frantic pace" |
| Super Mario All-Stars | Super Mario World, New Super Mario Bros. "Same series · 2D platforming", Super Mario 64 "Same series · also by Nintendo EAD", Kirby's Dream Land "2D platforming · cute", Super Mario Kart |
| Mario Kart: Double Dash | Mario Kart 64, Mario Kart DS "Same series · kart racing", Super Mario Kart "Same series · versus on one screen", Crash Team Racing "Kart racing · car combat", F-Zero X |
| Celeste Classic (port) | none: no gamedb entry, no tags |

| Lens | Games |
|---|---|
| GBA · Action · short sessions | 2: Astro Boy: Omega Factor, Mega Man Zero (no GBA game of the library is under 5 hours) |
| Co-op on one screen | 3: Gunstar Heroes, Secret of Mana, Streets of Rage 2 |
| Racing · versus on one screen | 10: Crash Team Racing, F-Zero GX, F-Zero X, Gran Turismo 4, Mario Kart: Double Dash, Mario Kart 64, OutRun 2006, R4, Super Mario Kart, Wipeout 2097 |
| Cosy | 1: Animal Crossing |

### Snapshots

```sh
python3 -I scripts/curator-fake-library.py /tmp/fake dark      # or light
POCKETUI_MOCK=1 POCKETUI_ES_SYSTEMS=/tmp/fake/es_systems.cfg POCKETUI_DATA=/tmp/fake/data \
  target/release/pocket-ui --snapshot curator:echoes:ps2:revenge out.png
scripts/curator-shots.sh            # every screen, dark and light, into screenshots/curator
```

States: `home[:<look>[:<row>]]` (Home on the Curator row, or `favorites`, the first pinned `shelf` or `sticker`, `nofavs`: a
Favorites row with no games; covers are decoded on the spot), `hub[:<look>:<row>]` (in that look on that row, 0 the Lens), `options[:<keys>]` (`options:da`: the
shelf's order), `shelf:<i>`, `lens[:<keys>]` (`S` SELECT, the order sheet; `s` START), `save[:<keys>]`, `ready[:lens]` (before the
index), `card:<system>:<name>[:<downs>|:<keys>]` (keys `y d r l`: `card:ps2:revenge:yddddrr` is the picker on My rating; `f` and `c` make the open card's rows again as a new index and the sync conflicts' scan do),
`surprise[:<seed>[:<n>]]` and `surprise:empty` (Surprise me's card with a fixed seed, `n` more picks; before the index), `echoes:…`, `versions:…`, `stickers:…`, `settings[:<row>]`, and since
Curator 2 `upnext:<system>:<name>[:<look>]` (the shelf after that game ended), `cabinet:<keys>` (the Cabinet look
after r l u d a b) and `cabinetlens:<lens keys>` (the Lens's games as a cabinet shelf). On the
virtual device: `python3 -I scripts/curator-fake-library.py ~/.cache/pocket-vdev/cur-fake dark /storage/roms`, start
with `VDEV_ROMS=~/.cache/pocket-vdev/cur-fake/roms` and copy `data/*.json` into `/storage/.config/pocket-ui/`.

## Curator 2 as built (`feat/curator-2`, 2026-10-23)

Four additions, each behind Settings → Games → Curator like the rest. The user guide is `docs/user/curator.md` ("Lens",
"Echoes", "Up next"), `playing.md` (the Cabinet), `controls.md` and `settings.md#curator`.

### Lens: Story, Grinding, Reading and Length ranges

The three axes that were in curatordb but had no facet are facets now (`Facet::{Story, Grinding, Reading}`), so
"PS1 · story · little grinding · German" is a Lens: System, Story, Grinding and Language picked together.

| Facet | Values (key) | From |
|---|---|---|
| Story | Story-driven (`story-driven`, weight 2–3), Some story (`story-some`, a weight-1 tag), Cinematic (`cinematic`), Rich world and lore (`lore`), Little story (`story-little`) | the story tags; Little story: a game the labeller knew without any story tag |
| Grinding | Grinding (`grinding`), Not known for grinding (`grinding-not-noted`) | the grinding tag; the second value: a game the labeller knew without it |
| Reading | Little reading (`text-light`), Lots of reading (`text-heavy`) | `GameTags::text_heavy` false / true |

- **Honesty rules** (`features::feel_tags`). The "little" values exist only for games the labeller knew
  (confidence 2 or 3) and, for Reading, only where text-heavy was answered (`Some(false)`; 261 entries say nothing).
  A game without data is in none of a facet's values, so a "little" value never means "unknown". They are Lens values
  only: a `FeelTag` of weight 0 makes a facet value and no Echoes term.
- **Wording follows the data.** Grinding is tagged on only 193 of 13,412 entries (not on every grindy RPG: Pokémon
  Emerald has none, Crystal has), so its other value says "Not known for grinding", not "No grinding". Story has
  tags on about 1,760 entries, so "Little story" is fair for a known game without one. A weight-1 story tag (shipped
  only when cross-checked) is "Some story"; weights 2 and 3, also at 70 % for a barely known game (1.4), are
  "Story-driven" (`STORY_DRIVEN_FROM` 1.2).
- **Length** stays the shipped buckets (< 5, 5–15, 15–40, 40+ hours, endless); the data is good to about one bucket
  (CURATOR-DATA.md: 69 % on the golden set), so nothing finer is pretended. Picks of neighbouring steps read as one
  range in the suggested shelf name and the picks line (`lens::length_range`: "under 15 hours", "5–40 hours",
  "15+ hours"), which is how "under about 8 hours" is asked. `Facet::note` puts one line under the values of
  Length, Story, Grinding and Reading saying this.
- The doc check `every_lens_filter_is_described` also wants every value of `Facet::fixed_labels` (Length, Story,
  Grinding, Reading) named in the user guide's "Lens" section.
- Cost: the Lens keypress with 21 facets is 24 µs (90th percentile, 5,000 games; budget 1 ms).

### Up next

Roon Radio goes on after the album; Up next does the same for a game, but only suggests.

- **The cue** (`App::curator_watch_game`, called from `curator_tick`): the running game is remembered at every tick.
  When `running` is empty again, the game ran for at least 60 s (`UP_NEXT_AFTER`) and no notice is on the shelf
  (a failed start), its path becomes the seed. Nothing in `game_exited` or the quit path was touched: a game started
  again (options, a fallback to another emulator) or handed over by the switcher never leaves `running` empty, so
  it gets no suggestion; PC streaming and tools are apps and get none. Switched off, nothing is remembered.
- **The work** happens on the index thread: the session already makes a new index (`curator.touch()`); that build
  takes the seed and runs `Index::up_next` after the index (one Echoes list: 0.33 ms median on 5,000 games), and
  `Event::Built` brings it along. The shelf is back before any of it; the suggestion appears a moment later.
- **The rules** (`echoes::Index::up_next`, tested in `up_next_is_the_first_echo_you_might_play_next`): the first of
  the game's Echoes (so never its own versions, its remakes or a dropped game) that isn't completed or dropped,
  wasn't played in the last 24 h (`UP_NEXT_RECENT`, so two games don't take turns) and scores at least 0.3
  (`UP_NEXT_MIN`). Otherwise nothing.
- **Showing it** (`App::paint_up_next`): only while the shelf is on screen with nothing over it (`up_next_room`:
  no card, page, menu, switcher, search, settings, notice). A paper label over the bottom of the shelf in every look
  ("Up next", the box, the name, "PSP · Same series · …"), the shelf fading out under it, and the hint row says
  "A Play it · B Not now" instead of the shelf's. It slides in like a toast (`motion::paint_toast`) and goes after
  8 s (`UP_NEXT_SHOWN`); the loop wakes for that (`curator_up_next_wake`), no frames are drawn in between.
- **Input** (`App::curator_up_next_press`, the first line of `App::press`): A starts the game, B puts it away, any
  other button puts it away and goes on as usual. A tap on the label is A. It never starts anything by itself.
- **Default: off** (since 2026-10-25; it was on). It takes no room until a game ends and says nothing when nothing is
  alike enough, but for those few seconds A means "play the suggestion", and A is the button most likely still being
  pressed as a game ends (the same hazard the 600 ms wake guard in `sleep.rs` exists for). So it is an opt-in: the
  first start's Enthusiast and the Tour's card turn it on. `Prefs::up_next` has `#[serde(default)]` = false, so a
  `curator.json` that has the key keeps its value and only a file from before the key existed gets Off. Settings →
  Games → Curator → Up next after a game (`SETTINGS_ROWS`, row 3).
- **main.rs hooks** (a few lines each, commented): `App::press` (input), `paint_home` (instead of the shelf's
  hints), `about_to_wait` (`curator_up_next_wake`: one frame per refresh while it slides in, then the wake-up when
  it goes). The Cabinet's "B Back" on a Curator list is one more condition in `paint_hints`.

### Curator in the Cabinet look

The cabinet had fixed shelves (Continue, Favorites, systems); Curator now has shelves of its own between Favorites
and the systems (`cabinet::ShelfKey::{Curator, Lens, SmartShelf, Sticker}`, `Cabinet::build`'s `curator` list,
`App::cabinet_curator_keys`):

- **Curator**: a box (a DVD-size case in Curator's colour, a sticker's in the stickers' colour) for the Lens and for
  every smart shelf and sticker, as the Curator list has them; Y pins, X opens the options sheet, A opens the Lens or
  the list.
- **The list open from there** (an unpinned smart shelf or sticker, the Lens's games after START) is a shelf right
  under it while it is open, with its name on the plate; B goes back to the Curator shelf or the Lens (the hints say
  "B Back" then). Moving to another shelf with the D-pad leaves it, and the back trail is dropped.
- **Pinned smart shelves and stickers** are shelves all the time, as they are rows on Home. A sticker's shelf has the
  sticker itself on the board's edge instead of the brass card holder.
- The keys are compared every frame (cheap: prefs, stickers, the screen); a change makes the shelves again. A new
  index marks them dirty too. A list with no games doesn't open in the cabinet ("No games here right now"), since an
  empty shelf has no case to stand on.
- A game on two shelves (a smart shelf and its system's, also Favorites and its system's before) came out of both at
  once: `Pull` follows the item, not the shelf. The cabinet now remembers the shelf of the case out and of the one
  going back (`out_on`, `back_on`), so only those come out.
- Box pictures are cached by a made-up path per box and name (`curator/<item>/<name>`), so boxes don't share one
  picture and a renamed shelf gets a new spine; they have no box art.

### "Why these?" on the Echoes page

Every Echo carries `lift`, what taste added to its score (`before × TASTE_FACTOR × affinity`). `Index::why` says
something only when taste changed the order of the shown list: a game listed above one that would be above it
without taste. Then it names the taste games behind those lifted games (`closest_taste_game`, the same as for
"Like X" reasons, at most two): "You finished Burnout 3 - Takedown", "You starred Chrono Trigger". When taste changed
nothing (Burnout Revenge's list) or no game of the profile is alike enough to name, the line counts the games as
before. 14 µs for a list of 12; it is worked out when the page opens.

### The card's Curator rows by touch

`game_card::Card::row_areas` gives the rows' places on screen as last painted (cut to the scrolling body), and
`focus_row` moves the selection there. `App::curator_card_hits` registers them after the card is painted and
`App::curator_card_tap` (from `app_touch`) does what taps do in lists: the first tap selects a row, a tap on the
selected one opens it as A does.

## Sort orders, My rating and Surprise me (`feat/lens-sort-rating-surprise`, 2026-10-25)

Gaps #5 and #6 of the 25 October round ("no sort orders and no personal rating", "no pick for me"). The user guide is
`docs/user/library.md` ("My rating") and `docs/user/curator.md` ("Lens", "Smart shelves", "Surprise me"), `controls.md` and `web-companion.md`.

### My rating

- **Kept** in `library.json` next to status and note: `GameMeta::rating`, 1 to 5 (`RATING_MAX`), 0 for none, written only
  when there is one (`skip_serializing_if`), so the file stays version 2, a file without ratings is byte for byte as
  before, and an older pocket-ui reads it (tested). `LibraryMeta::set_rating`; the web companion's
  `POST /api/game/rating {id, rating}` (0 takes it away; anything above 5 is refused, not cut) through `Edit::Rating`.
- **Set** on the card with the status picker (Y): a row **My rating** under the five statuses and above "Wrong game?
  Choose another" (`game_card::RATING_ROW`), a star where the statuses have their marks and five stars at its right
  end; on that row Left / Right give fewer or more stars (elsewhere they move like Up / Down, as before), A keeps them
  (`Out::SetRating`), B leaves them. The picker's title is "Status and rating". The hint bar on the row says "LEFT
  RIGHT Stars". No new button on the card: Y was already "what I think of this game".
- **Shown** under "Your game" as a row **Rating**: five stars of 13 px radius, the given ones in the text colour,
  the rest a dim mix of Dust and the page (`game_card::paint_stars`, which draws `switcher::star`). Not gold: gold is the
  Favorites star, and a rating is not a favourite. Without one the row says "None. Press Y" in Dust, as the note does.
  The label at the top and the shelf stay as they were (calm: the rating is on the card, not on every spine).
- **The Lens facet** `Facet::Rating` ("My rating", id `rating`, after Status): values "5 stars" … "1 star" (keys "5" …
  "1", only those someone has) and "Not rated" (`unrated`), which is there even before the first rating, so the facet
  and its note ("Your own stars. Rate a game on its card: Y, then My rating.") explain themselves in an unrated
  library. Shelf names: "rated 5", picks from 5 down without a gap read "rated 4 and up" (`lens::rating_range`).
- **Taste**: `index::taste_weight` adds (rating − 3) × 0.5: five stars count like a Favorites star, one star like
  dropping the game; 0 (none) adds nothing, so a player who never rates sees no change.

### Sort orders

- `sort::Order`: **Name** (the old order, by name then system), **Last played first**, **Most played first**, **Best
  rated first**, **Oldest first** (gamedb year), **Shortest first** (Curator's length bucket, Endless after 40+ h).
  Each puts the games without its figure last, in name order (stable sort after `Index::list`), so a game is never
  left out. With merged versions a game stands for its work: the latest last play, the sum of the time, the highest
  rating of its versions (`Index::played`).
- **What data supports**: last played (recent games), play time (library.json and recent games, the larger),
  rating, year (Wikidata, 17,000+ entries), length (curatordb, about 9,300 known games, good to about one bucket).
  Not offered: review score (too few entries and not "mine"), date added (the library has no reliable one), sessions
  (Most played says it better).
- **Where**: SELECT on the Lens opens a sheet of the six (`LensOut::Order`, `Page::Order`, `Sheet::choice`); START shows
  the games in it; X saves it with the shelf (`Purpose::SaveShelf { order }`). Each smart shelf keeps its own
  (`SmartShelf::order`, written only when it isn't Name) and its options sheet has **Order: <current>**, which saves at
  once. The Lens of the session keeps its order like its picks (`CuratorUi::order`); editing a shelf in the Lens uses
  and saves the shelf's order. Stickers stay by name. The list's title and the shelf's row say the order when it isn't
  Name ("Racing · most played first"); the Lens says it after its picks line.
- **Speed**: only the list is sorted, after the Lens has picked it: the Lens's keypress (bitsets, popcounts) doesn't
  change. Measured with the 5,000-game speed test (`speed_index_build_lens_keypress_and_echoes_for_5000_games`, host
  under load): Lens keypress 43 µs (90th percentile, 22 facets), START by name 3.3 ms, START in the slowest order
  4.4 ms (budget 16 ms). The web companion lists a shelf in its order too.
- The letter jump (L2/R2) still goes to the next first letter in a list in another order; it is harmless there and
  not worth a special case.

### Surprise me

- **Where, and why there.** An entry **Surprise me** right under the Lens in the Curator list (`Item::Surprise`), and so
  a box on the Cabinet's Curator shelf. Home stays as it is (no new row for everyone, "simple by default"); finding
  something to play is what the Curator list is for, and it is two presses from Home. It follows the Lens & smart
  shelves switch like the rest of that list. Its row says what it picks from ("From 70 games"; "Getting ready…"
  before the index, "No games yet" with none), never a blank in Split's panel.
- **The draw** (`Index::surprise`): every work once (its preferred version), weighted by `surprise_weight` = status ×
  recency × rating × taste: Not started, Playing, Paused 1, Completed 1/6, Dropped 1/20; played in the last day 0.1,
  week 1/3, month 2/3, longer or never 1, an unfinished game left for over a month 1.3; 4–5 stars 1.3, 2 stars 0.4,
  1 star 0.2; with a taste profile × (1 + affinity), at least 1/3. The games shown since the entry was pressed are
  left out (`CuratorUi::surprise_seen`) until all were, so Right brings something new. A small SplitMix64 generator
  (`surprise::Rng`) is seeded from the clock at the first pick; tests and `--snapshot curator:surprise:<seed>` seed it,
  so the same seed gives the same games. One pick among 5,000 games: about 0.1 ms on the UI thread (budget 5 ms).
- **The reason** (`surprise_reason`) says what is true of the pick, not the weights: when it was last played ("Never
  played", "Played 3 days ago", "Not played for 3 months", "Not played for 2 years"; "Started, …" for Playing or Paused,
  "Finished, …" for Completed), then a rating of 4 or 5 ("you gave it 5 stars") or, when the taste profile leans to
  the game (affinity ≥ 0.2), its family or sub-genre the profile likes most ("you like arcade racing games", "you like
  Japanese RPGs"). Without a profile (nothing played, rated or starred) the second part is left out.
- **Showing it**: the picked game's ordinary card (`App::show_card`) with `CardData::surprise` set: one line over the
  two columns, "Surprise me" in Dust and the reason in amber (the card's colour for suggestions, as "Playing? Press
  Y"). Right on that card asks for another (`Out::Another` → `App::curator_surprise`); the hint bar puts "RIGHT
  Another" after Play and Back and leaves out START (Emulator) to keep the row short; START still works. B goes back
  to the Curator list. Before the index or with no games, A on the entry says so in a toast and stays.
- **Not done**: Surprise me inside an open Lens (picking among its games) was considered and left out: one entry is
  calmer, and the Lens's START with "Last played first" or a rating already answers most "what next" questions.

## Curator on Home (`feat/curator-forward`, 2026-10-26)

The owner, playing on the device: Curator "could stand out a bit more, it's a bit hidden", its row showed "just an empty
field" where a game shows its box art, and Favorites should show the games' pictures. User guide: [user/playing.md](user/playing.md#home),
[user/curator.md](user/curator.md#curator).

- **Where the row is.** The Curator row is the **top row of Home** in the list looks (Shelf, Minimal, Split, Cards):
  Curator, Search, Continue, the other recent games, Favorites, the pinned smart shelves and stickers, the systems
  (`shelf::Item::with_curator(…, top)`, `with_search` puts Search under it, `first_row` skips both). Home still opens
  on Continue, Search is still one press up, and nothing the player uses every day moved: the recent games, Favorites
  and the systems keep their places; only Curator came up (it was the row after Favorites, up to seven presses down
  and often off the screen). At rest the Shelf look's selection is its third row, so Curator and Search are the two
  spines above Continue: Curator is seen every time Home opens. Considered and left out: a tab or pill beside Search
  (the top line is the place's name and the status corner; Left/Right already move along the list, so a second
  axis would be new for one entry), and a row of its own above Continue that Home opens near (it would move Search).
  The Cabinet keeps its order (Continue, Favorites, Curator, systems): its Curator shelf of tall boxes in Curator's
  colour is already a place of its own, Up on its top shelf is the search, and Continue has to stay the shelf it
  opens on.
- **What the row says.** The spine's end says what Curator has for you ("Today: Super Metroid"; "Getting ready…";
  the shelf and sticker counts with no games). Pulled out, the label has "Your library by feel" over the name, as
  "Continue" is over the last game: four words for what Curator is, for a player who never opened it.
- **The panel** (`home_panel`, painted by `app_home_panel`) where a game's label has its box art, 176 px high
  whatever the text size, against the label's right end, at most 600 px wide (`PANEL_W`), and in Split's panel
  (260 px high, centred; the line under the name takes two lines there):
  - **Curator**: the cover of the day's pick with "Today" under it, then a fan of up to three of the games waiting
    ("4 to rediscover": started, not finished, not played for 30 days; with none of those, "12 never played", the pick counted too, as the Lens's Played → Never counts),
    each a little behind the one before. The line under the name: "Today: Super Metroid · Never played · you gave it
    5 stars", the reason being Surprise me's (`surprise_reason`). A line too long for its room loses its last
    " · " parts first (`fit_parts`, two parts stay), then is cut with "…"; and before the pick's reason would go, the
    fan gives way (`panel_make_room`: Large text, German).
  - **Favorites, pinned smart shelves and stickers**: the first five games' covers (as many as fit; three to four on
    the label) side by side, standing on the label's foot as cases on a shelf, in the list's order
    (`CuratorUi::shelf_tops`, `sticker_tops`, made with the counts in `curator_counts`, pinned ones only).
  - **Missing data, never a blank.** A game without box art is a tile in its system's colour (darkened, as a case
    without a cover in the cabinet) with its name in whole words (`title_lines`; a word too long is cut with "…"),
    at the design's 21 px in every text size, as it stands in for a picture. Behind another in the fan only its edge
    shows. A cover still being decoded is a faint tile. Before the index: three faint tiles and "Getting ready…".
    A list with no games says how to fill it ("Press Y on a game to star it, and it shows here"; "No games here right
    now"; "Put it on a game from its card: X, then Stickers"). The Favorites row itself is still only on Home while a
    starred game is there, so its empty line shows only where that can happen (a snapshot state, `nofavs`).
  - **Minimal and Cards** have no picture side for these rows and are as before (Cards shows the spine's line, now
    "Today: …", large on the card).
- **Surprise me** shows the day's pick first, once a session (`CuratorUi::today_shown`, reset when a new index brings
  another pick), so the game the row names is the game A finds; its row in the Curator list shows the pick's cover
  and line, and in the Cabinet the Surprise me box has the pick's box art on its front (`App::surprise_front`; its
  spine stays Curator's, its picture key has the pick in it so a new pick makes a new front).
- **The day's pick** (`Index::today`): `Index::surprise` with a generator seeded by the day (`now / 86 400`, UTC), so
  the same game all day and after a restart; it changes at midnight or when the player's state moves the weights
  (after a session the index is built again: playing the pick usually makes it another). The waiting games are counted
  per work (preferred version), sorted by `surprise_weight`, the pick left out so no cover shows twice.
- **Cost.** Nothing new at start: Home opens on Continue, and the first frame paints no panel (its rows are spines).
  `Index::today` runs on the index thread with each new index (one draw and one pass over the works; budget 5 ms for
  5,000 games in `speed_index_build_lens_keypress_and_echoes_for_5000_games`). Painting a panel asks only memory: the tiles come from `curator.today` and
  the favourites' list, which file a game's box art is is looked up once per game (`App::cover_file`, the cabinet's
  cache: a few `stat`s the first time), and covers come from the image cache; one not decoded yet is asked of the
  picture loader's thread (`App::picture`, the one Split's preview uses) and arrives with a redraw. Unlike a game's
  label, which decodes its box art on the UI thread the first time, a panel never decodes on it. Navigation over
  these rows costs what any label does plus at most five cache look-ups; `docs/latency/device.json`'s move and open
  figures are not affected by construction (not measured on the device by this branch).
- **Snapshots**: `curator:home[:<look>[:<row>]]` (above); `curator:cabinet:ddr` is the Surprise me box. In
  `tests/i18n-states.txt` (German and Large).

## Connections and Rediscover (`feat/connections`, 2026-10-30)

What a game's credits say leads to the player's own games with the same name: the developer, publisher, series and
genre on the info page (`game_info`, Minimal, Cards, Split) and on the box's back (`game_box`, Shelf, Cabinet) become
selectable, and A opens "More from Criterion Games", "More published by Electronic Arts", "Burnout series, in release
order" or "More racing games" as a shelf of the player's games (`Screen::Connection`, every look; in the Cabinet a
shelf of its own with the title on its plate). The user's side is [library.md](user/library.md#connections) and
[curator.md](user/curator.md#rediscover).

- **Data** (`app_connections::entries`): the catalog's games with Wikidata's developers, publishers, series and
  genres (`Features::publishers`, `genres` were added for it) and what the EmulationStation gamelist adds (developer,
  publisher, genre, year): both, so "Criterion" from a scraper meets "Criterion Games" from Wikidata.
- **Matching** (`connections::key`): lower case, letters and digits, "&" as "and"; a company's trailing words
  ("Games", "Studios", "Inc.", "Ltd.", "Entertainment", "Digital"…) dropped, so "Criterion", "Criterion Games" and
  "Criterion Studios" are one; a value that is only such a word ("Ltd." from "Square Co., Ltd.") matches nothing; a
  genre's "video game" dropped and a few spellings joined ("Platformer"/"platform game", "Race"/"Driving"/"racing").
  Values are split at "," "/" ";" (the card joins two, a gamelist writes "Racing, Driving").
- **Series from titles** (`connections::title_stem`, `derive_series`), only where the data has none, conservatively:
  the words before a number (arabic, or a roman II–XV) or a subtitle (":", " - ") are a stem; a stem is a series when
  two *different* titles share it ("Burnout 2 - Point of Impact", "Burnout 3 - Takedown"; the same title on two
  systems is one). A title that is the stem itself ("Burnout") also takes the titles that start with it and a word
  ("Burnout Revenge"). Stems shorter than three letters and a few plain words ("Super", "Final", "Dark"…) never are.
  The card then shows **Series** with it (`card_connections`).
- **Never an empty list**: `Index::link` returns a list only when another of the player's games has the value (for a
  series another title); otherwise the fact is plain text, no arrow, not selectable. A game without facts has
  nothing to pick: Down scrolls, A plays. A list whose games went while it was open says "No games here".
- **Order**: series and developer/publisher by year (unknown last), then name; genre by name. A series lists this game
  too; the others leave it out. Mixed systems merge as Curator's other lists (`curator_merge`, Settings → Curator →
  Merge versions).
- **Input**: the info page's Down steps through the linked facts (`InfoView::link_step`) before the description
  scrolls, Up scrolls back first and leaves the first fact to none (A plays again). On the box's back Down picks a
  linked fact in view, as the card's Curator rows are picked (`BoxView::link_step`; turning over lets go). A on one is
  `Out::Connection(i)`; `open_connection` closes the card, remembers screen, selection and card (`ConnectionsUi::back`,
  eight deep), and B on the list (`connection_back`, also the Cabinet's B) restores them and opens the card again.
  The return screen uses the system name and the card uses its system and ROM path, so a rescan can reorder both.
  Open and nested lists follow the new library by path; a removed originating game returns to Home without a card.
- **Looks**: `kit::link_value`, the value with a small drawn arrow, the selected one on a pill (the label's paper and
  ink on the page; ink and the back's paper on the box), cut with "…" to stay in its column.
- **Cost**: nothing at menu open; a card's open looks each fact up (`Index::link`: a hash lookup and the list's sort).
  5,000 games: the index in 11 ms, four lookups 0.9 ms on the build computer while other builds ran
  (`speed_of_the_index_for_5000_games`; the series from titles are one pass over the titles' word boundaries; the
  first version compared every base title with every title, 73 ms for 335 games on the Mini V2). The index is built on
  Curator's catalog thread with each new catalog (the library read again) from data the catalog already resolved, and
  again on the same catalog when the gamelists change (`ConnectionsUi::stale`: a few milliseconds, not a new catalog;
  at the start the gamelists usually arrive while the first catalog is being made).

**Rediscover** (`Item::Rediscover`, `Screen::Rediscover`): in the Curator list after the smart shelves while it has a
game. `connections::rediscover` on the index thread with every index: the games played a year ago today (local date,
the 28th for 29 February; a play-history session that day, or the last time played), sorted by time played, then the
games last played more than 365 days ago, longest played first. "Last played" is the newest of the recent games, the
play history and an EmulationStation gamelist's `<lastplayed>` (`gamelist::Entry::last_played`, read as UTC; the
gamelist cache's `VERSION` went to 2 so the field is read once). The shelf's line says why ("played a year ago today",
"last played 412 days ago").
Synced `history.json` session starts and durations are u64 values: Rediscover saturates both session ends and total
play time, so damaged records cannot wrap into old dates or reverse the play-time ranking. Future last-played
values are skipped before signed calendar arithmetic; session-day candidates already come only from the narrow
year-ago window in `rediscover_input`.

**Tests**: `connections::tests` (matching, stems, series, order, Rediscover's dates), `game_card` (the info page's and
the box's picking), `tests/connections.rs` (journeys with the real binary: the developer's list without the game itself
and with one game on two systems once, the series in release order, B back to the card, the box's back, every look at
1280x720 and 640x480 in English Normal and German Large with no text cut or overlapping, a homebrew game with nothing
to pick, Rediscover only with data and a year ago today first). Snapshots: `info:<look>:ps2:revenge:dddd` (the series
picked), `…:dddda` (its list), `info:shelf:ps2:revenge:rd` (the box's back), `curator:rediscover[:<look>]` with
`POCKETUI_REDISCOVER=1 scripts/curator-fake-library.py`.

