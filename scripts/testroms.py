#!/usr/bin/env python3
"""Writes tiny self-made test ROMs for systems we have no legal game for, so pocket-core can be
checked on the device (picture, sound, input, save states, frame pacing):

    testroms.py OUTDIR      -> sms-test.sms, gg-test.gg, sg-test.sg, a2600-test.a26, ws-test.ws, fds-test.fds,
                               fds-bios-standin.rom, vb-test.vb, snes-test.sfc, gba-test.gba,
                               naomi-bios-standin.zip, naomi-test.bin

Every ROM draws colour bars and plays a steady tone. Holding a button moves the picture (Master System / Game
Gear: button 2 scrolls the bars for good, button 1 shifts them vertically while held; SG-1000: button 1 changes the
backdrop; Atari 2600: fire moves the gradient), so a screenshot can tell whether input arrived and, after a state
is loaded, whether the picture went back. WonderSwan: the bars scroll by themselves. Famicom Disk System: an empty
two-sided disk and a stand-in for disksys.rom (our own program, for the virtual device; it doesn't boot real disks), see
`fds_bios`. Virtual Boy: bars in both eyes' frame buffers, A moves them. SNES and GBA: a red screen, to start a game
from the second folders of those consoles (sfc, gbah). NAOMI: a stand-in for naomi.zip (`naomi_bios`) and an empty
cartridge. They are original, not games.
"""
import struct
import sys
from pathlib import Path


class Asm:
    """Just enough of an assembler: bytes, labels, and 16-bit / relative references."""

    def __init__(self, origin=0):
        self.origin, self.out, self.labels, self.fix = origin, bytearray(), {}, []

    def label(self, name):
        self.labels[name] = self.origin + len(self.out)

    def b(self, *vals):
        self.out += bytes(v & 0xFF for v in vals)

    def w(self, val):
        self.out += struct.pack("<H", val & 0xFFFF)

    def ref16(self, name):
        self.fix.append((len(self.out), name, "abs"))
        self.w(0)

    def ref8(self, name):
        self.fix.append((len(self.out), name, "rel"))
        self.b(0)

    def pad_to(self, addr, fill=0):
        self.out += bytes([fill]) * (addr - self.origin - len(self.out))

    def done(self):
        for pos, name, kind in self.fix:
            target = self.labels[name]
            if kind == "abs":
                self.out[pos : pos + 2] = struct.pack("<H", target)
            else:
                off = target - (self.origin + pos + 1)
                assert -128 <= off <= 127, f"{name}: branch too far ({off})"
                self.out[pos] = off & 0xFF
        return bytes(self.out)


SCROLL = 0xC000  # a byte of the Z80's RAM


