# openmir2-serverconf-nonnumeric-asymmetry

> 状态：**已登记（2026-11-21，opc-19 裁决：保留更严格的 Rust 行为，登记为偏差）**。
> 来源：卡 `openmir2-magic-serverconf-batch-plan` §6.7 的「Server.conf 解析对拍照」实现过程（卡
> `openmir2-serverconf-parse-parity`）。

## 现象（两侧对同一输入的解析语义不同）

- **oracle（C#）**：`ServerConf.LoadConfig()` → `ConfigFile.ReadWriteInteger`（`src/OpenMir2/Common/ConfigFile.cs:55-63`）
  → 键存在时调 `ReadInteger`（同文件 `:170-185`）：值非空时 `int.TryParse` 失败 ⇒ **回落默认值**并继续；
  空值也回落默认值。即「非空非数字」不报错。
- **Rust**：`bins/game-svr/src/config.rs` 的 `[Setup]` 解析对 `MABMABEHITRANDRATE`
  （`:1279-1283`）、`MABMABEHITMINLVLIMIT`、`MABMABEHITSUCESSRATE`、`MABMABEHITMABETIMERATE`、
  `AMYOUNSULPOINT` 等键用 `v.parse().with_context(...)?` ⇒ **非数字直接返回 `Err`**（启动失败）。
  `MAGTURNUNDEADLEVEL` 等少量键有 `!v.is_empty()` 守卫（空值回落缺省），但**非数字仍 Err**。
- 空值口径补充：**带守卫的键**（如 `MagTurnUndeadLevel=`）两侧都回落缺省，一致；
  **无守卫的键**（MabMabe* / AmyOunsulPoint）空值走 `parse("")` ⇒ Rust Err，与「非数字」同类。
- **第二个 Rust 解析器 `PetSlaveConfig`（`crates/data/src/pet_slave.rs`，读 `[Setup]` 的
  `MagTammingLevel/TargetLevel/TargetHPRate/Count`）不属于本偏差**：它走 `IniFile::read_integer`
  （`crates/data/src/exps.rs` 的 `read_integer`）——缺键、空值、非数字**一律回落 `def`**
  （`unwrap_or(def)`），与 C# `ReadWriteInteger/ReadInteger` 语义一致。

## 三态语义对照（同一输入 → 两侧结果）

| 输入形态 | C#（`ConfigFile`） | Rust `config.rs`（MabMabe*/Amy 等无守卫臂） | Rust `PetSlaveConfig` |
| --- | --- | --- | --- |
| 缺键 | 回落 ctor 缺省 | 回落 `unwrap_or` 缺省 | 回落 `def` 缺省 |
| 空值（`KEY=`，**无** `!v.is_empty()` 守卫） | 回落缺省 | **`Err`** | 回落缺省 |
| 空值（`KEY=`，**有**守卫，如 `MagTurnUndeadLevel`） | 回落缺省 | 回落缺省 | 回落缺省 |
| 非数字 | 回落缺省 | **`Err`** | 回落缺省 |

⇒ 只有 `config.rs` 的**无守卫 + 空值/非数字**两个格子是不对称（启动期 fail-closed）。
注意「回落缺省值」不等于「等于部署值」：`pet_empty.conf`（`MagTammingLevel=`）回落的是缺省 **50**，
而部署值是 **40** —— 这个差值本身证明「空值 ≠ 部署」，对拍照对此有独立断言。

## 影响面

- 仅**启动期配置解析**：输入是运维手写的 `Server.conf`；不存在任何帧面/协议差异
  （不改变任何 `SM_*` 帧、抽数或行为面）。
- Rust 侧更严格（打错字即 fail-closed 停机）；oracle 侧静默吃缺省继续跑。

## 当前决定

**保留 Rust 的严格行为**，登记为已知偏差：把 Rust 改成「非数字回落缺省」是**产品行为变更**
（会让配置打错字时静默使用缺省值），不应夹带在夹具批次里做。

- 对拍测试（`mir2-rs/bins/game-svr/tests/serverconf_parse_parity.rs`）把该输入断言为**已知不对称**：
  C# 侧值 == 缺省（`MabMabeHitRandRate=100`），Rust 侧 `load()` 返回 `Err`；不是"两侧相等"。
- 偏差清单的 `deviation` 条目由 QA（`3b`）按既有机制处理；本卡只出事实与决定。

## 移除条件（满足任一即重开）

1. 产品决定与 oracle 对齐（`Server.conf` 打错字静默回落缺省）⇒ 改 `config.rs` 的对应解析臂为
   `unwrap_or(default)` 并同步把对拍用例改成「两侧值相等」；
2. oracle 侧改为 fail-closed（`ReadWriteInteger` 对非数字抛错/停机）⇒ 两侧自然一致，本卡可关闭。

## 证据（可复现）

- 合成夹具 `mir2-rs/tests/parity/fixtures/serverconf/nonnumeric.conf`（`MabMabeHitRandRate=abc`）与
  `empty_mabmabe.conf`（`MabMabeHitRandRate=`）；
- C# 探针实测：`mab_mabe_hit_rand_rate=100`（回落缺省）；
- Rust：`game_svr::config::load()` 返回 `Err`（`MabMabeHitRandRate 不是合法整数："abc"` / 空串同）；
- 对照：`pet_nonnumeric.conf` / `pet_empty.conf` 两侧一致回落 50（`PetSlaveConfig` 无此偏差）。

## 变异刀与恢复 sha（证据对准树）

对拍照的两条变异刀都只改 `deploy.conf`，**恢复后 `sha256(deploy.conf)` 与备份相等**：

- 2026-11-21（六键版）：`deploy.conf` sha256 =
  `f64e902a299e7683581d534fa671b3cff6adcb1be4b68d0914c4390cbeee0498`
  （`MagTurnUndeadLevel` 44→50 红后恢复验证）。
- 扩到十键后：`deploy.conf` sha256 =
  `426203f675e84e1a099373942c755288a54fca183974bdf85a4f2b6dfb5838da`
  （`MagTurnUndeadLevel` 44→50、`MagTammingLevel` 40→50 两刀各自红后恢复验证）。

红原文（摘要）：`assertion left == right failed: [deploy.conf] C# 侧解析值与期望值不一致`，
`left: [50, 100, 10, 21, 20, 15, 40, 1, 100, 5]` / `right: [44, …]`（第一刀），
`left: […, 50, 1, 100, 5]` / `right: […, 40, 1, 100, 5]`（第二刀）。

## 与"缺省≠部署"的关系（四条，不是五条）

对拍照同时钉住**缺省与部署不一致**的键：`MagTurnUndeadLevel` 50→**44**、`AmyOunsulPoint` 10→**15**
（`config.rs` 六键）；`MagTammingLevel` 50→**40**、`MagTammingTargetLevel` 10→**1**
（`PetSlaveConfig` 四键）。这四条由测试的**绝对期望值**断言覆盖；与本节的不对称是两个独立问题。
