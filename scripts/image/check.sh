#!/bin/sh
# Offline checks of a pocket-ui SD image. Runs in the pocket-image-tools container (scripts/check-image.sh starts
# it), with the repository at /repo. Only reads the image: the FAT partition through mtools, STORAGE as a copy.
#   check.sh IMG VERSION WORKDIR
set -u
. /repo/scripts/image/pins.sh
. /repo/scripts/image/grub-checks.sh
IMG=$1
VERSION=$2
W=$3
FAT="$IMG@@$((P1_START * 512))"
fails=0
warns=0

check() {
  if eval "$2"; then
    echo "ok   $1"
  else
    echo "FAIL $1"
    fails=$((fails + 1))
    return 1
  fi
}
warn() {
  if eval "$2"; then
    echo "ok   $1"
  else
    echo "WARN $1"
    warns=$((warns + 1))
  fi
}
md5() { md5sum "$1" 2>/dev/null | cut -d' ' -f1; }
same() { cmp -s "$1" "$2"; }

rm -rf "$W/fat" "$W/releases" "$W/p2.ext4" "$W/lists"
mkdir -p "$W/fat" "$W/lists"

echo "== partition table"
sfdisk -d "$IMG" >"$W/lists/sfdisk" 2>/dev/null
part() { grep "^$IMG$1 " "$W/lists/sfdisk" | sed 's/^[^:]*: *//; s/, uuid=[^,]*//; s/, attrs="[^"]*"//'; }
check "GPT" 'grep -qx "label: gpt" "$W/lists/sfdisk"'
check "exactly two partitions" '[ "$(grep -c "^$IMG[0-9]" "$W/lists/sfdisk")" = 2 ]'
check "p1 system, FAT, as in ROCKNIX $ROCKNIX_TAG" \
  '[ "$(part 1)" = "start=       $P1_START, size=     $P1_SIZE, type=EBD0A0A2-B9E5-4433-87C0-68B6B72699C7, name=\"system\"" ]'
check "p2 storage, Linux, as in ROCKNIX $ROCKNIX_TAG" \
  '[ "$(part 2)" = "start=     $P2_START, size=       $P2_SIZE, type=0FC63DAF-8483-4772-8E79-3D69D8477DE4, name=\"storage\"" ]'
check "image size unchanged (2,198,863,872 bytes)" '[ "$(stat -c %s "$IMG")" = 2198863872 ]'

