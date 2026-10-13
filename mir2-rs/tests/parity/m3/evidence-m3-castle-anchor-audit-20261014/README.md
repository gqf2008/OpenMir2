# ⑧ castle oracle 锚横向体检 + castle 锚门接入夹具链深检门 —— 证据包

- 卡：`openmir2-m3-castle-file-load`（M3⑧-实现 6；关 reviewer-5 的 **O-1**「把
  `castle_oracle_anchor_gate.py` 接入某个 gate runner / 常跑项」）
- 分支：`worktree-m3-castle-anchor-audit`
- base：`origin/master ad0592b19`（已含 ⑧-6 第②片收口 `9e3a9bc32` = merge `b5f07621d`）
- tier：**L**（纯文档/工具片：`.rs` 差量经机判证明**全在注释行**；无 RNG/时序/落库/战斗数值）
- 写集：`mir2-rs/crates/world/src/castle.rs`（仅 doc 注释）+ `mir2-rs/tests/parity/m3/castle_oracle_anchor_gate.py`
  + `mir2-rs/tests/parity/m3/fixture_chain_gate.py` + 本证据目录。
  零 `src/ sql/ MirClient`、零 `crates/protocol`、零 `crates/storage`、零 `bins/`、零生产代码。

## 1. 本片做了什么

⑧-6 收口片新增的门 `castle_oracle_anchor_gate.py` 此前**只是一个独立脚本**（reviewer-5 O-1：
「未见任何 runner/CI 或 evidence 生成流程自动调用它」）。本片做三件事：

1. **门覆盖面扩展 + 牙齿加厚**（`castle_oracle_anchor_gate.py`）：
   - `ANCHORS` 从 35 条扩到 **82 条**，把 **⑧-7 攻城状态机模块头**（`castle.rs` 顶到 ⑧-8 段之前，
     即 `castle.rs:1-175`）的锚全部登记：`UserCastle.Run` / `StartWallconquestWar` /
     `StopWallconquestWar` / `MainDoorControl` / `AddAttackerInfo` / `IsMasterGuild` / `IsMember` /
     `InPalaceGuildCount` / `CheckInPalace` + 10 个 N1 抛出点与被吞调用点 + `M2Share.AddDateTimeOfDay`
     怪行为 + `CastleConst`/`Messages`/`GameSvrConf`/`ServerConf` 常量与配置锚；
   - 新增 **覆盖面判定**：`COVERED_MARKERS` 括出的区间内**每个** `文件.cs:NNN` 锚都必须在
     `ANCHORS` 表内（未登记 ⇒ RED），且区间内**不得**留裸 `:NNN`（必须写成 `文件.cs:NNN`）；
   - `selftest` 加 3 条覆盖面牙齿（未登记锚 / 裸锚 / 覆盖标记缺失）+ 真身覆盖面 GREEN；
   - `mutation` 加 MUT4（覆盖面未登记锚）/ MUT5（`:311-446`→`:311-448` 范围漂移，两条独立牙）/
     MUT6（`UserCastle.cs:480-483` 回退成裸 `:480-483`）。
2. **横向体检：订正 `castle.rs` 里 ⑧-7 方法范围的漂移锚**（全在 doc 注释，零行为）：
   模块头 6 处 + 后文 4 处（见 §2 表），全部按**冻结 `src/`** 实读行号。
3. **接入夹具链深检门（O-1 正解）**（`fixture_chain_gate.py`）：
   - `SHARED_CHECKS` 注册两条主机门：`castle_anchor_selftest` / `castle_anchor_check`；
   - 新增 `_tool_argv` / `_run_tool` 按扩展名分派（`.py` ⇒ `python -I`，`.ps1` ⇒ `powershell -File`），
     共享管线不再只能跑 PowerShell；
   - `selftest` 加 7 条牙齿（含 主机门（.py）登记项 ≥1、`_tool_argv` 两向、`_run_tool` 真跑 rc=0 /
     缺件 ⇒ 127）。

## 2. oracle 锚横向体检（订正前 → 冻结 `src/` 实值）

