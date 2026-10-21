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
//! - **NPC 脚本装载（T-137 已补）**：oracle `NpCinitialize` 挂图成功后 `normNpc.LoadNPCScript()`
//!   （`NormNpc.cs:1590-1601`，`IsQuest=true` ⇒ `FilePath="Npc_def"`、脚本名 = `ChrName-MapName`，
//!   部署件 `Npc_Def/{比奇国王-0122,沙巴克城堡管理-0150,老人-3}.txt`）。本模块的
//!   [`place_fixed_npcs_with_scripts`] 在**倒序挂图**的同一趟里按 oracle 次序装载
//!   `Npc_def/<ChrName>-<MapName>.txt`（`boFlag=false`、`rename_rand` = 世界同一条 rng），
//!   并把 `Merchant.ScriptList` 塞进与商人共用的 [`crate::npc::MerchantScriptBook`]
//!   （`ActorId` = 世界实体 id）⇒ 点击 `@main` / `CM_MERCHANTDLGSELECT` 的 `GotoLable` 有脚本可跑。
//!   残余：`CastleOfficial` 本体（T-40/T-140）、`@Login`/`QFunction` 入口、真机窗口判据，见
//!   `tests/parity/whitelist.md` T-137。
//! - **NPC 的 `Run()`/`ProcessNpcs` 已建模**（**世界 tick 扩容首批 T3**，卡
//!   `openmir2-world-tick-expansion`）：`crates/world/src/npc.rs` 的 `World::npc_tick`/`npc_run`（固定 NPC
//!   `GuildOfficial.Run` / `Merchant.Run`），由宿主 `sim_loop` 起世界时 `enable_npc_tick()`。仍差：
//!   `NpcLimit` 墙钟预算（`whitelist.md` T-64）与 `RM_TURN`/`RM_HIT` 帧面（B-4）。

use std::path::Path;

use mir2_envir::npcs::NpcInfo;
use mir2_envir::paths::envir_file_path;
use mir2_envir::{decode_text, MapLoader};
use mir2_script::{LocalFs, ScriptParsers};
use mir2_shared::rng::RandomNumber;
use mir2_world::npc::{NpcSpawn, NpcState};

use crate::downlink::{npc_display, DisplayTable};
use crate::npc::{MerchantNpc, MerchantScriptBook, NpcDef, NpcLoadFailure};

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
///
/// 不保留 `(actor_id, ChrName, MapName)` 记录；需要给 `Npc_def` 脚本装载用的调用点请走
/// [`place_npcs_recording`]（本函数是它的丢弃包装）。
#[must_use]
pub fn place_npcs(world: &mut mir2_world::World, spawns: Vec<NpcSpawn>) -> (usize, Vec<String>) {
    let (kept, failed, _) = place_npcs_recording(
        world,
        spawns.into_iter().map(|s| (s, String::new())).collect(),
    );
    (kept, failed)
}

/// 挂图成功后留在世界的固定 NPC（`NpCinitialize` 的 `LoadNPCScript` 输入）。
///
/// `actor_id` = 世界实体 id（宿主 `ActorId`，脚本表键）；`map_name` 来自 `Npcs.txt` 的
/// `MapName` 列（= oracle `NormNpc.MapName`，脚本名后缀），`x`/`y` = `Npcs.txt` 坐标
/// （`GotoLable` 的 `CurrX/CurrY` 与 `$MAPNAME` 同源）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlacedNpc {
    pub actor_id: i32,
    pub chr_name: String,
    pub map_name: String,
    pub x: i32,
    pub y: i32,
}

