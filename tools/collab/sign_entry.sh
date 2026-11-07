#!/usr/bin/env sh
# sign_entry — 签名包装：**所有 oid 参数先过写侧校验**，再把解析后的全 oid 交给 `wg collab entry`。
#
# 卡 `openmir2-collab-oid-resolvability-gate`（写侧 fail-closed）。背景：walgit 对 oid **零校验**，
# 短号/手补长号都会**原样落库**；今晚五个入口各犯一次（review head / patch head / `--parent` ×2 /
# `--base`+`--head`）。本脚本把「拒绝短号」从纪律变成工具行为：
#   * `--base` / `--head` ⇒ `oid_gate verify`：必须 40 位小写 hex 且解析到 **commit**；
#   * `--parent`         ⇒ `oid_gate verify-entry`：必须非空 hex 且解析得到对象（条目 oid 是 **blob 对象**，判据是对象存在）；
#   * 任一不满足 ⇒ **rc=1，拒绝调用 walgit**（不会写坏条目）。
#
# 2026-11-01 增补（卡 `openmir2-collab-sign-entry-hardening`，两起写侧误签事故）：
#   * `--parent` **缺席或为空 ⇒ 拒签**（不再静默按「根条目」照签）；建线请显式 `--root`；
#   * `--root` 只允许在「该 thread 尚无任何条目」时使用——**任何 kind（含 `--kind issue`
#     建线）都要先探测线程历史**，有历史条目却声明根条目 ⇒ 拒签（用 `wg collab thread <id>` 判空）；
#   * `--body-file` 的 mtime 超过 `--max-body-age`（默认 1800 秒）⇒ 拒签，
#     错误里打印 body 的 `sha256` 与 `mtime`；确认不是上一张卡的残留后加 `--allow-stale-body` 显式放行；
#   * 校验通过后打印 `body_sha256=<hex>`，供签后回读比对。
#   * 以上三项由 `sign_entry.py --precheck` 实现，本脚本在解析 oid 之前调用它。
#
# 用法（字段与 `wg collab entry` 对齐）：
#   # patch / status（有父条目）
#   tools/collab/sign_entry.sh --repo . --kind patch --id <thread> \
#       --parent <entry-id> --base <oid> --head <oid> --body-file body.json \
#       [--push walgit] [--check-only]
#   # 建线（kind=issue）：**必须显式 --root**（不能靠「不写 --parent」），且该 thread 必须为空
#   tools/collab/sign_entry.sh --repo . --kind issue --id <thread> \
#       --root --body-file body.json [--push walgit]
#   # 需要重签旧 body 时（先确认不是上一张卡的残留）
#   tools/collab/sign_entry.sh ... --allow-stale-body [--max-body-age <秒>]
# 退出码：0 已签（或 --check-only 校验通过）；1 校验失败（拒绝）；2 用法错。
set -eu

REPO="."
KIND=""; ID=""; PARENT=""; BASE=""; HEAD=""; BODY=""; PUSH=""; CHECK_ONLY=0
HAVE_PARENT=0; HAVE_BASE=0; HAVE_HEAD=0; HAVE_BODY=0
ROOT=0; ALLOW_STALE=0; MAX_AGE=1800

while [ $# -gt 0 ]; do
  case "$1" in
    --repo) REPO="$2"; shift 2 ;;
    --kind) KIND="$2"; shift 2 ;;
    --id) ID="$2"; shift 2 ;;
    --parent) PARENT="$2"; HAVE_PARENT=1; shift 2 ;;
    --base) BASE="$2"; HAVE_BASE=1; shift 2 ;;
    --head) HEAD="$2"; HAVE_HEAD=1; shift 2 ;;
    --body-file) BODY="$2"; HAVE_BODY=1; shift 2 ;;
    --root) ROOT=1; shift ;;
    --allow-stale-body) ALLOW_STALE=1; shift ;;
    --max-body-age) MAX_AGE="$2"; shift 2 ;;
    --push) PUSH="$2"; shift 2 ;;
    --check-only) CHECK_ONLY=1; shift ;;
    *) echo "sign_entry: 未知参数 $1" >&2; exit 2 ;;
  esac
done

if [ -z "$KIND" ] || [ -z "$ID" ] || [ "$HAVE_BODY" -ne 1 ]; then
  echo "sign_entry: --kind / --id / --body-file 必填" >&2; exit 2
