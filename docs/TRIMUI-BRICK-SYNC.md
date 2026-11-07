# Save sync on the Brick's own card

Settings > Connections > Sync works on the standalone card as on ROCKNIX: the same page, the same Syncthing folders, the same pairing
with other devices (src/sync.rs, docs/user/sync.md). What differs is only who runs Syncthing.

| | ROCKNIX | Brick card |
|---|---|---|
| Program | the image's `syncthing` | **Syncthing 2.1.7**, the official static arm64 build, unchanged (MPL-2.0, `licences/syncthing/`, source: the tag on GitHub; `scripts/brick-remote-parts.sh` pins sha256) |
| Service | `systemctl ... syncthing` | `systemctl ... syncthing` is the card's stand-in (`hwroot/usr/bin/systemctl`) and lands in `brick-syncthing start|stop|status|sync` |
| Boot | `syncthing.enabled` in system.cfg | the same line, read by `deploy/brick/start.sh` (like `tailscale.up`) |
| Home (keys, config.xml) | `/storage/.config/syncthing` | in RAM (`/tmp/pocket-ui/syncthing`): the card's FAT cannot keep file modes. A watcher copies `config.xml` and the keys to `storage/.config/syncthing` within 5 s (the place pocket-ui reads); `start` puts them back first, so the device ID stays |
| Index database | in the home | `--data` on the card, `storage/.cache/syncthing-db` |
| Web interface | 0.0.0.0:8384, TLS, user `pocket`, our password | the same; `config.xml` is set with `sed` (no xmlstarlet on the card), inside the `<gui>` and `<options>` blocks only; no password file means no start |
| Usage reports, crash reports, browser, self-upgrade | off | off |

pocket-ui's `/storage` is the card's folder bound there by `start.sh`, so every path in `sync.rs` (`/storage/roms`, `.stignore`, the password,
the REST address) is already right. Folders for systems the Brick does not have (PS2, GameCube, Wii, 3DS) are not offered because their
folders are missing.

## Checks

- `scripts/test-brick-card.sh` "Sync": a stand-in `syncthing` proves no start without a password, serve flags (RAM home, card database,
  no browser), password on stdin only, the `sed` result (all addresses, TLS, reports off, a device's own `<address>` untouched), the
  copy to the card, the keys coming back after a restart, no double start.
- Unit test in `app_settings_groups.rs`: the standalone card keeps the Sync row, NextUI's card does not.

## Only a real Brick can answer

- That the static arm64 Syncthing runs on the firmware's kernel (it needs nothing but the kernel) and how much of the 1 GB it takes
  while hashing; whether `serve --data` accepts the card's FAT (SQLite and file locks on FAT). If the database misbehaves there, move
  `CARD_DB` into RAM (a rescan of the save folders at each boot, small).
- Pairing with a ROCKNIX device over the LAN (local discovery) and over Tailscale.
- `generate --home` and the `config.xml` shape of 2.1.7 against the `sed` lines (the test uses a stand-in with the shape of Syncthing's).
