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
import hashlib
import json
import os
import re
import sys

DEFAULT_THRESHOLDS = {
    "login_success_pct": 100.0,     # §6 M5：登录成功率 100%
    "conn_lost_steady": 0,          # §6 M5：无掉线
    # tick 判据（2026-11-22 总监裁决，依据 = 4a 窗口两臂同负载 p99 均 109ms）：
    #   FAIL 线 = **Owner 验收线 200ms**；
    #   100ms 只作 **WARN**（内部余量提示，不判红）；
    #   P50 ≤ 100ms 仍是**硬门**。
    "tick_p99_ms": 200.0,
    "tick_p99_warn_ms": 100.0,
    "tick_p50_ms": 100.0,
    # A1（2026-11-15）：BotSrv 兜住的内部异常数。默认 0（绝对上限）。
    # 唯一的放宽通道 = 卡面评审过的 internal-errors-allowlist.json（按类别 + max）。
    "internal_errors": 0,
}

# A1 逃生口（2026-11-15 总监追加）：白名单是**仓库内文件**，随卡面 patch 评审；
# 运行时没有「随手加一条」的开关。文件缺失/为空/条目非法 ⇒ 容忍度 0（fail-closed）。
DEFAULT_ALLOWLIST_PATH = os.path.join(os.path.dirname(os.path.abspath(__file__)),
                                      "internal-errors-allowlist.json")

# ==== 卡 openmir2-botsrv-recv-thread-nre（总监 2026-11-15）：夹具错误类别普查 ====
# 口径：宣布「全绿」前必须**枚举完** BotSrv 有能力发出的错误类别并逐类计数；
# 全绿 = 枚举出的每一类都出现、且为 0（或有卡面评审过的白名单条目）。
# 本表与 src/BotSrv/BotErrorCensus.cs 的 `Order` **逐字一致**；缺键/多键都判红（枚举漂移可见）。
ERROR_CENSUS_SCHEMA = 1
ERROR_CENSUS_CATEGORIES = {
    # ---- 收包线程（ClientManager.ProcessReviceMessage 的 per-packet catch）----
    "recv_packet_null":  {"class": "fault", "feeds_internal": True,
                          "why": "收包线程处理一帧时的 NullReferenceException；N1 窗口 ~3.95 万条就是这一类（以前只进日志）"},
    "recv_packet_index": {"class": "fault", "feeds_internal": True,
                          "why": "收包线程的 ArgumentOutOfRange/IndexOutOfRange（ActorList 无锁并发增删）"},
    "recv_packet_other": {"class": "fault", "feeds_internal": True,
                          "why": "收包线程的其它异常（未归入上述两类的兜底，出现即需逐个归因）"},
    # ---- 每假人一轮（ClientManager.SafeRun）----
    "run_round_null":    {"class": "fault", "feeds_internal": True,
                          "why": "RobotPlayer.Run/RunAutoPlay 一轮里的 NRE（共享 MShare/ActorList 竞态）"},
    "run_round_index":   {"class": "fault", "feeds_internal": True,
                          "why": "一轮里的 AOORE/IOORE（列表并发改/下标失效）"},
    "run_round_other":   {"class": "fault", "feeds_internal": True,
                          "why": "一轮里的其它异常（兜底）"},
    # ---- 整轮调度（AppService.Run 的 runThread）----
    "run_loop_exception": {"class": "fault", "feeds_internal": True,
                           "why": "runThread 整轮（含建号/AddClient）抛出的异常；以前建号段异常会静默打死线程"},
    # ---- socket 事件处理器（fire-and-forget 边界）----
    "socket_handler_exception": {"class": "fault", "feeds_internal": True,
                                 "why": "OnConnected/OnDisconnected/OnReceivedData/OnError 处理器抛异常；以前会打死收包线程/进程"},
    # ---- socket 系统调用（只计非生命周期错误码）----
    "socket_recv_fault":    {"class": "fault", "feeds_internal": False,
                             "why": "Receive 的非生命周期 SocketError（reset/refused/timeout 等另计 conn_err_*/conn_lost）"},
    "socket_send_fault":    {"class": "fault", "feeds_internal": False,
                             "why": "Send 的非生命周期 SocketError"},
    "socket_connect_fault": {"class": "fault", "feeds_internal": False,
                             "why": "Connect/登录连接建立阶段的非生命周期异常"},
    # ---- 证据链/采集面（失败会静默丢证据）----
    "metric_flush_failure":         {"class": "fault", "feeds_internal": False,
                                     "why": "load_stats.ndjson / login_conns.ndjson 写盘失败（证据链断）"},
    "self_report_write_failure":    {"class": "fault", "feeds_internal": False,
                                     "why": "MIR2_BOT_SELFREPORT 追加失败（自报证据断）"},
    "drop_exp_writer_open_failure": {"class": "fault", "feeds_internal": False,
                                     "why": "掉落/经验采集 writer 打不开"},
    "drop_exp_writer_write_failure": {"class": "fault", "feeds_internal": False,
                                      "why": "掉落/经验行写失败"},
    # ---- 解析/进程级 ----
    "bag_item_decode_failure": {"class": "fault", "feeds_internal": True,
                                "why": "SM_BAGITEMS/SM_ADDITEM 解码失败（背包视图与线上不一致）"},
    "unhandled_exception":     {"class": "fault", "feeds_internal": True,
                                "why": "AppDomain.UnhandledException（进程级；另落 fatal.ndjson）"},
    "unobserved_task_exception": {"class": "fault", "feeds_internal": True,
                                  "why": "TaskScheduler.UnobservedTaskException（fire-and-forget Task）"},
    "internal_error_unclassified": {"class": "fault", "feeds_internal": True,
                                    "why": "走了无类别的 LoadMetrics.InternalError()（新增 catch 漏登记 ⇒ 必须归因）"},
    # ---- 运行期事件（允许非 0；必须出现在报告里）----
    "teardown_race": {"class": "operational", "feeds_internal": False,
                      "why": "主动 Disconnect 后 recv/send 撞 ObjectDisposed（预期非 0；不计故障）"},
}

# 既有的事件计数（不是异常，但属于「错误面」；报告逐项列出，缺失 fail-closed）。
# 这些字段各自已有独立判据（login_success_pct / conn_lost_steady / E2E），本表只保证「枚举到、读得到」。
ERROR_CENSUS_OPERATIONAL_STATS = (
    "conn_refused", "conn_lost",
    "conn_err_refused", "conn_err_reset", "conn_err_timeout", "conn_err_other",
    "login_send_failed", "login_send_closed", "register_send_closed",
    "partial_frames", "chunks_without_hash", "ident_overflow",
    "logon_misdelivered", "in_world_suppressed",
    "autoplay_pre_world", "autoplay_skipped_pre_world",
)

# 卡 openmir2-botsrv-autoplay-nre 已合入 master 39d418816：BeginScene 的三类计数已接线。
# 总监裁决（2026-11-15）：
#   · `scene_null_actor_skipped` **每份报告必须显式列出**（是 0 也要列，不许不提）；
#   · 上界：稳态窗口内 `scene_null_actor_skipped / scene_rounds < 0.5%`，超了必须单独解释；
#   · 语义：并发下读到正被清尾的槽位，安全跳过，不读、不删、不登记；属「已知且受限」，不等于夹具无错；
#   · `scene_list_race_skipped` / `scene_index_race_skipped` 是护栏触发的竞态证据，必须为 0。
SCENE_NULL_UPPER_BOUND_PCT = 0.5
SCENE_NULL_SEMANTICS = ("并发下读到正被清尾的槽位（收包线程 RemoveAt 的瞬时空位）：安全跳过，"
                        "不读、不删、不登记；属已知且受限，不等于夹具无错。")
ERROR_CENSUS_SCENE_STATS = (
    "scene_null_actor_skipped",
    "scene_list_race_skipped",
    "scene_index_race_skipped",
    "scene_rounds",
)

DEFAULT_CENSUS_ALLOWLIST_PATH = os.path.join(os.path.dirname(os.path.abspath(__file__)),
                                             "error-census-allowlist.json")
CENSUS_ALLOWLIST_ENTRY_FIELDS = ("id", "category", "max", "reason", "owner", "review_ref")

# C# 类别表（源文件）——用于「枚举表两边逐字一致」的契约检查。
CENSUS_CS_PATH = os.path.normpath(os.path.join(
    os.path.dirname(os.path.abspath(__file__)), "..", "..", "src", "BotSrv", "BotErrorCensus.cs"))

# BotSrv 日志输出模板（源文件）——真实异常块形态与牙必须绑定它；改模板 ⇒ 牙要跟着红。
APP_SERVER_CS_PATH = os.path.normpath(os.path.join(
    os.path.dirname(os.path.abspath(__file__)), "..", "..", "src", "BotSrv", "AppServer.cs"))


def parse_census_contract(text):
    """解析 C# `BotErrorCensus.cs` 的 Order[] 与 FeedsInternalErrors 集合，和本文件的注册表对账。

    这**不是**行为牙（行为牙在 C# --selftest-error-census 与下面的 evaluate_error_census），
    而是「新增/改名类别时两边必须同时改」的漂移检测：运行期 JSON 缺键/多键已经会 fail-closed，
    本检查让漂移在**没有真机窗口**时也能暴露。
    """
    errs = []
    consts = dict(re.findall(r'public const string (\w+)\s*=\s*"([^"]+)";', text))
    m = re.search(r'private static readonly string\[\] Order\s*=\s*\{(.*?)\};', text, re.S)
    if not m:
        errs.append("找不到 Order[]")
    else:
        order = []
        for ident in re.findall(r'\b([A-Z]\w*)\b', m.group(1)):
            if ident in consts:
                order.append(consts[ident])
            else:
                errs.append("Order 里未知常量 " + ident)
        py_order = list(ERROR_CENSUS_CATEGORIES.keys())
        if order != py_order:
            errs.append("Order 与 Python 类别表不一致：C#=%r Python=%r" % (order, py_order))
    m2 = re.search(
        r'private static readonly HashSet<string> FeedsInternalErrors\s*=\s*new HashSet<string>'
        r'\(StringComparer\.Ordinal\)\s*\{(.*?)\};', text, re.S)
    if not m2:
        errs.append("找不到 FeedsInternalErrors")
    else:
        feeds = set()
        for ident in re.findall(r'\b([A-Z]\w*)\b', m2.group(1)):
            if ident in consts:
                feeds.add(consts[ident])
            else:
                errs.append("FeedsInternalErrors 里未知常量 " + ident)
        py_feeds = set(k for k, v in ERROR_CENSUS_CATEGORIES.items() if v["feeds_internal"])
        if feeds != py_feeds:
            errs.append("feeds_internal 集合不一致：C#=%r Python=%r" % (sorted(feeds), sorted(py_feeds)))
    return errs



# A4（2026-11-15）：结论级窗口前提。HoldSec < 360s 的轮次只可当冒烟，不得作为 GREEN 结论。
# 进世界实测串行载入 ~5–7/s（tier-1000：5s 56 → 75s 425），360s 才够 1000 档走完载入并进入稳态。
MIN_CONCLUSION_HOLD_SEC = 360

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

    # 父卡场景并发计数（卡 openmir2-botsrv-autoplay-nre，master 39d418816 已接线）：
    # 上界分母 = 稳态窗内 BeginScene 实际迭代轮次；null 槽位跳过占比必须 <0.5%。
    def _scene_delta(key):
        if key not in base or key not in last:
            return None
        return last[key] - base[key]

    _scene_rounds_steady = _scene_delta("scene_rounds")
    _scene_null_steady = _scene_delta("scene_null_actor_skipped")
    _scene_null_pct = (round(100.0 * _scene_null_steady / _scene_rounds_steady, 4)
                       if (_scene_rounds_steady and _scene_null_steady is not None) else None)

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
        # T-12（F4/F3(a)）：挂机轮次按“本会话是否进世界”分账 + 注册包观测，展示“改前/改后”。
        "in_world_suppressed": last.get("in_world_suppressed", 0),
        "autoplay_in_world": last.get("autoplay_in_world", 0),
        "autoplay_pre_world": last.get("autoplay_pre_world", 0),
        "autoplay_skipped_pre_world": last.get("autoplay_skipped_pre_world", 0),
        "register_sent": last.get("register_sent", 0),
        "register_send_closed": last.get("register_send_closed", 0),
        "probe_sent": last.get("probe_sent", 0),
        "internal_errors": last.get("internal_errors", 0),
        # A1（2026-11-15）：字段是否存在要单独记 —— 缺失不得静默当 0（judge 里 fail-closed）。
        "internal_errors_present": ("internal_errors" in last),
        "internal_errors_ramp": base.get("internal_errors", 0),
        "internal_errors_steady": (last.get("internal_errors", 0) - base.get("internal_errors", 0)),
        "internal_errors_per_bot": (round(last.get("internal_errors", 0) / count, 3) if count else None),
        # 卡 openmir2-botsrv-recv-thread-nre：错误类别普查（逐类）。缺失必须与 0 区分：
        # error_census_present=False ⇒ judge 里 fail-closed；不得静默当「全 0」。
        "error_census": last.get("error_census"),
        "error_census_present": ("error_census" in last),
        "error_census_schema": last.get("error_census_schema"),
        "error_census_operational_counts": {
            k: last.get(k) for k in ERROR_CENSUS_OPERATIONAL_STATS},
        "error_census_operational_present": {
            k: (k in last) for k in ERROR_CENSUS_OPERATIONAL_STATS},
        # 父卡场景并发计数（已接线）：每份报告必须显式列出；缺失与 0 区分（judge fail-closed）。
        "scene_null_actor_skipped": last.get("scene_null_actor_skipped"),
        "scene_null_actor_skipped_present": ("scene_null_actor_skipped" in last),
        "scene_null_actor_skipped_ramp": base.get("scene_null_actor_skipped"),
        "scene_null_actor_skipped_steady": _scene_null_steady,
        "scene_list_race_skipped": last.get("scene_list_race_skipped"),
        "scene_list_race_skipped_present": ("scene_list_race_skipped" in last),
        "scene_list_race_skipped_steady": _scene_delta("scene_list_race_skipped"),
        "scene_index_race_skipped": last.get("scene_index_race_skipped"),
        "scene_index_race_skipped_present": ("scene_index_race_skipped" in last),
        "scene_index_race_skipped_steady": _scene_delta("scene_index_race_skipped"),
        "scene_rounds": last.get("scene_rounds"),
        "scene_rounds_present": ("scene_rounds" in last),
        "scene_rounds_steady": _scene_rounds_steady,
        "scene_null_skip_pct_steady": _scene_null_pct,
        "scene_null_skip_semantics": SCENE_NULL_SEMANTICS,
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


def summarize_mem(rows, tail_sec=60, peak_mean_pct=5.0):
    """按进程归算内存曲线：峰值/均值/最后 tail_sec 的斜率（MB/s）。

    另给「峰值-均值」标注（README.md 判据表「内存末段斜率」行：M5 长跑另判「峰值-均值 < 5%」）：
      · peak_mean_gap_pct = 100*(ws_peak_mb - ws_mean_mb)/ws_mean_mb（ws_mean_mb<=0 ⇒ None）
      · peak_mean_pct     = 生效阈值回显（默认 5.0）
      · peak_mean_ok      = gap 非 None 且 gap < peak_mean_pct
    **只作标注，不并入 tier["verdict"]**（README 明写「M5 长跑另判」；
    并入会把现有档位 GREEN 翻红 = R 级判据变更，须另开卡）。
    """
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
        ws_peak = round(max(ws), 1)
        ws_mean = round(sum(ws) / len(ws), 1)
        # 「峰值-均值」标注（README「内存末段斜率」行：M5 长跑另判）。只标注，不改 verdict。
        peak_mean_gap = (round(100.0 * (ws_peak - ws_mean) / ws_mean, 1) if ws_mean > 0 else None)
        out[proc] = {
            "samples": len(rs),
            "ws_peak_mb": ws_peak,
            "ws_mean_mb": ws_mean,
            "ws_last_mb": round(ws[-1], 1),
            "priv_peak_mb": round(max((r.get("priv_mb") or 0) for r in rs), 1),
            "slope_tail_mb_per_s": (round(slope, 3) if slope is not None else None),
            "tail_window_s": round((last.get("t_ms", 0) - tail[0].get("t_ms", 0)) / 1000.0, 1) if tail else 0,
            "window_s": round((last.get("t_ms", 0) - t0) / 1000.0, 1),
            "peak_mean_gap_pct": peak_mean_gap,
            "peak_mean_pct": peak_mean_pct,
            "peak_mean_ok": (peak_mean_gap is not None) and peak_mean_gap < peak_mean_pct,
        }
    return out



