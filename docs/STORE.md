# pocket-store

> **Status:** the Store page and all four package kinds work end to end against a local test catalog. There is
> no public catalog yet (see "Hosting" and "Status" below), so **the Store is hidden**: no Settings → Games → Store
> row until a catalog address is configured (see "When the Store is offered").

The OS stays small with good defaults; everything optional comes from the store, in the spirit of
NextUI's Pak Store (LoveRetro/nextui-pak-store, MIT; only the idea is taken, no code).

## What goes where
- **In the system:** pocket-ui, pocket-core, the common cores, picture defaults per console, Wi-Fi,
  cover art, save states.
- **In the store:** cores few people need and newer core versions, frame packs, themes (type), tools.
  PortMaster games stay in Extras.

## When the Store is offered
The Store page is the only way into the Store, and it needs a catalog. `store::catalog_url(settings.store_url)` gives
the address: settings.json `"store_url"` when it is set and not blank, otherwise the built-in `store::DEFAULT_URL`.
That constant is **empty** until a catalog is hosted. With no address:

- Settings → Games has no **Store** row (and its line reads "Add games, extras, tools, PC streaming."); the cursor,
  the hint bar and the touch rows count only the rows that are shown (`app_settings_groups::rows`). Nothing else links
  to the page (not Extras, not the web companion, not first-start Extras).
- `store::fetch_catalog("")` answers "There is no store yet." without touching the network, so a snapshot of the
  page (`--snapshot store`) says that too.
- Tools installed from a store earlier still show under Settings → Help & tools → Tools; they never depended on the page.

Setting `store_url`, or filling `DEFAULT_URL` when the hosting is decided, brings the row back at the next start
(`App` reads it once; `app_settings_groups::set_store_offered`). `file://` addresses count, which is how the local
test catalog is used below. The code and the page are untouched by all this.

## Settings → Store
Three levels, all in the Settings row style:

1. **Categories:** Emulators (catalog category `cores`, also `emulators`), Frames, Themes, Tools, then any
   other category in the catalog. Each row shows how many packages it holds, or a paper sticker
   "1 update" when installed packages have a newer version. X refreshes the catalog.
2. **Packages of a category:** name, and on the right the size (not installed), "Installed", an
   "Update" sticker, or the progress in % while it is being installed.
3. **A package:** a paper label with the name, category, version and size (and "Installed" /
   "Update available"), the description, author, licence, the installed version when an update is
   waiting, where it goes, and the actions:
   - **Install, <size>** / **Update to <version>**: download with a progress bar, then install.
   - **Run** (tools only): runs the tool and shows its last line of output.
   - **Remove**: asks for a second A first, so a double press after installing can't undo it.

The catalog is fetched once when the Store opens; after a failed refresh the last verified list stays
usable. Downloads run on a worker thread like the Updates page; B leaves the page while one runs.

Messages (in red when something failed):

| Situation | Text |
|---|---|
| Catalog not reachable | Couldn't reach the store. Check Wi-Fi. |
| No catalog address (the page is normally hidden then) | There is no store yet. |
| Catalog signature wrong | The store list's signature doesn't match, so it wasn't used. Nothing was changed. |
| Download broke off | The download didn't finish. Check Wi-Fi and try again. |
| SHA-256 doesn't match | The download is damaged or was changed. Nothing was installed. |
| Archive with `..` or absolute paths | The package contains unsafe paths. Nothing was installed. |

The starter pack (free games for an empty card, [STARTER-PACK.md](STARTER-PACK.md)) is not a Store package kind but reuses the
Store's download: `store::download_with_cause` is `download_to` (the same curl, limits, progress and size cap) that also says
whether the network was missing, so the starter pack can answer "offline" calmly. The Store's messages are unchanged.

Code: `src/store_page.rs` (the page, `impl App` in the pocket-ui binary), `src/store.rs` (catalog,
verification, install and remove). The hooks in `src/main.rs` are the Settings row, `SettingsPage::Store`,
`UserEvent::Store` and `store_url` in settings.json. The row exists only with a catalog address (above).

## Package kinds

| Category | `kind` | `dest` | How it takes effect |
|---|---|---|---|
| `cores` | `file` | `/storage/cores` | see below |
| `frames` | `tar` | `/storage/.config/pocket-ui/frames` | pocket-core reads the packs at every game start (docs/FRAMES.md) |
| `themes` | `tar` | `/storage/.config/pocket-ui/fonts` | `title.ttf` and `body.ttf` replace Barlow; pocket-ui reloads its type right after install or remove |
| `tools` | `file` | `<data>/tools` (whatever `dest` says) | Settings → Help & tools → Tools lists it; the Store's Run starts it too |

