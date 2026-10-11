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
workers=0
for tag in $(grep '|worker|' "$watch/lanes.conf" | cut -d'|' -f1); do alive "$tag" && workers=$((workers+1)); done
for tag in $(grep '|worker|' "$watch/lanes.conf" | cut -d'|' -f1); do
  if ! alive "$tag" && [ "$workers" -lt "$maxworker" ]; then
    echo "$(date '+%H:%M:%S') relaunch worker $tag"; bash "$watch/start-lane.sh" "$tag"; workers=$((workers+1))
  fi
done
# ---- ③ reviewer 重拉 ----
for tag in $(grep '|reviewer|' "$watch/lanes.conf" | cut -d'|' -f1); do
  if ! alive "$tag"; then echo "$(date '+%H:%M:%S') relaunch reviewer $tag"; bash "$watch/start-lane.sh" "$tag"; fi
done
echo "$(date '+%H:%M:%S') tick done workers=$workers"
