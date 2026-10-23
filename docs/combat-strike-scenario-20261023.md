# R3 场景 `combat_strike`：主动一击 + 命中帧（卡 `openmir2-scenario-combat-strike`）

- 车道：`wt-worker-71` / principal `openmir2-worker-71` / 分支 `card/scenario-combat-strike`
- tier：**L**（只造场景夹具与说明；零生产代码、零帧字节）
- 本篇是 **request_changes（review `3c15d4ed`）后的返工版**：F1 换目标并给出完整判定表；F2 继续如实登记；
  F5 修正 ident 14 的措辞；F3/F4 只登记并指明归属卡。
- 未起栈、未连库、未占端口。本文所有数字都是离线推导：读 `.map`、部署 `Envir/`、`sql/`、`E:/tmp/conformance/baseline/*.sql`。

---

## 0. 结论先行

1. **F1 采纳**：`僵尸2`（Race=95 `DigOutZombi`）出生即 `FixedHideMode=true`，按 oracle 的隐藏视野门**永远不会出土**，
   `SendRefMsg` 的隐藏早退还会吞掉 `RM_STRUCK` 广播 ⇒ `wait_for 31` 恒超时。原目标作废。
2. **目标已换成非 FixedHide 的确定性刷怪点**：map 4 封魔谷 (256,231) 的 **`恶魔弓箭手`**（Race=112 `ArcherGuard`，
   `FixedHideMode=false`），`placement` 到 (256,232) 面向正上。
3. **但这条场景现在仍然是 BLOCKED-FIXTURE**：把所有「确定性刷怪点 × 基线角色」两两算过，
   **没有任何一对能同时满足「打穿目标 AC」和「过目标躲闪门」**（最好的一对是 `m3st0` 打 `恶魔弓箭手`，
   单击命中率 **33%**）。所以它现在跑起来会 `wait_for` 超时 —— 场景文件里已写 **BLOCKED** 字样，
   不许当窗口输入。缺什么夹具见 §6。
4. **F2 如实登记**：卡面「删掉攻击步 ⇒ `--check-scenario` / dry-run 应红」这条判据**本卡做不到**，
   工具对 ident 语义零校验（reviewer 实测 `wait_for=4095` 也 rc=0）。本卡**不**把它写成 PASS，
   也不改工具（不在写集）；另开工具卡的建议见 §7.3。

---

## 1. F1：目标重选与判定表

### 1.1 「确定性刷怪点」的全集（可复算）

判定口径全部按 oracle 源码，不按肉眼读文本：

- 分词：`LocalDB.cs:12` `TextSplitConst = { ' ', '\t' }` + `HUtil32.GetValidStr3` 的
  `Split(div, 2, StringSplitOptions.RemoveEmptyEntries)`（`src/OpenMir2/HUtil32.cs:384-390`）⇒ **空字段被丢弃**。
- 解析序：`LocalDB.cs:536-558` = Map, X, Y, MonName(`GetValidStrCap`), Range, Count, Zen, MissionGenRate。
- Range=0 时落格是精确的：`WorldServer.MonGen.cs:849-851`
  `nX = (X - Range) + Random(Range*2+1)` ⇒ `X + Random(1)`，而 `Random.Next(1) ≡ 0`（`RandomNumber.cs:93-99`）。
- 名字必须能在 `monsters` 表里查到：`WorldServer.cs:167-175 GetMonRace`（查不到返回 **-1**），
  而 `WorldServer.MonGen.cs:819` `if (monGen.Race > 0)` 才建怪 ⇒ **Race=-1 的行根本不刷**。

按这套口径扫部署 `Envir/MonGen.txt`（sha256 `e5c39fb7…748f`，三个部署根一致）：

