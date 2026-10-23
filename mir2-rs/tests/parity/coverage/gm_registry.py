#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""gm_registry.py — GM 命令「可达性登记 + 门禁」（卡 openmir2-gm-reachability-registry）。

本版是返工件（reviewer-1 `ad13dc87` 的 B1/B2/I1/I2/I3/M1/M2）。相对 v1 的三处口径修正：

1. **命令名/handler 由三层叠加推导，不拿 conf 键当 CmdName**（B2）
   `GameCmdConf.LoadConfig` 里 `commands.<Field>.CmdName = ReadWriteString("Command", "<Key>", …)`
   的 `<Key>` 是**字段键**（`StorageLock`/`Flag`/`SuperMake`/`SpiritStart`/`Map`/`MapInfo`…），
   不是 CmdName。最终名 = `conf[Command][Key]` ?? 字段构造默认名。同理权限 = `conf[Permission][PermKey]`
   ?? `PerMissionMin` 构造默认值。handler 由字段的 `[RegisterCommand(typeof(T))]` 定位 T 的 `[Command(…)]`
   行（`CommandSystem.cs:41-71` 建 customCommandMap：键 = 类的 `[Command]` 属性名，值 = GameCmd）；
   `RegisterCommandGroups`（`:76-117`）再按属性名命中后把类**改名/改权限**。属性名≠最终名的命令
   （`Map`/`MapInfo` 正是对调的一对）必须这样解才不会指错类。
   **自校验**：本工具推导出的 185 条名字与权限必须与 `gmcmd.rs` 的 `COMMANDS` 表**完全一致**，
   不一致 ⇒ 门禁红（这是「取数口径对没对」的机器判据，不靠人看）。

2. **新增第四态 `script-reachable`**（B1）
   脚本动作 `GMEXECUTE`（`ExecutionCode.cs:464` / `ExecutionProcessingSys.cs:4925-4947`）先把
   `playerActor.Permission = 10`，再 `ProcessUserLineMsg("@<cmd> …")`，走的是同一条权限门
   （`CommandSystem.Execute` → `GameCommand.Handle`，`GameCommand.cs:37`）。所以 `0 < perm_min ≤ 10`
   的命令**只要被部署脚本真实调用、且所在 label 可达**，在 Release 现网就可达。
   本工具据此扫描部署 Envir：先按 oracle 的加载规则算出**已加载脚本集合**（`LocalDB.QFunctionNpc/
   QMangeNpc/RobotNpc` + `MerChant.txt` + `Npcs.txt` + `#CALL` 传递闭包），再在已加载文件内做
   **label 可达性**（引擎 `GotoLable` 根 + NPC 点击 `@main` + 超链接/`goto`/`DelayCall`/`#CALL` 边），
   最后只把「label 可达」的 GMEXECUTE 记为证据。

3. **门禁校验「与源码一致」而非「自洽」**（I2）
   gate 会重跑 build 并与 JSON **逐字段比对**，另加独立断言：handler 行所属类的注册名必须等于该行
   `cmd`；`register_command=true ⇒ handler 非空`；`perm_min=null ⇒ verdict=undetermined`；
   `script-reachable ⇒ 证据非空且逐条可复算`；`unreachable ⇒ 部署脚本里确无该命令的可达调用`。

判据（写死在工具里）
--------------------
  perm_min == 0                                   ⇒ reachable（玩家自然可达，必须开实现卡）
  0 < perm_min <= 10 ∧ 存在可达的 GMEXECUTE 调用   ⇒ script-reachable（脚本抬权可达，必须开实现卡）
  其它 perm_min > 0 ∧ 无脚本证据                   ⇒ unreachable（权限门挡住）
  无可解析命令体（既无字段/类注册）                ⇒ structural-unreachable（必须能证明，否则红）
  任一必需数据源缺失/解析歧义                      ⇒ undetermined（门禁红，rc=2）

部署面（I1）
------------
  oracle = `E:/MirServer/M2GameSvr`，Release 构建：
    * `!deployed-from.json` 的 `build_dir` 以 `\\bin\\Release` 结尾；
    * `GameSrv.dll` / `CommandSystem.dll` **不含** UTF-16LE 调试串「当前运行调试模式」
      （`GameCommand.cs:33-36` 的 `#if DEBUG` 分支才会写这个串）。
  反例：`E:/MirServer/Mir200/GameSvr.dll` **含**该串 ⇒ Debug 构建。**若把 Debug 构建当对拍面，
  `GameCommand.cs:33-36` 会把每个调用者的 Permission 直接置 10，所有 perm_min ≤ 10 的命令对
  普通玩家全部可达 ⇒ 本表全部 unreachable 结论立即失效（全部改判 reachable）。**

