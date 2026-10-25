# mir_flow 四类玩家动作阶段（捡物 / 买卖 / NPC 对话 / 有目标战斗）

> 卡：`openmir2-mirflow-combat-pickup-stages`（tier **L**，分支 `card/mirflow-stages`）。
> 本文是**实现与离线性证据**说明，不是真机验收报告。哪些必须在真机上验，见 §7。
> 本轮不起服务端、不占端口、不碰 `E:/MirServer`、不动 mysqld；真机验证是下一步独立窗口。

## 1. 交付概要

| 项 | 内容 |
|---|---|
| 驱动脚本 | `tools/client/mir_flow.ps1`（只**追加**；既有 25 阶段名与相对顺序不变） |
| 新增阶段名 | `pickup` / `npc_dialog` / `trade` / `combat_target` / `combat`（写在 `stages.ndjson`） |
| 开关 | `-PickupStage` / `-NpcDialogStage` / `-TradeStage` / `-CombatStage` |
| 证据产物 | `<OutDir>/flow-stages-evidence.json`（schema `openmir2.mirflow-stages/1`） |
| 抓包指纹 | `tools/capture/clienthook/Sha256.pas` + `Mir2ClientHook.dpr`（data 事件写**真实** sha256） |
| 指纹校验 | `tools/capture/clienthook/verify_chunks.ps1`（逐 data 事件复算） |
| 离线自检 | `tools/client/mir_flow.selftest.ps1`（新增 14a–14f）、`tools/capture/clienthook/selftest_verify_chunks.ps1`、`selftest_sha256.ps1` |

**判据一律落在可观察副作用**，不用「某帧出现」当成功：

| 类别 | 前置（A3 牙删掉它就 RED） | 成功判据（副作用） |
|---|---|---|
| 捡物 `pickup` | 客户端发 `CM_PICKUP(1001)`，且 header `param/tag` == 角色格（服务端 oracle 会直接忽略别格） | 动作后 `SM_ADDITEM(200)` ⇒ 背包行数**增加**；或 `SM_GOLDCHANGED(653)` ⇒ 金币变化 |
| NPC 对话 `npcdialog` | 客户端发 `CM_CLICKNPC(1010)` | 点击后出现 **body 非空** 的 `SM_MERCHANTSAY(643)`（脚本说词）或 `SM_MENU_OK(767)`（菜单文本） |
| 买卖 `trade` | 交易请求前 `dialogMaxAge` 内先有商人对话：`CM_CLICKNPC(1010)`/`CM_MERCHANTDLGSELECT(1011)` + 643/767 body 非空 | 成功帧（买 650 / 卖 648）+ 金币按方向变化（买减 / 卖增） |
| 有目标战斗 `combat` | profile `fixtures.monsters` 有**同图**靶子；`CM_HIT(3014)` 的 `Dir` 使「前方一格」正好是靶子格；距离 ≤ AP 范围 | `CM_HIT` 之后**非自己**的 actor 出现 `SM_STRUCK(31)`（HP 下降/伤害>0）或 `SM_DEATH(32)` |

四类**各自独立**：每类的 RED 都由自己的前置/副作用判定，不共用一颗牙。

## 2. 用法

真机（下一轮窗口）最小示例：

```powershell
# 前置：proxy 已在跑（-ProxyDir 指向 mir2_proxy --out），客户端用新 hook DLL 注入
powershell -ExecutionPolicy Bypass -File tools/client/mir_flow.ps1 `
  -OutDir E:\tmp\flow-stages -ProxyDir E:\tmp\proxy -ExpectPos "330,331" `
  -PickupStage     -PickupPixel "512,304" `
  -NpcDialogStage  -NpcDialogPixel "512,263" `
  -TradeStage      -TradeNpcPixel "512,263" -TradeMenuPixel "600,300" -TradeKind buy `
  -CombatStage     -Profile tools/window/profiles/butch_monsters.json -PlayerMap "0" `
  -CombatTargetPixel "560,304" -ActionWaitSec 8
```

离线复算（不启动客户端；牙就是靠它重放）：

```powershell
powershell -ExecutionPolicy Bypass -File tools/client/mir_flow.ps1 `
  -StageEvidenceCheck -ProxyDir <hook 产物目录> -OutDir <临时目录> `
  -PickupStage -NpcDialogStage -TradeStage `
  -CombatStage -Profile <profile.json> -PlayerMap "0"
