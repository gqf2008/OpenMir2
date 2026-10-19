# Lane C — `openmir2-monster-attack-cadence-lead` 定量核查

> 只读车道：本文件之外未改动仓库任何文件；所有 capture 均从原目录只读解析。
> 统计脚本：`E:/tmp/lane-c-scan.py`、`E:/tmp/lane-c-scan2.py`、`E:/tmp/lane-c-perhitter.py`、`E:/tmp/lane-c-edcode.py`、`E:/tmp/lane-c-final.py`（均可重跑）。
> 结论日期 2026-10-19；基线 master `049771a9b`。

## 0. 结论（三档）

### A. 无差异（`SM_STRUCK` 的**攻击节拍门 / 伤害口径 / 目标有效性**）

- **每只怪的攻击间隔两侧同值**：把 `SM_STRUCK` 尾 `Tag1`（= `processMsg.nParam3` / Rust `hitter_id`）按怪分组后，
  - Rust 7 个会话共 **92 个同怪间隔，最小 3.147 s**，`p10=3.196 s`，**0 条 < 3.0 s**；
  - C# 8 个会话共 **25 个同怪间隔，最小 3.190 s**，`p10=3.246 s`，**0 条 < 3.0 s**。
- 这与内容库（`mir2_data.monsters`，只读查询）完全吻合：`(605,578)` 五条刷怪线的怪
  `WALK_SPD/ATTACK_SPD = 1500/2500`（稻草人 83、半兽人 83、多钩猫 81、钉耙猫 81）与 `500/3000`（鹿 52）。
  oracle 的门是「**只在 walk-due tick 上**判 `now - AttackTick > NextHitTime`」
  （`MonsterObject.cs:227` + `MonsterObject.cs:245-250`），
  稳态间隔 = 下一个 1500 ms 整倍且 > 2500 ms ⇒ **3000 ms + tick 量化**；观测 3.15–3.51 s 正好落在这里。
- 伤害序列同量级：Rust L1 单次 1–7（均值 2.5–5.0）；C# L1 单次 1–5（均值 1.0–3.3，10-18 五趟 1.88–3.33）。
  最快的一趟 C# 围殴（`s2c3`：20 伤害 / 6.4 s，21.0 s 死亡）与 Rust `w02c`（20 伤害 / 4.7 s，10.48 s 死亡）同阶。
- 目标有效性/索敌实现逐行对照后未发现会改变节拍的差异（见 §3）。

### B. 确证缺陷（**与节拍无关**，但会让「怪打玩家」在客户端看起来异常）

1. **怪物 AI 移动的 `SM_WALK(11)` 完全不下发**：Rust 全部 13 个 capture **0 条** `SM_WALK`；
   C# 8 个有效 capture **17–76 条**。
   - oracle：`BaseObject.WalkTo`（`BaseObject.cs:601`）→ `Walk(RM_WALK)`（`:675`）→ `BaseObject.Walk`（`:1525`）
     → `SendRefMsg(nIdent, …)`（`:1571`）→ 观察者 `PlayObject.Message.cs:1403-1410` 回 `SM_WALK`。
   - Rust：`monai.rs::walk_to`（`crates/world/src/monai.rs:134-144`）→ `World::try_step_now`
     （`crates/world/src/world.rs:6028`）→ `World::try_step`（`world.rs:6082`）只改 `e.x/e.y`，
     **不产生任何事件**；宿主 `downlink.rs::move_frame` 只被 `CM_WALK` 路径使用。
   - 后果：客户端看不到怪移动（怪会「站着/瞬移贴脸」），但**服务端坐标与攻击时点不变**。
2. **怪物普攻的 `SM_HIT(14)` 不下发**：Rust `SM_STRUCK` 5–66 条的 capture 里 `SM_HIT` **0 条**；
   C# 同批 **5–26 条**（通常 ≈ 受击条数）。
   - oracle：`AnimalObject.Attack`（`AnimalObject.cs:167-171`）在 `AttackDir` 后 **无条件**
     `SendAttackMsg(RM_HIT, …)` → 观察者 `PlayObject.Message.cs:1433-1440` 回 `SM_HIT`。
   - Rust：通用 `monai.rs::attack_target`（`:486-535`）只 `world.attack_once` + 记 `pending_ai_attacks`，
     **不调** `send_ref_msg(…, RM_HIT, …)`；只有专用怪覆写路径（如 `monai.rs:1027`）才发。
   - 后果：客户端看不到挥砍动作帧；同样**不影响服务端攻击时点**。

