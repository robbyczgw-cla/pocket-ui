#!/bin/sh
# Tests of scripts/image/grub-checks.sh (the image check's reading of grub.cfg): sh scripts/image/test-grub-checks.sh
# Runs against the repository's deploy/sd-efi/grub.cfg and variants of it. Needs no Docker and no image.
set -u
HERE=$(cd "$(dirname "$0")" && pwd)
ROOT=$(cd "$HERE/../.." && pwd)
. "$HERE/grub-checks.sh"
CFG=$ROOT/deploy/sd-efi/grub.cfg
GQ=$ROOT/deploy/grub-quiet.sh
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

# The repository's own file, which names fbcon=rotate and fbcon=map in its comment: this failed the image check
# on the day the boot screen's grub.cfg met it.
check "the fixture comment names fbcon= (the case under test)" 'grep -Eq "^#.*fbcon=" "$CFG"'
check "deploy/sd-efi/grub.cfg is clean" 'grub_cfg_quiet_clean "$CFG"'
check "deploy/sd-efi/grub.cfg is already quiet for boot-quiet.sh" 'grub_cfg_quiet_for_boot_quiet "$CFG" "$GQ"'

# Variants that must be refused.
sed 's|^  search .*|&\n  echo "pocket-ui: loading..."|' "$CFG" >"$T/echo.cfg"
check "an echo line is refused" '! grub_cfg_quiet_clean "$T/echo.cfg"'
sed 's|^  search .*|&\n\techo hello|' "$CFG" >"$T/tab-echo.cfg"
check "an echo line after a tab is refused" '! grub_cfg_quiet_clean "$T/tab-echo.cfg"'
sed 's|^\(  linux .*\) gpt$|\1 gpt fbcon=rotate:1|' "$CFG" >"$T/fbcon.cfg"
check "fbcon=rotate on the kernel line is refused" '! grub_cfg_quiet_clean "$T/fbcon.cfg"'
sed 's| quiet loglevel=3 vt.global_cursor_default=0||' "$CFG" >"$T/loud.cfg"
check "a kernel line without the quiet words is no quiet start for boot-quiet.sh" '! grub_cfg_quiet_for_boot_quiet "$T/loud.cfg" "$GQ"'
check "the loud file itself has no echo and no fbcon= (so only boot-quiet.sh's reading refuses it)" 'grub_cfg_quiet_clean "$T/loud.cfg"'
before=$(cksum <"$CFG")
grub_cfg_quiet_for_boot_quiet "$CFG" "$GQ"
check "the file given is left as it was" '[ "$(cksum <"$CFG")" = "$before" ]'
# The check as it was before grub-checks.sh: red on the repository's own file.
check "the old one-line grep refused deploy/sd-efi/grub.cfg (why this file exists)" 'grep -Eq "^ *echo |fbcon=" "$CFG"'

echo
if [ $fails -eq 0 ]; then echo "all grub-checks tests passed"; else echo "$fails grub-checks tests failed"; exit 1; fi
