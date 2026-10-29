//! Settings → Store: browse the signed catalog by category, open a package, install, update,
//! remove, and run tools. Downloads and installs run on a worker thread like the Updates page;
//! results come back as `UserEvent::Store`. The catalog and package rules are in `pocket_ui::store`
//! and docs/STORE.md.
//!
//! Three levels, all in the Settings row style: categories, the packages of one category with an
//! installed or update marker, and a package's page (a paper label with name and version, the
//! description, facts and the actions).

use std::path::Path;

use pocket_ui::gfx::Canvas;
use pocket_ui::pad::Button;
use pocket_ui::palette::*;
use pocket_ui::store::{self, Catalog, Package, Status, Store};
use pocket_ui::text::{Face, Text};

use crate::{stripe, App, SettingsPage, UserEvent, CANCEL, CONFIRM, NU_BTN_PAD, NU_PAD, NU_PILL, NU_SMALL};

/// The Store's pages in order: catalog category, title, what it holds. "emulators" shares the
/// cores page; other categories get a page of their own after these.
const PAGES: [(&str, &str, &str); 4] = [
    ("cores", "Emulators", "Extra libretro cores, and newer versions of the built-in ones."),
    ("frames", "Frames", "Art around the game picture, per console."),
    ("themes", "Themes", "Fonts for pocket-ui's menus."),
    ("tools", "Tools", "Small helpers, started from the Store or from Tools in Settings."),
];

/// Paper label text that is quieter than ink.
const PENCIL: u32 = 0x6b6862;

#[derive(Debug)]
pub enum Event {
    Catalog(Result<Catalog, String>),
    Progress(u64),
    /// An action finished: what to say about it, or why it failed.
    Done { id: String, action: Action, result: Result<String, String> },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Action {
    Install,
    Update,
    Remove,
    Run,
}

impl Action {
    fn label(self, p: &Package) -> String {
        match self {
            Action::Install => format!("Install, {}", size_label(p.size)),
            Action::Update => format!("Update to {}", p.version),
            Action::Remove => "Remove".into(),
            Action::Run => "Run".into(),
        }
    }

    fn hint(self) -> &'static str {
        match self {
            Action::Install => "Install",
            Action::Update => "Update",
            Action::Remove => "Remove",
            Action::Run => "Run",
        }
    }
}

#[derive(Clone, PartialEq)]
enum View {
    Categories,
    List(String),
    Detail(String, String),
}

/// A running action: which package, what is done, the expected size for the progress bar.
struct Busy {
    id: String,
    label: String,
    total: u64,
    /// When it started: a store job keeps idle sleep away, for `update::BUSY_BLOCKS_SLEEP_FOR` at most (review N5).
    since: std::time::Instant,
}

pub struct StorePage {
    /// settings.json `store_url`; `store::DEFAULT_URL` when unset (`store::catalog_url`).
    pub url: Option<String>,
    catalog: Option<Catalog>,
    loading: bool,
    /// Why the catalog couldn't be loaded.
    error: Option<String>,
    installed: Store,
    view: View,
    cat_sel: usize,
    list_sel: usize,
    action_sel: usize,
    /// Remove was pressed once; the next A removes, anything else cancels.
    confirm_remove: bool,
    busy: Option<Busy>,
    done: u64,
    /// The last result for a package: (package id, text, is an error).
    message: Option<(String, String, bool)>,
}

impl StorePage {
    /// Changes whenever what the store shows moves: for the UI sounds.
    pub fn ui_mark(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        match &self.view {
            View::Categories => 0u8.hash(&mut h),
            View::List(key) => (1u8, key).hash(&mut h),
            View::Detail(key, id) => (2u8, key, id).hash(&mut h),
        }
        (self.cat_sel, self.list_sel, self.action_sel, self.confirm_remove).hash(&mut h);
        h.finish()
    }

