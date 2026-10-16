//! `Envir/Npcs.txt` 固定管理 NPC 的**宿主装载层**（红名单 R0-1，卡
//! `openmir2-npcs-fixed-loading`，放置半）。
//!
//! 解析半在 `mir2-rs/crates/envir/src/npcs.rs`（`LocalDB.cs:655-713` 的逐行 1:1）；
//! 世界面在 `mir2-rs/crates/world/src/npc.rs`（`World::{insert_npc,attach_npc_to_map}`）。
//! 本模块把两者接到宿主：
//!
//! 1. [`load_npcs_file`] = 文件半：`M2Share.GetEnvirFilePath("Npcs.txt")`（`LocalDB.cs:664`）+
//!    `File.Exists` 门（`:665`）+ `StringList.LoadFromFile`（GBK，`:668`）——解析交给 `crates/envir`。
//! 2. [`build_npc_spawns`] = `NpCinitialize` 的 `FindMap(MapName)` 门（`WorldServer.Merchant.cs:39-48`）：
//!    未命中地图 ⇒ 丢弃该记录（oracle `Error + RemoveAt`）。**照 `bins/game-svr/src/merchant.rs` 的
//!    O-1 口径在构造（抽数）前过门**（部署件三图俱在 ⇒ 门不触发；合成用例才是门的牙齿；
//!    与 oracle「先构造抽数、后 FindMap」的次序差异登记见模块尾）。
//! 3. [`place_npcs`] = 建实体 + 挂图两段（照 [`crate::merchant::spawn_merchants`]）：
//!    **正序** [`mir2_world::World::insert_npc`]（`LocalDB.LoadNpcs` 文件序；每条经 ctor 公共链
//!    `BaseObject`+`AnimalObject` 抽 **4 次**：`Random(1500)`/`Random(2000)+2000`/`Random(3000)`/
//!    `Random(3000)`，`BaseObject.cs:439/441` + `AnimalObject.cs:76/77`）、**倒序**
//!    [`mir2_world::World::attach_npc_to_map`]（`NpCinitialize` 的 `for (i=Count-1;i>=0;i--)`）。
//!
//! ## 与 oracle 的建模差异（登记）
//!
//! - **FindMap 门次序**：oracle 在 `LoadNpcs`（`:690-698`）先 `new …`（抽 4 次 ctor 随机数）
//!   再 `AddQuestNpc`，`FindMap` 门在**之后的** `NpCinitialize`（`:39-48`）⇒ 未命中地图的记录
//!   **已抽数**。本片照 merchant 卡 O-1 口径在构造前过门 ⇒ 未命中的记录**不抽数**。部署件
//!   `Npcs.txt` 三图（`0122`/`0150`/`3`）俱在 ⇒ 该差异**不可达**；仅在合成用例里体现。
//! - **NPC 脚本未装载**：oracle `NpCinitialize` 挂图成功后 `normNpc.LoadNPCScript()`
//!   （`NormNpc.cs:1590-1601`，`IsQuest=true` ⇒ `FilePath="Npc_def"`、脚本名 = `ChrName-MapName`，
//!   部署件 `Npc_Def/{比奇国王-0122,沙巴克城堡管理-0150,老人-3}.txt`）。脚本宿主（`NpcHost`）
//!   当前只吃 `MapQuest_def` 口径的 `QuestNpcRecord`（`file_path`/`script_name`）⇒ 固定 NPC 的
//!   `Npc_def/<ChrName>-<MapName>` 脚本装载**不在本片**（属 NPC 对话/点击行为卡，
//!   见 `openmir2-m3-quest-dialog-wiring`）。本片交付的是**世界面可见性**（P0 判据）。
//! - **NPC 的 `Run()`/`ProcessNpcs` 已建模**（**世界 tick 扩容首批 T3**，卡
//!   `openmir2-world-tick-expansion`）：`crates/world/src/npc.rs` 的 `World::npc_tick`/`npc_run`（固定 NPC
//!   `GuildOfficial.Run` / `Merchant.Run`），由宿主 `sim_loop` 起世界时 `enable_npc_tick()`。仍差：
//!   `NpcLimit` 墙钟预算（`whitelist.md` T-64）与 `RM_TURN`/`RM_HIT` 帧面（B-4）。

