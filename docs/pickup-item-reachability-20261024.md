# pickup_item 端到端可达性：T-19/T-25 下的路线裁决 — 交付报告

卡：`openmir2-pickup-item-reachability`（tier：**L**，理由见 §1）· owner：openmir2-worker-4 ·
分支：`card/pickup-item-reachability`（base = walgit/master `152d9e985`）· 2026-10-24

## 0. 结论（先给答案）

**选乙：`pickup_item` 登记为「已接受非目标 / 结构性不可判」，不伪造成功支。**

甲（改走 `ControlDropItem` + 便宜物）**只能让 600 可达，不能让拾取可达**：该支在
`PlayObject.Operate.cs:284-289` 直接 `Dispose(userItem)` + `ItemList.RemoveAt(i)`，
**根本不创建 `MapItem`**；场景后续的 `CM_WALK`/`CM_PICKUP` 仍然无物可拾，
`SM_ADDITEM(200)`/`SM_BAGITEMS(201)` 依旧结构性不可达。甲还要改部署 `Server.conf`
（`ControlDropItem=0` ⇒ 1），这是**仓库外的全局环境变更**，而场景 DSL 没有配置声明通道
（`requires` 只认 `magic`），无法做到「声明层可机判 + 两侧同构」。因此乙是唯一自洽路线。

| 项 | 值 |
| --- | --- |
| 确认 status oid | `e8f0f70c2b760986d2c4bd96025667a9de21ada6`（parent `522a28bf`） |
| base | `152d9e985c42c5e1dbaef28a13bee97af71d673a`（walgit/master） |
| 分支 | `card/pickup-item-reachability` |
| tier | **L**（纯登记：场景 `blocked` + 文档勾注 + 离线审计脚本；零 Rust、零 `src/**`、零 `rules.json`、零协议/DB/RNG/时序改动） |
| window_required | **false**（判据是「登记缺失必红」的离线审计，可在钉住 commit 上复跑；不需要真机窗口） |

> tier 说明：本卡 topic 是「窗口可达性」，但本路线**没有行为面改动**——它记录的是一条
> **oracle 结构与 Rust 1:1 复刻**（T-19/T-25），不是新实现。按 `AGENTS.md` 判据分级，
> 不触及 RNG/时序/落库/战斗数值，故判 L；若 reviewer 认为「可达性结论」本身必须占窗口，
> 可改判 R 并补 `window_required:true`，但本路线没有可跑的窗口项（场景已被封存）。

## 1. ① 取证链（C# 行号齐全）

### 1.1 门：`MapItem.ItemId` 恒 0 ⇒ `AddItemToMap` 恒 false

| 证据 | 位置 | 内容 |
| --- | --- | --- |
| 字段与构造器 | `src/Modules/SystemModule/Data/MapItem.cs:5-42` | `record struct MapItem`；`ItemId` 默认 0（`:10`）；构造器里唯一赋值被注释：`:41` `// this.ItemId = M2Share.ActorMgr.GetNextIdentity();` |
| 唯一门 | `src/M2Server/Maps/Envirnoment.cs:88-92` | `public bool AddItemToMap(...)` 首行 `if (mapItem.ItemId == 0) { return false; }` |
| 金币合并支同样跳过 0 | `Envirnoment.cs:114-123` | 遍历同格 `CellType.Item` 时 `if (cellItem.ItemId == 0) continue;` |
| 删除匹配 | `Envirnoment.cs:635` `DeleteFromMap(..., cellId, ...)` | 按 `CellObjId` 匹配 ⇒ 无物可删 |

**全仓 `MapItem` 创建点只有 4 处，全部不赋 `ItemId`**：

1. `src/M2Server/Actor/BaseObject.cs:551`（`DropItemDown`，普通丢弃）；
2. `src/M2Server/Actor/BaseObject.cs:740`（`DropGoldDown`，掉金币）；
3. `src/Modules/ScriptEngine/Processings/ExecutionProcessingSys.cs:5694`（脚本 `@throwitem` 金币支）；
4. `src/Modules/ScriptEngine/Processings/ExecutionProcessingSys.cs:5730`（脚本 `@throwitem` 物品支）。

四处的 `mapItem.ItemId` 只作为**读**出现在 `SendRefMsg(RM_ITEMSHOW, ..., mapItem.ItemId, ...)`
（`BaseObject.cs:570`/`:752`、`ExecutionProcessingSys.cs:5703`），从不赋值。
`PlayObject.cs:3154` / `RobotPlayer.Base.cs:547` 读的是**地上已有**的 `CellObjectMgr` 项，不是生产者。

