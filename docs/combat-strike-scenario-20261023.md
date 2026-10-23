# R3 场景 `combat_strike`：主动一击 + 命中帧（卡 `openmir2-scenario-combat-strike`）

- 车道：`wt-worker-71` / principal `openmir2-worker-71` / 分支 `card/scenario-combat-strike`
- tier：**L**（只造场景夹具；零生产代码、零帧字节）
- 交付物：`tools/probe/scenarios/combat_strike.json`（+ 本说明 + `tools/probe/scenarios/README.md` 一段）
- 未起栈、未连库、未占端口（本文所有数字都是离线推导：读 `.map`、读部署 `Envir/`、读 `sql/` 与 `E:/tmp/conformance/baseline/*.sql`）

---

## 0. 结论先行（三句话）

1. **场景已落地并且 `--check-scenario` / `card_window --dry-run` 全绿**：`placement` 到 D402(138,108)，
   `CM_TURN(3010)` 定朝向 → `CM_HIT(3014)` → 等 `SM_STRUCK(31)`。
2. **目标走「预置位 + 已知刷怪点」路线**（不是发现、不是宠物、不是挥空），理由与逐条 oracle 出处见 §2。
3. **两处必须如实登记的缺口**（都在 §5 / §6，不是「已解决」）：
   - **「没打中」支没有任何 s2c 协议帧**（只有明文 `+GD` ack，`ident=null`）⇒ 现有 DSL 的
     `wait_for` 集合**在语义上无法覆盖该支**；按卡面「先写设计说明、不硬凑、不改 DSL 语义」处理，并给出最小扩展建议。
   - **默认窗口夹具 `winplay0` 打不出伤害**（1 级法师，DC 0-1、HitPoint 5）：任何确定性刷怪点的怪 AC 都 ≥2
     ⇒ `GetHitStruckDamage` 归零 ⇒ 永远没有 `SM_STRUCK`。**真跑必须换夹具角色（推荐 `gm01`）**，见 §6。

---

## 1. 目标路线：为什么是「预置位 + 已知刷怪点」

卡面允许三条路，逐条评估（都有可复算的证据）：

| 路线 | 结论 | 证据 |
|---|---|---|
| ② 玩家自己的宠物/英雄 | **不可用** | 没有 `CM_*` 能凭空造宠：召唤要 `CM_SPELL` + 已学技能 + 蓝；`characters_magic` 里 `winplay0`（PlayerId 6228）**0 条**技能记录（`E:/tmp/conformance/baseline/mir2_test_db.sql` 的 `characters_magic` 段） |
| ③ 纯「打空气」 | **不可表达** | 挥空只回 `+GD` 明文 ack（§5.1）；probe 三步之内**必须有** `wait_for`/`wait_frames`/`wait_idents`（`mir2_probe.py:1099-1110`），挥空的等待只能超时 ⇒ 直接违反卡面「不许恒超时」 |
| ① 预置位 + 已知刷怪点 | **采用** | 见 §2 / §3：`placement` 由 `card_window.py` 在起栈前改 `characters.MapName/CX/CY`（`docs/window-runbook-20261023.md` §1.5），MonGen 里的刷怪行给出**精确格** |

**为什么不用「发现」**：`param_discovery.py` 的规则表只有 `self_x`/`self_y`（SM_LOGON）、
`merchant_actor`/`npc_actor`/`king_actor`（643）与 `computed`（`move_x`/`move_y`），对 `target_*` 无规则
（`docs/window-runbook-20261023.md` §4.5 表格：`combat_hit` 一行 `❌ 需新帧/新场景`）。本卡不改 `param_discovery.py`，
所以目标坐标只能来自场景声明的 `placement` 与部署件里的固定刷怪点。

---

## 2. 目标：D402(138,107) 的 `僵尸2`（唯一一条 Range=0 的可打怪）

`E:/tmp/conformance/csharp-root/M2GameSvr/Envir/MonGen.txt:325`：

```
D402  138   107   僵尸2             0    1    10
```

**字段必须按 oracle 的分词口径读**：`LocalDB.cs:12` 的 `TextSplitConst = { ' ', '\t' }` +
`HUtil32.GetValidStr3` 用 `Split(div, 2, StringSplitOptions.RemoveEmptyEntries)`（`src/OpenMir2/HUtil32.cs:384-390`）
⇒ **空字段被丢掉**。所以这一行是「Range=0 / 数量=1 / 重生 10 分钟 / 集中刷几率=0」，
刷出来的怪**恰好在 (138,107)**，不是随机散布。

