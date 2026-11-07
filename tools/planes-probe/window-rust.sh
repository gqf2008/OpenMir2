#!/usr/bin/env bash
# G11 / v15 的 R 级真机观测（Rust 单栈）。见 docs/g11-planes-design-20261108.md §15。
#
# 用法：
#   tools/planes-probe/window-rust.sh [<mir2-server 可执行文件>]
#   （默认 mir2-rs/target/debug/mir2-server.exe；需先 `cargo build -p mir2-server`）
#
# 判据（对应 §15 的观测项 1-5）：
#   1 主服 ServerIndex=0：无「节点运行模式」/失败日志，MsgSrvPort 无监听；
#   2 分服 ServerNumber>0：启动即一条失败日志（≤3s），之后约每 11s 一条（≥3 个间隔，±1.5s）；
#   3 分服 ServerNumber=0：仅一条失败日志，之后 ≥30s 无重复；
#   4 非法 MsgSrvAddr：进程启动失败（非零退出 + 明确错误行）；
#   5 无文件副作用（临时目录里只应有 Server.conf 与 out.log）。
#
# 退出码：0 全过；1 有判据不符；2 用法/环境错。
set -u

REPO="$(cd "$(dirname "$0")/../.." && pwd)"
BIN="${1:-$REPO/mir2-rs/target/debug/mir2-server.exe}"
[ -x "$BIN" ] || { echo "找不到可执行文件：$BIN（先 cargo build -p mir2-server）" >&2; exit 2; }

WORK="$(mktemp -d "${TMPDIR:-/tmp}/g11-win.XXXXXX")" || exit 2
cleanup() { rm -rf "$WORK"; }
trap cleanup EXIT

run_case() { # name gate_port msg_port server_index server_number msg_addr seconds
  local name="$1" gp="$2" mp="$3" si="$4" sn="$5" addr="$6" secs="$7"
  local d="$WORK/$name"; mkdir -p "$d"
  cat > "$d/Server.conf" <<EOF
[Server]
GateAddr=127.0.0.1
GatePort=$gp
CheckBlock=12288
IDSAddr=127.0.0.1
IDSPort=5600
DBAddr=127.0.0.1
DBPort=6000
WorldMaps=1000x1000
ServerIndex=$si
ServerNumber=$sn
MsgSrvAddr=$addr
MsgSrvPort=$mp
EOF
  timeout "$secs" "$BIN" game-svr "$d/Server.conf" > "$d/out.log" 2>&1
  echo $?
}

# 主服用例：后台起进程 → 运行中探测 MsgSrvPort → 等它超时退出。
# 输出 "<rc> <refused|listening>"。连得上即判 listening（主服不该有监听）。
run_srv0_with_probe() { # gate_port msg_port seconds probe_delay
  local gp="$1" mp="$2" secs="$3" delay="$4"
  local d="$WORK/srv0"; mkdir -p "$d"
  cat > "$d/Server.conf" <<EOF
[Server]
GateAddr=127.0.0.1
GatePort=$gp
CheckBlock=12288
IDSAddr=127.0.0.1
IDSPort=5600
DBAddr=127.0.0.1
DBPort=6000
WorldMaps=1000x1000
ServerIndex=0
ServerNumber=0
MsgSrvAddr=127.0.0.1
MsgSrvPort=$mp
EOF
  timeout "$secs" "$BIN" game-svr "$d/Server.conf" > "$d/out.log" 2>&1 &
  local pid=$!
  sleep "$delay"
  local state
  if python -I -B -c "import socket,sys
s=socket.socket(); s.settimeout(1)
try:
    s.connect(('127.0.0.1', $mp)); sys.exit(1)
except OSError:
    sys.exit(0)"; then state=refused; else state=listening; fi
  wait "$pid"; local rc=$?
  echo "$rc $state"
}

# 端口：选高位、互不冲突、无人监听
P_SRV0=24995; P_SN1=24999; P_SN0=24998; P_BAD=24993
G_SRV0=24996; G_SN1=24997; G_SN0=24994; G_BAD=24992

