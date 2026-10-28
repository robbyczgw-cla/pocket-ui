# The update channel on pocketui.xyz

Status: 2026-10-26. Designed, built and tested on the build machine with made-up artefacts and a throwaway key.
**Nothing is published yet**: no list on pocketui.xyz, no GitHub release. How the device installs an update and
rolls back is in [UPDATES.md](UPDATES.md); this page is about where the files live and how a release gets there.

## Layout

A small signed list on the website, the big files on GitHub Releases of the public repository:

| What | Where | Size |
|---|---|---|
| The update list | `https://pocketui.xyz/updates/stable/update.json` (`update::DEFAULT_URL`) | about 1 KB |
| Its signature | `https://pocketui.xyz/updates/stable/update.json.sig` (base64 Ed25519 of the exact bytes) | 88 bytes |
| The store catalog, once there is one | `https://pocketui.xyz/updates/store/catalog.json` and `.sig` ([below](#the-store-catalog)) | a few KB |
| The app update | `https://github.com/<owner>/pocket-ui/releases/download/v<version>/pocket-ui-<version>-aarch64.tar.gz` | about 8 MB |
| The install package for a ROCKNIX card | `…/v<version>/pocket-ui-<version>-rocknix-install.zip` | about 8 MB |
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

- **Vercel** (the website, static) takes at most 100 MB per deployment on the Hobby plan (1 GB on Pro), and the
  website is about 31 MB already. An image of 1.5 GB cannot go there; a list of 1 KB is ideal there.
- **GitHub Releases** takes files under 2 GiB each (the image is 1.52 GB: watch it; `release-channel.sh` refuses
  2 GiB or more), serves them to anyone without a token as long as the repository is public, and costs nothing. A
  download URL answers with a redirect (302) to GitHub's download host with a short-lived address; pocket-ui follows
  it (below).
- **Neither host is trusted.** The list is signed and every file is checked against its sha256, so a changed file or
  list on either host is refused. What the hosts have to provide is HTTPS, anonymous reads and stable URLs.

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

2. **Check the artefacts** for BIOS files, ROMs and cores (none of them may ship),
   and write the release notes into `release-out/NOTES.txt` (shown on the Updates page and on the release page).
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
   site-updates/updates/store/…                               → the website (only with release-out/store/)
   site-updates/github/pocket-ui-$V-aarch64.tar.gz, SHA256SUMS, notes.md, assets.txt
   site-updates/vercel-headers.json, NEXT-STEPS.txt
   ```

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
   a while.
8. **Deploy the list**: copy `site-updates/updates/` into the website's folder (as `updates/`, next to `index.html`;
   `cp -R site-updates/updates/. <website>/updates/`), check that its `vercel.json` still has the `/updates/(.*)` rule
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
| No list on the website yet (404) or another error status | The update server has no update list right now. Try again later. |
| No network, or the website can't be reached | Couldn't reach the update server. Check Wi-Fi. |
| A network that wants a sign-in in a browser | Connected, but this network wants a sign-in in a browser. |
| List or signature changed, or another key | The update list isn't signed by us. Nothing was changed. |
| A file the list names is not on GitHub (404) | The update file isn't on the server. Nothing was installed. |
| A file differs from the list | The download is damaged or was changed. Nothing was installed. |
| `update_url` set to `http://…` | The update address has to start with https://. Nothing was changed. |
| The list names an `http://` file | The update list names a file that isn't on https. Nothing was installed. |
| A redirect to plain http | The update server sent an address that isn't https. Nothing was changed. |

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
  one; `--check` with all files there, one changed and one missing.

Not tested: anything against the real pocketui.xyz or GitHub (nothing is published), the real key (not on the build
machine), a device fetching from the public channel.

## What's left

- The owner's steps: the key ([above](#the-signing-key)), the public repository, the first release.
- A `serial` and `expires` in the update list, checked like the store catalog's (`store::check_fresh`), so that a
  replayed old list is refused instead of only hiding a newer version.
- A download page on the website that reads `sd_image` and `install_zip` from the list.