两条都建议单开卡（不是本卡），并补「改坏必红」帧断言：怪原地走 1 格 ⇒ 观察者必收 `SM_WALK`；
怪打玩家 1 次 ⇒ 观察者必收 `SM_HIT`。

### C. 证据不足（原线索「Rust 14 s 内被 5–8 次打死 vs C# 30 s 仅 2 次」不能作为节拍缺陷证据）

- **C# 对照样本不满足「同点」**：`csharp/s2c-w01n` 的 `SM_NEWMAP(51)` 是 **`(605,588)`**，
  不是 `(605,578)`；Rust 三趟全是 `(605,578)`。10 格的 y 偏移正好把玩家放进/移出刷怪带。
- **同点 C# L1 只有 1 趟**（`csharp/s2c`，`(605,578)`）：5 次受击、46.6 s 未死、只有 1 只怪出手。
- **怪物「目标数」由该会话的生成/游走位置决定**：按 `SM_TURN(10)` 的 `param/tag` 还原进场时各 actor 坐标，
  以「距玩家 Chebyshev ≤ 7（= 玩家 `ViewRange 12` − 怪 `ViewRange 5`，`PlayObject.cs:3129-3139`）
  且为主动索敌怪（排除 `raceImg=11` 鹿、`raceImg=10/12` NPC）」计：
  - Rust L1 三趟：**6 / 4 / 7** 只主动索敌怪在投递半径内；
  - C# L1 同点 `s2c`：**1** 只（另外两只在 2/4 格的是 **鹿 race 52**——oracle 里 `CreateMonster` 用普通
    `MonsterObject`（`WorldServer.MonGen.cs:526-544`），**不继承 `ATMonster.Run`，从不主动索敌**，
    只有挨打才 `Struck → SetTargetCreat`）；
  - C# L1 10-18 五趟 `(605,581)`：**1 / 2 / 4 / 3 / 4** 只，死亡 28.9 / 21.0 / 存活 / 存活 / 19.9 s ——
    **与 Rust 同阶**。
- 因此「Rust 死得快」在当前样本里可以由「该会话恰好有 4–7 只主动索敌怪在半径内」解释；
  但样本量（同点 C# L1 = 1 趟）不足以证明两侧**分布**无差异，需按 §6 补采样。

---

## 1. 口径（很重要，否则会读错字段）

`SM_STRUCK(31)` 的字段定义：

- oracle `PlayObject.Message.cs:1601`：
  `MakeMessage(SM_STRUCK, ActorId, baseObject.WAbil.HP, baseObject.WAbil.MaxHP, processMsg.wParam)`
  ⇒ `Recog` = 受击者、`Param` = HP、`Tag` = MaxHP、`Series` = 本次伤害。
- **Rust 生产默认走 `HIGHHP` 客户端布局**（`bins/game-svr/src/smframe.rs:837-895`）：
  `Param` = `LoWord(HP)`、`Tag` = `HiWord(HP)`、尾 `Param1` = **MaxHP**、`Series` = 本次伤害。
  ⇒ **不能把 Rust 的 `tag` 当 MaxHP**；本报告用尾 `Param1` 判等级。
- 尾 `Tag1` = 命中者 `ActorId`（两侧同），本报告用它做**逐怪**间隔；头部 `Recog` 做入站/出站区分。

等级判定（MaxHP）：

| 侧 | capture | MaxHP | 判定 |
|---|---|---|---|
| Rust | `s2c-w02` / `w02b` / `w02c` | 19 | **L1（纳入对照）** |
| Rust | `s2c-w02d` / `w02e` / `s2c-live` | 419（live 末 3 帧 442，升级） | L30（排除） |
| C# | `s2c` / `s2c-w01n` / `s2c2..6` | 19 | **L1（纳入对照）** |
| C# | `s2c-w01b` | 442 | L30（排除） |

