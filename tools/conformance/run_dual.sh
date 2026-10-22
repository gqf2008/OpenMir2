#!/usr/bin/env bash
# -*- coding: utf-8 -*-
# run_dual.sh — 双栈无头对拍「一条命令」：恢复基线 → 起 A 栈 → 跑场景 → 抓库快照 → 停栈
#               → 起 B 栈 → 同样 → compare（帧流 + 库后态）。
#
# 设计要点
#   * 端口、凭据、账号**一律不写死**：栈命令/驱动器参数要么来自命令行，要么来自环境变量
#     （见 --help）。本脚本里没有任何 HOST:PORT、账号或密码常量。
#   * **缺参 fail-closed：退出码 2**（不是 1：1 表示「有真实差异」，2 表示「判不了/没跑成」，
#     与 compare_runs.py 的口径一致）。任何一步失败也一律 rc=2，绝不用后续 PASS 掩盖。
#   * 停栈命令**无论中途是否失败都会执行**（失败时也要把栈停掉，别占端口）。
#   * 两侧串行、同一组端口、同一份库基线：与 docs/conformance-first-dual-run-20261023.md §1 一致。
#
# 用法（最典型）：
#   CONFORMANCE_PASSWORD=... tools/conformance/run_dual.sh \
#     --scenario tools/probe/scenarios/login_only.json \
#     --login-gate HOST:PORT --account '<账号>' --server-name '<服务器名>' \
#     --stack-a-cmd 'E:/tmp/conformance/start-csharp3.ps1' \
#     --stop-a-cmd  'E:/tmp/conformance/stop-csharp.ps1' \
#     --stack-b-cmd 'E:/tmp/conformance/start-rust.ps1' \
#     --stop-b-cmd  'E:/tmp/conformance/stop-rust.ps1' \
#     --restore-cmd 'E:/tmp/conformance/restore-baseline.sh' \
#     --db-a-cmd 'E:/tmp/conformance/snap-csharp.sh' \
#     --db-b-cmd 'E:/tmp/conformance/snap-rust.sh' \
#     --out E:/tmp/conformance/runs/round2
#
# 命令语义
#   --stack-a-cmd / --stack-b-cmd   启动命令：**必须返回**（栈在后台起好后返回 0），
#                                   阻塞不退的命令会让脚本卡住（脚本不做超时看护）。
#   --stop-a-cmd  / --stop-b-cmd    停止命令：在场景 + 抓库之后执行；失败也继续。
#   --restore-cmd                   基线恢复：**每侧各跑一次**（起 A 前、起 B 前）。
#   --db-a-cmd / --db-b-cmd         库快照：应把快照写到 $DUAL_RUN_DIR/db_snapshot.json
#                                   （脚本导出的环境变量；不给 compare_runs 就会记录
#                                   db.state=absent_both，本次不判库后态）。
#   --probe-cmd                     驱动器命令前缀（默认仓库内 tools/probe/mir2_probe.py）。
#
# 环境变量（命令行参数优先；密码建议只走环境，别进命令行历史）
#   CONFORMANCE_SCENARIO CONFORMANCE_STACK_A_CMD CONFORMANCE_STACK_B_CMD CONFORMANCE_OUT
#   CONFORMANCE_LOGIN_GATE CONFORMANCE_ACCOUNT CONFORMANCE_PASSWORD CONFORMANCE_SERVER_NAME
#   CONFORMANCE_CHAR_NAME CONFORMANCE_RESTORE_CMD CONFORMANCE_STOP_A_CMD CONFORMANCE_STOP_B_CMD
#   CONFORMANCE_DB_A_CMD CONFORMANCE_DB_B_CMD CONFORMANCE_PROBE_CMD CONFORMANCE_RULES
#
# 退出码：0 = compare PASS；1 = compare FAIL（有真实差异）；2 = 缺参/某步失败/UNDECIDABLE。
set -u
set -o pipefail

HERE=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
REPO_ROOT=$(cd "$HERE/../.." && pwd)

