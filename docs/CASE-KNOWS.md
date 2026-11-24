# The case knows where you are

*Reference. The code: `src/case_knows.rs` (the saves' layouts), `src/case_knows_read.rs` (files, cache, thread, the
line), `src/app_case_knows.rs` (the App's side). For players: [user/library.md](user/library.md#where-you-are-in-the-game).*

Turn a game's box over and its back says where you are in it, read from your own save: "8 badges · Charizard Lv. 45 ·
game clock 31:12", "62% items · game clock 4:31", "43 of 96 exits", "2 of 3 pendants · 9 hearts". The same line stands beside Continue
on the shelf and on the "Where was I?" card. No account, no network. One switch: Settings → Look → Cozy extras → **Progress from saves**
(`cozy_extras::Extra::ProgressFromSaves`, on). Off: no line in any of the three places, and `App::case_progress` / `case_art` answer
before the thread is even started, so no save and no ROM is read (a spoiler-free case). The Android handheld view shares the code and the switch.

## What is read, and when nothing is shown

- **Only the battery save** (`<game>.srm`, `<game>.sav`, DeSmuME's `<game>.dsv` with its footer after the 512 KiB; on the TrimUI Brick pocket-core's `POCKET_CORE_SAVE_DIR`, in
  Android's handheld view the player's `saves/<key>/`). Never a save state: the battery save is the same bytes on every
  device and every emulator. A moment continued from a save state can be ahead of the game's last in-game save; the
  line is what the game itself saved, with the game's own clock.
- **The release by its ROM's CRC-32**, as libretro-database's No-Intro lists give it (`case_knows::RELEASES`). The sum
  comes from the identify cache (`rom_id`, `identify/checksums.tsv`) when it has the file at its size and time, else it
  is worked out once on the reading thread, only for a file of a size one of the releases has
  (`case_knows::worth_checking`). A SNES file with a copier header matches by its second sum. **DS games** by the game
  code in their cartridge header instead (`case_knows::ds_key`: 512 bytes read, the header's own CRC-16 checked; "CPUE"
  is Platinum in English): their ROMs are 64 to 512 MiB, more than the identify cache reads through, trimmed dumps
  never match No-Intro's sum, and one code covers every revision. A hack built on a release keeps its code: it shows a
  line only when its save is laid out as the release's and passes the same checks (Japanese releases, and hacks made
  from them, have another code).
- **The save by the game's own checksum**, the way the game itself checks it before it loads the save, then by what
  the game can hold (levels 1 to 100, minutes under 60, at most 96 exits, heart containers in eights …). A hack, a
  translation, a revision not in the list, a save the game would call damaged, a value it can't have: **nothing is
  shown**, never a wrong line. A Pokémon whose name isn't in plain letters (a Japanese trade) leaves out the name, not
  the line.
- **Several files in one save** (Super Mario World, A Link to the Past): the file whose bytes changed since the last
  read is the one being played (`case_knows::Memory`, kept in the cache). One file alone is that one. Several and nothing
  to go by: nothing. Super Metroid says itself which file was chosen last (`0x1FEC`).

## Checked against the games