def _mem_series_stats(rs, tail_sec=60, peak_mean_pct=5.0):
    """一组采样行（单进程或单实例）的内存峰值/均值/末段斜率。与 summarize_mem 的计算逐字段一致。"""
    rs = [r for r in rs if r.get("ws_mb") is not None]
    if not rs:
        return None
    ws = [r["ws_mb"] for r in rs]
    t0 = rs[0].get("t_ms", 0)
    last = rs[-1]
    tail = [r for r in rs if r.get("t_ms", 0) >= last.get("t_ms", 0) - tail_sec * 1000]
    if len(tail) >= 2:
        dt = (tail[-1]["t_ms"] - tail[0]["t_ms"]) / 1000.0
        slope = (tail[-1]["ws_mb"] - tail[0]["ws_mb"]) / dt if dt > 0 else 0.0
    else:
        slope = None
    ws_peak = round(max(ws), 1)
    ws_mean = round(sum(ws) / len(ws), 1)
    peak_mean_gap = (round(100.0 * (ws_peak - ws_mean) / ws_mean, 1) if ws_mean > 0 else None)
    return {
        "samples": len(rs),
        "ws_peak_mb": ws_peak,
        "ws_mean_mb": ws_mean,
        "ws_last_mb": round(ws[-1], 1),
        "priv_peak_mb": round(max((r.get("priv_mb") or 0) for r in rs), 1),
        "slope_tail_mb_per_s": (round(slope, 3) if slope is not None else None),
        "tail_window_s": round((last.get("t_ms", 0) - tail[0].get("t_ms", 0)) / 1000.0, 1) if tail else 0,
        "window_s": round((last.get("t_ms", 0) - t0) / 1000.0, 1),
        "peak_mean_gap_pct": peak_mean_gap,
        "peak_mean_pct": peak_mean_pct,
        "peak_mean_ok": (peak_mean_gap is not None) and (peak_mean_gap < peak_mean_pct),
    }


def summarize_mem_instances(rows, tail_sec=60, peak_mean_pct=5.0):
    """**分片聚合（C4 前置，2026-11-23）**：按 `(proc, pid)` 分组给每个实例的内存曲线，
    另给每个 proc 名下的『合计』视图（把同一 t_ms 的各实例 ws/priv 求和）。

    返回 `{"by_instance": {"BotSvr#1234": {...}}, "by_proc_sum": {"BotSvr": {...}}, "instances": N}`。
    单实例时 by_proc_sum 与 summarize_mem 逐字段相等（向后兼容）；summarize_mem 本身不改。
    """
    by_inst = {}
    for r in rows:
        pid = r.get("pid")
        key = "%s#%s" % (r.get("proc"), pid) if pid is not None else str(r.get("proc"))
        by_inst.setdefault(key, []).append(r)
    inst = {}
    for k, rs in sorted(by_inst.items()):
        st = _mem_series_stats(rs, tail_sec, peak_mean_pct)
        if st is not None:
            inst[k] = st
    # 合计：同 proc 名下按 t_ms 把各实例求和（同一采样循环的各 pid 共享 t_ms）
    by_t = {}
    for r in rows:
        by_t.setdefault(r.get("proc"), {}).setdefault(r.get("t_ms"), []).append(r)
    by_proc_sum = {}
    for proc, tmap in sorted(by_t.items(), key=lambda kv: str(kv[0])):
        summed = []
        for t, rs in sorted(tmap.items(), key=lambda kv: (kv[0] is None, kv[0] or 0)):
            summed.append({
                "proc": proc, "pid": None, "t_ms": t,
                "ws_mb": sum((x.get("ws_mb") or 0) for x in rs),
                "priv_mb": sum((x.get("priv_mb") or 0) for x in rs),
                "cpu_s": sum((x.get("cpu_s") or 0) for x in rs),
            })
        st = _mem_series_stats(summed, tail_sec, peak_mean_pct)
        if st is not None:
            by_proc_sum[proc] = st
    return {"by_instance": inst, "by_proc_sum": by_proc_sum, "instances": len(inst)}


def _sum_numeric_dict(a, b):
    """两个计数字典逐键求和（非数值键保留先到者）。用于 ident_counts / error_census。"""
    out = dict(a or {})
    for k, v in (b or {}).items():
        cur = out.get(k)
        if (isinstance(v, (int, float)) and not isinstance(v, bool)
                and isinstance(cur, (int, float)) and not isinstance(cur, bool)):
            out[k] = cur + v
        elif k not in out:
            out[k] = v
    return out


def _merge_stats_rows(rows):
    """把同一时刻各分片的一行合并为一行：计数求和、hist 逐桶求和、tick_max_ms 取 max、
    ident_counts/error_census 逐键求和、t_ms/hist_bucket_ms/schema 取首个、字符串取首见。"""
    first = rows[0]
    m = dict(first)
    for r in rows[1:]:
        for k, v in r.items():
            if k in ("t_ms", "hist_bucket_ms", "error_census_schema", "hist"):
                continue  # 取首个 / hist 另行逐桶求和
            if k == "tick_max_ms":
                if isinstance(v, (int, float)) and isinstance(m.get(k), (int, float)):
                    m[k] = max(m[k], v)
                else:
                    m[k] = v
            elif isinstance(v, dict):
                m[k] = _sum_numeric_dict(m.get(k) if isinstance(m.get(k), dict) else {}, v)
            elif isinstance(v, (int, float)) and not isinstance(v, bool):
                cur = m.get(k)
                m[k] = (cur + v) if (isinstance(cur, (int, float)) and not isinstance(cur, bool)) else v
            elif k not in m:
                m[k] = v
    hs = [r.get("hist", []) for r in rows]
    n = max((len(h) for h in hs), default=0)
    m["hist"] = [sum((h[i] if i < len(h) else 0) for h in hs) for i in range(n)]
    return m


def merge_shard_stats(stats_list):
    """**分片聚合（C4 前置）**：多份 load_stats.ndjson（每分片一份）按行下标对齐后逐行求和。
    单分片原样返回（向后兼容）。随后交给 `summarize_tick(merged, total_count)` ⇒
    login/world/internal/tick 全部『求和后再算比率』（不是对各分片比率取平均）。"""
    stats_list = [s for s in stats_list if s]
    if not stats_list:
        return []
    if len(stats_list) == 1:
        return stats_list[0]
    n = min(len(s) for s in stats_list)
    return [_merge_stats_rows([s[i] for s in stats_list]) for i in range(n)]


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


# BotSrv 异常块的四类落盘形态（2026-11-15 卡 `openmir2-botsrv-errorstack-diagnosable`）：
#   ① bot_round：`假人[id]异常被兜住[N]：<ex.ToString()>` —— 有类型行 + `   at ` 帧；
#   ② run_loop ：`压测兜住的异常[N]：<Type>: <Message>`        —— 有类型，无堆栈；
#   ③ socket   ：`socket 事件处理器异常被兜住：<ex.ToString()>` —— 有类型 + 堆栈；
#   ④ err      ：`[ERR] <message>`（`LogService.Error(Exception)` 的产物）。修复前
#      `LogService.cs` 写成 `Logger.Error(ex.Message, ex)` ⇒ Serilog 绑泛型重载 ⇒
#      `LogEvent.Exception=null` ⇒ 只有消息（本轮 360s 的 4 条就是这种）；修复后
#      消息里带 `<Type>: <Message>`，堆栈由 `{Exception}` 续在后续行。
# 归桶键 = (类别, 异常类型, 消息)：只看类型会把同类型的未知消息一起豁免。
_INTERNAL_ERR_RE = re.compile(
    r"(?:"
    r"(?P<bot>异常被兜住\[\d+\])：|"
    r"(?P<loop>压测兜住的异常\[\d+\])：|"
    r"(?P<socket>socket 事件处理器异常被兜住)：|"
    r"\[ERR\]"
    r")\s*(?P<body>.*)$")
# 从块首行抽异常类型：`System.NullReferenceException: ...` / `NullReferenceException: ...`。
# 只认以 `Exception` 结尾的 .NET 全名/短名，避免把 `[ERR] some: thing` 误当类型。
_EXC_TYPE_RE = re.compile(r"^\s*(?P<type>[A-Za-z_][\w.`]*Exception)\s*:\s*(?P<msg>.*)$")
# 真实 bots.log 每行以 `HH:mm:ss.fff ` 开头（AppServer.cs 的 outputTemplate）。
# 块延续只在「非时间戳行」上发生，遇到下一条时间戳行即结束。
# **前提**：Serilog Console sink 对每个 LogEvent 原子写出（一条事件的 message/`{Exception}`
# 不会与另一条事件交错），所以「下一条时间戳行」就是块边界；这不是在赌日志格式。
_TS_RE = re.compile(r"^\d{1,2}:\d{2}:\d{2}[.,]\d{3}\s")
_STACK_RE = re.compile(r"^\s+at\s+")


def summarize_internal_error_log(path, max_types=5, max_stack=3):
    """A1（2026-11-15）：从 BotSrv 的 `bots.log` 摘要 `internal_errors` 的 top 类型与堆栈首行。

    `load_stats.ndjson.internal_errors` 只有计数；类型/堆栈只在 BotSrv 的日志里，且
    `LoadMetrics.ShouldLogInternalError` **只打前 20 条**（避免刷爆日志）。
    所以本函数给的是「样本的 top」，总量一律以 ndjson 计数为准，报告里两者分开写。

    **块 = 一条异常块，可跨行**（真实 `outputTemplate` = `{Message:lj}{NewLine}{Exception}`）：
      `[ERR] <msg>` 行之后，紧跟的非时间戳 `System.XxxException: <msg>` 行并入**同一块**
      作为类型/消息，其后的 `   at ` 行并入该块的 stack_head；遇下一条时间戳行才结束。
      跨块拼凑（A 块只有类型、B 块只有堆栈）**不算可诊断**（卡
      `openmir2-botsrv-errorstack-diagnosable` R2 裁决）。

    字段词义：
      · `logged_blocks`         = **可识别**异常块数（含只有消息的 `[ERR]`）；
      · `typed_blocks`          = 其中能抽出**异常类型**的块数；
      · `untyped_blocks`        = `logged_blocks - typed_blocks`；
      · `stacked_blocks`        = 至少带 1 个 `   at ` 帧的块数；
      · `typed_stacked_blocks`  = **同一块内**既有类型又有堆栈的块数（R2 ①）。
    `diagnosable=true` 要求 `typed_stacked_blocks > 0` 且 `untyped_blocks == 0`（R2 ②，fail-closed）。

    按 **(类别, 异常类型, 消息)** 分桶：白名单要「精确到能区分已知无害与未知」，
    只看类型会把同类型的未知消息一起豁免。
    """
    if not path or not os.path.isfile(path):
        return {"log": path, "available": False,
                "reason": "bots.log 不存在/未提供（top 异常类型无从归算）"}
    blocks = []
    cur = None
    with open(path, "r", encoding="utf-8", errors="replace") as f:
        for line in f:
            m = _INTERNAL_ERR_RE.search(line)
            if m:
                if m.group("bot"):
                    cat = "bot_round"
                elif m.group("loop"):
                    cat = "run_loop"
                elif m.group("socket"):
                    cat = "socket"
                else:
                    cat = "err"
                body = (m.group("body") or "").strip()
                tm = _EXC_TYPE_RE.match(body)
                cur = {
                    "category": cat,
                    "type": (tm.group("type") if tm else None),
                    "message": (tm.group("msg").strip() if tm else body),
                    "stack_head": [],
                    "has_stack": False,
                    "sample": line.strip()[:300],
                }
                blocks.append(cur)
                continue
            if cur is None:
                continue
            if _STACK_RE.match(line):
                if not cur["has_stack"]:
                    cur["has_stack"] = True
                if len(cur["stack_head"]) < max_stack:
                    cur["stack_head"].append(line.strip()[:300])
                continue
            # 真实模板形态：`[ERR] <msg>` 之后紧跟的类型行并入**同一块**。
            # 只接受非时间戳行，避免把下一条日志误当成类型行。
            if cur["type"] is None and not _TS_RE.match(line):
                tm = _EXC_TYPE_RE.match(line)
                if tm:
                    cur["type"] = tm.group("type")
                    cur["message"] = tm.group("msg").strip()
                    continue
            # 其余行（含下一条时间戳行）结束当前块。
            cur = None
    types = {}
    typed = 0
    stacked = 0
    typed_stacked = 0
    categories = {}
    for b in blocks:
        cat = b["category"]
        categories[cat] = categories.get(cat, 0) + 1
        if b["type"]:
            typed += 1
        if b["has_stack"]:
            stacked += 1
        if b["type"] and b["has_stack"]:
            typed_stacked += 1
        key = (cat, b["type"], b["message"])
        e = types.setdefault(key, {
            "category": cat, "type": b["type"], "message": b["message"][:300],
            "count": 0, "stack_head": [], "sample": b["sample"],
        })
        e["count"] += 1
        for sh in b["stack_head"]:
            if len(e["stack_head"]) < max_stack:
                e["stack_head"].append(sh)
    logged = len(blocks)
    ordered = sorted(types.values(),
                     key=lambda x: (-x["count"], x["category"], x["type"] or "", x["message"]))
    return {
        "log": path,
        "available": True,
        "logged_blocks": logged,
        "typed_blocks": typed,
        "untyped_blocks": logged - typed,
        "stacked_blocks": stacked,
        "typed_stacked_blocks": typed_stacked,
        "categories": categories,
        "types": ordered[:max_types],
        "types_total": len(ordered),
        "note": ("BotSrv 只打前 20 条异常（ShouldLogInternalError）；"
                 "总量看 load_stats.ndjson.internal_errors。"
                 "logged_blocks=可识别异常块数（含只有消息的 [ERR]），"
                 "typed_blocks=能抽出异常类型的块数，stacked_blocks=带 `   at ` 帧的块数，"
                 "typed_stacked_blocks=同块内类型+堆栈齐的块数；"
                 "diagnosable 要求同块证据且 untyped_blocks==0"),
    }


def load_bots_runtime(path):
    """读 `load_gate.ps1` 落的 `bots_runtime.json`（PowerShell 写 UTF-8 无 BOM，容错 BOM）。"""
    with open(path, "rb") as f:
        raw = f.read().decode("utf-8-sig")
    rt = json.loads(raw)
    if rt.get("oom_hits") is None:
        oom_lines = rt.get("err_log_oom_lines") or []
        rt["oom_hits"] = (len(oom_lines) if rt.get("err_log_oom") else 0) + len(rt.get("app_error_events") or [])
    return rt


ALLOWLIST_ENTRY_FIELDS = ("category", "message_pattern", "reason", "owner", "review_ref")


def empty_allowlist(path=None, error="allowlist 未提供/缺失 ⇒ 容忍度 0（fail-closed）"):
    """容忍度 0 的白名单：任何 internal_errors > 0 都判 RED。"""
    return {"path": path, "available": False, "valid": True, "error": error,
            "sha256": None, "entries": [], "entries_count": 0}


def _validate_allowlist_entry(e, idx):
    """返回错误列表；任何必填项缺失/非法 ⇒ 该条不得生效（不得静默豁免）。"""
    errs = []
    if not isinstance(e, dict):
        return ["not an object"]
    for k in ALLOWLIST_ENTRY_FIELDS:
        if not isinstance(e.get(k), str) or not e.get(k).strip():
            errs.append(k)
    mx = e.get("max")
    if isinstance(mx, bool) or not isinstance(mx, int) or mx < 0:
        errs.append("max(>=0 int)")
    pat = e.get("message_pattern")
    if isinstance(pat, str) and pat:
        try:
            re.compile(pat)
        except re.error as ex:
            errs.append("message_pattern(regex: %s)" % ex)
    return errs


def load_allowlist(path):
    """读 internal_errors 白名单（仓库文件，随卡面 patch 评审）。

    语义（总监 2026-11-15）：
      · 文件缺失/为空/JSON 坏/条目非法 ⇒ 容忍度 0（fail-closed），**不是**「全部放行」；
      · 条目按 `category`（异常类型）+ `message_pattern`（消息正则）匹配；未列入的类别一律 RED；
      · `max` 是该类别允许的累计次数上限；总量超过预算 ⇒ RED。
    """
    if not path:
        return empty_allowlist(path, "allowlist 路径为空 ⇒ 容忍度 0（fail-closed）")
    if not os.path.isfile(path):
        return empty_allowlist(path, "白名单文件不存在 ⇒ 容忍度 0（fail-closed）")
    try:
        with open(path, "rb") as f:
            raw = f.read()
    except OSError as ex:
        return empty_allowlist(path, "白名单读不了：%s ⇒ 容忍度 0" % ex)
    out = {"path": path, "available": True, "valid": True, "error": None,
           "sha256": hashlib.sha256(raw).hexdigest(), "entries": [], "entries_count": 0}
    try:
        doc = json.loads(raw.decode("utf-8-sig"))
    except (ValueError, UnicodeDecodeError) as ex:
        out["valid"] = False
        out["error"] = "白名单 JSON 非法：%s ⇒ 容忍度 0" % ex
        return out
    entries = doc.get("entries") if isinstance(doc, dict) else None
    if not isinstance(entries, list):
        out["valid"] = False
        out["error"] = "白名单缺 entries[]（或不是数组）⇒ 容忍度 0"
        return out
    good = []
    for i, e in enumerate(entries):
        errs = _validate_allowlist_entry(e, i)
        if errs:
            out["valid"] = False
            prev = out["error"] + "；" if out["error"] else ""
            out["error"] = prev + ("条目 #%d 非法（%s），已忽略" % (i, ", ".join(errs)))
            continue
        good.append({
            "id": (e.get("id") if isinstance(e.get("id"), str) and e.get("id").strip()
                   else "#%d" % i),
            "category": e["category"].strip(),
            "message_pattern": e["message_pattern"],
            "max": int(e["max"]),
            "reason": e["reason"].strip(),
            "owner": e["owner"].strip(),
            "review_ref": e["review_ref"].strip(),
        })
    out["entries"] = good
    out["entries_count"] = len(good)
    return out


