# tests/parity —— D 线（数据与公式）对拍门禁

判据来源：`docs/Rust重写设计文档.md` §6.0（完成定义）+ D 线任务书。
原则：判据可测、能证伪、差异要么 0 要么进 `whitelist.md`、证据四件套。

## 结构

```
tests/parity/
  csharp/ParityGolden/   # C# 金标准生成器（链接真实 RandomNumber.cs / ConfigFile.cs；
                         # MonGetRandomItems / RecalcLevelAbilitys / 极品加成链为原码逐字拷贝）
  csharp/DbNullProbe/    # 一次性探针：实测 MySqlConnector/DataReaderExtension 的 NULL 语义
  fixtures/              # 夹具（现网内容的只读副本；Exps.conf、MonItems/*.txt（GBK）、物品子集 JSON）
  golden/                # 金标准输出（由 ParityGolden 生成，入库冻结）
  tests/                 # Rust 对拍测试（cargo test）
```

## 跑门禁（完整）

```bash
cd mir2-rs
cargo test --workspace                      # 单测 + 对拍门禁②③④ + levelabil 加固门禁
cargo test -p mir2-parity-tests --test table_counts -- --ignored --nocapture   # 门禁①（需本机 MySQL）
```

## 重新生成金标准（仅在 C# 参照源或夹具变更后）

```bash
cd mir2-rs/tests/parity/csharp/ParityGolden
FIX='E:/Users/gxh/Documents/GitHub/OpenMir2/mir2-rs/tests/parity/fixtures'
GOLD='E:/Users/gxh/Documents/GitHub/OpenMir2/mir2-rs/tests/parity/golden'
dotnet run -- rng 42 "$GOLD/rng_seed42.txt"
dotnet run --no-build -- rng 20240229 "$GOLD/rng_seed20240229.txt"
dotnet run --no-build -- rnglarge "$GOLD/rng_large_range.txt"
dotnet run --no-build -- exp "$FIX/Exps.conf" "$GOLD/exp_table.txt"
dotnet run --no-build -- levelabil "$GOLD/levelabil.txt"
dotnet run --no-build -- drop "$FIX/MonItems/jiangshi1.txt" "$FIX/items_jiangshi1.json" 42 100 40 "$GOLD/drop_jiangshi1_100.txt"
for f in m5 m6 m10 m11 m15 m19 m20 m21 m24 m26 r22 r23 u15 u22 u23 u24 u26; do
  dotnet run --no-build -- drop "$FIX/MonItems/synth/$f.txt" "$FIX/items_synth.json" 42 100 1 "$GOLD/drop_synth_$f.txt"
done
```

物品夹具（`fixtures/items_jiangshi1.json`）由只读导出工具从真实库生成：

```bash
cargo run -p mir2-parity-tests --bin dump_item_fixture -- \
  "mysql://root@127.0.0.1:3306/mir2_data" \
  "tests/parity/fixtures/MonItems/jiangshi1.txt" \
  "tests/parity/fixtures/items_jiangshi1.json"
```

**可复现性记录（2026-10-11）**：`dump_item_fixture` 的源文件曾被 `mir2-rs/.gitignore` 的
`**/bin/` 规则静默吞掉（目录约定 `src/bin/` 与 C# 构建产出 `bin/` 相撞），⇒ 上面这条命令在
干净检出上**跑不了**；本批已把工具入库（ignore 规则由 `worktree-m2b-p99` 同一根因修复）。
入库后按上面命令重导出（本机 MySQL 3306 + 真实 `mir2_data`）：
`stditems 全表 1000 行，掉落表 44 行，导出夹具 43 行`，产物与库内夹具
**sha256 逐字节一致**：`items_jiangshi1.json` = `76CEFB9535F75AEC7DBE7AF117657CA39C5A5610E4344B3FFD94EEAEE1920FA9`
（3942 字节 / 260 行）。

⚠️ 已知缺口：`fixtures/items_synth.json`（17 行）**当前无法从这份库重生成**，但**输入清单并没有丢**——
口径更正（2026-10-11，独立审查 reviewer-3 的探针证据，取代先前"输入未记录/手工清单"的错判）：

