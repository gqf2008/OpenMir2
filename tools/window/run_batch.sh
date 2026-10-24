#!/usr/bin/env bash
# -*- coding: utf-8 -*-
# run_batch.sh — 窗口 run-3 一键续跑（场景数**以清单为准**，不硬编码；alt/window-magic 混合）· 卡 openmir2-window-run2-prep
#
# 一条命令完成：release 重建 + head/sha256 对齐 → 私栈/两坑/pin 前置自检
#                → 按清单逐场景跑 card_window.py → 每个场景落 report/card-window/README
#                → 批次汇总 batch.jsonl / batch-summary.json。
#
# **续跑语义**：默认跳过「已完成」的场景。完成 = `card-window.json` 里
#   verdict ∈ {PASS, FAIL} 且 compare_rc ∈ {0,1} 且 report_kind=compare 且
#   frames.compared>0 且 artifacts 里有 report.json/README.txt 且 git_head == 当前 HEAD。
#   UNDECIDABLE / 中途被杀（没有 card-window.json）/ head 已变的场景会重跑；
#   card_window 会把旧产物整体归档到 `<out>/_stale-<ts>-<pid>/`，不会拿旧 PASS 冒充本次。
#
# 用法（在仓库/worktree 根目录执行；脚本自身会 cd 到检出根）：
#   bash tools/window/run_batch.sh                    # 全流程（重建 + 自检 + 清单内全部可跑场景）
#   bash tools/window/run_batch.sh --dry-run          # 只打印计划 + 跑只读前置自检，不起栈
#   bash tools/window/run_batch.sh --list             # 打印场景清单（含 blocked 标注与计数）
#   bash tools/window/run_batch.sh --only login_only,spell_cast
#   bash tools/window/run_batch.sh --from 9           # 从第 9 个场景接着跑（含 9；上界 = 场景数）
#   bash tools/window/run_batch.sh --force            # 忽略续跑判据，全部重跑
#   bash tools/window/run_batch.sh --no-rebuild       # 跳过 cargo build（不推荐）
#   bash tools/window/run_batch.sh --no-preflight     # 跳过前置自检（只在调试时用）
#   bash tools/window/run_batch.sh --skip-pin-smoke   # pin 链只做静态核对（证据里登记未冒烟）
#   bash tools/window/run_batch.sh --rng-log          # 每侧落 RNG 流水并校验（缺账本/无 site ⇒ 该场景 rc=2）
#   bash tools/window/run_batch.sh --run-dir E:/tmp/window/run3 --timeout-sec 2700
#
# 退出码：0 = 所有选中场景都产出了 PASS/FAIL（有结论）；2 = 缺参/前置失败/出现
#         UNDECIDABLE/超时/目录不完整（与 card_window 的 fail-closed 一致）。
#         **UNDECIDABLE 不是通过**，也会让批次 rc=2。
#
# 红线（本脚本遵守）：不动 mysqld（清理只按 E:/tmp/conformance/csharp-root 路径杀进程 +
#   stop-rust.ps1）；不写生产库（profile 固定 alt/window-magic，测试库）；
#   场景 JSON 只读（--dry-run 由 card_window 负责，本脚本不改场景）。
set -uo pipefail

HERE=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
REPO=$(cd "$HERE/../.." && pwd)
MANIFEST=${WINDOW_MANIFEST:-"$HERE/run3-scenarios.json"}  # 自测可用 WINDOW_MANIFEST 指向临时清单
CARD_WINDOW="tools/conformance/card_window.py"
PREFLIGHT_PRIVATE="tools/window/preflight_private_stack.py"
PREFLIGHT_PUBLIC="tools/conformance/preflight.py"
REBUILD_PS1="tools/window/rebuild-server.ps1"
STAMP="mir2-rs/target/release/mir2-server.build.json"

RUN_DIR=${WINDOW_RUN_DIR:-E:/tmp/window/run3}
CARD=${WINDOW_CARD:-openmir2-window-run-2}
TIMEOUT_SEC=${WINDOW_TIMEOUT_SEC:-2700}
PY=${WINDOW_PYTHON:-python}

