#!/usr/bin/env python3
"""D3：排空辅助函数 `DrainPassLoop` 的「自然命中」单测（不依赖夹具、不依赖探针记录）。

- 基线：`dotnet build` + `eat-probe.dll --drain-selftest` ⇒ `ok=True`（`always_full` 每趟重满，跑到
  `MaxDrainPasses` ⇒ 辅助函数自己置 `hitCap=True`；`one_pass` ⇒ hitCap=False、passes=1；`none` ⇒ passes=0）。
- 变异：把 `DrainPassLoop` 触顶返回 `hitCap` 由 `true` 改 `false` ⇒ 自测 `ok=False`（红）。
- 还原（Program.cs sha256 回基线）⇒ 自测复绿。

主用例 gate 对 `hit_cap=true` 的拒绝由 D2（useitem.rs 变异，见 capture.py 的 `D2_hit_cap_forced`）证明。
运行：`python -I -B capture_drain_selftest.py`（需 dotnet；会临时改 `eat-probe/Program.cs` 后字节还原）。
"""
import datetime
import hashlib
import os
import subprocess
import sys

ROOT = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.abspath(os.path.join(ROOT, "..", "..", "..", "..", "..", ".."))
PROBE = os.path.join(REPO, "mir2-rs", "tests", "parity", "m3", "eat-probe", "Program.cs")
PROJ = os.path.join(REPO, "mir2-rs", "tests", "parity", "m3", "eat-probe")
DLL = os.path.join(PROJ, "bin", "Release", "net8.0", "eat-probe.dll")

OLD = "        return (maxPasses, true);"
NEW = "        return (maxPasses, false);"


def ts():
    return datetime.datetime.now().astimezone().isoformat(timespec="seconds")


def sha(b):
    return hashlib.sha256(b).hexdigest()


def run(cmd, env=None):
    p = subprocess.run(cmd, env=env, capture_output=True)
    return p.returncode, p.stdout.decode("utf-8", "replace"), p.stderr.decode("utf-8", "replace")


def write_seg(path, header, rc, out, err):
    with open(path, "w", encoding="utf-8", newline="\n") as f:
        f.write(header)
        f.write("=== stderr ===\n" + err)
        f.write("=== stdout ===\n" + out)
        f.write("# rc=%d\n" % rc)


def build(env):
    return run(["dotnet", "build", PROJ, "-c", "Release", "-v", "q"], env=env)


def selftest(env):
    return run(["dotnet", DLL, "--drain-selftest"], env=env)


def main():
    probe0 = open(PROBE, "rb").read()
    psha0 = sha(probe0)
    env = dict(os.environ)
    env["CARGO_BUILD_JOBS"] = "6"
    env["DOTNET_CLI_TELEMETRY_OPTOUT"] = "1"
    with open(os.path.join(ROOT, "D3_sha_before.txt"), "w", newline="\n") as f:
        f.write("Program.cs sha256=" + psha0 + "\n# ts=" + ts() + "\n")

    rc_b, o_b, e_b = build(env)
    write_seg(os.path.join(ROOT, "D3_build.txt"), "# ts=%s\n# baseline build\n" % ts(), rc_b, o_b, e_b)
    if rc_b != 0:
        print("BASELINE BUILD FAILED")
        return 2
    rc0, o0, e0 = selftest(env)
    write_seg(
        os.path.join(ROOT, "D3_drain_selftest_green.txt"),
        "# ts=%s\n# baseline: eat-probe.dll --drain-selftest\n# expect: ok=True, always_full hitCap=True\n" % ts(),
        rc0,
        o0,
        e0,
    )
    baseline_ok = rc0 == 0 and "ok=True" in o0 and "hitCap=True" in o0

    txt = probe0.decode("utf-8")
    if txt.count(OLD) != 1:
        print("MUTATION TARGET NOT UNIQUE:", repr(OLD), txt.count(OLD))
        return 2
    open(PROBE, "wb").write(txt.replace(OLD, NEW).encode("utf-8"))
    rc, d, _ = run(["git", "diff", "--", "mir2-rs/tests/parity/m3/eat-probe/Program.cs"], env=env)
    with open(os.path.join(ROOT, "D3_probe_diff.txt"), "w", encoding="utf-8", newline="\n") as f:
        f.write("# ts=%s\n# DrainPassLoop cap-return mutated true->false\n" % ts() + d)

    rc_b2, o_b2, e_b2 = build(env)
    if rc_b2 != 0:
        open(PROBE, "wb").write(probe0)
        print("MUTATED BUILD FAILED (restored)")
        return 2
    rc_r, o_r, e_r = selftest(env)
    write_seg(
        os.path.join(ROOT, "D3_drain_selftest_red.txt"),
        "# ts=%s\n# mutated: DrainPassLoop does not set hitCap on cap\n# expect: ok=False\n" % ts(),
        rc_r,
        o_r,
        e_r,
    )
    red_ok = rc_r != 0 and "ok=False" in o_r

    open(PROBE, "wb").write(probe0)
    restored = sha(open(PROBE, "rb").read()) == psha0
    rc_b3, o_b3, e_b3 = build(env)
    rc_g, o_g, e_g = (selftest(env) if rc_b3 == 0 else (rc_b3, o_b3, e_b3))
    write_seg(
        os.path.join(ROOT, "D3_drain_selftest_restored_green.txt"),
        "# ts=%s\n# restored (Program.cs sha256 == baseline)\n# expect: ok=True\n" % ts(),
        rc_g,
        o_g,
        e_g,
    )
    green_ok = rc_g == 0 and "ok=True" in o_g and "hitCap=True" in o_g

    with open(os.path.join(ROOT, "D3_commands.txt"), "w", encoding="utf-8", newline="\n") as f:
        f.write(
            "Program.cs sha256(before)=%s\n" % psha0
            + "mutation: %r -> %r\n" % (OLD, NEW)
            + "baseline_ok=%s red_ok=%s restore_sha_ok=%s restored_green_ok=%s ts=%s\n"
            % (baseline_ok, red_ok, restored, green_ok, ts())
        )
    print(
        "D3 baseline_ok=%s red_ok=%s restore_sha_ok=%s restored_green_ok=%s"
        % (baseline_ok, red_ok, restored, green_ok)
    )
    return 0 if (baseline_ok and red_ok and restored and green_ok) else 1


if __name__ == "__main__":
    sys.exit(main())
