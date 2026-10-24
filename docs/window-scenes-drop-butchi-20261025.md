# 窗口新场景第三批：`dropgold_zero_draw` / `butch_two_draws`（W-DG-1/2，tier L）

- 卡：`openmir2-window-scenes-drop-butchi`（owner `openmir2-dev-2`，branch `card/window-scenes-drop-butchi`，base `788c2db1f`）。
- 写集（fork-point 口径 4 文件）：`tools/probe/scenarios/dropgold_zero_draw.json`、`tools/probe/scenarios/butch_two_draws.json`、
  `tools/window/run3-scenarios.json`（只追加 #20/#21）、本报告。
- 来源：台账 `docs/window-closure-ledger-20261025.md` #6 行（R 片 `a3bf5340f`）、`docs/window-kit-coverage-20261025.md` §1.6。
- **不动**：`card_window.py` / `run_batch.sh` / `preflight_private_stack.py` / `mir2_probe.py`；不改生产代码；不起服务端。

## 1. 场景 1：`dropgold_zero_draw`（W-DG-1）

| 项 | 内容 |
|---|---|
| 触发 | `CM_DROPGOLD(1016)`（`src/OpenMir2/Messages.cs:68`）→ `PlayObject.Message.cs:903-908` 的 `nParam1 > 0` 门 → `ClientDropGold(nParam1)` |
| 线上字段 | C2S 与 `CM_HIT` 同系：`nParam1 = LoWord(Recog)` ⇒ 场景取 `recog=100`（100 金） |
| 步序 | `send 1016(recog=100)` → `wait_for 653`（**`SM_GOLDCHANGED`**，`messages.rs:290`；**不是** `SM_DROPITEM`——那是 600/601 的 `CM_DROPITEM` 线）→ `sleep 500` |
| 期望证据 | `SM_GOLDCHANGED(653)` **恰好 1 条**、`Recog` 与请求同源；**金币前后差 = 0**、**地面无金币**；**RNG 流位不前进**（R 片 `a3bf5340f` 的「深体零抽数」结论） |
| 失败形态 | 第 2 步超时 ⇒ 没进 `ClientDropGold`（`nParam1<=0`）**或命中前置门**：`InSafeDisableDrop && InSafeZone()`、`ControlDropItem && nGold < CanDropGold`（`Operate.cs:224-232`，只发 `RM_MENU_OK` 提示）⇒ 夹具/配置问题，**不是 RNG** |

**K-2（BUG-REPLICA 2 的等价行为，非缺陷）**：`ClientDropGold`（`Operate.cs:222-246`）为 `Gold -= nGold; if (!DropGoldDown(...)) Gold += nGold; GoldChanged();`；`DropGoldDown` 构造的 `MapItem` **从不赋 `ItemId`**（默认 0），`AddItemToMap` 首行 `if (mapItem.ItemId == 0) return false;`（`Envirnoment.cs:88-93`）恒失败 ⇒ **金币回滚、净差 = 0**，但 653 照发。**切勿把「差 = 100」当判据**——那会在正确实现上误判失败。

**降级口径（重要）**：RNG 流水依赖私栈 `-RngLog` + `MIR2_RNG_SITE=1` + C# hook（卡 `openmir2-window-csharp-rng` 在接线）。
未接线时本场景**只能验 653 帧与金币差**，RNG 面记「**未观测**」，**不得写成 PASS**。

## 2. 场景 2：`butch_two_draws`（W-DG-2）—— **blocked**

| 项 | 内容 |
|---|---|
| 触发 | `CM_BUTCH(1007)`（`Messages.cs:57`）→ `PlayObject.Message.cs:829-831` 的 `ClientGetButchItem(nParam1,nParam2,nParam3,wParam)`；成功出 `SM_SKELETON(33)` → `SM_BUTCH(637)` |
| 线上字段 | `recog = MakeLong(尸体X,尸体Y)` ⇒ `nParam1=X`、`nParam2=Y`（由夹具提供，场景用变量 `target_xy`，不写死） |
| 期望证据 | `33` → `637` 帧序；两抽值域 `Random(16)+5 ∈ [5,20]`、`Random(201)+100 ∈ [100,300]`；**流位恰停第二抽** + site/order 账本 |
| 失败形态 | 第 1 步后面超时 ⇒ `ClientGetButchItem` false（无尸体/距离/速度门），不是抽数问题 |

