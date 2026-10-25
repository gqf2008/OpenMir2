#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""card_evidence.py — 条件② 的**卡面判据**机判器（只读；不启动服务端、不占端口）。

判据表 = `tools/window/card-criteria.json`（schema 见下）；运行器读一个**证据根**
（`frames.ndjson` 的集合），逐条给出 verdict ∈ {PASS, FAIL, UNDECIDABLE}。

三态语义（**UNDECIDABLE 绝不是 PASS**）：
  * PASS        —— 找到该 ident 的帧，且**所有**断言字段按比较方式成立；
  * FAIL        —— 找到帧且字段存在，但比较不成立；
  * UNDECIDABLE —— 缺数据：证据根没有该 ident / 字段缺失 / 字段为 null·空串 / 比较方式无法使用
                   （如对非数字做 ge）。**默认拒绝通过**。

与 `compare_runs.py --rules` 的分工：rules 管「两侧 ident 级 diff」；本工具管「卡面判据在**单侧证据**上是否成立」。

用法：
  python -I -B tools/window/card_evidence.py --run-dir <证据根> [--criteria <json>] [--json out.json]
  python -I -B tools/window/card_evidence.py --selftest

退出码（钉死）：
  0 —— 无 FAIL（全部 PASS 或 UNDECIDABLE）；
  1 —— 至少一条 FAIL；
  2 —— 用法错误 / 证据根读不到 / 判据表不合法（fail-closed）。