> 反例（差点踩进去）：宝箱那批行 `D2052  50  50  宝箱  <空>  50  10  10` 看着像 Range=0，
> 但空字段被丢掉后是 **Range=50 / 数量=10** ⇒ 箱子散布在 (50,50)±50 的 101×101 格里，**不能当固定目标**。

怪的定义（`E:/tmp/conformance/baseline/mir2_test_data.sql:304`，与 `sql/mir2_data.sql` 同值）：

```
(53,'僵尸2',95,41,50,25,1,0,160,155,0,2,1,8,17,0,0,15,11,2000,1,0,3000,...)
 Idx Race RImg App Lvl U C Exp HP MP AC MAC DC DCMAX MC SC SPEED HIT WALK...
     95            25    155   2  1  8  17        15  11  2000
```

⇒ **AC=2 / SPEED=15 / HIT=11 / HP=155 / WALK_SPD=2000ms / Undead=1**。这三个数决定 §6 的可行性。

站位：`placement {map:"D402", x:138, y:108}` = 目标正南邻格。离线校验（`card_window.read_map_cell`，
判据 = oracle `Envirnoment.CanWalk(x,y,true)`：越界或 bit15 阻挡即不可走）：

```
(138,107) width=200 height=200 walkable=True  bk=0x0000 fr=0x0000
(138,108) width=200 height=200 walkable=True  bk=0x0cff fr=0x0000
```

`MapInfo.txt`：`[D402  废矿区东部]`（无 `|N` 别名 ⇒ ServerIndex 0，与两侧 `Server.conf` 的 `ServerIndex=0` 一致）。

---

## 3. 逐步 oracle 出处（每一步都写到 `file:line`）

### 3.1 线上字段口径（**这是本卡最容易写错的一处**）

`src/GameSrv/Word/WorldServer.cs:831-844`（`CM_HORSERUN/CM_TURN/CM_WALK/CM_SITDOWN/CM_RUN/CM_HIT…` 同段）：

```csharp
playObject.SendMsg(playObject, defMsg.Ident, defMsg.Tag,
                   HUtil32.LoWord(defMsg.Recog), HUtil32.HiWord(defMsg.Recog), 0);
```

对照 `SendMsg(IActor, wIdent, wParam, nParam1, nParam2, nParam3)`（`BaseObject.Message.cs:98`）：

| 线上字段 | 进到 `ProcessMessage` 的 | 含义 |
|---|---|---|
| `ident` | `wIdent` | 命令号 |
| `recog` | **`nParam1 = LoWord(recog)`、`nParam2 = HiWord(recog)`** | X / Y（**打包在 recog 里**） |
| `tag` | `wParam` | 方向 Dir |
| `param` | **不被读** | — |

⇒ `CM_HIT` 的 `recog` 必须是 **`(Y<<16)|X`**，不是「X 放 recog、Y 放 param」。
本场景 `self_pos = (108<<16)|138 = 7078026`，与 `placement` 同源。

> **顺带发现的既有缺陷（不在本卡写集，只登记）**：`tools/probe/scenarios/combat_hit.json` 用的是
> `recog=target_x, param=target_y`。按上面的口径，`HiWord(recog)` 会是 0 ⇒ `nY=0`，
> 而成功门要求 `nY == CurrY`（§3.3）⇒ **一击都不会被服务端接受**；再加上它只等 `SM_HIT(14)`
> （本方看不到自己的 14，§3.5）。两者叠加使它**恒超时**，与卡面「combat_hit 现在恒 rc=2」的结论一致。
> 修不修留给 `openmir2-window-discovery-rules` / 后续窗口卡裁决，本卡不动它。

### 3.2 `CM_TURN(3010)` —— 先定朝向（**不可省**）

