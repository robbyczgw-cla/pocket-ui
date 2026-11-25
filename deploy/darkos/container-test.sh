#!/bin/sh
# Container test of the dArkOS port on the card layouts players have (docs/DARKOS.md, "What is tested"). Written for this
# project. Run it as root INSIDE a container made from a dArkOS rootfs (aarch64, privileged, with the kernel's exfat module),
# with the folders
#   /pkg   the port as it is zipped: /pkg/ports/pocket-ui.sh and /pkg/ports/pocket-ui/
#   /roms-src   snes-test.sfc and gba-test.gba (any small test ROMs)
#   /img   roms-exfat.img and roms2-exfat.img (mkfs.exfat images, 128 MB or more)
# GLENV: extra environment for the port in a container without the Mali GPU (test-6: EGL_PLATFORM=surfaceless for Mesa's
# llvmpipe standing in for the blob, with the real image's cores; docs/DARKOS.md "PSP and Dreamcast").
# container-test.sh <layout>     layout: roms | roms2 | both | pad (the face buttons, standard layout, with a virtual pad: /ship/vpad.py, or VPAD_PY) | ui (Back to EmulationStation, Power off) | osd (the level bar over a game on the CPU copy) | power (virtual PMIC key, host isolation, crash recovery)
# The card is mounted the way dArkOS mounts it (exFAT, umask=0000, uid/gid of ark: fmask=0000,dmask=0000), the port is started
# as EmulationStation starts it (user ark, through ports/pocket-ui.sh and sudo), the screen is a plain file, the console a
# file. It checks the welcome screen, a game start from that card, Save & quit back to the shelf, a clean end (no mounts,
# no processes) and the failure screen for six ways the start can go wrong. Exit code 0 = all passed.
LAYOUT=${1:-both}
FAILS=0
ok() { echo "PASS  $*"; }
bad() { echo "FAIL  $*"; FAILS=$((FAILS + 1)); }
check() { # description, command...
  d=$1; shift; if "$@" >/dev/null 2>&1; then ok "$d"; else bad "$d"; fi
}
uid=$(id -u ark); gid=$(id -g ark); arkhome=$(getent passwd ark | cut -d: -f6)
status() { { tr '\t' '\n' </tmp/pocket-ui.status | sed -n "s/^$1=//p" | head -1; } 2>/dev/null; }
# The UI's command FIFO, never waiting for ever: with pocket-ui gone a plain echo blocks until something reads (0.6.0
# release run: the systems layout hung 6 h after the Dreamcast ended the UI). A missed command fails the next check.
ui_cmd() { timeout 5 sh -c 'echo "$1" >/tmp/pocket-ui.in' _ "$1" || echo "  (nobody reads '$1': pocket-ui is gone, place=$(status place))"; }
key() { ui_cmd "$1"; sleep "${2:-2}"; }
waitfor() { # what, value, seconds
  n=0; while [ $n -lt $(($3 * 2)) ]; do case "$(status "$1")" in "$2"*) return 0 ;; esac; sleep 0.5; n=$((n + 1)); done; return 1
}
card_mount() { # mount point, image
  umount "$1" 2>/dev/null; mkdir -p "$1"
  mount -t exfat -o loop,umask=0000,iocharset=utf8,noatime,nofail,uid=$uid,gid=$gid "$2" "$1" || return 1
  grep " $1 " /proc/mounts
}
install_port() { # card
  rm -rf "$1/ports" "$1/snes" "$1/gba"; mkdir -p "$1/ports" "$1/snes" "$1/gba"
  cp -r /pkg/ports/. "$1/ports/" 2>/dev/null; cp /roms-src/snes-test.sfc "$1/snes/"; cp /roms-src/gba-test.gba "$1/gba/"
  chown -R ark:ark "$1" 2>/dev/null; chmod +x "$1/ports/pocket-ui.sh" 2>/dev/null
}
es_start() { # card: as EmulationStation: user ark, the port's entry, sudo
  EP=$1/ports; : >/tmp/tty.out; head -c $((640 * 480 * 4)) /dev/zero >/tmp/fb.raw; chmod 666 /tmp/fb.raw /tmp/tty.out
  rm -rf "$EP/pocket-ui/logs" /tmp/pocket-ui.status /tmp/es-rc
  echo 'Defaults env_keep += "DARKOS_* EGL_PLATFORM POCKET_CORE_TEST"' >/etc/sudoers.d/zz-container-test; chmod 440 /etc/sudoers.d/zz-container-test
  su ark -c "DARKOS_FB=/tmp/fb.raw DARKOS_TTY=/tmp/tty.out DARKOS_HOLD=${HOLD:-1} $PADENV setsid '$EP/pocket-ui.sh' >/tmp/es-stdout.txt 2>&1 </dev/null; echo \$? >/tmp/es-rc" &
}
wait_end() { n=0; while [ $n -lt 80 ] && [ ! -e /tmp/es-rc ]; do sleep 0.5; n=$((n + 1)); done; }

# The first start, A on each page until the shelf: no fixed count of presses, the pages change between versions (0.6 added
# "How cozy should it be?" after the look; four presses left it there and START then ended the setup instead of opening
# Settings). Each press waits for its page to change, so a slow page never takes the next press for the shelf.
finish_setup() {
  n=0
  while [ $n -lt 8 ]; do
    p=$(status place); case "$p" in setup:*) ;; *) break ;; esac
    key a 0.3; m=0; while [ $m -lt 16 ] && [ "$(status place)" = "$p" ]; do sleep 0.5; m=$((m + 1)); done; sleep 1; n=$((n + 1))
  done
  sleep 1.5; echo "  (first start: $n presses, at $(status place))"
}
run_card() { # card
  C=$1; P=$C/ports/pocket-ui; echo "=== port and games on $C (exFAT)"
  install_port "$C"
  es_start "$C"
  waitfor place setup:welcome 15 && ok "$C: welcome screen" || bad "$C: no welcome screen (place=$(status place))"
  finish_setup
  key a; key a 3
  waitfor game playing 20 && ok "$C: a game from $C starts" || bad "$C: game did not start (place=$(status place))"
  check "$C: pocket-core runs the game with a dArkOS core" grep -q "launch gba $C\|launch gba /storage/roms/gba" "$P/logs/pocket-ui.log"
  check "$C: pocket-ui runs from the card's own folder" grep -q "board darkos-rg353m.*roms $C" "$P/logs/start.log"
  sleep 2; key home; key up; key a 3
  waitfor place shelf 10 && ok "$C: Save & quit is back on the shelf" || bad "$C: not back on the shelf (place=$(status place))"
  pkill -TERM -f "$P/app/start.sh direct" 2>/dev/null; wait_end
  check "$C: no pocket-ui processes left" test -z "$(pgrep -f "$P/app/bin/pocket")"
  check "$C: no mounts left" test "$(grep -c pocket-view /proc/mounts)" = 0
  check "$C: a normal end shows no failure screen" test ! -s /tmp/tty.out
}

