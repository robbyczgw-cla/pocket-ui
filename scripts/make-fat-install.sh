#!/bin/sh
# Builds the "two files on the card" install for an OFFICIAL ROCKNIX card (docs/INSTALL-DUAL-BOOT.md, phase "FAT"):
#   build/image/pocket-ui-<version>-rocknix-card.zip   holding mount-storage.sh, pocket-ui-seed.tar.gz, README.txt, LICENSE, NOTICE.md
#
#   scripts/make-fat-install.sh [--version V] [--bin-dir DIR] [--out DIR] [--hakux DIR]
#     --version  pocket-ui version (default: Cargo.toml); the binaries are built with POCKETUI_VERSION=V
#     --bin-dir  take pocket-ui, pocket-core and pocket-splash from DIR instead of building them (must be that version)
#     --out      output directory (default build/image, gitignored)
#     --gba-core DIR pinned ARM64 gpSP build (cores, licence and corresponding source)
#     --bsnes-hd DIR pinned ARM64 bsnes-hd build (scripts/build-bsnes-hd.sh: core, licences and corresponding source); needed when
#                pocket-ui plays Mode 7 HD with its own bsnes-hd, which it then finds beside itself in cores/ (src/mode7_hd.rs)
#     --ds-core  DIR pinned portable DSperate build (scripts/build-dsperate.sh: binary, licences, full source and recipe);
#                needed when pocket-ui uses DSperate, which it then finds beside itself in cores/ (src/ds.rs)
#     --hakux    DIR from `scripts/build-hakux-core.sh`: the seed then carries the Xbox core (release/hakux, docs/XBOX.md),
#                the same files and layout as `make-install-zip.sh --hakux`; without it Xbox needs an app update carrying hakuX
#
# The tar has the layout of the SD image's seed (scripts/image/build.sh) and of install.sh: pocket-ui/releases/<version>,
# current and the fixed links, the launcher, licences/, .config/system.d with the essway drop-in and the boot screen's
# unit, and with --hakux <release>/hakux with the core, its .info, SOURCE.txt, our patches, BUILD-INFO, licenses/ and
# SHA256SUMS (scripts/hakux-files.sh). The hook (deploy/fat/mount-storage.sh.in) carries the tar's md5, because initramfs
# has md5sum but the user copies only two files. Entries are root-owned with the commit's time stamp, so a rebuild gives the
# same files.
# The folder out/pocket-ui-<version>-rocknix-card/ with the loose files stays beside the zip (the tests use it).
# Tests: deploy/fat/test-hook.sh
set -eu
ROOT=$(cd "$(dirname "$0")/.." && pwd)
TARGET=aarch64-unknown-linux-gnu
VERSION=
BIN_DIR=
GBA_CORE_DIR=
DS_CORE_DIR=
BSNES_HD_DIR=
HAKUX_DIR=
OUT=$ROOT/build/image
while [ $# -gt 0 ]; do
  case $1 in
    --version) VERSION=$2; shift ;;
    --bin-dir) BIN_DIR=$2; shift ;;
    --out) OUT=$2; shift ;;
    --gba-core) GBA_CORE_DIR=$2; shift ;;
    --ds-core) DS_CORE_DIR=$2; shift ;;
    --bsnes-hd) BSNES_HD_DIR=$2; shift ;;
    --hakux) HAKUX_DIR=$2; shift ;;
    -h | --help) sed -n '2,17p' "$0"; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
  shift
done
[ -n "$VERSION" ] || VERSION=$(sed -n 's/^version = "\(.*\)"$/\1/p' "$ROOT/Cargo.toml" | head -1)
case $VERSION in '' | *[!A-Za-z0-9.+-]*) echo "bad version: '$VERSION'" >&2; exit 2 ;; esac
die() { echo "make-fat-install: $*" >&2; exit 1; }
for f in LICENSE NOTICE.md; do [ -f "$ROOT/$f" ] || die "$f is missing: the package is not releasable without it"; done

