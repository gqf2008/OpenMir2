# whitelist 待复核项清单（T-161 .. T-193）— owner 一次签批表

> 卡片：`openmir2-whitelist-review-sheet`（tier=L，写集仅本文件）。
> 基线：master `e4dd64c77`；`mir2-rs/tests/parity/whitelist.md` 801 行。
> 门禁基线（本卡核对用）：anchor gate **397 条锚 ALL PASS**（本卡重跑 `whitelist_anchor_gate.py check` 得同值）、registry **entries=150 / duplicates=none / GREEN**（本卡重跑 `m3_whitelist_registry_audit.py audit` 得同值）。
> 本文件**只做盘点**：不改 `whitelist.md`、不改代码、不替 owner 下裁决。C 组只列选项与代价。

## 0. 范围与编号口径

- 抽取 `whitelist.md` 中 **T-161 .. T-193**（含）全部登记行，共 **32 条**。
- **T-185 空缺**：`grep 'T-185' whitelist.md` = 0 命中；`docs/orphan-encoder-trigger-audit-20261023.md` §编号口径 已载明「T-185 空缺」，非漏抽。
- 章节分布：GM 重载/管理·封禁·桩族（T-161..T-164）→ 变量表/传送召唤/夫妻/师徒/装备派生/配置/口径（T-165..T-171）→ SelGate 端口（T-172）→ AdminList 名单比较（T-173）→ 孤儿出站编码器（T-174..T-183）→ SearchViewRange 首扫基准（T-184）→ GM 第 4 批数值/角色族（T-186..T-191）→ 城堡商人 FixedHide（T-192）→ ya52 MESSAGEBOX（T-193）。
- 三档统计：**A = 6 条 / B = 23 条 / C = 3 条**。

图例：

- **登记类型**：`已实现待窗口`（实现已在、只差真机窗口或已接线）/ `不可达登记`（生产不可达、登记不接线）/ `已知偏差`（刻意保留的口径差）/ `缺陷跟踪`（已知缺口已拆卡跟踪）。
- **依据形态**：`有 oracle 文件:行`（至少一处 C# 源文件 + 行号）/ `只有结论`（只有路径/符号或纯论断，无行号）/ `只有卡号`（本次范围内 **0 条**）。
- **reviewer 复核痕迹**：`有` = 条目内已留具体复核编号/oid；`无` = 仅写「待独立 principal 复核」或「待 owner 复核」。

---

## A 组 — 可直接签批（依据完整、描述与代码一致）

> 判定标准：差异面在生产**不可达**或**两侧等价**，且有具体 oracle 文件:行 / 门禁证据，本次已抽样核对描述与代码一致。

| 编号 | 一句话主题 | 登记类型 | 依据形态 | reviewer 复核痕迹 | 签批时看什么（≤30 字） |
| --- | --- | --- | --- | --- | --- |
| T-170 | GM 批次 3 五个配置值硬编码进 `GmConfig::default()` | 已知偏差 | 有 oracle 文件:行（`Server.conf:718-719,828-830`） | 无（待独立 principal） | 五值与部署逐项相同、perm 0 不可达 |
| T-171 | 地图/在线目标/掌门/`HomeMap` 四处口径近似 | 已知偏差 | 有 oracle 行（`MapManager.cs:10`、`WorldServer.cs:947`） | 无 | ①ASCII 等价；②③④ perm 0 不可达 |
| T-172 | C# SelGate 监听端口改为可配置（oracle 源改动） | 已知偏差 | 有 oracle 行（`GateShare.cs:21`、`ConfigManager.cs:71`） | 无（owner 已口头批准，待本行留痕） | 只改端口来源、缺省仍 7100 |
| T-176 | `SM_OPENDOOR_OK`(612)/`SM_CLOSEDOOR`(614) 登记不接线 | 不可达登记 | 有 oracle 行（`PlayObject.Message.cs:1751/1754`） | 无 | 认 T-65 台账：门链结构性不可达 |
| T-177 | `SM_CLOSEHEALTH`(1101) 已按 `BreakOpenHealth` 接线 | 已实现待窗口（L） | 有 oracle 行（`BaseObject.Base.cs:219-222`、`BaseObject.cs:933-942`、`Message.cs:2013`） | 无 | 核门/状态位/±12 广播/12B 帧 |
| T-179 | `SM_ITEMSHOW`(610) 登记不接线 | 不可达登记 | 有 oracle 行（`BaseObject.cs:570/752`、`Envirnoment.cs:90`） | 无 | 认 `MapItem.ItemId` 恒 0 ⇒ 恒 return |

