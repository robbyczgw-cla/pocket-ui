#!/bin/bash
# Host test of scripts/device-check.sh (DC_LOCAL, no device, no ssh): the underrun count from the core's summary lines, the retried
# and diagnosed capture, the waits on the status files, the build identity and the check's version in the result.
# Run by scripts/test-device-check.sh; alone:  bash scripts/test-device-check-harness.sh
set -u
R=$(cd "$(dirname "$0")" && pwd); REPO=$R
T=$(mktemp -d "${TMPDIR:-/tmp}/test-odin-dc.XXXXXX"); trap 'rm -rf "$T"' EXIT
fail=0; ok() { echo "ok   $1"; }; bad() { echo "FAIL $1"; fail=1; }
mkdir -p "$T/work" "$T/out" "$T/bin" "$T/cfg/logs"
cp "$R/ui-status.sh" "$T/work/"
export DC_LOCAL=1 DC_SOURCE_ONLY=1 DC_CFG=$T/cfg DC_WORK=$T/work DC_CORE_STATUS=$T/core.status POCKETUI_STATUS=$T/ui.status DC_CAPTURE_PAUSE=0
# shellcheck disable=SC1090
. "$R/device-check.sh" --device odin --out "$T/out"
TMPD=$(mktemp -d "$T/tmpd.XXXXXX")

# ---- underruns: the core's summary, not the number of lines that mention the word
DC_LOG=$T/dreamcast-core-summary.log; GBA_LOG=$T/gba-core-summary.log
cat >"$DC_LOG" <<'LOG'
[  717.890] frames 931  fps 29.50  frame 33.90 ms ± 35.08  p99 103.86 ms  max 305.89 ms  late(>25ms) 188  core 20.48 ms/frame (max 104.1)  audio underruns 3494  over 42 s
[  717.890] audio buffered at exit 41 ms, underruns at 0.1s×29 3.0s×971
[  734.000] frames 32  fps 50.49  frame 19.81 ms ± 15.08  p99 102.42 ms  max 102.42 ms  late(>25ms) 2  core 3.92 ms/frame (max 56.4)  audio underruns 12  over 8 s
[  734.000] audio buffered at exit 45 ms, underruns at 0.6s×12
LOG
cat >"$GBA_LOG" <<'LOG'
frames 1853  fps 59.29  frame 16.87 ms ± 8.64  p99 17.27 ms  max 385.98 ms  late(>25ms) 4  core 3.46 ms/frame (max 14.4)  audio underruns 2  over 31 s
audio buffered at exit 19 ms, underruns at 4.2s×2
LOG
read -r n r l <<<"$(core_underruns <"$DC_LOG")"
[ "$n $r $l" = "3506 2 3494,12" ] && ok "Dreamcast run: 3494 + 12 = $n underruns in $r runs ($l)" || bad "dreamcast underruns: $n $r $l"
read -r n r l <<<"$(core_underruns <"$GBA_LOG")"
[ "$n $r" = "2 1" ] && ok "GBA run: $n underruns" || bad "gba underruns: $n $r"
[ "$(core_underruns </dev/null)" = "0 0 -" ] && ok "no summary at all: 0 0 -" || bad "empty log"
[ "$(printf 'frames 1 fps 60.00  audio underruns 0  over 31 s\naudio buffered at exit 19 ms\n' | core_underruns)" = "0 1 0" ] && ok "a clean run counts 0 (one summary seen)" || bad "clean run"
old=$(count_underruns <"$DC_LOG")
[ "$old" -lt 100 ] && ok "(the old line counter said $old for the same log: the fault this replaces)" || bad "old counter changed: $old"

