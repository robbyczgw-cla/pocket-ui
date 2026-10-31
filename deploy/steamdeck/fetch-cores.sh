#!/bin/sh
# Downloads libretro cores for pocket-core into the app's cores/ folder, from the libretro buildbot, to YOUR machine.
# Only needed when RetroArch's Flatpak (org.libretro.RetroArch, Online Updater > Core Downloader) doesn't have them;
# pocket-ui finds both (POCKETUI_CORES). Nothing is shipped with pocket-ui: licences are yours to read (docs/STEAMDECK.md).
#   fetch-cores.sh            the free-licence set: GPL, MPL and MIT cores
#   fetch-cores.sh --personal also snes9x and genesis_plus_gx (non-commercial licences: for your own use only)
set -eu
dest=${POCKETUI_CORES_DIR:-${XDG_DATA_HOME:-$HOME/.local/share}/pocket-ui-app/cores}
base=https://buildbot.libretro.com/nightly/linux/x86_64/latest
free="mgba gambatte nestopia gearsystem melondsds pcsx_rearmed mupen64plus_next mednafen_pce_fast"
personal="snes9x genesis_plus_gx"
set -- ${1:+$1}
list=${CORES:-$free}
[ "${1:-}" = "--personal" ] && list="$free $personal"
mkdir -p "$dest"; tmp=$(mktemp -d); trap 'rm -rf "$tmp"' EXIT
for c in $list; do
  if curl -fsSL "$base/${c}_libretro.so.zip" -o "$tmp/$c.zip" && (cd "$tmp" && python3 -I -c 'import sys,zipfile;zipfile.ZipFile(sys.argv[1]).extractall(sys.argv[2])' "$c.zip" "$dest"); then echo "ok   $c"; else echo "FAIL $c (not on the buildbot for x86_64?)"; fi
done
echo "cores in $dest"
