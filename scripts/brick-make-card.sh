#!/bin/bash
# Builds the standalone TrimUI Brick card: one folder (and a zip of it) to copy onto an empty FAT card
# (docs/TRIMUI-BRICK-STANDALONE.md).
#   scripts/brick-make-card.sh OUT_DIR BINS_DIR LIBS_DIR CORES_DIR [VERSION]
# VERSION    what the programs were built as (POCKETUI_VERSION; `pocket-ui --version` prints it): app/RELEASE, compared by the updater.
#            A card meant for the update channel needs a version that counts up, e.g. 0.1.14-brick.6 (release 0.1.14, beta 6; docs/TRIMUI-BRICK-REMOTE.md, scripts/brick-publish.sh).
# PARTS      (environment BRICK_PARTS) the output of scripts/brick-remote-parts.sh: Tailscale, curl, dropbear with their licences
# BRICK_UPDATE_URL  (optional) the update channel's update.json, written to app/update-url (docs/TRIMUI-BRICK-REMOTE.md); never public
# BINS_DIR   pocket-ui, pocket-core, pocket-fb (cargo zigbuild --release --features brick --target aarch64-unknown-linux-gnu.2.33)
#            (CARGO_TARGET_DIR set before `. scripts/release-rustflags.sh`) and brick-hw
#            (zig cc -target aarch64-linux-gnu.2.33 -O2 -s -ffile-prefix-map=$PWD=/pocket-ui deploy/brick/brick-hw.c)
# LIBS_DIR   libwayland-client.so.0 libwayland-cursor.so.0 libwayland-egl.so.1 libxkbcommon.so.0 (Debian bookworm arm64, what pocket-ui
#            loads) and libffi.so.8 (libwayland-client's; the Brick only has libffi.so.7)
# CORES_DIR  the output of scripts/brick-build-cores.sh (cores/, licences/, sources/, CORES.txt)
# Result: OUT_DIR/card/ (the folder), OUT_DIR/pocket-ui-brick-card-<version>.zip, and a check that everything the updater
# looks for is in it. Nothing from NextUI or MinUI goes in (their licence is not ours); the Brick's own firmware stays on the Brick.
set -eu
OUT=${1:?out dir}; BINS=${2:?bins}; LIBS=${3:?libs}; CORES=${4:?cores dir}; PARTS=${BRICK_PARTS:?BRICK_PARTS: the folder scripts/brick-remote-parts.sh made}
HERE=$(cd "$(dirname "$0")" && pwd); ROOT=$(dirname "$HERE")
COMMIT=$(git -C "$ROOT" rev-parse --short HEAD 2>/dev/null || echo unknown)
VERSION=${5:-brick-$COMMIT}
die() { echo "brick-make-card: $*" >&2; exit 1; }
for f in pocket-ui pocket-core pocket-fb brick-hw brick-egl; do [ -f "$BINS/$f" ] || die "$BINS/$f is missing"; done
for f in libwayland-client.so.0 libwayland-cursor.so.0 libwayland-egl.so.1 libxkbcommon.so.0 libffi.so.8; do [ -e "$LIBS/$f" ] || die "$LIBS/$f is missing"; done
for c in gambatte mgba nestopia snes9x pcsx_rearmed gpsp fceumm picodrive beetle_ngp beetle_wswan beetle_pce_fast handy stella prosystem fbneo mupen64plus_next ppsspp flycast gearsystem beetle_supergrafx pokemini beetle_vb fake08 tic80 melondsds dosbox_pure scummvm yabause; do [ -f "$CORES/cores/${c}_libretro.so" ] || die "core $c is missing in $CORES/cores"; done
for f in tailscale tailscaled curl dropbearmulti syncthing; do [ -f "$PARTS/bin/$f" ] || die "$PARTS/bin/$f is missing (scripts/brick-remote-parts.sh)"; done
[ -s "$CORES/assets/bios/scummvm/extra/sky.cpt" ] && [ -s "$CORES/assets/bios/scummvm/theme/scummmodern.zip" ] || die "ScummVM engine data/theme bundle is missing"
mkdir -p "$OUT"; OUT=$(cd "$OUT" && pwd)
CARD=$OUT/card
rm -rf "$CARD"; mkdir -p "$CARD/.tmp_update" "$CARD/trimui/app" "$CARD/pocket-ui"/app/{bin,lib,cores,msg,hwroot/usr/bin,licences,sources} "$CARD/pocket-ui/logs" "$CARD/Roms" "$CARD/Saves/savestates"
# pocket-ui/start.sh is the fixed part (it picks the program folder and takes a failed update back); app/ is the program, which an
# update replaces as a whole; storage/, logs/ and authorized_keys next to it are the player's (start.sh makes them on the card).
P=$CARD/pocket-ui/app
cp "$ROOT/deploy/brick/boot.sh" "$CARD/pocket-ui/start.sh"
cp "$ROOT/deploy/brick/start.sh" "$P/start.sh"
cp "$ROOT/deploy/brick/entries/updater" "$CARD/.tmp_update/updater"                          # entry: NextUI's start script
cp "$ROOT/deploy/brick/entries/MainUI" "$ROOT/deploy/brick/entries/premainui.sh" "$ROOT/deploy/brick/entries/preload.sh" "$CARD/trimui/app/"   # entry: TrimUI's own
for f in pocket-ui pocket-core pocket-fb brick-hw brick-egl; do cp "$BINS/$f" "$P/bin/$f"; done
# Our own programs carry no build machine path (scripts/release-rustflags.sh with CARGO_TARGET_DIR set before it is sourced;
# brick-hw with -ffile-prefix-map and -s). BRICK_ALLOW_PATHS=1 for a private test card only.
[ "${BRICK_ALLOW_PATHS:-0}" = 1 ] || "$ROOT/scripts/check-binary-paths.sh" "$P"/bin/pocket-ui "$P"/bin/pocket-core "$P"/bin/pocket-fb "$P"/bin/brick-egl "$P"/bin/brick-hw >/dev/null \
  || die "a program carries a build path (scripts/check-binary-paths.sh $BINS/*)"
