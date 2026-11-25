#!/bin/sh
# pocket-ui as a dArkOS port (prototype, docs/DARKOS.md). Written for this project; nothing here comes from dArkOS or
# EmulationStation. Started by ../pocket-ui.sh (root, through sudo).
#
# 1. checks the screen (/dev/fb0, 640 wide), the board (device tree `model`), our programs and the system's cores
# 2. runs the rest in a mount namespace of its own (`unshare -m`): the /storage pocket-ui and pocket-core expect is a view
#    in /tmp, made of the system's folders plus our storage/; the player's games in /roms/<system> are bound READ ONLY.
#    (bios/dc and bios/PPSSPP stay writable for Dreamcast memory cards and PSP fonts; the PSP memory stick is <card>/psp/ppsspp)
#    The mounts exist only inside that namespace and end with it, also after a crash. Nothing is written outside this
#    folder except /tmp/pocket-ui-darkos and those three PSP/Dreamcast folders, where dArkOS's own emulators write too.
# 3. stops the text console drawing on the screen (KD_GRAPHICS on tty1), starts pocket-fb (Wayland on /dev/fb0) and pocket-ui
# 4. at the end: console back to text, a screen left dark lit again, our helpers stopped; EmulationStation comes back.
# If anything goes wrong the failure screen (fail_screen below) shows the reason and the last log lines on the console for
# 15 s, the way dArkOS's own scripts and PortMaster print to tty1 (setfont + printf): no program of ours is needed for it.
# Test hooks (container, docs/DARKOS.md): DARKOS_FB (a plain file as the screen), DARKOS_MODEL, DARKOS_ROMS, DARKOS_CORES,
# DARKOS_TTY, DARKOS_RUN, DARKOS_DEVICE (dArkOS's .DEVICE name), DARKOS_ASOUND (its /etc/asound.conf), DARKOS_ALSA,
# DARKOS_HOLD (seconds the failure screen stays, 15), DARKOS_TEST_EGL (another libEGL for the 3D cores' context: a missing one
# proves the fallback to dArkOS's command).
ROOT=$(cd "$(dirname "$0")" && pwd)
DATA=$(dirname "$ROOT")
[ "$(id -u)" = 0 ] || exec sudo -n /bin/sh "$ROOT/start.sh" "$@"
RUN=${DARKOS_RUN:-/tmp/pocket-ui-darkos}
FB=${DARKOS_FB:-/dev/fb0}
# The games folder is the one the port was copied into: /roms/ports/… → /roms, /roms2/ports/… (second card) → /roms2.
ROMS=${DARKOS_ROMS:-$(dirname "$(dirname "$DATA")")}
# dArkOS's RetroArch cores, in its user ark's home folder (read only, by path).
ARK_HOME=$(getent passwd ark 2>/dev/null | cut -d: -f6)
CORES=${DARKOS_CORES:-${ARK_HOME:-/nonexistent}/.config/retroarch/cores}
TTY=${DARKOS_TTY:-/dev/tty1}
LOG=$DATA/logs/start.log
mkdir -p "$DATA/logs" "$DATA/storage/.config" "$DATA/storage/roms/savestates" "$DATA/storage/roms/bios" "$DATA/storage/saves" "$RUN"
log() { printf '%s %s\n' "$(date +%H:%M:%S)" "$*" >>"$LOG"; }
FBMODE=; [ -f "$FB" ] && FBMODE="--mode 640x480x32"

