//! Android presentation of the shared Linux Updates actions and signed offers.
use super::phone::{MenuRow, Room};
use super::*;
use pocket_ui::{update_channel::Manifest, update_page::{self, Action as UpdateAction}};
pub const CONFIRM: &str = "After updating, pocket-ui closes. Open it again; your games and settings stay.";

#[derive(Default)]
pub struct UpdatesUi {
    pub version: String,
    pub manifest: Option<Manifest>,
    pub hint: pocket_ui::update_hint::State,
    pub checking: bool,
    pub phase: String,
    pub bytes: u64,
    pub total: u64,
    pub cancellable: bool,
    pub message: String,
    pub confirming: bool,
}
impl UpdatesUi {
    pub fn available(&self) -> Option<&pocket_ui::update_channel::Release> {
        update_page::available(self.manifest.as_ref(), &self.version, true)
    }
    fn install_active(&self) -> bool {
        matches!(self.phase.as_str(), "connecting" | "downloading" | "checking" | "permission" | "installing")
    }
    pub fn busy(&self) -> bool {
        self.checking || self.install_active()
    }
    fn actions(&self) -> Vec<UpdateAction> { update_page::actions(self.available().is_some(), self.confirming) }
    pub fn checked(&mut self, result: Result<Manifest, String>) {
        self.checking = false;
        self.confirming = false;
        match result {
            Ok(m) => { self.manifest = Some(m); self.message = if self.available().is_none() { update_page::UP_TO_DATE.into() } else { String::new() }; }
            Err(why) => self.message = why,
        }
    }
    fn words(&self) -> String {
        if self.checking { return tr("Checking for updates"); }
        if self.phase == "downloading" {
            return tr(&format!("Downloading the update… {}%", (self.bytes.saturating_mul(100) / self.total.max(1)).min(100)));
        }
        if !self.message.is_empty() { return tr(&self.message); }
        if self.phase == "connecting" { return tr("Connecting to the download server…"); }
        if self.phase == "checking" { return tr("Checking the download…"); }
        if self.confirming { return tr(CONFIRM); }
        self.available().map(|r| r.notes.clone()).unwrap_or_default()
    }
}
impl Ui {
    pub(super) fn launch_game(&mut self) -> Command {
        if self.updates.install_active() {
            self.navigate(Screen::Updates);
            Command::Notice("Finish or cancel the update before starting a game.")
        } else { Command::Launch }
    }
    pub(super) fn open_updates(&mut self) -> Command {
        self.navigate(Screen::Updates);
        if self.updates.busy() || self.updates.manifest.is_some() { Command::None }
        else { self.updates.checking = true; self.updates.message.clear(); Command::UpdateCheck }
    }
    pub fn updates_checked(&mut self, result: Result<Manifest, String>) {
        self.updates.checked(result);
        if self.screen == Screen::Updates { self.menu = 0; self.follow = true; }
    }
    pub(super) fn updates_rows(&self) -> Vec<MenuRow> {
        self.updates.actions().into_iter().map(|a| MenuRow {
            label: tr(&a.label(self.updates.available().map(|r| r.version.as_str()))),
            value: if a == UpdateAction::Auto { tr(if self.updates.hint.off { "Off" } else { "On" }) } else { String::new() },
            check: None, stepper: false,
        }).collect()
    }
    pub(super) fn updates_press(&mut self, b: Button) -> Command {
        match b {
            Button::Up | Button::Left => self.move_by(-1),
            Button::Down | Button::Right => self.move_by(1),
            Button::South if self.updates.busy() && self.updates.cancellable => {
                self.updates.message = "Cancelling…".into();
                return Command::UpdateCancel;
            }
            Button::South if self.updates.confirming => { self.updates.confirming = false; self.menu = 0; }
            Button::South | Button::Start => self.navigate(Screen::Settings),
            Button::East => {
                let Some(action) = self.updates.actions().get(self.menu).copied() else { return Command::None };
                if action == UpdateAction::Auto {
                    self.updates.hint.set_auto(self.updates.hint.off);
                    return Command::UpdateAuto;
                }
                if self.updates.busy() { return Command::None; }
                self.updates.message.clear();
                match action {
                    UpdateAction::Check => { self.updates.checking = true; return Command::UpdateCheck; }
                    UpdateAction::InstallApp => { self.updates.confirming = true; self.menu = 0; }
                    UpdateAction::InstallNow => {
                        self.updates.confirming = false;
                        self.updates.phase = "connecting".into(); self.updates.bytes = 0;
                        self.updates.total = self.updates.available().map_or(0, |a| a.size);
                        self.updates.cancellable = true;
                        return Command::UpdateInstall;
                    }
                    UpdateAction::Later => { self.updates.confirming = false; self.menu = 0; }
                    UpdateAction::Auto => {}
                }
            }
            _ => {}
        }
        Command::None
    }
    pub(super) fn paint_updates(&mut self, c: &mut Canvas, text: &mut Text, room: Room) {
        let size = self.sp(15.0);
        let step = size as i32 + self.d(7.0);
        let w = c.w - 2 * room.margin;
        let version = match self.updates.available() {
            Some(a) => format!("pocket-ui {}", tr(&format!("{}, {} available", self.updates.version, a.version))),
            None => format!("pocket-ui {}", self.updates.version),
        };
        let words = self.updates.words();
        let mut y = room.top + self.d(4.0);
        for line in text.wrap(Face::Body, size, &version, w) {
            text.draw(c, Face::Body, size, room.margin, y + size as i32, pal().dust, &line); y += step;
        }
        self.target(Area::new(room.margin, room.top, w, y - room.top), &version, Action::Label);
        if self.updates.total > 0 && self.updates.busy() {
            let frac = (self.updates.bytes as f32 / self.updates.total as f32).clamp(0.0, 1.0);
            let h = self.d(5.0).max(2);
            c.rect(room.margin, y, w, h, pal().pill);
            c.rect(room.margin, y, (w as f32 * frac) as i32, h, pal().label);
            y += h + self.d(8.0);
        }
        let top = y;
        let most = (((room.bottom - room.top) / 2 - (y - room.top)) / step).max(2) as usize;
        let lines: Vec<_> = words.lines().flat_map(|line| text.wrap(Face::Body, size, line, w)).collect();
        for line in lines.iter().take(most) {
            text.draw(c, Face::Body, size, room.margin, y + size as i32, pal().chalk, line); y += step;
        }
        self.cut.extend(lines.iter().skip(most).cloned());
        if !words.is_empty() { self.target(Area::new(room.margin, top, w, (y - top).max(step)), &words, Action::Label); }
        let rows = self.updates_rows();
        let cancelling = self.updates.busy() && self.updates.cancellable;
        let menu_bottom = room.bottom - if cancelling { self.d(52.0) } else { 0 };
        self.paint_menu_phone(c, text, Room { top: y + self.d(8.0), bottom: menu_bottom, ..room }, rows);
        if cancelling {
            // Always reachable with touch as well as B; download cancellation does not navigate away.
            let bh = self.d(44.0);
            let area = Area::new(c.w - room.margin - self.d(112.0), room.bottom - bh, self.d(112.0), bh);
            self.pill(c, text, area, "Cancel", false, Action::Press(Button::South));
        }
    }
}
