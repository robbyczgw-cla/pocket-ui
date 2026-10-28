#!/bin/sh
# Tests for the two initramfs hooks of the SD image, deploy/image/mount-storage.sh and post-flash.sh.
#   sh deploy/image/test-hooks.sh
# The hooks use the real paths /storage and /flash, so the tests run as root in a throwaway container (the virtual
# device's base image: ROCKNIX's BusyBox, as in the initramfs; POCKETUI_TEST_IMAGE picks another one). mount_part,
# mount and progress are fakes that log their calls, as ROCKNIX's init would provide them.
# POCKETUI_HOOKS=<dir> tests the hooks in another directory (used to show that the tests fail without them).
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

DROPIN=.config/system.d/essway.service.d/10-pocket-ui.conf

# A fresh card as the image leaves it. $1 = resize: STORAGE not grown yet (first boot).
setup() {
  rm -rf /storage /flash /calls
  mkdir -p /storage/pocket-ui "/storage/.pocket-seed/$(dirname $DROPIN)" /flash
  echo '[Service]' >"/storage/.pocket-seed/$DROPIN"
  chmod 644 "/storage/.pocket-seed/$DROPIN"
  cp /hooks/mount-storage.sh /hooks/post-flash.sh /flash/ 2>/dev/null
  [ "${1:-}" = resize ] && : >/storage/.please_resize_me
  : >/calls
}

# Sources mount-storage.sh as ROCKNIX's init does (inside mount_storage, $disk set), with logging fakes.
boot() {
  $SH -c '
    mount_part() { echo "mount_part $*" >>/calls; }
    mount() { echo "mount $*" >>/calls; }
    progress() { echo "progress $*" >>/calls; }
    disk=LABEL=STORAGE
    mount_storage() { if [ -f /flash/mount-storage.sh ]; then . /flash/mount-storage.sh; else mount_part "$disk" /storage rw,noatime; fi; }
    mount_storage
  '
}

echo "== first boot: STORAGE still 32 MB, ROCKNIX will grow it"
setup resize
boot
check "mounts STORAGE as ROCKNIX does" 'grep -qx "mount_part LABEL=STORAGE /storage rw,noatime" /calls'
check "no /storage/.config yet (fs-resize would refuse)" '[ ! -e /storage/.config ]'
check "seed stays" '[ -f "/storage/.pocket-seed/$DROPIN" ]'
check "hook stays on /flash" '[ -f /flash/mount-storage.sh ]'
check "/flash not touched" '! grep -q "^mount " /calls'

echo "== second boot: STORAGE grown"
rm -f /storage/.please_resize_me
: >/calls
boot
check "mounts STORAGE" 'grep -qx "mount_part LABEL=STORAGE /storage rw,noatime" /calls'
check "drop-in in place" '[ "$(cat "/storage/$DROPIN" 2>/dev/null)" = "[Service]" ]'
check "drop-in mode 644" '[ "$(stat -c %a "/storage/$DROPIN" 2>/dev/null)" = 644 ]'
check "seed gone" '[ ! -e /storage/.pocket-seed ]'
check "pocket-ui untouched" '[ -d /storage/pocket-ui ]'
check "hook deleted itself" '[ ! -e /flash/mount-storage.sh ]'
check "/flash rw for the delete, then ro again" '[ "$(grep "^mount " /calls | tr "\n" "|")" = "mount -o remount,rw /flash|mount -o remount,ro /flash|" ]'

echo "== later boots: ROCKNIX mounts /storage itself"
: >/calls
boot
check "plain mount" '[ "$(cat /calls)" = "mount_part LABEL=STORAGE /storage rw,noatime" ]'

echo "== the seed was already moved but the hook could not delete itself"
setup
rm -rf /storage/.pocket-seed
mkdir -p /storage/.config/pocket-ui
echo '{}' >/storage/.config/pocket-ui/settings.json
boot
check "deletes itself" '[ ! -e /flash/mount-storage.sh ]'
check "copies nothing over existing settings" '[ "$(cat /storage/.config/pocket-ui/settings.json)" = "{}" ] && [ ! -e "/storage/$DROPIN" ]'

echo "== existing files in /storage/.config survive the move"
setup
mkdir -p /storage/.config/system.d
echo keep >/storage/.config/system.d/other.conf
boot
check "other file kept" '[ "$(cat /storage/.config/system.d/other.conf)" = keep ]'
check "drop-in added" '[ -f "/storage/$DROPIN" ]'

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
