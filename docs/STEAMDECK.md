# Steam Deck (0.3, internal QA)

> **Status:** the Deck profile follows main, including the 0.3 look and library changes. Bundle and Flatpak
> are built for internal QA; this version has **not run on a Deck**. The earlier SSH Gamescope session on a real
> Deck is recorded at the end. Headless checks cannot establish Steam Input, real suspend, GPU or audio behaviour.

pocket-ui on SteamOS is an ordinary Linux program that Steam starts as a "Non-Steam game". Gamescope (Game Mode) gives
it a window; Steam Input gives it a gamepad. It does **not** replace SteamOS, EmulationStation, EmuDeck or Steam.

## The Deck profile (`src/device.rs`)

`Model::SteamDeck`: 1280x800 (16:10), no device tree. Found by `POCKETUI_DEVICE=steamdeck` (what the launcher sets) or
by DMI (`sys_vendor` = `Valve`, `product_name` = `Jupiter` (LCD) / `Galileo` (OLED)). SteamOS owns the radios, sleep, fan,
clock and updates, so `Device::host_owned()` hides these Settings rows (`app_settings_groups::host_owned_page`): Wi-Fi,
Remote access, Bluetooth, Tailscale, Performance & fan, Time, Updates, Tools, Store, and two with nothing to work
with there: Sync (ROCKNIX's Syncthing service) and Haptics (the handhelds' motor). Sleep & power stays with one row, the
slideshow before the screen goes off (pocket-ui's own; `sleep_settings::rows`). The category lines under Sound & feel, Playing,
Connections, System and Help & tools name only the rows that are left (`DECK_LINES`), and the Add games and Extras rows
leave out file sharing and PortMaster (`row_line`). `has_fan`, `has_wifi`, `has_sticks` (the LED row) are false, so the other pages that ask say "none" instead of offering a mock. Not touched: the
handhelds (no change for any other model; `host_owned()` is false for them).

## Running it

| Variable | Meaning |
|---|---|
| `POCKETUI_DEVICE=steamdeck` | the profile |
| `POCKETUI_DATA` | settings, covers, history (the launcher uses `~/.local/share/pocket-ui`) |
| `POCKETUI_ES_SYSTEMS` | the generated `es_systems.cfg` |
| `POCKETUI_HOST_BIN` | path of `pocket-core` (default `/storage/pocket-ui/pocket-core`) |
| `POCKETUI_RUNEMU` | the `runemu.sh` stand-in (default `/usr/bin/runemu.sh`) |
| `POCKETUI_CORES` | `:`-separated folders searched for `<core>_libretro.so` after `/storage/cores` |
| `POCKETUI_STATES` | save states (the launcher uses `<data>/savestates`; SteamOS has no writable `/storage`) |
| `POCKETUI_BIOS` | BIOS files, pocket-core's system folder (the launcher uses `<data>/bios` and links the player's EmuDeck/RetroDECK BIOS into it) |

Build with `--features x11` (`Cargo.toml`): winit/glutin/softbuffer also get an X11 backend, loaded with `dlopen`. On
a Wayland session winit still takes Wayland first; Game Mode's XWayland is covered by the X11 path. Without the feature
(all handhelds) nothing changes.

## Building the packages (from main, one command)

On an x86_64 Linux machine with this repository (the Flatpak step runs in a container on the build host named by `DECK_BUILD_HOST`):

    nice deploy/steamdeck/build-bundle.sh && deploy/steamdeck/build-flatpak.sh target/deck-bundle

For an internal QA package, set `POCKETUI_VERSION` (for example `0.3.1-deck-qa`) and `POCKETUI_COMMIT` to the source commit.
The archive uses the same version as the binaries; without an override both use Cargo.toml.

It builds `pocket-ui` and `pocket-core` with `--release --features x11`, writes `target/pocket-ui-deck-<version>.tar.gz`
(install.sh, Add to Steam, fetch-cores.sh and the Steam Input template inside) and then `target/pocket-ui-deck.flatpak`.
`DECK_ZIGBUILD=1` cross-builds against glibc 2.38 (`x86_64-unknown-linux-gnu.2.38`) on Linux too — the reproducible way, so
a build host with a newer glibc than SteamOS makes no binary that dies on the Deck; the packaged binaries are path-checked
either way (`scripts/check-binary-paths.sh`). `CORES_DIR=<dir>` puts cores into the
bundle (licences: "Cores" below). The merge gate compiles the Deck-only code on every merge (`scripts/check-platforms.sh`), so main
stays buildable for the Deck without a long-lived branch.

