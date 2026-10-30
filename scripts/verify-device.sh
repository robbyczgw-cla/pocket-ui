#!/bin/sh
# Is the device in its normal state? One line per check, then `VERIFY: ok` (exit 0) or `VERIFY: FAILED (...)` (exit 1).
#   scripts/verify-device.sh [--no-local] [host]      host: root@<ip>; default POCKETUI_HOST / scripts/local.env
# `dev.sh deploy` and `latency.sh` (on a device) run it at the end; run it by hand after any measurement or demo.
#
# Why (2026-10-24): a demo script left a pocket-ui running outside the essway unit for about six hours, with
# POCKETUI_TRACE and POCKETUI_NO_HISTORY set. Later deploys failed silently ("text busy"), real play was not recorded,
# and a feature was reported as running from an old log line. Each check below is one thing that went wrong then.
#
# Checks (the unit is essway when the boot-UI drop-in is installed, as scripts/autostart.sh does; else pocketui):
#   service    the unit is active
#   pocket-ui  exactly one pocket-ui runs (pidof; never pgrep -f), and for how long
#   cgroup     /proc/<pid>/cgroup of it names the unit: not an orphan started from a shell or a script
#   env        no pocket-ui (or pocket-core) has POCKETUI_TRACE set, or POCKETUI_NO_HISTORY with a value other than
#              empty or 0 (those two do not hold recording off, src/play_log.rs); no redirected data/library or test
#              hardware, clock, sleep, power or snapshot overrides; no dev.env left behind
#   md5        what runs (/proc/<pid>/exe) is the deployed binary (/storage/pocket-ui/pocket-ui, which follows
#              current/); and the deployed one is the one built here (target/aarch64-unknown-linux-gnu/release),
#              unless --no-local (latency.sh: it measures whatever is installed, not necessarily this build)
#   pocket-core  only reported: a game may be running. A pocket-core with a stale binary or the variables above fails.
# One SSH session for all of it (sshd rate-limits many quick logins). It waits up to VERIFY_WAIT seconds (default 10)
# for the unit and pocket-ui to come up, since it runs right after a restart.
#
# For the tests (scripts/test-verify-device.sh): VERIFY_PROC, VERIFY_STORAGE, VERIFY_TMP replace /proc, /storage, /tmp
# on the remote side; VERIFY_BUILD_DIR replaces the local build folder.
set -u
ROOT=$(cd "$(dirname "$0")/.." && pwd)
. "$(dirname "$0")/host.sh"
LOCAL=1
HOST_ARG=
for a in "$@"; do
  case "$a" in
    --no-local) LOCAL=0 ;;
    -h | --help) sed -n '2,/^set -u/p' "$0" | sed '$d; s/^# \{0,1\}//'; exit 0 ;;
    -*) echo "verify-device: unknown option $a" >&2; exit 2 ;;
    *) HOST_ARG=$a ;;
  esac
done
[ -z "$HOST_ARG" ] || HOST=$HOST_ARG
need_host
BUILD=${VERIFY_BUILD_DIR:-$ROOT/target/aarch64-unknown-linux-gnu/release}
WAIT=${VERIFY_WAIT:-10}

