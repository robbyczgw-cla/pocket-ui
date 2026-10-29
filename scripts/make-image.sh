#!/bin/sh
# Builds the ready-to-flash SD card image for the Retroid Pocket Mini V2 (docs/SD-IMAGE.md):
#   build/image/pocket-ui-<version>-rp-mini-v2.img.xz, its notes (.txt) and SHA256SUMS.
#
#   scripts/make-image.sh [--version V] [--bin-dir DIR] [--source TEXT] [--release-url URL] [--out DIR] [--keep-work] [--no-check]
#     --version      pocket-ui version (default: Cargo.toml); the binaries are built with POCKETUI_VERSION=V
#     --bin-dir      take pocket-ui, pocket-core and pocket-splash from DIR instead of building them (must answer --version V)
#     --source       where the notes say pocket-ui's source is (default: "the pocket-ui repository, commit <HEAD>");
#                    for a public image the public repository and its tag or commit
#     --release-url  the page the source archives sit on, for the notes and SOURCE-OFFER.txt
#                    (https://github.com/<owner>/pocket-ui/releases/tag/vV; its issues page is where the offer
#                    says to ask; without it the notes name both in words and check-image.sh fails the offer)
#     --out          output directory (default build/image, gitignored)
#     --keep-work    keep the unpacked image in OUT/work
#     --no-check     skip scripts/check-image.sh at the end
#
# Starts from the official ROCKNIX 20261001 SM8250 image (downloaded once into downloads/rocknix-20261001/ of the
# main checkout, or ROCKNIX_CACHE; sha256 pinned in scripts/image/pins.sh) and only adds files: GRUB, kernel and
# device tree on the FAT partition (scripts/make-sd-efi.sh), two initramfs hooks (deploy/image/), the notes, the
# source offer and the licence texts (pocket-ui-info/), pocket-ui on STORAGE. SYSTEM is repacked without the few files
# in scripts/image/pins.sh SYSTEM_REMOVE; partition table and KERNEL stay as ROCKNIX ships them.
#
# Only writes files below OUT and the download cache. Never touches a disk: no dd to /dev, no flashing.
# Needs Docker (mtools, debugfs, fdtput and GRUB run in containers), cargo zigbuild unless --bin-dir.
set -eu
ROOT=$(cd "$(dirname "$0")/.." && pwd)
. "$ROOT/scripts/image/pins.sh"
. "$ROOT/scripts/image/tools.sh"
UA=pocket-ui-image-build/0.1
TARGET=aarch64-unknown-linux-gnu

VERSION=
BIN_DIR=
SOURCE=
RELEASE_URL=
OUT=$ROOT/build/image
KEEP=0
CHECK=1
while [ $# -gt 0 ]; do
  case $1 in
    --version) VERSION=$2; shift ;;
    --bin-dir) BIN_DIR=$2; shift ;;
    --source) SOURCE=$2; shift ;;
    --release-url) RELEASE_URL=$2; shift ;;
    --out) OUT=$2; shift ;;
    --keep-work) KEEP=1 ;;
    --no-check) CHECK=0 ;;
    -h | --help) sed -n '2,22p' "$0"; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
  shift
