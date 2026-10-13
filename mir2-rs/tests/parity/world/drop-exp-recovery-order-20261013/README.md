# reviewer-5 O-1 收口：`sim_loop` 同 tick 帧序（消息段 → 体力/魔力点段）

> 卡：**`openmir2-m3-drop-exp-wiring`**（M3④-实现 D，owner `openmir2-worker-11`）。
> 来源：reviewer-5 在 review `1791904017`（@被审 head `d2155f9c`，已并入 master）的 findings **O-1**。
> 工作目录 `.claude/worktrees/m2-svr`（分支 `drop-exp-host`）；base = `origin/master 5964479f3`。
> 写集：`mir2-rs/bins/game-svr/src/main.rs`（本片零 `crates/**`、零 `src/**`、零 `tests/parity/m3/**`）。

## 0. 一句话

把 `sim_loop` 每 tick 的帧面下行段按 oracle 的 `Run()` 次序重排：**消息段**（击杀经验/升级链
`drain_kill_exp_chain` + AOI 出现帧 `drain_aoi_downlink`）**先于** `BaseObject.Run()` 的
**体力/魔力点段**（`drain_stamina`）；同时把 `drain_periodic_save` 后移到整个 `Run()` 之后
（oracle `WorldServer.ProcessHumans` 是 `playObject.Run()` 之后再 `SaveHumanRcd`）。

## 1. 问题（reviewer-5 O-1）

oracle 的体力点段在**消息循环之后**跑（`BaseObject.Base.cs:28-34` 消息段 → `:37-81/:105-110` 填充/扣血），
而修复前 Rust `sim_loop` 把 `drain_stamina` 放在 `drain_kill_exp_chain` **之前**。同 tick 内
某玩家既升级又扣体力点时，线上序变成 `53(体力) → 53(升级) → 44 → 45 → 52 → 752`，
而 oracle 是 `53(升级) → 44 → 45 → 52 → 752 → 53(体力)`。

## 2. 交付

| 位置 | 改动 |
| --- | --- |
| `bins/game-svr/src/main.rs`（新 `drain_run_downlinks`） | 把 `drain_kill_exp_chain → drain_aoi_downlink → drain_stamina` 固定在**一处**（顺序可被用例钉住） |
| `bins/game-svr/src/main.rs`（`sim_loop`） | 用 `drain_run_downlinks` 取代原先分散的三段；`drain_periodic_save` 后移到它之后；`drain_recovery`（T-28）不变（它本就在消息段之前，位置正确） |
| `bins/game-svr/src/main.rs`（`drain_stamina` 文档） | 「与 `drain_recovery` 同一位置」订正为「消息段之后」，指向 `drain_run_downlinks` |
| `bins/game-svr/src/main.rs`（新用例） | `tests::tick_frame_order_puts_stamina_after_kill_exp_chain` |

oracle 锚点逐行见 `01-oracle-anchors.txt`；改前/改后见 `03-change.txt`。

## 3. 判据与牙齿

- **构造级判据**：`tick_frame_order_puts_stamina_after_kill_exp_chain` —— 同 tick 既 `leveled`
  `exp_awards` 又 `stamina.health_tick < -HealthFillTime` ⇒ 断言
  `idents == [SM_HEALTHSPELLCHANGED(53), SM_WINEXP(44), SM_LEVELUP(45), SM_ABILITY(52), SM_SUBABILITY(752), SM_HEALTHSPELLCHANGED(53)]`
  且首条 53 `param == MaxHP`（升级满血快照）、末条 53 `param == MaxHP-1`（体力点扣 1）。
- **改坏必红**：把 `drain_stamina` 挪回 `drain_kill_exp_chain` 之前 ⇒ 用例 `FAILED rc=101`，
  `left: [53, 53, 44, 45, 52, 752]`（原始件 `08-redcheck-frame-order.txt`，含还原后复绿）。
- **边界**：见 `09-boundary.txt`（净差量仅 `bins/game-svr/src/main.rs` + 本证据目录；冻结面空）。
- 真机两侧同序列对拍（100 杀 DoD②③⑤，含本帧序站点）仍属**窗口项**，由 owner 在独占窗口复验。

## 4. 门禁（隔离 `CARGO_TARGET_DIR=E:/tmp/m2svr-target`，检出外）

| 门 | 结果 | 原始件 |
| --- | --- | --- |
| `cargo fmt --all -- --check` | rc=0 | `04-gates-fmt.txt` |
| `cargo clippy --offline --workspace --all-targets -- -D warnings` | rc=0 | `05-gates-clippy.txt` |
| `cargo test --offline --workspace` | 101 suites · **1191 passed · 0 failed · 22 ignored** | `06-gates-workspace-test.txt` |
| `cargo run --offline -p msgcodegen -- --check` | rc=0（604 条） | `07-gates-msgcodegen.txt` |

## 5. 证据清单

`01-oracle-anchors.txt` / `02-commands.txt` / `03-change.txt` / `04-gates-fmt.txt` /
`05-gates-clippy.txt` / `06-gates-workspace-test.txt` / `07-gates-msgcodegen.txt` /
`08-redcheck-frame-order.txt` / `09-boundary.txt` / `README.md` / `SHA256SUMS.txt`。
