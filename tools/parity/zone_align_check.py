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
- 角色归一 `--roles gA,gB,tA,tB`：未命中 A/B 的帧（other）不计 A/C1/C2/D，只计数（装置文档 §3）。
- **self 桶（卡「区 k MaxExp=0」证据配套）**：`recog==0` 的自我帧（如 `SM_ABILITY(52)`）不在
  `--roles` 映射里，旧口径整帧排除。`--self-bucket` 把该桶按**收件人侧**单独归键比较：
  * `off`（默认）⇒ 与旧版逐项一致（不输出 self_* 字段，rc 不变）；
  * `report` ⇒ 输出 self_* 与红项，**不改 rc**（产品修复前先看形态）；
  * `judge`  ⇒ self 红计入 rc（修复后的正式判据）。
  * 键 = `sha256(f"{side}|{ident}|{param}|{tag}|{series}|{body_hex}")`；`--side A|B` 必须显式给
    （golden A 对 test A、golden B 对 test B）。非 off 模式缺/错 `--side` ⇒ rc=2（fail-closed）。
  * 边界：只纳入 `recog==0`；`recog=-1/1/2000`（系统通告）与实体帧仍在 `other`。
- 自测：`--self-selftest`（离线、合成夹具、临时目录；不读仓库状态）。
- 退出码：0 全过；1 A/C1/D 红（judge 模式下含 self 红）；2 判不了。
"""
import argparse, json, sys, hashlib, collections, os, tempfile

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

def _self_key(side, f):
    """self 桶键：显式 side 进键，保证 A 侧与 B 侧永不互相配对。"""
    h = f'{side}|{f["ident"]}|{f["param"]}|{f["tag"]}|{f["series"]}|{f["body_hex"]}'
    return hashlib.sha256(h.encode()).hexdigest()

def _self_bucket(G, T, side):
    """`recog==0` 自我帧按收件人侧做多重集比较（G/T 已过 ignore/trim/entry-window）。"""
    sg = [f for f in G if f["recog"] == 0]
    st = [f for f in T if f["recog"] == 0]
    cg = collections.Counter(_self_key(side, f) for f in sg)
    ct = collections.Counter(_self_key(side, f) for f in st)
    reds = []
    for k in sorted(set(cg) | set(ct)):
        if cg[k] == ct[k]:
            continue
        sample = next((f for f in sg + st if _self_key(side, f) == k), None)
        reds.append({
            "key": k[:16], "golden": cg[k], "test": ct[k],
            "ident": sample["ident"] if sample else None,
            "param": sample["param"] if sample else None,
            "tag": sample["tag"] if sample else None,
            "series": sample["series"] if sample else None,
            "body_hex_golden": next((f["body_hex"][:80] for f in sg
                                     if _self_key(side, f) == k), ""),
            "body_hex_test": next((f["body_hex"][:80] for f in st
                                   if _self_key(side, f) == k), ""),
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

def run_core(golden_dir, test_dir, sidecar, socket, mutant="", entry_window_keys="",
             ignore_idents="", trim_tail_ms=0, roles="", self_bucket="off", side=""):
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

    role_map = {}
    if roles:
        parts = [int(x) for x in roles.split(",")]
        role_map = {parts[0]: "A", parts[1]: "B", parts[2]: "A", parts[3]: "B"}
    def key(f):
        if not role_map:
            return sha12(f)
        role = role_map.get(f["recog"])
        if role is None:
            return None
        hdr = f'{role}|{f["ident"]}|{f["param"]}|{f["tag"]}|{f["series"]}|{f["body_hex"]}'
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
    # self 桶：additive（`other_frames_*` 仍含 recog=0，self 是它的子集；不改旧字段）。
    self_red = 0
    if self_bucket in ("report", "judge"):
        sb = _self_bucket(G, T, side)
        out["self_bucket"] = self_bucket
        out["self_side"] = side
        out.update(sb)
        self_red = sb["self_red_count"]
    rc = 1 if (reds or bad_order or judge_d_red) else 0
    if self_bucket == "judge" and self_red > 0:
        rc = 1
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
        def case(name, gframes, tframes):
            g = write_dir(os.path.join(tmp, name, "golden"), gframes)
            t = write_dir(os.path.join(tmp, name, "test"), tframes)
            sc = os.path.join(tmp, name, "cross-src.jsonl")
            open(sc, "w", encoding="utf-8").close()
            return g, t, sc

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

    expected = 10
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
    out, rc = run_core(a.golden_dir, a.test_dir, a.sidecar, a.socket,
                       mutant=a.mutant, entry_window_keys=a.entry_window_keys,
                       ignore_idents=a.ignore_idents, trim_tail_ms=a.trim_tail_ms,
                       roles=a.roles, self_bucket=a.self_bucket, side=a.side)
    print(json.dumps(out, ensure_ascii=False, indent=1))
    sys.exit(rc)

if __name__ == "__main__":
    main()
