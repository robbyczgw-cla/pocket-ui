#!/bin/sh
# Tests of the ROCKNIX comparison (docs/COMPARE-ROCKNIX.md), no device needed:
#   1. rocknix-compare.py selftest: the Wayland screencopy client against a fake compositor, the analysis on made-up
#      recordings, a whole run analysed and tabled
#   2. the analysis on a synthetic video (ffmpeg, when there is one): a moving corner, a box that appears and grows
#   3. scripts/rocknix-compare-device.sh in a made-up root, with a stand-in for the driver that changes what a run
#      changes: everything is put back (and what the run added is gone, the big caches untouched), the ES-only mark
#      is removed and essway restarted; also when the run is stopped with SIGTERM half way
#   --sim: also both UIs through every scenario but the wake on a simulated Pocket (rocknix-compare.py selftest --sim,
#      the real driver, recorder and analysis; about 5 minutes)
set -eu
ROOT=$(cd "$(dirname "$0")/.." && pwd)
PY=$ROOT/scripts/rocknix-compare.py
python3 -I "$PY" selftest

tmp=$(mktemp -d "${TMPDIR:-/tmp}/compare-test.XXXXXX")
trap 'rm -rf "$tmp"' EXIT
fail() { echo "test-rocknix-compare: $*" >&2; exit 1; }

# --- 2. a synthetic video --------------------------------------------------------------------------------------
if command -v ffmpeg >/dev/null 2>&1; then
  # 160x120 at 60 fps, 2 s: grey, a small square running round the top-left corner all the time (a clock), from frame
  # 75 on a box at the bottom that is 40 wide, from frame 77 on 120 wide. Encoded lossy and decoded again.
  ffmpeg -loglevel error -f lavfi -i "color=c=0x707070:s=160x120:r=60:d=2" \
    -vf "drawbox=x='mod(t*180,30)':y=2:w=6:h=6:c=white:t=fill,drawbox=x=20:y=80:w=40:h=30:c=0xE0C020:t=fill:enable='gte(n,75)',drawbox=x=60:y=80:w=80:h=30:c=0xE0C020:t=fill:enable='gte(n,77)'" \
    -c:v libx264 -crf 12 -pix_fmt yuv420p "$tmp/synth.mp4"
  python3 -I "$PY" import-video "$tmp/synth.mp4" "$tmp/synth.pfr" --start-ms 0 --step 1
  python3 -I - "$PY" "$tmp/synth.pfr" <<'EOF'
import importlib.util, sys
spec = importlib.util.spec_from_file_location("rc", sys.argv[1])
rc = importlib.util.module_from_spec(spec)
spec.loader.exec_module(rc)
rec = rc.Recording(sys.argv[2])
pics = rc.Pics(rec.head["sw"], rec.head["sh"], rec.head["format"])
press = int(1200e6)  # frame 72 is at 1200 ms
f, how = rc.react(rec, pics, press, int(1900e6))
assert f and round(f.t / 1e6 * 60 / 1000) == 75, (f and f.t / 1e6, how)
f, how = rc.done(rec, pics, press, int(1900e6))
assert f and round(f.t / 1e6 * 60 / 1000) == 77, (f and f.t / 1e6, how)
# Before the box, only the running square: no reaction.
f, how = rc.react(rec, pics, int(500e6), int(1200e6))
assert f is None, (f.t / 1e6, how)
print("test-rocknix-compare: synthetic video ok (react at frame 75, done at frame 77, the running corner ignored)")
EOF
else
  echo "test-rocknix-compare: no ffmpeg, the synthetic video is skipped"
fi

# --- 3. the device script in a made-up root --------------------------------------------------------------------
R=$tmp/root
W=$tmp/work
BIN=$tmp/bin
mkdir -p "$R/storage/.config/pocket-ui/covers" "$R/storage/.config/pocket-ui/slots/gba" "$R/storage/.config/retroarch/config/mGBA" \
  "$R/storage/.config/emulationstation" "$R/storage/.config/system/configs" "$R/storage/roms/gba" "$R/storage/roms/savestates/gba" \
  "$R/storage/pocket-ui" "$R/sys/class/backlight/panel" "$R/sys/class/rtc/rtc0" "$R/etc" "$R/tmp" "$W/scripts" "$BIN"
