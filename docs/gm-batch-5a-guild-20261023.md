# GM 批次 5A：行会/城堡族 6 条接线（卡 `openmir2-gm-batch-5a-guild-impl`）

- 基线：`walgit/master` = `512d9ecf5`；worktree `E:/tmp/wt-worker-69`，分支 `card/gm-batch-5a-guild`。
- tier = **L**（Leader 裁定 `9e7d2227` 的 A 片）：只动 `gmcmd.rs` 表项 + 命令体 + 用例，零生产代码、零 `src/**`。
- 父卡：`openmir2-gm-batch-5-guild-castle`（当前 `goal_gate` 条件② 唯一 FAIL 卡）；本卡承接其 A 片。

---

## 0. 结论先行

1. **A 片 = 6 条**，与裁定一致；但**生效名与权限的来源不是 `[Command]` 属性**，而是
   `GameCommands.cs` 构造 + 部署 `Command.conf`：三条被 `[Command]` 里写 **0** 的命令实际是 **10**
   （构造覆盖），其中三条还被 `Command.conf [Command]` 改成了**中文生效名**。
   `SbkDoorControlCommand` 注册在字段 `SbkDoor` 上，生效名是 **`@SbkDoor`**（不是 `@SbkDoorControl`）。
2. **4 条真 no-op**（Announcement / 允许联盟 / GuildWar / SbkDoor）；**2 条（联盟 / 取消联盟）**
   只在**非会长**时为零帧，**会长支**会进 `ClientGuildAlly` / `ClientGuildBreakAlly`（帧 + `SS_207`），
   依赖可写行会态与 G11 跨服 ⇒ **登记 T-263**，宿主只留 `log_note`（不静默、不假 no-op）。
3. **B/C 片未做**（见 §4）；父卡点名的两个怪癖（`ChangeSabukLord` 的 `bool.Parse` 缺参、
   `GuildRecall` 的 `(short)` 强转）**都在 B 片命令上，本卡零触碰**（`src/**` 冻结面也没动）。

---

## 1. A 片 6 条逐条清单（现网生效名 / oracle 出处 / 权限门 / 期望帧）

> 「权限」= **生效 `PermissionMin`**（`GameCommands.cs` 构造覆盖 + `Command.conf [Permission]`）。
> 「帧」= 部署测试角色（**无行会**）执行时的帧面。

| # | 生效名 | 代码类 | oracle `file:line` | 权限（来源） | 期望帧（无行会角色） |
|---:|---|---|---|---:|---|
| 1 | `@Announcement` | `AnnouncementCommand` | `Commands/AnnouncementCommand.cs:8`（`[Command(...,10)]`）；命令体 `:14-41` **整段被注释** | **10**（`GameCommands.cs:466`） | **零帧**（真 no-op） |
| 2 | `@联盟` | `AuthCommand` | `Commands/AuthCommand.cs:8/12-17`：`if (IsGuildMaster()) ClientGuildAlly();` | **10**（`GameCommands.cs:372` 覆盖属性里的 0） | 非会长 ⇒ **零帧**；会长 ⇒ 见 §3（T-263） |
| 3 | `@允许联盟` | `AuthallyCommand` | `Commands/AuthallyCommand.cs:5/9-23`：`if (IsGuildMaster()) { /* 翻转 EnableAuthAlly + 两条 SysMsg 全被注释 */ } return;` | **10**（`GameCommands.cs:371` 覆盖 0） | **零帧**（两条分支都无输出） |
| 4 | `@取消联盟` | `AuthCancelCommand` | `Commands/AuthCancelCommand.cs:9/12-21`：空参 ⇒ `return`；`if (IsGuildMaster()) ClientGuildBreakAlly(@params[0]);` | **10**（`GameCommands.cs:373` 覆盖 0） | 空参 / 非会长 ⇒ **零帧**；会长 ⇒ 见 §3（T-263） |
| 5 | `@GuildWar` | `GuildWarCommand` | `Commands/GuildWarCommand.cs:5/9-15`：体只有 `if (PlayerActor.Permission < 6) return;` | **10**（`[Command(...,10)]`） | **零帧**（过权限门后体必 return） |
| 6 | `@SbkDoor` | `SbkDoorControlCommand` | `Commands/SbkDoorControlCommand.cs:5/7-9`：**空体** | **10**（`GameCommands.cs:526` + `Command.conf [Permission] SbkDoor=10`） | **零帧**（真 no-op） |

