# R7 场景 `equip_struck_save`：装备耐久变化（卡 `openmir2-scenario-equip-struck-save`）

- 基线：`walgit/master` = **`1893470a0`**（返工后 rebase；首版基线是 `302038193`）；worktree `E:/tmp/wt-worker-69`，分支 `card/scenario-equip-struck-save`。
- tier = **L**：只造场景夹具（`tools/probe/scenarios/equip_struck_save.json` + 本文），零生产代码、零帧字节。
- 场景：`tools/probe/scenarios/equip_struck_save.json`（5 步），placement **`map=0103 (19,19)`**，`params.self_pos=1245203`。
- sha256（返工后树）：**`4c703eca506caf22e54965d2e169188eae08082cbca5a25ccd29d0bb6f5e40aa`**

---

## 0. 结论先行

1. **oracle 里有两条耐久磨损路**：① **攻击方武器**（`AttackSuccess` → `DoDamageWeapon`，**有目标就扣，命中/躲闪都扣**）；
   ② **受击方护甲**（`StruckDamage`，**必须先被打中**）。
   本场景选 **①**：它唯一的逃逸条件是「身前格没有目标」，与命中概率无关。
2. **返工核心：换平台**。首版把目标放在比奇省 map 0 (288,609/610)，被 reviewer-6 证伪——
   `MonGen.txt:112/113/124/125` 的 鹿/稻草人 `Range=50` **刷怪箱覆盖该点**，`GenMonRate=10` 下每行刷 50 只，
   稻草人 Race=83 `SlowAtMonster` 会主动攻击（`StruckTime` 会拒掉 `CM_HIT`）。现改为 **map 0103（铁器店）**：
   该图在 `MonGen.txt` 里**零行**（部署件与两侧窗口 Envir 的 `MonGen.txt` 逐字节相同）⇒ 不存在任何刷怪箱，
   视野内**确定零怪、零主动攻击**。证明表见 §2.2/§2.3。
3. **场景 5 步**：`CM_TURN(3010)` 定朝向 Up → `sleep 400` → `CM_HIT(3014)` → `wait_for [642, 203]` →
   `wait_idents [642]`（成功硬门）。目标是 map 0103 唯一的 actor：商店 NPC「卫家店」(19,18)。
4. **`SM_DURACHANGE(642)` 才是磨损帧**；`SM_UPDATEITEM(203)` 在普通磨损里**根本不发**（只在
   `CheckWeaponUpgrade` 成功支等属性更新路径出现）。硬门只认 642；**注意破零支会发两条 642**（`DoDamageWeapon` 破零支一条 + 尾桶门 `Math.Abs(nDura/1.03) != nDuraPoint` 再一条，载荷同为 `Dura=0`）——`wait_idents [642]` 不受影响，但真机判定**不要按「恰一条」写**。
5. **Rust 侧已接 642**（卡 `openmir2-attack-weapon-wear-642` 合入 master `8dcb8c8ca`）：`AttackSuccess` 有目标即 `Random(5)+2-WeaponStrong` → `DoDamageWeapon` → `SM_DURACHANGE(642)`（`Recog=Dura`/`Param=槽`/`Tag,Series=DuraMax`），破零支 `SM_DELITEM(202)` → `642(0)` →（尾桶门）**再一条 642(0)**。今日 `card_window --dry-run --scenario equip_struck_save.json` **rc=0**；真机窗口**未跑**（取证须真机）。
6. **存档往返不属本卡**（要连库）：窗口侧判据见 §5。

---

## 1. 第 1 步：oracle 的耐久扣减点（逐条 `file:line` + 抽数口径）

