#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""gm_gate_check.py — `@Level0` 权限门等价判据的机判器（卡 `openmir2-gm-target-state-write`）。

只读：读 `<run>/<side>/frames.ndjson` 与两份 DB 快照 JSON，**不起栈、不连库**。

三条判据（逐侧）：
  G1  恰 1 条 `SM_SYSMESSAGE(100)` 且 `Param = MakeWord(f=255,b=56) = 0x38FF (14591)`、
      正文含「权限不够!!!」。**先按 `param==0x38FF` 过滤再计数**——登录/公告类也是
      `SM_SYSMESSAGE(100)`（绿字 `param=65499`，实网 2058 条），不得算进 G1；
  G2  **零** 帧：`SM_ADDITEM(200)` / `SM_DELITEM(202)` / `SM_WEIGHTCHANGED(622)` /
      `SM_LEVELUP(45)` / `SM_ABILITY(52)` / `SM_SUBABILITY(752)`；
  G3  跑前后 `characters_ablity.Level` 逐字节相同。冻结 schema 的 `characters_ablity`
      **没有 `ChrName`** ⇒ 按 `characters.Id = characters_ablity.PlayerId` join 取 Level
      （与执行单 `docs/mcp-admin-window-sheet-20261101.md` §4.3 的 SQL 同源）；
      0 行 / 格式不认 ⇒ UNDECIDABLE（不猜）。

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
# 红字 Param = MakeWord(fColor, bColor) = fColor | (bColor << 8)：**低位是前景色、高位是背景色**，
# 与 oracle `HUtil32.MakeWord(ushort low, ushort high)`（`src/OpenMir2/HUtil32.cs:66-69`）及
# Rust `make_word16`（`mir2-rs/bins/game-svr/src/smframe.rs:39-51`）一致。
# 部署 `Server.conf` 的 RedMsgFColor=255 / RedMsgBColor=56 ⇒ 255 | (56 << 8) = 0x38FF = 14591。
# Rust 单测在同一值上钉死：`mir2-rs/bins/game-svr/src/lib.rs:46536`
# （`assert_eq!(param, 0x38FF, "Red = MakeWord(255,56)")`）。
# 反序写法 `(255 << 8) | 56 = 0xFF38 = 65336` 是旧 bug，按现网真帧永远对不上。
RED_PARAM = 0x38FF
# 独立来源（不是本文件的合成夹具）：真机捕获里 ident=100 的红字帧 wire `param`。
#   /e/tmp/window/db-rerun/run/gm_batch/csharp/frames.ndjson:34
#   {"ident": 100, "param": 14591, "series": 1, "body_text": "本服务器还没有启用登录锁功能!!!"}
#   同目录 rust/frames.ndjson:34 为同一帧；两侧 param 均为 14591 = 0x38FF。
# 该捕获是 `@LockLogin` 的红字（正文不同），这里只取它的 **param 字段**作独立锚。
# reviewer-20 的现网普查（525 个 frames.ndjson）同向：ident==100 里 param==14591 共 49 条、
# param==65336（旧错值）共 **0** 条；绿字公告 param==65499 共 2058 条（不得计入 G1）。
REAL_CAPTURE_RED_PARAM = 14591


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
    # G1 只认红字 100：登录/公告类也是 ident=100（绿字 param=65499），先按 param 过滤再计数。
    red100 = [f for f in sys100 if f.get('param') == RED_PARAM]
    ok_cnt = len(red100) == 1
    # 正文：没有体文本就判不了（不猜）
    text = body_text_of(red100[0]) if ok_cnt else ''
    ok_txt = (PERM_TEXT in text) if text else None
    if not ok_cnt:
        seen = sorted({f.get('param') for f in sys100}, key=repr)
        res.append(('G1', 'FAIL', 'ident=100 且 param=%d 出现 %d 次（期望恰 1；ident=100 共 %d 条，params=%r）'
                    % (RED_PARAM, len(red100), len(sys100), seen[:8])))
    elif ok_txt is None:
        res.append(('G1', 'UNDECIDABLE', 'frames.ndjson 无 body_text，判不了正文'))
    elif not ok_txt:
        res.append(('G1', 'FAIL', '100 正文不含「%s」（实得 %r）' % (PERM_TEXT, text[:60])))
    else:
        res.append(('G1', 'PASS', '恰 1 条红字 100，Param=%d，正文含「%s」' % (RED_PARAM, PERM_TEXT)))

    bad = [(ident_of(f), i) for i, f in enumerate(frames) if ident_of(f) in FORBIDDEN]
    if bad:
        res.append(('G2', 'FAIL', '出现禁用帧 %r' % (bad[:5],)))
    else:
        res.append(('G2', 'PASS', '零 %r' % (FORBIDDEN,)))
    return res


