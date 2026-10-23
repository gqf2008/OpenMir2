# OpenMir2 重写剩余量清单（可复现盘点）

- 采集时间：第 1 轮 2026-10-22T14:34Z；第 2 轮 2026-10-22T14:46Z；第 3 轮 2026-10-22T15:10Z；
  **第 4 轮（本文件当前版本，2026-10-23）按 recon 门在 master `6d79be4da` 上重算全部数字**
- 采集点：第 4 轮 worktree `E:/tmp/wt-worker-40`（分支 `card/inventory-refresh`），基线 `6d79be4da`
  （含 `73d84e33a` 窗口欠账重分档、`6d79be4da` 孤儿编码器触发源审计）
- 复核输入：本卡 `openmir2-inventory-refresh` 的 issue（Leader 2026-10-23 实测清单）；历史复核见 §9/§10
- 只读面：`src/**`、`sql/**`、`E:/MirServer/**`；第 4 轮未起服务端/客户端进程、未占端口、未写 DB
- 口径：**每个数字后面给「命令 → 当 HEAD 实测输出摘要」**；估算一律标 `估算`，不计入结论。
  **三条一步到位的复算命令见 §11「复核方式」**。

> **一句话结论（第 4 轮，master `6d79be4da`）**：脚本引擎在**部署语料**上仍 100% 覆盖（106/106、0 未实现）。
> 真正的剩余量集中在四处：
> **① 窗口欠账**：152 张 A 卡的机器分档 = **①57**（48 可进池 + 9 前置未清）/ **②45**（8 模板 → 45 次参数化 run）/ **③50**（③-G 1 + ③-O 27 + ③-N 22）；
> **② GM**：185 条表里 **88 条未实现**，另有 **21 条第三态**（文本逐字节、宿主动作未接），完整实现 **76 条**；
> **③ 协议**：入站 tracked **2**（`CM_ADJUST_BONUS` 待接线；`CM_OPENDOOR` 已判结构性不可达 T-65）；
> 出站 tracked **13** = **12 个孤儿编码器**（审计三态：0 可直接接线 / 8 需补上游 / 4 不可达）+ **`SM_DONATE_FAIL`（已判非缺陷）**；
> **④ 内容**：`rust=no` 实现缺口 **0**（`UnbindList`、`AdminList` 均已接；tracked 3 条均 `rust=yes`，只是卡未关闭）。

> **第 4 轮相对第 3 轮的差异（旧数字 → 新数字）**：GM 已实现 35→**97**（完整 14→**76**、第三态 21 不变、未实现 150→**88**）；
> 协议 implemented 170→**172** / tracked 17→**15**（`CM_WANTMINIMAP`、`SM_CLOSEHEALTH` 已接）；
> 内容 implemented 11→**13** / tracked 5→**3**（UnbindList、AdminList 已接）；
> board 进行中 185→**203** / 已完成 153→**169**；A 152 由「人工四分类」改为**机器分档 ①57/②45/③50**。

---

## 0. 汇总数字（全部可复现）

| 维度 | 总量 | 已完成 | 未完成/欠账 | 证据 |
|---|---:|---:|---:|---|
| 脚本派发目标（注册 handler + switch 分支） | 274 | 147（118 handler 名 + 29 switch） | **124** = 123 唯一 handler 名（条件 62 + 动作 61）+ switch `TakecheckItem` 1 | §1.1 |
| 脚本：部署语料实际用到的派发目标 | 106 | **106** | **0** | §1.2 |
| GM 命令（185 条表 + 内建 2） | 187 | 76 完整 + **21 第三态** + 2 内建 | **88 未实现**（文本已逐字节 / 宿主动作未接） | §2 |
| 协议派发（入站 + 出站） | 191 | 172 | **15 tracked**（入站 2 + 出站 13）+ 3 n/a-unreachable + 1 n/a-oracle-noop | §4.1 |
| 内容加载调用点 | 40 | 13 implemented | **3 tracked**（均 `rust=yes`）+ 24 n/a；`rust=no` 实现缺口 **0** | §4.1 |
| 协作卡（board 实读，快照 `6d79be4da` @ 2026-10-23T07:31+08:00） | 377 | 169 已完成 | 203 进行中 + 5 待审（0 需要人 / 0 受阻 / 0 待认领） | §3.1 |
| A 窗口欠账（152 张 A 卡集合的机器分档） | 152 | — | **①57**（48 可进池 + 9 前置未清）/ **②45**（8 模板）/ **③50**（③-G 1 + ③-O 27 + ③-N 22） | §3.1 + `docs/window-debt-reclass-20261023.md` |

> 计数口径注（避免 147+124≠274 的疑问）：274 = 244 个 handler **code**（条件 111 + 动作 133）+ 30 个 switch 分支；
> handler 数组里有 3 处重名（条件 1 处、动作 2 处），所以**唯一 handler 名 = 241**。
> 147（已实现）+ 124（未实现）= 271，再加上 3 处重名 = 274。

---

## 1. 脚本引擎派发面（`crates/script`）

### 1.1 注册 / 已实现 / 未实现

> 口径说明：设计文档与 `dispatch.rs` 模块头提到的 `coverage_report` **在当前 HEAD 不是函数名**——
> `grep -rn coverage_report crates/script` 只命中 `dispatch.rs:7` 的文档注释。实际实现拆成
> `dispatch.rs` 的三态查询（`condition_target_status`/`execution_target_status`）+
> `crates/script-tool/src/parsestats.rs::coverage_lines`。本节的数字按后者复算。

| 类别 | 注册表（code / 唯一名） | 已实现 handler | 未实现 handler（唯一名） | switch 分支（注册/已实现） | 目标合计（已实现/总） |
|---|---:|---:|---:|---:|---:|
| 条件 | 111 / 110 | 48 | **62** | 10 / 10 | 58 / 121 |
| 动作 | 133 / 131 | 70 | **61** | 20 / 19 | 89 / 153 |
| 合计 | 244 / 241 | 118 | **123** | 30 / 29 | **147 / 274** |

> 计数口径：注册表按 **code** 数（条件 111、动作 133）；`CONDITION_HANDLERS` 有 1 个 handler 名复用到 2 个 code、
> `EXECUTION_HANDLERS` 有 2 个名字复用 ⇒ **唯一 handler 名 = 241**。上表「已实现/未实现」按**唯一 handler 名**计。
> 目标合计按 code 口径：条件 111（handler）+10（switch）=121；动作 133+20=153；合计 274。
> **第 4 轮变化**：`IMPLEMENTED_EXECUTIONS` 由 69→**70**（新增 `ActionOfSetMapMode`，见 §5.2 A-21 订正），
> 因此动作未实现由 63→**61**；条件未实现 62 不变。

唯一的 **switch 未实现项 = `TakecheckItem`**（`ENGINE_SWITCH_EXECUTIONS` 20 条中缺 1；
`set(ENGINE_SWITCH_EXECUTIONS) - set(IMPLEMENTED_SWITCH_EXECUTIONS)` ⇒ `{'TakecheckItem'}`）。
它不在部署语料里（见 §1.2），所以不造成语料缺口。

未实现的 handler 名（唯一名，第 4 轮实测：条件 62 / 动作 61；完整清单）：

```
条件：ConditionOfAnsiContainsText, ConditionOfCheCkContAinsTextList, ConditionOfCheckAccountIpList,
        ConditionOfCheckCastleChageDay, ConditionOfCheckCastleDoorStatus, ConditionOfCheckCastleGold,
        ConditionOfCheckCastleWarDay, ConditionOfCheckChiefItemCount, ConditionOfCheckContribution,
        ConditionOfCheckDc, ConditionOfCheckDura, ConditionOfCheckExp, ConditionOfCheckFlourishPoint,
        ConditionOfCheckGuildAuraePoint, ConditionOfCheckGuildBuildPoint, ConditionOfCheckGuildList,
        ConditionOfCheckHp, ConditionOfCheckInMapRange, ConditionOfCheckIpList,
        ConditionOfCheckIsAttackGuild, ConditionOfCheckIsDefenseAllyGuild, ConditionOfCheckIsDefenseGuild,
        ConditionOfCheckIsMaster, ConditionOfCheckIsOnMap, ConditionOfCheckItemType,
        ConditionOfCheckLuckyPoint, ConditionOfCheckMC, ConditionOfCheckMagicLvl,
        ConditionOfCheckMapHumanCount, ConditionOfCheckMapMonCount, ConditionOfCheckMapName,
        ConditionOfCheckMarryCount, ConditionOfCheckMaster, ConditionOfCheckMemberType,
        ConditionOfCheckMonMapCount, ConditionOfCheckMp, ConditionOfCheckNameIpList,
        ConditionOfCheckNameListPostion, ConditionOfCheckOfGuild, ConditionOfCheckOnlineLongMin,
        ConditionOfCheckPkPoint, ConditionOfCheckPos, ConditionOfCheckPoseIsMaster,
        ConditionOfCheckPoseMaster, ConditionOfCheckRangeMonCount, ConditionOfCheckSc,
        ConditionOfCheckServerName, ConditionOfCheckSkill, ConditionOfCheckSlaveCount,
        ConditionOfCheckSlaveLevel, ConditionOfCheckSlaveName, ConditionOfCheckStabilityPoint,
        ConditionOfCheckTextList, ConditionOfCompareText, ConditionOfDayOfWeek, ConditionOfDayTime,
        ConditionOfHaveMaster, ConditionOfHour, ConditionOfIsHigh, ConditionOfMin, ConditionOfPoseHaveMaster,
        ConditionOfReviveSlave
动作：ActionOfAddAccountList, ActionOfAddGuildList, ActionOfAddIpList, ActionOfAddNameDateList,
        ActionOfBonusPoint, ActionOfChangeGender, ActionOfChangeJob, ActionOfChangeNameColor,
        ActionOfChangePerMission, ActionOfClearList, ActionOfClearMakeItems, ActionOfClearNeedItems,
        ActionOfClearPassword, ActionOfClearSkill, ActionOfDelGuildList, ActionOfDelIpList, ActionOfDelMarry,
        ActionOfDelMaster, ActionOfDelNameDateList, ActionOfDelNoJobSkill, ActionOfDelSkill, ActionOfDivData,
        ActionOfExchangeMap, ActionOfExeaction, ActionOfGetMarry, ActionOfGetMaster, ActionOfGroupAddList,
        ActionOfGroupMoveMap, ActionOfGuildAuraePoint, ActionOfGuildBuildPoint, ActionOfGuildChiefItemCount,
        ActionOfGuildFlourishPoint, ActionOfGuildRecall, ActionOfGuildstabilityPoint, ActionOfKillSlave,
        ActionOfKillSlaveName, ActionOfMapTing, ActionOfMaster, ActionOfMonClear, ActionOfMonGen,
        ActionOfOpenMagicBox, ActionOfPercentData, ActionOfPkPoint, ActionOfPkZone, ActionOfQueryBagItems,
        ActionOfQuerySaleSell, ActionOfQueryTrustDeal, ActionOfRandomMove, ActionOfReCallMap,
        ActionOfRecallmob, ActionOfRestBonusPoint, ActionOfSetMemberLevel, ActionOfSetMemberType,
        ActionOfSetRandomNo, ActionOfSetRankLevelName, ActionOfSkillLevel, ActionOfSumData,
        ActionOfTakeCastleGold, ActionOfThrowitem, ActionOfUnMaster, ActionOfUpgradeItems
```

> 与第 3 轮清单的差：动作侧少 2 个——`ActionOfSetMapMode`（A-21，已实现）与旧稿的重名计数误差已按唯一名口径订正。

### 1.2 部署语料实测覆盖（关键判据）

```sh
cd mir2-rs
CARGO_TARGET_DIR=<任选空目录> \
  cargo run --release --locked -p mir2-script-tool -- parse-stats E:/MirServer/M2GameSvr/Envir \
  --json E:/tmp/parsestats.json --require-implemented
```

输出摘要（第 4 轮 master `6d79be4da` 实测，rc=0）：

```
Envir: E:/MirServer/M2GameSvr/Envir
文件: 639 加载: 639 缺失: 0 异常: 0
脚本 178 标签 2386 过程 3563 条件 2137 动作 8869 否则动作 953
解析错误: 427；解码异常文件: 0
未注册（C# 静默忽略）合计: 130 次；条件 distinct code 41 / 动作 distinct code 68
语料派发目标覆盖：已实现 106 个（11829 次调用）；未实现 0 个
```

- 语料用到的 106 个派发目标**全部已实现，未实现 = 0**（`--require-implemented` 退出码 0）。
- 130 次「未注册」是 C# 本身静默忽略的 code（不属于缺口）。
- 解析错误 427 行是**语料事实**（含 `SETOFFLINEPLAY`、`RecallHero` 等未注册命令），不导致 LoadPanic，
  也不影响派发覆盖结论（`panics=0`）。
- 注意：`--json` 输出不含 `cond_usage`/`act_usage`；覆盖结论以 stdout 的
  `语料派发目标覆盖` 行为准。

---

## 2. GM 命令面（`bins/game-svr/src/gmcmd.rs`）

### 2.1 计数

```sh
awk '/^pub const COMMANDS/,/^];/' bins/game-svr/src/gmcmd.rs | grep -c "name:"
# 185
awk '/^pub const COMMANDS/,/^];/' bins/game-svr/src/gmcmd.rs | grep -o "imp: Imp::[A-Za-z]*" | sort | uniq -c | head -3
#  88 imp: Imp::Unimplemented；其余 97 条各有具名 Imp
# 第三态（T-161 的 14 + T-163 的 7 = 21 条具名 Imp）：
awk '/^pub const COMMANDS/,/^];/' bins/game-svr/src/gmcmd.rs | grep -cE 'imp: Imp::(ReloadAllGuild|ReloadGuild|ReloadLineNotice|ReloadManage|ReloadRobotManage|ReLoadAdmin|LoadAdmin|ReloadItemDb|ReloadMagicDb|ReloadMonItems|ReloadRobot|ReloadAbuse|ReloadNpc|NpcScript|TestFire|FireBurn|TestStatus|BackStep|UserCmd|TestGetBagItems|TestSpeedMode),'
# 21
```

| 项 | 条数 | 证据 |
|---|---:|---|
| 命令表 `COMMANDS` | **185** | `grep -c "name:"` ⇒ 185；单测 `COMMANDS.len()==185` |
| 内建命令 | **2**（`@help`、`@commands`） | `BUILTIN_HELP`/`BUILTIN_COMMANDS`，表外处理 |
| 已实现命令体（具名 `Imp`） | **97** | `Imp::Unimplemented` = 88 ⇒ 185−88 = 97 |
| 　├ 完整实现 | **76** | 97 − 第三态 21 |
| 　└ **第三态：文本已逐字节、宿主动作未接** | **21**（T-161 的 14 + T-163 的 7） | `whitelist.md:709`（T-161）/ `whitelist.md:711`（T-163）；定义与脚注见 §2.2 |
| 未实现命令体 | **88** | 同上 |
| perm 0（现网可达） | **5** | `perm_min: 0` ⇒ `AddRobotPlay`/`LockLogin`/`拒绝`/`拒绝私聊`/`时间`；加内建 `@help` = 6 |

