#!/usr/bin/env python3
"""Original, silent moving-shape carts for the Brick compatibility probe (not games)."""
from pathlib import Path
import sys
import struct

sys.path.insert(0, str(Path(__file__).resolve().parent))
from testroms import megadrive

out = Path(sys.argv[1])
out.mkdir(parents=True, exist_ok=True)
(out / 'pico-test.p8').write_text(
    'pico-8 cartridge // http://www.pico-8.com\nversion 42\n__lua__\n'
    't=0\nfunction _update() t=t+1 end\n'
    'function _draw() cls(1) rectfill(t%100,20,t%100+20,40,8) end\n'
)
code = b'-- script: lua\nt=0\nfunction TIC() t=t+1 cls(1) rect(t%200,20,20,20,8) end\n'
# TIC-80 code chunk: bank 0, type 5, little-endian 16-bit size, reserved byte.
(out / 'tic-test.tic').write_bytes(bytes([5, len(code) & 255, len(code) >> 8, 0]) + code)
# DOS COM: mode 13h, fill A000:0000 with a colour, spin. No disk or sound access.
(out / 'dos-test.com').write_bytes(bytes.fromhex('b81300cd10b800a08ec031ffb9007db80303fcf3abebfe'))

# Original 32X bootstrap: MD paints its backdrop, enables the adapter; both SH2s
# branch to themselves in SDRAM. Exercises the SH2 JIT, not 32X game speed/video.
rom = bytearray(megadrive())
rom[0x100:0x110] = b"SEGA 32X        "
code_end = rom.find(bytes.fromhex('60fe'), 0x200)
rom[code_end:code_end + 10] = bytes.fromhex('33fc000300a1510060fe')
struct.pack_into('>IIIIIII', rom, 0x3d4, 0x400, 0, 4, 0x06000000, 0x06000000, 0, 0)
rom[0x400:0x404] = bytes.fromhex('affe0009')
(out / '32x-test.32x').write_bytes(rom)
