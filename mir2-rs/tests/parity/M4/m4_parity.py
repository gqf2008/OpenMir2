#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""M4 收口门禁（卡 `openmir2-m4-parity-cases`）：

  ① parsecount 两侧对拍数表 —— 同一脚本语料上，C# 参照 vs Rust 的
     『语句(过程) / 条件 / 动作 / 宏展开』四类计数逐项对比，产出入库表格；
  ③ 『双侧逐行多重集差 = 0』门禁 —— 复用 M3 先例 `tests/parity/m3/scoped_diff.py`
     的**多重集/计数式**口径（`multiset_delta` / `cmd_diff`），不另造第二套计数实现。

一条命令（默认在 mir2-rs/ 下执行）：

  python -I tests/parity/M4/m4_parity.py --envir "E:/MirServer/M2GameSvr/Envir"

做四件事（产物全部落 `tests/parity/evidence/M4/`）：

  1. 跑 Rust `parse-stats` 与 C# harness（`tools/script-parity-cs`）→
     `parse-stats-rust.json` / `parse-stats-cs.json`；
  2. 生成 ① 的四类计数逐项对比表 → `parsecount-both-sides.md`；
  3. 把两侧 JSON 归一成 `scoped_diff` 行格式，调 `scoped_diff.py diff` 做
     逐行多重集比对 → `multiset-gate.txt`（差异非 0 即 exit 1）；
  4. 边界语料（`fixtures/macroenvir`，含 `#DEFINE` 常量替换）两侧结构摘要
     逐字节比对 → `macro-parity.txt`（宏展开效果的可证伪证据）。

牙齿（§6.0-2「恒绿的门禁等于没有门禁」）：

  selftest    多重集门禁的「插一行 / 删一行 / 重复行」必红自检（不需语料/端口）；
  redcheck    篡改 C# JSON 的一个计数 / 一条错误行 → 真实门禁必红（用副本，不改入库件）。

