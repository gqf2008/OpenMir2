# run3 三场景前置分诊（2026-10-27，卡 `openmir2-window-run3-prereq-triage`）

> 只读分诊 + **离线**修复。**三个场景都没有真机复跑**：本轮窗口仍被占用，且冻结期禁止起栈/连库写。
> 所有结论的证据来自**已存在的产物**（`E:/tmp/window/run3/**` 的 `run.json` / `frames.ndjson` / `inventory_fixture.json`）
> 与**基线 dump**（`E:/tmp/conformance/baseline/mir2_test_db.sql`）——不新起进程、不连数据库。

> **返工记录（reviewer-21 `3dc30ae5`，独立重放 reviewer-20 `da3d6f5b` 的磁盘证据）**：
> 本卡第一版（`709e6277b`）在 n13/n17 上的归因**被推翻并已重写**——
> ① n13 不是「打不到身前格」，是**角色没武器**；② n17 不是「要换声明通道」，是
> **`card_window.py` 的占用谓词把「有行」当成「有物品」**（已改谓词 + 补牙，见 §n17）。
> 第一版里为 n17 加的 profile 迁移与 `--profile` 接线**已全部撤回**。
> 另外 reviewer-21 指出：`mir2_probe.py` 的 `Recorder`（`mir2_probe.py:161-171`，docstring 明写「把 s2c 帧流式写成
> frames.ndjson」）**只录 s2c**，`run.json` 的 `send` 条目是驱动器**自己的发送意图记录**（ident/recog/tag +
> `frame_hex`/`frame_sha256`），不是抓到的 c2s 流 ⇒ 本文不再把「c2s 帧证据」当判据（**探针能力缺口，另记**）。

三件事现在是三种性质：**n12 场景语义不成立 ⇒ 封存**；**n13 缺武器前置 ⇒ 场景自描述补齐**；
**n17 工具谓词 bug ⇒ 改谓词 + 三颗牙**。改动只落在
`tools/probe/scenarios/npc_script_chain.json`、`tools/probe/scenarios/equip_struck_save.json`、
`tools/conformance/card_window.py`（仅占用谓词 + selftest 牙）、`tools/window/run3-scenarios.json`、本文档。

---

## n12 `npc_script_chain`：**场景语义不成立 ⇒ blocked**（owner 裁决，2026-10-27；结论不变）

**裁决口径**：不是「夹具坐标写错」，而是**这条脚本链在 oracle 上不可达**——任何夹具修法都救不回来。
run3 里该行标 `blocked`，计数 `场景数 26（blocked 5 ⇒ 入批 21）`，与
`tools/hygiene/docs_delivery_gate.toml`、`docs/window-exec-sheet-20261025.md`、
`docs/needs-human-20261025.md` 三处同步一致（`run_batch.sh --list` 的 `expect` 也改成 5/21）。

**机制（与 `king_dialog` / `king_castle_war` 同族，`docs/window-runbook-20261023.md` §10）**：

| 环节 | 锚点 | 行为 |
|---|---|---|
| 点击入口 | `src/M2Server/Player/PlayObject.Base.cs:674-698`（`:688` 是 `FindMerchant(actorId) ?? FindNpc(actorId)`） | 第一臂先执行 |
| `FindMerchant` | `src/GameSrv/Word/WorldServer.cs:1446-1449` → `ActorMgr.Get<IMerchant>` = `(T)actor` **硬转**（`src/Modules/SystemModule/ActorManager.cs:58-61`） | 类型不符 ⇒ **抛 `InvalidCastException`**，第二臂 `FindNpc` **永远到不了** |
| 目标类型 | `Envir/Npcs.txt:3`（比奇国王 0122 29 32，type=1）→ `src/GameSrv/DB/LocalDB.cs:693-694` 造 `GuildOfficial : NormNpc`（`src/GameSrv/Npc/GuildOfficial.cs:14`，**不实现 `IMerchant`**） | 恒抛 |
| 异常去向 | `src/M2Server/Player/PlayObject.Message.cs:392-400`（`PlayObject.Run` 的 catch；`ident=1010≠0` ⇒ 不 `MakeGhost`） | 静默吞掉 ⇒ **0 帧**、不掉线 |

⇒ 场景第一个 `wait_for 643` **不可能到达**；Rust 侧放行 type=1（`fixed_npc_script_allowed`）会回 643 ⇒ 这是**真差异候选（R）**，
既不能当真机判据，也不许把 wait 改成「等任意帧」或加宽 ident 变绿。

**已落改动（`tools/probe/scenarios/npc_script_chain.json`）**：

1. 新增顶层 `blocked.reason`（schema 只允许 `reason` / `missing_fixture`；本场景**不是缺夹具**，故不写 `missing_fixture`），
   正文写明上面这条硬转链、Rust 侧差异、owner 裁决与本文指针；
