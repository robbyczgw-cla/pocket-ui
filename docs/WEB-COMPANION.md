# Web companion: protocol, endpoints, threat model

The web companion serves the player's library to a browser on the tailnet: browse and search, edit status, notes,
stickers, favourites, smart shelves and the preferred version, upload games, BIOS files and PortMaster files, play
time, the battery history and a screenshot. User guide: [user/web-companion.md](user/web-companion.md). Code:
`src/web/` (library), `src/web_page.rs` (Settings → Connections → Web companion's state), `src/app_web.rs` (the App's side).

> **Status** (ledger: [STATUS.md](STATUS.md)): built and tested on a desktop (unit tests, the API end to end over real
> sockets with a stand-in tailscaled and a stand-in App, the page in headless Chromium against `examples/web_dev.rs`).
> **Smoke-checked on a Mini V2 over a real tailnet** (2026-10-24, after the review fixes): see
> [Device checks](#device-checks-partly-done), where the rest is still open.

pocket-ui runs as root. The companion is therefore built small, with no new crate (std, `libc`, `sha2`, `base64`,
`serde_json`, `png`, all already in the tree) and no shell anywhere.

## Threads and what it costs

| State | What runs |
|---|---|
| Off (the default) | Nothing: no thread, no socket. `App::web_sync` returns at its first line; `web::Shared` is not even made until the Settings page opens |
| On, no browser | A supervisor thread in `poll` on its listening sockets and a wake pipe, waking every 30 s to compare the Tailscale addresses (one `getifaddrs`); three workers blocked on a channel. All four at nice 19 and the idle I/O class (`web::lower_priority`) |
| On, a browser | One worker per request (`Connection: close`, one request per connection). Reads come from the App's files and a library cache (five minutes; read again after an upload). Edits go to the App as `UserEvent::Web(Event::Edit)` and are applied with the same functions a button press uses; the worker waits at most 3 s for the answer |
| A game runs | Everything but `/api/session`, `/api/status`, `/api/screenshot` (one `grim` capture per 10 s at most, otherwise the last picture again) and `/api/pair` answers `503 {"playing": true}`. An upload under way pauses between 256 KB chunks until the game ends (and gives up after 30 minutes) |
| Switched off | The listening sockets close at once; `Server::stop` waits for the supervisor at most 1 s (it never blocks: the queue-full `503` is one non-blocking write) and never for a worker. Workers close what is still queued unanswered, an upload stops at its next chunk and removes its temporary file, an edit still on its way to the App is answered "off" and not applied |
| On, but the port can't be opened | The supervisor keeps the reason (`Shared::problem`: port 80 in use, not allowed) and tries again every 30 s; Settings → Connections → Web companion shows **Not listening** and the reason instead of "Waiting for Tailscale" |

The UI thread's share: `web_sync` per loop turn, which calls `WebUi::turn` (start when due, then a few comparisons;
stores into `Shared` only when the game, Curator's index or the private-screen flag changed; a start that failed is
tried again after 30 s, never per frame), the edits themselves, and, after a game upload, swapping in the library the worker has
already read (`Edit::LibraryChanged(Systems)`), only on Home or the Settings list and never in a game. The server
starts `START_AFTER` (3 s) after pocket-ui, never in the way of the first frame. Settings → Connections → Web companion redraws once
a second only while the pairing code is shown.

## Where it listens

`net::plan` takes the addresses of interfaces whose name starts with `tailscale` and whose address is in Tailscale's
ranges (100.64.0.0/10, fd7a:115c:a1e0::/48), port 80. Never `0.0.0.0`/`::`, never loopback, never the Wi-Fi: there is
no switch for any other network. Each socket is bound to its address **and** tied to its interface with
`SO_BINDTODEVICE` (`net::bind`): on Linux's weak host model, a packet for 100.x arriving on wlan0 would otherwise be
accepted. Every accepted connection is checked again (`net::peer_allowed`: a Tailscale address) before a worker sees
it; refusals are counted in the log. A CGNAT address that a phone hotspot hands to wlan0 is in the same range and is
ignored because of the interface name. When Tailscale comes up later or its address changes, the supervisor notices
within 30 s; the plan includes the interface's index, so a `tailscale0` made anew by a restarted tailscaled with the
same name and address is listened on anew too (a socket tied to the old index would hear nothing).

