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
| 触发 | `CM_BUTCH(1007)`（`Messages.cs:57`）→ `PlayObject.Message.cs:829-831` 的 `ClientGetButchItem(nParam1,nParam2,nParam3,wParam)`；成功出 **`SM_BUTCH(637)`**（收官帧）；`SM_SKELETON(33)` **不是每刀必发**（只有 `BodyLeathery<=0` 才发，见 §2c.3b） |
| 线上字段 | `recog = MakeLong(尸体X,尸体Y)` ⇒ `nParam1=X`、`nParam2=Y`——尸体格由**运行期令牌 `@last_kill_xy`** 提供（寻靶击杀后尸体不一定在静态 `target_xy`） |
| 期望证据 | **`SM_BUTCH(637)`** 收官（`33` 非必发、**不作判据**）；两抽值域 `Random(16)+5 ∈ [5,20]`、`Random(201)+100 ∈ [100,300]`；**流位恰停第二抽** + site/order 账本 |
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
3. **死了但屠宰不接受**：不在 `kill` 内——由紧随其后的 `wait_for(637)` 超时表达（死亡帧已到，
   说明击杀成功；**637 不来** = 屠宰被拒（尸体被清/距离或速度门））。

### 2b.3 场景与状态

- `butch_two_draws` 步序（**现行版**）：`kill`（**寻靶** `track: nearest:鸡`，`max_retries=3`、
  `interval_ms=1000`；`self_pos=21627210`，`target_xy` 只是夹具落点）→ `send CM_BUTCH(1007,
  recog=@last_kill_xy)` → `wait_for(637)` → `sleep 500`。
  **`SM_SKELETON(33)` 不是必现**：`PlayObject.Operate.cs:731-738` 只有 `BodyLeathery<=0` 才发，
  每次屠宰随机扣 5–20 ⇒ 首刀远到不了 0；**不得把它写进步序当判据**（见 §2.3 第 3 条）。
  run3 `#21` 的 `extra` 现在是 `["--timeout-ms","60000","--profile","tools/window/profiles/butch_monsters.json"]`
  （后者提供活体夹具：`tools/window/profiles/butch_monsters.json`，map 0 的 (330,331) 刷 1 只「鸡」）。
- **`target_xy`/`self_pos` 放 `params`（默认 21692746/21627210），不要放 `required`**：`card_window` 见到任何
  `required` 会先跑 discovery（`npc_click_auto`），map 0 无 NPC ⇒ 必然超时（实测踩到，rc=2）。
- **状态：仍 `blocked`**，`blocked.reason` 已改为「能力齐、只差实测」；解封条件（实测步骤 +
  证据清单 + 失败分流）写在场景 `blocked.missing_fixture`。
- 自测：`kill` 进 schema 校验（未知键/越界/缺 `self_pos`/非相邻全拒）、`SEND_KINDS`、`check_scenario`
  方向校验（`ident`/`turn_ident` 必须 CM、`wait_ident` 必须 SM）；运行期用假 socket 覆盖
  dead/alive/no-target 三态。

## 2c. 实测记录（2026-10-25，卡 `openmir2-butch-two-draws-unblock`）

> 目的：记录两次真机尝试与由此收窄的缺口；**结论仍是 blocked（未打通）**，不凑绿。

### 2c.1 环境与前置事实