---

## B 组 — 需看证据（有 oracle 锚但需人判等价性 / 依据仅为结论）

> 判定标准：差异不是「今天可观测的行为偏差」而是「未接线的能力」或「口径近似」，但要么依据只有结论（无行号），要么是 R 级待窗口项，owner 签批前需认一条等结论。

| 编号 | 一句话主题 | 登记类型 | 依据形态 | reviewer 复核痕迹 | 签批时看什么（≤30 字） |
| --- | --- | --- | --- | --- | --- |
| T-161 | 重载/管理族 14 条只回 oracle 文本、宿主动作未接 | 不可达登记（perm10） | 只有结论 | 无 | 认 8 条本 no-op + perm 门 |
| T-162 | 封禁名单族 9 条不建三张全局表 | 不可达登记（oracle no-op） | 只有结论 | 无 | 认 oracle 增删读写全注释 |
| T-163 | 桩/测试族 7 条宿主副作用未接线 | 不可达登记（perm10） | 只有结论 | 无 | 认零帧 + 越界怪癖照抄 |
| T-164 | 参数解析两处口径近似（不可观测） | 已知偏差 | 只有结论 | 无 | 认空白集差异不可达 |
| T-165 | 动作参数 `$<整数>` 的变量表未实现 | 已知偏差 | 只有结论 | 无 | 认部署 `<$n>` 计数为 0 |
| T-166 | 传送/召唤族移动与 RNG 抽数未接线 | 不可达登记（R） | 只有结论 | 无 | 认 perm10；接时复用既有宿主 |
| T-167 | 夫妻传送/配偶在线态未接线 | 不可达登记（R） | 只有结论 | 无 | 认 `dear_human` 生产写点 = 0 |
| T-168 | 师徒在线列表未接线 | 不可达登记（R） | 只有结论 | 无 | 认 `MasterList` 无宿主 |
| T-169 | `Teleport` 装备派生未接线 | 不可达登记（R） | 有 oracle 行（`PlayObject.cs:2235-2237`） | 无 | 认 Rust 全仓该常量 0 命中 |
| T-174 | `SM_BREAKWEAPON`(1102) 登记不接线 | 已实现待窗口（R） | 有 oracle 行（`PlayObject.cs:4121`、`Message.cs:2016`） | 无 | 认耐久链未接 ⇒ 两侧零帧 |
| T-178 | `SM_HIDEEVENT`(805)/`SM_SHOWEVENT`(804) 登记不接线 | 不可达登记 | 有 oracle 行（`Message.cs:1993/1999`） | 无 | 认 `VisibleEvents` 差集未建模 |
| T-180 | `SM_LAMPCHANGEDURA`(655) 登记不接线 | 已实现待窗口（R） | 有 oracle 行（`Message.cs:2265/2274`） | 无 | 认火把耐久链未接 ⇒ 零帧 |
| T-181 | `SM_QUERYYBSELL`/`SM_QUERYYBDEAL` 登记不接线 | 不可达登记 | 有 oracle 行（`ExecutionProcessingSys.cs:2443/2550`） | 无 | 认脚本 handler 不存在 |
| T-182 | `SM_RECONNECT`(802) 登记不接线 | 不可达登记 | 只有结论 | 无 | 认跨服子系统整体未移植 |
| T-183 | `SM_SKELETON`(33) 登记不接线 | 不可达登记 | 有 oracle 行（`PlayObject.Operate.cs:736`） | 无 | 认尸体转骷髅状态未建模 |
| T-184 | 登录帧→首扫帧间隔归 T-73（不是 SearchTick 基准） | 已知偏差（时序 R） | 有 oracle 行（`PlayObject.cs:899-901`、`WorldServer.cs:663-667`） | 无 | 认 T-73 选项 a 既决 |
| T-186 | 数值/角色族 44 条的目标数值态缺口 | 不可达登记（R） | 只有结论 | 无 | 认 `Entity.gold` 恒 0、真值在 SessionGround |
| T-187 | 「发给其它角色」的 SysMsg 无通道 | 不可达登记（R） | 只有结论 | 无 | 认只出发起者、目标侧缺通道 |
| T-188 | 背包/物品绑定宿主未接线 | 不可达登记（R） | 只有结论 | 无 | 认五条命令过门后零帧 |
| T-189 | 魔法库/技能表宿主未接线 | 不可达登记（R） | 只有结论 | 无 | 认 `@TrainingMagic` 三支判不出 |
| T-190 | 运行期配置写点与跨服在线表未接线 | 已知偏差 | 只有结论 | 无 | 认①死门有测试②单区等价 |
| T-191 | `MessageSettings` 文案取构造默认值 | 已知偏差 | 有 oracle 行（`MessageSettings.cs:39-97`） | 无 | 认 13 键与部署逐字相同 |
| T-192 | 城堡商人 `FixedHideMode` 的 `RM_DISAPPEAR` 侧零帧 | 已实现待窗口（R） | 有 oracle 行（`Merchant.cs:145-157`） | **有**（reviewer-1 `1de72e17`） | 认已拆卡承接 `RM_DISAPPEAR` |