> 任务给的 capture 清单里 **`s2c-w02d`/`w02e` 都是 L30**；真正的三趟 L1 是 `s2c-w02`/`w02b`/`w02c`
> （清单里没列，但目录存在且是唯一与 C# L1 可比的一组）。
> 实际死亡时刻：`w02` **19.16 s / 5 次**、`w02b` **13.85 s / 8 次**、`w02c` **10.48 s / 4 次**
> （「~14 s 内 5–8 次」对 `w02b` 成立，对 `w02`/`w02c` 不精确）。

---

## 2. 统计表

`hits_in` = 入站 `SM_STRUCK`；`hitters` = 不同 `Tag1` 数；`active≤7` = 进场时距玩家 ≤7 且主动索敌的 actor 数；
`min per-hit gap` = 同一 `Tag1` 相邻两击的最小间隔；`death` = 入站 `SM_NOWDEATH(34)`。

| 运行 | capture | enter | MaxHP | hits_in | hitters | active≤7 | 每次伤害 | 合计/均值 | 首/末击(s) | 死亡(s) | min 同怪间隔(s) |
|---|---|---|---|---|---|---|---|---|---|---|---|
| Rust L1 #1 | `rust/s2c-w02` | (605,578) | 19 | 5 | 4 | 6 | 2,1,4,1,7 | 15 / 3.00 | 3.33 / 19.16 | 19.16 | 3.202 |
| Rust L1 #2 | `rust/s2c-w02b` | (605,578) | 19 | 8 | 6 | 4 | 2,1,2,4,6,1,2,2 | 20 / 2.50 | 7.43 / 14.24 | 13.85 | 3.604 |
| Rust L1 #3 | `rust/s2c-w02c` | (605,578) | 19 | 4 | 3 | 7 | 7,3,4,6 | 20 / 5.00 | 5.74 / 10.48 | 10.48 | 3.147 |
| Rust L30 #4 | `rust/s2c-w02d` | (605,578) | 419 | 35 | 7 | 8 | （见 §2.1） | 130 / 3.71 | 3.56 / 41.21 | 存活 | 3.164 |
| Rust L30 #5 | `rust/s2c-w02e` | (605,578) | 419 | 13 | 6 | 8 | 5,1,5,7,5,2,4,3,2,1,6,5,9 | 55 / 4.23 | 2.84 / 15.86 | 15.66 | 3.187 |
| Rust L30 W0-2 | `rust/s2c-live` | (605,578) | 419 | 59 | 8 | 9 | （略） | 263 / 4.46 | 2.65 / 64.40 | 存活 | 3.193 |
| C# L1 probe | `csharp/s2c` | **(605,578)** | 19 | 5 | 1 | 1 | 1,2,1,2,1 | 7 / 1.40 | 13.30 / 46.59 | 存活 | 3.246 |
| C# L1 W0-1 | `csharp/s2c-w01n` | **(605,588)** | 19 | 2 | 1 | 1 | 1,1 | 2 / 1.00 | 6.01 / 27.46 | 存活 | 21.448 |
| C# L30 | `csharp/s2c-w01b` | (605,578) | 442 | 12 | 3 | 0 | 4,3,4,1,2,3,4,2,4,2,4,3 | 36 / 3.00 | 23.07 / 77.12 | 存活 | 3.245 |
| C# L1 10-18 #2 | `csharp/s2c2` | (605,581) | 19 | 7 | 5 | 1 | 2,3,4,3,4,3,4 | 23 / 3.29 | 13.11 / 28.90 | 28.90 | 3.190 |
| C# L1 10-18 #3 | `csharp/s2c3` | (605,581) | 19 | 6 | 4 | 2 | 4,2,3,3,4,4 | 20 / 3.33 | 14.60 / 21.00 | 21.00 | 3.507 |
| C# L1 10-18 #4 | `csharp/s2c4` | (605,581) | 19 | 6 | 4 | 4 | 3,2,2,1,2,5 | 15 / 2.50 | 10.71 / 37.05 | 存活 | 3.306 |
| C# L1 10-18 #5 | `csharp/s2c5` | (605,581) | 19 | 8 | 5 | 3 | 2,2,2,1,2,2,3,1 | 15 / 1.88 | 13.39 / 21.38 | 存活 | 3.290 |
| C# L1 10-18 #6 | `csharp/s2c6` | (605,581) | 19 | 6 | 4 | 4 | 2,4,4,4,4,1 | 19 / 3.17 | 10.42 / 19.93 | 19.93 | 3.252 |
| Rust 10-18 A | `rust/s2cA` | (605,581) | 19 | 3 | 1 | 2 | 6,8,7 | 21 / 7.00 | 3.32 / 27.37 | 27.57 | 9.618 |

