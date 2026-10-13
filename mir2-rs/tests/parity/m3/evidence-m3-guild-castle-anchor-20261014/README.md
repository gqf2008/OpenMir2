# evidence-m3-guild-castle-anchor-20261014

卡：`openmir2-m3-guild-castle-fixture-botsrv`（M3⑧-10 行会/攻城动作序列夹具）。
本片 = **⑧-10 残余 Minor「锚行号」收口**（reviewer 交付链：⑧-6 reviewer-5 O-1 片
`3d76cdb9` 在 patch body / README §4 显式登记的 `guild_castle_scenario.json` M-1）。

- tier：**L**（夹具/工具/文档：零 `.rs`、零生产路径、零 RNG/时序/落库/战斗数值；真机两侧同序列
  diff=0 仍是本卡后续 DoD）。
- 离线：零起栈、零端口；`CARGO_TARGET_DIR=E:/tmp/openmir2-m3cs-target`；工作树 = `m3cs`。
- base = `origin/master` 的 `d4673b16c`（head 见 `10-boundary.txt`）。

## 1. 修了什么

### 1.1 `guild_castle_scenario.json` 锚行号逐条对冻结 `src/` 复核（M-1）

本片把 `oracle_anchors` 里 **34 条 `File.cs:SPEC` 引用 / 97 个行点**逐条对冻结 `src/` 复算，
订正了下列**已机判确证**的漂移（其余区域型锚按原样锁成行点，见 `guild_castle_anchor_gate.py`
的 `REGISTERED`）：

| 锚 | 改前 | 改后（冻结 src/ 实测） | 依据 |
| --- | --- | --- | --- |
| `castle_run` | `UserCastle.cs:311-448` | `311-446` | `Run()` 起 311 于 `:446 }` 闭合（与 ⑧-7 横向体检一致） |
| `castle_door` | `UserCastle.cs:890-910` | `890-909` | `MainDoorControl` 起 890 于 `:909 }` 闭合 |
| `castle_door` | `UserCastle.cs:920-997` | `920-996` | `RepairDoor`(920-949)+`RepairWall`(956-996) 区间；:997 为空白行 |
| `build_guild` | `GuildOfficial.cs:112-178/142-161/170-177` | `112-172/142-162/163-170` | `ReQuestBuildGuild` 112-172；`if (AddGuild)` 142-162；`RM_BUILDGUILD_OK/FAIL` 163-170 |
| `guild_panel` | `PlayObject.Operate.cs:1229-1277` | `1229-1276` | `ClientOpenGuildDlg` 闭合行 |
| `guild_panel` | `/ :1283-1306` | `:1283-1305` | `ClientGuildMemberList` 闭合行 |
| `member_ops` | `1307-1368/1369-1425/1426-1443` | `1307-1367/1369-1424/1426-1442` | 三个方法闭合行 |
| `ally` | `PlayObject.Operate.cs:1465-1560` | `1465-1523` | `ClientGuildAlly` 闭合行（1560 已越到另一法） |
| `ally` | `GuildInfo.cs:1007-1019` | `1007-1018` | `AllyGuild` 闭合行 |
| `guild_war` | `GuildOfficial.cs:180-201` | `180-199` | `ReQuestGuildWar` 闭合行（201 是下一法声明） |
| `guild_war` | `PlayObject.Base.cs:3349-3386` | `3349-3385` | `ReQuestGuildWar` 闭合行 |
| `region` | `UserCastle.cs:454-479` | `454-478` | `InCastleWarArea` 闭合行 |
| `frames` | `Messages.cs:750/753/754/756/.../771` | `Messages.cs:364/367/368/.../383` | 原值是**消息号**（750=SM_CHANGEGUILDNAME…），不是行号 ⇒ 换算为 `Messages.cs` 冻结行 |
| `botsrv_primitives` | `RobotPlayer.cs:3687-3790` | `RobotPlayer.cs:4708-4809` | `SendGuildDlg/Home/MemberList/AddMem/DelMem/UpdateNotice/UpdateGrade` 定义区 |
| `botsrv_primitives` | `:3532/3594 SendMerchantDlgSelect` | `RobotPlayer.cs:4553 SendMerchantDlgSelect` | 原值是越界行（`RobotPlayer.Operate.cs` 仅 1594 行）；定义在 `RobotPlayer.cs:4553` |
| `botsrv_primitives` | `RobotPlayer.Operate.cs:1305-1520` | `1319-1540` | 原值起点是别组消息 `case -2:`；改为 `SM_OPENGUILDDLG`(1319) … `SM_BUILDGUILD_FAIL`(1540) 观测区 |

