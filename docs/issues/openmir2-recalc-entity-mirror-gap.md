# 重算后缺实体镜像（recalc → entity mirror gap）

> 记录点：`master = 977d260a1`。本卡只登记"扫同族"结果与验收口径，**不含实现**；
> **不与 `card/657-series-spellbuf` 捆在一起合并**（那条是 spell 路径的 series 修复）。

## 1. 不变量（oracle）

oracle 里玩家对象只有**一份**：`RecalcAbilitys()` 尾部 `HitSpeed = AddAbil.HitSpeed`（`PlayObject.cs:2899`）
写回对象本身；657 在**出队时**读 `this.HitSpeed`（`PlayObject.Message.cs:1845-1847`）。
⇒ Rust 侧任何"重算之后可能产出 657"的路径，都必须把会话值 `sg.hit_speed` **镜像回实体**
（生产等价物 = `apply_ability_to_entity(...)`）。缺这一步 ⇒ 657 的 `Series` 读到重算前的旧值。

## 2. 扫描表（`recalc_session_ability` 16 个生产调用点，`master 977d260a1`）

判据：所在函数内是否存在 `apply_ability_to_entity` 或直接 `vitals.hit_speed =` 镜像。

| # | 调用点 | 函数 | 镜像 | 发 657 | 657 `Series` 读哪 | 判定 |
| --- | --- | --- | --- | --- | --- | --- |
| 1 | `apply_pending_level_up` | 升级 | ❌ | 是 | 自见走 `sg`（`recalc_session_ability` 内赋值）；观察者读各自实体 | **不受 657 值影响（已核）**；仅"实体未镜像"不变量缺口 |
| 2 | `apply_gm_use_item_write` | GM 用药 | ❌ | 否 | — | 登记（无 657，不变量缺口） |
| 3 | `handle_inbound_cmd` | 入图 | ✅ | 是 | 实体（已镜像） | 无损 |
| 4 | `handle_bag_cmd_with_magic` | 穿脱 | ✅ | 是 | 实体 | 无损 |
| 5 | `handle_shop_cmd_inner` | 商店升级 | ✅ | 是 | 实体 | 无损 |
| 6 | `handle_eat_cmd`（第一处） | 吃物品 | ❌ | 是 | **自我走 `WireSession(sg)`；观察者走实体** | **同族风险（观察者面，"半对"）** |
| 7 | `handle_eat_cmd`（第二处） | 吃物品 | ❌ | 是 | 同上 | 同上 |
| 8 | `handle_spell_cmd_with_magic` | 施法（36） | ✅ | 是 | 实体 | 无损 |
| 9 | `handle_spell_cmd_with_magic` | 施法（shield/buff） | ❌（**§2bis 校正**：`card/657-series-spellbuf` 只改来源、未加镜像；旧版此处误记 ✅） | 是 | 实体（可能陈旧） | **同族风险（镜像缺口）** |
| 10 | `drain_buff_downlink` | buff 排水 | ❌ | 是 | 实体（自见+观察者都经 helper） | **同族风险（`#9` 路径）** |
| 11 | `drain_levelup_healthspell_with_magic` | 升级回血 | ✅ | 是 | 实体 | 无损 |
| 12 | `apply_pending_gm_target_ops`（臂 1） | GM 删技能 | ✅ | 是 | 实体（会话优先？按实体裁定应为实体） | 无损 |
| 13 | `apply_pending_gm_target_ops`（臂 2） | GM 删技能 | ✅ | 是 | 实体 | 无损 |
| 14 | `drain_player_need_recheck_with_magic` | P21 装备复检 | ✅ | 是 | 实体 | 无损 |
| 15 | `drain_player_hunger` | 饥饿跨档 | ✅ | 是 | 实体 | 无损 |
| 16 | `drain_player_revival_with_magic` | 复活戒指 | ✅ | 是 | 实体 | 无损 |

⇒ **10 有 / 6 无**；其中 **3 个函数（4 个调用点）有 657 且读实体**，是真正的同族风险面。

## 2bis 全部 657 发射点与其 `Series` 源（底图）