DO_REBUILD=1
DO_PREFLIGHT=1
DRY_RUN=0
FORCE=0
SKIP_PIN_SMOKE=0
RNG_LOG=0
ONLY=""
FROM=""

usage() {
    sed -n '3,43p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
}

die() {
    echo "run_batch: $1" >&2
    echo "run_batch: 缺参/前置失败 ⇒ rc=2（fail-closed；--help 看用法）" >&2
    exit 2
}

while [ $# -gt 0 ]; do
    case "$1" in
        --help|-h)        usage; exit 0 ;;
        --list)           LIST_ONLY=1; shift ;;
        --dry-run)        DRY_RUN=1; shift ;;
        --run-dir)        [ $# -ge 2 ] || die "--run-dir 缺值"; RUN_DIR=$2; shift 2 ;;
        --card)           [ $# -ge 2 ] || die "--card 缺值"; CARD=$2; shift 2 ;;
        --only)           [ $# -ge 2 ] || die "--only 缺值"; ONLY=$2; shift 2 ;;
        --from)           [ $# -ge 2 ] || die "--from 缺值"; FROM=$2; shift 2 ;;
        --timeout-sec)    [ $# -ge 2 ] || die "--timeout-sec 缺值"; TIMEOUT_SEC=$2; shift 2 ;;
        --force)          FORCE=1; shift ;;
        --no-rebuild)     DO_REBUILD=0; shift ;;
        --no-preflight)   DO_PREFLIGHT=0; shift ;;
        --skip-pin-smoke) SKIP_PIN_SMOKE=1; shift ;;
        --rng-log)        RNG_LOG=1; shift ;;
        --python)         [ $# -ge 2 ] || die "--python 缺值"; PY=$2; shift 2 ;;
        *)                die "未知参数 $1" ;;
    esac
done
LIST_ONLY=${LIST_ONLY:-0}

[ -n "$ONLY" ] && [ -n "$FROM" ] && die "--only 与 --from 不能同时给"
if [ -n "$FROM" ]; then
    # 上界在解析清单（得 N）后再校验；这里只查「正整数」形状。
    case "$FROM" in ''|*[!0-9]*) die "--from 必须是正整数（实际 $FROM）" ;; esac
    [ "$FROM" -ge 1 ] || die "--from 必须 ≥1（实际 $FROM）"
fi
case "$TIMEOUT_SEC" in ''|*[!0-9]*) die "--timeout-sec 必须是整数" ;; esac

cd "$REPO" || die "进不了检出根：$REPO"
[ -f "$MANIFEST" ] || die "缺场景清单：$MANIFEST"
[ -f "$CARD_WINDOW" ] || die "缺 $CARD_WINDOW（在正确的检出里跑？）"
command -v "$PY" >/dev/null 2>&1 || die "找不到 python（--python 指定）"
command -v powershell >/dev/null 2>&1 || die "找不到 powershell"

# ---- 清单（唯一事实源；batch 与 preflight 共用 tools/window/run3-scenarios.json）----
# **场景数不硬编码**：期望值 = manifest 的 `scenarios` 条数 N，并校验 `n` 从 1 连续到 N
#（断号/重复/非正/缺字段 ⇒ rc=2 fail-closed）。`blocked` 场景只在 --list 里标注、不进批次。
# **blocked 有两个来源，两处都要写才闭环**（卡 openmir2-blocked-two-sources-doc）：
#   * 本清单行（`run3-scenarios.json` 的 `blocked`，本脚本 `:152-158` 读取）——管**批次排除**；
#   * 场景 JSON（`tools/probe/scenarios/<name>.json` 的 `blocked`，`card_window.py::scenario_blocked` 读取）
#     ——管**直接跑 card_window 时的起栈前 BLOCKED（rc=2）**。
#   只写 JSON：批次不会排除（`--list` 仍显示 blocked 0）；只写清单行：直接跑 card_window 仍会执行。
#   现有例子：`clearmapmon_t118` 只在场景 JSON（且不在本清单）；`butch_two_draws` 两处都有（#21）。
# 行格式（US/\x1f 分隔，空字段保留）：n  name  profile  extra  blocked(0/1)  reason
_rows_file=$(mktemp) || die "建不了临时文件"
if ! "$PY" -I -B - "$MANIFEST" >"$_rows_file" <<'PY'
import json, sys

