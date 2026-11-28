# Android extras (opt-in levels beyond a normal app)

Settings › System › **Android extras** in Android's handheld view (user side: [docs/user/android-mvp.md](user/android-mvp.md#android-extras)).
The goal (26.11.2026): the Android app should feel as native as pocket-ui OS. Standard stays without any of this;
every level is the player's choice, shown as an extra. The app is not for the Play Store, so its rules don't apply
(WRITE_SECURE_SETTINGS, an accessibility service for keys, Shizuku).

## Levels

| Level | Given by | Can |
|---|---|---|
| Standard | nothing | a normal app |
| ADB grant | `adb shell pm grant <package> android.permission.WRITE_SECURE_SETTINGS`, once (stays until revoked or uninstalled), or A on its row with Shizuku or root (A there also revokes, after putting the switches back) | `Settings.Global`: the switches |
| Shizuku | [Shizuku](https://shizuku.rikka.app/) started by the player (Wireless debugging or root), pocket-ui allowed in its dialog | Android's shell (uid 2000): a key press for the emulator's window only (`TargetKey`: bound by Android on 14+, focus-checked on 13), `logcat` of every process, and `pm grant/revoke --user <n>` when the player presses A on the grant's row |
| Root | Magisk's `su` (`Extras.checkRoot`, asked only on A) | the shell's part via `su -c` (the same bound key press: `TargetKey` runs as root), and the scaffold in `src/android_extras/root.rs` |

`Status::level` picks the highest. The page reads `Extras.status` (JSON) on a thread when it opens, after every press and
when the view comes back to the front.

## Pieces

| Where | What |
|---|---|
| `src/android_extras.rs` | the page's state, rows, words, presses (`Page::press` → `Action::Bridge(what, arg)`), the overlay's English lines (`OVERLAY_LINES`, written translated to `files/extras/texts.tsv` for Java) |
| `src/android_extras/root.rs` | root scaffold: cpufreq/devfreq discovery script and parser, per-game `Profile` writes and restore, a fan's `hwmon/pwm1`, the RG556's `bootloader_control` block in `misc` |
| `src/app_android_extras.rs` | the page in the binary (paint, thread, snapshot `android-extras[:plain|adb|shizuku|root|old|wait][:<row>]`) |
| `android/handheld/app_android.rs` | `extras_status`, `extras_action`; the overlay lines written at every start in another app; `android_extras_front` on resume |
| `Extras.java` | status JSON, actions, the switches with their old values in `files/extras/before.properties`, the grant through Shizuku/root on A (`--user`), Android's accessibility page for the service, `su`, the shell (Shizuku or root), the whole log for `ReportSupport` |
| `ShizukuLink.java`, `ExtrasProvider.java` | Shizuku's protocol (API 13): binder via the provider `<package>.shizuku` in the `:extras` process, `attachApplication`, permission, `newProcess` |
| `android/aidl/moe/shizuku/server/*.aidl`, `android/java/moe/shizuku/**` | Shizuku-API's binder interfaces (made by `scripts/android/shizuku-aidl.sh`) and `BinderContainer`; MIT (`android/licences/SHIZUKU-API.txt`; the brief said Apache-2.0, the repository says MIT) |
| `PadWatch.java`, `PadChord.java`, `PadMenu.java`, `EmulatorKeys.java`, `PressGate.java` | the accessibility service (key filter + window changes), HOME / SELECT+START, the overlay menu (taken focus, delayed presses only into the app they were meant for), F2/F4 per app and `files/extras/keys.txt`, each emulator's state folders and file names (`stateDirs`, `statePattern`), the result line over the game (`say`) |
| `TargetKey.java` | the key press: run by Shizuku or `su` as `app_process -Djava.class.path=<our APK> /system/bin xyz.pocketui.app.TargetKey <nonce> <uid> <code> <deadline> <wait> <pattern> <dir>…` (no hidden-API rules outside an app). 14+: `IInputManager.injectInputEventToTarget(event, WAIT_FOR_RESULT, uid)`, a mismatch throws and nothing is delivered. 13: the input dispatcher's focused window (its dump through a pipe, `focusedUid`) must belong to `uid` right before the down and again right after it. Deadline in uptime ms, a life limit of its own, only F1 to F12 and app uids; after a save a per-path snapshot of the state folders; one status line `pocket-targetkey <nonce> <status>` that only counts with exit 0 (`outcome`) |
| `scripts/android/test/ExtrasKeysTest.java` | JVM test of the three pure classes, run by `java-reader-rule.sh` |

The Shizuku client is written out by hand instead of the AAR: the build has no Gradle (javac, d8, aapt). It uses only
the generated stubs and the documented attach (transaction 18 with the application binder and a bundle of package and API
version). Shizuku's server sends the binder when one of our activities comes to the foreground; it reaches the `:extras`
process through the provider. If that process was killed later, the binder comes back with the next foreground change.

## What depends on the firmware or the emulator (honest list)

| Thing | Status | Why |
|---|---|---|
| Hide system bars | **Android 10 and older only** | `policy_control` was removed in Android 11; there is no global immersive setting since. On 11+ the row is not shown. Our own windows hide the bars anyway (PocketBase/PlayerActivity `immersive()`); other emulators decide for themselves. |
| No pop-up notifications | AOSP `heads_up_notifications_enabled` | OEM builds may ignore it; untested on the Mini V2 and RG556. |
| No animations | the three `*_animation_scale` | AOSP; works with the grant on Android 13–15 (common in other apps). |
| Stay awake while charging | `stay_on_while_plugged_in = 7` | AOSP developer option. |
| HOME in the key filter | **firmware-dependent** | AOSP passes HOME through the accessibility key filter before the policy's HOME handling (that is how button-remapper apps work), but OEM builds (RG Home, Retroid's MODE key) may handle their keys earlier. SELECT + START is the fallback. Back gestures are no key events. |
| SELECT before START | by design | a key filter can't take back a press it already passed: the emulator sees a short SELECT. |
| Overlay focus | taken once no game key is held | the overlay is `TYPE_ACCESSIBILITY_OVERLAY` and takes the focus: built-in pads whose D-pad is a hat send motion events, which never pass a key filter (emulator test 26.11.2026, uinput pad). It waits until every key the emulator saw go down is released there (SELECT of SELECT + START), plus 120 ms. The emulator loses the focus meanwhile; whether it pauses then depends on the app. |
| Save/load keys | **Android 13+, only RetroArch by default** | 14+: bound to the emulator's uid by Android (`injectInputEventToTarget`). 13: the same call falls back to every window on a mismatch for callers that may inject (shell, root; AOSP android13-release b/228161340, seen on the RG556), so TargetKey checks the dispatcher's focus before and after the down instead (rest risk below). 12 and older: no Save/Load/Teach rows (`PadWatch.canSave`, `Status::can_save`). RetroArch: F2/F4 are its defaults. PPSSPP has no default save key (`Core/KeyMapDefaults.cpp`); Dolphin, Azahar, ARMSX2/NetherSX2, DuckStation, DraStic need their hotkey mapped once (the menu's Teach rows). hakuX: unknown, and its state folder too (never "Saved"). PPSSPP checked in the emulator (Android 15) and TargetKey on the RG556 (Android 13); the others not yet. |
| "Saved" | **the apps' default state folders** | `EmulatorKeys.stateDirs`: PPSSPP `PSP/PPSSPP_STATE` (shared or app-specific memory stick), Dolphin `StateSaves`, ARMSX2 family `sstates`, Azahar/Lime3DS `states`, DuckStation `savestates`, DraStic `DraStic/savestates`, RetroArch `RetroArch/states`. A folder the player picked elsewhere (Azahar and Lime3DS ask for one) is not found: the line says "Save key sent". The shell and root can read `Android/data`. |
| Save and quit | **left out** | Sol's review 26.11.2026: a pressed key says nothing about whether the emulator saved, so ending the app after it could lose the game. The emulator's own menu ends a game. |
| Restricted settings | Android 13+ | a sideloaded app's accessibility service is greyed out until "Allow restricted settings" in its app info; the ADB grant avoids that page entirely. Android 15 may use Enhanced Confirmation (CDD §9.8). |
| Shizuku after reboot | stops | non-root Shizuku needs starting again after every boot (Wireless debugging); with root it can start by itself. A grant given through it stays until taken back. |
| Logs | Shizuku or root | `READ_LOGS` is not used: Android 13+ asks the player at every read. The report keeps only our processes and the emulator apps' (from "Start proc <pid>:<package>/" lines). |
| Root clocks/fan/misc | **scaffold, not on the page** | scripts and parsing are tested (`android_extras::root`); real sysfs nodes, permissions under SELinux and the RG556 block need a device. Most Android handhelds drive their fan in a vendor service (no `hwmon` fan node). |

## Sol's review of 26.11.2026 (QA APK a2002b35d)

Done here: no Save and quit; the menu closes when another app or the shelf comes in front, and a delayed key press only
lands in the app it was meant for (menu session + app in front); the release of a key the emulator saw go down passes
through while the menu is open (SELECT of SELECT + START), per input device; `pm` with `--user`; no grant behind the
player's back (Shizuku/root give and take it only on A on its row) and the accessibility service only on Android's page.
Second round (review of d69eadf3e): a delayed press goes out only if no window at all came in front since (any app, our
shelf, Android's panels; `PressGate`, also A→B→A) and the game we started is still the one in front; the menu takes the
focus only after SELECT's release reached the emulator. Shown in the emulator with a uinput pad: SELECT held 0.9 s into the
menu, PPSSPP got its release, then the D-pad moved the menu; Teach then the shelf in front, or Settings and back, within
1.5 s: "F2 not pressed"; Teach left alone: F2 reached PPSSPP.
Third round (review of 6aaab4b7c): the gate was checked before the shell started, but `input keyevent` ran globally a
moment later, so a window could still take the focus in between (also our shelf: a save lost). Now every press goes
through `TargetKey`: Android's input dispatcher checks at the moment it hands out the key that the focused window belongs
to the emulator's uid, else it drops the key (`Dropping injected event`) and the press counts as missed; the menu then says
"Not saved". PressGate stays in front of it (no process started for a press that is already stale). Root uses the same
program under `su`, so it binds the same way. Fourth round (review of 82c82f0ba, and the RG556): Android 13's
InputManagerService answers a mismatch by injecting again into every window when the caller may inject (shell, root;
14 and 15 throw instead). The lead's decision (27.11.2026): keep Android 13, the main target handhelds (RG556, Mini V2), with a focus check:
TargetKey reads the input dispatcher's own focused window (the `input` service's dump through a pipe, not `dumpsys`
parsing of the window manager) and sends only when it belongs to the emulator's uid, then looks again right after the down
("unsure" → "Not sent safely" when it moved). **Rest risk on Android 13:** a window that takes the focus in the
milliseconds between that first look and the down gets the key (one of F1 to F12, nothing else). pocket-ui drops such a key on
these paths: the player's `dispatchKeyEvent` and the views' `PocketBase.dispatchKeyEvent` (`EmulatorKeys.strayEmulatorKey`,
F1 to F12 from Android's virtual keyboard), and first in both native views' key arms (`emulator_f_key`,
before the Mobile view's "any key" steps). The native filter can't tell where a key came from, so a real keyboard's F1 to F12 also do
nothing in the native views (they had no action there before either, apart from the "any key" steps); the player's Java
filter lets a real keyboard's F-keys through to computer cores. Not covered: our dialogs (AlertDialog windows of their own); none of them acts
on F1 to F12 today.
Fifth round (Sol's recheck of 058919942, lead 27.11.2026: Android 13 stays best effort): the dump is read strictly
(`focusedUid`): the focused window's name must hold no quote, comma, `=` or line break (a title is set by the app and printed
unescaped before the real fields), exactly one window line of that name, its owner from the fixed field run after the name,
which must occur once; anything else gives no uid and no press. Known and accepted on 13: the focus going A → B → A between
the two looks is not seen (a JVM test pins it). The deadline is checked after the focus look, right before the down; a
press belongs to its menu action and game start (`action`, `Extras.playingSince`) and its program is ended at once
(`Extras.cancelPress`) when a new menu or window comes; the busy slot frees itself by time if a Shizuku call hangs.
"Saved" means a state file seen new or changed and unchanged over three complete scans in a row; an incomplete scan (an
unreadable folder, the budget) starts over, an unreadable folder is not taken for a missing one. It is a file seen, not
proof of the game or slot; the line says so in the manual.
Sixth round (Sol's recheck of f7df20bc8, lead: specially made multi-line window titles of another app stay part of the
Android 13 rest risk; the parser is not built on further): a dump cut short (read error, size limit) is no dump (`readWhole`, tested). Each press
has a token file in our own shared folder (`Extras.newPressToken`, `getExternalFilesDir/press/<nonce>`, readable by the
shell and root); a new menu, window or game start deletes it (`stale`, `Extras.gameStarted`/`gameEnded`), also before the
program runs, and TargetKey presses only while it exists, looked at right before the down. The press checks its menu
action, game start and gate again after making the token. On the accessibility thread only the token goes; ending the
process (`Extras.cancelPress`) runs on its own thread, and process handle and busy slot belong to one press each (only
their owner clears them).
Seventh round (Sol's recheck of 5a47a70b7): a press never removes another press's token; processes are kept per token
(a late one can't displace another's); a press that takes over from one whose time ran out removes the old token first.
`gameStarted` runs before the emulator's activity starts, writes a new launch id into the playing note and only then
removes the tokens, and the press compares that id (not a file time). The clock is read once more after the token look.
Eighth round (Sol's recheck of 38cfee80f): `LaunchNote` writes a lock before the new launch id and removes it after; a
launch id that could not be written leaves the lock, and while it is there the menu offers no Save/Load and a press made
before is refused (no lock possible: the old note goes first; neither possible, a read-only folder: the lock goes into the
shared press folder; that too impossible: the game is not started, `LaunchNote.Result.FAILED`). `PressSlot` hands the one press slot over by time and ends
exactly the press it took over (its token, now or when it makes one), never a newer one. Both are JVM-tested. On the page,
"ADB access is already granted." shows beside the Save/Load line on Android 13+ too. Also from that review: the
target must be one app with its own uid in our user (not ours, no system or shared uid), one press at a time, a deadline in
uptime ms checked in the program right before the down, a life limit for the program, the Shizuku process ended on every
path, results only for the newest menu action, and "Saved" only from a complete per-path snapshot with a file that is new
or changed and stopped growing; the status line carries a nonce and counts only with exit 0.
"Saved" only with a new state file, else "Save key sent".
Left (P2): a lifecycle for the root and playing markers, a journal for failed `putString` restores, the report's log filter.

## Checked in the Air emulator (26.11.2026, Android 15, QA package, PPSSPP 1.19.3 from F-Droid, Shizuku 13.6.0)

- The page: levels Standard → ADB grant (real `pm grant`) → Shizuku (Shizuku's server started over adb, its binder reached
  `ExtrasProvider`, attach, Shizuku's own dialog allowed with the pad, the grant given by Shizuku); Root check without Magisk
  says Not found.
- The switches write 0/0/0/0/7 and put back exactly what was there (1, 1.0, 1.0, unset → 1, 1).
- The pause menu's service turned on by the grant; a uinput pad (scripts/android/test/vpad.c, vendor without a key layout):
  HOME/MODE and SELECT + START open the menu over PPSSPP, the hat D-pad moves, A on Save state presses F2 in PPSSPP through
  Shizuku (PPSSPP logged the key). Injected keys (`input keyevent`) never reach a key filter: test with a real or uinput pad.
- Bound presses (26.11.2026 evening, QA APK of feat/android-extras-target, Psp2048 homebrew in PPSSPP's memory stick):
  Teach the save key with PPSSPP's mapping waiting → PPSSPP mapped `kbd.F2`; the same for load (`kbd.F4`). Save state →
  `result=saved PSP200881__0.ppst`, the file appeared, the line said Saved. Moves, then Load state → the board went back to
  the saved one ("Load key sent"). Race: Save state, then Settings or our own shelf (HandheldActivity) brought in front
  0.15 to 0.75 s later: either PressGate ("F2 not pressed") or, after its check, the dispatcher (`Dropping injected event:
  … would be dispatched to window … HandheldActivity`), each time "Not saved", the state file unchanged, no key in PPSSPP.
  `TargetKey` by hand as shell (uid 2000) and as root (`su 0`): Settings in front → dropped, PPSSPP in front → its key
  arrived. Android 15 reports the mismatch as `IllegalArgumentException` (13 and 14: false); both count as missed.
- Seen once: right after the uinput pad was made anew, the first two D-pad steps after HOME reached PPSSPP's mapping
  dialog (it mapped the hat) before the menu had the focus; measured otherwise, the menu has the focus 140 ms after HOME.
- RG556 (Android 13, 27.11.2026), first try with the plain targeted call: with the notification shade in front it answered
  `sent`; system_server logged "Targeted input event was not directed at a window owned by uid … Falling back to injecting
  into all windows" and the key (F12) went to the shade. Hence the focus check on 13 (above).
- RG556 again with the focus check, `TargetKey` by hand as shell, F12 into PPSSPP: PPSSPP in front → `sent`; deadline passed
  → `late`; Settings in front → `missed`, nothing injected; Settings started 0 to 0.4 s after the program: `missed` (0, 50,
  100 ms), `sent up-skipped` (150, 200 ms: the down reached PPSSPP, the focus left before the up), `sent` (300, 400 ms); no
  "Falling back" line in any run.
- Not seen yet: a real handheld's HOME key, RetroArch, Magisk's `su` (the emulator's `su 0` only), a device with Android 14, the
  whole menu flow on the RG556 (only `TargetKey` by hand there).

## Not done yet (needs a device or a decision)

- Device runs: the Air emulator covers the page, the grant, the switches and the service; the Mini V2 and RG556 (HOME
  in the filter, real emulators, Shizuku over Wireless debugging) need the lead's go.
- Root: the page rows for clocks per game and the fan (UI on the game card or pause menu), and the RG556's "Restart in
  pocket-ui OS" (needs the Linux side of DUAL-BOOT.md first; the block code refuses anything without the magic and a
  matching CRC, writes, reads back).
- QA packages: the Shizuku provider's authority is `<package>.shizuku`; a QA package must rewrite it like the `.roms` one
  (`pack.py`), or it conflicts with an installed release that has it.
