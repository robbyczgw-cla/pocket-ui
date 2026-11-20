# RomM as a source of games

Settings → Games → **RomM** (user guide: [docs/user/romm.md](user/romm.md)). The games on the player's own
[RomM](https://romm.app) server stand on the shelf beside the card's; a game downloads at its first start and is a file on
the card from then on. Shared code for Linux (ROCKNIX, the Brick, dArkOS) and Android's handheld view.

| Module | What it does |
|---|---|
| `src/romm.rs` | The account and token, pairing, the list (`fetch_catalog`, `catalog.json`), the shelf (`shelf_games`, `names_in`), the download (`download`), covers (`fetch_cover`), the player's words for each failure (`Failure::words`) |
| `src/romm_net.rs` | The requests: `Net::call` (one question, one answer) and `Net::fetch` (one file appended to its part, from its length on). `Curl` on Linux, `AndroidNet` on Android (`RommHttp.java` for `https://`, `Plain` for `http://`) |
| `src/romm_page.rs` | Settings → Games → RomM's state: rows, keyboard, the pairing view, what a press asks for |
| `src/app_romm.rs` | The App's side: threads, painting, snapshots, the list's refresh, the start's download (`RommFetch`) |
| `src/romm_tests.rs` | Every path against a made-up server |
| `tests/romm_server.rs` | The same flows against a real RomM (by hand, see Tests) |

## Pairing

RomM 5.0 and newer: RomM's device flow. `POST /api/auth/device/init` (open) gives a `device_code`, an 8-letter
`user_code` and `verification_path_complete` (`/pair/device?user_code=…`): the Pocket shows the code and a QR code of
that page on the typed server. The player approves it there, signed in to RomM; meanwhile the Pocket asks
`POST /api/auth/device/token` every `interval` seconds (`authorization_pending`, `slow_down`, `access_denied`,
`expired_token`) until a token comes. RomM registers the Pocket as a device (`client_device_identifier`: a random id made
once, `<data>/romm/device-id`; name `pocket-ui (<model>)`).

RomM 4.8 and 4.9: the 8-digit code of a Client API Token's *Pair* (`POST /api/client-tokens/exchange`). A server without
the device flow answers its start with 404/405; when `/api/heartbeat` answers, the page says to pair with a code.

Asked for: `me.read roms.read platforms.read assets.read` (`romm::SCOPES`): read only. The token goes in
`<data>/romm/token` (0600, `secret::write_private`), the server and user in `account.json`. The token is never logged,
never in another file, never on a command line: curl reads its whole request from stdin (`-K -`).

## The list and the shelf

`fetch_catalog`: `GET /api/platforms`, then `GET /api/roms?platform_ids=<id>&limit=500&offset=…&with_files=true` per
platform this Pocket has a system for. The platform's `fs_slug`, `slug` and name go through the folder names other cards
use (`library::system_for_folder`, `layouts::system_ids`), plus a few of RomM's own (`EXTRA_SLUGS`). Never on the shelf:
Switch and PC/phone platforms (`NEVER`), files of a type the system doesn't take, a game whose folder has folders in it,
names that aren't plain file names. They are counted (`Catalog::skipped`).

`library::load_with` (the scan behind the shelf, its cache and every list) adds each system's RomM games that are not on
the card (`shelf_games`), at the path their download will have: `<system folder>/<fs_name>`, or
`<system folder>/<folder>/<start file>` for a game of several files (the start file: `.m3u`, `.cue`, `.gdi`, … ,
`start_file`). Favourites, history, hidden games, covers and sync key the game the same before and after the download,
and nothing moves when it lands. A game on the card with a RomM game's file name keeps RomM's name (`names_in`). A game
already on the card under the same file name (any case) is not listed twice.

The list is read again after pairing, from the page, and 40 s after each start when paired, online and no game runs.
After a read, RomM's covers go into the cover cache (`covers::cover_path`) for games without one: PNG as it is, JPEG
turned into PNG, anything else (WebP) left to the libretro box art. RomM's description of a game goes on its card when the player's gamelist has none (`Summary::Romm`, "From your RomM server"; a Wikipedia text on hand comes first).

## The download

A start of a RomM game hands a `RommFetch` to the launch's preparation (`app_launch::prepare`), which runs beside the start's
scene. Unless the file is there, `romm::download`:

1. checks the free space (the bytes still missing plus 64 MB, `RESERVE`) — `Failure::NoRoom` says how much more;
2. fetches into `<system folder>/.pocket-ui-romm/<id>/<file>.part`, appending from the part's length (a range request),
   so a broken-off download goes on where it stopped (`GET /api/roms/<id>/content/<file>`; one file of a folder game
   with `?file_ids=<file id>`, which serves it as it is instead of a zip);
3. checks each file's size against RomM's;
4. renames the file (or the finished folder) into place, never over something that is there.

The bar ("Getting the game from RomM, once", "3 MB / 8 MB") is `app_loading`'s, as for Java's first load; B stops it
(`romm_press`). A failure ends the start with its words as the note (`Outcome::Empty`).

## Android

`usesCleartextTraffic="false"` keeps Java's HTTP from plain `http://`, which is what a RomM at home usually is
(`http://192.168.x.y:8080`). So `http://` goes through `romm_net::Plain` (std `TcpStream`, HTTP/1.1, lengths and chunked
bodies, redirects only within the same host and plain HTTP), and `https://` through `RommHttp.java` (Android's
certificate store). Binary answers (covers) go through `fetch` into a file, never through the bridge's text.

## Tests

- `cargo test --lib romm`: pairing, the list (pages, mapping, folders, skipped), the shelf and the library scan, the
  download (whole, resumed, folder, already there), every failure and its words, covers.
- `tests/romm_server.rs` (`#[ignore]`): against a real RomM with only homebrew and dummy files, never real games:
  `docker compose` with `rommapp/romm` and `mariadb`, a library of `roms/gb`, `gba`, `nes`, `psx/<folder>`, a test user
  that approves the pairing. `POCKETUI_ROMM_URL=http://<host>:18080 POCKETUI_ROMM_LOGIN=<user>:<password> cargo test --test
  romm_server -- --ignored`. It pairs by the device flow, reads the list with curl and with `Plain` (the same), downloads
  every game both ways (the same bytes), resumes from half a file, fetches a cover, and checks a server that is gone, a
  wrong token, a wrong pairing code, an address that isn't RomM, and a token revoked in RomM. Removing the device alone in
  RomM 5.4 leaves its token working; revoking the token is what unpairs.

## Not yet

- Saves and states to and from RomM (`/api/sync/negotiate`, `POST /api/saves`): a second step, only ever adding, never
  replacing a save on either side.
- A game whose folder has folders in it, and games that are archives RomM unpacks.