子命令：build / gate / summary / selftest
退出码：0 绿 / 1 红 / 2 无法判定
"""
import argparse
import copy
import hashlib
import json
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.abspath(os.path.join(HERE, "..", "..", "..", ".."))
RUST_TABLE = os.path.join(REPO, "mir2-rs", "bins", "game-svr", "src", "gmcmd.rs")
GAMECMDS = os.path.join(REPO, "src", "Modules", "GameCommand", "GameCommands.cs")
GAMECMDS_CONF = os.path.join(REPO, "src", "Modules", "GameCommand", "Conf", "GameCmdConf.cs")
SCRIPT_FLAGS = os.path.join(REPO, "src", "Modules", "SystemModule", "Const", "ScriptFlagConst.cs")
CS_DIR = os.path.join(REPO, "src", "Modules", "GameCommand", "Commands")
GAME_COMMAND_DIR = os.path.join(REPO, "src", "Modules", "GameCommand")
SRC_DIR = os.path.join(REPO, "src")
REGISTRY = os.path.join(HERE, "gm_registry.json")
DEFAULT_CONF = os.path.join("E:" + os.sep, "MirServer", "M2GameSvr", "Command.conf")
DEFAULT_ENVIR = os.path.join("E:" + os.sep, "MirServer", "M2GameSvr", "Envir")
DEFAULT_DEPLOY = os.path.join("E:" + os.sep, "MirServer", "M2GameSvr")

# 第三态（文本已逐字节 / 宿主动作未接）的具名 Imp，与
# docs/rewrite-remaining-inventory-20261022.md §2.1 的 awk 口径一致（T-161 的 14 + T-163 的 7 = 21）。
THIRD_STATE_IMPS = {
    "ReloadAllGuild", "ReloadGuild", "ReloadLineNotice", "ReloadManage", "ReloadRobotManage",
    "ReLoadAdmin", "LoadAdmin", "ReloadItemDb", "ReloadMagicDb", "ReloadMonItems", "ReloadRobot",
    "ReloadAbuse", "ReloadNpc", "NpcScript", "TestFire", "FireBurn", "TestStatus", "BackStep",
    "UserCmd", "TestGetBagItems", "TestSpeedMode",
}

# 部署脚本里 GMEXECUTE 抬到的权限值（ExecutionProcessingSys.cs:4938 `playerActor.Permission = 10`）。
GMEXECUTE_PERMISSION = 10

# script-reachable 且 Rust 未实现 ⇒ 每条开一张签名子卡（卡 openmir2-gm-reachability-registry 的返工交付）。
CHILD_CARDS = {
    "禁言": "openmir2-gm-impl-shutup",
    "取消禁言": "openmir2-gm-impl-shutup-release",
    "VV踢人": "openmir2-gm-impl-kick",
    "ShowEffect": "openmir2-gm-impl-showeffect",
    "MobFireBurn": "openmir2-gm-impl-mobfireburn",
    "改属性": "openmir2-gm-impl-smake",
    "清理垃圾": "openmir2-gm-impl-clearbag",
    "开始提问": "openmir2-gm-impl-startquest",
}

# 对 reviewer-1 `ad13dc87` 的逐条复核结论（静态证据，工具不依赖它们做判定，只作为交付说明随表落盘）。
REVIEW_CORRECTIONS = [
    {
        "id": "B1-cite-ReAlive",
        "claim": "reviewer B1：复活（ReAlive）经 QFunction-0.txt:63 的 GMEXECUTE 可达。",
        "finding": "不成立：该行所在 label 是 [@PlayDie]，在本 oracle 里是死代码。",
        "evidence": [
            "src/M2Server/Player/PlayObject.Message.cs:1638 是唯一的死亡事件派发，标签是 \"@OnDeath\"；"
            "QFunction-0.txt 没有 [@OnDeath]，也没有任何文件引用 @PlayDie（全 Envir 递归扫描，0 处 hyperlink/goto/DelayCall/QUERYVALUE/#CALL）。",
            "src/Modules/SystemModule/Const/ScriptFlagConst.cs:180 的 SPLAYDIE=\"@PLAYDIE\" 只有定义、全仓库无引用 ⇒ 没有任何代码能拼出 @PlayDie。",
            "同类：QFunction-0.txt:2 的 [@PlayLevelUp] 也是死代码（PlayObject.cs:5250 的 GotoLable 被注释掉，且写的是 \"@LevelUp\"）。",
        ],
        "verdict": "ReAlive 在本表维持 unreachable；reviewer 的 cite 需作废（命令本身仍可能经其它未发现通道，已按 incoming=0 穷举证明）。",
    },
    {
        "id": "B1-cite-ReleaseShutup",
        "claim": "reviewer B1：取消禁言经 QFunction-0.txt:6 的 [@PlayLevelUp] 可达。",
        "finding": "该 cite 同样是死 label；但取消禁言在 功能NPC/比武-GA2.txt 的 [@AA]（NPC 超链接，@main 根）有 3 处调用 ⇒ 结论仍是 script-reachable。",
        "evidence": [
            "Market_Def/功能NPC/比武-GA2.txt:43/68/106（[@AA]，由 [@main] 的 <给装备/@AA> 进入，@main 是 NormNpc.cs:73 的点击根）",
        ],
        "verdict": "verdict 不变（script-reachable），但依据换成比武-GA2。",
    },
    {
        "id": "B2-structural-zero",
        "claim": "reviewer B2：结构性不可达 28 条全部不成立，结构性桶应为 0。",
        "finding": "复核采纳：本工具按 GameCmdConf 字段键 + [RegisterCommand] 推导后，88 条全部解析出 [Command] 处理类，"
                   "structural-unreachable = 0；Map→MapInfoCommand.cs:8、MapInfo→ShowMapInfoCommand.cs:6 的对调已修正。",
        "evidence": [
            "crosscheck.ok=true（185 条名字与 perm 与 gmcmd.rs COMMANDS 完全一致）",
            "selftest 牙 M 专测 Map/MapInfo 未对调",
        ],
        "verdict": "采纳。",
    },
    {
        "id": "script-unknown-cmds",
        "claim": "部署脚本里出现 +jww / -jww / 去 / 改变忠诚。",
        "finding": "这 4 个在 oracle 里都是「未知命令」（CommandSystem.Execute 回 未知命令），不需要 Rust 实现任何东西。",
        "evidence": [
            "E:/MirServer/M2GameSvr/Command.conf 的命令键拼写是 DenyCharNameLogon/DelDenyCharNameLogon/ShowCharNameLogon"
            "（Char），而 src/Modules/GameCommand/Conf/GameCmdConf.cs:327/333/339 读的是 DenyChrNameLogon/DelDenyChrNameLogon/"
            "ShowDenyChrNameLogon（Chr）⇒ 6 条 conf 改名是死键，最终命令名退回构造默认值 DenyChrNameLogon 等，"
            "+jww / -jww 不可能是命令名。",
            "去 / 改变忠诚 在 185 条命令表里不存在（gmcmd.rs 无此 name，Command.conf 也无此值）。",
        ],
        "verdict": "登记为 unknown_script_commands（oracle 侧即 no-op），不开实现卡。",
    },
]


CMD_DEF_RE = re.compile(
    r"CommandDef\s*\{\s*name:\s*\"(?P<name>[^\"]+)\",\s*perm_min:\s*(?P<perm>\d+),"
    r"\s*params:\s*(?P<params>\d+),\s*imp:\s*Imp::(?P<imp>\w+)\s*,\s*\}", re.S)
GC_REG_RE = re.compile(
    r"\[RegisterCommand\(\s*\(?\s*typeof\(\s*(?P<cls>\w+)\s*\)\s*\)?\s*\)\]\s*"
    r"public\s+readonly\s+GameCmd\s+(?P<field>\w+)\s*;", re.S)
GC_CTOR_RE = re.compile(r"(?P<field>\w+)\s*=\s*new\s+GameCmd\s*\{(?P<body>[^{}]*)\}", re.S)
GC_CONF_BIND_RE = re.compile(
    r"commands\.(?P<field>\w+)\.(?P<prop>CmdName|PerMissionMin)\s*=\s*"
    r"ReadWrite(?:String|Byte)\(\s*\"(?P<sec>\w+)\"\s*,\s*\"(?P<key>[^\"]+)\"")
CS_ATTR_RE = re.compile(r"\[Command\((?P<args>(?:[^()\"]|\"(?:\\.|[^\"\\])*\")*)\)\]")
CS_CLASS_RE = re.compile(
    r"\[Command\((?P<args>(?:[^()\"]|\"(?:\\.|[^\"\\])*\")*)\)\]\s*"
    r"(?:(?:public|internal|sealed|abstract|partial|static)\s+)*class\s+(?P<cls>\w+)", re.S)
CS_EXEC_RE = re.compile(r"^\s*public\s+void\s+Execute\s*\(")
LABEL_RE = re.compile(r"^\[([^\]]+)\]\s*$")
GMEXECUTE_RE = re.compile(r"^\s*gmexecute\b(.*)$", re.I)
CALL_RE = re.compile(r"#CALL\s*\[([^\]]+)\]\s*@?(\S*)", re.I)
HYPERLINK_RE = re.compile(r"/(@[^\s<>/\\,()]+)")


def sha256(path):
    h = hashlib.sha256()
    with open(path, "rb") as fh:
        for chunk in iter(lambda: fh.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def read_text(path, encoding="utf-8"):
    with open(path, "r", encoding=encoding, errors="replace") as fh:
        return fh.read()


def read_gbk(path):
    with open(path, "rb") as fh:
        return fh.read().decode("gbk", errors="replace")


def relpath(path):
    return os.path.relpath(path, REPO).replace(os.sep, "/")


def fail(msg, code=2):
    sys.stderr.write(msg.rstrip() + "\n")
    raise SystemExit(code)


# --------------------------------------------------------------------------
# 1. Rust 侧命令表
# --------------------------------------------------------------------------
def parse_rust(path=RUST_TABLE):
    if not os.path.exists(path):
        fail("无法判定：缺少 %s" % path)
    text = read_text(path)
    start = text.find("pub const COMMANDS")
    if start < 0:
        fail("无法判定：%s 里找不到 COMMANDS 表" % path)
    rows = []
    for m in CMD_DEF_RE.finditer(text, start):
        rows.append({"name": m.group("name"), "perm_min": int(m.group("perm")),
                     "params": int(m.group("params")), "imp": m.group("imp")})
    if not rows:
        fail("无法判定：COMMANDS 表解析为空（格式变了？）")
    return rows


# --------------------------------------------------------------------------
# 2. oracle 侧三层叠加（GameCommands 字段 → GameCmdConf 键 → Command.conf）
# --------------------------------------------------------------------------
def parse_gamecommands(path=GAMECMDS):
    if not os.path.exists(path):
        fail("无法判定：缺少 %s" % path)
    text = read_text(path, "utf-8-sig")
    reg = {}
    reg_line = {}
    for m in GC_REG_RE.finditer(text):
        reg[m.group("field")] = m.group("cls")
        reg_line[m.group("field")] = text.count("\n", 0, m.start()) + 1
    ctor = {}
    for m in GC_CTOR_RE.finditer(text):
        body = m.group("body")
        nm = re.search(r'CmdName\s*=\s*"([^"]*)"', body)
        pmin = re.search(r"PerMissionMin\s*=\s*(\d+)", body)
        if not nm or not pmin:
            continue
        ctor[m.group("field")] = {"cmd_name": nm.group(1), "perm_min": int(pmin.group(1)),
                                  "line": text.count("\n", 0, m.start()) + 1}
    if not reg or not ctor:
        fail("无法判定：GameCommands.cs 解析为空（格式变了？）")
    return {"register": reg, "register_line": reg_line, "ctor": ctor,
            "path": relpath(path), "sha256": sha256(path)}


def parse_gamecmdconf(path=GAMECMDS_CONF):
    if not os.path.exists(path):
        fail("无法判定：缺少 %s" % path)
    text = read_text(path, "utf-8-sig")
    binds = {}
    for m in GC_CONF_BIND_RE.finditer(text):
        binds.setdefault(m.group("field"), []).append(
            (m.group("prop"), m.group("sec"), m.group("key")))
    if not binds:
        fail("无法判定：GameCmdConf.cs 解析为空（格式变了？）")
    return {"bindings": binds, "path": relpath(path), "sha256": sha256(path)}


def parse_conf(path):
    """-> ({section_lower: {key_lower: raw}}, state)。"""
    if not path or not os.path.exists(path):
        return None, "absent"
    text = read_gbk(path)
    conf, section = {}, None
    for line in text.splitlines():
        s = line.strip()
        if not s or s.startswith(";"):
            continue
        if s.startswith("[") and s.endswith("]"):
            section = s[1:-1].lower()
            conf.setdefault(section, {})
            continue
        if "=" not in s or section is None:
            continue
        k, _, v = s.partition("=")
        conf[section][k.strip().lower()] = v.strip()
    return conf, "present"


def layered_values(field, gamecmds, gamecmdconf, conf):
    """逐行模拟 GameCmdConf.LoadConfig：按出现顺序叠加，缺键回落上一层的值。"""
    base = gamecmds["ctor"].get(field)
    if base is None:
        return None
    name, name_src = base["cmd_name"], "GameCommands.cs 构造默认"
    perm, perm_src = base["perm_min"], "GameCommands.cs 构造默认"
    for prop, sec, key in gamecmdconf["bindings"].get(field, []):
        val = (conf or {}).get(sec.lower(), {}).get(key.lower())
        if val is None:
            continue
        if prop == "CmdName":
            name, name_src = val, "conf[%s].%s" % (sec, key)
        else:
            try:
                perm, perm_src = int(val), "conf[%s].%s" % (sec, key)
            except ValueError:
                pass
    return {"cmd_name": name, "name_source": name_src,
            "perm_min": perm, "perm_source": perm_src,
            "field": field, "field_line": gamecmds["register_line"].get(field),
            "register_class": gamecmds["register"].get(field)}


def split_attr_args(raw):
    out, buf, q, esc = [], "", False, False
    for ch in raw:
        if q:
            buf += ch
            if esc:
                esc = False
            elif ch == "\\":
                esc = True
            elif ch == '"':
                q = False
        elif ch == '"':
            q = True
            buf += ch
        elif ch == ",":
            out.append(buf.strip())
            buf = ""
        else:
            buf += ch
    if buf.strip():
        out.append(buf.strip())
    vals = []
    for tok in out:
        if len(tok) >= 2 and tok[0] == '"' and tok[-1] == '"':
            vals.append(tok[1:-1])
        elif re.fullmatch(r"\d+", tok):
            vals.append(int(tok))
        else:
            vals.append(None)
    return vals


def parse_command_classes(cs_dir=CS_DIR):
    """-> {class: {attr_name, perm, file, line, exec_line}}（一个 .cs 可定义多个类）。"""
    if not os.path.isdir(cs_dir):
        fail("无法判定：缺少 %s" % cs_dir)
    classes = {}
    for fn in sorted(os.listdir(cs_dir)):
        if not fn.endswith(".cs"):
            continue
        path = os.path.join(cs_dir, fn)
        lines = read_text(path, "utf-8-sig").splitlines()
        text = "\n".join(lines)
        for m in CS_CLASS_RE.finditer(text):
            vals = split_attr_args(m.group("args"))
            name = vals[0] if vals and isinstance(vals[0], str) else None
            cls = m.group("cls")
            if not name:
                continue
            if cls in classes:
                fail("无法判定：%s 里类 %s 重复定义 [Command]（解析歧义）" % (fn, cls))
            attr_line = text.count("\n", 0, m.start()) + 1
            perm = next((v for v in vals[1:] if isinstance(v, int)), 0)
            exec_line = None
            for j in range(attr_line + 1, len(lines) + 1):
                if CS_EXEC_RE.match(lines[j - 1]):
                    exec_line = j
                    break
            classes[cls] = {"attr_name": name, "perm_attr": perm, "file": relpath(path),
                            "line": attr_line, "exec_line": exec_line}
    if not classes:
        fail("无法判定：Commands/*.cs 解析为空（格式变了？）")
    return classes


def resolve_command_maps(gamecmds, gamecmdconf, conf, classes):
    """复刻 CommandSystem.cs 的 RegisterCustomCommand + RegisterCommandGroups。

    返回 (maps, custom, diagnostics)：maps = {最终 CmdName: 行信息}。
    """
    warnings = []
    custom = {}  # key = 处理类的 [Command] 属性名
    for field, cls in gamecmds["register"].items():
        if field not in gamecmds["ctor"]:
            continue
        lay = layered_values(field, gamecmds, gamecmdconf, conf)
        if lay is None or not lay["cmd_name"]:
            continue
        info = classes.get(cls)
        if info is None:
            warnings.append("字段 %s 的 [RegisterCommand] 指向不存在的类 %s" % (field, cls))
            continue
        key = info["attr_name"]
        if key in custom:
            warnings.append("属性名 %s 重复注册（CommandSystem 记 warn 并跳过后一个）" % key)
            continue
        custom[key] = dict(lay, **{"handler_class": cls, "attr_name": key})

    maps, collisions = {}, []
    for cls, info in classes.items():
        key = info["attr_name"]
        if key in maps:
            collisions.append((key, cls, maps[key]["handler_class"]))
            continue
        if key in custom:
            row = custom[key]
            final_name = row["cmd_name"]
            perm = row["perm_min"]
            perm_src = row["perm_source"]
            name_src = row["name_source"]
            field = row["field"]
            via_custom = True
        else:
            final_name = key
            perm = info["perm_attr"]
            perm_src = "cs[Command] 属性"
            name_src = "cs[Command] 属性"
            field = None
            via_custom = False
        maps[final_name] = {
            "cmd_name": final_name, "handler_class": cls, "attr_name": key,
            "handler": "%s:%d" % (info["file"], info["line"]),
            "handler_line": info["line"], "handler_file": info["file"],
            "handler_exec_line": info["exec_line"],
            "perm_min": perm, "perm_source": perm_src, "perm_attr": info["perm_attr"],
            "name_source": name_src, "field": field, "via_custom": via_custom,
            "field_line": (custom[key]["field_line"] if via_custom else None),
            "register_command": bool(field),
        }
    if collisions:
        warnings.append("命令名冲突 %d 处（CommandSystem 会抛重复键）: %r" % (len(collisions), collisions[:3]))
    return maps, custom, warnings


# --------------------------------------------------------------------------
# 3. 部署脚本：加载集合 + label 可达性 + GMEXECUTE 实参
# --------------------------------------------------------------------------
def resolve_case_insensitive(base, rel):
    cur = base
    for part in rel.replace("\\", "/").split("/"):
        if not part:
            continue
        try:
            names = os.listdir(cur)
        except OSError:
            return None
        hit = next((n for n in names if n.lower() == part.lower()), None)
        if hit is None:
            return None
        cur = os.path.join(cur, hit)
    return cur if os.path.exists(cur) else None


def engine_label_roots():
    """从 C# 源码里取引擎侧 GotoLable 的标签根（字面量 + ScriptFlagConst 常量）。"""
    consts = {}
    if os.path.exists(SCRIPT_FLAGS):
        for m in re.finditer(r"public\s+const\s+string\s+(\w+)\s*=\s*\"([^\"]*)\"",
                             read_text(SCRIPT_FLAGS, "utf-8-sig")):
            consts[m.group(1)] = m.group(2)
    exact, prefix, refs = set(), set(), []
    for dp, _dn, fns in os.walk(SRC_DIR):
        for fn in fns:
            if not fn.endswith(".cs"):
                continue
            path = os.path.join(dp, fn)
            for i, line in enumerate(read_text(path, "utf-8-sig").splitlines(), 1):
                s = line.strip()
                if s.startswith("//") or "GotoLable(" not in line:
                    continue
                if "void GotoLable" in line or "GotoLableCheckStringList" in line:
                    continue
                for m in re.finditer(r'\"(@[^\"]+)\"(\s*\+)?', line):
                    lab = m.group(1)
                    if m.group(2):
                        prefix.add(lab.lower())
                    else:
                        exact.add(lab.lower())
                    refs.append("%s:%d" % (relpath(path), i))
                for m in re.finditer(r"ScriptFlagConst\.(\w+)", line):
                    val = consts.get(m.group(1))
                    if val:
                        exact.add(val.lower())
                        refs.append("%s:%d" % (relpath(path), i))
    # `@main` 是 NPC 点击入口（NormNpc.cs:73），但只对**可点击的 Merchant/NormNpc 脚本**成立；
    # QFunction/QManage/RobotManage 是 IsHide 的功能 NPC，没有点击入口，故不在这里加根，
    # 由 label_graph 按文件是否 NPC 脚本决定（loaded[rel]["npc"]）。
    return {"exact": exact, "prefix": prefix, "sources": sorted(set(refs))}