```

参数（新增）：

| 参数 | 默认 | 说明 |
|---|---|---|
| `-PickupPixel "x,y"` | 空 | 脚下物品的**屏幕像素**（服务端只处理角色格上的物品） |
| `-NpcDialogPixel "x,y"` | 空 | NPC 屏幕像素；留空回落到 `-NpcClick` |
| `-TradeNpcPixel` / `-TradeMenuPixel` | 空 | 先点商人 NPC，再点物品/出售菜单项 |
| `-TradeKind buy\|sell` | buy | 期望的交易方向（请求 ident 与方向不符 ⇒ RED） |
| `-Profile` | 空 | card-window profile（读 `fixtures.monsters`） |
| `-PlayerMap` | 空 | 角色所在图；**必须显式给**（经典协议线上没有图名，无法自行证明同图） |
| `-AttackRangeCells` | 1 | AP/近战范围（切比雪夫格数） |
| `-CombatTargetPixel "x,y"` | 空 | 靶子屏幕像素；留空用格心投影 `(512,304)+(dx*48,dy*32)`（该步长**未被证实**，见 §7） |
| `-ActionWaitSec` | 8 | 动作后等服务端副作用的秒数 |
| `-TradeDialogMaxAgeSec` | 60 | 商人对话必须在窗口内打开 |
| `-LogDir` | 空 | 服务端日志目录（可选；脚本内**不硬编码**任何日志路径） |
| `-RequireLogEvidence` | 关 | `-LogDir` 为空/无 `*.log` 时 RED（否则显式 SKIP 并说明） |
| `-EvidenceOut` | `<OutDir>/flow-stages-evidence.json` | 证据文件 |
| `-StageEvidenceCheck` | 关 | 离线：只评估证据，不启动客户端 |

约束（fail-closed）：四类阶段都要求 **`-ProxyDir`（live hook dump）**；`capture.jsonl` 不含编码后 body 长度，判不了 643/767 的「脚本确实说了话」。`-CombatStage` 要求 `-Profile` 与 `-PlayerMap`。

## 3. Oracle 锚点（ident 与判据来源）

复现（在仓库根；`<rev>` 用交付 head）：

```powershell
git rev-parse HEAD
grep -n "CM_PICKUP\|CM_CLICKNPC\|CM_MERCHANTDLGSELECT\|CM_USERSELLITEM\|CM_USERBUYITEM\|SM_ADDITEM\|SM_BAGITEMS\|SM_GOLDCHANGED\|SM_MERCHANTSAY\|SM_MENU_OK\|SM_STRUCK\|SM_DEATH\|SM_LOGON\|SM_ABILITY\|SM_BUYITEM_SUCCESS\|SM_USERSELLITEM_OK" src/OpenMir2/Messages.cs
grep -n "case Messages.CM_PICKUP" -A 4 src/M2Server/Player/PlayObject.Message.cs
grep -n "case Messages.CM_HIT" -A 2 src/M2Server/Player/PlayObject.Message.cs
grep -n "nX == CurrX" src/M2Server/Player/PlayObject.Attack.cs
grep -n "GetFrontPosition\|GetPoseCreate" src/M2Server/Actor/BaseObject.cs
```

### 3.1 捡物

- `CM_PICKUP = 1001`：`src/OpenMir2/Messages.cs:46`。
- 分发：`src/GameSrv/Word/WorldServer.cs:768 ProcessUserMessage`；`CM_PICKUP` 走 default ⇒ `SendMsg(ident, Series, Recog, Param, Tag)`，即线上 header `param=ownX, tag=ownY`。
- 处理：`src/M2Server/Player/PlayObject.Message.cs:811`——**只有 `CurrX==nParam2 && CurrY==nParam3` 才继续**（`:812`），否则静默 return。
- 成功副作用：`ClientPickUpItem()`（`PlayObject.Base.cs:55`）→ `AddItemToBag` + `SendAddItem`（`:117`）→ `SM_ADDITEM(200)`；金币则 `GoldChanged()` → `SM_GOLDCHANGED(653)`。
- 背包行数口径：`SM_BAGITEMS(201).series = ItemList.Count`（`PlayObject.Operate.cs:61`）；`SM_ADDITEM.series=1`（`PlayObject.Base.cs:226`）。

### 3.2 买卖

- ident：`CM_USERSELLITEM=1013`、`CM_USERBUYITEM=1014`（`Messages.cs:65/66`）；成功帧 `SM_USERSELLITEM_OK=648`、`SM_BUYITEM_SUCCESS=650`（`:310/312`）；金币 `SM_GOLDCHANGED=653`（`:315`）。
- 商人菜单：`CM_MERCHANTDLGSELECT=1011`（`Messages.cs:63`）→ `PlayObject.Message.cs:889` → `ClientMerchantDlgSelect`（`PlayObject.Operate.cs:114`）；买/卖处理在 `Operate.cs:192/162`。
- 金币：`SM_GOLDCHANGED` 的 `recog = Gold`（`PlayObject.Message.cs:1834`）；登录基线另在 `SM_ABILITY(52)` 的 `recog = Gold`（同文件 RM_ABILITY 支；Rust 侧 `bins/game-svr/src/enter.rs:118/787` 同口径）——实测冒烟产物里只有 52、没有 653，所以判据两个来源都收。
- **767 口径**：`SM_MENU_OK(767)` 成功/失败共用，**不能当成功帧**；这里只把「767 且 body 非空」当**对话内容**（脚本给了菜单），成功仍看 650/648 + 金币变化。

### 3.3 NPC 对话

- `CM_CLICKNPC=1010`（`Messages.cs:62`）→ `PlayObject.Message.cs:885` → `ClientClickNpc`（`PlayObject.Base.cs:674`）；距离/冷却门后 `normNpc.Click(this)`（`:693`）跑 Envir 脚本。
- 脚本说词：`RM_MERCHANTSAY` → `SM_MERCHANTSAY(643)`（`PlayObject.Message.cs:1724`，`param=MakeWord(...)`，**body 是脚本文本**）。
- 菜单：`RM_MENU_OK` → `SM_MENU_OK(767)`（`:1952-1953`，`body=processMsg.Msg`）。
- 因此判据是「**点击之后 + body 非空**」——空回帧不算对话。

### 3.4 有目标战斗

- `CM_HIT=3014`（`Messages.cs`）；分发特例：`WorldServer.cs:835-847` 把 `Tag→wParam（方向）`、`Recog→nParam1/2（坐标）`。
- 处理：`PlayObject.Message.cs:1221` → `ClientHitXY`（`PlayObject.Attack.cs:823`）；**服务端要求 `nX == CurrX && nY == CurrY`**（`:883`）⇒ 线上 `Recog = MakeLong(CurrX,CurrY)`，`Tag = Dir`。
- 目标解析：`GetPoseCreate()`（`BaseObject.cs:1253`）用 `GetFrontPosition`（`:977`）取**前方一格**；方向 0=Up..7=UpLeft（`src/OpenMir2/Enums/Direction.cs`）。
- 命中副作用：受害者的 `RM_STRUCK` → `SM_STRUCK(31)`：`recog=受击者, param=HP(之后), tag=MaxHP, series=本次伤害`（`PlayObject.Message.cs:1601`）；死亡 `SM_DEATH(32)`：`recog=死亡者`（`:1644`）。
- 自身 ActorId：`SM_LOGON(50).recog = ActorId`（`PlayObject.cs:1131`）——用来把「自己被打」排除在「怪物掉血」之外。

## 4. 证据产物

`<OutDir>/flow-stages-evidence.json`（每次阶段/离线评估后重写；UTF-8 无 BOM）：

```json
{
  "schema": "openmir2.mirflow-stages/1",
  "mode": "live | evidence-check",
  "evidence_source": { "kind": "proxy", "path": "...", "since_ms": 0 },
  "profile": { "path": "...", "player_map": "0", "range_cells": 1 },
  "log_evidence": { "status": "SKIP|GREEN|RED", "files_scanned": 0, "lines_scanned": 0, "err_lines": 0, "reason": "..." },
  "categories": {
    "pickup":    { "verdict": "GREEN|RED", "before": { "bag_rows": 5, "gold": 1000 },
                   "after": { "bag_rows": 6, "gold": 1000 }, "effect": { "kind": "bag_add", "...": "..." }, "reason": "..." },
    "npcdialog": { "click": { "ident": 1010, "ts_ms": 2000 },
                   "dialogue": { "say_frames": 1, "menu_frames": 0, "max_body_len": 60, "first_ts_ms": 2100 } },
    "trade":     { "kind": "buy", "request": { "ident": 1014 }, "dialogue": { "opens": 1, "dialogue_frames": 1 },
                   "success": { "ident": 650 }, "before": { "gold": 1000 }, "after": { "gold": 900 }, "gold_delta": -100 },
    "combat":    { "target": { "map": "0", "x": 331, "y": 331, "name": "鸡", "count": 4, "distance": 1 },
                   "hit": { "own_x": 330, "own_y": 331, "dir": 2, "front_x": 331, "front_y": 331 },
                   "effect": "hp", "hp_before": 10, "hp_after": 8, "max_hp": 10, "damage": 2, "actor": 8888 }
  }
}
```

复算：`evidence_source.path` 指向的 hook 目录（`chunks.ndjson` + `conn-*.bin`）+ `-Profile`/`-PlayerMap` 就是全部输入；
用同一条 `-StageEvidenceCheck` 命令重跑即得到同一份判定（无隐藏状态、无客户端）。

## 5. 牙（A3）与原始输出

全部牙用 **$env:TEMP 假根**（`New-StageProxyDir` 造 `chunks.ndjson` + `conn-*.bin`），不写任何生产/Capture 目录。
重放：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools/client/mir_flow.selftest.ps1
# => SELFTEST_PASS=113 SELFTEST_FAIL=0 ; SELFTEST_OK
```

