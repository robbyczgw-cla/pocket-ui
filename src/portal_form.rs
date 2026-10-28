//! "Sign in…" on the sign-in sheet (`portal_page`): the network's sign-in page in pocket-ui's own style. Its name, the
//! first lines of its text (all of it on "Read the whole page"), and its form: text fields typed with the on-screen
//! keyboard (a password shown as dots), boxes to tick, choices, and its buttons. A button sends the form, and only a
//! press of it does (`Ask::Send`); what came back is shown as it is: online, the next step of the page, or why the
//! Pocket can't do this one and the other ways.
//!
//! What was typed lives in this page only: it is overwritten when the page closes (`Drop`), and the copy that is sent is
//! overwritten once sent (`portal::Session::submit`). The requests are `portal`'s; this file never touches the network.

use crate::gfx::Canvas;
use crate::html_form::{Field, Form, Kind, NoForm, Page};
use crate::keyboard::{Keyboard, Outcome as Typed};
use crate::pad::Button;
use crate::palette::*;
use crate::portal::{Failure, Outcome};
use crate::text::{Face, Text};

/// Forms sent in one sign-in before the helper gives up (a page that answers each one with another).
pub const MAX_STEPS: usize = 5;
/// Lines of the page's text shown above the form.
const EXCERPT_LINES: usize = 3;

/// A row of the form view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Row {
    /// The page's whole text, in a reader.
    Read,
    /// A field of a form: (form, field) in `Page::forms`.
    Field(usize, usize),
}

/// The page with a form, as the person fills it in.
pub struct Filling {
    pub page: Page,
    /// Where it was loaded from (the base of its links, and the referer of what is sent).
    pub url: String,
    pub rows: Vec<Row>,
    pub sel: usize,
    /// The field being typed into, and its keyboard.
    pub typing: Option<(usize, Keyboard)>,
    /// The reader of the whole text is open, scrolled down by this many pixels.
    pub reading: Option<i32>,
    /// What the last press found wrong ("Fill in Room number first."), or what the page did.
    pub note: Option<String>,
    /// Forms sent so far in this sign-in.
    pub steps: usize,
    reader_max: std::cell::Cell<i32>,
}

impl Drop for Filling {
    fn drop(&mut self) {
        for form in &mut self.page.forms {
            form.wipe();
        }
        if let Some((_, kb)) = &mut self.typing {
            crate::portal::wipe(&mut kb.text);
        }
    }
}

/// Why the sign-in ended without a form to fill in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum End {
    Online,
    /// Scripts or nothing to fill in; the page's name when there was one.
    NeedsBrowser(NoForm, String),
    /// Sent, and the network still wants a sign-in, with nothing more to fill in.
    StillWants,
    /// The form would send to another site.
    ElseWhere(String),
    /// More forms than `MAX_STEPS`.
    TooManySteps,
    Failed(Failure),
}

pub enum State {
    Loading,
    Filling(Box<Filling>),
    /// The form was sent; the page waits for the answer.
    Sending(Box<Filling>),
    Ended(End),
}

/// What a press asks of the Wi-Fi page.
#[derive(Debug)]
pub enum Ask {
    Nothing,
    /// Back to the sheet.
    Back,
    /// Load the sign-in page again.
    Reload,
    /// The sheet's Other ways.
    OtherWays,
    /// Send this copy of a form, as the button `submitter` would, for the page loaded from `url`.
    Send { form: Box<Form>, submitter: Option<usize>, url: String },
}

pub struct SignIn {
    pub state: State,
    /// The page's certificate couldn't be checked (`portal::Session::insecure`).
    pub insecure: bool,
    /// The host of the page, for its line ("Sign-in page: …").
    pub host: String,
    /// Which answer belongs to this page (an answer for an earlier one is dropped).
    pub ticket: u64,
    /// The row of an ended sign-in (Try again, Other ways).
    pub end_sel: usize,
}

/// The rows of a page: "Read the whole page" when its text is longer than the lines shown, then the fields people see
/// and the buttons of every usable form.
pub fn rows(page: &Page, forms: &[usize], excerpt_cut: bool) -> Vec<Row> {
    let mut rows = Vec::new();
    if excerpt_cut {
        rows.push(Row::Read);
    }
    for &fi in forms {
        if let Some(form) = page.forms.get(fi) {
            rows.extend(form.shown().map(|(k, _)| Row::Field(fi, k)));
        }
    }
    rows
}

/// A field's name on its row: the page's label, else its own words for what it is.
pub fn field_name(f: &Field) -> String {
    if !f.label.is_empty() {
        return f.label.clone();
    }
    match f.kind {
        Kind::Submit { .. } => "Send".into(),
        Kind::Password => "Password".into(),
        Kind::Email => "E-mail address".into(),
        Kind::Checkbox => "Tick box".into(),
        _ if !f.name.is_empty() => f.name.chars().take(60).collect(),
        _ => "Field without a name".into(),
    }
}

