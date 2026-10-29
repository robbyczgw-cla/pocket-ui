# German glossary

The words the German interface uses, so every page says the same thing the same way. The table itself is
`assets/i18n/de.tsv`; how it works is in [DEVELOPING.md](DEVELOPING.md#translations).

## Adding a string

1. Write the English string in the code.
2. `scripts/i18n-extract.py --stub` lists every string without a German line as a table line to fill in.
3. Translate it in `assets/i18n/de.tsv` with the words below. A number in it is `{n}` (`{m}`, `{k}` for more), a
   name kept as it is `{s}`, a value the table translates `{t}`. A name that stays English goes into
   `assets/i18n/keep.txt`, a string that is never painted on its own (a log line, a key) into `assets/i18n/skip.txt`.
4. `cargo test --release --test i18n_snapshots` must pass; it also renders the states of `tests/i18n-states.txt` in
   German. Add the states of a new page there.
5. Look at the page with `scripts/i18n-shots.sh`, dark and light: nothing cut off or overlapping that isn't cut off
   in English too.

## The English it translates

English is the source and the default language. It follows a few rules, so the German has one thing to match:

- British spelling: colour, grey, favourites, catalogue, licence (the noun), centred, analogue, -ise.
- "45%" with no space (the German keeps "45 %"); "45 °C", "4.8 W", "12 ms" with one.
- A path in Settings: "Settings › Playing › Battery"; the manual writes "Settings → Playing → Battery".
- Buttons as printed: START, SELECT, HOME, Vol+ and Vol-; "the default" (never "the start setting").

## Tone

- **du**, never Sie. Calm and factual, as the English: no exclamation marks, no superlatives, nothing sold.
- Short. German runs about 30 % longer than English, and the pages were laid out for English: prefer the shorter
  of two good words ("Schnellmenü", not "Schnelleinstellungen"), drop filler ("bitte" only where English says
  "please"), and check the page in a snapshot (both appearances) before you commit.
- Instructions as statements or imperatives the way the English has them: "Drück A", "Noch einmal A meldet dich
  ab", "Wähl dein Netzwerk".
- Button names stay as printed on the Pocket: A, B, X, Y, L1, R1, L2, R2, SELECT, START, HOME. The D-pad's
  directions are words: Links/Rechts, Hoch/Runter.
- Typography: „deutsche Anführungszeichen“, an ellipsis after a space ("Wird geladen …"), a decimal comma only in
  sentences written out (numbers the code formats keep their point).

## Names that stay English

They are names, not descriptions, and the manual (still English) uses them:

| Kept | Why |
|---|---|
| pocket-ui, pocket-core, Curator, **Lens**, **Echoes**, **Pixel Path** | pocket-ui's own feature names |
| RetroAchievements, RetroArch, EmulationStation, ROCKNIX, PortMaster, Moonlight, Sunshine, Syncthing, Tailscale, Bluetooth, Store | products and services |
| Home | the name of the screen; also on the hint bar |
| Run-ahead, Grinding, Shader, Cheats, Core, Tools, Updates, Sync, Extras, Tour, Logs, Screenshot | established in German gaming and tech use |
| English, Deutsch | a language is always shown in its own name (Settings → Look & feel → Language) |
| Little, Big, Prime | the processor's core clusters (Settings → Playing → Performance & fan) |
| Game, system, console, emulator and core names; the names of people, makers and places | names |

## Terms

| English | German | Note |
|---|---|---|
| Settings | Einstellungen | |
| Settings' categories: Look & feel, Playing, Games, Connections, System | Aussehen, Spielen, Spiele, Verbindungen, System | a path: "Einstellungen › Spielen › Akku" |
| Playing (the category, the game status) | Spielen | one word for both: the table has one line per English word |
| Web companion | Web-Begleiter | the Pocket's Settings page is German, the pages in the browser stay English |
| History, Play history, Year in review, Month in review | Verlauf, Spielverlauf, Jahresrückblick, Monatsrückblick | |
| Session, streak | Sitzung, Serie | |
| Look (Settings → Look & feel → Look), look | Ansicht | "Choose a look" → "Wähl eine Ansicht" |
| Shelf, Minimal, Split, Cards, Cabinet (the looks) | Regal, Minimal, Geteilt, Karten, Vitrine | |
| Appearance: Dark, Light, Automatic | Darstellung: Dunkel, Hell, Automatisch | |
| Motion: Full, Reduced, Off | Bewegung: Voll, Reduziert, Aus | |
| Language | Sprache | |
| Button layout (Settings → Playing): Nintendo, Xbox | Tastenbelegung: Nintendo, Xbox | the two names stay; the GameCube option in Options has its own line |
| Continue (Home row and pause menu), Resume | Fortsetzen | one word for both places |
| SD card (the storage medium: "Free on the SD card", "Copy the file to the SD card again") | SD-Karte | never just "card": on the game card "Karte" is the page of a game (English "the game's card", German "Spielkarte"), and the SD card is its own word ("Bleibt auf der SD-Karte") |
| Memory card (a console's save file) | Memory Card | |
| Save state | Spielstand | a moment saved by pocket-core or the emulator; the menu action is "Spielstand speichern" |
| save, in-game save (`.srm`, memory card) | Spielspeicher | what the game itself writes |
| saves and states | Spielspeicher und Spielstände | |
| Save / Load (pause menu) | Speichern / Laden | |
| Slot | Platz | "Slot 3" → "Platz 3" |
| Auto, Before load (the pause menu's saved moments) | Auto, Vorher | "Vorher" is short enough for the Load pill; in sentences "der Spielstand vor dem Laden" |
| Earlier (the pause menu's rolling states), Earlier states (its setting) | Früher, Frühere Spielstände | "Früher" like "Vorher": short for the pill; a session start is "Sitzungsbeginn" |
| Quit | Beenden | |
| Pause menu | Pausenmenü | |
| Quick settings | Schnellmenü | |
| Options (a game's) | Optionen | |
| Picture, Shortcuts, Extras (Options groups) | Bild, Tastenkürzel, Extras | |
| Over the game (Settings → Playing, Options → Extras): Off, FPS, FPS + sensors, Pixel Path | Anzeige im Spiel: Aus, FPS, FPS + Sensoren, Pixel Path | what pocket-core draws over a game. Its line: "FPS", "CPU", "EMU" and the units stay; FPS + sensors says "Lüfter 45 %", "Lüfter aus", "lädt" (`osd_sensors`) |
| Screen frame(s) | Bildschirmrahmen | "frames" alone: Rahmen |
| Runs on | Läuft mit | |
| Achievements | Erfolge | the service stays RetroAchievements |
| Smart shelf | Smartes Regal | |
| Sticker, Stickers | Sticker | plural without -s |
| Up next | Als Nächstes | |
| Versions, preferred | Versionen, bevorzugt | |
| Star / Unstar, starred | Stern / Stern weg, mit Stern | the hint bar is short |
| Favourites | Favoriten | |
| Pin / Unpin | Anheften / Lösen | |
| Recent games, Recently played | Zuletzt gespielt | |
| Wi-Fi | WLAN | |
| Pair, pairing | koppeln, Kopplung | |
| Forget (a device, a network) | vergessen | |
| Remote access (SSH) | Fernzugriff (SSH) | |
| File sharing | Dateifreigabe | |
| PC streaming | PC-Streaming | |
| Sleep | Ruhezustand | |
| Power off, Restart | Ausschalten, Neu starten | |
| Power (the button, the row) | Ein/Aus | |
| Battery | Akku | |
| Haptics, Sounds | Haptik, Töne | |
| Performance & fan, Smooth fan | Leistung & Lüfter, Sanfter Lüfter | |
| Balanced, Battery, Performance (CPU) | Ausgewogen, Akku, Leistung | |
| Quiet, Cool (fan) | Leise, Kühl | |
| Fast-forward, Rewind | Vorspulen, Zurückspulen | |
| Cover art, box art | Cover | |
| Pictures (Settings → Games → Extras); screenshot, title screen | Spielbilder; Screenshot, Titelbild | the collection's in-game picture and title screen beside the box art; "Box art" → "Cover", "Box art and screenshots" → "Cover und Screenshots" |
| Safe mode | Abgesicherter Modus | |
| Backup & reset | Sichern & zurücksetzen | |
| Manual, user guide | Handbuch, Anleitung | |
| About | Über | |
| Back, Open, Switch (a toggle), Change, Close | Zurück, Öffnen, Umschalten, Ändern, Schließen | hint bar |
| On, Off | An, Aus | |

## Left in English for now

- **The manual** (Settings → System → Manual, the pages of `docs/user/`): its text stays English; its title, page line and
  hints around it are German. The painter is switched off while its pages are drawn (`Text::set_translate`).
- **Console fact sheets and credits** (`assets/data/consoles.json`, `console-credits.json`): the summaries, hardware
  values and notes are data written in English; the headings and labels around them are German.
- **Data from elsewhere**: game names and descriptions, Wikipedia and Wikidata texts (genres such as "Platform
  game"), PortMaster's descriptions and instructions, RetroAchievements' achievement names, device and network
  names, time zone city names.
- **Messages of other programs** passed through as they are (bluetoothctl, Moonlight, curl errors, a core's
  error text), and technical details inside a German sentence: "Dieses System-Update würde pocket-ui kaputt machen
  (libEGL.so.1 of the new system can't be read to check it)", "Die Store-Liste ist nicht lesbar (catalog: …)".
- **The game card's one-line fallback** ("A 1993 platform game by Nintendo, part of the Super Mario series."): it is
  put together from Wikidata's English genre names and shown only when no description could be loaded, like the
  English description it stands in for.
- **Same English word, two meanings**: the table is keyed by the English text, so a word gets one translation
  everywhere. Where that matters, the English uses two words: the hint is "Choose" ("Wählen"), the button
  "Select"; a network without a password says "No password" ("Ohne Passwort"), not the hint's "Open" ("Öffnen");
  the shelf's fact row is "Runs on" ("Läuft mit"), the console page's heading "On this Pocket". Left as it is:
  "Clear" is "Leeren" (Lens: clear the filters), also where it names a see-through shell colour.
- **A language on its own** is shown in its own name: "English" and "Deutsch" in Settings → Look & feel → Language and on the
  Welcome page (`i18n::Cache::get`). In a game's list of languages it is a part of a line and German: "USA ·
  Englisch, Französisch".
- **Names inside a line**: a line of parts ("PlayStation 2 · 2005 · Rennspiel") translates the parts the table has
  and leaves the rest. A list split at commas leaves only its first part as it is ("PlayStation 2, durchgespielt"),
  so a game called "Pokémon Red, Blue, and Yellow" is never turned into "Pokémon Red, Blau, …".

## Wi-Fi connection errors

Connection diagnostics from nmcli stay in the log. The page uses fixed sentences, including
"The network took too long to respond. Try again." → "Das Netzwerk antwortet nicht. Versuch es noch einmal."
and "The connection didn't work. Try again." → "Die Verbindung hat nicht geklappt. Versuch es noch einmal."
Missing secrets still lead to the password prompt; unavailable networks suggest scanning again. Unknown errors
use "Keine Verbindung zum Netzwerk. Versuch es noch einmal." without quoting technical details.

For Wi-Fi, use "Versuch es noch einmal" consistently. A device still starting says
"WLAN ist noch nicht bereit. Versuch es gleich noch einmal."; it does not blame the network. Failed removal uses
"Das Netzwerk ließ sich nicht entfernen" and a failed country change uses "Das WLAN-Land ließ sich nicht einstellen".
