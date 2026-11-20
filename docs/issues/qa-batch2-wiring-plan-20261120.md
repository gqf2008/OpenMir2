# 接线计划（只读，不动代码）：MG-58/60/61/62/63 + MG-57 装置前提

> 立稿树 `0b42eb25c`（∈master 历史）；落库基线 `1104c26f6`。本文件只给计划与判据，未改任何代码、未起构建。

## 一、5 条共享尾叶子（49/51/52/53/54）——零装置改动批次

**统一形态**（oracle `MagicManager.cs`）：case 体只有 `boTrain = true;`（`:607/:631/:634/:637/:640`），
帧面全部来自 switch 级外层门：`:159 RM_SPELL` 广播（**自守卫** `PlayObject.Message.cs:1455` 的 `if (processMsg.ActorId != ActorId)` ⇒ **施法者本人不收 `SM_SPELL(17)`**；既有 golden `fire` 的 caster 帧 `[638,31,31,656,640]` 可印证）→ `:648 RM_MAGICFIRE`（`boSpellFire` 缺省 true）→
**`:659`** `if (Level < 3 && boTrain)` → **`:661`** `if (TrainLevel[Level] <= Abil.Level)` →
**`:663`** `TrainSkill(userMagic, Random(3)+1)` → **`:666`** `SendSelfDelayMsg(RM_MAGIC_LVEXP,…,1000)`；
升级支在 `PlayObject.cs:5216-5217`（`TranPoint -= MaxTrain[nLevel]; Level++; UpdateDelayMsg(…,800)`）。

| 条目 | 魔号 | 驱动（3 支/条） | 比较器 | 断言面（逐收件人，按序） |
| --- | --- | --- | --- | --- |
| MG-58 | 49 | `skill49_parity_s1/_s5/_s13` | `real_rust_ledger_skill49` | caster `[638]`（**无 17**）+1000ms 后 `640`；target/obs_in `[17, 638]`；obs_out `[]`；**负向断言 caster 恰 0 条 `17`** |
| MG-60 | 51 | `skill51_parity_s1/_s5/_s13` | `real_rust_ledger_skill51` | 同上 |
| MG-61 | 52 | `skill52_parity_s1/_s5/_s13` | `real_rust_ledger_skill52` | 同上 |
| MG-62 | 53 | `skill53_parity_s1/_s5/_s13` | `real_rust_ledger_skill53` | 同上 |
| MG-63 | 54 | `skill54_parity_s1/_s5/_s13` | `real_rust_ledger_skill54` | 同上 |

**夹具设计要点（每条写清"对哪条断言起作用"——下一批照抄也懂为什么）**
- **`UserMagicLevel < 3`** —— 对 **`:659` 训练门**（`Level<3 && boTrain && TrainLevel[Level]<=Abil.Level`）。
  不满足 ⇒ 训练尾整段不执行 ⇒ **没有 640、也没有那 1 抽** ⇒ `boTrain` 这一面与"普通施法"不可区分
  （只剩 638 与 target/obs_in 的 17）⇒ 牙 B/C 会失去判别对象。**这是本批的主判别前提。**
  `UserMagicLevel = 3`（或更大）与之等价：**既无 `Random(3)` 也无 640** ⇒ `Level < 3` 同时是**抽数门与帧门**，夹具必须显式写 `UserMagicLevel ∈ {0,1,2}`。
