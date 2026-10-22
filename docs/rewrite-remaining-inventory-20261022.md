# OpenMir2 重写剩余量清单（可复现盘点）

- 采集时间：第 1 轮 2026-10-22T14:34Z；第 2 轮复核 2026-10-22T14:46Z；**第 3 轮（处置第 2 轮复核）2026-10-22T15:10Z（本文件当前版本）**
- 采集点：worktree `/e/tmp/wt-inventory`；第 1 轮 HEAD `0e7da10b8`；第 2 轮 `git merge master` 后 HEAD `24e9a5fab4647c3b12d02d1a7c62e12b462667ab`（= master `b0dde46a9` + 本文件第 1 轮提交的合并树）；**第 3 轮基于第 2 轮文档提交 `019b64e88`（树 = `24e9a5fab` + 文档改动），本轮只改本文档**
- 分支：`inventory-20261022`（第 3 轮在 `019b64e88` 上处置；master 未再前进，仍 `b0dde46a9`）
- 复核输入：reviewer-14 第 1 轮 `request_changes`（review oid `1d3bc1e1`）+ **第 2 轮 `request_changes`（review oid `c3e551d8`）**，均 tier=L；逐条处置见 §9（第 1 轮）/ §10（第 3 轮）
- 只读面：`src/**`、`sql/**`、`E:/MirServer/**`；未起进程、未占端口、未写 DB
- 口径：**每个数字后面给「命令 → 输出摘要」**；估算一律标 `估算`，不计入结论。

> **一句话结论**：脚本引擎在**部署语料**上已 100% 覆盖（106/106 个派发目标、0 未实现）；真正的剩余量集中在
> **① 185 张 in-progress 卡里 152 张是 R 级真机窗口欠账**、**② GM 命令 185 条里 150 条命令体未实现，另 21 条只到「文本已逐字节、宿主动作未接」（第三态）**、
> **③ 协议出站 13 帧有编码器但无生产发射点（另 1 帧 `SM_DONATE_FAIL` 连编码器都没有）**、**④ 内容消费件缺口已清零（UnbindList、AdminList 均已接，2026-10-23）**。

> **快照后变化（2026-10-22T15:30Z，master `7fa7a2e99`）**：GM 第 3 批（传送/召唤族 18 条）已合入，
> 因此本文档 §2 的「已实现 35 / 未实现 150」更新为 **已实现 53 / 未实现 132**（第三态 21 不变，
> 精确到「185 − 5 − 30 − 18 = 132」）；其余各节数字仍是 `0e7da10b8` 快照，未变。

---

## 0. 汇总数字（全部可复现）

| 维度 | 总量 | 已完成 | 未完成/欠账 | 证据 |
|---|---:|---:|---:|---|
| 脚本派发目标（注册 + switch） | 274 | 148 | 126 | §1.1 |
| 脚本：部署语料实际用到的派发目标 | 106 | **106** | **0** | §1.2 |
| GM 命令（185 条表 + 内建 2） | 187 | 16（14 完整 + 2 内建） | 150 未实现 + **21 第三态**（文本已逐字节 / 宿主动作未接） | §2 |
| 协议派发（入站 + 出站） | 191 | 170 | 17 tracked + 3 n/a-unreachable | §4.1 |
| 内容加载调用点 | 40 | 11 implemented + 24 n/a | 5 tracked（其中 `rust=no` 2） | §4.1 |
| 协作卡（in-progress） | 185 | — | A 152 / B 22（+3 条无卡面）/ D 8 / C 2 / O 1 | §3 |

---

## 1. 脚本引擎派发面（`crates/script`）

### 1.1 注册 / 已实现 / 未实现

> 口径说明：设计文档与 `dispatch.rs` 模块头提到的 `coverage_report` **在当前 HEAD 不是函数名**——
> `grep -rn coverage_report crates/script` 只命中 `dispatch.rs:7` 的文档注释。实际实现拆成
> `dispatch.rs` 的三态查询（`condition_target_status`/`execution_target_status`）+
> `crates/script-tool/src/parsestats.rs::coverage_lines`。本节的数字按后者复算。

| 类别 | 注册表 | 已实现 handler | 未实现 handler | switch 分支（注册/已实现） | 目标合计（已实现/总） |
|---|---:|---:|---:|---:|---:|
| 条件 | 111 | 49 codes（48 names） | **62** | 10 / 10 | 59 / 121 |
| 动作 | 133 | 70 codes（69 names） | **63** | 20 / 19 | 89 / 153 |
| 合计 | 244 | 119 codes（117 names） | **125** | 30 / 29 | **148 / 274** |

> 计数口径：上表「已实现/未实现」按**注册 code** 数；`CONDITION_HANDLERS` 有 1 个 handler 名复用到 2 个 code，
> `EXECUTION_HANDLERS` 有 2 个名字复用 ⇒ **按 handler 名去重**的未实现数为 62（条件）/ 62（动作）。

