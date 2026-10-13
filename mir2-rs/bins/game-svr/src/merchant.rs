//! 商人**宿主装载层**（卡 `openmir2-m3-merchant-entity-tick`，M3⑤-实现 2 第①片）。
//!
//! 本片把 ⑤-1 的 `crates/envir` 商人记录接到世界实体，并落 owner 转交的两条必做项：
//!
//! - **O-1（FindMap 门）**：`WorldServer.Merchant.cs:8-25` 的 `SystemShare.MapMgr.FindMap(MapName)
//!   != null` —— 不满足则**丢弃该记录**（oracle `Error + RemoveAt`）。部署件上有 4 条 map 名不存在
//!   （`DB`×1 / `ZM6`×1 / `jiaren`×2）⇒ 131 → ~127。宿主侧等价物 = `MapRegistry::id_of`。
//! - **O-2（`rename_rand` 注入）**：`ScriptParsers.cs:857` 的重名 label 改名走
//!   `SystemShare.RandomNumber.GetRandomNumber(1, 200)` —— 由本层把**共享 `world.rng`** 的
//!   `get_random_number(1, 200)` 注入 `MapLoader::load_merchants`（不另开私有流）。
//!
//! ## 与 oracle 的建模差异（登记，留给本卡 tick 分片）
//! - `BaseObject` ctor 的 `RunTick = GetTickCount() + Random(1500)`（`BaseObject.cs:439`）本片
//!   **不抽**（构造抽的落点须与 tick 分片一起按 oracle 次序定；见 `crates/world::merchant` 模块头）。
//! - `ProcessRefillIndex`（`ScriptParsers.cs:838`，全局 `CurrentMerchantIndex++`）⑤-1 未随记录暴露
//!   ⇒ 本片置 0（只影响 `Merchant.Run` 的补货延时门，属 tick 分片）。
//! - `merchant_tick`（`ProcessMerchants`）本片**不驱动**（分片预算 5ms 的确定性等价物待 owner 拍板）。

use mir2_envir::MapLoader;
use mir2_shared::rng::RandomNumber;
use mir2_world::merchant::{MerchantFlags, MerchantGoods, MerchantSpawn, MerchantState};

/// 装载结果读数（O-1 的真机判据：`records` vs `kept`）。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MerchantLoadReport {
    /// `LocalDB.LoadMerchant` 入库的记录数（部署件 = 131）。
    pub records: usize,
    /// 过 `FindMap` 门后留在世界里的记录数（部署件 = 127）。
    pub kept: usize,
    /// 被 `FindMap` 门丢弃的 `(ChrName, MapName)`（部署件 4 条）。
    pub dropped_unknown_map: Vec<(String, String)>,
}

/// `MerchantRecord` + `map_id` → [`MerchantSpawn`]（逐字段；不显式命名 `crates/script` 类型，
/// 避免给宿主引入脚本引擎的直接依赖）。
#[must_use]
pub fn to_spawn(record: mir2_envir::MerchantRecord, map_id: usize) -> MerchantSpawn {
    let refill_goods = record
        .refill_goods
        .into_iter()
        .map(|g| MerchantGoods {
            item_name: g.item_name,
            count: g.count,
            refill_time: g.refill_time,
        })
        .collect();
    let f = record.flags;
    let flags = MerchantFlags {
        is_buy: f.is_buy,
        is_sell: f.is_sell,
        is_make_drug: f.is_make_drug,
        is_prices: f.is_prices,
        is_storage: f.is_storage,
        is_getback: f.is_getback,
        is_upgradenow: f.is_upgradenow,
        is_get_backupgnow: f.is_get_backupgnow,
        is_repair: f.is_repair,
        is_sup_repair: f.is_sup_repair,
        is_send_msg: f.is_send_msg,
        is_use_item_name: f.is_use_item_name,
        is_offline_msg: f.is_offline_msg,
        is_yb_deal: f.is_yb_deal,
    };
    MerchantSpawn {
        name: record.chr_name,
        x: i32::from(record.x),
        y: i32::from(record.y),
        map_id,
        state: MerchantState::new(
            record.script_name,
            record.map_name,
            record.npc_flag,
            record.appr,
            record.price_rate,
            record.castle_merchant,
            record.can_move,
            record.move_time,
            flags,
            record.item_type_list,
            refill_goods,
            0,
        ),
    }
}

