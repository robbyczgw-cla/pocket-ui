#!/bin/sh
# Makes an R36S screen overlay (overlays/mipi-panel.dtbo on the RK3326 card) from the .dtb of the handheld's stock card, on a
# computer, with ROCKNIX's own scripts taken from the pinned SYSTEM (docs/SD-IMAGE-RK3326.md, "Picking the screen").
#   scripts/image/r36s-overlay-from-stock.sh [--tool importpanel|rocknix_dtbo] [--options LSi-HPi] [--base TREE.dtb]
#                                           --system SYSTEM [--wheel fdt-0.3.3-py3-none-any.whl] STOCK.dtb OUT.dtbo
#   --tool     importpanel (default): ROCKNIX's /usr/libexec/generic-dsi/importpanel.py -O dtbo, the screen only (the init
#              sequence and timings of the stock tree's panel@0, for ROCKNIX's generic-dsi driver).
#              rocknix_dtbo: ROCKNIX's rocknix_dtbo.py (the EE clones' first-start quirk): screen, sticks, keys and headphones,
#              written for the EE clone tree; on the R36S tree it may not apply (the check below says so).
#   --options  rocknix_dtbo's flags (e.g. LSi-HPi), passed on as they are.
#   --base     a tree to apply the result to (fdtoverlay, if installed): prints whether it applies, exit 3 if not.
#   --system   ROCKNIX RK3326 20261101's SYSTEM (the FAT partition's, pins-rk3326.sh RK3326_SYSTEM_MD5) or the card's repacked
#              one (RK3326_POCKET_SYSTEM_MD5): the scripts come out of it with unsquashfs and are checked by sha256.
#   --wheel    the pinned pyFDT wheel (PyPI fdt 0.3.3, Apache-2.0; the SYSTEM has it only as Python 3.14 bytecode). Without it,
#              it is downloaded once to ~/.cache/pocket-ui/pyfdt/ and checked.
# Writes only OUT.dtbo and a temporary folder. No global installs, Python runs with -I.
set -eu
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
. "$ROOT/scripts/image/pins.sh"
die() { echo "r36s-overlay-from-stock: $*" >&2; exit 2; }
[ -n "${RK3326_TAG:-}" ] || die "no RK3326 pins (scripts/image/pins-rk3326.sh)"
TOOL=importpanel OPTS= BASE= SYSTEM= WHEEL=
while [ $# -gt 2 ]; do
  case $1 in
    --tool) TOOL=$2; shift ;;
    --options) OPTS=$2; shift ;;
    --base) BASE=$2; shift ;;
    --system) SYSTEM=$2; shift ;;
    --wheel) WHEEL=$2; shift ;;
    *) die "unknown argument: $1" ;;
  esac
  shift
done
[ $# = 2 ] || { sed -n '2,4p' "$0" >&2; exit 2; }
STOCK=$1 OUT=$2
[ -f "$STOCK" ] || die "no stock tree at $STOCK"
[ -n "$SYSTEM" ] && [ -f "$SYSTEM" ] || die "--system: ROCKNIX's RK3326 SYSTEM is needed"
case $TOOL in importpanel | rocknix_dtbo) ;; *) die "--tool is importpanel or rocknix_dtbo" ;; esac
sha() { sha256sum "$1" | cut -d' ' -f1; }
sum=$(md5sum "$SYSTEM" | cut -d' ' -f1)
[ "$sum" = "$RK3326_SYSTEM_MD5" ] || [ "$sum" = "$RK3326_POCKET_SYSTEM_MD5" ] || die "$SYSTEM is not ROCKNIX $RK3326_TAG's RK3326 SYSTEM (md5 $sum)"
if [ -z "$WHEEL" ]; then
  WHEEL=$HOME/.cache/pocket-ui/pyfdt/${RK3326_PYFDT_URL##*/}
  if [ ! -f "$WHEEL" ] || [ "$(sha "$WHEEL")" != "$RK3326_PYFDT_SHA256" ]; then
    mkdir -p "$(dirname "$WHEEL")"
    curl -fsSL -A pocket-ui-image-build/0.1 -o "$WHEEL.part" "$RK3326_PYFDT_URL"
    mv "$WHEEL.part" "$WHEEL"
  fi
fi
[ "$(sha "$WHEEL")" = "$RK3326_PYFDT_SHA256" ] || die "$WHEEL is not the pinned pyFDT wheel"
T=$(mktemp -d)
trap 'rm -rf "$T"' EXIT
case $TOOL in importpanel) want=$RK3326_IMPORTPANEL_SHA256 ;; *) want=$RK3326_ROCKNIX_DTBO_SHA256 ;; esac
unsquashfs -cat "$SYSTEM" "usr/libexec/generic-dsi/$TOOL.py" >"$T/$TOOL.py" 2>/dev/null || die "no $TOOL.py in $SYSTEM"
[ "$(sha "$T/$TOOL.py")" = "$want" ] || die "$TOOL.py in SYSTEM is not the pinned one"
mkdir "$T/lib"
python3 -I -m zipfile -e "$WHEEL" "$T/lib"
[ -f "$T/lib/fdt/__init__.py" ] || die "the wheel holds no fdt package"
if [ $TOOL = importpanel ]; then set -- -O dtbo -o "$OUT" "$STOCK"; else set -- "$STOCK" ${OPTS:+"$OPTS"} -o "$OUT"; fi
python3 -I -W ignore::SyntaxWarning -c 'import runpy, sys; sys.path.insert(0, sys.argv[1]); script = sys.argv[2]; sys.argv = [script] + sys.argv[3:]; runpy.run_path(script, run_name="__main__")' \
  "$T/lib" "$T/$TOOL.py" "$@"
[ -s "$OUT" ] || die "no overlay written"
[ "$(od -An -tx1 -N4 "$OUT" | tr -d ' ')" = d00dfeed ] || die "$OUT is not a device tree blob"
echo "overlay: $OUT ($(wc -c <"$OUT") bytes, sha256 $(sha "$OUT"))"
if [ -n "$BASE" ]; then
  command -v fdtoverlay >/dev/null || { echo "fdtoverlay not installed: not applied to $BASE"; exit 0; }
  if fdtoverlay -i "$BASE" -o "$T/applied.dtb" "$OUT" 2>"$T/err"; then
    echo "applies to $(basename "$BASE")"
  else
    echo "does NOT apply to $(basename "$BASE"): $(tail -1 "$T/err")"
    exit 3
  fi
fi
