#!/bin/bash
# Host test of scripts/brick-smoke.sh against a fake Brick (docs/TRIMUI-BRICK-SMOKE.md): a fake sysfs/proc tree like scripts/test-brick-hw.sh's, and a small
# simulator that answers the FIFOs the way pocket-ui does (power dark/wake, play, quit). A good Brick must pass everything; each bug the smoke test
# exists for (lit LEDs in the dark, picture options without effect, the clock too low in a game, no route, busy while dark) must be caught.
# What it cannot prove: that the real Brick behaves like the simulator. It proves the checks see what they are written to see.
#   scripts/test-brick-smoke.sh
set -u
ROOT=$(cd "$(dirname "$0")/.." && pwd)
fails=0
ok() { echo "ok   $*"; }
bad() { echo "FAIL $*"; fails=$((fails + 1)); }
W=$(mktemp -d "${TMPDIR:-/tmp}/brick-smoke-test.XXXXXX")
SIMPID=
trap '[ -z "$SIMPID" ] || kill "$SIMPID" 2>/dev/null; rm -rf "$W"' EXIT
command -v python3 >/dev/null || { echo "SKIP no python3"; exit 0; }

cat >"$W/sim.py" <<'PY'
import os, sys, threading, json, time
R = sys.argv[1]; bugs = set(sys.argv[2].split(",")) - {""}
def w(p, v):
    os.makedirs(os.path.dirname(p), exist_ok=True); open(p, "w").write(v)
def rd(p): return open(p).read().strip()
cpu = R + "/sys/devices/system/cpu/cpufreq/policy0/"
bl = R + "/tmp/pocket-ui/hwroot/sys/class/backlight/panel/brightness"
led = R + "/sys/class/led_anim/"
st = {"dark": False, "game": None}
def clocks(gov, mx, cur):
    w(cpu + "scaling_governor", gov); w(cpu + "scaling_max_freq", str(mx)); w(cpu + "scaling_cur_freq", str(cur))
def menu(): clocks("ondemand", 2000000, 1000000)
def fb(settings):
    W, H, S = 1024, 768, 4096
    rows = bytearray(S * 1024 * 2)           # two pages, the second one shown (pan 0,768)
    page = bytearray(S * H)
    sc = settings.get("scaling", "native") if "options-ignored" not in bugs else "native"
    ef = settings.get("effect", "none") if "options-ignored" not in bugs else "none"
    x0, x1, y0, y1 = (112, 912, 24, 744) if sc == "native" else (0, 1024, 0, 768)
    for y in range(y0, y1):
        line = bytearray()
        for x in range(x0, x1):
            v = 200 if (ef != "scanlines" or y % 5) else 60
            line += bytes((v, (x * 3) & 255, 90, 0))
        page[y * S + x0 * 4:y * S + x1 * 4] = line
    if settings.get("show_fps") == "fps" and "options-ignored" not in bugs:
        for y in range(4, 30):
            for x in range(4, 200): page[y * S + x * 4:y * S + x * 4 + 4] = bytes((255, 255, 255, 0))
    w(R + "/dev/fb0", ""); open(R + "/dev/fb0", "wb").write(bytes(S * 768) + bytes(page))
    w(R + "/sys/class/graphics/fb0/pan", "0,768\n"); w(R + "/sys/class/graphics/fb0/stride", "4096\n")
def proc(pid, comm, cmd, ticks=0):
    d = "%s/proc/%d/" % (R, pid); os.makedirs(d, exist_ok=True)
    w(d + "comm", comm + "\n"); open(d + "cmdline", "wb").write(cmd.replace(" ", "\0").encode() + b"\0")
    w(d + "stat", "%d (%s) S 1 1 1 0 -1 0 0 0 0 0 %d %d 0 0 20 0 1 0 1 0 0\n" % (pid, comm, ticks, 0))
def set_dark(on):
    st["dark"] = on
    w(bl, "0\n" if on else "70\n")
    if on:
        clocks("powersave" if "dark-clock" not in bugs else "performance", 408000 if "dark-clock" not in bugs else 2000000, 408000 if "dark-clock" not in bugs else 2000000)
        for f in ("effect_l", "effect_lr", "effect_enable"): w(led + f, "0\n")
        if "frame-hex" in bugs: w(led + "frame_hex", "373737 " * 6 + "000000 " * 17 + "\n")
    else: menu()
def power():
    set_dark(not st["dark"])
def play(system, path):
    gs = R + "/mnt/SDCARD/pocket-ui/storage/.config/pocket-ui/game-settings/%s/%s.json" % (system, os.path.splitext(os.path.basename(path))[0])
    s = json.load(open(gs)) if os.path.exists(gs) else {}
    time.sleep(0.3)
    proc(900, "pocket-core", "pocket-core core.so " + path)
    clocks("performance" if "game-clock" not in bugs else "ondemand", 2000000, 2000000 if "game-clock" not in bugs else 900000)
    fb(s); st["game"] = 900
    if "sound-on" in bugs: w(R + "/mix/HpSpeaker_Switch", "on\n")
