# blocked 场景对抗性复核（run3 的 3 条 blocked）

卡：`openmir2-blocked-scenarios-audit`（tier=L，角色 reviewer；owner `openmir2-reviewer-21`）。
基线：`master=a60e04ad9`（本轮开工时）。日期：2026-10-25。
写集：仅本文件。**未改**场景 JSON / run3 清单 / 工具代码；**未起服务端、未占端口、未碰 `E:/MirServer` 内容**。

方法：把每条「为什么跑不了」拆成一句可证伪断言，逐条回 **oracle 源码（`src/**` 文件:行）**、
**冻结内容（`E:/tmp/conformance/csharp-root/M2GameSvr/Envir/**` 与 `mir2_data` 结构）**、
**现有工具（`tools/window/run_batch.sh`、`tools/conformance/card_window.py`、`tools/probe/mir2_probe.py`）**核对。
裁定三档：**已证伪（可跑/可构造）** / **成立（真不可达）** / **部分成立（缺 X）**。

实况：`bash tools/window/run_batch.sh --list` ⇒ `场景数 24（blocked 3 ⇒ 入批 21）`，
blocked = `#21 butch_two_draws` / `#22 castle_merchant_tax` / `#24 castle_merchant_tax_upgrade`。

---

## 0. 总表

| # | 场景 | blocked 理由（一句） | 裁定 | 关键依据 |
|---|---|---|---|---|
| 21 | `butch_two_draws` | 「能力齐、只差实测」击杀→屠宰链 | **已证伪**（现工具构造下跑不通） | `fixtures.monsters` **追加**到 MonGen.txt 末尾（`card_window.py:4472`）；刷怪线程 200ms/条、部署 `ProcessMonsterMultiThreadLimit` 缺省 **1** ⇒ 单表 3081 行 ⇒ 追加行首次评估 ≈ **616s**，而 #21 是 `--timeout-ms 60000` |
| 22 | `castle_merchant_tax` | ①买武器腿缺 652 ClientItem 解码 ②`SaveTick`/600s 门不可观测 | ①**成立**（工具缺口）②**已证伪**；**真缺口=税入账原因未定位**（§2.3） | `MakeIndex` 是运行时计数（`GameItemSystem.cs:133-136`）⇒ 不可由内容推出；`SaveTick` 段只更新内存 tick（`UserCastle.cs:798-805`），写盘门是 `ConsoleShowUserCountTime`（可覆盖 5s） |
| 24 | `castle_merchant_tax_upgrade` | 「run3 内置 profile 不能声明 ⇒ 不能进批」 | **部分成立**：进批通道**已证伪**（有构造）；真缺口是 NPC 选择 | `run_batch.sh:409-421` 把 `extra` 追加在 `--profile "$prof"` **之后**；`--profile` 接受 JSON 路径（`card_window.py:857-872`）⇒ `extra:["--profile","<path>.json"]` 生效 |

---

## 1. `#21 butch_two_draws` —— **「只差实测」不成立**

### 1.1 断言拆分

- **A1**「活体夹具已落地，能在 (330,331) 刷出 1 只动物」——**部分成立**：夹具**存在**，但刷怪时序使其在窗口内**刷不出来**。
- **A2**「kill 步与 C# RNG 接线已落地」——**成立**（代码在：`tools/probe/mir2_probe.py` 的 `kill` 步；`15dbbe843` 的 RNG 接线）。
- **A3**「只差实测」——**已证伪**。

### 1.2 A1 的证据链（同一「错误假设」：往 MonGen.txt 末尾追加就能刷）

1. 夹具写盘形态：`card_window.py:4440-4474` —— `head + append`，日志原文「**追加** N 行」，写在整个文件**末尾**。
2. 加载顺序：`src/GameSrv/DB/LocalDB.cs:531-560`（`LoadMonGen` 在 `:501`）顺序解析 `MonGen.txt` ⇒ `MonGenList` 与文件行序一一对应 ⇒ 追加行落在**表尾**。
3. 刷怪节奏：`WorldServer.MonGen.cs:196-230` —— 每 `(now - RegenMonstersTick) > RegenMonstersTime` 只处理
   `mongenList[CurrMonGenIdx]` 一条，然后 `CurrMonGenIdx++`（到尾回 0）；Rust 侧同构
   （`mir2-rs/crates/world/src/mongen.rs:723-768` 的 docstring 逐条对齐 `:207/:209/:211-230`）。
