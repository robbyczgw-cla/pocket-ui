#!/bin/sh
# The licence texts that travel with every copy of the programs (the install zip and the SD image):
#
#   scripts/licence-files.sh DIR
#
# DIR gets LICENSE (pocket-ui's GPL), NOTICE.md (credits), THIRD-PARTY-CRATES.md (every linked Rust crate with its
# licence), THIRD-PARTY-LICENSES.txt (the licence and notice files of those crates, from `third-party.sh --texts`) and
# FONTS-OFL.txt (the SIL Open Font License of the Barlow, Pixelify Sans and Nunito fonts compiled into the programs). Settings > About names
# where they end up on the device (/storage/pocket-ui/licences). Fails when any is missing or empty.
set -eu
ROOT=$(cd "$(dirname "$0")/.." && pwd)
[ $# -eq 1 ] || { sed -n '2,9p' "$0" | sed 's/^# \{0,1\}//'; exit 2; }
OUT=$1
die() { echo "licence-files: $*" >&2; exit 1; }
rm -rf "$OUT"
mkdir -p "$OUT"
cp "$ROOT/LICENSE" "$ROOT/NOTICE.md" "$OUT/" || die "LICENSE or NOTICE.md is missing"
cp "$ROOT/assets/fonts/OFL.txt" "$OUT/FONTS-OFL.txt" || die "assets/fonts/OFL.txt is missing"
# The script exits 1 while the four Apache-2.0 crates are flagged (an accepted decision, docs/LICENCES.md); the files
# are written all the same, so only a missing or empty file counts here.
"$ROOT/scripts/third-party.sh" --texts "$OUT/THIRD-PARTY-LICENSES.txt" --out "$OUT/THIRD-PARTY-CRATES.md" >/dev/null 2>&1 || true
for f in LICENSE NOTICE.md FONTS-OFL.txt THIRD-PARTY-CRATES.md THIRD-PARTY-LICENSES.txt; do
  [ -s "$OUT/$f" ] || die "$f was not written"
done
echo "licence-files: $(ls "$OUT" | wc -l) files in $OUT"
