#!/bin/sh
# Every state of tests/i18n-states.txt in German, dark and light, with Curator's made-up library, as contact sheets,
# plus what the painter noticed (docs/DEVELOPING.md "Translations"):
#   missing.txt        strings painted without a German line, before keep.txt (the test sorts them)
#   cut-de-only.txt    strings `Text::fit` cut short with "…" in German but not in English, with their states
#   overflow-de-only.txt  strings drawn past the right edge in German but not in English
# The states are rendered in English too, for those two comparisons.
#   scripts/i18n-shots.sh [OUT]   (default: screenshots/i18n-de)
#   LOOKS="dark" TILE=3x3 STATES="settings;safe rescue:1" scripts/i18n-shots.sh   (some looks, smaller tiles, some states, ";" between)
# Needs ImageMagick (convert, montage). The single pictures stay in a temporary folder; only the sheets (JPEG, at most
# 1280 pixels wide) go to OUT, so a review folder holds dozens of files, not hundreds. On Linux without a Wayland
# compositor it starts scripts/wayland-stub.py.
set -eu
ROOT=$(cd "$(dirname "$0")/.." && pwd)
OUT=${1:-$ROOT/screenshots/i18n-de}
LOOKS=${LOOKS:-dark light}
TILE=${TILE:-2x2}
JOBS=${JOBS:-4}
BIN=$ROOT/target/release/pocket-ui
WORK=$(mktemp -d "${TMPDIR:-/tmp}/pocketui-i18n-shots.XXXXXX")
STUB_PID=
cleanup() {
  [ -n "$STUB_PID" ] && kill "$STUB_PID" 2>/dev/null || true
  rm -rf "$WORK"
}
trap cleanup EXIT INT TERM
mkdir -p "$OUT"
rm -f "$OUT"/sheet-*.jpg "$OUT"/*.txt
(cd "$ROOT" && cargo build --release -q --bin pocket-ui)
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

# One snapshot: <look> <lang> <state line>
cat >"$WORK/one.sh" <<'EOF'
#!/bin/sh
look=$1 lang=$2; shift 2; line=$*
safe=; state=$line
case "$line" in "safe "*) safe=--safe; state=${line#safe } ;; esac
name=$(printf '%s' "$state" | tr ':/ ,=()' '-------')
# Each snapshot gets its own copy of the data folder: some write into it (save slots), and they run side by side.
# (The same path in both languages: a page that shows it measures the same in the comparison below.)
data="$WORK/d/$look/$name"
rm -rf "$data" && mkdir -p "$WORK/d/$look" && cp -R "$WORK/lib-$look/data" "$data"
# shellcheck disable=SC2086
POCKETUI_MOCK=1 POCKETUI_LANG=$lang POCKETUI_I18N_MISSES="$WORK/$look-$lang/$name.tsv" \
  POCKETUI_ES_SYSTEMS="$WORK/lib-$look/es_systems.cfg" POCKETUI_DATA="$data" \
  "$BIN" $safe --snapshot "$state" "$WORK/$look-$lang/$name.png" >/dev/null 2>&1 || echo "failed: $state ($look, $lang)"
EOF
export WORK BIN
for look in $LOOKS; do
  python3 -I "$ROOT/scripts/curator-fake-library.py" "$WORK/lib-$look" "$look" >/dev/null
  for lang in de en; do
    mkdir -p "$WORK/$look-$lang"
    tr '\n' '\0' <"$WORK/states" | xargs -0 -P "$JOBS" -I{} sh "$WORK/one.sh" "$look" "$lang" {}
  done
  # The German pictures, labelled with their state, in the list's order.
  n=$(echo "$TILE" | awk -Fx '{print $1 * $2}')
  cols=$(echo "$TILE" | cut -dx -f1)
  tile_w=$(( (1280 - 8 * cols) / cols ))
  : >"$WORK/list"
  while IFS= read -r line; do
    state=${line#safe }
    name=$(printf '%s' "$state" | tr ':/ ,=()' '-------')
    [ -f "$WORK/$look-de/$name.png" ] && printf '%s\t%s\n' "$WORK/$look-de/$name.png" "$line" >>"$WORK/list"
  done <"$WORK/states"
  split -l "$n" -d -a 3 "$WORK/list" "$WORK/part-$look-"
  for part in "$WORK"/part-"$look"-*; do
    k=${part##*-}
    set --
    while IFS="$(printf '\t')" read -r png label; do set -- "$@" -label "$label" "$png"; done <"$part"
    montage "$@" -tile "$TILE" -geometry "${tile_w}x+4+4" -pointsize 18 -background '#303030' -fill '#e0e0e0' \
      -quality 75 "$OUT/sheet-$look-$k.jpg"
    rm "$part"
  done
done

# What the painter noticed, over every look.
cat "$WORK"/*-de/*.tsv 2>/dev/null | grep '^M' | cut -f2- | sort -u >"$OUT/missing.txt" || true
for kind in C X; do
  file=cut-de-only.txt
  [ "$kind" = X ] && file=overflow-de-only.txt
  : >"$OUT/$file"
  for look in $LOOKS; do
    for tsv in "$WORK/$look-de"/*.tsv; do
      [ -f "$tsv" ] || continue
      en="$WORK/$look-en/$(basename "$tsv")"
      grep "^$kind	" "$tsv" | cut -f2- | sort -u >"$WORK/a" || true
      { [ -f "$en" ] && grep "^$kind	" "$en" | cut -f2- | sort -u; } >"$WORK/b" || true
      comm -23 "$WORK/a" "$WORK/b" | sed "s|\$|	($look: $(basename "$tsv" .tsv))|"
    done
  done | sort >>"$OUT/$file"
done
echo "$OUT: $(ls "$OUT"/sheet-*.jpg | wc -l | tr -d ' ') sheets; untranslated as recorded (before keep.txt): $(wc -l <"$OUT/missing.txt" | tr -d ' '); cut short only in German: $(wc -l <"$OUT/cut-de-only.txt" | tr -d ' '); past the edge only in German: $(wc -l <"$OUT/overflow-de-only.txt" | tr -d ' ')"