# ---- capture: a whole picture or a retry, never a silent empty file
ppm() { printf 'P6\n4 3\n255\n'; head -c 36 /dev/zero | tr '\0' '\100'; }
ppm >"$T/good.ppm"; ppm_ok "$T/good.ppm" && ok "whole PPM accepted" || bad "whole PPM"
head -c 20 "$T/good.ppm" >"$T/cut.ppm"; ppm_ok "$T/cut.ppm" && bad "truncated PPM accepted" || ok "truncated PPM refused"
: >"$T/empty.ppm"; ppm_ok "$T/empty.ppm" && bad "empty file accepted" || ok "empty file refused"
cat >"$T/bin/grim" <<'G'
#!/bin/sh
# fails (empty, exit 1) the first $FAKE_FAILS times, counted in $FAKE_STATE
n=$(cat "$FAKE_STATE" 2>/dev/null || echo 0); echo $((n + 1)) >"$FAKE_STATE"
[ "$n" -ge "$FAKE_FAILS" ] || { echo "failed to copy output" >&2; exit 1; }
printf 'P6\n4 3\n255\n'; head -c 36 /dev/zero | tr '\0' '\100'
G
cat >"$T/bin/swaymsg" <<'G'
#!/bin/sh
# get_outputs: power false until "output X power on" was called
if [ "$1" = -t ]; then
  if [ -e "$FAKE_STATE.on" ]; then printf '[\n  {\n  "name": "DSI-1",\n  "power": true\n  }\n]\n'; else printf '[\n  {\n  "name": "DSI-1",\n  "power": false\n  }\n]\n'; fi
else touch "$FAKE_STATE.on"; echo "$*" >"$FAKE_STATE.args"; fi
G
chmod +x "$T/bin/grim" "$T/bin/swaymsg"
export FAKE_STATE=$T/fake
rm -f "$FAKE_STATE"*; FAKE_FAILS=2 PATH="$T/bin:$PATH" capture pre >/dev/null
ppm_ok "$TMPD/pre.ppm" && [ "$(cat "$FAKE_STATE")" = 3 ] && ok "capture: two refusals, third try gives the picture" || bad "capture retry ($(cat "$FAKE_STATE" 2>/dev/null))"
case $CAPNOTES in *"pre: 3 tries"*) ok "the retries are written down: $CAPNOTES" ;; *) bad "no note: $CAPNOTES" ;; esac
case $CAPNOTES in *"output was off"*) [ "$(cat "$FAKE_STATE.args")" = "output DSI-1 power on" ] && ok "an output that was off is switched on (output DSI-1 power on)" || bad "wrong power command: $(cat "$FAKE_STATE.args")" ;; *) bad "dpms off not noticed" ;; esac
rm -f "$FAKE_STATE"*; CAPNOTES=; FAKE_FAILS=99 PATH="$T/bin:$PATH" DC_CAPTURE_TRIES=3 capture post >/dev/null && bad "a capture that never worked reported success" || ok "no picture after 3 tries: reported as no picture"
case $CAPNOTES in *"GAVE UP"*"failed to copy output"*) ok "and the reason is kept: $CAPNOTES" ;; *) bad "no diagnosis: $CAPNOTES" ;; esac
[ "$(rmse pre post)" = NA ] && ok "RMSE of a missing picture stays NA (what the result calls 'no readable picture')" || bad "rmse"
rm -f "$FAKE_STATE"*; CAPNOTES=; FAKE_FAILS=0 PATH="$T/bin:$PATH" capture start >/dev/null
[ -z "$CAPNOTES" ] && ok "a first-try capture adds no note" || bad "note without a retry: $CAPNOTES"