## Who gets in

1. **Host check** (DNS rebinding): `Host` must be one of the Pocket's names from tailscaled (host name, MagicDNS name,
   its first label) or an address it listens on; otherwise `421`.
2. **Principal** (`api::Ctx::principal`):
   - a paired browser: cookie `pocket_session` (32 random bytes, base64url; `HttpOnly; SameSite=Strict; Path=/`,
     a year). The Pocket keeps only SHA-256 of the token in `<data>/web/browsers.json` (folder 0700, file written
     atomically), with a public id, a name from the User-Agent (and the tailnet device's name) and dates. At most 16
     browsers; pairing a 17th forgets the one unused longest. Revoking deletes the entry. An entry paired more than a
     year ago (`auth::SESSION_LIFETIME`, the cookie's `Max-Age`) is refused and forgotten on the Pocket too. Every
     cookie of that name the browser sends is tried, so one set for a wider domain by another site can't hide it.
   - the owner's device, when Settings → Connections → Web companion → Your Tailscale devices is **Open** (the default): tailscaled's
     LocalAPI `whois` for the peer address (over `/run/tailscale/tailscaled.sock`, HTTP/1.0, 2 s limit, answers kept a
     minute, dropped when "Your Tailscale devices" changes) must name the same user id as this node's `status`, neither
     of them tagged (`net::owner_allows`). A device shared into the tailnet by another user, or a tagged one, needs a
     code. The unit of trust is the Tailscale **account**, not the device: every untagged device of the owner's
     account gets in (the manual says so), and revoking a paired browser on such a device does not lock it out while
     the switch is Open.
3. **Pairing** (`auth::Pairing`, `POST /api/pair`): Settings → Pair a browser makes a six-digit code from
   `/dev/urandom` (rejection sampling, uniform), valid five minutes, for one browser. Five wrong tries end it; leaving
   the page ends it. Tries are answered one at a time and at most one a second, whichever connection they come from
   and whether a code is showing or not (the check holds a lock across the wait). A wrong code and a try without a
   code get the same answer (`api::NOT_PAIRED`, no count of tries left), so nobody can watch for the moment a code
   appears. The QR code holds `http://<name>/#pair=<code>`: the fragment never reaches a server or a log.
   `GET /api/session` does not say whether a code is showing.
4. **CSRF**: every `POST` needs `X-CSRF-Token` = the first 16 bytes of HMAC-SHA256(key, principal) in hex, where the
   key is 32 random bytes in `browsers.json` that never leave the Pocket. The page gets it from `GET /api/session`,
   which another site can't read (no CORS headers are ever sent, so no preflight passes either).
5. **Origin**: a `POST`, and any `/api/` request, is refused (`403`) when `Sec-Fetch-Site` is present and not
   `same-origin`, or `Origin` is present and not this host. This also keeps a page of another site, open in the
   owner's browser (which needs no cookie), from causing a screenshot or reading anything.

## HTTP

`http.rs`, std only: one request per connection; the head at most 16 KB and 64 header lines, all of it within 15 s
of the accept (`HEAD_DEADLINE`, counted from when the supervisor accepted it, so time in the queue counts); a body
other than an upload's within another 15 s; an upload's body 60 s per read; only `Content-Length` bodies (`Transfer-Encoding` → `411`, two
lengths → `400`); JSON bodies at most 64 KB; methods `GET`, `HEAD`, `POST` (others `405`). A refused request's body is
drained for up to 1 MB / 1 s after the answer, so the browser reads the reason instead of a reset. Three workers and a
queue of 24; beyond that `503` with `Retry-After`. Every response carries:

```
Content-Security-Policy: default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self' blob:; font-src 'self';
  connect-src 'self'; manifest-src 'self'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'
X-Content-Type-Options: nosniff        X-Frame-Options: DENY          Referrer-Policy: no-referrer
Cross-Origin-Opener-Policy: same-origin  Cross-Origin-Resource-Policy: same-origin
Permissions-Policy: camera=(), microphone=(), geolocation=(), usb=(), payment=()
Cache-Control: no-store                (the page's own files: public, immutable, versioned with ?v=)
```

The page (`src/web/page/`: one HTML file, `app.css`, `app.js` in plain JavaScript, the icon, the manifest, the two
Barlow faces) is compiled in (`assets.rs`). It loads nothing from elsewhere, has no inline script or style, and puts
every text from the Pocket into the page with `textContent`. The log names method, route, status and time, never a
query, a cookie, a code or an address.

## Endpoints

Ids are the first 16 hex digits of SHA-256 of a game's path; paths never travel. "Auth" is a paired browser or the
owner's device; every `POST` also needs the CSRF token and passes the origin check.

| Method and path | Auth | While a game runs | What it does |
|---|---|---|---|
| `GET /`, `/index.html`, `/app.css`, `/app.js`, `/icon.svg`, `/favicon.ico`, `/manifest.webmanifest`, `/fonts/body.ttf`, `/fonts/title.ttf` | no | yes | The page. Any other path needs auth and then is `404` |
| `GET /api/session` | no | yes | `signedIn`, `via` (`paired`/`tailscale`), the name, the CSRF token; without auth only the version and whether a game runs |
| `POST /api/pair` `{"code"}` | no (origin check) | yes | A right code: the cookie. Wrong, or no code showing: the same `403` (`api::NOT_PAIRED`); one answer a second |
| `POST /api/logout` | yes | no | Forgets this paired browser and clears the cookie |
| `GET /api/status` | yes | yes | Now playing (name, system, since) and whether a game runs |
| `GET /api/screenshot` | yes | yes | The screen at half size as PNG (`grim`, 3 s limit); a new capture at most every 10 s (`X-Shot-Age`). While the Pocket shows any Settings page or a first-start step (`Shared::screen_private`, published by the App every loop turn and set before a pairing code exists), a grey placeholder with `X-Screen-Hidden: 1`, and the screen is not read |
| `GET /api/library` | yes | no | Systems with counts and colours, favourites count, stickers (with counts), smart shelves, limits |
| `GET /api/games?system=` \| `favorites=1` \| `sticker=<id>` \| `shelf=<index>` \| `q=<text>` [`&offset=`] | yes | no | A list (600 per answer; search: 80 characters, 200 hits) |
| `GET /api/game?id=` | yes | no | One game: the list row, note, file name, gamedb facts, `shots` (the times of its screenshots), Curator's versions |
| `GET /api/shot?id=&at=` | yes | no | One of the game's screenshots (HOME + A in a game, `src/screenshot.rs`) as PNG, read only: `at` is the time part of the file name (`YYYY-MM-DD HH-MM-SS`, optional ` (n)`) as `/api/game` lists it in `shots` (newest 60); anything else is `404`, so no path or other file can be asked for. 16 MB at most, PNG only. The folder is `Roots::screenshots` (`/storage/roms/screenshots/<system>/<game> <at>.png`) |
| `GET /api/cover?id=[&kind=snap\|title]` | yes | no | The game's cover PNG from the covers folder (8 MB at most, PNG only); `kind=snap` or `title` the screenshot or title screen from the `snaps/` or `titles/` folder instead (404 for any other kind, and with Settings → Games → Pictures & extras → Game pictures on Box art only). `/api/game` lists the kinds the game has as `pictures` |
| `GET /api/stats` | yes | no | Totals from `library.json` and `recents.json`: time, sessions, games played, per system, the top 30, and `history` (below), with every path removed |
| `GET /api/battery` | yes | no | `battery_history`'s last 48 hours |
| `GET /api/uploads` | yes | no | Where uploads may go: systems (with extensions), BIOS needs, ports waiting for files, free space, limits |
| `POST /api/upload/check?kind=…&size=` | yes | no | The name and place an upload would get, whether it exists, or why not (a check may make the target's folder) |
| `POST /api/upload?kind=rom&system=&name=` \| `kind=bios&name=[&sub=dc]` \| `kind=port&dir=&rel=&name=` [`&overwrite=1`] | yes | no (a running one pauses) | The body is the file (`Content-Length`) |
| `POST /api/game/favorite` `{id,on}`, `/api/game/status` `{id,status}`, `/api/game/note` `{id,note}`, `/api/game/rating` `{id,rating}` (0–5, 0 takes it away), `/api/game/sticker` `{id,sticker,on}`, `/api/game/preferred` `{id}` | yes | no | An edit, applied by the App |
| `POST /api/stickers/new` `{name[,id]}`, `/api/stickers/rename` `{sticker,name}`, `/api/stickers/delete` `{sticker}` | yes | no | Stickers, as Settings → Games → Curator's sticker pages do them |
| `POST /api/shelves/rename` `{index,was,name}`, `/api/shelves/delete` `{index,was}` | yes | no | Smart shelves, by place and old name (a shelf changed on the Pocket meanwhile is refused) |

Errors are `{"error": "<a sentence for the page>"}` with the status: `400` bad request, `401` not signed in, `403`
refused (CSRF, origin, place, file type), `404`, `409` exists or changed, `411`, `413` too large, `421` wrong host,
`503` busy or a game runs, `507` no room.

### Play history

`web::set_history_source(fn(&Path) -> Option<serde_json::Value>)` registers a function (once) that `GET /api/stats`
calls with the data folder; its answer goes under `"history"`, `null` without one. `main()` registers
`play_history::stats_json` (docs/HISTORY.md, "Read API"; a function pointer stored in a `OnceLock`, nothing runs then).
It is called on a worker for each `GET /api/stats` (never while a game runs) and caches its result until the log or
the day changes; `Ctx::history` lets tests and the development server put their own in.

**No path reaches the browser.** The read API names each game by its full path (`games`, `came_back`, `completed`,
`recent`). Before the answer is sent, `api::without_paths` walks the whole value: every `path` field is removed, a
game that is in the library gets its hashed id instead (the same id as everywhere else, for a link to its page), and
any other string that starts with `/` or `\` becomes `null`. The test
`the_play_history_reaches_the_stats_page_without_a_single_path` checks the raw answer for `/roms/`, the data
folder and the word `"path"`.

The page: with `history`, **Play history** first (counted since, all-time time and sessions with what was carried
from before the log, a note when `damaged`; Week / Month / Year as buttons and in the address, `#/stats/year`; per
period the tiles, a bar chart per day or per week, the longest and current streak, most played, systems, came back,
completed, or "Not recorded: the history counts from …" when `not_recorded`, or "Nothing played in this period"),
the twenty recent sessions (without time: "it did not end normally"; undated: "before the clock was set"), then
**All time, by game** from the library. Without it: a sentence that Play history is off or empty, then the library
totals under **Play time**. Charts are drawn for the width they get and again when the window changes size.

## Uploads

`files.rs`. Three kinds of place only:

- **Games**: the system's folder from es_systems.cfg, only inside `/storage/roms`, never `ports`, `bios`,
  `portmaster`, `savestates` or `port-files`, never a system whose extensions are scripts or programs; the folder,
  once links are followed, must still be inside the ROMs folder and its first folder there must still not be one of
  those (a link `roms/gba` → `roms/ports` is refused); it is made if missing, one level below. The file is written
  through the resolved folder. The name must end in one of the system's extensions.
- **BIOS**: `roms/bios` (itself checked the same way), or `roms/bios/dc` for the files `hostsys` names there (the
  folder spelt as hostsys spells it, not as sent; not a link). No scripts, no programs.
- **PortMaster**: `roms/port-files/<port>/<relative path>` for an installed port that says it is not ready to run and
  whose instructions name a place (`port_files::drop_target`); pocket-ui moves the files into the port at its next
  start, never over an existing file, never a link (`port_files::take_dropped`, as for the file share). Every folder
  on the way is made by us and must not be a link; at most 12 levels; no scripts (`.sh`, `.bash`, `.py`, `.pl`), no
  native code by name (`.so`, `.so.N…`, `.elf`, `.ko`, `.AppImage`, `.run`) or by content (a file that starts with
  the ELF magic is refused while it arrives and nothing is kept).

Names (`sanitize_name`): the last part after `/` or `\`; control characters, invisible ones (zero-width, writing
direction marks and overrides, the byte-order mark), look-alike slashes and `\ / : * ? " < > |` become `_`; no leading
dots or spaces (no hidden files, never `.` or `..`), no trailing dots or spaces; at most 200 bytes, the extension kept.
A relative path is cleaned segment by segment and refused for `..`, `.`, empty segments or a leading `/`.

Writing (`receive`): a hidden temporary file beside the target (`.<name>.<pid>-<n>.upload`, `O_CREAT|O_EXCL|O_NOFOLLOW`,
0644), `fsync`; then the target's folder is resolved once more and must be the very path it was written through (a
folder on the way that became a link meanwhile is refused); then `renameat2(RENAME_NOREPLACE)` (ext4, exFAT and FAT
take it), or a hard link and an unlink; a file system with neither refuses rather than check-then-rename. So an
existing file is replaced only with `overwrite=1`, which the page sends after asking; a link or anything but a plain
file in the target's place is refused. A short body, a write error, a game running for more than 30 minutes or the
companion being switched off remove the temporary file. Temporary files of an earlier pocket-ui (another pid) beside
the target are removed. Limits: 9 GB per game or port file, 128 MB per BIOS file, and the card keeps 256 MB free:
each upload reserves its size under a lock before a byte is read (`files::reserve`), so uploads at the same time
can't all count the same free space; a card whose free space can't be read takes nothing.

## Threat model

Assets: the device (pocket-ui runs as root), the player's files and saves, the library notes, the screen. Trusted:
the Pocket's own files and es_systems.cfg, tailscaled and its LocalAPI socket (root only), the player at the Pocket.
Out of scope: someone with SSH or the card in hand.

| Attacker | Can reach the port? | What stops them |
|---|---|---|
| A device on the same Wi-Fi | No | Not bound to the Wi-Fi address; `SO_BINDTODEVICE` drops packets for 100.x arriving on wlan0; the peer check refuses non-Tailscale sources |
| The internet | No | Only the tailnet; tailscaled's WireGuard |
| Another user's device on the tailnet (shared node) or a tagged node | Yes, if the tailnet's ACLs allow | `whois` names another user or a tag: a code is needed, which only the Pocket's screen shows. Guessing: one try a second, five per code, 1 in 200 000 per code |
| The owner's own device, with a malicious website open | Yes (the browser can send requests to 100.x) | No CORS; the origin check refuses requests marked cross-site; writes need the CSRF token, which the other site can't read; `SameSite=Strict` cookies; the Host check stops DNS rebinding; `X-Frame-Options`/`frame-ancestors` stop framing |
| A paired browser that is lost | Yes | Revoke it in Settings → Connections → Web companion → Paired browsers; the token is useless from then on (only its hash was kept) |
| A malicious authorised user (the owner or a paired friend) | Yes | Can write only into the three kinds of place above, never over a file without asking, never a script or native code, never into `ports/`; cannot read files other than covers, the library data and the screen (never while it shows Settings or a first-start step). Not a code-execution boundary for ports: a port's engine reads the data the player drops (that is what the drop is for), may run data in formats of its own (Lua, a game's own bytecode), and `take_dropped` lets a dropped file replace a placeholder (`readme*`, `.txt`, `.md`, empty) in the port. That is the same trust as the file share, which can write the same folders |
| Someone using the owner's Tailscale account on another device (family, a server added with the owner's key) | Yes | With "Your Tailscale devices" Open, nothing: the trust unit is the account (the manual says so). Code makes every browser pair, and revoking then locks a device out |
| A slow or flooding client | Yes (tailnet only) | 15 s for the head, three workers, a queue of 24, `503` beyond; all at the lowest priority, so games and menus come first |

