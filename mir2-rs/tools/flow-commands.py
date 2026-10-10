"""六类 NPC 流程的命令使用分析（B2 范围收敛用）。

用法: python -I tools/flow-commands.py <Envir目录>
"""
import collections
import pathlib
import sys

ENVIR = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else r"E:\MirServer\M2GameSvr\Envir")

GROUPS = {
    "商店": lambda p: p.parent.name == "Market_Def",
    "传送": lambda p: any(k in p.name for k in ("传送", "老兵", "移动", "回城")),
    "修理": lambda p: any(k in p.name for k in ("铁匠", "武器", "修理", "首饰")),
    "仓库": lambda p: "仓库" in p.name or "保管" in p.name,
    "行会": lambda p: any(k in p.name for k in ("行会", "国王", "城堡", "沙巴克")),
    "任务": lambda p: any(k in p.name for k in ("任务", "QMission", "QFunction", "QManage")),
}

files = sorted(p for p in ENVIR.rglob("*.txt"))


def commands_of(path):
    text = path.read_bytes().decode("gbk", errors="replace")
    sec = None
    out = collections.Counter()
    for line in text.splitlines():
        line = line.strip()
        if not line or line[0] in ";/":
            continue
        if line[0] == "[":
            sec = None
            continue
        if line[0] == "#":
            u = line.upper()
            if u.startswith("#IF"):
                sec = "if"
            elif u.startswith("#ACT"):
                sec = "act"
            elif u.startswith("#ELSEACT"):
                sec = "else"
            elif u.startswith(("#SAY", "#ELSESAY")):
                sec = None
            continue
        if sec:
            out[line.split()[0].split("\t")[0].upper()] += 1
    return out


for name, pred in GROUPS.items():
    sel = [p for p in files if pred(p)]
    cmds = collections.Counter()
    for p in sel:
        cmds.update(commands_of(p))
    top = ", ".join(f"{c}×{n}" for c, n in cmds.most_common(14))
    print(f"{name}: 文件 {len(sel)}，命令 {len(cmds)} 种")
    print(f"   {top}")
