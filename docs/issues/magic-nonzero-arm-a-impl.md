# 非零值魔表对照臂 Arm A——实现与证据（2026-11-21）

> 计划卡：`docs/issues/magic-nonzero-table-arm-plan.md`（`7c`，master `8ce09f334`）。
> 实现分支：`card/nonzero-arm-a`（base `8ce09f334`）。
> 红线自证：**产品代码 0 改动**（`MagicInfo` 结构、`MagicManager`/`MagicBase`/Rust `world.rs` 均未改）；
> 不新增 MG 条目、不改 `parity-gap-magic.json`；**既有 9 支 golden 一律未重生成**；不 `#[ignore]` 掩盖红。

## 1. 提交（分步）

| 提交 | 内容 |
| --- | --- |
| `738f4fd7f` | C# 生成器：`MagicSpec` 五字段 + `BuildMagicInfo` + Fixture 非零才落键 + `--nonzero-control` + 三支 `--case` |
| `67c216897` | Rust 夹具：`fireball_parity_drive` 从 golden 读五字段（缺键=0；`-nonzero` fail-closed）+ 文件名映射 |
| `97e798ef2` | 三支新 golden（首版） |
| `21d2834e6` | `StepDrainMs=100` 重出三份 golden（修首版 `due_ms_rel golden=0 / rust=600` 的记录面伪影） |
| `d2cf9a4ed` | 三支驱动 + 三支比较器 + `nonzero_fixture_expect` per-stem 期望表 |

## 2. 五字段与部署真值（`sql/mir2_data.sql` 生效行）

| MagID | 名称 | 生效行 `Idx` | Power | MaxPower | DefPower | DefMaxPower | TrainLv |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | 火球术 | 1 | 8 | 8 | 2 | 2 | 3 |
| 11 | 雷电术 | 11 | 14 | 28 | 10 | 10 | 3 |
| 24 | 地狱雷光 | 24 | 10 | 30 | 10 | 30 | 3 |

- `LoadMagicDB` **恒置 `TrainLv = 3`**；`WorldServer.FindMagic` 取 `select *` 读到的**第一行** ⇒
  重复 MagID 必须按最小 `Idx` 取。**48 未做**；若做须取 `Idx=38` 的「群体施毒术」（10/10/4/4），
  不是 `Idx=48` 的「气功波」（1/1/0/0）。
- golden 键：`magic_power` / `magic_max_power` / `magic_def_power` / `magic_def_max_power` /
  `magic_train_lv`；**非零才写、缺键=0**，旧 stem 全部缺键 ⇒ 与 `fireball_rig` 缺省表逐字段相同。
- Rust `MagicInfo` 同名字段：`power: u16` / `max_power: u16` / `def_power: u8` /
  `def_max_power: u8` / `train_lv: u8`；`max_train` 不属 Arm A，两侧保持缺省 0。

## 3. arm vs `--nonzero-control` 差异清单（同 label/seed；`delivery` 墙钟除外）

| stem | 差异（control → arm） |
| --- | --- |
| `fireball-nonzero` | miss 无差异（无伤害）；edge `TargetHp` 85→79、SM31 `series` 15→21；hit 91→85、9→15；`8021/8044/8103` 的 `wParam/p1` 随动 |
| `lightning11-nonzero` | miss 无差异；edge Rng 第 2 抽 0→1、HP 85→67、series 15→33；hit Rng 第 2 抽 0→3、HP 91→72、series 9→28 |
| `lightflower-nonzero` | s1/s5/s13 首抽 0→4/6/10，HP 99→97 / 98→96 / 97→94，series 1→3 / 2→4 / 3→6；`640` 的 `due_ms_rel` control 1000 → arm 800（`TrainLv=3` 训练支） |

三支在至少一个有意义的伤害字段上与零值对照不同 ⇒ 均有判别力（不是"两支一样"）。

**关于 `21d2834e6` 的 `StepDrainMs=100`（回答 opc-19 ②1）**：这是**让装置按真实时间步进排水**
（`MagicGolden` 的既有口径：每 100ms 睡眠、只在到期时刻处理消息；`firecharm/hbfireball/fireboom`
等既有 stem 同款），**不是把差异藏到不观测的窗口**。首版 `StepDrainMs=0` 是"睡满 1200ms 后一次排空"，
600ms 交付帧被记录成 `due_ms_rel=0`（记录面伪影）；Rust 驱动一直按 100ms 步进世界时钟并记 600/800/1000。
改后**arm vs control 的 HP/series/Rng 差异逐值不变**，只有记录的 due 从 0 变为真实值；因此那 28 条不是
产品分歧，是记录面口径修正。

**跑批 run_id（数值证据）**：Rust 驱动 + 比较器 `nonzero-final-1795242871`（9 驱动 + 3 比较器全绿）；
合并前门 `parity-gate -Mode Both` 的 run_id `nonzero-arm-gate-1795242993`（Driver 150 / Comparator 50 全绿）。
差异清单本身由 C# 生成器的 `--case <stem> --out <临时文件>` 直接产出，不依赖 Rust 台账。
**`StepDrainMs=100` 的边界**：判据 = 改后 golden 的 `due_ms_rel` 仍表达延迟量
（`lightflower` control 1000 / arm 800 即证明），且 100ms < 最小可判差 200ms。
**若未来出现延迟差 < 100ms 的支，本步长不再保真，需重估**。

## 4. 判据（per-stem 期望表）

- 比较器 `real_rust_ledger_{fireball,lightning11,lightflower}_nonzero` 之前调用
  `nonzero_fixture_expect`：**只对新增 stem** 断言五键 = §2 的 SQL 真值（防 golden 重生成时静默换行）；
  其余 stem 不走该表。