def quit_game():
    if st["game"]:
        import shutil; shutil.rmtree("%s/proc/900" % R, ignore_errors=True); st["game"] = None; menu()
def reader(path, fn):
    fd = os.open(path, os.O_RDWR); buf = b""
    while True:
        buf += os.read(fd, 4096)
        while b"\n" in buf:
            line, buf = buf.split(b"\n", 1)
            try: fn(line.decode().strip())
            except Exception as e: print("sim:", e, file=sys.stderr)
def ui(line):
    if line == "power": power()
    elif line.startswith("play ") and "no-play" not in bugs:
        _, sysn, path = line.split(" ", 2)
        if ".pocket-ui-test" in path: play(sysn, path)
def core(line):
    if line.startswith("quit"): quit_game()
w(bl, "70\n"); menu(); w(cpu + "cpuinfo_max_freq", "2000000\n"); w(cpu + "scaling_available_frequencies", "408000 816000 1416000 2000000\n"); w(cpu + "cpuinfo_min_freq", "408000\n")
for z in ("l", "r", "lr", "m", "f1", "f2"): w(led + "effect_" + z, "0\n"); w(led + "effect_rgb_hex_" + z, "000000\n"); w(led + "effect_duration_" + z, "1000\n")
w(led + "effect_enable", "0\n"); w(led + "anim_frames_enable", "0\n"); w(led + "frame_hex", "000000 " * 23 + "\n"); w(led + "max_scale", "14\n")
for n in range(3): w(R + "/sys/class/leds/sunxi_led%d/brightness" % n, "0\n")
if "lit-leds" in bugs: w(R + "/sys/class/leds/sunxi_led1/brightness", "37\n")
proc(101, "pocket-ui", "pocket-ui", 10); proc(102, "pocket-fb", "pocket-fb --wayland x", 10); proc(103, "brick-hw", "brick-hw watch /x 70"); proc(104, "sh", "sh /app/brick-net watch")
if "busy-dark" in bugs:
    def spin():
        n = 0
        while True:
            n += 150; w("%s/proc/102/stat" % R, "102 (pocket-fb) S 1 1 1 0 -1 0 0 0 0 0 %d 0 0 0 20 0 1 0 1 0 0\n" % n); time.sleep(0.5)
    threading.Thread(target=spin, daemon=True).start()
if "no-route" not in bugs: w(R + "/proc/net/route", "Iface Destination Gateway\nwlan0 00000000 0100000A\n")
else: w(R + "/proc/net/route", "Iface Destination Gateway\nwlan0 0000A8C0 00000000\n")
for p, f in ((R + "/tmp/pocket-ui.in", ui), (R + "/tmp/pocket-core.cmd", core)):
    if os.path.exists(p): os.remove(p)
    os.mkfifo(p); threading.Thread(target=reader, args=(p, f), daemon=True).start()
if "start-dark" in bugs: set_dark(True)
open(R + "/ready", "w").write("1")
while True: time.sleep(1)
PY

# The sound chip's mixer, as `amixer -c 0 [-q] cget|cset name=X [V]` sees it: one file per control below $MIXDIR.
cat >"$W/amixer" <<'SH'
#!/bin/sh
q=; while [ $# -gt 0 ]; do case $1 in -c) shift ;; -q) q=1 ;; cget|cset) op=$1 ;; name=*) n=${1#name=} ;; *) v=$1 ;; esac; shift; done
f="$MIXDIR/$(echo "$n" | tr ' ' _)"
if [ "$op" = cset ]; then echo "$v" >"$f"; else echo "numid=1,iface=MIXER,name='$n'"; echo "  : values=$(cat "$f" 2>/dev/null)"; fi
SH
chmod +x "$W/amixer"
printf '#!/bin/sh\nexit 0\n' >"$W/brick-hw"; chmod +x "$W/brick-hw"

run_case() { # name bugs owner-flag
  local R=$W/$1; rm -rf "$R"; mkdir -p "$R"
  local C=$R/mnt/SDCARD
  mkdir -p "$C/pocket-ui/app" "$C/pocket-ui/storage/.config/pocket-ui" "$C/pocket-ui/logs" "$C/Saves/savestates" "$C/Roms/gba" "$R/dev"
  echo brick-test1234 >"$C/pocket-ui/app/RELEASE"; echo "pocket-ui brick standalone brick-test1234 (pocket-ui commit abc1234, 2026-11-03)" >"$C/pocket-ui/app/VERSION"
  echo "GL: the probe works, the screen goes through the GPU" >"$C/pocket-ui/logs/boot.log"
  echo save >"$C/Saves/a.sav"; echo rom >"$C/Roms/gba/x.gba"
  mkdir -p "$R/mix"; echo off >"$R/mix/HpSpeaker_Switch"; echo off >"$R/mix/Headphone_Switch"; echo 0,0 >"$R/mix/DAC_volume"
  case ",$2," in *,not-muted,*) echo on >"$R/mix/HpSpeaker_Switch" ;; esac
  python3 -I "$W/sim.py" "$R" "$2" 2>"$R/sim.err" & SIMPID=$!
  local n=0; while [ ! -e "$R/ready" ] && [ $n -lt 50 ]; do sleep 0.1; n=$((n + 1)); done
  MIXDIR=$R/mix SMOKE_AMIXER=$W/amixer SMOKE_HW=$W/brick-hw SMOKE_STEP=0.2 SMOKE_WINDOW=1 "$ROOT/scripts/brick-smoke.sh" --local-root "$R" --log-dir "$W/logs" ${3:+--owner-ok} --rom "$ROOT/docs/../README.md" --expect-version brick-test1234 >"$W/$1.out" 2>&1
  echo $? >"$W/$1.rc"
  kill "$SIMPID" 2>/dev/null; wait "$SIMPID" 2>/dev/null; SIMPID=
}
has() { grep -q "$2" "$W/$1.out"; }
check() { if eval "$3"; then ok "$1: $2"; else bad "$1: $2"; sed 's/^/    | /' "$W/$1.out" | head -40; fi; }

