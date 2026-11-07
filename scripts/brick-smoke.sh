#!/bin/bash
# Smoke test of an installed pocket-ui card on the real TrimUI Brick, from the dev machine over ssh (docs/TRIMUI-BRICK-SMOKE.md). It finds what host tests
# cannot: the lights lit in the dark, picture options without effect, a lost route, a clock too low in a game.
#   scripts/brick-smoke.sh [--owner-ok] [--host root@<brick>] [--expect-version V] [--rom FILE | --device-rom PATH-ON-THE-CARD] [--system gb] [--log-dir DIR]
# Without --owner-ok only the checks that leave the screen alone run (1 version, 2 services + route, 3 clock in the menu, 4 lights, 7 owner's files).
# With --owner-ok (the owner is NOT using the Brick) it also goes dark and wakes (5) and plays a free test game for ~3 minutes (6).
# Holds /tmp/brick-device.lock for the whole run (10 minutes at most). Sound is not touched. No reboot, no suspend, no pkill.
# Exit 0 only when no check failed (SKIP does not fail). Output: a table and a log file in --log-dir (default ~/brick-smoke-logs).
# --local-root DIR runs the device half against a fake tree (scripts/test-brick-smoke.sh).
set -u
HERE=$(cd "$(dirname "$0")" && pwd)
HOST=${BRICK_HOST:-$(cat "$HOME/.config/pocket-ui/brick-host" 2>/dev/null)}; OWNER=; EXPECT=; SYSTEM=gb; LOGDIR=$HOME/brick-smoke-logs; LOCALROOT=
ROM=${BRICK_SMOKE_ROM:-$HOME/.config/pocket-ui/brick-smoke.gb}   # a free homebrew (e.g. Libbet and the Magic Floor, zlib), not one of the owner's games
while [ $# -gt 0 ]; do
  case $1 in
    --owner-ok) OWNER=1 ;;
    --host) HOST=$2; shift ;;
    --expect-version) EXPECT=$2; shift ;;
    --rom) ROM=$2; shift ;;
    --device-rom) DEVROM=$2; shift ;;
    --system) SYSTEM=$2; shift ;;
    --log-dir) LOGDIR=$2; shift ;;
    --local-root) LOCALROOT=$2; shift ;;
    *) echo "unknown argument $1"; exit 2 ;;
  esac
  shift
