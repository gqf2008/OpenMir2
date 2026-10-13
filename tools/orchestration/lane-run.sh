#!/usr/bin/env bash
# owner 2026-10-12：**车道本体的启动器**。由 start-lane.sh 经 WMI `Win32_Process Create` 起
# ⇒ 不继承 tick/loop 实例的 console（否则 console 一关车道会收 CTRL_CLOSE 在 1–4 分钟内猝死）。
# owner 2026-10-13：**角色化 prompt**——`role=merge` 的合并检查点车道必须被明确告知"你的活就是合并"，
# 不能沿用 worker/reviewer 那条含 "do NOT merge" 的 prompt（否则它会把唯一该干的活拒掉）。
set -u
watch=/e/tmp/openmir2-watch; tag="$1"
line=$(grep -E "^$tag\|" "$watch/lanes.conf") || { echo "MISSING-LANE $tag（lanes.conf 无此行）" >> "$watch/lane-status.log"; exit 1; }
wt=$(echo "$line" | cut -d'|' -f2); brief=$(echo "$line" | cut -d'|' -f3); role=$(echo "$line" | cut -d'|' -f4)
if [ ! -d "$wt" ]; then
  echo "MISSING-WORKTREE $tag -> $wt（车道未启动）" >> "$watch/lane-status.log"
  exit 1
fi
cd "$wt" || exit 1
common="Read $brief and execute it (the brief is on this machine). Work ONLY inside your own worktree (the cd above; NEVER edit, commit or push in the shared main checkout E:/Users/gxh/Documents/GitHub/OpenMir2 -- it is the rev lane's worktree; if you need a build use your own worktree with CARGO_TARGET_DIR under E:/tmp). Deliver via walgit as the brief says; "
if [ "$role" = "merge" ]; then
  exec pi -p -- "$common YOU ARE THE MERGE CHECKPOINT LANE: merging approved cards into master IS your job -- follow the brief's admission criteria, four gates and dual push exactly; never review, never author, never hand-resolve conflicts (abort instead). Do NOT self-sign: never sign a review of your own patch. Tag: $tag" \
    < /dev/null >> "$watch/agent-$tag.log" 2>&1
fi
exec pi -p -- "$common do NOT merge. Do NOT self-sign: never sign a review of your own patch. If you are a reviewer lane, you MUST sign a 'kind=review' entry (approve/request_changes) for the patch you audited, per your brief. Tag: $tag" \
  < /dev/null >> "$watch/agent-$tag.log" 2>&1
