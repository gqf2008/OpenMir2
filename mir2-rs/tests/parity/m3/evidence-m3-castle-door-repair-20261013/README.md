# 证据包：M3⑧-实现 8 门/墙状态（卡 `openmir2-m3-castle-door-repair`）

- **branch**：`worktree-m3-castle-door-repair`
- **base**：`refs/heads/master` = `origin/master 029d23eae`
- **写集**：`mir2-rs/crates/world/**`（`src/castle.rs` 扩展 + `src/map.rs` 两方法 + `src/lib.rs` re-export）
  + `mir2-rs/tests/parity/**`（本证据包 + `whitelist.md` 的 B-15 行）
- **零起栈 / 零端口**：全离线（构造级 unit 用例，虚拟时钟经既有 `CastleNow`/`now_tick` 注入）
- **oracle 锚**：`00-oracle-anchor-lines.txt`（`sed`/`rg -n` 实测本仓库 `src/`）

## 1. 交付内容

| 文件 | 说明 |
| --- | --- |
| `crates/world/src/castle.rs` | 扩展 `CastleUnit`（`hp/max_hp/dir/hold_place/set_map_flaged/health_tick/death_tick`）+ `door_open/close/die/run/ref_status`、`wall_initialize/ref_status/run`、`repair_door`/`repair_wall`、`main_door_control` 升级为带帧；`StructureEvent`/`CastleOfficialAction`/`OfficialRepairResult`/`HireSubstringError`；`door_map_xy_flag_ops`/`door_state_text`/`castle_official_action`/`hire_substring`/`official_repair_door`/`official_repair_wall`/`apply_structure_events`/`hutil32_round`/`compare_lstr` + 19 条新用例 |
| `crates/world/src/map.rs` | `MapGrid::set_map_xy_flag`（照搬 `Envir.SetMapXyFlag` 的 `Valid` 前置）+ `MapGrid::cell_attribute`（掩码判据直读口） |
| `crates/world/src/lib.rs` | `pub use castle::{…}` 增补本片新符号（只加本行） |
| `tests/parity/whitelist.md` | 新增 **B-15** 行（门/墙掩码 `LowWall` 不可恢复的 oracle bug 复刻） |

## 2. oracle 锚（逐行，见 `00-oracle-anchor-lines.txt`）

