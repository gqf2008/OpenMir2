#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""脚本宿主缺省实现台账的自检门（卡 `openmir2-script-host-noop-audit`）。

纯标准库、零副作用（`check` 只读）。子命令：

  check        校验台账（JSON）与仓库现状一致；rc=0 绿 / rc=1 红
  selftest     跑 check 绿 + 两颗牙（各自期望红），并证明真实台账未被动过（sha256）
  mutate X     打印牙 X 的变异行（调试用），不改文件

台账：同目录 `script_host_noop_audit.json`
行号基线：台账 `base_oid`（本门不校验 commit，只校验「文件存在 + 该行含所引文本/声明」）

判红口径（缺一即红）：
  ① 结构：168 行 = ScriptPlayer 165 + ScriptNpc 3；verdict 三态计数一致；无重名
  ② 每条 `mod_ref` 行存在且含 `fn <name>`
  ③ 每条 `oracle` 文件存在、行号在范围内、该行含（空白归一后的）所引文本
  ④ `needs_card` ⇒ 方法名**不在**对应宿主 impl 块（NpcPlayer / HostNpc）里
  ⑤ `safe` 且理由含「覆写」⇒ 方法名**在**该 impl 块里，且 evidence 指向的 npc.rs 行含 `fn <name>`
  ⑥ `covered` ⇒ card 属于已知在途卡集合，且方法未被覆写
  ⑦ 三张子卡 id 都在 `subcards` 里，且被引用的行确实指向该卡
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
MOD_RS = os.path.join(ROOT, 'mir2-rs', 'crates', 'script', 'src', 'engine', 'mod.rs')

KNOWN_CARDS = {
    'openmir2-script-sendmsg-host',
    'openmir2-gm-script-host-seam',
}
SUBCARDS = {
    'openmir2-script-action-var-text',
    'openmir2-script-envir-root-host',
    'openmir2-script-dyn-var-host',
}
VERDICTS = {'safe', 'needs_card', 'covered'}
KINDS = {'noop', 'const', 'delegate'}


def norm(s):
    return re.sub(r'\s+', ' ', s).strip()


def read_lines(path):
    with open(path, encoding='utf-8', errors='replace') as f:
        return f.read().split('\n')


def parse_impl(path, header_re):
    """返回 (methods:set, line_of:dict) ，按「col 0 的 `}`」界定 impl 块。"""
    lines = read_lines(path)
    methods, line_of = set(), {}
    for i, l in enumerate(lines):
        if re.search(header_re, l):
            j = i + 1
            while j < len(lines) and lines[j].rstrip() != '}':
                m = re.match(r'\s*fn\s+([A-Za-z0-9_]+)', lines[j])
                if m:
                    methods.add(m.group(1))
                    line_of.setdefault(m.group(1), j + 1)
                j += 1
    return methods, line_of


class Red(Exception):
    pass


def load():
    with open(LEDGER, encoding='utf-8') as f:
        return json.load(f)


