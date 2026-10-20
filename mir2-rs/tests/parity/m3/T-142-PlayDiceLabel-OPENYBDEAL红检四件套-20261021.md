# T-142 红检四件套（PlayDiceLabel 第三支 / OPENYBDEAL）

- 卡：`scriptpd`（车道 scriptpd）；登记：`mir2-rs/tests/parity/whitelist.md` 的 **T-142**（原分支条目曾用 T-141，与 MB-6 撞号，rebase 时整体改为 T-142）。
- 目的：兑现 `feat(script): PlayDiceLabel 第三支与 OPENYBDEAL 动作` 提交里「红检见该批报告」的承诺——本文件给出**命令 + 输出 + 路径 + hash** 四件套，可逐条复跑。
- 判据口径：**红检 = 改坏生产路径后，指定断言必须变红；还原后 sha256 与改前一致**。所有改坏只在工作树内临时发生，未提交、未推送。
- 运行环境：worktree `E:/tmp/wt-scriptpd`，分支 `script-playdice`；`CARGO_TARGET_DIR=/e/tmp/wt-scriptpd-target`；cargo 命令在 `mir2-rs/` 下执行。
- oracle 锚点：`src/M2Server/Player/PlayObject.Base.cs:2814-2829`（`LableIsCanJmp` 三支）、`src/Modules/ScriptEngine/Processings/ExecutionProcessingSys.cs:2030-2033`（`ActionOfPlayDice` 写 `PlayDiceLabel`）、`:2329-2357`（`ActionOfOpenSaleDeal`）、`src/M2Server/Player/PlayObject.cs:588`（`PlayDiceLabel` 未初始化自动属性）、`:856`（`SaleDeal` 会话字段）、`src/M2Server/M2Share.cs:600+`（`GetValNameNo`）。

## 0. 绿基线（改坏前）

```console
$ cd mir2-rs && CARGO_TARGET_DIR=/e/tmp/wt-scriptpd-target cargo test -p mir2-script --test engine_tests <name> -- --exact
```

```text
test lable_is_can_jmp_three_branches_and_play_dice_is_one_shot ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 58 filtered out; finished in 0.00s
test action_of_play_dice_label_is_consumed_once_by_jmp_gate ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 58 filtered out; finished in 0.00s
test lable_is_can_jmp_none_and_some_empty_are_not_equivalent ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 58 filtered out; finished in 0.00s
test open_yb_deal_deducts_gold_and_opens_sale_deal ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 58 filtered out; finished in 0.00s
test open_yb_deal_reads_numeric_var_parameter ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 58 filtered out; finished in 0.00s
test open_yb_deal_insufficient_and_already_open_paths ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 58 filtered out; finished in 0.00s
test open_yb_deal_param_boundary_falls_back_to_zero ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 58 filtered out; finished in 0.00s
```

被测文件 sha256（改坏前 = 还原后应一致）：

```text
6d8dcb1b1f959ced7f8e2a2ab92c9856013330185611915af83b240ac381eb1e  mir2-rs/crates/script/src/engine/mod.rs
320de2b0fd3600d40bf350373eddbc08874a87c0ef81ba522f1354fc2d962084  mir2-rs/crates/script/src/engine/actions.rs
bd42b5732a1d5edc1ad410e1a4a1368a6fc70216b36421cfc58db54f9cec2908  mir2-rs/crates/script/src/engine/vars.rs
c553a79e505d94e0e3835ed364318df87ef1118efa6081ec789a3afb4da7e80b  mir2-rs/crates/script/tests/engine_tests.rs
```

## 1. 红检矩阵（8 颗牙）

| # | 判据 | 改坏点（生产路径） | 指定断言（必须红） | rc |
| --- | --- | --- | --- | --- |
| M1 | `@main` 恒可跳且不消费骰标 | `lable_is_can_jmp` 删除 `@main` 支的 `return true` | `lable_is_can_jmp_three_branches_and_play_dice_is_one_shot` | 101 |
| M2 | 第三支命中即一次性清空（`string.Empty`） | 删除 `player.set_play_dice_label("")` | `action_of_play_dice_label_is_consumed_once_by_jmp_gate` | 101 |
| M3 | `CanJmpScriptLableMap` 命中可跳 | 表命中支加 `false &&` 失效 | `lable_is_can_jmp_three_branches_and_play_dice_is_one_shot` | 101 |
| M4 | 成功支是 `GameGold >= 费用`（恰好相等也成功） | `>=` 改 `>` | `open_yb_deal_reads_numeric_var_parameter` | 101 |
| M5 | 费用先走 `GetValValue` 数值变量 | `Some(v) => v` 改 `Some(_) => 0` | `open_yb_deal_reads_numeric_var_parameter` | 101 |
| M6 | 已开通 ⇒ 提示后**早退** | 删除已开通支的 `return` | `open_yb_deal_insufficient_and_already_open_paths` | 101 |
| M7 | `None`（C# null）与 `Some("")` 不等价 | `if let Some(_)` 改为 `unwrap_or_default()` | `lable_is_can_jmp_none_and_some_empty_are_not_equivalent` | 101 |
| M8 | `P100`（len=4）取首位数字 ⇒ `MNVal[1]`，不回落 -1 | `P` 分支 `len > 3` 时强制 `-1` | `open_yb_deal_param_boundary_falls_back_to_zero` | 101 |

