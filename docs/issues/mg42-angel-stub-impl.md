# MG-42（41 `SKILL_ANGEL`）stub 交付记录

> 分支 `card/mg42-angel-summon`，base master `0e2eb1316`。
> 数据缺口卡：`docs/issues/openmir2-angel-summon-monster-missing.md`（本批**不改产品代码**）。

## 1. SCOPE（写死，别扩大）

走 **stub**：`mir2-rs/tests/parity/csharp/MagicGolden/OfflineWorldEngine.cs` 的
`RegenMonsterByName` 是离线替身（17/30 同法）。本批证明的是「**给定一个怪物之后**的
adopt 行为」：

- `MakeSlave` 的 `Master` / `IsSlave` / `SlaveMakeLevel` / `SlaveExpLevel` / `RecalcAbilitys` /
  半补 / `RefNameColor` / `SlaveList`；
- 施法帧 41/638/642、训练尾 640 的自延迟（+1000ms 交付批）；
- `cap_hit`：`MakeSlave(..., nMaxMob=1)` 的 `SlaveList.Count < 1` 门失败 ⇒ 不新增实体、不造怪。

**不证明生产可召唤**：冻结 `sql/mir2_data.sql` 的 `monsters` 无 `精灵` 行 ⇒ `GetMonRace`
（`src/GameSrv/Word/WorldServer.cs:167-174`）返 -1 ⇒ `CreateMonster`
（`src/GameSrv/Word/WorldServer.MonGen.cs:498`）无 `default` 臂 ⇒ `MakeSlave` null。
**升格 scope 必须与本 SCOPE 同文**；不得把本 golden 的绿灯写成"生产可召唤"。

## 2. 交付面

| 面 | 落点 |
| --- | --- |
| 装置 | `--case angel`（`AngelBatch`，summon_ok/cap_hit）+ `Angel` 观测块（镜像 `Skeleton`） |
| golden | `mir2-rs/tests/parity/golden/magic/angel.json` |
| 驱动 | `tests::angel_parity_summon_ok` / `tests::angel_parity_cap_hit`（`--exact --ignored`，同 `RUN_ID`） |
| 比较器 | `real_rust_ledger_angel`（`--exact --ignored`） |

## 3. 不进入比较面的字段（实测口径）

- **`Rng`**：离线替身 `RegenMonsterByName` 不抽造怪数，Rust `create_monster_oracle` 真抽
  ⇒ 两侧不同源（与 17/30 同口径）。
- **640 的 `tag`**（TranPoint）：随造怪抽数不同（与 17 同口径）；比较
  `(recog,param,series,due)`，`due` 按既有 `|Δ|≤32ms` 判。

## 3b. §57 规则 3：新增 `#[ignore]` 用例由谁跑

`tests::angel_parity_{summon_ok,cap_hit}` 与 `real_rust_ledger_angel` 由**合并门**
`parity-gate.ps1 -Mode Both` 自动发现，并逐个 `--exact --ignored` 跑（驱动与比较器共用同一次
`RUN_ID`）。本批全量门实测：**Driver 127 支 / Comparator 43 支**（angel 在集合内且绿；
唯一红是已登记的 `spacemove 11029`）。

## 4. 前卡边界

`41` 的"施法帧 + 640 自延迟"原本只由单测 `spell_cmd_angel_training_tail_is_self_delayed` 覆盖
（其注释是"41 无 golden ⇒ 本条是唯一判据"）。本批之后该单测**不再是唯一判据**：
golden + 比较器 + 驱动共同覆盖同一行为面（机制级单测仍保留）。
