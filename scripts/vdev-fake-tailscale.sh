#!/bin/sh
# Stand-ins for Tailscale in a virtual Mini V2, to try Settings > Tailscale (docs/user/remote.md) without a tailnet.
# extract-rocknix.sh drops /usr/sbin/tailscale{,d}, and the container has no systemd, so this puts two small scripts
# in their place, inside the container only: a `systemctl` that knows tailscaled (and passes everything else to the
# real one), and a `tailscale` that answers like the real one for `status --json`, `up`, `logout` and `set`.
# Nothing here talks to Tailscale's servers or to any network.
#
#   VDEV_NAME=<name> scripts/vdev-fake-tailscale.sh install    put the stand-ins in (before: the page says "Not installed")
#   VDEV_NAME=<name> scripts/vdev-fake-tailscale.sh approve    the sign-in is approved "on the phone": `tailscale up` ends
#   VDEV_NAME=<name> scripts/vdev-fake-tailscale.sh state      what the stand-ins and ROCKNIX's setting say
#   VDEV_NAME=<name> scripts/vdev-fake-tailscale.sh reset      back to logged out, service stopped
#   VDEV_NAME=<name> scripts/vdev-fake-tailscale.sh hang|unhang   `tailscale` stops answering (sleeps) / answers again
#
# `up` prints the sign-in address like the real one (stderr, "To authenticate, visit:") and waits for `approve`.
# An auth key starting with `tskey-auth-` signs in at once; any other key fails with the real error text.
set -eu
ROOT=$(cd "$(dirname "$0")/.." && pwd)
C=vdev-${VDEV_NAME:-dev}
dx() { docker exec "$C" "$@"; }

case "${1:-}" in
  install)
    dx mkdir -p /tmp/fakets
    for f in running needslogin stopped; do
      docker cp "$ROOT/src/fixtures/tailscale-status-$f.json" "$C:/tmp/fakets/$f.json"
    done
    docker exec -i "$C" sh -c 'cat > /usr/sbin/tailscale && chmod +x /usr/sbin/tailscale' <<'EOF'
#!/bin/sh
# Stand-in for ROCKNIX's tailscale (scripts/vdev-fake-tailscale.sh).
D=/tmp/fakets
if [ ! -e $D/active ]; then
  echo "failed to connect to local tailscaled; it doesn't appear to be running (sudo systemctl start tailscaled ?)" >&2
  exit 1
fi
# `touch /tmp/fakets/hang` (see "hang" below) makes every call hang, like a stuck tailscaled.
[ ! -e $D/hang ] || sleep 120
state() { cat $D/state 2>/dev/null || echo needslogin; }
host() { cat $D/hostname 2>/dev/null || echo pocket-mini; }
echo "$*" >> $D/calls
case "$1" in
  status)
    case "$(state)" in
      running) sed "s/pocket-mini/$(host)/g" $D/running.json ;;
      stopped) sed "s/pocket-mini/$(host)/g" $D/stopped.json ;;
      *) sed "s/pocket-mini/$(host)/g" $D/needslogin.json ;;
    esac ;;
  up)
    shift
    key=
    for a in "$@"; do
      case $a in
        --hostname=*) echo "${a#--hostname=}" > $D/hostname ;;
        --auth-key=file:*) key=${a#--auth-key=file:} ;;
      esac
    done
    if [ "$(state)" != needslogin ]; then echo running > $D/state; exit 0; fi
    if [ -n "$key" ]; then
      if grep -q '^tskey-auth-' "$key"; then echo running > $D/state; exit 0; fi
      echo "Error: backend error: invalid key" >&2
      exit 1
    fi
    printf '\nTo authenticate, visit:\n\n\thttps://login.tailscale.com/a/1b2c3d4e5f6a7b\n\n' >&2
    while [ ! -e $D/approve ]; do sleep 0.3; done
    rm -f $D/approve
    echo running > $D/state
    echo Success. ;;
  logout) echo needslogin > $D/state ;;
  set)
    shift
    for a in "$@"; do
      case $a in --hostname=*) echo "${a#--hostname=}" > $D/hostname ;; esac
    done ;;
  *) echo "stand-in tailscale: $*" >&2; exit 2 ;;
esac
EOF
    dx sh -c '[ -e /usr/bin/systemctl.real ] || mv /usr/bin/systemctl /usr/bin/systemctl.real'
    docker exec -i "$C" sh -c 'cat > /usr/bin/systemctl && chmod +x /usr/bin/systemctl' <<'EOF'
#!/bin/sh
# Stand-in for systemctl: knows tailscaled, passes the rest on (scripts/vdev-fake-tailscale.sh).
D=/tmp/fakets
case " $* " in
  *" tailscaled "*|*" tailscaled")
    echo "systemctl $*" >> $D/calls
    case "$1" in
      start) touch $D/active; exit 0 ;;
      stop) rm -f $D/active; exit 0 ;;
      is-active) [ -e $D/active ]; exit $? ;;
    esac ;;
esac
exec /usr/bin/systemctl.real "$@"
EOF
    echo "stand-ins installed in $C" ;;
  approve) dx touch /tmp/fakets/approve ;;
  state)
    dx sh -c 'echo "service: $([ -e /tmp/fakets/active ] && echo active || echo inactive)"; echo "login: $(cat /tmp/fakets/state 2>/dev/null || echo needslogin)"; echo "hostname: $(cat /tmp/fakets/hostname 2>/dev/null || echo "(default)")"; echo "system.cfg: $(grep "^tailscale" /storage/.config/system/configs/system.cfg 2>/dev/null || echo "(no tailscale.up)")"; echo "calls:"; grep -v "is-active\|status --json" /tmp/fakets/calls 2>/dev/null | sed "s/^/  /"' ;;
  hang) dx touch /tmp/fakets/hang ;;
  unhang) dx rm -f /tmp/fakets/hang ;;
  reset) dx sh -c 'rm -f /tmp/fakets/active /tmp/fakets/state /tmp/fakets/approve /tmp/fakets/hostname /tmp/fakets/calls' ;;
  *) echo "usage: VDEV_NAME=<name> $0 install|approve|state|reset|hang|unhang" >&2; exit 2 ;;
esac
