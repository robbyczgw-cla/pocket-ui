# Package downloads

`src/store.rs` remains the package backend for updates, the starter pack and tools. The Store page and
Settings row were removed in 0.4. Existing installed files and tools stay in place. The backend downloads
packages, verifies their checksums and tracks the installed files so removal affects only that package.

## Android core packages

The 17-core lock keeps thirteen libraries in the APK, including the small cores through PS1 and Saturn.
PSP (with its system files), DS, N64 and Dreamcast are optional signed packages. The measured compressed libraries
are approximately PSP 7.33 MB (plus its system assets), DS 1.84 MB, N64 1.53 MB and Dreamcast 4.75 MB.
The threshold is 1.5 MB for a candidate system library; PSP also brings system assets. Saturn's corrected YabaSanshiro
library is about 0.69 MB compressed and stays bundled. Sizes painted in first start and Extras come from the actual
generated tar.gz, including notices and PSP assets, rather than these library-only measurements.

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