| 本模块 | oracle | 事实 |
| --- | --- | --- |
| `CastleUnit::door_open` | `CastleDoor.cs:46-58` | `Death` 提前返回；`Dir=7` → `RM_DIGUP` → `IsOpened=true` → `StoneMode=true` → `SetMapXyFlag(0)` → `HoldPlace=false` |
| `CastleUnit::door_close` | `CastleDoor.cs:60-76` | `Death` 提前返回；`Dir=(byte)(3-Round(HP/MaxHP*3))`、`Dir-3>=0 ⇒ 0` → `RM_DIGDOWN` → `IsOpened=false` → `StoneMode=false` → `SetMapXyFlag(1)` → `HoldPlace=true` |
| `CastleUnit::door_die` | `CastleDoor.cs:78-82` | `base.Die()`（置 `Death`）+ `SetMapXyFlag(2)` |
| `CastleUnit::door_run` | `CastleDoor.cs:84-104` | `Death && Castle != null ⇒ DeathTick=GetTickCount()`，否则 `HealthTick=0`；`!IsOpened` 时按比例刷 `Dir` + `RM_TURN`（`n08 < 3`） |
| `CastleUnit::door_ref_status` | `CastleDoor.cs:106-115` | `3-Round(HP/MaxHP*3)`、`n08-3>=0 ⇒ 0` → `RM_ALIVE` |
| `door_map_xy_flag_ops` | `CastleDoor.cs:23-44` | 15/12/12 项有序 `(dx,dy,walk)`（`nFlag=0/1/2`），含重复格 |
| `MapGrid::set_map_xy_flag` | `Envirnoment.cs:1362-1369` | 只在 `GetCellInfo` 成功（`Valid`）时 `SetAttribute` |
| `CastleUnit::wall_initialize` | `WallStructure.cs:17-21` | `Dir=0` 后 `base.Initialize()` |
| `CastleUnit::wall_ref_status` | `WallStructure.cs:23-40` | `HP>0 ⇒ 3-Round(…)` 否则 `4`；`n08>=5 ⇒ 0`；`RM_ALIVE` |
| `CastleUnit::wall_run` | `WallStructure.cs:47-82` | 死亡 ⇒ `DeathTick` + `SetMapFlaged ⇒ SetMapXyFlag(本格,true)`；否则 `HealthTick=0` + `!SetMapFlaged ⇒ SetMapXyFlag(本格,false)`；按比例刷 `Dir` + `RM_DIGUP`（`n08 < 5`） |
| `UserCastle::main_door_control` | `UserCastle.cs:890-909` | `BaseObject!=null && !Ghost` 时幂等 `Open/Close` |
| `UserCastle::repair_door` | `UserCastle.cs:920-955` | `null \|\| UnderWar \|\| HP>=MaxHP` ⇒ false；`(GetTickCount()-StruckTick) > 60s` 才回满；死亡支加 `Death=false`、`IsOpened=false`；`RefStatus` |
| `UserCastle::repair_wall` | `UserCastle.cs:956-997` | `nWallIndex` 1/2/3 ⇒ 左/中/右；其余 ⇒ `Wall=null` ⇒ false；其余同门 |
| `door_state_text` | `CastleOfficial.cs:52-67` | `Death ⇒ "destroyed"`；`IsOpened ⇒ "opened"`；否则 `"closed"` |
| `castle_official_action` | `CastleOfficial.cs:199-238` | 精确匹配 `@openmaindoor`/`@closemaindoor`/`@repairdoornow`/`@repairwallnow1..3`；`CompareLStr` 前缀匹配雇佣 |
| `official_repair_door` / `official_repair_wall` | `CastleOfficial.cs:358-406` | 先判 `TotalGold >= 价格`，成功才扣；文案「修理成功。/城门不需要修理!!!/城内资金不足!!!」 |
| `hire_substring`（N5） | `CastleOfficial.cs:229/235` | `Substring(13, sLabel.Length)` 的 length 实参恒越界 ⇒ 抛 ⇒ 被 `catch` 吞 |
| `hutil32_round` | `HUtil32.cs:131-134` | `(int)Math.Round(r, MidpointRounding.AwayFromZero)` |

## 3. 构造级判据（30/30，`01-rust-vectors.txt`，命令可复现）

```
CARGO_TARGET_DIR=E:/tmp/openmir2-m3cs-target \
  cargo test --manifest-path mir2-rs/Cargo.toml -p mir2-world --lib castle --offline
```

既有 11 条攻城状态机用例保持（其中 `daily_war_starts_…` 与 `main_door_control_is_idempotent_…`
按 ⑧-8 的 `CastleDoor.Close` 帧/掩码更新期望值）。新增 19 条：

