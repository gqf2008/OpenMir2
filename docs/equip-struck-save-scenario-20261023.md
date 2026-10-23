# R7 场景 `equip_struck_save`：装备耐久变化（卡 `openmir2-scenario-equip-struck-save`）

- 基线：`walgit/master` = `302038193`；worktree `E:/tmp/wt-worker-69`，分支 `card/scenario-equip-struck-save`。
- tier = **L**：只造场景夹具（`tools/probe/scenarios/equip_struck_save.json` + 本文），零生产代码、零帧字节。
- 场景：`tools/probe/scenarios/equip_struck_save.json`（5 步），placement `map=0 (288,610)`，`params.self_pos=39977248`。
- sha256（提交后树）：**`9d6701147f182934b92e765787ae5be71d19e3ec72aa0c29857c028238add9bb`**

---

## 0. 结论先行

1. **oracle 里有两条耐久磨损路**：① **攻击方武器**（`AttackSuccess` → `DoDamageWeapon`，**有目标就扣，命中/躲闪都扣**）；
   ② **受击方护甲**（`StruckDamage`，**必须先被打中**）。
   本场景选 **①**：它唯一的逃逸条件是「身前格没有目标」，与命中概率无关——`combat_strike` 的 BLOCKED 正是死在「必中」上。
2. **场景 5 步**：`CM_TURN(3010)` 定朝向 Up → `sleep 400`（操作间隔门）→ `CM_HIT(3014)` → `wait_for [642, 203]` →
   `wait_idents [642]`（成功硬门）。目标选**商店 NPC**（不还手 ⇒ 不会触发 `StruckTime` 操作门；不动 ⇒ 前方格必有目标）。
3. **`SM_DURACHANGE(642)` 才是磨损帧**；`SM_UPDATEITEM(203)` 在**普通磨损里根本不发**，只在
   `CheckWeaponUpgrade` 成功支（要求武器 `Desc[10] > 0`）等属性更新路径出现。两者都放进 wait 集合只为「换夹具也不恒超时」，
   成功硬门只认 642。
4. **Rust 侧这条帧还没接**（T-199 的相邻登记：`AttackSuccess` 的 `DoDamageWeapon` 与 `RM_DURACHANGE` 未接；
   受击方 `wear_durability` 只改世界状态、**没有事件通道**）⇒ 现在跑窗口会在「Rust 缺 642」上红。
   这是**实现缺口**，不是场景或 oracle 不可达（§8.1）。
5. **存档往返不属本卡**（要连库）：只登记窗口侧判据（§5）。

---

## 1. 第 1 步：oracle 的耐久扣减点（逐条 `file:line` + 抽数口径）

| # | 路径 | 扣减点 | 抽数（RNG） | 触发条件 | 帧 |
|---|---|---|---|---|---|
| 1 | **攻击方武器** | `PlayObject.Attack.cs:1436-1470` `DoDamageWeapon(nWeaponDamage)` | `nWeaponDamage = Random(5) + 2 - AddAbil.WeaponStrong`（`:225`）——**每次「有目标」的攻击恰抽一次**，命中/躲闪/伤害 0 都抽 | 有武器（`UseItems[Weapon].Index > 0`）+ `AttackSuccess` 的 `targetObject != null`（`:217-220`）+ `nWeaponDamage > 0`（`:226`） | `Round(Dura/1.03)` 桶变化 ⇒ `RM_DURACHANGE`（`:1458-1466`）；扣到 ≤0 ⇒ `SendDelItems`（→**202**）+ `Recog=0` 的 **642**（`:1444-1457`） |
| 2 | 攻击方武器（挖矿） | `PlayObject.Base.cs:1574`（`PileStones`） | `Random(15) + 5`，且**先**要 `Random(MakeMineHitRate) == 0`（`:1545`） | 站在矿点 + 武器 `Shape==19` | 同上 |
| 3 | **受击方衣服** | `PlayObject.cs:2965-2996`（`StruckDamage` 的衣服位特判） | `nDam = Random(10) + 5`（`:2957`）——**每被打中一次抽一次**；衣服位**不再抽**，直接扣 | **必须先被打中**（命中门在 `BaseObject.Attack.cs:44-50` 的 `HitPoint < Random(SpeedPoint)`） | `Round(Dura/1000)` 桶变化 ⇒ 642（`:2992-2995`）；破零 ⇒ 202 + `FeatureChanged`(41) |
| 4 | 受击方其它已装备格 | `PlayObject.cs:2998-3039` | 每格**各抽一次** `Random(8) == 0`（`:3000`）才扣；扣的额度复用 #3 的 `nDam` | 被打中 + 该格 `Index > 0` + 命中 1/8 | 同上（1000 桶门） |
| 5 | 火把/蜡烛 | `PlayObject.Message.cs:2233-2278` `UseLamp` | **无**（`Dura - 1`，`unchecked` 回绕） | 每 `DecLightItemDrugTime` 周期 | **`SM_LAMPCHANGEDURA(655)`**，不是 642（已由 T-199 接线） |
| 6 | 护身符（施法消耗） | `Magic/MagicBase.cs:118`（`Dura -= duration`） | 无 | 施法消耗护身符 | **642**（Rust 已实现，见 `bins/game-svr/src/lib.rs` 的 `spell` 用例） |

