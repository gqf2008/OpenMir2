#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""load_report.py — 假人压测的指标归算（C6 载荷门禁）。

**口径（与 M2 的 p99-baseline 对齐的就是这一份，两侧共用）**

tick 服务时延（`tick_ms_*`）
  定义：客户端收到明文动作帧 `#+GD/<rtime>!` 的时刻 − 帧里带的 `rtime`，
  两者都在 `Environment.TickCount`（**开机毫秒、全机同域**）上取值。
  语义 =「服务端取时间戳 → 客户端收完这一帧」的耗时（本机环回，无网络时延），
  直接反映世界线程/发送队列被拖住的程度。
  样本来源：假人进程内的直方图（`load_stats.ndjson`，1ms 桶，0..200ms + 溢出桶，累计值）。
  **稳态窗** = 从「登录数首次达到目标」那一行到最后一行的差分（上线爬坡期单列，不混进 P99）。
  P50/P90/P99 = 直方图上"累计占比首次 ≥ q"的那个桶（1ms 分辨率）。
  量化下限：`Environment.TickCount` 在 Windows 上粒度约 15.6ms ⇒ 单样本 ±16ms，报告里显式记 `clock_granularity_note`。

登录成功率（`login_success_pct`）
  = 稳态窗内 `login_ok` 峰值 ÷ 目标假人数（来源：BotSrv 内部计数 `LoadMetrics.LoginOk()`，
  不是抓日志行——日志行只在"无期限提示"分支打印，会漏）。

掉线（`conn_lost`）
  = 登录之后 socket 被关闭/超时（`ConnectionReset`/`TimedOut`/其它）的累计次数；
  与"连不上"（`conn_refused`）分开计。爬坡期的掉线单列（`conn_lost_ramp`），稳态期的算 `conn_lost_steady`。

内存曲线（`mem`）
  每 `--mem-sample-sec` 采样一次各进程的 WS/私有字节/CPU；给峰值、均值、以及**最后 60s 的斜率**（MB/s），
  斜率用来判 M5 的"最后一段斜率 ≈ 0"（真泄漏 vs 分配器高水位）。

用法：
  python tools/botload/load_report.py summarize --stats <load_stats.ndjson> --mem <mem.ndjson> \
      --count 200 --stagger-ms 20 --hold-sec 60 --out <report.json>
  python tools/botload/load_report.py selftest
