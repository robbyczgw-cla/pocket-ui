#!/bin/sh
# Builds the ready-to-flash SD card images of a family that boots ROCKNIX's own way (docs/SD-IMAGE.md, "H700" and
# "SM8550"), one per model: build/image-<family>/pocket-ui-<version>-<model>.img.xz, the notes (.txt) and SHA256SUMS.
# A new family is a block of pins in scripts/image/pins.sh, its notes (deploy/image/README-<family>.txt.in,
# SOURCE-OFFER-<family>.txt.in, firmware-licences-<family>/) and its boot checks in scripts/image/check-h700.sh.
#
#   scripts/make-image-family.sh --family H700|SM8550 [--version V] [--bin-dir DIR] [--models "m1 m2"] [--source TEXT]
#                              [--release-url URL] [--out DIR] [--keep-work] [--no-check]
#     --family       the block of pins (pins.sh <FAMILY>_*)
#     --version      pocket-ui version (default: Cargo.toml); the binaries are built with POCKETUI_VERSION=V
#     --bin-dir      take pocket-ui, pocket-core and pocket-splash from DIR instead of building them (must answer --version V)
#     --models       image names from pins.sh <FAMILY>_MODELS (default: all of them)
#     --source, --release-url   as for scripts/make-image.sh
#     --out          output directory (default build/image-<family>, gitignored)
#     --keep-work    keep the unpacked images in OUT/work
#     --no-check     skip scripts/check-image-family.sh at the end
#
# Starts from the family's official ROCKNIX 20261001 image (downloaded once into downloads/rocknix-20261001-<family>/ of
# the main checkout, or ROCKNIX_CACHE; sha256 pinned in scripts/image/pins.sh) and only adds files: two initramfs hooks
# (deploy/image/), the notes, the source offer and the licence texts (pocket-ui-info/), pocket-ui on STORAGE, and on the
# H700 dtb.img (the model's device tree). SYSTEM is repacked without the few files in pins.sh <FAMILY>_SYSTEM_REMOVE; the
# partition table, KERNEL and what boots the card (the H700's U-Boot 8 KiB in) stay as shipped.
#
# Only writes files below OUT and the download cache. Never touches a disk: no dd to /dev, no flashing.
# Needs Docker (mtools, debugfs and squashfs-tools run in a container), cargo zigbuild unless --bin-dir.
set -eu
ROOT=$(cd "$(dirname "$0")/.." && pwd)
. "$ROOT/scripts/image/pins.sh"
. "$ROOT/scripts/image/tools.sh"
UA=pocket-ui-image-build/0.1
TARGET=aarch64-unknown-linux-gnu

FAMILY=
VERSION=
BIN_DIR=
SOURCE=
RELEASE_URL=
MODELS=
OUT=
KEEP=0
CHECK=1
while [ $# -gt 0 ]; do
  case $1 in
    --family) FAMILY=$2; shift ;;
    --version) VERSION=$2; shift ;;
    --bin-dir) BIN_DIR=$2; shift ;;
    --models) MODELS=$2; shift ;;
    --source) SOURCE=$2; shift ;;
    --release-url) RELEASE_URL=$2; shift ;;
    --out) OUT=$2; shift ;;
    --keep-work) KEEP=1 ;;
    --no-check) CHECK=0 ;;
    -h | --help) sed -n '2,28p' "$0"; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
  shift
