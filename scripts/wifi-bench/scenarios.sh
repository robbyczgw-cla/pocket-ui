# The bench's scenarios (docs/WIFI-BENCH.md), sourced by bench.sh. Each runs on a fresh pair of containers and writes
# PASS/FAIL lines (check), NOTE lines, numbered screenshots + visible text (shot) and the logs of both sides (collect).
# A check says what the player should see, or what NetworkManager/iwd should be left with; a FAIL is a reproduction.

# boot [WIFI_ENABLED] [blocked]: the device starts: system.cfg's wifi.enabled (default 1), the services, the station's
# radio blocked by rfkill once NetworkManager runs (the fresh card's log: NetworkManager up at 4 s, wlan0 unavailable
# at 15.6 s when ROCKNIX's autostart applied wifi.enabled=0; a block before NetworkManager starts is undone by it,
# its own wish being on), then pocket-ui.
boot() {
  bench_up 0
  set_wifi_enabled "${1:-1}"
  services_up
  if [ "${2:-}" = blocked ]; then
    sleep 2
    sta_rfkill block
    wait_for 10 sh -c "docker exec $STA nmcli -t -f DEVICE,STATE dev | grep -qx 'wlan0:unavailable'" || note "wlan0 not unavailable after the block"
  fi
  deploy_ui
  sleep 2
}

# Settings → Connections → Wi-Fi from the shelf (START, three rows down, A, A).
open_wifi_page() {
  # After 120 s without input the screen goes dark, and a press then only wakes it: the rest would shift (the hotspot's
  # wait launched the shelf's game). B wakes it; on the shelf B does nothing else.
  if sx grep -E '^sleep: (light|awake)' /var/log/pocket-ui.log | tail -1 | grep -q '^sleep: light'; then ui b; sleep 1; fi
  ui start; ui down; ui down; ui down; ui a; ui a
  sees "Other network" 5 || { shot wifi-page-missing; return 1; }
}

# The page's first row (L1 jumps five up and stops at the top).
top_row() { ui l1; ui l1; ui l1; }

# scan_for SSID [SECONDS]: X until the page lists SSID.
scan_for() {
  _sf_end=$(( $(date +%s) + ${2:-40} ))
  while [ "$(date +%s)" -le "$_sf_end" ]; do
    ui x
    sees "$1" 12 && return 0
  done
  return 1
}

# connect_with_password PASSWORD: A on the first row (the network), the keyboard, START.
connect_with_password() {
  top_row; ui a
  sees "Password for" 5 || { shot no-password-prompt; return 1; }
  type_text "$1"
  ui start
}

# sees_either A B [SECONDS]: the visible text holds A or B.
sees_either() {
  _sx_end=$(( $(date +%s) + ${3:-10} ))
  while [ "$(date +%s)" -le "$_sx_end" ]; do
    frame | grep -qF -e "$1" -e "$2" && return 0
    sleep 0.5
  done
  return 1
}
ssid_rows() { frame | grep -cxF -- "$1" || true; }
pocket_ui_alive() { sx pidof pocket-ui >/dev/null; }
bssid_in_use() { sx iw dev wlan0 link | sed -n 's/^Connected to \([0-9a-f:]*\).*/\1/p'; }
ap_of_bssid() { # the radio name (wbapN) whose address is BSSID
  for n in 1 2 3 4 5 6; do
    [ "$(ax cat /sys/class/ieee80211/wbap$n/macaddress 2>/dev/null)" = "$1" ] && { echo "wbap$n"; return; }
  done
}
page_connected() { sees "Connected to $1" "${2:-5}"; }
# connections_row SSID [SECONDS]: on Settings → Connections (one B from the Wi-Fi page) the Wi-Fi row names SSID.
# The row's facts are read again every 5 s while the list is shown (settings_values::EVERY).
connections_row() {
  sees "Remote access" 3 || return 1
  sees "$1 >" "${2:-10}" && return 0
  # What the row is made of (settings_values::read): "connected" is wlan0's operstate "up" (status_corner).
  note "Connections shows '$(frame | grep -A1 -xF Wi-Fi | sed -n 2p)'; wlan0 operstate $(sx cat /sys/class/net/wlan0/operstate), NetworkManager $(nm_state)"
  _cr_t0=$(date +%s)
  if sees "$1 >" 70; then note "  ... the row named $1 $(( $(date +%s) - _cr_t0 )) s later (the page is painted again at the minute)"
  else note "  ... still not after 70 s more"; fi
  return 1
}

