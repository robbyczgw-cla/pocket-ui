//! Android's first start: portable Pocket painters, persisted progress, and framework SAF on a worker.
use super::*;
const NEXT: u16 = 100;
const SKIP: u16 = 101;
const BACK: u16 = 102;
const ANIMATIONS: u16 = 90;
const APPEARANCE: u16 = 91;
const CANCEL_DOWNLOAD: u16 = 92;
/// "Where your games are": a games folder found on the device (10, 11), an emulator app found (40...).
const FOUND_FOLDER: u16 = 10;
const FOUND_APP: u16 = 40;
/// ... and an app to get for a system with games and none to play it (60, 61).
const FOUND_GET: u16 = 60;

impl Ui {
    /// "More systems?" is a step only while the APK offers a system to download; since every core is in the APK
    /// (0.5.x) there is none, and the first start has seven steps.
    fn systems_step_shown(&self) -> bool {
        !self.core_offers.is_empty()
    }
    /// The steps of the first start, in order. With a pad in the hands (a handheld, 18.11.2026) four: the welcome is the
    /// language's page, the controls need no page (the pad's hint bar says them) and the covers come on their own on
    /// unmetered Wi-Fi (Settings has the switch). With touch all of them.
    pub(super) fn setup_steps(&self) -> Vec<usize> {
        if self.controller_connected {
            return vec![1, 2, 3, 7];
        }
        let mut steps = vec![0, 1, 2, 3, 4];
        if self.systems_step_shown() {
            steps.push(5);
        }
        steps.extend([6, 7]);
        steps
    }
    /// Where `step` stands as the player counts the steps, and how many there are (0-based place, count). A step the
    /// list hasn't (a pad came in on the covers step) counts as the next one's place.
    fn setup_place(&self, step: usize) -> (usize, usize) {
        let steps = self.setup_steps();
        (steps.iter().filter(|s| **s < step).count().min(steps.len() - 1), steps.len())
    }
    fn next_step(&self, step: usize) -> Option<usize> {
        self.setup_steps().into_iter().find(|s| *s > step)
    }
    fn previous_step(&self, step: usize) -> Option<usize> {
        self.setup_steps().into_iter().rev().find(|s| *s < step)
    }
    /// A first start left on "More systems?" by an older version goes on with the next step when nothing is offered; one
    /// on a step the pad's way hasn't (the welcome, the covers, the controls) goes on with the next one it has.
    pub fn skip_empty_steps(&mut self) {
        if self.prefs.onboarding_done {
            return;
        }
        if self.prefs.onboarding_step == 5 && !self.systems_step_shown() {
            self.prefs.onboarding_step = 6;
        }
        let step = self.prefs.onboarding_step.min(7);
        if !self.setup_steps().contains(&step) {
            self.prefs.onboarding_step = self.next_step(step).unwrap_or(7);
            self.setup_row = 0;
        }
    }
    /// The games folders found on the device that aren't chosen yet: two at most, the fullest first.
    pub(super) fn found_folders(&self) -> Vec<pocket_ui::android_first_start::FoundFolder> {
        let Some(folders) = self.found.as_ref().and_then(|f| f.folders.clone()) else { return Vec::new() };
        let mut new: Vec<_> = folders
            .into_iter()
            .filter(|f| {
                let id = f.document_uri().and_then(|u| u.rsplit('/').next().map(str::to_string)).unwrap_or_default();
                !self.granted.iter().any(|g| g.eq_ignore_ascii_case(&id))
            })
            .collect();
        new.sort_by_key(|f| std::cmp::Reverse(f.games.unwrap_or(0)));
        new.truncate(2);
        new
    }
    /// The systems with games: the library's and those of the folders found.
    fn found_systems(&self) -> Vec<&'static str> {
        let mut out: Vec<&'static str> = Vec::new();
        for g in self.library.iter().flat_map(|l| l.games.iter()) {
            if let Some((id, ..)) = pocket_ui::document_library::system(g.system_id) {
                if !out.contains(&id) {
                    out.push(id);
                }
            }
        }
        if let Some(folders) = self.found.as_ref().and_then(|f| f.folders.as_ref()) {
            for id in pocket_ui::android_first_start::systems_found(folders) {
                if !out.contains(&id) {
                    out.push(id);
                }
            }
        }
        out
    }
    /// The emulator apps on the device for the systems pocket-ui's player doesn't have, unless already chosen.
    pub(super) fn found_offers(&self) -> Vec<pocket_ui::android_first_start::AppOffer> {
        let Some(phone) = &self.found_phone else { return Vec::new() };
        let chosen: std::collections::BTreeMap<String, String> = self.prefs.plays_with.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        pocket_ui::android_first_start::app_offers(phone, &chosen, &self.found_systems())
    }
    /// The systems with games that no app on the device plays: their app's official page.
    pub(super) fn found_to_get(&self) -> Vec<pocket_ui::android_first_start::AppToGet> {
        let Some(phone) = &self.found_phone else { return Vec::new() };
        pocket_ui::android_first_start::apps_to_get(phone, &self.found_systems())
    }
    /// The BIOS the systems found still want, with the folder said as this view has it (a folder beside the games).
    pub(super) fn found_bios(&self) -> Vec<pocket_ui::android_first_start::BiosNeed> {
        let mut names: Vec<String> = self.found.as_ref().map(|f| f.bios.clone()).unwrap_or_default();
        if let Some(lib) = &self.library {
            names.extend(lib.bios.iter().map(|p| p.name.clone()));
        }
        let kept = super::super::data_dir().join("bios");
        pocket_ui::android_first_start::missing_bios(&self.found_systems(), |s| {
            pocket_ui::android_first_start::bios_among(s, names.iter().map(String::as_str)) || pocket_ui::document_library::bios_hint(s, &[], &kept).is_none()
        })
    }
    /// The line under a system's name: its size where the channel is public, else "Not available here".
    fn core_size_or_soon(&self, p: &pocket_ui::android_cores::CorePackage) -> String {
        if self.core_channel_open { super::core_size(p.package.size) } else { tr("Not available here") }
    }
    fn setup_items(&self) -> Vec<(String, u16)> {
        let mut items = match self.prefs.onboarding_step.min(7) {
            1 => pocket_ui::i18n::Lang::ALL.iter().enumerate().map(|(i, l)| (l.name().to_string(), i as u16)).collect(),
            2 => {
                // Folders found first (the likely answer), then the picker, then the apps found as switches.
                let few = if self.setup_compact >= 3 { 1 } else { 2 };
                let mut rows: Vec<(String, u16)> = self.found_folders().iter().take(few).enumerate().map(|(i, f)| (tr(&f.row()), FOUND_FOLDER + i as u16)).collect();
                rows.push((
                    tr(if self.library.is_some() || !rows.is_empty() {
                        "Choose another folder"
                    } else {
                        "Choose your ROM folder"
                    }),
                    0,
                ));
                // The apps found: one switch for them all (the panel names them), as on the Handheld view.
                let offers = self.found_offers();
                if !offers.is_empty() {
                    let names = pocket_ui::android_first_start::offer_names(&offers);
                    rows.push((format!("{}{} · {}", if self.offers_off { "" } else { "• " }, tr("Emulator apps"), names.join(", ")), FOUND_APP));
                }
                for (i, g) in self.found_to_get().iter().take(few).enumerate() {
                    rows.push((tr(&g.row()), FOUND_GET + i as u16));
                }
                rows
            }
            3 => {
                let mut rows: Vec<_> = Theme::PHONE
                    .iter()
                    .enumerate()
                    .map(|(i, t)| (format!("{}{}", if self.prefs.theme == *t { "• " } else { "" }, tr(t.name())), i as u16))
                    .collect();
                rows.push((
                    format!("{} · {}", tr("Animations"), tr(if self.prefs.cozy.unwrap_or(true) { "Cozy" } else { "Quick" })),
                    ANIMATIONS,
                ));
                rows.push((format!("{} · {}", tr("Appearance"), tr(if self.prefs.light { "Light" } else { "Dark" })), APPEARANCE));
                rows
            }
            4 => vec![
                (
                    format!("{}{}", if self.prefs.download_media == Some(true) { "• " } else { "" }, tr("Download on Wi-Fi")),
                    0,
                ),
                (
                    format!("{}{}", if self.prefs.download_media == Some(false) { "• " } else { "" }, tr("Not now")),
                    1,
                ),
            ],
            5 => self.core_offers.iter().enumerate().map(|(i,p)| (format!("{} · {}",p.package.name, self.core_size_or_soon(p)),i as u16)).collect(),
            _ => vec![],
        };
        items.push((tr("Continue"), NEXT));
        if self.prefs.onboarding_step < 7 {
            items.push((tr("Skip"), SKIP));
        }
        if self.previous_step(self.prefs.onboarding_step).is_some() {
            items.push((tr("Back"), BACK));
        }
        if self.core_busy.is_some() && self.prefs.onboarding_step >= 6 { items.push((tr("Cancel download"), CANCEL_DOWNLOAD)); }
        items
    }
    fn setup_activate(&mut self, index: usize) -> Command {
        let Some((_, action)) = self.setup_items().get(index).cloned() else {
            return Command::None;
        };
        self.setup_row = index;
        let step = self.prefs.onboarding_step.min(7);
        match action {
            CANCEL_DOWNLOAD => return Command::CoreCancel,
            NEXT | SKIP => {
                if step == 4 && self.prefs.download_media.is_none() {
                    self.prefs.download_media = Some(false);
                }
                if step == 5 { self.prefs.core_download_consent = self.core_channel_open && action == NEXT; if action == SKIP { self.prefs.selected_cores.clear(); } }
                // Continue on the games: the apps found play their systems, as Settings → Plays with would set them.
                if step == 2 && action == NEXT {
                    for o in self.found_offers().into_iter().filter(|_| !self.offers_off) {
                        self.prefs.plays_with.insert(o.system.to_string(), o.app.package.clone());
                    }
                }
                match self.next_step(step) {
                    Some(next) => self.prefs.onboarding_step = next,
                    None => {
                        // With a pad the covers had no page: they come on unmetered Wi-Fi, as the step would have offered.
                        if self.prefs.download_media.is_none() {
                            self.prefs.download_media = Some(self.controller_connected);
                        }
                        self.prefs.onboarding_done = true;
                        self.navigate(Screen::Library);
                    }
                }
                self.setup_row = 0;
            }
            BACK => {
                self.prefs.onboarding_step = self.previous_step(step).unwrap_or(step);
                self.setup_row = 0;
            }
            _ => match step {
                1 => {
                    if let Some(lang) = pocket_ui::i18n::Lang::ALL.get(action as usize) {
                        self.prefs.choose_language(*lang);
                    }
                }
                2 if action >= FOUND_GET => {
                    if let Some(g) = self.found_to_get().get((action - FOUND_GET) as usize) {
                        return Command::OpenPage(g.url);
                    }
                    return Command::None;
                }
                2 if action >= FOUND_APP => {
                    self.offers_off = !self.offers_off;
                    return Command::None;
                }
                2 if action >= FOUND_FOLDER => {
                    if let Some(uri) = self.found_folders().get((action - FOUND_FOLDER) as usize).and_then(|f| f.document_uri()) {
                        return Command::FolderAt(uri);
                    }
                    return Command::Folder;
                }
                2 => return Command::Folder,
                3 => {
                    match action {
                        ANIMATIONS => self.prefs.cozy = Some(!self.prefs.cozy.unwrap_or(true)),
                        APPEARANCE => self.prefs.light = !self.prefs.light,
                        _ => if let Some(theme) = Theme::PHONE.get(action as usize) { self.prefs.theme = *theme; },
                    }
                    self.cover_changed();
                }
                4 => self.prefs.download_media = Some(action == 0),
                5 => if let Some(p) = self.core_offers.get(action as usize).filter(|_| self.core_channel_open) {
                    if !self.prefs.selected_cores.remove(&p.core) { self.prefs.selected_cores.insert(p.core.clone()); }
                },
                _ => (),
            },
        }
        Command::PersistSetup
    }
    pub(super) fn setup_press(&mut self, button: Button) -> Command {
        self.skip_empty_steps();
        let items = self.setup_items();
        if button == Button::South && self.core_busy.is_some() { return Command::CoreCancel; }
        match button {
            Button::Up | Button::Left => self.setup_row = self.setup_row.saturating_sub(1),
            Button::Down | Button::Right => self.setup_row = (self.setup_row + 1).min(items.len() - 1),
            Button::East => return self.setup_activate(self.setup_row),
            Button::South => {
                if let Some(i) = items.iter().position(|(_, a)| *a == BACK) {
                    return self.setup_activate(i);
                }
            }
            Button::Start => {
                if let Some(i) = items.iter().position(|(_, a)| *a == SKIP) {
                    return self.setup_activate(i);
                }
            }
            _ => (),
        }
        Command::None
    }
    pub(super) fn setup_tap(&mut self, x: f32, y: f32) -> Command {
        self.skip_empty_steps();
        let index = self.regions.iter().rev().find_map(|r| {
            if r.area.contains(x, y) {
                if let Action::Setup(i) = r.action {
                    Some(i)
                } else {
                    None
                }
            } else {
                None
            }
        });
        index.map_or(Command::None, |i| self.setup_activate(i))
    }
    /// Whether choice `action` of this step is the one in use (a radio mark), or `None` for a row that acts.
    fn setup_chosen(&self, action: u16) -> Option<bool> {
        match self.prefs.onboarding_step.min(7) {
            1 => Some(pocket_ui::i18n::Lang::ALL.get(action as usize) == Some(&pocket_ui::i18n::current())),
            3 if (action as usize) < Theme::PHONE.len() => Some(Theme::PHONE.get(action as usize) == Some(&self.prefs.theme)),
            4 => Some(self.prefs.download_media == Some(action == 0)),
            2 if action >= FOUND_GET => None,
            2 if action >= FOUND_APP => Some(!self.offers_off),
            5 if !self.core_channel_open => None,
            5 => self.core_offers.get(action as usize).map(|p| self.prefs.selected_cores.contains(&p.core)),
            _ => None,
        }
    }
    /// The page's title and sentence for `step` (as keys of the translation table).
    fn setup_words(&self, step: usize) -> (&'static str, &'static str) {
        match step {
            0 => ("Welcome to pocket-ui", "Your games, a shelf of your own, and a player built in. Let's make it yours."),
            // With a pad the language's page is the welcome (`setup_steps`).
            1 if self.controller_connected => ("Welcome to pocket-ui", "Choose the language for menus and help. You can change it in Settings."),
            1 => ("Language", "Choose the language for menus and help. You can change it in Settings."),
            2 => ("Where your games are", "Choose your ROM folders. Android remembers access; your game files stay where they are."),
            3 => ("Your look", "Pick a look. Cozy takes time to open the case; Quick gets straight into the game."),
            4 => ("Covers and pictures", "Covers and pictures download on unmetered Wi-Fi as you browse. Your choice stays in Settings."),
            5 => ("More systems?", if self.core_channel_open { "Choose extra systems to download on Wi-Fi. All are optional. You can add them later in Extras." } else { COMING_SOON_STEP }),
            6 => ("Your controls", "Tap a game to select it, tap again to play. Swipe along the shelf. Back goes one step back."),
            // With a pad the covers had no page: the last one says they come on their own.
            _ if self.controller_connected && self.prefs.download_media.is_none() => ("Ready to play", "Everything is ready. Covers and pictures download on unmetered Wi-Fi as you browse; Settings can turn that off."),
            _ => ("Ready to play", "Everything is ready. You can change these choices in Settings and add more game folders later."),
        }
    }

    /// The plan the page is laid out in on a `size` screen: the usual one while it fits, else the first of the ladder
    /// (`SetupPlan::ladder`) that does, else the one with the fewest faults. Measured, not painted.
    fn pick_setup_plan(&mut self, text: &mut Text, size: (i32, i32), base_dp: f32) -> SetupPlan {
        self.setup_compact = 0;
        if size.0 <= size.1 {
            // Upright, "Where your games are" with what the device has can be more than a small phone holds at a large font
            // (720x1600 with its bars, font 1.3): it leaves out, step by step, what the rows say anyway.
            if self.prefs.onboarding_step != 2 {
                return SetupPlan::USUAL;
            }
            for level in 0..=3 {
                self.setup_compact = level;
                if self.setup_layout(text, size, SetupPlan::USUAL).faults.is_empty() {
                    return SetupPlan::USUAL;
                }
            }
            // Still too much: the smaller type of the ladder below.
        }
        // "Where your games are" with what the device has may need to leave things out on a small window too (a square one
        // is wider than tall once Android's bars are off it): each plan of the ladder at each compact level, the first that fits.
        let levels = if self.prefs.onboarding_step == 2 { 0..=3 } else { 0..=0 };
        let mut best = (usize::MAX, SetupPlan::USUAL, 0);
        for level in levels {
            self.setup_compact = level;
            for plan in SetupPlan::ladder() {
                self.dp = base_dp * plan.shrink;
                let faults = if self.prefs.onboarding_step == 5 { self.system_layout(text, size, plan).faults.len() } else { self.setup_layout(text, size, plan).faults.len() };
                if faults == 0 {
                    self.dp = base_dp;
                    return plan;
                }
                if faults < best.0 {
                    best = (faults, plan, level);
                }
            }
        }
        self.dp = base_dp;
        self.setup_compact = best.2;
        best.1
    }

    /// Where everything of a step's page stands for `plan` on a `w` x `h` canvas, and what is wrong with it (a word torn,
    /// text on text or off the canvas, a button's word cut, no room for the choices): nothing is painted here, so the
    /// plans can be tried one after the other.
    fn setup_layout(&mut self, text: &mut Text, (w, h): (i32, i32), plan: SetupPlan) -> SetupLayout {
        let step = self.prefs.onboarding_step.min(7);
        let wide = w > h;
        let margin = self.d(if !wide { 24.0 } else if plan.tight { 16.0 } else { 32.0 });
        let col_gap = self.d(if plan.tight { 16.0 } else { 32.0 });
        let body_w = if wide { (w - 2 * margin) * plan.share / 100 } else { w - 2 * margin };
        let (title_key, description_key) = self.setup_words(step);
        let (title, description) = (tr(title_key), tr(description_key));
        let items = self.setup_items();
        let mut l = SetupLayout::new(plan, items.clone());
        // The step indicator: a segment a step (eight, or seven without "More systems?"), the ones done in Chalk, and
        // "Step n of 8" beside the Skip button when both fit,
        // else under it (a narrow column, large type). In a column too narrow for segments beside Skip, the segments take
        // the column's whole width and Skip and the words go under them.
        let seg_gap = self.d(6.0);
        let skip_w = items.iter().find(|(_, a)| *a == SKIP).map_or(0, |(lb, _)| self.d(88.0).max(text.width(Face::Title, self.sp(16.0), lb) + self.d(24.0)));
        let bar_y = self.d(20.0);
        let small = self.sp(14.0);
        let target = self.d(48.0);
        let (place, total) = self.setup_place(step);
        let progress = tr(&format!("Step {} of {total}", place + 1));
        let total = total as i32;
        let stacked = wide && (body_w - skip_w - self.d(16.0) - (total - 1) * seg_gap) / total < self.d(3.0);
        let bar_w = if stacked { body_w } else { body_w - skip_w - self.d(16.0) };
        let seg = (bar_w - (total - 1) * seg_gap) / total;
        l.bar = (margin, bar_y, seg, seg_gap, self.d(4.0), place as i32);
        let beside = margin + text.width(Face::Body, small, &progress) + self.d(8.0) <= margin + body_w - skip_w;
        let skip_y = if stacked { bar_y + self.d(10.0) } else { bar_y - self.d(12.0) };
        let progress_y = match (stacked, beside) {
            (false, true) => bar_y + self.d(30.0),
            (true, true) => skip_y + target / 2 + self.d(5.0),
            _ => skip_y + target + self.d(4.0) + small as i32,
        };
        l.put(Face::Body, small, margin, progress_y, pal().dust, &progress);
        l.label(Area::new(margin, progress_y - self.d(38.0), bar_w, self.d(44.0)), progress);
        if let Some((i, (label, _))) = items.iter().enumerate().find(|(_, (_, a))| *a == SKIP) {
            let area = Area::new(margin + body_w - skip_w, skip_y, skip_w, target);
            // Skip is a quiet text button: its 48 dp target is not drawn, only its word is.
            let (sp16, word_w) = (self.sp(16.0), text.width(Face::Title, self.sp(16.0), label));
            let size = pocket_ui::type_scale::px(sp16) as i32;
            let base = area.y + area.h / 2 + phone::type_mid(sp16);
            l.skip_ink = Some(Area::new(area.x + (area.w - word_w) / 2, base - size * 7 / 10, word_w, size * 8 / 10));
            l.skip = Some((i, label.clone(), area));
            l.ops.push(Op::Skip);
        }
        let mut y = progress_y + self.d(if wide { 30.0 } else { 42.0 });
        // Lines as far apart as the type is really painted (Android's font size grows it), a long welcome on three; the
        // title as large as it can be with every word whole (a narrow column).
        let ts = text.title_px(Face::Title, self.sp(if wide { 26.0 } else { 30.0 }), title_key, body_w, self.sp(if wide { 18.0 } else { 30.0 }));
        let title_px = pocket_ui::type_scale::px(ts) as i32;
        let title_top = y - title_px;
        if !text.words_fit(Face::Title, ts, title_key, body_w) || text.wrap(Face::Title, ts, &title, body_w).len() > 3 {
            l.faults.push("the title does not fit its column".into());
        }
        for line in text.wrap_at_most(Face::Title, ts, &title, body_w, 3) {
            l.put(Face::Title, ts, margin, y, pal().chalk, &line);
            y += title_px + self.d(4.0);
        }
        l.label(Area::new(margin, title_top, body_w, y - title_top), title);
        y += self.d(4.0);
        let bs = self.sp(if wide { 15.0 } else { 16.0 });
        let line_h = bs as i32 + self.d(8.0);
        if !text.words_fit(Face::Body, bs, description_key, body_w) {
            l.faults.push("a word of the sentence does not fit its column".into());
        }
        let sentence = !(step == 2 && self.setup_compact >= 1);
        for line in text.wrap(Face::Body, bs, &description, body_w).into_iter().filter(|_| sentence) {
            l.put(Face::Body, bs, margin, y, pal().dust, &line);
            y += line_h;
        }
        y += self.d(8.0);
        // Back and Continue: at the bottom of the text column (across both columns in the bar plan), Continue the primary
        // one and the wider.
        // Upright (a phone, its first start made for it): large buttons for a thumb.
        let bh = self.d(if wide { 48.0 } else { 60.0 });
        let buttons_y = h - bh - self.d(16.0);
        let back = items.iter().enumerate().find(|(_, (_, a))| *a == BACK).map(|(i, (lb, _))| (i, lb.clone()));
        let next = items.iter().enumerate().find(|(_, (_, a))| *a == NEXT).map(|(i, (lb, _))| (i, lb.clone()));
        let gap = self.d(12.0);
        let row_w = if plan.bar { w - 2 * margin } else { body_w };
        // Back a third of the column, wider when its word needs it ("Zurück" on a small landscape screen).
        let back_w = back.as_ref().map_or(0, |(_, lb)| ((row_w - gap) / 3).max(text.width(Face::Title, self.sp(17.0), lb) + self.d(32.0)).min((row_w - gap) / 2));
        if let Some((i, lb)) = back {
            l.back = Some((i, lb, Area::new(margin, buttons_y, back_w, bh)));
            l.ops.push(Op::Back);
        }
        if let Some((i, lb)) = next {
            let x = margin + if back_w > 0 { back_w + gap } else { 0 };
            l.next = Some((i, lb, Area::new(x, buttons_y, margin + row_w - x, bh)));
            l.ops.push(Op::Next);
        }
        // A button shows its whole word at least in its smallest size.
        for (_, lb, area) in [&l.back, &l.next].into_iter().flatten() {
            if text.width(Face::Title, self.sp(13.0), lb) + self.d(24.0) > area.w {
                l.faults.push(format!("a button's word does not fit: {lb:?} needs {} has {}", text.width(Face::Title, self.sp(13.0), lb) + self.d(24.0), area.w));
            }
        }
        // What the step shows and asks: under the words in portrait, in the right-hand column in landscape.
        let (rx, rw, mut ry, r_bottom) = if wide {
            let x = margin + body_w + col_gap;
            (x, w - x - margin, self.d(20.0), if plan.bar { buttons_y - self.d(12.0) } else { h - self.d(16.0) })
        } else {
            (margin, body_w, y, buttons_y - self.d(16.0))
        };
        if rw < self.d(60.0) {
            l.faults.push("no room for the right-hand column".into());
        }
        if step == 2 {
            let status = if self.scan_running {
                tr("Reading library...")
            } else if let Some(lib) = &self.library {
                let systems: HashSet<_> = lib.games.iter().map(|g| g.system_id).collect();
                let found = tr(&format!("Found {} games on {} systems", lib.games.len(), systems.len()));
                if lib.games.is_empty() {
                    format!("{found}. {}", tr("Choose the folder that contains your system folders, or a system folder such as psp or gba."))
                } else {
                    found
                }
            } else if !self.found_folders().is_empty() {
                tr("Games found on this device. Choose them below, or another folder.")
            } else if self.found.as_ref().is_some_and(|f| f.can_read) {
                // Only where the files could be read: without that right "none found" would be a guess.
                tr("No games found on this device yet. Choose the folder you put them in, or add games later.")
            } else {
                tr("No folder chosen yet. You can add games later.")
            };
            // Compact (a small phone, large font): the found folders' rows say it, the line above them goes.
            let quiet = self.setup_compact >= 2 && self.library.is_none() && !self.found_folders().is_empty();
            let panel_lines = if quiet { Vec::new() } else { text.wrap(Face::Body, bs, &status, rw - self.d(32.0)) };
            let ph = if quiet { 0 } else { panel_lines.len() as i32 * line_h + self.d(24.0) };
            if !quiet {
                l.ops.push(Op::Panel(Area::new(rx, ry, rw, ph)));
            }
            let mut ly = ry + self.d(12.0) + bs as i32;
            for line in &panel_lines {
                l.put(Face::Body, bs, rx + self.d(16.0), ly, pal().chalk, line);
                ly += line_h;
            }
            if !quiet {
                l.label(Area::new(rx, ry, rw, ph), status);
                ry += ph + self.d(12.0);
            }
            if !self.message.is_empty() && !self.scan_running {
                let message = text.fit(Face::Body, small, &self.message, rw);
                l.put(Face::Body, small, rx, ry + small as i32, pal().alert, &message);
                ry += line_h;
            }
        }
        if step == 6 {
            let status = tr(if self.controller_connected { "Controller detected" } else { "Touch" });
            let ss = self.sp(22.0);
            l.put(Face::Title, ss, rx, ry + ss as i32, pal().chalk, &status);
            l.label(Area::new(rx, ry, rw, ss as i32 + self.d(8.0)), status);
            ry += ss as i32 + self.d(16.0);
            for line in text.wrap(Face::Body, small, &tr("Controller: D-pad moves, A chooses, B goes back, X opens the case, START opens Settings."), rw) {
                l.put(Face::Body, small, rx, ry + small as i32, pal().dust, &line);
                ry += small as i32 + self.d(8.0);
            }
            ry += self.d(8.0);
        }
        if step == 4 {
            let label = tr(if self.wifi_ready { "Wi-Fi ready" } else { "Downloads wait for unmetered Wi-Fi." });
            for line in text.wrap(Face::Body, small, &label, rw) {
                l.put(Face::Body, small, rx, ry + small as i32, pal().dust, &line);
                ry += small as i32 + self.d(8.0);
            }
            ry += self.d(8.0);
        }
        // The last page: a BIOS a system found still wants, one sentence each with its folder (Saturn's first, it is
        // needed), before the first game asks for it. The step with the games has no room for them upright.
        if step == 7 {
            for b in self.found_bios() {
                let line = tr(b.line(true));
                let lines = text.wrap(Face::Body, small, &line, rw);
                let top = ry;
                for l2 in &lines {
                    l.put(Face::Body, small, rx, ry + small as i32, if b.required { pal().alert } else { pal().dust }, l2);
                    ry += small as i32 + self.d(8.0);
                }
                l.label(Area::new(rx, top, rw, ry - top), line);
                ry += self.d(8.0);
            }
        }
        if step >= 6 && !self.core_note.is_empty() {
            for line in text.wrap(Face::Body, small, &self.core_note, rw) {
                l.put(Face::Body, small, rx, ry + small as i32, pal().dust, &line);
                ry += small as i32 + self.d(8.0);
            }
            l.label(Area::new(rx, ry - small as i32 - self.d(8.0), rw, small as i32 + self.d(8.0)), self.core_note.clone());
            ry += self.d(8.0);
        }
        l.right = (rx, rw, ry, r_bottom);
        // The choices: rows with a radio mark for a setting, a pill for an action. The looks go two to a row so their
        // preview keeps room; Animations and Appearance share the last row.
        let choices: Vec<(usize, String, u16)> = items.iter().enumerate().filter(|(_, (_, a))| *a < NEXT).map(|(i, (lb, a))| (i, lb.clone(), *a)).collect();
        let col_gap = self.d(10.0);
        let half = (rw - col_gap) / 2;
        // What each choice needs on one line, in the usual size and in the smallest: a radio row its mark and words,
        // an action its words.
        let label_px = self.sp(17.0);
        let smallest = self.sp(13.0);
        let radio_extra = self.d(60.0);
        let needs: Vec<(i32, i32)> = choices
            .iter()
            .map(|(_, lb, a)| {
                let (face, extra, lb) = match self.setup_chosen(*a) {
                    Some(_) => (Face::Body, radio_extra, lb.trim_start_matches("• ")),
                    None => (Face::Title, self.d(24.0), lb.as_str()),
                };
                (text.width(face, label_px, lb) + extra, text.width(face, smallest, lb) + extra)
            })
            .collect();
        // Each choice's (column, row, width). The looks two to a row, Animations and Appearance side by side (landscape)
        // when their words fit there; a Retroid's small landscape column often has no room for both.
        let looks = Theme::PHONE.len() as i32;
        let cells_of = |looks_half: bool, extras_half: bool| -> Vec<(i32, i32, i32)> {
            let look_rows = if looks_half { (looks + 1) / 2 } else { looks };
            (0..choices.len() as i32)
                .map(|n| match step {
                    3 if n < looks && looks_half => (n % 2, n / 2, half),
                    3 if n < looks => (0, n, rw),
                    3 if extras_half => (n - looks, look_rows, half),
                    3 => (0, look_rows + n - looks, rw),
                    // Six languages are taller than a small landscape screen's column: two to a row when one to a row has no room.
                    1 if looks_half => (n % 2, n / 2, half),
                    _ => (0, n, rw),
                })
                .collect()
        };
        let arrangements: Vec<Vec<(i32, i32, i32)>> = match step {
            3 if wide => vec![cells_of(true, true), cells_of(true, false), cells_of(false, false)],
            3 => vec![cells_of(true, false), cells_of(false, false)],
            1 => vec![cells_of(false, false), cells_of(true, false)],
            _ => vec![cells_of(false, false)],
        };
        let rows = |cells: &[(i32, i32, i32)]| cells.iter().map(|(_, r, _)| r + 1).max().unwrap_or(0);
        let base_pitch = self.d(if wide { 52.0 } else { 64.0 });
        let line_px = pocket_ui::type_scale::px(label_px) as i32 * 6 / 5;
        let room = r_bottom - ry;
        // The most compact arrangement with every word whole in the usual size, else a little smaller; else one to a row
        // with two lines where needed (taller rows, every word still whole); else the same with the rows closer together.
        let mut chosen = None;
        for pitch in [base_pitch, (base_pitch as f32 * 0.88) as i32, (base_pitch as f32 * 0.8) as i32] {
            let fitting = |tier: usize| {
                arrangements.iter().find(|cells| rows(cells) * pitch <= room && needs.iter().zip(cells.iter()).all(|(n, (_, _, cw))| if tier == 0 { n.0 <= *cw } else { n.1 <= *cw }))
            };
            if let Some(cells) = fitting(0).or_else(|| fitting(1)) {
                chosen = Some((cells.clone(), pitch));
                break;
            }
            let last = arrangements.last().cloned().unwrap_or_default();
            let tall = pitch.max(2 * line_px + self.d(20.0));
            // Two lines: a setting's words whole on two lines, an action's word whole in its smallest size.
            let two_lines = choices.iter().zip(needs.iter()).zip(last.iter()).all(|(((_, lb, a), n), (_, _, cw))| match self.setup_chosen(*a) {
                Some(_) => {
                    let lb = lb.trim_start_matches("• ");
                    text.words_fit(Face::Body, label_px, lb, cw - radio_extra) && text.wrap(Face::Body, label_px, lb, cw - radio_extra).len() <= 2
                }
                None => n.1 <= *cw,
            });
            if rows(&last) * tall <= room && two_lines {
                chosen = Some((last, tall));
                break;
            }
        }
        let (cells, pitch) = chosen.clone().unwrap_or_else(|| (arrangements[0].clone(), base_pitch));
        if chosen.is_none() && !choices.is_empty() {
            l.faults.push(format!("the choices do not fit the right-hand column: rw {rw} room {room} needs {needs:?} cells {:?}", arrangements.iter().map(|a| a.iter().map(|c| c.2).collect::<Vec<_>>()).collect::<Vec<_>>()));
        }
        let list_h = cells.iter().map(|(_, r, _)| r + 1).max().unwrap_or(0) * pitch;
        // The look's preview goes above its choices; everything else's picture below them.
        let preview_h = if step == 3 { (r_bottom - ry - list_h - self.d(12.0)).min(self.d(220.0)) } else { 0 };
        let top = (ry + if step == 3 && preview_h > self.d(60.0) { preview_h + self.d(12.0) } else { 0 }).min(r_bottom - list_h);
        l.choices = (choices, cells, pitch, list_h, preview_h);
        l.top = top;
        // Every word whole, nothing on anything else, all of it on the canvas.
        for (i, (_, lb, a)) in l.choices.0.iter().enumerate() {
            let (_, row, cw) = l.choices.1[i];
            let col = l.choices.1[i].0;
            let area = Area::new(rx + col * (half + col_gap), top + row * pitch, cw, pitch - self.d(8.0));
            let _ = (lb, a);
            l.areas.push(area);
        }
        l.check(text, (w, h));
        l
    }

    pub(super) fn paint_setup(&mut self, c: &mut Canvas, text: &mut Text) {
        self.skip_empty_steps();
        let base_dp = self.dp;
        let plan = self.pick_setup_plan(text, (c.w, c.h), base_dp);
        self.dp = base_dp * plan.shrink;
        if self.prefs.onboarding_step == 5 {
            let layout = self.system_layout(text, (c.w, c.h), plan);
            self.paint_system_setup(c, text, layout);
        } else {
            let layout = self.setup_layout(text, (c.w, c.h), plan);
            self.setup_faults = layout.faults.clone();
            self.paint_setup_layout(c, text, layout);
        }
        self.dp = base_dp;
    }

    fn paint_setup_layout(&mut self, c: &mut Canvas, text: &mut Text, l: SetupLayout) {
        let step = self.prefs.onboarding_step.min(7);
        let hints = self.controller_hints();
        let (bx, by, seg, seg_gap, bar_h, done) = l.bar;
        for i in 0..self.setup_place(step).1 as i32 {
            let color = if i <= done { pal().chalk } else { pal().pill };
            c.round_rect(bx + i * (seg + seg_gap), by, seg, bar_h, bar_h / 2, color);
        }
        for op in &l.ops {
            match op {
                Op::Put(p) => {
                    text.draw(c, p.face, p.px, p.x, p.y, p.color, &p.s);
                }
                Op::Label(area, name) => self.region(*area, name.clone(), Action::Label),
                Op::Panel(area) => kit::panel(c, area.x, area.y, area.w, area.h),
                Op::Skip => {
                    let Some((i, label, area)) = &l.skip else { continue };
                    // Skip: a quiet text button at the top right, a whole 48 dp target.
                    let on = hints && *i == self.setup_row;
                    if on {
                        c.round_rect(area.x, area.y, area.w, area.h, area.h / 2, pal().pill);
                        // The pill alone is as dark as the page: the pad's focus is the frame Continue and Back have.
                        c.frame(area.x, area.y, area.w, area.h, self.d(2.0), pal().chalk);
                    }
                    let sw = text.width(Face::Title, self.sp(16.0), label);
                    text.draw(c, Face::Title, self.sp(16.0), area.x + (area.w - sw) / 2, area.y + area.h / 2 + phone::type_mid(self.sp(16.0)), pal().chalk, label);
                    self.region(*area, label.clone(), Action::Setup(*i));
                }
                Op::Back => {
                    if let Some((i, label, area)) = &l.back {
                        self.setup_button(c, text, *area, label, false, *i);
                    }
                }
                Op::Next => {
                    if let Some((i, label, area)) = &l.next {
                        self.setup_button(c, text, *area, label, true, *i);
                    }
                }
            }
        }
        let (rx, rw, ry, r_bottom) = l.right;
        let (choices, _, _, list_h, preview_h) = &l.choices;
        let (list_h, preview_h) = (*list_h, *preview_h);
        let label_px = self.sp(17.0);
        let line_px = pocket_ui::type_scale::px(label_px) as i32 * 6 / 5;
        if step == 3 && preview_h > self.d(60.0) {
            let (w, h) = (rw, preview_h);
            let key = (self.prefs.theme, self.prefs.light, w, h);
            if self.setup_preview.as_ref().is_none_or(|(old, _)| *old != key) {
                self.setup_preview = Some((key, look_preview(text, self.prefs.theme, w, h, self.dp)));
            }
            if let Some((_, pic)) = &self.setup_preview {
                art::blit(c, pic, rx + (w - pic.w) / 2, ry, 256, (0, c.h));
            }
        }
        let top = l.top;
        for (n, (i, label, action)) in choices.iter().enumerate() {
            let area = l.areas[n];
            let on = hints && *i == self.setup_row;
            // The mark replaces the "• " on screen; the accessibility name keeps it (TalkBack says which is chosen).
            let spoken = label.clone();
            let label = label.trim_start_matches("• ").to_string();
            match self.setup_chosen(*action) {
                Some(chosen) => {
                    let bg = if chosen { pal().label } else { pal().spine };
                    let fg = if chosen { pal().ink } else { pal().chalk };
                    c.round_rect(area.x, area.y, area.w, area.h, self.d(12.0), bg);
                    if on {
                        c.frame(area.x - 3, area.y - 3, area.w + 6, area.h + 6, self.d(2.0), pal().chalk);
                    }
                    let r = self.d(20.0);
                    self.radio(c, area.x + self.d(14.0) + r / 2, area.y + area.h / 2, r, chosen, fg);
                    let width = area.w - r - self.d(40.0);
                    let (s, _) = text.fit_sizes(Face::Body, &[label_px, self.sp(15.0), self.sp(13.0)], &label, width);
                    let s = if text.width(Face::Body, s, &label) <= width { s } else { label_px };
                    let lines = text.wrap_at_most(Face::Body, s, &label, width, 2);
                    let first = area.y + area.h / 2 + phone::type_mid(s) - (lines.len() as i32 - 1) * line_px / 2;
                    for (k, line) in lines.iter().enumerate() {
                        text.draw(c, Face::Body, s, area.x + r + self.d(26.0), first + k as i32 * line_px, fg, line);
                    }
                    self.region(area, spoken, Action::Setup(*i));
                }
                None => self.setup_button(c, text, area, &label, false, *i),
            }
        }
        // The same handmade art as the shelf's empty pages, as large as the free room allows.
        let after = top + list_h + self.d(12.0);
        let (ax, mut ay, aw, mut ah) = (rx, after, rw, r_bottom - after);
        if step == 0 && ah > self.d(120.0) {
            // pocket-ui's mark above the shelf, in whole pixels (it is pixel art).
            if let Some(logo) = pocket_ui::ui_art::get(pocket_ui::ui_art::Art::Logo) {
                let k = ((ah * 2 / 5).min(self.d(150.0)) / logo.h as i32).max(1);
                let (lw, lh) = (logo.w as i32 * k, logo.h as i32 * k);
                let top = ay + self.d(8.0);
                pocket_ui::ui_art::paint(c, pocket_ui::ui_art::Art::Logo, ax + (aw - lw) / 2, top, k);
                self.region(Area::new(ax + (aw - lw) / 2, top, lw, lh), "pocket-ui".into(), Action::Label);
                ay = top + lh + self.d(16.0);
                ah = r_bottom - ay;
            }
        }
        if step != 3 && aw > self.d(80.0) && ah > self.d(80.0) {
            let picture = if step == 4 {
                pocket_ui::ui_art::Art::Screenshots
            } else {
                pocket_ui::ui_art::Art::NoGames
            };
            if let Some(sprite) = pocket_ui::ui_art::get(picture) {
                let fitted = sprite.fitted(aw.min(self.d(320.0)), ah.min(self.d(300.0)));
                c.blit_sprite(&fitted, ax + (aw - fitted.w as i32) / 2, ay + (ah - fitted.h as i32) / 2, 0);
            }
        }
    }

    /// The "More systems?" step's page (`setup_layout` for the step with its own two kinds of button and its two-line rows).
    fn system_layout(&mut self, text: &mut Text, (w, h): (i32, i32), plan: SetupPlan) -> SetupLayout {
        let px = |size| pocket_ui::type_scale::px(size) as i32;
        let wide = w > h;
        let margin = self.d(if !wide { 24.0 } else if plan.tight { 16.0 } else { 32.0 });
        let col_gap = self.d(if plan.tight { 16.0 } else { 32.0 });
        let left = if wide { (w - 2 * margin) * plan.share / 100 } else { w - 2 * margin };
        let small = self.sp(12.0);
        let title = self.sp(22.0);
        let body = self.sp(14.0);
        let mut l = SetupLayout::new(plan, self.setup_items());
        let mut y = self.d(20.0);
        let gap = self.d(6.0);
        l.bar = (margin, y, (left - 7 * gap) / 8, gap, self.d(4.0), 5);
        y += self.d(12.0) + px(small);
        let progress = tr("Step 6 of 8");
        l.put(Face::Body, small, margin, y, pal().dust, &progress);
        l.label(Area::new(margin, y - px(small), left, px(small)), progress);
        y += self.d(16.0);
        let (title_key, words_key) = self.setup_words(5);
        if !text.words_fit(Face::Title, title, title_key, left) {
            l.faults.push("the title does not fit its column".into());
        }
        for line in text.wrap(Face::Title, title, &tr(title_key), left) {
            y += px(title);
            l.put(Face::Title, title, margin, y, pal().chalk, &line);
        }
        l.label(Area::new(margin, y - px(title), left, px(title)), tr(title_key));
        y += self.d(8.0);
        let step_words = tr(words_key);
        let words_top = y;
        if !text.words_fit(Face::Body, body, words_key, left) {
            l.faults.push("a word of the sentence does not fit its column".into());
        }
        for line in text.wrap(Face::Body, body, &step_words, left) {
            y += px(body);
            l.put(Face::Body, body, margin, y, pal().dust, &line);
            y += self.d(6.0);
        }
        // Read out whole, like the title above it.
        l.label(Area::new(margin, words_top, left, y - words_top), step_words);
        y += self.d(12.0);

        let primary_h = self.d(52.0).max(px(self.sp(17.0)) + self.d(24.0));
        let secondary_h = self.d(48.0).max(px(body) + self.d(24.0));
        let bottom = h - self.d(16.0);
        let items = l.items.clone();
        let word = |text: &mut Text, s: &str| text.width(Face::Title, body, s) + self.d(32.0);
        let (mut back, mut skip, mut next) = (None, None, None);
        let primary_y = bottom - primary_h;
        let secondary_y = primary_y - self.d(8.0) - secondary_h;
        let row_gap = self.d(12.0);
        let words_bottom;
        if plan.bar {
            // One row across both columns: Back, Skip, Continue the rest.
            let (mut x, row_w) = (margin, w - 2 * margin);
            let find = |a: u16| items.iter().enumerate().find(|(_, (_, b))| *b == a).map(|(i, (lb, _))| (i, lb.clone()));
            if let Some((i, lb)) = find(BACK) {
                let bw = word(text, &lb);
                back = Some((i, lb, Area::new(x, primary_y, bw, primary_h)));
                x += bw + row_gap;
            }
            if let Some((i, lb)) = find(SKIP) {
                let bw = word(text, &lb);
                skip = Some((i, lb, Area::new(x, primary_y, bw, primary_h)));
                x += bw + row_gap;
            }
            if let Some((i, lb)) = find(NEXT) {
                next = Some((i, lb, Area::new(x, primary_y, margin + row_w - x, primary_h)));
            }
            words_bottom = primary_y;
        } else {
            let back_w = (text.width(Face::Title, body, &tr("Back")) + self.d(32.0)).max(left / 3).min(left / 2);
            for (i, (label, action)) in items.iter().enumerate().filter(|(_, (_, a))| *a >= NEXT) {
                match *action {
                    NEXT => next = Some((i, label.clone(), Area::new(margin, primary_y, left, primary_h))),
                    BACK => back = Some((i, label.clone(), Area::new(margin, secondary_y, back_w, secondary_h))),
                    SKIP => skip = Some((i, label.clone(), Area::new(margin + back_w + row_gap, secondary_y, left - back_w - row_gap, secondary_h))),
                    _ => (),
                }
            }
            words_bottom = secondary_y;
        }
        // The two quiet buttons are painted with their words as they are: a word wider than its button is a fault.
        for (_, label, area) in [&back, &skip].into_iter().flatten() {
            if text.width(Face::Title, body, label) > area.w - self.d(16.0) {
                l.faults.push("a quiet button's word is cut".into());
            }
        }
        let top = self.d(16.0);
        let (x, width, end) = if wide {
            let x = margin + left + col_gap;
            (x, w - x - margin, if plan.bar { primary_y - self.d(12.0) } else { bottom })
        } else {
            (margin, left, secondary_y - self.d(12.0))
        };
        let top = if wide { top } else { y };
        let name_size = self.sp(16.0);
        let size_size = self.sp(13.0);
        let pitch = self.d(56.0).max(px(name_size) + px(size_size) + self.d(24.0));
        let open = self.core_channel_open;
        let radio = if open { self.d(20.0) } else { 0 };
        let room = width - radio - self.d(if open { 44.0 } else { 32.0 });
        for (i, p) in self.core_offers.clone().iter().enumerate() {
            l.areas.push(Area::new(x, top + i as i32 * pitch, width, pitch - self.d(6.0)));
            let size = self.core_size_or_soon(p);
            if text.width(Face::Body, name_size, &p.package.name) > room || text.width(Face::Body, size_size, &size) > room {
                l.faults.push("a system's name or size is cut".into());
            }
        }
        if top + self.core_offers.len() as i32 * pitch > end {
            l.faults.push("the systems do not fit the right-hand column".into());
        }
        l.right = (x, width, top + self.core_offers.len() as i32 * pitch, end);
        // Continue, Skip, Back: the order of the page's rows.
        for (op, present) in [(Op::Next, next.is_some()), (Op::Skip, skip.is_some()), (Op::Back, back.is_some())] {
            if present {
                l.ops.push(op);
            }
        }
        l.skip = skip;
        l.back = back;
        l.next = next;
        let _ = words_bottom;
        l.check(text, (w, h));
        l
    }

    /// Download consent has several rows with two sizes of type. Measure painted pixels, including Android's
    /// font scale; dp alone would put the system's name on top of its download size at 200%.
    fn paint_system_setup(&mut self, c: &mut Canvas, text: &mut Text, l: SetupLayout) {
        let px = |size| pocket_ui::type_scale::px(size) as i32;
        let hints = self.controller_hints();
        let body = self.sp(14.0);
        let (bx, by, seg, gap, bar_h, done) = l.bar;
        for i in 0..8 {
            c.round_rect(bx + i * (seg + gap), by, seg, bar_h, self.d(2.0), if i <= done { pal().chalk } else { pal().pill });
        }
        for op in &l.ops {
            match op {
                Op::Put(p) => {
                    text.draw(c, p.face, p.px, p.x, p.y, p.color, &p.s);
                }
                Op::Label(area, name) => self.region(*area, name.clone(), Action::Label),
                Op::Next => {
                    if let Some((i, label, area)) = &l.next {
                        self.setup_button(c, text, *area, label, true, *i);
                    }
                }
                Op::Skip | Op::Back => {
                    let found = if matches!(op, Op::Back) { &l.back } else { &l.skip };
                    let Some((i, label, area)) = found else { continue };
                    let on = hints && self.setup_row == *i;
                    if on { c.round_rect(area.x, area.y, area.w, area.h, self.d(12.0), pal().pill); }
                    let shown = text.fit(Face::Title, body, label, area.w - self.d(16.0));
                    if shown != *label { self.cut.push(label.clone()); }
                    let width = text.width(Face::Title, body, &shown);
                    text.draw(c, Face::Title, body, area.x + (area.w - width) / 2, area.y + area.h / 2 + phone::type_mid(body), pal().chalk, &shown);
                    self.region(*area, label.clone(), Action::Setup(*i));
                }
                Op::Panel(_) => (),
            }
        }
        let (x, width, art_from, end) = l.right;
        let name_size = self.sp(16.0);
        let size_size = self.sp(13.0);
        for (i, p) in self.core_offers.clone().iter().enumerate() {
            let area = l.areas[i];
            let open = self.core_channel_open;
            let chosen = open && self.prefs.selected_cores.contains(&p.core);
            let fg = if chosen { pal().ink } else { pal().chalk };
            c.round_rect(area.x, area.y, area.w, area.h, self.d(12.0), if chosen { pal().label } else { pal().spine });
            if hints && self.setup_row == i { c.frame(area.x, area.y, area.w, area.h, self.d(2.0), pal().chalk); }
            // No radio for a system that cannot be chosen yet: it is only listed, as "Not available here".
            let radio = if open { self.d(20.0) } else { 0 };
            if open { self.radio(c, area.x + self.d(16.0) + radio / 2, area.y + area.h / 2, radio, chosen, fg); }
            let tx = area.x + radio + self.d(if open { 28.0 } else { 16.0 });
            let room = area.w - radio - self.d(if open { 44.0 } else { 32.0 });
            let shown = text.fit(Face::Body, name_size, &p.package.name, room);
            if shown != p.package.name { self.cut.push(p.package.name.clone()); }
            let first = area.y + self.d(8.0) + px(name_size);
            text.draw(c, Face::Body, name_size, tx, first, fg, &shown);
            let size = self.core_size_or_soon(p);
            text.draw(c, Face::Body, size_size, tx, first + self.d(4.0) + px(size_size), if open { fg } else { pal().dust }, &size);
            self.region(area, format!("{} · {}", p.package.name, size), Action::Setup(i));
        }
        // Decorative art only uses space left after every actual download choice.
        let art_top = art_from + self.d(16.0);
        if end - art_top > self.d(90.0) {
            if let Some(sprite) = pocket_ui::ui_art::get(pocket_ui::ui_art::Art::NoGames) {
                let fitted = sprite.fitted(width.min(self.d(320.0)), (end - art_top).min(self.d(300.0)));
                c.blit_sprite(&fitted, x + (width - fitted.w as i32) / 2, art_top, 0);
            }
        }
    }

    fn setup_button(&mut self, c: &mut Canvas, text: &mut Text, area: Area, label: &str, primary: bool, i: usize) {
        let on = self.controller_hints() && i == self.setup_row;
        let (bg, fg) = if primary { (pal().label, pal().ink) } else { (pal().pill, pal().chalk) };
        c.round_rect(area.x, area.y, area.w, area.h, area.h / 2, bg);
        if on {
            c.frame(area.x - 4, area.y - 4, area.w + 8, area.h + 8, self.d(2.0), pal().chalk);
        }
        // A little smaller before "…": a whole word on a button reads better than a cut one.
        let (s, shown) = text.fit_sizes(Face::Title, &[self.sp(17.0), self.sp(15.0), self.sp(13.0)], label, area.w - self.d(24.0));
        let tw = text.width(Face::Title, s, &shown);
        text.draw(c, Face::Title, s, area.x + (area.w - tw) / 2, area.y + area.h / 2 + phone::type_mid(s), fg, &shown);
        self.region(area, label.into(), Action::Setup(i));
    }
}

