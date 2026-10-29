#!/bin/sh
# ROCKNIX's two writes, preserving the isolated bench's XDG_RUNTIME_DIR.
# The stock helper sources /etc/profile, which points at the real login runtime.
pw-metadata -n settings 0 clock.force-quantum 1023
pw-metadata -n settings 0 clock.force-quantum 1024