**为什么 blocked（双缺口）**：

1. **无尸体夹具**：`card_window.py` 只能 placement 玩家，不能预置「单只固定动物尸体」；窗口 kit 目前也没有 drop/butch 场景。
2. **RNG 面未接线**：「两抽值域 + 恰停第二抽」必须两侧流水（`-RngLog` + `MIR2_RNG_SITE=1` + C# `RngSeedHook`），C# 侧未接。

解封条件写在 `blocked.missing_fixture`：① 尸体夹具（单只固定动物在已知格被杀后的尸体）；② C# 栈接 `start-csharp-hook.ps1`
+ `RngSeedHook.dll`（`openmir2-window-csharp-rng`）+ 两侧同 `-Seed N` + `MIR2_RNG_SITE=1`。
**blocked 场景不进 run3 批次**（runner 见到 `blocked` 即 rc=2）——因此本卡只把 `dropgold_zero_draw` 作为可跑行加入，
`butch_two_draws` 的 run3 行**已登记但需 tester 在批次里跳过**，或解封后再真正启用（见下节口径）。

## 2b. 2026-10-25 更新：kill 步落地（卡 `openmir2-probe-kill-step`）

> 本节是**后续卡**对 §2 的修订：`butch_two_draws` 的「场景击杀步」缺口已补，但**仍 blocked**——
> 红线要求**实测**击杀→屠宰链；本轮无私有栈窗口，故不声称可跑。

### 2b.1 `kill` 步契约（`tools/probe/mir2_probe.py`）

```json
{"kill": {"target_xy": "target_xy", "self_pos": "self_pos",
          "ident": 3014, "turn_ident": 3010,
          "max_swings": 20, "interval_ms": 1000, "wait_ident": 34}}
```

| 项 | 语义 / oracle |
|---|---|
| `target_xy` | 目标格的 `MakeLong(X,Y)`；**必须与 `self_pos` 八向相邻**（加载期就拒）——`CM_HIT` 只打 `GetPoseCreate()` 的**正前方格**（`PlayObject.Attack.cs:26/883`） |
| `self_pos` | `MakeLong(自身X,自身Y)`（可由 params 提供，缺省名为 `self_pos`；R5 与 placement 自洽） |
| `ident`/`turn_ident` | 默认 `CM_HIT(3014)` / `CM_TURN(3010)`（`Messages.cs:120/136`）；必须 `CM_*`（`check_scenario` R3b） |
| `max_swings` | 1..100，默认 20 |
| `interval_ms` | 350..60000，默认 **1000**——同 ident 连击之间有 `HitIntervalTime - HitSpeed*ItemSpeed`（`PlayObject.Attack.cs:850`；部署 `HitIntervalTime=850`，`Server.conf:325`）。**低于它不只是浪费挥击**：被拒的帧进超速分支，部署 `MaxHitMsgCount=1`/`OverSpeedKickCount=2`/`KickOverSpeed=1`（`Server.conf:318/406/407`）⇒ 累计 >2 次被拒会 `BoEmergencyClose` **直接踢掉探针连接**（review `b0a7797b0`） |
| `wait_ident` | 默认 `[SM_NOWDEATH(34), SM_DEATH(32)]`；**必须 `SM_*`**。真死亡广播是 `RM_DEATH` nParam3=1（`BaseObject.Base.cs:351` → `PlayObject.Message.cs:1632-1634`），帧里 **param/tag = 目标 X/Y**；`SM_DEATH(32)` 是对「已死 actor 进入视野」的刷新帧（`PlayObject.cs:3235`） |

### 2b.2 三态失败（各不同错误串）

