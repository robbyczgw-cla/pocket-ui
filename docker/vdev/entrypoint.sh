#!/bin/sh
# PID 1 of a virtual Mini V2 (run with `docker run --init`), or of one of its SM8250 siblings (VDEV_DEVICE: rp5,
# flip2, mini1). Brings up what the real device has under systemd, in the same places: PipeWire + WirePlumber +
# pipewire-pulse (system-wide, as on ROCKNIX), a sway compositor (headless, the device's screen size) and the
# /storage layout. pocket-ui itself is started later
# by `vdev.sh deploy`, through the real deploy/run-pocket-ui.sh.
#
# Ready when /run/vdev/ready exists; progress in /var/log/vdev-boot.log.

export PATH=/vdev/bin:/usr/bin:/usr/sbin
export HOME=/storage
export XDG_RUNTIME_DIR=/var/run/0-runtime-dir
export WAYLAND_DISPLAY=wayland-1
export SWAYSOCK=$XDG_RUNTIME_DIR/sway-ipc.0.sock
export PIPEWIRE_RUNTIME_DIR=/run/pipewire
export DBUS_SESSION_BUS_ADDRESS=unix:path=/run/dbus/system_bus_socket
export GIO_USE_VFS=local

# The system services run on ROCKNIX's own libraries; only clients of GL (pocket-core) get the llvmpipe Mesa.
CLEAN="env -u LD_LIBRARY_PATH -u LIBGL_DRIVERS_PATH -u __EGL_VENDOR_LIBRARY_FILENAMES"
LOG=/var/log/vdev-boot.log
say() { echo "[$(date +%T)] $*" >>$LOG; }
T0=$(date +%s.%N)

# --- /run, /var layout (ROCKNIX's systemd-tmpfiles does this on the device) ---------------------------
mkdir -p /run /var/log /tmp
[ -L /var/run ] || { rmdir /var/run 2>/dev/null; ln -s /run /var/run; }
mkdir -p $XDG_RUNTIME_DIR/pulse /run/pipewire /run/dbus /run/sway /run/vdev /var/lock
chmod 0700 $XDG_RUNTIME_DIR
chmod 1777 /tmp

# A privileged container starts with a copy of the VM's /dev, which holds the input devices of every other
# virtual device on this Docker host. Drop them: this device only sees its own pad.
rm -rf /dev/input

# --- which device (vdev.sh up --device, docs/VIRTUAL-DEVICE.md "Other devices") -------------------------
# The screen as sway shows it, the touch panel's portrait size, and the device tree pocket-ui reads to tell the
# devices apart (src/device.rs): the strings of ROCKNIX's sm8250-retroidpocket-*.dts.
SOC=qcom,sm8250
export VDEV_TOUCH_ROT=90 VDEV_TOUCH=1
case "${VDEV_DEVICE:-miniv2}" in
  odin2mini) SCREEN=1920x1080 PANEL=1080x1920 MODEL="AYN Odin 2 Mini" COMPAT="ayn,odin2mini" LID=0 SOC=qcom,qcs8550; export VDEV_TOUCH_ROT=270 ;;
  odin2) SCREEN=1920x1080 PANEL=1080x1920 MODEL="AYN Odin 2" COMPAT="ayn,odin2" LID=0 SOC=qcom,qcs8550; export VDEV_TOUCH_ROT=270 ;;
  odin2portal) SCREEN=1920x1080 PANEL=1080x1920 MODEL="AYN Odin 2 Portal" COMPAT="ayn,odin2portal" LID=0 SOC=qcom,qcs8550 ;;
  # Allwinner H700 (ROCKNIX's H700 image; docs/DEVICES.md): 640x480, no touch panel. The RG28XX's panel is 480x640 turned
  # 270 degrees, the SP's lid is a gpio-keys SW_LID. Strings from the image's DTBs (sun50i-h700-anbernic-*.dtb).
  rg28xx) SCREEN=640x480 PANEL=480x640 MODEL="Anbernic RG28XX" COMPAT="anbernic,rg28xx" LID=0 SOC=allwinner,sun50i-h700; export VDEV_TOUCH=0 ;;
  rg35xx-sp) SCREEN=640x480 PANEL=640x480 MODEL="Anbernic RG35XX SP" COMPAT="anbernic,rg35xx-sp" LID=1 SOC=allwinner,sun50i-h700; export VDEV_TOUCH=0 ;;
  rp6) SCREEN=1920x1080 PANEL=1080x1920 MODEL="Retroid Pocket 6" COMPAT="retroidpocket,rp6" LID=0 SOC=qcom,qcs8550 ;;
  nova) SCREEN=1280x960 PANEL=1280x960 MODEL="Retroid Pocket Nova" COMPAT="retroidpocket,rpnova" LID=0 SOC=qcom,qcs8550; export VDEV_TOUCH_ROT=0 ;;
  rp5) SCREEN=1920x1080 PANEL=1080x1920 MODEL="Retroid Pocket 5" COMPAT="retroidpocket,rp5" LID=0 ;;
  flip2) SCREEN=1920x1080 PANEL=1080x1920 MODEL="Retroid Pocket Flip2" COMPAT="retroidpocket,rpflip2" LID=1 ;;
  mini1) SCREEN=1280x960 PANEL=960x1280 MODEL="Retroid Pocket Mini" COMPAT="retroidpocket,rpmini" LID=0 ;;
  *) SCREEN=1240x1080 PANEL=1080x1240 MODEL="Retroid Pocket Mini V2" COMPAT="retroidpocket,rpminiv2" LID=0 ;;