def validate(doc):
    errs = []

    def need(cond, msg):
        if not cond:
            errs.append(msg)

    # ① 结构
    rows = doc.get('rows') or []
    need(doc.get('card') == 'openmir2-script-host-noop-audit', 'card id 不符')
    need(isinstance(doc.get('base_oid'), str) and len(doc['base_oid']) >= 7, '缺 base_oid')
    need(len(rows) == 168, '行数应为 168（ScriptPlayer 165 + ScriptNpc 3），实为 %d' % len(rows))
    traits = [r.get('trait') for r in rows]
    need(traits.count('player') == 165, 'player 缺省实现应为 165，实为 %d' % traits.count('player'))
    need(traits.count('npc') == 3, 'npc 缺省实现应为 3，实为 %d' % traits.count('npc'))
    names = [r.get('name') for r in rows]
    need(len(names) == len(set(names)), '台账有重名方法: %s' % sorted({n for n in names if names.count(n) > 1}))
    cnt = {v: sum(1 for r in rows if r.get('verdict') == v) for v in VERDICTS}
    need(cnt['safe'] + cnt['needs_card'] + cnt['covered'] == len(rows), 'verdict 三态未覆盖全部行')
    declared = doc.get('counts') or {}
    for k in ('safe', 'needs_card', 'covered'):
        need(declared.get(k) == cnt[k], 'counts.%s 声明 %r 与实算 %d 不符' % (k, declared.get(k), cnt[k]))

    # 宿主 impl
    npc_methods, npc_line = parse_impl(NPC_RS, r'impl ScriptPlayer for NpcPlayer')
    hostnpc_methods, hostnpc_line = parse_impl(NPC_RS, r'impl ScriptNpc for HostNpc')
    impl = {'player': (npc_methods, npc_line, 'NpcPlayer'), 'npc': (hostnpc_methods, hostnpc_line, 'HostNpc')}
    need(bool(npc_methods), '解析不到 impl ScriptPlayer for NpcPlayer')
    need(bool(hostnpc_methods), '解析不到 impl ScriptNpc for HostNpc')

    mod_lines = read_lines(MOD_RS)

    for r in rows:
        name = r.get('name')
        tag = '`%s`' % name
        need(r.get('verdict') in VERDICTS, '%s verdict 非法: %r' % (tag, r.get('verdict')))
        need(r.get('kind') in KINDS, '%s kind 非法: %r' % (tag, r.get('kind')))
        need(bool(r.get('semantics')), '%s 缺 semantics' % tag)
        need(bool(r.get('reason')), '%s 缺 reason' % tag)

        # ② mod_ref
        mr = r.get('mod_ref') or ''
        m = re.match(r'^(.*mod\.rs):(\d+)$', mr)
        if not m:
            errs.append('%s mod_ref 格式非法: %r' % (tag, mr))
        else:
            ln = int(m.group(2))
            if ln < 1 or ln > len(mod_lines):
                errs.append('%s mod_ref 行号越界: %d' % (tag, ln))
            elif not re.search(r'\bfn\s+' + re.escape(name) + r'\b', mod_lines[ln - 1]):
                errs.append('%s mod_ref 指向的行不含 fn 声明: %s' % (tag, mr))

        # ③ oracle
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

        # ④⑤⑥ 覆写一致性
        methods, line_of, host_name = impl[r['trait']]
        is_impl = name in methods
        v = r.get('verdict')
        if v == 'needs_card':
            if is_impl:
                errs.append('%s 判「需开卡」但 %s 已覆写（npc.rs:%d）' % (tag, host_name, line_of[name]))
        elif v == 'covered':
            if r.get('card') not in KNOWN_CARDS:
                errs.append('%s covered 指向未知卡: %r' % (tag, r.get('card')))
            if is_impl:
                errs.append('%s covered 但 %s 已覆写 ⇒ 不该再挂在在途卡下' % (tag, host_name))
        else:  # safe
            if r.get('host_overridden'):
                if not is_impl:
                    errs.append('%s 理由称「已覆写」但 %s impl 块里没有该方法' % (tag, host_name))
                ev = r.get('evidence') or []
                if not ev:
                    errs.append('%s 理由称「已覆写」但缺 evidence' % tag)
                for e in ev:
                    em = re.match(r'^(.*npc\.rs):(\d+)$', e)
                    if not em:
                        errs.append('%s evidence 格式非法: %r' % (tag, e))
                        continue
                    p = os.path.join(ROOT, em.group(1))
                    if not os.path.isfile(p):
                        errs.append('%s evidence 文件不存在: %s' % (tag, em.group(1)))
                        continue
                    el = read_lines(p)
                    ln = int(em.group(2))
                    if ln < 1 or ln > len(el) or not re.search(r'\bfn\s+' + re.escape(name) + r'\b', el[ln - 1]):
                        errs.append('%s evidence 行不含 fn 声明: %s' % (tag, e))
            else:
                if len((r.get('reason') or '')) < 30:
                    errs.append('%s 判安全但理由过短（需给出等价证明）' % tag)

        # ⑦ 子卡
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