## Library (`deploy/steamdeck/gen-es-systems.py`)

Looks for ROMs in: `$POCKETUI_ROMS`, EmuDeck `~/Emulation/roms/<system>`, RetroDECK `~/retrodeck/roms/<system>`, ES-DE (`~/ES-DE/ROMs`, or `ROMDirectory` in
`es_settings.xml`), the SD card (`/run/media/*/Emulation/roms`, `/run/media/*/retrodeck/roms`). Folder names are mapped (`genesis` -> Mega Drive, `gc` ->
GameCube, `n3ds` -> 3DS, ...), empty systems are hidden by the app. The generator keeps the complete system catalog so the shared reader can
recognise other-card folder names at that ROM root, even when no standard folder exists. Written fresh at every start, so new ROMs show up.
BIOS: the launcher links the files of `~/Emulation/bios` and `~/retrodeck/bios` (and the SD card's copies, their `dc/`
folder included) into pocket-core's BIOS folder in the data (`POCKETUI_BIOS`), never replacing a file that is there, so
putting a BIOS where EmuDeck or RetroDECK expects it is enough. Their `saves` are not read: pocket-core writes battery
saves beside the game (the ROM's folder, as on the handhelds) and its states into the data folder. gamelist.xml of ES-DE
(the existing gamelist reader takes ROCKNIX paths; not checked).

## Shared 0.3 features on the Deck

Retro and Barlow, Normal and Large text, long game names, printed discs and cartridges use the same painters as the
handhelds, through the Deck's scaled 1280×800 canvas. Console cards keep their full names in the reserved title band,
including neighbouring cards in Large Retro; wrapped names are not translated word by word. Focused manual links
keep their text contrast in both appearances and Retro palettes. The shared library reader recognises other cards' folder names
and existing artwork; the generated system list still starts from an EmuDeck, RetroDECK or ES-DE ROM root.

Cheats apply to pocket-core games. The launcher finds a database already downloaded by RetroArch at
`~/.var/app/org.libretro.RetroArch/config/retroarch/cheats` or `~/.config/retroarch/cheats`, read only inside the Flatpak.
An explicit `POCKET_CHEATS_DB` wins. The Deck package ships and downloads no cheat database; without one, no cheat
list is offered. Emulator guests keep their own cheat settings.

Updates and their automatic notice stay off (`host_owned`); there is no Deck OTA channel. Reinstall an internal
bundle or Flatpak to update it. Save and load errors use main's confirmed-outcome handling; old saves stay in place.

## Windows without sway (`src/desk.rs`)

On the handhelds `control.rs` asks sway to put pocket-ui or the game in front. Where the system owns the screen (the Deck
profile, or `POCKETUI_WINDOWING=x11`) the same questions go to plain X11 requests (x11rb): the game's window is the top-most
mapped top-level window that is not pocket-ui's (`WM_CLASS` `pocket-ui`, which the window now sets on X11 too), "in front" is a
raise plus the input focus (Gamescope shows the focused window), "close" is `WM_DELETE_WINDOW`. Without a window manager (Xvfb) the
windows are also sized to the screen. Every request is logged as `desk: ...`. The pause menu opens over a guest and ends it; seen
in the headless runs below. With no X11 build (`--features x11` off) every call says no.

## Emulators

* **Libretro systems (NES, SNES, GB/GBC/GBA, Mega Drive, SMS/GG, PC Engine, PS1, N64, NDS, Dreamcast):** `pocket-core` loads the core
  from `POCKETUI_CORES`: the app's `cores/` and `pocket-ui-app/cores` (what `fetch-cores.sh` writes), then RetroArch's Flatpak
  (`~/.var/app/org.libretro.RetroArch/config/retroarch/cores`, and its install folder in `/var/lib/flatpak/app/...` or
  `~/.local/share/flatpak/app/...`; **these paths are from the Flatpak layout, not seen on a Deck**), then a normal install.
  A core that is not there says so: "The mgba core isn't installed: get it in RetroArch (Flatpak) > Online Updater > Core
  Downloader, or run fetch-cores.sh." (`fallback::core_missing_named`; before this a missing core restarted the game for ever,
  fixed on main in `e31dd7a`).