PIDS=
FBPID=
BRIDGE_STUCK=
kd() { # the console's mode: 1 graphics (the console draws nothing), 0 text
  [ -c "$TTY" ] && command -v python3 >/dev/null 2>&1 && python3 -I -c 'import fcntl,os,sys; fd=os.open(sys.argv[1], os.O_RDWR); fcntl.ioctl(fd, 0x4B3A, int(sys.argv[2]))' "$TTY" "$1" 2>>"$LOG"
}
stop_helpers() {
  for p in $PIDS; do kill "$p" 2>/dev/null; done
  n=0
  while [ $n -lt 20 ]; do
    alive=0; for p in $PIDS; do kill -0 "$p" 2>/dev/null && alive=1; done
    [ $alive = 0 ] && break
    sleep 0.2; n=$((n + 1))
  done
  for p in $PIDS; do kill -KILL "$p" 2>/dev/null; done
  if [ -n "$FBPID" ] && kill -0 "$FBPID" 2>/dev/null; then
    # Give SIGKILL time to take effect without wait(1), which cannot reap a task stuck in the kernel.
    n=0
    while [ "$n" -lt 5 ] && kill -0 "$FBPID" 2>/dev/null; do
      sleep 0.2
      n=$((n + 1))
    done
    if kill -0 "$FBPID" 2>/dev/null; then
      BRIDGE_STUCK=1
      log "screen bridge pid $FBPID remains after SIGKILL; skipping console and backlight operations"
    fi
  fi
  PIDS= FBPID=
}

# The failure screen: the reason, what to do and the last log lines, on the console for DARKOS_HOLD seconds (15), then
# EmulationStation comes back. Written with printf and setfont only, as dArkOS's own scripts do, so it also works when our
# programs are missing, cannot run or crashed. The Handyfoto of this screen is all a tester has to send.
fail_screen() { # reason, hint
  stop_helpers
  [ -z "$BRIDGE_STUCK" ] || return 0
  kd 0
  [ -w "$TTY" ] || return 0
  for f in Lat7-TerminusBold20x10 Lat7-Terminus16 Lat7-TerminusBold16; do
    [ -f "/usr/share/consolefonts/$f.psf.gz" ] && { setfont -C "$TTY" "$f" >/dev/null 2>&1; break; }
  done
  size=$(stty -F "$TTY" size 2>/dev/null); rows=${size% *}; cols=${size#* }
  case "$rows" in ''|*[!0-9]*) rows=24 ;; esac
  case "$cols" in ''|*[!0-9]*) cols=64 ;; esac
  [ "$cols" -ge 20 ] || cols=64
  avail=$((rows - 8)); [ $avail -ge 4 ] || avail=4
  n1=$((avail * 6 / 14)); n2=$((avail * 5 / 14)); n3=$((avail - n1 - n2))
  [ $n1 -ge 1 ] || n1=1
  w=$((cols - 1))
  lines() { # file, prefix, count: the last lines that are left after the filter, each wrapped, at most count rows
    [ -s "$1" ] && tr -d '\r\000' <"$1" | grep -v -e '^[[:space:]]*$' -e 'STOP:' -e ' sound: ' | tail -n "$3" | while IFS= read -r l; do printf '%s\n' "$2$l"; done | fold -w $w | tail -n "$3"
  }
  {
    printf '\033c\033[?25l\033[1;31m'
    printf '%s\n' "pocket-ui can't start" | cut -c1-$w
    printf '\033[0m'
    printf '%s\n' "$1" | fold -s -w $w
    [ -z "$2" ] || printf '%s\n' "$2" | fold -s -w $w
    printf '\n'
    lines "$DATA/logs/start.log" "" $n1
    lines "$DATA/logs/pocket-fb.log" "fb: " $n3
    lines "$DATA/logs/pocket-ui.log" "ui: " $n2
    printf '\n\033[1;33m%s\033[0m\n' "Send a photo of this screen."
  } >>"$TTY" 2>/dev/null
  sleep "${DARKOS_HOLD:-15}"
  printf '\033c' >>"$TTY" 2>/dev/null
}
die() {
  log "STOP: $1 $2"
  fail_screen "$1" "$2"
  exit 3  # pocket-ui.sh: the reason was shown
}

