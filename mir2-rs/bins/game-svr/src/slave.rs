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
//! ## 未接（登记，别按常识补）
//!
//! - **`MakeSlave` 的 `RecalcAbilitys` 派生类速度改写**（`WhiteSkeleton`/`ElfMonster`）与
//!   `RefNameColor` 色彩：归 `openmir2-m3-pet-slave-lifecycle`（⑨-3）。
//! - **46 分身**（`MagMakeClone`）：见 `crates/world::slave` 的 `CLONE_DEFERRED`。
//! - **20 诱惑**（`MagTamming` 受奴尾）需要 `World::tame_adopt`，而该尾在
//!   `openmir2-m3-pet-slave-lifecycle`（⑨-3，待审）里落；本片只接 17/30/41 召唤面，
//!   20 由 ⑨-3 合入后的补片接（登记在卡上，别在此重复实现）。
//! - **护身符扣光分支的 `SM_DELITEMS` 帧**：oracle `UseAmulet` 的 `else` 支
//!   （`MagicBase.cs:124-126`）`SendDelItems` 尚无编码器 ⇒ 状态照改（`Dura=0`/清槽），
//!   帧面登记。

use std::path::Path;

use mir2_data::exps::IniFile;
use mir2_data::pet_slave::PetSlaveConfig;
use mir2_data::MagicInfo;
use mir2_envir::MapRegistry;
use mir2_shared::rng::RandomNumber;
use mir2_world::magic::{self, AmuletSlot, AmuletUse, SkillTrainState};
use mir2_world::slave::{SKILL_ANGEL, SKILL_SINSU, SKILL_SKELLETON, SKILL_TAMMING};
use mir2_world::{SlaveSpawner, SummonOutcome, World};

use crate::downlink::DisplayTable;
use crate::mongen::{create_monster_oracle, MonsterTable, WorldMonsterSink};

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
    let _ = SKILL_TAMMING; // 20 由 ⑨-3 合入后的补片接（模块头登记）；引用以固定常量来源。
    SlaveCastOutcome {
        accepted: tail.returned,
        magic_fire: tail.magic_fire,
        amulet,
        effect,
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
}
