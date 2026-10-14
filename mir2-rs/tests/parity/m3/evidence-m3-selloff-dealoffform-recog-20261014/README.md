# M3⑩-1 收口片：`SM_SENDDEALOFFFORM` 头字段口径（Recog=商人 ActorId / Param=0）

- 卡片：`openmir2-m3-selloff-consignment-world`（⑩-1，作者 `openmir2-worker-20`）。
- 分支：`worktree-m3-selloff-dealoffform-recog`（worktree `m3hp`，principal `openmir2-worker-20`）。
- base = `a4058a19a`（= origin/master = walgit/master）；代码提交 = `65d0b34f2`；
  **patch head = 本证据提交**（`git log -1 --format=%H -- mir2-rs/tests/parity/m3/evidence-m3-selloff-dealoffform-recog-20261014`）。
- 写集：`mir2-rs/bins/game-svr/src/selloff.rs`（净改动 1 个 `.rs`）。
- 边界：`git diff --name-only $BASE $HEAD -- src sql MirClient` = 空；`crates/protocol|storage` = 空（见 `20-boundary.txt`）。
- 纪律：**未起栈、未占端口**；判据全在构造级 `cargo test`。

## 0. 结论（先看这条）

⑩-1 已合入的 `open_deal_off_form`（`bins/game-svr/src/selloff.rs`）把
`SM_SENDDEALOFFFORM` 的线缆 12B 命令头**字段写反**了：

| 字段 | 旧实现（错） | oracle（对） |
| --- | --- | --- |
| `Recog`（32 位） | `0` | `nParam1 = 商人 ActorId` |
| `Param`（16 位） | `npc_id as u16`（**把 32 位 ActorId 截成 16 位**） | `nParam2 = 0` |

oracle 链（逐位推导见 `01-oracle-anchors.txt`）：
`Merchant.cs:1044 user.SendMsg(RM_SENDDEALOFFFORM, 0, ActorId, 0, 0)`
→ `PlayObject.Message.cs:2047 SendDefMessage(SM_SENDDEALOFFFORM, processMsg.nParam1, processMsg.nParam2, 0, 0, processMsg.Msg)`
→ `SendDefMessage(nRecog=nParam1, nParam=0)`。⇒ `Recog = 商人 ActorId`（32 位）、`Param = 0`。

本片把两字段归位，并补一条**线缆级**判据（`npc_id = 0x1234_5678 > u16::MAX`，钉死「32 位不得截进
16 位的 `Param`」）；改坏（换回旧口径）⇒ 两用例必红。

> 注：`open_deal_off_form` 目前**仍无生产调用点**（触发点 = merchant label `@dealybme`，本体下详），
> 所以这条修正在今日的活栈上不可观测；但它是 oracle 逐字节口径的正确形态，且是后续接线的前提
> （接线见 §6）。**不是**行为差异，是已合并实现的 oracle 对齐缺陷。

## 1. oracle 锚点（行号为冻结 `src/` 实测）

| 本模块 | oracle |
| --- | --- |
| 触发点（label `@dealybme` + `IsYbDeal`） | `src/GameSrv/Npc/Merchant.cs:930-935`；常量 `ScriptFlagConst.cs:25` = `"@dealybme"` |
| `UserSelectOpenDealOffForm` 本体 | `src/GameSrv/Npc/Merchant.cs:1038-1057` |
| 成功支发送 | `Merchant.cs:1044` = `user.SendMsg(Messages.RM_SENDDEALOFFFORM, 0, ActorId, 0, 0);` |
| `SendMsg` 签名 | `src/M2Server/Actor/BaseObject.Message.cs:34`（`wIdent, wParam, nParam1, nParam2, nParam3, sMsg`） |
| RM→SM 映射 | `src/M2Server/Player/PlayObject.Message.cs:2046-2048` |
| `SendDefMessage` 签名 | `src/M2Server/Player/PlayObject.Socket.cs:27`（`wIdent, nRecog, nParam, nTag, nSeries, sMsg`） |
| `MakeMessage` 签名 | `src/OpenMir2/Messages.cs:846`（`msg, recog, param, tag, series`） |