复算命令（在 `mir2-rs/` 下）：

```sh
python - <<'PY'
import re
d=open("crates/script/src/engine/dispatch.rs",encoding="utf-8").read()
h=open("crates/script/src/handlers.rs",encoding="utf-8").read()
def arr(t,n):
    m=re.search(r"(?:pub )?(?:static|const) "+n+r"[^=]*=\s*&\[(.*?)\];",t,re.S)
    return re.findall(r'"([^"]+)"',m.group(1))
for n in ["CONDITION_HANDLERS","EXECUTION_HANDLERS","ENGINE_SWITCH_CONDITIONS","ENGINE_SWITCH_EXECUTIONS"]:
    print(n,len(arr(h,n)))
for n in ["IMPLEMENTED_CONDITIONS","IMPLEMENTED_EXECUTIONS","IMPLEMENTED_SWITCH_CONDITIONS","IMPLEMENTED_SWITCH_EXECUTIONS"]:
    print(n,len(arr(d,n)))
PY
# 输出摘要：CONDITION_HANDLERS 111 / EXECUTION_HANDLERS 133 / ENGINE_SWITCH_CONDITIONS 10 /
#           ENGINE_SWITCH_EXECUTIONS 20 / IMPLEMENTED_CONDITIONS 48 / IMPLEMENTED_EXECUTIONS 69 /
#           IMPLEMENTED_SWITCH_CONDITIONS 10 / IMPLEMENTED_SWITCH_EXECUTIONS 19
```

唯一的 **switch 未实现项 = `TakecheckItem`**（`ENGINE_SWITCH_EXECUTIONS` 20 条中缺 1；
`set(ENGINE_SWITCH_EXECUTIONS) - set(IMPLEMENTED_SWITCH_EXECUTIONS)` ⇒ `{'TakecheckItem'}`）。
它不在部署语料里（见 §1.2），所以不造成语料缺口。

未实现的 62 个条件 / 62 个动作 handler 名（完整清单）：

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
ConditionOfHaveMaster, ConditionOfHour, ConditionOfIsHigh, ConditionOfMin,
ConditionOfPoseHaveMaster, ConditionOfReviveSlave
动作：ActionOfAddAccountList, ActionOfAddGuildList, ActionOfAddIpList, ActionOfAddNameDateList,
ActionOfBonusPoint, ActionOfChangeGender, ActionOfChangeJob, ActionOfChangeNameColor,
ActionOfChangePerMission, ActionOfClearList, ActionOfClearMakeItems, ActionOfClearNeedItems,
ActionOfClearPassword, ActionOfClearSkill, ActionOfDelGuildList, ActionOfDelIpList,
ActionOfDelMarry, ActionOfDelMaster, ActionOfDelNameDateList, ActionOfDelNoJobSkill,
ActionOfDelSkill, ActionOfDivData, ActionOfExchangeMap, ActionOfExeaction, ActionOfGetMarry,
ActionOfGetMaster, ActionOfGroupAddList, ActionOfGroupMoveMap, ActionOfGuildAuraePoint,
ActionOfGuildBuildPoint, ActionOfGuildChiefItemCount, ActionOfGuildFlourishPoint,
ActionOfGuildRecall, ActionOfGuildstabilityPoint, ActionOfKillSlave, ActionOfKillSlaveName,
ActionOfMapTing, ActionOfMaster, ActionOfMonClear, ActionOfMonGen, ActionOfOpenMagicBox,
ActionOfPercentData, ActionOfPkPoint, ActionOfPkZone, ActionOfQueryBagItems,
ActionOfQuerySaleSell, ActionOfQueryTrustDeal, ActionOfRandomMove, ActionOfReCallMap,
ActionOfRecallmob, ActionOfRestBonusPoint, ActionOfSetMapMode, ActionOfSetMemberLevel,
ActionOfSetMemberType, ActionOfSetRandomNo, ActionOfSetRankLevelName, ActionOfSkillLevel,
ActionOfSumData, ActionOfTakeCastleGold, ActionOfThrowitem, ActionOfUnMaster,
ActionOfUpgradeItems
```

### 1.2 部署语料实测覆盖（关键判据）

```sh
CARGO_TARGET_DIR=/e/tmp/wt-inventory-target \
  cargo run --release -p mir2-script-tool -- parse-stats /e/MirServer/M2GameSvr/Envir \
  --json /tmp/parsestats.json --require-implemented