### Security self-review (2026-10-23)

Done endpoint by endpoint against the code above, with a test for each property in `src/web/tests.rs`:

- **Static files**: a fixed table; no path is mapped to the file system, so `..`, `%2e%2e` or encoded slashes have
  nothing to reach (`/../../etc/passwd` → `401`, then `404`). Index served with the version only.
- **`/api/session`**: unauthenticated it tells the version and whether a game runs, nothing more (the pairing state
  was removed in this review). The CSRF token only to an authenticated principal.
- **`/api/pair`**: origin-checked; JSON at most 64 KB; codes from the kernel's random source, uniform; compared in
  constant time; serialized one a second; five per code; used once; the cookie's token 256 bits, stored hashed.
- **`/api/logout`**: CSRF; removes only the caller's own entry.
- **`/api/status`**, **`/api/battery`**, **`/api/library`**, **`/api/games`**, **`/api/game`**, **`/api/stats`**:
  read-only; ids are hashes looked up in the library, never paths; query text bounded; responses carry no paths
  (only the file name of a game) and `no-store`. The play history inside `/api/stats` comes with full paths and
  leaves without them (`without_paths`, reviewed 2026-10-23 when the history was wired).
- **`/api/cover`**: the path is built by `covers::cover_path` (or `game_pictures::path` for the two kinds it knows) from a library game, never from the query; PNG only,
  8 MB at most.