* **Big emulators:** their Flatpak ids are the emulator names in the generated `es_systems.cfg`: PCSX2 `net.pcsx2.PCSX2`,
  Dolphin `org.DolphinEmu.dolphin-emu` (GameCube, Wii), PPSSPP `org.ppsspp.PPSSPP`, Cemu `info.cemu.Cemu`, RPCS3 `net.rpcs3.RPCS3`,
  xemu `app.xemu.xemu`, Azahar `org.azahar_emu.Azahar` (3DS), Vita3K `org.vita3k.Vita3K`. `runemu.sh` starts them (inside our own
  Flatpak through `flatpak-spawn --host flatpak run`). Command lines are from each emulator's documentation, **not run on a Deck**.
  hakuX (our Xbox core) has no x86_64 build; Xbox is xemu.
  - **PCSX2 has the full pause menu:** it is the ARMSX2 integration (`standalone::Emu::Armsx2`) pointed at the Flatpak: PINE socket
    `$XDG_RUNTIME_DIR/app/net.pcsx2.PCSX2/pcsx2.sock`, `PCSX2.ini` in `~/.var/app/net.pcsx2.PCSX2/config/PCSX2/inis/` (created
    with PINE on if the Flatpak never ran), process `pcsx2-qt`. Save, Load and the resume state on Quit are therefore as on the
    handhelds, **but untested against a real PCSX2 Flatpak** (only the paths and the ini edit are unit-checked).
  - **Every other Flatpak is a guest** (`standalone::guest::Guest::Flatpak`): the pause menu offers Continue and Quit and says the
    emulator keeps its own saves. Quit asks the window to close, waits 8 s, runs `flatpak kill <id>`, then SIGTERM and SIGKILL to the
    game's group; it ends as soon as neither the group nor `flatpak ps` lists the app. A Flatpak that isn't installed says so
    ("org.DolphinEmu.dolphin-emu isn't installed: install it from Discover (Flatpak).").
  Tested with a stand-in `flatpak` and a Tk window under Xvfb: launch, pause menu over it, Quit closes it (the stand-in ignores the
  close request, so `flatpak kill` ended it after 8 s).

### Cores: the choice and the licences

The Deck app **ships no cores**. It uses RetroArch's Flatpak cores when present, else `fetch-cores.sh` downloads them from the
libretro buildbot to the player's own machine (nothing of ours is redistributed; the project's rule that its servers never hold
them stays). Default set, all free-licence: mGBA (MPL-2.0), Gambatte, Nestopia, Gearsystem (GPL-3.0-or-later), melonDS DS, PCSX
ReARMed, Mupen64Plus-Next, Mednafen PCE Fast, Flycast (GPL). Gambatte and Mupen64Plus-Next are GPL-2.0-only, the combination question
The licence inventory (LICENCES.md, section 9) raises for any distributed binary; downloading them on the player's machine avoids shipping the
combination, it does not answer the question. `fetch-cores.sh --personal` adds Snes9x and Genesis Plus GX (non-commercial licences,
"personal use": fine for the player's own copy, never bundled). pocket-core still routes SNES to Snes9x and Mega Drive to Genesis
Plus GX as on the handhelds, so those two systems need `--personal` (or RetroArch's copies); no free-licence core replaces them
without changing core options and save states, which is a separate job.

## Controller

Steam Input shows pocket-ui a virtual Xbox 360 pad (`/dev/input/event*`), which `src/pad.rs` reads by evdev (no other path is
used for the pad). The bundle ships a layout of its own, `deploy/steamdeck/controller_neptune.vdf`: Valve's plain Gamepad
plus the Deck's back buttons and trackpads. `install.sh` copies it to Steam's `controller_base/templates`, and the player
picks it once (pocket-ui > Controller settings > Templates > **pocket-ui**):

* the sticks, D-pad, face buttons, bumpers, triggers, Start (Menu) and Select (View) go through as they are;
* **L4 = L1, R4 = R1** (the upper back buttons repeat the bumpers: HOME+L4 loads, HOME+R4 saves);
* **L5 and R5 = Guide**, pocket-ui's HOME (the pause menu in a game, quick settings on the shelf). The Steam button and the
  "..." button stay Steam's, so its overlay, Exit Game and quick-access menu never fight HOME. Without this, Steam keeps
  its button's Guide for itself and pocket-ui's pause menu has no button of its own on the Deck;
* the **right trackpad is a mouse** (a click taps), the **left one scrolls**: pocket-ui's window feeds the pointer into the
  same tap/swipe path as a finger (`app_touch::mouse_*`), so menus take clicks, drags and the wheel, and a game keeps its own.
  The Deck's touch screen reaches Gamescope as that same mouse, so tapping the menus works the same way (its evdev touch
  device is still read only on the handhelds).

The template was written from Valve's format **without a Deck to load it on**; the first hardware session must confirm Steam
lists and applies it. The stock **Gamepad** template is the fallback (same pad, no back buttons, no trackpad mapping —
HOME then needs a back button set to Guide by hand under Controller settings). The face buttons in the menus follow
Settings > Playing > Button layout (Xbox or Nintendo); the controls card prints the Deck's own letters (Y top, X left,
B right, A bottom) and draws L4/L5/R4/R5 with their jobs, whatever the menu layout says. The gyro is unused.

