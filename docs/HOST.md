# pocket-core: our own libretro host

## What libretro is
A libretro *core* (e.g. `gambatte_libretro.so`) is an emulator without a user interface; a *frontend*
loads it and handles picture, sound and input. RetroArch is the big official frontend, NextUI's
`minarch` a small one, pocket-core ours. All use the same cores.

## Scope
- Software-rendered systems first. On pocket-core from the shelf: GB/GBC (gambatte), GBA (mgba), NES (nestopia),
  SNES (snes9x) via `HOST_CORES` in `src/main.rs`, and every *verified* system of `src/hostsys.rs`
  (table below).
- N64, PSP, Dreamcast (OpenGL, see "3D cores" below) and NDS run on pocket-core since the second pass
  ("N64, PSP, Dreamcast and NDS on pocket-core" below): at least as good as ROCKNIX's default emulator in
  every measurement, with the pause menu, save slots and picture options on top.
- Sega Saturn runs on pocket-core with ROCKNIX's `yabasanshiro` core when a Saturn BIOS is in `roms/bios`; without
  one it stays on standalone YabaSanshiro ("Sega Saturn" below).
- DOS (`dosbox_pure`), ScummVM (`scummvm`), PICO-8 (`fake08`, unless the official PICO-8 is installed) and TIC-80
  (`tic80`) run on pocket-core ("DOS, ScummVM, PICO-8 and TIC-80" below).
- Famicom Disk System (`nestopia`, needs `disksys.rom`), SuperGrafx (`beetle_supergrafx`), Virtual Boy (`beetle_vb`)
  and ROCKNIX's second folders of the HOST_CORES consoles (`famicom`, `nesh`, `sfc`, `snesh`, `gbh`, `gbch`, `gbah`,
  `gbav`) run on pocket-core since 2026-10-23, checked in the virtual device; NAOMI and Atomiswave (`flycast`) are wired
  but not verified ("More systems (2026-10-23)" below).
- GameCube, Wii, PS2 and 3DS stay on their standalone emulators, all with our pause menu (save slots, Options, Quit
  following Settings → Playing → Remember where I quit; `docs/BIG-EMULATORS.md`).
- `src/hostsys.rs` lists the systems pocket-core can host (core, BIOS files, pad type, core options). A system is
  routed to pocket-core only when `verified` is set; the others stay on RetroArch until someone has played
  them through and measured them. `POCKET_UI_HOST_ALL=1` (pocket-ui's environment) routes the unverified ones
  too, for testing.

## Systems added (2026-10-21)
"Checked with" says what the game on the device was. Only Old Towers, Reflectron/Santatlantean and Crash
Bandicoot 2 are real games; there were no ROMs for the other systems, so they were checked with tiny ROMs
written for this (`scripts/testroms.py`: moving colour bars, a steady tone, a button that changes the picture).
That proves loading, picture, sound, input, save states and speed of the host and core, not every game's
quirks.

| System (ROCKNIX name) | Core (ROCKNIX default) | Status | Checked with |
|---|---|---|---|
| Mega Drive (megadrive, megadrive-japan, genesis, genh, megadriveh) | genesis_plus_gx | **on pocket-core** | Old Towers (real, Mega Drive homebrew) |
| Master System, Game Gear, SG-1000 (mastersystem, gamegear, ggh, sg-1000) | gearsystem | **on pocket-core** | test ROMs, one per system |
| PC Engine / TurboGrafx-16 (pcengine, tg16) | beetle_pce_fast | **on pocket-core** | Reflectron, Santatlantean (real homebrew) |
| WonderSwan (+Color) (wonderswan, wonderswancolor) | beetle_wswan | **on pocket-core** | test ROM (monochrome) |
| Atari 2600 (atari2600) | stella | **on pocket-core** | test ROM |
| PlayStation (psx) | pcsx_rearmed | **on pocket-core** | Crash Bandicoot 2 (real, PAL, 50 Hz) |
| Sega CD (segacd, megacd) | genesis_plus_gx | RetroArch | no BIOS and no disc here; only the missing-BIOS message was checked |
| PC Engine CD (pcenginecd, tg16cd) | beetle_pce_fast | RetroArch | no `syscard3.pce`, no disc; same |
| 32X (sega32x) | picodrive | RetroArch | nothing to run |
| Neo Geo Pocket (+Color) (ngp, ngpc) | beetle_ngp | RetroArch | nothing to run |
| Atari Lynx (atarilynx) | handy | RetroArch | nothing to run |
| Atari 7800 (atari7800) | prosystem | RetroArch | nothing to run |
| Pokemon mini (pokemini) | pokemini | RetroArch | nothing to run |
| Neo Geo, CPS1-3, FBNeo (neogeo, cps1, cps2, cps3, fbn) | fbneo | RetroArch | no ROM sets; the core is a 20+ MB library too big for the virtual device |

ROCKNIX's default core for Master System, Game Gear and SG-1000 is gearsystem, not genesis_plus_gx; pocket-core
uses the default so that saves and behaviour stay as they are today. Arcade (`arcade`, MAME 2003-Plus by
default) is not wired. PlayStation: ROCKNIX's default is the 32-bit `pcsx_rearmed32`; there is no 32-bit
pocket-core, so the 64-bit `pcsx_rearmed` runs (RetroArch measured with the same one).

Save states, checked on the virtual device with the same ROMs (`scripts/syscheck.sh play ... "fifo save" "press A 700"
... "fifo load"`, `scripts/shotdiff.py`): the picture after Load matches the one at Save (difference 0.0-0.4 of 255,
the "Loaded slot" toast) while the one after playing on differs by 38-68, for Mega Drive, PC Engine, Master System,
Game Gear and Atari 2600; SG-1000 and WonderSwan have nothing that stays changed, there the core only answered
that the state loaded. Auto-resume (`.pstate.auto` written on exit, loaded on the next start, "resumed" in the log)
worked for all eight; Mega Drive, PC Engine, PlayStation and Atari 2600 were also started from the shelf on the
device and resumed there.

To check one of the RetroArch-only systems when its ROMs exist:
`scripts/devlock.sh me scripts/syscheck.sh play <system> <core> <rom> "wait 6" "shot a" "fifo save" "press A 300" "shot b" "fifo load" "shot c" quit`
and `bench` / `ra` next to it (see the header of `scripts/syscheck.sh`), then set `verified: true` in
`src/hostsys.rs`.

## How it works
- **Picture**: core frame → GLES 3 texture → nearest filtering. The frame *height* is scaled by a whole
  number and the *width* follows from the core's aspect ratio (NES/SNES pixels aren't square, so they come
  out about 4:3). The scale is the largest one where both still fit the 1240×1080 screen:
  | | frame | scale | on screen |
  |---|---|---|---|
  | Game Boy | 160×144 | 7× | 1120×1008 |
  | GBA | 240×160 | 5× | 1200×800 (6× would be wider than the screen) |
  | NES (overscan cropped) | 256×224 | 4× | 1170×896 |
  | NES, 240 lines, a 4:3 shape | 256×240 | 3× (4× is 1280 wide) | 960×720 (`native_steps_down_until_the_width_fits`) |
  | SNES | 256×224 | 4× | 1195×896 |
  Geometry changes while playing (`SET_GEOMETRY`, e.g. SNES hi-res) are followed.
  A screen frame can sit behind a picture that doesn't fill the screen (docs/FRAMES.md). Sharpness and screen
  effects are one fragment pass: [Picture effects](#picture-effects-2026-10-24).
- **Pause menu**: pocket-ui sends `pause` / `resume` instead of SIGSTOP; while paused the core doesn't
  run, sound fades to silence and pocket-core renders `preview` requests of the Picture settings
  offscreen (docs/FRAMES.md). On `pause` it writes `paused` into pocket-ui's pipe (`POCKET_HANDOFF_FD`), and pocket-ui
  covers it only then; `shot <seq>` right after renders the last frame, whole, for behind the menu
  (`/tmp/pocket-core-shot.bin`, docs/ARCHITECTURE.md "The pause menu"). All commands: header of `src/bin/pocket_core.rs`. While the menu's Pixel Path page is
  open, `pixelpath <seq>` four times a second writes what the picture and sound go through (docs/CURATOR.md).
- **Pacing**: vsync, plus a frame limiter at the display's refresh rate. On this sway, vsync alone let
  frames through early (61 fps).
  - Cores within 5 % of the display rate (59.73, 59.82, 59.92, 60.10 Hz) run once per refresh.
  - Others are scheduled by time: PAL games at 50 Hz (PlayStation Europe) get a repeated picture every sixth
    refresh, 75 Hz cores two frames every fourth. Audio keeps the core's own rate then. Checked: Crash
    Bandicoot 2 ran 1800 core frames in 36 s = 50.0 fps, 0 underruns.
- **Sound**: PulseAudio simple API, loaded with dlopen. Linear resampling to 48 kHz.
  - Base ratio includes the core/display rate skew (59.73 → 60 Hz).
  - ±0.5 % dynamic rate control on top steers the buffer to 40 ms; the server holds another 20 ms.
- **Input**: evdev (InputPlumber's virtual pad), Nintendo layout. The Mini V2's buttons are numbered like the
  RetroPad, so by default every button presses the console button of the same name. The pause menu's Controls
  page remaps them per console or game (`ctl.<button>` = a console button or `none`), showing the console's own
  names from `SET_INPUT_DESCRIPTORS` ("A", "Select", "D-Pad Up", …). The left stick presses the D-pad
  (`ctl.stick_dpad`, on by default except for systems with analog sticks, `controls::ANALOG_SYSTEMS`); with it off,
  both sticks go to the core as analog input. Hold-to-quit (SELECT+START) and the menu's HOME tap always read the
  physical buttons, whatever the layout. Up to four players, each from their own controller (ports 0-3, the same
  layout for all): see [Players](#players-couch-co-op-srcplayersrs).
  - Mega Drive 6-button: X, Y, L, R reach the core and genesis_plus_gx's default pad type is the one that adapts to
    6-button games, as in RetroArch; untested, there was no 6-button game.
  - PlayStation gets a DualShock (`hostsys`: `analog`, and `psx` is in `controls::ANALOG_SYSTEMS`): both sticks go to
    the core as `RETRO_DEVICE_ANALOG`. The device id comes from the core's own controller list (517 for
    pcsx_rearmed's "dualshock"), not from a fixed constant. Untested with a game that reads the sticks.
  - Test input: with `POCKET_CORE_TEST=1`, the command FIFO also takes `press A 150` and `axis lx 0.8`.
- **Battery saves**: `roms/<system>/<game>.srm`, the same place and format RetroArch uses (a NES save made by
  RetroArch loads and is written back byte for byte). Written every 10 s when changed (RetroArch's autosave
  interval) and on exit, through a temporary file and a rename.
- **Cartridge clock** (2026-10-25): a core's non-empty `RETRO_MEMORY_RTC` is kept in `roms/<system>/<game>.rtc`,
  RetroArch's name (`fill_pathname(savefile, ".rtc")`: the `.srm` path with the extension swapped; also beside
  melonDS's `.sav`). Read right after `retro_load_game`, before the first frame, like the battery save; written with
  it, by the same writer and at the same moments (10 s flush, exit, `autosave`), only when changed. No memory or size 0:
  no file. What the routed cores expose (checked in their libretro sources, 2026-10-25): Gambatte 8 bytes (the RTC's
  base time) for MBC3+TIMER and HuC3 cartridges, so Pokémon Gold/Silver/Crystal; without the file the clock started
  from zero at every cold start. Snes9x 20 bytes for S-RTC and SPC7110 games. mGBA only for Game Boy MBC3 cartridges
  (GBA routes to mGBA, GB/GBC to Gambatte): for a GBA game its RTC data pointer falls through to work RAM but the size
  is 0, so nothing is written, and the GBA clock reads the system time. Nestopia, Genesis Plus GX, PokeMini and
  melonDS DS expose none. A failed resume or a fresh start after a newer battery save puts the clock back with the
  battery save (`back_to_start`).
- **Rotation**: `SET_ROTATION` (vertical arcade games, Beetle WonderSwan's portrait mode) turns the picture by quarter
  turns in the vertex shader, added to Options → Picture → Screen orientation's (see
  [Screen orientation](#screen-orientation-2026-10-25)); scaling and effects work on the turned shape. The core's aspect
  ratio is taken as the turned picture's (until 2026-10-25 it was inverted, which would have squeezed a vertical FBNeo
  game into 4:3). `pocket-core --rotate N` stands in for the core's request, for testing. Checked with all three turns
  on a Mega Drive game (HUD corners move where they should). No vertical arcade game to try, so a real FBNeo game is
  still to be seen.
- **Discs**: cores that take a path (PlayStation, Sega CD, PC Engine CD) get it as is: `.cue`, `.chd`, `.m3u`,
  `.pbp`. Multi-disc: the core's disk control interface (`SET_DISK_CONTROL_INTERFACE`, the first five callbacks of
  the extended one) lists the discs of an `.m3u` or a multi-disc `.pbp`. pocket-core writes what it says to
  `/tmp/pocket-core.disc` (`src/disc.rs`: the disc in the drive, 1-based or 0 when the core names none, the count, and
  the game's path so another game's file is never read) after the load, at `pause` when it changed and after every
  change; it removes it at the start (a crashed run's) and at the end. The pause menu shows a **Disc** row when the
  file names this game and more than one disc (`src/app_disc.rs`): Left/Right choose, A sends `disc <n>` (1-based)
  and goes back to the game. `Core::set_disc` opens the tray, sets the disc and closes the tray in one go, as the
  interface allows, as the existing `disc next` did. Whether every game notices a change made in one go (some may want
  the lid open for a moment) is not checked yet: if one doesn't, the tray close moves a few frames later.
  pocket-core answers with a note over the game ("Disc 2 of 3", or "Couldn't change the disc"). `disc next` still
  works (wraps around). A single `.cue` reports one disc: no row. `disc next` was checked with an `.m3u` listing the
  disc twice; the Disc row has not been tried on a Mini V2 with a real multi-disc game yet.
- **Archives**: `.zip` is unpacked for cores that take ROM data and don't read zips themselves (the same rule
  RetroArch applies; `src/archive.rs`, stored and deflate). 7z is not supported: pocket-ui sends such games to
  RetroArch. The entry is not chosen by archive order: the one file with an extension from the core's list is the game
  (none: the one file that is not clutter, `archive::is_clutter`: texts, pictures, checksums). Mac leftovers
  (`__MACOSX/`, `._*`, `.DS_Store`) never count, whatever their extension (`archive::is_junk`).
  Two or more candidates, an encrypted entry (general-purpose flag bit 0, or method 99), a method other than 0 and 8,
  a zip without a game, a file without the end-of-central-directory record (cut off by a copy or a download that
  stopped: RetroArch's reader needs that record as well), or unpacked bytes whose CRC-32 (`crc32fast`, the central
  directory's value) differs are
  refused with a sentence for the player that names the zip and says what to do (`archive::is_refusal`). The sentence
  goes the usual way, `fail()` -> `/tmp/pocket-core.error` -> the card on the shelf, and `fallback::should_retry` does
  not hand such a game to RetroArch (it would choose by order or play a damaged ROM, and the game would be remembered
  there). Any other unreadable zip ("not a valid zip file": ZIP64, a comment over 64 KiB), a deflate stream that fails or a
  length that differs, and a ROM over 64 MiB are not refusals: they still fall back to RetroArch. Not covered: other
  codecs (deflate64, bzip2, LZMA), ZIP64, passwords. The CRC runs over the bytes already in memory, off the UI thread,
  before the core starts: a few ms for a normal ROM. RetroAchievements' `hash_rom` and the starter pack's install use
  the same reader, so a zip with several games gets no hash there either.
- **Patches** (softpatching, `src/softpatch.rs`, as RetroArch): `<rom stem>.ips`, `.bps` or `.ups` next to the ROM
  (the first that exists, in that order, the ending in lower or upper case), then `<stem>.<ext>1`, `<ext>2`... of the
  same kind until the first gap, applied in order. For a zip the stem is the zip's. `Core::load` checks for them
  (`softpatch::find`: up to six `stat`s, nothing read when there is none) and, when one exists, hands the core the
  patched game (`softpatch::for_core`): the bytes for a core that takes data (the file, or the game unpacked from its
  zip), a copy in `/tmp/pocket-core-patched-<pid>/<file name>` for a `need_fullpath` core (removed with the `Core`,
  after `retro_unload_game`; `--restore-system`, which pocket-ui runs after every game, removes those of runs that no
  longer exist). Formats: IPS with run-length records and Lunar IPS's truncate extension (three bytes after `EOF`),
  BPS (all four commands, metadata skipped) and UPS (forwards only), both with their source, target and patch CRC-32;
  BPS/UPS sizes are checked before anything is allocated, at most 128 MiB out (`MAX_OUTPUT`), patches up to 64 MiB.
  A refusal is a sentence that names the patch and says what to do (`softpatch::message`): another version of the
  game (BPS/UPS source size or CRC; a copier header counts as another version, as in Flips and RetroArch), damaged
  (not a patch, cut short, a CRC of the patch or of the result, a copy outside the game), too big, or a kind of game
  that can't be patched (disc images and sheets, `.m3u`, ScummVM, a folder, a zip the core opens itself). It goes the
  `fail()` -> `/tmp/pocket-core.error` -> shelf notice way, and `fallback::should_retry` does not hand the game to
  RetroArch (`softpatch::is_refusal`), which would start the original after a failed patch. Unlike RetroArch, a
  patch that doesn't apply never starts the game unpatched. Saves and states keep the ROM file's name (a hack next to
  its base game shares its save, as in RetroArch; the manual says so). `Core::patches` lists what was applied: Pixel
  Path's emulator stage says "Patched: <file>" (`pixel_path::Report::patches`, grade Enhanced, like cheats), and the
  game card shows a "Patch" row (`softpatch::find` when the card opens). RetroAchievements hashes the patched game
  (`achievements::hash_rom` -> `softpatch::patched_content`), as RetroArch hashes the patched buffer: a translation
  RetroAchievements knows gets its set, any other patched game none, and the original's hash is never sent for a
  patched game. Patching costs one read of the patch and one pass over the game before the core starts: a few ms for
  a cartridge game; none of it on pocket-ui's UI thread.
- **libretro VFS** (`src/vfs.rs`, version 3): Stella reads its ROM only through it and failed without it
  ("Unrecognized ROM file type"); pcsx_rearmed also uses it for disc images. `seek` returns 0 on success like
  RetroArch's own (pcsx_rearmed treats the new position as an error).
- **BIOS and failed starts**: `src/hostsys.rs` knows the BIOS files a system needs (Sega CD, PC Engine CD); when
  one is missing, pocket-core writes the reason to `/tmp/pocket-core.error` and exits, and pocket-ui shows it for
  8 s as a card at the top of the shelf (`src/notice.rs`). Other failed starts carry the core's own last
  error line. Checked on the virtual device with a Sega CD game and no BIOS:
  "Sega CD needs a BIOS file in roms/bios: bios_CD_U.bin, bios_CD_E.bin or bios_CD_J.bin".
- **Core log**: the log callback now formats the core's messages (six integer arguments), so BIOS and file
  errors read properly in `POCKET_CORE_TRACE=1` output.
- **Core options** (`src/core_options.rs`): we report options version 2 and take every dialect: `SET_VARIABLES`,
  `SET_CORE_OPTIONS`(`_INTL`), `SET_CORE_OPTIONS_V2`(`_V2_INTL`), plus `SET_CORE_OPTIONS_DISPLAY`, the update-display
  callback, `SET_VARIABLE` and `GET_VARIABLE_UPDATE`. Values come from (lowest first) the core's default, our
  curated default for that core (`core_options::curation`, which also hides options that make no sense on a
  handheld), and the user's saved `opt.<key>` entries. pocket-core writes
  `<data>/core-options/<system>.json` at start and whenever options, their visibility or the layout change; the
  pause menu's Emulator and Controls pages read it and send `opt <key> <value>` / `ctl <button> <value>` over the
  FIFO. The menu freezes the game (SIGSTOP), so the core sees changes, and updates which options are visible,
  only once the game continues; the menu shows the new state the next time it opens.
  `pocket-core --dump-options <core.so>` prints what a core declares, for curating a new one.
- **New core**: `POCKET_CORE_TRACE=1 pocket-core …` prints every environment command the core asks for that
  we don't answer.
- **Testing without touching the buttons**: `scripts/vpad.py` creates a virtual gamepad on the device
  (start it before the game, then `echo "press A 150" > /tmp/vpad.cmd`). Stop it before restarting
  pocket-ui, which would otherwise read it as a second pad.

### When a save can't be written

`SaveWriter` (`src/save_writer.rs`) keeps the outcome of the newest write of each file (`.srm`, `.rtc`). A failed one
stays "failing" until a write of that file gets through, and is written again at every 10 s flush; `fsutil::write_atomic`
writes a temporary file and renames it, removing the temporary file on error, so the file on the card stays the last
good one. `Core::save_trouble` reads that outcome without waiting (one mutex, at the flush); `Core::write_sram` (exit,
`autosave`, `quit nosave`) tries once and returns the kind (`save_trouble::Trouble`: `Full` for ENOSPC/EDQUOT,
`Unwritable` for EROFS, EIO, EACCES and the rest). pocket-core then:

- shows the kind's line over the game, at the bottom in the notes' place, until a save gets through
  (`overlay::Hud::save_trouble`; a note has its moment first). Only a change repaints the pill.
- writes `/tmp/pocket-core.save-trouble` (`full` or `unwritable`) when the trouble starts, removes it when it ends, and
  removes a stale one at start.
- after an exit or `autosave` whose battery save failed but whose auto state was written, leaves an empty
  `<game>.pstate.auto.battery-unsaved` (`slots::battery_unsaved`). The next `resume` reads and removes it: with the note,
  a state whose battery save differs from the card's wins (it holds the newer one) unless the file on the card is newer
  than the state (written since, by Syncthing or RetroArch). Without it that start would take the older save on the
  card for the newer one and throw the state's away.

pocket-ui reads the note when the pause menu opens over a pocket-core game (`app_save_trouble`): a line under the title,
and Quit opens "Couldn't save. Quit anyway?" (Keep playing / Quit anyway, with what is lost). Quit anyway goes on as
Quit does; the exit save is tried once more. A game that ended with the note still there gets a notice on the shelf
(`Notice::take_after_game`). The game switcher, sleep and the critical-battery power-off don't ask: they send their
usual commands and pocket-core tries the save once. A write that hangs on a dying card holds nothing up for long: a
quit's escalation (`quit::grace`) ends the process, the power-off waits 25 s at most (`quit_for_power_off`). Tests: `save_writer` (ENOSPC, EROFS, per-file waiting), `libretro::resume_tests` (a fake
core with and without RTC memory, a full card, the note), `save_trouble`, `overlay`.

## Picture effects (2026-10-24)

`src/shaders/video.frag` (one fragment pass, one variant per sharpness × effect, compiled on first use by
`video::Renderer`), its numbers and their CPU reference in `src/picture_fx.rs` (handed to the shader as `#define`s, so
they can't drift apart), user guide in [user/options.md](user/options.md#screen-effect). Tuned for this panel: 1240×1080
at about 310 ppi, seen from 25-35 cm, where one pixel is 0.08 mm.

**What was there** (until 2026-10-24): LCD grid (a 1-2 px dark line at each texel's start, darkened in encoded
values), Scanlines (a cosine dip, 55 % deep at every brightness), CRT (a Gaussian beam sampled at the pixel's
centre, a 3-pixel RGB stripe mask, a highlight boost) and Crisp (sharp bilinear by the texture unit). One texture tap
each, cheap. Weaknesses: everything mixed encoded values, so scanlines and the grid made the picture darker
(Scanlines about a sixth) and mid-tones muddy; a beam sampled at pixel centres; a 3-pixel mask period, which can
beat against an OLED's subpixels where they come in pairs; scanlines faded out below 3 px a line, and a 3D core
rendering at 2× (N64 at 640×480) had lines at 480, too fine to show; Crisp's one-pixel blend in encoded values made
a bright pixel on a dark ground lighter or heavier with its phase (the uneven sizes Crisp is there to hide).

**What it is now** (names and saved values unchanged: `none`, `grid`, `scanlines`, `crt`):

| Effect | How | Texture taps |
|---|---|---|
| **Scanlines** | Each line is a beam: a plateau with soft edges (`BEAM_EDGE` 0.45 lines), half-height from 0.25 lines (black) to 0.5 (full brightness, the gap closes) by each channel's own brightness squared (a TV's three guns; a bright red line is as tall as a white one in red, so its plateau never has to clip). Area-normalised and averaged exactly over each output pixel's height (the trapezoid's integral is a few multiplications), so every line gives off exactly the light of its colour at any scale and phase: lines are even at 4× (SNES Integer), 3.875× (4:3 at Aspect, 1240×930) and 4.15× (SNES at Aspect); the slow ripple a fractional scale leaves is below 0.4 % (below 1 % for near-black lines; point sampling the same profile: 1-9 %). Computed in the picture's line space from the two nearest lines, in linear light (gamma 2: a square and a square root; within a few per cent of the panel's 2.2, half the cost of `pow`). Below 2-3 px a line they fade out (480i on 1080 px). Across a line: crisp steering (no uneven columns) for Sharp and Crisp, bilinear for Soft | 2 |
| **CRT** | Scanlines as above, a two-pixel magenta/green mask (`MASK` 0.12 in linear light, gain so a pair averages white; 0.16 mm per pair is beyond the eye at arm's length, and in step with the subpixels if the panel pairs them, so it gives texture, not stripes), and a glow: 7 % of a blurred copy (the texture's mipmaps, made with `glGenerateMipmap` when a new picture came, the level about 28 texels tall, each of its texels 1/28 of the picture's height, 8 lines of a SNES picture) mixed in linear light. No curvature | 3 |
| **LCD grid** | A gap at each texel's start, `LCD_GAP` 0.18 of a texel (0.9 px at GBA 5×, 1.26 px at GB 7×, 0.5 px at DS 2.8×), taking `LCD_DARK` 75 % of the light where it covers, drawn by how much of each output pixel it covers (exact at whole scales, even at fractional ones), in linear light, with 8 % given back inside the cell. Fades out below 2 px a texel. **LCD colours** (Game Boy Advance only, `lcd_colour`): Pokefan531's public-domain GBA screen profile (colour matrix, level 0.94) with half its darkening (decoding gamma 2.7 instead of 3.2, which is very dark on an OLED); left out while the core's own colour correction is on (`pixel_path::corrects_colour`, read whenever pocket-core writes the options snapshot), so it never applies twice. Game Boy Color: gambatte corrects colours out of the box, so no row | as Sharp/Crisp |
| **Crisp** (sharpness) | Flat texels and a one-output-pixel ramp at their edges (exactly nearest at whole scales), now blended from four texel fetches in linear light, so a pixel keeps its weight at every phase | 4 fetches |

Line space: one line per row, except a 3D core rendering above its console's lines (`picture_fx::crt_lines`, frames
from `copy_texture` and `pixel_path::native_lines`): N64 at 2× (480 rows) gets 240 lines, each sampled between its
two rows; Dreamcast at 1× keeps 480. A 2D core's 448/480-row interlaced pictures keep their rows and the lines fade
out, as on a TV. The mask stays in screen columns when the picture is turned (SET_ROTATION, Screen orientation); the
lines turn with it.

Not done, on purpose: ghosting / response blur (gambatte's "Mix frames" and mGBA's "Interframe blending" already do
it on the CPU for a few µs and Pixel Path shows them; a GPU copy would do it twice), RGB subpixels inside LCD cells
(1.7 px stripes at GBA 5× cost a third of the light and can't be seen at this density), a DMG-specific light grid,
curvature.

**Cost.** Measured on the host (`cargo run --release --example picture_shots -- --time 300`, llvmpipe on 10 CPU
threads, a busy shared host, the best of three runs, wall time per picture including the clear of 1240×1080; the
old shaders measured the same way before the change with 60 pictures):

| Picture | Sharp, none | Crisp | Scanlines | CRT |
|---|---|---|---|---|
| SNES 4× (1195×896), old | 1.44 ms | 2.09 | 1.81 | 1.84 |
| SNES 4×, new | 1.29 | 2.24 | 3.62 | 5.76 |
| PS1 at Aspect (1240×930), old | 1.48 | 2.24 | 2.04 | 2.12 |
| PS1 at Aspect, new | 1.57 | 2.84 | 3.64 | 5.74 |
| N64 640×480 through `copy_texture`, Aspect, new | 1.48 | 2.71 | 3.78 | 5.85 |

llvmpipe is a CPU rasteriser: `pow`, mipmap sampling and every ALU operation cost far more there than on a GPU, so
these numbers rank the effects, nothing more (its timer queries measure nothing useful). On the Adreno 650 the
estimate from the shader's size: the CRT is about 300 scalar ALU operations, 10 special-function ones (square
roots, reciprocals) and 3 bilinear taps per pixel; Scanlines about 270 and 2 taps; Crisp 4 fetches and about 40;
the old effects 40-60 and one tap. With about 512 ALU lanes at ROCKNIX's lowest GPU clock (305 MHz) and 1.15 M
picture pixels, that is **about 2.2 ms per picture for the CRT** (1.1 ms at 587 MHz), about 2 ms for Scanlines,
0.5-1 ms for Crisp and the LCD grid, against about 0.35 ms for the old CRT. The device measurement below (1.46 ms
for the CRT, 1.32 ms for Scanlines) lies between the estimate's two clocks; which clock the GPU ran at was not logged.
(A first measurement had made the estimate look about 4 times too high: it was wrong, see below.) Nothing new runs on pocket-core's loop besides the draw (the mipmaps are
one GL call in it when the CRT's picture changed); run-ahead and fast-forward draw one picture per refresh as before.

**Measured on the Mini V2** (2026-10-26, main `9df5564`, pocket-core's own GL timer `POCKET_CORE_GPU_TIMER=1`, no
screen frame, windows of 600 pictures, mean of the picture pass):

| Game, scaling | None | Effect |
|---|---|---|
| Donkey Kong Country (SNES), Integer | 0.19 ms | Scanlines 1.32 ms, CRT 1.46 ms |
| F-Zero: Maximum Velocity (GBA), Integer | 0.17 ms | LCD grid 0.28 ms |

So on the SNES picture the CRT adds about 1.3 ms and Scanlines about 1.1 ms (8-9 % of a 60 Hz refresh of 16.7 ms), the
LCD grid on the GBA about 0.1 ms. Not logged: the GPU clock during the windows (ROCKNIX scales it between 305 and
587 MHz; the same pass costs about twice as much at the lowest clock), an open item for the next measurement. Not
measured: other cores and scalings (3D cores, N64, Crisp, Aspect), and how the picture looks on the panel (the visual
checklist is still open).

**The first measurement was wrong** (2026-10-25, main `0eacac4`, the same SNES game at Integer, but with the CRT TV
frame on): 0.397 ms for None and 0.58-0.59 ms for each of LCD grid, Scanlines and CRT, written here and in
STATUS.md as "an effect adds about 0.19 ms, about 3.5 % of a refresh". Three effects whose shaders differ several times
in their work cannot cost the same to a hundredth of a millisecond: most likely the effect was not switched in the
running game between the windows, so the three windows measured one and the same pass, and the screen frame was in
all of them. Before reading a window as an effect, check that the picture on the panel changed, and take the frame
off (or note it).

**To repeat it**: `POCKET_CORE_GPU_TIMER=1` puts GL timer queries
(`GL_EXT_disjoint_timer_query`) around the picture pass; pocket-core logs `gpu picture: … ms mean, median, p99` every
600 pictures (read back frames later, nothing waits). Without the extension it logs that and stays off.

**Tests**: `picture_fx`'s unit tests (each line's light exact at 4×, 3.875×, 4.15×, 3×, 3.37×, 2×; slow ripple below
0.5 %; gaps in the dark, closed at white; plateau never above full brightness; mask pair white; Crisp exact at whole
scales and even at 4.67×, where blending encoded values loses 6-10 % of a bright pixel's light depending on its phase; the LCD gap's share at every scale and phase; GBA
colours; the line count of 3D cores) and `tests/picture_shaders.rs` on a headless GLES 3 context (all 15 variants
compile and link on Mesa's strict GLSL ES compiler; Sharp and Crisp at 7× are the source pixels; a flat grey at
Aspect keeps its light within 4 % in every 8-line block for Scanlines and CRT; the GPU's scanlines match the
reference within 2/255; LCD grid and colours; the CRT mask; N64 at 2× gets 240 lines). Comparison pictures:
`examples/picture_shots.rs` draws made-up test frames (a 16-bit scene, a GBA RPG scene, Game Boy greens, a PS1-like
3D frame with its dither, a test pattern, two DS screens, the N64 path) through the real renderer at 1240×1080.