- `Range=0 且 MissionGenRate=0` 的行共 **92** 行；其中名字能在 `monsters` 表解析出来的只有 **30** 行
  （其余 62 行的名字如 `弓箭护卫`/`沃玛教主9`/`练功师10`/`飞火流星`/`魔龙教主2` 在表里没有，
  Race=-1，**不会刷**——上一版我把 `弓箭护卫` 误当成候选，这次已剔掉）。
- 这 30 行只落到 **4 个 Race**：95、102、112、115。

### 1.2 Race → 类 → 隐藏/石化门（逐条 `file:line`）

| Race | 常量出处 | `CreateMonster` 分支 | 实例类 | `FixedHideMode` | `StoneMode` | 依据 |
|---:|---|---|---|---|---|---|
| **95** | `src/OpenMir2/Enums/Race.cs:82` `MonsterDigoutzombi` | `WorldServer.MonGen.cs:602-603` `new DigOutZombi()` | `DigOutZombi` | **true** | false | `DigOutZombi.cs:15`；`BaseObject.cs:2517-2520`（`if (this is DigOutZombi) FixedHideMode = true;`） |
| **102** | `Race.cs:89` `MonsterScultureking` | `MonGen.cs:635-636` `new ScultureKingMonster()` | `ScultureKingMonster` | false | **true** | `BaseObject.cs:2533-2536`（`StoneMode = true; CharStatusEx = STONEMODE;`） |
| **112** | `Race.cs:90` `MonsterArcherguard` | `MonGen.cs:659-660` `new ArcherGuard()` | `ArcherGuard : GuardUnit : NormNpc : AnimalObject` | **false** | false | 不在 `FixedHideMode = true` 的任何一处（全仓只有 7 处：`DigOutZombi.cs:15`、`StickMonster.cs:21/68`、`WhiteSkeleton.cs:16`、`ElfMonster.cs:18`、`ElfWarriorMonster.cs:30`、`Merchant.cs:148`、`PlayObject.cs:993`） |
| **115** | `Race.cs:93` `RoBot` | `MonGen.cs:668-669` `new BigHeartMonster()` | `BigHeartMonster : AnimalObject` | false | false | 同上 |

**为什么 95 是死路（reviewer F1 的链路，我复核成立）**：

1. 出生即隐藏：`DigOutZombi.cs:15` + `BaseObject.cs:2517-2520`。
2. 出土只有一条路：`DigOutZombi.Run()` 的隐藏支遍历**它自己的** `VisibleActors`（`DigOutZombi.cs:18-42`），
   条件满足才 `Turned()`（`:44-50`，置 `FixedHideMode=false` 并发 `RM_DIGUP`）。
3. 谁往怪物 `VisibleActors` 里塞人：玩家 `SearchViewRange` 里那段
   `if (baseObject.CellType == CellType.Monster && !ObMode && !FixedHideMode) … SendMessage(baseObject.ActorId, RM_UPDATEVIEWRANGE, …)`
   —— **`!FixedHideMode` 直接把它挡掉**（`PlayObject.cs:3125-3139`）；外侧还有一层
   `if (!baseObject.Ghost && !baseObject.FixedHideMode && !baseObject.ObMode)`（`PlayObject.cs:3131`）。
4. 怪物自搜只发生在 guard/slave 一支：`WorldServer.MonGen.cs:302-311`
   （`IsSlave || Race is Guard or ArcherGuard or SlaveMonster`）。
5. ⇒ 先于玩家登录刷出的隐藏掘尸僵尸**学不到「旁边有人」**，永不出土。
6. 即便打中它也白打：`SendRefMsg` 在 `ObMode || FixedHideMode` 时只发给自己（`BaseObject.Message.cs:390-395`），
   `RM_REFMESSAGE → RM_STRUCK` 的广播（`BaseObject.Base.cs:701-706`）出不去 ⇒ 客户端收不到 `SM_STRUCK(31)`。

### 1.3 换成的目标：map 4 (256,231) `恶魔弓箭手`

`Envir/MonGen.txt:90`：

```
4     256   231   恶魔弓箭手      0     1     10
```

