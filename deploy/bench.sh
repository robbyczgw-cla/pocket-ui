#!/bin/sh
# Measures a running game once per second and prints averages.
# Usage: bench.sh SECONDS PROCESS_NAME [LABEL]
#   CPU %   summed over all processes with that exact name (100 % = one core)
#   MHz     current frequency of the little/big/prime CPU clusters and the GPU
#   °C      CPU and GPU temperature
#   W       system draw from the battery; "-" while USB is plugged in
SECS=${1:?seconds}; NAME=${2:?process name}; LABEL=${3:-$2}
HZ=$(getconf CLK_TCK 2>/dev/null || echo 100)
GPU=/sys/devices/platform/soc@0/3d00000.gpu/devfreq/3d00000.gpu/cur_freq
BAT=/sys/class/power_supply/battery
USB=/sys/class/power_supply/pm8150b-charger

ticks() {
  for pid in $(pidof "$NAME"); do cut -d' ' -f14,15 "/proc/$pid/stat" 2>/dev/null; done |
    awk '{t += $1 + $2} END {print t + 0}'
}
khz() { cat /sys/devices/system/cpu/cpufreq/policy$1/scaling_cur_freq; }
zone() { for z in /sys/class/thermal/thermal_zone*; do [ "$(cat $z/type)" = "$1" ] && cat $z/temp && return; done; echo 0; }
watts() {
  # With USB plugged in the charger and fuel-gauge readings contradict each other on SM8250,
  # so power is only reported on battery.
  if [ "$(cat $USB/online)" = "0" ]; then
    echo "$(cat $BAT/current_now) $(cat $BAT/voltage_now)" | awk '{c=$1<0?-$1:$1; printf "%.2f", c*$2/1e12}'
  else
    echo "-"
  fi
}

# CPU % is process ticks over the time the CPUs were awake (all-CPU ticks in /proc/stat divided by the
# number of CPUs), not over the wall clock: when the device suspends in the middle (others use it too),
# the gap does not count and the percentage stays right.
NCPU=$(grep -c '^cpu[0-9]' /proc/stat)
awake() { awk -v n="$NCPU" '/^cpu / {t = 0; for (i = 2; i <= NF; i++) t += $i; print t / n}' /proc/stat; }
echo "time,cpu_pct,little_mhz,big_mhz,prime_mhz,gpu_mhz,cpu_c,gpu_c,watts"
prev=$(ticks); aprev=$(awake); n=0
while [ $n -lt "$SECS" ]; do
  sleep 1
  now=$(ticks); anow=$(awake)
  cpu=$(awk -v d="$((now - prev))" -v a="$aprev" -v b="$anow" 'BEGIN {w = b - a; printf "%d", (w > 0 ? d * 100 / w : 0)}')
  prev=$now; aprev=$anow
  echo "$n,$cpu,$(( $(khz 0)/1000 )),$(( $(khz 4)/1000 )),$(( $(khz 7)/1000 )),$(( $(cat $GPU)/1000000 )),$(( $(zone cpu4-bottom-thermal)/1000 )),$(( $(zone gpu-top-thermal)/1000 )),$(watts)"
  n=$((n + 1))
done | tee /tmp/bench.csv | tail -n +1 >/dev/null
awk -F, -v label="$LABEL" 'NR>1 && NR>3 {n++; for(i=2;i<=8;i++) s[i]+=$i; if ($9!="-") {w+=$9; wn++}}
  END {printf "%s: cpu %.0f%%  little %.0f  big %.0f  prime %.0f MHz  gpu %.0f MHz  cpu %.0f°C gpu %.0f°C  %s W  (%d samples)\n",
       label, s[2]/n, s[3]/n, s[4]/n, s[5]/n, s[6]/n, s[7]/n, s[8]/n, (wn ? sprintf("%.2f", w/wn) : "-"), n}' /tmp/bench.csv
