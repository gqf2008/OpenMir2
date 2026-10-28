# -*- coding: utf-8 -*-
"""run6 equip_struck_save 的武器磨损/落库只读重建（卡 openmir2-equip-struck-dura-db-triage）。

不连接 DB、不起栈。直接读已冻结的 frames/c2s/fixture/save_settle/db TSV：
  python -I -B tools/window/equip_struck_dura_triage.py \
    --root E:/tmp/window/run6/equip_struck_save --json E:/tmp/dev-2/dura-run6-analysis.json

--check 跑合成正负例与变异牙（无攻击、同值、run6 形态、快照不一致、重复 642、
比较器非恒等；RNG 账本存在/缺失/坏头三臂），真实状态只从 tempdir 读，不写仓库。
"""
import argparse
import csv
import hashlib
import json
import sys
import tempfile
from pathlib import Path

CM_HIT = 3014
SM_DURACHANGE = 642
WEAPON_SLOT = 1
WEAR_MIN = 2
WEAR_MAX = 6  # Random(5)+2，WeaponStrong=0


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
    if "INJECT_OK" not in head or "SEED_PROBE" not in head:
        return {"path": str(path), "exists": True, "valid": False,
                "detail": "RNG 账本缺 INJECT_OK/SEED_PROBE 自证头"}
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
        valid_path.write_text("INJECT_OK\nSEED_PROBE\n0\tNext\t5\t4\tcombat.rs:1151\n",
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

    expected_checks = 9
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
