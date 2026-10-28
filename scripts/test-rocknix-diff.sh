#!/bin/sh
# Test of rocknix-diff.sh on two made-up unpacked systems (folders; no unsquashfs, git or network needed): the UI-start
# scripts, functions, system.cfg keys, es_systems entries, cores, start scripts and libraries it has to report, and the
# checklist lines they make.
#   scripts/test-rocknix-diff.sh
set -u
HERE=$(cd "$(dirname "$0")" && pwd)
T=$(mktemp -d)
trap 'rm -rf "$T"' EXIT
fails=0
check() {
  if grep -qF -- "$2" "$T/out"; then echo "ok   $1"; else echo "FAIL $1: no line with: $2"; fails=$((fails + 1)); fi
}
check_not() {
  if grep -qF -- "$2" "$T/out"; then echo "FAIL $1: unexpected: $2"; fails=$((fails + 1)); else echo "ok   $1"; fi
}

# side <dir> <old|new>
side() {
  d=$1
  mkdir -p "$d/usr/bin" "$d/etc/profile.d" "$d/usr/config/system/configs" "$d/usr/config/emulationstation" "$d/usr/lib/libretro"
  # ROCKNIX 20261001's start_es.sh (a version the launcher knows), changed on the new side.
  printf '#!/bin/bash\n# SPDX-License-Identifier: GPL-2.0-or-later\n# Copyright (C) 2024 ROCKNIX (https://github.com/ROCKNIX)\n\n### setup is the same\n. $(dirname $0)/es_settings\n\n' >"$d/usr/bin/start_es.sh"
  [ "$2" = new ] && printf 'brand_new_step\n\n' >>"$d/usr/bin/start_es.sh"
  printf 'emulationstation --log-path /var/log --no-splash\n' >>"$d/usr/bin/start_es.sh"
  printf 'export SDL_AUDIODRIVER=pulseaudio\n' >"$d/usr/bin/es_settings"
  echo 'runemu' >"$d/usr/bin/runemu.sh"
  [ "$2" = new ] && echo 'runemu, changed' >"$d/usr/bin/runemu.sh"
  {
    printf 'function get_setting() {\n  echo one\n}\n\n'
    if [ "$2" = old ]; then printf 'function wait_lock() {\n  sleep 1\n}\n\nfunction old_helper() {\n  :\n}\n'; fi
    if [ "$2" = new ]; then printf 'function wait_lock() {\n  flock -x 3\n}\n\nfunction brand_new_step() {\n  :\n}\n'; fi
  } >"$d/etc/profile.d/001-functions"
  if [ "$2" = old ]; then printf 'wifi.enabled=0\ngone.key=1\nngpc.intergerscale=1\n' >"$d/usr/config/system/configs/system.cfg"
  else printf 'wifi.enabled=1\nngpc.integerscale=1\nwifi.country=US\n' >"$d/usr/config/system/configs/system.cfg"; fi
  core=gambatte
  [ "$2" = new ] && core=sameboy
  cat >"$d/usr/config/emulationstation/es_systems.cfg" <<EOF
<?xml version="1.0"?>
<systemList>
  <system><name>gb</name><path>/storage/roms/gb</path><extension>.gb .zip</extension>
    <emulators><emulator name="retroarch"><cores><core default="true">$core</core></cores></emulator></emulators></system>
  <system><name>only$2</name><path>/storage/roms/x</path><extension>.x</extension></system>
</systemList>
EOF
  : >"$d/usr/lib/libretro/${core}_libretro.so"
  : >"$d/usr/lib/libretro/mgba_libretro.so"
  : >"$d/usr/bin/start_melonds.sh"
  [ "$2" = old ] && : >"$d/usr/bin/start_mednafen.sh"
  : >"$d/usr/lib/libxkbcommon.so.0.8.0"
  ln -s libxkbcommon.so.0.8.0 "$d/usr/lib/libxkbcommon.so.0"
  [ "$2" = old ] && : >"$d/usr/lib/libEGL.so.1"
}
side "$T/old" old
side "$T/new" new
sh "$HERE/rocknix-diff.sh" "$T/old" "$T/new" >"$T/out" 2>&1
echo "exit $?" >>"$T/out"

check "it ends normally" "exit 0"
check "start_es.sh: changed and not a version the launcher knows" "start_es.sh: CHANGED, NOT known to the launcher"
check "the new line in its diff" "+brand_new_step"
check "es_settings unchanged" "es_settings: same"
check "checklist: repeat the new step" "[ ] start_es.sh"
check "runemu.sh changed" "runemu.sh: changed (+1 -1)"
check "functions: added, gone and changed" "- added: brand_new_step"
check "functions: gone" "- gone: old_helper"
check "functions: wait_lock changed, which pocket-ui relies on" "wait_lock (pocket-ui relies on it)"
check "checklist: wait_lock" "[ ] function wait_lock changed"
check "system.cfg: changed value" "wifi.enabled: 0 -> 1 (pocket-ui uses it)"
check "system.cfg: added" "added wifi.country=US"
check "system.cfg: removed" "removed gone.key (was 1)"
check "es_systems: a core change in a system pocket-core plays" "gb (pocket-core plays it): retroarch: (('gambatte',), ('gambatte',)) -> (('sameboy',), ('sameboy',))"
check "es_systems: systems added and gone" "new system onlynew"
check "es_systems: gone" "system onlyold gone"
check "cores: gambatte gone, marked" "gone: gambatte (pocket-core uses it)"
check "checklist: the core pocket-core needs" "[ ] core gambatte gone"
check "start scripts: Mednafen's gone, which pocket-ui names" "gone: start_mednafen.sh (pocket-ui names it)"
check "checklist: the start script" "[ ] start_mednafen.sh gone"
check "libraries: the link and its target" "libxkbcommon.so.0 -> libxkbcommon.so.0.8.0"
check "libraries: EGL gone" "libEGL.so.1 (file)  ->  libEGL.so.1 MISSING"
check "checklist: the missing library" "[ ] libEGL.so.1 MISSING"
check_not "mgba is in both: not listed" "mgba (pocket-core uses it)"

sh "$HERE/rocknix-diff.sh" "$T/old" 20261001 >"$T/out" 2>&1
echo "exit $?" >>"$T/out"
check "a ref and a folder are not compared" "not one of each"
check "  and that is an error" "exit 2"

echo
if [ $fails -eq 0 ]; then echo "all rocknix-diff tests passed"; else echo "$fails rocknix-diff test(s) failed"; exit 1; fi
