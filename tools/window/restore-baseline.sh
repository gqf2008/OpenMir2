#!/bin/sh
# tools/window/restore-baseline.sh — C# 侧测试库基线恢复（仓库内事实源）
#
# 卡 `openmir2-window-tmp-hardening`：原实现只在 `E:/tmp/conformance/restore-baseline.sh`，
# 外部清扫（历史三次）后 alt 窗口连 restore 都跑不了。脚本收进仓库；**基线 SQL 仍是
# 部署件**（`<conformance-root>/baseline/*.sql`，约 26MB，不入库），路径按
# `MIR2_CONFORMANCE_ROOT` 解析，缺件 fail-closed 并点名。
#
# 用法：
#   tools/window/restore-baseline.sh [baseline_dir]
#   tools/window/restore-baseline.sh --check [baseline_dir]   # 只校验前置，不连库/不改库
#
# 环境变量：
#   MIR2_CONFORMANCE_ROOT  部署根（缺省 E:/tmp/conformance）
#   MIR2_MYSQL             mysql 客户端（缺省与 tools/dbsnap 同口径）
#
# 退出码：0 = 三个库都恢复成功（或 --check 前置齐全）；2 = 缺件/用法错/恢复失败（fail-closed）。
set -eu

CHECK=0
if [ "${1:-}" = "--check" ]; then CHECK=1; shift; fi
if [ "${1:-}" = "--help" ] || [ "${1:-}" = "-h" ]; then
  sed -n '2,19p' "$0"; exit 0
fi

MYSQL=${MIR2_MYSQL:-D:/mysql/mariadb-10.11.19-winx64/bin/mysql.exe}
CONF_ROOT=${MIR2_CONFORMANCE_ROOT:-E:/tmp/conformance}
BASE=${1:-$CONF_ROOT/baseline}
DBS="mir2_test_db mir2_test_account mir2_test_data"

if [ ! -x "$MYSQL" ] && [ ! -f "$MYSQL" ]; then
  echo "RED: 缺 MySQL 客户端：$MYSQL（设 MIR2_MYSQL 或安装 mariadb）" >&2
  exit 2
fi
if [ ! -d "$BASE" ]; then
  echo "RED: 缺基线目录：$BASE（MIR2_CONFORMANCE_ROOT=$CONF_ROOT；外部清扫后需重建 baseline）" >&2
  exit 2
fi
missing=""
for db in $DBS; do
  if [ ! -f "$BASE/$db.sql" ]; then missing="$missing $BASE/$db.sql"; fi
done
if [ -n "$missing" ]; then
  echo "RED: 缺基线 SQL：$missing（baseline 未就绪，restore 无从恢复；fail-closed）" >&2
  exit 2
fi

if [ "$CHECK" -eq 1 ]; then
  echo "OK: restore-baseline 前置齐全（base=$BASE mysql=$MYSQL）"
  exit 0
fi

for db in $DBS; do
  "$MYSQL" -h127.0.0.1 -uroot -e "DROP DATABASE IF EXISTS \`$db\`; CREATE DATABASE \`$db\` CHARACTER SET utf8mb4 COLLATE utf8mb4_general_ci;"
  "$MYSQL" -h127.0.0.1 -uroot "$db" < "$BASE/$db.sql"
done
exit 0
