//! `Envir/MonGen.txt` 加载器集成判据（卡 `openmir2-m3-mongen-load-spawn`）。
//!
//! 与 `mongen.rs` 内联单测互补：这里只走 `mir2_envir` 的公开 API，覆盖
//! 「`loadgen` 包含在 oracle 下不可达」这条**刻意保留的缺陷**在加载器层的牙齿
//! （把 `LocalDB.cs:520` 的实参写反修"对" ⇒ 本用例必红）。

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use mir2_envir::{mon_gen_count, MapLoader, HEADER_BYTES};

static SEQ: AtomicU64 = AtomicU64::new(0);

fn temp_dir(tag: &str) -> PathBuf {
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let dir =
        std::env::temp_dir().join(format!("mir2-mongen-it-{}-{}-{n}", std::process::id(), tag));
    if dir.exists() {
        fs::remove_dir_all(&dir).expect("清旧临时目录");
    }
    fs::create_dir_all(&dir).expect("建临时目录");
    dir
}

fn write_gbk(path: &Path, text: &str) {
    let (bytes, _, _) = encoding_rs::GBK.encode(text);
    fs::write(path, bytes.as_ref()).unwrap();
}

/// 2×2 全走格 `.map` 字节。
fn synth_map() -> Vec<u8> {
    let mut buf = vec![0u8; HEADER_BYTES + 4 * mir2_envir::CELL_BYTES];
    buf[0..2].copy_from_slice(&2u16.to_le_bytes());
    buf[2..4].copy_from_slice(&2u16.to_le_bytes());
    buf
}

/// `loadgen` 包含在 oracle 下**永不触发**（`LocalDB.cs:520` 的 `CompareLStr` 实参写反）。
///
/// 夹具让 `loadgen` 目标文件真实存在且内容是一行**合法**配置（地图 `B` 在注册表里）：
/// - oracle 口径 ⇒ 只加载主文件里那 1 条（`A`），子文件那行**不得**出现；
/// - 若有人"顺手修"成 `compare_lstr(行, "loadgen")` ⇒ 子文件被展开、结果多出 `B` 行 ⇒ 本用例必红。
#[test]
fn loader_does_not_expand_loadgen_include() {
    let dir = temp_dir("loadgen");
    let envir = dir.join("Envir");
    let map_dir = dir.join("Map");
    let mongen_dir = envir.join("MonGen");
    fs::create_dir_all(&map_dir).unwrap();
    fs::create_dir_all(&mongen_dir).unwrap();

    // 两张图注册进注册表（A / B 都能通过 `GetMapInfo != null` 校验）。
    fs::write(map_dir.join("fa.map"), synth_map()).unwrap();
    fs::write(map_dir.join("fb.map"), synth_map()).unwrap();
    write_gbk(&envir.join("MapInfo.txt"), "[A|fa 甲]\n[B|fb 乙]\n");

    // 主文件：一条合法行 + 一条 `loadgen` 指令（目标为子文件里的合法行）。
    write_gbk(
        &envir.join("MonGen.txt"),
        "A 1 2 甲 0 3 60\nloadgen /sub.txt\n",
    );
    write_gbk(&mongen_dir.join("sub.txt"), "B 7 8 乙 0 4 30\n");

    let loader = MapLoader::new(dir.clone(), "Map", "Envir");
    let registry = loader.load_registry().unwrap();
    assert!(registry.id_of("A").is_some());
    assert!(registry.id_of("B").is_some());

    let list = loader.load_mon_gen(&registry).unwrap().unwrap();
    assert_eq!(
        list.len(),
        1,
        "oracle 的 loadgen 判定实参写反 ⇒ 子文件不得被展开；实际 {list:?}"
    );
    assert_eq!(list[0].map_name, "A");
    assert_eq!(mon_gen_count(&list), 3);

    fs::remove_dir_all(&dir).ok();
}

/// 真机部署件读数（仓外 `E:\MirServer`，须显式 `MIR2_MONGEN_DIR` 才跑）。
///
/// 本用例是**读数登记**而非 C# 交叉断言：C# 侧探针（`LoadMonGen` 的 `mongenCount`）
/// 由 ②③ 的两侧窗口/后续探针补；这里只固定「部署件实读」与结构不变量，便于日后回归对比。
#[test]
#[ignore = "读真机 E:\\MirServer（仓外）；MIR2_MONGEN_DIR 显式跑"]
fn real_deployment_mongen_reading() {
    let base = std::env::var("MIR2_MONGEN_DIR").expect("需设 MIR2_MONGEN_DIR=<M2GameSvr 目录>");
    let loader = MapLoader::new(base, "Map", "Envir");
    let registry = loader.load_registry().unwrap();
    let list = loader
        .load_mon_gen(&registry)
        .unwrap()
        .expect("部署件应有 Envir/MonGen.txt");
    let total: i32 = mon_gen_count(&list);
    println!(
        "MonGen entries={} sum_count={} maps_loaded={}",
        list.len(),
        total,
        registry.len()
    );
    for m in &list {
        assert!(!m.map_name.is_empty());
        assert!(!m.mon_name.is_empty());
        assert!(m.zen_time > 0);
        assert!(registry.id_of(&m.map_name).is_some());
    }
    // 本机部署件实读（2026-10-13）：entries=2827 / sum_count=39019 / maps_loaded=322；
    // 与独立 Python 复刻（GetValidStr3 + MapInfo 名文件拆分 + 同一校验）逐项一致。
    assert_eq!(list.len(), 2827, "部署件条目数漂移（见 last-m2svr 报告）");
    assert_eq!(total, 39019, "部署件 ΣCount 漂移（见 last-m2svr 报告）");
    assert_eq!(registry.len(), 322, "部署件地图注册表条目数漂移");
}
