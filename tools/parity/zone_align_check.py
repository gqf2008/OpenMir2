#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""zone_align_check.py — 两区 vs N=1 golden 的分类后序列比较器（M1a 验收装置）。

判定逻辑（写死，供审）：
- 比较键 = sha256(12B 头 + body)（与旁路同口径；头含 recog/param/tag/series，报告另附 ident）。
- 类归属：旁路 `cross-src.jsonl` 按 (socket, sha) 建多重集；对收件人帧按捕获顺序扣减 ⇒ CrossZone；
  扣不到 ⇒ World。同 sha 多事件时扣减顺序是约定，歧义列出、不据此下结论。
- 判据 A（硬，无丢无重）：multiset(测试帧 sha) == multiset(golden帧 sha)。
- 判据 B（只归因）：W/X 计数与 ident 直方图；不与 golden 分片比。
- 判据 C1（硬，可定序对）：若 golden 中 sha a 的全部出现都早于 sha b 的全部出现，测试也必须如此。
- 判据 C2（软）：完整序列逐项相等，仅报告（缺 Sim tick，跨类 +1 合法错位不判红）。
- 判据 D（硬，旁路消费）：该 socket 的旁路行必须全部被消费（unconsumed==0）。
- mutant（模拟真牙，仅用于"牙必须红"的自检）：
  * cross-as-local = 模拟"把跨区帧当本区帧（不 +1）"：不做扣减 ⇒ 旁路未消费 ⇒ D 红；
  * global-plus1   = 模拟"全局统一 +1"：分类按"前一帧"错位匹配 ⇒ 旁路未消费 ⇒ D 红；
  * golden-perturb = 模拟"扰动 N=1 基线"：删 golden 首帧 ⇒ A 红。
  去掉区 k FRAMES/AOI_DOWNLINK 的产品变异牙需要变异构建窗口，不在本脚本模拟范围。
- 角色归一 `--roles gA,gB,tA,tB`：未命中 A/B 的帧（other）不计 A/C1/C2/D，只计数（装置文档 §3）。
- **body 角色归一（窄白名单，2026-11-23 卡 `openmir2-hit-hp-fixture-ineffective` 配套）**：
  31 帧的判定面 = header（recog/param/tag/series）+ body 的 `MaxHP/Status/Mag` + **按角色归一的 Hitter**；
  其它 body 字节原样比较。**只归一 ident=31、且只归一角色（`--roles` 命中）hitter**；未登记 ident
  （638/41/22/23/1104 等）一律原样比较。`MessageBodyWL.Tag1 = 命中者 ActorId` 是身份字段（依据见
  `BODY_ROLE_FIELDS` 注释），
  actor id 随区段变化是构造性的 ⇒ 按 `--roles` 映射成同一角色占位；**两侧映射到不同角色或映射不到 ⇒ 不归一 ⇒ 必红**。
  EDCode 缺失/解码失败 ⇒ `degraded` 标记 + `normalized_frames` 计数 + WRN + **rc=2**（不得静默降级为原样比较）。
  只对登记过的 `(ident, 字段)` 生效，未登记 ident 一律不归一；**禁止**清空/忽略该字段。
  **覆盖边界（A23，与报告 `body_role_norm.surface` 同文）**：仅 ident=31、仅角色 hitter 归一；
  怪物 hitter 的 31 仍 raw；638/41/22/23/1104 未归一（出现且 recog 映射到角色即红）；
  22/23 当前场景未产生（探针侧事实）、1104 无生产调用点。
- **self 桶（卡「区 k MaxExp=0」证据配套）**：`recog==0` 的自我帧（如 `SM_ABILITY(52)`）不在
  `--roles` 映射里，旧口径整帧排除。`--self-bucket` 把该桶按**收件人侧**单独归键比较：
  * `off`（默认）⇒ 与旧版逐项一致（不输出 self_* 字段，rc 不变）；
  * `report` ⇒ 输出 self_* 与红项，**不改 rc**（产品修复前先看形态）；
  * `judge`  ⇒ self 红计入 rc（修复后的正式判据）。
  * 键 = `sha256(f"{side}|{ident}|{param}|{tag}|{series}|{body_hex}")`；`--side A|B` 必须显式给
    （golden A 对 test A、golden B 对 test B）。非 off 模式缺/错 `--side` ⇒ rc=2（fail-closed）。
  * 边界：只纳入 `recog==0`；`recog=-1/1/2000`（系统通告）与实体帧仍在 `other`。
- **gate 死槽桶（`--dead-socket-bucket`，默认关）**：D 判据的窄归因。sidecar 行（同 socket）
  **且**同 socket 的 gate 死槽 WRN（`下发 SM_* 帧时 socket=N 的槽不存在，丢弃 L 字节`，
  字节数一致）两条证据同时命中，才把该行记为 `dropped_at_gate_dead_socket`（不计 unconsumed，
  但计数与逐行明细必须打印）。缺任一证据 ⇒ 仍算 `unconsumed`。**禁止**用 dead-socket 计数
  作为归因条件。启用时需 `--gate-log <本次 test 的 game-svr.out.log>`，缺失/不可读 ⇒ rc=2。
