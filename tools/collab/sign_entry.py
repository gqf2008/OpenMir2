# -*- coding: utf-8 -*-
"""dev-2 车道签名脚本（卡 openmir2-signing-script-head-placeholder，A1/A2）。

修复点：head/base 一律 `git rev-parse <arg>` 实取并 **assert 全 40 位小写 hex**；
非 hex（含 %P/散文/缩写）**拒签**（exit 2，不产出条目）。parent 同样必须 40 位 hex。

用法：
  python sign_entry.py --repo <worktree> --kind patch --id <thread> \\
      --parent <40hex|tail> --body-file body.json \\
      [--head HEAD] [--base <rev>] [--push]

body.json 里若含 "head"/"base" 键：
  * 值为 "@HEAD"/"@<rev>" 形态 ⇒ 由本脚本 rev-parse 展开为全 40 位小写 hex；
  * 值不是 hex 且不是 @rev 形态 ⇒ 拒签（复现夹具即此形态）。

--selftest：离线跑 A2 复现夹具（坏占位符 ⇒ 拒签；@HEAD ⇒ 展开成 hex）。
"""
import argparse
import json
import re
import subprocess
import sys

HEX40 = re.compile(r"^[0-9a-f]{40}$")


def die(msg):
    print("sign_entry: REFUSED: %s" % msg, file=sys.stderr)
    sys.exit(2)


def rev_parse(repo, rev):
    """rev-parse 实取；失败/非 40 位小写 hex ⇒ None。"""
    try:
        out = subprocess.run(["git", "-C", repo, "rev-parse", rev],
                             capture_output=True, text=True, timeout=30)
    except Exception:
        return None
    v = (out.stdout or "").strip()
    return v if (out.returncode == 0 and HEX40.match(v)) else None


def expand_rev(repo, spec, name):
    """@rev 或 40-hex 字面量 ⇒ 40 位小写 hex；否则拒签。"""
    if spec.startswith("@"):
        v = rev_parse(repo, spec[1:])
        if v is None:
            die("%s 的 @%s 无法 rev-parse 成 40 位 hex" % (name, spec[1:]))
        return v
    return check_hex(name, spec)


def check_hex(name, value):
    """签名前断言：必须是全 40 位小写 hex（非 hex 拒签）。"""
    if not isinstance(value, str) or not HEX40.match(value):
        die("%s=%r 不是 40 位小写 hex（占位符/散文/缩写都拒签）" % (name, value))
    return value


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--repo", default=".")
    ap.add_argument("--kind", default=None)
    ap.add_argument("--id", default=None)
    ap.add_argument("--parent", default=None, help="线程尾 entry oid（40 hex）")
    ap.add_argument("--body-file", default=None)
    ap.add_argument("--head", default=None, help="body.head 的取值来源 rev（默认 HEAD）")
    ap.add_argument("--base", default=None, help="body.base 的取值来源 rev")
    ap.add_argument("--refs-head", default=None,
                    help="walgit --head（进 entry.refs，机器读者用）；@rev 或 40 hex")
    ap.add_argument("--refs-base", default=None,
                    help="walgit --base（进 entry.refs）；@rev 或 40 hex")
    ap.add_argument("--push", action="store_true")
    ap.add_argument("--dry-run", action="store_true",
                    help="只校验并打印将签的 body，不调 walgit")
    ap.add_argument("--selftest", action="store_true")
    args = ap.parse_args()
    if args.selftest:
        return selftest(args.repo)

    if args.selftest:
        return selftest(args.repo)

    body = json.load(open(args.body_file, encoding="utf-8"))

    # ---- A1 核心：head/base 展开与断言（先于任何签名动作）----
    if "head" in body:
        v = body["head"]
        if isinstance(v, str) and v.startswith("@"):
            real = rev_parse(args.repo, v[1:])
            if real is None:
                die("body.head 的 @%s 无法 rev-parse 成 40 位 hex" % v[1:])
            body["head"] = real
        check_hex("body.head", body["head"])
    if "base" in body:
        v = body["base"]
        if isinstance(v, str) and v.startswith("@"):
            real = rev_parse(args.repo, v[1:])
            if real is None:
                die("body.base 的 @%s 无法 rev-parse 成 40 位 hex" % v[1:])
            body["base"] = real
        check_hex("body.base", body["base"])
    # refs.head / refs.base（entry.refs —— 机器读者按这里取）：@rev 展开成 40 hex。
    refs_head = refs_base = None
    if args.refs_head is not None:
        refs_head = expand_rev(args.repo, args.refs_head, "refs_head")
    if args.refs_base is not None:
        refs_base = expand_rev(args.repo, args.refs_base, "refs_base")
    # parent（线程尾）也必须是 hex：防「从散文里截一段」当 parent。
    check_hex("parent", args.parent)
    # kind/id 白名单形状（防手滑把 body 文件当 kind）。
    if args.kind not in ("issue", "status", "patch", "review", "merge_result",
                         "comment", "review_request"):
        die("kind=%r 不在允许集合" % args.kind)
    if not args.id or not re.match(r"^[\w\-]{4,80}$", args.id):
        die("id=%r 形状不像线程 id" % args.id)

    payload = json.dumps(body, ensure_ascii=False)
    print("sign_entry: OK head=%s base=%s parent=%s"
          % (body.get("head", "-"), body.get("base", "-"), args.parent[:12]))
    if args.dry_run:
        print(payload)
        return 0

    cmd = ["walgit", "collab", "entry", "--repo", args.repo, "--kind", args.kind,
           "--id", args.id, "--parent", args.parent, "--body", payload]
    if refs_head is not None:
        cmd += ["--head", refs_head]
    if refs_base is not None:
        cmd += ["--base", refs_base]
    if args.push:
        cmd += ["--push", "walgit"]
    pr = subprocess.run(cmd, capture_output=True, text=True)
    sys.stdout.write(pr.stdout or "")
    sys.stderr.write(pr.stderr or "")
    if pr.returncode != 0:
        die("walgit collab entry 失败 rc=%d" % pr.returncode)
    return pr.returncode