### Cores
Verified in the ROCKNIX 20261001 image (SYSTEM squashfs, as unpacked for the virtual device):

- `/usr/lib/systemd/system/tmp-cores.mount` mounts an **overlay at `/tmp/cores`**:
  `lowerdir=/usr/lib/libretro,upperdir=/storage/cores`. `/usr/lib/libretro` itself is *not* overlaid.
- RetroArch loads from there: `retroarch.cfg` has `libretro_directory = "/tmp/cores"`, and `runemu.sh`
  starts `retroarch -L /tmp/cores/${CORE}_libretro.so`.

So a core installed to `/storage/cores/<name>_libretro.so` is what RetroArch uses from then on, ahead of the
image's copy. pocket-core used to be started with `/usr/lib/libretro/<core>_libretro.so`; it now gets
`store::core_path(core)`, which prefers `/storage/cores` the same way (this also works where the overlay
isn't mounted, such as the virtual device).

What it takes for pocket-ui to *use* a store core:

| Case | What happens | Needed |
|---|---|---|
| Newer build of a core the image already has (same file name) | Picked up at the next game start, by pocket-core and by RetroArch | Nothing |
| A core for a system pocket-core hosts, but a different core | pocket-core keeps using the core in `HOST_CORES` / `hostsys.rs` | A code change in `src/hostsys.rs` (core name, BIOS, options), shipped as an app update, after checking it on the device (`verified`) |
| A new core for a system RetroArch runs | RetroArch can load it, but pocket-ui starts the system's default core from `es_systems.cfg` (`library.rs`, `default="true"`) | The core must be listed under the system's `<emulator>` in `es_systems.cfg` and be the default there. Not done yet: packages can't change `es_systems.cfg` |

A core must be aarch64, linked against glibc 2.38 or older (the image has 2.41), and named
`<core>_libretro.so`. Check one without pocket-ui: `vdev.sh run-core /storage/cores/<core>_libretro.so <rom>`.

Proposal for the last two rows, not built: an optional `"systems": ["<system>", …]` on core packages. After
installing, pocket-ui would add the core to those systems (a small `store/cores.json` that `launch` consults
before `hostsys`), so nothing in the read-only image or in ROCKNIX's `es_systems.cfg` has to change.

### Themes
Only type for now: a theme package installs `title.ttf` (titles, spines, labels) and/or `body.ttf` (everything
else) into `<data>/fonts` (`src/text.rs`, `fonts_dir()`). A file that fontdue can't read is skipped and the
built-in face is used. Colours and layouts are built in (`palette.rs`, the four shelf looks) and can't come from
the store yet.

### Tools
A tool is a shell script. It goes to pocket-ui's own folder `<data>/tools` (`/storage/.config/pocket-ui/tools`,
`store::tools_dir`), never to ROCKNIX's Tools folder `/storage/.config/modules`: ROCKNIX's autostart
`001-sync-modules` runs `rsync -a --delete /usr/config/modules/ /storage/.config/modules/` at every boot (and
`post-update` again), so anything installed there was gone after the next restart (confirmed on the Mini V2,
2026-10-23). A package of category `tools`, or one whose `dest` is `/storage/.config/modules` (catalogs before
2026-10-23), is installed into `<data>/tools` whatever `dest` says (`Package::install_dir`).

Tools installed by an older pocket-ui are moved over once, on a thread at start and again before Settings → Help & tools → Tools
lists the tools (`store::migrate_tools_in`): files still in ROCKNIX's folder move to `<data>/tools` and the inventory
records the new places; a tool whose files ROCKNIX has deleted already is taken off `installed.json`, so the Store
offers Install again instead of claiming it is there. The ownership check and the stage-then-swap of an update work
on the new paths as before. `installed.json` also keeps each package's `name` and `run`, so Settings → Help & tools → Tools can
list installed tools without the catalog.

