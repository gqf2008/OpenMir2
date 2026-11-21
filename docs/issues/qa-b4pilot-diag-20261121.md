# QA 独立诊断：B4(ii) pilot 合并门两支红（bigcloak s5/s13，2026-11-21）

> 触发：`card/b4-ii-pilot @ dab72ec06` 的门 `-Mode Both`：Driver 148/150 红、Comparator 50/50 绿。
> 红：
> ```
> tests::big_cloak_parity_s5   [bigcloak/s5]  pilot：caster 1002 必须 900   left: []   right: [900]
> tests::big_cloak_parity_s13  [bigcloak/s13] pilot：caster 1002 必须 900   left: []   right: [900]
> ```
> 本文是第二个人的独立判断，用于与 `89` 自报根因交叉验证。**只读诊断**，未改产品代码、未跑 cargo。

## 1. 结论：B（期望错）——断言把 s1 的 actor id 写死

pilot 新增的六处取数（`big_cloak_assert` 内的 `due_of` 闭包）按
`ident == 657 && recog == <常量>` 过滤，常量写成 **`1002` / `1001`**——那是 **s1** 的 actor 段。
s5/s13 的 actor 段由 `id_base` 决定（`skeleton_rig`：caster = `id_base + 1`、第 i 个观察者 = `id_base + 2 + i`），
即 **2001/2002 与 3001/3002** ⇒ 过滤结果恒为 `[]`，与"pilot 有没有延迟这条帧"无关。

**不是 A（实现缺口）**：三支驱动走**同一条**路径——`big_cloak_assert` → `fireball_parity_drive`，
只有 `(seed, id_base)` 不同；`preload` 在全仓只有**一处**实现调用
（`drain_b3_self_deliveries_with_pending` 内、`preload=true` 时），`preload=true` 的调用点恰**两处**，
都在 rig 侧：`fireball_parity_drive` 的每步循环顶部、`B3Rig::deliver` 的同款分支；
**没有第三处**；产品侧 `sim_loop` 尚未接线（该函数标 `#[allow(dead_code)] // pilot seam`）。

**不是 C（语义错）**：延迟判据 `push_addressed` 用 turn 快照 `order` 比较**严格 `<`**，
`order` 由 `SelfDelayTurn.actor_id` 现场构造，与 `id_base` 无关。

## 2. 旧实现台账基线（pilot **之前**；留档对照用）

用 `card/b4-ii-pilot` 之前构建的单跑取证（台账目录重定向，未碰主门台账）：

| 档 | 657 的 `(recog, due_ms_rel)`（caster / target / obs_in 三收件人同形） |
| --- | --- |
| s1 | `(1001, 900)`、`(1002, 800)` |
| s5 | `(2001, 900)`、`(2002, 800)` |
| s13 | `(3001, 900)`、`(3002, 800)` |

> **这是旧实现的读数**。pilot 合入后，正确期望是 `caster` 的 **`id_base+2` 由 800 改到 900**、
> `id_base+1` 保持 900；`target`/`obs_in` 保持 `{id_base+2@800, id_base+1@900}`。
> 即 s5 应为 `(2002, 900)`、s13 应为 `(3002, 900)`。

## 3. 两条非红因的观察（供作者顺手处理）

1. **循环尾缺 final flush（latent）**：`fireball_parity_drive` 的 `while elapsed < wait_ms` 退出后，
   若 `pending_657` 仍非空，没有末步 preload/flush ⇒ **静默丢帧**。本用例延迟发生在 800ms、
   `wait_ms = 1200`（三档一致），900ms 下一步就 preload，**不触发**。
   处置建议（opc-19 已定）：退出时 `pending` 非空 ⇒ **具名 panic**（fail-closed）；确实允许残留必须有注释说明。
2. **注释与代码相反**：`push_addressed` 的文档注释写"下标 `<=` 当前下标"，实现是严格 `<`（与"钉死严格 `<`"的裁决一致）。
   注释应改成 `<`，否则误导后手。

## 4. 取证纪律（本次新增）

**单跑取证前先验二进制指纹**：opc-19 给的 exe 路径对应的二进制**不含** pilot 的调试串
（`DBG657` / `DBGNEW drain_b3_with_pending`），且用它单跑 s5 是 **1 passed** ⇒ 那是 pilot **之前**的构建，
不能用来复核 pilot 行为。以后单跑前至少核一项：`strings <exe> | grep <新代码特有串>`、
或用例名/断言串、或 mtime 与提交时刻对齐；不核指纹的"单跑结果"不可信。

## 5. 未验证项

pilot 构建下 s5/s13 的真实 `due` 值本文**未实测**（没有 pilot 二进制），是按"与 s1 同形"推断的
`base+2 → 900`。作者重编后单跑 s5 一次即可闭环。