fail_case() { # card, name, expected text in the console
  C=$1; P=$C/ports/pocket-ui; name=$2; want=$3
  : >/tmp/tty.out; rm -rf "$P/logs"
  HOLD=1 es_start "$C"; wait_end
  grep -q "$want" /tmp/tty.out && grep -q "Send a photo of this screen" /tmp/tty.out && ok "$C: failure screen: $name" || { bad "$C: failure screen: $name (console: $(tr -d '\033' </tmp/tty.out | head -c 200))"; }
}
run_failures() {
  C=$1; P=$C/ports/pocket-ui; echo "=== failure screens on $C"
  install_port "$C"; cp -a "$P/app/bin" /tmp/bin.keep; cp "$P/start.sh" /tmp/start.keep
  restore() { cp -a /tmp/bin.keep/. "$P/app/bin/"; cp /tmp/start.keep "$P/start.sh"; }
  rm -f "$P/app/bin/pocket-ui"; fail_case "$C" "program missing" "bin/pocket-ui is missing"; restore
  printf '#!/bin/sh\necho "test crash" >&2\nexit 7\n' >"$P/app/bin/pocket-ui"; fail_case "$C" "pocket-ui crashes" "code 7"; restore
  printf '#!/bin/sh\nexit 0\n' >"$P/app/bin/pocket-ui"; fail_case "$C" "pocket-ui ends at once" "stopped after"; restore
  printf '#!/bin/sh\necho "cannot open fb" >&2\nexit 1\n' >"$P/app/bin/pocket-fb"; fail_case "$C" "screen bridge dies" "pocket-fb"; restore
  mv "$P/app/bin/pocket-fb" "$P/app/bin/pocket-fb-real"
  printf '#!/bin/sh\nsleep 6\nexec "$(dirname "$0")/pocket-fb-real" "$@"\n' >"$P/app/bin/pocket-fb"
  chmod +x "$P/app/bin/pocket-fb"
  es_start "$C"
  waitfor place setup:welcome 20 && ok "$C: a slow bridge still reaches the welcome screen" || bad "$C: slow bridge never reached the welcome screen"
  check "$C: the launcher logged the slow bridge" grep -q 'screen bridge after 5 s: Wayland missing, IPC missing' "$P/logs/start.log"
  pkill -TERM -f "$P/app/start.sh direct" 2>/dev/null; wait_end
  rm -f "$P/app/bin/pocket-fb-real"; restore
  printf '#!/bin/sh\necho "positioned framebuffer writes: ${POCKET_FB_POSITIONED_WRITE:-off}"\nexit 1\n' >"$P/app/bin/pocket-fb"
  for model in 'R36S-V21' G80CA-MB 'Hardkernel ODROID-GO3' 'Anbernic RG353M'; do
    case "$model" in
      'Anbernic RG353M') device=RG353M expected=off ;;
      'Hardkernel ODROID-GO3') device=RG351MP expected=1 ;;
      *) device=RG353M expected=1 ;;
    esac
    PADENV="DARKOS_MODEL='$model' DARKOS_DEVICE=$device DARKOS_COMPATIBLE=''" HOLD=1 es_start "$C"
    wait_end
    check "$C: $model positioned framebuffer writes are $expected" grep -qx "positioned framebuffer writes: $expected" "$P/logs/pocket-fb.log"
  done
  PADENV="DARKOS_MODEL='Unknown board' DARKOS_DEVICE=GENERIC DARKOS_COMPATIBLE='vendor,board rockchip,rk3326'" HOLD=1 es_start "$C"
  wait_end
  check "$C: an unknown RK3326 board also enables positioned framebuffer writes" grep -qx 'positioned framebuffer writes: 1' "$P/logs/pocket-fb.log"
  restore
  rm -f "$P/start.sh"; fail_case "$C" "start.sh missing" "start.sh did not run"; restore
  # a card mounted noexec: start.sh is run by sh, the programs cannot be run
  mount -o remount,noexec "$C"; : >/tmp/tty.out
  head -c $((640 * 480 * 4)) /dev/zero >/tmp/fb.raw
  DARKOS_FB=/tmp/fb.raw DARKOS_TTY=/tmp/tty.out DARKOS_HOLD=1 sh "$P/start.sh" >/dev/null 2>&1
  mount -o remount,exec "$C"
  grep -q "cannot be run" /tmp/tty.out && ok "$C: failure screen: card mounted noexec" || bad "$C: failure screen: noexec (console: $(tr -d '\033' </tmp/tty.out | head -c 200))"
  restore
}

# The face buttons: a virtual pad that looks like the RG353M's, pressed by its evdev codes
# (304 = BTN_SOUTH, 305 = BTN_EAST, the bottom and the right button) with no system file read. On the
# welcome screen A moves on, B does nothing: so the screen shows which code is A.
stop_port() { # card: TERM to the app supervisor, then wait for its helpers and the shared boot wrapper to end
  pkill -TERM -f "$1/ports/pocket-ui/app/start.sh direct" 2>/dev/null; wait_end
  n=0; while [ $n -lt 40 ] && pgrep -f "$1/ports/pocket-ui/(app/bin/pocket|app/start.sh|start.sh)" >/dev/null; do sleep 0.5; n=$((n + 1)); done
}
pad_case() { # card, name, env for the port, code that must move the welcome screen on, code that must not, text of the log line
  C=$1; P=$C/ports/pocket-ui; name=$2; PADENV=$3; yes=$4; no=$5; line=$6
  for code in $yes $no; do
    install_port "$C"  # a fresh storage each time: the welcome screen is the first run's
    es_start "$C"
    waitfor place setup:welcome 15 || { bad "$C: $name: no welcome screen"; stop_port "$C"; return; }
    sleep 1; echo "tap $code" >/tmp/vpad.fifo; sleep 2
    moved=no; [ "$(status place)" != setup:welcome ] && moved=yes
    want=no; [ "$code" = "$yes" ] && want=yes
    [ "$moved" = "$want" ] && ok "$C: $name: code $code, moves on: $moved" || bad "$C: $name: code $code moved on: $moved, expected $want (place=$(status place))"
    grep -q "$line" "$P/logs/pocket-ui.log" && ok "$C: $name: log says '$line'" || bad "$C: $name: log lacks '$line' ($(grep 'pad map' "$P/logs/pocket-ui.log" | head -2))"
    stop_port "$C"
  done
}
run_pad() {
  C=$1; P=$C/ports/pocket-ui; echo "=== face buttons on $C"
  install_port "$C"
  python3 "${VPAD_PY:-/ship/vpad.py}" create /tmp/vpad.fifo >/tmp/vpad.out 2>&1 &
  VPAD=$!; n=0; while [ ! -s /tmp/vpad.fifo.node ] && [ $n -lt 50 ]; do sleep 0.2; n=$((n + 1)); done
  [ -s /tmp/vpad.fifo.node ] || { bad "virtual pad: $(cat /tmp/vpad.out)"; return; }
  pad_case "$C" "default layout (no system file is read)" "" 305 304 ""
  echo quit >/tmp/vpad.fifo; wait "$VPAD" 2>/dev/null
}