echo "== FAT partition (/flash)"
check "label ROCKNIX (boot=LABEL=ROCKNIX, GRUB search --label)" 'mlabel -i "$FAT" -s :: 2>/dev/null | grep -q "Volume label is ROCKNIX"'
# Bare recursive listing: one path per line, directories with a trailing slash.
mdir -/ -b -i "$FAT" ::/ 2>/dev/null | sed 's#^::/##' | sort >"$W/lists/fat"
cat >"$W/lists/fat.expected" <<EOF
EFI/
EFI/BOOT/
EFI/BOOT/BOOTAA64.EFI
KERNEL
KERNEL.md5
SYSTEM
SYSTEM.md5
boot/
boot/grub/
boot/grub/grub-efi-arm64-bin.version
boot/grub/grub.cfg
boot/pocket/
boot/pocket/Image
boot/pocket/KERNEL.source.md5
boot/pocket/miniv2-noufs.dtb
mount-storage.sh
pocket-ui-info/
pocket-ui-info/README.txt
pocket-ui-info/SOURCE-OFFER.txt
pocket-ui-info/pocket-ui-os.txt
post-flash.sh
EOF
# The firmware's terms, as in the repository.
echo "pocket-ui-info/firmware-licences/" >>"$W/lists/fat.expected"
for f in /repo/deploy/image/firmware-licences/*; do echo "pocket-ui-info/firmware-licences/${f##*/}" >>"$W/lists/fat.expected"; done
# pocket-ui's LICENSE and NOTICE.md belong on the card as soon as the repository has them.
for f in LICENSE NOTICE.md; do [ ! -f "/repo/$f" ] || echo "pocket-ui-info/$f" >>"$W/lists/fat.expected"; done
# The crates' licence texts and the fonts' OFL (scripts/licence-files.sh); mdir lists the directory too.
echo "pocket-ui-info/licences/" >>"$W/lists/fat.expected"
for f in LICENSE NOTICE.md FONTS-OFL.txt THIRD-PARTY-CRATES.md THIRD-PARTY-LICENSES.txt; do echo "pocket-ui-info/licences/$f" >>"$W/lists/fat.expected"; done
sort -o "$W/lists/fat.expected" "$W/lists/fat.expected"
# ROCKNIX's LICENSE.md and licenses folder come from the tag at build time; counted and spot-checked below.
grep -v '^pocket-ui-info/rocknix-licenses/' "$W/lists/fat" >"$W/lists/fat.own"
check "exactly the expected files (no rocknix_abl, nothing else)" 'same "$W/lists/fat.own" "$W/lists/fat.expected"' || diff "$W/lists/fat.expected" "$W/lists/fat.own" | sed 's/^/     /'
check "rocknix-licenses/: ROCKNIX's LICENSE.md and its licenses folder ($ROCKNIX_LICENSE_FILES files)" \
  '[ "$(grep -c "^pocket-ui-info/rocknix-licenses/.*[^/]$" "$W/lists/fat")" = "$ROCKNIX_LICENSE_FILES" ] && grep -qx "pocket-ui-info/rocknix-licenses/LICENSE.md" "$W/lists/fat" && grep -qx "pocket-ui-info/rocknix-licenses/licenses/GPL-2.0-only.txt" "$W/lists/fat"'
for f in KERNEL KERNEL.md5 SYSTEM.md5 EFI/BOOT/BOOTAA64.EFI boot/grub/grub.cfg boot/grub/grub-efi-arm64-bin.version \
  boot/pocket/Image boot/pocket/KERNEL.source.md5 boot/pocket/miniv2-noufs.dtb mount-storage.sh post-flash.sh \
  pocket-ui-info/README.txt pocket-ui-info/pocket-ui-os.txt; do
  mkdir -p "$W/fat/$(dirname "$f")"
  mcopy -n -i "$FAT" "::/$f" "$W/fat/$f" 2>/dev/null
done
check "KERNEL is ROCKNIX $ROCKNIX_TAG (md5 $KERNEL_MD5)" '[ "$(md5 "$W/fat/KERNEL")" = "$KERNEL_MD5" ]'
check "KERNEL.md5 agrees" '[ "$(cut -d" " -f1 "$W/fat/KERNEL.md5")" = "$KERNEL_MD5" ]'
mcopy -n -i "$FAT" ::/SYSTEM "$W/SYSTEM" 2>/dev/null
check "SYSTEM is ROCKNIX $ROCKNIX_TAG's without SYSTEM_REMOVE (md5 $POCKET_SYSTEM_MD5)" '[ "$(md5 "$W/SYSTEM")" = "$POCKET_SYSTEM_MD5" ]'
check "SYSTEM.md5 agrees, in ROCKNIX's format" '[ "$(mtype -i "$FAT" ::/SYSTEM.md5)" = "$POCKET_SYSTEM_MD5  target/SYSTEM" ]'

check "FAT free space is all zeros (no remnant of ROCKNIX's original SYSTEM)" \
  'python3 -I /repo/scripts/image/fat-free-zero.py "$IMG" "$((P1_START * 512))" --check >/dev/null'
dd if="$IMG" of="$W/p1.fat" bs=512 skip="$P1_START" count="$P1_SIZE" status=none
check "FAT file system clean (fsck.vfat -n)" 'fsck.vfat -n "$W/p1.fat" >"$W/lists/fsck.vfat" 2>&1'
rm -f "$W/p1.fat"

