// "Send from your phone" (src/phone_upload.rs): picks files, asks the Pocket where each goes, sends them one or two at
// a time. Nothing is fetched from anywhere else; the words follow the phone's language (German, Spanish or English).
"use strict";

(function () {
  var data = JSON.parse(document.getElementById("data").textContent);
  var base = "/" + location.pathname.split("/")[1] + "/";
  var lang = (navigator.language || "").toLowerCase().slice(0, 2);
  var de = lang === "de";
  var es = lang === "es";
  var T = de ? {
    title: "Spiele auf deinen Pocket schicken",
    lead: "Wähle Spieldateien auf diesem Handy. Sie landen direkt im richtigen Ordner auf dem Pocket.",
    pick: "Dateien wählen",
    folder: "Ordner wählen",
    stopped: "Der Pocket hat diese Seite geschlossen oder ist eingeschlafen. Dateien, die noch unterwegs waren, wurden nicht behalten. Öffne „Vom Handy senden“ auf dem Pocket erneut und scanne den neuen Code.",
    help: "Nur im selben WLAN und nur, solange die Seite auf dem Pocket offen ist. Nicht verschlüsselt: Andere in diesem WLAN könnten den Code und die gesendeten Spiele sehen und sie unterwegs verändern. Nur im eigenen WLAN zu Hause nutzen. Herunterladen kann so niemand etwas vom Pocket; er sagt nur, ob ein Spiel mit demselben Namen schon da ist.",
    choose: "System wählen …",
    chooseLine: "Mehrere Systeme nehmen diese Datei. Wähle eines:",
    follows: "Gehört zu ",
    same: function (n, name) { return "Auch die " + (n === 1 ? "andere Datei" : n + " anderen Dateien") + " an " + name; },
    waiting: "Wartet …",
    checking: "Wird geprüft …",
    sending: "Wird gesendet: ",
    added: "Hinzugefügt: ",
    exists: "Ist schon auf dem Pocket und bleibt, wie sie ist. Zum Ersetzen erst dort löschen:",
    none: "Kein System nimmt diese Art Datei.",
    save: "Spielstände werden so nicht übertragen.",
    unreadable: "Dieses Handy konnte die Datei nicht lesen. Von dieser Datei wurde nichts behalten.",
    lost: "Die Verbindung zum Pocket ist abgebrochen. Von dieser Datei wurde nichts behalten.",
    summary: function (n, of, skipped) {
      return "Hinzugefügt: " + n + (n === 1 ? " Spiel" : " Spiele") + " von " + of + (skipped ? " (" + skipped + " andere Dateien übersprungen)" : "");
    }
  } : es ? {
    title: "Pasa juegos a tu Pocket",
    lead: "Elige archivos de juego en este teléfono. Van directo a la carpeta correcta del Pocket.",
    pick: "Elegir archivos",
    folder: "Elegir una carpeta",
    stopped: "El Pocket cerró esta página o se suspendió. Los archivos que iban en camino no se guardaron. Abre «Desde el teléfono» en el Pocket otra vez y escanea el código nuevo.",
    help: "Solo con el mismo Wi-Fi y solo mientras la página esté abierta en el Pocket. Sin cifrar: otras personas en este Wi-Fi podrían ver el código y los juegos que envías, y cambiarlos por el camino. Úsalo solo en tu Wi-Fi de casa. Nadie puede descargar nada del Pocket así; solo te dice si ya hay un juego con el mismo nombre.",
    choose: "Elige un sistema…",
    chooseLine: "Varios sistemas aceptan este archivo. Elige uno:",
    follows: "Va con ",
    same: function (n, name) { return "Enviar también " + (n === 1 ? "el otro archivo" : "los otros " + n + " archivos") + " a " + name; },
    waiting: "Esperando…",
    checking: "Comprobando…",
    sending: "Enviando: ",
    added: "Añadido: ",
    exists: "Ya está en el Pocket y se queda como está. Para reemplazarlo, bórralo allí primero:",
    none: "Ningún sistema acepta este tipo de archivo.",
    save: "Las partidas no se pasan así.",
    unreadable: "Este teléfono no pudo leer el archivo. De este archivo no se guardó nada.",
    lost: "Se perdió la conexión con el Pocket. De este archivo no se guardó nada.",
    summary: function (n, of, skipped) {
      return "Añadido: " + n + (n === 1 ? " juego" : " juegos") + " de " + of + (skipped ? " (" + skipped + (skipped === 1 ? " otro archivo omitido)" : " otros archivos omitidos)") : "");
    }
  } : {
    title: "Add games to your Pocket",
    lead: "Pick game files on this phone. They go straight into the right folder on the Pocket.",
    pick: "Choose files",
    folder: "Choose a folder",
    stopped: "The Pocket closed this page or went to sleep. Files that were still on their way were not kept. Open “Send from your phone” on the Pocket again and scan the new code.",
    help: "Same Wi-Fi only, and only while the page is open on the Pocket. Not encrypted: others on this Wi-Fi could see the code and the games you send, and change them on the way. Use it on your own Wi-Fi at home. Nobody can download anything from the Pocket this way; it only says whether a game of the same name is already there.",
    choose: "Choose a system…",
    chooseLine: "Several systems take this file. Choose one:",
    follows: "Goes with ",
    same: function (n, name) { return "Send the " + (n === 1 ? "other file" : n + " others") + " to " + name + " too"; },
    waiting: "Waiting…",
    checking: "Checking…",
    sending: "Sending: ",
    added: "Added: ",
    exists: "Already on the Pocket and kept as it is. To replace it, delete it there first:",
    none: "No system takes this kind of file.",
    save: "Saves can't be sent this way.",
    unreadable: "This phone could not read the file. Nothing of this file was kept.",
    lost: "The connection to the Pocket broke off. Nothing of this file was kept.",
    summary: function (n, of, skipped) {
      return "Added: " + n + (n === 1 ? " game" : " games") + " of " + of + (skipped ? " (" + skipped + " other files skipped)" : "");
    }
  };
  if (de) {
    document.documentElement.lang = "de";
    document.title = "pocket-ui: Spiele hinzufügen";
  } else if (es) {
    document.documentElement.lang = "es";
    document.title = "pocket-ui: añadir juegos";
  }
  document.querySelectorAll("[data-t]").forEach(function (el) { el.textContent = T[el.getAttribute("data-t")]; });

  var ios = /iPhone|iPad|iPod/.test(navigator.userAgent);
  var folderInput = document.getElementById("folder");
  if (!ios && "webkitdirectory" in folderInput) {
    document.getElementById("folder-pick").hidden = false;
  }

  var rows = [];
  var running = 0;
  var stopped = false;
  var skippedQuietly = 0;
  var PARALLEL = 2;
  var SHEETS = ["cue", "gdi", "m3u"];
  // A sheet is read to find its tracks; the Pocket takes none larger (`MAX_SHEET`).
  var MAX_SHEET = 65536;
  var sameButton = null;

  function ext(name) {
    var i = name.lastIndexOf(".");
    return i < 0 ? "" : name.slice(i + 1).toLowerCase();
  }

  // The systems that take `name`, by the longest extension that fits (the Pocket decides again).
  function candidates(name) {
    var lower = name.toLowerCase();
    return data.systems.filter(function (s) {
      return s.ext.some(function (e) { return e && lower.length > e.length + 1 && lower.slice(-(e.length + 1)) === "." + e; });
    });
  }

  function size(n) {
    if (n < 1048576) return Math.max(1, Math.round(n / 1024)) + " KB";
    if (n < 1073741824) return Math.round(n / 1048576) + " MB";
    return (n / 1073741824).toFixed(1) + " GB";
  }

  function el(tag, cls, text) {
    var e = document.createElement(tag);
    if (cls) e.className = cls;
    if (text !== undefined) e.textContent = text;
    return e;
  }

  function setLine(row, text, cls) {
    row.line.textContent = text;
    row.line.className = "line" + (cls ? " " + cls : "");
  }

  function summary() {
    var tried = rows.filter(function (r) { return r.state !== "skipped"; }).length;
    var added = rows.filter(function (r) { return r.state === "added"; }).length;
    var s = document.getElementById("summary");
    s.hidden = rows.length === 0;
    s.textContent = T.summary(added, tried, skippedQuietly);
  }

  function add(file, fromFolder) {
    var rel = file.webkitRelativePath || "";
    var parts = rel.split("/");
    if (file.name.charAt(0) === ".") return;
    var e = ext(file.name);
    // `dir`: the one folder it keeps on the Pocket. `source`: where it lies on the phone, which says what belongs together
    // (Dreamcast/Game and Saturn/Game are two discs).
    var dir = parts.length > 1 && data.disc.indexOf(e) >= 0 ? parts[parts.length - 2] : "";
    var source = parts.slice(0, -1).join("/");
    var cands = candidates(file.name);
    var isSave = data.saves.indexOf(e) >= 0 || e.indexOf("state") === 0;
    if (fromFolder && (cands.length === 0 || isSave)) {
      skippedQuietly++;
      return;
    }
    var li = el("li");
    // Where several systems take it, the one the extension usually means (the Pocket's own rule, `PREFER`).
    var want = data.prefer[e] || e;
    var usual = cands.filter(function (s) { return s.id === want; })[0];
    var row = { file: file, dir: dir, source: source, cands: cands, system: cands.length === 1 ? cands[0].id : (usual ? usual.id : ""), state: "waiting", li: li };
    li.appendChild(el("div", "name", file.name));
    row.line = el("div", "line", T.waiting + " " + size(file.size));
    li.appendChild(row.line);
    document.getElementById("list").appendChild(li);
    rows.push(row);
    if (isSave) {
      row.state = "failed";
      setLine(row, T.save, "alert");
    } else if (cands.length === 0) {
      row.state = "failed";
      setLine(row, T.none, "alert");
    } else if (cands.length > 1 && !row.system) {
      row.state = "choose";
      chooser(row);
    }
  }

  function chooser(row) {
    setLine(row, T.chooseLine);
    var sel = el("select");
    sel.appendChild(el("option", "", T.choose));
    row.cands.forEach(function (s) {
      var o = el("option", "", s.name + " (" + s.folder + ")");
      o.value = s.id;
      sel.appendChild(o);
    });
    sel.addEventListener("change", function () {
      if (!sel.value) return;
      var name = sel.options[sel.selectedIndex].textContent;
      assign(row, sel.value);
      offerSame(row, sel.value, name);
      pump();
    });
    row.select = sel;
    row.li.appendChild(sel);
  }

  // This row goes to system `id`, and the tracks its sheet names with it; no other game does.
  function assign(row, id) {
    row.system = id;
    row.state = "waiting";
    if (row.select) row.select.remove();
    row.select = null;
    setLine(row, T.waiting);
    (row.tracks || []).forEach(function (t) { assign(t, id); });
  }

  // The same system for the files still asking only when the player says so: one .iso may be PS2 and the next PSP.
  function offerSame(row, id, name) {
    if (sameButton) sameButton.remove();
    sameButton = null;
    function asking() {
      return rows.filter(function (r) { return r.state === "choose" && r.cands.some(function (s) { return s.id === id; }); });
    }
    var n = asking().length;
    if (!n) return;
    var b = el("button", "same", T.same(n, name));
    b.type = "button";
    b.addEventListener("click", function () {
      asking().forEach(function (r) { assign(r, id); });
      b.remove();
      sameButton = null;
      pump();
    });
    row.li.appendChild(b);
    sameButton = b;
  }

  // The file names a disc sheet refers to, as the Pocket reads them (`sheet_refs`).
  function sheetNames(kind, text) {
    var out = [];
    text.split(/\r?\n/).forEach(function (raw) {
      var l = raw.replace(/^[\ufeff\s]+|\s+$/g, "");
      if (!l) return;
      if (kind === "m3u") {
        if (l.charAt(0) !== "#") out.push(l);
      } else if (kind === "cue") {
        var m = /^(?:FILE|CDTEXTFILE)\s+(?:"([^"]*)"|(\S+))/i.exec(l);
        if (m) out.push(m[1] !== undefined ? m[1] : m[2]);
      } else {
        (l.match(/"[^"]*"|\S+/g) || []).forEach(function (f) {
          if (!/^[\d-]+$/.test(f)) out.push(f.replace(/"/g, ""));
        });
      }
    });
    return out.map(function (n) { return n.toLowerCase(); });
  }

  function open(row) {
    return row.state === "held" || row.state === "choose";
  }

  // The sheets among `batch` are read, and the files they name beside them follow them: a track is no game of its own.
  function link(batch, done) {
    var sheets = batch.filter(function (r) { return SHEETS.indexOf(ext(r.file.name)) >= 0 && open(r) && r.file.size <= MAX_SHEET; });
    var left = sheets.length;
    if (!left) return done();
    function one() {
      if (--left === 0) done();
    }
    sheets.forEach(function (sheet) {
      var reader = new FileReader();
      reader.onload = function () {
        var names = sheetNames(ext(sheet.file.name), String(reader.result));
        batch.forEach(function (r) {
          if (r === sheet || r.sheet || r.source !== sheet.source || !open(r) || names.indexOf(r.file.name.toLowerCase()) < 0) return;
          r.sheet = sheet;
          (sheet.tracks = sheet.tracks || []).push(r);
          if (r.select) r.select.remove();
          r.select = null;
          r.state = "follows";
          setLine(r, T.follows + sheet.file.name);
        });
        one();
      };
      reader.onerror = one;
      reader.readAsText(sheet.file);
    });
  }

  function query(row) {
    var q = "name=" + encodeURIComponent(row.file.name) + "&size=" + row.file.size;
    if (row.system) q += "&system=" + encodeURIComponent(row.system);
    if (row.dir) q += "&dir=" + encodeURIComponent(row.dir);
    return q;
  }

  // The Pocket sends a short code with each refusal; the sentence in the phone's language is looked up here. Without a
  // code (or in English) the Pocket's own English sentence is shown.
  var R = de ? {
    bad_name: "Dieser Dateiname kann nicht verwendet werden.",
    saves: "Spielstände werden so nicht übertragen.",
    bios: "Das ist eine BIOS-Datei. BIOS-Dateien kommen mit der SD-Karte oder dem Web-Begleiter nach roms/bios.",
    marker: "Ein Name mit so einem Teil mittendrin kann so nicht gesendet werden. Benenne die Datei zuerst um.",
    folder_marker: "Ein Ordner mit so einem Teil im Namen kann nicht verwendet werden. Benenne ihn zuerst um.",
    wrong_system: "Dieses System nimmt diese Art Datei hier nicht.",
    no_system: "Kein System nimmt diese Art Datei hier.",
    choose_system: "Mehrere Systeme nehmen diese Art Datei: Wähle eines.",
    bad_folder: "Dieser Ordnername kann nicht verwendet werden.",
    in_the_way: "Ein gleichnamiger Eintrag steht im Weg.",
    sheet_names: "Diese Disc-Liste nennt Dateien außerhalb ihres Ordners. Sie darf nur Dateien in ihrem eigenen Ordner nennen.",
    sheet_big: "Diese Disc-Liste ist zu groß.",
    enough: "Diese Seite hat genug Dateien angenommen. Schließe sie und öffne sie neu für mehr.",
    exists: "Ist schon auf dem Pocket und bleibt, wie sie ist.",
    no_room: "Nicht genug Platz auf der SD-Karte.",
    too_big: "Die Datei ist zu groß für dieses System.",
    cut_off: "Der Upload wurde abgebrochen. Nichts wurde behalten.",
    gone: "Diese Datei kam nicht ganz an. Nichts wurde behalten. Sende sie noch einmal."
  } : es ? {
    bad_name: "Ese nombre de archivo no se puede usar.",
    saves: "Las partidas no se pasan así.",
    bios: "Es un archivo BIOS. Los BIOS van a roms/bios con la tarjeta SD o el acompañante web.",
    marker: "Un nombre con esa parte en medio no se puede enviar así. Renombra el archivo primero.",
    folder_marker: "Una carpeta con esa parte en el nombre no se puede usar. Renómbrala primero.",
    wrong_system: "Ese sistema no acepta este tipo de archivo aquí.",
    no_system: "Ningún sistema acepta este tipo de archivo aquí.",
    choose_system: "Varios sistemas aceptan este tipo de archivo: elige uno.",
    bad_folder: "Ese nombre de carpeta no se puede usar.",
    in_the_way: "Hay algo con ese nombre que estorba.",
    sheet_names: "Esta lista de discos nombra archivos fuera de su carpeta. Solo puede nombrar archivos de su propia carpeta.",
    sheet_big: "Esta lista de discos es demasiado grande.",
    enough: "Esta página ya ha aceptado bastantes archivos. Ciérrala y ábrela de nuevo para más.",
    exists: "Ya está en el Pocket y se queda como está.",
    no_room: "No hay espacio en la tarjeta SD.",
    too_big: "El archivo es demasiado grande para este sistema.",
    cut_off: "Se cortó la subida. No se guardó nada.",
    gone: "Este archivo no llegó entero. No se guardó nada. Envíalo otra vez."
  } : null;

  function reason(x, r) {
    if (R && r.code && R[r.code]) return R[r.code];
    if (de && x.status === 507) return "Nicht genug Platz auf der SD-Karte. " + (r.error || "");
    if (de && x.status === 413) return "Die Datei ist zu groß. " + (r.error || "");
    if (es && x.status === 507) return "No hay espacio en la tarjeta SD. " + (r.error || "");
    if (es && x.status === 413) return "El archivo es demasiado grande. " + (r.error || "");
    return r.error || ("HTTP " + x.status);
  }

  function lost() {
    stopped = true;
    document.getElementById("stopped").hidden = false;
  }

  function finish(row, state, text, cls) {
    row.state = state;
    setLine(row, text, cls);
    if (row.bar) {
      row.bar.remove();
      row.bar = null;
    }
    running--;
    summary();
    pump();
  }

  function check(row) {
    row.state = "checking";
    running++;
    setLine(row, T.checking);
    var x = new XMLHttpRequest();
    x.open("POST", base + "check?" + query(row));
    x.onload = function () {
      var r = {};
      try { r = JSON.parse(x.responseText); } catch (e) { r = { error: x.responseText }; }
      if (!r.ok) return finish(row, "failed", reason(x, r), "alert");
      if (r.exists) return finish(row, "skipped", T.exists + " " + r.path, "alert");
      running--;
      send(row);
    };
    x.onerror = function () { noAnswer(row, null); };
    x.send();
  }

  // Files go in pieces, each read into memory first and sent as its own request: Chrome on Android can't send a large
  // file from the phone's file picker in one go (it breaks off at once; 25.11.2026, a 1.62 GB PS2 disc). A piece that
  // gets no answer goes again; the Pocket says where it stands.
  var PIECE = 8388608;
  var TRIES = 8;

  function post(url, data, progress, done) {
    var x = new XMLHttpRequest();
    x.open("POST", url);
    if (progress) x.upload.onprogress = function (e) { progress(e.loaded); };
    x.onload = function () {
      var r = {};
      try { r = JSON.parse(x.responseText) || {}; } catch (e) { r = { error: x.responseText }; }
      done(x, r);
    };
    x.onerror = function () { done(null, {}); };
    x.onabort = x.onerror;
    x.send(data === undefined ? null : data);
  }

  // The bytes from `from` to `to` of `file`, read into memory: done(null, bytes) or done(error).
  function readPiece(file, from, to, done) {
    var called = false;
    function once(err, data) {
      if (called) return;
      called = true;
      done(err, data);
    }
    try {
      var reader = new FileReader();
      reader.onload = function () { once(null, reader.result); };
      reader.onerror = function () { once(reader.error || "error"); };
      reader.readAsArrayBuffer(file.slice(from, to));
    } catch (e) {
      once(e);
    }
  }

  // Whether the Pocket still has this page open: only then is it the connection that broke off, not the page.
  function stillOpen(done) {
    var x = new XMLHttpRequest();
    x.open("GET", base + "app.css");
    x.onload = function () { done(!(x.status === 503 && x.responseText === "Closed")); };
    x.onerror = function () { done(false); };
    x.send();
  }

  // A request got no answer, also after the tries: the page closed on the Pocket, or only the connection broke off.
  function noAnswer(row, id) {
    stillOpen(function (open) {
      if (open && id) cancel(id);
      if (!open) lost();
      finish(row, "failed", T.lost, "alert");
    });
  }

  function cancel(id) {
    post(base + "cancel?id=" + encodeURIComponent(id), undefined, null, function () {});
  }

  function progress(row, n) {
    if (!row.bar) return;
    row.bar.value = n;
    setLine(row, T.sending + Math.floor((n * 100) / (row.file.size || 1)) + " % · " + size(n) + " / " + size(row.file.size));
  }

  function send(row) {
    if (stopped) return;
    row.state = "sending";
    running++;
    row.bar = el("progress");
    row.bar.max = row.file.size || 1;
    row.bar.value = 0;
    row.li.appendChild(row.bar);
    setLine(row, T.sending + "0 %");
    post(base + "start?" + query(row), undefined, null, function (x, r) {
      if (!x) return noAnswer(row, null);
      if (x.status === 503 && r.error === "Closed") {
        lost();
        return finish(row, "failed", T.lost, "alert");
      }
      if (!r.ok) return finish(row, "failed", reason(x, r), "alert");
      row.id = r.id;
      row.piece = r.piece || PIECE;
      row.at = 0;
      row.tries = 0;
      piece(row);
    });
  }

  function piece(row) {
    var from = row.at;
    readPiece(row.file, from, Math.min(row.file.size, from + row.piece), function (err, data) {
      if (err) {
        cancel(row.id);
        return finish(row, "failed", T.unreadable, "alert");
      }
      post(base + "part?id=" + encodeURIComponent(row.id) + "&offset=" + from, data, function (n) { progress(row, from + n); }, function (x, r) {
        // The last piece: the file is in place.
        if (x && r.ok && r.path !== undefined) return finish(row, "added", T.added + r.path, "ok");
        if (x && r.ok) {
          row.at = r.received;
          row.tries = 0;
          progress(row, row.at);
          return piece(row);
        }
        if (x && x.status === 503 && r.error === "Closed") {
          lost();
          return finish(row, "failed", T.lost, "alert");
        }
        // Cut off or at the wrong place: on from what the Pocket has.
        if (x && typeof r.received === "number" && ++row.tries <= TRIES) {
          row.at = r.received;
          return piece(row);
        }
        // No answer, or the Pocket was busy: the same piece again after a moment.
        if (!x || x.status === 503) {
          if (++row.tries <= TRIES) return setTimeout(function () { piece(row); }, Math.min(15000, 1000 * Math.pow(2, row.tries - 1)));
          return noAnswer(row, row.id);
        }
        if (typeof r.received === "number") cancel(row.id);
        finish(row, "failed", reason(x, r), "alert");
      });
    });
  }

  function pump() {
    if (stopped) return;
    for (var i = 0; i < rows.length && running < PARALLEL; i++) {
      if (rows[i].state === "waiting") check(rows[i]);
    }
  }

  function picked(input, fromFolder) {
    var before = rows.length;
    Array.prototype.forEach.call(input.files, function (f) { add(f, fromFolder); });
    input.value = "";
    summary();
    // Not sent before the sheets among them are read: a track goes where its sheet goes.
    var batch = rows.slice(before);
    batch.forEach(function (r) { if (r.state === "waiting") r.state = "held"; });
    link(batch, function () {
      batch.forEach(function (r) { if (r.state === "held") r.state = "waiting"; });
      batch.forEach(function (r) { if (r.state === "waiting" && r.tracks) assign(r, r.system); });
      pump();
    });
  }

  document.getElementById("files").addEventListener("change", function () { picked(this, false); });
  folderInput.addEventListener("change", function () { picked(this, true); });
})();
