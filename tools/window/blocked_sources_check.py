#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""blocked_sources_check.py — run3 的 blocked **两来源一致性**机判（只读；不起服务端、不占端口）。

背景（卡 `openmir2-run3-blocked-text-sync`，audit `docs/blocked-scenarios-audit-20261025.md` §3.2）：

`blocked` 有两个来源，各自管一件事——
  * **清单行** `tools/window/run3-scenarios.json` 的 `blocked`：管**批次排除**（`run_batch.sh` 只看这一处）；
  * **场景 JSON** `tools/probe/scenarios/<name>.json` 的 `blocked`：管**直接跑 card_window** 时的人工口径。
只改一处 = 两处漂移：批次里照样跳过、文档上却写「可跑」，或反过来把注定失败的场景放进批次。
本工具把「两处要么同时有、要么同时没有」变成可机判。

用法：
  python -I -B tools/window/blocked_sources_check.py check
  python -I -B tools/window/blocked_sources_check.py check --list-output E:/tmp/run3-list.txt
  python -I -B tools/window/blocked_sources_check.py selftest

`--list-output` = 先 `bash tools/window/run_batch.sh --list > <file>` 再喂进来：额外要求
**`--list` 标 YES 的场景集合 == 场景 JSON 里 blocked 为真的集合**。

退出码（钉死）：
  0 = 两来源一致；
  1 = 有漂移（点名场景；含「blocked 文案为空/缺 reason」）；
  2 = 用法错误 / 文件读不了 / 清单行找不到对应场景 JSON（fail-closed，不猜）。
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
    """场景 JSON blocked：必须是非空对象且带非空 reason。"""
    return isinstance(scene.get('blocked'), dict) and bool(scene['blocked'])


def scene_reason_ok(scene):
    b = scene.get('blocked')
    if not isinstance(b, dict):
        return False
    reason = b.get('reason')
    return isinstance(reason, str) and bool(reason.strip())


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


def check(run3_path, scenes_dir, list_output=None):
    """返回 (problems, info)。problems 非空 ⇒ rc=1。"""
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
        if rb:
            # 两处都 blocked：还要求两处都写清原因（空文案 = 漂移的另一种形态）
            if not isinstance(row.get('blocked'), str) or not row['blocked'].strip():
                problems.append('%s：清单行 blocked 为真但不是非空字符串' % name)
            if not scene_reason_ok(scene):
                problems.append('%s：场景 JSON blocked 缺非空 reason' % name)
    if list_output:
        lrows, lblocked = parse_list_output(list_output)
        listed = set(lrows.values())
        if listed != {r['name'] for r in rows}:
            problems.append('--list 的场景集合与清单不同：只在 --list=%s，只在清单=%s'
                            % (sorted(listed - {r['name'] for r in rows}),
                               sorted({r['name'] for r in rows} - listed)))
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

        def write(rows, scene_block):
            json.dump({'format': 'run3/1', 'scenarios': rows},
                      open(run3, 'w', encoding='utf-8'), ensure_ascii=False)
            for row in rows:
                name = row['name']
                sdoc = {'name': name, 'steps': []}
                if name in scene_block:
                    sdoc['blocked'] = {'reason': '人工原因 %s' % name,
                                       'missing_fixture': '夹具'}
                json.dump(sdoc, open(os.path.join(scenes, name + '.json'), 'w',
                                     encoding='utf-8'), ensure_ascii=False)

        row_b = {'name': 'a', 'blocked': '清单原因 a'}
        row_ok = {'name': 'b'}
        row_b2 = {'name': 'c', 'blocked': '清单原因 c'}

        # 牙 a：只改清单行（清单有、场景无）⇒ 红
        write([row_b, row_ok], scene_block=set())
        probs, _ = check(run3, scenes)
        chk(any('a' in p and '不一致' in p for p in probs),
            '牙a：只在清单行写 blocked ⇒ 红')
        # 牙 b：只改场景 JSON（场景有、清单无）⇒ 红
        write([{'name': 'a'}, row_ok], scene_block={'a'})
        probs, _ = check(run3, scenes)
        chk(any('a' in p and '不一致' in p for p in probs),
            '牙b：只在场景 JSON 写 blocked ⇒ 红')
        # 牙 c：两处都写 ⇒ 绿
        write([row_b, row_ok], scene_block={'a'})
        probs, _ = check(run3, scenes)
        chk(probs == [], '牙c：两处都写 ⇒ 绿')
        # 两处都没有 ⇒ 绿
        write([row_ok], scene_block=set())
        probs, _ = check(run3, scenes)
        chk(probs == [], '两处都没有 ⇒ 绿')
        # 场景 JSON blocked 缺 reason ⇒ 红（空文案也是一种漂移）
        write([row_b2], scene_block=set())
        doc = json.load(open(os.path.join(scenes, 'c.json'), encoding='utf-8'))
        doc['blocked'] = {'missing_fixture': 'x'}
        json.dump(doc, open(os.path.join(scenes, 'c.json'), 'w', encoding='utf-8'),
                  ensure_ascii=False)
        probs, _ = check(run3, scenes)
        chk(any('c' in p and 'reason' in p for p in probs),
            '场景 JSON blocked 缺 reason ⇒ 红')
        # 清单行找不到场景 JSON ⇒ UsageError（rc=2，不猜）
        write([{'name': 'a', 'blocked': 'x'}], scene_block=set())
        try:
            check(run3, os.path.join(tmp, 'nope'))
            chk(False, '缺场景 JSON ⇒ rc=2')
        except UsageError:
            chk(True, '缺场景 JSON ⇒ rc=2')
        # --list 输出：YES 集合 != 场景 JSON blocked 集合 ⇒ 红
        write([row_ok], scene_block=set())
        lop = os.path.join(tmp, 'list.txt')
        open(lop, 'w', encoding='utf-8').write(
            '  1  b            alt           YES     \n场景数 1（blocked 1 ⇒ 入批 0）\n')
        probs, _ = check(run3, scenes, lop)
        chk(any('--list' in p and 'blocked' in p for p in probs),
            '--list 的 YES 集合 != 场景 JSON blocked ⇒ 红')
        open(lop, 'w', encoding='utf-8').write(
            '  1  b            alt           -       \n场景数 1（blocked 0 ⇒ 入批 1）\n')
        probs, _ = check(run3, scenes, lop)
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
