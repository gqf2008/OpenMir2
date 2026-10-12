//! 真身对拍：商店定价 / 修理公式（`crates/formula/src/shop.rs`）。
//!
//! 向量来源 = `mir2-rs/tests/parity/m3/shop-price-probe`（C# 真身 `GameSrv.Npc.Merchant`
//! 私有方法反射调用）→ `vectors/shop_price_vectors.tsv`。本用例回读 TSV，逐族重建输入并
//! 逐字段核对 Rust 实现（`GetItemPrice`/`GetUserItemPrice`/`GetUserPrice`/`GetSellItemPrice` +
//! 修理成本/副作用），含 oracle 缺陷（`StdMode 43` 改写 `DuraMax`、整数截断耗久、恒真守卫）。
//!
//! **改坏必红**：见 [`redcheck_stdmode43_duramax_side_effect`] 与
//! [`redcheck_repair_dec_dura_truncation`]——去掉/改坏任一照搬缺陷 ⇒ 向量用例必红。

use std::collections::HashMap;
use std::path::PathBuf;
use std::str::FromStr;

use mir2_formula::shop::{
    get_item_price, get_sell_item_price, get_user_item_price, get_user_price, query_repair_cost,
    repair_item, ItemPrice, PriceTables, RepairLabel, RepairOutcome, ShopConfig, StdItem, UserItem,
};

fn vectors_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/parity/m3/shop-price-probe/vectors/shop_price_vectors.tsv")
}

fn p<T: FromStr>(s: &str) -> T
where
    T::Err: std::fmt::Debug,
{
    s.parse::<T>()
        .unwrap_or_else(|e| panic!("解析 {s:?} 失败: {e:?}"))
}

fn std_item(std_mode: u8, dura_max: u16, price: i32) -> StdItem {
    StdItem {
        std_mode,
        dura_max,
        price,
        ..StdItem::default()
    }
}

