#!/bin/bash
# Test for scripts/check-release-tree.py. Builds small temporary trees and archives and checks the exit code
# of the checker: a tree with a BIOS or ROM must fail, a clean tree and the real repository must pass.
# Everything is fake: the "BIOS" and "ROM" files hold a few bytes of text. No real BIOS or ROM is involved.
#
#   scripts/test-check-release-tree.sh
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CHECK="$ROOT/scripts/check-release-tree.py"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
pass=0
fail=0

# expect <name> <want: pass (exit 0) | fail (exit 1, findings) | error (exit 2)> <command...>
expect() {
    local name="$1" want="$2"
    shift 2
    local out rc
    out="$("$@" 2>&1)"
    rc=$?
    # exit codes of the checker: 0 = clean, 1 = something found, 2 = usage error / script missing
    local got=error
    [ "$rc" -eq 0 ] && got=pass
    [ "$rc" -eq 1 ] && got=fail
    if [ "$got" = "$want" ]; then
        printf 'ok    %-52s (checker %s, exit %s)\n' "$name" "$got" "$rc"
        pass=$((pass + 1))
    else
        printf 'FAIL  %-52s wanted %s, checker %s (exit %s)\n' "$name" "$want" "$got" "$rc"
        printf '%s\n' "$out" | sed 's/^/      | /' | head -8
        fail=$((fail + 1))
    fi
}

check() { python3 -I "$CHECK" "$@"; }

# --- trees -------------------------------------------------------------------------------------------------
mkdir -p "$TMP/bios-tree/roms" && printf 'not a real bios\n' > "$TMP/bios-tree/roms/scph1001.bin"
mkdir -p "$TMP/bios-nested/a/b" && printf 'x\n' > "$TMP/bios-nested/a/b/DC_BOOT.BIN"
mkdir -p "$TMP/rom-tree" && printf 'not a rom\n' > "$TMP/rom-tree/Game (Europe).sfc"
mkdir -p "$TMP/iso-tree" && printf 'not an iso\n' > "$TMP/iso-tree/disc.iso"
mkdir -p "$TMP/clean-tree/src" && printf 'fn main() {}\n' > "$TMP/clean-tree/src/main.rs" \
    && printf 'hello\n' > "$TMP/clean-tree/README.md"
# content match: an innocent name, but the md5 is on the (test) list
mkdir -p "$TMP/md5-tree" && printf 'hello md5 test\n' > "$TMP/md5-tree/readme.dat"
printf '%s  test-entry  # made up for this test\n' "$(python3 -I -c 'import hashlib;print(hashlib.md5(b"hello md5 test\n").hexdigest())')" > "$TMP/md5-list.txt"
# allowlist: an own test fixture that would otherwise be flagged
mkdir -p "$TMP/allow-tree/tests/fixtures" && printf 'own homebrew test file\n' > "$TMP/allow-tree/tests/fixtures/own-test.nes"
printf '# path glob  # reason\ntests/fixtures/own-test.nes  # our own test file, MIT\n' > "$TMP/allow-list.txt"
# an unlisted core binary
mkdir -p "$TMP/core-tree" && printf '\177ELF fake\n' > "$TMP/core-tree/snes9x_libretro.so"

# --- archives ----------------------------------------------------------------------------------------------
mkdir -p "$TMP/tar-src/storage/roms" && printf 'x\n' > "$TMP/tar-src/storage/roms/scph5501.bin"
tar -C "$TMP/tar-src" -cf "$TMP/bios.tar" storage
tar -C "$TMP/tar-src" -czf "$TMP/bios.tar.gz" storage
tar -C "$TMP/clean-tree" -cf "$TMP/clean.tar" .
(cd "$TMP/rom-tree" && python3 -I -c 'import zipfile;z=zipfile.ZipFile("../rom.zip","w");z.write("Game (Europe).sfc");z.close()')
# a "disk image": raw bytes with a BIOS name inside, as a directory entry of a file system would hold it
python3 -I -c 'import sys;open(sys.argv[1],"wb").write(b"\0"*4096+b"SCPH1001.BIN"+b"\0"*4096)' "$TMP/disk.img"
python3 -I -c 'import sys;open(sys.argv[1],"wb").write(b"\0"*8192)' "$TMP/clean.img"

echo "checker: $CHECK"
expect "tree with scph1001.bin fails"            fail check "$TMP/bios-tree"
expect "tree with nested DC_BOOT.BIN fails"      fail check "$TMP/bios-nested"
expect "tree with a .sfc ROM fails"              fail check "$TMP/rom-tree"
expect "tree with an .iso fails"                 fail check "$TMP/iso-tree"
expect "tree with a libretro core binary fails"  fail check "$TMP/core-tree"
expect "tree matched by md5 list fails"          fail check --md5-list "$TMP/md5-list.txt" "$TMP/md5-tree"
expect "md5 tree passes without the list entry"  pass check --md5-list /dev/null "$TMP/md5-tree"
expect "clean tree passes"                       pass check "$TMP/clean-tree"
expect "fixture without allowlist fails"         fail check "$TMP/allow-tree"
expect "fixture with allowlist passes"           pass check --allowlist "$TMP/allow-list.txt" "$TMP/allow-tree"
expect "tar with a BIOS member fails"            fail check "$TMP/bios.tar"
expect "tar.gz with a BIOS member fails"         fail check "$TMP/bios.tar.gz"
expect "clean tar passes"                        pass check "$TMP/clean.tar"
expect "zip with a ROM member fails"             fail check "$TMP/rom.zip"
expect "raw image with a BIOS name fails"        fail check "$TMP/disk.img"
expect "clean raw image passes"                  pass check "$TMP/clean.img"
expect "two arguments: one bad, one good fails"  fail check "$TMP/clean-tree" "$TMP/bios-tree"
expect "missing path is a usage error"           error check "$TMP/does-not-exist"
# the real repository, with the project's allowlist (default arguments)
expect "real repository tree passes"             pass check

echo
echo "passed: $pass, failed: $fail"
[ "$fail" -eq 0 ]
