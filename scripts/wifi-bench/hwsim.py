#!/usr/bin/env python3
"""Creates and removes mac80211_hwsim radios over generic netlink (no hwsim_mgmt needed), for the Wi-Fi bench
(docs/WIFI-BENCH.md).

    python3 -I hwsim.py new <name> [<name> ...]   one radio per name; prints "<name> <radio id>"
    python3 -I hwsim.py del <name> [<name> ...]   removes them (a missing one is not an error)

A radio is created in the network namespace of the caller, and only radios created in the same namespace hear each
other (hwsim's "netgroup"): the bench creates all of them in the access point container and moves the station's phy
into the station container afterwards (`iw phy <name> set netns <pid>`), which keeps its netgroup.
The kernel module must be loaded with `radios=0` first. Standard library only.
"""
import os
import socket
import struct
import sys

NETLINK_GENERIC = 16
GENL_ID_CTRL = 0x10
CTRL_CMD_GETFAMILY = 3
CTRL_ATTR_FAMILY_ID = 1
CTRL_ATTR_FAMILY_NAME = 2
NLM_F_REQUEST = 1
NLM_F_ACK = 4
NLMSG_ERROR = 2

HWSIM_CMD_NEW_RADIO = 4
HWSIM_CMD_DEL_RADIO = 5
HWSIM_ATTR_CHANNELS = 9
HWSIM_ATTR_RADIO_NAME = 17


def attr(kind, data):
    a = struct.pack("HH", 4 + len(data), kind) + data
    return a + b"\0" * (-len(a) % 4)


class Genl:
    def __init__(self):
        self.sock = socket.socket(socket.AF_NETLINK, socket.SOCK_RAW, NETLINK_GENERIC)
        self.sock.bind((0, 0))
        self.seq = 0

    def request(self, family, cmd, attrs):
        """Sends one request with an ack; returns (error code, reply payloads). hwsim answers NEW_RADIO with the new
        radio's id in the ack's error field (a positive number)."""
        self.seq += 1
        body = struct.pack("BBH", cmd, 1, 0) + b"".join(attrs)
        self.sock.send(struct.pack("IHHII", 16 + len(body), family, NLM_F_REQUEST | NLM_F_ACK, self.seq, os.getpid()) + body)
        replies = []
        while True:
            data = self.sock.recv(65536)
            off = 0
            while off + 16 <= len(data):
                length, kind, _, seq, _ = struct.unpack_from("IHHII", data, off)
                if seq == self.seq:
                    if kind == NLMSG_ERROR:
                        return struct.unpack_from("i", data, off + 16)[0], replies
                    replies.append(data[off + 16 + 4:off + length])
                off += (length + 3) & ~3

    def family(self, name):
        err, replies = self.request(GENL_ID_CTRL, CTRL_CMD_GETFAMILY, [attr(CTRL_ATTR_FAMILY_NAME, name.encode() + b"\0")])
        for payload in replies:
            off = 0
            while off + 4 <= len(payload):
                alen, akind = struct.unpack_from("HH", payload, off)
                if akind == CTRL_ATTR_FAMILY_ID:
                    return struct.unpack_from("H", payload, off + 4)[0]
                off += (alen + 3) & ~3
        sys.exit(f"hwsim: no generic netlink family {name} (error {err}); is mac80211_hwsim loaded?")


def main():
    if len(sys.argv) < 3 or sys.argv[1] not in ("new", "del"):
        sys.exit(__doc__)
    nl = Genl()
    fam = nl.family("MAC80211_HWSIM")
    status = 0
    for name in sys.argv[2:]:
        if sys.argv[1] == "new":
            err, _ = nl.request(fam, HWSIM_CMD_NEW_RADIO, [attr(HWSIM_ATTR_RADIO_NAME, name.encode()), attr(HWSIM_ATTR_CHANNELS, struct.pack("I", 1))])
            if err < 0:
                print(f"hwsim: new {name}: {os.strerror(-err)}", file=sys.stderr)
                status = 1
            else:
                print(name, err)
        else:
            err, _ = nl.request(fam, HWSIM_CMD_DEL_RADIO, [attr(HWSIM_ATTR_RADIO_NAME, name.encode())])
            if err < 0 and -err != 19:  # ENODEV: already gone
                print(f"hwsim: del {name}: {os.strerror(-err)}", file=sys.stderr)
                status = 1
    sys.exit(status)


if __name__ == "__main__":
    main()
