#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""MonGen 取数次序的**独立复刻**（不引用 Rust 代码）——`crates/world/src/mongen.rs` 的交叉验证。

口径来源（逐行实读 master `1b576172` 的 `src/`）：
- `GameSvr/Word/WorldServer.MonGen.cs:63-99`（`InitializeMonster` 两次分片抽数）
- `GameSvr/Word/WorldServer.MonGen.cs:811-874`（`RegenMonsters` 位置抽数）
- `GameSvr/Word/WorldServer.MonGen.cs:740/744/747`（`CreateMonster` 的 Dir / 掉落 / CoolEye 抽数）

RNG 实现：`src/OpenMir2/RandomNumber.cs` 包装的 .NET Framework `System.Random`
（subtractive 生成器），与 `mir2-rs/tests/parity/m3/rng-isolate-attack/compare_dedicated_streams.py`
里的 `DotNetRandom` 同一口径（该文件已与 .NET 实机对拍过）。

用法：`python -I 02-crosscheck-python-mongen-rng.py`（打印 JSON 供 Rust 侧断言）。
"""

from __future__ import annotations

import json

MBIG = 2147483647
MSEED = 161803398


class DotNetRandom:
    def __init__(self, seed: int) -> None:
        subtraction = MBIG if seed == -2147483648 else abs(seed)
        mj = MSEED - subtraction
        a = [0] * 56
        a[55] = mj
        mk = 1
        for i in range(1, 55):
            ii = (21 * i) % 55
            a[ii] = mk
            mk = mj - mk
            if mk < 0:
                mk += MBIG
            mj = a[ii]
        for _ in range(4):
            for i in range(1, 56):
                a[i] -= a[1 + (i + 30) % 55]
                if a[i] < 0:
                    a[i] += MBIG
        self.a = a
        self.ix = 0
        self.ixp = 21

    def _sample_int(self) -> int:
        li = self.ix + 1
        lp = self.ixp + 1
        if li >= 56:
            li = 1
        if lp >= 56:
            lp = 1
        r = self.a[li] - self.a[lp]
        if r == MBIG:
            r -= 1
        if r < 0:
            r += MBIG
        self.a[li] = r
        self.ix = li
        self.ixp = lp
        return r

    def _sample(self) -> float:
        return self._sample_int() * (1.0 / MBIG)

    def next_max(self, max_value: int) -> int:
        # .NET `Random.Next(int maxValue)` = `(int)(Sample() * maxValue)`（消耗 1 次采样）。
        return int(self._sample() * max_value)

    def random_byte(self, value: int) -> int:
        # `RandomNumber.RandomByte(byte value)` = `(byte)random.Next(value)`（value < 255）。
        return self.next_max(value) & 0xFF


def initialize_monster_draws(entries, monster_list_names, limit, seed):
    """复刻 `WorldServer.MonGen.cs:63-99` 的抽取序列（只回放 RNG，不建模结构）。"""
    rng = DotNetRandom(seed)
    # `monsterGenMap`：OrdinalIgnoreCase 分组，保留首次出现拼写/插入序。
    groups = []
    for name in entries:
        key = name.lower()
        if not any(g[0] == key for g in groups):
            groups.append((key, []))
    draws = []
    for _ in groups:
        draws.append(rng.next_max(limit))
    for _ in monster_list_names:
        draws.append(rng.next_max(limit))
    return draws


def regen_positions(seed, x, y, rng_range, count, mission_gen_rate, create_draws):
    """复刻 `WorldServer.MonGen.cs:811-874` 的位置抽数。

    `create_draws` = 每只 `CreateMonster` 内部的 RNG 消耗（Dir + 掉落 + CoolEye），
    用于验证「位置抽 ↔ CreateMonster 抽」的 interleaving。
    """
    rng = DotNetRandom(seed)
    span = rng_range * 2 + 1
    base_x = x - rng_range
    base_y = y - rng_range
    positions = []
    if mission_gen_rate > 0 and rng.next_max(100) < mission_gen_rate:
        nx = base_x + rng.next_max(span)
        ny = base_y + rng.next_max(span)
        for _ in range(count):
            px = nx - 10 + rng.next_max(20)
            py = ny - 10 + rng.next_max(20)
            for _ in range(create_draws):
                rng.random_byte(8)
            positions.append((px, py))
    else:
        for _ in range(count):
            px = base_x + rng.next_max(span)
            py = base_y + rng.next_max(span)
            for _ in range(create_draws):
                rng.random_byte(8)
            positions.append((px, py))
    return positions


def main():
    out = {}

    # ① initialize_monster：limit=2、seed=42；3 个不同怪名（大小写不敏感分组）+ 2 个 MonsterList 项
    entries = ["A", "B", "a", "C"]
    out["initialize"] = {
        "limit": 2,
        "seed": 42,
        "distinct_names": ["A", "B", "C"],
        "monster_list": ["A", "C"],
        "draws": initialize_monster_draws(entries, ["A", "C"], 2, 42),
    }

    # ② RegenMonsters 普通支：range=2, X=166, Y=266, count=4, MissionGenRate=0, seed=42
    #    create_draws = [RandomByte(8), RandomByte(8)]（模拟 Dir + 一次掉落抽）
    out["regen_normal"] = {
        "seed": 42,
        "x": 166,
        "y": 266,
        "range": 2,
        "count": 4,
        "mission_gen_rate": 0,
        "create_draws": 2,
        "positions": regen_positions(42, 166, 266, 2, 4, 0, 2),
    }

    # ③ RegenMonsters 集中支：MissionGenRate=100（恒命中），count=3, seed=42
    out["regen_mission"] = {
        "seed": 42,
        "x": 166,
        "y": 266,
        "range": 3,
        "count": 3,
        "mission_gen_rate": 100,
        "create_draws": 2,
        "positions": regen_positions(42, 166, 266, 3, 3, 100, 2),
    }

    # ④ 普通支消费完后的下一次 `Random(1000000)`（Rust 测试 `regen_positions_match_independent_python_normal` 的尾部断言）
    rng = DotNetRandom(42)
    span = 2 * 2 + 1
    for _ in range(4):
        rng.next_max(span)
        rng.next_max(span)
        rng.random_byte(8)
        rng.random_byte(8)
    out["regen_normal_next_draw"] = rng.next_max(1000000)

    print(json.dumps(out, ensure_ascii=False, indent=2, sort_keys=True))


if __name__ == "__main__":
    main()
