# buff 路径的 657 接线与 `boChg` 逐收件人修正（2026-11-20）

> base `master = cc62ee05e`；分支 `card/657-buff`。本卡对应"五处 657 补法"清单的**第 1 批**（`#9`）。
> 规划来源：`docs/issues/qa-657-wiring-plan-20261120.md`。

## 1. 缺口的判定与修复

### 1.1 `boChg` 支：单 body + `Series` 恒 0（**已存在的线上偏差**）
- 旧实现（`drain_buff_downlink` 的 `buff_status_changes` 支）：
  `let body = sm_charstatuschanged(actor, status, 0); broadcast_ref_msg_with_self(.., &[body]);`
- oracle handler：`SendDefMessage(SM_CHARSTATUSCHANGED, processMsg.ActorId, LoWord(nParam1),
  HiWord(nParam1), this.HitSpeed)`——**`Series` 取收件人自己出队时的 `HitSpeed`**
  （`src/M2Server/Player/PlayObject.Message.cs:1845-1847`）。
- 影响：单 body 把所有收件人的 `Series` 固定成 0；buff 路径是高频路径（每次施加/到期）。
- **修法**：改用 `send_charstatuschanged_self_and_observers`（自见 1 + 观察者每人按
  `world.entities.get(target).hit_speed` 重编码），再补一条自见副本（`SendUpdateMsg` 去重不命中 ⇒ 自见 2 条）。
- **为什么不用 `broadcast_ref_msg_with_self`**：其签名是 `bodies: &[Vec<u8>]`，同一份 body
  发给所有收件人 ⇒ `Series` 固定成源值。**下一个人看到这里不要"复用"它来发 657。**

### 1.2 `boNeedRecalc` 支：657 完全缺失（新增接线）
- oracle `RecalcAbilitys()` 在 `Race == ActorRace.Play` 时无条件
  `CharStatus = GetCharStatus(); StatusChanged(); SendUpdateMsg(RM_CHARSTATUSCHANGED, …)`
  （`src/M2Server/Player/PlayObject.cs:2851-2876`）⇒ 自见 2 条 + 观察者各 1 条。
- 入队 657(155) 晚于 `RM_ABILITY`(67) ⇒ 线上 **52 先、657 后**（本宿主 52 直发，657 排其后）。
- 修法：每 actor 的 recalcs 循环里，发完 52 后逐收件人发 657（同 §1.1 的 helper + 第二自见副本）。

## 2. 既有期望的改动（逐处）

| 用例 | 改前 | 改后 | 依据 |
| --- | --- | --- | --- |
| `drain_buff_downlink_emits_charstatuschanged_and_healthspellchanged`（`boChg` 657 唯一旧用例） | 自见 657 **1 条** | **2 条**（StatusChanged + SendUpdateMsg）；`Series=0` 的**值不变**（夹具 `hit_speed` 本就是 0） | oracle `:2851-2876` |
| `drain_buff_downlink_emits_ability_then_657_then_sysmsg`（A2 线序） | `[52, 657, 100]` | `[52, 657×4, 100]`（recalcs 支 2 + status_changes 支 2）；`Series` 未断言 | 同上 |

**未改**：其它以 `Series=0` 为期望的用例（卸装链 watcher 等）——它们的收件人 `hit_speed` 本身就是 0，值合法。
**另需登记**：golden `[stem/label] 657 的 (param,tag,series)` 断言为 `== (0,0,0)`——oracle 夹具在
`HitSpeed=0` 区间取得，**该 golden 对 `Series` 无区分力**（承认证据边界，不是掩饰）。

## 3. 退化区间结论（独立问题）

**buff 路径的 657 证据全部取自 `HitSpeed = 0` 的退化区间**：改动前 buff 657 仅两条用例，
`hit_speed` 均未设 ⇒ `boChg` 的"固定 0"至今未红。
反例佐证（per-recipient 机制本身有覆盖，但不走 buff）：`t135_charstatuschanged_per_recipient_hit_speed_on_production_drain`
（p1=7/p2=13/far=99）、登录 657（近邻 `series=9`）、`refmsg_657_fixture`（观察者 20+i）。
⇒ **问题只在 buff 这一条路径**；不是"整套 657 证据都不可信"。

## 4. 牙（三颗，实跑）

1. `buff_657_bochg_series_is_per_recipient_hit_speed`：源 7 / 观察者 +5/+11/+12 = 20/21/22、+13 超 range。
   断言自见 2 条 `Series=7`、三名观察者各 1 条且 `Series` 分别等于自己的值、+13 零帧。
2. `buff_657_bochg_three_recipients_distinct_series`：把三名观察者改成 **3/7/9**（全非 0 且互异），
   断言各自 `Series` 正确（防"两人里恰好一个 0"的巧合）。
3. `buff_657_needrecalc_after_ability_and_per_recipient`：自见线序 `[52, 657, 657]` + 观察者逐人
   `Series`。

**证据分工（防误读）**：既有 `drain_buff_downlink_emits_charstatuschanged_and_healthspellchanged`
的收件人是自己且 `hit_speed` 未设（=0）⇒ 它**只守"自见 2 条 + CharStatus 字段"**，单 body 与逐收件人
产出相同，**不能**用它证明逐收件人；逐收件人由下面两颗新牙（夹具非 0）验证。

**M1 的红必须红在 `Series` 值（实测原文）**：
```
buff_657_bochg_series_is_per_recipient_hit_speed:
  assertion `left == right` failed: 自见 Series = 源自己 hit_speed   left: 0  right: 7
buff_657_bochg_three_recipients_distinct_series:
  三个收件人各自 Series 必须是对应自己的 3/7/9；实得 [0, 0, 0]
```
（M2 的红才是帧数（自见 1 vs 2）、M3 的红才是缺帧（`[52]` vs `[52,657,657]`）——它们是**故意**验那两件事。）
**变异实跑**（还原后 `lib.rs` 无 `MUTATION` 标记）：
| 变异 | 结果 |
| --- | --- |
| M1 退回单 body（`broadcast_ref_msg_with_self` + `Series` 0） | `buff_657_bochg_series_*` 与 `buff_657_bochg_three_*` **红** |
| M2 删第二自见副本 | `buff_657_bochg_series_*` 红（自见 1 vs 2） |
| M3 短路 `boNeedRecalc` 的 657 段 | `buff_657_needrecalc_*` 红（`[52]` vs `[52,657,657]`） |

## 5. 门禁

- `cargo test -p game-svr --lib`：**2149 passed / 0 failed / 70 ignored**（rc=0；基线 2146 + 3 新牙）。
- `cargo test --workspace`：**4374 passed / 0 failed / 155 ignored**（rc=0）。
- `cargo clippy --workspace --all-targets -- -D warnings`：**rc=0**
  （首跑曾红：新牙里 `get(&905).is_none()` 触发 `clippy::unnecessary-get-then-check`，已改 `!contains_key(&905)`。
  **教训①**：交付里"待补"必须按"可能红"写，不能当成"还没碰的绿地"——`ae` 对 §5 的指摘即此。
  **教训②**：`--all-targets` 的 lint 只跑 `--lib` 测试时**不一定暴露**，与 fmt 一起列入必跑项。）
- `cargo fmt --all -- --check`：rc=0。
- `cargo run -p msgcodegen -- --check`：rc=0（604 条常量一致）。
