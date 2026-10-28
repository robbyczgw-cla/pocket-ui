#!/bin/sh
# Tests for scripts/verify-device.sh against a fake device: a fake `ssh` that runs the remote script here, a fake
# /proc (VERIFY_PROC), fake `systemctl` and `pidof`. Nothing connects anywhere. About 2 s.
#   scripts/test-verify-device.sh
set -u
here=$(cd "$(dirname "$0")" && pwd)
V=$here/verify-device.sh
T=$(mktemp -d)
trap 'rm -rf "$T"' EXIT
mkdir -p "$T/bin" "$T/build"
fails=0
n=0

# ssh [-o X]... host command: runs the command here, the way sshd's shell would (and notes that it was called).
cat >"$T/bin/ssh" <<EOF
#!/bin/sh
echo ssh >>"$T/ssh-calls"
while [ "\$1" = -o ]; do shift 2; done
[ "\$FAKE_SSH_DOWN" = 1 ] && { echo "ssh: connect to host \$1: Connection refused" >&2; exit 255; }
shift
exec sh -c "\$1"
EOF
# systemctl is-active <unit>: the state in $T/state (default active); pidof <name>: the words in $T/pids/<name>.
cat >"$T/bin/systemctl" <<EOF
#!/bin/sh
echo "\$*" >>"$T/systemctl-calls"
[ "\$1" = is-active ] || exit 0
s=active; [ -f "$T/state" ] && s=\$(cat "$T/state")
echo "\$s"; [ "\$s" = active ]
EOF
cat >"$T/bin/pidof" <<EOF
#!/bin/sh
[ -s "$T/pids/\$1" ] || exit 1
cat "$T/pids/\$1"
EOF
chmod +x "$T/bin/ssh" "$T/bin/systemctl" "$T/bin/pidof"

export PATH="$T/bin:$PATH"
export VERIFY_PROC=$T/proc VERIFY_STORAGE=$T/storage VERIFY_TMP=$T/tmp VERIFY_BUILD_DIR=$T/build VERIFY_WAIT=0
unset POCKETUI_HOST

# A healthy device: the boot-UI drop-in, binaries "deployed", one pocket-ui (pid 100) in essway.service, running them.
healthy() {
  rm -rf "$T/proc" "$T/storage" "$T/tmp" "$T/pids" "$T/state" "$T/build" "$T/ssh-calls"
  mkdir -p "$T/proc" "$T/tmp" "$T/pids" "$T/build" "$T/storage/.config/system.d/essway.service.d" "$T/storage/pocket-ui/releases/dev"
  : >"$T/storage/.config/system.d/essway.service.d/10-pocket-ui.conf"
  echo "fake pocket-ui build" >"$T/storage/pocket-ui/releases/dev/pocket-ui"
  echo "fake pocket-core build" >"$T/storage/pocket-ui/releases/dev/pocket-core"
  ln -s releases/dev "$T/storage/pocket-ui/current"
  for b in pocket-ui pocket-core; do ln -s current/$b "$T/storage/pocket-ui/$b"; done
  cp "$T/storage/pocket-ui/releases/dev/pocket-ui" "$T/storage/pocket-ui/releases/dev/pocket-core" "$T/build/"
  echo "40.50 11111.11" >"$T/proc/uptime"
  proc ui 100 pocket-ui "0::/system.slice/essway.service" ""
}

# proc kind pid binary cgroup "ENV=1 ENV2=2": a fake /proc/<pid> whose exe is a copy of the deployed binary.
proc() {
  d=$T/proc/$2
  mkdir -p "$d"
  cp "$T/storage/pocket-ui/releases/dev/$3" "$d/exe"
  echo "$4" >"$d/cgroup"
  : >"$d/environ"
  for kv in $5; do printf '%s\0' "$kv" >>"$d/environ"; done
  printf 'XDG_RUNTIME_DIR=/x\0' >>"$d/environ"
  # pid (comm with a space and a paren) state ... field 22 = starttime, 1000 ticks after boot (hz 100 -> 10 s)
  echo "$2 ($3 x) S 1 1 1 0 -1 0 0 0 0 0 0 0 0 0 20 0 1 0 1000 0 0" >"$d/stat"
  p=$(cat "$T/pids/$3" 2>/dev/null)
  echo "${p:+$p }$2" >"$T/pids/$3"
}

# run [args]: output in $out, exit status in $rc.
run() { out=$(sh "$V" "$@" 2>&1); rc=$?; }

# want <name> <exit> <grep-pattern>...: the run's exit status and every pattern found in the output.
want() {
  name=$1; code=$2; shift 2
  n=$((n + 1))
  bad=
  [ "$rc" = "$code" ] || bad="exit $rc, wanted $code"
  for pat in "$@"; do
    printf '%s\n' "$out" | grep -q -- "$pat" || bad="$bad; missing '$pat'"
  done
  if [ -z "$bad" ]; then echo "ok   $name"; else echo "FAIL $name:$bad"; printf '%s\n' "$out" | sed 's/^/       | /'; fails=$((fails + 1)); fi
}
wantnot() { # <name> <pattern>
  n=$((n + 1))
  if printf '%s\n' "$out" | grep -q -- "$2"; then echo "FAIL $1: found '$2'"; printf '%s\n' "$out" | sed 's/^/       | /'; fails=$((fails + 1)); else echo "ok   $1"; fi
}

