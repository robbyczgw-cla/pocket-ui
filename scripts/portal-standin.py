#!/usr/bin/env python3
"""A stand-in Wi-Fi sign-in page ("captive portal") on 127.0.0.1, for tests/portal_e2e.rs and trying the sign-in helper
by hand (docs/PORTAL-SIGNIN.md, "Testing"). It behaves like a small hotel or cafe portal: until a client has signed in,
the connectivity check's address is redirected to the portal, which sets a session cookie, redirects once more and
serves its page; the form's answer lets the client in, and the check then answers "NetworkManager is online".

    python3 -I scripts/portal-standin.py <mode> <testdata/portal dir> <port file> [<log file>]

Modes: hotel (room number + surname + accept, POST; right answer 412 / Müller), clickthrough (one GET button), js (a
JavaScript-only page), interstitial (a "please wait" page whose timer forwards to another one, whose hidden form the
page submits on load, which lets the client in without anything filled in), loop (endless redirects), elsewhere (a form
that posts to another site). The port it bound to is written to <port file>; every request (method, path, body) to
<log file>. Only 127.0.0.1, nothing else is touched. PORTAL_STANDIN_BASE=<http://host> makes the redirects name that
address instead of 127.0.0.1:<port> (the Wi-Fi bench, docs/WIFI-BENCH.md).
"""
import http.server
import os
import sys
import urllib.parse

MODE, FIXTURES, PORT_FILE = sys.argv[1], sys.argv[2], sys.argv[3]
LOG = sys.argv[4] if len(sys.argv) > 4 else None
STATE = {"authed": False}
COOKIE = "sess=7f3a9c"
ONLINE = b"NetworkManager is online"


def page(name):
    with open(os.path.join(FIXTURES, name), "rb") as f:
        return f.read()


class Portal(http.server.BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, *args):
        pass

    def log(self, body=b""):
        if LOG:
            with open(LOG, "ab") as f:
                f.write(f"{self.command} {self.path} ".encode() + body + b"\n")

    def send(self, code, body=b"", headers=()):
        self.send_response(code)
        for k, v in headers:
            self.send_header(k, v)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def html(self, body, headers=()):
        self.send(200, body, [("Content-Type", "text/html; charset=utf-8"), *headers])

    def base(self):
        # PORTAL_STANDIN_BASE: the address the redirects name instead (the Wi-Fi bench's hotel network, whose gateway
        # forwards port 80 here: "http://login.wb-hotel.example", docs/WIFI-BENCH.md).
        return os.environ.get("PORTAL_STANDIN_BASE") or f"http://127.0.0.1:{self.server.server_address[1]}"

    def has_cookie(self):
        return COOKIE in (self.headers.get("Cookie") or "")

    def do_GET(self):
        self.log()
        url = urllib.parse.urlsplit(self.path)
        q = urllib.parse.parse_qs(url.query)
        if url.path == "/check_network_status.txt":
            if STATE["authed"]:
                return self.send(200, ONLINE, [("Content-Type", "text/plain")])
            return self.send(302, b"", [("Location", f"{self.base()}/portal/start?orig=check")])
        if url.path == "/portal/start":
            where = "/portal/wait" if MODE == "interstitial" else "/portal/login"
            return self.send(302, b"", [("Location", where), ("Set-Cookie", COOKIE + "; Path=/; HttpOnly")])
        if url.path == "/portal/wait" and MODE == "interstitial":
            # "Internet Access - please wait." with a timer's literal address, as the hotel from the first real test.
            return self.html(page("interstitial-js.html").replace(b"/guest/gateway.html", b"/portal/wait2"))
        if url.path == "/portal/wait2" and MODE == "interstitial":
            # Cisco/Aruba-style: the page submits its own hidden form on load.
            return self.html(page("interstitial-wait.html").replace(b"/guest/auth.html", b"/portal/accept"))
        if url.path == "/portal/login":
            if not self.has_cookie():
                return self.send(403, b"no session cookie")
            if MODE == "hotel":
                return self.html(page("hotel-room.html").replace(b'action="login.php"', b'action="/portal/login.php"'))
            if MODE == "clickthrough":
                return self.html(page("clickthrough.html"))
            if MODE == "js":
                return self.html(page("js-only.html"))
            if MODE == "elsewhere":
                return self.html(b'<form method=post action="http://198.51.100.7/collect"><input name=email><input type=submit></form>')
            if MODE == "loop":
                return self.send(302, b"", [("Location", "/portal/login")])
        if url.path == "/opennds_auth/" and MODE == "clickthrough" and self.has_cookie() and q.get("tok") == ["8f2c1a77"]:
            STATE["authed"] = True
            return self.send(302, b"", [("Location", q.get("redir", ["/"])[0])])
        if url.path == "/portal/welcome":
            return self.html(b"<h1>You're connected</h1><p>Enjoy your stay.</p>")
        return self.send(404, b"not here")

    def do_POST(self):
        n = int(self.headers.get("Content-Length") or 0)
        body = self.rfile.read(min(n, 65536))
        self.log(body)
        if self.path == "/portal/accept" and MODE == "interstitial":
            # The hidden form the page submits on load: its values alone let the client in.
            form = urllib.parse.parse_qs(body.decode("utf-8", "replace"))
            if self.has_cookie() and form.get("buttonClicked") == ["4"] and form.get("mac"):
                STATE["authed"] = True
                return self.send(303, b"", [("Location", "/portal/welcome")])
        if self.path == "/portal/login.php" and MODE == "hotel":
            form = urllib.parse.parse_qs(body.decode("utf-8", "replace"))
            ok = (
                self.has_cookie()
                and form.get("csrf") == ["b7e1&9"]
                and form.get("room") == ["412"]
                and form.get("surname") == ["Müller"]
                and form.get("accept") == ["1"]
                and form.get("action") == ["login"]
            )
            if ok:
                STATE["authed"] = True
                return self.send(303, b"", [("Location", "/portal/welcome")])
            again = page("hotel-room.html").replace(b'action="login.php"', b'action="/portal/login.php"')
            again = again.replace(b"<h1>", b"<p class=error>Room number and surname don't match a booking.</p><h1>", 1)
            return self.html(again)
        return self.send(404, b"not here")


server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Portal)
with open(PORT_FILE + ".tmp", "w") as f:
    f.write(str(server.server_address[1]))
os.replace(PORT_FILE + ".tmp", PORT_FILE)
server.serve_forever()
