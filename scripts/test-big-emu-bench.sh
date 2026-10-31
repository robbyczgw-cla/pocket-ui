#!/bin/sh
# Tests of scripts/big-emu-bench.sh without a device (about 1 min): the statistics and verdicts of big-emu-bench.py,
# then the device half run on this machine (BIGEMU_LOCAL=1) against a fake /sys, a fake runemu.sh and a fake Dolphin that
# presents frames at a chosen rate: a smooth run, a slow one, a 30 fps game, the fan starting, an emulator that dies,
# a low battery, a missing game. Each checks the folders, mute and brightness are put back and nothing is left running.
#   scripts/test-big-emu-bench.sh
set -u
ROOT=$(cd "$(dirname "$0")/.." && pwd)
python3 -I "$ROOT/scripts/big-emu-bench.py" selftest || exit 1
tmp=$(mktemp -d "${TMPDIR:-/tmp}/test-big-emu.XXXXXX")
trap 'kill $(pidof dolphin-emu-nogui armsx2-qt) 2>/dev/null; [ -n "${KEEP:-}" ] || rm -rf "$tmp"' EXIT
failed=0
check() { # <what> <command...>
  what=$1; shift
  if "$@" >/dev/null 2>&1; then echo "ok   $what"; else echo "FAIL $what"; failed=1; fi
}

S=$tmp/storage SYSD=$tmp/sys B=$tmp/bin
mkdir -p "$B" "$SYSD/class/power_supply/battery" "$SYSD/class/hwmon/hwmon9" "$SYSD/class/thermal/thermal_zone0" \
  "$SYSD/class/thermal/thermal_zone1" "$SYSD/class/devfreq/3d00000.gpu" "$SYSD/class/backlight/bl" \
  "$SYSD/devices/system/cpu/cpufreq/policy0" "$SYSD/devices/system/cpu/cpufreq/policy4" "$SYSD/devices/system/cpu/cpufreq/policy7" \
  "$S/.config/pocket-ui" "$S/.config/dolphin-emu/GamecubeControllerProfiles" "$S/.config/ARMSX2/inis" "$S/.local/share/dolphin-emu/Wii" "$S/roms/wii" "$S/roms/ps2" "$S/roms/gamecube"
w() { printf '%s\n' "$2" >"$1"; }
resetsys() {
  w "$SYSD/class/power_supply/battery/capacity" 80; w "$SYSD/class/power_supply/battery/status" Discharging
  w "$SYSD/class/power_supply/battery/current_now" -500000; w "$SYSD/class/power_supply/battery/voltage_now" 3900000
  w "$SYSD/class/hwmon/hwmon9/pwm1" 0
  w "$SYSD/class/thermal/thermal_zone0/type" cpu4-bottom-thermal; w "$SYSD/class/thermal/thermal_zone0/temp" 48000
  w "$SYSD/class/thermal/thermal_zone1/type" gpu-top-thermal; w "$SYSD/class/thermal/thermal_zone1/temp" 45000
  w "$SYSD/class/devfreq/3d00000.gpu/cur_freq" 305000000
  w "$SYSD/devices/system/cpu/cpufreq/policy0/scaling_cur_freq" 1000000; w "$SYSD/devices/system/cpu/cpufreq/policy4/scaling_cur_freq" 2000000
  w "$SYSD/devices/system/cpu/cpufreq/policy7/scaling_cur_freq" 2400000
  w "$SYSD/class/backlight/bl/brightness" 800; w "$SYSD/class/backlight/bl/max_brightness" 1000
  w "$tmp/mute" 0
}
resetsys
w "$S/.config/pocket-ui/no-autosuspend" ""
w "$S/.config/dolphin-emu/GFX.ini" "original"
w "$S/.local/share/dolphin-emu/Wii/save.bin" "user-save"
w "$S/roms/wii/Game One.iso" "fps=60"
w "$S/roms/wii/Slow.iso" "fps=35"
w "$S/roms/wii/Thirty.iso" "fps=30"
w "$S/roms/wii/Fan.iso" "fps=60 fan_after=3"
w "$S/roms/wii/Dies.iso" "fps=60 die_after=3"
w "$S/roms/ps2/Game.chd" "fps=60"
w "$S/roms/ps2/Mcd001.ps2" "user-card"
w "$S/roms/gamecube/Game.rvz" "fps=60"
w "$S/.config/ARMSX2/inis/PCSX2.ini" "[Pad1]
LargeMotorScale = 1
SmallMotorScale = 1"
w "$S/.config/dolphin-emu/GCPadNew.ini" "Rumble/Motor = Motor"
w "$S/.config/dolphin-emu/GamecubeControllerProfiles/GCPadNew.ini.east.stacked" "Rumble/Motor = Motor"