### 2.2 按 §4.3 的 12 族分解（精确、互斥）

```sh
# 解析 §4.3 表（185 行）× gmcmd.rs 的 (name, imp) 做 join；族取表格第 3 列。
# 复算脚本见 §8「复现脚本」join.awk。
# 输出摘要（每族 total/done/remain；done 再按 §2.1 的第三态 21 条拆成「完整 / 第三态」）：
```

| 族 | 总数 | 完整实现 | 第三态（文本级） | 未实现 |
|---|---:|---:|---:|---:|
| 数值/角色 | 44 | **44** | 0 | 0 |
| 其它 | 35 | 5 | 0 | **30** |
| 传送/召唤 | 17 | **17** | 0 | 0 |
| 行会/城堡/赛事 | 16 | 0 | 0 | **16** |
| 查询/展示 | 16 | 0 | 0 | **16** |
| 重载/管理 | 14 | 0 | **14**（T-161） | 0 |
| 造物/刷怪 | 9 | 0 | 0 | **9** |
| 封禁名单 | 9 | **9**（T-162） | 0 | 0 |
| 桩/测试 | 8 | **1**（G1 `AddRobotPlay`） | **7**（T-163） | 0 |
| 禁言/监听 | 7 | 0 | 0 | **7** |
| 仓库/登录密码 | 6 | 0 | 0 | **6** |
| 会话控制 | 4 | 0 | 0 | **4** |
| **合计** | **185** | **76** | **21** | **88** |

具名 `Imp` 的 97 条 = **完整实现 76** + **第三态 21**。按族：数值/角色 44（GM 第 4 批）、传送/召唤 17（第 3 批）、
封禁名单 9（T-162）、重载/管理 14（T-161 第三态）、桩/测试 8 = T-163 的 7（第三态）+ G1 的 `AddRobotPlay`、
其它 5（G1 四条 `时间/拒绝/拒绝私聊/LockLogin` + `传送`/`UserMoveXy`）。76 + 21 = 97；与 §2.1 的 `Imp::Unimplemented` = 88 合并即 185。

**第三态定义（本轮新增，回应 reviewer-14 第 2 轮唯一阻塞 Important）**：命令体已把 oracle 文本**逐字节**复刻
（`SysMsg` 文本、分支、越界怪癖照抄），但**宿主动作未接线**（`whitelist.md:709` T-161 的重载/管理 14 +
`whitelist.md:711` T-163 的桩/测试 7）。⇒ 这 21 条的真机可观测行为**只能是「回 oracle 文本 + 零副作用」**，
不含 oracle 会产生的后续状态变化（重读后内容刷新、事件入队、状态表写入/广播、`CharPushed` 推挤）。
该偏差**已按族登记**（作者 `openmir2-worker-4`，卡 `openmir2-gm-commands-design`，tier=L，待独立 principal 复核）；
21 条命令均 `PermissionMin = 10`，现网 `Permission` 恒 0（T-156 ③）⇒ 生产不可达。

> **脚注（避免把第三态当同质集合）**：21 条里真正存在「宿主副作用未接线」的是 **10 条**——
> T-161 的 ①2（`ReloadAllGuild`/`ReloadGuild`）、②1（`ReloadLineNotice`）、③2（`ReloadManage`/`ReloadRobotManage`）、
> ④1（`ReLoadAdmin`），T-163 的 ①2（`TestFire`/`FireBurn`）、②1（`TestStatus`）、③1（`BackStep`）；
> 另 10 条（T-161「无残余的 8 条」+ T-163 的 ④`UserCmd`、⑤`TestGetBagItems`）oracle 宿主本身是注释/no-op
> ⇒ 文本级即等价；余 1 条 `TestSpeedMode` 只翻会话字段 `test_speed_mode`（该字段当前无其它消费者，无宿主依赖）。
> 因此 **21 = 按族登记的第三态口径**，而**可观测缺口口径 = 10 条**；两个口径不可混用。

---

## 3. 卡 backlog（`walgit collab board --format json`）

### 3.1 总览（第 4 轮 board 实读）

```sh
walgit collab board --format json > E:/tmp/board.json
python -c "import json;d=json.load(open('E:/tmp/board.json',encoding='utf-8'));print({c['name']:len(c['cards']) for c in d['columns']})"
# {'待认领': 0, '进行中': 203, '待审': 5, '需要人': 0, '受阻': 0, '已完成': 169}
```

**board 快照（`6d79be4da` @ 2026-10-23T07:31+08:00）：203 进行中 + 5 待审 + 169 已完成（共 377）。**
**这是实时量，复跑会变**（复审当日实测已是 200 进行中 / 8 待审 / 170 已完成；卡随时移动），以跑命令当次输出为准。第 3 轮快照是「185 进行中 / 153 已完成」，
两者不可混用。**卡数只回答「有多少卡在飞」，不回答「还差多少行为」**；后者看 §0 的四行机器口径。

#### A 窗口欠账：152 张的机器分档（取代旧「A 152 / B 22 / D 8 / C 2 / O 1」人工四分类）

旧四分类的集合硬编码在 `/e/tmp/audit-inv-20261022/finalcls.py`（**不在仓库内、基于 185 卡快照**），
任何人 checkout 后都无法复算 ⇒ 第 4 轮起**不再作为本文计数**，只保留 §3.2 的历史逐卡快照。
当前权威口径是已合入 master `73d84e33a` 的 `docs/window-debt-reclass-20261023.md`：

| 档 | 卡数 | 定义 | 细分 |
|---|---:|---|---|
| **①** 现有场景已覆盖 | **57** | 现有双栈场景/`run_*_pair.ps1` 跑一次即可给证据 | **①-A 48 可直接进窗口池** + **①-B 9 前置未清（不进池）** |
| **②** 只差一个新场景 | **45** | 判据可协议判定，但没有现有场景发那串 `CM_*` | **8 个场景模板（R1–R8）→ 45 次参数化 run**（一卡一次 run） |
| **③** 协议对拍不可替代 | **50** | ③-G GUI 1 + ③-O 离线/日志/长跑/工具缺口 27 + ③-N 无窗口项原文 22 | ③-N 不派，先补 `window_items` |
| **合计** | **152** | 与旧「A 152」同一集合 | 57 + 45 + 50 = 152 |

复算：`docs/window-debt-reclass-20261023.md` §1/§3/§8 的脚本；本轮只引用其已合入数字，不重跑其分类。

#### B 类（未接线/未实现）的机器口径

手册（`docs/leader-goal-20261023.md` §1 条件3）把 B 类定义为「recon dispatch 的 `tracked` 清单」。
第 4 轮实测 **tracked 15**：入站 2 + 出站 13。其中**真正待接线**的只有：

- 入站 `CM_ADJUST_BONUS`（1 条，`lib.rs` 仍传 `bonus_point=0`，见 §6 #5）；
- 出站「需补上游生产者」8 帧（T-174/178/180/181/182/183，**均无卡、待按帧拆建**，见 §4.1 与 §6 #1）。

其余 6 条是**已判不可达/非缺陷但门仍计 tracked**：`CM_OPENDOOR`（T-65）、4 个孤儿帧（T-175/176/179）、
`SM_DONATE_FAIL`（非缺陷）。⇒ **B 类有效待办 = 1 + 8 = 9 条**，而不是 15 或旧稿的 14。

### 3.2–3.6 逐卡清单（历史快照，仅作参考）

> **⚠ 以下是 2026-10-22 的逐卡快照（当时 board = 185 进行中 / 153 已完成），不是当前状态。**
> 当前分档看 §3.1 的机器口径；卡的实时位置看 `walgit collab board`。
> 保留本节只为追溯「当时为什么这样分类」，其中的计数（A 152 / B 22 / D 8 / C 2 / O 1）**不要引用**。


### A. 真机窗口欠账（R 级缺证据）