G="F-Zero - Maximum Velocity (USA)"
echo '{"theme":"dark"}' >"$R/storage/.config/pocket-ui/settings.json"
echo '[1,2]' >"$R/storage/.config/pocket-ui/recents.json"
echo slot >"$R/storage/.config/pocket-ui/slots/gba/x"
echo cover >"$R/storage/.config/pocket-ui/covers/a.png"
echo 'video_driver = "glcore"' >"$R/storage/.config/retroarch/retroarch.cfg"
echo 'n64 = 1' >"$R/storage/.config/retroarch/ParaLLEl N64.opt"
echo 'mgba = 1' >"$R/storage/.config/retroarch/config/mGBA/mGBA.opt"
echo '<config/>' >"$R/storage/.config/emulationstation/es_settings.cfg"
printf 'global.autosave=1\ngba.core=mgba\n' >"$R/storage/.config/system/configs/system.cfg"
echo rom >"$R/storage/roms/gba/$G.gba"
echo save >"$R/storage/roms/gba/$G.srm"
echo '<gameList/>' >"$R/storage/roms/gba/gamelist.xml"
echo state >"$R/storage/roms/savestates/gba/$G.state.auto"
echo 300 >"$R/sys/class/backlight/panel/brightness"
: >"$R/sys/class/rtc/rtc0/wakealarm"
echo 'OS_VERSION="20261001"' >"$R/etc/os-release"
cp "$ROOT/scripts/rocknix-compare-device.sh" "$W/scripts/"
cp "$ROOT/scripts/rocknix-compare-plan.json" "$W/scripts/"
# Stand-ins: systemctl, pactl, journalctl write what they were asked into calls.log; pidof finds nothing.
for c in systemctl pactl journalctl; do
  cat >"$BIN/$c" <<EOF
#!/bin/sh
echo "$c \$*" >>"$tmp/calls.log"
case "$c \$1" in "pactl get-sink-mute") echo "Mute: no" ;; "pactl get-sink-volume") echo "Volume: front-left: 42598 /  65% / -11.21 dB" ;; esac
exit 0
EOF
done
printf '#!/bin/sh\nexit 1\n' >"$BIN/pidof"
printf '#!/bin/sh\nexit 0\n' >"$BIN/sway"
# The driver's stand-in: what a run changes. SLOW=1: stops in the middle (for the SIGTERM case).
cat >"$tmp/drive" <<EOF
#!/bin/sh
R=$R
case "\$1" in
  probe) echo '{}'; echo '{"A": 305}' >"\$3"; exit 0 ;;
