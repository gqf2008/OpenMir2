# openmir2-charstatuschanged-gap —— `RM_CHARSTATUSCHANGED(10139)` 在吃路径未映射（丢弃）

- 卡号建议：`openmir2-charstatuschanged-gap`（本文即卡面）
- 发现：2026-11-20，`IT-12` 祝福油探针（`mir2-rs/tests/parity/m3/oil-probe`）实测：
  `oil_vectors.json` 里**每一个"吃掉"的用例**，oracle 的通知表都多一条 `RM_CHARSTATUSCHANGED(10139)`
  （`priority=155`）。
- 状态：**未修**（登记）。影响面跨卡，不随 IT-12 一起改。

## 1. oracle 产生点

- `RecalcAbilitys()`（`src/M2Server/Actor/BaseObject.cs`）尾部
  `SendRefMsg(Messages.RM_CHARSTATUSCHANGED, 0, CharStatus, 0, 0, "")`（`:1516`）——
  `SendRefMsg` 对**自己 + 可见玩家**都入队/下发。
- 另有去重变体 `PlayObject.cs:2876` 的 `SendUpdateMsg(RM_CHARSTATUSCHANGED, HitSpeed, CharStatus, 0, 0, "")`
  （同 ident，`SendUpdateMsg` 会按 ident 先删旧条目；见 `gate_criticality.rs` 的 657 行注释）。
- **泵序键**：`(byte)wIdent = 10139 & 0xFF = 155` ⇒ 吃路径上
  `RM_SUBABILITY(62) < RM_ABILITY(67) < RM_WEIGHTCHANGED(131) < RM_CHARSTATUSCHANGED(155) < RM_SYSMESSAGE(252)`。

## 2. 宿主丢弃点

- `game_svr::pickwire::notice_frame` **没有 `RM_CHARSTATUSCHANGED` 臂** ⇒ 返回 `None`；
- `handle_eat_cmd` 的排水（`emit_eat_notices` 的 `None` 支）记
  `WRN CM_EAT：通知 ident=10139 未接 SM_* 映射（登记…）` 并**丢弃**。
- 注意：**657 的广播面已建模**（`mir2-rs/bins/game-svr/src/gate_criticality.rs` 的 657 行注释：`SM_CHARSTATUSCHANGED` 的收件人/去重口径）。本缺口只针对**发送者自己的那条自通知**（`RecalcAbilitys` → `SendRefMsg` 的 self 分支）；补映射时必须先核 self 与广播是否重复下发。

## 3. Blast radius（可复跑，按 grep 出清单）

```
# 宿主侧生产路径（**截至 `mod tests {` 之前**；原样 grep 会多出 13 处 mod tests 内的测试调用 ⇒ 29 行）
N=$(grep -n "^mod tests {" mir2-rs/bins/game-svr/src/lib.rs | head -1 | cut -d: -f1)
grep -rn "recalc_session_ability(" mir2-rs/bins/game-svr/src/lib.rs | awk -F: -v n="$N" '$1<n' | grep -v "fn recalc_session_ability" | wc -l   # 2026-11-20 = 16

# oracle 侧：RecalcAbilitys 的调用点（决定哪些行为分支触发）
grep -rn "RecalcAbilitys()" src/M2Server/Player/*.cs

# 已有模型（657 广播面）
grep -rn "RM_CHARSTATUSCHANGED" mir2-rs/bins/game-svr/src/
```

- 2026-11-20 的 `lib.rs` 生产调用点（行号随 master 漂移，以 grep 为准）：
  `2964 / 3166 / 4109 / 5231 / 7713 / 10809 / 10862 / 14084 / 16168 / 18385 / 19000 / 21932 / 21938 / 24599 / 24880 / 25462`。
- 已实证命中：**IT-12 祝福油**（`PlayObject.WeaptonMakeLuck` 两支行内 `RecalcAbilitys()`，
  `src/M2Server/Player/PlayObject.cs:1545` / `:1624`）。
- 待复评（按上表逐条核）：IT-08 升级支（`HasLevelUp → RecalcAbilitys`）、IT-14/15 修复油
  （`RepairWeapon`/`SuperRepairWeapon` 本身不 Recalc，但同路径若有 Recalc 才命中）、
  以及一切经 `recalc_session_ability` 的装备/属性变更路径。

## 4. 补齐后哪些卡可复评

- IT-12：把 10139 纳入 `pumped`/`notices` 对比（当前探针已记录，比较器按缺口排除）。
- 其余命中的卡：按 §3 的 greps 清单逐个复核「通知表」是否漏这一条。

## 5. 修复要点（不在本卡范围，供接卡人）

1. `notice_frame` 增 `RM_CHARSTATUSCHANGED` 臂：`SM_CHARSTATUSCHANGED(657)` 的
   `Recog/Param/Tag` 口径见 `PlayObject.Message.cs:1845-1847`（`SendRefMsg` 传 `CharStatus`）。
2. 去重口径：`SendUpdateMsg` 变体在 oracle 按 ident 去重（先删旧条目）；`SendRefMsg` 变体不去重。
   补映射前先确认本路径属于哪一种。
3. 不得静默：在补上前，`emit_eat_notices` 的 WRN 必须保留（现状即如此）。
