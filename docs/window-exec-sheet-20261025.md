# 开窗当天操作单（window exec sheet）— 2026-10-25

- 卡：`openmir2-window-exec-sheet`（tier L，owner `openmir2-dev-2`，branch `card/window-exec-sheet`，base `14ec2e943`）。
- 定位：**owner 开窗当天照着做的单页**。数据来源（不复制原文，只指回）：
  `docs/window-closure-ledger-20261025.md`（台账 9 行）、`tools/window/run3-scenarios.json`（**23 条，含 2 条 blocked**）、
  `docs/window-kit-coverage-20261025.md` §2（P0/P1 缺口）、`docs/window-runbook-20261023.md` **§0.5（开窗前 60 秒自查）/ §6**、`docs/delivery-acceptance-runbook-20261025.md` §2/§3。
- **红线**（每条都在源文档有出处）：不跑 `E:/MirServer/start-all.ps1`、不用 `start-all -Stop`（会连 mysqld 一起停）、
  不假设任何未复算字段（含 `cleared`）、**登记型 PASS ≠ 已有真机证据**。

## ① 前置检查（全绿才开窗）

| # | 检查 | 命令 / 判据 | 不合格怎么办 |
|---:|---|---|---|
| 1 | 端口 | `powershell -File E:/MirServer/port-status.ps1` ⇒ **5000–7200 全 DOWN**、**mysqld 3306 UP** | 占用 ⇒ 定位后清进程；**不要跑 `start-all.ps1`**（窗口栈由 `run_batch`/`card_window` 自己起；`check_ports_free` 占用即 rc=2）。源：`delivery-acceptance-runbook` §3 B1 |
| 2 | 私栈前置 | `python -I -B tools/window/preflight_private_stack.py`（10 步，含 `castle_list_symmetry` / `envir_shadow_symmetry`，见该脚本 `--help`） | 按脚本输出修；`-CheckPinsOnly` 可只核钉住项 |
| 2b | 城表对齐 | preflight 的 `castle_list_symmetry` 必须 OK（两侧含 `M2GameSvr-hook` 影子内容/sha256 一致） | 不一致 ⇒ 按 `window-runbook` §0.9 用 `Copy-Item` 对齐（W-TAX-2 需两城 `0,1`），**不碰 `E:/MirServer`**；否则城堡税/多城项必假红 |
| 2c | 影子=基线 | preflight 的 `envir_shadow_symmetry` 必须 OK（`csharp-root/M2GameSvr/Envir` vs `M2GameSvr-hook/Envir` vs `rust-root` 逐文件 sha256；实测 642 文件 0 差异） | 不一致 ⇒ 用基线 Envir 覆盖影子（或删影子 `GameSvr.exe` 让部署重建）；否则 `--rng-log` 窗口会用被改过的 oracle 脚本（内容冻结风险） |
| 2d | 写后复核（G-2） | 夹具应用后/收工后**重跑 preflight**（或比对基线/影子 sha）；preflight 是**写前**检查，绿 ≠ 运行期一致（runbook §0.9） | 任一 Envir/影子 sha 变化 ⇒ 先还原再到 2c；`--rng-log` 下夹具必须同时写基线+影子（G-1） |
| 3 | 场景清单（**以 `--list` 实况为准**） | `bash tools/window/run_batch.sh --list` ⇒ **`场景数 23（blocked 2 ⇒ 入批 21）`**，#21/#22 标 `YES` | 数量不符 ⇒ 停（`run3-scenarios.json` 被改了）；blocked 计数不符 ⇒ 清单行的 `blocked` 字段被动过 |
| 4 | 门禁基线 | `python -I -B tools/goal/goal_gate.py --skip-tests --json`（全量版需低负载时段） | 红 ⇒ 先修再开窗（`window-runbook` §6） |

> blocked 语义（已实测）：**blocked 场景只在 `--list` 标注、不进批次**；普通批次自动跳过并打印 `blocked B ⇒ 入批 N−B`；
> 只有 `--only <该场景>` 点名才 rc=2（**预期**）。源：`tools/window/run_batch.sh:107-108 / :223-225`。

## ② 分轮次执行表（场景 → 卡片 W 项 → 期望证据 → 失败先看什么）