## 2. 原始红检流水（命令 / 输出 / 路径 / hash）

> `SHA_MUTATED` = 改坏后的文件 hash；`SHA_RESTORED` = 从备份拷回后的 hash（应等于第 0 节的绿基线 hash）。

```text
PRISTINE mod.rs:    6d8dcb1b1f959ced7f8e2a2ab92c9856013330185611915af83b240ac381eb1e
PRISTINE actions.rs:320de2b0fd3600d40bf350373eddbc08874a87c0ef81ba522f1354fc2d962084
PRISTINE vars.rs:   bd42b5732a1d5edc1ad410e1a4a1368a6fc70216b36421cfc58db54f9cec2908

=== M1 @main 支不再恒真 ===
$ cargo test -p mir2-script --test engine_tests lable_is_can_jmp_three_branches_and_play_dice_is_one_shot -- --exact
FILE: mir2-rs/crates/script/src/engine/mod.rs
SHA_MUTATED: 9afaf57e6bf2a285c98d4d6f5fd5ef4f862aabe74a31a8573d6e43b0eeb99cc1

thread 'lable_is_can_jmp_three_branches_and_play_dice_is_one_shot' (14556) panicked at crates\script\tests\engine_tests.rs:1986:5:
assertion `left == right` failed: @main 先命中，不消费骰标
  left: Some("")
 right: Some("@main")
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace


failures:
    lable_is_can_jmp_three_branches_and_play_dice_is_one_shot

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 58 filtered out; finished in 0.00s

error: test failed, to rerun pass `-p mir2-script --test engine_tests`
RC: 101
SHA_RESTORED: 6d8dcb1b1f959ced7f8e2a2ab92c9856013330185611915af83b240ac381eb1e

=== M2 去掉 PlayDiceLabel 一次性清空 ===
$ cargo test -p mir2-script --test engine_tests action_of_play_dice_label_is_consumed_once_by_jmp_gate -- --exact
FILE: mir2-rs/crates/script/src/engine/mod.rs
SHA_MUTATED: 76d460102056b8921a6161bc470fe66b827baf16cf49602043f8ee09135b02b4

thread 'action_of_play_dice_label_is_consumed_once_by_jmp_gate' (22236) panicked at crates\script\tests\engine_tests.rs:2038:5:
assertion `left == right` failed
  left: Some("@dice")
 right: Some("")
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace


failures:
    action_of_play_dice_label_is_consumed_once_by_jmp_gate

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 58 filtered out; finished in 0.00s

error: test failed, to rerun pass `-p mir2-script --test engine_tests`
RC: 101
SHA_RESTORED: 6d8dcb1b1f959ced7f8e2a2ab92c9856013330185611915af83b240ac381eb1e

=== M3 表命中支失效 ===
$ cargo test -p mir2-script --test engine_tests lable_is_can_jmp_three_branches_and_play_dice_is_one_shot -- --exact
FILE: mir2-rs/crates/script/src/engine/mod.rs
SHA_MUTATED: afb36585e4fa7ca13a97a087026fd1fda8cbdb064fe0bfd63ca78a096f6decad

---- lable_is_can_jmp_three_branches_and_play_dice_is_one_shot stdout ----

thread 'lable_is_can_jmp_three_branches_and_play_dice_is_one_shot' (1656) panicked at crates\script\tests\engine_tests.rs:1992:5:
assertion failed: lable_is_can_jmp(&mut player, &table, "@table")
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace


failures:
    lable_is_can_jmp_three_branches_and_play_dice_is_one_shot

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 58 filtered out; finished in 0.00s

error: test failed, to rerun pass `-p mir2-script --test engine_tests`
RC: 101
SHA_RESTORED: 6d8dcb1b1f959ced7f8e2a2ab92c9856013330185611915af83b240ac381eb1e

=== M4 成功率判定 >= 改 > ===
$ cargo test -p mir2-script --test engine_tests open_yb_deal_reads_numeric_var_parameter -- --exact
FILE: mir2-rs/crates/script/src/engine/actions.rs
SHA_MUTATED: 9287c4f4ac55e0a034c7d2df813c2a787b630db9b8232fbd0792376b7fbf37b7

thread 'open_yb_deal_reads_numeric_var_parameter' (18776) panicked at crates\script\tests\engine_tests.rs:2069:5:
assertion `left == right` failed
  left: 20
 right: 0
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace


failures:
    open_yb_deal_reads_numeric_var_parameter

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 58 filtered out; finished in 0.00s

error: test failed, to rerun pass `-p mir2-script --test engine_tests`
RC: 101
SHA_RESTORED: 320de2b0fd3600d40bf350373eddbc08874a87c0ef81ba522f1354fc2d962084

=== M5 数值变量读取被吞（恒用 0） ===
$ cargo test -p mir2-script --test engine_tests open_yb_deal_reads_numeric_var_parameter -- --exact
FILE: mir2-rs/crates/script/src/engine/actions.rs
SHA_MUTATED: d3257e71cc5b1682f065425a4ccdf1b0efac7e056e1ac51f2267e7fcf07cb69f

thread 'open_yb_deal_reads_numeric_var_parameter' (7856) panicked at crates\script\tests\engine_tests.rs:2063:5:
assertion `left == right` failed
  left: 50
 right: 30
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace


failures:
    open_yb_deal_reads_numeric_var_parameter

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 58 filtered out; finished in 0.00s

error: test failed, to rerun pass `-p mir2-script --test engine_tests`
RC: 101
SHA_RESTORED: 320de2b0fd3600d40bf350373eddbc08874a87c0ef81ba522f1354fc2d962084

=== M6 去掉已开通早退 ===
$ cargo test -p mir2-script --test engine_tests open_yb_deal_insufficient_and_already_open_paths -- --exact
FILE: mir2-rs/crates/script/src/engine/actions.rs
SHA_MUTATED: 20fe89bf6f2149398393d26f60d8e7da399dcb09b3d0b70f3454d5547d574ce8

thread 'open_yb_deal_insufficient_and_already_open_paths' (19372) panicked at crates\script\tests\engine_tests.rs:2089:5:
assertion `left == right` failed
  left: "msg 11009 0 0 0 0 测试玩家/开通寄售服务成功!!!\\ \\<返回/@main>"
 right: "msg 11009 0 0 0 0 测试玩家/您已开通寄售服务,不需要再开通!!!\\ \\<返回/@main>"
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace


failures:
    open_yb_deal_insufficient_and_already_open_paths

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 58 filtered out; finished in 0.00s

error: test failed, to rerun pass `-p mir2-script --test engine_tests`
RC: 101
SHA_RESTORED: 320de2b0fd3600d40bf350373eddbc08874a87c0ef81ba522f1354fc2d962084

=== M7 None 退化为 Some_empty ===
$ cargo test -p mir2-script --test engine_tests lable_is_can_jmp_none_and_some_empty_are_not_equivalent -- --exact
FILE: mir2-rs/crates/script/src/engine/mod.rs
SHA_MUTATED: f56e008b2253185c01ad99933290f2eb9c581ae3a2e07914cc155eaa7dd8bdad

---- lable_is_can_jmp_none_and_some_empty_are_not_equivalent stdout ----

thread 'lable_is_can_jmp_none_and_some_empty_are_not_equivalent' (13624) panicked at crates\script\tests\engine_tests.rs:2013:5:
None 与空 label 不等价
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace


failures:
    lable_is_can_jmp_none_and_some_empty_are_not_equivalent

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 58 filtered out; finished in 0.00s

error: test failed, to rerun pass `-p mir2-script --test engine_tests`
RC: 101
SHA_RESTORED: 6d8dcb1b1f959ced7f8e2a2ab92c9856013330185611915af83b240ac381eb1e

=== M8 P 分支 len>3 回落 -1 ===
$ cargo test -p mir2-script --test engine_tests open_yb_deal_param_boundary_falls_back_to_zero -- --exact
FILE: mir2-rs/crates/script/src/engine/vars.rs
SHA_MUTATED: 4186540d97113c4321bb4611d9bc3c512a75b0ca4bcd22d103e79e0607de656b

thread 'open_yb_deal_param_boundary_falls_back_to_zero' (22416) panicked at crates\script\tests\engine_tests.rs:2110:5:
assertion `left == right` failed: P100 ⇒ MNVal[1] == 3
  left: 5
 right: 2
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace


failures:
    open_yb_deal_param_boundary_falls_back_to_zero

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 58 filtered out; finished in 0.00s

error: test failed, to rerun pass `-p mir2-script --test engine_tests`
RC: 101
SHA_RESTORED: bd42b5732a1d5edc1ad410e1a4a1368a6fc70216b36421cfc58db54f9cec2908
```

## 3. 还原与残留

- 还原方式：改坏前把 `mod.rs` / `actions.rs` / `vars.rs` 备份到 `/e/tmp/wt-scriptpd-redcheck-bk/`，每颗牙跑完立即拷回；上表每个 `SHA_RESTORED` 均与第 0 节绿基线 sha256 **逐字节一致**。
- 绿基线复跑：8 颗牙还原后，第 0 节的 7 条目标用例全绿（`58 filtered out; 1 passed`），随后全 crate 测试、fmt、clippy、workspace 门禁另行复跑（见 patch 说明）。
- 临时物：`/e/tmp/wt-scriptpd-redcheck-bk/`、`/e/tmp/wt-scriptpd-redcheck*.sh`、`/e/tmp/wt-scriptpd-redcheck-out.txt`、`/e/tmp/wt-scriptpd-green.txt` 在证据落盘后删除；仓内无临时脚本残留。
