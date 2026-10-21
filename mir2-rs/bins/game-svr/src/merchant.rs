//! 商人**宿主装载层**（卡 `openmir2-m3-merchant-entity-tick`，M3⑤-实现 2 第①/②片）。
//!
//! 本层把 ⑤-1 的 `crates/envir` 商人记录接到世界实体，并落 owner 转交的必做项：
//!
//! - **O-1（FindMap 门）**：`WorldServer.Merchant.cs:8-25` 的 `SystemShare.MapMgr.FindMap(MapName)
//!   != null` —— 不满足则**丢弃该记录**（oracle `Error + RemoveAt`）。部署件读数（两 reviewer +
//!   真机向量）：**131 → 131（门不触发，`dropped=[]`）** —— `FindMap`（`MapManager.cs:146`）按
//!   **逻辑名**（`|` 前）查表，不看 `.map` 文件名，而 `MapInfo.txt:1/:10/:21` 三段俱在。宿主侧
//!   等价物 = `MapRegistry::id_of`（合成用例 `find_map_gate_drops_unknown_map` 才是门的牙齿）。
//! - **O-2（`rename_rand` 注入）**：`ScriptParsers.cs:857` 的重名 label 改名走
//!   `SystemShare.RandomNumber.GetRandomNumber(1, 200)` —— 由本层把**共享 `world.rng`** 的
//!   `get_random_number(1, 200)` 注入 `MapLoader::load_merchants`（不另开私有流；站点经
//!   `crates/world::merchant::merchant_rename_rand` 归 `merchant` 组）。
//!
//! ## 与 oracle 的建模差异（登记）
//! - `ProcessRefillIndex`（`ScriptParsers.cs:838`，全局 `CurrentMerchantIndex++`）⑤-1 未随记录暴露
//!   ⇒ 置 0（只影响 `Merchant.Run` 的补货延时门，见 `crates/world::merchant`）。
//! - 次序：oracle `LocalDB.LoadMerchant` 正序构造抽、`MerchantList` 正序（倒序 `Initialize`
//!   **不**重排它）、`MerchantInitialize` 只把 `AddToMap` 的 `obj_list` 追加序倒过来。⇒ 本层
//!   **正序** [`mir2_world::World::merchant_ctor_run_delay`] + **正序**
//!   [`mir2_world::World::insert_merchant`] 建实体（槽位序 = 文件序 = tick 序），仅
//!   **倒序** [`mir2_world::World::attach_merchant_to_map`] 落格（`obj_list` 反序）；
//!   见 [`spawn_merchants`]（reviewer-5 `0f220cac` F-1 返工）。
//! - `is_hide` 当前恒 `false` 且不可注入；oracle `MerchantInitialize` 在 `LoadMerchantScript` 之前
//!   ⇒ 此刻确为默认 `false`，1:1 成立（详见 `crates/world::merchant` 模块头）。

use mir2_envir::MapLoader;
use mir2_script::ScriptInfo;
use mir2_shared::rng::RandomNumber;
use mir2_world::merchant::{MerchantFlags, MerchantGoods, MerchantSpawn, MerchantState};

use crate::downlink::{npc_display, DisplayTable};
use crate::npc::{MerchantNpc, MerchantScriptBook, NpcDef};

/// 装载结果读数（O-1 的真机判据：`records` vs `kept`）。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MerchantLoadReport {
    /// `LocalDB.LoadMerchant` 入库的记录数（部署件 = 131）。
    pub records: usize,
    /// 过 `FindMap` 门后留在世界里的记录数（部署件 = 131，门不触发）。
    pub kept: usize,
    /// 被 `FindMap` 门丢弃的 `(ChrName, MapName)`（部署件 = 空；合成用例才触发门）。
    pub dropped_unknown_map: Vec<(String, String)>,
}