# fake wpctl (state in $tmp/mute), pw-top (the ERR of one node from $tmp/err), swaymsg
cat >"$B/wpctl" <<'X'
#!/bin/sh
case $1 in
  get-volume) if [ "$(cat "$FAKE/mute")" = 1 ]; then echo "Volume: 0.50 [MUTED]"; else echo "Volume: 0.50"; fi ;;
  set-mute) echo "$3" >"$FAKE/mute" ;;
esac
X
cat >"$B/pw-top" <<'X'
#!/bin/sh
echo "S   ID  QUANT   RATE    WAIT    BUSY   W/Q   B/Q  ERR  FORMAT  NAME"
echo "R   31   1024  48000   10us   5us   0.01  0.01    0  S16LE   alsa_output"
echo "R   44   1024  48000   10us   5us   0.01  0.01  ${FAKE_ERR:-0}  S16LE  + dolphin-emu-nogui"
X
# the fake emulator: presents frames at fps=, writes like frametime.c, edits the emulator folders, ends on SIGTERM
cat >"$B/fake-emu.py" <<'X'
import os, signal, sys, time
cfg = dict(kv.split("=") for kv in open(sys.argv[1]).read().split())  # argv[1]: the game
fps = float(cfg["fps"])
out = open(os.path.join(os.environ["BIG_FTDIR"], "dolphin-emu-nogui.%d" % os.getpid()), "w")
signal.signal(signal.SIGTERM, lambda *a: (out.flush(), sys.exit(0)))
S = os.environ["BIG_STORAGE"]
open(S + "/.config/dolphin-emu/GFX.ini", "w").write("changed by the emulator")
open(S + "/.local/share/dolphin-emu/Wii/new.bin", "w").write("x")
os.makedirs(S + "/.local/share/azahar-emu", exist_ok=True)
t0 = time.monotonic(); n = 0
while True:
    now = time.monotonic()
    out.write("%d\n" % (now * 1e9)); n += 1
    if n % 30 == 0: out.flush()
    if "fan_after" in cfg and now - t0 > float(cfg["fan_after"]):
        open(os.environ["BIG_SYS"] + "/class/hwmon/hwmon9/pwm1", "w").write("51\n")
    if "die_after" in cfg and now - t0 > float(cfg["die_after"]):
        out.flush(); sys.exit(1)
    time.sleep(max(0, t0 + n / fps - time.monotonic()))