**武器升级破碎链（另一族，别混）**：武器 `Desc[10] > 0` 时 `AttackDir` 先跑 `CheckWeaponUpgrade`
（`PlayObject.cs:4110-4138`）——成功支发 **203**（`SendUpdateItem`）+ `RM_ABILITY`；破碎支发 **202** +
`RM_BREAKWEAPON`（**1102**）+ `FeatureChanged`(41)。它**不是**耐久磨损，但会抢在磨损前发生（`AttackDir:25-31`
先于 `AttackSuccess`）。

### 1.1 抽数口径（窗口必须双开 `MIR2_RNG_LOG`）
- 路 #1 的 `Random(5)` 在**第一次有目标的攻击**就发生，且与命中判定**互不影响**；
  它决定 `nWeaponDamage ∈ [2,6]`（`WeaponStrong=0` 时恒 >0），进而决定 642 的 `Recog=Dura` 与落库的 `Dura`。
- 路 #3/#4 的 `Random(10)`/`Random(8)` 只在**被打中**时发生，与 #1 不同步。
- ⇒ **窗口必须先双开 `MIR2_RNG_LOG` 对「有目标的 CM_HIT」这一次攻击的抽数流水**，再比 642 的字段；
  只比帧不看抽数流水会把「同值不同抽」当成等价。

---

## 2. 选路：为什么是「攻击方武器」

| 路 | 需不需要「打中」 | 逃逸条件 | 本场景可用性 |
|---|---|---|---|
| **攻击方武器磨损** | **不需要**。`AttackSuccess`（`PlayObject.Attack.cs:215`）在 `_Attack` 之后无条件调用；`canHit`/`nPower` 只影响伤害 | 只有 `targetObject == null`（`:217-220`）或没武器/`nWeaponDamage<=0`（`:226`） | ✅ 只要前方格有**任何** actor |
| 受击方护甲磨损 | **需要**。命中门 `HitPoint < RandomByte(target.SpeedPoint)`（`BaseObject.Attack.cs:44-50`） | 怪必须选中我们、且命中 | ❌ 概率路径（`combat_strike` BLOCKED 同型） |

**目标为什么选商店 NPC 而不是怪**：
- `GetPoseCreate()`（`BaseObject.cs:1253-1262`）→ `Envir.GetMovingObject(nX, nY, true)`（`Envirnoment.cs:1203-1233`）
  返回格内**任何** `ActorObject`（`Envirnoment.cs:285` 对所有入图 actor 都置 `ActorObject=true`）⇒ 商店 NPC 是合法目标。
- NPC **不动**（`GetFrontPosition` 每次算的是同一格）且**不还手** ⇒ 不会被 `StruckTime` 操作门
  （`CheckActionStatus` 开头，`PlayObject.Base.cs:2670-2678`）打断后续操作。
- 部署核对：`Envir/MerChant.txt:38` = `比奇城/万能 0 288 609 边界杂货店 0 3 0`；脚本
  `Market_Def/比奇城/万能-0.txt` 存在；map 0 的 `MonGen` 半径 15 内**零刷怪**（最近的弓箭护卫在 y≈487-514）。