done
case $FAMILY in H700 | SM8550) ;; *) echo "--family must be H700 or SM8550, not '$FAMILY'" >&2; exit 2 ;; esac
family_pins "$FAMILY"
FAM=$(echo "$FAMILY" | tr 'A-Z' 'a-z')
[ -n "$OUT" ] || OUT=$ROOT/build/image-$FAM
[ -n "$VERSION" ] || VERSION=$(sed -n 's/^version = "\(.*\)"$/\1/p' "$ROOT/Cargo.toml" | head -1)
case $VERSION in '' | *[!A-Za-z0-9.+-]*) echo "bad version: '$VERSION'" >&2; exit 2 ;; esac
case $OUT in /dev/* | /dev) echo "refusing to write to $OUT" >&2; exit 2 ;; esac
ALL_MODELS=$(echo "$FAM_MODELS" | awk 'NF { printf "%s ", $1 }')
[ -n "$MODELS" ] || MODELS=$ALL_MODELS
for m in $MODELS; do
  case " $ALL_MODELS" in *" $m "*) ;; *) echo "unknown model '$m' (${FAMILY}_MODELS: $ALL_MODELS)" >&2; exit 2 ;; esac
done
mkdir -p "$OUT/work"
OUT=$(cd "$OUT" && pwd)
W=$OUT/work
say() { printf '\n== %s\n' "$*"; }
die() { echo "make-image-family: $*" >&2; exit 1; }
sha() { shasum -a 256 "$1" | cut -d' ' -f1; }
TOOLS=$(tools_image)
in_tools() { docker run --rm -e FAMILY="$FAMILY" -e MODEL="${MODEL:-}" -v "$W:/w" -v "$ROOT:/repo:ro" "$TOOLS" "$@"; }
# model name as ROCKNIX's tree calls it, for the notes ("Anbernic RG35XX Plus")
model_name() { echo "$FAM_MODELS" | awk -v m="$1" '$1 == m { print $4 }' | tr _ ' '; }
model_dtb() { echo "$FAM_MODELS" | awk -v m="$1" '$1 == m { print $2 }'; }

REPO_COMMIT=$(git -C "$ROOT" rev-parse HEAD 2>/dev/null || echo unknown)
[ -z "$(git -C "$ROOT" status --porcelain 2>/dev/null)" ] || REPO_COMMIT=$REPO_COMMIT-dirty
EPOCH=${SOURCE_DATE_EPOCH:-$(git -C "$ROOT" log -1 --format=%ct 2>/dev/null || echo 0)}
[ -n "$SOURCE" ] || SOURCE="the pocket-ui repository, commit $REPO_COMMIT"
if [ -n "$RELEASE_URL" ]; then
  ISSUES="${RELEASE_URL%%/releases*}/issues"
else
  # The notes then name no web address, and the check refuses that at the very end of the build: fine for a card only we test.
  echo "note: no --release-url: scripts/check-image-family.sh will fail SOURCE-OFFER.txt, which must name the issues page" >&2
  RELEASE_URL="the pocket-ui release page for version $VERSION"
  ISSUES="the pocket-ui repository's issues page"
fi
case $SOURCE$RELEASE_URL in *'|'*) die "--source and --release-url must not contain |" ;; esac

say "ROCKNIX $ROCKNIX_TAG ($FAM_IMG)"
if [ -z "${ROCKNIX_CACHE:-}" ]; then
  main=$(cd "$ROOT" && cd "$(git rev-parse --git-common-dir 2>/dev/null || echo .git)/.." && pwd)
  ROCKNIX_CACHE=$(cd "$main/.." && pwd)/downloads/rocknix-$ROCKNIX_TAG-$FAM
  [ -d "$ROCKNIX_CACHE" ] || ROCKNIX_CACHE=$main/downloads/rocknix-$ROCKNIX_TAG-$FAM
fi
mkdir -p "$ROCKNIX_CACHE"
GZ=$ROCKNIX_CACHE/$FAM_IMG.gz
if [ -f "$ROCKNIX_CACHE/$FAM_IMG" ] && [ "$(sha "$ROCKNIX_CACHE/$FAM_IMG")" = "$FAM_IMG_SHA256" ]; then
  echo "cached: $ROCKNIX_CACHE/$FAM_IMG (sha256 ok)"
  cp "$ROCKNIX_CACHE/$FAM_IMG" "$W/base.img"
else
  if [ ! -f "$GZ" ] || [ "$(sha "$GZ")" != "$FAM_GZ_SHA256" ]; then
    echo "downloading $FAM_URL"
    curl -fL --retry 3 -A "$UA" -o "$GZ.part" "$FAM_URL"
    mv "$GZ.part" "$GZ"
  fi
  [ "$(sha "$GZ")" = "$FAM_GZ_SHA256" ] || die "$GZ: sha256 $(sha "$GZ") is not the published $FAM_GZ_SHA256"
  echo "sha256 of the .img.gz ok, unpacking"
  gzip -dc "$GZ" >"$W/base.img"
fi
[ "$(sha "$W/base.img")" = "$FAM_IMG_SHA256" ] || die "unpacked image: sha256 mismatch"

say "SYSTEM without $(echo $FAM_SYSTEM_REMOVE | wc -w) paths (pins.sh ${FAMILY}_SYSTEM_REMOVE; takes a while: zstd level 19)"
docker run --rm -e FAMILY="$FAMILY" -e SQUASH_CPUS="${SQUASH_CPUS:-4}" -v "$W:/w" -v "$ROOT:/repo:ro" "$TOOLS" nice -n 19 sh /repo/scripts/image/build.sh strip-system

say "pocket-ui $VERSION"
rm -rf "$W/bin"
mkdir -p "$W/bin"
if [ -n "$BIN_DIR" ]; then
  cp "$BIN_DIR/pocket-ui" "$BIN_DIR/pocket-core" "$BIN_DIR/pocket-splash" "$W/bin/" || die "$BIN_DIR needs pocket-ui, pocket-core and pocket-splash"
  echo "taken from $BIN_DIR"
else
  (cd "$ROOT" && . "$ROOT/scripts/release-rustflags.sh" && POCKETUI_VERSION=$VERSION POCKETUI_COMMIT=$REPO_COMMIT cargo zigbuild --release --target "$TARGET.2.38" --bin pocket-ui --bin pocket-core --bin pocket-splash)
  cp "$ROOT/target/$TARGET/release/pocket-ui" "$ROOT/target/$TARGET/release/pocket-core" "$ROOT/target/$TARGET/release/pocket-splash" "$W/bin/"
fi
sh "$ROOT/scripts/check-binary-paths.sh" "$W/bin/pocket-ui" "$W/bin/pocket-core" "$W/bin/pocket-splash" || die "the binaries carry build paths (scripts/release-rustflags.sh)"

# ROCKNIX's LICENSE.md and licenses folder at the pinned tag (not inside SYSTEM), checked against the commit.
RL=$ROCKNIX_CACHE/licenses-$ROCKNIX_COMMIT
[ -f "$RL/LICENSE.md" ] || RL=$(dirname "$ROCKNIX_CACHE")/rocknix-$ROCKNIX_TAG/licenses-$ROCKNIX_COMMIT
if [ ! -f "$RL/LICENSE.md" ]; then
  RL=$ROCKNIX_CACHE/licenses-$ROCKNIX_COMMIT
  rm -rf "$RL.git"
  git clone -q --depth 1 --branch "$ROCKNIX_TAG" --filter=blob:none --sparse https://github.com/ROCKNIX/distribution.git "$RL.git"
  [ "$(git -C "$RL.git" rev-parse HEAD)" = "$ROCKNIX_COMMIT" ] || die "ROCKNIX tag $ROCKNIX_TAG is not commit $ROCKNIX_COMMIT"
  git -C "$RL.git" sparse-checkout set licenses
  mkdir -p "$RL.new"
  cp "$RL.git/LICENSE.md" "$RL.new/"
  cp -r "$RL.git/licenses" "$RL.new/licenses"
  rm -rf "$RL.git" "$RL"
  mv "$RL.new" "$RL"
fi

NAMES=
for MODEL in $MODELS; do
  NAME=pocket-ui-$VERSION-$MODEL
  MODEL_NAME=$(model_name "$MODEL")
  DTB_NAME=$(model_dtb "$MODEL")
  say "$NAME ($MODEL_NAME)"
  rm -rf "$W/info"
  mkdir -p "$W/info"
  for f in README SOURCE-OFFER; do
    sed -e "s|@VERSION@|$VERSION|g" -e "s|@ROCKNIX_TAG@|$ROCKNIX_TAG|g" -e "s|@ROCKNIX_COMMIT@|$ROCKNIX_COMMIT|g" \
      -e "s|@POCKET_UI_SOURCE@|$SOURCE|g" -e "s|@RELEASE_URL@|$RELEASE_URL|g" -e "s|@ISSUES_URL@|$ISSUES|g" \
      -e "s|@OS_NAME@|$POCKET_OS_NAME|g" -e "s|@MODEL_NAME@|$MODEL_NAME|g" -e "s|@IMAGE_NAME@|$NAME|g" -e "s|@DTB_NAME@|$DTB_NAME|g" \
      "$ROOT/deploy/image/$f-$FAM.txt.in" | notes_blocks "$(has_games_drive "$FAMILY" "$MODEL" && echo 1 || echo 0)" >"$W/info/$f.txt"
    ! grep -n '@[A-Z_]*@' "$W/info/$f.txt" || die "placeholder left in $f.txt"
  done
  printf 'NAME=%s\nVERSION=%s\n' "$POCKET_OS_NAME" "$VERSION" >"$W/info/pocket-ui-os.txt"
  mkdir -p "$W/info/rocknix-licenses"
  cp "$RL/LICENSE.md" "$W/info/rocknix-licenses/"
  cp -r "$RL/licenses" "$W/info/rocknix-licenses/licenses"
  # The firmware's terms: the family's folder, plus the licence texts it shares with the SM8250 list.
  mkdir -p "$W/info/firmware-licences"
  cp "$ROOT/deploy/image/firmware-licences-$FAM/"* "$W/info/firmware-licences/"
  for f in $FAM_FW_SHARED; do cp "$ROOT/deploy/image/firmware-licences/$f" "$W/info/firmware-licences/"; done
  for f in LICENSE NOTICE.md; do
    if [ -f "$ROOT/$f" ]; then
      cp "$ROOT/$f" "$W/info/$f"
    else
      echo "WARNING: $f is not in the repository yet: the card goes without it, and the image is not releasable" >&2
    fi
  done
  sh "$ROOT/scripts/licence-files.sh" "$W/info/licences" || die "the licence texts could not be collected"
  find "$W/info" -type f -exec chmod 644 {} + -o -type d -exec chmod 755 {} +

  cp "$W/base.img" "$W/image.img"
  in_tools sh /repo/scripts/image/build.sh assemble "$VERSION" "$EPOCH"
  say "compressing $NAME"
  rm -f "$OUT/$NAME.img.xz"
  in_tools sh -c "nice -n 19 xz -T${XZ_THREADS:-4} -6 -c /w/image.img >/w/out.img.xz"
  in_tools chown -R "$(id -u):$(id -g)" /w 2>/dev/null || true
  mv "$W/out.img.xz" "$OUT/$NAME.img.xz"
  cp "$W/info/README.txt" "$OUT/$NAME.txt"
  NAMES="$NAMES $NAME"
done
rm -f "$W/image.img"
(cd "$OUT" && for n in $NAMES; do shasum -a 256 "$n.img.xz" "$n.txt"; done >SHA256SUMS)
[ $KEEP = 1 ] || rm -rf "$W"
for n in $NAMES; do ls -l "$OUT/$n.img.xz" "$OUT/$n.txt"; done
cat "$OUT/SHA256SUMS"

if [ $CHECK = 1 ]; then
  bad=0
  for n in $NAMES; do
    say "checking $n (scripts/check-image-family.sh)"
    CHECK_WORK=$OUT/check sh "$ROOT/scripts/check-image-family.sh" "$OUT/$n.img.xz" || bad=1
  done
  [ $bad = 0 ] || exit 1
fi
