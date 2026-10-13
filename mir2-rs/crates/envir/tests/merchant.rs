//! `Envir/Merchant.txt` 商人加载器集成判据（卡 `openmir2-m3-merchant-load`）。
//!
//! 与 `merchant.rs` 内联单测互补：这里走 `mir2_envir` 的公开 API，覆盖
//! 「`LocalDB.LoadMerchant` + `ScriptParsers` 商人脚本头 / `[goods]`」这条链在加载器层的
//! **头字段装配**与**刻意保留的 oracle 语义**（`%PRICERATE >= 55` 才生效、`Appr` 必填、
//! `BoCanMove` 需 `MoveTime > 0`）的牙齿。

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use mir2_envir::{MapLoader, MerchantRecord, MERCHANT_DEFAULT_PRICE_RATE};

static SEQ: AtomicU64 = AtomicU64::new(0);

fn temp_dir(tag: &str) -> PathBuf {
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "mir2-merchant-it-{}-{}-{n}",
        std::process::id(),
        tag
    ));
    if dir.exists() {
        fs::remove_dir_all(&dir).expect("清旧临时目录");
    }
    fs::create_dir_all(&dir).expect("建临时目录");
    dir
}

/// `Merchant.txt` 与 `Market_Def/*.txt` 部署件均为 GBK 编码。
fn write_gbk(path: &Path, text: &str) {
    let (bytes, _, _) = encoding_rs::GBK.encode(text);
    fs::write(path, bytes.as_ref()).unwrap();
}

/// 重名 label 改名用的确定性随机源（oracle 是 `RandomNumber.GetRandomNumber(1, 200)`）；
/// 夹具里脚本无重名 label，故取值不影响头字段。
fn det_rand() -> impl FnMut(i32, i32) -> i32 {
    let mut n = 0i32;
    move |lo, hi| {
        n += 1;
        lo + (n % (hi - lo + 1))
    }
}

fn build_envir(root: &Path, merchant_txt: &str) {
    let envir = root.join("Envir");
    let market = envir.join("Market_Def");
    fs::create_dir_all(&market).unwrap();
    write_gbk(&envir.join("Merchant.txt"), merchant_txt);

    // `(buy sell)` 头 + `%150` + `+30/+3` + `[goods]`。
    write_gbk(
        &market.join("A-B.txt"),
        "(@buy @sell)\n%150\n+30\n+3\n[@main]\nhi\n[goods]\n蜡烛 20 3\n",
    );
    // `%20 < 55` ⇒ oracle 不覆盖 PriceRate（保持 100）；`(storage getback)`。
    write_gbk(
        &market.join("CB-D.txt"),
        "(@storage @getback)\n%20\n+25\n[@main]\nhi\n[goods]\n金条 1 60\n",
    );
    // `G-H.txt` 故意不建 ⇒ 脚本缺失，头字段保持默认。
}

#[test]
fn loader_assembles_script_header_and_goods() {
    let dir = temp_dir("assemble");
    build_envir(
        &dir,
        ";注释\nA B 1 2 甲 0 3 0 0 0\nCB D 3 4 乙 0 4 1 1 30\nE F 5 6 丙 0\nG H 7 8 丁 0 5 0\n",
    );
    let loader = MapLoader::new(&dir, "Map", "Envir");
    let mut rng = det_rand();
    let list = loader.load_merchants(&mut rng).unwrap().unwrap();

    // `E F` 缺 Appr 被丢弃 ⇒ 余 3 条（A B, CB D, G H）。
    assert_eq!(list.len(), 3, "实际 {list:?}");

    let a = &list[0];
    assert_eq!(a.script_name, "A");
    assert_eq!(a.map_name, "B");
    assert!(a.flags.is_buy && a.flags.is_sell);
    assert_eq!(a.price_rate, 150, "%150 >= 55 应生效");
    assert_eq!(a.item_type_list, vec![30, 3]);
    assert_eq!(a.refill_goods.len(), 1);
    assert_eq!(a.refill_goods[0].item_name, "蜡烛");
    assert_eq!(a.refill_goods[0].count, 20);
    assert_eq!(a.refill_goods[0].refill_time, 3);

    let b = &list[1];
    assert!(b.flags.is_storage && b.flags.is_getback);
    // **牙齿**：`%20 < 55` ⇒ oracle 不覆盖，保持构造默认 100。
    // 若有人把 `ScriptParsers.cs:687` 的 `>= 55` 改成 `>= 0`，这里会变 20 ⇒ 必红。
    assert_eq!(
        b.price_rate, MERCHANT_DEFAULT_PRICE_RATE,
        "%20 低于阈值 55 ⇒ PriceRate 保持默认 100（oracle ScriptParsers.cs:687）"
    );
    assert_eq!(b.item_type_list, vec![25]);
    assert_eq!(b.refill_goods.len(), 1);
    // `CanMove=1 && MoveTime=30` ⇒ 可移动。
    assert!(b.can_move);
    assert_eq!(b.move_time, 30);

    let g = &list[2];
    // 脚本缺失 ⇒ 头字段默认（oracle LogService.Error 后不写字段）。
    assert_eq!(g.price_rate, MERCHANT_DEFAULT_PRICE_RATE);
    assert!(g.item_type_list.is_empty());
    assert!(g.refill_goods.is_empty());
    assert!(!g.flags.is_buy && !g.flags.is_storage);

    fs::remove_dir_all(&dir).ok();
}