use mir2_envir::npcs::NpcInfo;
use mir2_envir::paths::envir_file_path;
use mir2_envir::{decode_text, MapLoader};
use mir2_world::npc::{NpcSpawn, NpcState};

/// NPC 装载/放置读数（日志/判据用）。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NpcPlacementReport {
    /// `LoadNpcs` 解析出的入库行数（`LocalDB.cs:685-708`）。
    pub lines: usize,
    /// 过 `FindMap` 门 + 挂图成功后**留在世界**的 NPC 数。
    pub placed: usize,
    /// 被 `FindMap` 门丢弃的 `(ChrName, MapName)`（`WorldServer.Merchant.cs:60`）。
    pub dropped_unknown_map: Vec<(String, String)>,
    /// 挂图失败被移除的 `ChrName`（`AddtoMapSuccess && !IsHide ⇒ RemoveAt`，`:49-52`）。
    pub dropped_add_to_map: Vec<String>,
}

/// `NpcInfo` → [`NpcSpawn`]（逐字段；`map_id` = `FindMap` 命中下标）。
#[must_use]
pub fn to_spawn(info: &NpcInfo, map_id: usize) -> NpcSpawn {
    NpcSpawn {
        name: info.chr_name.clone(),
        x: i32::from(info.x),
        y: i32::from(info.y),
        map_id,
        state: NpcState {
            npc_type: info.npc_type as i32,
            npc_flag: info.npc_flag,
            appr: info.appr,
            // ctor 抽数留空，由 `World::insert_npc` 按 oracle 次序落定。
            run_tick_delta: 0,
            base_search_time: 0,
            // 固定 NPC 经 `Npcs.txt` 装载**无** `BoCanMove`/`MoveTime` 列（`LocalDB.LoadNpcs`
            // 不置它们）⇒ 恒缺省（`Merchant.Run` 的随机移动支不可达）。
            ..NpcState::default()
        },
    }
}

/// `NpCinitialize` 的 `FindMap` 门（`WorldServer.Merchant.cs:39-48`）+ 装配。
///
/// `resolve_map` = `SystemShare.MapMgr.FindMap(MapName)` 的宿主等价物
/// （生产传入 `|n| registry.id_of(n)`；单测传入合成闭包）。
///
/// 返回 `(spawns, dropped_unknown_map)`；`dropped_unknown_map` 是未命中地图被丢的记录
/// （oracle `Error + RemoveAt`）。
#[must_use]
pub fn build_npc_spawns(
    npcs: &[NpcInfo],
    resolve_map: impl Fn(&str) -> Option<usize>,
) -> (Vec<NpcSpawn>, Vec<(String, String)>) {
    let mut spawns = Vec::with_capacity(npcs.len());
    let mut dropped = Vec::new();
    for info in npcs {
        match resolve_map(&info.map_name) {
            Some(map_id) => spawns.push(to_spawn(info, map_id)),
            None => dropped.push((info.chr_name.clone(), info.map_name.clone())),
        }
    }
    (spawns, dropped)
}

/// 建实体 + 挂图两段（照 [`crate::merchant::spawn_merchants`] 的两个独立次序）：
///
/// 1. `LocalDB.LoadNpcs` **正序**逐条 `new …`（ctor 公共链抽 4 次：`Random(1500)`/`Random(2000)+2000`/
///    `Random(3000)`/`Random(3000)`，`BaseObject.cs:439/441` + `AnimalObject.cs:76/77`）
///    ⇒ 先**正序** [`mir2_world::World::insert_npc`]（实体槽位序 = 文件序）；
/// 2. `NpCinitialize`（`WorldServer.Merchant.cs:39`）的 `for (i = Count-1; i >= 0; i--)`
///    ⇒ **倒序** [`mir2_world::World::attach_npc_to_map`] 落格（map cell `obj_list` 追加序 = 倒序）。
///
/// 返回 `(留在世界的 NPC 数, 挂图失败被移除的 ChrName)`。
#[must_use]
pub fn place_npcs(world: &mut mir2_world::World, spawns: Vec<NpcSpawn>) -> (usize, Vec<String>) {
    // ① 正序建实体（ctor 抽数 = LocalDB.LoadNpcs 文件序，走 world.rng 同一条流）。
    let pending: Vec<(Option<i32>, String)> = spawns
        .into_iter()
        .map(|spawn| {
            let name = spawn.name.clone();
            (world.insert_npc(spawn), name)
        })
        .collect();
    // ② 倒序挂图（NpCinitialize 的倒序 for）。
    let mut kept = 0;
    let mut failed = Vec::new();
    for (id, name) in pending.into_iter().rev() {
        match id {
            Some(id) if world.attach_npc_to_map(id) => kept += 1,
            _ => failed.push(name),
        }
    }
    (kept, failed)
}

