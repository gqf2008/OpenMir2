# -*- coding: utf-8 -*-
"""run6 equip_struck_save 的武器磨损/落库只读重建（卡 openmir2-equip-struck-dura-db-triage）。

不连接 DB、不起栈。直接读已冻结的 frames/c2s/fixture/save_settle/db TSV：
  python -I -B tools/window/equip_struck_dura_triage.py \
    --root E:/tmp/window/run6/equip_struck_save --json E:/tmp/dev-2/dura-run6-analysis.json

--check 跑合成正负例与变异牙（无攻击、同值、run6 形态、快照不一致、重复 642、
比较器非恒等；RNG 账本存在/缺失/坏头/只有字样无 probe 行；攻击窗口多抽 B/多抽 C/少抽 B/
缺终点/起点不唯一；player_ctor 双侧；analyze()→analyze_rng_streams 接线正负例），
真实状态只从 tempdir 读，不写仓库。
"""
import argparse
import collections
import csv
import hashlib
import importlib.util
import json
import sys
import tempfile
from pathlib import Path

RNG_DIFF_PATH = (Path(__file__).resolve().parents[2]
                 / "mir2-rs/tests/parity/m3/rng-trace-probe/rng_ledger_diff.py")

CM_HIT = 3014
SM_DURACHANGE = 642
WEAPON_SLOT = 1
WEAR_MIN = 2
WEAR_MAX = 6  # Random(5)+2，WeaponStrong=0

# 攻击局部窗口的站点 vocabulary（与 rng_ledger_diff.CANON_RULES 的 attack 族一致）。
# 窗口 shape 取「A→首个 attack_success 之间**全部** attack 族 canonical 的有序序列（含重复）」，
# 这样一侧多抽/少抽一次 B/C 会直接改变 shape，而不是被固定三元组吞掉。
ATTACK_WINDOW_CANON = (
    "atk_power_a", "atk_power_b", "atk_power_base", "hit_gate", "attack_success",
    "hit_struck_damage", "hit_struck_damage2", "struck_durability",
    "struck_durability_slot", "attack_skill_point_count",
)
# 玩家构造抽的站点识别（C# PlayObject..ctor / Rust ctor_fold 折叠点）。抽成常量便于自测钉住。
PLAYER_CTOR_SITES = ("PlayObject..ctor@PlayObject.cs", "ctor_fold.rs:545")


class InputError(Exception):
    pass


def read_json(path):
    return json.loads(Path(path).read_text(encoding="utf-8-sig"))


def read_ndjson(path):
    return [json.loads(line) for line in Path(path).read_text(
        encoding="utf-8-sig").splitlines() if line.strip()]