def _rows_of(snap, names):
    """按候选表名取行列表；表不存在返回 None（区分「0 行」与「格式不认」）。"""
    for name in names:
        rows = snap.get(name)
        if isinstance(rows, list):
            return rows
    return None


def _level_of(snap, chr_name):
    """按冻结 schema 取 `characters_ablity.Level`（join `characters`）。

    冻结 schema 的 `characters_ablity` **没有 `ChrName`**（只有 `PlayerId / Level / ModifyTime`，
    见 `sql/mir2_db.sql:6073-6094` 与执行单 §4.3）⇒ 必须
    `characters c JOIN characters_ablity a ON a.PlayerId = c.Id`，与执行单 SQL 同源。
    返回 `(level, why)`；`level is None` 时 `why` 说明是 0 行、多义还是格式不认（不猜）。
    """
    if not isinstance(snap, dict):
        return None, '快照不是 JSON 对象'
    chars = _rows_of(snap, ('characters', 'Characters'))
    abl = _rows_of(snap, ('characters_ablity', 'CharactersAblity', 'ablity'))
    if chars is None or abl is None:
        return None, '快照缺 characters / characters_ablity 表（格式不认，不猜）'
    picked = []
    for c in chars:
        if not isinstance(c, dict):
            continue
        if chr_name is not None and c.get('ChrName') != chr_name:
            continue
        cid = c.get('Id')
        for a in abl:
            if isinstance(a, dict) and a.get('PlayerId') == cid and 'Level' in a:
                picked.append(a['Level'])
    if not picked:
        if chr_name is not None:
            return None, 'join 后 0 行（characters 无 ChrName=%r 或 abilities 无对应 PlayerId）' % (chr_name,)
        return None, 'join 后 0 行'
    if len(set(picked)) != 1:
        return None, 'join 命中 %d 行且 Level 不唯一 %r（不猜）' % (len(picked), sorted(set(picked), key=repr))
    return picked[0], 'join 命中 1 行'


