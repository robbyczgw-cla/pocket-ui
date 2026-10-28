# Security

pocket-ui runs as root on a handheld that can join Wi-Fi networks and a tailnet, share files over Samba, run an SSH
server and a web companion. If you find a way to get into a device, read its files or take over its updates, please
tell us privately before anyone else.

## How to report

Use GitHub's private reporting: the **Security** tab of this repository → **Report a vulnerability**. Only the
maintainers see the report. Please include:

- what an attacker needs (same Wi-Fi, same tailnet, physical access, a crafted file on the card, ...),
- the steps to reproduce, and the pocket-ui and ROCKNIX versions,
- what you think the impact is.

Do not open a public issue for a security problem. We will answer within a week, keep you posted on the fix,
and credit you in the release notes if you want.

## What is in scope

- pocket-ui and pocket-core (this repository): the web companion, file sharing and SSH settings, the update and
  Store signatures, the pairing of devices, anything that reads files from the card or the network.
- How pocket-ui configures ROCKNIX's services (Samba, SSH, Syncthing, Tailscale).

Problems in ROCKNIX itself, in RetroArch or in an emulator belong to those projects; we are glad to help pass a
report on. The design and the hardening that is in place are described in
[docs/SECURITY-HARDENING.md](docs/SECURITY-HARDENING.md).

## Supported versions

pocket-ui is early and has no maintained release branches. Fixes go into the newest version.
