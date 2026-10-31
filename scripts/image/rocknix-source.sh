#!/bin/bash
# The complete corresponding source of the ROCKNIX system inside the SD card image (docs/SD-IMAGE.md, "Source code"):
# ROCKNIX's build tree at the pinned tag plus every source archive and git checkout its SM8250 build fetches, made
# with ROCKNIX's own tools (scripts/pkgjson + scripts/genbuildplan.py for the package list, scripts/get to fetch,
# the same as "make src-pkg" packs after a full build). This is what the written source offer promises.
#
#   scripts/image/rocknix-source.sh OUT [--device FAMILY] [--no-pack | --pack-only | --plan-only | --dry-run]
#     OUT        work and output directory (several GB; needs bash, git, curl/wget, python3, xz, split)
#     --device   the ROCKNIX device family whose build is fetched (default SM8250; H700 for the H700 images);
#                the Makefile's target of that name builds ARCH=arm and ARCH=aarch64
#     --no-pack  fetch only, do not write the tar parts
#     --plan-only  clone the tree and write packages.txt, then stop (to seed sources/ from an earlier fetch first)
#     --dry-run  print the family, the file names and the pinned tag, then stop (nothing is cloned or fetched)
#
# Output in OUT: rocknix-<tag>-distribution.tar.xz (the build tree at the tag), rocknix-<tag>-<FAMILY>-sources.tar.xz.part-*
# (the fetched sources without git history, split below GitHub's 2 GiB asset limit; join with cat), packages.txt,
# failed.txt, SHA256SUMS. Packing only (after a fetch): --pack-only.
# Fetching needs no root and no Docker; nothing is built. A failed package is listed in failed.txt and the run goes on.
set -eu
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
. "$ROOT/scripts/image/pins.sh"
[ $# -ge 1 ] || { sed -n '7,9p' "$0" >&2; exit 2; }
OUT=$1
shift
PACK=1
FETCH=1
DRY=0
PLAN=0
FAMILY=SM8250
while [ $# -gt 0 ]; do
  case $1 in
    --no-pack) PACK=0 ;;
    --pack-only) FETCH=0 ;;
    --plan-only) PLAN=1 ;;
    --dry-run) DRY=1 ;;
    --device) [ $# -ge 2 ] || { echo "--device needs a family" >&2; exit 2; }; FAMILY=$2; shift ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
  shift
done
# Only families whose images this project builds; the name goes into file names and into DEVICE= of ROCKNIX's tools.
case $FAMILY in SM8250 | H700) ;; *) echo "unsupported --device '$FAMILY' (SM8250 or H700)" >&2; exit 2 ;; esac
if [ $DRY = 1 ]; then
  echo "device=$FAMILY tag=$ROCKNIX_TAG commit=$ROCKNIX_COMMIT"
  echo "sources=rocknix-$ROCKNIX_TAG-$FAMILY-sources.tar.xz.part-NN"
  exit 0
fi
mkdir -p "$OUT"
OUT=$(cd "$OUT" && pwd)
D=$OUT/distribution

if [ ! -d "$D/.git" ]; then
  git clone -q --depth 1 --branch "$ROCKNIX_TAG" https://github.com/ROCKNIX/distribution.git "$D"
fi
got=$(git -C "$D" rev-parse HEAD)
[ "$got" = "$ROCKNIX_COMMIT" ] || { echo "tag $ROCKNIX_TAG is $got, pinned $ROCKNIX_COMMIT" >&2; exit 1; }

# The family build runs ARCH=arm (32-bit libraries) and ARCH=aarch64 (Makefile, target SM8250 / H700); both build "image".
cd "$D"
export PROJECT=ROCKNIX DEVICE=$FAMILY
for a in aarch64 arm; do
  ARCH=$a ./scripts/pkgjson 2>/dev/null | ARCH=$a python3 ./scripts/genbuildplan.py --build image --with-json "$OUT/plan-$a.json" >/dev/null
done
python3 -I - "$OUT/plan-aarch64.json" "$OUT/plan-arm.json" >"$OUT/packages.txt" <<'EOF'
import json, sys
names = set()
for f in sys.argv[1:]:
    for step in json.load(open(f)):
        names.add(step["name"].split(":")[0])
        names.update(u.split(":")[0] for u in step.get("unpacks", []))
print("\n".join(sorted(names)))
EOF
# Packages the card carries but the "image" build plan does not list because ROCKNIX builds them in its bootloader step:
# the H700's U-Boot (written 8 KiB into the card and kept in SYSTEM as usr/share/bootloader/H700_DDR*_u-boot-*.bin),
# the Arm Trusted Firmware inside it and the suspend stub linked into it.
case $FAMILY in H700) printf '%s\n' u-boot-DDR3 u-boot-DDR4 atf h700-suspend-stub >>"$OUT/packages.txt"; sort -u -o "$OUT/packages.txt" "$OUT/packages.txt" ;; esac
echo "$(wc -l <"$OUT/packages.txt") packages"
[ $PLAN = 0 ] || exit 0

: >"$OUT/failed.txt.new"
[ $FETCH = 1 ] || cp "$OUT/failed.txt" "$OUT/failed.txt.new"
[ $FETCH = 1 ] && while read -r p; do
  if ! ARCH=aarch64 ./scripts/get "$p" >>"$OUT/get.log" 2>&1 && ! ARCH=aarch64 ./scripts/get "$p" >>"$OUT/get.log" 2>&1; then
    echo "$p" >>"$OUT/failed.txt.new"
    echo "FAILED $p"
  fi
done <"$OUT/packages.txt"
mv "$OUT/failed.txt.new" "$OUT/failed.txt"
echo "fetched: $(du -sh "$D/sources" | cut -f1) in sources/, $(wc -l <"$OUT/failed.txt") failed"

[ $PACK = 1 ] || exit 0
cd "$OUT"
git -C "$D" archive --format=tar --prefix="rocknix-$ROCKNIX_TAG/" HEAD | xz -T0 -6 >"rocknix-$ROCKNIX_TAG-distribution.tar.xz"
rm -f "rocknix-$ROCKNIX_TAG-$FAMILY-sources.tar"*
# The git checkouts without their history (--exclude-vcs: .git of each checkout and its submodules; the source is the
# checked-out tree at the package's PKG_VERSION), xz-compressed, split into 1900 MiB parts.
tar -C "$D" --sort=name --owner=0 --group=0 --numeric-owner --exclude-vcs -cf - sources |
  xz -T"${XZ_THREADS:-4}" -3 |
  split -b 1900M -d -a 2 - "rocknix-$ROCKNIX_TAG-$FAMILY-sources.tar.xz.part-"
sha256sum "rocknix-$ROCKNIX_TAG-distribution.tar.xz" "rocknix-$ROCKNIX_TAG-$FAMILY-sources.tar.xz.part-"* packages.txt failed.txt >SHA256SUMS
ls -l "$OUT"