```

输出摘要（rc=0）：

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
awk '/^pub const COMMANDS/,/^];/' bins/game-svr/src/gmcmd.rs | grep -o "imp: Imp::[A-Za-z]*" | sort | uniq -c
# 150 imp: Imp::Unimplemented；其余 35 条各有具名 Imp
# 第三态（T-161 的 14 + T-163 的 7 = 21 条具名 Imp）：
awk '/^pub const COMMANDS/,/^];/' bins/game-svr/src/gmcmd.rs | grep -cE 'imp: Imp::(ReloadAllGuild|ReloadGuild|ReloadLineNotice|ReloadManage|ReloadRobotManage|ReLoadAdmin|LoadAdmin|ReloadItemDb|ReloadMagicDb|ReloadMonItems|ReloadRobot|ReloadAbuse|ReloadNpc|NpcScript|TestFire|FireBurn|TestStatus|BackStep|UserCmd|TestGetBagItems|TestSpeedMode),'
# 21
```

| 项 | 条数 | 证据 |
|---|---:|---|
| 命令表 `COMMANDS` | **185** | `grep -c "name:"` ⇒ 185；单测 `COMMANDS.len()==185` |
| 内建命令 | **2**（`@help`、`@commands`） | `BUILTIN_HELP`/`BUILTIN_COMMANDS`，表外处理 |
| 已实现命令体（具名 `Imp`） | **35**（G1 5 + 批次 2/3 30） | `Imp::Unimplemented` = 150 ⇒ 185−150 = 35 |
| 　├ 完整实现 | **14**（封禁名单 9 + G1 5） | 封禁 9 条按 T-162（oracle 盘点操作全为注释 ⇒ 空表分支即等价）；G1 5 条无宿主缺口 |
| 　└ **第三态：文本已逐字节、宿主动作未接** | **21**（T-161 的 14 + T-163 的 7） | `whitelist.md:654` / `whitelist.md:656`；定义与脚注见 §2.2 |
| 未实现命令体 | **150** | 同上 |
| perm 0（现网可达） | **5** | `perm_min: 0` ⇒ `AddRobotPlay`/`LockLogin`/`拒绝`/`拒绝私聊`/`时间`；加内建 `@help` = 6 |

### 2.2 按 §4.3 的 12 族分解（精确、互斥）

```sh
# 解析 §4.3 表（185 行）× gmcmd.rs 的 (name, imp) 做 join；族取表格第 3 列。
# 复算脚本见 §8「复现脚本」join.awk。
# 输出摘要（每族 total/done/remain；done 再按 §2.1 的第三态 21 条拆成「完整 / 第三态」）：
```

| 族 | 总数 | 完整实现 | 第三态（文本级） | 未实现 |
|---|---:|---:|---:|---:|
| 数值/角色 | 44 | 0 | 0 | **44** |
| 其它 | 35 | 4 | 0 | **31** |
| 传送/召唤 | 17 | 0 | 0 | **17** |
| 行会/城堡/赛事 | 16 | 0 | 0 | **16** |
| 查询/展示 | 16 | 0 | 0 | **16** |
| 重载/管理 | 14 | 0 | **14**（T-161） | 0 |
| 造物/刷怪 | 9 | 0 | 0 | **9** |
| 封禁名单 | 9 | **9**（T-162） | 0 | 0 |
| 桩/测试 | 8 | **1**（G1 `AddRobotPlay`） | **7**（T-163） | 0 |
| 禁言/监听 | 7 | 0 | 0 | **7** |
| 仓库/登录密码 | 6 | 0 | 0 | **6** |
| 会话控制 | 4 | 0 | 0 | **4** |
| **合计** | **185** | **14** | **21** | **150** |

具名 `Imp` 的 35 条 = **完整实现 14** + **第三态 21**。按族：重载/管理 14（T-161）、封禁名单 9（T-162）、
桩/测试 8 = T-163 的 7 + G1 的 `AddRobotPlay`、其它 4（G1 除 `AddRobotPlay` 外的四条，即
`时间/拒绝/拒绝私聊/LockLogin`）。14 + 21 = 35；与 §2.1 的 `Imp::Unimplemented` = 150 合并即 185。

**第三态定义（本轮新增，回应 reviewer-14 第 2 轮唯一阻塞 Important）**：命令体已把 oracle 文本**逐字节**复刻
（`SysMsg` 文本、分支、越界怪癖照抄），但**宿主动作未接线**（`whitelist.md:654` T-161 的重载/管理 14 +
`whitelist.md:656` T-163 的桩/测试 7）。⇒ 这 21 条的真机可观测行为**只能是「回 oracle 文本 + 零副作用」**，
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

### 3.1 总览

```sh
walgit collab board --format json > E:/tmp/board-r2.json
python -c "import json;d=json.load(open('E:/tmp/board-r2.json',encoding='utf-8'));print({c['name']:len(c['cards']) for c in d['columns']})"
# {'待认领': 1, '进行中': 185, '待审': 0, '需要人': 0, '受阻': 0, '已完成': 153}
```

