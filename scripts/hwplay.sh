#!/bin/sh
# Plays a game on pocket-core (or RetroArch) with the virtual pad and takes screenshots on the way.
# pocket-ui is stopped meanwhile (it would read the virtual pad too) and restarted at the end.
# Wrap in scripts/devlock.sh.
#   hwplay.sh CORE ROM STEPS
# STEPS is a space-separated list, run in order on the device:
#   w:SECONDS   wait          p:BUTTON  press (150 ms)   h:BUTTON:MS  hold
#   s:NAME      screenshot    c:CMD     pocket-core command (save, load, quit)
#   b:SECONDS   deploy/bench.sh for that long on the running emulator
#   r:CMD       RetroArch network command (LOAD_STATE …)    t:x  list the emulator's threads
#   x:CMD       any shell command on the device, ~ for spaces
#   T:SECONDS   CPU per thread of the emulator over that long (deploy/thread-cpu.sh)
#   v:CMD       any vpad.py command (stick 0.5~0, down~A …), ~ for spaces
# Env: PRE=shell command run on the device first; GDB=1 runs pocket-core under gdb (backtrace in the log); RA=1 runs the game in RetroArch through runemu.sh instead; TRACE=1 sets POCKET_CORE_TRACE;
#      SA=<process> runs ROCKNIX's standalone emulator through runemu.sh (CORE is then its ES core, e.g. ppsspp-sa,
#      and SA the process to measure, e.g. ppsspp; EMU=<ES emulator name> if it differs from SA);
#      PCENV="K=V ..." extra environment for pocket-core; SYSTEM (default: the ROM's folder) for runemu.sh.
# Every emulator runs with deploy/frametime.c preloaded: b: steps also print the frame times of each process
# that presented frames during the window (same method for every host), and the PipeWire xrun counts (pw-top ERR).
set -eu
. "$(dirname "$0")/host.sh"
ROOT=$(cd "$(dirname "$0")/.." && pwd)
CORE=$1; ROM=$2; STEPS=$3
DIR=/storage/hwrender
SYSTEM=${SYSTEM:-$(basename "$(dirname "$ROM")")}
WL="XDG_RUNTIME_DIR=/var/run/0-runtime-dir WAYLAND_DISPLAY=wayland-1"
TAG=${RA:+ra}; TAG=${SA:+sa}${TAG}; TAG=${TAG:-pc}
PROC=pocket-core; [ -n "${RA:-}" ] && PROC=retroarch; [ -n "${SA:-}" ] && PROC=$SA
FT="LD_PRELOAD=$DIR/frametime.so"; [ -n "${NOFT:-}" ] && FT=""
VPAD_SDL="03000000341200007856000001000000,pocket-ui test pad,platform:Linux,a:b0,b:b1,y:b2,x:b3,leftshoulder:b4,rightshoulder:b5,lefttrigger:b6,righttrigger:b7,back:b8,start:b9,guide:b10,dpup:b11,dpdown:b12,dpleft:b13,dpright:b14,leftx:a0,lefty:a1,rightx:a2,righty:a3,"

