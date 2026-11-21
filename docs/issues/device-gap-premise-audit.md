# 装置缺口前提审计（7 条 `implemented_unverified` 的 magic 叶子）

> 状态：**只读审计**（2026-11-21，base master `2414209c3`）；**不改代码、不改装置、不起构建**。
> 目的：实现批开工前核对「计划里声称的装置缺口」是否成立。今天已出过两次同类事故
> （`ServerConf` 缺省=0、640 tag 分层），本文件是第三次预防。
> 本文件结论已被 `docs/issues/qa-next-batch-plan-20261121.md` 对应 6 行引用（前提更正来源）。

## 0. 结论速览

**成立 2｜部分成立 3｜不成立 1｜成立但计划漏项 1**（共 7 条）：

| 条目 | 计划声称 | 结论 |
| --- | --- | --- |
| MG-16（8 抗拒火环） | 视野回填 + 可走格 `CanWalk` | **部分成立**（表空成立；`CanWalk` **不适用**） |
| MG-35（21 瞬息移动） | 施法后坐标观测缺失 | **不成立**（落点在 807 帧的 `param/tag` 里；真缺口是 `HomeMap` 旋钮） |
| MG-57（37 气功波） | 同 MG-16 | **部分成立**（同 MG-16） |
| MG-34（20 诱惑之光） | 观测面缺口 | **成立**（观察面）；**另漏一条「缺省≠部署」** |
| MG-36（22 火墙） | 跨 tick 伤害不可观测 | **成立**（伤害面；登记面可观测） |
| MG-55（46 分身术） | 克隆体观测 + 记 10/41 | **部分成立**（10/41 已在记；缺的是克隆体字段与 ActorId 归一化） |
| MG-42（41 精灵召唤） | 数据缺口阻塞 golden | **成立（生产数据）**，但「阻塞出 golden」表述要改（harness 可 stub） |

## 1. 四条需要改口径的细节

### 1.1 MG-35（21）**不成立**：落点已可观测，真缺口是 `HomeMap` 旋钮
- 同图 `SpaceMove` 发 `SendRefMsg(RM_SPACEMOVE_SHOW2, Dir, CurrX, CurrY, 0, "")`
  （`BaseObject.cs:1137`；`nInt == 0` 时为 `RM_SPACEMOVE_SHOW`，`BaseObject.cs:1141`）。
- 收件人侧 handler 把它转成 `SM_SPACEMOVE_SHOW2`/`SM_SPACEMOVE_SHOW`，payload =
  `nParam1(CurrX) / nParam2(CurrY) / MakeWord(wParam=Dir, Light)`（`PlayObject.Message.cs:1967-1976`），
  且**无「排除自己」守卫** ⇒ 施法者与旁观者都会收到。
- ⇒ golden 的 `Sent` 面（`param/tag`）**就是落点**，不需要新观测字段。
- **真缺口**：harness 全文没有设置 `caster` 的 `HomeMap`，`MapRandomMove`（`BaseObject.cs:1217`
  `SystemShare.MapMgr.FindMap(sMapName)`）拿到空名 ⇒ 返回 null ⇒ 不移动、不出 807/801。
  ⇒ 实现批只需 **配置旋钮 `HomeMap="0"` + `EnableMaps`**（`OfflineMapSystem` 已在）。

### 1.2 MG-16 / MG-57（8/37 推挤）**部分成立**：只需视野回填，`CanWalk` 不适用
- `MagPushArround` 遍历 `playObject.VisibleActors`（`src/M2Server/Magic/MagicManager.cs:27`）。
- 表默认空：`BaseObject.cs:407` 构造时 `VisibleActors = new List<VisibleBaseObject>()`；唯一填写点
  `UpdateVisibleGay`（`BaseObject.ViewRange.cs:12`），调用者只在广播路径（`BaseObject.ViewRange.cs:114`、
  `PlayObject.cs:3131`）；harness 的 `MagicGolden/*.cs` 对 `VisibleActors|UpdateVisibleGay` **0 命中** ⇒ **表空成立**。
