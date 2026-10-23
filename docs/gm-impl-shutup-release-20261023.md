# GM `@取消禁言`（`ShutupReleaseCommand`）实现（T-212）

- 卡：`openmir2-gm-impl-shutup-release`（tier=R；父卡 `openmir2-gm-reachability-registry` 的 `script-reachable` 项）
- 分支：`card/openmir2-gm-impl-shutup-release`，base master `619de1ce9`
- 作者：`openmir2-worker-24`；worktree `OpenMir2-wt-cronclaim`

## 1. oracle 逐行（`src/Modules/GameCommand/Commands/ShutupReleaseCommand.cs:12-49`）

| 行 | 代码 | 语义 |
| --- | --- | --- |
| `:14` | `if (@params == null) return;` | **死分支**：`GameCommand.Handle` 的 `case 2` 传 `parameters.Split(' ')`（不返回 null）；只有 `default:` 支传 null，而本命令 `Execute` 是两参签名 |
| `:18` | `sHumanName = @params.Length > 0 ? @params[0] : ""` | 名字 |
| `:19` | `boAll = @params.Length > 1 ? bool.Parse(@params[1]) : false` | **先求值**；第二段不是 `true`/`false`（空串、`1`、`yes`…）⇒ `FormatException` 冒泡出命令体、被 `PlayObject.Chat.cs:402-406` 的整段 `catch` 吞掉 ⇒ **零帧** |
| `:20` | `if (string.IsNullOrEmpty(sHumanName) \|\| !string.IsNullOrEmpty(sHumanName))` | **`A \|\| !A` 恒真** |
| `:22-23` | `SysMsg(Command.CommandHelp, Red, Hint); return;` | 唯一可观测帧 |
| `:25-48` | `Enter/LeaveCriticalSection(DenySayMsgList)`，其中 `:28-43` 的名单 `ContainsKey`/`Remove`/两处 `SysMsg` **整段被注释** | 不解禁、不通知目标、不落库；`boAll` 只被注释代码引用 |

`Command.CommandHelp` = `命令格式: @{Name} {Help}`（`CommandAttribute.cs:56-58`）；`Name` 被部署
`Command.conf` 的 `[Command] ReleaseShutup=取消禁言` 覆盖，`Help` = `CommandHelp.cs:41` 的「人物名称」
⇒ 逐字节 **`命令格式: @取消禁言 人物名称`**，色词 Red/Hint（部署 `RedMsgFColor/RedMsgBColor` ⇒ `(0xFF,0x38)`）。

> 结论：online 行为 = 「**永远一条帮助文本**」。`取消禁言 <名> true` 与 `<名> false` 同帧；
> `<名> <非布尔>` ⇒ 零帧。按 §1 硬约束③ **BUG-REPLICA，不修**。

## 2. 实现（`bins/game-svr/src/gmcmd.rs`）

- `Imp::ShutupRelease` + 派发 `Imp::ShutupRelease => shutup_release(parameters)`；
- `取消禁言` 表项 `Imp::Unimplemented → Imp::ShutupRelease`（`perm_min 10` = `[Permission] ReleaseShutup=10`，`params 2`）；
- `shutup_release()`：`split_params`（`Split(' ')` 保留空段）→ 第二段存在时 `parse_csharp_bool`（`Trim` 后只认 `true`/`false`，大小写不敏感）失败即 `no_frames(NOTE_SHUTUP_RELEASE_BOOL_PARSE)` → 否则 `red(SHUTUP_RELEASE_HELP)`；
- 常量 `SHUTUP_RELEASE_HELP = "命令格式: @取消禁言 人物名称"`；
- `gm_registry.json` 重算：`Implemented 98 → 99`、`Unimplemented 87 → 86`，`script_reachable_but_implemented` 新增 3 条 `比武-GA2.txt:43/68/106`；既有计数用例同步更新。

## 3. 用例（5）与牙（2）

用例（`gmcmd.rs::tests`）：

1. `shutup_release_is_wired_in_the_command_table`：imp / perm 10 / params 2 / 帮助文本**字面量**。
2. `shutup_release_always_returns_the_same_help_text`：无参、有名字、`true`、`false`、制表符不分段
   —— 5 种写法都恰好一条 `(Red, "命令格式: @取消禁言 人物名称")`，无其它帧、无登记。
3. `shutup_release_bool_parse_failure_is_zero_frames`：`yes`/`1`/`TRUE2`/双空格空段 ⇒ 零帧 +
   `NOTE_SHUTUP_RELEASE_BOOL_PARSE` 登记；对照 `\ttrue`（`Trim` 后合法）⇒ 帮助文本。
4. `shutup_release_permission_gate_blocks_body`：perm 9 ⇒ `权限不够!!!`；perm 10 ⇒ 帮助文本。
5. `shutup_release_script_gmexecute_path_through_npc_player_seam`：生产跳线（`NpcPlayer`）
   `@取消禁言 新人` ⇒ 恰好一条帮助帧 `(0xFF,0x38)`；`@取消禁言 新人 1` ⇒ 零帧 + 登记。

牙（实做并还原，`sha256` 一致：`gmcmd.rs` `778020f5f173ad26…`）：

| 牙 | 改动 | 结果 |
| --- | --- | --- |
| 1 | 删 `bool.Parse` 守卫 | 用例 3、5 红（得帮助文本而非零帧） |
| 2 | 帮助文本改成现网描述名 `恢复禁言` | 用例 1（以及改成字面量比对的用例 2）红 |

## 4. 门禁（本 head）

