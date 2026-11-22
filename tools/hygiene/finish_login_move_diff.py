#!/usr/bin/env python3
"""finish_login_move_diff.py — L2-5 ③ 段 1a「纯搬移」的机械对账（可复跑）。

背景：段 1a 把 `DBR_LOADHUMANRCD(1100)` `recog==1` 臂的登录完成块（~1307 行）搬进自由函数
`finish_login(&mut LoginCtx)`。人眼"逐句同文"不是证据；本工具做机械对账：

  base 侧：`git show <base>:<path>` 里该臂的**块体**（`{` 之后到配对 `}` 之前）；
  head 侧：`<path>`（工作树或 `git show <head>:<path>`）里 `fn finish_login(...)` 的**函数体**。

两边先做**已知适配归一**（段 1a 提交信息里列过的清单），再比较：
  1. head 侧去掉解构 + 影子前导（`let LoginCtx { ... } = ctx;` 到
     `let shard_runtime = &mut **shard_runtime;`）与末尾写回 `*ctx.item_number = item_number;`；
  2. reborrow：`&mut *<name>` → `&mut <name>`（name ∈ 该清单的可变字段）；
  3. 空白：两侧各去掉**全部空白字符**后逐字符比较（rustfmt 换行/缩进差异不算语义差异）；
  3c. rustfmt 的**逗号增删**：尾逗号（`f(a,)`≡`f(a)`、`[a,]`/`{a,}`）与 **match 臂分隔逗号**
      （块臂 `=> { e }` 折叠成表达式臂时 rustfmt 会补 `,`；两侧都按 `,<模式>=>` 归一）；
  3b. rustfmt 的**单表达式花括号折叠**：`|x| { expr }` ≡ `|x| expr`、`=> { expr }` ≡ `=> expr`
      （body 不含 `;`/嵌套花括号时归一；rustfmt 在新缩进位置会做这个折叠，语义等价）；
  4. 字符串字面量：另比较**全部双引号字面量的多重集**（原文逐字节，不去空白）——
     防止「删空白」把字符串内部的空格变化掩掉。

判据：① 去空白正文完全相等；② 字符串多重集相等。两者都成立 ⇒ GREEN（"差异 0 字符"），
否则 RED 并给出首个差异的上下文（两侧各 ±120 字符）。

退出码：0 = 绿；1 = 有差异（红）；2 = 判不了（用法错误 / 提取失败 / git 失败）。

用法（仓库根）：
  python -I -B tools/hygiene/finish_login_move_diff.py --base c2e5226de
  python -I -B tools/hygiene/finish_login_move_diff.py --base c2e5226de --head 9e1e99846
  python -I -B tools/hygiene/finish_login_move_diff.py --selftest
"""
import argparse
import re
import subprocess
import sys

ARM_IF = "if ident == mir2_protocol::messages::DBR_LOADHUMANRCD && recog == 1 {"
FN_SIG = "fn finish_login(ctx: &mut LoginCtx<'_>) {"
PROLOGUE_END = "let shard_runtime = &mut **shard_runtime;"
WRITE_BACK = "*ctx.item_number = item_number;"
MUTABLE_NAMES = [
    "world",
    "displays",
    "online",
    "ground_sessions",
    "saves",
    "engine",
    "gates",
    "login_tails",
    "aoi_frames_total",
    "reconnect_pending",
    "kick_pending",
    "pwd_sessions",
    "npc_host",
    "guilds",
    "npc_vars_base",
    "shard_runtime",
]


def _repo_root():
    import os

    here = os.path.dirname(os.path.abspath(__file__))
    return os.path.dirname(os.path.dirname(here))


def _skip_literal_or_comment(text, i):
    """若 text[i] 起是字符串/字符/行注释/块注释，返回其结束下标（不含），否则 None。"""
    n = len(text)
    ch = text[i]
    if ch == '"':
        j = i + 1
        while j < n:
            if text[j] == "\\":
                j += 2
                continue
            if text[j] == '"':
                return j + 1
            j += 1
        return n
    if ch == "/" and i + 1 < n and text[i + 1] == "/":
        j = text.find("\n", i)
        return n if j < 0 else j
    if ch == "/" and i + 1 < n and text[i + 1] == "*":
        j = text.find("*/", i + 2)
        return n if j < 0 else j + 2
    return None


