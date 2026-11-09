#!/bin/sh
# Read where pocket-ui is before pressing (docs/DEVICE-TESTS.md "Where the UI is"). Runs ON the device (busybox sh); the
# dev machine's scripts pipe it over ssh:  ssh "$HOST" sh -s -- expect 'settings:*' < scripts/ui-status.sh
#
#   ui-status.sh status                       the status line as "<where>  sleep=<..> game=<..> rev=<n>"; exit 1 without one
#   ui-status.sh expect GLOB [SECONDS]        wait (default 5 s) until the UI is awake at a page matching GLOB, then print it;
#                                             exit 1 on timeout with what it says instead
#   ui-status.sh send [--at GLOB] [--launch] CMD...
#                                             the command FIFO's words (up down a b … wake), one after the other. --at:
#                                             first wait until the UI is at GLOB (else nothing is pressed, exit 3).
#                                             `a` is refused (exit 3) where it starts a game (a shelf, recent games, the
#                                             game card or box, the child's shelf) unless --launch is given, and while
#                                             the screen is dark or still waking (exit 4: send `wake` first, never `b`).
#                                             `wake` turns the screen on without a button and waits until presses count.
#
# GLOB is a shell pattern on the page and its covers as `@where` writes them: "shelf:Home", "settings:List Look",
# "pause:Main / quick", "setup:wifi*". A notice up makes it "shelf:Home / notice": "shelf:Home" no longer matches.
# POCKETUI_STATUS (default /tmp/pocket-ui.status) and POCKETUI_FIFO (default /tmp/pocket-ui.in) name the files.
set -f
STATUS=${POCKETUI_STATUS:-/tmp/pocket-ui.status}
FIFO=${POCKETUI_FIFO:-/tmp/pocket-ui.in}
TAB=$(printf '\t')

# The fields into st_*; 1: no file, 2: written by a pocket-ui that is gone (stale).
st_read() {
  st_place=; st_cover=; st_sleep=; st_game=; st_rev=; st_pid=
  line=$(cat "$STATUS" 2>/dev/null) || return 1
  [ -n "$line" ] || return 1
  old=$IFS; IFS=$TAB
  for f in $line; do
    case $f in
      place=*) st_place=${f#place=} ;;
      cover=*) st_cover=${f#cover=} ;;
      sleep=*) st_sleep=${f#sleep=} ;;
      game=*) st_game=${f#game=} ;;
      rev=*) st_rev=${f#rev=} ;;
      pid=*) st_pid=${f#pid=} ;;
    esac
  done
  IFS=$old
  [ -n "$st_pid" ] && [ -d "/proc/$st_pid" ] || return 2
  return 0
}

# The page and its covers, as @where: "shelf:Home / notice".
st_where() {
  if [ -z "$st_cover" ] || [ "$st_cover" = none ]; then printf '%s\n' "$st_place"; return; fi
  printf '%s / %s\n' "$st_place" "$(printf '%s' "$st_cover" | sed 's#,# / #g')"
}

st_show() { printf '%s  sleep=%s game=%s rev=%s\n' "$(st_where)" "$st_sleep" "$st_game" "$st_rev"; }

# Wait until awake at a page matching $1 (a pattern), at most $2 seconds.
st_wait() {
  n=$(( ${2:-5} * 10 ))
  while :; do
    if st_read && [ "$st_sleep" = awake ]; then
      # shellcheck disable=SC2254  # the pattern is meant to match
      case "$(st_where)" in $1) return 0 ;; esac
    fi
    n=$((n - 1))
    [ "$n" -gt 0 ] || return 1
    sleep 0.1
  done
}

# Where A starts a game (or plays the one under the cursor): nothing covers the page and it is one of these.
starts_a_game() {
  [ -z "$st_cover" ] || [ "$st_cover" = none ] || return 1
  case $st_place in shelf:*|"recent games"*|card|box|kids:shelf) return 0 ;; esac
  return 1
}

say() { printf '%s\n' "ui-status: $*" >&2; }

fifo() { timeout 2 sh -c 'printf "%s\n" "$1" > "$2"' sh "$1" "$FIFO"; }

cmd=${1:-}; [ $# -gt 0 ] && shift
case $cmd in
  status)
    st_read; rc=$?
    case $rc in
      0) st_show ;;
      1) say "no status file $STATUS (pocket-ui not running, or older than the status file)"; exit 1 ;;
      *) say "stale status file (pid $st_pid is gone): $(st_where)"; exit 1 ;;
    esac ;;
  expect)
    [ -n "${1:-}" ] || { say "usage: expect GLOB [SECONDS]"; exit 2; }
    if st_wait "$1" "${2:-5}"; then st_where; exit 0; fi
    if st_read; then say "not at '$1' after ${2:-5} s: $(st_show)"; else say "not at '$1': no live status file"; fi
    exit 1 ;;
  send)
    at= launch=
    while [ $# -gt 0 ]; do
      case $1 in
        --at) at=${2:-}; shift 2 ;;
        --launch) launch=1; shift ;;
        *) break ;;
      esac
    done
    pidof pocket-ui >/dev/null 2>&1 || [ -n "${POCKETUI_STATUS:-}" ] || { say "pocket-ui is not running"; exit 1; }
    if [ -n "$at" ] && ! st_wait "$at" 5; then
      if st_read; then say "not at '$at': $(st_show); nothing pressed"; else say "not at '$at': no live status file; nothing pressed"; fi
      exit 3
    fi
    for k in "$@"; do
      if [ "$k" = wake ]; then
        fifo wake || exit 1
        st_wait '*' 3 || { say "still not awake after wake: $(st_show)"; exit 4; }
        continue
      fi
      if st_read; then
        case $st_sleep in
          awake) ;;
          *) say "the screen is $st_sleep: '$k' would only wake it; send wake first"; exit 4 ;;
        esac
        if [ "$k" = a ] && [ -z "$launch" ] && starts_a_game; then
          say "A at $(st_where) starts a game: refused (--launch to mean it)"; exit 3
        fi
      elif [ "$k" = a ] && [ -z "$launch" ]; then
        say "no live status file: A could start a game blind: refused (--launch to mean it)"; exit 3
      else
        say "no live status file: pressing '$k' blind"
      fi
      fifo "$k" || exit 1
      sleep 0.2
    done ;;
  *)
    say "usage: ui-status.sh status | expect GLOB [SECONDS] | send [--at GLOB] [--launch] CMD..."; exit 2 ;;
esac
