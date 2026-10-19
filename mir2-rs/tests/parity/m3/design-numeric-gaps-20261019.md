# Lane B — 登录期/节拍数值缺口 最小修法简报

- 基线：`77aba528d`（master）。只读 lane：未建 worktree、未改仓库、未碰 `E:/MirServer`、未起进程。
- 三个缺口均引用既有审计，不是新发现：
  - 缺口 1/2：`openmir2-client-e2e-enter-gaps` 与评审员对本卡的补充；
  - 缺口 3：`mir2-rs/tests/parity/m3/audit-monster-attack-cadence-20261019.md` §「评审员的更正」C-8。
- 行号以基线 `77aba528d` 为准。判级：三处**全部 R 级**（见每节 `tier_why`），合并后不得 `closed`，
  须保持 `in-progress` 并写明待真机窗口项（AGENTS「判据分级」收口条）。

---

## 缺口 1：新手物品入包顺序（52 帧 Weight 少四件）

### Oracle 锚点
- `src/M2Server/Player/PlayObject.cs:1205-1244`：`if (IsNewHuman) { … ItemList.Add(userItem) }`
  固定四件：蜡烛 → 金创药(小量) → 木剑 → 布衣(男/女)；四次 `ItemList.Add` 在 :1210/:1219/:1228/:1240。
- `PlayObject.cs:1308-1310`：`RecalcLevelAbilitys(); RecalcAbilitys(); Abil.MaxExp = GetLevelExp(...)`。
  ⇒ 重算**读取的 ItemList 已含四件新手物品**，`WAbil.Weight` 含四件重量。
- `src/GameSrv/Word/WorldServer.cs:250-277`：`IsNewHuman` 判据 = `string.IsNullOrEmpty(HomeMap)` 且 `Abil.Level == 0`。

### Rust 现状与原因
- `bins/game-svr/src/main.rs:14732-14744`：新角色记录落 `HomeMap/坐标/Abil`；此时**尚未**加新手物品。
- `main.rs:14837-14863`：`enter::AbilityInputs.bag_items = d.bag_items.as_deref()`（记录原始背包）。
- `main.rs:14889`：`enter::apply(&mut inputs, &wx)` 出 50/41/213/52/752；
  `enter.rs:739` 用 `recalc_for_session(&a, …)` 算 52/752 ⇒ `WAbil.Weight` 只算记录背包。
- `main.rs:14896-14897`：同一份输入再算一次，写 `sg.state.wabil` / 世界实体。
- `main.rs:14938-14945`：会话 `item_list = bag_items_from(d.bag_items)`。
- `main.rs:15039-15056`：**到这一步才** `add_newbie_items(&mut sg.state.item_list, …)`。
- 因果：① 52/752 与首次 `wabil` 在加物品之前算完；② 重算读的是 `d.bag_items`，
  `add_newbie_items` 写的是 `sg.state.item_list`，**两个列表不同源**（评审员补充）⇒ 仅把调用提前仍不够。

### 最小修法（写集：`bins/game-svr/src/main.rs` 一处）
在 `main.rs:14732` 的 `if is_new_human { if let Some(r) = full.as_mut() { … } }` 内、
`r.data.abil = Some(new_human_ability());`（:14742）之后，把四件物品**直接并进记录背包**：

```rust
// oracle PlayObject.cs:1205-1244：四件先入 ItemList，:1308-1310 才重算
let mut items = Vec::new();
for w in add_newbie_items(
    &mut items,
    &enter_cfg.catalog,
    &enter_cfg.newbie,
    r.data.sex,
    &mut item_number,
) {
    glog!("WRN", "{w}（照 oracle CopyToUserItemFromName 失败分支，不入包）");
}
let bag = r.data.bag_items.get_or_insert_with(Vec::new);
for it in &items {
    bag.push(Some(server_item_from_bag(it))); // 已有 helper：main.rs:4708
}
```

然后：
- 删除 `main.rs:15039-15056` 的 late `add_newbie_items`（保留 :15030-15038 的日志；日志里的
  `Vec::len()` 若要仍表「非空件数」可改读 `bag_items_from(...)`，纯日志，不影响行为）。