4. 线程数：`ProcessMonsterMultiThreadLimit` **未出现在部署 `Server.conf`**（`grep -c` = 0）⇒ 取缺省
   **1**（`GameSvrConf.cs:1896`）⇒ `MonGenInfoThreadMap` 只有一张表，就是全部 3081 行。
5. 量化：`E:/tmp/conformance/csharp-root/M2GameSvr/Envir/MonGen.txt` 共 **3081 行**（map 0 占 77 行）；
   `RegenMonstersTime=200`（部署 `Server.conf:50`）⇒ 全表一轮 ≈ **3081 × 0.2s ≈ 616s**。
   追加行在表尾 ⇒ 首次评估 ≈ 616s 之后；`#21` 的 run3 行是 `extra:["--timeout-ms","60000"]` ⇒ 60s 内
   最多轮过约 300 行，**永远轮不到追加行**。

⇒ 现状下 `kill` 步会稳定落在三态之一 `no target frames at (330,331)`（不是「伤害不足」）。

### 1.3 解封构造（现有工具集内）

- **正解（已另卡）**：把怪物行**前置**到 MonGen.txt 首行（tester 的 P0 prepend 卡）。前置后首次评估在
  第一轮（≤200ms），`monGen.StartTick == 0` ⇒ 立即刷（`MonGen.cs:233-252`）。
- **过渡（不计入夹具口径，仅供单次实测）**：起栈前手工把该行插到 `MonGen.txt` **首行**并备份/还原；
  这违反「声明式夹具」口径，**不得**作为解封依据，只可作为验证 kill→butch 链的一次性手段。
- 备选（不推荐）：把窗口拉到 >616s（`--timeout-ms 700000` 级）——代价高且脆弱，不能算「可跑」。

### 1.4 影响面

`fixtures.monsters` 的语义是「往本侧 MonGen **追加**」（`card_window.py:4014` 注释：monsters 不进对称校验，
按侧独立声明）。**任何**未来声明 `fixtures.monsters` 的 profile 都继承这条时序限制（部署是单表 3081 行、
1 条/200ms）。目前内置 profile 未声明 monsters（只有 selftest 用），所以其它在跑场景不受影响。

---

## 2. `#22 castle_merchant_tax` —— ①成立、②已证伪

### 2.1 断言拆分

- **B1**「买**武器**腿需要 `SM_SENDDETAILGOODSLIST(652)` 的 ClientItem buffer 取 MakeIndex，当前工具解不了」——**成立**。
- **B2**「`SaveTick` 10 分钟门不可观测 ⇒ 只能真实时点复拍」——**已证伪**。
- **B3**「0151 的买/卖/修三腿 `Castle` 恒非空 ⇒ 不需要 `CastleGetAllNpcTax`」——**未复核**（见 §2.4）。

### 2.2 B1 的证据

- 买武器的判定：`Merchant.cs:1249` —— `if (stdItem.StdMode <= 4 || StdMode == 42 || StdMode == 31 || userItem.MakeIndex == nInt)`。
  武器（StdMode>4）必须 `MakeIndex` 精确相等；药品走 `StdMode<=4` 名支（这就是 #23 能跑的原因）。
- `MakeIndex` **不是** `StdItem.Id`：货架货由 `CopyToUserItemFromName` 生成
  （`Merchant.cs:309-312` → `GameItemSystem.cs:113-139`），其中 `item.Index = i + 1`（模板序号）、
  **`item.MakeIndex = M2Share.GetItemNumber()`**（运行时递增计数）⇒ **无法**从冻结内容/`mir2_data.stditems` 推导。
- 唯一客户端可见来源是 652（`Merchant.cs:1320-1349` 组包：`clientItem.MakeIndex = userItem.MakeIndex`，
  `sSendMsg += EDCode.EncodeBuffer(clientItem) + "/"`）；645 只给「名/submenu/价格/库存」。