四类牙的原始输出（删掉前置/副作用 ⇒ 对应类别 RED 且点名）：

| 牙 | 构造 | 原始输出（节选） |
|---|---|---|
| 捡物 | 有 `CM_PICKUP`，删掉 `SM_ADDITEM`/金币变化 | `STAGE_EVIDENCE category=pickup verdict=RED reason=取物后无入包（SM_ADDITEM 200）也无金币变化（SM_GOLDCHANGED 653）` |
| 捡物② | `CM_PICKUP` 目标格 ≠ 角色格 | `... category=pickup verdict=RED ... != 角色格（服务端 oracle 会直接忽略）` |
| NPC 对话 | 有 643 对话帧，删掉 `CM_CLICKNPC` | `STAGE_EVIDENCE category=npcdialog verdict=RED reason=未发出 c2s CM_CLICKNPC(1010)（点 NPC 前置动作缺失）` |
| 买卖 | 有 1014+650+金币变化，删掉商人对话 | `STAGE_EVIDENCE category=trade verdict=RED reason=未先打开商人对话（前置缺失：不点 NPC 直接交易 / 无脚本说词帧）` |
| 买卖② | 对话+650 都在，金币未变 | `... category=trade verdict=RED ... 金币未减少` |
| 战斗 | profile 靶子在别的图（=没刷出） | `STAGE_EVIDENCE category=combat verdict=RED reason=profile 在 map=0 没有靶子（刷不出即 RED，不静默跳过）` |
| 战斗② | `CM_HIT` 方向没指向靶子 | `... category=combat verdict=RED ... CM_HIT 朝向 dir=6 的前方格 (329,331) 与同图靶子不匹配（攻击未指向靶子）` |
| 战斗③ | 打向靶子格但无任何掉血/死亡 | `... category=combat verdict=RED ... 无掉血(SM_STRUCK 31)也无死亡(SM_DEATH 32)（靶子可能没刷出）` |