# ---------------------------------------------------------------- Sega Master System / Game Gear / SG-1000
def sega(kind):
    """kind: 'sms', 'gg' or 'sg'. Z80 program; VDP on ports 0xBE/0xBF, PSG on 0x7F, pad on 0xDC."""
    a = Asm()
    a.b(0xF3)                    # di
    a.b(0xED, 0x56)              # im 1
    a.b(0x31); a.w(0xDFF0)       # ld sp,$dff0
    a.b(0xC3); a.ref16("main")   # jp main
    a.pad_to(0x38); a.b(0xFB, 0xED, 0x4D)   # ei; reti
    a.pad_to(0x66); a.b(0xED, 0x45)         # retn

    def vdp_reg(reg, val):
        a.b(0x3E, val, 0xD3, 0xBF, 0x3E, 0x80 | reg, 0xD3, 0xBF)

    a.label("main")
    if kind == "sg":
        # TMS9918 Graphics I: names 0x1400, colours 0x2000, patterns 0x0000.
        for reg, val in [(0, 0x00), (1, 0xC0), (2, 0x05), (3, 0x80), (4, 0x00), (5, 0x76), (6, 0x03), (7, 0x04)]:
            vdp_reg(reg, val)
    else:
        for reg, val in [(0, 0x04), (1, 0x80), (2, 0xFF), (3, 0xFF), (4, 0xFF), (5, 0xFF), (6, 0xFF), (7, 0x00), (8, 0x00), (9, 0x00), (10, 0xFF)]:
            vdp_reg(reg, val)

    def set_vram(addr):
        a.b(0x3E, addr & 0xFF, 0xD3, 0xBF, 0x3E, 0x40 | (addr >> 8), 0xD3, 0xBF)

    # clear VRAM
    set_vram(0)
    a.b(0x21); a.w(0x4000)                  # ld hl,$4000
    a.label("clr")
    a.b(0xAF, 0xD3, 0xBE, 0x2B, 0x7C, 0xB5)  # xor a; out (be),a; dec hl; ld a,h; or l
    a.b(0x20); a.ref8("clr")                # jr nz,clr

    if kind == "sg":
        # patterns 0, 8, 16, 24 solid; colour table: group n gets its own colour
        for g in range(4):
            set_vram(g * 64)
            a.b(0x06, 8, 0x0E, 0xBE, 0x21); a.ref16(f"solid"); a.b(0xED, 0xB3)   # ld b,8; ld c,be; ld hl,solid; otir
        set_vram(0x2000)
        a.b(0x06, 4, 0x0E, 0xBE, 0x21); a.ref16("sgcol"); a.b(0xED, 0xB3)
        # names: 768 entries, ((x >> 2) & 3) * 8
        set_vram(0x1400)
    else:
        # palette: 16 colours
        a.b(0x3E, 0x00, 0xD3, 0xBF, 0x3E, 0xC0, 0xD3, 0xBF)    # CRAM address 0
        if kind == "gg":
            a.b(0x06, 64, 0x0E, 0xBE, 0x21); a.ref16("ggpal"); a.b(0xED, 0xB3)
        else:
            a.b(0x06, 32, 0x0E, 0xBE, 0x21); a.ref16("smspal"); a.b(0xED, 0xB3)
        # tiles 0..3 (32 bytes each) at VRAM 0
        set_vram(0)
        a.b(0x06, 128, 0x0E, 0xBE, 0x21); a.ref16("tiles"); a.b(0xED, 0xB3)
        set_vram(0x3800)
    # name table: 768 entries
    a.b(0x21); a.w(0)                       # ld hl,0
    a.b(0x01); a.w(768)                     # ld bc,768
    a.label("names")
    a.b(0x7D, 0x0F, 0x0F, 0xE6, 0x03)       # ld a,l; rrca; rrca; and 3
    if kind == "sg":
        a.b(0x87, 0x87, 0x87)               # add a,a x3  (* 8)
    a.b(0xD3, 0xBE)                         # out (be),a
    if kind != "sg":
        a.b(0xAF, 0xD3, 0xBE)               # xor a; out (be),a  (high byte of the entry)
    a.b(0x23, 0x0B, 0x78, 0xB1)             # inc hl; dec bc; ld a,b; or c
    a.b(0x20); a.ref8("names")

    vdp_reg(1, 0xC0)                        # display on
    # PSG: tone channel 0 about 440 Hz, volume 3
    a.b(0x3E, 0x8E, 0xD3, 0x7F, 0x3E, 0x0F, 0xD3, 0x7F, 0x3E, 0x93, 0xD3, 0x7F)

    # main loop: wait for vblank, scroll, read button 1 -> border colour
    a.label("frame")
    a.b(0xDB, 0xBF)                         # in a,(bf)   status; bit 7 = vblank
    a.b(0xCB, 0x7F)                         # bit 7,a
    a.b(0x28); a.ref8("frame")              # jr z,frame
    a.b(0xDB, 0xDC)                         # in a,(dc)   pad: bit 4 = button 1, bit 5 = button 2 (0 = pressed)
    if kind != "sg":
        a.b(0x47)                           # ld b,a
        a.b(0xCB, 0x68)                     # bit 5,b
        a.b(0x20); a.ref8("noscr")          # jr nz,noscr   (the bars scroll while button 2 is held)
        a.b(0x3A); a.w(SCROLL)              # ld a,(scroll)
        a.b(0x3C)                           # inc a
        a.b(0x32); a.w(SCROLL)              # ld (scroll),a
        a.label("noscr")
        a.b(0x3A); a.w(SCROLL)              # ld a,(scroll)
        a.b(0xD3, 0xBF, 0x3E, 0x88, 0xD3, 0xBF)   # out (bf),a ; ld a,$88 ; out (bf),a   (reg 8 = hscroll)
        a.b(0x78)                           # ld a,b
    a.b(0xE6, 0x10)                         # and $10
    a.b(0x20); a.ref8("up")                 # jr nz,up
    a.b(0x3E, 0x30 if kind != "sg" else 0x0A)   # pressed: picture shifts (SMS/GG) / backdrop colour (SG)
    a.b(0x18); a.ref8("setb")
    a.label("up")
    a.b(0x3E, 0x00 if kind != "sg" else 0x04)
    a.label("setb")
    # SMS/GG: reg 9 = vertical scroll, SG: reg 7 = backdrop
    a.b(0xD3, 0xBF, 0x3E, 0x89 if kind != "sg" else 0x87, 0xD3, 0xBF)
    a.b(0x18); a.ref8("frame")

    a.label("solid"); a.b(*([0xFF] * 8))
    a.label("sgcol"); a.b(0x81, 0x31, 0x51, 0xB1)   # red, green, blue, yellow on black (fg<<4 | bg)
    a.label("smspal")
    for _ in range(2):
        a.b(0x00, 0x30, 0x0C, 0x03, 0x0F, 0x3C, 0x33, 0x2A, 0, 0, 0, 0x3F, 0, 0, 0, 0)
    a.label("ggpal")
    for _ in range(2):
        for c in [0x000, 0xF00, 0x0F0, 0x00F, 0x0FF, 0xFF0, 0xF0F, 0x888, 0, 0, 0, 0xFFF, 0, 0, 0, 0]:
            a.b(c & 0xFF, c >> 8)
    a.label("tiles")
    # tile 0 blank; tile 1 colour 1; tile 2 colour 2; tile 3 colour 3 (4 bytes per row = bit planes)
    for planes in [(0, 0, 0, 0), (0xFF, 0, 0, 0), (0, 0xFF, 0, 0), (0xFF, 0xFF, 0, 0)]:
        for _ in range(8):
            a.b(*planes)
    rom = a.done()
    return rom


