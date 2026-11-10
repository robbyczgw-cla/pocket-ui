#!/bin/sh
# Test of scripts/image/storage-budget.sh and of what build.sh puts into STORAGE (no Docker, no download, seconds).
# Part of the gate (scripts/gate.sh). The 0.3.1 programs did not fit into the 32 MiB STORAGE (2026-11-08): both image builds
# stopped with "Could not allocate block"; the licence texts were stored there a second time.
set -u
cd "$(dirname "$0")/../.." || exit 1
. scripts/image/pins.sh
fail=0
ok() { echo "ok: $1"; }
bad() { echo "FAIL: $1" >&2; fail=1; }
T=$(mktemp -d) || exit 1
trap 'rm -rf "$T"' EXIT
mk() { # dir ui core splash (bytes)
  mkdir -p "$1"; truncate -s "$2" "$1/pocket-ui"; truncate -s "$3" "$1/pocket-core"; truncate -s "$4" "$1/pocket-splash"
}
budget() { sh scripts/image/storage-budget.sh "$1" >"$T/out" 2>&1; }

# The real sizes of the two releases (aarch64 release builds).
mk "$T/v030" 19278504 6057800 1989008
mk "$T/v031" 19543168 6066736 1991712
for version in v030 v031; do
  budget "$T/$version"; r=$?
  [ $r = 1 ] && grep -q 'next release will not' "$T/out" && ok "$version: old thin-LTO programs no longer meet the growth reserve" || { bad "$version: insufficient reserve must fail (exit $r)"; cat "$T/out" >&2; }
done

# Whole-program LTO, opt-level 3: measured AArch64 programs from the same source (docs/BINARY-SIZE.md).
mk "$T/fat" 18140104 5593224 1949552
budget "$T/fat"; r=$?; [ $r = 0 ] && ok "fat-LTO programs fit with the new reserve" || { bad "fat-LTO programs: exit $r"; cat "$T/out" >&2; }
left=$(sed -n 's/.* \([0-9]*\) left = .*/\1/p' "$T/out")
[ "${left:-0}" -ge 512 ] && ok "fat-LTO keeps at least 2 MiB free (${left:-0} blocks)" || bad "fat-LTO keeps only ${left:-0} blocks free"

# With these core/splash sizes and the pinned image, a UI of exactly 4647 blocks leaves 512 blocks.
# One extra byte needs another block; a byte-count-only budget must not let it pass.
mk "$T/boundary" 19034112 5593224 1949552
budget "$T/boundary"; r=$?; [ $r = 0 ] && ok "exactly the growth reserve fits" || bad "growth reserve boundary: exit $r"
truncate -s 19034113 "$T/boundary/pocket-ui"
budget "$T/boundary"; r=$?; [ $r = 1 ] && grep -q 'next release will not' "$T/out" && ok "one block below the reserve fails" || bad "one block below the reserve: exit $r"

# Growth: each of the next releases may add what 0.3.1 did (67 blocks); a program 700 KiB bigger must be called out before the build.
mk "$T/grown" $((19543168 + 700 * 1024)) 6066736 1991712
budget "$T/grown"; r=$?; [ $r = 1 ] && grep -q "next release will not" "$T/out" && ok "programs 700 KiB bigger: too little room is red" || { bad "700 KiB growth: exit $r"; cat "$T/out" >&2; }
mk "$T/huge" $((19543168 + 2 * 1024 * 1024)) 6066736 1991712
budget "$T/huge"; r=$?; [ $r = 1 ] && grep -q "does NOT fit" "$T/out" && ok "programs that do not fit at all are red" || { bad "2 MiB growth: exit $r"; cat "$T/out" >&2; }
# The same 0.3.1 programs with the licence texts stored a second time (303 blocks) would not have fitted: the failure of 08.11.
[ $((P2_FREE_BLOCKS - 6767 - 303)) -lt 0 ] && ok "licence texts on STORAGE as well would have been 43 blocks short" || bad "arithmetic of the 0.3.1 failure"
mkdir -p "$T/empty"; budget "$T/empty"; r=$?; [ $r = 2 ] && ok "programs missing: usage error (exit 2), not a pass" || bad "missing programs: exit $r"

# build.sh: the licence texts are on the FAT partition (pocket-ui-info/licences); STORAGE has a link to them, no second copy.
! grep -q 'cp "\$W/info/licences/"\* "\$S/storage/licences/"' scripts/image/build.sh && ok "build.sh does not copy the licence texts into the STORAGE stage" || bad "build.sh still copies the licence texts into STORAGE"
grep -q 'symlink /pocket-ui/licences /flash/pocket-ui-info/licences' scripts/image/build.sh && ok "build.sh links /pocket-ui/licences to /flash/pocket-ui-info/licences" || bad "build.sh has no licences link"
grep -q 'P2_FREE_BLOCKS' scripts/image/build.sh && ok "build.sh checks the free blocks of ROCKNIX's STORAGE against pins.sh" || bad "build.sh does not check P2_FREE_BLOCKS"
grep -q 'P2_MIN_HEADROOM_BLOCKS' scripts/image/check.sh && ok "check.sh fails a finished image with too little room (not a warning)" || bad "check.sh has no hard headroom check"
! grep -q 'warn "STORAGE keeps a quarter free' scripts/image/check.sh && ok "the 'quarter free' warning is gone" || bad "the 'quarter free' warning is still a warning"
grep -q 'storage-budget.sh' scripts/release-check.sh && ok "release-check.sh runs the budget" || bad "release-check.sh does not run the budget"

[ $fail = 0 ] && echo "storage budget tests: ok" || echo "storage budget tests: FAILED" >&2
exit $fail