def _allowlist_hit(entry, t):
    if entry.get("category") != t.get("type"):
        return False
    try:
        return re.search(entry["message_pattern"], t.get("message") or "") is not None
    except re.error:
        return False


def evaluate_internal_errors(tier, allowlist):
    """按白名单评估 internal_errors：返回可审计块 + fails（未列入/超预算/无法归因）。

    · 总量 = load_stats.ndjson 的累计计数；类别/次数 = bots.log 的样本（BotSrv 只打前 20 条）。
    · 每个样本桶按文件顺序匹配**第一条**命中的白名单条目；命中条目去重后 `max` 之和 = 预算。
    · 未命中任何条目的桶 ⇒ RED；总量 > 预算 ⇒ RED；没有样本可归因但总量 > 0 ⇒ RED。
    """
    total = tier.get("internal_errors")
    diag = tier.get("internal_error_diag") or {}
    samples = diag.get("types") or []
    ev = {
        "path": (allowlist or {}).get("path"),
        "sha256": (allowlist or {}).get("sha256"),
        "available": bool((allowlist or {}).get("available")),
        "valid": bool((allowlist or {}).get("valid", True)),
        "error": (allowlist or {}).get("error"),
        "entries": (allowlist or {}).get("entries", []),
        "entries_count": (allowlist or {}).get("entries_count", 0),
        "total": total,
        "budget": 0,
        "matched": [],
        "unmatched": [],
        "excused": False,
        "fails": [],
        "review_note": "白名单是卡面 patch 的一部分，必须随 patch 评审；运行时没有新增条目/覆盖路径的开关",
    }
    if tier.get("internal_errors_present") is False:
        ev["fails"].append("load_stats 缺 internal_errors 字段（无从判定 ⇒ fail-closed）")
        return ev
    if total is None:
        ev["fails"].append("internal_errors=None（缺计数 ⇒ fail-closed）")
        return ev
    if total <= 0:
        ev["excused"] = True
        return ev
    if not samples:
        ev["fails"].append("internal_errors %d > 0 但 bots.log 无异常类型归因（白名单无法证明无害 ⇒ fail-closed）" % total)
        return ev
    used = {}
    for t in samples:
        hit = None
        for entry in ev["entries"]:
            if _allowlist_hit(entry, t):
                hit = entry
                break
        if hit is None:
            ev["unmatched"].append({"type": t.get("type"), "count": t.get("count"),
                                    "message": t.get("message")})
        else:
            ev["matched"].append({"type": t.get("type"), "count": t.get("count"),
                                  "message": t.get("message"),
                                  "entry_id": hit["id"], "max": hit["max"]})
            used[hit["id"]] = hit
    ev["budget"] = sum(e["max"] for e in used.values())
    if ev["unmatched"]:
        ev["fails"].append("internal_errors 含未列入白名单的类别：" + ", ".join(
            "%s×%s" % (u["type"] or "(untyped)", u["count"]) for u in ev["unmatched"]) +
            "（白名单只豁免已评审的已知无害类别）")
    if total > ev["budget"]:
        detail = ", ".join("%s %s/%s（%s）" % (m["type"], m["count"], m["max"], m["entry_id"])
                           for m in ev["matched"]) or "无可匹配条目"
        ev["fails"].append("internal_errors %d > 白名单预算 %d（%s）" % (total, ev["budget"], detail))
    ev["excused"] = not ev["fails"]
    return ev


def empty_census_allowlist(path=None, error="census 白名单未提供/缺失 ⇒ 容忍度 0（fail-closed）"):
    return {"path": path, "available": False, "valid": True, "error": error,
            "sha256": None, "entries": [], "entries_count": 0}


def _validate_census_entry(e, idx):
    errs = []
    if not isinstance(e, dict):
        return ["not an object"]
    for k in CENSUS_ALLOWLIST_ENTRY_FIELDS:
        if not isinstance(e.get(k), str) or not e.get(k).strip():
            errs.append(k)
    if isinstance(e.get("category"), str) and e["category"].strip() not in ERROR_CENSUS_CATEGORIES:
        errs.append("category(未登记的类别)")
    mx = e.get("max")
    if isinstance(mx, bool) or not isinstance(mx, int) or mx < 0:
        errs.append("max(>=0 int)")
    return errs


def load_census_allowlist(path):
    """读「错误类别普查」白名单（仓库文件，随卡面 patch 评审）。

    与 internal-errors-allowlist 的分工：
      · feeds_internal=True 的类别照旧可由 internal-errors-allowlist 豁免（同一个异常类型白名单）；
      · 其余 fault 类别（socket/证据链等）只能由本文件按 `category + max` 豁免；
      · 文件缺失/空/条目非法 ⇒ 容忍度 0。`teardown_race` 是 operational，不需要豁免。
    """
    if not path:
        return empty_census_allowlist(path, "census allowlist 路径为空 ⇒ 容忍度 0")
    if not os.path.isfile(path):
        return empty_census_allowlist(path, "census 白名单文件不存在 ⇒ 容忍度 0")
    try:
        with open(path, "rb") as f:
            raw = f.read()
    except OSError as ex:
        return empty_census_allowlist(path, "census 白名单读不了：%s ⇒ 容忍度 0" % ex)
    out = {"path": path, "available": True, "valid": True, "error": None,
           "sha256": hashlib.sha256(raw).hexdigest(), "entries": [], "entries_count": 0}
    try:
        doc = json.loads(raw.decode("utf-8-sig"))
    except (ValueError, UnicodeDecodeError) as ex:
        out["valid"] = False
        out["error"] = "census 白名单 JSON 非法：%s ⇒ 容忍度 0" % ex
        return out
    entries = doc.get("entries") if isinstance(doc, dict) else None
    if not isinstance(entries, list):
        out["valid"] = False
        out["error"] = "census 白名单缺 entries[] ⇒ 容忍度 0"
        return out
    good = []
    for i, e in enumerate(entries):
        errs = _validate_census_entry(e, i)
        if errs:
            out["valid"] = False
            prev = out["error"] + "；" if out["error"] else ""
            out["error"] = prev + ("条目 #%d 非法（%s），已忽略" % (i, ", ".join(errs)))
            continue
        good.append({
            "id": e["id"].strip(),
            "category": e["category"].strip(),
            "max": int(e["max"]),
            "reason": e["reason"].strip(),
            "owner": e["owner"].strip(),
            "review_ref": e["review_ref"].strip(),
        })
    out["entries"] = good
    out["entries_count"] = len(good)
    return out


def _census_allowlist_hit(entries, category):
    for e in entries:
        if e.get("category") == category:
            return e
    return None


def evaluate_diagnosability(total, diag):
    """卡 `openmir2-botsrv-errorstack-diagnosable`：`internal_errors>0` 时必须能拿到
    异常类型与堆栈首行；「有计数但拿不到可诊断文本」本身判红（fail-closed）。

    R2 裁决（2026-11-15 总监）：`diagnosable=true` 必须**同时**满足
      ① 至少一个块**同块**内既有类型又有 stack_head（`typed_stacked_blocks > 0`，
         不接受 A 块出类型、B 块出堆栈的跨块拼凑）；
      ② `untyped_blocks == 0`（有一个未类型化块就不算可诊断——宁红不绿）。

    其余分支（全部 fail-closed）：
      · total 为 None（缺计数）⇒ false + fail；
      · total <= 0 ⇒ 不适用（`diagnosable=None`，不产生 fail；空输入不得误报）；
      · bots.log 摘要不可用/未提供 ⇒ false + fail；
      · 有块但 `typed_blocks==0`（只有 `[ERR] <msg>`）⇒ false + fail；
      · 有类型也有一块带堆栈、但没有一块同时具备 ⇒ false（跨块拼凑）；
      · 类型齐但没有任何堆栈帧 ⇒ false（知道是什么、不知道在哪）。

    为什么**不**用 `internal_errors > logged_blocks` 单独判红：BotSrv 只打前 20 条
    （`ShouldLogInternalError`），样本数天然可能小于总量；只要**样本**满足 R2①②，
    就满足「知道是什么、在哪」。`total > logged_blocks` 只记 `sampled_only=True`
    （报告可见），不单独判红——否则 3.95 万条 NRE 的窗口会因采样上限永久假红。
    """
    d = diag or {}
    out = {
        "internal_errors": total,
        "diag_available": bool(d.get("available")),
        "logged_blocks": d.get("logged_blocks"),
        "typed_blocks": d.get("typed_blocks"),
        "untyped_blocks": d.get("untyped_blocks"),
        "stacked_blocks": d.get("stacked_blocks"),
        "typed_stacked_blocks": d.get("typed_stacked_blocks"),
        "sampled_only": None,
        "coverage_note": "",
        "diagnosable": None,
        "status": "not_applicable",
        "fails": [],
    }
    if total is None:
        out["diagnosable"] = False
        out["status"] = "missing_count"
        out["fails"].append("internal_errors=None：无法判断异常是否可诊断 ⇒ fail-closed")
        return out
    if total <= 0:
        return out
    if not d.get("available"):
        out["diagnosable"] = False
        out["status"] = "missing_diag"
        out["fails"].append(
            "internal_errors %d > 0 但 bots.log 摘要不可用（无异常类型/堆栈）⇒ fail-closed" % total)
        return out
    logged = d.get("logged_blocks") or 0
    typed = d.get("typed_blocks") or 0
    stacked = d.get("stacked_blocks") or 0
    typed_stacked = d.get("typed_stacked_blocks")
    if typed_stacked is None:
        typed_stacked = 0
    untyped = d.get("untyped_blocks")
    if untyped is None:
        untyped = max(logged - typed, 0)
    out["sampled_only"] = total > logged
    if out["sampled_only"]:
        out["coverage_note"] = (
            "internal_errors %d > 样本 %d：BotSrv 只打前 20 条（ShouldLogInternalError）；"
            "样本满足 R2 即通过，未采样部分不可逐条归因" % (total, logged))
    if logged <= 0:
        out["diagnosable"] = False
        out["status"] = "no_blocks"
        out["fails"].append(
            "internal_errors %d > 0 但 bots.log 无可识别异常块 ⇒ 有计数无文本（fail-closed）" % total)
    elif typed <= 0:
        out["diagnosable"] = False
        out["status"] = "untyped_blocks"
        out["fails"].append(
            "internal_errors %d > 0 但 %d 个可识别块无一抽出异常类型"
            "（只有消息，如 `[ERR] Object reference not set...`）⇒ 不可诊断（fail-closed）"
            % (total, logged))
    elif untyped > 0:
        out["diagnosable"] = False
        out["status"] = "mixed_blocks"
        out["fails"].append(
            "internal_errors %d > 0：%d 个可识别块中仍有 %d 个未类型化"
            "（R2②：有一个未类型化块就不算可诊断）⇒ fail-closed"
            % (total, logged, untyped))
    elif typed_stacked <= 0:
        out["diagnosable"] = False
        if stacked > 0:
            out["status"] = "cross_block_only"
            out["fails"].append(
                "internal_errors %d > 0：有类型块也有带堆栈的块，但没有一个块**同块**具备"
                "类型+堆栈（R2①：跨块拼凑不算可诊断）⇒ fail-closed" % total)
        else:
            out["status"] = "no_stack"
            out["fails"].append(
                "internal_errors %d > 0：%d 个块有异常类型但无堆栈帧（无法定位）⇒ fail-closed"
                % (total, typed))
    else:
        out["diagnosable"] = True
        out["status"] = "ok"
    return out