/// 与 [`MerchantSpawn`] 下标对齐的脚本种子（`Merchant.ScriptList` + 说词变量所需字段）。
///
/// 来自 [`mir2_envir::LoadedMerchant`] 的**同一次**解析（禁止宿主二次解析：重名 label 会
/// 二次抽 `rename_rand`，见 `crates/envir::LoadedMerchant` 的文档）。
#[derive(Debug)]
pub struct MerchantScriptSeed {
    pub chr_name: String,
    pub script_name: String,
    pub map_name: String,
    pub x: i16,
    pub y: i16,
    pub price_rate: i32,
    /// `Merchant.ScriptList`（`Merchant.LoadMerchantScript`，`Merchant.cs:1089-1095`）。
    pub scripts: Vec<ScriptInfo>,
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
            // `Goods.RefillTick` 由脚本解析置 0（`ScriptParsers.cs:965`）。
            refill_tick: 0,
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

/// `[Setup] ClearExpireUpgradeWeaponDays` → 每个商人的 [`MerchantState::clear_expire_upgrade_days`]。
///
/// oracle 的 `ClearExpreUpgradeListData`（`Merchant.cs:1076-1088`）在 `Merchant.Run` 的 10min
/// 门里按该配置删过期在途升级（部署 3 天；`GameSvrConf.cs:1423` 缺省 8）。
pub fn set_upgrade_clear_days(world: &mut mir2_world::World, days: u8) {
    for id in world.merchant_ids_sorted() {
        if let Some(st) = world.entities.get_mut(id).and_then(|e| e.merchant.as_mut()) {
            st.clear_expire_upgrade_days = days;
        }
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
) -> Result<
    (
        Vec<MerchantSpawn>,
        MerchantLoadReport,
        Vec<MerchantScriptSeed>,
    ),
    String,
> {
    // O-2：`SystemShare.RandomNumber.GetRandomNumber(1, 200)`（`ScriptParsers.cs:857`）。
    // 经 `crates/world::merchant::merchant_rename_rand` 转发 ⇒ `#[track_caller]` 记到的站点是
    // `crates/world/src/merchant.rs`（`merchant` 组），**不**在宿主产生未归类站点。
    let mut rename_rand =
        |min: i32, max: i32| mir2_world::merchant::merchant_rename_rand(rng, min, max);
    let records = loader
        .load_merchants_loaded(&mut rename_rand)
        .map_err(|e| e.to_string())?
        .unwrap_or_default();
    let total = records.len();
    let mut spawns = Vec::with_capacity(total);
    let mut seeds = Vec::with_capacity(total);
    let mut dropped_unknown_map = Vec::new();
    for loaded in records {
        let record = loaded.record;
        // O-1：`FindMap(MapName) == null ⇒ Error + RemoveAt`（`WorldServer.Merchant.cs:24-31`）。
        match resolve_map(&record.map_name) {
            Some(map_id) => {
                seeds.push(MerchantScriptSeed {
                    chr_name: record.chr_name.clone(),
                    script_name: record.script_name.clone(),
                    map_name: record.map_name.clone(),
                    x: record.x,
                    y: record.y,
                    price_rate: record.price_rate,
                    scripts: loaded.scripts,
                });
                spawns.push(to_spawn(record, map_id));
            }
            None => dropped_unknown_map.push((record.chr_name, record.map_name)),
        }
    }
    let report = MerchantLoadReport {
        records: total,
        kept: spawns.len(),
        dropped_unknown_map,
    };
    Ok((spawns, report, seeds))
}

/// 把装载好的商人逐条挂图，**按 oracle 的两个独立次序**（owner `b311a876b1` 裁决 +
/// reviewer-5 `0f220cac` F-1 返工）：
///
/// 1. `LocalDB.LoadMerchant` 是**正序**逐条 `new Merchant()`（ctor 抽 `RunTick`，
///    `BaseObject.cs:439`）⇒ 先正序 [`mir2_world::World::merchant_ctor_run_delay`] 把每条构造抽
///    落定（取数次序 = 文件行序）；
/// 2. `MerchantList` 由 `AddMerchant` **正序**追加，`MerchantInitialize`
///    （`WorldServer.Merchant.cs:8`）的 `for (i = Count-1; i >= 0; i--)` **不重排它**，
///    `ProcessMerchants`（`:68`）又从 `MerchantPosition` 起**正序**遍历 ⇒ **实体槽位序
///    （[`mir2_world::World::merchant_ids_sorted`] / `merchant_tick` 的 pass 内次序）必须是文件正序**；
/// 3. 倒序只作用于 `Initialize()` 里 `AddToMap` 对 map cell `obj_list` 的**追加次序**
///    ⇒ **只有挂图这一步倒序**。
///
/// ⇒ 先**正序** [`mir2_world::World::insert_merchant`] 建实体（槽位序 = 文件序）、再**倒序**
/// [`mir2_world::World::attach_merchant_to_map`] 落格（`obj_list` 追加 = 反序）。两个次序解耦，
/// **不可**把整个 spawn 循环一起 `rev()`（那会把 tick 次序也反过来；reviewer-5 F-1 自复现过）。
///
/// 返回 `(挂图成功数, 挂图失败被移除的 ChrName)`。
#[must_use]
pub fn spawn_merchants(
    world: &mut mir2_world::World,
    spawns: Vec<MerchantSpawn>,
) -> (usize, Vec<String>) {
    let mut displays = DisplayTable::new();
    let (kept, failed, _book) =
        spawn_merchants_with_scripts(world, spawns, Vec::new(), &mut displays);
    (kept, failed)
}

/// [`spawn_merchants`] + **商人脚本表**（`Merchant.ScriptList`，按世界实体 id 建索引）。
///
/// `seeds` 与 `spawns` **下标对齐**（来自 [`load_merchant_spawns`]）；空 `seeds` = 不建表
/// （旧调用方/单测）。只有**挂图成功**的商人进入脚本表——oracle `MerchantInitialize` 在
/// `AddtoMap`/`FindMap` 失败时 `RemoveAt`，不会执行 `LoadMerchantScript`（`WorldServer.Merchant.cs:8-31`）。
///
/// 次序约束（reviewer-5 `0f220cac` F-1 + owner `b311a876b1`）不变：正序建实体、倒序挂图。
#[must_use]
pub fn spawn_merchants_with_scripts(
    world: &mut mir2_world::World,
    mut spawns: Vec<MerchantSpawn>,
    seeds: Vec<MerchantScriptSeed>,
    displays: &mut DisplayTable,
) -> (usize, Vec<String>, MerchantScriptBook) {
    debug_assert!(
        seeds.is_empty() || seeds.len() == spawns.len(),
        "seeds 必须与 spawns 下标对齐"
    );
    // ① `LocalDB.LoadMerchant` 正序构造抽（每条 1 次；站点归 `merchant` 组）。
    for spawn in spawns.iter_mut() {
        spawn.state.run_tick = world.merchant_ctor_run_delay();
    }
    // ② `MerchantList` 正序建实体：实体槽位序 = 文件序 ⇒ `merchant_ids_sorted`/`merchant_tick` 正序。
    let mut seeds = seeds.into_iter();
    let pending: Vec<(Option<i32>, String, Option<MerchantScriptSeed>)> = spawns
        .into_iter()
        .map(|spawn| {
            let name = spawn.name.clone();
            (world.insert_merchant(spawn), name, seeds.next())
        })
        .collect();
    // ③ `MerchantInitialize` 倒序挂图：只有 `AddToMap` 的 `obj_list` 追加序是反序。
    let mut kept = 0;
    let mut failed = Vec::new();
    let mut book = MerchantScriptBook::new(0);
    for (id, name, seed) in pending.into_iter().rev() {
        match id {
            Some(id) if world.attach_merchant_to_map(id) => {
                kept += 1;
                // **卡 `openmir2-npc-display-table`**：挂图成功的商人登记展示表，否则
                // `drain_aoi_downlink` 的 `displays.get` 守卫会跳过 ⇒ 客户端看不见商人。
                // 字段口径同固定 NPC（`Merchant : NormNpc`：Light=2、RaceImg=50）——
                // `Appr` 在商人侧存于 `MerchantState.appr`（`BaseObject.Appr` 的镜像），
                // 不在 `Entity.appr`。
                if let Some(e) = world.entities.get(id) {
                    let appr = e.merchant.as_ref().map_or(0, |m| m.appr);
                    displays.set(id, npc_display(e.ai.dir, e.race_img, appr, &name));
                }
                if let Some(seed) = seed {
                    book.insert(MerchantNpc {
                        def: NpcDef {
                            chr_name: seed.chr_name,
                            // `Merchant.LoadMerchantScript` 先把 `Path = "Market_Def"`
                            // （`Merchant.cs:1092`），`CHECKNAMELIST` 一类条件按它拼相对路径。
                            file_path: "Market_Def".to_string(),
                            script_name: Some(seed.script_name),
                            is_hide: false,
                            is_quest: false,
                            loaded: !seed.scripts.is_empty(),
                            scripts: seed.scripts,
                            actor_id: id,
                        },
                        price_rate: seed.price_rate,
                        // `NormNpc.Initialize` 的 `CastleMgr.InCastleWarArea(this)`：城堡家族（⑧）
                        // 未落地 ⇒ `None`（oracle 的 `Castle == null` 支，`"????"`）。
                        castle: None,
                        map_name: seed.map_name,
                        curr_x: i32::from(seed.x),
                        curr_y: i32::from(seed.y),
                    });
                }
            }
            _ => failed.push(name),
        }
    }
    (kept, failed, book)
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
        let (spawns, report, _seeds) = load_merchant_spawns(
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
        let (spawns, _, _seeds) = load_merchant_spawns(
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

    /// **C3 `merchant_script_book_actor_ids`**：脚本表按**世界实体 id**（= `ActorId`）索引，
    /// 与 `MerchantList` 一一对应（`ScriptEngine.cs:131` 的 `ActorMgr.Get(actorId)` 口径）。
    ///
    /// 牙：用装载序/0 当键（不是实体 id）/ 挂图失败也入表 ⇒ 红。
    #[test]
    fn merchant_script_book_maps_world_actor_ids() {
        let dir = temp_envir("book", MERCHANT_TXT);
        let market = dir.join("Envir").join("Market_Def");
        fs::create_dir_all(market.join("smith")).unwrap();
        fs::create_dir_all(market.join("fengmo")).unwrap();
        fs::write(
            market.join("smith/repair-0.txt"),
            "[@main]
smith main
",
        )
        .unwrap();
        fs::write(
            market.join("fengmo/smith-0139.txt"),
            "[@main]
风魔 main
[~@repair]
修好了。
",
        )
        .unwrap();
        let loader = loader_for(&dir);
        let mut rng = RandomNumber::with_seed(42);
        let (spawns, report, seeds) = load_merchant_spawns(
            &loader,
            |name| match name {
                "0" | "0139" => Some(0),
                _ => None,
            },
            &mut rng,
        )
        .expect("装载不应失败");
        assert_eq!(report.kept, spawns.len());
        assert_eq!(seeds.len(), spawns.len(), "seeds 与 spawns 下标对齐");
        let mut world = mir2_world::World::new(64, 64);
        let mut displays = DisplayTable::new();
        let (kept, failed, book) =
            spawn_merchants_with_scripts(&mut world, spawns, seeds, &mut displays);
        assert_eq!(
            displays.len(),
            world.merchant_ids_sorted().len(),
            "商人数 = 展示表项数（数量闭合：卡 openmir2-npc-display-table）"
        );
        assert_eq!(displays.len(), kept);
        assert_eq!(kept, 2);
        assert!(failed.is_empty());
        assert_eq!(book.len(), kept, "只有挂图成功的商人进脚本表");
        let ids: Vec<i32> = world.merchant_ids_sorted();
        assert_eq!(ids.len(), 2);
        for id in ids {
            let name = world.entities.get(id).expect("实体在").name.clone();
            let entry = book
                .get(id)
                .unwrap_or_else(|| panic!("实体 {id}（{name}）缺少脚本表项"));
            assert_eq!(entry.def.actor_id, id);
            assert_eq!(entry.def.chr_name, name);
            assert_eq!(entry.def.file_path, "Market_Def");
            assert!(entry.def.script_name.is_some());
            assert!(entry.def.scripts[0].contains_label("@main"));
        }
    }

    /// **C2b `market_def_no_second_parse`**：重名 label 的 `rename_rand` 抽数发生在
    /// `load_merchant_spawns` 的**那一次**解析里；`spawn_merchants_with_scripts` 不接受
    /// RNG（结构上不可能二次解析/二次抽）。
    ///
    /// 牙：宿主若改为在 spawn 阶段重新 `load_script_file`，`rename_rand` 计数会再 +1 ⇒ 红；
    /// 装载器若丢掉 scripts，标签表为空 ⇒ 红。
    #[test]
    fn market_def_rename_rand_is_drawn_once_at_load() {
        let dir = temp_envir(
            "rng",
            "smith/dup 0 1 1 dup 0 3 0
",
        );
        let market = dir.join("Envir").join("Market_Def");
        fs::create_dir_all(market.join("smith")).unwrap();
        fs::write(
            market.join("smith/dup-0.txt"),
            "[@main]
A
[@dup]
B
[@dup]
C
",
        )
        .unwrap();
        let loader = loader_for(&dir);
        let mut calls = 0;
        let mut rng = RandomNumber::with_seed(42);
        let mut rename_rand = |min: i32, max: i32| {
            calls += 1;
            mir2_world::merchant::merchant_rename_rand(&mut rng, min, max)
        };
        let list = loader
            .load_merchants_loaded(&mut rename_rand)
            .expect("装载不应失败")
            .expect("应有商人");
        assert_eq!(calls, 1, "一个重名 label ⇒ 恰好一次 GetRandomNumber(1,200)");
        let labels: Vec<&str> = list[0].scripts[0]
            .record_list
            .iter()
            .map(|r| r.s_label.as_str())
            .collect();
        assert_eq!(labels.len(), 3, "重名 label 改名后仍保留三条记录");
        assert_eq!(labels[0], "@main");
        assert!(labels[1].starts_with("@dup"));
        assert!(labels[2].starts_with("@dup"));
        assert_ne!(labels[1], labels[2]);
        assert_eq!(
            list[0].scripts[0].find_record("@dup").unwrap().s_label,
            "@dup"
        );
    }

    /// 无 `Merchant.txt` ⇒ oracle `File.Exists == false` ⇒ 无记录、无差异。
    #[test]
    fn missing_merchant_txt_is_empty() {
        let dir = temp_envir("none", "\n");
        let _ = fs::remove_file(dir.join("Envir").join("Merchant.txt"));
        let loader = loader_for(&dir);
        let mut rng = RandomNumber::with_seed(42);
        let (spawns, report, _seeds) =
            load_merchant_spawns(&loader, |_| Some(0), &mut rng).unwrap();
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
        let (spawns, report, _seeds) =
            load_merchant_spawns(&loader, |n| registry.id_of(n), &mut rng)
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

    /// reviewer-5 `0f220cac` F-1 返工 + owner `b311a876b1` 倒序裁决：**两个次序必须解耦**。
    ///
    /// oracle `WorldServer.Merchant.cs:8` 的 `for (i = Count-1; i >= 0; i--)` 只让 `AddToMap`
    /// 对 map cell `obj_list` 的**追加次序是反序**；`MerchantList` 本身仍是 `LocalDB.LoadMerchant`
    /// 的**正序**（`ProcessMerchants` 从 `MerchantPosition` 起正序 `Run`）⇒ 实体槽位序
    /// （`merchant_ids_sorted`）必须是文件正序。
    ///
    /// 牙齿：把整个 spawn 循环一起 `rev()`（正序抽 + 倒序建实体/挂图）⇒ `merchant_ids_sorted`
    /// 断言必红；改成全正序 ⇒ `obj_list[0]` 断言必红。
    #[test]
    fn spawn_merchants_decouples_tick_order_from_obj_list_order() {
        fn spawn_named(name: &str) -> MerchantSpawn {
            MerchantSpawn {
                name: name.to_string(),
                x: 5,
                y: 5,
                map_id: 0,
                state: MerchantState::new(
                    "m/script".to_string(),
                    "0".to_string(),
                    0,
                    9,
                    100,
                    false,
                    false,
                    0,
                    MerchantFlags::default(),
                    vec![],
                    vec![],
                    0,
                ),
            }
        }
        let mut world = mir2_world::World::new(32, 32);
        let (kept, failed) = spawn_merchants(&mut world, vec![spawn_named("A"), spawn_named("B")]);
        assert_eq!(kept, 2);
        assert!(failed.is_empty());

        // ① tick 序（实体槽位序 = `MerchantList` 序）必须是文件正序。
        let tick_names: Vec<String> = world
            .merchant_ids_sorted()
            .into_iter()
            .map(|id| world.entities.get(id).expect("实体在").name.clone())
            .collect();
        assert_eq!(
            tick_names,
            vec!["A".to_string(), "B".to_string()],
            "MerchantList/tick 序 = LocalDB 文件正序（入实体正序）；把 spawn 循环整体 rev() ⇒ 必红"
        );

        // ② `obj_list` 追加序仍是 `MerchantInitialize` 倒序。
        let (cell, ok) = world.map(0).get_cell_info(5, 5);
        assert!(ok);
        let first = cell.obj_list[0].cell_obj_id;
        let first_name = world.entities.get(first).expect("实体在").name.clone();
        assert_eq!(
            first_name, "B",
            "倒序 MerchantInitialize ⇒ 后加载的 B 先 AddToMap（obj_list[0] 是 B）；改回正序 ⇒ 必红"
        );
    }
}
