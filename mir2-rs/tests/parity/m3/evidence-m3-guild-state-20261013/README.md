# M3⑧-实现 4 第①片 证据包：行会状态机（`crates/world/src/guild.rs`）

卡：`openmir2-m3-guild-state`（⑧-4）。本片 = **离线 crates/world 状态机**（不含 `bins/game-svr` 派发/帧）。
base = origin/master `f4d7173fb`（本片首审 base 为 `cf3f81c35`；rework 片先 `git merge origin/master` 再重跑门禁）。
review：reviewer-5 `41d42c32`（`request_changes`，F-1）→ 本片 = **F-1 返工** + 合并 master 后重跑四门禁。

## 交付内容

- `mir2-rs/crates/world/src/guild.rs`（新）：`GuildState`（`GuildInfo` 内存态）/`GuildManager`/`GuildRank`/
  `WarGuild`/`GuildConfig`（含 O-1 三键）/`check_guild_name`/成员列表文本；照搬 oracle 的 N2/N3/N4 与
  `DelHumanObj` 的 `Compare(...)==1`、`GuildAllList` 恒空。
- `mir2-rs/crates/world/src/lib.rs`（+`pub mod guild;` + `pub use guild::{…}`，2 处共 5 行）。
- **28 条**模块内构造级用例（原 22 + F-1 新增 6）+ **4 条改坏必红**（原 2 + F-1 新增 2）。

## F-1 返工（reviewer-5 `41d42c32`，Important）

**问题**：`bo_changed`/`dw_save_tick` 被当成「`.txt` 待写」标记，与 oracle 的「`.ini` 配置脏」
（`GuildInfo.BoChanged`/`DwSaveTick`）语义混用且反向。

**oracle 口径**（锚见 `11-f1-oracle-anchors.txt`）：`BoChanged=true` **只**由 ① `.ini` 五键的
5 个 public setter（`GuildInfo.cs:21-48` → `:1090-1117`）与 ② `UpdateGuildFile()`（`:584-589`）置位；
`SaveGuildInfoFile()`（`:366-370`）**不碰**二者。

**改动（全部按 oracle）**：

| Rust | 改前 | 改后 | oracle |
| --- | --- | --- | --- |
| `GuildState::set_conf` | 不置脏 | **置 `bo_changed = true`** | `GuildConf.LoadConfig` 经 5 setter（`:21-48`/`:1090-1117`）⇒ 加载后 BoChanged=true |
| `set_guild_info` | 置脏 | 不置脏 | `:463-484` 只 `SaveGuildInfoFile()` |
| `GuildManager::add_guild` | 额外调 `update_guild_file(now_tick)` | 去掉（签名去 `now_tick`） | `GuildManager.cs:19-32` 只 `SetGuildInfo` |
| `del_ally_guild` | 置脏 | 不置脏 | `:129-142` 只 `SaveGuildInfoFile()` |
| `ally_guild` | 置脏 | 可观测 no-op | `:1007-1019` 只 `SaveGuildInfoFile()` |
| `backup_guild_file` | 调 `update_guild_file(0)` | 去掉（同族一并订正） | `:591-621` 末尾只 `SaveGuildInfoFile()` |
| `member_list_text` 截断 | `chars().count() > 5000` | `encode_utf16().count() > 5000` | `PlayObject.Operate.cs:1283` 的 `sSendMsg.Length`（UTF-16 码元） |

保持不变（经 F-1 复核与 oracle 一致）：`check_save_guild_file`（`:525-533`）、`update_guild_file`（`:584-589`）、
`add_member`/`del_member`/`add_war_guild`/`update_rank` 的置脏路径、`GuildManager::run`。

## 判据（命令 + 原始件 + 读数）