/// How the first-start page is laid out on a screen that is wider than tall. It keeps its usual look whenever that fits;
/// when a word would tear, text would lie on text or a button's word would be cut, the next plan of the ladder is tried:
/// the words' column narrower, Back and Continue across both columns, tighter margins, and only then everything a little
/// smaller (never a word cut in the middle, never a "…").
#[derive(Clone, Copy, Debug, PartialEq)]
struct SetupPlan {
    /// The share of the width (percent) the words' column keeps.
    share: i32,
    /// Back and Continue (and Skip on the systems step) run across both columns under them.
    bar: bool,
    /// Margins and the gap between the columns half as wide.
    tight: bool,
    /// Type, rows and spacing as a share of their usual size.
    shrink: f32,
}

impl SetupPlan {
    /// What the page has always been.
    const USUAL: SetupPlan = SetupPlan { share: 46, bar: false, tight: false, shrink: 1.0 };

    /// The plans in the order they are tried: reflow first (columns, buttons, margins), then smaller.
    fn ladder() -> Vec<SetupPlan> {
        let mut plans = Vec::new();
        for shrink in [1.0, 0.92, 0.85, 0.78, 0.7] {
            for tight in [false, true] {
                for (share, bar) in [(46, false), (40, false), (34, false), (40, true), (34, true), (28, true)] {
                    plans.push(SetupPlan { share, bar, tight, shrink });
                }
            }
        }
        plans
    }
}

