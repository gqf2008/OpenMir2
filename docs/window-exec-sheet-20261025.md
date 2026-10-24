# 开窗当天操作单（window exec sheet）— 2026-10-25

- 卡：`openmir2-window-exec-sheet`（tier L，owner `openmir2-dev-2`，branch `card/window-exec-sheet`，base `14ec2e943`）。
- 定位：**owner 开窗当天照着做的单页**。数据来源（不复制原文，只指回）：
  `docs/window-closure-ledger-20261025.md`（台账 9 行）、`tools/window/run3-scenarios.json`（21 条，含 1 条 blocked）、
  `docs/window-kit-coverage-20261025.md` §2（P0/P1 缺口）、`docs/window-runbook-20261023.md` **§0.5（开窗前 60 秒自查）/ §6**、`docs/delivery-acceptance-runbook-20261025.md` §2/§3。
- **红线**（每条都在源文档有出处）：不跑 `E:/MirServer/start-all.ps1`、不用 `start-all -Stop`（会连 mysqld 一起停）、
  不假设任何未复算字段（含 `cleared`）、**登记型 PASS ≠ 已有真机证据**。

## ① 前置检查（全绿才开窗）

| # | 检查 | 命令 / 判据 | 不合格怎么办 |
|---:|---|---|---|
| 1 | 端口 | `powershell -File E:/MirServer/port-status.ps1` ⇒ **5000–7200 全 DOWN**、**mysqld 3306 UP** | 占用 ⇒ 定位后清进程；**不要跑 `start-all.ps1`**（窗口栈由 `run_batch`/`card_window` 自己起；`check_ports_free` 占用即 rc=2）。源：`delivery-acceptance-runbook` §3 B1 |
| 2 | 私栈前置 | `python -I -B tools/window/preflight_private_stack.py`（9 步，含 `castle_list_symmetry`，见该脚本 `--help`） | 按脚本输出修；`-CheckPinsOnly` 可只核钉住项 |
| 2b | 城表对齐 | preflight 的 `castle_list_symmetry` 必须 OK（两侧含 `M2GameSvr-hook` 影子内容/sha256 一致） | 不一致 ⇒ 按 `window-runbook` §0.9 用 `Copy-Item` 对齐（W-TAX-2 需两城 `0,1`），**不碰 `E:/MirServer`**；否则城堡税/多城项必假红 |
| 3 | 场景清单 | `bash tools/window/run_batch.sh --list` ⇒ **`场景数 21（blocked 1 ⇒ 入批 20）`**，#21 标 `YES` | 数量不符 ⇒ 停（`run3-scenarios.json` 被改了）；blocked 计数不符 ⇒ 清单行的 `blocked` 字段被动过 |
| 4 | 门禁基线 | `python -I -B tools/goal/goal_gate.py --skip-tests --json`（全量版需低负载时段） | 红 ⇒ 先修再开窗（`window-runbook` §6） |

> blocked 语义（已实测）：**blocked 场景只在 `--list` 标注、不进批次**；普通批次自动跳过并打印 `blocked B ⇒ 入批 N−B`；
> 只有 `--only <该场景>` 点名才 rc=2（**预期**）。源：`tools/window/run_batch.sh:107-108 / :223-225`。

## ② 分轮次执行表（场景 → 卡片 W 项 → 期望证据 → 失败先看什么）

