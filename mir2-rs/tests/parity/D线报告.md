# D 线验收报告：数据与公式（2026-10-10）

判据来源：`docs/Rust重写设计文档.md` §6.0 / §12.2-D + D 线任务书。
产出：`mir2-rs/crates/data`、`mir2-rs/crates/shared`（含 `rng.rs`）、
`mir2-rs/crates/formula`、`mir2-rs/tests/parity`。

## 结论

| 验收 | 结果 | 证据 |
| --- | --- | --- |
| ① 各表加载条数与 C# 启动日志一致 | **通过**（stditems 1000/1000、monsters 705/705、magics 108/108、goldsales 0/0） | §证据-① |
| ② 固定种子 RNG 输出序列与 C# 逐项相同 | **通过**（种子 42、20240229 各 2660 行，diff=0） | §证据-② |
| ③ 经验曲线逐级一致（1→255 全表 diff=0，含英雄同表） | **通过**（NeedExps 1000 项 + 11 标量 diff=0） | §证据-③ |
| ④ 固定种子 100 次击杀掉落列表与 C# 相同 | **通过**（真实怪「僵尸1」×100 杀 + 合成 17 分支 ×100 杀，逐项 diff=0） | §证据-④ |

加固（验收外）：`RecalcLevelAbilitys`（负重/属性加成）三职业 × 256 级共 768 行对拍 diff=0。
红检：自动红检 7 条每次随门禁运行；人工红检 3 条实测变红（输出见附录 A）。
白名单：对拍差异 0 项；豁免/限制 6 项登记于 `tests/parity/whitelist.md`（D-1..D-6）。

## 证据-① 表加载条数

命令：

```bash
cargo test -p mir2-parity-tests --test table_counts -- --ignored --nocapture
```

原始输出（Rust 侧，2026-10-10，连接 `mysql://root@127.0.0.1:3306/mir2_data`，只读 SELECT）：

```text
表加载条数对拍（rust vs C# 启动日志）:
  stditems: rust=1000  csharp=1000
  monsters: rust=705  csharp=705
  magics: rust=108  csharp=108
  goldsales: rust=0  csharp=0
test table_counts_match_csharp_startup_log ... ok
```

C# 侧数字（`E:\MirServer\logs\GameSvr.out.log`，2026-10-10 00:10:50 启动）：

```text
00:10:50.806 [INF] 物品数据库加载成功...[1000]
00:10:51.786 [INF] 加载怪物数据库成功...[705]
00:10:51.789 [INF] 加载技能数据库成功...[108]
00:10:51.808 [INF] 读取物品寄售列表成功...[0]
```

**关键发现（加载语义）**：C# 读列走 `src/OpenMir2/Extensions/DataReaderExtension.cs`，
不是原生 `IDataReader.GetXxx`。现网 `stditems` 全表 1000 行的 `Effrate2/Effvalue2/SlowDown/
Tox/ToxAvoid/UniqueItem/OverlapItem/LIGHT/ItemType/ItemSet/Reference` 列**均为 NULL**
（与 `sql/mir2_data.sql` dump 一致），C# 靠扩展方法的默认值语义容忍
（`GetByte(NULL)→0`、`GetInt32(NULL)→-1`、`GetString(NULL)→""`、
`GetSByte(NULL)→抛 InvalidCastException`）。探针实证：`csharp/DbNullProbe`（引用现网
`MySqlConnector.dll`）输出 `GetByte(NULL) 抛 InvalidCastException`——即若按原生读取，
C# 与 Rust 都会加载失败；按扩展方法语义两侧均成功加载 1000 行。
Rust 转换层已逐条对齐（`crates/data/src/loaders.rs` 模块注释有对照表）。

## 证据-② RNG 序列

金标准：`csharp/ParityGolden` 链接**真实** `src/OpenMir2/RandomNumber.cs`
（私有 `Random` 字段经反射注入固定种子），调用脚本与 Rust 侧逐条一致
（`Random()`/`Random(100)`/`Random(10,1000)`/`GetRandomNumber(1,6)`/`RandomByte(200)`/
每 10 轮 `GenerateRandomNumber(8)`/每 25 轮 `RandomSelect(10 选 3)`，共 500 轮）。

命令：

```bash
dotnet run -- rng 42 "$GOLD/rng_seed42.txt"          # C# 金标准
cargo test --test rng_parity                          # Rust 对拍
```

对拍数字（首 3 行示例，两侧一致；全量 2660 行/种子，diff=0）：

| 行 | 调用 | 值（两侧相同） |
| --- | --- | --- |
| 1 | `Random()` | 1434747710 |
| 2 | `Random(100)` | 14 |
| 3 | `Random(10,1000)` | 134 |

红检：换种子（43）→ 序列不同（自动）；金标准第 10 行改 0 → 测试红
（`第 10 行: rust=815 csharp=0`，附录 A-2）。

注：C# 生产路径无参 `new Random()` 在 .NET 6+ 为 xoshiro256**，与固定种子
legacy 算法不同序列；无参无法对拍，登记 whitelist D-3。

## 证据-③ 经验曲线

