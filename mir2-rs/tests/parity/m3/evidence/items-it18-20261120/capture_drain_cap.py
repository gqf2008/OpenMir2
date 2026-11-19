#!/usr/bin/env python3
"""IT-18 D 组证据：把 eat-probe 的抽空上限 `MaxDrainPasses` 8→1（并让记录的 `max_passes` 跟随），
重出夹具 `eat_vectors.json`（每例 `middle.max_passes=1`）→ 跑**主用例** `matches_oracle_eat_fixture`
⇒ 必红（`抽空上限异常 ... max_passes=1`）→ 还原源码与夹具（sha256 回原始）→ 复绿。

为什么变异在 eat-probe 而非 useitem.rs：`max_passes` 是**记录面**（eat-probe 写进夹具）的值；
比较器（useitem.rs）只是读它并断言 ==8。只改 useitem.rs 的常量不会改变夹具记录值，红原文仍打印
夹具旧值（false/8），不是"上限被改"的证据。故正确变异点是记录面上限本身。

stdout / stderr 分段、带时间戳；并保存 Program.cs / 夹具的 `git diff` 作源码证据。
运行：`python -I -B capture_drain_cap.py`。
"""
import datetime
import hashlib
import json
import os
import subprocess
import sys

ROOT = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.abspath(os.path.join(ROOT, "..", "..", "..", "..", "..", ".."))
PROBE = os.path.join(REPO, "mir2-rs", "tests", "parity", "m3", "eat-probe", "Program.cs")
FIXTURE = os.path.join(REPO, "mir2-rs", "tests", "parity", "m3", "fixtures", "eat_vectors.json")
MIR2 = os.path.join(REPO, "mir2-rs")
PROJ = os.path.join(REPO, "mir2-rs", "tests", "parity", "m3", "eat-probe")
DLL = os.path.join(PROJ, "bin", "Release", "net8.0", "eat-probe.dll")

A_OLD = "const int MaxDrainPasses = 8;"
A_NEW = "const int MaxDrainPasses = 1;"
B_OLD = "max_passes = 8"
B_NEW = "max_passes = 1"


def ts():
    return datetime.datetime.now().astimezone().isoformat(timespec="seconds")


def sha(b):
    return hashlib.sha256(b).hexdigest()


def run(cmd, cwd=None, env=None):
    p = subprocess.run(cmd, cwd=cwd, env=env, capture_output=True)
    return p.returncode, p.stdout.decode("utf-8", "replace"), p.stderr.decode("utf-8", "replace")


def write_seg(path, header, rc, out, err):
    with open(path, "w", encoding="utf-8", newline="\n") as f:
        f.write(header)
        f.write("=== stderr ===\n" + err)
        f.write("=== stdout ===\n" + out)
        f.write("# rc=%d\n" % rc)