| # | 路径 | 扣减点 | 抽数（RNG） | 触发条件 | 帧 |
|---|---|---|---|---|---|
| 1 | **攻击方武器** | `PlayObject.Attack.cs:1436-1470` `DoDamageWeapon(nWeaponDamage)` | `nWeaponDamage = Random(5) + 2 - AddAbil.WeaponStrong`（`:225`）——**每次「有目标」的攻击恰抽一次**，命中/躲闪/伤害 0 都抽 | 有武器（`UseItems[Weapon].Index > 0`）+ `AttackSuccess` 的 `targetObject != null`（`:217-220`）+ `nWeaponDamage > 0`（`:226`） | 见 §4.1（**几乎必发**一条 642）；扣到 ≤0 ⇒ `SendDelItems`（→**202**）+ `Recog=0` 的 **642**（`:1444-1457`） |
| 2 | 攻击方武器（挖矿） | `PlayObject.Base.cs:1574`（`PileStones`） | `Random(15) + 5`，且**先**要 `Random(MakeMineHitRate) == 0`（`:1545`） | 站在矿点 + 武器 `Shape==19` | 同上 |
| 3 | **受击方衣服** | `PlayObject.cs:2965-2996`（`StruckDamage` 的衣服位特判） | `nDam = Random(10) + 5`（`:2957`）——**每被打中一次抽一次**；衣服位**不再抽**，直接扣 | **必须先被打中**（命中门在 `BaseObject.Attack.cs:44-50` 的 `HitPoint < Random(SpeedPoint)`） | `Round(Dura/1000)` 桶变化 ⇒ 642（`:2992-2995`）；破零 ⇒ 202 + `FeatureChanged`(41) |
| 4 | 受击方其它已装备格 | `PlayObject.cs:2998-3039` | 每格**各抽一次** `Random(8) == 0`（`:3000`）才扣；扣的额度复用 #3 的 `nDam` | 被打中 + 该格 `Index > 0` + 命中 1/8 | 同上（1000 桶门） |
| 5 | 火把/蜡烛 | `PlayObject.Message.cs:2233-2278` `UseLamp` | **无**（`Dura - 1`，`unchecked` 回绕） | 每 `DecLightItemDrugTime` 周期 | **`SM_LAMPCHANGEDURA(655)`**，不是 642（T-199 已接线） |
| 6 | 护身符（施法消耗） | `Magic/MagicBase.cs:118`（`Dura -= duration`） | 无 | 施法消耗护身符 | **642**（Rust 已实现） |

**武器升级破碎链（另一族，别混）**：武器 `Desc[10] > 0` 时 `AttackDir` 先跑 `CheckWeaponUpgrade`
（`PlayObject.cs:4110-4138`）——成功支发 **203**（`SendUpdateItem`）+ `RM_ABILITY`；破碎支发 **202** +
`RM_BREAKWEAPON`（**1102**）+ `FeatureChanged`(41)。它**不是**耐久磨损，但会抢在磨损前发生（`AttackDir:24-31`
先于 `AttackSuccess`）。

### 1.1 抽数口径（窗口必须双开 `MIR2_RNG_LOG`）
- 路 #1 的 `Random(5)` 在**第一次有目标的攻击**就发生，与命中判定**互不影响**；
  它决定 `nWeaponDamage`、进而决定 642 的 `Recog=Dura` 与落库的 `Dura`。
- 路 #3/#4 的 `Random(10)`/`Random(8)` 只在**被打中**时发生，与 #1 不同步。
- ⇒ 窗口先双开 `MIR2_RNG_LOG` 对「有目标的 `CM_HIT`」这一次攻击的抽数流水，再比 642 的字段。

---

## 2. 选路与平台（返工重点）

### 2.1 攻击方 vs 受击方

| 路 | 需不需要「打中」 | 逃逸条件 | 本场景可用性 |
|---|---|---|---|
| **攻击方武器磨损** | **不需要**。`AttackSuccess`（`PlayObject.Attack.cs:215`）在 `_Attack` 之后无条件调用；`canHit`/`nPower` 只影响伤害 | 只有 `targetObject == null`（`:217-220`）、没武器、或 `nWeaponDamage<=0`（`:226`） | ✅ 只要前方格有**任何** actor |
| 受击方护甲磨损 | **需要**。命中门 `HitPoint < RandomByte(target.SpeedPoint)`（`BaseObject.Attack.cs:44-50`） | 怪必须选中我们、且命中 | ❌ 概率路径（`combat_strike` BLOCKED 同型） |