同片把 scenario 的 `cases[].note` 里与上表同源的锚一并订正（`:180-201`→`:180-199`、
`GuildInfo.cs:1007-1019`→`1007-1018`、`920-955`→`920-949`、`956-997`→`956-996`），保持
「锚 ↔ 说明」一致。`01-scenario-anchor-diff.txt` 为完整 diff。

### 1.2 新增 fail-closed 门 `guild_castle_anchor_gate.py`（防再次漂移）

判据（任一不满足 ⇒ rc=1）：
1. **覆盖面**：`oracle_anchors` 解析出的每个 `File.cs:SPEC` 必须在 `REGISTERED` 表内；表内有而
   场景没有也判红（防「新锚不登记就假绿」/表过期）。
2. **读侧**：每条锚声明的每个 `(line, token)` 必须命中冻结 `src/` 对应行（改行号/改文件/越界 ⇒ RED）。
   `token` 取冻结行本体，锁住锚的位置语义。
3. 门同时给出 `selftest`（表结构/读侧 token 牙/未登记牙/表过期牙/越界牙/真身正向）与
   `mutation`（MUT1 `311-446→448`、MUT2 `890-909→910`、MUT3 读侧 token 漂移，各自必红）。

### 1.3 接线（不是「只在仓库里躺着的脚本」）

`run_guild_castle_pair.ps1 -SelfCheckNoStack` 新增 3 条：
`anchor-gate-check` / `anchor-gate-selftest` / `anchor-gate-mutation-red`；
无栈自检从 **19/19** 升为 **22/22**（`05-run-guildcastle-selfcheck.txt`）。

### 1.4 whitelist T-46 证据指针订正（reviewer-5 O-1）

T-46 行原先只指 `m3/evidence-m3-guild-castle-fixture-botsrv-20261013/**`，实际最新实测是
`...-merge2/merge3-20261013/**` ⇒ 补齐三目录 + 本片证据目录。

## 2. 命令与读数

| 件 | 命令 | 读数 |
| --- | --- | --- |
| `02` | `python -I mir2-rs/tests/parity/m3/guild_castle_anchor_gate.py check` | GREEN（34 锚 / 97 行点）rc=0 |
| `03` | `... selftest` | SELFTEST GREEN rc=0 |
| `04` | `... mutation` | MUTATION GREEN（MUT1/MUT2/MUT3 全 RED、MUT4 真身 GREEN）rc=0 |
| `05` | `powershell -File mir2-rs/tests/parity/m3/run_guild_castle_pair.ps1 -SelfCheckNoStack` | SELFTEST **ok=22/22** rc=0（含 3 条锚门） |
| `06` | `cargo fmt --all -- --check` | rc=0 |
| `07` | `cargo clippy --workspace --all-targets -- -D warnings` | rc=0（0 warning） |
| `08` | `cargo test --workspace` | **101 suites / 1199 passed / 0 failed / 22 ignored** rc=0 |
| `09` | `cargo run -q -p msgcodegen -- --check` | 604 条常量 rc=0 |
| `10` | boundary | 冻结面（`src` `sql` `MirClient` `protocol` `storage` `bins`）空；`*.rs` 空 |

## 3. 边界（`10-boundary.txt`）

改树仅：`mir2-rs/tests/parity/m3/guild_castle_scenario.json`（锚/说明）、
`mir2-rs/tests/parity/m3/guild_castle_anchor_gate.py`（新）、
`mir2-rs/tests/parity/m3/run_guild_castle_pair.ps1`（自检 3 条）、
`mir2-rs/tests/parity/whitelist.md`（T-46 指针）、本证据目录。
**零 `.rs`、零 `src/`/`sql/`/`MirClient/`/`crates/`/`bins/` 改动**。

## 4. 残留物 / 后续

- `guild_castle_anchor_gate.py` 的 `REGISTERED` 中，**区域型锚**（如 `PlayObject.Message.cs:975-1007`
  的 9 个 `CM_GUILD*` 派发、`UserCastle.cs:242-309` 的 Load/SaveAttackSabukWall、
  `CastleOfficial.cs:199-226` 的 else-if 链、`WorldServer.cs:298-334`）按其冻结起止行锁成行点，
  **未按单方法体收窄**——这是「锚 = oracle 区域」的原意，不是未核。若将来要按方法体收窄，另开片。
  （`botsrv_primitives` 的观测区已按上表收窄到 `1319-1540`。）
- 真机两侧 `run_guild_castle_pair.ps1` 同序列 diff=0（本卡 DoD②③）仍等 ⑧ 实现卡 +
  `openmir2-m3-clock-injection` + owner 独占窗口，本片**未起栈/未占端口**。
- 本片临时物：`E:/tmp/openmir2-m3cs-target`（编译缓存，登记保留）；`E:/tmp/m3cs-anchor/`
  （锚解析/生成脚本，非仓内件，登记保留）。
