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
#: 状态同步/无条件回帧：只凭「帧出现」不能判成功，机判条目**必须**带 fields 或 order
#: （卡 `openmir2-criteria-table-hardening`；下一个「637 类假绿」在写表时就被拦住）。
GUARD_IDENTS = {767, 5008, 637, 653, 654, 657, 52}
GUARD_NAMES = {'SM_MENU_OK', 'SM_GAMEGOLDNAME', 'SM_BUTCH', 'SM_GOLDCHANGED',
               'SM_CHANGELIGHT', 'SM_CHARSTATUSCHANGED', 'SM_ABILITY'}
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
        if 'max_count' in a:
            mc = a['max_count']
            if not isinstance(mc, int) or isinstance(mc, bool) or mc < 1:
                raise UsageError('%s.assert.max_count 必须是 ≥1 的整数' % where)
            if mc < int(a.get('min_count', 1)):
                raise UsageError('%s.assert.max_count 不能小于 min_count' % where)
        if a.get('count_scope', 'filtered') not in ('filtered', 'all'):
            raise UsageError("%s.assert.count_scope 只认 filtered/all（实得 %r）"
                             % (where, a.get('count_scope')))
        if a.get('match', 'last') not in ('last', 'any', 'all'):
            raise UsageError('%s.assert.match 只认 last/any/all' % where)
        forbid = a.get('forbid')
        if forbid is not None:
            if not isinstance(forbid, list) or not forbid:
                raise UsageError('%s.assert.forbid 必须是非空 ident 列表' % where)
        rng = a.get('rng_ledger')
        if rng is not None:
            if not isinstance(rng, dict) or not isinstance(rng.get('draws'), list)                     or not rng['draws']:
                raise UsageError('%s.assert.rng_ledger 必须是 {draws:[{max,count}],site_substr:[…]}'
                                 % where)
            for d in rng['draws']:
                if not isinstance(d, dict) or 'max' not in d or 'count' not in d:
                    raise UsageError('%s.assert.rng_ledger.draws 每项要 max/count' % where)
        guard = (isinstance(a['ident'], int) and a['ident'] in GUARD_IDENTS)             or (isinstance(a['ident'], str) and a['ident'] in GUARD_NAMES)
        if guard and not (a.get('fields') or a.get('order')):
            raise UsageError(
                '%s：ident=%r 属状态同步/无条件回帧，machine 判据必须带 fields 或 order'
                '（只凭帧出现会假绿；见 card-criteria-hardening）'
                % (where, a['ident']))
        if 'narrowed' in c and not isinstance(c['narrowed'], str):
            raise UsageError('%s.narrowed 必须是字符串（说明该判据相对 source 收窄了什么）' % where)
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
    # 排除 `discovery/` 子根：那是参数发现跑的**另一个场景**，混入会让计数/帧序假绿
    # （卡 `openmir2-criteria-table-hardening`：e23 根里 discovery 的登录帧曾让 5008 计数 1→2）。
    files = sorted(f for f in set(files)
                   if '/discovery/' not in f.replace(os.sep, '/'))
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


def _count_ledger(ledger, rng):
    """按 max 计账本行数；site_substr 给出时每类还要有任一命中该子串。"""
    rows = []
    for d in rng['draws']:
        want_max = int(d['max'])
        sel = [e for e in ledger if e.get('max') == want_max]
        subs = [s2 for s2 in (rng.get('site_substr') or [])]
        detail = []
        if subs:
            matched = [e for e in sel if any(s2 in str(e.get('site', '')) for s2 in subs)]
            if not matched:
                return None, 'max=%d 的账本行 site 不含 %r' % (want_max, subs)
            detail = matched
        if len(sel) != int(d['count']):
            return None, 'max=%d 账本计数 %d != %d' % (want_max, len(sel), int(d['count']))
        rows.extend(sel)
    return rows, None