按 §1.1 口径 = Map `4` / X `256` / Y `231` / Range `0` / Count `1` / Zen `10` 分钟 / MissionGenRate `0`
⇒ **精确落格**。怪定义（`E:/tmp/conformance/baseline/mir2_test_data.sql` 的 `monsters` 段，与 `sql/mir2_data.sql` 同值）：

```
(139,'恶魔弓箭手',112,45,47,99,0,100,1,4000,30,30,30,50,120,0,0,18,15,500,...)
      Race=112        Lvl=99  HP=4000 AC=30 MAC=30 DC=50..120  SPEED=18 HIT=15 WALK_SPD=500
```

**为什么它不会重蹈覆辙（逐条对照 §1.2）**：

| 掘尸僵尸的死因 | 恶魔弓箭手的情况 |
|---|---|
| `FixedHideMode=true`，视野更新被 `!FixedHideMode` 挡掉 | `ArcherGuard` 不在任何 `FixedHideMode=true` 的名单里 ⇒ **false** |
| 不会出土 ⇒ 没有 `SM_DIGUP` 见证点 | 无需出土；登录即可见（`ViewRange=12`，`ArcherGuard.cs:12-19`） |
| 隐藏时 `SendRefMsg` 只发自己 ⇒ 收不到 31 | 非隐藏 ⇒ `BaseObject.Base.cs:701-706` 的 `SendRefMsg(RM_STRUCK)` 正常广播 |
| 会乱走（出土后 `Wondering`） | 见下 |

「会不会乱走」：怪物在**被玩家看到之前是休眠的**（`WorldServer.MonGen.cs:283-297`：
`if (!monster.IsVisibleActive && ProcessRunCount < ProcessMonsterInterval) ProcessRunCount++; else Run()`），
所以从起栈到登录这段时间它不动；玩家一登录，`ArcherGuard.Run`（`ArcherGuard.cs:44-81`）立刻锁定目标
（`ViewRange=12`，靠 `SetTargetCreat`），随后 `MonsterObject.Run` 走的是「有目标 ⇒ 攻击」而不是 `Wondering()`
（`MonsterObject.cs:342-357`）⇒ **出手时它仍在 (256,231)**。它本身也不会走位（`ArcherGuard` 没有 `GotoTargetXY`）。

站位与可走性（`card_window.read_map_cell`，判据 = oracle `Envirnoment.CanWalk(x,y,true)`）：

```
(256,231) walkable=True bk=0x0387 fr=0x0000   ← 目标格
(256,232) walkable=True bk=0x0387 fr=0x0000   ← 站位格（正南邻格）
map 4 = 封魔谷，500x500，MapInfo 行 '[4 封魔谷] DAY'（无 |N 别名 ⇒ ServerIndex 0，与两侧 Server.conf 一致）
```

### 1.4 为什么不是另外三个 Race

| 候选 | 问题 |
|---|---|
| Race 102 `祖玛教主` | 出生 `StoneMode=true`（`BaseObject.cs:2533-2536`），而 `IsAttackTarget` 对 `targetObject.StoneMode` **直接返回 false**（`BaseObject.Base.cs:384-387`）⇒ 未「醒来」前根本打不中；AC 20 / SPEED 32 也最差 |
| Race 115 `赤月恶魔` | AC 18 / SPEED 25，且 `WALK_SPD=0`（DB）⇒ 每次 `Run` 都满足 `(now - WalkTick) > WalkSpeed`，站位最不稳 |
| Race 115 `千年树妖` | AC 20 / SPEED 35，命中率最差；`WALK_SPD=400` |

⇒ 112 的 `恶魔弓箭手` 是「非隐藏 + AC/SPEED 最低 + 不移位」的唯一合理选择。

---

## 2. oracle 链（协议层，逐条 `file:line`；与上一版一致）

### 2.1 线上字段口径（最容易写错的一处）

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

