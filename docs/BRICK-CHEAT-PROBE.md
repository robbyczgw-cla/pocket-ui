# Cheat effect probe

`scripts/brick-cheat-check.c` runs an existing libretro core without a display,
audio device or controller. It proves that a code changes RAM, keeps working
while the game continues and after loading a state, and stops rewriting RAM
when the frontend clears the cheat list. Video is discarded without hashing.
It shares the minimal VFS and core-option defaults of `brick-core-check.c`.

Build with `cc -O1 scripts/brick-cheat-check.c -ldl -o cheat-check`. For the
Brick use `zig cc -target aarch64-linux-gnu.2.33` instead of `cc`. Compile on
the build machine; run on the device only within a coordinated device session.

The arguments are `CORE.so ROM CODE RAM_OFFSET EXPECTED [WIDTH [FRAMES [WARMUP]]]`.
Numbers accept `0x` prefixes. The default width is one byte, with three frames
per check and 120 warmup frames. Two- and four-byte reads are little endian.
The code is passed to `retro_cheat_set` unchanged, including `+` separators.
Code strings of 256 bytes or more are rejected because some core parsers have
fixed buffers. The probe has a 30-second alarm, including core loading and state
checks, so it is bounded even on firmware without a `timeout` applet. An external
timeout can provide another bound on the host.

The probe writes a sentinel differing in **every byte** from the target value.
With cheats off, the game must not naturally produce the expected value. With
the code on, RAM must reach the expected value after running frames. A
four-byte check spanning two CodeBreaker writes therefore verifies both writes.
It serializes an on-state in memory, changes RAM, restores the state, and
checks both restored RAM and continued cheat application. No reapply occurs
after restoring: losing an active cheat must be detected. It then tests direct
`enabled=false` as a diagnostic, clears cheats, and checks that the target
value is no longer forced. Loading the earlier on-state while cheats are off
must also keep them off, without a reset after loading. Any failed assertion
returns a nonzero exit status.

`BRICK_CHEAT_SYSTEM_DIR` selects a directory containing BIOS files.
`BRICK_CHEAT_SAVE_DIR` selects a disposable directory (default `/tmp`).
`BRICK_CHEAT_MEMORY_ID` selects a libretro memory region (default 2, system
RAM). `BRICK_CHEAT_MAP_START` instead selects an exact, writable linear system
RAM region received through `SET_MEMORY_MAPS`; the offset remains relative to
that region. The probe copies map descriptors immediately because core-owned
descriptor arrays may be temporary.

Use disposable content directories too: a core can create files beside its
ROM despite receiving a save directory. The probe itself never persists states
or saves. It mutates only the running core's RAM and in-memory state. Its
results prove the core API and memory effect; pause-menu navigation and a
player's Continue workflow still need the frontend device test.

## Known core behavior

These observations come from the core source snapshots used for the Brick
build, rather than from another frontend's cheat documentation.

| Core snapshot | Code format and constraints | Toggle behavior |
| --- | --- | --- |
| gpSP `5819380` | Unencrypted CodeBreaker pairs, eight hex address digits and four value digits. Nonhex separators, including `+`, become spaces. Maximum 20 entries, 64 pairs per entry, input shorter than 1024 bytes. Encrypted and other code types are unsupported. | The snapshot implements `enabled=false`. Reset followed by enabled-only reapply also works. |
| mGBA `e31759b` | GBA CodeBreaker and supported GameShark/Action Replay forms; whitespace or `+` accepted. | `index` and `enabled` are ignored. Clear and reapply enabled entries for any change. |
| Snes9x `890b5d4` | PAR / Game Genie; tokens separated by `+,.; `; `AAAAAA:VV` is normalized to PAR. Fixed 256-byte input copy. | Appends groups, ignoring the index. `enabled=false` does not remove a previous active group. Clear and reapply enabled entries. |
| Gambatte `d9d6cd0` | GameShark `01VVAALL` RAM writes and Game Genie; `+` becomes `;`. GameShark writes execute through the Vblank interrupt handler. | Tracks entry indices and enabled flags. Reset and reapply also works. |
| PCSX-ReARMed `da2cb8e` | Eight hex address digits and four value digits; nonhex separators alternate between a space and newline. | Supports enabled flags; the code string must remain non-null even when disabling. |