2. **收敛旧说法**（reviewer-21 点名的那处矛盾）：原 `description` 与 `placement.note` 写「点国王出 643」
   「discovery 点出的 `643.recog` 必然是国王」——与「点击恒 0 帧」直接冲突，已改为「链是设计意图、oracle 上到不了第一个 643」，
   `643.recog` 那条假设**明确作废**；
3. 存活腿只剩**国王周期转身**的 `SM_TURN`（`src/GameSrv/Npc/GuildOfficial.cs:39-47`：`Random(40)==0 ⇒ TurnTo`，与点击无关）
   ⇒ `auto:last_turn` 仍能取到国王 actor id，但整条链仍不可达；
4. `steps[0].note` 加封存前缀（「本步在 oracle 上永不产生 643」）。

**不采用的替代**：① 换 discovery 到 `npc_click_king`——点国王同样 0 帧；② 把 placement 改到 map 3 的老人 (669,338)
并保留 `@@buildguildnow`——老人脚本里没有该标签，场景语义整体错位。

**离线自证**：`python -B tools/probe/mir2_probe.py --scenario tools/probe/scenarios/npc_script_chain.json --check-scenario --var npc_actor=1`
⇒ `scenario OK: npc_script_chain (12 steps, ...)`，`rc=0`（`blocked` 形状合法即放行；`card_window` 见 `blocked` 一律 rc=2 不跑）。

---

## n13 `equip_struck_save`：根因 = **基线角色没有武器** ⇒ 场景补「穿木剑」前置

**现象（既有产物）**：`E:/tmp/window/run3/equip_struck_save/csharp/run.json`：

| # | 步骤 | t_ms | 结果 |
|---|---|---|---|
| 0 | `click_npc pos:19,18` → 命中「卫家店」actor `554491892` | 3886 | 静态对象（首见 seq=20） |
| 1 | `wait_for 643` | 4087 | **matched**（`waited_ms=126`）⇒ NPC 真在、真应答 |
| 2 | `send 3010 CM_TURN recog=1245203 tag=0`（驱动侧意图记录） | 4414 | `frame_sha256=0dd80c33…` |
| 3 | `sleep 400` | — | — |
| 4 | `send 3014 CM_HIT recog=1245203 tag=0` | 5216 | `frame_sha256=83b7d711…` |
| 5 | `wait_for [642, 203]` | 5417 起 | **matched=false**（`waited_ms=3012`） |

`recog=1245203 = MakeLong(19,19)` = 自己坐标（`src/GameSrv/Word/WorldServer.cs:831-844` 的 `LoWord/HiWord` 口径），
`tag=0 = Direction.Up`（`src/OpenMir2/Enums/Direction.cs:5`）⇒ 落点与朝向都对：
`ClientHitXY`（`src/M2Server/Player/PlayObject.Attack.cs:823`）要求 `nX==CurrX && nY==CurrY` ✔，
`AttackDir`（`Attack.cs:24`）的目标 = `GetPoseCreate()` = `GetFrontPosition`（`src/M2Server/Actor/BaseObject.cs:977`）+`Envir.GetMovingObject`
⇒ (19,18) 的卫家店非空（步骤 1 的 643 已证）。`frames.ndjson` 在 4087 之后**一条 s2c 帧都没有**（下一条是 t=5559 的
`656 SM_CHANGENAMECOLOR`，与攻击无关），也没有「被拒」类回帧（若被 `CheckActionStatus`/间隔门拒，会走 `SendRefMsg(RM_MOVEFAIL)` 支）。

**真因：`UseItems[Weapon]` 为空。** oracle 两处各自独立的门：

- `AttackSuccess`（`Attack.cs:215-229`）只在 `targetObject == null` 时 early-return；本例非 null，于是走到
  `if ((nWeaponDamage > 0) && (UseItems[ItemLocation.Weapon] != null) && (UseItems[ItemLocation.Weapon].Index > 0)) DoDamageWeapon(...)`；
- `DoDamageWeapon`（`Attack.cs:1436-1440`）第一句就是 `UseItems[Weapon] == null || Index <= 0 ⇒ return`。

**而 run3 剖面的角色确实空手**——`E:/tmp/conformance/baseline/mir2_test_db.sql`（每侧起栈前 `restore-baseline.sh` 恢复的**同一份**基线）：

- 登录角色是 **`winplay0` / `PlayerId=6228`**（`upgrade_retrieve/card-window.log:156-163` 实测 `1 6228 0`；
  `placement.json` 的 `before.id=6228`；`save_settle.json` 的 `行=winplay0`）；