| # | card id | owner/worktree | 阻塞原因（来自最新 status 的 work） |
|---:|---|---|---|
| 1 | `openmir2-action-line-variable-text` | openmir2-orchestrator@main/master | R 级窗口未跑：① C# SENDMSG 0 <$USERNAME>:<$STR(S1)> 151 235（QFunction-0.txt:388）② MESSAGEBOX 含链接在前/命名变量 ③ <$USERNAME><$GAMEGOLD> 两路不同 |
| 2 | `openmir2-script-take-persist` | openmir2-worker-5@wt-takepersist/script-take-persist | r10 真机窗口项 1 GREEN（筹码包 take 后存档槽位清零） |
| 3 | `openmir2-openybdeal-host-wiring` | openmir2-orchestrator@main/master | 余量=真机掷骰/寄售面板 + 残余⑨-⑬ |
| 4 | `openmir2-npc-def-loading` | openmir2-orchestrator@main/master | 余量=真机窗口四项 |
| 5 | `openmir2-weapon-upgrade-values` | openmir2-orchestrator@main/master | 余量=窗口五项 |
| 6 | `openmir2-npc-hit-frames` | openmir2-orchestrator@main/master | 余量=窗口五项 |
| 7 | `openmir2-monster-ref-drains` | openmir2-orchestrator@main/master | 余量=真机四类帧与数据可达性 |
| 8 | `openmir2-guard-ctor-constants` | openmir2-orchestrator@main/master | 余量=W-guard-1..4 |
| 9 | `openmir2-a22-castle-persistence` | openmir2-orchestrator@main/master | 余量=W-A22-1..4 |
| 10 | `openmir2-npc-ctor-constants` | openmir2-orchestrator@main/master | 余量=W1-W4 窗口 |
| 11 | `openmir2-t138-request-castle-war` | openmir2-orchestrator@main/master | 余量=W-T138-1..4 |
| 12 | `openmir2-npc-display-table` | openmir2-orchestrator@main/master | 余量=真机帧面与点击场景 |
| 13 | `openmir2-single-binary-roles` | openmir2-orchestrator@main/master | 余量=RNG trace site groups 复验 + 旧 exe 与 mir2-server 日志逐行对比（窗口项） |
| 14 | `openmir2-login-charstatus-657` | openmir2-orchestrator@main/master | 按 R 级不 closed：余量=Rust 侧重采 657 + 邻居相对序 |
| 15 | `openmir2-a09-upgrade-item` | openmir2-orchestrator@main/master | 余量=真机六项窗口 |
| 16 | `openmir2-merchant-race-bias` | openmir2-orchestrator@main/master | 余量=真机攻击商人/目标集/GetFeature/Push 时序四项窗口 |
| 17 | `openmir2-superman-normnpc` | openmir2-orchestrator@main/master | 余量=真机窗口（城门/守卫/城墙/NPC 商人 |
| 18 | `openmir2-mb8-9-monsters` | openmir2-orchestrator@main/master | 余量=真机 race215 帧（需先接 T-148）+ NormNpc SuperMan（另卡 openmir2-superman-normnpc） |
| 19 | `openmir2-monster-walk-frames` | openmir2-orchestrator@main/master | 余量=真机重采 + T-146 四条残余 |
| 20 | `openmir2-login-654-order` | openmir2-orchestrator@main/master | 按 R 级不 closed：余量=真机三项（改动后 Rust 登录抓包/有会角色 811,46 与 750 位置/非 TestServer SysMsg 条数） |
| 21 | `openmir2-db-encoding` | openmir2-orchestrator@main/master | 按 R 级不 closed：余量=E-7 真机 oracle 对照 + 4 条 Low 台账 |
| 22 | `openmir2-market-def-impl` | openmir2-orchestrator@main/master | 按 R 级不 closed：余量=真机说词/变量 + T-145 未接分支 |
| 23 | `openmir2-account-protection-13` | openmir2-orchestrator@main/master | 按 R 级不 closed：余量=真机 roundtrip 13/12 列逐列 |
| 24 | `openmir2-strlist-bom` | openmir2-orchestrator@main/master | 余量=真机 BOM 文件读取 |
| 25 | `openmir2-mb7-monsters` | openmir2-orchestrator@main/master | 按 R 级不 closed：余量=真机观察 + T-143 |
| 26 | `openmir2-encoding-crate` | openmir2-orchestrator@main/master | 余量=真机逐字节 + data StringList 读侧 BOM 缺口（另开卡） |
| 27 | `openmir2-mb2-monsters` | openmir2-orchestrator@main/master | 余量=真机观察 + T-141 |
| 28 | `openmir2-hutil32-parity` | openmir2-orchestrator@main/master | 按 R 级不 closed：余量=round 调用点真机对拍 + 10 个 GB2312 族成员（零依赖契约）+ 两份 Round 分叉（建议 T-136） |
| 29 | `openmir2-guild-official-npc` | openmir2-orchestrator@main/master | 按 R 级不 closed：余量=真机窗口三项 + T-137（固定 NPC 脚本装载，最大缺口） |
| 30 | `openmir2-mb5-monsters` | openmir2-orchestrator@main/master | 按 R 级不 closed：余量=真机观察 + T-135（host drain 10139） |
| 31 | `openmir2-castle-war-area-impl` | openmir2-orchestrator@main/master | 按 R 级不 closed：余量=真机 W-1..W-5（四臂落点 rng/766 bit4/名字色/10s Run）+ T-125..T-132 |
| 32 | `openmir2-dbsrv-scheme-a` | openmir2-orchestrator@main/master | 按 R 级不 closed：余量=真机 W-DB-1..7（慢存档/全序/闸门/独立失败）+ T-120 |
| 33 | `openmir2-encoding-core-e1-e2` | openmir2-orchestrator@main/master | 余量=真机窗口（非 ASCII 文本逐字节 + 两条 BOM 路径） |
| 34 | `openmir2-chat-channels` | openmir2-orchestrator@main/master | 按 R 级不 closed：余量=真机窗口三项 + T-118（NRE 已 1:1，意图投递待 owner 批准后另开 bug-fix 批次） |
| 35 | `openmir2-guild-g3-persistence` | openmir2-orchestrator@main/master | 按 R 级不 closed：余量=真机窗口五项（104 扇出/SS_208/四段字节/相位/实际收件人集）+ T-97..T-117 登记 |
| 36 | `openmir2-castle-guard-target` | openmir2-orchestrator@main/master | 余量=真机窗口：同城/盟友/UnderWar 目标选择 + T-96⑥④（AnimalObject.Struck 派发边界） |
| 37 | `openmir2-castle-entities` | openmir2-orchestrator@main/master | 按 R 级不 closed：余量=真机窗口五项（部署 0 实体/4 告警、夹具 16 实体 96 抽、毒攻门、同城/盟友/UnderWar 目标选择、1-tick 相位）+ T-96 六条登记 |
| 38 | `openmir2-upgrade-npc` | openmir2-orchestrator@main/master | 按 R 级不 closed：余量=真机窗口五项（抽数序/Desc[10] 字节/帧序/654·153/破零链），另 T-95 的九条登记随卡 |
| 39 | `openmir2-guild-g2-dispatch` | openmir2-orchestrator@main/master | 余量=750 真机出队序等三项 |
| 40 | `openmir2-run-gap-pl4` | openmir2-orchestrator@main/master | 余量=真机窗口三项（见 review.window_items） |
| 41 | `openmir2-selgate-reconnect` | openmir2-orchestrator@main/master | 余量=真机窗口两项 |
| 42 | `openmir2-dbsrv-fault-paths` | openmir2-orchestrator@main/master | 余量=真机窗口 + LB-08/09/10 |
| 43 | `openmir2-gate-idle-timeout` | openmir2-orchestrator@main/master | 余量=真机窗口两项 |
| 44 | `openmir2-run-gap-pl2` | openmir2-orchestrator@main/master | 余量=真机窗口两项 |
| 45 | `openmir2-gategate-slots-gm` | openmir2-orchestrator@main/master | 余量=真机窗口三项 |
| 46 | `openmir2-login-numeric-gaps` | openmir2-orchestrator@main/master | 余量=真机窗口三项（见 review.window_items） |
| 47 | `openmir2-oracle-humitems-align` | openmir2-orchestrator@main/master | 余量=真机窗口：部署基线已变，下次开窗重抓装备槽字段级证据 |
| 48 | `openmir2-run-gap-pl3` | openmir2-orchestrator@main/master | 余量=真机窗口（内容启用后的数值/帧序）+ T-85 登记的八条偏差 |
| 49 | `openmir2-delmagic-addmagic-chain` | openmir2-orchestrator@main/master | 余量=真机窗口三项（见 review.window_items） |
| 50 | `openmir2-login-frame-order-52-752` | openmir2-orchestrator@main/master | 余量=非 TestServer 部署/发光装备/IsSpirit 的槽位变化未取样本 |
| 51 | `openmir2-gategate-speed-rules` | openmir2-orchestrator@main/master | 余量=真机窗口四项（登录包相位、0-210ms 内未丢帧的先后、超长帧回归、ItemSpeed/SpeedLimit 接线） |
| 52 | `openmir2-healthspell-emission-model` | openmir2-orchestrator@main/master | 余量=真机窗口四项（见 review.window_items） |
| 53 | `openmir2-monster-attack-hit-frame` | openmir2-orchestrator@main/master | 余量=真机窗口（同 tick/200ms 顺序三项） |
| 54 | `openmir2-run-gap-pl1` | openmir2-orchestrator@main/master | 余量=真机窗口四项（见 review.window_items） |
| 55 | `openmir2-monster-attack-cadence-lead` | openmir2-orchestrator@main/master | 余量=按评审口径的 HP 受控重采样（真机窗口），并入窗口项 |
| 56 | `openmir2-run-gap-mb4` | openmir2-orchestrator@main/master | 余量=真机窗口：race 92 的 SpaceMove/MapRandomMove 落点与狂暴 500/400 节拍 |
| 57 | `openmir2-magic-8-21-wiring` | openmir2-orchestrator@main/master | 余量：① 真机窗口（37 的 Random(20)/Random(2) 流水 + 逐格 SM_BACKSTEP，可与 8 同场景） |
| 58 | `openmir2-player-dir-supply` | openmir2-orchestrator@main/master | 余量：真机窗口 + T-81 登记 |
| 59 | `openmir2-player-drop-useitems` | openmir2-orchestrator@main/master | 余量：真机窗口 + T-81 登记 |
| 60 | `openmir2-rm-delaypushed` | openmir2-orchestrator@main/master | 余量：真机窗口 + T-81 登记 |
| 61 | `openmir2-rm-delaymagic-six-ids` | openmir2-orchestrator@main/master | 余量：真机窗口 + T-81 登记 |
| 62 | `openmir2-save-misc-gaps` | openmir2-orchestrator@main/master | 余量：真机窗口 |
| 63 | `openmir2-db-error-reply` | openmir2-orchestrator@main/master | 余量：真机窗口（失败注入的 C# 对拍） |
| 64 | `openmir2-persist-single-sided-columns` | openmir2-orchestrator@main/master | 余量：真机窗口 |
| 65 | `openmir2-attacktarget-overrides` | openmir2-orchestrator@main/master | 余量：真机窗口（82/119/130/91/107/105） |
| 66 | `openmir2-smframe-minor-gaps` | openmir2-orchestrator@main/master | 余量：真机窗口 + T-79/T-80 登记项 |
| 67 | `openmir2-sm-merchantsay-recog` | openmir2-orchestrator@main/master | 余量：真机窗口 + T-79/T-80 登记项 |
| 68 | `openmir2-login-useitems-magic` | openmir2-orchestrator@main/master | 余量：真机窗口 + T-79/T-80 登记项 |
| 69 | `openmir2-sm-updateitem` | openmir2-orchestrator@main/master | 余量：真机窗口 + T-79/T-80 登记项 |
| 70 | `openmir2-sm-additem-tail` | openmir2-orchestrator@main/master | 余量：真机窗口 + T-79/T-80 登记项 |
| 71 | `openmir2-search-target-clear-target` | openmir2-orchestrator@main/master | 余量：① 真机窗口 |
| 72 | `openmir2-proper-target-unfolded` | openmir2-orchestrator@main/master | 余量：① 真机窗口（攻击已死目标的抽数流水） |
| 73 | `openmir2-active-seek-race-missing` | openmir2-orchestrator@main/master | 余量：① 真机窗口 |
| 74 | `openmir2-rm-refmessage-delay-layer` | openmir2-orchestrator@main/master | 余量：① **真机窗口**（200ms 后二次 `Struck` 的全局帧序） |
| 75 | `openmir2-save-on-shutdown` | openmir2-orchestrator@main/master | 余量：① **真机窗口**（`DB_SAVEHUMANRCD` 帧与落库 diff、`-Stop` graceful 日志 |
| 76 | `openmir2-run-gap-mb3` | openmir2-orchestrator@main/master | 余量：① **真机窗口**（107/105/102/112） |
| 77 | `openmir2-persist-item-attr-and-gold` | openmir2-orchestrator@main/master | 余量：① G-01 功能上仍不落库（oracle 门决定，`B-117`） |
| 78 | `openmir2-persist-useitems-wiring` | openmir2-orchestrator@main/master | 余量：① **真机窗口**（耐久磨损/破零/存档往返） |
| 79 | `openmir2-persist-create-rows` | openmir2-orchestrator@main/master | 余量：① 真机窗口（建角/兜底建角往返） |
| 80 | `openmir2-monster-view-throttle` | openmir2-orchestrator@main/master | 余量：① **真机窗口** |
| 81 | `openmir2-run-gap-mb1` | openmir2-orchestrator@main/master | 余量：① **真机窗口**（race 101/95 各 ≥1、R38 抽样） |
| 82 | `openmir2-run-gap-checkdupobj` | openmir2-orchestrator@main/master | 余量：① **真机窗口**——3 人同格推挤的源/目标格身份序列 + AOI 相位（1000ms 口径） |
| 83 | `openmir2-run-gap-monster-state-expiry` | openmir2-orchestrator@main/master | 余量：① **真机窗口**（CrazyMode 色变 0xF9 / RefNameColor 帧面 / FireHit-TwinHit 到期消息） |
| 84 | `openmir2-run-gap-corpse-ghost-revive` | openmir2-orchestrator@main/master | 余量：① **真机窗口**（3 分钟变尸 / 可复活支 ZenTime-20s / 5 分钟移除 / 两帧 RM_TURN） |
| 85 | `openmir2-run-gap-pr1-head-blocking` | openmir2-orchestrator@main/master | 余量：① 真机窗口 |
| 86 | `openmir2-disappear-recipients-gridscan` | openmir2-orchestrator@main/master | 余量：① **真机窗口**（节流窗口内进入 ±12 的观察者、离世广播与 AOI/移动帧的全局次序） |
| 87 | `openmir2-run-gap-monster-specials` | openmir2-orchestrator@main/master | 余量：① **真机窗口**（同 tick 产仔抽数相对次序、`SM_HIT` 帧面、Clone 扣 MP） |
| 88 | `openmir2-run-gap-monster-master-follow` | openmir2-orchestrator@main/master | 余量：① **真机窗口**（异图/超距/SlaveRelax 两侧流水） |
| 89 | `openmir2-movetomovingobject-quirk` | openmir2-orchestrator@main/master | 余量：① **真机窗口**（多对象格移动时的源/目标格身份序列） |
| 90 | `openmir2-player-searchviewrange-cadence` | openmir2-orchestrator@main/master | 余量：① **真机窗口**——首扫约 1.2s、之后每 1.2s |
| 91 | `openmir2-aoi-verify-maptime` | openmir2-orchestrator@main/master | 余量：① **真机窗口**——同缩窗场景需验 vis>=1、nearest<=vr、capture 出 SM_TURN(281,661) + 配对 SM_FEATURECHANGED、SM_WALK=0（W0-4 复跑） |
| 92 | `openmir2-spell-shared-tail` | openmir2-orchestrator@main/master | 余量：① 真机窗口（注入时钟下 656 帧序） |
| 93 | `openmir2-login-notice-clienttick` | openmir2-orchestrator@main/master | 余量：① 真机窗口（含 `SoftVersionDateEx==0` 两侧构造） |
| 94 | `openmir2-spell-mp-wrapper` | openmir2-orchestrator@main/master | 余量：① 真机窗口（失败回帧 + MP/53/尾时序） |
| 95 | `openmir2-enter-map-cell-rejected-vs-oracle` | openmir2-orchestrator@main/master | 余量：① 真机窗口（切图失败路径 |
| 96 | `openmir2-enter-shift-loop` | openmir2-orchestrator@main/master | 余量：① 真机窗口（墙格出生登录：RNG 流水 + 落点 |
| 97 | `openmir2-map-random-move-shift-loop` | openmir2-orchestrator@main/master | 余量：① 真机窗口（商人到点移动：RNG 流水 + 落点） |
| 98 | `openmir2-recall-slave-shift-live` | openmir2-orchestrator@main/master | 余量：① 真机窗口（W0 §2e：两侧 RNG 账本 `VERDICT IDENTICAL` + 回召落点同坐标） |
| 99 | `openmir2-mapcell-attributes-loading` | openmir2-orchestrator@main/master | 余量：① 真机窗口（**必核**：route added=1649、刷怪位置与 RNG 流水、进图/落格/AOI/CanWalk(true)/snap） |
| 100 | `openmir2-mapcell-consumer-teeth` | openmir2-orchestrator@main/master | 余量：① 真机窗口核「走位/推/冲刺帧面 + NPC/商人出现帧」（W0 runbook §W0-6 已列 |
| 101 | `openmir2-mongen-terrain-loop-live` | openmir2-orchestrator@main/master | 余量：真机窗口核「两侧出生坐标逐点一致 + RNG 流水」（W0 runbook §W0-6/§W0-8 已列，依赖 mapcell 装载 |
| 102 | `openmir2-ctor-barrier-a-draw` | openmir2-orchestrator@main/master | 余量：① 真机窗口（**默认流水已变，须重取基线** |
| 103 | `openmir2-ctor-draws-fold` | openmir2-orchestrator@main/master | 余量：① 屏障 A 的 1 抽（ServerConf.cs:114 ItemNumber += Random(10000)）——补上后段②/段③ 与 oracle 等距，须另立片 |
| 104 | `openmir2-snap-cell-fidelity` | openmir2-orchestrator@main/master | 余量：① 真机窗口项 |
| 105 | `openmir2-default-arm-spell-surface` | openmir2-orchestrator@main/master | 余量：① 真机逐帧对拍（窗口项） |
| 106 | `openmir2-buff-producer-wiring` | openmir2-orchestrator@main/master | 余量=真机窗口项 + 新开片：四块 snap 改用忠实镜像 `cetin_near_xy`（复验新发现的格子层差异）+ 补窗口宽度上界牙 |
| 107 | `openmir2-buff-system` | openmir2-coordinator@main/master | 余量=真机窗口项 + A-5 生产接线批 |
| 108 | `openmir2-world-tick-expansion` | openmir2-coordinator@main/master | 余量=真机窗口项（RNG 流水/时钟/公告节拍） |
| 109 | `openmir2-transmap-maptrigger` | openmir2-coordinator@main/master | 余量=真机窗口项 |
| 110 | `openmir2-maptrigger-monster-and-door` | openmir2-coordinator@main/master | 余量=真机窗口项 W-R1-a/b/c |
| 111 | `openmir2-orphan-encoder-wiring` | openmir2-coordinator@main/master | 余量=真机窗口项 |
| 112 | `openmir2-window-collector-preflight` | openmir2-coordinator@main/master | 余量=真机窗口项：两侧种子交叉断言 / C# site 列 / 前置门生产路径不误拦 |
| 113 | `openmir2-outbound-encoders-red-batch` | openmir2-coordinator@main/master | 按 tier=R **不 closed**——待窗口项：制药抽数流水 + 帧序 + MYSTATUS 派生落库 |
| 114 | `openmir2-m3-monster-active-seek` | openmir2-worker-22@land/worktree-redlist-monai9 | 余量：真机 W0/W9 对拍 + 登记未建模面（覆写 Run 的子类、SuperGuard/ArcherGuard 自有 pvp 扫描） |
| 115 | `openmir2-m3-drop-exp-wiring` | openmir2-worker-22@land/worktree-redlist-x2 | 余量：真机 100 杀两相窗口（依赖 T-59 rng-scene 已落）+ quest 真机窗口 |
| 116 | `openmir2-m3-rng-scene-driver` | openmir2-worker-22@land/worktree-redlist-x1 | 余量：真机两侧账本对拍（W9 窗）+ BotSrv real wiring（进 0137/pin 怪/dump，属 W9 窗项） |
| 117 | `openmir2-npcs-fixed-loading` | openmir2-worker-22@land/worktree-redlist-t1-npcs | 剩余 DoD = 真机两侧 NPC 出现帧一致（并入联调窗，与 ③a/GuardList 同窗） |
| 118 | `openmir2-m3-quest-align` | openmir2-svc-1@window/worktree-window | 窗口 r3（master d4d5b89c 未动）：窗口 r3 复核⑥（master 未动）：仍 BLOCKED，三重阻断与 r2 相同：①markers 前缀口径（卡 openmir2-m3-fixture-marker-prefix-bug 未认领）②C# k=0 frame 事件 JSON 截… |
| 119 | `openmir2-guardlist-guards-loading` | openmir2-worker-22@guardlist/worktree-guardlist-loading | 剩余 DoD = 真机两侧项：③a 反向点出现帧一致（C# 侧现 1 对、修复后应两侧同）+ MIR2_RNG_SEED 双侧 site/order 对拍——并入下次联调窗口（与 ③b/③a 复跑同窗） |
| 120 | `openmir2-m3-drop-exp-fixture-botsrv` | openmir2-worker-11@m2-svr/worktree-m3-drop-exp-fixture-botsrv | **保持 in-progress（离线夹具片，不 closed）**：本 approve 只覆盖离线夹具/接线/登记片，卡级真机 DoD③⑤ = 真机两侧 100 杀同序列 diff=0 + drop_exp_purity_gate 实测（阻塞 = owner -Go 独占窗口 + shared 承… |
| 121 | `openmir2-m3-hero-pet-align` | openmir2-svc-1@window/worktree-window | 窗口 r3（master d4d5b89c 未动）：窗口 r3 复核⑨（master 未动）：PREREQ（静态） |
| 122 | `openmir2-m3-fixture-chain-gate` | openmir2-worker-5@m3cs/worktree-m3-chain-gate-o1o2-closeout | review do_not_close=true）**：剩余 DoD = 卡级真机两侧同序列 diff=0（各 READY 夹具 run_*.ps1 -SelfCheckNoStack + 共享管线自检需世界侧实现 + openmir2-m3-clock-injection + owner 独占窗口… |
| 123 | `openmir2-m3-rng-trace-scene-groups` | openmir2-worker-4@m3bag/wip/m3bag-anchor-teeth | **保持 in-progress（不 closed，review do_not_close 明示）**：本 approve 只覆盖离线证据/门片，卡级 R 真机 DoD（同场景含怪两相 trace + groupdiff --fold --strict 的 site/order diff=0）仍 n… |
| 124 | `openmir2-m3-pet-slave-masterdie-wire` | openmir2-worker-20@m3hp/worktree-m3-selloff-dealoffform-recog | **按 tier=R 不 closed（保持 in-progress）**：剩余 DoD = ① 真机两侧 run_hero_pet_pair.ps1 主人死 step 同序列 diff=0（含叛变站点帧序/字节 + RNG 抽数 site/order）② 混合队列场景未建模 not_verifie… |
| 125 | `openmir2-m3-pet-slave-amulet-delitem-frame` | openmir2-worker-20@m3hp/worktree-m3-pet-slave-amulet-delitem | **按卡级剩余 DoD 保持 in-progress（不 closed）**：剩余 = ① 真机 run_hero_pet_pair.ps1 护身符扣光 step 两侧同序列 diff=0（owner 独占窗口，review not_verified）② unimplemented 登记：SendD… |
| 126 | `openmir2-m3-party-trade-align` | openmir2-svc-1@window/worktree-window | 窗口 r3（master d4d5b89c 未动）：窗口 r3 复核⑦（master 未动）：PREREQ（静态） |
| 127 | `openmir2-m3-pet-slave-clone46` | openmir2-worker-20@m3hp/worktree-m3-pet-slave-clone46 | **按 tier=R 不 closed（保持 in-progress）**：剩余 DoD = 真机两侧 run_hero_pet_pair.ps1 make_clone step 同序列 diff=0（待真机判据（窗口）），按 review not_verified 保留 |
| 128 | `openmir2-m3-shop-fixture-botsrv` | openmir2-worker-5@m3cs/worktree-m3-shop-frame-json-parse | **保持 in-progress（不 closed）**：本片虽为 L 级（window_required=false），但卡级真机 DoD②③（真机两侧同序列 run_npc_shop_pair.ps1 + npc_shop_diff.py 字段级 diff=0）仍待世界侧（⑤-2/⑤-4/⑤-5… |
| 129 | `openmir2-m3-selloff-fixture-botsrv` | openmir2-worker-5@m3cs/worktree-m3-selloff-fixture-botsrv | 剩余 DoD②③ = 真机两侧同序列 diff=0（等 ⑩-1/⑩-2 落地 + 独占窗口） |
| 130 | `openmir2-m3-fixture-marker-prefix-bug` | openmir2-worker-5@m3cs/worktree-m3-marker-guard-registration | **保持 in-progress（夹具/离线片，不 closed）**：剩余 DoD = 卡级 ① 真机两侧同序列 diff=0（阻塞 = 承接卡/driver + owner 独占窗口，按 review not_verified 登记） |
| 131 | `openmir2-m3-drop-exp-align` | openmir2-svc-1@window/worktree-window | 窗口 r3（master d4d5b89c 未动）：窗口 r3 复核④（master 未动）：PREREQ（静态） |
| 132 | `openmir2-m3-pet-slave-lifecycle` | openmir2-worker-20@m3hp/worktree-m3-pet-slave-abilitys | **按 tier=R 不 closed（保持 in-progress）**：剩余 DoD = 真机两侧 run_hero_pet_pair.ps1 同序列 diff=0（召唤/诱惑/叛变/幽灵各站点，归 openmir2-m3-hero-pet-align + owner 独占窗口），按 revie… |
| 133 | `openmir2-m3-hero-pet-fixture-botsrv` | openmir2-worker-5@m3cs/worktree-m3-hero-pet-fixture-minors | **不 closed（保持 in-progress）**：剩余 DoD②③ = 真机两侧 run_hero_pet_pair.ps1 同序列 diff=0（需 ⑨-2/⑨-3 世界侧 + openmir2-m3-clock-injection + owner 独占窗口），按 review not_v… |
| 134 | `openmir2-m3-guild-castle-fixture-botsrv` | openmir2-worker-5@m3cs/worktree-m3-guild-castle-anchor | **夹具卡 ⇒ 保持 in-progress（不 closed）**：剩余 DoD②③ = 真机两侧 run_guild_castle_pair.ps1 同序列 diff=0（需 ⑧-3..⑧-9 实现卡 + openmir2-m3-clock-injection + owner 独占窗口），按 r… |
| 135 | `openmir2-m3-selloff-goldsales-load-wire` | openmir2-worker-20@m3hp/worktree-m3-selloff-goldsales-load-wire | **不 closed（DoD 未全达，保守）**：剩余 = 真机两侧同序列对拍 run_selloff_pair.ps1 + selloff_diff.py + goldsales 快照 diff=0（等独占窗口，归 openmir2-m3-mail-auction-align） |
| 136 | `openmir2-m3-shop-dealing-gate` | openmir2-worker-11@m2-svr/worktree-m3-shop-dealing-gate | **不 closed（review OA-1 明示：本 approve 只覆盖 Dealing 前置门一片，卡级 DoD 真机两相未闭环）**：剩余 DoD = 真机端到端（交易中发 CM_USERBUYITEM/CM_USERGETDETAILITEM 观察零帧 + characters/gold… |
| 137 | `openmir2-m3-shop-dialog-wiring` | openmir2-worker-11@m2-svr/shop-wire-host | **按 tier=R 不 closed（保持 in-progress）**：剩余 DoD = 真机两侧 run_npc_shop_pair.ps1 同序列 diff=0（详情 652 / 仓库 700-707 帧序与字节 + characters_storageitem 槽位 diff=0）与 DB… |
| 138 | `openmir2-m3-quest-fixture-botsrv` | openmir2-worker-5@m3cs/worktree-m3-quest-fixture-botsrv | 剩余 DoD②③ = 真机两侧同序列 diff=0（-Side rust preflight 仍 PREREQ，等 ⑥-3 落地 + 独占窗口） |
| 139 | `openmir2-m3-quest-persist-observability` | openmir2-worker-21@m3pt/worktree-m3-quest-persist-observability | run_pair.ps1 + scoped_diff.py --canonical）需独占窗口，按 not_verified 登记 |
| 140 | `openmir2-m3-mail-auction-align` | openmir2-svc-1@window/worktree-window | 窗口 r3（master d4d5b89c 未动）：窗口 r3 复核⑩（master 未动）：PREREQ（静态） |
| 141 | `openmir2-m3-party-deal-capacity-int` | openmir2-worker-21@m3pt/worktree-m3-party-deal-capacity-int | **按 tier=L 但卡级真机 DoD 未达 ⇒ 不 closed（保持 in-progress，review note 明示「不因本片收卡」）**：剩余 DoD = 真机两侧同序列 diff=0（归 openmir2-m3-party-trade-align + 独占窗口，not_verifie… |
| 142 | `openmir2-m3-clock-injection` | openmir2-worker-11@m2-svr/m3-clock-inject-host | **按 tier=R 不 closed（保持 in-progress）**：剩余 DoD = ① C# 影子 hook 半（HUtil32.GetTickCount/DateTime.Now 不可替换字段，待 owner 在 Mono.Cecil IL patch / 影子源码重建 / 不改 ora… |
| 143 | `openmir2-m3-pet-slave-summon` | openmir2-worker-20@m3hp/worktree-m3-pet-slave-m3-rc-eol | 合并检查点（role=merge, openmir2-coordinator）：⑨-2 M-3 收口片（本片 = 仓库卫生离线片：mir2-rs/.gitattributes 追加 `*.rc text eol=lf`，零 .rs，非窗口）已合入 master bdc508ef1（approve o… |
| 144 | `openmir2-m3-party-gainexp-npe` | openmir2-worker-21@m3pt/worktree-m3-party-gainexp-npe | **按 tier=R 不 closed**：剩余 DoD = 真机两侧同序列 diff=0（归 openmir2-m3-party-trade-align + 独占窗口），按 not_verified 登记 |
| 145 | `openmir2-m3-shop-scenario-authority` | openmir2-worker-5@m3cs/worktree-m3-shop-scenario-authority | **剩余 DoD = 真机**：run_npc_shop_pair.ps1 在独占窗口跑两侧同序列 diff=0（等 ⑤-4 第②片仓库存档面 + ⑤-5 / NPC 入视野），离线判据本体已完整 |
| 146 | `openmir2-m3-fixture-matrix` | openmir2-worker-5@m3cs/worktree-m3-fixture-matrix | **剩余 = 真机 DoD**：READY 夹具需 -Side rust preflight 前置落地 + 独占窗口跑两侧同序列 diff=0 |
| 147 | `openmir2-m3-guild-castle-align` | openmir2-svc-1@window/worktree-window | 窗口 r3（master d4d5b89c 未动）：窗口 r3 复核⑧（master 未动）：PREREQ（静态） |
| 148 | `openmir2-m3-npc-shop-align` | openmir2-svc-1@window/worktree-window | 窗口 r3（master d4d5b89c 未动）：窗口 r3 复核⑤（master d4d5b89c 未动，与 r1/r2 相同）：仍 BLOCKED，与 r2 逐条相同 |
| 149 | `openmir2-m3-scoped-diff-ability` | openmir2-svc-1@window/worktree-window | 窗口 r3（master d4d5b89c 未动）真机加深复跑 ②③F-2（360 击）：attack_diff 仍 GREEN diff=0（360/157/203/6576） |
| 150 | `openmir2-m3-party-trade-fixture-botsrv` | openmir2-worker-5@m3cs/worktree-m3-party-trade-fixture-botsrv | 剩余 DoD②③ = 真机两侧同序列 diff=0（等 ⑦-2/⑦-3 落地 + 独占窗口） |
| 151 | `openmir2-m3-final-acceptance` | openmir2-worker-5@m3cs/worktree-m3-final-acceptance | **剩余**：DoD② 300 假人 24h 长跑（**等独占窗口** |
| 152 | `openmir2-click-path-bag-writeback` | openmir2-worker-5@wt-clickbag/click-bag-writeback | 余量=真机落库对拍（T-160：`characters_bagitem`/`hum_items`/`characters.Gold`）；master 合并 `b0dde46a9`，按 R 不 closed |
### B. 代码未开工/接线未做