def sega_rom(kind):
    code = sega(kind)
    rom = bytearray(0x8000)
    rom[: len(code)] = code
    # header at $7FF0: "TMR SEGA", region + size nibble (SMS export 4, Game Gear international 6; 32 KB = $C)
    if kind != "sg":
        rom[0x7FF0:0x7FF8] = b"TMR SEGA"
        rom[0x7FFF] = {"sms": 0x4C, "gg": 0x6C}[kind]
    return bytes(rom)


# ---------------------------------------------------------------- Atari 2600
def a2600():
    """4 KB cartridge: a colour gradient that moves while fire is held, and a steady tone."""
    a = Asm(0xF000)
    ops = a

    def lda_i(v): ops.b(0xA9, v)
    def sta_z(z): ops.b(0x85, z)
    def ldx_i(v): ops.b(0xA2, v)
    VSYNC, VBLANK, WSYNC, COLUBK, AUDC0, AUDF0, AUDV0, INPT4 = 0x00, 0x01, 0x02, 0x09, 0x15, 0x17, 0x19, 0x0C
    FRAME = 0x80
    a.label("reset")
    a.b(0x78, 0xD8)                         # sei; cld
    ldx_i(0); a.b(0x8A)                     # ldx #0; txa
    a.label("clear")
    a.b(0xCA, 0x9A, 0x48)                   # dex; txs; pha
    a.b(0xD0); a.ref8("clear")              # bne
    lda_i(4); sta_z(AUDC0)                  # tone type
    lda_i(10); sta_z(AUDV0)                 # volume
    lda_i(12); sta_z(AUDF0)                 # pitch
    a.label("frame")
    lda_i(2); sta_z(VBLANK); sta_z(VSYNC)
    sta_z(WSYNC); sta_z(WSYNC); sta_z(WSYNC)
    lda_i(0); sta_z(VSYNC)
    ldx_i(37)
    a.label("vb")
    sta_z(WSYNC); a.b(0xCA)                 # dex
    a.b(0xD0); a.ref8("vb")
    lda_i(0); sta_z(VBLANK)
    # fire button (INPT4 bit 7 = 0 when pressed): colour offset changes
    a.b(0xA5, INPT4)                        # lda INPT4
    a.b(0x30); a.ref8("nofire")             # bmi nofire
    a.b(0xE6, FRAME)                        # inc FRAME: the gradient moves while fire is held
    a.label("nofire")
    ldx_i(192)
    a.label("pic")
    sta_z(WSYNC)
    a.b(0x8A, 0x18, 0x65, FRAME)            # txa; clc; adc FRAME
    sta_z(COLUBK)
    a.b(0xCA)                               # dex
    a.b(0xD0); a.ref8("pic")
    lda_i(2); sta_z(VBLANK)
    ldx_i(30)
    a.label("os")
    sta_z(WSYNC); a.b(0xCA)
    a.b(0xD0); a.ref8("os")
    a.b(0x4C); a.ref16("frame")             # jmp frame
    code = a.done()
    rom = bytearray(0x1000)
    rom[: len(code)] = code
    rom[0xFFC:0xFFE] = struct.pack("<H", 0xF000)
    rom[0xFFE:0x1000] = struct.pack("<H", 0xF000)
    return bytes(rom)


