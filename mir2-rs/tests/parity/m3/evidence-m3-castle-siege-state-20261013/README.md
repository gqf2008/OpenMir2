# 证据包：M3⑧-实现 7 攻城状态机（卡 `openmir2-m3-castle-siege-state`）

- **branch**：`worktree-m3-castle-siege-state`
- **base**：`refs/heads/master` = `origin/master d5be14f40`
- **写集**：`mir2-rs/crates/world/**`（新增 `src/castle.rs` + `src/lib.rs` 两行接线）
- **零起栈 / 零端口**：全离线（构造级 unit 用例）
- **oracle 锚**：`00-oracle-anchor-lines.txt`（`rg -n` 实测本仓库 `src/`）

## 1. 交付内容

| 文件 | 说明 |
| --- | --- |
| `crates/world/src/castle.rs`（新） | `UserCastle` 攻城状态机：`run` / `start_wallconquest_war` / `stop_wallconquest_war` / `main_door_control` / `add_attacker_info` / 归属判定 + N1 no-op + 11 条构造级用例 |
| `crates/world/src/lib.rs` | `pub mod castle;` + re-export（**只加本卡两处**，不动他人行） |

## 2. oracle 锚（逐行，见 `00-oracle-anchor-lines.txt`）

| 本模块 | oracle | 事实 |
| --- | --- | --- |
| `UserCastle::run` | `UserCastle.cs:311-448` | `MapName`/归属服门 `:318/:323`；跨天重置 `:328-339`；每日 `hour==StartCastlewarTime` 开战 `:340-376`；回收 Ghost `:377-389`；围墙 `StoneMode` `:391-440` |
| `start_wallconquest_war` | `UserCastle.cs:564-575` | `GetMapRageHuman(PalaceEnvir, HomeX, HomeY, 100)` → 逐玩家 `RefShowName` |
| `stop_wallconquest_war` | `UserCastle.cs:577-600` | `UnderWar=false` + `AttackGuildList.Clear()` + 广播（**玩家处理不可达**，见 §4） |
| `main_door_control` | `UserCastle.cs:890-909` | `BaseObject!=null && !Ghost` 时幂等 `Open`/`Close` |
| `add_attacker_info` | `UserCastle.cs:998-1014` | `InAttackerList` 查重 → `AttackDate=AddDateTimeOfDay(Now,days)` → `SaveAttackSabukWall` + `SS_212` |
| `is_master_guild` / `is_member` | `UserCastle.cs:639-642` / `:480-483` | `MasterGuild != null && MasterGuild == guild` |
| `in_palace_guild_count` | `UserCastle.cs:604-607` | 返回 `AttackGuildList.Count`（**不是皇宫内玩家数**，登记） |
| `check_in_palace` | `UserCastle.cs:680-706` | 任一面墙 `BaseObject` 非空、`Death`、坐标命中 |

## 3. 构造级判据（11/11，`01-rust-vectors.txt`，命令可复现）

```
CARGO_TARGET_DIR=E:/tmp/openmir2-m3cs-target \
  cargo test --manifest-path mir2-rs/Cargo.toml -p mir2-world --lib castle --offline
```