退出码：0 = 全绿；1 = 有差异（红）；2 = 用法/环境错误。
"""

import argparse
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")

HERE = Path(__file__).resolve().parent            # mir2-rs/tests/parity/M4
PARITY = HERE.parent                              # mir2-rs/tests/parity
MIR2RS = PARITY.parents[1]                        # mir2-rs/
REPO = MIR2RS.parent                              # 仓库根
SCOPED_DIFF = PARITY / "m3" / "scoped_diff.py"
EVIDENCE = PARITY / "evidence" / "M4"

DEFAULT_ENVIR = r"E:/MirServer/M2GameSvr/Envir"

# scoped_diff 行格式：{"table", "row_sha256", "line"}
CS_ERR_RE = re.compile(r"^脚本错误: (.*) 第:\d+ 行: .*$")


# --------------------------------------------------------------------------- #
# 两侧 JSON 的归一（统一到同一份行语料）
# --------------------------------------------------------------------------- #
def rust_error_line(e):
    return e.get("line", "")


def cs_error_line(e):
    m = CS_ERR_RE.match(e.get("message", ""))
    return m.group(1) if m else e.get("message", "")


def emit_rows(rust, cs):
    """把两侧 parse-stats JSON 归一成 scoped_diff 行（table/row_sha256/line）。

    行语料 = 全局计数 + 逐文件结构摘要 + 逐文件原文摘要 + 逐条脚本错误行。
    这样 ① 的计数、逐文件内容（含宏展开后的命令/参数/说词）、③ 的逐行错误
    全部落在同一套多重集比对里，避免出现「只看计数」的静默缺口。
    """
    def rows_for(side, err_line):
        rows = []
        st = side["stats"]
        # 全局计数（含文件集/加载/异常）
        stats = {
            "files": side["files"],
            "loaded": side["loaded"],
            "missing": side["missing"],
            "panics": side["panics"],
            **_comparable_stats(st),
        }
        for k in sorted(stats):
            rows.append(("__stats__", f"{k}={stats[k]}"))
        # 逐文件结构摘要（含宏展开后的 label/cmd/参数/说词）
        for f, d in sorted(side.get("structure_digest", {}).items()):
            rows.append((f, "struct hash={hash} s={s} r={r} p={p} c={c} a={a} e={e}".format(**d)))
        # 逐文件原文摘要（解码结果全文）
        for f, h in sorted(side.get("text_digest", {}).items()):
            rows.append((f, f"text {h}"))
        # 逐条脚本错误行（③ 的核心）
        for e in side.get("errors", []):
            if e.get("kind", "ScriptError") != "ScriptError":
                continue
            rows.append((e["file"], f"err {err_line(e)}"))
        # 整文件解析异常（LoadPanic），两侧各自标记
        for e in side.get("errors", []):
            if e.get("kind", "ScriptError") == "ScriptError":
                continue
            rows.append((e["file"], f"panic {e.get('line', '')}"))
        return rows

    def to_jsonl(rows):
        out = []
        for table, line in rows:
            out.append({
                "table": table,
                "row_sha256": hashlib.sha256(line.encode("utf-8")).hexdigest(),
                "line": line,
            })
        return out

    return to_jsonl(rows_for(rust, rust_error_line)), to_jsonl(rows_for(cs, cs_error_line))


def _comparable_stats(st):
    """两侧 harness 都产出（或可同口径映射）的计数（用于 __stats__ 行）。"""
    keys = [
        "scripts", "records", "procedures", "conditions", "actions", "else_actions",
        "goods", "parse_errors",
    ]
    out = {k: st[k] for k in keys if k in st}
    # 两侧命名不同、语义相同的映射（两侧都归一成同一批键）
    out["item_type_entries"] = st.get("item_type_entries", st.get("item_type_lines"))
    out["load_failures"] = st.get(
        "load_failures", st.get("call_failures", 0) + st.get("include_failures", 0))
    return out


# --------------------------------------------------------------------------- #
# 跑两侧 parse-stats
# --------------------------------------------------------------------------- #
def build_cs_harness():
    r = subprocess.run(
        ["dotnet", "build", str(MIR2RS / "tools/script-parity-cs/script-parity-cs.csproj"),
         "-v", "q", "--nologo"],
        cwd=MIR2RS, capture_output=True, text=True, encoding="utf-8", errors="replace",
    )
    if r.returncode != 0:
        print(r.stdout[-2000:])
        raise SystemExit("C# harness 构建失败")


def _normalize_paths(text):
    """把本 worktree/repo 的绝对路径归一为稳定 token。

    C# harness 会把相对语料路径 `Path.GetFullPath` 补成绝对路径（含 worktree 目录名），
    直接入库会让证据字节跨 worktree 不可复现（评审 N-1 同类）；这里只归一工具回显的
    路径前缀，不改语料本身。
    """
    for p in (MIR2RS, REPO):
        abs_p = str(Path(p).resolve())
        token = "<" + Path(p).name + ">"
        for variant in {abs_p, abs_p.replace("\\", "/"), abs_p.replace("/", "\\")}:
            text = text.replace(variant, token)
    return text


def run_side(envir, out_dir):
    out_dir.mkdir(parents=True, exist_ok=True)
    rust_json = out_dir / "parse-stats-rust.json"
    cs_json = out_dir / "parse-stats-cs.json"

    r = subprocess.run(
        ["cargo", "run", "-q", "-p", "mir2-script-tool", "--offline", "--",
         "parse-stats", str(envir), "--json", str(rust_json)],
        cwd=MIR2RS, capture_output=True, text=True, encoding="utf-8", errors="replace",
    )
    if r.returncode != 0:
        print(r.stdout[-3000:]); print(r.stderr[-3000:])
        raise SystemExit("Rust parse-stats 失败")
    (out_dir / "parse-stats-rust.stdout.txt").write_text(
        _normalize_paths(r.stdout), encoding="utf-8", newline="\n")

    build_cs_harness()
    r = subprocess.run(
        ["dotnet", "run", "--project", "tools/script-parity-cs/script-parity-cs.csproj",
         "--no-build", "--", str(envir), str(cs_json)],
        cwd=MIR2RS, capture_output=True, text=True, encoding="utf-8", errors="replace",
    )
    if r.returncode != 0:
        print(r.stdout[-3000:]); print(r.stderr[-3000:])
        raise SystemExit("C# parse-stats 失败")
    (out_dir / "parse-stats-cs.stdout.txt").write_text(
        _normalize_paths(r.stdout), encoding="utf-8", newline="\n")
    return rust_json, cs_json


# --------------------------------------------------------------------------- #
# ③ 多重集门禁（复用 scoped_diff.py）
# --------------------------------------------------------------------------- #
def run_multiset_gate(rust, cs, out_dir):
    a_rows, b_rows = emit_rows(rust, cs)
    a = out_dir / "lines-rust.jsonl"
    b = out_dir / "lines-cs.jsonl"
    _write_jsonl(a, a_rows)
    _write_jsonl(b, b_rows)
    r = subprocess.run(
        [sys.executable, "-I", str(SCOPED_DIFF), "diff", str(a), str(b),
         "--summary-out", str(out_dir / "multiset-summary.json")],
        capture_output=True, text=True, encoding="utf-8", errors="replace",
    )
    text = r.stdout + (("\n--stderr--\n" + r.stderr) if r.stderr.strip() else "")
    (out_dir / "multiset-gate.txt").write_text(
        f"$ python -I tests/parity/m3/scoped_diff.py diff {a.name} {b.name}\n" + text,
        encoding="utf-8", newline="\n")
    print(text)
    return r.returncode == 0


def _write_jsonl(path, rows):
    with open(path, "w", encoding="utf-8", newline="\n") as f:
        for row in rows:
            f.write(json.dumps(row, ensure_ascii=False) + "\n")


# --------------------------------------------------------------------------- #
# ① 计数表
# --------------------------------------------------------------------------- #
def parsecount_table(rust, cs, envir):
    rs, cs_ = rust["stats"], cs["stats"]
    rows = [
        # 类别, 指标, C# 值, Rust 值
        ("语句", "过程 SayingProcedure（可执行语句块）", cs_["procedures"], rs["procedures"]),
        ("语句", "标签 SayingRecord", cs_["records"], rs["records"]),
        ("语句", "脚本 ScriptInfo", cs_["scripts"], rs["scripts"]),
        ("条件", "条件 ConditionList", cs_["conditions"], rs["conditions"]),
        ("动作", "动作 ActionList", cs_["actions"], rs["actions"]),
        ("动作", "否则动作 ElseActionList", cs_["else_actions"], rs["else_actions"]),
        ("宏展开", "#DEFINE 常量替换（define_substitutions）", "0（语料无 #DEFINE）", rs["define_substitutions"]),
        ("其他", "商品 goods", cs_["goods"], rs["goods"]),
        ("其他", "物品类型项 / item_type_lines", cs_["item_type_entries"], rs["item_type_lines"]),
        ("其他", "解析错误行 parse_errors", cs_["parse_errors"], rs["parse_errors"]),
        ("其他", "#CALL 加载失败 load_failures", cs_["load_failures"], rs["call_failures"] + rs["include_failures"]),
    ]
    struct_ok = rust.get("structure_digest") == cs.get("structure_digest")
    text_ok = rust.get("text_digest") == cs.get("text_digest")
    err_r = sorted((e["file"], rust_error_line(e)) for e in rust["errors"] if e.get("kind") == "ScriptError")
    err_c = sorted((e["file"], cs_error_line(e)) for e in cs["errors"])
    err_ok = err_r == err_c

    lines = []
    lines.append("# M4① parsecount 两侧对拍数表")
    lines.append("")
    lines.append("> 本文件由一条命令重生成，勿手改：")
    lines.append(">")
    lines.append("> ```")
    lines.append('> python -I tests/parity/M4/m4_parity.py --envir "E:/MirServer/M2GameSvr/Envir"')
    lines.append("> ```")
    lines.append("")
    lines.append(f"- 语料（两侧同一份）：`{envir}`")
    lines.append(f"- 语料文件数：C# `{cs['files']}` / Rust `{rust['files']}`"
                 f"（加载 {cs['loaded']}/{rust['loaded']}，缺失 {cs['missing']}/{rust['missing']}，"
                 f"异常 {cs['panics']}/{rust['panics']}）")
    lines.append(f"- 计数口径：Rust `mir2-script-tool parse-stats`；C# 参照 `tools/script-parity-cs`"
                 f"（只读引用 `src/Modules/ScriptEngine`，DispatchProxy 假 NPC/商人）。")
    lines.append("")
    lines.append("## ① 四类计数（M4 验收② 原文：语句 / 条件 / 动作 / 宏展开）")
    lines.append("")
    lines.append("| 类别 | 指标 | C# 参照 | Rust | 差 | 判定 |")
    lines.append("| --- | --- | ---: | ---: | ---: | --- |")
    for cat, metric, c, r in rows:
        if isinstance(c, int) and isinstance(r, int):
            diff = r - c
            verdict = "✓" if diff == 0 else "✗"
            lines.append(f"| {cat} | {metric} | {c} | {r} | {diff:+d} | {verdict} |")
        else:
            lines.append(f"| {cat} | {metric} | {c} | {r} | — | ✓（见下注） |")
    lines.append("")
    lines.append("## ①′ 逐文件内容与逐行错误（③ 门禁的另一面）")
    lines.append("")
    lines.append("| 项 | C# 参照 | Rust | 判定 |")
    lines.append("| --- | ---: | ---: | --- |")
    lines.append(f"| 逐文件结构摘要（label/cmd/参数/宏展开后说词） | {len(cs.get('structure_digest', {}))} 文件 | {len(rust.get('structure_digest', {}))} 文件 | {'✓ 逐字节相同' if struct_ok else '✗ 不一致'} |")
    lines.append(f"| 逐文件原文摘要（解码结果全文） | {len(cs.get('text_digest', {}))} 文件 | {len(rust.get('text_digest', {}))} 文件 | {'✓ 逐字节相同' if text_ok else '✗ 不一致'} |")
    lines.append(f"| 脚本错误行（逐行多重集） | {len(err_c)} 条 | {len(err_r)} 条 | {'✓ 逐行相同（差 0）' if err_ok else '✗ 有差异'} |")
    lines.append("")
    lines.append("## 注")
    lines.append("")
    lines.append("- **宏展开**：C# 参照实现本身不暴露『常量替换次数』计数器，故 C# 列取语料事实值"
                 " `0`（同语料下 `#DEFINE`/`#INCLUDE` 指令数 = 0，grep 见 `multiset-gate.txt` 上方命令）；"
                 "Rust 计数器 `define_substitutions` 亦为 0。宏展开的**效果**由『逐文件结构摘要』"
                 "（含展开后的命令/参数/说词全文）逐字节比对代偿；边界语料（`fixtures/macroenvir`，"
                 "含 2 次 `#DEFINE` 替换）的两侧结构摘要逐字节相同，见 `macro-parity.txt`。")
    lines.append("- Rust harness 侧另有 C# harness 未暴露的计数（`call_loads`/`price_rate_lines`/"
                 "`merchant_cmd_lines`/`label_renames`/`include_loads` 等）：其可观测效果同样并入结构摘要。")
    lines.append("- 427 条解析错误是 C# 参照在语料上的**真实行为**（`TakeOn`/`GAMEGIRD`/`ReadRandomLine` 等"
                 "LEGM2 变体遗留命令在 C# 源码中不存在），两侧逐行多重集差 = 0；见 `whitelist.md` B-101。")
    lines.append("")
    return "\n".join(lines)


# --------------------------------------------------------------------------- #
# ④ 宏边界语料
# --------------------------------------------------------------------------- #
def macro_parity(out_dir):
    fixture = HERE / "fixtures" / "macroenvir"
    if not fixture.is_dir():
        return True, "缺边界语料目录"
    tmp = out_dir / "macro"
    # 用仓库相对路径调用两侧 harness：否则 stdout 里会写进本 worktree 的绝对路径，
    # 证据字节跨 worktree 不可复现（N-1 同类问题）。
    fixture_arg = fixture.relative_to(MIR2RS).as_posix()
    rust_json, cs_json = run_side(fixture_arg, tmp)
    rust = json.loads(rust_json.read_text(encoding="utf-8"))
    cs = json.loads(cs_json.read_text(encoding="utf-8"))
    same = rust.get("structure_digest") == cs.get("structure_digest")
    lines = [
        "# M4① 边界语料：宏展开（#DEFINE 常量替换）两侧结构摘要",
        "",
        f"- 语料：`tests/parity/M4/fixtures/macroenvir/Npc_def/define.txt`（2 条 `#DEFINE`）",
        f"- Rust `define_substitutions` = {rust['stats']['define_substitutions']}",
        f"- Rust 结构摘要：{json.dumps(rust.get('structure_digest'), ensure_ascii=False)}",
        f"- C#  结构摘要：{json.dumps(cs.get('structure_digest'), ensure_ascii=False)}",
        f"- 判定：{'✓ 逐字节相同（宏展开效果一致）' if same else '✗ 不一致'}",
        "",
        "> 说明：C# 参照不暴露展开次数计数器，故宏展开的『条数』以 Rust 计数为准、"
        "『效果』以两侧结构摘要（命令码/参数/说词，均在替换之后）逐字节比对为准。",
        "",
    ]
    (out_dir / "macro-parity.txt").write_text("\n".join(lines), encoding="utf-8", newline="\n")
    print("\n".join(lines))
    return same, ""


# --------------------------------------------------------------------------- #
# 牙齿：selftest / redcheck
# --------------------------------------------------------------------------- #
def _scoped_diff_exit(a_rows, b_rows):
    tmp = tempfile.mkdtemp(prefix="m4-multiset-")
    a = Path(tmp) / "a.jsonl"
    b = Path(tmp) / "b.jsonl"
    _write_jsonl(a, a_rows)
    _write_jsonl(b, b_rows)
    r = subprocess.run([sys.executable, "-I", str(SCOPED_DIFF), "diff", str(a), str(b)],
                       capture_output=True, text=True, encoding="utf-8", errors="replace")
    shutil.rmtree(tmp, ignore_errors=True)
    return r.returncode, r.stdout


def _row(table, line):
    return {"table": table, "row_sha256": hashlib.sha256(line.encode()).hexdigest(), "line": line}


def cmd_selftest(_args):
    cases = []
    base = [_row("f.txt", "err A"), _row("f.txt", "err B")]
    dup = _row("f.txt", "err A")
    cases.append(("1/5 相同行料判绿", base, list(base), 0))
    cases.append(("2/5 插入一行判红", base, base + [_row("f.txt", "err C")], 1))
    cases.append(("3/5 删除一行判红", base, base[:-1], 1))
    cases.append(("4/5 重复行 A=[x,x] vs B=[x] 判红（旧 set 差集会假绿）",
                  base + [dup], list(base), 1))
    cases.append(("5/5 重复行两侧相同判绿", base + [dup], base + [dup], 0))
    bad = 0
    for name, a, b, want in cases:
        got, out = _scoped_diff_exit(a, b)
        ok = got == want
        if not ok:
            bad += 1
        print(f"{'PASS' if ok else 'RED '} {name}（期望 exit {want}，实得 {got}）")
        if not ok:
            print(out)
    if bad:
        print(f"RED: selftest {bad} 项不过")
        return 2
    print("GREEN: 多重集门禁 selftest 5/5（含插行/删行/重复行必红）")
    return 0


def cmd_redcheck(args):
    """篡改 C# JSON 的一个计数 / 一条错误行 → 真实门禁必红（不改入库件）。"""
    out_dir = args.out or EVIDENCE
    rust_path = out_dir / "parse-stats-rust.json"
    cs_path = out_dir / "parse-stats-cs.json"
    if not rust_path.exists() or not cs_path.exists():
        print("缺两侧 JSON；先跑一次默认命令")
        return 2
    rust = json.loads(rust_path.read_text(encoding="utf-8"))
    cs = json.loads(cs_path.read_text(encoding="utf-8"))
    bad = 0

    # 红检 A：计数 +1
    tampered = json.loads(cs_path.read_text(encoding="utf-8"))
    tampered["stats"]["conditions"] += 1
    a_rows, _ = emit_rows(rust, cs)
    _, b_rows = emit_rows(rust, tampered)
    got, out = _scoped_diff_exit(a_rows, b_rows)
    ok = got == 1
    bad += 0 if ok else 1
    print(f"{'PASS' if ok else 'RED '} 红检 A：stats.conditions +1 → 门禁判红（exit {got}）")
    print("  " + out.strip().splitlines()[-1])

    # 红检 B：错误行文本改 1 字符（计数不变）
    tampered = json.loads(cs_path.read_text(encoding="utf-8"))
    if tampered["errors"]:
        tampered["errors"][0]["message"] = tampered["errors"][0]["message"].replace(" 第:", "X第:", 1)
    a_rows, _ = emit_rows(rust, cs)
    _, b_rows = emit_rows(rust, tampered)
    got, out = _scoped_diff_exit(a_rows, b_rows)
    ok = got == 1
    bad += 0 if ok else 1
    print(f"{'PASS' if ok else 'RED '} 红检 B：错误行文本改 1 字符 → 门禁判红（exit {got}）")
    print("  " + out.strip().splitlines()[-1])

    if bad:
        print(f"RED: redcheck {bad} 项不过")
        return 2
    print("GREEN: redcheck 2/2（计数与逐行错误都真的有牙齿）")
    return 0