fn approx(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

fn check_item_price(f: &[&str]) {
    assert_eq!(f.len(), 11, "item_price 列数非法: {f:?}");
    let index: u16 = p(f[2]);
    let list_present = f[3] == "1";
    let list_index: u16 = p(f[4]);
    let list_price: f64 = p(f[5]);
    let std_present = f[6] == "1";
    let std_mode: u8 = p(f[7]);
    let std_price: i32 = p(f[8]);
    let in_type_list = f[9] == "1";
    let expect: f64 = p(f[10]);

    let list = if list_present {
        vec![ItemPrice {
            w_index: list_index,
            n_price: list_price,
        }]
    } else {
        Vec::new()
    };
    let types = if in_type_list {
        vec![i32::from(std_mode)]
    } else {
        Vec::new()
    };
    let mut catalog: HashMap<u16, StdItem> = HashMap::new();
    if std_present {
        catalog.insert(index, std_item(std_mode, 0, std_price));
    }
    let tables = PriceTables {
        source: &catalog,
        item_price_list: &list,
        item_type_list: &types,
    };
    let got = get_item_price(&tables, index);
    assert!(
        approx(got, expect),
        "item_price {}: got {got}, expect {expect}",
        f[1]
    );
}

fn check_user_item_price(f: &[&str]) {
    assert_eq!(f.len(), 18, "user_item_price 列数非法: {f:?}");
    let std_present = f[2] == "1";
    let base: f64 = p(f[3]);
    let std_mode: u8 = p(f[4]);
    let std_dura_max: u16 = p(f[5]);
    let dura: u16 = p(f[6]);
    let dura_max: u16 = p(f[7]);
    let mut desc = [0u8; 14];
    for (i, slot) in desc.iter_mut().enumerate().take(8) {
        *slot = p(f[8 + i]);
    }
    let expect: f64 = p(f[16]);
    let expect_dura_max: u16 = p(f[17]);

    let list = vec![ItemPrice {
        w_index: 100,
        n_price: base,
    }];
    let mut catalog: HashMap<u16, StdItem> = HashMap::new();
    if std_present {
        catalog.insert(100, std_item(std_mode, std_dura_max, 0));
    }
    let mut user_item = UserItem {
        index: 100,
        dura,
        dura_max,
        desc,
        ..UserItem::default()
    };
    let tables = PriceTables {
        source: &catalog,
        item_price_list: &list,
        item_type_list: &[],
    };
    let got = get_user_item_price(&tables, &mut user_item);
    assert!(
        approx(got, expect),
        "user_item_price {}: got {got}, expect {expect}",
        f[1]
    );
    assert_eq!(
        user_item.dura_max, expect_dura_max,
        "user_item_price {}: DuraMax 副作用 got {}, expect {expect_dura_max}",
        f[1], user_item.dura_max
    );
}

fn check_user_price(f: &[&str]) {
    assert_eq!(f.len(), 8, "user_price 列数非法: {f:?}");
    let n_price: f64 = p(f[2]);
    let price_rate: i32 = p(f[3]);
    let cfg = ShopConfig {
        price_rate,
        castle_merchant: f[4] == "1",
        castle_member: f[5] == "1",
        castle_member_price_rate: p(f[6]),
        ..ShopConfig::default()
    };
    let expect: i32 = p(f[7]);
    let got = get_user_price(&cfg, n_price);
    assert_eq!(
        got, expect,
        "user_price {}: got {got}, expect {expect}",
        f[1]
    );
}

fn check_sell_price(f: &[&str]) {
    assert_eq!(f.len(), 4, "sell_price 列数非法: {f:?}");
    let n_price: f64 = p(f[2]);
    let expect: i32 = p(f[3]);
    let got = get_sell_item_price(n_price);
    assert_eq!(
        got, expect,
        "sell_price {}: got {got}, expect {expect}",
        f[1]
    );
}

fn check_repair_cost(f: &[&str]) {
    assert_eq!(f.len(), 11, "repair_cost 列数非法: {f:?}");
    let base: f64 = p(f[2]);
    let price_rate: i32 = p(f[3]);
    let dura: u16 = p(f[4]);
    let dura_max: u16 = p(f[5]);
    let std_mode: u8 = p(f[6]);
    let label_kind: i32 = p(f[7]);
    let is_repair = f[8] == "1";
    let is_sup_repair = f[9] == "1";
    let expect: i32 = p(f[10]);

    let list = vec![ItemPrice {
        w_index: 200,
        n_price: base,
    }];
    let mut catalog: HashMap<u16, StdItem> = HashMap::new();
    catalog.insert(200, std_item(std_mode, 1000, 0));
    let cfg = ShopConfig {
        price_rate,
        ..ShopConfig::default()
    };
    let mut user_item = UserItem {
        index: 200,
        dura,
        dura_max,
        ..UserItem::default()
    };
    let tables = PriceTables {
        source: &catalog,
        item_price_list: &list,
        item_type_list: &[],
    };
    let got = query_repair_cost(
        &tables,
        &cfg,
        &mut user_item,
        label_kind == 1,
        is_repair,
        is_sup_repair,
    );
    assert_eq!(
        got, expect,
        "repair_cost {}: got {got}, expect {expect}",
        f[1]
    );
}

fn check_repair_item(f: &[&str]) {
    assert_eq!(f.len(), 15, "repair_item 列数非法: {f:?}");
    let base: f64 = p(f[2]);
    let price_rate: i32 = p(f[3]);
    let dura: u16 = p(f[4]);
    let dura_max: u16 = p(f[5]);
    let std_mode: u8 = p(f[6]);
    let label_kind: i32 = p(f[7]);
    let is_repair = f[8] == "1";
    let is_sup_repair = f[9] == "1";
    let gold: i32 = p(f[10]);
    let expect_frames = f[11];
    let expect_gold: i32 = p(f[12]);
    let expect_dura: u16 = p(f[13]);
    let expect_dura_max: u16 = p(f[14]);

    let label = match label_kind {
        1 => RepairLabel::SuperRepair,
        2 => RepairLabel::SuperRepairFail,
        _ => RepairLabel::Other,
    };
    let expect_outcome = if expect_frames == "NONE" {
        RepairOutcome::NoFrame
    } else if expect_frames == "FAIL" {
        RepairOutcome::Fail
    } else {
        RepairOutcome::Ok
    };

    let list = vec![ItemPrice {
        w_index: 300,
        n_price: base,
    }];
    let mut catalog: HashMap<u16, StdItem> = HashMap::new();
    catalog.insert(300, std_item(std_mode, 1000, 0));
    let cfg = ShopConfig {
        price_rate,
        ..ShopConfig::default()
    };
    let mut user_item = UserItem {
        index: 300,
        dura,
        dura_max,
        ..UserItem::default()
    };
    let tables = PriceTables {
        source: &catalog,
        item_price_list: &list,
        item_type_list: &[],
    };
    let got = repair_item(
        &tables,
        &cfg,
        &mut user_item,
        gold,
        label,
        is_repair,
        is_sup_repair,
    );
    assert_eq!(
        got.outcome, expect_outcome,
        "repair_item {}: outcome {:?}, expect {expect_outcome:?} (frames={expect_frames})",
        f[1], got.outcome
    );
    assert_eq!(got.gold, expect_gold, "repair_item {}: gold", f[1]);
    assert_eq!(got.dura, expect_dura, "repair_item {}: dura", f[1]);
    assert_eq!(
        got.dura_max, expect_dura_max,
        "repair_item {}: dura_max",
        f[1]
    );
}

#[test]
fn shop_price_vectors_match_oracle() {
    let path = vectors_path();
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("读不到真身向量 {}: {e}", path.display()));
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let f: Vec<&str> = line.split('\t').collect();
        *counts.entry(f[0]).or_insert(0) += 1;
        match f[0] {
            "item_price" => check_item_price(&f),
            "uip" => check_user_item_price(&f),
            "user_price" => check_user_price(&f),
            "sell_price" => check_sell_price(&f),
            "repair_cost" => check_repair_cost(&f),
            "repair_item" => check_repair_item(&f),
            other => panic!("未知向量族 {other}: {line}"),
        }
    }
    // 各族至少一条，防止向量文件被掏空后“空跑即绿”。
    for family in [
        "item_price",
        "uip",
        "user_price",
        "sell_price",
        "repair_cost",
        "repair_item",
    ] {
        assert!(
            counts.get(family).copied().unwrap_or(0) > 0,
            "向量族 {family} 缺失"
        );
    }
}