healthy
run root@fake
want "a healthy device passes" 0 "^ok   essway.service is active" "^ok   one pocket-ui: pid 100, running for 30 s" \
  "^ok   pocket-ui 100 is in essway.service" "^ok   pocket-ui 100 environment is clean" "^ok   pocket-ui 100 runs the deployed binary" \
  "^ok   pocket-ui deployed md5 .* equals the local build" "^ok   no pocket-core is running" "^VERIFY: ok$"
wantnot "a healthy device has no FAIL line" "^FAIL"
n=$((n + 1))
if [ "$(wc -l <"$T/ssh-calls")" = 1 ]; then echo "ok   one ssh session for all the checks"; else echo "FAIL one ssh session for all the checks"; fails=$((fails + 1)); fi

healthy; run
want "no host: stops with the message, exit 2" 2 "POCKETUI_HOST"
POCKETUI_HOST=root@fake run; want "the host from POCKETUI_HOST" 0 "^VERIFY: ok$"

healthy; echo inactive >"$T/state"; run root@fake
want "essway not active fails" 1 "^FAIL essway.service is 'inactive'" "^VERIFY: FAILED (service)"

healthy; : >"$T/pids/pocket-ui"; rm -rf "$T/proc/100"; run root@fake
want "no pocket-ui fails" 1 "^FAIL no pocket-ui is running" "^VERIFY: FAILED (pocket-ui)"
healthy; : >"$T/pids/pocket-ui"; rm -rf "$T/proc/100"; touch "$T/tmp/pocket-ui.es-only"; run root@fake
want "no pocket-ui says why when the EmulationStation mark is set" 1 "es-only is set"

healthy; proc ui 200 pocket-ui "0::/user.slice/session-3.scope" "POCKETUI_TRACE=/tmp/t.log POCKETUI_NO_HISTORY=1"; run root@fake
want "an orphan next to the unit's pocket-ui fails: count, cgroup, env" 1 "^FAIL 2 pocket-ui are running" \
  "^FAIL pocket-ui 200 is not in essway.service: cgroup '0::/user.slice/session-3.scope" "^FAIL pocket-ui 200 has POCKETUI_TRACE=/tmp/t.log, POCKETUI_NO_HISTORY=1" \
  "^VERIFY: FAILED (pocket-ui, cgroup, env)"
want "  ... and the healthy one is still reported ok" 1 "^ok   pocket-ui 100 is in essway.service"

healthy; rm -rf "$T/proc/100" "$T/pids/pocket-ui"; proc ui 100 pocket-ui "0::/system.slice/essway.service" "POCKETUI_TRACE=/tmp/pocket-trace.log"; run root@fake
want "POCKETUI_TRACE set fails" 1 "^FAIL pocket-ui 100 has POCKETUI_TRACE=/tmp/pocket-trace.log" "^VERIFY: FAILED (env)"
healthy; rm -rf "$T/proc/100" "$T/pids/pocket-ui"; proc ui 100 pocket-ui "0::/system.slice/essway.service" "POCKETUI_NO_HISTORY=1"; run root@fake
want "POCKETUI_NO_HISTORY=1 fails" 1 "^FAIL pocket-ui 100 has POCKETUI_NO_HISTORY=1"
healthy; rm -rf "$T/proc/100" "$T/pids/pocket-ui"; proc ui 100 pocket-ui "0::/system.slice/essway.service" "POCKETUI_NO_HISTORY=0"; run root@fake
want "POCKETUI_NO_HISTORY=0 is fine (it disables the switch)" 0 "^VERIFY: ok$"
healthy; rm -rf "$T/proc/100" "$T/pids/pocket-ui"; proc ui 100 pocket-ui "0::/system.slice/essway.service" "POCKETUI_TRACE_OTHER=1 POCKETUI_NO_HISTORYX=1"; run root@fake
want "variables that only start with the names do not count" 0 "^VERIFY: ok$"

healthy; echo "an older build" >"$T/proc/100/exe"; run root@fake
want "a running binary that is not the deployed one fails (text busy)" 1 "^FAIL pocket-ui 100 runs md5 '[0-9a-f]*' but the deployed" "^VERIFY: FAILED (md5)"
healthy; rm "$T/proc/100/exe"; run root@fake
want "an unreadable exe fails" 1 "runs md5 'unreadable'" "^VERIFY: FAILED (md5)"
healthy; echo "built after the deploy" >"$T/build/pocket-ui"; run root@fake
want "deployed binary differs from the local build fails" 1 "^FAIL pocket-ui deployed md5 .* differs from the local build" "^VERIFY: FAILED (md5)"
run --no-local root@fake
want "--no-local skips the local comparison" 0 "^info pocket-ui: deployed md5 .*--no-local" "^VERIFY: ok$"
healthy; rm "$T/build/pocket-core" "$T/build/pocket-ui"; run root@fake
want "no local build is reported, not a failure" 0 "^info pocket-ui: no local build in" "^VERIFY: ok$"
healthy; rm "$T/storage/pocket-ui/releases/dev/pocket-core"; run root@fake
want "a missing deployed binary fails" 1 "^FAIL no deployed pocket-core" "^VERIFY: FAILED (md5)"