⇒ `CM_HIT` 的 `recog` 必须是 **`(Y<<16)|X`**。本场景 `self_pos = (232<<16)|256 = 15204608`，与 `placement` 同源。

### 2.2 `CM_TURN(3010)` —— 先定朝向（不可省）

- 命令号：`crates/protocol/src/messages.rs:103`；oracle `src/OpenMir2/Messages.cs:120`。
- 派发：`PlayObject.Message.cs:1008-1013` → `ClientChangeDir(wIdent, nParam1, nParam2, (byte)wParam, ref delayTime)`。
- 实现：`PlayObject.Operate.cs:310-339`，`if (nX == CurrX && nY == CurrY) { Dir = nDir; … }`。
- **为什么必须先转身**：取目标用 `GetPoseCreate()`（`PlayObject.Attack.cs:26`），读的是**状态化 `Dir`**
  （`BaseObject.cs:1253-1261` + `GetFrontPosition` `BaseObject.cs:977-1030`）；而 `Dir = nDir`
  （`BaseObject.Attack.cs:12`）发生在取目标**之后**（`PlayObject.Attack.cs:75`）⇒ 报文 `tag` 只影响动作帧。

### 2.3 `CM_HIT(3014)` —— 发起攻击

- 命令号：`messages.rs:111` / `Messages.cs:136`。（卡面正文的 `CM_HIT(3005)` 是笔误，3005 = `CM_THROW`。）
- 派发：`PlayObject.Message.cs:1221-1230` → `ClientHitXY(wIdent, nParam1, nParam2, (byte)wParam, …)`。
- `ClientHitXY`（`PlayObject.Attack.cs:823-963`）：
  - `:832` `IsCanHit` 守卫、`:836` 死亡/麻痹守卫、`:839-877` 限速（`CheckActionStatus` + `HitIntervalTime`）
  - **`:883` `if (nX == CurrX && nY == CurrY)`** ← 成功门（必须点在自己所站的格）
  - `:909-911` `case CM_HIT: AttackDir(null, 0, nDir);`
- `AttackDir`（`PlayObject.Attack.cs:24-77`）：`:26` 取目标 → `:73` 攻击力 → `:75` `_Attack` → `:76` 动作帧 → `:77` `AttackSuccess`。

### 2.4 命中 → `SM_STRUCK(31)`

`BaseObject.Attack.cs:30-65`：

```
:37  if (IsProperTarget(targetObject))
:39      if (targetObject.HitPoint > 0)                      // 判的是**目标**的 HIT
:41          if (HitPoint < RandomByte(targetObject.SpeedPoint)) nPower = 0;   // 躲闪门
:51  if (nPower > 0)
:53      nPower = targetObject.GetHitStruckDamage(this, nPower);   // 扣目标 AC（BaseObject.Base.cs:789-806）
:57      targetObject.SendStruckDelayMsg(RM_REFMESSAGE, …);
:61  if (targetObject.Race > ActorRace.Play)
:63      targetObject.SendMsg(targetObject, RM_STRUCK, …);
```

受击者收 `RM_REFMESSAGE` 后 `SendRefMsg(RM_STRUCK, …)`（`BaseObject.Base.cs:701-706`），
玩家侧再按 `PlayObject.Message.cs:1562-1628` 回 `SM_STRUCK(31)`（`messages.rs:192` / `Messages.cs:197`）。

### 2.5 为什么等 31 而不是 14

`PlayObject.Message.cs:1433-1437` 的 `if (processMsg.ActorId != ActorId)` 把**自己那一击**的 `RM_HIT` 过滤掉
⇒ 本方客户端**永远收不到自己的 `SM_HIT(14)`**（Rust 侧同口径，见 `lib.rs` 的
`hit_cmd_dispatches_ack_action_frame_and_world_attack`：动作帧只发观察者、`+GD` 只发发起者）。