```
cargo fmt --all -- --check                              OK
cargo clippy --workspace --all-targets -- -D warnings   OK
cargo test --workspace                                  141 个测试二进制 / 3101 passed / 0 FAILED（rc=0；第二次合并 master 后重跑）
cargo run -p msgcodegen -- --check                      604 条一致
whitelist_anchor_gate.py check                          507 条锚 ALL PASS（含 T-212）
m3_whitelist_registry_audit.py audit                    GREEN
t_reservation_gate.py check                             GREEN
gm_registry.py build && gate && selftest                GREEN（14 teeth）
```

**环境噪声记录**（按队列纪律 #11 的三态判定）：
第 1 次 `cargo test --workspace` rc=101，红在 `game-gate --test relay::oversize_frame_is_dropped_without_kick`
（期望 1 条命令帧、实得 0）；**单跑该用例 rc=0**，且本卡只改 `gm-svr` 的 `gmcmd.rs` ⇒ 判为并发/时序噪声。
第 2 次 rc=101 是**真回归**：既有计数用例 `batch2_table_wires_thirty_command_bodies` 仍按 98 条断言，
已随本卡更新为 99/86。第 3 次 rc=0、141 个测试二进制 0 FAILED。

## 5. 残余 / 窗口项（R，未跑）

- 真机双栈窗口未跑：部署脚本 `Market_Def/功能NPC/比武-GA2.txt:43`（`@aa ← @main`）经
  `GMEXECUTE 取消禁言 self` 触发 ⇒ 两侧各恰好一条 `SM_SYSMESSAGE(100)`、正文
  `命令格式: @取消禁言 人物名称`、色词 `MakeWord(255,56)`；对照 `取消禁言 <名> 1` ⇒ 两侧零帧；
  名单后态两侧均无变化。窗口未跑前本卡不得 `closed`（T-212）。
- 前置卡 `openmir2-gm-script-exec-seams` **已合入 master `7bbb5ea7`**（`c0a55368f` 现亦为 master 祖先）
  ⇒ 脚本路 `GMEXECUTE` 抬权桥在位、窗口可复跑；未跑只是 R 窗口未排期。
- **F1（review `01007b5ee8`）已订正 · 再订正（Leader 复核）**：T-212 窗口项现为
  「前置已齐、待排期」——前置卡已合入 master `7bbb5ea7`；真实剩余 = R 窗口未排期 + 部署无 perm=10
  账号（`Envir/adminlist.txt` 唯一行缺 IP 段被 `LocalDB.cs:80-82` 整行 `continue`，`StartPermission=0`）
  ⇒ 唯一能打到命令体的是脚本路 `GMEXECUTE`。部署落点（Leader 独立核过）：`Merchant.txt:137` →
  `Market_Def/功能NPC/比武-GA2.txt` `[@main]`:2 → `[@AA]`:39 三段 `#if CheckJob` 的 `#act` 首行
  `gmexecute 取消禁言 self`（`CM_CLICKNPC` 同图 + `|dx|,|dy| ≤ 15`）。

## 5b. 复审返工（review `01007b5ee8`）

| 编号 | 级别 | 处置 |
|---|---|---|
| F1 | important | T-212 窗口项改「当前不可复跑」+ 写明两处前置（前置卡未合入、AdminList 无 IP 段 ⇒ 无 perm=10 玩家）与放行条件（`mir2-rs/tests/parity/whitelist.md`，本节上条同源）。 |
| F2 | minor | `gm_registry.py build` 重算源码指纹：`gm_registry.json` 只改 1 行 `sources.rust_table.sha256`（`769de74093cc…` → 当前 `gmcmd.rs` 实测值），`gate`/`selftest` 仍 GREEN（14 teeth）。 |
| F3 | minor | 新增 `shutup_release_help_is_gbk_bytes_on_the_wire`：对 `命令格式: @取消禁言 人物名称` 做**线上 GBK 逐字节**断言（`crate::downlink::gbk`，28 字节；UTF-8 相同不能替代）。牙：改常量字 ⇒ 本用例红。 |

## 5c. 合并 master（`75b4a4d79`）

- master 已并入 `ShowEffect`/`StartQuest` 等批 5 卡；`gmcmd.rs` 冲突以 **master 为基底重放本卡增量**解决
  （常量 / 枚举 / 派发臂 / `shutup_release`+`parse_csharp_bool` / 表项 / 6 用例），并删除 master 里
  同名的「`取消禁言` → `Imp::Unimplemented`」旧表项（否则表 186 行、`find` 命中旧项 ⇒ 零帧）。
- 计数：`Implemented 101 / Unimplemented 84`（master 100 + 本卡 1）；`gm_registry.py build` 重新生成指纹。
- `whitelist.md` / `T-RESERVATIONS.md`：保留 master 全部行 + 重插 T-212 行（含 F1 订正）。

## 5d. 第二次合并 + 工具适配（review 后的 master 前进）

- master 再并入 `gm-integration`（kick/mobfireburn/smake/clearbag 返工）⇒ 同法重放本卡增量：
  `Implemented 105 / Unimplemented 80`，`gm_registry.py build` 重新生成登记表。
- **工具适配（跨卡写集，已在卡上给 Leader 留 comment）**：本卡把 `取消禁言` 实现后，
  `commands` 里再无「未实现的 script-reachable 行」⇒ `gm_registry.py selftest` 的牙 K/N
  `next(...)` 抛 StopIteration。改为 `next(..., None)` + **显式 `[skip]` 打印**（不静默放行），
  结尾 OK 行标注 `skipped=K,N`；在 clean master 检出上验证 14 颗牙仍全 ok、无 skip。

## 6. 残留物清单

- `E:/tmp/w24-sr.sha`（牙的 sha256 基线）、`E:/tmp/w24-sr-tests{,2,3}.txt`（三次全量测试输出）。
