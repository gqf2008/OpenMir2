"""六类流程效果对拍：同一脚本 + 同一 label + 同一初始状态，跑 C# 与 Rust 两侧并 diff。

用法:
  python -I tools/flow-diff.py <用例文件>

用例文件（JSON 数组）每项:
  {"name": "传送-幻境", "script": "E:/.../比奇省传送员-0.txt", "label": "@gohj2",
   "gold": 200000, "level": 10, "items": "金币:1:1", "player_items": "金币:5"}

输出：每例打印两侧日志 diff（一致则打印 OK）。
"""
import json
import pathlib
import subprocess
import sys

REPO = pathlib.Path(__file__).resolve().parents[1]      # mir2-rs/
ROOT = REPO.parent                                       # 仓库根


def run_rust(case):
    args = [
        "cargo", "run", "-q", "-p", "mir2-script-tool", "--", "flow-run",
        case["script"], case["label"], "--gold", str(case.get("gold", 1000)),
        "--level", str(case.get("level", 10)),
    ]
    if case.get("items"):
        args += ["--items", case["items"]]
    if case.get("player_items"):
        args += ["--player-items", case["player_items"]]
    r = subprocess.run(args, cwd=REPO, capture_output=True, text=True, encoding="utf-8")
    return r.stdout.strip().splitlines(), r.stderr.strip()


def run_cs(case):
    args = [
        "dotnet", "run", "--project", str(REPO / "tools/script-parity-cs"), "--no-build", "--",
        "run", case["script"], case["label"], "--gold", str(case.get("gold", 1000)),
        "--level", str(case.get("level", 10)),
    ]
    if case.get("items"):
        args += ["--items", case["items"]]
    if case.get("player_items"):
        args += ["--player-items", case["player_items"]]
    r = subprocess.run(args, cwd=REPO, capture_output=True, text=True, encoding="utf-8")
    return r.stdout.strip().splitlines(), r.stderr.strip()


def main():
    cases = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
    ok = bad = 0
    for case in cases:
        rust, rust_err = run_rust(case)
        cs, cs_err = run_cs(case)
        if rust == cs:
            ok += 1
            print(f"OK   {case['name']}: {len(cs)} 行一致")
            if cs:
                for line in cs:
                    print(f"       {line}")
        else:
            bad += 1
            print(f"DIFF {case['name']}")
            print(f"  C#  ({len(cs)} 行): {cs}")
            print(f"  Rust({len(rust)} 行): {rust}")
            if rust_err:
                print(f"  rust stderr: {rust_err[:400]}")
            if cs_err:
                print(f"  cs stderr: {cs_err[:400]}")
    print(f"\n== 用例 {len(cases)}：一致 {ok} / 不一致 {bad} ==")
    return 0 if bad == 0 else 1


if __name__ == "__main__":
    raise SystemExit(main())