**F5 修正后的措辞**：`14` 在 `rules.json` 里是 `action=ignore`、`approved_by` 仍是 `PENDING`
（`docs/window-runbook-20261023.md` §1 第 5.1 条）。**「出现一条 14」本身不会判 UNDECIDABLE** ——
它会按 ignore 规则整帧剔除；**UNDECIDABLE 来自 `pending_ignore_rule`**：`compare_runs.py` 发现这条规则
真的剔了帧、而 `approved_by` 还是 PENDING，就判不可判（runbook:61-67）。本场景 `wait_for` 不含 14 的两个理由：
① 本方本来就收不到自己的 14；② 它是 PENDING ignore，窗口批次要先清（`openmir2-conformance-rules-idents`）。

---

## 3. 场景文件本体（4 步）

| # | 动作 | 说明 |
|---|---|---|
| 1 | `send` 3010 `recog=self_pos` `tag=0` | 转正上（`Direction.Up = 0`） |
| 2 | `sleep` 400ms | `CheckActionStatus`（`PlayObject.Base.cs:2680-2760`）要求不同操作间隔 ≥ `ActionIntervalTime=350ms`（`GameSvrConf.cs:1815/1820`），再叠默认 `--between-ms 200` |
| 3 | `send` 3014 `recog=self_pos` `tag=0` | 打正上格 → `GetPoseCreate()` 取到 (256,231) 的恶魔弓箭手 |
| 4 | `wait_for` **31**，3000ms | `SM_STRUCK(31)` |

`self_pos` 放顶层 `params` 当默认值（同 `dice_chain` 的 `npc_actor=119`），**不计入「发现能力」**；
由 `placement` 推出，改 `placement` 必须同步改它，换落地格时可用 `--var self_pos=<新值>` 覆盖。

---

## 4. 打不中 = 零帧：DSL 表达不了（原样保留的登记）

| 分支 | 服务端行为 | 本方看到什么 |
|---|---|---|
| 打中（`nPower>0`） | `SendStruckDelayMsg(RM_REFMESSAGE)`（`BaseObject.Attack.cs:57`） | `SM_STRUCK(31)` |
| 未命中 / 伤害被 AC 吃成 0 | `if (nPower > 0)` 不成立（`:51-60`） | **没有任何协议帧** |
| 被拒（不在自己格 / 死亡 / 麻痹 / 限速） | `SendRefMsg(RM_MOVEFAIL,…)`（`PlayObject.Message.cs:1245-1252`） | **无**（只给观察者） |

「被接受」唯一可观测的信号是明文 ack（`PlayObject.Message.cs:1230-1234` → `M2Share.cs:205`），
probe 记为 `frame_class=server_ack_time`、`ident=null`（`mir2_probe.py:176-184`），
而 `Conn.wait_for` 按 `fr.ident` 匹配（`mir2_probe.py:291-303`）⇒ **DSL 里没有任何一步能等它**。

⇒ 按卡面第 8 条不改 DSL。最小扩展建议（留后续卡裁决）：
① 新增 `wait_ack`（只匹配 `frame_class=server_ack_time` 且 `body_hex` 以 `+GD/` 开头）；
② `wait_for` 允许 `"server_ack_time"` 字符串值；③ `wait_for` 加伴生键 `"frame_class"`。

---

## 5. 牙（≥1）+ sha256

sha256（提交后的树）：**`9f04ae0808d02b13fcd6f63482316cc0a0891c2a7d7e1befe76ec27770cf9a9a`**
（`sha256sum tools/probe/scenarios/combat_strike.json`；T1~T4 四次改坏后都逐字节还原，读数不变）。