def cmd_check(doc_path=LEDGER):
    global LEDGER
    old = LEDGER
    LEDGER = doc_path
    try:
        try:
            doc = load()
        except Exception as e:  # noqa: BLE001
            print('RED: 台账读取失败: %s' % e)
            return 1
        errs = validate(doc)
    finally:
        LEDGER = old
    if errs:
        print('RED: %d 处不一致' % len(errs))
        for e in errs[:50]:
            print('  - ' + e)
        if len(errs) > 50:
            print('  ... 其余 %d 条省略' % (len(errs) - 50))
        return 1
    rows = doc['rows']
    print('GREEN: %d 行台账全部可核（safe %d / needs_card %d / covered %d）' % (
        len(rows), sum(1 for r in rows if r['verdict'] == 'safe'),
        sum(1 for r in rows if r['verdict'] == 'needs_card'),
        sum(1 for r in rows if r['verdict'] == 'covered')))
    return 0


def mutate_tooth1(doc):
    """牙①：把一条 safe 行的 oracle 证据改成不存在的文件/越界行号/不含引文的行。"""
    target = next(r for r in doc['rows'] if r['verdict'] == 'safe' and r['overridden'])
    target['oracle'] = {
        'file': 'src/Modules/ScriptEngine/__no_such_file__.cs',
        'line': 999999,
        'text': 'fn %s' % target['name'],
    }
    target['name'] = target['name']  # 保持名字，故意只坏证据
    return '牙①：%s 的 oracle 改成 src/.../__no_such_file__.cs:999999' % target['name']


def mutate_tooth2(doc):
    """牙②：把一条 needs_card（未覆写）行谎报成「safe：已覆写」。"""
    target = next(r for r in doc['rows'] if r['verdict'] == 'needs_card' and r['name'] == 'space_move')
    target['verdict'] = 'safe'
    target['reason'] = 'NpcPlayer 已在 npc.rs 覆写，缺省实现不被使用。'
    target['evidence'] = ['mir2-rs/bins/game-svr/src/npc.rs:1049']
    target['host_overridden'] = True
    # 同步调账计数，确保牙齿红在「覆写矛盾」本身，而不是被计数门先挡下
    doc['counts']['safe'] += 1
    doc['counts']['needs_card'] -= 1
    return '牙②：把 space_move 改判「safe：已覆写」（npc.rs:1049）'


def run_case(name, mutate):
    doc = json.loads(json.dumps(load()))
    how = mutate(doc)
    tmp = os.path.join(HERE, '__tooth_tmp__.json')
    try:
        with open(tmp, 'w', encoding='utf-8') as f:
            json.dump(doc, f, ensure_ascii=False)
        errs = validate(doc)
    finally:
        if os.path.exists(tmp):
            os.remove(tmp)
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
    rc1 = run_case('tooth1', mutate_tooth1)
    rc2 = run_case('tooth2', mutate_tooth2)
    after = sha256(LEDGER)
    print('== 台账 sha256（运行后）: %s' % after)
    if after != before:
        print('RED: 真实台账被改动（sha 变化）')
        return 1
    print('== 还原证明: sha256 前后一致（%s）' % before)
    ok = (rc1 == 0 and rc2 == 0)
    print('== selftest %s' % ('GREEN' if ok else 'RED'))
    return 0 if ok else 1


def main(argv):
    cmd = argv[1] if len(argv) > 1 else 'check'
    if cmd == 'check':
        return cmd_check()
    if cmd == 'selftest':
        return cmd_selftest()
    print(__doc__)
    return 2


if __name__ == '__main__':
    sys.exit(main(sys.argv))
