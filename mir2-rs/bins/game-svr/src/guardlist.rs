//! GuardList **放置半**宿主接线（卡 `openmir2-guardlist-guards-loading` 放置片）。
//!
//! 解析半在 `mir2-rs/crates/envir/src/guardlist.rs`（`LocalDB.cs:100-141` 的逐行 1:1）；
//! 本模块把解析结果接到世界：
//!
//! 1. [`regen_monster_by_name`] = oracle `WorldEngine.RegenMonsterByName`
//!    （`src/GameSrv/Word/WorldServer.MonGen.cs:382-438`）：
//!    `GetMonRace`（`:384`，`WorldServer.cs:167-174`）→ `CreateMonster`（`:385`，取数次序复用
//!    [`crate::mongen::create_monster_oracle`]：构造链 → `Dir=RandomByte(8)` → 掉落预算 →
//!    CoolEye → 地形移位）→ `GetMonsterThreadId`（`:386`，`WorldServer.cs:176-183`，
//!    **大小写敏感**）→ 合成 `MonGenInfo{Range=0, Count=1, ZenTime=0, MissionGenRate=0}`
//!    （`:390-404`）+ `TryAdd` 登记 `CertList`（`:405`）→ 追加到该线程列表尾（`:407`）。
//! 2. [`place_guards`] = `LoadGuardList` 的放置循环（`LocalDB.cs:113-135`）：逐行放置，
//!    成功 ⇒ `guard.Dir = (byte)StrToInt(direction, 0)`（`:133`）**覆盖** `RandomByte(8)`；
//!    失败（返 null ⇒ 未知地图/未知模板名/31 次移位失败）⇒ 只跳过置向。
//! 3. [`load_guard_list_file`] = 文件半：`M2Share.GetEnvirFilePath("GuardList.txt")` +
//!    `File.Exists` 门 + `StringList.LoadFromFile`（GBK）——解析交给 `crates/envir`。
//!
//! ## oracle 语义要点（逐条实读钉死）
//!
//! - **加载时机**：`GameApp.cs:239-243`（`InitializeWorld`）——`InitializationMonsterThread`（`:238`）
//!   之后、`WorldEngine.Initialize()`（`:244`）之前；即**启动装载阶段**（先于首个 regen tick），
//!   且必须晚于 `InitializeMonster`（`GameApp.cs:150`，`MonsterThreadMap` 在那里填齐）。
//!   宿主侧 = `MonGenHost::start` 之后、首个 `step_mon_gen` 之前。
//! - **`[Server] VentureServer` 门**：`GameApp.cs:240` `if (!Config.VentureServer)` 才调
//!   `LoadGuardList`（整文件不读）——门在**调用点**，由宿主（main.rs）持有。
//! - **合成 `MonGenInfo` 的两个照搬缺陷**（`WorldServer.MonGen.cs:390-407` + `MonGenInfo.cs:41-57`）：
//!   `Race` 不回填（保持 0）⇒ `RegenMonsters` 的 `race <= 0` 早退（`:817-820`）⇒ **守卫死后不重生**、
//!   regen 门每轮空转不刷；`TryAdd` 只登记 `CertList`、**不动 `ActiveCount`**（保持 0 ⇒
//!   `genModCount(1) > ActiveCount(0)` 恒真，但 race 门拦下）。两条都不"顺手修"。
//! - **静态性归属**：守卫是普通 `GuardUnit`/`ArcherPolice` 实体（race 11/12，走
//!   [`crate::mongen::race_is_spawnable`] 的标准生成路径），「站桩不游走」来自模板 AI
//!   （承接卡 `openmir2-m3-monster-ai-tick`），**本片不特判**。
//! - **`GetMonsterThreadId` 失败支**（`:436-439` 返 null）：名字大小写与 `MonsterThreadMap`
//!   不一致时 oracle 的 cert 已在 `CreateMonster` 里 `AddMapObject`（孤儿留在图上）；
//!   本仓实体也已入 `world`（同形保留，登记）。`Race` 查表是 OrdinalIgnoreCase、线程表是
//!   大小写敏感 ⇒ 仅"大小写错位"可触发；部署件 `GuardList.txt` 与 `monsters` 表逐字一致 ⇒
//!   实际不可达。
//! - **RNG 流**：与 MonGen 同一条 `M2Share.RandomNumber`（构造链/Dir/掉落/CoolEye 逐抽）；
//!   本模块经 `place_guards` 临时移出 `world.rng`（与 `step_mon_gen` 同口径）。
//!   oracle 里 `LoadGuardList` 与已启动怪物线程的 regen 抽数存在墙钟竞态；本仓单线程模型
//!   固定为「守卫先抽、regen 后抽」（装载在首个 tick 之前），登记为建模差异。

