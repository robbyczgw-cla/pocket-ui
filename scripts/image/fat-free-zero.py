#!/usr/bin/env python3
"""Zero (or check) the free clusters of a FAT32 partition inside an image file.

    fat-free-zero.py IMAGE OFFSET_BYTES          zero every free cluster
    fat-free-zero.py IMAGE OFFSET_BYTES --check  exit 1 if a free cluster holds a non-zero byte

Replacing SYSTEM on the FAT partition (scripts/image/build.sh) frees the clusters of ROCKNIX's original SYSTEM without
clearing them: its bytes, including the files the image leaves out on purpose (pins.sh SYSTEM_REMOVE), would ride
along in the image. Only free clusters are touched (FAT entry 0); files, directories and the FAT stay as they are.
"""
import struct
import sys

CHUNK = 64 << 20


def main():
    img, offset = sys.argv[1], int(sys.argv[2])
    check = "--check" in sys.argv[3:]
    with open(img, "rb" if check else "r+b") as f:
        f.seek(offset)
        bpb = f.read(512)
        bps = struct.unpack_from("<H", bpb, 11)[0]
        spc = bpb[13]
        reserved = struct.unpack_from("<H", bpb, 14)[0]
        nfats = bpb[16]
        total = struct.unpack_from("<I", bpb, 32)[0]
        fatsz = struct.unpack_from("<I", bpb, 36)[0]
        if fatsz == 0 or bpb[82:87] != b"FAT32":
            sys.exit("fat-free-zero: not a FAT32 partition")
        data_start = (reserved + nfats * fatsz) * bps
        csize = spc * bps
        nclusters = (total * bps - data_start) // csize
        f.seek(offset + reserved * bps)
        fat = f.read((nclusters + 2) * 4)
        free = 0
        dirty = 0
        n = 2
        while n < nclusters + 2:
            if struct.unpack_from("<I", fat, n * 4)[0] & 0x0FFFFFFF:
                n += 1
                continue
            run = n
            while n < nclusters + 2 and struct.unpack_from("<I", fat, n * 4)[0] & 0x0FFFFFFF == 0:
                n += 1
            free += n - run
            pos = offset + data_start + (run - 2) * csize
            left = (n - run) * csize
            while left:
                size = min(left, CHUNK)
                f.seek(pos)
                if check:
                    if f.read(size).count(0) != size:
                        dirty += 1
                        break
                else:
                    f.write(bytes(size))
                pos += size
                left -= size
        mib = free * csize >> 20
        if check:
            if dirty:
                print(f"fat-free-zero: {dirty} free runs hold data ({mib} MiB free in all)")
                sys.exit(1)
            print(f"fat-free-zero: {free} free clusters ({mib} MiB), all zero")
        else:
            print(f"fat-free-zero: {free} free clusters ({mib} MiB) zeroed")


if __name__ == "__main__":
    main()