> **类型感知复核（关键）**：`grep -rnE 'ItemId\s*=[^=]' src/ --include=*.cs` 现有 3 处命中，
> 全部是 **`DropExpDropInfo`**（`src/BotSrv/MShare.cs:923`，机器人遥测结构体）的同名字段，
> **不是 `MapItem`**：`DropExpSeqSelfCheck.cs:111-112`（测试字面量）、`RobotPlayer.cs:1490`
> （从 `SM_ITEMSHOW` 帧的 `Param` 反填）。详见 §6 越界发现 F1。

### 1.2 普通丢弃：恒 601

| 步骤 | 位置 | 行为 |
| --- | --- | --- |
| 入口 | `PlayObject.Message.cs:801-810` | `CM_DROPITEM` 成功发 `SM_DROPITEM_SUCCESS(600)`（`:804`），失败发 `SM_DROPITEM_FAIL(601)`（`:808`），`Recog = nParam1 = MakeIndex` |
| 匹配与失败 | `PlayObject.Operate.cs:251-306` | `ClientDropItem`：DealLastTick 门（`:275`）、`IsCanDrop`/`NoThrowItem`（`:261-266`）、按 `MakeIndex`+名称匹配（`:279-283`） |
| 普通支 | `PlayObject.Operate.cs:292-299` | `DropItemDown(userItem, dropWide, false, 0, ActorId)`；返回 false 则 `result=false` |
| 造物 | `BaseObject.cs:529-583` | `new MapItem { UserItem, Name, Looks, ... }`（`:551-565`）**不含 ItemId**；`:568` `if (Envir.AddItemToMap(dx, dy, mapItem))` ⇒ 恒 false；`:570` 的 `RM_ITEMSHOW` 永不执行 |

⇒ 普通丢弃**永远** 601，物品留在背包；这正是 `ground_vectors.json` 的 `drop_fail_item_id_zero`
与 Rust `ground.rs` 的 `put_item` 守卫（`:276`）两侧同构的形态（T-19）。

### 1.3 便宜物支：600 可达，但物品被销毁

| 证据 | 位置 | 内容 |
| --- | --- | --- |
| 判定 | `PlayObject.Operate.cs:284` | `if (Config.ControlDropItem && stdItem.Price < Config.CanDropPrice)` |
| 动作 | `PlayObject.Operate.cs:286-289` | `Dispose(userItem); ItemList.RemoveAt(i); result = true; break;` —— **没有 `new MapItem`，没有 `AddItemToMap`，没有 `RM_ITEMSHOW`** |
| 金币同构 | `PlayObject.Operate.cs:229-233` | `ControlDropItem && nGold < CanDropGold` ⇒ `RM_MENU_OK`（CanotDropGoldMsg），不是产出地物 |
| 机器向量 | `mir2-rs/tests/parity/m3/fixtures/ground_vectors.json` case `drop_ok_control_price` | `ctx.control_drop_item=true`、`can_drop_price=500`、请求 `make_index=52`；期望 `immediate_frames=[{ident:600, recog:52, ...}]`、`notices=[10115]`（只有负重变化，**无 610 ITEMSHOW**）、`ground_before=null`、后态 `item_list=[53]`（52 已被移除）。⇒ 600 与「地物落地」互斥 |

### 1.4 便宜物阈值（可复算）

| 证据 | 位置 | 值 |
| --- | --- | --- |
| 木剑 Price | `sql/mir2_data.sql:1197`（`stditems` 列序 `:936-985`，第 23 列 `Price`） | **50** |
| C# 缺省 | `GameSvrConf.cs:730` `ControlDropItem`；`:738` `CanDropGold`；`:742` `CanDropPrice`；缺省 `:1703` false / `:1705` 1000 / `:1706` 500 | `50 < 500` ⇒ 若开关打开，木剑走便宜物支 |
| C# 读取 | `ServerConf.cs:481` / `:483` / `:484` | `[Setup] ControlDropItem` / `CanDropGold` / `CanDropPrice` |
| 部署现值（只读核对） | `E:/MirServer/M2GameSvr/Server.conf:720-723`（`M2GameSvr-hook`、`Mir200` 同值） | `ControlDropItem=0`、`InSafeDisableDrop=1`、`CanDropGold=1000`、`CanDropPrice=1000`；`DropItemRage` 无行 ⇒ C# 缺省 3（`:1754`） |
| Rust 同构 | `mir2-rs/bins/game-svr/src/config.rs:1185/1187`（缺省 false/500）、`ground.rs:706-710`（同一 `<` 门） | 两侧同构 |

### 1.5 拾取支：地面无物 ⇒ `ClientPickUpItem` 静默返回