SCENARIO=${CONFORMANCE_SCENARIO:-}
STACK_A=${CONFORMANCE_STACK_A_CMD:-}
STACK_B=${CONFORMANCE_STACK_B_CMD:-}
OUT=${CONFORMANCE_OUT:-}
RESTORE_CMD=${CONFORMANCE_RESTORE_CMD:-}
STOP_A=${CONFORMANCE_STOP_A_CMD:-}
STOP_B=${CONFORMANCE_STOP_B_CMD:-}
DB_A=${CONFORMANCE_DB_A_CMD:-}
DB_B=${CONFORMANCE_DB_B_CMD:-}
PROBE_CMD=${CONFORMANCE_PROBE_CMD:-}
RULES=${CONFORMANCE_RULES:-$HERE/rules.json}
LOGIN_GATE=${CONFORMANCE_LOGIN_GATE:-}
ACCOUNT=${CONFORMANCE_ACCOUNT:-}
PASSWORD=${CONFORMANCE_PASSWORD:-}
SERVER_NAME=${CONFORMANCE_SERVER_NAME:-}
CHAR_NAME=${CONFORMANCE_CHAR_NAME:-}
PROBE_EXTRA=()
COMPARE_EXTRA=()
SELFTEST=0

usage() {
    awk 'NR > 2 && /^#/ {sub(/^# ?/, ""); print; next} NR > 2 && !/^#/ {exit}' "${BASH_SOURCE[0]}"
}

die_usage() {
    echo "run_dual.sh: $1" >&2
    echo "（缺参/用法错误 ⇒ 退出码 2，fail-closed；--help 看用法）" >&2
    exit 2
}

while [ $# -gt 0 ]; do
    case "$1" in
        --scenario)      [ $# -ge 2 ] || die_usage "--scenario 缺值"; SCENARIO=$2; shift 2 ;;
        --stack-a-cmd)   [ $# -ge 2 ] || die_usage "--stack-a-cmd 缺值"; STACK_A=$2; shift 2 ;;
        --stack-b-cmd)   [ $# -ge 2 ] || die_usage "--stack-b-cmd 缺值"; STACK_B=$2; shift 2 ;;
        --out)           [ $# -ge 2 ] || die_usage "--out 缺值"; OUT=$2; shift 2 ;;
        --restore-cmd)   [ $# -ge 2 ] || die_usage "--restore-cmd 缺值"; RESTORE_CMD=$2; shift 2 ;;
        --stop-a-cmd)    [ $# -ge 2 ] || die_usage "--stop-a-cmd 缺值"; STOP_A=$2; shift 2 ;;
        --stop-b-cmd)    [ $# -ge 2 ] || die_usage "--stop-b-cmd 缺值"; STOP_B=$2; shift 2 ;;
        --db-a-cmd)      [ $# -ge 2 ] || die_usage "--db-a-cmd 缺值"; DB_A=$2; shift 2 ;;
        --db-b-cmd)      [ $# -ge 2 ] || die_usage "--db-b-cmd 缺值"; DB_B=$2; shift 2 ;;
        --probe-cmd)     [ $# -ge 2 ] || die_usage "--probe-cmd 缺值"; PROBE_CMD=$2; shift 2 ;;
        --probe-arg)     [ $# -ge 2 ] || die_usage "--probe-arg 缺值"; PROBE_EXTRA+=("$2"); shift 2 ;;
        --rules)         [ $# -ge 2 ] || die_usage "--rules 缺值"; RULES=$2; shift 2 ;;
        --login-gate)    [ $# -ge 2 ] || die_usage "--login-gate 缺值"; LOGIN_GATE=$2; shift 2 ;;
        --account)       [ $# -ge 2 ] || die_usage "--account 缺值"; ACCOUNT=$2; shift 2 ;;
        --password)      [ $# -ge 2 ] || die_usage "--password 缺值"; PASSWORD=$2; shift 2 ;;
        --server-name)   [ $# -ge 2 ] || die_usage "--server-name 缺值"; SERVER_NAME=$2; shift 2 ;;
        --char-name)     [ $# -ge 2 ] || die_usage "--char-name 缺值"; CHAR_NAME=$2; shift 2 ;;
        --allow-missing-fields|--require-db|--allow-empty|--quiet)
                         COMPARE_EXTRA+=("$1"); shift ;;
        --selftest)      SELFTEST=1; shift ;;
        --help|-h)       usage; exit 0 ;;
        *)               die_usage "未知参数 $1" ;;
    esac