def load_rng_ledger(run_dir):
    """读证据根下所有 `rng-stream.tsv`（5 列 seq/kind/max/result/site）；无则 None。"""
    out = []
    found = False
    for path in glob.glob(os.path.join(run_dir, '**', 'rng-stream.tsv'), recursive=True):
        found = True
        try:
            for ln in open(path, encoding='utf-8', errors='replace'):
                cols = ln.rstrip('\r\n').split('\t')
                if len(cols) < 5:
                    continue
                try:
                    mx = int(cols[2])
                except ValueError:
                    continue
                out.append({'seq': cols[0], 'kind': cols[1], 'max': mx,
                            'result': cols[3], 'site': cols[4]})
        except OSError:
            continue
    return out if found else None


def evaluate(item, frames, names, scenario=None, ledger=None):
    """返回 (verdict, detail)。

    scenario = 证据根声明的场景名；None 表示未声明（由 main 按 --allow-unverified 决定是否放行）。
    条目可带 `match`：`last`（默认，只判最后一帧）/ `any`（任一帧满足即 PASS）/ `all`（每帧都必须满足）。
    """
    if scenario is not None and item.get('scenario') != scenario:
        return 'UNDECIDABLE', '场景不符：证据根=%s，判据场景=%s' % (scenario, item.get('scenario'))
    if item.get('machine', True) is False:
        return 'UNDECIDABLE', '人工判据（不可机判）：' + item['reason']
    a = item['assert']
    if a.get('rng_ledger'):
        if ledger is None:
            return 'UNDECIDABLE', 'rng_ledger 断言需要 --run-dir 下有 rng-stream.tsv（本证据根未找到账本）'
        got_rows, err = _count_ledger(ledger, a['rng_ledger'])
        if err:
            return 'FAIL', 'rng_ledger 不成立：' + err
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
    ident_sel = sel          # 该 ident 在证据根里的**全量**（仅受 only_when 约束）
    for spec in a.get('forbid') or []:
        fid = resolve_ident(spec, names)
        fsel = [f for f in frames if f.get('ident') == fid]
        if pat:
            fsel = [f for f in fsel if path_matches(f['__file'], pat)]
        if fsel:
            return 'FAIL', 'forbid 帧 ident=%d 出现了 %d 次（成对 FAIL 帧不得出现）' % (
                fid, len(fsel))
    if 'recog' in a:
        rec = [f for f in sel if f.get('recog') == a['recog']]
        if not rec:
            return 'FAIL', 'ident=%d 的 recog 均 != %r（实得 %r）' % (
                ident, a['recog'], sorted({f.get('recog') for f in sel}))
        sel = rec
    min_count = int(a.get('min_count', 1))
    # 计数口径（卡 `openmir2-card-evidence-count-scope`）：
    #   filtered（默认）= 按 `recog` 过滤后的帧数（原语义）；
    #   all           = 按 ident 的**总帧数**（不受 recog 过滤影响）——「653 总帧数恰一条」用它。
    scope = a.get('count_scope', 'filtered')
    counted = ident_sel if scope == 'all' else sel
    if len(counted) < min_count:
        return 'UNDECIDABLE', 'ident=%d %s出现 %d 次 < min_count=%d' % (
            ident, '' if scope == 'filtered' else '（count_scope=all）', len(counted), min_count)
    if a.get('max_count') is not None and len(counted) > int(a['max_count']):
        return 'FAIL', 'ident=%d %s出现 %d 次 > max_count=%d（多出的帧是回归，不是缺数据）' % (
            ident, '' if scope == 'filtered' else '（count_scope=all）',
            len(counted), int(a['max_count']))
    fields = a.get('fields') or []
    match = a.get('match', 'last')
    results = []
    for fr in ([sel[-1]] if match == 'last' else sel):
        problems = []
        und = None
        for f in fields:
            path = f['path']
            cmp = f.get('cmp', 'eq')
            mask = f.get('mask', 'none')
            got = fr.get(path)
            if got is None or (isinstance(got, str) and not got.strip()):
                und = 'ident=%d 的字段 %s 缺失/为空（帧 seq=%s）' % (ident, path, fr.get('seq'))
                break
            ok = compare(got, f['expect'], cmp, mask)
            if ok is None:
                und = '字段 %s 无法比较（got=%r cmp=%s）' % (path, got, cmp)
                break
            if not ok:
                problems.append('%s: got=%r expect=%r cmp=%s mask=%s'
                                % (path, got, f['expect'], cmp, mask))
        results.append((fr, problems, und))
    if match == 'last':
        _fr, problems, und = results[0]
        if und:
            return 'UNDECIDABLE', und
        if problems:
            return 'FAIL', '; '.join(problems)
        return 'PASS', 'ident=%d ×%d 断言全部成立（match=last）' % (ident, len(sel))
    if match == 'any':
        for fr, problems, und in results:
            if und is None and not problems:
                return 'PASS', 'ident=%d ×%d 有帧满足（match=any，seq=%s）' % (
                    ident, len(sel), fr.get('seq'))
        if all(und is not None for _fr, _problems, und in results):
            return 'UNDECIDABLE', results[-1][2]
        bad = [r for r in results if r[1]][-1]
        return 'FAIL', 'ident=%d ×%d 无帧满足（match=any）；最后一帧不满足：%s' % (
            ident, len(sel), '; '.join(bad[1]))
    for fr, problems, und in results:
        if und:
            return 'UNDECIDABLE', und
        if problems:
            return 'FAIL', 'ident=%d 帧 seq=%s 不满足（match=all）：%s' % (
                ident, fr.get('seq'), '; '.join(problems))
    return 'PASS', 'ident=%d ×%d 每帧断言成立（match=all）' % (ident, len(sel))


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
        # 牙 f：max_count 超限 ⇒ FAIL（让「恰一条」可表达）
        two = [dict(frames[0], seq=1), dict(frames[0], seq=2)]
        v, _ = evaluate({'card': 'c', 'w_item': 'w', 'scenario': 's',
                         'assert': {'ident': 653, 'min_count': 1, 'max_count': 1}}, two, {})
        chk(v == 'FAIL', '牙f：帧数超 max_count ⇒ FAIL（恰一条）')
        v, _ = evaluate({'card': 'c', 'w_item': 'w', 'scenario': 's',
                         'assert': {'ident': 653, 'min_count': 1, 'max_count': 1}}, got, {})
        chk(v == 'PASS', '牙f-对照：恰一帧 ⇒ PASS')
        # 牙 g：min_count 不足 ⇒ UNDECIDABLE
        v, _ = evaluate({'card': 'c', 'w_item': 'w', 'scenario': 's',
                         'assert': {'ident': 653, 'min_count': 2}}, got, {})
        chk(v == 'UNDECIDABLE', '牙g：次数 < min_count ⇒ UNDECIDABLE')
        # 牙 h：场景归属不符 ⇒ UNDECIDABLE（防张冠李戴）
        v, _ = evaluate({'card': 'c', 'w_item': 'w', 'scenario': 'B',
                         'assert': {'ident': 653}}, got, {}, 'A')
        chk(v == 'UNDECIDABLE', '牙h：证据根场景 A ≠ 判据场景 B ⇒ UNDECIDABLE')
        v, _ = evaluate({'card': 'c', 'w_item': 'w', 'scenario': 'A',
                         'assert': {'ident': 653}}, got, {}, 'A')
        chk(v == 'PASS', '牙h-对照：场景相符 ⇒ PASS')
        # 牙 i：match=last / any 的语义分歧（同 ident 多帧）
        multi = [dict(frames[0], seq=1, param=6), dict(frames[0], seq=2, param=5)]
        v, _ = evaluate({'card': 'c', 'w_item': 'w', 'scenario': 's', 'assert': {
            'ident': 653, 'fields': [{'path': 'param', 'expect': 6}]}}, multi, {})
        chk(v == 'FAIL', '牙i：match=last 只判最后一帧 ⇒ FAIL')
        v, _ = evaluate({'card': 'c', 'w_item': 'w', 'scenario': 's', 'assert': {
            'ident': 653, 'match': 'any',
            'fields': [{'path': 'param', 'expect': 6}]}}, multi, {})
        chk(v == 'PASS', '牙i-对照：match=any 有帧满足 ⇒ PASS')
        # 牙 j：证据根无 run.json 且未给 --scenario ⇒ 全 UNDECIDABLE（F7）
        import contextlib
        import io as _io
        norun = os.path.join(tmp, 'norun')
        os.makedirs(norun)
        with open(os.path.join(norun, 'frames.ndjson'), 'w', encoding='utf-8') as fh:
            fh.write(json.dumps({'seq': 1, 'ident': 653, 'param': 4}) + chr(10))
        crit2 = os.path.join(tmp, 'crit2.json')
        with open(crit2, 'w', encoding='utf-8') as fh:
            json.dump({'format': 'card-criteria/1', 'criteria': [
                {'card': 'c', 'w_item': 'w', 'scenario': 's',
                 'assert': {'ident': 653, 'fields': [{'path': 'param', 'expect': 4}]}}]}, fh)
        buf = _io.StringIO()
        with contextlib.redirect_stdout(buf):
            rc = main(['--run-dir', norun, '--criteria', crit2])
        out = buf.getvalue()
        chk(rc == 0 and not any(l.startswith('PASS') for l in out.splitlines()),
            '牙j：缺 run.json + 无 --scenario ⇒ 全 UNDECIDABLE')
        buf = _io.StringIO()
        with contextlib.redirect_stdout(buf):
            rc = main(['--run-dir', norun, '--criteria', crit2, '--allow-unverified'])
        chk(rc == 0 and any(l.startswith('PASS') for l in buf.getvalue().splitlines()),
            '牙j-对照：--allow-unverified ⇒ 放行判 PASS')
        # 牙 k：count_scope=all ⇒ 「653 总帧数恰 1」；两帧（recog 100/999）必须 FAIL
        scope_all = {'card': 'c', 'w_item': 'w', 'scenario': 's', 'assert': {
            'ident': 653, 'recog': 100, 'min_count': 1, 'max_count': 1,
            'count_scope': 'all'}}
        scope_filt = {'card': 'c', 'w_item': 'w', 'scenario': 's', 'assert': {
            'ident': 653, 'recog': 100, 'min_count': 1, 'max_count': 1}}
        two653 = [dict(frames[0], seq=1, recog=100), dict(frames[0], seq=2, recog=999)]
        v, d = evaluate(scope_all, two653, {})
        chk(v == 'FAIL' and 'count_scope=all' in d,
            '牙k：count_scope=all 且 653 总帧数=2 ⇒ FAIL（原假 PASS）')
        v, _ = evaluate(scope_filt, two653, {})
        chk(v == 'PASS', '牙k-对照：默认 filtered 只数 recog=100 ⇒ PASS')
        one653 = [dict(frames[0], seq=1, recog=100)]
        v, _ = evaluate(scope_all, one653, {})
        chk(v == 'PASS', '牙k2：count_scope=all 且总帧数=1 ⇒ PASS')
        bad_scope = os.path.join(tmp, 'badscope.json')
        with open(bad_scope, 'w', encoding='utf-8') as fh:
            json.dump({'format': 'card-criteria/1', 'criteria': [
                {'card': 'c', 'w_item': 'w', 'scenario': 's',
                 'assert': {'ident': 653, 'count_scope': 'nope'}}]}, fh)
        try:
            load_criteria(bad_scope)
            chk(False, '牙m：count_scope 非法 ⇒ 加载期拒')
        except UsageError:
            chk(True, '牙m：count_scope 非法 ⇒ 加载期拒（点名）')
        # 牙 n：W-DG-2 的 oracle 口径——637 收官、**不要求 33**
        wdg2 = {'card': 'c', 'w_item': 'w', 'scenario': 's', 'assert': {
            'ident': 637, 'min_count': 1, 'order': [32, 637]}}
        death_butch = [dict(frames[0], seq=1, ident=32, recog=100),
                       dict(frames[0], seq=2, ident=637, recog=100)]
        v, _ = evaluate(wdg2, death_butch, {})
        chk(v == 'PASS', '牙n：SM_DEATH(32) → SM_BUTCH(637) ⇒ PASS')
        only33 = [dict(frames[0], seq=1, ident=32, recog=100),
                  dict(frames[0], seq=2, ident=33, recog=100)]
        v, _ = evaluate(wdg2, only33, {})
        chk(v != 'PASS', '牙n-对照：只有 33 没有 637 ⇒ 不得 PASS（33 非每刀必发）')
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

        # 牙 g：结构性守卫——machine=true + 受守卫 ident + 无 fields/order ⇒ 判据表不合法
        guard_bad = {'format': 'card-criteria/1', 'criteria': [
            {'card': 'c', 'w_item': 'w', 'scenario': 's',
             'assert': {'ident': 'SM_BUTCH', 'min_count': 1}}]}
        gb = os.path.join(tmp, 'crit-bad.json')
        with open(gb, 'w', encoding='utf-8') as fh:
            json.dump(guard_bad, fh, ensure_ascii=False)
        try:
            load_criteria(gb)
            chk(False, '牙g：machine + SM_BUTCH(637) 无 fields/order ⇒ 判据表不合法')
        except UsageError as ex:
            chk('fields 或 order' in str(ex),
                '牙g：machine + SM_BUTCH(637) 无 fields/order ⇒ UsageError（%s）' % str(ex)[:40])
        guard_ok = {'format': 'card-criteria/1', 'criteria': [
            {'card': 'c', 'w_item': 'w', 'scenario': 's',
             'assert': {'ident': 'SM_BUTCH', 'min_count': 1,
                        'fields': [{'path': 'param', 'expect': 1}]}}]}
        go = os.path.join(tmp, 'crit-ok.json')
        with open(go, 'w', encoding='utf-8') as fh:
            json.dump(guard_ok, fh, ensure_ascii=False)
        try:
            chk(len(load_criteria(go)) == 1, '牙g-对照：补上 fields 后判据表合法')
        except UsageError as ex:
            chk(False, '牙g-对照：补上 fields 仍报错：%s' % ex)

        # 牙 g2：657（SM_CHARSTATUSCHANGED，登录必发状态同步帧）同样受守卫
        g657_bad = {'format': 'card-criteria/1', 'criteria': [
            {'card': 'c', 'w_item': 'w', 'scenario': 's',
             'assert': {'ident': 657, 'min_count': 2}}]}
        b657 = os.path.join(tmp, 'crit-657-bad.json')
        with open(b657, 'w', encoding='utf-8') as fh:
            json.dump(g657_bad, fh, ensure_ascii=False)
        try:
            load_criteria(b657)
            chk(False, '牙g2：machine + 657 无 fields/order ⇒ 必须 UsageError')
        except UsageError:
            chk(True, '牙g2：machine + 657 无 fields/order ⇒ UsageError')
        g657_ok = {'format': 'card-criteria/1', 'criteria': [
            {'card': 'c', 'w_item': 'w', 'scenario': 's',
             'assert': {'ident': 657, 'min_count': 2,
                        'order': ['SM_ABILITY', 'SM_CHARSTATUSCHANGED']}}]}
        o657 = os.path.join(tmp, 'crit-657-ok.json')
        with open(o657, 'w', encoding='utf-8') as fh:
            json.dump(g657_ok, fh, ensure_ascii=False)
        try:
            chk(len(load_criteria(o657)) == 1, '牙g2-对照：657 补 order 后合法')
        except UsageError as ex:
            chk(False, '牙g2-对照：657 补 order 仍报错：%s' % ex)

        # 牙 f：成对帧 forbid —— FAIL 帧出现即失败
        pair_frames = [dict(frames[0], seq=1, ident=615, param=0)]
        v, _ = evaluate({'card': 'c', 'w_item': 'w', 'scenario': 's', 'assert': {
            'ident': 615, 'min_count': 1, 'forbid': [616]}},
            pair_frames, {})
        chk(v == 'PASS', '牙f：615 出现且 616 缺席 ⇒ PASS')
        v, _ = evaluate({'card': 'c', 'w_item': 'w', 'scenario': 's', 'assert': {
            'ident': 615, 'min_count': 1, 'forbid': [616]}},
            pair_frames + [dict(frames[0], seq=2, ident=616, param=0)], {})
        chk(v == 'FAIL' and 'forbid' in _, '牙f：616 出现 ⇒ FAIL（成对 FAIL 帧不得出现）')

        # 牙 r：rng_ledger —— 恰两抽 + site 归属；无账本 ⇒ UNDECIDABLE
        rng_item = {'card': 'c', 'w_item': 'w', 'scenario': 's', 'assert': {
            'ident': 32, 'min_count': 1,
            'rng_ledger': {'draws': [{'max': 16, 'count': 1}, {'max': 201, 'count': 1}],
                           'site_substr': ['Operate.cs:722', 'Operate.cs:723']}}}
        ok_ledger = [{'max': 16, 'site': 'Operate.cs:722'}, {'max': 201, 'site': 'Operate.cs:723'}]
        v, _ = evaluate(rng_item, [dict(frames[0], ident=32)], {}, ledger=ok_ledger)
        chk(v == 'PASS', '牙r：账本恰两抽（16×1、201×1）且 site 正确 ⇒ PASS')
        v, _ = evaluate(rng_item, [dict(frames[0], ident=32)], {},
                        ledger=[ok_ledger[0]])
        chk(v == 'FAIL', '牙r：缺 201 抽 ⇒ FAIL')
        v, _ = evaluate(rng_item, [dict(frames[0], ident=32)], {}, ledger=None)
        chk(v == 'UNDECIDABLE', '牙r：证据根无 rng-stream.tsv ⇒ UNDECIDABLE（不是 PASS）')
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
    ap.add_argument('--allow-unverified', action='store_true',
                    help='证据根无 run.json 且未给 --scenario 时仍逐条判（provenance=unverified）；'
                         '缺省则全部 UNDECIDABLE（fail-closed）')
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
        ledger = load_rng_ledger(args.run_dir)
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
    gated = False
    if scenario is None:
        if args.allow_unverified:
            print('注意：未声明 --scenario 且证据根无 run.json；--allow-unverified 已放行，'
                  'provenance=unverified——PASS 只说明「该证据根里出现了这些帧」，不得当窗口证据。')
        else:
            gated = True
            print('证据根场景未声明（缺 run.json 且无 --scenario）⇒ **全部 UNDECIDABLE**（fail-closed）；'
                  '确需放行请显式加 --allow-unverified。')
    else:
        print('证据根场景 = %s（来源 %s）' % (scenario, provenance))
    for it in items:
        if gated:
            verdict, detail = 'UNDECIDABLE', '证据根场景未声明（缺 run.json 且无 --scenario）'
        else:
            try:
                verdict, detail = evaluate(it, frames, names, scenario, ledger)
            except UsageError as ex:
                print('card_evidence: 判据 %s/%s：%s'
                      % (it.get('card'), it.get('w_item'), ex), file=sys.stderr)
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
