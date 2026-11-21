# openmir2-golden-generator-drift — 既有 golden 与当前生成器 schema/墙钟漂移

> **判据原则（2026-11-21 增补）**：**缺键本身不是问题，“缺了又被读”才是问题**——下文 §5b 的普查里，**42/52 缺 poison 六键**之所以不是缺陷，是因为**没有任何消费方**；只有“缺键 + 有消费方”才会静默失效（见 §12 的实例）。

**状态**：事实登记（只读查证）；**待决策（是否整批重生成）**，非紧急。
**查证 head**：master `2692d0a26`。

## 1. 现象
仓库既有 magic golden（如 `mir2-rs/tests/parity/golden/magic/skeleton.json`）是**旧 schema** 生成的；用当前 master 的 MagicGolden 生成器**重跑会出 diff**：
- **新增字段**（结构 diff）：`caster_actor_id` / `target_actor_id` / `recog` / `due_ms_rel` / `hit_speed` / `frame_order_scope`（均已在 master 的 `Program.cs` 里：属性声明见 `FireballResult`，帧字段见 `ParseFrames`；属 c6 的 recog-schema 变更）。
- **值变化**：`delivery`（`GetTickCount` **墙钟**）。

## 2. 影响
- **任何「重生成 == 仓库现状」的判据都不成立**（旧 golden 天然 stale）；用逐字节相等当门是错判据。
- golden 含**墙钟字段**（`delivery`）⇒ **天然不可逐字节复现**。§13 允许"行为等价 + 自报身份"，但**必须明说哪些字段不可复现**。

## 3. 要求（口径）
- 列「哪些 golden 已 stale、哪些字段不可复现」的清单口径：**不可复现 = 墙钟/GetTickCount 派生字段**（至少 `delivery`；`MasterRoyaltyTick` 已按 §36 不记不比）。
- 结论化的回归判据应改为「改动不引入**语义相关**变化」：**结构 diff 的新增键 ⊆ 已登记 schema 变更集，值变化 ⊆ 墙钟集**，逐条归类（附命令）。
- 后续是否**整批重生成**所有 magic golden（一次把 schema/墙钟对齐）——决策点，非今晚解决。

## 4. 证据（本卡生成时）
对 skeleton：结构 diff `ADDED ⊆ {schema}`、`CHANGED = {delivery}`、`SinSu/recall 相关 = 空`、`未归类 = 空`。

## 5. 整类根因：造怪抽数「已记录、两侧记账/消费不同」⇒ RNG 流偏移（不止 SINSU）
`OfflineWorldEngine.RegenMonsterByName` 的替身**造怪抽数是确定且已记录在 golden `Rng` 里的**（`[1500,2000,3000,3000,2000]` + `Random(3)`；前五抽 = `BaseObject(1500,2000)` + `AnimalObject(3000,3000)` + `MonsterObject` ctor `SearchTime = 3000 + Random(2000)`），差异在**两侧的记账/消费口径**：`skeleton`/`angel` 的驱动不做 RNG 记账（台账无 `Rng` 键），`sinsu` 已记账且前 5 抽逐值相同、第 6 抽起不同（`create_monster_oracle` 另多抽，`bins/game-svr/src/mongen.rs:498/501`）⇒ **凡走 `RegenMonsterByName` 的叶子，RNG 流在两实现间错位**，后果有二（同一根因）：
- **`Rng` 长度/逐值不同源**（SINSU 实测 golden 6 抽 vs Rust 8 抽：**前 5 同**、Rust 多 3 抽；skeleton/angel 连账本都没有）⇒ 这类 leaf 当前不宜逐值比 Rng（是**记账/消费口径不同**，非产品错）。
- **帧面 RNG-payload 偏移**（例：SINSU 640 `SM_MAGIC_LVEXP` 的 `tag`=训练 `Random(3)+1`，golden=2 vs rust=3）⇒ 与 drain 窗口无关。
⇒ 凡 `# act` 召唤/造怪类叶子，评审时先问"这个值是否来自造怪/train 抽"；是则从比较面排除并注明此根因。

