# Feature tour

*Part of [DEVELOPING.md](../DEVELOPING.md). Read this when you add a feature that needs a tour card.*

## Adding a feature to the tour

Every optional feature has one entry in `features::FEATURES` (`src/features.rs`). The first start's "How do you like to
play?" and "New in this version" read only that list, so a new feature needs no code in either of them (Settings → Help & tools → Tour is not
this list: it shows the six explained basics of `features::BASICS`, then the Big text choice; other new features do not go there):

```rust
Feature {
    id: "night_light",                   // (an example) stable; kept in settings.json (`tour_later`), used by the snapshots
    title: "Night light",                // the card's title, sentence case
    plain: "warmer colours after sunset", // what it does in everyday words, lower case, no feature names: the first start's "On: …, …" lists these
    line: "Warmer colours after sunset.",  // one sentence, ends with a full stop
    place: "Settings › Screen & sound › Night light", // where it is changed later (the fonts have no →, the UI writes ›)
    guide: "settings-sound.md#night-light",    // its section in docs/user/, checked by doc_checks
    since: "0.3.0",                      // the version it ships in: "New in this version" shows it after that update
    switch: Some(Switch { get: |s| s.night_light, set: |s, on| s.night_light = on }),
    simple: Pick::Off,                   // what the profiles do with it: On, Off or Keep
    enthusiast: Pick::On,
    preview: pv::night_light,            // fn(&mut Canvas, &mut Text) in feature_previews.rs
    todo: None,
    try_it: None,                        // Some("Try Night light"): after an update the card starts on On and A says this
},
```

`plain` is what the first start's profile page lists under "On:" and "Off:" instead of the title (a new user does not
know "Echoes" or "Pixel Path"): short, lower case, no feature names; `features::tests` checks it for every entry with a
switch, and its German line goes into `assets/i18n/de.tsv`. An entry without a switch repeats its title there.

1. **The switch** reads and writes `features::Switches`, a copy of what Settings keeps. When the feature's setting is
   not in `Switches` yet, add the field, fill it in `App::switches` and use and save it in `App::adopt_switches`
   (`app_tour.rs`) exactly as its Settings page does. A feature without a switch (`switch: None`, both picks `Keep`) is a
   card that says where it is; it is shown after an update and in "Choose for yourself", never in the Tour.
2. **The preview** paints the feature's screen at 1240 × 1080 in `pal()` with made-up data ("Sample game"), using the real
   model where there is one; the card scales it down. Things drawn over a game stay dark (`palette::scoped(0, …)`). No
   file, no network: a preview is painted when its card is first shown and must stay within a few milliseconds
   (`cargo run --release --example feature_cards` prints the times: 1 to 4 ms for a preview and 1.5 to 5 ms for a
   card's first paint with it on an M1 Max, 0.5 ms for each paint after that).
3. **The docs**: its section in `docs/user/` (the `guide` anchor), and a row in the table of `docs/user/tour.md` when
   it has a switch (in the registry's order), or its title in bold under the table when it has none. `cargo test`
   checks both.
4. **The order** of the entries is the order of the cards; switch cards come before the others.

`todo: Some("branch: what is missing")` keeps an entry out of every list until it is wired (none is now). To wire
one: give it its switch and picks, its preview, `since`, its own `guide` section, and set `todo: None`.

**"New in this version".** settings.json keeps `seen_version`, the version whose cards were shown (written by the first
start and, once per new version, after the first frame). `features::due` takes the entries with `since` newer than that
and not newer than the running version, at most three, newest first. A device set up before the registry has no
`seen_version` and counts as `features::BASELINE` (0.1.0, the version of the first entries), so an update never brings
cards for what was already there. Dev builds report Cargo's version; to see the cards, build with `POCKETUI_VERSION` newer
than an entry's `since`, use `--snapshot tour-new:<id>`, or (mock runs) `POCKETUI_NEW_IN`. A look worth trying has
`try_it` (Retro, 0.3.0): in "New in this version" only, while it is off, its card starts on On and A says "Try Retro", so
one press tries it and B leaves everything as it was.

**Asked elsewhere, notes, builds** (0.8.0). A switch the first start asks on a step of its own (ScreenScraper under Box art,
Order on the look step) is in `features::FIRST_START_ASKS`: "Choose for yourself" leaves it out, and the setup's copy of
the switches never overwrites what those steps set (`App` takes `take_tour` with the App's own values for them). A card
that must say more than its line (what ScreenScraper sends) has a `features::NOTES` entry, painted whole under its line across
the page; the preview gives up the height it takes. `features::in_this_build` keeps a card out of "New in this version" where its
feature is not in the build (ScreenScraper without pocket-ui's developer id, `screenscraper::shown`; tests use
`screenscraper::shown_for_this_thread`).

**Real box pictures picker** (0.8.0, owner 27.11.2026). In "New in this version" the ScreenScraper card is no On/Off/Later
card: `Cards::picker` paints the four kinds (`screenscraper::PICTURE_NAMES`, the rows under ScreenScraper in Settings) as
rows with a tick box on the left and the chosen kind's real example on the right (`features::SAMPLES`, `assets/ss-samples/`, Tobu Tobu
Girl's CC BY 4.0 print art, credited under the picture with `SAMPLE_CREDIT`; sources in `assets/ss-samples/SOURCES.md`).
The ticks start at `Switches::screenscraper_show` (`screenscraper::Show::default` on a device that never changed them). A
flips the chosen kind in `Cards::picks` only; START takes them into `Switches` with ScreenScraper On (`Out::Changed`, nothing
ticked: stays off); B puts the card off like Later and goes to the next card. `App::adopt_switches` sets the kinds before
On, so the first run fetches only those. The first start shows the same page: A on Your games' Real box pictures row (while
it is off) opens `Step::Cards` with `Mode::Pictures`; START gives the setup the kinds and On (`take_screenscraper_pictures`
before `take_screenscraper`, also in that order in `main.rs`), B nothing; both go back to the row. When "New in this
version" opens, the shelf's touch targets are cleared and a gesture in progress is dropped (`touch_rec.reset`), so a late
tap on its START hint can't become the card's START; the same drop happens whenever a button changes the first start's
page (`main.rs`, `Setup::at` before and after the press).
Snapshots: `--snapshot tour-new:screenscraper:<keys>` (d, u, a, b, s START; pressed on the cards only, nothing saved) and
`--snapshot setup:first-games:<keys>`.