---

## C 组 — 需裁决（行为偏差要 owner 拍板）

> 判定标准：不是「今天等价」，而是**存在需 owner 选择的方向**（内容冻结面 / 是否现在补实现 / 是否改口径），且不同选择代价不同。逐条给「两个选项 + 各自代价」，本表不推荐、不下裁决。

| 编号 | 一句话主题 | 登记类型 | 依据形态 | reviewer 复核痕迹 | 裁决焦点（≤30 字） |
| --- | --- | --- | --- | --- | --- |
| T-175 | `Dragon1=神兽1` 是否复活 `SM_CHANGEFACE` | 不可达登记（部署级） | 有 oracle 行（`ElfMonster.cs:72`、`ElfWarriorMonster.cs:149`、`WorldServer.MonGen.cs:666`）+ `Server.conf:130` | 无 | 内容冻结面是否放开 |
| T-193 | GuildOfficial 等三条漏斗仍丢 `SM_MENU_OK(767)` | 缺陷跟踪（R） | 有 oracle 行（`ExecutionProcessingSys.cs:3416`、`PlayObject.Message.cs:1953`）+ 真机抓包 `body_sha256` | **有**（W-2 复核后收窄） | 是否现在下沉 mapping |
| T-173 | 名单名比较只折叠 ASCII（非 .NET Unicode 折叠） | 已知偏差 | 有 oracle 行（`WorldServer.cs:1021`）+ 卡 `openmir2-envir-adminlist-consumer` | **有**（复核 R6-1） | 是否改 Unicode 折叠 |

### C-1 T-175 `Dragon1=神兽1` 是否复活 `SM_CHANGEFACE`

**事实链（本次已核）**

- oracle：`src/M2Server/Monster/Monsters/ElfMonster.cs:72` `MakeClone(SystemShare.Config.Dragon1, this)`；现网 `Server.conf:130` **`Dragon1=神兽`**（race 113）⇒ 克隆出 113，`:76` 的 `is ElfWarriorMonster` 恒假 ⇒ 全仓唯一 `new ElfWarriorMonster()`（`WorldServer.MonGen.cs:666`）永不执行 ⇒ `ElfWarriorMonster.cs:149` 的 `RM_CHANGEFACE` 不可达。
- Rust 世界层**已建模**：`crates/world/src/monai.rs:2949` 的 `MakeClone` 支在 `:2983` 发 `RM_CHANGEFACE`；但克隆名硬编码 `DRAGON1_NAME = "神兽"`（`monai.rs:3632`）；宿主 `bins/game-svr/src/lib.rs` 无该臂（`grep RM_CHANGEFACE` = 0 命中）。
- 这条与 `whitelist.md` 现行 T-175 的**前提表述不一致**（见 §自检 C-1）：现行写「上游 ElfWarrior 分裂支未移植」，实际是「世界层已建模、现网内容不可达 + 宿主缺臂 + 克隆名硬编码」。

**选项 A：保持内容冻结，不复活**

- 做什么：不改 `Server.conf`，不接线；保留 T-175 登记。
- 代价：换形功能在现网永不生效（两侧都不出 1104，**当前等价**）；零代码风险、零窗口成本。
- 遗留：T-175 的前提文字需按孤儿审计订正，否则会误导后续排期。

**选项 B：复活（三步同批）**

- 做什么：① 内容面把 `Server.conf`/`Server1.conf` 的 `Dragon1` 改回 `神兽1`（**冻结面，须 owner 裁决**）；② `crates/world` 拆掉 `DRAGON1_NAME` 硬编码，改随 `PetSlaveConfig.dragon1` 注入（`slave.rs:252` 召唤路径已有该通路，克隆路径未走）；③ 补宿主 `lib.rs` 的 `RM_CHANGEFACE` 臂。
- 代价：**M**（三步须同批；只做 ①+③ ⇒ oracle 出 114 / Rust 仍出 113，**先制造行为差异**且 Rust 永不出 1104）；需真机窗口复跑 ELF 分裂场景。