/// Whether `field_name` is pocket-ui's own words (translated) rather than the page's (painted as they are).
pub fn own_name(f: &Field) -> bool {
    f.label.is_empty() && (matches!(f.kind, Kind::Submit { .. } | Kind::Password | Kind::Email | Kind::Checkbox) || f.name.is_empty())
}

/// What a field holds, as its row shows it: the text (a password as dots), the choice, or "Empty".
pub fn field_value(f: &Field) -> String {
    match f.kind {
        Kind::Password if f.value.is_empty() => "Empty".into(),
        Kind::Password => "•".repeat(f.value.chars().count().min(12)),
        k if k.typed() && f.value.is_empty() => "Empty".into(),
        k if k.typed() => f.value.clone(),
        Kind::Radio | Kind::Select => f.selected.and_then(|s| f.options.get(s)).map_or("Choose".into(), |(_, l)| l.clone()),
        _ => String::new(),
    }
}

/// The first field of `form` that must be filled in or ticked and isn't.
pub fn missing(form: &Form) -> Option<&Field> {
    form.fields.iter().find(|f| {
        f.required
            && !f.disabled
            && match f.kind {
                Kind::Checkbox => !f.checked,
                Kind::Radio | Kind::Select => f.selected.is_none(),
                k if k.typed() => f.value.trim().is_empty(),
                _ => false,
            }
    })
}

/// A password as the keyboard's field shows it: dots, the last letter typed as it is (so a slip is seen).
pub fn masked(s: &str) -> String {
    let n = s.chars().count();
    let last: String = s.chars().last().map(String::from).unwrap_or_default();
    format!("{}{}", "•".repeat(n.saturating_sub(1)), last)
}

impl SignIn {
    pub fn new(host: &str, ticket: u64) -> SignIn {
        SignIn { state: State::Loading, insecure: false, host: host.to_string(), ticket, end_sel: 0 }
    }

    /// What `portal` found: the page with its form, or the end.
    pub fn on_outcome(&mut self, outcome: Outcome, insecure: bool) {
        self.insecure |= insecure;
        let steps = match &self.state {
            State::Sending(f) | State::Filling(f) => f.steps,
            _ => 0,
        };
        let was_sending = matches!(self.state, State::Sending(_));
        self.end_sel = 0;
        self.state = match outcome {
            Outcome::Online => State::Ended(End::Online),
            Outcome::Form { .. } if steps >= MAX_STEPS => State::Ended(End::TooManySteps),
            Outcome::Form { page, url, forms } => {
                if let Some(h) = crate::net_check::host_of(&url) {
                    self.host = h.to_string();
                }
                let rows = rows(&page, &forms, long_text(&page));
                // The first thing to fill in, else the first button.
                let sel = rows.iter().position(|r| matches!(r, Row::Field(..))).unwrap_or(0);
                let note = was_sending.then(|| "The network still wants a sign-in. Check what the page says.".to_string());
                State::Filling(Box::new(Filling { page, url, rows, sel, typing: None, reading: None, note, steps, reader_max: std::cell::Cell::new(0) }))
            }
            Outcome::NeedsBrowser { why, page } if was_sending && why == NoForm::Nothing => {
                let _ = page;
                State::Ended(End::StillWants)
            }
            Outcome::NeedsBrowser { why, page } => State::Ended(End::NeedsBrowser(why, page.map(|p| p.name().to_string()).unwrap_or_default())),
            Outcome::ElseWhere(host) => State::Ended(End::ElseWhere(host)),
            Outcome::Failed(f) => State::Ended(End::Failed(f)),
        };
    }

    /// The network let the Pocket in (a re-check): whatever was being filled in is done with.
    pub fn online(&mut self) {
        self.state = State::Ended(End::Online);
    }

    /// The rows of an ended sign-in.
    pub fn end_rows(&self) -> Vec<(&'static str, Ask)> {
        match &self.state {
            State::Ended(End::Online) => vec![],
            State::Ended(End::StillWants | End::Failed(_) | End::TooManySteps) => vec![("Try again", Ask::Reload), ("Other ways", Ask::OtherWays)],
            _ => vec![("Other ways", Ask::OtherWays)],
        }
    }

    pub fn press(&mut self, b: Button) -> Ask {
        match &mut self.state {
            State::Loading | State::Sending(_) => match b {
                Button::South => Ask::Back,
                _ => Ask::Nothing,
            },
            State::Ended(_) => {
                let rows = self.end_rows();
                match b {
                    Button::Up => self.end_sel = self.end_sel.saturating_sub(1),
                    Button::Down => self.end_sel = (self.end_sel + 1).min(rows.len().saturating_sub(1)),
                    Button::East => return rows.into_iter().nth(self.end_sel).map_or(Ask::Back, |(_, a)| a),
                    Button::South => return Ask::Back,
                    _ => {}
                }
                Ask::Nothing
            }
            State::Filling(f) => f.press(b),
        }
    }

