#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""param_discovery.py — 对拍台的「参数发现」阶段（卡 openmir2-window-param-discovery）。

背景：15 个场景里 9 个声明了 `required` 运行时变量（NPC/商人 ActorId、物品 MakeIndex、自身坐标），
缺 `--var` 即加载期 `scenario-error`(rc=2)。本模块负责**从已录 s2c 帧里推导这些变量**；
推导不出来就 fail-closed（逐个点名缺哪个），**绝不用默认值/猜值**（与 DSL 的 `required` 语义一致）。

「推导」与「规则」严格分栏（返工后；依据与代价见 docs/param-discovery-design-20261023.md §2）：

| 变量 | 类别 | 来源 | oracle / 依据 |
|---|---|---|---|
| `self_x` / `self_y` | 推导 | `SM_LOGON(50)` 的 `param` / `tag` | `PlayObject.cs:1128-1131 SendLogon()` `MakeMessage(SM_LOGON, ActorId, CurrX, CurrY, …)` |
| `merchant_actor` | 推导 | `SM_MERCHANTSAY(643)` 的 `recog` | 643 是 NPC **应答点击**才发的面板（`NormNpc.Click → GotoLable(@main) → RM_MERCHANTSAY`），`PlayObject.Message.cs:1721-1724` |
| `npc_actor` | 优先推导，退化为**规则** | 有 643 就取 643 的 `recog`（实证）；否则取 `SM_TURN(10)` 首个 `recog != self` | 退化规则是**部署假设**（非帧语义）：点错 actor 时后续步骤**响亮超时**，不会静默通过 |
| `king_actor` | 不可推导 | 帧里没有「该 actor 是国王」的信息 | fail-closed |
| `make_index` / `item_name` | 不可推导（现版） | 需 `SM_BAGITEMS(201)` 的包体 | 现网 22+ 个 run **无 201** ⇒ `bag_equip`/`eat_item` 恒红，201 规则另卡 |
| `move_*` / `target_*` / `takeon_where` / `hit_dir` | 不可推导 | 无来源帧 | fail-closed（可由操作者 `--var` 显式给） |

用法：

```sh
python -I -B tools/probe/param_discovery.py selftest                 # 构造级用例 + 牙（离线）

# 单次推导：输出 values/missing/argv，missing 非空 ⇒ rc=2
python -I -B tools/probe/param_discovery.py <frames.ndjson> npc_actor,self_x

# 逐场景：读每个场景的 required ⇒ 推导 ⇒ 拼 --var 跑 mir2_probe --check-scenario
python -I -B tools/probe/param_discovery.py check-all <frames.ndjson> [scenarios_dir]
python -I -B tools/probe/param_discovery.py check-all <frames> [dir] --allow-missing[=N]
```

`check-all` 的退出码（fail-closed 口径）：

- 默认：**任一**行非 0（`missing` 或 `error`）⇒ rc=2；
- `--allow-missing`：只放行本模块自己的 `missing` 行（不可推导已登记）；`error` 行（场景本身坏了）**仍 rc=2**；
- `--allow-missing=N`：放行的同时钉住 `missing` 行数必须恰好为 N，否则 rc=2（防止多坏了几条还悄悄放行）；
- **零场景判红**：`scenarios_dir` 不存在/不是目录、或目录里一个 `.json` 都没有 ⇒ rc=2（「检查了零个场景」绝不当绿）；
- **场景文件坏掉**（坏 JSON / 顶层非对象 / `required` 不是非空字符串数组 / `params` 非对象）⇒ 该行
  `cls=error`、`scenario-load: …`、整体 rc=2，`--allow-missing` 也不放行。

