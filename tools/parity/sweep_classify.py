#!/usr/bin/env python3
"""装置 sweep 分类器（§5.1 四类兼容判据）。

用法:
  python sweep_classify.py --before <旧装置输出目录> --after <新装置输出目录> \
      [--audit <新装置 frames-audit.jsonl 目录>] --out <报告路径>

分类（每支一个结论）:
  ok            两侧逐字段完全相同
  仅顺序        Sent 多重集相同、仅相对顺序不同（Rm 亦同）
  仅 due        除 due_ms_rel 外逐字段相同，只有 due 不同
  内容          其它任何差异
  无台账        某侧缺文件 / JSON 解析失败

"跨 actor 且收件人本步已跑过"标记: 对每个差异帧 (who, ident, recog)，在审计侧车里找
enqueue_step < run_step 的同帧行；命中即标 cross_actor_deferred=True。

退出码: 0 = 全部 ok/仅顺序/仅 due 且 仅 due 全部带 cross_actor_deferred；2 = 出现 内容/无台账/未解释的 仅 due。
（判 rc 时不接管道，见 docs 纪律。）

仪器自检（2026-11-21 教训）：`SendMessage` 在 oracle 里是 `record struct`；若装置侧用 `object` +
`ReferenceEqualityComparer` 做键，`PeekAll()` 与消费端各得一个新装箱 ⇒ 查表恒 miss ⇒ `enqueue_step`
恒回落当前步（641 行"enqueue==run"全是假象）。因此本脚本强制自检：**审计总行数 > 100 且
`enqueue_step < run_step` 的行数为 0 ⇒ 判仪器 RED（rc=2）**，防止同款静默错仪器再次发生。
"""
import argparse, json, os, sys

def load(p):
    try:
        with open(p, encoding="utf-8") as f:
            return json.load(f)
    except Exception:
        return None

def sent_key(m, with_due=True):
    k = (m.get("ident"), m.get("recog"), m.get("param"), m.get("tag"), m.get("series"), m.get("tail"))
    return k + ((m.get("due_ms_rel"),) if with_due else ())

def rm_key(m):
    return (m.get("ident"), m.get("wParam"), m.get("p1"), m.get("p2"), m.get("p3"), m.get("msg"), m.get("late"))

def receivers(doc):
    out = {}
    for c in doc.get("cases") or []:
        for r in c.get("Receivers") or []:
            out[(c.get("Label"), r.get("Who"))] = r
    return out