done

log() {
    printf '%s run_dual: %s\n' "$(date -u +%H:%M:%S)" "$*" >&2
    printf '%s run_dual: %s\n' "$(date -u +%H:%M:%S)" "$*" >>"${LOG_FILE:-/dev/null}"
}

# ---------------------------------------------------------------------------
# 自检（离线，不连网/不连库/不起真栈）
# ---------------------------------------------------------------------------
selftest() {
    local tmp
    tmp=$(mktemp -d "${TMPDIR:-/tmp}/run-dual-selftest.XXXXXX") || return 2
    local pass=0 fail=0
    chk() { if eval "$1"; then pass=$((pass+1)); echo "PASS $2"; else fail=$((fail+1)); echo "FAIL $2"; fi; }
    : >"$tmp/scenario.json"

    # 桩：每个命令都只写 flag / 合成 frames.ndjson
    cat >"$tmp/restore.sh" <<'STUB'
#!/usr/bin/env bash
printf 'restore %s\n' "${DUAL_SIDE:-?}" >>"$DUAL_OUT/restore.flag"
STUB
    cat >"$tmp/stack.sh" <<'STUB'
#!/usr/bin/env bash
printf 'up %s\n' "${DUAL_SIDE:-?}" >>"$DUAL_OUT/stack.flag"
STUB
    cat >"$tmp/stop.sh" <<'STUB'
#!/usr/bin/env bash
printf 'down %s\n' "${DUAL_SIDE:-?}" >>"$DUAL_OUT/stop.flag"
STUB
    cat >"$tmp/db.sh" <<'STUB'
#!/usr/bin/env bash
printf 'db %s\n' "${DUAL_SIDE:-?}" >>"$DUAL_OUT/db.flag"
STUB
    cat >"$tmp/probe.sh" <<'STUB'
#!/usr/bin/env bash
# 只认 --out；写一帧 658（规则表里 compare），可选 --mutate 让 b 侧差一个字节
out=""
mutate="${STUB_MUTATE:-0}"
while [ $# -gt 0 ]; do
    case "$1" in
        --out) out=$2; shift 2 ;;
        --mutate) mutate=1; shift ;;
        *) shift ;;
    esac
done
[ -n "$out" ] || { echo "stub probe: 缺 --out" >&2; exit 2; }
mkdir -p "$out"
body="ok"
if [ "$mutate" = "1" ] && [ "${DUAL_SIDE:-a}" = "b" ]; then body="no"; fi
hex=$(printf '%s' "$body" | od -An -tx1 | tr -d ' \n')
sha=$(printf '%s' "$body" | sha256sum | cut -d' ' -f1)
printf '{"ident":658,"phase":"notice","seq":1,"t_ms":1,"conn":"%s","recog":0,"param":0,"tag":0,"series":0,"body_len":%d,"body_hex":"%s","body_sha256":"%s"}\n' \
    "${DUAL_SIDE:-a}" "${#body}" "$hex" "$sha" >"$out/frames.ndjson"