# ---------------------------------------------------------------- WonderSwan (monochrome)
def wonderswan():
    """128 KB ROM: x86 code in the last 64 KB bank, scrolling bars, a square wave on channel 1."""
    a = Asm()
    a.b(0xFA, 0xFC, 0x31, 0xC0, 0x8E, 0xD8, 0x8E, 0xC0, 0x8E, 0xD0)       # cli; cld; xor ax,ax; mov ds,ax; mov es,ax; mov ss,ax
    a.b(0xBC, 0x00, 0x20)                                                  # mov sp,0x2000
    for port, val in [(0x1C, 0x50), (0x1D, 0xFA), (0x1E, 0x00), (0x1F, 0x00)]:   # LCD shade pool
        a.b(0xB0, val, 0xE6, port)
    a.b(0xB8, 0x10, 0x32, 0xE7, 0x20)                                      # palette 0: colours 0..3 = shades 0..3
    a.b(0xBF, 0x10, 0x20)                                                  # mov di,0x2010   (tile 1)
    for word in (0x00FF, 0xFF00, 0xFFFF):                                  # tiles 1..3: one colour each (two bit planes)
        a.b(0xB9, 0x08, 0x00, 0xB8, word & 0xFF, word >> 8, 0xF3, 0xAB)     # mov cx,8; mov ax,word; rep stosw
    a.b(0xBF, 0x00, 0x08, 0x31, 0xDB, 0xB9, 0x00, 0x04)                    # mov di,0x800; xor bx,bx; mov cx,1024
    a.label("map")
    a.b(0x89, 0xD8, 0xD1, 0xE8, 0xD1, 0xE8, 0x25, 0x03, 0x00, 0xAB, 0x43)  # mov ax,bx; shr ax,1 x2; and ax,3; stosw; inc bx
    a.b(0xE2); a.ref8("map")                                               # loop map
    a.b(0xB0, 0x01, 0xE6, 0x07)                                            # screen 1 map at 0x800
    a.b(0xBF, 0x00, 0x04, 0xB9, 0x04, 0x00, 0xB8, 0xFF, 0xFF, 0xF3, 0xAB)  # wave table at 0x400: 8 bytes high ...
    a.b(0xB9, 0x04, 0x00, 0x31, 0xC0, 0xF3, 0xAB)                          # ... 8 bytes low
    a.b(0xB0, 0x10, 0xE6, 0x8F)                                            # wave base = 0x400 >> 6
    a.b(0xB8, 0x26, 0x07, 0xE7, 0x80)                                      # channel 1 frequency (about 440 Hz)
    a.b(0xB0, 0xFF, 0xE6, 0x88, 0xB0, 0x01, 0xE6, 0x90, 0xB0, 0x0F, 0xE6, 0x91)   # volume, channel 1 on, output on
    a.b(0xB0, 0x01, 0xE6, 0x00)                                            # screen 1 on
    a.label("frame")
    a.b(0xE4, 0x02, 0x3C, 0x90)                                            # in al,2 (line); cmp al,144
    a.b(0x75); a.ref8("frame")                                             # jne frame
    a.b(0xFE, 0xC2, 0x88, 0xD0, 0xE6, 0x10)                                # inc dl; mov al,dl; out 0x10,al (scroll x)
    a.label("leave")
    a.b(0xE4, 0x02, 0x3C, 0x90)
    a.b(0x74); a.ref8("leave")                                             # je leave
    a.b(0xEB); a.ref8("frame")
    code = a.done()
    rom = bytearray(0x20000)
    rom[0x10000 : 0x10000 + len(code)] = code
    # footer: jmp far F000:0000, then publisher, mono, game id, version, size, save type, flags, RTC, checksum
    rom[0x1FFF0:0x20000] = bytes([0xEA, 0x00, 0x00, 0x00, 0xF0, 0x00, 0x01, 0x00, 0x01, 0x00, 0x01, 0x00, 0x04, 0x00, 0x00, 0x00])
    return bytes(rom)


# ---------------------------------------------------------------- Famicom Disk System
def fds_bios():
    """A STAND-IN for disksys.rom (8 KB at $E000), for the virtual device only: our own 6502 program, not Nintendo's
    BIOS, so it boots no real disk. Nestopia runs it like the BIOS ("unknown BIOS ROM"), so the FDS path of the
    emulator (the disk image loaded, the drive, the BIOS in roms/bios) is exercised: colour bars, a steady tone, the
    bars move while A is held, the backdrop is red while B is held and green while no disk is in the drive (read
    from the drive's status register $4032, so L1's "eject / change side" shows on screen)."""
    a = Asm(0xE000)

    def sta(addr): a.b(0x8D, addr & 0xFF, addr >> 8)
    def lda_i(v): a.b(0xA9, v)
    def lda(addr): a.b(0xAD, addr & 0xFF, addr >> 8)

    a.label("reset")
    a.b(0x78, 0xD8, 0xA2, 0xFF, 0x9A)       # sei; cld; ldx #$ff; txs
    lda_i(0); sta(0x2000); sta(0x2001)
    lda_i(0x83); sta(0x4023)                # disk and sound registers on
    for n in (1, 2):                        # two vblanks: the PPU is ready
        a.label(f"vw{n}")
        a.b(0x2C, 0x02, 0x20)               # bit $2002
        a.b(0x10); a.ref8(f"vw{n}")         # bpl
    lda_i(0x3F); sta(0x2006); lda_i(0); sta(0x2006)
    a.b(0xA2, 0)                            # ldx #0
    a.label("pal")
    a.b(0xBD); a.ref16("paltab"); sta(0x2007); a.b(0xE8, 0xE0, 32)   # lda paltab,x; sta $2007; inx; cpx #32
    a.b(0xD0); a.ref8("pal")
    lda_i(0); sta(0x2006); sta(0x2006)      # CHR RAM: tiles 0..3, one colour each
    a.b(0xA2, 0)
    a.label("chr")
    a.b(0xBD); a.ref16("chrtab"); sta(0x2007); a.b(0xE8, 0xE0, 64)
    a.b(0xD0); a.ref8("chr")
    lda_i(0x20); sta(0x2006); lda_i(0); sta(0x2006)
    a.b(0xA0, 30)                           # ldy #30 rows
    a.label("row")
    a.b(0xA2, 0)
    a.label("col")
    a.b(0x8A, 0x4A, 0x4A, 0x4A, 0x29, 0x03); sta(0x2007); a.b(0xE8, 0xE0, 32)   # tile (x >> 3) & 3
    a.b(0xD0); a.ref8("col")
    a.b(0x88, 0xD0); a.ref8("row")          # dey; bne row
    a.b(0xA2, 0)
    a.label("att")                          # attributes: four bands of palettes
    a.b(0x8A, 0x4A, 0x4A, 0x4A, 0x29, 0x03, 0xA8, 0xB9); a.ref16("attrtab"); sta(0x2007); a.b(0xE8, 0xE0, 64)
    a.b(0xD0); a.ref8("att")
    lda_i(0x0F); sta(0x4015)                # APU: pulse 1, about 440 Hz, constant volume
    lda_i(0xBF); sta(0x4000); lda_i(0x08); sta(0x4001); lda_i(0xFD); sta(0x4002); lda_i(0x00); sta(0x4003)
    lda_i(0); a.b(0x85, 0x11)               # scroll = 0
    lda_i(0x80); sta(0x2000); lda_i(0x1E); sta(0x2001)   # NMI and rendering on
    a.label("loop")
    a.b(0x4C); a.ref16("loop")

    a.label("nmi")
    a.b(0x48, 0x8A, 0x48)                   # pha; txa; pha
    lda_i(1); sta(0x4016); lda_i(0); sta(0x4016)
    lda(0x4016); a.b(0x29, 1, 0x85, 0x10)   # A -> $10
    lda(0x4016); a.b(0x29, 1, 0x85, 0x12)   # B -> $12
    a.b(0xA5, 0x10, 0xF0, 0x02, 0xE6, 0x11) # lda $10; beq +2; inc $11 (the bars move while A is held)
    lda_i(0x3F); sta(0x2006); lda_i(0); sta(0x2006)
    lda(0x4032); a.b(0x29, 1)               # drive status: bit 0 set = no disk
    a.b(0xF0); a.ref8("inserted")
    lda_i(0x2A); a.b(0x4C); a.ref16("setc") # green: no disk
    a.label("inserted")
    a.b(0xA5, 0x12, 0xF0); a.ref8("nob")
    lda_i(0x16); a.b(0x4C); a.ref16("setc") # red: B held
    a.label("nob")
    lda_i(0x02)                             # dark blue
    a.label("setc")
    sta(0x2007)
    lda_i(0); sta(0x2006); sta(0x2006)
    a.b(0xA5, 0x11); sta(0x2005); lda_i(0); sta(0x2005)
    lda_i(0x80); sta(0x2000)
    a.b(0x68, 0xAA, 0x68, 0x40)             # pla; tax; pla; rti
    a.label("irq")
    a.b(0x40)
    a.label("paltab")
    a.b(*([0x02, 0x16, 0x2A, 0x12, 0x02, 0x27, 0x1A, 0x30, 0x02, 0x06, 0x38, 0x21, 0x02, 0x14, 0x3B, 0x0F] * 2))
    a.label("chrtab")
    a.b(*([0] * 16 + [0xFF] * 8 + [0] * 8 + [0] * 8 + [0xFF] * 8 + [0xFF] * 16))
    a.label("attrtab")
    a.b(0x00, 0x55, 0xAA, 0xFF)
    code = a.done()
    rom = bytearray(0x2000)
    rom[: len(code)] = code
    rom[0x1FFA:0x2000] = struct.pack("<HHH", a.labels["nmi"], a.labels["reset"], a.labels["irq"])
    return bytes(rom)