- probe 侧：`grep -n 'ClientItem|BAGITEMS|EDCode' tools/probe/mir2_probe.py` **无**任何 buffer 解码
  （只有字符串 EDCode **编码**）；`652` 在 probe 里只作为 `wait_for` 的 ident 出现。
- 解封构造：给 probe 加「EDCode buffer 解码 + 从句柄取 ClientItem.MakeIndex」并新增 DSL 喂
  `CM_USERBUYITEM(1014, param/tag = Lo/Hi(MakeIndex))`——需改 `tools/probe/mir2_probe.py`（**另卡**）。
  **短中期不受阻**：W-TAX-1 的税路已由 `#23 castle_merchant_tax_potion`（药品腿，买价 88/税 4）覆盖。

### 2.3 B2 已证伪（与 castle 卡 F1 同一证据）

- `SaveTick` 段（`src/M2Server/Castle/UserCastle.cs:798-805`）函数体只有
  `SaveTick = GetTickCount()`，日志是注释 ⇒ **不触发任何写盘**，不构成「10 分钟门」。
- 写盘是 `CastleMgr.Save()`（`CastleManager.cs:150`），调用点 `SystemProcessor.ShowOnline()`
  （`src/GameSrv/Word/Threads/SystemProcessor.cs:54-62`），门是 `ConsoleShowUserCountTime`（缺省 600000，
  `GameSvrConf.cs:1439`），且被 `SystemProcessor.ExecuteInternal` **每 tick** 调用（`:49`）。
- 该门**可被 profile 覆盖**：`fixtures.config_overrides.Server.conf = {ConsoleShowUserCountTime: 5000}`
  （castle 卡已实测生效并还原）。
- 因此「只能真实时点复拍 600s」**不成立**；真正未解的是「覆盖到 5s 之后 `SabukW.txt` 仍零写入」——
  原因**未定位**（castle 卡 §4.2 已改口径；两行诊断 `已读取 [N] 个城堡信息` / `在线数: <N>` 待补）。
  ⇒ 本条 blocked 理由里的 ② 应改成「**税入账观测未定位**」，不能写成「SaveTick 门不可观测」。

### 2.4 B3 未复核（依据待补）

本轮未能定位「0151 走 EnvirList ⇒ `Castle` 恒非空」的可核来源：两侧 Envir 树里**没有** `EnvirList.txt`
（`find E:/tmp/conformance -name EnvirList.txt` 为空）；`MapInfo.txt:838` 只有 `[0151 …] FIGHT`（战斗地图），
不直接等价于「NPC 的 Castle 非空」。`MerChant.txt:83` 确有 0151 的武器商行。
**该句不影响 §2 的裁定**（买腿是否需要税覆盖与武器腿能否跑是两件事），但引用链需补齐或改引代码位置。

---

## 3. `#24 castle_merchant_tax_upgrade` —— 进批通道**已证伪**，真缺口是 NPC 选择

### 3.1 断言拆分

- **C1**「run3 内置 profile 不能声明 `config_overrides` ⇒ 本场景不能进批次」——**已证伪**。
- **C2**「其余能力齐，只差进批」——**部分成立**：进批后可跑之前，仍卡在 **按名字/坐标选 NPC**（工具缺口）。

### 3.2 C1 的证据 + 构造

- `run_batch.sh:409-421`：`cmd=("$PY" … --profile "$prof")`，随后 `if [ -n "$extra" ]; then … cmd+=("${ex[@]}")`
  ⇒ manifest 行的 `extra` **追加在 `--profile "$prof"` 之后**。
- `card_window.py:9796-9798`：`--profile` 的帮助写着「`alt` | `<profile.json 路径>`」；
  `load_profile`（`:857-872`）对 `os.path.isfile(name)` 走 JSON 路径分支。
