# Options

Pause menu → **Options** lets you dial in visuals, controls, cheats, shortcuts and performance for the game you're
playing. This page covers games running in pocket-ui's own player ([Systems](systems.md#on-pocket-uis-own-host)).
PlayStation 2, GameCube, Wii and 3DS have their own compact Options ([Options for PlayStation 2, GameCube, Wii and
3DS](options-big.md)), while RetroArch and standalone emulators open [Runs on](#runs-on). Want to tweak a game before
launching it? Press START on its card for [This game](#this-game).

| Entry | What it holds |
|---|---|
| **Picture** | Scaling, visual look, screen frame, orientation, 3D resolution, Mode 7 and Dreamcast widescreen, with a live preview. |
| **Emulator** | Core-specific tweaks like colour palettes or sprite limits (shown when the core offers them). |
| **Controls** | Per-button mapping, stick/D-pad swaps, DS stylus/layout actions and Hold to toggle. |
| **Cheats** | Libretro cheat database plus your own `.cht` files (one-time Wi-Fi download on TrimUI). See [Cheats](options-cheats.md). |
| **Shortcuts** | Toggle individual HOME + button in-game shortcuts. |
| **Speed, rumble, fan** | Fast-forward speed, rewind, Earlier states, on-screen overlay, rumble, performance, fan and run-ahead. |
| **Zoom** | Magnify the paused frame 2x/3x for tiny text ([Zoom](big-text.md#zoom)). |
| **Runs on** | Pick which emulator/core runs this game or console (uses its own Save changes; see [Runs on](#runs-on)). |
| **Keep for** | Save tweaks to **This game** or **This console** (**Restore defaults** underneath resets them). |

Move with Up/Down (wraps at ends), change inline values (**‹ value ›**) with Left/Right or A, open subpages (**›**)
with A, and go back with B. **Every change applies live and saves immediately** to whichever scope **Keep for** shows
([below](#keep-for)).

## Picture

Pause menu → Options → **Picture** previews your tweaks live over the paused frame with two side-by-side panels:

- **Whole screen:** zoomed-out preview showing scaling, placement and screen frame.
- **Actual size:** 1:1 crop from the centre showing scanlines, LCD grid, Old TV mask and Smooth exactly as they look in
  motion.

Updates live on every press (or shows "No picture" if unavailable). With Large text, the list scrolls cleanly beside the
preview.

### Screen scaling

| Choice | What it looks like |
|---|---|
| **Integer** | Scales by whole pixel multiples so every pixel is identical, tack-sharp with black borders (e.g. Game Boy 7× at 1120×1008, GBA 5× at 1200×800, NES/SNES 4× at ~1170×896 / 1195×896). Picks the largest whole multiple that fits. |
| **Integer, smaller** | One whole step down so pixels stay razor-sharp while leaving breathing room for screen frames (Game Boy 6×, GBA 4×, NES/SNES 3×). |
| **Aspect** | Fills as much screen as possible while keeping the console's true aspect ratio (slightly uneven pixel sizes). |
| **Fullscreen** | Stretches edge to edge across the entire display. |

### Look

Pick the vibe of your screen in one row. Handhelds include **LCD grid**; TV consoles, arcade and computers include
**Scanlines** and **Old TV**. **Custom** unlocks manual Sharpness and Effect combos below.

| Choice | What it looks like |
|---|---|
| **Clean** | Pure, even pixels with zero filters (default on every console). |
| **Smooth** | Sharp pixels with jagged diagonal stair-steps gently rounded off. |
| **Soft** | Subtle blur across the picture for a softer vintage look. |
| **Bloom** | Crisp image with gentle glow around bright highlights. |
| **LCD grid** | Handhelds: fine dark pixel grid like a classic portable screen. |
| **Scanlines** | TV consoles, arcade and computers: subtle CRT scanlines that brighten naturally on light areas. |
| **Old TV** | Scanlines plus a fine phosphor colour mask and soft glow (image only; pair with a [Screen frame](options-screen.md#screen-frame) for the TV cabinet). |

Picking a Look sets Sharpness and Effect together without touching scaling or frames. Any custom combo (like Soft +
Scanlines, or LCD grid on PlayStation) displays under **Custom** automatically.

Quick recommendations: **LCD grid** on handhelds; **Old TV** + Integer scaling on NES, SNES, Mega Drive and PC Engine;
**Clean** on PlayStation, Saturn, N64, PSP, Dreamcast and DS.

**LCD colours** *(GBA, shown with LCD grid)*: **On** applies the richer, warmer palette GBA games were tuned for (turns
on automatically with LCD grid; hidden if the core's own colour correction is enabled under Options → Emulator).

### Custom: Sharpness and Effect

Picking **Custom** reveals two sub-rows: **Sharpness** (**Sharp** hard pixels, **Crisp** balanced sub-pixels at any
scale, **Soft** gentle blur) and **Effect** (**None**, **LCD grid**, **Scanlines**, **Old TV**, **Bloom**, **Smooth**; all
available on any console).

### Screen frame and Screen orientation

For console shells, CRT bezels, Ambient glow and vertical TATE rotation, see
[Screen frame and Screen orientation](options-screen.md).

### Resolution

Available for Nintendo 64, PSP, Dreamcast and PlayStation on Snapdragon 865 and Snapdragon 8 Gen 2 handhelds (both
pocket-ui OS and pocket-ui for Android). Sets internal 3D rendering sharpness: **Original** (N64 320×240, PSP 480×272,
Dreamcast 640×480, PS1 ~320×240) up to **2x**, **3x** or **4x**, offering only multipliers verified to run full speed on
your hardware.

- **Takes effect on next launch** (just like PS2/GameCube/Wii/Xbox; see [their Options](options-big.md)), saved per
  console or per game via [Keep for](#keep-for) → This game.
- Defaults to the sharpest full-speed tier per console:

| Handheld | Nintendo 64 | PSP | Dreamcast | PlayStation |
|---|---|---|---|---|
| Retroid Pocket Mini V2, Mini, Pocket 5, Flip 2 (Snapdragon 865) | 2x (3x offered) | 2x (3x offered) | 2x | Original (2x offered) |
| Retroid Pocket 6, Nova, AYN Odin 2 family (Snapdragon 8 Gen 2) | 2x (3x offered) | 2x (3x offered) | 2x | Original (2x offered) |

  *(Other Android chips, H700, RK3566 and RK3326 omit this row, since H700/Rockchip use ROCKNIX's emulators for N64/PSP/Dreamcast
  and PS1 Enhanced Resolution stays under [Emulator](#emulator).)*
- The TrimUI Brick has no Resolution row either: N64 starts at its own 320×240 there, for full speed. For a sharper 640×480 picture
  set **Native Resolution Factor** to 2x under [Emulator](#emulator) (busy scenes run slower).
- PS1 defaults to Original because 2x (Enhanced Resolution) can black out pause menus in a few titles (like Crash
  Bandicoot 2).
- When this row is present, the duplicate resolution switch under Emulator is hidden to keep one clear place.

### Mode 7

Super Nintendo games on Snapdragon 865 and 8 Gen 2 handhelds (pocket-ui OS and pocket-ui for Android). Enhances the
SNES's rotating/tilting Mode 7 floors in *Super Mario Kart*, *F-Zero*, *Pilotwings* and more:

- **Original:** classic SNES resolution (Snes9x).
- **HD:** crisp high-res Mode 7 floors (2× on Snapdragon 865, 3× on Snapdragon 8 Gen 2), so distant tracks look razor clean
  while sprites and HUD stay authentic.
- **HD widescreen:** extends Mode 7 tracks to fill 16:9 displays on widescreen handhelds (Retroid Pocket 5, Flip 2,
  Pocket 6, Odin 2 family; omitted on 4:3/square screens like Mini V2). Menus stay 4:3, and sprites near screen edges
  may pop in slightly since the original game never drew off-screen.
- pocket-ui for Android offers Original and HD (its bsnes core supports HD without widescreen).
- On pocket-ui OS, HD / HD widescreen run on **bsnes-hd** instead of Snes9x (slightly warmer CPU; in-game saves
  carry over, while Continue states from the other core stay separate so the game starts fresh once). pocket-ui brings
  its own bsnes-hd for cards whose system has none; if it is still missing, the game plays Original on Snes9x and says so.
- **Takes effect on next launch**, saved per console or per game via [Keep for](#keep-for) → This game.

### Widescreen

Dreamcast games, wherever pocket-ui plays them with Flycast (pocket-ui OS, pocket-ui beta for dArkOS and pocket-ui for
Android; on pocket-ui OS the H700 and the other small-chip handhelds keep the Dreamcast on the system's own emulator).
Shows more of the game at the sides, in 16:9:

- **Off** (default): the console's own 4:3.
- **Patch:** Flycast's widescreen patch for the game, made for that game and clean. Many well-known games have one
  (*Sonic Adventure*, *Crazy Taxi*, *Soulcalibur* and more); a game without one stays 4:3.
- **Hack:** every 3D game draws wider. Works everywhere, but the revealed sides can show gaps, missing walls or
  objects popping in; menus and 2D screens stay 4:3 in the middle.
- The picture takes the 16:9 shape by itself (with the Dreamcast's starting scaling, Aspect). On a 4:3 or nearly square
  screen like the Mini V2's it gets black bars above and below instead.
- Vertical arcade-style games stay as they are.
- **Takes effect on next launch**, saved per console or per game via [Keep for](#keep-for) → This game. The Emulator
  page's own widescreen switches are hidden to keep one clear place.

### Starting values per console

| Consoles | Starts with |
|---|---|
| Disc and 3D consoles: PlayStation, Mega-CD, PC Engine CD, Saturn, Nintendo 64, PSP, Dreamcast | **Aspect**, **Crisp**, None, frame None |
| Nintendo DS (stacked dual screens) | **Aspect**, **Crisp**, None, frame None |
| ScummVM (320×200 adventures) | **Aspect**, **Crisp**, None, frame None |
| All other handhelds and 2D home consoles | Integer, Sharp, None, frame None |

Every console starts clean with no effect or frame. Turn on [Settings → Playing → Screen frames](settings-playing.md#screen-frames)
(**Handhelds** or **Every console**) to default to **Auto** frames without overriding per-game or per-console choices.

## Emulator

Pause menu → Options → **Emulator** exposes the active core's own tweaks (palettes, colour correction, accuracy hacks)
while hiding desktop clutter. Open groups with A/Right, adjust with Left/Right, skip 5 values on long lists with L1/R1.
Settings needing a restart note it right below; others apply when you resume.

## Controls

Pause menu → Options → **Controls** customises in-game controls per game or console:

- **Left stick as D-pad** (On/Off): On by default except PlayStation (where both sticks are analogue).
- **D-pad as left stick** (On/Off, stick-based consoles like PS1/N64/PSP/Dreamcast): lets stickless handhelds (TrimUI
  Brick, RG35XX Plus/SP, RG28XX, RG34XX) walk smoothly in 3D games (On by default for N64/PSP/Dreamcast on stickless
  units; moving a real stick always wins).
- **Button rows** (A, B, X, Y, L1, R1, L2, R2, L3, R3, Select, Start, D-pad): map each physical button to any console
  input or **None**.
- **Nintendo DS actions:** bind **Stylus mode** or **Screen layout** to any button (saved per game or console on Linux
  and Android). On stickless handhelds, **L2** toggles Stylus mode (D-pad moves pointer, DS A touches/drags; release held
  directions when toggling) and **R2** taps to swap screens (top/bottom or top-only/bottom-only) or holds to cycle
  layouts. Handhelds with sticks keep their defaults.
- **Hold to toggle** ([details](big-text.md#hold-to-toggle)), **Reset controls** and **Controller setup**
  ([per-controller mapping](controls-settings.md#controller-setup)).

Multiplayer maps use the same per-game layout by physical button position (printed right button = A, unaffected by menu
[Button layout](settings-sound.md#button-layout)). HOME and SELECT + START quit always stay bound to their real buttons.
On TrimUI Brick, F1 and F2 are global shortcuts configured under Settings → Controls → **Lights & buttons** and cannot
be mapped to console inputs.

## Cheats

Available on pocket-core whenever the libretro cheat database or your own `.cht` file matches the game (one-time Wi-Fi
download on TrimUI). Full guide: [Options: Cheats](options-cheats.md).

## Shortcuts

Pause menu → Options → **Shortcuts** toggles each HOME + button combo in pocket-core:

| Row | Keys | What it does |
|---|---|---|
| **Save state** | HOME + R1 | Quick-save to the active slot. |
| **Load state** | HOME + L1 | Quick-load from the active slot. |
| **Change slot** | HOME + Left / Right | Cycle active save slot (1 to 8). |
| **Fast-forward** | HOME + R2 | Toggle fast-forward on/off (speed set under [Speed, rumble, fan](#speed-rumble-fan)). |
| **Rewind** | Hold HOME + L2 | Rewind while held (requires Rewind On under [Speed, rumble, fan](#speed-rumble-fan)). |
| **Screenshot** | HOME + A | Save a [screenshot](playing.md#screenshots) silently without pausing. |
| **Over the game** | HOME + Y | Cycle [Over the game](pause-menu.md#over-the-game) (Off → FPS → FPS + sensors → Pixel Path) until you quit. |
| **Zoom** | HOME + X | Magnify the paused frame ([Zoom](big-text.md#zoom); B returns; Off by default). |

Brief toasts confirm actions ("Saved to slot 3", "Slot 4 is empty", "Screenshot saved"). Inputs never leak into the game
while HOME is held, and analogue L2/R2 trigger halfway down. Disable any shortcut here individually.

### Shortcuts in RetroArch games

RetroArch games on pocket-ui OS get the same HOME combos automatically (respecting any switches turned Off here):

| Keys | In RetroArch |
|---|---|
| HOME + R1 | Quick-save (shared with pause menu Save/Load). |
| HOME + L1 | Quick-load. |
| HOME + R2 | Toggle fast-forward. |
| Hold HOME + L2 | Rewind (when **Rewind** is On under Runs on). |
| HOME + A | Capture a screenshot into Settings → Games → Screenshots. |
| HOME + Y | Toggle RetroArch's FPS counter (follows Over the game switch). |
| HOME + X | Open RetroArch's own menu. |
| HOME + START, twice | Quit RetroArch and save Continue state (double-tap prevents accidents). |

HOME still opens the pause menu and HOME + SELECT opens [Recent games](playing.md#recent-games). **Change slot** and
**Zoom** aren't available in RetroArch; check Pause menu → Options (Runs on) → **Shortcuts** mid-game for the exact
active list.

On ROCKNIX, pocket-ui writes only hotkey lines to `retroarch.cfg` and sets `system.autohotkeys=0` (using your pad's HOME
key, or SELECT if it has none). In pocket-ui for Android and pocket-ui beta for dArkOS, RetroArch keeps its own keys.

## Speed, rumble, fan

Pause menu → Options → **Speed, rumble, fan** (bold values are defaults):

| Setting | Choices | What it does |
|---|---|---|
| **Fast-forward speed** | 2x, 3x, **4x**, 8x, As fast as possible | Fast-forward multiplier for HOME + R2 (mutes audio, displays "FF 4x"). |
| **Rewind** | **Off**, On | Buffers the last minute so holding HOME + L2 rewinds smoothly ("REWIND", muted; costs extra CPU/battery). |
| **Earlier states** | Off, **On** | Auto-snapshots every 5 minutes (last 30 min) plus session start for pause menu **Earlier** ([Earlier](pause-menu-saves.md#earlier)). **Defaults Off on N64, PSP, DS, Dreamcast and Saturn** (8 to 40 MB states). Capped at 100 MB/game and 1 GB total. |
| **Over the game** | **Off**, FPS, FPS + sensors, Pixel Path (with Pixel Path in the pause menu) | Live overlay: **FPS** ("60 FPS   CPU 23%"), **FPS + sensors** (adds temp, fan, battery; [details](pause-menu.md#the-fps-line-and-sensors)), or **Pixel Path** ([details](pause-menu.md#pixel-path-over-the-game)). Defaults to Settings → Playing → [Over the game](settings-playing.md#over-the-game). |
| **Rumble** | Off, Low, **Medium**, High | Vibration intensity for rumble-capable games. |
| **Performance** | Battery, **Auto**, Max | **Auto** *(formerly Balanced)* scales dynamically; **Battery** caps CPU around half speed for light 2D; **Max** locks top clocks. |
| **Fan** | Very quiet, **Quiet**, Auto, Cool | Game cooling curve (smooth with [Smooth fan](performance.md#smooth-fan) On; hidden on fanless H700 and TrimUI Brick). |
| **Run-ahead** | **Off**, 1 frame, 2 frames | Cuts 1 to 2 frames of internal game latency on supported 8/16-bit and handheld cores (NES, FDS, SNES, GB/GBC/GBA, Mega Drive, Master System, Game Gear, SG-1000, PC Engine, SuperGrafx, Atari 2600, WonderSwan, Virtual Boy). See [Run-ahead](#run-ahead). |

- Rewind starts Off because state history uses noticeable CPU (especially on SNES); loading a state resets the rewind
  buffer.
- Performance and Fan reset automatically when you exit, even after a crash.
- Games without overrides inherit Settings → Playing → [Performance & fan](performance.md) (or
  [Quick settings](quick-settings.md#quick-settings)); **Restore defaults** reverts to those baselines.

### Run-ahead

Many classic games inherently take 1 to 2 frames to respond on real hardware. Run-ahead simulates ahead with your inputs to
hide that lag, so jumps and dodges feel snappier without changing gameplay.

- **Recommended starting points:** start with **1 frame**; try **2 frames** only if a game still feels slightly heavy
  (if graphics visibly twitch when pressing a button, step back to 1 frame):

  | Console | Start with |
  |---|---|
  | NES, Game Boy, Game Boy Color, Master System, Game Gear, SG-1000 | 1 frame |
  | SNES, Mega Drive, PC Engine | 1 frame (many games handle 2) |
  | Game Boy Advance | 1 frame (2 frames can overwhelm heavy GBA games like F-Zero: Maximum Velocity on Mini V2 and auto-disable) |
  | Atari 2600, WonderSwan | 1 frame |
  | Famicom Disk System, SuperGrafx, Virtual Boy | 1 frame |

- Runs 2 to 3 internal frames per visible frame, using more CPU/battery ([HOST.md](../HOST.md)).
- Auto-disables cleanly with a message if a core doesn't support it, drops frames or fails deterministic replay.
- Pauses automatically during fast-forward and rewind; saves and cheats always use the real game state. Saved per game
  or console via [Keep for](#keep-for).

## Keep for

Changes save live; **Keep for** decides whether they apply to **This game** or **This console** (toggle with Left/Right
or A; starts on **This console** if the console already has custom settings, else **This game**). **Restore defaults**
(press A twice) wipes both game and console overrides. Games with their own overrides stay independent until you switch
**Keep for** back to **This console** or restore defaults.

## Runs on

On ROCKNIX, if an emulator or its core is missing, the game stays on the shelf and tells you what to install.
You can choose a different emulator here.

Choose which emulator runs a game or console, opened via **START** on a console page, START on a game card → **Runs on**,
or Pause → Options → **Runs on** (or Pause → **Options** on RetroArch/standalone; use card START for PS2/GameCube/Wii/3DS).

The top row shows the active emulator (e.g. `pocket-core · mGBA` or `RetroArch · gpSP`), starting with **Default (…)**
followed by pocket-core and every working ROCKNIX emulator for that console:

- **pocket-core:** instant Continue, eight visual save slots, live Options, shortcuts, run-ahead, fast switcher and
  RetroAchievements.
- **RetroArch:** single auto-save on Quit, slower start, without pocket-ui's slots, Options or achievements.
- **Standalone:** ARMSX2, Dolphin and Azahar keep pocket-ui Save/Load/Options/Continue; others use in-game saving only.

Save states never cross emulators; switching emulators boots from your in-game save.

The choice is stored as ROCKNIX's `<console>.emulator` and `<console>.core` lines in `system.cfg`, so ROCKNIX's own menu
sees it too. If you edit those lines by hand (over Samba or SSH) while pocket-ui runs, it uses them after **Restart
pocket-ui** (hold SELECT + START on the shelf); a choice made here or in ROCKNIX's own menu counts at once.

When RetroArch is picked, extra rows appear (applied on next launch): **Aspect ratio** (Core provided, 4:3, 3:2, 16:9,
Square pixels, Full screen), **Integer scaling**, **Smoothing**, **Shader** (None plus curated ROCKNIX CRT/LCD presets
like `crt-geom` and `lcd-grid-v2`), **Rewind**, **Run-ahead** (1 to 2 frames) and **Auto save and load** (turning Off
disables Continue). Rewind/run-ahead hide on heavy systems (N64, PSP, Dreamcast, Saturn, 32X). Includes **Shortcuts** in
the pause menu, **D-pad** / **Screen size** for [Phone games](systems.md#phone-games-j2me), and **D-pad** (**Automatic**,
**Left stick**, **D-pad**) for N64 and Dreamcast on RetroArch so stickless handhelds steer smoothly (never overwriting
your own RetroArch remaps; PPSSPP auto-maps D-pad to nub on stickless devices too).

Because emulator changes take effect on next launch, press **Save changes** (**For this console**, **For this game**, or
**Restore defaults**) to commit. Choices sync with ROCKNIX's own Emulator setting (`AUTO` = pocket-core). Filenames with
special characters (`[`, `*`, `$`) save at the console level.

## This game

Press **START** on any game card or box to configure it right from the shelf before launching:

| Row | What it holds |
|---|---|
| **Screen scaling**, **Look** (Sharpness & Effect under Custom), **Screen frame** (+ colour/finish), **Screen orientation**, **Resolution** (N64/PSP/Dreamcast/PS1), **Mode 7** (SNES), **Widescreen** (Dreamcast) | All [Picture](#picture) settings (previewed once in-game). |
| **Fast-forward speed**, **Rewind**, **Earlier states**, **Over the game**, **Rumble**, **Performance**, **Fan**, **Run-ahead** | Same rows as [Speed, rumble, fan](#speed-rumble-fan). |
| **Save state**, **Load state**, **Change slot**, **Fast-forward**, **Rewind**, **Screenshot**, **Over the game**, **Zoom** | All [Shortcuts](#shortcuts). |
| **Controls** | Button remapping and stick/D-pad options ([Controls](#controls)). |
| **Runs on** | Emulator selection ([Runs on](#runs-on); B returns here). |
| **Resolution** and emulator rows | [Options](options-big.md) for PS2, GameCube, Wii, 3DS and Xbox (applied next launch). |
| **Use the console's settings** | Clears this game's override so it inherits its console defaults again (shown only when customised). |

Changes save immediately to **this game's** settings, sharing the exact same file as Pause → Options with **Keep for →
This game**.
