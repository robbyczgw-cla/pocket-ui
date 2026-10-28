#!/bin/sh
# Tests of deploy/grub-quiet.sh against a fake /flash (no device, no root rights): sh deploy/test-grub-quiet.sh
# The fixture is the grub.cfg the SD boot files had before the quiet start (deploy/sd-efi/grub.cfg until 2026-10-26),
# as it is on cards set up by hand. Each case runs under every shell found of dash, bash (ROCKNIX's sh) and busybox sh.
set -u
HERE=$(cd "$(dirname "$0")" && pwd)
SCRIPT=$HERE/grub-quiet.sh
T=$(mktemp -d)
trap 'rm -rf "$T"' EXIT
fails=0

check() {
  if eval "$2"; then
    echo "ok   $1"
  else
    echo "FAIL $1"
    fails=$((fails + 1))
  fi
}

G=$T/flash/boot/grub
CFG=$G/grub.cfg
BAK=$CFG.before-quiet
fixture() {
  rm -rf "$T/flash"
  mkdir -p "$G"
  cat >"$CFG" <<'EOF'
# pocket-ui on the Retroid Pocket Mini V2: Boot mode: Loader -> Retroid u-boot (loader) -> this GRUB -> Linux.
set timeout=0
set default=0

menuentry "pocket-ui" {
  search --no-floppy --set=root --label ROCKNIX
  echo "pocket-ui: loading..."
  devicetree /boot/pocket/miniv2-noufs.dtb
  linux /boot/pocket/Image boot=LABEL=ROCKNIX disk=LABEL=STORAGE rootwait console=tty0 video=efifb:off gpt
}

# Only built-in modules exist (no /boot/grub/arm64-efi), so this uses `reboot`, not `halt`.
menuentry "Restart" {
  reboot
}
EOF
  cp "$CFG" "$T/original"
}
run() { (cd "$T" && POCKETUI_FLASH=$T/flash $SH "$SCRIPT" "$@" >"$T/out" 2>&1); }
said() { grep -q -- "$1" "$T/out"; }
kernel_line() { grep '^ *linux /boot/pocket/Image ' "$1"; }
# Every file under the fake /flash with its checksum: "nothing changed" compares two of these.
tree() { (cd "$T/flash" && find . -type f -exec cksum {} + | sort); }
WANT_LINE=$(kernel_line "$HERE/sd-efi/grub.cfg")

SHELLS=
for s in dash bash "busybox sh"; do
  command -v ${s%% *} >/dev/null && SHELLS="$SHELLS${SHELLS:+:}$s"
