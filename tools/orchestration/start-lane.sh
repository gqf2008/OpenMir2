#!/usr/bin/env bash
set -u
watch=/e/tmp/openmir2-watch; tag="$1"
line=$(grep -E "^$tag\|" "$watch/lanes.conf") || { echo "no lane $tag"; exit 1; }
wt=$(echo "$line" | cut -d'|' -f2); brief=$(echo "$line" | cut -d'|' -f3)
( cd "$wt" && nohup pi -p -- "Read $brief and execute it (the brief is on this machine). Work only in this repo/worktree. Deliver via walgit as the brief says; do NOT merge and do NOT sign review. Tag: $tag" \
    < /dev/null > "$watch/agent-$tag.log" 2>&1 & )
echo "started $tag"
