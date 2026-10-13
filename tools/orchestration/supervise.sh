#!/usr/bin/env bash
# 每次 tick：① 先把各车道状态落到 lane-status.log（巡检第一条就是它）
#            ② worker 车道（死了就重拉，cap 3）③ reviewer 车道（死了就重拉）
# 触发 = ABB busybox watch（-n 600）。执行者 = pi。不放慢查询。
watch=/e/tmp/openmir2-watch; repo=/e/Users/gxh/Documents/GitHub/OpenMir2; maxworker=3
alive() {
  powershell -NoProfile -Command "if (@(Get-CimInstance Win32_Process | Where-Object { \$_.Name -eq 'node.exe' -and \$_.CommandLine -match 'Tag: $1' }).Count -gt 0) { exit 0 } else { exit 1 }" >/dev/null 2>&1
}
# ---- ① 状态快照 ----
snap="$watch/lane-status.log"
{ echo "==== $(date '+%Y-%m-%d %H:%M:%S') ===="
  na=0; nall=0
  while IFS='|' read -r tag wt brief role card; do
    case "$tag" in ''|\#*) continue;; esac
    nall=$((nall+1))
    if alive "$tag"; then a="yes"; na=$((na+1)); else a="no"; fi
    head=$(git -C "$wt" rev-parse --short HEAD 2>/dev/null || echo "-")
    ahead=$(git -C "$repo" rev-list --count master.."$head" 2>/dev/null || echo "?")
    printf "  %-7s alive=%-3s head=%-9s ahead=%-3s %s\n" "$tag" "$a" "$head" "$ahead" "${card:-（reviewer）}"
  done < "$watch/lanes.conf"
  echo "  alive: $na / $nall"
} >> "$snap"
# ---- ② worker 重拉 ----
# owner 2026-10-11 改：槽位计数**每次启动前重算**（count_workers）。原版在 start-lane.sh 失败（如 worktree 不存在）时
# 仍 workers+1 ⇒ 后面的排队车道整轮拿不到槽、队列永不前进（实测踩过：五条车道全卡）。
count_workers() { local n=0 t; for t in $(grep '|worker|' "$watch/lanes.conf" | cut -d'|' -f1); do alive "$t" && n=$((n+1)); done; echo "$n"; }
for tag in $(grep '|worker|' "$watch/lanes.conf" | cut -d'|' -f1); do
  alive "$tag" && continue
  [ "$(count_workers)" -lt "$maxworker" ] || break
  echo "$(date '+%H:%M:%S') relaunch worker $tag"; bash "$watch/start-lane.sh" "$tag"
done
# ---- ③ reviewer 重拉 ----
for tag in $(grep '|reviewer|' "$watch/lanes.conf" | cut -d'|' -f1); do
  if ! alive "$tag"; then echo "$(date '+%H:%M:%S') relaunch reviewer $tag"; bash "$watch/start-lane.sh" "$tag"; fi
done
# ---- ④ merge 重拉（owner 2026-10-13 同意：审批通过即自动合并）----
# 合并检查点车道：把【待审】里「非作者 approve 覆盖当前 head」的卡合入 master（准入判据 + 四门禁 + 双端推送
# 见 briefs/brief-merge.md）。与 reviewer 车道一样**不受 maxworker 限制**，但**必须每 tick 保证在**
# —— 它是全流程的出口，缺了它队列只会越堆越长（2026-10-13 实测：待审堆到 7 张，人工清完车道又交 6 张）。
for tag in $(grep '|merge|' "$watch/lanes.conf" | cut -d'|' -f1); do
  if ! alive "$tag"; then echo "$(date '+%H:%M:%S') relaunch merge $tag"; bash "$watch/start-lane.sh" "$tag"; fi
done
echo "$(date '+%H:%M:%S') tick done workers=$(count_workers)"
