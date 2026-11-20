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
| 9 | `handle_spell_cmd_with_magic` | 施法（shield/buff） | ✅（本批新增） | 是 | 实体 | 无损（见 `card/657-series-spellbuf`） |
| 10 | `drain_buff_downlink` | buff 排水 | ❌ | 是 | 实体（自见+观察者都经 helper） | **同族风险（`#9` 路径）** |
| 11 | `drain_levelup_healthspell_with_magic` | 升级回血 | ✅ | 是 | 实体 | 无损 |
| 12 | `apply_pending_gm_target_ops`（臂 1） | GM 删技能 | ✅ | 是 | 实体（会话优先？按实体裁定应为实体） | 无损 |
| 13 | `apply_pending_gm_target_ops`（臂 2） | GM 删技能 | ✅ | 是 | 实体 | 无损 |
| 14 | `drain_player_need_recheck_with_magic` | P21 装备复检 | ✅ | 是 | 实体 | 无损 |
| 15 | `drain_player_hunger` | 饥饿跨档 | ✅ | 是 | 实体 | 无损 |
| 16 | `drain_player_revival_with_magic` | 复活戒指 | ✅ | 是 | 实体 | 无损 |

⇒ **10 有 / 6 无**；其中 **3 个函数（4 个调用点）有 657 且读实体**，是真正的同族风险面。

## 3. 三个同族风险点（复现条件）

0. **`apply_pending_level_up`（升级链）：不受 657 值影响（`89` 静态分析确认）**——自见帧的 `Series`
   取 `sg.hit_speed`（`recalc_session_ability` 内已赋值），**不读实体**；观察者读的是**各自**实体，
   本 actor 重算不改别人 ⇒ 只保留"实体未镜像"这一不变量缺口登记，**不列风险、不加 657 用例**。
1. **`handle_eat_cmd`（观察者面，半对）**：吃物品触发 `RecalcAbilitys` 后**观察者**收到的 657
   `Series` 走实体（旧值）；**自我面**走 `WireSession`（`sg`，已新）⇒ 同一事件两套值。
   复现：吃使 `hit_speed` 变化的物品（如带 `atk_spd` 的武器切换类），一名观察者在 ±range 内。
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
  的"657 条数 2→1"，并已被要求补 **series 断言**。本卡的两个风险点（`apply_pending_level_up`、
  `drain_buff_downlink`）与他改的是**同一个函数/相邻代码**——**改前先对表**，避免两人改同一处。
  `handle_eat_cmd` 的观察者面归本卡；自我面 `WireSession` 归吃路径既有实现。
- 与 `card/657-series-spellbuf`：后者修的是 `handle_spell_cmd_with_magic` 两处（第 8/9 行），
  已含镜像；**本卡不重复、也不与之捆绑合并**。

## 6. 状态

- 本卡：`open`（未实现）；仅扫描表与验收口径。
- 不动任何清单 `status`、不改 golden/比较器。
