# The update channel on pocketui.xyz

Status: 2026-11-06. The stable signed channel is live. Since 0.2.1 its app update and install package are served directly by pocketui.xyz. How the device installs an update and
rolls back is in [UPDATES.md](UPDATES.md); this page is about where the files live and how a release gets there.

## Bounded downloads and alternate sources (0.3)

A release may add `"mirrors": ["https://github.com/owner/repo/releases/download/vX/file"]`.
`url` remains the first source, so older clients still work; all sources serve **identical bytes**.
The complete list of sources is signed with the existing Ed25519 key. Size and SHA-256 checks
apply equally to every source. HTTPS and HTTPS-only redirects remain enforced (`file://` for offline tests).
A changed mirror list without a fresh signature is refused.

The worker tries at most four signed sources, once each. It stops a source after 30 seconds
without any file growth, or after five minutes plus the file's size at 10 KB/s (`source_deadline`: a 15 MB
app gets about 30 minutes, a 1.5 GB system update a day), including DNS, TLS and redirects. A flat five minutes
cut every system update and, on weak Wi-Fi, the app, and the next source began from nothing. Curl also has
20-second connect and 30-second low-speed timeouts. There are no hidden curl retries. A host that stalls is left
after 30 seconds; one that trickles is bounded by that deadline. When the deadline ends while bytes
still come, the page says "The download is too slow on this network"; curl's own limit is 10 s after the
worker's. Before the download the app update checks the SD card's room: the rest of the download, three times
the archive unpacked, and 32 MB to spare (`update::room_for_app`); curl's write error (23) says the card is
full, not Wi-Fi. A failed secure connection (curl 35, 60) with the clock before this build (`clock_check`) names
the clock.
Cancellation stops the operation rather than starting the next host. On a host change, partial
bytes are discarded: a corrupt prefix from one host cannot poison the next. A partial file from
the same source can still resume on a later attempt; the final full-file hash remains mandatory.

The page names connecting, trying the next server, downloading, checking and preparing phases;
a stalled final source reports “The download server stopped responding. Nothing was installed.
Try again.” Failure leaves the active installation unchanged. Source-index failures go to the
UI log without URLs or credentials. The progress counter restarts on a change of host.

`scripts/release-channel.sh` prepares the app beside the signed website list under
`/updates/<channel>/`, with GitHub as the signed fallback (the default since 0.3; `--github-primary` keeps a
test channel on GitHub only, and so does a `--base-url` of its own unless `--direct-app` is given). pocket-ui
up to 0.2.x knows no mirrors and downloads only `url`: an app that only GitHub's download host serves would
reach none of the players whose network blocks it. It refuses an app over 80,000,000 bytes to leave margin
below the Hobby upload limit, and a version that is not plain `X.Y.Z` (every pocket-ui compares versions by
their numbers only: `0.3.0-rc1` would count as newer than `0.3.0`). Only `--channel brick` also accepts
four numeric groups, such as `0.3.0.1`: this hotfix updates `0.3.0` and is superseded by `0.3.1`.
The ROCKNIX stable and beta channels still require three groups. It publishes nothing.
`store-tool mirror KEY update.json app URL [URL] [URL]` signs alternate sources;
`store-tool list` and `release-channel.sh --check` check every source, not only the primary.
Card images, source archives and installers can stay on the release. Check the complete
website deployment size and remaining monthly transfer before requesting a public deploy.

## Layout

A small signed list and the app update on the website, the big files on GitHub Releases of the public repository:

| What | Where | Size |
|---|---|---|
| The update list | `https://pocketui.xyz/updates/stable/update.json` (`update::DEFAULT_URL`) | about 1 KB |
| Its signature | `https://pocketui.xyz/updates/stable/update.json.sig` (base64 Ed25519 of the exact bytes) | 88 bytes |
| The store catalog, once there is one | `https://pocketui.xyz/updates/store/catalog.json` and `.sig` ([below](#the-store-catalog)) | a few KB |
| The app update | `https://pocketui.xyz/updates/stable/pocket-ui-<version>-aarch64.tar.gz`, the same file on `https://github.com/<owner>/pocket-ui/releases/download/v<version>/` as its signed mirror | about 15 MB |
| The install package for a ROCKNIX card | `https://github.com/<owner>/pocket-ui/releases/download/v<version>/pocket-ui-<version>-rocknix-install.zip` | about 15 MB |
| The SD card image and its notes | `…/v<version>/pocket-ui-<version>-rp-mini-v2.img.xz`, `.txt` | about 1.5 GB |
| Checksums for people | `…/v<version>/SHA256SUMS` | |

`<owner>/pocket-ui` is the public repository (not created yet). The list names every file with its absolute URL, size
and sha256:

```json
{ "format": 1,
  "app": { "version": "0.2.0", "url": "https://github.com/<owner>/pocket-ui/releases/download/v0.2.0/pocket-ui-0.2.0-aarch64.tar.gz",
           "sha256": "…", "size": 8471397, "notes": "…" },
  "os": null,
  "sd_image": { "version": "0.2.0", "url": "…/v0.2.0/pocket-ui-0.2.0-rp-mini-v2.img.xz", "sha256": "…", "size": 1518697476, "notes": "…" },
  "install_zip": { "version": "0.2.0", "url": "…/v0.2.0/pocket-ui-0.2.0-rocknix-install.zip", "sha256": "…", "size": 8518997, "notes": "…" } }
```

`app` and `os` are what a device installs. `sd_image` and `install_zip` are for people (a download page, a checksum to
compare); a device ignores them. `os` stays empty until our own system build exists ([UPDATES.md](UPDATES.md#os-next-start-never-the-bootloader)).

Why the split:

- **Vercel** (the website, static) documents a 100 MB static CLI upload limit on the Hobby plan (1 GB on Pro), and the
  website is about 31 MB already. An image of 1.5 GB cannot go there; a list of 1 KB is ideal there.
- **GitHub Releases** takes files under 2 GiB each (the image is 1.52 GB: watch it; `release-channel.sh` refuses
  2 GiB or more), serves them to anyone without a token as long as the repository is public, and costs nothing. A
  download URL answers with a redirect (302) to GitHub's download host with a short-lived address; pocket-ui follows
  it (below).
- **Neither host is trusted.** The list is signed and every file is checked against its sha256, so a changed file or
  list on either host is refused. What the hosts have to provide is HTTPS, anonymous reads and stable URLs.

## The Brick card

The TrimUI Brick's own card ([TRIMUI-BRICK-REMOTE.md](TRIMUI-BRICK-REMOTE.md)) has a channel of its own, laid out the same way:
the signed list at `https://pocketui.xyz/updates/brick/update.json` (`update::BRICK_DEFAULT_URL`, asked by a card that has no
`app/update-url`, which is every card from the public zip), the files on the same GitHub release as the ROCKNIX ones. `app` is
`pocket-ui-brick-<version>.tar.gz`, the card's whole program folder (the device installs it with `update::install_brick`);
`install_zip` is `pocket-ui-trimui-card-<version>.zip`, for people. Same key, same checks, same caching rule (it is under
`/updates/`). The cores' source is not in the tarball: it is `pocket-ui-trimui-sources-<version>.zip` on the same release, next to
the card zip. No Tailscale and no account are involved; a private beta card asks the channel its `app/update-url` names instead.

    BRICK_PARTS=PARTS BRICK_SOURCES_APART=1 BRICK_POCKETUI_SOURCE=pocket-ui-<v>.tar.gz \
      scripts/brick-make-card.sh OUT BINS LIBS CORES <v>                 # no BRICK_UPDATE_URL: the card asks the public channel
    scripts/brick-publish.sh OUT/card --public OWNER/pocket-ui --out brick-public --zip OUT/pocket-ui-trimui-card-<v>.zip --notes "…"
    gh release upload v<v> brick-public/release/* OUT/pocket-ui-trimui-sources-<v>.zip --repo OWNER/pocket-ui
    scripts/release-channel.sh --check brick-public/updates/brick/update.json
    # then brick-public/updates/brick/ into the website's updates/brick/, deployed with it

`brick-publish.sh --public` refuses a build with an `app/update-url`, a beta version (`X.Y.Z-brick.N`) and a key pocket-ui does not
trust, and publishes nothing itself (tests: `scripts/test-brick-card.sh`, "public:").
Public Brick releases may also use `X.Y.Z.W` for a Brick-only hotfix. If it becomes GitHub Latest,
its notes must direct other handhelds to their unchanged ROCKNIX release; the stable channel stays unchanged.
`scripts/test-brick-hotfix-version.py` checks the real publishers' channel and version validation without signing.

## Caching

The list must never be served stale for long, and the list and its signature must always match.

- Vercel keeps static files in its edge cache for the lifetime of a deployment; a new deployment starts with a new
  cache. A deployment switches every file at once, so `update.json` and `update.json.sig` change together: there is
  no moment in which an old signature sits next to a new list (as there would be when two files are uploaded one by
  one to a plain server).
- Browsers and any cache between the website and a device are told not to keep the files. The rule for the website's
  `vercel.json` (`release-channel.sh` writes it as `site-updates/vercel-headers.json`):

  ```json
  { "source": "/updates/(.*)",
    "headers": [
      { "key": "Cache-Control", "value": "no-cache, no-store, must-revalidate" },
      { "key": "Pragma", "value": "no-cache" } ] }
  ```

  The website's `vercel.json` already has exactly this rule, after its general `/(.*)` rule (checked 2026-10-26);
  keep it last, so its `Cache-Control` wins over the general one.
- curl on the device keeps no cache, so every check asks the website again.
- The files on GitHub never change after the upload (a new version gets a new tag and new names), so their caching
  does not matter.

## The signing key

One Ed25519 key signs the update list and the store catalog. Its public half is compiled into pocket-ui
(`store::PUBLIC_KEY`, `src/store.rs`); a list signed with any other key is refused ("The update list isn't signed by
us. Nothing was changed.").

- **Where the private key lives:** `~/.config/pocket-ui/store-signing.key` on the build machine, mode 0600, outside
  every repository (`POCKETUI_SIGNING_KEY=<file>` or `--key <file>` point elsewhere). It is 32 raw bytes, as
  `store-tool keygen` writes it. Never in a repository, never on Vercel, never in a CI secret: signing is a step the
  owner runs by hand on the build machine.
- **A backup:** one offline copy (a password manager or an encrypted stick). Without the key no update can reach a
  device again: a new key means a new public key in the code, and devices only learn it from an update.
- **The release key (2026-10-26):** made on the build machine with `store-tool keygen` into the default path; its
  public half is `PUBLIC_KEY` in `src/store.rs`, and the test catalog is signed with it again. The earlier test key
  is retired: lists and catalogs signed with it are refused from this version on. The offline backup is the owner's.
- `release-channel.sh` signs a small probe first and refuses a key whose signature does not verify against the
  `PUBLIC_KEY` built into the same tree, before it hashes anything.
- If the key leaks, whoever has it can sign lists, but still has to get them onto pocketui.xyz or between a device
  and the website (HTTPS). Make a new pair, ship the new `PUBLIC_KEY` in a release signed with the old key, and from
  then on sign with the new one.

## Releasing, step by step

On the build machine, from a clean checkout of the release commit. Nothing below publishes anything until step 6.

1. **Build once**, so that the update, the install package and the image carry the very same programs:

   ```sh
   V=0.2.0
   POCKETUI_VERSION=$V cargo zigbuild --release --target aarch64-unknown-linux-gnu.2.38 --bin pocket-ui --bin pocket-core
   mkdir -p release-out/bin
   cp target/aarch64-unknown-linux-gnu/release/pocket-ui target/aarch64-unknown-linux-gnu/release/pocket-core release-out/bin/
   scripts/make-install-zip.sh --version $V --bin-dir release-out/bin --out release-out
   scripts/make-image.sh --version $V --bin-dir release-out/bin --out release-out     # optional: the SD image
   ```

   For an update supporting H700 GBA, build the pinned ARM64 gpSP with `scripts/build-gpsp.sh` and pass its output
   folder as `--gba-core BUILD_DIR` to both `make-install-zip.sh` and `release-channel.sh` below. This includes the
   verified core, licence, corresponding source and recipe; a prebuilt app archive missing them is refused.

2. **Check the artefacts** for BIOS files, ROMs and unapproved cores. The optional pinned gpSP is shipped only with
   its licence and corresponding source ([HOST.md](HOST.md#h700-gba)); source repositories stay free of core binaries.
   Write the release notes into `release-out/NOTES.txt` (shown on the Updates page and on the release page).
   For packaged gpSP artefacts, invoke `scripts/check-release-tree.py --allowlist deploy/gpsp/release-allowlist.txt`
   with the app archive, install zip or image. The normal source-tree check keeps rejecting third-party cores.
3. **Get the list that is live now**, so that what this release does not replace (a system update) is carried over:

   ```sh
   mkdir -p /tmp/live && cd /tmp/live && curl -fsSLO https://pocketui.xyz/updates/stable/update.json \
     && curl -fsSLO https://pocketui.xyz/updates/stable/update.json.sig; cd -
   ```

   (Before the first release there is none; leave out `--previous`.)
4. **Make and sign the list:**

   ```sh
   scripts/release-channel.sh --repo <owner>/pocket-ui --in release-out --previous /tmp/live/update.json
   ```

   It packs `pocket-ui-$V-aarch64.tar.gz` from `release-out/bin/` (the two programs and `deploy/portmaster-setup.sh`,
   the same bytes for the same files), checks both programs are aarch64 and carry the version, refuses a file of
   2 GiB or more and a key pocket-ui does not trust, and writes `site-updates/` (gitignored):

   ```
   site-updates/updates/stable/update.json, update.json.sig   → the website
   site-updates/updates/stable/pocket-ui-$V-aarch64.tar.gz    → the website (the app's first source)
   site-updates/updates/store/…                               → the website (only with release-out/store/)
   site-updates/github/pocket-ui-$V-aarch64.tar.gz, SHA256SUMS, notes.md, assets.txt
   site-updates/vercel-headers.json, NEXT-STEPS.txt
   ```

   The loose Ports scripts (`Install pocket-ui.sh`, `Uninstall pocket-ui.sh`) in `release-out/` are copied to
   `github/` under the names GitHub gives them (spaces become dots: `Install.pocket-ui.sh`), and `SHA256SUMS` lists
   them so; the ports-installer and rocknix-card zips of `release-out/` are listed too.

   `--help` lists the rest (`--os-tar`/`--os-version` for a system update, `--channel`, `--base-url`).
5. **Keep a copy** of `site-updates/` per version (`release-archive/<version>/` outside the repository): it is what a
   rollback deploys again.
6. **Upload the files** (the command is in `NEXT-STEPS.txt`; `gh` must be logged in as the repository's owner):

   ```sh
   gh release create v$V --repo <owner>/pocket-ui --title "pocket-ui $V" --notes-file site-updates/github/notes.md \
     $(cat site-updates/github/assets.txt)
   ```

7. **Check the uploads against the signed list**, before the list goes live:

   ```sh
   scripts/release-channel.sh --check site-updates/updates/stable/update.json
   ```

   It downloads every file the list names the way a device does (https only, GitHub's redirect followed) and compares
   size and sha256: `ok`, `MISSING` or `CHANGED` per file, exit 1 unless all are ok. The image is 1.5 GB, so this takes
   a while. The app's website copy is not live yet at this point: the one next to the list is compared instead
   ("not live yet"); run the check once more after step 8.
8. **Deploy the list**: copy `site-updates/updates/` into the website's folder (as `updates/`, next to `index.html`;
   `cp -R site-updates/updates/. <website>/updates/`, which keeps the files of earlier versions that are there), check that its `vercel.json` still has the `/updates/(.*)` rule
   ([Caching](#caching)), and deploy the website as usual (`vercel deploy --prod` from its folder).
9. **Look at it:**

   ```sh
   curl -sI https://pocketui.xyz/updates/stable/update.json | grep -i -E '^(HTTP|cache-control)'   # 200, no-cache
   ```

   then on a device: Settings → System → Updates.

The order matters: files first (6, 7), list last (8). A list that names a file not uploaded yet makes every device
that tries it say "The update file isn't on the server. Nothing was installed."

## Rollback

**On the device** nothing has to be done: an update that does not start, or does not show its first screen within
20 seconds, is replaced by the previous version by itself, and one that keeps crashing later by the last version that
ran well ([UPDATES.md](UPDATES.md#app-live-with-automatic-way-back)).

**On the channel**, when a release turns out bad:

1. **Stop it spreading:** put the previous list back. Either Vercel's *Instant Rollback* to the deployment before, or
   copy the archived `site-updates/updates/` of the previous version (step 5) into the website and deploy. The old
   list is still validly signed. Devices that have not installed the bad version stop seeing it.
2. **Devices that installed it** are not taken back by the channel: a device only offers a version newer than its own
   (`update::newer`), so that an old signed list can never downgrade a device. The fix goes forward: release the next
   version (0.2.1) with the fix, and the devices get it as a normal update.
3. **Keep the GitHub release** of every version a live list names: deleting its files turns every install of it into
   "The update file isn't on the server". Mark a bad release as such in its notes instead.

A stale or replayed old list (a cache that ignored the headers, someone between a device and the website) can only
hide a newer version, never install an older one. The update list has no serial number or expiry yet, unlike the
store catalog; see [what's left](#whats-left).

## What the device does

1. Settings → System → Updates checks when the page opens for the first time, and on **Check again**: curl fetches
   `update.json` and `update.json.sig` (15 s to connect, 60 s at most), and the signature is checked against
   `store::PUBLIC_KEY` before anything in the list is read.
2. It offers the app when the list's version is newer than its own, the system when it is newer than the running
   system's `OS_VERSION` (`/etc/os-release`).
3. **Install** downloads the file from GitHub: curl follows the redirect to GitHub's download host (`-L`), resumes a
   partial file from an earlier try (`-C -`), stops a file that grows past the signed size, and then the size and
   sha256 are compared with the signed list. Only then is it unpacked and checked further ([UPDATES.md](UPDATES.md)).
4. **HTTPS only.** Every curl call of the updater gets `--proto =https,file --proto-redir =https`: the list and the
   files must be on `https://` (or `file://`, a channel on the card for tests and the device test), and a redirect may
   only lead to `https://`. An `http://` address is refused before curl runs.

What the page says when something is not right:

| Situation | Message |
|---|---|
| No list on the website yet (404) or another error status | No updates published yet. A new version installs like the first one; your games and settings stay. |
| No network, or the website can't be reached | Couldn't reach the update server. Check Wi-Fi. |
| A network that wants a sign-in in a browser | Connected, but this network wants a sign-in in a browser. |
| List or signature changed, or another key | The update list isn't signed by us. Nothing was changed. |
| A file the list names is not on GitHub (404) | The update file isn't on the server. Nothing was installed. |
| A file differs from the list | The download is damaged or was changed. Nothing was installed. |
| `update_url` set to `http://…` | The update address has to start with https://. Nothing was changed. |
| The list names an `http://` file | The update list names a file that isn't on https. Nothing was installed. |
| A redirect to plain http | The update server sent an address that isn't https. Nothing was changed. |
| Not enough room on the SD card (before the download) | The SD card is too full for this update: it needs … MB more. Nothing was changed. |
| The card filled up during the download | The update couldn't be saved: the SD card is full or can't be written. Nothing was installed. |
| Still coming, but too slowly for the file's time | The download is too slow on this network. Nothing was installed. Try again on a faster Wi-Fi. |
| The clock is before this build and the secure connection fails | The clock looks wrong, so the update server can't be checked. Set it in Settings › System › Time, or wait for network time over Wi-Fi. |

**Overrides, for tests and development:** `"update_url"` in `/storage/.config/pocket-ui/settings.json` replaces
`update::DEFAULT_URL` (`https://…` or `file:///…`); `"store_url"` replaces `store::DEFAULT_URL`. Both are kept by
Backup & reset.

## The store catalog

There is no public catalog yet, so `store::DEFAULT_URL` stays empty and the Store row stays hidden
([STORE.md](STORE.md#when-the-store-is-offered)). When the first catalog is published:

- It goes to `https://pocketui.xyz/updates/store/catalog.json` with its `.sig`, signed with the same key, numbered
  with `serial` and given an `expires` (`scripts/make-test-catalog.sh` shows both).
- Put the catalog folder (signed `catalog.json`, `.sig`, `packages/`) into `release-out/store/`; `release-channel.sh`
  checks its signature and copies it to `site-updates/updates/store/`. Small packages can sit next to the catalog
  (relative URLs); anything big goes into a GitHub release with an absolute URL, since the whole website must stay
  under Vercel's 100 MB (the script warns above 20 MB in `updates/`).
- Put the address into `store::DEFAULT_URL` (and the user guide) in the release that ships with the first catalog;
  the row then appears by itself.
- The store's own downloads still allow `http://`, for the local test server of [STORE.md](STORE.md#local-test-catalog);
  its catalog and packages are signed and hashed like the update list.

## Tests

- `src/update.rs`: a real curl against a small server on 127.0.0.1 that answers like GitHub Releases (302 to a
  download host with a token in the URL, ranges for a resume): the file is fetched through the redirect, a partial
  file is resumed, other bytes are refused and deleted, a missing file gives its message, and with the production
  redirect rule a redirect to plain http is not followed. The test server speaks http, so that test allows http for
  the first request; the curl arguments and the scheme check of the production setting have tests of their own.
  Plus the messages for each curl exit code (stand-in curl).
- `tests/release_channel.rs`: `release-channel.sh` with made-up artefacts and a throwaway key from `store-tool
  keygen`: the signed list (verified with the key's public half and read with `update::parse_manifest`), the tarball's
  contents and that a rebuild gives the same bytes, the files for GitHub and their checksums, the headers file, that
  the private key ends up in none of the output; refusals (a key pocket-ui doesn't trust, no key, another
  architecture or version, 2 GiB, http); carrying a system update over from the live list and refusing a changed
  one; `--check` with all files there, one changed and one missing, and with the app's website copy not live yet.
  Every list it writes, and the live 0.2.2 list (`testdata/update-channel/`), is read with a frozen copy of the list
  as pocket-ui 0.1.7 to 0.2.2 read it: the app it finds is the same, on pocketui.xyz. Versions other than plain `X.Y.Z`
  are refused outside the Brick-only four-part hotfix exception above.

Not tested: anything against the real pocketui.xyz or GitHub (nothing is published), the real key (not on the build
machine), a device fetching from the public channel.

## What's left

- The owner's steps: the key ([above](#the-signing-key)), the public repository, the first release.
- A `serial` and `expires` in the update list, checked like the store catalog's (`store::check_fresh`), so that a
  replayed old list is refused instead of only hiding a newer version.
- A download page on the website that reads `sd_image` and `install_zip` from the list.

## Small standalone-card updates (public Brick channel)

The first public card must be rebuilt with `update/brick_assets.rs`. `brick-publish.sh --public`
refuses a UI binary without the updater's work-file names `.asset-moves` and `.restore-assets` (the manifest's own name is not
kept as text by an optimized build); existing pre-release
candidate archives must never be distributed against the small-update channel. Old beta cards
keep their private channel and its full archives. Full legacy archives remain installable by
the new updater. The marker is a build compatibility guard; signatures and SHA-256 provide trust.

The public package builder (`scripts/brick-update-pack.py`) excludes `cores/`, `bios-data/`
and six infrequently changing helper programs (`brick-egl`, `curl`, `dropbearmulti`, `syncthing`,
`tailscale`, `tailscaled`). Everything else is a complete app archive. Its embedded
`asset-packages.json` names each required file's exact size and SHA-256, and an immutable
one-file package with signed primary/fallback URLs. The cheat database (`cores/cht.pack`, 31 MB, one file) is one of them:
downloaded once and moved on unchanged, so regular app updates stay small. The app tar's verified hash authenticates
this inventory. Neither the publisher nor the device accepts packages over 80,000,000 bytes.

A matching installed immutable asset is hashed and moved into `app.new` with a durable recovery
manifest on FAT32. The fixed boot script restores it if activation is interrupted or the new version fails.
A missing or changed asset is downloaded into RAM and verified before any card writes,
unpacked in a separate temporary directory with the existing strict tar validator, checked to
contain only its named file, and moved into staging. No component can replace application
executables or player data: allowed paths are cores, BIOS data and the exact six helper names.
Free space includes inherited data, one temporary component and reserve; cancellation and all
failures leave `app` active. Only after every asset passes does the existing ready/pending/rename
sequence run. There is no binary patching and no dependency on one specific previous version.

`release-channel.sh --check` checks every embedded asset source after verifying the app tar,
including the fallback URL. It publishes nothing. The cores' sources remain provided alongside the full
card on the public release; they are never downloaded on the device during an app update.
Budget regular app releases separately from core changes and first installs.

When deploying prepared files after approval, retain the currently offered app and at least the
two previous app archives, plus every immutable asset any retained app references. Merge the
new files into the website tree; do not replace its complete updates folder with the new output.
A player may have opened an older signed offer before the new deployment. Never overwrite an
asset filename: its archive SHA-256 is its name. A changed asset receives a different name.