# --- (a) a mesh: six access points, one name, one password; roaming when the one in use goes away --------------------
scenario_mesh() {
  boot 1
  for n in 1 2 3 4 5 6; do ax sh /bench/scripts/wifi-bench/ap.sh start wbap$n BenchMesh mesh-pass-1 $(( (n - 1) % 3 * 5 + 1 )); done
  open_wifi_page
  check "the page lists the mesh network" scan_for BenchMesh
  shot mesh-list
  check "six access points with one name are one row" test "$(ssid_rows BenchMesh)" = 1
  connect_with_password mesh-pass-1
  check "the page says connected to BenchMesh (within 60 s)" page_connected BenchMesh 60
  shot connected
  check "NetworkManager: wlan0 connected to BenchMesh" is_connected_to BenchMesh
  check "an address from the network" has_ip
  for round in 1 2; do
    bss=$(bssid_in_use); ap=$(ap_of_bssid "$bss")
    note "round $round: in use $bss ($ap); switching it off"
    [ -n "$ap" ] && ax sh /bench/scripts/wifi-bench/ap.sh stop "$ap"
    t0=$(date +%s)
    if wait_for 60 sh -c "[ \"\$(docker exec $STA iw dev wlan0 link | sed -n 's/^Connected to \([0-9a-f:]*\).*/\1/p')\" != '' ] && [ \"\$(docker exec $STA iw dev wlan0 link | sed -n 's/^Connected to \([0-9a-f:]*\).*/\1/p')\" != '$bss' ]"; then
      note "round $round: on $(bssid_in_use) after $(( $(date +%s) - t0 )) s"
    fi
    check "round $round: on another access point of the mesh within 60 s" wait_for 20 sh -c "b=\$(docker exec $STA iw dev wlan0 link | sed -n 's/^Connected to \([0-9a-f:]*\).*/\1/p'); [ -n \"\$b\" ] && [ \"\$b\" != '$bss' ]"
    check "round $round: NetworkManager connected with an address (within 30 s more)" wait_for 30 sh -c "docker exec $STA nmcli -t -f DEVICE,STATE dev | grep -qx 'wlan0:connected' && docker exec $STA ip -4 addr show wlan0 | grep -q 'inet 10.77.0.'"
    ui x
    check "round $round: the page still says connected to BenchMesh" page_connected BenchMesh 30
    shot "round$round"
  done
  ui b
  check "Connections says BenchMesh (within 15 s)" connections_row BenchMesh 15
  check "one saved profile for BenchMesh" test "$(profiles_for BenchMesh)" = 1
  check "no error on the page" sh -c "! docker exec $STA cat /tmp/frame.txt | grep -qE 'Try again|didn.t work|out of range'"
  check "pocket-ui still running" pocket_ui_alive
}