## 5b. 三类完整分类：装置一行 bug / 历史陈旧 / 待生成值差（52 份普查，2026-11-21 增补）

> 承接 H1 下的**判据原则**（缺键本身不是问题，“缺了又被读”才是问题）。
> 查证 head：master `867290d04`；范围 = `mir2-rs/tests/parity/golden/magic/*.json` 去 `legacy/`（52 份）。
> 与既有节的分工：§5 给**抽数口径**（已记录、两侧记账/消费不同）；§7 给 51 份的**字段出现表**（actor_id/recog/due/delivery）；
> §12 给“缺键 ⇒ 静默失效”的**实例**。本节给**全量 52 份的三类分类 + 消费面 + 待办**，不重复它们的数字。

### 5b.1 类 A：`FireballResult.Angel` 漏 ignore-null（**装置一行 bug，待修**）

- **现象**：51/52 文件的 case 级**没有** `Angel` 键（仅 `angel.json` 有）。
- **根因**：`mir2-rs/tests/parity/csharp/MagicGolden/Program.cs` 的 `FireballResult.Angel` 声明为普通属性，
  **漏了** `[JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]`；同文件 `Skeleton` / `SinSu` / `Train` /
  `StatusTimeArr` / `TargetMonsterHpBefore/After` **都有**该属性（装置自身不一致，angel 批引入）。
- **后果**：任何 stem 用今天的生成器重出 ⇒ 每个 case 多一行 `"Angel": null`（52 份共 144 case）——与取值无关。
- **修法**：补该属性（**本卡不改**；`Program.cs` 正被 MG-55 改，避免撞车，由编排在合入后统一排）。
- **修后可验判据**：
  1. 重生成任一**非 41** stem ⇒ 产物 `grep -c '"Angel"'` = **0**；
  2. 重生成 `angel` ⇒ `Angel` 对象块仍在、字段集合与仓库件一致（值除墙钟外可比）；
  3. 全量重生成后，**类 A 差集 = 0**（下表 A 列归零）。

### 5b.2 类 B：历史陈旧（“重生成 ≠ 仓库件”，**不是缺陷**）

| 差异键 | 受影响 | 装置现状 | **消费面（决定是否要紧）** |
| --- | --- | --- | --- |
| `Sent[].recog` / `Sent[].due_ms_rel` | **23/52 文件**、467/2202 帧 | `Program.cs` 的 `ParseFrames` 现在**无条件**写两键 | 读它们**只有** sinsu（`sent_filter_sinsu`）与施毒 4 支；这些 golden 均在最新 10 份内（已有键）。其余比较器用 `(ident,param,tag,series,tail)` 多重集 ⇒ **不受影响** |
| `Fixture` poison 六键（`amulet_shape`/`amy_ounsul_point`/`red_msg_f_color`/`red_msg_b_color`/`target_anti_poison`/`you_poisoned`） | **42/52** | 装置现在**无条件**写六键（不只 poison stem） | 比较器**不读** `Fixture`；驱动只在 poison stem 用 `golden_fixture_required`，而那些 golden 都有 ⇒ **不受影响** |
| 顶层 `fixture_spec` | `fireball.json`（1 份） | 现行 `EmitGolden` 必写 | Rust 侧**无机器消费**（仅作 rig 几何的手工对齐说明）⇒ **不受影响** |

⇒ 三者都是**历史陈旧**：装置后来变了、golden 没重出。**若将来要把这些键纳入比较面，必须先全量重出**并一次性升级口径。

### 5b.3 类 C：值不同（**必须重生成才能给**）

静态只能查“键的有无”。**帧取值 / HP / Rng 等值差异**只能靠真重生成：
- 逐 case `dotnet run --project mir2-rs/tests/parity/csharp/MagicGolden -c Release -- --case <name> --out <scratch>/<name>.json`（**不跑** `--workspace`）；
- case 覆盖：52 份对应 case（`warrskill` 产 10 份 `warr-*`；`amyounsul-shape2`→`amyounsul2`、`groupamyounsul-shape2`→`groupamyounsul2`、`fireball5`→`fireball-5`、`lightning11`→`lightning-11`、`lightning45`→`lightning-45`）；
- 比较：剔除 `delivery` / `due_ms_rel` / 克隆体 `actor_id` 后，逐文件三栏——**新增键** / **仓库件缺键** / **值不同**；**“值不同”单独点名**（那类才可能藏缺陷）。
- 车道忙时不跑（2026-11-21 时 `LANES=BUSY`，本节当时未生成）。

