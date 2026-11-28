#!/bin/sh
# Builds a pocket-ui release for the update channel and adds it to <out>/update.json (signed).
#   release-app.sh <version> <out-dir> ["notes"]
# Upload everything in <out-dir> to the channel afterwards (update.json, .sig, the tarball).
set -eu
VERSION=$1
OUT=$2
NOTES=${3:-}
ROOT=$(cd "$(dirname "$0")/.." && pwd)
KEY=${POCKETUI_SIGNING_KEY:-$HOME/.config/pocket-ui/store-signing.key}
TARGET=aarch64-unknown-linux-gnu

cd "$ROOT"
. "$ROOT/scripts/release-rustflags.sh"
# ScreenScraper's developer id from this machine's file, if it has one (build.rs; never in the repository).
. "$ROOT/scripts/screenscraper-env.sh"
# The commit Report a problem names (src/report.rs); "-dirty" when the files differ from it.
COMMIT=$(git -C "$ROOT" rev-parse --short=12 HEAD 2>/dev/null || echo unknown)
[ -z "$(git -C "$ROOT" status --porcelain 2>/dev/null)" ] || COMMIT=$COMMIT-dirty
POCKETUI_VERSION=$VERSION POCKETUI_COMMIT=$COMMIT cargo zigbuild --release --target "$TARGET.2.38" --bin pocket-ui --bin pocket-core --bin pocket-splash
cargo build --release --bin store-tool
sh scripts/check-binary-paths.sh "target/$TARGET/release/pocket-ui" "target/$TARGET/release/pocket-core" "target/$TARGET/release/pocket-splash" || exit 1
mkdir -p "$OUT"
STAGE=$(mktemp -d)
# pocket-splash, the boot screen, goes along: its unit runs current/pocket-splash (docs/BOOT-SPLASH.md).
cp "target/$TARGET/release/pocket-ui" "target/$TARGET/release/pocket-core" "target/$TARGET/release/pocket-splash" deploy/portmaster-setup.sh "$STAGE/"
chmod +x "$STAGE"/*
# The Chinese font files, read from the card (src/text_fallback.rs `shipped_dir`: fonts/ next to the programs).
sh scripts/shipped-fonts.sh "$STAGE"
FILES='pocket-ui pocket-core pocket-splash portmaster-setup.sh fonts'
if LC_ALL=C grep -a -q dsperate "$STAGE/pocket-ui"; then
  [ -n "${DS_CORE_DIR:-}" ] || { echo 'release-app: DS_CORE_DIR must name the portable DSperate build' >&2; exit 1; }
  python3 -I scripts/dsperate-files.py "$DS_CORE_DIR" "$STAGE/cores"
  python3 -I scripts/dsperate-files.py --check "$STAGE/cores"
  FILES="$FILES cores"
fi
TARBALL="$OUT/pocket-ui-$VERSION-aarch64.tar.gz"
COPYFILE_DISABLE=1 tar -czf "$TARBALL" -C "$STAGE" $FILES
rm -rf "$STAGE"
target/release/store-tool update "$KEY" "$OUT/update.json" app "$VERSION" "$TARBALL" "$(basename "$TARBALL")" "$NOTES"