echo "== inside SYSTEM"
unsquashfs -lln "$W/SYSTEM" 2>/dev/null | sed -n 's#^.* squashfs-root/##p' >"$W/lists/system"
check "SYSTEM lists its files ($(wc -l <"$W/lists/system") entries)" '[ "$(wc -l <"$W/lists/system")" -gt 60000 ]'
for p in $SYSTEM_REMOVE; do
  check "not in SYSTEM: $p" '! grep -q "^$p\\( \\|/\\|$\\)" "$W/lists/system"'
done
syscat() { unsquashfs -cat "$W/SYSTEM" "$1" 2>/dev/null; }
echo "$SYSTEM_ZIP_REMOVE" | while read -r z glob; do
  [ -n "$z" ] || continue
  syscat "$z" >"$W/lists/zip"
  check "$z: intact, without $glob" \
    'unzip -tq "$W/lists/zip" >/dev/null && [ -z "$(unzip -Z1 "$W/lists/zip" | grep "^$(echo "$glob" | sed "s/\\*/[^\\/]*/g")\$")" ]' || echo "FAILED-ZIP" >>"$W/lists/zipfail"
done
check "every SYSTEM_ZIP_REMOVE archive checked out" '[ ! -e "$W/lists/zipfail" ]'
rm -f "$W/lists/zip" "$W/lists/zipfail"
# Remote access stays off until the player turns it on in pocket-ui (docs/SECURITY-HARDENING.md).
for k in ssh.enabled samba.enabled syncthing.enabled; do
  check "ROCKNIX's default $k=0 (nothing listens until it is turned on)" 'syscat usr/config/system/configs/system.cfg | grep -qx "$k=0"'
done
check "no SSH keys or host keys in SYSTEM (usr/config/ssh/authorized_keys, ssh_host_*)" \
  '! grep -Eq "(^usr/config/ssh/authorized_keys|ssh_host_.*key)" "$W/lists/system"'
check "the kernel command line does not turn SSH on (no ssh word)" '! grep -Eq "^ *linux .* ssh( |$)" "$W/fat/boot/grub/grub.cfg"'
check "the only BIOS file in SYSTEM is ReGBA's open-source GBA BIOS (GPL, md5 $REGBA_BIOS_MD5)" \
  '[ "$(grep -i "bios.*\\.bin$" "$W/lists/system")" = usr/config/nanoboyadvance/bios/gba_bios.bin ] && [ "$(syscat usr/config/nanoboyadvance/bios/gba_bios.bin | md5sum | cut -d" " -f1)" = "$REGBA_BIOS_MD5" ]'
FW=usr/lib/kernel-overlays/base/lib/firmware
unsquashfs -lln "$W/SYSTEM" "$FW" 2>/dev/null | grep '^-' | sed -n "s#^.* squashfs-root/$FW/##p" | sort >"$W/lists/firmware"
check "every firmware file in SYSTEM ($(wc -l <"$W/lists/firmware")) is listed with its terms in firmware-licences/README.txt" \
  '( while read -r f; do grep -q " $f " /repo/deploy/image/firmware-licences/README.txt || { echo "     not listed: $f"; exit 1; }; done <"$W/lists/firmware" )'
rm -f "$W/SYSTEM"

echo "== boot files"
check "grub.cfg is deploy/sd-efi/grub.cfg" 'same "$W/fat/boot/grub/grub.cfg" /repo/deploy/sd-efi/grub.cfg'
G=$W/fat/boot/grub/grub.cfg
check "grub.cfg finds the FAT partition by label ROCKNIX" 'grep -Eq "^ *search .*--label ROCKNIX$" "$G"'
check "grub.cfg loads /boot/pocket/Image" 'grep -Eq "^ *linux /boot/pocket/Image " "$G"'
check "grub.cfg loads /boot/pocket/miniv2-noufs.dtb" 'grep -Eq "^ *devicetree /boot/pocket/miniv2-noufs.dtb$" "$G"'
check "kernel command line: boot=LABEL=ROCKNIX disk=LABEL=STORAGE" 'grep -Eq "^ *linux .* boot=LABEL=ROCKNIX disk=LABEL=STORAGE " "$G"'
# The quiet start (docs/BOOT-SPLASH.md): no kernel or GRUB text on the screen, and fbcon left as the kernel sets it.
check "kernel command line: quiet loglevel=3 vt.global_cursor_default=0" 'grep -Eq "^ *linux .* rootwait quiet loglevel=3 vt.global_cursor_default=0 " "$G"'
check "grub.cfg prints nothing (no echo) and leaves fbcon alone (no fbcon=; comments may name it)" 'grub_cfg_quiet_clean "$G"'
check "boot-quiet.sh (deploy/grub-quiet.sh) finds this grub.cfg already quiet and would change nothing" \
  'grub_cfg_quiet_for_boot_quiet "$G" /repo/deploy/grub-quiet.sh'