- 自测：`--self-selftest`（离线、合成夹具、临时目录；不读仓库状态）。
- 退出码：0 全过；1 A/C1/D 红（judge 模式下含 self 红）；2 判不了。
"""
import argparse, json, sys, hashlib, collections, os, tempfile, re, struct

_REPO_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
_PROBE = os.environ.get("MIR2_PROBE_DIR", os.path.join(_REPO_ROOT, "tools", "probe"))
if _PROBE not in sys.path:
    sys.path.insert(0, _PROBE)
try:
    from mir2frame import pack_header  # 12B: Recog(i32)|Ident|Param|Tag|Series（小端）
except Exception:
    import struct
    def pack_header(recog, ident, param=0, tag=0, series=0):
        return struct.pack("<iHHHH", recog, ident, param, tag, series)

def sha12(f):
    hdr = pack_header(f["recog"], f["ident"], f["param"], f["tag"], f["series"])
    body = bytes.fromhex(f["body_hex"])
    return hashlib.sha256(hdr + body).hexdigest()

# ---------------------------------------------------------------------------
# **body 角色归一（窄白名单）**：只登记 `(ident, 字段)`，按 `--roles` 把 body 内的身份字段
# （ActorId）映射成角色占位再比较。
#
# 为什么 31 的 Tag1 是身份字段（符号锚 + 原文）：
# - C# `MessageBodyWL`（`src/OpenMir2/Packets/ServerPackets/MessageBodyWL.cs`）：
#   `public int Param1; public int Param2; public int Tag1; public int Tag2;`
#   —— `Tag1` 是**命中者 ActorId**（SM_STRUCK 的 RM_STRUCK 支填 `processMsg.Tag1`）；
# - `PlayObject.Message.cs:1605` 的 `RM_STRUCK` 支把 `wl.Tag1` 当作命中者；
# - 本仓同位置：`sm_struck` / `sm_struck_client`（`bins/game-svr/src/smframe.rs`）的
#   `wl[8..12] = hitter_id`（两种布局都是 16B `MessageBodyWL`）。
# ⇒ golden 的 201 与 test 的 4194327 都映射到角色 P 时，两侧应换成同一占位；映射后仍不等 ⇒ 真差异。
#
# 反面（禁止）：把该字段清空/忽略会把"命中者换成另一个角色"也判绿 —— 自测有双向牙覆盖。
BODY_ROLE_FIELDS = {31: (8, 12)}  # ident -> (start, end) in decoded 16B MessageBodyWL
_ROLE_PLACEHOLDER = {"A": 0x7FFF_0001, "B": 0x7FFF_0002}
_ED_MOD = {}
_BODY_NORM_CACHE = {}


def _load_ed():
    """复用既有 EDCode 实现（LoginSrv 金标准采集器里的 C# EncryptUtil 移植），不新写一套。"""
    if "mod" in _ED_MOD:
        return _ED_MOD["mod"]
    path = os.path.join(_REPO_ROOT, "mir2-rs", "tests", "parity", "loginsrv", "oracle_capture.py")
    mod = None
    if os.path.isfile(path):
        import importlib.util
        try:
            spec = importlib.util.spec_from_file_location("_zone_align_edcode", path)
            m = importlib.util.module_from_spec(spec)
            spec.loader.exec_module(m)
            mod = m
        except Exception:
            mod = None
    _ED_MOD["mod"] = mod
    return mod


def _role_sig(role_map):
    """缓存键必须含 role_map（同一进程内多跑/单测会换角色映射）。"""
    return tuple(sorted(role_map.items()))


def body_role_norm(f, role_map):
    """按窄白名单把 body 身份字段的角色映射成占位。

    返回 `(hex|None, status)`；status ∈ `off`（未给 --roles）/`raw_unregistered`（ident 未登记）/
    `normalized`（已按角色归一）/`raw_unmapped`（actor 不在 --roles）/`degraded`（EDCode 缺失或解码失败，
    hex=None ⇒ 调用方不得把它当"原样比较"用；整跑按口径 rc=2）。
    """
    if not role_map:
        return f["body_hex"], "off"
    span = BODY_ROLE_FIELDS.get(f["ident"])
    if not span:
        return f["body_hex"], "raw_unregistered"
    ck = (f["ident"], f["body_hex"], _role_sig(role_map))
    if ck in _BODY_NORM_CACHE:
        return _BODY_NORM_CACHE[ck]
    ed = _load_ed()
    if ed is None:
        out = (None, "degraded")
        _BODY_NORM_CACHE[ck] = out
        return out
    try:
        raw = ed.edcode_decode(bytes.fromhex(f["body_hex"]))
    except Exception:
        out = (None, "degraded")
        _BODY_NORM_CACHE[ck] = out
        return out
    start, end = span
    if len(raw) < end:
        out = (None, "degraded")
        _BODY_NORM_CACHE[ck] = out
        return out
    actor = int.from_bytes(raw[start:end], "little", signed=False)
    role = role_map.get(actor)
    if role in _ROLE_PLACEHOLDER:
        raw = bytearray(raw)
        raw[start:end] = _ROLE_PLACEHOLDER[role].to_bytes(4, "little")
        out = (ed.edcode_encode(bytes(raw)).hex(), "normalized")
    else:
        out = (f["body_hex"], "raw_unmapped")
    _BODY_NORM_CACHE[ck] = out
    return out


def attach_body_role_norm(frames, role_map):
    """逐帧算一次归一结果并挂到帧上（键/self 键/计数共用同一结果，避免每次 key() 重解）。"""
    counts = collections.Counter()
    for f in frames:
        hx, st = body_role_norm(f, role_map)
        f["_bn_hex"] = hx if hx is not None else f["body_hex"]
        f["_bn_status"] = st
        counts[st] += 1
    return counts

def load_frames(path):
    out = []
    for line in open(path, encoding="utf-8"):
        line = line.strip()
        if not line:
            continue
        f = json.loads(line)
        if f.get("ident") is None:
            continue
        if f.get("conn") not in ("gamegate", "game"):
            continue
        out.append(f)
    return out

def order_violations(G, T):
    """可定序对一致性：golden 中 a 全早于 b ⇒ 测试也必须 a 全早于 b（反之亦然）。"""
    posG = collections.defaultdict(list); posT = collections.defaultdict(list)
    for i, f in enumerate(G): posG[sha12(f)].append(i)
    for i, f in enumerate(T): posT[sha12(f)].append(i)
    keys = list(posG)
    bad = []
    for i in range(len(keys)):
        for j in range(i + 1, len(keys)):
            a, b = keys[i], keys[j]
            if a not in posT or b not in posT:
                continue
            if max(posG[a]) < min(posG[b]):
                if not max(posT[a]) < min(posT[b]): bad.append((a, b))
            elif max(posG[b]) < min(posG[a]):
                if not max(posT[b]) < min(posT[a]): bad.append((b, a))
    return bad

