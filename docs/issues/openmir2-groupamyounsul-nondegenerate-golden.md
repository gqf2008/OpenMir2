# MG-50 `groupamyounsul(48)` 需要一份**非退化** golden（2026-11-20）

> 触发：`card/magic-mg23-49-50`（`9ec253049`）接线时发现，48 的现行 golden **完全没覆盖效果体**。
> 本卡只登记"为什么需要新 case、需要什么形态、怎么验收"；**不在本卡造 oracle 数据**。
> 现行处置：48 保持 `implemented_unverified`（接线已合，`notes` 记"golden 退化、效果体未覆盖"）。

## 1. 现象：golden 是退化场景

`golden/magic/groupamyounsul.json`（`s1/s5/s13` 三支）的真实形态：

- `Fixture.rng_total_draws = 0`（施法与交付**一抽都没有**）；
- 两侧都没有 `RM_POISON`；`TargetHp` 前后都是 `100`；
- 帧面只有 `17`（`SM_SPELL`）+ `638`（`SM_MAGICFIRE`），即**只剩施法本身**。

⇒ `cast_group_poison_spell` 的逐目标循环**一次都没执行**。**根因已钉（2026-11-20 QA 复核）**：
现行探针的护身符写死 `StdMode=25, **Shape=5**`，而 48 的逐目标门是 `src/M2Server/Magic/MagicManager.cs:908` 的
`CheckAmulet(playObject, 1, **nType=2**)`——`src/M2Server/Magic/MagicBase.cs:65-70` 的 nType=2 支要求 **`Shape <= 2`** ⇒
**每个目标都在护身符门处被跳过，根本走不到 `Random(AntiPoison+7)`**（这正是 `Rng=0` 的直接原因）。
（"目标是否在 `mag_objects_in_range` 内"仍未被单独证明；造新夹具时**两件事都要满足**：目标在范围内 + 护身符 `Shape≤2`。）
48 的"群体施毒"声明行为在本 golden 下**完全不可观测**。

## 2. 四刀变异原文（最强证据：全绿 ⇒ 循环零次）

对 `base 3362758fa` 逐刀施加、每刀单跑 `tests::groupamyounsul_parity_s1`，**四刀全部 `1 passed`**：

| # | 变异 | 改动点 |
| --- | --- | --- |
| ① | 毒伤表达式 | `power_poison`：`i32::from(get_rpow(rng, sc)) * 2` → `* 3` |
| ② | 目标范围 | `cast_group_poison_spell` 内 `let range = i32::from(level).max(1)` → `+ 5` |
| ③ | 合法目标门 | 移除 `if !crate::combat::is_proper_target_grouped(...) { continue; }` |
| ④ | 毒素骰 | `if !crate::magic_effect::poison_dice(&mut self.rng, anti_poison)` → `if false && ...` |

**判读**：④ 强制骰子通过都无变化 ⇒ 不是骰子/门/表达式的问题，是**循环体没进入**。
（各刀还原均核 `sha256sum` 与快照一致、`MUTATION` 标记为 0。）

## 3. 新 case 需要的夹具形态

- **一个敌对目标真的落在 `mag_objects_in_range(caster.map, tx, ty, level.max(1))` 内**
  （现行夹具里 caster/target/obs_in 都不进该集合 —— 这是根因）；
- 施法者有可用护身符，且 **`CheckAmulet(playObject, 1, nType=2)` 通过**（`src/M2Server/Magic/MagicManager.cs:908`；
  `src/M2Server/Magic/MagicBase.cs:65-70`：nType=2 要求 **`Shape <= 2`**）⇒ `use_amulet` 才会返回 `Some(Shape)`。
  **现行探针 `StdMode=25, Shape=5` 过不了这道门**；`nType=1`（`Shape == 5`）是 `:323` 那条、属魔号 14/15（`MG-28/29`），与 48 无关。
- 触发的 `Shape` 至少覆盖 **1（绿毒，`n_int=40`）**；若 oracle 场景允许，**2（红毒，30）** 另起一支；
- 期望面必须包含：`RM_POISON`（`type/nPower/caster/point` 四字段）、`Rng` 非空、
  目标 `HP` 或毒状态的可观测变化。

## 4. 验收条件（本卡的 DoD）

1. 新 case 落盘后，**§2 的四刀变异至少一刀变红**——这是"效果体真的跑到了"的**必要条件**，
   也是本卡的**唯一硬判据**（不许用"帧数多了"代替）；
2. 新 case 的 `rng_total_draws > 0`，且台账与 golden 都出现 `RM_POISON`；
3. 48 的驱动/比较器（`groupamyounsul_parity_s*` / `real_rust_ledger_groupamyounsul`）把新 case
   一并纳入（`b1_golden_drain_assert` 的 `DrainPasses` 按新 golden 更新）；