| card id | owner/worktree | 阻塞原因（来自最新 status 的 work） |
|---|---|---|
| `openmir2-castle-pure` | openmir2-orchestrator@main/master | 余量=接线（shop/main 消费税意图）+ W1-W3 |
| `openmir2-script-playdice-openybdeal` | openmir2-orchestrator@main/master | 余量=bins 接线（NpcPlayer set/get + game-svr 调用）+ 真机窗口 + T-142 |
| `openmir2-market-def-design` | openmir2-orchestrator@main/master | 余量=实现（R）：商人脚本装载 + [~@repair] + PlayDiceLabel |
| `openmir2-castle-tax-item-design` | openmir2-orchestrator@main/master | 余量=实现（R）+ GetCastle/MasterGuild 前置 |
| `openmir2-castle-official-design` | openmir2-orchestrator@main/master | 余量=实现车道（R）+ 共用固定 NPC 对话层前置 |
| `openmir2-loader-fields` | openmir2-orchestrator@main/master | 按 R 级不 closed：余量=接线（game-svr 改消费新加载器）+ 真机窗口 |
| `openmir2-castle-war-area-design` | openmir2-orchestrator@main/master | 余量=实现车道（R：进图落点 RNG / 766 bit4 / 名字色 / 状态机）+ 真机窗口 |
| `openmir2-dbsrv-connection-design` | openmir2-orchestrator@main/master | 余量=方案 A 实现（另开卡，tier=R）+ LB-10-E1 真机证据（窗口） |
| `openmir2-run-gap-mb2` | openmir2-orchestrator@impl-mb2/detached@0ebdbbf54 | 车道在飞（wf_6845f065-267） |
| `openmir2-monster-run-overrides` | openmir2-orchestrator@main/master | 余量：12 条未决（延迟队列/RunTime 节拍/MAC·AntiMagic/目标选择/SendRefMsg 缓存…） |
| `openmir2-spell-frame-bypass-paths` | openmir2-orchestrator@main/master | 余量=**实现**（① 机器人路径触发集合 |
| `openmir2-inbound-dispatch-red-batch` | openmir2-worker-22@land/worktree-redlist-c10 | 余量：CM_CLICKNPC 的 recog→映射牙（待 @main label 派发接线）+ 各 handler 出帧的发射点接线片 + RNG 面（DROPGOLD 深体接线即升 R） |
| `openmir2-envir-redlist-batch` | openmir2-worker-22@land/worktree-redlist-t2noclear | 余量：T3 的 4 颗待补牙 + 各消费点接线片（含 NoClearMonList 照搬 oracle no-op 的裁定）+ cards.json 指派（wave4 W3） |
| `openmir2-m3-selloff-consignment-world` | openmir2-worker-20@m3hp/worktree-m3-selloff-dealoffform-recog | **按卡级 tier=R 不 closed（保持 in-progress）**：卡级真机 DoD（两侧 run_selloff_pair.ps1 + selloff_diff.py 字段级 diff=0）仍需世界侧 SaleDeal setter（脚本 OPENYBDEAL -> ActionOfO… |
| `openmir2-m3-guild-war` | openmir2-worker-5@m3cs/worktree-m3-guild-war-quirk-reg | **不 closed（保持 in-progress）**：仍有非窗口剩余 DoD = 第②片 bins/game-svr 的 CM_* 派发/帧/RM_USERNAME payload/落盘 IO 与 GuildManager.Run 超时 tick 接线（等 owner bins 串行序裁决），另… |
| `openmir2-m3-castle-file-load` | openmir2-worker-5@m3cs/worktree-m3-castle-anchor-semantics | **保持 in-progress（离线夹具片，不 closed）**：剩余 DoD = ① bins/game-svr 启动接线（等 owner bins 串行裁决）② 真机两侧 run_guild_castle_pair.ps1 同序列 diff=0（待真机判据（窗口）），按 review not… |
| `openmir2-m3-guild-file-store` | openmir2-worker-5@m3cs/worktree-m3-guild-store-swallow | **不 closed（卡级 DoD 未达 + tier=L 但仍有非窗口剩余项）**：剩余 = ① bins/game-svr 启动 LoadGuildInfo / 退出 SaveGuildList 接线（等 owner bins 串行序裁决）② 真机两侧 run_guild_castle_pair… |
| `openmir2-m3-castle-door-repair` | openmir2-worker-5@m3cs/worktree-m3-castle-door-repair-o1 | **不 closed（卡级 DoD 未达，保持 in-progress）**：剩余 = 第②片 bins/game-svr 门/墙 tick 接线（等 owner 对 bins 串行序裁决）+ 真机两侧 run_guild_castle_pair.ps1 diff=0（归 ⑧-10 夹具 + 独占窗… |
| `openmir2-m3-guild-state` | openmir2-worker-5@m3cs/worktree-m3-guild-state | **不 closed（本卡 DoD 未全达，非窗口项仍在）**：剩余 = 第②片 bins/game-svr 派发/帧（9 个 CM_GUILD* + 成员/封号/公告/联盟/解散帧）与 LoadGuildInfo/SaveGuildList 启动/退出接线，等 owner 对 bins 串行序裁决… |
| `openmir2-m3-castle-gold-tax` | openmir2-worker-5@m3cs/worktree-m3-castle-gold-tax | **按 tier=R 不 closed**：剩余 DoD = ① 真机两侧「同一 10min SaveTick 时间门 → TodayIncome/TotalGold 落盘」diff=0（依赖 ⑧-1 C# GetTickCount 注入片 + owner 独占窗口）② 第②片 bins/game-… |
| `openmir2-m3-castle-siege-state` | openmir2-worker-5@m3cs/worktree-m3-castle-siege-state | tier=L 但**不 closed**：剩余 DoD = 第②片 bins/game-svr 接线（tick 驱动 UserCastle::run + CastleEvent 落帧/落盘 SaveAttackSabukWall，等 owner 对 bins 串行序裁决，作者 comment f26… |
| `openmir2-m3-party-attribution` | openmir2-worker-21@m3pt/worktree-m3-party-attribution | tier=R ⇒ 不 closed：剩余 DoD = 真机两侧同序列 diff=0（run_party_trade_pair.ps1 + 独占窗口，归 openmir2-m3-party-trade-align），按 not_verified 登记 |