# The way back to EmulationStation (Settings > System, the quick settings) must end the port as a normal end (exit code 77 is not a
# failure), and Power off must reach the real systemctl: pocket-ui runs in a chroot, where systemctl itself does nothing, so the
# request goes through start.sh. The system's systemctl is a script that notes its arguments (DARKOS_SYSTEMCTL).
gone() { # card: nothing of the port is left, no failure screen, a clean end
  C=$1; P=$C/ports/pocket-ui
  n=0; while [ $n -lt 40 ] && pgrep -f "$P/(app/bin/pocket|app/start.sh|start.sh)" >/dev/null; do sleep 0.5; n=$((n + 1)); done
  check "$2: no processes left" test -z "$(pgrep -f "$P/(app/bin/pocket|app/start.sh|start.sh)")"
  check "$2: no mounts left" test "$(grep -c pocket-view /proc/mounts)" = 0
  check "$2: exit code 0" test "$(cat /tmp/es-rc 2>/dev/null)" = 0
  check "$2: no failure screen" test ! -s /tmp/tty.out
  check "$2: the log says it was on purpose" grep -q "left on purpose" "$P/logs/start.log"
}
to_shelf() { waitfor place setup:welcome 15 && ok "$1: welcome screen" || bad "$1: no welcome screen"; finish_setup; }
# Settings at the category NAME (START opens Settings; R1 goes to the next category): no fixed count of presses, the tree of
# categories changes between versions. Logs the places it passed for a failing run.
open_category() {
  key start 1.5; n=0
  while [ $n -lt 9 ] && [ "$(status place)" != "settings:List $1" ]; do echo "  (settings at $(status place))"; key r1 0.8; n=$((n + 1)); done
  [ "$(status place)" = "settings:List $1" ]
}
again_shelf() { waitfor place shelf 20 && ok "$1: shelf after a restart" || bad "$1: no shelf (place=$(status place))"; }
run_ui() {
  C=$1; P=$C/ports/pocket-ui; echo "=== way back to EmulationStation and power off on $C"
  printf '#!/bin/sh\necho "$@" >>/tmp/systemctl.calls\n' >/tmp/fake-systemctl; chmod 755 /tmp/fake-systemctl
  PADENV="DARKOS_SYSTEMCTL=/tmp/fake-systemctl "
  install_port "$C"; es_start "$C"; to_shelf "$C (Settings)"
  open_category System; check "$C: Settings > System is open" test "$(status place)" = "settings:List System"
  key a 4; wait_end
  gone "$C" "$C: Settings > System > Back to EmulationStation"
  # No logger runs on dArkOS: pocket-ui itself marks the boot's end (src/logs.rs mark_clean), else a power off from
  # EmulationStation made the next start say "The device shut down unexpectedly last time." (RG353M, 0.5.0).
  check "$C: Back to EmulationStation marks the boot clean" test -s "$P/storage/.config/pocket-ui/logs/clean-exit"
  install_port "$C"; es_start "$C"; to_shelf "$C (quick settings)"
  key home 1.5; key down 0.4; key down 0.4; key down 0.4; key down 0.4; key down 0.4; key a 4; wait_end
  gone "$C" "$C: quick settings > Back to EmulationStation"
  install_port "$C"; rm -f /tmp/systemctl.calls; es_start "$C"; to_shelf "$C (power off)"
  ui_cmd poweroff
  n=0; while [ $n -lt 60 ] && ! grep -q poweroff /tmp/systemctl.calls 2>/dev/null; do sleep 0.5; n=$((n + 1)); done
  check "$C: Power off reaches systemctl poweroff" grep -qx poweroff /tmp/systemctl.calls
  check "$C: the log says who asked" grep -q "power: systemctl poweroff" "$P/logs/start.log"
  L=$P/storage/.config/pocket-ui/logs
  check "$C: Power off marks the boot clean before its command" test -s "$L/clean-exit"
  stop_port "$C"
  # The next boot. The container keeps the host's boot id, so the state file is given an earlier boot's.
  next_boot() { sed -i 's/^id=.*/id=container-earlier-boot-'"$1"'/' "$L/boot"; es_start "$C"; again_shelf "$C ($2)"; }
  unexpected() { grep -q "without a clean shutdown" "$P/storage/.config/pocket-ui/crash.log" 2>/dev/null; }
  next_boot 1 "start after Power off"
  unexpected && bad "$C: 'shut down unexpectedly' after Power off" || ok "$C: no 'shut down unexpectedly' after Power off"
  # A power cut (pocket-ui killed, nothing cleans up) is what the notice is for.
  victim=$(pgrep -f "^$P/app/bin/pocket-ui$"); [ -n "$victim" ] && kill -KILL $victim; wait_end; stop_port "$C"
  next_boot 2 "start after a power cut"
  unexpected && ok "$C: a power cut still says the device shut down unexpectedly" || bad "$C: no notice after a power cut"
  stop_port "$C"
}

