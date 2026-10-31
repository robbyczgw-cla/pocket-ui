#!/usr/bin/env python3
"""Original GB diagnostic: a checkerboard, D-pad scroll, constant ~440 Hz square wave. No game or BIOS bundled."""
import sys
from pathlib import Path
rom=bytearray(32768)
rom[0x100:0x104]=bytes.fromhex('00 c3 50 01')
# Required cartridge-format magic. The program itself is original pocket-ui test code.
rom[0x104:0x134]=bytes.fromhex('ce ed 66 66 cc 0d 00 0b 03 73 00 83 00 0c 00 0d 00 08 11 1f 88 89 00 0e dc cc 6e e6 dd dd d9 99 bb bb 67 63 6e 0e ec cc dd dc 99 9f bb b9 33 3e')
rom[0x134:0x144]=b'POCKET AUDIO'.ljust(16,b'\0')
code=bytearray.fromhex('f3 31 fe ff af e0 40 06 00 21 00 80')
for _ in range(8): code+=bytes.fromhex('3e aa 22 3e 55 22')
for address,value in [(0x47,0xe4),(0x26,0x80),(0x24,0x77),(0x25,0x11),(0x10,0),(0x11,0x80),(0x12,0xf0),(0x13,0xd6),(0x14,0x86),(0x40,0x91)]:code+=bytes([0x3e,value,0xe0,address])
loop=0x150+len(code)
code+=bytes.fromhex('f0 44 fe 90 38 fa 3e 20 e0 00 f0 00 e6 0f fe 0f 28 04 04 78 e0 43 f0 44 fe 90 30 fa')
code+=bytes([0xc3,loop&255,loop>>8]);rom[0x150:0x150+len(code)]=code
rom[0x14d]=(-sum(rom[0x134:0x14d])-25)&255
checksum=sum(rom[:0x14e])+sum(rom[0x150:]);rom[0x14e:0x150]=bytes([checksum>>8&255,checksum&255])
out=Path(sys.argv[1]);out.parent.mkdir(parents=True,exist_ok=True);out.write_bytes(rom)
