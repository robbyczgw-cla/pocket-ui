#!/bin/sh
# Test of rocknix-contract.sh on a tiny made-up ROCKNIX root and a data file with a row of every kind: all ok, then
# each kind broken once (CHANGED or MISSING with the evidence), the exit codes, -q, and --ref against a made-up source
# tree in a local git repository. No network, no unsquashfs; well under a second. cargo test runs it
# (os_compat::tests::the_rocknix_contract_script_passes_its_self_test).
#   scripts/test-rocknix-contract.sh
set -u
HERE=$(cd "$(dirname "$0")" && pwd)
T=$(mktemp -d "${TMPDIR:-/tmp}/rocknix-contract-test.XXXXXX")
trap 'rm -rf "$T"' EXIT
fails=0
pass() { echo "ok   $1"; }
fail() { echo "FAIL $1"; fails=$((fails + 1)); }
has() { if grep -qF -- "$2" "$T/out"; then pass "$1"; else fail "$1: no line with: $2"; sed 's/^/     | /' "$T/out"; fi; }
hasnt() { if grep -qF -- "$2" "$T/out"; then fail "$1: unexpected: $2"; else pass "$1"; fi; }
run() { ROCKNIX_CONTRACT_DATA=$T/data.txt sh "$HERE/rocknix-contract.sh" "$@" >"$T/out" 2>&1; rc=$?; }
code() { if [ "$rc" = "$2" ]; then pass "$1"; else fail "$1: exit $rc, not $2"; sed 's/^/     | /' "$T/out"; fi; }

R=$T/root
mkdir -p "$R/usr/bin" "$R/usr/lib/libretro" "$R/usr/lib/systemd/system" "$R/etc/profile.d" "$R/usr/config/system/configs" \
  "$R/usr/config/emulationstation"
ln -s usr/bin "$R/bin"
cat >"$R/etc/profile.d/001-functions" <<'EOF'
J_CONF_LOCK="/tmp/.system.cfg.lock"
function get_setting() {
  awk '/^global.'"${1}"='/' x
}
function wait_lock() {
  while true
  do
    if (set -o noclobber; echo "$$" > "${J_CONF_LOCK}") 2>/dev/null
    then break
    fi
  done
}
function set_setting() {
  echo "${1}=${2}" >> "${J_CONF}"
}
prerotate_env() {
  :
}
function sway_fullscreen {
  :
}
EOF
printf '#!/bin/bash\n. $(dirname $0)/es_settings\nprerotate_env 0 1\nemulationstation --no-splash\n' >"$R/usr/bin/start_es.sh"
printf '#!/bin/bash\nARG=$1\nPLATFORM="${ARGUMENTS##*-P}"\nX=$(get_setting cooling.profile "${PLATFORM}")\n' >"$R/usr/bin/runemu.sh"
chmod +x "$R/usr/bin/start_es.sh" "$R/usr/bin/runemu.sh"
ln -s /usr/bin/runemu.sh "$R/usr/bin/runemu-link"
printf 'TRIPS_QUIET="1 2 3 4 5 6"\n' >"$R/usr/bin/fancontrol"
printf 'system.language=en_US\nwifi.enabled=0\n' >"$R/usr/config/system/configs/system.cfg"
: >"$R/usr/lib/libretro/gambatte_libretro.so"
printf 'a\0/storage/.config/system.d\0b' >"$R/usr/lib/systemd/libsystemd-shared-255.so"
printf 'xx\0GNU C Library (GNU libc) stable release version 2.41.\0yy' >"$R/usr/lib/libc.so.6"
cat >"$R/usr/config/emulationstation/es_systems.cfg" <<'EOF'
<?xml version="1.0"?>
<systemList>
  <system>
    <name>gb</name>
    <fullname>Game Boy</fullname>
    <path>/storage/roms/gb</path>
    <extension>.gb .GB .zip</extension>
    <emulators><emulator name="retroarch"><cores><core default="true">gambatte</core></cores></emulator></emulators>
  </system>
  <system>
    <name>tools</name>
    <path>/storage/.config/modules</path>
    <extension>.sh</extension>
  </system>
  <system>
    <name>psx</name>
    <path>/storage/roms/psx</path>
    <extension>.cue .chd</extension>
  </system>
</systemList>
EOF
start_sha=$(if command -v sha256sum >/dev/null 2>&1; then sha256sum "$R/usr/bin/start_es.sh"; else shasum -a 256 "$R/usr/bin/start_es.sh"; fi | cut -d' ' -f1)