# Real evdev, no synthetic UI power command. An already-open host reader must receive nothing while the
# UI owns the key, including after an ogage-style reader restart, and receive presses again after exit/crash.
run_power() {
  # uinput is shared with the VM kernel: its own logind must be inhibited by the OUTER test runner.
  # Otherwise the intentional host-delivery checks below could shut down the entire build VM.
  [ "${POWER_TEST_VM_INHIBITED:-}" = 1 ] || { bad "power test requires a VM handle-power-key inhibitor (see docs/DARKOS.md)"; return; }
  C=$1; P=$C/ports/pocket-ui; echo "=== exclusive power key, short/long press and host restoration"
  rm -f /tmp/vpower.fifo.node /tmp/host-power* /tmp/systemctl.calls /tmp/suspend.saved
  python3 "${VPAD_PY:-/ship/vpad.py}" power /tmp/vpower.fifo >/tmp/vpower.out 2>&1 &
  VPOWER=$!; n=0
  while [ ! -s /tmp/vpower.fifo.node ] && [ $n -lt 50 ]; do sleep 0.2; n=$((n + 1)); done
  [ -s /tmp/vpower.fifo.node ] || { bad "virtual power key: $(cat /tmp/vpower.out)"; return; }
  NODE=$(cat /tmp/vpower.fifo.node)
  observe() {
    rm -f /tmp/host-power.ready
    python3 "${POWER_OBSERVER_PY:-/ship/power-observer.py}" "$NODE" /tmp/host-power >/tmp/observer.out 2>&1 & OBSERVER=$!
    n=0; while [ ! -e /tmp/host-power.ready ] && [ $n -lt 50 ]; do sleep 0.1; n=$((n + 1)); done
  }
  press() { echo "tap 116 ${1:-120}" >/tmp/vpower.fifo; sleep "${2:-2}"; }
  observe; press
  check "host sees power before pocket-ui" grep -q '^1$' /tmp/host-power
  : >/tmp/host-power
  cat >/tmp/fake-systemctl <<'SH'
#!/bin/sh
echo "$@" >>/tmp/systemctl.calls
if [ "$1" = suspend ]; then
  # At the actual suspend request, the auto state must already have been updated by the running core.
  find /roms/ports/pocket-ui/storage -type f -name '*.pstate.auto.z' -newer /tmp/power-save-before >/tmp/suspend.saved
  sleep 2
fi
SH
  chmod 755 /tmp/fake-systemctl
  PADENV="DARKOS_SYSTEMCTL=/tmp/fake-systemctl "
  install_port "$C"; es_start "$C"; to_shelf "$C (power key)"
  check "UI claims the PMIC node in the chroot" grep -q 'power key: claimed rk805 pwrkey .*exclusively' "$P/logs/pocket-ui.log"
  press 1200
  check "holding power opens the power menu" test "$(status cover)" = power
  check "holding power never suspends" test ! -s /tmp/systemctl.calls
  check "host does not react to the held key" test ! -s /tmp/host-power
  # Capture before dismissing; the menu must offer exactly the four port choices.
  cp /tmp/fb.raw /out/power-menu.raw
  key b 1
  kill "$OBSERVER"; wait "$OBSERVER" 2>/dev/null; observe
  key a; key a 3
  waitfor game playing 20 && ok "game starts with exclusive power key" || bad "game did not start"
  touch /tmp/power-save-before; sleep 1
  press 120 5
  n=0; while [ ! -s /tmp/systemctl.calls ] && [ $n -lt 40 ]; do sleep 0.5; n=$((n + 1)); done
  check "short power reaches host systemctl suspend" grep -qx suspend /tmp/systemctl.calls
  check "game auto state was saved before systemctl suspend" test -s /tmp/suspend.saved
  check "sleep confirms the new state" grep -q 'sleep: state saved' "$P/logs/pocket-ui.log"
  check "restarted host reader still sees no power" test ! -s /tmp/host-power
  # The fake suspend cannot freeze the VM clock; another key wakes its light-sleep state.
  key b 3
  press 1200
  check "holding power still opens the menu after sleep" test "$(status cover)" = power
  key down 0.3; key down 0.3; key down 0.3
  touch /tmp/es-save-before; sleep 1
  key a 3; wait_end
  gone "$C" "power menu > Back to EmulationStation"
  find "$P/storage" -type f -name '*.pstate.auto.z' -newer /tmp/es-save-before >/tmp/es.saved
  check "Back to ES saved the running game before exit" test -s /tmp/es.saved
  press
  check "host sees power again after Back to ES" grep -q '^1$' /tmp/host-power
  : >/tmp/host-power
  cp "$P/logs/pocket-ui.log" /out/power-ui.log; cp "$P/logs/start.log" /out/power-start.log
  install_port "$C"; es_start "$C"; to_shelf "$C (crash)"
  # Find the binary process, not sudo/the shell, and kill without giving its cleanup code a chance.
  victim=$(pgrep -f "^$P/app/bin/pocket-ui$")
  [ -n "$victim" ] && kill -KILL $victim
  wait_end
  press
  check "host sees power again after UI SIGKILL" grep -q '^1$' /tmp/host-power
  check "crash leaves no port processes" test -z "$(pgrep -f "$P/(app/bin/pocket|app/start.sh|start.sh)")"
  check "crash leaves no port mounts" test "$(grep -c pocket-view /proc/mounts)" = 0
  kill "$OBSERVER"; wait "$OBSERVER" 2>/dev/null
  echo quit >/tmp/vpower.fifo; wait "$VPOWER" 2>/dev/null
}

# The real UI selects installed cores and crosses the chroot boundary for native guests.
# Only the three native ES command templates are instrumented; their defaults remain those of the image.
run_systems() {
  C=$1; [ -n "${ONLY_GLFAIL:-}" ] && { run_glfail "$C"; return; }
  P=$C/ports/pocket-ui; echo "=== extra systems, PSP/Dreamcast on pocket-core and native handover on $C"
  install_port "$C"
  # Docker gives /dev/shm 64 MB; dArkOS's is systemd's RAM-sized tmpfs. PPSSPP keeps the PSP's memory there (SIGBUS at 64 MB).
  [ "$(df -k /dev/shm | awk 'NR==2{print $2}')" -ge 262144 ] || mount -o remount,size=512m /dev/shm
  rm -rf "$C/bios" "$C/psp" "$C/dreamcast"; mkdir -p "$C/bios"
  for pair in 'ngp NgpAsteroids.ngp' 'ngpc NgpAsteroids.ngp' 'psp Psp2048.pbp' 'dreamcast 240pSuite.cdi' 'gc native.iso'; do
    set -- $pair; mkdir -p "$C/$1/.pocket-ui-test"
    case $1 in gc) printf 'native-fixture-game\n' >"$C/$1/$2" ;; *) cp "/ship/roms/$2" "$C/$1/$2" ;; esac
    cp "$C/$1/$2" "$C/$1/.pocket-ui-test/$2"
  done
  chown -R ark:ark "$C" 2>/dev/null
  cat >/tmp/darkos-native-fixture.sh <<'SH'
