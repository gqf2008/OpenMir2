#!/usr/bin/env bash
# 各车道状态：alive(进程) + worktree HEAD/ahead + 卡片状态(看板)
watch=/e/tmp/openmir2-watch; repo=/e/Users/gxh/Documents/GitHub/OpenMir2
alive() { powershell -NoProfile -Command "if (@(Get-CimInstance Win32_Process | Where-Object { \$_.Name -eq 'node.exe' -and \$_.CommandLine -match 'Tag: $1' }).Count -gt 0) { exit 0 } else { exit 1 }" >/dev/null 2>&1; }
printf "%-7s %-6s %-10s %-9s %s\n" TAG ALIVE HEAD AHEAD CARD
n=0
while IFS='|' read -r tag wt brief role card; do
  case "$tag" in ''|\#*) continue;; esac
  if alive "$tag"; then a="yes"; n=$((n+1)); else a="no"; fi
  head=$(git -C "$wt" rev-parse --short HEAD 2>/dev/null || echo "-")
  ahead=$(git -C "$repo" rev-list --count master.."$head" 2>/dev/null || echo "?")
  printf "%-7s %-6s %-10s %-9s %s\n" "$tag" "$a" "$head" "$ahead" "${card:-（reviewer）}"
done < "$watch/lanes.conf"
echo "----"
echo "alive 车道数：$n / $(grep -cE '^[a-z]' "$watch/lanes.conf")"
echo "看板："
cd "$repo" || { echo "status: **进不去仓库 $repo**" >&2; exit 2; }
# 卡 openmir2-walgit-local-commands-env：**不许**再写 `walgit collab ... 2>/dev/null | grep`。
# 2026-11-02 12:52 起，CLI 在缺 WALGIT_LOCAL_COMMANDS=1 时把「接口被拒」打到 stderr、rc=1；
# 管道取的是 grep 的 rc，错误被吞 ⇒ 看板显示成「空的」的假绿。统一走封装，它 fail-closed。
if ! board=$(python -I -B tools/orchestration/walgit_collab.py board); then
  echo "status: **看板读取失败**（walgit-collab fail-closed，原因见上方 stderr）；" \
       "不要把它当成「没有待审」。" >&2
  exit 1
fi
printf '%s\n' "$board" | grep -E '^== (进行中|待审)' -A0 || true