| 项 | 事实 |
|---|---|
| 角色 | 基线 `winplay0`（Id 6228）L1；`characters_ablity.Dc=0` ⇒ **空手对 AC=0 的鸡也打 0 伤害**（`SM_STRUCK` 只在 `wParam>0` 发）⇒ 场景必须先 `CM_TAKEONITEM` 装武器 |
| 背包 | **46/46 满**且无武器 ⇒ `inventory` 夹具（要求空槽）写不进去；本轮按 Leader 授权用**一次性手工 DB 夹具**（restore 后：删槽 40、插 木剑 `StdIndex=207/MakeIndex=5001/Dura 4000`、`characters` 落点 `0/330/330`），before/after SELECT 均已留证 |
| MonGen | 部署树实际 **3080/3081 行**（日志里的 39020 不是行数）；`RegenMonstersTime=200ms` 且 `ProcessMonsterMultiThreadLimit` 缺省 **1**（`GameSvrConf.cs:1896`）⇒ 刷怪线程**每 200ms 只评估一条**，扫一轮 ≈ **616s ≈ 10 分钟** |
| 装备 | `SM_TAKEON_OK(615)` 只证明**被接收**；是否生效须靠 `SM_STRUCK(wParam>0)` 或死亡帧证明 |

### 2c.2 第一轮（MonGen 夹具 **append** 到文件尾）：`no target frames`

- 现场：`SM_TAKEON_OK(615)` ✔；20 次 `CM_HIT(3014, recog=自身, tag=Down)` 全部被接受（`SM_HIT(14)` × 11，其余被 `HitIntervalTime` 节流）；**`SM_STRUCK(31)` = 0、目标格活动帧 = 0、死亡帧 = 0** ⇒ `kill: no target frames at (330,331)`。
- **根因**：夹具行被追加在 3080 行末尾；单线程 200ms/条 ⇒ 窗口内（~60s）根本评估不到该行（扫一轮约 10 分钟）。⇒ **活体/尸体类夹具必须 prepend 到文件头**（首个闸门 `StartTick==0` 立即刷）。
- 已排除：空手 0 伤（本轮已装木剑并 615 成功）。

### 2c.3 第二轮（prepend）：**被宿主内存压力终止，无结论**

- prepend 已生效（MonGen 两处 `sha e5c39fb7 -> 8ad1826a`，首行 `0 330 331 鸡 0 1 360`）；C# 栈起来过（15000/17000/17100/17200 曾在听）。
- 但后台任务在起栈阶段被宿主**因系统内存过低终止**（非命令失败）⇒ probe 未运行，`frames.ndjson`/`run.json` 均未生成；**1/2/3 三段（prepend 是否真刷出 / kill / butch）全部「未观测」**。
- 清理：C# 栈停、MonGen 两处还原（`e5c39fb7`）、DB restore-baseline、`Castle/List.txt` 三份一致（`9b38d284`）、`stack-release` 已签。

### 2c.3b 装备后 settle 与「33 不是首刀」（tester run4/run5 证据，含一条被撤回的推论）

- **保留**：装备完成后 **settle 1.5s** 再发 `CM_TURN`/`CM_HIT`（run4→run5 真正起作用的改动之一；
  另一处是把夹具从 1 只鸡改成 2 只）。场景已含 `sleep 1500` + 显式 `CM_TURN(3010, tag=4)`。
- **撤回**：曾据 run4 推断「装备后服务端把朝向重置成 tag=2 ⇒ 方向错位导致空打」。run5 同样**没有**
  `tag=4` 的回帧（唯一自身 `SM_TURN` 是 `param=512 tag=2`，应为装备动作），但 kill 正常命中并击杀
  ⇒ **「没有 SM_TURN 回帧」不能当作朝向未生效的判据**；场景里「等自己的 SM_TURN(10)」那一步已删。
- **`SM_SKELETON(33)` 不是首刀就发**（oracle 真相）：`PlayObject.Operate.cs:731-738` 只有
  `BodyLeathery <= 0` 才置 `Skeleton=true` 并发 `RM_SKELETON`；每次屠宰随机扣 5–20，首刀远到不了 0。
  ⇒ **不得把「33 未出现」判成失败**；butch 的**收官帧是 `SM_BUTCH(637)`**，场景只等 637。
  要验 33 必须把 `BodyLeathery` 打到 ≤0（多次屠宰或夹具预置），并单独写依据。
- run5（tester，C# 侧）已打通：`kill result=dead`、**4 刀**打死，帧序
  `34(SM_NOWDEATH, recog=target, 330/331) → 31(SM_STRUCK, series=5) → 32(SM_DEATH, 330/331) → 637(SM_BUTCH)`。