use mir2_envir::guardlist::GuardInfo;
use mir2_envir::paths::envir_file_path;
use mir2_envir::{decode_text, MapLoader, MapRegistry};
use mir2_shared::rng::RandomNumber;
use mir2_world::mongen::{MonGenEntry, MonGenState};

use crate::downlink;
use crate::mongen::{
    create_monster_oracle, MonGenHost, MonsterSink, MonsterTable, WorldMonsterSink,
};

/// [`place_guards`] 的读数（日志/判据用）。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GuardPlacementReport {
    /// 解析出的守卫行数（`LoadGuardList` 的入库行）。
    pub lines: usize,
    /// `RegenMonsterByName` 成功（含置向）的行数。
    pub placed: usize,
    /// 返 null 被跳过的行数（未知地图 / 未知模板名 / 31 次移位失败 ⇒ 只跳过置向）。
    pub skipped: usize,
}

/// oracle `WorldEngine.RegenMonsterByName`（`WorldServer.MonGen.cs:382-438`）。
///
/// 返回 `Some(actor_id)` = 放置成功（调用方随后置 `Dir`）；`None` = oracle 的 `null`
/// （`FindMap` 失败 / race 不在 switch / 移位 31 次失败 / `GetMonsterThreadId` 为 -1）。
///
/// RNG 契约与 [`crate::mongen::create_monster_oracle`] 完全一致（本函数不额外抽数）。
#[allow(clippy::too_many_arguments)] // 与 `create_monster_oracle`/`MonGenHost::step` 同款豁免
pub fn regen_monster_by_name<S: MonsterSink>(
    rng: &mut RandomNumber,
    sink: &mut S,
    table: &MonsterTable,
    state: &mut MonGenState,
    map_name: &str,
    x: i16,
    y: i16,
    mon_name: &str,
) -> Option<i32> {
    // `:384` `GetMonRace(sMonName)`（OrdinalIgnoreCase；未命中 -1 ⇒ create 门拦下，不抽数）。
    let race = table.race_of(mon_name);
    // `:385` `CreateMonster(sMap, nX, nY, nRace, sMonName)`（构造链 → Dir → 掉落 → CoolEye → 移位）。
    let id = create_monster_oracle(rng, sink, table, map_name, x, y, race, mon_name)?;
    // `:386-388` `GetMonsterThreadId`；-1 ⇒ return null。**登记偏差（v1-F1）**：oracle
    // `WorldServer.cs:80` 的 MonsterThreadMap 实为 OrdinalIgnoreCase；本仓 BTreeMap 精确匹配
    // ——ASCII 大小写错位的守卫名会落「孤儿实体+跳过」而 oracle 正常放置（部署件守卫名纯
    // CJK 不可达；要改查表 eq_ignore_ascii_case 另开片）。
    // （实体已入世界 = oracle 的孤儿-on-map，见模块头登记）。
    let thread_id = usize::try_from(state.monster_thread_map.get(mon_name).copied()?).ok()?;
    // `:390-406` 合成 `MonGenInfo`：Range=0/Count=1/ZenTime=0/MissionGenRate=0；
    // **Race 不回填**（=0 ⇒ `RegenMonsters` race<=0 早退，守卫死后不重生）；
    // **ActiveCount 不加**（`MonGenInfo.cs:41-57` 的 `TryAdd` 只登记 CertList）⇒ regen 门
    // 每轮空转（genModCount(1) > 0 恒真、race 门拦下、StartTick 回写）——照搬。
    let idx = state.entries.len();
    state.entries.push(MonGenEntry {
        map_name: map_name.to_string(),
        x,
        y,
        mon_name: mon_name.to_string(),
        range: 0,
        count: 1,
        active_count: 0,
        zen_time: 0,
        mission_gen_rate: 0,
        race: 0,
        start_tick: 0,
        cert_list: vec![id],
        thread_id,
    });
    // `:407` `MonGenInfoThreadMap[threadId].Add(MonGenInfo)`（追加到该线程列表尾）。
    // oracle 在该 thread id 无列表时抛 `KeyNotFoundException`（整个 `LoadGuardList` 被 catch
    // 吞掉、余下行全跳）；本仓 `entry().or_default()` 建空列表继续（部署 limit=1 ⇒ 恒有
    // thread 0，两行为相同；limit>1 的差异登记在模块头）。
    let thread = state.threads.entry(thread_id).or_default();
    thread.id = thread_id;
    thread.entries.push(idx);
    // `:432-433` `Envir.AddObject + AddToMaped = true` —— sink 的 `spawn_monster` 已入世界。
    Some(id)
}