mGBA's generic `retro_get_memory_size(2)` incorrectly reports the GB working
RAM size (32 KB) even for GBA. Its memory map correctly describes 256 KB EWRAM
at `0x02000000`; use `BRICK_CHEAT_MAP_START=0x02000000` for an EWRAM offset beyond
the advertised generic range. This probe refuses out-of-range generic reads
instead of trusting a guessed allocation size.

Useful database examples, with exact game-region matching required:

| Game / cheat | Code | RAM offset / expected / width |
| --- | --- | --- |
| Advance Wars (USA), P1 Infinite G | `8201AB9C+423F+8201AB9E+000F` | EWRAM `0x1ab9c` / `0x000f423f` (999999) / 4 |
| Contra III (USA), lives | `7E1F8A03+7E039203` | SNES WRAM `0x1f8a` / 3 / 1 (first code) |
| Super Mario Land, unlimited lives | `010315DA` | DMG WRAM `0x1a15` / 3 / 1 |

The ROM must actually reach a point at which the cheat can execute. For
example, an original GB test loop must enable Vblank interrupts before
Gambatte's GameShark handler runs. An idle loop with interrupts disabled is a
useful negative control, not evidence that the core lacks cheat support.

## Container verification

The five ARM core binaries corresponding to the snapshots above passed the
probe against the Brick's glibc 2.33 sysroot in an ARM64 Linux container. Tests
used original synthetic GBA, SNES, GB and PS-X EXE programs, 30 warmup frames
and three frames per assertion. gpSP and mGBA passed the full four-byte,
two-code max-money write; Snes9x passed the first byte of the two-code lives
example; Gambatte passed the DMG lives address; PCSX-ReARMed passed a two-byte
write at RAM offset `0x1000` with `80001000+0009` using HLE BIOS. All passed
both state-resume checks without a state-load reset or reapply. Direct disable
continued writing on mGBA and Snes9x, as predicted by their sources.

An ignored gpSP CRC code (`00000000+0000`) intentionally failed the on/resume
assertions and returned exit status 1, confirming that accepting a cheat string
alone does not make this probe pass. These container runs establish core API
behavior; the separate real-device checks below establish the Brick result.


## Brick hardware verification (2026-11-08)

The actual Brick, with public 0.3.0.1 preserved, ran temporary ARM frontend
and player binaries from RAM, using its installed cores. Disposable copies
of the existing games, settings, saves and states kept player files untouched.
Each game was opened through pocket-ui, the named code enabled in
Options > Cheats, continued, saved and loaded, then disabled in the same page.
The test-only player `peek` command read the core RAM before and after.

| Real game | Core | RAM effect through the frontend |
| --- | --- | --- |
| Advance Wars (USA), P1 Infinite G | gpSP | EWRAM `0x1ab9c`: `00 00 00 00` → `3f 42 0f 00` (999999) |
| Contra III, Infinite Lives | Snes9x | WRAM `0x1f8a`: `00` → `03` |
| Pokémon Crystal, Infinite Money | Gambatte | WRAM `0x184e`: `00 00 00` → `0f 42 3f` (999999, big endian) |

All three values survived Continue and a real frontend save/load. Disabling
sent the player's empty cheat list and removed the persisted enabled entry.
The native probe also passed on the same real games, including Advance Wars
with mGBA, and proved that cleared codes stop forcing the sentinel after
Continue and loading an earlier on-state. This distinction matters: switching
off cannot take back money or lives already granted. The mGBA run used its
EWRAM map as described above.

A homebrew game without a matching file showed the explanatory empty page
with Back alone, while the database contained another game's cheats. The
JPEGs record the actual menu state; RAM readings establish the game effect.
The menu's dimmed background can retain a previous game's frame and is not
used as HUD evidence. PS1 has container evidence only, not this hardware check.

Each session restored and hash-checked the original configuration and protected
files before resuming the original UI. CPU policy and the complete ALSA mixer
were unchanged, all temporary files and bounded guards were removed, and the
Brick remained on public 0.3.0.1, muted, LEDs off and panel brightness zero.
