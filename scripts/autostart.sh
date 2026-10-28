#!/bin/sh
# pocket-ui as the boot UI in place of EmulationStation (Phase 6 in docs/INSTALL-DUAL-BOOT.md), from the
# Mac over SSH. It installs deploy/essway-pocket-ui.conf as a drop-in for ROCKNIX's essway unit, and the boot screen
# deploy/pocket-splash.service with its link in sysinit.target.wants (docs/BOOT-SPLASH.md).
#
#   autostart.sh install             dry run (default): read-only checks on the device, prints every change
#   autostart.sh install --apply     copies the drop-in, daemon-reload, restarts essway: pocket-ui comes up
#   autostart.sh uninstall           dry run of the uninstall
#   autostart.sh uninstall --apply   removes the drop-in, daemon-reload, restarts essway: EmulationStation again
#   autostart.sh status              read-only: drop-in, essway, what runs in it
# Options: --host root@<device-ip> (or POCKETUI_HOST, or scripts/local.env)
#
# Only these are written: /storage/.config/system.d/essway.service.d/10-pocket-ui.conf, and
# /storage/.config/system.d/pocket-splash.service with sysinit.target.wants/pocket-splash.service. Restarting
# essway ends the running UI and a running game. Without those files, ROCKNIX is exactly as before.
set -eu

. "$(dirname "$0")/host.sh"
ROOT=$(cd "$(dirname "$0")/.." && pwd)
CONF=$ROOT/deploy/essway-pocket-ui.conf
DROPIN_DIR=/storage/.config/system.d/essway.service.d
DROPIN=$DROPIN_DIR/10-pocket-ui.conf
SPLASH_CONF=$ROOT/deploy/pocket-splash.service
SPLASH=/storage/.config/system.d/pocket-splash.service
SPLASH_LINK=/storage/.config/system.d/sysinit.target.wants/pocket-splash.service
LAUNCHER=/storage/pocket-ui/run-pocket-ui.sh
ES_ONLY=/tmp/pocket-ui.es-only

ACTION=
APPLY=0
while [ $# -gt 0 ]; do
  case $1 in
    install | uninstall | status) ACTION=$1 ;;
    --apply) APPLY=1 ;;
    --dry-run) APPLY=0 ;;
    --host) HOST=$2; shift ;;
    -h | --help) sed -n '2,13p' "$0"; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
  shift
done
[ -n "$ACTION" ] || { sed -n '2,13p' "$0" >&2; exit 2; }

say() { printf '%s\n' "$*"; }
step() { if [ $APPLY = 1 ]; then say "==> $1"; else say "[dry-run] would: $1"; fi; }
die() { say "ABORT: $*" >&2; exit 1; }
remote() { ssh -o BatchMode=yes -o ConnectTimeout=10 "$HOST" "$@"; }
# Runs $1 on the device with --apply; prints it otherwise.
act() {
  step "$2"
  if [ $APPLY = 1 ]; then remote "$1"; fi
}

# Reports what the device runs now. Read-only.
show_status() {
  remote "if [ -f $DROPIN ]; then echo 'drop-in:        installed ($DROPIN)'; else echo 'drop-in:        not installed'; fi
    echo \"boot screen:    \$([ -L $SPLASH_LINK ] && [ -f $SPLASH ] && echo installed || echo 'not installed')\"
    echo \"essway:         \$(systemctl is-active essway 2>/dev/null) / \$(systemctl is-enabled essway 2>/dev/null)\"
    echo \"drop-ins:       \$(systemctl show -p DropInPaths --value essway 2>/dev/null)\"
    echo \"restarts:       \$(systemctl show -p NRestarts --value essway 2>/dev/null)\"
    echo \"pocket-ui:      \$(pidof pocket-ui >/dev/null && echo running || echo 'not running')\"
    echo \"EmulationStation: \$(pidof emulationstation >/dev/null && echo running || echo 'not running')\"
    [ -f $ES_ONLY ] && echo \"es-only mark:   $ES_ONLY (dev.sh stop), EmulationStation until deploy or reboot\"
    echo \"version:        current -> \$(readlink /storage/pocket-ui/current 2>/dev/null), known good -> \$(readlink /storage/pocket-ui/known-good 2>/dev/null || echo none)\"
    [ -f /storage/pocket-ui/es-fallback ] && echo \"EmulationStation opened by itself: \$(cat /storage/pocket-ui/es-fallback)\"; true"
  if remote "[ -f $DROPIN ]" && ! remote "cat $DROPIN" | cmp -s - "$CONF"; then
    say "note:           the drop-in on the device differs from deploy/essway-pocket-ui.conf"
  fi
}