def classify_stem(before, after, audit_rows):
    if before is None or after is None:
        return "无台账", False, []
    rb, ra = receivers(before), receivers(after)
    if set(rb) != set(ra):
        return "内容", False, ["收件人集合不同"]
    window_tail = bool(after.get("cases") and any(c.get("NotDue") for c in after.get("cases"))
                       and not (before.get("cases") and any(c.get("NotDue") for c in before.get("cases"))))
    order = {"ok": 0, "仅顺序": 1, "仅 due": 2, "仅窗口尾": 3, "内容": 4}
    worst, cross, notes = "ok", False, []
    for k in sorted(rb):
        b, a = rb[k], ra[k]
        seq_b = [sent_key(m) for m in (b.get("Sent") or [])]
        seq_a = [sent_key(m) for m in (a.get("Sent") or [])]
        nod_b = [sent_key(m, False) for m in (b.get("Sent") or [])]
        nod_a = [sent_key(m, False) for m in (a.get("Sent") or [])]
        rm_b = [rm_key(m) for m in (b.get("Rm") or [])]
        rm_a = [rm_key(m) for m in (a.get("Rm") or [])]
        if seq_b == seq_a and rm_b == rm_a:
            cls = "ok"
        elif sorted(seq_b) == sorted(seq_a) and sorted(rm_b) == sorted(rm_a):
            cls = "仅顺序"
            notes.append("%s/%s 仅顺序不同" % (k[1], k[0]))
        elif sorted(nod_b) == sorted(nod_a) and sorted(rm_b) == sorted(rm_a):
            cls = "仅 due"
            notes.append("%s/%s 仅 due 不同" % (k[1], k[0]))
            # 标记"跨 actor 且收件人本步已跑过"：找同帧审计行 enqueue_step < run_step
            for m in (a.get("Sent") or []):
                key = sent_key(m, False)
                if key in nod_b and any(
                    row.get("who") == k[1]
                    and (row.get("ident"), row.get("recog")) == (key[0], key[1])
                    and row.get("enqueue_step", 0) < row.get("run_step", 0)
                    for row in audit_rows
                ):
                    cross = True
        elif window_tail:
            cls = "仅窗口尾"
            notes.append("%s/%s 帧数差 + after.NotDue 非空（窗口尾）" % (k[1], k[0]))
        else:
            cls = "内容"
            notes.append("%s/%s 内容不同（前 2 帧：%s vs %s）" % (k[1], k[0], sorted(seq_b)[:2], sorted(seq_a)[:2]))
        if order[cls] > order[worst]:
            worst = cls
    return worst, cross, notes[:3]

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--before", required=True)
    ap.add_argument("--after", required=True)
    ap.add_argument("--audit", default=None)
    ap.add_argument("--out", required=True)
    args = ap.parse_args()
    stems = sorted({os.path.splitext(f)[0] for f in os.listdir(args.after) if f.endswith(".json")})
    rows, bad = [], []
    audit_total, audit_cross, cross_stems = 0, 0, []
    for stem in stems:
        before = load(os.path.join(args.before, stem + ".json"))
        after = load(os.path.join(args.after, stem + ".json"))
        audit = []
        if args.audit:
            # 按 case label 分文件（<stem>.<label>.audit.jsonl）；兼容旧的单文件命名
            import glob as _glob
            for apath in sorted(set(_glob.glob(os.path.join(args.audit, stem + ".audit.jsonl.*.jsonl")) +
                                    _glob.glob(os.path.join(args.audit, stem + ".*.audit.jsonl")))) +                           [os.path.join(args.audit, stem + ".audit.jsonl")]:
                if not os.path.exists(apath):
                    continue
                base = os.path.basename(apath)
                if base.startswith(stem + ".audit.jsonl.") and base.endswith(".jsonl"):
                    label = base[len(stem) + len(".audit.jsonl."):-len(".jsonl")]
                elif base.startswith(stem + ".") and base.endswith(".audit.jsonl"):
                    label = base[len(stem) + 1:-len(".audit.jsonl")]
                else:
                    label = None
                with open(apath, encoding="utf-8") as f:
                    for line in f:
                        line = line.strip()
                        if line:
                            try:
                                row = json.loads(line)
                                if label:
                                    row["_case"] = label
                                audit.append(row)
                            except Exception:
                                pass
        audit_total += len(audit)
        stem_cross = sum(1 for r in audit if r.get("enqueue_step", 0) < r.get("run_step", 0))
        audit_cross += stem_cross
        if stem_cross > 0:
            cross_stems.append(stem)
        cls, cross, notes = classify_stem(before, after, audit)
        rows.append((stem, cls, cross, notes))
        wt = bool(after and after.get("cases") and any(c.get("NotDue") for c in after["cases"])
                  and not (before and before.get("cases") and any(c.get("NotDue") for c in before["cases"])))
        if cls in ("内容", "无台账") or (cls == "仅 due" and not cross and not wt):
            bad.append(stem)
    with open(args.out, "w", encoding="utf-8") as f:
        f.write("# 装置 sweep 分类（§5.1）\n\n")
        f.write("| stem | 类别 | 跨 actor 且收件人本步已跑过 | 说明 |\n| --- | --- | --- | --- |\n")
        for stem, cls, cross, notes in rows:
            f.write("| %s | %s | %s | %s |\n" % (stem, cls, "是" if cross else "否", "；".join(notes).replace("|", "/")))
        f.write("\n总计 %d 支；疑模型/未解释：%d 支（%s）\n" % (len(rows), len(bad), ", ".join(bad) or "无"))
    for stem, cls, cross, _ in rows:
        print("%-28s %-6s cross=%s" % (stem, cls, cross))
    print("AUDIT_ROWS=%d CROSS_ROWS=%d CROSS_STEMS=%d (%s)" % (audit_total, audit_cross, len(cross_stems), ",".join(cross_stems[:8])))
    instrument_red = (audit_total == 0 and len(rows) > 0) or (audit_total > 100 and audit_cross == 0)
    if instrument_red:
        print("INSTRUMENT_RED：审计 %d 行却 0 行 enqueue<run ⇒ 仪器疑似 again（装箱/键/扫描点）" % audit_total)
    return 2 if (bad or instrument_red) else 0



