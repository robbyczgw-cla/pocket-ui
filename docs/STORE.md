# Package downloads

`src/store.rs` remains the package backend for updates, the starter pack and tools. The Store page and
Settings row were removed in 0.4. Existing installed files and tools stay in place. The backend downloads
packages, verifies their checksums and tracks the installed files so removal affects only that package.

## Android core packages

Since 0.5.x the APK carries all 17 cores of the lock: PSP with its system files (`assets/system/PPSSPP`, copied to
`files/bios/PPSSPP` by the player), DS, N64 and Dreamcast as well. `package-cores.py` has an empty `DOWNLOADS` set, so
`assets/core-downloads.json` and `android/build/core-downloads/catalog.json` list no package and `release-apk.sh` copies no
catalog into the release folder; `release-check.sh` refuses an APK that leaves a core of the lock out or offers one as a
download (`apk_audit.py --lock`). The machinery below stays for a core that may need to be a download later.
A phone that downloaded one of the four with 0.5.0-android.5 or older uses the APK's copy (`corePath` asks for it first);
`android_cores::retire_bundled` removes the downloaded files, folder and store record, and unfinished downloads, at the
next start of either view. Saves, states, core options and `files/bios` stay.

`build-apk.sh` builds corresponding sources and calls `package-cores.py`. The APK's `cores.json` contains only its
bundled libraries; `assets/core-downloads.json` provides the trusted APK's size/hash/system offers. Optional archives
and `catalog.json` are in `android/build/core-downloads/`. Each archive includes a manifest with pinned source/hash
references and licences; PSP also carries all recorded `system/PPSSPP` files. All native LOAD segments are checked
for 16 KB alignment before packaging. `ANDROID_CORE_CACHE` can point to an existing read-only build cache; every
lock entry, binary hash, source hash and alignment is checked. `ANDROID_CORE_VERSION` and `ANDROID_CORE_SERIAL`
override the default timestamp version/serial when preparing a stable catalog.

Before serving a catalog, use `store-tool sign <private-key-file> <catalog.json>` with the existing store signing key.
The private key stays outside the checkout. Keep the matching corresponding-source archives with the channel assets;
the existing Android core-source export contains them. `android_cores::verified_catalog` calls the same shared
Ed25519/SHA functions as the APK updater and the store's expiry/replay guard. Installations use the existing store
inventory and safe extraction, with a fixed private destination for each core; catalog `dest` cannot escape it.

For local QA, copy the signed catalog and packages to the test app's private or external-files folder, then pass its
`file://` catalog address in the `coreCatalogUrl` launch extra. Other local paths are refused. Production requests use
HTTPS and unmetered Wi-Fi; private `launcher.json` `core_catalog_url` selects a channel. No public URL is configured
or published at this milestone. Archives are keyed by their signed SHA in `store/partials`; B/Wi-Fi loss leaves the
partial, the next choice sends Range, and an ignored Range response replaces it safely. Signature/hash failures
install nothing. `Store::install_bytes` replaces files only after the full package is checked.
