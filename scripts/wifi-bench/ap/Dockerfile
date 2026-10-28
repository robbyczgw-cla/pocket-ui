# The access point side of the Wi-Fi bench (docs/WIFI-BENCH.md): hostapd, dnsmasq (DHCP + DNS), iw, nftables for the
# captive portal's redirect, python3 for hwsim.py and the portal stand-in. Debian's packages: the APs are the world
# around the device, not ROCKNIX.
FROM debian:trixie-slim
RUN apt-get update && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
      hostapd dnsmasq-base iw iproute2 python3 nftables procps rfkill \
    && rm -rf /var/lib/apt/lists/*
