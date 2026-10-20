//! Lane **marketdef**：`Market_Def` 商人脚本装载判据（C1/C8/C9/装载退化）。
//!
//! 这些用例走 **公开 API** `MapLoader::load_merchants_loaded`（不是 `crates/script` 的解析器单测）：
//! 牙齿针对 `crates/envir/src/merchant.rs` 的生产路径——**丢弃 `LoadOutcome.scripts`** 会让
//! `scripts`/标签表全空 ⇒ 本文件必红。

use std::fs;
use std::path::Path;

use mir2_envir::{MapLoader, MERCHANT_SCRIPT_DIR};

fn temp_dir(tag: &str) -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "mir2-marketdef-it-{}-{}-{n}",
        std::process::id(),
        tag
    ));
    if dir.exists() {
        fs::remove_dir_all(&dir).expect("清旧临时目录");
    }
    fs::create_dir_all(dir.join("Envir").join(MERCHANT_SCRIPT_DIR)).expect("建 Envir/Market_Def");
    dir
}

fn write_gbk(path: &Path, text: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    let (bytes, _, _) = encoding_rs::GBK.encode(text);
    fs::write(path, bytes.as_ref()).unwrap();
}

fn no_rand() -> impl FnMut(i32, i32) -> i32 {
    |_lo, _hi| 0
}

/// **C1 `market_def_scripts_are_retained`**
///
/// `Merchant.LoadMerchantScript`（`Merchant.cs:1089-1095`）把整份 `Market_Def` 脚本装进
/// `Merchant.ScriptList`；`load_merchants_loaded` 必须把**同一次解析**的 `scripts` 带出来。
///
/// 牙齿：`crates/envir/src/merchant.rs` 若回到「只取四个头字段、丢 `outcome.scripts`」，
/// `scripts` 为空 ⇒ 本用例红（且标签表断言红）。
#[test]
fn market_def_scripts_are_retained() {
    let dir = temp_dir("retain");
    write_gbk(
        &dir.join("Envir").join("Merchant.txt"),
        "A/店 B 1 2 甲 0 3 0 0 0\nC/店 D 3 4 乙 0 4 0 0 0\n",
    );
    write_gbk(
        &dir.join("Envir").join(MERCHANT_SCRIPT_DIR).join("A/店-B.txt"),
        "(@buy @sell @repair @s_repair)\n%120\n+30\n[@main]\n欢迎 <购买/@buy> <修理/@repair>\n[~@repair]\n修得还算不错。\\ \\ \\ <返 回/@Main]\n[~@s_repair]\n特殊修理完成。\n",
    );
    write_gbk(
        &dir.join("Envir")
            .join(MERCHANT_SCRIPT_DIR)
            .join("C/店-D.txt"),
        "[@main]\n空店\n",
    );
    let loader = MapLoader::new(&dir, "Map", "Envir");
    let mut rng = no_rand();
    let list = loader.load_merchants_loaded(&mut rng).unwrap().unwrap();
    assert_eq!(list.len(), 2);

    let a = &list[0];
    assert_eq!(a.record.price_rate, 120);
    assert_eq!(a.scripts.len(), 1, "一份脚本文件产出一个 ScriptInfo");
    let rec = &a.scripts[0];
    for label in ["@main", "~@repair", "~@s_repair"] {
        assert!(
            rec.contains_label(label),
            "脚本标签 {label} 必须保留在 Merchant.ScriptList 里"
        );
    }
    assert_eq!(
        rec.find_record("~@repair").unwrap().procedure_list[0].s_say_msg,
        "修得还算不错。\\ \\ \\ <返 回/@Main]",
        "说词原文（含 `\\` 行标记）必须逐字保留"
    );
    assert!(a.record.flags.is_buy && a.record.flags.is_repair && a.record.flags.is_sup_repair);
    assert_eq!(a.record.item_type_list, vec![30]);

    let c = &list[1];
    assert_eq!(c.scripts.len(), 1);
    assert!(c.scripts[0].contains_label("@main"));
    assert!(!c.scripts[0].contains_label("~@repair"));
}

