# M3⑧-实现 5 第①片 证据包：行会战宣战全链（`crates/world/src/guild.rs`）

卡：`openmir2-m3-guild-war`（⑧-5）。本片 = **离线 `crates/world` 行会战编排**（NPC 层 + `PlayObject` 层）。
base = origin/master `ab60452d0`（已含 ⑧-4 合并点 `6254f95b8`）。**不含** `bins/game-svr` 的 `CM_*` 派发 / 帧 / 落盘 IO（第②片，等 owner 对 bins 串行序裁决）。

## 交付内容

- `mir2-rs/crates/world/src/guild.rs`：
  - 新增 `GuildWarEvent` / `GuildWarRequest` / `GuildWarOutcome` 三型；
  - 新增 `GuildState::ref_member_name`（`GuildInfo.cs:348-364`；仅在线成员 `RefShowName`）；
  - 新增 `GuildManager::request_guild_war`（NPC 层 `GuildOfficial.cs:180-199` + `PlayObject` 层 `PlayObject.Base.cs:3349-3385` 全链）。
  - **复用** ⑧-4 已落并已合入 master 的 `GuildState::add_war_guild`（N2 反逻辑）/ `end_guild_war` / `GuildManager::run`（无改动）。
- `mir2-rs/crates/world/src/lib.rs`：`pub use guild::{…}` 增列 `GuildWarEvent`/`GuildWarOutcome`/`GuildWarRequest`（1 行改动）。
- **11 条**新增构造级向量（guild 模块 39 passed = ⑧-4 的 28 + 本片 11）+ **4 条改坏必红**。

## 实现的 oracle 语义（逐条锚见 `01-oracle-anchors.txt`）

`request_guild_war` 按 oracle 的两层顺序执行：

1. **NPC 层**（`GuildOfficial.cs:180-199`）：
   - `FindGuild(sGuildName) == null` ⇒ `SysMsg("行会 X 不存在!!!")`，**不扣钱**；
   - `Gold < GuildWarPrice` ⇒ `SysMsg("你没有足够的金币!!!")`，**不扣钱**；
   - 否则 `DecGold(GuildWarPrice)` + `GoldChanged()`（`gold_spent = war_price`）⇒ 进入 `PlayObject.ReQuestGuildWar`。
2. **PlayObject 层**（`PlayObject.Base.cs:3349-3385`）：
   - `!IsGuildMaster()`（`MyGuild == null || GuildRankNo != 1`）⇒ `SysMsg("只有行会掌门人才能申请!!!")`；
   - `ServerIndex != 0` ⇒ `SysMsg("这个命令不能在本服务器上使用!!!")`；
   - `FindGuild == null` ⇒ `SysMsg("行会不存在!!!")`；
   - `warGuild = MyGuild.AddWarGuild(guild)`；`if (warGuild.WarTick > 0)` ⇒ `guild.AddWarGuild(MyGuild)`；
     - 返回值 `.Guild != null` ⇒ `warGuild.WarTick = 0`（**副本赋值，列表零副作用**）；
     - 否则 `boReQuestOk = true` ⇒ 双发 `SS_207`。
3. 每次 `AddWarGuild` 内**无条件** `RefMemberName()`（逐在线成员 `RefShowName`，`RM_USERNAME`）+ `UpdateGuildFile()`。

### 两道照搬的 oracle 怪行为（硬约束 3，向量锁死）

1. **先扣钱后判掌门**：`GuildOfficial.cs:186` 的 `DecGold` 先于 `PlayObject.Base.cs:3351` 的 `IsGuildMaster`
   ⇒ 非掌门 / 非 0 区 / `PlayObject` 层目标缺失也**白扣 `GuildWarPrice`**（`03/06` 两条红检与
   `request_guild_war_non_master_still_pays_gold` / `..._no_guild_...` / `..._non_zero_server_...` 三条向量）。
2. **N2 `AddWarGuild` 条件反置**（⑧-4 已实现，本片以编排层向量锁死）：
   - 双方**都未战** ⇒ `AddWarGuild` 返回 `default`（`WarTick=0`）⇒ **宣战不生效**（列表不变、无 `SendGuildMsg`、无 `SS_207`）；
   - 本会**已战**、目标未列本会（非对称态）⇒ 本会 `GuildWarList` **追加重复条目** + 两条 `SendGuildMsg`，
     反向调用返回 `default`（`.Guild == null`）⇒ `boReQuestOk = true` ⇒ 双发 `SS_207`；
   - 双方**都已战** ⇒ 两侧各追加重复；反向 `.Guild != null` ⇒ `WarTick=0`（副本）⇒ **不发 `SS_207`**。