**in-progress = 185 张**；`待认领 = 1` 是本次审计卡 `openmir2-rewrite-inventory` 自身，不进入分类。
分类口径（**人工逐卡指派**；集合硬编码在 `/e/tmp/audit-inv-20261022/finalcls.py`，**不是**从 `work` 文本正则自动生成）：

- **A 真机窗口欠账（R 级缺证据）**：已合并且最新 status 明示窗口/真机/`tier=R` 欠账 ⇒ **152**（第 2 轮：移出 `rng-consumer-structure-parity`、移入已合的 `click-path-bag-writeback`）
- **B 代码未开工/接线未做**：最新 status 明示 `实现未开`/`第②片`/`接线`/`另开卡` ⇒ **22**（另有 3 条**无卡可派**的代码面 B-补，见 §3.2.1，不计入 22）
- **D needs-human**：最新 status 明示 owner 裁决/排期/删除授权 ⇒ **8**
- **C 已合但卡未关（书务/已声明收口）**：**2**（`gate-gmclose-race`、`rng-consumer-structure-parity`）
- **O 在飞/待审**（不属四类，单列）⇒ **1**（`gm-commands-design`；批次 3 在 `wt-gm3` 未提交）

> 口径（对应 reviewer-14 Minor）：
> 1. board 的 `需要人` 列实测 **0**；本文 D=8 是**按 `work` 文本中「owner 裁决/排期/授权」推断**的，不是 board 列计数。
> 2. `work` = 该卡最新一次 `status` 的快照，可能 stale；与源码冲突时以 §5 对账为准。
> 3. `rng-consumer-structure-parity` 的 `work` 先写「三项余量已全部收口」、尾部又列 R 级真机窗口；按复核意见归 **C**，窗口尾巴保留在 C 表「备注」列，不当作 A 类欠账。
> 4. `click-path-bag-writeback` 已有 master 合并 `b0dde46a9`（见其 `work`），故从「在飞」改归 **A**（T-160 明示真机落库对拍待窗口）。

### 3.2–3.6 逐卡清单

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
| `SETMPAMODE DECHP` / `sys_msg` | m3 A-21 / T-85 | `crates/script/src/handlers.rs:211` 注册 `(216, "ActionOfSetMapMode")`，但 `engine/actions.rs` **无实现分支**；`engine/mod.rs:384` 的 `ScriptPlayer::sys_msg` 空默认，`NpcPlayer`（`npc.rs:967`）未覆写；`HungerDecPower` 无消费者 | 无卡；tier=R（内容启用后）。建议随 `openmir2-run-gap-pl3` 后续片 |
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
python tests/parity/coverage/recon_gates.py content  --repo .. --json /tmp/recon-content.json
python tests/parity/coverage/recon_gates.py dispatch --repo .. --json /tmp/recon-dispatch.json
```

**内容加载门**（40 个 oracle `Load*` 调用点）：

```
summary: {"n/a-oracle-noop": 9, "n/a-db": 5, "tracked": 5, "implemented": 11,
          "n/a-deployed": 1, "n/a-empty": 8, "n/a-other": 1}
```

`tracked` = 缺口已指派卡（不是 RED）；其中 **`rust=no` 才是实现缺口**：

| 内容件 | rust 判定 | 现状 |
|---|---|---|
| `UnbindList.txt` | **yes** | 2026-10-23 已接（`bins/game-svr/src/unbindlist.rs` + `useitem.rs` 解包 6 件；recon `rust=yes / implemented`）。历史：loader 在 `crates/data/src/lists.rs`，此前 game-svr 消费点未接（白名单 T-17） |
| `AdminList.txt` | **no** | loader 在 `crates/data/src/lists.rs`，**无生产消费者**；oracle 的 `GetHumPermission` 被注释 ⇒ 与「permission 恒 0」裁决一致（D-1） |
| `MiniMap.txt` | yes | 已加载；但 `CM_WANTMINIMAP` 消费点仍传 `0`（见 §6 top5 #3） |
| `LineNotice.txt` | yes | `line_notice` 生产接线已存在 |
| `Robot.txt` | yes | 已加载 |

**协议派发门**（191 条入站 `CM_*` + 出站 `RM_*→SM_*`）：

```
summary: {"n/a-oracle-noop": 1, "implemented": 170, "tracked": 17, "n/a-unreachable": 3}
inbound : implemented 69 / tracked 3 / n/a-oracle-noop 1     （共 73）
outbound: implemented 101 / tracked 14 / n/a-unreachable 3   （共 118）
```

- **入站 tracked 3**：`CM_ADJUST_BONUS`、`CM_OPENDOOR`、`CM_WANTMINIMAP`。
  三者都不是「无分支」：`inbound.rs` 已有 handler，但 `lib.rs` 生产分支分别存在
  「BonusPoint 未挂会话态」「门表恒 false / 结构性 no-op」「MinMap 传 0」的接线缺口（见 §6）。
- **出站 tracked 14 = 13 个「有编码器、无生产发射点」+ 1 个 `SM_DONATE_FAIL`（连编码器都没有）**：

```
13 帧（编码器定义：smframe_ext.rs 12 个 + smframe.rs 的 sm_skeleton）：
SM_BREAKWEAPON, SM_CHANGEFACE, SM_CLOSEDOOR, SM_CLOSEHEALTH, SM_HIDEEVENT,
SM_ITEMSHOW, SM_LAMPCHANGEDURA, SM_OPENDOOR_OK, SM_QUERYYBDEAL, SM_QUERYYBSELL,
SM_RECONNECT, SM_SHOWEVENT, SM_SKELETON