cat >"$T/data.txt" <<EOF
# a comment
file|the link to bin|launcher|bin/runemu.sh|
file|one of two|os_compat|usr/lib/nothing;usr/bin/fancontrol|
exec|runemu.sh through a link|spawn_game|usr/bin/runemu-link|
grep|runemu.sh reads -P|spawn_game|usr/bin/runemu.sh|PLATFORM="\\\$\{ARGUMENTS##\*-P\}"
fn|get_setting|emu_choice|etc/profile.d/001-functions|get_setting
fn|sway_fullscreen|none|etc/profile.d/001-functions|sway_fullscreen
body|set_setting appends|system_cfg|etc/profile.d/001-functions:set_setting|echo "\\\$\{1\}=
keys|runemu reads cooling.profile|fan_curve|usr/bin/runemu.sh|cooling.profile
cfg|default system.language|i18n|usr/config/system/configs/system.cfg|system.language
sha|start_es.sh known|launcher|usr/bin/start_es.sh|0000 $start_sha
calls|start_es.sh calls known functions|launcher|usr/bin/start_es.sh|etc/profile.d/001-functions prerotate_env
lockstyle|lock style|system_cfg|etc/profile.d/001-functions|
trips|quiet trips|fan_curve|usr/bin/fancontrol|quiet 1 2 3 4 5 6
sys|gb|library|gb|/storage/roms/gb .gb .GB .zip
sys|nds (not in the reference)|library|nds|-
sysroot|folders|library|usr/config/emulationstation/es_systems.cfg|/storage/roms/ tools
absent|no override|leds|usr/lib/override|
core|gambatte|hostsys|usr/lib/libretro|gambatte
glibc|glibc|zigbuild|usr/lib/libc.so.6|2.38
systemd|systemd|drop-in|usr/lib/systemd|254
text|unit folder|drop-in|usr/lib/systemd/libsystemd-shared-*.so|/storage/.config/system.d
live:file|system.cfg on the card|system_cfg|storage/.config/system/configs/system.cfg|
src|-|-|usr/bin/runemu.sh|pkg/runemu.sh
src|-|-|etc/profile.d/001-functions|pkg/001-functions
EOF

# 1. Everything as expected.
run --root "$R"
code "a root that keeps every promise exits 0" 0
has "the last line" "CONTRACT: ok"
has "a link inside the root is followed there" "ok      the link to bin: /bin/runemu.sh"
has "an absolute link is followed inside the root" "ok      runemu.sh through a link"
has "a grep row shows the line" 'usr/bin/runemu.sh:3:PLATFORM="${ARGUMENTS##*-P}"'
has "the second alternative counts" "ok      one of two: /usr/bin/fancontrol"
has "a function without ()" "ok      sway_fullscreen: sway_fullscreen() in"
has "sha known" "(a version the launcher knows)"
has "start_es.sh's calls" "ok      start_es.sh calls known functions: calls prerotate_env"
has "the 20261001 lock" "ok      lock style: noclobber with \$\$"
has "es_systems entry" "ok      gb: /storage/roms/gb, 3 extensions as expected"
has "a system known to live elsewhere" "ok      folders: 3 systems, every folder under /storage/roms/ but those of tools"
has "absent" "ok      no override: no /usr/lib/override"
has "a system not in the reference" "ok      nds (not in the reference): not in es_systems"
has "glibc version" "ok      glibc: glibc 2.41"
has "systemd version" "ok      systemd: systemd 255"
has "a string in a binary file found through a pattern" 'ok      unit folder: "/storage/.config/system.d" in /usr/lib/systemd/libsystemd-shared-255.so'
has "live rows are n/a on a folder" "n/a     system.cfg on the card: only on a running system"
has "the summary" "rocknix-contract: 21 ok, 0 changed or missing, 1 n/a"
run -q --root "$R"
hasnt "-q leaves out ok rows" "ok      gb"
has "-q keeps the last line" "CONTRACT: ok"