#### 3.2.1 B-补：无卡可派的代码面（来自 m3 A 表；**不计入 22 张卡**）

| 面 | 索引 | 现状（HEAD `24e9a5fab`，逐文件复核） | 处置 |
|---|---|---|---|
| `SETOFFLINE` 的 `AutoGetExp*` 四字段 | m3 A-10 / T-86 | `crates/script/src/engine/mod.rs:603` 的 `ScriptPlayer::set_offline` 是**空默认实现**；`bins/game-svr/src/npc.rs:967` 的 `impl ScriptPlayer for NpcPlayer` **未覆写**；`crates/script/src/engine/actions.rs:617-623` 的 `ActionOfOffLine` 调它 ⇒ 四字段 + `OffLineFlag`/`KickOffLineTick` 均不写 | 无卡；tier=R（PL-2 复验 F6）。建议并入 PL-2/PL-3 后续片或新卡 |
| `SETMPAMODE DECHP` / `sys_msg` | m3 A-21 / T-85 | **已解决 @ `f5b401cc8`（merge `d1b13ba14`）**：分支表 + `sys_msg` 下发帧 + 宿主落库全链 | R 卡等真机窗口 |
| `Market_Def` 商人脚本装载/说词 | m3 A-08 / T-47③ / T-145 残余 | `shop.rs:36-38` 明写宿主**未装载** `Merchant.LoadScriptFile`；`MerchantScriptBook` 已存在并用于 `Npc_def` 点击路径，但 `Market_Def` 的 `~@repair`/say 链路仍按「脚本未装载 ⇒ 无后续帧」登记 | 无实现卡；建议新卡 `openmir2-merchant-script-load`（本文 §6 top5 #4） |

### D. needs-human（owner 裁决/排期）

| card id | owner/worktree | 阻塞原因（来自最新 status 的 work） |
|---|---|---|
| `openmir2-gategate-keepalive` | openmir2-orchestrator@main/master | 余量=真机窗口三项（10s 周期/3min 窄竞态可达性需 owner 裁决/断链重连与 15 天清理） |
| `openmir2-run-gap-player-run-quantification` | openmir2-orchestrator@main/master | 余量：① **裁定证据**——按 §3 方案取数（B1 静置压血看 53 填充间隔：50/s≈6.0s vs 40/s≈7.5s） |
| `openmir2-window-checklist` | openmir2-orchestrator@main/master | 余量：① owner 排窗口（整包 W0 仍 NO-GO，唯一未解硬阻塞 = RNG 两侧驱动，设计件 `ec6e1eb4f` 推荐 P11，承接实现待 O1–O7 裁决） |
| `openmir2-rng-scene-pin-path` | openmir2-orchestrator@main/master | **承接实现未开**：需 owner 先裁决 O1–O7（T-* 白名单改 `src/GameSrv/**`、`dir` 覆盖、时钟注入、影子 Envir、是否需真实会话、与 ④ 同窗编排、C# 描述件 producer） |
| `openmir2-tmp-cleanup-audit-redo` | openmir2-orchestrator@main/master | **余量 = owner 裁决是否执行删除**（A 类 73 条/448.1 GiB 白名单 + 4 个 dangling worktree 与 verify-a5 11G 等 D 类须人核） |
| `openmir2-npc-ctor-draws` | openmir2-coordinator@main/master | 余量：① owner 裁决 8 处按 F（只补抽）还是 M（建实体） |
| `openmir2-m5-convergence-drills` | openmir2-worker-22@m5tools/worktree-m5-stage0-tools | 阶段0（零窗口 L 级工具）已合入 master 2f0b102c5ec98bffd0f5a6121cf64f4bc7cb4219：①load_report --mem-tail-sec+峰值-均值（CLI 缝两向牙）②新 error_curve.py（19/19 牙，桶界/窗口牙 |
| `openmir2-world-bevy-migration` | openmir2-worker-14@wbevy/worktree-world-bevy | 再动 `crates/world` 会与 M2 验收 / `m3bag` 的接缝争写集）⇒ **解冻条件 = M2 验收通过，或 owner 重新裁决** |

### C. 已合但卡未关（书务/已声明收口）

| card id | owner/worktree | 阻塞原因（来自最新 status 的 work） | 备注 |
|---|---|---|---|
| `openmir2-gate-gmclose-race` | openmir2-orchestrator@main/master | 无待窗口项 | — |
| `openmir2-rng-consumer-structure-parity` | openmir2-orchestrator@main/master | **三项余量已全部收口**（① 方案 C ② :342 登记 ③ 同表机判） | work 尾部仍列 R 级真机窗口：两侧构造段 `seq=1..69` + 魔法场景账本 `VERDICT IDENTICAL`；按复核意见归 C，窗口尾巴留此备查 |

### O. 在飞/待审（不计入四类）

| card id | owner/worktree | 阻塞原因（来自最新 status 的 work） |
|---|---|---|
| `openmir2-gm-commands-design` | openmir2-worker-4@wt-gm3/gm-batch-3 | 批次 3/3 传送/召唤族 17 条 + @传送：命令体逐行照 oracle，移动/RNG 未接项登记 T-166..T-171；head `bdf4e0ab0`，`wt-gm3` 尚有未提交修改（`gmcmd.rs`/`lib.rs`/`whitelist.md`） |
---

## 4. 模块面：已存在但入口未接线

### 4.1 既有 recon 门的机器判定（权威口径）

`mir2-rs/tests/parity/coverage/recon_gates.py` 已实现「生产活跃函数」可达性分析
（从 `bins/*/src` 的 `main` 经名字调用图可达，剔除 `#[cfg(test)]`），以下两个门直接可用：

```sh
cd mir2-rs
python tests/parity/coverage/recon_gates.py content  --repo .. --json E:/tmp/recon-content.json
python tests/parity/coverage/recon_gates.py dispatch --repo .. --json E:/tmp/recon-dispatch.json
# ①附注：crates/ 侧三档（§4.2）——同一个调用图，只去掉 bins 过滤
python - <<'PY'
import sys,re
sys.path.insert(0,'tests/parity/coverage'); import recon_gates as rg
c=rg.RustCorpus(rg.rust_source_dirs(rg.Path('..'),exclude=rg.DISPATCH_EXCLUDE))
live,spans=rg.production_live_functions(c); lines=rg._file_code_lines(c)
d=[s for s in spans if 'bins' not in rg.Path(s['file']).parts and
   re.search(r'\bpub\s+(?:async\s+)?fn\s+'+re.escape(s['name'])+r'\b', lines[s['file']][s['start']])]
names=sorted({s['name'] for s in d if s['name'] not in live})
cnt={n:sum(1 for _,_,ln in c.code if re.search(r'(?<![A-Za-z0-9_])'+re.escape(n)+r'(?![A-Za-z0-9_])',ln)) for n in names}
wl=open('tests/parity/whitelist.md',encoding='utf-8').read()
only={n for n in names if cnt[n]==1}
named={n for n in names if re.search(r'(?<![A-Za-z0-9_])'+re.escape(n)+r'(?![A-Za-z0-9_])',wl)}
print('defs',len(d),'unique',len(set(s['name'] for s in d)),'unreach',len(names))
print('only',len(only),'named_outside_only',len(named-only),'triage',len(set(names)-only-named))
PY
# 期望：defs 1068 unique 941 unreach 198
#       only 42 named_outside_only 25 triage 131
```

**内容加载门**（40 个 oracle `Load*` 调用点，第 4 轮实测）：

```
summary: {"n/a-oracle-noop": 9, "n/a-db": 5, "tracked": 3, "implemented": 13,
          "n/a-deployed": 1, "n/a-empty": 8, "n/a-other": 1}
```

`tracked` = **有卡未关**，不是 RED；**`rust=no` 才是实现缺口，当前 = 0**。
3 条 tracked 全部 `rust=yes`（MiniMap / LineNotice / Robot），卡 `openmir2-envir-redlist-batch` 的接线已落地：

| 内容件 | recon 判定 | 现状（commit / 卡） |
|---|---|---|
| `UnbindList.txt` | **yes / implemented** | 已接 @ `4c983375c`（merge `f796eaeed`）；卡 `openmir2-unbindlist-consumer` 仍 in-progress（R 窗口真机项） |
| `AdminList.txt` | **yes / implemented** | 已接 @ `55fd7bee0`（merge `140c6aecc`；「装载活、读取死」照抄 oracle）；卡 `openmir2-envir-adminlist` 已收口 |
| `MiniMap.txt` | yes / tracked | 已加载；`CM_WANTMINIMAP` 消费点已接 @ `4d9bab444`（merge `8889f7f28`，卡 `openmir2-minimap-consumer-wire` closed @ `51a0d5342`） |
| `LineNotice.txt` | yes / tracked | `line_notice` 生产接线已存在 |
| `Robot.txt` | yes / tracked | 已加载 |

**协议派发门**（191 条入站 `CM_*` + 出站 `RM_*→SM_*`，第 4 轮实测）：

```
summary: {"n/a-oracle-noop": 1, "implemented": 172, "tracked": 15, "n/a-unreachable": 3}
inbound : implemented 70 / tracked 2 / n/a-oracle-noop 1          （共 73）
outbound: implemented 102 / tracked 13 / n/a-unreachable 3       （共 118）
```

- **入站 tracked 2**：
  - `CM_ADJUST_BONUS` —— 真接线缺口：`lib.rs:2383-2385` 仍 `client_adjust_bonus(cmd.recog, 0)`（`BonusPoint` 未挂会话态）。
  - `CM_OPENDOOR` —— **不是接线缺口**：门链已判结构性不可达（`T-65`，卡 `openmir2-door-chain-wiring` 已合 `75fa9a2d6`；
    R 卡保持 in-progress 等真机窗口）。Rust 维持 `client_open_door(false)` 即逐字节等价。
- **出站 tracked 13 = 12 个孤儿编码器 + `SM_DONATE_FAIL`**。12 帧三态以
  `docs/orphan-encoder-trigger-audit-20261023.md`（master `6d79be4da`）为准：**0 可直接接线 / 8 需补上游 / 4 不可达**。

| 帧 | 三态 | 依据 / 台账 |
|---|---|---|
| `SM_BREAKWEAPON(1102)` | 需补上游（武器升级结算） | T-174；**无卡（待建）**；审计建议细化 T-195（未登记） |
| `SM_HIDEEVENT(805)` / `SM_SHOWEVENT(804)` | 需补上游（`VisibleEvents` 差集 + `MapEvent.Id==0` 折叠） | T-178；**无卡（待建）**；审计建议 T-196（未登记） |
| `SM_LAMPCHANGEDURA(655)` | 需补上游（`UseLamp`；线上字段 `Recog=Dura`） | T-180；**无卡（待建）**；审计建议 T-194（未登记） |
| `SM_QUERYYBDEAL(23023)` / `SM_QUERYYBSELL(23021)` | 需补上游（脚本动作 343/344 + 字节尾通道） | T-181；**无卡（待建）**；审计建议 T-198（未登记） |
| `SM_RECONNECT(802)` | 需补上游（G11 跨服搬家；GM 面 perm 0 锁死） | T-182；**2026-10-24 订正：已有卡 `openmir2-sm-reconnect-wiring`，已合入 master `283a27460`（R 卡保持 in-progress）**；审计建议 T-199（未登记） |
| `SM_SKELETON(33)` | 需补上游（屠宰效果体 `Skeleton`/`BodyLeathery`） | T-183；**2026-10-24 订正：已有卡 `openmir2-sm-skeleton-wiring`（in-progress）**；审计建议 T-197（未登记） |
| `SM_CLOSEDOOR(614)` / `SM_OPENDOOR_OK(612)` | **不可达（oracle 结构性）** | T-176 + T-65（门守卫写反；`75fa9a2d6`） |
| `SM_ITEMSHOW(610)` | **不可达（oracle 结构性）** | T-179 + T-19/T-25/T-32（`MapItem.ItemId` 恒 0） |
| `SM_CHANGEFACE(1104)` | **不可达（现网部署级）** | T-175（旧措辞）；审计建议订正为 T-200（未登记），复活=三步、代价 M |
| `SM_DONATE_FAIL(765)` | **非缺陷**（oracle `RM_DONATE_FAIL` 零生产者） | 卡 `openmir2-sm-donate-fail-encoder` closed；门仍把它归 `openmir2-m3-guild-state` |