另 1 帧：SM_DONATE_FAIL —— `grep -rn 'sm_donate_fail' bins/game-svr/src` = 0；
recon 门把它归卡 `openmir2-m3-guild-state`（不是 orphan-encoder-wiring）。
```

> 卡 `openmir2-orphan-encoder-wiring` 的最新 status 写「片103 落地；余量=真机窗口项」，
> 但**当前 HEAD 的派发门仍判 13 帧 `rust=no`**——以门为准，这 13 帧没有生产发射点。
> `SM_DONATE_FAIL` 与它们不同源（无编码器；oracle 失败支属行会/城堡链路），不得并进同一批。

### 4.2 名字级可达性扫描（`pub fn` 调用点是否落在生产路径）

用同一个 `production_live_functions` 对 `bins/game-svr/src` 的全部 `pub fn` 做反查：

```sh
# 见 §8.3 livefns.py；结果：
live fns 2638 spans 3873
pub fns in bins defs 842 unique 724
unreachable pub fns defs 63 unique 58
```

58 个「从 `main` 不可达」的 pub fn 分三类：

| 类别 | 数量 | 模块 / 函数 | 判定 |
|---|---:|---|---|
| 测试支撑（预期） | 13 | `testsupport.rs`：`setup_db/open_db/insert_character/...` | 非生产，忽略 |
| 编码器（多数与 §4.1 出站缺口**不同源**） | 25 | `smframe_ext.rs` 15、`smframe.rs` 9、`guild_frames.rs::sm_guild_break_ally_ok_770` | **仅 13 个**对应派发门 tracked；其余 12 个是旧 helper/内部帧（`sm_makedrug_fail`/`sm_makedrug_success`/`sm_sendusermakedrugitemlist` 3；`smframe.rs` 的 `client_movefail_layout_enabled`/`frames`/`sm_horserun`/`sm_movefail`/`sm_movefail_client`/`sm_movefail_frame`/`sm_run`/`sm_walk` 8；`sm_guild_break_ally_ok_770` 1），**不算缺口** |
| 其它需人工复核 | 20 | `config::default_path`、`chat::{cry_text,cry_hear_frame}`、`inbound::{client_make_drug_item,sm_butch}`、`npcs::{build_npc_spawns,place_npcs}`、`selloff::{get_sell_off_gold,select_sell_date}`、`ability::item_names`、`bag::from_slots`、`castle::parse_castle_date`、`gamegold::auto_tick`、`homemap::new_human_spawn`、`merchant::spawn_merchants`、`mongen::venture_server_default_is_false`、`npc::run_std_mode_func`、`record::encode_player_data`、`shop::is_merchant`、`trade::deal_cancel_a` | **名字级过近似**：闭包/trait 对象/同名方法会误报。已抽查 `chat::cry_*` 是**故意的**（oracle `CryCry` NRE，T-118，零帧），`place_npcs`/`spawn_merchants` 的生产入口走 `*_with_scripts` 变体 ⇒ 多数非真缺口 |

`crates/` 同口径：204 个 def / 201 个 unique 名字从 bins `main` 不可达；**不建议直接当缺口清单**——
`hutil32.rs`（27）、`castle.rs`（32）、`lists.rs`（18）、`castle_tax.rs`（13）、`combat.rs`（13）、
`magic.rs`（13）、`world.rs`（13）里混有纯工具函数、测试专用 API 与动态派发。需要按卡逐项 triage。

### 4.3 模块面结论

- **接线缺口**：协议出站 13 帧（+1 帧 `SM_DONATE_FAIL` 缺编码器）仍在；内容消费 2 件（UnbindList、AdminList）已于 2026-10-23 关闭。
- 入站 3 条 tracked 是「handler 已有、生产参数未挂」的**小接线**，不是重写缺口。
- 模块文件本身没有「文件存在但 `mod` 未声明」的孤儿：`find bins/game-svr/src -name '*.rs'` 与
  `lib.rs` 的 `mod` 声明逐一对账，仅 `castle/tests.rs`、`guild_official/tests.rs`、`guild_runtime/tests.rs`
  三个 `tests` 子模块未声明（测试文件，正常）。

---

## 5. 与既有盘点对账（reviewer-14 Important-1）

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
| A-21 | `SETMPAMODE DECHP` / `sys_msg` | **B-补（§3.2.1）** | 开放（无卡；`ActionOfSetMapMode` 无实现分支） |
| A-22 | `CastleMgr.Run/Save` 持久化排水 | B `openmir2-castle-pure` | 开放（已合 `4a0590e90`；接线 + W1-W3） |

### 5.3 开窗前置（R6/R7/R8 三红 + E1–E4 授权）

所有 A 类（152 张）与 r-card 的 71 张共用同一组机器前置；`docs/m3-remaining-inventory-20261022.md` §0.2/「下一次开窗」记录 2026-10-22 **三红**：

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

⇒ 本文 §6 的 top5 中 #2/#5（落库/数值）与 #1 的耐久帧属 R，**开窗前只能做构造级/离线片**。

### 5.4 计数口径差异（避免混用）

- m3 A/B/C/D 是**面 / 行**口径（22 / 60 / 20 / 14），本文 §3 是**卡**口径（A 152 / B 22 / D 8 / C 2 / O 1）；两者不可相加。
- m3 D 14 是 **owner 待裁决项**（含 E1–E4、O1–O7、D-08、bins 串行序等）；本文 D 8 是从卡 `work` 文本推断的 **needs-human 卡**。二者**不是同一集合**，交集随抽取口径变化（复算命令见 §8.4 末尾）：
  - **全件口径**（在整份 `m3-remaining-inventory-20261022.md` 抽 `openmir2-*` id）⇒ **4 张**：`gategate-keepalive`（m3 B-40）、`m5-convergence-drills`（m3 B-58 / G11）、`npc-ctor-draws`（D-12）、`tmp-cleanup-audit-redo`（D-10）；
  - **m3 D 表口径**（只抽「## D 表」段）⇒ **2 张**：`npc-ctor-draws`（D-12）、`tmp-cleanup-audit-redo`（D-10）。
  - 原稿「交集 3 张」不可复现，已按上述两口径改写；`rng-scene-pin-path` 在 m3 件里未以卡 id 出现（D-09 只引用 `m3-rng-scene-driver`），故不计入交集。

---

## 6. 下一批 top5（可立刻派）

排序依据：**现网可见行为 / 风险 / 写集冲突**。**当前唯一在跑的 Rust 代码车道 = `wt-gm3`**（`openmir2-worker-4`，分支 `gm-batch-3`，**HEAD `e39a08963`，工作区干净**）。第 2 轮记录的「工作区有未提交修改 `gmcmd.rs` + `lib.rs` + `whitelist.md`」在 R1 修订提交 `e39a08963` 时已落盘；第 3 轮复核 `git -C /e/tmp/wt-gm3 status --porcelain` ⇒ **0 行**。其它 worktree 实测（`git worktree list`，2026-10-22T15:05Z）：`wt-clickbag` 在已合并 commit `cddd3e0dd`（detached，干净，等清理）、`OpenMir2-wt-goods` 在 `lane/server-goods` @ `4eb774463`（干净，未合 master；`git diff --stat master...HEAD` **只有 `src/GameSrv/Npc/Merchant.cs`（C#）** ⇒ 不是 Rust 代码车道；master 有同题 `fb2824b6b` 但 oid 不同）、`m2svr-window-origin` 是窗口镜像（detached @ `229d52cd9`，非车道）、`wt-inventory` 为本审计线（只改文档）。**旧稿把 board 历史 `worktree` 字段当在跑清单，已更正。**

> **共同阻塞**：下 5 条全部要改 `bins/game-svr/src/lib.rs`（RM→SM 泵 / 入站分支 / `handle_shop_cmd`），而 `lib.rs` 正被 `wt-gm3` 占用；因此 **top5 只能串行排在 `wt-gm3` 合并之后**，没有一条能与它并行。`whitelist.md` 与 `smframe_ext.rs` 亦为共享热点。

| # | 单元（卡 id / 建议新卡） | 写集（已逐文件核实） | tier | 判据 | 与在跑车道冲突 |
|---:|---|---|---|---|---|
| 1 | `openmir2-orphan-encoder-batch-a`（**只含 13 个有编码器帧**；剔除 `SM_DONATE_FAIL`） | `smframe_ext.rs`（12 编码器）+ `smframe.rs`（`sm_skeleton`）+ `lib.rs`（RM→SM 发射泵）+ 生产者 `pickwire.rs`/`useitem.rs`/`sessionwire.rs`/`mapdoor.rs`/`ground.rs`；`whitelist.md`（若登记 T-*） | L（帧接线）/ R（`SM_BREAKWEAPON`、`SM_LAMPCHANGEDURA` 改耐久） | ① 每条：oracle 触发点逐行对照 + 帧字节（GBK）；② 抽样冒烟：至少 1 条端到端（建议 `SM_RECONNECT`）；③ 牙：撤发射点 ⇒ 该帧门 RED；④ 耐久两帧改走 R 窗口 | **lib.rs + whitelist.md 与 wt-gm3 冲突**。另：`SM_CHANGEFACE`/`SM_SKELETON` 的生产者本身未实现（上游功能缺口），应先拆子片或明确「只做编码器+构造用例」 |
| 2 | `openmir2-unbindlist-consumer` | `useitem.rs`（StdMode 31）、`bag.rs`、`lib.rs`（调用点）；`crates/data/src/lists.rs` **只读** | **R** | ① StdMode 31 + `AniCount==0` ⇒ 解包 6 件，背包逐槽字段 diff；② 容量不足分支与 oracle 同（不吃）；③ 真机窗口；④ 牙：把解包改回 no-op ⇒ T-17 用例红 | **lib.rs 与 wt-gm3 冲突**；`useitem.rs`/`bag.rs` 无其它车道 |
| 3 | `openmir2-minimap-consumer-wire` | `lib.rs:2259`（`CM_WANTMINIMAP` 分支）+ `contentload.rs`（loader 已在）+ `inbound.rs`（`client_get_min_map` 已有） | **L** | ① 构造级：`Envir.MinMap>0` ⇒ `SM_READMINIMAP_OK(710)` 且 `param=MinMap`；`=0` ⇒ `SM_READMINIMAP_FAIL`；② 抽样冒烟：进图后发 `CM_WANTMINIMAP(1033)` 收到 OK；③ 牙：把传参改回 0 ⇒ FAIL 帧用例红 | **lib.rs 与 wt-gm3 冲突**；`contentload.rs`/`inbound.rs` 无其它车道 |
| 4 | `openmir2-merchant-script-load`（A-08 / T-47③；**原稿漏列**） | `shop.rs`（`~@repair`/say 链路）+ `lib.rs`（`handle_shop_cmd`）+ `merchant.rs`/`npcs.rs`（`MerchantScriptBook` 已存在）+ `main.rs`（`Market_Def` 装载序） | L（说词/标签）/ R（修理/买卖写金币与背包） | ① 商人 `Market_Def` 装载数与 oracle 同；② `~@repair` 命中 ⇒ `SM_MERCHANTSAY(643)` 出队序；③ 牙：删装载 ⇒ say 用例红；④ 买卖/修理落库走 R 窗口 | **lib.rs 与 wt-gm3 冲突**；`shop.rs`/`merchant.rs` 无其它车道 |
| 5 | `openmir2-adjust-bonus-wire` | `lib.rs:2275`（`CM_ADJUST_BONUS` 现传 `bonus_point=0`）+ `inbound.rs` + `record.rs`/`ability.rs`；`crates/storage` 的 `PlayerData.bonus_point`（`playerdata.rs:641`）**已存在** | **R** | ① 逐字段：`BonusPoint=n` 后 `SM_ABILITY(52)`/`SM_SUBABILITY(752)` 两帧；② `bonus_abil_changed=false`（oracle 恒 `+=0`）；③ 真机字段级 diff（窗口）；④ 牙：`nPoint+0==BonusPoint` 取反 ⇒ 用例红 | **lib.rs 与 wt-gm3 冲突**。**接口冻结线**：`crates/storage`/`crates/protocol` 若需改字段或编码，必须先按 §12.4 通知受影响线；当前 `bonus_point` 已存在，预计只读，不改 schema |

> 明确的**不派**：`openmir2-gm-batch-4-numeric` 与 `wt-gm3` 的 `gmcmd.rs` 直接冲突，且批次 3 尚未合并；等 `wt-gm3` 收口后再排。

---

## 7. 最不确定的 2 个数字（建议复核者重点核）

> 原第 1 项「GM G2–G6 批次分解」已按复核意见与 §2.2 整节一起删除；GM 侧精确口径只看 §2.2 的 12 族表（185/35/150）。

1. **卡 backlog 的 A/B/D/C/O 分类**（§3）。分类是**人工逐卡指派**（集合在 `/e/tmp/audit-inv-20261022/finalcls.py`），
   依据是 `work` 自由文本，不是结构化字段；边界卡至少 4 张：`openmir2-script-take-persist`（窗口 vs 夹具）、
   `openmir2-openybdeal-host-wiring`（真机 + 残余代码）、`openmir2-gm-commands-design`（在飞 vs 已合）、
   `openmir2-inbound-dispatch-red-batch`/`openmir2-envir-redlist-batch`（剩余接线 vs 可收口）。
   预计 **±5 张**（A↔B 或 A↔C 漂移）。
2. **`crates/` 的 201 个「不可达 pub fn」**（§4.2）。名字级调用图是**过近似**，
   trait object / 闭包 / 同名方法会把已接线函数判成不可达（已证例：`chat::cry_*` 是故意零帧、
   `place_npcs` 走 `*_with_scripts` 变体）。`bins/` 的 58 个更可信；`crates/` 的 201 个只能当 triage 候选，
   直接当缺口会**高估**。

> 附：§1.1 的「274 个派发目标」是把「注册表 handler」与「engine switch 分支」相加得到；
> 两者 key 空间不同（handler 名 vs CmdCode case），但**不保证一个 code 只落一边**——若要「按 code 去重」的
> 口径，需要先跑一遍 `codes.rs` 的枚举全集，当前数字未做该去重（偏保守的高估）。

---

## 8. 复现脚本与残留物

### 8.1 GM 族 × 实现状态 join（§2.2）

```sh
cd /e/tmp/wt-inventory/mir2-rs
# 1) 从设计件截出 §4.3 表（342..533 行）
sed -n '342,533p' tests/parity/m3/design-gm-commands-20261023.md > E:/tmp/table43.md
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

