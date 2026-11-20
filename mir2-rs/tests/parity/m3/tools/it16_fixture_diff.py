#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""IT-16 夹具比较器：归一化后逐路径 diff（A1/A2/B1/B2 非干扰证据的可复跑脚本）。

用途（卡 §9.11「② RNG 钩子是新测量口径」）：
- `A1/A2`：**不设** `MIR2_RNG_SEED`（钩子关）跑两次 eat-probe；
- `B1/B2`：`DOTNET_STARTUP_HOOKS=<RngSeedHook.dll>` + 两个不同 `MIR2_RNG_SEED`；
- 本脚本把两份夹具归一化后逐路径比较，输出差异路径。

归一化清单（两项，均为**墙钟派生、非行为**字段）：
1. `cases[*].expect.added_items[*].make_index` → 0；
2. `cases[*].expect.item_list[*].make_index` → 0，**仅限该例新建件**（按同一例 `added_items`
   的 make_index 值集合匹配；既有件的 make_index（如 1）保持可比）。

`--drop-lottery`：忽略顶层 `lottery` 块（A 运行 `enabled=false`、B 运行有 12 例；它只存在于
播种运行，故 A≈B 的逐字节面必须把它排除；B1 与 B2 的 lottery 块另用脚本/断言比较）。

退出码：0 = 归一化后完全相同；1 = 有差异（逐条打印路径）；2 = 用法/读文件错误。

调用示例（仓库根执行）：

    python -I -B mir2-rs/tests/parity/m3/tools/it16_fixture_diff.py \
        E:/UserTemp/it16-A1.json E:/UserTemp/it16-B1.json --drop-lottery
"""

import json
import sys


def normalize(doc, drop_lottery=False):
    doc = json.loads(json.dumps(doc))
    for case in doc.get('cases', []):
        ex = case.get('expect', {})
        created = {it.get('make_index') for it in (ex.get('added_items') or [])}
        for it in ex.get('added_items') or []:
            it['make_index'] = 0
        for it in ex.get('item_list') or []:
            if it.get('make_index') in created:
                it['make_index'] = 0
    if drop_lottery:
        doc.pop('lottery', None)
    return doc


def walk(a, b, path=''):
    if type(a) is not type(b):
        return [(path, a, b)]
    diffs = []
    if isinstance(a, dict):
        for k in sorted(set(a) | set(b)):
            if k not in a or k not in b:
                diffs.append((path + '/' + k, a.get(k, '<missing>'), b.get(k, '<missing>')))
            else:
                diffs += walk(a[k], b[k], path + '/' + k)
    elif isinstance(a, list):
        if len(a) != len(b):
            diffs.append((path + '/len', len(a), len(b)))
        for i, (x, y) in enumerate(zip(a, b)):
            diffs += walk(x, y, '%s[%d]' % (path, i))
    elif a != b:
        diffs.append((path, a, b))
    return diffs


def main(argv):
    args = [a for a in argv[1:] if not a.startswith('--')]
    if len(args) != 2:
        print(__doc__)
        return 2
    drop = '--drop-lottery' in argv
    try:
        a = normalize(json.load(open(args[0], encoding='utf-8')), drop)
        b = normalize(json.load(open(args[1], encoding='utf-8')), drop)
    except (OSError, ValueError) as e:
        print('读/解析夹具失败：%s' % e, file=sys.stderr)
        return 2
    diffs = walk(a, b)
    print('diffs=%d' % len(diffs))
    for item in diffs[:200]:
        print('  %r' % (item,))
    return 0 if not diffs else 1


if __name__ == '__main__':
    sys.exit(main(sys.argv))
