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
cd "$repo" && walgit collab board 2>/dev/null | grep -E '^== (进行中|待审)' -A0