- 不要改 `enter::AbilityInputs` 签名：记录一旦含四件，52/752、`sg.state.wabil`、会话 `item_list`
  （:14942）与存档（`build_save_record` 读 `sg.state.item_list`，:5014）自动同源。
- 顺序理由：`bag_items_from` 先按槽序展平非空记录项、再追加新手件；oracle 也是先 `GetHumData`
  装 `ItemList` 再 `Add` 四件 ⇒ 顺序一致。`MakeIndex` 仍由同一 `item_number` 计数器分配，且只分配一次。
- 不改冻结面。

### 构造级判据（>=3，含牙）
1. `newbie_items_enter_record_bag_before_recalc`（main.rs 测试模块）
   - 构造 `CharacterData{ bag_items: Some(vec![None; 46]), sex: 0, .. }`，走新 helper；
   - 断言 `bag_items_from(d.bag_items)` 的 index 序 = `[蜡烛, 金创药, 木剑, 布衣(男)]`、长度 4。
   - 牙：把插入挪回 :15039 之后 ⇒ 展平为空，必红。
2. `frame52_weight_counts_newbie_items`
   - 测试目录给四件赋 `weight = 1/1/5/3`；用改后的记录构造 `AbilityInputs` → `recalc_for_session`；
   - 断言 `out.wabil.weight == 10`，并断言 `record::encode_ability(&out.wabil)` 的 Weight 字段 == 10。
   - 牙：`bag_items` 仍传 `d.bag_items`（未并入四件）⇒ 0，必红。
3. `login_session_recalc_keeps_newbie_weight`
   - `SessionGround.state.item_list = bag_items_from(改后记录)`；调 `recalc_session_ability`；
   - 断言 `sg.state.wabil.weight == 10`。
   - 牙：只改 `AbilityInputs`/帧、不改记录（会话列表没有四件）⇒ 0，必红。
4. `newbie_items_added_exactly_once`
   - 计数器 100 → 104，列表长度 4；牙：新旧两处都调用 ⇒ 8 件 / 108，必红。

### tier 与理由
**R**。`tier_why`：改变 52 帧 `WAbil.Weight` 与属性重算输入，命中判据分级 ④ 明列的「负重上限 /
属性重算」；且是逐字节帧内容。必须真机登录新角色核对 52 尾 Weight（客户端负重条）。

### 部署内容可达性
- 新角色创建：`src/DBSrv/Services/Impl/UserService.cs:258-278` `NewChrData` → `new CharacterData()`；
  `PlayerDataInfo.cs:55-120` 的构造默认 `HomeMap = null`、`Abil = new Ability()`（Level=0）
  ⇒ `characters` 行 `HomeMap` 为 NULL ⇒ 登录时 `IsNullOrEmpty(HomeMap)` 真、`Level==0` 真
  ⇒ **每个新角色的首次登录**都进 `IsNewHuman` 分支。可达性 = 100%（首个号即可复现）。

### 残留（不在本最小修法内）
- oracle 加完四件后还有「非法物品剔除 / UseItems 合法检查 / 复制品去重」（`PlayObject.cs:1246-1306`），
  本仓未建模；新手四件按模板名生成，恒合法。既有记录项的合法性检查另卡。

---
## 缺口 2：`Abil.MaxExp` 登录不重算（52 帧 MaxExp 取记录值；实体 0/0）

### Oracle 锚点
- `PlayObject.cs:1310-1312`：登录末尾
  `Abil.MaxExp = GetLevelExp(Abil.Level); WAbil.Exp = Abil.Exp; WAbil.MaxExp = Abil.MaxExp;`
- `src/M2Server/Actor/BaseObject.cs:1409-1421` `GetLevelExp`：`NeedExps[nLevel]`（`<= MaxLevel(255)`），
  越界走 `NeedExps[Length]`（BUG-REPLICA）。
- `WorldServer.cs:260-275`：新角色 `abil.MaxExp = 100`（发生在 :1310 之前，随后被 1310 覆盖）。
- `PlayObject.cs:5244-5246`：升级 `HasLevelUp` 同样先 `Abil.MaxExp = GetLevelExp(Abil.Level)` 再重算。