**Device checks** (the main session; none of this was seen on the panel):

1. SNES at Integer (4×) and at Aspect, CRT and Scanlines: lines even, no slow bands; dark areas show gaps, white and
   saturated colours close them; colours as bright as without an effect. The mask must not show as stripes or tint
   at normal distance (close up a fine texture is fine); if it does, lower `picture_fx::MASK`.
2. PS1 at Aspect (Crash Bandicoot, a menu in 480i: lines fade there), N64 (Mario 64, GLideN64 at 2×: 240 lines) with
   CRT: same checks; `POCKET_CORE_GPU_TIMER=1` and read the `gpu picture:` lines in /var/log/pocket-ui.log for
   Sharp/none, Crisp, Scanlines and CRT; fps and late refreshes unchanged with run-ahead 1 on SNES and with
   fast-forward.
3. GBA at Integer (5×) and Game Boy (7×) with LCD grid: the grid visible but calm; GBA with LCD colours on and off,
   then with mGBA's own Color Correction on (ours steps back, Pixel Path's Look says so). NDS at Aspect (2.8×): an
   even, faint grid.
4. Crisp at Aspect (SNES, ScummVM): no uneven columns, pixel weights even.
5. The pause menu's live preview ("Actual size") shows the same as the game.

## Screen orientation (2026-10-25)