def brace_body(text, anchor):
    """返回 anchor 行末 `{` 与配对 `}` 之间的文本；找不到/不配对 ⇒ None。"""
    i = text.find(anchor)
    if i < 0:
        return None
    i = text.find("{", i + len(anchor) - 1)
    if i < 0:
        return None
    depth = 0
    j = i
    n = len(text)
    while j < n:
        skipped = _skip_literal_or_comment(text, j)
        if skipped is not None:
            j = skipped
            continue
        if text[j] == "{":
            depth += 1
        elif text[j] == "}":
            depth -= 1
            if depth == 0:
                return text[i + 1 : j]
        j += 1
    return None


def strip_adaptations(body):
    """按段 1a 适配清单归一 head 侧函数体。返回 (normalized_body, notes)。"""
    lines = body.split("\n")
    start = 0
    end = len(lines)
    # 前导：解构 + 影子（到 PROLOGUE_END 行为止）
    for k, line in enumerate(lines):
        if line.strip() == PROLOGUE_END:
            start = k + 1
            break
    else:
        return None, ["head 侧找不到适配前导结束行（PROLOGUE_END）"]
    # 末尾：写回行
    for k in range(len(lines) - 1, start - 1, -1):
        if lines[k].strip() == WRITE_BACK:
            end = k
            break
    text = "\n".join(lines[start:end])
    notes = []
    for name in MUTABLE_NAMES:
        pattern = re.compile(r"&mut\s+\*" + re.escape(name) + r"\b(?!\s*\.)")
        text, hits = pattern.subn("&mut " + name, text)
        if hits:
            notes.append("reborrow &mut *%s ×%d" % (name, hits))
    return text, notes


def squeeze(text):
    return "".join(ch for ch in text if not ch.isspace())


_CLOSURE_BRACES = re.compile(r"\|([A-Za-z_][A-Za-z0-9_]*[^|{}]*)\|\{([^{};]*)\}")


_MATCH_ARM_BRACES = re.compile(r"=>\{([^{};]*)\}")
_MATCH_ARM_COMMA = re.compile(r",([A-Za-z_][A-Za-z0-9_:]*(?:\([^()]*\))?)=>")


def collapse_single_expr_closures(squeezed):
    """把 `|x|{expr}` / `=>{expr}` 折叠成无花括号（仅单表达式，body 不含 `;`/花括号）。"""
    out = _CLOSURE_BRACES.sub(lambda m: "|%s|%s" % (m.group(1), m.group(2)), squeezed)
    out = _MATCH_ARM_BRACES.sub(lambda m: "=>" + m.group(1), out)
    # 块臂折叠成表达式臂后 rustfmt 补的分隔逗号：两侧都去掉（`...,<模式>=>` → `...<模式>=>`）。
    return _MATCH_ARM_COMMA.sub(lambda m: m.group(1) + "=>", out)


def strip_trailing_commas(squeezed):
    """去掉紧邻 `)` `]` `}` `>` 之前的尾逗号（Rust 里语义中立；rustfmt 视换行增删）。"""
    prev = None
    out = squeezed
    while prev != out:
        prev = out
        for closer in (")", "]", "}", ">"):
            out = out.replace("," + closer, closer)
    return out


def string_literals(text):
    out = []
    i = 0
    n = len(text)
    while i < n:
        if text[i] == '"':
            j = i + 1
            while j < n:
                if text[j] == "\\":
                    j += 2
                    continue
                if text[j] == '"':
                    out.append(text[i : j + 1])
                    j += 1
                    break
                j += 1
            i = j
        else:
            i += 1
    return out