for p in $(awk '$1 == "linux" || $1 == "devicetree" || $1 == "initrd" { print $2 }' "$G"); do
  check "file named in grub.cfg exists: $p" '[ -s "$W/fat$p" ]'
done
check "GRUB is an arm64 EFI application" 'file -b "$W/fat/EFI/BOOT/BOOTAA64.EFI" | grep -q "EFI application.*Aarch64"'
warn "GRUB is byte for byte the one on the author's device" '[ "$(md5 "$W/fat/EFI/BOOT/BOOTAA64.EFI")" = "$GRUB_EFI_MD5_DEVICE" ]'
check "GRUB version recorded: $(cat "$W/fat/boot/grub/grub-efi-arm64-bin.version" 2>/dev/null)" 'grep -q "^grub-efi-arm64-bin " "$W/fat/boot/grub/grub-efi-arm64-bin.version"'
check "Image is the one on the author's device (md5 $IMAGE_MD5)" '[ "$(md5 "$W/fat/boot/pocket/Image")" = "$IMAGE_MD5" ]'
python3 -I - "$W/fat/KERNEL" "$W/kernel-image" <<'EOF'
import struct, sys, zlib
d = open(sys.argv[1], 'rb').read()
size, page = struct.unpack('<I', d[8:12])[0], struct.unpack('<I', d[36:40])[0]
payload = d[page:page + size]
open(sys.argv[2], 'wb').write(zlib.decompressobj(31).decompress(payload) if payload[:2] == b'\x1f\x8b' else payload)
EOF
check "Image is unpacked from this KERNEL" 'same "$W/kernel-image" "$W/fat/boot/pocket/Image"'
rm -f "$W/kernel-image"
D=$W/fat/boot/pocket/miniv2-noufs.dtb
check "dtb is the one on the author's device (md5 $DTB_MD5)" '[ "$(md5 "$D")" = "$DTB_MD5" ]'
check "dtb model is Retroid Pocket Mini V2" '[ "$(fdtget -t s "$D" / model)" = "Retroid Pocket Mini V2" ]'
check "dtb: internal UFS disabled (Android stays invisible)" \
  '[ "$(fdtget -t s "$D" /soc@0/ufshc@1d84000 status)" = disabled ] && [ "$(fdtget -t s "$D" /soc@0/phy@1d87000 status)" = disabled ]'
check "KERNEL.source.md5 names this KERNEL" '[ "$(cat "$W/fat/boot/pocket/KERNEL.source.md5")" = "$KERNEL_MD5" ]'
check "initramfs hook mount-storage.sh is deploy/image/mount-storage.sh" 'same "$W/fat/mount-storage.sh" /repo/deploy/image/mount-storage.sh'
check "initramfs hook post-flash.sh is deploy/image/post-flash.sh" 'same "$W/fat/post-flash.sh" /repo/deploy/image/post-flash.sh'
check "pocket-ui-info/pocket-ui-os.txt: $POCKET_OS_NAME $VERSION, for Settings > About" \
  '[ "$(cat "$W/fat/pocket-ui-info/pocket-ui-os.txt")" = "$(printf "NAME=%s\nVERSION=%s" "$POCKET_OS_NAME" "$VERSION")" ]'