- 站位 (288,610)：`0.map` 该格 `bk=4/fr=0` 可走；`CM_CLICKNPC` 无关（本场景不发点击），
  `CM_TURN`/`CM_HIT` 只要求坐标是自己的（`ClientChangeDir` `:328-334` / `ClientHitXY` `:883`）。

> **已知代价**：`IsAttackTarget`（`BaseObject.Base.cs:377-470`）对**玩家攻方**直接 `return true`
> ⇒ 这一击可能真的打到 NPC（掉血/`SM_STRUCK`），也可能被躲闪；两侧同种子下一致，但**不是**「零副作用」的保证。
> 它是本场景唯一的非磨损副作用，窗口按帧集合比较时不另做断言。

---

## 3. 场景本体（5 步）

| # | 步骤 | 说明 |
|---|---|---|
| 1 | `send CM_TURN(3010)` `recog=self_pos(39977248)` `tag=0` | `recog=(Y<<16)|X`（`WorldServer.cs:831-844`）；`tag=Dir=Up=0`；必须先转身，`CM_HIT` 取目标走状态化 `Dir` |
| 2 | `sleep 400` | 不同操作间隔 ≥ `ActionIntervalTime`（部署 `Server.conf:330 = 300`）+ 驱动器 `--between-ms 200` |
| 3 | `send CM_HIT(3014)` `recog=self_pos` `tag=0` | `ClientHitXY`（`PlayObject.Attack.cs:823-960`）→ `AttackDir(null,0,Dir)` → `GetPoseCreate()` = 商店 NPC |
| 4 | `wait_for [642, 203]` `timeout_ms=3000` | 两支都覆盖，任一出现即过（见 §4） |
| 5 | `wait_idents [642]` `timeout_ms=3000` | **成功硬门**：没武器 / 前方格没目标 ⇒ 零 642 ⇒ 红 |

`self_pos` 是**自己坐标**的打包值（不是 actor id）；它由 `placement` 决定，**改 placement 必须同步改它**
（`--check-scenario` 不校验两者同源，属工具缺口，归 `openmir2-probe-scenario-validation-gaps`，见 §8.2）。

---

## 4. `642` 与 `203` 各自在什么条件下出现

### 4.1 `SM_DURACHANGE(642)` —— 本场景该来的那一条
- 线上字段（`PlayObject.Message.cs:1803-1805`）：`SendDefMessage(SM_DURACHANGE, nRecog=nParam1=Dura, nParam=wParam=槽, nTag=LoWord(nParam2=DuraMax), nSeries=HiWord(nParam2=DuraMax))`
  ⇒ **`Recog = 扣后的 Dura`、`Param = ItemLocation`（Weapon=1）、`Tag/Series = DuraMax` 的低/高 16 位**。
- 发送条件（`DoDamageWeapon`，`PlayObject.Attack.cs:1458-1466`）：`Round(Dura_before/1.03) != Round(Dura_after/1.03)`。
  以 `Dura≈5000`、`nWeaponDamage∈[2,6]` 算，每次都会跨桶 ⇒ **一击一条**。
- 破零支（`:1444-1457`）：先把 `Dura=0`、`SendDelItems`（**202**，`PlayObject.Base.cs:1648-1665`）→ `Index=0` → 再发 `Recog=0` 的 **642**。

### 4.2 `SM_UPDATEITEM(203)` —— 普通磨损**不发**
- `SendUpdateItem`（`PlayObject.Base.cs:1668-1688`）是「物品属性更新」，尾 = `EncodeBuffer(ClientItem)`。
- 攻击路径里**唯一**的生产点是 `CheckWeaponUpgrade` 的成功支（`PlayObject.cs:4110-4138`，
  前置 `UseItems[Weapon].Desc[ItemAttr.WeaponUpgrade] > 0`，即升级 NPC 取回过的武器）。
  其余 203 来源：换装特殊衣服（`PlayObject.Operate.cs:450-480`）、GM/脚本动作——**都不属于磨损**。
- ⇒ 本场景夹具是**未升级武器**：该来的是 642，203 不该出现。把 203 放进第 4 步集合只是为了让「升级武器夹具」
  不至于恒超时（那套夹具下 203 先发、642 照发，第 5 步仍绿）；**两者不互相替代**。