def compare(base_body, head_body):
    """返回 (ok, report_lines)。"""
    report = []
    if head_body is None:
        return False, ["提取失败：head 侧找不到 finish_login 函数体或花括号不配对"]
    norm, notes = strip_adaptations(head_body)
    if norm is None:
        return False, ["提取失败：" + "; ".join(notes)]
    sb = strip_trailing_commas(collapse_single_expr_closures(squeeze(base_body)))
    sh = strip_trailing_commas(collapse_single_expr_closures(squeeze(norm)))
    report.append(
        "base 块体: %d 行 / 去空白 %d 字符；head 函数体（归一后）: %d 行 / 去空白 %d 字符"
        % (
            len(base_body.split("\n")),
            len(sb),
            len(norm.split("\n")),
            len(sh),
        )
    )
    for note in notes:
        report.append("适配归一: " + note)
    ok = True
    if sb != sh:
        ok = False
        pos = next((k for k in range(min(len(sb), len(sh))) if sb[k] != sh[k]), min(len(sb), len(sh)))
        report.append("RED：去空白正文不一致（首个差异在第 %d 字符）" % pos)
        report.append("  base: …%s…" % sb[max(0, pos - 120) : pos + 120])
        report.append("  head: …%s…" % sh[max(0, pos - 120) : pos + 120])
    else:
        report.append("去空白正文：完全相等（差异 0 字符）")
    lb = sorted(string_literals(base_body))
    lh = sorted(string_literals(norm))
    if lb != lh:
        ok = False
        only_b = [x for x in lb if x not in lh][:10]
        only_h = [x for x in lh if x not in lb][:10]
        report.append("RED：字符串字面量多重集不一致")
        report.append("  仅 base: %r" % only_b)
        report.append("  仅 head: %r" % only_h)
    else:
        report.append("字符串字面量多重集：相等（%d 条）" % len(lb))
    return ok, report


def git_show(repo, rev, path):
    try:
        out = subprocess.run(
            ["git", "-C", repo, "show", "%s:%s" % (rev, path)],
            capture_output=True,
            check=False,
        )
    except OSError as e:  # 判不了
        raise SystemExit("判不了：无法执行 git（%s）" % e)
    if out.returncode != 0:
        raise SystemExit(
            "判不了：git show %s:%s 失败（rc=%d, %s）"
            % (rev, path, out.returncode, out.stderr.decode("utf-8", "replace").strip()[:300])
        )
    return out.stdout.decode("utf-8", "replace")


def run_check(repo, path, base_rev, head_rev):
    base_src = git_show(repo, base_rev, path)
    if head_rev in (None, "", "WORKTREE"):
        with open("%s/%s" % (repo, path), "r", encoding="utf-8", newline="") as f:
            head_src = f.read()
    else:
        head_src = git_show(repo, head_rev, path)
    base_body = brace_body(base_src, ARM_IF)
    if base_body is None:
        raise SystemExit("判不了：base 侧找不到 DBR_LOADHUMANRCD recog==1 臂（或花括号不配对）")
    head_body = brace_body(head_src, FN_SIG)
    if head_body is None:
        raise SystemExit("判不了：head 侧找不到 finish_login 函数体（或花括号不配对）")
    ok, report = compare(base_body, head_body)
    for line in report:
        print(line)
    print("RESULT: %s" % ("GREEN" if ok else "RED"))
    return 0 if ok else 1