A release shows a line only when its reader was checked against **saves the game itself wrote**, read as the game's
own screens show them (`case_knows::Release::verified`, `case_knows_read::Reading::progress`; 15.11.2026: the games
headless in their libretro cores, values set in RAM, the game's own save, its file screen compared). For Super Mario
World's own cartridge and the DS games: saves real players wrote and shared (a Virtual Console save; Project Pokémon's
contributed saves with their screenshots, one of them the game's own Continue screen), read as those screens show
them; the sources are in CASE-KNOWS-VERIFY.md of the review notes. One region's versions and revisions share their save
code, so the check covers the region of the ROMs checked: the USA (and "USA, Europe") releases; the DS entries are the
English cartridges' game codes. Not shown, their reading and tests kept:

- Every release from another region or language (Europe-only, Germany, France, Spain, Italy, Japan, Australia,
  Canada): no ROM and no documented save of theirs checked. One exception: Diddy Kong Racing's European cartridge (Rev 1), checked
  against a save it wrote (the same EEPROM layout as the US one), so both European releases show a line.
- Fire Emblem's mode: on no save menu screen, so the line names the chapter, the lord and the clock.
- Paper Mario's file never played (its clock 0): the game's menu says "First Play" and nothing else; so does pocket-ui.

To bring one back: a save the game wrote, its line equal to the game's screen, the release (or kind) added in
`Release::verified`, and the tables here and in user/library.md.

## Cost

Nothing is read until the shelf, a box, the card or a start asks for one game (`App::case_line`), and then on the
`case-knows` thread at the lowest CPU and I/O priority. The painter never waits: the line appears once the answer is
there (`UserEvent::CaseKnows`, a frame only when it changed). While a game runs the thread reads only saves of ROMs it
knows already (for "Where was I?"), never a ROM through. The cache, `<data>/identify/case-knows.tsv`, holds per ROM
its size, time, the release's CRC (or `-`) and what a several-file save remembers; a save is read again only when its
size or time changed (2 KiB to 512 KiB, under a millisecond to parse; files over 1 MiB are not read). A game's end makes its line stale, and so do saves Pocket Sync
brings from another Pocket (`App::case_saves_came`); the old line stays on screen until the new one is in.

## The games

| Game | Releases | The line | The save |
|---|---|---|---|
| Pokémon Red, Blue, Yellow | EN, DE, ES, FR, IT | badges · first Pokémon and level · the game's clock | bank 1 from the player's name to the checksum byte at `0x3523` |
| Pokémon Gold, Silver | EN, DE, ES, FR, IT | badges of Johto and Kanto · first Pokémon that isn't an egg · clock | `0x2009` to `0x2D68`, its sum at `0x2D69` |
| Pokémon Crystal | EN (USA, Rev 1, Australia), DE, ES, FR, IT | as Gold | `0x2009` to `0x2B82`, its sum at `0x2D0D` |
| Pokémon Ruby, Sapphire | EN (Rev 0–2), DE (Rev 0–1), ES, FR, IT (Rev 0–1) | badges · first Pokémon · clock | two copies of 14 sectors, each with its checksum; the newer whole copy |
| Pokémon Emerald | EN, DE, ES, FR, IT | as Ruby | as Ruby |
| Pokémon FireRed, LeafGreen | EN (Rev 0–1), DE, ES, FR, IT | as Ruby | as Ruby |
| Pokémon Diamond, Pearl, Platinum | EN, DE, ES, FR, IT (game codes `ADA?`, `APA?`, `CPU?`) | badges · first Pokémon that isn't an egg · clock | 512 KiB: the general block at 0 and 0x40000 (0xC100 bytes, Platinum 0xCF2C), each with a footer of the save counter, the block's counter, its size and the CRC-16 (CCITT) of all before the footer's 0x14 bytes; the newer valid one by save counter, then block counter (PKHeX `SAV4`, `SAV4BlockDetection`). Trainer at 0x64 (Platinum 0x68): badges +0x1A, clock +0x22; team count at the party −4, six of 236 bytes, each unscrambled by its checksum and personality and checked by its own sum (`PokeCrypto`) |
| Pokémon HeartGold, SoulSilver | EN, DE, ES, FR, IT (`IPK?`, `IPG?`) | badges of Johto and Kanto · first Pokémon · clock | as Diamond, 0xF628 bytes, the CRC over all but the last 0x10 (checked against a real save); Kanto's badges at trainer +0x1F |
| Pokémon Black, White, Black 2, White 2 | EN, DE, ES, FR, IT (`IRB?`, `IRA?`, `IRE?`, `IRD?`) | badges · first Pokémon · clock | blocks at fixed places, each with its CRC-16 two bytes after it and again in the table of sums (0x23F00, Black 2 0x25F00): the party (0x18E00, six of 220 bytes), the trainer's data (0x19400, clock +0x24), the badges' bits (0x21200 +4, Black 2 0x21100) (PKHeX `SaveBlockAccessor5BW`/`B2W2`) |
| Super Metroid | Japan/USA, Europe | the ending's item percentage · clock | three files with sums and complements; the file chosen last |
| Metroid: Zero Mission | USA, Europe | the ending's item percentage (tanks by the difficulty's step, suit, beams, bombs) · clock | three files and their copies, each a sum of 32-bit words and its complement; the record of the file saved last (`0x1E`, its own sum). Offsets from the metroidret decomp's `struct Sram` laid out as the game's compiler does it (every struct on 4 bytes: files of 4640 bytes, copies at 14048, the record at 28032), checked against a save the game wrote and its file screen |
| Super Mario 64 | USA, Europe, Japan, Shindou (`.z64` and byte-swapped) | stars of 120, as the file select counts them · a slim bar | the EEPROM (512 bytes; the start of mupen64plus's `.srm`, or a `.eep`): four files with two copies, each ending in "DA" and its byte sum (n64decomp/sm64 `struct SaveBuffer`); the file saved last by what changed |
| The Legend of Zelda: Ocarina of Time | USA (Rev 0–2), Europe (Rev 0–1), each `.z64` and byte-swapped | spiritual stones of 3, from the first medallion medallions of 6 · heart containers · marks | the SRAM (from 0x20800 in mupen64plus's `.srm`, or a `.sra`; either word order): three slots of 0x1450 bytes and their backups, "ZELDAZ" and the sum of the 16-bit words (zeldaret/oot `include/save.h`, `z_sram.c`); the file played by what changed |
| The Legend of Zelda: Majora's Mask | USA | remains of 4 · hearts · marks | flash (in mupen64plus's `.srm` from 0x28800, or alone): two files, each its owl save (pages 0x100/0x200, backups 0x180/0x280) before its new-cycle save (pages 0/0x80, backups +0x40); "ZELDA3" and the byte sum (0x100C bytes, an owl save 0x3CA0) at 0x100A; most health 0x34, remains bits of the quest items at 0xBC (zeldaret/mm `z64save.h`, `z_sram_NES.c`); either 32-bit word order |
| Paper Mario | USA, Europe (each `.z64` and byte-swapped) | Star Spirits of 7 · Mario's level · clock, as its file select shows them · marks | the flash (from 0x28800 in mupen64plus's `.srm`, or a `.fla`): six saves at 16 KiB steps with "Mario Story 006", a 32-bit word sum and its complement; the globals' file last chosen, its newest save by count (pmret/papermario `fio.c`, `struct SaveData`, `SaveMetadata`) |
| Mario Kart 64 | USA, Europe (Rev 0–1), Japan (Rev 0–1), each `.z64` and byte-swapped | gold cups of 16 (4 cups in 4 classes) · a slim bar | the EEPROM: the Grand Prix points at 0x180 (two bits a cup, 3 gold) and the two check bytes after them (n64decomp/mk64 `save_data.h`, `save.c`) |
| Star Fox 64 / Lylat Wars | USA (Rev 0–1), Japan, Australia, each `.z64` and byte-swapped | medals, and those in Expert | the EEPROM's save and backup (0x100 each), each with its 16-bit check (`Save_Checksum`); the planets' flag bytes as the N64's compiler lays out the bit fields (from the high bit), checked for sense: no medal without its clear, no clear unplayed (sonicdcer/sf64 `sf64save.h`, `fox_save.c`) |
| Diddy Kong Racing | USA (Rev 0–1), Europe (Rev 0–1), Japan, each `.z64` and byte-swapped | golden balloons of 47 · a slim bar | the EEPROM: three files of 40 bytes, bit fields high bit first, the first 16 bits minus 5 minus the bytes after them make 0; the balloons from bit 112 (DavidSM64/Diddy-Kong-Racing `save_layout.h`, `save_data.c`) |
| Final Fantasy III (the sixth) | USA (Rev 0–1) | the active party's leader and level · clock | three slots of 0xA00 bytes (a copy of $7E:1600 on), their bytes adding up to the word at 0x9FE, the slot chosen last at 0x1FF0 (everything8215/ff6 `save.asm`, Data Crystal's RAM map); the leader as the save screen shows it, the active party's character in the first row (`InitCharProp`), checked against saves the game wrote |
| EarthBound | USA | the party's leader and level · how many friends travel along (the game keeps no clock to show) | six blocks of 1280 bytes, a file and its copy, each with "HAL Laboratory, inc.", a byte sum and an XOR of the words (Herringway/ebsrc `structs.asm`, `system/saves/`); the game writes the header into all six blocks, so a file is there when it holds more; the characters 95 bytes each from 505; names in the game's letters, checked against its own name table and a save the game wrote |
| The Legend of Zelda: Link's Awakening DX | USA/Europe (Rev 0–2), Germany, France (Rev 0–1) | instruments of 8 · heart containers · marks | three files of 0x3AD bytes from 0x100, each opened by 1, 3, 5, 7, 9 (the game has no sum: its own mark, then only values it can hold), a copy of $D800-$DB7F (zladx/LADX-Disassembly `sram.asm`, `wram.asm`). The game marks all three at its first start; a file is there when it has a name, as the player select counts them |
| The Legend of Zelda: The Minish Cap | USA, Europe | elements of 4 · hearts · marks | EEPROM of 8 KiB: three files of 0x500 bytes from 0x80, each with a copy 0x1000 on and a status of sum, negative and "MCZ3" (`CalculateChecksum`); the header's file chosen last; most health at 0xAB (8 a heart), elements as items 0x40-0x43 of the 2-bit inventory at 0xF2; read in either 8-byte block order (zeldaret/tmc `save.c`, `save.h`, `GetInventoryValue`) |
| The Legend of Zelda: Oracle of Ages, Oracle of Seasons | USA | essences of 8 · hearts · marks | three files of 0x550 bytes at 0x10, 0x560, 0xAB0 with copies at 0x1000…; the sum of the words after the first and the game's string `Z21216-0`/`Z11216-0`; most health (4 a heart) and the essences' bits at the game's own addresses for each (Stewmath/oracles-disasm `fileManagement.s`, `wram.s`) |
| Super Mario World | USA, Europe (Rev 0–1), Japan | exits of 96 | three files and their copies, bytes and sum make `0x5A5A` |
| Super Mario All-Stars + Super Mario World | USA (also padded to 3 MiB) | exits of 96 · bar | Super Mario World's files and sum, four of them (A to D) from 0x1000 with the copies 0x23C on, the file chosen last at 0x0E (checked against a real save and saves the game wrote) |
| The Legend of Zelda: A Link to the Past | USA, Europe, Germany, France, Canada | pendants of 3 (Light World) or crystals of 7 (after Agahnim) · heart containers | three files and their mirrors, marked `0x55AA`, words make `0x5A5A` |
| Fire Emblem (the seventh), Fire Emblem: The Sacred Stones | USA | the chapter as the save menu names it · the lord whose story it is · clock | 32 KiB: a header (magics, `Checksum16` of 0x50 bytes, the save written last at 0x62) and a table of blocks (magics, place, size, `Checksum32`: the halfwords' sum and XOR); the game block's play state: frames at 0, chapter 0xE, state bits 0x14, story 0x1B; chapter numbers per story from the decomps' chapter lists (7ucina/fireemblem7u, FireEmblemUniverse/fireemblem8u). A tower, ruins or skirmish save and the creature campaign leave the chapter out |
| Donkey Kong Country 1, 2, 3 | USA (1 and 2 also Rev 1) | the share its file select shows (of 101, 102, 105) · clock (2 and 3) · bar | 2 KiB, three files and none marked as played last (the file that changed is). 1: slots of 0x154 bytes from 8, "ARER", the words' XOR and sum at 0 and 2, the share at 0x0E. 2 and 3: files of 0x2A8 bytes from 8 / 0x28A from 0x62, sum and XOR of the words from 6, 0x52 and the kind at 4, the first player's clock (frames) and share (Yoshifanatic1 DKC1, p4plus2/DKC2-disassembly, BlueImp78/DKC3-Disassembly). Europe and DKC1 Rev 2 left out: no disassembly of them, no save of theirs checked (DKC1 Rev 1's save, made by the game, reads the same) |
| Chrono Trigger | USA | the party's leader and level · clock | three slots of 0xA00 bytes, each with its sum at 0x1FF0 + 2n (16-bit words from the slot's end to its start, each carry into the next addition) and the mark 0xE41B at 0x1FF8 + 2n; the slot saved last at 0x1FE0; party 0x580, names 0x5B0, levels in the characters' blocks from 0x200, clock from 0x5E3 as the game keeps it ($7E0400): frames, seconds, minute units, minute tens, hour units, hour tens (Data Crystal's 0x5E5/0x5E6 as minutes/hours is wrong: it read an 18:44 save as 4:04; checked against saves the game wrote and its menu's TIME); chrono-trigger-save-editor and snessum for the sum, checked against real saves |
| Golden Sun, The Lost Age | USA/Europe, Germany, France, Spain, Italy | the leader (Isaac, Felix) and level as the file screen shows them · clock | 64 KiB flash of records with "CAMELOT", the file's number (0-2), a byte sum from 0x10 (the first game leaves out the last 8 bytes of its 0x1000) and a count of saves; the higher count of one number is the newer, a damaged one gives way to the older. Name, level and the clock in frames at the file's start, where the file screen reads them (Shmargus/GSRecomp, from the games' routines; the games' own file screens showed a test save's name, level and clock from these places) |
| Super Mario RPG | USA | Mario's level (the game keeps no clock) | four slots of 0x800 bytes in the first 8 KiB (snes9x writes 32 KiB): the byte sum's complement at 0x7FC, 0xFF first in a free slot, the slot chosen last at 0x7FE; Mario's level the slot's first byte (Yoshifanatic1's disassembly). Checked: a save the game wrote, and a made one the game's file select shows as written |
| Lufia II: Rise of the Sinistrals | USA | the hero and level · clock (another leader: the clock alone, only the hero's name is saved) | four slots of 0x800 bytes, scrambled from 4 with the game's random numbers (seed at 1), the words from 4 plus 0x6502 at 2; a free slot's first byte 0xF0, the slot saved last in slot 0's first byte; name 0x3C8 (back to front), leader 0x3D1, clock 0x3F6, levels 0x5F1 + 0x3D n doubled (the game's routines from the ROM, Lufia2Decomp). Checked: saves the game wrote, and a made one its slot list shows as written |
| Pokémon Mystery Dungeon: Red Rescue Team | USA | the leader (its nickname) and level · clock, as the Continue screen shows them | 128 KiB flash: the save at 0 and again at 0x6000 (0x57D4 bytes), the sum of its 32-bit words from 4 at 0 and the magic 0x05071412 at 0x414, the second copy only when the first one's sum fails; from 0x448 the recruited Pokémon as bits (lowest first), 323 per record (level, then the name from bit 243), the leader's number at bit 134791; the clock in the adventure's bits at 0x4EE0 (pret/pmd-red). Checked: a save the game wrote, and that save with a changed name and clock, which its Continue screen shows as written. Europe left out: the decomp is of the USA ROM |
| Final Fantasy VI Advance | USA | the active party's leader and level · clock (a leader not named yet, "??????": the clock alone) | 32 KiB: three slots of 0xA00 bytes from 0, each with its byte sum at 0x9FE (an unused slot is zeros), "FINAL FANTASY VI     ADVANCE" at 0x1F00, the slot saved last (1-3) at 0x1FF0; the characters (0x25 bytes, name +2, level +8), their party bits at 0x250, the active party at 0x6F5, the clock at 0x263 (Janne Hellas' save importer, FF3h). Checked: saves the game wrote, and one with a changed name, level and clock its Load Game screen shows as written |
| Tactics Ogre: The Knight of Lodis | USA | the hero and level · clock, as the file screen shows them | 64 KiB flash in 16 sectors: a save is a sector starting "TOGQST02" (count at 8, CRC-32 at 0xC over both parts with the CRC and links as zeros, file 0-2 at 0x11, the second part's sector at 0x12); the highest count is the newest; the file screen's summary: name 0x14 (8 letters), level 0x24, clock 0x2A (jiangzhengwenjz/totkol). Checked: a save the game wrote, and one with a changed name, level and clock its file screen shows as written |
| Breath of Fire II | USA | the hero and level, as the file select shows them (the clock is left out: no save the game made has checked it) | three slots of 0xAA0 bytes, each the sum of its words from 2, then "SAN!BOF2" (a free slot has no mark); characters from 0x1E0, 0x40 bytes (name, the character at +6, level +7), the first Ryu (0, or 9 in the prologue) is the hero (the game's routines read from the ROM). Checked: a save the game wrote, and one with a changed name and level its file select shows as written |
| Mario & Luigi: Superstar Saga | USA | the leading brother and his level · clock, as the LOAD screen shows them | 8 KiB EEPROM (mGBA keeps its 8-byte blocks back to front; both orders are read): "MALRPG", the files in use as bits at 6, three files of 0x6F8 bytes from 0x10 with their number, an 8-bit sum and "AD"; from 4 the leader bit, Mario's level 0x4C, Luigi's 0x88, play time in frames 0x6EC (jellees/mlss; the levels' places found with a save the game wrote, whose LOAD screen showed the values written there). Checked: saves the game wrote and one with changed levels |
| Mother 3 | the English fan translation v1.3a (HQ Sound) of the Japanese game, which keeps its save code | the leader and level · clock, as the Load screen shows them | 64 KiB flash in banks of 0x1000 bytes (2, 0x334D, words add up to 0xFFFF); the game's own bank 0 (mirror 1): the file saved last at 4, per file a summary of 0x64 bytes from 8 (time in sixtieths +0, name +0x36 in the translation's letters, ASCII less 0x20, level +0x46) and its mark at 0x68/0xCC; the file's banks 4-6/10-12 with copies after them, one copy whole (Data Crystal, Kurausukun/mother3). The Load screen reads the summary, not the file: found when a changed file left the screen as it was and a changed summary changed it. The Japanese ROM writes its own letters and isn't listed. Checked: saves the game wrote and one with a changed summary |
| Kirby & the Amazing Mirror | USA | mirror shards of 8 · marks | blocks with an 8-byte check (word sum, complement, negative, 0), each twice, the first copy if good; from 24 the three files' worlds (0x30C bytes, the sizes of the ROM's own table), the shards' bits 0x10 into the treasures (jiangzhengwenjz/katam `save.c`, `treasures.h`; checked against a save the game wrote). The file select's percentage comes from tables in the ROM and is never saved, so the line counts shards. A never-played file is all zeros and says nothing |
| Banjo-Kazooie | USA (Rev 0–1), Europe, each `.z64` and byte-swapped | Jiggies · notes · the game's clock, as the game select says them · a bar of 100 | the EEPROM: four files of 0x78 bytes (0x11, the game 1–3, Rare's sum at 0x74, banjo.decomp `savedata_init`); the Jiggies' bits from 2, the nine worlds' best notes as 7-bit fields at 0x22, the eleven places' seconds at 0x2A. Saving moves a game to the spare file; the first good file of each game counts, as in the game select. Checked against saves the game wrote (USA Rev 1) |
| Metroid Fusion | USA | item share · clock, as Zero Mission · a bar | three files of 0x1200 bytes from 0x200, each followed by its copy, "MetroidEpisode4A", 32-bit word sum at 16 and its complement (as in Zero Mission); the file chosen last in the 0x7000 record ("SELECTSAVENUMBER", file at 22); `struct Equipment` at 0xC4, clock at 0xE4 (metroidret/mf). The share is the ending's: tanks only (energy over 99 by 100, missiles over 10 by 5, power bombs over 10 by 2). The file select shows energy and clock; both checked against saves the game wrote with its own save routine (17.11.). A file made at a new start and never saved says nothing |
| GoldenEye 007 | USA, Europe, each `.z64` and byte-swapped | the furthest mission done at the hardest difficulty done, as the folder select names them ("Mission 2.i: Surface · Secret Agent"), or "007" alone once that is open | the EEPROM: five saves of 0x60 bytes from 0x20 with Rare's two sums (type A) over bytes 8 on; byte 8 the folder, the save's turn and "free"; best times as 10-bit fields from 0x12, (difficulty × 20 + mission) × 10 (n64decomp/007 `file2.c`, `front.c`). Which folder was played last isn't kept: the one that changed. Checked: the game loaded our folder and showed "Secret Agent / Mission 2.i", then copied it in its own folder select (17.11.) |

Not in it: Japanese releases (their saves are laid out differently), re-releases (Switch, Virtual Console), betas,
demos, pirate carts, the GBA ports. The layouts were taken as facts from PKHeX, the pret disassemblies, the Super Metroid
community disassembly, SMWDisX and snesrev/zelda3 (sources at the top of `src/case_knows.rs`); no code was copied.

## Words

Each part of the line is a line of the translation tables (`{n} badges`, `{s} Lv. {n}`, `game clock {n}:{n2}`, `{n}% items`,
`{n} of {m} exits`, `{n} of {m} pendants`, `{n} of {m} crystals`, `{n} hearts`, `No badges yet`, `1 badge`): German
"8 Orden · Glurak Lv. 45 · Spieluhr 31:12", Spanish "8 medallas · Glurak Nv. 45 · reloj del juego 31:12". A Pokémon's name is the game's
own, in the game's language, in the case a sentence uses ("CHARIZARD" → "Charizard"; a name with a small letter is the
player's and stays). The line is painted as it is (`Text::draw_name`), broken between its parts, never inside one
(`case_knows_read::lines`).

The game's clock is always named as the game's ("game clock 31:12", "Spieluhr 31:12"), never a bare time beside
pocket-ui's own "Played": a game whose save shows progress but that pocket-ui never timed says "Played: Not here yet"
(`CardData::not_played`) on the box, the card and the info page, not "Not yet".

## Pictures

Under the line, small and quiet, in the pixels of the game itself:

- **Pokémon Ruby, Sapphire, Emerald, FireRed, LeafGreen:** the team's party icons (the first frame, each standing on
  one line, an egg as the game draws it) and the eight badges of the trainer card, the won ones bright, the others
  faint. Both are read **at run time from the player's own ROM** (`src/case_knows_art.rs`); nothing of a game is in
  pocket-ui, its package or this repository, only numbers to find them by: the 440-byte table of icon palette numbers
  that all five games share (its length and CRC-32; the icon pointers lie right before it, the three palettes' table
  right after), the badge sheet by the CRC-32 of its 1024 bytes unpacked (LZ77 in Emerald and FireRed, plain in Ruby
  and Sapphire) and its palette by its own. Every pointer, tag and sum is checked; what isn't found leaves its row
  out. Each Pokémon's species comes from its scrambled data, unscrambled as the game does and checked with the
  Pokémon's own checksum; one that fails leaves the whole team row out, the line stays.
- **Pokémon Gold, Silver, Crystal:** the party menu's icons (39 shapes the species share, an egg as the game draws
  it) in the party menu's own colours, and the trainer card's eight Johto badges in its colours, the won ones bright
  (the line counts Kanto's too). Found by the code that uses them (`ReadMonMenuIcon`, `LoadOverworldMonIcon` and the
  trainer card's `ld de, BadgeGFX … 44` as pret's pokecrystal names them; a few opcodes, `case_knows_art::find_gb`),
  then checked: the 251 icon numbers, the 39 icons in table order and the 44 badge tiles each by its CRC-32, the
  palettes by theirs. Gold and Crystal (USA, Europe) have the same pictures; other releases show them when they check
  out, else the line alone. Red, Blue and Yellow: the line alone.
- **Pokémon Diamond, Pearl, Platinum, HeartGold, SoulSilver:** the team's party icons (the first of their two frames,
  an egg as the game draws it). Read part by part from the player's own ROM, never the whole file
  (`case_knows_art::find_nds`): the header, the file system's name and allocation tables (NitroFS), the icon archive
  (`poketool/icongra/poke_icon.narc`, Platinum `pl_poke_icon.narc`, HeartGold and SoulSilver `a/0/2/0`; a NARC whose
  file 0 is the palettes, file 7 + species the icon, 4 bits a pixel), and the icons' palette numbers from the game's
  code: one byte per species, the egg and Manaphy's egg, 496 bytes found by their length and CRC-32 (the same in
  Platinum, HeartGold and SoulSilver, as pret's pokeplatinum and pokeheartgold have them) in the main program, unpacked
  with the DS's backward LZ when it is packed (`case_knows_art::blz`), else in the overlays. The numbers are kept in
  the cache line with the archive's place (the code may be packed). A ROM where they aren't found (a hack that recoloured
  its icons) shows the line alone. The DS games' badges are counted in the line, not drawn; Black and White show the
  line alone.
- **Super Metroid** (items of 100) and **Super Mario World** (exits of 96): a slim bar.
- **A Link to the Past:** the pause menu's own pendants (red, blue, green, or the empty one) and, after Agahnim, its
  seven crystals, on a dark plate like the menu's window (the pictures are made for a dark ground: a white chain).
  Read from the player's ROM: the HUD's third 2-bit sheet (pack 105) unpacked with the game's own packing
  (`case_knows_art::zelda_unpack`, as snesrev/zelda3 documents it) and the HUD palette, each found by a first word and
  its checksum (the sheet's once unpacked), so any release with the same pictures is found; the tiles and palettes are
  the ones `Hud_DrawProgressIcons` draws. Where they aren't found: three or seven plain marks.

The pictures are drawn at a whole multiple of their pixels (2 at the normal text size, `case_knows_art::scale`), on the
box's back, the card page and the info page; "Where was I?" shows the team alone. The places in a ROM are found once
(the ROM read through on the reading thread, never while a game runs: 11 to 16 ms on the build computer once the bytes
are in memory) and kept in `<data>/identify/case-knows-art.tsv` by the ROM's size and time; then a picture is a few
hundred bytes read where they say. `examples/case_knows_bench.rs` times it on a device.

**Rights.** Like an emulator drawing the game, pocket-ui shows the player's own ROM's pictures on the player's own
device; it copies nothing anywhere and ships none.

## Where it is painted

| Place | Code | |
|---|---|---|
| The box's back, right under the title (above the pictures, seen without scrolling; not again under "Your game") | `game_box::paint_back_body`, `paint_progress` | across the back, at most two lines |
| The card page, first line under "Your game" | `game_card` | the same |
| The info page (Split) | `game_info::your_rows` (`Val::Line`) | after the status; in Easy first |
| Beside Continue: the line of a recent game on Home, and of a game whose case stands open | `App::describe` | it takes the place of "3 days ago, 12 h played"; Rediscover's reason stays |
| "Where was I?" | `where_was_i::Card::progress`, `app_where_was_i` | it takes the place of the time played: one clock on the card |

## Adding a game

1. Its No-Intro CRCs into `RELEASES` (one `Kind` per save layout, a `Lang` for the letters of names); a DS game its
   game codes (`ds`).
2. A reader in `case_knows.rs`: the game's own check first, then only values the game can have. Every field from a
   documented source, named in the module's header.
3. A save made byte by byte in the tests (`case_knows::tests`), as the game writes it: the line it gives, a changed
   byte that gives nothing, a value out of range that gives nothing. No one's real save in the repository.
4. Its words in `Progress::parts` and both tables, and the table above.

## Status

Tests: the readers on saves made in the tests (`case_knows`), the files, cache and languages (`case_knows_read`).
Snapshots of the box's back, the card, Continue's line and "Where was I?" with made-up progress
(`POCKETUI_SNAPSHOT_PROGRESS`).