def fds_disk(sides=2):
    """An .fds image (fwNES header) of `sides` empty disk sides: the disk info block and a file count of 0 each."""
    out = bytearray(b"FDS\x1a" + bytes([sides]) + bytes(11))
    for side in range(sides):
        info = bytearray(56)
        info[0] = 0x01
        info[1:15] = b"*NINTENDO-HVC*"
        info[16:19] = b"PKT"                # game name
        info[19] = 0x20
        info[21] = side                     # side number
        info[25:30] = b"\xff" * 5
        info[31:34] = b"\x61\x01\x01"       # date (BCD)
        block = bytes(info) + b"\x02\x00"   # block 2: no files
        out += block + bytes(65500 - len(block))
    return bytes(out)


# ---------------------------------------------------------------- Virtual Boy
class V810:
    """Just enough of a V810 assembler (little-endian halfwords; 32-bit forms put the opcode halfword first)."""

    def __init__(self, origin):
        self.a = Asm(origin)
        self.br = []

    def label(self, name):
        self.a.label(name)

    def h(self, v):
        self.a.w(v)

    def r(self, op, reg1, reg2):            # format I: reg2 <- reg2 op reg1
        self.h((op << 10) | (reg2 << 5) | reg1)

    def i5(self, op, imm, reg2):            # format II
        self.h((op << 10) | (reg2 << 5) | (imm & 0x1F))

    def i16(self, op, imm, reg1, reg2):     # format V / VI: opcode halfword, then the 16 bits
        self.h((op << 10) | (reg2 << 5) | reg1)
        self.h(imm)

    def b(self, cond, name):                # format III, relative to the branch itself
        self.br.append((len(self.a.out), cond, name))
        self.h(0)

    def done(self):
        for pos, cond, name in self.br:
            disp = self.a.labels[name] - (self.a.origin + pos)
            assert -256 <= disp < 256 and disp % 2 == 0, f"{name}: branch too far ({disp})"
            self.a.out[pos : pos + 2] = struct.pack("<H", 0x8000 | (cond << 9) | (disp & 0x1FF))
        return bytes(self.a.out)