### 8.2 卡分类（§3）

```sh
walgit collab board --format json > E:/tmp/board-r2.json
python - <<'PY'
import json
d=json.load(open('E:/tmp/board-r2.json',encoding='utf-8'))
rows=[[c['id'],c.get('owner'),c.get('worktree'),c.get('branch'),c.get('work')]
      for col in d['columns'] for c in col['cards'] if col['name']=='进行中']
print(len(rows))  # 185
print({c['name']:len(c['cards']) for c in d['columns']})
PY
# 分类：人工逐卡指派，集合硬编码在 /e/tmp/audit-inv-20261022/finalcls.py
# 第 2 轮修正：rng-consumer-structure-parity 从 A 移到 C；click-path-bag-writeback 从 O 移到 A。
# 第 3 轮：脚本已按上述两处更新并实跑（cd /e/tmp/audit-inv-20261022 && python -I finalcls.py）
# 实跑输出：Counter({'a': 152, 'b': 22, 'd': 8, 'c': 2, 'o': 1})
# 即 A 152 / B 22 / D 8 / C 2 / O 1，与本文 §3.1 一致。
```

### 8.3 生产可达性（§4.2）

```sh
cd /e/tmp/wt-inventory/mir2-rs
python - <<'PY'
import sys
sys.path.insert(0,'tests/parity/coverage')
import recon_gates as rg
repo=rg.Path('..')
corpus=rg.RustCorpus(rg.rust_source_dirs(repo, exclude=rg.DISPATCH_EXCLUDE))
live,spans=rg.production_live_functions(corpus)
print('live',len(live),'spans',len(spans))
PY
# 输出：live 2638 spans 3873（HEAD 24e9a5fab 复跑，与第 1 轮相同）
```