def load_script_set(envir, roots):
    """按 oracle 的加载规则求已加载脚本集合；返回 (loaded{rel: origin}, queue_meta)。"""
    mer_chant = os.path.join(envir, "MerChant.txt")
    npcs = os.path.join(envir, "Npcs.txt")
    entries = []
    for rel, kind in (("Market_Def/QFunction-0.txt", "LocalDB.QFunctionNpc.cs:196"),
                      ("MapQuest_def/QManage.txt", "LocalDB.QMangeNpc.cs:230"),
                      ("Robot_def/RobotManage.txt", "LocalDB.RobotNpc.cs:289")):
        entries.append({"rel": rel, "origin": kind, "npc": False})
    if os.path.exists(mer_chant):
        # LocalDB.LoadMerchant.cs:412 用 HUtil32.GetValidStr3 按空白切列：col0=ScriptName, col1=MapName。
        for i, line in enumerate(read_gbk(mer_chant).splitlines(), 1):
            s = line.strip()
            if not s or s.startswith(";"):
                continue
            cols = s.split()
            if len(cols) >= 2:
                entries.append({"rel": "Market_Def/%s-%s.txt" % (cols[0], cols[1]),
                                "origin": "MerChant.txt:%d" % i, "npc": True})
    if os.path.exists(npcs):
        for i, line in enumerate(read_gbk(npcs).splitlines(), 1):
            s = line.strip()
            if not s or s.startswith(";"):
                continue
            cols = s.split()
            if len(cols) >= 3 and cols[1] == "1":  # code 1 = IsQuest → Npc_def/<名>-<地图>.txt
                entries.append({"rel": "Npc_def/%s-%s.txt" % (cols[0], cols[2]),
                                "origin": "Npcs.txt:%d" % i, "npc": True})

    loaded, queue, missing = {}, list(entries), []
    while queue:
        item = queue.pop(0)
        rel = item["rel"].replace("\\", "/")
        if rel in loaded:
            continue
        # 总是按目录真实大小写解析（Windows 下 os.path.exists 大小写不敏感，会把 rel 停在请求拼写上）
        path = resolve_case_insensitive(envir, rel)
        if path is None:
            missing.append(rel)
            continue
        rel = os.path.relpath(path, envir).replace(os.sep, "/")
        if rel in loaded:
            continue
        loaded[rel] = item
        for i, line in enumerate(read_gbk(path).splitlines(), 1):
            m = CALL_RE.search(line)
            if not m:
                continue
            target = "QuestDiary/" + m.group(1).strip().replace("\\", "/").lstrip("/")
            queue.append({"rel": target, "origin": "#CALL %s:%d" % (rel, i),
                          "npc": False, "call_label": m.group(2)})
    return loaded, missing