## 判据（命令 + 原始件 + 读数）

| 项 | 命令 | 原始件 | 读数 |
| --- | --- | --- | --- |
| 构造级向量 | `cargo test -p mir2-world --lib guild:: --offline` | `02-rust-vectors.txt` | **39 passed / 0 failed**（新增 11） |
| oracle 锚 | — | `01-oracle-anchors.txt` | `src/**` 10 段逐行 |
| 改坏必红 ①掌门人门反置 | 见原始件 | `03-mutation-1-master-gate-RED.txt` | **7 FAILED rc=101**（还原后绿，源 sha 一致） |
| 改坏必红 ②`request_ok` 反置 | 见原始件 | `04-mutation-2-request-ok-RED.txt` | **3 FAILED rc=101**（还原后绿） |
| 改坏必红 ③去 `online` 过滤 | 见原始件 | `05-mutation-3-online-filter-RED.txt` | **1 FAILED rc=101**（还原后绿） |
| 改坏必红 ④`gold_spent` 恒 0 | 见原始件 | `06-mutation-4-gold-spent-RED.txt` | **5 FAILED rc=101**（还原后绿） |
| 门禁 fmt | `cargo fmt --all -- --check` | `07-gate-fmt.txt` | rc=0 |
| 门禁 clippy | `cargo clippy --workspace --all-targets --offline -- -D warnings` | `08-gate-clippy.txt` | rc=0 |
| 门禁 test | `cargo test --workspace --offline` | `09-gate-test.txt` | **98 suites / 1107 passed / 0 failed / 21 ignored** |
| 门禁 msgcodegen | `cargo run -q -p msgcodegen --offline -- --check` | `10-gate-msgcodegen.txt` | **604 OK** |
| 边界 | `git diff --name-only ab60452d0 -- src sql MirClient protocol storage bins` | `11-boundary.txt` | **空** |
| 证据 hash | `python -I tools/evidence-hash/verify_sha256sums.py --check-extra <本目录>` | `12-verify.txt` | GREEN（见文件） |

`CARGO_TARGET_DIR=E:/tmp/openmir2-m3cs-target`，全部 `--offline`；零起栈、零端口。
源 `mir2-rs/crates/world/src/guild.rs` sha256（仓库 LF 口径）
= `f27c6f8f77ec2e5a308ad0c381e50cb2b784c6df8c6b4640415cba6ddafacbea`
（改坏 4 次后还原一致，见 `02`/`03`–`06`）。

## not done（第②片 / 真机）

1. **`bins/game-svr` 接线**：`@scl_guildwar` 脚本派发 → `GuildOfficial.ReQuestGuildWar`（`DecGold`/`GoldChanged`）
   → `PlayObject.ReQuestGuildWar`；`GuildManager.Run` 逐 tick 驱动 + `SendGuildMsg`/`SS_207`/`RM_USERNAME` 落帧。
   **等 owner 对 bins 串行序裁决**（⑧-3/4/6/7/8/9 的第②片同族，作者 comment 已请求）。
2. **真机两侧同序列 diff=0**（`run_guild_castle_pair.ps1` + `GuildWarTime` 小值夹具 + 独占窗口）——归 ⑧-10 夹具 + owner 窗口。
3. `SendGuildMsg` 的 `GuildMsgPreFix` 前缀（`〖行会〗`，`GuildInfo.cs:435-438` + `ShowPreFixMsg`）属帧层，
   本片事件只带**入参原文**；第②片接线时按 oracle 前置。（`GuildMsgPreFix` 初值 `GameSvrConf.cs:1843`。）

## 残留物清单

- 中间件 `E:/tmp/m3cs-work/`：`gw-*.log`（fmt/clippy/test/msgcodegen/四改坏）、`guild.rs.bak`（改坏前备份）。
  仅供审计复核，不含凭据，可随时删。
- 编译缓存 `CARGO_TARGET_DIR=E:/tmp/openmir2-m3cs-target`（外置，未落检出内）。
- 本 worktree 无新增未跟踪目录（`git status --short` 仅两件已改源码 + 本证据目录）。