/// `LoadNpcs` 的文件半（`LocalDB.cs:664-668`）：
/// `GetEnvirFilePath("Npcs.txt")` + `File.Exists` 门 + GBK 解码 + 整文件解析。
///
/// `Ok(None)` = 文件不存在（oracle `File.Exists` 假 ⇒ 整个方法静默返回）；
/// `Err` = 读失败/解析异常（oracle `LoadNpcs` 无 try/catch ⇒ 宿主按 WRN 登记）。
///
/// # Errors
/// 读文件失败（非 NotFound）或 `crates/envir` 解析层冒泡的 `Err`，返回人话错误。
pub fn load_npcs_file(loader: &MapLoader) -> Result<Option<Vec<NpcInfo>>, String> {
    let path = envir_file_path(&loader.base_path, &loader.envir_dir, "", "Npcs.txt");
    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("读 {} 失败：{e}", path.display())),
    };
    mir2_envir::npcs::load_npcs(&decode_text(&bytes))
        .map(Some)
        .map_err(|e| format!("解析 Npcs.txt 失败：{e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mir2_envir::npcs::NpcType;
    use mir2_shared::rng::RandomNumber;

    /// 用例级唯一临时目录的序号（照 `guardlist.rs`/`merchant.rs` 测试口径）。
    static COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

    struct Fixture {
        base: std::path::PathBuf,
        registry: mir2_envir::MapRegistry,
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.base);
        }
    }

    /// 64×64 全零地形 + 注册表（map `"0"`⇒id 0，map `"0122"`⇒id 1）。
    /// 尺寸取 64 让部署件坐标（如 `0122` 的 29,32）落在界内。
    fn fixture() -> Fixture {
        let base = std::env::temp_dir().join(format!(
            "gs-npcs-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        let mut bytes = vec![0u8; mir2_envir::HEADER_BYTES];
        bytes[0..2].copy_from_slice(&64_i16.to_le_bytes());
        bytes[2..4].copy_from_slice(&64_i16.to_le_bytes());
        bytes.resize(
            mir2_envir::HEADER_BYTES + 64 * 64 * mir2_envir::CELL_BYTES,
            0,
        );
        std::fs::create_dir_all(base.join("Map")).unwrap();
        std::fs::write(base.join("Map").join("0.map"), &bytes).unwrap();
        std::fs::write(base.join("Map").join("0122.map"), &bytes).unwrap();
        let loader = MapLoader::new(base.clone(), "Map", "Envir");
        let registry = mir2_envir::MapRegistry::from_entries(
            loader,
            vec![
                mir2_envir::MapInfoEntry {
                    map_name: "0".to_string(),
                    map_file_name: "0".to_string(),
                    desc: String::new(),
                    server_index: 0,
                    flag: mir2_envir::MapInfoFlag::default(),
                },
                mir2_envir::MapInfoEntry {
                    map_name: "0122".to_string(),
                    map_file_name: "0122".to_string(),
                    desc: String::new(),
                    server_index: 0,
                    flag: mir2_envir::MapInfoFlag::default(),
                },
            ],
        );
        assert_eq!(registry.id_of("0"), Some(0));
        assert_eq!(registry.id_of("0122"), Some(1));
        Fixture { base, registry }
    }

    fn info(name: &str, ty: NpcType, map: &str, x: i16, y: i16, flag: i16, appr: u16) -> NpcInfo {
        NpcInfo {
            chr_name: name.to_string(),
            npc_type: ty,
            map_name: map.to_string(),
            x,
            y,
            npc_flag: flag,
            appr,
        }
    }

    /// 一个固定 NPC 的可对拍字段快照（避免 9 元组触发 `clippy::type_complexity`）。
    #[derive(Debug, PartialEq, Eq)]
    struct NpcRow {
        id: i32,
        name: String,
        map_id: usize,
        x: i32,
        y: i32,
        race: u8,
        npc_type: i32,
        npc_flag: i16,
        appr: u16,
    }

    /// 按实体槽位序列出固定 NPC 的字段快照（闭包捕获冲突 ⇒ 独立函数）。
    fn npc_fields(world: &mir2_world::World) -> Vec<NpcRow> {
        world
            .npc_ids_sorted()
            .into_iter()
            .map(|id| {
                let e = world.entities.get(id).expect("NPC 在仓库");
                let st = e.npc.expect("npc 状态已设");
                NpcRow {
                    id: e.id,
                    name: e.name.clone(),
                    map_id: e.map_id,
                    x: e.x,
                    y: e.y,
                    race: e.race,
                    npc_type: st.npc_type,
                    npc_flag: st.npc_flag,
                    appr: st.appr,
                }
            })
            .collect()
    }

    /// 判据 1（端到端逐字段）：三条部署件形态行 ⇒ 世界内三个固定 NPC，`name/map_id/(x,y)/
    /// race/npc_type/flag/appr` 逐字段相等；`NpcType` 三分支各一。
    ///
    /// 红检：去掉 `place_npcs` 的放置（或 `build_npc_spawns` 过滤全丢）⇒ 世界 0 个 ⇒ 红。
    #[test]
    fn place_npcs_puts_each_npc_in_world_field_by_field() {
        let f = fixture();
        let npcs = vec![
            info("比奇国王", NpcType::GuildOfficial, "0122", 29, 32, 0, 8),
            info(
                "沙巴克城堡管理",
                NpcType::CastleOfficial,
                "0122",
                7,
                16,
                0,
                8,
            ),
            info("老人", NpcType::Merchant, "0", 3, 3, 0, 1),
        ];
        let (spawns, dropped) = build_npc_spawns(&npcs, |n| f.registry.id_of(n));
        assert!(dropped.is_empty(), "三图俱在 ⇒ 门不触发");
        assert_eq!(spawns.len(), 3);

        let mut world = mir2_world::World::with_maps(vec![(64, 64), (64, 64)]);
        let (kept, failed) = place_npcs(&mut world, spawns);
        assert_eq!(kept, 3);
        assert!(failed.is_empty());

        assert_eq!(
            npc_fields(&world),
            vec![
                NpcRow {
                    id: world.npc_ids_sorted()[0],
                    name: "比奇国王".to_string(),
                    map_id: 1,
                    x: 29,
                    y: 32,
                    race: 10,
                    npc_type: 1,
                    npc_flag: 0,
                    appr: 8,
                },
                NpcRow {
                    id: world.npc_ids_sorted()[1],
                    name: "沙巴克城堡管理".to_string(),
                    map_id: 1,
                    x: 7,
                    y: 16,
                    race: 10,
                    npc_type: 2,
                    npc_flag: 0,
                    appr: 8,
                },
                NpcRow {
                    id: world.npc_ids_sorted()[2],
                    name: "老人".to_string(),
                    map_id: 0,
                    x: 3,
                    y: 3,
                    race: 10,
                    npc_type: 0,
                    npc_flag: 0,
                    appr: 1,
                },
            ],
            "name/map_id/(x,y)/race/npc_type/flag/appr 逐字段"
        );
        // race = ActorRace.NPC ⇒ 不可被选为攻击目标（`combat::is_attack_target_base` 的 race 窗）。
        for row in npc_fields(&world) {
            assert_eq!(
                world.entities.get(row.id).unwrap().race,
                mir2_world::combat::ACTOR_RACE_NPC
            );
        }
    }

    /// 判据 2（`FindMap` 门 + 两段次序）：未知地图被丢、世界内剩下两条；实体槽位序 = 文件正序。
    ///
    /// 红检：把 `build_npc_spawns` 的门删掉（全部收起）⇒ 未知地图那条也进世界 ⇒ 计数断言必红。
    #[test]
    fn find_map_gate_drops_unknown_map_and_keeps_file_order() {
        let f = fixture();
        let npcs = vec![
            info("甲", NpcType::Merchant, "0122", 1, 1, 0, 8),
            info("乙", NpcType::Merchant, "9999", 1, 1, 0, 8), // 未知地图 ⇒ 丢
            info("丙", NpcType::Merchant, "0", 2, 2, 0, 8),
        ];
        let (spawns, dropped) = build_npc_spawns(&npcs, |n| f.registry.id_of(n));
        assert_eq!(dropped, vec![("乙".to_string(), "9999".to_string())]);
        assert_eq!(spawns.len(), 2);

        let mut world = mir2_world::World::with_maps(vec![(64, 64), (64, 64)]);
        let (kept, _) = place_npcs(&mut world, spawns);
        assert_eq!(kept, 2);
        let names: Vec<String> = npc_fields(&world).into_iter().map(|f| f.name).collect();
        assert_eq!(
            names,
            vec!["甲".to_string(), "丙".to_string()],
            "槽位序 = 文件正序"
        );
    }

    /// 判据 3（ctor 抽数，改坏必红）：`insert_npc` 每条按 oracle ctor **公共链**抽 4 次
    /// （`BaseObject` `Random(1500)`→`Random(2000)+2000`，再 `AnimalObject` `Random(3000)`→
    /// `Random(3000)`；`BaseObject.cs:439/441` + `AnimalObject.cs:76/77`），位置与手工复算对齐；
    /// 两条 ⇒ 共 8 抽。
    ///
    /// **订正缘由**：原断言「恰 2 抽」与 oracle **相反**——每只固定 NPC 少抽 `AnimalObject` 的
    /// 2 抽，共享 RNG 流自首只 NPC 构造起整体错位（本卡 critical）。现按 4 抽复算。
    ///
    /// 红检：删掉 `insert_npc` 里的抽数（或只抽 2 次）⇒ 随机流位置断言必红。
    #[test]
    fn insert_npc_draws_four_ctor_values_in_oracle_order() {
        let seed = 4242;
        let mut world = mir2_world::World::with_maps(vec![(64, 64), (64, 64)]);
        world.rng = RandomNumber::with_seed(seed);
        let spawns = vec![
            to_spawn(&info("甲", NpcType::Merchant, "0", 1, 1, 0, 8), 0),
            to_spawn(&info("乙", NpcType::Merchant, "0", 2, 2, 0, 8), 0),
        ];
        let _ = place_npcs(&mut world, spawns);

        // 期望侧：每条 `BaseObject`(1500→2000) 再 `AnimalObject`(3000→3000)（字面 oracle 参数）。
        let mut b = RandomNumber::with_seed(seed);
        let expect: Vec<(i32, i32, i32, i32)> = (0..2)
            .map(|_| {
                let run = b.random_below(1500);
                let search = b.random_below(2000) + 2000;
                let attack = b.random_below(3000);
                let walk = b.random_below(3000);
                (run, search, attack, walk)
            })
            .collect();

        assert_eq!(
            world.rng.random(),
            b.random(),
            "抽数个数/次序与手工复算一致"
        );
        let ids = world.npc_ids_sorted();
        let s0 = world.entities.get(ids[0]).unwrap().npc.unwrap();
        let s1 = world.entities.get(ids[1]).unwrap().npc.unwrap();
        assert_eq!(
            (
                s0.run_tick_delta,
                s0.base_search_time,
                s0.attack_tick_delta,
                s0.walk_tick_delta
            ),
            expect[0]
        );
        assert_eq!(
            (
                s1.run_tick_delta,
                s1.base_search_time,
                s1.attack_tick_delta,
                s1.walk_tick_delta
            ),
            expect[1]
        );
    }

    /// **跨路径一致性门（本卡重点，防「少抽 2 次」复发）**：
    /// oracle 里怪物 / 固定 NPC / 商人列表**都经**同一条 ctor 公共链
    /// `BaseObject`（`BaseObject.cs:439/441`）→ `AnimalObject`（`AnimalObject.cs:76/77`），
    /// 抽同样的 4 个数、同序。不变量 = 一处定义（`mir2_world::BASE_ANIMAL_CTOR_DRAW_MODS` +
    /// `BaseAnimalCtorDraws::draw`）+ 两模块三调用点直接消费（`crates/world/src/npc.rs` 的
    /// `insert_npc`；`crates/world/src/merchant.rs` 的 `merchant_ctor_run_delay`/`spawn_merchant`）+
    /// 一条被本门钉齐的同值副本（怪物路径 `bins/game-svr/src/mongen.rs` 硬编码 4 抽）。
    /// 本判据把三条路径都钉在同一张表上 ⇒ 改任一路径的抽数即红。
    ///
    /// 注：商人后 3 抽（`SearchTime`/`AttackTick`/`WalkTick`）在 oracle 里对商人**不被消费**
    /// （`Merchant.Run` 只读 `RunTick`；动物 AI 不驱动 NPC），其**值被丢弃**、只有流位置可观测；
    /// 故商人路径咬「次数（4 抽）+ 第 0 抽 kind/值」，其余 3 抽的 kind 由单一表保证。
    ///
    /// 红检：把 `insert_npc`（或 `merchant_ctor_run_delay`，或宿主 `mongen::draw_monster_construct`
    /// 的公共段）改回少抽 2 次 ⇒ 对应 `assert_eq!` 必红。
    #[test]
    fn base_animal_ctor_chain_matches_oracle_on_all_three_paths() {
        // ① 定义侧（一处定义）：kind + 顺序 = oracle。
        assert_eq!(
            mir2_world::BASE_ANIMAL_CTOR_DRAW_MODS,
            [1500, 2000, 3000, 3000]
        );

        let seed = 20261017_i32;
        let expect_of = |seed: i32| {
            let mut r = RandomNumber::with_seed(seed);
            mir2_world::BaseAnimalCtorDraws::draw(&mut r)
        };

        // ② 怪物路径（宿主 `mongen::draw_monster_construct`；race 11 = GuardUnit，公共链后无额外抽）。
        {
            let mut rng = RandomNumber::with_seed(seed);
            let d = crate::mongen::draw_monster_construct(&mut rng, 11);
            let e = expect_of(seed);
            assert_eq!(
                (
                    d.run_tick_delta,
                    d.base_search_time,
                    d.attack_tick_delta,
                    d.walk_tick_delta
                ),
                (
                    e.run_tick_delta,
                    e.base_search_time,
                    e.attack_tick_delta,
                    e.walk_tick_delta
                ),
                "怪物路径公共段 ctor 抽取必须与公共链表一致"
            );
        }

        // ③ 固定 NPC 路径（`World::insert_npc`）。
        {
            let mut world = mir2_world::World::with_maps(vec![(8, 8)]);
            world.rng = RandomNumber::with_seed(seed);
            let id = world
                .spawn_npc(to_spawn(&info("甲", NpcType::Merchant, "0", 1, 1, 0, 8), 0))
                .expect("挂图成功");
            let s = world.entities.get(id).unwrap().npc.unwrap();
            let e = expect_of(seed);
            assert_eq!(
                (
                    s.run_tick_delta,
                    s.base_search_time,
                    s.attack_tick_delta,
                    s.walk_tick_delta
                ),
                (
                    e.run_tick_delta,
                    e.base_search_time,
                    e.attack_tick_delta,
                    e.walk_tick_delta
                ),
                "固定 NPC 路径 ctor 抽取必须与公共链表一致（原只抽 2 次 = 少抽 2 次）"
            );
            // 流位置：同 4 抽后 `world.rng` 必须落回参考流同一位置。只比上面 4 个值**抓不到
            // 「多抽 1 次」**（不改这 4 个值、只挪流位置）⇒ 显式比流位置（照 ④ 商人分支写法）。
            let mut refr = RandomNumber::with_seed(seed);
            let _ = mir2_world::BaseAnimalCtorDraws::draw(&mut refr);
            assert_eq!(
                world.rng.random(),
                refr.random(),
                "固定 NPC 路径 ctor 抽取次数/次序 = 公共链 4 抽（多抽 1 次 ⇒ 流位置必错）"
            );
        }

        // ④ 商人列表路径（宿主 `spawn_merchants` 的预抽 = `World::merchant_ctor_run_delay`）。
        {
            let mut world = mir2_world::World::with_maps(vec![(8, 8)]);
            world.clock_ms = 0;
            world.rng = RandomNumber::with_seed(seed);
            let got_run = world.merchant_ctor_run_delay();
            let mut refr = RandomNumber::with_seed(seed);
            let e = mir2_world::BaseAnimalCtorDraws::draw(&mut refr);
            assert_eq!(
                got_run,
                i64::from(e.run_tick_delta),
                "商人 RunTick = 公共链第 0 抽"
            );
            assert_eq!(
                world.rng.random(),
                refr.random(),
                "商人路径 ctor 抽取次数/次序 = 公共链 4 抽（少抽 ⇒ 流位置必错）"
            );
        }
    }

    /// 判据 4（`NpCinitialize` 倒序挂图）：map cell `obj_list` 追加序 = 文件**倒序**
    /// （后加载的先 AddToMap）；实体槽位序仍是文件正序。
    ///
    /// 红检：把 `place_npcs` 的 attach 循环改回正序 ⇒ `obj_list[0]` 断言必红。
    #[test]
    fn npc_initialize_attaches_in_reverse_order() {
        // 两条落在**同一格**（5×5 图，坐标 (2,2)）⇒ 可观察 obj_list 追加序。
        let spawns = vec![
            to_spawn(&info("A", NpcType::Merchant, "0", 2, 2, 0, 8), 0),
            to_spawn(&info("B", NpcType::Merchant, "0", 2, 2, 0, 8), 0),
        ];
        let mut world = mir2_world::World::with_maps(vec![(64, 64), (64, 64)]);
        let (kept, _) = place_npcs(&mut world, spawns);
        assert_eq!(kept, 2);

        let (cell, ok) = world.map(0).get_cell_info(2, 2);
        assert!(ok);
        let first = cell.obj_list[0].cell_obj_id;
        let first_name = world.entities.get(first).expect("实体在").name.clone();
        assert_eq!(
            first_name, "B",
            "倒序 NpCinitialize ⇒ 后加载的 B 先 AddToMap"
        );
        // 实体槽位序（QuestNpcList 序）仍 = 文件正序。
        let slot_names: Vec<String> = npc_fields(&world).into_iter().map(|f| f.name).collect();
        assert_eq!(slot_names, vec!["A".to_string(), "B".to_string()]);
    }

    /// 判据 5（文件半）：无文件 ⇒ `Ok(None)`；有 GBK 文件 ⇒ 逐行解析（部署件三行形态）。
    #[test]
    fn load_npcs_file_missing_is_none_and_reads_gbk() {
        let f = fixture();
        let loader = MapLoader::new(f.base.clone(), "Map", "Envir");
        assert!(
            load_npcs_file(&loader).unwrap().is_none(),
            "无 Npcs.txt ⇒ Ok(None)（File.Exists 门）"
        );
        // 部署件三行体（中文名按 **GBK 字节**写：`decode_text` 按 GBK 解码）+ CRLF + 一行注释。
        std::fs::create_dir_all(f.base.join("Envir")).unwrap();
        let mut body = Vec::new();
        // 注释行 ";新建 NPC 文件"。
        body.extend_from_slice(b";\xd0\xc2\xbd\xa8 NPC \xce\xc4\xbc\xfe\r\n");
        // 「比奇国王」的 GBK 字节 = B1 C8 C6 E6 B9 FA CD F5。
        body.extend_from_slice(&[0xB1, 0xC8, 0xC6, 0xE6, 0xB9, 0xFA, 0xCD, 0xF5]);
        body.extend_from_slice(b"           1     0122  29    32    0     8\r\n");
        // 「老人」的 GBK 字节 = C0 CF C8 CB。
        body.extend_from_slice(&[0xC0, 0xCF, 0xC8, 0xCB]);
        body.extend_from_slice(b"               0     3     669   338   0     1\r\n");
        std::fs::write(f.base.join("Envir").join("Npcs.txt"), &body).unwrap();
        let list = load_npcs_file(&loader).unwrap().expect("Some(list)");
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].chr_name, "比奇国王");
        assert_eq!(list[0].npc_type, NpcType::GuildOfficial);
        assert_eq!((list[1].x, list[1].y), (669, 338));
    }

    /// 判据 6（去掉加载 ⇒ 0 个 NPC）：`load_npcs_file` 返回 `None`（无文件）时整个世界 0 个 NPC
    /// ——即「去加载 ⇒ 0 个」这条变异必红（这里是正向对照）。
    #[test]
    fn missing_file_yields_zero_npcs() {
        let f = fixture();
        let loader = MapLoader::new(f.base.clone(), "Map", "Envir");
        let mut world = mir2_world::World::with_maps(vec![(64, 64), (64, 64)]);
        if let Some(npcs) = load_npcs_file(&loader).unwrap() {
            let (spawns, _) = build_npc_spawns(&npcs, |n| f.registry.id_of(n));
            let _ = place_npcs(&mut world, spawns);
        }
        assert_eq!(
            world.npc_ids_sorted().len(),
            0,
            "无 Npcs.txt ⇒ 世界 0 个 NPC"
        );
    }

    /// **真机部署件向量**：`E:/MirServer/M2GameSvr` 的 `Envir/Npcs.txt`（227B/3 行）+ 真注册表
    /// （`Envir/MapInfo.txt` + `Map/*.map`）。
    ///
    /// 断言：解析 3 条、`FindMap` 门不触发（`0122`/`0150`/`3` 三段俱在 MapInfo）、逐字段形态。
    /// 需真实部署件与 `MIR2_BASE_DIR`（缺省 `E:/MirServer/M2GameSvr`）；用
    /// `cargo test -p game-svr --lib real_deployment_npcs -- --ignored --nocapture` 跑。
    #[test]
    #[ignore = "真机部署件向量：需 E:/MirServer/M2GameSvr（证据/owner 跑）"]
    fn real_deployment_npcs() {
        let base =
            std::env::var("MIR2_BASE_DIR").unwrap_or_else(|_| "E:/MirServer/M2GameSvr".into());
        let loader = MapLoader::new(&base, "Map", "Envir");
        let registry = loader.load_registry().expect("载入真地图注册表");
        let npcs = load_npcs_file(&loader)
            .expect("载入真 Npcs.txt")
            .expect("Some(list)");
        let (spawns, dropped) = build_npc_spawns(&npcs, |n| registry.id_of(n));
        for s in &spawns {
            println!(
                "NPCS_VECTOR name={} type={} map_id={} x={} y={} flag={} appr={}",
                s.name, s.state.npc_type, s.map_id, s.x, s.y, s.state.npc_flag, s.state.appr
            );
        }
        println!(
            "NPCS_VECTOR records={} spawns={} dropped={:?}",
            npcs.len(),
            spawns.len(),
            dropped
        );
        assert_eq!(npcs.len(), 3, "部署件 Npcs.txt 入库 3 条");
        assert!(
            dropped.is_empty(),
            "0122/0150/3 三段俱在 MapInfo ⇒ 门不触发"
        );
        assert_eq!(spawns.len(), 3);
        assert_eq!(
            npcs.iter().map(|n| n.chr_name.as_str()).collect::<Vec<_>>(),
            vec!["比奇国王", "沙巴克城堡管理", "老人"]
        );
        assert_eq!(
            npcs.iter().map(|n| n.npc_type).collect::<Vec<_>>(),
            vec![
                NpcType::GuildOfficial,
                NpcType::CastleOfficial,
                NpcType::Merchant
            ]
        );
        assert_eq!((npcs[0].x, npcs[0].y), (29, 32));
        assert_eq!((npcs[1].x, npcs[1].y), (7, 16));
        assert_eq!((npcs[2].x, npcs[2].y), (669, 338));
    }
}