    pub fn new(data_dir: &Path, url: Option<String>) -> StorePage {
        StorePage {
            url,
            catalog: None,
            loading: false,
            error: None,
            installed: Store::open(data_dir),
            view: View::Categories,
            cat_sel: 0,
            list_sel: 0,
            action_sel: 0,
            confirm_remove: false,
            busy: None,
            done: 0,
            message: None,
        }
    }

    /// When the running install, update, removal or tool started; None when none runs.
    pub fn busy_since(&self) -> Option<std::time::Instant> {
        self.busy.as_ref().map(|b| b.since)
    }

    /// The catalog address; empty when there is no catalog (the Store is hidden then, `store::catalog_url`).
    fn catalog_url(&self) -> String {
        store::catalog_url(self.url.as_deref()).unwrap_or_default()
    }

    /// Category keys present in the catalog, in page order.
    fn categories(&self) -> Vec<String> {
        let Some(cat) = &self.catalog else { return Vec::new() };
        let mut keys: Vec<String> = PAGES.iter().map(|p| p.0.to_string()).filter(|k| cat.packages.iter().any(|p| page_key(&p.category) == *k)).collect();
        let mut others: Vec<String> = cat.packages.iter().map(|p| page_key(&p.category)).filter(|k| !keys.contains(k)).collect();
        others.sort();
        others.dedup();
        keys.extend(others);
        keys
    }

    fn packages(&self, key: &str) -> Vec<Package> {
        let mut list: Vec<Package> = self.catalog.iter().flat_map(|c| c.packages.iter()).filter(|p| page_key(&p.category) == key).cloned().collect();
        list.sort_by_key(|p| p.name.to_lowercase());
        list
    }

    fn package(&self, id: &str) -> Option<Package> {
        self.catalog.as_ref()?.packages.iter().find(|p| p.id == id).cloned()
    }

    /// What the package's page offers, in display order.
    fn actions(&self, p: &Package) -> Vec<Action> {
        match self.installed.status(p) {
            Status::NotInstalled => vec![Action::Install],
            Status::UpdateAvailable => vec![Action::Update, Action::Remove],
            Status::Installed if p.run.is_some() => vec![Action::Run, Action::Remove],
            Status::Installed => vec![Action::Remove],
        }
    }

    fn updates_in(&self, key: &str) -> usize {
        self.packages(key).iter().filter(|p| self.installed.status(p) == Status::UpdateAvailable).count()
    }
}

/// The page a catalog category is listed on.
fn page_key(category: &str) -> String {
    match category {
        "emulators" => "cores".into(),
        other => other.to_string(),
    }
}

fn page_title(key: &str) -> String {
    PAGES.iter().find(|p| p.0 == key).map(|p| p.1.to_string()).unwrap_or_else(|| {
        let mut c = key.chars();
        c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
    })
}

fn page_blurb(key: &str) -> &'static str {
    PAGES.iter().find(|p| p.0 == key).map_or("", |p| p.2)
}

fn size_label(bytes: u64) -> String {
    match bytes {
        0..=1023 => format!("{bytes} bytes"),
        1024..=1_048_575 => format!("{} KB", bytes.div_ceil(1024)),
        _ => format!("{:.1} MB", bytes as f64 / 1_048_576.0),
    }
}

/// Muted colour of a category, for the edge of a package's label.
fn page_colour(key: &str) -> u32 {
    match key {
        "cores" => 0x5a7d8c,
        "frames" => 0x8c6d5a,
        "themes" => 0x7a6cb0,
        "tools" => 0x7d8c5a,
        other => stripe(other),
    }
}

impl App {
    /// Opens the Store from Settings; the catalog is fetched the first time.
    pub(crate) fn open_store(&mut self) -> Option<SettingsPage> {
        // Read again: the start's thread may have moved tools out of ROCKNIX's folder since (`store::migrate_tools_in`).
        if self.store.busy.is_none() {
            self.store.installed = Store::open(&self.data_dir);
        }
        if self.store.catalog.is_none() && !self.store.loading {
            self.store_fetch();
        }
        Some(SettingsPage::Store)
    }