> 采集：`master 24d2de316`（`card/657-series-spellbuf` 已并入 `ce86a026f` 之后）。
> 取点方式见下，**行号只作引用、不作散文锚**（§49/HIGH_CHURN）：
>
> ```text
> git show 24d2de316:mir2-rs/bins/game-svr/src/lib.rs | grep -n "sm_charstatuschanged("
> git grep -n "sm_charstatuschanged\|SM_CHARSTATUSCHANGED" 24d2de316 -- mir2-rs/bins mir2-rs/crates
> ```
>
> **本表不是"这些都是对的清单"**，读它之前必须先看两条定性。

**定性 ①：读 `sg`（会话）的那批不等于错。** 是否错取决于**该点所在函数在重算之后有没有做实体镜像**
（`apply_ability_to_entity(...)`，或函数内直接写 `e.vitals.hit_speed = sg.hit_speed`）。
所以"镜像"列取自 §2 的 16 点扫描表；表里 ❌ 的那些点，657 的 `Series` 会读到**重算前的旧实体值**。

**定性 ②：`pickwire::notice_frame` 与 `readbook::maybe_read_book` 取的是"会话"不是"实体"。**
按本卡口径（`Series` = **收件人自己的实体值**），这两处**本身就是待判点**，不是参照物；
别把它们当成"正确写法"去对齐。

| # | 发射点（符号锚） | 收件人 | `Series` 源 | 所在函数有镜像 | 定性 |
| --- | --- | --- | --- | --- | --- |
| 1 | `push_charstatuschanged`（`SpellFrameBuf` 版 helper） | ±range **含自己**，逐人重编码 | **实体**（每个收件人自己） | —（helper） | 参照实现 |
| 2 | `push_charstatuschanged_except_self`（入队版 helper） | ±range 除自己 | **实体** | —（helper） | 参照实现 |
| 3 | `broadcast_charstatuschanged_except_self`（flush 版） | 同上 | **实体** | —（helper） | 参照实现 |
| 4 | `push_charstatuschanged_self_and_observers`（三动作） | 自见 ×2（`:2875` 广播副本 + `:2876` 自更新）+ 观察者 | **自见 = `hit_speed` 入参（会话，调用方传 `sg.hit_speed`）**；观察者 = 实体 | —（helper） | **待判**（自见那两份） |
| 5 | `send_charstatuschanged_self_once_and_observers` | 自见 ×1 + 观察者 | **自见 = 入参（会话）**；观察者 = 实体 | —（helper） | **待判**（自见那份） |
| 6 | `handle_spell_cmd_with_magic` · `SKILL_SHIELD` 臂的 `take_buff_applies` 循环 | 每人 1 条 | 实体（经 helper） | **❌**（本支无镜像） | 同族风险（镜像缺口） |
| 7 | `handle_spell_cmd_with_magic` · buff-apply-kind 支的 `take_buff_applies` 循环 | 每人 1 条 | 实体（经 helper） | ❌ | 同族风险（镜像缺口） |
| 8 | 同上 · 自更新副本（`push_actor`） | 自己 | 自己实体 | ❌ | 同族风险 |
| 9 | `handle_spell_cmd_with_magic` · 技能 36 支（`out.status_changed` + 自更新副本） | helper + 自己 | 实体 | **✅**（函数内直接写 `e.vitals.hit_speed = sg.hit_speed`） | 无损 |
| 10 | `handle_inbound_cmd`（登录入图） | 自己 | `sg.hit_speed`（会话） | ✅ `apply_ability_to_entity` | 无损（会话 = 刚镜像后的值） |
| 11 | `handle_bag_cmd_with_magic`（穿脱） | 自己 | `sg.hit_speed` | ✅ | 无损 |
| 12 | `drain_buff_downlink`（`boNeedRecalc`） | 自见 1 条 + 观察者逐人 | 自见 = 自己实体；观察者 = 各自实体 | **❌** | 同族风险（§3 第 3 点） |
| 13 | `drain_monster_ref_msgs`（`RM_CHARSTATUSCHANGED` 下行） | `m.recipients` 逐人 | **实体** | —（怪无 `RecalcAbilitys`） | 无损 |
| 14 | `drain_player_need_recheck_with_magic` | 自己 | `hit_speed`（会话） | ✅ | 无损 |
| 15 | `pickwire::notice_frame`（RM 出队接缝，只发自己那份） | 自己 | **会话 `session.hit_speed`** | —（函数内无重算） | **待判** |
| 16 | `readbook::maybe_read_book` | 自己 | **会话 `sg.hit_speed`** | —（函数内无重算） | **待判** |
| 17 | `smframe::frames_split_with_others`（登录队列的两条 657） | 自己 ×2 | `char_status` 的 `hit_speed` 输入（登录构建期 = 重算后值） | — | 无损（输入即新值） |