- 命令号：`crates/protocol/src/messages.rs:103` `CM_TURN = 3010`；oracle `src/OpenMir2/Messages.cs:120`。
- 派发：`PlayObject.Message.cs:1008-1013` → `ClientChangeDir(wIdent, nParam1, nParam2, (byte)wParam, ref delayTime)`。
- 实现：`PlayObject.Operate.cs:310-339`，`if (nX == CurrX && nY == CurrY) { Dir = nDir; … }`。
- **为什么必须先转身**：`CM_HIT` 取目标用的是 `GetPoseCreate()`（`PlayObject.Attack.cs:26`），
  它读的是**状态化 `Dir`**（`BaseObject.cs:977-1030 GetFrontPosition` 按 `Dir` 算前格）；
  而 `Dir = nDir`（`BaseObject.Attack.cs:12`）发生在取目标**之后**的 `PlayObject.Attack.cs:75`。
  ⇒ **本击报文里的 `tag` 只影响动作帧与后续状态，救不了这一击打谁**。（Rust 侧同一结论写在
  `mir2-rs/bins/game-svr/src/lib.rs` 的 `hit_cmd_dispatches_ack_action_frame_and_world_attack` 注释里：
  「本帧 dir=4 与状态化朝向 0 相反 ⇒ 取目标必须用状态化朝向」。）
- 成功也会回 `+GD`（`PlayObject.Message.cs:1012`）。

### 3.3 `CM_HIT(3014)` —— 发起攻击

- 命令号：`crates/protocol/src/messages.rs:111` `CM_HIT = 3014`；oracle `src/OpenMir2/Messages.cs:136`。
  （**卡面正文写的 `CM_HIT(3005)` 是笔误**：`mir2-rs/crates/protocol/src/messages.rs:105` 的 3005 是 `CM_THROW`。）
- 派发：`PlayObject.Message.cs:1221-1230`，9 条攻击 ident 归一个处理器
  `ClientHitXY(wIdent, nParam1, nParam2, (byte)wParam, LateDelivery, ref delayTime)`。
- `ClientHitXY`：`PlayObject.Attack.cs:823-963`。关键节点：
  - `:832` `if (!IsCanHit) return false;`
  - `:836` 死亡/麻痹守卫
  - `:839-877` 限速检查（`CheckActionStatus` + `HitIntervalTime`）
  - **`:883` `if (nX == CurrX && nY == CurrY)`** ← 成功门（**必须点在自己所站的格**）
  - `:909-911` `case CM_HIT: AttackDir(null, 0, nDir);`（`wHitMode=0`）
  - `:945-959` 成功支扣体力/魔法点
- `AttackDir`：`PlayObject.Attack.cs:24-77`
  - `:26` `IActor attackTarget = targetObject ?? GetPoseCreate();`
  - `:73` `GetAttackPowerHit(...)`
  - `:75` `AttackDir(attackTarget, nPower, nDir)` → `BaseObject.Attack.cs:30-65 _Attack`
  - `:76` `SendAttackMsg(GetHitMode(wHitMode), Dir, CurrX, CurrY)` → `BaseObject.Attack.cs:173-176`
  - `:77` `AttackSuccess(...)`

### 3.4 命中 → `SM_STRUCK(31)`（本方视角）

`BaseObject.Attack.cs:30-65 _Attack`：

```
:37  if (IsProperTarget(targetObject))            // → BaseObject.Base.cs:470-473 → PlayObject.Attack.cs:627+
:39      if (targetObject.HitPoint > 0)           // 注意：判的是**目标**的 HIT
:41          if (HitPoint < RandomByte(targetObject.SpeedPoint)) nPower = 0;   // 躲闪门
:53  nPower = targetObject.GetHitStruckDamage(this, nPower);   // 扣目标 AC（BaseObject.Base.cs:789-806）
:57  targetObject.SendStruckDelayMsg(RM_REFMESSAGE, nPower, HP, MaxHP, ActorId, "", 200);
:61  if (targetObject.Race > ActorRace.Play)
:63      targetObject.SendMsg(targetObject, RM_STRUCK, nPower, HP, MaxHP, ActorId);
```

受击者收到 `RM_REFMESSAGE` 后 `SendRefMsg(RM_STRUCK, …)`（`BaseObject.Base.cs:701-706`），
视野内玩家再用 `PlayObject.Message.cs:1562-1628` 的 `RM_STRUCK` 支回 **`SM_STRUCK(31)`**
（`messages.rs:192` / `Messages.cs:197`），头 = `(受击者 ActorId, 受击后 HP, MaxHP, 伤害)`。

### 3.5 为什么等的是 31 而不是 14

