#!/usr/bin/env python3
"""The internet as the Wi-Fi bench's access points see it (docs/WIFI-BENCH.md): every name resolves to the network's
gateway (dnsmasq), and this answers every plain web request there the way NetworkManager's community check page does,
so pocket-ui's online check (src/net_check.rs) says online. Anything else gets the same text; nothing leaves the bench.

    python3 -I internet.py <address> <port>
"""
import http.server
import sys

ONLINE = b"NetworkManager is online"


class Internet(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        self.send_response(200)
        self.send_header("Content-Type", "text/plain")
        self.send_header("Content-Length", str(len(ONLINE)))
        self.end_headers()
        self.wfile.write(ONLINE)

    do_HEAD = do_GET

    def log_message(self, fmt, *args):
        sys.stderr.write("%s %s\n" % (self.client_address[0], fmt % args))
        sys.stderr.flush()


http.server.ThreadingHTTPServer((sys.argv[1], int(sys.argv[2])), Internet).serve_forever()