**§2 行 9 校正（本表暴露的自相矛盾）**：`handle_spell_cmd_with_magic` 的 shield/buff 支在
`card/657-series-spellbuf` 里**只改来源、没有加实体镜像**（该卡 §6 明写"实体镜像不在本卡"）；
§2 行 9 原句"✅（本批新增）"是更早草稿的残留，**已改为 ❌**。技能 36 支（行 8）的镜像在函数内
直接写 `e.vitals.hit_speed`，仍为 ✅。

**表外**：`msgqueue::send_update_msg` 一类的 `RM_CHARSTATUSCHANGED` **入队**不是发射点，
657 只在出队接缝（第 15 行）或宿主直发处产生。

## 3. 三个同族风险点（复现条件）

0. **`apply_pending_level_up`（升级链）：不受 657 值影响（`89` 静态分析确认）**——自见帧的 `Series`
   取 `sg.hit_speed`（`recalc_session_ability` 内已赋值），**不读实体**；观察者读的是**各自**实体，
   本 actor 重算不改别人 ⇒ 只保留"实体未镜像"这一不变量缺口登记，**不列风险、不加 657 用例**。
1. **`handle_eat_cmd`（吃物品）：本卡**原写"观察者面旧值"——**该表述已校正**：657 的 `Series`
   是**收件人自己**的值（`PlayObject.Message.cs:1845-1847` 的 `this.HitSpeed`），观察者收到的那条
   取的是观察者自己的实体，**不因吃物品者重算而陈旧**。真实缺口只有两处：
   ①吃物品者自己的实体 `hit_speed` 未镜像（**不变量缺口**，其自见 657 走 `pickwire::notice_frame` 的
   会话值，值本身是对的）；②该实体陈旧后，后续任何"读实体"的 657（如 `drain_buff_downlink`）会对
   该角色读出旧值。**故本处只做镜像、不列为 657 值风险。**
3. **`drain_buff_downlink`（`#9` 路径）**：`boNeedRecalc` 重算后自见与观察者都经
   `push_charstatuschanged` 读实体 ⇒ 全部旧值。复现：buff 施加/到期使 `HitSpeed` 变化
   （`ExtraAbil[HITSPEEDUP]` 或装备变化） + 非 0 旧值。

## 4. 验收口径

- 每个风险点**一条能区分的用例**：断言 **`Series` 的具体值**（重算后值 ≠ 旧值），
  **不是条数断言**；夹具必须让"旧值 ≠ 新值"（非退化输入）。
- **哨兵用例形式（`89` 建议，采纳）**：如 `drain_buff_downlink` 自见面——构造"实体 `hit_speed=99` /
  会话 `hit_speed=0`"的区分态，期望帧 `Series=0`、缺镜像时实发 `99`。**该用例是"区分性用例"，
  不代表线上存在这种不一致态**，卡面照此注明，落地时以它转绿。
- 每个点配**变异**：删掉镜像 ⇒ 该用例红（`golden/期望=新值` vs `实际=旧值`）。
- `apply_gm_use_item_write` 只登记，不修、不加用例。

## 5. 与其它卡的边界

- **与 `card/charstatus-657-b13`（`89`）重叠**：他正在改 `#1`/`#9`/`#13`/`level0`/`gm_smake` 等
  的"657 条数 2→1"，并已被要求补 **series 断言**。**本卡唯一的 657 值风险点是
  `drain_buff_downlink`（自见面）与 `handle_eat_cmd`（观察者面）**；`apply_pending_level_up`
  **仅登记不变量缺口、不影响 657 值**（见 §3 第 0 条与扫描表第 1 行）。
  与他改的**同一个函数/相邻代码**——**改前先对表**，避免两人改同一处。
  `handle_eat_cmd` 的观察者面归本卡；自我面 `WireSession` 归吃路径既有实现。
