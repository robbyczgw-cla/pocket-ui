# The Wi-Fi bench's building blocks (docs/WIFI-BENCH.md), sourced by bench.sh on the Docker host (the colima VM's
# engine on a Mac). Two containers on the VM's kernel with mac80211_hwsim:
#   wb-ap     Debian: hostapd access points, dnsmasq, the internet or a captive portal (ap.sh); creates all radios
#   vdev-wb   the virtual device (scripts/vdev.sh, ROCKNIX 20261001 userland) started with --network none: ROCKNIX's
#             own NetworkManager 1.51.4 + iwd 3.12 on one hwsim radio, and the real pocket-ui on top
# Radios are created in wb-ap (they only hear radios made in the same network namespace) and the station's is moved
# into vdev-wb. Nothing on the Mac or in the VM's own network changes.

WB_ROOT=${WB_ROOT:?}               # the staged tree: scripts/, docker/, deploy/, testdata/portal, target/.../release
AP=wb-ap
STA=vdev-wb
STA_PHY=wbsta
export VDEV_NAME=wb
AP_IMAGE=wifi-bench-ap:$(shasum "$WB_ROOT/scripts/wifi-bench/ap/Dockerfile" | cut -c1-10)
OUT=${WB_OUT:-$WB_ROOT/results/manual}
STEP=0

say() { printf '[%s] %s\n' "$(date +%T)" "$*" >&2; }
die() { say "bench: $*"; exit 1; }
ax() { docker exec "$AP" "$@"; }
sx() { docker exec "$STA" "$@"; }

# --- the kernel ---------------------------------------------------------------------------------------------------
vm() { # a command as root in the VM (colima on a Mac; directly where Docker runs on Linux with root)
  if command -v colima >/dev/null 2>&1; then colima ssh -- sudo sh -c "$*"; else sh -c "$*"; fi
}

ensure_hwsim() {
  vm 'test -d /sys/module/mac80211_hwsim' 2>/dev/null && return 0
  say "loading mac80211_hwsim (radios=0) in the VM"
  vm 'modprobe mac80211_hwsim radios=0 2>/dev/null || { apt-get update -qq && DEBIAN_FRONTEND=noninteractive apt-get install -y -qq linux-modules-extra-$(uname -r) >/dev/null && modprobe mac80211_hwsim radios=0; }' \
    || die "mac80211_hwsim can't be loaded in the VM"
}

# --- containers ---------------------------------------------------------------------------------------------------
ensure_ap_image() {
  docker image inspect "$AP_IMAGE" >/dev/null 2>&1 && return 0
  say "building $AP_IMAGE"
  docker build -q -t "$AP_IMAGE" "$WB_ROOT/scripts/wifi-bench/ap" >/dev/null
}

# up [onboarding 0|1]: both containers, the radios, the station's services; pocket-ui is not started yet (deploy_ui).
bench_up() {
  onboarding=${1:-0}
  ensure_hwsim
  ensure_ap_image
  bench_down quiet
  mkdir -p "$OUT"
  docker run -d --name "$AP" --hostname wb-ap --privileged --pid=host --network none \
    -v "$WB_ROOT:/bench:ro" "$AP_IMAGE" sleep infinity >/dev/null
  VDEV_ONBOARDING=$onboarding VDEV_DOCKER_ARGS="--network none" "$WB_ROOT/scripts/vdev.sh" up 2>>"$OUT/bench.log" \
    || die "the virtual device did not come up"
  # The radios: the station's and six for access points, all in the AP container's namespace.
  ax python3 -I /bench/scripts/wifi-bench/hwsim.py new "$STA_PHY" wbap1 wbap2 wbap3 wbap4 wbap5 wbap6 >>"$OUT/bench.log"
  ax iw phy "$STA_PHY" set netns "$(docker inspect -f '{{.State.Pid}}' "$STA")"
  ax sh /bench/scripts/wifi-bench/ap.sh net
  docker cp "$WB_ROOT/scripts/wifi-bench/sta-services.sh" "$STA:/usr/local/sta-services.sh" 2>/dev/null \
    || { sx mkdir -p /usr/local; docker cp "$WB_ROOT/scripts/wifi-bench/sta-services.sh" "$STA:/usr/local/sta-services.sh"; }
  # The station's wlan0: hwsim names it wlanN; the device's is wlan0 (wifictl takes the first wlan*, pocket-ui wlan0).
  i=0 dev=
  while [ -z "$dev" ] && [ $i -lt 50 ]; do
    dev=$(sx sh -c "ls /sys/class/ieee80211/$STA_PHY/device/net/ 2>/dev/null" | head -1); i=$((i + 1)); sleep 0.1
  done
  [ -n "$dev" ] || die "the station's radio did not arrive in $STA"
}