### C-2 T-193 GuildOfficial 等三条漏斗仍丢 `SM_MENU_OK(767)`

**事实链（本次已核）**

- 767 在**商人 4 处 `GotoLable` 排水口**已接线，并由真机 C# 抓包逐字节钉住（`body` 38B、`body_sha256=e6d73c8e…`）。
- 仍丢 767 的三条漏斗：① `NpcHost::run_login`（`@Login`/QManage）；② QFunction（`run_party_goto_label`，部署 `QFunction-0.txt` 5 处 `MESSAGEBOX`）；③ **`GuildOfficial`（本部署可达）**——`guild_official::queue_script_output`（`bins/game-svr/src/guild_official.rs:377`）直接调 `pickwire::script_frame`（`pickwire.rs:151`），767 无映射 ⇒ 整帧丢。
- 可达证据：`Envir/Npcs.txt` 首行「比奇国王」type=1=GuildOfficial，脚本有 6 处 `MESSAGEBOX`；持「祈福项链」无「祝福油」→ 点国王 → `[@rw03]` `#ELSEact` ⇒ C# 发 767、本仓 0 帧。

**选项 A：现在把映射下沉进 `pickwire::script_frame`**

- 做什么：在 `pickwire::script_frame` 内补 767（及同函数内 `RM_MERCHANTDLGCLOSE(10127)`/`RM_WINEXP(8075)`/`RM_DURACHANGE(10125)`/`RM_ADJUST_BONUS(8078)` 等）。
- 代价：改 `pickwire.rs`，可能触 `guild_official.rs`/`lib.rs`（与在跑车道写集需协调）；M 级；需窗口复跑三条漏斗。
- 收益：一次覆盖 GuildOfficial + 未来 `@Login`/QFunction，消除**现网可达**的客户端零帧差异。

**选项 B：维持现状，按现登记等待**

- 做什么：不改代码，保留 T-193。
- 代价：**现网可达路径持续分叉**——比奇国王 `[@rw03]` 分支 C# 发 767、Rust 零帧；零改动风险。
- 遗留：卡 `openmir2-ya52-menu-ok-missing` 已承接，需 owner 决定排期优先级。

### C-3 T-173 名单名比较只折叠 ASCII（非 .NET `OrdinalIgnoreCase`）

**事实链（本次已核）**

- oracle：`src/GameSrv/Word/WorldServer.cs:1021` `GetHumPermission` 用 `string.Compare(..., StringComparison.OrdinalIgnoreCase)`。
- Rust：`crates/data/src/lists.rs:358` `lookup_admin` 用 `eq_ignore_ascii_case`（`adminlist.rs:109` 复用）。
- 差：`Ä/ä`、`Σ/σ`、全角 `Ａ/ａ` 在 .NET 相等、Rust 不等。当前**不可达**：`GetHumPermission` 唯一调用点在 `PlayObject.cs:1188-1194` 整段注释 ⇒ 生产零调用。

**选项 A：接受域限定（仅 ASCII + GBK 汉字语料等价）**

- 做什么：不改代码；把「域限定」明确写进卡（`lookup_folds_ascii_only` 已把「非 ASCII 不折叠」钉成显式事实）。
- 代价：将来若开 `D-*` 接 `Permission` 写点、且角色名含非 ASCII 大小写字母，会与 oracle 分叉；零改动、零窗口。
- 约束：开 `Permission` 写点前**必须先做 B**（可作为 B 的前置条件登记）。

**选项 B：改 Unicode simple case folding（逐字符 fold 后比较）**

- 做什么：改 `crates/data/src/lists.rs` 的 `lookup_admin`（及共用 helper 的名单比较）。
- 代价：须同步更新 `lookup_admin_is_ordinal_ignore_case`/`lookup_folds_ascii_only`（后者是红检，改成完整折叠即红）；并复查同一 helper 是否被物品名等其它名单复用（可能引入新的等价关系）；L 级。
- 收益：与 .NET `OrdinalIgnoreCase` 逐字符一致。

---

## 自检记录（本卡唯一质量门槛）