done
IFS_OLD=$IFS
IFS=:
for SH in $SHELLS; do
  IFS=$IFS_OLD
  echo "== $SH"

  fixture
  run status
  check "$SH: status of a card set up before: off, no backup" '[ $? = 0 ] && said "quiet start: off" && said "backup: none"'
  check "$SH: status changes nothing" 'cmp -s "$CFG" "$T/original" && [ ! -e "$BAK" ]'

  run apply
  check "$SH: apply (exit 0)" '[ $? = 0 ] && said "quiet start: on from the next start"'
  check "$SH: apply: the kernel line is the SD image's (deploy/sd-efi/grub.cfg)" '[ "$(kernel_line "$CFG")" = "$WANT_LINE" ]' ||
    { echo "     got:  $(kernel_line "$CFG")"; echo "     want: $WANT_LINE"; }
  check "$SH: apply: GRUB's echo is gone" '! grep -q "echo" "$CFG"'
  check "$SH: apply: every other line is as it was" \
    '[ "$(grep -v -e "^ *linux " -e "^ *echo " "$T/original")" = "$(grep -v "^ *linux " "$CFG")" ]'
  check "$SH: apply: the old file is kept next to it, byte for byte" 'cmp -s "$BAK" "$T/original"'
  check "$SH: apply: no fbcon= words, nothing left behind" '! grep -q "fbcon=" "$CFG" && [ ! -e "$CFG.new" ] && [ "$(ls "$G" | wc -l)" = 2 ]'
  cp "$CFG" "$T/quiet"
  run status
  check "$SH: status after apply: on, backup named" 'said "quiet start: on" && said "backup: .*before-quiet"'

  before=$(tree)
  run apply
  check "$SH: apply again: exit 0, says so" '[ $? = 0 ] && said "already on; nothing changed"'
  check "$SH: apply again: nothing changed (idempotent), the backup is still the original" '[ "$(tree)" = "$before" ] && cmp -s "$BAK" "$T/original"'

  run undo
  check "$SH: undo (exit 0)" '[ $? = 0 ] && said "quiet start: off from the next start"'
  check "$SH: undo: the original is back byte for byte, the backup is gone" 'cmp -s "$CFG" "$T/original" && [ ! -e "$BAK" ] && [ ! -e "$CFG.new" ]'
  run undo
  check "$SH: undo without a backup: refused (exit 1), file unchanged" '[ $? = 1 ] && said "no backup" && cmp -s "$CFG" "$T/original"'
  run apply
  check "$SH: apply after undo: quiet again, a new backup" '[ $? = 0 ] && cmp -s "$CFG" "$T/quiet" && cmp -s "$BAK" "$T/original"'

  # Other words on the kernel line stay where they are; one already there is set, never doubled.
  fixture
  sed -i 's#^  linux .*#  linux /boot/pocket/Image boot=LABEL=ROCKNIX disk=LABEL=STORAGE rootwait loglevel=7 console=tty0 video=efifb:off gpt nodtbo foo=bar vt.global_cursor_default=1 loglevel=8#' "$CFG"
  run apply
  check "$SH: other words kept, existing ones set once" \
    '[ "$(kernel_line "$CFG")" = "  linux /boot/pocket/Image boot=LABEL=ROCKNIX disk=LABEL=STORAGE rootwait quiet loglevel=3 vt.global_cursor_default=0 console=tty0 video=efifb:off gpt nodtbo foo=bar" ]' ||
    echo "     got: $(kernel_line "$CFG")"
  fixture
  sed -i -e 's# rootwait##' -e '/echo/d' "$CFG"
  cp "$CFG" "$T/original"
  run apply
  check "$SH: no rootwait and no echo: the words go at the end" \
    '[ $? = 0 ] && [ "$(kernel_line "$CFG")" = "  linux /boot/pocket/Image boot=LABEL=ROCKNIX disk=LABEL=STORAGE console=tty0 video=efifb:off gpt quiet loglevel=3 vt.global_cursor_default=0" ] && cmp -s "$BAK" "$T/original"' ||
    echo "     got: $(kernel_line "$CFG")"
  fixture
  sed -i 's/rootwait /rootwait quiet /' "$CFG"
  run apply
  check "$SH: quiet there but the rest not: completed, not doubled" '[ $? = 0 ] && [ "$(kernel_line "$CFG" | grep -o quiet | wc -l)" = 1 ] && [ "$(kernel_line "$CFG")" = "$WANT_LINE" ]'

  # Files that are not pocket-ui's GRUB config: refused, nothing written.
  refused() {
    name=$1 text=$2
    before=$(tree)
    run apply
    rc=$?
    check "$SH: refused, $name (exit 1, says why, nothing changed)" '[ $rc = 1 ] && said "STOPPED" && said "$text" && [ "$(tree)" = "$before" ]' ||
      sed 's/^/     /' "$T/out"
  }
  fixture
  cat >"$CFG" <<'EOF'
set timeout=3
menuentry "ROCKNIX" {
  linux /KERNEL boot=LABEL=ROCKNIX disk=LABEL=STORAGE quiet
}
EOF
  refused "another system's grub.cfg" 'no menuentry "pocket-ui"'
  fixture
  sed -i 's#/boot/pocket/Image#/boot/other/Image#' "$CFG"
  refused "no kernel line for /boot/pocket/Image" "0 kernel lines"
  fixture
  sed -i 's#^  linux \(.*\)#  linux \1\n  linux \1#' "$CFG"
  refused "two kernel lines" "2 kernel lines"
  fixture
  sed -i 's# disk=LABEL=STORAGE##' "$CFG"
  refused "kernel line without disk=LABEL=STORAGE" "boot=LABEL=ROCKNIX disk=LABEL=STORAGE"
  fixture
  : >"$CFG"
  refused "an empty file" "missing or empty"
  fixture
  rm -f "$CFG"
  refused "no grub.cfg" "missing or empty"
  fixture
  run apply
  printf 'set timeout=0\n' >"$BAK"
  before=$(tree)
  run undo
  check "$SH: undo refuses a backup that is not pocket-ui's (nothing changed)" '[ $? = 1 ] && said "backup is not" && [ "$(tree)" = "$before" ]'
  run bogus
  check "$SH: an unknown action prints the usage (exit 2)" '[ $? = 2 ] && said "sh grub-quiet.sh apply"'
  IFS=:
done
IFS=$IFS_OLD

if [ $fails = 0 ]; then echo "all grub-quiet tests passed ($SHELLS)"; else echo "$fails grub-quiet tests failed"; exit 1; fi
