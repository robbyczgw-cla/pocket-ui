# In-game menu and per-console options, NextUI style (research 2026-10-21)

> **Status: design notes, largely built** on pocket-core (pause menu, 8 slots, Picture, Emulator, Controls,
> Shortcuts, Extras, per-console and per-game settings). What it does today is in
> [user/pause-menu.md](user/pause-menu.md), [user/options.md](user/options.md) and [HOST.md](HOST.md). "Left out" below lists cheats and rewind tuning;
> both have since been added.

[V] = verified in source, [I] = to verify on the device. NU = research/NextUI, RX = ROCKNIX tree.

## What NextUI does
- **Stock systems** [V]: only 7: FC, GB, GBC, GBA, MD, PS, SFC (`NU/skeleton/SYSTEM/tg5040/paks/Emus/`).
  Everything else is an extra.
- **Curation** [V]: each system's `default.cfg` presets values. A `-` prefix locks a value and hides it from the menu
  (`ma_config.c:35`). This is a blacklist, not a whitelist.
- **Menu** [V]: Continue · Save · Load · Options · Quit (`ma_menu.c:161-165`).
  - Save/Load: 8 slots, chosen with ←/→, each with a preview at half screen size, "Empty Slot" when unused.
    The last slot is remembered per game. Slot 9 is auto-resume.
  - Options: Frontend (scaling, sharpness, effect, FF limit, …), Emulator (core options), Controls, Shortcuts,
    then **Save for console / Save for game / Restore defaults**.
- **Game switcher** [V]: saves state, quits, and the launcher shows a carousel of recents with their state screenshots.

## How it maps onto ROCKNIX
- **Layers already exist** [V]: `get_setting` reads `system["rom"].key` → `system.key` → `system.<key>` → `key` →
  `global.key` (`RX/packages/rocknix/profile.d/001-functions:28-45`). setsettings.sh writes them into RetroArch's
  append config on every launch. Save for console = `<sys>.key`; save for game = `<sys>["<rom>"].key`;
  restore = delete those keys.
- **Changing video options needs a relaunch**: SAVE_STATE → quit → launch again; auto-load restores the moment.
  Expected 2–3 s [I].
  - Scaling: `<sys>.ratio`, `<sys>.integerscale`
  - Sharpness: `<sys>.smooth`
  - Effect: `<sys>.shaderset`
- **Live**, no relaunch [I]:
  - SAVE_STATE_SLOT n / LOAD_STATE_SLOT n
  - FAST_FORWARD(_HOLD)
  - RESET
  - DISK_NEXT/PREV/EJECT_TOGGLE
  - SCREENSHOT
- **Previews** [V]: `.state.png` / `.stateN.png` next to the states in `/storage/roms/savestates/<sys>/`.
- **Core options** [I]: `config/<CoreName>/<CoreName>.opt` (console) or `<rom>.opt` (game).
- **ROCKNIX bug** [V]: `setsettings.sh:1123` checks `[ ! -f "GAMBATTECONF" ]` without `$`, so Gambatte.opt is
  rewritten on every launch. GB palettes therefore have to go through system.cfg keys for now.

## pocket-ui plan (only what people use)
- **Menu:** Continue · Save · Load (8 slots + auto, preview) · Options · Quit.
- **Options:**
  - Picture: scaling, sharpness, effect
  - 1–2 game options per system
  - "For this console" / "For this game" / "Reset"
- **Fixed shortcuts** instead of an editor: hold for fast forward, save/load.
- **Left out:** CPU, ambient LEDs, offsets, core sync, debug HUD, rewind tuning, shader pragmas, cheats.

**Picture defaults on 1240×1080** (integer fits well):
- GB/GBC/GG 7× (1120×1008)
- GBA 5× (1200×800)
- NES/SNES 4×
- Mega Drive and PS1: 4:3 at full width (1240×930)

| System (core on SM8250) | Picture default | Game options |
|---|---|---|
| GB (gambatte) | integer, sharp | palette, frame blending |
| GBC (gambatte) | integer | colour correction, blending |
| GBA (mgba) | integer | colour correction, interframe blending |
| NES (nestopia) | integer / 4:3 | palette, overscan |
| SNES (snes9x) | 4:3 | – |
| Mega Drive (genesis_plus_gx) | 4:3 | 3/6-button pad |
| SMS / GG (gearsystem) | SMS 4:3, GG integer | – |
| PS1 (pcsx_rearmed32) | 4:3, smooth | dithering, hi-res, disc |
| PCE (beetle_pce_fast) | 4:3 | – |
| N64 (mupen64plus_next) | 4:3 | resolution |
| PS2 (ARMSX2), GameCube (Dolphin) | standalone, Vulkan 2x | Save/Load/resume through PINE and Dolphin's pad hotkeys (docs/BIG-EMULATORS.md) |
| NDS, PSP, Dreamcast, Wii | standalone | menu offers Quit only; their own menus stay |
