//! 宝宝/召唤（slave）宿主侧接线 —— `crates/world::slave` 的 [`SlaveSpawner`] 生产实现
//! （`WorldEngine.RegenMonsterByName` 的等价物）+ `Server.conf` 的宝宝配置读侧。
//!
//! 交付卡：`openmir2-m3-pet-slave-summon`（M3⑨-实现 2）。
//!
//! ## 职责
//!
//! - [`MonsterTableSpawner`]：把 oracle `BaseObject.MakeSlave`（`BaseObject.cs:1190`）里的
//!   `SystemShare.WorldEngine.RegenMonsterByName(Envir.MapName, nX, nY, sMonName)`
//!   （`WorldServer.MonGen.cs:382-432` → `CreateMonster`）落到 **④-B 的同一条刷怪路径**：
//!   用 [`create_monster_oracle`] 在宿主的 `world.rng` 上按 oracle 次序抽数并生成实体。
//!   ——RNG 取数次序与 MonGen 刷怪完全同源，故 Claim B 账本不新增分叉。
//! - [`summon_from_spell`]：把 `crates/world::summon_slave`（选型/门/落尾/神兽回召）与
//!   上面的 spawner 拼成宿主一次调用。
//! - [`pet_slave_config_from_conf`]：`Server.conf` → [`PetSlaveConfig`]（`crates/data::pet_slave`
//!   的 `from_ini`；只读，不写回）。
//!
//! ## 第②片（⑨-2 接线）
//!
//! - [`SlaveCastInput`] / [`cast_slave_spell`]：`DoSpell` 的**召唤面**（17/30/41）效果体 +
//!   护身符门（`CheckAmulet`/`UseAmulet`）+ 共享尾（`boSpellFail`/`boSpellFire`/`boTrain`）。
//!   帧面（`RM_SPELL`/`RM_MAGICFIRE`/`RM_MAGIC_LVEXP`/`RM_DURACHANGE` 的 `SM_*` 编码）在
//!   [`crate::smframe`]，由宿主 `handle_spell_cmd` 发。
//!
//! ## 第③片（⑨-2 接线，20 诱惑）
//!
//! - [`TammingCastInput`] / [`cast_tamming_spell`]：`SKILL_TAMMING`（20）效果体 ——
//!   `IsProperTarget && !Animal` 门 → `mag_tamming`（取数账本在 `crates/world::magic`）→
//!   `TargetCret = null` → 受奴时 [`World::tame_adopt`] 落尾（Master/MasterRoyaltyTick/
//!   SlaveMakeLevel/MasterTick/WalkSpeed/NextHitTime/BreakHolySeizeMode/SlaveList.Add）→
//!   非受奴支 `HP = 0` → 共享尾。`Animal` 用构造期等价物
//!   [`mir2_world::combat::race_is_animal`]（race 51/52/53/82/84/85/90）。
//!
//! ## 未接（登记，别按常识补）
//!
//! - **`MakeSlave` 的 `RecalcAbilitys` 已落**（⑨-2 reviewer-5 `M-1` 收口）：
//!   `crates/world::slave::make_slave_adopt` 现按 oracle 先跑 `ApplySlaveLevelAbilitys`
//!   （`WAbil.DC`/`WAbil.MaxHP`，`BaseObject.Abil.cs:133-170`，`Race >= Animal` 门 + 三类支）
//!   再跑三个派生类的速度覆写（`WhiteSkeleton.cs:21-27` / `ElfMonster.cs:31-41` /
//!   `ElfWarriorMonster.cs:35-39,105-110`）；`RefNameColor` 色彩仍归帧面接线片。
//! - **46 分身**（`MagMakeClone`）：已接 —— [`CloneCastInput`] / [`cast_clone_spell`] 造玩家型
//!   克隆体（`crates/world::slave::spawn_clone`）并走共享尾；帧面（`RM_SPELL`/克隆 `RM_TURN`/
//!   `RM_MAGICFIRE`/`RM_MAGIC_LVEXP`）在宿主 `handle_spell_cmd` 的 46 支。克隆体的 tick/AI
//!   生命周期未建模（登记白名单 `T-51`）。
//! - **20 的 `RM_MAKEHOLYSEIZEMODE` 帧**（`MagicManager.cs:712/812`）与 **`OpenCrazyMode` 狂暴态/
//!   时长**（`:797/805`）：前者无编码器、后者本骨架无 `CrazyMode` 字段（对应抽数已在
//!   `mag_tamming` 消费，账本不变）⇒ 帧面/状态登记未接。
//! - **race 54 的 `Animal = true`**（`BaseObject.ReAliveEx` `:2601-2603`）：复活路径未建模（⑨-3 登记）
//!   ⇒ [`mir2_world::combat::race_is_animal`] 不计入。
//! - **护身符扣光分支的 `SM_DELITEM` 帧已接**：oracle `UseAmulet` 的 `else` 支
//!   （`MagicBase.cs:124-126`）`SendDelItems(UseItems[idx])` ⇒ `SM_DELITEM(202)` + `EncodeBuffer(clientItem)`
//!   （`PlayObject.Base.cs:1648-1665`）；帧体见 [`deleted_amulet_body`]，宿主在 `CM_SPELL` 的
//!   护身符结算处发（清槽**前**取件、`Dura` 恒 0）。

use std::path::Path;

use mir2_data::exps::IniFile;
use mir2_data::pet_slave::PetSlaveConfig;
use mir2_data::MagicInfo;
use mir2_envir::MapRegistry;
use mir2_shared::rng::RandomNumber;
use mir2_world::magic::{self, AmuletSlot, AmuletUse, SkillTrainState};
use mir2_world::slave::{
    spawn_clone, CloneSpawn, SKILL_ANGEL, SKILL_SINSU, SKILL_SKELLETON, SKILL_TAMMING,
};
use mir2_world::{SlaveSpawner, SummonOutcome, World};

use crate::ability::ItemCatalog;
use crate::bag::{item_display_name, BagItem};
use crate::downlink::DisplayTable;
use crate::mongen::{create_monster_oracle, MonsterTable, WorldMonsterSink};