## Installing (the one line)

On the Deck, Desktop Mode, in the folder the bundle was unpacked to:

```
tar xzf pocket-ui-deck-0.3.1.tar.gz && ./deck-bundle/install.sh
```

That copies pocket-ui to `~/.local/share/pocket-ui-app`, installs the Steam Input template and adds pocket-ui to Steam as
a non-Steam game (close Steam first, it rewrites its shortcuts on exit). The Flatpak is the other shape of the same thing
(`flatpak install --user pocket-ui-deck.flatpak`), added to Steam by hand (Games > Add a Non-Steam Game, the file
`io.github.pocketui.PocketUI`). Games come from the EmuDeck, RetroDECK or ES-DE folders already on the Deck.

## Sleep: the Deck's power button

SteamOS suspends the whole Deck on its power button, without asking the game. pocket-ui watches logind's
`PrepareForSleep` (`sleep::spawn_host_sleep`, `dbus-monitor --system`, out of the Flatpak through `flatpak-spawn --host`)
and holds the suspend with a delay inhibitor just long enough to write the running game's moment — pocket-core's
auto-resume state (`autosave`, the handhelds' deep-sleep save) or the resume state of the one standalone that has one.
For pocket-core, the inhibitor waits for a fresh completed moment for up to four seconds, rather than a fixed delay.
An unchanged older save or a failed request is not reported as saved. SteamOS owns the final suspend deadline;
if saving fails or the host deadline expires, the game stays in memory and battery loss can still lose that progress.
The log distinguishes a confirmed moment from a suspend without one. Waking returns to the game in memory. The big emulators
keep their own saves, as their pause menu says. Without `dbus-monitor` on the host the watch logs once and does
nothing. Monitor and inhibitor children are killed and reaped when released, receive a parent-death signal
if their owning watcher thread/process ends, and close unrelated inherited descriptors at exec. The handheld power-key
inhibitor survives its startup-thread handoff and ends when the UI closes its stdin pipe. Flatpak host helpers use
`--watch-bus` so they end with the spawning Flatpak connection too. Not seen on a Deck yet (the log line to look for: `sleep: the host is suspending; …`).

Continue, the open cases and "Where was I?" run on the Deck for the games pocket-core plays: their states live in
pocket-ui's data here (`POCKETUI_STATES`), so `frozen::available` is on when the launcher set that folder; the big
emulators never stand open. Zoom is pocket-core's like everywhere (Options, or HOME+X when its shortcut is on).

## Leaving

SELECT+START (or Escape) opens "Quit pocket-ui?" with Cancel, **Quit to Steam** (exit 0: the launcher script `exec`s pocket-ui, so
Steam is back) and Report a problem. There is no EmulationStation on a Deck: the dialog and Settings > About leave it out.
Steam's own Exit Game (Steam button > Exit Game) ends the process as well; the FIFO `quit` and a closed window exit cleanly.

## First start and Settings

The first-start steps skip Wi-Fi and the time zone (and count five steps, not seven), the Extras step has no PortMaster row, Add games
says where games go (Emulation/roms, the SD card) instead of offering file sharing, the button layout starts as Xbox (the Deck's A is the
bottom button; the symbols were Xbox already), and the battery is read from `BAT1` (the launcher sets `POCKETUI_BATTERY`). The Welcome text leaves out Wi-Fi and the time zone (SteamOS owns both), the About page has no
EmulationStation row, CPU and fan are not restored after a game (they were never changed), and Settings hides what SteamOS owns
(see the profile above). 1280x800 is painted at 1536x960 and scaled down (`scaled_layout`), because the pages are made for more lines
than 800 pixels hold: at 800 pixels the first-start pages ran into their hint bars.

Kids mode is off here: Steam's button opens its overlay over anything pocket-ui shows, so neither the Settings row nor the first start's "For a child" is there
(`device::hides`; docs/KIDS-MODE.md).

## What needs real hardware

Exactly what has **not** been seen on one, in the order it would matter:
1. Game Mode itself: Steam starting the shortcut, Gamescope's real window rules (the headless Gamescope in a container gave a
   1280x800 window and focus handling as above), the Steam button, the "..." menu, Exit Game.