printf 'probe %s\n' "${DUAL_SIDE:-a}" >>"$DUAL_OUT/probe.flag"
exit 0
STUB
    chmod +x "$tmp"/*.sh

    local o="$tmp/run"
    # 1) 正常路径：四步都跑，PASS
    bash "$0" --scenario "$tmp/scenario.json" --login-gate 127.0.0.1:1 --account stub --password stub \
        --stack-a-cmd "bash $tmp/stack.sh" --stack-b-cmd "bash $tmp/stack.sh" \
        --stop-a-cmd "bash $tmp/stop.sh" --stop-b-cmd "bash $tmp/stop.sh" \
        --restore-cmd "bash $tmp/restore.sh" \
        --db-a-cmd "bash $tmp/db.sh" --db-b-cmd "bash $tmp/db.sh" \
        --probe-cmd "bash $tmp/probe.sh" --out "$o" >/dev/null 2>&1
    local rc=$?
    chk '[ "$rc" = 0 ]' "全流程（恢复/起栈/场景/抓库/停栈 x2 + compare）=> rc 0"
    chk '[ "$(grep -c "^up" "$o/stack.flag" 2>/dev/null)" = 2 ]' "两侧各自起栈一次"
    chk '[ "$(grep -c "^restore" "$o/restore.flag" 2>/dev/null)" = 2 ]' "起每侧栈之前各恢复一次基线"
    chk '[ "$(grep -c "^down" "$o/stop.flag" 2>/dev/null)" = 2 ]' "两侧都停了栈"
    chk '[ "$(grep -c "^db" "$o/db.flag" 2>/dev/null)" = 2 ]' "两侧各抓一次库快照"
    chk '[ "$(python -I -c "import json,sys;print(json.load(open(sys.argv[1]))[\"verdict\"])" "$o/report.json")" = PASS ]' "产物 compare 报 PASS"

    # 2) 牙：b 侧差一个字节 => rc 1（不是绿）
    local o2="$tmp/run-bad"
    STUB_MUTATE=1 bash "$0" --scenario "$tmp/scenario.json" --login-gate 127.0.0.1:1 --account stub --password stub \
        --stack-a-cmd "bash $tmp/stack.sh" --stack-b-cmd "bash $tmp/stack.sh" \
        --probe-cmd "bash $tmp/probe.sh" --out "$o2" >/dev/null 2>&1
    rc=$?
    chk '[ "$rc" = 1 ]' "b 侧差一个字节 => rc 1（FAIL 不被吞）"

    # 3) 缺参 fail-closed => 2
    "$0" --stack-a-cmd "true" --stack-b-cmd "true" --out "$tmp/x" >/dev/null 2>&1
    chk '[ "$?" = 2 ]' "缺 --scenario => rc 2"
    bash "$0" --scenario "$tmp/s.json" --stack-a-cmd "true" --out "$tmp/x" >/dev/null 2>&1
    chk '[ "$?" = 2 ]' "缺 --stack-b-cmd => rc 2"
    bash "$0" --scenario "$tmp/s.json" --stack-a-cmd "true" --stack-b-cmd "true" >/dev/null 2>&1
    chk '[ "$?" = 2 ]' "缺 --out => rc 2"
    "$0" --bogus >/dev/null 2>&1
    chk '[ "$?" = 2 ]' "未知参数 => rc 2"

    # 4) 起栈失败 => rc 2，且不跑第二个栈
    local o3="$tmp/run-fail"
    bash "$0" --scenario "$tmp/scenario.json" --login-gate 127.0.0.1:1 --account stub --password stub \
        --stack-a-cmd "false" --stack-b-cmd "bash $tmp/stack.sh" \
        --probe-cmd "bash $tmp/probe.sh" --out "$o3" >/dev/null 2>&1
    rc=$?
    chk '[ "$rc" = 2 ]' "A 栈起不来 => rc 2（不靠 compare 的 PASS 掩盖）"
    chk '[ ! -f "$o3/probe.flag" ]' "A 栈起不来 => 不跑场景"

    # 5) 停栈必须执行：场景失败也要停
    local o4="$tmp/run-stop"
    local badprobe="$tmp/badprobe.sh"
    printf '#!/usr/bin/env bash\nexit 3\n' >"$badprobe"
    chmod +x "$badprobe"
    bash "$0" --scenario "$tmp/scenario.json" --login-gate 127.0.0.1:1 --account stub --password stub \
        --stack-a-cmd "bash $tmp/stack.sh" --stack-b-cmd "bash $tmp/stack.sh" \
        --stop-a-cmd "bash $tmp/stop.sh" --stop-b-cmd "bash $tmp/stop.sh" \
        --probe-cmd "bash $badprobe" --out "$o4" >/dev/null 2>&1
    rc=$?
    chk '[ "$rc" = 2 ]' "场景命令失败 => rc 2"
    chk '[ "$(grep -c "^down" "$o4/stop.flag" 2>/dev/null)" = 1 ]' "A 栈场景失败后仍然被停掉"

    rm -rf "$tmp"
    if [ "$fail" -ne 0 ]; then
        echo "RED: run_dual.sh selftest $pass/$((pass+fail))"
        return 1
    fi
    echo "GREEN: run_dual.sh selftest $pass/$((pass+fail))"
    return 0
}

[ "$SELFTEST" = "1" ] && { selftest; exit $?; }

# ---------------------------------------------------------------------------
# 前置校验（全部 fail-closed ⇒ rc 2）
# ---------------------------------------------------------------------------
[ -n "$SCENARIO" ] || die_usage "缺 --scenario（场景 JSON）"
[ -n "$STACK_A" ] || die_usage "缺 --stack-a-cmd"
[ -n "$STACK_B" ] || die_usage "缺 --stack-b-cmd"
[ -n "$OUT" ] || die_usage "缺 --out（产物目录）"
[ -n "$LOGIN_GATE" ] || die_usage "缺 --login-gate HOST:PORT（脚本不写死端口）"
[ -n "$ACCOUNT" ] || die_usage "缺 --account（脚本不写死凭据）"
[ -n "$PASSWORD" ] || die_usage "缺 --password（建议用 CONFORMANCE_PASSWORD 环境变量给）"
[ -f "$SCENARIO" ] || die_usage "场景文件不存在：$SCENARIO"
[ -f "$HERE/compare_runs.py" ] || die_usage "缺比较器：$HERE/compare_runs.py"
[ -f "$RULES" ] || die_usage "规则表不存在：$RULES"

if [ -z "$PROBE_CMD" ]; then
    PROBE_CMD="python -I -B $REPO_ROOT/tools/probe/mir2_probe.py"
fi

mkdir -p "$OUT" || die_usage "建不了产物目录：$OUT"
OUT=$(cd "$OUT" && pwd)
LOG_FILE="$OUT/run_dual.log"
: >"$LOG_FILE"
log "out=$OUT scenario=$SCENARIO rules=$RULES"

read -r -a PROBE_BASE <<<"$PROBE_CMD"
[ ${#PROBE_BASE[@]} -gt 0 ] || die_usage "空 --probe-cmd"
PROBE_ARGV=("${PROBE_BASE[@]}"
            --login-gate "$LOGIN_GATE" --account "$ACCOUNT" --password "$PASSWORD"
            --scenario "$SCENARIO")
[ -n "$SERVER_NAME" ] && PROBE_ARGV+=(--server-name "$SERVER_NAME")
[ -n "$CHAR_NAME" ] && PROBE_ARGV+=(--char-name "$CHAR_NAME")
PROBE_ARGV+=("${PROBE_EXTRA[@]+"${PROBE_EXTRA[@]}"}")

run_step() {
    local name=$1
    shift
    log "== $name :: $*"
    "$@" >>"$LOG_FILE" 2>&1
    local rc=$?
    log "   $name rc=$rc"
    return $rc
}

# 返回 0 = 该侧完整跑完；2 = 中途失败（停栈仍会执行）
run_side() {
    local side=$1 start_cmd=$2 stop_cmd=$3 db_cmd=$4
    local dir="$OUT/$side"
    local rc_probe=2 rc_db=0 rc_stop=0 failed=0
    mkdir -p "$dir"
    export DUAL_SIDE="$side" DUAL_OUT="$OUT" DUAL_RUN_DIR="$dir" DUAL_SCENARIO="$SCENARIO"

    if [ -n "$RESTORE_CMD" ]; then
        run_step "restore-$side" bash -c "$RESTORE_CMD" || failed=1
    fi
    if [ "$failed" -ne 0 ]; then
        log "!! restore-$side 失败：不起栈、不跑场景（rc=2）"
        return 2
    fi

    if ! run_step "stack-$side" bash -c "$start_cmd"; then
        log "!! stack-$side 起不来：不跑场景（rc=2）"
        if [ -n "$stop_cmd" ]; then run_step "stop-$side" bash -c "$stop_cmd" || true; fi
        return 2
    fi

    run_step "probe-$side" "${PROBE_ARGV[@]}" --out "$dir"
    rc_probe=$?

    if [ -n "$db_cmd" ]; then
        run_step "db-$side" bash -c "$db_cmd"
        rc_db=$?
        if [ "$rc_db" -ne 0 ]; then
            log "!! db-$side 抓库失败（rc=$rc_db）：本次库后态不可判"
        fi
    fi

    if [ -n "$stop_cmd" ]; then
        run_step "stop-$side" bash -c "$stop_cmd"
        rc_stop=$?
        if [ "$rc_stop" -ne 0 ]; then
            log "!! stop-$side 失败（rc=$rc_stop）：检查端口是否仍被占"
        fi
    fi

    SIDE_RC_PROBE[$side]=$rc_probe
    SIDE_RC_DB[$side]=$rc_db
    SIDE_RC_STOP[$side]=$rc_stop

    [ "$rc_probe" -eq 0 ] || return 2
    return 0
}

declare -A SIDE_RC_PROBE SIDE_RC_DB SIDE_RC_STOP
SIDE_RC_PROBE[a]=2; SIDE_RC_PROBE[b]=2
SIDE_RC_DB[a]=0; SIDE_RC_DB[b]=0
SIDE_RC_STOP[a]=0; SIDE_RC_STOP[b]=0

overall=0
if ! run_side a "$STACK_A" "$STOP_A" "$DB_A"; then
    overall=2
    log "!! A 侧未跑完：不再起 B 侧（避免两栈抢同一组端口）"
fi
if [ "$overall" -eq 0 ]; then
    if ! run_side b "$STACK_B" "$STOP_B" "$DB_B"; then
        overall=2
    fi
fi

cmp_rc=2
if [ "$overall" -eq 0 ]; then
    log "== compare"
    python -I -B "$HERE/compare_runs.py" \
        --oracle "$OUT/a" --rust "$OUT/b" --out "$OUT/report.json" \
        --rules "$RULES" "${COMPARE_EXTRA[@]+"${COMPARE_EXTRA[@]}"}" \
        >>"$LOG_FILE" 2>&1
    cmp_rc=$?
    log "   compare rc=$cmp_rc"
else
    log "== compare 跳过（前序步骤失败 ⇒ 判不了，rc=2）"
fi

python -I -B - "$OUT/run_dual.json" "$OUT" "$SCENARIO" "$RULES" \
    "${SIDE_RC_PROBE[a]}" "${SIDE_RC_PROBE[b]}" \
    "${SIDE_RC_DB[a]}" "${SIDE_RC_DB[b]}" \
    "${SIDE_RC_STOP[a]}" "${SIDE_RC_STOP[b]}" \
    "$overall" "$cmp_rc" <<'PY'
import json
import sys
out, scenario, rules = sys.argv[2], sys.argv[3], sys.argv[4]
v = sys.argv[5:]
doc = {
    "format": "run-dual/1",
    "out": out,
    "scenario": scenario,
    "rules": rules,
    "sides": {
        "a": {"probe_rc": int(v[0]), "db_rc": int(v[2]), "stop_rc": int(v[4])},
        "b": {"probe_rc": int(v[1]), "db_rc": int(v[3]), "stop_rc": int(v[5])},
    },
    "flow_rc": int(v[6]),
    "compare_rc": int(v[7]),
    "exit_code": int(v[6]) if int(v[6]) != 0 else int(v[7]),
}
with open(sys.argv[1], "w", encoding="utf-8", newline="\n") as f:
    json.dump(doc, f, ensure_ascii=False, indent=2, sort_keys=True)
    f.write("\n")
PY

if [ "$overall" -ne 0 ]; then
    log "结论：流程未跑完 ⇒ rc=2（日志 $LOG_FILE）"
    exit 2
fi
if [ -f "$OUT/report.json" ]; then
    python -I -B - "$OUT/report.json" <<'PY'
import json
import sys
with open(sys.argv[1], "r", encoding="utf-8") as f:
    rep = json.load(f)
print("VERDICT: %s (exit %d)  报告：%s" % (rep.get("verdict"), rep.get("exit_code"), sys.argv[1]))
PY
fi
log "结论：compare rc=$cmp_rc（日志 $LOG_FILE）"
exit "$cmp_rc"
