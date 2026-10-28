#!/bin/bash
# Tests for scripts/check-core-licences.sh. Builds small fixture trees (a hostsys.rs with HANDHELD_CORES and
# system(...) lines, a licence table) and checks what the check reports. Run: scripts/test-check-core-licences.sh
set -u
here="$(cd "$(dirname "$0")" && pwd)"
check="$here/check-core-licences.sh"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
pass=0; fail=0

TABLE='# core|verdict|version|evidence
okcore|or-later|abc123|header says "version 2 ... or (at your option) any later version"
v3core|v3|abc123|GPL-3.0-or-later
permcore|permissive|abc123|MIT
oldcore|v2-only|abc123|header says "version 2 as published by the Free Software Foundation"
mixcore|mixed-v2|abc123|one file says "version 2" only
ncore|non-commercial|abc123|licence forbids commercial use'

# fixture NAME HANDHELD_CORES_ENTRIES HOSTSYS_CORES   (entries like: ("gb","okcore"); cores space separated)
fixture() {
  local d="$tmp/$1"; mkdir -p "$d/src" "$d/scripts"
  printf '%s\n' "$TABLE" > "$d/scripts/core-licences.txt"
  {
    echo "pub const HANDHELD_CORES: &[(&str, &str)] = &[$2];"
    echo 'pub const SYSTEMS: &[HostSystem] = &['
    for c in $3; do printf '    system(\n        &["sys_%s", "alias"],\n        "%s",\n        NO_BIOS,\n        false,\n        true,\n    ),\n' "$c" "$c"; done
    echo '];'
  } > "$d/src/hostsys.rs"
  echo "$d"
}

# run NAME EXPECTED_EXIT GREP_PATTERN_THAT_MUST_MATCH [GREP_PATTERN_THAT_MUST_NOT_MATCH] -- check args...
t() {
  local name="$1" want_rc="$2" must="$3" mustnot="$4"; shift 4
  local out rc
  out="$("$check" "$@" 2>&1)"; rc=$?
  local ok=1
  [ "$rc" = "$want_rc" ] || ok=0
  [ -z "$must" ] || echo "$out" | grep -Eq "$must" || ok=0
  [ -z "$mustnot" ] || ! echo "$out" | grep -Eq "$mustnot" || ok=0
  if [ $ok = 1 ]; then pass=$((pass+1)); echo "ok   $name"; else fail=$((fail+1)); echo "FAIL $name (exit $rc, wanted $want_rc)"; echo "$out" | sed 's/^/     | /'; fi
}

d=$(fixture clean '("gb", "okcore"), ("gba", "permcore")' "v3core")
t "clean tree: no warning, exit 0" 0 "all hosted cores are GPLv3-compatible" "WARN" --root "$d" --table "$d/scripts/core-licences.txt"
t "clean tree, strict: exit 0" 0 "" "WARN" --strict --root "$d" --table "$d/scripts/core-licences.txt"

d=$(fixture v2main '("gb", "oldcore"), ("gba", "okcore")' "v3core")
t "v2-only core in HANDHELD_CORES: warns, report mode exits 0" 0 "WARN.*oldcore.*v2-only" "" --root "$d" --table "$d/scripts/core-licences.txt"
t "v2-only core in HANDHELD_CORES: strict exits 1" 1 "WARN.*oldcore" "" --strict --root "$d" --table "$d/scripts/core-licences.txt"

d=$(fixture v2hostsys '("gba", "okcore")' "v3core oldcore")
t "v2-only core in hostsys.rs: warns" 0 "WARN.*oldcore.*hostsys" "" --root "$d" --table "$d/scripts/core-licences.txt"
t "v2-only core in hostsys.rs: strict exits 1" 1 "WARN.*oldcore" "" --strict --root "$d" --table "$d/scripts/core-licences.txt"

d=$(fixture mixed '("gba", "okcore")' "mixcore")
t "mixed-v2 core: warns" 0 "WARN.*mixcore.*mixed-v2" "" --root "$d" --table "$d/scripts/core-licences.txt"

d=$(fixture unknown '("gba", "okcore"), ("x", "newcore")' "")
t "core missing from the table: warns" 0 "WARN.*newcore.*not in" "" --root "$d" --table "$d/scripts/core-licences.txt"
t "core missing from the table: strict exits 1" 1 "WARN.*newcore" "" --strict --root "$d" --table "$d/scripts/core-licences.txt"

d=$(fixture noncomm '("gba", "okcore")' "ncore")
t "non-commercial core: noted, not a warning, strict still 0" 0 "NOTE.*ncore.*non-commercial" "WARN" --strict --root "$d" --table "$d/scripts/core-licences.txt"

d=$(fixture oldtable '("gba", "okcore")' "")
echo 'okcore|v9-only|x|bogus verdict' > "$d/scripts/core-licences.txt"
t "unknown verdict word in the table: error exit 2" 2 "unknown verdict" "" --root "$d" --table "$d/scripts/core-licences.txt"

d=$(fixture nolist '' "")
sed -i '/HANDHELD_CORES/d' "$d/src/hostsys.rs"
t "HANDHELD_CORES missing from hostsys.rs: error exit 2, not silent" 2 "HANDHELD_CORES not found" "" --root "$d" --table "$d/scripts/core-licences.txt"

# the real tree: report mode must exit 0 and name the flagged cores
t "real tree: report mode exits 0" 0 "WARN" "" --root "$here/.."
t "real tree: every hosted core is in the table" 0 "" "not in core-licences" --root "$here/.."
t "real tree: gambatte (HANDHELD_CORES) is read" 0 "gambatte +v2-only" "" --list --root "$here/.."

echo "$pass passed, $fail failed"
[ $fail = 0 ]
