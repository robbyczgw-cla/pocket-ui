#!/bin/sh
# A stand-in for `wpa_cli` for the host tests of deploy/brick/hwroot/usr/bin/nmcli: the same output formats as the real one on
# the Brick (read off it 2026-11-02: `status` as key=value, `list_networks` and `scan_results` tab separated with a header),
# its state in $FAKE_WPA (a folder): `networks` (id TAB ssid TAB bssid TAB flags), `scan` (bssid TAB freq TAB dBm TAB flags TAB ssid),
# `psk-of` (ssid TAB key: the passwords the fake "network" accepts), `current` (a network id), `calls` (every command, one per line).
# Usage like wpa_cli: fake-wpa-cli.sh -p DIR -i IFACE COMMAND ARGS...
S=${FAKE_WPA:?FAKE_WPA}
[ "$1" = -p ] && shift 2; [ "$1" = -i ] && shift 2
cmd=$1; shift
echo "$cmd $*" >>"$S/calls"
TAB=$(printf '\t')
case "$cmd" in
  status)
    id=$(cat "$S/current" 2>/dev/null)
    if [ -n "$id" ] && [ "$(cat "$S/phase" 2>/dev/null)" != handshake ]; then
      ssid=$(awk -F"$TAB" -v i="$id" '$1 == i { print $2 }' "$S/networks")
      printf 'bssid=00:00:5e:00:53:01\nfreq=2422\nssid=%s\nid=%s\nmode=station\npairwise_cipher=CCMP\nkey_mgmt=WPA2-PSK\nwpa_state=COMPLETED\nip_address=192.0.2.172\naddress=00:00:5e:00:53:11\n' "$ssid" "$id"
    else
      printf 'wpa_state=%s\naddress=00:00:5e:00:53:11\n' "$( [ "$(cat "$S/phase" 2>/dev/null)" = handshake ] && echo 4WAY_HANDSHAKE || echo DISCONNECTED )"
    fi ;;
  list_networks)
    printf 'network id / ssid / bssid / flags\n'
    cur=$(cat "$S/current" 2>/dev/null)
    while IFS="$TAB" read -r id ssid bssid flags; do
      [ -n "$id" ] || continue
      f=$flags; [ "$id" = "$cur" ] && [ "$(cat "$S/phase" 2>/dev/null)" != handshake ] && f="[CURRENT]"
      printf '%s\t%s\t%s\t%s\n' "$id" "$ssid" "$bssid" "$f"
    done <"$S/networks" ;;
  scan) : ;;
  scan_results)
    printf 'bssid / frequency / signal level / flags / ssid\n'; cat "$S/scan" ;;
  add_network)
    n=$(awk -F"$TAB" 'BEGIN { m = -1 } $1 > m { m = $1 } END { print m + 1 }' "$S/networks")
    printf '%s\t\tany\t\n' "$n" >>"$S/networks"; echo "$n" ;;
  set_network)
    id=$1; key=$2; val=$3
    case "$key" in
      ssid) v=${val#\"}; v=${v%\"}
        awk -F"$TAB" -v OFS="$TAB" -v i="$id" -v s="$v" '$1 == i { $2 = s } { print }' "$S/networks" >"$S/networks.new" && mv "$S/networks.new" "$S/networks" ;;
      psk) v=${val#\"}; v=${v%\"}; echo "$id$TAB$v" >>"$S/psk-set" ;;
      scan_ssid) echo "$id" >>"$S/hidden" ;;
    esac
    echo OK ;;
  get_network)
    case "$2" in
      scan_ssid) grep -qx "$1" "$S/hidden" 2>/dev/null && echo 1 || echo FAIL ;;
      key_mgmt) echo WPA-PSK ;;
    esac ;;
  select_network)
    id=$1
    ssid=$(awk -F"$TAB" -v i="$id" '$1 == i { print $2 }' "$S/networks")
    given=$(awk -F"$TAB" -v i="$id" '$1 == i { v = $2; f = 1 } END { if (f) print "set:" v }' "$S/psk-set" 2>/dev/null)
    want=$(awk -F"$TAB" -v s="$ssid" '$1 == s { print "set:" $2 }' "$S/psk-of")
    if [ -n "$given" ] && [ "$given" != "$want" ]; then
      echo handshake >"$S/phase"; rm -f "$S/current"    # a key the network refuses: the handshake never ends
    else
      echo "$id" >"$S/current"; rm -f "$S/phase"
    fi
    echo OK ;;
  enable_network|save_config|disconnect|reconnect) echo OK ;;
  remove_network)
    awk -F"$TAB" -v i="$1" '$1 != i' "$S/networks" >"$S/networks.new" && mv "$S/networks.new" "$S/networks"
    [ "$(cat "$S/current" 2>/dev/null)" = "$1" ] && rm -f "$S/current"
    echo OK ;;
  *) echo FAIL; exit 1 ;;
esac