金标准：`ParityGolden exp` 用**真实** `ConfigFile.cs` 读夹具
`fixtures/Exps.conf`（现网 `E:\MirServer\M2GameSvr\Exps.conf` 只读副本，
1060 个 `LevelN` 键中 C# 只消费 `Level0..Level999`），复刻 `ExpsConf.LoadConfig`。

命令：

```bash
dotnet run -- exp "$FIX/Exps.conf" "$GOLD/exp_table.txt"
cargo test --test exp_parity
```

结果：`need_exps_table_matches_golden`（1000 项 diff=0）、
`get_level_exp_1_to_max_matches_golden`（1→255 逐级 diff=0，英雄与人物同表，
本代码库无独立英雄经验表）、`exp_scalars_match_golden`（11 个标量一致，
缺省键默认值已对齐 `GameSvrConf.cs:1828-1839`）。

红检：`fixtures/Exps.conf` 改 `Level500=999999` → 红（`NeedExps 全表 diff = 1`，
附录 A-1）。

## 证据-④ 掉落公式

金标准：`ParityGolden drop`（`MonGetRandomItems` / `LoadMonitems` / 极品加成链
均为原 C# **逐字拷贝** + 真实 `RandomNumber.cs`）。

- 主用例：真实怪物「僵尸1」（`fixtures/MonItems/jiangshi1.txt`，44 行掉落表，
  GBK）+ 真实物品子集（`fixtures/items_jiangshi1.json`，43 行，只读导出工具从
  现网 `stditems` 生成），`MonRandomAddValue=40`（现网 Server.conf:612），
  种子 42，100 次击杀 → 掉落列表逐项 diff=0。
- 分支覆盖：17 个单物品合成掉落表（`fixtures/MonItems/synth/`），
  `MonRandomAddValue=1` 逼每条 `RandomUpgradeItem`（StdMode 5/6/10/11/15/19/20/
  21/22/23/24/26）与 `RandomSetUnknownItem`（Shape 130/131/132）分支生效，
  各 100 杀 → diff=0。

主用例首 4 行（两侧一致；含 itemName bug 复刻证据——三个不同物品都复制成 Index 35）：

```text
KILL 1 G 0 N 0
KILL 2 G 384 N 3
I 35 10 12 0 0 0 0 0 0 0 0 0 0 0 0 0 0
I 35 10 12 0 0 0 0 0 0 0 0 0 0 0 0 0 0
```

`MakeIndex` 不含墙钟无法对拍（whitelist D-1）。红检：金币率 `10/100→5/100` →
红（第 2 行不一致，附录 A-3）；灵敏度边界登记 whitelist D-4。

## 产物 hash（金标准，SHA-256）

```text
5260f55bddf8b3e345ee7085840275ac67d31e0374abdb9e713e653550ad371e  rng_seed42.txt
c01ef17f26a99af9c08feb4b5dae805166f3d734d723b2a338716c704011014d  rng_seed20240229.txt
5c1c45ce949521ccac27dc8d80a7ea17ad25568da2344f6677b4e8af1e792bb7  exp_table.txt
23a2e58317c5cf333c25f2e9980dd5532fea4f5c6b6c6c121c1a0342bbd97c1e  levelabil.txt
e40d78c0cd169acc318d40755c20a840963be1ddd6e78af1af1e3000827f1aec  drop_jiangshi1_100.txt
（drop_synth_*.txt 17 个 hash 见 git 提交内文件）
```

基线提交：`e105c27d`（OpenMir2 master，docs: 新增 OpenMir2 → Rust 重写设计文档）。
门禁命令与重生成流程：`tests/parity/README.md`。

## 附录 A：人工红检输出

### A-1 经验曲线（改 `fixtures/Exps.conf` 的 `Level500=999999`）

```text
test need_exps_table_matches_golden ... FAILED
Level500: rust=999999 csharp=0
assertion failed: NeedExps 全表 diff = 1（要求 0）
```
恢复后：`test result: ok. 4 passed`。

### A-2 RNG（改 `golden/rng_seed42.txt` 第 10 行为 0）

```text
test rng_matches_csharp_golden_seed42 ... FAILED
assertion failed: RNG 序列与 C# 金标准不一致: 第 10 行: rust=`815` csharp=`0`
```
恢复后绿。

### A-3 掉落（`fixtures/MonItems/jiangshi1.txt` 金币率 `10/100→5/100`）

```text
test drop_jiangshi1_100_kills_match_golden ... FAILED
掉落对拍不一致（jiangshi1）第 2 行
```
恢复后：`test result: ok. 4 passed`。
（对照：`10/100→10/101` 的微调未变红——灵敏度边界，whitelist D-4。）

## 门禁汇总

```text
cargo fmt --all --check        # 通过
cargo clippy --workspace --all-targets   # 0 error（workspace lints: clippy::all=deny, unsafe_code=forbid）
cargo test --workspace         # 41 passed / 0 failed / 1 ignored（table_counts 需 MySQL）
cargo test -p mir2-parity-tests --test table_counts -- --ignored   # 通过（本机 MySQL）
```