# --- (b) a phone's hotspot that goes away and comes back -----------------------------------------------------------
scenario_hotspot() {
  boot 1
  ax sh /bench/scripts/wifi-bench/ap.sh start wbap1 "Phone hotspot" hot-pass-1 6
  open_wifi_page
  check "the page lists the hotspot" scan_for "Phone hotspot"
  connect_with_password hot-pass-1
  check "the page says connected to Phone hotspot (within 60 s)" page_connected "Phone hotspot" 60
  shot connected
  ui b; ui b; ui b   # back to the shelf: the player plays, the watchdog (src/wifi.rs) watches
  ax sh /bench/scripts/wifi-bench/ap.sh stop wbap1
  note "hotspot off at $(date +%T); back on in 90 s (longer than the watchdog's 60 s offline)"
  check "NetworkManager notices the hotspot is gone (within 30 s)" wait_for 30 sh -c "! docker exec $STA nmcli -t -f DEVICE,STATE dev | grep -qx 'wlan0:connected'"
  sleep 90
  ax sh /bench/scripts/wifi-bench/ap.sh start wbap1 "Phone hotspot" hot-pass-1 6
  t0=$(date +%s)
  check "connected again within 90 s of the hotspot's return" wait_for 90 sh -c "docker exec $STA nmcli -t -f DEVICE,STATE dev | grep -qx 'wlan0:connected' && docker exec $STA ip -4 addr show wlan0 | grep -q 'inet 10.77.0.'"
  note "back after $(( $(date +%s) - t0 )) s; pocket-ui: $(sx grep -E 'wifi: (offline|back|reconnect)' /var/log/pocket-ui.log | tr '\n' ';')"
  # The incident of 2026-10-27 (hotspot): pocket-ui's "connect saved" took down a link NetworkManager had just brought
  # back ("new-activation") and the second DHCP round never finished. 20 s to see if anything does that here.
  sleep 20
  check "nothing restarted a connection NetworkManager had already brought back (no 'new-activation')" \
    sh -c "! docker exec $STA grep -q \"reason 'new-activation'\" /var/log/NetworkManager.log"
  check "still connected 20 s later" is_connected_to "Phone hotspot"
  open_wifi_page
  check "the page says connected to Phone hotspot" page_connected "Phone hotspot" 20
  shot back
  ui b
  check "Connections says Phone hotspot (within 15 s)" connections_row "Phone hotspot" 15
  check "one saved profile" test "$(profiles_for 'Phone hotspot')" = 1
  check "pocket-ui still running" pocket_ui_alive
}

# --- (c) a café network with a sign-in page -------------------------------------------------------------------------
# The stand-in's "clickthrough" portal (one Continue button; the hotel's form wants "Müller", which the letters layer
# cannot type). The redirect names login.wb-hotel.example; the network's DNS and gateway send that to the stand-in.
scenario_portal() {
  boot 1
  ax sh /bench/scripts/wifi-bench/ap.sh start wbap1 "Cafe Kranz" - 11
  ax sh /bench/scripts/wifi-bench/ap.sh portal clickthrough
  open_wifi_page
  check "the page lists the café network" scan_for "Cafe Kranz"
  top_row; ui a   # an open network: no password
  check "the page says connected to Cafe Kranz (within 60 s)" page_connected "Cafe Kranz" 60
  check "the page shows the sign-in row (within 30 s)" sees "This network wants a sign-in" 30
  shot portal-row
  check "the sign-in row names the sign-in page's host" sees "login.wb-hotel.example" 2
  ui b
  check "Connections says Cafe Kranz, not 'Not connected', while the sign-in is pending (within 15 s)" connections_row "Cafe Kranz" 15
  shot connections-before-sign-in
  ui a                   # back into the Wi-Fi page (the Wi-Fi row is still selected)
  sees "This network wants a sign-in" 5 || true
  top_row; ui a          # the sign-in row: the sheet
  check "the sign-in sheet offers Sign in… on the Pocket" sees "Sign in…" 5
  shot sheet
  ui a                   # Sign in…: the helper loads the page
  sees_either "Continue" "No answer" 20
  check "the sign-in page loads at the first Sign in… (no 'No answer')" sh -c "! docker exec $STA cat /tmp/frame.txt | grep -qF 'No answer'"
  tries=1
  while frame | grep -qF "No answer" && [ $tries -lt 5 ]; do
    [ $tries = 1 ] && shot no-answer
    note "Sign in… try $tries: 'No answer' at $(date +%T) (portal requests so far: $(ax grep -c . /var/log/wb/portal-requests.log))"
    sleep 5; ui a; tries=$((tries + 1))   # the page's own Try again
    sees_either "Continue" "No answer" 20
  done
  check "the sign-in helper shows the café's page with its Continue button (within 20 s)" sees "Continue" 20
  check "the helper went through the portal's redirect" sh -c "docker exec $AP grep -q '/portal/start' /var/log/wb/portal-requests.log"
  shot sign-in-page
  ui a                   # Send (the page's one form)
  check "the helper says online after Send (within 20 s)" sees "You're online" 20
  check "the form was sent to the portal" sh -c "docker exec $AP grep -q opennds_auth /var/log/wb/portal-requests.log"
  shot after-send
  ui b; ui b
  check "the Wi-Fi page says online, without the sign-in row" sh -c "docker exec $STA cat /tmp/frame.txt | grep -qF \"You're online now\" && ! docker exec $STA cat /tmp/frame.txt | grep -qF 'This network wants a sign-in'"
  shot online
  ui b
  check "Connections says Cafe Kranz after the sign-in (within 15 s)" connections_row "Cafe Kranz" 15
  check "still connected" is_connected_to "Cafe Kranz"
  check "pocket-ui still running" pocket_ui_alive
}

