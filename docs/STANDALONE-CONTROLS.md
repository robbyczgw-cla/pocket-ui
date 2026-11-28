# Standalone emulators' saved controls

Which file holds a standalone emulator's button map, when ROCKNIX's start script writes it, and which controller it names.
Read from ROCKNIX's start scripts of the 20261001 and 20261101 pins (`packages/emulators/standalone/*/scripts/start_*.sh`,
`control-gen`) and the emulators' sources where the scripts leave it open. The code that acts on it: `src/stale_controls.rs`.

## Why controls can go dead

On the InputPlumber platforms (SM8250: Retroid Pocket 5 and Mini V2; SM8550, SM4450, SM6115, RK3576 and newer; the
20261101 scripts ask whether `/usr/share/inputplumber` is there) a game sees InputPlumber's virtual
"Sony Interactive Entertainment DualSense Wireless Controller" (SDL GUID `0300fd574c050000e60c000011810000`). The real pad
("Retroid Pocket Gamepad", `0300f353202000000130000001000000` on the Mini V2) is held by InputPlumber; its node moves to
`/dev/inputplumber/sources`, so SDL doesn't see it, though `/proc/bus/input/devices` still lists it as js0.

A file that is written once and names one controller stays bound to it. It goes dead when it was made for another
controller: before InputPlumber held the pad, on another handheld (a card or a backup moved over), or with a Bluetooth pad
as the first controller. A Mini V2 on 25.11.2026: YabaSanshiro's `input.cfg` of 21.11. was for the real pad, and
YabaSanshiro had written a keymap for the DualSense with nothing bound. No button worked, Start neither.

## Per emulator

| Emulator (start script) | File with the button map | Written by ROCKNIX | Controller it names | Can go stale |
|---|---|---|---|---|
| YabaSanshiro (`start_yabasanshiro.sh`) | `/storage/.config/yabasanshiro/input.cfg` (ROCKNIX's copy of EmulationStation's entry) and `keymapv2.json` (YabaSanshiro's own, made from `input.cfg`) | only when `input.cfg` is missing; then `keymapv2.json` is removed too (20261101: a device's own keymap by `QUIRK_DEVICE` when ROCKNIX has one) | InputPlumber platforms: the DualSense by name; others: js0's name, else a "joypad" | **yes**: player 1 is found by "<id>_<name>_<GUID>" only; no match, no pad. Handled. |
| Azahar (`start_azahar.sh`) | `/storage/.config/azahar/qt-config.ini`, `[Controls]` `profiles\N\button_*` | copied from `/usr/config/azahar` (the platform's own: InputPlumber, SM8250, RK3566 …) only when missing; pocket-ui does the same (`standalone::defaults`) | every button by SDL GUID and port | **yes**: a GUID that isn't here binds nothing. Handled. |
| Mednafen (`start_mednafen.sh`, `mednafen_gen_config.sh`) | `/storage/.config/mednafen/mednafen.cfg` | only when missing | the first joystick by Mednafen's id (MD5 of the SDL name and its axis, button and hat counts) | yes; not handled yet (needs MD5 of the name; rarely chosen) |
| Dolphin GC/Wii (`start_dolphin_gc.sh`, `start_dolphin_wii.sh`) | `GCPadNew.ini`, `WiimoteNew.ini`, `Hotkeys.ini` | the profile copied at every start, `Device = SDL/0/<name>` written at every start from `control-gen` (SDL's first joystick) | the current first controller | no; pocket-ui's controller bridge seats them anyway (`pad_bridge`) |
| ARMSX2 (`start_armsx2.sh`) | `inis/PCSX2.ini` `[Pad1]` | only when missing | by SDL index (`SDL-0/…`) | no; bridged as well |
| PPSSPP (`start_ppsspp.sh`) | `PSP/SYSTEM/controls.ini` | the folder only when missing | by device index (`10-…`) | no |
| melonDS (`start_melonds.sh`) | `melonDS.ini` `JoystickID=0` (`melonDS.toml` new at every start) | only when missing | by SDL index | no |
| DraStic (`start_drastic.sh`) | `drastic.cfg` `controls_a[…]` | only when missing | by joystick button number | no |
| Flycast (`start_flycast.sh`) | `emu.cfg` `maple_sdl_joystick_0 = 0`, `mappings/SDL_<name>.cfg` | `emu.cfg` once; Flycast writes a mapping per controller name itself | by index; a new name gets a new mapping | no |
| Mupen64Plus (`start_mupen64plus.sh`) | `InputAutoCfg.ini` in `/tmp` | at every start from `default.ini` or the game's `custominput.ini` | sections by name, chosen at the start | no |
| DuckStation (`start_duckstation.sh`) | `settings.ini` | only when missing | by SDL index (`SDL-0/…`) | no |
| ScummVM, PICO-8, xemu, VICE, Hatari | their own files | | SDL's game controller database, no saved controller | no |
| Amiberry, Cemu, Hypseus | rewritten at every start from js0 or `sdljoytest` | | the current first controller | no |

**dArkOS** starts its own emulators (`host-launch.py`); they use dArkOS's RetroArch and its configs for the built-in pad,
which has one name, no InputPlumber. **TrimUI Brick/Smart Pro** standalone emulators bind the built-in pad, also one name.
Neither has this case.

## What pocket-ui does

Before a YabaSanshiro or Azahar start through runemu.sh (`app_launch.rs`, the launch's preparation thread):

- **YabaSanshiro** is reset when `input.cfg` has no entry for the controller ROCKNIX picks, or `keymapv2.json`'s player 1 is
  another controller, or it binds nothing (every button id -1, what YabaSanshiro writes for a controller `input.cfg` doesn't
  know). Both files move into `pocket-ui-controls-<UTC time>/` beside them; ROCKNIX writes fresh ones in the same start.
- **Azahar** is reset when the active profile binds its buttons to GUIDs of which none is a controller the game can see.
  `qt-config.ini` moves aside the same way; pocket-ui and the start script copy ROCKNIX's.
- Names are compared exactly, GUIDs by vendor and product (a firmware version or the name's CRC in the GUID may differ).
- **The player's own map stays:** a file for the controller in hand is never touched, whatever is changed in it; only one
  for another controller, or one with nothing bound, is set aside, and it is kept, never deleted. A Bluetooth pad's own map
  is set aside when a game starts without that pad (the game would have no controls at all); it is in the folder to put back.
- **Only when a fresh file works:** ROCKNIX's choice is a controller the game sees (its event node in `/dev/input`), and
  EmulationStation has controls for it (YabaSanshiro), or ROCKNIX's own `qt-config.ini` binds a controller that is here
  (Azahar). Should ROCKNIX write the same stale file again, it stays the second time (the newest set-aside folder holds the
  same files), so the shelf doesn't say it at every start.
- The shelf says "Controls for Saturn were reset to match this controller." (3DS for Azahar) once, still up when the game
  ends. The log has the reason (`controls: …`).
