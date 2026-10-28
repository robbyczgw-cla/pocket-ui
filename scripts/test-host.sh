#!/bin/sh
# Test for scripts/host.sh: the device address comes from POCKETUI_HOST or scripts/local.env, never from a default.
# Uses a copy of host.sh in a temp folder and a fake `ssh`; nothing connects anywhere.
#   scripts/test-host.sh
set -u
here=$(cd "$(dirname "$0")" && pwd)
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
mkdir -p "$tmp/scripts" "$tmp/bin"
cp "$here/host.sh" "$tmp/scripts/host.sh"
printf '#!/bin/sh\n. "$(dirname "$0")/host.sh"\nssh "$HOST" true\necho "ran against $HOST"\n' > "$tmp/scripts/t.sh"
printf '#!/bin/sh\necho "fake ssh $*"\n' > "$tmp/bin/ssh"
chmod +x "$tmp/scripts/t.sh" "$tmp/bin/ssh"
failed=0
check() { # <what> <want> <got>
  if [ "$2" = "$3" ]; then echo "ok   $1"; else echo "FAIL $1: wanted '$2', got '$3'"; failed=1; fi
}
run() { PATH="$tmp/bin:$PATH" sh "$tmp/scripts/t.sh" 2>&1; }

unset POCKETUI_HOST
out=$(run); rc=$?
check "nothing set: exit status 2" 2 "$rc"
case "$out" in *POCKETUI_HOST*local.env*) check "nothing set: the message names both ways" yes yes ;; *) check "nothing set: the message names both ways" yes "$out" ;; esac
case "$out" in *"fake ssh"*) check "nothing set: ssh is not started" no yes ;; *) check "nothing set: ssh is not started" no no ;; esac

POCKETUI_HOST=root@192.0.2.1; export POCKETUI_HOST
check "environment variable is used" "fake ssh root@192.0.2.1 true
ran against root@192.0.2.1" "$(run)"

printf 'POCKETUI_HOST=root@192.0.2.2\n' > "$tmp/scripts/local.env"
check "the environment wins over local.env" "fake ssh root@192.0.2.1 true
ran against root@192.0.2.1" "$(run)"

unset POCKETUI_HOST
check "local.env is used when the environment is empty" "fake ssh root@192.0.2.2 true
ran against root@192.0.2.2" "$(run)"

# no address in any script of the repository
leak=$(grep -l -E '(^|[^0-9.])(10\.[0-9]+\.[0-9]+\.[0-9]+|192\.168\.[0-9]+\.[0-9]+)' "$here"/dev.sh "$here"/syscheck.sh "$here"/hwplay.sh "$here"/harden-ssh.sh "$here"/autostart.sh "$here"/host.sh 2>/dev/null)
check "no private address in the dev scripts" "" "$leak"

[ "$failed" -eq 0 ] && echo "all passed"
exit "$failed"
