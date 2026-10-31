# Supplementary Rust notices

`ndk` 0.9.0, `ndk-sys` 0.6.0+11769913 and `jni-sys-macros` 0.4.1 declare `MIT OR Apache-2.0`, but their registry
packages omit the licence files. These are the upstream MIT texts from the exact `.cargo_vcs_info.json` commits.
`sources.json` records immutable source URLs and hashes. The APK ships these with the generated dependency notices.
MIT is the selected option for these dependencies in the separate GPL-compatible player library.

`Handy-LGPL-2.1.txt` is the standard LGPL 2.1 text for Handy’s Blip_Buffer, copied verbatim from the system common-licenses package. Handy’s Stereo_Buffer is GPL-2.0-or-later; its source notices are retained separately.