### 2.2 旧 placement 为什么被否（reviewer-6 的 blocking，本卡接受）

`MonGen.txt` 的刷怪位置 = `X - Range + Random(Range*2+1)`（`WorldServer.MonGen.cs:825-826`/`:849-850`），
即**边长 `2*Range+1` 的方形箱体**；每行实际生成只数 = `max(1, round(Count / (GenMonRate/10)))`
（`:232`），部署 `GenMonRate=10`（`Server.conf:780`）⇒ **= Count**；`RegenMonstersTime=200ms`（`Server.conf:50`）
⇒ 起服后 0.2s 内就把整行一次性放出。

旧点 (288,609/610) 被下列 4 行覆盖：

| 文件行 | 怪 | 箱心 | Range | 覆盖范围 | 是否覆盖 (288,609/610) | 怪 Race（`sql/mir2_data.sql`） | 是否主动攻击 |
|---|---|---|---|---|---|---|---|
| `MonGen.txt:112` | 鹿 | (256,572) | 50 | x∈[206,306] y∈[522,622] | **是** | 52 `AnimalDeer`（`:227`） | 否（`WorldServer.MonGen.cs:526-545` → `MonsterObject{Animal=true}`） |
| `MonGen.txt:113` | 稻草人 | (256,572) | 50 | 同上 | **是** | 83（`:228`） | **是**（`WorldServer.MonGen.cs:566-567` → `SlowAtMonster : AtMonster`） |
| `MonGen.txt:124` | 鹿 | (328,566) | 50 | x∈[278,378] y∈[516,616] | **是** | 52 | 否 |
| `MonGen.txt:125` | 稻草人 | (328,566) | 50 | 同上 | **是** | 83 | **是** |

每行 `Count=50`（鹿/稻草人各 50），箱体 101×101 = 10201 格；玩家 `ViewRange=12` 的 25×25 = 625 格
⇒ 每个 50 只的箱体期望 `50*625/10201 ≈ 3.1` 只落在视野内，四行合计 **≈ 12 只**（与 reviewer-6 的估计一致）。
⇒ ① 稻草人会主动攻击，`StruckTime` 期内 `CheckActionStatus`（`PlayObject.Base.cs:2671-2680`）会拒掉 `CM_HIT`；
② 怪物 AOI 帧会污染比较器；③ 刷怪位置按墙钟 `ZenLimit` 截断、两侧不可复现。**该点作废。**

### 2.3 新 placement 的零怪证明（map 0103 铁器店）

| 证明项 | 结论 | 依据 |
|---|---|---|
| `MonGen.txt` 是否有 map 0103 的刷怪箱 | **零行** | 部署件 `E:/MirServer/Mir200/Envir/MonGen.txt`；且与两侧窗口 Envir（`E:/tmp/conformance/{csharp,rust}-root/M2GameSvr/Envir/MonGen.txt`）`cmp` 逐字节相同 ⇒ 三份都没有 0103 |
| 是否有 `loadgen` 子文件 | **无** | `grep -in loadgen MonGen.txt` = 0 命中，故不存在第二节点表 |
| 是否有脚本刷怪 | **无** | 全 `Envir` 扫描 `^MONGEN`：只有 `Market_Def/任务NPC/武士-0114.txt:146`（T118）与 `老人-1.txt:50`（Q001），与 0103 无关 |
| 是否有 MapQuest/MapEvent 触发 | **无** | `MapQuest.txt`/`MapEvent.txt` 无 0103 条目 |
| 图上是否有其它 actor | **只有目标 NPC** | `MerChant.txt:26` 一条（卫家店 (19,18)）；`Npcs.txt` 无 0103 条目 ⇒ 视野内零怪、零主动攻击 |
| 地图与落点 | 27×29，落点 (19,19) 可走 | `Map/0103.map`（部署件与两侧窗口根 `cmp` 相同）；`MapInfo.txt:293` `[0103 铁器店] FIGHT`，服务器号 0 |