path = sys.argv[1]
try:
    with open(path, encoding="utf-8-sig") as fh:
        doc = json.load(fh)
except Exception as ex:  # noqa: BLE001
    print("run_batch: 读不了清单 %s: %s" % (path, ex), file=sys.stderr)
    raise SystemExit(2)
sc = doc.get("scenarios")
if not isinstance(sc, list) or not sc:
    print("run_batch: 清单 %s 的 scenarios 必须是非空数组" % path, file=sys.stderr)
    raise SystemExit(2)


def clean(t):
    return (str(t).replace("\t", " ").replace("\n", " ").replace("\r", " ")
            .replace("\x1f", " "))


ns = []
rows = []
problems = []
for s in sc:
    if not isinstance(s, dict):
        problems.append("scenarios 元素必须是对象：%r" % (s,))
        continue
    n = s.get("n")
    name = s.get("name")
    prof = s.get("profile")
    if not isinstance(n, int) or isinstance(n, bool) or n <= 0:
        problems.append("scenario %r 的 n 必须是正整数（实际 %r）" % (name, n))
        continue
    if not isinstance(name, str) or not name:
        problems.append("scenario n=%s 缺 name" % n)
        continue
    if not isinstance(prof, str) or not prof:
        problems.append("scenario %s 缺 profile" % name)
        continue
    ns.append(n)
    extra = " ".join(str(x) for x in (s.get("extra") or []))
    b = s.get("blocked")
    if b is None or b is False:
        blocked, reason = "0", "-"
    elif b is True:
        blocked, reason = "1", "blocked（未给原因）"
    elif isinstance(b, str) and b.strip():
        blocked, reason = "1", b.strip()
    else:
        problems.append("scenario %s 的 blocked 必须是 bool/理由字符串" % name)
        continue
    rows.append("%d\x1f%s\x1f%s\x1f%s\x1f%s\x1f%s"
                % (n, clean(name), clean(prof), clean(extra), blocked, clean(reason)))

if sorted(ns) != list(range(1, len(ns) + 1)):
    problems.append("n 必须从 1 连续到 %d（实际 %s）" % (len(ns), ns))
if problems:
    for p in problems:
        print("run_batch: 清单校验失败：%s" % p, file=sys.stderr)
    raise SystemExit(2)
for r in sorted(rows, key=lambda t: int(t.split("\x1f", 1)[0])):
    print(r)
PY
then
    rm -f "$_rows_file"
    die "清单校验失败（见上面 run_batch: 清单校验失败 行；fail-closed）"
fi
ROWS=()
while IFS= read -r row; do
    [ -n "$row" ] && ROWS+=("$row")
done <"$_rows_file"
rm -f "$_rows_file"
N=${#ROWS[@]}
[ "$N" -gt 0 ] || die "清单没有场景（scenarios 为空）"
if [ -n "$FROM" ] && [ "$FROM" -gt "$N" ]; then
    die "--from 必须是 1..$N 的整数（实际 $FROM）"
fi
# 已封存场景不得入列：pickup_item 结构性不可判（T-19/T-25），card_window 见 blocked 即 rc=2。
for _row in "${ROWS[@]}"; do
    case "$_row" in
        *pickup_item*) die "清单不得包含已封存场景 pickup_item（T-19/T-25，见 docs/pickup-item-reachability-20261024.md）" ;;
    esac
done

declare -A ONLY_MAP=()
if [ -n "$ONLY" ]; then
    IFS=',' read -r -a _only_tokens <<<"$ONLY"
    for tok in "${_only_tokens[@]}"; do
        tok=$(echo "$tok" | tr -d '[:space:]')
        [ -n "$tok" ] && ONLY_MAP["$tok"]=1
    done