- **`max_train[level]` 取大** —— 对 **640 的时延支与载荷**：`CheckMagicLevelUp`（`PlayObject.cs:5198`）只有在
  `TrainLv > Level && MaxTrain[nLevel] <= TranPoint` 时才升（`:5216-5217`，800ms）；取大 ⇒ 不满足 ⇒ 走 `:666` 的 **1000ms** 支。
  **断言写法（两处写死，不留"大概"）**：
  ① 时延 = **1000ms**；载荷 `TranPoint = 初始TranPoint + (1..3)`（`:663` 是**累加**）——**夹具把初始 `TranPoint` 显式写成 0**，断言才可钉 `1..=3`；
  ② `Level` **与本帧前相等**（不是"绝对等于 0"）；**该断言的有效性依赖夹具前提 `MaxTrain > TranPoint`**，
  夹具里要**显式断言这个前提本身**（如 `assert!(max_train[0] > tran_point_after)`），否则将来有人调小 `max_train`，
  这条断言会**静默变成永真**。
  若同时想要升级支，**另加一例**（800ms），不要在同一例里混。
  **本批 scope 只做 1000ms 支**（`MaxTrain` 取大）：`MaxTrain` 取小会落 `CheckMagicLevelUp` 的 **800ms** 同 ident 变体，**属另一分支、不在本批**，需要时另立一例（并写进该例 scope）。
  **前提被改动时"谁先红"（每处断言各注一行）**：
  - `max_train` 调小（`≤ TranPoint`）⇒ `CheckMagicLevelUp` 成功 ⇒ 640 走 **800ms** 且载荷变 `(Level+1, TranPoint-MaxTrain)`
    ⇒ **断言①（时延 1000ms）先红**；同时夹具最前面的前提 `assert!(max_train[0] > tran_point_after)` **也会红**（前提断言在驱动最前，先于帧断言执行）。
  - 初始 `TranPoint` 非 0 ⇒ 载荷值变 `初始+(1..3)` ⇒ **断言①的"载荷 = 1..=3"部分先红**；断言②（Level 不变）仍绿 —— 两条断言能区分"哪类前提被破坏"。
  - `UserMagicLevel` 改成 ≥3 或 `TrainLevel[Level] > Abil.Level` ⇒ 训练块整段不执行 ⇒ **"640 出现"断言先红**，且 `Rng` 长度断言（1 vs 0）同时红。
  - 排水窗口 < 1000ms ⇒ 640 进 `NotDue` ⇒ **"640 出现"断言先红**（属假红，见固定项之六）。
- **跨 1000ms 排水** —— 对 **"640 出现"这条断言本身**：640 是自投递，未到点会进 `NotDue`，
  此时"没有 640"是**假红**（不是行为错）。`WaitMs ≥ 1000`（或 `StepDrainMs` 使排水跨过）。
- **RNG 预期**：每击恰 **1 抽** `Random(3)+1`（`:663`，仅训练门内）；台账 `Rng` 长度 = 1，用来与"没抽"区分。
  golden 生成后**逐支断言 `Rng.len() == 1`**；若出现别的抽数（>1）属**发现**（说明该叶子的效果体另有取数），不是预期，需单列。
- **收件人**：caster / target / obs_in / obs_out（与既有 BehaviorBatch 同形）；640 **只给 caster**。
- 版本门 `:167`：探针已设 `SoftVersionDateEx=20240101 / ClientTick=5` ⇒ 通过；无需改。

**牙（每条 stem 各一套；每刀预期红该 stem 的 3 支驱动 + 1 比较器）**

| 牙 | 变异点（Rust） | 预期红在哪 |
| --- | --- | --- |
| **A 去 638** | 让该魔号的共享尾不发 `RM_MAGICFIRE`（`do_spell_tail` 的 `magic_fire` 置 false 的等价） | 3 支的**帧序断言**缺 638（全部收件人）；Rng 不变 |
| **B `boTrain=false`** | `spell_tail_bo_train` 对该 id 返回 `Some(false)` | 3 支：caster 缺 640 **且** `Rng` 少 1 抽（两条独立断言都动） |
| **C 去 640** | tail 里跳过训练帧（保留抽数） | 3 支：caster 帧序缺 640，但 `Rng` 仍为 1 ⇒ 专测"抽了没发" |
| **D caster 自守卫** | 让 `SM_SPELL(17)` 也发给施法者（`broadcast_ref_msg` → `broadcast_ref_msg_with_self` 等价改动） | 3 支的**负向断言「caster 恰 0 条 17」**红（caster 前缀断言 `[638]` 也会多出 17） |

> **红集合必须点名，不许只给计数**（`gate-green-must-name-the-set` 的反向：红也要点名）。以 MG-58（49）为例：
> - 牙 A / B / C / D 的红集合均为 `{ skill49_parity_s1, skill49_parity_s5, skill49_parity_s13, real_rust_ledger_skill49 }`（4 个名字全写）；
> - 其余四条把 `skill49` 换成 `skill51` / `skill52` / `skill53` / `skill54`，同样逐条点名；
> - 牙 B 额外动 `Rng` 账本（少 1 抽），但仍应在**同一支**的 `Rng` 断言上红——两条断言同支，不另算支数。
> **追记（`opc-19` 追问）：牙 B 少的那 1 抽原本是干什么的？**
> 是 **训练熟练点抽** `TrainSkill(userMagic, Random(3)+1)`（`src/M2Server/Magic/MagicManager.cs:663`；门在 `:659/:661`）——**不是伤害 roll**（这几支没有伤害）。
> 它只在 `if (Level<3 && boTrain && TrainLevel[Level] <= Abil.Level)` 块内发生 ⇒ `boTrain=false` 时**必然少这 1 抽**，
> 且比较器**逐值比 `Rng` 数组**、长度不等即报错 ⇒ **"少 1 抽"是可观测的账本差**，不是纯副作用。
> **但主判据仍是"640 消失"**（训练帧），`Rng` 差列为**第二条独立观测**；两者同支，不重复计支数。
> **夹具前提**：必须让 `TrainLevel[Level] <= Abil.Level` 成立，否则正常例都不抽，"少 1 抽"就无从谈起。
>
> 若要求"一牙只红一支"，只能在某个 label 的期望里做差分（不建议——会削弱"每支都独立成立"）。

