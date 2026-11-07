#!/bin/sh
# Quick test of devlock.sh on macOS and Linux: a second caller waits for the first and runs after it, and a
# stale lock (older than 20 min) is broken. Uses a lock of its own (POCKET_DEVLOCK), never the real one.
#   scripts/test-devlock.sh [devlock.sh to test]
# On Linux, in the image of scripts/test-linux.sh:
#   docker run --rm -v "$PWD:/src" -w /src pocket-ui-test:trixie sh scripts/test-devlock.sh
set -u
DEVLOCK=${1:-$(dirname "$0")/devlock.sh}
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
export POCKET_DEVLOCK="$tmp/lock" POCKET_DEVLOCK_POLL=0.01
BUDGET=$(python3 "$(dirname "$0")/test-time-budget.py" 1)
REAL_SLEEP=$(command -v sleep)
mkdir "$tmp/bin"
cat >"$tmp/bin/sleep" <<EOF
#!/bin/sh
: >"$tmp/waiter-polled"
exec "$REAL_SLEEP" "\$@"
EOF
chmod +x "$tmp/bin/sleep"
export PATH="$tmp/bin:$PATH"
failed=0
check() { # <what> <expected> <actual>
  if [ "$2" = "$3" ]; then echo "ok   $1"; else echo "FAIL $1: expected '$2', got '$3'"; failed=1; fi
}
# Runs a command, killed after <secs> seconds (a lock that is never released or broken would hang the test).
limit() { # <secs> <command...>; reap the owned command, no sleeping watchdog left behind
  secs=$(($1 * BUDGET))
  shift
  python3 - "$secs" "$@" <<'LIMIT'
import os, signal, subprocess, sys
child = subprocess.Popen(sys.argv[2:], start_new_session=True)
try:
    raise SystemExit(child.wait(timeout=float(sys.argv[1])))
except subprocess.TimeoutExpired:
    os.killpg(child.pid, signal.SIGTERM)
    try:
        child.wait(timeout=1)
    except subprocess.TimeoutExpired:
        os.killpg(child.pid, signal.SIGKILL)
        child.wait()
    raise SystemExit(124)
LIMIT
}

# 1. While one run holds the lock, a second one waits and runs afterwards.
mkfifo "$tmp/release"
sh "$DEVLOCK" first sh -c "echo first-start >>'$tmp/log'; read release <'$tmp/release'; echo first-end >>'$tmp/log'" &
first=$!
n=0
until [ -f "$tmp/log" ] || [ $n -ge $((50 * BUDGET)) ]; do
  "$REAL_SLEEP" 0.1
  n=$((n + 1))
done
limit 20 sh "$DEVLOCK" second sh -c "echo second >>'$tmp/log'" &
second=$!
n=0
until [ -f "$tmp/waiter-polled" ] || [ $n -ge $((50 * BUDGET)) ]; do
  "$REAL_SLEEP" 0.1
  n=$((n + 1))
done
check "second caller encountered the held lock" yes "$([ -f "$tmp/waiter-polled" ] && echo yes || echo no)"
echo release >"$tmp/release"
wait "$second"
check "waiter exits cleanly" 0 $?
wait "$first"
check "waiter runs after the holder" "first-start first-end second" "$(tr '\n' ' ' <"$tmp/log" | sed 's/ $//')"
check "lock released" no "$([ -e "$POCKET_DEVLOCK" ] && echo yes || echo no)"

# 2. A lock left behind long ago (a killed session) is broken instead of waited for.
mkdir "$POCKET_DEVLOCK"
touch -t 200001010000 "$POCKET_DEVLOCK"
limit 10 sh "$DEVLOCK" third sh -c "echo third >'$tmp/stale'"
check "stale lock broken" 0 $?
check "command ran" third "$(cat "$tmp/stale" 2>/dev/null)"

exit $failed
