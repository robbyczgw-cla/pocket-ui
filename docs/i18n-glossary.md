# German glossary

The words the German interface uses, so every page says the same thing the same way. The table itself is
`assets/i18n/de.tsv`; how it works is in [dev/translations.md](dev/translations.md#translations).

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
- A path in Settings: "Settings › System › Battery"; the manual writes "Settings → System → Battery".
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
| English, Deutsch | a language is always shown in its own name (Settings → System → Language) |
| Little, Big, Prime | the processor's core clusters (Settings → Playing → Performance & fan) |
| Game, system, console, emulator and core names; the names of people, makers and places | names |

## Terms

| English | German | Note |
|---|---|---|
| Settings | Einstellungen | |
| Settings' categories: Connections, Look, Screen & sound, Controls, Playing, Games, System, Help & tools | Verbindungen, Aussehen, Bildschirm & Ton, Steuerung, Im Spiel, Spiele, System, Hilfe & Werkzeuge | a path: "Einstellungen › System › Akku" |
| Playing (the category) | Im Spiel | not "Spielen", one letter from "Spiele" beside it (Settings v2) |
| Web companion | Web-Begleiter | the Pocket's Settings page is German, the pages in the browser stay English |
| History, Play history, Year in review, Month in review | Verlauf, Spielverlauf, Jahresrückblick, Monatsrückblick | |
| Session, streak | Sitzung, Serie | |
| Look (Settings → Look → Layout), look | Ansicht | "Choose a look" → "Wähl eine Ansicht" |
| Shelf, Split, Cabinet (the looks; Minimal and Cards went with 0.4) | Regal, Geteilt, Vitrine | |
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
| Haptics, Menu sounds | Haptik, Menütöne | |
| Performance & fan, Smooth fan | Leistung & Lüfter, Sanfter Lüfter | |
| Balanced, Battery, Performance (CPU) | Ausgewogen, Akku, Leistung | |
| Quiet, Cool (fan) | Leise, Kühl | |
| Fast-forward, Rewind | Vorspulen, Zurückspulen | |
| Cover art, box art | Cover | |
| Pictures (Settings → Games → Box art & downloads); screenshot, title screen | Spielbilder; Screenshot, Titelbild | the collection's in-game picture and title screen beside the box art; "Box art" → "Cover", "Box art and screenshots" → "Cover und Screenshots" |
| Safe mode | Abgesicherter Modus | |
| Backup & reset | Sichern & zurücksetzen | |
| Manual, user guide | Handbuch, Anleitung | |
| About | Info | the Help & tools row and its page |
| Tools | Werkzeuge | as in "Hilfe & Werkzeuge" |
| Back, Open, Switch (a toggle), Change, Close | Zurück, Öffnen, Umschalten, Ändern, Schließen | hint bar |
| On, Off | An, Aus | |

## Left in English for now

- **The manual** (Settings → Help & tools → Manual, the pages of `docs/user/`): its text stays English; its title, page line and
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
- **A language on its own** is shown in its own name: "English" and "Deutsch" in Settings → System → Language and on the
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

# Spanish glossary

The words the Spanish interface uses (`assets/i18n/es.tsv`), translated from the English source, not from German.
The rules for adding a string are the ones at the top of this page; every line of `de.tsv` gets its `es.tsv` line before a
release (`i18n`'s release check `every_language_has_every_line`).

## Tone

- Neutral international Spanish for Spain and Latin America: **tú**, no vos, no vosotros, no regional word where a
  neutral one exists (`archivo`, not `fichero`; `PC`, not `ordenador` or `computadora`; `teléfono`, not `móvil` or `celular`).
- A retro handheld player talking: short, friendly, plain. No machine-stiff passives, no "por favor" unless the
  English says please, no exclamation marks the English doesn't have.
- Imperatives in tú: "Pulsa A", "Elige tu red", "Inténtalo de nuevo".
- Short. Spanish runs longer than English and the pages were laid out for English: take the shorter of two good words,
  drop articles in labels and hints. Hint bar and pills: one or two words.
- Typography: ¿…? and ¡…! in pairs, “comillas”, an ellipsis with no space ("Cargando…"), "45 %" and "12 h" with a
  space, numbers as the code formats them. Dates: "14 de marzo de 2025", short "14 mar 2025"; months and weekdays are
  lower case inside a date and a capital when alone. Time stays 24-hour.

## What stays as it is

pocket-ui, pocket-core, Curator, Lens, Echoes, Pixel Path, ROCKNIX, RetroArch, RetroAchievements, EmulationStation,
PortMaster, Moonlight, Sunshine, Syncthing, Tailscale, Bluetooth, Store, Wi-Fi; console, game, emulator and core names; button
names as printed (A, B, X, Y, L1, R1, L2, R2, SELECT, START, HOME, Vol+, Vol-); Run-ahead, Grinding, Shader, Core,
Extras, Sticker(s), Home, FPS, CPU, "Pocket" ("el Pocket", masculine); Little, Big, Prime; English, Deutsch, Español as a language's own name.

## Terms

| English | Spanish | Note |
|---|---|---|
| Settings | Ajustes | a path: "Ajustes › Sistema › Batería" |
| Settings' categories: Connections, Look, Screen & sound, Controls, Playing, Games, System, Help & tools | Conexiones, Aspecto, Pantalla y sonido, Controles, Jugando, Juegos, Sistema, Ayuda y herramientas | |
| Look (Settings → Look → Layout), look | Vista | "Choose a look" → "Elige una vista" |
| Shelf, Split, Cabinet (the looks; Minimal and Cards went with 0.4) | Estante, Dividida, Vitrina | |
| Appearance: Dark, Light, Automatic | Apariencia: Oscuro, Claro, Automático | |
| Motion: Full, Reduced, Off | Movimiento: Completo, Reducido, Desactivado | |
| Language | Idioma | |
| Button layout | Disposición de botones | |
| Continue, Resume | Continuar | one word for both |
| Game card (a game's page) | ficha del juego, ficha | never "tarjeta" |
| SD card | tarjeta SD | never just "tarjeta" |
| Memory card | tarjeta de memoria | |
| save, in-game save (what the game writes) | partida guardada, partida | |
| Save state | estado guardado, estado | a moment saved by pocket-core or the emulator |
| saves and states | partidas y estados | |
| Save / Load (pause menu) | Guardar / Cargar | |
| Slot | Ranura | |
| Before load, Earlier (pause menu pills) | Antes, Anterior | "Earlier states": "Estados anteriores" |
| Quit | Salir | |
| Pause menu, Quick settings | Menú de pausa, Ajustes rápidos | |
| Options (a game's); Picture, Shortcuts | Opciones; Imagen, Atajos | |
| Over the game | Sobre el juego | what pocket-core draws over a game |
| Screen frame(s); frames | Marco(s) de pantalla; marcos | |
| Moment; Continue | Momento; Continuar | An open case means a moment is waiting; the word Frozen is gone from the player's words |
| Runs on | Funciona con | |
| Achievements | Logros | |
| Smart shelf | Estante inteligente | |
| Up next | A continuación | |
| Versions, preferred | Versiones, preferida | |
| Star / Unstar, starred | Favorito / Quitar, favoritos | a star is a favourite |
| Favourites | Favoritos | |
| Pin / Unpin | Fijar / Soltar | |
| Recent games, Recently played | Recientes, Jugado hace poco | |
| Home (the screen) | Home | a name, as in the manual; also on the hint bar |
| Game check | Revisión de juegos | |
| Controller | mando | |
| Press (a button) | pulsar | |
| D-pad directions | Izquierda/Derecha, Arriba/Abajo; hint bar "Izq./Der." | |
| Pair, pairing; Forget | vincular, vinculación; olvidar | |
| Remote access; File sharing; PC streaming | Acceso remoto; Compartir archivos; Streaming desde PC | |
| Web companion | Compañero web | |
| Sleep; Power off; Restart; Power | Suspender; Apagar; Reiniciar; Encendido | |
| Battery; Haptics; Sounds | Batería; Vibración; Sonidos | |
| Performance & fan, Smooth fan | Rendimiento y ventilador, Ventilador suave | |
| Balanced, Quiet, Cool | Equilibrado, Silencioso, Fresco | |
| Fast-forward, Rewind | Avance rápido, Retroceso | |
| Cover art, box art | Portada | |
| Pictures; screenshot; title screen | Imágenes; captura; pantalla de título | |
| Cheats; Patch; Tour; Updates; Sync; Logs | Trucos; Parche; Recorrido; Actualizaciones; Sincronizar; Registros | |
| History, Play history, Year in review, Month in review | Historial, Historial de juego, Resumen del año, Resumen del mes | |
| Session, streak | Sesión, racha | |
| Safe mode; Backup & reset | Modo seguro; Copia y restablecer | |
| Manual; About; Tools | Manual; Acerca de; Herramientas | |
| Back, Open, Switch, Change, Close, Choose, Select | Atrás, Abrir, Cambiar, Cambiar, Cerrar, Elegir, Seleccionar | |
| On, Off | Activado, Desactivado | |
| Delete; Keep; Folder; Password; Download | Borrar; Conservar; Carpeta; Contraseña; Descargar | |
| Hold; online; D-pad; slot; rating; clock | mantener pulsado; con conexión; cruceta; ranura; puntuación; frecuencia | |
| Clamshell | Plegable | |

## Left in English

The same as in German, above: the manual's pages (the painter is switched off while they are drawn), data from
elsewhere, messages of other programs, names.

# Brazilian Portuguese glossary

The words the Brazilian Portuguese interface uses (`assets/i18n/pt.tsv`, language name "Português (Brasil)", code `pt`),
translated from the English source, not from German or Spanish. Every line of `de.tsv` gets its `pt.tsv` line before a release
(`i18n`'s release check `every_language_has_every_line`). Any `pt` locale (`pt_BR`, `pt-PT`, `pt`) starts it.

## Tone

- Brazilian Portuguese, **você** (never tu, never o senhor): the way a Brazilian retro-handheld player talks to a friend.
  Short, friendly, plain. No stiff passives, no "por favor" unless the English says please, no exclamation marks the English
  doesn't have. Gamer words people really use (`save`, `save state`, `slot`, `cheats`, `patch`, `online`, `overlay`) stay.
- Imperatives in você form: "Aperte A", "Escolha sua rede", "Tente de novo". Press = **apertar** (not pressionar).
- Short. Portuguese runs longer than English and the pages were laid out for English: take the shorter of two good words,
  drop articles in labels and hints. Hint bar and pills: one or two words.
- Typography: “aspas”, an ellipsis with no space ("Carregando…"), "45%" with no space like the English, "45 °C", "12 h" and
  "4,8 W" only where the code doesn't format the number; numbers as the code formats them. Dates: "14 de março de 2025", short
  "14 mar 2025"; months and weekdays lower case inside a date and a capital when alone. Time stays 24-hour.
- Brazilian spelling (since the 2009 agreement): "ação", "projeto", "tela", "arquivo", "celular", "controle", "baixar", "time"
  (not "ecrã", "ficheiro", "telemóvel", "comando", "descarregar", "equipa").

## What stays as it is

pocket-ui, pocket-core, Curator, Lens, Echoes, Pixel Path, ROCKNIX, RetroArch, RetroAchievements, EmulationStation,
PortMaster, Moonlight, Sunshine, Syncthing, Tailscale, Bluetooth, Store, Wi-Fi; game, emulator and core names; button names as
printed (A, B, X, Y, L1, R1, L2, R2, SELECT, START, HOME, Vol+, Vol-); Run-ahead, Grinding, Shader, Core, Extras, Sticker(s),
Home, FPS, CPU, "Pocket" ("o Pocket", masculine); Little, Big, Prime; "Português (Brasil)" as the language's own name.
Console names as Brazilians say them: Mega Drive (not Genesis), Master System, Super Nintendo / SNES, Nintendo 64, PlayStation,
Game Boy Advance, Neo Geo Pocket Color. Game names are never translated.

## Terms

| English | Brazilian Portuguese | Note |
|---|---|---|
| Settings | Configurações | a path: "Configurações › Sistema › Bateria" |
| Settings' categories: Connections, Look, Screen & sound, Controls, Playing, Games, System, Help & tools | Conexões, Visual, Tela e som, Controles, Jogando, Jogos, Sistema, Ajuda e ferramentas | |
| Look (Settings → Look → Layout), look | Visual | "Choose a look" → "Escolha um visual" |
| Shelf, Split, Cabinet (the looks) | Estante, Dividido, Vitrine | |
| Appearance: Dark, Light, Automatic | Aparência: Escuro, Claro, Automático | |
| Motion: Full, Reduced, Off | Movimento: Completo, Reduzido, Desligado | |
| Language | Idioma | |
| Button layout | Layout dos botões | |
| Continue, Resume | Continuar | one word for both |
| Game card (a game's page) | ficha do jogo, ficha | never "cartão" |
| SD card | cartão SD | never just "cartão" |
| Memory card | Memory Card | |
| save, in-game save (what the game writes) | save, save do jogo | |
| Save state | save state | a moment saved by pocket-core or the emulator |
| saves and states | saves e save states | |
| Save / Load (pause menu) | Salvar / Carregar | |
| Slot | Slot | |
| Before load, Earlier (pause menu pills) | Antes, Anterior | "Earlier states": "Estados anteriores" |
| Quit | Sair | |
| Pause menu, Quick settings | Menu de pausa, Configurações rápidas | |
| Options (a game's); Picture, Shortcuts | Opções; Imagem, Atalhos | |
| Over the game | Sobreposição | what pocket-core draws over a game |
| Screen frame(s); frames | Moldura(s) de tela; molduras | |
| Moment; Continue | Momento; Continuar | an open case means a moment is waiting |
| Runs on | Roda em | |
| Achievements | Conquistas | the service stays RetroAchievements |
| Smart shelf | Estante inteligente | |
| Up next | A seguir | |
| Versions, preferred | Versões, preferida | |
| Star / Unstar, starred | Favoritar / Desfavoritar, favoritos | a star is a favourite |
| Favourites | Favoritos | |
| Pin / Unpin | Fixar / Desafixar | |
| Recent games, Recently played | Recentes, Jogados recentemente | |
| Home (the screen) | Home | a name, as in the manual; also on the hint bar |
| Game check | Verificação de jogos | |
| Controller | controle | |
| Press (a button) | apertar | |
| D-pad; directions | direcional; Esquerda/Direita, Cima/Baixo; hint bar "Esq./Dir." | |
| Pair, pairing; Forget | parear, pareamento; esquecer | |
| Remote access; File sharing; PC streaming | Acesso remoto; Compartilhar arquivos; Streaming do PC | |
| Web companion | Companheiro web | |
| Sleep; Power off; Restart; Power | Suspender; Desligar; Reiniciar; Energia | |
| Battery; Haptics; Sounds | Bateria; Vibração; Sons | |
| Performance & fan, Smooth fan | Desempenho e ventoinha, Ventoinha suave | |
| Balanced, Quiet, Cool | Equilibrado, Silencioso, Frio | |
| Fast-forward, Rewind | Avanço rápido, Rebobinar | |
| Cover art, box art | Capa | |
| Pictures; screenshot; title screen | Imagens; captura de tela; tela de título | |
| Cheats; Patch; Tour; Updates; Sync; Logs | Cheats; Patch; Tour; Atualizações; Sincronizar; Registros | |
| History, Play history, Year in review, Month in review | Histórico, Histórico de jogo, Resumo do ano, Resumo do mês | |
| Session, streak | Sessão, sequência | |
| Safe mode; Backup & reset | Modo seguro; Backup e redefinição | |
| Manual; About; Tools | Manual; Sobre; Ferramentas | |
| Back, Open, Switch, Change, Close, Choose, Select | Voltar, Abrir, Alternar, Alterar, Fechar, Escolher, Selecionar | |
| On, Off | Ligado, Desligado | |
| Delete; Keep; Folder; Password; Download | Excluir; Manter; Pasta; Senha; Baixar | |
| Hold; online; slot; rating; clock | segurar; online; slot; nota; frequência | |
| Clamshell | Clamshell | |
| Developer, Publisher (a game card's rows) | Estúdio, Editora | short: the longer words wrapped the values beside them at Large text |
| Look (the Settings category) vs Appearance (Dark, Light) | Visual vs Aparência | two words, as the English has two |

## Left in English

The same as in German, above: the manual's pages (the painter is switched off while they are drawn), data from
elsewhere, messages of other programs, names.

# French glossary

The words the French interface uses (`assets/i18n/fr.tsv`, language name "Français", code `fr`), translated from the English
source, not from German, Spanish or Portuguese. Every line of `de.tsv` gets its `fr.tsv` line before a release
(`i18n`'s release check `every_language_has_every_line`). Any `fr` locale (`fr_FR`, `fr_CA`, `fr_BE`, `fr`) starts it.

## Tone

- French of France, **tu** (never vous): the way a French retro-handheld player talks to a friend. Short, friendly, plain. No
  stiff passives, no "veuillez", no exclamation marks the English doesn't have. Gamer words people really use (`save state`,
  `cheats`, `patch`, `slot`, `online`, `overlay`-style words only where there is no short French one) stay.
- Imperatives in tu form, **without s** for -er verbs: "Appuie sur A", "Choisis ton réseau", "Réessaie", but "Choisis", "Finis".
  Press = **appuyer sur**.
- Short. French runs about 15-20 % longer than English and the pages were laid out for English: take the shorter of two good
  words, drop articles in labels and hints. Hint bar and pills: one or two words ("Retour", "Ouvrir", "Choisir").
- Typography: « guillemets » with a no-break space inside; a **no-break space (U+00A0)** before `:` `;` `!` `?` and `%`
  ("A : ouvrir.", "Prêt ?", "45 %") and before `Mo`/`Go`/`°C`/`W`/`ms`/`h` where the table writes a unit after a number. (U+202F, the
  narrow one, is missing from every font, so U+00A0 is the one space; the painter never breaks a line at it.) The ellipsis has no
  space before it ("Chargement…"). The apostrophe is the straight one `'` as in the English source ("l'app", "d'un").
  File sizes the code formats (`GB`, `MB`) stay as it formats them; a size written in a table line says "Mo" and "Go".
  Dates: "14 mars 2025"; months and weekdays lower case; short months jan, fév, mars, avr, mai, juin, juil, août, sept,
  oct, nov, déc (no dot: the dot made two neighbours touch in Retro) and short weekdays lun., mar., mer., jeu., ven., sam., dim..
  Time stays 24-hour. Numbers as the code formats them (decimal point), a decimal comma only in sentences written out.
- Gender and number: write "un jeu", "une partie", "un état"; a hole `{s}` is a name, so keep articles that don't change with it
  ("Le jeu {s} ne démarre pas", not "{s} ne démarre pas" only when the English has no article).
- Accents on capitals: "État", "À propos", "Éteindre", "Écran".

## What stays as it is

pocket-ui, pocket-core, Curator, Lens, Echoes, Pixel Path, ROCKNIX, RetroArch, RetroAchievements, EmulationStation,
PortMaster, Moonlight, Sunshine, Syncthing, Tailscale, Bluetooth, Store, Wi-Fi; game, emulator and core names; button names as
printed (A, B, X, Y, L1, R1, L2, R2, SELECT, START, HOME, Vol+, Vol-); Run-ahead, Shader, Core, Extras, Sticker(s), Home, FPS, CPU,
"Pocket" ("le Pocket", masculine); Little, Big, Prime; "Français" as the language's own name.
Console names as the French say them: Super Nintendo / SNES, Mega Drive (not Genesis), Master System, PC Engine, Nintendo 64,
PlayStation, Game Boy Advance, Neo Geo Pocket Color, Game Gear, Dreamcast. Game names are never translated (a game's French
title belongs to the library's data, not to this table).

## Terms

| English | French | Note |
|---|---|---|
| Settings | Paramètres | a path: "Paramètres › Système › Batterie" |
| Settings' categories: Connections, Look, Screen & sound, Controls, Playing, Games, System, Help & tools | Connexions, Visuel, Écran et son, Commandes, En jeu, Jeux, Système, Aide et outils | |
| Look (Settings → Look → Layout), look | Visuel | "Choose a look" → "Choisis un visuel"; never "Aspect": that is the screen ratio's English word ("Format"); "Présentation" was too long on Pixel Path in Retro |
| Layout, Style (Settings → Look) | Disposition, Style | |
| Appearance: Dark, Light, Automatic | Apparence : Sombre, Clair, Automatique | |
| Motion: Full, Reduced, Off | Mouvement : Complet, Réduit, Désactivé | |
| Shelf, Split, Cabinet (the looks) | Étagère, Divisé, Vitrine | |
| Cocoa, Pea (Retro's colours) | Cacao, Pois | |
| Language | Langue | |
| Playing (the category, the game status) | En jeu | one word for both |
| Button layout | Disposition des boutons | |
| Continue, Resume | Reprendre | one word for both |
| Game card (a game's page) | fiche du jeu, fiche | never "carte" |
| SD card | carte SD | never just "carte" |
| Memory card | carte mémoire | |
| save, in-game save (what the game writes) | sauvegarde | |
| Save state | save state | feminine: "une save state", plural "save states" |
| saves and states | sauvegardes et save states | |
| Save / Load (pause menu) | Sauver / Charger | |
| Slot | Slot | |
| Before load, Earlier (pause menu pills) | Avant, Avant | "Earlier states": "États précédents" |
| Quit | Quitter | |
| Pause menu, Quick settings | Menu pause, Paramètres rapides | |
| Options (a game's); Picture, Shortcuts | Options ; Image, Raccourcis | |
| Over the game | Incrustation | what pocket-core draws over a game |
| Screen frame(s); frames | Cadre(s) d'écran ; cadres | |
| Runs on | Tourne sur | |
| Achievements | Succès | the service stays RetroAchievements |
| Smart shelf | Étagère smart | "intelligente" broke mid-word in narrow columns in Retro |
| Up next | Ensuite | |
| Versions, preferred | Versions, préférée | |
| Star / Unstar, starred | Mettre en favori / Retirer, en favori | hint bar: "Favori" / "Retirer" |
| Favourites | Favoris | |
| Pin / Unpin | Épingler / Détacher | |
| Recent games, Recently played | Récents, Joués récemment | |
| Home (the screen) | Home | a name, as in the manual; also on the hint bar |
| Game check | Vérification des jeux | |
| Controller | manette | |
| D-pad; directions | croix directionnelle ; Gauche/Droite, Haut/Bas | hint bar "Gauche/Droite" |
| Pair, pairing; Forget | associer, association ; oublier | |
| Remote access; File sharing; PC streaming | Accès à distance ; Partage de fichiers ; Streaming PC | |
| Web companion | Compagnon web | |
| Sleep; Power off; Restart; Power | Veille ; Éteindre ; Redémarrer ; Alimentation | |
| Battery; Haptics; Sounds | Batterie ; Vibrations ; Sons | |
| Performance & fan, Smooth fan | Performances et ventilateur, Ventilateur doux | the Brick shortens it to "Performances" |
| Balanced, Quiet, Cool | Équilibré, Silencieux, Frais | |
| Fast-forward, Rewind | Avance rapide, Rembobiner | |
| Cover art, box art | Jaquette | |
| Pictures; screenshot; title screen | Images ; capture d'écran ; écran-titre | |
| Cheats; Patch; Tour; Updates; Sync; Logs | Cheats ; Patch ; Visite ; Mises à jour ; Synchro ; Journaux | |
| History, Play history, Year in review, Month in review | Historique, Historique de jeu, Bilan de l'année, Bilan du mois | |
| Session, streak | Session, série | |
| Safe mode; Backup & reset | Mode sans échec ; Copie de secours et réinitialisation | |
| Manual; About; Tools | Manuel ; À propos ; Outils | |
| Back, Open, Switch, Change, Close, Choose, Select | Retour, Ouvrir, Basculer, Modifier, Fermer, Choisir, Sélectionner | |
| On, Off | Activé, Désactivé | |
| Delete; Keep; Folder; Password; Download | Supprimer ; Garder ; Dossier ; Mot de passe ; Télécharger | |
| Hold; online; rating; clock | maintenir ; en ligne ; note ; fréquence | |
| Clamshell | Clamshell | |
| Developer, Publisher (a game card's rows) | Studio, Éditeur | "Développeur" was too wide: the value column narrowed and broke "Intelligent Systems" in Retro |
| Kids mode | Mode enfant | |
| Moment (a saved resume point; "Where I quit") | instant ("Où j'ai arrêté") | "Quit keeps the moment" → "Quitter garde l'instant" |
| Cozy | Cosy | |

## Left in English

The same as in German, above: the manual's pages (the painter is switched off while they are drawn), data from
elsewhere, messages of other programs, names.

# Italian glossary

The words the Italian interface uses (`assets/i18n/it.tsv`, language name "Italiano", code `it`), translated from the English
source, not from German, Spanish or Portuguese. Every line of `de.tsv` gets its `it.tsv` line before a release
(`i18n`'s release check `every_language_has_every_line`). Any `it` locale (`it_IT`, `it-CH`, `it`) starts it.

## Tone

- **tu** (never Lei): the way an Italian retro-handheld player talks to a friend. Short, friendly, plain, natural gamer
  Italian. No stiff bureaucratic passives, no "per favore" unless the English says please, no exclamation marks the English
  doesn't have. Gamer words people really use (`save`, `slot`, `cheat`, `patch`, `online`, `overlay`, `backup`) stay.
- Imperatives in tu form: "Premi A", "Scegli la tua rete", "Riprova". Press = **premere**.
- Short. Italian runs about 20 % longer than English and the pages were laid out for English: take the shorter of two good
  words, drop articles in labels and hints, labels as imperatives ("Salva", not "Salvare"). Hint bar and pills: one or two words.
- Typography: “virgolette” like the English (no «caporali»), an ellipsis with no space ("Caricamento…"),
  "45%" with no space like the English, "45 °C", "12 ms"; numbers as the code formats them (point stays). Dates: "14 marzo 2025",
  short "14 mar 2025"; months and weekdays lower case inside a date and a capital when alone. Time stays 24-hour.
- Apostrophes and accents: à è é ì ò ù are normal text (the painter prewarms them); write "è", "perché", "più", "già", capitals
  with an accent as "È" (not "E'"). Elision as usual: "l'audio", "un'opzione", "dell'SD" (but "della scheda SD").
- Gender/number agreement must work with the placeholders: write the label so it doesn't depend on a `{n}` (e.g.
  "Giochi: {n}" instead of "{n} giochi trovati" when 1 would break); the table has separate lines for "1 game" / "{n} games".

## What stays as it is

pocket-ui, pocket-core, Curator, Lens, Echoes, Pixel Path, ROCKNIX, RetroArch, RetroAchievements, EmulationStation,
PortMaster, Moonlight, Sunshine, Syncthing, Tailscale, Bluetooth, Store, Wi-Fi; game, emulator and core names; button names as
printed (A, B, X, Y, L1, R1, L2, R2, SELECT, START, HOME, Vol+, Vol-); Run-ahead, Grinding, Shader, Core, Extras, Sticker(s),
Home, FPS, CPU, "Pocket" ("il Pocket", masculine); Little, Big, Prime; "Italiano" as the language's own name.
Console names as Italians say them: Mega Drive, Master System, Super Nintendo / SNES, Nintendo 64, PlayStation, Game Boy
Advance, Neo Geo Pocket Color, Game Gear. Game names are never translated (the Italian title only where the table's English
already lists one). Names kept in `keep.txt` come out unchanged.

## Terms

| English | Italian | Note |
|---|---|---|
| Settings | Impostazioni | a path: "Impostazioni › Sistema › Batteria" |
| Settings' categories: Connections, Look, Screen & sound, Controls, Playing, Games, System, Help & tools | Connessioni, Aspetto, Schermo e audio, Controlli, In gioco, Giochi, Sistema, Aiuto e strumenti | |
| Look (Settings → Look → Layout), look | Aspetto, "aspetto" | "Choose a look" → "Scegli un aspetto" |
| Appearance: Dark, Light, Automatic | Tema: Scuro, Chiaro, Automatico | two different words, as the English has two (Look vs Appearance) |
| Look & feel | Aspetto e stile | |
| Style (Retro, Barlow…), Colours, Cocoa, Pea | Stile, Colori, Cacao, Pisello | |
| Layout | Disposizione | |
| Shelf, Split, Cards, Cabinet (the looks) | Scaffale, Diviso, Schede, Vetrina | |
| Motion: Full, Reduced, Off | Movimento: Completo, Ridotto, Spento | masculine, agreeing with "Movimento" (the battery's "Full in {t}" stays "Piena tra") |
| Medium (Text size, Haptics strength: Bassa, Intermedia, Alta) | Intermedia | feminine like "Dimensione" and "Intensità"; "Media" is the table's word for storage media |
| Language | Lingua | |
| Button layout | Disposizione tasti | |
| Continue, Resume | Continua | one word for both |
| Game card (a game's page) | scheda del gioco, scheda | never "carta" |
| SD card | scheda SD | never just "scheda" for the medium |
| Memory card | Memory Card | |
| save, in-game save (what the game writes) | save, salvataggio del gioco | |
| Save state | save state | a moment saved by pocket-core or the emulator |
| saves and states | save e save state | |
| Save / Load (pause menu) | Salva / Carica | |
| Slot | Slot | |
| Before load, Earlier (pause menu pills) | Prima, Prima | "Earlier states": "Stati precedenti" |
| Quit | Esci | |
| Pause menu, Quick settings | Menu di pausa, Impostazioni rapide | |
| Options (a game's); Picture, Shortcuts | Opzioni; Immagine, Scorciatoie | |
| Over the game | Sul gioco | what pocket-core draws over a game |
| Screen frame(s); frames | Cornice(i) dello schermo; cornici | |
| Moment | Momento | an open case means a moment is waiting |
| Runs on | Gira su | |
| Achievements | Obiettivi | the service stays RetroAchievements |
| Smart shelf | Scaffale smart | "intelligente" broke a word at Large text in Retro |
| Up next | Prossimi | |
| Versions, preferred | Versioni, preferita | |
| Star / Unstar, starred | Preferisci / Togli, preferiti | a star is a favourite |
| Favourites | Preferiti | |
| Pin / Unpin | Fissa / Sblocca | |
| Recent games, Recently played | Recenti, Giocati di recente | |
| Home (the screen) | Home | a name, as in the manual; also on the hint bar |
| Game check | Controllo giochi | |
| Controller | controller | |
| Press (a button) | premere | |
| D-pad; directions | croce direzionale; Sinistra/Destra, Su/Giù; hint bar "Sin./Des." | |
| Pair, pairing; Forget | associare, associazione; dimenticare | |
| Remote access; File sharing; PC streaming | Accesso remoto; Condivisione file; Streaming dal PC | |
| Web companion | Companion web | |
| Sleep; Power off; Restart; Power | Standby; Spegni; Riavvia; Alimentazione | |
| Battery; Haptics; Sounds | Batteria; Vibrazione; Suoni | |
| Performance & fan, Smooth fan | Prestazioni e ventola, Ventola dolce | |
| Balanced, Quiet, Cool | Bilanciato, Silenziosa, Fresco | the fan choices agree with "Ventola" (feminine): Molto silenziosa, Silenziosa; "Cool" is shared with the screen colours |
| Fast-forward, Rewind | Avanti veloce, Riavvolgi | |
| Cover art, box art | Copertina | |
| Pictures; screenshot; title screen | Immagini; screenshot; schermata del titolo | |
| Cheats; Patch; Tour; Updates; Sync; Logs | Cheat; Patch; Tour; Aggiornamenti; Sincronizza; Log | |
| History, Play history, Year in review, Month in review | Cronologia, Cronologia di gioco, Il tuo anno, Il tuo mese | |
| Session, streak | Sessione, serie | |
| Safe mode; Backup & reset | Modalità sicura; Backup e ripristino | |
| Manual; About; Tools | Manuale; Info; Strumenti | |
| Back, Open, Switch, Change, Close, Choose, Select | Indietro, Apri, Cambia, Modifica, Chiudi, Scegli, Seleziona | |
| On, Off | Acceso, Spento | a switch row and in sentences ("Spento: solo dissolvenze"); never "Sì/No" for On/Off |
| Delete; Keep; Folder; Password; Download | Elimina; Tieni; Cartella; Password; Scarica | |
| Hold; online; slot; rating; clock | tieni premuto; online; slot; voto; frequenza | |
| Developer, Publisher (a game card's rows) | Studio, Editore | short: "Sviluppatore" broke a developer name beside it at Large text |

## Left in English

The same as in German, above: the manual's pages (the painter is switched off while they are drawn), data from
elsewhere, messages of other programs, names.