| # | 用例 | 钉住的 oracle 行为 |
| --- | --- | --- |
| 1 | `door_open_close_sets_state_and_frames_like_oracle` | `Open`/`Close` 的字段与帧（1+15 / 1+12）|
| 2 | `door_close_dir_tracks_hp_ratio_and_clamps_like_oracle` | `Dir` 比例 + `Dir-3>=0 ⇒ 0` 六个档 |
| 3 | `door_open_close_and_die_return_early_when_dead` | 死亡时 `Open/Close` 可观测 no-op；`Die` 仍发掩码 2 |
| 4 | `door_die_sets_death_then_mask2` | `Die` 的 `SetMapXyFlag(2)` = 12 项 true |
| 5 | `door_run_renews_death_tick_only_when_owned_by_castle` | `Death && Castle!=null` vs `HealthTick=0` |
| 6 | `door_run_turns_dir_only_when_closed_and_n08_below_3` | `RM_TURN` 的 `n08<3` 门 |
| 7 | `door_and_wall_ref_status_reset_dir_and_frame` | 门/墙 `RefStatus` 的 dir 公式与 `RM_ALIVE` |
| 8 | `wall_run_blocks_own_cell_and_restores_it_on_death` | `SetMapFlaged` 单格掩码 + `RM_DIGUP` |
| 9 | `door_mask_blocks_cells_and_low_wall_cannot_be_restored_like_oracle` | 通行掩码落格 + **B-15**（`LowWall` 不可恢复） |
| 10 | `door_map_xy_flag_ops_match_oracle_call_sequence` | 15/12/12 项调用序（含重复格） |
| 11 | `repair_door_gates_on_underwar_full_hp_and_60s_tick` | `UnderWar`/满血/60s 严格大于三只门 |
| 12 | `repair_door_dead_branch_clears_death_and_opened` | 死亡支 `Death=false` + `IsOpened=false` |
| 13 | `repair_door_rejects_missing_door` | `BaseObject==null` ⇒ false |
| 14 | `repair_wall_maps_indices_one_to_three_and_rejects_others` | 1/2/3 → 左/中/右；0/4 ⇒ false |
| 15 | `official_repair_door_checks_gold_then_repairs` | 价格门 → `RepairDoor` → 扣钱/文案 |
| 16 | `official_repair_wall_checks_gold_then_repairs` | 同上（墙）；先判钱后判墙 |
| 17 | `castle_official_labels_dispatch_and_n5_hire_is_unreachable` | 标签派发 + N5 前缀命中但不可达 |
| 18 | `n5_hire_substring_is_always_out_of_range_like_oracle` | `Substring` 长度实参恒越界（T-40） |
| 19 | `door_state_text_matches_oracle_order` | 先 `Death` 后 `IsOpened` |

## 4. 照搬的 oracle 怪行为（**不是**差异；硬约束③「行为等价优先于修 bug」）

1. **B-15：门/墙掩码的 `LowWall` 不可恢复**（白名单新增行）。`Envir.SetMapXyFlag`
   （`Envirnoment.cs:1362-1369`）先走 `GetCellInfo`，而 `GetCellInfo`（`:322-335`）对
   「越界**或** `Attribute != Walk`」都返 `success=false` ⇒ `SetAttribute` 不执行。
   于是 `CastleDoor.SetMapXyFlag(1)`（关门）把 9 格置 `LowWall` 后，`SetMapXyFlag(0)`（开门）
   的 `boFlag=true` 调用**无法**把它们恢复成 `Walk` ⇒ 城门「开」了但格子仍阻断。
   用例 9 逐格钉住；`MapGrid::set_map_xy_flag` 照搬该 `Valid` 前置。`WallStructure` 用
   `SetMapFlaged` 单格标记规避了这一点（用例 8），只有 `CastleDoor` 的 9 格掩码命中本 bug。
2. **N5 雇佣不可达**（白名单 **T-40**）：`CastleOfficial.cs:229/235` 的
   `sLabel.Substring(prefix.Length, sLabel.Length)` 的 length 实参恒越界（`13 + len > len`）⇒ 抛
   `ArgumentOutOfRangeException` 被 `UserSelect` 的 `catch` 吞 ⇒ `@hireguardnowN`/`@hirearchernowN`
   静默无效果。Rust 照搬：标签派发仍归 `HireUnreachable`，`hire_substring` 恒返
   `Err(ArgumentOutOfRange)`（用例 17/18）。
3. **`WallStructure.RefStatus` 的 `n08 >= 5 ⇒ 0`**（`:34-37`）在 `HP>0`/`MaxHP>0` 下不可达
   （`n08` 上界 = 4，来自 `HP<=0` 支），照搬保留该钳位（无独立用例，登记）。
4. **`CastleOfficial.$CASTLEDOORSTATE` 无 null 检查**（`:54`）：`MainDoor.BaseObject` 为空会抛
   `NullReferenceException`；`door_state_text` 要求调用方已确认门存在（登记）。

## 5. 改坏必红（牙齿自复现）

