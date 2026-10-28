# Sourced (`. "$(dirname "$0")/host.sh"`) by the scripts that talk to the device over SSH: dev.sh, syscheck.sh,
# hwplay.sh, harden-ssh.sh, autostart.sh. There is no address in the repository.
#
# The SSH target comes from, in this order:
#   1. the environment:            export POCKETUI_HOST=root@<device-ip>
#   2. scripts/local.env           your own file, not tracked (copy scripts/local.env.example): POCKETUI_HOST=root@<device-ip>
# `--host` of harden-ssh.sh and autostart.sh overrides both. Without any of them, the first ssh/scp call stops with
# a message instead of guessing.
if [ -z "${POCKETUI_HOST:-}" ] && [ -f "$(dirname "$0")/local.env" ]; then
  . "$(dirname "$0")/local.env"
fi
HOST=${POCKETUI_HOST:-}
need_host() {
  [ -n "${HOST:-}" ] && return 0
  echo "No device address. Set POCKETUI_HOST (export POCKETUI_HOST=root@<device-ip>) or put it in scripts/local.env" >&2
  echo "(see scripts/local.env.example and docs/DEVELOPING.md)." >&2
  exit 2
}
ssh() { need_host; command ssh "$@"; }
scp() { need_host; command scp "$@"; }
