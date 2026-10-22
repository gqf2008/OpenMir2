# 第九轮（r9）：掷骰对话框点击链全程打通；`SM_PLAYDICE` 在 oracle 下**不可达**

- 日期：2026-10-22
- 基线：master `00386fe81`（`mir2-server` debug 构建，2026-10-22 18:23）
- 环境：备用端口剖面（game-svr 15000 / db-srv 16000·15100·15700 / login-srv 15500·15600 /
  login-gate 17000 / sel-gate 17100 / **game-gate 17600**）+ 抓包代理 `17200 → 17600`
- 夹具：测试库 `mir2_test_db`，角色 `winplay0` 置 `B013 (5,9)`；**往背包种 1 个「筹码包」（StdIndex=44）**
  （`characters_bagitem Position=0 MakeIndex=70000001`）——只动自建测试库，未改任何内容文件

## 1. 结论（TL;DR）

| 项 | 判定 |
| --- | --- |
| 对话框链接点击链 | **GREEN**：`@main → @kaiz → @ya5 → @ya52` 四跳全部有 c2s `1011` 与服务端回帧 |
| 「正在投骰子…………」 | **GREEN**：`@ya52` 的 `#SAY` 原样下发（客户端截图可见） |
| `SM_PLAYDICE(8001)` | **未出现，且 oracle 下不可达**（见 §3）——不是本仓缺陷 |
| 新发现（另卡） | 脚本 `take` **不落库**：筹码包被脚本摘掉后在存档里仍在（见 §4） |

## 2. 点击链证据

服务端日志（`E:/tmp/window/r9/logs/game-svr.out.log`）：

```
11:19:07.731 [INF] CM_CLICKNPC：到达 Click 点 ChrName=winplay0 npc=119（@main 说词 1 条）
11:19:18.158 [DBG] sim: index=1 命令 Recog=119 Ident=1011 Param=0 Tag=0 Series=0 tail="@kaiz"
11:19:26.778 [DBG] sim: index=1 命令 Recog=119 Ident=1011 Param=0 Tag=0 Series=0 tail="@ya5"
11:19:38.407 [DBG] sim: index=1 命令 Recog=119 Ident=1011 Param=0 Tag=0 Series=0 tail="@ya52"
```

抓包（`proxy/conn-0002.*`）里对应 5 条 `SM_MERCHANTSAY(643)`：`@main` 226 B → 下注菜单 333 B →
`@ya5` 的「大/小」面板 → `@ya52` 的「正在投骰子…………」。**没有任何 `8001`。**

### 2.1 链接矩形改为**从截图自动求**（本轮新增的夹具手法，可直接复用）

`FState.pas:12097-12115` 的命中矩形只覆盖尖括号内可见文本（`Rect(lx+sX, ly, lx+sX+TextWidth, ly+14)`，
`lx=30`、`ly=20+16k`）。本轮不再肉眼估点，改为**像素检测**：

1. `mir_click.ps1 -Shot` 取整窗截图；
2. 裁出对话框面板（窗口原点 `(3,26)`），取「黄字」掩码 `R>170 & G>140 & B<110 & |R-G|<80`；
3. 按行分带（间隙 ≤3 px）、带内按列分 run（间隙 ≤10 px）、丢弃宽度 <12 px 的噪声；
4. **点击点取 `(run 中心 x, 带顶 y+7)`**——不是带中心：字形带比 14 px 命中框高（r7 的
   `y=97/100` 就死在「拿字形中心当矩形中心」上）。

实测（`shot2.png`，面板坐标）：`@kaiz` = `(43..107, y 68..79)`、`@chouma` = `(169..233)`、
`@huit` = `(331..377, y 138..150)`，与 `docs/dice-link-diagnosis-20261022.md` §3 的结论逐像素一致。
点击 `(75,75)` ⇒ `1011 @kaiz` **一次命中**。

上述步骤已固化成工具 **`tools/client/dlg_links.py`**（PIL+numpy，无其它依赖）：

```sh
python tools/client/dlg_links.py E:/tmp/window/r9/shot3.png              # JSON：每个 run 的矩形 + 点击点
python tools/client/dlg_links.py shot.png --say-file say.txt             # 按序贴上 <文本/@参数>
python tools/client/dlg_links.py --selftest                              # 对 r7 截图断言三个链接矩形
```

自测即判据：对 `E:/tmp/window/r7/shot-dice5.png` 必须报出 `43..107 / 169..233（y 带顶 68）`
与 `331..377（y 带顶 148）`，且点击点落在 14 px 命中框内（`SELFTEST GREEN`）。

> 注意：`@ya5` 之后的「大/小」面板是**大对话框**（`atlen>=15`），链接颜色不是黄色（灰白），
> 像素掩码抓不到；该面板坐标按行距直接取（`※小※` = client `(200,89)`）⇒ 命中 `@ya52`。

## 3. 为什么 `SM_PLAYDICE` 不可达：oracle 的动作循环「执行一条已注册动作即 return」

`src/Modules/ScriptEngine/ScriptEngine.cs:884-893`：

```csharp
for (int i = 0; i < actionList.Count; i++) {
    QuestActionInfo questActionInfo = actionList[i];
    if (ExecutionProcessing.IsRegister(questActionInfo.nCmdCode)) {
        ExecutionProcessing.Execute(normNpc, playerActor, questActionInfo, ref result);
        return result;                     // ← 只执行**第一条已注册动作**就返回
    }
    switch ((ExecutionCode)questActionInfo.nCmdCode) { case ExecutionCode.Take: … }
}
```