- **`CanWalk` 不适用**：harness 载的是真图（`mir2-rs/tests/parity/csharp/MagicGolden/Program.cs` 的
  `LoadMapData("0.map")`），`CharPushed`（`BaseObject.cs:854`）的 `Envir.CanWalk`（`BaseObject.cs:865`）
  因表空**一次都不会被调用**（不是"返回 false"）⇒ 批次成本下调：**只补视野回填**，不新造地图/可走面。

### 1.3 MG-55（46）**部分成立**：10/41 已在记，缺的是克隆体字段与 id 归一化
- `MagMakeClone`（`src/M2Server/Magic/MagicManager.cs:1337-1341`）只有 `new PlayCloneObject(...); return true;`。
- `PlayCloneObject` ctor（`PlayCloneObject.cs:9-20`）只发 `RM_TURN`（`:19`）；
  **`SM_FEATURECHANGED(41)` 不在 ctor 里**——来自收件人侧 `RM_TURN` handler：
  `PlayObject.Message.cs:1525-1560`（先 `SM_TURN(10)`，再 `if (wIdent == RM_TURN)` 发
  `SM_FEATURECHANGED`，`PlayObject.Message.cs:1555-1559`；ident 41 见 `src/OpenMir2/Messages.cs:202`）。
- 这两帧由**真身 handler** 产生、经 gate 缓冲落 `Sent` ⇒ harness **已在记**，**不要新增记录机制**。
- 真缺口：克隆体对象字段（name/pos/dir/gender/hair）+ ActorId 归一化（`ActorEntity.cs:20`
  `ActorId = SystemShare.ActorMgr.GetNextIdentity()` 全局自增）。

### 1.4 MG-42（41）**成立（生产数据）**，但「阻塞出 golden」要改
- 生产链：`MagMakeAngelSlave`（`src/M2Server/Magic/MagicManager.cs:1343-1354`）→ `MakeSlave`
  （`BaseObject.cs:1180-1200`）→ `RegenMonsterByName`（`WorldServer.MonGen.cs:382`）→
  `GetMonRace`（`WorldServer.cs:167-174`，查不到**返回 -1**）→ `CreateMonster` 按 race `switch`
  （`WorldServer.MonGen.cs:498-…`，**无 default 臂**）⇒ race = -1 时 `cert` 恒 null ⇒ `MakeSlave` 返回 null
  ⇒ **41 在生产中恒失败**。
- 数据面：仓内冻结 `sql/mir2_data.sql` 的 `monsters` 表**没有名为 `精灵` 的行**（7 处「精灵」全是 `骷髅精灵*`）。
- 但 harness **不是被卡住**：`mir2-rs/tests/parity/csharp/MagicGolden/OfflineWorldEngine.cs` 的
  `RegenMonsterByName` 是替身，可 stub（17/30 同法）⇒ 可出「stub 验证」的 golden。
- ⇒ 唯一真阻塞是**生产数据行**；卡见 `docs/issues/openmir2-angel-summon-monster-missing.md`（本分支重立）。

## 2. MG-34（20）：观察面成立，**另有一条「缺省≠部署」**，且配置解析分在两个 Rust 解析器

- 观察面缺口（成立）：无 batch 入口（harness 对未知用例输出「未知用例」并 rc=2）；`MonsterSpec`
  （`mir2-rs/tests/parity/csharp/MagicGolden/Program.cs` 的 `MonsterSpec` 类）字段止于 `AntiMagic`，
  无 `NoTame/Animal/MonGenActive/CanReAlive/WalkSpeed/NextHitTime`；`Skeleton` 块按 `spec.MagicId != 17`
  门控 ⇒ 20 恒 null；`RM_MAKEHOLYSEIZEMODE` 无命名断言面（该 RM 是 `targetObject.SendMsg`
  （`src/M2Server/Magic/MagicManager.cs:712/812`）= 目标怪物自己的队列；monster 收件人已出现在
  golden `Receivers`（如 fire 的 `mon0..mon3`）⇒ 实现时先核其 `Rm` 是否已落）。