fi

selected=()
BLOCKED_N=0
declare -A SEEN_ONLY=()
for row in "${ROWS[@]}"; do
    IFS=$'\x1f' read -r n name prof extra blocked reason <<<"$row"
    take=1
    requested=0
    if [ "${#ONLY_MAP[@]}" -gt 0 ]; then
        if [ -n "${ONLY_MAP[$n]:-}" ] || [ -n "${ONLY_MAP[$name]:-}" ]; then
            requested=1
            SEEN_ONLY["$n"]=1
            SEEN_ONLY["$name"]=1
        else
            take=0
        fi
    fi
    if [ -n "$FROM" ]; then
        [ "$n" -ge "$FROM" ] || take=0
    fi
    if [ "$blocked" = "1" ]; then
        BLOCKED_N=$((BLOCKED_N + 1))
        [ "$requested" -eq 1 ] && die "场景 $name（n=$n）为 blocked，不进批次：$reason"
        take=0
    fi
    [ "$take" -eq 1 ] && selected+=("$row")
done

if [ "${#ONLY_MAP[@]}" -gt 0 ] && [ "${#SEEN_ONLY[@]}" -eq 0 ]; then
    die "--only 没匹配到任何场景：$ONLY"
fi
if [ "${#ONLY_MAP[@]}" -gt 0 ]; then
    for tok in "${!ONLY_MAP[@]}"; do
        [ -n "${SEEN_ONLY[$tok]:-}" ] || die "--only 里的 $tok 不存在"
    done
fi
if [ "$LIST_ONLY" -eq 1 ]; then
    printf '%-3s %-20s %-13s %-7s %s\n' '#' 'scenario' 'profile' 'blocked' 'extra'
    for row in "${ROWS[@]}"; do
        IFS=$'\x1f' read -r n name prof extra blocked reason <<<"$row"
        if [ "$blocked" = "1" ]; then
            printf '%-3s %-20s %-13s %-7s %s | blocked: %s\n' \
                "$n" "$name" "$prof" 'YES' "${extra:-}" "$reason"
        else
            printf '%-3s %-20s %-13s %-7s %s\n' "$n" "$name" "$prof" '-' "${extra:-}"
        fi
    done
    echo "场景数 $N（blocked $BLOCKED_N ⇒ 入批 ${#selected[@]}）"
    exit 0
fi
[ "${#selected[@]}" -gt 0 ] || die "没有选中任何场景（--from $FROM / 全部 blocked？）"

# ---- 日志/证据目录 ----
if [ "$DRY_RUN" -eq 1 ]; then
    # dry-run 的日志/计划放到 run3 **旁边**（run3-dry-run），不污染真证据目录
    LOG_DIR="${RUN_DIR%/}-dry-run"
else
    LOG_DIR="$RUN_DIR"
fi
mkdir -p "$LOG_DIR" || die "建不了日志目录：$LOG_DIR"
BATCH_LOG="$LOG_DIR/batch.log"
BATCH_JSONL="$LOG_DIR/batch.jsonl"
touch "$BATCH_LOG"
[ "$DRY_RUN" -eq 1 ] || touch "$BATCH_JSONL"

log() {
    local line
    line="$(date +%H:%M:%S) run_batch: $*"
    printf '%s\n' "$line" >>"$BATCH_LOG"
    printf '%s\n' "$line"
}

now_iso() { date +%Y-%m-%dT%H:%M:%S%z; }

HEAD=$(git rev-parse HEAD 2>/dev/null) || die "git rev-parse HEAD 失败"
log "清单：场景数 $N，blocked $BLOCKED_N ⇒ 入批 ${#selected[@]}（清单=$MANIFEST）"
log "start card=$CARD run_dir=$RUN_DIR head=${HEAD:0:12} selected=${#selected[@]} dry_run=$DRY_RUN timeout=${TIMEOUT_SEC}s"

