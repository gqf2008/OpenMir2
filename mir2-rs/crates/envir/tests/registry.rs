//! MapInfo 解析 + 地图名→`map_id` 注册表 + 取图路径口径（卡 §1 最后一条）。
//!
//! 用 `std::env::temp_dir()` 下的**临时目录**造夹具（不读仓外目录），覆盖：
//! - `[name|file desc]`、`[name\tdesc]`、普通行；
//! - 注册表：`name → map_id`（大小写不敏感）、重复名跳过、文件缺失跳过；
//! - `require_id` 找不到 ⇒ `MapNameNotFound`（禁止静默占位图）；
//! - `Path.Combine` 语义（rooted 参数丢弃前面）+ 点位表路径 = `.map` 路径 + `.txt`。

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use mir2_envir::mapinfo::parse_section_line;
use mir2_envir::{
    map_file_path, parse_map_info_lines, path_combine, point_file_path, EnvirError, MapLoader,
    CELL_BYTES, DEFAULT_ENVIR_DIR, DEFAULT_MAP_DIR, HEADER_BYTES,
};

static SEQ: AtomicU64 = AtomicU64::new(0);

fn temp_dir(tag: &str) -> PathBuf {
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "mir2-envir-test-{}-{}-{n}",
        std::process::id(),
        tag
    ));
    if dir.exists() {
        fs::remove_dir_all(&dir).expect("清旧临时目录");
    }
    fs::create_dir_all(&dir).expect("建临时目录");
    dir
}

/// 3×3 全走格 `.map` 字节。
fn synth_walk(width: u16, height: u16) -> Vec<u8> {
    let n = usize::from(width) * usize::from(height);
    let mut buf = vec![0u8; HEADER_BYTES + n * CELL_BYTES];
    buf[0..2].copy_from_slice(&width.to_le_bytes());
    buf[2..4].copy_from_slice(&height.to_le_bytes());
    buf
}

/// 真实 `Envir/*.txt` 是 GB2312/GBK 编码；夹具也必须按 GBK 落盘，
/// 否则 UTF-8 的三字节中文会把后一个 ASCII 定界符（如 `]`）吃进 GBK 双字节里。
fn write_gbk(path: &Path, text: &str) {
    let (bytes, _, _) = encoding_rs::GBK.encode(text);
    fs::write(path, bytes.as_ref()).unwrap();
}

#[test]
fn section_line_parsing_matches_maps_cs() {
    // 真实部署件的三条形态。
    let e = parse_section_line("[jiaren|0150 佳人居] KILLFUNC(1) SAFE QUIZ").unwrap();
    assert_eq!(e.map_name, "jiaren");
    assert_eq!(e.map_file_name, "0150");
    assert_eq!(e.server_index, 0);

    let e = parse_section_line("[0147\t桃源] NORECALL NORANDOMMOVE SAFE").unwrap();
    assert_eq!(e.map_name, "0147");
    assert_eq!(e.map_file_name, "0147");

    let e = parse_section_line("[DB|DM011 恶魔大殿] NORECONNECT(0)").unwrap();
    assert_eq!(e.map_name, "DB");
    assert_eq!(e.map_file_name, "DM011");

    // 普通行（地图连接点）不是 section。
    assert!(parse_section_line("M100A\t29,81\t->\tM100B\t287,282").is_none());
    assert!(parse_section_line("; comment").is_none());
    assert!(parse_section_line("").is_none());
}

#[test]
fn parse_lines_keeps_order() {
    let lines = vec![
        "; c".to_string(),
        "[A|fa 甲]".to_string(),
        "route".to_string(),
        "[B|fb 乙]".to_string(),
    ];
    let entries = parse_map_info_lines(&lines);
    assert_eq!(
        entries
            .iter()
            .map(|e| (e.map_name.as_str(), e.map_file_name.as_str()))
            .collect::<Vec<_>>(),
        vec![("A", "fa"), ("B", "fb")]
    );
}

#[test]
fn path_conventions() {
    let base = Path::new("E:\\base");
    assert_eq!(
        map_file_path(base, DEFAULT_MAP_DIR, "0150"),
        PathBuf::from("E:\\base\\Map\\0150.map")
    );
    // 绝对 .map 路径 ⇒ `.NET Path.Combine` 丢弃前面参数 ⇒ 点位表 = <abs>.txt。
    let abs = Path::new("E:\\base\\Map\\0150.map");
    assert_eq!(
        point_file_path(base, DEFAULT_ENVIR_DIR, abs),
        PathBuf::from("E:\\base\\Map\\0150.map.txt")
    );
    // 相对 .map 路径 ⇒ Envir/Point/<rel>.txt。
    let rel = Path::new("Map\\0150.map");
    assert_eq!(
        point_file_path(base, DEFAULT_ENVIR_DIR, rel),
        PathBuf::from("E:\\base\\Envir\\Point\\Map\\0150.map.txt")
    );
    // Path.Combine：后一个 rooted 参数覆盖前一个。
    assert_eq!(
        path_combine(&[base, Path::new("Map"), abs]),
        PathBuf::from("E:\\base\\Map\\0150.map")
    );
}