#!/bin/sh
printf '%s\n' "$(id -u)" "$HOME" "$PATH" "$@" >"$3.native-launched"
printf 'native save\n' >"$3.native-save"
sleep 8
SH
  chmod 755 /tmp/darkos-native-fixture.sh
  python3 -I - /tmp/darkos-test6-es.cfg <<'PYCFG'
import sys,xml.etree.ElementTree as E
r=E.parse('/etc/emulationstation/es_systems.cfg')
for n in r.getroot().findall('system'):
 if n.findtext('name') in ('ppsspp','dreamcast','Gamecube'):
  n.find('command').text='/bin/sh /tmp/darkos-native-fixture.sh %EMULATOR% %CORE% %ROM%'
r.write(sys.argv[1])
PYCFG
  PADENV="DARKOS_ES_SYSTEMS=/tmp/darkos-test6-es.cfg POCKET_CORE_TEST=1 ${GLENV:-}"
  es_start "$C"; to_shelf "$C (extra systems)"
  check "NGP/NGPC, PSP, Dreamcast and GameCube enter the shelf inventory" python3 -I -c 'import json;d=json.load(open("/tmp/pocket-ui-darkos/systems.json"));assert all(s in d for s in ["ngp","ngpc","psp","dreamcast","gamecube"])'
  for sys in ngp ngpc; do
    ui_cmd "play $sys /storage/roms/$sys/.pocket-ui-test/NgpAsteroids.ngp"
    waitfor game playing 20 && ok "$sys starts through pocket-core" || bad "$sys did not start"
    check "$sys selected the original mednafen_ngp core" grep -q "launch $sys .*emulator pocket-core, core mednafen_ngp" "$P/logs/pocket-ui.log"
    key home; key up; key a 3
    waitfor place shelf 10 && ok "$sys Save & quit returns to the shelf" || bad "$sys did not return"
  done
  for spec in 'psp psp Psp2048.pbp ppsspp' 'dreamcast dreamcast 240pSuite.cdi flycast'; do
    set -- $spec; play_3d "$C" "$1" "$2" "$3" "$4"
  done
  check "PSP fonts and tables copied from dArkOS's PPSSPP into bios/PPSSPP" test -s "$C/bios/PPSSPP/ppge_atlas.zim" -a -d "$C/bios/PPSSPP/flash0/font"
  check "PSP memory stick is dArkOS's standalone PPSSPP folder, made as ppsspp.sh makes it" test -s "$C/psp/ppsspp/PSP/SYSTEM/controls.ini"
  check "bios/dc exists on the card for the Dreamcast's memory cards" test -d "$C/bios/dc"
  check "games stay read only in the view" sh -c "! touch /tmp/pocket-ui-darkos/view/storage/roms/psp/x 2>/dev/null"
  # GameCube stays on dArkOS's Dolphin.
  native_case "$C" gamecube gc native.iso
  stop_port "$C"
  # The dArkOS route as the fallback: without the 64-bit cores PSP and Dreamcast use dArkOS's own commands.
  rm -rf /tmp/darkos-test6-cores; mkdir -p /tmp/darkos-test6-cores
  for f in "$arkhome"/.config/retroarch/cores/*_libretro.so; do case $f in *flycast_libretro.so|*ppsspp_libretro.so) ;; *) ln -s "$f" /tmp/darkos-test6-cores/ ;; esac; done
  for pair in 'psp native.pbp' 'dreamcast native.cdi'; do set -- $pair; printf 'native-fixture-game\n' >"$C/$1/.pocket-ui-test/$2"; done
  PADENV="DARKOS_ES_SYSTEMS=/tmp/darkos-test6-es.cfg DARKOS_CORES=/tmp/darkos-test6-cores ${GLENV:-}"
  es_start "$C"; again_shelf "$C (no 3D cores)"
  native_case "$C" psp psp native.pbp
  native_case "$C" dreamcast dreamcast native.cdi
  stop_port "$C"
  # A start that fails before its first picture (no GL context) retries once with dArkOS's command.
  PADENV="DARKOS_ES_SYSTEMS=/tmp/darkos-test6-es.cfg DARKOS_TEST_EGL=/nonexistent/libEGL.so.1 ${GLENV:-}"
  es_start "$C"; again_shelf "$C (no GL context)"
  # The real game this time: the core opens it, only the context fails. A fresh copy: a game with saved moments is never
  # handed over automatically (`fallback`), the player is told and chooses Runs on instead.
  cp /ship/roms/240pSuite.cdi "$C/dreamcast/.pocket-ui-test/fresh.cdi"
  native_case "$C" dreamcast dreamcast fresh.cdi
  check "the failed pocket-core start was the GL context" grep -q "GL context: libEGL.so.1\|GL context: .*nonexistent" "$P/logs/pocket-ui.log"
  stop_port "$C"
  check "native handover leaves no helpers" test -z "$(pgrep -f '[/]tmp/darkos-native-fixture.sh')"
  check "extra systems leave no port processes" test -z "$(pgrep -f "$P/(app/bin/pocket|app/start.sh|start.sh)")"
  check "no view mounts left" test "$(grep -c pocket-view /proc/mounts)" = 0
  for folder in ngp ngpc psp dreamcast gc bios; do rm -rf "$C/$folder"; done
}

run_glfail() { # only the "no GL context" fallback, on a card prepared by a full run_systems
  C=$1; P=$C/ports/pocket-ui; install_port "$C"; rm -rf "$C/dreamcast"; mkdir -p "$C/dreamcast/.pocket-ui-test"
  cp /ship/roms/240pSuite.cdi "$C/dreamcast/"; cp /ship/roms/240pSuite.cdi "$C/dreamcast/.pocket-ui-test/"; chown -R ark:ark "$C"
  PADENV="DARKOS_ES_SYSTEMS=/tmp/darkos-test6-es.cfg DARKOS_TEST_EGL=/nonexistent/libEGL.so.1 ${GLENV:-}"
  es_start "$C"; to_shelf "$C (no GL context)"; waitfor place shelf 10; sleep 2
  native_case "$C" dreamcast dreamcast 240pSuite.cdi
  check "the failed pocket-core start was the GL context" grep -q "GL context: libEGL.so.1\|GL context: .*nonexistent" "$P/logs/pocket-ui.log"
  stop_port "$C"; rm -rf "$C/dreamcast"
}

# A 3D game on pocket-core: first frames through the core's GLES context, 8 s running, a real picture, a slot saved and
# loaded, the pause menu, Save & quit back to the shelf.
core_cmd() { timeout 5 sh -c 'echo "$1" >/tmp/pocket-core.cmd' _ "$1"; }  # never hangs on a FIFO without its reader
play_3d() { # card, system, folder, file, core
  C=$1; P=$C/ports/pocket-ui; sys=$2; folder=$3; file=$4; core=$5
  cp /tmp/fb.raw /tmp/fb.shelf
  ui_cmd "play $sys /storage/roms/$sys/.pocket-ui-test/$file"
  waitfor game playing 60 && ok "$sys starts through pocket-core" || bad "$sys did not start (place=$(status place) game=$(status game))"
  check "$sys selected dArkOS's 64-bit $core core" grep -q "launch $sys .*emulator pocket-core, core $core" "$P/logs/pocket-ui.log"
  check "$sys got a GLES context, read back to the CPU" grep -q "for the core on the vendor's EGL" "$P/logs/pocket-ui.log"
  sleep 8
  check "$sys still running after 8 s" sh -c "grep -q 'play=playing' /tmp/pocket-core.status && pgrep -f '[a]pp/bin/pocket-core' >/dev/null"
  cp /tmp/fb.raw "/out/test6-$sys.raw" 2>/dev/null
  check "$sys shows the game's picture (a third of the screen differs from the shelf, not one colour)" python3 -I -c 'a=open("/tmp/fb.raw","rb").read();b=open("/tmp/fb.shelf","rb").read();n=len(a)//4;d=sum(a[i*4:i*4+3]!=b[i*4:i*4+3] for i in range(0,n,5));assert d*5>n/3 and len({a[i*4:i*4+3] for i in range(0,n,7)})>4'
  if [ "$sys" = psp ]; then
    # Psp2048 saves its board after a move. Slots alone never exercise the
    # core's own memory-stick I/O (not RETRO_MEMORY_SAVE_RAM).
    for button in DOWN LEFT UP RIGHT DOWN LEFT UP RIGHT; do
      core_cmd "press $button 100"; sleep 0.4
    done
    sleep 1
    check "PSP in-game save reaches the standalone memory stick" test -s "$C/$folder/ppsspp/PSP/SAVEDATA/PSP2048/savedata.bin"
  fi
  core_cmd "save 2"; n=0
  while [ $n -lt 30 ] && [ -z "$(find "$P/storage" -name '*.pstate1' 2>/dev/null | grep -i "${file%.*}")" ]; do sleep 0.5; n=$((n + 1)); done
  check "$sys saved slot 2" test -n "$(find "$P/storage" -name '*.pstate1' -size +1k 2>/dev/null | grep -i "${file%.*}")"
  core_cmd "load 2"; sleep 4
  check "$sys loaded slot 2" grep -q "loaded .*${file%.*}.*pstate1" "$P/logs/pocket-ui.log"
  check "$sys still running after the load" grep -q 'play=playing' /tmp/pocket-core.status
  key home
  waitfor game paused 10 && ok "$sys pause menu opens" || bad "$sys no pause menu (place=$(status place) game=$(status game))"
  check "$sys pause menu is in front" sh -c "tr '\t' '\n' </tmp/pocket-ui.status | grep -q '^place=pause:'"
  key up; key a 3
  waitfor place shelf 20 && ok "$sys Save & quit returns to the shelf" || bad "$sys did not return (place=$(status place))"
  waitfor game none 10 && ok "$sys game ended" || bad "$sys still running"
}

native_case() { # card, system, folder, file: dArkOS's own command, as ark, outside the chroot, then the shelf again
  C=$1; sys=$2; folder=$3; file=$4; record=$C/$folder/.pocket-ui-test/$file.native-launched
  rm -f "$record"
  ui_cmd "play $sys /storage/roms/$sys/.pocket-ui-test/$file"
  n=0; while [ $n -lt 60 ] && [ ! -s "$record" ]; do sleep 0.5; n=$((n+1)); done
  check "$sys runs dArkOS's command as ark outside the chroot" test "$(head -1 "$record" 2>/dev/null)" = "$uid"
  check "$sys uses ark's original home" test "$(sed -n '2p' "$record" 2>/dev/null)" = "$arkhome"
  check "$sys uses the host tools rather than the port stand-ins" test "$(sed -n '3p' "$record" 2>/dev/null)" = /usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin
  check "$sys received the real ROM path" grep -qx "$C/$folder/.pocket-ui-test/$file" "$record"
  waitfor game none 25 && ok "$sys ends and returns to the shelf" || bad "$sys stayed running"
  waitfor place shelf 5 && ok "$sys shelf visible again" || bad "$sys no shelf"
}

# A signed synthetic next app validates, installs, fails before its first frame, and the real boot takes the installed build back.
# The fixture is prepared on the build host with the project signing key; that key never enters the container/card.
run_updates() {
  C=$1; P=$C/ports/pocket-ui; echo "=== signed update from Settings and automatic rollback"
  install_port "$C"
  installed=$(cat "$P/app/RELEASE")
  # UPDATE_FIXTURE: a signed synthetic next app whose component list matches the installed build (prepared outside).
  F=${UPDATE_FIXTURE:-/ship/test5/update-fixture}
  [ -s "$F/update.json.sig" ] || { bad "signed update fixture missing"; return; }
  rm -rf /tmp/darkos-test5-update-fixture; mkdir -p /tmp/darkos-test5-update-fixture
  cp "$F"/update.json "$F"/update.json.sig "$F"/next-app.tar.gz /tmp/darkos-test5-update-fixture/
  printf 'file:///tmp/darkos-test5-update-fixture/update.json\n' >"$P/update-url"
  mkdir -p "$P/storage/.config/pocket-ui"
  printf 'keep-save\n' >"$P/storage/keep-save"
  es_start "$C"; to_shelf "$C (updater)"
  # System's rows on dArkOS: Back to EmulationStation, then Updates.
  open_category System; key down 0.4; key a 1
  check "Updates is visible on dArkOS" test "$(status place)" = settings:Updates
  waitfor update checked 15 && ok "separate signed port manifest verified" || bad "manifest check: $(status update)"
  key a 0.5
  check "update confirmation belongs to the update itself" test "$(status update)" = confirming
  key a 1
  n=0; while [ $n -lt 100 ] && ! grep -q 'did not start.*back to' "$P/logs/boot.log" 2>/dev/null; do sleep 0.5; n=$((n+1)); done
  check "Settings update activates then rolls back its deliberately broken next app" grep -q 'did not start.*back to' "$P/logs/boot.log"
  waitfor place shelf 15 && ok "restored $installed shows the shelf again" || bad "restored UI: $(status place)"
  check "the installed app ($installed) is restored" grep -qx "$installed" "$P/app/RELEASE"
  next=$(python3 -I -c 'import json,sys;print(json.load(open(sys.argv[1]))["app"]["version"])' "$F/update.json")
  check "the failed version ($next) is retained separately" grep -qx "$next" "$P/app.failed/RELEASE"
  check "persistent saves survive the update and rollback" grep -qx keep-save "$P/storage/keep-save"
  check "no pending update remains" test ! -e "$P/storage/pocket-ui/update-pending"
  cp "$P/logs/boot.log" /out/update-rollback-boot.log
  stop_port "$C"
}

# The tester's real path: test-5 installed by hand, games and saves on the card, then Settings > Updates to the next build
# from a signed list (UPDATE56_ZIP: the installed build, UPDATE56_DIR: the list and only the packages that changed; run it
# with the container's network off, so a reused component cannot come from anywhere else).
run_update56() {
  C=$1; P=$C/ports/pocket-ui; echo "=== update from the installed build to the next one through Settings on $C"
  rm -rf /tmp/pkg-old; mkdir -p /tmp/pkg-old/ports
  python3 -I -m zipfile -e "${UPDATE56_ZIP:-/ship/test5/pocket-ui-darkos-test-5.zip}" /tmp/pkg-old/ports || { bad "old zip"; return; }
  rm -rf "$C/ports" "$C/snes" "$C/gba" "$C/psp" "$C/bios"; mkdir -p "$C/ports" "$C/snes" "$C/gba" "$C/psp/.pocket-ui-test" "$C/bios"
  cp -r /tmp/pkg-old/ports/. "$C/ports/"; cp /roms-src/snes-test.sfc "$C/snes/"; cp /roms-src/gba-test.gba "$C/gba/"
  cp /ship/roms/Psp2048.pbp "$C/psp/"; cp /ship/roms/Psp2048.pbp "$C/psp/.pocket-ui-test/"  # a folder without a visible game is not on the shelf
  chown -R ark:ark "$C" 2>/dev/null
  old=$(cat "$P/app/RELEASE")
  [ -s "${UPDATE56_DIR:-/ship/test6/fixture56}/update.json.sig" ] || { bad "signed update list missing"; return; }
  rm -rf /tmp/darkos-test6-update; mkdir -p /tmp/darkos-test6-update; cp "${UPDATE56_DIR:-/ship/test6/fixture56}"/* /tmp/darkos-test6-update/
  new=$(python3 -I -c 'import json;print(json.load(open("/tmp/darkos-test6-update/update.json"))["app"]["version"])')
  printf 'file:///tmp/darkos-test6-update/update.json\n' >"$P/update-url"
  PADENV="${GLENV:-}"
  es_start "$C"; to_shelf "$C ($old)"
  waitfor place shelf 15 || bad "$old: no shelf after the welcome screen ($(status place))"; sleep 3
  # A real save before the update: GBA Save & quit leaves its Continue state in storage/. Keys only while the game runs:
  # on the shelf the same keys would choose Back to EmulationStation.
  # The same presses as run_card (the shelf's first game, the GBA one): the published test-5 is driven only by keys here.
  key a; key a 3
  if waitfor game playing 30; then
    ok "$old plays a game"; sleep 2; key home; key up; key a 3
    waitfor place shelf 10 && ok "$old Save & quit" || bad "$old not back on the shelf"
  else
    bad "$old game did not start (place=$(status place))"
  fi
  states_before=$(find "$P/storage" -type f \( -name '*.pstate*' -o -name '*.srm' -o -name '*.sav' \) | sort | xargs -r sha256sum)
  check "$old left a saved game in storage/" test -n "$states_before"
  printf 'keep-me\n' >"$P/storage/keep-save"
  # the installed (old) build's Settings: test-5/test-6 (0.4.0-darkos-test-*) have the old tree with a category list first; every later
  # build has main's (category by name, System = Back to EmulationStation, then Updates)
  case $old in
    0.4.0-darkos-test-*)
      key start; key down 0.4; key down 0.4; key down 0.4; key down 0.4; key down 0.4; key a 1
      key down 0.4; key down 0.4; key down 0.4; key down 0.4; key down 0.4; key a 1 ;;
    *) open_category System; key down 0.4; key a 1 ;;
  esac
  check "Updates is open in $old" test "$(status place)" = settings:Updates
  waitfor update checked 20 && ok "$old verified the signed list of $new" || bad "list check: $(status update) $(status update.version)"
  key a 0.5
  check "the confirmation is the update's" test "$(status update)" = confirming
  key a 1
  n=0; while [ $n -lt 240 ] && ! grep -qx "$new" "$P/app/RELEASE" 2>/dev/null; do sleep 0.5; n=$((n+1)); done
  check "$new is the installed app" grep -qx "$new" "$P/app/RELEASE"
  check "$old is kept as app.previous" grep -qx "$old" "$P/app.previous/RELEASE"
  # A newer version first shows its "new in" cards over the shelf (app_tour): B closes them, as a player would.
  n=0; while [ $n -lt 120 ] && [ "$(status place)" != shelf ] && [ "$(status place)" != tour ]; do sleep 0.5; n=$((n + 1)); done
  if [ "$(status place)" = tour ]; then ok "$new shows what is new first"; n=0; while [ $n -lt 12 ] && [ "$(status place)" = tour ]; do key b 1; n=$((n + 1)); done; fi
  waitfor place shelf 20 && ok "$new comes up on its own and shows the shelf" || bad "$new UI: $(status place)"
  n=0; while [ $n -lt 60 ] && [ "$(status update.milestone)" != confirmed ] && [ "$(status update.milestone)" != stable ] && [ "$(status update.milestone)" != not-pending ]; do sleep 0.5; n=$((n+1)); done
  check "$new passed the first-picture check (milestone $(status update.milestone))" sh -c "test \"$(status update.milestone)\" != pending"
  check "no fall back to $old" sh -c "! grep -q 'did not start.*back to' '$P/logs/boot.log'"
  check "the version in About is $new" grep -q "$new" "$P/logs/pocket-ui.log"
  check "saves and states from $old are unchanged" sh -c "printf '%s\n' \"\$1\" | sha256sum -c --quiet" _ "$states_before"
  check "storage/ survives the update" grep -qx keep-me "$P/storage/keep-save"
  check "only changed packages were needed (the list holds $(ls /tmp/darkos-test6-update | grep -c darkos-asset) of the app's)" test "$(ls /tmp/darkos-test6-update | grep -c darkos-asset)" -lt 22
  cp "$P/logs/boot.log" /out/test6-update56-boot.log 2>/dev/null; grep -iE 'update|asset|component|reus' "$P/logs/pocket-ui.log" >/out/test6-update56-ui.log 2>/dev/null
  # The updated build plays PSP on pocket-core and comes back.
  sleep 3; ui_cmd "play psp /storage/roms/psp/.pocket-ui-test/Psp2048.pbp"
  if waitfor game playing 60; then
    ok "$new: PSP starts"
    check "$new: PSP runs on pocket-core" grep -q "launch psp .*emulator pocket-core, core ppsspp" "$P/logs/pocket-ui.log"
    sleep 3; key home; key up; key a 3
    waitfor place shelf 20 && ok "$new: PSP Save & quit" || bad "$new: PSP not back"
  else
    bad "$new: PSP did not start (place=$(status place))"
  fi
  stop_port "$C"
  check "no port processes left after the update" test -z "$(pgrep -f "$P/(app/bin/pocket|app/start.sh|start.sh)")"
  check "no view mounts left after the update" test "$(grep -c pocket-view /proc/mounts)" = 0
}

# Over the game on the CPU copy (pocket-fb without --gl, as on dArkOS): Vol+ during a game shows the level bar at the top
# centre of the screen file and it is gone again after its 1.4 s; the first game's note stands at the bottom. Screen files
# go to /out (osd-before/during/after.raw, 640x480 XRGB) for the renders.
fbdiff() { # a, b, x0, y0, x1, y1: pixels that differ inside the rectangle
  python3 -I -c 'import sys
a,b=(open(f,"rb").read() for f in sys.argv[1:3]); x0,y0,x1,y1=map(int,sys.argv[3:7])
print(sum(1 for y in range(y0,y1) for x in range(x0,x1) if a[(y*640+x)*4:(y*640+x)*4+3]!=b[(y*640+x)*4:(y*640+x)*4+3]))' "$@"
}
run_osd() {
  C=$1; P=$C/ports/pocket-ui; echo "=== level bar over a game on the CPU copy, $C"
  install_port "$C"; PADENV="${GLENV:-}"
  es_start "$C"; to_shelf "$C"
  key a; key a 3
  if ! waitfor game playing 20; then bad "$C: game did not start (place=$(status place))"; stop_port "$C"; return; fi
  check "$C: the game is drawn by the CPU copy" grep -q "shaders and frames off, the CPU copy" "$P/logs/pocket-ui.log"
  sleep 7  # the first game's note (6 s) is gone
  mkdir -p /out; cp /tmp/fb.raw /out/osd-before.raw
  key volup 0.5; cp /tmp/fb.raw /out/osd-during.raw
  sleep 2.5; cp /tmp/fb.raw /out/osd-after.raw
  bar=$(fbdiff /out/osd-before.raw /out/osd-during.raw 150 14 490 52); gone=$(fbdiff /out/osd-before.raw /out/osd-after.raw 150 14 490 52)
  below=$(fbdiff /out/osd-before.raw /out/osd-during.raw 0 120 640 480)
  echo "  bar area: $bar pixels changed with Vol+, $gone after 2.5 s; $below changed below it"
  check "$C: Vol+ shows the level bar at the top centre" test "$bar" -gt 3000
  check "$C: the bar is gone again" test "$gone" -lt $((bar / 4))
  key home; key up; key a 3
  waitfor place shelf 10 && ok "$C: Save & quit is back on the shelf" || bad "$C: not back on the shelf (place=$(status place))"
  stop_port "$C"
}

case "$LAYOUT" in systems) card_mount /roms2 /img/roms2-exfat.img || bad "mount /roms2"; run_systems /roms2; echo "=== $FAILS failed"; [ "$FAILS" = 0 ]; exit ;; esac
case "$LAYOUT" in update56) [ "$(df -k /dev/shm | awk 'NR==2{print $2}')" -ge 262144 ] || mount -o remount,size=512m /dev/shm; card_mount /roms /img/roms-exfat.img || bad "mount /roms"; run_update56 /roms; echo "=== $FAILS failed"; [ "$FAILS" = 0 ]; exit ;; esac
case "$LAYOUT" in updates) card_mount /roms /img/roms-exfat.img || bad "mount /roms"; run_updates /roms; echo "=== $FAILS failed"; [ "$FAILS" = 0 ]; exit ;; esac
case "$LAYOUT" in power) card_mount /roms /img/roms-exfat.img || bad "mount /roms"; run_power /roms; echo "=== $FAILS failed"; [ "$FAILS" = 0 ]; exit ;; esac

case "$LAYOUT" in ui) card_mount /roms /img/roms-exfat.img || bad "mount /roms"; run_ui /roms; echo "=== $FAILS failed"; [ "$FAILS" = 0 ]; exit ;; esac
case "$LAYOUT" in osd) card_mount /roms /img/roms-exfat.img || bad "mount /roms"; run_osd /roms; echo "=== $FAILS failed"; [ "$FAILS" = 0 ]; exit ;; esac
case "$LAYOUT" in pad) card_mount /roms /img/roms-exfat.img || bad "mount /roms"; run_pad /roms; echo "=== $FAILS failed"; [ "$FAILS" = 0 ]; exit ;; esac
case "$LAYOUT" in roms|both) card_mount /roms /img/roms-exfat.img || bad "mount /roms" ;; esac
case "$LAYOUT" in roms2|both) card_mount /roms2 /img/roms2-exfat.img || bad "mount /roms2" ;; esac
case "$LAYOUT" in roms|both) run_card /roms; run_failures /roms ;; esac
case "$LAYOUT" in roms2|both) run_card /roms2; run_failures /roms2 ;; esac
echo "=== $FAILS failed"
[ "$FAILS" = 0 ]
