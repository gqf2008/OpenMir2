# 比奇国王「不渲染 / 点不到」诊断（2026-10-22）

> 只读诊断；本卡写集只有这一份文档。基线 `940264798`，分支 `npc-render-diagnosis`。
> 未起任何服务端/客户端/代理，未占端口，未写库。

## 0. 结论（TL;DR）

| 问题 | 判定 |
| --- | --- |
| 收到 `SM_TURN(10)` / `SM_FEATURECHANGED(41)` 后客户端没建/没画 actor？ | **否**。两端源码都是「`SM_TURN` 建 actor」，`41` 只更新既有 actor；抓包帧与 C# oracle 同形，客户端**确实把国王画了出来**。 |
| 服务器缺帧/顺序错？ | **否**。r5 与 r3(C# oracle) 的 NPC 帧序列逐条同形，同一台客户端在两套栈上都渲染出国王。 |
| 画面里为什么看不到国王？ | 截图时点（`05c_ingame_ready`）角色已被**测试脚本自己**从 `(29,33)` 一路向南走到 `(29,44)`，国王 `(29,32)` 在 12 格外、超出视窗；而进图瞬间（`05_ingame`）国王是**可见的品红色剪影**（oracle 的 P2 体分块缺陷，两端同款）。 |
| 为什么点 `(512,263)/(512,227)` 变成 `CM_WALK(3011)`？ | 点击发生时角色已在 `(29,44)`，这两个屏幕点换算到的格子是 `(29,43)`/`(29,42)`，都是空地 ⇒ 走位。C# oracle 栈上**同样**（r3 抓包同款 11 条 `CM_WALK`）。 |
| `皇宫 29:44` vs 服务器 `(29,33)` 是坐标基不同吗？ | **否**。`29:44` 是客户端 `g_MySelf` 的真实当前坐标（真的走了 11 格），下一次登录服务器日志也是 `(29,44)`。`(29,33)` 只是**首次进图**那条日志。 |
| 本轮跑的客户端是 MirClientZig 吗？ | **不是**。`E:/tmp/window/r5/client/MirClinet.exe` 是冻结 Delphi 客户端（sha256 `c3cb8f73…`，与 `D:/MirClient-run/MirClinet.exe` 同哈希），不是 `MirClientZig.exe`（`0c6cc672…`）。 |

`root_cause_layer = fixture`（测试夹具/客户端身份），叠加一个**已登记并有意复刻**的客户端渲染缺陷（品红/乱码名）。

---

## 1. 现象原文与需要核对的前提

报障内容：

1. 服务器对 `比奇国王` 发了 `SM_TURN(10) recog=132 param=29 tag=32 series=0x0206/0x0203` 与
   `SM_FEATURECHANGED(41) recog=132 param=50 tag=8 series=0`（抓包 `E:/tmp/window/r5/s2c/capture.jsonl`）。
2. 客户端画面里没有国王精灵；点 `(512,263)/(512,227)` 变成 `CM_WALK(3011)` 而不是 `CM_CLICKNPC(1010)`。
3. 状态栏 `皇宫 29 44`，服务器日志 `CurMap:0122 (29,33)`。

上述三条**现象本身都成立**，但归因链里有两个前提不成立：客户端身份、以及「画面里没有国王」的取样时刻。下面逐条给证据。

---

## 2. 证据

### 2.1 帧序列：Rust 与 C# oracle 同形

r5（Rust game-svr，`E:/tmp/window/r5/s2c/capture.jsonl`）：

| seq | dir | ident | recog | param | tag | series | 含义 |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 4 | s2c | 51 `SM_NEWMAP` | 268 | 29 | 33 | 1 | 进图 `0122 (29,33)` |
| 5 | s2c | 50 `SM_LOGON` | 268 | 29 | 33 | 0 | 自身 actor=268 |
| 6 | s2c | 41 | 268 | 0 | 2 | 0 | 自身外观 |
| 24 | s2c | **10 `SM_TURN`** | **132** | **29** | **32** | **518 (0x0206)** | 国王落点/朝向 dir=6、光=2 |
| 25 | s2c | **41 `SM_FEATURECHANGED`** | **132** | **50** | **8** | 0 | RaceImg=50(Appr=8) |
| 60/61、62/63 | s2c | 10 / 41 | 132 | 29/50 | 32/8 | 518/515 | 周期重发（≈1.7s 一次） |

r3（C# oracle，`E:/tmp/window/20261021-r3/cking/s2c/capture.jsonl`）：

| seq | ident | recog | param | tag | series |
| --- | --- | --- | --- | --- | --- |
| 22 | **10** | **31475780** | **29** | **32** | **513 (0x0201)** |
| 23 | **41** | **31475780** | **50** | **8** | 0 |
| 37/38、45/46、68/69、83/84 | 10 / 41 | 同 | 29/50 | 32/8 | 515/517/519 |

**同形**：都是「`10`(X,Y,dir+light) 建 actor → `41`(Feature=Makelong(50,8)) 改外观」，且都周期重发。
差异只有两类，都与「建 actor」无关：

1. `recog` 数值不同（`132` vs `31475780`）——actor 句柄由服务器自选，客户端只做 `FindActor` 相等比较；
2. `series` 低字节（朝向）不同（6 vs 1）——都是合法朝向；高字节同为 2（光圈）。

另有一条 **Rust 侧多出来的帧**（C# oracle 没有）：r5 `seq 40 / 53 / 59` 的 `ident=14 SM_HIT recog=132 param=29 tag=32 series=6`。
`SM_HIT` 只更新既有 actor 的动作/坐标，**不建 actor**、也不会隐藏它，故不是本症状的因；但它是一条与 oracle 的偏差，见 §6.3。

### 2.2 客户端把国王画出来了（决定性截图）

进图后、公告点掉前（`E:/tmp/window/r5/flow4/05_ingame.png`，13:10，状态栏 `皇宫 29:35`）：

- 玩家北侧一格 `(29,32)` 处有一个**品红色（fuchsia）剪影**，头顶有蓝血条与**乱码绿名**（`谍绿?1?`），
  正是 `RaceImg=50` 的商人精灵。

同一场景用 **C# oracle** 服务端跑出来（`E:/tmp/window/20261021-r3/cking/flow/05_ingame.png`，状态栏 `皇宫 29:34`）：
**同一个品红剪影、同一串乱码绿名、同一位置**。

⇒ 「国王精灵」并不缺失。它是品红色的、名字是乱码；成因是 oracle 的
`SM_TURN` 体分块缺陷（`ClMain.pas:9748-9761` 按 16 个**编码字符**切体，服务端只发
`encodedLen(8)=11` 个 ⇒ `desc.Status` 高 16 位被名字首字符污染 ⇒
`TActor.GetDrawEffectValue`(`Actor.pas:2609-2636`) 在 `:2626` 返回 `ceFuchsia`；名字从第 17 字符起解 ⇒ 乱码）。
MirClientZig 在 `src/game/world.zig:41-46` 明确登记为「P2，裁决 = 复刻」，
判据在 `src/game/world.zig:4783`（抓包固定向量）。

**「没有国王」的取样时刻**是 `05c_ingame_ready.png`（13:10，状态栏 `皇宫 29:44`）——
那时角色已经走了 11 格，国王在 12 格之外（纵向可见约 ±9 格），确实不在画面里。

### 2.3 是谁把角色走掉的：脚本自己的「公告轮询点击」

`tools/client/mir_flow.ps1:912-919`：

```powershell
Stage "notice_poll"
for ($i = 0; $i -lt 12; $i++) {
  Send-Click 522 525 1        # 公告确定按钮的位置
  Start-Sleep -Milliseconds 1200
  ...
}
```

`Send-Click` 是**向窗口 PostMessage 客户端坐标**（`mir_flow.ps1:742-754`）。公告
`SM_SENDNOTICE(658)` 在 `r5 seq 3` 到达，第一次点击就把它点掉了（`seq 22 c2s ident=1018 CM_LOGINNOTICEOK`，+0.6s）。
**之后 11 次点击全部落在世界区**，每次都把 `g_nTargetX/Y` 重设到鼠标格，角色朝目标自动走位。

`TPlayScene.CXYfromMouseXY`（`PlayScn.pas:2951-2958`）：

```
ccy := Round((my - ShiftYOffset + m_nShiftY) / UNITY) + m_nRy;
```

`ShiftYOffset`（`MShare.pas:4618-4621`）在 768 高、`UNITY=32` 下约为 304，
`my=525` ⇒ `round((525-304)/32) = +7`，即点击格在角色**南侧约 7 格**。

抓包实测（r5，c2s，`recog = X | (Y<<16)`，`tag = 朝向`）：

```
seq 34 3011 recog 2228253 -> (29,34) tag=4
seq 36 3011 recog 2293789 -> (29,35) tag=4
...
seq 56 3011 recog 2883613 -> (29,44) tag=4
```

共 **11 条 `CM_WALK`，从 `(29,34)` 递增到 `(29,44)`，朝向恒为 4（南）**，时间跨度 +80.4s→+97.9s，
与 12 次公告点击的窗口完全重合。C# oracle 的 r3 抓包是**一模一样**的 11 条
（`seq 33..58`，recog `2228253…2883613`）⇒ 这是夹具行为，不是本仓服务端的差异。

服务器日志坐实了「真的走了」并落库：

```
05:09:55.401 [INF] 进世界 ChrName:winplay0 CurMap:0122 (29,33) Dir:0 entity=268   ← 报障引用的那条
05:10:44.312 [INF] 存档请求已发（DB_SAVEHUMANRCD）ChrName:winplay0 ... 记录 4852 字节
05:11:44.050 [INF] 进世界 ChrName:winplay0 CurMap:0122 (29,44) Dir:4 entity=3936   ← 第二轮登录，位置已是 44
```

⇒ 状态栏 `皇宫 29:44` 与服务器并不矛盾；矛盾只在于把**进图日志**当成了**实时位置**。

### 2.4 「点 (512,263)/(512,227) 变走位」同理

`CXYfromMouseXY`：`ccx := Round((mx - SCREENWIDTH div 2 + shiftX)/UNITX) + m_nRx`，
`SCREENWIDTH=1024` ⇒ `mx=512` 恰好是自身列 `29`。纵向：

- 自身 `(29,44)` 时，`(512,263)` ⇒ `round((263-304)/32) = -1` ⇒ 格 `(29,43)`；
- `(512,227)` ⇒ `round((227-304)/32) = -2` ⇒ 格 `(29,42)`。

两格都是空地。`TActor.GetCharacter` 只在**点击格 `ccy-1 … ccy+8`** 这一列里找 actor
（`PlayScn.pas:2968-2975`），国王在 `y=32`，不在扫描窗口内 ⇒ `target = nil` ⇒
`caWalk`（`ClMain.pas:5710-5722`）。所以发出去的是 `CM_WALK`，不是 `CM_CLICKNPC`。
**这套几何在 C# oracle 栈上完全相同**（r3 的走位序列一致），不是本仓的问题。

> 注：runbook 里把 `(512,227)` 记作「北侧一格」是按角色在 `(29,33)` 时的换算写的；
> 角色一旦走到 44，这个坐标就落到空地上。`(512,263)` 在 `(29,33)` 时才是正对国王那格
> （`round((263-304)/32) = -1` ⇒ `y=32`）。

### 2.5 本轮跑的客户端不是 MirClientZig

| 项 | 值 |
| --- | --- |
| `E:/tmp/window/r5/client/MirClinet.exe` | 6,561,280 B，mtime `2026-10-10 00:55`，sha256 `c3cb8f73d8872c5ad3e53f0f55a96d03139fb49397ebcacb4b3573c3e1c07996` |
| `D:/MirClient-run/MirClinet.exe` | **同 sha256**（同一份） |
| 该二进制特征 | 含 `Embarcadero` / `TfrmMain`(×6)，**不含任何 `zig` 字样** ⇒ 冻结 Delphi 客户端 |
| `MirClientZig/zig-out/bin/MirClientZig.exe` | 7,195,648 B，mtime `2026-10-22 11:54`，sha256 `0c6cc672fc86bde970d01340fe45833d99bb373addf12811370c7f951efd2676`，含 `MirClientZig`(×15) |
| 启动脚本 | `r5/client/run-release.cmd:4` → `"%~dp0MirClinet.exe" <加密启动参数>` |
| 日志佐证 | MirClientZig 每次启动必写 `!debug.txt`（`MirClientZig/src/core/log.zig:34`）；`r5/client/` 下**没有** `!debug.txt`。机器上唯一的 Zig `!debug.txt` 是 `D:/MirClient-run/!debug.txt`，11:51 那次（端口 7000、账号 `mir2test`），与本轮无关 |

⇒ 报障描述里的「客户端（MirClientZig，用 D:/MirClient-run 私有副本运行）」与实物不符：
r5 用的是**冻结 Delphi 客户端 + 私有资源目录**（资源用 junction 指回 `D:/MirClient-run`）。
两套栈（Rust / C#）用的都是这台 Delphi 客户端，所以「两边表现一样」是必然的。

> 这也解释了为什么「C# 对照实验未果」——对照其实不必做：**r3 那次本身就是 C# oracle + 同一台客户端的对照**，
> 结果就是「国王画出来（品红）+ 点不到（角色已被脚本走开）」。

---

## 3. MirClientZig 源码定位（若真用它跑，会不会建 actor？）

结论：**会**，而且不需要任何额外帧。

1. **分派**：`src/proto/dispatch.zig:467` `SM_TURN(10) → .turn`；`:494` `SM_FEATURECHANGED(41) → .featurechanged`。
2. **闸门**：`src/app/net_rig.zig:997-1029` `shouldApplyToWorld`——只要 `dispatch.lookupPlay(ident) != null`
   且分类是 `.login/.play`、且不在 `g_boMapMoving` 期间，就转给 world；`:691` `_ = self.world.apply(f);`。
3. **建 actor**：`src/game/world.zig:1870` `onTurn` → `:4497` `applyActorPose` → `:1401` `ensureActor`
   （不存在则 `append`，`actors_created += 1`）。`X/Y = param/tag`、`dir = LoByte(series)`、
   `light = HiByte(series)`、`feature/status` 取自体解出的 `TCharDesc` —— 与 `PlayScn.pas:3662-3668`
   的建 actor 名单 + `ReadyAction` 完全同构。
4. **41 只更新**：`src/game/world.zig:2611` `onFeatureChanged` = `findActor(...) orelse return`，
   与 `PlayScn.pas:3690`（不是建 actor 名单里的号 ⇒ `Actor=nil` 就 `Exit`）一致。
   ⇒ **不存在**「还需要 `SM_CHARSTATUSCHANGED`/`SM_APPEAR`/某种 announce 才建 actor」的隐藏条件。
5. **绘制**：`src/scene/play.zig:1195`、`:2637` 直接遍历 `world.actors.items`；
   `src/game/actors.zig:268-280` 把世界层新出现的 actor 无条件加入绘制表（无 race/appr 过滤）。
6. **点击**：`src/input/combat.zig:256` `mouseToCell`（逐字对 `CXYfromMouseXY`）；
   `:459` `getAttackFocusCharacter` → `:756` 商人分支 `npc_click`（`RCC_MERCHANT` 或 `on_sale`）。
   找不到目标 ⇒ `.move`。与 oracle 同一套几何，角色走远了同样只会走位。
7. **既有判据**：`src/game/world.zig:7529`「28 移动失败以 SM_TURN 落地（建 actor + 坐标/朝向/外观/光圈）」、
   `:7606`（801/807 建 actor + SM_TURN）、`:7676`（57 瞬移 → SM_TURN 落点）——都已覆盖「`SM_TURN` 建 actor」。

⇒ 在 MirClientZig 上，「收到 10/41 后不建/不画」这条**在源码层面不成立**。真要用它跑，预期同样是
「品红剪影 + 乱码名」（P2 有意复刻），并在角色走远后同样点不到。

---

## 4. 冻结 Delphi 客户端源码定位（实际跑的那台）

| 环节 | 位置 |
| --- | --- |
| `SM_TURN` 解码 + 转发 | `ClMain.pas:9746-9800`（体按 `GetCodeMsgSize(SizeOf(TCharDesc)*4/3)=16` 切，名字取尾部） |
| `SM_FEATURECHANGED` 转发 | `ClMain.pas:10831-10837`（`Feature=Makelong(param,tag)`、`State=Makelong(series,0)`） |
| 建 actor | `PlayScn.pas:3662-3668`（`SM_TURN/SM_WALK/SM_RUN/…` 名单内且 `FindActor=nil` ⇒ `NewActor`） |
| 41 只改既有 | `PlayScn.pas:3690-3694`（`Actor=nil` 已在上一步 `Exit`） |
| 品红成因 | `Actor.pas:2609-2636`，`:2626` `ceFuchsia` |
| 状态栏坐标 | `FState.pas:8890` `Format('%s %d:%d', [g_sMapTitle, g_MySelf.m_nCurrX, g_MySelf.m_nCurrY])` |
| 鼠标→格 | `PlayScn.pas:2951-2958` + `MShare.pas:4618-4621`（`ShiftYOffset`） |
| 点选 actor | `PlayScn.pas:2960-2990`（列扫描 `ccy+8 … ccy-1`） |

---

## 5. 根因判定

**`fixture`**。三条叠加，按贡献排序：

1. **夹具坐标**：`mir_flow.ps1` 的 `notice_poll` 用固定点 `(522,525)` 盲点 12 次；公告只吃第一次，
   其余 11 次落在世界区，把角色从 `(29,33)` 走到 `(29,44)` 并**存进档**。国王落到视窗外，
   后续「点国王」的屏幕坐标自然落到空地。C# oracle 栈上表现完全一致。
2. **客户端身份记错**：本轮实际跑的是冻结 Delphi 客户端，不是 MirClientZig；因此「去 MirClientZig 里找
   为什么不建 actor」这条路从一开始就没有对应现场。抓包 manifest（`r5/s2c/manifest.json`）只记录了
   proxy 与端口映射，**没有记录客户端二进制**，是这次误判的直接成因。
3. **渲染可读性**：国王确实画出来了，但被 oracle 的 P2 体分块缺陷染成品红 + 乱码名；
   在红色地毯背景上很容易被肉眼当成「没有精灵」。这不是缺陷，是**有意复刻**（`world.zig:41-46`）。

**服务器侧不需要为这个症状改任何帧**：建 actor 所需的 `10` + `41` 都已按 oracle 同形下发。

---

## 6. 下一步建议

### 6.1 夹具改（优先，能直接消除误判）

1. `tools/client/mir_flow.ps1:912-919`：`notice_poll` 不要在确定按钮位置盲点。
   改为「等 `SM_SENDNOTICE` 到达（或窗口上有公告对话框）→ 只点一次 → 校验 `CM_LOGINNOTICEOK` 已出网」，
   或点掉后立刻把鼠标移到界面区（如 `(522,760)`）再继续。
2. 点 NPC 之前**先断言角色坐标**：从抓包读最后一条 `CM_WALK` 的 `recog`（`X|Y<<16`）或截图 OCR 状态栏，
   不等于期望 `(29,33)` 就中止并报夹具错误。
3. 跑之前把测试库里的角色位置重置到 `(29,33)`（本轮第二轮登录直接从 `(29,44)` 开始，就是上一轮落库的）。
4. 在 `manifest.json` 里记 `client_exe` + `sha256` + `mtime`；这是本次误判的根因，成本极低。
5. 想点国王，用 `(512,263)`（角色在 `(29,33)` 时对应 `y=32`），不要用 `(512,227)`（对应 `y=31`）。

### 6.2 客户端改（MirClientZig）

- **无需改**：`SM_TURN` 建 actor、`41` 只更新，均与 oracle 同构（§3）。
- 建议补一条**回放判据**（tier=L）：把 `r5/s2c/capture.jsonl` 的 s2c 段喂给 `GameWorld.apply`，
  断言 `actors` 里存在 `id=132, x=29, y=32, race_img=50, appr=8`，且其 `feature=0x00080032`
  （`Makelong(50,8)`）。这样以后不必真机也能证明「帧到即建 actor」。
- 品红/乱码名**不要修**（P2 已裁决复刻）；若产品侧要修，须走 `T-*`/`D-*` 白名单并重开对拍。

### 6.3 服务器改（与本症状无关，但本轮抓包里有两处值得单独开卡）

1. **多出来的 `SM_HIT(14)`**：r5 `seq 40/53/59` 对国王发了 `SM_HIT recog=132 param=29 tag=32 series=6`，
   C# oracle 全程没有。来源是 `game-svr/src/downlink.rs:489-510`（`RM_HIT ⇒ SM_HIT(14)`，
   `lib.rs:11503-11531`），即「固定 NPC 受击下行」。需要查清是谁对商人发了 `RM_HIT`
   （本轮 `game.out.log` 里 `sim: tick … 实体内 267→3723` 持续增长，怀疑是 sim/假人链路的背景行为）。
   它不影响渲染，但属于与 oracle 的逐帧差异，对拍会被抓到。
2. **`SM_NEWMAP` 的 `series`**：Rust `1`，C# `2`（朝向/光）。若对拍按字段比会红，确认哪边才是 oracle。

### 6.4 若一定要复现「无国王」的原始症状

按 §6.1 修好夹具后重跑，预期结果是：进图即见**品红色国王**（乱码名），`(512,263)` 发出
`CM_CLICKNPC(1010)` 并在服务端开出商人对话。若那时仍然点不到，才是真正需要查的客户端/服务端问题。

---

## 7. 残留物

- 本文件（唯一写集）。
- 诊断期间的临时截图裁剪在 `E:/tmp/` 根下（`crop-status-*.png`、`st-*.png`、`crop-king-*.png`、`zig-frame-21*.png`），
  未入库；如需清理可直接删除。
- 未启动/未终止任何进程，端口状态未变。