/// `LocalDB.LoadMerchant`（含 ⑤-1 的脚本头装配）+ **O-1 FindMap 门** + **O-2 `rename_rand`**。
///
/// `resolve_map` = `SystemShare.MapMgr.FindMap(MapName)` 的宿主等价物
/// （生产传入 `|n| registry.id_of(n)`；单测传入合成闭包）。
///
/// 返回 `Ok(None)` 对应 oracle `File.Exists(sFileName) == false`（无 `Merchant.txt` ⇒ 无记录）。
///
/// # Errors
/// 主文件读失败 / 脚本解析抛 `LoadPanic`（见 `crates/envir` 的 `load_merchants`）。
pub fn load_merchant_spawns(
    loader: &MapLoader,
    resolve_map: impl Fn(&str) -> Option<usize>,
    rng: &mut RandomNumber,
) -> Result<(Vec<MerchantSpawn>, MerchantLoadReport), String> {
    // O-2：`SystemShare.RandomNumber.GetRandomNumber(1, 200)`（`ScriptParsers.cs:857`）。
    let mut rename_rand = |min: i32, max: i32| rng.get_random_number(min, max);
    let records = loader
        .load_merchants(&mut rename_rand)
        .map_err(|e| e.to_string())?
        .unwrap_or_default();
    let total = records.len();
    let mut spawns = Vec::with_capacity(total);
    let mut dropped_unknown_map = Vec::new();
    for record in records {
        // O-1：`FindMap(MapName) == null ⇒ Error + RemoveAt`（`WorldServer.Merchant.cs:24-31`）。
        match resolve_map(&record.map_name) {
            Some(map_id) => spawns.push(to_spawn(record, map_id)),
            None => dropped_unknown_map.push((record.chr_name, record.map_name)),
        }
    }
    let report = MerchantLoadReport {
        records: total,
        kept: spawns.len(),
        dropped_unknown_map,
    };
    Ok((spawns, report))
}

