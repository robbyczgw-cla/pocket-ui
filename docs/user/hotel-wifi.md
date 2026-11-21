# Hotel and train Wi-Fi

Hotel, train, airport and café networks usually block internet access until you click through a sign-in page—accepting
terms, typing your room number and last name, or entering a voucher code. Until you clear that page, box art, updates and
RetroAchievements can't connect even though Wi-Fi shows as joined.

## What the Pocket shows

Open Settings → Connections → **Wi-Fi** while joined to a captive network:

- The top row reads **This network wants a sign-in**, with the portal address on the right (such as
  `login.hotel-seeblick.example`). It's highlighted automatically, so press A to open it.
- The pop-up explains what the network wants and gives two choices: **Sign in…** and **Other ways**.
- While the Wi-Fi page or pop-up stays open, pocket-ui rechecks automatically every 15 seconds. As soon as the network
  lets you through (after signing in or once the front desk whitelists your device), the banner disappears and the page
  reads "You're online now. The network lets this Pocket in." Checking stops automatically once you're online or leave
  the page.

The connectivity check makes one tiny request to `nmcheck.gnome.org` (the OS's standard network check) and sends nothing
about you or your Pocket ([Troubleshooting](troubleshooting.md)). pocket-ui never auto-accepts terms or submits forms on
its own.

If an update or the PortMaster catalogue gets blocked by a sign-in page, it tells you straight: "Connected, but this
network wants a sign-in first. Open Settings › Connections › Wi-Fi."

## Sign in on the Pocket

**Sign in…** fetches the network's login form and displays it natively—no full web browser needed. It also follows
redirect pages ("Please wait", "Connecting…") without running any scripts.

1. At the top: the page title, address ("Sign-in page: login.hotel-seeblick.example") and opening lines. Pick **Read the
   whole page** to read the full text or terms (Up/Down to scroll, B to go back).
2. Underneath: each form field on its own row:
   - **Text fields** (room number, last name, email, voucher): press A for the on-screen keyboard, then START or **OK** to
     confirm. Passwords mask as dots with the last typed character visible.
   - **Checkboxes** ("I accept the terms of use"): press A to tick or untick. pocket-ui never ticks anything for you.
   - **Choices** (like 24 hours vs 1 hour): switch with Left/Right or A.
   - **Submit buttons** ("Connect", "Continue", tagged **Send**): press A to submit—nothing is sent until you do. Missing
     a required field? pocket-ui warns you ("Fill in “Room number” first.") without sending.
3. After submitting, pocket-ui checks your connection again:
   - **You're online:** all set—the Wi-Fi page updates too.
   - **Form returned** (for example "Room number and surname don't match a booking."): the banner reads "The network still
     wants a sign-in. Check what the page says." Fix the typo and send again.
   - **Still not online:** the portal accepted your form, but traffic is still blocked. Retry or try **Other ways**.
4. Some portals can't run without a full browser—pocket-ui tells you plainly and offers **Other ways**:
   - "This page needs a full browser": script-heavy portals (like Deutsche Bahn's WIFI@DB on ICE trains), link-only
     sign-ins (Facebook, Google, credit card) or forms that post to a different domain. The stubborn page is saved for
     your next [problem report](troubleshooting.md#report-a-problem) so it can be inspected later.
   - "Not sent": blocked because the form tried to submit your details to a different site than the sign-in portal.
   - "No answer": the portal timed out—pick **Try again**.

Anything you type lives only in memory until you leave the page—never saved to disk, never sent anywhere except that
portal's own form. If the portal's HTTPS certificate can't be verified (common in hotels), a red warning lets you know
others on the network could read what you send, just like on a phone or laptop.

## Other ways online

Select **Other ways** to see backup options along with your Pocket's Wi-Fi address:

1. **Phone hotspot:** switch on your phone's hotspot and connect from Wi-Fi settings. Uses mobile data and works
   everywhere; once saved, your Pocket auto-joins whenever your home network isn't nearby.
2. **Phone Wi-Fi sharing:** many Samsung Galaxy phones can share hotel Wi-Fi over their hotspot—sign in on your phone,
   then enable **Wi-Fi sharing** in hotspot settings so your Pocket rides the hotel Wi-Fi without burning mobile data.
   *(iPhones only share cellular data.)*
3. **Ask the front desk:** many hotels and dorms can whitelist a handheld by its Wi-Fi MAC address ("game console
   registration"). The sheet shows your Pocket's permanent MAC address in big type (for example `00:00:5E:00:53:7F`), or
   tells you if it couldn't be read.
4. **Travel router:** sign in once from your phone on a pocket travel router and your handheld stays online effortlessly—
   the cleanest setup for frequent travellers.

Signing in on your phone alone won't unlock your Pocket, since captive networks authorise each device individually.