def main():
    probe0 = open(PROBE, "rb").read()
    fix0 = open(FIXTURE, "rb").read()
    psha0 = sha(probe0)
    fsha0 = sha(fix0)
    with open(os.path.join(ROOT, "D_sha_before.txt"), "w", newline="\n") as f:
        f.write(
            "Program.cs sha256=" + psha0 + "\n"
            "eat_vectors.json sha256=" + fsha0 + "\n# ts=" + ts() + "\n"
        )

    txt = probe0.decode("utf-8")
    for o in (A_OLD, B_OLD):
        if txt.count(o) != 1:
            print("MUTATION TARGET NOT UNIQUE:", repr(o), txt.count(o))
            return 2
    open(PROBE, "wb").write(txt.replace(A_OLD, A_NEW).replace(B_OLD, B_NEW).encode("utf-8"))

    rc, d, _ = run(["git", "diff", "--", "mir2-rs/tests/parity/m3/eat-probe/Program.cs"], cwd=REPO)
    with open(os.path.join(ROOT, "D_probe_diff.txt"), "w", encoding="utf-8", newline="\n") as f:
        f.write("# ts=%s\n# Program.cs mutated (MaxDrainPasses 8->1, max_passes 8->1)\n" % ts() + d)

    env = dict(os.environ)
    env["CARGO_BUILD_JOBS"] = "6"
    env["DOTNET_CLI_TELEMETRY_OPTOUT"] = "1"
    rc_b, o_b, e_b = run(["dotnet", "build", PROJ, "-c", "Release", "-v", "q"], env=env)
    write_seg(os.path.join(ROOT, "D_build.txt"), "# ts=%s\n# dotnet build (mutated probe)\n" % ts(), rc_b, o_b, e_b)
    if rc_b != 0:
        open(PROBE, "wb").write(probe0)
        open(FIXTURE, "wb").write(fix0)
        print("BUILD FAILED (restored)")
        return 2
    # 探针按 CWD 解析 `src/...` 源锚 ⇒ 必须在仓库根运行。
    rc_g, o_g, e_g = run(["dotnet", DLL, FIXTURE], cwd=REPO, env=env)
    write_seg(os.path.join(ROOT, "D_regen.txt"), "# ts=%s\n# dotnet eat-probe.dll -> fixture (cwd=repo root)\n" % ts(), rc_g, o_g, e_g)
    if rc_g != 0:
        open(PROBE, "wb").write(probe0)
        open(FIXTURE, "wb").write(fix0)
        print("REGEN FAILED (restored)")
        return 2

    fix1 = open(FIXTURE, "rb").read()
    doc = json.loads(fix1)
    mps = sorted({c["expect"]["drain"]["max_passes"] for c in doc["cases"]})
    with open(os.path.join(ROOT, "D_fixture_max_passes.txt"), "w", newline="\n") as f:
        f.write("distinct max_passes across cases after mutation: " + repr(mps) + "\n# ts=" + ts() + "\n")
    rc, dd, _ = run(["git", "diff", "--", "mir2-rs/tests/parity/m3/fixtures/eat_vectors.json"], cwd=REPO)
    with open(os.path.join(ROOT, "D_fixture_diff.txt"), "w", encoding="utf-8", newline="\n") as f:
        f.write("# ts=%s\n# fixture regenerated (max_passes 8->1 per case)\n" % ts() + dd)
    if mps != [1]:
        open(PROBE, "wb").write(probe0)
        open(FIXTURE, "wb").write(fix0)
        print("FIXTURE max_passes NOT 1:", mps, "(restored)")
        return 2

    rc_r, o_r, e_r = run(
        ["cargo", "test", "-p", "game-svr", "--lib", "useitem::tests::matches_oracle_eat_fixture",
         "--", "--test-threads=1", "--nocapture"],
        cwd=MIR2,
        env=env,
    )
    write_seg(
        os.path.join(ROOT, "D_drain_cap_red.txt"),
        "# ts=%s\n# filter=matches_oracle_eat_fixture (probe MaxDrainPasses=1)\n# expect: FAILED, message contains max_passes=1\n" % ts(),
        rc_r,
        o_r,
        e_r,
    )
    blob = o_r + e_r
    red_ok = rc_r != 0 and "test result: FAILED" in o_r and "max_passes=1" in blob and "抽空上限异常" in blob

    open(PROBE, "wb").write(probe0)
    open(FIXTURE, "wb").write(fix0)
    restored = sha(open(PROBE, "rb").read()) == psha0 and sha(open(FIXTURE, "rb").read()) == fsha0

    rc_g2, o_g2, e_g2 = run(
        ["cargo", "test", "-p", "game-svr", "--lib", "useitem::tests::matches_oracle_eat_fixture",
         "--", "--test-threads=1", "--nocapture"],
        cwd=MIR2,
        env=env,
    )
    write_seg(
        os.path.join(ROOT, "D_drain_cap_green.txt"),
        "# ts=%s\n# filter=matches_oracle_eat_fixture (restored: Program.cs+fixture sha256 back to before)\n# expect: ok\n" % ts(),
        rc_g2,
        o_g2,
        e_g2,
    )
    grn_ok = rc_g2 == 0 and "test result: ok" in o_g2

    with open(os.path.join(ROOT, "D_commands.txt"), "w", encoding="utf-8", newline="\n") as f:
        f.write(
            "Program.cs sha256(before)=%s\n" % psha0
            + "eat_vectors.json sha256(before)=%s\n" % fsha0
            + "mutation: %r -> %r ; %r -> %r\n" % (A_OLD, A_NEW, B_OLD, B_NEW)
            + "restore_sha_ok=%s red_ok=%s green_ok=%s ts=%s\n" % (restored, red_ok, grn_ok, ts())
        )
    print("D restore_sha_ok=%s red_ok=%s green_ok=%s" % (restored, red_ok, grn_ok))
    return 0 if (restored and red_ok and grn_ok) else 1


if __name__ == "__main__":
    sys.exit(main())
