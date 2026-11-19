#!/usr/bin/env python3
"""IT-18 证据捕获（M1/M2/Z4/Z5/Z6）。

每条：变异 useitem.rs 源码 → 跑对应用例（应红）→ 还原（sha256 必须回原始）→ 复跑（应绿）。
stdout / stderr **分段**并带时间戳（cargo 的编译行在 stderr、test 输出在 stdout，混排会误读）。
D（抽空上限）证据另见 capture_drain_cap.py（它变异的是 eat-probe，需重出夹具）。

运行：`python -I -B capture.py`（单 crate 定点，CARGO_BUILD_JOBS=6）。
"""
import datetime
import hashlib
import os
import subprocess
import sys

ROOT = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.abspath(os.path.join(ROOT, "..", "..", "..", "..", "..", ".."))
SRC = os.path.join(REPO, "mir2-rs", "bins", "game-svr", "src", "useitem.rs")
MIR2 = os.path.join(REPO, "mir2-rs")


def ts():
    return datetime.datetime.now().astimezone().isoformat(timespec="seconds")


def sha(b):
    return hashlib.sha256(b).hexdigest()


def run(flt):
    env = dict(os.environ)
    env["CARGO_BUILD_JOBS"] = "6"
    p = subprocess.run(
        ["cargo", "test", "-p", "game-svr", "--lib", flt, "--", "--test-threads=1", "--nocapture"],
        cwd=MIR2,
        env=env,
        capture_output=True,
    )
    return p.returncode, p.stdout.decode("utf-8", "replace"), p.stderr.decode("utf-8", "replace")


def write_seg(path, header, rc, out, err):
    with open(path, "w", encoding="utf-8", newline="\n") as f:
        f.write(header)
        f.write("=== stderr (cargo/build) ===\n" + err)
        f.write("=== stdout (test harness) ===\n" + out)
        f.write("# rc=%d\n" % rc)


MUTATIONS = [
    (
        "M1_unbind_count",
        "for _ in 0..6 {",
        "for _ in 0..5 {",
        "useitem::tests::unbind_stdmode31_success_matches_oracle_fixture",
    ),
    (
        "M2_unbind_dura",
        "dura: tpl_unbind.dura_max,",
        "dura: 0,",
        "useitem::tests::unbind_stdmode31_success_matches_oracle_fixture",
    ),
    (
        "Z4_gate_removed",
        "!golden_nonzero || rust_nonzero",
        "true",
        "useitem::tests::z4_golden_nonzero_rust_zero_red",
    ),
    (
        "Z5_layer_removed",
        "    fn fixture_covers_two_layers(doc: &Value) -> bool {\n",
        "    fn fixture_covers_two_layers(doc: &Value) -> bool {\n        return true;\n",
        "useitem::tests::z5_single_layer_red",
    ),
    (
        "Z6_notice_equal_stubbed",
        "        a.sort();\n        b.sort();\n        a == b\n    }",
        "        a.sort();\n        b.sort();\n        true\n    }",
        "useitem::tests::tooth_notice_priority_changed_red",
    ),
]


def main():
    orig = open(SRC, "rb").read()
    osha = sha(orig)
    with open(os.path.join(ROOT, "sha_before.txt"), "w", newline="\n") as f:
        f.write("useitem.rs sha256=" + osha + "\n# ts=" + ts() + "\n")
    cmds = []
    fails = 0
    for name, old, new, flt in MUTATIONS:
        txt = orig.decode("utf-8")
        if txt.count(old) != 1:
            print("MUTATION TARGET NOT UNIQUE:", name, "count=", txt.count(old))
            return 2
        open(SRC, "wb").write(txt.replace(old, new).encode("utf-8"))
        rc_r, o_r, e_r = run(flt)
        write_seg(
            os.path.join(ROOT, name + "_red.txt"),
            "# ts=%s\n# filter=%s\n# mutation: %r -> %r\n# expect: FAILED\n" % (ts(), flt, old, new),
            rc_r,
            o_r,
            e_r,
        )
        open(SRC, "wb").write(orig)
        if sha(open(SRC, "rb").read()) != osha:
            print("RESTORE SHA MISMATCH:", name)
            return 2
        rc_g, o_g, e_g = run(flt)
        write_seg(
            os.path.join(ROOT, name + "_green.txt"),
            "# ts=%s\n# filter=%s\n# restored (sha256 == sha_before)\n# expect: ok\n" % (ts(), flt),
            rc_g,
            o_g,
            e_g,
        )
        red_ok = rc_r != 0 and "test result: FAILED" in o_r
        grn_ok = rc_g == 0 and "test result: ok" in o_g
        cmds.append(
            "%s ts=%s red_rc=%d red_failed=%s green_rc=%d green_ok=%s"
            % (name, ts(), rc_r, "test result: FAILED" in o_r, rc_g, "test result: ok" in o_g)
        )
        if not (red_ok and grn_ok):
            fails += 1
    rc_f, o_f, e_f = run("useitem::tests")
    write_seg(
        os.path.join(ROOT, "final_green.txt"),
        "# ts=%s\n# full useitem::tests (restored)\n# expect: ok\n" % ts(),
        rc_f,
        o_f,
        e_f,
    )
    with open(os.path.join(ROOT, "commands.txt"), "w", encoding="utf-8", newline="\n") as f:
        f.write(
            "cargo test -p game-svr --lib <filter> -- --test-threads=1 --nocapture  (CARGO_BUILD_JOBS=6)\n"
            "useitem.rs sha256=" + osha + "\n" + "\n".join(cmds) + "\n"
        )
    print("\n".join(cmds))
    print("final_full_rc=", rc_f)
    return 0 if (fails == 0 and rc_f == 0) else 1


if __name__ == "__main__":
    sys.exit(main())
