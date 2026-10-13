# M3⑧-实现 4 第①片 证据包：行会状态机（`crates/world/src/guild.rs`）

卡：`openmir2-m3-guild-state`（⑧-4）。本片 = **离线 crates/world 状态机**（不含 `bins/game-svr` 派发/帧）。
base = origin/master `cf3f81c35`。

## 交付内容

- `mir2-rs/crates/world/src/guild.rs`（新）：`GuildState`（`GuildInfo` 内存态）/`GuildManager`/`GuildRank`/
  `WarGuild`/`GuildConfig`（含 O-1 三键）/`check_guild_name`/成员列表文本；照搬 oracle 的 N2/N3/N4 与
  `DelHumanObj` 的 `Compare(...)==1`、`GuildAllList` 恒空。
- `mir2-rs/crates/world/src/lib.rs`（+`pub mod guild;` + `pub use guild::{…}`，2 处共 5 行）。
- 22 条模块内构造级用例 + 2 条改坏必红。

## 判据（命令 + 原始件 + 读数）

| 项 | 命令 | 原始件 | 读数 |
| --- | --- | --- | --- |
| 构造级向量 | `cargo test -p mir2-world --lib guild:: --offline` | `02-rust-vectors.txt` | **22 passed / 0 failed** |
| 改坏必红 ①N4 末字符 | `cargo test … check_guild_name_rejects_forbidden_chars_except_last` | `03-mutation-1-n4-last-char-RED.txt` | **FAILED rc=101**（还原则绿，源 sha 一致） |
| 改坏必红 ②DelHumanObj | `cargo test … del_human_obj_compare_equals_one_quirk` | `04-mutation-2-del-human-obj-RED.txt` | **FAILED rc=101**（还原则绿，源 sha 一致） |
| 门禁 fmt | `cargo fmt --all -- --check` | `05-gate-fmt.txt` | rc=0 |
| 门禁 clippy | `cargo clippy --workspace --all-targets --offline -- -D warnings` | `06-gate-clippy.txt` | rc=0 |
| 门禁 test | `cargo test --workspace --offline` | `07-gate-test.txt` | **98 suites / 1046 passed / 0 failed / 21 ignored** |
| 门禁 msgcodegen | `cargo run -q -p msgcodegen --offline -- --check` | `08-gate-msgcodegen.txt` | **604 OK** |
| 边界 | `git diff --name-only cf3f81c35 -- src sql MirClient protocol storage` | `09-boundary.txt` | **空** |

`CARGO_TARGET_DIR=E:/tmp/openmir2-m3cs-target`，全部 `--offline`；零起栈、零端口。

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
2. `.ini`（`GuildConf`）与 `.txt`（四段）的**落盘 IO** 未在本片做：`bo_changed`/`dw_save_tick` 只标记
   待落盘；`from_record`/`to_body` 与 ⑧-3 的 `mir2-data::guild`（解析/序列化）逐字段对接。落盘接线归第②片。
3. 面板文本 `ClientOpenGuildDlg`（`<KillGuilds>` 里 oracle 直接 `+` record struct ⇒ 会打出
   `WarGuild { Guild = M2Server.Guild.GuildInfo, WarTick = …, WarTime = … }` 字面量）未在本片做，
   属表现差异，随第②片按 oracle 字面量口径实现或登记。

## 残留物清单

- `E:/tmp/openmir2-m3cs-target`（编译缓存，保留供复现）。
- `/tmp/m3cs-guild.rs.bak`（改坏必红用的改前副本，可删）。
- `E:/tmp/m3cs-scratch/guild-state/`（命令捕获草稿，可删，不入库）。
