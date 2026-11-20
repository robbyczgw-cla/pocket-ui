#!/bin/sh
# Builds the dArkOS port folder (prototype, docs/DARKOS.md):
#   deploy/darkos/make-port.sh OUT BINS [VERSION]
# BINS holds pocket-ui, pocket-core and pocket-fb, built with
#   cargo zigbuild --release --features brick --target aarch64-unknown-linux-gnu.2.38 --bin pocket-ui --bin pocket-core --bin pocket-fb
# OUT/ports/ is what goes into /roms/ports/ on the card: pocket-ui.sh and the folder pocket-ui/. OUT/pocket-ui-darkos-<version>.zip
# holds the same, for a tester.
# UPDATE_CHANNEL=darkos-test (a beta between releases): the zip carries pocket-ui/update-url with the beta channel, which stays outside app/
# and so keeps the card on betas; a release zip has none, so the card follows the releases' channel (updates/darkos/).
set -eu
OUT=$1; BINS=$2; VERSION=${3:-proto}
HERE=$(cd "$(dirname "$0")" && pwd); REPO=$(cd "$HERE/../.." && pwd)
D=$OUT/ports/pocket-ui
P=$D/app
for f in pocket-ui pocket-core pocket-fb; do [ -x "$BINS/$f" ] || { echo "missing $BINS/$f" >&2; exit 1; }; done
rm -rf "$OUT/ports"; mkdir -p "$P/bin" "$P/port-data/hwroot/usr/bin"
cp "$HERE/pocket-ui.sh" "$OUT/ports/pocket-ui.sh"
cp "$REPO/deploy/brick/boot.sh" "$D/start.sh"
cp "$HERE/start.sh" "$P/start.sh"
cp "$HERE/systems.py" "$HERE/host-launch.py" "$P/port-data/"
for f in pocket-ui pocket-core pocket-fb; do cp "$BINS/$f" "$P/bin/$f"; done
if [ -n "${DS_CORE_DIR:-}" ]; then
  python3 -I "$REPO/scripts/dsperate-files.py" "$DS_CORE_DIR" "$P/bin/cores"
elif [ -f "$BINS/cores/dsperate_libretro.so" ]; then
  cp -R "$BINS/cores" "$P/bin/cores"
fi
if LC_ALL=C grep -a -q dsperate "$BINS/pocket-ui"; then
  python3 -I "$REPO/scripts/dsperate-files.py" --check "$P/bin/cores"
fi
cp "$HERE"/hwroot/usr/bin/* "$P/port-data/hwroot/usr/bin/"
# The screenshot stand-in asks pocket-fb for the picture (the same on the Brick's card).
sed 's|/../../../bin/pocket-fb|/../../../../bin/pocket-fb|' "$REPO/deploy/brick/hwroot/usr/bin/grim" >"$P/port-data/hwroot/usr/bin/grim"
cp "$REPO/LICENSE" "$P/LICENSE.txt"
sh "$REPO/scripts/licence-files.sh" "$P/port-data/licences/pocket-ui"
chmod +x "$OUT/ports/pocket-ui.sh" "$D/start.sh" "$P/start.sh" "$P"/bin/pocket-* "$P"/port-data/hwroot/usr/bin/*
echo "$VERSION" >"$P/VERSION"
# The app version is supplied by the release build; never try an ARM64 binary on the build host.
printf '%s\n' "${POCKETUI_VERSION:-0.4.0-darkos-$VERSION}" >"$P/RELEASE"
printf '1\n' >"$P/DARKOS-PORT"
case ${UPDATE_CHANNEL:-darkos} in
  darkos) ;;
  darkos-test) printf 'https://pocketui.xyz/updates/darkos-test/update.json\n' >"$D/update-url" ;;
  *) echo "UPDATE_CHANNEL must be darkos or darkos-test" >&2; exit 1 ;;
esac
(cd "$OUT/ports" && rm -f "../pocket-ui-darkos-$VERSION.zip" && python3 -I -m zipfile -c "../pocket-ui-darkos-$VERSION.zip" pocket-ui.sh pocket-ui)
echo "$OUT/ports and $OUT/pocket-ui-darkos-$VERSION.zip"