# The fresh card's state: wifi.enabled in system.cfg (0 on a fresh ROCKNIX), rfkill of the station's radio.
# ROCKNIX's first boot copies system.cfg from /usr/config; the vdev starts without one.
set_wifi_enabled() {
  sx sh -c "[ -f /storage/.config/system/configs/system.cfg ] || { mkdir -p /storage/.config/system/configs && cp /usr/config/system/configs/system.cfg /storage/.config/system/configs/; }"
  sx sed -i "s/^wifi.enabled=.*/wifi.enabled=$1/" /storage/.config/system/configs/system.cfg
}
sta_rfkill() { # block|unblock: only the station's radio (rfkill's ids are the VM's; `rfkill block wifi` would hit the APs)
  id=$(sx sh -c "basename \$(ls -d /sys/class/ieee80211/$STA_PHY/rfkill*)" | sed 's/rfkill//')
  sx rfkill "$1" "$id"
}

services_up() { sx sh /usr/local/sta-services.sh start >>"$OUT/bench.log" 2>&1 || die "station services did not start (bench.log)"; }

# pocket-ui as `vdev.sh deploy --no-build` starts it, with the visible text of every frame in /tmp/frame.txt.
deploy_ui() {
  sx sh -c 'grep -v "^POCKETUI_VISIBLE_TEXT=" /storage/pocket-ui/dev.env > /tmp/dev.env; echo POCKETUI_VISIBLE_TEXT=/tmp/frame.txt >> /tmp/dev.env; cat /tmp/dev.env > /storage/pocket-ui/dev.env'
  "$WB_ROOT/scripts/vdev.sh" deploy --no-build >>"$OUT/bench.log" 2>&1 || die "pocket-ui did not start (bench.log)"
}

collect() { # logs of both sides into $OUT
  mkdir -p "$OUT/logs"
  for f in pocket-ui.log iwd.log NetworkManager.log resolved.log udevd.log vdev-boot.log; do
    docker cp "$STA:/var/log/$f" "$OUT/logs/$f" 2>/dev/null || true
  done
  docker cp "$STA:/storage/.config/pocket-ui/logs" "$OUT/logs/pocket-ui-logs" 2>/dev/null || true
  docker cp "$AP:/var/log/wb" "$OUT/logs/ap" 2>/dev/null || true
  sx sh -c 'nmcli -f all dev; echo; nmcli -f NAME,UUID,TYPE,AUTOCONNECT,ACTIVE con; echo; ls -la /storage/.cache/iwd /storage/.config/NetworkManager/system-connections; for f in /storage/.cache/iwd/*.psk /storage/.config/NetworkManager/system-connections/*; do [ -f "$f" ] && { echo "== $f"; sed "s/^\(Passphrase\|PreSharedKey\|psk\)=.*/\1=<hidden>/" "$f"; }; done' \
    >"$OUT/logs/nm-end-state.txt" 2>&1 || true
}

bench_down() {
  [ "${1:-}" = quiet ] || collect
  docker rm -f "$STA" "$AP" >/dev/null 2>&1 || true
  # Radios left over (a container that died early): gone with their namespaces; the module stays loaded.
}

# --- the UI -------------------------------------------------------------------------------------------------------
ui() { sx vsend "$@"; sleep "${UI_PAUSE:-0.35}"; }
frame() { sx cat /tmp/frame.txt 2>/dev/null; }