def label_sections(text):
    """-> [(label_lower, [body lines])]。label 保留 @ / ~ 前缀；首个 label 之前的内容忽略。"""
    out, cur, buf = [], None, []
    for line in text.splitlines():
        m = LABEL_RE.match(line.strip())
        if m:
            if cur is not None:
                out.append((cur, buf))
            cur, buf = m.group(1).strip().lower(), []
        elif cur is not None:
            buf.append(line)
    if cur is not None:
        out.append((cur, buf))
    return out


def label_graph(envir, loaded, roots):
    """-> (reachable {(file,label)}, parent {(file,label): (file,label)|root}, sections)."""
    sections, edges = {}, {}
    for rel in loaded:
        sections[rel] = label_sections(read_gbk(os.path.join(envir, rel)))
    for rel in loaded:
        secs = sections[rel]
        known = {lab for lab, _ in secs}
        es = {}
        for lab, body in secs:
            outs = []
            sect = None
            for line in body:
                s = line.strip()
                up = s.upper()
                if up.startswith("#ACT") or up.startswith("#ELSEACT"):
                    sect = "act"
                elif up.startswith("#IF"):
                    sect = "if"
                elif up.startswith("#SAY"):
                    sect = "say"
                for m in HYPERLINK_RE.finditer(line):
                    if m.group(1).lower() in known:
                        outs.append((rel, m.group(1).lower()))
                if sect == "act":
                    # 动作行里的 label 实参：goto / delaygoto / delaycall / queryvalue / inputstring …
                    for m in re.finditer(r"[@~][^\s<>/\\,()'\"]+", s):
                        tok = m.group(0).lower()
                        if tok in known and (rel, tok) not in outs:
                            outs.append((rel, tok))
                m = CALL_RE.search(line)
                if m:
                    target = "QuestDiary/" + m.group(1).strip().replace("\\", "/").lstrip("/")
                    real = target if target in sections else None
                    if real is None:
                        # 大小写不敏感解析（Windows 语义）
                        for cand in loaded:
                            if cand.lower() == target.lower():
                                real = cand
                                break
                    call_lab = m.group(2).strip().lower()
                    if call_lab and not call_lab.startswith(("@", "~")):
                        call_lab = "@" + call_lab
                    if real and call_lab:
                        outs.append((real, call_lab))
            es[lab] = outs
        edges[rel] = es

    reachable, parent = set(), {}
    incoming = {}
    for rel, es in edges.items():
        for src_lab, outs in es.items():
            for tgt in outs:
                incoming.setdefault(tgt, set()).add((rel, src_lab))
    # 收集所有 (file,label) 真实节点（含跨文件 #CALL 目标）
    all_labels = set()
    for rel, secs in sections.items():
        for lab, _ in secs:
            all_labels.add((rel, lab))

    def is_engine_root(rel, lab):
        if lab in roots["exact"]:
            return True
        if lab == "@main" and loaded.get(rel, {}).get("npc"):
            return True
        return any(lab.startswith(p) for p in roots["prefix"])

    queue = []
    for rel, secs in sections.items():
        for lab, _ in secs:
            if is_engine_root(rel, lab):
                queue.append(((rel, lab), "engine GotoLable root"))

    while queue:
        node, why = queue.pop(0)
        if node not in all_labels or node in reachable:
            continue
        reachable.add(node)
        parent[node] = why
        rel, lab = node
        for tgt in edges.get(rel, {}).get(lab, []):
            if tgt in all_labels and tgt not in reachable:
                queue.append((tgt, "%s:%s" % (rel, lab)))
    return reachable, parent, sections, incoming


def scan_scripts(envir, roots):
    """-> dict：加载集合、label 可达性、GMEXECUTE 出现点（含是否可达）。"""
    if not os.path.isdir(envir):
        fail("无法判定：部署 Envir 不存在 %s" % envir)
    loaded, missing = load_script_set(envir, roots)
    reachable, parent, sections, incoming = label_graph(envir, loaded, roots)

    occurrences, unloaded_hits = [], []
    for dp, _dn, fns in os.walk(envir):
        for fn in fns:
            if not fn.lower().endswith(".txt"):
                continue
            path = os.path.join(dp, fn)
            rel = os.path.relpath(path, envir).replace(os.sep, "/")
            text = read_gbk(path)
            if "gmexecute" not in text.lower():
                continue
            if rel not in loaded:
                for i, line in enumerate(text.splitlines(), 1):
                    if GMEXECUTE_RE.match(line):
                        unloaded_hits.append({"file": rel, "line": i, "raw": line.strip()})
                continue
            label, section = None, None
            for i, line in enumerate(text.splitlines(), 1):
                s = line.strip()
                m = LABEL_RE.match(s)
                if m:
                    label, section = m.group(1).lower(), None
                    continue
                if s.upper().startswith("#ACT"):
                    section = "act"
                elif s.upper().startswith("#IF"):
                    section = "if"
                m = GMEXECUTE_RE.match(s)
                if not m:
                    continue
                args = m.group(1).strip().split()
                occurrences.append({
                    "file": rel, "line": i, "raw": s,
                    "label": label, "section": section,
                    "script_arg": args[0] if args else "",
                    "label_reachable": bool(label and (rel, label) in reachable),
                    "label_path": _path_str(parent, (rel, label)) if label and (rel, label) in reachable else None,
                })
    dead = {}
    for occ in occurrences:
        if occ["label_reachable"] or not occ["label"]:
            continue
        key = (occ["file"], occ["label"])
        d = dead.setdefault(key, {"file": occ["file"], "label": occ["label"],
                                  "incoming_from": sorted("%s:%s" % s for s in incoming.get(key, ())),
                                  "occurrences": []})
        d["occurrences"].append({"line": occ["line"], "script_arg": occ["script_arg"],
                                 "raw": occ["raw"]})
    for d in dead.values():
        d["reason"] = ("该 label 既不是引擎 GotoLable 根（%d 个字面量/常量根已枚举），"
                       "在 %d 个已加载脚本里也没有任何 hyperlink/goto/DelayCall/QUERYVALUE/#CALL 指向它"
                       "（incoming=%d）⇒ 该 label 的 #ACT 永远不执行"
                       % (len(roots["exact"]) + len(roots["prefix"]), len(loaded),
                          len(d["incoming_from"])))
    return {"envir": envir, "loaded": sorted(loaded), "loaded_count": len(loaded),
            "loaded_origin": {k: v.get("origin") for k, v in loaded.items()},
            "missing_roots": missing, "reachable_labels": sorted("%s:%s" % n for n in reachable),
            "occurrences": occurrences, "unloaded_hits": unloaded_hits,
            "dead_labels": sorted(dead.values(), key=lambda x: (x["file"], x["label"]))}