| # | 用例 | 钉住的 oracle 行为 |
| --- | --- | --- |
| 1 | `daily_war_starts_at_configured_hour_with_today_attackers` | 19 点不开、20 点开；`AttackGuildList=[攻方,守方]`；`AttackWarList` 清空；事件序 `RefreshShowName → SaveAttackSabukWall → SS_212 → Broadcast → SS_204`；`MainDoorControl(true)` 关门；三墙 `StoneMode=false` |
| 2 | `war_not_started_when_attacker_date_is_another_day` | `AttackDate != 今天` ⇒ 不开战，但 `IsStartWar=true`（`:345` 置位）；墙保持 `true` |
| 3 | `cross_day_resets_income_and_start_flag` | `:334-338`：`TodayIncome=0`、`IncomeToday=今天`、`IsStartWar=false` |
| 4 | `empty_or_foreign_map_returns_early` | `:318` 空 `MapName`、`:323` 非本服归属 ⇒ 直接返回，无状态变化 |
| 5 | `war_end_warning_then_stop` | 阈值 `CastleWarTime-ShowCastleWarEndMsgTime` **严格大于**才播预告（只播一次）；超过 `CastleWarTime` **严格大于**才停战；停战发生在 `if (UnderWar)` 块内 ⇒ **本 tick 墙仍 `false`**、下一 tick 才恢复 `true` |
| 6 | `stop_war_does_not_touch_players_like_oracle_bug` | 停战只清 `AttackGuildList` + 广播（§4 怪行为 2） |
| 7 | `ghost_units_are_reclaimed_guards_before_archers` | `:377-389` `Guards` 先、`Archers` 后按下标回收 `Ghost` |
| 8 | `main_door_control_is_idempotent_and_skips_ghost` | `:890-909` 状态不符才动；`Ghost` / 无门不动、不 panic |
| 9 | `add_attacker_info_dedups_and_ignores_days_like_oracle` | `InAttackerList` 查重；**照搬 `AddDateTimeOfDay` 忽略天数**（§4 怪行为 1） |
| 10 | `predicates_match_oracle` | `is_master_guild`/`is_member`/`in_palace_guild_count`/`check_in_palace` |
| 11 | `n1_entry_points_are_observable_noops` | 10 个 N1 入口 + 自动占领整链 = 调用前后**逐字段相同**、零事件、不停战、不改归属（T-38） |

## 4. 照搬的 oracle 怪行为（**不是**差异；硬约束③「行为等价优先于修 bug」）

1. **`AddDateTimeOfDay` 忽略天数**（`M2Share.cs:458-493`：函数体整段被注释，末尾只有
   `DateTime result = DateTime.Now;`）⇒ `AddAttackerInfo(GuildInfo).AttackDate = 本刻`，
   而**不是** `+StartCastleWarDays`。Rust 照搬（用例 9）。
2. **`StopWallconquestWar` 的玩家处理不可达**（`UserCastle.cs:582-584`：
   `GetMapOfRangeHumanCount(CastleEnvir, HomeX, HomeY, 100);` **丢弃返回值**，`ListC` 恒空 ⇒
   `ChangePkStatus(false)` / 非守方 `MapRandomMove` 从不执行）。Rust 照搬：只清表 + 广播（用例 6）。
3. **`InPalaceGuildCount` 命名与语义不符**（返回攻城行会数，不是皇宫内玩家数；盘点 §1.E）。
4. **停战 tick 内墙仍 `StoneMode=false`**（`:391-406` 先置 false、`:421` 才停战，本 tick 不再走 `else`）。

## 5. N1：10 个 `IUserCastle` 重载 = 可观测 no-op（owner 裁决 `a137cc900c`；白名单 **T-38**）

oracle 里这些重载体 `throw new NotImplementedException` 被外层 `catch` 吞，且调用点静态类型是
接口 `IUserCastle` ⇒ 真实实现所在的 `GuildInfo`/`PlayObject` 重载**永远选不中**（盘点 §1.L）。
Rust 照此建模：入口走 no-op（不 `panic!`/不返 `Err`/不改可观测状态/不发事件），**不**实现「本该有」的
提取/上缴/归属逻辑。10 个入口与抛出点：

`with_drawal_golds_n1`(:601) / `get_castle_n1`(:646) / `add_attacker_info_n1`(:672) /
`can_get_castle_n1`(:677) / `check_in_palace_player_n1`(:709) / `receipt_golds_n1`(:913) /
`is_attack_ally_guild_n1`(:1042) / `is_attack_guild_n1`(:1047) /
`is_defense_ally_guild_n1`(:1052) / `is_defense_guild_n1`(:1057)，
以及自动占领整链 `try_auto_occupy_n1`（`PlayObject.Message.cs:324-334`）。

## 6. 改坏必红（牙齿自复现）