Rust L30 #4 伤害序列：`8,4,6,1,5,4,6,6,3,7,2,2,3,4,8,2,1,7,2,1,1,3,9,1,1,1,8,1,3,6,1,2,6,1,4`。
Rust L30 W0-2 伤害序列：`3,8,4,9,7,1,4,1,1,8,7,6,4,7,2,1,4,5,1,3,1,4,6,7,3,2,4,3,7,6,1,1,8,3,1,8,8,6,9,5,7,3,4,4,7,6,2,9,8,1,3,2,6,6,2,2,2,5,5`。

### 2.1 同怪间隔分布（判别「节拍门」的核心）

| 侧 | n | min | p10 | p50 | max | <2.0 s | 2.0–2.9 s | 3.0–3.5 s | >3.5 s |
|---|---|---|---|---|---|---|---|---|---|
| Rust | 92 | **3.147** | 3.196 | 6.415 | 44.93 | **0** | **0** | 36 | 56 |
| C# | 25 | **3.190** | 3.246 | 3.399 | 23.10 | **0** | **0** | 14 | 11 |

> `>3.5 s` 的多数是 3.2 s 的整倍数（6.4/9.6/12.8/16.0 s）⇒ 怪中途失去邻接/目标，不是门被放宽。
> 两侧 p50 的差异（6.4 vs 3.4）反映的是「怪是否持续贴住玩家」，不是单怪攻击冷却。

### 2.2 关键对照

- **Rust 的 3.147 s 最小值与 C# 的 3.190 s 最小值相差 43 ms**；DB `ATTACK_SPD=2500` + `WALK_SPD=1500`
  的理论下限 = 3000 ms + tick 量化。两侧命中同一门。
- **C# 也会被围殴致死**：`s2c3` 21.0 s、`s2c2` 28.9 s、`s2c6` 19.9 s（L1、同图）。
  所以「C# 不会快速死亡」不成立；「C# 这一趟只有 1 只主动索敌怪在半径内」才是 `w01n` 的真因。

---

## 3. 逐条代码对照（oracle ↔ Rust）

