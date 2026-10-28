#!/bin/sh
# Rebuilds the local test catalog of the store: one tiny package of every kind, real formats,
# catalog.json signed with the store key (docs/STORE.md).
#   scripts/make-test-catalog.sh [OUT] [VERSION]
# OUT defaults to store/test-catalog. VERSION (default 1.0) goes into every package, so a second
# run with 1.1 into a copy shows all four as updates.
# SERIAL (default: now, in Unix seconds) numbers the catalog; a device refuses one lower than the
# newest it has seen from the same URL. EXPIRES_DAYS=<n> adds "expires" n days from now; without
# it the test catalog doesn't expire.
# Needs: cargo, zig (cargo-zigbuild needs it anyway), tar, the private key in
# ~/.config/pocket-ui/store-signing.key (or STORE_KEY=<file>).
#
# Packages:
#   frames-graphite-monitor  frame pack (pocket-core --export-frame) for GBA and GBC
#   type-titillium           theme: Titillium Web as pocket-ui's type (OFL, from retroarch-assets)
#   tool-storage-report      tool: a shell script in pocket-ui's tools folder, run from the Store and Settings > Tools
#   core-testpattern         libretro core (aarch64), our own test pattern, into /storage/cores
set -eu
ROOT=$(cd "$(dirname "$0")/.." && pwd)
OUT=${1:-$ROOT/store/test-catalog}
VER=${2:-1.0}
SERIAL=${SERIAL:-$(date +%s)}
EXPIRES=""
[ -n "${EXPIRES_DAYS:-}" ] && EXPIRES=$(( $(date +%s) + EXPIRES_DAYS * 86400 ))
KEY=${STORE_KEY:-$HOME/.config/pocket-ui/store-signing.key}
SRC=$ROOT/store/test-catalog/src
BIN=$ROOT/target/release
# macOS tar would add ._ files for extended attributes.
export COPYFILE_DISABLE=1

[ -f "$KEY" ] || { echo "no signing key at $KEY" >&2; exit 1; }
command -v zig >/dev/null || { echo "zig is needed for the test core" >&2; exit 1; }
(cd "$ROOT" && cargo build --release -q --bin pocket-core --bin store-tool)

WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
mkdir -p "$OUT/packages"
rm -f "$OUT"/packages/*

# Frame pack: pocket-core's monitor-style frame (any system it has no handheld style for) placed
# at the Native GBA and GBC positions, so it visibly replaces the built-in handheld frames.
"$BIN/pocket-core" --export-frame frame 240 160 "$WORK/gba" >/dev/null
"$BIN/pocket-core" --export-frame frame 160 144 "$WORK/gbc" >/dev/null
hole() { sed -n '/"hole"/,/]/p' "$1" | tr -d ' \n' | sed 's/.*\[\(.*\)\].*/\1/'; }
PACK=$WORK/frames/graphite-monitor
mkdir -p "$PACK"
cp "$WORK/gba/frame.png" "$PACK/gba.png"
cp "$WORK/gbc/frame.png" "$PACK/gbc.png"
cat >"$PACK/pack.json" <<EOF
{
  "name": "Graphite monitor $VER",
  "author": "pocket-ui",
  "license": "MIT",
  "frames": [
    { "systems": ["gba"], "image": "gba.png", "hole": [$(hole "$WORK/gba/pack.json")] },
    { "systems": ["gb", "gbc"], "image": "gbc.png", "hole": [$(hole "$WORK/gbc/pack.json")] }
  ]
}
EOF
tar -czf "$OUT/packages/frames-graphite-monitor.tar.gz" -C "$WORK/frames" graphite-monitor

# Type: title and body faces under the names pocket-ui looks for (src/text.rs), with the licence.
mkdir -p "$WORK/type"
cp "$SRC/fonts/TitilliumWeb-Bold.ttf" "$WORK/type/title.ttf"
cp "$SRC/fonts/TitilliumWeb-Regular.ttf" "$WORK/type/body.ttf"
cp "$SRC/fonts/OFL.txt" "$WORK/type/Titillium-OFL.txt"
tar -czf "$OUT/packages/type-titillium.tar.gz" -C "$WORK/type" title.ttf body.ttf Titillium-OFL.txt

# Tool: one file, its version in what it prints.
sed "s/@VERSION@/$VER/g" "$SRC/tool/storage-report.sh" >"$OUT/packages/storage-report.sh"

# Core: built for the device's glibc like pocket-ui itself.
zig cc -target aarch64-linux-gnu.2.38 -shared -fPIC -O2 -s -DVERSION="\"$VER\"" \
  -o "$OUT/packages/testpattern_libretro.so" "$SRC/testpattern/testpattern.c"

# catalog.json: hash and size from store-tool, descriptions kept free of quotes.
entry() { # id name category kind file dest name_on_disk description [run]
  set -- "$@" ""
  h=$("$BIN/store-tool" hash "$OUT/packages/$5")
  sha=${h%% *}
  size=$(echo "$h" | awk '{ print $2 }')
  disk=""
  [ -n "$7" ] && disk=", \"name_on_disk\": \"$7\""
  run=""
  [ -n "$9" ] && run=", \"run\": \"$9\""
  printf '    { "id": "%s", "name": "%s", "category": "%s", "version": "%s",\n' "$1" "$2" "$3" "$VER"
  printf '      "description": "%s",\n' "$8"
  printf '      "size": %s, "url": "packages/%s", "sha256": "%s",\n' "$size" "$5" "$sha"
  printf '      "kind": "%s", "dest": "%s"%s%s, "author": "pocket-ui", "license": "%s" }' "$4" "$6" "$disk" "$run" "$LICENSE"
}
LICENSE=MIT
{
  echo '{'
  echo '  "version": 1,'
  echo "  \"serial\": $SERIAL,"
  [ -z "$EXPIRES" ] || echo "  \"expires\": $EXPIRES,"
  echo '  "packages": ['
  entry frames-graphite-monitor "Graphite monitor" frames tar frames-graphite-monitor.tar.gz /storage/.config/pocket-ui/frames "" \
    "A plain graphite bezel around Game Boy and Game Boy Advance games, in place of the built-in handheld frames. Shows with Screen frame set to Auto."
  echo ','
  LICENSE=OFL-1.1
  entry type-titillium "Titillium type" themes tar type-titillium.tar.gz /storage/.config/pocket-ui/fonts "" \
    "Titillium Web for every title and line of text in pocket-ui, in place of Barlow. Remove it to go back."
  echo ','
  LICENSE=MIT
  entry tool-storage-report "Storage report" tools file storage-report.sh /storage/.config/pocket-ui/tools "Storage report.sh" \
    "Shows the free space on the card and how many files your game folders hold. Also listed under Tools in Settings." \
    "Storage report.sh"
  echo ','
  entry core-testpattern "Test pattern core" cores file testpattern_libretro.so /storage/cores testpattern_libretro.so \
    "A tiny libretro core that draws colour bars and a square you move with the D-pad. Checks that cores from the store load."
  echo
  echo '  ]'
  echo '}'
} >"$OUT/catalog.json"
"$BIN/store-tool" sign "$KEY" "$OUT/catalog.json"
echo "test catalog $VER in $OUT"
