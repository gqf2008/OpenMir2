# GM 登记表重建 + parser 修复（卡 `openmir2-gm-registry-rebuild`，tier=L）

日期：2026-10-31　基线：`98979d829`（walgit/master，含 F 族 `setflag/setopen/setunit` 合并）　分支：`card/gm-registry-rebuild-dev1`
授权：coordinator 签名 `76afb52e`（forbidden 豁免：本卡改 `gmcmd.rs` 两条 `name` 回 oracle 活名）；
返工：coordinator 签名 `294d8b59`（rebase 到 `98979d829` 后重跑工具 `build`，禁手改）。

## 结论

`coverage/gm_registry.py` 的既有 RED 不是三因子，而是**四因子**；其中第四因子（源码命令名漂移）
由卡 `openmir2-gm-command-name-typos`（`f40303917`）引入。全部修复后：

```
build   rc=0   WROTE gm_registry.json (97262 bytes)
gate    rc=0   GREEN: gm_registry 24 条与源码逐字段一致
selftest rc=0  OK (17 teeth; skipped=K,N)
table_total=185  imp_breakdown={"Implemented": 161, "Unimplemented": 24}
third_state=21   complete=140   crosscheck={"ok": true, rust_total/ model_total=185}
```

> **F4 订正（review m4）**：摘要块原写 `99269 bytes`，与本节下方重跑证据不一致；在卡合并点
> `53973b7e9` 上重跑 `gm_registry.py build` 得 `WROTE ... (97262 bytes)`，产物 sha256
> `aa3583e205a11f369cf6c06d5413bb9afdfb1f5991a628ca615eddeaef8f44f5` 与提交的 `gm_registry.json`
> 相同（逐字节可复现）⇒ 实跑值为 **97262 bytes**。

> 卡片原来写的「源码 156/27」是**旧 parser 的视角**：它漏掉的两条恰是 `Implemented`，
> 所以旧口径少算 2 条已实现。在 `1115bd1ef` 上修好 parser 的真值是 **158/27**；
> F 族（`98979d829`）把 `setflag/setopen/setunit` 改为 Implemented 后，现态真值是
> **161/24**（185 = 161 + 24）。返工后的登记表即按 161/24 生成。

## 因子 ①：parser 漏两表项（真 bug，已修）

- 根因：`CMD_DEF_RE` 要求 `CommandDef {` 后**紧跟** `name:`。`gmcmd.rs` 有两条表项在 `{` 与
  `name:` 之间夹了 oracle 出处注释（`ChangeMasterName` / `ShowDenyChrNameLogon`），旧正则静默漏掉，
  于是 `table_total=183`。
- 修复：`gm_registry.py` 新增 `_RUST_GAP = r"(?:\s|//[^\n]*(?:\n|$)|/\*.*?\*/)*"`，表项各字段之间
  允许 `//` / `///` / `/* */` 注释；注释是合法源码，不得改变计数。
- 证据：`parse_rust()` = 185；`CommandDef {` 出现 186 次 = 185 表项 + 1 次 `pub struct CommandDef`
  （结构体带 `pub name:`，不会被匹配）。

## 因子 ②：登记表陈旧（已按现态重建）

- committed 登记表仍是 2026-10-24 的 `111/74`；源码现态（基线 `98979d829`）`161/24`。
- **没有手写任何推导值**：用工具自己的 `build` 重新生成 `gm_registry.json`（utf-8 + LF + indent=1，
  可逐字节复现）。行数 74 → 24，`sources.rust_table.sha256` 更新为
  `3a351f48350307d7d8e41e3e0f9859d61c4001836dda71f94b8defc9f24c2690`（= 当前 `gmcmd.rs`）。
- 返工时相对 rebase 前的登记表 diff 仅 7+/70-：移除的正是 `setflag` / `setopen` / `setunit` 三行
  （F 族已实现），无新增行；与 reviewer-20 的 scratch 复现一致。

## 因子 ③：ClearItemMap / ContestPoint / EndContest 三 drift（定性：位置错配，不是字段真变）

- 现象里的「登记值」（`DisableSendMsgList` / `DisableSendMsg` / `EnableSendMsg`）不属于这三条命令；
  它们分别属于旧登记表第 0/1/2 行（`+-jy` / `+jy` / `-jy`）。旧表 74 行、新表 24 行，`_row_diff`
  按位置 `zip`，把新表三项与旧表前三行错配 ⇒ 伪 drift。
- 逐条按 `cmd` 对齐核，旧登记行本身**已经正确**（handler/field/perm 与源码一致）：

| cmd | 旧表行 | handler（旧=新=源码） | field / field_line |
|---|---|---|---|
| ClearItemMap | idx 4 | `ClearItemMapCommand.cs:6` | `-` / `-`（`cs[Command] 属性`） |
| ContestPoint | idx 5 | `ContestPointCommand.cs:10` | `ContestPoint` / 238 |
| EndContest | idx 8 | `EndContestCommand.cs:8` | `EndContest` / 242 |