/// `BoCanMove` 的 oracle 判定含 `MoveTime > 0`（`LocalDB.cs:463-466`）。
/// 去掉这个合取项 ⇒ 本用例必红。
#[test]
fn can_move_requires_positive_move_time() {
    let dir = temp_dir("canmove");
    build_envir(&dir, "A B 1 2 甲 0 3 0 1 0\n");
    let loader = MapLoader::new(&dir, "Map", "Envir");
    let mut rng = det_rand();
    let list = loader.load_merchants(&mut rng).unwrap().unwrap();
    assert_eq!(list.len(), 1);
    assert!(
        !list[0].can_move,
        "CanMove=1 但 MoveTime=0 ⇒ 不可移动（oracle 的 `&& MoveTime > 0`）"
    );
    fs::remove_dir_all(&dir).ok();
}

/// `Appr` 是入库必填列（`LocalDB.cs:447`）。放宽校验（去掉 `sAppr.is_empty()`）⇒ 本用例必红。
#[test]
fn rows_without_appr_are_dropped() {
    let dir = temp_dir("appr");
    build_envir(&dir, "A B 1 2 甲 0\n");
    let loader = MapLoader::new(&dir, "Map", "Envir");
    let mut rng = det_rand();
    let list = loader.load_merchants(&mut rng).unwrap().unwrap();
    assert!(list.is_empty(), "缺 Appr 的行不得入库，实际 {list:?}");
    fs::remove_dir_all(&dir).ok();
}

/// 主文件缺失 ⇒ `Ok(None)`（对应 oracle `File.Exists == false`）。
#[test]
fn missing_file_is_none() {
    let dir = temp_dir("missing");
    fs::create_dir_all(dir.join("Envir")).unwrap();
    let loader = MapLoader::new(&dir, "Map", "Envir");
    let mut rng = det_rand();
    assert!(loader.load_merchants(&mut rng).unwrap().is_none());
    fs::remove_dir_all(&dir).ok();
}

/// 真机部署件判据（**刻意 `#[ignore]`，不进常驻测试**）：读仓外 `E:\MirServer`。
///
/// ```text
/// $env:MIR2_GAMESVR_DIR='E:\MirServer\M2GameSvr'
/// cargo test -p mir2-envir --test merchant -- --ignored --nocapture
/// ```
///
/// 环境变量缺失时**不静默 skip**：直接报错。
#[test]
#[ignore = "读真机 E:\\MirServer（仓外）；MIR2_GAMESVR_DIR 显式跑"]
fn deployment_merchant_load_matches_oracle() {
    let base = std::env::var("MIR2_GAMESVR_DIR")
        .map_or_else(|_| PathBuf::from(r"E:\MirServer\M2GameSvr"), PathBuf::from);
    assert!(
        base.join("Envir").is_dir(),
        "部署件 Envir 目录不存在：{}（用 MIR2_GAMESVR_DIR 指定）",
        base.join("Envir").display()
    );
    let loader = MapLoader::new(&base, "Map", "Envir");
    let mut rng = det_rand();
    let list = loader
        .load_merchants(&mut rng)
        .unwrap_or_else(|e| panic!("加载部署件 Merchant.txt 失败: {e}"))
        .expect("Merchant.txt 应存在");

    eprintln!("部署件商人数: {}", list.len());
    assert!(list.len() > 50, "部署件商人数异常偏少: {}", list.len());

    let find = |name: &str| -> &MerchantRecord {
        list.iter()
            .find(|m| m.script_name == name)
            .unwrap_or_else(|| panic!("未找到商人 {name}"))
    };

    // `Merchant.txt` 部署件第 60 行：`盟重土城/仓库 0145 9 12 仓库保管员 0 9 0`。
    let warehouse = find("盟重土城/仓库");
    assert_eq!(warehouse.map_name, "0145");
    assert_eq!((warehouse.x, warehouse.y), (9, 12));
    assert_eq!(warehouse.chr_name, "仓库保管员");
    assert_eq!(warehouse.appr, 9);

    // 第 53 行 `封魔谷/恶魔修理师 d2008 31 24 封魔修理师 0 15 0`：
    // 脚本头 `%100`（>=55 生效）+ `(@buy @sell @repair @s_repair @storage @getback …)`。
    let repair = find("封魔谷/恶魔修理师");
    assert_eq!(repair.map_name, "d2008");
    assert_eq!(repair.price_rate, 100);
    assert!(repair.flags.is_repair && repair.flags.is_sup_repair);
    assert!(repair.flags.is_storage && repair.flags.is_getback);
    assert!(
        repair.item_type_list.contains(&10) && repair.item_type_list.contains(&30),
        "恶魔修理师 +StdMode 应含 10/30: {:?}",
        repair.item_type_list
    );

    // `%20 < 55` 的部署件（`封魔谷/恶魔杂货匠-4.txt`）⇒ PriceRate 未被覆盖，保持默认 100。
    let general = find("封魔谷/恶魔杂货匠");
    assert_eq!(
        general.price_rate, MERCHANT_DEFAULT_PRICE_RATE,
        "部署件 %20 低于阈值 ⇒ 默认 100"
    );

    // 至少一个商人有 `[goods]` 补货表，且条目形态正确。
    let with_goods = list.iter().find(|m| !m.refill_goods.is_empty());
    let with_goods = with_goods.expect("部署件应至少有带 [goods] 的商人");
    assert!(!with_goods.refill_goods[0].item_name.is_empty());

    // 摘要（供 owner 侧留档）。
    let goods_total: usize = list.iter().map(|m| m.refill_goods.len()).sum();
    let typed = list.iter().filter(|m| !m.item_type_list.is_empty()).count();
    eprintln!(
        "商人数={} 有 [goods] 的商人={} goods 总条目={} 有 +StdMode 的商人={}",
        list.len(),
        list.iter().filter(|m| !m.refill_goods.is_empty()).count(),
        goods_total,
        typed
    );
}
