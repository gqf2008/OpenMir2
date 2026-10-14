# evidence-m3-party-recall-ext-20261014

卡 `openmir2-m3-party-recall-ext`（M3⑦ 扩展面：组队脚本/命令面，`openmir2-worker-21`，车道 `m3pt`）。
本文件描述**返工后**（reviewer-5 `request_changes` F-1 收口）的交付；末节列返工要点。

全部命令在**本 worktree**（`E:/Users/gxh/Documents/GitHub/OpenMir2/.claude/worktrees/m3pt`）内、
`CARGO_TARGET_DIR=E:/tmp/openmir2-m3pt-target`（检出外）。**未起栈、未占端口、未 merge、未自签 review**。
base = `refs/heads/master`（返工时已把 `origin/master`（= `6dba8719d`）并入分支对齐 merge-base；
原交付 base 为 `083dc1d6d`）。交付 head = `refs/heads/worktree-m3-party-recall-ext` 的 tip。

## 范围

盘点报告 `M3⑦组队交易现状盘点报告-20261013.md` §1.O/§2#18 列的 5 个脚本面：

| oracle | Rust 落点 |
| --- | --- |
| `RECALLGROUPMEMBERS`（`ExecutionProcessingSys.cs:3487`，**空方法体**） | `crates/script/src/engine/actions.rs` → `ActionOfRecallGroupMembers`（no-op） |
| `GROUPRECALL`（`ExecutionProcessingSys.cs:3918`，只剩被注释的一行） | `actions.rs` → `ActionOfGroupRecall`（读 `GroupOwner`、无副作用） |
| `CHECKGROUPCOUNT`（`ConditionProcessingSys.cs:2670`，switch 被注释） | `conditions.rs` → `cond_check_group_count`（恒假 + `GetVarValue` 错误路径） |
| `CHECKGROUPCLASS`（`ConditionProcessingSys.cs:262`） | `conditions.rs` → `cond_check_group_class`（按成员职业计数） |
| `ISGROUPMASTER`（`ConditionProcessingSys.cs:4466`，只写 false） | `conditions.rs` → `cond_is_group_master`（团长保持初值 true） |

宿主读侧（新增）：`ScriptPlayer::group_owner/group_member_jobs`（`crates/script/src/engine/mod.rs`
默认 + `bins/game-svr/src/npc.rs` 适配）↔ `World::party_script_group_snapshot`（`crates/world/src/party.rs`）。

**不在本卡范围**：`@GroupRecall` GameCommand（`src/Modules/GameCommand/Commands/GroupRecallCommand.cs:12-66`）——
本仓**没有命令（聊天 `@xxx`）派发框架**，无法在不动新框架的前提下 1:1 落地；已在卡上登记为待承接项。

## tier

**L**。判据全部是纯逻辑分支 + 宿主快照读数：**不触** RNG 抽数序列 / 时序 / 落库 / 战斗数值
（`GetVarValue` 的读值在本卡只决定「是否报脚本条件错误」，不产生随机数）。⇒ 不占真机窗口，
`window_required=false`；不需要 owner `-Go`。

## 证据文件

| 文件 | 内容 |
| --- | --- |
| `00-commands.txt` | 复现命令清单 |
| `01-oracle-anchors.txt` | `src/` 实测行号 + 4 处缺陷本体逐字 |
| `02-rust-status-greps.txt` | 改前（origin/master：engine 无分支/无登记）↔ 改后（分支 + 登记 + 宿主读侧）+ 返工输入改动 |
| `03-gate-fmt.txt` | `cargo fmt --all -- --check` rc=0 |
| `04-gate-clippy.txt` | `cargo clippy --workspace --all-targets -- -D warnings` rc=0 |
| `05-gate-test.txt` | `cargo test --workspace` rc=0：101 suites / 1210 passed / 0 failed / 22 ignored |
| `06-gate-msgcodegen.txt` | `cargo run -p msgcodegen -- --check` rc=0：604 条常量 |
| `07-mutations-red.txt` | 改坏必红 4 条（逐条 FAILED + 还原后全绿） |
| `08-boundary.txt` | 冻结面 `src sql MirClient` = 0 + 全量改动清单 |