/// A line of text the page paints: face, size, left edge, baseline, colour, words.
struct Put {
    face: Face,
    px: u32,
    x: i32,
    y: i32,
    color: u32,
    s: String,
}

/// One thing the page does, in order.
enum Op {
    Put(Put),
    Label(Area, String),
    Panel(Area),
    Skip,
    Back,
    Next,
}

type Choice = (usize, String, u16);

/// A step's page as a plan puts it: where everything stands, measured, and what is wrong with it.
struct SetupLayout {
    plan: SetupPlan,
    items: Vec<(String, u16)>,
    /// The step indicator: left, top, segment width, gap, height, the last segment in Chalk.
    bar: (i32, i32, i32, i32, i32, i32),
    /// What the page paints, in the order it is painted and its areas are registered for taps and screen readers.
    ops: Vec<Op>,
    skip: Option<(usize, String, Area)>,
    /// Where Skip's word is painted.
    skip_ink: Option<Area>,
    back: Option<(usize, String, Area)>,
    next: Option<(usize, String, Area)>,
    /// The right-hand column: left edge, width, where its choices start, its bottom.
    right: (i32, i32, i32, i32),
    /// The choices, their (column, row, width) cells, the row pitch, the list's height, the preview's height.
    choices: (Vec<Choice>, Vec<(i32, i32, i32)>, i32, i32, i32),
    /// Where the choices' rows start, and each choice's area.
    top: i32,
    areas: Vec<Area>,
    faults: Vec<String>,
}

