// pocket-ui web companion: saves and Pocket Sync (src/web/saves.rs, src/web/pockets.rs). Loaded before app.js and uses
// its helpers (h, t, api, view, toast, size, fileRow, uploadPieces, dropZone) when it runs, never while it loads.
"use strict";

// "3 min ago", in the Pocket's language. `now` is the Pocket's clock, so a phone set wrong doesn't matter.
function agoOn(at, now) {
  const d = Math.max(0, now - at);
  if (d < 60) return t("just now");
  if (d < 3600) return t("{n} min ago", { n: Math.floor(d / 60) });
  if (d < 86400) return t("{n} h ago", { n: Math.floor(d / 3600) });
  if (d < 2 * 86400) return t("yesterday");
  return t("{n} days ago", { n: Math.floor(d / 86400) });
}

function whenText(at) {
  if (!at) return "";
  const lang = document.documentElement.lang || undefined;
  return new Date(at * 1000).toLocaleString(lang, { day: "numeric", month: "short", year: "numeric", hour: "2-digit", minute: "2-digit" });
}

const SAVE_KINDS = { save: () => t("Game save"), clock: () => t("Cartridge clock"), card: () => t("Memory card") };

function stateLabel(s) {
  if (s.slot === "continue") return t("Continue");
  if (s.slot === "undo") return t("Before load");
  if (s.slot === "slot") return t("Slot {n}", { n: s.n });
  return t("Other state");
}

function fileLink(id, name) {
  return "/api/saves/file?" + new URLSearchParams({ id, name });
}

// --- on a game's page ----------------------------------------------------------------------------------------------

// The game's saves and states, with a download for each and a drop zone to put one back. Fills `box` when it has them.
async function savesBlock(id, box) {
  let s;
  try {
    s = await api("/api/saves?id=" + encodeURIComponent(id));
  } catch (e) {
    fill(box, h("h3", {}, t("Saves and states")), h("p", { class: "alert small" }, e.message));
    return;
  }
  const row = (label, f) => h("li", {}, h("div", { class: "save-row" },
    h("span", { class: "name" }, label, " ", h("span", { class: "quiet small" }, f.name)),
    h("span", { class: "side quiet small" }, whenText(f.modified), " · ", size(f.bytes)),
    h("a", { class: "button dl-btn", href: fileLink(id, f.name), download: f.name }, t("Download file"))));
  const lists = [];
  if (s.saves.length) lists.push(h("ul", { class: "saves" }, s.saves.map((f) => row((SAVE_KINDS[f.kind] || SAVE_KINDS.save)(), f))));
  if (s.states.length) lists.push(h("ul", { class: "saves" }, s.states.map((f) => row(stateLabel(f), f))));
  const none = !s.saves.length && !s.states.length;
  const empty = none ? h("p", { class: "quiet" }, s.folder ? t("This game keeps its saves in its own folder. Its save states show here once there are some.") : t("No saves of this game on the Pocket yet.")) : null;
  const all = none ? null : h("p", {}, h("a", { class: "button", href: "/api/saves/zip?id=" + encodeURIComponent(id) }, t("All of this game as a ZIP")));

  const rows = h("div", {});
  const put = dropZone(t("Drop a save or a save state here"), false, async (files) => {
    for (const file of files) {
      const r = fileRow(file.name, size(file.size));
      rows.append(r.el);
      if (await putBack(id, file, r)) savesBlock(id, box);
    }
  });
  fill(box,
    h("h3", {}, t("Saves and states")),
    empty, lists, all,
    h("details", { class: "put-back" },
      h("summary", {}, t("Put a save back")),
      h("p", { class: "quiet small" }, t("Saves end in .srm or .sav, save states in .pstate or .state. The Pocket names the file after this game, so a save from another device or with another name fits.")),
      h("p", { class: "quiet small" }, t("Saves work on other devices too. Save states only work with the same emulator.")),
      h("p", { class: "quiet small" }, t("Only while no game runs. What it replaces goes into a backup first: Settings › System › Backup & reset.")),
      put, rows));
}

// Checks, asks before replacing, then sends one file. True when it went in.
async function putBack(id, file, r) {
  const q = { id, name: file.name };
  r.state.textContent = t("Checking…");
  let check;
  try {
    const res = await fetch("/api/saves/check?" + new URLSearchParams(Object.assign({ size: file.size }, q)), { method: "POST", credentials: "same-origin", headers: { "X-CSRF-Token": S.csrf || "" } });
    check = await res.json();
  } catch (e) {
    check = { ok: false };
  }
  const fail = (msg) => { r.state.className = "alert small"; r.state.textContent = msg; r.bar.remove(); return false; };
  if (!check.ok) return fail(check.error || t("The Pocket did not answer. Nothing was changed."));
  let replace = 0;
  if (check.replaces) {
    const was = check.replaces;
    if (!confirm(t("This replaces {s} from {s2}. The one there now goes into a backup first.", { s: was.name, s2: whenText(was.modified) }))) {
      r.state.textContent = t("Kept the one on the Pocket");
      r.bar.remove();
      return false;
    }
    replace = 1;
  }
  r.state.textContent = t("Uploading…");
  // In pieces as game files go: Chrome on Android breaks off a large state sent in one request.
  const res = await uploadPieces(Object.assign({ replace }, q), file, r, "/api/saves/upload/start");
  if (!res.ok) return fail(res.body.error || t("Not put back ({n}).", { n: res.status }));
  r.state.className = "ok small";
  r.state.textContent = res.body.replaced ? t("Put back as {s}. The old one is in the backup.", { s: res.body.name }) : t("Put back as {s}.", { s: res.body.name });
  r.bar.value = 1;
  return true;
}