- argparse 同一选项重复出现时**后者覆盖** ⇒ `extra: ["--profile", "<path>.json"]` 能让该行用自定义 profile。
- **前置约束（Leader 补充，2026-10-25）**：blocked 场景**整行不进批次**（`run_batch.sh:107-114`、`:152-158` 的 skip 分支），
  且 blocked 有**两个来源**（清单行的 `blocked` 管批次排除；场景 JSON 的 `blocked` 管直接跑 card_window，见 `:108-112`）
  ⇒ 只给 `extra` 加 `--profile` **不够**，必须**同时**清掉该行的 `blocked`（两处都清）才谈得上进批。
- 构造（需另卡改 run3，本卡按 ⑥ 不改）：
  1. 提交一个 profile JSON（dump `alt` + 加 `fixtures.config_overrides.Server.conf.CastleGetAllNpcTax=1`），
     建议放 `tools/window/profiles/castle_tax_upgrade.json`；
  2. `run3-scenarios.json` 的 #24 行：`"extra": ["--profile", "tools/window/profiles/castle_tax_upgrade.json"]`
     （与现有 `--timeout-ms` 用法同型）；
  3. `run_batch --list` 的 `extra` 列会显示它，`--dry-run` 打印的命令行同样带它（dry-run 分支 `:406` 同序拼接）。

### 3.3 C2 仍成立的部分

- `click_npc` 只接受 **actor id**（params 名解析）或 `auto:last_turn`（`mir2_probe.py:940-945`：`:940` `elif action == "click_npc"`、`:941` `auto:last_turn`、`:945` `resolve_param`；`refs` 收集器 `:1238-1240`），
  **不能**按名字/坐标点名；`--discovery-scenario` 只能换「发现用场景」，而发现结果仍取自
  「点一次 NPC → 643 → recog」这条固定逻辑。
- 实测记录（castle 卡 §4.2，实测）：map 3 在 (320,328)/(331,317) 都点到「装备回收」，`@upgrade` 无链接 ⇒ 643 超时。
- ⇒ 解封需要「按名字/坐标选 NPC」的 discovery 能力（工具卡）。
- 另有 castle 卡遗留：税写入**原因未定位**（§2.3）——即使进批、点对 NPC，税落盘仍不可判。

---

## 4. 交叉检查（acceptance ③、④）

- **本轮被推翻过的归因是否重演**：#22 的 ② 正是「落盘时机/定时器」式断言（`SaveTick`/600s 门），
  与 castle 税那次同类 ⇒ 已按 `UserCastle.cs` 与 `SystemProcessor.ShowOnline` **调用链**（而非只看常量）复核，
  裁定**已证伪**（§2.3）。台账/exec sheet 的 W-TAX-4 措辞应同步为「原因未定位」，不要留「SaveTick 门」说法。
- **MonGen 新事实的影响面**：blocked 三场景里只有 #21 用怪物夹具；但**任何**未来声明 `fixtures.monsters`
  的 profile 都受 §1 限制。建议把「前置插入」写进 `fixtures.monsters` 契约（tester P0 prepend 卡），
  并在夹具日志里打印「新行位于第 1 行 / 第 N 行 + 预计首次评估延迟」。
- 顺带记录：#22 的 ② 与 #24 的 C2 都落到「castle 税为何不落盘」这同一个未定位观测 —— 它现在是**一条**
  待办（两行诊断），不是两条。

## 5. 本次核过但未改的项

| 项 | 说明 |
|---|---|
| 场景 JSON / run3 清单 | 按卡面 ⑥ 一律不改；§1.3、§3.2 的构造写成「另卡执行」 |
| `fixtures.monsters` 实现 | 未改（属 tester P0 prepend 卡）；本卡只给证据链与影响面 |
| `SaveTick`/`ShowOnline` 调用链 | 只读复核，未改生产代码 |
| 0151 `Castle` 恒非空（B3） | **未复核通过**：两侧无 `EnvirList.txt`；需补引用或改引代码位置（清单见 §2.4） |
| `merchant_actor` 的 actor id 稳定性 | 未验证「跨起栈 id 稳定」的假设；不据此给构造，避免脆弱解 |

## 6. 残留物清单

- 本文件（唯一改动）。
- 临时物：无（只读 `grep/sed/stat` + `run_batch --list`；未起栈、未占端口、未写任何 Envir/DB 文件）。
- 未 push（walgit 故障期本地模式）。
