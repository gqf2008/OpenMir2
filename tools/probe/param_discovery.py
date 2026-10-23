#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""param_discovery.py — 对拍台的「参数发现」阶段（卡 openmir2-window-param-discovery）。

背景：15 个场景里 9 个声明了 `required` 运行时变量（NPC/商人 ActorId、物品 MakeIndex、自身坐标），
缺 `--var` 即加载期 `scenario-error`(rc=2)。本模块负责**从已录 s2c 帧里推导这些变量**；
推导不出来就 fail-closed（列出缺哪个），**绝不用默认值/猜值**（与 DSM 的 `required` 语义一致）。

推导规则（每条都写明来源帧与字段；规则之外一律 missing）：

| 变量 | 来源 | 说明 |
|---|---|---|
| `self_x` / `self_y` | `SM_LOGON(50)` 的 `param` / `tag` | 实测与该角色在 DB `characters.CX/CY` 一致 |
| `npc_actor` | `SM_TURN(10)` 中**首个 `recog != 自身`** 的 actor | 自身 actor 取 `SM_LOGON(50).recog` |
| `merchant_actor` | 同上（同一来源） | 部署里首个可见 actor 即赌场庄家（`SM_MERCHANTSAY(643)` 正文可佐证） |
| `king_actor` | 不推 | 帧里没有"该 actor 是国王"的信息 ⇒ fail-closed |
| `make_index` / `item_name` | 需 `SM_BAGITEMS(201)` | 现网 22+ 个 run 无 201 ⇒ 现阶段恒 missing |
| `move_x` / `move_y` / `target_x` / `target_y` / `takeon_where` / `hit_dir` | 不推 | 已录帧无 `SM_SPACEMOVE/SM_WALK`、无自身 `SM_TURN` ⇒ fail-closed |

用法：

```sh
python -I -B tools/probe/param_discovery.py selftest              # 构造级用例 + 牙（离线）
python -I -B tools/probe/param_discovery.py <frames.ndjson> npc_actor,self_x
```