/// [`place_npcs`] 的记录变体：返回 `(kept, failed, placed)`。
///
/// `spawns` 每项 = `(NpcSpawn, MapName)`（`MapName` 只用于记录，不参与建实体/挂图）；
/// `placed` 的**顺序 = 倒序挂图的处理序** = oracle `NpCinitialize` 的
/// `for (i = Count-1; i >= 0; i--)`，也即 `LoadNPCScript` 的调用序（脚本装载必须按此序抽数）。
#[must_use]
pub fn place_npcs_recording(
    world: &mut mir2_world::World,
    spawns: Vec<NpcSpawnWithMap>,
) -> (usize, Vec<String>, Vec<PlacedNpc>) {
    // ① 正序建实体（ctor 抽数 = LocalDB.LoadNpcs 文件序，走 world.rng 同一条流）。
    let pending: Vec<(Option<i32>, String, String, i32, i32)> = spawns
        .into_iter()
        .map(|(spawn, map_name)| {
            let name = spawn.name.clone();
            let (x, y) = (spawn.x, spawn.y);
            (world.insert_npc(spawn), name, map_name, x, y)
        })
        .collect();
    // ② 倒序挂图（NpCinitialize 的倒序 for）；成功的按同一序记进 `placed`（= 脚本装载序）。
    let mut kept = 0;
    let mut failed = Vec::new();
    let mut placed = Vec::new();
    for (id, name, map_name, x, y) in pending.into_iter().rev() {
        match id {
            Some(id) if world.attach_npc_to_map(id) => {
                kept += 1;
                placed.push(PlacedNpc {
                    actor_id: id,
                    chr_name: name,
                    map_name,
                    x,
                    y,
                });
            }
            _ => failed.push(name),
        }
    }
    (kept, failed, placed)
}

/// 一条待放置的固定 NPC：`(NpcSpawn, MapName)`（`MapName` 只用于脚本名/记录）。
pub type NpcSpawnWithMap = (NpcSpawn, String);

/// [`build_npc_spawns`] 的记录变体：`MapName` 与 spawn 逐项对齐（供 [`place_npcs_recording`]）。
#[must_use]
pub fn build_npc_spawns_recording(
    npcs: &[NpcInfo],
    resolve_map: impl Fn(&str) -> Option<usize>,
) -> (Vec<NpcSpawnWithMap>, Vec<(String, String)>) {
    let mut spawns = Vec::with_capacity(npcs.len());
    let mut dropped = Vec::new();
    for info in npcs {
        match resolve_map(&info.map_name) {
            Some(map_id) => spawns.push((to_spawn(info, map_id), info.map_name.clone())),
            None => dropped.push((info.chr_name.clone(), info.map_name.clone())),
        }
    }
    (spawns, dropped)
}

