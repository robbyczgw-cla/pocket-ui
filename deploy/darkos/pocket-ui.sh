#!/bin/bash
# pocket-ui in dArkOS's Ports (prototype, docs/DARKOS.md). Copy this file and the folder pocket-ui/ next to it into
# /roms/ports/ (or /roms2/ports/ on a second card). EmulationStation runs it as the user ark; the rest runs as root through
# ark's own sudo (dArkOS gives it without a password). Leaving pocket-ui ends this script, and EmulationStation comes back.
TTY=${DARKOS_TTY:-/dev/tty1}  # (the test hook of start.sh)
DIR=$(cd "$(dirname "$0")" && pwd)/pocket-ui
# start.sh is run by sh, so it needs neither the executable bit nor an untouched first line.
sudo -n /bin/sh "$DIR/start.sh" "$@"
code=$?
# start.sh ends with 0 (left normally) or 3 (it showed the reason on the screen itself). Anything else means it never ran
# properly (no sudo, folder not copied whole): say it here, on the console (tty1), the way dArkOS's own scripts print.
if [ "$code" != 0 ] && [ "$code" != 3 ]; then
  {
    printf '\033c\033[?25l\033[1;31mpocket-ui can'"'"'t start\033[0m\n'
    printf 'start.sh did not run (code %s).\n' "$code"
    printf 'Copy the whole pocket-ui folder and pocket-ui.sh\n'
    printf 'into ports again.\n\n'
    printf '\033[1;33mSend a photo of this screen.\033[0m\n'
  } >>"$TTY" 2>/dev/null
  sleep 15
  printf '\033c' >>"$TTY" 2>/dev/null
fi
exit 0