/// `LoadGuardList` 的放置循环（`LocalDB.cs:113-135`；解析半见 `crates/envir::guardlist`）。
///
/// 逐行 [`regen_monster_by_name`]；成功 ⇒ `guard.Dir = (byte)StrToInt(direction, 0)`
/// （`:133`）写实体 AI 态（`Entity.ai.dir`）与展示层（`DisplayTable` 的 `dir`，AOI 出现帧用），
/// 失败 ⇒ 跳过该行。取数走**同一条** `world.rng`（oracle `M2Share.RandomNumber`）。
pub fn place_guards(
    guards: &[GuardInfo],
    table: &MonsterTable,
    host: &mut MonGenHost,
    registry: &MapRegistry,
    world: &mut mir2_world::World,
    displays: &mut downlink::DisplayTable,
) -> GuardPlacementReport {
    let mut report = GuardPlacementReport {
        lines: guards.len(),
        ..GuardPlacementReport::default()
    };
    // 与 `step_mon_gen` 同口径：临时移出 `world.rng`（sink 需要 `&mut world` 落库）。
    let mut rng = std::mem::replace(&mut world.rng, RandomNumber::with_seed(0));
    {
        let mut sink = WorldMonsterSink {
            world,
            registry,
            table,
            displays,
        };
        for g in guards {
            match regen_monster_by_name(
                &mut rng,
                &mut sink,
                table,
                &mut host.state,
                &g.map_name,
                g.x,
                g.y,
                &g.mon_name,
            ) {
                Some(id) => {
                    // `:133` `guard.Dir = (byte)StrToInt(direction, 0)` —— 覆盖 `RandomByte(8)`。
                    if let Some(e) = sink.world.entities.get_mut(id) {
                        e.ai.dir = g.dir;
                    }
                    if let Some(mut d) = sink.displays.get(id).cloned() {
                        d.dir = g.dir;
                        sink.displays.set(id, d);
                    }
                    report.placed += 1;
                }
                None => report.skipped += 1,
            }
        }
    }
    world.rng = rng;
    report
}

