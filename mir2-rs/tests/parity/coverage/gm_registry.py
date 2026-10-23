#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""gm_registry.py — GM 命令「可达性登记 + 门禁」（卡 openmir2-gm-reachability-registry）。

口径（卡 acceptance 写死）
--------------------------
  perm_min > 0 ∧ 现网 StartPermission=0 ∧ GetHumPermission 被注释 ∧ AdminList 无有效 IP 段
      ⇒ verdict = unreachable（oracle 现网跑不到 ⇒ Rust 不实现即为行为等价）
  perm_min == 0 ⇒ verdict = reachable，必须开实现卡（这类不许登记成不可达）
  另有结构性不可达：GameCommands 声明了 CmdName 但**没有 [RegisterCommand] 处理类** ⇒ 现网无命令体。

数据源（三方交叉）
------------------
  rust : mir2-rs/bins/game-svr/src/gmcmd.rs 的 COMMANDS（name=现网拼写 / perm_min / imp）
  gcmd : src/Modules/GameCommand/GameCommands.cs —— PermissionMin 默认值来源 + [RegisterCommand] 处理类链接
  conf : 部署 Command.conf 的 [Command]（CmdName=现网名）与 [Permission]（CmdName=perm 覆盖）；
         缺省 E:/MirServer/M2GameSvr/Command.conf，可用 --conf 指定；不存在则记 absent
  cs   : src/Modules/GameCommand/Commands/*.cs —— 处理类的 [Command(…)] 行（handler file:line）

优先级：conf[Permission] > GameCommands.cs(PerMissionMin) > [Command] 属性 minUserLevel

子命令
------
  build / gate / summary / selftest（三颗牙）
退出码：0 绿 / 1 红 / 2 无法判定
"""
import argparse
import hashlib
import json
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.abspath(os.path.join(HERE, "..", "..", "..", ".."))
RUST_TABLE = os.path.join(REPO, "mir2-rs", "bins", "game-svr", "src", "gmcmd.rs")
GAMECMDS = os.path.join(REPO, "src", "Modules", "GameCommand", "GameCommands.cs")
CS_DIR = os.path.join(REPO, "src", "Modules", "GameCommand", "Commands")
REGISTRY = os.path.join(HERE, "gm_registry.json")
DEFAULT_CONF = os.path.join("E:" + os.sep, "MirServer", "M2GameSvr", "Command.conf")

CMD_DEF_RE = re.compile(
    r"CommandDef\s*\{\s*name:\s*\"(?P<name>[^\"]+)\",\s*perm_min:\s*(?P<perm>\d+),"
    r"\s*params:\s*(?P<params>\d+),\s*imp:\s*Imp::(?P<imp>\w+)\s*,\s*\}", re.S)
GC_REG_RE = re.compile(
    r"\[RegisterCommand\(\s*typeof\(\s*(?P<cls>\w+)\s*\)\s*\)\]\s*"
    r"public\s+readonly\s+GameCmd\s+(?P<field>\w+)\s*;", re.S)
GC_DEF_RE = re.compile(
    r"(?P<field>\w+)\s*=\s*new\s+GameCmd\s*\{\s*CmdName\s*=\s*\"(?P<name>[^\"]+)\""
    r"[^}]*?PerMissionMin\s*=\s*(?P<perm>\d+)[^}]*\}\s*;", re.S)
CS_ATTR_RE = re.compile(r"^\s*\[Command\((?P<args>.*)\)\]\s*$")
CS_EXEC_RE = re.compile(r"^\s*public\s+void\s+Execute\s*\(")


def sha256(path):
    h = hashlib.sha256()
    with open(path, "rb") as fh:
        for chunk in iter(lambda: fh.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def read_text(path, encoding="utf-8"):
    with open(path, "r", encoding=encoding, errors="replace") as fh:
        return fh.read()


def parse_rust(path=RUST_TABLE):
    if not os.path.exists(path):
        sys.stderr.write("无法判定：缺少 %s\n" % path)
        raise SystemExit(2)
    text = read_text(path)
    start = text.find("pub const COMMANDS")
    if start < 0:
        sys.stderr.write("无法判定：%s 里找不到 COMMANDS 表\n" % path)
        raise SystemExit(2)
    rows = []
    for m in CMD_DEF_RE.finditer(text, start):
        rows.append({"name": m.group("name"), "perm_min": int(m.group("perm")),
                     "params": int(m.group("params")), "imp": m.group("imp")})
    if not rows:
        sys.stderr.write("无法判定：COMMANDS 表解析为空（格式变了？）\n")
        raise SystemExit(2)
    return rows


def parse_gamecmds(path=GAMECMDS):
    """-> {cmdNameLower: {cmd_name, field, perm_default, handler_class, line}}。"""
    if not os.path.exists(path):
        sys.stderr.write("无法判定：缺少 %s\n" % path)
        raise SystemExit(2)
    text = read_text(path)
    reg = {}
    for m in GC_REG_RE.finditer(text):
        reg[m.group("field")] = m.group("cls")
    out = {}
    for m in GC_DEF_RE.finditer(text):
        name = m.group("name").strip()
        field = m.group("field").strip()
        line = text.count("\n", 0, m.start()) + 1
        out[name.lower()] = {"cmd_name": name, "field": field,
                             "perm_default": int(m.group("perm")),
                             "handler_class": reg.get(field), "line": line}
    if not out:
        sys.stderr.write("无法判定：GameCommands.cs 解析为空（格式变了？）\n")
        raise SystemExit(2)
    return out


def _attr_args(raw):
    out, buf, in_str, esc = [], "", False, False
    for ch in raw:
        if in_str:
            buf += ch
            if esc:
                esc = False
            elif ch == "\\":
                esc = True
            elif ch == '"':
                in_str = False
        elif ch == '"':
            in_str = True
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


def parse_cs(cs_dir=CS_DIR):
    """-> {attrNameLower: {name, perm_min, class, file, attr_line, exec_line}}。

    一个 .cs 文件可定义多个命令类，故扫描全部 `[Command(…)]`（不取“每文件首个”）。
    """
    if not os.path.isdir(cs_dir):
        sys.stderr.write("无法判定：缺少 %s\n" % cs_dir)
        raise SystemExit(2)
    out = {}
    for fn in sorted(os.listdir(cs_dir)):
        if not fn.endswith(".cs"):
            continue
        path = os.path.join(cs_dir, fn)
        cls = fn[:-3]
        lines = read_text(path).splitlines()
        for i, line in enumerate(lines, 1):
            m = CS_ATTR_RE.match(line)
            if not m:
                continue
            vals = _attr_args(m.group("args"))
            name = vals[0] if vals and isinstance(vals[0], str) else None
            if not name:
                continue
            perm = next((v for v in vals[1:] if isinstance(v, int)), 0)  # ctor 默认 0
            exec_line = None
            for j in range(i + 1, len(lines) + 1):
                if CS_EXEC_RE.match(lines[j - 1]):
                    exec_line = j
                    break
            key = name.lower()
            if key in out:
                continue
            out[key] = {"name": name, "perm_min": perm, "class": cls,
                        "file": os.path.relpath(path, REPO).replace(os.sep, "/"),
                        "attr_line": i, "exec_line": exec_line}
    return out


def parse_conf(path):
    """-> (rename{cmdNameLow -> 现网名}, perm{cmdNameLow -> int})；缺失返回 (None, None)。"""
    if not path or not os.path.exists(path):
        return None, None
    text = read_text(path, encoding="gbk")
    rename, perm, section = {}, {}, None
    for line in text.splitlines():
        s = line.strip()
        if not s or s.startswith(";"):
            continue
        if s.startswith("[") and s.endswith("]"):
            section = s[1:-1].lower()
            continue
        if "=" not in s:
            continue
        k, _, v = s.partition("=")
        k, v = k.strip(), v.strip()
        if section == "command":
            rename[k.lower()] = v
        elif section == "permission":
            try:
                perm[k.lower()] = int(v)
            except ValueError:
                pass
    return rename, perm


def build(conf_path):
    rust = parse_rust()
    gamecmds = parse_gamecmds()
    cs = parse_cs()
    rename, conf_perm = parse_conf(conf_path)
    conf_state = "present" if rename is not None else "absent"

    display_to_cmd, display_to_any = {}, {}
    if rename:
        for cmd_low, disp in rename.items():
            display_to_any.setdefault(disp, cmd_low)
            if cmd_low in gamecmds:
                display_to_cmd.setdefault(disp, cmd_low)

    rows = []
    for r in rust:
        if r["imp"] != "Unimplemented":
            continue
        cmd_low = display_to_cmd.get(r["name"]) or r["name"].lower()
        gc = gamecmds.get(cmd_low) if cmd_low else None

        perm_conf = conf_perm.get(cmd_low) if (conf_perm and cmd_low) else None
        perm_gc = gc["perm_default"] if gc else None
        # handler：命令体类由 [Command("CmdName", …)] 反射注册（CommandSystem 扫 Commands 程序集），
        # 因此以「属性名 == CmdName」为准；GameCommands 的 [RegisterCommand] 只作兜底证据。
        meta_attr = cs.get(cmd_low) if cmd_low else None
        handler_class = (meta_attr["class"] if meta_attr
                         else (gc["handler_class"] if gc else None))
        perm_attr = meta_attr["perm_min"] if meta_attr else None

        if perm_conf is not None:
            perm_min, perm_source = perm_conf, "conf[Permission]"
        elif perm_gc is not None:
            perm_min, perm_source = perm_gc, "GameCommands.cs"
        elif perm_attr is not None:
            perm_min, perm_source = perm_attr, "cs[Command]"
        else:
            perm_min, perm_source = None, None

        handler = exec_line = None
        if meta_attr:
            handler = "%s:%d" % (meta_attr["file"], meta_attr["attr_line"])
            exec_line = meta_attr["exec_line"]

        conf_only = (gc is None and rename is not None and r["name"] in display_to_any)
        if perm_min is None and conf_only:
            verdict = "unreachable"
            why = ("conf 有改名条目 %s=%s 但 GameCommands.cs 无该 CmdName 声明 ⇒ 现网无 GameCmd/命令体"
                   % (display_to_any[r["name"]], r["name"]))
        elif perm_min is None:
            verdict, why = "undetermined", "三源都缺（conf / GameCommands / [Command] 均无该命令）"
        elif gc is None and perm_attr is not None:
            verdict = "unreachable"
            why = ("仅有 [Command(\"%s\")] 属性、GameCommands.cs 无该 CmdName 声明 ⇒ 未注册为 GameCmd（现网无命令）"
                   % r["name"])
        elif handler is None:
            verdict = "unreachable"
            why = "GameCommands 声明 CmdName=%s（perm=%s）但无 [RegisterCommand] 处理类 ⇒ 现网无命令体" % (
                gc["cmd_name"] if gc else "?", perm_min)
        elif perm_min == 0:
            verdict, why = "reachable", "perm_min==0 ⇒ 现网可达，必须开实现卡"
        else:
            verdict = "unreachable"
            why = "perm_min>0 且 StartPermission=0 / GetHumPermission 注释 / AdminList 无有效 IP"

        rows.append({
            "cmd": r["name"], "cmd_name": gc["cmd_name"] if gc else None,
            "rust_imp": r["imp"], "rust_perm_min": r["perm_min"],
            "handler_class": handler_class, "handler": handler, "handler_exec_line": exec_line,
            "perm_min": perm_min, "perm_attr": perm_attr, "perm_source": perm_source,
            "verdict": verdict, "verdict_why": why,
            "evidence": {"conf_class": display_to_any.get(r["name"]),
                         "gamecommands_line": gc["line"] if gc else None,
                         "conf_permission": perm_conf,
                         "register_command": bool(gc and gc["handler_class"])},
        })

    doc = {
        "format": "gm-registry/1",
        "sources": {
            "rust_table": {"path": os.path.relpath(RUST_TABLE, REPO).replace(os.sep, "/"),
                           "sha256": sha256(RUST_TABLE)},
            "gamecommands": {"path": os.path.relpath(GAMECMDS, REPO).replace(os.sep, "/"),
                             "sha256": sha256(GAMECMDS)},
            "cs_dir": {"path": os.path.relpath(CS_DIR, REPO).replace(os.sep, "/"),
                       "classes": len(cs)},
            "command_conf": {"path": conf_path or DEFAULT_CONF, "state": conf_state,
                             "sha256": sha256(conf_path) if conf_state == "present" else None},
        },
        "assumptions": {
            "start_permission": 0,
            "get_hum_permission": "commented（PlayObject.cs:1188-1194）",
            "adminlist": "无有效 IP 段（Envir/AdminList.txt 仅一行，无 IP 段）",
        },
        "rule": "perm_min==0 ⇒ reachable（必须开实现卡）；perm_min>0 或无处理类 ⇒ unreachable",
        "commands": rows,
    }
    doc["summary"] = summarize_rows(rows)
    return doc


def summarize_rows(rows):
    by_verdict = {}
    for r in rows:
        by_verdict[r["verdict"]] = by_verdict.get(r["verdict"], 0) + 1
    return {"total": len(rows), "by_verdict": by_verdict,
            "reachable": by_verdict.get("reachable", 0),
            "unreachable": by_verdict.get("unreachable", 0),
            "undetermined": by_verdict.get("undetermined", 0)}


def print_summary(s):
    print("GM_REGISTRY total=%d reachable=%d unreachable=%d undetermined=%d"
          % (s["total"], s["reachable"], s["unreachable"], s["undetermined"]))


def cmd_build(args):
    doc = build(args.conf)
    with open(REGISTRY, "w", encoding="utf-8") as fh:
        json.dump(doc, fh, ensure_ascii=False, indent=1)
        fh.write("\n")
    print("WROTE %s" % REGISTRY)
    print_summary(doc["summary"])
    return 0 if doc["summary"]["undetermined"] == 0 else 2


def cmd_summary(args):
    if not os.path.exists(REGISTRY):
        sys.stderr.write("无法判定：%s 不存在，先跑 build\n" % REGISTRY)
        return 2
    doc = json.load(open(REGISTRY, encoding="utf-8"))
    print_summary(doc["summary"])
    imp, src, why = {}, {}, {}
    for r in doc["commands"]:
        imp[r["rust_imp"]] = imp.get(r["rust_imp"], 0) + 1
        src[str(r["perm_source"])] = src.get(str(r["perm_source"]), 0) + 1
        if r["verdict"] == "unreachable":
            key = "no_handler" if r["handler"] is None else "perm_gt_0"
            why[key] = why.get(key, 0) + 1
    print("imp:", json.dumps(imp, ensure_ascii=False, sort_keys=True))
    print("perm_source:", json.dumps(src, ensure_ascii=False, sort_keys=True))
    print("unreachable_breakdown:", json.dumps(why, ensure_ascii=False, sort_keys=True))
    table = parse_rust()
    full = {}
    for r in table:
        key = "Unimplemented" if r["imp"] == "Unimplemented" else "Implemented"
        full[key] = full.get(key, 0) + 1
    print("table_total=%d imp_breakdown=%s" % (len(table), json.dumps(full, sort_keys=True)))
    return 0


def scan(path):
    if not os.path.exists(path):
        return None
    return read_text(path).splitlines()


def check_registry(doc):
    problems = []
    for r in doc["commands"]:
        if r["verdict"] == "undetermined":
            problems.append("%s: verdict=undetermined（%s）" % (r["cmd"], r["verdict_why"]))
            continue
        handler = r.get("handler") or ""
        if ":" not in handler:
            if r.get("verdict") != "unreachable":
                problems.append("%s: 无 handler 但 verdict=%s（只有结构性不可达可缺 handler）"
                                % (r["cmd"], r.get("verdict")))
            continue
        path, _, line_s = handler.rpartition(":")
        lines = scan(os.path.join(REPO, path))
        if lines is None:
            problems.append("%s: handler 文件不存在 %s" % (r["cmd"], path))
            continue
        try:
            line = int(line_s)
        except ValueError:
            problems.append("%s: handler 行号非整数 %r" % (r["cmd"], line_s))
            continue
        if line < 1 or line > len(lines):
            problems.append("%s: handler 行号越界 %s:%d（文件 %d 行）"
                            % (r["cmd"], path, line, len(lines)))
            continue
        if "[Command(" not in lines[line - 1]:
            problems.append("%s: handler 行不含 [Command( 属性（%s:%d）" % (r["cmd"], path, line))
        perm = r.get("perm_min")
        if perm == 0 and r["verdict"] != "reachable":
            problems.append("%s: perm_min==0 但 verdict=%s（禁止把可达命令登记成不可达）"
                            % (r["cmd"], r["verdict"]))
        if isinstance(perm, int) and perm > 0 and r["verdict"] == "reachable":
            problems.append("%s: perm_min=%s>0 但 verdict=reachable（判据不一致）"
                            % (r["cmd"], perm))
    return problems


def cmd_gate(args):
    if not os.path.exists(REGISTRY):
        sys.stderr.write("无法判定：%s 不存在，先跑 build\n" % REGISTRY)
        return 2
    doc = json.load(open(REGISTRY, encoding="utf-8"))
    problems = check_registry(doc)
    if problems:
        print("RED: %d 个问题" % len(problems))
        for p in problems[:40]:
            print("  -", p)
        return 1
    print("GREEN: gm_registry %d 条全部通过（handler 存在或结构性缺省 ∧ 行号在界 ∧ verdict 与 perm_min 一致）"
          % len(doc["commands"]))
    return 0


def _recompute(rows, start_permission):
    """牙③：静态不可达的前提是 `StartPermission=0`（玩家 Permission 恒 0 ⇒ 任何 perm>0 的门都过不去）。

    StartPermission>0 时该前提不成立 —— 玩家可能带着 ≥perm 的权限进来，静态不可达不再成立，
    必须交回实机判定 ⇒ 一律记 reachable（待实机核），这就是“判据依赖实值”的证明。
    """
    out = []
    for r in rows:
        perm = r.get("perm_min")
        if perm is None:
            v = "undetermined"
        elif start_permission == 0 and perm > 0:
            v = "unreachable"
        else:
            v = "reachable"
        out.append(dict(r, verdict=v))
    return out


def cmd_selftest(args):
    fails = []

    def tooth(name, fn):
        try:
            fn()
            print("  [ok] %s" % name)
        except AssertionError as ex:
            fails.append("%s: %s" % (name, ex))

    print("gm_registry selftest（三颗牙）:")
    doc = build(args.conf)
    rows = doc["commands"]

    def tooth1():
        # 真实样本里 88 条全是 perm>0（可达的 6 条已实现、不在本表），故**合成**一条 perm_min=0 的行，
        # 借用真实 handler 以保证只触发“禁令”这一条判据。
        real = next((r for r in rows if r.get("handler")), None)
        assert real is not None, "样本里没有带 handler 的行，牙①无从构造"
        zero = dict(real, cmd="SELFTEST_ZERO", perm_min=0, verdict="reachable")
        bad = [dict(r) for r in rows] + [zero]
        bad[-1]["verdict"] = "unreachable"
        problems = check_registry({"commands": bad})
        assert any("禁止把可达命令登记成不可达" in p for p in problems), \
            "把 perm_min==0 改成 unreachable 后 gate 未红：%r" % problems[:3]

    def tooth2():
        bad = [dict(r) for r in rows]
        cand = [r for r in bad if r.get("handler")]
        assert cand, "样本没有带 handler 的命令，牙②无从构造"
        cand[0]["handler"] = cand[0]["handler"].rsplit(":", 1)[0] + ":999999"
        problems = check_registry({"commands": bad})
        assert any("越界" in p for p in problems), "行号越界后 gate 未红：%r" % problems[:3]

    def tooth3():
        flipped = _recompute(rows, 1)
        bad = [r for r in flipped if r.get("perm_min") is not None and r["verdict"] != "reachable"]
        assert not bad, "StartPermission=1 后仍有非 reachable：%r" % bad[:3]
        base = _recompute(rows, 0)
        assert any(r["verdict"] == "unreachable" for r in base), "StartPermission=0 下没有 unreachable 样本"

    tooth("① perm_min==0 改判 unreachable ⇒ gate 红", tooth1)
    tooth("② handler 行号越界 ⇒ gate 红", tooth2)
    tooth("③ StartPermission=1 ⇒ 全翻 reachable", tooth3)
    if fails:
        print("gm_registry selftest: FAIL")
        for f in fails:
            print("  -", f)
        return 1
    print("gm_registry selftest: OK (3 teeth)")
    return 0


def main(argv=None):
    ap = argparse.ArgumentParser(description="GM 命令可达性登记 + 门禁")
    ap.add_argument("cmd", choices=("build", "gate", "summary", "selftest"))
    ap.add_argument("--conf", default=DEFAULT_CONF, help="部署 Command.conf 路径（缺失则记 absent）")
    args = ap.parse_args(argv)
    return {"build": cmd_build, "gate": cmd_gate,
            "summary": cmd_summary, "selftest": cmd_selftest}[args.cmd](args)


if __name__ == "__main__":
    sys.exit(main())
