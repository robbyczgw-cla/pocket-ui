# ScreenScraper answers for the tests

Answers of the ScreenScraper API v2 (`output=json`, https://www.screenscraper.fr/webapi2.php): numbers as strings,
`response.ssuser` with the limits, `response.jeu` with `medias` as a list of `{type, parent, region, format, …}`.
Used by `src/screenscraper_tests.rs`.

- `ssuserInfos.json`, `jeuInfos-found.json`: written by hand before the first live run, in the shape it confirmed.
- `jeuInfos-series.json`: a live guest answer of 26.11.2026 (Metroid: Zero Mission, GBA), cut to the fields the client
  reads (`familles`, `numeros`, `systeme`, `rom.romregions`, three of its `medias`, `ssuser`); no address, id or
  password in it (`examples/screenscraper_live.rs --record` leaves `commandRequested` out). Data: ScreenScraper.fr,
  CC BY-NC-SA 4.0.
- `jeuInfos-facts.json`: a made-up game with every fact the card reads (`screenscraper_facts`): descriptions in three languages and one unknown, the main genre second, players with spaces, entities, dates in four regions.
