# Save sync pause ownership: recovery decision

**Status:** the narrow startup retry and a manual per-row Resume on the Sync page are implemented and verified in host
tests. Automatic recovery of the ambiguous interruption case remains design work; no device test or ownership-protocol change.

This branch characterises two ways saves can remain paused and retries startup resumes that fail.
It does not infer ownership of an ambiguous pause.
The contract remains: pocket-ui resumes only pauses it owns; a folder paused manually in Syncthing
must remain paused. Installed devices' `sync/held` file remains a newline-separated list of folder IDs.

## Current effects and their interruption windows

`sync::apply_holds` serialises local callers, reads the current game wish and `held`, then builds
`hold_plan`. For a newly paused folder it performs these effects in this order:

1. Ask Syncthing to pause the folder.
2. After API success, update the in-memory config and append the ID to the local owned list.
3. After processing all folders, atomically write the owned list to `held`.

| Observed folder | Legacy `held` | Current recovery |
| --- | --- | --- |
| Active | Absent | Leave active |
| Paused | Contains ID | Resume when no game wants the hold |
| Paused | Absent | Preserve as a manual pause |
| Active | Contains ID | Remove the stale ownership entry |

The last write can fail after Syncthing has already paused a folder. A process death after API success
but before that write has the same outcome. On restart there is no ownership entry, so recovery
preserves the pause as manual. This is a real gap in ownership, not a parser or planning failure.

Before this fix, `release_leftover_holds` waited in five-second steps for the standalone emulator to exit and for
Syncthing/config discovery to succeed. Once it called `apply_holds`, it exited even if resuming failed.
The failed folder remained in `held`, but automatic startup recovery did not retry until another
startup (or another caller of `apply_holds`). The worker now exits only on success; a failed resume
keeps the same five-second wait and rediscovers the emulator, current config and service before retrying.

## Reproductions and safety controls

`src/sync_ownership_tests.rs` uses a child test process because the current wish and operation lock
are process globals. Every fixture file lives in a temporary directory. The fake service persists
its paused state separately from the ownership marker. No device paths or real Syncthing are used.

- `legacy_owned_hold_resumes_without_touching_a_manual_pause` checks the old `pocket-saves\n` format,
  an ordinary pause/resume cycle, and preservation of a manually paused states folder.
- `refused_ownership_write_reports_error_but_service_has_already_paused` makes the marker path a
  directory, so the write fails even with elevated filesystem permissions. It characterises the
  existing partial success without accepting it as the desired behaviour.
- `a_crash_after_service_pause_still_leaves_ownership_for_restart` kills only its own child after
  the fake API has committed the pause and before the call returns. Its desired recovery assertion
  fails today: the restarted planner has no ownership evidence.
- `startup_recovery_retries_a_refused_resume_without_another_restart` refuses one resume operation
  and would accept the next. Its desired outcome failed before the fix: only the first attempt was made. It now checks two
  attempts with a wait between them, ownership retained until success, and the manual pause untouched.

The crash regression remains an explicitly ignored known bug, not a silently weakened assertion.
Run the `sync::tests::ownership` filter with `--include-ignored --test-threads=1` to reproduce that
remaining failure. The failed-resume regression runs normally and passes with the retry fix. The
startup loop uses injected discovery/wait functions so tests exercise its real retry rule without
reading device files or waiting five seconds; production keeps its existing discovery and interval.

## Small compatible change implemented

The startup worker exits only after `apply_holds` succeeds;
on failure it retains the existing error log, waits five seconds, and rediscovers emulator/config/service
before the next attempt. It keeps consulting the current game wish on every attempt. This needs no new
files and retains ownership on each failure. It does **not** close the API-success-before-marker window.

## Why moving the marker is not a complete safe fix

Prewriting the existing `held` entry changes its meaning from ownership of a successful pause to
intent to pause. Consider a process death after that write but before its API call. A person then
pauses the still-active folder using Syncthing. On restart pocket-ui sees `paused + held` and resumes
that person's pause. The old ownership promise has been broken even though the file format parses.

Writing an additional intent journal can distinguish pending intent from confirmed ownership, but
the interval between external API commit and journal confirmation remains ambiguous: the service
exposes a paused boolean, not a compare-and-set ownership token. A timeout response can have the same
ambiguity as a killed client. An in-memory enum or write ordering alone cannot prove who owns it.

## Decision needed before fixing the interruption gap

Preserve the conservative policy by default: an ambiguous pause remains paused rather than risk
unpausing a manual hold. Exact automatic recovery requires an explicit ownership policy beyond the
current contract, or a service operation that stores an operation/owner token atomically with the
pause. A compatible additive journal could record uncertainty and expose it for manual recovery,
but that is a new on-disk protocol requiring its own migration and power-cut tests.

The independent retry fix is authorised and implemented. Do not ship a marker reorder as a claim of
full crash recovery. Keep the interruption regression explicit until a policy is chosen; retain
all legacy `held` readers and manual-pause behaviour.

## Manual Resume on the Sync page (built)

Settings › Connections › Sync shows a paused folder on its share row: the value reads **Paused**, and the line under the
list says who paused it as far as pocket-ui can tell. "Paused by pocket-ui" means every paused folder of the row is in
`held`; anything else reads "Paused, not by pocket-ui", which covers a pause made in Syncthing and the crash gap above
(the two can't be told apart). The poller reads `config.xml` and `held` every three seconds (`sync::held_now`); it never
resumes anything.

While Syncthing answers, **X Resume** resumes every paused folder of the selected row (`sync::resume`,
`sync::resume_share`). GameCube & Wii is one row with two folders, so it resumes both and says so ("X resumes both
folders", "Both folders resumed"). Resume reads `config.xml` again, runs under the same lock as `apply_holds`, and is
refused while a game runs (its holds are wanted, or a standalone emulator still runs). A resumed folder leaves `held`;
one Syncthing refuses stays paused, and in `held` if it was there, and the status line shows the error. Without an
answering Syncthing the row still shows **Paused** but offers no Resume. **A** on the row still shares or stops sharing,
as before. Snapshots: `sync:paused-ours`, `sync:paused-unknown`, `sync:resuming`, `sync:resume-failed`,
`sync:paused-offline`. The navigation explorer's `sync_a_pause_not_by_pocket_ui_…` test drives a made-up Syncthing
(`POCKETUI_SYNC_ROOT` under `POCKETUI_MOCK`, a stand-in `curl`).