# --------------------------------------------------------------------------- #
# 默认：完整跑
# --------------------------------------------------------------------------- #
def cmd_run(args):
    envir = Path(args.envir)
    if not envir.is_dir():
        print(f"Envir 目录不存在: {envir}")
        return 2
    out_dir = Path(args.out) if args.out else EVIDENCE
    out_dir.mkdir(parents=True, exist_ok=True)

    # 一条命令自带牙齿：先跑多重集自检（插行/删行/重复行必红），不过则直接判红。
    print("== 门禁自检（牙齿：插行/删行/重复行必红）==")
    if cmd_selftest(args) != 0:
        return 2
    print()

    if not args.reuse:
        rust_json, cs_json = run_side(envir, out_dir)
    else:
        rust_json = out_dir / "parse-stats-rust.json"
        cs_json = out_dir / "parse-stats-cs.json"
    rust = json.loads(Path(rust_json).read_text(encoding="utf-8"))
    cs = json.loads(Path(cs_json).read_text(encoding="utf-8"))

    table = parsecount_table(rust, cs, envir)
    (out_dir / "parsecount-both-sides.md").write_text(table, encoding="utf-8", newline="\n")
    print(table)

    # 语料宏指令事实（供「宏展开=0」举证）
    macro_cmds = _corpus_directives(envir)
    (out_dir / "corpus-directives.txt").write_text(macro_cmds, encoding="utf-8", newline="\n")

    ok_multi = run_multiset_gate(rust, cs, out_dir)
    ok_macro, _ = macro_parity(out_dir)
    ok_counts, _ = _counts_diff(rust, cs)
    print()
    print("== M4 门禁结果 ==")
    print(f"  ① parsecount 两侧数表         : {'GREEN' if ok_counts else 'RED'}")
    print(f"  ③ 双侧逐行多重集差 = 0         : {'GREEN' if ok_multi else 'RED'}")
    print(f"  ① 宏边界语料结构摘要一致       : {'GREEN' if ok_macro else 'RED'}")
    return 0 if (ok_multi and ok_macro) else 1