/// `MagicBase.UseAmulet`（`MagicBase.cs:109-127`）`else` 支里 `SendDelItems(UseItems[idx])`
/// 的**立即帧** body（`PlayObject.Base.cs:1648-1665`）。
///
/// oracle 语句序（`:121-126`）：
/// ```text
/// playObject.UseItems[idx].Dura = 0;                 // ① 先置 0
/// playObject.SendDelItems(playObject.UseItems[idx]);// ② 帧内 Dura 即 0
/// playObject.UseItems[idx].Index = 0;                // ③ 再清槽
/// ```
/// ⇒ 帧里的 `Dura` **恒 0**（不是扣光前的值），`DuraMax`/`MakeIndex`/`Desc` 与 `Index` 取原件。
/// `StdMode == 50` 的名称后缀用当下的 `Dura`（= 0）⇒ `" #0"`。
///
/// 与 [`crate::trade::send_add_item_body`]（`SM_ADDITEM`）的差别只在两处：ident `202` vs `200`，
/// 以及**不做** `M2Share.StdModeMap` 的 `Desc[8]`⇒`Shape`(0/130) 改写（`SendDelItems` 不动 `Shape`）。
/// `GetUpgradeStdItem` 的 desc 升级差量未建模（同 `T-24`）：`desc` 全 0 的物品逐字节等价。
#[must_use]
pub fn deleted_amulet_body(
    actor_id: i32,
    item: &BagItem,
    catalog: &dyn ItemCatalog,
) -> Option<Vec<u8>> {
    let tpl = catalog.get(item.index)?;
    let mut name = item_display_name(catalog, item);
    if tpl.std_mode == 50 {
        // oracle `clientItem.Item.Name = clientItem.Item.Name + " #" + userItem.Dura;`（Dura 已置 0）。
        name = format!("{name} #0");
    }
    let ci = crate::itemwire::ClientItem {
        tpl,
        name,
        make_index: item.make_index,
        dura: 0,
        dura_max: item.dura_max,
        desc: item.desc,
    };
    let tail = mir2_protocol::edcode::encode(&crate::itemwire::encode_client_item(&ci));
    Some(crate::smframe::sm_delitem(actor_id, &tail))
}

/// `[Server]`/`[Setup]` 的宝宝配置读侧（等价 `GameSvrConf` 的对应读取点）。
///
/// 照 [`crate::mongen::mon_gen_config_from_conf`] 的口径：读文件失败返回人话错误（不静默用缺省）；
/// 现网 `Server.conf` 的 UTF-8 BOM 照事实剥离（首 3 字节 `ef bb bf`）。
///
/// # Errors
/// 读文件失败时返回错误字符串。
pub fn pet_slave_config_from_conf(path: &Path) -> Result<PetSlaveConfig, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("读 {} 失败：{e}", path.display()))?;
    let bytes = bytes
        .strip_prefix(&[0xEF, 0xBB, 0xBF][..])
        .unwrap_or(&bytes);
    let ini = IniFile::from_gbk_bytes(bytes);
    Ok(PetSlaveConfig::from_ini(&ini))
}

/// [`SlaveSpawner`] 的生产实现：`RegenMonsterByName` → `CreateMonster` 的宿主等价物。
///
/// `displays` 是 `bins/game-svr` 的展示层（与刷怪同源：`WorldMonsterSink::spawn_monster`
/// 在里面登记 `Feature/Light/ShowName`）。
pub struct MonsterTableSpawner<'a> {
    pub registry: &'a MapRegistry,
    pub table: &'a MonsterTable,
    pub displays: &'a mut DisplayTable,
}

impl SlaveSpawner for MonsterTableSpawner<'_> {
    fn spawn(
        &mut self,
        world: &mut World,
        rng: &mut mir2_shared::rng::RandomNumber,
        map_id: usize,
        name: &str,
        x: i16,
        y: i16,
    ) -> Option<i32> {
        // `RegenMonsterByName(sMap, …)`：`sMap = Envir.MapName`（主人所在图）。
        let map_name = self.registry.name_of(map_id)?.to_string();
        // `RegenMonsterByName` 首句 `int nRace = GetMonRace(sMonName);`（未命中 = -1 ⇒ `CreateMonster`
        // 的 switch 未命中 ⇒ null，且**不抽任何数**，与 `create_monster_oracle` 一致）。
        let race = self.table.race_of(name);
        let mut sink = WorldMonsterSink {
            world,
            registry: self.registry,
            table: self.table,
            displays: &mut *self.displays,
        };
        create_monster_oracle(rng, &mut sink, self.table, &map_name, x, y, race, name)
    }
}

/// 宿主一次调用：`crates/world::summon_slave` + [`MonsterTableSpawner`]。
///
/// `caster_dir` = 主人的 `BaseObject.Dir`（玩家在宿主侧的权威 `Dir` 在 `downlink::DisplayTable`
/// —— 客户端指令维护；`Entity.ai.dir` 只对怪物由 AI 维护 ⇒ 调用方显式传入）。
///
/// **调用方职责（oracle 前置，未接）**：`DoSpell` 的护身符门（`CheckAmulet`/`UseAmulet`）、
/// `MagicAttackRage` 距离门与版本门、`RM_SPELL` 帧、`boTrain` 尾——见模块头「未接」。
#[allow(clippy::too_many_arguments)]
pub fn summon_from_spell(
    world: &mut World,
    caster_id: i32,
    caster_dir: u8,
    magic_id: u16,
    magic_level: u8,
    cfg: &PetSlaveConfig,
    registry: &MapRegistry,
    table: &MonsterTable,
    displays: &mut DisplayTable,
) -> SummonOutcome {
    let mut spawner = MonsterTableSpawner {
        registry,
        table,
        displays,
    };
    mir2_world::summon_slave(
        world,
        caster_id,
        caster_dir,
        magic_id,
        magic_level,
        cfg,
        &mut spawner,
    )
}

/// **⑨-2 第②片**：`DoSpell` 召唤面（17/30/41）效果体的宿主输入。
///
/// `armringl`/`bujuk` 是**本会话**的 `UseItems[ArmRingl]`/`UseItems[Bujuk]` 已查表形态
/// （`ItemLocation` 值 + `StdItem.StdMode/Shape` + `UserItem.Dura`）。调用方负责把它们
/// 落到 [`AmuletSlot`]（用 `crate::ability::ItemCatalog`）。
#[derive(Clone, Copy, Debug)]
pub struct SlaveCastInput<'a> {
    pub magic_id: u16,
    pub magic_level: u8,
    pub caster_id: i32,
    pub caster_dir: u8,
    /// `PlayObject.Abil.Level`（熟练度尾门 `TrainLevel[Level] <= Abil.Level`）。
    pub caster_abil_level: u8,
    pub armringl: Option<AmuletSlot>,
    pub bujuk: Option<AmuletSlot>,
    /// `PlayObject.FastTrain`（oracle 由「快速修炼项链」置位，`PlayObject.cs:2168`；
    /// 本仓未装载该面 ⇒ 宿主恒传 `false`，登记）。
    pub fast_train: bool,
    pub cfg: &'a PetSlaveConfig,
    /// `UserMagic.Magic` —— 调用方已用 `MagicTable` 查到本魔号（无命中 ⇒ 不调本函数）。
    pub magic: &'a MagicInfo,
}