done
[ -n "$VERSION" ] || VERSION=$(sed -n 's/^version = "\(.*\)"$/\1/p' "$ROOT/Cargo.toml" | head -1)
case $VERSION in '' | *[!A-Za-z0-9.+-]*) echo "bad version: '$VERSION'" >&2; exit 2 ;; esac
case $OUT in /dev/* | /dev) echo "refusing to write to $OUT" >&2; exit 2 ;; esac
mkdir -p "$OUT/work"
OUT=$(cd "$OUT" && pwd)
W=$OUT/work
NAME=pocket-ui-$VERSION-rp-mini-v2
say() { printf '\n== %s\n' "$*"; }
die() { echo "make-image: $*" >&2; exit 1; }
sha() { shasum -a 256 "$1" | cut -d' ' -f1; }
TOOLS=$(tools_image)
in_tools() { docker run --rm -v "$W:/w" -v "$ROOT:/repo:ro" "$TOOLS" "$@"; }

# The repository's own commit, for the notes; "-dirty" when the files differ from it.
REPO_COMMIT=$(git -C "$ROOT" rev-parse HEAD 2>/dev/null || echo unknown)
[ -z "$(git -C "$ROOT" status --porcelain 2>/dev/null)" ] || REPO_COMMIT=$REPO_COMMIT-dirty
# Every added file gets the commit's time stamp, so a rebuild of the same commit adds the same files.
EPOCH=${SOURCE_DATE_EPOCH:-$(git -C "$ROOT" log -1 --format=%ct 2>/dev/null || echo 0)}
[ -n "$SOURCE" ] || SOURCE="the pocket-ui repository, commit $REPO_COMMIT"
# The public repository's address is not written into the tree (the export allows no owner name): it comes in here.
if [ -n "$RELEASE_URL" ]; then
  ISSUES="${RELEASE_URL%%/releases*}/issues"
else
  RELEASE_URL="the pocket-ui release page for version $VERSION"
  ISSUES="the pocket-ui repository's issues page"
fi
case $SOURCE$RELEASE_URL in *'|'*) die "--source and --release-url must not contain |" ;; esac

say "ROCKNIX $ROCKNIX_TAG ($ROCKNIX_IMG)"
if [ -z "${ROCKNIX_CACHE:-}" ]; then
  main=$(cd "$ROOT" && cd "$(git rev-parse --git-common-dir 2>/dev/null || echo .git)/.." && pwd)
  ROCKNIX_CACHE=$(cd "$main/.." && pwd)/downloads/rocknix-$ROCKNIX_TAG
  [ -d "$ROCKNIX_CACHE" ] || ROCKNIX_CACHE=$main/downloads/rocknix-$ROCKNIX_TAG
fi
mkdir -p "$ROCKNIX_CACHE"
GZ=$ROCKNIX_CACHE/$ROCKNIX_IMG.gz
if [ -f "$ROCKNIX_CACHE/$ROCKNIX_IMG" ] && [ "$(sha "$ROCKNIX_CACHE/$ROCKNIX_IMG")" = "$ROCKNIX_IMG_SHA256" ]; then
  echo "cached: $ROCKNIX_CACHE/$ROCKNIX_IMG (sha256 ok)"
  cp "$ROCKNIX_CACHE/$ROCKNIX_IMG" "$W/base.img"
else
  if [ ! -f "$GZ" ] || [ "$(sha "$GZ")" != "$ROCKNIX_GZ_SHA256" ]; then
    echo "downloading $ROCKNIX_URL"
    curl -fL --retry 3 -A "$UA" -o "$GZ.part" "$ROCKNIX_URL"
    mv "$GZ.part" "$GZ"
  fi
  [ "$(sha "$GZ")" = "$ROCKNIX_GZ_SHA256" ] || die "$GZ: sha256 $(sha "$GZ") is not the published $ROCKNIX_GZ_SHA256"
  echo "sha256 of the .img.gz ok, unpacking"
  gzip -dc "$GZ" >"$W/base.img"
fi
[ "$(sha "$W/base.img")" = "$ROCKNIX_IMG_SHA256" ] || die "unpacked image: sha256 mismatch"
in_tools sh /repo/scripts/image/build.sh extract

say "SYSTEM without $(echo $SYSTEM_REMOVE | wc -w) paths (pins.sh SYSTEM_REMOVE; takes a while: zstd level 19)"
docker run --rm -e SQUASH_CPUS="${SQUASH_CPUS:-4}" -v "$W:/w" -v "$ROOT:/repo:ro" "$TOOLS" nice -n 19 sh /repo/scripts/image/build.sh strip-system

say "boot files (scripts/make-sd-efi.sh)"
rm -rf "$W/sd-efi"
sh "$ROOT/scripts/make-sd-efi.sh" "$W/KERNEL" "$W/miniv2-noufs.dtb" "$W/sd-efi" >/dev/null
[ "$(md5 -q "$W/sd-efi/boot/pocket/Image" 2>/dev/null || md5sum "$W/sd-efi/boot/pocket/Image" | cut -d' ' -f1)" = "$IMAGE_MD5" ] ||
  die "boot/pocket/Image differs from the kernel that booted on the device"
GRUB_VERSION=$(cat "$W/sd-efi/boot/grub/grub-efi-arm64-bin.version")
echo "GRUB: $GRUB_VERSION"

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

say "notes and licences"
rm -rf "$W/info"
mkdir -p "$W/info"
GRUB_SRC=$(sed -n 's/.*(source \([^ ]*\) \([^)]*\)).*/\1 \2/p' "$W/sd-efi/boot/grub/grub-efi-arm64-bin.version")
GRUB_SOURCE_URL="https://snapshot.debian.org/package/${GRUB_SRC% *}/${GRUB_SRC#* }/"
for f in README SOURCE-OFFER; do
  sed -e "s|@VERSION@|$VERSION|g" -e "s|@ROCKNIX_TAG@|$ROCKNIX_TAG|g" -e "s|@ROCKNIX_COMMIT@|$ROCKNIX_COMMIT|g" \
    -e "s|@POCKET_UI_SOURCE@|$SOURCE|g" -e "s|@RELEASE_URL@|$RELEASE_URL|g" -e "s|@ISSUES_URL@|$ISSUES|g" -e "s|@GRUB_VERSION@|$GRUB_VERSION|g" \
    -e "s|@GRUB_DEB_VERSION@|${GRUB_SRC#* }|g" -e "s|@GRUB_SOURCE_URL@|$GRUB_SOURCE_URL|g" \
    "$ROOT/deploy/image/$f.txt.in" >"$W/info/$f.txt"
  ! grep -n '@[A-Z_]*@' "$W/info/$f.txt" || die "placeholder left in $f.txt"
