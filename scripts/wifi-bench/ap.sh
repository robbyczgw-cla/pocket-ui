#!/bin/sh
# The world around the device, inside the access point container (docs/WIFI-BENCH.md): hostapd access points on hwsim
# radios, all bridged into one network (br0, 10.77.0.1/24) with dnsmasq for DHCP and DNS, and a web server that plays
# "the internet" for pocket-ui's online check (every name resolves to 10.77.0.1; http://nmcheck.gnome.org/
# check_network_status.txt answers "NetworkManager is online"), or a captive portal instead of it.
#
#   ap.sh net                                  br0, dnsmasq, the internet stand-in
#   ap.sh start NAME SSID PSK|- CHANNEL        an access point on radio NAME ("-": open network)
#   ap.sh stop NAME                            that access point gone (the phone's hotspot switched off)
#   ap.sh portal MODE | ap.sh internet         the captive portal (scripts/portal-standin.py MODE) or the internet
#   ap.sh dhcp off|on                          DHCP stops answering, or answers again
#   ap.sh status
set -eu
B=/bench
RUN=/run/wb
mkdir -p "$RUN" /var/log/wb
LOG=/var/log/wb

iface_of() { ls "/sys/class/ieee80211/$1/device/net/" 2>/dev/null | head -1; }

dnsmasq_start() {
  dhcp=$1
  [ -f "$RUN/dnsmasq.pid" ] && kill "$(cat "$RUN/dnsmasq.pid")" 2>/dev/null || true
  sleep 0.2
  set -- --interface=br0 --bind-interfaces --no-resolv --no-hosts --address=/#/10.77.0.1 \
    --log-facility="$LOG/dnsmasq.log" --log-dhcp --log-queries --pid-file="$RUN/dnsmasq.pid" --dhcp-leasefile="$RUN/leases"
  [ "$dhcp" = on ] && set -- "$@" --dhcp-range=10.77.0.50,10.77.0.150,255.255.255.0,1h --dhcp-option=3,10.77.0.1 --dhcp-option=6,10.77.0.1
  dnsmasq "$@"
}

web_stop() {
  for f in internet portal; do
    [ -f "$RUN/$f.pid" ] && kill "$(cat "$RUN/$f.pid")" 2>/dev/null || true
    rm -f "$RUN/$f.pid"
  done
  nft delete table ip wbportal 2>/dev/null || true
}

case ${1:-} in
  net)
    ip link add br0 type bridge 2>/dev/null || true
    ip addr add 10.77.0.1/24 dev br0 2>/dev/null || true
    ip link set br0 up
    ip link set lo up
    dnsmasq_start on
    "$0" internet
    ;;
  start)
    name=$2 ssid=$3 psk=$4 chan=$5
    dev=$(iface_of "$name")
    [ -n "$dev" ] || { echo "ap.sh: no radio $name here" >&2; exit 1; }
    {
      echo "interface=$dev"; echo "bridge=br0"; echo "driver=nl80211"; echo "ssid=$ssid"; echo "channel=$chan"
      echo "hw_mode=g"; echo "ieee80211n=1"; echo "ctrl_interface=$RUN/hostapd-$name"
      if [ "$psk" != - ]; then
        echo "wpa=2"; echo "wpa_key_mgmt=WPA-PSK"; echo "rsn_pairwise=CCMP"; echo "wpa_passphrase=$psk"
      fi
    } >"$RUN/$name.conf"
    hostapd -B -t -P "$RUN/$name.pid" -f "$LOG/hostapd-$name.log" "$RUN/$name.conf"
    i=0
    until grep -q AP-ENABLED "$LOG/hostapd-$name.log" 2>/dev/null; do
      i=$((i + 1)); [ $i -gt 50 ] && { tail -5 "$LOG/hostapd-$name.log" >&2; exit 1; }; sleep 0.1
    done
    echo "ap $name: $ssid on channel $chan ($dev, $(cat "/sys/class/net/$dev/address"))"
    ;;
  stop)
    name=$2
    [ -f "$RUN/$name.pid" ] && kill "$(cat "$RUN/$name.pid")" 2>/dev/null || true
    rm -f "$RUN/$name.pid"
    echo "ap $name: stopped"
    ;;
  internet)
    web_stop
    python3 -I "$B/scripts/wifi-bench/internet.py" 10.77.0.1 80 >>"$LOG/internet.log" 2>&1 &
    echo $! >"$RUN/internet.pid"
    ;;
  portal)
    mode=${2:-hotel}
    web_stop
    rm -f "$RUN/portal.port"
    PORTAL_STANDIN_BASE=http://login.wb-hotel.example python3 -I "$B/scripts/portal-standin.py" "$mode" "$B/testdata/portal" "$RUN/portal.port" "$LOG/portal-requests.log" >>"$LOG/portal.log" 2>&1 &
    echo $! >"$RUN/portal.pid"
    i=0
    until [ -s "$RUN/portal.port" ]; do i=$((i + 1)); [ $i -gt 50 ] && { echo "ap.sh: portal did not start" >&2; exit 1; }; sleep 0.1; done
    port=$(cat "$RUN/portal.port")
    # The stand-in only listens on 127.0.0.1: the network's web requests are sent there, as a hotel's gateway does.
    sysctl -qw net.ipv4.conf.br0.route_localnet=1
    nft -f - <<EOF
table ip wbportal {
  chain pre { type nat hook prerouting priority dstnat; iifname "br0" tcp dport 80 dnat to 127.0.0.1:$port; }
}
EOF
    echo "portal $mode on 127.0.0.1:$port"
    ;;
  dhcp) dnsmasq_start "${2:?on|off}" ;;
  status)
    for p in "$RUN"/*.pid; do [ -f "$p" ] && echo "$(basename "$p" .pid): $(kill -0 "$(cat "$p")" 2>/dev/null && echo up || echo down)"; done
    cat "$RUN/leases" 2>/dev/null || true
    ;;
  *) echo "usage: see the header of ap.sh" >&2; exit 2 ;;
esac
