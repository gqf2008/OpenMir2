# openmir2-lottery-ticket-gm-realtime

> 状态：**follow-up 登记卡（2026-11-20，opc-07 发布）**。触发：IT-16（彩票）设计评审时 `openmir2-43` 指出的第二个消费者。
> 关联：`docs/issues/openmir2-items-parity.md` §9.11（IT-16）、`docs/issues/openmir2-runtime-sysmsg-text-frames.md`

## 1. 现状（事实，带锚）

- **oracle**：`@LotteryTicket`（`LotteryTicketCommand.cs`）直接读 `SystemShare.Config.WinLotteryCount / NoWinLotteryCount / WinLotteryLevel1..6` —— **实时进程值**。
- **Rust**：`gmcmd.rs` 的 `GmConfig.win_lottery_*` 是 **`Default::default()` 的常量快照**（构造处已定位），不随运行期变化。
- **IT-16 落地后的后果**：中奖/未中会改 `WinLotteryShared` 的实时计数（未中每次 `no_win += 500`），而 GM 命令仍显示常量 ⇒ **运维看到的是过期值**。

## 2. 目标

`@LotteryTicket` 显示 `WinLotteryShared` 的**实时**值；与 oracle 的"读进程当前值"语义一致。

## 3. 验收

- 一条**行为证据**：制造一次未中奖（`no_win += 500`）后，`@LotteryTicket` 的回包显示**新值**（新旧值都给原文）；
- 复位语义：`WinLotteryShared::reset(init)` 之后命令显示初值；
- 不得把命令改成"读配置默认值"充数——**必须是共享状态**。

## 4. 不做 / 边界

- 不改 IT-16 的判定逻辑（那是 §9.11 的范围）；
- 不改 oracle 的 `SysMsg` 时序 bug 复制（见 §9.11 的"oracle bug，为 parity 复制"登记）。