    /// The form was handed on to be sent: the page shows it while it waits.
    pub fn sending(&mut self) {
        if let State::Filling(_) = &self.state {
            let State::Filling(f) = std::mem::replace(&mut self.state, State::Loading) else { return };
            let mut f = f;
            f.steps += 1;
            f.note = None;
            self.state = State::Sending(f);
        }
    }

    pub fn mark(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        self.end_sel.hash(&mut h);
        match &self.state {
            State::Loading => 0u8.hash(&mut h),
            State::Sending(_) => 1u8.hash(&mut h),
            State::Ended(e) => (2u8, format!("{e:?}")).hash(&mut h),
            State::Filling(f) => {
                (3u8, f.sel, f.reading, &f.note).hash(&mut h);
                if let Some((k, kb)) = &f.typing {
                    (k, kb.text.len(), kb.row, kb.col, kb.shift, kb.symbols).hash(&mut h);
                }
                for form in &f.page.forms {
                    for fl in &form.fields {
                        (fl.value.len(), fl.checked, fl.selected).hash(&mut h);
                    }
                }
            }
        }
        h.finish()
    }
}

impl Filling {
    fn field(&self, row: Row) -> Option<&Field> {
        match row {
            Row::Field(fi, k) => self.page.forms.get(fi)?.fields.get(k),
            Row::Read => None,
        }
    }

    fn field_mut(&mut self, row: Row) -> Option<&mut Field> {
        match row {
            Row::Field(fi, k) => self.page.forms.get_mut(fi)?.fields.get_mut(k),
            Row::Read => None,
        }
    }

    pub fn selected(&self) -> Option<&Field> {
        self.rows.get(self.sel).and_then(|r| self.field(*r))
    }

    fn press(&mut self, b: Button) -> Ask {
        if let Some(scroll) = &mut self.reading {
            match b {
                Button::Up => *scroll = (*scroll - 120).max(0),
                Button::Down => *scroll = (*scroll + 120).min(self.reader_max.get()),
                Button::South | Button::East => self.reading = None,
                _ => {}
            }
            return Ask::Nothing;
        }
        if let Some((row, kb)) = &mut self.typing {
            let row = *row;
            match kb.press(b) {
                Typed::Editing => {}
                Typed::Cancelled => self.typing = None,
                Typed::Done(mut text) => {
                    let r = self.rows[row];
                    if let Some(f) = self.field_mut(r) {
                        crate::portal::wipe(&mut f.value);
                        f.value = text.chars().take(f.max).collect();
                    }
                    crate::portal::wipe(&mut text);
                    if let Some((_, mut kb)) = self.typing.take() {
                        crate::portal::wipe(&mut kb.text);
                    }
                    self.note = None;
                }
            }
            return Ask::Nothing;
        }
        let row = self.rows.get(self.sel).copied();
        match b {
            Button::Up => self.sel = self.sel.saturating_sub(1),
            Button::Down => self.sel = (self.sel + 1).min(self.rows.len().saturating_sub(1)),
            Button::L1 => self.sel = self.sel.saturating_sub(5),
            Button::R1 => self.sel = (self.sel + 5).min(self.rows.len().saturating_sub(1)),
            Button::South => return Ask::Back,
            Button::Left | Button::Right => self.cycle(b == Button::Right),
            Button::East => match row {
                Some(Row::Read) => self.reading = Some(0),
                Some(r @ Row::Field(fi, k)) => {
                    let Some(kind) = self.field(r).map(|f| f.kind) else { return Ask::Nothing };
                    match kind {
                        Kind::Checkbox => {
                            if let Some(f) = self.field_mut(r) {
                                f.checked = !f.checked;
                            }
                        }
                        Kind::Radio | Kind::Select => self.cycle(true),
                        Kind::Submit { .. } => return self.send(fi, k),
                        k if k.typed() => {
                            let kb = Keyboard { text: self.field(r).map(|f| f.value.clone()).unwrap_or_default(), ..Keyboard::default() };
                            self.typing = Some((self.sel, kb));
                        }
                        _ => {}
                    }
                }
                None => {}
            },
            _ => {}
        }
        Ask::Nothing
    }

    /// Left/Right (or A) on a choice: the next or previous option.
    fn cycle(&mut self, forward: bool) {
        let Some(row) = self.rows.get(self.sel).copied() else { return };
        if let Some(f) = self.field_mut(row).filter(|f| matches!(f.kind, Kind::Radio | Kind::Select)) {
            let n = f.options.len();
            if n == 0 {
                return;
            }
            f.selected = Some(match (f.selected, forward) {
                (None, true) => 0,
                (None, false) => n - 1,
                (Some(s), true) => (s + 1) % n,
                (Some(s), false) => (s + n - 1) % n,
            });
        }
    }