`PlayObject.Message.cs:1433-1437`：

```csharp
case Messages.RM_HIT:
    if (processMsg.ActorId != ActorId) { ClientMsg = MakeMessage(SM_HIT, ...); SendSocket(ClientMsg); }
```

**自己那一击的 `RM_HIT` 会被这条 `ActorId != ActorId` 过滤掉**（`SendRefMsg` 会把 RM_HIT 也投给本人，
`BaseObject.Message.cs:383-…`，但本人处理时丢弃）⇒ **本方客户端永远收不到自己攻击的 `SM_HIT(14)`**。
Rust 侧同口径（`lib.rs` 的用例断言动作帧只发观察者 902、`+GD` 只发发起者 901）。

另外 `SM_HIT(14)` 在 `rules.json` 里是 `action=ignore` 且 `approved_by` 仍是 `PENDING`
（`docs/window-runbook-20261023.md` §1 第 5.1 条）——真跑时出现一条 14 会把整次窗口判 `UNDECIDABLE`。
本场景的 `wait_for` **刻意不包含 14**。

---

## 4. 场景文件本体（4 步）

| # | 动作 | 说明 |
|---|---|---|
| 1 | `send` 3010 `recog=self_pos` `tag=0` | 转正上（`Direction.Up = 0`） |
| 2 | `sleep` 400ms | `CheckActionStatus`（`PlayObject.Base.cs:2680-2760`）在不同操作之间要求 `ActionIntervalTime=350ms`（`GameSvrConf.cs:1815/1820`）；驱动器再叠默认 `--between-ms 200` |
| 3 | `send` 3014 `recog=self_pos` `tag=0` | 打正上格 —— 服务端 `GetPoseCreate()` 取到 (138,107) 的僵尸2 |
| 4 | `wait_for` **31**，3000ms | `SM_STRUCK(31)` |

`self_pos` 放在顶层 `params` 当默认值（和 `dice_chain` 的 `npc_actor=119` 同类），
**明确不计入「发现能力」**：它由 `placement` 推出（`(y<<16)|x`），改 `placement` 必须同步改 `params.self_pos`；
换一个落地格时用 `--var self_pos=<新值>` 覆盖，文件不动。

---

## 5. 打不中 = 零帧：DSL 无法表达，只能登记 + 给扩展建议

### 5.1 事实（逐条可查）

一次**被服务端接受**的攻击（`nX==CurrX && nY==CurrY` 门通过）之后，本方客户端只可能收到三类东西：

| 分支 | 服务端行为 | 本方看到什么 |
|---|---|---|
| 打中（`nPower>0`） | `_Attack` 里 `SendStruckDelayMsg(RM_REFMESSAGE)`（`BaseObject.Attack.cs:57`） | `SM_STRUCK(31)` |
| 未命中（躲闪门置 0）/ 伤害被 AC 吃成 0 | `if (nPower > 0)` 不成立（`BaseObject.Attack.cs:51-60`） | **没有任何协议帧** |
| 被拒（不在自己格 / 死亡 / 麻痹 / 限速） | `SendRefMsg(RM_MOVEFAIL,…)`（`PlayObject.Message.cs:1245-1252`） | **无**（`RM_MOVEFAIL` 经 `SendRefMsg` 只给观察者，本人不收） |

**唯独「被接受」这一件事**有一个可观测信号：明文 ack

```csharp
// PlayObject.Message.cs:1230-1234
if (ClientHitXY(...)) { ActionTick = …; SendSocket(M2Share.GetGoodTick); }
// M2Share.cs:205: GetGoodTick => string.Format(sSTATUS_GOOD, GetTickCount())
```

probe 把它记成 `frame_class = "server_ack_time"`、`ident = null`
（`tools/probe/scenarios/README.md`「证据文件」节；`mir2_probe.py:176-184`）。

### 5.2 为什么 `wait_for` 覆盖不了

`Conn.wait_for` 的匹配键是 `fr.ident in wanted`（`mir2_probe.py:291-303`）；明文 ack 的 `ident` 是 `None`
⇒ **DSL 里没有任何一步能等到它**。`wait_idents` / `wait_frames` 同样按 ident，且 `wait_frames` 只数累计帧数。
`ACTION_KEYS` 只有 `send/wait_for/wait_frames/wait_idents/sleep/say/click_npc/dlg_select`（`mir2_probe.py:338-339`）。