### 5b.4 非判据字段与 id 两类（防止混谈）

- `delivery`（绝对 tick）、`due_ms_rel`（装置实测毫秒）：两次生成必不同 ⇒ 比较面沿用既有剔除/容差口径。
- **id 必须分两类**：
  - 施法者/目标 `caster_actor_id` / `target_actor_id`：由 `id_base`（1000/2000/3000）派生 ⇒ 生成**稳定**（29 份 `Fixture` 有）。
  - **克隆体自身 `actor_id`**（`HUtil32.Sequence()` 产生）：两次生成**不同**（实测 `1682780500` → `1921834330`）⇒ **不可复现量，不得作判据**。
  - `rel_to_caster`：仓内当前**不存在**此键；若将来出现按克隆体 id 同口径处理。
- 抽数口径见 §5：`skeleton`/`angel` 的驱动**不记账**（台账无 `Rng` 键）⇒ follow-up 是“**先让驱动参与 RNG 记账**”，不是“补消费”；`sinsu` 已记账、前 5 抽逐值相同、第 6 抽起不同。

### 5b.5 逐文件普查表（52 份）

| 文件 | cases | 帧 | A:缺 `Angel` | B1:帧缺 `recog`/`due_ms_rel` | B2:缺 poison 六键 | B3:无 `fixture_spec` |
| --- | ---: | ---: | :---: | :---: | :---: | :---: |
| `amyounsul` | 3 | 42 | 是 | — | — | — |
| `amyounsul2` | 3 | 42 | 是 | — | — | — |
| `angel` | 2 | 13 | — | — | — | — |
| `bigcloak` | 3 | 39 | 是 | 是 | 缺 6 键 | — |
| `bighealing` | 3 | 17 | 是 | — | 缺 6 键 | — |
| `cloak` | 3 | 30 | 是 | 是 | 缺 6 键 | — |
| `dejiwonho` | 3 | 39 | 是 | — | 缺 6 键 | — |
| `energyrepulsor` | 3 | 30 | 是 | — | — | — |
| `fire` | 3 | 47 | 是 | — | 缺 6 键 | — |
| `fireball-5` | 3 | 29 | 是 | 是 | 缺 6 键 | — |
| `fireball` | 3 | 29 | 是 | 是 | 缺 6 键 | 是 |
| `fireboom` | 3 | 39 | 是 | — | 缺 6 键 | — |
| `fireboom47` | 3 | 39 | 是 | — | 缺 6 键 | — |
| `firecharm` | 3 | 32 | 是 | 是 | 缺 6 键 | — |
| `firewind` | 3 | 30 | 是 | — | — | — |
| `groupamyounsul` | 3 | 42 | 是 | — | — | — |
| `groupamyounsul2` | 3 | 42 | 是 | — | — | — |
| `groupdeding` | 5 | 65 | 是 | 是 | 缺 6 键 | — |
| `grouplightening` | 3 | 36 | 是 | 是 | 缺 6 键 | — |
| `hangmajinbub` | 3 | 903 | 是 | — | 缺 6 键 | — |
| `hbfireball` | 3 | 32 | 是 | — | 缺 6 键 | — |
| `healing` | 3 | 17 | 是 | — | 缺 6 键 | — |
| `holyshield` | 4 | 26 | 是 | 是 | 缺 6 键 | — |
| `lightflower` | 3 | 39 | 是 | — | 缺 6 键 | — |
| `lightning-11` | 3 | 29 | 是 | 是 | 缺 6 键 | — |
| `lightning-45` | 3 | 29 | 是 | 是 | 缺 6 键 | — |
| `mabmabe` | 3 | 29 | 是 | — | — | — |
| `shield` | 3 | 27 | 是 | — | 缺 6 键 | — |
| `showhp` | 3 | 23 | 是 | 是 | 缺 6 键 | — |
| `sinsu` | 2 | 16 | 是 | — | 缺 6 键 | — |
| `skeleton` | 2 | 13 | 是 | — | 缺 6 键 | — |
| `skill10` | 3 | 47 | 是 | — | 缺 6 键 | — |
| `skill49` | 3 | 18 | 是 | — | 缺 6 键 | — |
| `skill51` | 3 | 18 | 是 | — | 缺 6 键 | — |
| `skill52` | 3 | 18 | 是 | — | 缺 6 键 | — |
| `skill53` | 3 | 18 | 是 | — | 缺 6 键 | — |
| `skill54` | 3 | 18 | 是 | — | 缺 6 键 | — |
| `snowwind` | 3 | 39 | 是 | — | 缺 6 键 | — |
| `spacemove` | 3 | 45 | 是 | — | — | — |
| `turnundead` | 3 | 16 | 是 | — | — | — |
| `uenhancer` | 3 | 39 | 是 | 是 | 缺 6 键 | — |
| `unamyounsul` | 4 | 22 | 是 | 是 | 缺 6 键 | — |
| `warr-12` | 2 | 4 | 是 | 是 | 缺 6 键 | — |
| `warr-25` | 2 | 4 | 是 | 是 | 缺 6 键 | — |
| `warr-26` | 3 | 5 | 是 | 是 | 缺 6 键 | — |
| `warr-27` | 1 | 5 | 是 | 是 | 缺 6 键 | — |
| `warr-3` | 1 | 3 | 是 | 是 | 缺 6 键 | — |
| `warr-34` | 2 | 4 | 是 | 是 | 缺 6 键 | — |
| `warr-38` | 2 | 4 | 是 | 是 | 缺 6 键 | — |
| `warr-4` | 1 | 3 | 是 | 是 | 缺 6 键 | — |
| `warr-43` | 2 | 4 | 是 | 是 | 缺 6 键 | — |
| `warr-7` | 1 | 3 | 是 | 是 | 缺 6 键 | — |
| **合计** | **144** | **2202** | **51 份** | **467/2202 帧（23 份）** | **42 份** | **1 份** |