### Rust 现状与原因
- 52/752 走 `enter.rs:739` → `recalc_for_session`（enter.rs:155-174）→ `ability::recalc_abilitys`。
  后者 `ability.rs:536` 取 `level_ability_base(inputs.abil)`，`ability.rs:476` 是 `..abil.clone()`，
  `ability.rs:638-655` 的 `wabil` 又 `..base` ⇒ `max_exp` **原样从角色记录复制**；
  `recalc_abilitys` 全函数没有 `MaxExp = GetLevelExp` 的等价物（它本来就不在 oracle 的
  `RecalcAbilitys` 里；oracle 是在 `OnLogin`/`HasLevelUp` 外层单独赋值）——本仓漏的就是这两处外层赋值。
- 存档侧已对：`build_save_record`（`main.rs:4911` 起，`max_exp` 在 `:4972-4976`）用
  `get_level_exp(&world.need_exps, MAX_LEVEL, base.level)`。所以**只有 52/752、会话 wabil、实体**错。
- 新角色：`new_human_ability()`（`main.rs:4859-4880`）给 `max_exp: 100`，而现网 `Exps.conf`
  `Level1=10`（夹具是现网只读副本，`tests/parity/tests/exp_parity.rs:4-5`）⇒ 52 帧 New human
  MaxExp 100 vs oracle 10。
- **同一根因的实体落点（必须同批）**：`apply_ability_to_entity`（`main.rs:5404`）只写
  dc/ac/hit/mp/hp/level 等，**不写 exp/max_exp**；`Entity` 缺省 `exp: 0, max_exp: 0`
  （`crates/world/src/entity.rs:1006`）。`crates/world/src/combat.rs:1409-1417` 的 `win_exp`：
  `killer.exp += dw; let leveled = killer.exp >= killer.max_exp;` ⇒ 登录后**第一只怪**必然
  `0 >= 0` 为真，产生一次伪升级（等级 +1、`exp -= 0`）。所有玩家可达。

### 最小修法（写集：`bins/game-svr/src/main.rs` 两处）
1. 在 `main.rs:14744`（`if is_new_human {…}` 之后、`data = full.as_ref()` 之前）对**所有角色**做
   oracle :1310 的记录侧赋值：
```rust
// oracle PlayObject.cs:1310：登录重新取得升级所需经验值（所有角色）
if let Some(r) = full.as_mut() {
    if let Some(a) = r.data.abil.as_mut() {
        a.max_exp = mir2_formula::exp::get_level_exp(
            &world.need_exps,
            mir2_data::models::MAX_LEVEL,
            i32::from(a.level),
        );
    }
}
```
   放在 `new_human_ability()` 之后 ⇒ 新角色的 100 被 `Exps[1]=10` 覆盖，与 oracle 同序/同值。
   因为后续 `AbilityInputs.abil`、`EquipInputs.abil = a.abil.clone()`（:14977）都读这一份，
   52/752、装备/卸下重算、存档自动一致；**不要**改 `ability.rs`（oracle 的 `RecalcAbilitys`
   不写 MaxExp）。
2. 在 `main.rs:14906-14914` 的 `apply_ability_to_entity(e, …)` 之后补：
```rust
e.exp = out.wabil.exp;         // oracle Abil.Exp / WAbil.Exp
e.max_exp = out.wabil.max_exp; // oracle Abil.MaxExp / WAbil.MaxExp
```
   （等价做法是并入 `apply_ability_to_entity`；数值相同，但登录点更贴合 oracle 的一次性赋值。）

### 构造级判据（>=3，含牙）
1. `login_max_exp_uses_exps_table`
   - `Ability{level:1, max_exp:100}` + `need_exps[1]=10` ⇒ `10`；
     `Ability{level:10, max_exp:1}` + `need_exps[10]=300` ⇒ `300`。
   - 牙：删掉记录侧赋值 ⇒ 100/1，必红。
2. `frame52_max_exp_equals_login_value`
   - 同一记录 → `recalc_for_session` → `out.wabil.max_exp == 10`；
     `record::encode_ability(&out.wabil)` 的 MaxExp 偏移解码 == 10（client-58 布局也同样断言）。
   - 牙：`recalc_abilitys` 只会 `..base` 复制记录值（`ability.rs:476/654`）⇒ 不改记录必为 100。
