#!/bin/sh
# Dev loop against the device over SSH.
#   dev.sh deploy        build for aarch64, copy, (re)start pocket-ui, then verify-device.sh (exit 1 when the device is not
#                        in its normal state: one pocket-ui in the unit, no trace variables, the deployed binary running).
#                        With pocket-ui as the boot UI it also puts the boot screen's unit in place
#                        (deploy/pocket-splash.service, docs/BOOT-SPLASH.md); it shows from the next start of the device.
#   dev.sh send up down a   drive the UI through its command FIFO
#   dev.sh shot NAME     screenshot to screenshots/NAME.png
#   dev.sh stop          stop pocket-ui, EmulationStation comes back
#
# With pocket-ui as the boot UI (scripts/autostart.sh, drop-in for essway) pocket-ui runs in the essway
# unit: deploy restarts essway instead of starting the pocketui unit. stop leaves the mark
# /tmp/pocket-ui.es-only and restarts essway, so the launcher hands over to EmulationStation at once; it
# stays like that until the next deploy (which removes the mark) or a reboot (/tmp is emptied).
set -eu
. "$(dirname "$0")/host.sh"
ROOT=$(cd "$(dirname "$0")/.." && pwd)
WAYLAND="XDG_RUNTIME_DIR=/var/run/0-runtime-dir WAYLAND_DISPLAY=wayland-1"
DROPIN=/storage/.config/system.d/essway.service.d/10-pocket-ui.conf
SPLASH=/storage/.config/system.d/pocket-splash.service
ES_ONLY=/tmp/pocket-ui.es-only

case "${1:-}" in
  deploy)
    # The commit Report a problem names (src/report.rs).
    commit=$(git -C "$ROOT" rev-parse --short=12 HEAD 2>/dev/null || echo unknown)
    [ -z "$(git -C "$ROOT" status --porcelain 2>/dev/null)" ] || commit=$commit-dirty
    (cd "$ROOT" && . "$ROOT/scripts/release-rustflags.sh" && POCKETUI_COMMIT=$commit cargo zigbuild --release --target aarch64-unknown-linux-gnu.2.38 --bin pocket-ui --bin pocket-core --bin pocket-splash)
    # Dev builds go into releases/dev, which becomes current; signed releases stay untouched.
    # A running pocket-ui keeps its binary busy: stop it first, in whichever unit it runs.
    # A known good dev build (the launcher's way back, run-pocket-ui.sh) is copied aside first: the new build replaces
    # releases/dev in place and has not proven anything yet.
    ssh "$HOST" 'systemctl stop pocketui 2>/dev/null; systemctl reset-failed pocketui 2>/dev/null;
      if [ -f '"$DROPIN"' ]; then rm -f '"$ES_ONLY"'; systemctl stop essway; fi;
      cd /storage/pocket-ui 2>/dev/null || mkdir -p /storage/pocket-ui; mkdir -p /storage/pocket-ui/releases/dev;
      cd /storage/pocket-ui && if [ "$(readlink known-good 2>/dev/null)" = releases/dev ]; then
        rm -rf releases/dev-good; cp -a releases/dev releases/dev-good && ln -sfn releases/dev-good known-good; fi; true'
    scp -q "$ROOT/target/aarch64-unknown-linux-gnu/release/pocket-ui" "$ROOT/target/aarch64-unknown-linux-gnu/release/pocket-core" "$ROOT/target/aarch64-unknown-linux-gnu/release/pocket-splash" "$ROOT/deploy/portmaster-setup.sh" "$HOST:/storage/pocket-ui/releases/dev/"
    scp -q "$ROOT/deploy/run-pocket-ui.sh" "$HOST:/storage/pocket-ui/"
    # The boot screen goes with the boot UI's drop-in, as install.sh and autostart.sh do it; never started here.
    scp -q "$ROOT/deploy/pocket-splash.service" "$HOST:/tmp/pocket-splash.service"
    ssh "$HOST" 'if [ -f '"$DROPIN"' ]; then
        cmp -s /tmp/pocket-splash.service '"$SPLASH"' || { cp /tmp/pocket-splash.service '"$SPLASH"'.new && mv -f '"$SPLASH"'.new '"$SPLASH"'; }
        mkdir -p /storage/.config/system.d/sysinit.target.wants && ln -sfn ../pocket-splash.service /storage/.config/system.d/sysinit.target.wants/pocket-splash.service
        systemctl daemon-reload; fi; rm -f /tmp/pocket-splash.service'
    ssh "$HOST" 'cd /storage/pocket-ui && chmod +x run-pocket-ui.sh releases/dev/*; rm -f update-pending;
      for f in pocket-ui pocket-core portmaster-setup.sh; do [ -L $f ] || rm -f $f; ln -sfn current/$f $f; done;
      if [ "$(readlink current)" != releases/dev ]; then [ -L current ] && ln -sfn "$(readlink current)" previous; ln -sfn releases/dev current; fi;
      if [ -f '"$DROPIN"' ]; then systemctl reset-failed essway 2>/dev/null; systemctl restart essway;
      else systemd-run --unit=pocketui /storage/pocket-ui/run-pocket-ui.sh >/dev/null; fi;
      sleep 1.5; cat /var/log/pocket-ui.log'
    # Not just "the copy worked": the right binary runs, once, in the unit, with a clean environment (docs/DEVELOPING.md).
    "$ROOT/scripts/verify-device.sh" "$HOST"
    ;;
  send)
    # One SSH session for the whole sequence: many quick logins trip sshd's rate limit.
    shift
    # A FIFO without a reader blocks the writer: give up if pocket-ui isn't running.
    ssh "$HOST" "pidof pocket-ui >/dev/null || { echo 'pocket-ui is not running' >&2; exit 1; }
      for cmd in $*; do timeout 2 sh -c \"echo \$cmd > /tmp/pocket-ui.in\" || exit 1; sleep 0.2; done"
    ;;
  shot)
    mkdir -p "$ROOT/screenshots"
    ssh "$HOST" "$WAYLAND grim /tmp/pocket-ui-shot.png"
    scp -q "$HOST:/tmp/pocket-ui-shot.png" "$ROOT/screenshots/${2:-shot}.png"
    echo "$ROOT/screenshots/${2:-shot}.png"
    ;;
  stop)
    ssh "$HOST" 'if [ -f '"$DROPIN"' ]; then
        touch '"$ES_ONLY"'; systemctl stop pocketui 2>/dev/null; systemctl reset-failed essway 2>/dev/null; systemctl restart essway;
      else systemctl stop pocketui; systemctl start essway; fi'
    ;;
  log)
    ssh "$HOST" 'cat /var/log/pocket-ui.log'
    ;;
  *)
    echo "usage: dev.sh deploy|send CMD...|shot NAME|stop|log" >&2
    exit 2
    ;;
esac
