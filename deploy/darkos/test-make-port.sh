#!/bin/sh
# deploy/darkos/make-port.sh from stand-in programs: the port folder and its zip carry what a dArkOS player needs that is
# not a program, starting with the Chinese and Japanese font files in app/fonts (scripts/shipped-fonts.sh), byte for byte
# (27.11.2026: make-port.sh left them out and zh/ja fell back to English; the 0.8 kit added them after it).
#   sh deploy/darkos/test-make-port.sh
set -eu
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
# A release build's own DS core folder or channel must not stand in for the stand-ins.
unset DS_CORE_DIR UPDATE_CHANNEL
T=$(mktemp -d "${TMPDIR:-/tmp}/darkos-port-test.XXXXXX"); T=$(cd "$T" && pwd)
trap 'rm -rf "$T"' EXIT
B=$T/bins
mkdir -p "$B/cores"
for f in pocket-ui pocket-core pocket-fb; do printf '#!/bin/sh\nexit 0\n' >"$B/$f"; chmod +x "$B/$f"; done
printf "stand-in core\n" >"$B/cores/dsperate_libretro.so"
bad=0
if ! POCKETUI_VERSION=9.9.9 sh "$ROOT/deploy/darkos/make-port.sh" "$T/out" "$B" 9.9.9 >"$T/make-port.log" 2>&1; then
  echo "FAIL make-port.sh with stand-in programs:"; tail -20 "$T/make-port.log"; exit 1
fi
A=$T/out/ports/pocket-ui/app
if sh "$ROOT/scripts/shipped-fonts.sh" --check "$A"; then echo "ok   the port folder has the shipped font files in app/fonts"
else echo "FAIL the port folder lacks the shipped font files in app/fonts"; bad=1; fi
Z=$T/out/pocket-ui-darkos-9.9.9.zip
FONTS=$(sed -n 's/^FONTS="\(.*\)"$/\1/p' "$ROOT/scripts/shipped-fonts.sh")
[ -n "$FONTS" ] || { echo "FAIL no FONTS=\"…\" line in scripts/shipped-fonts.sh to check the zip against"; exit 1; }
for f in $FONTS; do
  if python3 -I - "$Z" "pocket-ui/app/fonts/$f" "$ROOT/assets/fonts/$f" <<'PY'
import sys, zipfile
with zipfile.ZipFile(sys.argv[1]) as z:
    names = set(z.namelist())
    sys.exit(0 if sys.argv[2] in names and z.read(sys.argv[2]) == open(sys.argv[3], 'rb').read() else 1)
PY
  then echo "ok   the zip carries app/fonts/$f"
  else echo "FAIL the zip lacks app/fonts/$f or it differs from assets/fonts"; bad=1; fi
done
exit $bad
