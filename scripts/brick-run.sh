#!/bin/sh
# One kit stage on the TrimUI Brick from the build machine (scripts/brick-kit.sh has the list). Holds /tmp/brick-device.lock for the whole stage
# (a screen stage included), so another agent's session cannot overlap. Usage: BRICK_HOST=<host> scripts/brick-run.sh <stage>
# A screen stage takes the Brick's screen: tell the owner first. Never longer than 10 minutes under the lock.
stage=${1:?stage (guard read binary rom viewcheck snapshot input ui game pause pak egl soak gltest glui)}
host=${BRICK_HOST:?set BRICK_HOST to the name or address of the Brick}
exec flock -w 600 /tmp/brick-device.lock timeout 600 ssh -o ConnectTimeout=10 -o BatchMode=yes "root@$host" "KIT_OWNER_PRESENT=${KIT_OWNER_PRESENT:-} sh /mnt/SDCARD/.pocket-ui-test/brick-kit.sh $stage"