def _path_str(parent, node, limit=8):
    chain, cur = [], node
    while cur in parent and len(chain) < limit:
        why = parent[cur]
        chain.append("%s:%s" % cur)
        if isinstance(why, str) and why.endswith("root"):
            chain.append(why)
            break
        if isinstance(why, str) and ":" in why and not why.endswith("root"):
            f, _, lab = why.partition(":")
            cur = (f, "@" + lab.lstrip("@"))
            continue
        break
    return " <- ".join(chain)


# --------------------------------------------------------------------------
# 4. build
# --------------------------------------------------------------------------
def detect_deployment(deploy_dir):
    info = {"path": deploy_dir, "present": os.path.isdir(deploy_dir), "build": None,
            "evidence": [], "debug_marker": "当前运行调试模式"}
    if not info["present"]:
        info["evidence"].append("部署目录不存在")
        return info
    meta = os.path.join(deploy_dir, "!deployed-from.json")
    if os.path.exists(meta):
        try:
            doc = json.loads(read_text(meta, "utf-8-sig"))
            bd = str(doc.get("build_dir", ""))
            info["deployed_from"] = {"build_dir": bd, "git_head": doc.get("git_head"),
                                     "deployed_at": doc.get("deployed_at")}
            if re.search(r"bin[\\/]Release$", bd, re.I):
                info["build"] = "Release"
                info["evidence"].append("!deployed-from.json build_dir=%s" % bd)
            elif re.search(r"bin[\\/]Debug$", bd, re.I):
                info["build"] = "Debug"
                info["evidence"].append("!deployed-from.json build_dir=%s" % bd)
        except ValueError:
            info["evidence"].append("!deployed-from.json 无法解析")
    needle = info["debug_marker"].encode("utf-16-le")
    marker = []
    for name in ("GameSrv.dll", "CommandSystem.dll"):
        p = os.path.join(deploy_dir, name)
        if os.path.exists(p):
            hit = needle in open(p, "rb").read()
            marker.append((name, hit))
            if hit:
                info["evidence"].append("%s 含 UTF-16LE 调试串『%s』（#if DEBUG 分支）"
                                        % (name, info["debug_marker"]))
    info["debug_marker_hits"] = {n: h for n, h in marker}
    if marker and not any(h for _, h in marker):
        info["evidence"].append("GameSrv.dll/CommandSystem.dll 均不含 Debug 调试串")
        if info["build"] is None:
            info["build"] = "Release"
    elif any(h for _, h in marker):
        info["build"] = "Debug"
    return info


def build(conf_path, envir_path, deploy_dir):
    rust = parse_rust()
    gamecmds = parse_gamecommands()
    gamecmdconf = parse_gamecmdconf()
    conf, conf_state = parse_conf(conf_path)
    classes = parse_command_classes()
    maps, custom, warnings = resolve_command_maps(gamecmds, gamecmdconf, conf, classes)
    roots = engine_label_roots()
    scripts = scan_scripts(envir_path, roots)
    deployment = detect_deployment(deploy_dir)

    # 自校验：推导结果必须与 oracle 派生的 Rust 表完全一致（名字集合 + 权限）。
    rust_names = [r["name"] for r in rust]
    model_missing = [n for n in rust_names if n not in maps]
    model_extra = [n for n in maps if n not in rust_names]
    perm_drift = [{"cmd": r["name"], "rust": r["perm_min"], "model": maps[r["name"]]["perm_min"]}
                  for r in rust if r["name"] in maps and maps[r["name"]]["perm_min"] != r["perm_min"]]
    crosscheck = {"ok": not (model_missing or model_extra or perm_drift),
                  "missing_in_model": model_missing[:20], "extra_in_model": model_extra[:20],
                  "perm_drift": perm_drift[:20], "rust_total": len(rust), "model_total": len(maps)}

    # GMEXECUTE 实参 → 命令解析（CommandMaps 是 OrdinalIgnoreCase）
    lower_map = {k.lower(): k for k in maps}
    for occ in scripts["occurrences"]:
        arg = occ["script_arg"].lower()
        occ["resolved_cmd"] = lower_map.get(arg)
        occ["resolved_rust_imp"] = None
        if occ["resolved_cmd"]:
            for r in rust:
                if r["name"] == occ["resolved_cmd"]:
                    occ["resolved_rust_imp"] = r["imp"]
                    break
    unknown_cmds = sorted({occ["script_arg"] for occ in scripts["occurrences"]
                           if occ["label_reachable"] and occ["script_arg"] and not occ["resolved_cmd"]})

    evidence_by_cmd = {}
    for occ in scripts["occurrences"]:
        if not occ["label_reachable"] or not occ["resolved_cmd"]:
            continue
        evidence_by_cmd.setdefault(occ["resolved_cmd"], []).append({
            "file": occ["file"], "line": occ["line"], "label": occ["label"],
            "section": occ["section"], "script_arg": occ["script_arg"],
            "raw": occ["raw"], "label_path": occ["label_path"],
            "load_origin": scripts["loaded_origin"].get(occ["file"]),
        })

    rows = []
    for r in rust:
        if r["imp"] != "Unimplemented":
            continue
        m = maps.get(r["name"])
        ev = evidence_by_cmd.get(r["name"], [])
        if m is None:
            handler = None
            perm = None
            perm_src = None
            row_extra = {}
        else:
            handler = m["handler"]
            perm = m["perm_min"]
            perm_src = m["perm_source"]
            row_extra = {"handler_class": m["handler_class"], "attr_name": m["attr_name"],
                         "handler_exec_line": m["handler_exec_line"],
                         "name_source": m["name_source"],
                         "register_command": m["register_command"],
                         "perm_attr": m["perm_attr"], "field": m["field"],
                         "field_line": m["field_line"]}
        if perm is None:
            verdict = "undetermined"
            why = "三层叠加后仍无命令体（字段/类注册均缺失）"
        elif handler is None:
            verdict = "structural-unreachable"
            why = "有 CmdName 但无 [Command] 处理类 ⇒ 现网无命令体"
        elif perm == 0:
            verdict = "reachable"
            why = "perm_min==0 ⇒ 玩家自然可达，必须开实现卡"
        elif ev and perm <= GMEXECUTE_PERMISSION:
            verdict = "script-reachable"
            why = ("perm_min=%d>0，但部署脚本经 GMEXECUTE 抬权到 %d 调用（%d 处可达调用）⇒ 现网可达，必须开实现卡"
                   % (perm, GMEXECUTE_PERMISSION, len(ev)))
        elif ev and perm > GMEXECUTE_PERMISSION:
            verdict = "unreachable"
            why = ("perm_min=%d > GMEXECUTE 抬到的 %d ⇒ 即使脚本调用也被权限门挡住"
                   % (perm, GMEXECUTE_PERMISSION))
        else:
            verdict = "unreachable"
            why = "perm_min>0 且部署脚本无该命令的可达 GMEXECUTE 调用 ⇒ 被权限门挡住"
        row = {"cmd": r["name"], "cmd_name": m["cmd_name"] if m else None,
               "child_card": CHILD_CARDS.get(r["name"]),
               "rust_imp": r["imp"], "rust_perm_min": r["perm_min"],
               "handler": handler, "perm_min": perm, "perm_source": perm_src,
               "verdict": verdict, "verdict_why": why,
               "script_evidence": ev}
        row.update(row_extra)
        rows.append(row)

    script_reachable_implemented = []
    for occ in scripts["occurrences"]:
        if not occ["label_reachable"] or not occ["resolved_cmd"]:
            continue
        if occ["resolved_rust_imp"] and occ["resolved_rust_imp"] != "Unimplemented":
            script_reachable_implemented.append({
                "cmd": occ["resolved_cmd"], "file": occ["file"], "line": occ["line"],
                "label": occ["label"], "rust_imp": occ["resolved_rust_imp"],
                "third_state": occ["resolved_rust_imp"] in THIRD_STATE_IMPS,
            })

    third_state = [r for r in rust if r["imp"] in THIRD_STATE_IMPS]
    doc = {
        "format": "gm-registry/2",
        "deployment": {
            "oracle": deployment,
            "scope": "Release",
            "debug_caveat": (
                "oracle 面 = M2GameSvr（Release）。GameCommand.cs:33-36 在 #if DEBUG 下把每个调用者的 "
                "Permission 直接置 10 ⇒ 若以 Debug 构建（如 E:/MirServer/Mir200/GameSvr.dll，含 UTF-16LE "
                "『当前运行调试模式』）为对拍面，所有 perm_min<=10 的命令对普通玩家全部可达，"
                "本表全部 unreachable 结论立即失效（全部改判 reachable）。"),
            "other_deployments": {
                "E:/MirServer/Mir200": "Debug（GameSrv.dll 含 UTF-16LE 调试串）",
                "E:/MirServer/M2GameSvr-hook": "Release（同一 debug 串检测为否）",
            },
        },
        "sources": {
            "rust_table": {"path": relpath(RUST_TABLE), "sha256": sha256(RUST_TABLE)},
            "gamecommands": {"path": gamecmds["path"], "sha256": gamecmds["sha256"],
                             "fields": len(gamecmds["register"])},
            "gamecmd_conf": {"path": gamecmdconf["path"], "sha256": gamecmdconf["sha256"],
                             "bound_fields": len(gamecmdconf["bindings"])},
            "command_conf": {"path": conf_path, "state": conf_state,
                             "sha256": sha256(conf_path) if conf_state == "present" else None},
            "commands_dir": {"path": relpath(CS_DIR), "classes": len(classes)},
            "envir": {"path": envir_path, "loaded_files": scripts["loaded_count"],
                      "missing_roots": scripts["missing_roots"],
                      "gmexecute_files": sorted({o["file"] for o in scripts["occurrences"]}
                                                | {h["file"] for h in scripts["unloaded_hits"]}),
                      "script_sha256": {
                          f: sha256(os.path.join(envir_path, f))
                          for f in sorted({o["file"] for o in scripts["occurrences"]}
                                          | {h["file"] for h in scripts["unloaded_hits"]})}},
            "engine_label_roots": {"exact": sorted(roots["exact"]),
                                   "prefix": sorted(roots["prefix"]),
                                   "sources": roots["sources"][:40]},
        },
        "assumptions": {
            "start_permission": 0,
            "get_hum_permission": "commented（PlayObject.cs:1188-1194）",
            "adminlist": "无有效 IP 段（Envir/adminlist.txt 仅一行注释）",
            "gmexecute_permission": GMEXECUTE_PERMISSION,
            "script_channel": ("GMEXECUTE（ExecutionCode.cs:464 / ExecutionProcessingSys.cs:4925-4947）"
                               "先把 playerActor.Permission 置 10，再 ProcessUserLineMsg ⇒ 同一条权限门"),
            "build_reproducible": "本工具以 utf-8 + LF 写出，build 后工作区应保持干净",
        },
        "rule": (
            "perm_min==0 ⇒ reachable；0<perm_min<=10 且部署脚本存在可达 GMEXECUTE 调用 ⇒ script-reachable；"
            "perm_min>0 且无脚本证据 ⇒ unreachable；无命令体 ⇒ structural-unreachable；任一源缺失 ⇒ undetermined"),
        "crosscheck": crosscheck,
        "warnings": warnings,
        "unknown_script_commands": unknown_cmds,
        "script_occurrence_summary": {
            "total": len(scripts["occurrences"]),
            "label_reachable": sum(1 for o in scripts["occurrences"] if o["label_reachable"]),
            "label_dead": sum(1 for o in scripts["occurrences"] if not o["label_reachable"]),
            "unloaded_files_hit": sorted({h["file"] for h in scripts["unloaded_hits"]}),
        },
        "script_occurrences": [
            {k: o[k] for k in ("file", "line", "label", "section", "script_arg",
                               "resolved_cmd", "label_reachable", "label_path", "raw")}
            for o in scripts["occurrences"]],
        "unloaded_script_hits": scripts["unloaded_hits"],
        "dead_gmexecute_labels": scripts["dead_labels"],
        "review_corrections": REVIEW_CORRECTIONS,
        "script_reachable_but_implemented": script_reachable_implemented,
        "commands": rows,
    }
    doc["summary"] = summarize_rows(rows, rust, third_state, doc)
    return doc


