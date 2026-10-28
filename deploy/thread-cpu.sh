#!/bin/sh
# CPU per thread of a running emulator: thread-cpu.sh PROCESS_NAME SECONDS
# Prints each thread's share of one core over the window (100 % = one core), busiest first.
NAME=${1:?process name}; SECS=${2:-10}
HZ=$(getconf CLK_TCK 2>/dev/null || echo 100)
snap() {
  for p in $(pidof "$NAME"); do
    for t in /proc/$p/task/*; do
      # utime and stime are fields 14 and 15; the name in field 2 may contain spaces, so cut after ") ".
      echo "$(basename $t) $(tr ' ' '_' < $t/comm) $(sed 's/.*) //' $t/stat | cut -d' ' -f12,13)"
    done
  done
}
snap > /tmp/tcpu0; sleep "$SECS"; snap > /tmp/tcpu1
awk -v hz="$HZ" -v s="$SECS" 'NR == FNR {a[$1] = $3 + $4; next} {d = $3 + $4 - a[$1]; if (d > 0) printf "%5.1f%%  %s\n", d * 100 / hz / s, $2}' /tmp/tcpu0 /tmp/tcpu1 | sort -rn | head -12