done
# ROCKNIX's LICENSE.md and licenses folder at the pinned tag (they are not inside SYSTEM), checked against the commit.
RL=$ROCKNIX_CACHE/licenses-$ROCKNIX_COMMIT
if [ ! -f "$RL/LICENSE.md" ]; then
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
mkdir -p "$W/info/rocknix-licenses"
cp "$RL/LICENSE.md" "$W/info/rocknix-licenses/"
cp -r "$RL/licenses" "$W/info/rocknix-licenses/licenses"
# The firmware's terms (deploy/image/firmware-licences/README.txt says which file needs which).
cp -r "$ROOT/deploy/image/firmware-licences" "$W/info/firmware-licences"
# pocket-ui's licence (GPL-2.0-or-later) and its third-party credits go onto the card as they are in the repository.
for f in LICENSE NOTICE.md; do
  if [ -f "$ROOT/$f" ]; then
    cp "$ROOT/$f" "$W/info/$f"
  else
    echo "WARNING: $f is not in the repository yet: the card goes without it, and the image is not releasable" >&2
  fi
done
# The texts that have to travel with the programs: the crates' licences and the fonts' OFL (scripts/licence-files.sh).
sh "$ROOT/scripts/licence-files.sh" "$W/info/licences" || die "the licence texts could not be collected"
find "$W/info" -type f -exec chmod 644 {} + -o -type d -exec chmod 755 {} +

say "assembling"
cp "$W/base.img" "$W/image.img"
in_tools sh /repo/scripts/image/build.sh assemble "$VERSION" "$EPOCH"

say "compressing"
rm -f "$OUT/$NAME.img.xz"
in_tools sh -c 'xz -T0 -6 -c /w/image.img >/w/out.img.xz'
# What the containers wrote (KERNEL, the device tree, the image) back to the user running this: on Linux it is root's.
in_tools chown -R "$(id -u):$(id -g)" /w 2>/dev/null || true
mv "$W/out.img.xz" "$OUT/$NAME.img.xz"
cp "$W/info/README.txt" "$OUT/$NAME.txt"
(cd "$OUT" && shasum -a 256 "$NAME.img.xz" "$NAME.txt" >SHA256SUMS)
[ $KEEP = 1 ] || rm -rf "$W"
ls -l "$OUT/$NAME.img.xz" "$OUT/$NAME.txt" "$OUT/SHA256SUMS"
cat "$OUT/SHA256SUMS"

if [ $CHECK = 1 ]; then
  say "checking (scripts/check-image.sh)"
  sh "$ROOT/scripts/check-image.sh" "$OUT/$NAME.img.xz"
fi