/// `NpCinitialize` 的第三段（`WorldServer.Merchant.cs:54` → `NormNpc.LoadNPCScript`，
/// `NormNpc.cs:1590-1601`）：对**挂图成功**的固定 NPC 按 `placed` 的序（= 倒序 for）
/// 装载 `Npc_def/<ChrName>-<MapName>.txt`（`boFlag=false`），装进与商人共用的脚本表。
///
/// - `IsQuest=true`（`NormNpc` ctor）⇒ `FilePath="Npc_def"`、脚本名 = `ChrName + '-' + MapName`；
/// - `File.Exists` 假 ⇒ oracle `LogService.Error("Script file not found: ...")` 且 `ScriptList` 空
///   ⇒ 本函数 `loaded=false` 并把 `(ChrName, stem)` 记进 `missing`（不静默吞）；
/// - 解析抛异常（`LoadPanic`）⇒ 记进 `failures`（oracle 异常穿透 `NpCinitialize`；本宿主登记后继续，
///   与 `NpcHost::load` 同口径）；
/// - `rename_rand` 必须是**世界同一条** rng（oracle `SystemShare.RandomNumber`）：调用方传入。
pub fn load_npc_def_scripts(
    envir_root: &Path,
    placed: &[PlacedNpc],
    book: &mut MerchantScriptBook,
    rename_rand: &mut dyn FnMut(i32, i32) -> i32,
    failures: &mut Vec<NpcLoadFailure>,
    missing: &mut Vec<(String, String)>,
) {
    let parsers = ScriptParsers::new();
    let fs = LocalFs;
    for npc in placed {
        // `NormNpc.LoadNPCScript`：`sScriptName = ChrName + '-' + MapName`（`NormNpc.cs:1595`）。
        let stem = format!("{}-{}", npc.chr_name, npc.map_name);
        let (scripts, loaded) =
            match parsers.load_script_file(&fs, envir_root, "Npc_def", &stem, false, rename_rand) {
                Ok(Some(outcome)) => (outcome.scripts, true),
                Ok(None) => {
                    missing.push((npc.chr_name.clone(), stem.clone()));
                    (Vec::new(), false)
                }
                Err(panic) => {
                    failures.push(NpcLoadFailure {
                        chr_name: npc.chr_name.clone(),
                        file_path: "Npc_def".to_string(),
                        script_name: stem.clone(),
                        reason: panic.to_string(),
                    });
                    (Vec::new(), false)
                }
            };
        book.insert(MerchantNpc {
            def: NpcDef {
                chr_name: npc.chr_name.clone(),
                // `LoadNPCScript` 的 `FilePath = "Npc_def"`（`NormNpc.cs:1593`）——脚本动作按它
                // 拼 `CHECKNAMELIST` 一类相对路径。
                file_path: "Npc_def".to_string(),
                script_name: Some(stem),
                // `NormNpc` ctor `IsHide = false`（`NormNpc.cs:45`）。
                is_hide: false,
                // `NormNpc` ctor `IsQuest = true`（`NormNpc.cs:48`）。
                is_quest: true,
                scripts,
                loaded,
                actor_id: npc.actor_id,
            },
            // `Merchant` ctor `PriceRate = 100`（`Merchant.cs:86`）；`Npcs.txt` 无该列。
            price_rate: 100,
            // `NormNpc.Initialize` 的 `CastleMgr.InCastleWarArea(this)`：城堡家族（⑧）未落地 ⇒ None。
            castle: None,
            map_name: npc.map_name.clone(),
            curr_x: npc.x,
            curr_y: npc.y,
        });
    }
}

/// [`place_fixed_npcs_with_scripts`] 的读数（日志/判据用）。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FixedNpcLoadReport {
    /// `Npcs.txt` 解析出的记录数。
    pub lines: usize,
    /// 过 `FindMap` 门 + 挂图成功后留在世界的 NPC 数。
    pub placed: usize,
    /// 被 `FindMap` 门丢弃的 `(ChrName, MapName)`。
    pub dropped_unknown_map: Vec<(String, String)>,
    /// 挂图失败被移除的 `ChrName`。
    pub dropped_add_to_map: Vec<String>,
    /// 留在世界的 NPC（挂图倒序 = `LoadNPCScript` 调用序）。
    pub placed_npcs: Vec<PlacedNpc>,
    /// `Npc_def` 脚本解析抛异常（`LoadPanic`，oracle 异常穿透）。
    pub script_failures: Vec<NpcLoadFailure>,
    /// 脚本文件不存在（`File.Exists` 假，oracle `LogService.Error`），`(ChrName, stem)`。
    pub script_missing: Vec<(String, String)>,
}