fi
if [ ! -f "$BODY" ]; then echo "sign_entry: body 文件不存在：$BODY" >&2; exit 2; fi

# N1：路径相对**脚本自身**（仓外调用也找得到），`--repo` 透传给校验器
HERE="$(dirname "$0")"
GATE="$HERE/oid_gate.py"
PRECHECK="$HERE/sign_entry.py"
PY_BIN="${SIGN_ENTRY_PYTHON:-python}"

# ---- 写侧 fail-closed：parent/root 与 body 新鲜度（先于 oid 解析与任何签名动作）----
set -- --precheck --repo "$REPO" --kind "$KIND" --id "$ID" \
    --body-file "$BODY" --max-body-age "$MAX_AGE"
if [ "$ROOT" -eq 1 ]; then set -- "$@" --root; fi
if [ "$HAVE_PARENT" -eq 1 ]; then set -- "$@" --parent "$PARENT"; fi
if [ "$ALLOW_STALE" -eq 1 ]; then set -- "$@" --allow-stale-body; fi
prc=0
pre_out=$("$PY_BIN" -I "$PRECHECK" "$@" 2>&1) || prc=$?
if [ "$prc" -ne 0 ]; then
  printf '%s\n' "$pre_out" >&2
  echo "sign_entry: 前置校验失败，拒绝签名（rc=$prc）" >&2
  exit "$prc"
fi
# Python 在 Windows 上把 \n 写成 \r\n；先去掉 CR，行尾锚点才咬得住。
BODY_SHA=$(printf '%s\n' "$pre_out" | tr -d '\r' | sed -n 's/^body_sha256=\([0-9a-f]\{64\}\)$/\1/p' | head -1)
if [ -z "$BODY_SHA" ]; then
  echo "sign_entry: 前置校验没回吐 body_sha256（$pre_out）" >&2
  exit 1
fi

resolve_commit() {
  # $1 = 字段名, $2 = 值；成功时 echo 全 oid
  if ! out=$("$PY_BIN" -I "$GATE" verify "$2" --repo "$REPO" 2>&1); then
    echo "sign_entry: **拒绝 $1=$2**：$out" >&2
    exit 1
  fi
  full=$(printf '%s\n' "$out" | grep -E '^[0-9a-f]{40}$' | head -1)
  if [ -z "$full" ]; then
    echo "sign_entry: **拒绝 $1=$2**：校验器没回吐全 oid（$out）" >&2
    exit 1
  fi
  printf '%s' "$full"
}

FULL_BASE=""; FULL_HEAD=""
if [ "$HAVE_BASE" -eq 1 ]; then FULL_BASE=$(resolve_commit --base "$BASE"); fi
if [ "$HAVE_HEAD" -eq 1 ]; then FULL_HEAD=$(resolve_commit --head "$HEAD"); fi

if [ "$HAVE_PARENT" -eq 1 ] && [ -n "$PARENT" ]; then
  if ! out=$("$PY_BIN" -I "$GATE" verify-entry "$PARENT" --repo "$REPO" 2>&1); then
    echo "sign_entry: **拒绝 --parent=$PARENT**：$out" >&2
    exit 1
  fi
fi

echo "sign_entry: 校验通过（base=${FULL_BASE:-<none>} head=${FULL_HEAD:-<none>} parent=${PARENT:-<root>}）" >&2
if [ "$CHECK_ONLY" -eq 1 ]; then
  echo "sign_entry: body_sha256=$BODY_SHA" >&2
  exit 0
fi

set -- --repo "$REPO" --kind "$KIND" --id "$ID" --body "$(cat "$BODY")"
if [ -n "$PARENT" ]; then set -- "$@" --parent "$PARENT"; fi
if [ -n "$FULL_BASE" ]; then set -- "$@" --base "$FULL_BASE"; fi
if [ -n "$FULL_HEAD" ]; then set -- "$@" --head "$FULL_HEAD"; fi
if [ -n "$PUSH" ]; then set -- "$@" --push "$PUSH"; fi

# 卡 openmir2-collab-interface-wg-migration：walgit 0.14.0 起协作命令是 `wg collab`；
# 旧版的 WALGIT_LOCAL_COMMANDS=1 开关已不存在，不要再加。
wg collab entry "$@"
echo "sign_entry: body_sha256=$BODY_SHA" >&2
