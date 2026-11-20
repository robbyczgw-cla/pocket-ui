#!/bin/sh
# Tests of scripts/image/bios-checks.sh (the image check's "no BIOS files"): sh scripts/image/test-bios-checks.sh
# Fake STORAGE trees and FAT lists; needs no Docker and no image. Found 19.11.2026: every image with DSperate failed
# "no BIOS files" on DSperate's FreeBIOS licence folder, so the exception is one exact path and real BIOS files still fail.
set -u
HERE=$(cd "$(dirname "$0")" && pwd)
. "$HERE/bios-checks.sh"
T=$(mktemp -d)
trap 'rm -rf "$T"' EXIT
fails=0
V=0.6.0
LIC=pocket-ui/releases/$V/cores/dsperate/licences/DSperate_src_core_bios
fresh() { rm -rf "$T/s"; mkdir -p "$T/s/pocket-ui/releases/$V/cores"; : >"$T/fat"; }
put() { mkdir -p "$(dirname "$T/s/$1")"; echo x >"$T/s/$1"; }
expect() {  # expect ok|FAIL description
  if no_bios_files "$T/s" "$T/fat" "$V"; then got=ok; else got=FAIL; fi
  if [ "$got" = "$1" ]; then echo "ok   $2"; else echo "FAIL $2 (check said $got, expected $1)"; fails=$((fails + 1)); fi
}
fresh; expect ok "a STORAGE without BIOS passes"
fresh; put "$LIC/LICENSE.freebios"; expect ok "DSperate's FreeBIOS notice at its place in the release passes"
fresh; put "$LIC/LICENSE.freebios"; put "pocket-ui/releases/$V/cores/dsperate_libretro.so"; expect ok "with the core beside it"
fresh; put "roms/bios/scph1001.bin"; expect FAIL "a roms/bios folder fails"
fresh; put "pocket-ui/gba_bios.bin"; expect FAIL "a gba_bios.bin fails"
fresh; put "pocket-ui/SCPH_BIOS.BIN"; expect FAIL "an upper-case BIOS name fails"
fresh; put "$LIC/LICENSE.freebios"; put "$LIC/bios7.bin"; expect FAIL "a BIOS file inside DSperate's licence folder fails"
fresh; put "$LIC/LICENSE.freebios"; put "pocket-ui/releases/$V/cores/dsperate/licences/nds_bios9.bin"; expect FAIL "a BIOS beside DSperate's licences fails"
fresh; put "pocket-ui/releases/0.5.1/cores/dsperate/licences/DSperate_src_core_bios/LICENSE.freebios"; expect FAIL "the notice in another release's folder fails"
fresh; put "pocket-ui/licences/DSperate_src_core_bios/LICENSE.freebios"; expect FAIL "the notice elsewhere on STORAGE fails"
fresh; echo "/bios/dc_boot.bin" >"$T/fat"; expect FAIL "a BIOS in the FAT list fails"
fresh; put "$LIC/LICENSE.freebios"; echo "/KERNEL" >"$T/fat"; expect ok "a FAT list without BIOS passes"
[ $fails = 0 ] && echo "bios-checks: all passed" || { echo "bios-checks: $fails failed"; exit 1; }