### 8.4 与既有盘点对账（§5，第 2 轮新增；第 3 轮补 §5.4 交集复算）

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

### 8.5 残留物清单

| 路径 | 内容 | 处置 |
|---|---|---|
| `/e/tmp/audit-inv-20261022/` | 第 1 轮盘点中间物 + **第 3 轮已更新并重跑的分类脚本**：`board.json`、`cards.tsv`、`cardtables.md`、`pairs.tsv`、`parsestats.{json,txt}`、`recon-*.json/txt`、`join.awk`、`classify*.py`、**`finalcls.py` / `finalcls.txt`（§8.2 分类的唯一事实源，第 3 轮按 rng→C、clickbag→A 修正后重跑）**、`livefns*.py`、`prod/` | 保留供复核；可随时删除，不含仓库内产物 |
| `E:/tmp/board-r2.json`、`E:/tmp/recon-{content,dispatch}-r2.json`、`E:/tmp/livefns-r2.txt`、`E:/tmp/parsestats-r2.{json,txt}`、`E:/tmp/{sec5,sec6,tail,test-hd}.md`、`E:/tmp/inv-round1-backup.md` | 第 2 轮复核中间物（board 快照、两门重跑、livefns、parse-stats、本文件分段稿/备份） | 保留供复核；可随时删除 |
| `/e/tmp/wt-inventory-target/` | `mir2-script-tool` 的 release 构建产物 | 保留（构建缓存）；未提交 |
| 仓库内 | **仅** `docs/rewrite-remaining-inventory-20261022.md` 一个文件 | 本卡唯一写集 |

- 未启动任何服务端/客户端进程；`E:/MirServer` 只读访问（`parse-stats` 读 639 个脚本、`recon_gates` 读部署件）。
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
