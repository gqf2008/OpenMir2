#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""脚本宿主缺省实现台账的自检门（卡 `openmir2-script-host-noop-audit`）。

纯标准库、零副作用（`check` 只读）。子命令：

  check [--strict-lines]   校验台账（JSON）与仓库现状一致；rc=0 绿 / rc=1 红
  selftest                 跑 check 绿 + 三颗牙（各自期望红），并证明真实台账未被动过（sha256）
  mutate X                 打印牙 X 的变异说明（调试用），不改文件

台账：同目录 `script_host_noop_audit.json`

**证据口径（复审 `94718b41` finding-1 后改）**：
  - `oracle`（`src/**`，冻结面）按 **file:line 精确**核行——oracle 不随分支漂移。
  - `mod` / `evidence`（`npc.rs`、`engine/mod.rs`，热文件）按 **符号**核：
      * `mod`：该文件里存在 `fn <name>` 声明；`mod_line` 仅作文档参考。
      * `evidence`：`impl` 块里存在该方法，且记录行号落在**同一 impl 块**范围内。
    ⇒ 别的分支给 `npc.rs` 加/删行（不增删覆写）不会再把门判红；`--strict-lines` 可恢复逐行严格核。

判红口径（缺一即红）：
  ① 结构：168 行 = ScriptPlayer 165 + ScriptNpc 3；verdict/safe_kind 计数一致；无重名
  ② 每条 `mod`：文件存在且含 `fn <name>`（strict 时还要求行号命中该声明）
  ③ 每条 `oracle`：文件存在、行号在范围内、该行含（空白归一后的）所引文本
  ④ `needs_card` ⇒ 方法名**不在**对应宿主 impl 块
  ⑤ `safe`+`host_overridden` ⇒ 在 impl 块内，evidence 的 impl/method/line 自洽且行落在块内
  ⑥ `safe`+`equivalent|approximation` ⇒ 未覆写、reason ≥30 字、safe_kind 合法
  ⑦ `covered` ⇒ card ∈ 在途卡集合，且方法未被覆写
  ⑧ 四张子卡 id 都在 `subcards` 里，被引用的行确实指向该卡