impl SetupLayout {
    fn new(plan: SetupPlan, items: Vec<(String, u16)>) -> Self {
        SetupLayout {
            plan,
            items,
            bar: (0, 0, 0, 0, 0, 0),
            ops: Vec::new(),
            skip: None,
            skip_ink: None,
            back: None,
            next: None,
            right: (0, 0, 0, 0),
            choices: (Vec::new(), Vec::new(), 0, 0, 0),
            top: 0,
            areas: Vec::new(),
            faults: Vec::new(),
        }
    }

    fn put(&mut self, face: Face, px: u32, x: i32, y: i32, color: u32, s: &str) {
        self.ops.push(Op::Put(Put { face, px, x, y, color, s: s.to_string() }));
    }

    /// A text block read out whole to screen readers.
    fn label(&mut self, area: Area, name: String) {
        self.ops.push(Op::Label(area, name));
    }

    /// Faults of where things stand: text off the canvas or on other text or on a button or choice, a button off it.
    fn check(&mut self, text: &mut Text, (w, h): (i32, i32)) {
        let mut ink: Vec<Area> = Vec::new();
        let mut words: Vec<String> = Vec::new();
        for p in self.ops.iter().filter_map(|op| if let Op::Put(p) = op { Some(p) } else { None }) {
            let size = pocket_ui::type_scale::px(p.px) as i32;
            ink.push(Area::new(p.x, p.y - size * 7 / 10, text.width(p.face, p.px, &p.s), size * 8 / 10));
            words.push(p.s.clone());
        }
        if let Some(a) = self.skip_ink {
            ink.push(a);
            words.push("Skip".into());
        }
        // Back and Continue are pills: their rounded ends are empty corners, so only the straight middle counts.
        let pills = [&self.back, &self.next].into_iter().flatten().map(|(_, _, a)| Area::new(a.x + a.h * 3 / 10, a.y, a.w - a.h * 3 / 5, a.h));
        let shapes: Vec<Area> = pills.chain(self.areas.iter().copied()).collect();
        let overlap = |a: &Area, b: &Area| (a.x + a.w).min(b.x + b.w) - a.x.max(b.x) > 1 && (a.y + a.h).min(b.y + b.h) - a.y.max(b.y) > 1;
        for (i, a) in ink.iter().enumerate() {
            if a.x < 0 || a.y < 0 || a.x + a.w > w || a.y + a.h > h {
                self.faults.push(format!("text off the canvas: {:?} {a:?}", words[i]));
            }
            if let Some(j) = (i + 1..ink.len()).find(|j| overlap(a, &ink[*j])) {
                self.faults.push(format!("text on text: {:?} {a:?} and {:?} {:?}", words[i], words[j], ink[j]));
            }
            if let Some(b) = shapes.iter().find(|b| overlap(a, b)) {
                self.faults.push(format!("text on a button or a choice: {:?} {a:?} on {b:?}", words[i]));
            }
        }
        let targets = shapes.iter().copied().chain(self.skip.iter().map(|(_, _, a)| *a));
        if targets.into_iter().any(|a| a.x < 0 || a.y < 0 || a.x + a.w > w || a.y + a.h > h) {
            self.faults.push("a button or a choice off the canvas".into());
        }
        for (i, a) in shapes.iter().enumerate() {
            if shapes.iter().skip(i + 1).any(|b| overlap(a, b)) {
                self.faults.push("a button on another".into());
            }
        }
    }
}