    fn store_fetch(&mut self) {
        let url = self.store.catalog_url();
        let (proxy, data_dir) = (self.proxy.clone(), self.data_dir.clone());
        self.store.loading = true;
        self.store.error = None;
        std::thread::spawn(move || {
            let _ = proxy.send_event(UserEvent::Store(Event::Catalog(store::fetch_catalog(&url, &data_dir))));
        });
    }

    fn store_start(&mut self, p: Package, action: Action) {
        if self.store.busy.is_some() {
            return;
        }
        let proxy = self.proxy.clone();
        let send = move |e| {
            let _ = proxy.send_event(UserEvent::Store(e));
        };
        let (url, data_dir, id) = (self.store.catalog_url(), self.data_dir.clone(), p.id.clone());
        let label = match action {
            Action::Install => format!("Installing {}", p.name),
            Action::Update => format!("Updating {}", p.name),
            Action::Remove => format!("Removing {}", p.name),
            Action::Run => format!("Running {}", p.name),
        };
        let total = if matches!(action, Action::Install | Action::Update) { p.size } else { 0 };
        self.store.busy = Some(Busy { id: id.clone(), label, total, since: std::time::Instant::now() });
        self.store.done = 0;
        self.store.message = None;
        std::thread::spawn(move || {
            let result = match action {
                Action::Install | Action::Update => {
                    let progress = |n| send(Event::Progress(n));
                    Store::open(&data_dir).install_from(&p, &url, &progress).map(|_| match action {
                        Action::Update => format!("Updated to {}.", p.version),
                        _ => "Installed.".to_string(),
                    })
                }
                Action::Remove => Store::open(&data_dir).uninstall(&p.id).map(|_| "Removed.".to_string()),
                Action::Run => match p.run.as_deref() {
                    Some(script) if !script.contains("..") => pocket_ui::tools::run_quiet(&p.install_dir(&data_dir).join(script)),
                    _ => Err("This tool has nothing to run.".into()),
                },
            };
            send(Event::Done { id, action, result });
        });
    }

    pub(crate) fn store_event(&mut self, e: Event) {
        match e {
            Event::Progress(n) => self.store.done = n,
            Event::Catalog(result) => {
                self.store.loading = false;
                match result {
                    Ok(c) => {
                        self.store.catalog = Some(c);
                        // A damaged installed.json stays in view: every action is refused until it is fixed.
                        self.store.error = self.store.installed.damaged().map(str::to_string);
                        // Packages may have gone from the catalog: the selections stay in range.
                        self.store.cat_sel = self.store.cat_sel.min(self.store.categories().len().saturating_sub(1));
                    }
                    Err(e) => self.store.error = Some(e),
                }
            }
            Event::Done { id, action, result } => {
                self.store.busy = None;
                self.store.installed = Store::open(&self.data_dir);
                let is_theme = self.store.package(&id).is_some_and(|p| page_key(&p.category) == "themes");
                if result.is_ok() && is_theme && action != Action::Run {
                    // New or removed type shows at once.
                    self.text = Text::new();
                }
                if result.is_ok() && matches!(action, Action::Install | Action::Update | Action::Remove) {
                    println!("store: {action:?} {id} ok");
                    self.store.action_sel = 0;
                }
                self.store.message = Some(match result {
                    Ok(m) => (id, m, false),
                    Err(e) => (id, e, true),
                });
            }
        }
    }