def sha256(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def public_frame(frame):
    return {key: frame.get(key) for key in
            ("seq", "t_ms", "phase", "ident", "recog", "param", "tag", "series", "body_len")}


def rng_ledger_status(path):
    """按 card_window --rng-log 的真实产物判定账本是否可用。

    固定路径是 `<out>/<side>/rng-stream.tsv`；probe 的 run.json argv 不含 `--rng-log`。
    格式与 `card_window.check_rng_ledger` 同：头四行内有 `INJECT_OK` 与 `SEED_PROBE`，
    其后每行 `seq\tkind\tmax/arg\tresult\tsite` 至少 5 列且 site 非空。
    """
    path = Path(path)
    if not path.is_file():
        return {"path": str(path), "exists": False, "valid": False,
                "detail": "RNG 账本不存在"}
    try:
        lines = [line.rstrip("\r\n") for line in path.read_text(
            encoding="utf-8", errors="replace").splitlines() if line.strip()]
    except OSError as exc:
        return {"path": str(path), "exists": True, "valid": False,
                "detail": "RNG 账本读失败：%s" % exc}
    if len(lines) < 2:
        return {"path": str(path), "exists": True, "valid": False,
                "detail": "RNG 账本为空/不足两行"}
    head = "\n".join(lines[:4])
    if "INJECT_OK" not in head:
        return {"path": str(path), "exists": True, "valid": False,
                "detail": "RNG 账本缺 INJECT_OK 自证头"}
    probes = [line for line in lines[:4] if line.split()[:1] == ["SEED_PROBE"]]
    if not probes:
        return {"path": str(path), "exists": True, "valid": False,
                "detail": "RNG 账本缺以 SEED_PROBE 开头的 probe 行"}
    if len(probes[0].split()) < 2:
        return {"path": str(path), "exists": True, "valid": False,
                "detail": "SEED_PROBE 行没有探针值"}
    for number, line in enumerate(lines[2:], start=3):
        columns = line.split("\t")
        if len(columns) < 5:
            return {"path": str(path), "exists": True, "valid": False,
                    "detail": "RNG 账本第 %d 行列数 %d < 5" % (number, len(columns))}
        if not columns[4].strip():
            return {"path": str(path), "exists": True, "valid": False,
                    "detail": "RNG 账本第 %d 行 site 为空" % number}
    return {"path": str(path), "exists": True, "valid": True,
            "detail": "%d 行，头/列/site 均通过" % len(lines)}


def pair_rng_log_status(statuses):
    """`rng_log_present` 只认至少一侧**可用**的账本；坏文件单独登记，不得算作证据。"""
    return {
        "rng_log_present": any(status["valid"] for status in statuses),
        "rng_log_any_file": any(status["exists"] for status in statuses),
        "rng_log_invalid_sides": [status["path"] for status in statuses
                                  if status["exists"] and not status["valid"]],
    }


def _load_rngdiff():
    spec = importlib.util.spec_from_file_location("rng_ledger_diff", RNG_DIFF_PATH)
    if spec is None or spec.loader is None:
        raise InputError("无法加载 %s" % RNG_DIFF_PATH)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def _seed_probe(path):
    """返回 probe 行的 token 列表（不含 `SEED_PROBE` 本身）；缺行/无值 ⇒ None。

    不再返回原始字符串：`None == None` 会让「两侧都缺 probe 行」被误判成 `seed_probe_equal=true`。
    """
    for line in Path(path).read_text(encoding="utf-8", errors="replace").splitlines():
        parts = line.split()
        if parts[:1] == ["SEED_PROBE"]:
            return parts[1:] or None
    return None


def analyze_rng_streams(csharp_path, rust_path):
    """对 run9 两侧 rng-stream 做站点/攻击局部结构对照。

    不是拿全局 draw index 当同一契约：C# 的共享 `Random` 会被怪物线程并发调用，
    日志里同一攻击的 A/B/C 三抽可能夹着别的站点；Rust 单线程则连续。这里分别报告
    全局首差与「攻击局部三抽形状/值」，并把跨线程夹层显式量化。
    """
    try:
        rd = _load_rngdiff()
        a = rd.parse_trace(str(csharp_path))
        b = rd.parse_trace(str(rust_path))
    except (OSError, ValueError, AttributeError) as exc:
        return {"status": "UNDECIDABLE", "reason": "RNG 账本解析失败：%s" % exc}
    if not a or not b:
        return {"status": "UNDECIDABLE", "reason": "RNG 账本为空"}

    def key(row):
        return (row[1], row[2], row[3])

    first = None
    for i in range(min(len(a), len(b))):
        if key(a[i]) != key(b[i]):
            first = {
                "index": i + 1,
                "csharp": {"site": a[i][4], "max": a[i][2], "result": a[i][3],
                           "canonical": rd.canonical(a[i][4])},
                "rust": {"site": b[i][4], "max": b[i][2], "result": b[i][3],
                         "canonical": rd.canonical(b[i][4])},
            }
            break

    def attack_draw_counts(rows):
        counts = collections.Counter(
            rd.canonical(row[4]) for row in rows
            if rd.canonical(row[4]) in ATTACK_WINDOW_CANON)
        return dict(sorted(counts.items()))

    def attack_window(rows):
        """窗口 = 唯一的 `atk_power_a` → 其后的首个 `attack_success`（含两端）。

        shape 不是「搜到 A/B/C 再拼固定三元组」，而是窗口内**全部** attack 族 canonical 的
        有序序列（含重复）；重复/缺失的 B、C 会改变 shape ⇒ 可判多抽/少抽。起点不唯一
        （一次跑里多次玩家攻击）或没有终点时返回 None，交上层判 UNDECIDABLE，不静默取第一个。
        """
        starts = [i for i, row in enumerate(rows)
                  if rd.canonical(row[4]) == "atk_power_a"]
        if len(starts) != 1:
            reason = ("缺攻击起点 atk_power_a" if not starts
                      else "攻击起点 atk_power_a 有 %d 个，无法唯一定窗" % len(starts))
            return None, len(starts), reason
        ia = starts[0]
        ic = next((i for i in range(ia + 1, len(rows))
                   if rd.canonical(rows[i][4]) == "attack_success"), None)
        if ic is None:
            return None, len(starts), "atk_power_a 之后没有 attack_success，窗口无法定界"
        window = rows[ia:ic + 1]
        attack_rows = [row for row in window
                       if rd.canonical(row[4]) in ATTACK_WINDOW_CANON]
        shape = [rd.canonical(row[4]) for row in attack_rows]
        # 首个 Success 之后再出现的 Success 属于同一次攻击的毒门取数（C# :221/:225、
        # Rust 1150/1258 同 canonical）或多抽；扫描到下一个非 Success 的攻击族抽为止
        # （那已是下一次攻击的开始），不把后续攻击算进本窗口。两侧该计数不等 ⇒ 多/少抽 C。
        stop = next((i for i in range(ic + 1, len(rows))
                     if rd.canonical(rows[i][4]) in ATTACK_WINDOW_CANON
                     and rd.canonical(rows[i][4]) != "attack_success"), len(rows))
        trailing = [row for row in rows[ic + 1:stop]
                    if rd.canonical(row[4]) == "attack_success"]
        middle = rows[ia + 1:ic]
        interleaved = collections.Counter(
            rd.canonical(row[4]) for row in middle
            if rd.canonical(row[4]) not in ATTACK_WINDOW_CANON)
        return {
            "shape": shape,
            "attack_counts": dict(sorted(collections.Counter(shape).items())),
            "maxes": [row[2] for row in attack_rows],
            "window_draws": [ia + 1, ic + 1],
            "values": [{"max": row[2], "result": row[3], "seq": row[0], "site": row[4]}
                       for row in attack_rows],
            "trailing_success_after_window": len(trailing),
            "trailing_success_seqs": [row[0] for row in trailing],
            "interleaved_between_a_success": sum(interleaved.values()),
            "interleaved_sites": dict(sorted(interleaved.items())),
        }, len(starts), None

    def player_ctor(rows):
        for i, row in enumerate(rows):
            site = row[4].replace("\\", "/")
            if any(marker in site for marker in PLAYER_CTOR_SITES):
                return {"index": i + 1, "seq": row[0], "max": row[2],
                        "result": row[3], "site": row[4]}
        return None

    ca, ca_total, ca_reason = attack_window(a)
    rb, rb_total, rb_reason = attack_window(b)
    reason = None
    if ca is None or rb is None:
        classification = "UNDECIDABLE_ATTACK_WINDOW"
        reason = "；".join(part for part in (
            "csharp: " + ca_reason if ca_reason else "",
            "rust: " + rb_reason if rb_reason else "") if part)
    elif ca["shape"] != rb["shape"]:
        classification = "ATTACK_CALL_SHAPE_DIVERGENCE"
        reason = "攻击族有序序列（含重复）不同 ⇒ 存在多抽/少抽或次序差"
    elif ca["trailing_success_after_window"] != rb["trailing_success_after_window"]:
        classification = "ATTACK_CALL_SHAPE_DIVERGENCE"
        reason = "首个 Success 之后的 Success 抽数不同（毒门/多抽 C）"
    elif ca["maxes"] != rb["maxes"]:
        classification = "ATTACK_CALL_ARG_DIVERGENCE"
        reason = "攻击族调用序列相同但取数上界不同（参数面差，不是多/少抽）"
    else:
        classification = "ATTACK_LOCAL_SHAPE_EQUAL_GLOBAL_STREAM_INTERLEAVED"
    csharp_probe = _seed_probe(csharp_path)
    rust_probe = _seed_probe(rust_path)
    result = {
        "status": "UNDECIDABLE" if classification == "UNDECIDABLE_ATTACK_WINDOW" else "DIAGNOSED",
        "seed_probe": {"csharp": csharp_probe, "rust": rust_probe},
        "seed_probe_equal": bool(csharp_probe) and bool(rust_probe)
                            and csharp_probe == rust_probe,
        "draws": {"csharp": len(a), "rust": len(b)},
        "first_value_divergence": first,
        "player_ctor_draw": {"csharp": player_ctor(a), "rust": player_ctor(b)},
        "attack_windows_total": {"csharp": ca_total, "rust": rb_total},
        "attack_draw_counts": {"csharp": attack_draw_counts(a),
                               "rust": attack_draw_counts(b)},
        "attack_local": {"csharp": ca, "rust": rb},
        "classification": classification,
        "boundary": "全局 draw 序列含 C# 多线程共享 Random 的交错；首差与末值/夹层差不能直接归为"
                    "攻击实现缺/多抽。shape 是 A→首个 Success 之间全部 attack 族 canonical 的"
                    "有序序列（含重复），多/少抽会直接触发 ATTACK_CALL_SHAPE_DIVERGENCE；"
                    "shape 相等仍不等于 Dura 数值相等（流位置/交错仍是 R 级未决项）。",
    }
    if reason:
        result["reason"] = reason
    return result


def evaluate(initial_dura, hit_count, dura_values, final_dura):
    """由事件序列推末值；不让末值反推一个脱离事件的故事。"""
    wear = initial_dura - final_dura
    snapshot_consistent = (final_dura == dura_values[-1] if dura_values
                           else final_dura == initial_dura)
    event_count_consistent = hit_count == len(dura_values)
    single_wear_candidate = (hit_count == 1 and event_count_consistent
                             and snapshot_consistent and WEAR_MIN <= wear <= WEAR_MAX)
    return {
        "initial_dura": initial_dura,
        "hit_count": hit_count,
        "dura_values": list(dura_values),
        "final_dura": final_dura,
        "wear": wear,
        "inferred_random5": wear - 2 if single_wear_candidate else None,
        "snapshot_consistent": snapshot_consistent,
        "event_count_consistent": event_count_consistent,
        "single_wear_candidate": single_wear_candidate,
    }


def classify_pair(csharp, rust):
    if (csharp["hit_count"] == 0 and rust["hit_count"] == 0
            and csharp["final_dura"] == csharp["initial_dura"]
            and rust["final_dura"] == rust["initial_dura"]):
        return "NO_ATTACK_NO_CHANGE"
    if csharp["single_wear_candidate"] and rust["single_wear_candidate"]:
        if csharp["initial_dura"] != rust["initial_dura"]:
            return "UNDECIDABLE_INITIAL_DIFF"
        if csharp["wear"] == rust["wear"]:
            return "SAME_SINGLE_WEAR"
        return "SINGLE_RANDOM5_FLOW_DIFFERENCE_CANDIDATE"
    return "UNDECIDABLE_EVENT_OR_SNAPSHOT"


def equipped_row(root, side, player_id, make_index):
    db_dir = Path(root) / side / "db"
    manifest = read_json(db_dir / "manifest.json")
    table = next((item for item in manifest["tables"]
                  if item["table"].endswith(".characters_item")), None)
    if table is None:
        raise InputError("%s 缺 characters_item manifest" % side)
    columns = [item["name"] for item in table["columns"]]
    matches = []
    with (db_dir / table["file"]).open(encoding="utf-8-sig", newline="") as handle:
        for row in csv.reader(handle, delimiter="\t"):
            record = dict(zip(columns, row))
            if (record.get("PlayerId") == str(player_id)
                    and record.get("Position") == str(WEAPON_SLOT)
                    and record.get("MakeIndex") == str(make_index)):
                matches.append(record)
    if len(matches) != 1:
        raise InputError("%s 装备位 MakeIndex=%s 应恰一行，实得 %d"
                         % (side, make_index, len(matches)))
    return {"table": table["table"], "row": matches[0], "sha256": sha256(db_dir / table["file"])}


def analyze(root):
    root = Path(root)
    result = {"format": "equip-struck-dura-triage/1", "root": str(root), "sides": {}}
    for side in ("csharp", "rust"):
        side_dir = root / side
        fixture = read_json(side_dir / "inventory.json")
        declared = fixture["declared"]
        if len(declared) != 1:
            raise InputError("%s 本分析器要求恰一个武器夹具，实得 %d" % (side, len(declared)))
        weapon = declared[0]
        initial = int(weapon["dura"])
        make_index = int(weapon["make_index"])
        frames = read_ndjson(side_dir / "frames.ndjson")
        c2s = read_ndjson(side_dir / "c2s.ndjson")
        hits = [public_frame(frame) for frame in c2s if frame.get("ident") == CM_HIT]
        dura_frames = [public_frame(frame) for frame in frames
                       if frame.get("ident") == SM_DURACHANGE]
        row_evidence = equipped_row(root, side, fixture["player_id"], make_index)
        row = row_evidence["row"]
        evaluated = evaluate(initial, len(hits), [frame["recog"] for frame in dura_frames],
                             int(row["Dura"]))
        settle = read_json(side_dir / "save_settle.json")
        run = read_json(side_dir / "run.json")
        rng_log = rng_ledger_status(side_dir / "rng-stream.tsv")
        result["sides"][side] = {
            "fixture": {
                "player_id": fixture["player_id"], "make_index": make_index,
                "std_index": weapon["index"], "position": weapon["position"],
                "initial_dura": initial, "dura_max": int(weapon["dura_max"]),
                "sha256": sha256(side_dir / "inventory.json"),
            },
            "attacks": hits,
            "dura_frames": dura_frames,
            "related_frame_counts": {
                str(ident): sum(frame.get("ident") == ident for frame in frames)
                for ident in (14, 31, 202, 203, 642)
            },
            "db": row_evidence,
            "evaluation": evaluated,
            "save_settle": {
                "action": settle.get("action"), "rc": settle.get("rc"),
                "changes": settle.get("changes"), "elapsed_ms": settle.get("elapsed_ms"),
                "quiet_ms": settle.get("quiet_ms"), "settled": settle.get("settled"),
                "last_witness": settle.get("witness_last"),
            },
            "run_wall": {"start": run.get("start_wall"), "end": run.get("end_wall")},
            "rng_log": rng_log,
            "frames_sha256": sha256(side_dir / "frames.ndjson"),
            "c2s_sha256": sha256(side_dir / "c2s.ndjson"),
        }
    result["pair"] = {
        "classification": classify_pair(result["sides"]["csharp"]["evaluation"],
                                        result["sides"]["rust"]["evaluation"]),
        "wear_difference_csharp_minus_rust": (
            result["sides"]["csharp"]["evaluation"]["wear"]
            - result["sides"]["rust"]["evaluation"]["wear"]),
        "final_db_equals_last_642_both": all(
            result["sides"][side]["evaluation"]["snapshot_consistent"]
            for side in ("csharp", "rust")),
        **pair_rng_log_status([result["sides"][side]["rng_log"]
                               for side in ("csharp", "rust")]),
        "boundary": "rng_log_present 只认 <side>/rng-stream.tsv 通过头/列/site 校验；"
                    "单次 Random(5) 形态候选不等于已有 RNG 流水证明，无有效流水时最终 R 结论保持未决。",
    }
    rng_paths = {side: root / side / "rng-stream.tsv" for side in ("csharp", "rust")}
    if all(result["sides"][side]["rng_log"]["valid"] for side in ("csharp", "rust")):
        result["rng_alignment"] = analyze_rng_streams(
            rng_paths["csharp"], rng_paths["rust"])
    else:
        result["rng_alignment"] = {
            "status": "UNDECIDABLE",
            "reason": "两侧没有通过头/列/site 校验的 rng-stream.tsv",
        }
    return result


def synthetic(initial=4000, hits=1, dura_values=(3997,), final=3997):
    return evaluate(initial, hits, list(dura_values), final)


def check():
    cases = []

    def require(condition, name):
        cases.append((bool(condition), name))
        print("%s %s" % ("PASS" if condition else "FAIL", name))

    no_attack_c = synthetic(hits=0, dura_values=(), final=4000)
    no_attack_r = synthetic(hits=0, dura_values=(), final=4000)
    require(classify_pair(no_attack_c, no_attack_r) == "NO_ATTACK_NO_CHANGE"
            and no_attack_c["wear"] == no_attack_r["wear"] == 0,
            "无成功攻击：两侧 Dura 均不变")

    same_c = synthetic()
    same_r = synthetic()
    require(classify_pair(same_c, same_r) == "SAME_SINGLE_WEAR",
            "同一次同值磨损：不得伪报随机流位差")

    run6_c = synthetic(dura_values=(3997,), final=3997)
    run6_r = synthetic(dura_values=(3994,), final=3994)
    require(classify_pair(run6_c, run6_r) == "SINGLE_RANDOM5_FLOW_DIFFERENCE_CANDIDATE"
            and run6_c["inferred_random5"] == 1 and run6_r["inferred_random5"] == 4,
            "run6 形态：3997/3994 分别唯一映射 Random(5)=1/4")

    bad_snapshot = synthetic(dura_values=(3994,), final=4000)
    require(not bad_snapshot["snapshot_consistent"]
            and classify_pair(run6_c, bad_snapshot) == "UNDECIDABLE_EVENT_OR_SNAPSHOT",
            "变异牙①：DB 未等于最后 642 ⇒ 不得判单次 RNG")

    duplicate_642 = synthetic(dura_values=(3997, 3994), final=3994)
    require(not duplicate_642["event_count_consistent"]
            and classify_pair(run6_c, duplicate_642) == "UNDECIDABLE_EVENT_OR_SNAPSHOT",
            "变异牙②：一次攻击却两条 642 ⇒ 不得编单次 RNG 解释")

    require(classify_pair(same_c, same_r) != classify_pair(run6_c, run6_r),
            "负对照：构造应报差输入，比较器不恒等")

    with tempfile.TemporaryDirectory() as tmp:
        missing = rng_ledger_status(Path(tmp) / "missing.tsv")
        require(not missing["exists"] and not missing["valid"]
                and pair_rng_log_status([missing])["rng_log_present"] is False,
                "RNG 账本无文件 ⇒ present=false")

        valid_path = Path(tmp) / "rng-stream.tsv"
        valid_path.write_text("INJECT_OK\nSEED_PROBE 1 2 3\n0\tNext\t5\t4\tcombat.rs:1151\n",
                              encoding="utf-8")
        valid = rng_ledger_status(valid_path)
        require(valid["exists"] and valid["valid"]
                and pair_rng_log_status([valid])["rng_log_present"] is True,
                "RNG 账本存在+头/列/site 合格 ⇒ present=true")

        bad_path = Path(tmp) / "bad.tsv"
        bad_path.write_text("NO_HEADER\nSEED_PROBE\n0\tNext\t5\t4\tcombat.rs:1151\n",
                            encoding="utf-8")
        bad = rng_ledger_status(bad_path)
        require(bad["exists"] and not bad["valid"]
                and pair_rng_log_status([bad])["rng_log_present"] is False,
                "变异牙③：坏头文件存在但不得当流水证据")

        def rng_file(name, rows, seed="1 2 3"):
            p = Path(tmp) / name
            body = "1\tINJECT_OK\t42\t0\nSEED_PROBE %s\n" % seed
            body += "".join("%d\tNext(max)\t%s\t%s\t%s\n" % row for row in rows)
            p.write_text(body, encoding="utf-8")
            return p

        ca = "M2Server.Player.PlayObject.GetAttackPower@PlayObject.cs:2929"
        cb = "M2Server.Player.PlayObject.GetAttackPower@PlayObject.cs:2934"
        cc = "M2Server.Player.PlayObject.AttackSuccess@PlayObject.Attack.cs:223"
        w = "M2Server.Actor.AnimalObject.Wondering@AnimalObject.cs:427"
        ra = r"crates\world\src\combat.rs:109"
        rb_ = r"crates\world\src\combat.rs:111"
        rc = r"crates\world\src\combat.rs:1152"
        csharp = rng_file("csharp.tsv", [
            (1, 5, 3, ca), (2, 20, 7, w), (3, 20, 1, w),
            (4, 10, 8, cb), (5, 20, 2, w), (6, 5, 3, cc)])
        rust = rng_file("rust.tsv", [
            (1, 5, 3, ra), (2, 10, 8, rb_), (3, 5, 4, rc)])
        def side_case(align_result, side):
            """缺窗口/断接线时返回空 dict，让断言命名 FAIL 而不是抛 KeyError/TypeError。"""
            return ((align_result.get("attack_local") or {}).get(side) or {})

        align = analyze_rng_streams(csharp, rust)
        require(align["seed_probe_equal"]
                and align["classification"] == "ATTACK_LOCAL_SHAPE_EQUAL_GLOBAL_STREAM_INTERLEAVED"
                and side_case(align, "csharp").get("shape")
                == ["atk_power_a", "atk_power_b", "attack_success"]
                and side_case(align, "csharp").get("interleaved_between_a_success") == 3
                and side_case(align, "rust").get("interleaved_between_a_success") == 0
                and ((align.get("attack_draw_counts") or {}).get("csharp") or {})
                .get("atk_power_a") == 1,
                "RNG 诊断：攻击局部 shape 相等，C# 夹层抽数被量化、攻击计数可读")

        require(align["first_value_divergence"] is not None
                and align["first_value_divergence"]["index"] == 2,
                "RNG 诊断：报出首个值分歧（示例 index=2）")

        missing_b = rng_file("missing-b.tsv", [
            (1, 5, 3, ra), (2, 5, 4, rc)])
        align_missing_b = analyze_rng_streams(csharp, missing_b)
        require(align_missing_b["classification"] == "ATTACK_CALL_SHAPE_DIVERGENCE"
                and side_case(align_missing_b, "rust").get("shape")
                == ["atk_power_a", "attack_success"],
                "RNG 诊断牙①：一侧少抽一次 B ⇒ ATTACK_CALL_SHAPE_DIVERGENCE（不是 EQUAL/UNDECIDABLE）")

        extra_b = rng_file("extra-b.tsv", [
            (1, 5, 3, ra), (2, 10, 8, rb_), (3, 5, 4, rb_), (4, 5, 3, rc)])
        align_extra_b = analyze_rng_streams(csharp, extra_b)
        require(align_extra_b["classification"] == "ATTACK_CALL_SHAPE_DIVERGENCE"
                and side_case(align_extra_b, "rust").get("shape")
                == ["atk_power_a", "atk_power_b", "atk_power_b", "attack_success"],
                "RNG 诊断牙②：一侧多抽一次 B ⇒ DIVERGENCE（旧固定三元组会漏报成 EQUAL）")

        extra_c = rng_file("extra-c.tsv", [
            (1, 5, 3, ra), (2, 10, 8, rb_), (3, 5, 4, rc), (4, 5, 1, rc)])
        align_extra_c = analyze_rng_streams(csharp, extra_c)
        require(align_extra_c["classification"] == "ATTACK_CALL_SHAPE_DIVERGENCE"
                and side_case(align_extra_c, "rust").get("trailing_success_after_window") == 1
                and side_case(align_extra_c, "csharp").get("trailing_success_after_window") == 0,
                "RNG 诊断牙③：一侧多抽一次 C ⇒ DIVERGENCE（首个 Success 之后仍有 Success）")

        no_success = rng_file("no-success.tsv", [
            (1, 5, 3, ra), (2, 10, 8, rb_)])
        align_no_success = analyze_rng_streams(csharp, no_success)
        require(align_no_success["classification"] == "UNDECIDABLE_ATTACK_WINDOW"
                and "attack_success" in align_no_success.get("reason", ""),
                "RNG 诊断牙④：缺 attack_success ⇒ UNDECIDABLE（窗口无法定界）")

        two_starts = rng_file("two-starts.tsv", [
            (1, 5, 3, ra), (2, 10, 8, rb_), (3, 5, 3, rc), (4, 5, 3, ra)])
        align_two = analyze_rng_streams(csharp, two_starts)
        require(align_two["classification"] == "UNDECIDABLE_ATTACK_WINDOW"
                and align_two["attack_windows_total"] == {"csharp": 1, "rust": 2}
                and "2 个" in align_two.get("reason", ""),
                "RNG 诊断牙⑤：起点不唯一 ⇒ UNDECIDABLE + attack_windows_total（不静默取第一个）")

        ctor_c = "M2Server.Player.PlayObject..ctor@PlayObject.cs:1046"
        ctor_r = r"crates\world\src\ctor_fold.rs:545"
        ctor_csharp = rng_file("ctor-c.tsv", [
            (1, 999999, 168434, ctor_c),
            (2, 5, 3, ca), (3, 10, 8, cb), (4, 5, 3, cc)])
        ctor_rust = rng_file("ctor-r.tsv", [
            (1, 20, 7, w), (2, 999999, 943999, ctor_r),
            (3, 5, 3, ra), (4, 10, 8, rb_), (5, 5, 3, rc)])
        align_ctor = analyze_rng_streams(ctor_csharp, ctor_rust)
        ctor_csharp_draw = align_ctor["player_ctor_draw"]["csharp"]
        ctor_rust_draw = align_ctor["player_ctor_draw"]["rust"]
        require(ctor_csharp_draw is not None and ctor_rust_draw is not None
                and ctor_csharp_draw["index"] == 1 and ctor_rust_draw["index"] == 2
                and align_ctor["classification"]
                == "ATTACK_LOCAL_SHAPE_EQUAL_GLOBAL_STREAM_INTERLEAVED",
                "RNG 诊断牙⑥：player_ctor_draw 双侧取到构造抽（删识别分支即 FAIL）")

        diff_seed = rng_file("diff-seed.tsv", [
            (1, 5, 3, ra), (2, 10, 8, rb_), (3, 5, 4, rc)], seed="9 9 9")
        require(analyze_rng_streams(csharp, diff_seed)["seed_probe_equal"] is False,
                "RNG 诊断：SEED_PROBE 不同 ⇒ 显式 false")

        no_probe = Path(tmp) / "no-probe.tsv"
        no_probe.write_text("1\tINJECT_OK\t42\t0\n0\tNext\t5\t3\tcombat.rs:1151\n",
                            encoding="utf-8")
        align_no_probe = analyze_rng_streams(no_probe, no_probe)
        require(align_no_probe["seed_probe_equal"] is False
                and align_no_probe["seed_probe"]["csharp"] is None
                and align_no_probe["seed_probe"]["rust"] is None,
                "RNG 诊断牙⑦：两侧都缺 SEED_PROBE 行 ⇒ seed_probe_equal=false（None==None 不得为真）")

        word_only = Path(tmp) / "word-only.tsv"
        word_only.write_text(
            "1\tINJECT_OK\t42\t0\n# note: SEED_PROBE missing here\n"
            "0\tNext\t5\t3\tcombat.rs:1151\n", encoding="utf-8")
        require(rng_ledger_status(word_only)["valid"] is False
                and "SEED_PROBE" in rng_ledger_status(word_only)["detail"],
                "RNG 账本牙⑧：头里只出现 SEED_PROBE 字样、没有 probe 行 ⇒ 不合格")

        def write_root(name, rng_csharp, rng_rust):
            root = Path(tmp) / name
            for side, rng_text in (("csharp", rng_csharp), ("rust", rng_rust)):
                d = root / side
                (d / "db").mkdir(parents=True)
                (d / "inventory.json").write_text(json.dumps({
                    "player_id": 1001,
                    "declared": [{"dura": 4000, "dura_max": 4000, "make_index": 42,
                                  "index": 7, "position": 1}]}), encoding="utf-8")
                frame = {"seq": 1, "t_ms": 1, "phase": "run", "ident": 642,
                         "recog": 3997, "param": 0, "tag": 0, "series": 0, "body_len": 0}
                (d / "frames.ndjson").write_text(json.dumps(frame) + "\n", encoding="utf-8")
                hit = dict(frame, ident=3014, recog=0)
                (d / "c2s.ndjson").write_text(json.dumps(hit) + "\n", encoding="utf-8")
                (d / "save_settle.json").write_text("{}\n", encoding="utf-8")
                (d / "run.json").write_text("{}\n", encoding="utf-8")
                (d / "db" / "manifest.json").write_text(json.dumps({"tables": [{
                    "table": "mir2.characters_item", "file": "characters_item.tsv",
                    "columns": [{"name": "PlayerId"}, {"name": "Position"},
                                {"name": "MakeIndex"}, {"name": "Dura"}]}]}), encoding="utf-8")
                (d / "db" / "characters_item.tsv").write_text(
                    "PlayerId\tPosition\tMakeIndex\tDura\n1001\t1\t42\t3997\n",
                    encoding="utf-8")
                (d / "rng-stream.tsv").write_text(rng_text, encoding="utf-8")
            return root

        ok_root = write_root("root-ok", csharp.read_text(encoding="utf-8"),
                             rust.read_text(encoding="utf-8"))
        wired = analyze(ok_root)
        wired_rng = wired.get("rng_alignment") or {}
        require(wired["pair"]["classification"] == "SAME_SINGLE_WEAR"
                and wired_rng.get("status") == "DIAGNOSED"
                and wired_rng.get("classification")
                == "ATTACK_LOCAL_SHAPE_EQUAL_GLOBAL_STREAM_INTERLEAVED"
                and (wired_rng.get("attack_windows_total") or {}).get("rust") == 1,
                "analyze() 接线：两侧合法账本 ⇒ rng_alignment 真挂上（改 if False 即 FAIL）")

        bad_root = write_root(
            "root-bad",
            "NO_HEADER\nSEED_PROBE 1 2 3\n0\tNext\t5\t4\tcombat.rs:1151\n",
            rust.read_text(encoding="utf-8"))
        bad_wired = analyze(bad_root)
        bad_rng = bad_wired.get("rng_alignment") or {}
        require(bad_rng.get("status") == "UNDECIDABLE"
                and "rng-stream" in bad_rng.get("reason", ""),
                "analyze() 接线牙：一侧坏头 ⇒ rng_alignment=UNDECIDABLE 且带 reason")

    expected_checks = 22
    passed = sum(condition for condition, _name in cases)
    print("CHECK %d/%d" % (passed, len(cases)))
    return 0 if (passed == len(cases) == expected_checks and cases) else 1


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root")
    parser.add_argument("--json")
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    if args.check:
        return check()
    if not args.root:
        parser.error("分析真实产物必须给 --root")
    try:
        result = analyze(args.root)
    except (InputError, OSError, KeyError, ValueError) as exc:
        print("INPUT ERROR: %s" % exc, file=sys.stderr)
        return 2
    text = json.dumps(result, ensure_ascii=False, indent=2) + "\n"
    if args.json:
        Path(args.json).write_text(text, encoding="utf-8", newline="\n")
    sys.stdout.write(text)
    return 0


if __name__ == "__main__":
    sys.exit(main())
