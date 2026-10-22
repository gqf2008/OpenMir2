# OpenMir2 重写剩余量清单（可复现盘点）

- 采集时间：2026-10-22T14:34Z（本机 22:34 +08:00）
- 采集点：worktree `/e/tmp/wt-inventory`，HEAD **`0e7da10b8c6b83e6199fa0b10c609174da67f011`**（`git log --oneline -1` ⇒ `0e7da10b8 Merge branch 'action-line-var' …`）
- 分支：`inventory-20261022`（从 master `0e7da10b8` 起）
- 只读面：`src/**`、`sql/**`、`E:/MirServer/**`；未起进程、未占端口、未写 DB
- 口径：**每个数字后面给「命令 → 输出摘要」**；估算一律标 `估算`，不计入结论。

> **一句话结论**：脚本引擎在**部署语料**上已 100% 覆盖（106/106 个派发目标、0 未实现）；真正的剩余量集中在
> **① 185 张 in-progress 卡里 152 张是 R 级真机窗口欠账**、**② GM 命令 185 条里 150 条命令体未实现**、
> **③ 协议出站 14 帧有编码器但无生产发射点**、**④ 2 个内容件的消费点/装载缺口（UnbindList、AdminList）**。

---

## 0. 汇总数字（全部可复现）

| 维度 | 总量 | 已完成 | 未完成/欠账 | 证据 |
|---|---:|---:|---:|---|
| 脚本派发目标（注册 + switch） | 274 | 148 | 126 | §1.1 |
| 脚本：部署语料实际用到的派发目标 | 106 | **106** | **0** | §1.2 |
| GM 命令（185 条表 + 内建 2） | 187 | 37（35 + 2 内建） | 150 | §2 |
| 协议派发（入站 + 出站） | 191 | 170 | 17 tracked + 3 n/a-unreachable | §4.1 |
| 内容加载调用点 | 40 | 11 implemented + 24 n/a | 5 tracked（其中 `rust=no` 2） | §4.1 |
| 协作卡（in-progress） | 185 | — | A 152 / B 22 / D 8 / C 1 / O 2 | §3 |

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
```

| 项 | 条数 | 证据 |
|---|---:|---|
| 命令表 `COMMANDS` | **185** | `grep -c "name:"` ⇒ 185；单测 `COMMANDS.len()==185` |
| 内建命令 | **2**（`@help`、`@commands`） | `BUILTIN_HELP`/`BUILTIN_COMMANDS`，表外处理 |
| 已实现命令体 | **35**（G1 5 + 批次 2/3 30） | `Imp::Unimplemented` = 150 ⇒ 185−150 = 35 |
| 未实现命令体 | **150** | 同上 |
| perm 0（现网可达） | **5** | `perm_min: 0` ⇒ `AddRobotPlay`/`LockLogin`/`拒绝`/`拒绝私聊`/`时间`；加内建 `@help` = 6 |

### 2.2 设计文档 6 个批次的落地情况

设计件 `tests/parity/m3/design-gm-commands-20261023.md` §6 定义 G1–G6。**当前 master 的实际落地顺序是
G1 + 一个跨族切片（批次 2/3）**，而不是按 G2→G6 顺序：

| 批次 | 设计内容 | 状态 | 条数 | 证据 |
|---|---|---|---:|---|
| **G1** | 派发骨架 + 现网可达 6 条（表内 5 + 内建 `@help`；`@commands` 走异常语义） | **已实现** | **5 + 2 内建** | `Imp::{Date,PrvMsg,AllowMsg,LockLogin,AddRobotPlay}`；`GmSession::permission` 恒 0 |
| （批次 2/3） | 重载/管理 14 + 封禁名单 9 + 桩/测试 7（`@AddRobotPlay` 已在 G1） | **已实现（乱序切片）** | **30**（三族共 31，含 G1 的 `@AddRobotPlay`） | `Imp::Load*/Reload*/Deny*/Test*` = 14+9+7 |
| G2 | 会话开关族 | 未开工 | 设计显式列 12 条，**跨 4 个族**（见下注） | 全部仍 `Unimplemented` |
| G3 | 传送/召唤族 | 未开工 | 设计显式列 8 条；所属族共 **17** 条 | 族表「传送/召唤 17 / 已实现 0」 |
| G4 | 数值/落库族 | 未开工 | **44**（=「数值/角色」族） | 族表 |
| G5 | 行会/城堡/赛事 | 未开工 | **16** | 族表 |
| G6 | 桩/死键/红字 | **桩/测试族已全部实现**；`@@!` 红字/`@commands` 语义待定 | 族内剩 **0** | 族表「桩/测试 8 / 已实现 8」 |
| **合计** | | | 5 + 30 + 150 = 185（+2 内建） | |

> **批次计数的不确定性（见 §6-①）**：设计 §6 只给 G2/G3 的**示例命令名**（各 12/8 条），
> 未给 G2–G6 的**互斥全集**。G2 的 12 条跨「其它(7)/传送(1)/数值(2)/查询(2)」，与 G3/G4 的族计数重叠，
> 因此 **G2–G6 无法在文档现有信息下做互斥分解**；能精确复算的只有
> `G1=5`、`已实现批次2/3=30`、`剩余=150`。

### 2.3 按 §4.3 的 12 族分解（精确、互斥）

```sh
# 解析 §4.3 表（185 行）× gmcmd.rs 的 (name, imp) 做 join；族取表格第 3 列。
# 复算脚本见 §7「复现脚本」join.awk。
# 输出摘要（每族 total/done/remain）：
```

| 族 | 总数 | 已实现 | 剩余 |
|---|---:|---:|---:|
| 数值/角色 | 44 | 0 | **44** |
| 其它 | 35 | 4 | **31** |
| 传送/召唤 | 17 | 0 | **17** |
| 行会/城堡/赛事 | 16 | 0 | **16** |
| 查询/展示 | 16 | 0 | **16** |
| 重载/管理 | 14 | **14** | 0 |
| 造物/刷怪 | 9 | 0 | **9** |
| 封禁名单 | 9 | **9** | 0 |
| 桩/测试 | 8 | **8** | 0 |
| 禁言/监听 | 7 | 0 | **7** |
| 仓库/登录密码 | 6 | 0 | **6** |
| 会话控制 | 4 | 0 | **4** |
| **合计** | **185** | **35** | **150** |

已实现的 35 条按 `imp` 标签：重载/管理 14、封禁名单 9、桩/测试 7、G1 的 5
（`时间/拒绝/拒绝私聊/LockLogin/AddRobotPlay`）。「其它」族已实现的 4 条正是 G1 除 `AddRobotPlay` 外的四条。

---

## 3. 卡 backlog（`walgit collab board --format json`）

### 3.1 总览

```sh
walgit collab board --format json > /tmp/board.json
python -c "import json;d=json.load(open('/tmp/board.json'));print({c['name']:len(c['cards']) for c in d['columns']})"
# {'待认领': 0, '进行中': 185, '待审': 0, '需要人': 0, '受阻': 0, '已完成': 153}
```

**in-progress = 185 张**。分类规则（本文档口径，脚本见 §7）：

- **A 真机窗口欠账（R 级缺证据）**：已合并且最新 status 明示窗口/真机/`tier=R` 欠账 ⇒ **152**
- **B 代码未开工/接线未做**：最新 status 明示 `实现未开`/`第②片`/`接线`/`另开卡` ⇒ **22**
- **D needs-human**：最新 status 明示 owner 裁决/排期/删除授权 ⇒ **8**
- **C 已合但卡未关（纯书务）**：明示「无待窗口项」且无其它剩余 ⇒ **1**
- **O 在飞/待审**（不属四类，单列）⇒ **2**

> 说明：本仓库 R 级策略要求「R 卡 merge 后也不 closed」，因此 A 与 C 的边界是「是否还有窗口/真机证据欠账」；
> 卡片的「阻塞原因」取自 board JSON 的 `work` 字段（= 该卡最新一条 `status` 的 `work`），未再逐卡拉线程正文；
> 若某卡最后一条是 `comment` 而非 `status`，此处仍显示最近一次 `status` 的摘要。

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
| 96 | `openmir2-rng-consumer-structure-parity` | openmir2-orchestrator@main/master | **三项余量已全部收口**（① 方案 C ② :342 登记 ③ 同表机判） |
| 97 | `openmir2-enter-shift-loop` | openmir2-orchestrator@main/master | 余量：① 真机窗口（墙格出生登录：RNG 流水 + 落点 |
| 98 | `openmir2-map-random-move-shift-loop` | openmir2-orchestrator@main/master | 余量：① 真机窗口（商人到点移动：RNG 流水 + 落点） |
| 99 | `openmir2-recall-slave-shift-live` | openmir2-orchestrator@main/master | 余量：① 真机窗口（W0 §2e：两侧 RNG 账本 `VERDICT IDENTICAL` + 回召落点同坐标） |
| 100 | `openmir2-mapcell-attributes-loading` | openmir2-orchestrator@main/master | 余量：① 真机窗口（**必核**：route added=1649、刷怪位置与 RNG 流水、进图/落格/AOI/CanWalk(true)/snap） |
| 101 | `openmir2-mapcell-consumer-teeth` | openmir2-orchestrator@main/master | 余量：① 真机窗口核「走位/推/冲刺帧面 + NPC/商人出现帧」（W0 runbook §W0-6 已列 |
| 102 | `openmir2-mongen-terrain-loop-live` | openmir2-orchestrator@main/master | 余量：真机窗口核「两侧出生坐标逐点一致 + RNG 流水」（W0 runbook §W0-6/§W0-8 已列，依赖 mapcell 装载 |
| 103 | `openmir2-ctor-barrier-a-draw` | openmir2-orchestrator@main/master | 余量：① 真机窗口（**默认流水已变，须重取基线** |
| 104 | `openmir2-ctor-draws-fold` | openmir2-orchestrator@main/master | 余量：① 屏障 A 的 1 抽（ServerConf.cs:114 ItemNumber += Random(10000)）——补上后段②/段③ 与 oracle 等距，须另立片 |
| 105 | `openmir2-snap-cell-fidelity` | openmir2-orchestrator@main/master | 余量：① 真机窗口项 |
| 106 | `openmir2-default-arm-spell-surface` | openmir2-orchestrator@main/master | 余量：① 真机逐帧对拍（窗口项） |
| 107 | `openmir2-buff-producer-wiring` | openmir2-orchestrator@main/master | 余量=真机窗口项 + 新开片：四块 snap 改用忠实镜像 `cetin_near_xy`（复验新发现的格子层差异）+ 补窗口宽度上界牙 |
| 108 | `openmir2-buff-system` | openmir2-coordinator@main/master | 余量=真机窗口项 + A-5 生产接线批 |
| 109 | `openmir2-world-tick-expansion` | openmir2-coordinator@main/master | 余量=真机窗口项（RNG 流水/时钟/公告节拍） |
| 110 | `openmir2-transmap-maptrigger` | openmir2-coordinator@main/master | 余量=真机窗口项 |
| 111 | `openmir2-maptrigger-monster-and-door` | openmir2-coordinator@main/master | 余量=真机窗口项 W-R1-a/b/c |
| 112 | `openmir2-orphan-encoder-wiring` | openmir2-coordinator@main/master | 余量=真机窗口项 |
| 113 | `openmir2-window-collector-preflight` | openmir2-coordinator@main/master | 余量=真机窗口项：两侧种子交叉断言 / C# site 列 / 前置门生产路径不误拦 |
| 114 | `openmir2-outbound-encoders-red-batch` | openmir2-coordinator@main/master | 按 tier=R **不 closed**——待窗口项：制药抽数流水 + 帧序 + MYSTATUS 派生落库 |
| 115 | `openmir2-m3-monster-active-seek` | openmir2-worker-22@land/worktree-redlist-monai9 | 余量：真机 W0/W9 对拍 + 登记未建模面（覆写 Run 的子类、SuperGuard/ArcherGuard 自有 pvp 扫描） |
| 116 | `openmir2-m3-drop-exp-wiring` | openmir2-worker-22@land/worktree-redlist-x2 | 余量：真机 100 杀两相窗口（依赖 T-59 rng-scene 已落）+ quest 真机窗口 |
| 117 | `openmir2-m3-rng-scene-driver` | openmir2-worker-22@land/worktree-redlist-x1 | 余量：真机两侧账本对拍（W9 窗）+ BotSrv real wiring（进 0137/pin 怪/dump，属 W9 窗项） |
| 118 | `openmir2-npcs-fixed-loading` | openmir2-worker-22@land/worktree-redlist-t1-npcs | 剩余 DoD = 真机两侧 NPC 出现帧一致（并入联调窗，与 ③a/GuardList 同窗） |
| 119 | `openmir2-m3-quest-align` | openmir2-svc-1@window/worktree-window | 窗口 r3（master d4d5b89c 未动）：窗口 r3 复核⑥（master 未动）：仍 BLOCKED，三重阻断与 r2 相同：①markers 前缀口径（卡 openmir2-m3-fixture-marker-prefix-bug 未认领）②C# k=0 frame 事件 JSON 截… |
| 120 | `openmir2-guardlist-guards-loading` | openmir2-worker-22@guardlist/worktree-guardlist-loading | 剩余 DoD = 真机两侧项：③a 反向点出现帧一致（C# 侧现 1 对、修复后应两侧同）+ MIR2_RNG_SEED 双侧 site/order 对拍——并入下次联调窗口（与 ③b/③a 复跑同窗） |
| 121 | `openmir2-m3-drop-exp-fixture-botsrv` | openmir2-worker-11@m2-svr/worktree-m3-drop-exp-fixture-botsrv | **保持 in-progress（离线夹具片，不 closed）**：本 approve 只覆盖离线夹具/接线/登记片，卡级真机 DoD③⑤ = 真机两侧 100 杀同序列 diff=0 + drop_exp_purity_gate 实测（阻塞 = owner -Go 独占窗口 + shared 承… |
| 122 | `openmir2-m3-hero-pet-align` | openmir2-svc-1@window/worktree-window | 窗口 r3（master d4d5b89c 未动）：窗口 r3 复核⑨（master 未动）：PREREQ（静态） |
| 123 | `openmir2-m3-fixture-chain-gate` | openmir2-worker-5@m3cs/worktree-m3-chain-gate-o1o2-closeout | review do_not_close=true）**：剩余 DoD = 卡级真机两侧同序列 diff=0（各 READY 夹具 run_*.ps1 -SelfCheckNoStack + 共享管线自检需世界侧实现 + openmir2-m3-clock-injection + owner 独占窗口… |
| 124 | `openmir2-m3-rng-trace-scene-groups` | openmir2-worker-4@m3bag/wip/m3bag-anchor-teeth | **保持 in-progress（不 closed，review do_not_close 明示）**：本 approve 只覆盖离线证据/门片，卡级 R 真机 DoD（同场景含怪两相 trace + groupdiff --fold --strict 的 site/order diff=0）仍 n… |
| 125 | `openmir2-m3-pet-slave-masterdie-wire` | openmir2-worker-20@m3hp/worktree-m3-selloff-dealoffform-recog | **按 tier=R 不 closed（保持 in-progress）**：剩余 DoD = ① 真机两侧 run_hero_pet_pair.ps1 主人死 step 同序列 diff=0（含叛变站点帧序/字节 + RNG 抽数 site/order）② 混合队列场景未建模 not_verifie… |
| 126 | `openmir2-m3-pet-slave-amulet-delitem-frame` | openmir2-worker-20@m3hp/worktree-m3-pet-slave-amulet-delitem | **按卡级剩余 DoD 保持 in-progress（不 closed）**：剩余 = ① 真机 run_hero_pet_pair.ps1 护身符扣光 step 两侧同序列 diff=0（owner 独占窗口，review not_verified）② unimplemented 登记：SendD… |
| 127 | `openmir2-m3-party-trade-align` | openmir2-svc-1@window/worktree-window | 窗口 r3（master d4d5b89c 未动）：窗口 r3 复核⑦（master 未动）：PREREQ（静态） |
| 128 | `openmir2-m3-pet-slave-clone46` | openmir2-worker-20@m3hp/worktree-m3-pet-slave-clone46 | **按 tier=R 不 closed（保持 in-progress）**：剩余 DoD = 真机两侧 run_hero_pet_pair.ps1 make_clone step 同序列 diff=0（待真机判据（窗口）），按 review not_verified 保留 |
| 129 | `openmir2-m3-shop-fixture-botsrv` | openmir2-worker-5@m3cs/worktree-m3-shop-frame-json-parse | **保持 in-progress（不 closed）**：本片虽为 L 级（window_required=false），但卡级真机 DoD②③（真机两侧同序列 run_npc_shop_pair.ps1 + npc_shop_diff.py 字段级 diff=0）仍待世界侧（⑤-2/⑤-4/⑤-5… |
| 130 | `openmir2-m3-selloff-fixture-botsrv` | openmir2-worker-5@m3cs/worktree-m3-selloff-fixture-botsrv | 剩余 DoD②③ = 真机两侧同序列 diff=0（等 ⑩-1/⑩-2 落地 + 独占窗口） |
| 131 | `openmir2-m3-fixture-marker-prefix-bug` | openmir2-worker-5@m3cs/worktree-m3-marker-guard-registration | **保持 in-progress（夹具/离线片，不 closed）**：剩余 DoD = 卡级 ① 真机两侧同序列 diff=0（阻塞 = 承接卡/driver + owner 独占窗口，按 review not_verified 登记） |
| 132 | `openmir2-m3-drop-exp-align` | openmir2-svc-1@window/worktree-window | 窗口 r3（master d4d5b89c 未动）：窗口 r3 复核④（master 未动）：PREREQ（静态） |
| 133 | `openmir2-m3-pet-slave-lifecycle` | openmir2-worker-20@m3hp/worktree-m3-pet-slave-abilitys | **按 tier=R 不 closed（保持 in-progress）**：剩余 DoD = 真机两侧 run_hero_pet_pair.ps1 同序列 diff=0（召唤/诱惑/叛变/幽灵各站点，归 openmir2-m3-hero-pet-align + owner 独占窗口），按 revie… |
| 134 | `openmir2-m3-hero-pet-fixture-botsrv` | openmir2-worker-5@m3cs/worktree-m3-hero-pet-fixture-minors | **不 closed（保持 in-progress）**：剩余 DoD②③ = 真机两侧 run_hero_pet_pair.ps1 同序列 diff=0（需 ⑨-2/⑨-3 世界侧 + openmir2-m3-clock-injection + owner 独占窗口），按 review not_v… |
| 135 | `openmir2-m3-guild-castle-fixture-botsrv` | openmir2-worker-5@m3cs/worktree-m3-guild-castle-anchor | **夹具卡 ⇒ 保持 in-progress（不 closed）**：剩余 DoD②③ = 真机两侧 run_guild_castle_pair.ps1 同序列 diff=0（需 ⑧-3..⑧-9 实现卡 + openmir2-m3-clock-injection + owner 独占窗口），按 r… |
| 136 | `openmir2-m3-selloff-goldsales-load-wire` | openmir2-worker-20@m3hp/worktree-m3-selloff-goldsales-load-wire | **不 closed（DoD 未全达，保守）**：剩余 = 真机两侧同序列对拍 run_selloff_pair.ps1 + selloff_diff.py + goldsales 快照 diff=0（等独占窗口，归 openmir2-m3-mail-auction-align） |
| 137 | `openmir2-m3-shop-dealing-gate` | openmir2-worker-11@m2-svr/worktree-m3-shop-dealing-gate | **不 closed（review OA-1 明示：本 approve 只覆盖 Dealing 前置门一片，卡级 DoD 真机两相未闭环）**：剩余 DoD = 真机端到端（交易中发 CM_USERBUYITEM/CM_USERGETDETAILITEM 观察零帧 + characters/gold… |
| 138 | `openmir2-m3-shop-dialog-wiring` | openmir2-worker-11@m2-svr/shop-wire-host | **按 tier=R 不 closed（保持 in-progress）**：剩余 DoD = 真机两侧 run_npc_shop_pair.ps1 同序列 diff=0（详情 652 / 仓库 700-707 帧序与字节 + characters_storageitem 槽位 diff=0）与 DB… |
| 139 | `openmir2-m3-quest-fixture-botsrv` | openmir2-worker-5@m3cs/worktree-m3-quest-fixture-botsrv | 剩余 DoD②③ = 真机两侧同序列 diff=0（-Side rust preflight 仍 PREREQ，等 ⑥-3 落地 + 独占窗口） |
| 140 | `openmir2-m3-quest-persist-observability` | openmir2-worker-21@m3pt/worktree-m3-quest-persist-observability | run_pair.ps1 + scoped_diff.py --canonical）需独占窗口，按 not_verified 登记 |
| 141 | `openmir2-m3-mail-auction-align` | openmir2-svc-1@window/worktree-window | 窗口 r3（master d4d5b89c 未动）：窗口 r3 复核⑩（master 未动）：PREREQ（静态） |
| 142 | `openmir2-m3-party-deal-capacity-int` | openmir2-worker-21@m3pt/worktree-m3-party-deal-capacity-int | **按 tier=L 但卡级真机 DoD 未达 ⇒ 不 closed（保持 in-progress，review note 明示「不因本片收卡」）**：剩余 DoD = 真机两侧同序列 diff=0（归 openmir2-m3-party-trade-align + 独占窗口，not_verifie… |
| 143 | `openmir2-m3-clock-injection` | openmir2-worker-11@m2-svr/m3-clock-inject-host | **按 tier=R 不 closed（保持 in-progress）**：剩余 DoD = ① C# 影子 hook 半（HUtil32.GetTickCount/DateTime.Now 不可替换字段，待 owner 在 Mono.Cecil IL patch / 影子源码重建 / 不改 ora… |
| 144 | `openmir2-m3-pet-slave-summon` | openmir2-worker-20@m3hp/worktree-m3-pet-slave-m3-rc-eol | 合并检查点（role=merge, openmir2-coordinator）：⑨-2 M-3 收口片（本片 = 仓库卫生离线片：mir2-rs/.gitattributes 追加 `*.rc text eol=lf`，零 .rs，非窗口）已合入 master bdc508ef1（approve o… |
| 145 | `openmir2-m3-party-gainexp-npe` | openmir2-worker-21@m3pt/worktree-m3-party-gainexp-npe | **按 tier=R 不 closed**：剩余 DoD = 真机两侧同序列 diff=0（归 openmir2-m3-party-trade-align + 独占窗口），按 not_verified 登记 |
| 146 | `openmir2-m3-shop-scenario-authority` | openmir2-worker-5@m3cs/worktree-m3-shop-scenario-authority | **剩余 DoD = 真机**：run_npc_shop_pair.ps1 在独占窗口跑两侧同序列 diff=0（等 ⑤-4 第②片仓库存档面 + ⑤-5 / NPC 入视野），离线判据本体已完整 |
| 147 | `openmir2-m3-fixture-matrix` | openmir2-worker-5@m3cs/worktree-m3-fixture-matrix | **剩余 = 真机 DoD**：READY 夹具需 -Side rust preflight 前置落地 + 独占窗口跑两侧同序列 diff=0 |
| 148 | `openmir2-m3-guild-castle-align` | openmir2-svc-1@window/worktree-window | 窗口 r3（master d4d5b89c 未动）：窗口 r3 复核⑧（master 未动）：PREREQ（静态） |
| 149 | `openmir2-m3-npc-shop-align` | openmir2-svc-1@window/worktree-window | 窗口 r3（master d4d5b89c 未动）：窗口 r3 复核⑤（master d4d5b89c 未动，与 r1/r2 相同）：仍 BLOCKED，与 r2 逐条相同 |
| 150 | `openmir2-m3-scoped-diff-ability` | openmir2-svc-1@window/worktree-window | 窗口 r3（master d4d5b89c 未动）真机加深复跑 ②③F-2（360 击）：attack_diff 仍 GREEN diff=0（360/157/203/6576） |
| 151 | `openmir2-m3-party-trade-fixture-botsrv` | openmir2-worker-5@m3cs/worktree-m3-party-trade-fixture-botsrv | 剩余 DoD②③ = 真机两侧同序列 diff=0（等 ⑦-2/⑦-3 落地 + 独占窗口） |
| 152 | `openmir2-m3-final-acceptance` | openmir2-worker-5@m3cs/worktree-m3-final-acceptance | **剩余**：DoD② 300 假人 24h 长跑（**等独占窗口** |

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

### C. 已合但卡未关（书务）

| card id | owner/worktree | 阻塞原因（来自最新 status 的 work） |
|---|---|---|
| `openmir2-gate-gmclose-race` | openmir2-orchestrator@main/master | 无待窗口项 |

### O. 在飞/待审（不计入四类）

| card id | owner/worktree | 阻塞原因（来自最新 status 的 work） |
|---|---|---|
| `openmir2-gm-commands-design` | openmir2-worker-4@wt-gm3/gm-batch-3 | 批次 3/3 传送/召唤族 17 条 + @传送：命令体逐行照 oracle，移动/RNG 未接项登记 T-166..T-171 |
| `openmir2-click-path-bag-writeback` | openmir2-worker-5@wt-clickbag/click-bag-writeback | 固定 NPC 点击 @main 复用 run_merchant_goto：背包/佩戴位/金币写回（T-160） |
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
| `UnbindList.txt` | **no** | loader 在 `crates/data/src/lists.rs`，**game-svr 消费点未接**（StdMode 31 解包，白名单 T-17） |
| `AdminList.txt` | **no** | loader 在 `crates/data/src/lists.rs`，**无生产消费者**；oracle 的 `GetHumPermission` 被注释 ⇒ 与「permission 恒 0」裁决一致（D-1） |
| `MiniMap.txt` | yes | 已加载；但 `CM_WANTMINIMAP` 消费点仍传 `0`（见 §5-①） |
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
  「BonusPoint 未挂会话态」「门表恒 false / 结构性 no-op」「MinMap 传 0」的接线缺口（见 §5）。
- **出站 tracked 14**（编码器已有、生产发射点 = 0，`grep SM_X bins/game-svr/src` 只命中 `smframe_ext.rs` 定义）：

```
SM_BREAKWEAPON, SM_CHANGEFACE, SM_CLOSEDOOR, SM_CLOSEHEALTH, SM_HIDEEVENT,
SM_ITEMSHOW, SM_LAMPCHANGEDURA, SM_OPENDOOR_OK, SM_QUERYYBDEAL, SM_QUERYYBSELL,
SM_RECONNECT, SM_SHOWEVENT, SM_SKELETON, SM_DONATE_FAIL
```

> 卡 `openmir2-orphan-encoder-wiring` 的最新 status 写「片103 落地；余量=真机窗口项」，
> 但**当前 HEAD 的派发门仍判 14 帧 `rust=no`**——以门为准，这 14 帧没有生产发射点。

### 4.2 名字级可达性扫描（`pub fn` 调用点是否落在生产路径）

用同一个 `production_live_functions` 对 `bins/game-svr/src` 的全部 `pub fn` 做反查：

```sh
# 见 §7 livefns.py；结果：
live fns 2638 spans 3873
pub fns in bins defs 842 unique 724
unreachable pub fns defs 63 unique 58
```

58 个「从 `main` 不可达」的 pub fn 分三类：

| 类别 | 数量 | 模块 / 函数 | 判定 |
|---|---:|---|---|
| 测试支撑（预期） | 13 | `testsupport.rs`：`setup_db/open_db/insert_character/...` | 非生产，忽略 |
| 编码器（与 §4.1 出站缺口同源） | 25 | `smframe_ext.rs` 15、`smframe.rs` 9、`guild_frames.rs::sm_guild_break_ally_ok_770` | 确有缺口；以派发门 14 条 tracked 为准 |
| 其它需人工复核 | 20 | `config::default_path`、`chat::{cry_text,cry_hear_frame}`、`inbound::{client_make_drug_item,sm_butch}`、`npcs::{build_npc_spawns,place_npcs}`、`selloff::{get_sell_off_gold,select_sell_date}`、`ability::item_names`、`bag::from_slots`、`castle::parse_castle_date`、`gamegold::auto_tick`、`homemap::new_human_spawn`、`merchant::spawn_merchants`、`mongen::venture_server_default_is_false`、`npc::run_std_mode_func`、`record::encode_player_data`、`shop::is_merchant`、`trade::deal_cancel_a` | **名字级过近似**：闭包/trait 对象/同名方法会误报。已抽查 `chat::cry_*` 是**故意的**（oracle `CryCry` NRE，T-118，零帧），`place_npcs`/`spawn_merchants` 的生产入口走 `*_with_scripts` 变体 ⇒ 多数非真缺口 |

`crates/` 同口径：204 个 def / 201 个 unique 名字从 bins `main` 不可达；**不建议直接当缺口清单**——
`hutil32.rs`（27）、`castle.rs`（32）、`lists.rs`（18）、`castle_tax.rs`（13）、`combat.rs`（13）、
`magic.rs`（13）、`world.rs`（13）里混有纯工具函数、测试专用 API 与动态派发。需要按卡逐项 triage。

### 4.3 模块面结论

- **真正可执行的接线缺口只有两类**：协议出站 14 帧 + 内容消费 2 件（UnbindList、AdminList）。
- 入站 3 条 tracked 是「handler 已有、生产参数未挂」的**小接线**，不是重写缺口。
- 模块文件本身没有「文件存在但 `mod` 未声明」的孤儿：`find bins/game-svr/src -name '*.rs'` 与
  `lib.rs` 的 `mod` 声明逐一对账，仅 `castle/tests.rs`、`guild_official/tests.rs`、`guild_runtime/tests.rs`
  三个 `tests` 子模块未声明（测试文件，正常）。

---

## 5. 下一批 top5（可立刻派）

排序依据：**现网可见行为 / 风险 / 与在跑车道写集冲突**。
当前活跃 worktree（board `worktree` 字段）：`wt-gm3`（worker-4，`gmcmd.rs`）、`wt-takepersist`（worker-5）、
`wt-clickbag`（worker-5）、`m3cs`（worker-5，shop/castle）、`m3hp`（worker-20，selloff/pet）、
`m3pt`（worker-21，party/quest）、`window`（svc-1，无代码）、`m5tools`（worker-22）、`wbevy`（worker-14，已 park）。
**`lib.rs` 是几乎所有车道的共享热点**，以下单元都要求一个短 `lib.rs` 串行窗口。

| # | 单元（卡 id / 建议新卡） | 写集 | tier | 判据 | 预计阻塞点 |
|---:|---|---|---|---|---|
| 1 | `openmir2-minimap-consumer-wire` | `bins/game-svr/src/contentload.rs`、`inbound.rs`、`lib.rs`（1 个分支） | **L** | ① 构造级：`Envir.MinMap>0` ⇒ `SM_READMINIMAP_OK(710)` 且 `param=MinMap`；`=0` ⇒ `SM_READMINIMAP_FAIL`；② 抽样冒烟：进图后发 `CM_WANTMINIMAP(1033)` 收到 OK；③ 牙：把传参改回 0 ⇒ FAIL 帧用例红 | `lib.rs` 写集排队；`contentload.rs` 与 MiniMap loader 的 `MapRegistry` 只读接口需确认 |
| 2 | `openmir2-adjust-bonus-wire` | `bins/game-svr/src/lib.rs`、`ability.rs`、`inbound.rs` | **R** | ① 逐字段：`BonusPoint=n` 后 `SM_ABILITY(52)`/`SM_SUBABILITY(752)` 两帧；② `bonus_abil_changed=false`（oracle 恒 `+=0`）；③ 真机字段级 diff（窗口）；④ 牙：`nPoint+0==BonusPoint` 取反 ⇒ 用例红 | 会话态 `BonusPoint` 需新增字段并接存档；R 窗口；`lib.rs` 冲突 |
| 3 | `openmir2-unbindlist-consumer` | `bins/game-svr/src/useitem.rs`、`bag.rs`、`lib.rs`（调用点）；`crates/data/src/lists.rs` 只读 | **R** | ① StdMode 31 + `AniCount==0` ⇒ 解包 6 件，背包逐槽字段 diff；② 容量不足分支与 oracle 同（不吃）；③ 真机窗口；④ 牙：把解包改回 no-op ⇒ T-17 用例红 | R 窗口；`useitem.rs` 当前无活跃车道（好）；需 `UnbindList` 装载文本 |
| 4 | `openmir2-orphan-encoder-batch-a` | `bins/game-svr/src/smframe_ext.rs` + 各生产者：`pickwire.rs`/`useitem.rs`/`sessionwire.rs`/`mapdoor.rs`（按帧拆子补丁） | **L（帧接线）/ R（耐久类）** | ① 每条：oracle 触发点逐行对照 + 帧字节（GBK）；② 抽样冒烟：至少 1 条端到端（建议 `SM_RECONNECT`）；③ 牙：撤掉发射点 ⇒ 该帧门 RED；④ `SM_BREAKWEAPON`/`SM_LAMPCHANGEDURA` 改耐久 ⇒ 升 R 走窗口 | 部分帧的生产者本身未实现（`SM_CHANGEFACE`/`SM_SKELETON`/`SM_DONATE_FAIL`）；`SM_ITEMSHOW` 已被 `m3-drop-exp-wiring` 占用；`SM_QUERYYB*` 与 `m3hp` 车道冲突 |
| 5 | `openmir2-gm-batch-4-numeric`（G4 数值/角色 44 条） | **仅** `bins/game-svr/src/gmcmd.rs`（+ 数值写回复用 `ability.rs`/`bag.rs`/`dbclient.rs` 只读） | **R** | ① 44 条命令逐条：权限门 10 + 目标在线/离线两分支帧；② 字段级 diff（钱/经验/属性/负重）；③ 真机窗口（permission 需临时抬到 10，见 design W-3）；④ 牙：离线分支文本、`@加钱` 数值、`@GameGold` 帧 | **`gmcmd.rs` 与在跑 `wt-gm3`（gm-batch-3）直接冲突，必须等其合并后串行**；现网 permission=0 ⇒ 只能构造级/窗口验证 |

> 为什么不是「M3 第②片 bins 接线」：那 9 张卡（⑧-3/⑧-4/⑧-5/⑧-6/⑧-7/⑧-8/⑧-9、⑦-4、⑩-1）
> 都卡在 **owner 对 `bins/game-svr` 串行序的裁决**（D 类），不是可立即派的写集。

---

## 6. 最不确定的 3 个数字（建议复核者重点核）

1. **GM 的 G2–G6 批次剩余条数**（§2.2）。设计件 §6 只给示例命令名，不给互斥全集；
   G2 的 12 条跨 4 个族，与 G3/G4/G5 的族计数重叠。可精确复算的只有 `G1=5`、`批次2/3=30`、`剩余=150`；
   任何「G2=12 / G3=8 / …」的分解都至少有 **±10 条**的口径差。**族表（§2.3）才是精确口径。**
2. **卡 backlog 的 A/B/D/C/O 分类**（§3）。分类来自 board JSON 的 `work` 自由文本，不是结构化字段；
   边界卡至少 4 张：`openmir2-script-take-persist`（窗口 vs 夹具）、`openmir2-openybdeal-host-wiring`
   （真机 + 残余代码）、`openmir2-gm-commands-design`（在飞 vs 已合）、
   `openmir2-inbound-dispatch-red-batch`/`openmir2-envir-redlist-batch`（剩余接线 vs 可收口）。
   预计 **±5 张**（A↔B 或 A↔C 漂移）。
3. **`crates/` 的 201 个「不可达 pub fn」**（§4.2）。名字级调用图是**过近似**，
   trait object / 闭包 / 同名方法会把已接线函数判成不可达（已证例：`chat::cry_*` 是故意零帧、
   `place_npcs` 走 `*_with_scripts` 变体）。`bins/` 的 58 个更可信；`crates/` 的 201 个只能当 triage 候选，
   直接当缺口会**高估**。

> 附：§1.1 的「274 个派发目标」是把「注册表 handler」与「engine switch 分支」相加得到；
> 两者 key 空间不同（handler 名 vs CmdCode case），但**不保证一个 code 只落一边**——若要「按 code 去重」的
> 口径，需要先跑一遍 `codes.rs` 的枚举全集，当前数字未做该去重（偏保守的高估）。

---

## 7. 复现脚本与残留物

### 7.1 GM 族 × 实现状态 join（§2.3）

```sh
# 1) 从设计件截出 §4.3 表（342..533 行）
sed -n '342,533p' tests/parity/m3/design-gm-commands-20261023.md > /tmp/table43.md
# 2) join.awk（FS="|"；gmcmd.rs 取 name/imp，table 取第 3 列族 + 第 5 列现网名）
cat > /tmp/join.awk <<'AWK'
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
awk -f /tmp/join.awk bins/game-svr/src/gmcmd.rs /tmp/table43.md > /tmp/pairs.tsv
# 3) 族统计
awk -F'\t' '$1=="FAM"{fam[$2]=$3} $1=="IMP"{imp[$2]=$3}
END{for(n in fam){k=fam[n];t[k]++; if(imp[n]=="Unimplemented")r[k]++;else d[k]++}
for(k in t) printf "%-16s total=%3d done=%3d remain=%3d\n",k,t[k],d[k],r[k]}' /tmp/pairs.tsv | sort
# 校验：FAM 185 行、IMP 185 行、名字集合双向零差集（comm -3 为空）
```

### 7.2 卡分类（§3）

```sh
walgit collab board --format json > /tmp/board.json
python - <<'PY'
import json,csv
d=json.load(open('/tmp/board.json',encoding='utf-8'))
rows=[[c['id'],c.get('owner'),c.get('worktree'),c.get('branch'),c.get('work')]
      for col in d['columns'] for c in col['cards'] if col['name']=='进行中']