def virtualboy():
    """64 KB ROM: V810 code that writes colour bars straight into the VIP's frame buffers (no drawing engine), the
    left eye's buffers with bars, the right eye's with the same bars in reverse shades, and plays a square wave on
    the first sound channel. Holding A moves the bars; holding the right D-pad's up swaps the shades of both eyes."""
    MOV, ADD, CMP, OR_, XOR, NOT, JMP = 0x00, 0x01, 0x03, 0x0C, 0x0E, 0x0F, 0x06
    MOVI, ADDI5, SHLI, SHRI = 0x10, 0x11, 0x14, 0x15
    MOVEA, ANDI, XORI, MOVHI = 0x28, 0x2D, 0x2E, 0x2F
    LDB, LDW, STB, STH, STW = 0x30, 0x33, 0x34, 0x35, 0x37
    BE, BNE, BR = 2, 10, 5
    v = V810(0x07000000)
    v.label("start")
    v.i16(MOVHI, 0x0006, 0, 1); v.i16(MOVEA, 0xF800, 1, 1)   # r1 = 0x0005F800, the VIP's registers
    for reg, val in [(0x24, 32), (0x26, 64), (0x28, 32)]:     # BRTA, BRTB, BRTC
        v.i16(MOVEA, val, 0, 2); v.i16(STH, reg, 1, 2)
    v.i16(STH, 0x42, 1, 0)                                    # XPCTRL = 0: the drawing engine stays off
    v.i16(MOVEA, 0x0302, 0, 2); v.i16(STH, 0x22, 1, 2)        # DPCTRL: display on, refresh, sync
    v.i16(MOVHI, 0x0100, 0, 3)                                # r3 = 0x01000000, the sound unit
    v.r(MOV, 3, 8); v.i16(MOVEA, 16, 0, 10); v.i16(MOVEA, 63, 0, 2)
    v.label("w1")                                             # wave 0: 16 samples high ...
    v.i16(STW, 0, 8, 2); v.i5(ADDI5, 4, 8); v.i5(ADDI5, -1, 10); v.b(BNE, "w1")
    v.i16(MOVEA, 16, 0, 10)
    v.label("w2")                                             # ... 16 low
    v.i16(STW, 0, 8, 0); v.i5(ADDI5, 4, 8); v.i5(ADDI5, -1, 10); v.b(BNE, "w2")
    v.i16(MOVEA, 0x400, 3, 8)                                 # channel 1
    for off, val in [(4, 0xFF), (8, 0x9D), (12, 0x06), (16, 0xF0), (20, 0), (24, 0), (0, 0x80)]:
        v.i16(MOVEA, val, 0, 2); v.i16(STB, off, 8, 2)        # volume, about 440 Hz, envelope 15, wave 0, on
    v.i16(MOVHI, 0x0200, 0, 3)                                # r3 = 0x02000000, the pad's serial port
    v.i5(MOVI, 0, 4)                                          # r4 = scroll
    v.i16(MOVEA, 0x84, 0, 2); v.i16(STB, 0x28, 3, 2)          # SCR: start a read of the pad
    v.i16(MOVHI, 0x0700, 0, 16); v.i16(MOVEA, 0x1000, 16, 16) # r16 = the table of words
    v.label("frame")
    v.label("busy")
    v.i16(LDB, 0x28, 3, 2); v.i16(ANDI, 2, 2, 2); v.b(BNE, "busy")
    v.i16(LDB, 0x10, 3, 5); v.i16(ANDI, 0xFF, 5, 5)           # SDLR
    v.i16(LDB, 0x14, 3, 2); v.i16(ANDI, 0xFF, 2, 2); v.i5(SHLI, 8, 2); v.r(OR_, 2, 5)   # | SDHR << 8
    v.i16(MOVEA, 0x84, 0, 2); v.i16(STB, 0x28, 3, 2)          # next read
    v.i16(ANDI, 0x04, 5, 2); v.b(BE, "noa")                   # A: the bars move
    v.i5(ADDI5, 2, 4)
    v.label("noa")
    v.i16(ANDI, 0x40, 5, 6)                                   # r6: right D-pad up held
    v.i5(MOVI, 0, 7)                                          # r7 = column
    v.i5(MOVI, 0, 12)                                         # r12 = left buffer 0 + column * 64
    v.label("col")
    v.r(MOV, 7, 2); v.r(ADD, 4, 2); v.i5(SHRI, 5, 2); v.i16(ANDI, 3, 2, 2)   # shade = ((x + scroll) >> 5) & 3
    v.r(CMP, 0, 6); v.b(BE, "keep")
    v.i16(XORI, 3, 2, 2)
    v.label("keep")
    v.i5(SHLI, 2, 2); v.r(ADD, 16, 2); v.i16(LDW, 0, 2, 9)    # r9 = the shade in all 16 pixels of a word
    v.r(NOT, 9, 11)                                           # r11: the right eye's reverse shade
    v.i16(MOVEA, 0x4000, 12, 13); v.i16(MOVEA, 0x4000, 13, 13)   # r13 = left buffer 1 (+0x8000)
    v.i16(MOVHI, 1, 12, 14)                                   # r14 = right buffer 0 (+0x10000)
    v.i16(MOVHI, 1, 13, 15)                                   # r15 = right buffer 1 (+0x18000)
    v.i16(MOVEA, 16, 0, 10)
    v.label("cw")                                             # 16 words: 256 rows of two bits
    v.i16(STW, 0, 12, 9); v.i16(STW, 0, 13, 9); v.i16(STW, 0, 14, 11); v.i16(STW, 0, 15, 11)
    v.i5(ADDI5, 4, 12); v.i5(ADDI5, 4, 13); v.i5(ADDI5, 4, 14); v.i5(ADDI5, 4, 15)
    v.i5(ADDI5, -1, 10); v.b(BNE, "cw")
    v.i5(ADDI5, 1, 7)
    v.i16(MOVEA, 384, 0, 2); v.r(CMP, 2, 7); v.b(BNE, "col")
    v.h((0x2A << 10)); v.h(0)                                 # placeholder, replaced below by jr frame
    code = bytearray(v.done())
    jr_at = len(code) - 4
    disp = v.a.labels["frame"] - (0x07000000 + jr_at)
    code[jr_at : jr_at + 4] = struct.pack("<HH", (0x2A << 10) | ((disp >> 16) & 0x3FF), disp & 0xFFFF)
    rom = bytearray(0x10000)
    rom[: len(code)] = code
    rom[0x1000:0x1010] = struct.pack("<4I", 0x00000000, 0x55555555, 0xAAAAAAAA, 0xFFFFFFFF)
    rom[0xFDE0:0xFDF4] = b"POCKET TEST".ljust(20)  # header: title, maker, game code, version
    rom[0xFDF9:0xFE00] = b"PK" + b"PKTE" + b"\x00"
    # reset at 0xFFFFFFF0 (the ROM's last 16 bytes): r1 = 0x07000000; jmp [r1]
    reset = V810(0x0700FFF0)
    reset.i16(MOVHI, 0x0700, 0, 1); reset.r(JMP, 1, 0)
    rom[0xFFF0 : 0xFFF0 + 6] = reset.done()
    return bytes(rom)