esac
export VDEV_SCREEN=$SCREEN VDEV_PANEL=$PANEL VDEV_LID=$LID
# The container sees the Docker VM's /sys: a tmpfs over /sys/firmware (privileged containers may) gets the device
# tree's two files. Where that isn't allowed, POCKETUI_DEVICE in dev.env says the same (below).
DT_OK=0
if mount -t tmpfs -o size=64k vdev-dt /sys/firmware 2>/dev/null; then
  mkdir -p /sys/firmware/devicetree/base
  printf '%s\000' "$MODEL" >/sys/firmware/devicetree/base/model
  printf '%s\000%s\000' "$COMPAT" "$SOC" >/sys/firmware/devicetree/base/compatible
  [ "$SOC" != qcom,qcs8550 ] || printf 'qcom,sm8550\000' >>/sys/firmware/devicetree/base/compatible
  DT_OK=1
fi

# --- /storage ---------------------------------------------------------------------------------------
mkdir -p /storage/.config/pocket-ui /storage/.config/emulationstation /storage/.config/sway \
  /storage/roms/bios /storage/roms/savestates /storage/pocket-ui
# pocket-ui reads ES's systems file from /storage/.config; on the device ES puts it there at first boot.
[ -f /storage/.config/emulationstation/es_systems.cfg ] || \
  cp /usr/config/emulationstation/es_systems.cfg /storage/.config/emulationstation/es_systems.cfg
sed "s/mode --custom 1240x1080@60Hz/mode --custom ${SCREEN}@60Hz/" /vdev/sway-config >/storage/.config/sway/config
if [ $DT_OK = 0 ]; then
  touch /storage/pocket-ui/dev.env
  grep -v '^POCKETUI_DEVICE=' /storage/pocket-ui/dev.env >/tmp/dev.env.$$ 2>/dev/null; mv /tmp/dev.env.$$ /storage/pocket-ui/dev.env
  echo "POCKETUI_DEVICE=${VDEV_DEVICE:-miniv2}" >>/storage/pocket-ui/dev.env
fi
# The VM has no device-specific fbcon. Match the virtual panel coordinates, never the host's console rotation.
touch /storage/pocket-ui/dev.env
grep -v '^POCKETUI_TOUCH_ROTATE=' /storage/pocket-ui/dev.env >/tmp/dev.env.$$ 2>/dev/null; mv /tmp/dev.env.$$ /storage/pocket-ui/dev.env
echo "POCKETUI_TOUCH_ROTATE=$VDEV_TOUCH_ROT" >>/storage/pocket-ui/dev.env
# user pipewire config: a null sink stands in for the speakers
mkdir -p /storage/.config/pipewire/pipewire.conf.d
cp /vdev/pipewire-null-sink.conf /storage/.config/pipewire/pipewire.conf.d/50-vdev-null-sink.conf
# roms: host folder (read-only) -> /storage/roms/<system>/ symlinks, so saves land in the container
if [ -d /mnt/roms ]; then
  python3 -I /vdev/roms-link.py /mnt/roms /storage/roms /storage/.config/emulationstation/es_systems.cfg >>$LOG 2>&1