> 结论：**map 0103 上不存在任何刷怪来源**，怪既不会生成也不会走进来（无跨图移动），这是比「视野内刚好没怪」更强的性质。

### 2.4 目标 NPC：不掉血、不还手、不动
- `GetPoseCreate()`（`BaseObject.cs:1253-1262`）→ `Envir.GetMovingObject(nX, nY, true)`（`Envirnoment.cs:1203-1239`）
  返回格内**任何** `ActorObject`（`Envirnoment.cs:285` 对所有入图 actor 都置 `ActorObject=true`）⇒ 商店 NPC 是合法目标。
- **不掉血**：玩家侧 `IsProperTarget`（`PlayObject.cs:1924`）→ `IsAttackTarget` **覆写**（`PlayObject.Attack.cs:627-753`）：
  默认 `AttatckMode=HAM_ALL`（`PlayObject.cs:908`）下只有 `Race < ActorRace.NPC(10) || Race > ActorRace.PeaceNpc(15)` 才 `result=true`；
  商店 NPC 的 `Race = ActorRace.NPC = 10`（`NormNpc.cs:42`）⇒ **false** ⇒ `_Attack` 的 `nPower=0`、不调 `StruckDamage`。
  （reviewer-6 的更正成立，本卡据此改写了原先「可能打到 NPC」的说法。）
- **不还手/不动**：`NormNpc` 无 AI 攻击、无移动 ⇒ 不会触发 `StruckTime`，前方格永远有目标。

---

## 3. 场景本体（5 步）

| # | 步骤 | 说明 |
|---|---|---|
| 1 | `send CM_TURN(3010)` `recog=self_pos(1245203)` `tag=0` | `self_pos=(Y<<16)|X=(19<<16)|19`；`tag=Dir=Up=0`；`ClientChangeDir`（`PlayObject.Operate.cs:310-334`）要求坐标是自己 |
| 2 | `sleep 400` | 不同操作间隔 ≥ `ActionIntervalTime`（部署 `Server.conf:330 = 300`）+ 驱动器 `--between-ms 200` |
| 3 | `send CM_HIT(3014)` `recog=self_pos` `tag=0` | `ClientHitXY`（`PlayObject.Attack.cs:823-968`）→ `AttackDir(null,0,Dir)`（`:910`）→ `GetPoseCreate()` = 卫家店 |
| 4 | `wait_for [642, 203]` `timeout_ms=3000` | 两支都覆盖，任一出现即过（§4） |
| 5 | `wait_idents [642]` `timeout_ms=3000` | **成功硬门**：没武器 / 前方格没目标 / `nWeaponDamage<=0` ⇒ 零 642 ⇒ 红 |

`self_pos` 是**自己坐标**的打包值（不是 actor id），由 `placement` 决定；两者的自洽现在由
**R5**（`--check-scenario` 的 placement↔self_pos 校验）把守（见 §7 的 T5）。

---

## 4. `642` 与 `203` 各自在什么条件下出现

### 4.1 `SM_DURACHANGE(642)` —— 本场景该来的那一条
- 线上字段（`PlayObject.Message.cs:1803-1805`）：`SendDefMessage(SM_DURACHANGE, nRecog=nParam1=Dura, nParam=wParam=槽, nTag=LoWord(nParam2=DuraMax), nSeries=HiWord(nParam2=DuraMax))`
  ⇒ **`Recog = 扣后的 Dura`、`Param = ItemLocation`（Weapon=1）、`Tag/Series = DuraMax` 低/高 16 位**。
- 发送条件（`DoDamageWeapon`，`PlayObject.Attack.cs:1466`）：
  `if (Math.Abs(nDura / 1.03) != nDuraPoint)` —— **左边是未取整的 `double`，右边是扣前 `Round(Dura/1.03)` 的 `int`**。
  两者只在「扣后 `Dura/1.03` 恰为整数」时相等（测度零）⇒ **实际上几乎每次都发**。
  （首版把它写成「两边都 `Round` 再比」，与代码不符，已订正——reviewer-6 minor ②。）