- **`/api/screenshot`**: authenticated; `grim` started without a shell, 3 s limit, one per 10 s; a page of another
  site can't trigger it in a modern browser (origin check) and can't read it in any.
- **`/api/uploads`**: read-only.
- **`/api/upload/check`**: `POST` with CSRF since this review (it may make an empty target folder).
- **`/api/upload`**: CSRF; kinds and places as above; name and path cleaning (NUL, control, invisible and look-alike
  characters, `..`, absolute paths, links on the way and in the target's place); size limit and free space before
  reading; atomic, no-replace rename; cut uploads keep nothing; `RENAME_NOREPLACE` closes the race with a file
  appearing meanwhile.
- **Edits** (`/api/game/*`, `/api/stickers/*`, `/api/shelves/*`): CSRF; values typed and bounded by `library_meta`
  (note and sticker lengths) and `curator::prefs` (shelf names); applied on the UI thread through the App's own
  functions, so the files are never written by two threads.

Changed in this review: the "Also on Wi-Fi" option was removed (the companion listens only on Tailscale); the origin
check covers every `/api/` request; the upload check became a `POST`; pairing tries are serialized across
connections; the head of a request has a total deadline; `/api/session` no longer tells whether a code is showing;
the BIOS folder itself must stay inside the ROMs folder and its `dc` subfolder is spelt as hostsys spells it; port
uploads refuse scripts; invisible and look-alike characters are cleaned from names; temporary files of an earlier run
are swept; `blob:` images are allowed for the screenshot.

Known limits: plain HTTP inside the tailnet (WireGuard encrypts it; a certificate would need Tailscale's HTTPS
feature, which is a tailnet-wide setting); the cookie has no `Secure` flag and can't carry the `__Host-` prefix for
that reason. A paired browser stays valid for a year unless revoked. Uploaded files are root's (0644), like files
copied over SSH.

### Independent review (2026-10-24) and what was done

An adversarial review (`REVIEW-MUSE-2026-10-24.md`, outside the repository) found one crash and eighteen smaller
points. Each fix has a regression test (`src/web/tests.rs`, section "review 2026-10-24", and `app_web.rs`'s tests).

| | Finding | Done |
|---|---|---|
| F1 | `web_start` called `web_sync`, which called `web_start`: switching on more than 3 s after the start (and every start with it on) overflowed the stack | Fixed: the lifecycle lives in `WebUi` (`turn`, `start`, `stop`, `publish`, `set_prefs`); starting never calls back into the loop. `App::web_sync` and the Settings switch call those functions; tests run them as the App does (on at the start after 3 s, on later, off, on again, a failed start waiting 30 s, pairing). The App itself needs a display and an event loop and is not built in tests; that path is the device check below |
| F2 | No total deadline for a body; the head's deadline started at the worker | Fixed: a non-upload body has 15 s after the head; the head's 15 s count from the accept |
| F3 | The supervisor wrote the queue-full `503` blocking; `stop` joined it on the UI thread | Fixed: one non-blocking write (`server::reply_busy`); `stop` waits at most 1 s |
| F4 | The screenshot could capture the pairing code and other Settings screens | Fixed: hidden while any Settings page or first-start step shows, and from before a pairing code exists; a placeholder says so |
| F5 | "No code" answered at once and differently: an oracle for when a code appears | Fixed: one answer a second for every try, the same text, no count of tries |
| F6 | Paired browsers never expired on the Pocket | Fixed: a year after pairing, as the cookie |
| F7 | Switching off let queued requests, edits on their way and uploads finish | Fixed: queued connections are closed, edits are answered "off", an upload stops at its next chunk and keeps nothing |
| F8 | A cover was read whole before its size was checked | Fixed: the size first, then a bounded read |
| F9 | Uploads at the same time shared one free-space check; unknown free space passed | Fixed: a reservation under a lock; unknown free space refuses |
| F10 | Port uploads were not a code-execution boundary | Native code is refused by name and by ELF content; the threat model now says what remains (engine data formats, placeholders), the same as the file share |
| F11 | A folder checked once could be swapped for a link before the rename; a plain-rename fallback | Fixed: written through the resolved folder, resolved again before the rename; the plain-rename fallback is gone |
| F12 | A link inside `roms` could lead into a `NEVER` folder | Fixed: the first folder is checked again after resolving |
| F13 | A `tailscale0` made anew with the same address kept a dead socket | Fixed: the interface index is part of the plan |
| F14 | Trust is per account; whois answers kept a minute | The manual says what Open means; whois answers are dropped when the switch changes. A device removed from the account in Tailscale may still get in for up to a minute: accepted, as asking tailscaled on every request would cost a LocalAPI call each |
| F15 | The history scrubber only looked at `path` fields | Fixed: keys with slashes go, and any text that starts with a slash, mentions `/storage/`, `roms/`, the data folder or the ROMs folder |
| F16 | Another site's cookie of the same name could hide the real one | Fixed: every cookie of that name is tried |
| F17 | A broken escape in the address stopped the page | Fixed: decoded with a fallback |
| F18 | Sticker and shelf numbers were cut to 32 bits | Fixed: refused when they don't fit |
| F19 | A worker panic is permanent; lock poisoning | Not changed as suggested: pocket-ui is built with `panic = "abort"`, so a panic anywhere ends the process (and the launcher starts it again); `catch_unwind` can't help and poisoned locks never outlive it. Instead the one indexing found in a reachable path (`shrink_png` with a picture whose pixels don't match its size) refuses such a picture; no other reachable panic was found |

## Working on it

```sh
cargo run --release --example web_dev -- --port 8737 [--empty] [--playing] [--no-history]
```

serves `http://127.0.0.1:8737` as the owner's device sees it and `http://127.0.0.1:8738` as another user's device
(the pairing page; the code is printed) with a made-up library, covers, play time, a smart shelf, a port waiting for
files and 40 hours of battery, in `/tmp/pocketui-web-dev`. `--empty` has no games at all; the Stats page gets made-up play history over the made-up games
(`play_history::demo`, given with full paths as the real one is), `--no-history` leaves it out. These sockets are bound by
address only, for development. `node scripts/web-shots.mjs http://127.0.0.1:8737 <out> library system/gba upload stats`
takes the page as a phone and a computer see it, dark and light (`CHROME=…`, and `CHROME_ARGS=--no-sandbox` on a
Linux host without user namespaces). Settings → Connections → Web companion: `--snapshot web[:off|:none|:waiting|:busy|:pair|:browsers|:revoke][:<row>]` (`busy`: port 80 taken).

## Device checks (partly done)

Done on the Mini V2 (2026-10-24, `63d7030`): checks 1 and 2 in part. Off means nothing listens and no `web` thread
runs; switching on more than 3 s after the start does not crash; it listens only on the Tailscale addresses and
refuses the Wi-Fi address; a restart with it on works; the session API answers over the tailnet; the stats carry no
paths; with Settings open the screenshot is a placeholder (check 5, first half). Not done: pairing with a code
(3), uploads (4), a running game and the latency comparison (6).

1. Off (default): `ss -ltnp` shows nothing on port 80, `ps -T` no `web` threads. Then switch it on in Settings
   (more than 3 s after pocket-ui started: the path review F1 found crashing), off, on again; restart pocket-ui with
   it on: it starts listening about 3 s after the start, and pocket-ui keeps running.
2. On with Tailscale connected: `ss -ltnp` shows port 80 only on the `100.x` and `fd7a:…` addresses with
   `%tailscale0`; from another device on the same Wi-Fi, `curl http://<wlan ip>/` and `curl --interface wlan0
   http://100.x/` (with a route) both fail; from a tailnet device the page opens.
3. Owner's device opens without a code; a device of another user (or Your Tailscale devices = Code) gets the
   pairing page; the QR code pairs; five wrong codes end the code; revoking ends a browser at once.
4. Uploads: a GBA game appears on Home after returning there; a BIOS file ticks off its line; a port's files move in
   at the port's start; a 2 GB upload over Tailscale, and one cut off midway (nothing left in the folder).
5. With Settings → Connections → Web companion → Pair a browser open, the page's Screenshot shows "Hidden"; on Home it shows the
   screen. Switch the companion off during a large upload: the page reports it, nothing is left in the folder.
6. A game running: the page says so; Now playing and the screenshot work; `scripts/latency.sh --compare` with the
   companion on and off shows no difference in start, menus and switching.
