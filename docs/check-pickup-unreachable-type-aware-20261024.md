# `check_pickup_unreachable.ps1` 类型感知修复（tier L，工具面）

- 卡：`openmir2-check-pickup-unreachable-type-aware`
- 承接：`openmir2-pickup-item-reachability` 的越界发现（T-25 结论**不变**）
- 结论：**T-25 不变**（`MapItem.ItemId` 恒 0 ⇒ `AddItemToMap` 恒 false ⇒ 地面无物 ⇒ `CM_PICKUP` 不可达）；
  本卡只修检查器自身的**类型盲假红**，不放宽判据语义。
- 写集：`tools/m2-acceptance/check_pickup_unreachable.ps1`、本文档。Rust 零变更，`src/**` 零变更。

## 1. 现象（master `152d9e985`）

```powershell
powershell -ExecutionPolicy Bypass -File tools/m2-acceptance/check_pickup_unreachable.ps1
# rc=1（假红）
RED: 发现未注释的 ItemId 赋值点 —— pickup 不可达的证明被推翻：
  src\BotSrv\DropExpSeqSelfCheck.cs:111: new MShare.DropExpDropInfo { ..., ItemId = 0, ... }
  src\BotSrv\DropExpSeqSelfCheck.cs:112: new MShare.DropExpDropInfo { ..., ItemId = 0, ... }
  src\BotSrv\Player\RobotPlayer.cs:1490:                       ItemId = b,
```

三处全部是 **`MShare.DropExpDropInfo`**（`src/BotSrv/MShare.cs:923` 声明的另一个结构体）的对象初始化器，
与 `SystemModule.Data.MapItem` 无关。旧实现用 `ItemId\s*=[^=]` 全局扫、**不看类型/构造点** ⇒ 把
`DropExpDropInfo.ItemId` 当成 MapItem 赋值。

## 2. 修复方式（类型/构造点感知，非「凡 .ItemId 即忽略」）

保留原证明骨架（B 构造器注释、C `AddItemToMap` 守卫），把 A 段换成**只认指向 MapItem 的赋值点**：

| 规则 | 识别对象 |
| --- | --- |
| A1 | `new MapItem { … ItemId = … }`：只查该**构造调用**对象初始化器的**顶层**（depth==1）；嵌套在里面的其它结构体初始化器不算 |
| A2 | MapItem 类型变量（`MapItem x` 声明 / `var x = new MapItem`）上的 `x.ItemId = …`；变量名**按文件**收集，避免跨文件同名误配 |
| A3 | `Get<MapItem>(…).ItemId = …` 链式写 |
| A4 | 经 MapItem 字段/属性的链式写：`xxx.MapItem.ItemId = …` |
| 前置 | 先做**注释/字符串遮罩**（`//`、`/* */`、普通/verbatim/interpolated 字符串、字符字面量），换行保留 ⇒ 行号仍对得上；注释里的 `// this.ItemId = …` 不算代码 |

- 赋值运算符覆盖 `=` 与复合赋值（`+=`/`-=`/`??=` …），`==`/`!=` 比较排除。
- **失败即红**：`new MapItem` 的构造实参/初始化器括号不配对时无法判定 ⇒ 记 RED，不静默通过。
- 残留口径（故意保守，写在脚本头注释里）：类型追踪只覆盖显式声明与 `new` 结果，不做完整语义分析；
  若将来把 MapItem 塞进 `object`/`dynamic` 再写字段，脚本看不见 ⇒ 应补规则而不是放宽匹配（宁可假红，不可假绿）。

## 3. 验收

### ① master 上 rc=0

```
powershell -ExecutionPolicy Bypass -File tools/m2-acceptance/check_pickup_unreachable.ps1
# rc=0（GREEN），实测输出：
-- A) 扫描 901 个 .cs 找**指向 MapItem** 的 ItemId 赋值点（注释/字符串已遮罩）
  MapItem 类型变量名（全仓去重）3 个：cellItem, mapItem, SelMapItem
  OK: 0 个指向 MapItem 的未注释 ItemId 赋值点（其它结构体的 .ItemId 不计；唯一 MapItem 赋值是 MapItem.cs 注释）
  OK: MapItem.cs 构造器里 ItemId 赋值仍是注释 -> // this.ItemId = MShare.ActorMgr.GetNextIdentity();
  OK: Envirnoment.AddItemToMap 首行 if (mapItem.ItemId == 0) return false; 在
  OK: 全仓 4 个 MapItem 构造点（new MapItem / 目标类型 new），均无 ItemId 赋值（A 已覆盖）
    src\M2Server\Actor\BaseObject.cs:551: new MapItem
    src\M2Server\Actor\BaseObject.cs:740: new MapItem
    src\Modules\ScriptEngine\Processings\ExecutionProcessingSys.cs:5694: new MapItem
    src\Modules\ScriptEngine\Processings\ExecutionProcessingSys.cs:5730: new MapItem
GREEN: ... CM_PICKUP 不可达。
```