判据表 schema（`--criteria` 默认 `tools/window/card-criteria.json`）：
{
  "format": "card-criteria/1",
  "criteria": [
    {"card": "openmir2-…", "w_item": "W-XX-1", "scenario": "…",
     "source": "docs/…§…",
     "assert": {
       "ident": 653,                     # 数字或消息名（名→号由 crates/protocol/src/messages.rs 解析）
       "recog": 9912,                    # 可选：recog 必须等于该值（数字）
       "fields": [                        # 可空；空 = 只要求该 ident 至少出现一次
         {"path": "param", "expect": 4, "cmp": "eq"},
         {"path": "body_text", "expect": "…", "cmp": "eq", "mask": "datetime"},
         {"path": "param", "expect": 3, "cmp": "ge"}
       ],
       "order": [643, 767, 44],          # 可选：这些 ident 的**首个** seq 必须递增（帧序）
       "min_count": 1,                    # 可选：该 ident 至少出现几次
       "only_when": {"file_glob": "…"}    # 可选：只在该证据子根下判（否则 UNDECIDABLE）
     }}
   ]
}
条目可声明 `"machine": false, "reason": "…"`（无 `assert`）：运行器给 **UNDECIDABLE + 原因**，
用来逐行登记「本轮不可机判」的 W 项——**不许静默跳过**。
`cmp` ∈ {eq, ne, ge, le, contains, regex}；`mask` ∈ {datetime, digits, none}（`datetime` 抹掉时间串、
`digits` 抹掉所有数字串后比较）。
"""
from __future__ import annotations

import argparse
import fnmatch
import glob
import json
import os
import re
import shutil
import sys
import tempfile

MESSAGES_RS = os.path.join('mir2-rs', 'crates', 'protocol', 'src', 'messages.rs')
CMP = ('eq', 'ne', 'ge', 'le', 'contains', 'regex')
MASKS = ('datetime', 'digits', 'none')

DATE_RE = re.compile(r'\d{4}-\d{2}-\d{2}(?:[ T]\d{2}:\d{2}:\d{2})?')
DIGITS_RE = re.compile(r'\d+')


class UsageError(Exception):
    pass


# ---------------------------------------------------------------- 消息名 → ident
def load_ident_names(repo):
    path = os.path.join(repo, MESSAGES_RS)
    names = {}
    try:
        text = open(path, encoding='utf-8').read()
    except OSError:
        return names
    for m in re.finditer(r'pub const ([A-Z][A-Z0-9_]*): u16 = (\d+);', text):
        names.setdefault(m.group(1), int(m.group(2)))
    return names


def resolve_ident(spec, names):
    if isinstance(spec, int) and not isinstance(spec, bool):
        return spec
    if isinstance(spec, str):
        if spec.isdigit():
            return int(spec)
        if spec in names:
            return names[spec]
        raise UsageError('未知消息名：%s（不在 %s 里）' % (spec, MESSAGES_RS))
    raise UsageError('ident 必须是数字或消息名，得到 %r' % (spec,))


# ---------------------------------------------------------------- 判据表校验
def load_criteria(path):
    try:
        doc = json.load(open(path, encoding='utf-8'))
    except OSError as ex:
        raise UsageError('读不到判据表 %s：%s' % (path, ex))
    except ValueError as ex:
        raise UsageError('判据表 %s 不是合法 JSON：%s' % (path, ex))
    if doc.get('format') != 'card-criteria/1':
        raise UsageError('判据表 format 必须是 card-criteria/1')
    items = doc.get('criteria')
    if not isinstance(items, list) or not items:
        raise UsageError('判据表 criteria 必须是非空数组')
    for i, c in enumerate(items):
        where = 'criteria[%d]' % i
        for k in ('card', 'w_item', 'scenario'):
            if k not in c:
                raise UsageError('%s 缺 %s' % (where, k))
        if c.get('machine', True) is False:
            if not c.get('reason'):
                raise UsageError('%s 声明 machine=false 但缺 reason（不许静默跳过）' % where)
            continue
        if 'assert' not in c:
            raise UsageError('%s 缺 assert（机判条目）' % where)
        a = c['assert']
        if not isinstance(a, dict):
            raise UsageError('%s.assert 必须是对象' % where)
        if 'ident' not in a:
            raise UsageError('%s.assert 缺 ident' % where)
        if 'order' in a and (not isinstance(a['order'], list) or len(a['order']) < 2):
            raise UsageError('%s.assert.order 必须是 ≥2 项的 ident 列表' % where)
        for f in a.get('fields') or []:
            if 'path' not in f or 'expect' not in f:
                raise UsageError('%s.assert.fields 每项要 path/expect' % where)
            if f.get('cmp', 'eq') not in CMP:
                raise UsageError('%s.assert.fields.cmp=%r 不认识' % (where, f.get('cmp')))
            if f.get('mask', 'none') not in MASKS:
                raise UsageError('%s.assert.fields.mask=%r 不认识' % (where, f.get('mask')))
    return items


# ---------------------------------------------------------------- 证据读取
def scenario_from_run_json(run_dir):
    """探针产物 `run.json` 里的 `scenario.name`（保留现场的真实场景名，避免手填错）。"""
    p = os.path.join(run_dir, 'run.json')
    try:
        doc = json.load(open(p, encoding='utf-8'))
    except (OSError, ValueError):
        return None
    name = (doc.get('scenario') or {}).get('name')
    return name if isinstance(name, str) and name else None


def load_frames(run_dir):
    files = []
    top = os.path.join(run_dir, 'frames.ndjson')
    if os.path.isfile(top):
        files.append(top)
    files.extend(sorted(glob.glob(os.path.join(run_dir, '**', 'frames.ndjson'), recursive=True)))
    files = sorted(set(files))
    if not files:
        raise UsageError('证据根 %s 下没有 frames.ndjson' % run_dir)
    frames = []
    for f in files:
        for line in open(f, encoding='utf-8', errors='replace'):
            line = line.strip()
            if not line:
                continue
            try:
                d = json.loads(line)
            except ValueError:
                continue
            d['__file'] = f
            frames.append(d)
    return files, frames


# ---------------------------------------------------------------- 断言
def apply_mask(value, mask):
    if not isinstance(value, str):
        return value
    if mask == 'datetime':
        return DATE_RE.sub('<DATETIME>', value)
    if mask == 'digits':
        return DIGITS_RE.sub('<N>', value)
    return value


def compare(got, expect, cmp, mask):
    g = apply_mask(got, mask)
    e = apply_mask(expect, mask) if isinstance(expect, str) else expect
    if cmp == 'eq':
        return g == e
    if cmp == 'ne':
        return g != e
    if cmp in ('ge', 'le'):
        try:
            return (float(g) >= float(e)) if cmp == 'ge' else (float(g) <= float(e))
        except (TypeError, ValueError):
            return None
    if cmp == 'contains':
        return isinstance(g, str) and isinstance(e, str) and e in g
    if cmp == 'regex':
        return isinstance(g, str) and re.search(str(e), g) is not None
    return None


def path_matches(path, pattern):
    """file_glob：把两边都规范成 '/' 分隔；命中整路径或路径尾部即算匹配。"""
    p = path.replace(os.sep, '/').replace(chr(92), '/')
    q = pattern.replace(os.sep, '/').replace(chr(92), '/')
    if fnmatch.fnmatch(p, q):
        return True
    return fnmatch.fnmatch(p, '*/' + q.lstrip('/'))


def check_order(order, frames, names):
    """order=[ident,...]：要求这些 ident 的**首个** seq 在该文件内严格递增。"""
    per_file = {}
    for f in frames:
        per_file.setdefault(f.get('__file') or '<memory>', []).append(f)
    checked = 0
    for fn, fs in sorted(per_file.items()):
        first = {}
        for spec in order:
            ident = resolve_ident(spec, names)
            seqs = [f.get('seq') for f in fs if f.get('ident') == ident and isinstance(f.get('seq'), int)]
            if seqs:
                first[ident] = min(seqs)
        if len(first) < len(order):
            continue
        checked += 1
        seq = [first[resolve_ident(x, names)] for x in order]
        if any(b <= a_ for a_, b in zip(seq, seq[1:])):
            return 'FAIL', '帧序不成立（%s）：%s' % (
                os.path.basename(fn), ['%s@%d' % (x, first[resolve_ident(x, names)]) for x in order])
    if not checked:
        return 'UNDECIDABLE', 'order 里的 ident 未齐（%s）' % [str(x) for x in order]
    return 'PASS', '帧序成立（%d 个证据文件各检 1 次）' % checked


def evaluate(item, frames, names, scenario=None):
    """返回 (verdict, detail)。scenario = 证据根声明的场景名（None = 未声明，不做场景归属校验）。"""
    if scenario is not None and item.get('scenario') != scenario:
        return 'UNDECIDABLE', '场景不符：证据根=%s，判据场景=%s' % (scenario, item.get('scenario'))
    if item.get('machine', True) is False:
        return 'UNDECIDABLE', '人工判据（不可机判）：' + item['reason']
    a = item['assert']
    if a.get('order'):
        verdict, detail = check_order(a['order'], frames, names)
        if verdict != 'PASS':
            return verdict, detail
    ident = resolve_ident(a['ident'], names)
    sel = [f for f in frames if f.get('ident') == ident]
    pat = (a.get('only_when') or {}).get('file_glob')
    if pat:
        sel = [f for f in sel if path_matches(f['__file'], pat)]
    if not sel:
        return 'UNDECIDABLE', 'ident=%d 不在证据根里（该场景没跑到/帧未采）' % ident
    min_count = int(a.get('min_count', 1))
    if len(sel) < min_count:
        return 'UNDECIDABLE', 'ident=%d 只出现 %d 次 < min_count=%d' % (ident, len(sel), min_count)
    if 'recog' in a:
        rec = [f for f in sel if f.get('recog') == a['recog']]
        if not rec:
            return 'FAIL', 'ident=%d 的 recog 均 != %r（实得 %r）' % (
                ident, a['recog'], sorted({f.get('recog') for f in sel}))
        sel = rec
    problems = []
    for f in a.get('fields') or []:
        path = f['path']
        cmp = f.get('cmp', 'eq')
        mask = f.get('mask', 'none')
        got = sel[-1].get(path)
        if got is None or (isinstance(got, str) and not got.strip()):
            return 'UNDECIDABLE', 'ident=%d 的字段 %s 缺失/为空（帧 seq=%s）' % (
                ident, path, sel[-1].get('seq'))
        ok = compare(got, f['expect'], cmp, mask)
        if ok is None:
            return 'UNDECIDABLE', '字段 %s 无法比较（got=%r cmp=%s）' % (path, got, cmp)
        if not ok:
            problems.append('%s: got=%r expect=%r cmp=%s mask=%s' % (path, got, f['expect'], cmp, mask))
    if problems:
        return 'FAIL', '; '.join(problems)
    return 'PASS', 'ident=%d ×%d 断言全部成立' % (ident, len(sel))


# ---------------------------------------------------------------- selftest（4 牙）
def selftest():
    ok = fail = 0

    def chk(cond, name):
        nonlocal ok, fail
        if cond:
            ok += 1
            print('PASS %s' % name)
        else:
            fail += 1
            print('FAIL %s' % name)

    tmp = tempfile.mkdtemp(prefix='card-evidence-selftest.')
    try:
        run = os.path.join(tmp, 'run')
        os.makedirs(run)
        frames = [
            {'seq': 1, 'ident': 653, 'recog': 9912, 'param': 4, 'tag': 0,
             'body_text': '2026-10-25 06:15:48 买药成功'}
        ]
        with open(os.path.join(run, 'frames.ndjson'), 'w', encoding='utf-8') as fh:
            for d in frames:
                fh.write(json.dumps(d, ensure_ascii=False) + '\n')

        files, got = load_frames(run)
        chk(len(got) == 1, '读证据根：1 帧')

        def run_one(assert_obj, names=None):
            item = {'card': 'c', 'w_item': 'w', 'scenario': 's', 'assert': assert_obj}
            return evaluate(item, got, names or {})

        # 绿基线
        v, _ = run_one({'ident': 653, 'recog': 9912,
                        'fields': [{'path': 'param', 'expect': 4}]})
        chk(v == 'PASS', '绿基线：正确期望 ⇒ PASS')
        # 掩码基座
        v, d = run_one({'ident': 653, 'fields': [
            {'path': 'body_text', 'expect': '2026-10-25 06:15:48 买药成功',
             'cmp': 'contains', 'mask': 'datetime'}]})
        chk(v == 'PASS', '掩码：datetime 掩码下同形文本 ⇒ PASS')

        # 牙 a：期望值改错 ⇒ FAIL（不是 PASS）
        v, _ = run_one({'ident': 653, 'fields': [{'path': 'param', 'expect': 999}]})
        chk(v == 'FAIL', '牙a：期望值改错 ⇒ FAIL')
        # 牙 b：不存在的 ident ⇒ UNDECIDABLE（不是 PASS）
        v, _ = run_one({'ident': 10309, 'fields': []})
        chk(v == 'UNDECIDABLE', '牙b：ident 不在帧里 ⇒ UNDECIDABLE')
        # 牙 b2：字段不存在 ⇒ UNDECIDABLE
        v, _ = run_one({'ident': 653, 'fields': [{'path': 'no_such_field', 'expect': 1}]})
        chk(v == 'UNDECIDABLE', '牙b2：字段不存在 ⇒ UNDECIDABLE')
        # 牙 c：值为 null/空 ⇒ UNDECIDABLE
        v, _ = run_one({'ident': 653, 'fields': [{'path': 'tag', 'expect': 0}]})
        # 上面 tag=0 是**存在的 0**，必须 PASS（0 不是 null）
        chk(v == 'PASS', '牙c-对照：字段值为 0（非 null）⇒ PASS')
        frames2 = dict(frames[0]); frames2['body_text'] = ''
        v, _ = evaluate({'card': 'c', 'w_item': 'w', 'scenario': 's', 'assert': {
            'ident': 653, 'fields': [{'path': 'body_text', 'expect': 'x'}]}}, [frames2], {})
        chk(v == 'UNDECIDABLE', '牙c：字段为空串 ⇒ UNDECIDABLE')
        # 牙 d：掩码字段当真值比（不写 mask）⇒ 必须红（证明掩码生效）
        v, _ = run_one({'ident': 653, 'fields': [
            {'path': 'body_text', 'expect': '2026-10-25 06:15:48 买药成功', 'cmp': 'contains'}]})
        # 无 mask 时原文相同 ⇒ 仍 PASS；换成「换了一天的时间串」才是掩码的意义
        v2, _ = evaluate({'card': 'c', 'w_item': 'w', 'scenario': 's', 'assert': {
            'ident': 653, 'fields': [{'path': 'body_text', 'expect': '2026-10-26 00:00:00 买药成功',
                                      'cmp': 'contains'}]}}, got, {})
        chk(v2 == 'FAIL', '牙d1：不同日期串且未写 mask ⇒ FAIL（掩码未生效）')
        v3, _ = evaluate({'card': 'c', 'w_item': 'w', 'scenario': 's', 'assert': {
            'ident': 653, 'fields': [{'path': 'body_text', 'expect': '2026-10-26 00:00:00 买药成功',
                                      'cmp': 'contains', 'mask': 'datetime'}]}}, got, {})
        chk(v3 == 'PASS', '牙d2：同一条加 datetime mask ⇒ PASS（掩码生效）')
        # 牙 e：帧序反了 ⇒ FAIL
        rev = [dict(frames[0], seq=9, ident=44), dict(frames[0], seq=1, ident=767)]
        v, _ = evaluate({'card': 'c', 'w_item': 'w', 'scenario': 's',
                         'assert': {'ident': 44, 'order': [44, 767]}}, rev, {})
        chk(v == 'FAIL', '牙e：帧序反了 ⇒ FAIL')
        v, _ = evaluate({'card': 'c', 'w_item': 'w', 'scenario': 's',
                         'assert': {'ident': 44, 'order': [767, 44]}}, rev, {})
        chk(v == 'PASS', '牙e-对照：帧序正确 ⇒ PASS')
        # 人工条目：显式 machine=false ⇒ UNDECIDABLE（带原因，绝不是 PASS）
        v, d = evaluate({'card': 'c', 'w_item': 'w', 'scenario': 's', 'machine': False,
                         'reason': '需 DB 后态快照'}, got, {})
        chk(v == 'UNDECIDABLE' and 'DB' in d, '人工条目 ⇒ UNDECIDABLE + 原因')
        # 判据表校验 fail-closed
        bad = os.path.join(tmp, 'bad.json')
        open(bad, 'w', encoding='utf-8').write(json.dumps({'format': 'card-criteria/1', 'criteria': [
            {'card': 'c', 'w_item': 'w', 'scenario': 's', 'assert': {'ident': 1,
                                                                     'fields': [{'path': 'x', 'expect': 1, 'cmp': 'zzz'}]}}]}))
        try:
            load_criteria(bad)
            chk(False, '判据表：未知 cmp ⇒ UsageError')
        except UsageError:
            chk(True, '判据表：未知 cmp ⇒ UsageError')
    finally:
        shutil.rmtree(tmp, ignore_errors=True)

    print('%s: card_evidence selftest %d/%d' % ('GREEN' if not fail else 'RED', ok, ok + fail))
    return 0 if not fail else 1


# ---------------------------------------------------------------- main
def main(argv=None):
    ap = argparse.ArgumentParser(description='卡面判据机判器（只读）')
    ap.add_argument('--run-dir', help='证据根（其下有 frames.ndjson）')
    ap.add_argument('--criteria', default=os.path.join('tools', 'window', 'card-criteria.json'))
    ap.add_argument('--scenario', help='证据根所属场景名；给了就核每条判据的 scenario，不符即 UNDECIDABLE')
    ap.add_argument('--json', help='把逐条 verdict 写成 JSON')
    ap.add_argument('--selftest', action='store_true')
    args = ap.parse_args(argv)
    if args.selftest:
        return selftest()
    if not args.run_dir:
        ap.error('缺 --run-dir')
    try:
        items = load_criteria(args.criteria)
        names = load_ident_names('.')
        files, frames = load_frames(args.run_dir)
    except UsageError as ex:
        print('card_evidence: %s' % ex, file=sys.stderr)
        return 2

    results = []
    scenario = args.scenario
    if scenario:
        provenance = 'declared:' + scenario
    else:
        scenario = scenario_from_run_json(args.run_dir)
        provenance = ('run.json:' + scenario) if scenario else 'unverified'
    if scenario is None:
        print('注意：未声明 --scenario 且证据根无 run.json ⇒ 场景归属未校验，'
              'PASS 只说明「该证据根里出现了这些帧」，不得当窗口证据。')
    else:
        print('证据根场景 = %s（来源 %s）' % (scenario, provenance))
    for it in items:
        try:
            verdict, detail = evaluate(it, frames, names, scenario)
        except UsageError as ex:
            print('card_evidence: 判据 %s/%s：%s' % (it.get('card'), it.get('w_item'), ex), file=sys.stderr)
            return 2
        results.append({'card': it['card'], 'w_item': it['w_item'], 'scenario': it.get('scenario'),
                        'source': it.get('source'), 'provenance': provenance,
                        'verdict': verdict, 'detail': detail})

    for r in results:
        print('%-12s %s %-22s %s' % (r['verdict'], r['card'][:40], r['w_item'], r['detail']))
    counts = {v: sum(1 for r in results if r['verdict'] == v) for v in ('PASS', 'FAIL', 'UNDECIDABLE')}
    print('总计：%d 条 = PASS %d / FAIL %d / UNDECIDABLE %d（证据根 %s，%d 帧）'
          % (len(results), counts['PASS'], counts['FAIL'], counts['UNDECIDABLE'],
             args.run_dir, len(frames)))
    if args.json:
        json.dump({'format': 'card-evidence/1', 'run_dir': args.run_dir, 'scenario': scenario,
                   'provenance': provenance, 'frames': len(frames),
                   'files': files, 'counts': counts, 'results': results},
                  open(args.json, 'w', encoding='utf-8'), ensure_ascii=False, indent=2)
        print('json => %s' % args.json)
    return 1 if counts['FAIL'] else 0


if __name__ == '__main__':
    sys.exit(main())