def summarize_rows(rows, rust, third_state, doc=None):
    by_verdict = {}
    for r in rows:
        by_verdict[r["verdict"]] = by_verdict.get(r["verdict"], 0) + 1
    imp = {}
    for r in rust:
        key = "Unimplemented" if r["imp"] == "Unimplemented" else "Implemented"
        imp[key] = imp.get(key, 0) + 1
    return {
        "total": len(rows), "by_verdict": by_verdict,
        "reachable": by_verdict.get("reachable", 0),
        "script_reachable": by_verdict.get("script-reachable", 0),
        "unreachable": by_verdict.get("unreachable", 0),
        "structural_unreachable": by_verdict.get("structural-unreachable", 0),
        "undetermined": by_verdict.get("undetermined", 0),
        "table_total": len(rust),
        "table_imp_breakdown": imp,
        "third_state": len(third_state),
        "third_state_imps": sorted(THIRD_STATE_IMPS),
        "complete_implemented": imp.get("Implemented", 0) - len(third_state),
        "script_occurrences": (doc or {}).get("script_occurrence_summary"),
    }


# --------------------------------------------------------------------------
# 5. 门禁
# --------------------------------------------------------------------------
def _row_diff(want, got, limit=25):
    diffs = []
    for a, b in zip(want, got):
        for k in sorted(set(a) | set(b)):
            if a.get(k) != b.get(k):
                diffs.append("%s.%s: 登记=%r 源码=%r" % (a.get("cmd"), k, a.get(k), b.get(k)))
                if len(diffs) >= limit:
                    return diffs
    if len(want) != len(got):
        diffs.append("行数不一致：登记=%d 源码=%d" % (len(got), len(want)))
    return diffs


def check_registry(doc, conf_path, envir_path, deploy_dir, recomputed=None):
    problems = []
    if doc.get("format") != "gm-registry/2":
        problems.append("format=%r（应为 gm-registry/2）" % doc.get("format"))

    # (a) 与源码逐字段一致（不是自洽，而是真值）
    fresh = recomputed if recomputed is not None else build(conf_path, envir_path, deploy_dir)
    if not fresh["crosscheck"]["ok"]:
        problems.append("取数口径自校验失败（推导表与 gmcmd.rs 不一致）：%r" % fresh["crosscheck"])
    problems.extend(_row_diff(fresh["commands"], doc.get("commands", [])))
    if fresh.get("summary") != doc.get("summary"):
        for k in sorted(set(fresh.get("summary", {})) | set(doc.get("summary", {}))):
            if fresh["summary"].get(k) != doc["summary"].get(k):
                problems.append("summary.%s: 登记=%r 源码=%r"
                                % (k, doc["summary"].get(k), fresh["summary"].get(k)))

    # (b) 独立断言：row 内部一致性 + 与源码的语义一致性
    maps, _custom, _w = resolve_command_maps(parse_gamecommands(), parse_gamecmdconf(),
                                             parse_conf(conf_path)[0], parse_command_classes())
    lower_maps = {k.lower(): v for k, v in maps.items()}
    deployed_evidence = {}
    for r in fresh["commands"]:
        for e in r["script_evidence"]:
            deployed_evidence.setdefault(r["cmd"], []).append(e)
    for r in doc.get("commands", []):
        cmd = r.get("cmd")
        verdict = r.get("verdict")
        handler = r.get("handler") or ""
        perm = r.get("perm_min")
        if verdict == "undetermined":
            problems.append("%s: verdict=undetermined（%s）" % (cmd, r.get("verdict_why")))
            continue
        if handler and ":" in handler:
            path, _, line_s = handler.rpartition(":")
            full = os.path.join(REPO, path)
            if not os.path.exists(full):
                problems.append("%s: handler 文件不存在 %s" % (cmd, path))
            else:
                lines = read_text(full, "utf-8-sig").splitlines()
                try:
                    line = int(line_s)
                except ValueError:
                    problems.append("%s: handler 行号非整数 %r" % (cmd, line_s))
                    continue
                if line < 1 or line > len(lines):
                    problems.append("%s: handler 行号越界 %s:%d（文件 %d 行）"
                                    % (cmd, path, line, len(lines)))
                elif "[Command(" not in lines[line - 1]:
                    problems.append("%s: handler 行不含 [Command(（%s:%d）" % (cmd, path, line))
                else:
                    # 该行所属类的注册名必须就是本行的 cmd（挡「在界但指错命令」）
                    cls = _class_at(lines, line)
                    info = _class_attr(os.path.join(REPO, path), cls) if cls else None
                    if info is None:
                        problems.append("%s: 无法从 %s:%d 解析出命令类" % (cmd, path, line))
                    else:
                        final = _final_name_for_class(info, maps)
                        if final is None:
                            problems.append("%s: %s 的 [Command] 名 %r 在命令表里解析不出最终名"
                                            % (cmd, cls, info["attr_name"]))
                        elif final.lower() != cmd.lower():
                            problems.append("%s: handler 指向别的命令（%s:%d ⇒ %r，应为 %r）"
                                            % (cmd, path, line, final, cmd))
        elif r.get("register_command"):
            problems.append("%s: register_command=true 但 handler 为空（自相矛盾）" % cmd)
        if perm is None and verdict != "undetermined":
            problems.append("%s: perm_min=null 但 verdict=%s（不得判权限型结论）" % (cmd, verdict))
        if perm == 0 and verdict != "reachable":
            problems.append("%s: perm_min==0 但 verdict=%s（禁止把可达命令登记成不可达）" % (cmd, verdict))
        if isinstance(perm, int) and perm > 0 and verdict == "reachable":
            problems.append("%s: perm_min=%d>0 但 verdict=reachable（判据不一致）" % (cmd, perm))
        if verdict == "script-reachable":
            if not isinstance(perm, int) or not (0 < perm <= GMEXECUTE_PERMISSION):
                problems.append("%s: script-reachable 但 perm_min=%r 不在 (0,%d]"
                                % (cmd, perm, GMEXECUTE_PERMISSION))
            ev = r.get("script_evidence") or []
            if not ev:
                problems.append("%s: script-reachable 但没有 script_evidence" % cmd)
            for e in ev:
                p = os.path.join(envir_path, e.get("file", ""))
                if not os.path.exists(p):
                    problems.append("%s: 脚本证据文件不存在 %r" % (cmd, e.get("file")))
                    continue
                lines = read_gbk(p).splitlines()
                idx = e.get("line", 0)
                if idx < 1 or idx > len(lines):
                    problems.append("%s: 脚本证据行号越界 %s:%s" % (cmd, e.get("file"), idx))
                    continue
                if not GMEXECUTE_RE.match(lines[idx - 1].strip()):
                    problems.append("%s: 脚本证据 %s:%s 不是 GMEXECUTE 行"
                                    % (cmd, e.get("file"), idx))
                    continue
                arg = GMEXECUTE_RE.match(lines[idx - 1].strip()).group(1).strip().split()
                arg = arg[0].lower() if arg else ""
                target = lower_maps.get(arg)
                if not target or target["cmd_name"].lower() != cmd.lower():
                    problems.append("%s: 脚本证据 %s:%s 的实参 %r 解析不到本命令"
                                    % (cmd, e.get("file"), idx, arg))
            if cmd not in deployed_evidence:
                problems.append("%s: 声称 script-reachable，但本次源码扫描没有任何可达脚本调用" % cmd)
            if not r.get("child_card"):
                problems.append("%s: script-reachable 但没链接子卡（child_card 为空）" % cmd)
        if verdict == "unreachable":
            if deployed_evidence.get(cmd):
                problems.append("%s: 登记 unreachable，但部署脚本里有 %d 处可达 GMEXECUTE 调用"
                                % (cmd, len(deployed_evidence[cmd])))
        if verdict == "structural-unreachable":
            if cmd in maps:
                problems.append("%s: 登记 structural-unreachable，但命令表里解析到了 handler %s"
                                % (cmd, maps[cmd]["handler"]))
    # (c) 部署构型必须显式声明
    dep = (doc.get("deployment") or {}).get("oracle") or {}
    if not dep.get("build"):
        problems.append("deployment.oracle.build 未声明（Release/Debug 必须显式）")
    if not (doc.get("deployment") or {}).get("debug_caveat"):
        problems.append("deployment.debug_caveat 为空（必须写明 Debug 面会推翻哪些结论）")
    return problems