REPO_COMMIT=$(git -C "$ROOT" rev-parse HEAD 2>/dev/null || echo unknown)
[ -z "$(git -C "$ROOT" status --porcelain 2>/dev/null)" ] || REPO_COMMIT=$REPO_COMMIT-dirty
EPOCH=${SOURCE_DATE_EPOCH:-$(git -C "$ROOT" log -1 --format=%ct 2>/dev/null || echo 315532800)}
TESTED=$(sed -n 's/^TESTED_ROCKNIX="\(.*\)"$/\1/p' "$ROOT/deploy/package/install.sh")
[ -n "$TESTED" ] || die "no TESTED_ROCKNIX in deploy/package/install.sh"

if [ -z "$BIN_DIR" ]; then
  (cd "$ROOT" && . "$ROOT/scripts/release-rustflags.sh" && POCKETUI_VERSION=$VERSION POCKETUI_COMMIT=$REPO_COMMIT cargo zigbuild --release --target "$TARGET.2.38" --bin pocket-ui --bin pocket-core --bin pocket-splash)
  BIN_DIR=$ROOT/target/$TARGET/release
fi
for b in pocket-ui pocket-core pocket-splash; do
  file -b "$BIN_DIR/$b" | grep -q 'ELF 64-bit LSB.*ARM aarch64' || die "$BIN_DIR/$b is not an aarch64 program"
  grep -aq "$VERSION" "$BIN_DIR/$b" || die "$BIN_DIR/$b does not carry version $VERSION"
done
sh "$ROOT/scripts/check-binary-paths.sh" "$BIN_DIR/pocket-ui" "$BIN_DIR/pocket-core" "$BIN_DIR/pocket-splash" || die "the binaries carry build paths (scripts/release-rustflags.sh)"

NAME=pocket-ui-$VERSION-rocknix-card
mkdir -p "$OUT"
OUT=$(cd "$OUT" && pwd)
S=$(mktemp -d)
trap 'rm -rf "$S"' EXIT
T=$S/tree
R=$T/pocket-ui/releases/$VERSION
UNITS=$T/.config/system.d
mkdir -p "$R" "$T/pocket-ui/licences" "$UNITS/essway.service.d" "$UNITS/sysinit.target.wants"
install -m 755 "$BIN_DIR/pocket-ui" "$BIN_DIR/pocket-core" "$BIN_DIR/pocket-splash" "$ROOT/deploy/portmaster-setup.sh" "$R/"
sh "$ROOT/scripts/shipped-fonts.sh" "$R"
[ -z "$GBA_CORE_DIR" ] || sh "$ROOT/scripts/gba-core-files.sh" "$GBA_CORE_DIR" "$R/cores"
[ -z "$DS_CORE_DIR" ] || python3 -I "$ROOT/scripts/dsperate-files.py" "$DS_CORE_DIR" "$R/cores"
[ -z "$BSNES_HD_DIR" ] || sh "$ROOT/scripts/bsnes-hd-files.sh" "$BSNES_HD_DIR" "$R/cores"
# A pocket-ui with DSperate carries its verified core, licences and source (as release-channel.sh and the images do)
if LC_ALL=C grep -a -q dsperate "$R/pocket-ui"; then
  python3 -I "$ROOT/scripts/dsperate-files.py" --check "$R/cores" || die "DSperate core, source and notices required: pass --ds-core BUILD_DIR"
fi
# A pocket-ui that finds bsnes-hd beside itself (src/mode7_hd.rs BUNDLED) carries the verified core, licences and source.
if LC_ALL=C grep -a -q 'cores/bsnes_hd_beta_libretro.so' "$R/pocket-ui"; then
  sh "$ROOT/scripts/bsnes-hd-files.sh" --check "$R/cores" || die "bsnes-hd core, source and licences required: pass --bsnes-hd BUILD_DIR (scripts/build-bsnes-hd.sh)"
