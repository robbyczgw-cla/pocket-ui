#!/bin/sh
# SPDX-License-Identifier: GPL-2.0-or-later
# Card curl, deliberately limited: large bursts can wedge the TrimUI SDIO/Wi-Fi driver.
set -eu
url=${1:?https URL}; dest=${2:?destination}
case "$url" in https://*) ;; *) echo 'PortMaster: HTTPS download required' >&2; exit 1;; esac
app=${BRICK_APP:-$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)}
curl=${BRICK_PORTMASTER_CURL:-$app/bin/curl}
trap 'rm -f "$dest.part" "$dest.headers"' EXIT HUP INT TERM
"$curl" --fail --location --proto '=https' --proto-redir '=https' --tlsv1.2 \
  --connect-timeout 20 --max-time 3600 --retry 3 --retry-delay 2 --limit-rate 512k \
  --dump-header "$dest.headers" --output "$dest.part" "$url"
mv "$dest.part" "$dest"
sleep 0.2