| 轮次 | run3 场景 | 关哪张卡的哪个 W 项 | 期望证据（落 `E:/tmp/window/...`） | 失败时先看 |
|---|---|---|---|---|
| W1 | #1–#17（`login_only` … `upgrade_retrieve`） | 台账 #2 `W-LF-1`（部分：既有角色进图腿）、#3 的相邻腿；其余为通用冒烟 | `card_window.py` 每轮 out 目录的 `frames.ndjson` + DB 九表快照 + `compare_runs.py` 输出 | `run_batch` 的 rc 双列（outer/compare）；探针日志 `probe.log` |
| W2 | #18 `gamepoint_exchange` | 台账 #3 **W-PD-1**（GAMEPOINT 落库 + `SM_GAMEGOLDNAME(5008)`） | 5008 的 `Param/Tag` 逐字节 + `characters.GamePoint` 前后值（9→0）+ 643 帧序 | 先看是否命中 `CHECKGAMEPOINT > 8`（`:522`）门；`SoftVersionDateEx == 0` 会导致根本不发 5008（`PlayObject.Base.cs:930-933`） |
| W3 | #19 `clearmapmon_q001` | 台账 #4 **W-CM-2** | `SM_DISAPPEAR(30)` 帧序 + 实体/AOI 差集 + `MONGENEX` 刷怪位置/数量 | **第 4 步 `wait_for 643` 超时**（第 3 步 `dlg_select @rw01` 的 1011 被 CanJmp 拒）⇒ 多半是前置**蛇毒×1 未预置**（`老人-1.txt` `[@main]` 的 `checkitem 蛇毒` 支才给 `@rw01` 链接 :20）；若已预置仍超时，再查 `[@rw01]` 页的 `@rw02` 链接(:35) 与被清怪是否在场 |
| W3 | #20 `dropgold_zero_draw` | 台账 #6 **W-DG-1** | `SM_GOLDCHANGED(653)` 恰一条 + `Recog` 同源 + **金币净差 0** + 地面无金币 + RNG 流位不前进 | 653 缺 ⇒ 没进 `ClientDropGold` 或命中前置门（SafeZone / `nGold < CanDropGold`，只发 `RM_MENU_OK`）；**金币差写成 100 是错的**（BUG-REPLICA 2：`DropGoldDown` 恒失败回滚） |
| —（不入批） | #21 `butch_two_draws` | 台账 #6 **W-DG-2** | 见 ③：本轮不可跑 | — |

**台账里未落到 run3 场景的卡**（窗口需手工/新夹具，按期执行见台账对应行）：
#1 商税（W-TAX-1..4，需城堡商人配置副本；**跑前先完成 ① 2b 的 `Castle/List.txt` 对齐**，否则 W-TAX-2 必然假红）、#3 的 `@back` 子项（部署非目标）、#5 T-127（`castle_zone`）、
#7/#8 castle-official(-actions)（W-CO-1..6，**W-CO-6 需 owner 批准测试专用城主注入**）、#9 msgqueue（W-MQ-1..4，含 `getback` 多页）。
源：`docs/window-closure-ledger-20261025.md` §1。

## ③ 本轮明确「不可跑」（不许含糊成「视情况」）

| 项 | 为什么不可跑 | 解封条件 |
|---|---|---|
| `butch_two_draws`（run3 #21，清单标 `YES`） | ① 无「单只固定动物尸体」夹具（`card_window` 只能 placement 玩家）；② RNG 面未接线（`-RngLog` + `MIR2_RNG_SITE=1` + C# hook） | ① 尸体夹具落地（tester 的 drop/butch 夹具片）；② 卡 `openmir2-window-csharp-rng` 合入并在窗口 C# 栈接 `start-csharp-hook.ps1` + `RngSeedHook.dll`，两侧同 `-Seed N` + `MIR2_RNG_SITE=1`。源：`tools/probe/scenarios/butch_two_draws.json` 的 `blocked` |
| `clearmapmon_t118`（**不在 run3**） | `[@st]` 的 `movr M3 3`（`武士-0114.txt:27`）分支随机；M3 不可注入；且输分支会扣佛牌 + 写 `命运之刃名单.txt`（`:37-43`） | ① C# 栈接 hook/seed；② 两侧同 `-Seed N`，用 `MIR2_RNG_SITE=1` 流水确认该 seed 下 `movr` 取 **2**；③ 每次尝试前恢复夹具（佛牌×1 + 从名单移除角色）；④ 记 seed 号后由 owner 裁决入批。源：`docs/window-scenes-clearmapmon-20261025.md` §1.6 |

**P0/P1 缺口的当前状态**（源：`docs/window-kit-coverage-20261025.md` §2 + 各修复卡）：