## 二、MG-57（37）装置前提：视野回填 + 可走地图 —— **需独立批次（不得混入其它批）**

> **为什么独立**：最小回填要调用 `BaseObject.UpdateVisibleGay`（或探针侧等价物）——这是**广播路径的公共函数**
> （`BaseObject.ViewRange.cs:12-46`，调用点 `BaseObject.ViewRange.cs:114` / `PlayObject.cs:3131`），不是这条叶子私有的。
> 动它会影响所有依赖 `VisibleActors` 的路径 ⇒ 必须**单独一批、单独回归**：全 workspace 测试 + §57 扩后的
> 22 条比较器门（+ 若动到探针则另加装置自测）。

**缺口**：oracle `MagPushArround`（`src/M2Server/Magic/MagicManager.cs:24-52`）遍历 `playObject.VisibleActors`；
该表由 **`BaseObject.UpdateVisibleGay(IActor)`**（`BaseObject.ViewRange.cs:12-46`）填充，
而其调用点在**广播路径**（`BaseObject.ViewRange.cs:114`、`PlayObject.cs:3131`）。探针手工建对象、不跑服务器主循环 ⇒ 表为空。

**最小回填序列（探测顺序，从最便宜开始）**
1. 每个候选目标对 caster 调一次 `caster.UpdateVisibleGay(target)`（players/pets 才会入表）⇒ 直接验证"表非空"；
2. 若 `MagPushArround` 有推挤，`CharPushed` 需要**可走格**：探针已 `LoadMapData("0.map")` + `env.AddMapObject(...)`，
   但需确认 `CanWalk`/门/障碍在替身路径下可用（若走真 `Envirnomap` 接口应可用）；
3. 帧面预期：`SM_BACKSTEP(9)` **逐格**一条（含受推者自己）、`RM_PUSH` 逐格、共享尾 638/640；
4. 若 1 有效但 2 不通，判"需 WorldEngine 替身扩展"，与 17 的离线替身同源处理。

**牙（37，装置修好后）**：等级门（`Abil.Level > target.Abil.Level`）/ `Random(20)` 阈值 /
`Random(2)` 格数 / `IsProperTarget` 门——每一刀都能让 `SM_BACKSTEP` 的**条数或顺序**分叉。

## 三、依赖与顺序
- 5 条的 golden 需 `c6` 批量出（recog schema 完成后），**同一形态一次生成**；
- `MG-57` 的装置改动与 `MG-64/65`（护身符 Shape 旋钮）互不依赖，可并行立卡。

## 四、本轮修订（2026-11-20，落库 `1104c26f6` 前）

1. **caster 收件面更正**：由 `[17, 638]` 更正为 **`[638]` + 640**——`RM_SPELL` 处理器带自守卫
   （`PlayObject.Message.cs:1455` `if (processMsg.ActorId != ActorId)`），施法者不收自己的 `SM_SPELL(17)`；
   既有 golden `fire` 的 caster 帧 `[638,31,31,656,640]` 印证。并新增**负向牙 D**「caster 恰 0 条 17」把这条自守卫钉住。
2. **640 的 1000ms 支是唯一 scope**：`MaxTrain` 取小会走 `CheckMagicLevelUp` 的 **800ms** 同 ident 变体，
   属另一分支（另立一例、另写 scope），本批不做。
3. **`UserMagicLevel < 3` 是抽数门与帧门**：`=3`（或更大）时训练尾整段不跑——**既无 `Random(3)` 也无 640**。
4. **恰 1 抽**：每支施法只应有 `:663` 的 `Random(3)+1` 一次；golden 生成后逐支断言 `Rng.len() == 1`。