def _self_key(side, f, role_map=None):
    """self 桶键：显式 side 进键，保证 A 侧与 B 侧永不互相配对。"""
    body = f["_bn_hex"] if "_bn_hex" in f else f["body_hex"]
    h = f'{side}|{f["ident"]}|{f["param"]}|{f["tag"]}|{f["series"]}|{body}'
    return hashlib.sha256(h.encode()).hexdigest()

def _self_bucket(G, T, side, role_map=None):
    """`recog==0` 自我帧按收件人侧做多重集比较（G/T 已过 ignore/trim/entry-window）。"""
    sg = [f for f in G if f["recog"] == 0]
    st = [f for f in T if f["recog"] == 0]
    cg = collections.Counter(_self_key(side, f, role_map) for f in sg)
    ct = collections.Counter(_self_key(side, f, role_map) for f in st)
    reds = []
    for k in sorted(set(cg) | set(ct)):
        if cg[k] == ct[k]:
            continue
        sample = next((f for f in sg + st if _self_key(side, f, role_map) == k), None)
        reds.append({
            "key": k[:16], "golden": cg[k], "test": ct[k],
            "ident": sample["ident"] if sample else None,
            "param": sample["param"] if sample else None,
            "tag": sample["tag"] if sample else None,
            "series": sample["series"] if sample else None,
            "body_hex_golden": next((f["body_hex"][:80] for f in sg
                                     if _self_key(side, f, role_map) == k), ""),
            "body_hex_test": next((f["body_hex"][:80] for f in st
                                   if _self_key(side, f, role_map) == k), ""),
        })
    return {"self_frames_golden": len(sg), "self_frames_test": len(st),
            "self_red_count": len(reds), "self_reds": reds}

def _self_args_error(mode, side):
    """self 桶参数校验（fail-closed）。返回错误串或 None。"""
    if mode not in ("off", "report", "judge"):
        return " --self-bucket 非法：%r（只允许 off|report|judge）" % (mode,)
    if side not in ("", "A", "B"):
        return "--side 非法：%r（只允许 A|B）" % (side,)
    if mode != "off" and side not in ("A", "B"):
        return "--self-bucket 非 off 时 --side 必填（A|B）"
    return None

_DEAD_SOCKET_WRN = re.compile(r"下发 SM_\* 帧时 socket=(\d+) 的槽不存在，丢弃 (\d+) 字节")

def _gate_dead_socket_evidence(path, socket):
    """读 gate 日志里的死槽 WRN，返回 Counter{丢弃字节数: 次数}（只收该 socket）。"""
    ev = collections.Counter()
    with open(path, encoding="utf-8", errors="replace") as f:
        for line in f:
            m = _DEAD_SOCKET_WRN.search(line)
            if m and int(m.group(1)) == socket:
                ev[int(m.group(2))] += 1
    return ev

def _dead_bucket_error(bucket, gate_log):
    """`--dead-socket-bucket` 的 fail-closed 校验。返回错误串或 None。"""
    if not bucket:
        return None
    if not gate_log:
        return "--dead-socket-bucket 需要 --gate-log（本次 test 跑的 game-svr.out.log）"
    if not os.path.exists(gate_log):
        return "gate 日志不存在：%s" % gate_log
    return None