### 2c.4 账本代价（实测）

C# `-RngLog` 每抽数都落盘：~~10 分钟即 **397,951,366 字节**（上一轮 899MB/20min）~~ ⇒ 已瘦身为 head 2MB 后删除原文件。**后续实测必须限时/限行**（hook 无 `MIR2_RNG_MAX_LINES` 之类开关，是待补能力）。

### 2c.4b 走位问题的确定性解法，以及一个**未解的接口缺口**（2026-10-25 复测前分析）

**已有解法（已合入 master）**：卡 `openmir2-probe-kill-track-target` 落地 `kill: {track: "nearest:<名>"}`
——每轮按**最新帧**取目标当前坐标与 actor，出手前复核，走开就重算（`max_retries`，默认 3）。

**节流口径（别为了快把挥击压没）**：track 每轮 = 选靶 → `CM_TURN` → `pump(interval)` → 复核 →
`CM_HIT` → `pump(interval)`，所以**相邻两次 `CM_HIT` 的真实间隔 ≈ 2 × interval_ms**；
部署 `HitIntervalTime = 850`（`src/M2Server/Player/PlayObject.Attack.cs:850`、`Server.conf:325`）
⇒ `interval_ms` 的下限 350 只是硬门槛，**不是推荐值**：350/400 ⇒ 有效间隔 700/800ms < 850ms，
一部分挥击会被「攻击忙」静默吃掉。**默认 1000（⇒ 有效 2000ms）是安全值，别往下压**。

**新发现（本轮实测前的静态核查）：track 模式下 `CM_BUTCH` 拿不到尸体格。**

- `CM_BUTCH(1007)` 的 `recog = MakeLong(尸体X, 尸体Y)`（`PlayObject.Message.cs:829-831`），必须是**尸体所在格**；
- track 击杀结束时，尸体格 = 杀步记录的 `run.steps[i].target_xy`（死亡帧 `param/tag` 也带该格，
  见 `_execute_kill_track` 的匹配条件）——**数据在运行期确实存在**；
- 但场景里后续 `send` 的 `recog` 只能写**静态** params（`resolve_param` 只查 `params`/字面量，
  `mir2_probe.py:396-407`），**没有**把上一个 step 的运行期结果接进下一步的机制
  （现有两个运行期特例只用于 NPC 对话：`state["last_merchant_recog"]` / `last_turn_recog`）；
- ⇒ 用 track 杀掉之后，`CM_BUTCH` 的坐标无从写起；写死 `params.target_xy` 在目标走过位时必错。

**三条可选路径（按代价排序）**：

| # | 方案 | 代价 / 结论 |
|---|---|---|
| A | probe 加运行期令牌：kill 成功时写 `state["last_kill_xy"]`，`send.recog` 允许 `"@last_kill_xy"` 在运行期解析 | 最小改（约 10 行 + 牙），但**属 probe 写集**，要 Leader 派给写集 owner |
| B | 换成**不走位**的怪 | **已排除**：`WalkStep=0` 只有 36 行，除 `HP=0` 的占位 `558` 外全是 卧龙守将/魔将/道尊/战将（HP 1000–3000、AC 0–10），没有「低血量站桩怪」可打（证据见 §2c.4c） |
| C | 8 邻格扫 `CM_BUTCH`（尸体必在自身 8 邻里） | **不采用**：错格的 `CM_BUTCH` 依赖未登记的服务端行为（可能只少一条回帧、也可能有副作用），拿它当判据会把「猜」写进场景 |

⇒ **已解决**：Leader 裁决方案 A 归本线，已开卡 `openmir2-probe-runtime-param-last-kill-xy` 并合入 master `83799acc9`；本卡场景已改为 **kill 用 `track`（`nearest:鸡`，`max_retries=3`，`interval_ms=1000`）+ `CM_BUTCH recog=@last_kill_xy`**，`--check-scenario` rc=0（依赖 `card/probe-runtime-param-check` 的 `_var_refs` 放行令牌，见该卡 follow-up）。