# 主服用例要在**运行期间**探测 MsgSrvPort：主服若静默监听而不打日志，光看日志发现不了。
probe_srv0=$(run_srv0_with_probe "$G_SRV0" "$P_SRV0" 10 5)
rc_srv0="${probe_srv0%% *}"; srv0_port_state="${probe_srv0##* }"
rc_sn1=$(run_case sn1 "$G_SN1" "$P_SN1" 1 1 127.0.0.1 38)
rc_sn0=$(run_case sn0 "$G_SN0" "$P_SN0" 1 0 127.0.0.1 36)
rc_bad=$(run_case bad "$G_BAD" "$P_BAD" 1 0 not-an-ip 20)

python -I -B - "$WORK" "$rc_srv0" "$rc_sn1" "$rc_sn0" "$rc_bad" "$srv0_port_state" <<'PY'
import io, re, sys, os
work, rc_srv0, rc_sn1, rc_sn0, rc_bad, srv0_port_state = sys.argv[1:7]
fail = []

def lines(name):
    p = os.path.join(work, name, "out.log")
    return io.open(p, encoding="utf-8", errors="replace").read().splitlines()

def ts(l):
    m = re.match(r"^(\d\d):(\d\d):(\d\d)\.(\d\d\d)", l)
    if not m: return None
    h, mi, s, ms = map(int, m.groups())
    return ((h * 60 + mi) * 60 + s) + ms / 1000.0

def planes_events(name):
    out = []
    for l in lines(name):
        if "节点运行模式" in l or "链接主分区服务器" in l:
            out.append((ts(l), "node" if "节点运行模式" in l else "fail"))
    return out

# 1 主服：无跨服日志、无监听
ev = planes_events("srv0")
if ev: fail.append(f"1 主服 ServerIndex=0 出现了跨服日志：{ev[:3]}")
if rc_srv0 not in ("124",): fail.append(f"1 主服进程应持续运行（timeout 124），实得 rc={rc_srv0}")
if srv0_port_state != "refused":
    fail.append(f"1 主服 MsgSrvPort 必须无监听，实测 {srv0_port_state}")

# 2 分服 ServerNumber=1：首条 ≤3s 后，间隔 11±1.5s，≥3 个
ev = planes_events("sn1")
nodes = [t for t, k in ev if k == "node"]
fails = [t for t, k in ev if k == "fail"]
if len(nodes) != 1: fail.append(f"2 节点模式日志应恰一条，实得 {len(nodes)}")
if len(fails) < 4: fail.append(f"2 38s 内失败日志应 ≥4 条（首条+3 间隔），实得 {len(fails)}")
else:
    first = fails[0] - nodes[0]
    if first > 3.0: fail.append(f"2 首条失败离启动 {first:.2f}s > 3s")
    gaps = [b - a for a, b in zip(fails, fails[1:])]
    bad = [g for g in gaps if abs(g - 11.0) > 1.5]
    if bad: fail.append(f"2 周期间隔应 11±1.5s，越界：{['%.2f' % g for g in bad]}（全部 {['%.2f' % g for g in gaps]}）")

# 3 分服 ServerNumber=0：仅一条
ev = planes_events("sn0")
n_fail = sum(1 for _, k in ev if k == "fail")
if n_fail != 1: fail.append(f"3 ServerNumber=0 应仅 1 条失败日志，实得 {n_fail}")

# 4 非法端点：启动失败 + 明确错误
if rc_bad in ("0", "124"): fail.append(f"4 非法 MsgSrvAddr 应启动失败，实得 rc={rc_bad}")
if not any("MsgSrvAddr 不是合法 IP 地址" in l for l in lines("bad")):
    fail.append("4 缺明确的 MsgSrvAddr 错误行")

# 5 无文件副作用
for name in ("srv0", "sn1", "sn0", "bad"):
    d = os.path.join(work, name)
    extra = sorted(set(os.listdir(d)) - {"Server.conf", "out.log"})
    if extra: fail.append(f"5 {name} 目录出现副作用文件：{extra}")

if fail:
    print("WINDOW-RED")
    for f in fail: print(" -", f)
    sys.exit(1)
print("WINDOW-GREEN：观测项 1-5 全部符合")
PY
rc=$?

echo "（消息端口 $P_SRV0 / 24999 / 24998：主服端口在运行期间由本脚本主动探测，其余无人监听）"
exit "$rc"