With `"run": "<file>"` the Store offers Run, and Settings → Help & tools → Tools lists the tool: pocket-ui starts
`sh <data>/tools/<file>` in `/tmp`, waits for it and shows the last line it printed (the last line of its errors if
it fails). A tool has to finish by itself and can't ask anything; there is no terminal. ROCKNIX's own tools, with
terminal and windows, are started from Settings → Help & tools → Tools ([SYSTEM-PAGES.md](SYSTEM-PAGES.md#tools)).

## Catalog format
`catalog.json`, signed as a whole: `catalog.json.sig` next to it is the base64 Ed25519 signature of the exact
bytes. pocket-ui only accepts catalogs signed with the key in `src/store.rs` (`PUBLIC_KEY`).

```json
{
  "version": 1,
  "serial": 1792656239,
  "expires": 1824192239,
  "packages": [
    { "id": "frames-graphite-monitor", "name": "Graphite monitor", "category": "frames", "version": "1.0",
      "description": "A plain graphite bezel around Game Boy and Game Boy Advance games, …",
      "size": 169244, "url": "packages/frames-graphite-monitor.tar.gz", "sha256": "eee58d6b…",
      "kind": "tar", "dest": "/storage/.config/pocket-ui/frames", "author": "pocket-ui", "license": "MIT" },
    { "id": "tool-storage-report", "name": "Storage report", "category": "tools", "version": "1.0",
      "description": "…", "size": 445, "url": "packages/storage-report.sh", "sha256": "…",
      "kind": "file", "dest": "/storage/.config/pocket-ui/tools", "name_on_disk": "Storage report.sh",
      "run": "Storage report.sh", "author": "pocket-ui", "license": "MIT" }
  ]
}
```

| Field | |
|---|---|
| `serial` | Catalog, optional: a number raised with every published catalog (Unix time works). A device remembers the highest serial per store URL in `<data>/store/catalog-serials.json` and refuses a lower one, so an old signed catalog can't be replayed. Missing counts as 0 |
| `expires` | Catalog, optional: Unix time in seconds after which the catalog is refused |
| `id` | Stable identifier; installed state is kept per id. Letters, digits, `.`, `_`, `-` |
| `name`, `description` | Shown in the Store; sentence case, no marketing |
| `category` | `cores`, `frames`, `themes`, `tools` (`emulators` is shown with `cores`); others get their own page |
| `version` | Compared by number groups like app updates (`1.10` > `1.9`); only a later version than the installed one counts as an update, and an older one is never installed over it |
| `size` | Bytes, for the list and the progress bar (`store-tool hash` prints it) |
| `url` | Absolute, or relative to the catalog (`packages/x.tar.gz`), so a catalog folder works from any host or from `file://` |
| `sha256` | Of the downloaded file; checked before anything is written to `dest` |
| `kind` | `tar` (a .tar.gz unpacked into `dest`) or `file` (written to `dest/name_on_disk`) |
| `dest` | Absolute folder on the device. Ignored for tools, which always go to `<data>/tools` (see [Tools](#tools)) |
| `name_on_disk` | `file` only; default is the id. No `/` |
| `author`, `license` | Optional, shown on the package page. Ship only what may be redistributed |
| `run` | Optional, tools: script relative to `dest` that Run starts |

## Safety
- Unsigned or changed catalogs are refused; the host is not trusted, only the key.
- Signed but stale catalogs are refused too: past `expires`, or with a lower `serial` than one the device
  already had from that URL. Even with an old catalog, an older package version is never offered as an
  update (B5).
- A download whose SHA-256 differs from the catalog's is thrown away before anything is unpacked.
- Archives with `..` or absolute paths are refused, and so are archives with anything but plain files and
  folders (symlinks, hardlinks, devices, FIFOs). pocket-ui reads every tar header itself before tar runs,
  then feeds tar exactly the checked bytes (`src/untar.rs`).
- Packages are unpacked as root with `--no-same-owner --no-same-permissions` into a private folder, and
  every mode is then set: folders and executables 0755, other files 0644, never setuid/setgid. BusyBox tar
  on ROCKNIX keeps a setuid bit even with `--no-same-permissions`, so this last step is the one that counts.
- Every installed file is recorded in `<data>/store/installed.json`, together with the folders the package
  created. Remove and Update delete exactly those files, then those folders if they are empty, so files the
  user put there stay. `dest` folders themselves (such as `/storage/cores`, the overlay's upper layer) are
  never removed.
- `installed.json` is written whole or not at all (temporary file, fsync, rename, fsync of the folder); the
  list before the last change stays as `installed.json.bak`. A damaged or unreadable `installed.json` is
  not taken for an empty one: the Store then installs and removes nothing and says so, until the file is
  fixed by hand (for example from the `.bak`).
- A file belongs to the package that installed it: a package that would write a file another installed
  package owns is refused, and removing a package never deletes a file another one still lists. `dest`
  must be an absolute path without `..`, so the same file can't hide under two spellings.
- An update downloads the new version, unpacks it into `<data>/store/.staging-<id>` and checks it there
  (entries, modes, files of other packages, folders or links in the way). A failed download or a refused
  package leaves the installed version exactly as it was. Only then are the new files moved in (a rename
  per file) and the files and empty folders the old version had and the new one doesn't removed. Should a
  move fail halfway, everything already in place is recorded, so Remove still cleans up.

## Publishing (build machine)
- Key: `~/.config/pocket-ui/store-signing.key` (private, 0600, never committed).
1. Build the package: a .tar.gz of the files as they should appear under `dest`, or the single file.
   On macOS set `COPYFILE_DISABLE=1` for `tar`, or it adds `._` files.
2. `store-tool hash <file>` prints the sha256 and size for the entry.
3. Add the entry to `catalog.json`, then `store-tool sign ~/.config/pocket-ui/store-signing.key catalog.json`
   writes `catalog.json.sig`.
4. Upload the packages, `catalog.json` and `catalog.json.sig` together (catalog last).
5. `store::DEFAULT_URL` is empty until the hosting is decided, which hides the Store (see "When the Store is
   offered"). Once there is an address, put it there (and in `docs/user`); settings.json
   `"store_url": "…/catalog.json"` points one device at a catalog now.

## Hosting
Decided 2026-10-26, together with the update channel ([UPDATE-CHANNEL.md](UPDATE-CHANNEL.md#the-store-catalog)): the
catalog goes to `https://pocketui.xyz/updates/store/catalog.json` (the website, static on Vercel), small packages next
to it and big ones on GitHub Releases of the public repository. The catalog is signed and every package is hashed, so
neither host needs trust. Nothing is published yet, so `DEFAULT_URL` stays empty; `scripts/release-channel.sh` copies
a signed catalog folder from `release-out/store/` into the folder for the website.

## Local test catalog
`store/test-catalog/` holds one tiny real package of every kind, signed with the real key:

| Package | Kind | What it shows |
|---|---|---|
| `frames-graphite-monitor` | tar | pocket-core's monitor-style frame (`--export-frame`) at the GBA and GBC Native positions, so GB/GBC/GBA games get a graphite bezel instead of the built-in handheld lens |
| `type-titillium` | tar | Titillium Web (OFL, from retroarch-assets in the ROCKNIX image) as pocket-ui's type |
| `tool-storage-report` | file | A script printing free space and the number of game files; its version is in its output |
| `core-testpattern` | file | Our own libretro core (`store/test-catalog/src/testpattern/testpattern.c`, MIT, 5 KB): colour bars and a square the D-pad moves |

`scripts/make-test-catalog.sh [OUT] [VERSION]` rebuilds it (needs cargo, zig and the signing key). A second
run with `1.1` into another folder gives a catalog in which all four are updates. Each run numbers its
catalog with the current time (`SERIAL=<n>` sets it; `EXPIRES_DAYS=<n>` adds an expiry). A device that
already had a newer catalog from the same URL refuses an older one, so to go back, delete
`<data>/store/catalog-serials.json` on it.

Testing in the virtual device (`docs/VIRTUAL-DEVICE.md`):

```bash
VDEV_NAME=store scripts/vdev.sh deploy
docker cp store/test-catalog/. vdev-store:/storage/test-catalog/
docker exec vdev-store sh -c 'echo "{\"onboarded\": true, \"store_url\": \"file:///storage/test-catalog/catalog.json\"}" \
  > /storage/.config/pocket-ui/settings.json'
VDEV_NAME=store scripts/vdev.sh deploy --no-build
```

- `file://` installs are instant. To watch progress, serve a catalog folder over HTTP inside the container
  (`docker exec -d -w /storage/<folder> vdev-store python3 -m http.server 8765 --bind 127.0.0.1`, `store_url`
  `http://127.0.0.1:8765/catalog.json`) and slow curl down with `limit-rate = 15k` in `/storage/.curlrc`
  (pocket-ui's `HOME` is `/storage`; the limit doesn't apply to `file://`).
- Stopping that server gives the Wi-Fi errors; changing a byte of a served package or of `catalog.json`
  gives the hash and signature errors.
- The core: `VDEV_NAME=store scripts/vdev.sh run-core /storage/cores/testpattern_libretro.so <any file>`.

Checked on 2026-10-21 in the virtual device: install, progress, update 1.0 → 1.1, run, remove for all four;
the frame pack around a GBA game; Titillium switching in and out at once; the test core at 60 fps in pocket-core;
all five error messages; nothing left after removing everything.
Checked on 2026-10-23 in the virtual device: the tool installs into `<data>/tools` ("Goes to" says so), runs from the
Store and from Settings → Help & tools → Tools; an old install listed under `/storage/.config/modules` is moved over at start, one
whose file was gone is taken off the list.

## Status
- Done: the Store page, four package kinds end to end, test catalog, file:// and relative URLs; hidden while no
  catalog address is configured.
- Open: the first public catalog (hosting is decided, above); core packages that add a core to a system (proposal above); colour themes; a
  per-pack frame choice in the picture menu (docs/FRAMES.md); the
  same list in first-start Extras (docs/ONBOARDING-STORE.md).