#[test]
fn registry_assigns_ids_in_map_info_order() {
    let base = temp_dir("registry");
    let map_dir = base.join("Map");
    let envir_dir = base.join("Envir");
    fs::create_dir_all(&map_dir).unwrap();
    fs::create_dir_all(&envir_dir).unwrap();
    fs::write(map_dir.join("fa.map"), synth_walk(3, 3)).unwrap();
    fs::write(map_dir.join("fb.map"), synth_walk(4, 4)).unwrap();
    write_gbk(
        &envir_dir.join("MapInfo.txt"),
        concat!(
            "; MapInfo 样件\n",
            "[MapA|fa 甲地] SAFE DAY\n",
            "[MapB|fb 乙地] DAY\n",
            "MapA\t1,1\t->\tMapB\t2,2\n",
        ),
    );

    let loader = MapLoader::new(&base, "Map", "Envir");
    let reg = loader.load_registry().expect("加载注册表");

    assert_eq!(reg.len(), 2);
    assert_eq!(reg.id_of("MapA"), Some(0));
    assert_eq!(reg.id_of("MapB"), Some(1));
    assert_eq!(
        reg.id_of("mapa"),
        Some(0),
        "大小写不敏感（OrdinalIgnoreCase）"
    );
    assert_eq!(reg.name_of(0), Some("MapA"));
    assert_eq!(reg.name_of(1), Some("MapB"));
    assert_eq!(reg.map_by_name("MapA").unwrap().width, 3);
    assert_eq!(reg.map_by_name("MapB").unwrap().height, 4);
    assert_eq!(reg.get(1).unwrap().map_name, "MapB");
    assert!(reg.skipped().is_empty());

    // 找不到就报错，绝不静默用占位图。
    match reg.require_id("没有这张图") {
        Err(EnvirError::MapNameNotFound(name)) => assert_eq!(name, "没有这张图"),
        other => panic!("期望 MapNameNotFound，实际 {other:?}"),
    }
}

#[test]
fn registry_skips_duplicates_and_missing_files() {
    let base = temp_dir("registry-skip");
    let map_dir = base.join("Map");
    let envir_dir = base.join("Envir");
    fs::create_dir_all(&map_dir).unwrap();
    fs::create_dir_all(&envir_dir).unwrap();
    fs::write(map_dir.join("fa.map"), synth_walk(3, 3)).unwrap();
    write_gbk(
        &envir_dir.join("MapInfo.txt"),
        concat!(
            "[MapA|fa 甲]\n",
            "[MapA|fa 甲的重复]\n",  // 重名 ⇒ 跳过
            "[MapC|missing 缺失]\n", // 文件缺失 ⇒ 跳过
            "[MapB|fa 乙]\n",        // 复用 fa.map，名字不同 ⇒ 正常入表
        ),
    );

    let reg = MapLoader::new(&base, "Map", "Envir")
        .load_registry()
        .expect("加载注册表");
    assert_eq!(reg.len(), 2);
    assert_eq!(reg.id_of("MapA"), Some(0));
    assert_eq!(reg.id_of("MapB"), Some(1), "map_id 按成功入表的先后");
    assert_eq!(reg.skipped().len(), 2);
    assert!(reg.skipped()[0].reason.contains("重复"));
    assert!(reg.skipped()[1].reason.contains("不存在") || !reg.skipped()[1].reason.is_empty());
}

#[test]
fn registry_reads_point_file_next_to_map() {
    let base = temp_dir("registry-points");
    let map_dir = base.join("Map");
    let envir_dir = base.join("Envir");
    fs::create_dir_all(&map_dir).unwrap();
    fs::create_dir_all(&envir_dir).unwrap();
    fs::write(map_dir.join("fa.map"), synth_walk(20, 20)).unwrap();
    // 点位表路径 = <abs .map>.txt（见 paths 模块头）。
    fs::write(map_dir.join("fa.map.txt"), "3,4\n;skip\n5,6\n").unwrap();
    write_gbk(&envir_dir.join("MapInfo.txt"), "[MapA|fa 甲]\n");

    let reg = MapLoader::new(&base, "Map", "Envir")
        .load_registry()
        .expect("加载注册表");
    let m = reg.get(0).unwrap();
    assert_eq!(m.points.len(), 2);
    assert_eq!((m.points[0].x, m.points[0].y), (3, 4));
    assert!(m.point_file.is_some());
}