"""

import hashlib
import json
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, '..', '..', '..', '..'))
LEDGER = os.path.join(HERE, 'script_host_noop_audit.json')
NPC_RS = os.path.join(ROOT, 'mir2-rs', 'bins', 'game-svr', 'src', 'npc.rs')

KNOWN_CARDS = {
    'openmir2-script-sendmsg-host',
    'openmir2-gm-script-host-seam',
}
SUBCARDS = {
    'openmir2-script-action-var-text',
    'openmir2-script-envir-root-host',
    'openmir2-script-dyn-var-host',
    'openmir2-script-user-account-host',
}
VERDICTS = {'safe', 'needs_card', 'covered'}
SAFE_KINDS = {'overridden', 'equivalent', 'approximation'}
IMPL_OF = {'player': 'ScriptPlayer for NpcPlayer', 'npc': 'ScriptNpc for HostNpc'}


def norm(s):
    return re.sub(r'\s+', ' ', s).strip()


def read_lines(path):
    with open(path, encoding='utf-8', errors='replace') as f:
        return f.read().split('\n')


def parse_impls(path):
    """返回 {impl_name: dict(methods=set, line_of=dict, start=int, end=int)}。"""
    lines = read_lines(path)
    out = {}
    for i, l in enumerate(lines):
        m = re.search(r'impl\s+(Script(?:Player|Npc))\s+for\s+(NpcPlayer|HostNpc)', l)
        if not m:
            continue
        end = i + 1
        while end < len(lines) and lines[end].rstrip() != '}':
            end += 1
        key = '%s for %s' % (m.group(1), m.group(2))
        methods, line_of = set(), {}
        for j in range(i + 1, end):
            mm = re.match(r'\s*fn\s+([A-Za-z0-9_]+)', lines[j])
            if mm:
                methods.add(mm.group(1))
                line_of.setdefault(mm.group(1), j + 1)
        out[key] = dict(methods=methods, line_of=line_of, start=i + 1, end=end)
    return out


class Ctx:
    def __init__(self, strict_lines=False):
        self.strict = strict_lines
        self.impls = parse_impls(NPC_RS)
        self._mod_cache = {}
        self.warnings = []

    def mod_lines(self, path):
        if path not in self._mod_cache:
            self._mod_cache[path] = read_lines(path)
        return self._mod_cache[path]


def validate(doc, ctx):
    errs = []
    warns = ctx.warnings

    def need(cond, msg):
        if not cond:
            errs.append(msg)

    rows = doc.get('rows') or []
    need(doc.get('card') == 'openmir2-script-host-noop-audit', 'card id 不符')
    need(isinstance(doc.get('base_oid'), str) and len(doc['base_oid']) >= 7, '缺 base_oid')
    need(bool(doc.get('line_baseline')), '缺 line_baseline（证据行号基线）')
    need(len(rows) == 168, '行数应为 168（ScriptPlayer 165 + ScriptNpc 3），实为 %d' % len(rows))
    traits = [r.get('trait') for r in rows]
    need(traits.count('player') == 165, 'player 缺省实现应为 165，实为 %d' % traits.count('player'))
    need(traits.count('npc') == 3, 'npc 缺省实现应为 3，实为 %d' % traits.count('npc'))
    names = [r.get('name') for r in rows]
    need(len(names) == len(set(names)), '台账有重名方法: %s' % sorted({n for n in names if names.count(n) > 1}))
    cnt = {v: sum(1 for r in rows if r.get('verdict') == v) for v in VERDICTS}
    need(cnt['safe'] + cnt['needs_card'] + cnt['covered'] == len(rows), 'verdict 三态未覆盖全部行')
    sk = {k: sum(1 for r in rows if r.get('safe_kind') == k) for k in SAFE_KINDS}
    need(sum(sk.values()) == cnt['safe'], 'safe_kind 计数(%d)与 safe 行数(%d)不符' % (sum(sk.values()), cnt['safe']))
    declared = doc.get('counts') or {}
    for k in ('safe', 'needs_card', 'covered'):
        need(declared.get(k) == cnt[k], 'counts.%s 声明 %r 与实算 %d 不符' % (k, declared.get(k), cnt[k]))
    for k, v in sk.items():
        if 'safe_kind' in declared:
            need(declared['safe_kind'].get(k) == v, 'counts.safe_kind.%s 声明 %r 与实算 %d 不符' % (k, declared['safe_kind'].get(k), v))

    for impl_name in IMPL_OF.values():
        need(impl_name in ctx.impls and ctx.impls[impl_name]['methods'],
             '解析不到 impl %s（npc.rs 结构变了？）' % impl_name)

    for r in rows:
        name = r.get('name')
        tag = '`%s`' % name
        v = r.get('verdict')
        need(v in VERDICTS, '%s verdict 非法: %r' % (tag, v))
        need(r.get('kind') in {'noop', 'const', 'delegate'}, '%s kind 非法: %r' % (tag, r.get('kind')))
        need(bool(r.get('semantics')), '%s 缺 semantics' % tag)
        need(bool(r.get('reason')), '%s 缺 reason' % tag)

        # ② mod：符号定位（strict 时行号必须命中声明）
        mf = r.get('mod_file') or ''
        mpath = os.path.join(ROOT, mf)
        if not os.path.isfile(mpath):
            errs.append('%s mod 文件不存在: %r' % (tag, mf))
        else:
            mlines = ctx.mod_lines(mpath)
            hits = [i + 1 for i, l in enumerate(mlines) if re.search(r'\bfn\s+' + re.escape(name) + r'\b', l)]
            if not hits:
                errs.append('%s mod 文件里找不到 fn 声明: %s' % (tag, mf))
            elif ctx.strict and r.get('mod_line') not in hits:
                errs.append('%s mod_line %r 不在 fn 声明行 %s 里（strict）' % (tag, r.get('mod_line'), hits))
            elif r.get('mod_line') not in hits:
                warns.append('%s mod_line %r 已漂移（现为 %s）；符号核通过' % (tag, r.get('mod_line'), hits))

        # ③ oracle：冻结面，精确核行
        o = r.get('oracle')
        if not o:
            errs.append('%s 缺 oracle file:line（本台账所有行都应可核）' % tag)
        else:
            path = os.path.join(ROOT, o.get('file', ''))
            if not os.path.isfile(path):
                errs.append('%s oracle 文件不存在: %s' % (tag, o.get('file')))
            else:
                olines = read_lines(path)
                ln = o.get('line') or 0
                if not isinstance(ln, int) or ln < 1 or ln > len(olines):
                    errs.append('%s oracle 行号越界: %s:%r' % (tag, o.get('file'), ln))
                elif norm(o.get('text', '')) not in norm(olines[ln - 1]):
                    errs.append('%s oracle 行上不含所引文本: %s:%d' % (tag, o.get('file'), ln))

        # ④⑤⑥⑦ 覆写一致性
        impl = ctx.impls[IMPL_OF[r['trait']]]
        is_impl = name in impl['methods']
        if v == 'needs_card':
            if is_impl:
                errs.append('%s 判「需开卡」但 %s 已覆写（npc.rs:%d）' % (tag, IMPL_OF[r['trait']], impl['line_of'][name]))
        elif v == 'covered':
            if r.get('card') not in KNOWN_CARDS:
                errs.append('%s covered 指向未知卡: %r' % (tag, r.get('card')))
            if is_impl:
                errs.append('%s covered 但 %s 已覆写 ⇒ 不该再挂在在途卡下' % (tag, IMPL_OF[r['trait']]))
        else:
            kind = r.get('safe_kind')
            if kind not in SAFE_KINDS:
                errs.append('%s safe 行 safe_kind 非法: %r' % (tag, kind))
            elif kind == 'overridden':
                if not r.get('host_overridden') or not is_impl:
                    errs.append('%s 标「已覆写」但 %s impl 块里没有该方法' % (tag, IMPL_OF[r['trait']]))
                ev = r.get('evidence')
                if not isinstance(ev, dict) or not ev:
                    errs.append('%s 标「已覆写」但缺 evidence 对象' % tag)
                else:
                    if ev.get('impl') != IMPL_OF[r['trait']]:
                        errs.append('%s evidence.impl 与 trait 不符: %r' % (tag, ev.get('impl')))
                    if ev.get('method') != name:
                        errs.append('%s evidence.method 与行名不符: %r' % (tag, ev.get('method')))
                    ln = ev.get('line')
                    if not isinstance(ln, int) or not (impl['start'] < ln <= impl['end']):
                        errs.append('%s evidence.line %r 不在 impl 块 [%d,%d] 内' % (tag, ln, impl['start'], impl['end']))
                    elif impl['line_of'].get(name) != ln:
                        warns.append('%s evidence.line %d 已漂移（现为 %d）；符号核通过'
                                     % (tag, ln, impl['line_of'][name]))
            else:
                if r.get('host_overridden') or is_impl:
                    errs.append('%s 标「%s」但宿主已覆写 ⇒ 口径矛盾' % (tag, kind))
                if len((r.get('reason') or '')) < 30:
                    errs.append('%s 判安全但理由过短（需给出等价/近似证明）' % tag)

        # ⑧ 子卡
        sc = r.get('subcard')
        if sc is not None:
            if sc not in SUBCARDS:
                errs.append('%s subcard 未知: %r' % (tag, sc))
            if v != 'needs_card':
                errs.append('%s 非 needs_card 却挂了子卡' % tag)

    declared_subs = {s.get('id') for s in (doc.get('subcards') or [])}
    need(declared_subs == SUBCARDS, 'subcards 声明与门内置集合不一致: %s' % sorted(declared_subs ^ SUBCARDS))
    for s in doc.get('subcards') or []:
        need(isinstance(s.get('oid'), str) and len(s['oid']) == 40, '子卡 %s 的 oid 非法' % s.get('id'))

    return errs


def sha256(path):
    h = hashlib.sha256()
    with open(path, 'rb') as f:
        for chunk in iter(lambda: f.read(65536), b''):
            h.update(chunk)
    return h.hexdigest()


def cmd_check(doc_path=LEDGER, strict_lines=False):
    try:
        with open(doc_path, encoding='utf-8') as f:
            doc = json.load(f)
    except Exception as e:  # noqa: BLE001
        print('RED: 台账读取失败: %s' % e)
        return 1
    ctx = Ctx(strict_lines)
    errs = validate(doc, ctx)
    if errs:
        print('RED: %d 处不一致%s' % (len(errs), '（strict-lines）' if strict_lines else ''))
        for e in errs[:50]:
            print('  - ' + e)
        if len(errs) > 50:
            print('  ... 其余 %d 条省略' % (len(errs) - 50))
        return 1
    rows = doc['rows']
    if ctx.warnings:
        print('WARN: %d 处行号已漂移（符号核通过，仅文档参考值过时）：' % len(ctx.warnings))
        for w in ctx.warnings[:10]:
            print('  ~ ' + w)
    print('GREEN: %d 行台账全部可核（safe %d / needs_card %d / covered %d；行号基线 %s）' % (
        len(rows), sum(1 for r in rows if r['verdict'] == 'safe'),
        sum(1 for r in rows if r['verdict'] == 'needs_card'),
        sum(1 for r in rows if r['verdict'] == 'covered'), doc.get('line_baseline')))
    return 0


def mut_tooth1(doc):
    target = next(r for r in doc['rows'] if r['verdict'] == 'safe' and r['safe_kind'] == 'overridden')
    target['oracle'] = {'file': 'src/Modules/ScriptEngine/__no_such_file__.cs', 'line': 999999,
                        'text': 'fn %s' % target['name']}
    return '牙①：%s 的 oracle 改成 src/.../__no_such_file__.cs:999999' % target['name']


def mut_tooth2(doc):
    target = next(r for r in doc['rows'] if r['verdict'] == 'needs_card' and r['name'] == 'space_move')
    target['verdict'] = 'safe'
    target['safe_kind'] = 'overridden'
    target['host_overridden'] = True
    target['reason'] = 'NpcPlayer 已在 npc.rs 覆写，缺省实现不被使用。'
    target['evidence'] = {'file': 'mir2-rs/bins/game-svr/src/npc.rs',
                          'impl': 'ScriptPlayer for NpcPlayer', 'method': 'space_move', 'line': 1072}
    doc['counts']['safe'] += 1
    doc['counts']['needs_card'] -= 1
    doc['counts']['safe_kind']['overridden'] += 1
    return '牙②：把 space_move 改判「safe：已覆写」（npc.rs:1072）'


def mut_tooth3(doc):
    target = next(r for r in doc['rows'] if r['verdict'] == 'safe' and r['safe_kind'] == 'overridden')
    target['evidence']['line'] = 1  # 落在 impl 块外
    return '牙③：%s 的 evidence.line 改成 1（impl 块外）' % target['name']


def run_case(name, mutate):
    doc = json.loads(json.dumps(json.load(open(LEDGER, encoding='utf-8'))))
    how = mutate(doc)
    errs = validate(doc, Ctx(False))
    if errs:
        print('  RED（期望）: %s ⇒ %d 处不一致；首条: %s' % (how, len(errs), errs[0]))
        return 0
    print('  GREEN（非期望！）: %s 未被门抓到' % how)
    return 1


def cmd_selftest():
    before = sha256(LEDGER)
    print('== 台账 sha256（运行前）: %s' % before)
    rc = cmd_check()
    print('== check rc=%d（期望 0）' % rc)
    if rc != 0:
        return 1
    rcs = [run_case('tooth1', mut_tooth1), run_case('tooth2', mut_tooth2), run_case('tooth3', mut_tooth3)]
    after = sha256(LEDGER)
    print('== 台账 sha256（运行后）: %s' % after)
    if after != before:
        print('RED: 真实台账被改动（sha 变化）')
        return 1
    print('== 还原证明: sha256 前后一致（%s）' % before)
    ok = all(rc == 0 for rc in rcs)
    print('== selftest %s' % ('GREEN' if ok else 'RED'))
    return 0 if ok else 1


def main(argv):
    args = [a for a in argv[1:] if not a.startswith('--')]
    strict = '--strict-lines' in argv
    cmd = args[0] if args else 'check'
    if cmd == 'check':
        return cmd_check(strict_lines=strict)
    if cmd == 'selftest':
        return cmd_selftest()
    print(__doc__)
    return 2


if __name__ == '__main__':
    sys.exit(main(sys.argv))