- 源码核对：三条类的 `[Command(...)]` 分别在 `ClearItemMapCommand.cs:6`、`ContestPointCommand.cs:10`、
  `EndContestCommand.cs:8`；`Execute` 行号 10/14/12 与登记一致。
- 结论：**不是源码字段真变，也不是登记值错**，而是「陈旧登记表 + 按位置 diff」的错配；重建后消失。

## 因子 ④（卡片三因子之外）：`gmcmd.rs` 两条命令名与 oracle 不符

- 卡片的「parser 漏了 MasterName/ShowDenyCharNameLogon」并不准确：Rust 表那两条当时叫
  `ChangeMasterName` / `ShowDenyChrNameLogon`。只修 parser 时 crosscheck 仍红：

```
missing_in_model=["ChangeMasterName","ShowDenyChrNameLogon"]
extra_in_model  =["MasterName","ShowDenyCharNameLogon"]
```

- oracle 活名是 **`MasterName` / `ShowDenyCharNameLogon`**，证据链（三重独立）：
  1. `CommandSystem.cs:100-107`：命中 custom 后 `commandAttribute.Name = customCommand.CmdName`，
     最终名取 `GameCmd.CmdName`，**不是**类 `[Command]` 属性名；
  2. `GameCommands.cs:528` / `:541` ctor 默认 = `MasterName` / `ShowDenyCharNameLogon`；
  3. `GameCmdConf.cs:313/339` 读 `[Command] ChangeMasterName` / `ShowDenyChrNameLogon`，部署
     `E:/MirServer/M2GameSvr/Command.conf` 均无此键 ⇒ `ReadWriteString` 回落 ctor 默认
     （`ConfigFile.cs:105-112`）；设计件 `design-gm-commands-20261023.md` §3.3 明写
     「现网拼写是 `@MasterName`，不是 `@ChangeMasterName`」，行 86 表同样记 `MasterName`。
- 引入源：`f40303917`（卡 `openmir2-gm-command-name-typos`）把原本正确的两条改成了
  「类属性名」拼写，并同时加注释 ⇒ 才让旧 parser 漏项。该卡把 `ReadWriteString` 的回退值误读成
  类 `[Command]` 默认名，实际是 `GameCmd` ctor 默认名。
- 修复（严格限授权范围）：`gmcmd.rs` 两条 `name` 回 oracle 活名 + 其注释 + `CHANGE_MASTER_NAME_HELP`
  帮助文本 + 4 条名字测试 + `lib.rs` 2 处测试夹具；`Imp` 变体名/类名/函数名不动。

## 牙（变异）与实跑

- **O（parser 少一项 ⇒ gate 红）**：selftest 内经真实 `build` 路径丢掉 `MasterName` 一项，
  断言 `crosscheck.ok=False` 且 gate 诊断点名 `MasterName`。`[ok]`。
- **Q（把两条 name 改回类属性名 ⇒ crosscheck 红）**：selftest 内把表项名改回
  `ChangeMasterName` / `ShowDenyChrNameLogon`，断言 crosscheck 红并点名 missing/extra 四个名字。`[ok]`。
- **真盘变异**（已还原，返工后在 `98979d829` 树上复跑）：把 `gmcmd.rs` 两条 `name` 改回旧拼写 ⇒
  `gate rc=1, 取数口径自校验失败 {missing_in_model:[ChangeMasterName,ShowDenyChrNameLogon], extra_in_model:[MasterName,ShowDenyCharNameLogon], rust_total:185, model_total:185}`；
  还原后 `sha256(gmcmd.rs)=3a351f48350307d7d8e41e3e0f9859d61c4001836dda71f94b8defc9f24c2690`，无残留。
- 门禁实跑（本 worktree，基线 `98979d829`）：
  - `python -I -B mir2-rs/tests/parity/coverage/gm_registry.py build` rc=0（WROTE 97262 bytes，可逐字节复现）
  - `gate` rc=0（GREEN 24 行）；`selftest` rc=0（17 牙，K/N 显式 skip；registry sha256 `aa3583e2…`）
  - `cargo fmt --all -- --check` rc=0
  - `cargo clippy -p game-svr --all-targets -- -D warnings` rc=0
  - `cargo test -p game-svr --lib` ⇒ **1630 passed / 0 failed / 4 ignored**
  - `cargo run -q -p msgcodegen -- --check` rc=0（604 条常量与 C# 源一致）
  - `git diff --check 98979d829 HEAD` rc=0

## 返工 F1（时序：base 早于 F 族合并）

- reviewer-20 在 `98979d829 + 2044a8b67` 合并树上跑 gate 得 RED：登记表 `158/27` vs 源码 `161/24`。
  根因是 patch base `1115bd1ef` 早于 F 族 `98979d829`，非 patch 逻辑错误；单独树 `2044a8b67`
  gate GREEN 且 build 可复现。