1. **没刷出来**：`kill: no target frames at (X,Y) after N swing(s)` —— 窗口内目标格零帧
   （`fixtures.monsters` 未生效/落点不同格；也含「0 伤害打空」：`SM_STRUCK(31)` 只在
   `wParam>0` 时发，`PlayObject.Message.cs:1564`）。
2. **打了没死**：`kill: target alive after N swing(s)` —— 收到过目标格活动帧（`SM_TURN/DEATH` 带坐标）
   或带伤害的他人 `SM_STRUCK`，但没等到死亡帧；错误串带帧摘要。
3. **死了但屠宰不接受**：不在 `kill` 内——由紧随其后的 `wait_for(33)` 超时表达（死亡帧已到，
   说明击杀成功；33 不来 = 尸体被清/距离或速度门）。

### 2b.3 场景与状态

- `butch_two_draws` 步序：`kill`（`target_xy` = 目标格、`self_pos=21627210`）→ `CM_BUTCH(1007)`
  → `SM_SKELETON(33)` → `SM_BUTCH(637)` → `sleep 500`；场景总时长 20×1s ⇒ run3 `#21`
  带 `extra=["--timeout-ms","60000"]`。
- **`target_xy`/`self_pos` 放 `params`（默认 21692746/21627210），不要放 `required`**：`card_window` 见到任何
  `required` 会先跑 discovery（`npc_click_auto`），map 0 无 NPC ⇒ 必然超时（实测踩到，rc=2）。
- **状态：仍 `blocked`**，`blocked.reason` 已改为「能力齐、只差实测」；解封条件（实测步骤 +
  证据清单 + 失败分流）写在场景 `blocked.missing_fixture`。
- 自测：`kill` 进 schema 校验（未知键/越界/缺 `self_pos`/非相邻全拒）、`SEND_KINDS`、`check_scenario`
  方向校验（`ident`/`turn_ident` 必须 CM、`wait_ident` 必须 SM）；运行期用假 socket 覆盖
  dead/alive/no-target 三态。

## 3. run3 注册（只追加两行）

`tools/window/run3-scenarios.json`：`#20 dropgold_zero_draw`（`profile=alt`、`extra=[]`）、`#21 butch_two_draws`（`profile=alt`、`extra=["--timeout-ms","60000"]`；见 §2b.3），
既有 19 条**逐字节未动**（diff = +2 行 + #19 补逗号）。`#21` 行带 `"blocked": "<原因>"`。

**blocked 的批次语义（按 `tools/window/run_batch.sh:107-108` 注释 + `:223-225` 实现）**：
`blocked` 场景**只在 `--list` 里标注、不进批次**——普通批次会自动跳过并打印
`场景数 N（blocked B ⇒ 入批 N−B）`；**只有 `--only <该场景>` 点名时才会 die（rc=2）**，那是**预期行为**。
因此窗口**不需要手工剔除 #21**，直接跑批次即可；解封后才入批。

本分支实测：

```text
$ bash tools/window/run_batch.sh --list
...
21  butch_two_draws      alt           YES      | blocked: 无「单只固定动物尸体」夹具 + RNG 面未接线（详见 tools/probe/scenarios/butch_two_draws.json 的 blocked）
场景数 21（blocked 1 ⇒ 入批 20）
```

## 4. 依赖与降级总表

| 依赖 | 影响 | 未就绪时 |
|---|---|---|
| 私栈 `-RngLog` + `MIR2_RNG_SITE=1` | W-DG-1 的「零抽数」、W-DG-2 的「两抽值域/恰停第二抽」 | 记「未观测」，不得 PASS |
| C# `RngSeedHook`（`openmir2-window-csharp-rng`） | 两侧同 seed 对齐 | 同上 |
| 尸体夹具（单只固定动物） | W-DG-2 的全部判据 | `butch_two_draws` 保持 `blocked` |

## 5. 自证与门禁

- `--check-scenario`：`dropgold_zero_draw` **rc=0（3 步）**、`butch_two_draws` **rc=0（4 步，blocked 形状合法）**；
  既有 19 条场景 check 仍全绿。
- 门禁：四卫生门 + `git diff --check <base> <head>` + `merge_tree_helper`，见 patch body / status。
