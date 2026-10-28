#!/bin/sh
# A quiet start on a card that boots through pocket-ui's GRUB (docs/BOOT-SPLASH.md, "Quiet start"): puts
# `quiet loglevel=3 vt.global_cursor_default=0` on the kernel line of /flash/boot/grub/grub.cfg and takes out GRUB's
# `echo "pocket-ui: loading..."`. The SD image has it already (deploy/sd-efi/grub.cfg); this is for a card set up
# before. Run it on the device as root (scripts/boot-quiet.sh does that from the computer):
#
#   sh grub-quiet.sh status   what the card has now; changes nothing
#   sh grub-quiet.sh apply    keeps the file as it is as grub.cfg.before-quiet next to it, then writes the quiet one
#   sh grub-quiet.sh undo     puts grub.cfg.before-quiet back and removes it
#
# Every other word and line stays as it is (a loglevel= or vt.global_cursor_default= already there is set to the
# value above, once). Running apply again changes nothing. It refuses a grub.cfg that is not pocket-ui's: no
# `menuentry "pocket-ui"`, or not exactly one `linux /boot/pocket/Image ... boot=LABEL=ROCKNIX disk=LABEL=STORAGE`
# line. Never adds fbcon=rotate or fbcon=map (sway's turn and touch read /sys/class/graphics/fbcon/rotate).
# ROCKNIX mounts /flash read-only: it is remounted read-write for the write and read-only again after it. It takes
# effect at the next start. If the card does not start any more: Boot mode: Android, put the card in a computer and
# copy boot/grub/grub.cfg.before-quiet over boot/grub/grub.cfg on its first partition (label ROCKNIX).
#
# Tests: deploy/test-grub-quiet.sh (POCKETUI_FLASH is a fake /flash for them; it is never remounted).
set -u
FLASH=${POCKETUI_FLASH:-/flash}
CFG=$FLASH/boot/grub/grub.cfg
BAK=$CFG.before-quiet
NEW=$CFG.new
WORDS="quiet loglevel=3 vt.global_cursor_default=0"
ECHO_LINE='^[[:space:]]*echo "pocket-ui: loading[.][.][.]"[[:space:]]*$'
LINUX_LINE='^[[:space:]]*linux[[:space:]][[:space:]]*/boot/pocket/Image[[:space:]]'

say() { printf '%s\n' "$@"; }
TMP=$(mktemp -d) || exit 1
trap 'rm -rf "$TMP"' EXIT
RO=0
die() {
  say "STOPPED: $*" >&2
  rm -f "$NEW" 2>/dev/null
  [ $RO = 0 ] || mount -o remount,ro /flash
  exit 1
}

# Whether $1 is pocket-ui's grub.cfg; says why not on stderr.
ours() {
  [ -s "$1" ] || { say "$1 is missing or empty" >&2; return 1; }
  grep -q '^menuentry "pocket-ui"' "$1" || { say "$1 has no menuentry \"pocket-ui\": not pocket-ui's GRUB config" >&2; return 1; }
  n=$(grep -c "$LINUX_LINE" "$1")
  [ "$n" = 1 ] || { say "$1 has $n kernel lines for /boot/pocket/Image, expected exactly one" >&2; return 1; }
  grep "$LINUX_LINE" "$1" | grep -q ' boot=LABEL=ROCKNIX disk=LABEL=STORAGE\( \|$\)' ||
    { say "$1: the kernel line has no 'boot=LABEL=ROCKNIX disk=LABEL=STORAGE'" >&2; return 1; }
}

# Whether $1 has all three words and no echo line.
is_quiet() {
  line=$(grep "$LINUX_LINE" "$1")
  for w in $WORDS; do
    case " $line " in *" $w "*) ;; *) return 1 ;; esac
  done
  ! grep -q "$ECHO_LINE" "$1"
}