# ---- waiting on the status files
mkdir -p "$T/p$$"; sleep 30 & live=$!   # a live pid for the status lines
TAB=$(printf '\t')
core_line() { printf 'v=1%srev=%s%spid=%s%splay=%s%squit=none\n' "$TAB" "$2" "$TAB" "$live" "$TAB" "$1" "$TAB" >"$T/core.tmp"; mv "$T/core.tmp" "$DC_CORE_STATUS"; }
core_line held 1
( sleep 0.7; core_line playing 2 ) & 
start=$SECONDS; v=$(core_wait play playing 8); rc=$?
[ $rc = 0 ] && [ "$v" = playing ] && [ $((SECONDS - start)) -le 4 ] && ok "core_wait returns as soon as play=playing ($v), not after the full timeout" || bad "core_wait rc=$rc v=$v"
core_line held 3; v=$(core_wait play playing 1); rc=$?
[ $rc = 1 ] && [ "$v" = held ] && ok "core_wait: stays held -> timeout (1), says what it saw ($v)" || bad "timeout case rc=$rc v=$v"
core_line menu-paused 4; v=$(core_wait play 'menu-paused|held' 2); rc=$?
[ $rc = 0 ] && ok "alternatives in the pattern (menu-paused|held)" || bad "alternatives rc=$rc"
rm -f "$DC_CORE_STATUS"; core_wait play playing 1 >/dev/null; [ $? = 2 ] && ok "no core status file (older build): 2, the caller keeps its sleep" || bad "missing file"
printf 'v=1%spid=999999%splay=playing\n' "$TAB" "$TAB" >"$DC_CORE_STATUS"; core_wait play playing 1 >/dev/null; [ $? = 2 ] && ok "a status whose pid is gone counts as no status" || bad "stale pid"
ui_line() { printf 'v=1%srev=1%spid=%s%splace=shelf:Home%ssleep=awake%sgame=%s\n' "$TAB" "$TAB" "$live" "$TAB" "$TAB" "$TAB" "$1" >"$T/ui.tmp"; mv "$T/ui.tmp" "$POCKETUI_STATUS"; }
ui_line starting; ( sleep 0.7; ui_line playing ) &
v=$(ui_game_wait 'playing|paused' 8); rc=$?
[ $rc = 0 ] && [ "$v" = playing ] && ok "ui_game_wait: starting -> playing ($v)" || bad "ui wait rc=$rc v=$v"
ui_line starting; v=$(ui_game_wait playing 1); rc=$?
[ $rc = 1 ] && [ "$v" = starting ] && ok "ui_game_wait: still starting -> timeout, names it ($v)" || bad "ui timeout rc=$rc v=$v"
rm -f "$POCKETUI_STATUS"; ui_game_wait playing 1 >/dev/null; [ $? = 2 ] && ok "no UI status: 2" || bad "ui missing"
CAPNOTES=; waited 1 "x never" || true; waited 0 y; waited 2 z
[ "$CAPNOTES" = "x never" ] && ok "only a timeout with a live status becomes a note" || bad "waited notes: $CAPNOTES"
kill $live 2>/dev/null

# ---- the build and the check in the result
RESULTS=(); FINAL="muted=yes;dark=yes;still=yes"; BLOCKED=0; WHY=
dev() { case $1 in *VERFILES* | *"pidof pocket-ui"*) echo "pocket-ui 0.2.0 md5 da7849dd" ;; *) echo ;; esac; }
fin_with() { DC_BUILD_COMMIT=$1 DC_BUILD_MD5=$2 fin 2>/dev/null; }
printf 'system=gba\nresult=PASS\nunderruns=3506\nwhy=\n' >"$T/g1"; RESULTS=("$T/g1")
j=$(fin_with bbce0421588c83b4d967a597c016a02c27d13b2 da7849dd5555 | tail -n 1)
python3 -I - "$j" <<'PY' && ok "JSON: commit, binary md5 verified, the check's version" || bad "JSON build fields"
import json, sys
d = json.load(open(sys.argv[1]))
assert d["build_commit"] == "bbce0421588c83b4d967a597c016a02c27d13b2", d
assert d["build"] == "bbce0421" and d["build_verified"] == "yes" and d["build_binary_md5"] == "da7849dd", d
assert d["check_version"].startswith("device-check/2026-11-09"), d
PY
case $j in *-bbce0421.json) ok "file name carries the commit, not 'unknown'" ;; *) bad "file name $j" ;; esac
txt=$(fin_with bbce0421588c83b4d967a597c016a02c27d13b2 deadbeef | head -n 1)
case $txt in *"DOES NOT MATCH"*) ok "another binary on the device than the artifact: flagged ($txt)" ;; *) bad "mismatch not flagged: $txt" ;; esac
j2=$(fin | tail -n 1); python3 -I - "$j2" <<'PY' && ok "without a caller commit: build 'unknown', verified NA (like the repo version)" || bad "plain fin"
import json, sys
d = json.load(open(sys.argv[1])); assert d["build"] == "unknown" and d["build_verified"] == "NA" and d["check_version"], d
PY
exit $fail
