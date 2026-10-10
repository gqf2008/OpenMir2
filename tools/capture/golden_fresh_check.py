#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""golden_fresh_check.py — 入库金标准的「新鲜度 + 登记」自动对账（C4 续）。

**为什么需要它**：改了导出器/协议口径之后，入库的金标准 artifact 不会自己更新 ——
"旧 artifact + 新下游"会让下游（replay 等）对新口径产生**假红/假绿**，
而登记 sha256 若是对着旧件算的，会**自洽地通过**所有"hash 对不对"的自检。
本项目 2026-10-10 实测栽过一次：C4 改了导出器口径，只重导了 C3 与上批 16 帧，
C2 那份还是 C4 前的旧 schema（缺 frame_form/layout/body_segments），
用当前 replay 跑它 ⇒ act 帧口径假红 66 条 + ② 字段对拍 21 条，
README 却写着"② = 0"。⇒ 结论：**"登记能对上"不能证明产物是新的**，要另配判据。

本工具对每份入库金标准做三件事（任一不符即 RED，退出码 1）：
  1. **集合对账**：`mir2-rs/tests/golden/*.jsonl` 与 `registry.json` 双向一致
     （新增金标准忘了登记 / 登记项文件不在 ⇒ 立刻可见）。
  2. **新鲜度**：用**当前**导出器 + **当前派生**的分段布局表，从登记的来源裸 dump 重导，
     结果必须与入库文件**逐字节相同**（= 入库件就是当前口径的产物，真值仍是裸 dump）。
  3. **登记一致性**：README.md 登记表里该行的 sha256 必须等于文件实际 sha256
     （提交后 `sha256(git show HEAD:<file>)` 亦同：本仓 `.gitattributes` 对 `*.jsonl` 固定 LF）。

用法：
  python tools/capture/golden_fresh_check.py                # 默认全量
  python tools/capture/golden_fresh_check.py --only c2       # 只查文件名含 c2 的
  python tools/capture/golden_fresh_check.py --selftest      # 改坏必红（临时目录里做，不动入库件）