| 轮次 | run3 场景 | 关哪张卡的哪个 W 项 | 期望证据（落 `E:/tmp/window/...`） | 失败时先看 |
|---|---|---|---|---|
| W1 | #1–#17（`login_only` … `upgrade_retrieve`） | 台账 #2 `W-LF-1`（部分：既有角色进图腿）、#3 的相邻腿；其余为通用冒烟 | `card_window.py` 每轮 out 目录的 `frames.ndjson` + DB 九表快照 + `compare_runs.py` 输出 | `run_batch` 的 rc 双列（outer/compare）；探针日志 `probe.log` |
| W2 | #18 `gamepoint_exchange` | 台账 #3 **W-PD-1**（GAMEPOINT 落库 + `SM_GAMEGOLDNAME(5008)`） | 5008 的 `Param/Tag` 逐字节 + `characters.GamePoint` 前后值（9→0）+ 643 帧序 | 先看是否命中 `CHECKGAMEPOINT > 8`（`:522`）门；`SoftVersionDateEx == 0` 会导致根本不发 5008（`PlayObject.Base.cs:930-933`） |
| W3 | #19 `clearmapmon_q001` | 台账 #4 **W-CM-2**；W-CM-1 **只覆盖「MONGENEX 刷怪/同名多只/玩家切图」**——**清场与宠物/召唤物仅构造级，无真机证据**（引台账第 4 行） | **活性检查**：末步 `wait_idents [10]`（累计、**无 seq 下界**）只证「连接内曾出现 `SM_TURN`」，**不区分来源/时点，≠「那 2 只 `骷髅精灵5` 进图」**。数量/位置交给证据层：`SM_TURN` 帧的 `recog`(actor id)/`param`+`tag`(x,y) + 两侧 `frames.ndjson` 逐帧对拍 + 实体/AOI 差集；**不期望 `SM_DISAPPEAR(30)`**（Q001 无 MonGen 行，清的是空图） | **第 4 步 `wait_for 643` 超时**（第 3 步 `dlg_select @rw01` 的 1011 被 CanJmp 拒）⇒ 多半是前置**蛇毒×1 未预置**（`老人-1.txt` `[@main]` 的 `checkitem 蛇毒` 支才给 `@rw01` 链接 :20）；若已预置仍超时，再查 `[@rw01]` 页的 `@rw02` 链接(:35) 与被清怪是否在场 |
| W3 | #20 `dropgold_zero_draw` | 台账 #6 **W-DG-1** | `SM_GOLDCHANGED(653)` 恰一条 + `Recog` 同源 + **金币净差 0** + 地面无金币 + RNG 流位不前进 | 653 缺 ⇒ 没进 `ClientDropGold` 或命中前置门（SafeZone / `nGold < CanDropGold`，只发 `RM_MENU_OK`）；**金币差写成 100 是错的**（BUG-REPLICA 2：`DropGoldDown` 恒失败回滚） |
| W3 | #22 `castle_merchant_tax`（**blocked**） | 台账 #1 **W-TAX-2/3** | 见 ③：本轮不可跑（需配置覆盖通道 + 第二 placement + 600s 落盘） | — |
| W3 | #23 `castle_merchant_tax_potion` | 台账 #1 **W-TAX-1（买腿）** | 步序：`0153` → 643 → `@buy` → 645 → `CM_USERBUYITEM(1014, 金创药(小量))` → `wait_idents[650,200]` → `sleep 620000`；**期望买价 88、当场 `650.recog` = Gold 9912**、620s 后 `SabukW` 的 `TotalGold`/`TodayIncome` **各 +4** | 650 缺 ⇒ 买卖前置（金币/负重/门槛）不满足；620s 后文件未 +4 ⇒ `SaveTick` 10min 门或税目的地不对（先看台账 #1 的 W-TAX-2 前置） |
| —（不入批） | #21 `butch_two_draws` | 台账 #6 **W-DG-2** | 见 ③：本轮不可跑 | — |

**台账里未落到 run3 场景的卡**（窗口需手工/新夹具，按期执行见台账对应行）：
#1 商税（W-TAX-1..4，需城堡商人配置副本；**跑前先完成 ① 2b 的 `Castle/List.txt` 对齐**，否则 W-TAX-2 必然假红）、#3 的 `@back` 子项（部署非目标）、#5 T-127（`castle_zone`）、
#7/#8 castle-official(-actions)（W-CO-1..6，**W-CO-6 需 owner 批准测试专用城主注入**）、#9 msgqueue（W-MQ-1..4，含 `getback` 多页）。
源：`docs/window-closure-ledger-20261025.md` §1。

## ③ 本轮明确「不可跑」（不许含糊成「视情况」）

| 项 | 为什么不可跑 | 解封条件 |
|---|---|---|
| `butch_two_draws`（run3 #21，清单标 `YES`） | **未跑真机整轮**——夹具、C# RNG 接线（`15dbbe843`）、**场景击杀步均已落地**（实验轮 `kill result=dead`、`matched_ident=34`，`E:/tmp/window/butch-run2`）；缺的只是**账本两抽**的窗口证据 | 跑一轮真机并采到账本（工具与清单见 `69bffc9b0`）：`Operate.cs:722 Random(16)` 与 `:723 Random(201)` **各 1 条**（**按 site 过滤**——真机账本里 `max=201` 另有 206 条来自 `MonGen.cs:815`）、`max=16` 在前、值域 `[5,20]`/`[100,300]`；**`SM_BUTCH(637)` 只作伴随**（`:749` 无条件发，不是成功判据）；跑完先看 `rng_log_trim.trimmed=false`（台账被裁 ⇒ 证据作废、**不得判 FAIL**）。判据工具 `tools/window/butch_ledger.py`、执行清单 `docs/butch-ledger-run-checklist-20261025.md`；源卡 `openmir2-butch-two-draws-unblock` |
| `clearmapmon_t118`（**不在 run3**；**已定性永久不可达**，截至 `92682d3fb`） | ① `[@main]` 要 `checkitem 佛牌 1` + `checklevel 43`，但三库 `stditems` 均无「佛牌」、`characters.Level=1` 且 `ASSETS_COLUMNS` 无 `Level` ⇒ `@rw01` 对正常客户端**不可达**；② 同 seed（42/43）跨全新起栈 `movr` 实抽仍漂移 ⇒ 无稳定赢页 seed。**替代判据**：W-CM-1 = `clearmapmon_q001` 真机（#19）+ t118 构造级/单测覆盖（见台账第 4 行）。原 blocking 记录保留为历史 |

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