| # | 行为 | oracle | Rust | 判定 |
|---|---|---|---|---|
| 1 | 攻击时间门 | `MonsterObject.cs:227` `now - AttackTick > NextHitTime`（严格大于） | `monai.rs:496-500`（`attack_target`）同式 | **一致** |
| 2 | 攻击只在 walk-due tick 上判 | `MonsterObject.cs:245-250` `!WalkWaitLocked && now-WalkTick > WalkSpeed` 分支内 `AttackTarget()` | `monai.rs:3252-3340`（`monster_run`），`walk_due` 在 `:3314`，攻击分支在其后 | **一致** |
| 3 | `NextHitTime` 来源 | `WorldServer.MonGen.cs:906-909` `NextHitTime = monster.AttackSpeed`、`WalkSpeed = monster.WalkSpeed` | `bins/game-svr/src/mongen.rs:806-807` 同字段；`crates/data/src/loaders.rs:264-272` 与 `MySqlDB.cs:213-223` 同口径（`MAX(200,…)`、下限 200） | **一致** |
| 4 | 默认值 | `BaseObject.cs:446-447` `WalkSpeed=1400`、`NextHitTime=2000` | `entity.rs:369/376` `2000/1400` | **一致** |
| 5 | `AttackTick` 初值 | `AnimalObject.cs:76` `now - Random(3000)` | `entity.rs:145` + `seed_from_spawn`（构造抽数一次、不重抽） | **一致** |
| 6 | 索敌节拍 | `ATMonster.cs:13-24` `(now-SearchEnemyTick)>8000 \|\| ((now-SearchEnemyTick)>1000 && TargetCret==null)` | `monai.rs:3270-3282` 同式（含 105/92 补门） | **一致** |
| 7 | 索敌集合 | `ATMonster` 直派生（含 82/84/90/91/97/106/119 等） | `monai.rs:642` + `:661-665` `is_active_seek_race` 已含 `53,81,82,83,84,86,88,89,90,91,97,106,118,119,130` | **一致**（审计 G08 已在当前树闭合） |
| 8 | `SearchTarget` | `AnimalObject.cs:388-416` | `monai.rs:240-320`（含 `RemoveAt` 跳项怪癖） | **一致**（G05/A16 的已知口径差见 §5） |
| 9 | 目标有效性 | `BaseObject.Base.cs:377-465`；`Race>=Animal && Master==null` ⇒ 打玩家 | `combat.rs:432-465` `is_attack_target_base` 同支 | **一致**（本场景 Master==null） |
| 10 | 邻接判定 | `BaseObject.cs:1264-1313` `GetAttackDir` 八邻格 | `monai.rs:171-205` `get_attack_dir` 同序 | **一致** |
| 11 | 移动门 | `MonsterObject.cs:245-250` `now-WalkTick > WalkSpeed` | `monai.rs:3314-3330` 同式 | **一致** |
| 12 | 视野投递半径 | `PlayObject.cs:3129-3139` `|Δ| <= ViewRange - baseObject.ViewRange`（玩家 12、怪 5 ⇒ 7） | `aoi.rs:252-257` `span = view_range - other.view_range`，两轴同判 | **一致** |
| 13 | 怪物接受视野的节流 | `MonsterObject.cs:136-141` `(now-SearchTick) <= SearchTime ⇒ return` | `aoi.rs:76-114` `update_monster_visible` 同式 | **一致** |
| 14 | 怪物移动帧 | `BaseObject.cs:1525/1571` → `RM_WALK` → `PlayObject.Message.cs:1403` `SM_WALK` | `walk_to`/`try_step` **无事件** | **不一致（见 §0-B1）** |
| 15 | 怪物挥砍帧 | `AnimalObject.cs:167-171` `SendAttackMsg(RM_HIT)` → `SM_HIT` | 通用 `attack_target` **不发** | **不一致（见 §0-B2）** |
| 16 | 驱动频率 | `ProcessMonsters` 200 ms + 逐怪 `RunTime=250`（D04）、`ProcessMonsterInterval=2`（D05） | `monster_ai_tick` 每 200 ms 无条件跑全部怪 | **已知登记差**（审计 D04/D05）；只影响 AI 评估频率，**不改变 walk-due/攻击门**，故不改同怪间隔 |

> 第 16 条的量化影响：Rust 每 200 ms 评估一次、walker 门 1500 ms ⇒ walk-due 时刻量化 0–200 ms；
> C# 200 ms 线程 + 250 ms `RunTime` ⇒ 0–450 ms。观测同怪间隔 Rust 3.147–3.604、C# 3.190–3.507，
> 正好是同一门的两种量化，**不构成缺陷**。

---

## 4. 「目标数不同」的定量解释（不是节拍）

进场时按 `SM_TURN(10)` 还原的 actor 坐标（示例，`rust/s2c-w02b`，玩家 `(605,578)`）：

- 4 只**主动索敌**怪在 ≤7：`921`(稻草人 18) d4、`1590`(猫 17) d4、`904`(稻草人 18) d7、`1125`(稻草人 18) d7；
- 另有 `1231`/`1237`/`1238`/`1235` 等在半兽人 19，位于 d8–d12，靠玩家/怪移动后进入。

同一时刻 `csharp/s2c`（玩家 `(605,578)`）：

- ≤7 的三只是：`252974053`(鹿 11) d2、`619109316`(鹿 11) d4、`817813679`(稻草人 18) d6；
- **只有 1 只是主动索敌怪** ⇒ 5 次受击全部来自 `817813679`（`Tag1` 唯一），与 oracle 对鹿的实现完全一致。

> 鹿（race 52）在 `WorldServer.MonGen.cs:526-544` 是普通 `MonsterObject`（或 1/30 的 `ChickenDeer`），
> **不继承 `ATMonster.Run`**，因此不主动索敌。把鹿计入「围殴的怪数」会高估两侧的差异。