- **配置差异（新增，属「缺省≠部署」第 3 条）**：部署 `E:/MirServer/M2GameSvr/Server.conf`
  实测 `MagTammingLevel=40`、`MagTammingTargetLevel=1`、`MagTammingTargetHPRate=100`、`MagTammingCount=5`；
  ctor 缺省 **50 / 10 / 100 / 5**（`src/Modules/SystemModule/Conf/Model/GameSvrConf.cs:1774-1777`）
  ⇒ **两条新差异（50→40、10→1）**。连同 `MagTurnUndeadLevel 50→44`、`AmyOunsulPoint 10→15`，
  现在共 **4 条**「缺省≠部署」（并入 ServerConf 批）。
  这四项影响 `src/M2Server/Magic/MagicManager.cs:723`（橡皮筋门）与 `:725`（等级门）。
- **两个 Rust 解析器（对拍照必须都覆盖）**：
  - `MagTurnUndeadLevel` / `MabMabeHit*` / `AmyOunsulPoint` 在 `mir2-rs/bins/game-svr/src/config.rs`
    （`MAGTURNUNDEADLEVEL` 与 `MABMABE*`/`AMYOUNSULPOINT` 解析臂；缺省回落 50/100/10/21/20/10）。
  - **`MagTamming*` 四个键不在 `config.rs`**（该文件 0 命中），而在第二个解析器
    `mir2-rs/crates/data/src/pet_slave.rs` 的 `PetSlaveConfig`：字段 `:147/:149/:152/:154`、
    缺省 `:185-188` = **50 / 10 / 100 / 5**，且键名照抄 oracle 缺陷 `MagTammingTargetHPRate`。
  ⇒ 「Rust ↔ C# 配置对拍照」须同时覆盖这两个解析器，否则 `MagTamming*` 面**没有任何对拍**。

## 3. 待核（**不升级为结论**）

1. **部署库 `monsters` 是否有 `精灵` 行**：本审计**未直连查库**（避免访问凭据）；仓内冻结
   `sql/mir2_data.sql` 已证无。**要核定需**：一条只读 `SELECT … FROM monsters WHERE name='精灵'`
   （由有权限者执行）。
2. **MG-34 的 `RM_MAKEHOLYSEIZEMODE` 是否已落在 monster 收件人的 `Rm`**：静态只能看到
  harness 的 `RecordingMonster` 有 `DrainRaw`、golden 的 `Receivers` 里有怪物条目；**要核定需**：
   看一份实际 golden/台账实例（例如 fire 的 `mon0` 的 `Rm` 是否含目标怪物自己的 RM）。

## 4. 可复跑的最小核对

```bash
# 视野表：harness 零命中
grep -rn "VisibleActors\|UpdateVisibleGay" mir2-rs/tests/parity/csharp/MagicGolden/*.cs   # 期望无输出
# 21 的落点帧载体
grep -n "RM_SPACEMOVE_SHOW2\|RM_SPACEMOVE_SHOW," src/M2Server/Actor/BaseObject.cs          # 期望 :1137 / :1141
# 41 的 41 帧来源
grep -n "SM_FEATURECHANGED" src/M2Server/Player/PlayObject.Message.cs                     # 期望在 RM_TURN handler 内
# 精灵数据
grep -c "'精灵'," sql/mir2_data.sql                                                        # 期望 0
# MagTamming 的第二个解析器
grep -c "MAGTAMMING" mir2-rs/bins/game-svr/src/config.rs                                   # 期望 0
grep -n "mag_tamming_" mir2-rs/crates/data/src/pet_slave.rs                               # 期望 :147/:149/:152/:154 与缺省 :185-188
```