fn look_preview(text: &mut Text, theme: Theme, w: i32, h: i32, dp: f32) -> art::Pic {
    let mut pic = art::Pic::new(w, h, 0);
    {
        let mut c = Canvas::new(&mut pic.px, w as u32, h as u32);
        c.clear(pal().void);
        let look = art::Look {
            color: 0x8468ba,
            band: 0x8468ba,
            plastic: art::plastic("gba"),
            tape: false,
        };
        let m = Medium::Gba;
        if theme == Theme::Big {
            let (fw, fh) = media::front_size(m, 1.0);
            let k = (w as f32 * 0.9 / fw as f32).min(h as f32 * 0.95 / fh as f32);
            let front = media::front(text, m, k, look, "pocket-ui", None, pocket_ui::system_art::get("gba"), "Game Boy Advance");
            art::blit(&mut c, &front, (w - front.w) / 2, (h - front.h) / 2, 256, (0, h));
        } else if theme == Theme::Split {
            let listw = w / 2;
            // Rows in dp, like the phone's lists, so the preview reads at the same size.
            let (row, gap) = ((40.0 * dp) as i32, (6.0 * dp) as i32);
            let rows = ((h - gap) / (row + gap)).clamp(1, 4);
            let y0 = (h - rows * (row + gap) + gap) / 2;
            for i in 0..rows {
                let y = y0 + i * (row + gap);
                kit::shelf_row(&mut c, 0, y, listw - gap, row, i == 0);
                c.rect(0, y, (5.0 * dp) as i32, row, look.color);
                let font = (16.0 * dp) as u32;
                text.draw(&mut c, Face::Title, font, (16.0 * dp) as i32, y + row / 2 + font as i32 * 7 / 20, if i == 0 { pal().ink } else { pal().chalk }, &format!("pocket-ui {}", i + 1));
            }
            let (fw, fh) = media::front_size(m, 1.0);
            let k = ((w - listw) as f32 * 0.8 / fw as f32).min(h as f32 * 0.85 / fh as f32);
            let front = media::front(text, m, k, look, "pocket-ui", None, pocket_ui::system_art::get("gba"), "Game Boy Advance");
            art::blit(&mut c, &front, listw + (w - listw - front.w) / 2, (h - front.h) / 2, 256, (0, h));
        } else {
            if theme == Theme::Cabinet {
                art::blit(&mut c, &art::compartment(w, h - 20, 0), 0, 0, 256, (0, h));
            } else {
                c.rect(0, h - 24, w, 12, pal().spine);
            }
            // Three cases standing on the board, as large as the preview's height, together in the middle.
            let k = h as f32 / m.size().h as f32 * 0.8;
            let items: Vec<_> = (0..3)
                .map(|i| {
                    let item = if i == 1 {
                        media::front(text, m, k, look, "pocket-ui", None, pocket_ui::system_art::get("gba"), "Game Boy Advance")
                    } else {
                        media::spine(text, m, k, look, "pocket-ui")
                    };
                    let available = (w / 3 - 20).max(20);
                    if item.w > available {
                        media::rescaled(&item, available as f32 / item.w as f32)
                    } else {
                        item
                    }
                })
                .collect();
            let total: i32 = items.iter().map(|p| p.w + 16).sum::<i32>() - 16;
            let mut x = (w - total) / 2;
            for item in &items {
                art::blit(&mut c, item, x, h - 30 - item.h, 256, (0, h));
                x += item.w + 16;
            }
        }
    }
    for pixel in &mut pic.px {
        *pixel |= 0xff00_0000;
    }
    pic
}