**生效名的出处（容易写错的三条）**：`GameCmdConf.cs:16-18` 分别用
`Command.conf [Command]` 的 `AuthAlly=允许联盟` / `Auth=联盟` / `AuthCancel=取消联盟` 覆盖 `CmdName`
（部署件 `E:/MirServer/M2GameSvr/Command.conf:20/22/24`，GBK）；`SbkDoor` 见 `Command.conf:176`。
> **设计件 §4.3 的「帧 —」对这 6 条基本成立**，但 **联盟 / 取消联盟的会长支其实会出帧**（§3），
> 设计表的「效应 N」对该臂是**过宽**的表述——本卡按 Leader 的要求把该臂显式登记，不沿用「N」当依据。

---

## 2. 实现（`gmcmd.rs`）

- `Imp` 新增 6 个变体（`Announcement` / `Auth` / `Authally` / `AuthCancel` / `GuildWar` / `SbkDoorControl`），
  每个变体的文档注释都写了 oracle 出处 + 生效名 + 权限来源。
- **表项**：`Announcement` / `GuildWar` 由 `Unimplemented` 改为新变体；既有中文表项
  `允许联盟` / `取消联盟` / `联盟` 改为新变体；既有 `SbkDoor` 改为 `Imp::SbkDoorControl`
  （**不新增** `SbkDoorControl` 表项——那会造出一条线上不存在的命令）。
- **命令体**：
  - 4 条真 no-op 直接 `no_frames(...)`（note 带 oracle 行号）；
  - `auth(world)`：`world.is_guild_master(speaker)` ⇒ 会长支只留 note（T-263）；否则零帧；
  - `auth_cancel(parameters, world)`：**先判空参**（`AuthCancelCommand.cs:14-17`）⇒ 零帧；再判会长 ⇒ note；否则零帧。
- **`params` 列**与设计表一致：Announcement 2 / 联盟 1 / 允许联盟 1 / 取消联盟 2 / GuildWar 1 / SbkDoor 1。

---

## 3. 会长支为什么登记而不是实现（T-263）

`ClientGuildAlly`（`PlayObject.Operate.cs:1465-1523`）与 `ClientGuildBreakAlly`（`:1525-1560`）会长支需要：

1. **可写行会态**：`MyGuild.AllyGuild/DelAllyGuild`、`RefMemberName`、`SendGuildMsg`、
   `EnableAuthAlly`/`IsNotWarGuild` 判定 —— 本仓 `GmWorld` 只有 `GuildManager` 的**只读视图**
   （`member_of_guild`/`find_guild`/`get_rank_name`），没有写面。
2. **G11 跨服**：成功支两条 `SendServerGroupMsg(SS_207, …)`。
3. **帧**：`SM_GUILDMAKEALLY_OK` / `SM_GUILDMAKEALLY_FAIL(n8)`（n8 = -1..-4）/
   `SM_GUILDBREAKALLY_OK` —— 帧面的字节断言要等上述两面前置落地。

⇒ 本卡把「非会长支」做到 1:1（零帧），会长支**登记 T-263**、note 里写清 `ClientGuildAlly` /
`ClientGuildBreakAlly` 与 `T-263`，并用用例钉住「不许静默」。

---

## 4. B / C 片（未做；归属建议，未自行建 issue）

