#!/usr/bin/env sh
# sign_entry — 签名包装：**所有 oid 参数先过写侧校验**，再把解析后的全 oid 交给 `walgit collab entry`。
#
# 卡 `openmir2-collab-oid-resolvability-gate`（写侧 fail-closed）。背景：walgit 对 oid **零校验**，
# 短号/手补长号都会**原样落库**；今晚五个入口各犯一次（review head / patch head / `--parent` ×2 /
# `--base`+`--head`）。本脚本把「拒绝短号」从纪律变成工具行为：
#   * `--base` / `--head` ⇒ `oid_gate verify`：必须 40 位小写 hex 且解析到 **commit**；
#   * `--parent`         ⇒ `oid_gate verify-entry`：必须非空 hex 且解析得到对象（条目 oid 是 **blob 对象**，判据是对象存在）；
#   * 任一不满足 ⇒ **rc=1，拒绝调用 walgit**（不会写坏条目）。
#
# 用法（字段与 `walgit collab entry` 对齐）：
#   tools/collab/sign_entry.sh --repo . --kind patch --id <thread> \
#       --parent <entry-id> --base <oid> --head <oid> --body-file /tmp/body.json [--push walgit] [--check-only]
# 退出码：0 已签（或 --check-only 校验通过）；1 校验失败（拒绝）；2 用法错。
set -eu

REPO="."
KIND=""; ID=""; PARENT=""; BASE=""; HEAD=""; BODY=""; PUSH=""; CHECK_ONLY=0
HAVE_PARENT=0; HAVE_BASE=0; HAVE_HEAD=0; HAVE_BODY=0

while [ $# -gt 0 ]; do
  case "$1" in
    --repo) REPO="$2"; shift 2 ;;
    --kind) KIND="$2"; shift 2 ;;
    --id) ID="$2"; shift 2 ;;
    --parent) PARENT="$2"; HAVE_PARENT=1; shift 2 ;;
    --base) BASE="$2"; HAVE_BASE=1; shift 2 ;;
    --head) HEAD="$2"; HAVE_HEAD=1; shift 2 ;;
    --body-file) BODY="$2"; HAVE_BODY=1; shift 2 ;;
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
GATE="$(dirname "$0")/oid_gate.py"
PY_BIN="${SIGN_ENTRY_PYTHON:-python}"

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
if [ "$CHECK_ONLY" -eq 1 ]; then exit 0; fi

set -- --repo "$REPO" --kind "$KIND" --id "$ID" --body "$(cat "$BODY")"
if [ "$HAVE_PARENT" -eq 1 ]; then set -- "$@" --parent "$PARENT"; fi
if [ -n "$FULL_BASE" ]; then set -- "$@" --base "$FULL_BASE"; fi
if [ -n "$FULL_HEAD" ]; then set -- "$@" --head "$FULL_HEAD"; fi
if [ -n "$PUSH" ]; then set -- "$@" --push "$PUSH"; fi

walgit collab entry "$@"