    /// The button `k` of form `fi` was pressed: the form goes when everything it needs is there.
    fn send(&mut self, fi: usize, k: usize) -> Ask {
        let Some(form) = self.page.forms.get(fi) else { return Ask::Nothing };
        if let Some(f) = missing(form) {
            let name = field_name(f);
            self.note = Some(match f.kind {
                Kind::Checkbox => format!("Tick “{name}” first."),
                Kind::Radio | Kind::Select => format!("Choose “{name}” first."),
                _ => format!("Fill in “{name}” first."),
            });
            if let Some(at) = self.rows.iter().position(|r| *r == Row::Field(fi, form.fields.iter().position(|x| std::ptr::eq(x, f)).unwrap_or(0))) {
                self.sel = at;
            }
            return Ask::Nothing;
        }
        Ask::Send { form: Box::new(form.clone()), submitter: Some(k), url: self.url.clone() }
    }
}

/// The lines of `text` (the page's, already wrapped) and whether there is more than `EXCERPT_LINES` of them.
fn excerpt(text: &mut Text, page_text: &str, name: &str, w: i32) -> (Vec<String>, bool) {
    let mut lines = Vec::new();
    // The heading is the page's name above: not again.
    for para in page_text.lines().skip_while(|l| *l == name) {
        lines.extend(text.wrap(Face::Body, 24, para, w));
        if lines.len() > EXCERPT_LINES {
            break;
        }
    }
    let cut = lines.len() > EXCERPT_LINES;
    lines.truncate(EXCERPT_LINES);
    if cut {
        if let Some(last) = lines.last_mut() {
            *last = text.fit(Face::Body, 24, &format!("{last} …"), w);
        }
    }
    (lines, cut)
}

/// Whether a page's text is longer than the few lines shown above its form (about 100 characters a line at the
/// page's width): then the form view has a "Read the whole page" row. Generous on purpose: the row costs nothing when
/// the text would just have fitted.
pub fn long_text(page: &Page) -> bool {
    let rest: Vec<&str> = page.text.lines().skip_while(|l| *l == page.name()).collect();
    rest.len() > EXCERPT_LINES || rest.iter().map(|l| l.chars().count()).sum::<usize>() > 2 * 90
}

impl SignIn {
    pub fn paint(&self, c: &mut Canvas, text: &mut Text, dots: &str) {
        let w = c.w - 2 * MARGIN;
        match &self.state {
            State::Loading => {
                text.draw(c, Face::Title, 67, MARGIN, 140, pal().chalk, "Sign in");
                text.draw(c, Face::Body, 28, MARGIN, 200, pal().dust, &format!("Loading the sign-in page{dots}"));
                // The hotel's bell and hourglass while the page comes (`ui_art`).
                let k = crate::ui_art::empty_scale(c.h);
                let (aw, ah) = crate::ui_art::size(crate::ui_art::Art::Hotel, k);
                let y = (c.h - ah) / 2 + 40;
                if y > 240 && y + ah < c.h - 130 {
                    crate::ui_art::paint(c, crate::ui_art::Art::Hotel, (c.w - aw) / 2, y, k);
                }
                crate::setup::hints(c, text, &[("B", "Back")]);
            }
            State::Filling(f) => self.paint_filling(c, text, f, None),
            State::Sending(f) => self.paint_filling(c, text, f, Some(dots)),
            State::Ended(end) => self.paint_end(c, text, end, w),
        }
    }

    fn paint_end(&self, c: &mut Canvas, text: &mut Text, end: &End, w: i32) {
        let (title, body): (&str, String) = match end {
            End::Online => ("You're online", "The network lets this Pocket in. Games, the store and updates can go online.".into()),
            End::NeedsBrowser(NoForm::NeedsScripts, _) => ("This page needs a full browser", "It only works with scripts, which the Pocket doesn't run. Use one of the other ways.".into()),
            End::NeedsBrowser(NoForm::Nothing, _) => ("This page needs a full browser", "It has nothing the Pocket can fill in, only links (to sign in with another account or to pay, for example). Use one of the other ways.".into()),
            End::NeedsBrowser(NoForm::ElseWhere, _) => ("This page needs a full browser", "It sends what it carries to an address the Pocket doesn't send to. Use one of the other ways.".into()),
            End::StillWants => ("Still not online", "The page took the details, but the network still wants a sign-in. Check what you typed and try again, or use one of the other ways.".into()),
            End::ElseWhere(host) => ("Not sent", format!("This page would send your details to another address ({host}). The Pocket only sends them to the sign-in page itself. Use one of the other ways.")),
            End::TooManySteps => ("Still not online", "The page keeps asking for more. Use one of the other ways.".into()),
            End::Failed(Failure::Unreachable) => ("No answer", "The sign-in page didn't answer. Try again in a moment.".into()),
            End::Failed(Failure::TooBig) => ("This page needs a full browser", "The sign-in page is too large for the Pocket. Use one of the other ways.".into()),
            End::Failed(Failure::Loop) => ("This page needs a full browser", "The sign-in page keeps sending the Pocket on to other pages. Use one of the other ways.".into()),
            End::Failed(Failure::Refused(_)) => ("This page needs a full browser", "The sign-in page leads to an address the Pocket doesn't open. Use one of the other ways.".into()),
            End::Failed(Failure::Other(_)) => ("No answer", "Couldn't load the sign-in page. Try again in a moment.".into()),
        };
        let title = text.fit(Face::Title, 67, title, w);
        text.draw(c, Face::Title, 67, MARGIN, 140, pal().chalk, &title);
        let step = crate::type_scale::line(28, 12);
        let mut y = 200;
        if let End::NeedsBrowser(_, name) = end {
            if !name.is_empty() {
                text.set_translate(false);
                let name = text.fit(Face::Body, 28, name, w);
                text.draw(c, Face::Body, 28, MARGIN, y, pal().chalk, &name);
                text.set_translate(true);
                y += step;
            }
        }
        y = text.paragraph(c, Face::Body, 28, (MARGIN, y), w, step, y + 4 * step, pal().dust, &body) + 12;
        let row = crate::type_scale::row(70, 37);
        let rows = self.end_rows();
        for (i, (name, _)) in rows.iter().enumerate() {
            let ry = y + i as i32 * row;
            let selected = i == self.end_sel;
            c.rect(MARGIN, ry, w, row - 8, if selected { pal().label } else { pal().spine });
            text.draw(c, Face::Title, 37, MARGIN + 28, ry + (row - 8) / 2 + crate::type_scale::base(13, 37), if selected { pal().ink } else { pal().chalk }, name);
        }
        let mut h = Vec::new();
        if !rows.is_empty() {
            h.push(("A", "Open"));
        }
        h.push(("B", "Back"));
        crate::setup::hints(c, text, &h);
    }

