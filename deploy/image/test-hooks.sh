#!/bin/sh
# Tests for the SD image's initramfs hook post-flash.sh (mount-storage.sh: deploy/image/test-mount-storage.sh, in the gate).
#   sh deploy/image/test-hooks.sh
# The hook uses the real paths /flash, /sysroot and /dev/console, so the tests run as root in a throwaway container (the
# virtual device's base image: ROCKNIX's BusyBox, as in the initramfs; POCKETUI_TEST_IMAGE picks another one).
# POCKETUI_HOOKS=<dir> tests the hook in another directory (used to show that the tests fail without it).
set -u
HERE=$(cd "$(dirname "$0")" && pwd)
if [ "${POCKETUI_IN_TEST_CONTAINER:-}" != 1 ]; then
  exec docker run --rm -e POCKETUI_IN_TEST_CONTAINER=1 -v "${POCKETUI_HOOKS:-$HERE}:/hooks:ro" -v "$HERE/test-hooks.sh:/test-hooks.sh:ro" \
    "${POCKETUI_TEST_IMAGE:-pocket-vdev-base:20261001}" /bin/sh /test-hooks.sh
fi
SH="sh"
command -v busybox >/dev/null && SH="busybox sh"
fails=0

check() {
  if eval "$2"; then
    echo "ok   $1"
  else
    echo "FAIL $1"
    fails=$((fails + 1))
  fi
}

# A card as the image leaves it: post-flash.sh on /flash.
setup() {
  rm -rf /flash
  mkdir -p /flash
  cp /hooks/post-flash.sh /flash/ 2>/dev/null
}

echo "== post-flash.sh: no ROCKNIX logo"
setup
mkdir -p /sysroot/etc /usr/bin
echo "... Version: 20261001 (official)" >/sysroot/etc/issue
rm -f /usr/bin/rocknix-splash /tmp/splash-ran
printf '#!/bin/sh\n: >/tmp/splash-ran\n' >/usr/bin/rocknix-splash
chmod 755 /usr/bin/rocknix-splash
# /dev/console is a terminal on the device: each `>` adds to the screen. A FIFO with a reader does the same here.
rm -f /dev/console /tmp/console
mkfifo /dev/console
(while :; do cat /dev/console >>/tmp/console; done) &
reader=$!
$SH -c '
  load_splash() { /usr/bin/rocknix-splash; }
  mount_flash() { if [ -f /flash/post-flash.sh ]; then . /flash/post-flash.sh; fi; }
  mount_flash
  load_splash
'
sleep 0.3
kill $reader 2>/dev/null
check "rocknix-splash not started" '[ ! -e /tmp/splash-ran ]'
check "says pocket-ui, based on ROCKNIX" 'grep -q "pocket-ui, based on ROCKNIX" /tmp/console'
check "keeps the version line" 'grep -q "Version: 20261001" /tmp/console'
check "first start says Initializing" 'grep -q "Initializing, please wait" /tmp/console'

echo
if [ $fails -eq 0 ]; then echo "all passed"; else echo "$fails failed"; exit 1; fi
