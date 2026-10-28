//! Settings → Tools: the state of the page and what a press asks for. The App finds the tools on a thread
//! (`tools::scan`), starts one as an app or runs a Store tool quietly, and reports back with an `Event`; painting is in
//! the binary (`app_tools.rs`). Nothing here touches the system, so every path is a plain test.

use crate::pad::Button;
use crate::tools::{Risk, Run, Tool};

/// News for the page from its threads.
#[derive(Debug)]
pub enum Event {
    /// The tools were found (`tools::scan`).
    Scanned(Vec<Tool>),
    /// A Store tool ran (`tools::run_quiet`): its name and its last line, or why it failed.
    Ran { name: String, result: Result<String, String> },
}

/// What a press asks the App to do.
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Nothing,
    Close,
    /// Look for the tools again.
    Rescan,
    /// Start this tool as an app (`Run::App`).
    Launch(Tool),
    /// Run this Store tool without a window (`Run::Quiet`).
    RunQuiet(Tool),
}

#[derive(Default)]
pub struct ToolsPage {
    /// None while the first scan runs.
    pub tools: Option<Vec<Tool>>,
    pub sel: usize,
    /// The row whose warning is showing: the next A starts it, anything else cancels.
    pub confirm: Option<usize>,
    /// A Store tool runs (its row).
    pub busy: Option<usize>,
    /// The last result: text, is an error.
    pub message: Option<(String, bool)>,
}

impl ToolsPage {
    pub fn new() -> ToolsPage {
        ToolsPage::default()
    }

    pub fn len(&self) -> usize {
        self.tools.as_ref().map_or(0, Vec::len)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn selected(&self) -> Option<&Tool> {
        self.tools.as_ref()?.get(self.sel)
    }

    pub fn press(&mut self, b: Button, confirm: Button, cancel: Button) -> Action {
        let n = self.len();
        let before = self.sel;
        match b {
            b if b == cancel => {
                if self.confirm.take().is_some() {
                    return Action::Nothing;
                }
                return Action::Close;
            }
            Button::Up if n > 0 => self.sel = (self.sel + n - 1) % n,
            Button::Down if n > 0 => self.sel = (self.sel + 1) % n,
            Button::L1 => self.sel = self.sel.saturating_sub(5),
            Button::R1 if n > 0 => self.sel = (self.sel + 5).min(n - 1),
            Button::North if self.busy.is_none() => {
                self.confirm = None;
                self.message = None;
                return Action::Rescan;
            }
            b if b == confirm => return self.start(),
            _ => {}
        }
        if self.sel != before {
            self.confirm = None;
            self.message = None;
        }
        Action::Nothing
    }

    /// A on the selected tool: a risky one shows its warning first; a Store tool that runs blocks the others.
    fn start(&mut self) -> Action {
        if self.busy.is_some() {
            return Action::Nothing;
        }
        let Some(tool) = self.selected().cloned() else { return Action::Nothing };
        if matches!(tool.risk, Risk::Warn(_)) && self.confirm != Some(self.sel) {
            self.confirm = Some(self.sel);
            return Action::Nothing;
        }
        self.confirm = None;
        self.message = None;
        match tool.run {
            Run::App => Action::Launch(tool),
            Run::Quiet => {
                self.busy = Some(self.sel);
                Action::RunQuiet(tool)
            }
        }
    }

    pub fn event(&mut self, e: Event) {
        match e {
            Event::Scanned(tools) => {
                // The same tool stays selected when the list changes under it.
                let was = self.selected().map(|t| t.script.clone());
                self.sel = was.and_then(|s| tools.iter().position(|t| t.script == s)).unwrap_or(0).min(tools.len().saturating_sub(1));
                self.confirm = None;
                self.tools = Some(tools);
            }
            Event::Ran { name, result } => {
                self.busy = None;
                // The tool's name is the heading above the message already.
                println!("tools: {name} said {result:?}");
                self.message = Some(match result {
                    Ok(line) => (line, false),
                    Err(e) => (e, true),
                });
            }
        }
    }

    /// The hint for A.
    pub fn a_hint(&self) -> &'static str {
        match self.selected() {
            None => "Start",
            Some(_) if self.confirm == Some(self.sel) => "Start anyway",
            Some(t) if t.run == Run::Quiet => "Run",
            Some(_) => "Start",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::{demo, Group};

    const A: Button = Button::East;
    const B: Button = Button::South;

    fn page() -> ToolsPage {
        let mut p = ToolsPage::new();
        p.event(Event::Scanned(demo()));
        p
    }

    fn at(p: &mut ToolsPage, name: &str) {
        p.sel = p.tools.as_ref().unwrap().iter().position(|t| t.name == name).unwrap();
    }

    #[test]
    fn a_safe_tool_starts_with_one_press_a_risky_one_with_two() {
        let mut p = page();
        at(&mut p, "Dolphin");
        assert!(matches!(p.press(A, A, B), Action::Launch(t) if t.name == "Dolphin"));
        at(&mut p, "Cloud backup");
        assert_eq!(p.press(A, A, B), Action::Nothing, "the warning first");
        assert_eq!(p.confirm, Some(p.sel));
        assert_eq!(p.a_hint(), "Start anyway");
        assert!(matches!(p.press(A, A, B), Action::Launch(t) if t.name == "Cloud backup"));
        assert_eq!(p.confirm, None);
    }

    #[test]
    fn b_or_moving_takes_the_warning_back_and_b_then_leaves() {
        let mut p = page();
        at(&mut p, "Remove ._ files");
        p.press(A, A, B);
        assert_eq!(p.press(B, A, B), Action::Nothing);
        assert_eq!(p.confirm, None);
        p.press(A, A, B);
        p.press(Button::Down, A, B);
        assert_eq!(p.confirm, None, "moving away cancels");
        assert_eq!(p.press(B, A, B), Action::Close);
    }

    #[test]
    fn a_store_tool_runs_quietly_and_says_what_it_printed() {
        let mut p = page();
        at(&mut p, "Storage report");
        assert!(matches!(p.press(A, A, B), Action::RunQuiet(t) if t.group == Group::Store));
        assert_eq!(p.press(A, A, B), Action::Nothing, "one at a time");
        assert_eq!(p.press(Button::North, A, B), Action::Nothing, "no rescan while it runs");
        p.event(Event::Ran { name: "Storage report".into(), result: Ok("Free: 12 GB".into()) });
        assert_eq!(p.busy, None);
        assert_eq!(p.message, Some(("Free: 12 GB".into(), false)));
    }

    #[test]
    fn a_new_list_keeps_the_selected_tool() {
        let mut p = page();
        at(&mut p, "Terminal");
        let mut fewer = demo();
        fewer.retain(|t| t.name != "Dolphin");
        p.event(Event::Scanned(fewer));
        assert_eq!(p.selected().unwrap().name, "Terminal");
        p.event(Event::Scanned(Vec::new()));
        assert_eq!((p.sel, p.selected()), (0, None));
        assert_eq!(p.press(A, A, B), Action::Nothing);
        assert_eq!(p.press(Button::Down, A, B), Action::Nothing);
    }
}
