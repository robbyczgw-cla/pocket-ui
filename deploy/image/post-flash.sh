# pocket-ui SD image: start without ROCKNIX's logo. On the FAT partition as /flash/post-flash.sh
# (scripts/make-image.sh).
#
# ROCKNIX's initramfs sources this file at every boot right after mounting /flash (init, mount_flash(): "if [ -f
# /flash/post-flash.sh ]"), before it calls load_splash. The image is based on ROCKNIX but is not ROCKNIX, so it
# must not greet you with ROCKNIX's branding: this load_splash is ROCKNIX's own (tag 20261001) without the call to
# /usr/bin/rocknix-splash, which draws the ROCKNIX logo. The version lines and "please wait" stay.
#
# Runs in the initramfs: BusyBox ash. Tests: deploy/image/test-hooks.sh

load_splash() {
  echo -en "\033[1000H\033[2Kpocket-ui, based on ROCKNIX\n$(cat /sysroot/etc/issue)\n\n" >/dev/console
  if [ -f "/storage/.configured" ]; then
    echo -ne "\033[1000H\033[2K==> Loading, please wait..." >/dev/console
  else
    echo -ne "\033[1000H\033[2K==> Initializing, please wait..." >/dev/console
  fi
}