done
mkdir -p "$LOGDIR"
STAMP=$(date +%Y%m%d-%H%M%S)
LOG=$LOGDIR/smoke-$STAMP.log
W=$(mktemp -d "${TMPDIR:-/tmp}/brick-smoke.XXXXXX"); trap 'rm -rf "$W"' EXIT
OUT=$W/device.out
DEVICE_SH=$HERE/brick/smoke-device.sh
[ -n "$HOST" ] || [ -n "$LOCALROOT" ] || { echo "no Brick address: --host, BRICK_HOST or ~/.config/pocket-ui/brick-host"; exit 2; }
SSH=(ssh -o BatchMode=yes -o ConnectTimeout=10 -o ServerAliveInterval=15 "$HOST")
CARD=/mnt/SDCARD; WORK=$CARD/.pocket-ui-test/smoke
DEVROM=${DEVROM:-}
# --device-rom: a free homebrew already on the card (copied there on the card, no upload over a slow link); its system must be on the shelf.
if [ -n "$DEVROM" ]; then ROMNAME=pocketsmoke.${DEVROM##*.}; else ROMNAME=pocketsmoke.${ROM##*.}; fi

say() { echo "$*" | tee -a "$LOG"; }

if [ -z "$LOCALROOT" ]; then
  # Lock first: another agent's session must not overlap (the lock is the one scripts/brick-run.sh uses).
  exec 9>/tmp/brick-device.lock
  flock -w 600 9 || { echo "the Brick is locked by another session"; exit 3; }
  # One owner-ok run takes the screen: the caller has made sure nobody plays. Remind of the sound: the test game runs for seconds.
  [ -z "$OWNER" ] || say "owner-ok: the screen goes dark for ~25 s and a test game plays ~3 minutes (sound: the mute slider/volume as it is; keep the Brick muted)"
  timeout 20 "${SSH[@]}" "mkdir -p $WORK/rom && cat >/tmp/smoke-device.sh" <"$DEVICE_SH" || { echo "cannot reach $HOST"; exit 3; }
  if [ -n "$OWNER" ] && [ -n "$DEVROM" ]; then
    timeout 120 "${SSH[@]}" "rm -f $WORK/rom/pocketsmoke.*; cp '$DEVROM' $WORK/rom/$ROMNAME" || { echo "cannot copy $DEVROM on the card"; exit 2; }
  elif [ -n "$OWNER" ] && [ "$(stat -c %s "$ROM" 2>/dev/null || echo 0)" -lt 16384 ]; then echo "no real test ROM at $ROM (under 16 KB): --rom FILE, BRICK_SMOKE_ROM or --device-rom"; exit 2
  elif [ -n "$OWNER" ] && [ -f "$ROM" ]; then
    timeout 60 "${SSH[@]}" "cat >$WORK/rom/$ROMNAME" <"$ROM" || say "rom upload failed"
  fi
  args=(); [ -z "$OWNER" ] || args+=(--owner-ok --rom "$WORK/rom/$ROMNAME" --system "$SYSTEM"); [ -z "$EXPECT" ] || args+=(--expect-version "$EXPECT")
  timeout 560 "${SSH[@]}" "sh /tmp/smoke-device.sh ${args[*]}" </dev/null >"$OUT" 2>"$W/device.err"
  rc=$?
else
  mkdir -p "$LOCALROOT/mnt/SDCARD/.pocket-ui-test/smoke/rom"
  args=(); [ -z "$OWNER" ] || { cp "$ROM" "$LOCALROOT/mnt/SDCARD/.pocket-ui-test/smoke/rom/$ROMNAME" 2>/dev/null; args+=(--owner-ok --rom "$LOCALROOT/mnt/SDCARD/.pocket-ui-test/smoke/rom/$ROMNAME" --system "$SYSTEM"); }
  [ -z "$EXPECT" ] || args+=(--expect-version "$EXPECT")
  R=$LOCALROOT SMOKE_STEP=${SMOKE_STEP:-0} SMOKE_WINDOW=${SMOKE_WINDOW:-1} ${SMOKE_SH:-sh} "$DEVICE_SH" "${args[@]}" </dev/null >"$OUT" 2>"$W/device.err"
  rc=$?
fi
grep -q '^DONE' "$OUT" || echo "RESULT FAIL device-run \"the device script did not finish (ssh rc=$rc): $(tail -n 2 "$W/device.err" | tr '\n' ' ')\"" >>"$OUT"

# --- the pictures: fetch what the device captured and compare here (python3 on this computer, the Brick has none)
fetch() { # file -> local
  if [ -n "$LOCALROOT" ]; then cp "$1" "$2"; else timeout 60 "${SSH[@]}" "cat '$1'" </dev/null >"$2"; fi
}
declare -A CAPF CAPSTRIDE CAPY
NAMES=()
while read -r _ name file stride y; do
  CAPF[$name]=$file; CAPSTRIDE[$name]=$stride; CAPY[$name]=$y; NAMES+=("$name")
done < <(grep '^CAP ' "$OUT")
if grep -q "^CAP " "$OUT"; then
  for n in "${NAMES[@]}"; do fetch "${CAPF[$n]}" "$W/$n.raw" 2>/dev/null || rm -f "$W/$n.raw"; done
  # Kept for a look: each capture small, and its middle at 1:1 (scanlines and frames are only seen there). JPEG, never PNG.
  if command -v ffmpeg >/dev/null; then
    for n in "${NAMES[@]}"; do
      [ -s "$W/$n.raw" ] || continue
      ffmpeg -loglevel error -y -f rawvideo -pixel_format bgr0 -video_size "$(( ${CAPSTRIDE[$n]:-4096} / 4 ))x768" -i "$W/$n.raw" -vf "crop=1024:768:0:0,scale=512:-1" -q:v 5 "$LOGDIR/smoke-$STAMP-$n.jpg" 2>/dev/null
      ffmpeg -loglevel error -y -f rawvideo -pixel_format bgr0 -video_size "$(( ${CAPSTRIDE[$n]:-4096} / 4 ))x768" -i "$W/$n.raw" -vf "crop=256:128:384:320" -q:v 3 "$LOGDIR/smoke-$STAMP-$n-1to1.jpg" 2>/dev/null
    done
  fi
  [ -n "$LOCALROOT" ] || timeout 20 "${SSH[@]}" "rm -rf $WORK/cap $WORK/rom" </dev/null
  python3 -I - "$W" "${CAPSTRIDE[A]:-4096}" >>"$OUT" <<'PY'
import os, sys
w, stride = sys.argv[1], int(sys.argv[2])
W, H = 1024, 768
def load(n):
    p = os.path.join(w, n + ".raw")
    if not os.path.exists(p): return None
    b = open(p, "rb").read()
    if len(b) < stride * H: return None
    rows = []
    for y in range(H):
        rows.append(memoryview(b)[y * stride:y * stride + W * 4].cast("I"))
    return rows
def diff(a, b, x0=0, x1=W, y0=0, y1=H):
    n = 0
    for y in range(y0, y1):
        ra, rb = a[y], b[y]
        n += sum(1 for i in range(x0, x1) if (ra[i] ^ rb[i]) & 0x00FFFFFF)
    return n, (x1 - x0) * (y1 - y0)
A, A2, B, C, D = (load(n) for n in ("A", "A2", "B", "C", "D"))
def res(st, i, d): print("RESULT %s %s %s" % (st, i, d))
if A is None:
    res("FAIL", "6-picture", "no readable picture of the game from /dev/fb0")
else:
    lit = sum(1 for y in range(0, H, 4) for i in range(0, W, 4) if A[y][i] & 0x00FFFFFF)
    share = lit * 100 // ((H // 4) * (W // 4))
    res("PASS" if share >= 5 else "FAIL", "6-picture-shown", "%d %% of the sampled pixels are not black while the game runs" % share)
    na, nt = diff(A, A2) if A2 else (0, 1)
    noise = na / nt
    nc, ct = diff(A, A2, 0, 260, 0, 48) if A2 else (0, 1)
    cnoise = nc / ct
    # A game moves (Monomyth changed 26 % of its pixels between two looks), so the options are judged by what they do, not by a pixel diff:
    # Fullscreen makes the picture's area larger; Scanlines make neighbouring rows differ. A against A2 is the same measure's own noise.
    def lum(v): return ((v >> 16) & 255) + ((v >> 8) & 255) + (v & 255)
    def bbox(F):
        xs = [x for y in range(0, H, 2) for x in range(0, W, 2) if F[y][x] & 0x00FFFFFF]
        ys = [y for y in range(0, H, 2) if any(F[y][x] & 0x00FFFFFF for x in range(0, W, 2))]
        return (min(xs), min(ys), max(xs) + 2, max(ys) + 2) if xs else (0, 0, 0, 0)
    def area(b): return max(0, b[2] - b[0]) * max(0, b[3] - b[1])
    def rowdiff(F, b):
        n = t = 0
        for y in range(b[1], min(b[3], H) - 1):
            for x in range(b[0], b[2], 4):
                t += abs(lum(F[y][x]) - lum(F[y + 1][x])); n += 1
        return t / max(n, 1)
    ba = bbox(A)
    if B is None: res("FAIL", "6-option-B", "scaling Integer -> Fullscreen: no picture")
    else:
        bb = bbox(B); n2 = abs(area(bbox(A2)) - area(ba)) / max(area(ba), 1) if A2 else 0
        ok = area(bb) >= area(ba) * max(1.05, 1 + 4 * n2)
        res("PASS" if ok else "FAIL", "6-option-B", "scaling Integer -> Fullscreen: picture %dx%d -> %dx%d (area noise %.1f %%; need >=5 %% larger)" % (ba[2] - ba[0], ba[3] - ba[1], bb[2] - bb[0], bb[3] - bb[1], n2 * 100))
    # Scanlines, whatever the scene: a sharp whole-number scale repeats each game row exactly (only the borders between game rows
    # differ), Scanlines shade the rows inside one. Share of lit neighbouring row pairs that are not identical.
    def rowsplit(F, b):
        n = t = 0
        for y in range(b[1], min(b[3], H) - 1):
            ra, rb = F[y], F[y + 1]
            for x in range(b[0], b[2], 2):
                u, v = ra[x] & 0x00FFFFFF, rb[x] & 0x00FFFFFF
                if lum(u) > 40 or lum(v) > 40:
                    t += 1; n += u != v
        return n / max(t, 1), t
    if C is None: res("FAIL", "6-option-C", "effect None -> Scanlines: no picture")
    else:
        (sa, ta), (sc, tc) = rowsplit(A, ba), rowsplit(C, bbox(C))
        s2 = rowsplit(A2, ba)[0] if A2 else sa
        ok = tc > 500 and sc >= sa + max(0.25, 4 * abs(s2 - sa)) and sc >= 1.5 * sa
        res("PASS" if ok else "FAIL", "6-option-C", "effect None -> Scanlines: lit row pairs that differ %.0f %% -> %.0f %% (A against A2 %.0f %%; need +25 points)" % (sa * 100, sc * 100, s2 * 100))
    if D is None: res("FAIL", "6-over-the-game", "Over the game: no picture")
    else:
        n, t = diff(A, D, 0, 260, 0, 48); r = n / t
        ok = r >= max(0.01, 4 * cnoise)
        res("PASS" if ok else "FAIL", "6-over-the-game", "FPS line: %.1f %% of the top-left corner (260x48) changed (noise %.2f %%; need >=1 %%)" % (r * 100, cnoise * 100))
PY
fi

# --- the table
fails=0; passes=0; skips=0
{
  echo "pocket-ui Brick smoke test $STAMP  host=${LOCALROOT:+local:}$HOST  owner-ok=${OWNER:-no}"
  printf '%-5s %-22s %s\n' STATE CHECK DETAIL
  grep -E '^(RESULT|INFO) ' "$OUT" | while IFS= read -r line; do
    case $line in
      "INFO "*) printf '%-5s %-22s %s\n' info '' "${line#INFO }" ;;
      *) read -r _ st id detail <<<"$line"; printf '%-5s %-22s %s\n' "$st" "$id" "$detail" ;;
    esac
  done
} | tee -a "$LOG"
fails=$(grep -c '^RESULT FAIL ' "$OUT"); passes=$(grep -c '^RESULT PASS ' "$OUT"); skips=$(grep -c '^RESULT SKIP ' "$OUT")
say "=> $passes passed, $fails failed, $skips skipped. Log: $LOG"
cat "$OUT" >>"$LOG"
[ "$fails" = 0 ]