/// 把装载好的商人逐条挂图（oracle `MerchantInitialize` 的 `Initialize()` 段）。
///
/// 返回 `(挂图成功数, 挂图失败被移除的 ChrName)`。
#[must_use]
pub fn spawn_merchants(
    world: &mut mir2_world::World,
    spawns: Vec<MerchantSpawn>,
) -> (usize, Vec<String>) {
    let mut kept = 0;
    let mut failed = Vec::new();
    for spawn in spawns {
        let name = spawn.name.clone();
        if world.spawn_merchant(spawn).is_some() {
            kept += 1;
        } else {
            failed.push(name);
        }
    }
    (kept, failed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// 临时 Envir 根（`Merchant.txt` 的宿主装载输入；脚本文件缺失 ⇒ oracle `LogService.Error`
    /// 后保持头字段默认，与 ⑤-1 的分支一致）。用 `pid + 原子计数` 避免并行用例撞目录。
    fn temp_envir(name: &str, merchant_text: &str) -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU32, Ordering};
        static SEQ: AtomicU32 = AtomicU32::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("gs-merchant-{name}-{}-{n}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("Envir")).expect("建临时 Envir");
        fs::write(dir.join("Envir").join("Merchant.txt"), merchant_text).expect("写 Merchant.txt");
        dir
    }

    fn loader_for(dir: &std::path::Path) -> MapLoader {
        MapLoader::new(dir, "Map", "Envir")
    }

    // 夹具用 ASCII 名（decode_text 对无 BOM 文件按 GBK 解码，与部署件同口径；
    // 这里不断言中文名，避免把源码的 UTF-8 当成线上字节）。
    const MERCHANT_TXT: &str = "\
smith/repair 0 9 12 repairman 0 9 0\n\
fengmo/smith 0139 16 20 smith 0 27 0\n\
DB/script jiaren 1 1 dummy 0 3 0\n";

    /// O-1 正例：`FindMap` 命中 2 条、未命中 1 条（`jiaren` 被丢）⇒ `records=3, kept=2`。
    #[test]
    fn find_map_gate_drops_unknown_map() {
        let dir = temp_envir("gate", MERCHANT_TXT);
        let loader = loader_for(&dir);
        let mut rng = RandomNumber::with_seed(42);
        let (spawns, report) = load_merchant_spawns(
            &loader,
            |name| match name {
                "0" => Some(0),
                "0139" => Some(1),
                _ => None,
            },
            &mut rng,
        )
        .expect("装载不应失败");
        assert_eq!(report.records, 3, "LocalDB 入库 3 条");
        assert_eq!(report.kept, 2, "过 FindMap 门留 2 条");
        assert_eq!(
            report.dropped_unknown_map,
            vec![("dummy".to_string(), "jiaren".to_string())]
        );
        assert_eq!(spawns.len(), 2);
        assert_eq!(spawns[0].map_id, 0);
        assert_eq!(spawns[1].map_id, 1);
        assert_eq!(spawns[1].name, "smith");
    }

    /// 挂图：命中地图的商人落到 world（`spawn_merchant` 成功）。
    #[test]
    fn gated_spawns_attach_to_world() {
        let dir = temp_envir("attach", MERCHANT_TXT);
        let loader = loader_for(&dir);
        let mut rng = RandomNumber::with_seed(42);
        let (spawns, _) = load_merchant_spawns(
            &loader,
            |name| match name {
                "0" | "0139" => Some(0),
                _ => None,
            },
            &mut rng,
        )
        .expect("装载不应失败");
        let mut world = mir2_world::World::new(64, 64);
        let (kept, failed) = spawn_merchants(&mut world, spawns);
        assert_eq!(kept, 2);
        assert!(failed.is_empty());
        assert_eq!(world.merchant_ids_sorted().len(), 2);
    }

    /// 无 `Merchant.txt` ⇒ oracle `File.Exists == false` ⇒ 无记录、无差异。
    #[test]
    fn missing_merchant_txt_is_empty() {
        let dir = temp_envir("none", "\n");
        let _ = fs::remove_file(dir.join("Envir").join("Merchant.txt"));
        let loader = loader_for(&dir);
        let mut rng = RandomNumber::with_seed(42);
        let (spawns, report) = load_merchant_spawns(&loader, |_| Some(0), &mut rng).unwrap();
        assert!(spawns.is_empty());
        assert_eq!(report.records, 0);
    }

    /// **真机部署件向量**：`E:/MirServer/M2GameSvr` 的 `Envir/Merchant.txt`（131 条）+
    /// `Envir/MapInfo.txt`+`Map/*.map` 注册表。
    ///
    /// ⚠ **owner `26efc51d` 的 O-1 前提经本片实测不成立**（按 `FindMap` 真口径复核）：owner 说
    /// 4 条记录的 map 名（`DB`×1 / `ZM6`×1 / `jiaren`×2）在 MapInfo.txt 与 Map/*.map 里都没有
    /// ⇒ 应丢到 127。实际三段都在 MapInfo.txt 里（`:10 [DB|DM011 新手地图]`、
    /// `:21 [ZM6|D5071 ...]`、`:1 [jiaren|0150 ...]`，逻辑名 = `|` 前），且
    /// `Map/DM011.map`/`Map/D5071.map`/`Map/0150.map` 都存在 ⇒ `AddMapInfo`
    /// （`MapManager.cs:63-99`）落 `_mapList[逻辑名]`；`FindMap`（`MapManager.cs:146`）按
    /// **逻辑名**查表（不是 `.map` 文件名）⇒ 4 条都命中。
    ///
    /// ⇒ 正确读数是 **131 → 131（0 条被门丢弃）**。owner 疑似把**逻辑名**当成 `Map/*.map` 的
    /// **文件名**去找（`DB.map`/`ZM6.map`/`jiaren.map` 确实不存在，但 oracle 不按文件名找）。
    /// 门仍按 oracle 实现（`resolve_map` 未命中即丢），本部署只是恰好没触发；合成用例
    /// [`find_map_gate_drops_unknown_map`] 才是门的可用判据。
    ///
    /// 需真实部署件与 `MIR2_BASE_DIR`（缺省 `E:/MirServer/M2GameSvr`）；用
    /// `cargo test -p game-svr --lib real_deployment -- --ignored --nocapture` 跑。
    #[test]
    #[ignore = "真机部署件向量：需 E:/MirServer/M2GameSvr（证据/owner 跑）"]
    fn real_deployment_find_map_gate() {
        let base =
            std::env::var("MIR2_BASE_DIR").unwrap_or_else(|_| "E:/MirServer/M2GameSvr".into());
        let loader = MapLoader::new(&base, "Map", "Envir");
        let registry = loader.load_registry().expect("载入真地图注册表");
        let mut rng = RandomNumber::with_seed(42);
        let (spawns, report) = load_merchant_spawns(&loader, |n| registry.id_of(n), &mut rng)
            .expect("载入真 Merchant.txt");
        println!(
            "MERCHANT_VECTOR records={} kept={} dropped={:?}",
            report.records, report.kept, report.dropped_unknown_map
        );
        assert_eq!(report.kept, spawns.len());
        assert_eq!(report.records, 131, "部署件 Merchant.txt 入库 131 条");
        assert_eq!(
            report.kept, 131,
            "FindMap 门在本部署无命中：131 全保留（见函数头实测）"
        );
        assert!(report.dropped_unknown_map.is_empty());
    }
}