### 2c.4c 「换站桩怪」的数据证据（只读 SELECT，未起栈）

```sql
SELECT Idx,Name,HP,AC,MAC,DC,DCMAX,SPEED,WalkStep FROM monsters WHERE WalkStep=0 ORDER BY HP ASC, Idx ASC;
-- 36 行：558 HP=0（占位）；403-418 卧龙守将 HP=1000 AC=10；
--        425-430 卧龙魔将 HP=1500 AC=0；419-424 卧龙道尊 HP=3000；431-437 卧龙战将 HP=3000
```

即使 `木剑` 装上（Dc>0），对 AC=10 的守将伤害会被吃光、对 AC=0 的魔将/道尊/战将则要 150–750 刀
（`max_swings` 上限 100、每刀 2s）⇒ **没有可用目标**。

### 2c.5 重跑清单（待窗口）

0. **前置门：`Dc>0` 自证**（装备后态快照 `characters_ablity.Dc` / `characters_item` 武器行）——
   `Dc=0` 时空手对 AC=0 也是 0 伤，kill 永远等不到死亡帧（第一轮实测）；
1. 用 tester 的 **prepend 夹具**（`5b8aac21` 合入后走声明式 `fixtures.monsters`，不再手工插首行）；
2. 场景保留 `CM_TAKEONITEM(1003)+SM_TAKEON_OK(615)` 前置 + `sleep 1500` settle；
3. **kill 用 track**（`track: "nearest:鸡"`, `max_retries: 3`, `interval_ms: 1000`——有效 2s，安全高于
   `HitIntervalTime=850`）；三段判据分开写：
   - **刷出**：目标格出现活动帧（`SM_TURN/APPEAR`）；
   - **命中扣血**：`SM_STRUCK(31)`（`param=HP`）或 `SM_HIT(14)` 计数 > 0，且非「被吃掉的挥击」
     （数 `SM_HIT` 与实际 `CM_HIT` 发送数对比）；
   - **死亡 + 屠宰**：`SM_NOWDEATH(34)`/`SM_DEATH(32)`（`param/tag` = 目标格）⇒ `CM_BUTCH` ⇒ **`SM_BUTCH(637)`**
     （收官帧；33 需 `BodyLeathery<=0`，不是首刀判据） + 两抽（`Random(16)+5`、`Random(201)+100`）的 site/order；
4. **`CM_BUTCH` 的坐标**由 §2c.4b 的方案 A 解决（`@last_kill_xy`，已随 probe 卡合入）：场景已改为
   **kill 用 track + `CM_BUTCH recog=@last_kill_xy`**，`--check-scenario` rc 0；`target_xy` 只剩
   「夹具落点」语义，**不再参与八向相邻校验**（鸡刷错位置会在 kill 处 **fail-closed**：`StepFailure` 报「目标与自身不八向相邻 ⇒ 需手动站位」）；
5. 账本限时（≤5 分钟）并当场裁剪。

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
21  butch_two_draws      alt           YES      | blocked: **已解：前置插入生效**（原缺口 = MonGen 追加到表尾：3081 行 × RegenMonstersTime=200ms ≈ 616s，且部署 ProcessMonsterMultiThreadLimit 未配=1 ⇒ 60s 内轮不到追加行）；**剩余缺口 = 真机实测**
22  castle_merchant_tax  alt           YES      | blocked: ...
23  castle_merchant_tax_potion …
24  castle_merchant_tax_upgrade …
场景数 24（blocked 3 ⇒ 入批 21）
```

（文案与场景 JSON 的 `blocked.reason` **同源**：`blocked_sources_check` 要求两侧含齐
`3081` / `616` / `ProcessMonsterMultiThreadLimit` 三个 token，改一侧必须同步另一侧。）

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