对应 GREEN 反例（同一假根 + 补齐前置/副作用）：
`category=pickup verdict=GREEN reason=取物副作用成立`、`npcdialog ... GREEN`、`trade ... GREEN reason=商人对白前置成立 + 成功帧 + 金币按方向变化`、`combat ... GREEN reason=靶子同图且在 AP 内 + 攻击指向靶子 + 目标掉血/死亡`。

## 6. A4 / A5 / A6

### A4：chunks.ndjson 真实 sha256 + 篡改牙

- `Sha256.pas`：纯 Pascal SHA-256（FIPS 180-4），不依赖 CryptoAPI；`Mir2ClientHook.dpr` 的 `LogData` 对**本次 send/recv 的 n 字节**计算并写入 `sha256`。
- 自检：`selftest_sha256.ps1` —— FIPS 向量（空串/abc/56B）+ 与 .NET 交叉（1000×'a'、0..255×4 二进制）⇒ `SHA256_SELFTEST_PASS=7 FAIL=0`。
- 校验：`verify_chunks.ps1 -CaptureDir <dir>` 逐 data 事件复算；**空串 / 越界 / 不符 / 0 条事件**都是 RED。
- 牙（A4）：`selftest_verify_chunks.ps1` 原始输出：
  - 正确：`CHUNKS_VERIFY GREEN events=1 bins=1 bytes=8`
  - 篡改 1 字节：`CHUNKS_VERIFY RED line=1 conn=1 dir=s2c off=0 n=8 指纹不符：记录=9ac2197d9258257b… 实际=c456027337364ca9…`（rc=1）
  - 空 sha（旧 DLL）：`CHUNKS_VERIFY RED ... sha256 为空串（旧产物/DLL 未计算）`
  - 空扫描：`CHUNKS_VERIFY RED data 事件 0 条（扫了 0 个对象 —— 不得当绿）`