| 级别 | 缺口 | 现状 |
|---|---|---|
| P0 | `assets` 白名单只有 `Gold` | **部分已解**：`GamePoint` 已入白名单（`aa94a89db`）；`gamepoint_exchange` 的 `assets` 键回填后的写法见 `97eb5365d` |
| P0 | `--rng-log` 能力与 C# 侧 rng-hook | **部分已解**：`--rng-log` 工具面已贯通（`aa94a89db`：`card_window.py` 注入 `MIR2_RNG_LOG` + `check_rng_ledger`、`run_batch.sh` 透传、`start_mir2_server.ps1` 接 `-RngLog`）；**C# 侧缺 rng-hook 部署**（`DOTNET_STARTUP_HOOKS` + `RngSeedHook.dll`，卡 `openmir2-window-csharp-rng`）⇒ 现在传 `--rng-log` 会让 C# 侧 `check_rng_ledger` 失败（`card_window.py:3583-3590`）；hook 落地后两侧账本才能逐位对拍。与 `delivery-acceptance-runbook` **B5**（带 RNG 账本的场景用 `--rng-log`）对齐：**能力已有、C# 侧待 hook** |
| P0 | C# 无 `MIR2_CLOCK_*` | **未解**（W-TAX-4/W-T127-2 只能真实时间点复拍并登记降级） |
| P1 | 无 castle fixture（#1/#5/#7/#8 需手工或等夹具）、**`CM_NEWCHR` 仍缺**、`@back` 字面不存在 | `@back` 已定性为**非目标**；**W-LF-1 的新建角色腿本轮跑不了**（probe 无 `newchr` 步骤 ⇒ 只能窗口手工建号；卡 `openmir2-probe-newchr-step`，dev-1） |

## ④ 收窗与回填

1. **证据落点**：每轮 `card_window.py` 的 out 目录（`E:/tmp/window/<run>/<scenario>/`），保留 `frames.ndjson`、DB 快照、`compare_runs.py` 输出；
   抓帧模板见各卡报告（如 `docs/dropgold-butcheffect-rng-20261025.md` §5、`docs/clearmapmon-impl-20261025.md` §4）。
2. **台账回填**：逐卡填证据路径 / 帧 sha256 / DB 快照；**只填证据 + 在卡线程签 `comment`**，
   **不要把台账状态写成 `verified`**（不在 ledger 词表；`closed` 由 `delivery-acceptance-runbook` 的 **B7** 判定）。
3. **收窗**：停两侧栈（**不含 mysqld**）→ `port-status.ps1` 复核 5000–7200 恢复 DOWN → 证据清单进卡 comment。
4. **谁定 closed**：`window_required` 的 R 卡由 owner/coordinator 按 B7 判据收口；作者不自签。

## ⑤ 红线（重复一遍，逐条有源）

- **不跑** `E:/MirServer/start-all.ps1`、**不用** `start-all -Stop`（会连 mysqld）、不要动 mysqld：`-Stop` 的原话在 `docs/window-runbook-20261023.md:145`（§0.5 的退路说明）；`delivery-acceptance-runbook` §3 B1 只写「不要跑 `start-all.ps1`」「不要动 mysqld」。
- **不假设** `cleared` 等未复算字段；清怪/刷怪证据 = 实体/AOI 差集 + 帧序（`window-kit-coverage` §1.4）。
- **不把**「登记型 PASS」或「构造级断言已绿」当成真机证据（`window-closure-ledger` §1 注）。
- 场景数/blocked 数以 `run_batch --list` 实况为准，**不写死**（`run_batch` 已去硬编码，`2135f6154`）。

## 附：本单与源文档的对应

| 本单章节 | 源 |
|---|---|
| ① 前置 | `docs/delivery-acceptance-runbook-20261025.md` §3（B1/B2/B3）；`docs/window-runbook-20261023.md` §6 |
| ② 轮次表 | `docs/window-closure-ledger-20261025.md` §1/§2；`tools/window/run3-scenarios.json` |
| ③ 不可跑 | 场景 JSON 的 `blocked`；`docs/window-scenes-clearmapmon-20261025.md` §1.6 |
| 缺口 | `docs/window-kit-coverage-20261025.md` §2 |
| ④ 收窗 | `docs/delivery-acceptance-runbook-20261025.md` §3（B6/B7/B10） |
