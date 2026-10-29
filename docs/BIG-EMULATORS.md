# PS2, GameCube, Wii and 3DS: ARMSX2, Dolphin and Azahar (measured 2026-10-21, Wii/3DS added 2026-10-22)

The big systems on the Mini V2 run on ROCKNIX's standalone emulators. pocket-ui gives PS2, GameCube, Wii and
3DS the same pause menu as its libretro host: Continue · Save · Load · Quit, a preview for the save, a resume
state written on Quit and loaded on the next start, and **Options** (Continue · Save · Load · Options · Quit, see
"Options in the pause menu"; Wii and 3DS since 2026-10-22). Code: `src/standalone.rs`, with hooks in `src/main.rs`
(launch, open menu, Save/Load, Quit, exit) and `src/sleep.rs` (resume state before sleep); Options in
`src/emu_settings.rs`. Dolphin's Qt builds (`dolphin-qt-gc`, `dolphin-qt-wii`), WiiWare and Xbox keep
Continue/Quit; so do Wii U and PS3, now as guests with a clean Quit ("Wii U (Cemu) and PS3 (RPCS3)" at the end).

**Status ledger: [STATUS.md](STATUS.md).** PS2 and GameCube were measured on the device (2026-10-21).
**Wii and 3DS are implemented and unit-tested, not yet tried on the device** (see "Wii and 3DS" below for the
pending checks and the exact commands). **Options are not yet tried on the device either** (device test in
"Options in the pause menu"). Their saves can be synced with Syncthing (Settings > Connections > Sync, see "Sync" at the end), also
not yet tried on the device.


## States: slots 1-8, Auto, Before load (2026-10-22, not yet run on the Mini V2)

Supersedes the slot choices of the sections below ("Save = slot 1, Quit/resume = slot 0" and the like); the model is
`src/states.rs`, the user's view `docs/user/pause-menu.md` ("Save slots, Auto and Before load").
- Save writes manual slots 1-8 only. Quit and sleep write **Auto**, loaded by the next start. Every Load first writes
  **Before load**, until the next Quit (a Load of Before load itself leaves it as it is). None of the three shares a place.
- ARMSX2: PINE takes any slot byte. 1-8 are ARMSX2's own, Auto is 99, Before load 98 (its menus have 1-10).
  The old Auto, slot 0, moves to 99 when a preview `.png` sits beside it (pocket-ui writes one, ARMSX2 doesn't).
- Dolphin: slots 1-8 are `.s01`-`.s08`, reached with the hotkeys' Up/Down from the start slot (each step ~0.65 s,
  so slot 8 takes about 10 s; the slot counter is not tracked across calls, as before). Auto and Before load are files
  of ours (`.pocket-auto`, `.pocket-undo`) passed through `.s00`, the start slot outside 1-10: `.s00` is parked, the state
  saved there and renamed (or ours hard-linked there and loaded, watched with inotify like Azahar's quick slot) and
  `.s00` put back. The old layout (Save `.s00`, Quit `.s01`) migrates once per game (`states-v2` marker in the data
  folder): an `.s01` with a pocket-ui preview becomes Auto, `.s00` becomes slot 1 if that is free.
- Azahar: one manual slot, the quick slot; Auto and Before load are `<rom>.auto.cst` and `<rom>.undo.cst` through it, as
  Auto was. Slots 1-10 have menu actions but no shortcut, so there is no honest way to offer more.
- pocket-core: `.pstate` (slot 1) .. `.pstate7`, `.pstate.auto` (when a start has taken it, the `.taken` file still counts
  as Auto), `.pstate.undo`; `load auto`, `load undo` over the command FIFO; the undo is written by pocket-core itself before
  every load, hotkey or menu.
Device checks: see the report of this change (Dolphin slot stepping and the `.s00` transit, ARMSX2 slots 98/99, Azahar's
undo through the quick slot).
- **One action at a time, for one game** (review N4 / SOL-03 / SOL-12, 2026-10-23): Save, Load, Quit, the resume load
  after a start, the save before sleep and power-off, the save of an options restart and the setting hotkeys / "Advanced…"
  chord each take a turn (`standalone::turn`, one mutex) and wait for the action before to finish, so Dolphin's slot steps
  of two actions never interleave and the `.s00`/quick-slot transit is never parked twice. A turn is bound to the launch
  (`Session::id`): a worker whose game has ended does nothing, and `chord`/PINE refuse to reach the next game's emulator
  from a thread bound to an ended one. A save counts as written only when the file is another version than the one
  stamped just before asking (inode, mtime, size), looks the same twice 300 ms apart and the emulator no longer has it
  open (`/proc/<pid>/fd`; `fsutil::wait_new_version`); a file written a moment before the request no longer passes.
  The same check confirms pocket-core's `autosave` before a suspend. A Quit right after a Save can therefore take the
  Save's time longer (still inside `QUIT_GRACE` in the normal case).
- **A parked slot is always put back** (review SOL-04): a start of Dolphin, like one of Azahar, first restores a
  `.s00.pocket-ui` left by a Save or Load that pocket-ui didn't see through (`standalone::transit_of`,
  `azahar::recover`: a state written meanwhile becomes Auto, a link of Auto or Before load just goes), and `park`
  refuses while such a backup exists instead of renaming the next state over the player's own `.s00`.

## Versions
| | ROCKNIX 20261001 ships | Upstream now |
|---|---|---|
| ARMSX2 | "2.7.1" (version forced in package.mk), commit `19c83bf`, 2026-09-24, with ARMSX2's own Turnip `axfl1-011` (Mesa 26.3-devel) | stable 2.8.4 is Android-only; Linux arm64 only as nightlies: `nightly-20261020` = 2.8.4-399-gc66d053fe5, 1281 commits newer |
| Dolphin | 2606 (`6094cfcf`), nogui + Qt, ROCKNIX patches incl. pad hotkeys and Wayland nogui | – (not compared) |

**ARMSX2 nightly vs ROCKNIX's build**, same scene, same CPU governor (ondemand), both on the ARMSX2 Turnip, run
side by side from `/storage/armsx2-nightly` (extracted Qt AppImage, own config/states/memory cards):

| Ico opening, 25 s | fps avg (min) | worst frame | CPU (100 % = 1 core) | °C CPU/GPU |
|---|---|---|---|---|
| ROCKNIX, Vulkan 1x | 59.84 (59.80) | 51.8 ms | 134 % | 46/43 |
| nightly, Vulkan 1x | 59.83 (59.80) | 38.8 ms | 146 % | 44/42 |
| ROCKNIX, Vulkan 2x + vsync | 60.02 (59.25) | 43.5 ms | 130 % | 46/44 |
| nightly, Vulkan 2x + vsync | 60.03 (59.51) | 39.1 ms | 152 % | 46/44 |

Not clearly better (both full speed, the nightly uses ~10–15 % more CPU here), so ROCKNIX's build stays.
If a later nightly is worth it, ship it as a store package rather than over `/usr`:
- artifact: the `Linux-arm64-4K-pages.AppImage` (Qt; the `SDL-handheld` build is for bare kmsdrm and the release
  notes send ROCKNIX users to the AppImage), extracted to `/storage/emus/armsx2-<tag>/` (`--appimage-extract`),
  pinned by sha256 in our signed index (nightly 20261020: `f88bca9d…2032df`);
- a launcher like `start_armsx2.sh` (same `VK_DRIVER_FILES` Turnip, `QT_QPA_PLATFORM=wayland`) that pocket-ui calls
  instead of runemu for PS2 when the package is installed; the config stays `/storage/.config/ARMSX2`;
- states from 2.7.1 and nightlies are not guaranteed to load in each other; keep per-version state folders.
There is also a `libretro-core` nightly; pocket-core can't host it until it offers hardware rendering.