⇒ 按卡面第 8 条：**先出设计说明，不硬凑、不改 DSL 语义**。本卡**不**扩展 DSL。

### 5.3 最小扩展建议（留给后续卡裁决，本卡不实现）

三选一，按改动面从小到大：

1. **`wait_ack` 动作**（推荐）：`{"wait_ack": {"kind": "good_tick", "timeout_ms": 1000}}`，只匹配
   `frame_class=server_ack_time` 且 `body_hex` 以 `+GD/` 开头的帧。零语义改动（只是把已经落盘的那类帧
   暴露成可等待目标），比较器侧不受影响（`rules.json` 只按 ident 登记 s2c，ack 已是独立 `frame_class`）。
2. `wait_for` 允许特殊值 `"server_ack_time"`：把 `wanted` 从 `set[int]` 放宽成 `set[int|str]`，字符串只匹配 `frame_class`。
   改动更小但把两种匹配键混在一个字段里，可读性差。
3. 给 `wait_for` 加 `"frame_class": "server_ack_time"` 的可选伴随键。语义最干净，但要动加载期校验。

**无论哪一种，都必须补一条牙**：把攻击步骤删掉后 `--check-scenario`/dry-run 要红——现有 DSL 做不到（见 §7）。

---

## 6. 夹具前置：`winplay0` 打不出伤害（必须换角色）

`card_window.py` 的 `alt` profile 默认 `account/char_name = "winplay0"`（`card_window.py:222-224`）。
基线库里 `winplay0` 是：

```
mir2_test_db.sql:88   (6228,0,'winplay0','winplay0','B013',7,12, Level=1, Dir=3, Hair=1, Sex=0, Job=1=法师, …)
characters_ablity     (6228, Level=1, Ac=1, Mac=0, Dc=0, …, Hp=16, Mp=17, MaxHP=16, MaxMP=18, …)
characters_magic      PlayerId=6228 → 0 条技能
```

按 oracle 重算：

| 量 | 值 | 出处 |
|---|---|---|
| `Abil.DC` | 法师 1 级：`n = nLevel/7 = 0` ⇒ `MakeWord(max(-1,0), max(1,0)) = 0-1` | `PlayObject.cs:5288` |
| `GetBaseAttackPoewr()` | `GetAttackPower(0, 1)` = `0 + Random(2)` ∈ **{0,1}** | `BaseObject.Attack.cs:25-28` + `PlayObject.cs:2909-2939` |
| `HitPoint` | 法师也走 `DEFHIT(5) + BonusAbil.Hit/…` = **5**（无技能、无装备） | `PlayObject.cs:3668/3673`；`MessageSettings.cs:10 DEFHIT=5` |
| 对 `僵尸2` 的伤害 | `max(0, {0,1} - AC=2)` = **恒 0** | `BaseObject.Base.cs:789-806` |

⇒ **`winplay0` 打任何确定性刷怪点的怪都是 0 伤害**（§6.1 表里所有候选 AC ≥ 2），
`if (nPower > 0)` 不成立 ⇒ **永远没有 `SM_STRUCK`**。这不是场景写错，是夹具不够。

**推荐夹具 `gm01`**（`characters` Id=1，账号 `admin`）：

| 量 | 值 | 依据 |
|---|---|---|
| Job / Level | 战士 / 40 | `mir2_test_db.sql` characters 第 1 行 |
| `Abil.DC` | `MakeWord(max(40/5-1,1)=7, max(1,40/5)=8)` = **7-8** | `PlayObject.cs:5300` |
| 技能 | `characters_magic`：MagicId **3 = 基本剑法**，**Level=3** | `MagicConst.cs:16`；`characters_magic` (2269,3,3) |
| `HitPoint` | `5 + Round(9/3.0*3)` = **14** | `PlayObject.cs:3690` |
| 对 `僵尸2`（SPEED=15） | 命中门 `14 < Random(15)` **永不成立** ⇒ **必中**；伤害 `7-8 - AC 2 = 5-6 > 0` ⇒ **必有 `SM_STRUCK`** | `BaseObject.Attack.cs:39-57` |

跑法（不改 profile 文件，走环境变量覆盖，`card_window.py:596-612`）：