| # | 怎么改坏 | 期望 | 实测 |
|---|---|---|---|
| T1 | 删掉 `wait_for` 那一步 | `--check-scenario` 红 | rc=2（`has no wait step`） |
| T2 | `placement` 改到越界/阻挡格 | `card_window --dry-run` 红 | rc=2 |
| T3 | **删掉攻击步（`send` 3014）** | **卡面要求红** | **rc=0 —— 工具层缺口，不写成 PASS（F2）** |
| T4 | `wait_for` 改成不存在的 ident 4095 | 理想应红 | rc=0 —— 同一缺口（reviewer 实测） |

两次改坏都逐字节还原，sha256 前后一致。

### 5.3 关于 F2 的处置

- **不把它写成 PASS**：卡面这条判据在本卡**未达成**，原因在工具（`mir2_probe.py:1096-1110` 只查形状 + 至少一个 wait 步），
  不在场景。
- 治理建议（另开卡，不在本卡写集）：给 `--check-scenario` 补
  ① `send.ident ∈ HIT_IDENTS` 时要求存在 `wait_for` 且 recog 形态可判；② `wait_for` 的 ident 必须在
  `messages.rs`/`rules.json` 里存在；③ 攻击场景必须「先 send 后 wait」。落地后再补这条牙。

---

## 6. 夹具矩阵：为什么现在打不出命中（F1 的「缺什么」）

### 6.1 基线夹具的全部可用角色能力

- `characters_bonusability` 全表**只有 1 行种子** `(13339,1,2,3,4,5,6,7,8,9,10)` ⇒ 其余角色 `BonusAbil = 0`。
- `characters_magic` 全表**只有 8 行**；只有 `PlayerId=1`（`gm01`）有 `MagicId 3 = 基本剑法`（`MagicConst.cs:16`），**Level 3**。
- `characters_bagitem` 里 `winplay0`/`m3st*` 全是零物品（`m3st*` 46 行全 0），`gm01` 的 12 件都在包裹位（0..11），
  没有穿在身上的 DC/准确装备。

| 角色 | Job/Level | `Abil.DC`（出处） | `GetBaseAttackPoewr` | `HitPoint` |
|---|---|---|---|---|
| `winplay0` | 法师 1 | `MakeWord(max(1/7-1,0),max(1,0)) = 0-1`（`PlayObject.cs:5288`） | `0+Random(2) ∈ {0,1}` | `DEFHIT=5`（`MessageSettings.cs:10`；`PlayObject.cs:3668/3673`） |
| `gm01` | 战士 40 | `MakeWord(max(40/5-1,1),max(1,40/5)) = 7-8`（`PlayObject.cs:5300`） | `7+Random(2) ∈ {7,8}` | `5 + Round(9/3.0*3) = 14`（`PlayObject.cs:3690`） |
| `m3st0`/`m3sa0`/`m3se0`/`m3p2c200` | 战士 255 | `MakeWord(max(255/5-1,1),max(1,51)) = 50-51`（`PlayObject.cs:5300`） | `50+Random(2) ∈ {50,51}` | **5**（无技能、无 bonus） |

### 6.2 目标侧的四个门槛数

| 目标 | AC | SPEED | DC | 备注 |
|---|---:|---:|---|---|
| 恶魔弓箭手（112） | 30 | 18 | 50-120 | 非隐藏、不移位 ⇒ **本卡选它** |
| 祖玛教主（102） | 20 | 32 | 40-65 | 另有 `StoneMode` 门 |
| 赤月恶魔（115） | 18 | 25 | 25-70 | `WALK_SPD=0` |
| 千年树妖（115） | 20 | 35 | 20-50 | `WALK_SPD=400` |

### 6.3 两两组合（伤害 = `max(0, DC_min - AC)`；命中率 = `P(HitPoint ≥ Random(SPEED))` = `(HitPoint+1)/SPEED`）