if [ "${1:-}" != --inside ]; then
  : >"$LOG"
  : >"$DATA/logs/pocket-fb.log"
  log "=== start, $(uname -r), $(sed -n 's/^PRETTY_NAME=//p' /etc/os-release 2>/dev/null)"
  chmod +x "$ROOT"/bin/* "$ROOT"/port-data/hwroot/usr/bin/* 2>/dev/null  # a card that keeps no permissions (exFAT, FAT) ignores this and runs everything
  for f in pocket-ui pocket-core pocket-fb; do
    [ -e "$ROOT/bin/$f" ] || die "The file bin/$f is missing." "Copy the whole pocket-ui folder again."
    [ -x "$ROOT/bin/$f" ] || die "bin/$f cannot be run from this card." "Copied by a program that drops permissions, or the card is mounted noexec."
  done
  [ -c "$FB" ] || [ -f "$FB" ] || die "No screen device ($FB)." "This does not look like a dArkOS handheld."
  # The board: dArkOS's own name for it (written at its build, `~ark/.config/.DEVICE`), else the device tree's `model`.
  model=${DARKOS_MODEL:-$(tr -d '\0' 2>/dev/null </proc/device-tree/model)}
  dev=${DARKOS_DEVICE:-$(tr -d '\0\r\n ' 2>/dev/null <"${ARK_HOME:-/nonexistent}/.config/.DEVICE")}
  # The R36S family runs community builds of dArkOS (dArkOSRE-R36): the device tree's model is the board and panel
  # ("R36S-V21 2024-12-18 2551", "HL-R36H-V20 ...", "G80CA-MB V1.2 ... Panel 8", "Y3506_V04 ..."), whatever `.DEVICE` says.
  case "$model" in
    R36*|HL-R36H*|GR36S*|G80C*|Y3506*) dev=R36S ;;
  esac
  if [ -z "$dev" ]; then
    case "$model" in
      "Anbernic RG353M"*) dev=RG353M ;;
      "Anbernic RG353V"*) dev=RG353V ;;
      "Anbernic RG503"*) dev=RG503 ;;
      "Powkiddy RK2023"*) dev=RK2023 ;;
      "Powkiddy RGB30"*) dev=RGB30 ;;
      "Powkiddy RGB20 Pro"*) dev=RGB20PRO ;;
      MINILOONG*) dev=MINILOONG ;;
      "Hardkernel ODROID-GO3"*) dev=RG351MP ;;
      "BatleXP G"*) dev=G350 ;;
      "A10 Mini"*) dev=A10MINI ;;
      "Hardkernel ODROID-GO2"*) dev=RGB10 ;;
    esac
  fi
  # One line per board of src/darkos.rs (a test holds the two together): its screen, and REFUSE for the ones the test
  # version has no layout check for.
  REFUSE=
  case "$dev" in
    RG353M) BOARD=darkos-rg353m SCREEN=640x480 ;;
    RG353V|RG353VS) BOARD=darkos-rg353v SCREEN=640x480 ;;
    RK2023) BOARD=darkos-rk2023 SCREEN=640x480 ;;
    RG503) BOARD=darkos-rg503 SCREEN=960x544 REFUSE="The RG503's 960x544 screen" ;;
    RGB30) BOARD=darkos-rgb30 SCREEN=720x720 REFUSE="The RGB30's 720x720 screen" ;;
    RGB20PRO) BOARD=darkos-rgb20pro SCREEN=1024x768 REFUSE="The RGB20 Pro's 1024x768 screen" ;;
    MINILOONG) BOARD=darkos-miniloong SCREEN=720x960 REFUSE="The MiniLoong's 720x960 screen" ;;
    RG351MP) BOARD=darkos-rg351mp SCREEN=640x480 ;;
    R36S) BOARD=darkos-r36s SCREEN=640x480 ;;
    G350) BOARD=darkos-g350 SCREEN=640x480 ;;
    A10MINI) BOARD=darkos-a10mini SCREEN=640x480 ;;
    RGB10) BOARD=darkos-rgb10 SCREEN=320x480 REFUSE="The RGB10's 320x480 screen" ;;
    *) BOARD=darkos-generic SCREEN= ;;
  esac
  [ -z "$REFUSE" ] || die "$REFUSE is not supported by this test version yet." "Only 640x480 handhelds for now."
  # Some community builds report only a board-specific compatible. Fall back to the known RK3326 device keys.
  compatible=${DARKOS_COMPATIBLE-$(tr '\000' ' ' 2>/dev/null </proc/device-tree/compatible)}
  RK3326_FB_WRITE=
  case " $compatible " in
    *' rockchip,rk3326 '*|*' rockchip,px30 '*) RK3326_FB_WRITE=1 ;;
    *' rockchip,rk3566 '*|*' rockchip,rk3568 '*) ;;
    *) case "$dev" in R36S|RG351*|G350|A10MINI|RGB10) RK3326_FB_WRITE=1 ;; esac ;;
  esac
  export RK3326_FB_WRITE
  # What the screen really is: fbdev's mode (`U:640x480p-60`), else the width of its virtual size.
  size=$(sed -n '1s/^[^:]*:\([0-9]*x[0-9]*\).*/\1/p' /sys/class/graphics/fb0/modes 2>/dev/null)
  [ -n "$size" ] || size="$(cut -d, -f1 /sys/class/graphics/fb0/virtual_size 2>/dev/null)x480"
  [ -f "$FB" ] && size=640x480
  # What the kernel says about the screen, for the log (a player's photo of a picture that is too small or shifted needs it).
  g=/sys/class/graphics/fb0
  log "fb0: modes $(tr '\n' ' ' <$g/modes 2>/dev/null), virtual $(cat $g/virtual_size 2>/dev/null), bpp $(cat $g/bits_per_pixel 2>/dev/null), stride $(cat $g/stride 2>/dev/null), rotate $(cat $g/rotate 2>/dev/null), pan $(cat $g/pan 2>/dev/null)"
  [ "$size" = 640x480 ] || die "This screen is ${size:-of unknown size} pixels." "The test version needs a 640x480 screen."
  [ -d "$CORES" ] || log "no cores folder $CORES: the shelf shows, games will not start"
  log "board $BOARD (.DEVICE ${dev:-none}, model $model), screen $size, fb $FB, cores $CORES, roms $ROMS"
  export BOARD
  # The namespace's mounts are private: nothing of them reaches the system, and they go when the last process in it ends.
  # In the background and waited for, so a TERM (a shutdown, a stopped port) reaches the inner part, which cleans up.
  unshare -m --propagation private /bin/sh "$ROOT/start.sh" --inside 2>>"$LOG" &
  INNER=$!
  TERMED=
  trap 'TERMED=1; kill -TERM "$INNER" 2>/dev/null' INT TERM HUP
  wait "$INNER"; code=$?
  # A signal ends the first wait early: the inner part is still cleaning up.
  kill -0 "$INNER" 2>/dev/null && { wait "$INNER"; code=$?; }
  log "=== end ($code)"
  [ "$code" != 75 ] || exit 75
  # Something stopped without saying why (unshare refused, a crash of the script itself): say it now.
  if [ "$code" != 0 ] && [ "$code" != 3 ] && [ -z "$TERMED" ]; then
    die "pocket-ui stopped (code $code)." "The log lines below say where."
  fi
  exit 0