# ---- timeout 可用性（GNU coreutils；Git bash 自带）----
USE_TIMEOUT=0
if command -v timeout >/dev/null 2>&1 && timeout --version 2>/dev/null | grep -qi 'gnu coreutils'; then
    USE_TIMEOUT=1
else
    log "WARNING: 没有 GNU timeout ⇒ 场景不设外层超时（card_window 自己的子进程超时仍生效）"
fi

# ---- ① release 重建（对齐 head + sha256）----
if [ "$DO_REBUILD" -eq 1 ]; then
    if [ "$DRY_RUN" -eq 1 ]; then
        log "PLAN rebuild: powershell -NoProfile -ExecutionPolicy Bypass -File $REBUILD_PS1 -SyncStartRust -RequireClean"
    else
        log "== rebuild release（$REBUILD_PS1 -SyncStartRust -RequireClean）"
        if powershell -NoProfile -ExecutionPolicy Bypass -File "$REBUILD_PS1" \
                -SyncStartRust -RequireClean >"$LOG_DIR/rebuild.log" 2>&1; then
            sed -n '1,40p' "$LOG_DIR/rebuild.log" | while IFS= read -r ln; do log "  $ln"; done
        else
            sed -n '1,80p' "$LOG_DIR/rebuild.log" >&2
            die "release 重建失败（见 $LOG_DIR/rebuild.log）"
        fi
    fi
fi

# ---- ② 前置自检（只读；不起栈/不占端口/不连库）----
run_preflight() {
    local dir="$LOG_DIR/preflight" rc=0
    mkdir -p "$dir"
    local priv_args=(--json "$dir/preflight-private.json" --manifest "$MANIFEST")
    [ "$SKIP_PIN_SMOKE" -eq 1 ] && priv_args+=(--skip-pin-smoke)
    log "== preflight private（$PREFLIGHT_PRIVATE）"
    "$PY" -I -B "$PREFLIGHT_PRIVATE" "${priv_args[@]}" >"$dir/preflight-private.log" 2>&1
    rc=$?
    sed 's/^/  /' "$dir/preflight-private.log" | while IFS= read -r ln; do log "$ln"; done
    if [ "$rc" -ne 0 ]; then
        log "PREFLIGHT FAIL private rc=$rc（见 $dir/preflight-private.log）"
        return 2
    fi
    for prof in alt window-magic; do
        log "== preflight public --profile $prof --with-dry-run"
        "$PY" -I -B "$PREFLIGHT_PUBLIC" --profile "$prof" --with-dry-run \
            >"$dir/preflight-$prof.log" 2>&1
        rc=$?
        sed 's/^/  /' "$dir/preflight-$prof.log" | while IFS= read -r ln; do log "$ln"; done
        if [ "$rc" -ne 0 ]; then
            log "PREFLIGHT FAIL public($prof) rc=$rc（见 $dir/preflight-$prof.log）"
            return 2
        fi
    done
    log "PREFLIGHT PASS（private + alt + window-magic）"
    return 0
}

if [ "$DO_PREFLIGHT" -eq 1 ]; then
    if ! run_preflight; then
        die "前置自检未通过 ⇒ 不开跑（先按上面的 fix 行修；调试可 --no-preflight）"
    fi
else
    log "WARNING: --no-preflight ⇒ 跳过全部前置自检（证据里必须登记）"
fi

# ---- ③ 续跑判据 ----
is_complete() {
    "$PY" -I -B - "$1" "$HEAD" <<'PY'
import json, os, sys
path, head = sys.argv[1], sys.argv[2]
if not os.path.isfile(path):
    print("no")
    raise SystemExit
try:
    with open(path, encoding="utf-8-sig") as fh:
        d = json.load(fh)
except Exception:
    print("no")
    raise SystemExit
f = d.get("frames") or {}
arts = d.get("artifacts") or {}
ok = (d.get("verdict") in ("PASS", "FAIL")
      and isinstance(d.get("compare_rc"), int)
      and d.get("report_kind") == "compare"
      and int(f.get("compared") or 0) > 0
      and "README.txt" in arts and "report.json" in arts
      and d.get("git_head") == head)
print("yes" if ok else "no")
PY
}