/// [`cast_slave_spell`] 的结果（帧面参数由宿主据此构造）。
#[derive(Clone, Copy, Debug)]
pub struct SlaveCastOutcome {
    /// oracle `DoSpell` 的返回值（`boSpellFail` ⇒ `false`；`true` 时宿主仍发 `+GD`）。
    pub accepted: bool,
    /// `boSpellFire`（尾部是否发 `RM_MAGICFIRE`；召唤面恒 `true`）。
    pub magic_fire: bool,
    /// 本次消耗的护身符：`(ItemLocation 值, 消耗结果)`；`None` = 未消耗（门未走 / 不需护身符）。
    pub amulet: Option<(u8, AmuletUse)>,
    /// 召唤编排结果（`None` = 护身符门不过）。
    pub effect: Option<SummonOutcome>,
    /// 熟练度尾结算（[`SkillTrainState`] 已就地更新；`trained`/`levelup_msg` 决定是否发
    /// `RM_MAGIC_LVEXP`）。
    pub train: magic::SkillProficiencyOutcome,
}

/// **⑨-2 第②片**：召唤技能（17/30/41）的 `DoSpell` 效果体 + 共享尾。
///
/// 逐行锚 oracle：
/// - 17：`case SKILL_SKELLETON`（`MagicManager.cs:319-393` 分组）：`boSpellFail=true` →
///   `CheckAmulet(1,1)` → `UseAmulet(1,1)` → `MagMakeSlave` → `boTrain = 结果` → `boSpellFail=false`；
/// - 30：`case SKILL_SINSU`（`:450-461`）：`CheckAmulet(5,1)` → `UseAmulet(5,1)` → `MagMakeSinSuSlave`；
/// - 41：`case SKILL_ANGEL`（`:462-473`）：`CheckAmulet(2,1)` → `UseAmulet(2,1)` → `MagMakeAngelSlave`；
/// - 尾：`if (boSpellFail) return false;` → `if (boSpellFire) SendRefMsg(RM_MAGICFIRE,…)` →
///   `if (Level < 3 && boTrain) { … TrainSkill(Random(3)+1) … }`（`:644-669`，实现见
///   `crates/world::magic::do_spell_tail`）。
///
/// `boTrain` 只在 `MakeSlave != null`（[`SummonOutcome::Spawned`]）时为真：神兽的回召分支
/// （`Recalled`）oracle 里 `result` 仍为 `false`（`MagicManager.cs:1300-1306`）。
///
/// 取数次序：召唤的造怪取数在 [`summon_from_spell`]（宿主 `create_monster_oracle`），随后
/// 尾部的 `Random(3)+1` 走**同一条** `world.rng`（中间不插入其它取数）。
#[allow(clippy::too_many_arguments)]
pub fn cast_slave_spell(
    world: &mut World,
    input: &SlaveCastInput<'_>,
    train: &mut SkillTrainState,
    registry: &MapRegistry,
    table: &MonsterTable,
    displays: &mut DisplayTable,
) -> SlaveCastOutcome {
    let mut bo_spell_fail = false;
    let mut bo_train = false;
    let mut amulet = None;
    let mut effect = None;
    // 17/30/41 有护身符门；20/46 不走本支（调用方不应对它们调本函数）。
    if matches!(input.magic_id, SKILL_SKELLETON | SKILL_SINSU | SKILL_ANGEL) {
        bo_spell_fail = true;
        // `CheckAmulet(playObject, n, t, ref nAmuletIdx)`：三支需求由 `amulet_requirement` 给。
        if let Some((count, ty)) = magic::amulet_requirement(i32::from(input.magic_id)) {
            if let Some(slot) = magic::check_amulet(input.armringl, input.bujuk, count, ty) {
                // `UseAmulet` 的纯结果；宿主据此改 `UseItems`（扣耐久 / 扣光清槽）。
                let dura = match slot {
                    magic::ITEM_LOCATION_ARM_RINGL => input.armringl.map(|s| s.dura),
                    magic::ITEM_LOCATION_BUJUK => input.bujuk.map(|s| s.dura),
                    _ => None,
                }
                .unwrap_or(0);
                amulet = Some((slot, magic::use_amulet(dura, count)));
                let outcome = summon_from_spell(
                    world,
                    input.caster_id,
                    input.caster_dir,
                    input.magic_id,
                    input.magic_level,
                    input.cfg,
                    registry,
                    table,
                    displays,
                );
                // oracle `if (MakeSlave(...) != null) boTrain = true;`（回召/拒绝 ⇒ 不置）。
                if matches!(outcome, SummonOutcome::Spawned(_)) {
                    bo_train = true;
                }
                effect = Some(outcome);
                bo_spell_fail = false;
            }
        }
    }
    // 共享尾（`MagicManager.cs:644-669`）：`boSpellFire` 恒 true（召唤面没有置 false 的分支）。
    let mut rng = std::mem::replace(&mut world.rng, RandomNumber::with_seed(0));
    let tail = magic::do_spell_tail(
        bo_spell_fail,
        true,
        bo_train,
        input.fast_train,
        input.caster_abil_level,
        &mut rng,
        train,
    );
    world.rng = rng;
    let _ = SKILL_TAMMING; // 20 见 [`cast_tamming_spell`]（同片第③面）。
    SlaveCastOutcome {
        accepted: tail.returned,
        magic_fire: tail.magic_fire,
        amulet,
        effect,
        train: tail.train,
    }
}