    pub(crate) fn store_input(&mut self, b: Button) -> Option<SettingsPage> {
        let s = &mut self.store;
        let step = |sel: &mut usize, n: usize| match b {
            Button::Up | Button::Left if n > 0 => *sel = (*sel + n - 1) % n,
            Button::Down | Button::Right if n > 0 => *sel = (*sel + 1) % n,
            Button::L1 => *sel = sel.saturating_sub(5),
            Button::R1 if n > 0 => *sel = (*sel + 5).min(n - 1),
            _ => {}
        };
        match s.view.clone() {
            View::Categories => {
                let cats = s.categories();
                step(&mut s.cat_sel, cats.len());
                if b == CANCEL {
                    return None;
                }
                if b == Button::North || (b == CONFIRM && cats.is_empty()) {
                    if !s.loading {
                        self.store_fetch();
                    }
                } else if b == CONFIRM {
                    if let Some(key) = cats.get(s.cat_sel) {
                        s.view = View::List(key.clone());
                        s.list_sel = 0;
                    }
                }
            }
            View::List(key) => {
                let list = s.packages(&key);
                step(&mut s.list_sel, list.len());
                if b == CANCEL {
                    s.view = View::Categories;
                } else if b == CONFIRM {
                    if let Some(p) = list.get(s.list_sel) {
                        s.view = View::Detail(key, p.id.clone());
                        s.action_sel = 0;
                        if s.message.as_ref().is_some_and(|(id, _, _)| *id != p.id) {
                            s.message = None;
                        }
                    }
                }
            }
            View::Detail(key, id) => {
                let Some(p) = s.package(&id) else {
                    s.view = View::List(key);
                    return Some(SettingsPage::Store);
                };
                let actions = s.actions(&p);
                let confirming = std::mem::take(&mut s.confirm_remove);
                if s.busy.is_none() {
                    step(&mut s.action_sel, actions.len());
                }
                if b == CANCEL {
                    s.view = View::List(key);
                } else if b == CONFIRM && s.busy.is_none() {
                    match actions.get(s.action_sel).copied() {
                        // Removing takes a second press, so a double press after installing can't undo it.
                        Some(Action::Remove) if !confirming => s.confirm_remove = true,
                        Some(a) => self.store_start(p, a),
                        None => {}
                    }
                }
            }
        }
        Some(SettingsPage::Store)
    }

    /// Title, status pill and the current level of the Store.
    pub(crate) fn paint_store(&mut self, c: &mut Canvas) {
        c.clear(pal().void);
        self.status_pill(c);
        let title = match &self.store.view {
            View::Categories => "Store".to_string(),
            View::List(key) | View::Detail(key, _) if key.is_empty() => "Store".to_string(),
            View::List(key) | View::Detail(key, _) => page_title(key),
        };
        self.text.draw(c, Face::Title, 50, NU_PAD + NU_BTN_PAD, NU_PAD + 58, pal().chalk, &title);
        match self.store.view.clone() {
            View::Categories => self.paint_store_categories(c),
            View::List(key) => self.paint_store_list(c, &key),
            View::Detail(key, id) => match self.store.package(&id) {
                Some(p) => self.paint_store_detail(c, &key, &p),
                None => self.paint_store_list(c, &key),
            },
        }
    }

    /// One list row in the Settings style, with a right-hand marker.
    pub(crate) fn store_row(&mut self, c: &mut Canvas, y: i32, label: &str, selected: bool, marker: Marker) {
        let (pad, right, row, font) = (NU_PAD, c.w - NU_PAD, row_h(), FONT);
        let base = y + row / 2 + pocket_ui::type_scale::mid(font);
        let label = self.text.fit(Face::Body, font, label, right - pad - 2 * NU_BTN_PAD - 300);
        if selected {
            let w = self.text.width(Face::Body, font, &label) + 2 * NU_BTN_PAD;
            c.round_rect(pad, y + 4, right - pad, row - 8, (row - 8) / 2, pal().band);
            c.round_rect(pad, y + 4, w, row - 8, (row - 8) / 2, pal().select);
            self.text.draw(c, Face::Body, font, pad + NU_BTN_PAD, base, pal().ink, &label);
        } else {
            self.text.draw(c, Face::Body, font, pad + NU_BTN_PAD, base, pal().text, &label);
        }
        let quiet = if selected { pal().text } else { pal().dust };
        match marker {
            Marker::None => {}
            Marker::Text(t) => self.text.draw_right(c, Face::Body, NU_SMALL, right - NU_BTN_PAD, base - 2, quiet, &t),
            // A paper sticker: the one bright thing in a list, so updates are found at a glance.
            Marker::Sticker(t) => {
                let w = self.text.width(Face::Body, NU_SMALL, &t) + 36;
                let h = 44;
                let x = right - NU_BTN_PAD - w + 18;
                c.round_rect(x, y + (row - h) / 2, w, h, h / 2, pal().label);
                self.text.draw(c, Face::Body, NU_SMALL, x + 18, y + (row - h) / 2 + h / 2 + pocket_ui::type_scale::mid(NU_SMALL), pal().ink, &t);
            }
        }
    }