run_case good "" 1
check good "a healthy Brick passes all checks" '[ "$(cat $W/good.rc)" = 0 ] && ! has good "^FAIL" && has good "^PASS  *5-wake" && has good "^PASS  *6-option-B" && has good "^PASS  *6-over-the-game" && has good "^PASS  *7-owner-files"'
check good "the dark screen, the clock and the lights were looked at" 'has good "^PASS  *5-dark-clock" && has good "^PASS  *5-dark-lights" && has good "^PASS  *5-dark-load" && has good "^PASS  *6-game-clock"'

if command -v busybox >/dev/null; then
  SMOKE_SH="busybox ash" run_case bb "" 1
  check bb "the device script runs under BusyBox ash" '[ "$(cat $W/bb.rc)" = 0 ] && has bb "^PASS  *6-over-the-game" && has bb "^PASS  *5-wake"'
fi

run_case plain "" ""
check plain "without --owner-ok the screen steps are skipped, the rest runs" 'has plain "^SKIP  *5-dark" && has plain "^SKIP  *6-game" && has plain "^PASS  *4-lights" && [ "$(cat $W/plain.rc)" = 0 ]'
check good "the Brick ends dark and muted, the watcher never had to mute" 'has good "^PASS  *0-mute" && has good "^PASS  *9-end-muted" && has good "^PASS  *9-end-dark" && has good "^PASS  *9-sound-stayed-off"'
check plain "a session without --owner-ok ends dark too" 'has plain "^PASS  *9-end-dark"'

run_case sdark "start-dark" 1
check sdark "a Brick left dark is woken for 5 and 6 and ends dark again" '[ "$(cat $W/sdark.rc)" = 0 ] && has sdark "^PASS  *5-wake" && has sdark "^PASS  *6-over-the-game" && has sdark "^PASS  *9-end-dark"'
run_case loud "not-muted" 1
check loud "sound not off at the start: FAIL 0-mute and nothing else runs" 'has loud "^FAIL  *0-mute" && ! has loud "1-version" && [ "$(cat $W/loud.rc)" = 1 ]'
run_case son "sound-on" 1
check son "sound switched on during the run: muted again at once, and FAIL" 'has son "^FAIL  *9-sound-stayed-off" && has son "^PASS  *9-end-muted"'

run_case lit "lit-leds" ""
check lit "a lit sunxi LED with Lights=Off fails" 'has lit "^FAIL  *4-lights" && [ "$(cat $W/lit.rc)" = 1 ]'
run_case frame "frame-hex" 1
check frame "frame_hex still lit in the dark fails (the 03.11. bug)" 'has frame "^FAIL  *5-dark-lights.*frame_hex"'
run_case opt "options-ignored" 1
check opt "picture options and Over the game without effect fail" 'has opt "^FAIL  *6-option-B" && has opt "^FAIL  *6-option-C" && has opt "^FAIL  *6-over-the-game"'
run_case clk "game-clock" 1
check clk "a game on ondemand fails" 'has clk "^FAIL  *6-game-clock"'
run_case dclk "dark-clock" 1
check dclk "a dark screen at full speed fails" 'has dclk "^FAIL  *5-dark-clock"'
run_case route "no-route" ""
check route "no default route fails" 'has route "^FAIL  *2-services.*default-route"'
run_case busy "busy-dark" 1
check busy "pocket-fb busy while dark fails" 'has busy "^FAIL  *5-dark-load.*pocket-fb"'
run_case noplay "no-play" 1
check noplay "a pocket-ui without the play command is told so" 'has noplay "^FAIL  *6-start-A"'

[ $fails -eq 0 ] && echo "ALL OK" || { echo "$fails FAILED"; exit 1; }