---

## 5. 存档往返（**不属本卡**，只登记窗口侧要判什么）

本卡不连库、不判落库。窗口卡需要判的是：

1. **表与列**：`characters_item`（`sql/mir2_db.sql:293929-293940`）只有
   `PlayerId / Position / MakeIndex / StdIndex / Dura / DuraMax`；`Desc[0..13]` 不在本表，在
   `characters_item_attr`（`PlayDataStorage.Add.cs:366` 的 14 个 `VALUE0..13`）。
2. **写回条件**（`ComparerUserItem`，`PlayDataStorage.Save.cs:175-201`）：
   - 物品还在（`new.MakeIndex > 0`）⇒ 进 `chg` ⇒ `UpdateUseItemSql`（`:204`）按 `Position` 覆盖 `MakeIndex/StdIndex/Dura/DuraMax`；
   - 槽被清（`new.MakeIndex == 0 && old.MakeIndex > 0`）⇒ 进 `del` ⇒ `ClearUseItemSql`（`:203`）
     把该 `Position` 行写成 `MakeIndex=0, StdIndex=0, Dura=0, DuraMax=0`（**行保留，不清表**）。
   - 纯耐久变化（`MakeIndex` 不变）也会落库——因为条件是 `new.MakeIndex > 0`，与「值变了」无关。
3. **期望关系**（一次 `CM_HIT`、未破零）：
   - `Position = ItemLocation.Weapon (1)` 的行：`Dura_new = Dura_old - (Random(5)+2-WeaponStrength)`，
     **`DuraMax`、`MakeIndex`、`StdIndex` 不变**；
   - `characters_item_attr` 的 `VALUE0..13` **不变**（磨损不改 `Desc`）；
   - `0 < Dura_new < DuraMax` 且 `Dura_new < Dura_old`（严格减少，`nWeaponDamage >= 2`）。
4. **破零收尾**（窗口若要单测破零，需把武器 `Dura` 夹具压到 ≤6）：
   - 客户端：**202 → 642(Recog=0)**（同一 `CM_HIT` 内，202 先）；
   - 库：该 `Position` 行被 `ClearUseItemSql` 清空（`MakeIndex=0/StdIndex=0/Dura=0/DuraMax=0`）；
   - **不是** 203/1102（那是升级武器破碎链，见 §1 的「另一族」）。
5. **不对拍项**：`characters` 其它列、`characters_item_attr`、背包表——本路径不碰。

---

## 6. 覆盖到哪张卡 / 覆盖不到什么

- **覆盖**：`openmir2-persist-useitems-wiring`（批量计划第 32 行）的「**耐久磨损**」这一条：
  把两侧推到「一次有目标的攻击」，产出 642（以及破零时的 202+642），并给出「该判哪些字段/什么关系」的口径。
- **不覆盖**：
  1. **存档往返本身**（要连库、要 `dbsnap` 对拍）——归窗口卡（§5 只给口径）。
  2. **受击方护甲磨损**（要必中，概率路径）——本场景不碰；需要它时另开场景并接受概率或换夹具。
  3. **破零/破碎**：本场景的武器不会破零（正常耐久）；破碎链（1102/203/41）属 T-199，不在本场景。
  4. **火把 655**：另一族（T-199 已接线），本场景不覆盖。
  5. **RNG 逐抽对拍**：见 §1.1，需 `MIR2_RNG_LOG` 双开。

---

## 7. 牙 + sha256 + 门禁（实跑，命令 + rc）

场景 sha256：**`9d6701147f182934b92e765787ae5be71d19e3ec72aa0c29857c028238add9bb`**
（T1~T4 每次 `cp` 备份后逐字节还原，还原后 sha256 与基线一致）。