# 2. Each kind broken once.
rm "$R/usr/bin/fancontrol"
printf 'TRIPS_QUIET="1 2 3 4 5 7"\n' >"$R/usr/lib/fancontrol"
sed -i.bak 's/-P}/-p}/; s/cooling.profile/cooling.mode/' "$R/usr/bin/runemu.sh"
chmod -x "$R/usr/bin/runemu.sh"
printf 'scanout_reset_stale() {\n  :\n}\nfunction wait_lock() {\n  exec {fd}>>"${J_CONF_LOCK}"\n  flock -x "${fd}"\n}\nfunction set_setting() {\n  printf x\n}\n' >"$R/etc/profile.d/001-functions"
printf 'brand_new_step\nscanout_reset_stale\n' >>"$R/usr/bin/start_es.sh"
printf 'wifi.enabled=0\n' >"$R/usr/config/system/configs/system.cfg"
sed -i.bak 's#<path>/storage/roms/gb</path>#<path>/storage/games/gb</path>#; s#\.gb \.GB \.zip#.gb .GB .7z#' "$R/usr/config/emulationstation/es_systems.cfg"
rm "$R/usr/lib/libretro/gambatte_libretro.so"
printf 'GNU C Library stable release version 2.36.' >"$R/usr/lib/libc.so.6"
mkdir "$R/usr/lib/override"
rm "$R/usr/lib/systemd/libsystemd-shared-255.so"; printf "nothing" >"$R/usr/lib/systemd/libsystemd-shared-252.so"
run --root "$R"
code "a broken root exits 1" 1
has "a file gone" "MISSING one of two: /usr/lib/nothing, /usr/bin/fancontrol"
has "the code is named" "        pocket-ui: os_compat"
has "not executable" "CHANGED runemu.sh through a link: /usr/bin/runemu-link is not an executable file"
has "a grep that no longer matches" "CHANGED runemu.sh reads -P: /usr/bin/runemu.sh has no line matching"
has "a function gone" "MISSING get_setting: no function get_setting"
has "a function body changed" "CHANGED set_setting appends: set_setting() in /etc/profile.d/001-functions has no line matching"
has "a key no longer read" "CHANGED runemu reads cooling.profile: /usr/bin/runemu.sh no longer reads: cooling.profile"
has "a default key gone" "CHANGED default system.language: no system.language= in"
has "an unknown start_es.sh" "is not a version the launcher knows"
has "a new function call" "CHANGED start_es.sh calls known functions: calls scanout_reset_stale, which the launcher doesn't know"
has "the flock lock is known too" "ok      lock style: flock on \${J_CONF_LOCK}"
has "es_systems folder and extensions" "CHANGED gb: folder /storage/games/gb (was /storage/roms/gb); new extensions .7z; extensions gone .zip"
has "a folder outside /storage/roms" "CHANGED folders: not under /storage/roms/: gb=/storage/games/gb"
has "a core gone" "MISSING gambatte: /usr/lib/libretro/gambatte_libretro.so"
has "an old glibc" "CHANGED glibc: glibc 2.36 is older than 2.38"
has "an old systemd" "CHANGED systemd: systemd 252 is older than 254"
has "a string gone from a binary" 'CHANGED unit folder: no "/storage/.config/system.d" in /usr/lib/systemd/libsystemd-shared-252.so' 
has "an override that appeared" "CHANGED no override: /usr/lib/override is there now"
has "the count" "CONTRACT: 18 changed"
printf 'function wait_lock() {\n  touch "${J_CONF_LOCK}"\n}\n' >"$R/etc/profile.d/001-functions"
run --root "$R"
has "a lock style system_cfg doesn't know" "CHANGED lock style: wait_lock neither takes flock nor creates the file with noclobber"
mv "$R/usr/lib/fancontrol" "$R/usr/bin/fancontrol"
run --root "$R"
has "trips that differ" 'CHANGED quiet trips: TRIPS_QUIET="1 2 3 4 5 7", pocket-ui has "1 2 3 4 5 6"'

# 3. Usage.
run --root "$T/nothing"
code "a folder that isn't there exits 2" 2
run
code "no mode exits 2" 2

# 3b. --host: the checker and the data go to the other side in one piece (here a stand-in for ssh runs it on this
# computer's /, so most rows fail; what matters is that it runs, reads all rows and counts the live ones).
printf '#!/bin/sh\nshift\nexec "$@"\n' >"$T/fake-ssh"
chmod +x "$T/fake-ssh"
ROCKNIX_CONTRACT_SSH=$T/fake-ssh run --host pocket-not-used
has "--host runs the checker on the other side" "rocknix-contract: "
has "--host checks the live rows" "system.cfg on the card: "
hasnt "--host: live rows are not n/a there" "n/a     system.cfg on the card"
has "--host ends with the verdict" "CONTRACT: "

# 4. --ref: a made-up source tree.
S=$T/src
mkdir -p "$S/pkg"
printf '#!/bin/bash\nPLATFORM="${ARGUMENTS##*-P}"\nget_setting cooling.profile\n' >"$S/pkg/runemu.sh"
printf 'function get_setting() {\n  :\n}\nfunction wait_lock() {\n  exec {J_CONF_LOCK_FD}>>"${J_CONF_LOCK}"\n  flock -x "${J_CONF_LOCK_FD}"\n}\nfunction set_setting() {\n  echo "${1}=${2}" >> "${J_CONF}"\n}\nsway_fullscreen() {\n  :\n}\n' >"$S/pkg/001-functions"
if command -v git >/dev/null 2>&1; then
  (cd "$S" && git init -q && git add -A && git -c user.name=t -c user.email=t@t commit -qm one) || fail "git repository"
  ROCKNIX_SRC=$S run --ref HEAD
  code "--ref with every mapped file as expected exits 0" 0
  has "a mapped script is checked" "ok      runemu.sh reads -P"
  has "the lock of the source tree" "ok      lock style: flock on"
  has "a row whose file isn't mapped" "n/a     start_es.sh known: not in the source mapping"
  has "es_systems is n/a in the source" "n/a     gb: not in the source tree"
  has "--ref ends with the verdict" "CONTRACT: ok"
else
  echo "skip --ref (no git)"
fi

echo
if [ $fails -eq 0 ]; then echo "test-rocknix-contract: all passed"; exit 0; fi
echo "test-rocknix-contract: $fails failed"
exit 1