- 与 `card/657-series-spellbuf`：后者修的是 `handle_spell_cmd_with_magic` 两处（第 8/9 行），
  已含镜像；**本卡不重复、也不与之捆绑合并**。

## 6. 状态

- 本卡：**已实现，待复核合并**（§7.4 的实现记录）；扫描表与验收口径见 §2/§2bis/§3/§4。
- 不动任何清单 `status`、不改 golden/比较器。

## 7. 实现计划（定位已核，`master 24d2de316`；**未动手，等令牌**）

> 定位方式：`git show 24d2de316:mir2-rs/bins/game-svr/src/lib.rs` 上逐个函数读上下文；
> 行号只作引用（代码块内），散文只用符号锚。

### 7.1 五个改动点（按优先级）

| # | 符号锚 | 现状 | 改动 | 对 657 **值**的影响 | 牙 |
| --- | --- | --- | --- | --- | --- |
| **P1** | `handle_spell_cmd_with_magic` · 14/15 支（`HANGMAJINBUB`/`DEJIWONHO`）的 `take_buff_applies` 循环 | 657（广播副本 + 自更新副本）在 `recalc_session_ability` **之前**出帧 ⇒ 读的是**重算前**实体；且本支无镜像 | 把「recalc + 52」整块**移到**循环体最前（oracle `DefenceUp` 先 `RecalcAbilitys()`），recalc 后立刻 `e.vitals.hit_speed = sg.hit_speed` | **有**（target 由 7 → 0） | 7 世代 golden 的比较器 `real_rust_ledger_hangmajinbub` / `dejiwonho`：删镜像 ⇒ 红 |
| **P5** | `drain_buff_downlink`（`boNeedRecalc` 循环） | recalc 后直接发 52；657 自见取**实体**（未镜像 ⇒ 旧值） | recalc 之后、发帧之前加 `e.vitals.hit_speed = sg.hit_speed`（`*actor_id`） | **有** | 删镜像 ⇒ 该支自见 657 的 `Series` 断言红 |
| **P2** | `apply_pending_level_up` | recalc 后只写 `level/max_exp`，无 `hit_speed` 镜像 | 同函数实体写块里补一行 | **无**（自见 657 走 `send_charstatuschanged_self_once_and_observers` 的**会话**入参） | 实体值断言：`e.vitals.hit_speed == sg.hit_speed`；删镜像 ⇒ 红 |
| **P3/P4** | `handle_eat_cmd` 的两处 `recalc_session_ability` | 无镜像 | 各自 recalc 后补镜像 | **无**（自见 657 走 `pickwire::notice_frame` 的会话值；观察者读自己的实体） | 同上（实体值断言） |

**P1 的次序**：`100 → 657×2 → 52` 的**入 `SpellFrameBuf` 次序不变**（只把 recalc 提前），
优先级排序（52=67 < 657=155 < 100=252）出队结果因此逐字节不变；变的只有 657 的 `Series` 取值时点。

**P1 不动的**：`ev.char_status` 仍取 buff 事件里的值（golden 的 `param/tag` 已绿，不在本次范围）。

### 7.2 对 `c6` 六条比较器的**预先写死**的预期（先写下，避免事后解释）

修完 **P1** 后（`c6` rebase 到含本修的 master）：
- `shield`：**绿**——该支**不触发** recalc，实体本就是 7（`card/657-series-spellbuf` 已修来源）。
- `uenhancer`（36）：**绿**——函数内已有 `e.vitals.hit_speed = sg.hit_speed`。
- `hangmajinbub` / `dejiwonho`：**绿**——target 的 657 现在读重算后的 0（原先读 7）。
- `bigcloak` / `cloak`：**绿**（`c6` 未报红，且不经本卡改动面）。

