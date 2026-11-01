#!/usr/bin/env bash
# Measures a Chromium-based browser's RAM (PSS, all processes) and CPU on one page.
# Usage: scripts/measure.sh <binary> <url> [screenshot.png] [extra browser flags...]
set -u
bin=$(realpath "$1"); url=$2; shot=${3:-}; shift; shift; [ $# -gt 0 ] && shift
settle=${SETTLE:-25}; sample=${SAMPLE:-10}

"$bin" --url="$url" "$url" "$@" >/dev/null 2>&1 &
main=$!
sleep "$settle"

pids() { pgrep -f "^$bin" ; }
ticks() { local sum=0 p; for p in $(pids); do
    sum=$((sum + $(awk '{print $14+$15}' "/proc/$p/stat" 2>/dev/null || echo 0))); done; echo "$sum"; }

total=0; count=0
for p in $(pids); do
    pss=$(awk '/^Pss:/{print $2}' "/proc/$p/smaps_rollup" 2>/dev/null || echo 0)
    total=$((total + pss)); count=$((count + 1))
done
before=$(ticks); sleep "$sample"; after=$(ticks)
cpu=$(( (after - before) * 100 / (sample * $(getconf CLK_TCK)) ))

if [ -n "$shot" ]; then
    window=$(xdotool search --onlyvisible --pid "$main" 2>/dev/null | tail -1)
    [ -n "$window" ] && import -window "$window" "$shot"
fi
echo "$(basename "$bin") processes=$count ram_mb=$((total / 1024)) cpu_pct_of_one_core=$cpu url=$url"

kill "$main" 2>/dev/null; sleep 3; pkill -f "^$bin" 2>/dev/null; sleep 1