for f in tailscale tailscaled curl dropbearmulti syncthing; do cp "$PARTS/bin/$f" "$P/bin/$f"; done
# Symbols are no use on the card (the pocket-ui binaries are already stripped by the release profile; the rest here).
if command -v aarch64-linux-gnu-strip >/dev/null 2>&1; then aarch64-linux-gnu-strip "$P/bin/brick-hw" 2>/dev/null || true; fi
cp -L "$LIBS"/libwayland-client.so.0 "$LIBS"/libwayland-cursor.so.0 "$LIBS"/libwayland-egl.so.1 "$LIBS"/libxkbcommon.so.0 "$LIBS"/libffi.so.8 "$P/lib/"
cp "$CORES"/cores/*_libretro.so "$P/cores/"
cp "$ROOT"/deploy/brick/msg/*.png "$ROOT"/deploy/brick/msg/*.raw "$P/msg/"
cp "$ROOT"/deploy/brick/hwroot/usr/bin/* "$P/hwroot/usr/bin/"
cp "$ROOT/deploy/brick/es_systems.cfg" "$P/es_systems.cfg"; cp "$ROOT/deploy/brick/asoundrc" "$P/asoundrc"
# Time zone data (IANA tz database, public domain): the card carries its own, the firmware's may hold few zones or none.
# The region folders, UTC and the two tables the first start reads; nothing else of the build machine's zoneinfo.
ZONESRC=${BRICK_ZONEINFO:-/usr/share/zoneinfo}
[ -f "$ZONESRC/UTC" ] || die "no zone data at $ZONESRC (set BRICK_ZONEINFO)"
mkdir -p "$P/zoneinfo"
for r in Africa America Antarctica Arctic Asia Atlantic Australia Europe Indian Pacific; do [ -d "$ZONESRC/$r" ] && cp -RL "$ZONESRC/$r" "$P/zoneinfo/$r"; done
cp -L "$ZONESRC/UTC" "$P/zoneinfo/UTC"
for f in zone.tab iso3166.tab; do [ -f "$ZONESRC/$f" ] && cp "$ZONESRC/$f" "$P/zoneinfo/$f"; done
# Licences: ours, the cores' (with the table of verdicts), the bundled libraries'.
"${BRICK_LICENCE_FILES:-$ROOT/scripts/licence-files.sh}" "$P/licences/pocket-ui" >/dev/null
cp "$ROOT"/deploy/brick/licences/* "$P/licences/"
mkdir -p "$P/licences/cores"
cp -R "$CORES"/licences/. "$P/licences/cores/"
cp "$CORES/CORES.txt" "$P/licences/cores/CORES.txt"
# Licence files from a core's subfolders: core builds before 2026-11-05 named their folders "._src", "._ext_zlib": hidden, and macOS
# takes "._" files for its own resource forks and drops them. Visible names ("src", "ext_zlib").
find "$P/licences/cores" -depth -mindepth 2 -type d -name '._*' | while read -r d; do
  t=$(dirname "$d")/$(basename "$d" | cut -c3-)
  if [ -e "$t" ]; then cp -Rn "$d/." "$t/" && rm -rf "$d"; else mv "$d" "$t"; fi   # a folder of that name already: merged, nothing overwritten
done
[ -z "$(find "$P/licences/cores" -name '._*' | head -n 1)" ] || die "a licence folder still has a hidden ._ name"
# FBNeo asks for "the full text of this license", verbatim; its top-level LICENSE.md is only a link to it: the text from its source.
if ! find "$P/licences/cores/fbneo" -iname license.txt -path '*src*' 2>/dev/null | grep -q .; then
  set -- "$CORES"/sources/fbneo-*.tar.gz
  [ -f "$1" ] && tar -xzOf "$1" src/license.txt >"$P/licences/cores/fbneo/license.txt" 2>/dev/null || die "FBNeo's src/license.txt is not in its source archive"
fi
# The three non-commercial cores carry their own licence text word for word (their terms: free to pass on, never to sell).
for f in snes9x/LICENSE picodrive/COPYING; do [ -s "$P/licences/cores/$f" ] || die "licences/cores/$f is missing"; done
find "$P/licences/cores/fbneo" -iname license.txt | xargs grep -l "You may not sell" 2>/dev/null | grep -q . || die "FBNeo's licence text is missing"
python3 -I - "$ROOT/scripts/core-licences.txt" "$CORES/CORES.txt" "$ROOT/deploy/brick/core-licences.txt" >"$P/licences/CORES-LICENCES.txt" <<'PY'
import sys
verdicts = {"or-later": "GPL-2.0-or-later", "v3": "GPL-3.0-or-later", "permissive": "permissive (MIT / MPL-2.0 / zlib)",
            "v2-only": "GPL-2.0-only", "mixed-v2": "GPL-2.0, some files GPL-2.0-only",
            "non-commercial": "NON-COMMERCIAL: personal use only, may not be sold or used in a commercial product"}
table = {}
for line in [*open(sys.argv[1]), *open(sys.argv[3])]:
    if line.startswith("#") or "|" not in line: continue
    core, verdict, commit, evidence = (line.rstrip("\n").split("|", 3) + [""] * 4)[:4]
    table[core] = (verdict, commit, evidence)
print("The libretro cores on this card, built from these upstream commits (where the exact source of each is: SOURCES.txt next to this file).\n")
for l in open(sys.argv[2]):
    core, commit, so, sha = l.split()
    v, c, e = table.get(core, ("unknown", "", ""))
    print(f"{core}: {verdicts.get(v, v)}\n  commit {commit}\n  evidence {e}\n  {so} sha256 {sha}\n")
PY
# The cores' source (hundreds of MB) is on the card next to app/, not in it: an update over Wi-Fi replaces app/ only and needs none of
# it; scripts/brick-publish.sh puts the same archives into the channel's sources/ folder, next to the update.
mkdir -p "$CARD/pocket-ui/sources/cores"; cp "$CORES"/sources/*.tar.gz "$CARD/pocket-ui/sources/cores/"
# The network parts: Tailscale, curl (each as released), dropbear (built with our patch), their licences and where their source is.
for d in tailscale curl dropbear syncthing; do mkdir -p "$P/licences/$d"; cp -R "$PARTS/licences/$d/." "$P/licences/$d/"; done
mkdir -p "$P/sources/network"; cp -R "$PARTS/sources/." "$P/sources/network/"; cp "$PARTS/PARTS.txt" "$P/licences/NETWORK-PARTS.txt"
cat >"$P/licences/README.txt" <<TXT
pocket-ui on a TrimUI Brick (standalone card), $VERSION.
pocket-ui and its helpers: LICENSE, NOTICE.md and the third-party lists in pocket-ui/ (GPL-2.0-or-later and the crates' own licences).
Cores: CORES-LICENCES.txt, and each core's own licence files in cores/. Where the source of each core (the exact commit it was built
from) is: SOURCES.txt. snes9x, picodrive and fbneo are licensed for non-commercial use only (free to pass on under their licence terms, not to sell); see CORES-LICENCES.txt.
Bundled libraries (libwayland-*: MIT, libxkbcommon: MIT-style, libffi: MIT-style): libwayland-COPYING.txt, libxkbcommon-LICENSE.txt, libffi-copyright.txt.
Time zone data (pocket-ui/zoneinfo): the IANA Time Zone Database, public domain.
Network parts (NETWORK-PARTS.txt has their versions and checksums): Tailscale (BSD-3-Clause, tailscale/), Syncthing (MPL-2.0, syncthing/, unchanged), curl and its libraries
(curl/), dropbear (MIT, dropbear/; changed by pocket-ui, the change is in ../sources/network/). Where each one's source is:
../sources/network/SOURCES.txt.
Nothing of NextUI or MinUI is on this card. The Brick's own firmware libraries are used from the Brick, not copied.
TXT
echo "pocket-ui brick standalone $VERSION (pocket-ui commit $COMMIT, $(date -u +%Y-%m-%d))" >"$P/VERSION"
echo "$VERSION" >"$P/RELEASE"
[ -n "${BRICK_UPDATE_URL:-}" ] && echo "$BRICK_UPDATE_URL" >"$P/update-url"
cp "$P/VERSION" "$P/BUILD-INFO.txt"; { echo; cat "$CORES/CORES.txt"; } >>"$P/BUILD-INFO.txt"
# Everything the updater looks for, in the folder.
for f in bin/pocket-ui bin/pocket-core bin/pocket-fb bin/brick-hw bin/brick-egl lib/libwayland-client.so.0 lib/libffi.so.8 msg/starting.png msg/missing.png msg/missing.raw msg/crashed.png es_systems.cfg asoundrc \
  hwroot/usr/bin/volume hwroot/usr/bin/pactl hwroot/usr/bin/brightness hwroot/usr/bin/systemctl hwroot/usr/bin/wifictl hwroot/usr/bin/timedatectl zoneinfo/UTC hwroot/usr/bin/grim cores/gambatte_libretro.so cores/mgba_libretro.so \
  cores/nestopia_libretro.so cores/snes9x_libretro.so cores/pcsx_rearmed_libretro.so cores/gpsp_libretro.so cores/fceumm_libretro.so cores/picodrive_libretro.so VERSION; do [ -s "$P/$f" ] || die "the card folder has no $f"; done
for f in .tmp_update/updater trimui/app/MainUI trimui/app/premainui.sh trimui/app/preload.sh pocket-ui/start.sh pocket-ui/app/start.sh pocket-ui/app/RELEASE \
  pocket-ui/app/bin/tailscale pocket-ui/app/bin/tailscaled pocket-ui/app/bin/curl pocket-ui/app/bin/dropbearmulti pocket-ui/app/bin/syncthing pocket-ui/app/hwroot/usr/bin/brick-ssh pocket-ui/app/hwroot/usr/bin/brick-syncthing \
  pocket-ui/app/hwroot/usr/bin/brick-tailscale pocket-ui/app/hwroot/usr/bin/tailscale; do [ -s "$CARD/$f" ] || die "no $f"; done
# One folder per system, each with a README.txt: tools that drop empty folders (some zip and copy programs) then still keep them.
readme() { # system, what goes there
  case "$1" in
    pc|scummvm) layout="Keep each game's files together in its own subfolder, or use an archive where supported.";;
    *) layout="Put the game files straight in this folder, without sub-folders.";;
  esac
  printf '%s\n' "$2" "" "$layout Delete this file if you like; pocket-ui ignores it." >"$CARD/Roms/$1/README.txt"
}
for s in gb gbc gba nes snes psx megadrive mastersystem gamegear ngp ngpc wonderswan wonderswancolor pcengine atarilynx atari2600 atari7800 fbn neogeo cps1 cps2 cps3 n64 psp dreamcast fds sg-1000 segacd sega32x pcenginecd supergrafx pokemini virtualboy pico-8 tic-80 nds pc scummvm saturn bios; do mkdir -p "$CARD/Roms/$s"; done
readme gb "Game Boy games (.gb, or .zip)."
readme gbc "Game Boy Color games (.gbc, .gb, or .zip)."
readme gba "Game Boy Advance games (.gba, or .zip)."
readme nes "NES games (.nes, or .zip)."
readme snes "Super Nintendo games (.sfc, .smc, or .zip)."
readme psx "PlayStation games (.cue with its .bin files, or .chd, .pbp, .m3u, .iso)."
readme megadrive "Mega Drive / Genesis games (.md, .gen, .bin, .smd, or .zip)."
readme mastersystem "Master System games (.sms, or .zip)."
readme gamegear "Game Gear games (.gg, or .zip)."
readme ngp "Neo Geo Pocket games (.ngp or .zip)."
readme ngpc "Neo Geo Pocket Color games (.ngc or .zip)."
readme wonderswan "WonderSwan games (.ws or .zip)."
readme wonderswancolor "WonderSwan Color games (.wsc or .zip)."
readme pcengine "PC Engine games (.pce or .zip)."
readme atarilynx "Atari Lynx games (.lnx or .zip)."
readme atari2600 "Atari 2600 games (.a26 or .zip)."
readme atari7800 "Atari 7800 games (.a78 or .zip)."
readme fbn "Arcade (FinalBurn Neo): ZIP ROM sets matching FBNeo f3b7749. Keep archives intact; Neo Geo needs neogeo.zip in the game folder or bios/. No ROM sets are included."
readme neogeo "Neo Geo: ZIP ROM sets matching FBNeo f3b7749. Keep archives intact; Neo Geo needs neogeo.zip in the game folder or bios/. No ROM sets are included."
readme cps1 "Capcom Play System: ZIP ROM sets matching FBNeo f3b7749. Keep archives intact; Neo Geo needs neogeo.zip in the game folder or bios/. No ROM sets are included."
readme cps2 "Capcom Play System II: ZIP ROM sets matching FBNeo f3b7749. Keep archives intact; Neo Geo needs neogeo.zip in the game folder or bios/. No ROM sets are included."
readme cps3 "Capcom Play System III: ZIP ROM sets matching FBNeo f3b7749. Keep archives intact; Neo Geo needs neogeo.zip in the game folder or bios/. No ROM sets are included."
readme n64 "Nintendo 64 games (.z64, .n64, .v64, or .zip). Played by mupen64plus_next on the GPU."
readme psp "PSP games (.iso, .cso, .pbp, or .chd). Played by PPSSPP on the GPU."
readme dreamcast "Dreamcast games (.cdi, .gdi, .chd, .cue, or .m3u). Experimental: Flycast on the GPU, not at full speed (Virtua Tennis 2: 37 fps). Its BIOS files go in ../bios/dc/ (see the README there)."
mkdir -p "$CARD/Roms/bios/dc"
readme bios/dc "Dreamcast BIOS, put here by the owner: dc_boot.bin and dc_flash.bin (from your own console). The card does not include them. Flycast starts without them on its built-in replacement, with lower compatibility."
readme fds "Famicom Disk System games (.fds or ZIP). Put disksys.rom in Roms/bios."
readme sg-1000 "SG-1000 games (.sg, .sg1 or ZIP)."
readme segacd "Sega CD discs (CUE with all its tracks, CHD, ISO or M3U). Put the matching region BIOS bios_CD_U.bin, bios_CD_E.bin or bios_CD_J.bin in Roms/bios."
readme sega32x "Sega 32X games (.32x, BIN or ZIP). Game speed has not been measured on the Brick."
readme pcenginecd "PC Engine CD discs (CUE with tracks, CCD, CHD or M3U). Requires exactly Roms/bios/syscard3.pce."
readme supergrafx "SuperGrafx games (.sgx, .pce or ZIP)."
readme pokemini "Pokemon mini games (.min or ZIP). Optional BIOS: Roms/bios/bios.min."
readme virtualboy "Virtual Boy games (.vb, .vboy or ZIP). Some games need a second D-pad that the built-in pad cannot provide with the current mapping."
readme pico-8 "PICO-8 carts (.p8 or .p8.png), played with the free FAKE-08 emulator. The official PICO-8 runtime is not required. Compatibility varies."
readme tic-80 "TIC-80 Lua carts (.tic). Other cartridge languages are not enabled in this build."
readme nds "Nintendo DS games (.nds). Experimental: Brick speed is unmeasured. Games requiring touchscreen controls are limited by the built-in pad."
readme pc "DOS games (ZIP, DOSZ, EXE, COM or CONF). Keep all files for a game together. Gamepad-friendly games work best with the built-in controls."
readme scummvm "ScummVM game folders or .scummvm launch files. Supply all original game data. Engine data and themes are included in Roms/bios/scummvm. Mouse controls are limited on the built-in pad."
readme saturn "Saturn discs (CUE with tracks, CHD, ISO or M3U). Experimental software emulation: expected too slow for many games, Brick speed unmeasured. Put saturn_bios.bin, sega_101.bin or mpr-17933.bin in Roms/bios."
# Data the cores read from the BIOS folder (ScummVM's engine data and themes) travel in app/ (an update brings them); app/start.sh puts
# them into Roms/bios once per version. A card from the zip has them there already.
[ ! -d "$CORES/assets/bios" ] || { mkdir -p "$P/bios-data"; cp -R "$CORES/assets/bios/." "$P/bios-data/"; cp -R "$CORES/assets/bios/." "$CARD/Roms/bios/"; }
readme bios "BIOS files, for example the PlayStation BIOS scph5501.bin. Without one, PlayStation games use the built-in replacement. Dreamcast BIOS goes in the dc folder."
printf '%s\n' "Save states made by pocket-ui go here." >"$CARD/Saves/savestates/README.txt"
printf '%s\n' "pocket-ui writes its log files here (boot.log tells why the card does not start)." >"$CARD/pocket-ui/logs/README.txt"
cat >"$CARD/pocket-ui/README.txt" <<'TXT'
app/              pocket-ui itself. An update replaces this folder as a whole; app.previous/ is the version before, kept for the way back.
storage/          your settings, the Tailscale login, the Remote access password (made at the first start).
logs/             what pocket-ui writes down (boot.log first).
authorized_keys   SSH keys that may log in as root when Remote access is on, one per line, as in ~/.ssh/authorized_keys (empty at first).
TXT
# Keep this folder name free of NextUI's: a leftover .system or MinUI file would mean the wrong folder was copied over.
[ -e "$CARD/.system" ] && die ".system is in the card folder"
# BRICK_SOURCES_APART=1 (the public release): the cores' and pocket-ui's source go into their own zip next to the card zip, on the same
# release page, instead of into the card zip (half its size; every player downloads the card, few the source).
# BRICK_POCKETUI_SOURCE: pocket-ui's own source tarball (scripts/source-archive.sh), put into that zip.
ZIP=$OUT/pocket-ui-brick-card-$VERSION.zip
SRCZIP=$OUT/pocket-ui-brick-sources-$VERSION.zip
rm -f "$ZIP" "$SRCZIP"
if [ "${BRICK_SOURCES_APART:-0}" = 1 ]; then
  SRC=$OUT/sources-apart; rm -rf "$SRC"; mkdir -p "$SRC/sources"
  mv "$CARD/pocket-ui/sources/cores" "$SRC/sources/cores"; rmdir "$CARD/pocket-ui/sources"
  cp -R "$P/sources/network" "$SRC/sources/network"
  [ -z "${BRICK_POCKETUI_SOURCE:-}" ] || { [ -s "$BRICK_POCKETUI_SOURCE" ] || die "no pocket-ui source at $BRICK_POCKETUI_SOURCE"; cp "$BRICK_POCKETUI_SOURCE" "$SRC/sources/"; }
  cat >"$P/licences/SOURCES.txt" <<TXT
The source of the cores on this card, and of pocket-ui itself, is in pocket-ui-brick-sources-$VERSION.zip, published next to
pocket-ui-brick-card-$VERSION.zip on the same release page (https://github.com/robbyczgw-cla/pocket-ui/releases, tag of this version).
Each core's exact commit: CORES-LICENCES.txt. The network parts' source is on this card too: pocket-ui/app/sources/network/.
TXT
else
  cat >"$P/licences/SOURCES.txt" <<TXT
The source of the cores on this card: pocket-ui/sources/cores/ on a card made from the zip (one archive per core and commit), and
sources/ in the update channel. Each core's exact commit: CORES-LICENCES.txt. The network parts' source: pocket-ui/app/sources/network/.
TXT
fi
python3 -I - "$CARD" "$ZIP" <<'PY'
import os, sys, zipfile
card, out = sys.argv[1:3]
# Folders are real entries (name ends in "/", directory bit set), empty or not, so they survive unpacking on any system.
entries = []
for dp, dns, fns in os.walk(card):
    dns.sort()
    for d in dns: entries.append((os.path.relpath(os.path.join(dp, d), card) + "/", None))
    for f in sorted(fns): entries.append((os.path.relpath(os.path.join(dp, f), card), os.path.join(dp, f)))
with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED, compresslevel=9) as z:
    for rel, full in entries:
        zi = zipfile.ZipInfo(rel, (2026, 1, 1, 0, 0, 0))
        if full is None:
            zi.external_attr = (0o40755 << 16) | 0x10
            z.writestr(zi, b"")
            continue
        mode = 0o755 if rel.startswith((".tmp_update", "trimui", "pocket-ui/app/bin", "pocket-ui/app/hwroot", "pocket-ui/start.sh", "pocket-ui/app/start.sh")) else 0o644
        zi.external_attr = (0o100000 | mode) << 16
        zi.compress_type = zipfile.ZIP_DEFLATED
        z.writestr(zi, open(full, "rb").read())
PY
if [ "${BRICK_SOURCES_APART:-0}" = 1 ]; then
  (cd "$OUT/sources-apart" && python3 -I -c '
import os, sys, zipfile
with zipfile.ZipFile(sys.argv[1], "w", zipfile.ZIP_STORED) as z:
    for dp, dns, fns in os.walk("sources"):
        dns.sort()
        for f in sorted(fns):
            zi = zipfile.ZipInfo(os.path.join(dp, f), (2026, 1, 1, 0, 0, 0)); zi.external_attr = 0o644 << 16
            z.writestr(zi, open(os.path.join(dp, f), "rb").read())' "$SRCZIP")
  echo "brick-make-card: sources apart: $SRCZIP ($(du -h "$SRCZIP" | cut -f1))"
fi
echo "brick-make-card: $CARD ($(du -sh "$CARD" | cut -f1)), $ZIP ($(du -h "$ZIP" | cut -f1))"