覆盖区间 `castle.rs:1-175` 内 **49 条 distinct 锚全部命中**（`10-anchor-audit.txt`，
脚本 `E:/tmp/m3cs-work/anchor_audit.py`，只读冻结 `src/`）。订正的漂移：

| castle.rs 位置 | 原（漂移） | 订正为 | oracle 实证 |
| --- | --- | --- | --- |
| 模块头 `UserCastle::run` | `:311-448` | `:311-446` | `:311 public void Run()` / `:446 }` / `:448 public void Save()` |
| 模块头「跨天重置」等 4 处 | 裸 `:328-339` / `:340-376` / `:377-389` / `:391-440` | `UserCastle.cs:NNN` | 同上（裸锚在跨文件段落里会被误读） |
| 模块头 `start_wallconquest_war` | `:564-575` | `:564-572` | `:564 public void StartWallconquestWar()` / `:572 }`（`:574-575` 是下一段的文档注释） |
| 模块头 `stop_wallconquest_war` | `:577-600` | `:577-597` | `:577 public ...` / `:597 }`（`:599` 已是 `WithDrawalGolds`） |
| 模块头 `is_member` | 裸 `:480-483` | `UserCastle.cs:480-483` | `:480 public bool IsMember(PlayObject member)` |
| 模块头 `check_in_palace` | `:680-706` | `:680-705` | `:680 public bool CheckInPalace(int nX, int nY)` / `:705 }`（`:707` 是 `(IPlayerActor)` 重载） |
| N1 表 `get_castle_n1` 调用点 | `PlayObject.Message.cs:330` | `:329` | `:329 castle.GetCastle(MyGuild);`（`:330` 是 `SendServerGroupMsg`） |
| N1 表 `can_get_castle_n1` 调用点 | `PlayObject.Message.cs:328` | `:327` | `:327 castle.CanGetCastle(MyGuild)` |
| N1 表 `is_attack_guild_n1` 调用点 | `PlayObject.Message.cs:326` | `:325` | `:325 castle.IsAttackGuild(MyGuild)` |
| `MAX_CASTLE_ARCHER` 注释 | `CastleConst.cs:11` | `:10` | `:10 public const int MaxCastleArcher = 12;`（`:14` 才是 `MaxCalsteGuard`） |
| `CastleSiegeConfig::default` 注释 | 裸 `:1782` | `GameSvrConf.cs:1782` | `:1782 CastleName = "沙巴克";` |
| 后文 `UserCastle::run`（⑧-8 段） | `UserCastle.cs:311-448` | `:311-446` | 同 §2 第 1 行 |
| 后文 `start_wallconquest_war`（⑧-8 段） | `UserCastle.cs:564-575` | `:564-572` | 同上 |
| 后文 `stop_wallconquest_war`（⑧-8 段） | `UserCastle.cs:577-600` | `:577-597` | 同上 |
| 后文 `check_in_palace`（⑧-8 段） | `UserCastle.cs:680-706` | `:680-705` | 同上 |

后 4 条在**覆盖区间之外**（⑧-8/⑧-9 段落），门不覆盖它们；本片一并对齐以保证 `castle.rs`
内部不自相矛盾（同族前科：⑧-6 第②片 reviewer-6 M-1「同一文件里两种行号」）。

## 3. 判据与读数