| 步骤 | 位置 | 行为 |
| --- | --- | --- |
| 位置门 | `PlayObject.Message.cs:811-817` | 仅 `CurrX==nParam2 && CurrY==nParam3` 才 `ClientPickUpItem()` |
| 取物 | `PlayObject.Base.cs:55-66` | `Envir.GetItem(CurrX, CurrY, ref mapItem)`；`!success` 直接 `return`（无下行帧） |
| 进包 | `PlayObject.Base.cs:98-121` | `IsEnoughBag` → `DeleteFromMap` → `AddItemToBag` → `SendAddItem`（`:117`） |
| 200 帧 | `PlayObject.Base.cs:198-226` | `Messages.MakeMessage(SM_ADDITEM, ActorId, 0, 0, 1)`（`:226`） |
| 常量 | `src/OpenMir2/Messages.cs:46/220/221` | `CM_PICKUP=1001`、`SM_ADDITEM=200`、`SM_BAGITEMS=201` |

⇒ 地面永远无物（§1.1-1.3）⇒ `GetItem` 恒失败 ⇒ 200/201 两侧都不可达。这与 `window-run-1`
实测 `pickup_item` 双侧 `[200,201]` 超时一致，也与 T-25 的既有登记一致。

### 1.6 T-25 既有机器证明在 master 上是 RED（预存，非本卡引入）

`powershell -File tools/m2-acceptance/check_pickup_unreachable.ps1` 在 master `152d9e985`
实测 **rc=1**，点名 3 处 `ItemId =`。经类型鉴定（§1.1 末段）全部是 `DropExpDropInfo`，
脚本的正则是**类型盲**的。本卡未修改该脚本（越界发现 F1，见 §6），改用上述「4 个 `new MapItem`
+ 0 个 ItemId 赋值」的类型感知证据链。T-25 的结论不变。

## 2. ② 甲 vs 乙（为什么不是甲）

| 甲的必答项 | 结论 | 依据 |
| --- | --- | --- |
| `Server.conf` 可配？ | **可配，但只能改仓库外的部署件** | C# `ServerConf.cs:481/484`；Rust `config.rs:823/829`；部署现值 `E:/MirServer/M2GameSvr/Server.conf:720/723`。场景 DSL 顶层白名单只有 `name/description/params/required/computed/placement/blocked/requires/inventory/steps`（`tools/probe/scenarios/README.md:26`），`requires` 只认 `magic`（`mir2_probe.py:357`）⇒ **没有配置声明通道** |
| 取值约束？ | `ControlDropItem=true` 且 `stdItem.Price < CanDropPrice`（严格小于） | `PlayObject.Operate.cs:284`；木剑 Price=50（`sql/mir2_data.sql:1197`） |
| 便宜物阈值？ | 部署 `CanDropPrice=1000` ⇒ 50 命中；C#/Rust 缺省 500 也命中 | `GameSvrConf.cs:1706` / `config.rs:1187` |
| 两侧同构？ | **600 那一帧同构**（`Operate.cs:284-289` ↔ `ground.rs:706-710`） | 已由 `drop_ok_control_price` 向量钉住 |
| 场景可机判？ | **不能端到端**：便宜物支销毁物品、不建 `MapItem`，步骤 3-6（走 + 拾 + 200/201）仍不可达 | `Operate.cs:286-289`；`ground_vectors.json` `drop_ok_control_price` 的 `ground_before=null` |
| 副作用 | 改部署开关会改变**所有**场景/所有玩家的丢弃语义（含金币阈值 `:229`），且不在仓库内可复现 | 同上 |

**裁决**：甲只能把「600 同步点」变成可达，却把场景从「拾取」偷换成「丢弃 ack」，且依赖
仓库外的全局开关——不满足卡面「端到端可达性」与「声明层可机判」。**选乙。**

## 3. ② 乙的登记内容（交付物）

| 文件 | 改动 |
| --- | --- |
| `tools/probe/scenarios/pickup_item.json` | 顶层新增 `blocked`（`reason` 引 **T-19/T-25** + `Envirnoment.cs:88-92`；`missing_fixture` 写明「需要 `ItemId != 0` 的地物，oracle 无生产路径」）；`description` 改为 BLOCKED 说明；末步 note 由「按生产链必达」订正为「两侧结构性不可达」。步骤/placement/inventory 保留作证据与回归标本 |
| `docs/window-batch-plan-20261023.md` §7.4 | `pickup_item` 移出「现在就能跑」表（11 行 / 17 个场景）；加 2026-10-24 勾注（T-19/T-25 + 证据链 + 甲否决理由）；§2 历史行加「已作废」勾注 |
| `docs/window-runbook-20261023.md` | §4.5 参数发现表 + 参数发现说明加「封存提醒」：参数可发现，但场景已 `blocked`，`--dry-run` 按设计 `rc=2 BLOCKED`，不进窗口 |
| `docs/window-scenario-dryrun-sweep-20261024.md` | 加 2026-10-24 addendum：该文件 `pickup_item` 的 `rc=0 / 可跑` 是 09:58 历史快照，现按设计为 `rc=2 BLOCKED`；其余场景 rc 不变 |
| `tools/hygiene/scenario_undecidable_audit.py`（新） | 乙的机器牙：检查 `blocked` + `reason` 引 T-19/T-25 + §7.4 勾注；`--selftest` 6 组变异 |
| 本报告 | 取证链 / 裁决 / 牙 / 门禁 / 残留 |