    fn paint_store_categories(&mut self, c: &mut Canvas) {
        let top = NU_PAD + NU_PILL + 12;
        let cats = self.store.categories();
        if cats.is_empty() {
            let (msg, err) = match (&self.store.error, self.store.loading) {
                (_, true) => ("Loading the store…".to_string(), false),
                (Some(e), _) => (e.clone(), true),
                (None, _) if self.store.catalog.is_some() => ("Nothing in the store yet.".to_string(), false),
                (None, _) => ("The store isn't loaded.".to_string(), false),
            };
            // A picture above the words (`ui_art`): a radio asleep when it couldn't be reached, an empty stall else.
            let art = if err { pocket_ui::ui_art::Art::Offline } else { pocket_ui::ui_art::Art::Store };
            let dy = if self.store.loading { 0 } else { pocket_ui::ui_art::above_words(c, art, c.w / 2, true, top + 8, c.h / 2 - 60, 40) };
            self.store_paragraph(c, c.h / 2 - 60 + dy, &msg, if err { pal().alert } else { pal().chalk });
            let hints: &[(&str, &str)] = if self.store.loading { &[("B", "Back")] } else { &[("B", "Back"), ("A", "Try again")] };
            self.nu_hints(c, hints);
            return;
        }
        for (i, key) in cats.iter().enumerate() {
            let n = self.store.packages(key).len();
            let updates = self.store.updates_in(key);
            let marker = if updates > 0 {
                Marker::Sticker(if updates == 1 { "1 update".into() } else { format!("{updates} updates") })
            } else {
                Marker::Text(if n == 1 { "1 package".into() } else { format!("{n} packages") })
            };
            self.store_row(c, top + i as i32 * row_h(), &page_title(key), i == self.store.cat_sel, marker);
        }
        // The selected category's line, or why a refresh failed (the old list stays usable).
        let (line, colour) = match (&self.store.error, self.store.loading) {
            (_, true) => ("Looking for new packages…".to_string(), pal().dust),
            (Some(e), _) => (e.clone(), pal().alert),
            _ => (cats.get(self.store.cat_sel).map_or("", |k| page_blurb(k)).to_string(), pal().chalk),
        };
        self.store_footer(c, &line, colour);
        self.nu_hints(c, &[("B", "Back"), ("X", "Refresh"), ("A", "Open")]);
    }