    fn paint_filling(&self, c: &mut Canvas, text: &mut Text, f: &Filling, sending: Option<&str>) {
        if let Some((row, kb)) = &f.typing {
            let name = f.rows.get(*row).and_then(|r| f.field(*r)).map(field_name).unwrap_or_default();
            let password = f.rows.get(*row).and_then(|r| f.field(*r)).is_some_and(|x| x.kind == Kind::Password);
            let own = f.rows.get(*row).and_then(|r| f.field(*r)).is_some_and(own_name);
            text.set_translate(own);
            let title = text.fit(Face::Title, 50, &name, c.w - 2 * MARGIN);
            text.draw(c, Face::Title, 50, MARGIN, 110, pal().chalk, &title);
            text.set_translate(true);
            let shown = if password { masked(&kb.text) } else { kb.text.clone() };
            crate::setup::paint_keyboard_shown(c, text, kb, "OK", &shown);
            crate::setup::hints(c, text, &[("A", "Type"), ("B", "Delete"), ("X", "Shift"), ("Y", "Symbols"), ("START", "OK")]);
            return;
        }
        let w = c.w - 2 * MARGIN;
        if let Some(scroll) = f.reading {
            self.paint_reader(c, text, f, scroll, w);
            return;
        }
        // The page's name (its heading or title), as the page has it.
        text.set_translate(false);
        let name = if f.page.name().is_empty() { self.host.clone() } else { f.page.name().to_string() };
        let name = text.fit(Face::Title, 50, &name, w);
        text.draw(c, Face::Title, 50, MARGIN, 110, pal().chalk, &name);
        text.set_translate(true);
        let mut y = 160;
        let line = text.fit(Face::Body, 24, &format!("Sign-in page: {}", self.host), w);
        text.draw(c, Face::Body, 24, MARGIN, y, pal().dust, &line);
        y += crate::type_scale::line(24, 10);
        if self.insecure {
            let warn = text.fit(Face::Body, 24, "Its certificate can't be checked: others nearby could read what you send.", w);
            text.draw(c, Face::Body, 24, MARGIN, y, pal().alert, &warn);
            y += crate::type_scale::line(24, 10);
        }
        match (&f.note, sending) {
            (_, Some(dots)) => {
                text.draw(c, Face::Body, 28, MARGIN, y + 4, pal().chalk, &format!("Sending{dots}"));
                y += crate::type_scale::line(28, 12);
            }
            (Some(note), None) => {
                let note = text.fit(Face::Body, 28, note, w);
                text.draw(c, Face::Body, 28, MARGIN, y + 4, pal().alert, &note);
                y += crate::type_scale::line(28, 12);
            }
            _ => {}
        }
        // The first lines of the page's text.
        text.set_translate(false);
        let (lines, _) = excerpt(text, &f.page.text, f.page.name(), w);
        for l in &lines {
            text.draw(c, Face::Body, 24, MARGIN, y + 6, pal().dust, l);
            y += crate::type_scale::line(24, 10);
        }
        text.set_translate(true);
        // The rows: as many as fit above the hints and the description of the selected one, scrolled to it.
        let top = y + 18;
        let row = crate::type_scale::row(70, 37);
        let desc_lines = 2;
        let bottom = c.h - 100 - desc_lines * crate::type_scale::line(24, 8);
        let visible = ((bottom - top) / row).max(1) as usize;
        let first = f.sel.saturating_sub(visible.saturating_sub(1)).min(f.rows.len().saturating_sub(visible));
        // Whether the selected row's name was cut short: then it is written out in full below the rows.
        let mut cut = false;
        for (k, r) in f.rows.iter().enumerate().skip(first).take(visible) {
            let ry = top + (k - first) as i32 * row;
            let selected = k == f.sel;
            c.rect(MARGIN, ry, w, row - 8, if selected { pal().label } else { pal().spine });
            let fg = if selected { pal().ink } else { pal().chalk };
            let quiet = if selected { pal().label_dust } else { pal().dust };
            let mid = ry + (row - 8) / 2;
            let Some(field) = f.field(*r) else {
                text.draw(c, Face::Title, 37, MARGIN + 28, mid + crate::type_scale::base(13, 37), fg, "Read the whole page");
                text.draw_right(c, Face::Body, 24, MARGIN + w - 28, mid + crate::type_scale::base(10, 24), quiet, "Terms and info");
                continue;
            };
            // The side: a tick box, a value, or what A does on a button.
            let side_w = match field.kind {
                Kind::Checkbox => {
                    let s = 34;
                    let (bx, by) = (MARGIN + w - 28 - s, mid - s / 2);
                    c.frame(bx, by, s, s, 3, fg);
                    if field.checked {
                        crate::setup::tick(c, bx, by, s, fg);
                    }
                    s + 20
                }
                Kind::Submit { .. } => {
                    let s = "Send";
                    text.draw_right(c, Face::Body, 24, MARGIN + w - 28, mid + crate::type_scale::base(10, 24), quiet, s);
                    text.width(Face::Body, 24, s) + 20
                }
                _ => {
                    let v = field_value(field);
                    let own = v == "Empty" || v == "Choose";
                    text.set_translate(own);
                    let max = w / 2 - 40;
                    let v = text.fit(Face::Body, 24, &v, max);
                    let v = if selected && matches!(field.kind, Kind::Radio | Kind::Select) { format!("‹ {v} ›") } else { v };
                    text.draw_right(c, Face::Body, 24, MARGIN + w - 28, mid + crate::type_scale::base(10, 24), if own { quiet } else { fg }, &v);
                    let vw = text.width(Face::Body, 24, &v);
                    text.set_translate(true);
                    vw + 20
                }
            };
            let name = field_name(field);
            let own = own_name(field);
            text.set_translate(own);
            let name = text.fit(Face::Title, 37, &name, w - 56 - side_w);
            text.draw(c, Face::Title, 37, MARGIN + 28, mid + crate::type_scale::base(13, 37), fg, &name);
            if selected && name != field_name(field) {
                cut = true;
            }
            text.set_translate(true);
        }
        // The selected field's whole label (a long "I accept …" is cut on its row).
        if let Some(field) = f.selected().filter(|_| cut) {
            let label = field.label.clone();
            if !label.is_empty() {
                text.set_translate(false);
                let mut yy = bottom + 4 + crate::type_scale::px(24) as i32;
                for l in text.wrap_at_most(Face::Body, 24, &label, w, desc_lines as usize) {
                    text.draw(c, Face::Body, 24, MARGIN, yy, pal().dust, &l);
                    yy += crate::type_scale::line(24, 8);
                }
                text.set_translate(true);
            }
        }
        let a = match f.selected().map(|x| x.kind) {
            _ if sending.is_some() => None,
            Some(Kind::Checkbox) => Some("Tick"),
            Some(Kind::Radio | Kind::Select) => Some("Change"),
            Some(Kind::Submit { .. }) => Some("Send"),
            Some(k) if k.typed() => Some("Type"),
            None if f.rows.get(f.sel) == Some(&Row::Read) => Some("Read"),
            _ => None,
        };
        let mut h = Vec::new();
        if let Some(a) = a {
            h.push(("A", a));
        }
        h.push(("B", "Back"));
        crate::setup::hints(c, text, &h);
    }

