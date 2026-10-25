#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""gm_gate_check.py — `@Level0` 权限门等价判据的机判器（卡 `openmir2-gm-target-state-write`）。

只读：读 `<run>/<side>/frames.ndjson` 与两份 DB 快照 JSON，**不起栈、不连库**。

三条判据（逐侧）：
  G1  恰 1 条 `SM_SYSMESSAGE(100)`：`Param = MakeWord(255,56) = 65336`、正文含「权限不够!!!」；
  G2  **零** 帧：`SM_ADDITEM(200)` / `SM_DELITEM(202)` / `SM_WEIGHTCHANGED(622)` /
      `SM_LEVELUP(45)` / `SM_ABILITY(52)` / `SM_SUBABILITY(752)`；
  G3  跑前后 `characters_ablity.Level` 逐字节相同（快照格式不认 ⇒ UNDECIDABLE，不猜）。

用法：
  python -I -B tools/window/gm_gate_check.py check --run-dir E:/tmp/window/gm-gate \
      [--before E:/tmp/window/gm-gate/db_before.json --after E:/tmp/window/gm-gate/db_after.json]
  python -I -B tools/window/gm_gate_check.py selftest

退出码：0 = 全部 PASS；1 = 有 FAIL；2 = 有 UNDECIDABLE（fail-closed，不当作通过）。
"""
import argparse
import io
import json
import os
import sys

SM_SYS = 100
FORBIDDEN = (200, 202, 622, 45, 52, 752)
PERM_TEXT = '权限不够!!!'
RED_PARAM = (255 << 8) | 56  # MakeWord(RedMsgFColor 255, RedMsgBColor 56)


def load_frames(path):
    out = []
    with io.open(path, encoding='utf-8') as fh:
        for line in fh:
            line = line.strip()
            if line:
                out.append(json.loads(line))
    return out


def ident_of(fr):
    return fr.get('ident') if fr.get('ident') is not None else fr.get('id')


def body_text_of(fr):
    return fr.get('body_text') or fr.get('text') or ''


def check_side(frames):
    """返回 [(item, verdict, detail)]，verdict ∈ PASS/FAIL/UNDECIDABLE。"""
    res = []
    sys100 = [f for f in frames if ident_of(f) == SM_SYS]
    ok_cnt = len(sys100) == 1
    ok_par = ok_cnt and sys100[0].get('param') == RED_PARAM
    # 正文：没有体文本就判不了（不猜）
    text = body_text_of(sys100[0]) if ok_cnt else ''
    ok_txt = (PERM_TEXT in text) if text else None
    if not ok_cnt:
        res.append(('G1', 'FAIL', 'ident=100 出现 %d 次（期望恰 1）' % len(sys100)))
    elif not ok_par:
        res.append(('G1', 'FAIL', '100.Param=%r ≠ %d（MakeWord(255,56)）' % (sys100[0].get('param'), RED_PARAM)))
    elif ok_txt is None:
        res.append(('G1', 'UNDECIDABLE', 'frames.ndjson 无 body_text，判不了正文'))
    elif not ok_txt:
        res.append(('G1', 'FAIL', '100 正文不含「%s」（实得 %r）' % (PERM_TEXT, text[:60])))
    else:
        res.append(('G1', 'PASS', '恰 1 条 100，Param=%d，正文含「%s」' % (RED_PARAM, PERM_TEXT)))

    bad = [(ident_of(f), i) for i, f in enumerate(frames) if ident_of(f) in FORBIDDEN]
    if bad:
        res.append(('G2', 'FAIL', '出现禁用帧 %r' % (bad[:5],)))
    else:
        res.append(('G2', 'PASS', '零 %r' % (FORBIDDEN,)))
    return res


def _level_of(snap, chr_name):
    """在 dbsnap 里找 `characters_ablity.Level`；认不出结构就返回 None（⇒ UNDECIDABLE）。"""
    if not isinstance(snap, dict):
        return None
    for key in ('characters_ablity', 'CharactersAblity', 'ablity'):
        rows = snap.get(key)
        if isinstance(rows, list):
            for r in rows:
                if not isinstance(r, dict):
                    continue
                if chr_name is None or r.get('ChrName') == chr_name:
                    for col in ('Level', 'level'):
                        if col in r:
                            return r[col]
    return None


def check_g3(before_path, after_path, chr_name=None):
    if not (before_path and after_path):
        return ('G3', 'UNDECIDABLE', '未提供 --before/--after 快照')
    try:
        b = json.load(io.open(before_path, encoding='utf-8'))
        a = json.load(io.open(after_path, encoding='utf-8'))
    except (OSError, ValueError) as ex:
        return ('G3', 'UNDECIDABLE', '快照读不了：%s' % ex)
    lb, la = _level_of(b, chr_name), _level_of(a, chr_name)
    if lb is None or la is None:
        return ('G3', 'UNDECIDABLE', '快照里找不到 characters_ablity.Level（格式不认，不猜）')
    if lb == la:
        return ('G3', 'PASS', 'Level 跑前=跑后=%r' % (lb,))
    return ('G3', 'FAIL', 'Level 跑前=%r 跑后=%r' % (lb, la))


def cmd_check(args):
    rc = 0
    for side in ('csharp', 'rust'):
        path = os.path.join(args.run_dir, side, 'frames.ndjson')
        print('== %s' % side)
        if not os.path.isfile(path):
            print('  G1 UNDECIDABLE 缺 frames.ndjson：%s' % path)
            print('  G2 UNDECIDABLE 缺 frames.ndjson')
            rc = max(rc, 2)
            continue
        for item, verdict, detail in check_side(load_frames(path)):
            print('  %s %s %s' % (item, verdict, detail))
            rc = max(rc, {'PASS': 0, 'FAIL': 1, 'UNDECIDABLE': 2}[verdict])
    item, verdict, detail = check_g3(args.before, args.after, args.chr_name)
    print('== db\n  %s %s %s' % (item, verdict, detail))
    rc = max(rc, {'PASS': 0, 'FAIL': 1, 'UNDECIDABLE': 2}[verdict])
    return rc


def selftest():
    def f(ident, param=0, text=''):
        return {'ident': ident, 'param': param, 'body_text': text}

    # (名字, frames, 该红的 item, 期望 verdict)
    cases = [
        ('positive', [f(100, RED_PARAM, PERM_TEXT), f(10), f(41)], 'G1', 'PASS'),
        ('positive-g2', [f(100, RED_PARAM, PERM_TEXT), f(10), f(41)], 'G2', 'PASS'),
        ('two-sysmsg', [f(100, RED_PARAM, PERM_TEXT), f(100, RED_PARAM, PERM_TEXT)], 'G1', 'FAIL'),
        # 出现 45 = 命令体被执行了 ⇒ **G1 仍应 PASS、G2 必须 FAIL**（两者互不替代）
        ('has-45', [f(100, RED_PARAM, PERM_TEXT), f(45)], 'G2', 'FAIL'),
        ('has-45-g1', [f(100, RED_PARAM, PERM_TEXT), f(45)], 'G1', 'PASS'),
        ('bad-color', [f(100, (219 << 8) | 255, PERM_TEXT)], 'G1', 'FAIL'),
        ('bad-text', [f(100, RED_PARAM, '未知命令: Level0 5')], 'G1', 'FAIL'),
        ('no-text', [f(100, RED_PARAM, '')], 'G1', 'UNDECIDABLE'),
    ]

    ok = True
    for name, frames, item, want in cases:
        res = check_side(frames)
        got = [r for r in res if r[0] == item][0]
        good = got[1] == want
        print('  selftest %-12s %s  (%s %s %s)' % (name, 'OK' if good else 'MISMATCH', item, got[1], got[2][:40]))
        ok = ok and good

    # G3：正/反/认不出
    import tempfile
    td = tempfile.mkdtemp(prefix='gm_gate_selftest_')
    pa = os.path.join(td, 'a.json')
    pb = os.path.join(td, 'b.json')
    pc = os.path.join(td, 'c.json')
    open(pa, 'w', encoding='utf-8').write(json.dumps({'characters_ablity': [{'ChrName': 'x', 'Level': 5}]}))
    open(pb, 'w', encoding='utf-8').write(json.dumps({'characters_ablity': [{'ChrName': 'x', 'Level': 6}]}))
    open(pc, 'w', encoding='utf-8').write(json.dumps({'unknown': 1}))
    for name, a, b, want in (('same', pa, pa, 'PASS'),
                             ('diff', pa, pb, 'FAIL'),
                             ('unknown', pa, pc, 'UNDECIDABLE')):
        item, verdict, detail = check_g3(a, b)
        good = verdict == want
        print('  selftest g3-%-8s %s  (%s)' % (name, 'OK' if good else 'MISMATCH', verdict))
        ok = ok and good
    print('SELFTEST %s' % ('PASS' if ok else 'FAIL'))
    return 0 if ok else 1


def main(argv=None):
    ap = argparse.ArgumentParser(description='@Level0 权限门等价判据机判器')
    sub = ap.add_subparsers(dest='cmd', required=True)
    p = sub.add_parser('check')
    p.add_argument('--run-dir', required=True)
    p.add_argument('--before')
    p.add_argument('--after')
    p.add_argument('--chr-name', dest='chr_name')
    sub.add_parser('selftest')
    args = ap.parse_args(argv)
    if args.cmd == 'selftest':
        return selftest()
    return cmd_check(args)


if __name__ == '__main__':
    sys.exit(main())
