#!/usr/bin/env python3
"""Envir 的「非 MonGen AI 消费者」只读抽取（`file<TAB>ref_rows`）。

用法：
    python extract_map_ai_consumers.py <EnvirDir> <map>
输出：每个受检文件一行 `file<TAB>ref_rows`；`ref_rows` = 该文件里把 `<map>` 作为**整词/整字段**
出现的**数据行**数（跳过空行与 `;` 注释行）。全部为 0 ⇒ 该图没有守卫/商人/NPC/地图事件/任务触发器
这些**除 MonGen 外**的跑 AI 实体（R-1「零 RNG 消费者」场景隔离的补充判据）。

为什么需要它（owner brief 2026-10-12 15:05）：
    严格说不能只看 MonGen —— `GuardList.txt`（守卫）、`MerChant.txt`（商人/NPC）、`Npcs.txt`（NPC 脚本）、
    `MapEvent.txt`（地图事件）、`MapQuest.txt`（任务触发）都可能生成跑 AI 的实体并消耗全局 RNG。
    所以攻击对拍 pin 的图除了「MonGen 零行」外，还要求这些文件对**该图**零引用。

匹配口径：把行按「空白 / 方括号 / 逗号」切词后做**整词相等**（`[58]` 写成 `58`、`58,` 也切成 `58`），
而不是子串包含：避免 `0137` 误命中别的数字串。列位置在这些文件里不统一（守卫/商人/NPC 的图号列位不同），
整词匹配对「零引用」是保守且充分的。

**内容冻结**：本工具只读、不写任何数据文件；调用方把 stdout 重定向到
`fixtures/ai-consumer-map<map>.tsv`（再补文件头 provenance 注释），并把源文件 SHA256
记进 `fixtures/ai-consumer-provenance.tsv`。`gate_guards.ps1::Test-M3MapNoAiConsumers`
读这里的提取件判「该图非 MonGen AI 消费者引用数 == 0」。不参与游戏逻辑。
"""
import os
import re
import sys

# 受检文件全集（顺序即 fixture 行序，与 `gate_guards.ps1::$script:M3AiConsumerFiles` 必须一致）。
FILES = ['GuardList.txt', 'MerChant.txt', 'Npcs.txt', 'MapEvent.txt', 'MapQuest.txt']
SPLIT = re.compile(r'[\s\[\],]+')


def ref_rows(path: str, map_name: str) -> int:
    with open(path, 'rb') as fh:
        data = fh.read()
    n = 0
    for raw in data.replace(b'\r\n', b'\n').split(b'\n'):
        if not raw.strip() or raw.lstrip().startswith(b';'):
            continue
        tokens = [t for t in SPLIT.split(raw.decode('gb18030', 'replace')) if t]
        if map_name in tokens:
            n += 1
    return n


def main() -> int:
    if len(sys.argv) != 3:
        sys.stderr.write(__doc__)
        return 2
    envir_dir, map_name = sys.argv[1], sys.argv[2]
    out = []
    for name in FILES:
        out.append('{0}\t{1}'.format(name, ref_rows(os.path.join(envir_dir, name), map_name)))
    # 写 UTF-8/LF 字节：Windows 文本模式会把 \n 变 \r\n ⇒ 用 buffer 保证跨平台逐字节可复现。
    sys.stdout.buffer.write(('\n'.join(out) + '\n').encode('utf-8'))
    return 0


if __name__ == '__main__':
    sys.exit(main())