2. The shipped Steam Input template (`controller_neptune.vdf`): does Steam list and apply it, do L5/R5 arrive as Guide
   (HOME opens the pause menu, the Steam button still opens Steam's), L4/R4 as the bumpers, the trackpads as mouse and
   wheel — and the evdev pad's names and rumble.
3. The power button: suspend in a pocket-core game writes its moment first (log `sleep: the host is suspending; …`),
   waking is back in the game, and a start after the battery died in sleep continues from that moment.
4. Every big emulator's command line and window behaviour (`runemu.sh`); PCSX2's PINE socket in the Flatpak; Quit of each.
5. `flatpak-spawn --host` from inside our Flatpak (needs the Flatpak portal; the container has none), `flatpak info/ps/kill` through it.
6. RetroArch Flatpak's real core folders; GL/Vulkan under the Deck's GPU (pocket-core's GL cores ran on llvmpipe only); audio route;
   the picture Looks and Zoom on the Deck's panel (rendered headless only).
7. Performance and battery: nothing was measured; the Deck's own power and fan stay with SteamOS.
8. Touch through the mouse path; the Desktop-Mode install (`install.sh`) on a real SteamOS; `add-to-steam.py` on a real
   `shortcuts.vdf` (tested on a made-up one, binary VDF round trip); a RetroDECK folder and an EmuDeck BIOS seen for real.

## First real Deck (SteamOS 3.7, 2026-10-29, over SSH, headless Gamescope, sound into a null sink)

Run on a Deck (kernel 6.11 valve, glibc 2.41) that was in use: `gamescope --backend headless -W 1280 -H 800` as the `deck` user at nice 19,
`PULSE_SINK`/`PIPEWIRE_NODE` on a temporary null sink (the game's stream was seen on the null sink, none on the speakers; the sink was
removed afterwards), data in a copy folder (`POCKETUI_APP_DATA`), the player's ROMs and saves untouched.
- A binary built on a newer glibc does not start (a newer Ubuntu build host's gave `GLIBC_2.43`); build on an older glibc (2.39) or with zigbuild.
- The binaries built on glibc 2.39 run on SteamOS's libraries; the device is found (`device: Steam Deck`), the library is scanned, the shelf draws at 1280x800 with the real status corner (clock, Wi-Fi).
- GBA and SNES test games run through pocket-core with the **RetroArch Flatpak's own cores** (`mgba_libretro.so`, `snes9x_libretro.so`), the pause menu opens over them, Quit ends them.
- PCSX2 (user Flatpak, branch `stable`): started through `runemu.sh`, window `pcsx2-qt` found, pause menu over it, Quit closed it in 0.6 s. PPSSPP Flatpak started for a PSP game as the fallback when the core was missing.
- Found there: `flatpak run` finds nothing when `XDG_DATA_HOME` is moved (flatpak's user installation lives under it): the launcher now has `POCKETUI_APP_DATA`; a Flatpak does not inherit the caller's environment, so `runemu.sh` passes `PULSE_SINK`, `PIPEWIRE_NODE` and `SDL_AUDIODRIVER` on; the Quick settings > Power menu showed the Android boot hint (hidden on the Deck now).
- Side effects on the player's setup (all emulator-written): PCSX2.ini got `EnablePINE = true`, `Renderer`, `OsdMessagesPos` from our integration (restored from a backup after the test); PPSSPP rewrote its own `ppsspp.ini` on exit.

Automatic states from 0.2.1 use the same compressed `.pstate.auto.z` format as the handheld Linux player and Android.
`POCKETUI_STATES` still selects the folder. Manual slots stay raw, old raw Auto states remain readable, and older players
fall back to in-game saves without touching compressed moments; see [the pause menu](user/pause-menu.md).

The Achievements list keeps its achievement and point totals below its heading, so the wider Retro title face
and Large text do not shorten the totals at 1280×800.

The headless X11 test image includes `libxkbcommon-x11-0`, which winit loads dynamically before painting
a snapshot or opening its window under Xvfb.

Replacement covers keep every wrapped line of a game name untranslated (for example `Golden Sun`, whose
second line must not become the German abbreviation for Sunday).

The Cards title band also fits game names in full, including neighbouring publisher/Curator game cards
with Large Retro text. It uses the same measured title block as console names.
All printed lines use the smallest fitted size in that block, so a subtitle keeps its measured width.
Translated labels in the system's short facts fit inside their column, including Spanish in Retro.