    fn paint_store_list(&mut self, c: &mut Canvas, key: &str) {
        let top = NU_PAD + NU_PILL + 12;
        let list = self.store.packages(key);
        if list.is_empty() {
            // Never a blank page: the same stall as the categories view, and the way back.
            let (msg, err) = match &self.store.error {
                Some(e) => (e.clone(), true),
                None => ("Nothing here yet.".to_string(), false),
            };
            let art = if err { pocket_ui::ui_art::Art::Offline } else { pocket_ui::ui_art::Art::Store };
            let dy = pocket_ui::ui_art::above_words(c, art, c.w / 2, true, top + 8, c.h / 2 - 60, 40);
            self.store_paragraph(c, c.h / 2 - 60 + dy, &msg, if err { pal().alert } else { pal().chalk });
            self.nu_hints(c, &[("B", "Back")]);
            return;
        }
        let visible = ((c.h - top - NU_PILL - 170) / row_h()).max(1) as usize;
        let sel = self.store.list_sel.min(list.len().saturating_sub(1));
        let first = crate::window_start(sel, visible, list.len());
        for (i, p) in list.iter().enumerate().skip(first).take(visible) {
            let marker = match (&self.store.busy, self.store.installed.status(p)) {
                (Some(b), _) if b.id == p.id => Marker::Text(progress_text(b, self.store.done)),
                (_, Status::UpdateAvailable) => Marker::Sticker("Update".into()),
                (_, Status::Installed) => Marker::Text("Installed".into()),
                (_, Status::NotInstalled) => Marker::Text(size_label(p.size)),
            };
            self.store_row(c, top + (i - first) as i32 * row_h(), &p.name, i == sel, marker);
        }
        let desc = list.get(sel).map(|p| p.description.clone()).unwrap_or_default();
        self.store_footer(c, &desc, pal().chalk);
        self.nu_hints(c, &[("B", "Back"), ("A", "Open")]);
    }