抽样核对 **15 条**（A/B 组 ≥5 的要求已超额），逐条对照 oracle 源与 Rust 代码：

| 编号 | 核对动作 | 结论 |
| --- | --- | --- |
| T-169 | `grep -rn RING_SPACEMOVE_ITEM mir2-rs/crates mir2-rs/bins` = **0**；`PlayObject.cs:2235-2237` 为 `case RING_SPACEMOVE_ITEM: Teleport = true;` | **一致** |
| T-170 | `/e/MirServer/M2GameSvr/Server.conf`：`GuildRecallTime=600`(718)、`GroupRecallTime=5`(719)、`UserMoveCanDupObj=1`(828)、`UserMoveCanOnItem=1`(829)、`UserMoveTime=10`(830) | **一致（逐项）** |
| T-171 | `MapManager.cs:10` = `Dictionary<string, IEnvirnoment>(StringComparer.OrdinalIgnoreCase)` | **一致** |
| T-174 | `PlayObject.cs:4121` = `SendRefMsg(RM_BREAKWEAPON,…)`；`PlayObject.Message.cs:2016` = `SM_BREAKWEAPON` | **一致** |
| T-175 | `ElfWarriorMonster.cs:149` 命中；`ElfMonster.cs:72` `MakeClone(Config.Dragon1, this)`；`monai.rs:2983` 已发 `RM_CHANGEFACE`、`:3632` 硬编码 `DRAGON1_NAME`；`lib.rs` `RM_CHANGEFACE` **0 命中** | **前提表述不一致 → 见下** |
| T-176 | `PlayObject.Message.cs:1751`=`SM_OPENDOOR_OK`、`:1754`=`SM_CLOSEDOOR`（本次实读） | **一致** |
| T-177 | `BaseObject.Base.cs:219` `ShowHp && …>ShowHpInterval`；`BaseObject.cs:933-942` `BreakOpenHealth` 全 body；`Message.cs:2013`=`SM_CLOSEHEALTH` | **一致** |
| T-179 | `BaseObject.cs:570/752` 发 `RM_ITEMSHOW`；`Envirnoment.cs:90` `if (mapItem.ItemId == 0) return false;`；全仓 `src` 无 `MapItem.ItemId` 赋值点 | **一致** |
| T-180 | `PlayObject.Message.cs:2265/2274` = 两处 `RM_LAMPCHANGEDURA` | **一致** |
| T-181 | `ExecutionProcessingSys.cs:2443`=`RM_QUERYYBSELL`、`:2550`=`RM_QUERYYBDEAL` | **一致** |
| T-182 | `mir2-rs` 无 `!servertable.txt` 装载；`smframe_ext.rs:67` 自述「未移植（G11）」 | **一致** |
| T-183 | `PlayObject.Operate.cs:736` = `RM_SKELETON` | **一致** |
| T-186 | `entity.rs` 字段行号实测 `pk_point:591/luck:612/use_items:667/level:822/exp:826/gold:872`；`world.rs` 三处 `.gold=` 全在怪物生成/复活路径，玩家无写点 | **一致** |
| T-192 | `Merchant.cs:145/147/148/153/155`（先发 `RM_DISAPPEAR` 后置位、解除先清零） | **一致** |
| T-193 | `ActionOfMessageBox` = `ExecutionProcessingSys.cs:3416`；`SM_MENU_OK` 支 = `PlayObject.Message.cs:1953`；`queue_script_output` = `guild_official.rs:377`、`pickwire.rs:151` | **一致** |

**降档 / 订正说明（1 条）**

- **C-1（T-175）**：`whitelist.md` 现行 T-175 写「该怪物分裂支未移植（`bins/game-svr/src/mongen.rs` 只到 ElfWarrior 的 704/706/708 形态门）」，但实测 `crates/world/src/monai.rs:2949/2983` **已建模并已发** `RM_CHANGEFACE`，真正的断点有三：宿主 `lib.rs` 缺臂、`DRAGON1_NAME` 硬编码、现网 `Dragon1=神兽` 造不出 race-114。⇒ 本条**不按「未移植」签批**，已置 C 组并给出「保持冻结 / 三步复活」两个选项。此为本次唯一发现的「描述与代码不一致」，其余 14 条抽样均一致、无降档。

---

## 残留物清单

- 无新增临时文件/进程/数据库；本卡只新增 `docs/whitelist-review-sheet-20261023.md` 一个文件。
- 未修改 `mir2-rs/tests/parity/whitelist.md`、任何代码、规则表、看板。