### 5b.6 待办

1. **类 A 一行修复**（§5b.1），MG-55 合入后统一排；修后按三条判据验（尤其“全量重生成后 A 列归零”）。
2. **类 C 全量重生成**（§5b.3），先报车道；结果回填本节，**值不同单独点名**。
3. **RNG 记账 follow-up**（§5）：`skeleton`/`angel` 驱动先落 `Rng`，再谈消费/比对。
4. 类 B 是否全量重出，等类 C 结果一并决策（避免二次翻动）。


## 6. 共享替身变更：预置 slave 现在带 `CurrX/Y=0` + `Envir`
`Program.cs` 的 `PreSlaveCount` 预置 slave 构造（`RunMagicCase`）现为每个占位 slave 设 `CurrX=CurrY=0` 与 `Envir=caster.Envir`（此前不设）。
- **为什么**：`RecallSlave → SlaveList[i].SpaceMove(...)` 需目标对象 `Envir` 非空才能移动（SINSU `cap_hit` 回召面；见 `openmir2-golden-harness-mapmgr`）。
- **当前为何无影响**：`PreSlaveName=="pre-slave"` 的既有用例名字构造不变、`golden` 未重出 ⇒ 现状零 diff。
- **将来重生成注意**：若某用例的期望面涉及预存 slave 的坐标/`Envir`，重生成可能出 diff（预置 slave 从旧默认变为显式 `(0,0)`+env）；届时按「共享替身变更」归类，不是产品行为变化。

> **§7–§11 由 QA `openmir2-3b` 追加**（来源 `docs/issues/qa-golden-drift-inventory-20261120.md`；清单查证 head = master `01da212ce`）。与既有 §5（整类根因）/§6（共享替身）不重复，不覆盖任何原正文。

