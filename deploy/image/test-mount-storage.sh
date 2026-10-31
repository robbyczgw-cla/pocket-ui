#!/bin/sh
# Tests of the SD image's initramfs hook deploy/image/mount-storage.sh: the first-start seed and the guard against
# system updates pocket-ui did not stage (docs/SD-IMAGE.md "Updates on the card").
#   sh deploy/image/test-mount-storage.sh
# The hook runs under BusyBox sh with a PATH that holds ONLY applets ROCKNIX's initramfs has (busybox-init.conf of
# 20261001: no date, no test, no printf applet; ash has them as builtins), so a tool missing there fails here too.
# mount_part, mount and progress are fakes that log their calls, as ROCKNIX's init would provide them; PU_STORAGE points
# the hook at a fake /storage. About 1 s, part of the gate.
set -u
HERE=$(cd "$(dirname "$0")" && pwd)
command -v busybox >/dev/null || { echo "busybox is needed for this test"; exit 1; }
BB=$(command -v busybox)
T=$(mktemp -d)
trap 'rm -rf "$T"' EXIT
S=$T/storage
FL=$T/flash
fails=0
check() {
  if eval "$2"; then echo "ok   $1"; else echo "FAIL $1"; fails=$((fails + 1)); fi
}

mkdir "$T/applets"
for a in basename cat cp cut grep head ls mkdir mv rm sha256sum stat sync tr; do ln -s "$BB" "$T/applets/$a"; done

DROPIN=.config/system.d/essway.service.d/10-pocket-ui.conf

# A fresh card as the image leaves it. $1 = resize: STORAGE not grown yet (first boot).
setup() {
  rm -rf "$S" "$FL" "$T/calls"
  mkdir -p "$S/pocket-ui" "$S/.pocket-seed/$(dirname $DROPIN)" "$FL"
  echo '[Service]' >"$S/.pocket-seed/$DROPIN"
  chmod 644 "$S/.pocket-seed/$DROPIN"
  cp "$HERE/mount-storage.sh" "$FL/"
  [ "${1:-}" = resize ] && : >"$S/.please_resize_me"
  : >"$T/calls"
}

# Sources mount-storage.sh as ROCKNIX's init does (inside mount_storage, $disk set), with logging fakes.
boot() {
  env -i PATH="$T/applets" PU_STORAGE="$S" T="$T" FL="$FL" "$BB" sh -c '
    mount_part() { echo "mount_part $*" >>$T/calls; }
    mount() { echo "mount $*" >>$T/calls; }
    progress() { echo "progress $*" >>$T/calls; }
    disk=LABEL=STORAGE
    check_update() { echo "check_update" >>$T/calls; }
    mount_storage() { if [ -f $FL/mount-storage.sh ]; then . $FL/mount-storage.sh; else mount_part "$disk" /storage rw,noatime; fi; }
    # init runs its boot steps by name, mount_storage before check_update.
    for BOOT_STEP in mount_storage check_update; do ${BOOT_STEP}; done
  '
}

echo "== first boot: STORAGE still 32 MB, ROCKNIX will grow it"
setup resize
boot
check "mounts STORAGE as ROCKNIX does" 'grep -qx "mount_part LABEL=STORAGE /storage rw,noatime" $T/calls'
check "no .config yet (fs-resize would refuse)" '[ ! -e $S/.config ]'
check "seed stays" '[ -f "$S/.pocket-seed/$DROPIN" ]'
check "hook stays on /flash" '[ -f $FL/mount-storage.sh ]'
check "/flash not touched" '! grep -q "^mount " $T/calls'

echo "== second boot: STORAGE grown"
rm -f $S/.please_resize_me
: >$T/calls
boot
check "mounts STORAGE" 'grep -qx "mount_part LABEL=STORAGE /storage rw,noatime" $T/calls'
check "drop-in in place" '[ "$(cat "$S/$DROPIN" 2>/dev/null)" = "[Service]" ]'
check "drop-in mode 644" '[ "$(stat -c %a "$S/$DROPIN" 2>/dev/null)" = 644 ]'
check "seed gone" '[ ! -e $S/.pocket-seed ]'
check "pocket-ui untouched" '[ -d $S/pocket-ui ]'
check "hook stays: it guards every start" '[ -f $FL/mount-storage.sh ]'
check "/flash never written" '! grep -q "^mount " $T/calls'