# ---------------------------------------------------------------- SNES and GBA (second folders: sfc, snesh, gbah, gbav)
def snes():
    """32 KB LoROM: a red backdrop, nothing else. For the second SNES folders, whose core is the one of roms/snes:
    it shows that the game starts, saves and resumes from there."""
    code = bytes([0x78, 0x18, 0xFB, 0xE2, 0x30,             # sei; clc; xce; sep #$30
                  0xA9, 0x8F, 0x8D, 0x00, 0x21,             # forced blank
                  0x9C, 0x21, 0x21, 0xA9, 0x1F, 0x8D, 0x22, 0x21, 0x9C, 0x22, 0x21,   # colour 0 = red
                  0xA9, 0x0F, 0x8D, 0x00, 0x21,             # screen on
                  0x80, 0xFE])                              # bra *
    rom = bytearray(0x8000)
    rom[: len(code)] = code
    rom[0x7FC0:0x7FD5] = b"POCKET TEST".ljust(21)
    rom[0x7FD5], rom[0x7FD7] = 0x20, 0x08                  # LoROM, 32 KB
    rom[0x7FFC:0x7FFE] = struct.pack("<H", 0x8000)          # reset
    return bytes(rom)


def gba():
    """A GBA ROM (ARM code): mode 3, the whole screen red. Like `snes`, for the second GBA folders."""
    words = [0xEA00002E] + [0] * 47                         # b 0xC0, then the header area
    code = [0xE3A00301, 0xE3A01B01, 0xE2811003, 0xE1C010B0,  # DISPCNT = mode 3, BG2
            0xE3A00406, 0xE3A0101F, 0xE1811801, 0xE3A02C4B,  # VRAM, red twice, 19200 words
            0xE4801004, 0xE2522001, 0x1AFFFFFC, 0xEAFFFFFE]  # str r1,[r0],#4; subs; bne; b .
    rom = bytearray(struct.pack(f"<{len(words)}I", *words) + struct.pack(f"<{len(code)}I", *code))
    rom[0xA0:0xAC] = b"POCKET TEST\x00"
    rom[0xAC:0xB0] = b"PKTE"
    rom[0xB2] = 0x96
    rom[0xBD] = (-(sum(rom[0xA0:0xBD]) + 0x19)) & 0xFF
    return bytes(rom.ljust(0x10000, b"\x00"))