# sees TEXT [SECONDS]: the visible text holds TEXT (a fixed string) within SECONDS (default 10).
# (No locals in sh: every helper's variables have a prefix of their own, or a caller's loop would be overwritten.)
sees() {
  _se_end=$(( $(date +%s) + ${2:-10} ))
  while [ "$(date +%s)" -le "$_se_end" ]; do
    frame | grep -qF -- "$1" && return 0
    sleep 0.5
  done
  return 1
}

# shot NAME: a JPEG screenshot (sips on the Mac, else the PNG) and the visible text, numbered in order.
shot() {
  STEP=$((STEP + 1)); n=$(printf '%02d-%s' "$STEP" "$1")
  mkdir -p "$OUT/shots"
  frame >"$OUT/shots/$n.txt"
  sx sh -c 'grim /tmp/wb-shot.png' 2>/dev/null && docker cp "$STA:/tmp/wb-shot.png" "$OUT/shots/$n.png" 2>/dev/null || return 0
  if command -v sips >/dev/null 2>&1; then
    sips -s format jpeg -s formatOptions 70 "$OUT/shots/$n.png" --out "$OUT/shots/$n.jpg" >/dev/null 2>&1 && rm -f "$OUT/shots/$n.png"
  fi
}

# type_text TEXT: the on-screen keyboard (src/keyboard.rs, letters layer; starts on "q") typed with the D-pad.
# Only characters of the letters layer: 0-9 a-z - _ . @ (the bench's passwords use nothing else).
type_text() {
  moves=$(printf '%s' "$1" | awk '
    BEGIN { rows[0]="1234567890"; rows[1]="qwertyuiop"; rows[2]="asdfghjkl-"; rows[3]="zxcvbnm_.@"; r=1; c=0 }
    { n=split($0, ch, ""); for (i=1; i<=n; i++) {
        found=0; for (rr=0; rr<4 && !found; rr++) { p=index(rows[rr], ch[i]); if (p) { tr=rr; tc=p-1; found=1 } }
        if (!found) { print "ERR " ch[i]; exit 1 }
        while (r<tr) { out=out " down"; r++ } while (r>tr) { out=out " up"; r-- }
        while (c<tc) { out=out " right"; c++ } while (c>tc) { out=out " left"; c-- }
        out=out " a" } print out }')
  case $moves in ERR*) die "type_text: $moves is not on the letters layer" ;; esac
  # One FIFO writer for all keys (vsend's own pause is 0.2 s a key; the keyboard takes them faster).
  # shellcheck disable=SC2086
  sx sh -c 'for k in "$@"; do echo "$k" > /tmp/pocket-ui.in; sleep 0.06; done' sh $moves
  sleep 0.5
  frame | grep -qxF -- "$1" || note "typed '$1' but the screen shows: $(frame | sed -n 2p)"
}

# --- checks -------------------------------------------------------------------------------------------------------
# check "what the user/NM should show" COMMAND...: one PASS/FAIL line in $OUT/result.txt.
check() {
  what=$1; shift
  if "$@"; then echo "PASS  $what" | tee -a "$OUT/result.txt" >&2
  else echo "FAIL  $what" | tee -a "$OUT/result.txt" >&2; { echo "--- visible text at FAIL ($what):"; frame; echo; } >>"$OUT/fails.txt"; fi
}
nm_state() { sx nmcli -t -f DEVICE,STATE dev | sed -n 's/^wlan0://p'; }
nm_active_ssid() { sx nmcli -t -f ACTIVE,SSID dev wifi list --rescan no | sed -n 's/^yes://p' | head -1; }
is_connected_to() { [ "$(nm_state)" = connected ] && [ "$(nm_active_ssid)" = "$1" ]; }
wait_for() { # SECONDS COMMAND...: until COMMAND succeeds
  _wf_end=$(( $(date +%s) + $1 )); shift
  while [ "$(date +%s)" -le "$_wf_end" ]; do "$@" && return 0; sleep 1; done
  "$@"
}
profiles_for() { sx nmcli -t -f NAME,TYPE con | grep -c "^$1:802-11-wireless$" || true; }
has_ip() { sx ip -4 addr show wlan0 | grep -q 'inet 10\.77\.0\.'; }
log_has() { sx grep -qE -- "$1" /var/log/pocket-ui.log; }
note() { echo "NOTE  $*" | tee -a "$OUT/result.txt" >&2; }
