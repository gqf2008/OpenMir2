# SITDOWN 成功支：+GD 直发补接 / RM_POWERHIT 取证订正（tier L）

- 卡：`openmir2-sitdown-powerhit-broadcast`（tier **L**，owner `openmir2-dev-2`，branch `card/sitdown-powerhit-broadcast`）。
- 基线：`master = 86f8e32fa`（2026-10-25）。
- 写集：`mir2-rs/bins/game-svr/src/lib.rs`（SITDOWN 支）、合同测试、本文档。
- 承接：`docs/inbound-dispatch-emitpoints-20261025.md` 的 M-1/M-2（缺发帧 1/2、2/2）。

## 1. 结论先说：M-1 不是客户端帧缺口

- oracle `ClientSitDownHit` 成功支确实发 `SendRefMsg(RM_POWERHIT, 0,0,0,0,"")`
  （`src/M2Server/Player/PlayObject.Operate.cs:359`）。
- 但**接收端没有 `RM_POWERHIT` 的 case**：全仓 `grep -rn "RM_POWERHIT" src` 只有
  `Operate.cs:359`（发送）与 `src/OpenMir2/Messages.cs:705`（常量）；`PlayObject.Message.cs`
  与 `BaseObject.Base.cs` 的 `switch` 都没有该 ident（`SM_POWERHIT=18` 只由 `RM_SPELL2` 映射，
  `PlayObject.Message.cs:1464`）。
- ⇒ oracle 的这条广播对客户端**零帧**；Rust 目前不发，客户端可观测行为相同。
  **不凭空补发 `SM_POWERHIT(18)`**（forbidden：不得虚构帧/顺手改其它 handler）。
- M-1 的真实缺口只是 **server-side `SendRefMsg` 调用本身**（及其 `SendRefMsgTick`/
  `VisibleHumanList` 缓存副作用）；当前 Rust 不建模该缓存 ⇒ 无可观测差异。本卡**不实现一条
  无接收端的假广播**；若后续确需 1:1 复刻离群网络调用，另卡按「不可观测/内部缓存」登记。
  该订正已同步到 inbound 取证表（`docs/inbound-dispatch-emitpoints-20261025.md` 复核修订，
  分支 `card/inbound-dispatch-emitpoints` commit `8e444a20c`）。

## 2. M-2 已接：`SendSocket(M2Share.GetGoodTick)`

### 2.1 oracle

- `PlayObject.Message.cs:1284-1338`（`case CM_SITDOWN`）：`ClientSitDownHit(...)` 返回 true ⇒
  `ActionTick = GetTickCount(); SendSocket(M2Share.GetGoodTick);`
- `M2Share.GetGoodTick`（`src/M2Server/M2Share.cs:205`）= `string.Format(MessageSettings.sSTATUS_GOOD, GetTickCount())`；
  `sSTATUS_GOOD = "+GD/{0}"`（`src/Modules/SystemModule/MessageSettings.cs:6`）。
- `SendSocket(string)`：`PackLength = -len`、正文原样 ⇒ 网关发出 `'#' + 正文 + '!'`
  （真机观测 `#+GD/<tick>!`）。

### 2.2 Rust

- `SITDOWN` 成功支（`mir2-rs/bins/game-svr/src/lib.rs:3030`）现在调用既有
  `send_good_tick(online, gates, chr_name)`（定义 `mir2-rs/bins/game-svr/src/lib.rs:13581`），
  与 Walk/Spell 等路径共用同一 `movewire::good_tick_payload`（`SendRaw` 短包）。
- **没有**新增 `SM_POWERHIT` 帧（见 §1）。

### 2.3 合同测试

`tests::sitdown_success_emits_good_tick_and_no_powerhit_frame`（生产入口 `handle_inbound_cmd`）：

- 恰好 1 条 `SimToGate::SendRaw`，payload 以 `b"+GD/"` 开头；
- **0 条** `SimToGate::SendFrame`（钉住不凭空发 `SM_POWERHIT(18)`）。

### 2.4 牙

| 牙 | 变异 | 结果 |
|---|---|---|
| T1 | 删除 `send_good_tick(online, gates, chr_name);` | **RED**：`sitdown_success_emits_...` 失败（`0 passed; 1 failed`）；还原后绿 |

## 3. 门禁

| 检查 | 结果 |
|---|---|
| `cargo fmt --all -- --check` | rc=0 |
| `cargo clippy --workspace --all-targets -- -D warnings` | rc=0 |
| `cargo test --workspace` | **3196 passed / 0 failed** |
| `cargo run -p msgcodegen -- --check` | rc=0（604 条） |
| `conflict_marker` / `stale_pointer` / `dynamic_count_stamp` | rc=0 |
| `git diff --check` | rc=0 |
| `git merge-tree --write-tree master HEAD` | 提交后复跑，见 patch body |

## 4. 残留

- `ActionTick`（oracle `PlayObject` 字段）未建模——本支只补 `+GD` 直发；该字段的其它消费者
  （攻击/施法节拍）是否已有等价物不在本卡范围。
- M-1 的 server-side `SendRefMsg(RM_POWERHIT)` 与 `VisibleHumanList`/`SendRefMsgTick` 缓存未复刻
  （客户端零帧、无当前可观测差异）；按 §1 登记。
- 临时物：`E:/tmp/sitdown.bak`（牙备份，已还原；可删）、`/tmp/sit-*.out`、`/tmp/sit-tooth.log`。
- 不 merge、不自审。