# ---------------------------------------------------------------- NAOMI (stand-in BIOS, virtual device only)
def naomi_bios():
    """A STAND-IN for the NAOMI BIOS (MAME's naomi.zip holds 2 MB per region), for the virtual device only: our own
    SH-4 program, not Sega's, so it boots no game. Flycast runs it from the reset vector, so the core's NAOMI path
    (bios/dc/naomi.zip, the cartridge file, its GL renderer showing a frame buffer, states) is exercised. It points the
    video output at a 640x480 frame buffer of 32-bit pixels, paints eight coloured bands and then keeps drawing a
    white line down the middle, one pixel a moment, so later pictures differ and a loaded state can be told apart."""
    code = []
    lits, fix = [], []

    def lit(n, value):                       # mov.l @(disp,PC),Rn from the literal pool after the code
        fix.append((len(code), n, value))
        code.append(0)

    def op(v):
        code.append(v & 0xFFFF)

    def mov_r(m, n): op(0x6003 | n << 8 | m << 4)
    def add_i(i, n): op(0x7000 | n << 8 | (i & 0xFF))
    def add_r(m, n): op(0x300C | n << 8 | m << 4)
    def st(m, n): op(0x2002 | n << 8 | m << 4)   # mov.l Rm,@Rn
    def dt(n): op(0x4010 | n << 8)
    def bf_to(target): op(0x8B00 | (((target - (len(code) * 2 + 4)) // 2) & 0xFF))

    lit(1, 0xAC000000)                       # a mark in main RAM (peek 0): the program runs
    lit(0, 0x4B434F50); st(0, 1)
    lit(1, 0xA05F8044)                       # FB_R_CTRL: on, 0888 (32-bit pixels), VGA clock
    lit(0, 0x0080000D); st(0, 1)
    lit(1, 0xA05F8050)                       # FB_R_SOF1: frame buffer at 2 MB into video RAM
    lit(0, 0x00200000); st(0, 1)
    lit(1, 0xA05F805C)                       # FB_R_SIZE: 640 words a line, 480 lines, no gap
    lit(0, (1 << 20) | (479 << 10) | 639); st(0, 1)
    lit(1, 0xA05F80E8)                       # VO_CONTROL: video no longer blanked (it is after reset)
    lit(0, 0x00160000); st(0, 1)
    lit(3, 0xA5200000)                       # the frame buffer through the 32-bit path
    lit(6, 0x00203040)                       # first band's colour
    op(0xE708)                               # mov #8,r7: bands
    outer = len(code) * 2
    lit(4, 640 * 60)
    inner = len(code) * 2
    st(6, 3); add_i(4, 3); dt(4); bf_to(inner)
    lit(0, 0x00182808); add_r(0, 6)
    dt(7); bf_to(outer)
    lit(3, 0xA5200000 + 320 * 4)             # the white line, in the middle: one pixel per 2^23 turns of the counter
    lit(6, 0x00FFFFFF)
    lit(9, 640 * 4)
    lit(10, 0xAC000004)                      # where the line has got to, in main RAM too (peek 4..7)
    line = len(code) * 2
    lit(4, 0x00800000)
    wait = len(code) * 2
    dt(4); bf_to(wait)
    st(6, 3); add_r(9, 3); st(3, 10)
    op(0xA000 | (((line - (len(code) * 2 + 4)) // 2) & 0xFFF)); op(0x0009)   # bra line; nop
    pool = (len(code) * 2 + 3) & ~3
    words = []
    for pos, n, value in fix:
        words.append(value)
        disp = (pool + (len(words) - 1) * 4 - ((pos * 2) & ~3) - 4) // 4
        assert 0 <= disp < 256
        code[pos] = 0xD000 | n << 8 | disp
    blob = bytearray(struct.pack(f"<{len(code)}H", *code))
    blob += bytes(pool - len(blob))
    blob += struct.pack(f"<{len(words)}I", *words)
    return bytes(blob.ljust(0x200000, b"\xff"))


def naomi_zip():
    """naomi.zip with the stand-in under the names of every region's BIOS that Flycast looks for."""
    import io
    import zipfile
    out = io.BytesIO()
    with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as z:
        for name in ["epr-21576h.ic27", "epr-21576g.ic27", "epr-21577h.ic27", "epr-21577g.ic27", "epr-21578h.ic27",
                     "epr-21578g.ic27", "epr-21579d.ic27"]:
            z.writestr(name, naomi_bios())
    return out.getvalue()


def naomi_cart():
    """A 1 MB cartridge file with a NAOMI header and nothing to run (the stand-in BIOS never starts it)."""
    rom = bytearray(0x100000)
    rom[0x00:0x10] = b"NAOMI           "
    rom[0x10:0x30] = b"POCKET UI TEST".ljust(32)
    for i in range(8):                       # the game's name per region
        rom[0x30 + i * 32 : 0x50 + i * 32] = b"POCKET UI TEST".ljust(32)
    return bytes(rom)


def main():
    out = Path(sys.argv[1])
    out.mkdir(parents=True, exist_ok=True)
    (out / "sms-test.sms").write_bytes(sega_rom("sms"))
    (out / "gg-test.gg").write_bytes(sega_rom("gg"))
    (out / "sg-test.sg").write_bytes(sega_rom("sg"))
    (out / "a2600-test.a26").write_bytes(a2600())
    (out / "ws-test.ws").write_bytes(wonderswan())
    (out / "fds-test.fds").write_bytes(fds_disk())
    (out / "fds-bios-standin.rom").write_bytes(fds_bios())
    (out / "vb-test.vb").write_bytes(virtualboy())
    (out / "snes-test.sfc").write_bytes(snes())
    (out / "gba-test.gba").write_bytes(gba())
    (out / "naomi-bios-standin.zip").write_bytes(naomi_zip())
    (out / "naomi-test.bin").write_bytes(naomi_cart())
    print("written to", out)


if __name__ == "__main__":
    main()
