#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""卡面/文档引用的 probe 场景必须真的在 `run3-scenarios.json` 里（卡 `openmir2-scenario-manifest-gate`）。

背景：`softclose_relogin` 2026-10-25 就合入了（`e57024dbc`），但 `run3-scenarios.json` 里从来没有它
⇒ 场景写了却从不进批次，任何窗口都跑不到，**没有任何门会发现**。本工具就是那道门。

## 判据（定稿于卡上 comment `26644619` / `faf42983`）

| 档 | 条件 | 结果 |
|---|---|---|
| **RED** | 卡面（issue 的 `acceptance`、任何 body 里键名含 `window` 的字符串）引用 + 文件存在 + 不在 manifest + 无有效豁免 | rc=1，点名卡 id + 场景名 |
| **RED** | `run3-scenarios.json` 里的名字**没有对应文件** | rc=1（真 bug） |
| **UNDECIDABLE** | **卡面**引用到、但 `tools/probe/scenarios/<name>.json` 不存在（且无有效豁免） | **rc=2 判不了**（工具分不清「待建占位」与「已删但卡面没说」） |
| **STRUCT_EXEMPT** | 该名字被 manifest 的某个场景以 `--discovery-scenario` 引用 ⇒ 本来就不该单独进批次 | 过（自动判定） |
| **MANUAL_EXEMPT** | 豁免登记表里 `not_in_run3_reason` 带**卡 id + 行号**，且该卡 id 在 collab 线程里存在 | 过，且进 `--audit-exempt` 清单 |
| **INFO** | 仅 `docs/*.md` 散文引用（无论文件在不在） | 只列不判 |

**豁免通道自己也要有牙**：无卡 id / 卡 id 不存在 / 无行号 ⇒ 该豁免无效 ⇒ 回落到 RED 或 UNDECIDABLE。

## 实现约束（性能）

collab 侧有 **11,469** 个 inbox ref。逐个 `git cat-file -p` 跑不完（原型第一版 5 分钟未结束）
⇒ 必须用 `git cat-file --batch` 一次喂入。

## 用法

