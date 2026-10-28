#!/bin/sh
# The quiet start (docs/BOOT-SPLASH.md) on a card that boots through pocket-ui's GRUB and was set up before the SD
# image had it, from the computer over SSH. Runs deploy/grub-quiet.sh on the device:
#
#   boot-quiet.sh status   read-only: whether /flash/boot/grub/grub.cfg is quiet, and whether a backup is there
#   boot-quiet.sh apply    keeps grub.cfg as grub.cfg.before-quiet, writes the quiet one (effective at the next start)
#   boot-quiet.sh undo     puts grub.cfg.before-quiet back
# Options: --host root@<device-ip> (or POCKETUI_HOST, or scripts/local.env)
#
# Only /flash/boot/grub/grub.cfg and its backup next to it are written. The script refuses a grub.cfg that is not
# pocket-ui's. A card that no longer starts: Boot mode: Android, then copy boot/grub/grub.cfg.before-quiet over
# boot/grub/grub.cfg on the card's first partition from a computer.
set -eu
. "$(dirname "$0")/host.sh"
ROOT=$(cd "$(dirname "$0")/.." && pwd)
ACTION=
while [ $# -gt 0 ]; do
  case $1 in
    status | apply | undo) ACTION=$1 ;;
    --host) HOST=$2; shift ;;
    -h | --help) sed -n '2,13p' "$0"; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
  shift
done
[ -n "$ACTION" ] || { sed -n '2,13p' "$0" >&2; exit 2; }
ssh -o BatchMode=yes -o ConnectTimeout=10 "$HOST" sh -s -- "$ACTION" <"$ROOT/deploy/grub-quiet.sh"