fi
install -m 755 "$ROOT/deploy/run-pocket-ui.sh" "$T/pocket-ui/run-pocket-ui.sh"
install -m 755 "$ROOT/deploy/package/uninstall.sh" "$T/pocket-ui/uninstall.sh"
sh "$ROOT/scripts/licence-files.sh" "$S/lic" || die "the licence texts could not be collected"
cp "$S/lic/"* "$T/pocket-ui/licences/" && chmod 644 "$T/pocket-ui/licences/"*
ln -s "releases/$VERSION" "$T/pocket-ui/current"
# The app seeds hakuX from its release on the first start, like the signed app
# updater. The card hook never copies a bundle over a player's installed core.
if [ -n "$HAKUX_DIR" ]; then
  sh "$ROOT/scripts/hakux-files.sh" "$HAKUX_DIR" "$R/hakux" || die "--hakux: the core's files could not be copied"
fi
for f in pocket-ui pocket-core portmaster-setup.sh; do ln -s "current/$f" "$T/pocket-ui/$f"; done
install -m 644 "$ROOT/deploy/essway-pocket-ui.conf" "$UNITS/essway.service.d/10-pocket-ui.conf"
install -m 644 "$ROOT/deploy/pocket-splash.service" "$UNITS/pocket-splash.service"
ln -s ../pocket-splash.service "$UNITS/sysinit.target.wants/pocket-splash.service"

P=$S/$NAME
mkdir -p "$P"
# BusyBox's tar in the initramfs reads GNU format; sorted, root-owned, fixed time: the same tar for the same commit.
tar --format=gnu --sort=name --owner=0 --group=0 --numeric-owner --mtime="@$EPOCH" -C "$T" -cf - . | gzip -n -9 >"$P/pocket-ui-seed.tar.gz"
SEED_MD5=$(md5sum "$P/pocket-ui-seed.tar.gz" | cut -d' ' -f1)
NEED_KB=$(( $(du -sk "$T" | cut -f1) * 3 + 51200 ))
sed -e "s|@VERSION@|$VERSION|g" -e "s|@SEED_MD5@|$SEED_MD5|g" -e "s|@NEED_KB@|$NEED_KB|g" \
  "$ROOT/deploy/fat/mount-storage.sh.in" >"$P/mount-storage.sh"
! grep -n '@[A-Z_]*@' "$P/mount-storage.sh" || die "placeholder left in mount-storage.sh"
sed -e "s|@VERSION@|$VERSION|g" -e "s|@REPO_COMMIT@|$REPO_COMMIT|g" -e "s|@TESTED_ROCKNIX@|$TESTED|g" \
  -e "s|@NEED_MB@|$((NEED_KB / 1024))|g" "$ROOT/deploy/fat/README.txt.in" >"$P/README.txt"
! grep -n '@[A-Z_]*@' "$P/README.txt" || die "placeholder left in README.txt"
install -m 644 "$ROOT/LICENSE" "$ROOT/NOTICE.md" "$P/"
chmod 644 "$P/mount-storage.sh" "$P/pocket-ui-seed.tar.gz" "$P/README.txt"

rm -rf "$OUT/$NAME" "$OUT/$NAME.zip"
cp -r "$P" "$OUT/$NAME"
python3 -I - "$S" "$NAME" "$EPOCH" "$OUT/$NAME.zip" <<'PY'
import os, sys, time, zipfile
base, name, epoch, out = sys.argv[1], sys.argv[2], int(sys.argv[3]), sys.argv[4]
stamp = time.gmtime(max(epoch, 315532800))[:6]
with zipfile.ZipFile(out, 'w', zipfile.ZIP_DEFLATED, compresslevel=9) as z:
    for f in sorted(os.listdir(os.path.join(base, name))):
        p = os.path.join(base, name, f)
        info = zipfile.ZipInfo(f, stamp)
        info.external_attr = (0o100644) << 16
        info.compress_type = zipfile.ZIP_DEFLATED
        info.create_system = 3
        z.writestr(info, open(p, 'rb').read())
PY
ls -l "$OUT/$NAME.zip"
sha256sum "$OUT/$NAME.zip"
