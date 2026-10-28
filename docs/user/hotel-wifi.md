# Hotel and train Wi-Fi

Hotel, train, airport and café networks often show a page first: accept their terms, type a room number and
surname, or a voucher code. Until that page is done, nothing online works on that network (cover art, the store,
updates, RetroAchievements), even though the Wi-Fi itself is connected.

## What the Pocket shows

Settings → Connections → **Wi-Fi**, while you are connected to such a network:

- The first row of the list says **This network wants a sign-in**, with the sign-in page's address on the right
  (for example `login.hotel-seeblick.example`). It is selected when it appears, so A opens it.
- The sheet it opens says what the network wants and where its sign-in page is, and has two rows: **Sign in…** and
  **Other ways**.
- pocket-ui checks again by itself every 15 seconds while the Wi-Fi page or the sheet is open. When the network lets
  the Pocket in (you signed in, or the front desk allowed it), the row goes away and the page says "You're online now.
  The network lets this Pocket in." You don't have to press anything. The checking stops when you are online or
  leave the page.

The check is one small web request to `nmcheck.gnome.org` (the page the system's own network service uses); it
sends nothing about you or the Pocket ([Troubleshooting](troubleshooting.md)). The Pocket never accepts terms or
sends anything you typed anywhere by itself.

When an update, the store or the PortMaster catalogue can't go online because of such a page, the message says
"Connected, but this network wants a sign-in first. Open Settings › Connections › Wi-Fi."

## Sign in on the Pocket

**Sign in…** loads the network's sign-in page and shows it the Pocket's way (the Pocket has no web browser):

Many pages that first show "Please wait" or "Connecting…" and then forward to the sign-in form work too: the Pocket
follows them the way a browser would, without running their scripts.

1. At the top: the page's name, its address ("Sign-in page: login.hotel-seeblick.example") and its first lines.
   **Read the whole page** shows all of its text (the terms of use, for example); Up/Down scroll, B goes back.
2. Below: what the page asks for, one row each.
   - A text field (room number, surname, e-mail address, voucher code): A opens the keyboard, START or **OK** keeps
     what you typed. A password shows as dots, with the last letter you typed visible.
   - A box to tick ("I accept the terms of use"): A ticks or unticks it. The Pocket never ticks anything by itself.
   - A choice (24 hours or 1 hour, for example): Left/Right or A.
   - The page's buttons ("Connect", "Continue"), with **Send** on the right. A sends the page, and only then is anything
     sent. If something the page needs is still empty, the Pocket says so ("Fill in “Room number” first.") and sends
     nothing.
3. After sending, the Pocket checks the connection again:
   - **You're online**: done. The Wi-Fi page says so too.
   - The page comes back, perhaps with a message ("Room number and surname don't match a booking."): the line under
     the address says "The network still wants a sign-in. Check what the page says." Correct it and send again.
   - **Still not online**: the page took the details, but the network still wants a sign-in. Try again, or use one of
     the other ways.
4. Some pages can't be done on the Pocket. It says so plainly and offers **Other ways**:
   - "This page needs a full browser": pages that only work with scripts (for example the Deutsche Bahn's WIFI@DB on
     ICE trains), pages with only links (sign in with Facebook or Google, pay by card), and pages that send what they
     carry to another address than the sign-in page. The page it gave up on is
     kept for the next [report](troubleshooting.md#report-a-problem), so the problem can be looked at on a computer.
   - "Not sent": the page would send your details to another site than the sign-in page itself.
   - "No answer": the sign-in page didn't answer; **Try again**.

What you type stays on the Pocket only until you leave the page: it is never saved, and it goes nowhere but to the
sign-in page's own form. If the sign-in page's certificate can't be checked (common on hotel networks), the page says
so in red: anyone nearby could read what you send, as on any phone or laptop on that network.

## Other ways online

**Other ways** on the sheet lists them, with this Pocket's Wi-Fi address:

1. **Your phone's hotspot.** Turn it on and choose it on the Wi-Fi page. It uses your mobile data and works on every
   network. Once saved, the Pocket joins it by itself when your usual network is out of reach.
2. **Wi-Fi sharing from a phone.** Many Samsung Galaxy phones can pass the hotel's Wi-Fi on: sign in on the phone,
   then turn on the hotspot with **Wi-Fi sharing** (in the phone's hotspot settings). The Pocket joins the phone and
   uses the hotel's Wi-Fi, not your mobile data. iPhones share only mobile data.
3. **Ask the front desk.** Many hotels and halls of residence can let a device in by its Wi-Fi address (sometimes
   called "MAC address" or "game console registration"). The sheet shows this Pocket's address in large type, for
   example `00:00:5E:00:53:7F`. The Pocket always uses this same address. If it couldn't be read, the sheet says so
   in its place.
4. **A travel router.** Sign in once on the router's page from your phone; the Pocket joins the router's own Wi-Fi
   and stays connected. The best answer if you travel often.

Signing in on your phone alone usually does not help the Pocket: the network lets in each device separately.