echo "== notes on the card (pocket-ui-info/README.txt)"
R=$W/fat/pocket-ui-info/README.txt
check "names version $VERSION" 'grep -q "pocket-ui $VERSION" "$R"'
check "$POCKET_OS_NAME, built on ROCKNIX, not made or endorsed by its team, with link" 'grep -q "$POCKET_OS_NAME $VERSION" "$R" && grep -q "built on ROCKNIX" "$R" && grep -q "not made or" "$R" && grep -q "https://github.com/ROCKNIX/distribution" "$R"'
check "source offer: ROCKNIX tag $ROCKNIX_TAG and commit" 'grep -q "tag $ROCKNIX_TAG" "$R" && grep -q "$ROCKNIX_COMMIT" "$R"'
check "artwork under CC BY-NC-SA 4.0" 'grep -q "CC BY-NC-SA 4.0" "$R"'
check "free, non-commercial distribution only" 'grep -qi "non-commercial" "$R"'
check "says: no BIOS files, no games" 'grep -q "no BIOS files" "$R"'
check "says what was removed from SYSTEM and why (SKick, extra firmware)" 'grep -q "skick346.lha" "$R" && grep -q "AYN Thor" "$R"'
check "names the GBA BIOS as ReGBA's open-source replacement" 'grep -q "Nebuleon/ReGBA" "$R"'
check "says remote access is off, and that pocket-ui makes the password" 'grep -q "Remote access (SSH), file sharing and Syncthing are off" "$R"'
check "points to the source next to the image and to SOURCE-OFFER.txt" 'grep -q "pocket-ui-info/SOURCE-OFFER.txt" "$R"'
warn "names a clean pocket-ui source (no -dirty commit)" '! grep -q -- "-dirty" "$R"'
mcopy -n -i "$FAT" ::/pocket-ui-info/SOURCE-OFFER.txt "$W/fat/pocket-ui-info/SOURCE-OFFER.txt" 2>/dev/null
O=$W/fat/pocket-ui-info/SOURCE-OFFER.txt
check "SOURCE-OFFER.txt: written offer for three years, how to ask, ROCKNIX tag and commit" \
  'grep -q "^Written offer" "$O" && grep -q "at least three years" "$O" && grep -q "https://github.com/.*/issues" "$O" && grep -q "$ROCKNIX_COMMIT" "$O" && grep -q "pocket-ui $VERSION" "$O"'