# $1 with the words on its kernel line (right after rootwait, or at the end; one of them already there moves to that
# place with its value set) and without the echo line, on stdout.
quieten() {
  awk -v words="$WORDS" -v linux_re="$LINUX_LINE" -v echo_re="$ECHO_LINE" '
    $0 ~ echo_re { next }
    $0 ~ linux_re {
      indent = $0; sub(/[^ \t].*$/, "", indent)
      n = split(words, want, " ")
      out = ""; put = 0
      for (i = 1; i <= NF; i++) {
        mine = 0
        for (j = 1; j <= n; j++) { k = want[j]; sub(/=.*/, "", k); if ($i == k || index($i, k "=") == 1) mine = 1 }
        if (!mine) out = out (out == "" ? "" : " ") $i
        if ($i == "rootwait" && !put) { out = out " " words; put = 1 }
      }
      if (!put) out = out " " words
      print indent out
      next
    }
    { print }
' "$1"
}

# Remounts /flash read-write for the write, and back afterwards if it was read-only (never in the tests).
flash_rw() {
  [ -z "${POCKETUI_FLASH:-}" ] || return 0
  if awk '$2 == "/flash" { print $4 }' /proc/mounts | grep -q '^ro\(,\|$\)'; then
    mount -o remount,rw /flash || die "cannot remount /flash read-write"
    RO=1
  fi
}
flash_back() {
  sync
  [ $RO = 0 ] || mount -o remount,ro /flash || say "note: /flash stays read-write until the next start"
  RO=0
}

# Writes $NEW over $CFG in one rename, after checking it is pocket-ui's.
put_in_place() {
  sync
  ours "$NEW" || die "the new file would not be pocket-ui's; nothing changed"
  mv -f "$NEW" "$CFG" || die "cannot replace $CFG"
}

ACTION=${1:-}
[ -n "${POCKETUI_FLASH:-}" ] || [ "$(id -u)" = 0 ] || die "run it as root (ROCKNIX's SSH login is root)"
case $ACTION in
  status)
    ours "$CFG" || die "not pocket-ui's GRUB config; nothing to do here"
    if is_quiet "$CFG"; then say "quiet start: on"; else say "quiet start: off"; fi
    if [ -f "$BAK" ]; then say "backup: $BAK (sh grub-quiet.sh undo puts it back)"; else say "backup: none"; fi
    say "kernel line:$(grep "$LINUX_LINE" "$CFG" | sed 's/^[[:space:]]*linux[[:space:]]*/ /')"
    ;;
  apply)
    ours "$CFG" || die "not pocket-ui's GRUB config; nothing changed"
    if is_quiet "$CFG"; then
      say "quiet start: already on; nothing changed"
      exit 0
    fi
    C=$TMP/grub.cfg
    quieten "$CFG" >"$C" || { rm -f "$C"; die "cannot work out the new file"; }
    # Nothing but the kernel line and the echo line may differ, and the result has to be quiet.
    grep -v -e "$LINUX_LINE" -e "$ECHO_LINE" "$CFG" >"$C.old"
    grep -v -e "$LINUX_LINE" "$C" >"$C.new"
    if ! cmp -s "$C.old" "$C.new" || ! is_quiet "$C"; then
      rm -f "$C" "$C.old" "$C.new"
      die "the new file would change more than the kernel line, or would not be quiet; nothing changed"
    fi
    rm -f "$C.old" "$C.new"
    flash_rw
    # The file that started this system is the one to go back to.
    cp "$CFG" "$BAK.new" && mv -f "$BAK.new" "$BAK" || { rm -f "$BAK.new"; die "cannot write the backup $BAK; nothing changed"; }
    cp "$C" "$NEW" || die "cannot write $NEW"
    put_in_place
    flash_back
    say "quiet start: on from the next start. Kept the old file as $BAK; undo: sh grub-quiet.sh undo"
    ;;
  undo)
    [ -f "$BAK" ] || die "no backup $BAK; nothing to undo"
    ours "$BAK" || die "the backup is not pocket-ui's GRUB config; left alone"
    flash_rw
    cp "$BAK" "$NEW" || die "cannot write $NEW"
    put_in_place
    rm -f "$BAK"
    flash_back
    say "quiet start: off from the next start (the old grub.cfg is back)"
    ;;
  *)
    sed -n '2,20p' "$0" | sed 's/^# \{0,1\}//'
    [ "$ACTION" = -h ] || [ "$ACTION" = --help ] || exit 2
    ;;
esac
