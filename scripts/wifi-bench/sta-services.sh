#!/bin/sh
# The station's network services inside the virtual device (docs/WIFI-BENCH.md): ROCKNIX's own systemd-udevd, iwd,
# NetworkManager and systemd-resolved from the vdev's ROCKNIX 20261001 userland, started with the arguments and
# environment of their systemd units (NetworkManager: --debug for --no-daemon, see below) (/usr/lib/systemd/system/{iwd,NetworkManager}.service). The vdev already runs the
# system D-Bus. Logs: /var/log/{udevd,iwd,NetworkManager,resolved}.log, one line each with the uptime in front, like
# the device's journal.
#
#   sta-services.sh start            udevd (+ trigger), iwd, NetworkManager, resolved
#   sta-services.sh stop|restart NAME   iwd | nm | resolved
#   sta-services.sh status
set -u
LOGDIR=/var/log
CLEAN="env -u LD_LIBRARY_PATH -u LIBGL_DRIVERS_PATH -u __EGL_VENDOR_LIBRARY_FILENAMES"

# Uptime in front of every line, as `journalctl -o short-monotonic` shows it on the device.
stamp() { while IFS= read -r l; do printf '[%s] %s\n' "$(cut -d' ' -f1 /proc/uptime)" "$l"; done; }

run_bg() { # name, command...
  name=$1; shift
  ( "$@" 2>&1 | stamp >>"$LOGDIR/$name.log" ) &
}

pid_of() { # the daemon's own pid (not the pipe's subshell)
  case $1 in
    iwd) pidof iwd ;;
    nm) pidof NetworkManager ;;
    resolved) pidof systemd-resolved ;;
  esac | cut -d' ' -f1
}

start_one() {
  case $1 in
    iwd) run_bg iwd $CLEAN STATE_DIRECTORY=/storage/.cache/iwd /usr/lib/iwd ;;
    # The unit's --no-daemon logs to the journal; the container has none: --debug is the same daemon logging to stderr.
    nm) run_bg NetworkManager $CLEAN /usr/sbin/NetworkManager --debug ;;
    resolved) run_bg resolved $CLEAN /usr/lib/systemd/systemd-resolved ;;
  esac
  # Type=dbus in the units: up once the name is on the bus.
  bus=$(case $1 in iwd) echo net.connman.iwd ;; nm) echo org.freedesktop.NetworkManager ;; resolved) echo org.freedesktop.resolve1 ;; esac)
  i=0
  while [ $i -lt 50 ]; do
    dbus-send --system --dest=org.freedesktop.DBus --print-reply /org/freedesktop/DBus org.freedesktop.DBus.NameHasOwner string:"$bus" 2>/dev/null | grep -q 'boolean true' && return 0
    i=$((i + 1)); sleep 0.1
  done
  echo "sta-services: $1 did not appear on the bus as $bus" >&2
  return 1
}

stop_one() {
  p=$(pid_of "$1")
  [ -n "$p" ] || return 0
  kill "$p" 2>/dev/null
  i=0
  while kill -0 "$p" 2>/dev/null && [ $i -lt 50 ]; do i=$((i + 1)); sleep 0.1; done
  kill -9 "$p" 2>/dev/null
  true
}

case ${1:-} in
  start)
    # /run/systemd for resolved's runtime directory; never /run/systemd/system (pocket-ui would take that for systemd).
    mkdir -p /storage/.cache/iwd /storage/.config/NetworkManager/system-connections /run/systemd
    [ -L /var/run ] || ln -sfn /run /var/run
    # NetworkManager only manages devices that udev has seen (no udev: wlan0 "unmanaged").
    $CLEAN /usr/lib/systemd/systemd-udevd --daemon >>"$LOGDIR/udevd.log" 2>&1
    udevadm trigger --action=add --subsystem-match=net --subsystem-match=ieee80211 --subsystem-match=rfkill
    udevadm settle -t 5
    # The device's interface is wlan0 (pocket-ui and wifictl use that name). iwd deletes a driver's own interface and
    # makes one named wlan<phy index> (wlan0 on the device's phy0; wlan23 for a hwsim radio made after 23 others):
    # for the simulated driver only, iwd keeps the interface the radio came with (the bench's one change to ROCKNIX's
    # /etc/iwd/main.conf; the device's ath11k is not matched by it). The radio arrives as wlan0 or is renamed so.
    grep -q '^\[DriverQuirks\]' /etc/iwd/main.conf || printf '\n# Wi-Fi bench only (docs/WIFI-BENCH.md)\n[DriverQuirks]\nDefaultInterface=mac80211_hwsim\n' >>/etc/iwd/main.conf
    for try in 1 2 3 4 5; do
      for d in /sys/class/net/wlan*; do
        d=${d##*/}
        [ "$d" = wlan0 ] || { ip link set "$d" down; ip link set "$d" name wlan0 && echo "sta-services: $d renamed wlan0 (try $try)"; }
        break
      done
      sleep 0.5
      [ -e /sys/class/net/wlan0 ] && break
    done
    [ -e /sys/class/net/wlan0 ] || { echo "sta-services: no wlan0 ($(ls /sys/class/net))" >&2; exit 1; }
    # The device's /etc/resolv.conf points at resolved's stub (dns=systemd-resolved in NetworkManager.conf).
    echo 'nameserver 127.0.0.53' >/etc/resolv.conf
    start_one resolved || true
    start_one iwd && start_one nm
    ;;
  stop) stop_one "${2:?iwd|nm|resolved}" ;;
  restart) stop_one "${2:?iwd|nm|resolved}"; start_one "$2" ;;
  status)
    for s in iwd nm resolved; do echo "$s: $(pid_of $s || echo -)"; done
    ;;
  *) echo "usage: sta-services.sh start | stop|restart iwd|nm|resolved | status" >&2; exit 2 ;;
esac
