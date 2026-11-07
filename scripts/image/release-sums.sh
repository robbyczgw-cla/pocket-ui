#!/bin/sh
# The two checksum files that go next to the SD images on the release page, from the SHA256SUMS the image builds wrote
# (scripts/make-image.sh and make-image-family.sh), checked against the files before it writes anything.
#   scripts/image/release-sums.sh OUT_DIR BUILD_DIR [BUILD_DIR ...]
#     OUT_DIR/SHA256SUMS-images        the images that have booted on a unit: the Mini V2 and the RG35XX SP, RG28XX, RG35XX Plus
#     OUT_DIR/SHA256SUMS-images-more   every other image of the builds (the untested models, the DDR3 card)
# Each line is "<sha256>  <name>.img.xz" or "<name>.txt", the image first, as in the earlier releases. Exit 1 if a file is
# missing or does not match its sum. Edit TESTED when another model has really booted.
set -eu
[ $# -ge 2 ] || { sed -n '2,10p' "$0" >&2; exit 2; }
OUT=$1
shift
TESTED="rp-mini-v2 rg35xx-sp rg28xx rg35xx-plus"
mkdir -p "$OUT"
: >"$OUT/SHA256SUMS-images.new"
: >"$OUT/SHA256SUMS-images-more.new"
for d in "$@"; do
  [ -f "$d/SHA256SUMS" ] || { echo "release-sums: $d/SHA256SUMS is missing" >&2; exit 1; }
  (cd "$d" && sha256sum -c SHA256SUMS >/dev/null) || { echo "release-sums: $d: a file does not match its SHA256SUMS" >&2; exit 1; }
  # the image line, then its notes line
  for img in $(sed -n 's/^[0-9a-f]*  \(.*\)\.img\.xz$/\1/p' "$d/SHA256SUMS"); do
    model=$(echo "$img" | sed 's/^pocket-ui-.*-\(rp-mini-v2\|rg[a-z0-9-]*\)$/\1/')
    dest=more
    for t in $TESTED; do [ "$model" = "$t" ] && dest=images-first; done
    case $dest in images-first) f=$OUT/SHA256SUMS-images.new ;; *) f=$OUT/SHA256SUMS-images-more.new ;; esac
    grep "  $img\.img\.xz\$" "$d/SHA256SUMS" >>"$f"
    grep "  $img\.txt\$" "$d/SHA256SUMS" >>"$f"
  done
done
mv "$OUT/SHA256SUMS-images.new" "$OUT/SHA256SUMS-images"
mv "$OUT/SHA256SUMS-images-more.new" "$OUT/SHA256SUMS-images-more"
echo "SHA256SUMS-images: $(grep -c '\.img\.xz$' "$OUT/SHA256SUMS-images") images; SHA256SUMS-images-more: $(grep -c '\.img\.xz$' "$OUT/SHA256SUMS-images-more") images"