# The remote half: prints facts as key=value lines, judges nothing. Runs in the device's shell (bash on ROCKNIX).
REMOTE=$(cat <<'EOF'
P=${VERIFY_PROC:-/proc}; S=${VERIFY_STORAGE:-/storage}; TMP=${VERIFY_TMP:-/tmp}
DROPIN=$S/.config/system.d/essway.service.d/10-pocket-ui.conf
if [ -f "$DROPIN" ]; then U=essway; echo mode=essway; else U=pocketui; echo mode=plain; fi
echo "unit=$U"
i=0
while :; do
  st=$(systemctl is-active "$U" 2>&1 </dev/null | head -1)
  uis=$(pidof pocket-ui)
  { [ "$st" = active ] && [ -n "$uis" ]; } && break
  [ "$i" -ge $((WAIT * 5)) ] && break
  sleep 0.2; i=$((i + 1))
done
echo "unit_state=$st"
if [ -e "$S/pocket-ui/dev.env" ] || [ -L "$S/pocket-ui/dev.env" ]; then echo dev_env=yes; else echo dev_env=no; fi
[ -e "$TMP/pocket-ui.es-only" ] && echo es_mark=yes || echo es_mark=no
cores=$(pidof pocket-core)
echo "ui_pids=$(echo $uis)"
echo "core_pids=$(echo $cores)"
facts() { # kind pid
  k=$1; p=$2; d=$P/$p
  echo "$k.$p.exe=$(readlink "$d/exe" 2>/dev/null)"
  echo "$k.$p.md5=$(md5sum "$d/exe" 2>/dev/null | cut -d' ' -f1)"
  echo "$k.$p.cgroup=$(tr '\n' ' ' <"$d/cgroup" 2>/dev/null)"
  if env=$(tr '\0' '\n' <"$d/environ" 2>/dev/null) && [ -r "$d/environ" ]; then
    for v in POCKETUI_TRACE POCKETUI_NO_HISTORY; do
      line=$(printf '%s\n' "$env" | grep "^$v=" | head -1)
      if [ -n "$line" ]; then echo "$k.$p.$v=set:${line#*=}"; else echo "$k.$p.$v=unset"; fi
    done
    # Only report recognised test controls, never dump unrelated environment values or credentials.
    overrides=$(printf '%s\n' "$env" | while IFS= read -r line; do
      key=${line%%=*}; value=${line#*=}
      case "$key" in
        POCKETUI_DATA) [ "$value" = /storage/.config/pocket-ui ] && continue ;;
        POCKETUI_ES_SYSTEMS) [ "$value" = /storage/.config/emulationstation/es_systems.cfg ] && continue ;;
        POCKETUI_ES_HOME) [ "$value" = /storage ] && continue ;;
        POCKETUI_WIFI_IFACE) [ "$value" = wlan0 ] && continue ;;
        POCKETUI_BATTERY) [ "$value" = /sys/class/power_supply/battery ] && continue ;;
        POCKETUI_LOG) [ "$value" = /var/log/pocket-ui.log ] && continue ;;
        POCKETUI_FRAME_FILE) [ "$value" = /tmp/pocket-ui.frame ] && continue ;;
        POCKETUI_SLEEP_MINUTE_SECS) [ "$value" = 60 ] && continue ;;
        POCKETUI_RTC_ALARM) [ "$value" = /sys/class/rtc/rtc0/wakealarm ] && continue ;;
        POCKETUI_RTC_PROC) [ "$value" = /proc/driver/rtc ] && continue ;;
        POCKETUI_SUSPEND_CMD) [ "$value" = 'systemctl suspend' ] && continue ;;
        POCKETUI_POWEROFF_CMD) [ "$value" = 'systemctl poweroff' ] && continue ;;
        POCKETUI_REBOOT_CMD) [ "$value" = 'systemctl reboot' ] && continue ;;
        POCKETUI_BOOTLOADER_CMD) [ "$value" = 'systemctl reboot --reboot-argument=bootloader' ] && continue ;;
        POCKETUI_MOCK*|POCKETUI_SLEEP_*|POCKETUI_POWER_KEY|POCKETUI_BACKLIGHT|POCKETUI_SAFE*|POCKETUI_DEVICE|\
        POCKETUI_NO_AUTOSUSPEND|POCKETUI_NO_BOOT_MENU|POCKETUI_MOONLIGHT|POCKETUI_TV_OUTPUT|\
        POCKETUI_CLOCK_SHIFT|POCKETUI_SNAPSHOT_*|POCKETUI_FRAME_BENCH|POCKETUI_FRAME_LOG|POCKETUI_VISIBLE_TEXT|\
        POCKETUI_TEXT_BOXES|POCKETUI_MOTION_SCALE) ;;
        *) continue ;;
      esac
      printf '%s; ' "$line"
    done)
    echo "$k.$p.test_overrides=$overrides"
  else
    echo "$k.$p.POCKETUI_TRACE=unreadable"; echo "$k.$p.POCKETUI_NO_HISTORY=unreadable"
  fi
  up=$(cut -d' ' -f1 "$P/uptime" 2>/dev/null)
  start=$(sed 's/.*) //' "$d/stat" 2>/dev/null | cut -d' ' -f20)
  hz=$(getconf CLK_TCK 2>/dev/null || echo 100)
  echo "$k.$p.age=$(awk -v up="$up" -v st="$start" -v hz="$hz" 'BEGIN { if (up == "" || st == "") print "?"; else printf "%d", up - st / hz }')"
}
for p in $uis; do facts ui "$p"; done
for p in $cores; do facts core "$p"; done
for b in pocket-ui pocket-core; do
  echo "dep.$b=$(readlink -f "$S/pocket-ui/$b" 2>/dev/null)"
  echo "dep.$b.md5=$(md5sum "$S/pocket-ui/$b" 2>/dev/null | cut -d' ' -f1)"
done
echo end=1
EOF
)

md5_of() { # file -> md5 or nothing
  [ -f "$1" ] || return 0
  if command -v md5sum >/dev/null 2>&1; then md5sum "$1" | cut -d' ' -f1; else md5 -q "$1"; fi
}

fails=
ok() { echo "ok   $*"; }
info() { echo "info $*"; }
bad() { # name text...
  n=$1; shift
  echo "FAIL $*"
  case ", $fails," in *", $n,"*) ;; *) fails="$fails${fails:+, }$n" ;; esac
}