| 变异 | 结果 | 原始件 |
| --- | --- | --- |
| MUTATION-1：`can_get_castle_n1` 返回 `false` → `true`（N1 从 no-op 变「生效」） | `n1_entry_points_are_observable_noops` **FAILED**（10 passed/1 failed/rc=101） | `02-mutation-1-n1-takes-effect-RED.txt` |
| MUTATION-2：战时左墙 `set_stone_mode(false)` → `true` | `daily_war_starts_at_configured_hour…` + `war_end_warning_then_stop` **FAILED**（9 passed/2 failed/rc=101） | `03-mutation-2-left-wall-stone-mode-RED.txt` |
| 还原 | 11/11 GREEN；源码 sha256 `c69f5da2ef4c670300499fc70d8fb81a61038111332be607916adcdd595e3ab1` 与备份相同 | `01-rust-vectors.txt` |

变异前 `castle.rs` 备份：`/e/tmp/m3cs-castle-siege.rs.bak`（仓外临时件，见残留物清单）。

## 7. 四门禁（在 base `d5be14f40` 的交付树上重跑）

| 门 | 命令 | 结果 | 原始件 |
| --- | --- | --- | --- |
| fmt | `cargo fmt --all -- --check` | rc=0 | `04-gate-fmt.txt` |
| clippy | `CARGO_TARGET_DIR=E:/tmp/openmir2-m3cs-target cargo clippy --workspace --all-targets --offline -- -D warnings` | rc=0 | `05-gate-clippy.txt` |
| test | `CARGO_TARGET_DIR=E:/tmp/openmir2-m3cs-target cargo test --workspace --offline` | rc=0；**98 suites / 1024 passed / 0 failed / 21 ignored** | `06-gate-test.txt` |
| msgcodegen | `CARGO_TARGET_DIR=E:/tmp/openmir2-m3cs-target cargo run -p msgcodegen --offline -- --check` | rc=0；604 条常量一致 | `07-gate-msgcodegen.txt` |

## 8. 边界（`08-boundary.txt`）

对 base `d5be14f40`：仅 `mir2-rs/crates/world/src/{castle.rs,lib.rs}` 两文件；`src/ sql/ MirClient`
与 `crates/protocol` `crates/storage` 零改动；`bins/**` 零改动。

## 9. 未做 / 登记（承接）

- **`bins/game-svr` 接线**（tick 驱动 `UserCastle::run` + 事件落帧/落盘）：`bins/game-svr` 此刻有
  多条并行车道（m2-svr ⑤-4 / m3hp ⑨ / m3pt ⑦）⇒ 按 brief「写集相撞先报后动」留作紧邻第②片，
  等 owner 对 `bins/game-svr` 串行序裁决（同 ⑧-4 comment `018d7be2` 的请求）。
- **真机两侧时间门对拍**：本片用**构造级注入时钟**（`CastleNow`）保证离线可判；跨侧同起点/同推进的
  真机时间门仍依赖 ⑧-1 `openmir2-m3-clock-injection`（C# 侧时钟注入）+ 独占窗口。
- **⑧-6 第②片**（实体挂图 `RegenMonsterByName` + `WAbil.HP`）可用本模块的 `CastleUnit` 承接
  （门/墙/12 弓手/12 守卫的 `ghost`/`death`/`stone_mode`/`opened`/`x`/`y`/`struck_tick` 已就位）。
- `RepairDoor`/`RepairWall`（60s `StruckTick` 门）归 ⑧-8；`IncRateGold`/`WithDrawalGolds`/`ReceiptGolds`
  （`PlayObject` 重载）与商人税归 ⑧-9；`GetWarDate`/`GetAttackWarList` 面板文案面未建模。

## 10. 残留物清单

- `E:/tmp/openmir2-m3cs-target`：编译缓存（保留供复现）。
- `/e/tmp/m3cs-castle-siege.rs.bak`、`/e/tmp/m3cs-castle-siege.rs.final`：变异/还原用的源码副本（可删）。
- `/e/tmp/m3cs-claim.json` 等签名草稿（可删，不入库）。
- 共享主检出 `E:/Users/gxh/Documents/GitHub/OpenMir2` 全程未写、未编译、未提交。
