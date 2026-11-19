#!/usr/bin/env python3
"""IT-18 证据捕获：M1/M2（实现变异）+ Z4/Z5/Z6（比较器变异）各出「红原文 + 还原后复绿」。

每条：变异源码 → 跑对应用例（应红）→ 还原（sha256 必须回到变异前）→ 复跑（应绿）。
全部单 crate 定点（`cargo test -p game-svr --lib <filter>`），不改产品行为，不改夹具。
运行：`python -I -B capture.py`（在仓库 mir2-rs 已构建过的机器上；首次会编译）。
"""
import hashlib
import os
import subprocess
import sys

ROOT = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.abspath(os.path.join(ROOT, "..", "..", "..", "..", "..", ".."))
SRC = os.path.join(REPO, "mir2-rs", "bins", "game-svr", "src", "useitem.rs")
MIR2 = os.path.join(REPO, "mir2-rs")


def sha(b: bytes) -> str:
    return hashlib.sha256(b).hexdigest()


def run(flt: str):
    env = dict(os.environ)
    env["CARGO_BUILD_JOBS"] = "6"
    p = subprocess.run(
        ["cargo", "test", "-p", "game-svr", "--lib", flt, "--", "--test-threads=1", "--nocapture"],
        cwd=MIR2,
        env=env,
        capture_output=True,
    )
    return p.returncode, (p.stdout + p.stderr).decode("utf-8", "replace")


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
    (
        "D_drain_cap_removed",
        "!hit_cap && max_passes == 8",
        "!hit_cap && max_passes == 1",
        "useitem::tests::matches_oracle_eat_fixture",
    ),
]


def main() -> int:
    orig = open(SRC, "rb").read()
    orig_sha = sha(orig)
    with open(os.path.join(ROOT, "sha_before.txt"), "w", newline="\n") as f:
        f.write("useitem.rs sha256=" + orig_sha + "\n")
    cmds = []
    failures = 0
    for name, old, new, flt in MUTATIONS:
        txt = orig.decode("utf-8")
        if txt.count(old) != 1:
            print("MUTATION TARGET NOT UNIQUE:", name, "count=", txt.count(old))
            return 2
        open(SRC, "wb").write(txt.replace(old, new).encode("utf-8"))
        rc_red, out_red = run(flt)
        with open(
            os.path.join(ROOT, name + "_red.txt"), "w", encoding="utf-8", newline="\n"
        ) as f:
            f.write(
                "# mutation: %r -> %r\n# filter: %s\n# rc=%d\n\n"
                % (old, new, flt, rc_red)
                + out_red
            )
        open(SRC, "wb").write(orig)
        if sha(open(SRC, "rb").read()) != orig_sha:
            print("RESTORE SHA MISMATCH:", name)
            return 2
        rc_grn, out_grn = run(flt)
        with open(
            os.path.join(ROOT, name + "_green.txt"), "w", encoding="utf-8", newline="\n"
        ) as f:
            f.write("# restored; filter: %s\n# rc=%d\n\n" % (flt, rc_grn) + out_grn)
        red_ok = rc_red != 0 and "test result: FAILED" in out_red
        grn_ok = rc_grn == 0 and "test result: ok" in out_grn
        cmds.append(
            "%s: red_rc=%d red_failed=%s green_rc=%d green_ok=%s"
            % (name, rc_red, "test result: FAILED" in out_red, rc_grn, "test result: ok" in out_grn)
        )
        if not (red_ok and grn_ok):
            failures += 1
    rc_full, out_full = run("useitem::tests")
    with open(os.path.join(ROOT, "final_green.txt"), "w", encoding="utf-8", newline="\n") as f:
        f.write("# restored; full filter: useitem::tests\n# rc=%d\n\n" % rc_full + out_full)
    with open(os.path.join(ROOT, "commands.txt"), "w", encoding="utf-8", newline="\n") as f:
        f.write(
            "cargo test -p game-svr --lib <filter> -- --test-threads=1  (CARGO_BUILD_JOBS=6)\n"
            "useitem.rs sha256=" + orig_sha + "\n\n" + "\n".join(cmds) + "\n"
        )
    print("\n".join(cmds))
    print("final_full_rc=", rc_full)
    return 0 if (failures == 0 and rc_full == 0) else 1


if __name__ == "__main__":
    sys.exit(main())