## 判据（改坏必红，逐条）

1. `engine_tests::is_group_master_true_only_for_leader_like_oracle` —— MUT1（退化为恒假）必红。
2. `engine_tests::check_group_class_counts_members_by_job` —— MUT2（不看成员职业）必红。
3. `engine_tests::check_group_count_always_false_like_oracle` —— MUT3（实现成真比较）必红。
4. `engine_tests::group_recall_actions_are_noops_like_oracle` —— MUT4（no-op 改成发帧）必红。
5. `party_vectors::script_group_snapshot_reads_owner_and_member_jobs` —— 宿主快照口径。

## 口径 / 已知空缺

- oracle 的 4 处缺陷**1:1 照搬**（`switch` 被注释 ⇒ `CHECKGROUPCOUNT` 恒假；`ISGROUPMASTER` 只写
  `false` ⇒ 唯一为真路径 = 团长；`if (nJob < 0)` 因 `PlayerJob.None = 3` 恒假 ⇒ 未识别职业不报错；
  两个召回动作是空体/注释体 no-op）。登记 **whitelist T-52**；将来要修另开 `S-*`。
- `GetVarValue` 的 `ref nValue` 语义：本仓 `var_info_of` 原以 `n_value = 0` 起，本卡抽出
  `var_info_of_in(s_data, n_in)` 并把 `n_in` 透传（旧调用点传 0，行为不变）；`CHECKGROUPCOUNT`
  传 `-1`（= oracle 的 `HUtil32.StrToInt(sParam2, -1)`）。判据用 `<$STR(Q999)>`（`GetValNameNo`
  不命中且非 `S`/`N` 首字母 ⇒ oracle 保持 ref 初值 -1 ⇒ 报错）与普通非数字串（oracle 尾支
  `nValue = 0` ⇒ 不报错）的分野（用例 3）。
- **已知空缺（whitelist T-53）**：oracle `GetValNameValue` 对未知名还有 per-player `S`/`N` 回退
  （`m_StringList`/`IntegerList`），本仓未建模 ⇒ `<$STR(N…非数字)>` 上 oracle 写 `nValue=0`、Rust
  保持 ref 初值。本卡不扩写集实现该通用变量面（见返工节 F-1）。
- 真机两侧对拍**不在本卡**（本卡 5 个脚本面无帧、无副作用），故不登记 `not_verified` 的真机项。
- `@GroupRecall` 命令面待承接（见「范围」）。

## 返工（reviewer-5 `request_changes` F-1，2026-10-14）

- **F-1（Important）**：原用例把 `<$STR(NOTAVAR)>` 断成 `ScriptConditionError`，但 oracle 对首字母
  `N` 的未知名走 `ProcessingBase.cs:1420-1432` per-player `IntegerList` 回退（`nValue = 0`）**不报错**
  ⇒ 原 `..._like_oracle` 把差异锁死。**收口 = reviewer 给的 (b) 最小修**（不扩写集去实现 per-player
  变量表）：判据输入改 `<$STR(Q999)>`（oracle 亦保持 -1 ⇒ 报错），并把「`S`/`N` per-player 回退未实现」
  按 **T-53** 登记（该缺口是本卡之前就存在的通用变量解析面）。用例注释已写明。
- **O-1（合并前置）**：分支补推 `origin`（原先只在 `walgit`）。
- **O-2（撞号）**：本卡 whitelist 条目由 `T-51` 改 **`T-52`**（`T-51` 已被并发的 clone46 占用并合入
  master）；新增 `T-53` 并写明连号。
- **O-3（非阻塞）**：`crates/world/src/party.rs` 新 doc 的「实体表槽位序」订正为「**加入序**」。
- 返工后四门禁与 4 条改坏必红全部复跑（读数见上表）。

## 自检

```sh
python -I tools/evidence-hash/verify_sha256sums.py verify mir2-rs/tests/parity/m3/evidence-m3-party-recall-ext-20261014
python -I tools/evidence-hash/verify_sha256sums.py eolcheck mir2-rs/tests/parity/m3/evidence-m3-party-recall-ext-20261014
```
