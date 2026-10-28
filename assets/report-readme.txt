pocket-ui problem report
========================

Made on {when} by pocket-ui {version}, with Settings > System > About > Report a problem.

Where to send it
----------------
Open a bug report on pocket-ui's GitHub page (Issues > New issue > Bug report), say what happened and what you
expected, and attach this zip file. Nothing was sent anywhere: the file stays on your SD card until you attach it.
All files are plain text, so you can look through them first.

What is inside
--------------
{files}

Not found on this Pocket
------------------------
{missing}

What was left out
-----------------
Before the files were packed, these were replaced by markers in brackets:

  Wi-Fi network names and passwords                       [wifi-name] [password]
  IP and MAC addresses                                    [ip] [mac]
  Tailscale names and keys                                [tailnet-name] [tailscale-key] [hostname]
  RetroAchievements user name and token                   [user] [token]
  Syncthing device IDs                                    [sync-device-id]
  Passwords, tokens and cookies (web companion, sharing)  [password] [removed]
  User names in folder paths, card names, e-mail addresses [user] [card] [email]
  Bluetooth devices named after a person ("Alex's AirPods") [bluetooth-name]
  Game names                                              [game <system> <code>]

A game's name is replaced by its system and a short code, for example [game snes 1a2b3c4d]. The same game always
gets the same code, so the logs can still be followed. If a game matters for the problem, name it in your report.

The list of your games, your play history, favorites, notes and saves are not in this file.
