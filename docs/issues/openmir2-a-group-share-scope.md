# A 堆共享组（5 条）的 scope 与 slave/clone 装置契约（2026-11-20）

> 只读产物；status 由 QA 签后改。行号按 `165c6edd1`。

## 一、5 条共享组：`scope = shares_effect_evidence_with_MG-xx`

**共享的到底是什么**：同一魔号、同一 `case` 组（`src/M2Server/Magic/MagicManager.cs:315-321`）、同一内层效果分派行、
同一 helper —— **不是**"与已验证条目相同"这种一句话。

| 条目 | 魔号 | 内层分派 | 共享对象（effect evidence 引用） |
|---|---|---|---|
| MG-20 | 13 | `:328` | MG-27：golden `golden/magic/firecharm.json`，比较器 `real_rust_ledger_firecharm` |
| MG-21 | 14 | `:352` | MG-28：`hangmajinbub.json`，`real_rust_ledger_hangmajinbub` |
| MG-22 | 15 | `:359` | MG-29：`dejiwonho.json`，`real_rust_ledger_dejiwonho` |
| MG-25 | 18 | `:378` | MG-32：`cloak.json`，`real_rust_ledger_cloak` |
| MG-26 | 19 | `:384` | MG-33：`bigcloak.json`，`real_rust_ledger_bigcloak` |

**两侧分派元组（逐字段）**
| 字段 | oracle | Rust | 相等 |
|---|---|---|---|
| 组门 `CheckAmulet` | `(1,1)`（`:322-325`） | `amulet_requirement`: `13..=19 => Some((1,1))` | ✅ |
| 组门随机数 | **0 抽**（`MagicBase.cs` 的 `CheckAmulet` 无 `Random` 调用） | 同（`magic::check_amulet` 无 RNG） | ✅ |
| 效果分派 | 内层 `switch(MagicId)` 各一支 | `lib.rs` B3 分组匹配臂（14/15 一臂、16/18/19 一臂、13 单支） | 魔号→helper 一致（逐臂行号见 A 堆表） |

**写 scope 的前提（机检/触发条件，进各条 note）**：`IF 任一分派元组字段（CheckAmulet 参数 / 抽数 / helper 身份）不等
THEN scope 作废，该条立即降级为自行效果对拍`；当前五条逐项相等，故可写
`scope = shares_effect_evidence_with_MG-xx`。

## 二、`ExtraAbil` 负结论（17 上不咬）
`grep -n "ExtraAbil" src/M2Server/Magic/MagicManager.cs` ⇒ **0 命中**；`MagMakeSlave`（`:1308-1335`）只读
`CheckServerMakeSlave()`、`Abil.Level`、`SystemShare.Config.Skeleton*`，随后 `MakeSlave(...)`。
⇒ 17 的判定链**不存在**"把 `ExtraAbil` 折进 `WAbil` 当效果"的假设。负结论登记，省后来者复查。

## 三、slave/clone 记录面装置（覆盖 MG-24/41/42/55）契约（交 a2 落 `Program.cs`）

**oracle 侧可读性（已查，`165c6edd1`）**：
- `MakeSlave` 返回 `IActor`：`src/M2Server/Actor/BaseObject.cs:1179`（**非空 ⇒ boTrain**）；
- `SlaveList` 为 **public** `IList<IMonsterActor>`：同文件 `:141`；`MakeSlave` 内部 `:1181-1183` 惰性初始化。
⇒ **C# 探针能读到"召唤出了什么"**（返回对象 + `SlaveList`），无需扩展探针协议；只需把下列字段写进 golden。

**要记的字段（每条 case）**
1. `slave_return_non_null: bool`（`MakeSlave` 返回值）；
2. `slave_list[]`：每项 `{name, actor_id, level, master}`（名字/等级取实体属性）；
3. `slave_count`（= `slave_list` 长度）；
4. 相关帧（该次施法后 caster/目标/新实体的 SM 序列，与既有 Receivers 同形，新实体用 `slave{i}` 键）。

**分条计数（§28，机检形式）**：
- golden 顶层必须带 `oracle_source_sha256`（生成器源码 sha）+ `driver_case`（本次跑的 `--case` 名）；
- `slave_list` 的**每一项**带 `case` 字段（= 该 case 的 Label）；
- 比较器 `slave_case_verify(stem, label)` **只取 `slave_list[?].case == label` 的项**，并按 `oracle_source_sha256`
  与当前树比对；`case` 不匹配或 sha 不符 ⇒ 红。
- 四条（17/41/42/55）各自的证据 = 自己 case 的那一份（文件 + case + 驱动名三元组），
  **同一份 `slave_list` 不能被两条引用**——比较器按 `case` 过滤后天然不成立。

**不做什么**：不跑 Operate、不改 v2 卡、不动 `Program.cs`（a2 独占，本卡只出契约）。

## 四、slave 记录面的 API 选型（两个候选 + 代价，2026-11-20）

**背景事实**：Rust 侧**已有** `SlaveState.slave_list: Vec<i32>`（`crates/world-bevy/src/entity.rs` 的 `SlaveState.slave_list` 字段（符号锚）），
`EntityRef` 可直接读（`e.slave_list`）；并且有 `slave_list_cleanup_impl`（`crates/world-bevy/src/world.rs`
的 `SimSlaveCleanup` 段，含"按槽位序 + 表内倒序删除"与 `!e.ghost` 过滤）在同 tick 维护它——语义与
oracle 的 `SlaveList`（`BaseObject.cs:141`，`MakeSlave` 内追加、死亡/叛变时清理）**同源**。

| 候选 | 做法 | 代价 / 风险 |
|---|---|---|
| **A. 读既有 `slave_list` 镜像** | 施法后读 `entities.get(caster).slave_list`（id 列表）→ 再逐个 `get` 取 name/level/master | **顺序语义天然与 oracle 一致**（两次都在"追加序 + 清理后"上读）；风险：若将来有人不更新该镜像（新增召回路可忘写），读到的是**陈旧镜像**——但清理/追加路径已有测试钉住 |
| **B. 遍历 `entities` 全表按 `master == Some(caster)` 过滤** | 全表扫描，收集 `master==caster` 的实体 | 无镜像陈旧风险；**但拿不到 oracle 的追加序**（全表序不是 SlaveList 序），且与 oracle 的"清理时机"（死亡/幽灵剔除）不同步——号数可能多出刚死未清理的实体 |

**选型：A 为主、B 作交叉校验（牙）**。
- 证据面用 A（与 oracle `SlaveList` 同源、同序）；
- 每次用例额外断言 `A 的 id 集合 == B 的集合`——**B 不当证据，只当"镜像是否漏更新"的牙**；两者不等即红，
  报"镜像陈旧或清理时机不同步"。这样既保住顺序语义，又不让镜像静默失真。

**不选 A-alone 的理由**：镜像一旦漏更新，用例只会读到少/旧的 id，且**不会有任何红**（这正是"夹具自洽"）。
**不选 B-alone 的理由**：顺序与清理时机都偏离 oracle，等于用一个更粗的代理替换真实语义。