def _class_at(lines, line):
    """从 [Command( 行向下找到 class 声明（跳过其它属性/修饰符）。"""
    for i in range(line - 1, min(len(lines), line + 6)):
        m = re.search(r"\bclass\s+(\w+)", lines[i])
        if m:
            return m.group(1)
    return None


_CLASS_CACHE = {}


def _class_attr(path, cls):
    if path not in _CLASS_CACHE:
        classes = {}
        text = read_text(path, "utf-8-sig")
        for m in CS_CLASS_RE.finditer(text):
            vals = split_attr_args(m.group("args"))
            name = vals[0] if vals and isinstance(vals[0], str) else None
            classes[m.group("cls")] = {"attr_name": name,
                                       "line": text.count("\n", 0, m.start()) + 1}
        _CLASS_CACHE[path] = classes
    return _CLASS_CACHE[path].get(cls)


def _final_name_for_class(info, maps):
    """类的 [Command] 属性名 → 最终注册名（经 customCommandMap 改名）。"""
    key = info.get("attr_name")
    if not key:
        return None
    for final, row in maps.items():
        if row["attr_name"] == key:
            return final
    return None


# --------------------------------------------------------------------------
# 6. 命令实现
# --------------------------------------------------------------------------
def render(doc):
    return (json.dumps(doc, ensure_ascii=False, indent=1) + "\n").encode("utf-8")


def cmd_build(args):
    doc = build(args.conf, args.envir, args.deploy)
    with open(REGISTRY, "wb") as fh:
        fh.write(render(doc))
    print("WROTE %s (%d bytes)" % (REGISTRY, len(render(doc))))
    print_summary(doc["summary"])
    return 0 if doc["summary"]["undetermined"] == 0 else 2


def print_summary(s):
    print("GM_REGISTRY total=%d reachable=%d script-reachable=%d unreachable=%d "
          "structural-unreachable=%d undetermined=%d"
          % (s["total"], s["reachable"], s["script_reachable"], s["unreachable"],
             s["structural_unreachable"], s["undetermined"]))
    print("verdict_breakdown:", json.dumps(s["by_verdict"], ensure_ascii=False, sort_keys=True))
    print("table_total=%d imp_breakdown=%s third_state=%d complete=%d"
          % (s["table_total"], json.dumps(s["table_imp_breakdown"], sort_keys=True),
             s["third_state"], s["complete_implemented"]))


def cmd_summary(args):
    if not os.path.exists(REGISTRY):
        fail("无法判定：%s 不存在，先跑 build" % REGISTRY)
    doc = json.loads(read_text(REGISTRY, "utf-8"))
    s = doc["summary"]
    print_summary(s)
    src = {}
    for r in doc["commands"]:
        src[str(r["perm_source"])] = src.get(str(r["perm_source"]), 0) + 1
    print("perm_source:", json.dumps(src, ensure_ascii=False, sort_keys=True))
    print("crosscheck:", json.dumps(doc["crosscheck"], ensure_ascii=False, sort_keys=True))
    print("script_occurrences:", json.dumps(doc["script_occurrence_summary"],
                                            ensure_ascii=False, sort_keys=True))
    print("unknown_script_commands:", json.dumps(doc["unknown_script_commands"], ensure_ascii=False))
    print("script_reachable_but_implemented:", json.dumps(
        doc["script_reachable_but_implemented"], ensure_ascii=False))
    for r in doc["commands"]:
        if r["verdict"] != "unreachable":
            print("  %-9s %-16s perm=%s handler=%s" % (r["verdict"], r["cmd"], r["perm_min"], r["handler"]))
    return 0


def cmd_gate(args):
    if not os.path.exists(REGISTRY):
        fail("无法判定：%s 不存在，先跑 build" % REGISTRY)
    raw = open(REGISTRY, "rb").read()
    doc = json.loads(raw.decode("utf-8"))
    problems = check_registry(doc, args.conf, args.envir, args.deploy)
    if render(doc) != raw:
        problems.append("登记表不是本工具的规范序列化（utf-8 + LF + indent=1）⇒ 重跑 build 会有 diff")
    if problems:
        print("RED: %d 个问题" % len(problems))
        for p in problems[:60]:
            print("  -", p)
        return 1
    print("GREEN: gm_registry %d 条与源码逐字段一致（handler/perm/verdict/脚本证据全部复算通过）"
          % len(doc["commands"]))
    return 0


# --------------------------------------------------------------------------
# 7. selftest（牙）
# --------------------------------------------------------------------------
def _recompute_with_start_permission(rows, start_permission):
    """假设玩家自然 Permission = start_permission（而不是 0）。

    - 结构性缺命令体的行**不受权限假设影响**（I3 修正：v1 把它们一并翻成 reachable，掩盖了 B2）；
    - perm_min <= start_permission 的行改判 reachable；
    - perm_min > start_permission 的行保持原判（脚本证据仍可使其 script-reachable）。
    """
    out = []
    for r in rows:
        row = dict(r)
        perm = r.get("perm_min")
        if row["verdict"] == "structural-unreachable" or perm is None:
            out.append(row)
            continue
        if perm <= start_permission:
            row["verdict"] = "reachable"
            row["verdict_why"] = "StartPermission=%d ⇒ 自然可达" % start_permission
        out.append(row)
    return out