4. 只有 1–3 全过，才允许把 MG-50 从 `implemented_unverified` 升格 —— 且 manifest `notes`
   里"golden 退化"那条同步改写为"已由非退化 case 覆盖（case 名）"。

## 5. 与其它卡的关系

- `card/magic-mg23-49-50`（`9ec253049`）：接线已完成并合入；本卡**不改**它，只补 oracle 数据。
- 不新造 oracle 数据的边界属于那张卡；**本卡就是解除它的那张**——需要跑 C# 侧重新采集。

## 6. 施毒 6 条批（MG-13/14/15/50/64/65）WIP 交接（2026-11-21，`card/poison-batch`）

> **2026-11-21 收口**：本节“未做 1–5”已全部完成，见 §7。标记 `poison_cross_actor_same_tick`
> 按总监裁定**复用** `multi_victim_same_tick`（同根，登记见 `openmir2-b4-multivictim-strike-order.md`
> 「施毒 4 支同源」）；下文旧标记名仅作历史。

**基线** `b480336f9`。**收敛记录：比较差异 72 → 49 → 每 case 恰 1 条（仅 `SM_SYSMESSAGE(100)` 载荷）**。

### 已就绪（工作树）
- 装置三旋钮（`Program.cs`）：`AmuletShape`（护身符 StdItem）、`TargetAntiPoison`（登录序 recalc 后写）、
  **显式装载 `ServerConf`**（`[Setup] AmyOunsulPoint=15` 真正进 Config；`<=0` 即 `FixtureContractViolation`）；
  `Fixture` 记 `amulet_shape`/`target_anti_poison`/`amy_ounsul_point`。
- 4 份**非退化** golden：`amyounsul`/`amyounsul2`（6 Shape1/2）、`groupamyounsul`/`groupamyounsul2`（48 Shape1/2）：
  `rng_total_draws=3–4`、目标 `Rm` 含 `RM_POISON(8037)`、带 `frame_order_scope=poison_cross_actor_same_tick`、
  `StepDrainMs=100`、`t0` 取在 `DoSpell` 之前。
- 比较器（`magic_fireball_parity.rs`）：毒口径分支——顺序多重集 + chained 帧 due **批次类（0 vs >0）** +
  **late 帧（640）`|Δ|≤32ms` 容差**（15.6ms TickCount 粒度 ×2，注释含推导与 600/1000 判别力说明）；
  marker **白名单写死** `{fire, amyounsul, amyounsul2, groupamyounsul, groupamyounsul2}`。
- 驱动：`SpellFrameRig.amulet_shape`；三 Fixture 键读取；`RM_POISON` **交付前**抓取；
  `PoisonDelivery` → `8090`/`10139`/`11004` 装配；6 个新驱动 + 3 个比较器。

### 未做（按依赖，接手清单）
1. **① 装置补三键**：`Fixture` 增记 `red_msg_f_color`/`red_msg_b_color`/`you_poisoned`（同一 `SystemShare.Config`
   / String.conf 读；**缺键 ⇒ `FixtureContractViolation`，不许缺省**），驱动把 `drain_b4_self_deliveries(...,0,0,b"")`
   换成它们（文本经 `enter::gbk`）。牙：改 `you_poisoned` 一个字符 ⇒ `tail` 必红。
2. **② 驱动口径统一**：`group_amyounsul_assert` 里的 `per_receiver_frame_seq_matches_golden`（严格序）换成
   「多重集 + chained 批次类 + 640 容差」，与比较器同口径；改后三支须与比较器同绿。
3. **③ 牙**：L1 三颗（golden 640 due 1000→600 必红、→0 必红、→1016 必绿，且**只**对 late 帧）；
   `10139` 红毒 `0x40000000` 牙（绿毒 `int.MinValue` 只从 golden 比）；`657.series=HitSpeed` 比对。
4. **④ 四刀**（本卡 §2）：毒伤表达式 / 目标范围 / 合法目标门 / 毒素骰——至少一刀在比较器上红。
5. **⑤ 门禁**：fmt / clippy `--all-targets` / `--lib` 三数 / msgcodegen / coverage / manifests / 锚门；
   红原文进提交信息；`3b`/`10` 复核后由 Leader 合并。
6. 其余：`c6` 的 head（`49edaf0fd`）与 `magic_effect.rs`、`lib.rs` 魔号 10 区**不可碰**；别删远端分支。

### 已知边界
- 毒交付帧「跨 actor 同拍」与 `multi_victim_same_tick` **同根**（oracle 逐 actor `Run()` vs Rust 批量 flush）；
  撤销条件同：产品改为逐 actor 处理时删 marker。
- `Rm` 的 `10139.p1` 是真实位掩码（绿毒 `0x80000000>>0` = `int.MinValue`；红毒 `0x40000000`），
  **逐值从 golden 比，不写死常量**。