healthy; proc core 300 pocket-core "0::/system.slice/essway.service" ""; run root@fake
want "a running pocket-core is reported, not a failure" 0 "^info pocket-core is running (pids 300)" "^ok   pocket-core 300 runs the deployed binary" "^VERIFY: ok$"
healthy; proc core 300 pocket-core "0::/system.slice/essway.service" "POCKETUI_TRACE=/tmp/t"; run root@fake
want "a pocket-core with the trace set fails" 1 "^FAIL pocket-core 300 has POCKETUI_TRACE" "^VERIFY: FAILED (env)"
healthy; proc core 300 pocket-core "0::/system.slice/essway.service" ""; echo old >"$T/proc/300/exe"; run root@fake
want "a stale pocket-core fails" 1 "^FAIL pocket-core 300 runs md5" "^VERIFY: FAILED (md5)"

# Without the boot-UI drop-in pocket-ui runs as the transient unit pocketui (dev.sh deploy's other way).
healthy; rm "$T/storage/.config/system.d/essway.service.d/10-pocket-ui.conf"; run root@fake
want "no drop-in: the unit is pocketui and a pocket-ui in essway fails" 1 "^FAIL pocket-ui 100 is not in pocketui.service" "^ok   pocketui.service is active"
echo "0::/system.slice/pocketui.service" >"$T/proc/100/cgroup"; run root@fake
want "no drop-in: a pocket-ui in pocketui.service passes" 0 "^ok   pocket-ui 100 is in pocketui.service" "^VERIFY: ok$"

healthy; FAKE_SSH_DOWN=1 run root@fake
want "ssh down fails with the reason" 1 "^FAIL could not read the device's state over ssh (exit 255).*Connection refused" "^VERIFY: FAILED (ssh)"

run --bogus; want "unknown option: exit 2" 2 "unknown option"

# latency.sh's DEVICE_RESTORE (what the device runs after a measurement): an orphan pocket-ui and pocket-core of the
# harness are killed, then essway is restarted. Real sleeping processes stand in for them; pidof lists the live ones.
mkdir -p "$T/restore-bin"
cat >"$T/restore-bin/pidof" <<EOF
#!/bin/sh
r=
for p in \$(cat "$T/live/\$1" 2>/dev/null); do kill -0 \$p 2>/dev/null && r="\$r \$p"; done
[ -n "\$r" ] && echo \$r
EOF
cp "$T/bin/systemctl" "$T/restore-bin/systemctl"
printf '#!/bin/sh\necho "systemd-run $*" >>"%s/systemctl-calls"\n' "$T" >"$T/restore-bin/systemd-run"
chmod +x "$T/restore-bin/pidof" "$T/restore-bin/systemd-run"
restore() { # $1: yes when the boot-UI drop-in exists
  rm -rf "$T/live" "$T/systemctl-calls"; mkdir -p "$T/live"
  # In a subshell: they are then not this shell's children, and a killed one does not stay behind as a zombie.
  (sleep 60 >/dev/null 2>&1 & echo $! >"$T/live/pocket-ui")
  (sleep 60 >/dev/null 2>&1 & echo $! >"$T/live/pocket-core")
  dropin=$T/dropin; rm -f "$dropin"; [ "$1" = yes ] && : >"$dropin"
  eval "$(sed -n "/^DEVICE_RESTORE='/,/fi'\$/p" "$here/latency.sh")"
  PATH="$T/restore-bin:$PATH" DROPIN=$dropin sh -c "$DEVICE_RESTORE"
  alive=
  for f in pocket-ui pocket-core; do kill -0 "$(cat "$T/live/$f")" 2>/dev/null && alive="$alive $f"; done
  kill "$(cat "$T/live/pocket-ui")" "$(cat "$T/live/pocket-core")" 2>/dev/null
  out="alive:$alive
$(cat "$T/systemctl-calls")"; rc=0
}
restore yes
want "latency.sh restore kills the harness's pocket-ui and pocket-core, restarts essway" 0 "^alive:$" "^restart essway"
restore no
want "latency.sh restore without the drop-in starts the pocketui unit" 0 "^alive:$" "^systemd-run --unit=pocketui"

echo
if [ $fails -eq 0 ]; then echo "test-verify-device: all $n passed"; else echo "test-verify-device: $fails of $n FAILED"; exit 1; fi