耗时约 4.4s（901 个 .cs，遮罩 + 规则扫描）。

### ② 两颗牙（临时改 `src/**` 后逐条复跑并 `git checkout --` 还原，最终 `git status` 仅脚本一个 M）

| 牙 | 构造的变异 | 实测 |
| --- | --- | --- |
| 1a | `BaseObject.cs:551` 的 `new MapItem { … }` 初始化器里加 `ItemId = 1` | **rc=1**：`src\M2Server\Actor\BaseObject.cs:556: new MapItem 初始化器顶层给 ItemId 赋值 \| ItemId = 1` |
| 1b | 同处构造点之后加独立语句 `mapItem.ItemId = 1;` | **rc=1**：`src\M2Server\Actor\BaseObject.cs:557: MapItem 变量 mapItem 的 ItemId 赋值 \| mapItem.ItemId = 1;` |
| 2 | 只改 BotSrv 的两处 `DropExpDropInfo.ItemId`（`DropExpSeqSelfCheck.cs` `ItemId = 0`→`7`；`RobotPlayer.cs:1490` `ItemId = b`→`999`） | **rc=0**（GREEN）⇒ 其它结构体不再误判 |

### ③ 门禁卫生

| 门禁 | 结果 |
| --- | --- |
| `python -I tools/hygiene/conflict_marker_gate.py` | OK（0 markers / 7502 tracked files；加入本文档后复跑为 7503，以复跑为准），rc=0 |
| `python -I -B tools/hygiene/stale_pointer_gate.py` | OK，rc=0 |
| `python -I -B tools/hygiene/dynamic_count_stamp_gate.py` | OK（0 缺戳；6 条段落声明降级 INFO），rc=0 |
| `git diff --check` | 干净，rc=0 |
| `git merge-tree --write-tree walgit/master HEAD` | 无冲突，rc=0 |
| Rust 零变更 | `git diff --stat walgit/master..HEAD` 仅本文档与检查器脚本；无 `mir2-rs/**`、`src/**` |

## 4. 影响面与残留物

- 退出码口径不变（0/1/2），`check_screenshots.ps1` / `run_acceptance.ps1` 对它的调用契约不受影响。
- `tools/m2-acceptance/evidence/m2svr-pickup-unreachable-20261012/` 是旧口径的**历史留档**，本卡未改；
  重新采集时用 `-OutDir <dir>` 会得到新文案（判定与退出码不变）。
- 临时物：`E:/tmp/w10-evidence/`（本次 `-OutDir` 冒烟产物，可删）、`/tmp/t1a.log`、`/tmp/t1b.log`、`/tmp/t2.log`（牙日志）。
- 未自批：审查归独立 principal；本卡不自行 merge / 不自签 review。

## 5. 返工：reviewer-21 request_changes（P-1..P-6，2026-10-25）

reviewer-21 在钉住 `d7bd0282d` 上判 `request_changes`：方向正确（3 条假红消除、4 个构造点、
作者 3 颗牙重现），但重写把闸门强度降低了——4 条 important 变异**新脚本绿、旧脚本红**。
本章是返工记录；§3 是初版证据（`d7bd0282d`），以下为返工后的口径。

### 5.1 逐条修法

| finding | 根因 | 修法（脚本位置） |
| --- | --- | --- |
| **P-1**（important） | B 段只验「`// this.ItemId = …` 注释还在」；A 段不认 `this.` | 新增 **B2**：对 `MapItem.cs` 的遮罩源扫 `(?:this\.)?ItemId\s*(?:op)=`，命中即 RED（`==`/`!=` 排除） |
| **P-2**（important） | `Sort-Object -Unique` 在 PS 5.1 **大小写不敏感**，把同名不同拼写（`MapItem`/`mapItem`）并掉，随后大小写敏感匹配漏一半 | 新增 `Get-UniqueOrdinal`（`HashSet[string]` + `StringComparer.Ordinal`）替换两处去重；A5 左操作数用 `-ccontains` |
| **P-3**（important） | `var x = …Get<MapItem>(…)` 推导出的变量不进名字表 | 新增 `$VarGetMapItemRe`，把 `x` 收进按文件名字表（A2） |
| **P-4**（important） | record `with { ItemId = … }` 无规则（`MapItem` 是 `record struct`） | 新增 **A5**：左操作数是本文件 MapItem 变量（或邻近 `new MapItem`/`Get<MapItem>`）时，用 `Get-TopLevelInitializer` 查顶层 `ItemId` 赋值 |
| **P-5**（minor） | 数组/集合元素写漏检 | 新增 `$MapItemSeqDeclRe`（`MapItem[] a` / `List<MapItem>` 等）收名字 + **A6** `name[...].ItemId = …`；头注释残留口径补「字典值/LINQ 中间量/跨文件属性链」 |
| **P-6**（minor，可选） | A2/A4 对同一行各报一次 | `Get-DedupedRedLines` 按 `(文件, 行)` 去重后输出 |

