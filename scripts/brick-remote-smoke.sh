#!/bin/bash
# The Brick card's network parts on a real aarch64 CPU, no Brick (docs/TRIMUI-BRICK-REMOTE.md): an arm64 Debian container with
# BusyBox (the Brick's shell and tools) on an Apple-silicon Mac with Colima.
#   scripts/brick-remote-smoke.sh PARTS     PARTS = what scripts/brick-remote-parts.sh made
# Checks: Tailscale and curl run (static, arm64), curl does https with its own CA bundle, brick-tailscale starts tailscaled (userspace
# networking: the container has no tunnel device), the CLI answers through the card's socket, `tailscale up` prints the sign-in
# address (nobody signs in; the address expires), the login state reaches the card, Tailscale's local proxy carries https; dropbear
# lets root in with the device's password and the firmware's shell, not with the firmware's own root password, scp -O works, and
# switching off ends it. Needs network (apt, Tailscale's servers). Prints PASS/FAIL; exit 0 only when all pass.
set -u
PARTS=$(cd "${1:?parts dir}" && pwd); ROOT=$(cd "$(dirname "$0")/.." && pwd)
W=$(mktemp -d "${TMPDIR:-/tmp}/brick-remote-smoke.XXXXXX"); trap 'rm -rf "$W"' EXIT
mkdir -p "$W/hw"; cp "$ROOT"/deploy/brick/hwroot/usr/bin/{brick-ssh,brick-tailscale,tailscale,systemctl} "$W/hw/"
cat >"$W/inside.sh" <<'IN'
fails=0; pass() { echo "PASS $*"; }; fail() { echo "FAIL $*"; fails=$((fails+1)); }
apt-get -qq update >/dev/null 2>&1; DEBIAN_FRONTEND=noninteractive apt-get -qq install -y busybox-static openssh-client sshpass passwd >/dev/null 2>&1 || echo "(no apt network)"; mkdir -p /bb && busybox --install -s /bb; export PATH=/bb:$PATH
A=/mnt/SDCARD/pocket-ui/app; D=/mnt/SDCARD/pocket-ui; RUN=/tmp/pocket-ui
mkdir -p $A/bin $A/hwroot/usr/bin $D/storage/.config/pocket-ui/remote $RUN/bin
cp /parts/bin/* $A/bin/; cp /hw/* $A/hwroot/usr/bin/; chmod +x $A/bin/* $A/hwroot/usr/bin/*
export BRICK_APP=$A BRICK_DATA=$D BRICK_RUN=$RUN BRICK_STATE=$D/storage/.config/pocket-ui BRICK_TS_SYNC_EVERY=2
H=$A/hwroot/usr/bin
$A/bin/tailscaled --version | head -1 | grep -q 1.104.5 && pass "tailscaled runs (arm64, static)" || fail "tailscaled --version"
$A/bin/tailscale --version | head -1 | grep -q 1.104.5 && pass "tailscale runs" || fail "tailscale --version"
$A/bin/curl -fsS -o /dev/null -w '%{http_code}' https://pkgs.tailscale.com/stable/ | grep -q 200 && pass "curl: https with its own CA bundle (no system certificates used)" || fail "curl https"
[ -e /dev/net/tun ] && echo "(this container has a tun device)"
$H/brick-tailscale start && pass "brick-tailscale start ($(cat $RUN/tailscale-mode))" || fail "brick-tailscale start: $(tail -3 $RUN/tailscaled.log)"
sleep 3
st=$($H/tailscale status --json 2>/dev/null | tr -d ' \n' | grep -o '"BackendState":"[A-Za-z]*"')
echo "$st" | grep -q NeedsLogin && pass "the daemon answers through the card's socket: $st" || fail "status: $st"
out=$($H/tailscale up --reset --ssh=false --accept-dns=false --netfilter-mode=off --hostname=pocket-brick-smoke --timeout=20s 2>&1)
echo "$out" | grep -q 'https://login.tailscale.com/' && pass "tailscale up prints the sign-in address (nobody signs in here)" || fail "no sign-in address: $(echo "$out" | tail -2)"
sleep 3
[ -s $D/storage/.cache/tailscale/tailscaled.state ] && pass "the state reached the card ($(wc -c <$D/storage/.cache/tailscale/tailscaled.state) bytes)" || fail "no state on the card"
$A/bin/curl -fsS --proxy http://127.0.0.1:1055 -o /dev/null -w '%{http_code}' https://pkgs.tailscale.com/stable/ | grep -q 200 && pass "Tailscale's local proxy carries https (what the updater uses)" || fail "proxy"
$H/brick-tailscale stop; $H/brick-tailscale status && fail "still running after stop" || pass "brick-tailscale stop"
if command -v sshpass >/dev/null; then
  ln -sf /bin/busybox /bin/ash; sed -i "s#^root:.*#root:x:0:0:root:/root:/bin/ash#" /etc/passwd; rm -f /etc/shells
  echo 'root:firmware-pass' | chpasswd >/dev/null 2>&1
  echo 'otter-cedar-lamp-315' >$D/storage/.config/pocket-ui/remote/password
  ln -sf $A/bin/dropbearmulti $RUN/bin/scp
  $H/brick-ssh start && pass "dropbear starts (arm64, static)" || fail "brick-ssh start: $(tail -3 $RUN/dropbear.log)"
  O="-o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o LogLevel=ERROR -o PubkeyAuthentication=no"
  r=$(sshpass -p otter-cedar-lamp-315 ssh $O root@127.0.0.1 'echo in:$HOME:$SHELL' 2>&1)
  echo "$r" | grep -q "^in:$RUN/ssh/home:/bin/ash" && pass "root logs in with the device's password; home in RAM; the firmware's shell (not in /etc/shells) is taken" || fail "login: $r $(tail -3 $RUN/dropbear.log)"
  sshpass -p firmware-pass ssh $O root@127.0.0.1 true 2>/dev/null && fail "the firmware's root password logs in" || pass "the firmware's own root password does not log in"
  echo hello >/tmp/f; sshpass -p otter-cedar-lamp-315 scp -O $O root@127.0.0.1:/tmp/f /tmp/g 2>/dev/null && grep -q hello /tmp/g && pass "scp -O copies a file off the device" || fail "scp -O"
  $H/brick-ssh stop; sshpass -p otter-cedar-lamp-315 ssh $O -o ConnectTimeout=3 root@127.0.0.1 true 2>/dev/null && fail "login after stop" || pass "no login after switching off"
fi
echo "fails=$fails"
[ "$fails" = 0 ]
IN
docker run --rm --name "brick-remote-smoke-$$" --platform linux/arm64 -v "$PARTS":/parts:ro -v "$W/hw":/hw:ro -v "$W/inside.sh":/inside.sh:ro \
  debian:trixie-slim sh -c "apt-get -qq update >/dev/null 2>&1; DEBIAN_FRONTEND=noninteractive apt-get -qq install -y busybox-static >/dev/null 2>&1; busybox ash /inside.sh"
