#!/usr/bin/env python3
"""MonGen.txt → 单图刷怪点只读抽取（x<TAB>y<TAB>range）。

用法：
    python extract_mongen.py <MonGen.txt> <map>
输出：每行 `x<TAB>y<TAB>range`（range = 第 6 列「刷怪范围」）；该图无匹配 ⇒ **零行**。
MonGen.txt 是 GB18030/GBK 字节流：本工具只按 TAB 切分，地图名/坐标/范围为 ASCII 列，不依赖中文列。
**内容冻结**：本工具只读、不写任何数据文件；调用方把 stdout 重定向到
`fixtures/mongen-map<map>.tsv`（再补文件头 provenance 注释），并把行数/源 SHA256 记进
`fixtures/mongen-provenance.tsv`。

用途：R-1「零 RNG 消费者」场景隔离——攻击对拍必须落在**整张零刷怪**的图上，
`gate_guards.ps1::Test-M3MonGenEmpty` 读这里的提取件判「该图刷怪行数 == 0」。不参与游戏逻辑。
"""
import sys


def extract(source_path: str, map_name: str):
    with open(source_path, 'rb') as fh:
        data = fh.read()
    rows = []
    for raw in data.replace(b'\r\n', b'\n').split(b'\n'):
        if not raw.strip() or raw.lstrip().startswith(b';'):
            continue
        parts = raw.split(b'\t')
        if len(parts) < 6:
            continue
        if parts[0].decode('ascii', 'replace') != map_name:
            continue
        rows.append(b'\t'.join((parts[1], parts[2], parts[5])))
    return rows


def main() -> int:
    if len(sys.argv) != 3:
        sys.stderr.write(__doc__)
        return 2
    rows = extract(sys.argv[1], sys.argv[2])
    # 写 UTF-8/LF 字节：Windows 文本模式会把 \n 变 \r\n ⇒ 用 buffer 保证跨平台逐字节可复现。
    if rows:
        sys.stdout.buffer.write(b'\n'.join(rows) + b'\n')
    return 0


if __name__ == '__main__':
    sys.exit(main())