fi
# a device that has been set up: skip the first-start wizard unless VDEV_ONBOARDING=1
[ -f /storage/.config/pocket-ui/settings.json ] || [ "${VDEV_ONBOARDING:-0}" = 1 ] || \
  echo '{"onboarded": true}' >/storage/.config/pocket-ui/settings.json
say "storage ready"
say "device: ${VDEV_DEVICE:-miniv2} ($MODEL, $SCREEN$( [ $LID = 1 ] && echo ', lid'); device tree $( [ $DT_OK = 1 ] && echo 'in /sys/firmware' || echo 'not mountable, POCKETUI_DEVICE in dev.env'))"

# --- audio: the three services ROCKNIX starts, same arguments ---------------------------------------
mkdir -p /var/run/0-runtime-dir/pulse
$CLEAN /usr/bin/dbus-daemon --system --address=$DBUS_SESSION_BUS_ADDRESS --nofork --nopidfile >>/var/log/dbus.log 2>&1 &
$CLEAN /usr/bin/pipewire >>/var/log/pipewire.log 2>&1 &
for i in $(seq 1 50); do [ -S /run/pipewire/pipewire-0 ] && break; sleep 0.1; done
$CLEAN /usr/bin/wireplumber -p main-systemwide >>/var/log/wireplumber.log 2>&1 &
$CLEAN /usr/bin/pipewire-pulse >>/var/log/pipewire-pulse.log 2>&1 &
say "audio started"

# --- sway, headless ---------------------------------------------------------------------------------
export WLR_BACKENDS=headless
export WLR_RENDERER=pixman
export WLR_LIBINPUT_NO_DEVICES=1
export WLR_HEADLESS_OUTPUTS=1
$CLEAN -u WAYLAND_DISPLAY -u SWAYSOCK /usr/bin/sway -c /storage/.config/sway/config >/var/log/sway.log 2>&1 &
SWAY=$!
# ROCKNIX's sway is built to use these fixed names (wayland-1, sway-ipc.0.sock), as run-pocket-ui.sh expects
for i in $(seq 1 100); do
  [ -S $XDG_RUNTIME_DIR/wayland-1 ] && [ -S $SWAYSOCK ] && break
  sleep 0.1
done
export WAYLAND_DISPLAY=wayland-1
say "sway up"

# --- virtual gamepad (needs /dev/uinput, i.e. a privileged container) ------------------------------
if [ -c /dev/uinput ]; then
  python3 -I /vdev/vdev-pad.py >>/var/log/vdev-pad.log 2>&1 &
  for i in $(seq 1 30); do grep -q ready /var/log/vdev-pad.log 2>/dev/null && break; sleep 0.1; done
  say "pad: $(tail -1 /var/log/vdev-pad.log)"
else
  say "no /dev/uinput: no virtual pad, only the FIFO works"
fi

# keep the shell environment for docker exec sessions that source it
cat >/run/vdev/env <<EOF
export PATH=$PATH
export HOME=$HOME
export XDG_RUNTIME_DIR=$XDG_RUNTIME_DIR
export WAYLAND_DISPLAY=$WAYLAND_DISPLAY
export SWAYSOCK=$SWAYSOCK
export PIPEWIRE_RUNTIME_DIR=$PIPEWIRE_RUNTIME_DIR
export DBUS_SESSION_BUS_ADDRESS=$DBUS_SESSION_BUS_ADDRESS
EOF
T1=$(date +%s.%N)
say "ready after $(echo "$T1 $T0" | awk '{printf "%.1f", $1-$2}') s"
touch /run/vdev/ready

# sway is the display server: if it dies the device is dead, so the container ends with it
wait $SWAY
