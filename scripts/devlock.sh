#!/bin/sh
# Serialises access to the one test device between parallel sessions.
#   devlock.sh <who> <command...>   waits for the device, runs the command, releases it.
# Keep each locked run short (deploy, test, screenshots); a lock older than 20 min counts as stale.
# Works on macOS and Linux (scripts/test-devlock.sh). POCKET_DEVLOCK and POCKET_DEVLOCK_POLL are for that test.
LOCK=${POCKET_DEVLOCK:-/tmp/pocket-device.lock}
POLL=${POCKET_DEVLOCK_POLL:-5}
WHO=$1
shift
# When the lock was taken, in seconds since the epoch: GNU or BusyBox stat, else BSD stat (macOS). GNU stat
# understands `-f %m` too, but as file-system information, so the order matters.
mtime() { stat -c %Y "$1" 2>/dev/null || stat -f %m "$1" 2>/dev/null; }
while ! mkdir "$LOCK" 2>/dev/null; do
  taken=$(mtime "$LOCK")
  # Released in the meantime (or unreadable): counts as just taken, the next round sees.
  case $taken in '' | *[!0-9]*) taken=$(date +%s) ;; esac
  if [ $(($(date +%s) - taken)) -gt 1200 ]; then
    rm -rf "$LOCK"
    continue
  fi
  sleep "$POLL"
done
echo "$WHO $$" >"$LOCK/owner"
trap 'rm -rf "$LOCK"' EXIT INT TERM
"$@"