    fn paint_store_detail(&mut self, c: &mut Canvas, key: &str, p: &Package) {
        let (pad, right) = (NU_PAD, c.w - NU_PAD);
        let top = NU_PAD + NU_PILL + 12;
        let status = self.store.installed.status(p);
        let installed_version = self.store.installed.installed.get(&p.id).map(|i| i.version.clone());

        // The label: the package as a pulled-out spine, its category on the edge.
        let (lh, r) = (196, 18);
        c.round_rect(pad, top, right - pad, lh, r, page_colour(key));
        c.round_rect(pad + STRIPE, top, right - pad - STRIPE, lh, r, pal().label);
        c.rect(pad + STRIPE, top, r, lh, pal().label);
        let x = pad + STRIPE + NU_BTN_PAD;
        let badge = match status {
            Status::Installed => "Installed",
            Status::UpdateAvailable => "Update available",
            Status::NotInstalled => "",
        };
        let badge_w = if badge.is_empty() { 0 } else { self.text.width(Face::Body, NU_SMALL, badge) + 40 };
        let name = self.text.fit(Face::Title, 67, &p.name, right - x - NU_BTN_PAD - badge_w);
        self.text.draw(c, Face::Title, 67, x, top + 98, pal().ink, &name);
        let meta = format!("{} · Version {} · {}", page_title(key), p.version, size_label(p.size));
        self.text.draw(c, Face::Body, NU_SMALL, x, top + 150, pal().pick(PENCIL, 0x5f5b53), &meta);
        if !badge.is_empty() {
            self.text.draw_right(c, Face::Body, NU_SMALL, right - NU_BTN_PAD, top + 58, pal().pick(PENCIL, 0x5f5b53), badge);
        }

        // What it is.
        let max_w = right - pad - 2 * NU_BTN_PAD;
        let mut y = top + lh + 64;
        let lines = self.text.wrap(Face::Body, 34, &p.description, max_w);
        for line in lines.iter().take(4) {
            self.text.draw(c, Face::Body, 34, pad + NU_BTN_PAD, y, pal().chalk, line);
            y += 46;
        }

        // Facts in two columns, quiet.
        let mut facts: Vec<(&str, String)> = Vec::new();
        if let Some(v) = installed_version.filter(|_| status == Status::UpdateAvailable) {
            facts.push(("Installed", v));
        }
        if !p.author.is_empty() {
            facts.push(("By", p.author.clone()));
        }
        if !p.license.is_empty() {
            facts.push(("Licence", p.license.clone()));
        }
        y += 18;
        let col = (right - pad) / 2;
        for (i, (k, v)) in facts.iter().enumerate() {
            let fx = pad + NU_BTN_PAD + (i as i32 % 2) * col;
            let fy = y + (i as i32 / 2) * 44;
            let kw = self.text.draw(c, Face::Body, NU_SMALL, fx, fy, pal().dust, k);
            let v = self.text.fit(Face::Body, NU_SMALL, v, col - kw - 40 - NU_BTN_PAD);
            self.text.draw(c, Face::Body, NU_SMALL, fx + kw + 16, fy, pal().chalk, &v);
        }
        y += facts.len().div_ceil(2) as i32 * 44;
        // Where it goes, on a line of its own: paths are long.
        let kw = self.text.draw(c, Face::Body, NU_SMALL, pad + NU_BTN_PAD, y, pal().dust, "Goes to");
        let dest = self.text.fit(Face::Body, NU_SMALL, &p.install_dir(&self.data_dir).display().to_string(), max_w - kw - 16);
        self.text.draw(c, Face::Body, NU_SMALL, pad + NU_BTN_PAD + kw + 16, y, pal().chalk, &dest);
        y += 44;

        // Actions.
        let actions = self.store.actions(p);
        let busy = self.store.busy.as_ref().is_some_and(|b| b.id == p.id);
        let other_busy = self.store.busy.is_some() && !busy;
        let sel = self.store.action_sel.min(actions.len().saturating_sub(1));
        let ay = y.max(c.h - NU_PAD - NU_PILL - 170 - actions.len() as i32 * row_h());
        let confirming = self.store.confirm_remove;
        for (i, a) in actions.iter().enumerate() {
            let label = if confirming && *a == Action::Remove { "Really remove it?".to_string() } else { a.label(p) };
            let ry = ay + i as i32 * row_h();
            if self.store.busy.is_some() {
                let base = ry + row_h() / 2 + pocket_ui::type_scale::mid(FONT);
                self.text.draw(c, Face::Body, FONT, pad + NU_BTN_PAD, base, pal().dust, &label);
            } else {
                self.store_row(c, ry, &label, i == sel, Marker::None);
            }
        }

        // Progress, the last result, or why nothing can start right now.
        let message = self.store.message.clone().filter(|(id, _, _)| *id == p.id);
        match &self.store.busy {
            Some(b) if busy && b.total > 0 => {
                let (w, by) = (c.w - 2 * pad - 2 * NU_BTN_PAD, c.h - pad - NU_PILL - 90);
                let frac = (self.store.done as f32 / b.total as f32).clamp(0.0, 1.0);
                c.round_rect(pad + NU_BTN_PAD, by, w, 12, 6, pal().keycap);
                c.round_rect(pad + NU_BTN_PAD, by, ((w as f32 * frac) as i32).max(12), 12, 6, pal().label);
                let line = format!("{}, {}", b.label, progress_text(b, self.store.done));
                self.store_footer(c, &line, pal().chalk);
            }
            Some(b) if busy => self.store_footer(c, &format!("{}…", b.label), pal().chalk),
            Some(b) if other_busy => self.store_footer(c, &format!("{}. One thing at a time.", b.label), pal().dust),
            _ if confirming => self.store_footer(c, "Press A again to remove it. Any other button keeps it.", pal().chalk),
            _ => match message {
                Some((_, m, err)) => self.store_footer(c, &m, if err { pal().alert } else { pal().chalk }),
                None => {
                    let note = match actions.get(sel) {
                        Some(Action::Run) => "Runs the tool and shows its last line here.",
                        Some(Action::Remove) => "Removes exactly the files this package added.",
                        _ if key == "cores" => "Cores go to /storage/cores, where RetroArch and pocket-core find them.",
                        _ => "",
                    };
                    self.store_footer(c, note, pal().dust);
                }
            },
        }
        let hint = actions.get(sel).map_or("Choose", |a| a.hint());
        if self.store.busy.is_some() {
            self.nu_hints(c, &[("B", "Back")]);
        } else {
            self.nu_hints(c, &[("B", "Back"), ("A", hint)]);
        }
    }