3. `login_seeds_entity_exp_and_max_exp`
   - `new_player_entity` + 登录输出；断言 `e.exp == 记录 Exp`、`e.max_exp == Exps[level]`；
   - 再调 `mir2_world::combat::win_exp(&mut e, 1, &need_exps, &CombatConfig::default(), None)`；
     断言 `leveled == false` 且 `e.level` 不变。
   - 牙：不补实体两行（max_exp=0）⇒ `1 >= 0` 为真、`leveled == true`，必红。
4. `equip_recalc_preserves_login_max_exp`
   - 会话 `equip.abil.max_exp` 取登录后值；调 `recalc_session_ability`；
     断言 `sg.state.wabil.max_exp` 不被记录旧值覆盖。牙：若只改 `AbilityInputs` 而没同步
     `sg.equip.abil`（或漏记录赋值）⇒ 52/752 又回旧值，必红。

### tier 与理由
**R**。`tier_why`：经验曲线（`GetLevelExp`）+ 战斗数值（经验/等级/升级）+ 52 帧字节；并且
实体 0/0 会让首杀伪升级，属判据分级 ④ 的「经验曲线、属性重算」。必须真机窗口：新号首杀前后
`SM_WINEXP(44)` / `SM_LEVELUP(45)` / 52 三帧与 C# 对照。

### 部署内容可达性
- `sql/mir2_db.sql` 的 `characters_ablity` 5995 行中，**3780 行**是 `Level=1, MaxExp=100`
  （另有 537 行 `Level=0`）；现网 `Exps.conf Level1=10` ⇒ 这些角色下次登录时 oracle 的 52 尾
  MaxExp=10，Rust 当前发 100。等级 >1 的行在转储里已与 Exps 一致（Level123 的
  `4000000000` 在 .NET `TryParse` 溢出归 0，双方同形），所以 52 侧差异集中在「1 级/新号」；
  实体侧 `exp/max_exp=0` 则是**每次登录、每个角色**可达。
- 新角色首次登录（`new_human_ability` 100）必然命中，见缺口 1 的可达性链。

### 残留
- `get_level_exp` 的越界分支 `NeedExps[len]` 是既有 BUG-REPLICA（`crates/formula/src/exp.rs`），不改。
- `Exps.conf Level123=4000000000` 在 .NET `TryParse` 溢出为 0，Rust `str_to_int` 同样落 0；两侧一致。

---
## 缺口 3：`WalkStep/WalkWait` 未接线

### Oracle 锚点
- `src/GameSrv/DB/MySqlDB.cs:214-215`：
  `monster.WalkStep = (ushort)HUtil32._MAX(1, dr.GetInt32("WalkStep"));`
  `monster.WalkWait = (ushort)dr.GetInt32("WalkWait");`
- `src/GameSrv/Word/WorldServer.MonGen.cs:880-909` `ApplyMonsterAbility`：
  `:906 WalkSpeed`、`:907 WalkStep`、`:908 WalkWait`、`:909 NextHitTime`；
  调用点在 `:735`（构造函数之后、`cert.Initialize()` `:750` 之前）。
- 消费：`src/M2Server/Monster/MonsterObject.cs:260-275`
  （`WalkWaitLocked` 到点释放；`WalkCount > WalkStep ⇒ count=0 + 锁 + WalkWaitTick=now`）。

### Rust 现状与原因
- 数据层已对：`crates/data/src/loaders.rs:293-294` 读 `WalkStep`（`MAX(1, …)`）与 `WaLkWait`
  （`get_i32 as u16`；NULL→-1→65535，与 C# `DataReaderExtension.GetInt32` NULL→-1→`(ushort)`
  同形，`src/OpenMir2/Extensions/DataReaderExtension.cs:33-45`）。
- 主机层漏：`bins/game-svr/src/mongen.rs:805-808` 只写了
  `e.ai.walk_speed` / `e.ai.next_hit_time`；文件内 :802-804 自登记「`WalkStep/WalkWait` 仍未接线」。
- `crates/world/src/entity.rs:372-373` 缺省 `0/0` 是 **oracle ctor 缺省**（`AnimalObject`
  两个自动属性默认 0，`AnimalObject.cs:31-32`），本身不该改；缺的是 `ApplyMonsterAbility` 的写入。