/// **C8 `market_def_call_define_vectors`**：`#CALL [\call\inc.txt] @inc` 把被调文件的
/// `[@inc]` 段体 inline 到当前位置（`ScriptParsers.cs:141-188`）。
///
/// 牙齿：漏插入 ⇒ 说词缺段体 ⇒ 红；把 `#CALL` 行当普通说词行 ⇒ 红。
#[test]
fn market_def_call_inserts_quest_diary_body() {
    let dir = temp_dir("call");
    write_gbk(
        &dir.join("Envir").join("Merchant.txt"),
        "A/店 B 1 2 甲 0 3 0 0 0\n",
    );
    write_gbk(
        &dir.join("Envir")
            .join(MERCHANT_SCRIPT_DIR)
            .join("A/店-B.txt"),
        "[@main]\n主页面\n#CALL [\\call\\inc.txt] @inc\n",
    );
    write_gbk(
        &dir.join("Envir")
            .join("QuestDiary")
            .join("call")
            .join("inc.txt"),
        "[@inc]\n被调页面 \\ <返回/@main>\n",
    );
    let loader = MapLoader::new(&dir, "Map", "Envir");
    let mut rng = no_rand();
    let list = loader.load_merchants_loaded(&mut rng).unwrap().unwrap();
    let rec = &list[0].scripts[0];
    // oracle `LoadCallScript` 跳过被调文件的 `[label]` 行、把段体原样 inline 到当前位置
    // （`ScriptParsers.cs:141-188`）⇒ 段体并入当前 `[@main]` 说词（无分隔符）。
    assert_eq!(
        rec.find_record("@main").unwrap().procedure_list[0].s_say_msg,
        "主页面被调页面 \\ <返回/@main>",
        "`#CALL` 的段体必须 inline 进当前标签；漏插入/把 #CALL 当普通行 ⇒ 红"
    );
}

/// **C9 `market_def_encoding_bom_vectors`**：GB2312 无 BOM / UTF-8 BOM / UTF-16LE BOM 三种
/// `Market_Def` 文件都被 `StringList.LoadFromFile` 的同口径解码（`StringList.cs:302-357`）。
///
/// 牙齿：按 UTF-8 硬解 GBK（乱码）或把 `FF FE` 当 UTF-16BE ⇒ 文案断言红。
#[test]
fn market_def_encoding_bom_vectors() {
    let dir = temp_dir("enc");
    write_gbk(
        &dir.join("Envir").join("Merchant.txt"),
        "A/店 B 1 2 甲 0 3 0 0 0\nC/店 D 1 2 乙 0 3 0 0 0\nE/店 F 1 2 丙 0 3 0 0 0\n",
    );
    let market = dir.join("Envir").join(MERCHANT_SCRIPT_DIR);
    fs::create_dir_all(market.join("C")).unwrap();
    fs::create_dir_all(market.join("E")).unwrap();
    write_gbk(&market.join("A/店-B.txt"), "[@main]\n国服\n");
    // UTF-8 BOM。
    let mut utf8 = vec![0xEF, 0xBB, 0xBF];
    utf8.extend_from_slice("[@main]\n国服\n".as_bytes());
    fs::write(market.join("C/店-D.txt"), utf8).unwrap();
    // UTF-16LE BOM（FF FE，第三字节不是 FF）。
    let mut utf16 = vec![0xFF, 0xFE];
    for u in "[@main]\n国服\n".encode_utf16() {
        utf16.extend_from_slice(&u.to_le_bytes());
    }
    fs::write(market.join("E/店-F.txt"), utf16).unwrap();

    let loader = MapLoader::new(&dir, "Map", "Envir");
    let mut rng = no_rand();
    let list = loader.load_merchants_loaded(&mut rng).unwrap().unwrap();
    assert_eq!(list.len(), 3);
    for m in &list {
        assert_eq!(
            m.scripts[0].find_record("@main").unwrap().procedure_list[0].s_say_msg,
            "国服",
            "{} 的 BOM/编码解码必须与 StringList 同口径",
            m.record.script_name
        );
    }
}

/// **C10 `market_def_goods_and_flags`**：脚本头 `(@buy ...)` + `%` + `+` + `[goods]` 经
/// **装载器**装配到记录（`ScriptParsers.cs:683-970`）。
///
/// 牙齿：`boFlag=true` 被改成 false ⇒ flags 全空 / price_rate=100 ⇒ 红。
#[test]
fn market_def_goods_and_flags() {
    let dir = temp_dir("goods");
    write_gbk(
        &dir.join("Envir").join("Merchant.txt"),
        "A/店 B 1 2 甲 0 3 0 0 0\n",
    );
    write_gbk(
        &dir.join("Envir").join(MERCHANT_SCRIPT_DIR).join("A/店-B.txt"),
        "(@buy @sell @repair @s_repair)\n%150\n+30\n+3\n[@main]\nhi\n[goods]\n蜡烛 20 3\n金条 1 60\n",
    );
    let loader = MapLoader::new(&dir, "Map", "Envir");
    let mut rng = no_rand();
    let list = loader.load_merchants_loaded(&mut rng).unwrap().unwrap();
    let r = &list[0].record;
    assert!(r.flags.is_buy && r.flags.is_sell && r.flags.is_repair && r.flags.is_sup_repair);
    assert_eq!(r.price_rate, 150);
    assert_eq!(r.item_type_list, vec![30, 3]);
    assert_eq!(r.refill_goods.len(), 2);
    assert_eq!(r.refill_goods[0].item_name, "蜡烛");
    assert_eq!(r.refill_goods[1].item_name, "金条");
    assert_eq!(r.refill_goods[1].refill_time, 60);
}