def selftest(repo):
    """A2 复现夹具：坏占位符走签单流程。

    * 修复前形态（body.head = 字面 %P 散文）⇒ 本脚本**拒签** exit 2（旧流程会照签）；
    * @HEAD 形态 ⇒ 展开成 40 位小写 hex；
    * parent 非 hex ⇒ 拒签。
    """
    import tempfile, os
    cases = []

    def run_case(body, parent, want_refuse):
        f = tempfile.NamedTemporaryFile(
            mode="w", suffix=".json", delete=False, encoding="utf-8")
        json.dump(body, f, ensure_ascii=False)
        f.close()
        pr = subprocess.run(
            [sys.executable, "-B", __file__, "--repo", repo, "--kind", "patch",
             "--id", "selftest-signing-case", "--parent", parent,
             "--body-file", f.name, "--dry-run"],
            capture_output=True, text=True)
        os.unlink(f.name)
        refused = (pr.returncode == 2)
        assert refused == want_refuse, (
            "case head=%r parent=%r: refused=%r want=%r stderr=%s"
            % (body.get("head"), parent[:12], refused, want_refuse, pr.stderr))
        cases.append((body.get("head"), refused))

    good = "0123456789abcdef0123456789abcdef01234567"
    # ① 事故原形态：字面 %P 散文 ⇒ 必须拒签（旧流程会照签产出 refs.head 缺失条目）。
    run_case({"head": "docs 片单提交（parent=b5442dbd4，%P 实取）", "base": "b5442dbd4"},
             good, True)
    # ② 缩写也不收：12 位短哈希 ⇒ 拒签。
    run_case({"head": "b5442dbd440d", "base": "b5442dbd4"}, good, True)
    # ③ @HEAD ⇒ 展开（dry-run 不签，只校验）。
    run_case({"head": "@HEAD", "base": "@HEAD"}, good, False)
    # ④ parent 非 hex ⇒ 拒签。
    run_case({"head": "@HEAD", "base": "@HEAD"}, "%P 实取", True)
    # ⑤ 全 hex 直填 ⇒ 放行。
    run_case({"head": good, "base": good}, good, False)

    print("sign_entry selftest: OK (%d cases：散文拒签/缩写拒签/@HEAD 展开/"
          "parent 非 hex 拒签/全 hex 放行)" % len(cases))
    return 0


if __name__ == "__main__":
    sys.exit(main())
