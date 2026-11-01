# Send from your phone: the upload server, its routes and threat model

Settings → Games → Add games → **Y** (and the first start's games step) opens "Send from your phone": a QR code of
`http://<Wi-Fi address>:<port>/<code>/`, the address to type, and live status. The phone opens the page, picks files,
and they stream onto the SD card. User guide: [user/phone-upload.md](user/phone-upload.md).
Code: `src/phone_upload.rs` (server), `src/phone_upload_page.rs` (the Pocket's page), `assets/phone-upload/` (the
phone's page), `src/setup.rs` (`Step::Phone`).

> **Status** (ledger: [STATUS.md](STATUS.md)): merged and shipped in 0.1.10. The security review's findings (M1–M6,
> L1–L3, the recheck's M6 bypass, N1, N2) are fixed, each with a test. On an RG35XX Plus with curl over the Wi-Fi
> (2026-10-31, `examples/phone_upload_dev.rs`): 403 without the code, with a wrong Host and a foreign Origin; routing;
> `.doom`, an outside-pointing cue, oversize refused without making folders; connections without the code capped;
> 100 MB at a flat 3.4 MB RSS; the port closed afterwards. Still open: the Pocket's page itself, closing on sleep and a
> real phone (iOS Safari, Android Chrome) on hardware.

It is not the [web companion](WEB-COMPANION.md). The companion stays Tailscale-only and unchanged; this is the first
thing pocket-ui serves on the Wi-Fi LAN, so it is kept as small as it can be.

## Lifetime

| When | What happens |
|---|---|
| Page not open (always, by default) | Nothing: no thread, no socket. `Setup::phone_listening` is false; the tests check that the port is closed after the page is |
| Y on Add games | `Server::on_wifi`: the interface that holds `wifi::ip()` (never `tailscale*`, never loopback), a socket bound to that address with `SO_BINDTODEVICE`, port 0 (the kernel picks), a fresh code, one listener thread; registered for `phone_upload::sleeping` |
| B on the page | `Server` is dropped: the stop flag, the listening socket shut, every open connection shut down (a read waiting for the phone ends at once), the listener thread joined (≤1 s). The file under way loses its temporary file and the folders it made |
| The Pocket goes dark | `sleep` calls `phone_upload::sleeping()` at the start of light sleep (screen off after a while, the lid), of the power key's screen-off and of every suspend: the page closes there, before the screen is off, and its code is gone. A suspend that slipped past is caught afterwards by `CLOCK_BOOTTIME − CLOCK_MONOTONIC` growing by more than 2 s. The page says "Closed while the Pocket slept"; A opens it again with a new code |
| 10 min without a request that had the code | Closed by itself ("Closed after 10 minutes"). Connections without the code never count, open or closed |
| 60 min after opening | Closed by itself ("Closed after an hour"), whatever happens: no client keeps it open longer, not even one sending a byte a minute |
| 30 wrong codes or broken heads | Closed by itself ("Closed: someone was guessing") |
| A file arrives | Auto-sleep waits (`App::sleep_ctx`); every read waits at most 60 s, and the hour above bounds the rest |

A game can't run while the page is open (the setup is in front), so nothing is reachable during play.

## Routes

Everything is under `/<code>/`. Any other path, or a wrong code, gets `403 Forbidden` with that one word as the body
and nothing else (no JSON, no hint which part was wrong). The code is compared in constant time. A request with the
code must also name this page as its Host (`<address>:<port>`, so DNS rebinding gets nowhere), a browser's Origin, when
it sends one, must be `http://<address>:<port>`, and its `Sec-Fetch-Site`, when sent, `same-origin` (or `none` for
opening the page from the camera): a page of another site that learned the code can't post files with it.

| Route | What it does |
|---|---|
| `GET /<code>/` | The page (`index.html`), with the code in its two asset paths and the systems that take uploads in a `<script type="application/json">` block (id, name, folder, the file types this page takes for it and its size limit) |
| `GET /<code>/app.js`, `/<code>/app.css` | The page's script and style (the security headers allow no inline script or style) |
| `POST /<code>/check?name=&size=&system=&dir=` | Where the file would go, whether it fits, whether a file of that name is there: `{ok, path, system, exists}` or `{ok:false, error}` with the status of the reason. Makes nothing on the card |
| `POST /<code>/upload?name=&system=&dir=` | The file as the raw body with `Content-Length` (no multipart, no chunked encoding) |

No listing, no download, nothing read from the card; `check` answers only about the one name it is given.

At most eight connections at once, of which at most four that haven't shown the code yet (a browser opens three for
the page); more get `503`.

## Files

- **Which systems and types** (`ALLOWED`, security review M1): only systems whose games are plain data an emulator
  reads, and per system only the game file types that are both there and in es_systems.cfg: cartridges (Game Boy to
  N64, `.zip` of them), the DS, arcade sets (`.zip`, `.7z`), disc systems (`.iso .chd .cue .bin .gdi .cdi .m3u .rvz`
  …), the 3DS. Never: DOOM (`.doom` hands its launcher files, folders and options), Daphne (`.singe` is a Lua script),
  ports, PICO-8 and other interpreters, DOS and home computers, ScummVM, music and video. A system not in the list
  isn't offered and can't be chosen.
- **Name**: `files::sanitize_name`: the last part of what was sent, control, invisible and look-alike characters and
  `\ / : * ? " < > |` replaced, no leading dots or spaces, at most 200 bytes. Unicode names stay as they are.
- **Place**: the system from the query when the phone chose one (it must be one that takes the type), else the one
  system whose types fit, else the one the type usually means (`PREFER`: `.sfc` → snes, `.gb` → gb); otherwise refused
  with "choose one" (the phone's page asks first). Never through a link out of `roms`.
- **Disc folders**: `dir` (one cleaned name, no leading `_` or `.`) is kept only for disc files: `roms/psx/<dir>/<name>`,
  one level, which is what the library reads. A cartridge picked in a folder goes straight into the system's folder.
- **Disc sheets** (`.cue`, `.gdi`, `.m3u`; security review M6): at most 64 KB, read whole before anything is kept;
  every file they name (`FILE` and `CDTEXTFILE` in a cue, every field of a gdi that isn't a number, also on its first
  line, each line of an m3u) must be a plain name beside the sheet: no `/` or `\\`, no `..`, no drive, nothing hidden,
  not a link that is already there; a playlist may not name another playlist or a `.exe`. Otherwise refused, nothing kept.
- **Marker endings** (review recheck M6): PCSX-ReARMed treats any path containing `.m3u` as a playlist and any
  containing `.exe` as a PS-EXE (`strcasestr`), and looks for `.cue`/`.toc`/`.ccd`/`.mds`/`.sbi` beside a game. So a file
  or disc-folder name holding `.m3u .cue .gdi .ccd .toc .mds .sbi .cbn .pbp .exe` anywhere (any case) is refused,
  except as the file's own last ending where it is a type this page takes and checks (`Alias.m3u.bin`, a folder
  `Album.m3u` are refused). The other cores in the tree (mGBA, mupen64plus-next, Nestopia, snes9x, libretro-common)
  compare the real last ending; checked by grep of their sources, not against the cores on a device.
- **Refused**: saves (`.srm .sav .rtc .state* .mcr .eep .fla .sra`), BIOS files (hostsys's list and the usual
  `scph*.bin`, `gba_bios.bin`, `bios7.bin`, …), a file that starts like a Linux program.
- **Size** (security review M5): per system: cartridges and their zips 64 MB (the most pocket-core unpacks into
  memory), the DS 512 MB, arcade sets 1 GB, discs 9 GB (`files::MAX_ROM`), 3DS 4 GB. The loader is bounded too, for
  every game however it got onto the card: `archive::extract_rom` refuses a zip over 128 MB by its size before reading
  it, and a game handed to a core in memory is refused over 512 MB (`archive::read_bounded`). **What remains:** cores
  that read the file themselves (discs, arcade sets: `need_fullpath`) and standalone emulators decide alone how much
  they read; a large disc image is normal there.
- **Space** (security review M4): nothing is made on the card before the code, the type, the size, the free bytes (the
  card's free space less `files::RESERVE`, 256 MB, with uploads under way reserved first), the free file entries
  (`INODE_RESERVE`, 20 000, where the file system counts them) and this page's own count (`MAX_NEW_ENTRIES`, 2000 files
  and folders) are through. A refused or cut upload removes the folders it made, but only where no other upload under way uses them (each upload
  claims its folders under one lock, review recheck N1).
- **Writing**: a temporary file in `roms/.pocket-ui-uploads` (`.<name>.<pid>-<n>.upload`, created exclusively, no link
  followed; the folder is pocket-ui's own and nothing else writes there), 256 KB at a time, synced, then
  `renameat2(RENAME_NOREPLACE)` into place. An existing file is never replaced (there is no overwrite here, unlike the
  companion): the phone says it is there. A temporary file an earlier run left behind is swept from that folder, and
  only from there (security review L3): a file of the same pattern in a game folder is never touched.
- **Memory**: one 256 KB buffer and a 64 KB reader per request, at most eight requests: a 4 GB image costs no more RAM
  than a 4 MB one (the RG35XX family has 1 GB and no swap).

After the page closes the grid counts the folders again; leaving Settings reads the library, so the games are on the
shelf.

## Threat model

Who can reach it: anyone on the same Wi-Fi, for as long as the page is open, who has the code. The code is 8
characters from a 31-letter alphabet (≈40 bits) read from `/dev/urandom`, new per opening; 30 wrong tries close the
page. The connection is plain HTTP: someone on the same Wi-Fi can read the code and the files on their way, and could
change them. Both pages say so in plain words (security review L2).

| Threat | Answer |
|---|---|
| Someone on the Wi-Fi reads the code (shoulder, or sniffing plain HTTP) | They can add game files of the allowed types while the page is open, under the rules above. They can't list, download, delete or replace anything (there is no overwrite) |
| Guessing the code | 30 tries (wrong codes and broken heads), then closed; the 403 says nothing |
| A file a launcher or interpreter runs (`.doom`, `.singe`, scripts, ports) | Not taken: allow-list per system |
| A disc sheet pointing elsewhere (`FILE "/…"`, `../`) | Refused: sheets may name only files beside them |
| Path traversal, odd names, links | Cut to the last part, cleaned, links refused (`files::inside`, `rename_noreplace`, folder re-check before the rename) |
| Filling the card (bytes, file entries) or RAM | Per-system size limits, free-space and entry reserves, a count per page, streaming, bounded loaders |
| Keeping the page open without the code | Doesn't work: only requests with the code count; four such connections at most; an hour at most in any case |
| A website in the phone's browser posting with a leaked code | Host, Origin and Fetch Metadata must be this page; `no-referrer` keeps the code out of Referers |
| Listening when it shouldn't | Only while the page is open; closed on B, when the Pocket goes dark, idle, after an hour; bound to the Wi-Fi interface, never `0.0.0.0` |

Not covered: an attacker who has the code can add game files of the allowed types (up to the limits), which then show
on the shelf, and `check` tells them whether a file of a given name is in a game folder (one name per request). An
emulator's own handling of a hostile game file is the emulator's. That is the price of plain HTTP without pairing; it
is why the page is short-lived, closes when the Pocket goes dark, and says so.

## Checking it on a device

`curl` from a computer on the same Wi-Fi, with the address the page shows:

```sh
curl -s -o /dev/null -w '%{http_code}\n' http://<addr>:<port>/                          # 403
curl -s -X POST "http://<addr>:<port>/<code>/check?name=Game.gba&size=1000"              # {"ok":true,...}
curl -s -X POST -T Game.gba "http://<addr>:<port>/<code>/upload?name=Game.gba"
```

Then B on the Pocket and `ss -ltn` on the Pocket: the port is gone.