| 命令 | rc |
|---|---|
| `python -I -B tools/probe/mir2_probe.py --check-scenario --scenario tools/probe/scenarios/equip_struck_save.json` | **0**（`scenario OK: equip_struck_save (5 steps, params=self_pos=39977248)`） |
| `python -I -B tools/probe/mir2_probe.py --selftest` | **0**（8 checks） |
| `python -I -B tools/conformance/card_window.py --card openmir2-scenario-equip-struck-save --scenario tools/probe/scenarios/equip_struck_save.json --profile alt --dry-run` | **0**（两侧 `map=0 x=288 y=610` 图 700x700 可走） |
| `python -I -B tools/whitelist/t_reservation_gate.py check` | **0**（GREEN：预约表 208 行 / whitelist 208 号 / max=T-222） |
| **T1** 删关键 `send` 步（`CM_HIT`） | **0 —— 未达成，工具缺口** |
| **T2** `wait_for [642,203]` 改成不可达 `[4095]` | **0 —— 未达成，工具缺口** |
| **T3**（对照）删掉全部 wait 步 | **2**（N-5：`has no wait step`） |
| **T4**（对照）`wait_for` 越界 u16 `70000` | **2**（`out of range [0,65535]`） |

T1/T2 是卡面点名要求的牙，当前工具做不到（`--check-scenario` 只校验形状/必填/placement/「至少一个 wait」），
按卡面要求**如实登记、不写 PASS**，归属 **`openmir2-probe-scenario-validation-gaps`**；T3/T4 是当前工具确实能红的相邻牙。

---

## 8. 残留项（只登记，不改）

### 8.1 Rust 侧缺这条帧（会让窗口红）
- `mir2-rs/tests/parity/whitelist.md` **T-199** 的「范围外但相邻（既有登记）」原文：
  「`AttackSuccess` 的 `DoDamageWeapon`（`PlayObject.Attack.cs:225-229`）武器耐久磨损与 `RM_DURACHANGE(10125)` 未接（T-142 末段）」
  ——即**攻击方武器磨损整条未实现**。
- 受击方：`crates/world/src/combat.rs:824-841` 的 `struck_damage_durability` / `:767-775` 的 `wear_durability`
  **只改 `use_items[*].dura`**，没有事件通道；宿主的 `sync_use_items_from_entity_if_loaded`
  （`bins/game-svr/src/lib.rs:17664-17673`）只复制值、**不发帧**。
- 宿主里所有 642 的生产点都在**护身符/施法**路径（`lib.rs` 的 `spell` 用例族），与装备磨损无关。
- ⇒ 本场景作为窗口输入时，**Rust 侧必然缺 642**；修复前不得作为 PASS 依据。

### 8.2 工具缺口（T1/T2 + `self_pos` 同源）
- 需要：①「场景必须含某个 `send`」；②「`wait_for` ident 必须在消息表/可达集里」；③`placement` 与
  `self_pos`（`recog=(Y<<16)|X`）**同源校验**（改了 placement 不改 self_pos 现在仍 rc=0）。
  三条都归 **`openmir2-probe-scenario-validation-gaps`**（该卡已记同类：`send.tag=99`/`send.ident=9999` 均 rc=0）。

### 8.3 夹具要求
- 角色必须**装备一把有耐久的武器**（`characters_item` 的 `Position=ItemLocation.Weapon` 行，`Dura > 0`）。
  背包/装备夹具仍不在 `card_window.py` 能力内（runbook §1.5 末段，归 `openmir2-discovery-bagitems-201`）。
- 武器**未升级**（`characters_item_attr.VALUE10 == 0`）；否则走 `CheckWeaponUpgrade`，203/1102 会先出现。
- 旁观者：本场景只断言自己的帧；`SendDelItems`/`SendAddItem` 若在窗口内被别的玩家看到不影响本卡判据。

### 8.4 场景本身的已知限制
- `ClientHitXY` 的动作间隔门（部署 `HitIntervalTime=850`、`ActionIntervalTime=300`）用固定 `sleep 400`
  + `--between-ms 200` 兜；若窗口把 `--between-ms` 调成 0，仍有 400ms > 300ms 的余量。
- 目标可能被**真的打中**（`IsAttackTarget` 对玩家攻方恒 true，`BaseObject.Base.cs:377-470`）⇒ 会多出 NPC 侧的
  `SM_STRUCK` 一类帧；两侧同种子下应一致，但本卡不把它当判据（§2 末尾）。
- 本卡只在 worktree 内跑离线工具（`--check-scenario`/`--selftest`/`--dry-run`/`t_reservation_gate`），
  **零起栈、零连库、零端口**；`/e/tmp/w69b` 为本次离线分析临时目录，交付前已删除（见提交说明）。