> **第 4 轮变化**：`CM_WANTMINIMAP` 与 `SM_CLOSEHEALTH` 已离开 tracked（前者 `4d9bab444`，后者 `e0114909e`），
> 所以缺席的孤儿帧是 12 而不是 13。T-177（`SM_CLOSEHEALTH`）已由卡 `openmir2-sm-closehealth-wire` 接线（R 卡等窗口）。

### 4.2 名字级可达性扫描（`pub fn` 调用点是否落在生产路径）

用同一个 `production_live_functions` 对 `bins/game-svr/src` 的全部 `pub fn` 做反查（第 4 轮实测）：

```sh
cd mir2-rs
python - <<'PY'
import sys, re
sys.path.insert(0,'tests/parity/coverage')
import recon_gates as rg
repo=rg.Path('..')
corpus=rg.RustCorpus(rg.rust_source_dirs(repo, exclude=rg.DISPATCH_EXCLUDE))
live,spans=rg.production_live_functions(corpus)
print('live',len(live),'spans',len(spans))
lines=rg._file_code_lines(corpus)
defs=[]
for s in spans:
    if 'bins' not in rg.Path(s['file']).parts: continue
    first=lines[s['file']][s['start']]
    if re.search(r'\bpub\s+(?:async\s+)?fn\s+'+re.escape(s['name'])+r'\b', first):
        defs.append(s['name'])
un=[n for n in defs if n not in live]
print('pub fns defs',len(defs),'unique',len(set(defs)))
print('unreachable pub fns defs',len(un),'unique',len(set(un)))
PY
# 第 4 轮输出：live 2739 spans 3985
#              pub fns defs 848 unique 722
#              unreachable pub fns defs 48 unique 43
```

43 个「从 `main` 不可达」的 pub fn（唯一名）分三类：

| 类别 | 数量 | 模块 / 函数 | 判定 |
|---|---:|---|---|
| 测试支撑（预期） | 15 | `conn`/`ddl_for`/`drop_db`/`fail_insert_trigger`/`fail_update_trigger`/`gate_user`/`insert_character`/`insert_index`/`open_db`/`setup_db`/`take_frames`/`test_server`/`with_now`/`with_speaker`/`with_world` | 非生产，忽略 |
| 编码器 | 24 | 与 §4.1 出站 tracking **同源的 12 个**（`sm_breakweapon`/`sm_changeface`/`sm_closedoor`/`sm_hideevent`/`sm_itemshow`/`sm_lampchangedura`/`sm_opendoor_ok`/`sm_queryybdeal`/`sm_queryybsell`/`sm_reconnect`/`sm_showevent`/`sm_skeleton`）+ **12 个旧 helper/内部帧**（`client_movefail_layout_enabled`/`frames`/`sm_guild_break_ally_ok_770`/`sm_horserun`/`sm_makedrug_fail`/`sm_makedrug_success`/`sm_movefail`/`sm_movefail_client`/`sm_movefail_frame`/`sm_run`/`sm_sendusermakedrugitemlist`/`sm_walk`） | 同源 12 个算 tracked；其余 12 个**不算缺口** |
| 其它需人工复核 | 4 | `decode_user_reply`、`default_path`、`get_hum_permission`、`parse_castle_date` | 名字级过近似：闭包/trait 对象/同名方法会误报；`get_hum_permission` 是 oracle 注释死读（D-1） |

**`crates/` 侧同口径补报**（同一个 `production_live_functions` 调用图，只去掉 `bins` 过滤）：

```sh
python - <<'PY'
import sys,re
sys.path.insert(0,'tests/parity/coverage'); import recon_gates as rg
c=rg.RustCorpus(rg.rust_source_dirs(rg.Path('..'),exclude=rg.DISPATCH_EXCLUDE))
live,spans=rg.production_live_functions(c); lines=rg._file_code_lines(c)
d=[s for s in spans if 'bins' not in rg.Path(s['file']).parts and
   re.search(r'\bpub\s+(?:async\s+)?fn\s+'+re.escape(s['name'])+r'\b', lines[s['file']][s['start']])]
u=[s['name'] for s in d if s['name'] not in live]
print(len(d), len(set(s['name'] for s in d)), len(u), len(set(u)))
PY
# 第 4 轮实测：1068 941 201 198
```

`crates/` 实测 **1068 defs / 941 unique**，其中从 `bins` 的 `main` 不可达 **201 defs / 198 unique**。
按与 `bins` 一致的三档口径（**互斥**；判定 =「该名字在全部 crate 代码里出现的次数」+ 白名单点名）：

| 档 | 数量（unique） | 判定 | 代表模块 / 例子 |
|---|---:|---|---|
| **仅定义处出现（真不可达强候选）** | **42** | 全仓代码里除定义行外零出现 ⇒ 无任何调用点 | `magic.rs` 10、`world.rs` 6、`combat.rs` 3；如 `resolve_magic`、`power_holy_shield`、`mag_sace_move` |
| **白名单点名（已登记，待逐条复核）** | **25** | 名称在 `tests/parity/whitelist.md` 出现（2 条与上档重叠，归上档） | `castle.rs::get_castle`/`repair_door`/`repair_wall`、`lists.rs::load_no_clear_mon_list`、`merchant_tax_amount` |
| **待分诊（名字级过近似）** | **131** | 有调用点，但调用者自身不可达，或经 trait object/闭包/同名方法被误判 | `castle.rs` 27、`hutil32.rs` 26、`castle_tax.rs` 10、`combat.rs` 9 |

合计 42 + 25 + 131 = **198**。判定命令见 §11 第①条附注。
**注意**：这 198 条**不是 198 个待修缺口**——第二/三档里大量是
「已接线但名字级调用图看不到」的过近似（第 3 轮已证 `chat::cry_*`、`place_npcs` 等）。
真正可判的只有第一档 42 条，仍需逐条 triage 后才能进卡。

> **归因订正**：旧稿的「bins 63 defs / 58 unique」与「`crates/` 201」**来源无法归因到 `livefns.py` 的现规则**——
> 该脚本（`E:/tmp/audit-inv-20261022/livefns.py:13`）写死 `if "/bins/" not in f: continue`，只数 bins，
> 在本 HEAD 跑出的是 **bins 48/43**。旧数字应视为早期口径/旧快照的遗留，不再沿用；本轮两套数字都按仓内 API 现算。

### 4.3 模块面结论（第 4 轮）

- **真正待接线的协议面 = 9 条**：入站 `CM_ADJUST_BONUS` 1 条 + 出站「需补上游」8 帧。
- **已判不可达/非缺陷的 6 条**：`CM_OPENDOOR`（T-65）+ `SM_CLOSEDOOR`/`SM_OPENDOOR_OK`（T-176）+
  `SM_ITEMSHOW`（T-179）+ `SM_CHANGEFACE`（T-175）+ `SM_DONATE_FAIL`（非缺陷）。门仍计 tracked，但**不许接线**。
- **内容消费件实现缺口 = 0**：`UnbindList`、`AdminList` 均已接（2026-10-23）；tracked 3 条均 `rust=yes`。
- **孤儿编码器审计的 4 个不可达帧共享一个根因**：`MapItem.ItemId`/`MapEvent.Id`/`MapDoor.DoorId`
  三处 `GetNextIdentity` 全被注释 ⇒ 三者争 `CellObjectMgr` key 0（审计 §4.3/§5⑤）。这是**登记依据**，不是接线 TODO。
- 模块文件没有「文件存在但 `mod` 未声明」的孤儿：`find bins/game-svr/src -name '*.rs'` 与 `lib.rs` 的 `mod` 声明逐一对账，
  仅 `castle/tests.rs`、`guild_official/tests.rs`、`guild_runtime/tests.rs` 三个 `tests` 子模块未声明（测试文件，正常）。

---

## 5. 与既有盘点对账（reviewer-14 Important-1）

> **本节是第 2 轮（2026-10-22，HEAD `24e9a5fab`）的历史对账记录，状态列不是第 4 轮实测。**
> 第 4 轮只更新了 §5.3 的 A 152 分档与 §5.4 的口径说明；逐条卡状态请以 board 为准。

- 对账时点：HEAD `24e9a5fab`（master `b0dde46a9`）；board 快照 2026-10-22T14:46Z（`E:/tmp/board-r2.json`）。
- 方法：四份件按标题截段后用 `openmir2-[a-z0-9-]+` 抽 id，再与 board 的列名 join；本文 §3 的四类集合用同一方法反查。可复现命令见 §8.4。
- 结论：**同一批卡在不同件里的计数口径不同，但 id 可全部映射到本文四类，无 unmapped。**

### 5.1 四份件的口径 → HEAD 实测

| 既有件 | 原口径 | HEAD 实测（2026-10-22，board + 只读源码） | 映射 / 处置 |
|---|---|---|---|
| `docs/m3-remaining-inventory-20261022.md`（基线 `c83a9ae24`） | A 22 条 / B 60 行覆盖 69 卡 / C 20 条 / D 14 条 | B 表抽出 **74 个 id**（69 卡 + 5 个表后注记引用）**全部 `进行中`，closed=0**；join = 本文 A 65 / B 3 / D 4 / C 2。A 表 22 条中：A-04 已合 `d2302986f`、A-12..A-16（MB-5..MB-9）已合 `2fa927175`/`bfd5d1e2c`/`6f31f6079`/`189932857`、A-17（SM_WALK）已合 `759320894`；A-11（MB-2）仍在飞；A-08/A-10/A-21 仍无实现卡。 | A 表逐条见 §5.2；B 表全部落在本文 A/B/C/D，无新增；A 表 6 条（A-12..A-17）从「未开工」改判「已合待 R 窗口」（条目数不变，状态变）。 |
| `docs/r-card-evidence-status-20261021.md`（基线 `ce61cdca4`） | 11 张有真机证据 / 60 张无 | 表 1 的 11 张：**10 张 `进行中` + 1 张 `已完成`**（`openmir2-login-magic-211-empty-slots`，此后已 closed）；表 2 的 60 张：**全部 `进行中`，closed=0**；join = 本文 A 56 / D 3 / C 1。 | 「60 张无证据」成立；「11 张都未 closed」**不成立**（1 张已 closed）。`657×2` 仍开放（T-147 / `openmir2-login-charstatus-657`，A 类）。 |
| `mir2-rs/tests/parity/M3-report.md` §1 + `docs/Rust重写设计文档.md` §17.5 | 3 covered / 2 partial / 1 `oracle_absent` / 5 `not_covered`（11 子行，⑩ 邮件+拍卖合一项） | **不变**：covered = 背包/装备、战斗、技能（对应卡均 closed）；partial = NPC/商店⑤、拍卖 SellOff⑩b；`oracle_absent` = 邮件⑩a（T-36）；`not_covered` = 掉落经验④、任务⑥、组队交易⑦、公会攻城⑧、英雄宠物⑨。这些卡在本文 A/B/D 里均仍开放。 | §2 的 24h 长跑仍 `not_run`（卡 `openmir2-m3-final-acceptance`，本文 A #151）；开窗前置见 §5.3。 |
| `mir2-rs/tests/parity/m3/plan-run-gap-batches-20261018.md` | 34 个专用怪 `Run` 覆写（13 真机可达 + 18 数据不可达 + 3 在飞不重复排）；玩家 `Run` 8 段（P07/P21/P22/P25/P26/P27/P31/P32）→ PL-1..PL-4；跨批前置 PR-1..PR-5 | 玩家：PL-1..PL-4 **已全部合入**（`83beb2a2e`/`41eb7fa54`/`886741b00`/`bd5dc1bc3`，均按 R 等窗口）；怪物：MB-1/3/4 已合（`a1e387a77`/`f4c16f674`/`8b2ccbe55`），MB-2 在飞（`openmir2-run-gap-mb2`），MB-5..MB-9 已合（见上）；A-10/A-21 是 PL-2/PL-3 明确留下的未实现项。 | 规划件里「未开工」的 MB-5..MB-9 / PL-1..PL-4 状态已 stale；条目本身无遗漏。PR-1（自延时队列）等前置已随 MB-3/PL-1 落地。 |

### 5.2 m3 A 表 22 条逐条映射