mkdir -p "$W/fat/pocket-ui-info/firmware-licences"
for f in /repo/deploy/image/firmware-licences/*; do
  mcopy -n -i "$FAT" "::/pocket-ui-info/firmware-licences/${f##*/}" "$W/fat/pocket-ui-info/firmware-licences/${f##*/}" 2>/dev/null
done
check "firmware-licences/ is deploy/image/firmware-licences/" 'diff -r -q "$W/fat/pocket-ui-info/firmware-licences" /repo/deploy/image/firmware-licences >/dev/null'
check "pocket-ui licensed GPL-2.0-or-later, credits in NOTICE.md" \
  'grep -q "GNU GPL version 2 or later (pocket-ui-info/LICENSE)" "$R" && grep -q "pocket-ui-info/NOTICE.md" "$R"'
for f in LICENSE NOTICE.md; do
  if [ -f "/repo/$f" ]; then
    mcopy -n -i "$FAT" "::/pocket-ui-info/$f" "$W/fat/pocket-ui-info/$f" 2>/dev/null
    check "pocket-ui-info/$f is the repository's" 'same "$W/fat/pocket-ui-info/$f" "/repo/$f"'
  else
    warn "pocket-ui-info/$f: the repository has no $f yet (the image is not releasable without it)" false
  fi
done

for f in FONTS-OFL.txt THIRD-PARTY-CRATES.md THIRD-PARTY-LICENSES.txt; do
  mkdir -p "$W/fat/pocket-ui-info/licences"
  mcopy -n -i "$FAT" "::/pocket-ui-info/licences/$f" "$W/fat/pocket-ui-info/licences/$f" 2>/dev/null
  check "pocket-ui-info/licences/$f is not empty" '[ -s "$W/fat/pocket-ui-info/licences/$f" ]'
done
check "pocket-ui-info/licences/FONTS-OFL.txt is assets/fonts/OFL.txt" 'same "$W/fat/pocket-ui-info/licences/FONTS-OFL.txt" /repo/assets/fonts/OFL.txt'

echo "== STORAGE partition"
dd if="$IMG" of="$W/p2.ext4" bs=512 skip="$P2_START" count="$P2_SIZE" status=none
check "label STORAGE (disk=LABEL=STORAGE)" '[ "$(e2label "$W/p2.ext4")" = STORAGE ]'
check "file system clean (e2fsck -fn)" 'e2fsck -fn "$W/p2.ext4" >"$W/lists/e2fsck" 2>&1'
# Dumped inside the container, not to the bind mount, so owners and modes are the ones in the image.
S=/tmp/storage
rm -rf "$S"
mkdir -p "$S"
debugfs -R "rdump / $S" "$W/p2.ext4" >/dev/null 2>&1
(cd "$S" && find . -printf '%m %U:%G %y %p %l\n' | sed 's/ $//' | sort) >"$W/lists/storage"
V=$VERSION
sort >"$W/lists/storage.expected" <<EOF
755 0:0 d .
700 0:0 d ./lost+found
644 0:0 f ./.please_resize_me
755 0:0 d ./.pocket-seed
755 0:0 d ./.pocket-seed/.config
755 0:0 d ./.pocket-seed/.config/autostart
755 0:0 f ./.pocket-seed/.config/autostart/pocket-ui-image-defaults
755 0:0 d ./.pocket-seed/.config/system.d
755 0:0 d ./.pocket-seed/.config/system.d/essway.service.d
644 0:0 f ./.pocket-seed/.config/system.d/essway.service.d/10-pocket-ui.conf
644 0:0 f ./.pocket-seed/.config/system.d/pocket-splash.service
755 0:0 d ./.pocket-seed/.config/system.d/sysinit.target.wants
777 0:0 l ./.pocket-seed/.config/system.d/sysinit.target.wants/pocket-splash.service ../pocket-splash.service
755 0:0 d ./pocket-ui
755 0:0 f ./pocket-ui/run-pocket-ui.sh
777 0:0 l ./pocket-ui/current releases/$V
777 0:0 l ./pocket-ui/pocket-ui current/pocket-ui
777 0:0 l ./pocket-ui/pocket-core current/pocket-core
777 0:0 l ./pocket-ui/portmaster-setup.sh current/portmaster-setup.sh
755 0:0 d ./pocket-ui/licences
644 0:0 f ./pocket-ui/licences/FONTS-OFL.txt
644 0:0 f ./pocket-ui/licences/LICENSE
644 0:0 f ./pocket-ui/licences/NOTICE.md
644 0:0 f ./pocket-ui/licences/THIRD-PARTY-CRATES.md
644 0:0 f ./pocket-ui/licences/THIRD-PARTY-LICENSES.txt
755 0:0 d ./pocket-ui/releases
755 0:0 d ./pocket-ui/releases/$V
755 0:0 f ./pocket-ui/releases/$V/pocket-ui
755 0:0 f ./pocket-ui/releases/$V/pocket-core
755 0:0 f ./pocket-ui/releases/$V/pocket-splash
755 0:0 f ./pocket-ui/releases/$V/portmaster-setup.sh
EOF
check "exactly the expected files, owners and modes" 'same "$W/lists/storage" "$W/lists/storage.expected"' ||
  diff "$W/lists/storage.expected" "$W/lists/storage" | sed 's/^/     /'
check ".please_resize_me there: ROCKNIX grows STORAGE at the first boot" '[ -f "$S/.please_resize_me" ]'
check "no /storage/.config or .cache yet (fs-resize would refuse to grow)" '[ ! -e "$S/.config" ] && [ ! -e "$S/.cache" ]'
check "drop-in waiting in .pocket-seed is deploy/essway-pocket-ui.conf" \
  'same "$S/.pocket-seed/.config/system.d/essway.service.d/10-pocket-ui.conf" /repo/deploy/essway-pocket-ui.conf'
check "boot screen unit waiting in .pocket-seed is deploy/pocket-splash.service" \
  'same "$S/.pocket-seed/.config/system.d/pocket-splash.service" /repo/deploy/pocket-splash.service'
check "first-start settings waiting in .pocket-seed are deploy/image/image-defaults.sh" \
  'same "$S/.pocket-seed/.config/autostart/pocket-ui-image-defaults" /repo/deploy/image/image-defaults.sh'
check "licence texts in STORAGE: the repository's LICENSE and NOTICE.md, the fonts' OFL, the crates' texts (not empty)" \
  'same "$S/pocket-ui/licences/LICENSE" /repo/LICENSE && same "$S/pocket-ui/licences/NOTICE.md" /repo/NOTICE.md && same "$S/pocket-ui/licences/FONTS-OFL.txt" /repo/assets/fonts/OFL.txt && [ -s "$S/pocket-ui/licences/THIRD-PARTY-CRATES.md" ] && grep -q "^adler2 " "$S/pocket-ui/licences/THIRD-PARTY-LICENSES.txt"'
check "launcher is deploy/run-pocket-ui.sh" 'same "$S/pocket-ui/run-pocket-ui.sh" /repo/deploy/run-pocket-ui.sh'
check "portmaster-setup.sh is deploy/portmaster-setup.sh" 'same "$S/pocket-ui/releases/$V/portmaster-setup.sh" /repo/deploy/portmaster-setup.sh'
for b in pocket-ui pocket-core pocket-splash; do
  check "$b is an aarch64 Linux program" 'file -b "$S/pocket-ui/releases/$V/$b" | grep -q "ELF 64-bit LSB.*ARM aarch64"'
done
echo "-- first start, nothing from a developer device:"
check "no pocket-ui settings (first-start setup runs)" '[ -z "$(find "$S" -path "*/.config/pocket-ui*")" ]'
check "no dev-ssh marker, no SSH keys" '[ -z "$(find "$S" -name dev-ssh -o -name .ssh -o -name "*authorized_keys*" -o -name "ssh_host_*")" ]'
check "no Wi-Fi credentials (iwd, NetworkManager, wpa_supplicant)" \
  '[ -z "$(find "$S" -name "*.psk" -o -name "*.8021x" -o -name "*.nmconnection" -o -name "wpa_supplicant*" -o -path "*/iwd*")" ]'
check "no recents, saves or games" '[ -z "$(find "$S" -name "recents*" -o -name roms -o -name "*.srm" -o -name "*.state*")" ]'
check "no BIOS files" '[ -z "$(find "$S" -iname "*bios*")" ] && ! grep -qi bios "$W/lists/fat"'
check "no update left pending (update-pending, previous, dev.env)" \
  '[ ! -e "$S/pocket-ui/update-pending" ] && [ ! -L "$S/pocket-ui/previous" ] && [ ! -e "$S/pocket-ui/dev.env" ]'
echo "STORAGE before the first boot: $(du -sh "$S" | cut -f1) used of 32 MB"
# The programs grow with every release; STORAGE stays 32 MB until the first boot grows it (debugfs fails when full).
warn "STORAGE keeps a quarter free for the programs to grow (under 24 MB used)" '[ "$(du -sk "$S" | cut -f1)" -lt 24576 ]'
# For the --version check outside (scripts/check-image.sh).
[ -d "$S/pocket-ui/releases" ] && cp -r "$S/pocket-ui/releases" "$W/releases"

# The work files back to the user who started the check (CHECK_OWNER, uid:gid): root's directories in a bind mount
# could not be removed without root on a Linux machine.
[ -z "${CHECK_OWNER:-}" ] || chown -R "$CHECK_OWNER" "$W" 2>/dev/null || true

echo
if [ $fails -eq 0 ]; then
  echo "all checks passed ($warns warnings)"
else
  echo "$fails checks failed ($warns warnings)"
  exit 1
fi