- 17 条 fixture 与上面 bash 块里的 17 个 `MonItems/synth/*.txt` **一一对应**（名字就是各文件引用的名字，
  且 fixture 的 `std_mode` 恰好等于文件名里的数字：`m5`→木剑(5)、`m6`→匕首(6)、`m10`→布衣男S(10)、
  `u26`→神秘手镯P(26)…）；
- 不可复现出在**库侧**：枚举当前 `mir2_data.stditems` 后按同名（大小写不敏感）比对，17 个名字里
  **10 个不存在**（布衣男S/布衣女S/青铜头盔S/琥珀项链S/魔鬼项链S/钢手镯S/蓝色水晶戒指S/神秘戒指P/神秘手镯/神秘手镯P），
  存在的 7 个里还有 4 个属性不符（匕首 DB `std=5/shape=6` vs fixture `6/0`；记忆项链 `19/123` vs `21/0`；
  技巧项链 `20/120` vs `24/0`；神秘头盔 `shape 132` vs `130`；神秘戒指 `shape 130` vs `131`）
  ⇒ **这份 fixture 是从另一个库快照导出的**，不是当前这份 `mir2_data`。
- 影响：`tests/drop_parity.rs:137` 消费它、用例在 master 全绿 ⇒ **不阻塞**；要重生成得先拿到**与之匹配的库快照**
  （或让 fixture 的数值改为以当前库为准，并同步重导 C# 侧 `drop_synth_*.txt` 金标准）。

## 四条验收门禁 ↔ 测试对照

| 验收 | 测试 | 金标准 |
| --- | --- | --- |
| ① 表加载条数 = C# 启动日志 | `tests/table_counts.rs`（`--ignored`，连 MySQL） | C# 日志摘录（测试注释内） |
| ② 固定种子 RNG 序列逐项相同 | `tests/rng_parity.rs`（种子 42 / 20240229；另含大区间分支 `rng_large_range.txt`，三种子 × 两次连续大区间调用 + 采样消耗数） | 真实 `RandomNumber.cs` 输出 |
| ③ 经验曲线逐级 diff = 0（1→255，含英雄同表） | `tests/exp_parity.rs` | 真实 `ConfigFile.cs` 读 `Exps.conf` |
| ④ 固定种子 100 次击杀掉落列表相同 | `tests/drop_parity.rs`（真实怪 1 例 + 合成 17 分支） | 原码逐字拷贝 + 真实 RNG |

加固（验收外）：`tests/levelabil_parity.rs` —— 三职业 × 256 级 `RecalcLevelAbilitys`
（负重/属性加成公式）全表对拍。

## 红检方式（每条门禁怎么证明它会红）

自动红检（每次 `cargo test` 都跑）：
- `rng_parity::redcheck_wrong_seed_must_differ` / `redcheck_shuffled_calls_must_differ`
- `exp_parity::redcheck_tampered_table_must_differ`
- `drop_parity::redcheck_wrong_seed_must_differ` / `redcheck_tampered_drop_table_must_differ`
- `levelabil_parity::redcheck_changed_config_must_differ`

人工红检（已实测，输出见 `D线报告.md` 附录）：
1. `fixtures/Exps.conf` 改 `Level500=999999` → `exp_parity` 红（`NeedExps 全表 diff = 1`）→ 恢复后绿；
2. `golden/rng_seed42.txt` 改第 10 行 → `rng_parity` 红（`第 10 行: rust=815 csharp=0`）→ 恢复后绿；
3. `fixtures/MonItems/jiangshi1.txt` 金币率 `10/100→5/100` → `drop_parity` 红（第 2 行不一致）→ 恢复后绿。

已知灵敏度边界（如实登记）：概率微调（如 `10/100→10/101`）只改变判定窗口、
不改变 RNG 调用序列，N=100 采样可能踩不中翻转区间而**漏检**（实测未变红）。
序列级错误（种子、调用顺序、调用次数）100% 可检出。

## 注意

- **夹具即内容冻结**：`fixtures/` 全部是现网只读副本，重新复制会覆盖——先确认现网没漂移。
- `golden/` 入库冻结；改 C# 参照或夹具后必须重新生成并在 PR 里说明。
- `MakeIndex` 不进掉落对拍：C# `M2Share.GetItemNumber()` = 计数器 + `GetTickCount()`（墙钟），
  两次 C# 运行本身就不一致（见 `whitelist.md` D-1）。