- `ExecutionCode.Take`（=2）**没有**注册（`Processings/ExecutionProcessingSys.cs` 里没有
  `ExecutionCode.Take]` 的赋值，只有 `ScriptEngine.cs:898` 的 switch 分支）⇒ 继续循环；
- `ExecutionCode.Movr`（=36）**已注册**（`ExecutionProcessingSys.cs:166`）⇒ 执行后 **return** ⇒
  同一条 `#ACT` 里在它后面的 `inc d0 1` 与 `playdice 1 @cubeXX` **永远不执行**。

部署内容里**每一处** `playdice` 都长这样（全量扫描 `Envir/**`，24 处，无一例外）：

```
take 筹码包 N
movr d0 6
inc d0 1
playdice 1 @cubeXX
```

⇒ 按 oracle 语义，**这些脚本永远不会投骰**：玩家会白掉一个筹码，`@cubeXX` 也永远到不了。
这属「行为等价优先于修 bug」，本仓必须照搬（`mir2-rs/crates/script/src/engine/mod.rs:1321-1350`
的 `run_actions` 已是同构实现）。

### 3.1 本仓复现（临时探针，已还原）

用**部署脚本原文**走生产入口 `MerchantScriptBook::goto_label("@ya52")`：

```
says=["正在投骰子…………"]      # #SAY 照出
items_after=[]                # take 生效（NpcPlayer 快照内）
frames=[]                     # 没有 RM_PLAYDICE
dice_label=None
变体（动作列表只留 playdice）：frames=[(10500, Some("@cube52"))]   # 有 playdice 就必出帧
```

⇒ 引擎的 `playdice → RM_PLAYDICE(10500) → SM_PLAYDICE(8001)` 接线本身是好的（T-142 单测亦绿），
**卡点在 oracle 的动作循环提前 return**，与 `pickwire`/`smframe` 无关。

**处置**：T-142 的窗口项「真机抓 `SM_PLAYDICE`」改为**不可达，关闭**（证据即本文 §3）；
帧结构判据仍由 `pickwire::script_frame_maps_playdice_to_sm_playdice`（逐字节）承担。

## 4. 新发现：脚本 `take` **不落库**（另开卡）

- 本轮给角色种了 1 个筹码包；`@ya52` 的 `#IF CHECKitem 筹码包 1` 通过、`take` 执行、
  NpcPlayer 快照内 `items_after=[]`；
- 但断线存档（`11:23:07 存档请求已发 … 4868 字节`）后查库，
  `characters_bagitem(PlayerId=6228, Position=0)` 仍是 `StdIndex=44`；
- 根因：`NpcPlayer::remove_item`（`bins/game-svr/src/npc.rs:1241`）只改本地 `items: Vec<(String,i32)>`，
  而 `run_merchant_goto`（`lib.rs:5623-5655`）只回写 `mnmval` 与 gold-deal，**没有物品回写**。
- oracle 侧 `GotoLableTakeItem` 直接作用在 `PlayObject.ItemList` 上、随存档落库 ⇒ 这是**真实偏离**
  （tier=R：持久化面）。另卡 `openmir2-script-take-persist`。

## 5. 证据哈希

| 证据 | 路径 | sha256 |
| --- | --- | --- |
| 服务端日志 | `E:/tmp/window/r9/logs/game-svr.out.log` | `49f1b6b1caa0e29b226c91874125f8cfd35f78f9c9fc7b28b2a79d9626bb8835` |
| c2s 抓包 | `E:/tmp/window/r9/proxy/conn-0002.c2s.bin` | `2b409d1fecfe2596a742c46ea30830769f75e2a2c74f2636d7e0b946d57ae020` |
| s2c 抓包 | `E:/tmp/window/r9/proxy/conn-0002.s2c.bin` | `546fbc1c903c74c52cdd0e72897009e98c3050108ec8eb5a960f6abe56bf1fff` |
| `@main` 对话框 | `E:/tmp/window/r9/shot2.png` | `e00e5adddd59c7ed7bef9d8a4515e0a8944b57878f263083a972b1330d8bd7ec` |
| 下注菜单 | `E:/tmp/window/r9/shot3.png` | `c501684af7873c7fa4f18d512db31dbe3f476f57efc59fbc5bf51dff3bd59060` |
| 大/小面板 | `E:/tmp/window/r9/shot4.png` | `8989bf815e7c50694da647606f0574ddb36336253e0261d3045430814d26be6c` |
| 「正在投骰子」 | `E:/tmp/window/r9/shot5.png` | `7b7dcf00829574daabdd18991eb60fe14575f9e240c8cdc91bb32e73f244e1d6` |

## 6. 残留物

- `E:/tmp/window/r9/**`：本轮栈日志、抓包、截图（保留作证据，sha256 见上）；
- 测试库 `mir2_test_db`：角色 `winplay0` 位置 `(6,10)`（本轮点击走位）、背包 Position 0 仍是筹码包
  （§4 缺陷所致）；下一轮起点如需 `(5,9)` 直接改库；
- 栈已按反向顺序停止，15xxx/17xxx 全 DOWN（`15721` 是本机他方进程，与本轮无关）；
- 客户端进程已结束；代理已停。