def evaluate_error_census(tier, allowlist=None, internal_excused=False,
                          scene_null_explanation="",
                          scene_null_upper_bound_pct=SCENE_NULL_UPPER_BOUND_PCT):
    """逐类核对夹具错误面（卡 openmir2-botsrv-recv-thread-nre）。

    判定（全部 fail-closed）：
      1. `error_census` 对象必须存在、schema 匹配；
      2. 类别集合必须与 ERROR_CENSUS_CATEGORIES **完全相等**：缺键 = 旧版本/漏接线，多键 = 未登记的新类别；
      3. 每个 fault 类别必须为 0；>0 时只有两种豁免：feeds_internal 且 internal_errors 已被
         internal-errors-allowlist 整体豁免（同一份评审），或 census 白名单按 category+max 登记；
      4. `internal_errors` 必须等于 Σ(feeds_internal 类别)——任何绕过分类入口的计数都会被这条等式抓出；
      5. 既有事件计数（ERROR_CENSUS_OPERATIONAL_STATS）必须全部出现在 load_stats 里（缺失即枚举不完整）；
      6. 父卡（master 39d418816）四个场景字段必须全部出现：
         · `scene_null_actor_skipped` **每份报告显式列出**（0 也列）；稳态占比必须 < 上界（默认 0.5%），
           超了必须用 `scene_null_explanation` 单独解释，否则 RED；
         · `scene_list_race_skipped` / `scene_index_race_skipped` 是竞态证据，必须为 0；
         · `scene_rounds` 是上界分母（BeginScene 实际迭代轮次），缺失则无法判上界 ⇒ RED。
    `operational` 类别允许非 0，但必须逐类出现在报告里。
    """
    ev = {
        "schema_expected": ERROR_CENSUS_SCHEMA,
        "present": bool(tier.get("error_census_present")),
        "schema": tier.get("error_census_schema"),
        "categories": [],
        "operational": [],
        "parent_scene": [],
        "scene_null": None,
        "fault_total": 0,
        "fault_categories_nonzero": [],
        "internal_errors": tier.get("internal_errors"),
        "internal_census_sum": None,
        "diagnosability": None,
        "allowlist": {
            "path": (allowlist or {}).get("path"),
            "sha256": (allowlist or {}).get("sha256"),
            "available": bool((allowlist or {}).get("available")),
            "valid": bool((allowlist or {}).get("valid", True)),
            "error": (allowlist or {}).get("error"),
            "entries": (allowlist or {}).get("entries", []),
            "entries_count": (allowlist or {}).get("entries_count", 0),
        },
        "fails": [],
        "review_note": ("类别表与 src/BotSrv/BotErrorCensus.cs 的 Order 必须逐字一致；"
                        "白名单随卡面 patch 评审，运行时没有新增条目的开关；"
                        "父卡 scene_* 计数已接线，scene_null_actor_skipped 每份报告必列"),
    }
    if not ev["present"]:
        ev["fails"].append("load_stats 缺 error_census 字段（错误类别无法逐类核对 ⇒ fail-closed）")
    else:
        if tier.get("error_census_schema") != ERROR_CENSUS_SCHEMA:
            ev["fails"].append("error_census_schema=%s ≠ %d（口径变更必须同步两边）"
                               % (tier.get("error_census_schema"), ERROR_CENSUS_SCHEMA))
        actual = tier.get("error_census")
        if not isinstance(actual, dict):
            ev["fails"].append("error_census 不是 JSON 对象 ⇒ fail-closed")
            actual = {}
        missing = [c for c in ERROR_CENSUS_CATEGORIES if c not in actual]
        extra = [k for k in actual if k not in ERROR_CENSUS_CATEGORIES]
        if missing:
            ev["fails"].append("error_census 缺类别：" + ", ".join(missing) +
                               "（枚举表与实现漂移 ⇒ fail-closed）")
        if extra:
            ev["fails"].append("error_census 含未登记类别：" + ", ".join(extra) +
                               "（新增错误类别必须登记到两边）")
        entries = (allowlist or {}).get("entries", [])
        for cat, meta in ERROR_CENSUS_CATEGORIES.items():
            if cat not in actual:
                continue
            v = actual.get(cat)
            row = {"category": cat, "class": meta["class"],
                   "feeds_internal": bool(meta["feeds_internal"]),
                   "count": v, "status": "zero", "why": meta["why"]}
            valid = isinstance(v, int) and not isinstance(v, bool) and v >= 0
            if not valid:
                row["status"] = "invalid"
                ev["fails"].append("error_census.%s 不是非负整数（%r）⇒ fail-closed" % (cat, v))
            elif meta["class"] == "fault":
                ev["fault_total"] += v
                if v > 0:
                    if meta["feeds_internal"] and internal_excused:
                        row["status"] = "excused_by_internal_allowlist"
                    else:
                        hit = _census_allowlist_hit(entries, cat)
                        if hit is not None and v <= hit["max"]:
                            row["status"] = "excused_by_census_allowlist"
                            row["allow_entry_id"] = hit["id"]
                            row["allow_max"] = hit["max"]
                        else:
                            row["status"] = "nonzero_fault"
                            ev["fault_categories_nonzero"].append(cat)
                            why = (" 超过 census 白名单 max=%d（%s）" % (hit["max"], hit["id"])
                                   if hit is not None else " 未登记（fault 类别默认容忍度 0）")
                            ev["fails"].append("error_census 类别 %s=%d > 0%s" % (cat, v, why))
            else:
                row["status"] = "operational"
            ev["categories"].append(row)

        # internal_errors 对账：必须等于 Σ(feeds_internal 类别)。
        if not tier.get("internal_errors_present", False):
            ev["fails"].append("load_stats 缺 internal_errors 字段（无法与分类计数对账 ⇒ fail-closed）")
        else:
            total = tier.get("internal_errors")
            census_sum = 0
            for row in ev["categories"]:
                if row["feeds_internal"] and isinstance(row["count"], int) and not isinstance(row["count"], bool):
                    census_sum += row["count"]
            ev["internal_census_sum"] = census_sum
            if total is None:
                ev["fails"].append("internal_errors=None（缺计数 ⇒ fail-closed）")
            elif total != census_sum:
                ev["fails"].append(
                    "internal_errors %s ≠ 已分类故障之和 %s（存在未分类/绕过分类入口的计数）"
                    % (total, census_sum))

    # 诊断维度（卡 `openmir2-botsrv-errorstack-diagnosable`）：
    # 有 internal_errors 计数但摘要拿不到类型/堆栈 ⇒ 判红（fail-closed），并显式进报告。
    _ie_total = tier.get("internal_errors") if tier.get("internal_errors_present") else None
    ev["diagnosability"] = evaluate_diagnosability(_ie_total, tier.get("internal_error_diag"))
    ev["fails"].extend(ev["diagnosability"]["fails"])

    # 既有事件计数：必须出现（缺失 = 枚举不完整）。
    ops = tier.get("error_census_operational_counts") or {}
    ops_present = tier.get("error_census_operational_present") or {}
    for key in ERROR_CENSUS_OPERATIONAL_STATS:
        present = bool(ops_present.get(key))
        ev["operational"].append({"counter": key, "present": present, "count": ops.get(key)})
        if not present:
            ev["fails"].append("load_stats 缺既有事件计数 %s（错误面枚举不完整 ⇒ fail-closed）" % key)

    # 父卡场景并发计数（master 39d418816 已接线）：四个字段必须全部出现。
    # scene_null_actor_skipped 每份报告显式列出（0 也列）；上界见 SCENE_NULL_UPPER_BOUND_PCT。
    scene_last = {k: tier.get(k) for k in ERROR_CENSUS_SCENE_STATS}
    scene_present = {
        "scene_null_actor_skipped": bool(tier.get("scene_null_actor_skipped_present")),
        "scene_list_race_skipped": bool(tier.get("scene_list_race_skipped_present")),
        "scene_index_race_skipped": bool(tier.get("scene_index_race_skipped_present")),
        "scene_rounds": bool(tier.get("scene_rounds_present")),
    }
    for key in ERROR_CENSUS_SCENE_STATS:
        if not scene_present[key]:
            ev["fails"].append("load_stats 缺 scene 计数 %s（父卡已接线；每份报告必须出现）⇒ fail-closed"
                               % key)
    null_last = scene_last["scene_null_actor_skipped"]
    null_steady = tier.get("scene_null_actor_skipped_steady")
    rounds_steady = tier.get("scene_rounds_steady")
    scene_null = {
        "counter": "scene_null_actor_skipped",
        "count": null_last,
        "steady": null_steady,
        "scene_rounds": scene_last["scene_rounds"],
        "scene_rounds_steady": rounds_steady,
        "pct_steady": tier.get("scene_null_skip_pct_steady"),
        "upper_bound_pct": scene_null_upper_bound_pct,
        "semantics": SCENE_NULL_SEMANTICS,
        "status": "missing",
        "explanation": scene_null_explanation or "",
    }
    if not scene_present["scene_null_actor_skipped"] or not scene_present["scene_rounds"]:
        scene_null["status"] = "missing"
    elif null_steady is None or rounds_steady is None:
        scene_null["status"] = "insufficient_window"
        if (null_last or 0) > 0:
            ev["fails"].append("scene_null_actor_skipped 累计 %s > 0 但稳态窗口算不出"
                               "（base 行缺字段）⇒ fail-closed" % null_last)
    elif null_steady < 0 or rounds_steady < 0:
        scene_null["status"] = "invalid"
        ev["fails"].append("scene_null/scene_rounds 稳态倒退了（last < base）⇒ fail-closed")
    elif rounds_steady == 0:
        if null_steady > 0:
            scene_null["status"] = "rounds_missing"
            ev["fails"].append("scene_null_actor_skipped 稳态 %d > 0 但 scene_rounds=0"
                               "（分母缺 ⇒ fail-closed）" % null_steady)
        else:
            scene_null["status"] = "no_rounds"
    else:
        pct = round(100.0 * null_steady / rounds_steady, 4)
        scene_null["pct_steady"] = pct
        if pct < scene_null_upper_bound_pct:
            scene_null["status"] = "within_upper_bound"
        elif scene_null["explanation"]:
            scene_null["status"] = "over_upper_bound_explained"
        else:
            scene_null["status"] = "over_upper_bound"
            ev["fails"].append(
                "scene_null_actor_skipped 稳态占比 %.4f%% ≥ %.4g%%（%d/%d 轮）⇒ 需单独解释"
                % (pct, scene_null_upper_bound_pct, null_steady, rounds_steady))
    ev["scene_null"] = scene_null
    for key in ERROR_CENSUS_SCENE_STATS:
        row = {"counter": key, "present": scene_present[key], "count": scene_last[key],
               "steady": tier.get(key + "_steady")}
        if key == "scene_null_actor_skipped":
            row["status"] = scene_null["status"]
        elif key in ("scene_list_race_skipped", "scene_index_race_skipped"):
            v = scene_last[key]
            if not scene_present[key]:
                row["status"] = "missing"
            elif isinstance(v, int) and not isinstance(v, bool) and v == 0:
                row["status"] = "zero"
            else:
                row["status"] = "nonzero_fault"
                ev["fails"].append("%s=%r > 0（父卡护栏触发的竞态证据，必须为 0）" % (key, v))
        else:
            row["status"] = "denominator" if scene_present[key] else "missing"
        ev["parent_scene"].append(row)
    ev["complete"] = not ev["fails"]
    return ev