| 片 | 内容 | 建议归属 | 说明 |
|---|---|---|---|
| **B 片（6 条）** | `AddGuild` / `DelGuild` / `EndGuild` / `ForcedWallconquestWar` / `ChangeSabukLord` / `ShowSbkGold` | **`openmir2-gm-batch-5b-cross-server`**（tier=R，blocked-by = G11 跨服子系统） | 依赖 `WorldEngine.SendServerGroupMsg(SS_205/206/207)` 与会话行会可写态。父卡点名的两个怪癖都在这里：`ChangeSabukLord` 的 `bool.Parse(@params[2])` 缺参抛 `IndexOutOfRange` 被外层 catch 吞（照抄，不修）、`GuildRecall` 的 `(short)dwValue` 强转口径（`GuildRecall` 属传送/召唤族另一条）。**本卡零触碰**。 |
| **C 片（3 条）** | `StartContest` / `EndContest` / `ContestPoint` | 建议先在 `docs/rewrite-remaining-inventory-20261022.md` 记「已登记不实现」，再由 owner 决定是否开赛事子系统卡 | 仓内无赛事子系统；部署 `Envir` 里这 3 条 `GMEXECUTE` 命中 **0**（裁定 `9e7d2227` 的 C 片要求）⇒ 现网不可达。 |

---

## 5. 用例（`cargo test -p game-svr batch5a`，3 个测试）与牙

| 用例 | 断言 |
|---|---|
| `batch5a_noops_permission_gate_and_zero_frames` | 6 条 perm 10：**权限不足 ⇒ 恰好一条 `权限不够!!!` 且零帧**；权限满足（`run_gm`）⇒ 4 条真 no-op + 联盟/取消联盟 非会长/空参 ⇒ **零帧零消息**且各有 `log_note` |
| `batch5a_table_entries_are_wired` | 6 条生效名都在 `COMMANDS` 且 `imp != Unimplemented`、`perm_min == 10` |
| `batch5a_guild_master_path_is_registered_not_silent` | 会长支：零帧但 note 必须含 `ClientGuildAlly`/`ClientGuildBreakAlly` 与 `T-263` |

**牙（实跑，逐字节还原后 sha256 见 status）**：
1. 把 `允许联盟` 表项回退成 `Imp::Unimplemented` ⇒ `batch5a_table_entries_are_wired` 红；
2. 把 `no_frames` 的 note 或某条命令体的行为改坏（例如让 `授权/无参` 分支误发一条 SysMsg）⇒ 行为用例红。

---

## 6. 门禁（实跑）

| 命令 | 结果 |
|---|---|
| `python -I mir2-rs/tests/parity/coverage/gm_registry.py build` | `crosscheck.ok=true`（rust 185 == model 185、perm 0 drift）；`table_total=185`、`Implemented 110 / Unimplemented 75` |
| `python -I mir2-rs/tests/parity/coverage/gm_registry.py gate` | **GREEN（75 条与源码逐字段一致）** |
| `python -I mir2-rs/tests/parity/coverage/gm_registry.py selftest` | **OK（14 teeth）** |
| `cargo fmt --all -- --check` / `cargo clippy --workspace --all-targets -- -D warnings` | 见 status |
| `cargo test --workspace` | 见 status |
| `cargo run -p msgcodegen -- --check` | 见 status |
| `python -I tools/whitelist/t_reservation_gate.py check` | 见 status（本卡取号 T-263） |

---

## 7. 残留

1. **B/C 片**按上表归属；本卡不建 issue。
2. **父卡的角色问题**（卡面 evidence）：该卡 owner 曾记为 `openmir2-reviewer-5` —— 本卡由 **worker** 身份
   （`openmir2-worker-69`）承接 A 片，审查须由**第三方 principal** 做。
3. 会长支（T-263）待 G11 + 行会写面落地后接线；接线时按 `ClientGuildAlly`/`ClientGuildBreakAlly` 的
   帧字节逐条断言。
