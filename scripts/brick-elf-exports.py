#!/usr/bin/env python3
"""Checks that an ELF program defines and exports the named functions in its dynamic symbol table.

  python3 -I scripts/brick-elf-exports.py FILE NAME... [--not PREFIX...]
  python3 -I scripts/brick-elf-exports.py --stand-in FILE NAME...   (writes a minimal ELF exporting NAME..., for tests)

Exit 0 when every NAME is a defined global/weak function in .dynsym and no exported function starts with a PREFIX after --not,
else 1 with the names.
The Brick's pocket-core must export eglGetProcAddress (build.rs): PPSSPP asks EGL by name for its GLES 3 functions,
and the vendor's EGL 1.4 answers NULL (every PSP game crashed at its start, 20.11.2026)."""
import struct
import sys


def exports(data):
    if data[:4] != b'\x7fELF' or data[4] != 2 or data[5] != 1:
        raise ValueError('not a 64-bit little-endian ELF file')
    shoff, = struct.unpack_from('<Q', data, 0x28)
    shentsize, shnum = struct.unpack_from('<HH', data, 0x3a)
    sections = [struct.unpack_from('<IIQQQQIIQQ', data, shoff + i * shentsize) for i in range(shnum)]
    names = set()
    for _name, kind, _flags, _addr, off, size, link, _info, _align, entsize in sections:
        if kind != 11:  # SHT_DYNSYM
            continue
        stroff = sections[link][4]
        for i in range(size // (entsize or 24)):
            st_name, st_info, _other, st_shndx, _value, _size = struct.unpack_from('<IBBHQQ', data, off + i * 24)
            if st_shndx == 0 or st_info & 0xf != 2 or st_info >> 4 not in (1, 2):  # defined, FUNC, GLOBAL/WEAK
                continue
            end = data.index(b'\0', stroff + st_name)
            names.add(data[stroff + st_name:end].decode())
    return names


def stand_in(names):
    """A minimal ELF64 file (no code) whose .dynsym defines `names` as global functions."""
    strtab = b'\0' + b''.join(n.encode() + b'\0' for n in names)
    syms, at = [b'\0' * 24], 1
    for n in names:
        syms.append(struct.pack('<IBBHQQ', at, 0x12, 0, 1, 0x1000, 4))
        at += len(n) + 1
    dynsym = b''.join(syms)
    body_off = 64
    dynstr_off, dynsym_off = body_off, body_off + len(strtab)
    shoff = dynsym_off + len(dynsym)
    shdrs = [b'\0' * 64,
             struct.pack('<IIQQQQIIQQ', 0, 3, 2, 0, dynstr_off, len(strtab), 0, 0, 1, 0),
             struct.pack('<IIQQQQIIQQ', 0, 11, 2, 0, dynsym_off, len(dynsym), 1, 1, 8, 24)]
    header = b'\x7fELF' + bytes([2, 1, 1]) + b'\0' * 9 + struct.pack('<HHIQQQIHHHHHH', 3, 183, 1, 0, 0, shoff, 0, 64, 0, 0, 64, 3, 0)
    return header + strtab + dynsym + b''.join(shdrs)


def main(argv):
    if len(argv) >= 4 and argv[1] == '--stand-in':
        open(argv[2], 'wb').write(stand_in(argv[3:]))
        return 0
    if len(argv) < 3:
        print(__doc__.strip(), file=sys.stderr)
        return 2
    try:
        have = exports(open(argv[1], 'rb').read())
    except (OSError, ValueError, struct.error, IndexError) as e:
        print(f'{argv[1]}: {e}', file=sys.stderr)
        return 1
    args = argv[2:]
    wanted, forbidden = (args[:args.index('--not')], args[args.index('--not') + 1:]) if '--not' in args else (args, [])
    missing = [n for n in wanted if n not in have]
    if missing:
        print(f'{argv[1]} does not export: {" ".join(missing)}', file=sys.stderr)
        return 1
    leaked = sorted(n for n in have if any(n.startswith(p) for p in forbidden))
    if leaked:
        print(f'{argv[1]} exports {len(leaked)} names a core may have too: {" ".join(leaked[:8])}', file=sys.stderr)
        return 1
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