    fn paint_reader(&self, c: &mut Canvas, text: &mut Text, f: &Filling, scroll: i32, w: i32) {
        text.set_translate(false);
        let name = if f.page.name().is_empty() { self.host.clone() } else { f.page.name().to_string() };
        let name = text.fit(Face::Title, 50, &name, w);
        text.draw(c, Face::Title, 50, MARGIN, 110, pal().chalk, &name);
        let (top, bottom) = (140, c.h - 100);
        let mark = crate::text::boxes::mark();
        let mut band = c.rows(top, bottom);
        let step = crate::type_scale::line(28, 12);
        let mut y = 40 - scroll;
        for para in f.page.text.lines().skip_while(|l| *l == f.page.name()) {
            for l in text.wrap(Face::Body, 28, para, w) {
                if y > -step && y < bottom - top + step {
                    text.draw(&mut band, Face::Body, 28, MARGIN, y, pal().chalk, &l);
                }
                y += step;
            }
            y += 10;
        }
        crate::text::boxes::clip_since(mark, c, top, bottom);
        f.reader_max.set((y + scroll - (bottom - top)).max(0));
        text.set_translate(true);
        let mut h = Vec::new();
        if f.reader_max.get() > 0 {
            h.push(("Up/Down", "Scroll"));
        }
        h.push(("B", "Back"));
        crate::setup::hints(c, text, &h);
    }
}

