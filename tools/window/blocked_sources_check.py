#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""blocked_sources_check.py — run3 的 blocked **两来源一致性**机判（只读；不起服务端、不占端口）。

背景（卡 `openmir2-run3-blocked-text-sync`，reviewer-20 的 F1）：`blocked` 有两个来源——
  * **清单行** `tools/window/run3-scenarios.json` 的 `blocked`（管**批次排除**）；
  * **场景 JSON** `tools/probe/scenarios/<name>.json` 的 `blocked`（管**直接跑 card_window** 的人工口径）。

只比「两处是不是都 blocked」是**假绿**：两套文案本来就是两种措辞，只改内容不改真假时门禁仍绿。
所以本工具用一张**断言 token 表**（`BLOCKED_TOKENS`，每项带 source 锚）做确定性判据：
**每条 blocked 场景的那组 token 必须同时出现在「清单行文案」与「场景 JSON 文案」里**。

- 表项自带 `source` 锚（哪条 audit 裁定要求的），表本身也要被审查，不是第二份散文；
- **自一致性**：`check` 就是在验「表里的 token 在当前两侧文案里真实出现」——表与文案一起漂（改数字又顺手改表）时，
  若 token 不再出现在文案里，仍会红；
- **fail-closed**：出现未在表里登记的 blocked 场景 ⇒ **rc=2 并点名**（新场景不得绕过检查）。

用法：
  python -I -B tools/window/blocked_sources_check.py check
  python -I -B tools/window/blocked_sources_check.py check --list-output E:/tmp/run3-list.txt
  python -I -B tools/window/blocked_sources_check.py selftest

`--list-output` = 先 `bash tools/window/run_batch.sh --list > <file>` 再喂进来：额外要求
**`--list` 标 YES 的场景集合 == 场景 JSON 里 blocked 为真的集合**。

退出码（钉死）：
  0 = 两来源一致（真/假相同 + 断言 token 两侧齐 + --list 集合一致）；
  1 = 有漂移（点名场景：真假不一致 / 文案缺 token / 缺 reason / --list 集合不符）；
  2 = 用法错误 / 文件读不了 / 清单行找不到场景 JSON / **blocked 场景没有断言表条目**（fail-closed）。