前置：`dotnet build tools/capture/GoldenExport/GoldenExport.csproj`（缺可执行体则退出码 2）。
"""
import argparse
import glob
import hashlib
import json
import os
import shutil
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.abspath(os.path.join(HERE, "..", ".."))
DEFAULT_GOLDEN_DIR = os.path.join(REPO, "mir2-rs", "tests", "golden")

FAILS = []


def fail(msg):
    FAILS.append(msg)
    print("RED: " + msg)


def sha256_file(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def readme_registered_sha(filename, golden_dir):
    """从 README 登记表里取该文件的登记 sha256（每行 = 一条登记，最后一格是 hash）。"""
    readme = os.path.join(golden_dir, "README.md")
    with open(readme, encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if not line.startswith("|"):
                continue
            cells = [c.strip() for c in line.strip("|").split("|")]
            if len(cells) >= 2 and cells[0].strip("`") == filename:
                last = cells[-1].strip("`").strip()
                if len(last) == 64 and all(c in "0123456789abcdef" for c in last):
                    return last
                fail(f"README 里 {filename} 那一行的最后一格不是 sha256：{last!r}")
                return None
    fail(f"README 登记表里找不到 {filename} 的行（新增金标准必须登记来源/时间/sha256）")
    return None


def derive_layout(out_json):
    cmd = [sys.executable, os.path.join(HERE, "dump_body_layout.py"),
           "--frame-rs", os.path.join(REPO, "mir2-rs", "crates", "protocol", "src", "frame.rs"),
           "--constants", os.path.join(REPO, "mir2-rs", "crates", "protocol", "src", "messages.rs"),
           "--out", out_json]
    p = subprocess.run(cmd, capture_output=True, text=True)
    if p.returncode != 0:
        fail("派生分段布局表失败：" + (p.stdout + p.stderr).strip())
        return None
    return out_json


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--exe", default=os.path.join(HERE, "GoldenExport", "bin", "Debug", "net8.0",
                                                  "GoldenExport.exe"))
    ap.add_argument("--only", default=None, help="只查文件名含该子串的项（调试用）")
    ap.add_argument("--golden-dir", default=DEFAULT_GOLDEN_DIR)
    ap.add_argument("--selftest", action="store_true", help="改坏必红（在临时目录里做，不动入库件）")
    args = ap.parse_args()

    if not os.path.exists(args.exe):
        print("前置不满足：找不到导出器 %s\n先跑 dotnet build tools/capture/GoldenExport/GoldenExport.csproj" % args.exe)
        return 2
    if args.selftest:
        return selftest(args.exe)

    GOLDEN_DIR = args.golden_dir
    REGISTRY = os.path.join(GOLDEN_DIR, "registry.json")
    reg = json.load(open(REGISTRY, encoding="utf-8"))
    session_root = os.path.join(REPO, reg.get("session_root", os.path.join("tests", "golden")))
    entries = [g for g in reg["goldens"] if not args.only or args.only in g["file"]]
    on_disk = sorted(os.path.basename(p) for p in glob.glob(os.path.join(GOLDEN_DIR, "*.jsonl")))

    # ---- 1) 集合对账（新增/删除金标准必须同步 registry.json） ----
    reg_files = [g["file"] for g in reg["goldens"]]
    for f in on_disk:
        if f not in reg_files:
            fail(f"golden 目录里的 {f} 没在 registry.json 登记（新增金标准必须登记来源会话目录）")
    for f in reg_files:
        if f not in on_disk:
            fail(f"registry.json 登记的 {f} 在 {GOLDEN_DIR} 里不存在")
        p = os.path.join(GOLDEN_DIR, f)
        if os.path.exists(p):
            readme_registered_sha(f, GOLDEN_DIR)   # 登记行缺失/格式不对在这里就报红

    tmp = tempfile.mkdtemp(prefix="goldenfresh-")
    layout = derive_layout(os.path.join(tmp, "layout.json"))
    if layout is None:
        return 1

    # ---- 2) 新鲜度：当前口径重导 == 入库文件（逐字节） ----
    # ---- 3) 登记一致性：README 的 sha256 == 实际文件 sha256 ----
    for g in entries:
        f = g["file"]
        path = os.path.join(GOLDEN_DIR, f)
        if not os.path.exists(path):
            continue    # 集合对账已报红
        session = os.path.join(session_root, g["session"])
        actual = sha256_file(path)
        reg_sha = readme_registered_sha(f, GOLDEN_DIR)
        if reg_sha and reg_sha != actual:
            fail(f"{f} 登记 sha256 与实际文件不符：README={reg_sha[:16]}… 实际={actual[:16]}…（只改登记行/只换文件都算）")

        if not os.path.isdir(session):
            fail(f"{f} 的来源会话目录不存在：{session}")
            continue
        out = os.path.join(tmp, f)
        p = subprocess.run([args.exe, "--session", session, "--out", out, "--layout", layout],
                           capture_output=True, text=True)
        if p.returncode != 0:
            fail(f"{f} 重导失败（rc={p.returncode}）：" + (p.stdout + p.stderr).strip()[-300:])
            continue
        if not os.path.exists(out):
            fail(f"{f} 重导没有产出文件")
            continue
        fresh = sha256_file(out)
        if fresh != actual:
            fail(f"{f} 入库件已过期：当前口径重导 = {fresh[:16]}…，入库文件 = {actual[:16]}…"
                 f"（改了口径必须重导**全部**入库件并重登记；来源裸 dump：{g['session']}）")
        else:
            print(f"  PASS {f} 重导逐字节相同（sha256 {actual[:16]}…）且登记一致")

    if FAILS:
        print(f"RED: golden_fresh_check {len(FAILS)} 条变红")
        return 1
    print(f"GREEN: golden_fresh_check 全过（{len(entries)} 份入库件 = 当前口径产物，且登记自洽）")
    return 0


def selftest(exe):
    """改坏必红：在临时目录里复制一份登记目录，分别制造三类坏
    （入库件与当前口径不符 / 只改登记行 / registry 缺项），每次都必须判 RED；入库件本身不动。"""
    import re
    tmp = tempfile.mkdtemp(prefix="goldenfresh-selftest-")
    gdir = os.path.join(tmp, "golden")
    os.makedirs(gdir)
    for name in ("registry.json", "README.md"):
        shutil.copy2(os.path.join(DEFAULT_GOLDEN_DIR, name), os.path.join(gdir, name))
    reg = json.load(open(os.path.join(DEFAULT_GOLDEN_DIR, "registry.json"), encoding="utf-8"))
    for g in reg["goldens"]:
        shutil.copy2(os.path.join(DEFAULT_GOLDEN_DIR, g["file"]), os.path.join(gdir, g["file"]))

    def run():
        p = subprocess.run([sys.executable, os.path.abspath(__file__), "--exe", exe,
                            "--golden-dir", gdir], capture_output=True, text=True)
        return p.returncode, p.stdout + p.stderr

    ok = True
    rc, log = run()
    if rc != 0:
        print("RED: 自测第 1 步（干净副本）不是 GREEN：\n" + log[-500:])
        ok = False
    else:
        print("  PASS 1/4 干净副本 → GREEN")

    # 坏 1：入库件被改（模拟"旧 schema / 手工改过"），登记 sha 没跟着改
    victim = os.path.join(gdir, reg["goldens"][1]["file"])
    good = open(victim, "rb").read()
    open(victim, "wb").write(good.replace(b'"body_len":', b'"body_lenn":', 1))
    rc, log = run()
    if rc == 0 or "入库件已过期" not in log or "登记 sha256 与实际文件不符" not in log:
        print("RED: 自测第 2 步（入库件与当前口径不符）没有同时判出「过期 + 登记不符」：\n" + log[-500:])
        ok = False
    else:
        print("  PASS 2/4 入库件与当前口径不符 → RED（过期 + 登记不符都点到）")
    open(victim, "wb").write(good)

    # 坏 2：只改登记行（README 的 hash 换成别的），artifact 没动
    rp = os.path.join(gdir, "README.md")
    txt = open(rp, encoding="utf-8").read()
    f0 = reg["goldens"][0]["file"]
    m = re.search(r"(\| `" + re.escape(f0) + r"`[^\n]*\| `)([0-9a-f]{64})(` \|)", txt)
    if not m:
        print("RED: 自测第 3 步在 README 里找不到可改的登记行")
        ok = False
    else:
        open(rp, "w", encoding="utf-8", newline="\n").write(
            txt[:m.start(2)] + ("0" * 64) + txt[m.end(2):])
        rc, log = run()
        if rc == 0 or "登记 sha256 与实际文件不符" not in log:
            print("RED: 自测第 3 步（只改登记行）没判红：\n" + log[-500:])
            ok = False
        else:
            print("  PASS 3/4 只改登记行（artifact 未动）→ RED")
        shutil.copy2(os.path.join(DEFAULT_GOLDEN_DIR, "README.md"), rp)

    # 坏 3：registry 缺一项（新增金标准忘了登记）
    reg2 = json.load(open(os.path.join(gdir, "registry.json"), encoding="utf-8"))
    reg2["goldens"] = reg2["goldens"][:-1]
    json.dump(reg2, open(os.path.join(gdir, "registry.json"), "w", encoding="utf-8", newline="\n"),
              ensure_ascii=False, indent=2)
    rc, log = run()
    if rc == 0 or "没在 registry.json 登记" not in log:
        print("RED: 自测第 4 步（registry 缺项）没判红：\n" + log[-500:])
        ok = False
    else:
        print("  PASS 4/4 registry 缺一项 → RED")

    if not ok:
        print("RED: golden_fresh_check selftest 有未通过项")
        return 1
    print("GREEN: golden_fresh_check selftest 4/4（干净副本绿 + 三类坏各自红）")
    return 0


if __name__ == "__main__":
    sys.exit(main())