| 变异 | 结果 | 原始件 |
| --- | --- | --- |
| MUTATION-1：`door_map_xy_flag_ops` 的 `bo_flag = n_flag != 1` → `n_flag == 1`（掩码开关反置） | 4 条用例 **FAILED**（26 passed/4 failed/rc=101：`door_map_xy_flag_ops…`/`door_die_sets_death…`/`daily_war_starts…`/`door_mask_blocks…`） | `02-mutation-1-bo-flag-inverted-RED.txt` |
| MUTATION-2：`hire_substring` 去掉 length 检查、改返 `Ok(&label[start..])`（N5 变可达） | `n5_hire_substring_is_always_out_of_range_like_oracle` **FAILED**（`left: Ok("0")` / `right: Err(ArgumentOutOfRange)`，rc=101） | `03-mutation-2-n5-hire-becomes-reachable-RED.txt` |
| 还原 | 30/30 GREEN；源码 sha256 `e3d7bd0d1a1879c35bba370cb36308456c22f4a3de2c00b978a6a571ec4397d2` 与变异前相同（`01-rust-vectors.txt`） | |

## 6. 四门禁（在 base `029d23eae` 的交付树上重跑）

| 门 | 命令 | 结果 | 原始件 |
| --- | --- | --- | --- |
| fmt | `cargo fmt --all -- --check`（cwd `mir2-rs/`） | rc=0 | `04-gate-fmt.txt` |
| clippy | `CARGO_TARGET_DIR=E:/tmp/openmir2-m3cs-target cargo clippy --workspace --all-targets --offline -- -D warnings` | rc=0 | `05-gate-clippy.txt` |
| test | `CARGO_TARGET_DIR=E:/tmp/openmir2-m3cs-target cargo test --workspace --offline` | rc=0；**98 suites / 1057 passed / 0 failed / 21 ignored** | `06-gate-test.txt` |
| msgcodegen | `CARGO_TARGET_DIR=E:/tmp/openmir2-m3cs-target cargo run -p msgcodegen --offline -- --check` | rc=0；604 条常量一致 | `07-gate-msgcodegen.txt` |

附带离线门：`python -I mir2-rs/tests/parity/m3/m3_whitelist_registry_audit.py audit` ⇒
`WHITELIST_REGISTRY GREEN`（B-15 行不改 T-* 唯一性）。

## 7. 边界（`08-boundary.txt`）

相对 base `029d23eae`：仅 `mir2-rs/crates/world/src/{castle.rs,map.rs,lib.rs}` +
`mir2-rs/tests/parity/whitelist.md` + 本证据包；`src/ sql/ MirClient` 与
`crates/protocol` `crates/storage` `bins/**` 零改动。

## 8. 未做 / 登记（承接）

- **`bins/game-svr` 接线**（tick 驱动 `CastleUnit::door_run`/`wall_run`、把 `StructureEvent::SetMapXyFlag`
  作用到真地图、`UserCastle::run` 的门/墙 tick、`$CASTLEDOORSTATE` 脚本变量、`@repair*` 收包面）：
  `bins/game-svr` 此刻有多条并行车道 ⇒ 按 brief「写集相撞先报后动」留作紧邻第②片，等 owner 对
  `bins/game-svr` 串行序裁决（同 ⑧-4 comment `018d7be2` 的请求）。
- **真机两侧时间门对拍**：本片用**构造级注入时钟**（`now_tick`）保证离线可判；跨侧同起点/同推进仍依赖
  ⑧-1 `openmir2-m3-clock-injection`（C# 时钟注入）+ 独占窗口。
- **⑧-6 第②片**（实体挂图 `RegenMonsterByName` + `WAbil.HP`）可用本片的 `CastleUnit.hp/max_hp/dir`
  承接；`RepairDoor/RepairWall` 的 `StruckTick` 更新点（被击时）属伤害路径，归怪物 AI/受击接线。
- **⑧-9 税收/金库**接本片 `OfficialRepairResult` 的 `TotalGold` 扣减口径；`WithDrawalGolds`/`ReceiptGolds`
  的 N1 no-op 已在 ⑧-6 片建模（T-38）。

## 9. 残留物清单

- `E:/tmp/openmir2-m3cs-target`：编译缓存（保留供复现）。
- 变异用的源码副本 `/tmp/m3cs-castle-door-repair.rs.bak`（**已删**）。
- 共享主检出 `E:/Users/gxh/Documents/GitHub/OpenMir2` 全程**只读**（仅 `sed`/`rg` 取 oracle 锚），
  未写、未编译、未提交。