- **旧行为 RED 实测**：对 2026-10-25 真客户端冒烟产物 `E:/tmp/smoke/hook` 跑校验 ⇒ 全部 data 事件 `sha256 为空串`，rc=1（正是本轮要修的假绿）。

### A5：既有 25 阶段不变

`mir_flow.selftest.ps1` 静态断言：`start → inject_hook → … → ingame → talk → talk_sent → attack_N → walk_N → logout_button → … → done` 作为**子序列**存在且严格有序；新阶段 `pickup/npc_dialog/trade/combat_target/combat` 的位置都在 `attack` 之后、`walk` 之前（不插入/不重命名任何既有阶段）。

### A6：日志目录参数化

- `mir_flow.ps1` 内**没有** `E:/MirServer/logs` / `E:\MirServer\logs` 字面量（selftest 有源码断言）。
- `-LogDir` 语义：
  - 未给 ⇒ `LOG_EVIDENCE status=SKIP reason=-LogDir 未提供（…不硬编码日志路径…）`；
  - 给了空目录 / 不存在 ⇒ 默认 `status=SKIP` 且 reason 点名「空目录：0 个 *.log（SKIP）」；加 `-RequireLogEvidence` ⇒ `status=RED` 且整体 rc=1（`扫了 0 个对象，不得当绿`）；
  - 有日志 ⇒ 扫 `[ERR]`/`panic`，同时报 `files_scanned/lines_scanned`（防「扫了 0 个」假绿）。

## 7. ⚠ 未验证、必须真机才能确认的清单

**本轮没有起任何服务端/客户端；以下全部未实测。** 不能把本文当验收证据。

1. **新 hook DLL 端到端**：`Sha256.pas` 已单测 + `build.ps1` 已编译出 `Mir2ClientHook.dll`，但**未注入真客户端跑过**。真机首跑必须：data 事件 `sha256` 非空 + `verify_chunks.ps1` GREEN。
2. **客户端动作本身**：
   - 点脚下物品是否真发 `CM_PICKUP`（且 `param/tag` = 角色格）；
   - 点商人 → 菜单 → 物品是否真发 `CM_CLICKNPC` / `CM_MERCHANTDLGSELECT` / `CM_USERBUYITEM|SELLITEM`；
   - 右键转向 + 左键是否真发 `CM_HIT`（冒烟产物里**没有** 3014，三次点击落成了 3011/3013）；
   - 点 NPC 是否真打开脚本对话（643/767 且 body 非空）。