/// `LoadGuardList` 的文件半（`LocalDB.cs:109-113`）：
/// `GetEnvirFilePath("GuardList.txt")` + `File.Exists` 门 + GBK 解码 + 整文件解析。
///
/// `Ok(None)` = 文件不存在（oracle `File.Exists` 假 ⇒ 整个方法静默返回）；
/// `Err` = 读失败/解析异常（oracle 被 `:140-143` 的 catch 吞掉只留日志 ⇒ 宿主按 WRN 登记）。
///
/// # Errors
/// 读文件失败（非 NotFound）或 `crates/envir` 解析层冒泡的 `Err`，返回人话错误。
pub fn load_guard_list_file(loader: &MapLoader) -> Result<Option<Vec<GuardInfo>>, String> {
    let path = envir_file_path(&loader.base_path, &loader.envir_dir, "", "GuardList.txt");
    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("读 {} 失败：{e}", path.display())),
    };
    mir2_envir::guardlist::load_guard_list(&decode_text(&bytes))
        .map(Some)
        .map_err(|e| format!("解析 GuardList.txt 失败：{e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mir2_data::models::MonsterInfo;
    use mir2_world::mongen::MonGenConfig;

    /// 用例级唯一临时目录的序号（照 `mongen.rs` 测试的 pid+原子计数口径）。
    static COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

    /// 守卫模板（`monsters` 表一行：卫士 race 11 = `ActorRace.Supreguard` 家族的 `GuardUnit` 链）。
    fn guard_info_row() -> MonsterInfo {
        MonsterInfo {
            name: "卫士".to_string(),
            race: 11,
            cool_eye: 0,
            level: 3,
            hp: 25,
            dc: 1,
            max_dc: 2,
            ac: 4,
            hit_point: 5,
            speed: 6,
            exp: 11,
            life_attrib: 1,
            item_list: None,
            ..MonsterInfo::default()
        }
    }

    struct Fixture {
        base: std::path::PathBuf,
        registry: MapRegistry,
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.base);
        }
    }

    /// 5×5 全零地形 + 注册表（map "0" ⇒ map_id 0）——照 `mongen.rs` 的
    /// `world_monster_sink_seeds_ai_from_spawn_request` 夹具。
    fn fixture() -> Fixture {
        let base = std::env::temp_dir().join(format!(
            "gs-guardlist-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        let mut bytes = vec![0u8; mir2_envir::HEADER_BYTES];
        bytes[0..2].copy_from_slice(&5_i16.to_le_bytes());
        bytes[2..4].copy_from_slice(&5_i16.to_le_bytes());
        bytes.resize(mir2_envir::HEADER_BYTES + 5 * 5 * mir2_envir::CELL_BYTES, 0);
        std::fs::create_dir_all(base.join("Map")).unwrap();
        std::fs::write(base.join("Map").join("0.map"), &bytes).unwrap();
        let loader = MapLoader::new(base.clone(), "Map", "Envir");
        let registry = MapRegistry::from_entries(
            loader,
            vec![mir2_envir::MapInfoEntry {
                map_name: "0".to_string(),
                map_file_name: "0".to_string(),
                desc: String::new(),
                server_index: 0,
                flag: mir2_envir::MapInfoFlag::default(),
            }],
        );
        assert_eq!(registry.id_of("0"), Some(0), "注册表必须命中 map 0");
        Fixture { base, registry }
    }

    fn guard(mon_name: &str, map_name: &str, x: i16, y: i16, dir: u8) -> GuardInfo {
        GuardInfo {
            mon_name: mon_name.to_string(),
            map_name: map_name.to_string(),
            x,
            y,
            dir,
        }
    }

    /// 世界内 race 11 守卫数（闭包捕获会与 sink 的 `&mut world` 冲突 ⇒ 独立函数）。
    fn guard_count(world: &mir2_world::World) -> usize {
        world.entities.iter().filter(|e| e.race == 11).count()
    }

    /// 判据 1（端到端逐字段）：两条合法行 ⇒ 世界内两只守卫，`name/map_id/(x,y)/dir` 逐字段
    /// 相等；合成 `MonGenEntry` 与线程登记按 oracle `:390-407` 逐字段；展示层 `dir` 同步。
    ///
    /// 红检：去掉 `place_guards` 的放置（或 `regen_monster_by_name` 返 None）⇒ 世界 0 只 ⇒ 红。
    #[test]
    fn place_guards_puts_each_guard_in_world_field_by_field() {
        let f = fixture();
        let table = MonsterTable::new(vec![guard_info_row()]);
        let mut world = mir2_world::World::with_maps(vec![(5, 5)]);
        world.clock_ms = 12_345;
        let mut host = MonGenHost::start(
            Vec::new(),
            &table,
            false,
            MonGenConfig::default(),
            &mut world.rng,
        );
        let mut displays = downlink::DisplayTable::new();
        let report = place_guards(
            &[guard("卫士", "0", 2, 2, 5), guard("卫士", "0", 3, 1, 7)],
            &table,
            &mut host,
            &f.registry,
            &mut world,
            &mut displays,
        );
        assert_eq!(
            report,
            GuardPlacementReport {
                lines: 2,
                placed: 2,
                skipped: 0
            }
        );
        // 世界内守卫逐字段（id 取自合成条目的 CertList = oracle TryAdd 登记）。
        let ids: Vec<i32> = host
            .state
            .entries
            .iter()
            .flat_map(|e| e.cert_list.iter().copied())
            .collect();
        assert_eq!(ids.len(), 2, "两只守卫各登记一条 CertList");
        let fields: Vec<(i32, &str, usize, i32, i32, u8, u8)> = ids
            .iter()
            .map(|&id| {
                let e = world.entities.get(id).expect("守卫在仓库");
                (e.id, e.name.as_str(), e.map_id, e.x, e.y, e.race, e.ai.dir)
            })
            .collect();
        assert_eq!(
            fields,
            vec![
                (ids[0], "卫士", 0, 2, 2, 11, 5),
                (ids[1], "卫士", 0, 3, 1, 11, 7),
            ],
            "name/map_id/(x,y)/race/dir 逐字段"
        );
        // 展示层 dir 与实体一致（AOI 出现帧用）。
        for (&id, dir) in ids.iter().zip([5u8, 7u8]) {
            assert_eq!(displays.get(id).expect("展示层已登记").dir, dir);
        }
        // 合成 MonGenEntry 逐字段（`WorldServer.MonGen.cs:390-404` + TryAdd 不加 ActiveCount）。
        for (i, e) in host.state.entries.iter().enumerate() {
            assert_eq!(e.mon_name, "卫士");
            assert_eq!(e.map_name, "0");
            assert_eq!(e.range, 0);
            assert_eq!(e.count, 1);
            assert_eq!(e.active_count, 0, "TryAdd 不加 ActiveCount（照搬）");
            assert_eq!(e.zen_time, 0);
            assert_eq!(e.mission_gen_rate, 0);
            assert_eq!(
                e.race, 0,
                "Race 不回填 ⇒ RegenMonsters race<=0 早退（照搬）"
            );
            assert_eq!(e.cert_list, vec![ids[i]]);
            assert_eq!(e.thread_id, 0);
        }
        // 线程列表尾追加（`:407`）。
        assert_eq!(host.state.threads[&0].entries, vec![0, 1]);
    }

    /// 判据 2（`guard.Dir` 覆盖 `RandomByte(8)`，改坏必红）：手工复算 oracle 取数序
    /// （`InitializeMonster` 补抽 1 + 构造链 4 + Dir + CoolEye），配置向取 `(drawn+1)%8`
    /// ⇒ 必与抽出的 Dir 不同；断言实体 `ai.dir` = 配置向、且随机流位置对齐。
    ///
    /// 红检：删掉 `place_guards` 里的 `e.ai.dir = g.dir` ⇒ `ai.dir` 停在 `RandomByte(8)`
    /// 的抽值（= `(配置+7)%8`）⇒ 本用例必红。
    #[test]
    fn guard_dir_overwrites_the_random_byte_draw() {
        let f = fixture();
        let table = MonsterTable::new(vec![guard_info_row()]);
        let seed = 2026;
        // 期望侧：同种子手工复算（InitializeMonster 第二次补抽 1 次（MonsterList 1 名 × limit=1）
        // + race 11 构造链 4 抽（BaseObject×2 + AnimalObject×2，GuardUnit 链无派生抽）
        // + Dir RandomByte(8) + CoolEye Random(100)；item_list 空 ⇒ 掉落不抽）。
        let mut b = RandomNumber::with_seed(seed);
        let _ = b.random_below(1);
        let _ = b.random_below(1500);
        let _ = b.random_below(2000);
        let _ = b.random_below(3000);
        let _ = b.random_below(3000);
        let drawn_dir = b.random_byte(8);
        let _ = b.random_below(100);
        let configured_dir = (drawn_dir + 1) % 8;

        let mut world = mir2_world::World::with_maps(vec![(5, 5)]);
        world.rng = RandomNumber::with_seed(seed);
        let mut host = MonGenHost::start(
            Vec::new(),
            &table,
            false,
            MonGenConfig::default(),
            &mut world.rng,
        );
        let mut displays = downlink::DisplayTable::new();
        let report = place_guards(
            &[guard("卫士", "0", 2, 2, configured_dir)],
            &table,
            &mut host,
            &f.registry,
            &mut world,
            &mut displays,
        );
        assert_eq!(report.placed, 1);
        let id = host.state.entries[0].cert_list[0];
        let e = world.entities.get(id).expect("守卫在仓库");
        assert_ne!(drawn_dir, configured_dir, "夹具自证：两向必然不同");
        assert_eq!(e.ai.dir, configured_dir, "guard.Dir 必须覆盖 RandomByte(8)");
        assert_eq!(
            displays.get(id).expect("展示层").dir,
            configured_dir,
            "展示层 dir 同步覆盖"
        );
        // 随机流位置对齐（抽数个数/次序与手工复算一致）。
        assert_eq!(world.rng.random(), b.random());
    }

    /// 判据 3（oracle null 支：未知模板名/未知地图 ⇒ 跳过且**不抽数**）：
    /// `GetMonRace` 未命中 -1 ⇒ create 的 race 门拦下；`FindMap` 未命中 ⇒ create 的
    /// 首行拦下——两支都在任何抽数之前（`WorldServer.MonGen.cs:500-503` + switch 未命中）。
    #[test]
    fn unknown_template_or_map_skips_line_without_draws() {
        let f = fixture();
        let table = MonsterTable::new(vec![guard_info_row()]);
        let mut world = mir2_world::World::with_maps(vec![(5, 5)]);
        let seed = 77;
        world.rng = RandomNumber::with_seed(seed);
        let mut host = MonGenHost::start(
            Vec::new(),
            &table,
            false,
            MonGenConfig::default(),
            &mut world.rng,
        );
        let mut displays = downlink::DisplayTable::new();
        let report = place_guards(
            &[
                guard("神秘人", "0", 1, 1, 3), // 不在 monsters 表
                guard("卫士", "9", 1, 1, 3),   // 地图不在注册表
            ],
            &table,
            &mut host,
            &f.registry,
            &mut world,
            &mut displays,
        );
        assert_eq!(
            report,
            GuardPlacementReport {
                lines: 2,
                placed: 0,
                skipped: 2
            }
        );
        assert!(host.state.entries.is_empty(), "失败行不产生合成条目");
        assert_eq!(
            world.entities.iter().filter(|e| e.race == 11).count(),
            0,
            "世界内 0 只守卫"
        );
        // 期望侧只耗 `InitializeMonster` 的 1 次补抽 ⇒ 放置半一步未抽。
        let mut b = RandomNumber::with_seed(seed);
        let _ = b.random_below(1);
        assert_eq!(world.rng.random(), b.random(), "失败支不许抽数");
    }

    /// 判据 4（合成条目不让 regen 门补刷——`Race=0` 早退照搬）：放置后跑一轮 regen 门，
    /// 守卫数量不变、随机流不耗；`StartTick` 照 `boRegened=true` 回写（R5-1 同款）。
    ///
    /// 红检：给合成条目回填真 race（"顺手修"）⇒ `RegenMonsters` 会再刷一只 ⇒ 数量断言必红。
    #[test]
    fn guard_synthetic_entry_never_respawns_via_regen_gate() {
        let f = fixture();
        let table = MonsterTable::new(vec![guard_info_row()]);
        let seed = 31;
        let mut world = mir2_world::World::with_maps(vec![(5, 5)]);
        world.rng = RandomNumber::with_seed(seed);
        let mut host = MonGenHost::start(
            Vec::new(),
            &table,
            false,
            MonGenConfig::default(),
            &mut world.rng,
        );
        let mut displays = downlink::DisplayTable::new();
        let report = place_guards(
            &[guard("卫士", "0", 2, 2, 4)],
            &table,
            &mut host,
            &f.registry,
            &mut world,
            &mut displays,
        );
        assert_eq!(report.placed, 1);
        assert_eq!(guard_count(&world), 1);

        // 期望侧：InitializeMonster 补抽 1 + 构造链 4 + Dir + CoolEye（race 11 无掉落抽）。
        let mut b = RandomNumber::with_seed(seed);
        let _ = b.random_below(1);
        let _ = b.random_below(1500);
        let _ = b.random_below(2000);
        let _ = b.random_below(3000);
        let _ = b.random_below(3000);
        let _ = b.random_byte(8);
        let _ = b.random_below(100);

        // 一轮 step（now=1000 > RegenMonstersTime=200 ⇒ 门到点；轮询选中合成条目）。
        let mut clock = || 1000;
        let mut can = |_: &str| true;
        let decisions = {
            let mut rng = std::mem::replace(&mut world.rng, RandomNumber::with_seed(0));
            let d = {
                let mut sink = crate::mongen::WorldMonsterSink {
                    world: &mut world,
                    registry: &f.registry,
                    table: &table,
                    displays: &mut displays,
                };
                host.step(1000, 0, &mut rng, &mut clock, &mut can, &mut sink, &table)
            };
            world.rng = rng;
            d
        };
        assert_eq!(decisions.len(), 1);
        assert!(decisions[0].gate_due);
        assert_eq!(decisions[0].entry_index, 0, "轮询选中守卫合成条目");
        assert!(
            decisions[0].regened,
            "genModCount(1) > ActiveCount(0) ⇒ 调 RegenMonsters（照搬）"
        );
        assert_eq!(
            guard_count(&world),
            1,
            "race=0 早退 ⇒ 不补刷（WorldServer.MonGen.cs:817-820）"
        );
        assert_eq!(
            host.state.entries[0].active_count, 0,
            "ActiveCount 保持 0（TryAdd 不加）"
        );
        assert_eq!(
            host.state.entries[0].start_tick, 1000,
            "boRegened=true ⇒ 回写"
        );
        assert_eq!(world.rng.random(), b.random(), "regen 半步未抽数");
    }

    /// 判据 5（文件半）：无文件 ⇒ `Ok(None)`（oracle `File.Exists` 门 ⇒ 不放置 ⇒ 世界 0 只）；
    /// 有文件 ⇒ GBK 解码 + 部署件行形态逐字段解析。
    #[test]
    fn load_guard_list_file_missing_is_none_and_reads_gbk() {
        let f = fixture();
        let loader = MapLoader::new(f.base.clone(), "Map", "Envir");
        assert!(
            load_guard_list_file(&loader).unwrap().is_none(),
            "无 GuardList.txt ⇒ Ok(None)（File.Exists 门）"
        );
        // 部署件第 28 行形态（`卫士` 的 GBK 字节 = CE C0 CA BF）+ CRLF。
        std::fs::create_dir_all(f.base.join("Envir")).unwrap();
        let mut body: Vec<u8> = vec![0xCE, 0xC0, 0xCA, 0xBF];
        body.extend_from_slice(b"   0        281, 661    : 5\r\n");
        body.extend_from_slice(b";comment\r\n");
        std::fs::write(f.base.join("Envir").join("GuardList.txt"), &body).unwrap();
        let list = load_guard_list_file(&loader).unwrap().expect("Some(list)");
        assert_eq!(
            list,
            vec![GuardInfo {
                mon_name: "卫士".to_string(),
                map_name: "0".to_string(),
                x: 281,
                y: 661,
                dir: 5,
            }],
            "GBK 解码 + 行形态逐字段"
        );
    }
}
