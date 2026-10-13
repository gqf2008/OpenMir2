# evidence-m3-hero-pet-anchor-20261014

M3⑨-5 `openmir2-m3-hero-pet-fixture-botsrv` 的**残余 Minor 收口片**证据包（离线、零起栈、零端口）。

## 收口的三条 Minor（来源）

| id | 提者 | 内容 | 本片处置 |
| --- | --- | --- | --- |
| M-1 | reviewer-5（⑨-5 approve，`b60285d7`） | `hero_pet_scenario.json` 的 `oracle_anchors.send_spell` 写 `RobotPlayer.cs:3968` 而该行是无关代码（定义/调用点行号互不一致） | `send_spell` 改为**只锚函数名**（工具侧 BotSrv 不给行号，见下口径）；并**逐条复核全部锚**，另抓出两条漂移：`frames`（把消息号当行号）与 `ProcessSlaveGhost` 起点 |
| M-2 | reviewer-5（同上） | scenario 7 个 case 只覆盖 17/20/30/41，无 46（`MagMakeClone`） | 新增 `make_clone` case（observe / summon 46 / observe）；判据 = `slave_count` 前后不变 + 帧序列，`scenario-fixed-input` 自检点名该 case |
| M-3 | reviewer-6（⑨-5，`4c656156`） | peer 护栏 `baseline.json` 坏 JSON ⇒ RED 的分支只 code-verified，无 `-SelfCheckNoStack` 用例 | 第 ⑧ 组新增 `peer-baseline-invalid-red`（半写 JSON ⇒ `failed` 含 `peer_baseline_invalid` 且不异常中断） |

## 锚门口径（`hero_pet_anchor_gate.py`）

* **冻结 oracle**（`src/M2Server/**`、`src/OpenMir2/**`）⇒ 锚**行号**入表机判：覆盖面（场景锚 ⇄ REGISTERED 双向闭合）+ 每条 `(line, token)` 命中冻结 `src/` 行。27 锚 / 57 行点。
* **夹具工具侧**（`src/BotSrv/**`）⇒ 只锚**函数/字段名**、不给行号。理由：BotSrv 是 T-41..T-46 的**在途工具面**（另有 ④ 掉落/经验夹具正在 `src/BotSrv/**` 写入），行号会合法漂移；把工具侧行号钉进 fail-closed 门会让**别人合法的 BotSrv 改动**把门打红。这类锚不是 oracle 判据。
* 门内 `selftest` 含 5 类牙（表结构 / 读侧 token / 覆盖面未登记 / 覆盖面表过期 / 越界 + 真身 check）；`mutation` MUT1/MUT2/MUT3 必红、MUT4 真身必绿。

## 读数（本片实跑，原始件见同目录）

| 项 | 读数 | 原始件 |
| --- | --- | --- |
| 锚门 check | GREEN 27 锚 / 57 行点，rc=0 | `02-anchor-gate-check.txt` |
| 锚门 selftest | SELFTEST GREEN，rc=0 | `03-anchor-gate-selftest.txt` |
| 锚门 mutation | MUTATION GREEN（MUT1/2/3 RED、MUT4 GREEN），rc=0 | `04-anchor-gate-mutation.txt` |
| 夹具无栈自检 | `SELFTEST ok=23/23 failures=`，GREEN | `05-run-heropet-selfcheck.txt` |
| 四门禁 | fmt0 / clippy0(-D warnings) / 101 suites 1200 passed 0 failed 22 ignored（合并树读数；本目录 `08` 为分支上原始转录）/ msgcodegen 604 | `06`–`09` |
| 外部改坏必红 | `frames` 改回旧稿 ⇒ 锚门 RED 2 项；还原复绿 | `12-external-break-red.txt` |
| whitelist 计数复核 | ⑧-10 合并片把 T-42/T-43/T-44 误写成 22/22；实测三 runner 均 19/19，本片订正 | `13-whitelist-count-recheck.txt` |

## 顺带订正（本片透明登记，非静默改动）

⑧-10 锚门片（`8d19d4f0`，合并 `f51849a0b`）把 whitelist 里 T-42/T-43/T-44/T-46 的
`-SelfCheckNoStack` **19/19** 一并改成 **22/22**；但 22/22 只对 `run_guild_castle_pair.ps1`（⑧-10 确实加了 3 条
锚门）成立。`run_quest_pair.ps1` / `run_party_trade_pair.ps1` / `run_selloff_pair.ps1` 本轮实测均 **19/19**
（见 `13-whitelist-count-recheck.txt`）⇒ 本片把 T-42/T-43/T-44 订回 19/19，并去掉误加的「本片加 3 条场景锚门」字样；
T-46 保持 22/22。

## 边界

* 只改 `mir2-rs/tests/parity/m3/**`（夹具/证据）+ `mir2-rs/tests/parity/whitelist.md`（T-45 计数与证据指针）；**零 `.rs`**。
* 冻结面 `src/(非 BotSrv) sql MirClient mir2-rs/crates/protocol mir2-rs/crates/storage mir2-rs/bins` 空。
* 本卡 DoD②③（真机两侧 `run_hero_pet_pair.ps1` 同序列 diff=0）**不在本片范围**：等 ⑨-2/⑨-3 世界侧 + `openmir2-m3-clock-injection` + owner 独占窗口，卡保持 `in-progress`。

## tier

**L 级**（夹具/文档/离线判据；等价 + 抽样冒烟）：不触 RNG 抽数序列、时序、持久化落库、战斗数值；本片不占窗口。
