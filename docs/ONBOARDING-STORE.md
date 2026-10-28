# First start, store and device defaults (research 2026-10-21)

> **Status: design notes, partly built.** The first start as built is Welcome, Look, How do you like to play? (the
> profiles and the feature cards, `src/features.rs`), Wi-Fi, Time zone, Extras (cover art and PortMaster games), Add
> games (file sharing) and Done; see [user/getting-started.md](user/getting-started.md).
> Not built: the language step of the first start, the web upload with a QR code, extra cores
> and tools in first-start Extras, and the minisign-signed index (the Store uses Ed25519, see
> [STORE.md](STORE.md)). The Store itself is built.

[V] = verified in source, [I] = inference. Full paths are in the ROCKNIX tree under research/distribution.

## Principle
The OS stays minimal: the ROCKNIX image plus pocket-ui. Everything optional is chosen on first start or
later in the store. Defaults for the Mini V2 are tuned so nothing needs configuring before the first game.

## First start (NextUI-minimal, every step skippable)
1. **Language:** `set_setting system.language`.
2. **Wi-Fi:** networks sorted by signal → on-screen keyboard → connected.
   - Backend [V]: NetworkManager with iwd; keyfiles in `/storage/.config/NetworkManager/system-connections`.
   - Commands: `wifictl connect "$SSID" "$PSK" AT`, then `set_setting wifi.enabled 1`.
   - Scan with `nmcli -t -f SSID,SIGNAL,SECURITY,IN-USE dev wifi list --rescan yes`. Online check: `/usr/bin/amionline`.
   - On-screen keyboard: our own gamepad grid. ROCKNIX's wvkbd is touch-only.
3. **Time zone:** suggested, confirm with A → `set_setting system.timezone` (NTP follows) [I]. **Built** (after Wi-Fi):
   the guess is the system's zone unless it is UTC, else the country of `iw reg get` (the chip's own `phy#N
   (self-managed)` domain first, the global one is "00" until a network has told the system) mapped by a small table
   and `/usr/share/zoneinfo/zone.tab`, else UTC. Countries with several zones open the picker on their own list.
   No IP geolocation. NetworkManager (`nmcli`) has no country field, checked on the Mini V2 on 2026-10-22.
4. **Extras** (online, multi-select, runs in the background; the same list lives in the store later):
   - PortMaster games, ready to run
   - Extra emulator cores
   - Tools
5. **Copy games:**
   - Show QR code + address for the web upload (simple-http-server, port 80, `/storage`).
   - Also Samba share `games-roms` [V]. Both are off by default (`samba/simplehttp.enabled`) and get switched on here.
   - Note: BIOS files go to `/storage/roms/bios`, with a check view.
6. **Done:** go to Home ("Continue" or the empty state).

## Store
Same pattern as NextUI's Pak Store (LoveRetro/nextui-pak-store, MIT):
- a curated base JSON,
- CI builds the index,
- the index is served from GitHub Pages.

On top of that, per-artifact **sha256 + a minisign-signed index**. The Pak Store itself has no signatures.

What can be installed on ROCKNIX:
- **PortMaster ports** [V]:
  - Catalog: `PortsMaster/PortMaster-New/releases/latest/download/ports.json`; 1451 ports, 996 aarch64, 700 ready-to-run.
  - We list them natively and install with `harbourmaster install/uninstall`; the PortMaster setup runs once headless.
  - Paid/full ports show their `inst` text: the user supplies game data.
- **libretro cores** [V]: `/tmp/cores` is an overlay of `/usr/lib/libretro` with upper `/storage/cores`, so a
  `.so` dropped there wins. Source: buildbot.libretro.com aarch64 nightly (CRC32 only, so we pin our own hashes).
- **AppImages** in `roms/ports` launch via runemu [V]. Flatpak is not supported.
- Themes and tools for pocket-ui.
- **No BIOS files** (copyright). A checker only.

## In-game features via RetroArch (no own emulator host needed)
- **Network commands** on UDP 55355: SAVE_STATE / LOAD_STATE / STATE_SLOT_* / PAUSE_TOGGLE / MENU_TOGGLE /
  QUIT / GET_STATUS.
  - Off by default (`network_cmd_enable = "false"`). ROCKNIX already enables them per launch for lowerdeck
    (runemu.sh:304-317), and we can follow the same pattern.
- **State thumbnails:** `savestate_thumbnail_enable = "true"` gives `.state.png` next to each state, ready for a NextUI-style menu.
- **Auto resume:** `savestate_auto_save/load` through setsettings (`--autosave`), blocked in RA hardcore mode.

## Mini V2 defaults to set (next step)
- Auto save + auto load state per game (instant resume).
- RetroArch network commands on, for the quick menu and sleep.
- Video: integer scale off; core-provided aspect; 1240×1080 output; a light LCD shader for GB/GBC/GBA (later).
- Hotkeys:
  - HOME → pocket-ui quick menu
  - SELECT+START → quit game
- Stick deadzone ~15 % (left stick X rests at +12 %).
- CPU governor schedutil in menus; ROCKNIX switches to performance while a game runs (runemu).
