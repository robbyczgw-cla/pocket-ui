<h1 align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="docs/media/logo-on-dark.png">
    <img src="docs/media/logo-on-light.png" width="360" alt="pocket-ui">
  </picture>
</h1>

<p align="center"><b>pocket-ui OS puts every game in its real box on your retro handheld:</b><br>
take it off the shelf, click it in, and continue right where you stopped.</p>

<p align="center">
  <a href="https://pocketui.xyz"><img src="docs/media/hero.webp" width="620" alt="The shelf: Advance Wars ready to continue, the last games as cases and every console as a spine"></a>
</p>

<p align="center">
  <a href="https://pocketui.xyz/install"><b>Get it</b></a> ·
  <a href="https://pocketui.xyz/try">Try it in your browser</a> ·
  <a href="https://pocketui.xyz/#trailer">Watch the trailer</a>
</p>

## Why you'll like it

- **Real cases.** Cartridges, jewel cases and big cardboard boxes, made from photos of the real things. Press X and
  the case opens; turn it over and read what the game is about.
- **In it goes.** The cartridge clicks into its console, the old TV flickers on, and you're playing. Quit, and it
  pops back out and goes home to the shelf.
- **Continue.** Your last games wait half open on the shelf. One press and you're back in the very moment you left.
- **One pause menu for every console.** Save slots with pictures, Earlier to jump back a few minutes, screen frames
  and CRT looks, from Game Boy to GameCube.

Plus a Retro look in one switch, games sent from your phone, Game check, Kids Mode and updates over Wi-Fi.
[See everything it does](https://pocketui.xyz/features).

No account, no tracking, free and open source. Menus in English, German and Spanish.

## Get it

| Your handheld | How |
|---|---|
| Anbernic RG35XX family, RG28XX, RG34XX, RG40XX, RG CubeXX · Retroid Pocket Mini V2 · AYN Odin 2 family | [Flash one card, pop it in, play](https://pocketui.xyz/install) |
| TrimUI Brick and Smart Pro | [Unzip the card download onto an empty card](https://pocketui.xyz/install#brick) |
| Already on ROCKNIX | [Add pocket-ui in five minutes, undo it any time](https://pocketui.xyz/install#way-1) |

Three steps, one card, and your handheld's own system stays as it is. Retroid Pocket 5, Flip 2, 6 and Nova have beta
cards, and testers are very welcome.

You bring your own games and, for a few systems, BIOS files; pocket-ui includes neither. An empty shelf offers a
[starter pack](docs/STARTER-PACK.md) of free homebrew games.

[What plays on which handheld](https://pocketui.xyz/systems) · [All downloads](../../releases/latest)

## Learn more

- [Guide](https://pocketui.xyz/guide): your first evening, with pictures
- [Manual](docs/user/index.md): everything, page by page; also on the handheld under Settings → Help & tools
- [FAQ](https://pocketui.xyz/faq): short answers
- [What's new](CHANGELOG.md): every version
- [Status](docs/STATUS.md): every feature and where it has been played

Something off? **Settings → Help & tools → About → Report a problem** makes a file for a report, or
[open an issue](../../issues). Reports from your handheld help a lot.

## How it's made

pocket-ui is built by Robby Czesany together with AI coding agents (Claude and Codex). The agents write most of the
code; he decides what gets built and how it should look and feel, and plays it on his own handhelds. Questions are
welcome in [Discussions](https://github.com/robbyczgw-cla/pocket-ui/discussions).

## For developers

pocket-ui is written in Rust: `pocket-ui` is the shelf and menus, `pocket-core` our own libretro player.

```sh
cargo zigbuild --release --target aarch64-unknown-linux-gnu.2.38 --bin pocket-ui --bin pocket-core
cargo test --release
```

Set-up, the dev loop and the code layout: [docs/DEVELOPING.md](docs/DEVELOPING.md) ·
[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) · [every document](docs/README.md) ·
[CONTRIBUTING.md](CONTRIBUTING.md) · [code of conduct](CODE_OF_CONDUCT.md) · [SECURITY.md](SECURITY.md)

## Licence

pocket-ui is free software under the **GNU General Public License, version 2 or (at your option) any later version**
(`GPL-2.0-or-later`), without any warranty. The full text is in [LICENSE](LICENSE). Copyright (C) 2026 Robby Czesany.

pocket-ui is based on [ROCKNIX](https://rocknix.org) and is not endorsed by the ROCKNIX team. It includes no games,
ROMs or BIOS files. Everything that is not ours keeps its own licence, and some cores that ROCKNIX ships are for
non-commercial use only: read [NOTICE.md](NOTICE.md) before you redistribute an image.

## Credits

Thank you to ROCKNIX, libretro and the authors of the cores, PortMaster, RetroAchievements, Wikidata, Evan Amos and
everyone else in [CREDITS.md](CREDITS.md).