/// **⑨-2 第③片（20 诱惑）**：`SKILL_TAMMING` 效果体的宿主输入。
///
/// `target_id` = `PlayObject.Attack.cs:1271-1277` 的 `CretInNearXy` 命中结果
/// （`None` ⇒ oracle `DoSpell` 收到 `null` ⇒ `IsProperTarget(null)` 假）。
#[derive(Clone, Copy, Debug)]
pub struct TammingCastInput<'a> {
    /// `UserMagic.Level`（`MagTamming` 的 `magicLevel`）。
    pub magic_level: u8,
    pub caster_id: i32,
    /// `PlayObject.Abil.Level`（熟练度尾门）。
    pub caster_abil_level: u8,
    pub target_id: Option<i32>,
    /// `PlayObject.FastTrain`（本仓未装载该面 ⇒ 宿主恒传 `false`，登记）。
    pub fast_train: bool,
    pub cfg: &'a PetSlaveConfig,
    /// `UserMagic.Magic`（调用方用 `MagicTable` 查到本魔号；无命中 ⇒ 不调本函数）。
    pub magic: &'a MagicInfo,
}

/// [`cast_tamming_spell`] 的结果（帧面参数由宿主据此构造）。
#[derive(Clone, Copy, Debug)]
pub struct TammingCastOutcome {
    /// oracle `DoSpell` 的返回值（20 没有 `boSpellFail` 支 ⇒ 恒 `true`）。
    pub accepted: bool,
    /// `boSpellFire`（20 恒 `true` ⇒ 宿主发 `RM_MAGICFIRE`）。
    pub magic_fire: bool,
    /// 本次成功受奴（oracle `targetObject.Master = playObject` + 落尾）。
    pub tamed: bool,
    /// 本次**非受奴支**改写到的目标 HP（`None` = 未改；受奴支的 `/10` 由
    /// [`World::tame_adopt`] 落，不在本字段重复）。
    pub target_hp: Option<u16>,
    /// `OpenCrazyMode(Random(20)+10)` 支（`MagicManager.cs:795/802`）——抽数已在
    /// [`magic::mag_tamming`] 内消费，**狂暴态/时长未建模**（本骨架无 `CrazyMode` 字段，登记）。
    pub crazy_mode: bool,
    /// 熟练度尾结算（[`SkillTrainState`] 已就地更新）。
    pub train: magic::SkillProficiencyOutcome,
}

/// `MagTamming` 的入参装配（门不过 ⇒ `None`，oracle 不进入 `MagTamming`，也就**不抽任何数**）。
///
/// 两道门逐字对应 `MagicManager.cs:395`：`playObject.IsProperTarget(targetObject)`
/// （`PlayObject.cs:1929`，含 M3⑦-4 的 `HAM_GROUP` 组剔除）与 `!targetObject.Animal`
/// （[`mir2_world::combat::race_is_animal`] 的构造期等价物）。
fn tamming_input(world: &World, input: &TammingCastInput<'_>) -> Option<magic::TammingInput> {
    let target_id = input.target_id?;
    let caster = world.entities.get(input.caster_id)?;
    let target = world.entities.get(target_id)?;
    // `IsGroupMember` 读的是**团长**（`caster.GroupOwner`）的成员表（`PlayObject.cs:5158-5175`）；
    // 非团长施法者自身的 `group_members` 恒空（`party.rs` 的 `JoinGroup` 只写 `group_owner`）。
    if !mir2_world::combat::is_proper_target_grouped(
        caster,
        target,
        &world.attacker_group_members(caster),
    ) {
        return None;
    }
    if mir2_world::combat::race_is_animal(target.race) {
        return None;
    }
    Some(magic::TammingInput {
        magic_level: input.magic_level,
        caster_level: i32::from(caster.level),
        target_race: target.race,
        target_level: i32::from(target.level),
        target_abil_max_hp: i32::from(target.max_hp),
        target_life_attrib: target.life_attrib,
        // `NoTame = monster.boTame`（`WorldServer.MonGen.cs:911`），而 `MonsterInfo.boTame`
        // 全仓无赋值点（DB `monsters` 无该列）⇒ oracle 恒 `false`；照搬为常量（登记）。
        target_no_tame: false,
        target_master_is_caster: target.master == Some(input.caster_id),
        target_master_present: target.master.is_some(),
        target_hp: i32::from(target.hp),
        walk_speed: i32::try_from(target.ai.walk_speed).unwrap_or(i32::MAX),
        next_hit_time: i32::try_from(target.ai.next_hit_time).unwrap_or(i32::MAX),
        caster_slave_count: i32::try_from(caster.slave_list.len()).unwrap_or(i32::MAX),
        cfg_target_level: input.cfg.mag_tamming_target_level,
        cfg_tamming_level: input.cfg.mag_tamming_level,
        cfg_tamming_count: i32::from(input.cfg.mag_tamming_count),
        cfg_hp_rate: input.cfg.mag_tamming_hp_rate,
    })
}