esac
ui=\$3
D="\$R/storage/.config/pocket-ui"
echo '{"theme":"light"}' >"\$D/settings.json"
rm -f "\$D/recents.json"
echo new >"\$D/resume.json"
echo new >"\$D/covers/b.png"
rm -rf "\$D/slots"
echo 'video_driver = "gl"' >"\$R/storage/.config/retroarch/retroarch.cfg"
echo newer >"\$R/storage/roms/gba/$G.srm"
echo s1 >"\$R/storage/roms/savestates/gba/$G.state1"
rm -f "\$R/storage/roms/savestates/gba/$G.state.auto"
echo '<gameList played="1"/>' >"\$R/storage/roms/gba/gamelist.xml"
echo 'system.x=1' >>"\$R/storage/.config/system/configs/system.cfg"
echo 0 >"\$R/sys/class/backlight/panel/brightness"
echo +15 >"\$R/sys/class/rtc/rtc0/wakealarm"
touch "\$R/tmp/pocket-ui.es-only"
[ "\$ui" = pocket-ui ] && { grep -q '^POCKETUI_SUSPEND_CMD=.*+20 ' "\$R/storage/pocket-ui/dev.env" || { echo "no suspend command in dev.env" >&2; exit 9; }; }
echo "\$ui" >>"$tmp/driven"
if [ "\${SLOW:-}" = 1 ]; then echo \$\$ >"$tmp/halfway"; sleep 30; fi
exit 0
EOF
chmod +x "$BIN"/* "$tmp/drive"
# Everything but the copies, the big cache file the run added (it stays) and the RTC alarm (checked on its own).
snapshot() { (cd "$R" && find storage sys tmp -type f ! -path 'storage/.cmpbak/*' ! -name b.png ! -name wakealarm -exec md5sum {} + | LC_ALL=C sort -k2); }
snapshot >"$tmp/before"

run_device() {
  printf 'UIS="es pocket-ui"\nDRIVE_ARGS="--scenarios start --asleep 20"\nPLAN=scripts/rocknix-compare-plan.json\n' >"$W/run.env"
  env PATH="$BIN:$PATH" COMPARE_ROOT="$R" COMPARE_WORK="$W" COMPARE_PY="$tmp/drive" sh "$W/scripts/rocknix-compare-device.sh" >"$tmp/device.log" 2>&1
}
check_restored() { # $1: the case
  snapshot >"$tmp/after"
  diff "$tmp/before" "$tmp/after" >"$tmp/diff" || { cat "$tmp/diff"; cat "$tmp/device.log"; fail "$1: not everything was put back"; }
  [ -f "$R/storage/.config/pocket-ui/covers/b.png" ] || fail "$1: a big cache was touched"
  [ ! -e "$R/storage/pocket-ui/dev.env" ] || fail "$1: dev.env was left behind"
  [ ! -e "$R/tmp/pocket-ui.es-only" ] || fail "$1: the ES-only mark was left"
  grep "^systemctl" "$tmp/calls.log" | tail -1 | grep -q '^systemctl restart essway' || { cat "$tmp/calls.log"; fail "$1: essway was not restarted last"; }
  grep -q 'pactl set-sink-mute @DEFAULT_SINK@ 0' "$tmp/calls.log" || fail "$1: the speaker was not unmuted"
  grep -q 'pactl set-sink-volume @DEFAULT_SINK@ 65%' "$tmp/calls.log" || fail "$1: the volume was not put back"
  [ -f "$W/out.tgz" ] || fail "$1: no out.tgz"
  [ "$(cat "$R/sys/class/rtc/rtc0/wakealarm")" = 0 ] || fail "$1: an RTC alarm was left"
}

run_device || true
[ "$(cat "$W/rc")" = 0 ] || { cat "$tmp/device.log"; fail "rc $(cat "$W/rc"), expected 0"; }
[ "$(cat "$tmp/driven")" = "es
pocket-ui" ] || fail "the UIs were not both driven: $(cat "$tmp/driven")"
grep -q 'pactl set-sink-mute @DEFAULT_SINK@ 1' "$tmp/calls.log" || fail "not muted during the run"
check_restored "a whole run"

# Stopped half way (systemctl stop pocket-compare sends SIGTERM to the unit's processes).
rm -f "$tmp/calls.log" "$tmp/driven" "$W/rc"
printf 'UIS="es pocket-ui"\nDRIVE_ARGS="--scenarios start --asleep 20"\nPLAN=scripts/rocknix-compare-plan.json\n' >"$W/run.env"
env PATH="$BIN:$PATH" COMPARE_ROOT="$R" COMPARE_WORK="$W" COMPARE_PY="$tmp/drive" SLOW=1 sh "$W/scripts/rocknix-compare-device.sh" >"$tmp/device.log" 2>&1 &
pid=$!
i=0; while [ ! -f "$tmp/halfway" ] && [ $i -lt 100 ]; do sleep 0.1; i=$((i+1)); done
[ -f "$tmp/halfway" ] || fail "the stand-in driver did not start"
kill -TERM "$(cat "$tmp/halfway")" "$pid"  # the driver and the script: the unit's processes all get it
wait "$pid" || true
[ "$(cat "$W/rc")" = 3 ] || { cat "$tmp/device.log"; fail "stopped: rc $(cat "$W/rc"), expected 3"; }
check_restored "a stopped run"
echo "test-rocknix-compare: device script ok (everything put back after a whole run and after SIGTERM)"
if [ "${1:-}" = --sim ]; then
  COMPARE_SIM_KEEP=1 python3 -I "$PY" selftest --sim | tee "$tmp/sim.log"
  sim=$(sed -n 's/^selftest: the simulated run is in //p' "$tmp/sim.log")
  [ -d "$sim" ] || fail "the simulation left no folder"
  # The video from the simulated showcase: each clip placed on the presses' clock within a frame of the stand-in
  # wf-recorder's true start.
  python3 -I "$ROOT/scripts/rocknix-compare-video.py" "$sim" --numbers "$sim" --out "$tmp/video/v.mp4" --work "$tmp/video/work" >/dev/null 2>"$tmp/video.log" ||
    { cat "$tmp/video.log"; rm -rf "$sim"; fail "the video cut failed"; }
  python3 -I - "$sim" "$tmp/video/v.json" <<'EOF2'
import json, sys
prov = json.load(open(sys.argv[2]))
for ui in ("es", "pocket-ui"):
    true = int(open(f"{sys.argv[1]}/out/{ui}/showcase.mp4.t0").read())
    off = (prov["clips"][ui]["anchor_ns"] - true) / 1e6
    assert abs(off) < 17, (ui, off)
    print(f"test-rocknix-compare: video {ui}: lined up within {off:+.1f} ms (fit {prov['clips'][ui]['fit']:.2f})")
EOF2
  rm -rf "$sim"
fi
echo "test-rocknix-compare: ok"