# ---- ④ 场景清理（每个场景后兜底；card_window 自己已在 finally 停栈）----
cleanup_stack() {
    powershell -NoProfile -Command "Get-CimInstance Win32_Process | Where-Object { \$_.ExecutablePath -like 'E:\\tmp\\conformance\\csharp-root\\*' } | ForEach-Object { Stop-Process -Id \$_.ProcessId -Force }" >/dev/null 2>&1 || true
    powershell -NoProfile -ExecutionPolicy Bypass -File E:/tmp/conformance/stop-rust.ps1 >/dev/null 2>&1 || true
}

# ---- ⑤ 单场景 ----
record_row() {
    # $1 jsonl target, rest: key=value（由调用方拼好 JSON 片段）
    printf '%s\n' "$1" >>"$BATCH_JSONL"
}

run_one() {
    local n=$1 name=$2 prof=$3 extra=$4
    local out="$RUN_DIR/$name" logf="$RUN_DIR/$name.log"
    local card_json="$out/card-window.json"
    local started finished duration outer_rc verdict cmp_rc cw_exit o_fr r_fr cmp_fr
    local skipped=0 skip_reason=""
    local outer_rc=- duration=0

    if [ "$FORCE" -eq 0 ] && [ "$(is_complete "$card_json")" = "yes" ]; then
        skipped=1
        skip_reason="resume: complete evidence at same HEAD"
    fi

    if [ "$skipped" -eq 1 ]; then
        log "SKIP  n=$n name=$name（$skip_reason）"
    else
        if [ "$DRY_RUN" -eq 1 ]; then
            local rngflag=""
            [ "$RNG_LOG" -eq 1 ] && rngflag="--rng-log"
            log "PLAN  n=$n name=$name profile=$prof extra=[$extra] rng_log=$RNG_LOG out=$out"
            log "      $PY -I -B $CARD_WINDOW --card $CARD --scenario tools/probe/scenarios/$name.json --out $out --profile $prof ${rngflag} ${extra}"
            return 0
        fi
        log "START n=$n name=$name profile=$prof extra=[$extra] at=$(now_iso)"
        mkdir -p "$out" || die "建不了场景目录：$out"
        local -a cmd=("$PY" -I -B "$CARD_WINDOW" --card "$CARD"
                      --scenario "tools/probe/scenarios/$name.json"
                      --out "$out" --profile "$prof")
        if [ "$RNG_LOG" -eq 1 ]; then
            cmd+=("--rng-log")
        fi
        if [ -n "$extra" ]; then
            local -a ex=()
            read -r -a ex <<<"$extra"
            cmd+=("${ex[@]}")
        fi
        started=$(date +%s)
        if [ "$USE_TIMEOUT" -eq 1 ]; then
            timeout -k 60 "$TIMEOUT_SEC" "${cmd[@]}" >"$logf" 2>&1
            outer_rc=$?
        else
            "${cmd[@]}" >"$logf" 2>&1
            outer_rc=$?
        fi
        finished=$(date +%s)
        duration=$((finished - started))
        # **rc 双列口径**：outer_rc = 本脚本看到的外层返回码（0/2/124/137）；
        # compare_rc 从 card-window.json 读（0=PASS / 1=FAIL / null=判不了）。
        # 注意：run-2 的 batch.log 因 `echo "... $(date) ... $?"` 里命令替换先吃掉 $?
        # 而恒记 0；这里先落 outer_rc 再碰任何命令替换。
        cleanup_stack
    fi

    # 读证据（跳过时也读，便于汇总）
    if [ -f "$card_json" ]; then
        local meta
        meta=$("$PY" -I -B - "$card_json" <<'PY'
import json, sys
try:
    with open(sys.argv[1], encoding="utf-8-sig") as fh:
        d = json.load(fh)
except Exception:
    print("-\t-\t-\t-\t-\t-")
    raise SystemExit

def s(v):
    return "-" if v is None else str(v)

f = d.get("frames") or {}
print("\t".join([s(d.get("verdict")), s(d.get("compare_rc")), s(d.get("exit_code")),
                 s(f.get("oracle_total")), s(f.get("rust_total")), s(f.get("compared"))]))
PY
)
        IFS=$'\t' read -r verdict cmp_rc cw_exit o_fr r_fr cmp_fr <<<"$meta"
    else
        verdict="-"; cmp_rc="-"; cw_exit="-"; o_fr="-"; r_fr="-"; cmp_fr="-"
    fi

    if [ "$skipped" -eq 0 ]; then
        log "RESULT n=$n name=$name verdict=$verdict outer_rc=$outer_rc compare_rc=$cmp_rc exit_code=$cw_exit frames=${o_fr}/${r_fr}/${cmp_fr} duration=${duration}s"
    fi

    local json
    json=$("$PY" -I -B - "$n" "$name" "$prof" "$extra" "$verdict" "$cmp_rc" "$cw_exit" \
        "$o_fr" "$r_fr" "$cmp_fr" "$outer_rc" "$duration" "$skipped" "$skip_reason" \
        "$out" "$logf" <<'PY'
import json, sys
(n, name, prof, extra, verdict, cmp_rc, cw_exit, o_fr, r_fr, cmp_fr,
 outer_rc, duration, skipped, skip_reason, out, logf) = sys.argv[1:17]

def maybe_int(v):
    return int(v) if v.lstrip("-").isdigit() else None

print(json.dumps({
    "n": int(n), "name": name, "profile": prof,
    "extra": extra.split() if extra else [],
    "verdict": verdict, "compare_rc": maybe_int(cmp_rc),
    "exit_code": maybe_int(cw_exit), "outer_rc": maybe_int(outer_rc) if not skipped else None,
    "frames": {"oracle": maybe_int(o_fr), "rust": maybe_int(r_fr), "compared": maybe_int(cmp_fr)},
    "duration_s": None if skipped else int(duration),
    "skipped": bool(int(skipped)), "skip_reason": skip_reason or None,
    "dir": out, "log": logf,
    "evidence": {"card_window_json": out + "/card-window.json",
                 "report_json": out + "/report.json",
                 "readme": out + "/README.txt"},
}, ensure_ascii=False, sort_keys=True))
PY
)
    record_row "$json"
    if [ "$verdict" = "UNDECIDABLE" ] || [ "$verdict" = "-" ]; then
        return 2
    fi
    return 0
}

