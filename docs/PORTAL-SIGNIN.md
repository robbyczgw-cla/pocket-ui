# Wi-Fi with a sign-in page (hotel, train, café)

Settings → Connections → Wi-Fi on a network that wants a sign-in before anything goes online (a "captive portal").
The research behind it, with what other systems do: `RESEARCH-CAPTIVE-PORTAL-2026-10-26.md` (review folder). The user's
side: [user/hotel-wifi.md](user/hotel-wifi.md).

## What it does

- **Notices it.** `net_check` sends one plain request to `nmcheck.gnome.org` ([SECURITY-HARDENING.md](SECURITY-HARDENING.md#the-sign-in-check));
  a redirect or a page of the network's own is a sign-in. The redirect's address is kept (`net_check::Check::portal`:
  http or https only, no control characters, at most 2 KB).
- **Says it calmly, on a row of its own.** The Wi-Fi page's first row while connected to such a network is "This network
  wants a sign-in", with the sign-in page's host on the right; it is selected when it appears (`setup::relead`), and the
  networks below keep their selection. The line under the title stays "Connected to …".
- **The sheet** (`portal_page.rs`): what the network wants, "Sign-in page: <host>" (or "The network answers every address
  with its own page." when the check was not redirected), that pocket-ui checks again every 15 seconds, and the rows
  **Sign in…** (below) and **Other ways**: a phone's hotspot, Wi-Fi sharing from a phone, asking the front desk with this Pocket's Wi-Fi address
  (`wifi::mac`, `/sys/class/net/wlan0/address`, in capitals and Title 50; "couldn't be read" in its place when it
  can't be), and a travel router. The list scrolls (Up/Down) when Large text or German make it taller than the screen.
- **Re-checks by itself** (`setup::keep_watch`): while the Wi-Fi page or the sheet is on screen and shows a sign-in, a
  thread sends the same request every 15 s (`portal_page::RECHECK_SECS`, `net_check::recheck`). It stops with an answer
  that isn't a sign-in (online, unreachable) and when the page closes: it holds only a `Weak` of a token the page owns,
  so leaving the step or closing Settings (the setup is dropped) ends it within 0.2 s. "Unknown" (curl missing, an odd
  answer) leaves the page as it is and keeps checking. Online after a sign-in: the row goes, and the page says "You're
  online now. The network lets this Pocket in."; the sheet's title turns to "You're online".

## The sign-in helper ("Sign in…")

No browser engine: pocket-ui loads the sign-in page itself, reads its form and shows it in its own style
(`portal.rs` the requests, `html_form.rs` the reader, `portal_form.rs` the page).

- **Loading** (`portal::Session::open`, on a thread): from the address the check was redirected to (else the check's own
  address, which such a network answers with its page). Redirects are followed one by one (at most 8 together: HTTP,
  `<meta http-equiv="refresh">` and the "please wait" patterns below), each address checked before it is asked (rules
  below). Cookies are kept in a jar of this session only. A 511 or any other status with an HTML page is read like a
  200 (portals answer with odd statuses).
- **"Please wait" pages** (`Session::follow`): many portals show a small interstitial that forwards the browser to the
  real sign-in form ("Internet Access - please wait.", "Connecting…"). The helper does what the browser would, without
  running one line of the page's script: the link in a `<noscript>` block is preferred (it is what a browser without
  scripts is offered, and often the real one); a form the page submits **on load** whose fields are all hidden is sent
  exactly as the page carries it (method, action, hidden values — nothing typed, ticked or accepted), and only where a
  form may go (rules below). "On load" is read out of the code, not guessed: the `submit()` must sit in an
  `onload` attribute, in a `setTimeout`/`setInterval`, in a `load`/`DOMContentLoaded` handler, in jQuery's ready or at
  the top level of a script — never in a click handler, a named function or `submit(function(){…})` (which binds a
  handler and does not submit) — and the page must have no button and no action link of its own (an "Accept and
  connect" page is never sent by itself). A script or `onload` handler that goes to one literal address
  (`window.location = "…"`, `location.href = "…"`, `document.location…`, `top.location…`, `location.replace(…)`,
  `location.assign(…)`, and the same inside `setTimeout(function(){ location… = "…" }, n)`),
  `<meta http-equiv="refresh">` (odd spacing, quotes and case included) and a page that is a single `<iframe src>`
  are followed as the addresses they name. Only literal addresses: anything built at run time (a `+`, a variable) is
  not followed. A `<noscript>` link or a frame is followed only on the page's own site or the network's own gateway:
  a tracker's frame (the Google Tag Manager snippet) or an "enable JavaScript" link somewhere else is never loaded and
  never joins the hosts a form may go to. Each hop costs one of the 8 redirects, so a loop ends as before.
- **Reading** (`html_form::parse`): one pass, no tree, no scripts run. The title and first `<h1>`/`<h2>` (the page's
  name), the visible text (one paragraph a line, 12,000 characters at most; label words and scripts left out), and up
  to 8 forms of up to 64 fields: hidden fields (sent as they are), text, e-mail, number, telephone and password
  fields, check boxes, radio groups (one choice), selects, text areas, submit and image buttons, `<button>`. Labels
  from `<label for>`, a label around the field, `aria-label`, the text just before the field, `placeholder`, `title`.
  `<noscript>` content counts (it is what a browser without scripts shows). Latin-1 pages are decoded as such.
- **Usable form:** an action that isn't `javascript:`, and a button that sends it (or, on a page without scripts, at
  least one field). No usable form: "This page needs a full browser", with the reason: scripts (the page is an app, or
  its form needs them), nothing to fill in (links only: social login, payment), or the page's own form carries what it
  holds to an address the helper never sends to (said as what it is; only a form a person pressed is refused as
  "Not sent").
- **The page** (`portal_form`): the page's name and host, the first three lines of its text and **Read the whole
  page**, then one row per field and button. Text fields open the on-screen keyboard (a password as dots but the last
  letter, `maxlength` kept), A ticks a box, Left/Right/A changes a choice, A on a button sends. A required field or box
  left empty stops the send with "Fill in “…” first." / "Tick “…” first." and selects it. The selected row's whole
  label is written out below the rows when it was cut short.
- **Sending** (`portal::Session::submit`, on a thread, only on the button's press): HTML's rules for what a form sends
  (`html_form::form_data`), `application/x-www-form-urlencoded` (or `multipart/form-data` when the form says so) in
  UTF-8, GET forms in the address, with the page as referer and the session's cookies. Then redirects as above (always GET,
  without the data, also for a 307/308), and the check: online (asked twice, 2 s apart) is "You're
  online"; a page with a form again is the next step or the page's own error ("The network still wants a sign-in.
  Check what the page says.", at most 5 steps); nothing to fill in and not online is "Still not online" with Try again.
  An answer that comes back online also turns the Wi-Fi page online and makes the next update or store check ask again.
- **When it gives up**: an end that says "This page needs a full browser" saves the last page the network served (the
  HTML only, no cookies or headers, at most 200 KB, query values that look like a MAC or IP address redacted, the
  `value` of every hidden field blanked — the field names stay) as `logs/portal-last.html` in this boot's log folder;
  the next "Report a problem" takes it along (cleaned again like every file in the report), so the problem can be seen
  on a computer. No page is saved once a person's form was sent: an answer that echoes what was typed must never reach
  a file.

## Security rules and their tests

| Rule | Where | Tested by |
|---|---|---|
| Only http and https; no `user@` in an address; `javascript:`, `file:`, `data:`, `mailto:` never followed or sent to | `portal::Url::parse`, `Url::join` | `portal::tests::only_web_addresses_without_a_user_name_are_taken`, `links_resolve_as_in_a_browser_and_other_schemes_never`, `the_pocket_itself_is_refused_before_curl_runs` (a redirect to `file:`) |
| Never the Pocket itself (127/8, ::1, a mapped one), 0.0.0.0, multicast, broadcast: each host is looked up here and pinned with `--resolve`, so curl can't be steered elsewhere | `portal::forbidden`, `Session::request` | `the_pocket_itself_and_nowhere_are_never_asked`, `the_pocket_itself_is_refused_before_curl_runs` (curl never runs), `portal_e2e` (the real rules refuse the stand-in on 127.0.0.1) |
| A form goes only to its own action, and only to a host this session loaded a page from or an address of a private network (the network's gateway); anything else is "Not sent" | `Session::allowed`, `Session::submit` | `a_form_goes_only_to_its_own_site_and_typed_data_never_onto_a_command_line`, `portal_e2e::pages_the_helper_cant_do_end_plainly` (nothing reaches the stand-in) |
| A "please wait" page is followed only as navigation: literal web addresses with the same rules as links, one of the 8 hops each, and an all-hidden form the page submits on load only where a form may go | `html_form::script_redirect`, `Page::auto_form`, `Page::noscript_link`, `Session::follow` | `a_please_wait_page_leads_on_to_the_form_without_running_scripts`, `the_noscript_link_the_meta_refresh_and_a_single_frame_lead_to_the_form`, `hostile_interstitials_loop_or_are_refused_and_send_nothing` (a loop, `file:`/`javascript:`, a form to another public host) |
| Nothing is sent by itself on a page built around a press: the submit must run on load (not in a click handler or a named function) and the page must have no button or action link of its own | `html_form::submits_on_load`, `Reader::press`, `Page::auto_form` | `a_submit_that_only_runs_on_a_press_never_auto_submits` (the "Accept and connect" page), `a_form_the_page_submits_on_load_is_the_one_with_only_hidden_fields` |
| A `<noscript>` link or a frame is followed only on the page's own site or a private address: a tracker's frame or a help link somewhere else is never loaded and never joins the hosts a form may go to | `Session::follow`, `Reader`'s iframe reading | `a_tracker_frame_or_a_help_link_off_site_is_never_followed` |
| What a person typed never reaches the saved page or the report: no page is kept from a chain that carries a person's form | `Session::submit`, `Session::last_page` | `nothing_a_person_typed_is_kept_for_the_report` |
| Nothing is sent without the button's press; nothing is ticked by itself | `portal_form::Filling::press` | `portal_form::tests::a_hotel_page_is_filled_in_and_sent_only_on_its_button` |
| What was typed never on a command line or in a file: curl reads its whole request from a pipe (`-K -`), config values quoted so nothing typed can start a line of its own | `Session::request`, `portal::quoted` | `curl_config_values_cannot_start_a_line_of_their_own`, the submit test (the data is in the piped config) |
| What was typed is overwritten when the page closes and after it was sent (best effort: copies the allocator moved are not reached) | `portal::wipe`, `Form::wipe`, `Filling`'s drop, `setup::portal_send` | `a_typed_value_is_overwritten`, `what_was_typed_is_wiped_and_the_pages_own_values_stay` |
| Cookies in a private folder (0700, `XDG_RUNTIME_DIR` or `/tmp`), removed with the session (leaving "Sign in…") | `Session::new`, `Drop` | `the_cookie_jar_is_private_and_goes_with_the_session` |
| A hostile page costs a bounded time and memory and never panics (release builds abort on a panic): one linear pass, caps on text, forms, fields, options, attributes, values; 1 MiB a page, 10 s a request (curl ended at 13 s), 8 redirects, 12 s to look a name up (more than glibc's 5 s before it asks the DNS server again) | `html_form`, `Session::run` | `garbage_never_panics_and_stays_within_its_limits` (400 random pages, cut-off tags), `huge_and_deep_pages_are_one_pass_and_bounded` (200,000 nested divs, 100,000 attributes, 10,000 options), `endless_redirects_end`, `a_huge_page_and_a_hanging_curl_cost_a_bounded_time` |
| No proxy from the environment (`noproxy`, the proxy variables removed), only http and https even inside curl (`proto`), no user's curl config (`-q`); a fixed user agent that names pocket-ui | `Session::run`, `Session::request` | the submit test (`noproxy`, `proto`) |
| A certificate that can't be checked (exit 60): loaded once more without the check, and said in red on the page | `Session::request` | not with a real certificate (see Limits) |
| Page text is painted as it is (never through the translation table), without control characters or invisible direction marks | `html_form::clean`, `text.set_translate(false)` | `the_page_text_title_and_heading_are_plain` |

## What it never does

- Accept terms, tick a box, press a button or send anything a person typed by itself. The one thing it sends on its
  own is a form the page itself submits on load and whose fields are all hidden — what a browser sends the moment it
  opens that page (no person's data and no choice in it), only when the submit truly runs on load and the page has no
  button or action link of its own, and only where a form may go. The re-check sends only the
  `nmcheck.gnome.org` request.
- Run a script of the page, show a URL bar or follow a link the person chose: it is not a browser.
- Keep what was typed: no room number, name, e-mail address, code or password is saved, logged or sent anywhere but
  the form's own action. No page is kept for the report once a form was sent (an answer that echoes it is never saved).
- Run a timer outside the Wi-Fi page, or keep asking once the network answered online.
- Change the Pocket's Wi-Fi address (no MAC cloning) or offer a relay through a phone.

## Limits

- Pages that need scripts don't work: JavaScript apps (WIFI@DB on Deutsche Bahn trains and stations, which logs in
  through a JSON call), forms whose button runs a script, social login, payment, an SMS code sent by the page's
  script. Telekom's WISPr hotspots need an account. Each ends with "This page needs a full browser" and the other
  ways. Recipes for big operators (as OpenWrt's Travelmate has) are not built.
- What a "please wait" page does is read out, not run: one that computes its address (`location.href = base + x`) or
  fills its form in a script is not followed and ends with "This page needs a full browser" too. So does a page that
  has a button or action link of its own ("Accept and connect"): nothing is ever sent by itself there.
- A form that posts to a different public site than the one that served it is refused ("Not sent"), although some
  real portals do that; safer than the alternative.
- Forms are sent in UTF-8 whatever the page's `accept-charset`; a Latin-1 portal may get a name with umlauts wrong.
- A page whose form has an `onsubmit` script is sent without running it; if the script added something the portal
  needs, the answer is the page again or "Still not online".
- An https page with a self-signed certificate is loaded anyway and marked; the curl on the device and that path were
  not tried with a real portal's certificate.
- One request every 15 s while the page is open on a sign-in network; nothing when it is closed.
- The Wi-Fi address shown is the chip's own: ROCKNIX's iwd keeps it (no `AddressRandomization`). Not checked on a device
  whose system turned address randomisation on.

## Testing

**Host tests.** `html_form` (labels from every place a page puts them, choices and radio groups, what a form sends,
encodings, a page that needs scripts or has only links, `<noscript>` forms, refresh and script redirects, the
interstitial patterns read out of scripts, timers, `onload` and odd `<meta refresh>` syntax, the all-hidden form the
page submits on load, garbage and huge pages), `portal` (addresses, links, forbidden and private addresses, curl's
config, headers and body, a stand-in curl for endless redirects, a huge page, a hanging curl, garbage, the Pocket
itself, a form to another site, the cookie jar; the interstitial fixtures end to end against the stand-in curl: the
timer's page, the hidden form's request, the `<noscript>` link preferred, the meta refresh, the frame, and the hostile
ones — a loop, `file:`/`javascript:`, a form to another public host, the "Accept and connect" page whose submit runs
only on the press, a Google Tag Manager page), the saved last page and its redaction (percent-encoded and bare
addresses, hidden values blanked, private keys), and that no page is kept after a person's form was sent),
`portal_form` (a hotel page filled in and sent only on its button, required fields, `maxlength`, dots for a
password, choices, the ends, the next step, the steps' limit, the wipe), `portal_page` (Sign in… asks for the page, a
late answer is dropped), `logs` (the saved page moves into the boot's folder), `report` (it goes into the bundle
cleaned, and the cleaner knows addresses in every form and hidden values). Fixtures modelled on common portals:
`testdata/portal/` (click-through, room and surname, voucher with a select and an image button, e-mail with a radio
group, a JavaScript-only page, links only, and the interstitials:
`interstitial-js.html` a "please wait" timer, `interstitial-wait.html` a Cisco/Aruba-style auto-submit form,
`interstitial-noscript.html` a `<noscript>` fallback, `interstitial-iframe.html` a single-frame page,
`interstitial-meta.html` a refresh with odd syntax, `hostile-loop-1/2.html` a redirect loop, `hostile-scheme.html`
`file:`/`javascript:` addresses, `hostile-autosubmit.html` a form to another public host,
`hostile-accept-button.html` an "Accept and connect" page, `hostile-gtm.html` a tracking frame and a help link).

**End to end** (`tests/portal_e2e.rs`): `scripts/portal-standin.py` is a small portal on 127.0.0.1 (redirect of the
check's address, a session cookie, a second redirect, the page; the form's answer lets the client in, then the check's
address answers "NetworkManager is online"). The test drives the real `net_check::check_with`, `portal::Session` and
the real curl, fills the form in through `portal_form`'s presses: a wrong surname brings the page back with its
message, the right one goes online; a click-through page goes online with one button (a GET form); a "please wait"
interstitial (the `interstitial` mode) forwards from its timer's page to the hidden form's page and goes online
without anything filled in; a JavaScript-only page ends with "needs a full browser", endless redirects end, a form
that posts to another site is never sent, and the real rules refuse 127.0.0.1. The only difference from the device:
`Rules::allow_loopback`. Try it by hand:
`python3 -I scripts/portal-standin.py hotel testdata/portal /tmp/port` and `curl -i http://127.0.0.1:$(cat /tmp/port)/check_network_status.txt`.
No openNDS or other portal software was run (it needs root and a network of its own on the build machine).

**On a Wi-Fi network** ([WIFI-BENCH.md](WIFI-BENCH.md), scenario `portal`): the same stand-in behind a simulated café
access point, reached the way a real portal is (the network's DNS answers every name with its gateway, which forwards
port 80 to the stand-in; `PORTAL_STANDIN_BASE` makes the redirects name `login.wb-hotel.example`), with the real
pocket-ui, ROCKNIX's NetworkManager, iwd and resolved: the row, the sheet, Sign in…, Send and online, driven through
the Wi-Fi page.

**Snapshots:** `setup:wifi-portal-signin=<what>[:<keys>]` (`portal_form::demo`: hotel, clickthrough, voucher, email, js,
social, elsewhere, still, toomany, failed, online, loading).

### Step 1

`net_check` (the address kept from a redirect, only a web address, the host of an address), `wifi::parse_mac`,
`portal_page` (the sheet's lines, Other ways and back, a re-check's answer), `setup` (the row comes first and opens the
sheet, the selection follows, the re-check turns the page online without a press and stops, also when the page
closes; a missing Wi-Fi address). Snapshots: `setup:wifi-portal`, `setup:wifi-portal-sheet[:da]`,
`setup:wifi-portal-sheet-nomac:da`, dark and light, Normal and Large, English and German (`tests/i18n-states.txt`).