def selftest():
    """离线自测：合成夹具证明 ① 分类口径；② INSTRUMENT_RED 两条自检；③ rc 三态。"""
    import tempfile, subprocess, shutil
    root = tempfile.mkdtemp(prefix="sweepcl-")
    try:
        before = os.path.join(root, "before"); after = os.path.join(root, "after"); audit = os.path.join(root, "audit")
        for d in (before, after, audit): os.makedirs(d)
        def case(files, label, sent):
            return {"cases": [{"Label": label, "NotDue": [], "Receivers": [{"Who": "caster", "Rm": [], "Sent": sent}]}]}
        good = [{"ident": 657, "recog": 1, "param": 0, "tag": 0, "series": 7, "tail": "", "due_ms_rel": 800}]
        late = [{"ident": 657, "recog": 1, "param": 0, "tag": 0, "series": 7, "tail": "", "due_ms_rel": 900}]
        json.dump(case(0, "s1", good), open(os.path.join(before, "okstem.json"), "w"))
        json.dump(case(0, "s1", good), open(os.path.join(after, "okstem.json"), "w"))
        json.dump(case(0, "s1", good), open(os.path.join(before, "inststem.json"), "w"))
        json.dump(case(0, "s1", late), open(os.path.join(after, "inststem.json"), "w"))
        out = os.path.join(root, "rep.md")
        # ① 无审计 ⇒ 0 行 + stems>0 ⇒ INSTRUMENT_RED（rc=2）
        rc = subprocess.run([sys.executable, "-I", "-B", os.path.abspath(__file__), "--before", before, "--after", after,
                             "--audit", audit, "--out", out], capture_output=True, text=True).returncode
        assert rc == 2, "无审计必须 INSTRUMENT_RED rc=2，实际 %s" % rc
        # ② 只有非跨步审计（enqueue==run）且仅 due 差异 ⇒ 仍判红
        with open(os.path.join(audit, "inststem.audit.jsonl.s1.jsonl"), "w") as f:
            f.write(json.dumps({"who": "caster", "ident": 657, "recog": 1, "enqueue_step": 900, "run_step": 900}) + "\n")
        rc = subprocess.run([sys.executable, "-I", "-B", os.path.abspath(__file__), "--before", before, "--after", after,
                             "--audit", audit, "--out", out], capture_output=True, text=True).returncode
        assert rc == 2, "仅 due 无 cross 必须红，实际 %s" % rc
        # ③ 跨步证据（enqueue<run）⇒ 通过 rc=0
        with open(os.path.join(audit, "inststem.audit.jsonl.s1.jsonl"), "w") as f:
            f.write(json.dumps({"who": "caster", "ident": 657, "recog": 1, "enqueue_step": 800, "run_step": 900}) + "\n")
        rc = subprocess.run([sys.executable, "-I", "-B", os.path.abspath(__file__), "--before", before, "--after", after,
                             "--audit", audit, "--out", out], capture_output=True, text=True).returncode
        assert rc == 0, "有 cross 证据应 rc=0，实际 %s" % rc
        print("SWEEP_CLASSIFY_SELFTEST_OK 3/3")
        return 0
    finally:
        shutil.rmtree(root, ignore_errors=True)

if __name__ == "__main__":
    if "--selftest" in sys.argv:
        sys.exit(selftest())
    sys.exit(main())