/// 固定 NPC 的完整装载链（`NpCinitialize`）：`FindMap` 门 → 正序建实体 → 倒序挂图 →
/// 倒序 `LoadNPCScript`。
///
/// `rename_rand` 取 `world.rng`（取出、装载、原样放回），与 oracle「世界同一条 `RandomNumber`」一致。
#[must_use]
pub fn place_fixed_npcs_with_scripts(
    world: &mut mir2_world::World,
    npcs: &[NpcInfo],
    resolve_map: impl Fn(&str) -> Option<usize>,
    envir_root: &Path,
    book: &mut MerchantScriptBook,
    displays: &mut DisplayTable,
) -> FixedNpcLoadReport {
    let (spawns, dropped_unknown_map) = build_npc_spawns_recording(npcs, resolve_map);
    let (placed, dropped_add_to_map, placed_npcs) = place_npcs_recording(world, spawns);
    // **卡 `openmir2-npc-display-table`（T-137 残余）**：挂图成功的固定 NPC 必须登记展示表——
    // `drain_aoi_downlink` 只对 `DisplayTable` 命中的 actor 出 `SM_TURN`/`SM_FEATURECHANGED`
    // （`lib.rs` 的 `displays.get` 守卫），未登记 ⇒ 客户端看不见也点不到。
    // 这里只登记 `placed_npcs`（= 挂图成功且未被 `RemoveAt` 的），字段口径见
    // [`npc_display`]（oracle `NormNpc`：Light=2、RaceImg=50、Appr 来自行）。
    for npc in &placed_npcs {
        if let Some(e) = world.entities.get_mut(npc.actor_id) {
            // **T-154 Minor-2**：oracle `BaseObject()` ctor `Dir = 4`（`BaseObject.cs:378`），
            // `NormNpc`/`Merchant` ctor 不覆写 ⇒ 首个 `TurnTo` 之前的出现帧方向恒 4。
            // `Entity::new` 的 `ai.dir` 缺省 0（`crates/world` 不在本卡写集）⇒ 宿主在此补齐，
            // 再投影进展示表（`d.dir = e.ai.dir`）。
            e.ai.dir = crate::downlink::NPC_INITIAL_DIR;
            displays.set(
                npc.actor_id,
                npc_display(e.ai.dir, e.race_img, e.appr, &npc.chr_name),
            );
        }
    }
    let mut script_failures = Vec::new();
    let mut script_missing = Vec::new();
    if !placed_npcs.is_empty() {
        let mut rng = std::mem::replace(&mut world.rng, RandomNumber::with_seed(0));
        {
            let mut rename_rand = |min: i32, max: i32| rng.get_random_number(min, max);
            load_npc_def_scripts(
                envir_root,
                &placed_npcs,
                book,
                &mut rename_rand,
                &mut script_failures,
                &mut script_missing,
            );
        }
        world.rng = rng;
    }
    FixedNpcLoadReport {
        lines: npcs.len(),
        placed,
        dropped_unknown_map,
        dropped_add_to_map,
        placed_npcs,
        script_failures,
        script_missing,
    }
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

    /// 判据（装载半，T-137）：`place_fixed_npcs_with_scripts` 按**倒序挂图序**（= `NpCinitialize`
    /// 的倒序 for）装载 `Npc_def/<ChrName>-<MapName>.txt`，`loaded`/`file_path`/`is_quest` 落表；
    /// 缺文件记 `script_missing`（oracle `LogService.Error`，不静默吞）。
    ///
    /// 红检：把脚本名拼接 `{ChrName}-{MapName}` 改掉 ⇒ 两个文件都 `File.Exists` 假 ⇒
    /// `report.script_missing.len()` 变 2 且 `entry.def.loaded` 假 ⇒ 红。
    #[test]
    fn place_fixed_npcs_with_scripts_loads_npc_def_into_book() {
        let f = fixture();
        let npcs = vec![
            info("比奇国王", NpcType::GuildOfficial, "0122", 29, 32, 0, 8),
            info("老人", NpcType::Merchant, "0", 3, 3, 0, 1),
        ];
        std::fs::create_dir_all(f.base.join("Envir").join("Npc_def")).unwrap();
        std::fs::write(
            f.base
                .join("Envir")
                .join("Npc_def")
                .join("比奇国王-0122.txt"),
            encoding_rs::GBK
                .encode("[@main]\n国王菜单 <申请/@apply>\n")
                .0,
        )
        .unwrap();
        let mut world = mir2_world::World::with_maps(vec![(64, 64), (64, 64)]);
        let mut book = MerchantScriptBook::new(30);
        let mut displays = DisplayTable::new();
        let report = place_fixed_npcs_with_scripts(
            &mut world,
            &npcs,
            |n| f.registry.id_of(n),
            &f.base.join("Envir"),
            &mut book,
            &mut displays,
        );
        assert_eq!(report.placed, 2);
        // 倒序 = `NpCinitialize` 的 `for (i = Count-1; i >= 0; i--)` = `LoadNPCScript` 调用序。
        assert_eq!(
            report
                .placed_npcs
                .iter()
                .map(|p| p.chr_name.as_str())
                .collect::<Vec<_>>(),
            vec!["老人", "比奇国王"]
        );
        // 老人无脚本文件 ⇒ `script_missing`（oracle `LogService.Error("Script file not found")`）。
        assert_eq!(report.script_missing.len(), 1);
        assert_eq!(report.script_missing[0].0, "老人");
        assert!(report.script_failures.is_empty());
        let king = report
            .placed_npcs
            .iter()
            .find(|p| p.chr_name == "比奇国王")
            .unwrap();
        let entry = book.get(king.actor_id).expect("脚本表按 ActorId 有国王");
        assert!(entry.def.loaded);
        assert_eq!(entry.def.file_path, "Npc_def");
        assert!(entry.def.is_quest && !entry.def.is_hide);
        assert_eq!(entry.map_name, "0122");
        assert_eq!((entry.curr_x, entry.curr_y), (29, 32));
        assert!(!entry.def.scripts.is_empty());
        let elder = report
            .placed_npcs
            .iter()
            .find(|p| p.chr_name == "老人")
            .unwrap();
        let elder_entry = book.get(elder.actor_id).expect("缺文件也建表项");
        assert!(!elder_entry.def.loaded, "`File.Exists` 假 ⇒ loaded=false");
        assert!(elder_entry.def.scripts.is_empty());
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

        // **T-137**：部署件三份 `Npc_Def/<ChrName>-<MapName>.txt` 必须全部装载（目录名大小写
        // 不敏感；脚本名拼接 = `ChrName + '-' + MapName`）。这条只读部署件、不启服务。
        let placed: Vec<PlacedNpc> = npcs
            .iter()
            .enumerate()
            .map(|(i, n)| PlacedNpc {
                actor_id: i32::try_from(i + 1).unwrap(),
                chr_name: n.chr_name.clone(),
                map_name: n.map_name.clone(),
                x: i32::from(n.x),
                y: i32::from(n.y),
            })
            .collect();
        let envir_root = loader.base_path.join(&loader.envir_dir);
        let mut book = MerchantScriptBook::new(30);
        let mut failures = Vec::new();
        let mut missing = Vec::new();
        let mut world = mir2_world::World::with_maps(vec![(64, 64)]);
        let mut rng = std::mem::replace(&mut world.rng, RandomNumber::with_seed(1));
        {
            let mut rename_rand = |min: i32, max: i32| rng.get_random_number(min, max);
            load_npc_def_scripts(
                &envir_root,
                &placed,
                &mut book,
                &mut rename_rand,
                &mut failures,
                &mut missing,
            );
        }
        world.rng = rng;
        assert!(
            failures.is_empty() && missing.is_empty(),
            "部署件 Npc_Def 脚本必须全部装载：failures={failures:?} missing={missing:?}"
        );
        for npc in &placed {
            let entry = book.get(npc.actor_id).expect("脚本表按 ActorId 有条目");
            assert!(entry.def.loaded, "`{}` 的脚本必须存在", npc.chr_name);
            assert_eq!(entry.def.file_path, "Npc_def");
            assert!(!entry.def.scripts.is_empty());
            println!(
                "NPC_DEF_VECTOR chr={} map={} scripts={} records={}",
                npc.chr_name,
                npc.map_name,
                entry.def.scripts.len(),
                entry
                    .def
                    .scripts
                    .iter()
                    .map(|s| s.record_list.len())
                    .sum::<usize>()
            );
        }
        // `比奇国王-0122` 的 `@main` 必须在说词表里（`GetValidStr3` 的菜单文本）。
        let king = book.get(1).expect("国王条目");
        let king_labels: Vec<&str> = king
            .def
            .scripts
            .iter()
            .flat_map(|s| s.record_list.iter().map(|r| r.s_label.as_str()))
            .collect();
        assert!(
            king_labels.iter().any(|l| l.eq_ignore_ascii_case("@main")),
            "国王脚本必须有 @main：{king_labels:?}"
        );
    }

    /// **卡 `openmir2-npc-display-table` 判据（数量闭合）**：登记后「实体内固定 NPC 数 N ==
    /// 展示表 N」，且逐条 `Light=2` / `Feature=MakeMonsterFeature(50,0,Appr)` /
    /// `Dir=Entity.ai.dir`。合成夹具（不连部署）。
    ///
    /// 红检：删 `place_fixed_npcs_with_scripts` 里的登记循环 ⇒ `displays.len() == 0` ⇒ 必红；
    /// 把 `npc_display` 的 `Light` 改成 0（或 Feature 改成 0）⇒ 逐字段断言必红。
    #[test]
    fn place_fixed_npcs_registers_display_for_every_placed_npc() {
        let f = fixture();
        let npcs = vec![
            info("比奇国王", NpcType::GuildOfficial, "0122", 29, 32, 0, 8),
            info("老人", NpcType::Merchant, "0", 3, 3, 0, 1),
            info(
                "沙巴克城堡管理",
                NpcType::CastleOfficial,
                "0122",
                7,
                16,
                0,
                8,
            ),
        ];
        let mut world = mir2_world::World::with_maps(vec![(64, 64), (64, 64)]);
        let mut book = MerchantScriptBook::new(30);
        let mut displays = DisplayTable::new();
        let report = place_fixed_npcs_with_scripts(
            &mut world,
            &npcs,
            |n| f.registry.id_of(n),
            &f.base.join("Envir"),
            &mut book,
            &mut displays,
        );
        assert_eq!(report.placed, 3);
        assert_eq!(
            world.npc_ids_sorted().len(),
            displays.len(),
            "实体内固定 NPC 数必须等于展示表项数（数量闭合）"
        );
        assert_eq!(displays.len(), 3);
        for npc in &report.placed_npcs {
            let e = world.entities.get(npc.actor_id).expect("实体在");
            let d = displays
                .get(npc.actor_id)
                .unwrap_or_else(|| panic!("{} 未登记展示表", npc.chr_name));
            assert_eq!(d.dir, e.ai.dir, "Dir = Entity.ai.dir");
            assert_eq!(
                e.ai.dir,
                crate::downlink::NPC_INITIAL_DIR,
                "T-154 Minor-2：oracle BaseObject ctor Dir = 4（首个 TurnTo 之前）"
            );
            assert_eq!(e.ai.dir, 4, "BaseObject.cs:378 的 ctor 值就是 4");
            assert_eq!(d.light, 2, "NormNpc ctor Light = 2（NormNpc.cs:43）");
            assert_eq!(
                d.feature,
                crate::feature::make_monster_feature(50, 0, e.appr),
                "Feature = MakeMonsterFeature(RaceImg=50, 0, Appr={})",
                e.appr
            );
            assert_eq!(d.feature_ex, 0);
            assert_eq!(d.status, 0);
            assert_eq!(d.name_color, 255);
        }
        // 反证：展示表不是按空位/玩家登记（每项 key 都是真实 NPC 实体 id）。
        let ids: Vec<i32> = world.npc_ids_sorted();
        for id in ids {
            assert!(displays.get(id).is_some(), "NPC 实体 {id} 必须有展示项");
        }
    }
}
