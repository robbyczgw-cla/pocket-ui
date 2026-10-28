# Contributing

Thanks for looking. pocket-ui is a small project: one maintainer, one device (the Retroid Pocket Mini V2) and a
virtual device in Docker. That shapes what helps most.

## Reports from real devices are the most useful thing

Most of the code was tested in the virtual device or in unit tests only; [docs/STATUS.md](docs/STATUS.md) says which.
If you run pocket-ui on a Mini V2, an issue that says "I tried X on ROCKNIX <version>, and this happened" is worth
more than a feature idea. Use the bug form; it asks for the details that matter (device, ROCKNIX version, pocket-ui
version, how you installed it).

## Before you open a pull request

- For anything larger than a fix, open an issue first, so we can agree on the approach before you spend the time.
- Build and test as [docs/DEVELOPING.md](docs/DEVELOPING.md) describes. At least `cargo test --release` must pass;
  the documentation checks in it also cover the manual (`docs/user/`) and the links.
- If your change makes a feature work better or worse on the device, update its row in
  [docs/STATUS.md](docs/STATUS.md) in the same pull request, and say what you ran it on.
- A change people see goes into the manual (`docs/user/`) in the same pull request. New interface text needs its
  German line in `assets/i18n/de.tsv` (the tests tell you which).
- Keep speed in mind: the shelf has a speed guard (`scripts/latency.sh`, [docs/LATENCY.md](docs/LATENCY.md)).

## Ground rules

- **No ROMs, BIOS files, firmware, keys or links to them.** Not in code, tests, issues or screenshots.
  `scripts/check-release-tree.py` checks the tree; test data is either generated or our own.
- **No private data** in logs, screenshots or test fixtures: Wi-Fi names, addresses, tailnet names, serial
  numbers, Bluetooth addresses of your devices. Replace them with made-up values (the documentation ranges
  `192.0.2.0/24` and `198.51.100.0/24` work well).
- Contributions are accepted under the project's licence, GPL-2.0-or-later ([LICENSE](LICENSE)). By opening a pull
  request you confirm that you have the right to contribute the code under that licence.
- Third-party code or assets need their licence and source recorded in [NOTICE.md](NOTICE.md).

## Code of conduct

Everyone who takes part is expected to follow the [code of conduct](CODE_OF_CONDUCT.md).