## Control channels
- **ARMSX2: PINE** (PCSX2's IPC) on `$XDG_RUNTIME_DIR/pcsx2.sock`, kept and extended by ARMSX2: save/load slot,
  status, disc serial/CRC, plus `GetStats` (fps, frame times, thread load), `Get/SetSetting` (live).
  `deploy/pine.py` speaks it from SSH. State files: `/storage/roms/savestates/ps2/<serial> (<CRC>).NN.p2s`.
  - Save = slot 1 (ARMSX2's own default slot), Quit/resume = slot 0. Save takes ~0.6 s.
  - Quit: save slot 0 → SIGTERM (ARMSX2 shuts the VM down, memory cards flushed) → wait until PINE reports no VM →
    second SIGTERM ends the process (`-bigpicture` without `-batch` would otherwise stay in its game list).
    Measured 1.5 s from Quit to exit. runemu.sh is not signalled, so it restores the governor itself.
  - Resume: as soon as PINE reports the VM running, `LoadState 0` (the game shows for a moment first).
  - The state names come from PINE's serial and CRC, known once the VM runs (`Session::base`). Before that the pause
    menu has no Save or Load (a Save had no file to go to and failed silently) and says "Save and Load aren't ready
    yet. Save in the game before you quit." (`src/pause_saves.rs`); Quit then writes no resume state.
  - CLI `-statefile` exists but ROCKNIX's start script passes no extra arguments.
- **Dolphin nogui: ROCKNIX's pad hotkeys** (patch `000-hotkeys`): HOTKEY+R save, HOTKEY+L load, HOTKEY+Up/Down
  slot, HOTKEY+Start quit (HOTKEY = Select, or Guide with `gamecube.hotkey_enable_button=mode`). No IPC exists;
  `-s <state>` exists but again can't be passed through the start script.
  - pocket-ui presses them with InputPlumber's `SendButtonChord` (80 ms per button) on the virtual pad Dolphin
    reads. InputPlumber can't press analog triggers (and `SendEvent` panics a worker thread in 0.79.0), while GC
    L/R are the analog triggers. So every stock controller profile in
    `/storage/.config/dolphin-emu/GamecubeControllerProfiles/` also takes HOTKEY + stick click as L/R:
    `` Triggers/R = `Trigger R` | (`Back` & `Thumb R`) `` (GC games don't use the stick clicks; the start
    script copies the profile to GCPadNew.ini on every launch). Save = HOTKEY+R3, load = HOTKEY+L3.
  - Save = slot 0 (`<rom file>.s00`, Dolphin's start slot), Quit/resume = slot 1 (Up, save, Down).
    Save takes ~1.1 s. Quit: save slot 1, then SIGTERM (Dolphin's own graceful stop); 1.6–1.9 s to exit.
    (Measured with the chords before 2026-10-22; since then each chord holds a stick click before HOTKEY,
    see "Wii and 3DS". GameCube is to be re-checked with them.)
  - State names (ROCKNIX patch `002-savestate-names`): a disc image's file name; `.dol`/`.elf` homebrew use
    Dolphin's game id `ID-<file stem>`; `.m3u` the first disc's file name. `.wad` channels are named after the
    title id inside the WAD, which pocket-ui doesn't read: they keep Continue/Quit.
  - HOTKEY follows ROCKNIX's `get_setting hotkey_enable_button` (game, system, then global), like the start
    scripts; before 2026-10-22 only `gamecube.hotkey_enable_button` was read.
  - Resume: 3 s after Dolphin's window appears, slot 1 is loaded (Dolphin's OSD shows "Slot 1 selected" /
    "Loaded state" briefly).
- **Azahar: its Qt keyboard shortcuts**, pressed on InputPlumber's keyboard (`SendButtonChord` takes
  `Keyboard:Key…` names too; the Mini V2's composite device has a `keyboard` target): Quick Save = Alt+S,
  Quick Load = Alt+L, the same keys ROCKNIX's gptokeyb sends for HOME+R1/L1. See "Wii and 3DS".
- **Previews**: the frame grabbed when the menu opened, saved as `<state>.png` at half size (like RetroArch and
  pocket-core).
- Both need the game running while saving, so the pause menu resumes it (SIGCONT) first. On Quit the menu stays
  up with "Saving and quitting…" while the game runs behind it.

## Defaults for the Mini V2
Written the way ROCKNIX does it: keys the start scripts rewrite on every launch go to
`/storage/.config/system/configs/system.cfg` (only if not set already); everything else to the emulator's own INI.
Tuned values are written once (marker `defaults-ps2` / `defaults-gamecube` in pocket-ui's data folder), so later
changes by the player stay. What the menu depends on is enforced on every start.

**PS2** (`ps2.internal_resolution=2`; PCSX2.ini `Renderer = 14` Vulkan, `EnableWideScreenPatches = false`;
enforced: `EnablePINE = true`, `ConfirmShutdown = false`, `VsyncEnable = false`, no `Analog = Guide` binding,
because HOME is pocket-ui's). Measured on Ico's in-engine opening (boat to the castle) from a save state, 20–25 s
per setting, live-switched through PINE:

| setting | fps avg (min) | EE / GS / VU / GPU load | CPU | GPU clock | °C |
|---|---|---|---|---|---|
| ROCKNIX default: Auto(=Vulkan) 1x, vsync off | 59.8 (58.9–59.8) | 45 / 48 / 45 / 11 % | 130–140 % | 305 MHz | 46/44 |
| **Vulkan 2x, vsync off (new default)** | 59.8 (59.5–59.8) | 48 / 42 / 29 / 29 % | 121–134 % | 305 MHz | 47/44 |
| Vulkan 3x | 59.8 (59.8) | 45 / 54 / 45 / 37 % | 145 % | 581 MHz | 47/47 |
| OpenGL 2x | 27 (14 steady) | – | 98 % | 889 MHz | 47/47 |
| Vulkan 2x + vsync + sync to host | 60.0 (59.3–59.9) | 46 / 58 / 54 / 27 % | 130–135 % | 305 MHz | 46/44 |

- 2x (1280×896) is the closest step to the 1080-line panel and costs nothing measurable: the GPU stays at its
  lowest clock. 3x doubles the GPU clock for supersampling only; OpenGL is unusable.
- Vsync: no measurable pacing gain here, and with vsync ARMSX2's GS thread blocks on the compositor while the
  pause menu hides the game, so the Quit save never completed (12 s timeout). Off, as ROCKNIX ships it.
- CPU: ROCKNIX's EE cycle rate/skip 0, MTVU on, fast boot on are fine (full speed with ~1.3 cores busy).
- **ROCKNIX bug**: `start_armsx2.sh` tests `[ "$IRES" > "0" ]`, a redirect into a file named `0`; from a read-only
  working directory the test fails and the resolution is reset to 1x. pocket-ui now starts games from `/tmp`
  (verified: `upscale_multiplier` reads 2 through PINE after a launch).
- **"Current Blending Accuracy is Basic. Recommended Blending Accuracy for this game is High"** (ARMSX2 `19c83bf`,
  `GameDatabase.cpp`, `GSHWFixId::RecommendedBlendingLevel`): `applyGSHardwareFixes` posts it as the keyed toast
  `HWBlendingWarning` whenever the game's recommended level is above the configured one, and
  `VMManager::ApplySettings` runs it again after every settings change, so each PINE SetSetting brought it back.
  Nothing switches this one message off: `EmuCore WarnAboutUnsafeSettings` only gates `VMManager::WarnAboutUnsafeSettings`
  (a different list: "Blending Accuracy is below Basic", fast CDVD, ...), and `quiet` is only set for the settings
  screen's hypothetical run. A per-game INI key that claims the fix would silence it, but that layer beats PINE
  (see "Where the per-game values live"). What works is `[EmuCore/GS] OsdMessagesPos = 0` (None): `ImGuiManager`
  drops every posted toast then, and the FPS counter (`OsdPerformancePos`) stays. pocket-ui sets it on every
  ARMSX2 start (`standalone::defaults::ARMSX2_ENFORCED`). The cost: no ARMSX2 toast at all (state saved, upscale
  above the GPU's limit, alpha-test/AA1 recommendations); pocket-ui has its own notices. Not run on the virtual device
  (no ARMSX2 there); check on the Mini V2: no toast after Options changes, FPS counter still works.

**GameCube** (`gamecube.graphics_backend=vulkan`, `gamecube.internal_resolution=2` = ROCKNIX's defaults, now written
explicitly; Dolphin.ini/GFX.ini are overwritten from `/usr/config` on every start). Measured on Mario Kart:
Double Dash's attract race, Dolphin's own frame log (`deploy/dolphin-frametimes.py`), 20 s:

| setting | fps | frame sd | p99 / max | CPU | GPU clock | °C |
|---|---|---|---|---|---|---|
| **Vulkan 2x (ROCKNIX = new default)** | 59.95 | 1.78 ms | 20.4 / 21.6 ms | 66–103 % | 305 MHz | 45/42 |
| Vulkan 1x | 59.93 | 1.84 ms | 20.4 / 21.7 ms | 99 % | 305 MHz | 46/43 |
| Vulkan 3x | 59.94 | 1.44 ms | 20.0 / 24.2 ms | 106 % | 305 MHz | 46/43 |
| OpenGL 2x | 59.94 | 2.32 ms | 21.4 / 22.0 ms | 103–108 % | 305 MHz | 48/44 |
| Vulkan 2x + vsync | 59.94 | 1.80 ms | 20.3 / 21.2 ms | 102 % | 305 MHz | 47/43 |

2x (1280×1056) matches the 1080-line panel; Vulkan is steadier than OpenGL. This scene is light for the SM8250 (GPU
at its lowest clock even at 3x), so heavier games (Wind Waker, Metroid Prime) may still want 3x off. Vsync off as
shipped, for the same hidden-window reason as PS2 (not seen to hang here, not worth the risk).

**Correction (2026-10-22, from the source, not re-measured):** ROCKNIX's Dolphin carries patch
`004-enable-240p-res`, which divides the EFB scale by two: `InternalResolution = N` renders at N/2 × native
(start_dolphin_gc.sh: "InternalResolution is twice the multiplier (1 = 0.5x)"; its hotkey labels 2 = "480P",
4 = "720P", 6 = "1080P"). If the rows above were set through `gamecube.internal_resolution` 1/2/3, they ran at
0.5x / 1x (640×528) / 1.5x, and the default `2` is native 1x, not 1280×1056.

**Audio**: both emulators play through PipeWire's Pulse layer at its fixed 1024-frame quantum (21 ms,
`node.latency 1024/48000`); ARMSX2 adds its TimeStretch buffer (`BufferMS = 50`, `OutputLatencyMS = 20`). The real
output latency can't be read from outside (Pulse reports 0); left at the emulators' defaults.
**Power**: not measured; USB was connected all session and `bench.sh` only reports watts on battery.

## Xbox (hakuX in RetroArch, 2026-10-28)

The fourth channel is RetroArch's own **network command port** (UDP on localhost, a free port per session, in the appendconfig
`hakux.rs` writes): `QUIT` closes the core and RetroArch cleanly, `GET_STATUS` says when the game runs, and
`SAVE_STATE_SLOT n` / `LOAD_STATE_SLOT n` would save and load once the core has save states (today it reports a serialize
size of 0; `hakux_libretro.info` says `savestate = "false"` and the pause menu offers no Save or Load, only a line saying so).
Everything else is as for the other guests: the game is stopped while the menu is up, Quit has a plan with a fallback to
signals and the process group. All of it, with the licence and the device test, is in [XBOX.md](XBOX.md).

## Options in the pause menu (2026-10-22)
Continue · Save · Load · **Options** · Quit. Options is a short list per emulator, kept per game, applied when the
game goes on (Continue, B or HOME). Code: `src/emu_settings.rs` (one `Table` per emulator: setting, choices,
per-game store, the emulator's INI key, live path), hooks in `src/main.rs` (`Page::Standalone`, `resume_game`,
`begin_reapply`, `game_exited`) and a few accessors at the end of `src/standalone.rs`. Read from the sources ROCKNIX
20261001 builds: ARMSX2 `19c83bf` (package.mk sha256 matches the tarball read), Dolphin 2606 `6094cfcf` + ROCKNIX's
patches, and the start scripts of the image (`/usr/bin/start_armsx2.sh`, `start_dolphin_gc.sh`).

### What can change while the game runs
- **ARMSX2: everything, through PINE.** This fork extends PINE (`pcsx2/PINE.cpp`): `MsgGetSetting 0x11`,
  `MsgSetSetting 0x12`, `MsgGetEffectiveSetting 0x15`, `MsgGetStats 0x10`, `MsgFrameAdvance 0x13`, `MsgGSDump 0x14`.
  SetSetting writes the key into the **base** INI layer (`Host::SetBaseStringSettingValue` + `CommitBaseSettingChanges`)
  and runs `VMManager::ApplySettings()` on the CPU thread; a Renderer change reopens the GS inside the running VM
  (the reply's `restart_required` only says the GS device was torn down). No other external path exists: no
  SIGHUP/SIGUSR handler (QtHost.cpp handles only SIGINT/SIGTERM/SIGCHLD), no file watcher, no DBus; the per-game
  layer is only reloaded by ARMSX2's own UI (`VMManager::ReloadGameSettings`) or on a disc/ELF change.
- **Dolphin nogui: no reload path at all** (no IPC; the game INIs are read once at boot,
  `SConfig::SetPathsAndGameMetadata` → `GameConfigLoader`; the backend is chosen in `Core::Init` →
  `VideoBackendBase::PopulateBackendInfo`). Only ROCKNIX's pad hotkeys (patch `000-hotkeys`, also in the Wayland
  platform of patch 015) change settings live, through `Config::SetCurrent` (the CurrentRun layer, above the game INI):
  HOTKEY+Y toggles `GFX_SHOW_FPS`, HOTKEY+X toggles `GFX_ASPECT_RATIO` Stretch/Auto, HOTKEY+B cycles
  `GFX_EFB_SCALE` 2→4→6→1, HOTKEY+Z fast forward. They are toggles with their own `static int` state from
  Dolphin's start, so pocket-ui counts its presses per launch (`Toggles`) to know how many presses reach a value.
  The buttons are pressed through InputPlumber with the same chord design as Save/Load and the slot steps: both
  stick clicks first, then HOTKEY, then the face button (`dolphin::toggle`, `[L3, R3, HOTKEY, button]`). With both
  clicks held the profiles' extra L/R bindings (`with_extra_triggers`, HOTKEY & one click & not the other) stay off,
  so a setting hotkey never saves or loads. Which InputPlumber button is GC Y/X/B comes from the controller profile
  ROCKNIX copies at launch (`gamecube_controller_profile`: south default, east, custom). The game also sees that
  GC button for ~80 ms, as it sees R/L for Save/Load.
- **Dolphin nogui for Wii: the same hotkeys**, see "Options for Wii" below for how the Wiimote is kept out of them.
- **Azahar: nothing live.** Its settings change only in its Qt dialogs (no IPC; the scripting RPC on UDP 45987 reads
  and writes memory, it has no settings call) and are read at boot. So every 3DS option applies by restart.

| Option | PS2: per-game store → INI key (live) | GameCube and Wii: `GameSettings/<ID>.ini` key (live?) |
|---|---|---|
| Resolution | `ps2["<rom>"].internal_resolution` 1/2/3 → `[EmuCore/GS] upscale_multiplier` (PINE) | `[Video_Settings] InternalResolution` 2/4/6 = 1x/2x/3x (HOTKEY+B) |
| Renderer | `.graphics_backend` 2/1 → `[EmuCore/GS] Renderer` 14 Vulkan / 12 OpenGL (PINE, GS reopen) | `[Core] GFXBackend` Vulkan/OGL (restart) |
| Widescreen | `.enable_widescreen_patches` → `[EmuCore] EnableWideScreenPatches` (PINE; patch list reloads) | `[Video_Settings] wideScreenHack` (restart) |
| Aspect ratio | `.aspect_ratio` 0/1/2 → `[EmuCore/GS] AspectRatio` 4:3 / 16:9 / Stretch (PINE) | `[Video_Settings] AspectRatio` 0 Auto, 1 16:9, 2 4:3, 3 Stretch (Auto/Stretch HOTKEY+X; 16:9, 4:3 restart) |
| Frame skip | – (see below) | `[Video_Hacks] VISkip` "VBI skip" (restart) |
| Show FPS | `.show_fps` → `[EmuCore/GS] OsdShowFPS` (PINE) | `[Video_Settings] ShowFPS` (HOTKEY+Y) |

PS2 has no frame skip on Linux: this fork's manual frameskip (`GSSetManualFrameSkip`) is only reachable through
Android's JNI, with no INI key and no PINE message; `SkipDuplicateFrames` (already on) is not a frame skip.

**3DS (Azahar 2126.2), all by restart.** Read from the image's `start_azahar.sh` and `/usr/config/azahar/qt-config.ini`
and Azahar's `src/common/settings.h`:

| Option | per-game store | qt-config.ini key, who writes it at launch |
|---|---|---|
| Resolution 1x/2x/3x | `3ds["<rom>"].resolution_scale` 1/2/3 | `[Renderer] resolution_factor`, start_azahar.sh (every start; `0` = window size is left out) |
| Screens: Stacked / Large top / Top only | `.screen_layout` 0 / 2 / 1a | `[Layout] layout_option` 0/2/1 and `swap_screen=false`, start_azahar.sh (Side by side `3` and Bottom only `1b` left out: tiny or touch-only on this screen) |
| Renderer Vulkan / OpenGL | `.graphics_backend` 2 / 1 | `[Renderer] graphics_api` 2/1, start_azahar.sh |
| Texture filter Off / Bicubic / ScaleForce / xBRZ | `.pocket_texture_filter` 0/2/3/4 (ours) | `[Renderer] texture_filter`, written by pocket-ui before every 3DS start (`azahar_launch_config`), the game's value or the platform's or 0, with `\default=false`; start_azahar.sh never touches it. Anime4K (1) and MMPX (5) left out |
| Show FPS | `.rocknix.mangohud.enabled` 0/1 | none: runemu.sh starts the emulator through `/usr/bin/mangohud` when that key is 1 (`DEVICE_MANGOHUD_SUPPORT=true` on SM8250); pocket-ui then sets `MANGOHUD_CONFIGFILE=/storage/.config/pocket-ui/mangohud-fps.conf` (`fps_only`, top right) for that start, instead of ROCKNIX's full HUD |

Azahar's own FPS counter is in its Qt status bar, which its fullscreen mode hides, and it has no overlay setting on
desktop (none in `settings.h`), hence MangoHud. start_azahar.sh's per-game lookup is ROCKNIX's awk one (see PS2);
for a ROM name it can't match, the 3DS page says so and keeps nothing.

### Where the per-game values live, and ROCKNIX's rewrite
- **PS2: ROCKNIX's own per-game keys in system.cfg**, `ps2["<rom file>"].<key>=<value>`, not ARMSX2's
  `gamesettings/<serial>_<CRC>.ini`. start_armsx2.sh rewrites PCSX2.ini on every launch (`upscale_multiplier` always,
  line 113–118 of the 20261001 script; the others when ES has a value); with the per-game key its `get_setting`
  finds our value, so the rewrite writes our choice instead of clobbering it (verified with the real script in the
  virtual device: `upscale_multiplier = 3`, `OsdShowFPS = true` after a launch). ARMSX2's own per-game INI would also
  win over the rewrite, but its layer sits above the base layer PINE writes to, so nothing set there could change
  live any more. pocket-ui makes sure `ps2.graphics_backend=2`, `ps2.aspect_ratio=0`, `ps2.show_fps=false` exist
  (added only when missing) so every launch rewrites every key: neither a per-game value nor a live PINE change
  sticks to the next game. ROCKNIX's lookup is an awk regex escaping only `' ( ) &`; for ROM names with
  `[ ] * ? + { } | ^ $ \ "` or double blanks pocket-ui applies the per-game values itself through PINE once the VM runs.
- **A key set in ARMSX2's own menu** (Advanced…, its per-game properties) lands in
  `/storage/.config/ARMSX2/gamesettings/<serial>_<CRC>.ini` and beats PINE. When the player changes such a key in
  pocket-ui's Options, pocket-ui removes it from that INI (atomic, other lines kept) and applies by restart.
- **GameCube: Dolphin's user game INI** `/storage/.config/dolphin-emu/GameSettings/<GAMEID>.ini`, the 6-character ID
  read from the image header (ISO/GCM, RVZ/WIA at 0x58, CISO at 0x8000, WBFS at 0x200, GCZ first block inflated).
  start_dolphin_gc.sh copies GFX.ini and Dolphin.ini from `/usr/config` on every start (unless
  `write_protect_configs=false`) but never touches GameSettings, and the game layer wins over both. The files there
  are ROCKNIX's copies of Dolphin's game INIs with cheats; edits keep every other line.
- **Wii: the same GameSettings INI** (start_dolphin_wii.sh copies GFX.ini/Dolphin.ini the same way). DOL and ELF
  homebrew have no disc header; Dolphin names their game ID `ID-<file stem>` (`SConfig::MakeGameID`, the same name
  ROCKNIX's state names use), so their options go to `GameSettings/ID-<stem>.ini` (from Dolphin's source as remembered,
  **to be confirmed on the device**, see the device test).
- **3DS: ROCKNIX's per-game keys in system.cfg** (`3ds["<rom file>"].resolution_scale`, `.screen_layout`,
  `.graphics_backend`, `.rocknix.mangohud.enabled`) plus our `.pocket_texture_filter`; see the 3DS table above.
  pocket-ui writes the texture filter into qt-config.ini before every 3DS start, so it never sticks to the next game.
- All writes go through `fsutil::write_atomic`.
- **Pixel Path reads the same files** (`src/pixel_standalone.rs`, on a thread, never writing): ARMSX2's
  `gamesettings/<serial>_<CRC>.ini` over `inis/PCSX2.ini`; Dolphin's `GameSettings/<GAMEID>.ini` (`Video_<section>`)
  over `GFX.ini` and `Dolphin.ini`; Azahar's `qt-config.ini` (honouring `\default=true`). Which keys, and how they are
  graded: [CURATOR.md](CURATOR.md#pixel-path-for-ps2-gamecube-wii-and-3ds-featpixel-path-standalone-2026-10-24).
  Without a readable main file the pause menu doesn't list Pixel Path for the game.

### Applying by restart
`emu_settings::Restart`, a small state machine: Saving → Closing → Starting → Loading → Done, or Failed.
`standalone::save_and_close` writes the resume slot exactly like Quit (PINE slot 0 / Dolphin slot 1 via hotkeys) and
closes the emulator **only if the state was written**; then `game_exited` starts the same game again and the existing
start path loads that slot. The menu shows "Applying…" meanwhile; the Quit escalation (35 s) covers a hang. If
nothing was saved the game is paused again under the menu with "Couldn't save the game, so the new options apply at
its next start." After Save or Load a restart waits for the next start (two state writes never overlap).

### Advanced…
- **PS2:** ROCKNIX's PCSX2.ini binds `[Hotkeys] OpenPauseMenu = SDL-0/Guide & SDL-0/FaceNorth` (HOME+X on the
  Mini V2). pocket-ui resumes the game (window in front, fullscreen) and presses Guide+North through InputPlumber;
  pocket-ui itself ignores that HOME because another button joins it. ARMSX2's big-picture pause menu pauses the VM
  (PINE status 1) and has all of its settings, including per-game ones. pocket-ui watches PINE: back to running → done;
  VM gone (the player chose Close Game there, which leaves ARMSX2 in its game list) → pocket-ui closes ARMSX2 the Quit
  way and the shelf comes back. There is no PINE message to open the menu.
- **GameCube and Wii:** Dolphin nogui has no settings UI, so there is no Advanced row.
- **3DS:** Azahar's menu bar is hidden in fullscreen and no shortcut opens its settings dialog: no Advanced row.

### More… for GameCube and Wii (2026-10-22)
Dolphin nogui has no settings UI to open, so the last row of the GameCube and Wii options, **More…**, opens a second
page (`emu_settings::GC_MORE` / `WII_MORE`, a second `Menu` attached to the first; B goes back to the More… row).
Its rows are ROCKNIX's per-game keys, stored like the PS2 rows as `<platform>["<rom file>"].<key>` in system.cfg; the
start scripts read them with `get_setting` on every start and write them into GCPadNew.ini / WiimoteNew.ini,
Dolphin.ini and GFX.ini. So **every row applies by restart** (save → close → start → load); a changed More row also
takes the first page's live changes into the restart. Read from start_dolphin_gc.sh / start_dolphin_wii.sh of
20261001 (each first choice is what the script does with the key unset):

| Row | key → what the script does | GC | Wii |
|---|---|---|---|
| Button layout: A bottom / A right / Custom | `gamecube_controller_profile`: `east` and `custom` copy those profiles, anything else south | ✓ | – |
| Shoulders: Triggers / Bumpers | `swap_shoulders`: `true` takes `GCPadNew.ini.<layout>.inline` (L/R on L1/R1, Z on R2), else `.stacked` | ✓ | – |
| Controller: Sideways / Upright / Nunchuk / Classic / Custom | `wii_controller_profile` → `WiiControllerProfiles/<x>.ini` → WiimoteNew.ini, else hremote | – | ✓ |
| Rumble: On / Off | `rumble`: `false` deletes the profile's `Rumble` lines | ✓ | ✓ (Wiimote) |
| Shaders: Pause to compile / Skip until ready / Hybrid ubershaders / Ubershaders only | `shader_mode` 0/3/2/1 → `ShaderCompilationMode` (unset: GFX.ini's 0; pocket-ui sets the platform to 3 at start, see below) | ✓ | ✓ |
| Shaders at start: On / Off | `shader_precompile` → `WaitForShadersBeforeStarting` (unset: GFX.ini's True) | ✓ | ✓ |
| VSync: Off / On | `vsync`: `1` → `VSync = True` | ✓ | ✓ |
| CPU clock: 100/125/150/200/75 % | `clock_speed` `100`/`125`/`150`/`200`/`075` → `Overclock`, `OverclockEnable` (300 and 400 left out) | ✓ | ✓ |
| Sync GPU thread: Off / On | `sync_gpu_thread`: `true` → `SyncGPU = True` | ✓ | ✓ |
| EFB reads by CPU: Accurate / Fast | `skip_efb_cpu_access`: `false` → `EFBAccessEnable = False` (ROCKNIX's name reads backwards) | ✓ | ✓ |
| EFB copies, XFB copies: Fast / Accurate | `store_efb_to_texture_only`, `store_xfb_to_texture_only`: `false` → `…ToTextureEnable = False` | ✓ | ✓ |
| Texture cache: Fast / Medium / Accurate | `texture_cache_accuracy` 128 / 512 / 0 → `SafeTextureCacheColorSamples` | ✓ | ✓ |

Left out: `enable_cheats` (ROCKNIX's game INIs list cheats, but nothing here chooses which are on), `auto_swap_discs`
(on unless set; off would only break two-disc games), and anti-aliasing, audio backend, BIOS, save type and region.
"Custom" is offered only when the player's custom profile differs from the stock one the scripts copy there
(`custom_profile_is_own`: GC custom vs. the south profile, Wii custom.ini vs. vremote.ini after pocket-ui's guard).
A ROM name ROCKNIX's lookup can't match gets a page that says so and keeps nothing.

**Default button layout (the owner's choice, 2026-10-22): GameCube A on the right**, like GBA and the printed letters. At
start pocket-ui sets `gamecube.gamecube_controller_profile=east` when system.cfg has no non-empty value for it
(`with_gamecube_layout_default`, like `cap_gpu_if_unset`); a platform choice is never replaced, and per-game keys
win anyway. On the east profile GC A = `Button E`, B = `Button S`, X = `Button N`, Y = `Button W`, so the setting
hotkeys follow (`hotkey_profile` → `east.stacked` or `east.inline` with swapped shoulders; Y/X/B are
InputPlumber North/West/South there).

**Default shaders (the owner's test on the Mini V2, Mario Kart: Double Dash, 2026-10-22):** ROCKNIX's
`ShaderCompilationMode = 0` (synchronous) dropped to 30 fps again and again; with `gamecube.shader_mode=3`
(asynchronous, skip drawing) plus the performance governors below: "much better, not perfect". pocket-ui sets
`gamecube.shader_mode=3` and `wii.shader_mode=3` at start where they have no value (`START_DEFAULTS`, together
with the layout); a set value, per platform or per game, is never replaced.

### Performance (CPU and GPU) for the big systems (2026-10-22)
Measured on the device by the coordinator during the same test: under ROCKNIX's `ondemand` the prime core sat at
0.96 GHz and the big cores at 1.57 GHz while Dolphin needed more, and the GPU at 305–400 MHz under
`simple_ondemand`. So PS2, GameCube, Wii and 3DS get an Options row **Performance: Max / Auto / Battery** (Auto was called
Balanced until 2026-10-23; system.cfg keeps the value `balanced`), kept per game in system.cfg
(`<platform>["<rom>"].pocket_performance`, pocket-ui's own key), and applied **live** (`Live::Power`). Unless chosen,
a game follows Games: performance (Settings → Playing → Performance & fan, Quick settings): Battery there is Battery, Auto and
Max are **Max** (`emu_settings::set_default_performance`, set by the App; Quick settings over the paused game apply a
change at once with `standalone::set_performance`):

- **Max:** the big and prime clusters (every cpufreq policy but the first: policy4, policy7) on `performance`, and
  the GPU's devfreq (`/sys/class/devfreq/*gpu*`, `3d00000.gpu`) on `performance`. The little cluster keeps
  ROCKNIX's governor. The GPU stays under its `max_freq`, the stock 587 MHz that `cap_gpu_if_unset` sets.
  `schedutil` was not chosen: nothing here shows it ramps fast enough (to measure on the device).
- **Auto:** what was there (ROCKNIX's per-game `cpugovernor`, default `ondemand`; GPU `simple_ondemand`). On the test
  Pocket `ps2|gamecube|wii|3ds.cpugovernor=performance` were set by hand, so Auto changed nothing there (Burnout,
  2026-10-23): with those, Auto is full speed on the CPU too.
- **Battery:** big and prime capped at the lowest frequency ≥ half their top (power.rs's Battery), GPU as found.

Code: `power::Power::set_standalone` (reuses power.rs's notes and restore file, `/tmp/pocket-ui-standalone.restore`),
`standalone::set_performance`. **When:** runemu.sh writes `performance` at its start and then ROCKNIX's
`cpugovernor` just before the start script, which would replace ours; so the profile is applied in the resume
thread once the emulator runs (PINE running / Dolphin window / Azahar title), before the resume state loads.
**Restore:** when the game ends (`standalone::exited`) only what is still as we wrote it goes back: runemu.sh has
by then put `system.cpugovernor` on every cluster and the GPU on `auto`, and those stay. If pocket-ui crashed (and
runemu.sh died with the game), the restore file is undone at the next start, once no ARMSX2, Dolphin or Azahar
process is left (`standalone::recover_performance`). pocket-core games keep their own Extras profile, unchanged.
The resume thread takes the CPU's lease (`power::Lease`, [HOST.md](HOST.md#cpu-and-fan-on-rocknix-sm8250)) before it
notes the originals, so a port or game before it has put its settings back first; `standalone::exited` lets go of it
after the restore. The note is written whole before the first sysfs write, and nothing is applied without it.

Not measured here (the vdev has no cpufreq, no GPU and no battery): temperatures and draw. See the device checks.

**Wii does not follow it:** start_dolphin_wii.sh reads neither `gamecube_controller_profile` nor `swap_shoulders` and
always copies `GCPadNew.ini.south.stacked` ("needed for hotkeys"). Wii games that take a GameCube controller keep A
at the bottom; the Wii Remote's layout is the Controller row.

Not modelled: both start scripts swap `Button S`↔`Button E` and `Button W`↔`Button N` in the copied GCPadNew.ini
and WiimoteNew.ini when the SDL mapping of the pad has `a:` on a higher button index than `b:` (`FACE_SWAP`).
`profile_button` reads the profile files before that swap; the crossing of West/North it applies was measured on the
device with the south profile. If FACE_SWAP is active on the Mini V2, the east profile would put A at the bottom
and the X hotkey on the wrong button: the device check below tells.

### Options for Wii: the Wiimote is kept out of the hotkeys (2026-10-22)
Wii uses the GameCube table with the same live paths: HOTKEY+Y (Show FPS), +X (Auto/Stretch), +B (resolution) are
read by ROCKNIX's hotkey patch from the GameCube pad of port 1, which start_dolphin_wii.sh sets up from
`GCPadNew.ini.south.stacked` on every nogui start. On that profile GC Y/X/B are `Button N`/`Button E`/`Button W`,
which InputPlumber's West/East/North arrive as (`profile_button`).

The problem: the stock Wiimote profiles bind those face buttons too (hremote: A = `Button N`, B = `Button W`,
1 = `Button E`; classic, nunchuck and vremote similar), so HOTKEY+Y would have pressed Wiimote A in the game.
The fix extends `with_quiet_wiimote` to every input a chord presses besides the stick clicks: `Back`, `Guide`,
`Pad N`, `Pad S` **and `Button N`, `Button E`, `Button W`**. Each such binding becomes
`` (…) & !`Thumb L` & !`Thumb R` ``, and every chord holds L3 from its first press to its last release (setting
chords `[L3, R3, HOTKEY, face]`, slot steps `[L3, R3, HOTKEY, Up/Down]`, save/load `[R3|L3, HOTKEY]`), so the Wiimote
sees nothing of any chord. The GC pad's own face buttons are not guarded (the hotkey patch reads them), so a Wii game
that takes a GameCube controller sees Y/X/B for ~80 ms, like GameCube games.

`standalone::tests::no_chord_of_ours_presses_a_wiimote_button` proves it on the real stock profiles (fixtures copied
from ROCKNIX 20261001, `src/fixtures/dolphin-wii-*.ini`, md5-checked against the image): it evaluates every binding
of each rewritten profile, with Dolphin's `!`/`&`/`|` expression rules, at every step of every chord pocket-ui sends
on Wii (save/load on slots 0 and 1, the three setting hotkeys, Select and Guide as HOTKEY), and checks that each
binding still answers to its own button when no stick click is held. It failed before the fix (classic
`Classic/Buttons/Y = Button N` pressed by the Show FPS chord).

Not guarded: the player's own `custom.ini` Wiimote profile. With `wii_controller_profile=custom` the Wii page presses
no hotkey at all; every change applies by restart (`hotkey_profile` → None).

### Device test (to run; the device was not used for this)
PS2, Burnout 3 or SSX Tricky:
1. HOME → Options shows 2x · Vulkan · Off · 4:3 · Off · Advanced….
2. Show FPS On → B → Continue: the counter appears at once; `pine.py get EmuCore/GS OsdShowFPS` = true;
   system.cfg has `ps2["<rom>"].show_fps=true`.
3. Resolution 3x → Continue: sharper at once; `pine.py get EmuCore/GS upscale_multiplier` = 3, `pine.py watch 10` full speed.
4. Renderer OpenGL → Continue: short black (GS reopen), then slow; back to Vulkan.
5. Widescreen On + Aspect 16:9 (SSX Tricky has a widescreen patch in patches.zip, check) → Continue.
6. Quit, start again: the values are back (PCSX2.ini after the launch); another PS2 game starts with 2x and no FPS.
7. Advanced… → ARMSX2's menu opens over the game; B closes it and the game goes on; HOME still opens pocket-ui's menu.
8. Advanced… → Settings → this game's properties → set a resolution → close; pocket-ui Options shows it; change
   Resolution there → Continue: "Applying…", restart, back at the same moment within ~10 s; the key is gone from
   `gamesettings/<serial>_<CRC>.ini`.
9. Advanced… → Close Game in ARMSX2's menu: pocket-ui is back on the shelf within ~2 s, no ARMSX2 game list left.

GameCube, e.g. Mario Kart: Double Dash:
1. Options shows 1x · Vulkan · Off · Auto · Off · Off.
2. Show FPS On → Continue: the counter appears (HOTKEY+Y); watch whether the game reacted to the Y press.
3. Resolution 2x → Continue: Dolphin shows "Internal Resolution: 480P" then "720P" (two presses).
4. Aspect Stretch → Continue (stretched at once), then Auto.
5. Aspect 16:9 → Continue: "Applying…", restart, resumes at the same moment (Dolphin's "Loaded state" OSD).
6. Renderer OpenGL → restart; `GameSettings/GM4E01.ini` has `[Core] GFXBackend = OGL` and still all its cheats.
7. Frame skip When slow → restart.
8. With `gamecube.gamecube_controller_profile=east`: Show FPS still toggles (HOTKEY+West is GC Y there).
9. Quit and start again: the values are kept; another GameCube game is unchanged.

Wii, a disc game with the default Wiimote profile (hremote), then Blockamok Remix (DOL):
1. After the start: `grep -c 'Thumb L' /storage/.config/dolphin-emu/WiimoteNew.ini` = 7 guarded bindings
   (hremote: A, B, 1, -, Home, D-pad Left and Right, the ones on `Pad S`/`Pad N`); `grep 'Button N' WiimoteNew.ini`
   shows `` Buttons/A = (`Button N`) & !`Thumb L` & !`Thumb R` ``.
2. Options shows 1x · Vulkan · Off · Auto · Off · Off. Show FPS On → Continue: the counter appears, and the game
   shows **no** reaction to Wiimote A (no menu opens, nothing is selected). Resolution 2x → Continue: "480P", "720P".
3. Aspect Stretch → Continue, then Auto. Renderer OpenGL → "Applying…", restart, back at the same moment;
   `GameSettings/<ID>.ini` has `[Core] GFXBackend = OGL`.
4. Blockamok (DOL): Options keeps values; after Renderer OpenGL the file is `GameSettings/ID-BlockamokRemix.ini` and
   Dolphin's log/OSD shows the OpenGL backend after the restart. If it starts with Vulkan, Dolphin reads another
   name for executables: report it, the DOL row then needs a different ID.
5. `wii.wii_controller_profile=custom`: Show FPS On → "Applying…" (restart), no hotkey pressed.

Shaders and Performance (any big game; Mario Kart: Double Dash for numbers):
1. After deploying: `grep -e shader_mode system.cfg` shows `gamecube.shader_mode=3` (already set by hand on the test
   device: unchanged) and `wii.shader_mode=3`; `GFX.ini` has `ShaderCompilationMode = 3` after a GameCube start.
2. While playing: `cat /sys/devices/system/cpu/cpufreq/policy{0,4,7}/scaling_governor
   /sys/class/devfreq/3d00000.gpu/governor` = ROCKNIX's (ondemand), `performance`, `performance`, `performance`;
   `/tmp/pocket-ui-standalone.restore` exists; `grep 'standalone: performance' /var/log/pocket-ui.log` = "max: set".
3. Options → Performance Auto → Continue: policy4/7 and the GPU are back to ondemand/simple_ondemand at once.
   Battery: policy4/7 `scaling_max_freq` about half. Max again.
4. Quit: every governor is ROCKNIX's again (`system.cpugovernor`, GPU `simple_ondemand`), the restore file is gone.
5. `kill -9 $(pidof pocket-ui)` during a game, then Quit the game by HOME+START (ROCKNIX): after pocket-ui is back,
   the governors are ROCKNIX's within ~5 s of the emulator ending.
6. Numbers to take, Max against Auto, 5 min of the same race each, on battery: `deploy/bench.sh 300
   dolphin-emu-nogui` (fps, CPU/GPU °C, watts on battery) and Settings → Playing → Battery's per-system draw for gamecube; also
   whether `schedutil` on policy4/7 (written by hand) keeps up as well as `performance`.

GameCube and Wii, More… and the button layout:
1. After deploying, `grep gamecube_controller_profile /storage/.config/system/configs/system.cfg` shows
   `gamecube.gamecube_controller_profile=east` (only if it was unset before; a set value is unchanged).
2. Start Mario Kart: Double Dash. `grep -e 'Buttons/[ABXY] =' /storage/.config/dolphin-emu/GCPadNew.ini` shows
   `A = Button E`, `B = Button S`, `X = Button N`, `Y = Button W`. If it shows `A = Button S`, ROCKNIX's FACE_SWAP
   is active: report it (A is then at the bottom, and the hotkey mapping needs that swap). In the game: the right
   button accepts in the menus, the bottom one goes back.
3. Options: Show FPS On → the counter appears (HOTKEY + the top button, GC Y on east); Aspect Stretch → stretched
   (HOTKEY + left = GC X); Resolution 2x → "480P", "720P" (HOTKEY + bottom = GC B). None of them presses a game
   button longer than the chord.
4. Options → More…: Button layout A right · Shoulders Triggers · Rumble On · Shaders Pause to compile · Shaders at
   start On · VSync Off · CPU clock 100% · … B goes back to the More… row.
5. Shoulders Bumpers → B, Continue: "Applying…", restart at the same moment; system.cfg has
   `gamecube["<rom>"].swap_shoulders=true`, `GCPadNew.ini` has `Triggers/L = Shoulder L | (…)` and Z on R2.
   Save and Load from the menu still work (they use the stick-click L/R bindings, not the shoulders).
6. Button layout A bottom → restart → A on the bottom button; the setting hotkeys still work (now on the south
   layout's buttons). Back to A right.
7. CPU clock 150% → restart; `grep -e '^Overclock' Dolphin.ini` = 1.5 and `OverclockEnable = True`.
8. Shaders Ubershaders → restart; `GFX.ini` `ShaderCompilationMode = 2`; watch for stutter and for slowdown.
9. Wii disc game: More… shows Controller first and no Button layout; Controller Classic → restart,
   `WiimoteNew.ini` has `Extension = Classic`; Show FPS still toggles without a Wiimote press.

3DS, Blockamok Remix (3DSX):
1. Options shows 1x · Stacked · Vulkan · Off · Off; there is no Advanced row.
2. Resolution 2x → Continue: "Applying…", Azahar closes and restarts, the resume state loads within ~15 s;
   `grep resolution_factor /storage/.config/azahar/qt-config.ini` = 2; system.cfg has
   `3ds["BlockamokRemix.3dsx"].resolution_scale=2`.
3. Screens Top only → restart: only the top screen, scaled up. Large top → restart.
4. Texture filter xBRZ → restart: `texture_filter=4`, `texture_filter\default=false` in qt-config.ini.
5. Show FPS On → restart: a small FPS counter top right (MangoHud), not the full HUD;
   `tr '\0' '\n' < /proc/$(pidof azahar)/environ | grep MANGOHUD` shows our config file, and
   `grep -c -i mangohud /proc/$(pidof azahar)/maps` is more than 0 (the layer is loaded). Show FPS Off → restart:
   no counter.
6. Quit, start another 3DS game: 1x, Stacked, no filter, no counter.

## Measuring
- PS2: `python3 /storage/pocket-ui/pine.py watch 25` (fps, frame time, thread load per second) next to
  `deploy/bench.sh 25 armsx2-qt`; `pine.py save 9` / `load 9` give a repeatable scene, `pine.py set SECTION KEY
  VALUE` switches settings live.
- GameCube: add `[Video_Settings] LogRenderTimeToFile = True` to the game INI, create
  `/storage/.config/dolphin-emu/Logs`, then `deploy/dolphin-frametimes.py 20 40 render_times.txt`; CPU and
  temperatures with `deploy/bench.sh 20 dolphin-emu-nogui`.
- Driving a game from SSH: InputPlumber `SendButtonChord` (`busctl call org.shadowblip.InputPlumber
  /org/shadowblip/InputPlumber/CompositeDevice0 org.shadowblip.Input.CompositeDevice SendButtonChord as 1
  Gamepad:Button:South`). Stop pocket-ui first or launch through it: an idle pocket-ui reads the same pad and
  will start games of its own.

## Open
- Resume shows the game booting for a moment (PS2 until the VM runs, GameCube ~3 s) before the state loads, plus
  Dolphin's OSD lines.
- Dolphin's slot counter lives inside Dolphin: if the player steps slots with HOTKEY+Up/Down during play, the menu's
  Save/Load and the resume slot are off by that many. A clean fix needs a Dolphin patch (load/save by slot number,
  or an IPC); same for loading at boot with `-s`.
- Vsync off means possible tearing in both. Vsync would need the game shown while saving on Quit.
- Dolphin Qt builds keep Continue/Quit (no pad hotkeys there).
- Wii and 3DS: everything in "Wii and 3DS" below is still to be confirmed on the device.
- InputPlumber 0.79 `SendEvent` panics a worker thread ("Cannot block the current thread from within a runtime");
  only `SendButtonChord` is used.
- Power draw on battery still to measure; heavier GameCube scenes still to measure.
- `/storage/armsx2-nightly` (230 MB, the extracted nightly and its own config) can be deleted.
- Options on the device: none of it has run on the Mini V2 yet (see the device test above). Like the slot counter,
  Dolphin's hotkey toggles keep their state inside Dolphin: if the player presses HOTKEY+Y/X/B during play, the
  live Show FPS / Aspect / Resolution changes land one step off until the next start. The game also sees the GC
  button of each hotkey press (Wii games only when they take a GameCube controller).
- Options of a game whose disc ID can't be read (unknown image format) can't be kept; the page says so. DOL/ELF use
  `ID-<stem>` (to confirm on the device).
- 3DS options all restart the game (~10–15 s); MangoHud as the 3DS FPS counter is untested on this device.

## Wii and 3DS (2026-10-22, device check pending)

### How ROCKNIX runs them
- `wii` (and `wiiware`): emulator `dolphin`, cores `dolphin-sa-wii` (default, nogui) or `dolphin-qt-wii`;
  runemu.sh starts `start_dolphin_wii.sh <rom> wii <core>`. Same binary and the same hotkey patch as
  GameCube. pocket-ui handles `wii` with the nogui core; `wiiware` (in practice `.wad`) and the Qt core
  stay without.
- `3ds`: emulator `azahar`, core `azahar-sa`; runemu.sh starts `start_azahar.sh <rom> 3ds`, which runs
  gptokeyb (pad → keyboard: HOME+R1 → Alt+S, HOME+L1 → Alt+L, HOME+L2/R2 → F10/F9 layout/swap) and
  `/usr/bin/azahar <rom>` (Qt, `singleWindowMode=true`, `fullscreen=true`). ROCKNIX's "HOME+START quit" is
  gptokeyb's kill: `set_kill set "-9 azahar"`, so SIGKILL.

### Wii: which pad drives what
- On every nogui start, start_dolphin_wii.sh copies `GamecubeControllerProfiles/GCPadNew.ini.south.stacked`
  to `GCPadNew.ini` ("needed for hotkeys"). The hotkey patch reads `Pad::GetStatus(0)`, the GameCube pad of
  port 1, in Wii games as well (`SIDevice0 = 6`, a standard controller, so Wii games that take GameCube
  pads see it too). So HOTKEY + stick click as GC L/R (`with_extra_triggers`) works for Wii unchanged.
- The emulated Wiimote is a second, independent mapping of the same pad: on every start
  `WiiControllerProfiles/<profile>.ini` → `WiimoteNew.ini` (`wii_controller_profile`: hremote sideways is
  the default, vremote, nunchuck, classic, custom). In all four stock profiles Select is the Wiimote's `-`,
  Guide is HOME and the D-pad is the D-pad; neither stick click is bound. The hotkey profile does **not**
  keep these from the Wiimote: ROCKNIX's own HOTKEY+R presses `-` in the game as well.
- So pocket-ui's chords always hold a stick click first: save `[R3, HOTKEY]`, load `[L3, HOTKEY]`, slot step
  `[L3, R3, HOTKEY, Up/Down]` (InputPlumber releases in reverse order). On Wii launches, the four stock Wii
  profiles get `` (…) & !`Thumb L` & !`Thumb R` `` on every binding that uses `Back`, `Guide`, `Pad N`,
  `Pad S` or (since the Options, the setting hotkeys' face buttons) `Button N`, `Button E`, `Button W`
  (`with_quiet_wiimote`), so the Wiimote sees nothing of a chord. The GC L/R extras exclude the
  other stick click, so a slot step is neither L nor R. `custom.ini` (the player's own) is left alone; with
  it, Save/Load/Quit chords may reach the Wiimote, and the Options press no hotkey (restart instead).
- Not changed by pocket-ui, and limits to know about: pointer games (IR is on the right stick in the stock
  profiles, usable but not precise), motion (shake on L1, the nunchuk's on L2, no tilt or swing), Wii
  MotionPlus, Balance Board, a second Wiimote. Wii games that take a GameCube pad see D-pad Up/Down for
  80 ms during a slot step, as GameCube games always did.

### 3DS: Azahar's channel
- No control channel for states: Azahar's scripting RPC (`ENABLE_SCRIPTING`, on by default, UDP port 45987)
  only reads and writes memory, lists processes, and takes screenshots and perf stats; it has no save, load or
  quit. The command line only takes the ROM. That leaves its Qt shortcuts, which need keyboard focus, and sway refuses to focus a window
  hidden behind a fullscreen one. So for every save and load, Azahar is put in front
  (`[app_id=pocket-ui] fullscreen disable; [pid=N] focus; [pid=N] fullscreen enable`, then 300 ms for Qt
  to take the keyboard), and pocket-ui again after if it was in front. Menu Save and Load happen with the
  game shown anyway; on Quit and before sleep the game is visible while it saves. This also keeps Azahar's
  vsync'd Vulkan presentation (`use_vsync=true`, `async_presentation=true`) from waiting on frame callbacks
  that a hidden window never gets (the ARMSX2 lesson).
- **One slot only.** Quick Save and Quick Load use slot 0. Slots 1–10 have menu actions but no shortcuts,
  and "Save to Oldest / Load from Newest Slot" depend on the program id, which `.3dsx` games don't have.
  So:
  - Save and Load in the pause menu use Azahar's quick slot `<TITLEID>.00.cst` (the same as HOME+R1/L1),
    with the preview in `<state>.png`.
  - Quit and sleep: the quick slot is renamed to `….00.cst.pocket-ui`, Quick Save writes a fresh
    `.00.cst`, that file is renamed to `/storage/roms/savestates/3ds/<rom file>.auto.cst`, and the quick
    slot is renamed back.
  - Resume: the quick slot is parked the same way and `<rom>.auto.cst` is hard-linked as `.00.cst`, then
    Quick Load. As soon as Azahar has held the file open for 25 ms, the link is removed and the quick slot
    put back (watched with inotify; the slot listing before the load only peeks at the 256-byte header).
    Azahar keeps reading through its open descriptor.
  - If pocket-ui dies in between, `….pocket-ui` is left behind; the next launch of that game puts it back
    (`azahar::recover`).
- State names: `<program id, 16 hex digits>.NN.cst` (Azahar's `GetSaveStatePath`). pocket-ui reads the id
  from `.3ds`/`.cci` (NCSD partition 0) and `.cxi`/`.app` (NCCH 0x118). `.3dsx`, `.z3dsx`, `.elf` and
  `.axf` have none and Azahar uses 0, so all 3DSX homebrew share one quick slot (the resume state is per
  ROM file). Azahar 2126.2 can't boot `.cia` (it has no CIA loader), and `.zcci`/`.zcxi`/`.zcia` would need
  decompressing: these keep Continue/Quit.
- Quit: the resume state, then sway `[pid=N] kill` (a Qt close: Azahar stops the emulation and writes
  qt-config.ini), 8 s, SIGTERM (Azahar has no handler, so it ends at once), 8 s, SIGKILL. Enforced in
  qt-config.ini on every start: `confirmClose=false` and `saveStateWarning=false`, each with
  `\default=false` (Azahar ignores a value whose `\default` is true). When `/storage/.config/azahar` is
  missing, pocket-ui first creates it from `/usr/config/azahar`, as the start script would.
- Two screens: the Mini V2 has one panel (`DEVICE_HAS_DUAL_SCREEN` unset), so start_azahar.sh picks layout
  0, the top screen above the bottom one in one window. grim captures the whole output, so the pause
  screenshot and the previews show both screens; the bottom one is the touch screen. Nothing in pocket-ui
  depends on the layout.

### Virtual device
The virtual device can't run either emulator. It is aarch64 like the device, so the architecture is not
the problem. But `docker/extract-rocknix.sh` strips `/usr/bin/azahar`, there is no InputPlumber (so no
chords), and `dolphin-emu-nogui` dies with SIGBUS at start in the container, also with `-p headless -v Null`
and the cached interpreter without fastmem. What runs there: the unit tests, plus `scripts/test-linux.sh
standalone` for the inotify part (21 tests pass on Linux, 20 on macOS).

### Test software
Blockamok Remix v1.21 by Mode8fx (a fork of Blockamok by Carl Riis), MIT licence,
<https://github.com/Mode8fx/blockamok/releases/tag/v1.21> (music and sound credits are listed in the game):
- Wii: `BlockamokRemix-v1.2-wii.zip` (sha256 `453d4a43…ca0afb`) → `apps/BlockamokRemix/boot.dol`
  (`f8b51108…69f0b`), copied as `/storage/roms/wii/BlockamokRemix.dol`. States: `ID-BlockamokRemix.sNN`.
- 3DS: `BlockamokRemix-v1.2-3dsx.zip` (`fe972c69…58a00`) → `3ds/BlockamokRemix/BlockamokRemix.3dsx`
  (`b24a581a…c13faf`), copied to `/storage/roms/3ds/BlockamokRemix.3dsx`. States:
  `0000000000000000.00.cst` and `BlockamokRemix.3dsx.auto.cst`.

### Pending device checks (exact commands)
Run these from `pocket-ui/`, one short locked block per system. The device is muted and unmuted again, and
pocket-ui is left running. `$HB` is a folder that holds the two files above.

```sh
# 0. copy the homebrew, deploy this build
scripts/devlock.sh wii3ds sh -c 'H=root@<device-ip>
  scp -q "$HB/boot.dol" $H:/storage/roms/wii/BlockamokRemix.dol &&
  scp -q "$HB/BlockamokRemix.3dsx" $H:/storage/roms/3ds/BlockamokRemix.3dsx &&
  scripts/dev.sh deploy'

# 1. Wii (the same for 3DS, with the paths in brackets)
scripts/devlock.sh wii3ds sh -c 'H=root@<device-ip>
  M=$(ssh $H "wpctl get-volume @DEFAULT_AUDIO_SINK@" | grep -q MUTED && echo 1 || echo 0)
  ssh $H "wpctl set-mute @DEFAULT_AUDIO_SINK@ 1"; trap "ssh $H wpctl set-mute @DEFAULT_AUDIO_SINK@ $M" EXIT
  scripts/dev.sh shot home     # find the Wii [3DS] system and game, then send the moves + a to start it
  # scripts/dev.sh send <moves> a ; sleep 25
  ssh $H "grep -n Thumb /storage/.config/dolphin-emu/WiimoteNew.ini /storage/.config/dolphin-emu/GCPadNew.ini"
  # [3DS: ssh $H "grep -n -e confirmClose -e saveStateWarning /storage/.config/azahar/qt-config.ini"]
  scripts/dev.sh send home; sleep 1; scripts/dev.sh shot menu          # grim frame under the menu
  scripts/dev.sh send down a; sleep 6                                    # Save
  ssh $H "ls -l /storage/roms/savestates/wii/"   # ID-BlockamokRemix.s00 + .png [3ds: 0000000000000000.00.cst + .png]
  scripts/dev.sh send home down down a; sleep 4; scripts/dev.sh shot loaded   # Load
  scripts/dev.sh send home down down down a; sleep 15                    # Quit
  ssh $H "ls -l /storage/roms/savestates/wii/; pidof dolphin-emu-nogui; grep standalone /var/log/pocket-ui.log | tail"
  #   expect ID-BlockamokRemix.s01 (+.png) and no process [3ds: BlockamokRemix.3dsx.auto.cst, .00.cst unchanged, no *.pocket-ui]
  scripts/dev.sh send a; sleep 30; scripts/dev.sh shot resumed            # the last game is first on Home: resume
  ssh $H "grep standalone /var/log/pocket-ui.log | tail -4"              # load slot 1: sent [azahar opened …: Some(true)]
  scripts/dev.sh send home down down down a; sleep 15'                   # quit again; pocket-ui stays up
```
Besides the files, check that:
- the Wii game does not react to Save or Load (no `-` or HOME menu);
- on 3DS the game shows for a moment on Quit, and pocket-ui comes back;
- Azahar does not ask "close?" and does not show the save-state warning;
- the resumed game continues where it was quit, not at the quick save.
- GameCube still saves, loads and resumes with the new chord order (Mario Kart: Double Dash, as before).

## Sync of their saves (Settings > Sync, 2026-10-22, device check pending)
The pocket-core/RetroArch saves folder (`/storage/roms` with an ignore file of save extensions) never covered these:
`.ps2`, `.raw`, `.gci` and Azahar's save files are not in its list. Three rows of their own, on by default once Sync
is on (`src/sync.rs`, `Share` and `Dir`). Where things are was read from the 20261001 image (PCSX2.ini,
start_dolphin_gc.sh / start_dolphin_wii.sh, Dolphin.ini, start_azahar.sh):

| Row | Syncthing folder | `.stignore` (first match wins) |
|---|---|---|
| PS2 memory cards | `pocket-ps2-cards` = `/storage/roms/ps2` (`[Folders] MemoryCards`) | `!/Mcd*.ps2`, `*` |
| GameCube & Wii saves | `pocket-gc-cards` = `/storage/roms/bios/GC` (the scripts link `GC/MemoryCard?.<region>.raw` and the per-region GCI folders there) | `!/MemoryCard*.raw`, `!/*/*.gci`, `*` (IPL, DSP ROMs and fonts stay out) |
| | `pocket-wii-saves` = `/storage/.config/dolphin-emu/Wii/title` (`NANDRootPath` empty) | `!/00010000/*/data`, `!/00010001/*/data`, `!/00010004/*/data`, `*` |
| 3DS saves | `pocket-3ds-saves` = `/storage/roms/3ds/azahar/sdmc/Nintendo 3DS` (start_azahar.sh links sdmc there) | `!/*/*/title/00040000/*/data`, `!/*/*/extdata/00000000`, `*` (installed titles, updates, DLC are `content`) |

The saves folder now starts with `/3ds/azahar` (so a 3DS save named `*.sav` is not in two folders) and the BIOS folder
gets `/GC/MemoryCard*.raw` and `/GC/*/*.gci` (the cards belong to their row). The PS2, GameCube and 3DS folders lie
inside the saves or BIOS folder; their contents are kept out of the outer one, so no file is in two folders.
The unit tests check each ignore file against sample paths, ROMs of every kind among them.

- **Never while the emulator runs:** at launch (`App::launch` → `sync::hold_for_game`) the folders of the game's
  system are paused through the REST API (`PATCH /rest/config/folders/<id>` `{"paused":true}`), and resumed in the
  existing scan after the game (`run_after_game` → `apply_holds`), which then scans them whole. A memory card is
  rewritten in place while the game runs; paused, Syncthing neither sends a half-written card nor replaces a card
  the emulator has open. The ids pocket-ui paused are kept in `/storage/.config/pocket-ui/sync/held`: a folder the
  user paused by hand stays paused, and one left paused when pocket-ui stopped mid-game is resumed after its next
  start, once no ARMSX2, Dolphin or Azahar process is left. An options restart keeps the folders paused (no scan
  in between). Since review N1 (2026-10-22) **every** game holds `pocket-saves` and `pocket-states` the same way:
  pocket-core and RetroArch keep the battery save in memory and rewrite it every few seconds, which silently
  overwrote a newer `.srm` Syncthing had just brought in (Syncthing took the rewrite for a later local change and
  sent it on, no conflict). Paused, the incoming version waits; on resume both sides have changed and Syncthing
  keeps the older one as a `.sync-conflict-…` copy. A fast switch from game to game keeps them paused throughout
  (`apply_holds` takes the latest wish).
- **Folders that don't exist yet are not made by us:** start_dolphin_gc.sh copies Dolphin's DSP ROMs and fonts into
  `/storage/roms/bios/GC` only when that folder is missing, and start_dolphin_wii.sh / start_azahar.sh copy their whole
  config when theirs is missing; creating them first (Syncthing creates a missing folder root when it is added) would
  break those starts. Such folders are added after the first game of that system ends.
- **Conflicts:** memory cards are whole files, so Syncthing's rule applies to the whole card: the newer one wins, the
  other is kept as `Mcd001.sync-conflict-<date>-<time>-<device>.ps2`. GCI files, Wii and 3DS saves are per game.

Device check (Syncthing on, no second device needed):
1. Settings > Connections > Sync shows the rows "PS2 memory cards", "GameCube & Wii saves", "3DS saves" as Shared;
   `curl -sk -H "X-API-Key: $(xmlstarlet sel -t -v //gui/apikey /storage/.config/syncthing/config.xml)"
   https://127.0.0.1:8384/rest/config/folders | jq -r '.[] | "\(.id) \(.path) \(.paused)"'` lists
   `pocket-ps2-cards` (and the Dolphin/Azahar ones once those emulators have run), all `false`.
2. `cat /storage/roms/ps2/.stignore`; `ls /storage/roms/bios/GC/` still has `dsp_rom.bin` etc. after a GameCube start.
3. Start a PS2 game; within a few seconds the folder list shows `pocket-ps2-cards true` and
   `/storage/.config/pocket-ui/sync/held` names it. Save in the game, Quit: `false` again, `held` gone, and
   `/rest/db/status?folder=pocket-ps2-cards` shows the new `Mcd001.ps2` (`localFiles`, `inSyncFiles`).
4. Pause `pocket-ps2-cards` by hand in the web interface, play and quit a PS2 game: it stays paused.
5. Wii and 3DS: after the first game, `pocket-wii-saves` / `pocket-3ds-saves` appear;
   `/rest/db/browse?folder=pocket-3ds-saves&levels=8` lists only `…/data/…` and `extdata/00000000/…`, no `.app`.
6. Stop pocket-ui while a PS2 game runs (`kill $(pidof pocket-ui)`; its launcher starts it again): the folder stays
   `true` while ARMSX2 still runs, and turns `false` (with `held` gone) within ~5 s of ARMSX2 ending.

## Wii U (Cemu) and PS3 (RPCS3) on SM8550 (2026-10-28, experimental)
ROCKNIX's SM8550 image ships `cemu` (`start_cemu.sh`, core `cemu-sa`) and `rpcs3-sa` (`start_rpcs3.sh`). They are guests
of the pause menu without states (`standalone::guest`, `Saver::Other` in the menu): **Continue · Options · Quick settings ·
Quit**, with the honest line "This emulator keeps its own saves…" (`pause_saves::OWN_SAVES`).
- **Start:** as for every emulator, through `runemu.sh` (the shelf's launch path is unchanged). `standalone::launch`
  notes the guest and, once its process (`cemu`, `rpcs3-sa`, found by `comm`) exists, sets the CPU/GPU profile of the
  other big emulators (`set_performance(chosen_performance)`: the game's Options row, else Settings → Games: performance;
  Quick settings' Performance row acts on it, `Kind::Standalone` in `app_quick_profiles`). Fan: Smooth fan follows the
  same saved choices as for the other games. The profile is put back when the game ends (`exited`).
- **Pad:** input belongs to the game while it runs (`in_game_input`); only a HOME tap opens the menu. Nothing else of
  pocket-ui reacts to the pad.
- **Quit:** `guest::PLAN`: close the emulator's window (`swaymsg [pid=…] kill`, which is a normal close request: Cemu's
  Exit, RPCS3 with `confirmationBoxExitGame=false` as ROCKNIX ships it) → wait 8 s → SIGTERM → wait 5 s → SIGKILL, always
  on the process inside the game's process group; the main loop's `Escalation` (`How::Guest`, 20 s) stops the whole group
  if even that hangs. runemu.sh is not signalled, so it restores the governor and its other clean-up itself.
- **Save/Load: not offered.** Cemu has no save states. RPCS3 has no save states either, and no IPC or hotkey for them
  (ROCKNIX's `start_rpcs3.sh` passes no extra arguments; there is only `--no-gui <game>`). Saving is inside the game;
  the honest line says so. If a control channel appears upstream, extend `guest::Guest` with an `Emu`-style state set.
- Runs on SM8550 only because the emulators exist only there (`Guest::detect` doesn't check the device; without the
  binary `runemu.sh` fails to start the game, as before).
- Device check: see the report of this change (docs/DEVICE-TESTS.md style): start from the shelf, HOME → menu,
  Continue, HOME → Quit; `pidof cemu rpcs3-sa` empty afterwards, `verify-device.sh` ok.

## PS Vita (Vita3K) on SM8550 (2026-10-29, experimental)
ROCKNIX's SM8550 image ships Vita3K (`vita3k-sa`, `start_vita3k.sh`; es_systems system `psvita`, emulator `vita3k`, core
`vita3k-sa`, extensions `.pkg .psvita .zip`). It is the third guest of `standalone::guest` (`Guest::Vita3k`, process `vita3k-sa`),
with the same pause menu (**Continue · Options · Quick settings · Quit**, `pause_saves::OWN_SAVES`), the same performance
profile once the process exists, and the same Quit (`guest::PLAN`: window close → 8 s → SIGTERM → 5 s → SIGKILL, then every
`vita3k-sa` of the game's group). Vita3K's window close is an SDL quit event, so it ends like its own Exit.
- **Games on the shelf:** `roms/psvita/*.psvita` are one-line files with a title ID; `scan_vita3k.sh` (run by the Vita3K tool
  and after an install) writes them with the game's name from `vita-gamelist.txt`, so the shelf shows real titles. A `.pkg` (needs
  a `.txt` with the ZRIF next to it) or a `.zip` is installed by `start_vita3k.sh` on its first start and renamed afterwards
  (`.installed`, not listed). `roms/psvita/vita3k/` is Vita3K's own data folder; pocket-ui reads one folder level only, and
  that folder holds no game files at its top, so it never shows as a game.
- **Covers:** libretro thumbnails "Sony - PlayStation Vita" (`covers::playlist`), matched by title as for the other systems.
- **Art:** `assets/systems/psvita.png` (PixelLab conversion of Evan Amos's public-domain photo, `assets/systems/SOURCES.md`) and
  the small case with the blue top band (`Medium::Vita`, `assets/media/vita-*.png`, drawn by `assets/media/tools/draw_media.py`;
  the cabinet files it with the card boxes: `FormFactor::CardBox`, game box `Hull::Card`).
- **Firmware and games are the player's own.** Vita3K needs the Vita firmware (PUP files in `bios/vita3k`, installed by the
  `Start Vita3K.sh` tool); pocket-ui ships and fetches neither. Without it ROCKNIX's script says so with a notice and tries anyway.
- **Save/Load: not offered** (Vita3K has no save states; saves live in the game, in `roms/psvita/vita3k/ux0/user`).
- Runs on SM8550 only because the emulator exists only there (`Guest::detect` doesn't check the device).
- **First use:** `start_vita3k.sh` stops with "Vita3K Setup Required" until the tool **Start Vita3K** has been run once (it creates
  `roms/psvita/vita3k` and Vita3K's config) and, with firmware files in `roms/bios/vita3k`, installs them. A `.psvita` file whose title
  is not installed makes Vita3K quit at once ("--installed-path: ID not in {}"); the game then ends after a second and the shelf is back.
- **Device check (Odin 2 Mini, 2026-10-29, build of `feat/vita3k-guest`):** with no firmware and no game on the device, a `.psvita` pointing at
  a folder in `ux0/app` starts Vita3K (its first-run language page) through the shelf and `runemu.sh`: process `vita3k-sa` found, profile set,
  HOME → pocket-ui's menu over it (Continue · Options · Quick settings · Quit; Options shows "Runs on: Standalone · Vita3K"), Continue
  brings the window back, Quit three times: Vita3K gone 0.1-0.2 s after the press, no `vita3k-sa`, `start_vita3k.sh` or `runemu.sh` left,
  `verify-device.sh` ok. Not tried: a running game (no firmware; an open-source `.vpk`, mGBA for Vita, did not start without it: Vita3K
  shows its first-run page instead), the `.pkg`/`.zip` install path of `start_vita3k.sh`, Vita3K's `foot` window during an install.
