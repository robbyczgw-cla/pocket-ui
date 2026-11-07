#!/bin/sh
# The firmware's start script calls this after MainUI ended and decides from its exit code whether a card UI ran. 1 = "none did", so
# when pocket-ui is gone (it only ends by powering off, or when the card is broken) the firmware's own menu comes up instead of a dead screen.
exit 1