fi

# ---- inside the mount namespace -------------------------------------------------------------------------------------------
VIEW=$RUN/view
mkdir -p "$VIEW"
mount -t tmpfs -o mode=0755 pocket-view "$VIEW" || die "Could not set up the games folder view." "Details are in logs/start.log."
for d in bin boot dev etc home lib lib64 media mnt opt proc root run sbin srv sys tmp usr var roms; do
  if [ -L "/$d" ]; then ln -s "$(readlink "/$d")" "$VIEW/$d"; continue; fi
  [ -d "/$d" ] || continue
  mkdir -p "$VIEW/$d"
  if [ "$d" = tmp ]; then how=--bind; else how=--rbind; fi  # /tmp holds the view itself: not recursively
  mount $how "/$d" "$VIEW/$d" || { log "bind /$d failed"; die "Could not set up the games folder view." "Details are in logs/start.log."; }
done
# /dev is recursively bound above, so /dev/input/event* includes the separate "rk805 pwrkey" too. Do not
# pick an event number: it changes with the kernel/device. The UI claims that power-only node with EVIOCGRAB;
# closing its fd (also on a crash) restores ogage/logind without changing any host service or configuration.
# Our own folder at its own path, wherever the card is mounted: /roms is in the view above, a second card (/roms2) and
# anything else is not, and without it the chroot below finds no pocket-ui to run.
case "$DATA" in
  /roms/*) ;;
  *)
    mkdir -p "$VIEW$DATA" && mount --bind "$DATA" "$VIEW$DATA" || { log "bind $ROOT failed"; die "Could not set up the games folder view." "Details are in logs/start.log."; }
    ;;
esac
# /storage: our folder; inside it each system's folder shows the player's /roms/<system>, read only.
mkdir -p "$VIEW/storage"
mount --bind "$DATA/storage" "$VIEW/storage" || die "Could not set up the games folder view." "Details are in logs/start.log."
# Read the installed ES inventory (including a user's override), retaining its host launch commands.
if [ -f "$ROOT/bin/cores/dsperate_libretro.so" ]; then
  # A private, temporary view: the installed system cores remain untouched. Each core's .info comes along: pocket-core
  # finds the core's name there, and with it dArkOS's RetroArch options (hw_render::retroarch_options); without them
  # dArkOS's flycast ran on its bare defaults and crashed on every save state (0.6.0).
  mkdir -p "$RUN/cores"
  for f in "$CORES"/*_libretro.so "$CORES"/*_libretro.info; do
    [ -f "$f" ] && ln -sf "$f" "$RUN/cores/$(basename "$f")"
  done
  ln -sf "$ROOT/bin/cores/dsperate_libretro.so" "$RUN/cores/dsperate_libretro.so"
  CORES=$RUN/cores
fi
c=$DATA/storage/.config
mkdir -p "$c/emulationstation" "$c/system/configs"
ES=${DARKOS_ES_SYSTEMS:-$ARK_HOME/.emulationstation/es_systems.cfg}
[ -f "$ES" ] || ES=/etc/emulationstation/es_systems.cfg
python3 -I "$ROOT/port-data/systems.py" "$ES" "$ROMS" "$CORES" "$c/emulationstation" "$RUN" >>"$LOG" 2>&1 || die "Could not read dArkOS's game systems." "See logs/start.log."
while IFS="$(printf '\t')" read -r s src; do
  [ -n "$s" ] || continue
  mkdir -p "$DATA/storage/roms/$s"
  mount --bind "$src" "$VIEW/storage/roms/$s" && mount -o remount,bind,ro "$VIEW/storage/roms/$s" || die "Could not show $s read only." "See logs/start.log."
done <"$RUN/systems.mounts"
if [ -d "$ROMS/bios" ]; then
  mount --bind "$ROMS/bios" "$VIEW/storage/roms/bios" && mount -o remount,bind,ro "$VIEW/storage/roms/bios" || die "Could not show the BIOS folder read only." "See logs/start.log."
  # Two subfolders stay writable, where dArkOS's own emulators write too (docs/DARKOS.md "PSP and Dreamcast"):
  # bios/dc holds the Dreamcast's memory cards (one per game, shared with dArkOS's standalone Flycast, as on ROCKNIX),
  # bios/PPSSPP the fonts and tables pocket-core copies from dArkOS's PPSSPP before a PSP game.
  for d in dc PPSSPP; do
    case "$d" in dc) sys=dreamcast ;; *) sys=psp ;; esac
    grep -q "^$sys	" "$RUN/systems.mounts" 2>/dev/null || continue
    mkdir -p "$ROMS/bios/$d" && mount --bind "$ROMS/bios/$d" "$VIEW/storage/roms/bios/$d" || log "bios/$d stays read only"
  done
fi
# PSP: the memory stick (game saves in PSP/SAVEDATA) is dArkOS's standalone PPSSPP's, <card>/psp/ppsspp, so a game saved
# in one continues in the other (ROCKNIX shares it the same way). Made the way dArkOS's ppsspp.sh makes it on first use.
if grep -q "^psp	" "$RUN/systems.mounts" 2>/dev/null; then
  psp=$(sed -n 's/^psp	//p' "$RUN/systems.mounts" | head -1)
  [ -d "$psp/ppsspp" ] || [ ! -d /opt/ppsspp/backupforromsfolder/ppsspp ] || cp -rf /opt/ppsspp/backupforromsfolder/ppsspp/ "$psp/ppsspp/" || log "could not make $psp/ppsspp"
  if [ -d "$psp/ppsspp" ]; then
    mkdir -p "$DATA/storage/saves/psp" && mount --bind "$psp/ppsspp" "$VIEW/storage/saves/psp" || log "psp memory stick: our own storage/saves/psp"
  fi
fi
[ -d /opt/ppsspp/assets ] && export POCKETUI_PPSSPP_ASSETS=/opt/ppsspp/assets
# ScummVM: the core reads and writes scummvm.ini in its system directory, the BIOS folder, which is read only here and whose
# scummvm.ini is dArkOS's RetroArch's. pocket-core gives it a folder of our own instead (a copy of that ini to start from,
# hostsys::scummvm_place) and dArkOS's standalone ScummVM's engine data and themes.
mkdir -p "$DATA/storage/.config/scummvm" && export POCKETUI_SCUMMVM_SYSTEM=/storage/.config/scummvm
[ -d /opt/scummvm/extra ] && export POCKETUI_SCUMMVM_EXTRA=/opt/scummvm/extra
[ -d /opt/scummvm/themes ] && export POCKETUI_SCUMMVM_THEMES=/opt/scummvm/themes
# Dreamcast: dArkOS's flycast core (a 2024 build with the older reicast_* option names) crashes saving a state while it
# renders on its own thread (container, 14.11.2026: SIGSEGV on every slot save; without it save, load and Continue work).
# pocket-core reads /storage/.config/retroarch/retroarch-core-options.cfg beneath the player's own choices (hw_render);
# on dArkOS that file is ours, and this one line is all it holds unless the player chooses otherwise in Options.
RAO=$DATA/storage/.config/retroarch/retroarch-core-options.cfg
grep -q '^reicast_threaded_rendering' "$RAO" 2>/dev/null || { mkdir -p "${RAO%/*}" && echo 'reicast_threaded_rendering = "disabled"' >>"$RAO"; }
[ -n "${DARKOS_TEST_EGL:-}" ] && export BRICK_EGL_LIB=$DARKOS_TEST_EGL
# Preserve the player's Runs on choices. No generated defaults overwrite system.cfg.
touch "$c/system/configs/system.cfg"
# The native launcher re-enters this process's original root, outside the UI's chroot.
export DARKOS_LAUNCHER_PID=$$ DARKOS_ROOT=$ROOT DARKOS_ARK_HOME=$ARK_HOME
export DARKOS_ROMS=$ROMS
export DARKOS_HOST_SDL=${SDL_GAMECONTROLLERCONFIG:-}

# The environment for pocket-ui and pocket-core (both inside the view).
export POCKETUI_HOST=darkos POCKETUI_DEVICE=$BOARD
export POCKET_CORE_SOFTWARE=1
# The sound (docs/DARKOS.md "Sound"). pocket-ui and pocket-core send 44.1 kHz on dArkOS, the rate of its dmix, so ALSA converts
# nothing. RK3566: /etc/asound.conf (a link to ~ark/.asoundrc) is plug > softvol "Master" > dmix: `default`, shared by the
# UI's sounds and the game. RK3326: EmulationStation's `perfmax` deletes ~ark/.asoundrc while a port runs (perfnorm puts its
# copy .asoundrcbak back after), which leaves no dmix: then that same copy, dArkOS's own dmix (same key, same 44.1 kHz), is
# our HOME's .asoundrc for the time we run. Without it the card itself (plughw, one sound at a time: no UI sound during a game).
UHOME=$DATA/storage/home  # HOME of pocket-ui and pocket-core (/storage/home in the view)
mkdir -p "$UHOME"
rm -f "$UHOME/.asoundrc"
ASOUND=${DARKOS_ASOUND:-/etc/asound.conf}
if [ -n "${DARKOS_ALSA:-}" ]; then
  alsa=$DARKOS_ALSA; why="DARKOS_ALSA"
elif [ -r "$ASOUND" ] && grep -q dmix "$ASOUND" 2>/dev/null; then
  alsa=default; why="dArkOS's $(readlink -f "$ASOUND" 2>/dev/null || echo "$ASOUND")"
elif [ -n "$ARK_HOME" ] && grep -q dmix "$ARK_HOME/.asoundrcbak" 2>/dev/null; then
  cp "$ARK_HOME/.asoundrcbak" "$UHOME/.asoundrc"
  alsa=default; why="dArkOS's $ARK_HOME/.asoundrcbak as our .asoundrc (perfmax took ~ark/.asoundrc away)"
elif [ -r "$ASOUND" ]; then
  alsa=default; why="dArkOS's $ASOUND (no dmix in it)"
else
  alsa=plughw:0,0; why="no ALSA setup of dArkOS's: the card alone, one sound at a time"
fi
log "sound: ALSA $alsa at 44100 Hz ($why)"
export POCKET_CORE_ALSA_DEVICE=$alsa
export POCKETUI_CORES=$CORES POCKETUI_HOST_BIN=$ROOT/bin/pocket-core POCKET_CORE_SAVE_DIR=/storage/saves
# GBA: gpSP by default as on the H700 (mGBA stays for a Continue moment it made, and to choose), dArkOS's own gpSP.
[ -f "$CORES/gpsp_libretro.so" ] && export POCKETUI_GBA_CORE=$CORES/gpsp_libretro.so
export WAYLAND_DISPLAY=$RUN/wayland SWAYSOCK=$RUN/sway XDG_RUNTIME_DIR=$RUN HOME=/storage/home
export DARKOS_STATE=$RUN
export POCKETUI_BRICK_CARD=$DATA
# A persistent override is for a private test server; settings.json cannot switch this port to stable/Brick.
if [ -s "$DATA/update-url" ]; then
  export POCKETUI_DARKOS_UPDATE_URL=$(head -1 "$DATA/update-url")
fi
# Our stand-ins first: pocket-ui must not start, stop or reconfigure dArkOS's services (hwroot/usr/bin/systemctl and friends).
export PATH="$ROOT/port-data/hwroot/usr/bin:/usr/sbin:/usr/bin:/sbin:/bin"
chmod +x "$ROOT"/port-data/hwroot/usr/bin/* 2>/dev/null

# What we find, to put back at the end.
BL=$(ls -d /sys/class/backlight/* 2>/dev/null | head -1)
orig_bl=; [ -n "$BL" ] && orig_bl=$(cat "$BL/brightness" 2>/dev/null)
cleanup() {
  trap - EXIT INT TERM HUP
  stop_helpers
  if [ -z "$BRIDGE_STUCK" ]; then
    kd 0
    # Restore brightness only when the player did not leave the screen dark.
    [ -n "$orig_bl" ] && [ "$(cat "$BL/brightness" 2>/dev/null)" = 0 ] && [ "$orig_bl" != 0 ] && echo "$orig_bl" >"$BL/brightness" && log "screen was left dark: brightness back to $orig_bl"
  fi
  rm -f "$WAYLAND_DISPLAY" "$SWAYSOCK" "$UHOME/.asoundrc"
}
TERMED=
trap cleanup EXIT
trap 'TERMED=1; cleanup' INT TERM HUP

kd 1
rm -f "$WAYLAND_DISPLAY" "$SWAYSOCK"
# --take: the framebuffer's picture goes on the screen (EmulationStation has let go of it; fbdev's mode comes back).
POCKET_FB_POSITIONED_WRITE=$RK3326_FB_WRITE "$ROOT/bin/pocket-fb" --wayland "$WAYLAND_DISPLAY" --ipc "$SWAYSOCK" --fb "$FB" $FBMODE --take >>"$DATA/logs/pocket-fb.log" 2>&1 &
FBPID=$!
PIDS="$FBPID"
log "waiting for screen bridge (pid $FBPID, up to 20 s)"
i=0
while [ ! -S "$WAYLAND_DISPLAY" ] || [ ! -S "$SWAYSOCK" ]; do
  kill -0 "$FBPID" 2>/dev/null || die "The screen bridge (pocket-fb) stopped." "See logs/pocket-fb.log."
  if [ "$i" = 50 ] || [ "$i" = 200 ]; then
    wayland=missing ipc=missing
    [ -S "$WAYLAND_DISPLAY" ] && wayland=ready
    [ -S "$SWAYSOCK" ] && ipc=ready
    log "screen bridge after $((i / 10)) s: Wayland $wayland, IPC $ipc"
    [ "$i" = 200 ] && die "The screen bridge did not come up." "See logs/pocket-fb.log."
  fi
  sleep 0.1
  i=$((i + 1))
done
log "screen bridge ready after $((i / 10)).$((i % 10)) s"
[ -x "$VIEW$ROOT/bin/pocket-ui" ] || die "pocket-ui is not in the games folder view." "Looked for $ROOT/bin/pocket-ui."
# Power off, restart and sleep: pocket-ui's stand-in systemctl (hwroot) cannot reach systemd from the chroot, so it asks here,
# outside the chroot, and this runs the real one (dArkOS's own scripts do the same: `sudo systemctl poweroff`).
SYSTEMCTL=${DARKOS_SYSTEMCTL:-/usr/bin/systemctl}
PW=$RUN/power; rm -rf "$PW"; mkdir -p "$PW"
(
  while :; do
    for r in "$PW"/*.req; do
      [ -e "$r" ] || continue
      verb=$(cat "$r" 2>/dev/null); b=${r%.req}; rm -f "$r"
      case "$verb" in
        poweroff|reboot|suspend) log "power: systemctl $verb, asked by pocket-ui"; "$SYSTEMCTL" "$verb" >>"$LOG" 2>&1; echo $? >"$b.done" ;;
        *) echo 1 >"$b.done" ;;
      esac
    done
    sleep 0.2
  done
) &
PIDS="$! $PIDS"
STARTED=$(date +%s)
chroot "$VIEW" "$ROOT/bin/pocket-ui" >>"$DATA/logs/pocket-ui.log" 2>&1 &
UIPID=$!
PIDS="$UIPID $PIDS"
wait "$UIPID"; code=$?
secs=$(( $(date +%s) - STARTED ))
log "pocket-ui ended with $code after $secs s"
# A shutdown (TERM) or the player leaving normally is fine. Anything else, or an end within seconds, is a failure the player
# would otherwise see only as a black screen.
# Code 77 is pocket-ui's "Back to EmulationStation" (rescue::OPEN_ES_CODE), chosen by the player: a normal end, not a failure.
if [ "$code" = 75 ]; then
  log "update installed: restart through the shared Brick boot recovery"
  exit 75
elif [ "$code" = 77 ]; then
  log "left on purpose: back to EmulationStation"
elif [ -z "$TERMED" ] && { [ "$code" != 0 ] || [ "$secs" -lt 4 ]; }; then
  die "pocket-ui stopped after $secs s (code $code)." "Back to EmulationStation in a moment."
fi
exit 0