- 消费端已实现：`monai.rs:3310-3345`；目前永远读到 `walk_step=0, walk_wait=0`
  ⇒ `walk_count(1) > 0`，**每一步都置锁**（`WalkWait=0` 时下一 tick 立刻解锁）。

### 最小修法（写集：`bins/game-svr/src/mongen.rs` 两行）
在 `mongen.rs:805-808` 的 `if let Some(m) = info { … }` 内追加：
```rust
e.ai.walk_step = i32::from(m.walk_step); // MySqlDB.cs:214 → ApplyMonsterAbility :907
e.ai.walk_wait = i64::from(m.walk_wait); // MySqlDB.cs:215 → ApplyMonsterAbility :908
```
- 位置保持在 `apply_monster_class_constants(e, req.race, now)`（:809）之前，与 oracle
  `ApplyMonsterAbility`(:735) → `Initialize()`(:750) 的相对序一致；race 92 的
  `old_walk_time/old_hit_time` 只依赖 walk_speed/next_hit_time，不受影响。
- `info == None` 时 oracle `ApplyMonsterAbility` 直接 return（`MonGen.cs:882-885`）⇒ 保持 0/0，
  不要无条件写默认值。
- 不改 `crates/world`（字段与消费端都在）；不改 DDL/数据。

### 构造级判据（>=3，含牙）
1. `world_monster_sink_seeds_walk_step_wait`
   - `MonsterTable` 放 `MonsterInfo{name:"鸡", race:51, walk_speed:400, walk_step:3,
     walk_wait:1200, attack_speed:2000, ..}`；走 `WorldMonsterSink::spawn_monster`；
   - 断言 `e.ai.walk_step == 3`、`e.ai.walk_wait == 1200`、`e.ai.walk_speed == 400`。
   - 牙：删掉新增两行 ⇒ 0/0，必红。
2. `missing_monster_info_keeps_ctor_defaults`
   - 空 `MonsterTable` 生成同名怪（`info == None`）；
   - 断言 `walk_step == 0 && walk_wait == 0`（oracle 早退）。
   - 牙：把赋值挪到 `if let Some(m)` 之外并写成 `MAX(1, …)` 无条件落默认 ⇒ 必红。
3. `walk_wait_gates_after_walk_step_plus_one`
   - 生成后手工设 `walk_step=1, walk_wait=600, walk_speed=200`（并 `think_tick=now`、无目标）；
   - `clock=1000` 调 `mir2_world::monai::monster_run`：`walk_count==1`、`!walk_wait_locked`；
   - `clock=1400` 再调：`walk_count==0`、`walk_wait_locked==true`、`walk_wait_tick==1400`；
   - `clock=1800`（400 ≤ 600）：仍锁、不进入 walk 分支；
   - `clock=2100`（700 > 600）：解锁。
   - 牙：当前 `walk_step=0` ⇒ 第一次调用就 `1>0` 置锁且 `walk_wait_tick==1000` ⇒ 第一条断言红。

### tier 与理由
**R**。`tier_why`：直接改变怪物 AI 行走/攻击节拍（`WalkCount/WalkStep/WalkWaitLocked` 的时序），
命中判据分级 ②「时序（tick 顺序、出队序、settle/延迟）」。必须真机对拍同怪行走间隔。

### 部署内容可达性（只读 `sql/mir2_data.sql`；未连 `E:/MirServer`）
- `monsters` 705 行：`WalkWait != 0` 有 **25 行**：1200×16、600×8、500×1；
  其中 16 行的 Race ∈ 可刷集合：81/83/92/97/105/102/53，例如
  `圣域牛魔王(92, Step1, Wait1200, WALK_SPD400)`、`圣域半兽人(83,1,1200,800)`、
  `暗之祖玛教主(102,1,500,300)`、`圣域月魔(105,1,1200,400)`。
  这些怪每 2 步要停 600–1200ms，Rust 当前 0ms ⇒ 节拍差 3–6 个 200ms tick。
- `WalkStep > 1` 另有 31 行（2/3/4/5）；36 行 DB 值 0 被 loader 夹到 1（与 oracle 同）。
- 诚实边界：对 `WalkWait=0` 且 `WalkStep=1` 的绝大多数行，锁在同一/下一 tick 内被 walk_speed 门
  吞掉，肉眼节拍可能不变；但 25 行非零 WalkWait 是**可观测**差异，且 `WalkStep` 状态本身也需对齐。