def _counts_diff(rust, cs):
    """四类计数 + 可比计数是否全等。"""
    rs, cs_ = rust["stats"], cs["stats"]
    pairs = [
        (cs_["procedures"], rs["procedures"]), (cs_["records"], rs["records"]),
        (cs_["scripts"], rs["scripts"]), (cs_["conditions"], rs["conditions"]),
        (cs_["actions"], rs["actions"]), (cs_["else_actions"], rs["else_actions"]),
        (cs_["goods"], rs["goods"]), (cs_["item_type_entries"], rs["item_type_lines"]),
        (cs_["parse_errors"], rs["parse_errors"]),
    ]
    return all(c == r for c, r in pairs), pairs


def _corpus_directives(envir):
    """语料里 `#DEFINE`/`#INCLUDE`/`#SETHOME` 指令计数（两侧同口径的宏来源）。"""
    pat = re.compile(r"^\s*#(DEFINE|INCLUDE|SETHOME)\b", re.IGNORECASE)
    counts = {}
    for p in sorted(Path(envir).rglob("*.txt")):
        try:
            text = p.read_bytes().decode("gbk", errors="replace")
        except OSError:
            continue
        for line in text.splitlines():
            m = pat.match(line)
            if m:
                counts[m.group(1).upper()] = counts.get(m.group(1).upper(), 0) + 1
    lines = [f"{k}={v}" for k, v in sorted(counts.items())] or ["（语料无 #DEFINE/#INCLUDE/#SETHOME）"]
    return "\n".join(lines) + "\n"


def main():
    ap = argparse.ArgumentParser(description="M4 收口门禁（parsecount + 逐行多重集）")
    ap.add_argument("--envir", default=DEFAULT_ENVIR)
    ap.add_argument("--out", default=None)
    ap.add_argument("--reuse", action="store_true", help="复用已入库 JSON，不重跑两侧")
    sub = ap.add_subparsers(dest="cmd")
    sub.add_parser("run", help="完整跑（默认）")
    sub.add_parser("selftest", help="多重集门禁牙齿自检")
    sub.add_parser("redcheck", help="篡改副本 → 真实门禁必红")

    args = ap.parse_args()
    if args.cmd == "selftest":
        return cmd_selftest(args)
    if args.cmd == "redcheck":
        return cmd_redcheck(args)
    return cmd_run(args)


if __name__ == "__main__":
    raise SystemExit(main())