## 7. 施毒 6 条批收口记录（2026-11-21，`openmir2-7d`）

**交付分支**：`card/poison-batch`（base master `b480336f9`；WIP 起点 `19aa909f9`）。

### 7.1 完成项（对应 §6“未做 1–5”）
1. **① 三键 fail-closed**：装置 `Fixture` 记 `red_msg_f_color`/`red_msg_b_color`/`you_poisoned`
   （部署 `Server.conf [Setup]` + `String.conf [String]`，显式 `StringConf.LoadString()`）；
   驱动缺键 ⇒ `FixtureContractViolation`，文本经 `enter::gbk` 进 `drain_b4_self_deliveries`；
   `RM_SYSMESSAGE(11004)` 的 `param` 也改取 golden 色值（消灭硬编码 `255/56`）。
2. **② 驱动口径**：`groupamyounsul` 的严格序 → 毒口径（多重集 + chained due 批次类 + 640 |Δ|≤32ms），
   与比较器同口径；非删除。
3. **③ 判据与牙（按 charter §12 分档，别混列）**：
   - **产品行为牙**（同输入两套实现/变异，输出或状态变）：`SM_SYSMESSAGE(100).tail` 改一字符
     必红 —— `tooth_poison_sysmessage_tail_compared_red`。
   - **比较器级牙**（合成或改 golden 输入 ⇒ 比较器红，证"该字段在判"，**不是产品行为证据**）：
     L1 三态（640 due 1000→600/950/0 红、→1016 绿）+ **chained 批次类**（`657` due 1000→0 红）
     —— `tooth_poison_late_due_tolerance`；`10139.p1` 逐值比（期望从 golden 读）——
     `tooth_poison_10139_status_p1_compared_red`。
   - **断言（不是牙）**：`657.series == HitSpeed(7)` —— golden 输出 vs 夹具输入的一致性断言；
     其负例是**比较器级**的 `tooth_poison_657_series_compared_red`（改 0 必红）。
4. **④ 四刀（产品行为牙）**：毒伤表达式 `*2→*3` 一刀**真红**（`8037` p1 97→112 进帧面）；
   还原后全量重跑绿。
5. **⑧ 抓取时点牙（产品行为牙）**：把 `RM_POISON(8037)` 的抓取整体后移到排水之后 ⇒ 比较器必红
   `golden 有 rust 无=[[8037, 0, 97, 1001, 2, 255, 1]] rust 有 golden 无=[]`——证明
   “交付前抓”不是注释（还原后 `hash-object` 相等、`MUTATION` 计数 0）。
6. **⑨ 静态守卫（接线提示，非行为证据，charter §12）**：marker 白名单负例（非白名单 stem 带
   `multi_victim_same_tick` ⇒ 比较器入口 panic，`tooth_poison_marker_whitelist_rejects_foreign_stem`）；
   **`stem ↔ magic_id` 静态一致性**（fire=9 / skill10=10 / amyounsul*=6 / groupamyounsul*=48），
   防止"给 fire 加旋钮就被静默降级成批次类"。二者都是 `grep`/`matches!` 级检查。
7. **⑤ 门禁**：见 §7.4。

### 7.2 三旋钮 fail-closed（评审 A）
`amulet_shape` 必须 1|2（不写死 5）；`target_anti_poison` **0 也显式写入**（不再 `if v>0` 跳过）；
`amy_ounsul_point` 必须 >0（不再 `unwrap_or(10)`）。装置侧 `AmyOunsulPoint` 断言部署值 `==15`。
静态机判 `poison_goldens_carry_three_knobs`（读 JSON 键；**接线提示，非行为证据**，charter §12）：
4 文件 × 3 支 = **12 支**逐支三键存在 + 值域。

### 7.3 §4 四条（给 QA 升格用，本批不改 manifest）
1. **§2 四刀至少一刀红**：✔（毒伤表达式，红原文见提交 2）。
2. **`rng_total_draws > 0` 且台账与 golden 都有 `RM_POISON(8037)`**：✔
   `amyounsul` s1/s5/s13 = **3/4/4**；`amyounsul2` = **3/4/4**；`groupamyounsul` = **3/3/3**；
   `groupamyounsul2` = **3/3/3**。两侧 RM 均含 `8037`（type 0/1、nPower、caster、point）。
3. **驱动/比较器纳入新 case；`DrainPasses` 按新 golden 更新**：✔ 4 支 stem × 3 label 全部纳入；
   `DrainPasses` 2→3 是**非退化后实测真变**（`RM_POISON` 投递后派生即时帧需再排一趟），
   旧值 2 属于退化 golden（循环体从未执行）。