    /// Centred, wrapped text starting at `y`.
    pub(crate) fn store_paragraph(&mut self, c: &mut Canvas, y: i32, msg: &str, colour: u32) {
        let max_w = c.w - 2 * NU_PAD - 4 * NU_BTN_PAD;
        for (i, line) in self.text.wrap(Face::Body, 40, msg, max_w).iter().enumerate() {
            let w = self.text.width(Face::Body, 40, line);
            self.text.draw(c, Face::Body, 40, (c.w - w) / 2, y + i as i32 * 54, colour, line);
        }
    }

    /// Wrapped, centred text just above the button hints, like the Updates page.
    pub(crate) fn store_footer(&mut self, c: &mut Canvas, msg: &str, colour: u32) {
        let max_w = c.w - 2 * NU_PAD - 2 * NU_BTN_PAD;
        let lines = self.text.wrap(Face::Body, NU_SMALL, msg, max_w);
        let mut y = c.h - NU_PAD - NU_PILL - 30 - 40 * (lines.len() as i32 - 1).max(0);
        for line in &lines {
            let w = self.text.width(Face::Body, NU_SMALL, line);
            self.text.draw(c, Face::Body, NU_SMALL, (c.w - w) / 2, y, colour, line);
            y += 40;
        }
    }

    /// `pocket-ui --snapshot store[:list|:detail[:<category>[:<row>[:<action row>]]]]`: the Store as it
    /// would look, with the catalog from settings.json `store_url` loaded synchronously.
    pub(crate) fn store_snapshot(&mut self, state: &str) -> bool {
        let mut parts = state.split(':');
        if parts.next() != Some("store") {
            return false;
        }
        self.settings_page = Some(SettingsPage::Store);
        match store::fetch_catalog(&self.store.catalog_url(), &self.data_dir) {
            Ok(c) => self.store.catalog = Some(c),
            Err(e) => self.store.error = Some(e),
        }
        let level = parts.next().unwrap_or("");
        let key = parts.next().map(str::to_string).or_else(|| self.store.categories().first().cloned()).unwrap_or_default();
        // No catalog, no category: the categories page says so; a list or detail of "" would have no title.
        let level = if key.is_empty() { "" } else { level };
        let row: usize = parts.next().and_then(|n| n.parse().ok()).unwrap_or(0);
        self.store.action_sel = parts.next().and_then(|n| n.parse().ok()).unwrap_or(0);
        self.store.cat_sel = self.store.categories().iter().position(|k| *k == key).unwrap_or(0);
        self.store.list_sel = row;
        self.store.view = match level {
            "list" => View::List(key),
            "detail" => match self.store.packages(&key).get(row) {
                Some(p) => View::Detail(key, p.id.clone()),
                None => View::List(key),
            },
            _ => View::Categories,
        };
        true
    }
}

/// A row of the Store's lists: 66 px around `FONT`, taller with Settings → Text size (`type_scale`).
pub(crate) fn row_h() -> i32 {
    pocket_ui::type_scale::row(66, FONT)
}
pub(crate) const FONT: u32 = 36;

pub(crate) enum Marker {
    None,
    Text(String),
    Sticker(String),
}

fn progress_text(b: &Busy, done: u64) -> String {
    if b.total > 0 {
        format!("{}%", (done.min(b.total) * 100 / b.total))
    } else {
        "…".into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_read_naturally() {
        assert_eq!(size_label(512), "512 bytes");
        assert_eq!(size_label(1500), "2 KB");
        assert_eq!(size_label(3_500_000), "3.3 MB");
    }

    #[test]
    fn emulators_share_the_cores_page() {
        assert_eq!(page_key("emulators"), "cores");
        assert_eq!(page_title("cores"), "Emulators");
        assert_eq!(page_title("effects"), "Effects");
    }
}