# --- (d) a wrong password, then the right one ----------------------------------------------------------------------
scenario_password() { password_flow 1; }
# The fresh card's own network: one name on six access points (iwd tried each of them in its log, 2026-10-27).
scenario_password_mesh() { password_flow 6; }

password_flow() {
  boot 1
  for n in $(seq 1 "$1"); do ax sh /bench/scripts/wifi-bench/ap.sh start wbap$n BenchHome right-pass-1 $(( (n - 1) % 3 * 5 + 1 )); done
  open_wifi_page
  check "the page lists BenchHome" scan_for BenchHome
  connect_with_password wrong-pass-2
  check "the page says the password is wrong (within 90 s)" sees "Wrong password. Try again." 90
  shot wrong-password
  check "NetworkManager is not left connecting or connected" sh -c "s=\$(docker exec $STA nmcli -t -f DEVICE,STATE dev | sed -n 's/^wlan0://p'); [ \"\$s\" = disconnected ]"
  note "profiles after the wrong password: $(profiles_for BenchHome); iwd: $(sx ls /storage/.cache/iwd | tr '\n' ' ')"
  check "a network that never connected is not shown as Saved" sh -c "! docker exec $STA cat /tmp/frame.txt | grep -qxF Saved"
  # What a player does next: type the password again. Either the keyboard is still open under the message (fix branch
  # of 2026-10-27), or the page is back on the list and A on the network should ask for it; the fresh card instead
  # tried the saved (wrong) profile first ("net: connect saved").
  if frame | grep -qF "Password for"; then
    note "the keyboard stays open under the message"
  else
    top_row; ui a
  fi
  if sees "Password for" 5; then
    check "A after a wrong password asks for the password without trying the wrong one again" \
      sh -c "[ \"\$(docker exec $STA grep -c 'net: connect saved$' /var/log/pocket-ui.log)\" = 0 ]"
    note "pocket-ui's last connect lines: $(sx grep -E 'net: connect|wifi: connection failed' /var/log/pocket-ui.log | tail -3 | tr '\n' ';')"
    type_text right-pass-1
    ui start
  else
    note "A again did not ask for the password: $(frame | head -3 | tr '\n' '|')"
    shot no-prompt
    sees "Try again" 60 || true
    shot after-saved-attempt
    # The player's way out on the fresh card: Forget, then the password again.
    top_row; ui y; ui y; sleep 3
    top_row; ui a
    sees "Password for" 5 && { type_text right-pass-1; ui start; }
  fi
  check "the page says connected to BenchHome (within 90 s)" page_connected BenchHome 90
  shot connected
  check "NetworkManager: wlan0 connected to BenchHome" is_connected_to BenchHome
  check "exactly one saved profile for BenchHome" test "$(profiles_for BenchHome)" = 1
  check "pocket-ui still running" pocket_ui_alive
}