每行带分类（`cls`）：`derived`（required 全部由本模块**推导**，唯一算「发现能力已验证」）/
`rule`（required 里有变量来自退化**规则**，即部署假设，**不计入**发现能力）/
`no_required`（场景无需变量）/ `params_default`（**绿来自场景 params 硬编码默认值，不是发现值**）/
`missing`（本模块 fail-closed）/ `error`（场景校验或加载失败）。
"""
import json
import os
import sys

SM_TURN = 10
SM_LOGON = 50
SM_MERCHANTSAY = 643

#: 变量 -> 推导规则（写进输出，便于复审复算）；`kind` 区分推导与规则。
RULES = {
    "self_x": {
        "kind": "derived", "ident": SM_LOGON, "field": "param",
        "rule": "SM_LOGON(50).param",
        "oracle": "src/M2Server/Player/PlayObject.cs:1128-1131 SendLogon() "
                  "MakeMessage(SM_LOGON, ActorId, CurrX, CurrY, MakeWord(Dir, Light))",
    },
    "self_y": {
        "kind": "derived", "ident": SM_LOGON, "field": "tag",
        "rule": "SM_LOGON(50).tag",
        "oracle": "src/M2Server/Player/PlayObject.cs:1128-1131 SendLogon()（同上，tag=CurrY）",
    },
    "merchant_actor": {
        "kind": "derived", "ident": SM_MERCHANTSAY, "field": "recog",
        "rule": "SM_MERCHANTSAY(643).recog",
        "oracle": "NormNpc.Click → GotoLable(@main) → RM_MERCHANTSAY"
                  "（src/M2Server/Npc/NormNpc.cs:68-73,1615；"
                  "src/M2Server/Player/PlayObject.Message.cs:1721-1724）",
    },
    "npc_actor": {
        "kind": "derived", "ident": SM_MERCHANTSAY, "field": "recog",
        "rule": "SM_MERCHANTSAY(643).recog（有 643 时）",
        "oracle": "同 merchant_actor；没有 643 时退化为 RULES_FALLBACK",
    },
}

#: `npc_actor` 的退化规则：**规则（部署假设）而非推导**。点错 actor ⇒ 后续步骤响亮超时。
RULES_FALLBACK = {
    "npc_actor": {
        "kind": "rule", "ident": SM_TURN, "field": "recog",
        "rule": "SM_TURN(10) 首个 recog != self（部署假设）",
        "oracle": "src/M2Server/Player/PlayObject.Message.cs:1542 "
                  "MakeMessage(SM_TURN, ActorId, …)（只说明『该 actor 转了身』，不说明它是什么）",
    },
}

#: 可推导变量「这次为什么没推出来」的说明（single-derive 输出的 reasons）
REASONS = {
    "merchant_actor": "需 SM_MERCHANTSAY(643)：本 run 帧里没有 643（登录期收帧不含点击应答；"
                      "discovery run 必须点一次 NPC）",
    "npc_actor": "帧里既无 SM_MERCHANTSAY(643) 也无非自身的 SM_TURN(10)",
}

#: 明示「不可推导」的变量，命中即 missing（有据可查，避免有人以为漏实现）
UNDERIVABLE = {
    "make_index": "需 SM_BAGITEMS(201)；现网无 201 录制（本版未实现 201 规则，另卡）",
    "item_name": "同上（SM_BAGITEMS(201)）",
    "king_actor": "帧里无『该 actor 是国王』的信息",
    "move_x": "已录帧无 SM_SPACEMOVE/SM_WALK，亦无自身 SM_TURN",
    "move_y": "同上",
    "target_x": "帧里无目标格坐标",
    "target_y": "同上",
    "takeon_where": "无来源帧（需操作者显式 --var）",
    "hit_dir": "无来源帧（需操作者显式 --var）",
}


class DiscoveryError(Exception):
    """输入坏掉（坏行/非对象/字段非整数）⇒ 必须是干净的 rc=2，不是 traceback。"""


def load_frames(path):
    """读 frames.ndjson；坏行/非对象/读不开一律 DiscoveryError（带 file:lineno）。"""
    frames = []
    try:
        with open(path, "r", encoding="utf-8") as fh:
            for lineno, line in enumerate(fh, 1):
                line = line.strip()
                if not line:
                    continue
                try:
                    obj = json.loads(line)
                except ValueError as ex:
                    raise DiscoveryError("%s:%d: invalid JSON: %s" % (path, lineno, ex))
                if not isinstance(obj, dict):
                    raise DiscoveryError("%s:%d: frame must be a JSON object, got %s"
                                         % (path, lineno, type(obj).__name__))
                frames.append(obj)
    except OSError as ex:
        raise DiscoveryError("%s: cannot read frames: %s" % (path, ex))
    return frames


def _first(frames, ident, skip_recog=None):
    for frame in frames:
        if frame.get("ident") != ident:
            continue
        if skip_recog is not None and frame.get("recog") == skip_recog:
            continue
        return frame
    return None


def _value(frame, var, rule, field):
    """取字段并转 int；缺失 ⇒ None（调用方记 missing），非法值 ⇒ DiscoveryError。"""
    if frame is None:
        return None
    raw = frame.get(field)
    if raw is None:
        return None
    try:
        return int(raw)
    except (TypeError, ValueError):
        raise DiscoveryError("%s: %s.%s is not an integer: %r"
                             % (var, rule["rule"], field, raw))


def derive(frames, required, field_map=None):
    """按规则推导 required 变量。返回 (values, missing)；不做任何兜底。

    `field_map` 只用于牙（把某个变量故意指到别的字段）；正常调用不传。
    """
    values, missing = {}, []
    login = _first(frames, SM_LOGON)
    self_actor = login.get("recog") if login else None
    said = _first(frames, SM_MERCHANTSAY)          # 真的应答过点击的 NPC（实证来源）
    turned = None
    if self_actor is not None:
        turned = _first(frames, SM_TURN, skip_recog=self_actor)  # 退化规则来源

    overrides = dict(field_map or {})

    for var in required:
        if var in UNDERIVABLE:
            missing.append(var)
            continue
        if var == "npc_actor" and said is None:
            rule = RULES_FALLBACK["npc_actor"]
            frame = turned
        else:
            rule = RULES.get(var)
            if rule is None:                       # 规则表里没有 ⇒ fail-closed（不猜）
                missing.append(var)
                continue
            frame = {SM_LOGON: login, SM_MERCHANTSAY: said}.get(rule["ident"])
        field = overrides.get(var, rule["field"])
        value = _value(frame, var, rule, field)
        if value is None:
            missing.append(var)
            continue
        values[var] = {"value": value, "kind": rule["kind"], "rule": rule["rule"],
                       "oracle": rule["oracle"],
                       "source": {"ident": frame.get("ident"), "field": field,
                                  "seq": frame.get("seq")}}
    return values, missing


def render_vars(values):
    """-> ['--var', 'k=v', ...]（按变量名排序，保证确定性）。"""
    out = []
    for key in sorted(values):
        out += ["--var", "%s=%d" % (key, values[key]["value"])]
    return out


#: 分类；`params_default` = 绿来自场景 params 硬编码默认值，**不是**发现值；
#: `rule` = required 里有变量来自退化规则（部署假设），**不计入**「发现能力已验证」。
CLS_DERIVED = "derived"
CLS_RULE = "rule"
CLS_NO_REQUIRED = "no_required"
CLS_PARAMS_DEFAULT = "params_default"
CLS_MISSING = "missing"
CLS_ERROR = "error"


def _load_scenario(path):
    """读并校验场景文件；坏 JSON / 非对象 / required 不是非空字符串数组 ⇒ DiscoveryError。

    场景文件是 CLI 可指定的输入（可传自定义 scenarios_dir），与 frames 坏行同口径：
    必须是干净的 rc=2，不许 traceback / rc=1，也不许把 `required: "npc_actor"` 按字符拆开。
    """
    try:
        with open(path, "r", encoding="utf-8") as fh:
            doc = json.load(fh)
    except (OSError, ValueError) as ex:
        raise DiscoveryError("%s: cannot load scenario: %s" % (path, ex))
    if not isinstance(doc, dict):
        raise DiscoveryError("%s: scenario must be a JSON object, got %s"
                             % (path, type(doc).__name__))
    required = doc.get("required")
    if required is not None:
        if not isinstance(required, list) or not all(
                isinstance(item, str) and item for item in required):
            raise DiscoveryError("%s: scenario.required must be a list of non-empty strings, got %r"
                                 % (path, required))
    params = doc.get("params")
    if params is not None and not isinstance(params, dict):
        raise DiscoveryError("%s: scenario.params must be an object, got %r" % (path, params))
    return doc


def check_all(frames_path, scenarios_dir):
    """对每个场景：从 frames 推导它的 required ⇒ 拼 --var 跑 `mir2_probe --check-scenario`。

    返回 [(scenario, rc, detail, cls)]；rc 口径：0=通过 / 2=发现不出必需变量或场景校验失败。

    fail-closed 两处：`scenarios_dir` 不是目录 ⇒ DiscoveryError（rc=2）；
    单个场景文件坏掉 ⇒ 该行 cls=error（整体 rc=2，`--allow-missing` 也不放行）。
    「一个场景都没检查」绝不当绿——由 check_all_main 对空 rows 判 rc=2。
    """
    import glob
    import subprocess

    if not os.path.isdir(scenarios_dir):
        raise DiscoveryError("%s: scenarios dir does not exist or is not a directory"
                             % scenarios_dir)
    frames = load_frames(frames_path)
    here = os.path.dirname(os.path.abspath(__file__))
    probe = os.path.join(here, "mir2_probe.py")
    rows = []
    for path in sorted(glob.glob(os.path.join(scenarios_dir, "*.json"))):
        name = os.path.basename(path)
        try:
            scenario = _load_scenario(path)
        except DiscoveryError as ex:
            rows.append((name, 2, "scenario-load: %s" % ex, CLS_ERROR))
            continue
        required = scenario.get("required") or []
        params = scenario.get("params") or {}
        values, missing = derive(frames, required)
        if missing:
            # 本模块自己兜住 DSL 的 required：这一行必须是 missing（不许交给 mir2_probe 崩）
            rows.append((name, 2, "missing: %s" % ",".join(missing), CLS_MISSING))
            continue
        cmd = [sys.executable, "-I", "-B", probe, "--check-scenario", "--scenario", path]
        cmd += render_vars(values)
        proc = subprocess.run(cmd, capture_output=True, text=True, encoding="utf-8")
        detail = (proc.stdout or proc.stderr or "").strip().splitlines()
        detail = detail[-1] if detail else ""
        if proc.returncode == 0:
            if required:
                # 推导与规则分栏：只要有变量来自退化规则，这一行就不算「发现能力已验证」
                cls = CLS_RULE if any(v["kind"] == "rule" for v in values.values()) \
                    else CLS_DERIVED
            elif params:
                cls = CLS_PARAMS_DEFAULT
            else:
                cls = CLS_NO_REQUIRED
        else:
            cls = CLS_ERROR
        rows.append((name, proc.returncode, detail, cls))
    return rows


def _parse_check_all_args(args):
    """-> (positional, allow, expect_missing)；用法错误时返回 (None, None, msg)。"""
    allow, expect, rest = False, None, []
    i = 0
    while i < len(args):
        arg = args[i]
        if arg == "--allow-missing":
            allow = True
            if i + 1 < len(args) and args[i + 1].isdigit():
                expect = int(args[i + 1])
                i += 1
        elif arg.startswith("--allow-missing="):
            allow = True
            value = arg.split("=", 1)[1]
            if not value.isdigit():
                return None, None, "--allow-missing=N needs an integer, got %r" % value
            expect = int(value)
        elif arg in ("-h", "--help"):
            return None, None, None
        elif arg.startswith("-") and arg != "-":
            return None, None, "unknown option %r" % arg
        else:
            rest.append(arg)
        i += 1
    return rest, allow, expect


def check_all_main(args):
    parsed, allow, expect = _parse_check_all_args(args)
    if parsed is None:
        if expect is not None:                       # 用法错误
            print("param_discovery: %s" % expect, file=sys.stderr)
            return 2
        print(__doc__)                               # --help
        return 0
    if not parsed:
        print("usage: param_discovery.py check-all <frames.ndjson> [scenarios_dir] "
              "[--allow-missing[=N]]", file=sys.stderr)
        return 2
    scen = parsed[1] if len(parsed) > 1 else os.path.join(
        os.path.dirname(os.path.abspath(__file__)), "scenarios")
    try:
        rows = check_all(parsed[0], scen)
    except DiscoveryError as ex:
        print("param_discovery: %s" % ex, file=sys.stderr)
        return 2
    if not rows:
        # 「一个场景都没检查」绝不当绿（与 goal_gate 缺 counts、recon 唯一 tracked 同族）
        print("param_discovery: no scenarios found under %s — 零场景判绿是 fail-open，rc=2"
              % scen, file=sys.stderr)
        return 2
    for name, rc, detail, cls in rows:
        print("%-28s rc=%d  %-14s %s" % (name, rc, cls, detail[:100]))
    bad = [r for r in rows if r[1] != 0]
    missing = [r for r in rows if r[3] == CLS_MISSING]
    errors = [r for r in rows if r[3] == CLS_ERROR]
    print("SUMMARY scenarios=%d ok=%d rc2=%d derived=%d rule=%d no_required=%d "
          "params_default=%d missing=%d error=%d"
          % (len(rows), len(rows) - len(bad), len(bad),
             sum(1 for r in rows if r[3] == CLS_DERIVED),
             sum(1 for r in rows if r[3] == CLS_RULE),
             sum(1 for r in rows if r[3] == CLS_NO_REQUIRED),
             sum(1 for r in rows if r[3] == CLS_PARAMS_DEFAULT),
             len(missing), len(errors)))
    if errors:
        print("param_discovery: %d scenario error(s) — 场景本身坏了，--allow-missing 也不放行"
              % len(errors), file=sys.stderr)
        return 2
    if missing and not allow:
        print("param_discovery: %d scenario(s) fail-closed on missing vars "
              "（登记后可 --allow-missing=N 放行）" % len(missing), file=sys.stderr)
        return 2
    if allow and expect is not None and len(missing) != expect:
        print("param_discovery: expected %d missing scenario(s), got %d — 不可推导清单变了"
              % (expect, len(missing)), file=sys.stderr)
        return 2
    return 0


# --------------------------------------------------------------------------
# 构造级用例（≥6）+ 牙（≥2），全部离线
# --------------------------------------------------------------------------
def _f(seq, ident, recog=0, param=0, tag=0):
    return {"seq": seq, "ident": ident, "recog": recog, "param": param, "tag": tag}


def _fixture(with_npc=True, with_login=True, merchant_say=False):
    """`merchant_say=True` 模拟「点过并拿到 643」的 run；默认只有登录期收帧。"""
    frames = []
    if with_login:
        frames.append(_f(1, SM_LOGON, recog=191, param=7, tag=12))
    if with_npc:
        frames.append(_f(2, SM_TURN, recog=191, param=0, tag=0))   # 自己转身：必须排除
        frames.append(_f(3, SM_TURN, recog=119, param=8, tag=10))  # 目标 NPC
    if merchant_say:
        frames.append(_f(4, SM_MERCHANTSAY, recog=119, param=0, tag=0))
    return frames


def _tmp_check_all(scenarios, frames=None):
    """写临时 frames + 场景目录，返回 check_all 的行（供用例/牙断言行为）。"""
    import tempfile
    tmp = tempfile.mkdtemp(prefix="pd-checkall-")
    frames_path = os.path.join(tmp, "frames.ndjson")
    with open(frames_path, "w", encoding="utf-8") as fh:
        for frame in (frames if frames is not None else _fixture(merchant_say=True)):
            print(json.dumps(frame), file=fh)
    for name, doc in scenarios.items():
        with open(os.path.join(tmp, name), "w", encoding="utf-8") as fh:
            json.dump(doc, fh)
    return dict((r[0], r) for r in check_all(frames_path, tmp))


def _quiet_check_all_main(argv):
    """跑 check_all_main 但不刷屏（用例/牙只关心 rc）。"""
    import contextlib
    import io
    with contextlib.redirect_stdout(io.StringIO()), \
            contextlib.redirect_stderr(io.StringIO()):
        return check_all_main(argv)


def selftest():
    fails = []

    def case(name, fn):
        try:
            fn()
            print("  [ok] %s" % name)
        except AssertionError as ex:
            fails.append("%s: %s" % (name, ex))

    def c1():
        v, m = derive(_fixture(merchant_say=True),
                      ["self_x", "self_y", "npc_actor", "merchant_actor"])
        assert not m, m
        assert v["self_x"]["value"] == 7 and v["self_y"]["value"] == 12, v
        assert v["npc_actor"]["value"] == 119 and v["merchant_actor"]["value"] == 119, v
        # 有 643 时 npc_actor 走推导（同 merchant），来源是 643 那帧
        assert v["npc_actor"]["kind"] == "derived", v
        assert v["merchant_actor"]["source"] == {"ident": SM_MERCHANTSAY, "field": "recog",
                                                 "seq": 4}, v
        assert v["self_x"]["source"]["ident"] == SM_LOGON, v
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
        # 无 SM_BAGITEMS(201) ⇒ make_index/item_name 恒 missing（本版未实现 201 规则）
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

    def c8():
        # check-all 接线口径：可推导场景 rc=0；缺变量场景 rc=2 且**由本模块**报 missing:
        rows = _tmp_check_all({
            "ok.json": {"name": "ok_scen", "required": ["self_x"],
                        "steps": [{"wait_for": 50, "timeout_ms": 10}]},
            "bad.json": {"name": "bad_scen", "required": ["make_index"],
                         "steps": [{"wait_for": 50, "timeout_ms": 10}]},
        })
        assert rows["ok.json"][1] == 0, rows["ok.json"]
        assert rows["ok.json"][3] == CLS_DERIVED, rows["ok.json"]
        assert rows["bad.json"][1] == 2, rows["bad.json"]
        assert rows["bad.json"][3] == CLS_MISSING, rows["bad.json"]
        assert rows["bad.json"][2].startswith("missing:"), rows["bad.json"]

    def c9():
        # 分类诚实：只有 params 默认值的场景（dice_chain 形态）算 params_default，不算发现绿
        rows = _tmp_check_all({
            "diceish.json": {"name": "diceish", "params": {"npc_actor": 119},
                             "steps": [{"wait_for": 50, "timeout_ms": 10}]},
            "plain.json": {"name": "plain",
                           "steps": [{"wait_for": 50, "timeout_ms": 10}]},
        })
        assert rows["diceish.json"][1] == 0 and rows["diceish.json"][3] == CLS_PARAMS_DEFAULT, \
            rows["diceish.json"]
        assert rows["plain.json"][3] == CLS_NO_REQUIRED, rows["plain.json"]

    def c10():
        # --allow-missing：只放行 missing；坏场景(error)仍 rc=2；钉住的 N 不符也 rc=2
        import tempfile
        tmp = tempfile.mkdtemp(prefix="pd-allowmiss-")
        frames_path = os.path.join(tmp, "frames.ndjson")
        with open(frames_path, "w", encoding="utf-8") as fh:
            for frame in _fixture():
                print(json.dumps(frame), file=fh)
        scen = os.path.join(tmp, "scen")
        os.makedirs(scen)
        docs = {
            "ok.json": {"name": "ok", "required": ["self_x"],
                        "steps": [{"wait_for": 50, "timeout_ms": 10}]},
            "miss.json": {"name": "miss", "required": ["make_index"],
                          "steps": [{"wait_for": 50, "timeout_ms": 10}]},
            "broken.json": {"name": "broken", "required": ["self_x"],
                            "steps": [{"no_such_step": 1}]},
        }
        for name, doc in docs.items():
            with open(os.path.join(scen, name), "w", encoding="utf-8") as fh:
                json.dump(doc, fh)
        base = [frames_path, scen]                       # check_all_main 的参数不含 "check-all"

        def quiet(argv):
            return _quiet_check_all_main(argv)

        assert quiet(base) == 2, "默认必须 fail-closed"
        assert quiet(base + ["--allow-missing"]) == 2, "error 行不许被放行"
        assert quiet(base + ["--allow-missing=2"]) == 2, "实际 missing=1，钉 2 必须红"
        os.remove(os.path.join(scen, "broken.json"))     # 去掉坏场景后
        assert quiet(base + ["--allow-missing=1"]) == 0, "登记 1 条缺失应放行"
        assert quiet(base + ["--allow-missing=2"]) == 2, "钉住的 N 不符必须红"

    def c11():
        # 坏行必须干净 rc=2（不能 traceback / rc=1）
        import tempfile
        tmp = tempfile.mkdtemp(prefix="pd-badline-")
        bad = os.path.join(tmp, "frames.ndjson")
        with open(bad, "w", encoding="utf-8") as fh:
            fh.write('{"seq": 1, "ident": 50}\n')
            fh.write('{"seq": 2, "ident": 10, oops\n')
        try:
            load_frames(bad)
        except DiscoveryError as ex:
            assert "%s:2" % bad in str(ex), ex
        else:
            raise AssertionError("坏行必须抛 DiscoveryError")

    def tooth1():
        # 牙①：把 self_x 错取成 tag ⇒ 值变成 12，与真值 7 不同（用例会红）
        good, _ = derive(_fixture(), ["self_x"])
        bad, _ = derive(_fixture(), ["self_x"], field_map={"self_x": "tag"})
        assert good["self_x"]["value"] == 7, good
        assert bad["self_x"]["value"] == 12, bad
        assert bad["self_x"]["value"] != good["self_x"]["value"]

    def tooth2():
        # 牙②：check_all 必须**自己**在 missing 时拦下（cls=missing、detail 以 missing: 开头），
        # 而不是静默交给 mir2_probe 兜底。把 check_all 的 `if missing:` 改成 `if False:` ⇒
        # 这一行会变成 cls=error / detail 以 scenario-error: 开头 ⇒ 下面断言红（reviewer 的变异 M2）。
        rows = _tmp_check_all({
            "only.json": {"name": "only", "required": ["make_index"],
                          "steps": [{"wait_for": 50, "timeout_ms": 10}]},
        })
        row = rows["only.json"]
        assert row[3] == CLS_MISSING, "check_all 没自己拦 missing（cls=%s）" % row[3]
        assert row[2].startswith("missing:"), "detail 不是本模块的 %r" % row[2]

    def tooth3():
        # 牙③（reviewer blocking-1 的回归牙）：只有「第三方 actor 转身」、没有 643 的帧里，
        # merchant_actor 必须 fail-closed——任意第三方 actor 不得被当商人（旧规则此处 rc=0）。
        frames = [_f(1, SM_LOGON, recog=191, param=7, tag=12),
                  _f(2, SM_TURN, recog=191, param=0, tag=0),
                  _f(3, SM_TURN, recog=777, param=1, tag=1)]
        v, m = derive(frames, ["merchant_actor"])
        assert m == ["merchant_actor"] and not v, (v, m)
        rows = _tmp_check_all({
            "merchantish.json": {"name": "merchantish", "required": ["merchant_actor"],
                                 "steps": [{"wait_for": 643, "timeout_ms": 10}]},
        }, frames=frames)
        assert rows["merchantish.json"][1] == 2, rows["merchantish.json"]
        assert rows["merchantish.json"][3] == CLS_MISSING, rows["merchantish.json"]

    def c12():
        # 单场景推导路径的空判据 fail-open：一个变量都没要求 ⇒ rc=2（不许「什么都没推」判绿）。
        # 空 frames ✅ 已由「required 全部 missing ⇒ rc=2」兜住；这条补的是 required 本身为空。
        import tempfile
        tmp = tempfile.mkdtemp(prefix="pd-novars-")
        frames_path = os.path.join(tmp, "frames.ndjson")
        with open(frames_path, "w", encoding="utf-8") as fh:
            for frame in _fixture():
                print(json.dumps(frame), file=fh)
        import contextlib
        import io

        def quiet(argv):
            with contextlib.redirect_stdout(io.StringIO()), \
                    contextlib.redirect_stderr(io.StringIO()):
                return main(argv)

        assert quiet([frames_path, ""]) == 2, "空 --var 列表必须 rc=2"
        assert quiet([frames_path, " , "]) == 2, "只有分隔符/空白的 --var 列表必须 rc=2"
        # 空 frames + 有 required ⇒ missing ⇒ rc=2（回归：这条以前也是对的）
        empty = os.path.join(tmp, "empty.ndjson")
        open(empty, "w").close()
        assert quiet([empty, "self_x"]) == 2, "空 frames + required 必须 rc=2"

    def tooth4():
        # 牙④（reviewer C-1）：零场景判绿是 fail-open。
        # 变异：删掉 check_all_main 里 `if not rows: return 2` 与 check_all 的 isdir 检查 ⇒ 红。
        import tempfile
        tmp = tempfile.mkdtemp(prefix="pd-zeroscen-")
        frames_path = os.path.join(tmp, "frames.ndjson")
        with open(frames_path, "w", encoding="utf-8") as fh:
            for frame in _fixture():
                print(json.dumps(frame), file=fh)
        empty_dir = os.path.join(tmp, "empty-scenarios")
        os.makedirs(empty_dir)
        missing_dir = os.path.join(tmp, "no-such-dir")
        assert _quiet_check_all_main([frames_path, empty_dir]) == 2, "空目录（零场景）必须 rc=2"
        assert _quiet_check_all_main([frames_path, missing_dir]) == 2, "不存在的目录必须 rc=2"
        assert _quiet_check_all_main([frames_path, empty_dir, "--allow-missing"]) == 2, \
            "--allow-missing 也不许放行零场景"
        assert _quiet_check_all_main([frames_path, missing_dir, "--allow-missing=0"]) == 2, \
            "--allow-missing=0 也不许放行零场景"

    def tooth5():
        # 牙⑤（reviewer C-2）：场景文件坏掉必须干净落 cls=error / rc=2。
        # 变异：去掉 _load_scenario 的 try/except 与类型校验 ⇒ traceback rc=1 / detail 被按字符拆开 ⇒ 红。
        import tempfile
        tmp = tempfile.mkdtemp(prefix="pd-badscen-")
        frames_path = os.path.join(tmp, "frames.ndjson")
        with open(frames_path, "w", encoding="utf-8") as fh:
            for frame in _fixture():
                print(json.dumps(frame), file=fh)
        scen = os.path.join(tmp, "scen")
        os.makedirs(scen)
        cases = {
            "badjson.json": '{"name": "x", "steps": [\n',
            "array.json": '[1, 2, 3]\n',
            "reqstr.json": '{"name": "x", "required": "npc_actor", '
                           '"steps": [{"wait_for": 643, "timeout_ms": 10}]}\n',
            "reqint.json": '{"name": "x", "required": [123], '
                           '"steps": [{"wait_for": 643, "timeout_ms": 10}]}\n',
        }
        for name, text in cases.items():
            with open(os.path.join(scen, name), "w", encoding="utf-8") as fh:
                fh.write(text)
        rows = dict((r[0], r) for r in check_all(frames_path, scen))
        for name in cases:
            assert name in rows, rows
            assert rows[name][1] == 2, (name, rows[name])
            assert rows[name][3] == CLS_ERROR, (name, rows[name])
            assert rows[name][2].startswith("scenario-load:"), (name, rows[name])
        assert _quiet_check_all_main([frames_path, scen]) == 2, "坏场景整体必须 rc=2"
        assert _quiet_check_all_main([frames_path, scen, "--allow-missing"]) == 2, \
            "--allow-missing 不许放行 error 行"

    def tooth6():
        # 牙⑥（reviewer A-1）：required 里有任一变量来自退化规则 ⇒ cls=rule，不计入 derived。
        # 变异：check_all 里一律 cls=CLS_DERIVED ⇒ 本断言红。
        rows = _tmp_check_all({
            "npcish.json": {"name": "npcish", "required": ["npc_actor"],
                            "steps": [{"wait_for": 643, "timeout_ms": 10}]},
        }, frames=_fixture())                        # 无 643 ⇒ npc_actor 走 SM_TURN 规则
        assert rows["npcish.json"][1] == 0, rows["npcish.json"]
        assert rows["npcish.json"][3] == CLS_RULE, rows["npcish.json"]
        rows = _tmp_check_all({                      # 有 643 ⇒ npc_actor 走推导
            "npcish.json": {"name": "npcish", "required": ["npc_actor"],
                            "steps": [{"wait_for": 643, "timeout_ms": 10}]},
        }, frames=_fixture(merchant_say=True))
        assert rows["npcish.json"][3] == CLS_DERIVED, rows["npcish.json"]

    case("① 643+自身 ⇒ 值与来源正确（含 --var 渲染）", c1)
    case("② 缺 NPC ⇒ fail-closed", c2)
    case("③ 缺自身坐标 ⇒ fail-closed", c3)
    case("④ 自己的 SM_TURN 不得当 NPC", c4)
    case("⑤ 无 201 ⇒ make_index/item_name 恒 missing", c5)
    case("⑥ 同夹具两次跑同结果（确定性）", c6)
    case("⑦ 混合：可推导的给、不可推导的逐个点名", c7)
    case("⑧ check-all：可推导 rc=0 / 不可推导 rc=2 且报 missing:", c8)
    case("⑨ check-all 分类：params 默认值不算发现绿", c9)
    case("⑩ --allow-missing：只放行 missing，error 与钉数不符仍 rc=2", c10)
    case("⑪ 坏行 ⇒ 干净 rc=2（DiscoveryError）", c11)
    case("⑫ 单场景推导：空 --var 列表 / 空 frames ⇒ rc=2（空判据不算通过）", c12)
    case("牙① self_x 错取 tag ⇒ 值与真值不同", tooth1)
    case("牙② check-all 静默吞 missing（if missing:→if False:）⇒ 行为断言红", tooth2)
    case("牙③ 第三方 actor 无 643 ⇒ merchant_actor fail-closed", tooth3)
    case("牙④ 零场景（缺目录/空目录/--allow-missing）⇒ rc=2", tooth4)
    case("牙⑤ 场景文件坏掉（坏 JSON/数组/required 非字符串数组）⇒ cls=error rc=2", tooth5)
    case("牙⑥ required 含退化规则变量 ⇒ cls=rule（不计 derived）", tooth6)
    if fails:
        print("param_discovery selftest: FAIL")
        for item in fails:
            print("  -", item)
        return 1
    print("param_discovery selftest: OK (13 cases + 6 teeth)")
    return 0


def main(argv=None):
    argv = list(sys.argv[1:] if argv is None else argv)
    if not argv:
        print(__doc__)
        return 2
    try:
        if argv[0] == "selftest":
            return selftest()
        if argv[0] == "check-all":
            return check_all_main(argv[1:])
        if len(argv) < 2:
            sys.stderr.write("usage: param_discovery.py selftest | <frames.ndjson> "
                             "<var1,var2,...> | check-all <frames> [dir] [--allow-missing[=N]]\n")
            return 2
        frames = load_frames(argv[0])
        required = [x.strip() for x in argv[1].split(",") if x.strip()]
        if not required:
            # 空判据不算通过：一个变量都没要求 ⇒ 无从「发现」，退回 usage 口径 rc=2
            sys.stderr.write("param_discovery: no variables requested (empty --var list) — "
                             "空判据不算通过\n")
            return 2
        values, missing = derive(frames, required)
        print(json.dumps({"values": values, "missing": missing,
                          "reasons": dict((v, REASONS.get(v) or UNDERIVABLE.get(v)
                                           or "规则表无此变量（不猜）") for v in missing),
                          "argv": render_vars(values)}, ensure_ascii=False, indent=1))
        return 0 if not missing else 2
    except DiscoveryError as ex:
        sys.stderr.write("param_discovery: %s\n" % ex)
        return 2


if __name__ == "__main__":
    sys.exit(main())