4. **manifest `notes` 改写**：**留给 QA `3b`**（本批不动 `docs/issues/parity-gap-magic.json`）。
   **建议升格**：MG-13 / MG-14 / MG-15 / MG-50 / MG-64 / MG-65（六条，见 §7.5 映射）。

### 7.4 证据（可复现）
- 驱动：`cargo test -p game-svr --lib -- --exact --ignored tests::<driver>`，driver ∈
  `{amyounsul,amyounsul2,groupamyounsul,groupamyounsul2}_parity_{s1,s5,s13}`（**12 支**）；
- 比较器：`cargo test -p mir2-parity-tests --test magic_fireball_parity -- --exact --ignored
  real_rust_ledger_{amyounsul,amyounsul2,groupamyounsul,groupamyounsul2}`；
- **同一 `MIR2_LEDGER_RUN_ID` 覆盖全部 3 个 label**（只跑 s1 会让 s5/s13 撞 `run_id 不匹配`——
  这是唯一的假红入口，已在本批实测踩到并规避）；
- 牙（非 ignore）：`cargo test -p mir2-parity-tests --test magic_fireball_parity tooth_poison_`
  与 `cargo test -p game-svr --lib -- --exact tests::poison_goldens_carry_three_knobs`。
- 门禁：fmt / clippy（`-p game-svr -p mir2-parity-tests --all-targets -D warnings`）/
  `cargo test --workspace --lib` 三数 / msgcodegen `--check` / 锚门 / 清单 schema / 分派回归
  （原文见最终 head 的提交信息与交接报告）。
  **回归（合并后树，最终）**：18 驱动（施毒 12 + fire 3 + skill10 3）+ 6 比较器全绿。
  **证据分档（charter §12）**：产品行为牙 3 条（④ 四刀、⑧ 抓取时点、`100.tail` 单字符）；
  比较器级牙 2 组（L1 三态+chained 批次类、`10139.p1`）；断言 1 条（`657.series==HitSpeed`）；
  静态守卫 3 条（marker 白名单、`stem↔magic_id`、12 支三键机判）——后者只作接线提示。

### 7.5 stem ↔ 魔号/叶子映射（QA 升格按此）
| stem | 魔号 | 覆盖叶子 |
| --- | --- | --- |
| `amyounsul`（Shape=1） | 6 | **MG-13**（`CheckAmulet(1,2)` → 骰 → 投递主链）+ **MG-14**（绿毒 `DECHEALTH` 臂） |
| `amyounsul2`（Shape=2） | 6 | **MG-15**（红毒 `DAMAGEARMOR` 臂） |
| `groupamyounsul`（Shape=1） | 48 | **MG-50**（逐目标循环主链）+ **MG-64**（绿毒臂） |
| `groupamyounsul2`（Shape=2） | 48 | **MG-65**（红毒臂） |

### 7.6 已知口径（防后来人误判）
- `10139.p1` 本夹具绿/红毒**同为 `int.MinValue`**：位号 = `ActorBuffSystem` 的 **buff 插入序**
  （`0x80000000 >> i`），目标只有一条毒 buff ⇒ i=0。`BaseObject.Base.cs:170` 按 `PoisonState`
  取位（DAMAGEARMOR=1 ⇒ `0x40000000`）是**另一路径**，本 fixture 不经过；区分性夹具另立卡
  （需目标先有一条 buff）。
- 顺序放宽标记复用 `multi_victim_same_tick`（同根），白名单 = `fire` + 施毒 4 支；
  移除条件见 `openmir2-b4-multivictim-strike-order.md`「施毒 4 支同源」。
### 7.7 follow-up 待办（2026-11-21，合并后登记）
待办：驱动侧 `poison_stem`（三旋钮判定）与 `fireball_case_assert`（640 due 容差）两处 stem 名单，
应合并为 `fn is_poison(golden) -> bool`（`Fixture.magic_id ∈ {6,48}`，与比较器同源）；
失效方向为假红/漏检。并入下一个改驱动/比较器的批次，不单独开批。
**QA 复核追加（P3，2026-11-21）**：驱动侧 fail-closed 的「施毒族判定」目前按 **stem 名单**
`poison_stem = matches!(stem, …)` 开分支，而读侧 `else` 仍保留旧可选路径（`golden_fixture_opt` +
`if v > 0` / `unwrap_or(10)`）。今天四支都被名单命中、**无实害**；但**未来新增施毒 stem 未进名单**
会**静默**落旧可选路径（`amy_ounsul_point` 10 vs 15 这类错配可静默变绿）。修法同上条：按
`Fixture.magic_id ∈ {6,48}` 判定，或「三键任一存在 ⇒ 三键全必填」。**与上条合并为一个 follow-up**，
不单独开卡（`openmir2-10` 在 `0e8988dd8` 复核时提出）。