**行为面零改动**：`git diff --stat walgit/master -- mir2-rs src tools/conformance/rules.json` 为空；
`pickup_item.json` 的 `steps` 一字未改（只改 `description`/末步 `note`/新增 `blocked`）。

## 4. ③ 牙（登记缺失必红）

命令：`python -I tools/hygiene/scenario_undecidable_audit.py check` / `... selftest`

`--selftest` 实测**6/6 PASS**（临时目录，不碰工作区）：

| 变异 | 期望 | 实测 |
| --- | --- | --- |
| G good（三条齐全） | GREEN | PASS（0 error） |
| M1 删顶层 `blocked` | RED | PASS（`缺顶层 blocked 对象`） |
| M2 `reason` 不含 T-25 | RED | PASS（`未引用 T-25`） |
| M3 `missing_fixture` 空 | RED | PASS（`missing_fixture 为空`） |
| M4 §7.4 行缺「结构性不可判」 | RED | PASS（`没有把 'pickup_item' 标注为…`） |
| M5 §7.4 行不含 T-19/T-25 | RED | PASS（`未引用 T-19/T-25`） |

真实树上的 `check`：**GREEN rc=0**。若把场景的 `blocked` 删掉，M1 的同一路径即 RED；
若把 §7.4 勾注删掉，M4 即 RED。⇒ 「登记缺失必红」不是人工清单，是可复跑的脚本。

## 5. ④ 门禁（本卡 head 实跑）

| 门禁 | 结果 |
| --- | --- |
| `python -I -B tools/conformance/card_window.py --selftest` | **GREEN 279/279，rc=0** |
| `python -I tools/hygiene/conflict_marker_gate.py` | OK（0 markers / 7502 tracked files），rc=0 |
| `python -I -B tools/hygiene/stale_pointer_gate.py` | OK（164 files / 37905 lines；0 card-status hit），rc=0 |
| `python -I -B tools/hygiene/dynamic_count_stamp_gate.py` | OK（0 缺戳），rc=0 |
| `python -I tools/hygiene/scenario_undecidable_audit.py check` / `selftest` | GREEN rc=0 / 6/6 PASS rc=0 |
| `python -I -B tools/probe/mir2_probe.py --check-scenario --scenario tools/probe/scenarios/pickup_item.json --var pickup_x=321 --var pickup_y=329 --var pickup_xy=21561665` | `scenario OK (6 steps)`，rc=0（`blocked` 形状合法） |
| `python -I -B tools/conformance/card_window.py --dry-run --scenario tools/probe/scenarios/pickup_item.json --profile alt` | **rc=2 `VERDICT: BLOCKED`**（设计内：不起栈、不建 out、不碰端口） |
| `git diff --check` | rc=0 |
| `git merge-tree --write-tree walgit/master HEAD` | 见收尾记录（无冲突） |
| Rust / src / rules.json 零变更 | `git diff --stat walgit/master -- mir2-rs src tools/conformance/rules.json` 为空 |

## 6. 越界发现（登记，不在本卡修）

**F1：`tools/m2-acceptance/check_pickup_unreachable.ps1` 在 master 上是 RED（预存，类型盲）。**
脚本用 `ItemId\s*=[^=]` 全树扫，不看接收者类型；`src/BotSrv` 新增的机器人遥测结构体
`DropExpDropInfo`（`MShare.cs:923`）有同名字段，导致 3 处假阳性（`DropExpSeqSelfCheck.cs:111-112`、
`RobotPlayer.cs:1490`），实测 **rc=1**。T-25 结论本身不受影响（§1.1 的四点类型感知证据）。
建议另开 L 卡：把脚本的正则改成「`MapItem` 上下文/类型感知」，并在 selftest 里钉住
`DropExpDropInfo` 不误报。**本卡未改该脚本**（写集外，且它属于 M2 验收②证据面）。

## 7. 残留物清单

- 工作区外的临时物：无（审计脚本 selftest 用 `tempfile.mkdtemp` 并在 `finally` 清理）。
- 部署件：只**只读**核对 `E:/MirServer/**/Server.conf:720-723`，未改、未起栈、未碰 mysqld。
- 场景封存后 `--dry-run` 的 rc 由 0 变 2 是**设计内 BLOCKED**，不是回归；已在 §3 的三份文档中注明。
- 未修：F1（另有建议卡）、§1.3 的 600/601 帧登记（600/601 已由 master `152d9e985` 前的两张卡完成）。