原始件见 [`01-oracle-anchors.txt`](01-oracle-anchors.txt)。

## 2. 判据（DoD②，构造级；改坏必红见 redcheck/README.md）

新增 `selloff::tests::open_deal_off_form_wire_header_puts_npc_id_in_recog`：
经生产编码路径 `selloff_event_body` 取线缆 body，逐字节断言 12B 命令头：
`Recog = 0x1234_5678`（完整 i32）/ `Ident = SM_SENDDEALOFFFORM(23001)` / `Param = Tag = Series = 0`。
订正既有 `open_deal_off_form_gated_by_sale_deal_and_active_sell` 的断言（`recog = 42` / `param = 0`）。

`cargo test -p game-svr --lib selloff::` ⇒ **25 passed / 0 failed**（见 `03-selloff-tests.txt`）。

## 3. 四门禁（cwd = `mir2-rs`，`CARGO_TARGET_DIR=E:/tmp/openmir2-m3hp-target`）

| 门禁 | 读数 | 日志 |
| --- | --- | --- |
| `cargo fmt --all -- --check` | rc=0 | `gates/10-fmt.txt` |
| `cargo clippy --workspace --all-targets -- -D warnings` | rc=0 | `gates/11-clippy.txt` |
| `cargo test --workspace --no-fail-fast` | **101 suites · 1201 passed · 0 failed · 22 ignored** rc=0 | `gates/13-workspace-test.txt` |
| `cargo run -p msgcodegen -- --check` | **604 条** rc=0 | `gates/12-msgcodegen.txt` |

（同基线 master `a4058a19a` 为 1200 passed；本片 +1 = 新增用例。）

## 4. 改坏必红

见 [`redcheck/README.md`](redcheck/README.md)：MUT-1 = 换回 `recog=0 / param=npc_id as u16` ⇒
两用例 rc=101 RED；还原后 rc=0 GREEN，`git status` 仅 `M selloff.rs`。

## 5. tier / 未跑判据

- **tier = L**（纯帧字段口径 + 构造级可判；不触 RNG / 时序 / 落库 / 战斗数值）。
- **真机两侧 `run_selloff_pair.ps1` + `selloff_diff.py` diff=0 未跑**（归 `openmir2-m3-mail-auction-align`
  + owner 独占窗口；本片不占窗口）⇒ 卡级真机 DoD 仍 `not_verified`，不据此收卡。

## 6. 本片未做（登记，交 owner / 后续卡）

1. **`open_deal_off_form` 的生产接线仍缺**：触发点 = `CM_MERCHANTDLGSELECT` 的 label `@dealybme`
   （oracle `Merchant.cs:930-935`）。Rust 侧同类 label（`@storage`/`@getback`/…）在
   `game_svr::shop::user_select`（`bins/game-svr`）里分派，但 `@dealybme` 未接；接线还需把
   `SellOffBook` / `SellOffState` 传进 `handle_shop_cmd`，并把失败支（`RM_MERCHANTSAY` 两文案）
   落到 merchant/NPC 层。本片只修正帧本身（一个逻辑变更）。
2. **`SaleDeal` 无 setter ⇒ 成功支当前不可达**：oracle 由脚本 `OPENYBDEAL` →
   `ActionOfOpenSaleDeal`（`ExecutionProcessingSys.cs:2329`）置 `SaleDeal=true`；Rust 侧
   `crates/script/src/handlers.rs:250` 仅登记名字、未移植该效果体 ⇒ 即使接线，今日也恒走
   `!SaleDeal` 失败支。属 `crates/script` 写集（不在本卡），另开卡。

## 7. 证据清单（SHA256 见 `SHA256SUMS.txt`，LF/git-blob 口径）

| 文件 | 内容 |
| --- | --- |
| `01-oracle-anchors.txt` | oracle 行号锚点 + 逐位字段推导 |
| `02-change-summary.txt` | base/head、净改动、源码 diff |
| `03-selloff-tests.txt` | 构造级测试读数 |
| `20-boundary.txt` | 冻结面零改动 |
| `gates/10..13-*.txt` | 四门禁原始件 |
| `redcheck/` | MUT-1 变异红件 + 还原绿件 + README |
| `README.md` | 本文件 |
