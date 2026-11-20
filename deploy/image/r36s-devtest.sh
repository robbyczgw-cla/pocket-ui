#!/bin/sh
# pocket-ui R36S DEVTEST card only (make-image-family.sh, model r36s-devtest; never handed out): the handheld stays reachable from
# the build machine over a USB-C cable. The image seeds /storage/.cache/usbgadget/usbgadget.conf (USB_MODE=cdc, so ROCKNIX's
# 081-usbgadget starts its USB network gadget: the handheld is 169.254.170.2, its udhcpd gives the computer .1),
# /storage/.ssh/authorized_keys (the build machine's public key) and pocket-ui's dev-ssh marker; this file, run by ROCKNIX's
# autostart at every start, keeps SSH and the gadget on: ssh.enabled=1 (ROCKNIX starts sshd at every boot from then on) and
# sshd now, the gadget again if something switched it off. The R36S has one USB port: while it is a gadget, a USB Wi-Fi
# adapter on it does not work (ROCKNIX's usbgadget refuses to switch while a device is plugged in).
. /etc/profile.d/001-functions
[ "$(get_setting ssh.enabled)" = 1 ] || set_setting ssh.enabled 1
mkdir -p /storage/.cache/services && touch /storage/.cache/services/sshd.conf
systemctl is-active --quiet sshd || systemctl start sshd
[ "$(usbgadget 2>/dev/null)" = network ] || usbgadget network >/dev/null 2>&1
true