/// **⑨-2 第③片（20 诱惑）**：`SKILL_TAMMING` 的 `DoSpell` 效果体 + 共享尾。
///
/// 逐行锚 oracle：
/// - `case SKILL_TAMMING`（`MagicManager.cs:394-397`）：`IsProperTarget && !Animal` ⇒
///   `MagTamming(playObject, (MonsterObject)target, nTargetX, nTargetY, userMagic.Level)`，
///   返回 `true` ⇒ `boTrain = true`。
/// - `MagTamming`（`:704-825`）：取数账本由 [`magic::mag_tamming`] 逐字保留（Claim B 输入）；
///   本函数只做**状态落尾**：`TargetCret = null`（`:708`）、受奴（`Master`/`MasterRoyaltyTick`/
///   `SlaveMakeLevel`/`MasterTick`/`WalkSpeed`/`NextHitTime`/`BreakHolySeizeMode`/`SlaveList.Add`）
///   交给 [`World::tame_adopt`]（⑨-3），非受奴支的 `WAbil.HP = 0` 就地写。
/// - 尾（`:644-669`）：20 没有置 `boSpellFail` 的分支 ⇒ `boSpellFail=false`、`boSpellFire=true`。
///
/// 取数次序：`MagTamming` 的抽数（`Random(4-Level)` / `Random(2)` / … / `Random(caster.Lv*2)`）
/// 先走 `world.rng`，随后尾部的 `Random(3)+1` 走**同一条**流（中间不插入其它取数）。
///
/// # 未建模（登记，别按常识补）
/// - `SendMsg(RM_MAKEHOLYSEIZEMODE, …)`（`:712/797` 两支：调用前 `Master == 施法者`；
///   `Random(2) != 0`）——该帧无编码器，登记；状态面无副作用。
/// - `BreakCrazyMode()` / `OpenCrazyMode()` 的狂暴态与时长（`:754/796/803`）：本骨架无 `CrazyMode`
///   字段；`mag_tamming` 已把对应抽数消费掉（账本不变），状态登记不实现。
/// - `CanReAlive` / `MonGen.ActiveCount`（`:761-770`）：同 ⑨-3 登记（`MonGen` 未建模）。
pub fn cast_tamming_spell(
    world: &mut World,
    input: &TammingCastInput<'_>,
    train: &mut SkillTrainState,
) -> TammingCastOutcome {
    let mut bo_train = false;
    let mut tamed = false;
    let mut target_hp = None;
    let mut crazy_mode = false;
    // oracle `if (IsProperTarget(targetObject) && !targetObject.Animal) { if (MagTamming(...)) boTrain = true; }`
    // —— 门不过 ⇒ 连 `MagTamming` 都不进（`boTrain=false`、不抽数），但尾仍跑（`boSpellFire=true`）。
    if let Some(tin) = tamming_input(world, input) {
        let target_id = input.target_id.expect("门过 ⇒ target_id 必为 Some");
        let mut rng = std::mem::replace(&mut world.rng, RandomNumber::with_seed(0));
        let out = magic::mag_tamming(&mut rng, &tin);
        world.rng = rng;
        // `targetObject.TargetCret = null;`（`:708`，进入外层支即无条件清）。
        if out.target_creature_cleared {
            if let Some(t) = world.entities.get_mut(target_id) {
                t.ai.target_creature = None;
            }
        }
        // 受奴支：调用前 `Master != 施法者` 而结果 `Master == 施法者` ⇒ 本次受奴。
        // （调用前 `Master == 施法者` 的早退支也把该输出置真，故必须与输入合判。）
        tamed = out.target_master_is_caster && !tin.target_master_is_caster;
        if tamed {
            // 落尾（⑨-3）：`Master`/`MasterRoyaltyTick`/`SlaveMakeLevel`/`MasterTick`/
            // `WalkSpeed`/`NextHitTime`/`BreakHolySeizeMode`/`SlaveList.Add`（`MagicManager.cs:753-770`）。
            world.tame_adopt(
                input.caster_id,
                target_id,
                input.magic_level,
                out.royalty_draw,
            );
        } else if out.target_hp != tin.target_hp {
            // 非受奴支的 `targetObject.WAbil.HP = 0`（`:783/790`）。
            let hp = u16::try_from(out.target_hp.max(0)).unwrap_or(0);
            if let Some(t) = world.entities.get_mut(target_id) {
                t.hp = hp;
            }
            target_hp = Some(hp);
        }
        crazy_mode = out.crazy_mode;
        bo_train = out.result;
    }
    // 共享尾（`MagicManager.cs:644-669`）。
    let mut rng = std::mem::replace(&mut world.rng, RandomNumber::with_seed(0));
    let tail = magic::do_spell_tail(
        false,
        true,
        bo_train,
        input.fast_train,
        input.caster_abil_level,
        &mut rng,
        train,
    );
    world.rng = rng;
    TammingCastOutcome {
        accepted: tail.returned,
        magic_fire: tail.magic_fire,
        tamed,
        target_hp,
        crazy_mode,
        train: tail.train,
    }
}

/// **⑨-2 残余（46 分身）**：`DoSpell` 分身面（46）效果体的宿主输入。
#[derive(Clone, Copy, Debug)]
pub struct CloneCastInput {
    pub caster_id: i32,
    /// 主人 `PlayObject.Dir`（展示层字段；宿主从 `downlink::DisplayTable` 取，与召唤/诱惑面同口径）。
    pub caster_dir: u8,
    /// `PlayObject.Abil.Level`（熟练度尾门 `TrainLevel[Level] <= Abil.Level`）。
    pub caster_abil_level: u8,
    /// `PlayObject.FastTrain`（oracle 由「快速修炼项链」置位，`PlayObject.cs:2168`；
    /// 本仓未装载该面 ⇒ 宿主恒传 `false`，登记）。
    pub fast_train: bool,
}

/// [`cast_clone_spell`] 的结果（帧面参数由宿主据此构造）。
#[derive(Clone, Copy, Debug)]
pub struct CloneCastOutcome {
    /// oracle `DoSpell` 的返回值（46 没有 `boSpellFail` 支 ⇒ 恒 `true`）。
    pub accepted: bool,
    /// `boSpellFire`（46 恒 `true` ⇒ 宿主发 `RM_MAGICFIRE`）。
    pub magic_fire: bool,
    /// 本次造出的克隆体（主人不在世界/地图 id 越界 ⇒ `None`，宿主已先行校验主人在地）。
    pub clone: Option<CloneSpawn>,
    /// 熟练度尾结算（[`SkillTrainState`] 已就地更新）。
    pub train: magic::SkillProficiencyOutcome,
}