/// **C11 `market_def_missing_script_degrade`**：脚本文件缺失 ⇒ oracle
/// `LogService.Error` 后**不产脚本、头字段保持默认**（`ScriptParsers.cs:978`）；
/// 记录本身仍入库（`LoadMerchant` 不因脚本缺失丢记录）。
///
/// 牙齿：把「文件缺失」当解析失败/丢记录 ⇒ 断言红。
#[test]
fn market_def_missing_script_degrade() {
    let dir = temp_dir("missing");
    write_gbk(
        &dir.join("Envir").join("Merchant.txt"),
        "A/店 B 1 2 甲 0 3 0 0 0\n",
    );
    let loader = MapLoader::new(&dir, "Map", "Envir");
    let mut rng = no_rand();
    let list = loader.load_merchants_loaded(&mut rng).unwrap().unwrap();
    assert_eq!(list.len(), 1, "脚本缺失不丢商人记录");
    assert!(list[0].scripts.is_empty(), "文件不存在 ⇒ ScriptList 为空");
    assert_eq!(list[0].record.price_rate, 100);
    assert!(!list[0].record.flags.is_buy);
}

/// **C12 `market_def_same_parser_as_npc_def`**：商人脚本与 `Npc_def` 用**同一个解析器**，
/// 唯一差异是 `boFlag`（商人 true：多认 `%`/`+`/`(...)`/`[goods]`；`Npc_def` false）。
///
/// 牙：把商人数值改成 `boFlag=false` ⇒ flags/price_rate 断言红；改用另一解析器/丢标签 ⇒ 标签断言红。
#[test]
fn market_def_same_parser_as_npc_def_only_flag_differs() {
    use mir2_script::{LocalFs, ScriptParsers};
    let dir = temp_dir("parity");
    write_gbk(
        &dir.join("Envir").join("Merchant.txt"),
        "A/店 B 1 2 甲 0 3 0 0 0
",
    );
    let body = "(@buy @repair)
%120
+30
[@main]
主页面 <返回/@main>
[goods]
蜡烛 20 3
";
    write_gbk(
        &dir.join("Envir")
            .join(MERCHANT_SCRIPT_DIR)
            .join("A/店-B.txt"),
        body,
    );
    let loader = MapLoader::new(&dir, "Map", "Envir");
    let mut rng = no_rand();
    let merchant = loader.load_merchants_loaded(&mut rng).unwrap().unwrap();

    // 同一个文件、同一个解析器，`boFlag=false`（`Npc_def` 口径）。
    let parsers = ScriptParsers::new();
    let mut rng2 = no_rand();
    let npc = parsers
        .load_script_file(
            &LocalFs,
            &dir.join("Envir"),
            MERCHANT_SCRIPT_DIR,
            "A/店-B",
            false,
            &mut rng2,
        )
        .unwrap()
        .unwrap();
    let labels = |recs: &[mir2_script::ScriptInfo]| -> Vec<String> {
        recs[0]
            .record_list
            .iter()
            .map(|r| r.s_label.clone())
            .collect()
    };
    assert_eq!(
        labels(&merchant[0].scripts),
        labels(&npc.scripts),
        "标签/条件/动作语法同源（只有 boFlag 差异）"
    );
    assert!(merchant[0].record.flags.is_buy && merchant[0].record.flags.is_repair);
    assert_eq!(merchant[0].record.price_rate, 120);
    assert_eq!(merchant[0].record.refill_goods.len(), 1);
    // `boFlag=false`：头字段与 `[goods]` 不收集（`ScriptParsers.cs:681/946`）。
    assert!(!npc.merchant_flags.is_buy);
    assert_eq!(npc.price_rate, None);
    assert!(npc.refill_goods.is_empty());
}