"""
from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import sys
import tempfile

DEFAULT_RUN3 = os.path.join('tools', 'window', 'run3-scenarios.json')
DEFAULT_SCENES = os.path.join('tools', 'probe', 'scenarios')
LIST_ROW_RE = re.compile(r'^\s*(\d+)\s+(\S+)\s+(\S+)\s+(YES|-)\s*(.*)$')

#: 每条 blocked 场景**必须**在两侧文案里同时出现的机器可核 token（确定性、无阈值、不要求逐字）。
#: 加场景必须同时加一条，并写清 source（audit 报告的章节锚）——否则 `check` rc=2。
BLOCKED_TOKENS = {
    'butch_two_draws': {
        'source': 'docs/blocked-scenarios-audit-20261025.md §1.2',
        'tokens': ('3081', '616', 'ProcessMonsterMultiThreadLimit'),
    },
    'castle_merchant_tax': {
        'source': 'docs/blocked-scenarios-audit-20261025.md §2.2/§2.3',
        'tokens': ('652', '原因未定位', '已证伪'),
    },
    'castle_merchant_tax_upgrade': {
        'source': 'docs/blocked-scenarios-audit-20261025.md §3.2/§3.3',
        'tokens': ('已证伪', 'NPC'),
    },
}


class UsageError(Exception):
    pass


def load_run3(path):
    try:
        doc = json.load(open(path, encoding='utf-8'))
    except OSError as ex:
        raise UsageError('读不到清单 %s：%s' % (path, ex))
    except ValueError as ex:
        raise UsageError('清单 %s 不是合法 JSON：%s' % (path, ex))
    rows = doc.get('scenarios') if isinstance(doc, dict) else doc
    if not isinstance(rows, list) or not rows:
        raise UsageError('清单 %s 里没有 scenarios 数组' % path)
    for i, r in enumerate(rows):
        if not isinstance(r, dict) or not isinstance(r.get('name'), str) or not r['name']:
            raise UsageError('清单第 %d 行缺 name（fail-closed）' % (i + 1))
    return rows


def load_scene(scenes_dir, name):
    path = os.path.join(scenes_dir, name + '.json')
    try:
        return json.load(open(path, encoding='utf-8'))
    except OSError:
        raise UsageError('清单行 %r 找不到场景 JSON：%s（fail-closed，不猜）' % (name, path))
    except ValueError as ex:
        raise UsageError('场景 JSON %s 不是合法 JSON：%s' % (path, ex))


def row_blocked(row):
    """清单行 blocked：非空字符串/非空对象才算真。"""
    b = row.get('blocked')
    if isinstance(b, str):
        return bool(b.strip())
    if isinstance(b, dict):
        return bool(b)
    return bool(b)


def scene_blocked(scene):
    """场景 JSON blocked：必须是非空对象。"""
    return isinstance(scene.get('blocked'), dict) and bool(scene['blocked'])


def token_present(text, tok):
    """token 是否出现在文案里。

    **数字 token 用词边界**（`(?<![0-9])616(?![0-9])`）：`616` 不能被 `6160` 命中、`652` 不能被 `1652`、
    `3081` 不能被 `30810` ——否则数字一漂（多一位/少一位）门禁仍绿。非数字 token 保持**子串**语义
    （长标识符里出现即算，如 `ProcessMonsterMultiThreadLimit`）。
    """
    if tok.isdigit():
        return re.search(r"(?<![0-9])%s(?![0-9])" % re.escape(tok), text) is not None
    return tok in text


def scene_text(scene):
    b = scene.get('blocked') or {}
    return '%s\n%s' % (b.get('reason') or '', b.get('missing_fixture') or '')


def scene_reason_ok(scene):
    b = scene.get('blocked')
    if not isinstance(b, dict):
        return False
    reason = b.get('reason')
    return isinstance(reason, str) and bool(reason.strip())


def validate_tokens_table(tokens):
    if not isinstance(tokens, dict) or not tokens:
        raise UsageError('断言 token 表为空（fail-closed）')
    for name, entry in tokens.items():
        if not isinstance(entry, dict):
            raise UsageError('断言表 %r 必须是对象' % name)
        toks = entry.get('tokens')
        src = entry.get('source')
        if not isinstance(toks, (list, tuple)) or not toks or any(
                not isinstance(t, str) or not t for t in toks):
            raise UsageError('断言表 %r.tokens 必须是非空字符串数组' % name)
        if not isinstance(src, str) or not src.strip():
            raise UsageError('断言表 %r 缺 source 锚（表也要可追溯）' % name)


def parse_list_output(path):
    """解析 `run_batch.sh --list`：返回 (rows{n:name}, blocked_names set)。"""
    try:
        text = open(path, encoding='utf-8', errors='replace').read()
    except OSError as ex:
        raise UsageError('读不到 --list 输出 %s：%s' % (path, ex))
    rows = {}
    blocked = set()
    for line in text.splitlines():
        m = LIST_ROW_RE.match(line)
        if not m:
            continue
        n, name, _prof, yesno = int(m.group(1)), m.group(2), m.group(3), m.group(4)
        rows[n] = name
        if yesno == 'YES':
            blocked.add(name)
    if not rows:
        raise UsageError('--list 输出里没有解析到任何场景行（fail-closed）：%s' % path)
    return rows, blocked


def check(run3_path, scenes_dir, list_output=None, tokens=None):
    """返回 (problems, info)。problems 非空 ⇒ rc=1；UsageError ⇒ rc=2。"""
    tokens = BLOCKED_TOKENS if tokens is None else tokens
    validate_tokens_table(tokens)
    rows = load_run3(run3_path)
    problems = []
    info = {'rows': len(rows), 'blocked_run3': [], 'blocked_scene': []}
    for row in rows:
        name = row['name']
        scene = load_scene(scenes_dir, name)
        rb = row_blocked(row)
        sb = scene_blocked(scene)
        if rb:
            info['blocked_run3'].append(name)
        if sb:
            info['blocked_scene'].append(name)
        if rb != sb:
            problems.append(
                '%s：两来源不一致（清单行 blocked=%s，场景 JSON blocked=%s）'
                % (name, '真' if rb else '假', '真' if sb else '假'))
            continue
        if not rb:
            continue
        if not isinstance(row.get('blocked'), str) or not row['blocked'].strip():
            problems.append('%s：清单行 blocked 为真但不是非空字符串' % name)
        if not scene_reason_ok(scene):
            problems.append('%s：场景 JSON blocked 缺非空 reason' % name)
            continue
        entry = tokens.get(name)
        if entry is None:
            raise UsageError(
                '%s：清单与场景都 blocked，但**没有断言表条目**（fail-closed：新 blocked 场景必须在 '
                'blocked_sources_check.BLOCKED_TOKENS 登记 tokens + source，不得绕过检查）' % name)
        row_text = row.get('blocked') or ''
        stext = scene_text(scene)
        for tok in entry['tokens']:
            where = []
            if not token_present(row_text, tok):
                where.append('清单行')
            if not token_present(stext, tok):
                where.append('场景 JSON')
            if where:
                problems.append(
                    '%s：断言 token %r 未出现在 %s（断言表 source=%s；两侧文案都必须含齐）'
                    % (name, tok, '+'.join(where), entry['source']))
    registered = {n for n in info['blocked_scene']}
    unregistered = sorted(registered - set(tokens))
    if unregistered:  # 兜底：真/假不一致的已在上文 skip，不会走到这里漏判
        raise UsageError('以下 blocked 场景没有断言表条目：%s（fail-closed）' % unregistered)
    if list_output:
        lrows, lblocked = parse_list_output(list_output)
        listed = set(lrows.values())
        names = {r['name'] for r in rows}
        if listed != names:
            problems.append('--list 的场景集合与清单不同：只在 --list=%s，只在清单=%s'
                            % (sorted(listed - names), sorted(names - listed)))
        scene_set = set(info['blocked_scene'])
        if lblocked != scene_set:
            problems.append('--list 的 blocked 集合 != 场景 JSON 的 blocked 集合：'
                            '只在 --list=%s，只在场景 JSON=%s'
                            % (sorted(lblocked - scene_set), sorted(scene_set - lblocked)))
    return problems, info


# ---------------------------------------------------------------- selftest
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

    tmp = tempfile.mkdtemp(prefix='blocked-sources-selftest.')
    try:
        scenes = os.path.join(tmp, 'scenarios')
        os.makedirs(scenes)
        run3 = os.path.join(tmp, 'run3.json')
        table = {'a': {'source': 'audit §1', 'tokens': ('TOK1', 'TOK2')}}
        row_b = {'name': 'a', 'blocked': '清单：TOK1 与 TOK2 都在'}
        row_ok = {'name': 'b'}

        def write(rows, scene_block, reason='场景：TOK1 TOK2 都在'):
            json.dump({'format': 'run3/1', 'scenarios': rows},
                      open(run3, 'w', encoding='utf-8'), ensure_ascii=False)
            for row in rows:
                name = row['name']
                sdoc = {'name': name, 'steps': []}
                if name in scene_block:
                    sdoc['blocked'] = {'reason': reason, 'missing_fixture': '夹具'}
                json.dump(sdoc, open(os.path.join(scenes, name + '.json'), 'w',
                                     encoding='utf-8'), ensure_ascii=False)

        def run(rows, scene_block, reason='场景：TOK1 TOK2 都在', table_=None, extra_rows=None,
                list_path=None):
            write(rows, scene_block, reason)
            try:
                return check(run3, scenes, list_path, table_ if table_ is not None else table)
            except UsageError as ex:
                return ('USAGE', str(ex))

        # 牙 a：两处都 blocked，**只把场景 JSON 文案换成完全不同内容** ⇒ 必红（reviewer-20 的复现）
        probs, _ = run([row_b, row_ok], {'a'}, reason='此场景其实可跑（DRIFT 演示）')
        chk(any("'TOK1'" in p and '场景 JSON' in p for p in probs),
            '牙a：两处都 blocked 但场景文案完全不同 ⇒ 红（原假绿）')
        # 牙 b：只删清单行 ⇒ 红
        probs, _ = run([{'name': 'a'}, row_ok], {'a'})
        chk(any('a' in p and '不一致' in p for p in probs), '牙b：只删清单行 ⇒ 红')
        # 牙 c：只删场景 JSON ⇒ 红
        probs, _ = run([row_b, row_ok], set())
        chk(any('a' in p and '不一致' in p for p in probs), '牙c：只删场景 JSON ⇒ 红')
        # 牙 d：两处都删 ⇒ 绿
        probs, _ = run([row_ok, {'name': 'c'}], set())
        chk(probs == [], '牙d：两处都不 blocked ⇒ 绿')
        # 牙 e：新增 blocked 场景但表里没有 ⇒ rc=2 并点名
        out = run([row_b, {'name': 'c', 'blocked': '清单：TOK1 TOK2'}], {'a', 'c'})
        chk(isinstance(out, tuple) and out[0] == 'USAGE' and 'c' in out[1],
            '牙e：未登记的 blocked 场景 ⇒ rc=2 点名')
        # 牙 f：表里的 token 从两侧文案里都删掉（表不改）⇒ 红（自一致性）
        table2 = {'a': {'source': 'audit §1', 'tokens': ('TOK1', 'GONE')}}
        probs, _ = run([{'name': 'a', 'blocked': '清单：TOK1 在'}, row_ok], {'a'},
                       reason='场景：TOK1 在', table_=table2)
        chk(any("'GONE'" in p for p in probs), '牙f：表 token 与两侧文案不一致 ⇒ 红')
        # 牙 g：数字 token 词边界（616 不应被 6160 命中）
        table3 = {'a': {'source': 'audit §1', 'tokens': ('616', 'TOKX')}}
        probs, _ = run([{'name': 'a', 'blocked': '清单：TOKX 在，数 6160'}, row_ok], {'a'},
                       reason='场景：TOKX 在，数 6160', table_=table3)
        chk(any("'616'" in p for p in probs), '牙g：token 616 不被 6160 命中 ⇒ 红')
        probs, _ = run([{'name': 'a', 'blocked': '清单：TOKX 在，数 616'}, row_ok], {'a'},
                       reason='场景：TOKX 在，数 616', table_=table3)
        chk(probs == [], '牙g-对照：token 616 正常出现 ⇒ 绿')
        # 牙 h：3081 vs 30810
        table4 = {'a': {'source': 'audit §1', 'tokens': ('3081',)}}
        probs, _ = run([{'name': 'a', 'blocked': '清单：30810'}, row_ok], {'a'},
                       reason='场景：30810', table_=table4)
        chk(any("'3081'" in p for p in probs), '牙h：token 3081 不被 30810 命中 ⇒ 红')
        # 牙 i：652 vs 1652
        table5 = {'a': {'source': 'audit §1', 'tokens': ('652',)}}
        probs, _ = run([{'name': 'a', 'blocked': '清单：1652'}, row_ok], {'a'},
                       reason='场景：1652', table_=table5)
        chk(any("'652'" in p for p in probs), '牙i：token 652 不被 1652 命中 ⇒ 红')
        # 非数字 token 仍是子串语义：含在长标识符里也算
        table6 = {'a': {'source': 'audit §1', 'tokens': ('ProcessMonsterMultiThreadLimit',)}}
        probs, _ = run([{'name': 'a', 'blocked': '清单：x ProcessMonsterMultiThreadLimit x'},
                        row_ok], {'a'},
                       reason='场景：x ProcessMonsterMultiThreadLimit x', table_=table6)
        chk(probs == [], '非数字 token 维持子串语义 ⇒ 绿')
        # 表项缺 source ⇒ rc=2
        out = run([row_b, row_ok], {'a'},
                  table_={'a': {'tokens': ('TOK1',)}})
        chk(isinstance(out, tuple) and out[0] == 'USAGE' and 'source' in out[1],
            '断言表项缺 source ⇒ rc=2')
        # 场景 JSON blocked 缺 reason ⇒ 红
        probs, _ = run([{'name': 'a', 'blocked': 'TOK1 TOK2'}], set())
        sdoc = json.load(open(os.path.join(scenes, 'a.json'), encoding='utf-8'))
        sdoc['blocked'] = {'missing_fixture': 'x'}
        json.dump(sdoc, open(os.path.join(scenes, 'a.json'), 'w', encoding='utf-8'),
                  ensure_ascii=False)
        try:
            probs, _ = check(run3, scenes, None, table)
        except UsageError as ex:  # 缺 reason 时仍应给出 problem 而不是异常：这里要红
            probs = ['USAGE ' + str(ex)]
        chk(any('reason' in p for p in probs), '场景 JSON blocked 缺 reason ⇒ 红')
        # 缺场景 JSON ⇒ rc=2
        write([{'name': 'a', 'blocked': 'x'}], set())
        try:
            check(run3, os.path.join(tmp, 'nope'), None, table)
            chk(False, '缺场景 JSON ⇒ rc=2')
        except UsageError:
            chk(True, '缺场景 JSON ⇒ rc=2')
        # --list 输出：YES 集合 != 场景 JSON blocked 集合 ⇒ 红
        write([row_ok], set())
        lop = os.path.join(tmp, 'list.txt')
        open(lop, 'w', encoding='utf-8').write(
            '  1  b            alt           YES     \n场景数 1（blocked 1 ⇒ 入批 0）\n')
        out = run([row_ok], set(), list_path=lop)
        probs = out[0] if isinstance(out, tuple) else out
        chk(any('--list' in p and 'blocked' in p for p in probs),
            '--list 的 YES 集合 != 场景 JSON blocked ⇒ 红')
        open(lop, 'w', encoding='utf-8').write(
            '  1  b            alt           -       \n场景数 1（blocked 0 ⇒ 入批 1）\n')
        probs, _ = run([row_ok], set(), list_path=lop)
        chk(probs == [], '--list 与场景 JSON 都不 blocked ⇒ 绿')
    finally:
        shutil.rmtree(tmp, ignore_errors=True)

    print('%s: blocked_sources_check selftest %d/%d'
          % ('GREEN' if not fail else 'RED', ok, ok + fail))
    return 0 if not fail else 1


# ---------------------------------------------------------------- main
def main(argv=None):
    ap = argparse.ArgumentParser(description='run3 blocked 两来源一致性机判（只读）')
    ap.add_argument('cmd', choices=('check', 'selftest'))
    ap.add_argument('--run3', default=DEFAULT_RUN3)
    ap.add_argument('--scenarios-dir', default=DEFAULT_SCENES)
    ap.add_argument('--list-output', help='`run_batch.sh --list` 的输出文件（可选）')
    args = ap.parse_args(argv)
    if args.cmd == 'selftest':
        return selftest()
    try:
        problems, info = check(args.run3, args.scenarios_dir, args.list_output)
    except UsageError as ex:
        print('blocked_sources_check: %s' % ex, file=sys.stderr)
        return 2
    for p in problems:
        print('DRIFT  %s' % p)
    print('清单 %s：%d 行，blocked %d；场景 JSON blocked %d'
          % (args.run3, info['rows'], len(info['blocked_run3']), len(info['blocked_scene'])))
    print('blocked_sources_check: %s' % ('OK（两来源一致）' if not problems
                                        else 'DRIFT %d 处' % len(problems)))
    return 1 if problems else 0


if __name__ == '__main__':
    sys.exit(main())