### 残留
- 与缺口 1/2 无写集冲突（不同文件），可并行；测试放在 `mongen.rs` 的 `#[cfg(test)]`。

---

## 写集冲突与建议串行顺序

| 车道 | 缺口 | 写集 | 冲突 |
|---|---|---|---|
| B-1 | 1 新手物品 | `bins/game-svr/src/main.rs`（14732-14744、删除 15039-15056） | 与 B-2 同文件且同一 `full.as_mut()` 块 |
| B-2 | 2 MaxExp | `bins/game-svr/src/main.rs`（14744 后、14906-14914） | 与 B-1 同文件且紧邻 |
| B-3 | 3 WalkStep/Wait | `bins/game-svr/src/mongen.rs`（805-808 + 测试） | 无 |

建议：
1. **B-3 可立即并行**（写集不相交）。
2. **B-1 与 B-2 串行**：按 AGENTS §0b「同一文件一次只动一路」。推荐 `B-1 → B-2`（B-2 的两小块
   均落在 B-1 之后，rebase 简单）。若嫌两次评审开销，可把 1+2 合成**一张卡/一个 patch**
   （同一 owner、同一写集、同为 R 级、同一登录窗口对拍），但仍需独立 reviewer。
3. 两者都在 `main.rs` 测试模块加用例（新手 ~22735 区、经验/实体 ~26200 区），文本区域不同，
   但受同文件规则约束，不要并行。

## 合并/收口提示（R 级）
- 三处都要求真机窗口证据；`merge_result` 后卡片保持 `in-progress`，在 `work` 写明待窗口项，
  不得 `closed`。
- 证据四件套：命令 + 输出 + 路径 + hash；对拍项：
  ① 新号首登 52 尾 Weight/MaxExp；② 新号首杀 44/45/52/752 帧序；③ 非零 WalkWait 怪的同怪行走
  /攻击间隔（至少 圣域牛魔王 1200、暗之祖玛教主 500 各一组）。

## 收尾/残留物清单
- 本 lane 只产出 `E:/tmp/laneB-numeric-gaps.md`；无 worktree、无分支、无 `CARGO_TARGET_DIR`、
  未启动任何服务/客户端、未改 `src/**`/`sql/**`/`MirClient/`/`crates/protocol`/`crates/storage`。
- 既有审计原文保留：`audit-monster-attack-cadence-20261019.md:264` C-8。


---

## 评审员更正（2026-10-19，**以本节为准**）

- **缺口 1/2 的判据不咬生产接线**（Important）：`main.rs::sim_loop` 的内联代码在仓内测试里从不执行 ⇒ 按简报写的
  用例在删掉生产接线后仍会全绿。必须二选一：(a) 抽 `merge_newbie_items_into_record(...)` 等 helper 并在调用点单测；
  (b) 按仓内既有模式加源码文本断言（先例 `main.rs:16830-16877`、`33814-33831`），钉住「并入调用在
  `let data = full.as_ref()` 之前且恰一处」与「旧调用已删除」。
- 缺口 1 的日志语义：`main.rs:15030-15038` 打印的是记录 Vec 的 `Vec::len()`；修后新号会打印 50（46 空槽 + 4），
  不是「非空件数」——要么明确改读 `bag_items_from`，要么明确登记为已知日志偏差。
- 缺口 1 的测试构造：四件 weight 需为 1/1/5/3（现有 `newbie_catalog` 全 Default=0）；`bonus_tick_hit/speed`
  不能为 0（`ability.rs:764-775` 会除零 panic）。
- 缺口 1 的正向副作用登记：`main.rs:15161` 的 `npc_player_from` 读记录 `bag_items`，修后脚本侧首登即可见四件
  （与 oracle 的 ItemList 语义一致，但与旧行为不同）。
- 措辞：`full.data.bag_items` 应为 `r.data.bag_items`（`full: Option<PlayerRecord>`）。
- 缺口 2：`characters_ablity` 5995 行中 Level1+MaxExp100 = 3780、Level0 = 537；`Level123` 一处 DB 0 vs 文件
  4000000000，Rust 与 C# 都落 0（同形，不算偏差）。
- 三条修法方向与可达性数字**支持**；均为 R 级。