def check_g3(before_path, after_path, chr_name=None):
    if not (before_path and after_path):
        return ('G3', 'UNDECIDABLE', '未提供 --before/--after 快照')
    try:
        b = json.load(io.open(before_path, encoding='utf-8'))
        a = json.load(io.open(after_path, encoding='utf-8'))
    except (OSError, ValueError) as ex:
        return ('G3', 'UNDECIDABLE', '快照读不了：%s' % ex)
    lb, whyb = _level_of(b, chr_name)
    la, whya = _level_of(a, chr_name)
    if lb is None or la is None:
        return ('G3', 'UNDECIDABLE', 'Level 取不到（before: %s / after: %s；不猜）' % (whyb, whya))
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
        # G1P：登录/公告也是 ident=100（绿字 param=65499，实网 2058 条）——不得污染 G1 计数。
        ('mixed-announce', [f(100, 65499, '欢迎进入...'), f(100, RED_PARAM, PERM_TEXT), f(10)], 'G1', 'PASS'),
        ('announce-only', [f(100, 65499, '欢迎进入...')], 'G1', 'FAIL'),
        ('mixed-two-red', [f(100, 65499, '欢迎...'), f(100, RED_PARAM, PERM_TEXT),
                           f(100, RED_PARAM, PERM_TEXT)], 'G1', 'FAIL'),
        # A2 独立来源参数：14591 取自真机捕获（见 REAL_CAPTURE_RED_PARAM 注释），不是 RED_PARAM 的字面量拷贝。
        ('real-capture-param', [f(100, REAL_CAPTURE_RED_PARAM, PERM_TEXT), f(10)], 'G1', 'PASS'),
        # 回归牙：f/b 反序的 0xFF38(65336) 必须判 FAIL；若有人把 RED_PARAM 改回去，本用例变 PASS ⇒ selftest 红。
        ('reversed-param', [f(100, (255 << 8) | 56, PERM_TEXT)], 'G1', 'FAIL'),
        ('bad-text', [f(100, RED_PARAM, '未知命令: Level0 5')], 'G1', 'FAIL'),
        ('no-text', [f(100, RED_PARAM, '')], 'G1', 'UNDECIDABLE'),
    ]

    ok = True
    # 绝对锚（A1）：字面量必须**写在测试里**，不得从 RED_PARAM / REAL_CAPTURE_RED_PARAM 派生。
    # 否则两个常量被同时改（reviewer-21 变异 M5：两者都改成 9999）时，同源比较仍然自洽、牙齿空转。
    # 0x38FF = 14591 的三重 oracle 锚（与字面量并列，不是又一个同源常量）：
    #   * C# `HUtil32.MakeWord(low, high) = low | (high << 8)`（`src/OpenMir2/HUtil32.cs:66-69`）；
    #     部署 RedMsgFColor=255 / RedMsgBColor=56 ⇒ 255 | (56 << 8) = 0x38FF。
    #   * Rust `make_word16(f, b) = f | (b << 8)` 同序（`mir2-rs/bins/game-svr/src/smframe.rs:39-51`）。
    #   * Rust 单测钉死该值（`mir2-rs/bins/game-svr/src/lib.rs:46536`）。
    # 真机捕获同向（见上方 REAL_CAPTURE_RED_PARAM 注释：49 条 14591、0 条 65336）。
    for const_name, const_val in (('RED_PARAM', RED_PARAM),
                                  ('REAL_CAPTURE_RED_PARAM', REAL_CAPTURE_RED_PARAM)):
        if const_val != 0x38FF:
            print('  selftest red-param-abs MISMATCH  (%s=%d ≠ 字面量 0x38FF/14591)' % (const_name, const_val))
            ok = False
        else:
            print('  selftest red-param-abs OK  (%s=%d == 0x38FF)' % (const_name, const_val))
    # 牙齿：实现常量必须仍与真机捕获的独立值一致（单边改 RED_PARAM 会在这里先红）。
    if RED_PARAM != REAL_CAPTURE_RED_PARAM:
        print('  selftest red-param-source MISMATCH  (RED_PARAM=%d ≠ 真机捕获 %d)' % (RED_PARAM, REAL_CAPTURE_RED_PARAM))
        ok = False
    else:
        print('  selftest red-param-source OK  (RED_PARAM=%d == 真机捕获)' % RED_PARAM)
    for name, frames, item, want in cases:
        res = check_side(frames)
        got = [r for r in res if r[0] == item][0]
        good = got[1] == want
        print('  selftest %-12s %s  (%s %s %s)' % (name, 'OK' if good else 'MISMATCH', item, got[1], got[2][:40]))
        ok = ok and good

    # G3：按冻结 schema join characters(a.PlayerId = c.Id)；正/反/认不出/0 行
    import tempfile
    td = tempfile.mkdtemp(prefix='gm_gate_selftest_')
    pa = os.path.join(td, 'a.json')
    pb = os.path.join(td, 'b.json')
    pc = os.path.join(td, 'c.json')
    pd = os.path.join(td, 'd.json')
    pe = os.path.join(td, 'e.json')
    pf = os.path.join(td, 'f.json')
    # 冻结 schema：characters_ablity 无 ChrName，只有 PlayerId / Level。
    open(pa, 'w', encoding='utf-8').write(json.dumps({
        'characters': [{'Id': 1, 'ChrName': 'x'}],
        'characters_ablity': [{'PlayerId': 1, 'Level': 5}]}))
    open(pb, 'w', encoding='utf-8').write(json.dumps({
        'characters': [{'Id': 1, 'ChrName': 'x'}],
        'characters_ablity': [{'PlayerId': 1, 'Level': 6}]}))
    open(pc, 'w', encoding='utf-8').write(json.dumps({'unknown': 1}))
    # 角色存在但 abilities 无对应行 ⇒ join 0 行
    open(pd, 'w', encoding='utf-8').write(json.dumps({
        'characters': [{'Id': 1, 'ChrName': 'x'}],
        'characters_ablity': []}))
    # 另一个角色 ⇒ --chr-name 指定不存在的名字时 join 0 行（不报错）
    open(pe, 'w', encoding='utf-8').write(json.dumps({
        'characters': [{'Id': 2, 'ChrName': 'y'}],
        'characters_ablity': [{'PlayerId': 2, 'Level': 5}]}))
    # 两个角色各一行 ability：join 必须按 PlayerId 精确配对（不能只取第一行）
    open(pf, 'w', encoding='utf-8').write(json.dumps({
        'characters': [{'Id': 1, 'ChrName': 'x'}, {'Id': 2, 'ChrName': 'y'}],
        'characters_ablity': [{'PlayerId': 1, 'Level': 5}, {'PlayerId': 2, 'Level': 6}]}))
    for name, a, b, chr_name, want in (('same', pa, pa, None, 'PASS'),
                                       ('diff', pa, pb, None, 'FAIL'),
                                       ('diff-by-name', pa, pb, 'x', 'FAIL'),
                                       ('join-two-chars', pf, pf, 'x', 'PASS'),
                                       ('unknown', pa, pc, None, 'UNDECIDABLE'),
                                       ('missing-ability', pa, pd, None, 'UNDECIDABLE'),
                                       ('unknown-chr', pa, pe, 'nobody', 'UNDECIDABLE')):
        item, verdict, detail = check_g3(a, b, chr_name)
        good = verdict == want
        print('  selftest g3-%-14s %s  (%s)' % (name, 'OK' if good else 'MISMATCH', verdict))
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