"""
import argparse
import json
import os
import sys

DEFAULT_THRESHOLDS = {
    "login_success_pct": 100.0,     # §6 M5：登录成功率 100%
    "conn_lost_steady": 0,          # §6 M5：无掉线
    "tick_p99_ms": 100.0,           # §6 M5：tick P99 ≤ 100ms
}

# C6 第 5 轮（owner 裁决 11dbde39 / 17aa4753）：tier-1000 的登记口径是「同命令 3 连跑取最差 ≥ 阈值」。
# tier-1000 的登录在**上游 C# 部署栈**上有 ~0-5% 的残余丢包（48~52/1000 条连接发出 CM_IDPASSWORD 后
# 服务端一个字节都没回，见 tests/loadgate/README.md 的方向证明），这不是 Rust 移植缺陷；
# 把「100%」改成可复现的下界 95%（详见 tools/botload/README.md《阈值与理由》）。其他档位仍是 100%。
LOGIN_THRESHOLD_BY_MIN_TIER = [
    (1000, 95.0),   # tier >= 1000 用 95%
]


def default_login_threshold(count):
    """按档位给默认登录阈值（可被 --login-threshold 覆盖）。"""
    for min_tier, pct in sorted(LOGIN_THRESHOLD_BY_MIN_TIER, reverse=True):
        if count is not None and count >= min_tier:
            return pct
    return DEFAULT_THRESHOLDS["login_success_pct"]


def load_ndjson(path):
    rows = []
    with open(path, "r", encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if line:
                rows.append(json.loads(line))
    return rows


def hist_sub(a, b):
    ha, hb = a.get("hist", []), b.get("hist", [])
    n = max(len(ha), len(hb))
    return [(hb[i] if i < len(hb) else 0) - (ha[i] if i < len(ha) else 0) for i in range(n)]


def quantile_from_hist(hist, q, bucket_ms=1):
    """直方图上的分位数：最小的桶 i 使得累计占比 ≥ q（1ms 分辨率）。返回 (ms, 样本数)。"""
    total = sum(hist)
    if total <= 0:
        return None, 0
    need = q * total
    cum = 0
    for i, c in enumerate(hist):
        cum += c
        if cum >= need:
            return i * bucket_ms, total
    return (len(hist) - 1) * bucket_ms, total


def summarize_tick(stats, count):
    """稳态窗（登录数达到目标之后）的 tick 分布；爬坡期单列。"""
    if not stats:
        return {"error": "load_stats 为空"}
    peak_login = max(r.get("login_ok", 0) for r in stats)
    ramp_idx = None
    for i, r in enumerate(stats):
        if r.get("login_ok", 0) >= count:
            ramp_idx = i
            break
    last = stats[-1]
    base = stats[ramp_idx] if ramp_idx is not None else stats[0]
    hist = hist_sub(base, last)
    out = {
        "samples": sum(hist),
        "samples_total": last.get("tick_samples", 0),
        "max_ms": last.get("tick_max_ms"),
        "mean_ms": (round(last.get("tick_sum_ms", 0) / last["tick_samples"], 2)
                    if last.get("tick_samples") else None),
        "ramp_complete": ramp_idx is not None,
        "steady_window_ms": (last.get("t_ms", 0) - base.get("t_ms", 0)),
        "peak_login_ok": peak_login,
        "login_success_pct": round(100.0 * peak_login / count, 2) if count else None,
        "conn_refused": last.get("conn_refused", 0),
        "conn_lost": last.get("conn_lost", 0),
        "conn_lost_ramp": base.get("conn_lost", 0),
        "conn_lost_steady": last.get("conn_lost", 0) - base.get("conn_lost", 0),
        "hist_bucket_ms": last.get("hist_bucket_ms", 1),
        # tick 样本的**覆盖基数**：进世界（收到 SM_LOGON）的假人数与探针实际发出的动作数。
        # tick 只可能来自进世界的假人 ⇒ 这两个数一起读才知道"样本是从多少人身上量的"。
        "world_ok": last.get("in_world", 0),
        "world_ok_pct": (round(100.0 * last.get("in_world", 0) / count, 2) if count else None),
        "probe_sent": last.get("probe_sent", 0),
        "internal_errors": last.get("internal_errors", 0),
        # T-11 登录连接级观测（残余丢包方向定位）：发出登录包后没拿到 529 的连接数，
        # 以及其中 rx_bytes==0（服务端一个字节都没回）的数量；resend 是诊断对照（默认关）。
        "login_conn_total": last.get("login_conn_total", 0),
        "login_conn_no_resp": last.get("login_conn_no_resp", 0),
        "login_conn_no_resp_rx0": last.get("login_conn_no_resp_rx0", 0),
        "login_conn_resend": last.get("login_conn_resend", 0),
        "login_conn_resend_ok": last.get("login_conn_resend_ok", 0),
        "login_send_failed": last.get("login_send_failed", 0),
    }
    for q, key in ((0.50, "tick_p50_ms"), (0.90, "tick_p90_ms"), (0.99, "tick_p99_ms")):
        v, _ = quantile_from_hist(hist, q, out["hist_bucket_ms"])
        out[key] = v
    # 爬坡时间 = 首次达到目标登录数的那一行的时间戳（相对首行）
    out["ramp_ms"] = (base.get("t_ms", 0) - stats[0].get("t_ms", 0)) if ramp_idx is not None else None
    out["clock_granularity_note"] = ("Environment.TickCount 在 Windows 上粒度约 15.6ms"
                                     "（单样本 ±16ms 量化，是测量下限，不是噪声消除）")
    return out


def summarize_mem(rows, tail_sec=60):
    """按进程归算内存曲线：峰值/均值/最后 tail_sec 的斜率（MB/s）。"""
    by_proc = {}
    for r in rows:
        by_proc.setdefault(r["proc"], []).append(r)
    out = {}
    for proc, rs in sorted(by_proc.items()):
        rs = [r for r in rs if r.get("ws_mb") is not None]
        if not rs:
            continue
        ws = [r["ws_mb"] for r in rs]
        t0 = rs[0].get("t_ms", 0)
        last = rs[-1]
        tail = [r for r in rs if r.get("t_ms", 0) >= last.get("t_ms", 0) - tail_sec * 1000]
        if len(tail) >= 2:
            dt = (tail[-1]["t_ms"] - tail[0]["t_ms"]) / 1000.0
            slope = (tail[-1]["ws_mb"] - tail[0]["ws_mb"]) / dt if dt > 0 else 0.0
        else:
            slope = None
        out[proc] = {
            "samples": len(rs),
            "ws_peak_mb": round(max(ws), 1),
            "ws_mean_mb": round(sum(ws) / len(ws), 1),
            "ws_last_mb": round(ws[-1], 1),
            "priv_peak_mb": round(max((r.get("priv_mb") or 0) for r in rs), 1),
            "slope_tail_mb_per_s": (round(slope, 3) if slope is not None else None),
            "tail_window_s": round((last.get("t_ms", 0) - tail[0].get("t_ms", 0)) / 1000.0, 1) if tail else 0,
            "window_s": round((last.get("t_ms", 0) - t0) / 1000.0, 1),
        }
    return out


def summarize_login_conns(rows, bucket_ms=5000):
    """T-11 残余丢包**方向**归算（owner 裁决 11dbde39 的 must_first）。

    输入 = BotSrv 落的 `login_conns.ndjson`（每个假人的登录 socket 一行，见 `LoadMetrics.LoginConn`）。
    只回答一个问题：发出 CM_IDPASSWORD 后没拿到 SM_PASSOK_SELECTSERVER（ident 529）的连接，
    是「服务端一个字节都没回」还是「服务端回了、假人侧收包/解析丢了」：

      · `rx_bytes == 0`（rx0）⇒ 服务端在这条 TCP 连接上没写过任何东西 ⇒ **方向 = 上游（C# 部署栈）**；
      · `rx_bytes > 0` 且没解出 529 ⇒ 服务端回了、假人侧丢了 ⇒ **方向 = 假人（BotSrv，工具缺陷）**。

    `resend` / `resend_ok` 是自证对照：对「窗口内无应答」的连接在同一条 TCP 连接上重发一次登录包
    （`MIR2_BOT_LOGIN_RESEND_MS`），第二次仍拿不到应答 ⇒ 不是本机收包抖动。

    返回：总数 / 无应答数 / 其中 rx0（上游）/ rx>0（假人侧）/ 按发出时刻分窗的分布 / 重发对照。
    """
    def gi(r, k, d=0):
        v = r.get(k, d)
        return d if v is None else v

    total = len(rows)
    no_resp = [r for r in rows if not r.get("saw_529")]
    rx0 = [r for r in no_resp if gi(r, "rx_bytes") == 0]
    rx_gt0 = [r for r in no_resp if gi(r, "rx_bytes") > 0]
    resend = [r for r in no_resp if gi(r, "resend_sends") > 0]
    resend_ok = [r for r in resend if r.get("saw_529")]

    buckets = {}
    for r in no_resp:
        b = (gi(r, "login_tick") // bucket_ms) * bucket_ms
        e = buckets.setdefault(b, {"t0": b, "count": 0, "rx0": 0, "rx_gt0": 0})
        e["count"] += 1
        if gi(r, "rx_bytes") == 0:
            e["rx0"] += 1
        else:
            e["rx_gt0"] += 1

    if not no_resp:
        direction = "none"
    elif not rx_gt0:
        direction = "server-no-response"   # 全部 rx_bytes==0：服务端没回过任何字节
    elif not rx0:
        direction = "bot-client"              # 全部 rx_bytes>0：假人侧收包/解析丢了
    else:
        direction = "mixed"

    ticks = sorted(gi(r, "login_tick") for r in no_resp)
    return {
        "total": total,
        "no_resp": len(no_resp),
        "no_resp_rx0": len(rx0),
        "no_resp_rx_gt0": len(rx_gt0),
        "direction": direction,
        "direction_rule": ("rx_bytes==0 ⇒ 服务端在这条连接上没回过任何字节（方向=服务端未回；"
                           "触发条件——C# 栈上限 vs 同机负载——未归属，待空载复测）；"
                           "rx_bytes>0 且无 529 ⇒ 假人侧收包/解析丢失"),
        "no_resp_first_tick": (ticks[0] if ticks else None),
        "no_resp_last_tick": (ticks[-1] if ticks else None),
        "no_resp_span_ms": ((ticks[-1] - ticks[0]) if ticks else 0),
        "no_resp_resend": len(resend),
        "no_resp_resend_ok": len(resend_ok),
        "bucket_ms": bucket_ms,
        "buckets": [buckets[k] for k in sorted(buckets)],
    }


def summarize_hostload(rows):
    """本机负载探针归算（owner 裁决 `82f8e0c3` ④；口径照 tickbench `probe_load_window`）。

    只**标注**不判红：tick P99 在共享机器上不可归属（C# 参考栈 vs 同机并发编译），
    必须空载窗口才能定；本块给报告读者一个「这轮 P99 是在什么负载下量的」的可审阅字段。
    """
    cpu = [r.get("cpu_pct") for r in rows
           if isinstance(r.get("cpu_pct"), (int, float)) and r.get("cpu_pct") >= 0]
    heavy = [r.get("heavy_procs", 0) or 0 for r in rows]
    names = set()
    for r in rows:
        for nm in (r.get("heavy_names") or "").split(","):
            nm = nm.strip()
            if nm:
                names.add(nm)
    out = {
        "samples": len(rows),
        "cpu_mean_pct": (round(sum(cpu) / len(cpu), 1) if cpu else None),
        "cpu_peak_pct": (max(cpu) if cpu else None),
        "heavy_procs_max": (max(heavy) if heavy else 0),
        "heavy_names_seen": sorted(names),
        "caliber": ("CPU = Win32_Processor.LoadPercentage 均值(%)；heavy = "
                    "rustc/cargo/cargo-clippy/msbuild 进程数（照 tickbench probe_load_window）"),
    }
    out["loaded_window"] = bool((out["cpu_peak_pct"] is not None and out["cpu_peak_pct"] >= 40)
                                or out["heavy_procs_max"] >= 4)
    return out


def judge(tier, thresholds, criteria=("login", "conn", "tick"), allow_missing_tick=False,
          login_threshold=None):
    """按 §6 M5 的判据给单档结论（掉线只看稳态窗）。

    `criteria` 只选"要比哪几条阈值"，**不能用它把"缺 tick 样本"变成绿灯**（2026-10-10 的教训：
    同一份 0 样本产物在把判据做成可选之后从 RED 翻成 GREEN ⇒ 恒绿假门禁）。

    所以"缺 tick 样本"由**独立开关** `allow_missing_tick` 管辖，且默认判 RED：
      · 没有 tick 样本 + 没开关 ⇒ RED（不管 criteria 写了什么）；
      · 没有 tick 样本 + 有开关 ⇒ 出基线，但报告里带 `tick_missing`/`tick_missing_acknowledged`/
        `missing_tick_note`（"本次未采集 tick P99"）⇒ 下游一眼能看出这档不能当 M5 的 tick 基线引用。
    确实天然拿不到 tick 的场景（例如 M1 ⑤ 只比登录+选角）也必须显式加开关，不许"顺手改成可选"。
    """
    fails = []
    # 登录阈值：按档位的可复现下界（默认 tier>=1000 为 95%，其余 100%），可被 CLI 覆盖。
    # 「爬坡未完成」不再是独立判据：它等于「峰值登录 < 档位人数」，已被这条阈值判据覆盖
    # （tier-1000 的已知上游上限会让峰值停在 ~948/1000，用 100% 当判据就永远是 RED）。
    login_thr = login_threshold if login_threshold is not None else thresholds["login_success_pct"]
    tier["login_threshold_pct"] = login_thr
    if "login" in criteria and (tier.get("login_success_pct") is None
                                or tier["login_success_pct"] < login_thr):
        fails.append(f"登录成功率 {tier.get('login_success_pct')}% < {login_thr}%")
    if "conn" in criteria and tier.get("conn_lost_steady", 0) > thresholds["conn_lost_steady"]:
        fails.append(f"稳态掉线 {tier['conn_lost_steady']} > {thresholds['conn_lost_steady']}")
    tick_missing = tier.get("tick_p99_ms") is None
    if tick_missing:
        tier["tick_missing"] = True
        if allow_missing_tick:
            tier["tick_missing_acknowledged"] = True
            tier["missing_tick_note"] = "本次未采集 tick P99（--allow-missing-tick 显式放行；不得当作 M5 的 tick 基线）"
        else:
            fails.append("缺 tick 样本（tick_p99_ms = None）：默认判 RED；确实拿不到 tick 的场景也必须显式加 --allow-missing-tick")
    elif "tick" in criteria and tier["tick_p99_ms"] > thresholds["tick_p99_ms"]:
        fails.append(f"tick P99 {tier['tick_p99_ms']}ms > {thresholds['tick_p99_ms']}ms")
    tier["ramp_target"] = int(-(-tier.get("count", 0) * login_thr // 100))
    tier["criteria"] = list(criteria)
    tier["allow_missing_tick"] = bool(allow_missing_tick)
    tier["verdict"] = "GREEN" if not fails else "RED"
    tier["fails"] = fails
    return tier


def cmd_summarize(args):
    stats = load_ndjson(args.stats) if args.stats else []
    tier = summarize_tick(stats, args.count)
    if args.mem:
        tier["mem"] = summarize_mem(load_ndjson(args.mem))
    if args.login_conns and os.path.exists(args.login_conns):
        tier["login_conn_diag"] = summarize_login_conns(load_ndjson(args.login_conns))
    if args.hostload and os.path.exists(args.hostload):
        tier["host_load"] = summarize_hostload(load_ndjson(args.hostload))
    tier["count"] = args.count
    tier["stagger_ms"] = args.stagger_ms
    tier["hold_sec"] = args.hold_sec
    criteria = [c.strip() for c in args.criteria.split(",") if c.strip()]
    login_thr = args.login_threshold if args.login_threshold is not None else default_login_threshold(args.count)
    judge(tier, DEFAULT_THRESHOLDS, criteria, allow_missing_tick=args.allow_missing_tick,
          login_threshold=login_thr)
    text = json.dumps(tier, ensure_ascii=False, indent=2)
    if args.out:
        with open(args.out, "w", encoding="utf-8", newline="\n") as f:
            f.write(text + "\n")
    print(text)
    return 0 if tier["verdict"] == "GREEN" else 1


def cmd_selftest(_args):
    ok = True

    def expect(cond, msg):
        nonlocal ok
        if not cond:
            ok = False
            print("RED: " + msg)

    # 1) 分位数：980 个 5ms + 20 个 150ms ⇒ P50=5、P99=150；尾部敏感（挪走尾巴 ⇒ P99=5）
    hist = [0] * 201
    hist[5] = 980
    hist[150] = 20
    stats = [
        {"t_ms": 1000, "login_ok": 100, "tick_samples": 0, "tick_sum_ms": 0, "tick_max_ms": 0,
         "conn_lost": 0, "hist": [0] * 201},
        {"t_ms": 2000, "login_ok": 100, "tick_samples": 1000, "tick_sum_ms": 5 * 980 + 150 * 20,
         "tick_max_ms": 150, "conn_lost": 0, "hist": list(hist)},
    ]
    t = summarize_tick(stats, 100)
    expect(t["tick_p50_ms"] == 5, f"P50 应为 5，实得 {t['tick_p50_ms']}")
    expect(t["tick_p99_ms"] == 150, f"P99 应为 150，实得 {t['tick_p99_ms']}")
    expect(t["samples"] == 1000, f"稳态样本数应为 1000，实得 {t['samples']}")
    expect(t["ramp_complete"] is True, "登录达标应判 ramp_complete")
    hist2 = list(hist)
    hist2[150] = 0
    hist2[5] = 1000
    stats2 = [stats[0], dict(stats[1], hist=hist2, tick_max_ms=5)]
    t2 = summarize_tick(stats2, 100)
    expect(t2["tick_p99_ms"] == 5, "挪走尾巴后 P99 应降到 5（尾部敏感）")
    expect(t["tick_p99_ms"] != t2["tick_p99_ms"], "改坏输入必须让 P99 变（不是恒绿）")

    # 2) 登录没到阈值要判红（2026-10-11 第 5 轮：判据从「爬坡必须到 100%」改成「登录率必须 ≥ 阈值」）
    stats3 = [dict(stats[0], login_ok=10), dict(stats[1], login_ok=10)]
    t3 = judge(summarize_tick(stats3, 100), DEFAULT_THRESHOLDS)
    expect(t3["verdict"] == "RED" and any("登录成功率" in f for f in t3["fails"]),
           "登录率 < 阈值必须判红")
    # 2b) 阈值牙齿：同一份 960/1000 产物，阈值 100 ⇒ RED；阈值 95 ⇒ GREEN（判定必须随阈值变）
    stats960 = [dict(stats[0], login_ok=960), dict(stats[1], login_ok=960, hist=list(hist2), tick_max_ms=5)]
    t_hi = judge(summarize_tick(stats960, 1000), DEFAULT_THRESHOLDS, login_threshold=100.0)
    t_lo = judge(summarize_tick(stats960, 1000), DEFAULT_THRESHOLDS, login_threshold=95.0)
    expect(t_hi["verdict"] == "RED", "960/1000 在阈值 100 下必须 RED（阈值有牙齿）")
    expect(t_lo["verdict"] == "GREEN", "960/1000 在阈值 95 下必须 GREEN（判定随阈值变，不是恒绿）")
    expect(t_hi["login_threshold_pct"] == 100.0 and t_lo["login_threshold_pct"] == 95.0,
           "judge 必须把生效阈值写进报告")
    # 2c) T-11 残余丢包**方向**判据（owner 裁决 11dbde39 must_first）：rx_bytes==0 ⇒ 上游；
    #     rx_bytes>0 且无 529 ⇒ 假人侧。方向判据本身必须有牙齿（改坏必变）。
    up = [{"login_tick": 1000, "rx_bytes": 0, "saw_529": False, "resend_sends": 1},
          {"login_tick": 2000, "rx_bytes": 0, "saw_529": False, "resend_sends": 1},
          {"login_tick": 3000, "rx_bytes": 40, "saw_529": True, "resend_sends": 0}]
    d_up = summarize_login_conns(up)
    expect(d_up["direction"] == "server-no-response" and d_up["no_resp_rx0"] == 2,
           "全 rx_bytes==0 应判「上游（C# 栈）」")
    expect(d_up["no_resp_resend"] == 2 and d_up["no_resp_resend_ok"] == 0,
           "重发对照应记「发 2 / 成功 0」")
    expect(d_up["buckets"][0]["t0"] == 0 and d_up["buckets"][0]["rx0"] == 2, "应按 login_tick 分窗（默认 5s）")
    bot = [{"login_tick": 1000, "rx_bytes": 128, "saw_529": False},
           {"login_tick": 1500, "rx_bytes": 24, "saw_529": True}]
    d_bot = summarize_login_conns(bot)
    expect(d_bot["direction"] == "bot-client" and d_bot["no_resp_rx_gt0"] == 1,
           "rx_bytes>0 且无 529 应判「假人侧」（方向判据的牙齿）")
    expect(summarize_login_conns(up[:1] + bot[:1])["direction"] == "mixed", "两种都存在应判 mixed")
    expect(summarize_login_conns([{"login_tick": 1, "rx_bytes": 8, "saw_529": True}])["direction"] == "none",
           "无「无应答」连接应判 none")
    # 2d) 本机负载探针（owner 裁决 82f8e0c3 ④）：CPU≥40 或 heavy≥4 ⇒ loaded_window
    hl = summarize_hostload([{"cpu_pct": 10, "heavy_procs": 0},
                             {"cpu_pct": 55, "heavy_procs": 2, "heavy_names": "cargo"}])
    expect(hl["cpu_peak_pct"] == 55 and hl["cpu_mean_pct"] == 32.5 and hl["loaded_window"] is True,
           "负载探针：CPU 峰值≥40 应判 loaded_window")
    hl2 = summarize_hostload([{"cpu_pct": 5, "heavy_procs": 5, "heavy_names": "rustc,cargo"}])
    expect(hl2["loaded_window"] is True and hl2["heavy_names_seen"] == ["cargo", "rustc"],
           "负载探针：heavy≥4 应判 loaded_window，且应收取进程名")
    expect(summarize_hostload([{"cpu_pct": 5, "heavy_procs": 0}])["loaded_window"] is False,
           "低负载窗口不得判 loaded_window")

    # 3) 掉线：稳态掉线判红、爬坡期掉线不算稳态
    stats4 = [dict(stats[0], conn_lost=3), dict(stats[1], conn_lost=5)]
    t4 = judge(summarize_tick(stats4, 100), DEFAULT_THRESHOLDS)
    expect(t4["conn_lost_ramp"] == 3 and t4["conn_lost_steady"] == 2, "爬坡/稳态掉线应分开")
    expect(t4["verdict"] == "RED", "稳态掉线 > 0 必须判红")

    # 4) 内存斜率：线性上升 100s 涨 100MB ⇒ ≈1MB/s；平的 ⇒ ≈0
    mem_up = [{"t_ms": i * 1000, "proc": "GameSvr", "pid": 1, "ws_mb": 100 + i, "priv_mb": 120 + i}
              for i in range(101)]
    mem_flat = [{"t_ms": i * 1000, "proc": "GameSvr", "pid": 1, "ws_mb": 100, "priv_mb": 120}
                for i in range(101)]
    s_up = summarize_mem(mem_up)["GameSvr"]
    s_flat = summarize_mem(mem_flat)["GameSvr"]
    expect(abs(s_up["slope_tail_mb_per_s"] - 1.0) < 0.05,
           f"上升曲线斜率应 ≈1MB/s，实得 {s_up['slope_tail_mb_per_s']}")
    expect(abs(s_flat["slope_tail_mb_per_s"]) < 0.01,
           f"平坦曲线斜率应 ≈0，实得 {s_flat['slope_tail_mb_per_s']}")
    expect(s_up["ws_peak_mb"] == 200.0 and s_flat["ws_peak_mb"] == 100.0, "峰值归算错")

    # 5) 「缺 tick 样本」默认判 RED，且 --criteria 不能把它变成绿灯（2026-10-10 的恒绿假门禁回归项）
    stats5 = [{"t_ms": 1000, "login_ok": 100, "tick_samples": 0, "tick_sum_ms": 0, "tick_max_ms": 0,
               "conn_lost": 0, "hist": [0] * 201},
              {"t_ms": 2000, "login_ok": 100, "tick_samples": 0, "tick_sum_ms": 0, "tick_max_ms": 0,
               "conn_lost": 0, "hist": [0] * 201}]
    t5 = judge(summarize_tick(stats5, 100), DEFAULT_THRESHOLDS, ("login", "conn"))
    expect(t5["verdict"] == "RED", "0 样本 + criteria=login,conn 必须仍判 RED（不许被可选判据绕开）")
    expect(any("缺 tick 样本" in f for f in t5["fails"]), "0 样本应给出「缺 tick 样本」这条 fails")
    expect(t5.get("tick_missing") is True, "0 样本应标注 tick_missing")
    expect("missing_tick_note" not in t5, "没放行时不该写 missing_tick_note")
    t6 = judge(summarize_tick(stats5, 100), DEFAULT_THRESHOLDS, ("login", "conn"), allow_missing_tick=True)
    expect(not any("缺 tick 样本" in f for f in t6["fails"]), "显式 --allow-missing-tick 后不应再因缺样本判红")
    expect(t6.get("tick_missing_acknowledged") is True and "本次未采集 tick P99" in t6.get("missing_tick_note", ""),
           "放行后基线文件必须带「本次未采集 tick P99」标注")
    # 有样本时开关无副作用：仍只按阈值判（P99=150 > 100 ⇒ RED，且不得出现 tick_missing 标注）
    t7 = judge(summarize_tick(stats, 100), DEFAULT_THRESHOLDS, ("login", "conn", "tick"), allow_missing_tick=True)
    expect(t7["tick_p99_ms"] == 150 and t7["verdict"] == "RED" and any("tick P99 150" in f for f in t7["fails"]),
           "有样本时按阈值判（150ms > 100ms ⇒ RED），且开关不得改变这条判定")
    expect("tick_missing" not in t7, "有样本时不得出现 tick_missing 标注")

    if not ok:
        print("RED: load_report selftest 有未通过项")
        return 1
    print("GREEN: load_report selftest 全过（分位数/尾部敏感/登录阈值牙齿/掉线分窗/内存斜率/残余丢包方向/本机负载窗）")
    return 0


def svg_curve(series, width=560, height=180, title="", y_label="", x_label=""):
    """零依赖 SVG 折线图（三档曲线要能入库，不引 matplotlib）。
    series: [(label, [(x, y), ...]), ...]"""
    if not series:
        return ""
    xs = [p[0] for _, pts in series for p in pts]
    ys = [p[1] for _, pts in series for p in pts]
    xmin, xmax = min(xs), max(xs)
    ymin, ymax = 0.0, max(ys) if max(ys) > 0 else 1.0
    pad_l, pad_r, pad_t, pad_b = 46, 12, 22, 28
    w, h = width - pad_l - pad_r, height - pad_t - pad_b

    def px(x):
        return pad_l + (0 if xmax == xmin else (x - xmin) / (xmax - xmin) * w)

    def py(y):
        return pad_t + h - (y - ymin) / (ymax - ymin) * h

    colors = ["#2563eb", "#dc2626", "#16a34a", "#9333ea", "#ea580c"]
    out = [f'<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" '
           f'viewBox="0 0 {width} {height}" font-family="sans-serif" font-size="11">',
           f'<rect width="{width}" height="{height}" fill="#fff"/>',
           f'<text x="{pad_l}" y="14" font-size="12" font-weight="bold">{title}</text>',
           f'<line x1="{pad_l}" y1="{pad_t + h}" x2="{pad_l + w}" y2="{pad_t + h}" stroke="#94a3b8"/>',
           f'<line x1="{pad_l}" y1="{pad_t}" x2="{pad_l}" y2="{pad_t + h}" stroke="#94a3b8"/>',
           f'<text x="4" y="{pad_t + 8}">{ymax:.0f}</text>',
           f'<text x="4" y="{pad_t + h}">0</text>',
           f'<text x="{pad_l + w - 60}" y="{height - 6}">{x_label}</text>',
           f'<text x="4" y="{pad_t - 8}">{y_label}</text>']
    for i, (label, pts) in enumerate(series):
        c = colors[i % len(colors)]
        d = " ".join(("M" if j == 0 else "L") + f"{px(x):.1f},{py(y):.1f}" for j, (x, y) in enumerate(pts))
        out.append(f'<path d="{d}" fill="none" stroke="{c}" stroke-width="1.8"/>')
        for x, y in pts:
            out.append(f'<circle cx="{px(x):.1f}" cy="{py(y):.1f}" r="2.4" fill="{c}"/>')
        out.append(f'<text x="{pad_l + 6 + i * 92}" y="{pad_t + 12}" fill="{c}">{label}</text>')
    out.append("</svg>")
    return "\n".join(out)


def cmd_aggregate(args):
    """三档汇总 → SUMMARY.md（表 + 曲线）。输入 = load_gate_summary.json。"""
    # PowerShell 5.1 的 Set-Content -Encoding utf8 会带 BOM ⇒ 用 utf-8-sig 读（两种都能吃）
    data = json.load(open(args.summary, encoding="utf-8-sig"))
    reports = data.get("reports", [])
    outdir = args.out_dir or os.path.dirname(os.path.abspath(args.summary))
    lines = ["# 压测报告（C6 载荷门禁）", ""]
    lines.append(f"- 口径：{data.get('caliber', '见 tools/botload/load_report.py')}")
    lines.append(f"- 档位：{data.get('tiers')}；连接错峰 {data.get('stagger_ms')}ms/个；稳态时长 {data.get('hold_sec')}s")
    lines.append(f"- 判据（§6 M5）：登录成功率 = 档位阈值（tier≥1000 为 95%，其余 100%，见 report.json 的 "
                 f"login_threshold_pct）/ 稳态掉线 0 / tick P99 ≤ 100ms")
    lines.append("")
    has_run = any(t.get("run") for t in reports)

    def _hl(t, key):
        hl = t.get("host_load") or {}
        v = hl.get(key)
        return "-" if v is None else v

    if has_run:
        lines.append("| 轮次 | 档位 | 登录成功率 | 阈值 | 稳态掉线 | in_world | tick P50 | P90 | P99 | max | 稳态样本(上线) | CPU峰% | 重进程峰 | 结论 |")
        lines.append("| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |")
    else:
        lines.append("| 档位 | 登录成功率 | 阈值 | 稳态掉线 | in_world | tick P50 | P90 | P99 | max | 稳态样本(上线) | CPU峰% | 重进程峰 | 结论 |")
        lines.append("| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |")
    for t in sorted(reports, key=lambda r: (r.get("run", 0), r.get("count", 0))):
        ramp = ((str(round(t["ramp_ms"] / 1000.0, 1)) + "s") if t.get("ramp_ms") is not None else "-")
        if has_run:
            lines.append(
                "| r{run} | {count} | {lsp}% | {thr}% | {cls} | {iw} | {p50} | {p90} | {p99} | {mx} | "
                "{n} ({ramp}) | {cpu} | {hv} | {v} |".format(
                    run=t.get("run"), count=t.get("count"), lsp=t.get("login_success_pct"),
                    thr=t.get("login_threshold_pct"), cls=t.get("conn_lost_steady"),
                    iw=t.get("world_ok"), p50=t.get("tick_p50_ms"), p90=t.get("tick_p90_ms"),
                    p99=t.get("tick_p99_ms"), mx=t.get("max_ms"), n=t.get("samples"), ramp=ramp,
                    cpu=_hl(t, "cpu_peak_pct"), hv=_hl(t, "heavy_procs_max"), v=t.get("verdict")))
        else:
            lines.append(
                "| {count} | {lsp}% | {thr}% | {cls} | {iw} | {p50} | {p90} | {p99} | {mx} | "
                "{n} ({ramp}) | {cpu} | {hv} | {v} |".format(
                    count=t.get("count"), lsp=t.get("login_success_pct"), thr=t.get("login_threshold_pct"),
                    cls=t.get("conn_lost_steady"), iw=t.get("world_ok"),
                    p50=t.get("tick_p50_ms"), p90=t.get("tick_p90_ms"), p99=t.get("tick_p99_ms"),
                    mx=t.get("max_ms"), n=t.get("samples"), ramp=ramp,
                    cpu=_hl(t, "cpu_peak_pct"), hv=_hl(t, "heavy_procs_max"), v=t.get("verdict")))
    # 3 连跑取最差：把 worst_by_tier 也写进 SUMMARY（若 gate 带 -Repeat > 1）
    wbt = data.get("worst_by_tier") or {}
    if wbt:
        lines.append("")
        lines.append("## 同命令多连跑取最差（owner 裁决 11dbde39 ①）")
        lines.append("")
        lines.append("| 档位 | 轮数 | 阈值 | 最差登录率 | 结论 |")
        lines.append("| --- | --- | --- | --- | --- |")
        for k in sorted(wbt.keys(), key=lambda x: int(x)):
            e = wbt[k]
            n_runs = len(e.get("runs", []))
            lines.append(f"| {k} | {n_runs} | {e.get('threshold')}% | {e.get('worst_login_pct')}% | {e.get('verdict')} |")
    lines.append("")
    lines.append("## tick P99 离散度与本机负载（owner 裁决 82f8e0c3 ⑤）")
    lines.append("")
    lines.append("tick 口径 = 服务端 `rtime` → 客户端收帧耗时（含 BotSrv 收包/调度）；P99 只在稳态窗、")
    lines.append("样本基数 = `in_world`（真进世界的假人数，见下）。**本机负载探针**（CPU=Win32_Processor.LoadPercentage ")
    lines.append("均值；heavy=rustc/cargo/cargo-clippy/msbuild 进程数，叧 tickbench）只用于**归属**：")
    lines.append("CPU 峰 ≥ 40% 或 heavy 峰 ≥ 4 时 P99 **不可归属**（C# 参考栈 vs 同机并发编译）。")
    lines.append("")
    lines.append("| 轮次 | 档位 | in_world | probe_sent | tick_samples | P99 | max | CPU均/峰% | heavy峰 | 负载窗? |")
    lines.append("| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |")
    for t in sorted(reports, key=lambda r: (r.get("run", 0), r.get("count", 0))):
        hl = t.get("host_load") or {}
        lines.append(
            "| r{run} | {count} | {iw} | {ps} | {tsamp} | {p99} | {mx} | {cm}/{cp} | {hv} | {lw} |".format(
                run=t.get("run") or "-", count=t.get("count"), iw=t.get("world_ok"),
                ps=t.get("probe_sent"), tsamp=t.get("samples"), p99=t.get("tick_p99_ms"),
                mx=t.get("max_ms"), cm=hl.get("cpu_mean_pct", "-"), cp=hl.get("cpu_peak_pct", "-"),
                hv=hl.get("heavy_procs_max", "-"), lw=hl.get("loaded_window", "-")))
    lines.append("")
    lines.append("> **M5 待办（写实）**：§6 的「tick P99 ≤ 100ms」**目前在任何栈上都未验证通过**——")
    lines.append("> C# 参考栈 1000 档实测 125/201ms（在共享负载窗口内，归属待空载复测）；待 Rust 栈接管后（M5 灰度）复测。")
    lines.append("> 另：`in_world` 实测只有 ~10%（tier-500/1000 稳态 15~26），**不得**用这三档宣称「M5 三档：1000 假人在线」；")
    lines.append("> 「假人真正进世界」是 M5 收尾缺口（归宿：卡 `openmir2-botsrv-world-evidence`）。")
    lines.append("")
    # 曲线 1：tick P99 / P50 / max vs 档位
    pts_p99 = [(t["count"], t["tick_p99_ms"] or 0) for t in sorted(reports, key=lambda r: r.get("count", 0))]
    pts_p50 = [(t["count"], t["tick_p50_ms"] or 0) for t in sorted(reports, key=lambda r: r.get("count", 0))]
    pts_mx = [(t["count"], t.get("max_ms") or 0) for t in sorted(reports, key=lambda r: r.get("count", 0))]
    lines.append("## 曲线 1：tick 服务时延 vs 假人数（目标 P99 ≤ 100ms）")
    lines.append("")
    lines.append("```html")
    lines.append(svg_curve([("P99", pts_p99), ("P50", pts_p50), ("max", pts_mx)],
                           title="tick latency vs bots", y_label="ms", x_label="bots"))
    lines.append("```")
    # 曲线 2：GameSvr 内存曲线（各档）
    mem_series = []
    for t in sorted(reports, key=lambda r: r.get("count", 0)):
        g = (t.get("mem") or {}).get("GameSrv")
        if g:
            mem_series.append((f"{t['count']} bots", [(t["count"], g.get("ws_peak_mb", 0)),
                                                      (t["count"], g.get("ws_mean_mb", 0))]))
    lines.append("")
    lines.append("## 曲线 2：GameSvr 内存（峰值/均值 vs 假人数）")
    lines.append("")
    lines.append("```html")
    lines.append(svg_curve([(lbl, pts) for lbl, pts in mem_series],
                           title="GameSvr WS vs bots", y_label="MB", x_label="bots"))
    lines.append("```")
    lines.append("")
    lines.append("## 各进程内存（末档）")
    lines.append("")
    last = sorted(reports, key=lambda r: r.get("count", 0))[-1] if reports else None
    if last and last.get("mem"):
        lines.append("| 进程 | 峰值 WS | 均值 WS | 私有峰值 | 末段斜率 | 窗口 |")
        lines.append("| --- | --- | --- | --- | --- | --- |")
        for proc, m in sorted(last["mem"].items()):
            lines.append(f"| {proc} | {m['ws_peak_mb']}MB | {m['ws_mean_mb']}MB | {m['priv_peak_mb']}MB | "
                         f"{m['slope_tail_mb_per_s']}MB/s | {m['window_s']}s |")
    md_path = os.path.join(outdir, "SUMMARY.md")
    with open(md_path, "w", encoding="utf-8", newline="\n") as f:
        f.write("\n".join(lines) + "\n")
    print(f"AGGREGATE_OUT {md_path}")
    return 0


def main():
    ap = argparse.ArgumentParser(description="假人压测指标归算（C6）")
    sub = ap.add_subparsers(dest="cmd", required=True)
    s = sub.add_parser("summarize")
    s.add_argument("--stats", required=True, help="BotSrv 落的 load_stats.ndjson")
    s.add_argument("--mem", default="", help="内存采样 mem.ndjson（可选）")
    s.add_argument("--count", type=int, required=True, help="目标假人数（档位）")
    s.add_argument("--stagger-ms", type=int, default=0)
    s.add_argument("--hold-sec", type=int, default=0)
    s.add_argument("--out", default="")
    s.add_argument("--login-conns", default="",
                   help="BotSrv 落的 login_conns.ndjson（T-11 残余丢包方向归算；缺省不产出该块）")
    s.add_argument("--hostload", default="",
                   help="本机负载探针 hostload.ndjson（owner 裁决 82f8e0c3 ④；缺省不产出该块）")
    s.add_argument("--criteria", default="login,conn,tick",
                   help="判据子集（默认全三条）；M1 ⑤ 那种只比登录/连接的场景用 login,conn")
    s.add_argument("--login-threshold", type=float, default=None,
                   help="登录成功率阈值（%）；缺省按档位取（tier>=1000 为 95%，其余 100%）")
    s.add_argument("--allow-missing-tick", action="store_true",
                   help="显式放行「本次没有 tick 样本」（报告里会写入 missing_tick_note）；"
                        "不加则缺样本一律判 RED，且 --criteria 不能替代本开关")
    a = sub.add_parser("aggregate")
    a.add_argument("--summary", required=True, help="load_gate_summary.json")
    a.add_argument("--out-dir", default="")
    sub.add_parser("selftest")
    args = ap.parse_args()
    if args.cmd == "summarize":
        return cmd_summarize(args)
    if args.cmd == "aggregate":
        return cmd_aggregate(args)
    return cmd_selftest(args)


if __name__ == "__main__":
    sys.exit(main())