`csharp/s2c-w01n` 的问题更直接：`SM_NEWMAP` 是 `(605,588)`，玩家随后每次 `CM_WALK(dir=4)` 向下走，
离刷怪带越来越远；它只有 2 次受击是几何结果，不是节拍结果。

---

## 5. 不确定性（为什么只能判「证据不足」而不是「无任何差异」）

1. **同点 C# L1 样本 = 1 趟**（`csharp/s2c`）；Rust L1 = 3 趟。要断言「主动索敌怪数分布」两侧相同，样本不够。
2. **进场坐标失控**：`csharp/s2c-w01n` 的 `(605,588)` 与任务口径 `(605,578)` 不符。同一天两次 C# 起点
   `(605,578)`/`(605,588)`，说明「每趟起栈前 UPDATE」没有在该趟生效（或被上一趟的存档位置覆盖）。
3. **本地怪密度差**：进场可见 actor 数 Rust 11–21、C# 9–14（10-18 同点 `(605,581)` Rust 13 vs C# 9–14）。
   MonGen 在该点是 190 只 / Range=50；12 格视野理论期望 ≈ 11.9 只。Rust 的 16–21 略偏高，
   但不足以证明「生成半径/密度」有系统差——需要服务端权威坐标 dump。
4. **已登记的时序差会移动「首击时刻」但不移动「同怪间隔」**：
   - D03：玩家 `SearchViewRange` 节拍 Rust 200 ms vs oracle ~1000 ms（多 5.25×）；
   - D04：逐怪 `RunTime=250` 缺失（AI 评估 5/s vs ~4/s）；
   - D05：`ProcessMonsterInterval=2` 缺失（不可见怪评估 ~5/s vs ~1.3/s）；
   - G10/V01：`RM_UPDATEVIEWRANGE` + `SearchTime` 的「可见集到达时刻」口径不同（审计已标「需新卡」）。
   这些能解释首击 3–7 s（Rust）vs 10–14 s（C#）的一部分，但**不能解释同怪间隔**（已实测同门）。
5. **`SM_WALK` 缺失使客户端 capture 无法直接测「怪的贴身速度」**：只能间接从首击时刻反推；
   服务端坐标变化没有落到 wire 上。补服务端 dump 才能把「移动速度」也钉死。

---

## 6. 下一步最小采样方案（建议直接作为该卡的验收判据）

**目的**：把「节拍」与「目标数/几何」彻底分离，且每步机器可判。

1. **每侧 3 趟，每趟独立重置**（窗口内，owner 授权）：
   - 同一角色、同一点 `(605,578)`、`Level=1`、`Hp=MaxHP=19`；
   - **每趟起栈前执行** `UPDATE characters SET MapName='0', CX=605, CY=578`，并**校验** `SM_NEWMAP.param/tag == (605,578)`，
     不符即作废重跑（本轮的 `w01n` 就是在这里失守）；
   - 客户端驱动只用固定配方（不点怪），持续时间 ≥ 60 s 或死亡为止。
2. **每趟从 capture 机器判三条**：
   - ① 入站 `SM_STRUCK` 按 `Tag1` 分组，**每个 `Tag1` 的相邻间隔 ≥ 3000 ms**（DB `ATTACK_SPD=2500` + `WALK_SPD=1500`）；
   - ② 每次伤害 `Series` 落在该 `Tag1` 对应怪种的 `Lo(DC)..Hi(DC)` 减 AC 后的范围内
     （稻草人 1–2、猫 2–4、半兽人 4–9；鹿不主动攻击）；
   - ③ 首击时刻与 `min(距玩家 Chebyshev) / WalkSpeed` 的几何预测差 ≤ 1.5 s。
3. **补一档服务端权威采样**（消除 `SM_WALK` 缺失带来的盲区）：
   - 在 `MIR2_RNG_STATE_LOG` 的 dump 上加**可选**的全 actor 坐标（或复用 `--record-provenance` 旁路），
     每个玩家 `Run` 落一次 `x,y,target,attack_tick,walk_tick`（只读诊断，默认关）；
   - 用它直接算「每怪 walk-due 次数 / 移动格数 / 首击 delay」，与 capture 的 `SM_STRUCK` 对照。