X
# runemu.sh: starts the emulator in the foreground like ROCKNIX's, then "puts the governors back"
cat >"$B/runemu.sh" <<'X'
#!/bin/sh
echo "$@" >"$FAKE/runemu.args"; pwd >"$FAKE/runemu.cwd"
case $2 in -Pps2) emu=armsx2-qt ;; *) emu=dolphin-emu-nogui ;; esac
"$FAKE/bin/$emu" "$FAKE/bin/fake-emu.py" "$1"
touch "$FAKE/governors-restored"
X
chmod +x "$B"/*
# a process called dolphin-emu-nogui, as pidof wants it (a script would show as python3)
ln -s "$(command -v python3)" "$B/dolphin-emu-nogui"; ln -s "$(command -v python3)" "$B/armsx2-qt"
export FAKE=$tmp PATH="$B:$PATH" BIGEMU_LOCAL=1 BIGEMU_LOCK=none POCKETUI_HOST=
export BIG_SYS=$SYSD BIG_STORAGE=$S BIG_RUNEMU=$B/runemu.sh BIG_FTDIR=$tmp/ft BIG_WORK=$tmp/work BIG_CWD=$tmp BIG_STARTWAIT=15 \
  BIG_BACKUP=$tmp/backup BIG_FIFO=$tmp/ui.in

state_ok() { # the device is as it was
  [ "$(cat "$S/.config/dolphin-emu/GFX.ini")" = original ] && [ ! -e "$S/.local/share/dolphin-emu/Wii/new.bin" ] &&
  [ "$(cat "$S/.local/share/dolphin-emu/Wii/save.bin")" = user-save ] && [ ! -e "$S/.local/share/azahar-emu" ] &&
  [ ! -e "$tmp/backup" ] && [ "$(cat "$tmp/mute")" = 0 ] && [ "$(cat "$SYSD/class/backlight/bl/brightness")" = 800 ] &&
  [ -z "$(pidof dolphin-emu-nogui)" ] && [ ! -e "$tmp/ft" ]
}
run() { # <name> <args...>: output in $tmp/<name>.out, exit code in $rc
  name=$1; shift
  rm -f "$tmp/governors-restored"
  resetsys
  sh "$ROOT/scripts/big-emu-bench.sh" --out "$tmp/out-$name" --minutes 0.1 --warmup 2 "$@" >"$tmp/$name.out" 2>"$tmp/$name.err"
  rc=$?
}
has() { grep -q -- "$2" "$tmp/$1.out"; }

mkfifo "$tmp/ui.in"
cat "$tmp/ui.in" >"$tmp/nudges" &
reader=$!

run smooth "$S/roms/wii/Game One.iso"
check "smooth: exit 0" [ $rc = 0 ]
check "smooth: full speed" has smooth "full speed"
check "smooth: started like pocket-ui" grep -q -- "-Pwii --core=dolphin-sa-wii --emulator=dolphin --controllers=" "$tmp/runemu.args"
check "smooth: from /tmp-like cwd (BIG_CWD)" [ "$(cat "$tmp/runemu.cwd")" = "$tmp" ]
check "smooth: saves, mute, screen put back, nothing left" state_ok
check "smooth: governors restored by runemu.sh itself" [ -e "$tmp/governors-restored" ]
check "smooth: changes to saves/data are reported, config rewrites are not" sh -c "grep -q 'new.bin' '$tmp/smooth.out' && ! grep -q GFX.ini '$tmp/smooth.out'"
check "smooth: the screen was dimmed during the run" awk -F, '$1 == "m" && $14 == 50 {f = 1} END {exit !f}' "$tmp/out-smooth/g1/samples.csv"
check "smooth: a nudge reached pocket-ui's FIFO" [ -s "$tmp/nudges" ]
check "smooth: no leftover device.sh work folder" [ ! -e "$tmp/work/run-1" ]

FAKE_ERR=0 run slow "$S/roms/wii/Slow.iso"
check "slow: too slow" has slow "too slow"
check "slow: state" state_ok

run thirty "$S/roms/wii/Thirty.iso@30"
check "30 fps game with @30: full speed" has thirty "full speed"

run fan "$S/roms/wii/Fan.iso"
check "fan: exit 4" [ $rc = 4 ]
check "fan: aborted, says the fan" has fan "aborted - the fan started"
check "fan: state" state_ok

run dies "$S/roms/wii/Dies.iso"
check "dies: aborted" has dies "ended by itself"
check "dies: state" state_ok

# a screen that is dark stays dark: no press on pocket-ui's FIFO (it would light the screen), a lit screen is put out again
nudges_before=$(wc -c <"$tmp/nudges")
resetsys; w "$SYSD/class/backlight/bl/brightness" 0
rm -f "$tmp/governors-restored"
sh "$ROOT/scripts/big-emu-bench.sh" --out "$tmp/out-darkbl" --minutes 0.1 --warmup 2 "$S/roms/wii/Game One.iso" >"$tmp/darkbl.out" 2>"$tmp/darkbl.err"
check "dark screen: no nudge sent" [ "$(wc -c <"$tmp/nudges")" = "$nudges_before" ]
check "dark screen: still dark after" [ "$(cat "$SYSD/class/backlight/bl/brightness")" = 0 ]
check "dark screen: no 'screen went off' note" sh -c "! grep -q 'screen went off' '$tmp/darkbl.out'"

# PS2 (ARMSX2) and GameCube (Dolphin): started like pocket-ui, rumble off during the run, cards and config put back
run ps2 "$S/roms/ps2/Game.chd"
check "ps2: exit 0, full speed" sh -c "[ $rc = 0 ] && grep -q 'full speed' '$tmp/ps2.out'"
check "ps2: started like pocket-ui" grep -q -- "-Pps2 --core=armsx2-sa --emulator=armsx2 --controllers=" "$tmp/runemu.args"
check "ps2: rumble was off during the run" grep -q "LargeMotorScale = 0" "$tmp/ps2.err"
check "ps2: motor scales and memory card put back" sh -c "grep -q 'LargeMotorScale = 1' '$S/.config/ARMSX2/inis/PCSX2.ini' && [ \"\$(cat '$S/roms/ps2/Mcd001.ps2')\" = user-card ] && [ ! -e '$tmp/backup' ] && [ -z \"\$(pidof armsx2-qt)\" ]"
run gc "$S/roms/gamecube/Game.rvz"
check "gamecube: exit 0, full speed" sh -c "[ $rc = 0 ] && grep -q 'full speed' '$tmp/gc.out'"
check "gamecube: started like pocket-ui" grep -q -- "-Pgamecube --core=dolphin-sa-gc --emulator=dolphin --controllers=" "$tmp/runemu.args"
check "gamecube: rumble unmapped during the run, mapped again after" sh -c "grep -q 'Rumble/Motor =\$' '$tmp/gc.err' || grep -q '1 Rumble/Motor =' '$tmp/gc.err'; grep -q 'Rumble/Motor = Motor' '$S/.config/dolphin-emu/GamecubeControllerProfiles/GCPadNew.ini.east.stacked'"

# audio underruns: the fake node reports ERR 0 at the end of the warm-up and 7 at the end of the run (second call)
cat >"$B/pw-top" <<'X'
#!/bin/sh
n=$(cat "$FAKE/pwcalls" 2>/dev/null || echo 0); echo $((n + 1)) >"$FAKE/pwcalls"
echo "R   44   1024  48000   10us   5us   0.01  0.01  $([ "$n" = 0 ] && echo 0 || echo 7)  S16LE  + dolphin-emu-nogui"
X
rm -f "$tmp/pwcalls"
run audio "$S/roms/wii/Game One.iso"
check "audio: underruns counted, verdict playable" has audio "playable"
check "audio: names the underruns" has audio "7 audio underruns"

resetsys; w "$SYSD/class/power_supply/battery/capacity" 15
sh "$ROOT/scripts/big-emu-bench.sh" --out "$tmp/out-bat" --minutes 0.1 "$S/roms/wii/Game One.iso" >"$tmp/bat.out" 2>"$tmp/bat.err"; rc=$?
check "battery 15 %: refused (exit 2), nothing started" [ $rc = 2 ] && [ ! -e "$tmp/governors-restored" ]
check "battery 15 %: says so" grep -q "battery at 15" "$tmp/bat.err"
resetsys
rm "$S/.config/pocket-ui/no-autosuspend"
sh "$ROOT/scripts/big-emu-bench.sh" --out "$tmp/out-ns" --minutes 0.1 "$S/roms/wii/Game One.iso" >"$tmp/ns.out" 2>"$tmp/ns.err"; rc=$?
check "no no-autosuspend: refused" [ $rc = 2 ] && grep -q "no-autosuspend" "$tmp/ns.err"
w "$S/.config/pocket-ui/no-autosuspend" ""
sh "$ROOT/scripts/big-emu-bench.sh" --out "$tmp/out-nf" --minutes 0.1 "$S/roms/wii/Nope.iso" >"$tmp/nf.out" 2>"$tmp/nf.err"; rc=$?
check "missing game: refused" [ $rc = 2 ] && grep -q "no such file" "$tmp/nf.err"
check "too long a session is refused" sh -c "! sh '$ROOT/scripts/big-emu-bench.sh' --minutes 6 /storage/roms/wii/x.iso"
check "a folder that is neither ps2, gamecube, wii nor 3ds is refused" sh -c "! BIGEMU_LOCAL=1 BIGEMU_LOCK=none sh '$ROOT/scripts/big-emu-bench.sh' --out '$tmp/o3' --minutes 0.1 '$S/roms/snes/x.sfc'"
kill $reader 2>/dev/null
[ $failed = 0 ] && echo "test-big-emu-bench: all ok"
exit $failed