- 破零支（`:1444-1457`）：先把 `Dura=0`、`SendDelItems`（**202**，`PlayObject.Base.cs:1648-1665`）→ `Index=0` → 再发 `Recog=0` 的 **642**；**随后尾桶门通常再补一条同载荷 642**（见 §1.4）⇒ 破零一击常见 `202 → 642(0) → 642(0)`。

### 4.2 `SM_UPDATEITEM(203)` —— 普通磨损**不发**
- `SendUpdateItem`（`PlayObject.Base.cs:1668-1686`）是「物品属性更新」，尾 = `EncodeBuffer(ClientItem)`。
- 攻击路径里**唯一**的生产点是 `CheckWeaponUpgrade` 的成功支（`PlayObject.cs:4110-4138`，
  前置 `UseItems[Weapon].Desc[ItemAttr.WeaponUpgrade] > 0`，即升级 NPC 取回过的武器）。
  其余 203 来源：换装特殊衣服（`PlayObject.Operate.cs:450-480`）、GM/脚本动作——**都不属于磨损**。
- ⇒ 本场景夹具是**未升级武器**：该来的是 642，203 不该出现。把 203 放进第 4 步集合只是为了「升级武器夹具」
  不至于恒超时（那套夹具下 203 先发、642 照发，第 5 步仍绿）；**两者不互相替代**。

---

## 5. 存档往返（**不属本卡**，只登记窗口侧要判什么）

1. **表与列**：`characters_item`（`sql/mir2_db.sql:293929-293940`）只有
   `PlayerId / Position / MakeIndex / StdIndex / Dura / DuraMax`；`Desc[0..13]` 不在本表，在
   `characters_item_attr`（`PlayDataStorage.Add.cs:366` 的 14 个 `VALUE0..13`）。
2. **写回条件**（`ComparerUserItem`，`PlayDataStorage.Save.cs:175-201`）：
   - 物品还在（`new.MakeIndex > 0`）⇒ 进 `chg` ⇒ `UpdateUseItemSql`（`:204`）按 `Position` 覆盖 `MakeIndex/StdIndex/Dura/DuraMax`；
   - 槽被清（`new.MakeIndex == 0 && old.MakeIndex > 0`）⇒ 进 `del` ⇒ `ClearUseItemSql`（`:203`）
     把该 `Position` 行写成 `MakeIndex=0, StdIndex=0, Dura=0, DuraMax=0`（**行保留，不清表**）；
     **同一分支还会调 `ClearItemAttr`**（`Save.cs:236` → `:626-640`）把这个 `MakeIndex` 的
     `characters_item_attr` 行 `VALUE0..13` **全部清零**（reviewer-6 minor ④ 的补充）。
   - 纯耐久变化（`MakeIndex` 不变）也会落库——因为条件是 `new.MakeIndex > 0`，与「值变了」无关。
3. **期望关系**（一次 `CM_HIT`、未破零）：
   - `Position = ItemLocation.Weapon (1)` 的行：`Dura_new = Dura_old - nWeaponDamage`，
     `nWeaponDamage = Random(5)+2-WeaponStrength ∈ [2,6]`（有效 `SpecialPwr==0` 时）；
     **`DuraMax`、`MakeIndex`、`StdIndex` 不变**；
   - `characters_item_attr` 的 `VALUE0..13` **不变**（磨损不改 `Desc`）；
   - `0 < Dura_new < DuraMax` 且 `Dura_new < Dura_old`（严格减少，≥2）。