# --- (e) Forget while NetworkManager is still trying to connect ---------------------------------------------------
scenario_forget() {
  boot 1
  ax sh /bench/scripts/wifi-bench/ap.sh start wbap1 BenchHome old-pass-1 6
  open_wifi_page
  check "the page lists BenchHome" scan_for BenchHome
  connect_with_password old-pass-1
  check "connected with the old password" page_connected BenchHome 60
  # The router's password changes: the saved profile no longer works; NetworkManager keeps trying it.
  ax sh /bench/scripts/wifi-bench/ap.sh stop wbap1
  sleep 1
  ax sh /bench/scripts/wifi-bench/ap.sh start wbap1 BenchHome new-pass-2 6
  check "NetworkManager is trying the saved profile (within 60 s)" wait_for 60 sh -c "docker exec $STA nmcli -t -f DEVICE,STATE dev | grep -qE '^wlan0:(connecting|preparing|configuring|need auth)'"
  note "state when forgetting: $(nm_state)"
  top_row; ui y; ui y
  sleep 5
  shot forgot
  check "the page shows no error about forgetting" sh -c "! docker exec $STA cat /tmp/frame.txt | grep -qiE \"couldn.t forget|try again\""
  check "pocket-ui logged no failed forget" sh -c "! docker exec $STA grep -q \"couldn't forget\" /var/log/pocket-ui.log"
  check "no saved profile for BenchHome" wait_for 10 sh -c "[ \"\$(docker exec $STA nmcli -t -f NAME,TYPE con | grep -c '^BenchHome:802-11-wireless$')\" = 0 ]"
  check "iwd keeps no BenchHome.psk" wait_for 10 sh -c "! docker exec $STA test -e /storage/.cache/iwd/BenchHome.psk"
  sleep 10
  check "NetworkManager stopped trying (disconnected 10 s later)" sh -c "[ \"\$(docker exec $STA nmcli -t -f DEVICE,STATE dev | sed -n 's/^wlan0://p')\" = disconnected ]"
  # Then the new password, as the player would.
  ui x; sees BenchHome 20 || true
  connect_with_password new-pass-2
  check "the page says connected to BenchHome with the new password (within 90 s)" page_connected BenchHome 90
  shot connected-new
  check "NetworkManager: connected to BenchHome" is_connected_to BenchHome
  check "exactly one saved profile for BenchHome" test "$(profiles_for BenchHome)" = 1
  check "pocket-ui still running" pocket_ui_alive
}

# --- (f) the radio blocked at boot: a fresh card (Wi-Fi off) -------------------------------------------------------
scenario_rfkill() {
  boot 0 blocked
  ax sh /bench/scripts/wifi-bench/ap.sh start wbap1 BenchHome right-pass-1 6
  open_wifi_page
  check "the page says Wi-Fi is off (and how to turn it on)" sees "Wi-Fi is off" 15
  shot off
  check "pocket-ui did not switch it on by itself (the wish is off)" sh -c "docker exec $STA rfkill list | grep -A1 wbsta | grep -q 'Soft blocked: yes'"
  top_row; ui a
  check "A turns it on: the radio is unblocked (within 20 s)" wait_for 20 sh -c "docker exec $STA rfkill list | grep -A1 wbsta | grep -q 'Soft blocked: no'"
  check "the page lists BenchHome (within 40 s)" scan_for BenchHome 40
  shot on
  check "wifi.enabled is stored as 1" sh -c "docker exec $STA grep -qx 'wifi.enabled=1' /storage/.config/system/configs/system.cfg"
  connect_with_password right-pass-1
  check "the page says connected to BenchHome (within 60 s)" page_connected BenchHome 60
  shot connected
  check "pocket-ui still running" pocket_ui_alive
}

# --- (f') the radio blocked at boot while the wish is on (a resume or boot hook): pocket-ui switches it on once -----
scenario_rfkill_on() {
  boot 1 blocked
  ax sh /bench/scripts/wifi-bench/ap.sh start wbap1 BenchHome right-pass-1 6
  check "pocket-ui switches the radio on by itself (within 30 s)" wait_for 30 sh -c "docker exec $STA rfkill list | grep -A1 wbsta | grep -q 'Soft blocked: no'"
  check "pocket-ui logged why" log_has 'blocked while wifi.enabled is on; switching it on once'
  open_wifi_page
  check "the page lists BenchHome (within 40 s)" scan_for BenchHome 40
  check "the page does not say Wi-Fi is off" sh -c "! docker exec $STA cat /tmp/frame.txt | grep -qF 'Wi-Fi is off'"
  shot on
  connect_with_password right-pass-1
  check "the page says connected to BenchHome (within 60 s)" page_connected BenchHome 60
  check "pocket-ui still running" pocket_ui_alive
}