```powershell
$env:CONFORMANCE_ACCOUNT="admin"; $env:CONFORMANCE_CHAR_NAME="gm01"
python -I -B tools/conformance/card_window.py --card openmir2-scenario-combat-strike `
  --scenario tools/probe/scenarios/combat_strike.json --out E:/tmp/win-combat-strike --profile alt
```

**这是窗口批次的前置条件，本卡没有替你换 profile**（`card_window.py` 不在本卡写集）。
若窗口决定继续用 `winplay0`，那这条场景**必须**改成「先给号穿装备/学技能」的背包夹具（工具当前不支持，
见 `docs/window-runbook-20261023.md` §1.5 末条），否则恒超时。

### 6.1 为什么不是别的怪（离线筛选结果，可复算）

按 oracle 分词口径扫全部 `MonGen.txt`，**Range=0 且 集中刷几率=0** 的刷怪行只有 92 条，其中
「名字能在 `monsters` 表里解析出来」的 30 条里，AC/SPEED 如下（读 `sql/mir2_data.sql` + 部署 `MonGen.txt` 离线复算）：

| 刷怪行 | 怪 | AC | SPEED | HP | 问题 |
|---|---|---:|---:|---:|---|
| `D402:325` | 僵尸2 | **2** | 15 | 155 | **采用**：AC 最低、非 Boss |
| `D717:10` / `H006:11,12` | 石墓尸王 | 10 | 15 | 3000 | AC 10 ⇒ 只有 Lv255 战士打得穿 |
| `D515:3033` / `F011:3078` | 祖玛教主 | 20 | 32 | 3000 | Boss，DC 40-65，1 级/40 级都秒 |
| `D10062:3035` / `D10071:3073` | 赤月恶魔 | 18 | 25 | 5000 | Boss + `WALK_SPD=0`（每 tick 都走） |
| `4:9` | 千年树妖 | 20 | 35 | 5000 | Boss |
| `4:89-109` | 恶魔弓箭手 | 30 | 18 | 4000 | **守卫**（race 112 `ArcherGuard`），打它会招来箭雨 |
| `0/2/3:*` | 弓箭护卫 | 30 | 18 | 4000 | 同上 |

**没有任何一条确定性刷怪点的怪是 `HIT=0` 或 `AC=0`。** 唯二满足「必中」的怪是
`宝箱`（HIT=0、AC=0、SPEED=0）与 `角蝇`（SPEED=1），但它们的刷怪行一旦按真口径读出来就是
**Range=50/100 的散布**（§2 反例），拿不到固定格。

另外**不要去碰「藏在墙里不动」的捷径**：`宝箱`/`角蝇` 都是 `StickMonster` 一族，出生时 `FixedHideMode=true`，
而 `SendRefMsg` 在 `ObMode || FixedHideMode` 时**只发给自己**（`BaseObject.Message.cs:390-395`）
⇒ 在它「出土」（`RM_DIGUP`）之前，打中它**也不会**广播 `RM_STRUCK`，本方收不到 31。
Rust 侧是否复刻了这条 `FixedHideMode` 门控**尚无同口径用例**，把它写进夹具会引入一个与场景无关的 parity 风险。

---

## 7. 牙（≥1）+ sha256

场景文件提交后的 sha256：**`0c8e5670fe7fb470fc704a4d410bb7698e1af4c6226fbcf418c1f0a1a69685f3`**
（`sha256sum tools/probe/scenarios/combat_strike.json`，两次改坏-还原后读数一致）。

| # | 怎么改坏 | 期望 | 实测 |
|---|---|---|---|
| T1 | 删掉 `wait_for` 那一步（只剩 turn/sleep/hit） | `--check-scenario` 红 | `scenario-error: scenario 'combat_strike' has no wait step…` **rc=2** |
| T2 | `placement` 改到 D402 的阻挡格 (0,0)（`bk=34723=0x87a3`，bit15 置位） | `card_window --dry-run` 红 | `[csharp] 预置位…不可行 ⇒ 真跑会 rc=2` **rc=2** |
| — | 两次都逐字节还原 | sha256 不变 | `0c8e5670…85f3`（改前=改后） |

**卡面要求的「去掉攻击那一步就应红」做不到，如实登记**：`--check-scenario` 只做**形状**校验
（`mir2_probe.py:1096-1110`：未知键 / 类型 / 范围 / 缺 required / 无 wait 步），
删掉中间的 `send` 步骤不会让任何门变红。真正能把「这一击打没打」判红的只有**真跑**（`wait_for 31` 超时 ⇒ rc=1），
或 §5.3 的 DSL 扩展落地后的新牙。这是工具层的缺口，不是本场景的疏漏。

---

## 8. 覆盖到哪几张卡 / 覆盖不到什么（如实）

**能覆盖（作为窗口输入）**：
`monster-attack-hit-frame`（`SM_STRUCK` 头的 `(受击者, HP, MaxHP, 伤害)`）、
`rm-refmessage-delay-layer`（`RM_REFMESSAGE` 200ms 延迟层 → 广播 → 观察者回帧的次序）、
`attacktarget-overrides` / `proper-target-unfolded`（`GetPoseCreate` 取前格 + `IsProperTarget`）、
`weapon-durability-frames`（`CheckWeaponUpgrade` / `UseItems[Weapon]` 在 `AttackDir` 前段）、
`run-gap-pl1/mb1/mb3/mb4` 中与「玩家主动一击」相关的那部分。

**覆盖不到（必须由别的场景/构造级用例承担）**：
- **「没打中」支**：零协议帧（§5），DSL 表达不了。
- **本方自己的 `SM_HIT(14)`**：oracle 根本不发（§3.5）。
- **`CM_HEAVYHIT/CM_BIGHIT/…` 另外 8 条攻击 ident**：本场景只打 `CM_HIT`；它们的差异需要各开一击（或后续把 `ident`
  参数化——现在 `send.ident` 只收 int，但顶层 `params` 里放 int 再 `--var` 覆盖也可行；留待窗口批次按卡决定）。
- **多观察者语义**（两张号看同一击）：需要第二会话；本场景单会话。
- **`SM_DEATH`**：僵尸2 HP=155、我方伤害 5-6 ⇒ 不会一击致死，本场景不产生 `SM_DEATH`。

---

## 9. 门禁（本卡实际跑过的命令与 rc）

| 命令 | rc |
|---|---|
| `python -I -B tools/probe/mir2_probe.py --check-scenario --scenario tools/probe/scenarios/combat_strike.json` | **0**（`scenario OK: combat_strike (4 steps, params=self_pos=7078026)`） |
| 同上 + `--var self_pos=7078026` | **0** |
| `python -I -B tools/probe/mir2_probe.py --selftest` | **0**（8 checks OK） |
| `python -I -B tools/conformance/card_window.py --card openmir2-scenario-combat-strike --scenario tools/probe/scenarios/combat_strike.json --profile alt --dry-run` | **0**（两侧 `placement` 离线校验 200x200 可走） |
| T1 / T2（§7） | 2 / 2（改坏必红），还原后 sha256 一致 |

未跑 `python -I tools/whitelist/t_reservation_gate.py check`：本卡**没有**动任何 `T-*` 登记
（`mir2-rs/tests/parity/T-RESERVATIONS.md` 未改），该门只在动 `T-*` 时适用。

---

## 10. 残留物 / 未落地项

1. **`rules.json` 缺 `31`（`SM_STRUCK`）**：`tools/conformance/rules.json` 现在登记 37 个 s2c ident，
   含 14/100 但**不含 31**。按 `docs/window-runbook-20261023.md` §1 第 5 条，未登记 ident 出现会让整次窗口判
   `UNDECIDABLE`。**`tools/conformance/` 不在本卡写集**（写集 = `tools/probe/scenarios/`、`tools/probe/README.md`、`docs/`），
   所以本卡只登记、不改：真跑前必须由 rules 卡（建议并到 `openmir2-conformance-rules-idents`）给 31 补一条规则
   （注意 31 的头是 `(受击者, HP, MaxHP, 伤害)`，HP/伤害都随 RNG 变，需要按字段窄掩码而不是整帧 ignore）。
2. **`combat_hit.json` 的 recog/param 口径是错的**（§3.1 末），本卡未改（避免与 `openmir2-window-discovery-rules` 争写集）。
3. **profile 默认角色 `winplay0` 打不动怪**（§6），需要窗口批次换夹具角色或补背包夹具。
4. 本卡期间生成的临时脚本在 `E:/tmp/w71/`（纯离线分析：解析 MonGen / monsters / 基线库 / `.map`），不属于仓库内容。
