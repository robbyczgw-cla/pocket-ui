# One control scheme

Every page a player reaches works the same way: the first start, every Settings page and the setup pages it opens,
the pause menu, quick settings, dialogs. A page that breaks a rule here is a bug. How the parts look is in
[DESIGN.md](DESIGN.md); this page is how they behave. The navigation explorer (`tests/nav_explorer.rs`, `nav_probe`)
checks the rules on every page it can reach.

## Keys

- **Up/Down** move the focus from row to row. Every control drawn on the page is a row Up/Down reaches: a list row,
  a row of choices (pills), the looks' carousel, a button such as "Continue". Nothing on the page is only for show
  and still looks like a control.
- **Left/Right** change the value of the focused row (a choice, a switch, the look shown). On a row without a value
  they do nothing; inside a grid they move.
- **A** opens or confirms the focused row: a sub-page, the network to connect to, "Continue". On a page that is one
  choice (the look, a dialog) A keeps it and leaves.
- **B** goes back one step: to the parent page, the stage before (first start), or out of a sheet or a menu, and
  does nothing else on the way (what was chosen stays chosen). A page that previews a choice live (Settings › Look ›
  Layout) puts it back on B and says "Cancel". The first page of the first start has nothing before it; opened again
  from Settings, B goes back there.
- **Shortcut keys** (X, Y, L1/R1, L2, START, SELECT) are extras for what a row already does: X scans again beside the
  Scan again row, Y forgets beside the network's menu, Y switches For a child beside its row. Never the only way to
  anything. The hint bar names them when there is room (four hints at most, the focused row's keys first). Kept for
  the whole first start: START skips the rest of it, L2 switches Big text (its key is on the top row). L1/R1 page
  through a list longer than the screen.

## What a page shows

- **Focus is visible:** the focused list row is on the selection's paper (`kit::shelf_row`, `kit::selection`); a row
  of pills, the looks' carousel and the welcome's rows have a bar in the margin beside them (`setup::focus_bar`).
- **The hint bar** names the keys that work on the focused row, each with its word ("Left/Right Change", "A Open",
  "B Back"), and no key that does nothing there.
- **Rows:** the name at the left, the value at the right (`‹ Value ›` on the focused row when Left/Right change it),
  or pills for a few choices with the chosen one on paper and one line about it beside them.
- **Help lines** stand under the rows or over them, in the quiet colour, one or two lines. On a short screen they give
  way before the rows do: rows never run under the hint bar.
- **The way on** in a stage is a row or A itself (Continue, Continue offline, Looks right, I'll add games later,
  Finish), never B.

## Not yet following it

- The keyboards (password, network name, time zone search) keep their own keys (A types, B deletes, START sends).
- The PortMaster browser names L1/R1 (page) also where its list fits one screen.