rc=0
OUT=$(ssh -o ConnectTimeout=10 "$HOST" "WAIT=$WAIT; $REMOTE" 2>&1) || rc=$?
fact() { printf '%s\n' "$OUT" | sed -n "s/^$1=//p" | head -1; }
short() { echo "$1" | cut -c1-8; }

if [ "$(fact end)" != 1 ]; then
  bad ssh "could not read the device's state over ssh (exit $rc): $(printf '%s' "$OUT" | tail -3 | tr '\n' ' ')"
else
  unit=$(fact unit)
  state=$(fact unit_state)
  if [ "$(fact dev_env)" = yes ]; then
    bad env "/storage/pocket-ui/dev.env exists; restore/remove the test file, restart $unit after the game ends, then run verify-device.sh again"
  fi
  mark=
  [ "$(fact es_mark)" = yes ] && mark=" (/tmp/pocket-ui.es-only is set: scripts/dev.sh stop left EmulationStation in charge)"
  if [ "$state" = active ]; then ok "$unit.service is active"; else bad service "$unit.service is '$state', not active$mark"; fi

  uis=$(fact ui_pids)
  cores=$(fact core_pids)
  n=0
  for p in $uis; do n=$((n + 1)); done
  if [ "$n" = 1 ]; then
    ok "one pocket-ui: pid $uis, running for $(fact "ui.$uis.age") s"
  elif [ "$n" = 0 ]; then
    bad pocket-ui "no pocket-ui is running$mark"
  else
    bad pocket-ui "$n pocket-ui are running (pids $uis): one of them is not the unit's"
  fi

  for p in $uis; do
    cg=$(fact "ui.$p.cgroup")
    case "$cg" in
      *"$unit.service"*) ok "pocket-ui $p is in $unit.service ($(echo "$cg" | sed 's/^ *//; s/ *$//'))" ;;
      *) bad cgroup "pocket-ui $p is not in $unit.service: cgroup '${cg:-unreadable}' (started by hand or by a script?)" ;;
    esac
  done

  for kind in ui core; do
    name=pocket-ui; list=$uis
    [ $kind = core ] && { name=pocket-core; list=$cores; }
    for p in $list; do
      t=$(fact "$kind.$p.POCKETUI_TRACE")
      h=$(fact "$kind.$p.POCKETUI_NO_HISTORY")
      problem=
      case "$t" in
        unset | set:) ;;
        unreadable) problem="environment unreadable" ;;
        *) problem="POCKETUI_TRACE=${t#set:}" ;;
      esac
      case "$h" in
        unset | set: | set:0) ;;
        unreadable) ;;
        *) problem="${problem:+$problem, }POCKETUI_NO_HISTORY=${h#set:}" ;;
      esac
      overrides=$(fact "$kind.$p.test_overrides")
      [ -z "$overrides" ] || problem="${problem:+$problem, }test overrides: $overrides"
      if [ -n "$problem" ]; then
        bad env "$name $p has $problem; restore/remove dev.env and test overrides, restart $unit after the game ends, then run verify-device.sh again"
      else
        ok "$name $p environment is clean (POCKETUI_TRACE ${t:+$t}, POCKETUI_NO_HISTORY $h)"
      fi
    done
  done

  for b in pocket-ui pocket-core; do
    dep=$(fact "dep.$b.md5")
    path=$(fact "dep.$b")
    kind=ui; [ $b = pocket-core ] && kind=core
    if [ -z "$dep" ]; then
      bad md5 "no deployed $b at /storage/pocket-ui/$b"
      continue
    fi
    list=$uis; [ $kind = core ] && list=$cores
    for p in $list; do
      run=$(fact "$kind.$p.md5")
      if [ "$run" = "$dep" ]; then
        ok "$b $p runs the deployed binary (md5 $(short "$dep"), $path)"
      else
        bad md5 "$b $p runs md5 '${run:-unreadable}' but the deployed $path is $(short "$dep") (exe: $(fact "$kind.$p.exe"))"
      fi
    done
    if [ $LOCAL = 0 ]; then
      info "$b: deployed md5 $(short "$dep"), not compared with a local build (--no-local)"
    else
      loc=$(md5_of "$BUILD/$b")
      if [ -z "$loc" ]; then
        info "$b: no local build in $BUILD to compare with (deployed md5 $(short "$dep"))"
      elif [ "$loc" = "$dep" ]; then
        ok "$b deployed md5 $(short "$dep") equals the local build"
      else
        bad md5 "$b deployed md5 $(short "$dep") differs from the local build $(short "$loc") ($BUILD/$b)"
      fi
    fi
  done

  if [ -n "$cores" ]; then
    info "pocket-core is running (pids $cores): a game is on, or one was left behind"
  else
    ok "no pocket-core is running"
  fi
fi

if [ -z "$fails" ]; then
  echo "VERIFY: ok"
  exit 0
fi
echo "VERIFY: FAILED ($fails)"
exit 1