Options → Picture → **Screen orientation** (`orientation`: `normal`, `left`, `right`; per console or game through Save
changes like every Picture setting) says how the player holds the Pocket, for games made for an upright screen
(vertical shooters, WonderSwan, DS books). User guide: [options.md](user/options.md#screen-orientation).

- **Picture** (`video::Orientation`, `Renderer::turns`): Turned right draws the picture a quarter counter-clockwise on
  the panel (the shader's `rot` 1), Turned left a quarter clockwise (`rot` 3), on top of the core's `SET_ROTATION`; the
  sum mod 4 goes to the vertex shader, which already turned the texture coordinates. `video::turned_shape` gives the
  shape to scale: the frame's sides swap with every odd total, the aspect only with an odd turn of the player's (a
  core's aspect is that of its picture as meant to be seen, see Rotation above). Integer then scales the panel's
  height by whole multiples of the turned picture's: a 224×288 game at 3:4 goes from 3× (648×864) to 4× of its 224
  columns (1195×896), a WonderSwan game held upright is 576×896, a Game Boy 864×960. The preview is the same draw
  (`render_offscreen`), so it shows the turn; snapshots turn their stand-in frame with `video::turn_pixels`.
- **Cost**: nothing per pixel. The turn is four vertices' texture coordinates; the fragment shader and its variants
  are unchanged. Measured on the host at the same pixels (`picture_shots --time 120`, SNES scene at Fullscreen,
  upright against turned right, llvmpipe): Sharp 1.97 / 2.01 ms, Crisp 3.75 / 3.81, Soft 3.27 / 3.04, Scanlines
  5.19 / 5.11, CRT 8.47 / 7.90, Crisp CRT 8.26 / 8.31: the same within the run-to-run spread. On the device the GPU
  reads the texture column by column when turned; whether its texture cache minds is for `POCKET_CORE_GPU_TIMER=1`
  to show (device checks below).
- **Controls** (`controls::turn_buttons`, `turn_sticks`): the Pocket's own D-pad, the left stick as D-pad and both
  analog sticks turn with the picture, before the Controls mapping, so up is up in the turned game. Turned right:
  up → right, right → down, down → left, left → up; Turned left the other way. Face and shoulder buttons stay (a
  vertical shooter's buttons are pressed by feel; Controls remaps them). Only the player who holds the Pocket
  (`players::Inputs::is_pocket`); a Bluetooth controller doesn't turn with it. HOME shortcuts are not turned.
- **Effects**: Scanlines and the CRT's lines follow the picture's rows, so they turn with it; the CRT mask stays in
  the panel's columns (its subpixels). The LCD grid is in the picture's pixels.
- **Frames**: only Ambient and Glow (no up and down) show around a turned picture (`VideoSettings::shows_frame`); the
  others are drawn for a Pocket held as usual and would stand on their side. The choice is kept and comes back with
  Normal.
- **Touch**: the finger goes through the same turn (`video::frame_point`, the vertex shader's mapping, tested against
  it), so a turned picture gets a pointer, also from a core's own turn (before 2026-10-25 a turned picture got none).
- **Pause menu, notices, HUD**: stay upright for a Pocket held as usual. Decided so: the player turns the Pocket back
  to use them; a turned menu would need every page, the hint bar and the touch map turned for a moment that pauses
  the game anyway, and the menu is pocket-ui's (one process) while the picture is pocket-core's.
- **Launch picture** (pocket-ui, between A and the first frame, `launch_picture::prepare`): the auto-resume preview
  is turned and placed like the game, so the first frame lands on it. A core's own turn is unknown to pocket-ui
  there, as before.
- **Not turned**: screenshots (HOME + A) and save-state previews keep the core's frame; the switcher shows those
  previews as they are.
- **Arcade** stays on RetroArch: its own Vertical mode (FinalBurn Neo), TATE Mode (MAME 2003-Plus) or Video Rotation
  do it there ([arcade.md](user/arcade.md#vertical-games)); ROCKNIX's per-game `tatemode` and `rotation` in
  `system.cfg` set the last two. Not built into Runs on.
- **Tests**: `video` (orientation parsing, the turn per choice, `frame_point` against the shader source, turned
  shapes and whole-number scales, a core's own turn plus the player's, frames), `controls` (D-pad and sticks per turn,
  one consistent with the other), `players` (which seat is the Pocket), `touch` (a turned GB picture, a DS turned
  like a book), `options` (saved per game and per console, Normal by default), `pixel_path` (shape via
  `turned_shape`), `tests/picture_shaders.rs` (on llvmpipe: corners of a turned picture both ways, black around it and
  whole 6×6 texels, a core's turn undone by the player's, scanlines running down the panel when turned).

**Device checks**:

1. A WonderSwan game (beetle_wswan): Options → Picture → Screen orientation → Turned left, then Turned right: the
   picture fills 1120×720 or 576×896 as expected and the preview shows the same; the D-pad follows. Then SELECT (the
   core's own rotation): the picture turns upright on the panel (core aspect 9:14 kept, not squeezed) and Turned left
   makes it large again.
2. A vertical homebrew or a Game Boy game (Tate-style, any game will do to check the controls): Turned right with the
   Pocket held D-pad on top: up/down/left/right as seen; the left stick as D-pad too; a PS1 game with analog sticks
   turned likewise. A Bluetooth pad as player 2 is not turned.
3. `POCKET_CORE_GPU_TIMER=1`: `gpu picture:` the same upright and turned (SNES at Fullscreen with CRT, and at Integer).
4. Save changes → For this game, quit, start again: still turned; another game of the console is not.
5. A frame (Auto) with Turned left: none shown; Ambient and Glow shown; back to Normal: Auto's frame again.

## Pause menu features
Everything below is chosen in pocket-ui's pause menu (HOME tap) and applied to the running pocket-core at once through
`/tmp/pocket-core.cmd` (`set <key> <value>`); "Save changes" makes the values the console's or the game's.

**Save slots** (`src/slots.rs`). Eight per game plus the auto-resume state, in `/storage/roms/savestates/<system>/`:
`<game>.pstate` (slot 1, the file that always existed), `.pstate1` … `.pstate7` (slots 2 to 8), each with a `.png`
preview, `<game>.pstate.auto` and `<game>.pstate.undo` ("Before load", written before every Load and dropped at
quit; `src/states.rs`). Save and Load share one slot; Left/Right on either row steps through them, the
preview and a row of eight dots (filled = has a state) follow. The slot last used is remembered per game in
`<data>/slots/<system>/<game>`, so the menu and the hotkeys always agree. Commands: `save [n]`, `load [n]`, `slot n`
(n counts from 1, as on screen).

**Auto-resume and the battery save** (`Core::resume`). Exit and `autosave` write the battery save first, then
`<game>.pstate.auto`, and delete the taken ones below. The next start renames the state to `.pstate.auto.taken`
before loading it. If that session ends without an exit save (a crash, a dead battery), the start after it finds
only `.taken`, still the quit point of the session before, and tries it once more as `.taken.retry`; if that crashes
too, the next start is fresh. A state the core refuses is not tried again. If the battery save (`.srm`, or `.sav`
for melonDS) differs from the one inside the state, the game starts fresh from that save instead ("In-game save is
newer: not resumed"): most cores keep the battery save in their states, so resuming would put the older save back
and the 10 s flush would write it over the newer one (a crash after an in-game save, or the game played in RetroArch,
standalone melonDS or on another device in between). Since review SOL-02 the contents decide, not the file times
(a copy that kept its time, a clock set back), and the whole file counts (a longer save differing past the core's
length, padding aside). The file times only decide whether a snapshot of the starting game is taken before the load
(it costs a serialize); without one the game goes back with a reset and the battery save it started with. A core that
refuses a state after putting part of it in place (its battery save among it) is put back the same way, and a manual
load it refuses that way goes back to "Before load".

**Shortcuts** (`src/hotkeys.rs`, Options > Shortcuts, each can be switched off). pocket-core reads the same evdev pad
as pocket-ui; while HOME is down the game gets no buttons.

| Keys | Action |
|---|---|
| HOME + R1 | save to the current slot |
| HOME + L1 | load the current slot |
| HOME + Left / Right | previous / next slot |
| HOME + R2 | fast-forward on / off |
| hold HOME + L2 | rewind |
| HOME + Y | Over the game: the next level (Off → FPS → FPS + sensors → Pixel Path → Off) for this session, with a toast "Over the game: FPS" |

A HOME tap on its own still opens the menu: pocket-ui treats any other button pressed while HOME is down as "this was a
shortcut". The Mini V2's L2/R2 are analog axes (a virtual pad reports them as buttons); both count from half travel.
Messages ("Saved to slot 3", "FF 4x", "REWIND") are drawn over the picture by `src/overlay.rs`, and so is what Over the
game shows (below): the FPS line, or Pixel Path's strip beside the picture (`src/pixel_live.rs`,
[CURATOR.md](CURATOR.md#pixel-path-over-the-game-featpixel-path-live-2026-10-23)).

**Extras** (`src/extras.rs`, `src/rewind.rs`, `src/power.rs`):
- *Fast-forward speed*: 2x, 3x, 4x (default), 8x or as fast as possible. The core runs that many frames per screen
  refresh (max: as many as fit into 12 ms); only the last picture is drawn and sound is muted. The frames before the
  last are run without picture or sound (`Core::run_with`, GET_AUDIO_VIDEO_ENABLE, software-rendered cores only),
  "as fast as possible" shows the frame it expects to be the last within its 12 ms.
- *Rewind*: every frame's `retro_serialize` state is kept as a reverse delta of the next one (XOR, unchanged 8-byte
  words left out), at most 48 MB and 60 s. Holding HOME + L2 loads one state back per refresh (real-time reverse, sound
  muted); at the oldest state the picture holds. Loading a state clears the history, and so does the instant resume
  at start (a hardware core resumes some 30 frames in, after rewind has recorded its boot; review SOL-11).
- *Over the game* (`src/over_game.rs`, 2026-10-25; until then Show FPS with On / Sensors / Pixel Path, the Pixel Path
  shortcut and Curator's "Pixel Path over every game"): one level, **Off**, **FPS**, **FPS + sensors** or **Pixel
  Path**, of which pocket-core draws one piece at a time (`Level::counter`, `sensors`, `strip`). The Options key is
  `over_game` (`off`, `fps`, `sensors`, `pixel`); a game's or console's value wins over Settings → Playing → Over the game
  (`settings.json`'s `over_game`, read by `options::GameSettings::load` as the default below them). HOME + Y
  (`Action::NextOverGame`, Options key `hk_over`) moves `Extras::over_game` on for the session; a change of the option
  (`set over_game …`, also the Pixel Path page's Y) sets it again. **Migration:** `over_game::migrate` turns an Options
  file's `show_fps` (`on` → `fps`, the others as they were) and `hk_pixel` into the new keys when it is read, and
  `over_game::adopt_curator` moves `curator.json`'s `pixel_live` (only with `pixel_path` on, as it worked) into
  settings.json once at pocket-ui's start. The Pixel Path level no longer waits for Settings → Curator → Pixel Path in
  the pause menu, which is the page's switch alone.
  - *FPS*: "60 FPS   CPU 23%" top left, updated twice a second (CPU of the pocket-core process, 100 % = one core).
  - *FPS + sensors* adds the device's state to the same line: "60 FPS   CPU 23%   62 °C   Fan 45%   4.8 W" (hottest
    thermal zone, the fan's PWM duty or rpm or "Fan off", the battery draw in watts or "charging"; `src/osd_sensors.rs`).
    Read the way `perf_fan::Readout` reads them, but by a thread of its own that `Hud::set_sensors` starts when the level
    is switched on and ends when it is switched off, every 500 ms (the counter's cadence); that thread also words the
    part in the interface's language ("Lüfter 45%", "lädt": each part with words through `de.tsv` on its own, numbers and
    units as they are), so the emulation thread opens no file and formats nothing for it, `Hud::frame` only appends the
    last line twice a second. A sensor the device doesn't have (no fan, no
    gauge, a gauge without current, a full battery on the charger) is left out of the line, never shown as 0 or "?".
  - *Pixel Path*: Pixel Path's strip beside the picture (`pixel_live`), built from a fresh report twice a second while
    the level is on; it has the frame rate, so the FPS line is not drawn with it.
  - *Cost.* Per frame the level costs two branches (`counter()`, `sensors()`, which only acts on a change); Off draws
    nothing (`Overlay::draw` returns before touching GL when there is no text, tint, card or strip), starts no reader
    thread and builds no report (`update_strip` asks `strip()` first). FPS formats one string twice a second and
    uploads it when it changed; Pixel Path's budget is in CURATOR.md (14 µs a report on the M1, the strip uploaded only
    when it changed). Checked by `over_game` and `extras` tests (Off wants no piece) and `overlay`'s
    `off_formats_nothing_and_starts_no_reader`; `--bench` prints the draw time per frame and the strip's time.
  - Only pocket-core's games have these: a standalone emulator (ARMSX2, Dolphin, Azahar) draws its own picture and
    pocket-ui puts nothing over it while it runs, so there is nothing to add the numbers to cheaply (its own Show FPS
    is its counter; pocket-ui would need a layer of its own above the emulator's window just for this).
- *Performance* and *Fan*: see below.

- *Run-ahead*: Off (default), 1 or 2 frames, light systems only. See the next section.
- *Earlier states*: On (default; Off for N64, PSP, DS, Dreamcast, Saturn, NAOMI, Atomiswave). See
  [Earlier](#earlier-rolling-states-srctimeliners-2026-10-25).

### Earlier: rolling states (`src/timeline.rs`, 2026-10-25)

muOS-style timeline saves on pocket-core, the pause menu's **Earlier** row (`src/app_earlier.rs`;
[user guide](user/pause-menu.md#earlier)).

- **What is kept**: a ring of 6 states, one every 5 minutes of play (`<game>.pstate.earlier1` … `6`, the oldest
  replaced; which is newest is the file time), and the state each session continued from, copied from
  `.pstate.auto.taken` (or `.taken.retry`) with the auto state's picture (`.pstate.started1` … `3`). Each with a `.png`.
- **Play time** (`timeline::Clock`): every refresh of play adds the time since the one before, at most 250 ms; `pause`,
  `sleep`, the pause's end (`after_pause`) and Earlier switched off stop the clock, so the menu, sleep and a suspend
  nobody announced don't count, nor does a stalled refresh (a loading screen) beyond a quarter second. Fast-forward
  counts as the time it took.
- **The cost guard** (`timeline::moment`, `Timeline::moment`): the serialize has to run on the emulation thread. A
  state is taken between two refreshes (after the swap, before `limit` waits for the next one) only if the serialize
  is measured at a quarter of a refresh or less (run-ahead's `SERIALIZE_SHARE`, 4.17 ms at 60 Hz): Earlier's own last
  one in this session, else run-ahead's save + load mean, else rewind's capture mean, else the state is at most 2 MB
  (`retro_serialize_size`, asked once). Hardware-rendered cores never (their picture would be a GPU read-back). The
  others wait for a quiet moment: one second into the pause menu (`QUIET_AFTER`, when pocket-ui has listed the
  states), or the resume state a sleep writes anyway (`autosave`), copied without a second serialize. pocket-ui
  sends `load earlier3 <file time>`; a state replaced since it was listed is refused ("That earlier state was just
  replaced").
- **The writer** (`timeline::Writer`, thread `earlier-writer`): the emulation thread hands over the bytes and a copy
  of the last frame; the PNG, the file (`fsutil::replace_file`: temporary file and rename, no fsync, a lost one is
  harmless) and the caps run there. One waiting state of the ring is replaced by a newer one; at quit the one being
  written is finished and waiting ones are dropped. Nothing is written below `storage::LOW` (1 GB) free plus the
  state, nor a state larger than a game's cap.
- **Caps** (`timeline::enforce`): 100 MB a game, 1 GB over all systems' folders, pictures included, oldest out first,
  never the one just written. Storage's automatic-states clean-up takes them (`storage::auto_state_base`), its Save
  states row shows `Usage::earlier` with the caps; Sync's `pocket-states` `.stignore` leaves out
  `*.pstate.earlier*` and `*.pstate.started*` (refreshed for folders shared before, after a game).
- **Default**: On; Off for the big-state systems `timeline::BIG_STATES` (`options::console_default`).
- **Exit report**: `earlier: N states (… in play, … in a pause), … copied, … failed; serialize … ms (max …), … KB;
  written … in … ms each, … skipped, … removed by the caps`, and a line per state taken.

Measured with `examples/timeline_bench.rs` (300 frames in, 20 states a refresh apart, written through the writer) on
The build machine (x86-64, libretro buildbot cores of 2026-10-24/25, homebrew test images from `scripts/testroms.py` and tiny
programs of our own, so the states are those of a booted machine, not of a game in full flight):

| Core (system) | State | Serialize mean / max | Writer (state + PNG) | Guard before a measurement | 9 states |
|---|---|---|---|---|---|
| Gambatte (gb) | 34 KB | 0.00 / 0.02 ms | 0.4 ms | in play | 0.3 MB |
| Gambatte (gbc) | 58 KB | 0.02 / 0.08 ms | 0.4 ms | in play | 0.5 MB |
| Nestopia (nes) | 21 KB | 0.02 / 0.03 ms | 0.6 ms | in play | 0.2 MB |
| Stella (atari2600) | < 1 KB | 0.02 / 0.03 ms | 0.4 ms | in play | 0.0 MB |
| Gearsystem (mastersystem) | 165 KB | 0.05 / 0.07 ms | 0.7 ms | in play | 1.5 MB |
| mGBA (gba) | 516 KB | 0.61 / 0.71 ms | 1.0 ms | in play | 4.8 MB |
| Snes9x (snes) | 804 KB | 0.17 / 0.35 ms | 1.0 ms | in play | 7.4 MB |
| Genesis Plus GX (megadrive) | 1012 KB | 0.08 / 0.16 ms | 1.2 ms | in play | 9.3 MB |
| PCSX-ReARMed (psx) | 4352 KB | 1.31 / 2.23 ms | 3.3 ms | in a pause (over 2 MB); then by the measurement | 40 MB |
| melonDS DS (nds) | 18769 KB | 6.57 / 12.68 ms | 14.4 ms | in a pause | 173 MB: the cap keeps 5 |

On the device a serialize costs about two to three times as much (mGBA: 0.61 ms here, a rewind capture 1.14 ms there,
[Extras](#extras-2026-10-21-same-setup-39-s-runs-title-screens-and-attract-mode)), so the 8- and 16-bit systems and the
GBA stay well inside the 4.17 ms; the PlayStation is near it (its first state waits for a pause, then its own
measurement decides). The 3D systems' states are 16 to 40 MB (the exit save above; Saturn 8.8 MB) and they are
hardware-rendered: Off by default, and in a pause only when switched on. Not measured on the device yet: the
serialize per core in play (`earlier:` lines of the log), whether a state in play ever shows as a late frame, and the
writer's card time for the big states.

### Run-ahead (`src/run_ahead.rs`, 2026-10-23)

RetroArch's single-instance run-ahead: every refresh the real frame(s) run unseen and unheard with the buttons held
now, the state is saved into memory (`Core::save_quick`), 1 or 2 more frames run with the same buttons, the last one
is the picture shown, and the state is loaded back (`Core::load_quick`). A press shows up that many frames sooner;
games react 1-2 frames after the press inside the machine, so that is the delay it hides. RetroArch's second-instance
variant (a second copy of the core, kept in step) is not built: on these cores the whole single-instance run costs
0.2-2 ms (below), so the second instance would save little and double the memory.

- **Sound.** Each refresh plays the sound of its last `real` frames, counted over the real frames and the frames
  ahead (`run_ahead::plan`): one frame's worth per refresh, as without run-ahead, so the queue never drifts. With one
  real frame that is the sound of the frame shown, as in RetroArch. A press can cut off the first 1-2 frames of the
  sound it starts.
- **GET_AUDIO_VIDEO_ENABLE** (`47 | EXPERIMENTAL`, was unanswered): frames run for nobody say "no video, no audio"; what a
  core makes anyway is dropped in the callbacks (not even copied). While run-ahead saves or loads, the answer also has
  RETRO_AV_ENABLE_FAST_SAVESTATES (the state stays in memory, same core). On ROCKNIX 20261001, snes9x and
  genesis_plus_gx ask (`POCKET_CORE_TRACE=1` logs it); gambatte, mgba, nestopia, gearsystem, beetle_pce_fast,
  stella and beetle_wswan don't, so they still draw and mix those frames.
- **SET_SERIALIZATION_QUIRKS**: the bits are libretro.h's now. `QUIRK_CORE_VARIABLE_SIZE` was `1 << 5`
  (that is ENDIAN_DEPENDENT); it is `1 << 2`. Stella says CORE_VARIABLE_SIZE (quirks 0x4), so its shorter states
  were refused before; Genesis Plus GX says PLATFORM_DEPENDENT (0x40), which made its states count as variable in
  size. The answer stays "not supported" (false) as before, so no core changes what it does. Run-ahead refuses
  INCOMPLETE and waits up to 600 refreshes for MUST_INITIALIZE.
- **When it switches itself off** (a notice over the game, a line in the log): not one of `run_ahead::SYSTEMS`, a 3D
  core, no states, INCOMPLETE states, or the RetroArch info file next to the core (`<core>_libretro.info`,
  `savestate_features`) says anything but `deterministic` (RetroArch's own rule; on ROCKNIX every light core's says
  deterministic; handy and prosystem say `serialized`); save + load over 25 % of a refresh on average (first 60
  judged refreshes); the run going over its budget in more than 12 of the last 120 judged refreshes. A refresh is
  over when its CPU time (of the game thread, so the system taking the CPU away doesn't count) is over 70 % of a
  refresh (11.7 ms at 60 Hz) at the clock it ran at. Why 70 %: what has to fit before the next vsync is the run plus
  drawing and the swap; on the Mini V2 frames whose core took 9-12 ms all made their vsync (2026-10-21 table:
  60.00 fps, max 17.4 ms, 0 late in 42 s), and the GPU draws these pictures in a fraction of a millisecond. Not
  scaled by the clock: the clock (`scaling_cur_freq`/`scaling_max_freq`, read every 30 refreshes, only logged) is a
  moment, not the refresh measured, and the game thread moves between the little, big and prime cores, which differ
  in speed at the same MHz (device: cpu1 1804/1804, cpu6 710→2419, cpu7 2841 in one session). Timing is not
  judged for 180 refreshes after switching on (3 s: the start, the resume state, cold caches, the governor still
  at a low clock) and 60 after any command from pocket-ui (load, option, cheats, pause) or a gap of more than 3
  refresh periods (fast-forward, rewind, the menu); the picture shown ahead differs from the real frame that comes later with the same buttons in at least 3 and a
  quarter of the last 60 comparisons (every 4th real frame during the first 240, then during 60 of every 1800; one in a while is
  no proof: gambatte showed 2 of 237 at uCity's start, probably a stale picture while the Game Boy's screen is off);
  `retro_serialize`/`retro_unserialize` failing. Choosing another value tries again.
- **With the rest.** Not while fast-forwarding or rewinding. Rewind keeps run-ahead's state of the real game instead
  of saving its own (0.13 ms a capture instead of a serialize, Mega Drive). Slots, the auto state, the 10 s battery
  save flush and cheats all see the real game: they happen between refreshes, after the load. A slot's preview picture
  is the one shown, 1-2 frames ahead of the state. Any command from pocket-ui (a load, an option, cheats, the pause),
  a shortcut or rewinding drops the pictures waiting to be compared (`RunAhead::forget`). The HUD's FPS and EMU counts
  and the exit report's emulated frames count real frames only.
- **Setting.** Options > Extras > Run-ahead, `run_ahead` = `off` / `1` / `2` in the game-settings files, so per game >
  per console > default (Off) like the rest; Settings > Playing > Performance & fan has no run-ahead (its defaults don't cover
  it). The row is shown only for `run_ahead::SYSTEMS` (`options::applies`). `pocket-core --run-ahead N` overrides
  the setting (benches).

**Measured** (virtual device, 2026-10-23, M1 Max colima, the host busy with other agents' containers; core ms are
CPU time of the game thread per refresh from `examples/run_ahead_bench.rs`, 1200 refreshes after 300 of boot, no
drawing; VIRTUAL-DEVICE.md found the emulation itself to cost about the same as on the Mini V2):

| System, core, game | state | off | run-ahead 1 | run-ahead 2 | save + load | FF 4x all made → 3 skipped |
|---|---|---|---|---|---|---|
| GBC, gambatte, uCity | 178 KB | 0.10 ms | 0.24 | 0.33 | 0.02 ms | 0.40 → 0.38 |
| GBA, mgba, MegaMan & Bass | 396 KB | 0.54 | 1.47 | 2.00 | 0.23 | 2.39 → 2.32 |
| NES, nestopia, Nova the Squirrel | 45 KB | 0.64 (median 0.43) | 1.26 (0.88) | 1.63 (1.31) | 0.03 | 2.04 → 1.72 |
| SNES, snes9x, Contra III | 804 KB | 0.38 | 0.79 | 1.06 | 0.08 | 1.65 → 1.22 |
| Mega Drive, genesis_plus_gx, Old Towers | 1012 KB | 0.40 | 0.77 | 1.03 | 0.01 | 1.67 → 1.21 |
| Master System, gearsystem, sms-test | 177 KB | 0.34 | 0.72 | 1.04 | 0.07 | 1.20 → 1.18 |
| Game Gear, gearsystem, gg-test | 177 KB | 0.25 | 0.59 | 0.93 | 0.07 | 0.99 → 0.99 |
| PC Engine, beetle_pce_fast, Reflectron | 78 KB | 0.21 | 0.64 | 0.87 | 0.16 | 0.83 → 0.80 |
| Atari 2600, stella, a2600-test | 1 KB (variable) | 0.43 | 1.01 | 1.45 | 0.04 | 1.74 → 1.73 |
| WonderSwan, beetle_wswan, ws-test | 84 KB | 0.16 | 0.41 | 0.56 | 0.03 | 0.67 → 0.68 |

Means; every comparison came out the same (239 of 239 per run). Run-ahead costs about 1 + N frames plus the
save and load; the dearest, mgba at 2 frames, is 2.0 ms of a 16.7 ms refresh. Skipping the picture and sound
saves only where the core asks (snes9x -26 %, genesis_plus_gx -28 % in fast-forward), elsewhere only the frame copy.

Through the whole of pocket-core (`vdev.sh run-core <core> <rom> --bench 10 --run-ahead N`, wall time per refresh,
with llvmpipe drawing on the same CPUs, so higher and noisier): gambatte 0.22 / 0.44 / 0.75 ms, mgba 0.93 / 2.17 /
2.43, nestopia 0.99 / 2.09 / 2.90, snes9x 1.09 / 1.94 / 2.50, genesis_plus_gx 0.93 / 1.18 / 1.18, gearsystem SMS
0.53 / 1.15 / 1.53, GG 0.43 / 1.11 / 1.79, beetle_pce_fast 0.46 / 0.97 / 1.46, stella 0.95 / 1.86 / 2.66, beetle_wswan
0.31 / 0.74 / 0.95 (off / 1 / 2). Audio underruns did not go up with run-ahead (they follow the host's load).
In an earlier run that judged the budget by wall time, nestopia and genesis_plus_gx at 2 frames switched themselves
off on the busy host (hiccups of 20-30 ms); judged by CPU time they stayed on.

**First device run (2026-10-23, F-Zero: Maximum Velocity, mGBA, ondemand governor):** with the first version
(budget 50 % of a refresh in plain CPU time, 30 refreshes of warm-up) it switched itself off ~1 s after every start
at 1 and at 2 frames: 7.2 ms of CPU per refresh on average, save 0.7-0.9 ms + load 0.3 ms, every comparison the same.
The Mini V2 runs these cores 5-10 times slower than the container at the clocks ondemand picks for them (the
2026-10-21 table above: mGBA 2.5-3.2 ms a frame), and ondemand keeps a 20-45 % load at a low clock. Hence the settling above (and a clock
scaling, replaced after the second run). The exit log now also has `without run-ahead: N refreshes, CPU x ms each, clock y %`
(every refresh run without run-ahead, a clock_gettime pair each) and the clock share on the run-ahead line.

**Second device run (same game, budget judged at the highest clock, 50 %):** still off after the 3 s: 1 frame
7.29 ms a refresh (max 11.3), 2 frames 8.53 ms (max 13.7), without run-ahead 4.10 ms (max 10.9), the clock read at
36-71 % of the highest. Nothing on the run-ahead path costs more than the frames themselves: F-Zero simply costs mGBA
about 4 ms a frame at the clocks ondemand picks. What was found and changed:
- mGBA makes a whole state to tell its state size, and every save asked for it first (half of the 0.7 ms save).
  `Core::save_quick` now keeps the buffer's size and asks only when the core refuses it (RetroArch's run-ahead also
  sizes its buffer once). Container: mGBA save 0.145 → 0.090 ms, the size asked once per session.
- The pictures compared were hashed byte by byte, the shown one every frame for the first 4 s: now eight bytes a
  step and every 4th frame. Container: 0.001-0.007 ms a refresh.
- The exit line has the time per part: `per refresh: real frames, ahead, check, host (sound, achievements), save
  (state size asked n×), load, other`. Container, mGBA, 2 frames: real 0.91, ahead 1.37, check 0.007, host 0.002,
  save 0.267, load 0.176, other 0.007 ms: the frames are the cost, the rest is small.
- The budget: 70 % of a refresh in CPU time at the clock it ran at (above), not 50 % scaled to the highest clock.
  The device's numbers: 1 frame mean 7.3, max 11.3 (all under 11.7); 2 frames mean 8.5, max 13.7 (a few over).

**Third device run (F-Zero: Maximum Velocity, mGBA, Balanced, ondemand, budget 70 % at the clock it ran at):**

| | refreshes | CPU per refresh (max) | real / ahead frames | save / load | clock | result |
|---|---|---|---|---|---|---|
| off | 828 | 2.99 ms (10.3) | | | 50 % | |
| 1 frame | 1253 | 6.17 ms (18.2) | 2.97 / 2.52 ms | 0.389 / 0.329 ms, size asked once | 49 % | stays on, over budget 1, 59 of 59 compared the same |
| 2 frames | 423 | 8.20 ms (12.9) | 3.08 / 4.47 ms | | 75 % | switches itself off: 13 of 120 over 11.7 ms |

So GBA on the Mini V2: 1 frame; 2 frames are too much for heavier games and switch themselves off after a few
seconds (the user guide and the Extras row say so). The light 8- and 16-bit systems cost less per frame and are
expected to manage 2 frames; not yet measured on the device.

Still to do on the Mini V2: `run_ahead_bench` there (built with
`cargo zigbuild --release --target aarch64-unknown-linux-gnu.2.38 --example run_ahead_bench`; add `paced` to run at
60 Hz so the governor behaves as in play), the Battery profile at 2 frames, and how 1 and 2 frames feel in play.

### CPU and fan on ROCKNIX (SM8250)
ROCKNIX sets the CPU per game with `set_cpu_gov <governor>` (writes `cpufreq/policy{0,4,7}/scaling_governor`; `ondemand`
is its default, `runemu.sh` switches to `performance` while a game loads) and the cooling profile with
`set_setting cooling.profile quiet|moderate|aggressive` + `systemctl restart fancontrol`. The PWM fan
(`hwmon/pwmfan`, `pwm1`) is a thermal cooling device of the `cluster1-thermal` zone; the profiles only differ in the
temperatures of that zone's six `active` trip points (`trip_point_2..7_temp`), which `bin/fancontrol` writes. We do
the same writes ourselves, and put the originals back when the game ends:

| Setting | What is written |
|---|---|
| Performance: Auto (default; "Balanced" until 2026-10-23, value `auto`, `balanced` still read) | nothing; the system's governor and limits stay |
| Performance: Battery | `scaling_max_freq` of each cluster = lowest step at or above half its top speed (little 979 MHz, big 1286 MHz, prime 1517 MHz) |
| Performance: Max (value `performance`) | `scaling_governor` = `performance` on all three clusters |
| Fan: Auto | nothing; the system's cooling profile applies |
| Fan: Quiet (Games: fan's start value) | trips 52 / 62 / 69 / 75 / 80 / 85 °C (ROCKNIX "quiet") |
| Fan: Cool | trips 40 / 48 / 55 / 61 / 66 / 71 °C (ROCKNIX "aggressive") |

The originals are kept in `/tmp/pocket-core.restore` while anything is changed. If pocket-core is killed, pocket-ui
runs `pocket-core --restore-system` when the game process ends (and at its own start) and puts them back; that
does nothing while another pocket-core is running (or anyone holds the CPU's lease, below).

**The note first, and kept until everything is back** (`power.rs`, review SOL 2026-10-24 #2). Every user of
`power::Power` (pocket-core, a port's session, PS2/GameCube/Wii/3DS, Fan outside games) writes the whole restore plan
atomically (`fsutil::write_atomic`) before the first sysfs write: for a port or a big game every file it will write
(each cluster's limit and governor, the GPU's governor) with what was there. When the note can't be written
(`/tmp` full) nothing is changed; the game runs with the system's settings and the log says
`power: performance battery not applied: without its restore note it couldn't be undone after a crash`. A restore
deletes the note only when every write back worked (`power: not everything went back; … stays for the next try`
otherwise). A note that can't be read is never just deleted: `recover` renames it to `<note>.bad` and puts every
cluster on `schedutil` with `scaling_max_freq` = `cpuinfo_max_freq`, with one line `power: <note> can't be read (…);
kept as <note>.bad; the CPU goes back to schedutil at full speed`.

**One owner of the CPU at a time** (`power::Lease`, review SOL 2026-10-24 #1). Whoever changes the CPU for a game holds
an exclusive `flock` on `/tmp/pocket-perf.lock` from before it notes the originals until it has put them back:
pocket-core from just before `Power::system().recover()` at its start until `power.restore()` at its end (dropped
before the hand-over line, so a fast switch's next game starts without waiting for the old process's teardown), a
port's session thread from before its note until it has put everything back, a big game's `standalone::set_performance`
until `standalone::exited`. The next game takes the lease before it notes the CPU (`Lease::for_game`, up to 3 s; in
practice the restore before it takes milliseconds, and the log says `pocket-core: waited N ms for the game before to
put the CPU back` when it was 5 ms or more), so it never notes the caps of the game before as the system's, and an
old session can never write over a newer one's settings or note. The kernel drops the lock with a process that dies.

Only the zone's `active` trips are written (`Power::active_trips` filters on `trip_point_N_type == active`); `hot`,
`critical` and `passive` trips are never touched (test `hot_critical_and_passive_trips_are_never_written`, the fake
sysfs has one of each).

### Settings > Performance & fan (`src/perf_fan.rs`, `src/perf_fan_page.rs`, `src/app_perf_fan.rs`)

**Game defaults.** `<data>/performance.json` (`perf_fan::Prefs`, written with `fsutil::write_json`) holds
`games_performance`, `games_fan` and `outside_fan`, each with the values of `options.rs` (`battery|auto|performance`,
shown as Battery, Auto, Max; `quiet|auto|cool`); unknown values fall back alone (`settings::lenient`). `balanced`, Auto's
name until 2026-10-23, is read as `auto` everywhere (`#[serde(alias)]`, `Performance::from_value`, and
`GameSettings::load` for the game and console files). A missing `games_fan` is Quiet (`perf_fan::games_fan_default`). `options::GameSettings::load` reads the first
two as `globals`, and `GameSettings::index` resolves a key as: the loaded file's value (the game file if there is one,
else the console file; a game file still replaces the console file entirely, as before) → `globals` → `console_default`
→ the setting's default. pocket-core (`Extras::new`, `reload_settings`), the pause menu (shown value, stepping, Restore
defaults) and `effective()` all go through `index`, so nothing else needed to change, and existing settings files
read as before. A file that saved `"performance": "balanced"` explicitly is an explicit choice (Auto) and wins over the
page. Quick settings change the same `Prefs` (`quick_profiles`, `app_quick_profiles`); over a paused pocket-core game
without its own value they send `set performance|fan <value>` at once.

**Smooth fan** (`smooth_fan`, on by default; a file without the key reads as on, only `false` turns it off) replaces
the trip profiles below with pocket-ui's own fan curve in the menus and in every game: [FAN-CURVE.md](FAN-CURVE.md).
`App::perf_fan_sync` then keeps `FanKeeper` on Auto and hands `fan_curve::Plan { on, settling, fan, game, game_id }` to
the curve's thread; pocket-core games get `POCKETUI_FAN_REQUEST` and write their choice instead of calling
`Power::set_fan` (`Extras::fan_request`). Switching it off waits for the curve to hand back (`release_and_wait`)
before `FanKeeper` notes any trips, and `Power::set_fan` refuses to note trips that hold the curve's backstop.

**Fan outside games (Smooth fan off).** `perf_fan::FanKeeper` is a `Power` with its own note, `/tmp/pocket-ui-fan.restore`
(`power::OUTSIDE_RESTORE_FILE`). `App::perf_fan_sync` (every `about_to_wait`) applies
`perf_fan::outside_fan(choice, Moment { in_game, asleep, settling })`: the choice only while no game runs, sleep is
`Awake`, and no clean-up is under way; Auto otherwise. Auto is `Power::restore` (system trips back, note deleted), so
the note exists only while our profile is in place, and every change from Auto notes the trips found at that moment.

| Event | What happens to the fan |
|---|---|
| `launch` / `start_streaming` | `perf_fan_before_game` restores the system's trips synchronously, before `standalone::launch` and the spawn, so pocket-core's `note_fan` sees the system's trips |
| pocket-core game ends | `restore_system_then_fan`: `settling += 1`, `pocket-core --restore-system` on a thread, then `Event::Settled` (`settling -= 1`) and the choice is applied again; never on top of a killed game's trips |
| RetroArch / standalone game ends | applied again at the next `about_to_wait` |
| light or deep sleep, power off | `sleep.state() != Awake`: system's trips; back on wake |
| pocket-ui starts (Linux) | `restore_system_then_fan` as above; `FanKeeper`'s first `sync` also recovers a left-over note itself |
| pocket-ui exits | `perf_fan_exit` after `run_app` |
| pocket-ui crashes or is killed | `deploy/run-pocket-ui.sh`'s `clean_up` runs `pocket-core --restore-system`, which now recovers `/tmp/pocket-ui-fan.restore` too (before its "another pocket-core runs" check) |

**Readout.** While the page is open, a thread reads `perf_fan::Readout::read("/", battery::dir())` once a second (the
hottest `thermal_zone*/temp`, `hwmon` `pwmfan` `fan1_input`/`pwm1`, the fan zone's first active trip, each cpufreq
policy's `scaling_cur_freq`/`scaling_max_freq`, the GPU's devfreq `cur_freq`, the battery's `voltage_now` ×
`current_now`) and sends `UserEvent::PerfFan(Event::Readout)`. It reads nothing while the Pocket sleeps and stops when
the page closes. The same sysfs helpers as `sensors.log` (`logs.rs`).

**Screenshots:** `pocket-ui --snapshot perf[:<row>][:waiting] out.png` (made-up readings; Light with
`"appearance": "light"` in `POCKETUI_DATA/settings.json`); `--snapshot extras:4` shows the pause menu's Performance row.

### PortMaster ports: performance and fan (`src/port_perf.rs`)

Ports have no pause menu, so their Extras are a page of their card: START on a port's card opens **Performance & fan**
(`port_perf_page`, `app_port_perf.rs`), saved in `game-settings/ports/<port>.json` with the same keys and choices
as the pause menu's Extras (`performance`, `fan`) and in the same order as `options::GameSettings`: the port's own,
`game-settings/ports.json` (not written by any page), Settings → Playing → Performance & fan, the system's. Every port's
choice and note are held in memory (`port_perf::Store`, read on a thread at pocket-ui's start; a change goes to
memory first and to the card through the card writer, `fsutil::write_json_soon`), so the card, the page and a port's
start read no file on the UI thread. "Use the defaults" drops both keys from the port's file (`{}` when nothing else
is in it: nothing of its own).

What ROCKNIX's `runemu.sh` does around a port (read on the device image, 2026-10-23), and where ours goes:

| runemu.sh | What it writes | Ours |
|---|---|---|
| first line: `performance` | every cluster's `scaling_governor` = `performance`, the memory bus too | - |
| `cooling.profile ports <launcher>` set: `set_setting cooling.profile …` + `systemctl restart fancontrol` | the fan zone's trips (Smooth fan takes them over again) | the port's fan goes to the curve's plan before the start (`Plan::own`: wins over this switch) |
| `cpugovernor ports <launcher>` (else the system's) | every cluster's governor | - |
| `eval ${EMUPERF} bash <launcher>` | the port runs | `port_perf::start` waits for that process (`port_perf::Tracker`: `bash <launcher>`, also behind `taskset`, an AppImage itself; not runemu.sh, `sed`, `chmod`; or, for a launcher that `exec`s its game before a look every 250 ms sees it, a direct child of runemu.sh that is none of its helpers, isn't handed the launcher's path and lives 3 s), then takes the CPU's lease and applies `Power::set_port` |
| after the port: `performance`, the system's cooling profile, GPU level, threads; `quit`: the system's governor | governors, trips | `game_exited` → `port_perf::ended` (the UI thread only sets the session's end) → its thread puts back what is still as we wrote it, then lets go of the lease |

runemu.sh never writes `scaling_max_freq`, so Battery's caps (every cluster at the lowest step at or above half its top
speed, as for pocket-core; governors untouched) hold through all of its governor switches and come back exactly.
Performance is the standalone emulators' (`set_standalone`): `performance` on the big and prime clusters and the GPU's
devfreq, applied after runemu.sh's `cpugovernor` line, and left alone at the end once runemu.sh has put its own
governors back. The note is `/tmp/pocket-ui-port.restore` (with the group in `/tmp/pocket-ui-port.pgid`, written
before the note): pocket-ui's start (`port_perf::recover`, once the group is gone and no game holds the lease) and
`pocket-core --restore-system` (the launcher's clean-up, after `stop_game`) undo a crash. With Smooth fan off a port
keeps the system's cooling (runemu.sh's fancontrol restart at its end would race a trip profile of ours).

**The session's order** (review SOL 2026-10-24 #1, #10). A port's session is one thread: it finds the port's own
process, takes the lease (waiting for the game before to put the CPU back), checks only then that the port hasn't
ended (an ended session writes nothing, not even the group file), writes the group file and the note, applies,
measures, and when the session ends or runemu.sh's group is gone puts everything back and lets go of the lease.
`ended` (the UI thread) only sets the session's end, which the thread sees at once (a condition variable, no poll to
wait out). When the profile can't be applied the log says so: `ports: <launcher>: its own process didn't show in 90 s;
performance battery not applied`, `ports: <launcher> ended before its own process was seen; …`, `ports: the CPU is
still another game's; performance … not applied`. A port found after its launcher's `exec` logs `ports: <launcher>
runs (pid N, its launcher's exec, …)`.

**Before the settings are read** (review SOL 2026-10-24 #6). `port_perf::spawn_load` reads the files with no lock held
and puts the finished store in place; `port_perf::with` only looks at memory and says None until then, never reads or
waits. In those first moments the card's Speed says "Loading…", START on the card shows "Still reading the ports'
settings…", and a port started then (On start → Continue) gets Settings' Games: fan and decides its performance on
its thread once the store is there (`Wanted::Later`).

**Busy-wait look.** `port_perf::run` reads, 30 s after the port runs and then every 10 s for twelve windows, the
`utime + stime` of every thread of the port's process group (`/proc/<pid>/task/<tid>/stat`) and the busy and idle ticks
of `/proc/stat` (a few dozen small files; the thread sleeps in between, nothing on the UI thread). A window is
"spinning" when the busiest thread used at least 0.85 of a core while everything else together used at most 0.6
cores; nine of twelve make the verdict `busy` (`Meter::verdict`), kept in `<data>/port-cpu.json` with the averages.
A demanding port (several busy cores) is not busy-waiting; a calm one isn't either. The card shows "Busy core?
Press START" until the page has been opened once (`Note::seen`), and the page then offers "Use Battery for this port"
first. Nothing is ever applied by itself.

**Screenshots:** `--snapshot port-perf[:<row>][:busy][:own]` (DEVELOPING.md's table).

Reading it back over SSH: `cat /storage/.config/pocket-ui/performance.json`; the fan zone is the one with a `pwm-fan`
cooling device (`grep -l pwm-fan /sys/class/thermal/thermal_zone*/cdev*/type`), its trips
`paste <zone>/trip_point_*_type <zone>/trip_point_*_temp`; the notes `ls /tmp/pocket-ui-fan.restore /tmp/pocket-core.restore`;
what pocket-ui did `grep 'performance:' /var/log/pocket-ui.log`. Smooth fan: `cat /tmp/pocket-ui-fan.beat
/tmp/pocket-ui-fan-curve.restore`, `grep 'fan curve:' /var/log/pocket-ui.log` ([FAN-CURVE.md](FAN-CURVE.md#on-the-device)).

## Choosing the emulator: Runs on

`src/emu_choice.rs` (model, system.cfg, writer), `src/emu_choice_page.rs` (the page), `src/app_emu_choice.rs` (the App's
side: launch resolution, opening, painting, saving). User side: [user/options.md](user/options.md#runs-on).

### ROCKNIX's mechanism (20261001, read from the source and the image)

Sources: D = `github.com/ROCKNIX/distribution/blob/20261001/projects/ROCKNIX/packages/`, E =
`github.com/ROCKNIX/emulationstation-next/blob/cada856d86e3115fbbf0bce09dd761b0ee8fa9fd/es-app/src/`.

- **What a system can run with** is es_systems.cfg's `<emulators><emulator name="…"><cores><core default="true">…`,
  generated at build time by `add_emu_core <system> <emulator> <core> <default>` (D:`virtual/emulators/package.mk`). On
  the Mini V2 image, for example: `gba` → retroarch (mgba*, vbam, vba_next, beetle_gba, skyemu, gpsp), nanoboyadvance,
  mednafen, skyemu; `nds` → melonds (melonds-sa*), drastic, skyemu, retroarch (melonds, melondsds, desmume), dsperate;
  `psx` → retroarch (pcsx_rearmed32*, pcsx_rearmed, beetle_psx, duckstation, swanstation), mednafen, duckstation.
- **Default** (E:`SystemData.cpp` `getDefaultEmulator`/`getDefaultCore`): the emulator with a `default="true"` core,
  else the first; that core, else the emulator's first.
- **The choice** lives in `/storage/.config/system/configs/system.cfg`: `<system>.emulator` / `<system>.core`
  (E:`SystemData.cpp:1883-1903`, written by `GuiMenu::popSpecificConfigurationGui`, E:`GuiMenu.cpp:6115-6180`) and per
  game `<system>["<file>"].emulator` / `.core` (E:`FileData.cpp:1437-1530`; the name is `getConfigurationName`, the
  file name with `=` and `#` removed, E:`FileData.cpp:118-125`). AUTO is no line (`SystemConf::saveSystemConf` drops
  empty and "auto" values). A name the list doesn't have is ignored; a core the emulator doesn't have falls back to
  its default.
- **Launch**: ES resolves the pair and runs `runemu.sh <rom> -P<system> --core=<core> --emulator=<emulator>`
  (E:`FileData.cpp:489-575`). runemu.sh (D:`rocknix/sources/scripts/runemu.sh`) never reads the keys: `retroarch` →
  `setsettings.sh` then `retroarch -L /tmp/cores/<core>_libretro.so` (32-bit RetroArch for gpsp, pcsx_rearmed32,
  desmume), `mednafen` → `start_mednafen.sh`, GameCube/Wii → `start_dolphin_{gc,wii}.sh`, anything else →
  `/usr/bin/start_<core without its last -part>.sh`.
- **RetroArch settings** are applied by `setsettings.sh` at every RetroArch start through `get_setting <key> <system>
  <file>` (D:`rocknix/profile.d/001-functions`): the game's key, `<system>.<key>`, `system.<key>`, `<key>`,
  `global.<key>`. `set_aspectratio` (`ratio` → `aspect_ratio_index` by its CORE_RATIOS list), `set_integerscale`,
  `set_filtering` (`smooth`), `set_shader` (`shaderset`, a path under `/tmp/shaders`, an overlay of the shipped shader
  packages and `/storage/shaders`; `--set-shader`), `set_rewind` (off for NO_REWIND), `set_runahead` (`runahead` frames,
  `secondinstance`; off for NO_RUNAHEAD), `set_autosave` (`autosave` 1–3 → `savestate_auto_save/load`). ROCKNIX ships
  per-system values in `/usr/config/system/configs/system.cfg` (`gba.ratio=3/2`, `snes.ratio=4/3`, `global.autosave=1`
  …), which set_setting copies when the file is missing.
- **Hotkeys in RetroArch**: with `system.autohotkeys=1` (default) `configure_hotkeys` keeps the pad's hotkey button
  (the DualSense profile InputPlumber presents: PS = HOME) and maps R2 to toggle fast-forward and L2 to rewind (R1/L1
  save/load, Start exits).
- **Locking**: `set_setting` takes `/tmp/.system.cfg.lock` with `wait_lock` (noclobber + PID). Its `del_setting`
  matches the key as a sed regex, so a per-game key (`gba["Game (USA).gba"].core`, a bracket expression to sed) is
  never deleted and a second line is appended; ROCKNIX's own scripts never write per-game keys, ES writes the file
  itself.

### What pocket-ui does

- **Model** (`emu_choice::resolve`): the game's choice, else the console's, else the default. pocket-core is one more
  emulator (`<system>.emulator=pocket-core`, `.core=<libretro core>`), offered where it hosts the system (`HOST_CORES`,
  `hostsys::route`; installed). The default is pocket-core there, else es_systems.cfg's default (PICO-8 without the
  official binary: RetroArch's fake08, `hostsys::default_emulator`). A pocket-core choice the game can't take (a 7z, a
  file type pocket-core doesn't open, a game in `fallback.json`) goes to the next level down; `From::Fallback` says
  so on the page. ES's checks are kept: unknown emulator names and "auto" are no choice, a missing core is the
  emulator's default. One deliberate difference: a game naming only an emulator gets that emulator's default core,
  where ES would pair it with the console's core (an invalid pair).
- **ES compatibility**: ES ignores `pocket-core` (not in its list) and shows AUTO; every RetroArch or standalone
  choice is the same in both. Choices made in ES are honoured from the first start of this version (logged:
  `emulator choices from system.cfg: …`).
- **Speed**: `emu_choice::preload` reads system.cfg's emulator/core lines on a thread at start (logged with its time);
  `App::route` then resolves with a map lookup (`resolving_costs_microseconds`: well under 200 µs per resolve with
  900 keys, in practice a few µs). If a game starts before the thread is done, the file is read then (tiny). The page
  reads system.cfg and the shipped copy when it opens and stats each choice's library or start script (logged:
  `emulator choice: page for gba opened in …`); the shader presets are found on a thread the first time it opens.
- **Writing** (`emu_choice::write_system_cfg`, on a thread): one `sh` that sources ROCKNIX's 001-functions, starts
  from the shipped file if there is none (as set_setting), takes `wait_lock`, drops the old lines by exact text match
  (awk `index`), appends the new ones, renames into place, `sync`, releases the lock. The in-memory cache is updated at
  once. **SYSTEM-CFG WRITER**: feat/rescue-path builds the one locked system.cfg writer; the main session switches
  `write_system_cfg` to it when both are merged.
- **Saving** (`EmuChoicePage::edits`): For this console writes `<system>.emulator/.core` (removed when it is the
  default) and the RetroArch keys that differ from `system./global.` (removed otherwise), and removes the game's keys;
  For this game writes only what differs from the console's result; Restore defaults removes the choice and puts the
  RetroArch keys back to the shipped file's values. Choosing pocket-core for a game in `fallback.json` takes it out.
- **Per emulator**: the pause menu, fast switching, Quit without a resume point (Remember where I quit off), run-ahead and achievements key on the
  running emulator (`Running::hosted`, `retroarch`, `standalone::active`), which is now whatever was chosen: a GBA
  game set to RetroArch gets RetroArch's menu, quit and resume; one set to standalone NanoBoyAdvance gets the "other
  emulators" menu; ARMSX2/Dolphin/Azahar keep their full support when chosen (`standalone::Emu::detect`).
- **Not done**: the full shader list (hundreds of presets; a curated list of names found on the device, plus any
  value set in ES), video filters, bezels/overlays, `secondinstance`, RetroArch settings for standalone emulators
  (their own settings), netplay, and per-game RetroArch keys for file names `get_setting` can't match
  (`emu_settings::es_findable`; the page keeps those for the console). The RetroArch rows show only while RetroArch is
  chosen, as ES shows features per emulator (es_features.cfg is not read).

## Measurements, new systems (2026-10-21, Mini V2, 20 s each, `scripts/syscheck.sh bench` / `ra`, USB connected)
Same ROM on both sides, measured from 12 s to 30 s after launch, RetroArch through ROCKNIX's `runemu.sh`
with its normal config. CPU 100 % = one core, taken over the time the CPUs were awake (`deploy/bench.sh`
now ignores time the device spent suspended or with the display off, which other sessions caused). The device
was otherwise idle (stale emulators killed first). Frame timing is pocket-core's over a 36 s run.

| System (core, ROM) | RetroArch CPU | pocket-core CPU | pocket-core frame timing | Underruns |
|---|---|---|---|---|
| Mega Drive (genesis_plus_gx, Old Towers) | 29 % | 16 % | 60.00 fps, 16.67 ms ± 0.06, max 17.3, 0 late | 0 |
| PC Engine (beetle_pce_fast, Reflectron) | 13 % | 12 % | 60.00 fps, ± 0.09, max 17.4, 0 late | 0 |
| PlayStation (pcsx_rearmed, Crash Bandicoot 2, PAL) | 16 % | 13 % | 60.00 fps display, core 50.0 fps by time, ± 0.04, max 17.0 | 0 |
| Master System (gearsystem, test ROM) | 29 % | 11 % | 60.00 fps, ± 0.10, max 17.4 | 0 |
| Game Gear (gearsystem, test ROM) | 22 % | 14 % | 60.00 fps, ± 0.11, max 17.3 | 0 |
| SG-1000 (gearsystem, test ROM) | 25 % | 18 % | 60.00 fps, ± 0.07, max 17.0 | 0 |
| WonderSwan (beetle_wswan, test ROM) | 19 % | 12 % | 60.00 fps, ± 0.05, max 17.2 | 0 |
| Atari 2600 (stella, test ROM) | 41 % | 19 % | 60.00 fps, ± 0.11, max 17.2 | 0 |

The Master System, Game Gear and Atari 2600 ROMs of these runs scrolled by themselves; `scripts/testroms.py`
now makes them move only while a button is held (for the save-state check), which changes their load a little.
The cluster frequencies were about the same on both sides in every pair (about 1.0-1.3 GHz). Test ROMs are
light, so the gap between hosts is mostly fixed cost (RetroArch's menu, GL context, shader pipeline) that
the real games carry too; the real-game rows (Mega Drive, PC Engine, PlayStation) show the same direction.
PC Engine is the one where the two are level: Reflectron's title screen is almost all background.

Repeat runs after merging main (pause menu, hotkeys, overlay) show how much the device itself varies from run to run
(clusters scale on demand): pocket-core / RetroArch CPU in %, in run order. Mega Drive 16 / -, PC Engine 15, 12, 10 /
13, 16, 16, PlayStation 12 / -, Master System 20, 13 / 29, 28, Game Gear 13, SG-1000 10, WonderSwan 9 (RetroArch 19
before), Atari 2600 34, 20, 29 / 41, 32, 24. Conclusion: clearly lighter than RetroArch on Mega Drive, Master
System, Game Gear, SG-1000 and WonderSwan; about level or slightly lighter on PC Engine and PlayStation;
**Atari 2600 is level within the noise** (19-34 % against 24-41 %), not a proven win. Frame timing stayed
at 60.00 fps, max frame 17.0-17.3 ms (two runs had one 30 ms frame), 0-2 audio underruns per 36 s.

PlayStation, swanstation instead of pcsx_rearmed (same game): pocket-core **32 %**, RetroArch **26 %**. pocket-core
only offers a software renderer (`SET_HW_RENDER` is refused), RetroArch gives swanstation OpenGL. swanstation
therefore stays on RetroArch, and pcsx_rearmed (13 % vs 16 %) is the PlayStation core on pocket-core.

## Measurements (2026-10-21, Mini V2, 30 s each, `deploy/bench.sh`, USB connected)
Same ROM on both sides, measured from 8 s to 38 s after launch (title screens and attract mode, so the load
is the same kind of scene, not identical frames). CPU 100 % = one core; GPU stayed at its minimum (305 MHz)
everywhere, temperatures 35–36 °C.

| System (core, ROM) | RetroArch CPU | pocket-core CPU | pocket-core frame timing (42 s run) | Audio underruns |
|---|---|---|---|---|
| GBC (gambatte, µCity) | 12 % | 12 % | 60.00 fps, 16.67 ms ± 0.07, max 17.4 ms, 0 late | 0 (after the start fix; 2–3 in the first 0.2 s before) |
| GBA (mgba, MegaMan & Bass) | 42 % | 22 % | 60.00 fps, 16.67 ms ± 0.10, max 17.4 ms, 0 late | 0 |
| NES (nestopia, Nova the Squirrel) | 34 % | 27 % | 60.00 fps, 16.67 ms ± 0.08, max 17.4 ms, 0 late | 0 |
| SNES (snes9x, Contra III) | 42 % | 24 % | 60.00 fps, 16.67 ms ± 0.10, max 17.5 ms, 0 late | 0 |

Frame timing of RetroArch is not observable from outside. The core itself needs 2.5 ms (GBA), 3.6 ms (NES) and
2.6 ms (SNES) per frame on average, 9–12 ms at worst.

Power could not be measured with USB connected: the charger and fuel-gauge readings contradict each
other. `bench.sh` reports watts only on battery.

### Extras (2026-10-21, same setup, 39 s runs, title screens and attract mode)
Rewind, per frame on top of the game (`retro_serialize` + delta against the previous state):

| System | state size | capture (avg / max) | history after 39 s (2340 frames) | CPU off → on | core ms/frame off → on |
|---|---|---|---|---|---|
| GBC | 178 KB | 0.20 / 0.76 ms | 0.9 MB | 12 % → 10 % | 0.88 → 0.79 |
| NES | 45 KB | 0.17 / 0.57 ms | 0.5 MB | 28 % → 35 % | 3.36 → 3.99 |
| GBA | 396 KB | 1.14 / 4.33 ms | 4.2 MB | 29 % → 32 % | 3.22 → 3.76 |
| SNES | 804 KB | 1.06 / 3.46 ms | 6.3 MB | 19 % → 40 % | 2.24 → 4.38 |

On GBA and SNES the core itself also runs about 0.5 to 1 ms slower per frame (the state copies push its data out of the CPU cache), so the real cost is more than the capture time. No
late frame and no audio underrun in any run (60.00 fps, max 17.0–17.4 ms). A minute of history stays well below
the 48 MB budget (6–10 MB). That is why Rewind is off by default. `POCKET_CORE_REWIND_CHECK=1` makes pocket-core
compare every rewound frame with the frame it showed the first time: 314–320 of 314–320 identical on all four
cores (run on the virtual device, `docs/VIRTUAL-DEVICE.md`).

Fast-forward 4x on GBA: 239 emulated frames per second at 60 shown (overlay "60 FPS  EMU 239  CPU 50%").
Performance profiles (GBA, GBC): all three keep 60.00 fps with 0 late frames and 0 underruns.

| Profile | little / big / prime average clock | GBC CPU | GBA CPU, core ms/frame (max) |
|---|---|---|---|
| Balanced (system ondemand) | 1393 / 1310 / 1128 MHz (GBA) | 12 % | 29 %, 3.22 (12.8) |
| Battery (cap 979 / 1286 / 1517 MHz) | 783 / 1186 / 1112 MHz (GBA) | 11 % | 21 %, 2.59 (12.4) |
| Performance | 1804 / 2419 / 2841 MHz (GBA) | – | 19 %, 2.04 (4.4), frame time ± 0.02 ms |

Read back from sysfs while a game runs: Battery `scaling_max_freq` 979200 / 1286400 / 1516800, Performance governor
`performance` on all three clusters, Fan Cool trips 40 / 48 / 55 / 61 / 66 / 71 °C (fan at PWM 51, 4400 rpm at 43 °C),
Fan Quiet 52 … 85 °C; after Quit governors `ondemand`, limits 1804800 / 2419200 / 2841600, trips back at the
system's quiet profile and `/tmp/pocket-core.restore` gone.

## 3D cores: hardware rendering (`src/hw_render.rs`)
- **How**: `SET_HW_RENDER` for desktop OpenGL (core/compat) and GLES 2/3; Vulkan is refused, cores then
  fall back to GL. The core gets its own GL context, sharing objects with ours, and renders into our FBO
  (colour texture + depth/stencil as asked, sized to the core's max geometry, resized on `SET_SYSTEM_AV_INFO`).
  After each `retro_run` the frame is blitted into the `Renderer`'s texture (flipped when
  `bottom_left_origin`), so scaling, sharpness and effects work as for software cores; a duped frame shows
  the last copy. Two contexts mean neither side can break the other's GL state (also covers
  `SET_HW_SHARED_CONTEXT`); cost is two `eglMakeCurrent` per frame. The core's context is current whenever
  the core runs (run, serialize, unload); ours only to copy, draw and swap.
- **Context API**: ROCKNIX builds Mupen64Plus-Next and Flycast for desktop GL (they ask GL 3.3 / 3.2 core),
  PPSSPP for GLES. When a core asks for desktop GL our own context is GL 3.3 core too (sharing works only
  within one API); the video shaders then compile as GLSL 3.30 and a VAO is bound.
- **No preferred API** (`GET_PREFERRED_HW_RENDER` answers false): RetroArch's `glcore` driver answers
  "OpenGL core", which sends ROCKNIX's GLES PPSSPP to its **software renderer** (no `EmuThread`, 212 % CPU).
  `POCKET_CORE_PREFERRED_HW=<type>` overrides it for experiments.
- **Threads**: the libretro callback state is a global behind a mutex, not a thread-local: Mupen (threaded
  renderer) and PPSSPP call input and audio from their emulation threads. RetroArch-private
  `GET_CLEAR_ALL_THREAD_WAITS_CB` (`0x800003`) is answered with a no-op; Mupen calls it unchecked in
  `retro_serialize` and crashed without it.
- **Options**: 3D cores get RetroArch's options (`config/<Core>/<Core>.opt`, else `retroarch-core-options.cfg`),
  so resolution factor, renderer and frame duping match RetroArch's. They are the lowest layer: our curated
  defaults, the user's saved `opt.*` values and the system's required options (`hostsys`) win over them, also
  after the pause menu's Save or Restore defaults.
- **Saves**: save/load from the pause menu work (N64, PSP); previews are read back from the texture.
  Auto-resume waits for 30 presented frames: PPSSPP boots on a thread and hung on a state loaded at frame 1.
- **System files**: PPSSPP's assets were copied to `/storage/roms/bios/PPSSPP/` (from
  `/usr/config/ppsspp/assets`; RetroArch uses the same system dir), the Dreamcast BIOS to `bios/dc/`.
- **No game**: an empty ROM path starts a core without content (Dreamcast BIOS menu).
- Pictures default to fit + crisp for n64/psp/dreamcast and nds (`console_default`).
- **Mesa's threaded context is off** for 3D cores (`GALLIUM_THREAD=0`, set by pocket-core before any GL
  context exists): its driver thread cost Flycast 20 points of CPU and PPSSPP a few, for nothing.
- **Sound**: 3D cores get a 48 ms queue instead of 20 ms (`audio::HEAVY_TARGET_MS`); their frame times jump
  with shader compiles and scene loads. On exit the sound fades out before the exit save stalls.
- **Pause menu**: the live Picture preview renders the copied frame with our context made current for that
  moment; the frame (bezel) and the HUD/FPS overlay draw on top as for software cores. On a desktop-GL
  context every shader of ours (picture, frame, overlay) is compiled as GLSL 3.30 core (`video::glsl_for`).

### Measurements (2026-10-21, Mini V2, ondemand governor, USB connected so no watts)
Same scene on both sides: a state saved by pocket-core is loaded in pocket-core (`load`) and, copied as
`.state`, in RetroArch (`LOAD_STATE`), then 25 s of `deploy/bench.sh` (CPU 100 % = one core). GPU busy is
the time the kernel accounts to the process's DRM clients (`drm-engine-gpu` in fdinfo); the GPU stayed at its
lowest clock (305 MHz) in every run, 38–43 °C. Run with `scripts/hwplay.sh`, which also lists every emulator
process before and after each measurement (other sessions' games running meanwhile would count).

| System (core, scene) | pocket-core CPU | RetroArch CPU | GPU busy pc / RA | pocket-core frame timing |
|---|---|---|---|---|
| N64 (mupen64plus_next GLideN64, Mario Kart 64 race start, 640×480) | 46 %, 57 % | 64 %, 60 % | 7.9 % / 6.6 % | 59.5–59.8 fps, p99 16.8 ms; 2–4 frames > 25 ms, all in shader compiles at start |
| PSP (ppsspp, Bomberman stage 1-1, 480×272) | 28 % (GLES renderer) | 212 % (software renderer, see above) | – | 60.00 fps, p99 16.7 ms, 3 late at start |
| Dreamcast (flycast, BIOS menu, no game available) | 64 %, 72 %, 81 % | 53 %, 55 %, 53 % | 5.9 % / 6.5–7.3 % | 59.8 fps, p99 16.7–16.9 ms |

Not measured: PPSSPP and Flycast **standalone**, which are ROCKNIX's defaults for PSP and Dreamcast; no
Dreamcast, Saturn or DS game is available on the device or the test disk. Audio: no underruns inside the
measured windows; pocket-core's quit (state save of 16–40 MB) and loading screens underrun briefly.

### Recommendation (first pass, superseded by the next section)
- **N64: keep RetroArch** for now. pocket-core runs it correctly (3D, input, sound, save/load, resume) at
  the same or slightly lower CPU, but the two runs scatter by ±10 points; not clearly better.
- **PSP: keep PPSSPP standalone** (the default). pocket-core beats RetroArch's PPSSPP by far, but only
  because RetroArch here falls back to software rendering; the real competitor, standalone with Vulkan,
  is still to be measured on the same scene.
- **Dreamcast: keep Flycast standalone / RetroArch.** pocket-core costs 10–25 points more CPU than RetroArch
  on the BIOS menu; the reason is open (Flycast's threaded rendering vs. our two-context switch is the first
  suspect). Only the BIOS menu could be tested.
- `HOST_CORES` stays unchanged.

## N64, PSP, Dreamcast and NDS on pocket-core (2026-10-21, second pass)
All four are now routed to pocket-core (`hostsys.rs`, `verified`), each measured against **ROCKNIX's default**
for that system: RetroArch for N64, the standalone emulators for PSP (PPSSPP, Vulkan), Dreamcast (Flycast,
Vulkan, vsync off) and NDS (melonDS, Qt, software drawing). A game that pocket-core can't start still falls
back as before (`fallback.rs`), to the system's default emulator.

### How it was measured
- Same game, same scene, same settings the default emulator gets from ROCKNIX (pocket-core reads RetroArch's
  core options for 3D cores). Mini V2, ondemand governor, USB connected (no watts), device otherwise idle.
- `scripts/hwplay.sh`, now also for standalone emulators (`SA=<process>`). Input reaches every emulator the
  same way: `vpad.py` with `VPAD_INJECT` writes into the Mini V2's own pad device, so it takes the real path
  through InputPlumber (SDL emulators ignore an unknown virtual pad).
- **CPU**: `deploy/bench.sh` (100 % = one core), per thread with `deploy/thread-cpu.sh` (`T:` step).
- **Frame times**: `deploy/frametime.c`, preloaded into every emulator (`LD_PRELOAD`), logs the moment each
  frame is handed over (eglSwapBuffers, vkQueuePresentKHR, glXSwapBuffers, or the X11 put of a software-drawn
  window); `deploy/frametime-stats.py` gives fps, sd, p99 and frames over 25 ms for the measured window. The
  same method for every host, so the numbers compare; they show when frames are handed over, not when the
  display shows them.
- **Audio**: pocket-core counts its own underruns (now grouped into bursts in the exit report). The standalone
  emulators have no such counter; PipeWire's xrun count (pw-top ERR) stayed 0 for every host in every run.
- Scenes: N64 and PSP load a pocket-core state (RetroArch the same file as `.state`); PSP standalone, Dreamcast
  and NDS are played to the scene from boot with the same button sequence on both sides (fresh memory
  card / save file each run, standalone given the extra time its real BIOS boot takes).

### Results
| System, scene | | CPU | fps | frame sd / p99 / max | frames > 25 ms | audio underruns in the window |
|---|---|---|---|---|---|---|
| N64, Mario Kart 64, race (standing, driving), 20–25 s | **pocket-core** | 38, 47, 38, 39, 40 % | 60.00 | 2.0–3.6 / 20.5–23.5 / 22.5–30 ms | 0–8 (last run 0) | 0 (48 ms queue) |
| | RetroArch | 47, 53, 51 % | 60.1 | 7.9–10.3 / 30.7–36.9 / 36–66 ms | 178–416 | (not observable) |
| PSP, Bomberman stage 1-1, 25 s | **pocket-core** | 17 %, 18 % | 60.00 | 0.8–1.3 / 19.0–19.3 / 20 ms | 0 | 0 |
| | PPSSPP standalone | 16 %, 17 % | 59.94 | 0.2 / 17.1 / 17.7 ms | 0 | (not observable) |
| Dreamcast, Sonic Adventure 2, City Escape, 20 s | **pocket-core** | 69 %, 67 %, 67 % | 60.00 | 1.5–1.8 / 20.3–21.1 / 22 ms | 0 | 0 |
| | Flycast standalone | 71 %, 64 % | 59.9 | 8.3–9.5 / 42–43 / 54–57 ms | 153–175 | (not observable) |
| NDS, New Super Mario Bros., world map, 20 s | **pocket-core** | 90 %, 92 % | 60.00 | 2.5–2.6 / 23.0–23.5 / 25 ms | 0–2 | 0 |
| | melonDS standalone | 125 %, 127 % | 60.01 | 3.1 / 24.0 / 26 ms | 7 | (not observable) |

GPU busy (DRM fdinfo): N64 7.4 % / RetroArch 7.3 %; PSP 4.0–4.6 % / 4.6–4.7 %; Dreamcast 14–15 % at 305 MHz /
standalone 36 % at 550–640 MHz (Vulkan); NDS 2 % (software renderer) / 0 %.

Reading the table: CPU is lower on N64 and NDS, level on PSP and Dreamcast. Frames are handed over steadily
by pocket-core everywhere; RetroArch's threaded video and standalone Flycast without vsync hand them over in
bursts (pairs of frames, a third over 25 ms), standalone PPSSPP is the steadiest of all and pocket-core is
within 2 ms of it at p99. Screenshots: `screenshots/play-pc-*` / `play-ra-*` / `play-sa-*` of each scene.

### What changed in the host for this
- **Dreamcast CPU** (first pass: 64–81 % on the BIOS menu): `perf` showed pocket-core's own code under 1 %;
  the time went into Mesa's GL driver, a third of it on its threaded-context thread (`gdrv0`, 13–17 %). Per
  thread, Flycast-emu 38 %, our main thread (Flycast's renderer) 30 %, gdrv0 17 %. With `GALLIUM_THREAD=0`
  83–89 % → 67–69 %, level with standalone Flycast on Vulkan (render 44 % + emu 36 % in the same window).
  `MESA_NO_ERROR=1` changed nothing. The second GL context was not the cost.
- **N64 underruns**: with the 20 ms queue, 1–2 per 25 s of driving and 15 while a 16 MB state loaded;
  with 48 ms none while driving and 2 at the load. The exit save (16–40 MB) no longer underruns: the sound
  fades first.
- **Analog pad**: Flycast lists no "analog" pad type, so it got none plugged in and showed "Memory card not
  ready". Cores without such a type now get their joypad, as RetroArch does (Flycast "Controller", Mupen
  "Controller"); PlayStation still gets the DualShock. The analog sticks reach all three 3D cores.
- **Dreamcast memory cards**: standalone Flycast keeps one card per game, `bios/dc/<game id>_vmu_save_A1.bin`.
  pocket-core now uses per-game cards too (`flycast_per_content_vmus` = VMU A1, curated) and links the core's
  name for it (`roms/dreamcast/<game id>.A1.bin`) to the standalone file when the core first opens it
  (`vfs::share_flycast_vmus`). Checked: a file saved in standalone Flycast shows up in pocket-core's file select.
- **NDS saves**: melonDS keeps `roms/nds/<game>.sav`; pocket-core now reads and writes that file for NDS
  (`HostSystem::save_ext`) instead of `.srm` (same raw 8 KB for NSMB). A `.srm` from RetroArch carries over
  on first write.
- **DraStic saves** (`dsv.rs`, `Core::use_save_extension("sav")`): a `roms/nds/<game>.dsv` copied from Android becomes
  `<game>.sav` when the game starts and neither `.sav` nor `.srm` exists. The `.dsv` is DeSmuME's format (DraStic shares
  it; `BackupDevice::ensure`/`readFooter` in desmume/src/mc.cpp): the raw save plus a 122-byte footer, the 82-byte text
  `|<--Snip above here to create a raw sav by excluding this DeSmuME savedata footer:`, six little-endian `u32` (data
  written, `padSize`, memory type, address size, memory size, version 0) and the cookie `|-DESMUME SAVE-|`. With the
  cookie, `padSize` must be the file's length minus 122 and the version 0, else the file is refused; without it the
  whole file is the raw save. Sizes 512 B to 8 MiB, powers of two, else refused (a log line `DraStic save not taken
  over: ...`). The raw bytes go through `fsutil::write_atomic`; the `.dsv` is never changed and an existing `.sav` or
  `.srm` never overwritten. `use_save_extension` returns true when it took one over and pocket-core shows the note "Took
  over the DraStic save" once (the next start finds the `.sav`). Checked by unit tests with built files; not with a
  save from a real DraStic.
- **PSP saves**: nothing to do; standalone PPSSPP and the core both use `roms/psp/PSP/SAVEDATA`.

### NDS: layout and touch
- Core: melonDS DS (`melondsds`), software renderer with its threaded renderer and JIT (the defaults). Runs
  with the built-in BIOS/firmware; no DS BIOS is on the device. `melonds` (old core) and `desmume` were not
  needed.
- The screen is 1240×1080, almost square. The two DS screens **stacked** (256×384) scaled to fit give
  720×1080, each screen 720×540 (2.8×); side by side (512×192) only 1240×465 (2.4×, what standalone melonDS
  shows, under a menu bar). So: layout 1 top/bottom, R3 cycles to side by side and to the top screen alone
  (`core_options::curation("melondsds")`). Picture default: Aspect, Crisp, no effect, no frame.
- Touch: the core's own joystick pointer. Right stick moves it (left stick stays the D-pad), R2 touches,
  L2 speeds the pointer up, L3 is the microphone. Checked: the pointer moves on the bottom screen.
  The touch panel works too, see "Touch screen" below; the stick stays as the alternative (`melonds_touch_mode`
  is `auto`: the core takes the finger when there is one, else the stick).

### Checked end to end (from the shelf, this build deployed)
Dreamcast, NDS, PSP, N64 each launched from pocket-ui: "emulator pocket-core" in the log, picture right,
auto-resume ("resumed"), pause menu over the game, Save 1 (Dreamcast, 35 MB in 45 ms) and Quit back to the
shelf. Battery saves: Harvest Moon 64's RetroArch `.srm` is read by Mupen in pocket-core (same file).

### Open problems
- Dreamcast loading screens (and the boot at 9 s) underrun in bursts of 0.1–0.5 s: Flycast's `retro_run` blocks
  for up to 150 ms while the game loads. Same core as RetroArch; whether standalone Flycast also goes quiet
  there is not observable from outside.
- RetroArch's resume states (`.state.auto`, compressed) are not taken over: the first start of an N64 game
  on pocket-core begins at the game's own save, not where RetroArch left off; from then on pocket-core resumes.
- PSP: pocket-core runs the core at the display's 60.00 Hz instead of PPSSPP's 59.94 (0.1 % fast, as for
  all cores within 5 %); frame hand-over a little less even than standalone PPSSPP (p99 19 vs 17 ms).
- NDS: the real touch panel on the device itself (see "Touch screen"), and melonDS DS's OpenGL renderer (sharper 3D at higher resolution) are not
  tried; no DSi games.
- Dreamcast on Vulkan (Flycast asks for it first) would cut more CPU; pocket-core offers OpenGL only.

## Touch screen (2026-10-22)
The Mini V2's panel is `generic ft5x06 (8d)`, `/dev/input/event4`: multitouch protocol B (5 slots), direct input,
`ABS_MT_POSITION_X` 0..1079 and `_Y` 0..1239 (the panel is portrait-native), `BTN_TOUCH`. The device tree
has no swap or invert properties, so the coordinates are the panel's own. sway turns the output with
`output DSI-1 transform 90` (ROCKNIX derives it from `/sys/class/graphics/fbcon/rotate`, which is 1), and the touch
coordinates have to be turned the same way. fbcon's 1 is FB_ROTATE_CW (the picture turned clockwise onto the panel), so
screen x = y/1239, screen y = 1 - x/1079 (`touch::Rotation::Deg270`). Until 2026-10-23 it was read the other way round
(screen x = 1 - y/1239, screen y = x/1079), and on the device every touch landed point-mirrored: a tap on the DS's top
screen reached the bottom one, a tap on the bottom one was dropped (found by the owner, Super Mario 64 DS). The virtual device has
no fbcon value and keeps its quarter turn. `POCKETUI_TOUCH_ROTATE=0|90|180|270` overrides it if a device disagrees. Both programs read the device themselves (no grab, like the pad), logic in
`src/touch.rs`; `touch::linux::open()` is the one place that picks the device.

- **pocket-ui** (`src/app_touch.rs`): taps and swipes act on what the last frame painted (`HitMap`: rows, cards, hint
  pills) by sending the same button presses the pad sends, so nothing is reachable only by touch. Never while a game
  runs, a launch fades out, the pause menu, quick settings or setup are up. A touch counts as activity for sleep
  (`Sleep::touched`) but never wakes the screen.
- **pocket-core**: the finger becomes `RETRO_DEVICE_POINTER` (the capability is advertised), and is also answered as
  `RETRO_DEVICE_LIGHTGUN` (screen x/y, trigger, off-screen) and `RETRO_DEVICE_MOUSE` (movement, left button) for cores
  that ask for those; `pocket-core` never sets a port device itself, so only cores that poll them unconditionally see
  them. The position is the finger on the picture as drawn (`Renderer::picture`, whatever Picture settings say), as
  -32767..32767 across the *frame*, which is what melonDS DS expects. A tap shorter than a frame still shows for one frame.
  A turned picture (SET_ROTATION, Screen orientation) gets the finger through the same turn (`video::frame_point`). Touch made during a pause is not the game's; a finger still down when
  the game resumes stays out until it lifts.
- **NDS layouts** (`touch::nds_bottom`): the bottom screen in the frame is worked out from the frame's size (stacked
  256 x 2 screens + gap, side by side 2 screens x 192, a single screen) and the first matching layout among
  `melonds_screen_layout1..N`: top-bottom / bottom-top, left-right / right-left, top (no touch) / bottom. A touch outside it
  is not sent. Hybrid, large-screen and rotated layouts are unknown to us and passed to the core as they are. The first
  layout of the right shape wins, so with two stacked layouts configured the one the core is really showing may be the
  other (R3 cycling doesn't say which one is current).
- Checked in the virtual device with Mario Kart DS (uinput touch screen, see docs/VIRTUAL-DEVICE.md): a tap on the bottom
  screen presses the button under the finger to the pixel (the core's cursor dot sits where the finger was), a tap
  on the top screen does nothing, and after `opt melonds_screen_layout1 left-right` the same in side by side. The right
  stick still moves the pointer. Not checked: the real panel, see docs/DEVICE-TESTS.md.

## Sega Saturn (2026-10-22)
### What ROCKNIX ships (20261001, read on the Mini V2)
| | Version | Renderer | BIOS it looks for |
|---|---|---|---|
| `yabasanshiro_libretro.so` | v2.6.8 | OpenGL ≥ 3.3 or GLES ≥ 3.0, SH-2 dynarec | `saturn_bios.bin`, `sega_101.bin`, `mpr-17933.bin` in `roms/bios` |
| `kronos_libretro.so` | v2.6.0 | OpenGL core ≥ 4.3 | `kronos/saturn_bios.bin` |
| `beetle_saturn_libretro.so` | v0.9.45.1 | software (Mednafen, no dynarec) | `sega_101.bin`, `mpr-17933.bin` |
| `/usr/bin/yabasanshiro` (standalone) | | GL, SDL | none needed (HLE BIOS) |

`yabause_libretro.info` is there without its library. es_systems.cfg: `saturn` (`.cue .chd .iso .zip`), default emulator
`yabasanshiro` / core `yabasanshiro-sa` (`start_yabasanshiro.sh`), alternatives RetroArch with `yabasanshiro`,
`beetle_saturn`, `kronos`, and `mednafen` (`ss`). `start_yabasanshiro.sh` passes a BIOS (`-b`) only when ROCKNIX's
`use_hlebios` setting is 1; by default the standalone emulator runs with its HLE BIOS. "Yabadabadoo", a new Saturn
core reported for ROCKNIX in October 2026 (first released for the Miyoo Mini), is not in this build.

### Choice: `yabasanshiro`
- Same emulator as ROCKNIX's default, built for ARM (dynarec) and GL/GLES; the one that runs here (below).
- `kronos` asks for desktop GL 4.3 core and is the heavy desktop branch; `beetle_saturn` renders in software
  without a dynarec, far beyond this CPU at full speed. Neither could be tried: both read a real BIOS, none is on the
  device.
- **BIOS**: the core refuses to start without one of its three files (`We are missing the bios, ABORTING`), even
  with its "Force HLE BIOS" option on. `hostsys` lists them (`SATURN_BIOS`), and the system is marked
  `default_without_bios`: `hostsys::route` sends a Saturn game to pocket-core only when one of them is in
  `roms/bios`, otherwise it stays on standalone YabaSanshiro as before, without an error card. A game pocket-core
  can't start goes to standalone YabaSanshiro from then on (`fallback.rs`; the card now names the emulator:
  "…runs with YabaSanshiro from now on").
- **Picture**: ROCKNIX's build asks for desktop OpenGL (`RETRO_HW_CONTEXT_OPENGL`, compat 3.0), not GLES; it got
  `FD650 / 4.6 (Compatibility Profile) Mesa 26.2.2`, depth + stencil, bottom-left origin, framebuffer 704×512
  (resolution mode "original"). 320×240 at 4:3, 60 Hz (the core always reports NTSC). Picture defaults as for the
  other disc systems (fit, crisp).
- **Pad** (`hostsys::SATURN_PAD`, the core's own layout, the same as in RetroArch; the left stick works the D-pad):

  | Saturn | A | B | C | X | Y | Z | L | R | Start |
  |---|---|---|---|---|---|---|---|---|---|
  | Mini V2 | B (bottom) | A (right) | L1 | Y (left) | X (top) | R1 | L2 | R2 | Start |

  The Controls page shows these names (the core describes its buttons; `controls::targets_for` falls back to the
  same names for a Saturn core that doesn't). The Mini V2's InputPlumber pad reports L2/R2 as buttons too
  (`BTN_TL2`/`BTN_TR2` next to the axes), so Saturn L and R reach the core.
- **Saves**: states are 8.8 MB (4 MB RAM cartridge included, the core's default), written in 0.1–0.6 s. The core
  returns no state before its first `retro_run`; auto-resume waits 30 frames anyway (3D cores). In-game saves
  (backup RAM) go to `<rom folder>/yabasanshiro/backup.bin` (8 MB); standalone YabaSanshiro keeps
  `roms/bios/yabasanshiro/backup.bin`. Not shared yet (open).

### Measurements (Mini V2, on battery, Twelve Snakes, HLE)
Twelve Snakes 3.0.1 by Slinga (MIT licence, a 12-player Snake game made with Jo Engine;
https://github.com/slinga-homebrew/Twelve-Snakes, release 3.0.1 `game.iso`/`game.cue`, sha256 of the ISO
`31498a84…ed49e7`). There is no Saturn BIOS on the device, so these runs used the core's HLE BIOS:
`deploy/saturn-check.sh` (HLE=1) puts an empty placeholder `roms/bios/saturn_bios.bin` there for the run (the core
only checks that a file exists and never reads it with HLE forced) and removes it afterwards; the option comes from a
test data dir (`POCKETUI_DATA`), the user's settings stay untouched. Standalone ran through `runemu.sh` like
pocket-ui starts it, also with HLE.

| Run | CPU (`bench.sh`) | Power | Frames |
|---|---|---|---|
| pocket-core, title screen, ondemand | 127 % | 1.68 W | own: 60.00 fps, 16.67 ms ± 0.06, max 18.7, 0 late, core 5.4 ms/frame, 0 underruns; `frametime.so`: 60.00 fps, sd 1.73, p99 20.5, max 24.2, 0 > 25 ms |
| standalone YabaSanshiro, title screen | 76 % | 3.09 W | `frametime.so`: 60.91 fps, sd 1.92, p99 17.3, max 17.7, 0 > 25 ms |
| pocket-core, title, Performance profile | 110 % | 1.79 W | 60.00 fps ± 0.25, 1 frame over 25 ms (start) |
| pocket-core, in a game (scoreboard), ondemand, 45 s | 148 % | 1.57 W | 60.00 fps ± 0.07, max 19.2, 0 late, core 8.0 ms/frame, 0 underruns |

- Per thread over the same 8 s (title): pocket-core 100.5 % (a core thread: the Saturn sound CPU's async loop, which
  spins) + 14 % main thread (Performance profile; 51 % with ondemand in a game); standalone `scsp_async` 100.5 %,
  `yaba_main` 10 %, `vdp` 9.5 %. So both spend a whole core on the same spinning sound thread; `bench.sh`'s 76 % for
  standalone doesn't match its own thread sum (121 %), which the two windows (15 s / 8 s) don't explain. Read the CPU
  column as "about level", not as a win for standalone.
- Power, measured on battery: pocket-core 1.6–1.8 W, standalone 3.1 W (standalone ran with the big cluster at
  2.4 GHz, `runemu.sh` sets the `performance` governor while a game loads).
- Twelve Snakes is a light 2D game. What a 3D commercial game costs on either host is not known.

### Checked
- Device (pocket-core FIFO commands, the same pocket-ui sends): picture, Start and the face buttons (Saturn A on
  the bottom button joins the game), pause / resume, save to slot 1 (8.8 MB, 0.1–0.6 s), quit with the exit save,
  auto-resume on the next start ("resumed", back in the game at the quit point). Screenshots
  `screenshots/sat-device-*.png`.
- Virtual device, from the shelf with this build: without a BIOS file the game starts standalone YabaSanshiro
  (`launch saturn … (emulator yabasanshiro, core yabasanshiro-sa)`, title screen in `sat-vdev-standalone-nobios.png`);
  with the placeholder BIOS and HLE it starts on pocket-core. Pause menu (HOME): Save 1 with the cursor on
  "Battle Royale", cursor moved up to "Free For All", Load 1: the cursor is back on "Battle Royale"
  (`sat-vdev-c1/c2/c3.png`); Quit back to the shelf; started again: resumed on "Battle Royale"
  (`sat-vdev-resumed.png`). Controls page with the Saturn names (`sat-vdev-controls.png`), Emulator page with the
  core's options (`sat-vdev-emulator.png`).
- On the device, a slot loaded during a running round of Twelve Snakes did not visibly go back (clock, score and
  snake stayed as they were), while the same load on the menu screens, on the virtual device and at auto-resume did.
  The game's clock may come from the Saturn's real-time clock (the host's time) rather than from RAM; not settled.

### Pending on the device (it was in use; run when it is free)
```sh
# homebrew and scripts to /storage/pocket-test (once), then the pocket-core build of this branch:
scp "Twelve Snakes.iso" "Twelve Snakes.cue" root@<device>:/storage/pocket-test/saturn/
scp deploy/syscheck-play.sh deploy/saturn-check.sh deploy/bench.sh target/aarch64-unknown-linux-gnu/release/pocket-core root@<device>:/storage/pocket-test/bin/
# in-game load: save, let the round go on, load (watch the snake, not the clock)
scripts/devlock.sh saturn ssh root@<device> 'cd /storage/pocket-test/bin && HLE=1 BIN=/storage/pocket-test/bin/pocket-core sh saturn-check.sh "/storage/pocket-test/saturn/Twelve Snakes.cue" "wait 10" "press START 200" "wait 2" "press B 200" "wait 2" "press B 200" "wait 2" "fifo pause" "fifo save 1" "shot a" "fifo resume" "wait 4" "fifo pause" "fifo load 1" "fifo resume" "shot b" quit'
```
With a real Saturn BIOS in `roms/bios`: start a game from the shelf (it then goes to pocket-core by itself), HOME,
Save, Load, Quit, start again; and the same bench against standalone with that BIOS (`HLE=` empty).

### Open
- A real BIOS (non-HLE) run and a commercial game: neither was available.
- Sharing the backup RAM with standalone YabaSanshiro (`roms/bios/yabasanshiro/backup.bin`), as done for Flycast's
  memory cards; whether both write the same format is unchecked.
- An `.iso` next to its `.cue` shows up as a second game on the shelf (as `.bin` + `.cue` for other systems).
- Core options are the core's own (no curation yet); "Force HLE BIOS" is visible on the Emulator page.
- Yabadabadoo, once ROCKNIX ships it: one line in `hostsys.rs` to try it.

## DOS, ScummVM, PICO-8 and TIC-80 (2026-10-22)
### What ROCKNIX ships (20261001, es_systems.cfg and /usr/lib/libretro of the image)
| System (folder) | Shelf path, extensions | ROCKNIX default | pocket-core core | Save states |
|---|---|---|---|---|
| DOS (`pc`) | `roms/pc`: `.com .bat .exe .dosz` | RetroArch `dosbox_pure` | `dosbox_pure` (2.8 MB) | yes |
| ScummVM (`scummvm`) | `/storage/.config/scummvm/games`: `.sh .svm .scummvm` | standalone `scummvm-sa` | `scummvm` (121 MB, GL) | none (size 0) |
| PICO-8 (`pico-8`) | `roms/pico-8`: `.p8 .png` | `start_pico8.sh` (the official binary the user adds) | `fake08` | not usable, see below |
| TIC-80 (`tic-80`) | `roms/tic-80`: `.tic` | RetroArch `tic80` | `tic80` | not usable, see below |

### How each one is wired (`src/hostsys.rs`)
- **DOS**: the shelf also lists `.zip` (DOSBox Pure's main format, missing from ROCKNIX's list) and folders in
  `roms/pc`, but not `<game>.pure.zip`, the core's own copy of what a game wrote to its disk. A folder or zip with one
  program starts it; with several, DOSBox Pure's start menu picks. Curated options: `dosbox_pure_mouse_input=pad`
  (touch screen as a touchpad: drag moves, tap clicks, two-finger tap right-clicks), `dosbox_pure_on_screen_keyboard=keyboard`
  (L3 opens the on-screen keyboard; start menu and pad mapper are tabs of it), `dosbox_pure_aspect_correction=true`
  (4:3 instead of 1.6). The left stick goes to the core (`controls::ANALOG_SYSTEMS`). DOSBox Pure's state size grows
  while a game runs (364903 bytes saved, 391727 asked a few seconds later) and the core doesn't say so
  (SET_SERIALIZATION_QUIRKS); `HostSystem::states_vary` lets shorter states through, as RetroArch passes any file.
  The core runs at 70.086 Hz: scheduled by time.
- **ScummVM**: ROCKNIX's list holds standalone ScummVM's arguments (`--path="/storage/roms/scummvm/FOTAQ" queen`,
  written by "Scan ScummVM Games"), which the libretro core rejects ("Game not found"); `hostsys::content_path` gives
  the core the folder, where it detects the game itself. Folders in `roms/scummvm` that no listed file names are games
  too (no scan needed); `.sh` files in the list stay with runemu. The core reads `roms/bios/scummvm.ini` and looks for
  engine data only in `roms/bios/scummvm/extra`, which ROCKNIX doesn't have, and Beneath a Steel Sky needs `sky.cpt`.
  `hostsys::prepare_scummvm` sets `extrapath` and `themepath` to `/usr/share/scummvm` (ROCKNIX's copy for standalone:
  engine data, themes, virtual keyboard) and `savepath` to `/storage/.local/share/scummvm/saves` (standalone ScummVM's
  default; ROCKNIX's ini sets none) where they are missing or name no folder; other keys and game sections stay.
  Curated options: `scummvm_pointer_device=pointer` (a tap clicks where the finger is), A left click, B right click,
  L1 `.` (skip a line); the core's own layout for the rest (D-pad and sticks move the cursor, X F5, Y Escape, Select
  virtual keyboard, Start ScummVM's menu, R1 right click, R2 slow cursor). Hardware acceleration stays on (OpenGL 3.0
  compat). Fallback: standalone ScummVM.
  Picture: the core tells the shelf the game's own pixels (`SET_GEOMETRY` 320x200, aspect 1.6) and the launcher's
  1280x720 only at load; its `scummvm_gui_*` options and `scummvm.ini`'s `stretch_mode`/`aspect_ratio` change nothing
  in a running game (tried: `stretch_mode=fit`, `aspect_ratio=true` gave the same 960x600 picture). So pocket-core
  decides: `hostsys::display_aspect` shows 320x200 and 640x400 games at 4:3 (monitors stretched 200 lines to 240),
  and `options::console_default` scales ScummVM to fit, crisp, instead of Native (3x = 960 of 1240 px, black all
  round). Result on the virtual device: 1240x930, centred.
- **PICO-8**: `HostSystem::official`. With `pico8_64` where `start_pico8.sh` runs it (`roms/pico-8/aarch64/` when that
  folder exists, else `roms/pico-8/`), `route` leaves the game with ROCKNIX's default (standalone, Continue/Quit only);
  otherwise fake08 on pocket-core, and `hostsys::default_emulator` makes RetroArch's fake08 the default and fallback
  (start_pico8.sh would fail without the binary). fake08 maps RetroPad A to X, B to O.
- **TIC-80**: the core refuses to load without `RETRO_ENVIRONMENT_SET_FRAME_TIME_CALLBACK` ("Failed to set frame time
  callback"); `libretro.rs` takes it and calls it with the core's reference frame time before each frame.
  `tic80_pointer_device=touchscreen`: the touch screen is the TIC-80 mouse (absolute), not the relative mouse.

### Save states that don't hold the game
- **fake08**: a state is a header, the size of the cart's Lua state, that state (eris `persist_all`), PICO-8 RAM and
  audio. ROCKNIX's build writes the Lua size as 0 for every cart tried (Pico Janitor and a 15-line test cart,
  `got lua state size 0` in the trace), so a load puts the RAM back and nothing of the game: the test cart's dot and
  frame counter stayed where they were. The same core in RetroArch.
- **tic80**: 1 KB, the cart's persistent memory; after a load Tetris stayed exactly as it was.
- **scummvm**: `retro_serialize_size` is 0.

For these three `HostSystem::no_states` switches states off (`Core::disable_states`: no slots, no auto-resume, no
rewind) and Save in the pause menu says why ("No save states for ScummVM: Start opens its own Save and Load", "No save
states for PICO-8: carts keep their own saves", the same for TIC-80). The menu still lists Save and Load for them.

### libretro changes found on the way
- `GET_VARIABLE_UPDATE` answered true on the first frame (declaring options counted as a change). ScummVM then rereads
  its settings through its system object before its emulation thread has created it, and crashed in `retro_run`
  (gdb in the virtual device: a dynamic_cast of a null `g_system`). RetroArch answers false there, and so does
  pocket-core now: what the core read while loading is no update (`Core::load` clears the flag after `retro_load_game`).
- `Core::has_states`, `Core::disable_states`, `Core::states_vary_in_size`, the frame time callback (above).

### Checked on the virtual device (`VDEV_NAME=dosscumm`, llvmpipe, so no performance numbers)
The ScummVM core (121 MB) is not in the virtual device's image, which drops cores over 20 MB; it was copied in from the
ROCKNIX image together with `/usr/share/scummvm`.
- DOS from the shelf: zip and folder listed, the folder started (4:3), pause menu, Save 1, played on, Load 1
  (`scripts/shotdiff.py`: 9.73 after playing on, 0.29 after the load), quit, auto-resume ("resumed"). L3 opened the
  on-screen keyboard; a tap pressed the key under the pointer, a drag moved the pointer without clicking.
- ScummVM from the shelf: Flight of the Amazon Queen (ROCKNIX file) and Beneath a Steel Sky (ROCKNIX file, and as a bare
  folder) start and play. Taps on ScummVM's menu buttons (Options, About, Save) click them; A on "Close" closed the
  dialog; Y skipped the intro; L1 skipped dialogue. Save in the pause menu shows the message.
- PICO-8: Pico Janitor on fake08 from the shelf, A and B work. With a placeholder `roms/pico-8/pico8_64` the shelf
  starts `emulator pico-8, core pico8` (runemu.sh, which the virtual device can't run); without it pocket-core.
- TIC-80: Tetris from the shelf, pause menu, the message on Save.

Test content, all free to redistribute:
| Game | Source | Licence |
|---|---|---|
| PSR Invaders 1.1 (Paul S. Reid, 1995), `INVADERS.COM` sha256 `f9cf15ce…bcb152b` | FreeDOS archive, gitlab.com/FreeDOS/games/psrinvad | "Game, Source Code, and Documentation are Freely Distributable" (INVADERS.DOC); FreeDOS LSM: Open Source |
| Flight of the Amazon Queen, floppy (`FOTAQ_Floppy.zip`, sha256 `2e59de85…fde761e0`) | downloads.scummvm.org/frs/extras | freeware; licence in its readme (free distribution with the readme intact) |
| Beneath a Steel Sky, floppy 1.3 (`BASS-Floppy-1.3.zip`, sha256 `d0bac1bd…51078818`) | downloads.scummvm.org/frs/extras | freeware; same licence |
| Pico Janitor v1.1 by PeterPlaty (`picojanitor-1.p8.png`, sha256 `77a8d158…14fae7c1`) | lexaloffle.com/bbs/?pid=152069 | CC4-BY-NC-SA, as marked on the BBS |
| Tetris by AlKau (`demos/tetris.lua`, converted with `scripts/tic80cart.py`) | github.com/nesbox/TIC-80 | MIT (the TIC-80 repository) |

### Still to check on the Mini V2
Frame rate, CPU and power; the real touch panel; L3 on the real pad; ScummVM's GL renderer on Freedreno; an in-game
ScummVM save landing in standalone's folder and loading in standalone ScummVM; a DOS game that uses the mouse; a cart in
RetroArch's fake08 as the fallback. Steps in docs/DEVICE-TESTS.md.

## More systems (2026-10-23)
Branch `feat/more-systems`. Every system here keeps ROCKNIX's default core (es_systems.cfg 20261001), so battery saves
stay where they were and switching back to RetroArch under Runs on keeps them.

### What ROCKNIX ships and what pocket-core does with it
| System (folder) | ROCKNIX default | pocket-core | BIOS in `roms/bios` | Status |
|---|---|---|---|---|
| Famicom, NES hacks (`famicom`, `nesh`) | RetroArch `nestopia` | `nestopia` | none | **on pocket-core** (vdev) |
| Super Famicom, SNES hacks (`sfc`, `snesh`) | RetroArch `snes9x` | `snes9x` | none | **on pocket-core** (vdev) |
| Game Boy (Color) hacks (`gbh`, `gbch`) | RetroArch `gambatte` | `gambatte` | none | **on pocket-core** (vdev) |
| GBA hacks, GBA Video (`gbah`, `gbav`) | RetroArch `mgba` | `mgba` | none | **on pocket-core** (vdev) |
| Famicom Disk System (`fds`) | RetroArch `nestopia` | `nestopia` | `disksys.rom` | **on pocket-core** (vdev, stand-in BIOS) |
| SuperGrafx (`supergrafx`) | RetroArch `beetle_supergrafx` | `beetle_supergrafx`, HuCards only | none (CD games: System Card, they stay on RetroArch) | **on pocket-core** (vdev) |
| Virtual Boy (`virtualboy`) | RetroArch `beetle_vb` | `beetle_vb` | none | **on pocket-core** (vdev) |
| NAOMI, NAOMI 2 (`naomi`) | standalone `flycast-sa` | `flycast` | `dc/naomi.zip` or `dc/naomi2.zip` | wired, not verified |
| Atomiswave (`atomiswave`) | standalone `flycast-sa` | `flycast` | `dc/awbios.zip` | wired, not verified |

"vdev" means checked in the virtual device only (`VDEV_NAME=msy`, llvmpipe, so no performance numbers); the Mini V2 still
has to confirm them ("Still to check on the Mini V2" below). They share one switch, `hostsys::VERIFIED_2026_10_23`:
set it (or a single entry) to false and they go back to their default emulator.

### How each one is wired (`src/hostsys.rs`)
- **Second folders.** ROCKNIX lists the same consoles under more names: `famicom`, the "hacks" folders, Super
  Famicom, GBA Video. Before, only `nes`, `snes`, `gb`, `gbc` and `gba` (main.rs `HOST_CORES`) went to pocket-core and
  a game in `roms/famicom` started RetroArch. They are `HostSystem` entries with the same core and nothing else.
- **Famicom Disk System.** Nestopia reads `disksys.rom` from `roms/bios` itself; without it pocket-core shows
  "Famicom Disk System needs a BIOS file in roms/bios: disksys.rom" (RetroArch needs it as well, so no fallback). The
  core names L1 "(FDS) Disk Side Change" and R1 "(FDS) Eject Disk" in its input descriptors, so the Controls page shows
  them; `disc next` (FIFO) turns the disk too (Nestopia's disk control interface lists the sides). Disk writes go where
  Nestopia puts them (its own save file next to the game, as in RetroArch: `savefiles_in_content_dir`).
- **Nestopia's states vary in size** (NES as much as FDS): the size `retro_serialize_size` reports moves by 9 bytes from
  one moment to the next, so a resume state written at quit was refused at the next start as "wrong size (46370 of
  46379 bytes)" in about one start of two (Nova the Squirrel in `roms/nes`, `roms/famicom`, `roms/nesh`; FDS 177462 of
  177471). That was a bug of the NES on pocket-core too, not only of the new folders. Nestopia reads a state from a
  stream of the given length, so `hostsys::core_states_vary` now lists `nestopia` next to `mgba`: shorter states are
  handed over. After the change 13 of 13 starts resumed (NES, Famicom, FDS), and the picture after resuming equals the one at quit
  (difference 0.0).
- **SuperGrafx.** `exts: pce, sgx, zip`: HuCards. CD images (`.cue`, `.ccd`, `.chd`) in the same folder need a System
  Card (`syscard3.pce`) and stay with RetroArch, as `roms/pcenginecd` does. A `.pce` runs in PC Engine mode, an `.sgx`
  in SuperGrafx mode (the state doubles, 78 → 167 KB: the second VDC). 59.82 Hz like the PC Engine.
- **Virtual Boy.** The picture is the core's default: `vb_3dmode` anaglyph with `vb_anaglyph_preset` disabled, which
  shows the left eye only, in red on black (`vb_color_mode`), 384x224 at 3x = 1152x672. The stereoscopic modes stay
  core options (Options > Emulator): "3D mode (requires restart)" side-by-side puts both eyes next to each other at
  768x224 (1x on this screen; a live change did nothing, the next start did), "Anaglyph preset" colours the eyes for
  red/blue glasses, "Palette" replaces the red. The pad: the left D-pad is the D-pad, the right
  D-pad is on L2 (up), R2 (left), L3 (down), R3 (right) as beetle_vb lays it out, and `VB_OPTS` sets
  `vb_right_analog_to_digital=enabled` so the right stick works it too. X is the core's "Low-Battery Toggle". The core
  runs at 50.27 Hz: scheduled by time (Pixel Path: "Every sixth refresh repeats a picture").
- **NAOMI and Atomiswave.** The Dreamcast's core, with MAME's BIOS zips in `bios/dc`, the folder ROCKNIX's standalone
  Flycast uses as its data folder (`start_flycast.sh` links `roms/dreamcast/data` to it). `analog: true` and
  `controls::ANALOG_SYSTEMS` (already there): the sticks go to the core, for the racing games; fighting games want
  Controls > "Left stick as D-pad" on. Flycast names the buttons per game from MAME's input list (the core has the
  names Button 1-8, Coin, Start, Service, Test); with the stand-in cartridge it named only the RetroPad's own, so coin
  and start (expected on Select and Start) are still to be seen on the device. Not verified, so `route` leaves them on
  standalone Flycast; `POCKET_UI_HOST_ALL=1` routes them. Game settings and high scores (NVRAM, EEPROM) are not shared
  with standalone Flycast: the core writes its own into `reicast/` next to the game (`roms/naomi/reicast/<game>.nvmem`).
- **Run-ahead** is offered for all of the verified ones (`run_ahead::SYSTEMS`). Virtual device, run-ahead on, 10 s of
  play: Virtual Boy 1 frame 5.5 ms a refresh (save 0.10, load 0.05 ms, info file says nothing, 58 of 58 pictures
  ahead matched), SuperGrafx 2 frames 3.3 ms (load 1.0 ms), FDS 1 frame 1.8 ms, Famicom 1 frame 1.3 ms; all
  comparisons matched, none switched off. The CPU numbers are llvmpipe's machine, not the Mini V2's.
- **Display names**: `beetle_vb` is "Beetle VB" on the console page and under Runs on (`consoles::core_name`,
  `emu_choice`).
- **Game facts**: `scripts/build-gamedb.py` asks Wikidata for NAOMI (Q1369174, NAOMI 2 Q843916), Atomiswave (Q757617)
  and Pokémon mini (Q1759168) too: 12, 1 and 10 games (Wikidata files most arcade games under "arcade game", not their
  board). FDS, Virtual Boy and SuperGrafx (as `pcengine`) were there already. Curator's feel tags (`curatordb.bin`) have
  none of the three yet: that needs a labelling run (docs/CURATOR-DATA.md).

### Checked on the virtual device (`VDEV_NAME=msy`)
`scripts/syscheck.sh play` (pocket-core with FIFO input) for each, then from the shelf in pocket-ui for FDS and
Virtual Boy (`launch fds ... (emulator pocket-core, core nestopia)`), with the pause menu, Options > Controls and Pixel
Path. Save / play on / Load compared with `scripts/shotdiff.py` (difference after playing on, then after the load):
- FDS (self-made disk, stand-in BIOS): 41.4 / 0.40. L1 ejected and turned the disk ("Switched to Disk 1 Side B", the
  backdrop green: no disk in the drive). Missing BIOS: the message above. Resume after the fix.
- SuperGrafx, Reflectron (Aetherbyte, free homebrew) as `.pce` and as `.sgx`: 43.3 / 0.40 each; Start through the
  credits and the title; resume.
- Virtual Boy (self-made ROM): 25.1 / 0.40; A moved the bars; the right D-pad by the right stick and by L2 swapped the
  shades; side-by-side mode at the next start; resume. Pixel Path: Core Beetle VB v1.31.0, Source 384x224 50.27 Hz square
  pixels, Scale 3x 1152x672.
- sfc, snesh, gbah, gbav (self-made red-screen ROMs), gbh, gbch (uCity), famicom, nesh (Nova the Squirrel): start,
  quit, resume.
- NAOMI: missing BIOS message; then with a **stand-in** `naomi.zip` (our own SH-4 program under the BIOS file names,
  `scripts/testroms.py naomi_bios`; it boots no game) and an empty cartridge: Flycast starts in NAOMI mode, runs the
  program (a mark in main RAM, `fifo peek`), and shows its frame buffer only with `flycast_emulate_framebuffer=enabled`
  (real games draw through the TA, not straight into video RAM). Save 57.8 MB, load puts the machine back (the line the
  program draws went back, two seconds after the load), resume works. The 640x480 frame buffer came out 1218 wide and
  not centred, an effect of Flycast's frame buffer path to check with a real game. Atomiswave: the missing-BIOS
  message only; Flycast decides NAOMI or Atomiswave from MAME's game list, so a home-made cartridge can't be one.
- Flycast in the virtual device: the core (38 MB) is not in its image (cores over 20 MB are dropped); it was copied in
  from the ROCKNIX image, and `/dev/shm` (64 MB in the container) had to be raised to 512 MB
  (`mount -o remount,size=512M /dev/shm`): Flycast's memory takes 57 MB of it and `grim` then died with a bus error.

Test content: Reflectron (Aetherbyte, 2013), the free PC Engine homebrew already used above; Nova the Squirrel and uCity
as in docs/VIRTUAL-DEVICE.md; the rest is written by `scripts/testroms.py` (`fds-test.fds`, `fds-bios-standin.rom`,
`vb-test.vb`, `snes-test.sfc`, `gba-test.gba`, `naomi-bios-standin.zip`, `naomi-test.bin`). The two stand-ins are our
own programs under the BIOS file names, for the virtual device only; they never go onto a card.

### Still to check on the Mini V2
`disksys.rom` and a real FDS game (side change with L1 when the game asks for side B, an in-game save, then quit and
resume); a SuperGrafx game (a real SGX title or Reflectron as `.sgx`) and a Virtual Boy game against RetroArch
(`scripts/syscheck.sh bench` / `ra`, CPU and frame pacing, run-ahead 1 within budget); the right stick and L2/R2/L3/R3
on the real pad in a Virtual Boy game; a game from `roms/famicom` and from `roms/sfc` started from the shelf; NES
resume after the `nestopia` change (a game in `roms/nes`, quit, start again, `resumed` in the log, several times).
NAOMI and Atomiswave only with the BIOS sets in `roms/bios/dc` and a MAME romset of a game, with pocket-core started
directly (`scripts/syscheck.sh play naomi flycast <game.zip> ...`; the shelf routes only verified systems, and
`POCKET_UI_HOST_ALL` is pocket-ui's environment, which `dev.env` doesn't set): coin (Select) and start, the buttons, a
vertical game (rotation), Save / Load / resume, frame rate against standalone Flycast; then set `verified: true` for
them. Steps in docs/DEVICE-TESTS.md.

## Audio latency
PipeWire here runs a fixed 1024-frame quantum (21 ms). `pa_simple_get_latency` reports 85–105 ms inside the
server once playing, on top of our own 40 ms queue, so the real output latency is above the 64 ms
RetroArch is set to. Not tuned yet; `pa_simple` can't pass `ADJUST_LATENCY`, so the next step would be the
async PulseAudio API or PipeWire directly.
At the start the server swallows data at wire speed until its buffer is full; the audio thread now fills it
with silence first, otherwise our queue ran empty and glitched in the first 2 s (5 underruns on mGBA).

## Open problems
- The systems checked only with self-made ROMs (Master System, Game Gear, SG-1000, WonderSwan, Atari 2600) need
  one real game each: a ROM whose mapper, save RAM or 50 Hz mode the test ROM does not touch. Mega Drive 6-button,
  Sega CD, 32X, PC Engine CD, NGP, Lynx, 7800, Pokemon mini and FBNeo have not run at all; see the table for how to check one.
- No analog game was tried: Crash Bandicoot 2 is digital. The DualShock is requested and accepted by pcsx_rearmed
  (log: `player 1: dualshock (device 517)`) and the sticks are handed to its analog query, but a real analog game
  (Ape Escape, for example) still has to be played.
- Self-made ROMs hide differences between consoles' real games. One PlayStation run on the virtual device and
  one on the device had a burst of audio underruns around the 8.5 s mark (the first seconds after the BIOS, with
  the dynarec compiling); the 36 s benchmark runs had none.
- Core options are the cores' own defaults everywhere (as RetroArch has them here); no per-system overrides yet.
- The shelf lists a `.bin` next to its `.cue` as a second game, as before; both start (pcsx_rearmed also loads a bare `.bin`).
- The pause menu has no disc swap, rewind or fast-forward entry for these cores.

## Tools
- `scripts/syscheck.sh`: push pocket-core, copy ROMs, `play` (test input, screenshots, save/load), `bench` and `ra`
  (CPU against RetroArch). Works on the device (inside `devlock.sh`) and on the virtual device (`VDEV_NAME=...`).
  `deploy/syscheck-play.sh` is its device side; `scripts/shotdiff.py`, `scripts/contact.py` compare and tile screenshots.
- `scripts/testroms.py`: writes the test ROMs (Z80, 6502, x86, V810, 65816, ARM and SH-4 programs assembled in the script; the FDS and NAOMI BIOS stand-ins for the virtual device).
- `POCKET_CORE_TRACE=1` also prints every core option a core declares (`[opt]`), the file calls through the VFS
  (`[vfs]`) and the core's extensions and `need_fullpath`.

## Next
- Real games for the systems above; per-system core options.
- Lower audio latency (see above).
- Per-system filters and aspect choices.

## Audio latency (21.10.2026)

pocket-core now plays through ALSA's `pipewire` device, a native PipeWire stream, instead of
PipeWire's pulse layer, which keeps every pulse client at ≥ 1024 frames and about 90 ms. While a
game runs it lowers PipeWire's quantum from ROCKNIX's forced 1024 to 256 and puts ROCKNIX's value
back afterwards (`/usr/sbin/quantum`; pocket-ui repeats that after every hosted game in case
pocket-core crashed). Fallback: `POCKET_CORE_AUDIO=pulse`; `POCKET_CORE_QUANTUM=0` keeps 1024.

Mini V2, `--bench`, reported by the stream (`pa_simple_get_latency` / `snd_pcm_delay`) plus our queue:

| Path | Quantum | Server | Queue | Total | Underruns |
|---|---|---|---|---|---|
| pulse (before) | 1024 | 85–91 ms | 28–44 ms | 113–135 ms | 0–6 / 60 s |
| native, 20 ms buffer | 1024 | 73 ms | 25 ms | 98 ms | 0 / 20 s |
| native, 10 ms buffer | 256 | 26 ms | 17–21 ms | 43–47 ms | 0 / 60 s (GBA, SNES ×3) |

At 256 the sound card driver reported no errors (pw-top ERR 0) and PipeWire used 1 % of a core
(2 % before). End-to-end latency to the speaker isn't measured (no loopback); a listening test on
the device is still open.

## Bluetooth audio (2026-10-25)

Status: played on real headphones on 2026-10-27 and broken there (below, "The ALSA buffer"); fixed and measured off the
device with `scripts/audio-bench` (2026-10-28); the device check with headphones is still to do.

ROCKNIX ships PipeWire and WirePlumber with bluez5 (SBC, AAC, aptX, LDAC, Opus). The Bluetooth page pairs headphones
like a controller; WirePlumber then makes their sink (`bluez_output.*`) the default, and pocket-core's stream (ALSA's
`pipewire` device) and the shelf's sounds follow the default sink, so nothing in pocket-ui routes sound itself.

What changes is the quantum (`src/audio_route.rs`):

- **The speaker keeps 256, Bluetooth keeps ROCKNIX's.** PipeWire's bluez5 sink (`spa/plugins/bluez5/media-sink.c`) is
  a graph driver of its own (`node.driver = true`), encodes into its own buffer and sends A2DP packets on its own
  timer; "the quantum and packet size aren't correlated", and its reported delay is "(packet delay) + (codec internal
  delay) + (transport delay) + (latency offset) and doesn't depend on the quantum". A 256 quantum takes nothing off
  that and makes the graph wake four times as often, with crackles when a cycle is late. PipeWire's own default
  quantum is 1024 (`default.clock.quantum`, pipewire.conf(5)), which is also what ROCKNIX forces.
- **How the sink is found.** `pw-metadata -n default 0 default.audio.sink` (about 11 ms on the device under load);
  `default_sink_name` parses its `update: id:0 key:'default.audio.sink' value:'{"name":"…"}'` line, and a name
  starting `bluez_output.` is Bluetooth. Only asked when the kernel has a Bluetooth link (`/sys/class/bluetooth/hci0:<handle>`,
  one per connection, `hci_conn_add_sysfs` in net/bluetooth/hci_sysfs.c); without one there is no Bluetooth sink and no
  process is started. No `pw-metadata`, a failure, or output without that line: the speaker, as before.
- **Not on the launch path.** `Audio::open` used to run `pw-metadata … clock.force-quantum 256` itself (on the thread
  that opens the sound beside the window and GL, joined in `resumed`). Now it only starts `audio_route::Keeper`'s
  thread, which looks at the sink and sets the quantum; the stream was already open at 1024 before, as now.
- **During a game.** The keeper looks at the links every 2 s (a directory read). For 16 s after a link came or went,
  and for 16 s from the start when there is one, it asks for the default sink too, since WirePlumber switches a few
  seconds after the link: headphones appearing mid-game put ROCKNIX's quantum back, leaving sets 256 again. After a wake
  (`reassert_quantum`) it asks again. With a controller connected that is a few `pw-metadata` runs after each
  connect; with nothing connected, none.
- **Restore and keep.** The keeper puts ROCKNIX's quantum back when the game's `Audio` is dropped, only if it set
  256, and the drop waits for it; `keep_quantum` (fast switching) leaves it for the next game, as before. pocket-ui
  still restores after every hosted game in case pocket-core crashed.
- **The ALSA buffer (the Bluetooth bug of 2026-10-27).** The first games played on headphones (GBA, then SNES) sounded
  "extremely distorted and broken", the menu sounds "a bit distorted and quiet"; the speaker was fine, PS2 (a
  standalone emulator) too. Cause: pocket-core and the shelf asked ALSA's `pipewire` device for 10 ms (480 frames),
  and PipeWire's ALSA plugin hands the graph each cycle what that buffer holds. At the speaker's 256 that is enough; at
  ROCKNIX's 1024, which stays on Bluetooth, part of every 21 ms cycle went out empty. pocket-core's underrun counter
  never saw it (its own queue was full; the gaps were in PipeWire). Reproduced with `scripts/audio-bench`: 340
  dropouts in 7 s at 1024, none at 256. Now the writer opens the stream with a buffer for the cycle that runs
  (`audio::Ring`): 10 ms only while the game's 256 really runs (`audio_route::Keeper`'s `small` flag: set after the
  force succeeded, cleared before ROCKNIX's quantum comes back), 32 ms otherwise (a 1024 cycle and two writer chunks;
  22-50 ms all had no dropout). It waits for the keeper's first look (at most 0.5 s, beside the core's first frames)
  and opens the stream again when that changes during play, so headphones connected or switched off mid-game get the
  right buffer within one 2 s check. The shelf (`ui_sound::ui_ring`) does the same from what its quantum thread saw
  last (asked at its start and with every opening outside a game).
- **Queue.** 20 ms on the speaker at 256 (48 ms for 3D cores), as before. At 1024 the writer takes a whole cycle out
  of it at once, so it is 48 ms there (`CYCLE_TARGET_MS`; at 2048 a 20 ms queue underran in the bench). Together with
  the 32 ms buffer that adds about 60 ms on headphones, against the 100-250 ms the codec and the radio add.
- **Rate control.** It steered on each raw reading of the queue, which saws by a core frame (17 ms) and a writer
  chunk or a whole graph cycle: a pitch wobble of ±2.4 cents on the speaker and ±8 on Bluetooth (sidebands around a
  1 kHz tone at -30 to -40 dB). Now it steers on the fill averaged over about a second (`FILL_SMOOTH`), and not at all
  until the writer plays and for a second after (`WARM_S`): before, the empty queue at the start pulled the pitch 8
  cents down and the overfilled queue then 9 cents up for seconds. The averaged fill after that second is the
  reference it holds, moved to the target by at most 0.02 % of the pitch (`REFERENCE_SLEW`, 0.35 cents), so a queue
  that starts a few ms off (a stream just linked) takes a minute to get there instead of a 2-4 cent glide. The writer
  starts at the target instead of 1.5 times it, the Bluetooth stream is opened before the queue fills, and after the
  pause menu the control starts afresh the same way (the queue comes back up to a core frame off where it was). Unit test: `audio::tests::the_rate_control_holds_the_pitch_steady`
  (a simulated graph at 256, 1024 and 2048, clocks ±0.3 %, a start 30 % low).
- **The shelf** (`ui_sound`) skips its 256 on Bluetooth the same way, asked on its quantum thread.
- **Pixel Path** shows "· Bluetooth" instead of the output's milliseconds and says the codec and the radio add a delay
  pocket-core can't measure, often 100-250 ms (a typical range for A2DP codecs, not measured here).

**Measured** (`scripts/audio-bench`, ROCKNIX's PipeWire 1.2.6 in a container, 1 kHz from a 65536 Hz core at 60 Hz):

| Case | Dropouts and clicks | THD+N, worst 50 ms | Pitch wobble, rms / max |
|---|---|---|---|
| Before: speaker (256), 8 s | 0 | -34.5 dB | 0.72 / 2.2 cents |
| Before: Bluetooth (1024), 8 s | 373 | broken (+32 dB) | - |
| Before: Bluetooth, only the buffer larger | 0 | -22.5 dB | 2.5 / 7.0 cents |
| After: speaker, 10 min | 0 (see below) | -66.1 dB | 0.09 / 0.37 cents |
| After: Bluetooth, 10 min | 0 | -67.9 dB | 0.05 / 0.35 cents |
| After: Bluetooth, display 0.1 % fast, 10 min | 0 | -66.4 dB | 0.21 / 1.8 cents (tracks +1.73 cents, as it should) |
| After: pause menu twice and quit through it, 20 s, speaker / Bluetooth | 0 | -68.2 / -67.9 dB | 0.19 / 0.42, 0.19 / 0.37 cents |

The "before" THD+N on the speaker is the rate control's wobble (sidebands of the 1 kHz tone), not harmonic distortion;
what is left after is the linear resampler's images and 16-bit noise (about -68 dB). In the container PipeWire has no
real-time priority; with four benches at once two of six speaker runs had a few graph xruns (256 zero frames each,
pw-top's ERR the same on the recorder as on pocket-core's stream, pocket-core's own underruns 0). The speaker alone
for 10 minutes: ERR 0, none (the row above).

To check on the device with real headphones: the sound moves to them in the menu and a game, and back to the speaker
when they are switched off mid-game; `pw-metadata -n settings 0 clock.force-quantum` reads 1024 (ROCKNIX's) while a game
plays on them and 256 on the speaker, and changes within about 5 s of connecting or disconnecting; `ls
/sys/class/bluetooth` shows an `hci0:<n>` entry while they are connected; no crackles and no underruns in Pixel Path
over a minute of play; after sleep and wake the sound comes back on them. `pw-top` shows pocket-core's stream
(`alsa_playback.pocket-core`) at QUANT 384 on headphones (the 32 ms buffer's period) and 120 on the speaker, and the
log says `audio: Cycle buffer (32 ms)` on headphones.

## Cheats (`src/cheats.rs`)
- **Source**: libretro-database's `.cht` files, which ROCKNIX ships in `/usr/share/libretro-database/cht` and
  mounts (with the user's additions in `/storage/database`) at `/tmp/database/cht` (`cheat_database_path` of
  RetroArch). Folder per console (`cheats::folder`: "Nintendo - Game Boy Advance", ...). `POCKET_CHEATS_DB`
  points somewhere else for tests.
- **Matching** (`cheats::find`): by file name, no checksums. Brackets, case and punctuation are ignored ("&" =
  "and", "Name, The" = "The Name"). When the ROM names a region ("(USA)", "(U)", "(Europe)") only files for that
  region or for none are used; without one all files of the name are put together, plain name first, then Game
  Genie / Action Replay / Code Breaker variants, identical codes once, a clashing name gets "[Game Genie]".
  `&eacute;` and friends are decoded. A game the database doesn't have has no Cheats page.
- **Entries without a code** (`cheats::parse_cht_full`): the newer cht format has `cheatN_address`, `_value`,
  `_cheat_type` (0 none, 1 set to value, 2 increase ... as RetroArch's cheat manager), `_handler = 1` and an empty
  `_code`. Those are counted (`Parsed::unsupported`, equal ones of several files once) and the page ends with
  "N cheats use a format pocket-core can't apply yet" (a game with only such entries still gets the page, with that
  line alone). Entries with a code are as before, also when they carry an address as well. Not counted, because they are
  no cheats: titles and placeholders with neither code nor address, and the "(Rumbles)" files, whose entries have
  `cheat_type` 0 and `rumble_*` fields (RetroArch's rumble triggers; they match a game's name like any variant file).
  In ROCKNIX's copy (2026-10) the entries that would be counted are about 21 in the NES folder, 200 in PC Engine, 39 in
  Lynx, 41 in 2600 and 78 in 7800, none in Game Boy, GBA, Genesis or SNES. **Why not applied:** `retro_cheat_set` takes
  codes only. A memory write needs `retro_get_memory_data`/`retro_get_memory_map`, RetroArch's address numbering over
  the core's memory regions (not the same as the console's addresses for every core), a write every frame (also during
  run-ahead, rewind and state loads), widths in bits, big endian and the repeat fields. That is a subsystem, not a
  small change; it needs a device check per core, so it is left out until someone wants it.
- **Pause menu**: Options > Cheats (after Controls), list with On/Off, A switches, L1/R1 skip 5, "All off".
  The page stores the switched-on cheats at once in `<data>/cheats/<system>/<game>.json` (`{"on": ["Infinite
  Lives", ...]}`, keyed by name, a second equal name is "name #2"; no file when nothing is on) and sends `cheats` over
  the command FIFO. Cheats are deliberately not part of Save changes: they belong to the game.
- **pocket-core**: at start and on `cheats` it reads that file and calls `retro_cheat_reset`, then
  `retro_cheat_set(i, true, code)` for each switched-on cheat (`Core::set_cheats`); log line `cheats: N on`, and a
  toast "2 cheats on" / "Cheats off". The code string goes to the core unchanged (`+` joins several), exactly what
  RetroArch hands over, so what works there works here. Cores take cheats while paused; they act after Continue.
  Switching a cheat off stops the core writing; values it already wrote stay until the game changes them.
- **Checked on the virtual device** (ROCKNIX's real database copied into the container): SNES Contra III, 74 + variant cheats
  listed, page screenshot looked at; with `POCKET_CORE_TEST=1` `peek 1f88` (work RAM, new test command) reads
  0 before and 0xFF after "Always Invincible" (7E1F88FF) and 0 -> 1 for "Infinite Bombs" (7E1F8C01) switched on while the
  game ran. GBC, GBA and NES games (ROMs renamed after database entries) start with cheats on and run at normal speed.
  Not checked: that a real game visibly changes on the device, GBA Code Breaker and NES `0754:00` style codes in actual
  play (same strings and cores as RetroArch).

## Rumble and menu haptics (`src/rumble.rs`)
- **Motor**: the Mini V2's device tree (`retroidpocket,rpminiv2`, read from the KERNEL of the image) has a Qualcomm
  `qcom,pmi8998-haptics` node, `status = "okay"`, ERM actuator (`qcom,actuator-type = <1>`). ROCKNIX's udev rule names
  its input device `pmi8998_haptics`. InputPlumber's profile for the Retroid gamepad (`retroid_mcu`, targets `ds5` +
  keyboard) has no force feedback of its own and maps no rumble to the motor, so the motor is driven directly through its
  evdev node, not through the virtual pad.
- **Device choice**: every `/dev/input/event*` with `EV_FF` is probed; a name containing "haptic" or "vibra" wins, then
  pads (InputPlumber's DualSense, the virtual device's test pad), then others. `POCKET_RUMBLE_DEVICE=/dev/input/eventN`
  forces one. The effect type follows the device: `FF_RUMBLE`, else sine `FF_PERIODIC`, else `FF_CONSTANT` (memless
  drivers offer all three). Effects are uploaded with `EVIOCSFF` (one effect, updated in place) and played with an
  `EV_FF` write; raw ioctls, no library.
- **libretro**: `GET_RUMBLE_INTERFACE` is answered in `libretro.rs` by `rumble::environment`. The core's callback only
  posts a message to a worker thread (nothing blocks emulation); the stronger of the strong and weak request counts, scaled
  by the setting. Each player (ports 0-3) rumbles their own controller, see
  [Players](#players-couch-co-op-srcplayersrs). The worker plays 1.5 s leases and renews them every second while rumble is on, so a killed pocket-core
  leaves at most 1.5 s of vibration (closing the device erases the effect anyway). The pause menu opening (`pause`) and the
  end of the game stop it. A game that never rumbles never starts the thread or opens the device.
- **Strength**: Options > Extras > Rumble Off / Low / Medium / High (default Medium; `rumble` in `options.rs`, scale 0 /
  0.4 / 0.7 / 1.0), per console or game like the other Extras.
- **Menu haptics**: Settings > Look & feel > Haptics On / Off (default Off, stored in `<data>/haptics.json`). pocket-ui compares where
  the menus stand (shelf, Settings, pause menu pages, values) before and after Up / Down / Left / Right / L1 / R1 and
  sends one tick when it changed, at most one per 55 ms. A tick is 22 ms at 36000 of 65535; tune on the device with
  `POCKET_HAPTIC_TICK=<ms>,<strength>` in pocket-ui's environment.
- **Testing**: `pocket-core --rumble-test [ms]` goes through the same path a core's rumble takes (the libretro callback,
  Low / Medium / High, the weak motor, then five menu ticks) and prints the device it found. On the virtual device the pad
  accepts force feedback and logs every upload / play / stop to `/tmp/vpad-ff.log` (`vdev.sh sh cat /tmp/vpad-ff.log`).
  Checked there: device found, rumble effect with 26214 / 45875 / 65535 for Low / Medium / High, ticks of 22 ms, lease
  renewed every second while held, stop and erase at the end, menu ticks on Up / Down in Settings and the pause menu, none
  with haptics off. **Not checked: that the real motor moves.** That needs the Mini V2:
  `pocket-core --rumble-test`, then a game that rumbles.

## Players (couch co-op, `src/players.rs`)

**Several devices, one pad (review SOL-09).** Every input device is read on its own thread (`pad::linux::supervise`),
and the Pocket alone is already several: InputPlumber's virtual DualSense, its keyboard (BACK arrives as F1), the
gpio and power keys. Before, every device wrote into one shared state, so a second pad letting go of a button, or
being switched off, released a button the first one still held, and a centred stick overwrote a pushed one.
`pad::Mux` keeps each device's buttons and axes apart: a button is down while any device holds it, an axis is the one
pushed furthest, a device that goes away (`PadEvent::Disconnected`, sent by its pump after its own releases) lets go
of only what it held. Only changes of the merged state go on, so pocket-ui never sees a second press of a held button
or a release that isn't the last one, and a held direction's repeat keeps going (`main.rs` `user_event` merges before
anything else; `PadState::merge`). pocket-core's HOME shortcuts listen to the merged state too.

**Which controller a device belongs to** (`pad::controller_key`, `pad::linux::identify`, sent as
`PadEvent::Connected` before a device's first event): Bluetooth (bus 0x05, also Bluetooth LE pads that bluetoothd
creates through uhid) is a controller of its own, by its address (`uniq`); USB that is not a virtual device is one by
vendor, product and port; everything else is the Pocket (`builtin`): the platform's keys and InputPlumber's virtual
devices, which ROCKNIX's profile presents on the USB bus through uhid (`/sys/devices/virtual/...`). The keys are the
same in both programs and across reconnects. Only gamepads (a south face button) and the Pocket take a seat; a
headset's volume keys don't. If InputPlumber were set to take over Bluetooth pads too, they would become virtual
devices and count as the Pocket; ROCKNIX's profile doesn't (to check on the device, see below).

**Seats** (`players::Seats`, kept by pocket-core in `players::Inputs`):
- At the start (`Inputs::arrange`, after the first look at the devices) everyone is seated by the order in
  `<data>/players.json`, gaps closed, then the controllers it doesn't name: the Pocket first, then in the order they
  connected. No file: the Pocket is player 1.
- During the game a controller that connects gets the seat it had in this game if free, else the lowest free one; one
  that goes away leaves its seat empty (that player's port reads nothing). Both show a toast
  ("DualSense connected — Player 2", `players::connected_note`); the controllers there at the start don't.
- pocket-core writes the seats to `/tmp/pocket-core.players.json` on every change; the pause menu's Players page
  (`app_players.rs`) shows that file, re-reading it while open when its time changes. A on a controller there writes
  the new order (exact seats, `""` for an empty one, then the absent controllers it remembered) on a thread and sends
  `players` on the FIFO; pocket-core seats everyone exactly as written (`Mode::Exact`).
- The page and its menu item exist for pocket-core games with two or more seatable controllers connected (pocket-ui's
  own `PadState` knows them).

**libretro.** Port 0 is player 1 as before (`Core::run`); players 2-4 are set per frame with `Core::set_player` and
read by `input_state` for ports 1-3: RETRO_DEVICE_JOYPAD (single ids and the bitmask) and ANALOG; the touch screen
stays port 0's. `SET_CONTROLLER_INFO` is now read for every port the core lists (up to the `{NULL, 0}` end, at most
16); after loading, `Core::plug_other_players` calls `retro_set_controller_port_device` for ports 1-3 that the core
lists, as RetroArch does: the analog pad (DualShock or "analog" by name) for systems with sticks (`HostSystem::analog`),
else the plain joypad. Ports the core doesn't list keep what the core plugged in itself. The Controls page's layout
and the left stick as D-pad apply to every player; HOME held blocks every player's buttons; SELECT + START quits only
when one controller holds both (`Inputs::one_player_holds`). Run-ahead's input fingerprint includes the other
players, so their presses don't count as mispredictions.

**Rumble per player.** `set_rumble_state(port, …)` is taken for ports 0-3 (more are ignored); each port has its own
strong/weak level and its own target (`rumble::set_targets`, from the seats): the Pocket's motor for the Pocket's
seat, the controller's own event node for a Bluetooth or USB pad (hid-playstation and xpad offer `FF_RUMBLE` on the
pad's node), nothing for an empty seat. The motor is never chosen among the nodes other players rumble. When the seats
change while something rumbles, it stops on the old device and goes on on the new one. Before the first seating, and
in pocket-ui, port 0 is the motor as it always was.

**RetroArch and the standalone emulators** are not touched: RetroArch numbers controllers itself (ROCKNIX's udev
joypad driver; the virtual pad is normally player 1, the rest follow in the order RetroArch finds them; its own menu,
Settings → Input, changes it). Dolphin, ARMSX2, Azahar and the other standalones use their own controller settings
from ROCKNIX. pocket-ui only reads their input for its own menus and the HOME tap, which the merging leaves as it was
for one controller.

**Checked here** (unit tests and the virtual device; no real Bluetooth pad): the SOL-09 sequences on `PadState` and
`Mux`, per-player buttons, sticks, disconnects, hotplug, toasts and reordering on `Inputs`, the libretro ports and
controller plugging against a C stub core, the seat rules, the page's presses. **To check on the Mini V2** with a real
pad: the key a DualSense or Xbox pad gets (`pad: source N is bt:… (…)` in the log, and that InputPlumber's DualSense
is `builtin`), two-player input in a two-player game, the toast on connect and disconnect, rumble on the pad and on
the Pocket's motor at the same time, and the Players page (A on each controller).

## Screenshots (HOME + A, `src/screenshot.rs`)
- **Key**: HOME + A, the right face button. Taken: HOME + X (ARMSX2's pause menu, Dolphin's aspect ratio), + B (Dolphin's
  resolution), + Y (Dolphin's FPS, pocket-core's Pixel Path), + R1/L1/R2/L2/Left/Right (pocket-core's save, load, fast-forward,
  rewind, slot), + Vol+/- (brightness), + SELECT (the game switcher) and + START (ROCKNIX's quit)
  ([BIG-EMULATORS.md](BIG-EMULATORS.md), [user/controls.md](user/controls.md)). A is the one nothing in the repo uses; what
  RetroArch's and the ports' own HOME + A does has not been checked (RetroArch games are left alone, below). The thumb is
  on A anyway while HOME is held. pocket-ui's pause menu opens on a HOME *tap* only (`hotkeys::home_tap_step`: any other
  button going down while HOME is held ends the tap), so HOME + A opens nothing. Options → Shortcuts → Screenshot
  (`hk_shot`) switches it off on pocket-core.
- **pocket-core**: the hotkey (`Action::Screenshot`) clones the core's last frame (`App::last_frame`, a copy of at most
  a few hundred KB; a 3D core's frame is read back from the GPU first, the same readback a state's preview uses) and
  hands it to the `screenshot` thread (`screenshot::Saver`, a channel of one place). That thread converts it
  (`Frame::to_rgb`), encodes the PNG and writes it through `fsutil::write_atomic` (temporary file, `fsync`, rename), then
  reports back; the emulation loop picks the answer up each frame (`App::screenshot_results`) and shows "Screenshot
  saved" (or why not) as a toast, and mixes the Shutter sound into the game's sound (`chime::Player::start_samples`, when
  Settings → Sounds is on, at the UI sounds' volume). A second press while one is being written is dropped. The picture
  is the core's native frame (no scaling, screen frame, effects or HUD); the "as shown" copy was left out because
  the rendered output would need an offscreen render on the emulation thread (`write_shot` does that for the pause
  menu, and takes about as long as a frame).
  A vertical arcade game's frame is saved as the core delivers it, not turned upright, and so is a picture turned by
  Screen orientation.
- **Standalone emulators and ports** (`src/app_screenshot.rs`): pocket-ui sees the same pad events. HOME + A with no
  menu open, in a game whose emulator is neither pocket-core nor RetroArch nor an app, sends a request to the same
  thread, which runs `grim -t png -` (5 s limit) and writes the PNG. That is the whole screen, so the game's picture
  with whatever the emulator draws. Nothing can be shown over these games (pocket-ui is not in front), so the answer is
  the Shutter sound through `ui_sound::play_over_game` (the mixer thread; only when Sounds is on) or the Error sound,
  and a line in the log. Not switchable. RetroArch is left alone: its own hotkeys (and screenshot key) stay its own.
- **Files**: `/storage/roms/screenshots/<system>/<game> <YYYY-MM-DD HH-MM-SS>.png` (`POCKETUI_SCREENSHOTS` moves
  the folder). Under `roms` the file share shows it (`wifi::SHARE_VETOED` hides only ports, PortMaster, bios and
  savestates), and the companion lists it. (Where RetroArch's own screenshots go on ROCKNIX has not been checked on the
  device.) The name is time-sorted, has no colon (vfat/exFAT), and the game part goes through
  `screenshot::clean` (slashes and the characters the card refuses become `_`, no leading dot, at most 120 bytes).
  The same second twice gets ` (2)`.
- **Failures**: ENOSPC/EDQUOT → "the card is full"; EROFS, EACCES, EPERM, ENOTDIR, EEXIST → "the folder can't be
  written to"; no frame yet → "no picture yet"; `grim` failed → "the screen couldn't be read". The write is atomic, so
  none of them leaves a file or a `.tmp`.
- **Web companion**: `GET /api/shot` (read only, see [WEB-COMPANION.md](WEB-COMPANION.md)); the game's page lists them.
- **Tests**: `screenshot::tests` (names with Unicode and slashes, atomic write and the failing one, the thread, the
  list and `find`), `hotkeys::tests` (HOME + A fires once, switched off, the pause menu does not open), `web::tests`.

## UI sounds (`src/ui_sound.rs`)
- **Sounds**: navigation tick, confirm, back, menu open (pause menu and quick settings), game switcher, error /
  refused, and a tick on Vol+/Vol-. 90 to 450 ms each, 48 kHz mono, played on both channels.
- **Where they come from**: sample files in `assets/ui-sounds/` (mono 16-bit 48 kHz WAV, 140 KB in all), generated
  with ElevenLabs sound effects (paid Creator plan, commercial use; `docs/LICENCES.md`), prompts for "warm, tactile retro
  hardware": a soft wooden tick (Nav), a cartridge click (Confirm), a low wooden knock (Back), a drawer sliding open
  with a click (Menu), cartridges flipping past (Switcher), a muted button that does not engage (Error), a knob detent
  (Volume), an old shutter (Shutter). Each file is trimmed, faded in over 2 ms and out over up to 30 ms, and normalised to a
  peak of -1.5 dBFS (Nav -3), 90 to 450 ms long. `ui_sound::synth` decodes them; `synth_at` resamples for pocket-core's
  shutter. The loudness of the set is `Config::gain` alone. The earlier synthesised sine tones peaked at -6 to -10 dBFS
  in the file and then took the default gain of -17.5 dB, so they reached the output at -23 to -28 dBFS, about 20 dB
  under a game; at that level the 16-bit rounding of the gain stage (no dither) is audible on a 28 ms tick. The default
  is now step 7 (-7.5 dB), which puts the samples at about -9 dBFS.
  The mixer thread decodes all of them once when it starts (`ui_sound::start` at launch), so a sound is a buffer lookup.
- **Which sound**: `main.rs` takes a `View` before and after every pad event (what covers the screen: pause menu,
  switcher, quick settings; and a hash of everything that moves: shelf, Settings, pause menu pages and values,
  setup, store, quick settings, switcher) and asks `ui_sound::sound_for`. An overlay that opens gives Menu or
  Switcher, one that closes Back (Confirm with A); otherwise a change after A is Confirm, after B Back, after a
  direction, L1/R1 or a held repeat Nav; no change, no sound. `ui_sound::refuse()` marks a refusal (Load on an
  empty slot, Save changes that failed) and wins. Vol+/Vol- report `quick::Out::volume_step` when they change the
  volume (not with START or SELECT held). Taps arrive as button events and sound the same.
- **When not**: `ui_sound::allowed` on the state *after* the event: off, volume 0, asleep (`Sleep::state`), or a
  game running without the pause menu up. So starting a game and leaving the pause menu are silent, and nothing
  ever plays over gameplay. Muted or at 0 (`ui_sound::audible`) is asked with `pactl` (two processes) on a thread
  of its own, started as the output opens (trusted for 1.5 s; a muted sink closes the output again, the sound
  having played into a muted sink). Not from sleep's 15 s reading: on the device that kept the sounds off for up to
  15 s after unmuting. While the output is open, a mute in between is PipeWire's to honour.
- **Playing**: `ui_sound::play` checks the gate, keeps Nav and Volume ticks 40 ms apart (a held direction repeats
  every 70 ms) and posts to the mixer thread over a bounded channel (`try_send`): the UI thread never waits. The
  mixer owns an `audio::Sink`, the same native PipeWire stream through ALSA's `pipewire` device that pocket-core uses
  (10 ms ALSA buffer; `POCKET_CORE_AUDIO=pulse` applies here too). It opens it for the first sound, mixes up to four
  voices in 128-frame chunks (2.7 ms), writes silence between sounds and closes the stream 2 s after the last one, so
  an idle shelf keeps no stream or PipeWire graph running. `ui_sound::stop` closes it at once when a game starts.
  `POCKETUI_SOUND_DEBUG=1` logs, per sound, how long the mixer thread took to pick it up, the open time and what is
  already queued ahead of it.
- **Latency** (2026-10-22, virtual device, PipeWire at ROCKNIX's quantum of 1024; the device measured 27-41 ms to open
  and about 55 ms queued, the same shape): a sound waits for (a) the mixer to pick it up, which only happens when its
  blocked write returns (a quantum, 21 ms, at 1024: 2-21 ms, mean 11), (b) the silence already written ahead of it
  (22-26 ms, one quantum plus a little; `POCKET_CORE_ALSA_US` 5000 or 2000 changes nothing, the plugin sizes its buffer
  by the quantum), and for the first sound after 2 s of quiet (c) the mute check, two `pactl` processes (9-25 ms) before
  the output opened, (d) opening it (9-20 ms). So a tick in a list took about 35 ms from the thread to the stream,
  and the first one about 55 ms before it even began.
  Now: (c) runs beside (d); the mixer thread has nice -10 (`favour_this_thread`), chunks are 128 frames, and while the
  output is open on the shelf PipeWire's quantum is 256 (`audio::force_quantum`, the value pocket-core uses for a
  game; `POCKETUI_SOUND_QUANTUM=0` keeps ROCKNIX's) and restored when it closes (`audio::restore_quantum`, queued on
  its own thread so the order is kept). In a game pocket-core's quantum stays. Measured the same way: pickup 0.4-5.4 ms
  (mean 2.5), queued 9-10.6 ms, so about 13 ms instead of 35; first sound: output open after 9 ms with no mute check
  before it. Without the priority, quantum 256 gave 4-5 xrun lines in 25 presses where 1024 gave 0-1 (the virtual
  device draws with llvmpipe on the same cores as the mixer); with it 0-2. Expect about 14 ms queued on the device, not
  55. Not heard on the device: check for crackle (below).
- **Volume**: Settings > Look & feel > Sounds, A switches, Left/Right 1 to 10 (default on, 7), stored in `<data>/sounds.json`
  through `fsutil::write_json`. Gain is 2.5 dB per step below 10: step 3 is -17.5 dB, so the loudest default sound
  (Confirm) peaks near -24 dBFS before the system volume.
- **Tests**: `ui_sound::tests` (which event gives which sound, every suppression, settings, the synthesis: length,
  silent edges, peak; the mixer), `quick::tests::only_a_volume_change_asks_for_the_volume_tick`.
- **Checked on the virtual device**: output opens in 7–10 ms, about 10.5 ms queued per sound; a capture of the sink
  monitor shows the ticks at the computed level; off, muted and in a game silent; pause menu, switcher, Error, Vol±.
- **Checked on the Mini V2 (2026-10-22)**: all seven sounds in order (shelf, Settings, switcher, quick settings),
  output opens in 27–41 ms, about 55 ms queued per sound (ROCKNIX's quantum of 1024 outside games). The owner, at 20 %
  system volume and the default 30 %: loudness right, sound right, slightly late but not disturbing. Not heard on
  the device yet: Error and Vol± (both checked on the virtual device).

## RetroAchievements (`src/achievements/`)

pocket-core counts [RetroAchievements](https://retroachievements.org) with the official client library, rcheevos
12.5.0 (MIT), vendored in `vendor/rcheevos/` (`UPSTREAM` lists the files) and compiled by `build.rs` with the `cc`
crate, for the host and under `cargo zigbuild` for aarch64. `src/rc_shim.c` flattens rc_client and the rapi calls into
plain functions and structs for Rust (`src/achievements/ffi.rs`), so a newer rcheevos only touches the shim.
User guide: [docs/user/achievements.md](user/achievements.md).

| Part | What it does |
|---|---|
| `account.rs` | `<data>/retroachievements.json`: user name and token, written 0600 from the start (temp file created with that mode, fsync, rename). Never the password |
| `api.rs` | One-shot rapi requests: the password login (Settings), hash → game id and the console's progress (game card), late unlocks |
| `http.rs` | curl with `--connect-timeout 8 --max-time 30`; the POST body goes through stdin (no token or password in the process list); User-Agent `pocket-ui/<version> (Linux) <core>/<core version> rcheevos/12.5.0` |
| `session.rs` | rc_client for the running game (see below) |
| `offline.rs` | The last online answers per game hash (`achievements/offline/<hash>.json`: the set and the unlocks) stand in when the server can't be reached; a token login without an answer is answered from the account. Unlocks are written to `achievements/pending.json` before they are sent and removed when the server answers 200 |
| `snapshot.rs` | `/tmp/pocket-core.achievements.json` for the pause menu; `achievements/games/<system>/<stem>.json`, the tally for the card and the console page |
| `lookup.rs` | The card's background lookup: `r=gameid` once per game, `r=allprogress` once an hour per console (kept in `achievements/progress/`) |
| `chime.rs` | The unlock sound, synthesised at the core's rate and mixed into the game's samples |
| `game.rs` | pocket-core's three calls: `start`, and `show` for the HUD |

**Threads.** The game thread calls `Session::frame()` after every emulated frame (`rc_client_do_frame`; fast-forward:
after each run), `idle()` while paused, asleep or rewinding (rc_client's timers: retries, the rich-presence ping every
two minutes), `reset()` after a state load, an instant resume and the end of a rewind (achievements wait for their
condition to be false again, so a loaded moment can't unlock anything), and takes notices for the HUD. It never waits:
`server_call` only queues. The **"achievements"** thread hashes the ROM (`hash_rom`: the file, or the ROM inside a
zip; arcade and DOS zips as they are), signs in with the token, loads the set once the core's memory is readable, sends
unlocks of earlier sessions, fetches badges (`achievements/badges/`) and writes the files at most once a second (and at
once when the pause menu opens: `pause` command). **"achievements-net"** runs the requests one at a time and hands the
answers back to rc_client on that thread (rc_client locks itself). `rc_client_set_allow_background_memory_reads(0)`:
memory is read on the game thread only.

**Memory.** `libretro.rs` copies `RETRO_ENVIRONMENT_SET_MEMORY_MAPS` (the core lends the array for the call only) and
answers `SET_SUPPORT_ACHIEVEMENTS`. `Core::achievement_memory()` hands the map and `retro_get_memory_data/size` to
rcheevos' `rc_libretro_memory_init`, which lays them out the way RetroAchievements addresses each console. A core
whose memory appears only once the game runs is retried once a second; the set is loaded after that (rc_client checks
every address once at load). `rc_libretro_is_system_allowed` keeps cores RetroAchievements doesn't trust for a console
out. Session end: `close(300 ms)` before the core unloads; whatever is still on its way is in `pending.json`.

**Softcore only.** Hardcore forbids loading states, and instant resume is a state load. Leaderboards are hardcore-only
in rc_client, so they don't run.

**Testing without an account.** `cargo test --lib achievements` runs rc_client against canned server answers (the
formats of rcheevos' own tests): login, set, unlock by memory, award, reset after a state load, offline start from the
kept set, a late unlock sent with `o=<seconds>`, unknown game (404 `not_found`), refused token. In a game on the
virtual device, `POCKET_CORE_TEST=1` and the command `achievement-demo` show an unlock card with its sound.

**Checking the real server** (needs an account, Wi-Fi): Settings → Playing → RetroAchievements → Sign in; the log says
`achievements: signed in as <name>` and `retroachievements.json` holds a token. Start a game with a set (most
commercial Game Boy and NES games): the log shows `achievements: <rom> hashed in N ms: <md5>`, `[rcheevos] Identified
game ...`, `achievements: N of M unlocked`, and the "Achievements: N of M" note appears. The pause menu lists them.