def run_core(golden_dir, test_dir, sidecar, socket, mutant="", entry_window_keys="",
             ignore_idents="", trim_tail_ms=0, roles="", self_bucket="off", side="",
             dead_socket_bucket=False, gate_log=""):
    """核心比较：返回 (out_dict, rc)。cli 与 --self-selftest 共用同一实现。"""
    def trim(seq):
        if trim_tail_ms <= 0 or not seq:
            return seq
        last = max(f["t_ms"] for f in seq)
        return [f for f in seq if f["t_ms"] <= last - trim_tail_ms]
    ignore = set()
    if ignore_idents:
        ignore = {int(x) for x in ignore_idents.split(",") if x.strip()}
    G_full = [f for f in load_frames(golden_dir + "/frames.ndjson") if f["ident"] not in ignore]
    T_full = [f for f in load_frames(test_dir + "/frames.ndjson") if f["ident"] not in ignore]
    G = trim(G_full)
    T = trim(T_full)
    side_all = []
    if os.path.exists(sidecar):
        side_all = [json.loads(l) for l in open(sidecar, encoding="utf-8") if l.strip()]
    side_lines = [x for x in side_all if x["socket"] == socket]
    side_counts = collections.Counter(x["sha256"] for x in side_lines)
    pool = collections.Counter(side_counts)

    W = []; X = []
    if mutant == "cross-as-local":
        W = list(T_full)
    elif mutant == "global-plus1":
        for i, f in enumerate(T_full):
            prev = T_full[i - 1] if i > 0 else None
            sh_prev = sha12(prev) if prev else None
            if sh_prev is not None and pool.get(sh_prev, 0) > 0:
                pool[sh_prev] -= 1; X.append(f)
            else:
                W.append(f)
    else:
        for f in T_full:
            sh = sha12(f)
            if pool.get(sh, 0) > 0:
                pool[sh] -= 1; X.append(f)
            else:
                W.append(f)
    unconsumed = sum(pool.values())
    judge_d_red = unconsumed > 0
    # **gate 死槽桶（窄）**：sidecar 行 + 同 socket/同字节数的 gate WRN，两条证据齐全才归因。
    dead_attr = []
    if dead_socket_bucket:
        ev = _gate_dead_socket_evidence(gate_log, socket)
        for sha in sorted(pool):
            if pool[sha] <= 0:
                continue
            lines_for_sha = [x for x in side_lines if x["sha256"] == sha]
            per_len = collections.Counter(x["len"] for x in lines_for_sha)
            for ln in sorted(per_len):
                take = min(pool[sha], per_len[ln], ev.get(ln, 0))
                for _ in range(take):
                    pool[sha] -= 1
                    ev[ln] -= 1
                    hit = next(x for x in lines_for_sha if x["len"] == ln)
                    dead_attr.append({"sha256": sha[:16], "socket": socket,
                                      "len": ln, "tick": hit.get("tick")})
                if pool[sha] <= 0:
                    break
        unconsumed = sum(pool.values())
        judge_d_red = unconsumed > 0

    role_map = {}
    if roles:
        parts = [int(x) for x in roles.split(",")]
        role_map = {parts[0]: "A", parts[1]: "B", parts[2]: "A", parts[3]: "B"}
    bn_counts = collections.Counter()
    if role_map:
        bn_counts.update(attach_body_role_norm(G, role_map))
        bn_counts.update(attach_body_role_norm(T, role_map))
    def key(f):
        if not role_map:
            return sha12(f)
        role = role_map.get(f["recog"])
        if role is None:
            return None
        body = f["_bn_hex"] if "_bn_hex" in f else f["body_hex"]
        hdr = f'{role}|{f["ident"]}|{f["param"]}|{f["tag"]}|{f["series"]}|{body}'
        return hashlib.sha256(hdr.encode()).hexdigest()
    ew_keys = set(k for k in entry_window_keys.split(",") if k.strip())
    ew_g = [f for f in G if key(f) in ew_keys]
    ew_t = [f for f in T if key(f) in ew_keys]
    G = [f for f in G if key(f) not in ew_keys]
    T = [f for f in T if key(f) not in ew_keys]
    other_g = sum(1 for f in G if key(f) is None)
    other_t = sum(1 for f in T if key(f) is None)
    if mutant == "golden-perturb" and G:
        for i, f in enumerate(G):
            if key(f) is not None:
                del G[i]
                break

    mg = collections.Counter(k for f in G if (k := key(f)) is not None)
    mt = collections.Counter(k for f in T if (k := key(f)) is not None)
    reds = []
    for k in set(mg) | set(mt):
        if mg[k] != mt[k]:
            ident = next((f["ident"] for f in T + G if key(f) == k), None)
            reds.append((ident, k[:16], mg[k], mt[k]))
    # ---- C1：只判"两侧都唯一可定类"的对 ----
    # 类定义：该 sha 在**该收件人** sidecar 多重集里出现次数 n_side；帧数 n_frames。
    #   n_side==1 and n_frames==1 ⇒ CrossZone（唯一）；n_side==0 and n_frames==1 ⇒ World（唯一）；
    #   其它 ⇒ undecidable:sha_collision
    frame_counts = collections.Counter(sha12(f) for f in T_full)
    def cls(k):
        ns = side_counts.get(k, 0); nf = frame_counts.get(k, 0)
        if ns == 1 and nf == 1: return "X"
        if ns == 0 and nf == 1: return "W"
        return None
    posG = collections.defaultdict(list); posT = collections.defaultdict(list)
    for i, f in enumerate(G):
        k = key(f)
        if k is not None: posG[k].append(i)
    for i, f in enumerate(T):
        k = key(f)
        if k is not None: posT[k].append(i)
    keys = list(posG)
    c1_legal = 0; c1_viol = []; c1_undec = 0
    for i in range(len(keys)):
        for j in range(i + 1, len(keys)):
            x, y = keys[i], keys[j]
            if x not in posT or y not in posT: continue
            if not (max(posG[x]) < min(posG[y]) or max(posG[y]) < min(posG[x])): continue
            g_order = "xy" if max(posG[x]) < min(posG[y]) else "yx"
            t_order = "xy" if max(posT[x]) < min(posT[y]) else "yx"
            cx, cy = cls(x), cls(y)
            if cx is None or cy is None:
                c1_undec += 1; continue
            if cx != cy:
                c1_legal += 1; continue
            if g_order != t_order:
                c1_viol.append((x, y, g_order, t_order, cx))
            else:
                c1_legal += 1
    bad_order = c1_viol
    seqG = [k for f in G if (k := key(f)) is not None]
    seqT = [k for f in T if (k := key(f)) is not None]

    out = {
        "mutant": mutant or None,
        "golden_frames": len(G), "test_frames": len(T),
        "world_frames": len(W), "crosszone_frames": len(X),
        "sidecar_lines_for_socket": sum(side_counts.values()),
        "sidecar_unconsumed": unconsumed,
        "ambiguous_sha_count": sum(1 for sh, n in side_counts.items() if n > 1),
        "multiset_red_count": len(reds), "multiset_reds": reds[:20],
        "order_red_count": len(bad_order),
        "order_reds": [(x[0][:12], x[1][:12]) for x in bad_order[:20]],
        "c1_legal_shift_count": c1_legal, "c1_violation_count": len(c1_viol),
        "c1_undecidable_sha_collision_count": c1_undec,
        "full_sequence_equal": seqG == seqT,
        "other_frames_golden": other_g, "other_frames_test": other_t,
        "entry_window_golden": len(ew_g), "entry_window_test": len(ew_t),
        "world_idents": dict(collections.Counter(f["ident"] for f in W)),
        "crosszone_idents": dict(collections.Counter(f["ident"] for f in X)),
    }
    if dead_socket_bucket:
        out["dead_socket_bucket"] = True
        out["gate_log"] = gate_log
        out["dead_socket_attributed_count"] = len(dead_attr)
        out["dead_socket_attributed"] = dead_attr[:20]
    # self 桶：additive（`other_frames_*` 仍含 recog=0，self 是它的子集；不改旧字段）。
    self_red = 0
    if self_bucket in ("report", "judge"):
        sb = _self_bucket(G, T, side, role_map)
        out["self_bucket"] = self_bucket
        out["self_side"] = side
        out.update(sb)
        self_red = sb["self_red_count"]
    degraded = bn_counts.get("degraded", 0) > 0
    if degraded:
        print("WRN body_role_norm degraded: 存在 ident=31 帧无法解码（EDCode 缺失或坏 body）⇒ 判不了，按口径 rc=2",
              file=sys.stderr)
    out["body_role_norm"] = {
        "enabled": bool(role_map),
        "degraded": degraded,
        "normalized_frames": bn_counts.get("normalized", 0),
        "raw_unmapped_frames": bn_counts.get("raw_unmapped", 0),
        "raw_unregistered_frames": bn_counts.get("raw_unregistered", 0),
        "fields": [{"ident": i, "offset": [a, b], "name": "MessageBodyWL.Tag1(hitter ActorId)"}
                   for i, (a, b) in BODY_ROLE_FIELDS.items()],
        "surface": ("仅 ident=31、仅角色（--roles 命中）hitter 归一（替换为角色占位）；"
                    "怪物 hitter 的 31 仍 raw（映射不到角色 ⇒ 原样比较，两侧不同即红）；"
                    "未登记 ident（638/41/22/23/1104 等）一律原样比较（出现且 recog 映射到角色即红）；"
                    "22/23 当前场景未产生（探针侧事实，不是比较器过滤）；1104 无生产调用点；"
                    "EDCode 缺失/解码失败 ⇒ degraded + WRN + rc=2"),
    }
    rc = 1 if (reds or bad_order or judge_d_red) else 0
    if self_bucket == "judge" and self_red > 0:
        rc = 1
    if degraded:
        rc = 2
    return out, rc

