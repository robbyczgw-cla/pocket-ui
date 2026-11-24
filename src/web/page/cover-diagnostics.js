"use strict";

function boxArtUrl(game) {
  const query = new URLSearchParams({ id: game.id, v: game.coverRevision || "" });
  return "/api/cover?" + query;
}

function paintBoxArt(art, game, onMissing) {
  const missing = () => {
    art.hidden = true;
    art.replaceChildren();
    onMissing();
  };
  if (!game.cover) return missing();
  art.hidden = false;
  const image = h("img", { src: boxArtUrl(game), alt: t("Box art"),
    onerror: () => { if (image.parentNode === art) missing(); } });
  art.replaceChildren(image);
}

// Resize before sending anything to the Pocket. Keep the shape and never enlarge a small picture.
async function coverPng(file) {
  const url = URL.createObjectURL(file);
  try {
    const image = await new Promise((resolve, reject) => {
      const img = new Image();
      img.onload = () => resolve(img);
      img.onerror = reject;
      img.src = url;
    });
    const scale = Math.min(1, 1024 / Math.max(image.naturalWidth, image.naturalHeight));
    const canvas = document.createElement("canvas");
    canvas.width = Math.max(1, Math.round(image.naturalWidth * scale));
    canvas.height = Math.max(1, Math.round(image.naturalHeight * scale));
    canvas.getContext("2d").drawImage(image, 0, 0, canvas.width, canvas.height);
    return await new Promise((resolve, reject) => canvas.toBlob(
      (png) => png ? resolve(png) : reject(new Error()), "image/png"));
  } finally {
    URL.revokeObjectURL(url);
  }
}

function coverControls(game, art) {
  const query = new URLSearchParams({ id: game.id });
  const file = h("input", { type: "file", accept: "image/*", hidden: true });
  const choose = h("button", { onclick: () => file.click() }, t("Choose cover…"));
  const reset = h("button", { onclick: () => change(null) }, t("Reset to automatic"));
  const empty = h("span", { class: "quiet small" });
  const notice = h("p", { role: "status", class: "quiet small", hidden: true });
  const message = (text, error = false) => {
    notice.hidden = false;
    notice.className = error ? "alert small" : "quiet small";
    notice.textContent = text;
  };
  const paint = () => {
    empty.hidden = true;
    reset.hidden = !game.coverCustom;
    paintBoxArt(art, game, () => {
      empty.textContent = t("No cover yet.");
      empty.hidden = false;
    });
  };

  async function change(image) {
    choose.disabled = reset.disabled = true;
    message(image ? t("Saving cover…") : t("Resetting cover…"));
    try {
      let png;
      if (image) {
        try {
          png = await coverPng(image);
        } catch (e) {
          message(t("This picture could not be opened. Try another one."), true);
          return;
        }
        if (png.size > 8 * 1024 * 1024) {
          message(t("This cover is over 8 MB. Try a smaller picture."), true);
          return;
        }
      }
      const result = await api("/api/cover/" + (image ? "change?" : "reset?") + query, {
        method: "POST", body: png, headers: image ? { "Content-Type": "image/png" } : {},
      });
      Object.assign(game, result);
      paint();
      message(image ? t("Cover saved.") : t("Cover set to automatic."));
    } catch (e) {
      message(e instanceof Playing ? t("Try again when the game ends.") :
        t("The cover could not be changed. Try again."), true);
    } finally {
      choose.disabled = reset.disabled = false;
    }
  }

  file.addEventListener("change", () => {
    const image = file.files[0];
    file.value = "";
    if (image) change(image);
  });
  paint();
  return h("div", { class: "cover-controls" }, h("div", { class: "actions" }, file, choose, reset, empty), notice);
}

async function diagnosticsView() {
  const info = await api("/api/diagnostics/status");
  const notice = h("p", { role: "status", class: "alert small", hidden: true });
  const download = h("button", { class: "primary", onclick: async () => {
    notice.hidden = true;
    download.disabled = true;
    try {
      const response = await fetch("/api/diagnostics", { credentials: "same-origin" });
      if (response.status === 503 && (await response.json()).playing) {
        notice.textContent = t("Try again when the game ends.");
        notice.hidden = false;
        return;
      }
      if (!response.ok) throw new Error();
      const url = URL.createObjectURL(await response.blob());
      const link = h("a", { href: url, download: "pocket-diagnostics.zip", hidden: true });
      document.body.append(link);
      link.click();
      link.remove();
      setTimeout(() => URL.revokeObjectURL(url), 30000);
    } catch (e) {
      notice.textContent = t("The diagnosis ZIP could not be downloaded. Try again.");
      notice.hidden = false;
    } finally {
      download.disabled = false;
    }
  } }, t("Download diagnosis ZIP"));

  view(
    h("h2", {}, t("Diagnosis")),
    h("dl", { class: "facts" },
      h("dt", {}, t("App version")), h("dd", {}, info.version),
      h("dt", {}, t("Device model")), h("dd", {}, info.device || t("Device model unavailable"))),
    h("p", { class: "quiet" }, t("The ZIP contains the version, device model and recent app events.")),
    info.hasLog ? null : h("p", { class: "quiet" }, t("No shareable log entries yet.")),
    download, notice);
}