// --- the Saves tab -------------------------------------------------------------------------------------------------

function pocketLine(p, now) {
  switch (p.last) {
    case "synced": return p.okAt ? t("Synced") + " · " + agoOn(p.okAt, now) : t("Synced");
    case "asked": return t("Two saves: choose");
    case "failed": return p.failed === 1 ? t("1 save did not sync") : t("{n} saves did not sync", { n: p.failed });
    case "busy": return t("Playing a game");
    case "unpaired": return t("Pair again");
    case "bad": return t("Did not answer right");
    case "unreachable": {
      const since = p.downSince || now;
      const days = Math.floor(now / 86400) - Math.floor(since / 86400);
      if (days <= 0) return t("Not reachable since {s}", { s: new Date(since * 1000).toLocaleTimeString(document.documentElement.lang || undefined, { hour: "2-digit", minute: "2-digit" }) });
      if (days === 1) return t("Not reachable since yesterday");
      return t("Not reachable for {n} days", { n: days });
    }
    default: return t("Not synced yet");
  }
}

function gameLink(g) {
  const name = g.game || g.file;
  return g.id ? h("a", { href: "#/game/" + g.id }, name) : h("span", {}, name);
}

function pocketsPanel(p) {
  const kids = [h("h3", {}, t("Pocket Sync"))];
  if (!p.pockets.length) {
    kids.push(h("p", { class: "quiet" }, p.enabled
      ? t("No other Pocket is paired yet. Pair them on the Pockets themselves: Settings › Connections › Pocket Sync › Pair a Pocket.")
      : t("Pocket Sync is off. It keeps the saves of your Pockets in step. Turn it on at the Pocket: Settings › Connections › Pocket Sync.")));
    return h("div", { class: "panel" }, kids);
  }
  if (!p.enabled) kids.push(h("p", { class: "alert small" }, t("Pocket Sync is off on this Pocket. This is how things stood when it was last on.")));
  kids.push(h("ul", { class: "saves" }, p.pockets.map((x) => h("li", {}, h("div", { class: "save-row" },
    h("span", { class: "name" }, x.name), h("span", { class: "side small " + (["synced"].includes(x.last) ? "ok" : ["failed", "unreachable", "unpaired", "bad"].includes(x.last) ? "alert" : "quiet") }, pocketLine(x, p.now)))))));
  if (p.questions.length) {
    kids.push(h("h3", {}, t("Two versions")),
      h("p", { class: "quiet small" }, t("Played on two Pockets before they synced. Choose the one to keep on the Pocket: open the game there.")),
      h("ul", { class: "saves" }, p.questions.map((q) => h("li", {}, h("div", { class: "save-row" }, h("span", { class: "name" }, gameLink(q)), h("span", { class: "side quiet small" }, q.file))))));
  }
  kids.push(h("h3", {}, t("Last saves")));
  kids.push(p.recent.length
    ? h("ul", { class: "saves" }, p.recent.map((e) => h("li", {}, h("div", { class: "save-row" },
      h("span", { class: "name" }, gameLink(e)),
      h("span", { class: "side quiet small" }, (e.dir === "from" ? (e.pocket ? t("Came from {s}", { s: e.pocket }) : t("Came from a Pocket no longer paired")) : (e.pocket ? t("Went to {s}", { s: e.pocket }) : t("Went to a Pocket no longer paired"))) + " · " + agoOn(e.at, p.now))))))
    : h("p", { class: "quiet" }, t("No saves have gone between your Pockets yet.")));
  kids.push(h("p", { class: "quiet small" }, t("Pairing and choosing happen on the Pockets, never in the browser.")));
  return h("div", { class: "panel" }, kids);
}

async function savesView() {
  const [all, pockets] = await Promise.all([api("/api/saves/all"), api("/api/pockets")]);
  const backup = [];
  if (!all.saves.count && !all.states.count) {
    backup.push(h("p", { class: "quiet" }, t("No saves on the Pocket yet. They show up here once you have played.")));
  } else {
    backup.push(h("p", {}, all.saves.count === 1 ? t("1 save · {s}", { s: size(all.saves.bytes) }) : t("{n} saves · {s}", { n: all.saves.count, s: size(all.saves.bytes) }),
      all.states.count ? [" · ", all.states.count === 1 ? t("1 save state · {s}", { s: size(all.states.bytes) }) : t("{n} save states · {s}", { n: all.states.count, s: size(all.states.bytes) })] : null));
    const links = h("div", { class: "row" });
    if (all.saves.count && all.saves.fits) links.append(h("a", { class: "button primary", href: "/api/saves/zip" }, t("Download all saves (ZIP)")));
    if (all.states.count && all.states.fits) links.append(h("a", { class: "button", href: "/api/saves/zip?states=1" }, t("Saves and states (ZIP)")));
    backup.push(links);
    if (!all.saves.fits || (all.states.count && !all.states.fits)) backup.push(h("p", { class: "alert small" }, t("That is more than fits in one ZIP. Download each game's saves from its page.")));
    backup.push(h("p", { class: "quiet small" }, t("Saves work on other devices too. Save states only work with the same emulator.")));
  }
  backup.push(h("p", { class: "quiet small" }, t("To put a save back, open its game and drop the file on the game's page.")));
  if (all.backups) backup.push(h("p", { class: "quiet small" }, t("Saves replaced from here are kept on the Pocket: Settings › System › Backup & reset.")));
  view(h("h2", {}, t("Saves")), h("div", { class: "panel" }, h("h3", {}, t("Back up your saves")), backup), pocketsPanel(pockets));
}