| 角色 × 目标 | 伤害 | 单击命中率 | 能否出 `SM_STRUCK` |
|---|---:|---:|---|
| `winplay0` × 任一 | 0 | — | **否**（伤害恒 0） |
| `gm01` × 任一 | `max(0, 7-AC) = 0`（AC 均 ≥18） | — | **否**（伤害恒 0） |
| `m3st0` × 恶魔弓箭手 | 20-21 | 6/18 ≈ **33%** | 概率，**非确定** |
| `m3st0` × 赤月恶魔 | 32-33 | 6/25 = 24% | 概率 |
| `m3st0` × 祖玛教主 | 30-31 | 6/32 ≈ 19% | 概率（+ StoneMode） |
| `m3st0` × 千年树妖 | 30-31 | 6/35 ≈ 17% | 概率 |

⇒ **没有任何一对能给出确定的 `SM_STRUCK`**：能打穿 AC 的角色（`m3st*`）准确只有 5；
准确够的角色（`gm01`，14）攻击力只有 7-8，打不穿 AC ≥18。
而且 33% 单击成功率也不能靠「多打几下」补：`wait_for` 第一次没中就整步失败（`mir2_probe.py:699-712`），
`wait_idents` 又是累计口径，会被「怪打我们」产生的 31（`recog=self`）冒充 ⇒ 恒过。

### 6.4 需要什么夹具（这是本卡的「缺什么」结论）

**一个同时满足 `DC > 目标 AC` 与 `HitPoint ≥ 目标 SPEED - 1` 的角色**。以本卡目标（AC 30 / SPEED 18）为例：

- `DC ≥ 31`：`gm01`（7-8）需要一把 **DC ≥ 24** 的武器，或换 `m3st*`（50-51，已够）；
- `HitPoint ≥ 17`：`m3st*`（5）需要 **`BonusAbil.Hit ≥ 240`**（`DEFHIT(5) + Hit/20 ≥ 17`，`GameSvrConf.cs:1413` 的
  `BonusAbilofWarr.Hit = 20`），或补 **基本剑法 Lv3 + 精神力战法 Lv3**（5+9+8 = 22，`PlayObject.cs:3690/3696`）。

落地方式（任一）：给 `characters_bonusability` 补一行、给 `characters_magic` 补两条、或在 `characters_bagitem`
里放一把已穿戴的 DC 武器。**这些都超出本卡写集**（要改测试库基线），建议开一张「combat 夹具」卡。

内容侧的另一条出路（不改夹具）：**加一条 `Range=0` 且目标 `HIT=0` 或 `SPEED≤1` 的刷怪行** ——
但部署 `MonGen.txt` 是冻结内容，只能由 owner 裁决。

---

## 7. 本卡能证明什么 / 不能证明什么（F2 要求写死）

**能证明（离线、可复算）**

1. `CM_HIT`/`CM_TURN` 的线上字段口径：X/Y 打包在 `recog`，`param` 不被读（`WorldServer.cs:831-844` 两支都如此）。
2. 取目标必须靠「状态化 Dir」，因此 `CM_TURN` 是必经步骤（`PlayObject.Attack.cs:26` vs `BaseObject.Attack.cs:12`）。
3. 本方收不到自己的 `SM_HIT(14)`；命中只能等受击者的 `SM_STRUCK(31)`。
4. `僵尸2`/`石墓尸王`（Race 95 `DigOutZombi`）**不具备可打的可见性前提**；`弓箭护卫` 等名字**在 `monsters` 表里不存在**（不会刷）。
5. 确定性刷怪点（Range=0/MissionGenRate=0/名字可解析）只落到 4 个 Race，逐一的 FixedHide/StoneMode 判定见 §1.2。
6. 现基线夹具下，**任何确定性目标都打不出确定的 `SM_STRUCK`**（§6.3 表）。
7. 场景文件的形状/预置位合法：`--check-scenario` rc=0、`card_window --dry-run` 两侧 rc=0。

**不能证明 / 未达成**

