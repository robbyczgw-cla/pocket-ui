#!/usr/bin/env python3
"""Rebuild a ROCKNIX SM8250 KERNEL (Android boot image v0) with a single appended DTB.

The stock Retroid ABL picks the first appended DTB whose qcom,msm-id/board-id match.
All Retroid SM8250 DTBs share the same ids, so the stock ABL would boot the Flip2 tree.
Keeping only the requested model's DTB makes `fastboot boot` use the right one.

Usage: make-test-bootimg.py KERNEL OUT.img "Retroid Pocket Mini V2" [override.dtb] [cmdline]
Writes the model's original DTB to OUT.img.orig.dtb; an override DTB (same model) replaces it.
Pass "" as override.dtb to keep the original DTB while replacing the cmdline.
"""
import hashlib
import re
import struct
import sys


def fdt_model(blob):
    _, _, off_struct, off_strings = struct.unpack(">IIII", blob[:16])
    p, depth = off_struct, 0
    while True:
        tok = struct.unpack(">I", blob[p:p + 4])[0]
        p += 4
        if tok == 1:
            end = blob.index(b"\0", p)
            p = (end + 4) & ~3
            depth += 1
        elif tok == 2:
            depth -= 1
        elif tok == 3:
            length, name_off = struct.unpack(">II", blob[p:p + 8])
            p += 8
            value = blob[p:p + length]
            p = (p + length + 3) & ~3
            name = blob[off_strings:blob.index(b"\0", off_strings + name_off)][name_off:]
            if depth == 1 and name == b"model":
                return value.rstrip(b"\0").decode()
        elif tok == 9:
            return None


def main(src, dst, want, dtb_override=None, cmdline=None):
    data = open(src, "rb").read()
    assert data[:8] == b"ANDROID!", "not an Android boot image"
    (k_size, k_addr, r_size, r_addr, s_size, s_addr, tags, page) = struct.unpack("<8I", data[8:40])
    header_version = struct.unpack("<I", data[40:44])[0]
    assert header_version == 0, f"unsupported header version {header_version}"

    pages = lambda n: (n + page - 1) // page * page
    kernel = data[page:page + k_size]
    ramdisk = data[page + pages(k_size):page + pages(k_size) + r_size]
    second = data[page + pages(k_size) + pages(r_size):][:s_size]

    dtbs = [m.start() for m in re.finditer(b"\xd0\x0d\xfe\xed", kernel)]
    # Only real FDTs: a valid header has totalsize pointing inside the blob and version 17.
    trees = []
    for off in dtbs:
        total, = struct.unpack(">I", kernel[off + 4:off + 8])
        if off + total <= len(kernel) and struct.unpack(">I", kernel[off + 20:off + 24])[0] == 17:
            trees.append((off, total, fdt_model(kernel[off:off + total])))
    image_end = trees[0][0]
    picked = [t for t in trees if t[2] == want]
    assert len(picked) == 1, f"model {want!r} not found exactly once: {[t[2] for t in trees]}"
    off, total, _ = picked[0]
    dtb = kernel[off:off + total]
    open(dst + ".orig.dtb", "wb").write(dtb)
    if dtb_override:  # "" keeps the original
        dtb = open(dtb_override, "rb").read()
        assert fdt_model(dtb) == want, "override DTB has a different model"

    new_kernel = kernel[:image_end] + dtb

    sha = hashlib.sha1()
    for blob in (new_kernel, ramdisk, second):
        sha.update(blob)
        sha.update(struct.pack("<I", len(blob)))
    header = bytearray(data[:page])
    struct.pack_into("<I", header, 8, len(new_kernel))
    if cmdline is not None:
        raw = cmdline.encode()
        assert len(raw) < 512, "cmdline too long for header v0"
        header[64:64 + 512] = raw.ljust(512, b"\0")
    print("cmdline:", bytes(header[64:576]).split(b"\0")[0].decode())
    header[576:576 + 32] = sha.digest().ljust(32, b"\0")

    pad = lambda b: b + b"\0" * (pages(len(b)) - len(b))
    out = bytes(header) + pad(new_kernel) + pad(ramdisk) + (pad(second) if second else b"")
    open(dst, "wb").write(out)
    print(f"kept DTB {want!r} ({len(dtb)} bytes) of {len(trees)}; kernel {len(new_kernel)} bytes -> {dst}")


if __name__ == "__main__":
    main(*sys.argv[1:6])