## 7. 逐文件字段表（51 份；来源：QA `openmir2-3b`）
| 文件 | actor_id | recog | due_ms_rel | delivery(墙钟) |
| --- | --- | --- | --- | --- |
| `bigcloak.json` | — | — | — | Y |
| `bighealing.json` | — | — | — | Y |
| `cloak.json` | — | — | — | Y |
| `dejiwonho.json` | — | — | — | Y |
| `fire.json` | Y | Y | Y | Y |
| `fireball-5.json` | — | — | — | Y |
| `fireball.json` | — | — | — | Y |
| `fireboom.json` | — | — | — | Y |
| `fireboom47.json` | — | — | — | Y |
| `firecharm.json` | — | — | — | Y |
| `groupamyounsul.json` | — | — | — | Y |
| `groupdeding.json` | — | — | — | Y |
| `grouplightening.json` | — | — | — | Y |
| `hangmajinbub.json` | — | — | — | Y |
| `hbfireball.json` | — | — | — | Y |
| `healing.json` | — | — | — | Y |
| `holyshield.json` | — | — | — | Y |
| `legacy/bigcloak.json` | — | — | — | Y |
| `legacy/bighealing.json` | — | — | — | Y |
| `legacy/cloak.json` | — | — | — | Y |
| `legacy/dejiwonho.json` | — | — | — | Y |
| `legacy/fireball-5.json` | — | — | — | Y |
| `legacy/fireball.json` | — | — | — | Y |
| `legacy/fireboom.json` | — | — | — | Y |
| `legacy/firecharm.json` | — | — | — | Y |
| `legacy/hangmajinbub.json` | — | — | — | Y |
| `legacy/hbfireball.json` | — | — | — | Y |
| `legacy/healing.json` | — | — | — | Y |
| `legacy/lightning-11.json` | — | — | — | Y |
| `legacy/lightning-45.json` | — | — | — | Y |
| `legacy/shield.json` | — | — | — | Y |
| `legacy/showhp.json` | — | — | — | Y |
| `legacy/snowwind.json` | — | — | — | Y |
| `lightflower.json` | — | — | — | Y |
| `lightning-11.json` | — | — | — | Y |
| `lightning-45.json` | — | — | — | Y |
| `shield.json` | — | — | — | Y |
| `showhp.json` | — | — | — | Y |
| `skeleton.json` | — | — | — | Y |
| `snowwind.json` | — | — | — | Y |
| `uenhancer.json` | — | — | — | Y |
| `unamyounsul.json` | — | — | — | Y |
| `warr-12.json` | — | — | — | Y |
| `warr-25.json` | — | — | — | Y |
| `warr-26.json` | — | — | — | Y |
| `warr-27.json` | — | — | — | Y |
| `warr-3.json` | — | — | — | Y |
| `warr-34.json` | — | — | — | Y |
| `warr-38.json` | — | — | — | Y |
| `warr-4.json` | — | — | — | Y |
| `warr-7.json` | — | — | — | Y |

## 8. 分组结论
- **完全新 schema（actor_id + recog + due_ms_rel 全有）= 1 份**：`fire.json`
- **完全旧 schema（三项全无）= 50 份**（即上表除 `fire.json` 外的全部文件，含 9 份 `warr-*` 与 17 份 `legacy/*`）
- **混合（部分有）= 0 份** —— **本批为零，无需点名**。
- 附：`hit_speed` 键：**18 份有 / 34 份无**（`git grep -l '"hit_speed"' master -- <dir> | wc -l` = 18）；`frame_order_scope` 仅 `fire.json` 3 处。
- **`delivery`（墙钟）在 51 份 JSON 中全部存在**（`Rm` 记录里）。

## 9. 整批重生成的代价（必然 diff 的字段）
用当前 master 生成器重跑：
1. **50 份旧 schema 文件**：结构 diff 必然包含
   - `Fixture` 增 `caster_actor_id` / `target_actor_id`（`MagicGolden/Program.cs` 的 `caster_actor_id` / `target_actor_id` 赋值，**无条件写**）；
   - **每一条 `Sent` 帧**增 `recog` 与 `due_ms_rel`（同文件帧记录构造处的 `recog` / `due_ms_rel` 字段，**无条件写**）；
   - `Fixture` 增 `hit_speed`（同文件 `hit_speed` 字段，**无条件写**）。