/// **⑨-2 残余（46 分身）**：`SKILL_46` 的 `DoSpell` 效果体 + 共享尾。
///
/// 逐行锚 oracle：
/// - `case SKILL_46`（`MagicManager.cs:589-592`）：`if (MagMakeClone(playObject, userMagic)) { boTrain = true; }`
///   —— `MagMakeClone`（`:1337-1341`）恒返回 `true`；实体构造见
///   [`mir2_world::slave::spawn_clone`]（`PlayCloneObject.cs:5-23`）。
/// - 尾（`:644-669`）：46 没有置 `boSpellFail` 的分支 ⇒ `boSpellFail=false`、`boSpellFire=true`；
///   `boTrain=true` ⇒ `userMagic.Level < 3` 时走 `TrainSkill(Random(3)+1)` + `CheckMagicLevelUp`。
///
/// 取数次序：`MagMakeClone` 不抽数；尾部的 `Random(3)+1` 走**主人这条** `world.rng`
/// （与召唤/诱惑面同一条流，中间不插入其它取数）。**不许为「好测」改次序。**
pub fn cast_clone_spell(
    world: &mut World,
    input: &CloneCastInput,
    train: &mut SkillTrainState,
) -> CloneCastOutcome {
    // `new PlayCloneObject(playObject)`：实体 + `Envir.AddMapObject`（RNG 中立）。
    let clone = spawn_clone(world, input.caster_id, input.caster_dir);
    // oracle `if (MagMakeClone(...)) { boTrain = true; }` —— `MagMakeClone` **恒** `true`
    // （不看 `AddMapObject` 的返回）。`clone` 为 `None` 仅在主人不在世界/地图 id 越界时可达，
    // 而宿主 `handle_spell_cmd` 已先校验主人在地 ⇒ 生产路径恒 `Some`。
    let mut rng = std::mem::replace(&mut world.rng, RandomNumber::with_seed(0));
    let tail = magic::do_spell_tail(
        false,
        true,
        true,
        input.fast_train,
        input.caster_abil_level,
        &mut rng,
        train,
    );
    world.rng = rng;
    CloneCastOutcome {
        accepted: tail.returned,
        magic_fire: tail.magic_fire,
        clone,
        train: tail.train,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mir2_envir::MapLoader;

    /// 现网 `Server.conf` 的 BOM 口径（照 `config.rs` 的既有测试）：缺键回 `GameSvrConf` 初值。
    #[test]
    fn pet_slave_config_reads_conf_and_ignores_bom() {
        let dir = std::env::temp_dir().join(format!(
            "m3hp-pet-conf-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("Server.conf");
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend_from_slice(b"[Names]\nSkeleton=SkellyX\n[Setup]\nSkeletonCount=4\n");
        std::fs::write(&p, bytes).unwrap();
        let cfg = pet_slave_config_from_conf(&p).expect("应能读到配置");
        assert_eq!(cfg.skeleton, "SkellyX");
        assert_eq!(cfg.skeleton_count, 4);
        // 未写键 ⇒ GameSvrConf 初值。
        assert_eq!(cfg.dragon, PetSlaveConfig::default().dragon);
        std::fs::remove_file(&p).ok();
        std::fs::remove_dir(&dir).ok();
    }

    /// 读不到文件 ⇒ 明确报错（不静默用缺省）。
    #[test]
    fn pet_slave_config_missing_file_errors() {
        let p = std::env::temp_dir().join("m3hp-pet-conf-does-not-exist.conf");
        assert!(pet_slave_config_from_conf(&p).is_err());
    }

    /// ⑨-2 第②片：空注册表 + 空怪表就够跑效果体（不触盘）。
    fn cast_world() -> (World, i32) {
        let mut w = World::new(32, 32);
        let caster = {
            let mut e = mir2_world::Entity::new(0, "主人".to_string(), 10, 10);
            e.race = mir2_world::ACTOR_RACE_PLAY;
            e.level = 40;
            w.add_entity(e)
        };
        (w, caster)
    }

    fn cast_input<'a>(
        caster: i32,
        armringl: Option<AmuletSlot>,
        cfg: &'a PetSlaveConfig,
        magic: &'a MagicInfo,
    ) -> SlaveCastInput<'a> {
        SlaveCastInput {
            magic_id: SKILL_SKELLETON,
            magic_level: 1,
            caster_id: caster,
            caster_dir: 0,
            caster_abil_level: 40,
            armringl,
            bujuk: None,
            fast_train: false,
            cfg,
            magic,
        }
    }

    fn train_state() -> SkillTrainState {
        SkillTrainState {
            magic_id: SKILL_SKELLETON,
            level: 1,
            tran_point: 0,
            train_lv: 3,
            train_level: [0; 4],
            max_train: [100; 4],
        }
    }

    /// **⑨-2 第②片判据 ①**：护身符门不过（两槽空）⇒ `boSpellFail = true` ⇒
    /// `DoSpell` 返回 false、**不发** `RM_MAGICFIRE`、不消耗、不召唤（`MagicManager.cs:319-393`）。
    #[test]
    fn cast_without_amulet_is_spell_fail() {
        let (mut w, caster) = cast_world();
        let cfg = PetSlaveConfig::default();
        let magic = MagicInfo::default();
        let mut train = train_state();
        let reg = MapRegistry::from_entries(MapLoader::new(".", "map", "envir"), Vec::new());
        let table = MonsterTable::new(Vec::new());
        let mut displays = DisplayTable::new();
        let input = cast_input(caster, None, &cfg, &magic);
        let out = cast_slave_spell(&mut w, &input, &mut train, &reg, &table, &mut displays);
        assert!(!out.accepted, "无护身符 ⇒ boSpellFail ⇒ DoSpell false");
        assert!(!out.magic_fire, "boSpellFail 短路 ⇒ 不发 RM_MAGICFIRE");
        assert!(out.amulet.is_none(), "未消耗");
        assert!(out.effect.is_none(), "未走到召唤");
        assert!(!out.train.trained, "未练级");
    }

    /// **⑨-2 第②片判据 ②**：`ArmRingl` 有 450 持久（type1 Shape5）⇒ 过 `CheckAmulet(1,1)` ⇒
    /// 扣 100 至 350；`boSpellFail=false` ⇒ `boSpellFire` 发 `RM_MAGICFIRE`；空怪表 ⇒ 造怪失败
    /// `Rejected` ⇒ `boTrain=false`（oracle `MakeSlave != null` 才置）。
    #[test]
    fn cast_with_amulet_consumes_and_reaches_summon() {
        let (mut w, caster) = cast_world();
        let cfg = PetSlaveConfig::default();
        let magic = MagicInfo::default();
        let mut train = train_state();
        let reg = MapRegistry::from_entries(MapLoader::new(".", "map", "envir"), Vec::new());
        let table = MonsterTable::new(Vec::new());
        let mut displays = DisplayTable::new();
        let slot = Some(AmuletSlot {
            index: 1,
            std_mode: 25,
            shape: 5,
            dura: 450,
        });
        let input = cast_input(caster, slot, &cfg, &magic);
        let out = cast_slave_spell(&mut w, &input, &mut train, &reg, &table, &mut displays);
        assert!(out.accepted, "护身符过 ⇒ boSpellFail=false");
        assert!(out.magic_fire, "boSpellFire 恒 true");
        let (loc, used) = out.amulet.expect("应消耗 ArmRingl");
        assert_eq!(loc, magic::ITEM_LOCATION_ARM_RINGL);
        assert_eq!(used.dura_after, 350, "450 - 1*100");
        assert!(!used.deleted, "耐久够 ⇒ 不删");
        assert!(
            matches!(out.effect, Some(SummonOutcome::Rejected)),
            "空怪表 ⇒ 造怪失败 = Rejected（oracle MakeSlave == null）"
        );
        assert!(
            !out.train.trained,
            "召唤未成功 ⇒ boTrain=false ⇒ 不练级、不抽 Random(3)"
        );
    }

    // ---- ⑨-2 第③片：20 诱惑（SKILL_TAMMING）宿主接线 ------------------------------------------

    /// 诱惑用世界：主人 Lv40 + 一只怪（`target_race`/`target_hp`/`target_level` 可调）。
    fn tamming_world(
        seed: i32,
        target_race: u8,
        target_hp: u16,
        target_level: u8,
    ) -> (World, i32, i32) {
        let mut w = World::new(32, 32);
        let caster = {
            let mut e = mir2_world::Entity::new(0, "主人".to_string(), 10, 10);
            e.race = mir2_world::ACTOR_RACE_PLAY;
            e.level = 40;
            w.add_entity(e)
        };
        let target = {
            let mut e = mir2_world::Entity::new(0, "怪".to_string(), 11, 10);
            e.race = target_race;
            e.level = target_level;
            e.hp = target_hp;
            e.max_hp = target_hp;
            w.add_entity(e)
        };
        w.rng = RandomNumber::with_seed(seed);
        w.clock_ms = 5_000;
        (w, caster, target)
    }

    fn tamming_magic() -> MagicInfo {
        MagicInfo {
            magic_id: SKILL_TAMMING,
            train_lv: 3,
            train_level: [0; 4],
            max_train: [100; 4],
            ..MagicInfo::default()
        }
    }

    fn tamming_train() -> SkillTrainState {
        SkillTrainState {
            magic_id: SKILL_TAMMING,
            level: 1,
            tran_point: 0,
            train_lv: 3,
            train_level: [0; 4],
            max_train: [100; 4],
        }
    }

    fn tamming_cast<'a>(
        caster: i32,
        target: Option<i32>,
        cfg: &'a PetSlaveConfig,
        magic: &'a MagicInfo,
    ) -> TammingCastInput<'a> {
        TammingCastInput {
            magic_level: 1,
            caster_id: caster,
            caster_abil_level: 40,
            target_id: target,
            fast_train: false,
            cfg,
            magic,
        }
    }

    /// **⑨-2 第③片判据 ①（`!Animal` 门）**：动物（race 51）⇒ oracle 连 `MagTamming` 都不进
    /// ⇒ **零取数**、`boTrain=false`（`MagicManager.cs:395`）。尾仍发 `RM_MAGICFIRE`。
    #[test]
    fn tamming_animal_gate_blocks_and_consumes_no_draw() {
        let seed = 12_345;
        let (mut w, caster, target) = tamming_world(seed, 51, 30, 1);
        let cfg = PetSlaveConfig::default();
        let magic = tamming_magic();
        let mut train = tamming_train();
        let input = tamming_cast(caster, Some(target), &cfg, &magic);
        let out = cast_tamming_spell(&mut w, &input, &mut train);
        assert!(!out.tamed, "Animal（race 51）⇒ 门不过");
        assert!(out.magic_fire, "尾仍发 RM_MAGICFIRE");
        assert!(!out.train.trained, "boTrain=false ⇒ 不练级");
        // 门不过 ⇒ 不抽任何数：把世界流的下一次抽取与同种子独立流比对。
        let mut fresh = RandomNumber::with_seed(seed);
        assert_eq!(
            w.rng.random_below(1_000_000),
            fresh.random_below(1_000_000),
            "`!Animal` 门不过 ⇒ MagTamming 不进、不消费 RNG"
        );
    }

    /// **⑨-2 第③片判据 ②（受奴落尾）**：非动物怪 + 足够高的主人等级 ⇒ 某些种子下受奴成立；
    /// 落尾 = `Master`/`SlaveList`/`SlaveMakeLevel`/`MasterRoyaltyTick`（经 [`World::tame_adopt`]），
    /// 且 `TargetCret` 被清（`MagicManager.cs:708`）。
    #[test]
    fn tamming_adopts_slave_and_clears_target_creature() {
        let cfg = PetSlaveConfig::default();
        let magic = tamming_magic();
        let mut adopted = None;
        for seed in 0..5_000 {
            let (mut w, caster, target) =
                tamming_world(seed, mir2_world::ACTOR_RACE_MONSTER, 30, 1);
            w.entities.get_mut(target).unwrap().ai.target_creature = Some(999);
            let mut train = tamming_train();
            let input = tamming_cast(caster, Some(target), &cfg, &magic);
            let out = cast_tamming_spell(&mut w, &input, &mut train);
            if out.tamed {
                adopted = Some((w, caster, target, out));
                break;
            }
        }
        let (w, caster, target, out) = adopted.expect("5000 个种子里应至少有一次受奴");
        assert!(out.magic_fire, "尾发 RM_MAGICFIRE");
        let c = w.entities.get(caster).unwrap();
        let t = w.entities.get(target).unwrap();
        assert_eq!(t.master, Some(caster), "受奴 ⇒ Master = 施法者");
        assert!(c.slave_list.contains(&target), "SlaveList.Add");
        assert_eq!(t.slave_make_level, 1, "SlaveMakeLevel = magicLevel");
        assert!(
            t.master_royalty_tick >= (20 * 60 * 1000) + 5_000,
            "MasterRoyaltyTick = (draw + (Level<<2)*5 + 20)*60_000 + now"
        );
        assert_eq!(t.hp, 30, "调用前 Master=None ⇒ 不 /10");
        assert_eq!(t.ai.target_creature, None, "TargetCret = null");
    }

    /// **⑨-2 第③片判据 ③（不重复受奴）**：调用前 `Master == 施法者` ⇒ oracle 走
    /// `RM_MAKEHOLYSEIZEMODE` 早退支（`:710-715`），**不再**落一次受奴尾（不重复 `/10`、不重复入表）。
    #[test]
    fn tamming_does_not_re_adopt_existing_slave() {
        let cfg = PetSlaveConfig::default();
        let magic = tamming_magic();
        let mut hit = false;
        for seed in 0..5_000 {
            let (mut w, caster, target) =
                tamming_world(seed, mir2_world::ACTOR_RACE_MONSTER, 30, 1);
            {
                let t = w.entities.get_mut(target).unwrap();
                t.master = Some(caster);
                t.is_slave = true;
                t.ai.target_creature = Some(999);
            }
            w.entities.get_mut(caster).unwrap().slave_list.push(target);
            let mut train = tamming_train();
            let input = tamming_cast(caster, Some(target), &cfg, &magic);
            let out = cast_tamming_spell(&mut w, &input, &mut train);
            // 早退支进入过（`TargetCret` 被清）即为命中样本。
            if w.entities.get(target).unwrap().ai.target_creature.is_none() {
                hit = true;
                assert!(!out.tamed, "已受奴 ⇒ 不重复受奴");
                let t = w.entities.get(target).unwrap();
                assert_eq!(t.hp, 30, "早退支不改 HP（不 /10）");
                assert_eq!(
                    w.entities.get(caster).unwrap().slave_list,
                    vec![target],
                    "不重复入表"
                );
                break;
            }
        }
        assert!(hit, "5000 个种子里应至少有一次进入早退支");
    }

    /// **⑨-2 残余判据（46 分身）**：`cast_clone_spell` 造克隆体 + 走共享尾（`boTrain=true`）。
    ///
    /// 红检：把 `do_spell_tail` 的 `bo_train` 真值改成 `false` —— `level < 3` 时不再抽
    /// `Random(3)+1` ⇒ 本用例末句的 RNG 序列断言必红。
    #[test]
    fn cast_clone_spawns_and_runs_train_tail() {
        let mut w = World::new(32, 32);
        let caster = {
            let mut e = mir2_world::Entity::new(0, "主人".to_string(), 5, 5);
            e.race = mir2_world::ACTOR_RACE_PLAY;
            e.level = 40;
            w.add_entity(e)
        };
        w.tick();
        let mk_train = || SkillTrainState {
            magic_id: mir2_world::SKILL_46,
            level: 1,
            tran_point: 0,
            train_lv: 3,
            train_level: [0; 4],
            max_train: [100; 4],
        };
        // 期望的尾：同一条 RNG 上跑一次 `do_spell_tail(boSpellFail=false, magic_fire=true, boTrain=true, …)`。
        let mut expect_rng = w.rng.clone();
        let mut expect_train = mk_train();
        let _ = magic::do_spell_tail(
            false,
            true,
            true,
            false,
            40,
            &mut expect_rng,
            &mut expect_train,
        );

        let mut train = mk_train();
        let input = CloneCastInput {
            caster_id: caster,
            caster_dir: 2,
            caster_abil_level: 40,
            fast_train: false,
        };
        let out = cast_clone_spell(&mut w, &input, &mut train);
        assert!(out.accepted, "46 无 boSpellFail ⇒ DoSpell true");
        assert!(out.magic_fire, "boSpellFire 恒 true");
        let c = out.clone.expect("应造出克隆体");
        assert_eq!(c.dir, 6, "Dir = GetBackDir(2)");
        assert_eq!(w.entities.get(c.clone_id).unwrap().name, "Clone");
        // 尾账本一致（`boTrain=true` 的 `Random(3)+1` 已按 oracle 位置消费）。
        assert_eq!(train.level, expect_train.level);
        assert_eq!(train.tran_point, expect_train.tran_point);
        assert_eq!(
            w.rng.random(),
            expect_rng.random(),
            "尾部取数次序与 do_spell_tail 一致"
        );
    }

    /// **⑨-2 护身符扣光帧体**：`deleted_amulet_body` = `SM_DELITEM(202)` +
    /// `EncodeBuffer(ClientItem{Dura=0})`；`StdMode==50` 的名称后缀用当下的 `Dura`(0) ⇒ `" #0"`。
    #[test]
    fn deleted_amulet_body_encodes_delitem_with_zero_dura() {
        let mut catalog = crate::ability::StdItemCatalog::new();
        catalog.insert(
            1,
            mir2_data::StdItem {
                name: "护身符".into(),
                std_mode: 25,
                shape: 5,
                ..mir2_data::StdItem::default()
            },
        );
        catalog.insert(
            2,
            mir2_data::StdItem {
                name: "药".into(),
                std_mode: 50,
                ..mir2_data::StdItem::default()
            },
        );
        let item = BagItem {
            make_index: 7,
            index: 1,
            dura: 100,
            dura_max: 1000,
            desc: [0; 14],
        };
        let body = deleted_amulet_body(42, &item, &catalog).expect("模板存在 ⇒ Some");
        let cm = mir2_protocol::frame::CommandMessage::from_bytes(&body).unwrap();
        assert_eq!(cm.recog, 42, "Recog=ActorId");
        assert_eq!(cm.ident, mir2_protocol::messages::SM_DELITEM);
        assert_eq!((cm.param, cm.tag, cm.series), (0, 0, 1), "Series=1");
        let expect = crate::itemwire::ClientItem {
            tpl: catalog.get(1).unwrap(),
            name: "护身符".into(),
            make_index: 7,
            dura: 0,
            dura_max: 1000,
            desc: [0; 14],
        };
        let expect_tail =
            mir2_protocol::edcode::encode(&crate::itemwire::encode_client_item(&expect));
        assert_eq!(
            mir2_protocol::edcode::decode(&body[12..]),
            mir2_protocol::edcode::decode(&expect_tail),
            "尾 = EncodeBuffer(ClientItem，Dura 恒 0)"
        );
        // `StdMode == 50`：名称后缀 `" #" + userItem.Dura`，而 oracle 在 `SendDelItems` 前已置 Dura=0。
        let item50 = BagItem {
            make_index: 8,
            index: 2,
            dura: 5,
            dura_max: 9,
            desc: [0; 14],
        };
        let body50 = deleted_amulet_body(42, &item50, &catalog).expect("模板存在 ⇒ Some");
        let expect50 = crate::itemwire::ClientItem {
            tpl: catalog.get(2).unwrap(),
            name: "药 #0".into(),
            make_index: 8,
            dura: 0,
            dura_max: 9,
            desc: [0; 14],
        };
        let expect_tail50 =
            mir2_protocol::edcode::encode(&crate::itemwire::encode_client_item(&expect50));
        assert_eq!(
            mir2_protocol::edcode::decode(&body50[12..]),
            mir2_protocol::edcode::decode(&expect_tail50),
            "StdMode==50 名称后缀用当下的 Dura（=0）"
        );
        // 模板缺失 ⇒ `SendDelItems` 的 `if (stdItem != null)` 不成立 ⇒ 无帧。
        let missing = BagItem {
            make_index: 9,
            index: 999,
            dura: 0,
            dura_max: 0,
            desc: [0; 14],
        };
        assert!(deleted_amulet_body(42, &missing, &catalog).is_none());
    }
}
