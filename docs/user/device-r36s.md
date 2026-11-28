# R36S and R36XX

Two ways to play: the R36S's **own pocket-ui card**, which plays the games on your original card, or the
**pocket-ui beta for dArkOS**, started from Ports on the dArkOS card you already use.

- **pocket-ui as:** its own SD card image, or the pocket-ui beta for dArkOS
- **Status:** both Beta; the card is for genuine R36S handhelds, and the R36XX uses the same card. Other clones aren't
  covered yet
- **Get it:** [the R36S card](https://pocketui.xyz/install.html#r36s) ·
  [pocket-ui beta for dArkOS](https://pocketui.xyz/install.html#darkos)
- **Card or pocket-ui beta for dArkOS?** Side by side: [Which one is for you?](coming-from.md#which-one-is-for-you)

## The R36S card

- **Two slots, nothing copied:** the pocket-ui card goes in **TF1**, your original card (or your dArkOS card) in
  **TF2**. Its games, BIOS files and box art show up as they are, and the only files that change there are the saves your
  games write ([Coming from dArkOS](coming-from.md#coming-from-darkos-or-an-r36s-stock-card)).
- **First start:** up to 10 minutes with one or two restarts, so don't press anything.
- **The FN key** opens the pause menu.
- **No Wi-Fi needed:** add games on a computer, update with one zip on the games card
  ([No Wi-Fi? Update from the card](updates.md#no-wi-fi-update-from-the-card)), box art and PortMaster games from a
  computer too ([Without Wi-Fi](extras.md#without-wi-fi), [PortMaster games without Wi-Fi](extras.md#portmaster-games-without-wi-fi)).
- **Black screen after 10 minutes?** Newer screens need one extra file, which pocket-ui takes from your original card in
  TF2. Without it, the table of R36S boards (R36XX V21 included) names the file to try first:
  [R36S screens](other-handhelds.md#r36s-beta).
- **Plays:** Game Boy, Super Nintendo, Nintendo DS and PlayStation ran at full speed in our tests; Continue is back in
  about 2 seconds. N64 takes up to a minute to start, and how well it plays varies by game.
- **Battery always at 0% (or another wrong level)?** Settings → System → Battery, X: **Reset reading**
  ([Battery](settings-system.md#battery)). The R36S switches off; switch it on and charge it until it is full once.
- **Back to stock:** power off and put your original card back in TF1.

## pocket-ui beta for dArkOS

- **Install:** unzip the download into the `ports` folder of your games card (EASYROMS, or your second card), run
  Start → Game settings → Update gamelists in dArkOS, then launch Ports → pocket-ui. dArkOS and your games stay untouched.
- **Where it runs:** 640×480 dArkOS handhelds, including the R36S.
- **What dArkOS keeps:** Bluetooth, Remote access (SSH) and Time are set in dArkOS itself, and Wi-Fi shows dArkOS's
  connection; Tailscale, Sync, RomM and Screen frames aren't in this beta ([Settings](settings.md)). RetroArch keeps its
  own keys.
- **Back to dArkOS's menu:** **Back to EmulationStation** in Quick settings or Settings → System
  ([Quick settings](quick-settings.md)).
- **Updates:** Settings → System → **Updates** over Wi-Fi, or the dArkOS update zip on EASYROMS
  ([Updates](updates.md#no-wi-fi-update-from-the-card)).
- **Uninstall:** delete `pocket-ui.sh` and the `pocket-ui` folder.
- **Problems?** pocket-ui isn't made by the dArkOS team: send reports to us, with the logs in `ports/pocket-ui/logs`.

## Known limits

- The card doesn't cover other R36S clones yet. For another board or a clone, your original card in TF2 is the safe way to a
  picture.
- The screen file in the table is the closest one we found, not a promise.
- The 720×720 models (R36S Plus, Max, Ultra) aren't covered by pocket-ui beta for dArkOS ([dArkOS notes](../DARKOS.md)).

## More

[Coming from dArkOS or an R36S stock card](coming-from.md#coming-from-darkos-or-an-r36s-stock-card) ·
[Report a problem](troubleshooting.md#report-a-problem) · reports welcome in #testers on
[Discord](https://discord.gg/sDqUV4M49g)