def cmd_selftest(args):
    fails = []
    raw = open(REGISTRY, "rb").read() if os.path.exists(REGISTRY) else None
    if raw is None:
        fail("无法判定：%s 不存在，先跑 build" % REGISTRY)
    base_sha = hashlib.sha256(raw).hexdigest()
    base = json.loads(raw.decode("utf-8"))
    fresh = build(args.conf, args.envir, args.deploy)

    def tooth(name, fn):
        try:
            fn()
            now = hashlib.sha256(open(REGISTRY, "rb").read()).hexdigest()
            assert now == base_sha, ("牙执行后登记表字节变了：%s != %s"
                                     % (now[:16], base_sha[:16]))
            print("  [ok] %s  [sha256 %s 未变]" % (name, now[:16]))
        except AssertionError as ex:
            fails.append("%s: %s" % (name, ex))

    def gate_red(doc, needle, what):
        probs = check_registry(doc, args.conf, args.envir, args.deploy, recomputed=fresh)
        assert probs, "%s 后 gate 未红" % what
        assert any(needle in p for p in probs), \
            "%s 后 gate 红了但不是预期原因（needle=%r，前 6 条=%r）" % (what, needle, probs[:6])

    def row_with_handler(doc):
        return next((r for r in doc["commands"] if r.get("handler")), None)

    print("gm_registry selftest（牙）:")

    def t_a():
        doc = copy.deepcopy(base)
        row = doc["commands"][0]
        other = next(r for r in doc["commands"] if r["handler"] and r["handler"] != row["handler"])
        row["handler"] = other["handler"]
        gate_red(doc, row["cmd"], "A 在界指错命令")

    def t_b():
        doc = copy.deepcopy(base)
        row = row_with_handler(doc)
        assert row and isinstance(row["perm_min"], int), "样本没有带整数 perm 的行"
        row["perm_min"] = 8
        gate_red(doc, row["cmd"], "B perm 10→8")

    def t_c():
        doc = copy.deepcopy(base)
        row = doc["commands"][0]
        other = next(r for r in doc["commands"] if r["cmd"] != row["cmd"])
        row["cmd"] = other["cmd"]
        gate_red(doc, "", "C 改 cmd 名")

    def t_d():
        doc = copy.deepcopy(base)
        row = row_with_handler(doc)
        assert row is not None, "样本没有带 handler 的行"
        row["handler"] = None
        row["register_command"] = True
        gate_red(doc, row["cmd"], "D register_command=true + handler=null")

    def t_e():
        doc = copy.deepcopy(base)
        row = next(r for r in doc["commands"] if r["verdict"] == "unreachable")
        row["perm_min"] = None
        gate_red(doc, row["cmd"], "E perm=null + unreachable")

    def t_f():
        doc = copy.deepcopy(base)
        row = row_with_handler(doc)
        row["perm_min"] = 0
        row["verdict"] = "unreachable"
        gate_red(doc, row["cmd"], "F perm_min=0 却判 unreachable")

    def t_g():
        doc = copy.deepcopy(base)
        row = row_with_handler(doc)
        row["handler"] = row["handler"].rsplit(":", 1)[0] + ":999999"
        gate_red(doc, row["cmd"], "G 行号越界")

    def t_h():
        doc = copy.deepcopy(base)
        row = next(r for r in doc["commands"] if r["verdict"] == "unreachable")
        row["verdict"] = "script-reachable"
        row["script_evidence"] = []
        gate_red(doc, row["cmd"], "H script-reachable 但无证据")

    def t_i():
        doc = copy.deepcopy(base)
        row = next(r for r in doc["commands"] if r["verdict"] == "unreachable")
        row["verdict"] = "script-reachable"
        row["script_evidence"] = [{"file": "MapQuest_def/QManage.txt", "line": 1,
                                   "label": "@resume", "section": "act",
                                   "script_arg": "x", "raw": "[@Resume]",
                                   "label_path": None, "load_origin": None}]
        gate_red(doc, row["cmd"], "I 伪造脚本证据（该行不是 GMEXECUTE）")

    def t_j():
        doc = copy.deepcopy(base)
        row = next(r for r in doc["commands"] if r["cmd"].lower() == "fdgfdg56")
        assert row["handler"] and row["handler"].endswith("SetPassWordCommand.cs:177"), \
            "conf 键 StorageLock ⇒ field Lock ⇒ SetPassWordCommand.cs:177 的口径没生效：%r" % row["handler"]
        row["cmd_name"] = "StorageLock"  # v1 的错误口径：把 conf 键当 CmdName
        gate_red(doc, "fdgfdg56", "J conf 键当 CmdName")

    def t_k():
        doc = copy.deepcopy(base)
        row = next(r for r in doc["commands"] if r["verdict"] == "script-reachable")
        row["script_evidence"] = [dict(row["script_evidence"][0])]
        row["script_evidence"][0]["file"] = "Market_Def/QFunction-0 - 副本.txt"
        row["script_evidence"][0]["line"] = 6
        gate_red(doc, row["cmd"], "K 把证据指到未加载的副本脚本")

    def t_l():
        rows = base["commands"]
        sp10 = _recompute_with_start_permission(rows, 10)
        bad = [r for r in sp10 if r["perm_min"] is not None and r["perm_min"] <= 10
               and r["verdict"] not in ("reachable", "script-reachable")]
        assert not bad, "StartPermission=10 后仍有 perm<=10 的非可达行：%r" % bad[:3]
        sp1 = _recompute_with_start_permission(rows, 1)
        still = [r for r in sp1 if r["perm_min"] == 10 and r["verdict"] != "script-reachable"]
        assert still, "StartPermission=1 竟然把 perm=10 的行翻成可达（v1 的错误模型）"
        structural = [r for r in sp1 if r["verdict"] == "structural-unreachable"]
        assert all(r["handler"] is None for r in structural), \
            "结构性不可达的行在 StartPermission 假设下被翻掉了（I3）"

    def t_m():
        # 名字口径的独立自证：Map / MapInfo 必须分别指向 MapInfoCommand / ShowMapInfoCommand
        m_map = next((r for r in base["commands"] if r["cmd"] == "Map"), None)
        m_info = next((r for r in base["commands"] if r["cmd"] == "MapInfo"), None)
        assert m_map and m_map["handler"] and m_map["handler"].endswith("MapInfoCommand.cs:8"), \
            "Map 的 handler 应为 MapInfoCommand.cs:8，实际 %r" % (m_map or {}).get("handler")
        assert m_info and m_info["handler"] and m_info["handler"].endswith("ShowMapInfoCommand.cs:6"), \
            "MapInfo 的 handler 应为 ShowMapInfoCommand.cs:6，实际 %r" % (m_info or {}).get("handler")

    tooth("A 在界指错命令 ⇒ gate 红", t_a)
    tooth("B perm 10→8 ⇒ gate 红", t_b)
    tooth("C 改 cmd 名 ⇒ gate 红", t_c)
    tooth("D register_command=true + handler=null ⇒ gate 红", t_d)
    tooth("E perm=null + unreachable ⇒ gate 红", t_e)
    tooth("F perm_min=0 却判 unreachable ⇒ gate 红", t_f)
    tooth("G handler 行号越界 ⇒ gate 红", t_g)
    tooth("H script-reachable 无证据 ⇒ gate 红", t_h)
    tooth("I 伪造脚本证据 ⇒ gate 红", t_i)
    tooth("J 把 conf 键当 CmdName（B2 原缺陷）⇒ gate 红", t_j)
    tooth("K 证据指向未加载副本脚本 ⇒ gate 红", t_k)
    tooth("L StartPermission=1/10 重算：结构性行不被翻、perm=10 不被 sp=1 翻", t_l)
    def t_n():
        doc = copy.deepcopy(base)
        row = next(r for r in doc["commands"] if r["verdict"] == "script-reachable")
        row["child_card"] = None
        gate_red(doc, row["cmd"], "N script-reachable 未链接子卡")

    tooth("M Map / MapInfo handler 未对调", t_m)
    tooth("N script-reachable 未链接子卡 ⇒ gate 红", t_n)

    if base_sha != hashlib.sha256(open(REGISTRY, "rb").read()).hexdigest():
        fails.append("牙把登记表写坏了（sha256 变化）")
    # 内存副本还原的逐字节证明
    assert render(base) == raw, "内存副本重新序列化后与磁盘字节不一致"
    assert base_sha == hashlib.sha256(raw).hexdigest()
    if fails:
        print("gm_registry selftest: FAIL")
        for f in fails:
            print("  -", f)
        return 1
    print("gm_registry selftest: OK (14 teeth; registry sha256=%s 未变)" % base_sha[:16])
    return 0


def main(argv=None):
    ap = argparse.ArgumentParser(description="GM 命令可达性登记 + 门禁")
    ap.add_argument("cmd", choices=("build", "gate", "summary", "selftest"))
    ap.add_argument("--conf", default=DEFAULT_CONF, help="部署 Command.conf（缺失则记 absent）")
    ap.add_argument("--envir", default=DEFAULT_ENVIR, help="部署 Envir 根目录（脚本证据来源）")
    ap.add_argument("--deploy", default=DEFAULT_DEPLOY, help="部署根目录（构型判定）")
    args = ap.parse_args(argv)
    return {"build": cmd_build, "gate": cmd_gate, "summary": cmd_summary,
            "selftest": cmd_selftest}[args.cmd](args)


if __name__ == "__main__":
    sys.exit(main())
