// pocket-ui web companion: Photo album & screenshots (Point 3) and Software updates (Point 6).
// Kept in its own file so parallel 0.7 work merges cleanly. Uses only global t(en, vars) and PocketWeb helpers.
"use strict";

window.PocketMediaUpdate = (() => {
  let updatePoll = null;
  let reconnectPoll = null;
  let mediaFilter = "all";
  let lastUpdate = null;

  function tr(en, vars) {
    return typeof window.t === "function" ? window.t(en, vars) : en;
  }

  function pw() {
    return window.PocketWeb;
  }

  function dateLocale() {
    const lang = (document.documentElement && document.documentElement.lang) || "en";
    if (lang === "pt") return "pt-BR";
    return lang;
  }

  function formatDate(d) {
    if (!d || Number.isNaN(d.getTime())) return "";
    try {
      return new Intl.DateTimeFormat(dateLocale(), {
        day: "2-digit",
        month: "2-digit",
        year: "numeric",
        hour: "2-digit",
        minute: "2-digit",
        hour12: false,
      }).format(d);
    } catch (e) {
      return "";
    }
  }

  function formatStamp(at) {
    const raw = String(at || "");
    const m = /^(\d{4})-(\d{2})-(\d{2}) (\d{2})-(\d{2})-(\d{2})(?: (\(\d+\)))?$/.exec(raw);
    if (!m) return raw;
    const d = new Date(Number(m[1]), Number(m[2]) - 1, Number(m[3]), Number(m[4]), Number(m[5]), Number(m[6]));
    const formatted = formatDate(d) || raw;
    return m[7] ? formatted + " " + m[7] : formatted;
  }

  function formatUnix(unix) {
    if (!unix || unix <= 0) return "";
    return formatDate(new Date(unix * 1000));
  }

  function stopPolls() {
    clearTimeout(updatePoll);
    clearTimeout(reconnectPoll);
    updatePoll = null;
    reconnectPoll = null;
  }

  function translateNavTabs() {
    const more = document.querySelector('#tabs a[data-tab="more"]');
    if (more) more.textContent = t("More");
  }

  // --- Photo album & screenshots -----------------------------------------------------------------------------------

  function photoCard(item, opts) {
    const { h, post, toast, size } = pw();
    const showGame = opts && opts.showGame;
    const onDelete = opts && opts.onDelete;
    const viewUrl = "/api/album/photo?id=" + encodeURIComponent(item.id) + "&file=" + encodeURIComponent(item.file);
    const dlUrl = viewUrl + "&download=1";
    const meta = [item.note || null, formatUnix(item.at) || null, item.bytes ? size(item.bytes) : null].filter(Boolean).join(" · ");
    const delBtn = h("button", { type: "button", class: "media-del", onclick: async () => {
      if (!confirm(t("Delete this album photo from the Pocket?"))) return;
      delBtn.disabled = true;
      try {
        await post("/api/album/delete", { id: item.id, file: item.file });
        toast(t("Photo deleted"));
        if (onDelete) onDelete();
      } catch (e) {
        delBtn.disabled = false;
        toast(e.message, true);
      }
    } }, t("Delete"));

    return h("figure", { class: "media-card album-card", stripe: item.stripe || null },
      h("a", { class: "media-thumb", href: viewUrl, target: "_blank", rel: "noopener", title: item.note || item.file },
        h("img", { src: viewUrl, alt: item.note || item.file, loading: "lazy" })),
      h("figcaption", { class: "media-cap" },
        showGame ? h("a", { class: "media-game", href: "#/game/" + encodeURIComponent(item.id) }, item.game) : null,
        item.note ? h("div", { class: "media-note" }, item.note) : null,
        meta && !item.note ? h("div", { class: "quiet small" }, meta) : (formatUnix(item.at) ? h("div", { class: "quiet small" }, formatUnix(item.at)) : null),
        h("div", { class: "media-actions" },
          h("a", { class: "button small dl-btn", href: dlUrl, download: "" }, t("Download file")),
          delBtn)));
  }

  function shotCard(item, opts) {
    const { h, post, toast, size } = pw();
    const showGame = opts && opts.showGame;
    const onDelete = opts && opts.onDelete;
    const isShelf = Boolean(item.shelf);
    const viewUrl = isShelf
      ? "/api/shot?shelf=1&at=" + encodeURIComponent(item.at)
      : "/api/shot?id=" + encodeURIComponent(item.id) + "&at=" + encodeURIComponent(item.at);
    const dlUrl = viewUrl + "&download=1";
    const nice = formatStamp(item.at);
    const delBtn = h("button", { type: "button", class: "media-del", onclick: async () => {
      const promptText = isShelf ? t("Delete this shelf picture?") : t("Delete this screenshot?");
      if (!confirm(promptText)) return;
      delBtn.disabled = true;
      try {
        const payload = isShelf ? { shelf: true, at: item.at } : { id: item.id, at: item.at };
        await post("/api/shot/delete", payload);
        toast(isShelf ? t("Shelf picture deleted") : t("Screenshot deleted"));
        if (onDelete) onDelete();
      } catch (e) {
        delBtn.disabled = false;
        toast(e.message, true);
      }
    } }, t("Delete"));

    return h("figure", { class: "media-card shot-card", stripe: item.stripe || null },
      h("a", { class: "media-thumb", href: viewUrl, target: "_blank", rel: "noopener", title: nice },
        h("img", { src: viewUrl, alt: (isShelf ? t("Your shelf as a picture") : (item.game || t("Screenshots"))) + " " + nice, loading: "lazy" })),
      h("figcaption", { class: "media-cap" },
        showGame ? (isShelf
          ? h("div", { class: "media-game" }, t("Your shelf as a picture"))
          : h("a", { class: "media-game", href: "#/game/" + encodeURIComponent(item.id) }, item.game)) : null,
        h("div", { class: "quiet small" }, [nice, item.bytes ? size(item.bytes) : null].filter(Boolean).join(" · ")),
        h("div", { class: "media-actions" },
          h("a", { class: "button small dl-btn", href: dlUrl, download: "" }, t("Download file")),
          delBtn)));
  }

  /// Renders the compact `#/more` hub for Photo album & screenshots and Software update.
  async function moreView() {
    stopPolls();
    const { h, api, view } = pw();
    const [album, upd] = await Promise.all([api("/api/album"), api("/api/update")]);
    paintBadge(upd);

    const albumTotal = album.albumTotal || 0;
    const shotsTotal = (album.shotsTotal || 0) + (album.shelfTotal || 0);
    const mediaParts = [];
    if (albumTotal) mediaParts.push(albumTotal === 1 ? t("1 album photo") : t("{n} album photos", { n: albumTotal }));
    if (shotsTotal) mediaParts.push(shotsTotal === 1 ? t("1 screenshot") : t("{n} screenshots", { n: shotsTotal }));
    const mediaSide = mediaParts.length ? mediaParts.join(" · ") : t("No photos yet");

    const updateSide = upd.available
      ? h("span", { class: "update-pill" }, t("new: {s}", { s: upd.available }))
      : "pocket-ui " + upd.current;

    view(
      h("h2", {}, t("More")),
      h("ul", { class: "shelf more-list" },
        h("li", {},
          h("a", { class: "spine", href: "#/photos", stripe: "var(--amber)" },
            h("span", { class: "name" }, t("Photo album & screenshots")),
            h("span", { class: "side" }, mediaSide))),
        h("li", {},
          h("a", { class: "spine", href: "#/updates", stripe: upd.available ? "var(--amber)" : null },
            h("span", { class: "name" }, t("Software update")),
            h("span", { class: "side" }, updateSide)))));
  }

  /// Renders the dedicated `#/photos` view: photo album, game screenshots, shelf pictures, and ZIP download.
  async function photosView() {
    stopPolls();
    const { h, api, view } = pw();
    const data = await api("/api/album");
    const totalAll = (data.albumTotal || 0) + (data.shotsTotal || 0) + (data.shelfTotal || 0);

    const draw = () => {
      const filters = h("div", { class: "seg", role: "group", "aria-label": t("Photos") },
        [
          ["all", t("All")],
          ["album", t("Photo album")],
          ["shots", t("Screenshots")],
        ].map(([k, label]) => h("button", {
          type: "button",
          class: mediaFilter === k ? "on" : "",
          "aria-pressed": mediaFilter === k ? "true" : "false",
          onclick: () => { mediaFilter = k; draw(); },
        }, label)));

      const zipScope = mediaFilter === "album" ? "?scope=album" : mediaFilter === "shots" ? "?scope=shots" : "";
      const hasForScope = mediaFilter === "album"
        ? data.albumTotal > 0
        : mediaFilter === "shots"
          ? (data.shotsTotal + data.shelfTotal) > 0
          : totalAll > 0;

      const zipBtn = hasForScope
        ? h("a", { class: "button primary", href: "/api/media/zip" + zipScope, download: "" }, t("Download all as ZIP"))
        : h("button", { type: "button", disabled: true }, t("Download all as ZIP"));

      const parts = [
        h("a", { class: "back", href: "#/more" }, t("More")),
        h("div", { class: "media-head" },
          h("h2", {}, t("Photo album & screenshots")),
          zipBtn),
        h("div", { class: "row" }, filters),
      ];

      if (mediaFilter === "all" || mediaFilter === "album") {
        const albumCount = data.albumTotal === 1
          ? t("1 album photo")
          : t("{n} album photos", { n: data.albumTotal });
        parts.push(h("h3", {}, t("Photo album"), " ", h("span", { class: "count" }, albumCount)));
        if (data.album && data.album.length) {
          if (data.albumTotal > data.album.length) {
            parts.push(h("p", { class: "quiet small" }, t("Showing the newest {n} of {n2}.", { n: data.album.length, n2: data.albumTotal })));
          }
          parts.push(h("div", { class: "media-grid" }, data.album.map((item) => photoCard(item, { showGame: true, onDelete: photosView }))));
        } else {
          const emptyAlbum = data.albumOn
            ? t("No album photos yet. Big moments from your saves land here automatically.")
            : t("No album photos yet. Photos of big moments is off on the Pocket (Settings › Screen & sound › Photos of big moments).");
          parts.push(h("p", { class: "quiet" }, emptyAlbum));
        }
      }

      if (mediaFilter === "all" || mediaFilter === "shots") {
        const shotsAll = (data.shots || []).concat(data.shelf || []);
        const shotsTotal = (data.shotsTotal || 0) + (data.shelfTotal || 0);
        const shotsCount = shotsTotal === 1
          ? t("1 screenshot")
          : t("{n} screenshots", { n: shotsTotal });
        parts.push(h("h3", {}, t("Screenshots"), " ", h("span", { class: "count" }, shotsCount)));
        if (shotsAll.length) {
          if (data.shotsTotal > (data.shots || []).length) {
            parts.push(h("p", { class: "quiet small" }, t("Showing the newest {n} of {n2}.", { n: data.shots.length, n2: data.shotsTotal })));
          }
          parts.push(h("div", { class: "media-grid" }, shotsAll.map((item) => shotCard(item, { showGame: true, onDelete: photosView }))));
        } else {
          parts.push(h("p", { class: "quiet" }, t("No screenshots yet. Press HOME + A during a game to take one.")));
        }
      }

      view(...parts);
    };

    draw();
  }

  /// Renders the photo album & screenshots blocks on a single game page (`#/game/<id>`).
  function gameMedia(g, reload) {
    const { h } = pw();
    const albumList = g.album || [];
    const shotList = g.shots || [];
    const hasAny = albumList.length > 0 || shotList.length > 0;

    const albumHead = h("div", { class: "media-head media-subhead" },
      h("h3", {}, t("Photo album")),
      hasAny ? h("a", { class: "button small", href: "/api/media/zip?id=" + encodeURIComponent(g.id), download: "" }, t("Download game ZIP")) : null);

    const albumBlock = [
      albumHead,
      albumList.length
        ? h("div", { class: "media-grid" }, albumList.map((p) => photoCard(Object.assign({ id: g.id, game: g.name, stripe: g.stripe }, p), { showGame: false, onDelete: reload })))
        : h("p", { class: "quiet" }, g.albumOn
            ? t("No album photos for this game yet.")
            : t("No album photos yet. Photos of big moments is off on the Pocket (Settings › Screen & sound › Photos of big moments).")),
    ];

    const shotsBlock = [
      h("h3", {}, t("Screenshots")),
      shotList.length
        ? h("div", { class: "media-grid" }, shotList.map((at) => shotCard({ id: g.id, game: g.name, stripe: g.stripe, at }, { showGame: false, onDelete: reload })))
        : h("p", { class: "quiet" }, t("No screenshots for this game yet. Press HOME + A during a game to take one.")),
    ];

    return [albumBlock, shotsBlock];
  }

  /// Renders shelf pictures on `#/library` with individual download & delete controls.
  function shelfPictures(list, reload) {
    const { h } = pw();
    if (!list || !list.length) return null;
    return [
      h("h3", {}, t("Your shelf as a picture")),
      h("div", { class: "media-grid" }, list.map((at) => shotCard({ shelf: true, at }, { showGame: false, onDelete: reload }))),
    ];
  }

  // --- Software update (Point 6) -----------------------------------------------------------------------------------

  async function refreshBadge() {
    const { h, $, api, S } = pw();
    if (!S.csrf || S.playing) return;
    try {
      const u = await api("/api/update");
      paintBadge(u);
    } catch (e) {
      /* ignore while playing or offline */
    }
  }

  function onUpdatePage() {
    return /^#\/(more|updates)(\/|$)/.test((typeof location !== "undefined" && location.hash) || "");
  }

  function paintBadge(u) {
    if (u) lastUpdate = u;
    const cur = u || lastUpdate;
    const { h, $ } = pw();
    const slot = $("version");
    if (!slot || !cur) return;
    if (cur.available && !onUpdatePage()) {
      slot.replaceChildren(
        h("a", { class: "update-link", href: "#/updates" },
          "pocket-ui " + cur.current + " · ",
          h("span", { class: "update-pill" }, t("new: {s}", { s: cur.available })))
      );
    } else {
      slot.replaceChildren(h("a", { class: "quiet", href: "#/updates" }, "pocket-ui " + cur.current));
    }
    const tabEl = document.querySelector('#tabs a[data-tab="more"]');
    if (tabEl) tabEl.classList.toggle("has-update", Boolean(cur.available));
  }

  function syncBadge() {
    if (lastUpdate) paintBadge(lastUpdate);
  }

  async function waitForReconnect(targetVersion, box) {
    const { h, toast } = pw();
    let tries = 0;
    const probe = async () => {
      tries += 1;
      try {
        const r = await fetch("/api/session", { credentials: "same-origin" });
        if (r.ok) {
          const s = await r.json();
          const ver = s && s.info && s.info.version;
          if (ver && (ver === targetVersion || tries > 6)) {
            toast(t("pocket-ui is back on {s}.", { s: ver }));
            location.reload();
            return;
          }
        }
      } catch (e) {
        /* still restarting */
      }
      if (tries < 60) {
        reconnectPoll = setTimeout(probe, 2000);
      } else if (box) {
        box.append(h("p", { class: "quiet small" }, t("Installed. pocket-ui is restarting; this page will reconnect automatically.")));
      }
    };
    reconnectPoll = setTimeout(probe, 2500);
  }

  async function updatesView() {
    stopPolls();
    const { h, api, post, toast, view, size, S } = pw();
    const u = await api("/api/update");
    paintBadge(u);

    const isBusy = Boolean(u.busy);
    const playing = Boolean(u.playing || S.playing);

    const headline = u.available
      ? t("{s}, new: {s2}", { s: u.current, s2: u.available })
      : "pocket-ui " + u.current;

    const statusBlock = [];
    if (u.state === "checking") {
      statusBlock.push(h("p", {}, t("Checking for updates…")));
      statusBlock.push(h("progress", {}));
    } else if (u.state === "restarting") {
      statusBlock.push(h("p", { class: "ok" }, u.stepLine || t("Installed. pocket-ui is restarting; this page will reconnect automatically.")));
      statusBlock.push(h("p", { class: "quiet small" }, t("Installed. pocket-ui is restarting; this page will reconnect automatically.")));
      statusBlock.push(h("progress", { max: "100", value: "100" }));
    } else if (isBusy) {
      statusBlock.push(h("p", {}, u.stepLine || u.message || t("Update started on the Pocket.")));
      statusBlock.push(h("progress", u.percent !== null && u.percent !== undefined ? { max: "100", value: String(u.percent) } : {}));
    } else if (u.state === "failed") {
      statusBlock.push(h("p", { class: "alert" }, u.message || t("Install failed. Nothing was changed.")));
    } else if (!u.releaseBuild) {
      statusBlock.push(h("p", { class: "quiet" }, t("Development build: install a release package to use app updates.")));
    } else if (u.available) {
      statusBlock.push(h("p", {}, t("The update comes straight from pocket-ui. Your games and settings stay.")));
      if (u.size) {
        statusBlock.push(h("p", { class: "quiet small" }, t("Download size: {s}", { s: size(u.size) })));
      }
    } else if (u.checked) {
      statusBlock.push(h("p", { class: "ok" }, u.message || t("Everything is up to date.")));
    } else {
      statusBlock.push(h("p", { class: "quiet" }, t("Not checked yet. Check for updates to see if a new version is ready.")));
    }

    if (u.lowBattery) {
      statusBlock.push(h("p", { class: "alert small" }, u.lowBattery));
    }
    if (playing) {
      statusBlock.push(h("p", { class: "alert small" }, t("A game is running. Quit the game on the Pocket before installing an update.")));
    }

    const actions = [];
    if (u.available && u.releaseBuild) {
      const installBtn = h("button", {
        type: "button",
        class: "primary",
        disabled: isBusy || playing,
        onclick: async () => {
          if (!confirm(t("Update pocket-ui to {s} now? Your games and settings stay. The Pocket will restart.", { s: u.available }))) return;
          installBtn.disabled = true;
          try {
            const res = await post("/api/update/install", { version: u.available });
            toast(res && !res.restartingSoon ? t("Confirm the installation on the Pocket's screen.") : t("Update started on the Pocket."));
            updatesView();
          } catch (e) {
            installBtn.disabled = false;
            toast(e.message, true);
          }
        },
      }, t("Install"));
      actions.push(installBtn);
    }

    const checkBtn = h("button", {
      type: "button",
      disabled: isBusy || playing,
      onclick: async () => {
        checkBtn.disabled = true;
        try {
          await post("/api/update/check", {});
          updatesView();
        } catch (e) {
          checkBtn.disabled = false;
          toast(e.message, true);
        }
      },
    }, u.checked || u.available ? t("Check again") : t("Check for updates"));
    actions.push(checkBtn);

    const panel = h("div", { class: "panel update-panel" },
      h("div", { class: "quiet small" }, t("Current version")),
      h("h3", { class: "update-headline" }, headline),
      statusBlock,
      u.notes ? [h("h3", {}, t("What is new")), h("p", { class: "small update-notes" }, u.notes)] : null,
      h("div", { class: "row" }, actions));

    view(h("a", { class: "back", href: "#/more" }, t("More")), h("h2", {}, t("Software update")), panel);

    if (u.state === "restarting") {
      waitForReconnect(u.available, panel);
    } else if (isBusy) {
      updatePoll = setTimeout(() => {
        if ((location.hash || "").startsWith("#/updates")) updatesView();
      }, 1500);
    }
  }

  return {
    translateNavTabs,
    moreView,
    photosView,
    gameMedia,
    shelfPictures,
    updatesView,
    refreshBadge,
    paintBadge,
    syncBadge,
    formatStamp,
    formatUnix,
    stopPolls,
    tr,
  };
})();