4. **正向对照（确证 §0-B 两条）**：同一套窗口里让一只怪走 1 格并打玩家一次，
   断言客户端 capture 出现 `SM_WALK(11)` ≥1 与 `SM_HIT(14)` ≥1；当前 Rust 侧必红。
5. **判级**：① 属 R（时序）；② 属 R（战斗数值）；③ 属 R（时序）；第 4 条属 L（帧面）。

---

## 7. 复现

```sh
# 全部只读；输出仅本文件与 /e/tmp/lane-c-*.py
python -I /e/tmp/lane-c-scan2.py      # 全 capture 入站/出站/死亡/伤害
python -I /e/tmp/lane-c-perhitter.py  # 按 Tag1 分组的同怪间隔
python -I /e/tmp/lane-c-final.py      # §2 统计表（含 MaxHP/enter/active≤7）
python -I /e/tmp/lane-c-edcode.py     # EDCode 尾解码（Rust MaxHP 在尾 Param1）
```

内容库只读核对（`mysqld:3306`，`mir2_data`）：

```sql
SELECT HEX(Name),Race,RaceImg,Lvl,HP,DC,DCMAX,SPEED,HIT,WALK_SPD,WalkStep,WaLkWait,ATTACK_SPD
FROM mir2_data.monsters
WHERE HEX(Name) IN ('E9B9BF','E7A8BBE88D89E4BABA','E58D8AE585BDE4BABA',
                    'E5A49AE992A9E78CAB','E99289E88099E78CAB');
-- 鹿 52/11, 稻草人 83/18, 半兽人 83/19, 多钩猫 81/17, 钉耙猫 81/17
-- WALK_SPD/ATTACK_SPD: 500/3000 (鹿), 1500/2500 (其余)
```

## 8. 残留物清单

- 仅 `E:/tmp/lane-c-cadence.md` 与本目录 5 个 `lane-c-*.py` 统计脚本；未建 worktree、未改仓库文件、
  未碰 `E:/MirServer` 部署件、未起任何游戏进程。


---

## 评审员的更正（2026-10-19，**以本节为准**）

- **C-1（最重要）**：上述 L1 样本**并非满 HP**——首条入站 `SM_STRUCK` 的 HP 字段反推起始 HP：w02≤12、w02b≤17、w02c≤19、C# s2c≤18、w01n≤14。`w02` 总伤害 15 却以 MaxHP=19 死亡。⇒ 「Rust 死得快」**不能归因于怪数/节拍**，起始 HP 至少同样有力；统计表须标注「起始 HP 未控，不可比」。
- **C-2**：`w02b` 的 8 次含 **2 条死后受击**（死亡 13.85s，此后 14.05/14.24 各一条，HP=0）⇒ 死亡前是 6 次。
- **C-3**：理论节拍不是 3000 ms。Rust 200 ms tick 下 walk 周期 1600 ms ⇒ 攻击周期 ~3200 ms；oracle `Thread.Sleep(20)` + `RunTime=250` ⇒ ~1560/3120 ms。观测 min 3.147/3.190 与 ~3.1–3.2 s 一致。**门相同、量化不同**。
- **C-4**：因此「D04/D05 不改变同怪间隔」过强；正确表述是「门相同、量化差 ~80 ms，分布未被证伪」。
- **C-5**：B1（怪移动不发 `SM_WALK`）**不是新发现**——`tests/parity/world/monster-ai-frames-20261013/README.md:46` 已登记；同类还有 AI `turn_to` 的 `SM_TURN` 未接。
- **C-6**：B2（怪普攻不发 `SM_HIT(14)`）**成立且未在既有登记中找到**（`monai.rs:486-535` 只写 `pending_ai_attacks`；`drain_combat_downlink` 只出 `SM_STRUCK`）⇒ 单开卡；牙必须断言头部字段/收件人/与 `SM_STRUCK` 的同 tick 顺序，不能只数条数。
- **C-7**：「主动索敌怪数」是代理指标（按 raceImg 排除 10/11/12，未读 race）⇒ 降级为「与几何一致的解释之一」。
- **C-8**：`WalkStep/WalkWait` 未接线（`bins/game-svr/src/mongen.rs:803`、`entity.rs:372-373` 缺省 0/0；oracle `MySqlDB.cs:214-215` 是 `MAX(1,WalkStep)`）——本报告未交代，属既有缺口。