script=$(mktemp)
{
  echo "set +e; systemctl stop pocketui 2>/dev/null; pkill -9 -f /storage/hwrender/pocket-core; kill -9 \$(pidof retroarch) 2>/dev/null; pkill -x pocket-core; pkill -f '[v]pad.py'; sleep 0.5"
  echo 'gpu_ns() { for p in $(pidof "$@"); do for f in /proc/$p/fdinfo/*; do awk -v p=$p "/drm-client-id/{id=\$2} /drm-engine-gpu/{print p, id, \$2}" $f 2>/dev/null; done; done | sort -u -k1,2 | awk "{s+=\$3} END{print s+0}"; }'
  [ -n "${PRE:-}" ] && echo "$PRE"
  [ -n "${SA:-}" ] && echo "kill -9 \$(pidof $SA) 2>/dev/null"
  # Standalone emulators (or INJECT=1): the test input goes into the Mini V2's own pad, so it takes the real path.
  if [ -n "${SA:-}${INJECT:-}" ]; then
    # InputPlumber hides the device it reads; its node then lives in /dev/inputplumber/sources.
    echo "ev=\$(awk '/Name=\"Retroid Pocket Gamepad\"/ {f=1} f && /Handlers/ {for (i = 1; i <= NF; i++) if (\$i ~ /^event/) print \$i; exit}' /proc/bus/input/devices)"
    echo "export VPAD_INJECT=/dev/input/\$ev; [ -e \$VPAD_INJECT ] || VPAD_INJECT=/dev/inputplumber/sources/\$ev"
  fi
  echo "rm -rf /tmp/hwshots /tmp/ft; mkdir -p /tmp/hwshots; cd $DIR; nohup python3 vpad.py >/tmp/vpad.log 2>&1 & sleep 1.5"
  if [ -n "${SA:-}" ]; then
    # SDL games see the virtual pad only with a mapping (vpad.py's buttons and axes in SDL's order).
    echo "SDL_GAMECONTROLLERCONFIG='$VPAD_SDL' $FT nohup /usr/bin/runemu.sh \"$ROM\" -P$SYSTEM --core=$CORE --emulator=${EMU:-$SA} --controllers= >/tmp/hw.log 2>&1 &"
  elif [ -n "${RA:-}" ]; then
    if [ -z "$ROM" ]; then
      # No game (a BIOS menu): runemu.sh needs a ROM, so RetroArch runs directly on its config.
      echo "$WL nohup retroarch -L /usr/lib/libretro/${CORE}_libretro.so --config /storage/.config/retroarch/retroarch.cfg >/tmp/hw.log 2>&1 &"
    else
      echo "$FT nohup /usr/bin/runemu.sh \"$ROM\" -P$SYSTEM --core=$CORE --emulator=retroarch --controllers= >/tmp/hw.log 2>&1 &"
    fi
  else
    echo "$WL $FT ${PCENV:-} ${TRACE:+POCKET_CORE_TRACE=1} ${PREF:+POCKET_CORE_PREFERRED_HW=$PREF} nohup ${GDB:+gdb -batch -ex run -ex bt --args} ./pocket-core /usr/lib/libretro/${CORE}_libretro.so \"$ROM\" >/tmp/hw.log 2>&1 &"
  fi
  for step in $STEPS; do
    case $step in
      w:*) echo "sleep ${step#w:}" ;;
      p:*) echo "timeout 2 sh -c \"echo 'press ${step#p:} 150' > /tmp/vpad.cmd\"" ;;
      h:*) b=${step#h:}; echo "timeout 2 sh -c \"echo 'press ${b%%:*} ${b#*:}' > /tmp/vpad.cmd\"" ;;
      v:*) echo "timeout 2 sh -c \"echo '$(echo "${step#v:}" | tr "~" " ")' > /tmp/vpad.cmd\"" ;;
      c:*) echo "timeout 2 sh -c 'echo $(echo "${step#c:}" | tr "~" " ") > /tmp/pocket-core.cmd'" ;;
      r:*) echo "python3 -c 'import socket; socket.socket(socket.AF_INET, socket.SOCK_DGRAM).sendto(b\"${step#r:}\", (\"127.0.0.1\", 55355))'" ;;
      x:*) echo "${step#x:}" | tr "~" " " ;;
      T:*) echo "sh $DIR/thread-cpu.sh $PROC ${step#T:}" ;;
      t:*) echo "for p in \$(pidof $PROC); do echo threads: \$(cat /proc/\$p/task/*/comm | sort | uniq -c | tr '\n' ' '); done" ;;
      s:*) echo "$WL grim -s 0.5 /tmp/hwshots/play-$TAG-$CORE-${step#s:}.png" ;;
      # Lists every emulator process before and after: bench.sh sums all processes of that name, so
      # another session's game running meanwhile would count too.
      # GPU: busy time the kernel accounts to the emulator's and sway's DRM clients (fdinfo
      # drm-engine-gpu), as a share of wall time at whatever clock the GPU ran.
      b:*) echo "echo emulators: \$(for p in \$(pidof pocket-core retroarch); do readlink /proc/\$p/exe; done)"
           echo "pw-top -b -n 2 | awk '/ \\+ / {print \$2, \$9, \$NF}' > /tmp/pw0"
           echo "g0=\$(gpu_ns $PROC); s0=\$(gpu_ns sway); t0=\$(date +%s.%N); m0=\$(python3 -c 'import time; print(time.monotonic())')"
           echo "sh $DIR/bench.sh ${step#b:} $PROC $TAG-$CORE"
           echo "g1=\$(gpu_ns $PROC); s1=\$(gpu_ns sway); t1=\$(date +%s.%N); m1=\$(python3 -c 'import time; print(time.monotonic())')"
           echo "python3 $DIR/frametime-stats.py \$m0 \$m1 /tmp/ft/*"
           echo "pw-top -b -n 2 | awk '/ \\+ / {print \$2, \$9, \$NF}' > /tmp/pw1; echo pipewire xruns \(id err name, before / after\): \$(cat /tmp/pw0) / \$(cat /tmp/pw1)"
           echo "echo \"\$g0 \$g1 \$s0 \$s1 \$t0 \$t1\" | awk '{w=(\$6-\$5)*1e9; print \"gpu busy: emulator \" sprintf(\"%.1f\", (\$2-\$1)/w*100) \"%  sway \" sprintf(\"%.1f\", (\$4-\$3)/w*100) \"%\"}'"
           echo "echo emulators: \$(for p in \$(pidof pocket-core retroarch); do readlink /proc/\$p/exe; done)" ;;
    esac
  done
  if [ -n "${SA:-}" ]; then
    echo "kill \$(pidof $SA) 2>/dev/null"
  elif [ -n "${RA:-}" ]; then
    echo "python3 -c 'import socket; socket.socket(socket.AF_INET, socket.SOCK_DGRAM).sendto(b\"QUIT\", (\"127.0.0.1\", 55355))'"
  else
    echo "timeout 2 sh -c 'echo quit > /tmp/pocket-core.cmd'"
  fi
  echo "n=0; while pidof $PROC >/dev/null && [ \$n -lt 20 ]; do sleep 1; n=\$((n+1)); done; pidof $PROC >/dev/null && echo \"HUNG: $PROC did not exit, killing it\"; kill -9 \$(pidof $PROC) 2>/dev/null; pkill -f '[v]pad.py'"
  echo "grep -v '^\[core\] \$' /tmp/hw.log | tail -25"
  echo "systemd-run --unit=pocketui /storage/pocket-ui/run-pocket-ui.sh >/dev/null 2>&1; true"
} >"$script"

mkdir -p "$ROOT/screenshots"
ssh "$HOST" "mkdir -p $DIR"
mkdir -p "$ROOT/target/ft"
[ "$ROOT/target/ft/frametime.so" -nt "$ROOT/deploy/frametime.c" ] || zig cc -target aarch64-linux-gnu.2.38 -shared -fPIC -O2 -o "$ROOT/target/ft/frametime.so" "$ROOT/deploy/frametime.c"
scp -q "$ROOT/target/aarch64-unknown-linux-gnu/release/pocket-core" "$ROOT/scripts/vpad.py" "$ROOT/deploy/bench.sh" "$ROOT/deploy/frametime-stats.py" "$ROOT/deploy/thread-cpu.sh" \
  "$ROOT/target/ft/frametime.so" "$script" "$HOST:$DIR/"
ssh "$HOST" "sh $DIR/$(basename "$script")"
scp -q "$HOST:/tmp/hwshots/*.png" "$ROOT/screenshots/" 2>/dev/null && ls "$ROOT"/screenshots/play-$TAG-$CORE-* || true
rm -f "$script"
