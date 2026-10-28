#!/bin/sh
# Every state of tests/i18n-states.txt at other screen sizes (the other SM8250 Retroids, docs/DEVICES.md), with
# Curator's made-up library, and what the painter noticed there:
#   OUT/<size>-<look>-<text>-<lang>.problems   one line per state and finding: the text-box record (O: runs into
#       another string or the hint bar, T: touches the string beside it, E: cut by the edge; tests/text_size.rs)
#       and the i18n record's C (cut short with "…") and X (drawn past the right edge)
#   OUT/<size>-<look>-<text>-<lang>.md5        the pictures' checksums, one line per state (for "the Mini V2's pictures
#       didn't change": run once before and once after a change at 1240x1080 and compare the two files)
#   OUT/<size>-<look>-<text>-<lang>/*.png       only with KEEP=1 (the pictures, for contact sheets)
# Settings:
#   SIZES="1920x1080 1280x960"  LOOKS="dark light"  TEXTS="normal medium large"  LANGS=en  JOBS=4
#   STATES="home;settings" (some states, ";" between)   BIN=<pocket-ui> (default target/release/pocket-ui)
#   DEVICE=rp5 (POCKETUI_DEVICE for every snapshot; default none, the generic fallback)
#   scripts/size-sweep.sh [OUT]   (default /tmp/pocketui-size-sweep; nothing of it belongs in git)
# On Linux without a Wayland compositor it starts scripts/wayland-stub.py.
set -eu
ROOT=$(cd "$(dirname "$0")/.." && pwd)
OUT=${1:-/tmp/pocketui-size-sweep}
SIZES=${SIZES:-1920x1080 1280x960}
LOOKS=${LOOKS:-dark light}
TEXTS=${TEXTS:-normal medium large}
LANGS=${LANGS:-en}
JOBS=${JOBS:-4}
KEEP=${KEEP:-0}
DEVICE=${DEVICE:-}
BIN=${BIN:-$ROOT/target/release/pocket-ui}
WORK=$(mktemp -d "${TMPDIR:-/tmp}/pocketui-size-sweep.XXXXXX")
STUB_PID=
cleanup() {
  [ -n "$STUB_PID" ] && kill "$STUB_PID" 2>/dev/null || true
  rm -rf "$WORK"
}
trap cleanup EXIT INT TERM
mkdir -p "$OUT"
if [ "$(uname)" = Linux ] && [ -z "${WAYLAND_DISPLAY:-}" ]; then
  python3 -I "$ROOT/scripts/wayland-stub.py" "$WORK/wayland" &
  STUB_PID=$!
  while [ ! -S "$WORK/wayland" ]; do sleep 0.05; done
  export WAYLAND_DISPLAY="$WORK/wayland"
fi
if [ -n "${STATES:-}" ]; then
  printf '%s\n' "$STATES" | tr ';' '\n' >"$WORK/states"
else
  grep -v '^#' "$ROOT/tests/i18n-states.txt" | grep -v '^ *$' >"$WORK/states"
fi

# One snapshot: <dir> <look> <size> <text> <lang> <state line>
cat >"$WORK/one.sh" <<'EOF'
#!/bin/sh
dir=$1 look=$2 size=$3 text=$4 lang=$5; shift 5; line=$*
safe=; state=$line
case "$line" in "safe "*) safe=--safe; state=${line#safe } ;; esac
name=$(printf '%s' "$line" | tr ':/ ,=()' '-------')
data="$WORK/d/$(basename "$dir")-$name"
rm -rf "$data" && mkdir -p "$WORK/d" && cp -R "$WORK/lib-$look/data" "$data"
if [ -n "$DEVICE" ]; then export POCKETUI_DEVICE="$DEVICE"; fi
# shellcheck disable=SC2086
POCKETUI_MOCK=1 POCKETUI_LANG=$lang POCKETUI_TEXT_SIZE=$text POCKETUI_SNAPSHOT_SIZE=$size \
  POCKETUI_TEXT_BOXES="$dir/$name.box" POCKETUI_I18N_MISSES="$dir/$name.tsv" \
  POCKETUI_ES_SYSTEMS="$WORK/lib-$look/es_systems.cfg" POCKETUI_DATA="$data" \
  "$BIN" $safe --snapshot "$state" "$dir/$name.png" >/dev/null 2>&1 || echo "failed: $line" >"$dir/$name.failed"
rm -rf "$data"
EOF
export WORK BIN DEVICE
for look in $LOOKS; do
  python3 -I "$ROOT/scripts/curator-fake-library.py" "$WORK/lib-$look" "$look" >/dev/null
  for size in $SIZES; do
    for text in $TEXTS; do
      for lang in $LANGS; do
        tag="$size-$look-$text-$lang"
        dir="$WORK/$tag"
        mkdir -p "$dir"
        tr '\n' '\0' <"$WORK/states" | xargs -0 -P "$JOBS" -I{} sh "$WORK/one.sh" "$dir" "$look" "$size" "$text" "$lang" {}
        : >"$OUT/$tag.problems"
        : >"$OUT/$tag.md5"
        while IFS= read -r line; do
          name=$(printf '%s' "$line" | tr ':/ ,=()' '-------')
          if [ -f "$dir/$name.failed" ]; then
            printf '%s\tfailed\n' "$line" >>"$OUT/$tag.problems"
            continue
          fi
          [ -f "$dir/$name.png" ] && printf '%s\t%s\n' "$(md5sum <"$dir/$name.png" | cut -d' ' -f1)" "$line" >>"$OUT/$tag.md5"
          { cat "$dir/$name.box" 2>/dev/null; grep '^[CX]	' "$dir/$name.tsv" 2>/dev/null; } | sed "s|^|$line	|" >>"$OUT/$tag.problems" || true
        done <"$WORK/states"
        if [ "$KEEP" = 1 ]; then
          rm -rf "${OUT:?}/$tag" && mkdir -p "$OUT/$tag" && mv "$dir"/*.png "$OUT/$tag/" 2>/dev/null || true
        fi
        rm -rf "$dir"
        echo "$tag: $(wc -l <"$OUT/$tag.md5" | tr -d ' ') pictures, $(wc -l <"$OUT/$tag.problems" | tr -d ' ') findings"
      done
    done
  done
done