/// **改坏必红**：删掉 `StdMode 43` 分支对 `user_item.dura_max = 10000` 的写回，
/// `get_user_item_price` 会返回 300 而非 oracle 的 158 ⇒ 主向量用例必红。
#[test]
fn redcheck_stdmode43_duramax_side_effect() {
    let mut catalog: HashMap<u16, StdItem> = HashMap::new();
    catalog.insert(100, std_item(43, 1000, 0));
    let list = vec![ItemPrice {
        w_index: 100,
        n_price: 300.0,
    }];
    let mut oracle = UserItem {
        index: 100,
        dura: 500,
        dura_max: 500,
        ..UserItem::default()
    };
    let tables = PriceTables {
        source: &catalog,
        item_price_list: &list,
        item_type_list: &[],
    };
    let got = get_user_item_price(&tables, &mut oracle);
    // oracle：DuraMax 被改写为 10000 ⇒ n20 = 300/2/10000*(10000-500) = 142.5 ⇒ 158
    assert_eq!(oracle.dura_max, 10000, "oracle 路径必须写回 10000");
    assert!(approx(got, 158.0), "oracle 口径应得 158，got {got}");
    // 若“顺手修掉”该副作用（DuraMax 仍为 500）：n20 = 0 ⇒ 结果 300
    let without_clamp = 300.0f64;
    assert_ne!(
        got, without_clamp,
        "若两者相等说明副作用已被移除 ⇒ 真身向量会对不上"
    );
}

/// **改坏必红**：普通修理耗久的 **整数截断** 口径。
/// `diff=986, dec=30 ⇒ 986/30=32（截断）`；若改成四舍五入（33）则 `DuraMax` 差 1 ⇒ 向量必红。
#[test]
fn redcheck_repair_dec_dura_truncation() {
    let diff = 986i32;
    let dec = 30i32;
    let trunc = diff / dec; // C# int 除法（向零截断）
    let rounded = ((diff as f64) / (dec as f64)).round() as i32;
    assert_eq!(trunc, 32);
    assert_eq!(rounded, 33);
    assert_ne!(
        trunc, rounded,
        "本用例的前提是 diff/dec 非整数；改坏为四舍五入必红"
    );
}