⇒ **预期六条全绿。若 14/15 仍红，说明还有第二个来源**，届时按"别的来源"重新定位，不许把预期改写成"本来就会红"。
候选（尚未验证）：同场景里另有 `drain_buff_downlink` 产的 657、或 `ev.char_status` 的取值时点。

### 7.3 提交序（每步单独提交）

1. **P1**（解锁 `c6` 的四条红）——单独提交，先跑 `hangmajinbub`/`dejiwonho`/`shield` 的单包比较器；
2. **P5**——单独提交；
3. **P2/P3/P4**（不变量补全）——可合为一次提交，但提交信息逐点列出。

每步都要：`cargo test -p game-svr --lib` 全绿 + 该点自己的牙（删镜像 ⇒ 红）。

## 7.4 实现记录（本轮，已落分支）

| 提交 | 内容 | 证据 |
| --- | --- | --- |
| `d07cc84a2` | **P1**：14/15 支 recalc 前移 + 镜像 | 牙 `hangmajinbub_657_series_is_post_recalc`；变异①删镜像 ⇒ `实得 [7 ×98]`、变异②recalc 移回 657 之后 ⇒ 仍有 2 条取到 7 |
| `4f91d9f39` | **P2/P3/P4**：三处 recalc 后补镜像 | 扩 `gm_level_emits_657_self_and_nearby_observers`（删 P2 ⇒ 实体留 99）；新 `eat_recalc_mirrors_hit_speed_into_entity`（删 P3 ⇒ 红在「祝福油支」，删 P4 ⇒ 红在「经验书升级支」） |
| `cf9a97809` | 期望变更 + 解除缺口用例 ignore | 见下 |

**期望变更登记（旧的编码了缺陷行为）**：`buff_657_needrecalc_after_ability_and_per_recipient`
的自见 657 `Series` 由 `[7]` 改为 `[0]`——P5 补镜像后它读到的是**重算后**的 `HitSpeed`；
`7` 是重算前残留的实体旧值。同时解除 `buff_657_needrecalc_series_reads_post_recalc_value`
的 `#[ignore]`（该用例正是 `card/charstatus-657-b13` 登记本缺口时写的哨兵，P5 后自然转绿）。

**门禁（rebase 到 `fb2869fa1` 后实跑）**：`cargo fmt --all -- --check` rc=0；
`cargo test -p game-svr --lib` **2180 passed / 0 failed / 76 ignored**（rc=0）。

**`ignored` 由 77 降到 76 —— 是"解除一条 ignore"，不是"少跑一条"**：少掉的正是
`buff_657_needrecalc_series_reads_post_recalc_value`（`89` 立本缺口时加的 `#[ignore]` 哨兵）。
它现在**作为普通用例参与 2180 里的计数**；同一批 `passed` 由 2168（P1 时）→ 2180 的增量里也包含它。

**`[7] → [0]` 是 oracle 推出来的，不是"改到绿"**：夹具 `refmsg_657_fixture` 里 `src` 只有**实体**
`hit_speed = 7`（`EntitySpawn` 字段），会话 `test_session_ground` 不带任何 `HITSPEED` 加成 ⇒
`RecalcAbilitys()` 的 `HitSpeed = AddAbil.HitSpeed = 0`（`PlayObject.cs:2899`），而自见那条 657 在
`:2876` 入队、出队时读 `this.HitSpeed` ⇒ **oracle 值必为 0**。旧期望 `[7]` 取的是重算**前**的实体残留值。

## 7.5 为什么 `M-a` 当时测不出"顺序"（写给后来者）

`card/657-series-spellbuf` 之前做变异 `M-a`（顺序/镜像相关）时结果是"不红"，据此曾判定顺序不重要。
**那个结论只在当时成立**：那时 657 的 `Series` 还写死 `0`（单 body 广播），出帧时刻读到什么值与顺序无关。
`card/657-series-spellbuf` 把 `Series` 改成**读实体**之后，**顺序就变成可观测的**——
P1 的变异②（把 recalc 移回 657 之后）实跑仍能红出 2 条 `7` 就是直接证据。
⇒ **不要拿 `M-a` 的旧结论反驳新牙**；结论的有效性依赖于当时的取值口径。