```sh
python -I -B tools/hygiene/scenario_manifest_gate.py                 # 判定（0 绿 / 1 红 / 2 判不了）
python -I -B tools/hygiene/scenario_manifest_gate.py --audit-exempt  # 列出全部豁免供人复核
python -I -B tools/hygiene/scenario_manifest_gate.py --selftest      # 合成夹具自测
python -I -B tools/hygiene/scenario_manifest_gate.py --json          # 机读输出
```
"""
import json
import os
import re
import subprocess
import sys

# 卡 `openmir2-entry-shape-dedupe`：两态兼容只留一处（`tools/collab/entryrefs.py`）
sys.path.insert(0, os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "collab"))
from entryrefs import EntryRefError, entry_of  # noqa: E402

REPO = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
MANIFEST_REL = "tools/window/run3-scenarios.json"
SCEN_DIR_REL = "tools/probe/scenarios"
DOCS_REL = "docs"
EXEMPT_REL = "tools/hygiene/scenario_manifest_exempt.toml"

#: 场景路径引用（卡/文档里都长这样）
REF_RE = re.compile(r"tools/probe/scenarios/([A-Za-z0-9_\-]+)\.json")
#: manifest `extra` 里的 discovery 引用
DISC_FLAG = "--discovery-scenario"


def _strings(value):
    """递归收集 str（list/dict 都下钻）——`acceptance` 是数组、`window*` 是 list/dict 都要认。"""
    out = []
    if isinstance(value, str):
        out.append(value)
    elif isinstance(value, (list, tuple)):
        for v in value:
            out.extend(_strings(v))
    elif isinstance(value, dict):
        for v in value.values():
            out.extend(_strings(v))
    return out


def _card_surface_strings(body):
    """卡面条目里视为「窗口项文本」的键：`acceptance`（dict **或** list）与任何键名含 window 的值
    （str / list / dict 四种形态都遍历）。"""
    if not isinstance(body, dict):
        return []
    out = _strings(body.get("acceptance"))
    for k, v in body.items():
        if "window" in str(k).lower():
            out.extend(_strings(v))
    return out


def refs_from_cards(entries):
    """entries: [(thread_id, kind, body)] -> {scenario: {thread_id, ...}}（仅已合并卡）。"""
    merged = set()
    for tid, kind, body in entries:
        if kind == "merge_result" and isinstance(body, dict) and body.get("merged"):
            merged.add(tid)
    out = {}
    for tid, _kind, body in entries:
        if tid not in merged:
            continue
        for text in _card_surface_strings(body):
            for name in REF_RE.findall(text):
                out.setdefault(name, set()).add(tid)
    return out


def refs_from_docs(docs):
    """docs: {path: text} -> {scenario: {path, ...}}。"""
    out = {}
    for path, text in docs.items():
        for name in REF_RE.findall(text):
            out.setdefault(name, set()).add(path)
    return out


def discovery_names(manifest):
    out = set()
    for sc in manifest.get("scenarios") or []:
        extra = sc.get("extra") or []
        for i, tok in enumerate(extra):
            if tok == DISC_FLAG and i + 1 < len(extra):
                for name in REF_RE.findall(str(extra[i + 1])):
                    out.add(name)
    return out


def symmetric_diff(files, manifest_names, discovery, card_refs, doc_refs):
    """A4：`scenarios/*.json` 与 manifest 的**对称差**（两个方向都要点名）。

    * `file_without_manifest`：有文件、不在 manifest（含未被任何来源引用的孤儿）；
    * `manifest_without_file`：manifest 有名字、文件不存在 —— **真 bug**（evaluate 里判 RED）。
    """
    manifest_names = set(manifest_names)
    referenced = set(card_refs or {}) | set(doc_refs or {}) | set(discovery or {})
    return {
        "file_without_manifest": sorted(set(files) - manifest_names),
        "file_without_manifest_unreferenced": sorted(set(files) - manifest_names - referenced),
        "manifest_without_file": sorted(manifest_names - set(files)),
    }


UNDECIDABLE_REASON = "被引用但 tools/probe/scenarios/<name>.json 不存在（工具分不清待建/已删）"
RED_MISSING_MANIFEST = "被引用且文件存在，但不在 run3-scenarios.json，且无有效豁免"


def evaluate(manifest_names, discovery, files, card_refs, doc_refs, exempt, known_cards):
    """纯判定。返回 (rc, red, undecidable, info, struct_ok, manual_ok, bad_exempt)。

    规则见模块头；这里不碰 IO，便于自测与变异。
    """
    red, undec, info, struct_ok, manual_ok, bad_exempt = [], [], [], [], [], []
    referenced = {}
    for name, cards in card_refs.items():
        referenced.setdefault(name, {})["cards"] = sorted(cards)
    for name, docs in doc_refs.items():
        referenced.setdefault(name, {}).setdefault("docs", []).extend(sorted(docs))

    # `manifest_without_file` 是真 bug
    for name in sorted(set(manifest_names) - set(files)):
        red.append({"scenario": name, "why": "manifest_without_file",
                    "detail": "run3-scenarios.json 里有它，但场景文件不存在"})

    manual_names = set()
    for name, meta in (exempt or {}).items():
        card = str((meta or {}).get("card") or "").strip()
        line = str((meta or {}).get("line") or "").strip()
        reason = str((meta or {}).get("not_in_run3_reason") or "").strip()
        if not card or not line or not reason:
            bad_exempt.append({"scenario": name,
                               "why": "豁免必须有 card + line + not_in_run3_reason"})
            continue
        if card not in known_cards:
            bad_exempt.append({"scenario": name,
                               "why": "豁免引用的卡 `%s` 在 collab 线程里不存在" % card})
            continue
        manual_names.add(name)
        manual_ok.append({"scenario": name, "card": card, "line": line,
                          "not_in_run3_reason": reason})

    for name in sorted(referenced):
        ref = referenced[name]
        in_manifest = name in manifest_names
        exists = name in files
        if not exists:
            if name in manual_names:
                continue  # 豁免覆盖「不存在」：merged 卡 body 不可改，只能这样放行
            if ref.get("cards"):
                # **卡面声称可用、但文件不存在** ⇒ 判不了（工具分不清待建占位与已删）
                undec.append({"scenario": name, "cards": ref["cards"],
                              "docs": ref.get("docs", []), "why": UNDECIDABLE_REASON})
            else:
                # **仅 docs 散文提到** ⇒ INFO（历史记录里写着「待建 / 已删」很常见，
                # 拿它让合并门永久 rc=2 会把「判据在、但没人跑」换成「门自己长红」）
                info.append({"scenario": name, "docs": ref.get("docs", []),
                             "why": "docs 散文引用，但文件不存在（历史记录/占位，不判）"})
            continue
        if in_manifest:
            if ref.get("docs") and not ref.get("cards"):
                info.append({"scenario": name, "docs": ref["docs"],
                             "why": "docs 散文引用，已在 manifest"})
            continue
        if name in discovery:
            struct_ok.append({"scenario": name,
                              "why": "discovery-only：被 manifest 的 --discovery-scenario 引用"})
            continue
        if name in manual_names:
            continue
        red.append({"scenario": name, "why": RED_MISSING_MANIFEST,
                    "cards": ref.get("cards", []), "docs": ref.get("docs", [])})

    # 登记了但没人引用也没进 manifest 的豁免：登记腐朽
    for name in sorted(manual_names):
        if name not in referenced and name not in manifest_names:
            bad_exempt.append({"scenario": name,
                               "why": "豁免登记了，但没有任何来源引用它（腐朽登记）"})

    # **过期豁免**（Leader 派单 `2806dc8b` 的第 ① 条）：豁免所理由消失时必须被点名
    for name in sorted(manual_names):
        meta = (exempt or {}).get(name) or {}
        if name in manifest_names:
            bad_exempt.append({"scenario": name,
                               "why": "过期豁免：该场景**已进 manifest** ⇒ 豁免应删除"})
            continue
        if str(meta.get("covers") or "") == "missing_file" and name in files:
            bad_exempt.append({"scenario": name,
                               "why": "过期豁免：登记理由是「文件不存在」，但文件**已出现** ⇒ 豁免不再适用，按常规规则重判"})

    if red or bad_exempt:
        rc = 1
    elif undec:
        rc = 2
    else:
        rc = 0
    return rc, red, undec, info, struct_ok, manual_ok, bad_exempt


# ---------------------------------------------------------------- IO 层

def _run(args, cwd=None):
    return subprocess.run(args, cwd=cwd or REPO, capture_output=True, text=True,
                          encoding="utf-8", errors="replace")


def read_manifest(repo):
    with open(os.path.join(repo, MANIFEST_REL), encoding="utf-8") as fh:
        return json.load(fh)


def read_files(repo):
    d = os.path.join(repo, SCEN_DIR_REL)
    return {fn[:-5] for fn in os.listdir(d) if fn.endswith(".json")}


def read_docs(repo):
    d = os.path.join(repo, DOCS_REL)
    out = {}
    for fn in sorted(os.listdir(d)):
        if fn.endswith(".md"):
            with open(os.path.join(d, fn), encoding="utf-8", errors="replace") as fh:
                out[fn] = fh.read()
    return out


def read_collab(repo, stats=None):
    """`git cat-file --batch` 一次喂入全部 inbox ref（11k+ 个；逐个 `-p` 跑不完）。

    形状口径（卡 `openmir2-entry-shape-consumer-followups` A3，**显式登记**；判定口径本身不变）：

      * 扁平 blob（inbox）与 `{entry: …}` 包层（thread 视图）都由 `entry_of` 统一判定；
      * **id-only**（扁平、有 `id` 无 `kind`）⇒ `entry_of` 抛 `EntryRefError` ⇒ 计
        `stats["shape_entry_of"]` 并跳过。团队口径：这类形状在 11,661 条真实语料里 **0 命中**，
        跳过等于「这条不是条目」，不红、不 rc=2；
      * **wrapper-inner-no-id**（包层在、内层缺 `id`）⇒ `entry_of` 正常返回内层，随后
        `if not tid` 计 `stats["shape_no_id"]` 并跳过；
      * JSON 解不开 ⇒ 计 `stats["bad_json"]` 并跳过（与旧行为一致）；
      * **只有** `EntryRefError` 会被吞：`entry_of` 抛其它异常（真 bug / 上游契约破坏）必须冒出去，
        不许当成「不是条目」——牙见 `selftest` 的接缝③。

    `stats` 是可选的累加 dict（只给自测与诊断用）；不传时行为与旧版逐条相同。
    """
    if stats is None:
        stats = {}
    for _k in ("shape_entry_of", "shape_no_id", "bad_json"):
        stats.setdefault(_k, 0)
    r = _run(["git", "for-each-ref", "--format=%(refname)", "refs/collab/inbox/"], cwd=repo)
    if r.returncode != 0:
        raise RuntimeError("git for-each-ref 失败：%s" % (r.stderr or "").strip())
    refs = [x for x in r.stdout.split("\n") if x.strip()]
    if not refs:
        return [], set(), 0
    p = subprocess.Popen(["git", "cat-file", "--batch"], cwd=repo, stdin=subprocess.PIPE,
                         stdout=subprocess.PIPE)
    out, _ = p.communicate("".join(x + "\n" for x in refs).encode())
    entries, known = [], set()
    i, n = 0, len(out)
    while i < n:
        nl = out.find(b"\n", i)
        if nl < 0:
            break
        parts = out[i:nl].decode("utf-8", "replace").split()
        if len(parts) != 3 or parts[1] != "blob":
            i = nl + 1
            continue
        size = int(parts[2])
        body = out[nl + 1:nl + 1 + size]
        i = nl + 1 + size + 1
        try:
            doc = json.loads(body.decode("utf-8", "replace"))
        except Exception:
            stats["bad_json"] += 1
            continue
        # **接缝（review C1）**：collab blob 是**扁平**的（顶层 actor/body/id/kind/parent/…），
        # 没有 `entry` 包层；只有 `walgit collab thread` 的**视图**才有。
        # 两态兼容统一走 `tools/collab/entryrefs.entry_of`（卡 `openmir2-entry-shape-dedupe`）。
        # 卡 `openmir2-entry-shape-consumer-followups` A3：两条人工形态的语义在此**显式**落地并计数
        # （口径不变，仍然是「跳过」）——
        #   ① id-only（有 id 无 kind）⇒ entry_of 抛 EntryRefError ⇒ 记 shape_entry_of 跳过；
        #   ② wrapper-inner-no-id（包层在、内层缺 id）⇒ entry_of 返回内层 ⇒ 记 shape_no_id 跳过。
        # 只吞 EntryRefError：其它异常是真 bug，必须冒出去（selftest 接缝③）。
        try:
            ent = entry_of(doc)
        except EntryRefError:
            stats["shape_entry_of"] += 1
            continue
        tid = ent.get("id")
        if not tid:
            stats["shape_no_id"] += 1
            continue
        known.add(tid)
        entries.append((tid, ent.get("kind"), ent.get("body") or {}))
    return entries, known, len(refs)


def read_exempt(repo):
    path = os.path.join(repo, EXEMPT_REL)
    if not os.path.isfile(path):
        return {}
    try:
        import tomllib
        with open(path, "rb") as fh:
            doc = tomllib.load(fh)
    except Exception as e:
        raise RuntimeError("豁免表解析失败 %s：%s" % (EXEMPT_REL, e))
    out = {}
    for row in doc.get("exempt") or []:
        out[row["scenario"]] = row
    return out


def collect(repo):
    man = read_manifest(repo)
    manifest_names = {s.get("name") for s in (man.get("scenarios") or []) if s.get("name")}
    entries, known, n_refs = read_collab(repo)
    return {
        "refs_scanned": n_refs,
        "manifest_names": manifest_names,
        "discovery": discovery_names(man),
        "files": read_files(repo),
        "card_refs": refs_from_cards(entries),
        "doc_refs": refs_from_docs(read_docs(repo)),
        "exempt": read_exempt(repo),
        "known_cards": known,
    }


def render(rc, red, undec, info, struct_ok, manual_ok, bad_exempt, scanned=None):
    print("scenario-manifest gate：RED %d / UNDECIDABLE %d / STRUCT_EXEMPT %d / MANUAL_EXEMPT %d / INFO %d"
          % (len(red), len(undec), len(struct_ok), len(manual_ok), len(info)))
    print("豁免计数：STRUCT %d / MANUAL %d / BAD %d（BAD 含过期/腐朽/无效）"
          % (len(struct_ok), len(manual_ok), len(bad_exempt)))
    if scanned is not None:
        print("扫描：collab refs %d 条（cat-file --batch 一次喂入）/ 场景文件 %d / manifest %d"
              % (scanned["refs"], scanned["files"], scanned["manifest"]))
    for r in red:
        print("RED      %-28s %s%s" % (r["scenario"], r["why"],
                                       ("  cards=" + ",".join(r.get("cards") or []))
                                       if r.get("cards") else ""))
    for r in bad_exempt:
        print("RED      %-28s %s" % (r["scenario"], r["why"]))
    for r in undec:
        print("UNDECID  %-28s %s  cards=%s docs=%s" % (
            r["scenario"], r["why"], ",".join(r.get("cards") or []) or "-",
            ",".join(r.get("docs") or []) or "-"))
    for r in struct_ok:
        print("STRUCT   %-28s %s" % (r["scenario"], r["why"]))
    for r in manual_ok:
        print("MANUAL   %-28s card=%s line=%s" % (r["scenario"], r["card"], r["line"]))
    for r in info:
        print("INFO     %-28s docs=%s" % (r["scenario"], ",".join(r.get("docs") or [])))
    print("RESULT: %s" % {0: "GREEN", 1: "RED", 2: "UNDECIDABLE"}[rc])


def main(argv):
    if "--selftest" in argv:
        return selftest()
    repo = REPO
    if "--repo" in argv:
        repo = argv[argv.index("--repo") + 1]
    try:
        data = collect(repo)
    except Exception as e:
        print("scenario-manifest gate：rc=2 取数失败：%s" % e, file=sys.stderr)
        return 2
    ev = dict(data)
    ev.pop("refs_scanned", None)
    rc, red, undec, info, struct_ok, manual_ok, bad_exempt = evaluate(**ev)
    sym = symmetric_diff(data["files"], data["manifest_names"], data["discovery"],
                         data["card_refs"], data["doc_refs"])
    if "--audit-exempt" in argv:
        print("STRUCT_EXEMPT（自动判定，不需人工理由）：")
        for r in struct_ok:
            print("  %-28s %s" % (r["scenario"], r["why"]))
        print("MANUAL_EXEMPT（须卡 id + 行号，供人复核）：")
        for r in manual_ok:
            print("  %-28s card=%s line=%s reason=%s" % (
                r["scenario"], r["card"], r["line"], r["not_in_run3_reason"]))
        for r in bad_exempt:
            print("  !! %-25s %s" % (r["scenario"], r["why"]))
        print("豁免计数：STRUCT %d / MANUAL %d / BAD %d"
              % (len(struct_ok), len(manual_ok), len(bad_exempt)))
        return 1 if bad_exempt else 0
    if "--json" in argv:
        print(json.dumps({"rc": rc, "red": red, "undecidable": undec, "info": info,
                          "struct_exempt": struct_ok, "manual_exempt": manual_ok,
                          "bad_exempt": bad_exempt, "symmetric": sym},
                         ensure_ascii=False, indent=2))
    else:
        render(rc, red, undec, info, struct_ok, manual_ok, bad_exempt,
               {"refs": data["refs_scanned"], "files": len(data["files"]),
                "manifest": len(data["manifest_names"])})
        print("对称差：file_without_manifest %d（其中未被任何来源引用 %d）/ manifest_without_file %d"
              % (len(sym["file_without_manifest"]), len(sym["file_without_manifest_unreferenced"]),
                 len(sym["manifest_without_file"])))
    return rc


# ---------------------------------------------------------------- selftest

def selftest():
    """合成夹具（纯 dict，不碰 git/网络）。规则 19：每条退化输入都点名。"""
    ok = []

    def check(name, cond, detail=""):
        ok.append((name, bool(cond), detail))

    base = dict(manifest_names={"m1"}, discovery={"disc"}, files={"m1", "disc", "orphan"},
                card_refs={}, doc_refs={}, exempt={}, known_cards={"card-a"})

    # 1) 全绿：无引用
    rc, red, undec, info, st, man, bad = evaluate(**base)
    check("空引用 ⇒ GREEN", rc == 0 and not red and not undec, (rc, red, undec))

    # 2) 卡面引用 + 文件存在 + 不在 manifest ⇒ RED
    d = dict(base, card_refs={"orphan": {"card-a"}})
    rc, red, *_ = evaluate(**d)
    check("卡面引用未入 manifest ⇒ RED", rc == 1 and red and red[0]["scenario"] == "orphan", red)

    # 3) 同一条被 manifest 收录 ⇒ GREEN
    d = dict(base, card_refs={"orphan": {"card-a"}}, manifest_names={"m1", "orphan"})
    rc, red, undec, *_ = evaluate(**d)
    check("收录后 ⇒ GREEN", rc == 0 and not red, (rc, red))

    # 4) discovery-only ⇒ STRUCT_EXEMPT（自动，不用理由）
    d = dict(base, card_refs={"disc": {"card-a"}})
    rc, red, undec, info, st, *_ = evaluate(**d)
    check("discovery-only ⇒ STRUCT", rc == 0 and len(st) == 1 and st[0]["scenario"] == "disc", (rc, st))

    # 5) **卡面**引用到不存在的文件 ⇒ UNDECIDABLE（rc=2，不红不绿）
    d = dict(base, card_refs={"ghosty": {"card-a"}})
    rc, red, undec, *_ = evaluate(**d)
    check("卡面引用到不存在的文件 ⇒ rc=2", rc == 2 and undec and not red, (rc, red, undec))

    # 5b) **仅 docs** 引用到不存在的文件 ⇒ INFO（不让合并门永久 rc=2）
    d = dict(base, doc_refs={"gone": {"d.md"}})
    rc, red, undec, info, *_ = evaluate(**d)
    check("docs 引用到不存在的文件 ⇒ INFO", rc == 0 and not undec and len(info) == 1,
          (rc, undec, info))

    # 6) manifest 有名字但文件不存在 ⇒ RED（真 bug）
    d = dict(base, manifest_names={"m1", "vanished"})
    rc, red, *_ = evaluate(**d)
    check("manifest_without_file ⇒ RED", rc == 1 and red and red[0]["why"] == "manifest_without_file", red)

    # 7) 人工豁免：卡 + 行号 + 理由齐全且卡存在 ⇒ 过 + 进审计
    d = dict(base, card_refs={"orphan": {"card-a"}},
             exempt={"orphan": {"card": "card-a", "line": "docs/x.md:12",
                                "not_in_run3_reason": "结构性不可判"}})
    rc, red, undec, info, st, man, bad = evaluate(**d)
    check("有效豁免 ⇒ 过并进审计", rc == 0 and len(man) == 1 and not bad, (rc, man, bad))

    # 8) 豁免缺卡 id ⇒ 红（软化通道自己要有牙）
    d = dict(base, card_refs={"orphan": {"card-a"}},
             exempt={"orphan": {"card": "", "line": "docs/x.md:12",
                                "not_in_run3_reason": "暂不需要"}})
    rc, red, undec, info, st, man, bad = evaluate(**d)
    check("豁免缺卡 id ⇒ RED", rc == 1 and bad, (rc, bad))

    # 9) 豁免缺行号 ⇒ 红
    d = dict(base, card_refs={"orphan": {"card-a"}},
             exempt={"orphan": {"card": "card-a", "line": "",
                                "not_in_run3_reason": "理由"}})
    rc, red, undec, info, st, man, bad = evaluate(**d)
    check("豁免缺行号 ⇒ RED", rc == 1 and bad, (rc, bad))

    # 10) 豁免引用的卡不存在 ⇒ 红
    d = dict(base, card_refs={"orphan": {"card-a"}},
             exempt={"orphan": {"card": "no-such-card", "line": "docs/x.md:12",
                                "not_in_run3_reason": "理由"}})
    rc, red, undec, info, st, man, bad = evaluate(**d)
    check("豁免卡不存在 ⇒ RED", rc == 1 and bad, (rc, bad))

    # 11) 腐朽豁免：登记了但没人引用、也不在 manifest ⇒ 红
    d = dict(base, exempt={"orphan": {"card": "card-a", "line": "docs/x.md:1",
                                     "not_in_run3_reason": "理由"}})
    rc, red, undec, info, st, man, bad = evaluate(**d)
    check("腐朽豁免 ⇒ RED", rc == 1 and bad, (rc, bad))

    # 12) docs-only 引用且已在 manifest ⇒ INFO（只列不判）
    d = dict(base, doc_refs={"m1": {"d.md"}})
    rc, red, undec, info, *_ = evaluate(**d)
    check("docs 引用 + 已在 manifest ⇒ INFO", rc == 0 and len(info) == 1, (rc, info))

    # 13) docs-only 引用 + 文件存在 + 不在 manifest ⇒ RED（文档说可用也是可核声明）
    d = dict(base, doc_refs={"orphan": {"d.md"}})
    rc, red, *_ = evaluate(**d)
    check("docs 引用未入 manifest ⇒ RED", rc == 1 and red, (rc, red))

    # 14) 退化：manifest 为空 + 无引用 ⇒ GREEN（不是 rc=2）
    d = dict(manifest_names=set(), discovery=set(), files=set(), card_refs={}, doc_refs={},
             exempt={}, known_cards=set())
    rc, red, undec, *_ = evaluate(**d)
    check("全空 ⇒ GREEN（不是判不了）", rc == 0, rc)

    # 15) 卡面引用只从「已合并卡」取：未合并的引用不算（refs_from_cards）
    entries = [("card-a", "issue", {"acceptance": {"A1": "见 tools/probe/scenarios/x1.json"}}),
               ("card-a", "merge_result", {"merged": True}),
               ("card-b", "issue", {"acceptance": {"A1": "见 tools/probe/scenarios/x2.json"}})]
    r = refs_from_cards(entries)
    check("只取已合并卡的引用", set(r) == {"x1"}, r)

    # 16) 卡面只扫 acceptance 与 window* 键：普通正文里的引用不算
    entries = [("card-c", "merge_result", {"merged": True}),
               ("card-c", "comment", {"note": "tools/probe/scenarios/nope.json"}),
               ("card-c", "issue", {"acceptance": {"A2": "tools/probe/scenarios/yes.json"},
                                    "window_items": "tools/probe/scenarios/win.json"})]
    r = refs_from_cards(entries)
    check("卡面 = acceptance + window* 键", set(r) == {"yes", "win"}, r)

    # 17) refs 正则拒绝路径穿越/怪名（只认 [A-Za-z0-9_-]）
    r = refs_from_docs({"d.md": "tools/probe/scenarios/a_b-c.json 和 tools/probe/scenarios/../x.json"})
    check("只认合法场景名", set(r) == {"a_b-c"}, r)

    # 18) 对称差：两个方向都要点名；未引用的孤儿单独计数
    sym = symmetric_diff({"a", "b"}, {"a"}, set(), {}, {})
    check("对称差 file_without_manifest", sym["file_without_manifest"] == ["b"], sym)
    check("对称差孤儿 = 未被引用者", sym["file_without_manifest_unreferenced"] == ["b"], sym)
    sym = symmetric_diff({"a"}, {"a", "zz"}, set(), {}, {})
    check("对称差 manifest_without_file", sym["manifest_without_file"] == ["zz"], sym)

    # 19) 过期豁免 A：场景已进 manifest ⇒ 豁免应删除（rc=1）
    d = dict(base, manifest_names={"m1", "orphan"}, card_refs={"orphan": {"card-a"}},
             exempt={"orphan": {"card": "card-a", "line": "x:1", "not_in_run3_reason": "r"}})
    rc, red, undec, info, st, man, bad = evaluate(**d)
    check("过期豁免（已入批）⇒ RED", rc == 1 and any("已进 manifest" in b["why"] for b in bad), bad)

    # 20) 过期豁免 B：`covers=missing_file` 但文件已出现 ⇒ rc=1
    d = dict(base, doc_refs={"late": {"d.md"}},
             exempt={"late": {"card": "card-a", "line": "x:1",
                              "not_in_run3_reason": "文件待建", "covers": "missing_file"}},
             files={"m1", "disc", "orphan", "late"})
    rc, red, undec, info, st, man, bad = evaluate(**d)
    check("过期豁免（文件已出现）⇒ RED",
          rc == 1 and any("已出现" in b["why"] for b in bad), bad)

    # 21) 同一豁免在文件仍缺失时**不是**过期（covers=missing_file 正常生效）
    d = dict(base, card_refs={"late": {"card-a"}},
             exempt={"late": {"card": "card-a", "line": "x:1",
                              "not_in_run3_reason": "文件待建", "covers": "missing_file"}})
    rc, red, undec, info, st, man, bad = evaluate(**d)
    check("缺文件 + 有效豁免 ⇒ 过", rc == 0 and not bad, (rc, bad))

    # 22) acceptance 可以是**数组**（review C2）：list 里的引用必须被看见
    entries = [("card-d", "merge_result", {"merged": True}),
               ("card-d", "issue", {"acceptance": ["a: tools/probe/scenarios/acc1.json",
                                                   "b: tools/probe/scenarios/acc2.json"]})]
    r = refs_from_cards(entries)
    check("acceptance 数组 ⇒ 两个名字都取到", set(r) == {"acc1", "acc2"}, r)

    # 23) window* 的四种值形态都要遍历（review C3）：str / list / dict / 键名 window_required_items
    entries = [("card-e", "merge_result", {"merged": True}),
               ("card-e", "issue", {
                   "acceptance": {},
                   "window_items": "tools/probe/scenarios/w_str.json",
                   "window_missing": ["tools/probe/scenarios/w_list.json"],
                   "window_item": {"k": "tools/probe/scenarios/w_dict.json"},
                   "window_required_items": {"k2": ["tools/probe/scenarios/w_nested.json"]},
                   "notes": "tools/probe/scenarios/w_ignored.json"})]
    r = refs_from_cards(entries)
    check("window* 四种形态都遍历、非 window 键不扫",
          set(r) == {"w_str", "w_list", "w_dict", "w_nested"}, r)

    # 24) **读入接缝（review C1）**：合成一张**扁平** collab blob（顶层 kind/body/id），
    #     走真正的 git 路径（hash-object -w + update-ref + cat-file --batch）⇒ kind/body 必须非空、
    #     refs_from_cards 必须取到名字。24 条自测此前全绕过了这一步。
    import tempfile
    import shutil
    tmp = tempfile.mkdtemp(prefix="smg-seam-")
    env = {k: v for k, v in os.environ.items()
           if k not in ("GIT_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE")}
    try:
        subprocess.run(["git", "init", "-q", tmp], check=True, env=env)
        def _put(ref, obj):
            blob = json.dumps(obj).encode()
            h = subprocess.run(["git", "-C", tmp, "hash-object", "-w", "--stdin"],
                               input=blob, capture_output=True, check=True, env=env).stdout
            subprocess.run(["git", "-C", tmp, "update-ref", ref, h.decode().strip()],
                           check=True, env=env)
        _put("refs/collab/inbox/p/card-x-m1",
             {"version": 1, "kind": "merge_result", "id": "card-x", "actor": "p",
              "parent": "", "sig": "", "ts": 0, "body": {"merged": True}})
        _put("refs/collab/inbox/p/card-x-i1",
             {"version": 1, "kind": "issue", "id": "card-x", "actor": "p",
              "parent": "", "sig": "", "ts": 1,
              "body": {"acceptance": ["see tools/probe/scenarios/fromcard.json"]}})
        # 形状③：thread 视图**包层**（`{"entry": …}`）也必须解出同一份 entry
        _put("refs/collab/inbox/p/card-x-w1",
             {"entry": {"version": 1, "kind": "status", "id": "card-x", "actor": "p",
                        "parent": "", "sig": "", "ts": 2, "body": {"status": "in-progress"}},
              "oid": "f" * 40, "principal": "p", "verified": True})
        got, known2, n2 = read_collab(tmp)
        kinds = {k for _t, k, _b in got}
        bodies = [b for _t, _k, b in got if b]
        check("接缝：扁平 blob 解出 kind", kinds == {"merge_result", "issue", "status"}, kinds)
        check("接缝：扁平 blob 解出 body", all(b for b in bodies) and len(bodies) == 3, bodies)
        check("接缝：refs_from_cards 能取到名字",
              set(refs_from_cards(got)) == {"fromcard"}, refs_from_cards(got))
        check("接缝：扫描条数 = 3（含包层 blob）", n2 == 3, n2)
    finally:
        shutil.rmtree(tmp, ignore_errors=True)

    # A3（卡 `openmir2-entry-shape-consumer-followups`）：两条**人工形态**的显式语义 + 接缝牙。
    # 用第二个临时仓库，避免动上面那组「扫描条数 = 3」的断言。
    tmp2 = tempfile.mkdtemp(prefix="smg-shape-")
    try:
        subprocess.run(["git", "init", "-q", tmp2], check=True, env=env)

        def _put2(ref, obj):
            blob = json.dumps(obj).encode()
            h = subprocess.run(["git", "-C", tmp2, "hash-object", "-w", "--stdin"],
                               input=blob, capture_output=True, check=True, env=env).stdout
            subprocess.run(["git", "-C", tmp2, "update-ref", ref, h.decode().strip()],
                           check=True, env=env)

        # ① id-only：扁平、有 id 无 kind
        _put2("refs/collab/inbox/p/card-y-1", {"id": "card-y", "actor": "p", "ts": 3})
        # ② wrapper-inner-no-id：包层在、内层缺 id
        _put2("refs/collab/inbox/p/card-z-1",
              {"entry": {"version": 1, "kind": "status", "actor": "p", "body": {}},
               "oid": "e" * 40, "principal": "p", "verified": True})
        stats = {}
        got2, known2b, n2b = read_collab(tmp2, stats=stats)
        check("形状①：id-only（有 id 无 kind）⇒ 跳过并计 shape_entry_of",
              got2 == [] and known2b == set() and stats.get("shape_entry_of") == 1,
              (got2, stats))
        check("形状②：wrapper-inner-no-id ⇒ 取到内层但缺 id ⇒ 跳过并计 shape_no_id",
              stats.get("shape_no_id") == 1, stats)
        check("形状：两条都被跳过，扫描条数仍是 2（计数不混）", n2b == 2 and stats.get("bad_json") == 0,
              (n2b, stats))

        # ③ 接缝：entry_of 抛**非 EntryRefError** ⇒ 必须冒出去（不能被「不是条目」吞掉）
        _orig_entry_of = globals()["entry_of"]

        def _boom(_doc):
            raise RuntimeError("boom-from-entry_of")

        globals()["entry_of"] = _boom
        try:
            read_collab(tmp2)
            seam_ok = False
        except RuntimeError as ex:
            seam_ok = "boom-from-entry_of" in str(ex)
        finally:
            globals()["entry_of"] = _orig_entry_of
        check("接缝③：entry_of 抛非 EntryRefError ⇒ 冒出去（不当作正常跳过）", seam_ok)
    finally:
        shutil.rmtree(tmp2, ignore_errors=True)

    fails = [(n, d) for n, good, d in ok if not good]
    for n, good, d in ok:
        print("%s %s%s" % ("PASS" if good else "FAIL", n, "" if good else "  %r" % (d,)))
    print("SELFTEST %s（%d 条）" % ("PASS" if not fails else "FAIL", len(ok)))
    return 0 if not fails else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