1. **卡面判据「删掉攻击步 ⇒ 应红」**：工具不支持，实测 rc=0（F2，**不写成 PASS**）。
2. **场景真能跑出 `SM_STRUCK(31)`**：不能 —— 缺 §6.4 的夹具。所以本场景**不是可用的窗口输入**。
3. 帧级对拍结论（`SM_STRUCK` 头的 HP/伤害逐字节）：本卡零真机证据，属后续窗口批次。
4. Rust/C# 在 `SM_STRUCK` 生成链上的等价性：本卡不判。

---

## 8. 覆盖到哪几张卡 / 覆盖不到什么

**一旦 §6.4 夹具落地，可作为窗口输入覆盖**：
`monster-attack-hit-frame`（`SM_STRUCK` 头 `(受击者, HP, MaxHP, 伤害)`）、
`rm-refmessage-delay-layer`（`RM_REFMESSAGE` 200ms 延迟层 → 广播 → 观察者回帧的次序）、
`attacktarget-overrides` / `proper-target-unfolded`（`GetPoseCreate` 取前格 + `IsProperTarget`）、
`weapon-durability-frames`（`AttackDir` 前段的 `UseItems[Weapon]` / `CheckWeaponUpgrade`）、
`run-gap-pl1/mb1/mb3/mb4` 中与「玩家主动一击」相关的部分。

**始终覆盖不到**：没打中支（零帧，§4）；本方自己的 `SM_HIT(14)`；`CM_HEAVYHIT` 等另外 8 条攻击 ident；
多观察者语义（需第二会话）；`SM_DEATH`（目标 HP 4000，一击不死）。

---

## 9. 门禁（返工后实跑，命令 + rc）

| 命令 | rc |
|---|---|
| `python -I -B tools/probe/mir2_probe.py --check-scenario --scenario tools/probe/scenarios/combat_strike.json` | **0**（`scenario OK: combat_strike (4 steps, params=self_pos=15204608)`） |
| 同上 + `--var self_pos=15204608` | **0** |
| `python -I -B tools/probe/mir2_probe.py --selftest` | **0**（8 checks OK） |
| `python -I -B tools/conformance/card_window.py --card openmir2-scenario-combat-strike --scenario tools/probe/scenarios/combat_strike.json --profile alt --dry-run` | **0**（两侧 `map=4 x=256 y=232` 图 500x500 可走） |
| T1（删 wait_for） / T2（阻挡或越界 placement） | **2 / 2**（改坏必红） |
| T3（删攻击步）/ T4（`wait_for=4095`） | **0 / 0 —— 未达成，工具缺口（F2）** |

未跑 `t_reservation_gate.py check`：本卡未动任何 `T-*` 登记。

---

## 10. 残留项（F3/F4 只登记，不改）

1. **`rules.json` 缺 `31`（`SM_STRUCK`）** —— 已另开卡 **`openmir2-conformance-rule-sm-struck-31`** 在办。
   在它落地前本场景**不能**作为窗口输入：未登记 ident 一出现就 `UNCLASSIFIED` ⇒ 整次窗口 `UNDECIDABLE(rc=2)`
   （`docs/window-runbook-20261023.md` §1 第 5 条）。`31` 头是 `(受击者, HP, MaxHP, 伤害)`，HP/伤害随 RNG 变，
   应做**字段窄掩码**而不是整帧 ignore。`tools/conformance/` 不在本卡写集。
2. **`tools/probe/scenarios/combat_hit.json` 的 recog/param 口径是错的**（§2.1），且只等本方看不到的 14
   ⇒ 双重恒超时。归属卡建议 **`openmir2-window-discovery-rules`**（拥有 `tools/probe/` 写集），本卡不改。
3. **夹具缺口**（§6.4）：需要「DC ≥ 31 且 HitPoint ≥ 17」的角色，或 owner 裁决给冻结内容加一条
   `HIT=0 / SPEED≤1` 的 `Range=0` 刷怪行。建议开「combat 夹具」卡。
4. 本卡临时脚本在 `E:/tmp/w71/`（纯离线分析），不属于仓库内容。