- `characters_item` 中 `PlayerId=6228` 共 **13 行**（`Position 0..12`，`ItemLocation.Weapon = 1`）**全部** `MakeIndex=0, StdIndex=0, Dura=0, DuraMax=0`；
- 旧文档锚的裁决之杖（`StdIndex=223`、`Dura=22198/32000`、`Position=1`）属于 **`PlayerId=1` = `LoginID='admin'` / `ChrName='gm01'`**，
  不是本剖面跑的角色。

⇒ **不是「打不到」，是「没有武器可掉耐久」**：无 642 ⇒ 无耐久写入 ⇒ `save_settle` 的 `no_change_observed`
（`changes=0`，907ms、5 次采样）是**同一因果的果**，不是第二个问题。

**已落改动（场景自描述，不换角色、不改超时）——`tools/probe/scenarios/equip_struck_save.json`**：

1. 顶层 `inventory` 声明木剑一件进 **槽 40**：`index=207, make_index=5001, std_mode=5, ani_count=1, dura=4000, dura_max=4000`
   （`stditems#207` 实测 `StdMode=5 / Shape=1`；`ani_count` 必须等于 `Shape`，写 0 会 rc=2）；
2. `params` 补 `make_index=5001 / takeon_where=1 / item_name=木剑`（与 `upgrade_retrieve` 同型，值固定、不依赖 discovery）；
3. `steps[0..1]` 新增前置：`CM_TAKEONITEM(1003)`（`recog=make_index`、`param=takeon_where=1` 即 `ItemLocation.Weapon`、
   `text_from=item_name`）→ `wait_for 615`（`SM_TAKEON_OK`，`src/OpenMir2/Messages.cs:284`）。等回执而不是 sleep，
   避免「还没穿上就挥击」变成时序假红；
4. `description` 补一段：这条前置为什么之前不存在（旧文档锚的是 gm01 的武器，跑的是 winplay0）。

**为什么槽 40 可写**：基线里 `Position=40` 是**空占位行**（`Id 409`，全零），而本卡同时修正的占用谓词
只把「`StdIndex`/`MakeIndex` 非 0」算占用（§n17）——两处改动是**同一条基线事实**的两面。

**离线自证（不连库、不起栈）**：

```text
$ python -B tools/probe/mir2_probe.py --scenario tools/probe/scenarios/equip_struck_save.json --check-scenario
scenario OK: equip_struck_save (9 steps, params=item_name=木剑,make_index=5001,self_pos=1245203,takeon_where=1)   rc=0
$ python -B tools/conformance/card_window.py --scenario tools/probe/scenarios/equip_struck_save.json --dry-run
[csharp] 背包夹具（dry-run 不连库）：db=mir2_test_db   声明 1 件 positions=[40] make_index=[5001] 角色=winplay0
[rust]   背包夹具（dry-run 不连库）：db=mir2_test_db_r 声明 1 件 positions=[40] make_index=[5001] 角色=winplay0
```

**下一窗口第一件要看的事**（本卡未验证）：C# 侧 `frames` 出现 `642`（`SM_DURACHANGE`）、`save_settle` 的 `changes > 0`。
若 615 先到而 642 仍缺，第二个怀疑对象是「木剑的 `SpecialPwr`/`AddAbil.WeaponStrong` 让 `nWeaponDamage = Random(5)+2-strong ≤ 0`」
（木剑 `Source=0 ⇒ WeaponStrong=0`，按 `PlayObject.cs:4424-4426` 不应发生）——那时要按 R 级真机数值再判。

---

## n17 `upgrade_retrieve`：**`card_window.py` 的占用谓词 bug**（「有行」当成「有物品」）

**现象**（`E:/tmp/window/run3/upgrade_retrieve/report.json` 的 note / `card-window.log:164-172`）：

```text
SELECT COUNT(*) FROM `mir2_test_db`.`characters_bagitem`
 WHERE `PlayerId` = 6228 AND `Position` IN (40, 41);
→ 2
inventory: 声明槽位里已有 2 行（[40, 41]）——DELETE+INSERT 会把该行其它列重置，请改用空槽或先清空
```

**基线静态证明（不连库）**：`E:/tmp/conformance/baseline/mir2_test_db.sql` 里 `characters_bagitem` 对
`PlayerId=6228` 共 **46 行**，`Position 0..45`（`Id 369..414`），**每一行**都是
`MakeIndex=0, StdIndex=0, Dura=0, DuraMax=0` 的空占位；`Position=40 → Id 409`、`Position=41 → Id 410`。
即：**40/41 没有被任何物品占用**，只有「存档约定的一槽一行、清空=置零」的占位行。
`INVENTORY_MAX_BAG = 46`（`card_window.py:2570`）⇒ 整个背包就是 0..45，**不存在「没有行的空槽」**
⇒ 旧错误文案建议的「改用空槽」在这份基线上不可实现；把槽位从 40/41 换到任何别的槽也一样红。

**谓词 bug 的位置与修法**（`card_window.py:2928-2940`）：