4. **破零收尾**（窗口若要单测破零，需把武器 `Dura` 夹具压到 **≤2**——`nWeaponDamage` 最小 2，
   `Dura ≤ 6` 只保证「可能破」不保证一击破）：
   - 客户端：**202 → 642(Recog=0) →（尾桶门）642(Recog=0)**（同一 `CM_HIT` 内，202 先；破零常见两条 642，判定别按「恰一条」）；
   - 库：`characters_item` 该 `Position` 行被 `ClearUseItemSql` 清空
     （`MakeIndex=0/StdIndex=0/Dura=0/DuraMax=0`）**且** `characters_item_attr` 该 `MakeIndex` 行被
     `ClearItemAttr` 清零（`VALUE0..13` 全 0）；
   - **不是** 203/1102（那是升级武器破碎链，见 §1 的「另一族」）。
5. **不对拍项**：`characters` 其它列、背包表——本路径不碰。

---

## 6. 覆盖到哪张卡 / 覆盖不到什么

- **覆盖**：`openmir2-persist-useitems-wiring`（批量计划第 32 行）的「**耐久磨损**」这一条：
  把两侧推到「一次有目标的攻击」，产出 642（破零支另有 202 与第二条 642；本场景正常耐久不破零），并给出「该判哪些字段/什么关系」的口径。
- **不覆盖**：
  1. **存档往返本身**（要连库、要 `dbsnap` 对拍）——归窗口卡（§5 只给口径）。
  2. **受击方护甲磨损**（要必中，概率路径）——本场景不碰。
  3. **破零/破碎**：本场景的武器不会破零（正常耐久）；破碎链（1102/203/41）属 T-199。
  4. **火把 655**：另一族（T-199 已接线）。
  5. **RNG 逐抽对拍**：见 §1.1，需 `MIR2_RNG_LOG` 双开。

---

## 7. 牙 + sha256 + 门禁（**返工后在 master `1893470a0` 上实跑**）

场景 sha256：**`4c703eca506caf22e54965d2e169188eae08082cbca5a25ccd29d0bb6f5e40aa`**
（T1~T5 每次都 `cp` 备份后逐字节还原，还原后 sha256 与基线一致）。

| 命令 | rc | 说明 |
|---|---|---|
| `--check-scenario --scenario tools/probe/scenarios/equip_struck_save.json` | **0** | `scenario OK: equip_struck_save (5 steps, params=self_pos=1245203)` |
| `mir2_probe.py --selftest` | **0** | **9 checks**（旧树是 8；新 master 多了 R1–R6 族） |
| `card_window.py --dry-run --profile alt` | **0** | 两侧 `map=0103 x=19 y=19` 图 27x29 可走 |
| `t_reservation_gate.py check` | **0** | GREEN：预约表 208 行 / whitelist 208 号 / max=T-222；本卡未新增 T-* |
| **T1** 删关键 `send`（`CM_HIT`） | **0 —— 仍是真缺口** | 工具不校验「必须含某个 send」 |
| **T2** `wait_for [642,203]` → `[4095]` | **2**（R3） | `steps[3].wait_for ident 4095 is not a CM_*/SM_* constant …`（首版记 rc=0，旧树；**订正：master 上已是 rc=2**） |
| **T3** 删掉全部 wait 步 | **2** | N-5：`has no wait step` |
| **T4** `wait_for` 越界 u16 `70000` | **2** | `out of range [0,65535]` |
| **T5** `placement.y` 改 20、`self_pos` 不动 | **2**（R5） | `scenario.placement … disagrees with self_pos=1245203 …`（首版记「工具不校验、仍 rc=0」，**订正：master 上已是 rc=2**） |

- 只有 **T1** 仍是缺口：`--check-scenario` 不校验「场景必须含某个 send」，故「删关键 send」仍恒绿。
  已如实登记（不写 PASS），归 **`openmir2-probe-scenario-validation-gaps`**。

---

## 8. 残留项（只登记，不改）