def _selftest():
    """离线自测：合成夹具 + 临时目录（不读仓库状态）。"""
    checks = []
    def ck(name, cond, detail=""):
        checks.append((name, bool(cond), detail))

    def frame(ident, recog, param=0, tag=0, series=0, body=""):
        return {"seq": 0, "t_ms": 0, "conn": "gamegate", "ident": ident, "recog": recog,
                "param": param, "tag": tag, "series": series,
                "body_len": len(body) // 2, "body_hex": body}

    def write_dir(path, frames):
        os.makedirs(path)
        with open(os.path.join(path, "frames.ndjson"), "w", encoding="utf-8") as f:
            for fr in frames:
                f.write(json.dumps(fr, ensure_ascii=False) + "\n")
        return path

    judged_g = frame(41, 101, param=5, tag=1)
    judged_t = frame(41, 201, param=5, tag=1)
    self_ok = frame(52, 0, param=25344, body="aa00")
    self_ok_t = frame(52, 0, param=25344, body="aa00")
    self_bad_t = frame(52, 0, param=25344, body="aa01")
    roles = "101,102,201,202"

    with tempfile.TemporaryDirectory(prefix="zone-align-selftest-") as tmp:
        def case(name, gframes, tframes, sidecar_rows=None):
            g = write_dir(os.path.join(tmp, name, "golden"), gframes)
            t = write_dir(os.path.join(tmp, name, "test"), tframes)
            sc = os.path.join(tmp, name, "cross-src.jsonl")
            with open(sc, "w", encoding="utf-8") as f:
                for r in (sidecar_rows or []):
                    f.write(json.dumps(r, ensure_ascii=False) + chr(10))
            return g, t, sc

        def gate_log(name, lines):
            p = os.path.join(tmp, name + ".gate.log")
            with open(p, "w", encoding="utf-8") as f:
                for ln in lines:
                    f.write(ln + chr(10))
            return p

        def wrn(sock, ln):
            return ("09:47:37.230 [WRN] 下发 SM_* 帧时 socket=%d 的槽不存在，丢弃 %d 字节"
                    "（该会话后续落空不再逐条告警）" % (sock, ln))

        g, t, sc = case("diff", [judged_g, self_ok], [judged_t, self_bad_t])
        out, rc = run_core(g, t, sc, 1000, roles=roles, self_bucket="off")
        ck("off_ignores_self_diff", rc == 0 and "self_red_count" not in out, f"rc={rc}")
        out, rc = run_core(g, t, sc, 1000, roles=roles, self_bucket="report", side="A")
        # 一字节改动 = 两个 key（golden-only 一条、test-only 一条）⇒ self_red_count=2
        ck("report_surfaces_no_rc", rc == 0 and out.get("self_red_count") == 2
           and out.get("self_frames_golden") == 1 and out.get("self_frames_test") == 1,
           f"rc={rc} red={out.get('self_red_count')}")
        out, rc = run_core(g, t, sc, 1000, roles=roles, self_bucket="judge", side="A")
        ck("judge_reds", rc == 1 and out.get("self_red_count") == 2, f"rc={rc}")

        ge, te, sce = case("equal", [judged_g, self_ok], [judged_t, self_ok_t])
        out, rc = run_core(ge, te, sce, 1000, roles=roles, self_bucket="judge", side="A")
        ck("judge_equal_green", rc == 0 and out.get("self_red_count") == 0
           and out.get("self_frames_golden") == 1, f"rc={rc}")

        g1, t1, sc1 = case("one_side", [judged_g, self_ok], [judged_t])
        out, rc = run_core(g1, t1, sc1, 1000, roles=roles, self_bucket="judge", side="A")
        ck("judge_one_side_red", rc == 1 and out.get("self_frames_test") == 0,
           f"rc={rc} test_frames={out.get('self_frames_test')}")

        g0, t0, sc0 = case("none", [judged_g], [judged_t])
        out, rc = run_core(g0, t0, sc0, 1000, roles=roles, self_bucket="judge", side="A")
        ck("judge_none_ok", rc == 0 and out.get("self_frames_golden") == 0
           and out.get("self_frames_test") == 0, f"rc={rc}")

        ck("side_required", _self_args_error("judge", "") is not None
           and _self_args_error("report", "") is not None
           and _self_args_error("judge", "X") is not None
           and _self_args_error("off", "") is None
           and _self_args_error("report", "A") is None,
           "非 off 缺/错 side ⇒ 错误；off 不要求")
        ck("side_in_key", _self_key("A", self_ok) != _self_key("B", self_ok),
           "A/B 侧键必须不同")

        gz, tz, scz = case("empty", [], [])
        out, rc = run_core(gz, tz, scz, 1000, roles=roles, self_bucket="judge", side="A")
        ck("empty_input_ok", rc == 0 and out.get("self_frames_golden") == 0
           and out.get("self_frames_test") == 0, f"rc={rc}")

        out, rc = run_core(g, t, sc, 1000, roles=roles, self_bucket="judge", side="B")
        ck("side_b_works", rc == 1 and out.get("self_side") == "B", f"rc={rc}")

        # ---- gate 死槽桶（窄：sidecar 行 + 同 socket/同字节数 WRN 两条证据）----
        gone = frame(11, 168, param=320, tag=268, series=6, body="aa")
        gone2 = frame(11, 168, param=319, tag=268, series=6, body="aa")
        one = [{"tick": 68, "socket": 1000, "len": 13, "sha256": sha12(gone),
                "src": "CrossZone", "surface": "RefMsg"}]
        two = one + [{"tick": 69, "socket": 1000, "len": 13, "sha256": sha12(gone2),
                      "src": "CrossZone", "surface": "RefMsg"}]
        gd, td, sd = case("dead", [judged_g], [judged_t], one)
        w_ok = gate_log("dead_ok", [wrn(1000, 13)])
        w_empty = gate_log("dead_empty", [])
        w_len = gate_log("dead_len", [wrn(1000, 99)])
        w_sock = gate_log("dead_sock", [wrn(999, 13)])
        out, rc = run_core(gd, td, sd, 1000, roles=roles, dead_socket_bucket=True, gate_log=w_ok)
        ck("dead_bucket_attributed", rc == 0 and out.get("sidecar_unconsumed") == 0
           and out.get("dead_socket_attributed_count") == 1
           and out.get("dead_socket_attributed", [{}])[0].get("len") == 13,
           f"rc={rc} unconsumed={out.get('sidecar_unconsumed')} attributed={out.get('dead_socket_attributed_count')}")
        out, rc = run_core(gd, td, sd, 1000, roles=roles, dead_socket_bucket=True, gate_log=w_empty)
        ck("dead_bucket_no_wrn_red", rc == 1 and out.get("sidecar_unconsumed") == 1
           and out.get("dead_socket_attributed_count") == 0, f"rc={rc}")
        out, rc = run_core(gd, td, sd, 1000, roles=roles, dead_socket_bucket=True, gate_log=w_len)
        ck("dead_bucket_len_mismatch_red", rc == 1 and out.get("sidecar_unconsumed") == 1,
           f"rc={rc} unconsumed={out.get('sidecar_unconsumed')}")
        out, rc = run_core(gd, td, sd, 1000, roles=roles, dead_socket_bucket=True, gate_log=w_sock)
        ck("dead_bucket_socket_mismatch_red", rc == 1 and out.get("sidecar_unconsumed") == 1,
           f"rc={rc}")
        gd2, td2, sd2 = case("dead2", [judged_g], [judged_t], two)
        out, rc = run_core(gd2, td2, sd2, 1000, roles=roles, dead_socket_bucket=True, gate_log=w_ok)
        ck("dead_bucket_two_lines_one_wrn", rc == 1 and out.get("sidecar_unconsumed") == 1
           and out.get("dead_socket_attributed_count") == 1, f"rc={rc} unconsumed={out.get('sidecar_unconsumed')}")
        ck("dead_bucket_requires_log", _dead_bucket_error(True, "") is not None
           and _dead_bucket_error(True, os.path.join(tmp, "nope.log")) is not None
           and _dead_bucket_error(False, "") is None, "缺/不存在 ⇒ 错误；关 ⇒ 不要求")
        out, rc = run_core(gd, td, sd, 1000, roles=roles)
        ck("dead_bucket_off_unchanged", rc == 1 and out.get("sidecar_unconsumed") == 1
           and "dead_socket_attributed_count" not in out, f"rc={rc}")

    # ---- body 角色归一（31/Tag1）：按角色映射归一，不是抹字段；窄白名单 ----
    ed = _load_ed()
    ck("edcode_available_for_body_norm", ed is not None, "oracle_capture.py 缺失 ⇒ body 归一判不了")
    if ed is not None:
        def wl(max_hp=16, status=0, hitter=0, mag=0):
            return ed.edcode_encode(struct.pack("<iiii", max_hp, status, hitter, mag)).hex()
        g_ok = [frame(31, 101, param=15, series=1, body=wl(16, 0, 102))]      # golden hitter=B(102)
        t_ok = [frame(31, 201, param=15, series=1, body=wl(16, 0, 202))]      # test   hitter=B(202)
        t_rolebad = [frame(31, 201, param=15, series=1, body=wl(16, 0, 201))] # test hitter=A(201) ⇒ 角色不同
        t_hpbad = [frame(31, 201, param=15, series=1, body=wl(17, 0, 202))]   # MaxHP 16→17（内容字段）
        g_unmapped = [frame(31, 101, param=15, series=1, body=wl(16, 0, 999))] # golden hitter=未知 actor
        g_u = [frame(30, 101, body=wl(16, 0, 102))]                           # 未登记 ident（30）
        t_u = [frame(30, 201, body=wl(16, 0, 202))]
        out_ok, rc_ok = run_core(*case("body_norm_ok", g_ok, t_ok), 1000, roles=roles)
        ck("body_role_norm_same_role_green", rc_ok == 0 and out_ok["multiset_reds"] == [],
           f"rc={rc_ok} reds={out_ok['multiset_reds']}")
        out_rb, rc_rb = run_core(*case("body_norm_rolebad", g_ok, t_rolebad), 1000, roles=roles)
        ck("body_role_norm_diff_role_red",
           rc_rb == 1 and any(r[0] == 31 for r in out_rb["multiset_reds"]), f"rc={rc_rb}")
        out_hb, rc_hb = run_core(*case("body_norm_hpbad", g_ok, t_hpbad), 1000, roles=roles)
        ck("body_role_norm_content_field_red",
           rc_hb == 1 and any(r[0] == 31 for r in out_hb["multiset_reds"]), f"rc={rc_hb}")
        out_um, rc_um = run_core(*case("body_norm_unmapped", g_unmapped, t_ok), 1000, roles=roles)
        ck("body_role_norm_one_side_unmapped_red",
           rc_um == 1 and any(r[0] == 31 for r in out_um["multiset_reds"]), f"rc={rc_um}")
        out_u, rc_u = run_core(*case("body_norm_unregistered", g_u, t_u), 1000, roles=roles)
        ck("body_role_norm_unregistered_ident_red",
           rc_u == 1 and any(r[0] == 30 for r in out_u["multiset_reds"]), f"rc={rc_u}")

        # ---- 未登记 ident 的「出现即红」（不许被悄悄归一）----
        id4 = lambda v: ed.edcode_encode(struct.pack("<i", v)).hex()  # noqa: E731
        g638 = [frame(638, 101, body=id4(102))]
        out_638, rc_638 = run_core(*case("ident638", g638, [frame(638, 201, body=id4(202))]),
                                   1000, roles=roles)
        ck("unregistered_638_identity_change_red",
           rc_638 == 1 and any(r[0] == 638 for r in out_638["multiset_reds"]), f"rc={rc_638}")
        out_638c, rc_638c = run_core(*case("ident638c", g638, [frame(638, 201, body=id4(102))]),
                                     1000, roles=roles)
        ck("unregistered_638_same_identity_green", rc_638c == 0, f"rc={rc_638c}")

        g41 = [frame(41, 101, param=102)]
        out_41, rc_41 = run_core(*case("ident41", g41, [frame(41, 201, param=202)]),
                                 1000, roles=roles)
        ck("unregistered_41_identity_change_red",
           rc_41 == 1 and any(r[0] == 41 for r in out_41["multiset_reds"]), f"rc={rc_41}")
        out_41c, rc_41c = run_core(*case("ident41c", g41, [frame(41, 201, param=102)]),
                                   1000, roles=roles)
        ck("unregistered_41_same_identity_green", rc_41c == 0, f"rc={rc_41c}")

        # ---- A23 方向②：未登记 ident 的**语义差异**方向——id 相同也必须红 ----
        # 638 的身份位 = body 的 4B target ActorId；param/tag/series 是语义字段（本用例改 param）。
        out_638s, rc_638s = run_core(*case("ident638_sem",
                                           [frame(638, 101, param=2001, body=id4(102))],
                                           [frame(638, 201, param=2002, body=id4(102))]),
                                     1000, roles=roles)
        ck("unregistered_638_same_id_semantic_change_red",
           rc_638s == 1 and any(r[0] == 638 for r in out_638s["multiset_reds"]),
           f"rc={rc_638s}")
        # 41 的 param = LoWord(GetFeatureToLong())（**外观特征**，不是 ActorId；见文件头）；
        # tag/series/body 是语义字段——本用例只改 tag。
        out_41s, rc_41s = run_core(*case("ident41_sem",
                                         [frame(41, 101, param=102, tag=7)],
                                         [frame(41, 201, param=102, tag=8)]),
                                   1000, roles=roles)
        ck("unregistered_41_same_param_semantic_change_red",
           rc_41s == 1 and any(r[0] == 41 for r in out_41s["multiset_reds"]),
           f"rc={rc_41s}")

        # ---- A23：「出现即红」的前提 = 帧的 recog 必须映射到角色 ----
        # recog 不在 `--roles` ⇒ key() 为 None ⇒ 进 other 桶、不参与 A/C1/C2/D（只计数）。
        out_other, rc_other = run_core(*case("ident638_other",
                                             [frame(638, 777, body=id4(102))],
                                             [frame(638, 778, body=id4(202))]),
                                       1000, roles=roles)
        ck("unregistered_638_unmapped_recog_not_judged",
           rc_other == 0 and out_other["multiset_reds"] == []
           and out_other["other_frames_golden"] == 1
           and out_other["other_frames_test"] == 1,
           f"rc={rc_other} other=({out_other.get('other_frames_golden')},"
           f"{out_other.get('other_frames_test')})")

        # ---- A23：surface 覆盖边界必须自述（meta 牙，防文本漂移）----
        surface = ((out_638.get("body_role_norm") or {}).get("surface")) or ""
        need = ["31", "角色", "怪物 hitter", "638", "41", "22", "23", "1104"]
        missing = [w for w in need if w not in surface]
        ck("body_role_norm_surface_lists_boundaries", not missing,
           f"missing={missing} surface={surface[:160]}")

        ok_more, det_more = True, []
        for ident in (22, 23, 1104):
            o, r = run_core(*case("ident%d" % ident,
                                  [frame(ident, 101, body=id4(102))],
                                  [frame(ident, 201, body=id4(202))]), 1000, roles=roles)
            cond = r == 1 and any(x[0] == ident for x in o["multiset_reds"])
            ok_more = ok_more and cond
            det_more.append((ident, r))
        ck("unregistered_22_23_1104_identity_change_red", ok_more, det_more)

        # ---- 怪物 hitter 的 31 仍 raw ⇒ 红（只归一角色 hitter）----
        g_mon = [frame(31, 101, param=15, series=1, body=wl(16, 0, 900001))]
        out_mon, rc_mon = run_core(*case("monster31", g_mon,
                                         [frame(31, 201, param=15, series=1, body=wl(16, 0, 900002))]),
                                   1000, roles=roles)
        ck("monster_hitter_31_raw_red",
           rc_mon == 1 and any(r[0] == 31 for r in out_mon["multiset_reds"]), f"rc={rc_mon}")
        out_monc, rc_monc = run_core(*case("monster31c", g_mon,
                                           [frame(31, 201, param=15, series=1, body=wl(16, 0, 900001))]),
                                     1000, roles=roles)
        ck("monster_hitter_31_same_raw_green", rc_monc == 0, f"rc={rc_monc}")

        # ---- EDCode 失败两条路径：模块缺失 / 解码长度不足 ⇒ degraded + WRN + rc=2 ----
        import io as _io
        import contextlib as _ctx

        _BODY_NORM_CACHE.clear()
        saved_mod = _ED_MOD.get("mod", "<absent>")
        _ED_MOD["mod"] = None
        try:
            err = _io.StringIO()
            with _ctx.redirect_stderr(err):
                od, rd = run_core(*case("degraded_mod",
                                        [frame(31, 101, param=15, series=1, body=wl(16, 0, 102))],
                                        [frame(31, 201, param=15, series=1, body=wl(16, 0, 102))]),
                                  1000, roles=roles)
            ck("edcode_missing_degraded_rc2",
               rd == 2 and od["body_role_norm"]["degraded"]
               and od["body_role_norm"]["normalized_frames"] == 0
               and "degraded" in err.getvalue(), f"rc={rd} bn={od['body_role_norm']}")
        finally:
            if saved_mod == "<absent>":
                _ED_MOD.pop("mod", None)
            else:
                _ED_MOD["mod"] = saved_mod
            _BODY_NORM_CACHE.clear()

        _BODY_NORM_CACHE.clear()
        short = id4(7)  # 解出 4B < 12 ⇒ 长度不足
        err2 = _io.StringIO()
        with _ctx.redirect_stderr(err2):
            od2, rd2 = run_core(*case("degraded_short",
                                      [frame(31, 101, param=15, series=1, body=short)],
                                      [frame(31, 201, param=15, series=1, body=short)]),
                                1000, roles=roles)
        ck("edcode_short_body_degraded_rc2",
           rd2 == 2 and od2["body_role_norm"]["degraded"] and "degraded" in err2.getvalue(),
           f"rc={rd2} bn={od2['body_role_norm']}")
        _BODY_NORM_CACHE.clear()

        # ---- 缓存键必须含 role_map（同 body、不同 role_map ⇒ 结果各自正确）----
        fr = {"ident": 31, "body_hex": wl(16, 0, 102)}
        h1, s1 = body_role_norm(fr, {101: "A", 102: "B", 201: "A", 202: "B"})
        h2, s2 = body_role_norm(fr, {102: "A"})
        ck("body_role_norm_cache_key_includes_role_map",
           s1 == "normalized" and s2 == "normalized" and h1 != h2,
           f"s1={s1} s2={s2} same_hex={h1 == h2}")
        _BODY_NORM_CACHE.clear()

    expected = 37
    ok = all(c for _, c, _ in checks) and len(checks) == expected
    for name, cond, detail in checks:
        print(f"SELFTEST {name} {'ok' if cond else 'FAIL'} {detail}")
    print(f"SELFTEST_META checks={len(checks)} expect={expected}")
    if not ok:
        print("SELFTEST_FAIL")
        return 1
    print("SELFTEST_OK")
    return 0

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--golden-dir")
    ap.add_argument("--test-dir")
    ap.add_argument("--sidecar")
    ap.add_argument("--socket", type=int)
    ap.add_argument("--mutant", choices=("", "cross-as-local", "global-plus1", "golden-perturb"), default="")
    ap.add_argument("--entry-window-keys", default="", help="逗号分隔的归一 key（sha256）：登录-走位时序差产生的 key；单列 entry_window，不计红不计绿，不静默吞")
    ap.add_argument("--ignore-idents", default="", help="逗号分隔 ident 列表：从比较面排除（如 201=SM_BAGITEMS 属 M1c）")
    ap.add_argument("--trim-tail-ms", type=int, default=0, help="丢弃各侧末尾 N ms 内的帧（探针收尾抖动；默认 0=不裁）")
    ap.add_argument("--roles", default="", help="goldenA,goldenB,testA,testB（把各 run 的 ActorId 归一成 A/B；给空则按原 recog 比）")
    ap.add_argument("--self-bucket", choices=("off", "report", "judge"), default="off",
                    help="recog==0 自我帧单独归桶：off=旧行为；report=只报告不改 rc；judge=计入 rc")
    ap.add_argument("--side", choices=("", "A", "B"), default="",
                    help="self 桶的收件人侧（非 off 时必填；显式给，不从目录名推断）")
    ap.add_argument("--dead-socket-bucket", action="store_true",
                    help="D 判据窄归因：sidecar 行 + 同 socket/同字节数 gate 死槽 WRN 才记归因丢弃")
    ap.add_argument("--gate-log", default="", help="本次 test 跑的 game-svr.out.log（--dead-socket-bucket 时必填）")
    ap.add_argument("--self-selftest", action="store_true", help="离线自测（合成夹具）")
    a = ap.parse_args()
    if a.self_selftest:
        sys.exit(_selftest())
    if not (a.golden_dir and a.test_dir and a.sidecar and a.socket is not None):
        print(json.dumps({"error": "--golden-dir/--test-dir/--sidecar/--socket 必填"}, ensure_ascii=False))
        sys.exit(2)
    err = _self_args_error(a.self_bucket, a.side)
    if err:
        print(json.dumps({"error": err, "self_bucket": a.self_bucket, "side": a.side}, ensure_ascii=False))
        sys.exit(2)
    derr = _dead_bucket_error(a.dead_socket_bucket, a.gate_log)
    if derr:
        print(json.dumps({"error": derr, "dead_socket_bucket": a.dead_socket_bucket,
                          "gate_log": a.gate_log}, ensure_ascii=False))
        sys.exit(2)
    out, rc = run_core(a.golden_dir, a.test_dir, a.sidecar, a.socket,
                       mutant=a.mutant, entry_window_keys=a.entry_window_keys,
                       ignore_idents=a.ignore_idents, trim_tail_ms=a.trim_tail_ms,
                       roles=a.roles, self_bucket=a.self_bucket, side=a.side,
                       dead_socket_bucket=a.dead_socket_bucket, gate_log=a.gate_log)
    print(json.dumps(out, ensure_ascii=False, indent=1))
    sys.exit(rc)

if __name__ == "__main__":
    main()