bad=0
for row in "${selected[@]}"; do
    IFS=$'\x1f' read -r n name prof extra blocked reason <<<"$row"
    if ! run_one "$n" "$name" "$prof" "$extra"; then
        bad=1
    fi
done

# ---- ⑥ 汇总（batch.jsonl 增量保留；summary 每次重算）----
if [ "$DRY_RUN" -eq 1 ]; then
    log "dry-run 完成：未起栈、未占端口、未写场景；计划 $LOG_DIR/batch.log"
    exit 0
fi

"$PY" -I -B - "$RUN_DIR" "$MANIFEST" "$CARD" "$HEAD" >"$LOG_DIR/summary-build.log" 2>&1 <<'PY'
import hashlib, json, os, sys, time
run_dir, manifest, card, head = sys.argv[1:5]

def sha256(path):
    h = hashlib.sha256()
    with open(path, "rb") as fh:
        for chunk in iter(lambda: fh.read(65536), b""):
            h.update(chunk)
    return h.hexdigest()

with open(manifest, encoding="utf-8-sig") as fh:
    order = [s["name"] for s in json.load(fh)["scenarios"]]

rows = []
seen = set()
jsonl = os.path.join(run_dir, "batch.jsonl")
if os.path.isfile(jsonl):
    with open(jsonl, encoding="utf-8-sig") as fh:
        for line in fh:
            line = line.strip()
            if not line:
                continue
            try:
                d = json.loads(line)
            except ValueError:
                continue
            rows.append(d)
            seen.add(d.get("name"))