def judge(tier, thresholds, criteria=("login", "conn", "tick"), allow_missing_tick=False,
          login_threshold=None, world_threshold=None, bots_runtime=None,
          allowlist=None, census_allowlist=None,
          scene_null_explanation="", scene_null_upper_bound_pct=SCENE_NULL_UPPER_BOUND_PCT):
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
    warns = []
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
    # 在线（进世界）判据（T-12，可选）：默认 None=不判 —— M5 三档的 in_world 实测只有 ~10%，
    # 不能当默认阈值。显式给出时必须 >= 阈值；`world_ok_pct` 缺失或不足都判红（不是恒绿）。
    tier["world_threshold_pct"] = world_threshold
    if world_threshold is not None:
        wp = tier.get("world_ok_pct")
        if wp is None or wp < world_threshold:
            fails.append(f"在线（本会话收到 SM_LOGON 的假人数）{wp}% < {world_threshold}%")
    tick_missing = tier.get("tick_p99_ms") is None
    if tick_missing:
        tier["tick_missing"] = True
        if allow_missing_tick:
            tier["tick_missing_acknowledged"] = True
            tier["missing_tick_note"] = "本次未采集 tick P99（--allow-missing-tick 显式放行；不得当作 M5 的 tick 基线）"
        else:
            fails.append("缺 tick 样本（tick_p99_ms = None）：默认判 RED；确实拿不到 tick 的场景也必须显式加 --allow-missing-tick")
    elif "tick" in criteria:
        # 2026-11-22 总监裁决：FAIL = Owner 验收线（200ms）；>100ms 降为 WARN（内部余量提示）。
        if tier["tick_p99_ms"] > thresholds["tick_p99_ms"]:
            fails.append(
                f"tick P99 {tier['tick_p99_ms']}ms > {thresholds['tick_p99_ms']}ms（Owner 验收线）")
        elif tier["tick_p99_ms"] > thresholds.get("tick_p99_warn_ms", 0):
            warns.append(
                f"tick P99 {tier['tick_p99_ms']}ms > {thresholds['tick_p99_warn_ms']}ms"
                f"（内部余量提示；未达 Owner 验收线 {thresholds['tick_p99_ms']}ms，不判红）")
        p50 = tier.get("tick_p50_ms")
        p50_thr = thresholds.get("tick_p50_ms")
        if p50 is not None and p50_thr is not None and p50 > p50_thr:
            fails.append(f"tick P50 {p50}ms > {p50_thr}ms")
    # A1（2026-11-15）：internal_errors 判据。默认 0 容忍；唯一放宽通道是按类别评审的白名单
    # （未列入/超 max/无法归因/白名单缺失 ⇒ 都按容忍度 0 处理，fail-closed）。
    tier["internal_error_allowlist"] = evaluate_internal_errors(tier, allowlist)
    # 卡 openmir2-botsrv-recv-thread-nre：internal_errors 只是「总量」，census 是「逐类」。
    # 两者一起才满足总监规则「枚举出的每一类都为 0」；internal 判据默认包含 census。
    # 注意：internal_errors=0 时 evaluate_internal_errors 的 excused=True 只表示「无需豁免」，
    # **不能**拿它去豁免一个非 0 的 feeds_internal 类别（否则报告状态会误导）；必须 total > 0。
    _ie = tier["internal_error_allowlist"]
    tier["error_census"] = evaluate_error_census(
        tier, census_allowlist,
        internal_excused=bool(_ie.get("excused")) and bool(_ie.get("total")),
        scene_null_explanation=scene_null_explanation,
        scene_null_upper_bound_pct=scene_null_upper_bound_pct)
    if "internal" in criteria:
        fails.extend(tier["internal_error_allowlist"]["fails"])
        fails.extend(tier["error_census"]["fails"])
    # A2（2026-11-15）：BotSrv 崩溃判据。退出码只认「进程自己退出」；门禁 Stop-Process 的不算。
    tier["botsrv"] = bots_runtime
    if bots_runtime:
        rt = bots_runtime
        if rt.get("self_exited") is True:
            code = rt.get("exit_code")
            if code is None:
                fails.append("BotSrv 自行退出但退出码不可读（fail-closed）")
            elif int(code) != 0:
                hexs = rt.get("exit_code_hex") or ""
                fails.append("BotSrv 非 0 退出 code=" + str(code) +
                             ((" (" + str(hexs) + ")") if hexs else ""))
        if rt.get("err_log_oom") is True:
            oom = rt.get("err_log_oom_lines") or []
            excerpt = oom[0][:120] if oom else "见 bots.err.log"
            fails.append("bots.err.log 含 'Out of memory.'（BotSrv OOM；" + excerpt + "）")
        ev = rt.get("app_error_events") or []
        if ev:
            fails.append("Windows Application Error 0xe0434352 ×" + str(len(ev)) + "（BotSrv 崩溃事件）")
    # A4（2026-11-15）：窗口前提。**只作结论准入，不改 verdict**（避免静默改变 M1⑤/冒烟口径）。
    hold = tier.get("hold_sec") or 0
    window_ok = bool(hold >= MIN_CONCLUSION_HOLD_SEC)
    tier["window"] = {
        "hold_sec": hold,
        "hold_sec_required": MIN_CONCLUSION_HOLD_SEC,
        "hold_ok": window_ok,
        "conclusion_valid": window_ok,
        "conclusion_invalid_reason": ("" if window_ok else
                                      f"HoldSec {hold}s < {MIN_CONCLUSION_HOLD_SEC}s："
                                      "只可作为冒烟，不得作为压测 GREEN 结论"),
        "in_world_ramp_note": ("进世界串行载入实测 ~5–7/s（tier-1000：5s 56 → 75s 425）；"
                               "HoldSec<360s 的轮次不足以让 1000 档走完载入并进入稳态"),
    }
    tier["ramp_target"] = int(-(-tier.get("count", 0) * login_thr // 100))
    tier["criteria"] = list(criteria)
    tier["allow_missing_tick"] = bool(allow_missing_tick)
    tier["warns"] = warns
    tier["verdict"] = "GREEN" if not fails else "RED"
    tier["fails"] = fails
    return tier


def cmd_summarize(args):
    stat_paths = args.stats if isinstance(args.stats, list) else ([args.stats] if args.stats else [])
    stat_paths = [x for x in stat_paths if x]
    stats = merge_shard_stats([load_ndjson(p) for p in stat_paths])
    tier = summarize_tick(stats, args.count)
    tier["shards"] = {"stats_files": len(stat_paths), "merged": len(stat_paths) > 1}
    if args.mem:
        _mem_rows = load_ndjson(args.mem)
        tier["mem"] = summarize_mem(_mem_rows, tail_sec=args.mem_tail_sec)
        _mi = summarize_mem_instances(_mem_rows, tail_sec=args.mem_tail_sec)
        if _mi["instances"] > 1:
            tier["mem_instances"] = _mi
    # 卡 openmir2-botsrv-actor-pool-leak（总监追加）：只采样落盘、不参与越界判定的服务端进程名。
    # 声明值与「实际在 mem 行里出现过的名字」分开记录：不在跑 ⇒ 无行即为事实，不补 0。
    extra_names = [x for x in (args.mem_extra_names or "").replace(",", " ").split() if x]
    if extra_names:
        tier["mem_proc_names_extra"] = extra_names
        tier["mem_proc_names_extra_present"] = [n for n in extra_names if n in (tier.get("mem") or {})]
    if args.login_conns and os.path.exists(args.login_conns):
        tier["login_conn_diag"] = summarize_login_conns(load_ndjson(args.login_conns))
    if args.hostload and os.path.exists(args.hostload):
        tier["host_load"] = summarize_hostload(load_ndjson(args.hostload))
    # A1：top 异常类型/堆栈（来自 bots.log；样本只取日志里打出来的前 20 条）。
    if args.bots_log:
        tier["internal_error_diag"] = summarize_internal_error_log(args.bots_log)
    # A2：BotSrv 崩溃证据（load_gate.ps1 落的 bots_runtime.json）。
    bots_runtime = load_bots_runtime(args.bots_runtime) if args.bots_runtime else None
    # A1 逃生口：白名单从仓库文件读（默认 tools/botload/internal-errors-allowlist.json）；
    # 缺失/为空/非法 ⇒ 容忍度 0。路径与 sha256 进报告，便于评审核对运行时没有被换文件。
    allowlist = load_allowlist(args.allowlist)
    # 卡 openmir2-botsrv-recv-thread-nre：错误类别普查白名单（文件缺失/为空 ⇒ 容忍度 0）。
    census_allowlist = load_census_allowlist(args.census_allowlist)
    tier["count"] = args.count
    tier["stagger_ms"] = args.stagger_ms
    tier["hold_sec"] = args.hold_sec
    # 在线假人数 = 本会话收到 SM_LOGON 的假人数（T-12：只看该假人自己那条连接的证据，不看共享 MShare）
    tier["online_bots"] = tier.get("world_ok", 0)
    tier["online_base"] = args.count
    tier["online_source"] = "per-session SM_LOGON（BotSrv RobotPlayer.MarkInWorld ⇒ LoadMetrics.InWorld）"
    tier["autoplay_source"] = ("F4/T-12：挂机轮次只看本会话 InWorld（autoplay_pre_world 应为 0）；"
                               ">0 只可能来自 MIR2_BOT_LEGACY_SHARED_AUTOPLAY=1 的“改前”对照组")
    criteria = [c.strip() for c in args.criteria.split(",") if c.strip()]
    login_thr = args.login_threshold if args.login_threshold is not None else default_login_threshold(args.count)
    judge(tier, DEFAULT_THRESHOLDS, criteria, allow_missing_tick=args.allow_missing_tick,
          login_threshold=login_thr, world_threshold=args.world_threshold,
          bots_runtime=bots_runtime, allowlist=allowlist,
          census_allowlist=census_allowlist,
          scene_null_explanation=args.scene_null_explanation,
          scene_null_upper_bound_pct=args.scene_null_upper_bound_pct)
    # 总监硬规矩（2026-11-15）：每份结论必须显式列 login / no_resp / internal_errors / OOM。
    # None 一律表示「证据缺」，不得被下游当成 0。
    diag = tier.get("login_conn_diag") or {}
    ie_al = tier.get("internal_error_allowlist") or {}
    ec = tier.get("error_census") or {}
    _scene_null = ec.get("scene_null") or {}
    tier["required_fields"] = {
        "login_success_pct": tier.get("login_success_pct"),
        "login_conn_no_resp": (diag.get("no_resp") if diag else None),
        "internal_errors": (tier.get("internal_errors") if tier.get("internal_errors_present") else None),
        "oom_hits": (bots_runtime.get("oom_hits") if bots_runtime else None),
        "botsrv_self_exited": (bots_runtime.get("self_exited") if bots_runtime else None),
        "botsrv_exit_code": (bots_runtime.get("exit_code") if bots_runtime else None),
        "internal_errors_excused": ie_al.get("excused"),
        "internal_errors_allowlist_budget": ie_al.get("budget"),
        "internal_errors_allowlist_sha256": ie_al.get("sha256"),
        # 卡 openmir2-botsrv-errorstack-diagnosable：有计数但拿不到类型/堆栈必须可见且判红。
        "internal_errors_diagnosable": (ec.get("diagnosability") or {}).get("diagnosable"),
        "internal_errors_diag_status": (ec.get("diagnosability") or {}).get("status"),
        "internal_errors_logged_blocks": (ec.get("diagnosability") or {}).get("logged_blocks"),
        "internal_errors_typed_blocks": (ec.get("diagnosability") or {}).get("typed_blocks"),
        "internal_errors_stacked_blocks": (ec.get("diagnosability") or {}).get("stacked_blocks"),
        "internal_errors_sampled_only": (ec.get("diagnosability") or {}).get("sampled_only"),
        # 卡 openmir2-botsrv-recv-thread-nre：逐类普查的准入字段。
        "error_census_complete": ec.get("complete"),
        "error_census_fault_total": ec.get("fault_total"),
        "error_census_nonzero": ec.get("fault_categories_nonzero"),
        "error_census_categories_checked": len(ec.get("categories") or []),
        "error_census_allowlist_sha256": (ec.get("allowlist") or {}).get("sha256"),
        # 总监 2026-11-15：scene_null_actor_skipped 每份报告必须显式列出（0 也列），
        # 并给稳态占比与上界；语义固定写死，避免报告措辞漂移。
        "scene_null_actor_skipped": _scene_null.get("count"),
        "scene_null_actor_skipped_steady": _scene_null.get("steady"),
        "scene_null_scene_rounds_steady": _scene_null.get("scene_rounds_steady"),
        "scene_null_skip_pct_steady": _scene_null.get("pct_steady"),
        "scene_null_skip_upper_bound_pct": _scene_null.get("upper_bound_pct"),
        "scene_null_skip_status": _scene_null.get("status"),
        "scene_null_skip_explanation": _scene_null.get("explanation"),
        "scene_null_skip_semantics": _scene_null.get("semantics"),
        "scene_list_race_skipped": tier.get("scene_list_race_skipped"),
        "scene_index_race_skipped": tier.get("scene_index_race_skipped"),
        "verdict": tier["verdict"],
        "warns": tier.get("warns") or [],
        "note": ("四项必列字段 + 错误类别普查 + scene_null 上界；None = 证据缺（不得当作 0）；"
                 "scene_null_actor_skipped 是 0 也必须显式列出；不得写「夹具无错」"),
    }
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
    # 2b') 在线（进世界）判据的牙齿（T-12，F4）：同一批产物，抹掉一个假人的进世界证据 ⇒ 必红；齐 ⇒ 绿；
    #      不显式给阈值时不得因 in_world 少而判红（M5 三档的 in_world 只有 ~10%）。
    statsW_bad = [dict(stats[0], login_ok=3, in_world=2),
                  dict(stats[1], login_ok=3, in_world=2, hist=list(hist2), tick_max_ms=5)]
    statsW_ok = [dict(stats[0], login_ok=3, in_world=3),
                 dict(stats[1], login_ok=3, in_world=3, hist=list(hist2), tick_max_ms=5)]
    j_bad = judge(summarize_tick(statsW_bad, 3), DEFAULT_THRESHOLDS, world_threshold=100.0)
    j_ok = judge(summarize_tick(statsW_ok, 3), DEFAULT_THRESHOLDS, world_threshold=100.0)
    expect(j_bad["verdict"] == "RED" and any("在线" in f for f in j_bad["fails"]),
           "抹掉一个假人的进世界证据（in_world=2/3）在 world_threshold=100 下必须 RED（在线判据有牙齿）")
    expect(j_ok["verdict"] == "GREEN", "进世界证据齐（in_world=3/3）在 world_threshold=100 下必须 GREEN")
    expect(judge(summarize_tick(statsW_bad, 3), DEFAULT_THRESHOLDS)["verdict"] == "GREEN",
           "不显式给 world_threshold 时不得因 in_world 少而判红（默认不判在线）")
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

    # 4b) 「峰值-均值」判据（README「内存末段斜率」行：M5 长跑另判 峰值-均值 < 5%）：
    #     100 点 100MB + 1 点 150MB 的尖峰 ⇒ gap≈50% ⇒ peak_mean_ok=False；
    #     平坦 ⇒ gap≈0 ⇒ True。布尔随数据变（非恒值），阈值也有牙齿（5% 判假、60% 判真）。
    mem_spike = ([{"t_ms": i * 1000, "proc": "GameSvr", "pid": 1, "ws_mb": 100, "priv_mb": 120}
                  for i in range(100)]
                 + [{"t_ms": 100 * 1000, "proc": "GameSvr", "pid": 1, "ws_mb": 150, "priv_mb": 170}])
    s_spike = summarize_mem(mem_spike)["GameSvr"]
    expect(s_spike["peak_mean_ok"] is False and s_spike["peak_mean_gap_pct"] >= 49.0,
           f"尖峰（100×100MB + 1×150MB）gap 应 ≈50% 且 peak_mean_ok=False，实得 "
           f"gap={s_spike['peak_mean_gap_pct']} ok={s_spike['peak_mean_ok']}")
    expect(s_flat["peak_mean_ok"] is True and abs(s_flat["peak_mean_gap_pct"]) < 0.01,
           f"平坦曲线 gap 应 ≈0 且 peak_mean_ok=True，实得 "
           f"gap={s_flat['peak_mean_gap_pct']} ok={s_flat['peak_mean_ok']}")
    expect(s_spike["peak_mean_pct"] == 5.0, "peak_mean_pct 应回显默认阈值 5.0")
    expect(summarize_mem(mem_spike, peak_mean_pct=60.0)["GameSvr"]["peak_mean_ok"] is True,
           "峰值-均值阈值有牙齿：尖峰在 5% 下判假、在 60% 下判真")

    # 4c) --mem-tail-sec 接线牙：「前 50s 升、后 50s 平」⇒ tail=50 斜率≈0、tail=100 斜率>0；
    #     且不传 tail_sec 必须逐字段等价于显式 60（默认行为不变，不破坏既有报告口径）。
    mem_bend = ([{"t_ms": i * 1000, "proc": "GameSvr", "pid": 1, "ws_mb": 100 + i, "priv_mb": 120 + i}
                 for i in range(51)]
                + [{"t_ms": i * 1000, "proc": "GameSvr", "pid": 1, "ws_mb": 150, "priv_mb": 170}
                   for i in range(51, 101)])
    b50 = summarize_mem(mem_bend, tail_sec=50)["GameSvr"]
    b100 = summarize_mem(mem_bend, tail_sec=100)["GameSvr"]
    expect(abs(b50["slope_tail_mb_per_s"]) < 0.01,
           f"后 50s 平的曲线在 tail_sec=50 斜率应 ≈0，实得 {b50['slope_tail_mb_per_s']}")
    expect(b100["slope_tail_mb_per_s"] > 0.4,
           f"含上升段的曲线在 tail_sec=100 斜率应 >0.4，实得 {b100['slope_tail_mb_per_s']}")
    expect(b100["tail_window_s"] > b50["tail_window_s"],
           "tail_window_s 必须随 tail_sec 变（参数真接线，非死参数）")
    expect(summarize_mem(mem_bend)["GameSvr"] == summarize_mem(mem_bend, tail_sec=60)["GameSvr"],
           "不传 tail_sec 必须逐字段等价于 tail_sec=60（默认行为不变）")

    # 4d) 「峰值-均值」只作标注、不并入 verdict（README 明写「M5 长跑另判」）：
    #     即便内存是尖峰，login-only 判据仍 GREEN，且 fails 里不得出现「峰值」。
    tier_pm = summarize_tick(stats, 100)
    tier_pm["mem"] = summarize_mem(mem_spike)
    j_pm = judge(tier_pm, DEFAULT_THRESHOLDS, ("login",))
    expect(j_pm["verdict"] == "GREEN" and not any("峰值" in f for f in j_pm["fails"]),
           "尖峰内存的 peak_mean 标注不得把档位翻红（只标注不改判 = 非 R 级判据变更）")

    # 4e) CLI 缝牙齿（2026-10-15 验证者 F-1/F-2 补）：4c 只钉了**函数**默认与接线，
    #     真实调用方走 argparse（load_gate.ps1:267），故此处钉两个 CLI 级不变式：
    #     (i) argparse 默认值必须 =60（改默认 = 静默改变既有报告口径）；
    #     (ii) --mem-tail-sec 必须真接到 call site（解析后丢弃/不接线 ⇒ 与默认逐字节相同 ⇒ 必红）。
    import subprocess
    import tempfile
    with tempfile.TemporaryDirectory() as _td:
        _mem = os.path.join(_td, "mem.ndjson")
        _stats = os.path.join(_td, "stats.ndjson")
        with open(_mem, "w", encoding="utf-8", newline=chr(10)) as _f:
            for _i in range(51):
                print(json.dumps({"t_ms": _i * 1000, "proc": "GameSvr", "pid": 1,
                                  "ws_mb": 100 + _i, "priv_mb": 120 + _i}), file=_f)
            for _i in range(51, 101):
                print(json.dumps({"t_ms": _i * 1000, "proc": "GameSvr", "pid": 1,
                                  "ws_mb": 150, "priv_mb": 170}), file=_f)
        with open(_stats, "w", encoding="utf-8", newline=chr(10)) as _g:
            for _i in range(2):
                print(json.dumps({"t_ms": (_i + 1) * 1000, "login_ok": 100, "tick_samples": 5,
                                  "tick_sum_ms": 100, "tick_max_ms": 30, "conn_lost": 0,
                                  "hist": [0] * 201}), file=_g)

        def _cli_mem(_extra):
            _tag = ("_".join(_extra) or "default").replace("-", "")
            _out = os.path.join(_td, "r_%s.json" % _tag)
            try:
                _p = subprocess.run([sys.executable, "-I", "-B", os.path.abspath(__file__),
                                     "summarize", "--stats", _stats, "--count", "100",
                                     "--mem", _mem, "--out", _out] + _extra,
                                    capture_output=True, text=True,
                                    stdin=subprocess.DEVNULL, timeout=120)
            except subprocess.TimeoutExpired:
                class _Timeout:  # 与 rc 判据同形：非 0/1 即「真崩溃」
                    returncode = 124
                    stdout = ""
                    stderr = "TIMEOUT>120s"
                _p = _Timeout()
            # rc=0/1 = 正常退出（1 只是 judge 判 RED，与本节所钉的接线/默认无关）；其它值 = 真崩溃。
            expect(_p.returncode in (0, 1),
                   "CLI summarize 应 rc in (0,1)，实得 " + str(_p.returncode) + "；stderr 尾部=" + _p.stderr[-200:])
            with open(_out, encoding="utf-8") as _fh:
                return json.load(_fh)["mem"]["GameSvr"]

        _m_default = _cli_mem([])
        _m_explicit = _cli_mem(["--mem-tail-sec", "60"])
        _m_20 = _cli_mem(["--mem-tail-sec", "20"])
        expect(_m_default["tail_window_s"] == 60.0,
               "CLI 默认 --mem-tail-sec 必须 60（改默认=静默改既有报告口径），实得 "
               + str(_m_default["tail_window_s"]))
        expect(_m_default == _m_explicit,
               "CLI 不传 --mem-tail-sec 必须与 --mem-tail-sec 60 逐字段等价")
        expect(_m_20["tail_window_s"] == 20.0,
               "--mem-tail-sec 20 必须真接线到 call site（解析后丢弃必红），实得 tail_window_s="
               + str(_m_20["tail_window_s"]))
        expect(abs(_m_20["slope_tail_mb_per_s"]) < 0.01 < _m_default["slope_tail_mb_per_s"],
               "后 20s 平 / 默认 60s 含上升段 ⇒ 两档斜率必须不同（死参数必红）：20⇒"
               + str(_m_20["slope_tail_mb_per_s"]) + " vs 默认⇒" + str(_m_default["slope_tail_mb_per_s"]))

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
    # 有样本时开关无副作用：仍只按阈值判，且不得出现 tick_missing 标注。
    # 2026-11-22 裁决：FAIL=200ms、WARN=100ms ⇒ P99=150 必须 GREEN + WARN（不是 RED）。
    t7 = judge(summarize_tick(stats, 100), DEFAULT_THRESHOLDS, ("login", "conn", "tick"), allow_missing_tick=True)
    expect(t7["tick_p99_ms"] == 150 and t7["verdict"] == "GREEN",
           "P99=150 在 FAIL=200/WARN=100 下必须 GREEN（只有 WARN）")
    expect(any("tick P99 150" in w for w in t7.get("warns") or []),
           "P99=150 必须出现在 warns 里（内部余量提示）")
    expect("tick_missing" not in t7, "有样本时不得出现 tick_missing 标注")

    # 新判据的牙：P99=250 ⇒ RED；P50=110 ⇒ RED（硬门保留）；P99=200 恰好不过线不算红。
    t_hi = dict(t7); t_hi["tick_p99_ms"] = 250; t_hi["warns"] = []
    t_hi = judge(t_hi, DEFAULT_THRESHOLDS, ("login", "conn", "tick"))
    expect(t_hi["verdict"] == "RED" and any("tick P99 250" in f for f in t_hi["fails"]),
           "P99=250 > FAIL 200 ⇒ 必须 RED")
    t_p50 = dict(t7); t_p50["tick_p50_ms"] = 110; t_p50["warns"] = []
    t_p50 = judge(t_p50, DEFAULT_THRESHOLDS, ("login", "conn", "tick"))
    expect(t_p50["verdict"] == "RED" and any("tick P50 110" in f for f in t_p50["fails"]),
           "P50=110 > 100 ⇒ 必须 RED（P50 硬门）")
    t_edge = dict(t7); t_edge["tick_p99_ms"] = 200; t_edge["warns"] = []
    t_edge = judge(t_edge, DEFAULT_THRESHOLDS, ("login", "conn", "tick"))
    expect(t_edge["verdict"] == "GREEN", "P99=200（=FAIL 线）不判红（严格大于才红）")

    # 6) A1（2026-11-15）：internal_errors 判据 —— 默认 0 容忍；有异常必红；0 不变绿；缺字段 fail-closed。
    i3 = ("login", "conn", "tick", "internal")
    # 卡 openmir2-botsrv-recv-thread-nre：内部判据现在含「错误类别普查」。给所有 internal 用例提供
    # 完整的 census 对象（缺键会让旧用例变成 fail-closed 的噪声，掩盖真正要测的判定）。
    def _census(**over):
        d = {k: 0 for k in ERROR_CENSUS_CATEGORIES}
        d.update(over)
        return d

    def _ops(**over):
        d = {k: 0 for k in ERROR_CENSUS_OPERATIONAL_STATS}
        d.update(over)
        return d

    def _with_census(row, internal=None, census=None, ops=None, schema=ERROR_CENSUS_SCHEMA,
                     scene_null=0, scene_list=0, scene_index=0, scene_rounds=100):
        r = dict(row)
        r["error_census"] = _census(**(census or {}))
        r["error_census_schema"] = schema
        for _k in ERROR_CENSUS_OPERATIONAL_STATS:
            r[_k] = (ops or {}).get(_k, 0)
        r["scene_null_actor_skipped"] = scene_null
        r["scene_list_race_skipped"] = scene_list
        r["scene_index_race_skipped"] = scene_index
        r["scene_rounds"] = scene_rounds
        if internal is not None:
            r["internal_errors"] = internal
        return r

    def _census_tier(internal=0, census=None, ops=None, drop_census=False, drop_cat=None,
                     extra_cat=None, schema=ERROR_CENSUS_SCHEMA,
                     scene_null=0, scene_list=0, scene_index=0, scene_rounds=100,
                     drop_scene=None):
        # 三行：row0 = ramp-complete 基线（login 100，hist 全 0）；row1 = 中间行；row2 = last。
        # 稳态窗口 = row2 - row0，所以 scene_null 的占比 = scene_null / scene_rounds。
        rows = [
            _with_census(stats[0], internal=0, scene_rounds=0),
            _with_census(dict(stats[1], t_ms=2000, hist=list(hist2),
                              tick_sum_ms=5000, tick_max_ms=5), internal=internal,
                         census=census, ops=ops, schema=schema,
                         scene_rounds=scene_rounds // 2),
            _with_census(dict(stats[1], t_ms=3000, hist=list(hist2),
                              tick_sum_ms=5000, tick_max_ms=5), internal=internal,
                         census=census, ops=ops, schema=schema, scene_null=scene_null,
                         scene_list=scene_list, scene_index=scene_index,
                         scene_rounds=scene_rounds),
        ]
        if drop_census:
            rows[2].pop("error_census", None)
        if drop_cat:
            rows[2]["error_census"].pop(drop_cat, None)
        if extra_cat:
            rows[2]["error_census"][extra_cat] = 0
        if drop_scene:
            rows[2].pop(drop_scene, None)
        return summarize_tick(rows, 100)

    # 干净的 tick 分布（hist2 = 全部 5ms，P99=5）：让下面只由 internal/崩溃/窗口决定 verdict。
    stats_ie_ok = [_with_census(stats[0], internal=0),
                   _with_census(dict(stats[1], hist=list(hist2), tick_max_ms=5), internal=0)]
    stats_ie_bad = [_with_census(stats[0], internal=0),
                    _with_census(dict(stats[1], hist=list(hist2), tick_max_ms=5),
                                 internal=1061, census={"recv_packet_null": 1061})]
    t_ie_ok = judge(summarize_tick(stats_ie_ok, 100), DEFAULT_THRESHOLDS, i3)
    t_ie_bad = judge(summarize_tick(stats_ie_bad, 100), DEFAULT_THRESHOLDS, i3)
    expect(t_ie_ok["verdict"] == "GREEN", "internal_errors=0 不得把正常轮次翻红")
    expect(t_ie_bad["verdict"] == "RED" and any("internal_errors 1061" in f for f in t_ie_bad["fails"]),
           "internal_errors=1061 必须判红（默认阈值 0）")
    expect(t_ie_bad["internal_errors_per_bot"] == 10.61,
           f"按 bot 归一应可读（1061/100=10.61），实得 {t_ie_bad['internal_errors_per_bot']}")
    # 6a) A1 逃生口（总监 2026-11-15）：白名单按「类型+消息正则」匹配，max 内不红但必须列出；
    #     超 max / 未列入 / 缺文件 / 条目非法 ⇒ RED（容忍度 0）。
    _al_entries = [{
        "id": "nre-known", "category": "System.NullReferenceException",
        "message_pattern": r"Object reference not set",
        "max": 2, "reason": "已知无害（自测）", "owner": "selftest", "review_ref": "selftest"}]
    _al = {"path": "<selftest>", "available": True, "valid": True, "error": None,
           "sha256": "0" * 64, "entries": _al_entries, "entries_count": 1}
    # 真实形态：类型 + `   at ` 堆栈（`diagnosable` 判据要求两者都有）。
    _diag_nre = {"available": True, "logged_blocks": 2, "typed_blocks": 2,
                 "untyped_blocks": 0, "stacked_blocks": 2, "typed_stacked_blocks": 2,
                 "categories": {"bot_round": 2},
                 "types": [
        {"category": "bot_round", "type": "System.NullReferenceException",
         "message": "Object reference not set", "count": 2,
         "stack_head": ["at BotSrv.Scenes.Scene.PlayScene.BeginScene()"],
         "sample": "..."}]}

    def _with_ie(total, diag):
        _t = summarize_tick([_with_census(stats[0], internal=0),
                             _with_census(dict(stats[1], hist=list(hist2), tick_max_ms=5),
                                          internal=total,
                                          census={"recv_packet_null": total})], 100)
        if diag is not None:
            _t["internal_error_diag"] = diag
        return _t

    # ① 白名单内 ≤max ⇒ 不 RED，且报告列出 类别/次数/上限
    t_al_ok = judge(_with_ie(2, _diag_nre), DEFAULT_THRESHOLDS, i3, allowlist=_al)
    expect(t_al_ok["verdict"] == "GREEN", "白名单内 2/2 不得判红")
    _m = (t_al_ok.get("internal_error_allowlist") or {}).get("matched") or []
    expect(_m and _m[0]["type"] == "System.NullReferenceException" and _m[0]["count"] == 2
           and _m[0]["max"] == 2 and t_al_ok["internal_error_allowlist"]["budget"] == 2,
           "白名单命中必须在报告列出 类别/次数/上限")
    # ② 白名单内 >max ⇒ RED
    t_al_over = judge(_with_ie(5, {"available": True, "types": [
        {"type": "System.NullReferenceException", "message": "Object reference not set",
         "count": 5}]}), DEFAULT_THRESHOLDS, i3, allowlist=_al)
    expect(t_al_over["verdict"] == "RED" and any("白名单预算 2" in f for f in t_al_over["fails"]),
           "白名单内 5 > max 2 必须判红")
    # ③ 未列入的类别 ⇒ RED（即便另一个类别预算很大）
    _al_big = dict(_al)
    _al_big["entries"] = [dict(_al_entries[0], max=100)]
    _diag_oom = {"available": True, "logged_blocks": 1, "typed_blocks": 1,
                 "untyped_blocks": 0, "stacked_blocks": 0, "typed_stacked_blocks": 0,
                 "types": [
        {"category": "run_loop", "type": "System.OutOfMemoryException",
         "message": "boom", "count": 1, "stack_head": []}]}
    t_al_un = judge(_with_ie(1, _diag_oom), DEFAULT_THRESHOLDS, i3, allowlist=_al_big)
    expect(t_al_un["verdict"] == "RED" and any("未列入白名单" in f for f in t_al_un["fails"]),
           "未列入白名单的类别必须判红（不能靠别的类别预算兜住）")
    # ④ 缺文件/为空 ⇒ 容忍度 0（1 次也 RED）
    _missing = load_allowlist(os.path.join(tempfile.gettempdir(), "__no_such_allowlist__.json"))
    expect(_missing["available"] is False and _missing["entries_count"] == 0, "缺文件应得到空白名单")
    t_al_miss = judge(_with_ie(1, _diag_nre), DEFAULT_THRESHOLDS, i3, allowlist=_missing)
    expect(t_al_miss["verdict"] == "RED" and any("未列入白名单" in f for f in t_al_miss["fails"]),
           "白名单缺文件/为空 ⇒ 容忍度 0（1 次也 RED）")
    # 条目非法（未通过校验）⇒ 不生效；仓库默认白名单必须存在且为空（容忍度 0）
    _bad = {"path": "<selftest>", "available": True, "valid": False, "error": "bad",
            "sha256": None, "entries_count": 0, "entries": []}
    expect(judge(_with_ie(1, _diag_nre), DEFAULT_THRESHOLDS, i3, allowlist=_bad)["verdict"] == "RED",
           "白名单条目非法/未生效 ⇒ 容忍度 0")
    _default_al = load_allowlist(DEFAULT_ALLOWLIST_PATH)
    expect(_default_al["available"] is True and _default_al["valid"] is True
           and _default_al["entries_count"] == 0,
           "仓库默认白名单必须存在且 entries 为空（容忍度 0）")
    # criteria 不含 internal 时不判（M1⑤ 的 login,conn 口径不变）
    expect(judge(summarize_tick(stats_ie_bad, 100), DEFAULT_THRESHOLDS, ("login", "conn")
                 )["verdict"] == "GREEN",
           "criteria 不含 internal 时不得判 internal（M1⑤ 口径不变）")
    # 缺字段 fail-closed
    stats_missing = [dict(stats_ie_ok[0]), dict(stats_ie_ok[1])]
    for _r in stats_missing:
        _r.pop("internal_errors", None)
    t_miss = judge(summarize_tick(stats_missing, 100), DEFAULT_THRESHOLDS, i3)
    expect(t_miss["verdict"] == "RED" and any("缺 internal_errors" in f for f in t_miss["fails"]),
           "load_stats 缺 internal_errors 字段必须 fail-closed（不得当 0）")

    # 6b) bots.log 摘要：top 类型 + 堆栈首行（BotSrv 只打前 20 条，样本语义要写清）
    with tempfile.TemporaryDirectory() as _td2:
        _lg = os.path.join(_td2, "bots.log")
        with open(_lg, "w", encoding="utf-8", newline=chr(10)) as _f:
            _f.write("02:00:00.000 [WRN] 假人[loadbot1]异常被兜住[1]："
                     "System.NullReferenceException: Object reference not set\n")
            _f.write("   at BotSrv.Scenes.Scene.PlayScene.BeginScene()\n")
            _f.write("   at BotSrv.Player.RobotPlayer.Run()\n")
            _f.write("02:00:00.100 [WRN] 假人[loadbot2]异常被兜住[2]："
                     "System.NullReferenceException: Object reference not set\n")
            _f.write("02:00:00.200 [WRN] 压测兜住的异常[3]：System.OutOfMemoryException: boom\n")
        _d = summarize_internal_error_log(_lg)
        expect(_d["available"] is True and _d["logged_blocks"] == 3,
               f"bots.log 摘要应数出 3 条异常块，实得 {_d.get('logged_blocks')}")
        expect(_d["types"] and _d["types"][0]["type"] == "System.NullReferenceException"
               and _d["types"][0]["count"] == 2,
               "top 类型应为 NRE×2（按次数排序）")
        expect(_d["types"][0]["stack_head"]
               and _d["types"][0]["stack_head"][0].startswith("at BotSrv.Scenes.Scene.PlayScene.BeginScene"),
               "应保留堆栈首行")
        expect([t["type"] for t in _d["types"]][1] == "System.OutOfMemoryException",
               "第二种异常类型（压测兜住的异常）应被统计")
        expect(_d["typed_blocks"] == 3 and _d["untyped_blocks"] == 0,
               "三条块都有异常类型（typed_blocks=3），实得 "
               + str(_d.get("typed_blocks")) + "/" + str(_d.get("untyped_blocks")))
        expect(_d["stacked_blocks"] == 1,
               "只有第一条 bot_round 块带 `   at ` 帧（stacked_blocks=1），实得 "
               + str(_d.get("stacked_blocks")))
        expect(summarize_internal_error_log(os.path.join(_td2, "nope.log"))["available"] is False,
               "bots.log 不存在时不得假装有类型")

        # 6b-2) 卡 openmir2-botsrv-errorstack-diagnosable：四类形态都要能归桶。
        #   ① 修复后 `[ERR] <Type>: <Message>` + `   at ` ⇒ 抽出类型与 stack_head；
        #   ② socket 前缀 ⇒ 抽出类型与堆栈；
        #   ③ 本轮真实形态 `[ERR] Object reference...`（只有消息）⇒ typed_blocks=0 的那一类。
        _lg2 = os.path.join(_td2, "bots-shapes.log")
        with open(_lg2, "w", encoding="utf-8", newline=chr(10)) as _f:
            _f.write("19:58:21.075 [ERR] System.NullReferenceException: "
                     "Object reference not set to an instance of an object.\n")
            _f.write("   at BotSrv.ClientManager.ProcessOnePacket(RecvicePacket message)\n")
            _f.write("   at BotSrv.ClientManager.<>c.<RunPacketGuard>b__0_0()\n")
            _f.write("19:58:22.000 [WRN] socket 事件处理器异常被兜住："
                     "System.Net.Sockets.SocketException: reset\n")
            _f.write("   at BotSrv.ScoketClient.OnReceivedData()\n")
            _f.write("19:58:42.344 [ERR] Object reference not set to an instance of an object.\n")
        _d2 = summarize_internal_error_log(_lg2)
        expect(_d2["logged_blocks"] == 3,
               "三行异常块都要被认出（含只有消息的 [ERR]），实得 " + str(_d2.get("logged_blocks")))
        expect(_d2["typed_blocks"] == 2 and _d2["untyped_blocks"] == 1,
               "修复后形态 2 条有类型、真实形态 1 条只有消息，实得 "
               + str(_d2.get("typed_blocks")) + "/" + str(_d2.get("untyped_blocks")))
        expect(_d2["stacked_blocks"] == 2,
               "两条带 `   at ` 帧（[ERR] 修复后形态 + socket），实得 "
               + str(_d2.get("stacked_blocks")))
        _nre2 = next(t for t in _d2["types"] if t["type"] == "System.NullReferenceException")
        expect(_nre2["category"] == "err" and _nre2["stack_head"]
               and _nre2["stack_head"][0].startswith("at BotSrv.ClientManager.ProcessOnePacket"),
               "`[ERR] <Type>: <msg>` + 堆栈必须抽出类型与 stack_head")
        _sock2 = next(t for t in _d2["types"] if t["category"] == "socket")
        expect(_sock2["type"] == "System.Net.Sockets.SocketException" and _sock2["stack_head"],
               "socket 前缀必须抽出类型与堆栈")
        _un2 = next(t for t in _d2["types"] if t["type"] is None)
        expect(_un2["category"] == "err"
               and _un2["message"].startswith("Object reference not set"),
               "只有消息的 [ERR] 必须归到 err 且 type=None（不可诊断）")
        expect(_d2["typed_stacked_blocks"] == 2,
               "同行 [ERR] 与 socket 两块同块具备类型+堆栈（typed_stacked_blocks=2），实得 "
               + str(_d2.get("typed_stacked_blocks")))

        # 6b-3) R1 返工：真实模板形态 = 「消息行 + 类型行 + `   at ` 行」。
        #   AppServer.cs 的 outputTemplate 是 `{Message:lj}{NewLine}{Exception}`，
        #   所以 `[ERR] <msg>` 后的下一非栈行才是类型行，且不带 `[ERR]` 前缀。
        #   旧实现在类型行清 cur ⇒ type=null/stacked=0 的假红（P1）。
        _lg3 = os.path.join(_td2, "bots-real-shape.log")
        with open(_lg3, "w", encoding="utf-8", newline=chr(10)) as _f:
            _f.write("19:58:21.075 [ERR] Object reference not set to an instance of an object.\n")
            _f.write("System.NullReferenceException: Object reference not set to an instance of an object.\n")
            _f.write("   at OpenMir2.BotSrv.ClientManager.RunPacketGuard(System.Action action) "
                     "in /src/BotSrv/ClientManager.cs:line 96\n")
            _f.write("   at OpenMir2.BotSrv.ClientManager.ProcessOnePacket(RecvicePacket message)\n")
            _f.write("19:58:22.000 [INF] 机器人:[loadbot1] 跑向目标怪物（下一条日志）\n")
        _d3 = summarize_internal_error_log(_lg3)
        expect(_d3["logged_blocks"] == 1,
               "真实形态只算 1 个块（下一条时间戳行必须结束块），实得 "
               + str(_d3.get("logged_blocks")))
        expect(_d3["typed_blocks"] == 1 and _d3["untyped_blocks"] == 0,
               "类型行（下一非栈行）必须并入同一块，实得 typed/untyped="
               + str(_d3.get("typed_blocks")) + "/" + str(_d3.get("untyped_blocks")))
        expect(_d3["stacked_blocks"] == 1 and _d3["typed_stacked_blocks"] == 1,
               "同块必须具备类型+堆栈（stacked/typed_stacked=1/1），实得 "
               + str(_d3.get("stacked_blocks")) + "/" + str(_d3.get("typed_stacked_blocks")))
        _nre3 = _d3["types"][0]
        expect(_nre3["type"] == "System.NullReferenceException"
               and _nre3["message"].startswith("Object reference not set")
               and _nre3["stack_head"]
               and _nre3["stack_head"][0].startswith("at OpenMir2.BotSrv.ClientManager.RunPacketGuard"),
               "真实形态必须抽出类型/消息/stack_head，实得 " + repr(_nre3))

        # 6b-4) meta 牙：真实形态绑定 AppServer.cs 的 outputTemplate 字面量。
        #   改模板（删 {NewLine}/{Exception}、调顺序）⇒ 这条必红，合成日志不得与真实模板漂移。
        _expect_tpl = "{Timestamp:HH:mm:ss.fff} [{Level:u3}] {Message:lj}{NewLine}{Exception}"
        expect(os.path.isfile(APP_SERVER_CS_PATH), "AppServer 源文件必须存在：" + APP_SERVER_CS_PATH)
        if os.path.isfile(APP_SERVER_CS_PATH):
            with open(APP_SERVER_CS_PATH, encoding="utf-8-sig") as _af:
                _app_text = _af.read()
            _tpl_m = re.search(r'LogOutputTemplate\s*=\s*"([^"]*)"', _app_text)
            _got_tpl = _tpl_m.group(1) if _tpl_m else None
            expect(_got_tpl == _expect_tpl,
                   "模板字面量已变，请同步 Python 侧期望（真实形态牙绑定它）："
                   "AppServer.LogOutputTemplate=%r，期望=%r" % (_got_tpl, _expect_tpl))
            expect("outputTemplate: LogOutputTemplate" in _app_text,
                   "日志装配未引用 LogOutputTemplate（常量与装配漂移），请先修复 AppServer.cs")
            expect(_expect_tpl.index("{Message:lj}") < _expect_tpl.index("{NewLine}")
                   < _expect_tpl.index("{Exception}"),
                   "模板顺序已变：期望 Message→NewLine→Exception（真实形态三行的前提），"
                   "实际=%r；请同步 Python 侧期望" % _expect_tpl)

    # 6c) 卡 openmir2-botsrv-recv-thread-nre：错误类别普查 —— 枚举表 vs 实现、逐类 0、对账、白名单。
    _census_empty = {"path": "<selftest>", "available": True, "valid": True, "error": None,
                     "sha256": "0" * 64, "entries": [], "entries_count": 0}
    # 6c-0) 跨语言契约：C# BotErrorCensus.cs 的 Order/FeedsInternalErrors 必须与本表逐字一致。
    #        没有真机窗口也能发现的「枚举漂移」；改坏 C# 常量/删 Order 一项都必须报错（负例）。
    expect(os.path.isfile(CENSUS_CS_PATH), "C# 类别表源文件必须存在：" + CENSUS_CS_PATH)
    if os.path.isfile(CENSUS_CS_PATH):
        with open(CENSUS_CS_PATH, encoding="utf-8-sig") as _csf:
            _cs_text = _csf.read()
        _cs_errs = parse_census_contract(_cs_text)
        expect(not _cs_errs, "C#/Python 错误类别表必须逐字一致：" + "; ".join(_cs_errs))
        _cs_bad_name = _cs_text.replace(
            'public const string RecvPacketNull = "recv_packet_null";',
            'public const string RecvPacketNull = "recv_packet_null_typo";')
        expect(_cs_bad_name != _cs_text and bool(parse_census_contract(_cs_bad_name)),
               "契约检查有牙：改坏 C# 类别名必须报错")
        _cs_bad_order = _cs_text.replace("RecvPacketNull, RecvPacketIndex", "RecvPacketNull")
        expect(_cs_bad_order != _cs_text and bool(parse_census_contract(_cs_bad_order)),
               "契约检查有牙：删 Order 一类必须报错")
    t_c_ok = judge(_census_tier(internal=0), DEFAULT_THRESHOLDS, i3,
                   census_allowlist=_census_empty)
    _ev = t_c_ok.get("error_census") or {}
    expect(t_c_ok["verdict"] == "GREEN", "枚举类别全 0 + 对账一致 ⇒ 不得翻红")
    expect(_ev.get("complete") is True, "complete=True 才算普查通过")
    expect(len(_ev.get("categories") or []) == len(ERROR_CENSUS_CATEGORIES),
           "报告必须逐类列出（categories 数 = 枚举表大小）")
    expect(all(r["count"] == 0 for r in _ev.get("categories", []) if r["class"] == "fault"),
           "全 0 用例里 fault 类别计数必须都是 0")
    expect(len(_ev.get("operational") or []) == len(ERROR_CENSUS_OPERATIONAL_STATS),
           "既有事件计数必须逐项出现在报告里（枚举完整）")
    _scene_null = _ev.get("scene_null") or {}
    expect(_scene_null.get("count") == 0
           and _scene_null.get("status") in ("no_rounds", "within_upper_bound"),
           "scene_null_actor_skipped 必须在报告显式列出（0 也列）")
    expect(len(_ev.get("parent_scene") or []) == len(ERROR_CENSUS_SCENE_STATS),
           "父卡 scene 计数必须逐项出现在报告里")
    expect(all(r["present"] for r in _ev.get("parent_scene", [])),
           "父卡四个 scene 字段必须全部接线（缺失 fail-closed）")

    # ① fault 类别非 0 ⇒ RED 且点名类别（不是只数总量）
    t_c_bad = judge(_census_tier(internal=1, census={"recv_packet_null": 1}),
                    DEFAULT_THRESHOLDS, i3, census_allowlist=_census_empty)
    expect(t_c_bad["verdict"] == "RED" and any("recv_packet_null=1" in f for f in t_c_bad["fails"]),
           "recv_packet_null=1 必须判红并点名类别")
    expect("recv_packet_null" in ((t_c_bad.get("error_census") or {}).get("fault_categories_nonzero") or []),
           "报告必须列出非 0 的 fault 类别名")

    # ② 对账：census 1 但 internal_errors=0 ⇒ RED（存在绕过分类入口的计数/漏同步）
    t_c_cons = judge(_census_tier(internal=0, census={"recv_packet_null": 1}),
                     DEFAULT_THRESHOLDS, i3, census_allowlist=_census_empty)
    expect(t_c_cons["verdict"] == "RED" and any("已分类故障之和" in f for f in t_c_cons["fails"]),
           "internal_errors 与分类之和不一致 ⇒ fail-closed")
    _cons_row = next(r for r in (t_c_cons.get("error_census") or {}).get("categories", [])
                     if r["category"] == "recv_packet_null")
    expect(_cons_row["status"] == "nonzero_fault",
           "internal_errors=0 时不得把非 0 的 feeds_internal 类别标成 excused（状态必须诚实）")

    # ③ 缺类别 / 多类别 / 缺整个对象 ⇒ 三项都 RED（枚举漂移可见）
    t_c_miss = judge(_census_tier(drop_cat="recv_packet_null"), DEFAULT_THRESHOLDS, i3,
                     census_allowlist=_census_empty)
    expect(t_c_miss["verdict"] == "RED" and any("缺类别" in f for f in t_c_miss["fails"]),
           "census 缺类别必须 fail-closed")
    t_c_extra = judge(_census_tier(extra_cat="brand_new_fault"), DEFAULT_THRESHOLDS, i3,
                      census_allowlist=_census_empty)
    expect(t_c_extra["verdict"] == "RED" and any("未登记类别" in f for f in t_c_extra["fails"]),
           "census 多出未登记类别必须 fail-closed")
    t_c_none = judge(_census_tier(drop_census=True), DEFAULT_THRESHOLDS, i3,
                     census_allowlist=_census_empty)
    expect(t_c_none["verdict"] == "RED" and any("缺 error_census" in f for f in t_c_none["fails"]),
           "缺 error_census 对象必须 fail-closed（不得当全 0）")

    # ④ operational 类别允许非 0（但必须在报告里逐类出现）
    t_c_op = judge(_census_tier(internal=0, census={"teardown_race": 5}),
                   DEFAULT_THRESHOLDS, i3, census_allowlist=_census_empty)
    _op_row = next(r for r in (t_c_op.get("error_census") or {}).get("categories", [])
                   if r["category"] == "teardown_race")
    expect(t_c_op["verdict"] == "GREEN" and _op_row["count"] == 5
           and _op_row["status"] == "operational",
           "teardown_race 是运行期事件：非 0 不得翻红，但必须列出计数")

    # ⑤ census 白名单：category+max 内不红；超 max / 未登记 ⇒ RED（默认容忍度 0）
    def _cal(cat, mx):
        return {"path": "<selftest>", "available": True, "valid": True, "error": None,
                "sha256": "1" * 64, "entries_count": 1,
                "entries": [{"id": "selftest-" + cat, "category": cat, "max": mx,
                             "reason": "selftest", "owner": "selftest", "review_ref": "selftest"}]}
    t_c_al_ok = judge(_census_tier(internal=0, census={"metric_flush_failure": 1}),
                      DEFAULT_THRESHOLDS, i3, census_allowlist=_cal("metric_flush_failure", 1))
    expect(t_c_al_ok["verdict"] == "GREEN", "census 白名单内 1/1 不得翻红")
    _al_row = next(r for r in (t_c_al_ok.get("error_census") or {}).get("categories", [])
                   if r["category"] == "metric_flush_failure")
    expect(_al_row["status"] == "excused_by_census_allowlist" and _al_row["allow_max"] == 1,
           "census 白名单命中必须列出 category/count/max")
    t_c_al_over = judge(_census_tier(internal=0, census={"metric_flush_failure": 2}),
                        DEFAULT_THRESHOLDS, i3, census_allowlist=_cal("metric_flush_failure", 1))
    expect(t_c_al_over["verdict"] == "RED" and any("超过 census 白名单" in f
                                                   for f in t_c_al_over["fails"]),
           "census 白名单内 2 > max 1 必须 RED")
    _bad_cal = {"path": "<selftest>", "available": False, "valid": True, "error": "missing",
                "sha256": None, "entries": [], "entries_count": 0}
    t_c_al_missing = judge(_census_tier(internal=0, census={"metric_flush_failure": 1}),
                           DEFAULT_THRESHOLDS, i3, census_allowlist=_bad_cal)
    expect(t_c_al_missing["verdict"] == "RED", "census 白名单缺文件/为空 ⇒ 容忍度 0")

    # ⑥ 既有事件计数缺失 ⇒ RED（枚举不完整，不得静默跳过）
    _t_opmiss = _census_tier(internal=0)
    _t_opmiss["error_census_operational_present"]["conn_lost"] = False
    _t_opmiss["error_census_operational_counts"]["conn_lost"] = None
    t_c_opmiss = judge(_t_opmiss, DEFAULT_THRESHOLDS, i3, census_allowlist=_census_empty)
    expect(t_c_opmiss["verdict"] == "RED"
           and any("缺既有事件计数 conn_lost" in f for f in t_c_opmiss["fails"]),
           "load_stats 缺既有事件计数必须 fail-closed")

    # ⑥b) diagnosable 维度（卡 `openmir2-botsrv-errorstack-diagnosable`）：
    #      有 internal_errors 计数但摘要素不出类型/堆栈 ⇒ 必须判红并显式进报告。
    _diag_untyped = {"available": True, "logged_blocks": 4, "typed_blocks": 0,
                     "untyped_blocks": 4, "stacked_blocks": 0, "typed_stacked_blocks": 0,
                     "categories": {"err": 4}, "types": []}
    _t_du = _census_tier(internal=4, census={"recv_packet_null": 4})
    _t_du["internal_error_diag"] = _diag_untyped
    _t_du = judge(_t_du, DEFAULT_THRESHOLDS, i3, census_allowlist=_census_empty)
    _du = (_t_du.get("error_census") or {}).get("diagnosability") or {}
    expect(_du.get("diagnosable") is False and _du.get("status") == "untyped_blocks",
           "有计数但只有消息（本轮真实形态）⇒ diagnosable=false/untyped_blocks，实得 "
           + str(_du.get("status")))
    expect(any("不可诊断" in f for f in _t_du["fails"]),
           "不可诊断必须进 fails（fail-closed），fails=" + repr(_t_du["fails"]))

    _diag_ok = {"available": True, "logged_blocks": 4, "typed_blocks": 4,
                "untyped_blocks": 0, "stacked_blocks": 4, "typed_stacked_blocks": 4,
                "categories": {"err": 4},
                "types": [{"category": "err", "type": "System.NullReferenceException",
                           "message": "Object reference not set", "count": 4,
                           "stack_head": ["at BotSrv.ClientManager.ProcessOnePacket(...)"]}]}
    _t_dok = _census_tier(internal=4, census={"recv_packet_null": 4})
    _t_dok["internal_error_diag"] = _diag_ok
    _t_dok = judge(_t_dok, DEFAULT_THRESHOLDS, i3, census_allowlist=_census_empty)
    _dok = (_t_dok.get("error_census") or {}).get("diagnosability") or {}
    expect(_dok.get("diagnosable") is True and _dok.get("status") == "ok",
           "类型+堆栈齐 ⇒ diagnosable=true/ok，实得 " + str(_dok.get("status")))
    expect(not any("不可诊断" in f for f in _t_dok["fails"]),
           "可诊断时不得新增诊断类 fails")

    # 有类型但无堆栈（如只有 `压测兜住的异常[N]`）⇒ 知道是什么但不知道在哪。
    _diag_nostack = dict(_diag_ok, stacked_blocks=0, typed_stacked_blocks=0)
    _t_dns = _census_tier(internal=4, census={"recv_packet_null": 4})
    _t_dns["internal_error_diag"] = _diag_nostack
    _t_dns = judge(_t_dns, DEFAULT_THRESHOLDS, i3, census_allowlist=_census_empty)
    _dns = (_t_dns.get("error_census") or {}).get("diagnosability") or {}
    expect(_dns.get("diagnosable") is False and _dns.get("status") == "no_stack",
           "有类型无堆栈（无法定位）⇒ diagnosable=false/no_stack")

    # R2① 同块要求：有类型块 + 有堆栈块，但没有一块**同块**具备 ⇒ cross_block_only 必红。
    _diag_cross = dict(_diag_ok, stacked_blocks=4, typed_stacked_blocks=0)
    _t_dcross = _census_tier(internal=4, census={"recv_packet_null": 4})
    _t_dcross["internal_error_diag"] = _diag_cross
    _t_dcross = judge(_t_dcross, DEFAULT_THRESHOLDS, i3, census_allowlist=_census_empty)
    _dcross = (_t_dcross.get("error_census") or {}).get("diagnosability") or {}
    expect(_dcross.get("diagnosable") is False and _dcross.get("status") == "cross_block_only",
           "跨块拼凑（类型/堆栈不在同一块）⇒ diagnosable=false/cross_block_only，实得 "
           + str(_dcross.get("status")))
    expect(any("跨块拼凑" in f for f in _t_dcross["fails"]),
           "跨块拼凑必须进 fails（R2① fail-closed）")

    # R2② 未类型化块：即使有同块证据，只要还有一个未类型化块 ⇒ mixed_blocks 必红。
    _diag_mixed = dict(_diag_ok, logged_blocks=4, typed_blocks=3, untyped_blocks=1,
                       stacked_blocks=4, typed_stacked_blocks=3)
    _t_dmix = _census_tier(internal=4, census={"recv_packet_null": 4})
    _t_dmix["internal_error_diag"] = _diag_mixed
    _t_dmix = judge(_t_dmix, DEFAULT_THRESHOLDS, i3, census_allowlist=_census_empty)
    _dmix = (_t_dmix.get("error_census") or {}).get("diagnosability") or {}
    expect(_dmix.get("diagnosable") is False and _dmix.get("status") == "mixed_blocks",
           "有 1 个未类型化块 ⇒ diagnosable=false/mixed_blocks，实得 " + str(_dmix.get("status")))
    expect(any("未类型化" in f for f in _t_dmix["fails"]),
           "未类型化块必须进 fails（R2② fail-closed）")

    # R2① 端到端：A 块有类型无堆栈、B 块有堆栈无类型 ⇒ typed_stacked_blocks 必须为 0
    #          （按块统计，不得把两个半缺块拼成 1 个同块证据）。
    with tempfile.TemporaryDirectory() as _tdc:
        _lc = os.path.join(_tdc, "cross-block.log")
        with open(_lc, "w", encoding="utf-8", newline=chr(10)) as _f:
            _f.write("19:58:20.000 [WRN] 压测兜住的异常[1]：System.OutOfMemoryException: boom\n")
            _f.write("19:58:21.075 [ERR] Object reference not set to an instance of an object.\n")
            _f.write("   at BotSrv.ClientManager.ProcessOnePacket(RecvicePacket message)\n")
        _dc = summarize_internal_error_log(_lc)
        expect(_dc["typed_blocks"] == 1 and _dc["stacked_blocks"] == 1
               and _dc["typed_stacked_blocks"] == 0,
               "跨块不得拼凑：typed/stacked/typed_stacked 应为 1/1/0，实得 "
               + str(_dc.get("typed_blocks")) + "/" + str(_dc.get("stacked_blocks"))
               + "/" + str(_dc.get("typed_stacked_blocks")))

    # R2 端到端：真实摘要里同时有 typed（run_loop）与未类型化（[ERR] 只有消息）⇒ mixed_blocks。
    with tempfile.TemporaryDirectory() as _tdm:
        _lm = os.path.join(_tdm, "mixed.log")
        with open(_lm, "w", encoding="utf-8", newline=chr(10)) as _f:
            _f.write("19:58:20.000 [WRN] 压测兜住的异常[1]：System.OutOfMemoryException: boom\n")
            _f.write("19:58:21.075 [ERR] Object reference not set to an instance of an object.\n")
        _dm = summarize_internal_error_log(_lm)
        _evm = evaluate_diagnosability(2, _dm)
        expect(_dm["typed_blocks"] == 1 and _dm["untyped_blocks"] == 1
               and _evm["diagnosable"] is False and _evm["status"] == "mixed_blocks",
               "真实形态 mixed（1 typed + 1 untyped）⇒ mixed_blocks，实得 "
               + str(_dm.get("typed_blocks")) + "/" + str(_dm.get("untyped_blocks"))
               + "/" + str(_evm.get("status")))

    # 空输入：internal_errors=0 / 无日志 ⇒ 不误报（diagnosable=None，不新增 fail）。
    _t_dz = _census_tier(internal=0)
    _t_dz["internal_error_diag"] = {"available": False, "reason": "empty"}
    _t_dz = judge(_t_dz, DEFAULT_THRESHOLDS, i3, census_allowlist=_census_empty)
    _dz = (_t_dz.get("error_census") or {}).get("diagnosability") or {}
    expect(_t_dz["verdict"] == "GREEN" and _dz.get("diagnosable") is None
           and not _dz.get("fails"),
           "internal_errors=0 + 空日志 ⇒ diagnosable 不误报（None、无新 fail）")

    # 采样上限不是可诊断性判据：internal_errors 远大于 logged_blocks，但样本带类型+堆栈
    # ⇒ diagnosable=true，sampled_only=true（否则 3.95 万条窗口会因采样上限永久假红）。
    _t_cap = _census_tier(internal=39500, census={"recv_packet_null": 39500})
    _t_cap["internal_error_diag"] = dict(_diag_ok, logged_blocks=20, typed_blocks=20,
                                         stacked_blocks=20, typed_stacked_blocks=20)
    _t_cap = judge(_t_cap, DEFAULT_THRESHOLDS, i3, census_allowlist=_census_empty)
    _dcap = (_t_cap.get("error_census") or {}).get("diagnosability") or {}
    expect(_dcap.get("diagnosable") is True and _dcap.get("sampled_only") is True,
           "样本可诊断但总量更大 ⇒ diagnosable=true + sampled_only=true")

    # ⑦ 父卡场景计数（master 39d418816 已接线）：每份报告必列 scene_null；上界 <0.5%；list/index 必须 0。
    t_scene_zero = judge(_census_tier(scene_null=0, scene_rounds=1000),
                         DEFAULT_THRESHOLDS, i3, census_allowlist=_census_empty)
    _s0 = (t_scene_zero.get("error_census") or {}).get("scene_null") or {}
    expect(t_scene_zero["verdict"] == "GREEN" and _s0.get("count") == 0
           and _s0.get("status") == "within_upper_bound",
           "scene_null=0 必须 GREEN 且报告显式列出 0（不许不提）")
    t_scene_low = judge(_census_tier(scene_null=3, scene_rounds=1000),
                        DEFAULT_THRESHOLDS, i3, census_allowlist=_census_empty)
    _s3 = (t_scene_low.get("error_census") or {}).get("scene_null") or {}
    expect(t_scene_low["verdict"] == "GREEN" and _s3.get("pct_steady") == 0.3
           and _s3.get("status") == "within_upper_bound",
           "稳态 3/1000 = 0.3% < 0.5% ⇒ GREEN 且列出占比")
    # 恰好 0.5%：判据是严格 <，所以 0.5% 已超界，必须单独解释
    t_scene_edge = judge(_census_tier(scene_null=5, scene_rounds=1000),
                         DEFAULT_THRESHOLDS, i3, census_allowlist=_census_empty)
    expect(t_scene_edge["verdict"] == "RED"
           and any("需单独解释" in f for f in t_scene_edge["fails"]),
           "稳态 5/1000 = 0.5% 不满足 <0.5% ⇒ 必须 RED（需单独解释）")
    t_scene_edge_x = judge(_census_tier(scene_null=5, scene_rounds=1000),
                           DEFAULT_THRESHOLDS, i3, census_allowlist=_census_empty,
                           scene_null_explanation="已知且受限：并发清理尾槽的瞬时空位")
    _s5x = (t_scene_edge_x.get("error_census") or {}).get("scene_null") or {}
    expect(t_scene_edge_x["verdict"] == "GREEN"
           and _s5x.get("status") == "over_upper_bound_explained"
           and "并发清理尾槽" in (_s5x.get("explanation") or ""),
           "超界但给了单独解释 ⇒ 不判红，且解释进报告")
    t_scene_high = judge(_census_tier(scene_null=6, scene_rounds=1000),
                         DEFAULT_THRESHOLDS, i3, census_allowlist=_census_empty)
    expect(t_scene_high["verdict"] == "RED",
           "稳态 6/1000 = 0.6% 且无解释 ⇒ RED")
    t_scene_list = judge(_census_tier(scene_list=1), DEFAULT_THRESHOLDS, i3,
                         census_allowlist=_census_empty)
    expect(t_scene_list["verdict"] == "RED"
           and any("scene_list_race_skipped=1" in f for f in t_scene_list["fails"]),
           "scene_list_race_skipped > 0（护栏触发竞态）⇒ RED")
    t_scene_index = judge(_census_tier(scene_index=1), DEFAULT_THRESHOLDS, i3,
                          census_allowlist=_census_empty)
    expect(t_scene_index["verdict"] == "RED"
           and any("scene_index_race_skipped=1" in f for f in t_scene_index["fails"]),
           "scene_index_race_skipped > 0（护栏触发竞态）⇒ RED")
    t_scene_miss = judge(_census_tier(drop_scene="scene_rounds"), DEFAULT_THRESHOLDS, i3,
                         census_allowlist=_census_empty)
    expect(t_scene_miss["verdict"] == "RED"
           and any("缺 scene 计数 scene_rounds" in f for f in t_scene_miss["fails"]),
           "缺 scene_rounds（上界分母）⇒ fail-closed")
    t_scene_norounds = judge(_census_tier(scene_null=0, scene_rounds=0),
                             DEFAULT_THRESHOLDS, i3, census_allowlist=_census_empty)
    _snr = (t_scene_norounds.get("error_census") or {}).get("scene_null") or {}
    expect(t_scene_norounds["verdict"] == "GREEN" and _snr.get("status") == "no_rounds",
           "窗口内 0 轮且 0 跳过 ⇒ 不判红，但状态必须写 no_rounds")

    # 7) A2（2026-11-15）：BotSrv 崩溃判据 —— 伪造日志/退出码/事件 ⇒ 必红；门禁自停不算崩溃。
    rt_clean = {"self_exited": False, "stopped_by_gate": True, "exit_code": -1,
                "err_log_oom": False, "app_error_events": [], "oom_hits": 0}
    t_rt_ok = judge(summarize_tick(stats_ie_ok, 100), DEFAULT_THRESHOLDS, i3, bots_runtime=rt_clean)
    expect(t_rt_ok["verdict"] == "GREEN",
           "门禁 Stop-Process 的退出码（stopped_by_gate=true）不得当崩溃")
    rt_oom = {"self_exited": False, "exit_code": None, "err_log_oom": True,
              "err_log_oom_lines": ["Out of memory."], "app_error_events": []}
    t_oom = judge(summarize_tick(stats_ie_ok, 100), DEFAULT_THRESHOLDS, i3, bots_runtime=rt_oom)
    expect(t_oom["verdict"] == "RED" and any("Out of memory" in f for f in t_oom["fails"]),
           "伪造 bots.err.log 'Out of memory.' ⇒ 必须判红")
    rt_exit = {"self_exited": True, "exit_code": 3762504530, "exit_code_hex": "0xE0434352",
               "err_log_oom": False, "app_error_events": []}
    t_exit = judge(summarize_tick(stats_ie_ok, 100), DEFAULT_THRESHOLDS, i3, bots_runtime=rt_exit)
    expect(t_exit["verdict"] == "RED" and any("非 0 退出" in f for f in t_exit["fails"]),
           "伪造非 0 退出（0xE0434352）⇒ 必须判红")
    t_exit0 = judge(summarize_tick(stats_ie_ok, 100), DEFAULT_THRESHOLDS, i3,
                    bots_runtime={"self_exited": True, "exit_code": 0,
                                  "err_log_oom": False, "app_error_events": []})
    expect(t_exit0["verdict"] == "GREEN", "进程自行以 0 退出不得判崩溃")
    t_exit_none = judge(summarize_tick(stats_ie_ok, 100), DEFAULT_THRESHOLDS, i3,
                        bots_runtime={"self_exited": True, "exit_code": None,
                                      "err_log_oom": False, "app_error_events": []})
    expect(t_exit_none["verdict"] == "RED" and any("退出码不可读" in f for f in t_exit_none["fails"]),
           "自行退出但退出码不可读 ⇒ fail-closed")
    rt_app = {"self_exited": False, "exit_code": None, "err_log_oom": False,
              "app_error_events": [{"id": 1000, "code": "0xe0434352"}]}
    t_app = judge(summarize_tick(stats_ie_ok, 100), DEFAULT_THRESHOLDS, i3, bots_runtime=rt_app)
    expect(t_app["verdict"] == "RED" and any("0xe0434352" in f for f in t_app["fails"]),
           "伪造 Windows Application Error 0xe0434352 ⇒ 必须判红")

    # 8) A4（2026-11-15）：窗口前提只做结论准入，不改 verdict（否则静默改变 M1⑤/冒烟口径）。
    _tw = summarize_tick(stats_ie_ok, 100)
    _tw["hold_sec"] = 60
    _tw = judge(_tw, DEFAULT_THRESHOLDS, i3)
    expect(_tw["verdict"] == "GREEN" and _tw["window"]["conclusion_valid"] is False
           and _tw["window"]["hold_sec_required"] == 360,
           "HoldSec=60 时测量 verdict 仍 GREEN，但 conclusion_valid 必须 false")
    _tw2 = summarize_tick(stats_ie_ok, 100)
    _tw2["hold_sec"] = 360
    _tw2 = judge(_tw2, DEFAULT_THRESHOLDS, i3)
    expect(_tw2["window"]["conclusion_valid"] is True and _tw2["window"]["conclusion_invalid_reason"] == "",
           "HoldSec=360 时 conclusion_valid 必须 true")

    # 9) C4 前置（2026-11-23）：(proc,pid) 分组 + 各分片/合计两视图；分片求和后再算比率。
    # 9a) mem：两个 BotSvr 进程（pid 1001/1002）各一路采样 ⇒ by_instance 两键、by_proc_sum 峰值=同刻求和。
    _mem_shard = []
    for _pid, _base in ((1001, 1000.0), (1002, 1200.0)):
        for _k in range(3):
            _mem_shard.append({"proc": "BotSvr", "pid": _pid, "t_ms": _k * 1000,
                               "ws_mb": _base + _k * 10, "priv_mb": _base + _k * 10, "cpu_s": 0})
    _mi = summarize_mem_instances(_mem_shard)
    expect(set(_mi["by_instance"]) == {"BotSvr#1001", "BotSvr#1002"},
           "mem 实例必须按 (proc,pid) 分组，实得 " + str(sorted(_mi["by_instance"])))
    expect(_mi["by_proc_sum"]["BotSvr"]["ws_peak_mb"] == 2240.0,
           "合计峰值 = 各实例同刻求和（2200/2220/2240 ⇒ 2240），实得 "
           + str(_mi["by_proc_sum"]["BotSvr"]["ws_peak_mb"]))
    _m1 = summarize_mem([{"proc": "BotSvr", "pid": 7, "t_ms": 0, "ws_mb": 100, "priv_mb": 100},
                         {"proc": "BotSvr", "pid": 7, "t_ms": 1000, "ws_mb": 120, "priv_mb": 120}])
    expect(_m1["BotSvr"]["ws_peak_mb"] == 120.0, "单 pid 的 summarize_mem 输出必须不变（向后兼容）")

    # 9b) stats：A=100/100、B=150/300 ⇒ 求和=250/400=62.5%（若取比率平均会是 75%）⇒ 证明「先求和再算比率」。
    def _shard_stats(_ok, _nrows=3):
        return [{"t_ms": _i * 1000, "login_ok": (_ok if _i >= 1 else 0),
                 "in_world": (_ok if _i >= 1 else 0),
                 "internal_errors": (1 if _i == _nrows - 1 else 0),
                 "hist": ([0, 1] if _i == _nrows - 1 else [0, 0]),
                 "tick_samples": (1 if _i == _nrows - 1 else 0),
                 "tick_sum_ms": (_ok if _i == _nrows - 1 else 0),
                 "tick_max_ms": (_ok if _i == _nrows - 1 else 0),
                 "hist_bucket_ms": 1, "error_census_schema": "x",
                 "ident_counts": {"a": _ok}} for _i in range(_nrows)]
    _sA = _shard_stats(100)
    _sB = _shard_stats(150)
    _merged = merge_shard_stats([_sA, _sB])
    _t = summarize_tick(_merged, 400)  # 目标总数 = 100+300
    expect(_t["peak_login_ok"] == 250 and _t["login_success_pct"] == round(100.0 * 250 / 400, 2),
           "分片 login 必须『求和后再算比率』（250/400=62.5%，非 75%），实得 "
           + str(_t.get("login_success_pct")))
    expect(_t["world_ok"] == 250, "分片 world_ok 必须求和，实得 " + str(_t.get("world_ok")))
    expect(_t["internal_errors"] == 2, "分片 internal_errors 必须求和，实得 " + str(_t.get("internal_errors")))
    expect(_t["mean_ms"] == round(250 / 2, 2),
           "分片 tick mean = 求和 tick_sum_ms / 求和 tick_samples（250/2），实得 " + str(_t.get("mean_ms")))
    expect(merge_shard_stats([_sA]) is _sA, "单分片 merge 必须原样返回（向后兼容）")

    if not ok:
        print("RED: load_report selftest 有未通过项")
        return 1
    print("GREEN: load_report selftest 全过（分位数/尾部敏感/登录阈值牙齿/在线判据牙齿/掉线分窗/内存斜率/"
          "残余丢包方向/本机负载窗/internal_errors 阈值与 fail-closed/白名单四牙/错误类别普查逐类与对账/"
          "异常块四形态/真实模板跨行/diagnosable R2（无类型/混合/跨块必红）/"
          "scene_null 0.5% 上界与必列/BotSrv 崩溃三项/窗口准入）")
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
                 f"login_threshold_pct）/ 稳态掉线 0 / tick P99 ≤ 200ms（FAIL；>100ms 记 WARN）/ tick P50 ≤ 100ms")
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
    lines.append("> **判据更新（2026-11-22 总监裁决）**：tick P99 的 FAIL 线 = **200ms（Owner 验收线）**；>100ms 记 **WARN**；P50 ≤ 100ms 硬门。")
    lines.append("> C# 参考栈 1000 档实测 125/201ms（在共享负载窗口内，归属待空载复测）；待 Rust 栈接管后（M5 灰度）复测。")
    lines.append("> 另：`in_world` 实测只有 ~10%（tier-500/1000 稳态 15~26），**不得**用这三档宣称「M5 三档：1000 假人在线」；")
    lines.append("> 「假人真正进世界」是 M5 收尾缺口（归宿：卡 `openmir2-botsrv-world-evidence`）。")
    lines.append("")
    # 曲线 1：tick P99 / P50 / max vs 档位
    pts_p99 = [(t["count"], t["tick_p99_ms"] or 0) for t in sorted(reports, key=lambda r: r.get("count", 0))]
    pts_p50 = [(t["count"], t["tick_p50_ms"] or 0) for t in sorted(reports, key=lambda r: r.get("count", 0))]
    pts_mx = [(t["count"], t.get("max_ms") or 0) for t in sorted(reports, key=lambda r: r.get("count", 0))]
    lines.append("## 曲线 1：tick 服务时延 vs 假人数（目标 P99 ≤ 200ms；>100ms 为 WARN）")
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
    s.add_argument("--stats", required=True, action="append",
                   help="BotSrv 落的 load_stats.ndjson；分片时可重复传多份（按行求和后再算比率）")
    s.add_argument("--mem", default="", help="内存采样 mem.ndjson（可选）")
    s.add_argument("--mem-tail-sec", type=int, default=60,
                   help="内存末段斜率窗口（秒）；默认 60（不传 ⇒ 与既有报告口径逐字段一致）。"
                        "只影响 slope_tail_mb_per_s / tail_window_s")
    s.add_argument("--mem-extra-names", default="",
                   help="只采样落盘、不参与越界判定的额外进程名（逗号/空白分隔）；"
                        "写入 report.mem_proc_names_extra（实际有行的名字另记 mem_proc_names_extra_present）")
    s.add_argument("--count", type=int, required=True, help="目标假人数（档位）")
    s.add_argument("--stagger-ms", type=int, default=0)
    s.add_argument("--hold-sec", type=int, default=0)
    s.add_argument("--out", default="")
    s.add_argument("--login-conns", default="",
                   help="BotSrv 落的 login_conns.ndjson（T-11 残余丢包方向归算；缺省不产出该块）")
    s.add_argument("--hostload", default="",
                   help="本机负载探针 hostload.ndjson（owner 裁决 82f8e0c3 ④；缺省不产出该块）")
    s.add_argument("--criteria", default="login,conn,tick,internal",
                   help="判据子集（默认含 internal_errors）；M1 ⑤ 那种只比登录/连接的场景用 login,conn。"
                        "压测结论必须含 internal（A1，2026-11-15）")
    s.add_argument("--allowlist", default=DEFAULT_ALLOWLIST_PATH,
                   help="internal_errors 白名单（默认仓库内 tools/botload/internal-errors-allowlist.json）。"
                        "文件缺失/为空/条目非法 ⇒ 容忍度 0；白名单随卡面 patch 评审，运行时不得改")
    s.add_argument("--census-allowlist", default=DEFAULT_CENSUS_ALLOWLIST_PATH,
                   help="错误类别普查白名单（默认 tools/botload/error-census-allowlist.json）。"
                        "fault 类别默认容忍度 0；feeds_internal 类别也可由 internal-errors 白名单整体豁免。"
                        "文件缺失/为空 ⇒ 容忍度 0（卡 openmir2-botsrv-recv-thread-nre）")
    s.add_argument("--scene-null-upper-bound-pct", type=float, default=SCENE_NULL_UPPER_BOUND_PCT,
                   help="scene_null_actor_skipped 的稳态上界（%% of BeginScene rounds；默认 0.5）。"
                        "超界时必须用 --scene-null-explanation 单独解释，否则 RED")
    s.add_argument("--scene-null-explanation", default="",
                   help="scene_null_actor_skipped 超过上界时的单独解释（写进报告；空 = 不接受超界）")
    s.add_argument("--bots-log", default="",
                   help="BotSrv 的 bots.log（A1：归算 internal_errors 的 top 异常类型/堆栈首行；可选）")
    s.add_argument("--bots-runtime", default="",
                   help="BotSrv 运行时证据 bots_runtime.json（A2：退出码/OOM/Application Error；可选）")
    s.add_argument("--login-threshold", type=float, default=None,
                   help="登录成功率阈值（%）；缺省按档位取（tier>=1000 为 95%，其余 100%）")
    s.add_argument("--allow-missing-tick", action="store_true",
                   help="显式放行「本次没有 tick 样本」（报告里会写入 missing_tick_note）；"
                        "不加则缺样本一律判 RED，且 --criteria 不能替代本开关")
    s.add_argument("--world-threshold", type=float, default=None,
                   help="在线（本会话收到 SM_LOGON 的假人数）占比阈值（%）；缺省不判（T-12）。"
                        "bot-smoke -RequireWorld 用 100，用于证明「抹掉某假人的进世界证据 ⇒ 判据必红」")
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