输出是 JSON：`{"values": {var: {value, source, seq, ident}}, "missing": [...]}`；`missing` 非空时退出码 2。
"""
import json
import os
import sys

SM_TURN = 10
SM_LOGON = 50

#: 变量 -> 来源描述（写进输出，便于复审复算）
RULES = {
    "self_x": "SM_LOGON(50).param",
    "self_y": "SM_LOGON(50).tag",
    "npc_actor": "SM_TURN(10) 首个 recog != self",
    "merchant_actor": "SM_TURN(10) 首个 recog != self（同 npc 来源）",
}

#: 明示「不可推导」的变量，命中即 missing（有据可查，避免有人以为漏实现）
UNDERIVABLE = {
    "make_index": "需 SM_BAGITEMS(201)；现网 run 无 201",
    "item_name": "同上（SM_BAGITEMS(201)）",
    "king_actor": "帧里无『该 actor 是国王』的信息",
    "move_x": "已录帧无 SM_SPACEMOVE/SM_WALK，亦无自身 SM_TURN",
    "move_y": "同上",
    "target_x": "帧里无目标格坐标",
    "target_y": "同上",
    "takeon_where": "无来源帧（需操作者显式 --var）",
    "hit_dir": "无来源帧（需操作者显式 --var）",
}


def load_frames(path):
    frames = []
    with open(path, "r", encoding="utf-8") as fh:
        for line in fh:
            line = line.strip()
            if line:
                frames.append(json.loads(line))
    return frames


def derive(frames, required, field_map=None):
    """按规则推导 required 变量。返回 (values, missing)；不做任何兜底。"""
    values, missing = {}, []
    login = next((f for f in frames if f.get("ident") == SM_LOGON), None)
    self_actor = login.get("recog") if login else None
    turn = next((f for f in frames
                 if f.get("ident") == SM_TURN and self_actor is not None
                 and f.get("recog") != self_actor), None)

    fmap = {"self_x": "param", "self_y": "tag",
            "npc_actor": "recog", "merchant_actor": "recog"}
    if field_map:
        fmap.update(field_map)

    for var in required:
        if var in UNDERIVABLE:
            missing.append(var)
            continue
        frame = login if var in ("self_x", "self_y") else (turn if var in fmap else None)
        field = fmap.get(var)
        if frame is None or field is None or frame.get(field) is None:
            missing.append(var)
            continue
        values[var] = {"value": int(frame[field]), "source": RULES[var],
                       "seq": frame.get("seq"), "ident": frame.get("ident")}
    return values, missing


def render_vars(values):
    """-> ['--var', 'k=v', ...]（按变量名排序，保证确定性）。"""
    out = []
    for key in sorted(values):
        out += ["--var", "%s=%d" % (key, values[key]["value"])]
    return out


# --------------------------------------------------------------------------
# 构造级用例（≥6）+ 牙（≥2），全部离线
# --------------------------------------------------------------------------
def _f(seq, ident, recog=0, param=0, tag=0):
    return {"seq": seq, "ident": ident, "recog": recog, "param": param, "tag": tag}


def _fixture(with_npc=True, with_login=True):
    frames = []
    if with_login:
        frames.append(_f(1, SM_LOGON, recog=191, param=7, tag=12))
    if with_npc:
        frames.append(_f(2, SM_TURN, recog=191, param=0, tag=0))   # 自己转身：必须排除
        frames.append(_f(3, SM_TURN, recog=119, param=8, tag=10))  # 目标 NPC
    return frames


def selftest():
    fails = []

    def case(name, fn):
        try:
            fn()
            print("  [ok] %s" % name)
        except AssertionError as ex:
            fails.append("%s: %s" % (name, ex))

    def c1():
        v, m = derive(_fixture(), ["self_x", "self_y", "npc_actor", "merchant_actor"])
        assert not m, m
        assert v["self_x"]["value"] == 7 and v["self_y"]["value"] == 12, v
        assert v["npc_actor"]["value"] == 119 and v["merchant_actor"]["value"] == 119, v
        assert v["npc_actor"]["ident"] == SM_TURN and v["npc_actor"]["seq"] == 3, v
        assert render_vars(v) == ["--var", "merchant_actor=119", "--var", "npc_actor=119",
                                  "--var", "self_x=7", "--var", "self_y=12"], render_vars(v)

    def c2():
        v, m = derive(_fixture(with_npc=False), ["npc_actor"])
        assert m == ["npc_actor"] and not v, (v, m)

    def c3():
        v, m = derive(_fixture(with_login=False), ["self_x", "self_y"])
        assert m == ["self_x", "self_y"] and not v, (v, m)

    def c4():
        # 只有自己的 SM_TURN 时不得把自己当 NPC
        v, m = derive(_fixture(with_npc=False, with_login=True), ["npc_actor"])
        assert m == ["npc_actor"], (v, m)

    def c5():
        # 无 SM_BAGITEMS(201) ⇒ make_index/item_name 恒 missing
        v, m = derive(_fixture(), ["make_index", "item_name"])
        assert m == ["make_index", "item_name"] and not v, (v, m)

    def c6():
        # 确定性：同夹具两次跑同结果
        a = derive(_fixture(), ["self_x", "npc_actor"])
        b = derive(_fixture(), ["self_x", "npc_actor"])
        assert a == b, (a, b)

    def c7():
        # 混入不可推导变量时，可推导的仍给出、缺的逐个点名
        v, m = derive(_fixture(), ["self_x", "king_actor", "target_x"])
        assert v["self_x"]["value"] == 7, v
        assert m == ["king_actor", "target_x"], m

    def tooth1():
        # 牙①：把 self_x 错取成 tag ⇒ 值变成 12，与真值 7 不同（用例会红）
        good, _ = derive(_fixture(), ["self_x"])
        bad, _ = derive(_fixture(), ["self_x"], field_map={"self_x": "tag"})
        assert good["self_x"]["value"] == 7, good
        assert bad["self_x"]["value"] == 12, bad
        assert bad["self_x"]["value"] != good["self_x"]["value"]

    def tooth2():
        # 牙②：发现阶段静默返回空集合（把 missing 吞掉）必须能被用例识别
        v, m = derive(_fixture(with_npc=False), ["npc_actor"])
        assert m, "missing 被静默吞掉就无处可判——本用例必须红"

    case("① 含 NPC+自身 ⇒ 值与来源正确（含 --var 渲染）", c1)
    case("② 缺 NPC ⇒ fail-closed", c2)
    case("③ 缺自身坐标 ⇒ fail-closed", c3)
    case("④ 自己的 SM_TURN 不得当 NPC", c4)
    case("⑤ 无 201 ⇒ make_index/item_name 恒 missing", c5)
    case("⑥ 同夹具两次跑同结果（确定性）", c6)
    case("⑦ 混合：可推导的给、不可推导的逐个点名", c7)
    case("牙① self_x 错取 tag ⇒ 值与真值不同", tooth1)
    def c8():
        # check-all 的接线口径：可推导场景 rc=0、不可推导场景 rc=2 并点名
        import tempfile
        tmp = tempfile.mkdtemp(prefix="pd-checkall-")
        frames_path = os.path.join(tmp, "frames.ndjson")
        with open(frames_path, "w", encoding="utf-8") as fh:
            for f in _fixture():
                print(json.dumps(f), file=fh)
        ok_scen = {"name": "ok_scen", "required": ["self_x"],
                   "steps": [{"wait_for": 50, "timeout_ms": 10}]}
        bad_scen = {"name": "bad_scen", "required": ["make_index"],
                    "steps": [{"wait_for": 50, "timeout_ms": 10}]}
        for name, doc in (("ok.json", ok_scen), ("bad.json", bad_scen)):
            with open(os.path.join(tmp, name), "w", encoding="utf-8") as fh:
                json.dump(doc, fh)
        rows = dict((n, rc) for n, rc, _ in check_all(frames_path, tmp))
        assert rows["ok.json"] == 0, rows
        assert rows["bad.json"] == 2, rows

    case("⑧ check-all：可推导 rc=0 / 不可推导 rc=2", c8)
    case("牙② 静默空集合 ⇒ 用例红", tooth2)
    if fails:
        print("param_discovery selftest: FAIL")
        for f in fails:
            print("  -", f)
        return 1
    print("param_discovery selftest: OK (8 cases + 2 teeth)")
    return 0


def check_all(frames_path, scenarios_dir):
    """对每个场景：从 frames 推导它的 required ⇒ 拼 --var 跑 --check-scenario。

    返回 [(scenario, rc, detail)]；rc 口径：0=通过 / 2=发现不出必需变量或场景校验失败。
    """
    import glob
    import os
    import subprocess
    frames = load_frames(frames_path)
    here = os.path.dirname(os.path.abspath(__file__))
    probe = os.path.join(here, "mir2_probe.py")
    rows = []
    for path in sorted(glob.glob(os.path.join(scenarios_dir, "*.json"))):
        with open(path, "r", encoding="utf-8") as fh:
            scenario = json.load(fh)
        required = scenario.get("required") or []
        values, missing = derive(frames, required)
        if missing:
            rows.append((os.path.basename(path), 2, "missing: %s" % ",".join(missing)))
            continue
        cmd = [sys.executable, "-I", "-B", probe, "--check-scenario", "--scenario", path]
        cmd += render_vars(values)
        proc = subprocess.run(cmd, capture_output=True, text=True, encoding="utf-8")
        detail = (proc.stdout or proc.stderr or "").strip().splitlines()
        rows.append((os.path.basename(path), proc.returncode, detail[-1] if detail else ""))
    return rows


def main(argv=None):
    argv = list(sys.argv[1:] if argv is None else argv)
    if not argv:
        print(__doc__)
        return 2
    if argv[0] == "selftest":
        return selftest()
    if argv[0] == "check-all":
        if len(argv) < 2:
            print("usage: param_discovery.py check-all <frames.ndjson> [scenarios_dir]", file=sys.stderr)
            return 2
        scen = argv[2] if len(argv) > 2 else os.path.join(
            os.path.dirname(os.path.abspath(__file__)), "scenarios")
        rows = check_all(argv[1], scen)
        bad = [r for r in rows if r[1] != 0]
        for name, rc, detail in rows:
            print("%-28s rc=%d  %s" % (name, rc, detail[:100]))
        print("SUMMARY scenarios=%d ok=%d rc2=%d" % (len(rows), len(rows) - len(bad), len(bad)))
        return 0 if not bad else 2
    if len(argv) < 2:
        sys.stderr.write("usage: param_discovery.py selftest | <frames.ndjson> <var1,var2,...>\n")
        return 2
    frames = load_frames(argv[0])
    required = [x.strip() for x in argv[1].split(",") if x.strip()]
    values, missing = derive(frames, required)
    print(json.dumps({"values": values, "missing": missing,
                      "argv": render_vars(values)}, ensure_ascii=False, indent=1))
    return 0 if not missing else 2


if __name__ == "__main__":
    sys.exit(main())