/// What a sign-in page's request might bring, made up from the test pages of `testdata/portal`, for the snapshots
/// (`--snapshot setup:wifi-portal-signin=<what>`): hotel, clickthrough, voucher, email (a form); js, social (a page
/// that needs a browser); elsewhere, still, failed, toomany, online (an end); loading (None: still loading).
pub fn demo(what: &str) -> Option<Outcome> {
    let form = |html: &str, url: &str| {
        let page = crate::html_form::parse(html);
        Outcome::Form { forms: page.usable(), page, url: url.into() }
    };
    Some(match what {
        "hotel" => form(include_str!("../testdata/portal/hotel-room.html"), "http://login.hotel-seeblick.example/portal/login"),
        "clickthrough" => form(include_str!("../testdata/portal/clickthrough.html"), "http://192.168.4.1:2050/splash"),
        "voucher" => form(include_str!("../testdata/portal/voucher.html"), "https://portal.example.net:8443/guest/"),
        "email" => form(include_str!("../testdata/portal/email.html"), "http://wifi.mall-center.example/"),
        "js" => Outcome::NeedsBrowser { why: NoForm::NeedsScripts, page: Some(crate::html_form::parse(include_str!("../testdata/portal/js-only.html"))) },
        "social" => Outcome::NeedsBrowser { why: NoForm::Nothing, page: Some(crate::html_form::parse(include_str!("../testdata/portal/social.html"))) },
        "elsewhere" => Outcome::ElseWhere("collect.example".into()),
        "failed" => Outcome::Failed(Failure::Unreachable),
        "online" => Outcome::Online,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::html_form::parse;

    fn filling(html: &str) -> SignIn {
        let page = parse(html);
        let forms = page.usable();
        let mut s = SignIn::new("portal.example", 1);
        s.on_outcome(Outcome::Form { page, url: "http://portal.example/".into(), forms }, false);
        s
    }

    fn fill(s: &mut SignIn) -> &mut Filling {
        match &mut s.state {
            State::Filling(f) => f,
            _ => panic!("not filling"),
        }
    }

    /// Selects the row of the field called `name`.
    fn go(s: &mut SignIn, name: &str) {
        let f = fill(s);
        f.sel = f.rows.iter().position(|r| f.field(*r).is_some_and(|x| field_name(x) == name)).unwrap_or_else(|| panic!("{name}"));
    }

    fn type_text(s: &mut SignIn, t: &str) {
        assert!(matches!(s.press(Button::East), Ask::Nothing));
        let f = fill(s);
        f.typing.as_mut().expect("the keyboard").1.text = t.into();
        let _ = s.press(Button::Start);
    }

    #[test]
    fn a_hotel_page_is_filled_in_and_sent_only_on_its_button() {
        let mut s = filling(include_str!("../testdata/portal/hotel-room.html"));
        let f = fill(&mut s);
        assert_eq!(f.rows[0], Row::Read, "the terms are longer than the lines above the form");
        let names: Vec<String> = f.rows.iter().filter_map(|r| f.field(*r)).map(field_name).collect();
        assert_eq!(names, ["Room number", "Surname", "I have read and accept the terms of use", "Connect"]);
        assert_eq!(f.sel, 1, "the first field");
        // The button before anything is filled in: nothing is sent, the page says what is missing.
        go(&mut s, "Connect");
        assert!(matches!(s.press(Button::East), Ask::Nothing));
        assert_eq!(fill(&mut s).note.as_deref(), Some("Fill in “Room number” first."));
        assert_eq!(fill(&mut s).sel, 1, "moved to it");
        type_text(&mut s, "412");
        let _ = s.press(Button::Down);
        type_text(&mut s, "Müller");
        go(&mut s, "Connect");
        assert!(matches!(s.press(Button::East), Ask::Nothing));
        assert_eq!(fill(&mut s).note.as_deref(), Some("Tick “I have read and accept the terms of use” first."));
        assert!(!fill(&mut s).page.forms[0].fields.iter().any(|f| f.kind == Kind::Checkbox && f.checked), "never ticked by itself");
        let _ = s.press(Button::East);
        assert!(fill(&mut s).page.forms[0].fields.iter().any(|f| f.kind == Kind::Checkbox && f.checked), "ticked by the press");
        let _ = s.press(Button::Down);
        let Ask::Send { form, submitter, url } = s.press(Button::East) else { panic!("sent") };
        assert_eq!(url, "http://portal.example/");
        let data = crate::html_form::form_data(&form, submitter);
        assert_eq!(
            data,
            [("csrf", "b7e1&9"), ("room", "412"), ("surname", "Müller"), ("accept", "1"), ("action", "login")].map(|(k, v)| (k.to_string(), v.to_string()))
        );
        s.sending();
        assert!(matches!(s.state, State::Sending(_)));
        assert!(matches!(s.press(Button::East), Ask::Nothing), "nothing more while it is sent");
    }

    #[test]
    fn typed_text_keeps_the_fields_limit_and_a_password_shows_as_dots() {
        let mut s = filling("<form><input name=room maxlength=3><input type=password name=pw><input type=submit></form>");
        type_text(&mut s, "41234");
        assert_eq!(fill(&mut s).page.forms[0].fields[0].value, "412");
        let _ = s.press(Button::Down);
        type_text(&mut s, "geheim");
        let f = fill(&mut s);
        assert_eq!(field_value(&f.page.forms[0].fields[1]), "••••••");
        assert_eq!(masked("abc"), "••c");
        assert_eq!(masked(""), "");
    }

    #[test]
    fn choices_change_with_left_right_and_a() {
        let mut s = filling(include_str!("../testdata/portal/email.html"));
        let f = fill(&mut s);
        let at = f.rows.iter().position(|r| f.field(*r).is_some_and(|x| x.kind == Kind::Radio)).unwrap();
        f.sel = at;
        assert_eq!(field_value(f.selected().unwrap()), "Choose");
        let _ = s.press(Button::Right);
        assert_eq!(field_value(fill(&mut s).selected().unwrap()), "I am 16 or older");
        let _ = s.press(Button::East);
        assert_eq!(field_value(fill(&mut s).selected().unwrap()), "I am younger than 16");
        let _ = s.press(Button::Left);
        assert_eq!(field_value(fill(&mut s).selected().unwrap()), "I am 16 or older");
    }

    #[test]
    fn what_was_typed_is_wiped_and_the_pages_own_values_stay() {
        let mut s = filling("<form><input type=hidden name=t value=tok><input name=room><input type=checkbox name=c value=yes><input type=submit></form>");
        type_text(&mut s, "412");
        let f = fill(&mut s);
        f.page.forms[0].wipe();
        let vals: Vec<&str> = f.page.forms[0].fields.iter().map(|x| x.value.as_str()).collect();
        assert_eq!(vals, ["tok", "", "yes", ""]);
        // Closing with the keyboard open: its text is wiped too (`Drop`), and nothing panics.
        let _ = s.press(Button::East);
        drop(s);
    }

    #[test]
    fn every_end_says_what_happened_and_offers_a_way_on() {
        let mut s = SignIn::new("portal.example", 1);
        s.on_outcome(Outcome::NeedsBrowser { why: NoForm::NeedsScripts, page: Some(parse(include_str!("../testdata/portal/js-only.html"))) }, false);
        assert_eq!(s.state_end(), Some(End::NeedsBrowser(NoForm::NeedsScripts, "WIFI@DB".into())));
        assert!(matches!(s.press(Button::East), Ask::OtherWays));
        s.on_outcome(Outcome::Failed(Failure::Unreachable), false);
        let rows: Vec<&str> = s.end_rows().iter().map(|(n, _)| *n).collect();
        assert_eq!(rows, ["Try again", "Other ways"]);
        assert!(matches!(s.press(Button::East), Ask::Reload));
        s.on_outcome(Outcome::ElseWhere("collect.example".into()), false);
        assert_eq!(s.state_end(), Some(End::ElseWhere("collect.example".into())));
        assert!(matches!(s.press(Button::South), Ask::Back));
    }

    #[test]
    fn after_sending_a_new_form_is_the_next_step_and_too_many_end() {
        let html = "<form><input name=code required><input type=submit></form>";
        let mut s = filling(html);
        type_text(&mut s, "1234");
        let _ = s.press(Button::Down);
        assert!(matches!(s.press(Button::East), Ask::Send { .. }));
        s.sending();
        let page = parse(&format!("<p>Wrong code.</p>{html}"));
        let forms = page.usable();
        s.on_outcome(Outcome::Form { page, url: "http://portal.example/".into(), forms }, false);
        assert_eq!(fill(&mut s).note.as_deref(), Some("The network still wants a sign-in. Check what the page says."));
        assert_eq!(fill(&mut s).page.text, "Wrong code.");
        assert_eq!(fill(&mut s).page.forms[0].fields[0].value, "", "a new page starts empty");
        for _ in 0..MAX_STEPS {
            fill(&mut s).steps = MAX_STEPS;
        }
        s.sending();
        let page = parse(html);
        s.on_outcome(Outcome::Form { forms: page.usable(), page, url: "http://portal.example/".into() }, false);
        assert_eq!(s.state_end(), Some(End::TooManySteps));
        // Sent, then nothing to fill in and not online: still wants.
        let mut s = filling(html);
        let _ = s.press(Button::Down);
        fill(&mut s).page.forms[0].fields[0].value = "x".into();
        s.sending();
        s.on_outcome(Outcome::NeedsBrowser { why: NoForm::Nothing, page: None }, false);
        assert_eq!(s.state_end(), Some(End::StillWants));
    }

    impl SignIn {
        fn state_end(&self) -> Option<End> {
            match &self.state {
                State::Ended(e) => Some(e.clone()),
                _ => None,
            }
        }
    }
}