2. **全部 51 份**：`delivery` 是 `GetTickCount` 墙钟 ⇒ **重生成值必变、逐字节复现不可能**（含已是新 schema 的 `fire.json`）。
⇒ 判据不能是"重生成 == 仓库现状"，只能是卡 `openmir2-golden-generator-drift.md` §3 的"结构 diff ⊆ 已登记 schema 变更集、值变化 ⊆ 墙钟集"。

## 10. `hit_speed` 键的 18 份 = 哪一代
口径更正：按 **JSON 51 份**统计 —— **18 份有该键 / 33 份无**；此前"34 无"是把 `legacy/README.md`（非 golden）也算进去了。

| 分布 | 文件 | 判定 |
| --- | --- | --- |
| `hit_speed` **值 = 7**（6 份） | `cloak` / `bigcloak` / `shield` / `uenhancer` / `hangmajinbub` / `dejiwonho` | **7 代点亮件**（657 `series` 旋钮生效的那六份） |
| `hit_speed` **值 = 0**（12 份） | `fire` / `fireball-5` / `fireboom` / `fireboom47` / `firecharm` / `hbfireball` / `healing` / `bighealing` / `lightflower` / `lightning-11` / `lightning-45` / `snowwind` | **有键但退化值**（生成器已带该字段，夹具未设 7） |
| **无该键**（33 份） | 其余全部（含 9 份 `warr-*`、17 份 `legacy/*`、`groupdeding`/`grouplightening`/`groupamyounsul`/`holyshield`/`skeleton`/`showhp`/`unamyounsul`） | **0 代 schema** |

⇒ 同一目录混着**两代（甚至三种形态）**夹具；重生成时 `hit_speed` 是**第二个必然 diff 源**（对外层 33 份为"新增键"，对值 0 的 12 份为"值可能变"，对 6 份 7 代件为"值应不变但键序/格式可能变"）。

## 11. 不可复现字段：`delivery`（全部 51 份）
**"`delivery` 在全部 51 份 golden 中都存在（`Rm` 记录的墙钟）⇒ 不是'某些文件旧了'，而是'所有 golden 都含不可复现字段'"**；
按 §13 口径：行为等价 + 自报身份，不要求字节级，但**必须逐字段写明不可复现集合**（至少 `delivery`）。

## 12. 缺键 ⇒ 整段判据静默失效（`due_ms_rel` 实例）

比较器只在 **golden 第一帧带该键**时才把字段纳入比较面（`magic_fireball_parity.rs` 的
`sent_fields.push("due_ms_rel")`）；而**收件人级过滤**（如 `sent_filter_sinsu` 的 `keep` 白名单）
还会把某些 ident 整条移出比较面。两种机制叠加时会出现「金标准与实现不一致、判据却全绿」：

- **实例 A（缺键）**：`skeleton.json` 属旧世代（`Sent` 无 `due_ms_rel`）⇒ 640 的时序整段不判；
  用现行装置重生成可得 `640 due=1000`，而 Rust 在施法批发（`due=0`）——旧件下看不出来。
- **实例 B（keep 过滤）**：`sinsu.json` 的 caster 640 是 `due=1000`，Rust 台账全帧 `due=0`，
  但 `sent_filter_sinsu` 只留 `17/638/642` ⇒ 640 被过滤面吃掉，**当前绿≠已验**。
  **已收口（2026-11-21）**：`keep` 不动 + 补「只判 `due_ms_rel`」的不对称断言（见 sweep 卡 #6）。

**防御**：golden 与实现任一侧缺键 ⇒ 该字段判「未验」并显式登记（不得默认通过）；过滤白名单
（`keep`）必须在卡里列出**被排除的 ident 及其理由**。对应收口：`openmir2-magic-640-delay-sweep`。