| # | 面（m3 件原文要点） | 本文落点 | HEAD 状态 |
|---|---|---|---|
| A-01 | 行会 NPC 标签 `@@buildguildnow`/`@@guildwar`/`@requestcastlewarnow` | A `openmir2-guild-official-npc` | 开放（已合 `8761bd68c`；R 窗口 + T-137） |
| A-02 | 城堡 NPC 标签 UserSelect | B `openmir2-castle-official-design` | 开放（设计卡） |
| A-03 | 城堡税 / 成员价接线 | B `openmir2-castle-tax-item-design` | 开放（设计卡） |
| A-04 | `account_protection` 13 列 | A `openmir2-account-protection-13` | 开放（已合 `d2302986f`；R 窗口） |
| A-05 | 脚本 `OPENYBDEAL` | B `openmir2-script-playdice-openybdeal` | 开放（bins 接线） |
| A-06 | GM `@` 命令面 | §2 GM 族表 + O `openmir2-gm-commands-design` | 开放（150/185 未实现；批次 3/3 在飞） |
| A-07 | `SS_205/206/207/211/212` 跨服广播 | B `openmir2-castle-pure`（T-113 / D-08） | 开放（单服不可达；owner 裁决） |
| A-08 | `Market_Def` 商人脚本装载 | **B-补（§3.2.1）/ §6 top5 #4** | 开放（无实现卡） |
| A-09 | `GetUpgradeStdItem` Desc 差量 / 自定义名 | A `openmir2-weapon-upgrade-values` | 开放（已合 `229d52cd9`；R 窗口） |
| A-10 | `SETOFFLINE` 的 `AutoGetExp*` 四字段 | **B-补（§3.2.1）** | 开放（无卡；`set_offline` 空默认） |
| A-11 | MB-2（4 怪 `Run`） | B `openmir2-run-gap-mb2` | 开放（在飞） |
| A-12 | MB-5（5 怪 `Run`） | A `openmir2-mb5-monsters` | 开放（已合 `2fa927175`） |
| A-13 | MB-6（4 怪 `Run`） | A `openmir2-mb2-monsters` | 开放（已合 `bfd5d1e2c`） |
| A-14 | MB-7（3 怪 `Run`） | A `openmir2-mb7-monsters` | 开放（已合 `6f31f6079`） |
| A-15 | MB-8（5 怪 `Run`） | A `openmir2-mb8-9-monsters` | 开放（已合 `189932857`） |
| A-16 | MB-9（`FireballMonster` 全链） | A `openmir2-mb8-9-monsters` | 开放（同上） |
| A-17 | 怪物 AI 移动 `SM_WALK` 不下发 | A `openmir2-monster-walk-frames` | 开放（已合 `759320894`；T-146 残余） |
| A-18 | 编码 E-3..E-8 | A `openmir2-encoding-core-e1-e2`（callsites 卡已 closed） | 部分收口：`openmir2-encoding-callsites` **已完成**；E-7 的 R 窗口仍在 |
| A-19 | LB-10 持久化证据 | A `openmir2-dbsrv-fault-paths` | 开放（已合 `de71cc01d`；LB-08/09/10 窗口） |
| A-20 | M3 系统级夹具/驱动 | A `openmir2-m3-drop-exp-align` / `-quest-align` / `-guild-castle-align` / `-hero-pet-align` / `-final-acceptance` | 开放（HEAD 上 `run_drop_exp_pair.ps1`/`run_quest_pair.ps1`/`run_guild_castle_pair.ps1`/`run_hero_pet_pair.ps1` **均在仓内**；等窗口） |
| A-21 | `SETMPAMODE DECHP` / `sys_msg` | **已解决**（`f5b401cc8`，merge `d1b13ba14`） | 已解决：分支表 + `sys_msg` 下发帧 + 宿主落库全链（T-85）；R 卡等窗口 |
| A-22 | `CastleMgr.Run/Save` 持久化排水 | B `openmir2-castle-pure` | 开放（已合 `4a0590e90`；接线 + W1-W3） |

### 5.3 开窗前置（R6/R7/R8 三红 + E1–E4 授权）

所有 A 类（152 张 = **①57 / ②45 / ③50**，见 §3.1）与 r-card 的 71 张共用同一组机器前置；`docs/m3-remaining-inventory-20261022.md` §0.2/「下一次开窗」记录 2026-10-22 **三红**：

- **R6**：窗口镜像 `f3675f69aa` 落后基线 `c83a9ae24` 的 **93 件**生产路径（guard 实跑读数）。
- **R7**：`E:/tmp/m2svr-window-target` 与 `window-provenance.json` 不存在（未在当前镜像重编六件并重录 provenance）。
- **R8**：九端口全部 LISTENING（3306 亦 UP）⇒ 无独占窗口。
- **本审计未重跑 guard**（纪律：不起进程 / 不占端口），以上为引用该件的只读读数；开窗前必须按 `docs/window-run-sheet-20261021.md` 重跑。

授权边界 **E1–E4 均待 owner 裁决**（`docs/window-run-sheet-20261021.md §E`）：

- **E1** 写库边界：E1-a 克隆 `mir2_test_*`（只能行数/关键值断言）；E1-b 生产库 + 书面授权（才可字段级 diff）。
- **E2** 内容改动：影子 Envir / GuildBase / MonGen / 非 TestServer。
- **E3** MySQL 停/起（故障注入；仅**窗口 G6**（MySQL 故障注入块）需要）。
- **E4** C# DBSrv/LoginSrv 身份不在六件 provenance 内（E4-a 用主检出构建 / E4-b 全 Rust 对照，但失去 C# oracle）。
- RNG 账本/W9 另需 **O1–O7**（`docs/window-owner-decisions-20261018.md §5`）。

⇒ 本文 §6 的 top5 中 #2/#5（落库/数值）属 R；#1 已由审计改判，仅剩 8 帧「需补上游」里含耐久两帧
（T-174 `SM_BREAKWEAPON`、T-180 `SM_LAMPCHANGEDURA`）属 R，**开窗前只能做构造级/离线片**。

### 5.4 计数口径差异（避免混用）

- m3 A/B/C/D 是**面 / 行**口径（22 / 60 / 20 / 14）。本文 §3 的卡口径已在第 4 轮改为：board 实读 203 进行中 / 5 待审 / 169 已完成；
  窗口欠账 152 张按机器分档 ①57/②45/③50。**旧的 A 152/B 22/D 8/C 2/O 1 人工四分类已停用**（不可复算，见 §7）。两者不可相加。
- m3 D 14 是 **owner 待裁决项**（含 E1–E4、O1–O7、D-08、bins 串行序等）；旧的 D 8 是从卡 `work` 文本推断的 **needs-human 卡**（已停用）。二者**不是同一集合**，交集随抽取口径变化（复算命令见 §8.4 末尾）：
  - **全件口径**（在整份 `m3-remaining-inventory-20261022.md` 抽 `openmir2-*` id）⇒ **4 张**：`gategate-keepalive`（m3 B-40）、`m5-convergence-drills`（m3 B-58 / G11）、`npc-ctor-draws`（D-12）、`tmp-cleanup-audit-redo`（D-10）；
  - **m3 D 表口径**（只抽「## D 表」段）⇒ **2 张**：`npc-ctor-draws`（D-12）、`tmp-cleanup-audit-redo`（D-10）。
  - 原稿「交集 3 张」不可复现，已按上述两口径改写；`rng-scene-pin-path` 在 m3 件里未以卡 id 出现（D-09 只引用 `m3-rng-scene-driver`），故不计入交集。

---

## 6. 下一批 top5：第 4 轮结项状态

> 第 3 轮把排序依据写成「与在跑车道 `wt-gm3` 冲突 / `lib.rs` 被占用」，该前提**已过期**：
> 第 4 轮 board 是 203 进行中 / 5 待审，多车道并行。下表按**当前 HEAD 的机器事实**逐条结项，
> 不再沿用「等某条车道合并后才能排」的表述。

| # | 单元 | 第 4 轮状态 | 剩余部分 / 证据 |
|---:|---|---|---|
| 1 | `openmir2-orphan-encoder-batch-a` | **部分解决**：卡已 closed @ `f164ccd20`（12 帧登记 T-174..T-183 + 判定表；第 13 帧 `SM_CLOSEHEALTH` 转 `openmir2-sm-closehealth-wire` @ `e0114909e`）。但**审计 `6d79be4da` 改判**：0 可直接接线 / 8 需补上游 / 4 不可达。 | 剩余 = **8 帧需补上游**（T-174/178/180/181/182/183）+ **4 帧登记不接**（T-175/176/179）。已不是「一批接线」，需按帧拆上游卡。 |
| 2 | `openmir2-unbindlist-consumer` | **已接线** @ `4c983375c`（merge `f796eaeed`）；卡仍 in-progress。`recon content` 已 `rust=yes / implemented`。 | 剩余 = **R 窗口真机**：6 条 `SM_ADDITEM(200)` + `SM_EAT_OK(635)` + `SM_WEIGHTCHANGED(622)`，以及断线存档 `hum_items` 含 6 件。 |
| 3 | `openmir2-minimap-consumer-wire` | **已解决并关卡** @ `4d9bab444`（merge `8889f7f28`；status closed @ `51a0d5342`）。`CM_WANTMINIMAP` 已离开 tracked。 | 无。 |
| 4 | `openmir2-merchant-script-load`（A-08 / T-47③） | **未开工，仍无卡**。证据：`shop.rs:36-38` 仍写「`Market_Def` 商人脚本执行 + `GotoLable("~@repair")`：本宿主未装载」；`main.rs` 无 `Market_Def` 装载序。 | 需建卡；判据与写集沿用第 3 轮表（`shop.rs`/`lib.rs`/`merchant.rs`/`main.rs`）。 |
| 5 | `openmir2-adjust-bonus-wire` | **未开工，仍无卡**。证据：`lib.rs:2383-2385` 仍 `inbound::client_adjust_bonus(cmd.recog, 0)`。 | 需建卡；**R 级**（`SM_ABILITY(52)`/`SM_SUBABILITY(752)` 两帧 + 真机字段级 diff）。 |

**补充：已解决的相邻项**

| 单元 | 状态 | 剩余 |
|---|---|---|
| `openmir2-click-path-bag-writeback` | 已合 `b0dde46a9`（R 卡保持 in-progress） | 真机落库对拍（T-160） |
| `openmir2-weapon-upgrade-values` | 已合 `229d52cd9`（R 卡保持 in-progress） | 窗口五项 |

---

## 7. 已订正/仍不确定的数字（第 4 轮）

1. **旧 A/B/D/C/O 四分类（A 152 / B 22 / D 8 / C 2 / O 1）——已停用。**
   分类脚本 `/e/tmp/audit-inv-20261022/finalcls.py` 不在仓库、且基于 185 卡快照，任何人 checkout 都无法复算。
   第 4 轮起改用两个可复算口径：① 窗口欠账看 `docs/window-debt-reclass-20261023.md` 的 **①57 / ②45 / ③50**；
   ② 协议未接线看 `recon_gates.py dispatch` 的 tracked（15，其中有效待办 9）。
2. **`crates/` 的「201 个不可达 pub fn」——已补报，但只当分诊候选。**
   仓内 API 同口径复算：**201 defs / 198 unique**（defs 1068 / unique 941）。按 §4.2 三档：
   **42 仅定义处出现（真不可达强候选）/ 25 白名单点名（已登记待复核）/ 131 待分诊（名字级过近似）**。
   **198 不是 198 个缺口**；旧稿「201 来自 livefns.py」的归因已订正（该脚本现规则只数 bins = 48/43）。
3. **§1.1 的 274 口径仍有两种合法读法**：按 **handler code + switch 分支** = 274（已实现 147 / 未实现 124 + 3 重复 code）；
   按 **唯一 handler 名 + switch 分支** = 271（已实现 147 / 未实现 124）。两者不可相加或混用；
   本文表格固定用 274（code 口径）并在 §0 脚注写明唯一名 241。

> 仍在等窗口、但数字本身可复算的不确定性：`docs/window-debt-reclass` 的 ①/②/③ 分类依赖 `tools/probe/scenarios/`
> 的 15 个场景文件——其中 12 个当前在分支 `card/openmir2-dualrun-window-burndown`，**尚未进 master**（该文件 §1 明示）。**2026-10-24 订正**：该分支已合入 master `33b40970`，`tools/probe/scenarios/` 现有 20 个场景 JSON（含上述 15 个），本条与「① 实际只有 3 个」的推论均已过期。
> 若窗口基线不含该分支，① 的「现有场景」实际只有 3 个，这是开窗前必须确认的前置。

---

## 8. 复现脚本与残留物

### 8.1 GM 族 × 实现状态 join（§2.2）

```sh
cd mir2-rs
# 1) 从设计件截出 §4.3 表（按标题截，不按固定行号，避免行号漂移）
awk '/^### 4.3 /{f=1;next} /^### 4.4 /{f=0} f' \
  tests/parity/m3/design-gm-commands-20261023.md > E:/tmp/table43.md
grep -c '^| *[0-9]* *|' E:/tmp/table43.md        # 185
# 2) join.awk（FS="|"；gmcmd.rs 取 name/imp，table 取第 3 列族 + 第 5 列现网名）
cat > E:/tmp/join.awk <<'AWK'
BEGIN{FS="|"}
FILENAME ~ /gmcmd\.rs$/ {
  if ($0 ~ /name: "/) { n=$0; sub(/.*name: "/,"",n); sub(/".*/,"",n); name=n }
  if ($0 ~ /imp: Imp::/) { i=$0; sub(/.*imp: Imp::/,"",i); sub(/,.*/,"",i); print "IMP\t" name "\t" i }
  next
}
FILENAME ~ /table43/ {
  if ($0 !~ /^\| *[0-9]+ *\|/) next
  fam=$3; nm=$5; gsub(/^ +| +$/,"",fam); gsub(/^ +| +$/,"",nm); gsub(/`/,"",nm);
  print "FAM\t" nm "\t" fam
}
AWK
awk -f E:/tmp/join.awk bins/game-svr/src/gmcmd.rs E:/tmp/table43.md > E:/tmp/pairs.tsv
# 3) 族统计
awk -F'\t' '$1=="FAM"{fam[$2]=$3} $1=="IMP"{imp[$2]=$3}
END{for(n in fam){k=fam[n];t[k]++; if(imp[n]=="Unimplemented")r[k]++;else d[k]++}
for(k in t) printf "%-16s total=%3d done=%3d remain=%3d\n",k,t[k],d[k],r[k]}' E:/tmp/pairs.tsv | sort
# 校验：FAM 185 行、IMP 185 行、名字集合双向零差集（comm -3 为空）
```

第 4 轮实测：FAM 185 / IMP 185，`done` 合计 97、`remain` 合计 88；再按 T-161/T-163 的 21 条第三态拆，
即 **76 完整 + 21 第三态 + 88 未实现**（§2.2 表）。

### 8.2 卡计数（§3.1）

```sh
walgit collab board --format json > E:/tmp/board.json
python -c "import json;d=json.load(open('E:/tmp/board.json',encoding='utf-8'));print({c['name']:len(c['cards']) for c in d['columns']})"
# 第 4 轮快照（6d79be4da @ 2026-10-23T07:31+08:00；实时量，复跑会变）：
# {'待认领': 0, '进行中': 203, '待审': 5, '需要人': 0, '受阻': 0, '已完成': 169}
```

> **旧 §8.2 的 `finalcls.py` 分类已停用**（脚本不在仓库、基于 185 卡快照、不可复算）。
> 当前 A 窗口欠账的分档见 `docs/window-debt-reclass-20261023.md` §3/§8（①57 / ②45 / ③50）。

### 8.3 生产可达性（§4.2）

```sh
cd mir2-rs
python - <<'PY'
import sys, re
sys.path.insert(0,'tests/parity/coverage')
import recon_gates as rg
repo=rg.Path('..')
corpus=rg.RustCorpus(rg.rust_source_dirs(repo, exclude=rg.DISPATCH_EXCLUDE))
live,spans=rg.production_live_functions(corpus)
print('live',len(live),'spans',len(spans))
lines=rg._file_code_lines(corpus)
defs=[]
for s in spans:
    if 'bins' not in rg.Path(s['file']).parts: continue
    if re.search(r'\bpub\s+(?:async\s+)?fn\s+'+re.escape(s['name'])+r'\b',
                 lines[s['file']][s['start']]):
        defs.append(s['name'])