def selftest():
    NL = chr(10)
    checks = []

    def check(name, cond, detail=""):
        checks.append((name, bool(cond), detail))

    arm = ARM_IF + "\n  let a = 1;\n  foo(&mut world, \"x\");\n}\n"
    base_body = brace_body(arm, ARM_IF)
    fn = (
        FN_SIG
        + "\n    let LoginCtx { world, .. } = ctx;\n    let world = &mut **world;\n"
        + PROLOGUE_END
        + "\n    let a = 1;\n    foo(&mut *world, \"x\");\n    "
        + WRITE_BACK
        + "\n}\n"
    )
    head_body = brace_body(fn, FN_SIG)
    ok, rep = compare(base_body, head_body)
    check("搬移 + 适配（reborrow/前导/写回）⇒ GREEN", ok, rep)
    check("报告含「字符串字面量多重集：相等」", any("字符串字面量多重集：相等" in x for x in rep), rep)

    # 反例 1：真实 token 变化 ⇒ RED
    bad = fn.replace("let a = 1;", "let a = 2;")
    ok2, rep2 = compare(base_body, brace_body(bad, FN_SIG))
    check("token 变化 ⇒ RED", not ok2 and any("去空白正文不一致" in x for x in rep2), rep2)

    # 反例 2：只在字符串内部多一个空格 ⇒ 去空白会掩盖，字符串多重集必须抓住
    bad2 = fn.replace('"x"', '"x y"')
    ok3, rep3 = compare(base_body, brace_body(bad2, FN_SIG))
    check("字符串内空格变化 ⇒ RED", not ok3 and any("字符串字面量多重集不一致" in x for x in rep3), rep3)

    # 闭包折叠：`|x| { y }` 与 `|x| y` 等价（rustfmt 会折叠）
    b4 = ARM_IF + NL + "  let h = g(|r| { r.k() });" + NL + "}" + NL
    f4 = (
        FN_SIG
        + NL
        + "    "
        + PROLOGUE_END
        + NL
        + "  let h = g(|r| r.k());"
        + NL
        + "    "
        + WRITE_BACK
        + NL
        + "}"
        + NL
    )
    ok4, rep4 = compare(brace_body(b4, ARM_IF), brace_body(f4, FN_SIG))
    check("单表达式闭包花括号折叠 ⇒ GREEN", ok4, rep4)

    # match arm 单表达式花括号折叠：`=> { e }` ≡ `=> e`
    b6 = ARM_IF + NL + "  let v = match x { Some(a) => { a + 1 } None => 0 };" + NL + "}" + NL
    f6 = (
        FN_SIG
        + NL
        + "    "
        + PROLOGUE_END
        + NL
        + "  let v = match x { Some(a) => a + 1, None => 0 };"
        + NL
        + "    "
        + WRITE_BACK
        + NL
        + "}"
        + NL
    )
    ok6, rep6 = compare(brace_body(b6, ARM_IF), brace_body(f6, FN_SIG))
    check("match arm 花括号折叠 ⇒ GREEN", ok6, rep6)

    # 尾逗号增删：`f(a,)` ≡ `f(a)`
    b5 = ARM_IF + NL + "  g(a,);" + NL + "}" + NL
    f5 = (
        FN_SIG
        + NL
        + "    "
        + PROLOGUE_END
        + NL
        + "  g(a);"
        + NL
        + "    "
        + WRITE_BACK
        + NL
        + "}"
        + NL
    )
    ok5, rep5 = compare(brace_body(b5, ARM_IF), brace_body(f5, FN_SIG))
    check("尾逗号增删 ⇒ GREEN", ok5, rep5)

    # 提取自证：花括号在字符串/注释里不干扰
    tricky = ARM_IF + '\n  log("{} {"); // } {\n  let z = 1;\n}\n'
    body = brace_body(tricky, ARM_IF)
    check("字符串/注释里的花括号不干扰提取", body is not None and "let z = 1;" in body, body)

    failed = [c for c in checks if not c[1]]
    for name, okc, detail in checks:
        print("%s %s" % ("PASS" if okc else "FAIL", name))
        if not okc:
            print("  detail: %r" % (detail,))
    print("RESULT: %s（%d/%d）" % ("GREEN" if not failed else "RED", len(checks) - len(failed), len(checks)))
    return 0 if not failed else 1


def main(argv):
    ap = argparse.ArgumentParser(add_help=True)
    ap.add_argument("--base", default="")
    ap.add_argument("--head", default="WORKTREE")
    ap.add_argument("--path", default="mir2-rs/bins/game-svr/src/lib.rs")
    ap.add_argument("--selftest", action="store_true")
    args = ap.parse_args(argv)
    if args.selftest:
        return selftest()
    if not args.base:
        print("判不了：缺少 --base <rev>（或用 --selftest）")
        return 2
    return run_check(_repo_root(), args.path, args.base, args.head)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
