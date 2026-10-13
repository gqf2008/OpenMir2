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
//! ## 未接（登记，别按常识补）
//!
//! - **`CM_SPELL(3017)` 的效果体派发**：oracle 的召唤技能在 `DoSpell` 的 `switch` 里
//!   （`MagicManager.cs:373/455/467`），前置还有 `MagicBase.CheckAmulet`/`UseAmulet`
//!   （17/41 需 1 个 type-1、30 需 5 个）与 `boTrain` 熟练度尾。本模块只提供**召唤编排**；
//!   把 `handle_spell_cmd` 接到 `summon_from_spell` 需要同时接护身符面（`UseItems[ArmRingl/Bujuk]`
//!   + `ItemSystem.GetStdItem`）与 `RM_SPELL`/`RM_MAGICFIRE` 帧面 —— 归后续接线片。
//! - **`MakeSlave` 的 `RecalcAbilitys` 派生类速度改写**（`WhiteSkeleton`/`ElfMonster`）与
//!   `RefNameColor` 色彩：归 `openmir2-m3-pet-slave-lifecycle`（⑨-3）。
//! - **46 分身**（`MagMakeClone`）：见 `crates/world::slave` 的 `CLONE_DEFERRED`。

use std::path::Path;

use mir2_data::exps::IniFile;
use mir2_data::pet_slave::PetSlaveConfig;
use mir2_envir::MapRegistry;
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
