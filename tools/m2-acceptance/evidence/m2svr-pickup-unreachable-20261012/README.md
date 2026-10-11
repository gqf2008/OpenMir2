# `m2-06-pickup` 不可达 —— 机器可判证据（2026-10-12）

卡：`openmir2-m2-gamesvr-host`（M2 验收② 7 张截图）+ `openmir2-m2-enter-world-frames`。
门禁脚本：`tools/m2-acceptance/check_pickup_unreachable.ps1`（本目录即其 `-OutDir` 产物）。

## 结论

**`m2-06-pickup`（捡物）在真实 oracle（C# 栈）上物理采不到——不是 Rust 侧缺件。**
⇒ 验收② 的「7 张截图」判据本身在 oracle 上不可满足；`check_screenshots.ps1` 判「缺 1 张 = RED」会把
**一个 oracle 不可能发生的动作**算成 Rust 的账。**判据如何收口（6 张 + 夹具证据 / 其他最小改法）待 owner 签字。**

## 证明链（逐条可复跑）

1. `MapItem` 是 `record struct`，`int ItemId` 字段默认 0；其构造器里**唯一一处**赋值被注释：
   `src/Modules/SystemModule/Data/MapItem.cs:41`
   `// this.ItemId = M2Share.ActorMgr.GetNextIdentity();`
   （见 `oracle-guard.txt`）
2. 全仓 **895** 个 `.cs` 里**没有任何**未注释的 `ItemId = …` 赋值点（含对象初始化器）：
   `oracle-itemid-assignments.txt` 只列出那一条注释。
3. `Envirnoment.AddItemToMap` 首行守卫：`if (mapItem.ItemId == 0) return false;`
   （`src/M2Server/Maps/Envirnoment.cs:88-93`；见 `oracle-guard.txt`）。
4. 全仓 4 个 `new MapItem` 构造点（掉落 `BaseObject.cs:551/740`、脚本
   `ExecutionProcessingSys.cs:5694/5730`）都不设 `ItemId`（见 `oracle-construction-sites.txt`）。

⇒ `MapItem.ItemId` 恒 0 ⇒ `AddItemToMap` 恒 false ⇒ **地面永远没有物品** ⇒
`ClientPickUpItem`（`PlayObject.Base.cs:55`）的 `Envir.GetItem(CurrX,CurrY,ref mapItem)` 取不到
⇒ `CM_PICKUP` 无物可拾。这是 oracle 的既有形态（`m3bag` 已登记为 **T-19**：正常 `CM_DROPITEM` 恒失败）。

## 复跑

```powershell
# GREEN=exit 0（证明成立） / 1=证明被推翻（发现新的 ItemId 赋值点） / 2=PREREQ（缺 src/**）
powershell -ExecutionPolicy Bypass -File tools/m2-acceptance/check_pickup_unreachable.ps1 `
  -OutDir tools/m2-acceptance/evidence/m2svr-pickup-unreachable-20261012
```

## 红检（"改坏必红"，原始输出留档）

- `redcheck-mutated.txt`：在 `MapItem()` 构造器里插入 `this.ItemId = 123;` ⇒ 脚本 **exit 1**（RED），
  并打印该行位置 `MapItem.cs:41`；`MapItem.cs` 由 `.bak`（= `git HEAD` 字节）**逐字节还原**。
- `redcheck-restored.txt`：还原后 **exit 0**（GREEN）；`git status --short src/` 为空（oracle 未改）。

## 对验收② 的处置建议（**待 owner 签字，本片不擅自改判据**）

- 采 **6/7**（进图/走路/说话/看见别人/打怪/小退再进）+ 本目录的**不可达证明**；
- 拾取决策本身已由 `m3bag` 的 `ground-probe` 夹具（真身 `Operate(ProcessMessage)` 反射调
  `CM_PICKUP`/`CM_DROPITEM`，21 用例逐字段 + `ItemId` 由夹具显式给值）覆盖 ⇒ ② 的"拾取"证据用该夹具，
  而不是一张 oracle 采不到的截图。
- 备选：保留 7 张判据，但给 `m2-06-pickup` 在 `check_screenshots.ps1` 增加"oracle 不可达"分类
  （需 owner 签字后落地）。

## 残留物

- 无（脚本只读 `src/**`；红检用 `.bak` 还原，`git status` 干净）。