```sql
-- 修复前（只看行）：任何声明槽位只要有一行就 fail-closed
SELECT COUNT(*) FROM `db`.`characters_bagitem` WHERE `PlayerId` = ? AND `Position` IN (...);
-- 修复后（只看物品）：StdIndex/MakeIndex 同时为 0 的是空占位行，不算占用
SELECT COUNT(*) FROM `db`.`characters_bagitem` WHERE `PlayerId` = ? AND `Position` IN (...)
   AND (`StdIndex` <> 0 OR `MakeIndex` <> 0);
```

**为什么是 `OR` 而不是 `AND`**：`MakeIndex` 是带服务器号的合成键（DBSvr 写入必非 0；空行恒 0），
但**老物品行**完全可能只有 `StdIndex>0` 而 `MakeIndex=0`——写成 `AND` 会把它们当空槽，那就是**真的覆盖已有物品**。
三颗牙逐支钉住这三种形态（见下）。旧 `inventory` 通道的其它 fail-closed 语义（模板 `std_mode/ani_count` 比对、
写后读回、其它槽指纹不变）**一处未松**。

**三颗牙（`card_window.py --selftest`，编号 ⑲ / ⑳ / ㉑）**：假 runner `_OccupancyRunner` 不再返回常量，
而是**把 SQL 里的 `AND (…)` 谓词抠出来真的求值**（去反引号、`<>`→`!=`、`OR/AND` 小写化后 `eval`）——
这样牙咬的是 SQL 本身，而不是由假 runner 另写一套语义（那只是装饰）。变异实测（都在本 worktree 内做，恢复后
`sha256` 与变异前相同：`d6d17e2a0d0c`）：

| 变异 | 结果 | 咬住哪颗牙 |
|---|---|---|
| 整段谓词删掉（回到修复前） | `FAIL ⑲ 空占位行…不算占用 => rc=0`；`RED: 441/442`，rc=1 | ⑲（空占位不算占用） |
| `OR` → `AND` | `FAIL ㉑ 只有 StdIndex 非 0（老物品行）也算占用 => rc=2`；`RED: 441/442`，rc=1 | ㉑（OR 语义） |
| 不删检查、只留谓词 | `PASS ⑳ 真有物品的行…仍算占用 => rc=2` | ⑳（防「把检查整个删掉」） |

未变异时：`GREEN: card_window.py selftest 460/460`。

**同批次旁证**：`#14 bag_equip` / `#15 eat_item` 之所以在同一批里能替换槽 40/41（`before` 正是 `Id 409/410` 的全零行），
是因为它们走的是 `fixtures.inventory_replace`——那条路径**本来就没有这个谓词**。修复后两条通道对「空占位行」的判定一致。

**本卡对 n17 的改动 = 只有工具谓词 + 牙**：不给 `upgrade_retrieve` 加 profile、不改它的 `inventory` 声明、
不改 run3 #17 的 `extra`（第一版加的 `--profile tools/window/profiles/upgrade_retrieve.json` 已撤回）。

**残留（登记，不在本卡修）**：`fixtures.inventory_replace` 这条新通道**对「目标槽本来就有真物品」不做拦截**
（它会默默覆盖，只记 `before` 快照）；而旧 `inventory` 通道会 fail-closed。两条通道语义不一致，
要统一得另开卡（改的是新通道的契约，需要单独评审与真机回归）。

---

## 残余与未验证项（下一个窗口的入口）

1. **三处都没真机复跑**。开窗后按顺序看：#13 的前置两拍真的穿上木剑（`615`）且随后出 `642`、`save_settle.changes > 0`；
   #17 的 `inventory` 步骤不再报「声明槽位里已有 N 行」而是 `action=updated`；#12 不进批次（`run_batch --list` 标 `YES`）。
2. **探针能力缺口（另记）**：`mir2_probe.py` 的 `Recorder` 只录 **s2c**（`mir2_probe.py:161-171`），
   `run.json` 的 `send` 条目只是驱动侧意图记录 ⇒ 任何「c2s 帧证据」类判据目前都**没有数据源**。
   要拿 c2s 面必须另开卡给 Recorder 加方向字段（会改变 `frames.ndjson` 的既有消费方语义，需要单独评审）。
3. **n13 的通用解仍缺通道**：`fixtures` 只有 `characters_bagitem` 的写口，**没有装备位（`characters_item`）声明通道**；
   本卡用「背包 + `CM_TAKEONITEM`」绕过（与 `upgrade_retrieve` 同型）。要直接预置装备位仍需另开卡。
4. 本轮**没有**改动 `mir2-rs/**`、`src/**`、`sql/**`、`Envir/**`、`compare_runs.py`、rules.json 与任何超时；
   `tools/conformance/card_window.py` 只改占用谓词一处 + 新增 selftest 牙（3 颗）。