# Read-only checks before installing.
preflight_install() {
  say "== preflight (read-only) against $HOST"
  [ -r "$CONF" ] || die "$CONF missing"
  remote true || die "no key-based SSH login to $HOST"
  remote "[ -f /usr/lib/systemd/system/essway.service ]" || die "no essway.service on the device"
  remote "[ -x /usr/bin/start_es.sh ]" || die "no /usr/bin/start_es.sh on the device (EmulationStation fallback)"
  [ "$(remote 'systemctl is-enabled essway 2>/dev/null' || true)" != masked ] || die "essway is masked (ROCKNIX desktop mode?)"
  remote "[ -d /storage/.config/system.d ]" || die "/storage/.config/system.d is missing"
  remote "[ -x $LAUNCHER ] && [ -x /storage/pocket-ui/pocket-ui ]" || die "no pocket-ui installed in /storage/pocket-ui (dev.sh deploy first)"
  # A launcher without essway mode would stop essway, which is itself.
  remote "grep -q POCKETUI_AS_ESSWAY $LAUNCHER" || die "$LAUNCHER has no essway mode yet (dev.sh deploy first)"
  # The drop-in's growing restart pause (RestartSteps) needs systemd 254; older ones ignore it and restart every 2 s.
  sd=$(remote "systemctl --version | sed -n '1s/^systemd \\([0-9]*\\).*/\\1/p'" || true)
  [ "${sd:-0}" -ge 254 ] 2>/dev/null || say "note: systemd ${sd:-?} is older than 254: RestartSteps is ignored, essway restarts every 2 s when it fails"
  say "ok: essway, start_es.sh, /storage/.config/system.d, pocket-ui and its launcher with essway mode"
}

restart_essway() {
  act "systemctl stop pocketui 2>/dev/null; systemctl reset-failed pocketui 2>/dev/null; rm -f $ES_ONLY; systemctl reset-failed essway 2>/dev/null; systemctl restart essway" \
    "stop a pocketui unit left from dev.sh, remove the es-only mark, systemctl restart essway"
}

case $ACTION in
  status)
    show_status
    ;;
  install)
    preflight_install
    act "mkdir -p $DROPIN_DIR && cat >$DROPIN.new && mv $DROPIN.new $DROPIN" "copy deploy/essway-pocket-ui.conf to $DROPIN" <"$CONF"
    act "cat >$SPLASH.new && mv $SPLASH.new $SPLASH && mkdir -p ${SPLASH_LINK%/*} && ln -sfn ../pocket-splash.service $SPLASH_LINK" \
      "copy deploy/pocket-splash.service to $SPLASH, link it from sysinit.target.wants (the boot screen, from the next start)" <"$SPLASH_CONF"
    act "systemctl daemon-reload" "systemctl daemon-reload"
    step "check that systemd took the drop-in (DropInPaths, POCKETUI_AS_ESSWAY=1), otherwise remove it again"
    if [ $APPLY = 1 ]; then
      if ! remote "systemctl show -p DropInPaths --value essway | grep -q 10-pocket-ui.conf && systemctl show -p Environment --value essway | grep -q POCKETUI_AS_ESSWAY=1"; then
        remote "rm -f $DROPIN $SPLASH_LINK $SPLASH; rmdir $DROPIN_DIR 2>/dev/null; systemctl daemon-reload"
        die "systemd did not load $DROPIN; removed it again, nothing else changed"
      fi
    fi
    restart_essway
    if [ $APPLY = 1 ]; then
      sleep 5
      show_status
      remote "pidof pocket-ui >/dev/null" || { say "pocket-ui is not running; launcher log:"; remote "tail -n 20 /var/log/pocket-ui.log" || true; }
    fi
    ;;
  uninstall)
    remote true || die "no key-based SSH login to $HOST"
    act "rm -f $DROPIN; rmdir $DROPIN_DIR 2>/dev/null; true" "remove $DROPIN (and its directory if empty)"
    act "rm -f $SPLASH_LINK $SPLASH; rmdir ${SPLASH_LINK%/*} 2>/dev/null; true" "remove the boot screen $SPLASH and its link"
    act "systemctl daemon-reload" "systemctl daemon-reload"
    restart_essway
    if [ $APPLY = 1 ]; then
      sleep 5
      show_status
    fi
    ;;
esac
[ $APPLY = 1 ] || [ "$ACTION" = status ] || say "(dry run: nothing changed; add --apply to do it)"