- 处置：`git rebase 98979d829` 后**重跑工具 `build`**（禁手改），登记表变为 `161/24`（24 行）。
  相对 rebase 前 diff 仅 `7+/70-`，移除行恰为 `setflag` / `setopen` / `setunit`，无新增行。
  新 head 与 update patch 见卡线程。
- F2（Minor，预存口径）：`CMD_DEF_RE` 对被 `//` 或 `/* */` 注释掉的 `CommandDef` 仍会计数；
  字段乱序/缺字段/夹其它 token 三类应拒样例均正确拒绝。现源无被注释表项（185 = 185 表行 + 1 次
  `pub struct` 不匹配），无当前影响；登记为残余，后续可加去注释预处理。
- F3（卡面笔误订正）：卡 issue body 的 `tools/hygiene/coverage/` 与 `tools/hygiene/gm_registry.py`
  在仓内不存在；真实 gate 位于 **`mir2-rs/tests/parity/coverage/gm_registry.py`**（登记表同目录
  `gm_registry.json`，README 同目录）。本文件与 patch/status 交付说明按真实路径书写。

## F4 订正（第二轮）：`sources.rust_table.sha256` 口径（卡 `openmir2-gm-residual-round2`）

**背景（残余登记，非阻塞）**：本文件上文与 `docs/gm-implementation-report-20261031.md` §6 F4 条
（`report:119`）及 `openmir2-gm-residual-teeth` 复审的 F4 Info，把「registry 在 HEAD 上不可复现」
登记为残余。实测（`walgit/master 38439f539`）：

| 项 | 值 |
|---|---|
| 已提交 `gm_registry.json` | **86974 bytes**（`sources.rust_table.sha256 = 66579bc9…`） |
| HEAD 重跑 `gm_registry.py build` | **86974 bytes**，且**只差该 sha 一行**：`66579bc9… → c929f62d…`（= 残余牙批次给 `gmcmd.rs` 加测试行后的整文件 sha） |
| `gate` | 同一 HEAD 上 **GREEN**（gate 不校验 `sources.*`，判据是逐字段复算） |

> 订正一处旧数：上文 §结论/F4 写的 `97262 bytes` 是**基线 `98979d829`（161/24）时的历史值**；
> 现态表是 176/9，产物为 86974 bytes。「HEAD 重跑 86974 vs 仓库 97262」的表述把历史值当成了仓库现值。

**处置 = 方案 A（改变指纹语义，不改 gate）**。`sources.rust_table.sha256` 现为
**`COMMANDS` 表被解析后的规范化 payload** 的 sha256（`[{name,perm_min,params,imp}…]` 以
`sort_keys` + 紧凑分隔符序列化），并加 `sha256_scope: "parsed_table_normalized"` 显式标注。
动机：`gmcmd.rs` 同时承载命令表与大量 `#[cfg(test)]` 用例；整文件指纹会把纯测试变动算进
provenance，使已提交登记表在 HEAD 上「不可复现」，而该 sha 本就不参与门禁判定。

**证据（本 worktree，`gmcmd.rs` 已含本卡新增的顺序牙）**：

- `gate` rc=0：`GREEN: gm_registry 9 条与源码逐字段一致`。
- 同输入两次 `build` 逐字节一致：`sha256(gm_registry.json) = 42b1c123…`（两次相同）。
- **有牙**：把真实表项 `+-jid` 的 `perm_min 10 → 11` ⇒ 表指纹 `ab565da4… → 68ba9d94…`；
  还原后 `sha256(gmcmd.rs)=7e6a9f12…` 相等、`sha256(gm_registry.json)=42b1c123…` 相等，无残留。
- **测试行不污染 provenance**：`git show HEAD:…/gmcmd.rs`（raw sha `c929f62d…`，无本卡测试）
  与当前工作树（raw sha `7e6a9f12…`，含本卡测试）解析出的表 payload 相同 ⇒ 表指纹同为
  `ab565da4…`；即「只加测试行」不再改变已提交登记表的可复现性。

**gate 为何不校验该 sha**：gate 的合同是「登记行与 oracle/Rust 源码逐字段复算一致」（见 §3 与
`check_registry`），`sources` 是归档溯源信息。把整文件 sha 纳入 gate 会把「测试行」耦合进门禁，
正是本卡要消除的失效模式；纳入表 payload sha 则与 gate 已有的逐字段复算重复。故 gate 维持不校验，
仅由本文与产物字段声明语义。**未放宽任何既有判红**（gate 代码零改动）。

## 残留物清单

- 无新增临时文件；变异备份仅在分析期使用，已还原并校验 sha256，不入库。
- 未改 `docs/delivery-report-20261025-final.md` 等历史报告（其记载的旧改名结论已被本卡订正，
  以本文件与卡线程为准）。
- 未起服务端、未占端口、未碰 `E:/MirServer` 生产栈（只读 `Command.conf`）。