un=[n for n in defs if n not in live]
print('pub fns defs',len(defs),'unique',len(set(defs)))
print('unreachable pub fns defs',len(un),'unique',len(set(un)))
PY
# 第 4 轮实测：live 2739 spans 3985
#              pub fns defs 848 unique 722
#              unreachable pub fns defs 48 unique 43
```

### 8.4 与既有盘点对账（§5；**历史脚本，第 2/3 轮**）

> **本小节的脚本依赖 `/e/tmp/wt-inventory` 与 `E:/tmp/board-r2.json`（第 2 轮快照），不是第 4 轮复算入口。**
> 第 4 轮复算入口是 §11 的三条命令；本节保留只为追溯 §5 的历史结论。

```sh
cd /e/tmp/wt-inventory
python - <<'PY'
import re,json
board=json.load(open('E:/tmp/board-r2.json',encoding='utf-8'))
status={c['id']:col['name'] for col in board['columns'] for c in col['cards']}
def load(fn,s,e):
    t=open(fn,encoding='utf-8').read(); i=t.find(s); j=t.find(e,i)
    return set(re.findall(r'openmir2-[a-z0-9][a-z0-9-]+',t[i:j]))
m3='docs/m3-remaining-inventory-20261022.md'
r='docs/r-card-evidence-status-20261021.md'
MB=load(m3,'## B 表','## C 表'); T1=load(r,'## 表 1','## 表 2'); T2=load(r,'## 表 2','## 表 3')
from collections import Counter
for name,S in [('m3-B',MB),('r-表1',T1),('r-表2',T2)]:
    print(name,len(S),Counter(status.get(x,'MISSING') for x in S))
PY
# 第 2 轮输出：m3-B 74 全进行中；r-表1 13 = 12 进行中 + 1 已完成；r-表2 60 全进行中。
# 映射到本文四类：m3-B = A 65/B 3/D 4/C 2；r-表1 = A 12 + 1 closed；r-表2 = A 56/D 3/C 1。
```

§5.4 交集复算（m3 件 vs 本文 D 8 张）：

```sh
cd /e/tmp/wt-inventory
python - <<'PY'
import re
t=open('docs/m3-remaining-inventory-20261022.md',encoding='utf-8').read()
i=t.find('## D 表'); j=t.find('**D 表计数',i)
mine={'openmir2-gategate-keepalive','openmir2-run-gap-player-run-quantification','openmir2-window-checklist',
      'openmir2-rng-scene-pin-path','openmir2-tmp-cleanup-audit-redo','openmir2-npc-ctor-draws',
      'openmir2-m5-convergence-drills','openmir2-world-bevy-migration'}
print('D 表口径:',sorted(set(re.findall(r'openmir2-[a-z0-9][a-z0-9-]+',t[i:j]))&mine))
print('全件口径:',sorted(set(re.findall(r'openmir2-[a-z0-9][a-z0-9-]+',t))&mine))
PY
# 第 3 轮输出：
# D 表口径: ['openmir2-npc-ctor-draws', 'openmir2-tmp-cleanup-audit-redo']                      （2 张）
# 全件口径: ['openmir2-gategate-keepalive', 'openmir2-m5-convergence-drills',
#            'openmir2-npc-ctor-draws', 'openmir2-tmp-cleanup-audit-redo']                      （4 张）
```

### 8.5 残留物清单（第 4 轮）

| 路径 | 内容 | 处置 |
|---|---|---|
| `E:/tmp/wt-worker-40-target/` | `mir2-script-tool` 的 release 构建产物 | 保留（构建缓存）；未提交 |
| `E:/tmp/parsestats40.json`、`E:/tmp/parsestats40.txt` | §1.2 parse-stats 的 json + stdout | 保留供复核；可随时删除 |
| `E:/tmp/recon-dispatch40.json`、`E:/tmp/recon-content40.json` | §4.1 两门的 JSON | 保留供复核；可随时删除 |
| `E:/tmp/board40.json` | §3.1 board 快照 | 保留供复核；可随时删除 |
| `E:/tmp/goalgate40/goal_gate.py` | 从 `card/goal-gate` 分支取出的终点机判脚本（§11 第③条） | 保留；不是仓库产物 |
| 仓库内 | **仅** `docs/rewrite-remaining-inventory-20261022.md` 一个文件 | 本卡唯一写集 |

- 未启动任何服务端/客户端进程；`E:/MirServer` 只读访问（parse-stats 读 639 个脚本、recon 门读部署件）。
- 未修改 `src/**`、`sql/**`、`mir2-rs/**` 中任何代码；未写 DB。

---

## 9. reviewer-14 第 1 轮处置记录（`1d3bc1e1`，tier=L）

| 编号 | 结论 | 本轮处置 |
|---|---|---|
| Important-1 | 缺「与既有盘点对账」 | 新增 §5：四份件逐一映射、HEAD closed/open、m3 A 表 22 条逐条落点、开窗前置 R6/R7/R8 + E1–E4。 |
| Important-2 | 开放 whitelist 未对账 | 本表逐条核对 20 个 T-*：**全部仍为开放登记（无退役行）**——T-161/162/163（GM 批次 2/3，L）、T-157（script take 增量帧）、T-165（动作参数 `$<int>` 表）、T-85/T-86（PL-3/PL-2）、T-21/T-23/T-30/T-51/T-53/T-55/T-93/T-98/T-117/T-137/T-142/T-145/T-147 均为对应卡的未实现/待窗口登记。 |
| Important-3 | A-10/A-21 未进 B 类 | 新增 §3.2.1「B-补：无卡可派的代码面」，列 A-10（`set_offline` 空默认）、A-21（`ActionOfSetMapMode` 无分支/`sys_msg` 空默认），并补 A-08（原稿漏列）。 |
| Important-4 | §4.1 SM_DONATE_FAIL 无编码器；§4.2「25 同源」只对上 13 | §4.1 拆成 13 帧（有编码器）+ 1 帧（`SM_DONATE_FAIL`，无编码器，卡 `m3-guild-state`）；§4.2 明写 25 个里仅 13 个对应 tracked，其余 12 个不是缺口。 |
| Important-5 | §5「活跃 worktree」取自 board 历史字段；top5 #4 冲突不成立 | §6 改为 `git worktree list` 实测（唯一在跑 `wt-gm3`；`wt-clickbag` 已合并、`OpenMir2-wt-goods` 干净/parked）；**原 top5 #4（现 §6 #1 orphan-encoder）**删除「`m3-drop-exp-wiring`/`m3hp` 冲突」表述，只保留 lib.rs 冲突。 |
| Important-6 | top5 #5 写集只写 gmcmd.rs 不成立 | **原 top5 #5（GM batch-4）已不采纳**；本轮 §6 无 GM 单元。证据保留：GM 批次 2 `ed5988d23` 改 `gmcmd.rs` + `lib.rs`(+205) + `whitelist.md`、批次 3 `bdf4e0ab0` 改 `gmcmd.rs` + `lib.rs`(+15) + `whitelist.md` ⇒「仅 gmcmd.rs」的说法不成立。 |
| Minor-1 | finalcls.py 是硬编码；A #96 应归 C | §3.1 明写「人工逐卡指派、非正则」；`rng-consumer-structure-parity` 移入 C，A 计数保持 152（移出 1、移入 `click-path-bag-writeback` 1）。 |
| Minor-2 | board「需要人」实测 0；D=8 是推断 | §3.1 口径第 1 条明写。 |
| Minor-3 | §2.2 G2–G6 分解不可互斥，建议整节删除 | 已删除原 §2.2；族表升为 §2.2，§7 原第 1 项一并删除。 |
| 复核者 top5 | orphan-encoder-batch-a / unbindlist / minimap / merchant-script-load / adjust-bonus | **全部采纳**（§6 五条按此排序），并逐条重核写集：五条均与 `wt-gm3` 的 `lib.rs` 冲突，须串行等其合并；`SM_DONATE_FAIL` 已从 #1 剔除。 |


---

## 10. reviewer-14 第 2 轮（`c3e551d8`，tier=L）处置记录（第 3 轮）

| 编号 | 结论 | 本轮处置 |
|---|---|---|
| **Important-1（唯一阻塞）** | §0/§2.2 把 35 条 GM 记为「行为完成」，但 T-161（14）+ T-163（7）= 21/35 只是「注册已实现 / 宿主未接线」 | §0 汇总表拆成 `16 完整（14+2 内建）`；§2.1 引入**第三态**行（21 条 = T-161 的 14 + T-163 的 7，附 `grep -cE` 复算命令 ⇒ 21）；§2.2 族表改为 `总数/完整实现/第三态/未实现` 四列（14/21/150），并新增「第三态定义」段：文本已逐字节、宿主动作未接 ⇒ 真机行为只能是「文本 + 零副作用」，按 T-161/T-163 登记为已登记偏差；另加脚注区分「按族登记口径 21」与「可观测缺口口径 10」。 |
| Minor-1 | §8.2 的 finalcls.py 输出 152/22/8/2/1 为假；finalcls.py/txt 未列残留物 | **已更新脚本并重跑**：`finalcls.py` 增 `rng-consumer-structure-parity` 到 C、从 O 移除 `click-path-bag-writeback`（落回默认 A）；实跑 `Counter({'a': 152, 'b': 22, 'd': 8, 'c': 2, 'o': 1})`。§8.2 改为记录实跑输出（A152/B22/D8/C2/O1）；§8.5 把 `finalcls.py` / `finalcls.txt` 列入残留物并注明为 §8.2 唯一事实源。 |
| Minor-2 | §5.4「交集 3 张」不可复现；全件口径 4 张、m3 D 表口径 2 张 | §5.4 改按两口径并列：全件 **4 张**（+`tmp-cleanup-audit-redo`）、m3 D 表 **2 张**（`npc-ctor-draws`/`tmp-cleanup-audit-redo`），并注明 `rng-scene-pin-path` 未以卡 id 出现在 m3 件；§8.4 末尾补复算命令（实跑输出与上述一致）。 |
| Minor-3 | §6 的 wt-gm3 快照 stale；措辞宜改「Rust 代码车道」 | §6 更新为 `HEAD e39a08963`、工作区干净（`status --porcelain` = 0 行），说明原「未提交修改」已随 R1 修订提交落盘；措辞改「唯一在跑的 **Rust** 代码车道」，并补 `OpenMir2-wt-goods` 的 diff 只有 `src/GameSrv/Npc/Merchant.cs`（C#）⇒ 不是 Rust 车道；补 `m2svr-window-origin` @ `229d52cd9`、`wt-inventory` 只改文档。 |
| Minor-4 | §2.2「桩/测试 8」与「桩/测试 7」不一致 | 统一为：桩/测试族 **8 = T-163 的 7 + G1 的 `AddRobotPlay`**；§2.2 族表「完整实现 1（`AddRobotPlay`）+ 第三态 7（T-163）」，正文与 §2.1 同步。 |
| Minor-5 | §5.3 E3 的「G6」与 GM 批次混淆 | 改为「仅**窗口 G6**（MySQL 故障注入块）需要」。 |

- 本轮仍是**纯文档**：只改 `docs/rewrite-remaining-inventory-20261022.md`；只读 `src/**`、`sql/**`、`E:/MirServer/**`；
  未起进程、未占端口、未写 DB。所有数字均给出命令与实跑输出（§8.1–§8.4）。

---

## 11. 复核方式（三条命令复现本文所有数字）

> 三条命令都在仓库根执行（第①条会 `cd mir2-rs`）。`<任选空目录>` 是占位符（任选一个空目录即可，勿写死别人的 worktree）。第③条 `goal_gate.py` 尚未进 master
> （卡 `openmir2-goal-gate`，分支 `card/goal-gate` @ `807b44e6b`，复审中），所以用 `git show` 取脚本。

```sh
# ① 协议派发门 + 内容门（§4.1 的 172/15/3/1 与 13/3/24）
cd mir2-rs
python tests/parity/coverage/recon_gates.py dispatch --repo ..
python tests/parity/coverage/recon_gates.py content  --repo ..
# 期望（静态，不随卡移动）：dispatch summary {"n/a-oracle-noop":1,"implemented":172,"tracked":15,"n/a-unreachable":3}
#       content summary {"n/a-oracle-noop":9,"n/a-db":5,"tracked":3,"implemented":13,
#                        "n/a-deployed":1,"n/a-empty":8,"n/a-other":1}

# ② 脚本语料 100% + GM 面（§1.1/§1.2/§2 的 106/0、185/88/21）
cd mir2-rs
CARGO_TARGET_DIR=<任选空目录> cargo run --release --locked -p mir2-script-tool -- \
  parse-stats E:/MirServer/M2GameSvr/Envir --json E:/tmp/parsestats.json --require-implemented
awk '/^pub const COMMANDS/,/^];/' bins/game-svr/src/gmcmd.rs | grep -c "name:"
awk '/^pub const COMMANDS/,/^];/' bins/game-svr/src/gmcmd.rs | grep -c 'imp: Imp::Unimplemented'
# 期望：语料派发目标覆盖：已实现 106 个（11829 次调用）；未实现 0 个
#       185 / 88（⇒ 已实现 97 = 76 完整 + 21 第三态）

# ③ 看板 + 终点四条件（§3.1 快照 203/5/169；条件2 在快照时 FAIL = 5 张 R 卡无证据无登记；两者均为实时量）
walgit collab board --format json | python -c "import sys,json;d=json.load(sys.stdin);print({c['name']:len(c['cards']) for c in d['columns']})"
git show card/goal-gate:tools/goal/goal_gate.py > E:/tmp/goal_gate.py
python -I -B E:/tmp/goal_gate.py --repo . --full
# 期望 board（快照 6d79be4da @ 2026-10-23T07:31+08:00；**实时量，复跑以当次输出为准**）：
# {'待认领':0,'进行中':203,'待审':5,'需要人':0,'受阻':0,'已完成':169}
# goal_gate --full：条件1 PASS / 条件2 FAIL（快照时 5 张；**R 卡集合是实时量**，复审当日为 6 张）/ 条件3 PASS / 条件4 由门禁结果决定；总体 FAIL
```

| 本文数字 | 来源命令 |
|---|---|
| 106/106、0 未实现 | ② parse-stats 的「语料派发目标覆盖」行 |
| 185、88、21、76 | ② 的 awk 两条 + §2.1 第三态 grep |
| 172 / 15 / 3 / 1 | ① dispatch summary |
| 13 / 3 / 24 | ① content summary |
| 203 / 5 / 169 | ③ board |
| 条件② 5 张 R 卡（快照时；实时量） | ③ goal_gate 的 FAIL 明细 |
| 147/274、62/61/123、①57/②45/③50 | §1.1 / §3.1 内的专门脚本（第 4 轮已附实测输出） |
