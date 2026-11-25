# Systems

pocket-ui automatically displays every console ROCKNIX supports that has at least one game in its folder. Each game runs
on one of three emulator tiers:

- **pocket-ui's own player (pocket-core):** built specifically for pocket-ui, with crisp integer scaling, a full pause menu
  with eight save slots and live Options, HOME shortcuts and instant Continue. On most 2D systems it uses less CPU than
  RetroArch (roughly half on GBA and SNES) with low audio latency (~45 ms measured end to end).
- **RetroArch:** ROCKNIX's libretro player, using ROCKNIX's chosen core for that console.
- **Standalone emulators:** ROCKNIX's native emulators for heavier 3D systems like PlayStation 2, GameCube, Wii and 3DS.

For systems outside pocket-core, pocket-ui uses ROCKNIX's default emulator unless you override it. Press START on a game
card or console page, or open Options in the pause menu, and pick an emulator under [Runs on](options.md#runs-on) per
game or per console. Existing per-system or per-game emulator picks from ROCKNIX carry over both ways.

Most consoles also have a [console page](console-pages.md) (press X on the system) with hardware specs and
[Credits & links](console-pages.md#credits--links) to designers, shared chips and related consoles in your library.

## On pocket-ui's own host

| System | Folders | Emulator core |
|---|---|---|
| Game Boy | `gb`, `gbh` | Gambatte |
| Game Boy Color | `gbc`, `gbch` | Gambatte |
| Game Boy Advance | `gba`, `gbah`, `gbav` | mGBA; gpSP on [H700 and RK3326](#game-boy-advance-on-h700) and TrimUI Brick/Smart Pro |
| NES / Famicom | `nes`, `famicom`, `nesh` | Nestopia |
| Famicom Disk System | `fds` | Nestopia (needs a BIOS, see below) |
| SNES / Super Famicom | `snes`, `sfc`, `snesh` | Snes9x |
| Mega Drive / Genesis | `megadrive`, `megadrive-japan`, `genesis`, `genh`, `megadriveh` | Genesis Plus GX; PicoDrive on TrimUI Brick/Smart Pro |
| Master System | `mastersystem` | Gearsystem; PicoDrive on TrimUI Brick/Smart Pro |
| Game Gear | `gamegear`, `ggh` | Gearsystem; PicoDrive on TrimUI Brick/Smart Pro |
| SG-1000 | `sg-1000` | Gearsystem |
| PC Engine / TurboGrafx-16 | `pcengine`, `tg16` | Beetle PCE Fast |
| PC Engine SuperGrafx | `supergrafx` (HuCards, see below) | Beetle SuperGrafx |
| WonderSwan, WonderSwan Color | `wonderswan`, `wonderswancolor` | Beetle WonderSwan |
| Atari 2600 | `atari2600` | Stella |
| Virtual Boy | `virtualboy` | Beetle VB |
| PlayStation | `psx` | PCSX ReARMed |
| Nintendo 64 | `n64` | Mupen64Plus-Next; [RetroArch on H700, RK3566 and RK3326](#on-standalone-emulators) |
| PSP | `psp` | PPSSPP; [standalone PPSSPP on H700, RK3566 and RK3326](#on-standalone-emulators) |
| Dreamcast | `dreamcast` | Flycast; [RetroArch Flycast 2021 on H700, RK3566 and RK3326](#on-standalone-emulators) |
| Nintendo DS | `nds` | melonDS DS; **DSperate** on H700, RK3566, RK3326, TrimUI Brick/Smart Pro and dArkOS |
| Sega Saturn | `saturn` | YabaSanshiro (needs a Saturn BIOS, see below); Yabause on TrimUI Brick/Smart Pro |
| DOS | `pc` | DOSBox Pure |
| ScummVM | `scummvm` (see below) | ScummVM |
| PICO-8 | `pico-8` | FAKE-08 (unless official PICO-8 is installed, see below) |
| TIC-80 | `tic-80` | TIC-80 |

Only these folder names launch in pocket-core; other folders use ROCKNIX's default emulator. Both `.zip` and `.7z`
archives play directly in pocket-core when they contain one game file (see
[a zip file that won't start](troubleshooting.md#a-zip-game-says-it-holds-several-games-is-damaged-or-cant-be-opened)).
Folders ending in `h` (`nesh`, `snesh`, `gbh`, `gbch`, `gbah`) hold ROM hacks and fan translations; `gbav` holds Game
Boy Advance Video carts.

Run [Game check](game-check.md) (Settings → Games) anytime to spot missing, misplaced or mismatched BIOS files. Aside
from Saturn and Famicom Disk System, none of the systems above require a BIOS (Dreamcast uses `roms/bios/dc/dc_boot.bin`
if present and starts fine without it).

**Famicom Disk System:** drop `disksys.rom` into `roms/bios` (required in both pocket-core and RetroArch; missing BIOS
shows a clear card). Uses `.fds` files. Press **L1** to flip disk sides and **R1** to eject (listed in Pause → Controls);
disk saves write beside the `.fds` file just like RetroArch.

**PC Engine SuperGrafx:** HuCards (`.pce`, `.sgx`) run in pocket-core; CD games in the same folder (`.cue`, `.ccd`,
`.chd`) require a System Card BIOS and launch in RetroArch just like `roms/pcenginecd`.

**Virtual Boy:** renders one eye's classic red-on-black view. D-pad or left stick maps to the left D-pad; right stick (or
L2 (up), R2 (left), L3 (down) and R3 (right)) maps to the right D-pad. Under Options → Emulator, **Palette** switches red
to white or other tints, **Anaglyph preset** outputs red/blue or red/cyan 3D, and **3D mode** offers side-by-side
(applies next launch).

**Sega Saturn:** pocket-core requires `saturn_bios.bin`, `sega_101.bin` or `mpr-17933.bin` inside `roms/bios`; without
one, Saturn games silently launch in ROCKNIX's standalone YabaSanshiro (which ships an HLE BIOS). Supports `.cue`, `.chd`
and `.iso`. Default controls on Mini V2: bottom = A, right = B, left = X, top = Y, L1 = C, R1 = Z, L2/R2 = L/R, left
stick = D-pad (remappable under Pause → Controls). Note: in-game saves aren't shared with standalone YabaSanshiro yet.

**PlayStation:** supports `.cue`, `.chd`, `.m3u` and `.pbp` with dual analogue sticks ([Controls](options.md#controls))
and in-menu disc swapping ([below](#games-on-several-discs)). On TrimUI Brick, keep `.cue` files alongside all their
track files inside `Roms/psx`. Missing memory cards are created automatically on first launch.

**Nintendo 64, PSP and Dreamcast:** both sticks map as analogue sticks, and in-game saves are shared directly with
ROCKNIX's existing emulators (RetroArch N64 saves, `roms/psp/PSP/SAVEDATA`, and Dreamcast VMUs), so progress carries back
and forth. PSP requires no BIOS: pocket-ui copies ROCKNIX's system fonts/tables into `roms/bios/PPSSPP` if missing
without overwriting your own files. For Wild Arms XF and Crisis Core in pocket-ui's own PSP host, GPU readbacks are
skipped by default to prevent an exit-save hang (an explicit per-game or per-console Emulator setting still wins if you
need to change it for visual effects).

*Jeanne d'Arc* defaults to ROCKNIX's standalone PPSSPP because the libretro core can hang on its intro movie (save
in-game before quitting; standalone PPSSPP uses its own states while existing pocket-core states stay safe; your own
**Runs on** choice always wins).

**Nintendo DS:** **DSperate** is the default DS core on Anbernic H700, RK3566, RK3326 (R36S), TrimUI Brick/Smart Pro and dArkOS
handhelds, delivering much faster software emulation with pocket-ui's full pause menu, while Snapdragon handhelds default
to **melonDS DS**. Switch between cores per game or per console under [Runs on](options.md#runs-on); in-game `.sav`
files are shared (compatible with standalone melonDS too), while save states remain separate per core. Screens stack
vertically at maximum size; tap the bottom screen directly or steer the stylus with the right stick. On handhelds
without touch or sticks, **L2** toggles **Stylus mode** (D-pad moves pointer, A touches) and **R2** taps to swap screens
or holds to cycle layouts, both remappable under Options → Controls.

**DOS:** supports `.zip`, `.dosz`, `.exe`, `.com`, `.bat` or game folders inside `roms/pc`. DOSBox Pure boots single
executables straight in or shows a launcher menu. Touchscreen acts like a laptop trackpad (drag to move, tap to click,
two-finger tap to right-click). Press **L3** for DOSBox Pure's on-screen keyboard, menu and mapper; left stick maps to
joystick/keys, right stick moves the mouse, output is classic 4:3, save states work, and disk writes save to
`roms/pc/<game>.pure.zip` just like RetroArch.

**ScummVM:** lists games from ROCKNIX's "Scan ScummVM Games" (`/storage/.config/scummvm/games`) plus any subfolder
inside `roms/scummvm`. Touch taps directly; A clicks, B right-clicks, L1 skips dialogue, Y = Escape, X = F5 (game menu),
SELECT opens ScummVM's keyboard, START opens ScummVM's menu, sticks/D-pad steer the cursor and R2 slows it down. No save
states; save in-game or via START → Save (shared with standalone ScummVM).

**PICO-8 and TIC-80:** `.p8`/`.p8.png` and `.tic` carts. On PICO-8, A = X and B = O; on TIC-80, touch acts as mouse.
Neither core supports reliable save states (Quit saves no moment, though in-game high scores and cartdata save
normally). Drop official Linux PICO-8 (`pico8_64`) into `roms/pico-8/aarch64` (or `roms/pico-8`) to run native PICO-8
(pause menu shows Continue, Options, Quit) and unlock ROCKNIX's built-in "Splore" browser; without it, any cart
pocket-core can't launch falls back to RetroArch's FAKE-08.

Pause menu on pocket-core: Continue, Save, Load, Options and Save & quit (plus Achievements when signed in; START opens
Quick settings; ScummVM, PICO-8 and TIC-80 show Quit since they lack save states). See [The pause menu](pause-menu.md).

<!-- TODO: Master System, Game Gear, SG-1000, WonderSwan and Atari 2600 were checked only with
     self-made test ROMs, not real games (docs/HOST.md). Famicom Disk System, SuperGrafx, Virtual Boy
     and the second folders (famicom, sfc, the hacks folders) were checked in the virtual device only
     (docs/HOST.md "More systems"). Sega CD, 32X, PC Engine CD, Neo Geo Pocket, Lynx, Atari 7800,
     Pokemon mini, FBNeo arcade, NAOMI and Atomiswave are wired in src/hostsys.rs but not verified,
     so they stay on their default emulator. Update the table as systems are verified. -->

### Games on several discs

Group multi-disc PlayStation games (or other multi-disc pocket-core systems) into one shelf entry with an `.m3u`
playlist or a multi-disc `.pbp` file. That adds a **Disc** row to the pause menu so you can swap discs without leaving
the game ([Changing discs](pause-menu.md#changing-discs)).

Put every disc in its own folder under `roms/psx` alongside a plain text `.m3u` listing one disc per line:

```
roms/psx/Final Fantasy VII/
    Final Fantasy VII (Disc 1).chd
    Final Fantasy VII (Disc 2).chd
    Final Fantasy VII (Disc 3).chd
    Final Fantasy VII.m3u
```

Inside `Final Fantasy VII.m3u`:

```
Final Fantasy VII (Disc 1).chd
Final Fantasy VII (Disc 2).chd
Final Fantasy VII (Disc 3).chd
```

For `.cue`/`.bin` games, list only the `.cue` files. The shelf displays one clean entry named after the folder and hides
the individual discs.

All-lowercase filenames automatically title-case on the shelf ("libbet and the magic floor" → "Libbet and the Magic
Floor"), while mixed-case names keep their exact capitalisation.

On gpSP, GBA games with a real-time clock follow your handheld's clock (including elapsed time while powered off or
after loading an older save state; manual core RTC overrides still win). Keep your clock set accurately for Pokémon
berries and daily events.

### When a game does not start on the own host

If pocket-core can't launch a game, a clear card at the top of the shelf explains why (~8 seconds).

With **Game start animation: Cozy** and **Motion: Full** (Shelf and Cabinet), the cartridge or disc smoothly ejects back
into its case onto the shelf, followed by a plain-English explanation and fix, for example: *"Pocket CD Demo can't start
yet. Sega CD needs a file from your own console that isn't on the card: bios_CD_U.bin, bios_CD_E.bin or bios_CD_J.bin.
Copy one of them into roms/bios, then try again."* Any button skips the eject straight to the message, and A dismisses
it. Easy and Kids Mode show their own error screens.

If pocket-core fails before drawing a single frame, pocket-ui can automatically try ROCKNIX's fallback emulator once
(RetroArch, standalone YabaSanshiro for Saturn, or standalone ScummVM), noting which one it's trying. The next launch
retries pocket-core unless you lock another emulator under [Runs on](options.md#runs-on).

Crashes *after* the first frame, or games that already have pocket-core save states (Auto, slots, Before load, Earlier),
never automatically change emulators so your saved moments stay accessible. Updating pocket-ui clears older automatic fallbacks
so previously failed games get a fresh shot. Missing BIOS errors never fall back either (RetroArch needs the same BIOS,
except Saturn which defaults to standalone YabaSanshiro without a BIOS).

### Game Boy Advance on H700

On Anbernic H700 handhelds and RK3326 handhelds such as the R36S, newly started GBA games default to **pocket-core · gpSP** (keeping the full pause menu,
thumbnail slots and Continue). Games with existing mGBA Continue states or save slots automatically stick with mGBA so
existing states still load. Switch anytime to **pocket-core · mGBA** under [Runs on](options.md#runs-on); save in-game
first since save states don't cross cores. Under Auto performance, H700 games run at full CPU speed and restore clocks
on exit.

## On RetroArch

All other systems run via RetroArch, including Sega CD, 32X, PC Engine CD, Neo Geo Pocket, Lynx, Atari 7800, arcade
boards, SuperGrafx CD and fallback titles.

See [Arcade games](arcade.md) for matching arcade ROMsets and BIOS ZIPs, and
[Patches, translations and MSU-1](patches-msu1.md) for `.ips`/`.bps`/`.ups` patches and SNES MSU-1 packs.

### Phone games (J2ME)

Classic Java phone games (`.jar` in `roms/j2me`) run on RetroArch's FreeJ2ME-Plus. They require a one-time Java runtime
download (~200 MB over Wi-Fi; takes ~170 MB in `/storage/jdk`). Your first launch shows "Loading Java, once" with a
progress bar and jumps straight into the game when done (press B to cancel cleanly).

**No Wi-Fi?** (RG28XX, RG35XX 2024, R36S or offline): download `zulu11.48.21-ca-jdk11.0.11-linux_aarch64.tar.gz`
(197 MB, Azul's build) from pocketui.xyz/java and drop it onto your visible card partition (ROCKNIX first partition,
GAMES drive, or TF2 / `roms`). Launch any `.jar` game and pocket-ui verifies the checksum, installs Java and cleans up
the archive.

On the shelf, phone games sit in a dedicated phone-branded case and boot inside a classic candy-bar phone display.

**Controls:** shown right on the game card: **A** is OK (confirm/jump/shoot), **SELECT** and **START** are left/right
soft keys, **D-pad** is arrows, **Y** = 9, **B** = 7, **X** = 0, **L1/R1** = 1/3, **L2/R2** = * / #, left stick =
2/4/6/8 and L3 = 5. On stickless handhelds, games needing numpad movement can switch [Runs on](options.md#runs-on) ›
**D-pad** to **Number keys 2 4 6 8** (making Y = 5). *(pocket-ui swaps FreeJ2ME-Plus's default Y=OK / A=9 via a core
remap unless you set your own.)*

pocket-ui detects each game's native phone resolution (176×208 Nokia, 240×320 standard, 360×640 touch; defaults to
240×320) and integer-scales it when 2× fits, or scales cleanly to fit. Override any quirky title via Options › Runs on ›
**Screen size** (default **Automatic**). Note: 2D games play great, while 3D J2ME games often render imperfectly and
touch-only games won't respond to buttons.

<!-- TODO: confirm each system's default emulator against es_systems.cfg on the device; the
     examples above come from the developer notes (docs/PER-CONSOLE.md, docs/HOST.md). -->

Pause menu on RetroArch: Continue, Save, Load, Options, Save & quit (START opens Quick settings). Options opens
[Runs on](options.md#runs-on) for core choice plus RetroArch aspect ratio, integer scaling, smoothing, shader, rewind,
run-ahead and auto-save (applied next launch).

## PlayStation 2, GameCube, Wii and 3DS

Powered by ROCKNIX's standalone emulators: **ARMSX2** (PS2), **Dolphin** (GameCube & Wii) and **Azahar** (3DS), on the
Snapdragon handhelds. Of the smaller chips, only RK3566 handhelds play PlayStation 2 (ARMSX2); H700 and RK3326
handhelds have none of the four.

Pause menu: Continue, Save, Load, Options, Save & quit (plain Quit if Settings → Playing →
[Continue where I quit](settings-playing.md#continue-where-i-quit) is Off); START opens Quick settings.

- **Save & Load:** slots 1 to 8 plus Where I quit and Before load (3DS uses Azahar's single quick slot, the same one
  ROCKNIX's HOME + R1/L1 use). See [Save slots](pause-menu-saves.md#save-slots-where-i-quit-and-before-load).
- **Options:** per-game resolution, renderer, aspect ratio and more ([Options](options-big.md)).
- **Quit:** saves your spot cleanly ("Saving and quitting…", waiting up to 35 s; 3DS flashes back briefly while saving)
  and resumes right there next time after a brief boot.

Wii channels (`.wad`) and 3DS `.cia` / compressed Azahar formats (`.zcci`, `.zcxi`, `.zcia`), plus GameCube/Wii games
launched via Dolphin Qt, offer a simpler menu: Continue, Options (Runs on), Quick settings and Quit.

Wii games map your gamepad as a Wii Remote (motion/pointer-heavy titles stay tricky, same as stock ROCKNIX). On 3DS,
screens stack vertically with touch on the bottom screen.
<!-- TODO: Wii and 3DS Save/Load/Quit/continue are implemented but not yet confirmed on the device
     (docs/BIG-EMULATORS.md, "Wii and 3DS"). -->

On first launch per console, pocket-ui applies clean one-time defaults without overwriting later tweaks:

- **PlayStation 2:** Vulkan, 2× resolution, widescreen patches off.
- **GameCube / Wii:** Vulkan, 2× resolution, asynchronous shaders (no stutter), plus GameCube A on the right button if
  unset in ROCKNIX (adjust anytime under Pause → Options → More…).

A few pause-menu essentials (like unbinding HOME from PS2's Analog toggle) re-apply on each start.

## Xbox

*Experimental, on Snapdragon 865 and 8 Gen 2 handhelds.* Place original Xbox XISO `.iso` discs inside `roms/xbox` (no BIOS or HDD image needed). Runs on
**hakuX** inside RetroArch. If hakuX is missing, install a current [pocket-ui update](updates.md).
You can also choose xemu under [Runs on](options.md#runs-on).

- **Pause menu (HOME):** Continue, Options, Quick settings, Quit. [Options](options-big.md#xbox-experimental) lets you
  pick Resolution (Original 480p or 2x 960p) and Screen scaling (Aspect, Fullscreen, Integer), applied on next start.
- **Saves:** in-game saves work and store inside pocket-ui's folder (never cluttering `roms/xbox`). **Save states aren't
  supported yet**; Save, Load and Continue will appear once the core adds them.
- Compatibility, speed and audio vary on this young port; let us know what works for you.
- Credits: hakuX-lr 0.38.0 by portare-ch (PortareOS), based on hakuX by rfandango, GPL-3.0-or-later, release
  linux-v0.38.0 at dbc8b16. Full licence and source links live in `/storage/pocket-ui/licences/NOTICE.md`.

## On standalone emulators

Heavier systems without pocket-ui save-state hooks (including **Wii U (Cemu)**, **PlayStation 3 (RPCS3)** and
**PlayStation Vita (Vita3K)** (experimental, SM8550 handhelds like Odin 2 and Pocket 6/Nova only), plus **WiiWare**,
**NAOMI** and **Atomiswave** (standalone Flycast; requires MAME BIOS ZIPs `naomi.zip`, `naomi2.zip`, `awbios.zip` in
`roms/bios/dc`) open pocket-ui's pause menu on HOME (Continue, Options/Runs on, Quick settings, Quit) and close cleanly
on Quit. Use in-game saves before quitting; Continue isn't available here.

For **PlayStation Vita (Vita3K)**, drop official `.PUP` firmware files into `roms/bios/vita3k`, run **Start Vita3K**
once under Settings → Help & tools → [Tools](extras.md#tools), then place games in `roms/psvita` (`.psvita`, `.pkg` +
key `.txt`, or `.zip`). They appear on your shelf in blue-topped Vita cases.

Nintendo 64, PSP, Dreamcast, DS and Saturn (with BIOS) run in pocket-core on supported hardware after matching or
beating stock ROCKNIX emulators ([HOST.md](../HOST.md)), with automatic fallback if a title won't start. On the smaller
chips (Anbernic H700, RK3566 such as the RG353M and RG DS Plus, RK3326 such as the R36S), **Nintendo DS runs on DSperate
in pocket-core** with pocket-ui's full pause menu, while N64, PSP and Dreamcast stay on ROCKNIX's emulators (RetroArch
Mupen64Plus-Next for N64, standalone PPSSPP for PSP, RetroArch Flycast 2021 for Dreamcast) with the D-pad automatically
mapped to the left stick on stickless units ([Runs on › D-pad](options.md#runs-on)).

## Folder names

Drop games into `roms/<folder>`, one folder per console. Folder names follow ROCKNIX's standard layout; check the
subfolders already created under `roms` on your SD card for the full list.
<!-- TODO: add a complete table of folder names and file types, generated from es_systems.cfg. -->