print(len(rows))  # 185
PY
# 分类集合（A/B/D/C/O）见本文件 §3.1 的规则；脚本 /e/tmp/audit-inv-20261022/{classify,finalcls}.py
```

### 7.3 生产可达性（§4.2）

```sh
cd mir2-rs
python - <<'PY'
import sys
sys.path.insert(0,'tests/parity/coverage')
import recon_gates as rg
repo=rg.Path('..')
corpus=rg.RustCorpus(rg.rust_source_dirs(repo, exclude=rg.DISPATCH_EXCLUDE))
live,spans=rg.production_live_functions(corpus)
print('live',len(live),'spans',len(spans))
PY
# 输出：live 2638 spans 3873
```

### 7.4 残留物清单

| 路径 | 内容 | 处置 |
|---|---|---|
| `/e/tmp/audit-inv-20261022/` | 盘点中间物：`board.json`、`cards.tsv`、`cardtables.md`、`pairs.tsv`、`parsestats.{json,txt}`、`recon-*.json/txt`、`join.awk`、`classify*.py`、`livefns*.py`、`prod/` | 保留供复核；可随时删除，不含仓库内产物 |
| `/e/tmp/wt-inventory-target/` | `mir2-script-tool` 的 release 构建产物 | 保留（构建缓存）；未提交 |
| 仓库内 | **仅** `docs/rewrite-remaining-inventory-20261022.md` 一个文件 | 本卡唯一写集 |

- 未启动任何服务端/客户端进程；`E:/MirServer` 只读访问（`parse-stats` 读 639 个脚本、`recon_gates` 读部署件）。
- 未修改 `src/**`、`sql/**`、`mir2-rs/**` 中任何代码；未写 DB。