# jsonl 可能是同一场景多次（中断后续跑），保留最后一条
last = {}
for d in rows:
    last[d["name"]] = d
ordered = []
for name in order:
    ordered.append(last.get(name) or {"name": name, "verdict": "NOT_RUN", "skipped": False})

tot = {"PASS": 0, "FAIL": 0, "UNDECIDABLE": 0, "NOT_RUN": 0}
for d in ordered:
    v = d.get("verdict")
    if v in tot:
        tot[v] += 1
    elif v == "-":
        tot["UNDECIDABLE"] += 1

stamp = {}
# batch 的 cwd 就是检出根：相对当前目录读 build stamp
cand = os.path.join(os.getcwd(), "mir2-rs", "target", "release", "mir2-server.build.json")
if os.path.isfile(cand):
    with open(cand, encoding="utf-8-sig") as fh:
        stamp = json.load(fh)

doc = {
    "format": "window-batch-summary/1",
    "card": card,
    "run_dir": run_dir,
    "repo": os.getcwd(),
    "git_head": head,
    "generated_at": time.strftime("%Y-%m-%dT%H:%M:%S%z"),
    "rust_binary": {
        "path": stamp.get("exe"),
        "sha256": stamp.get("sha256"),
        "build_stamp": stamp or None,
    },
    "totals": tot,
    "scenarios": ordered,
}
path = os.path.join(run_dir, "batch-summary.json")
with open(path, "w", encoding="utf-8", newline="\n") as fh:
    json.dump(doc, fh, ensure_ascii=False, indent=2, sort_keys=True)
    fh.write("\n")
print("wrote " + path)
PY
summary_rc=$?
if [ "$summary_rc" -ne 0 ]; then
    log "WARNING: batch-summary.json 生成失败（见 $LOG_DIR/summary-build.log）"
fi

log "END 汇总：$RUN_DIR/batch-summary.json"
if [ -f "$RUN_DIR/batch-summary.json" ]; then
    "$PY" -I -B - "$RUN_DIR/batch-summary.json" <<'PY' | while IFS= read -r ln; do log "$ln"; done
import json, sys
with open(sys.argv[1], encoding="utf-8-sig") as fh:
    d = json.load(fh)
print("TOTALS " + " ".join("%s=%s" % (k, v) for k, v in sorted(d["totals"].items())))
for s in d["scenarios"]:
    print("ROW %02d %-20s %-13s verdict=%-11s outer_rc=%-4s compare_rc=%-4s%s" % (
        s.get("n") or 0, s["name"], s.get("profile") or "-",
        s.get("verdict"), s.get("outer_rc"), s.get("compare_rc"),
        "  (skipped)" if s.get("skipped") else ""))
PY
fi

# ---- 跑后对称性复核（卡 openmir2-postrun-symmetry-drift）----
# C# 运行期会按内存重写 Castle/List.txt 与 SabukW ⇒ 跑后漂移是预期行为，只标注（不改本轮 rc）。
log "== 跑后对称性复核（预期可能漂移；下一轮前须还原/对齐）"
CARD_WINDOW="$CARD_WINDOW" "$PY" -I -B - <<'PY' | while IFS= read -r ln; do log "$ln"; done
import importlib.util, os
path = os.environ["CARD_WINDOW"]
spec = importlib.util.spec_from_file_location("_postrun_cw", path)
mod = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mod)
prof = mod.load_profile("alt")
drift = mod.postrun_symmetry(prof)
if not drift:
    print("跑后对称性：OK")
else:
    files = ",".join((d.get("file") or "?") for d in drift[:8])
    print("跑后对称性：DRIFT(%d) %s" % (len(drift), files))
PY

if [ "$bad" -ne 0 ]; then
    log "批次结论：有 UNDECIDABLE/未产出证据的场景 ⇒ rc=2（UNDECIDABLE 不是通过）"
    exit 2
fi
log "批次结论：全部选中场景均有结论（PASS/FAIL）⇒ rc=0"
exit 0