| 项 | 命令 | 原始件 | 读数 |
| --- | --- | --- | --- |
| 构造级向量 | `cargo test -p mir2-world --lib guild:: --offline` | `12-f1-rust-vectors.txt` | **28 passed / 0 failed** |
| F-1 oracle 锚 | — | `11-f1-oracle-anchors.txt` | `src/M2Server/Guild/**` 逐行 |
| 改坏必红 ①N4 末字符 | 见原始件 | `03-mutation-1-n4-last-char-RED.txt` | **FAILED rc=101**（还原则绿，源 sha 一致） |
| 改坏必红 ②DelHumanObj | 见原始件 | `04-mutation-2-del-human-obj-RED.txt` | **FAILED rc=101**（还原则绿，源 sha 一致） |
| 改坏必红 ③F-1 set_conf 去脏 | 见原始件 | `13-mutation-3-f1-set-conf-RED.txt` | **FAILED rc=101**（`f1_load_marks_ini_dirty`，还原则绿） |
| 改坏必红 ④F-1 add_guild 复脏 | 见原始件 | `14-mutation-4-f1-add-guild-RED.txt` | **FAILED rc=101**（`f1_create_guild_is_clean`，还原则绿） |
| 门禁 fmt | `cargo fmt --all -- --check` | `05-gate-fmt.txt` | rc=0 |
| 门禁 clippy | `cargo clippy --workspace --all-targets --offline -- -D warnings` | `06-gate-clippy.txt` | rc=0 |
| 门禁 test | `cargo test --workspace --offline` | `07-gate-test.txt` | **98 suites / 1074 passed / 0 failed / 21 ignored** |
| 门禁 msgcodegen | `cargo run -q -p msgcodegen --offline -- --check` | `08-gate-msgcodegen.txt` | **604 OK** |
| 边界 | `git diff --name-only f4d7173fb -- src sql MirClient protocol storage` | `09-boundary.txt` | **空** |

`CARGO_TARGET_DIR=E:/tmp/openmir2-m3cs-target`，全部 `--offline`；零起栈、零端口。
返工后源 `mir2-rs/crates/world/src/guild.rs` sha256（仓库 LF 口径）
= `94ff62f9ae19f911250ead9bfe72a7cb81b0dcff761fe3f73c89b331b80ed92f`。
（首审源 sha = `ee996f699e5928902aa41df73f110327c8d688ca0242e382a18453ced7f11d76`，见 `02-rust-vectors.txt`。）

## 照搬的 oracle 怪行为（向量锁死，见模块头）

- **N2** `AddWarGuild` 条件反置：未在战 ⇒ 不生效（返回 `default`）；已在战 ⇒ 追加重复。
  向量 `add_war_guild_n2_not_in_war_is_noop` / `add_war_guild_n2_already_at_war_duplicates`。
- **N3** `UpdateRank` 成功路径把 `RankList` 置空。向量 `update_rank_success_clears_ranks_n3`。
- **N4** `CheckGuildName` 漏比末字符（`i < Length-1`）。向量 `check_guild_name_rejects_forbidden_chars_except_last`。
- **附** `DelHumanObj` 的 `Compare(...)==1`：等值/小于不动、大于才清位（置空名占位）。
  向量 `del_human_obj_compare_equals_one_quirk`。
- `GuildAllList` 恒空（`AllyGuild` 的 `Add` 被注释）。向量 `ally_guild_is_not_registered`。

## 已知建模限制（登记，非阻塞）

1. `UpdateRank` 的「未变化 ⇒ -1」判定：oracle `GuildMember` 是含 `PlayObject` 引用的 record struct，
   比较含引用相等；本片 `GuildRank.members` 只存名，故按**名相等**判「未变化」。当同成员的在线状态在
   新旧表间发生变化时，oracle 不会命中 `-1` 早退（会继续走到 `-6/0`），本片会命中 `-1`。该差异只在
   「旧表有在线态快照且发生变化」时可见；第②片接入真实会话引用后回核。
2. 落盘 IO 未在本片做：`from_record`/`to_body` 与 ⑧-3 的 `mir2-data::guild`（解析/序列化）逐字段对接；
   `bo_changed`/`dw_save_tick` **按 oracle 语义**只作「`.ini` 配置脏」状态（F-1 返工后：加载 ⇒ `true`，
   建会/`SetGuildInfo`/联盟/备份 ⇒ `false`），落盘接线与 `GuildStore` 归第②片。
3. 面板文本 `ClientOpenGuildDlg`（`<KillGuilds>` 里 oracle 直接 `+` record struct ⇒ 会打出
   `WarGuild { Guild = M2Server.Guild.GuildInfo, WarTick = …, WarTime = … }` 字面量）未在本片做，
   属表现差异，随第②片按 oracle 字面量口径实现或登记。
4. `member_list_text` 的 5000 截断计数已改 `encode_utf16().count()`，与 oracle `sSendMsg.Length` 同口径
   （reviewer M-2 已消解）。

## 残留物清单

- `E:/tmp/openmir2-m3cs-target`（编译缓存，保留供复现）。
- `E:/tmp/m3cs-scratch/guild-state-f1/guild.rs.pre`（改坏必红用的改前副本，可删；不入库）。
