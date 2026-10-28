#!/bin/sh
# Builds the package that installs pocket-ui on an existing ROCKNIX card (deploy/package/README.txt.in):
#   build/image/pocket-ui-<version>-rocknix-install.zip      the package, for the SSH way (README.txt)
#   build/image/Install pocket-ui.sh                          the same package inside one script, for roms/ports: the way
#   build/image/Uninstall pocket-ui.sh                        without SSH (deploy/package/port-installer.sh), and its counterpart
#
#   scripts/make-install-zip.sh [--version V] [--bin-dir DIR] [--out DIR]
#     --version  pocket-ui version (default: Cargo.toml); the binaries are built with POCKETUI_VERSION=V
#     --bin-dir  take pocket-ui, pocket-core and pocket-splash from DIR instead of building them (must be that version)
#     --out      output directory (default build/image, gitignored)
#
# The zip holds deploy/package/install.sh and uninstall.sh, the programs, the launcher, the essway drop-in and the
# boot screen's unit (deploy/pocket-splash.service) in
# files/ (with VERSION and SHA256SUMS, which install.sh checks), README.txt, LICENSE, NOTICE.md and licences/ (the crates'
# licence texts and the fonts' OFL, scripts/licence-files.sh; install.sh puts them in /storage/pocket-ui/licences). Every entry gets
# the commit's time stamp, so a rebuild of the same commit gives the same zip. Tests of the two scripts:
# deploy/package/test-package.sh.
set -eu
ROOT=$(cd "$(dirname "$0")/.." && pwd)
TARGET=aarch64-unknown-linux-gnu
VERSION=
BIN_DIR=
OUT=$ROOT/build/image
while [ $# -gt 0 ]; do
  case $1 in
    --version) VERSION=$2; shift ;;
    --bin-dir) BIN_DIR=$2; shift ;;
    --out) OUT=$2; shift ;;
    -h | --help) sed -n '2,16p' "$0"; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
  shift
done
[ -n "$VERSION" ] || VERSION=$(sed -n 's/^version = "\(.*\)"$/\1/p' "$ROOT/Cargo.toml" | head -1)
case $VERSION in '' | *[!A-Za-z0-9.+-]*) echo "bad version: '$VERSION'" >&2; exit 2 ;; esac
die() { echo "make-install-zip: $*" >&2; exit 1; }
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
  # The version is compiled in; install.sh checks it on the device by running the program.
  grep -aq "$VERSION" "$BIN_DIR/$b" || die "$BIN_DIR/$b does not carry version $VERSION"
done
sh "$ROOT/scripts/check-binary-paths.sh" "$BIN_DIR/pocket-ui" "$BIN_DIR/pocket-core" "$BIN_DIR/pocket-splash" || die "the binaries carry build paths (scripts/release-rustflags.sh)"

NAME=pocket-ui-$VERSION-rocknix-install
mkdir -p "$OUT"
OUT=$(cd "$OUT" && pwd)
S=$(mktemp -d)
trap 'rm -rf "$S"' EXIT
P=$S/$NAME
mkdir -p "$P/files"
install -m 755 "$BIN_DIR/pocket-ui" "$BIN_DIR/pocket-core" "$BIN_DIR/pocket-splash" "$ROOT/deploy/portmaster-setup.sh" "$ROOT/deploy/run-pocket-ui.sh" "$P/files/"
install -m 644 "$ROOT/deploy/essway-pocket-ui.conf" "$P/files/10-pocket-ui.conf"
install -m 644 "$ROOT/deploy/pocket-splash.service" "$P/files/pocket-splash.service"
printf '%s\n' "$VERSION" >"$P/files/VERSION"
(cd "$P/files" && sha256sum pocket-ui pocket-core pocket-splash portmaster-setup.sh run-pocket-ui.sh 10-pocket-ui.conf pocket-splash.service VERSION >SHA256SUMS)
install -m 755 "$ROOT/deploy/package/install.sh" "$ROOT/deploy/package/uninstall.sh" "$P/"
install -m 644 "$ROOT/LICENSE" "$ROOT/NOTICE.md" "$P/"
sh "$ROOT/scripts/licence-files.sh" "$P/licences" || die "the licence texts could not be collected"
chmod 644 "$P/licences/"*
NEED_MB=$(( ($(du -sk "$P/files" | cut -f1) * 2 + 10240) / 1024 ))
sed -e "s|@VERSION@|$VERSION|g" -e "s|@REPO_COMMIT@|$REPO_COMMIT|g" -e "s|@TESTED_ROCKNIX@|$TESTED|g" \
  -e "s|@NEED_MB@|$NEED_MB|g" "$ROOT/deploy/package/README.txt.in" >"$P/README.txt"
! grep -n '@[A-Z_]*@' "$P/README.txt" || die "placeholder left in README.txt"

# Python's zipfile: fixed order, time stamps and modes (no zip tool needed).
rm -f "$OUT/$NAME.zip"
python3 -I - "$S" "$NAME" "$EPOCH" "$OUT/$NAME.zip" <<'EOF'
import os, sys, time, zipfile
base, name, epoch, out = sys.argv[1], sys.argv[2], int(sys.argv[3]), sys.argv[4]
stamp = time.gmtime(max(epoch, 315532800))[:6]
with zipfile.ZipFile(out, 'w', zipfile.ZIP_DEFLATED, compresslevel=9) as z:
    for d, dirs, files in sorted(os.walk(os.path.join(base, name))):
        dirs.sort()
        rel = os.path.relpath(d, base)
        info = zipfile.ZipInfo(rel + '/', stamp)
        info.external_attr = (0o40755 << 16) | 0x10
        z.writestr(info, b'')
        for f in sorted(files):
            p = os.path.join(d, f)
            info = zipfile.ZipInfo(os.path.join(rel, f), stamp)
            info.external_attr = (0o100000 | (os.stat(p).st_mode & 0o777)) << 16
            info.compress_type = zipfile.ZIP_DEFLATED
            info.create_system = 3
            z.writestr(info, open(p, 'rb').read())
EOF

# The port installer: deploy/package/port-installer.sh with the package (the same tree as in the zip) as a tar.gz after
# it, PAYLOAD_BYTES long. Fixed order, owner and time stamps like the zip.
tar --sort=name --owner=0 --group=0 --numeric-owner --mtime="@$EPOCH" -C "$S" -cf - "$NAME" | gzip -9n >"$S/payload.tgz"
PAYLOAD_BYTES=$(wc -c <"$S/payload.tgz" | tr -d ' ')
sed "1,/^PAYLOAD_BYTES=0\$/ s/^PAYLOAD_BYTES=0\$/PAYLOAD_BYTES=$PAYLOAD_BYTES/" "$ROOT/deploy/package/port-installer.sh" >"$OUT/Install pocket-ui.sh"
cat "$S/payload.tgz" >>"$OUT/Install pocket-ui.sh"
install -m 755 "$ROOT/deploy/package/port-installer.sh" "$OUT/Uninstall pocket-ui.sh"
chmod 755 "$OUT/Install pocket-ui.sh"
ls -l "$OUT/$NAME.zip" "$OUT/Install pocket-ui.sh" "$OUT/Uninstall pocket-ui.sh"
sha256sum "$OUT/$NAME.zip"
