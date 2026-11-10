#!/bin/sh
# pocket-ui on a Steam Deck (docs/STEAMDECK.md). This is what the "Add to Steam" shortcut starts.
# Layout next to this script (the bundle, or /app/share/pocket-ui in the Flatpak):
#   pocket-ui  pocket-core  runemu.sh  gen-es-systems.py  [cores/<name>_libretro.so]
set -u
here=$(cd "$(dirname "$0")" && pwd)
data=${POCKETUI_APP_DATA:-${XDG_DATA_HOME:-$HOME/.local/share}/pocket-ui}
mkdir -p "$data" "$data/logs"

export POCKETUI_DEVICE=${POCKETUI_DEVICE:-steamdeck}
export POCKETUI_DATA=$data
export POCKETUI_ES_SYSTEMS=$data/es_systems.cfg
# Save states live with pocket-ui's data here: SteamOS has no writable /storage (the handhelds' states folder).
export POCKETUI_STATES=${POCKETUI_STATES:-$data/savestates}
mkdir -p "$POCKETUI_STATES"
# BIOS files likewise (`POCKETUI_BIOS`): pocket-core's system folder in the data, filled with links to the player's
# EmuDeck/RetroDECK BIOS (their files stay where they are; a file already here is never replaced).
export POCKETUI_BIOS=${POCKETUI_BIOS:-$data/bios}
mkdir -p "$POCKETUI_BIOS"
for src in "$HOME/Emulation/bios" "$HOME/retrodeck/bios" /run/media/*/Emulation/bios /run/media/*/retrodeck/bios; do
  [ -d "$src" ] || continue
  for f in "$src"/* "$src"/dc/*; do
    [ -f "$f" ] || continue
    rel=${f#"$src"/}
    case $rel in */*) mkdir -p "$POCKETUI_BIOS/${rel%/*}" ;; esac
    [ -e "$POCKETUI_BIOS/$rel" ] || ln -s "$f" "$POCKETUI_BIOS/$rel" 2>/dev/null || cp -n "$f" "$POCKETUI_BIOS/$rel" 2>/dev/null || true
  done
done
# The Deck's battery is BAT1 (the handhelds' is `battery`): the status corner, Settings > Battery and the low-battery warning read it.
for b in BAT1 BAT0; do [ -r "/sys/class/power_supply/$b/capacity" ] && { export POCKETUI_BATTERY=${POCKETUI_BATTERY:-/sys/class/power_supply/$b}; break; }; done
export POCKETUI_HOST_BIN=$here/pocket-core
export POCKETUI_RUNEMU=$here/runemu.sh
# Cores: ours first, then RetroArch's Flatpak (installed from Discover, or by EmuDeck), then a normal install.
export POCKETUI_CORES="$here/cores:${XDG_DATA_HOME:-$HOME/.local/share}/pocket-ui-app/cores:$HOME/.var/app/org.libretro.RetroArch/config/retroarch/cores:/var/lib/flatpak/app/org.libretro.RetroArch/current/active/files/lib/retroarch/cores:$HOME/.local/share/flatpak/app/org.libretro.RetroArch/current/active/files/lib/retroarch/cores:$HOME/.config/retroarch/cores:/usr/lib/libretro:/usr/lib/x86_64-linux-gnu/libretro"

# Cheats use the player's existing RetroArch database; both the menu and pocket-core inherit this path.
# An explicit database always wins. No database is downloaded or shipped in the Deck package.
if [ -z "${POCKET_CHEATS_DB:-}" ]; then
  for db in "$HOME/.var/app/org.libretro.RetroArch/config/retroarch/cheats" "$HOME/.config/retroarch/cheats"; do
    [ -d "$db" ] || continue
    export POCKET_CHEATS_DB=$db
    break
  done
fi

# ROMs: EmuDeck, ES-DE, the SD card (gen-es-systems.py). No ROM folder is not an error: pocket-ui shows its empty shelf.
if [ -z "${POCKETUI_ES_KEEP:-}" ]; then
  python3 -I "$here/gen-es-systems.py" "$POCKETUI_ES_SYSTEMS" ${POCKETUI_ROMS:+"$POCKETUI_ROMS"} >"$data/logs/library.log" 2>&1 \
    || printf '<?xml version="1.0"?>\n<systemList>\n</systemList>\n' >"$POCKETUI_ES_SYSTEMS"
fi

# Gamescope (Game Mode) gives a Wayland socket or XWayland; winit takes Wayland first, X11 otherwise.
# Steam Input's virtual pad is read from /dev/input (evdev), so the controller layout must be "Gamepad".
exec "$here/pocket-ui" "$@" >>"$data/logs/pocket-ui.log" 2>&1