| 项 | 命令 | 读数 | 证据 |
| --- | --- | --- | --- |
| castle 锚门 check | `python -I castle_oracle_anchor_gate.py check` | **GREEN 82 锚** | `01-castle-anchor-gate.txt` |
| castle 锚门 selftest | `... selftest` | **GREEN**（表结构 + 读侧 token + 写侧缺锚 + 3 条覆盖面牙齿 + 真身覆盖面） | `01` |
| castle 锚门 mutation | `... mutation` | **GREEN**（MUT1/2/4/5/6 全 RED，MUT3 真身 GREEN） | `01` |
| 链深检门 selftest | `python -I fixture_chain_gate.py selftest` | **GREEN 26/26**（原 19 + 本片 7） | `02` |
| 链深检门 check | `python -I fixture_chain_gate.py check` | **FIXTURE_CHAIN GREEN**（runner_ok=10，red=0；`castle_anchor_selftest`/`castle_anchor_check` 均 `present rc=0`） | `02` |
| **改坏必红** | 改 `castle.rs` 的 `:311-446`→`:311-448` | ① 门 `rc=1`（写侧缺锚 + 覆盖面未登记锚）② 链深检门 `rc=1`（两主机门点名，red=2）；还原后回 GREEN | `03-redcheck.txt` |
| 锚体检 | `python -I E:/tmp/m3cs-work/anchor_audit.py` | **50 锚 / 49 distinct / unresolved=0** | `10-anchor-audit.txt` |
| 四门禁（最终树） | fmt / clippy / test / msgcodegen | **fmt 0 / clippy 0 (-D warnings) / 101 suites 1199 passed 0 failed 22 ignored / msgcodegen 604** | `04`–`07` |
| 证据 hash | `verify_sha256sums.py verify --check-extra` + `eolcheck` | 见 `09-hash-check.txt` | `09` |

**L 级判据自证（等价 + 抽样冒烟）**：
- **等价**：`.rs` 差量经机判过滤（`08-boundary.txt` 末段）**无任何非注释行** ⇒ 零代码语义变更；
  两个 `.py` 差量是**新增门/牙齿与注册**，不改既有夹具判定（`02` 的 `check` 仍 GREEN、10 个夹具
  `RUN-OK` 全同）。
- **抽样冒烟**：进程级 — `fixture_chain_gate.py check` 真跑（跑通全部 10 个 READY 夹具的
  `-SelfCheckNoStack` + 4 条共享管线/主机门，`03` 证明该链对被审锚有牙齿）。

## 4. 未做 / 已知残留（如实登记，勿当成已关）

- **⑧-10 `openmir2-m3-guild-castle-fixture-botsrv` 的 M-1「锚行号」未在本片修**：
  `guild_castle_scenario.json` 的 `oracle_anchors` 里至少 2 条与冻结 `src/` 漂移 ——
  `"castle_run": "UserCastle.cs:311-448"`（应为 `:311-446`）、
  `"castle_door": "UserCastle.cs:890-910"`（应为 `:890-909`）；其余（`guild_*`/`castle_tax` 等
  ~40 条）**尚未逐条核**。属 ⑧-10 夹具卡的写集，建议由该卡收口片/后续夹具体检片统一机判
  （可复用本门的 `ANCHOR_RE` 口径），本片不越卡改。
- **⑧-8 / ⑧-9 段落的锚未纳入覆盖区间**（历史锚多，`castle_oracle_anchor_gate.py` 里已写明
  「逐条核另开片」）：本片只订正了后文与 ⑧-7 头部重复的 4 处；其余未核。
- **⑧-6 卡级 DoD 未变**：① `bins/game-svr` 接线（等 owner 对 bins 串行序裁决）
  ② 真机两侧 `run_guild_castle_pair.ps1` 同序列 `diff=0`（等 owner 独占窗口）。本片不涉。
- 门本身不校验**写侧锚的位置/邻接**（只判存在性 + 覆盖区间登记）——reviewer-5 已认可该口径
  （读侧逐行 token + 双牙齿已咬住两类真实漂移），本片维持。

## 5. 文件

```
00-commands.txt              全部命令（含 worktree/base/target）
01-castle-anchor-gate.txt    castle 锚门 check/selftest/mutation
02-fixture-chain-gate.txt    链深检门 selftest 26/26 + check GREEN（含主机门段）
03-redcheck.txt              改坏必红（门 RED + 链深检门 RED，还原回 GREEN）
04-gate-fmt.txt / 05-gate-clippy.txt / 06-gate-test.txt / 07-gate-msgcodegen.txt   四门禁
08-boundary.txt              净差量 / 冻结面空 / .rs 纯注释机判
09-hash-check.txt            证据 hash（repo LF/git-blob 口径 + eolcheck）
10-anchor-audit.txt          覆盖区间锚 → 冻结 src/ 实际行（49 distinct）
SHA256SUMS.txt               本目录清单（仓库口径）
```