3. **像素标定**：`-PickupPixel` / `-TradeNpcPixel` / `-TradeMenuPixel` / `-NpcDialogPixel` / `-CombatTargetPixel` 全需按实际 1024×768 窗口标定；默认格心投影「512,304 + dx*48/dy*32」的 48×32 步长**未被证实**（`docs/window-shrink-run-20261019.md` §F2 有反例）。
4. **同图证明**：经典协议线上没有图名；`-PlayerMap` 是**操作者断言**，不是机判。真机必须确认角色与 `fixtures.monsters` 的 map 一致（否则靶子在同格也打不到）。
5. **刷怪是否真的刷出**：判据的最终防线是 `SM_STRUCK`/`SM_DEATH`；但「fixture 写了、服务器真刷了、玩家同屏」三者只能靠真机组合验证。若靶子没刷出，表现是 `combat RED：无掉血/死亡`。
6. **服务端副作用形态**：`SM_ADDITEM/SM_GOLDCHANGED/SM_STRUCK/SM_DEATH` 的 header 口径已按 oracle 源码核对，但**未在 Rust 栈上实跑确认**（尤其是 `SM_ABILITY(52).recog=Gold` 的登录基线）。
7. **交易成功条件**：买需要角色有足够金币；卖需要背包有可卖物品。冒烟角色金币为 0（`SM_ABILITY recog=0`），真机脚本要先备好金币/物品。
8. **地板物品来源**：服务端只处理**角色格**上的物品。当前 profile 没有「地板物品」夹具；真机需先用怪物掉落/GM 放置，并让角色站上去。
9. **NPC 脚本内容**：比奇国王等 NPC 必须真的发 643（#SAY）或 767（菜单，body 非空）；空回帧会 RED（这是故意的）。
10. **时序参数**：`-ActionWaitSec=8`、`-TradeDialogMaxAgeSec=60`、右键→左键 400ms 都是保守值，未按真机延迟调过。
11. **多靶子/多怪格**：选靶是「AP 内最近的一个 fixture 格」；同格多只（count>1）只按格匹配，不区分具体 actor。
12. **阶段中止语义**：任一阶段 RED 会 `throw` 中止后续流程（包括既有小退/重进）；真机 runbook 需要把四类阶段放在合适顺序，或分次跑。

## 8. 已知限制（不是 bug，但要知道）

- `capture.jsonl` 没有编码后 body 长度 ⇒ 对话/交易前置判据只支持 `-ProxyDir`；live 四类阶段已 fail-closed 强制 `-ProxyDir`。
- `SM_MENU_OK(767)` 只当「对话内容帧」（body 非空），**不当成功帧**；买卖成功看 650/648 + 金币方向。
- 战斗的「同图」靠 `-PlayerMap` 显式声明；协议上没有图名，这是硬约束。
- 金币基线同时接受 `SM_ABILITY(52)` 与 `SM_GOLDCHANGED(653)` 的 `recog`（都是绝对值）；若一局里没有任何一条，`gold_before/after` 为 `null`，交易判 RED。

## 9. 复现命令汇总

```powershell
# 1) mir_flow 全套离线自检（含四类牙、A5/A6）
powershell -NoProfile -ExecutionPolicy Bypass -File tools/client/mir_flow.selftest.ps1
#    => SELFTEST_PASS=113 SELFTEST_FAIL=0 / SELFTEST_OK

# 2) SHA-256 构造级自检（编译 Sha256.pas + FIPS/.NET 交叉）
powershell -NoProfile -ExecutionPolicy Bypass -File tools/capture/clienthook/selftest_sha256.ps1
#    => SHA256_SELFTEST_PASS=7 FAIL=0 / SHA256_SELFTEST_OK

# 3) chunks 指纹校验的牙（含 1 字节篡改）
powershell -NoProfile -ExecutionPolicy Bypass -File tools/capture/clienthook/selftest_verify_chunks.ps1
#    => VERIFY_CHUNKS_SELFTEST_PASS=6 FAIL=0 / VERIFY_CHUNKS_SELFTEST_OK

# 4) 对真实（旧）hook 产物验证「空 sha 会 RED」
powershell -NoProfile -ExecutionPolicy Bypass -File tools/capture/clienthook/verify_chunks.ps1 -CaptureDir E:\tmp\smoke\hook
#    => CHUNKS_VERIFY RED ... sha256 为空串（旧产物/DLL 未计算） ; rc=1

# 5) 编译 hook DLL（产物在 tools/capture/clienthook/bin/，gitignore）
powershell -NoProfile -ExecutionPolicy Bypass -File tools/capture/clienthook/build.ps1
```

## 10. 本批残留物

- `tools/capture/clienthook/bin/`（`Mir2ClientHook.dll` / `Inject.exe` / `*.build.log`）：`build.ps1` 产物，**已在 `.gitignore`**；本批未删除（下一轮真机注入直接用）。
- 无临时目录、无 worktree、无服务端进程；未改 `tools/e2e/run_e2e.ps1`、`mir2-rs/**`、`src/**`、`sql/`、`Envir/`。