头注释同步：A 列表补 A5/A6 与 A2 的 `var … Get<MapItem>`；残留口径改为「A1–A6 之后继续加规则，
而不是放宽匹配」。**没有**放宽任何判据（仍然只认指向 MapItem 的赋值）。

### 5.2 变异重放（reviewer 原 verify 的原变异，逐条）

三个脚本：`new` = 本次返工；`reviewed` = `d7bd0282d`（有回归的版本）；`base` = `152d9e985`
（类型盲旧版，未变异也因 BotSrv 假红 rc=1）。

| 变异（reviewer 原 verify） | new | reviewed | base |
| --- | ---: | ---: | ---: |
| P-1 `MapItem.cs` 注释后加 `this.ItemId = 42;` | **1** | 0（漏检） | 1 |
| P-2 `PlayObject.cs:3154` 后加 `mapItem.ItemId = 1;`（与小写同名变量并存） | **1** | 0（漏检） | 1 |
| P-3 同处加 `var miVar = …Get<MapItem>(…)` + `miVar.ItemId = 1;` | **1** | 0（漏检） | 1 |
| P-4 `BaseObject.cs:551` 构造点后加 `mapItem = mapItem with { ItemId = 5 };` | **1** | 0（漏检） | 1 |
| P-5 同处加 `MapItem[] a = new MapItem[1]; a[0].ItemId = 2;` | **1** | 0（漏检） | 1 |

- 未变异基线：`new rc=0`、`reviewed rc=0`、`base rc=1`（原假红仍在旧版）。
- P-6 去重：临时 `src/TmpDedupeProbe.cs` 放 `holder.MapItem.ItemId = 1;` ⇒ **恰好 1 条 RED**
  （修复前 A4+A2 两条）。
- 负向控制：把 `RobotPlayer.cs:1490` 的 `ItemId = b` 改为 `ItemId = b + 1`（`DropExpDropInfo`）
  ⇒ `new rc=0`（其它结构体不再误判）。
- 所有临时 `src/**` 变异均在 `finally` 里 `git checkout --` 还原；最终 `git status` 只有检查器一个 `M`。

### 5.3 返工后 rc=0 输出（关键行）

```text
-- A) 扫描 901 个 .cs 找**指向 MapItem** 的 ItemId 赋值点（注释/字符串已遮罩）
  MapItem 类型变量名（全仓去重）4 个：mapItem, cellItem, MapItem, SelMapItem
  OK: 0 个指向 MapItem 的未注释 ItemId 赋值点（其它结构体的 .ItemId 不计；唯一 MapItem 赋值是 MapItem.cs 注释）
  OK: MapItem.cs 构造器里 ItemId 赋值仍是注释 -> // this.ItemId = M2Share.ActorMgr.GetNextIdentity();
  OK: MapItem 类型体内没有未注释的 ItemId 赋值（构造器/字段初始化都无）
  OK: Envirnoment.AddItemToMap 首行 if (mapItem.ItemId == 0) return false; 在
  OK: 全仓 4 个 MapItem 构造点（new MapItem / 目标类型 new），均无 ItemId 赋值（A 已覆盖）
GREEN: oracle 侧 MapItem.ItemId 恒 0 ⇒ AddItemToMap 恒 false ⇒ 地面无物 ⇒ CM_PICKUP 不可达。
```

（名字从 3 个变 4 个不是新增变量：`MapItem` 与 `mapItem` 是 PlayObject.cs 里两种不同拼写，
P-2 之前被大小写不敏感去重并掉了。）

### 5.4 门禁（返工后本检出实测）

| 门禁 | 结果 |
| --- | --- |
| `check_pickup_unreachable.ps1`（master src） | **rc=0**，901 个 .cs，4 构造点 |
| `conflict_marker_gate` | rc=0（0 markers / 7503 tracked files） |
| `stale_pointer_gate` | rc=0（165 files / 37986 lines；0 card-status hit） |
| `dynamic_count_stamp_gate` | rc=0（0 缺戳；6 条 INFO） |
| `t_reservation_gate` | rc=0（255 行 / 255 号） |
| `whitelist_anchor_gate` | rc=0（590 条锚 ALL PASS） |
| `m3_whitelist_registry_audit` | rc=0（GREEN） |
| `board_coverage_gate` | rc=0（577 张卡，0 hidden） |
| `git diff --check` | rc=0 |
| `git merge-tree --write-tree walgit/master HEAD` | 提交后复跑，结果见 patch body |
| Rust 零变更 | `git diff --stat <base>..HEAD -- src/ mir2-rs/` 为空 |

**T-25 结论不变**：全仓 `.ItemId` 仍无指向 MapItem 的赋值；本次只恢复闸门强度，不改变当前判定。