echo "== later boots: same mount, nothing else"
: >$T/calls
boot
check "plain mount, ROCKNIX's update check as usual" '[ "$(cat $T/calls)" = "$(printf "mount_part LABEL=STORAGE /storage rw,noatime\ncheck_update")" ]'
check "no .update-refused without updates" '[ ! -e $S/.update-refused ]'

echo "== existing settings are never overwritten by a seed left over"
setup
rm -rf $S/.pocket-seed
mkdir -p $S/.config/pocket-ui
echo '{}' >$S/.config/pocket-ui/settings.json
boot
check "copies nothing over existing settings" '[ "$(cat $S/.config/pocket-ui/settings.json)" = "{}" ] && [ ! -e "$S/$DROPIN" ]'

echo "== existing files in .config survive the move"
setup
mkdir -p $S/.config/system.d
echo keep >$S/.config/system.d/other.conf
boot
check "other file kept" '[ "$(cat $S/.config/system.d/other.conf)" = keep ]'
check "drop-in added" '[ -f "$S/$DROPIN" ]'

# The update guard. ROCKNIX's check_update runs after this hook and installs *.tar, *.img, *.img.gz or KERNEL+SYSTEM
# from $S/.update. TAR is a ROCKNIX-named update as EmulationStation's updater or pocket-ui would put it there.
U=$S/.update
R=$S/.update-refused
TAR=ROCKNIX-SM8250.aarch64-20261201.tar
# A grown card after its first starts, with an update tar ($1 = its contents).
updated() {
  setup
  rm -rf $S/.pocket-seed
  mkdir -p $U $S/roms
  echo game >$S/roms/game.gb
  printf '%s' "$1" >"$U/$TAR"
}
# The approval src/update.rs stage_os writes: "<sha256>  <name>".
approve() { echo "$(sha256sum "$U/$TAR" | cut -d' ' -f1)  $TAR" >$U/.pocket-ui-approved; }
# What check_update would find (init: ls -1 *.tar / *.img.gz / *.img, or KERNEL+SYSTEM).
installable() { ls $U/*.tar $U/*.img $U/*.img.gz 2>/dev/null | grep -q . || { [ -f $U/KERNEL ] && [ -f $U/SYSTEM ]; }; }

echo "== guard: a tar pocket-ui staged and approved is left for ROCKNIX"
updated "our update"
approve
boot
check "tar stays" '[ "$(cat $U/$TAR)" = "our update" ]'
check "approval stays for this start" '[ -f $U/.pocket-ui-approved ]'
check "nothing refused" '[ ! -e $R ]'
check "says it checked" 'grep -q "^progress pocket-ui: checking the system update" $T/calls'

echo "== guard: a tar without approval (EmulationStation's updater) is moved, not installed"
updated "rocknix update"
boot
check "nothing left to install" '! installable'
check "moved to .update-refused, unchanged" '[ "$(cat $R/$TAR)" = "rocknix update" ]'
check "the reason is logged" 'grep -q "^$TAR -> $TAR: not staged by pocket-ui" $R/refused.log'
check "user files untouched" '[ "$(cat $S/roms/game.gb)" = game ]'
check "said on screen" 'grep -q "^progress pocket-ui: system update $TAR not installed" $T/calls'

echo "== guard: a tar dropped over the network share next to an approved one"
updated "our update"
approve
printf 'dropped' >"$U/ROCKNIX-SM8250.aarch64-20270101.tar"
boot
check "ours stays" '[ "$(cat $U/$TAR)" = "our update" ]'
check "the dropped one is moved" '[ ! -e $U/ROCKNIX-SM8250.aarch64-20270101.tar ] && [ "$(cat $R/ROCKNIX-SM8250.aarch64-20270101.tar)" = dropped ]'
check "logged as not approved" 'grep -q "^ROCKNIX-SM8250.aarch64-20270101.tar -> .*not the update pocket-ui staged" $R/refused.log'

echo "== guard: the approved name, but other contents (replaced over the share)"
updated "our update"
approve
printf 'replaced' >"$U/$TAR"
boot
check "nothing left to install" '! installable'
check "moved, unchanged" '[ "$(cat $R/$TAR)" = replaced ]'
check "logged as a checksum mismatch" 'grep -q "checksum is not the one pocket-ui checked" $R/refused.log'
check "the spent approval goes" '[ ! -e $U/.pocket-ui-approved ]'

echo "== guard: an empty or broken approval refuses"
updated "our update"
: >$U/.pocket-ui-approved
boot
check "empty approval: moved" '! installable && [ -f $R/$TAR ]'
updated "our update"
echo "garbage" >$U/.pocket-ui-approved
boot
check "garbage approval: moved" '! installable && [ -f $R/$TAR ]'
updated "our update"
printf '%s  %s' "$(sha256sum "$U/$TAR" | cut -d' ' -f1)" "$TAR" >$U/.pocket-ui-approved
boot
check "approval without a final newline still counts" '[ -f $U/$TAR ] && [ ! -e $R ]'

echo "== guard: disk images and a loose KERNEL + SYSTEM are always refused"
updated "x"
rm -f "$U/$TAR"
printf img >$U/ROCKNIX-SM8250.aarch64-20261201.img
printf gz >$U/ROCKNIX-SM8250.aarch64-20261201.img.gz
printf k >$U/KERNEL
printf s >$U/SYSTEM
printf m >$U/SYSTEM.md5
boot
check "nothing left to install" '! installable'
check "all five moved, unchanged" '[ "$(cat $R/ROCKNIX-SM8250.aarch64-20261201.img $R/ROCKNIX-SM8250.aarch64-20261201.img.gz $R/KERNEL $R/SYSTEM $R/SYSTEM.md5)" = imggzksm ]'

echo "== guard: a second refusal of the same name keeps the first"
updated "first"
boot
printf 'second' >"$U/$TAR"
boot
check "both kept" '[ "$(cat $R/$TAR)" = first ] && [ "$(cat $R/$TAR.1)" = second ]'

echo "== guard: other files in .update stay where they are"
updated "x"
echo note >$U/readme.txt
mkdir -p $U/.tmp
boot
check "readme and ROCKNIX's .tmp untouched" '[ "$(cat $U/readme.txt)" = note ] && [ -d $U/.tmp ]'

echo "== guard: a file that cannot be moved to .update-refused is renamed where it is"
updated "rocknix update"
: >$R
boot
check "renamed, so check_update no longer sees it" '! installable && [ "$(cat $U/$TAR.refused)" = "rocknix update" ]'

echo "== guard: the in-place fallback never overwrites an earlier refused file"
updated "first update"
: >$R
boot
printf 'second update' >"$U/$TAR"
boot
check "both kept" '[ "$(cat $U/$TAR.refused)" = "first update" ] && [ "$(cat $U/$TAR.refused.1)" = "second update" ]'
check "nothing left to install" '! installable'
check "ROCKNIX's update check still runs (nothing foreign left)" '[ "$(grep -c "^check_update$" $T/calls)" = 2 ]'

echo "== guard: nothing can be moved at all (STORAGE read-only or damaged)"
if [ "$(id -u)" = 0 ]; then
  echo "skip (root ignores the read-only folder this case uses)"
else
  updated "rocknix update"
  : >$R
  chmod 555 $U
  boot
  chmod 755 $U
  check "the foreign tar is still there, untouched" '[ "$(cat $U/$TAR)" = "rocknix update" ]'
  check "ROCKNIX's update check is skipped for this start" '! grep -q "^check_update$" $T/calls'
  check "said on screen, start goes on" 'grep -q "^progress pocket-ui: .*update check skipped" $T/calls'
  chmod 755 $U; rm -f $R
  boot
  check "next start with a writable STORAGE: moved, update check back" '! installable && [ -f $R/$TAR ] && grep -q "^check_update$" $T/calls'
  updated "our update"
  approve
  chmod 555 $U
  boot
  chmod 755 $U
  check "an approved tar on a read-only STORAGE still installs (check not skipped)" '[ -f $U/$TAR ] && grep -q "^check_update$" $T/calls'
fi

echo "== guard: no .update at all"
setup
rm -rf $S/.pocket-seed $U
boot
check "nothing created" '[ ! -e $U ] && [ ! -e $R ]'

echo
if [ $fails -eq 0 ]; then echo "all passed"; else echo "$fails failed"; exit 1; fi
