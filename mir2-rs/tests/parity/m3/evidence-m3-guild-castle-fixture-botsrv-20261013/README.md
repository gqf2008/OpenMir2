# evidence-m3-guild-castle-fixture-botsrv-20261013

卡：`openmir2-m3-guild-castle-fixture-botsrv`（M3 逐系统对齐 ⑧ 的承接卡 10；父卡 `openmir2-m3-guild-castle-align`）。
交付 = **夹具骨架**（比较器 + 入口 + 无栈自检 + BotSrv 最小驱动），**真机两侧同序列未跑**（等 ⑧ 实现卡 +
`openmir2-m3-clock-injection` 时间门 + owner 发窗口）。基线 `origin/master 15510a47`；head `e095734d`。

> **重交（解冲突）**：原 head `37dda4ee` 基于 `0dde01b7`，与已合入 master 的 ⑥-5/⑦-5/⑩-3 在 9 处共享文件
> （`src/BotSrv/**` + `tools/botload/run_bots.ps1` + `m3/{run_pair,gate_guards}.ps1` + `whitelist.md`）真冲突。
> 已 `git merge origin/master`，按**并集**解（保留 master 的 Quest/PairTrade/SellOff 驱动与管线，叠加本片
> GuildCastle；条件式 save-settle 门/Green 判据/`ValidateSet` 合并两侧项），BotSrv `dotnet build` 0 error、
> 四副入口 PowerShell 解析 PASS，并在合并树上**重跑本目录全部读数**。旧 approve 随 head 变化失效。

## 文件

| 文件 | 内容 | 判据 |
| --- | --- | --- |
| `00-commands.txt` | 全部命令 | — |
| `01-guildcastle-diff-selftest.txt` | `guild_castle_diff.py selftest` | 末尾 `SELFTEST ok=36/36`，rc=0 |
| `02-run-guildcastle-pair-selfcheck.txt` | `run_guild_castle_pair.ps1 -SelfCheckNoStack -SelfCheckOut` | 末尾 `SELFTEST ok=19/19`，rc=0 |
| `03-botsrv-guildcastleseq-selftest.txt` | `BotSrv.exe --selftest-guildcastle` | 末尾 `GUILDCSEQ SELFTEST GREEN`，rc=0 |
| `04-run-bots-selfcheck.txt` | `run_bots.ps1 -SelfCheckNoStack` | rc=0（含 guildcastle done/expected/完备性两向） |
| `05-gate-fmt.txt` | `cargo fmt --all -- --check` | rc=0（空） |
| `06-gate-clippy.txt` | `cargo clippy --offline --workspace --all-targets -- -D warnings` | rc=0 |
| `07-gate-test.txt` | `cargo test --offline --workspace` | `94 suites / 878 passed / 0 failed / 21 ignored` |
| `08-gate-msgcodegen.txt` | `cargo run --offline -p msgcodegen -- --check` | `604 条常量` 一致 |
| `09-boundary.txt` | `git diff --name-only` | `src sql MirClient` 仅 `src/BotSrv/**`（T-46 例外）；总差量为实现件 + 证据 |
| `10-external-break-comparator.txt` | 外部删掉 `compare()` 的 `branch` 比较后跑 selftest | `SELFTEST ok=35/36`，rc=1 |

## 复现

见 `00-commands.txt`。编译用 `CARGO_TARGET_DIR=E:/tmp/m3cs-gc-target`，在**本 worktree** 内；未起栈、未占端口。

## 口径

- 比较器：逐 `(bot, k)` 帧**种类序列**（严格顺序）+ 文本帧（750/753/756/767）逐字节 + 声明标量
  （`SM_GUILDADDMEMBER_FAIL(758.a)` 等 Recog 码）+ `guild_name`/`guild_rank_name` + `members_*` 多重集 +
  金币 + 背包多重集 + `branch`；两假人（会长/peer）集合与每 bot 的 k 集合必须两侧相同；fail-closed。
- 落库：oracle「行会/攻城不落库」（三库无 guild/guildmember/castle 表列）⇒ 两侧都不写；
  DB 侧判据由 `run_guild_castle_pair.ps1` 的 `scoped_diff --canonical` 承担（增量=0）。
  行会/攻城持久产物是**文件**（`GuildBase/**`、`Castle/**`），窗口期两侧同目录同种子比。
- BotSrv 例外：T-46（BotSrv 不在 oracle 七进程内；默认关逐字节不变；打开只写本假人 socket）。

## 未做（如实）

- **DoD②③ 真机两侧同序列 diff=0 未跑**：等承接卡 `-guild-file-store` / `-guild-state` / `-guild-war` /
  `-castle-file-load` / `-castle-siege-state` / `-castle-door-repair` / `-castle-gold-tax` 落地 +
  `openmir2-m3-clock-injection`（时间门）+ owner 发窗口；`-Side rust` preflight 现为 PREREQ。
- 建会/宣战/攻城申请/开关门/修门墙走行会 NPC（`CM_MERCHANTDLGSELECT`），依赖 ⑥-3 NPC 运行与地图内行会 NPC。
- oracle N1/N2/N3/N4/N5（盘点 §1.L）只登记、未修（硬约束 3 照搬）。