/// 注册表暴露 `map_id` → `Envirnoment.Flag`/`MapDesc`（帧 766/54/46 的输入），
/// 且按 MapInfo.txt 的 section 序与加载成功序对齐。
#[test]
fn registry_exposes_flag_and_map_desc() {
    let base = temp_dir("registry-flag");
    let map_dir = base.join("Map");
    let envir_dir = base.join("Envir");
    fs::create_dir_all(&map_dir).unwrap();
    fs::create_dir_all(&envir_dir).unwrap();
    fs::write(map_dir.join("fa.map"), synth_walk(3, 3)).unwrap();
    fs::write(map_dir.join("fb.map"), synth_walk(3, 3)).unwrap();
    write_gbk(
        &envir_dir.join("MapInfo.txt"),
        concat!(
            "[MapA|fa 比奇省] DAY\n",
            "[MapB|fb 战地] FIGHT SAFE MUSIC(13)\n",
            "[MapC|missing 缺失]\n", // 文件缺失 ⇒ 不进表
            "[MapD|fb 夜地] DARK\n",
        ),
    );

    let reg = MapLoader::new(&base, "Map", "Envir")
        .load_registry()
        .expect("加载注册表");
    assert_eq!(reg.len(), 3);
    assert_eq!(reg.map_desc_of(0), Some("比奇省"));
    assert_eq!(reg.map_desc_of(1), Some("战地"));
    assert_eq!(reg.map_desc_of(2), Some("夜地"));
    assert!(reg.flag_of(0).unwrap().day_light);
    assert!(!reg.flag_of(0).unwrap().music);
    assert_eq!(reg.flag_of(0).unwrap().music_id, -1, "非 MUSIC 图");
    let b = reg.flag_of(1).unwrap();
    assert!(b.fight_zone && b.safe_area && b.music);
    assert_eq!(b.music_id, 13);
    assert!(reg.flag_of(2).unwrap().bo_darkness);
    assert!(reg.flag_of(3).is_none());
    // `MapC` 加载失败，`MapD` 拿到 `map_id=2`。
    assert_eq!(reg.name_of(2), Some("MapD"));
}

/// **L1 TransMap 数据面**：`MapManager.AddMapRoute` 的 `sEnvir && dEnvir` 门——只有 src/dst
/// 两侧都加载成功才入 `routes()`；任一侧缺失 ⇒ 进 `unresolved_routes()`（对应 `AddMapRoute`
/// 返回 false）。
///
/// 红检（变异牙）：把 `from_entries` 里 `(Some(s), Some(d))` 的 `s`/`d` 对调 ⇒ 下面
/// `src_map_id`/`dst_map_id` 断言必红。
#[test]
fn registry_resolves_routes_only_when_both_maps_load() {
    let base = temp_dir("registry-routes");
    let map_dir = base.join("Map");
    let envir_dir = base.join("Envir");
    fs::create_dir_all(&map_dir).unwrap();
    fs::create_dir_all(&envir_dir).unwrap();
    fs::write(map_dir.join("fa.map"), synth_walk(8, 8)).unwrap();
    fs::write(map_dir.join("fb.map"), synth_walk(8, 8)).unwrap();
    write_gbk(
        &envir_dir.join("MapInfo.txt"),
        concat!(
            "[MapA|fa 甲地]\n",
            "[MapB|fb 乙地]\n",
            "MapA\t1,1\t->\tMapB\t2,2\n",      // 两侧都在 ⇒ 入表
            "MapA\t3,3\t->\tNoSuchMap\t4,4\n", // 目标图不存在 ⇒ 丢弃
        ),
    );
    let reg = MapLoader::new(&base, "Map", "Envir")
        .load_registry()
        .expect("加载注册表");
    assert_eq!(reg.len(), 2);
    assert_eq!(reg.routes().len(), 1, "只保留两侧都能 FindMap 的连接点");
    let r = &reg.routes()[0];
    assert_eq!(r.route_id, 1);
    assert_eq!((r.src_map_id, r.src_x, r.src_y), (0, 1, 1));
    assert_eq!((r.dst_map_id, r.dst_x, r.dst_y), (1, 2, 2));
    assert_eq!(reg.unresolved_routes().len(), 1);
    assert_eq!(reg.unresolved_routes()[0].dst_map_name, "NoSuchMap");
}