### 8.1 Rust 侧武器磨损帧（**2026-10-24 订正：Rust 已接 642**，见本节末）
- `mir2-rs/tests/parity/whitelist.md` **T-199** 的「范围外但相邻（既有登记）」原文：
  「`AttackSuccess` 的 `DoDamageWeapon`（`PlayObject.Attack.cs:225-229`）武器耐久磨损与 `RM_DURACHANGE(10125)` 未接（T-142 末段）」
  ——即**攻击方武器磨损整条未实现**。〔**2026-10-24 订正**：该条已过期——卡 `openmir2-attack-weapon-wear-642` 合入 master `8dcb8c8ca`，攻击成功磨损与 `SM_DURACHANGE(642)` 已接（`drain_attack_weapon_wear`；破零支 `202 → 642(0) → 642(0)`）；今日 `--dry-run` rc=0；真机窗口仍未跑。〕
- 受击方：`crates/world/src/combat.rs:824-841` 的 `struck_damage_durability` / `:767-775` 的 `wear_durability`
  **只改 `use_items[*].dura`**，没有事件通道；宿主 `sync_use_items_from_entity_if_loaded`
  （`bins/game-svr/src/lib.rs`）只复制值、**不发帧**。
- 宿主里所有 642 的生产点**今非昔比**：`openmir2-attack-weapon-wear-642`（合入 `8dcb8c8ca`）已在攻击成功路径接上 `DoDamageWeapon` → `SM_DURACHANGE(642)`，并由 `drain_attack_weapon_wear` 出帧（202 直发、642 入队、只投自己）。
- ⇒ 本场景作为窗口输入时，**Rust 侧不再缺 642**；今日 `--dry-run` rc=0。**真机窗口未跑**（取证须真机），破零支的两条 642 见 §1 第 4 条。

### 8.2 工具缺口（返工后只剩 T1）
- master `1893470a0` 已补 R1–R6：T2（ident 必须存在于协议常量表，R3）与 T5（placement↔self_pos 自洽，R5）
  现在都能红（本卡实测 rc=2）。**仍缺**：①「场景必须含某个 `send`」（T1）。
  归 **`openmir2-probe-scenario-validation-gaps`**。

### 8.3 夹具要求
- 角色必须**装备一把有耐久的武器**（`characters_item` 的 `Position=ItemLocation.Weapon` 行，`Dura > 0`）；
  背包/装备夹具仍不在 `card_window.py` 能力内（runbook §1.5 末段，归 `openmir2-discovery-bagitems-201`）。
- **武器的有效 `SpecialPwr` 必须为 0**（reviewer-6 minor ①）：`AddAbil.WeaponStrong` 取自
  `GetUpgradeStdItem` 后的 `clientItem.Item.SpecialPwr`（`PlayObject.cs:4424-4426`，值域 1..10 才赋值），
  而 `SpecialPwr` 会被 `Desc[7] ∈ 1..10` 覆盖（`GameItemSystem.cs:745-751`）。
  `WeaponStrong ≠ 0` 时 `Random(5)+2-WeaponStrong` 可能 ≤0，被 `PlayObject.Attack.cs:226` 的门挡掉 ⇒ 零 642。
  ⇒ 夹具要求 `characters_item_attr.VALUE7 = 0` **且** 武器模板自身的 `SpecialPwr = 0`。
- 武器**未升级**（`characters_item_attr.VALUE10 == 0`）；否则走 `CheckWeaponUpgrade`，203/1102 会先出现。

### 8.4 场景本身的已知限制
- `ClientHitXY` 的动作间隔门（部署 `HitIntervalTime=850`、`ActionIntervalTime=300`）用固定 `sleep 400`
  + `--between-ms 200` 兜；若窗口把 `--between-ms` 调成 0，仍有 400ms > 300ms 的余量。
- 目标 NPC 不掉血（§2.4）⇒ 本场景没有 `SM_STRUCK` 一类副作用帧；唯一帧就是 642（破零支才会多出 202 与第二条 642；本场景不破零）。
- 本卡只在 worktree 内跑离线工具（`--check-scenario`/`--selftest`/`--dry-run`/`t_reservation_gate`），
  **零起栈、零连库、零端口**；`/e/tmp/w69c` 为本次离线分析临时目录，交付前已删除。