- 常规面（复用既有比较器）：逐接收者 SM 严格序 + 字段/尾、RM 多重集、`TargetHp`、RNG 逐值。
- 驱动 `seed` 一律取自 golden（`golden_seed`），不写死。

## 5. 变异牙（产品侧五字段读取恒零）

- 变异（**注入口/读侧变异，非产品逻辑变异**；按判据 8 属"注入口"）：把 `fireball_parity_drive`
  的五字段读取改为 `0u16/0u8`（快照 `E:/UserTemp/nonzero-arm/lib.rs.snap`）。它证明"记录值→消费值"
  这条链在 golden↔Rust 跨栈比较上会红，不声称产品公式被改；真值面由 golden 的 SQL 值 + per-stem 表守。
- 牙（2026-11-21 补）：五字段读取恒零 = **注入口变异（driver 读路径）** ⇒ 驱动+比较器双层红；
  **非产品侧变异**。
- 红（`MIR2_LEDGER_RUN_ID=nonzero-mut-1795242804`）：
  - 驱动：`fireball-nonzero` edge/hit、`lightning11-nonzero` edge/hit、`lightflower-nonzero` s1/s5/s13
    （三支 miss 无伤害，保持绿）；
  - 比较器 3 支全红，示例：
    `[edge] TargetHp 不等：golden=79 rust=85`；
    `[edge/caster] Rm 多重集不等： golden 有 rust 无=[[8021,21,6553701,2,2002,255,1],[8103,21,79,224,2001,167,0]] rust 有 golden 无=[[8021,15,…],[8103,15,85,…]]`。
- 既有 stem 保持绿（`nonzero-mut-1795242858`）：`fireball` / `fireball5` 六驱动 +
  `real_rust_ledger_fireball5` / `real_rust_ledger_parity` 两比较器。
- 还原：`sha256 0c39e165…` 相等、`MUTATION` 残留 0；还原后 9 驱动 + 3 比较器复跑全绿
  （`nonzero-final-1795242871`）。

## 6. 门与门集合

- `cargo fmt --all -- --check`：rc=0；
- `cargo check -p game-svr --lib -p mir2-parity-tests --tests`：rc=0；
- `cargo clippy -p game-svr -p mir2-parity-tests --all-targets -- -D warnings`：rc=0；
- `cargo test -p game-svr --lib`：**2204 passed / 0 failed / 158 ignored**（前值 149，**+9 全是本批三支
  stem 的驱动**，均 `#[ignore]`：`tests::fireball_nonzero_parity_{miss,edge,hit}`、
  `tests::lightning11_nonzero_parity_{miss,edge,hit}`、`tests::lightflower_nonzero_parity_{s1,s5,s13}`）；
  三个比较器在 `mir2-parity-tests` 里也是 `#[ignore]`，不在 `--lib` 计数内，由 `parity-gate` 覆盖；
- `cargo test --no-run -p game-svr --lib` / `-p mir2-parity-tests --test magic_fireball_parity`：rc=0；
- `parity-gate.ps1 -Mode Both -RunId nonzero-arm-gate-1795242993`：
  **Driver 150 ok / 0 红 / 0 零匹配；Comparator 50 ok / 0 红 / 0 零匹配；PARITY_GATE_OK**。
  - 本批新增：Driver **+9**（`fireball_nonzero_parity_{miss,edge,hit}`、
    `lightning11_nonzero_parity_{miss,edge,hit}`、`lightflower_nonzero_parity_{s1,s5,s13}`）；
    Comparator **+3**（`real_rust_ledger_{fireball_nonzero,lightning11_nonzero,lightflower_nonzero}`）。
  - 前值（本批前）= 141 / 47（150−9 / 50−3）。

## 7. 零漂移（base/new 生成器快照）

- 取 base `master:mir2-rs/tests/parity/csharp/MagicGolden/Program.cs`（`8ce09f334`）与实现版
  （`sha256 be640409…`）分别 build，生成三支既有代表 stem（`fireball` / `lightning11` / `lightflower`）：
  **剥 `delivery` 后逐字段 EQ**（base vs new 均为 EQ）。
- **剥 `delivery` 后的字段清单**（base/new 逐字段比较）：顶层 `magic_id / anti_magic / fixture_spec / cases`；
  每 case 的 `Fixture`（全部键）、`Label / DrainPasses / DrainPassesHitCap / Seed / SpellReturn /
  Receivers / Rng / TargetHp / TargetHpBefore / TargetMonsterHpBefore / TargetMonsterHpAfter /
  NotDue / CasterMp / CasterExtraAbil0 / Train`；`Receivers` 内 `Who / Rm`
  （`ident/wParam/p1/p2/p3/msg/late/priority`）与 `Sent`
  （`ident/recog/param/tag/series/due_ms_rel/tail`）。
  **唯一剥离字段 = `delivery`（墙钟，跨栈不可比）**；`NotDue` 未剥离。
- 其余既有 stem 结构上不受影响：`NonzeroArm=false` ⇒ 五键不落盘（`AttachStatusTimeArr` 的
  `HasValue` 门），`BuildMagicInfo` 五字段为 0（= ctor 缺省），`magic_train_lv` 旧门保持。

## 8. 未做 / 边界

- **Arm B（整行：DefSpell/Delay/TrainLevel/Effect/Job 等）不做**（计划卡 §2.4）。
- **48 第四支未做**（前三支已覆盖取整/固定项/两条随机区间三种差异类型）。
- `MaxTrain[0]` 不在 Arm A 五字段内：两侧保持 0（`lightflower-nonzero` 的 640 800ms 来自
  `TrainLv=3` 训练支，两侧一致）。
- 未改产品代码、未新增 MG 条目、未重生成既有 golden。
