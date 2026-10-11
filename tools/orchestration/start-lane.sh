#!/usr/bin/env bash
set -u
watch=/e/tmp/openmir2-watch; tag="$1"
line=$(grep -E "^$tag\|" "$watch/lanes.conf") || { echo "no lane $tag"; exit 1; }
wt=$(echo "$line" | cut -d'|' -f2); brief=$(echo "$line" | cut -d'|' -f3)
# owner 2026-10-11：worktree 不存在时**必须响亮失败**。原写法 `( cd "$wt" && nohup pi ... > agent-$tag.log )`
# 在 cd 失败时静默跳过、连 agent-$tag.log 都不产生，而 supervisor 仍把这轮算成"已启动一个 worker" ⇒
# 实测五条排队车道（m2env/m3bag/m1load/dbsrv6k/m3gs）就是这样全卡住的。车道 worktree 由 owner 预先建好。
if [ ! -d "$wt" ]; then
  echo "MISSING-WORKTREE $tag -> $wt（车道未启动；请在主检出用 git worktree add 建好再等下一 tick）" >> "$watch/lane-status.log"
  exit 1
fi
( cd "$wt" && nohup pi -p -- "Read $brief and execute it (the brief is on this machine). Work only in this repo/worktree. Deliver via walgit as the brief says; do NOT merge and do NOT sign review. Tag: $tag" \
    < /dev/null > "$watch/agent-$tag.log" 2>&1 & )
echo "started $tag"
