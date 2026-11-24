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

# The BLS entry for OpenABL (deploy/sd-efi/loader/entries/pocket-ui.conf) and GRUB's entry start the same thing.
BLS=$ROOT/deploy/sd-efi/loader/entries/pocket-ui.conf
check "the BLS entry starts GRUB's kernel, device tree and command line" 'bls_matches_grub "$BLS" "$CFG"'
sed 's| quiet loglevel=3 | |' "$BLS" >"$T/bls-loud.conf"
check "a BLS entry whose command line differs from GRUB's is refused" '! bls_matches_grub "$T/bls-loud.conf" "$CFG"'
sed '/^devicetree/d' "$BLS" >"$T/bls-nodtb.conf"
check "a BLS entry without the device tree is refused (OpenABL would take the menu's model, UFS visible)" '! bls_matches_grub "$T/bls-nodtb.conf" "$CFG"'
sed 's|miniv2-noufs.dtb|rp5-noufs.dtb|' "$BLS" >"$T/bls-otherdtb.conf"
check "a BLS entry naming another device tree than GRUB's is refused" '! bls_matches_grub "$T/bls-otherdtb.conf" "$CFG"'
{ cat "$BLS"; echo "initrd     /boot/pocket/initrd"; } >"$T/bls-initrd.conf"
check "a BLS entry with an initrd GRUB does not load is refused" '! bls_matches_grub "$T/bls-initrd.conf" "$CFG"'
sed 's|^linux .*|linux      /KERNEL|' "$BLS" >"$T/bls-kernel.conf"
check "a BLS entry starting ROCKNIX's /KERNEL instead of GRUB's Image is refused" '! bls_matches_grub "$T/bls-kernel.conf" "$CFG"'

# GRUB itself (scripts/make-sd-efi.sh): the pinned Debian packages, never "whatever trixie has today". 22.11.2026: trixie had
# moved on to 2.12-9+deb13u4, so every card's BOOTAA64.EFI differed from the device's and the release refused the warning.
. "$ROOT/scripts/image/pins.sh"
hex64() { printf '%s' "$1" | grep -Eq '^[0-9a-f]{64}$'; }
check "pins.sh pins GRUB 2.12-9+deb13u2 (the device's) on snapshot.debian.org" \
  '[ "${GRUB_DEB_VERSION:-}" = 2.12-9+deb13u2 ] && case ${GRUB_DEB_URL:-} in https://snapshot.debian.org/archive/debian/*/pool/main/g/grub2) true ;; *) false ;; esac'
check "pins.sh has the sha256 of grub-efi-arm64-bin (arm64) and grub-common (amd64, arm64)" \
  'hex64 "${GRUB_EFI_ARM64_DEB_SHA256:-}" && hex64 "${GRUB_COMMON_AMD64_DEB_SHA256:-}" && hex64 "${GRUB_COMMON_ARM64_DEB_SHA256:-}"'
check "make-sd-efi.sh no longer takes the newest GRUB (no apt-get download, no grub-common from the archive)" \
  '! grep -q "apt-get download" "$ROOT/scripts/make-sd-efi.sh" && ! grep -Eq "apt-get install .*[^/]grub-common( |$)" "$ROOT/scripts/make-sd-efi.sh"'
# A package with the wrong bytes is refused before any container runs (fake docker and curl, nothing is downloaded).
mkdir -p "$T/bin" "$T/debs"
printf '#!/bin/sh
[ "$1" = version ] && { echo amd64; exit 0; }
echo "$@" >>"%s/docker-calls"
' "$T" >"$T/bin/docker"
printf '#!/bin/sh
exit 22
' >"$T/bin/curl"
chmod +x "$T/bin/docker" "$T/bin/curl"
for f in "grub-efi-arm64-bin_${GRUB_DEB_VERSION:-x}_arm64.deb" "grub-common_${GRUB_DEB_VERSION:-x}_amd64.deb"; do echo "not the package" >"$T/debs/$f"; done
printf 'ANDROID!' >"$T/kernel"
: >"$T/tree.dtb"
PATH="$T/bin:$PATH" GRUB_DEB_CACHE="$T/debs" sh "$ROOT/scripts/make-sd-efi.sh" "$T/kernel" "$T/tree.dtb" "$T/sd" >"$T/wrong.log" 2>&1
rc=$?
check "make-sd-efi.sh refuses GRUB packages whose sha256 is not the pinned one, before Docker runs" \
  '[ $rc != 0 ] && [ ! -e "$T/docker-calls" ] && grep -q "sha256" "$T/wrong.log"'

echo
if [ $fails -eq 0 ]; then echo "all grub-checks tests passed"; else echo "$fails grub-checks tests failed"; exit 1; fi
