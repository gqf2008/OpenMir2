//! Envir 真地图注册表接入（`openmir2-m2-map-loading` 的集成点 ④：`CurMap` → `map_id`）。
//!
//! oracle 路径：`M2Share.BasePath`（可执行体目录）+ `Server.conf` 的 `[Share] EnvirDir`/`MapDir`
//! （`GameSvrConf.EnvirDir/MapDir` 的默认值分别是 `Envir`/`Map`，`GameSvrConf.cs:1295-1296`）
//! → `Envir/MapInfo.txt` 的 section 先后顺序就是 `map_id`（见 `mir2_envir::MapRegistry` 的头注释）。
//! 角色记录里的 `CurMap` 是**逻辑地图名**，必须经 `MapRegistry::require_id` 换成 `map_id`；
//! **找不到即拒绝进图**（返回 `Err`），不许静默退回 0 号占位图。
//!
//! 本模块只做「加载 + 取尺寸 + 换 id」三件事，不碰 `crates/envir` 内部，也不建世界（那是
//! `sim_loop` 的事）。`WorldMaps`（S1 占位尺寸）作为**没有 Envir**（开发/测试）时的回退保留并登记：
//! 真机验收（始终有 `Envir/MapInfo.txt`）之后应删掉占位键。三种加载结果的处置见 [`Maps`]。

use std::path::{Path, PathBuf};

use mir2_envir::{MapLoader, MapRegistry};

/// 运行目录 = `Server.conf` 所在目录。
///
/// 生产下 `config::default_path()` 把 `Server.conf` 放在可执行体同目录，因此与 oracle 的
/// `M2Share.BasePath = AppContext.BaseDirectory` 等价；测试/迁移场景可把 `Envir`/`Map`
/// 与显式传入的 `Server.conf` 放在同一目录。
///
/// **相对 `--config`（R3-3）**：显式传 `Server.conf` 这种相对路径时 `Path::parent()` 给出的是
/// **空目录**，`Envir`/`Map` 就会相对进程 cwd 解析；oracle 恒用 `AppContext.BaseDirectory`（exe 目录），
/// 与 cwd 无关。这里先把相对路径规范化到**当前目录**再取 parent，消除该不对称。
#[must_use]
pub fn base_path(conf: &Path) -> PathBuf {
    let abs = if conf.is_relative() {
        std::env::current_dir().map_or_else(|_| conf.to_path_buf(), |cwd| cwd.join(conf))
    } else {
        conf.to_path_buf()
    };
    abs.parent()
        .map_or_else(|| PathBuf::from("."), Path::to_path_buf)
}

/// 注册表加载三态——**「没有 Envir」与「Envir 坏了」必须分开**，后者不许静默退回占位图。
pub enum Maps {
    /// 已加载到非空注册表：用它的尺寸建世界 + `CurMap` 换 `map_id`。
    Envir(MapRegistry),
    /// **没有** `Envir/MapInfo.txt`（开发/测试形态）：保留 S1 `WorldMaps` 占位，调用方记 WRN。
    Placeholder,
    /// `MapInfo.txt` 在、但读失败或**一张都没加载成功**：调用方必须拒绝启动（失败要响）。
    Broken(String),
}

/// 按 `base/<envir_dir>/MapInfo.txt` + `base/<map_dir>/*.map` 加载注册表。
///
/// 判据（见 [`Maps`]）：`MapInfo.txt` **不存在** ⇒ [`Maps::Placeholder`]；文件在但 0 条有效 section、
/// 有 section 但全加载失败、或读失败 ⇒ [`Maps::Broken`]；否则 [`Maps::Envir`]。
///
/// 三态口径照 oracle `Maps.LoadMapInfo`（`src/GameSrv/Maps/Maps.cs:20-31`）用 `File.Exists` 区分
/// 「无文件」与「文件在」；**R3-1**：`Placeholder` 只留给「无文件」，别把「文件在但空表/全解析失败」
/// 也吞成占位（那是坏配置，必须响）。
#[must_use]
pub fn load_registry(base: &Path, envir_dir: &str, map_dir: &str) -> Maps {
    let loader = MapLoader::new(base, map_dir, envir_dir);
    match loader.load_map_info_entries() {
        Ok(entries) if entries.is_empty() => {
            let info_path = loader.map_info_path();
            if info_path.exists() {
                Maps::Broken(format!(
                    "{} 存在但 0 条有效 section（空表或全解析失败）",
                    info_path.display()
                ))
            } else {
                Maps::Placeholder
            }
        }
        Ok(entries) => {
            let reg = MapRegistry::from_entries(loader, entries);
            if reg.is_empty() {
                Maps::Broken(format!(
                    "MapInfo.txt 有 {} 条 section 但一张都没加载成功（skipped={}）",
                    reg.entries().len(),
                    reg.skipped().len()
                ))
            } else {
                Maps::Envir(reg)
            }
        }
        Err(e) => Maps::Broken(e.to_string()),
    }
}

/// 注册表的尺寸表（`map_id` = 下标），用于 `World::with_maps`。
#[must_use]
pub fn sizes(reg: &MapRegistry) -> Vec<(i32, i32)> {
    reg.maps().iter().map(|m| (m.width, m.height)).collect()
}

/// `CurMap`（逻辑名）→ `map_id`。
///
/// - 有注册表：`require_id`（找不到 ⇒ `Err`，调用方拒绝进图）；
/// - 无注册表（S1 `WorldMaps` 占位回退）：`Ok(0)`（调用方负责记 WRN，见 `sim_loop`）。
///
/// # Errors
/// 逻辑名不在注册表内 ⇒ `EnvirError::MapNameNotFound`。
pub fn resolve_map_id(
    registry: Option<&MapRegistry>,
    cur_map: &str,
) -> Result<usize, mir2_envir::EnvirError> {
    match registry {
        Some(reg) => reg.require_id(cur_map),
        None => Ok(0),
    }
}

/// 进图地图解析 = oracle `WorldServer.cs:319-341` 的 `FindMap(MapName)==null ⇒ MapName = HomeMap`。
///
/// 先试 `primary`（空 `HomeMap` 时已由 `homemap` 算出的出生点/记录 `CurMap`）；失败且 `fallback`
/// 非空且不同于 `primary`（记录的 `HomeMap`）再试一次。两者都找不到才 `Err`（调用方按
/// oracle「切服」/fail-closed 处置，不静默退回 0 号图）。
///
/// # Errors
/// `primary` 与 `fallback` 都不在注册表内 ⇒ 返回 `primary` 的 `EnvirError`。
pub fn resolve_enter_map(
    registry: Option<&MapRegistry>,
    primary: &str,
    fallback: &str,
) -> Result<(String, usize), mir2_envir::EnvirError> {
    match resolve_map_id(registry, primary) {
        Ok(id) => Ok((primary.to_string(), id)),
        Err(e) => {
            if !fallback.is_empty() && fallback != primary {
                resolve_map_id(registry, fallback).map(|id| (fallback.to_string(), id))
            } else {
                Err(e)
            }
        }
    }
}

/// 没有 Envir 注册表时的回退尺寸来源判定（owner `272582e1` 裁决；`WorldMaps` 可选化的
/// **fail-closed 半边**）。
///
/// 真 `E:/MirServer/M2GameSvr/Server.conf` 没有 `WorldMaps`（S1 占位键），故它由必需键降为可选键；
/// 但**两者都没有**时必须拒绝启动：否则 `World::with_maps(vec![])` 是个空世界，客户端进图只能
/// 回落 0 号占位图/静默黑屏（卡面 ④ 明禁的静默降级）。
///
/// - `maps` 非空（配置写了 `WorldMaps`）⇒ `Ok(maps)`：占位尺寸，调用方记 WRN（开发/测试形态）；
/// - `maps` 为空 ⇒ `Err`：既没有 `Envir/MapInfo.txt` 也没有 `WorldMaps` ⇒ fail-closed。
///
/// # Errors
/// `maps` 为空 ⇒ `Err(说明)`；调用方据此非零退出。
pub fn placeholder_sizes(maps: &[(i32, i32)]) -> Result<Vec<(i32, i32)>, String> {
    if maps.is_empty() {
        return Err(
            "既没有 Envir/MapInfo.txt 也没有 WorldMaps ⇒ 拒绝以占位图启动（fail-closed）"
                .to_string(),
        );
    }
    Ok(maps.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;
    use mir2_envir::{CELL_BYTES, HEADER_BYTES};
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// **W-1 修复**：用例级唯一临时目录。
    ///
    /// 旧口径 `gs-mapwire-<tag>-<pid>` 在同一进程内被多个用例共用（尤其 `TempEnv::new("nowm")`
    /// 与 `no_envir_and_no_worldmaps_is_fail_closed`）⇒ 并行 test 线程互相 `remove_dir_all`，
    /// `cargo test --workspace` 随机判红（reviewer-1 `893a4ae2` 实测 7 次红 2 次）。
    /// 这里用 **pid + 原子计数**（同进程内每调用递增）保证唯一，不再跨用例共享。
    fn unique_temp(tag: &str) -> PathBuf {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("gs-mapwire-{tag}-{}-{n}", std::process::id()))
    }

    /// 生成一张全 `Walk` 的合成 `.map`（布局见 `mir2_envir` 的头：52B 头写 W/H，随后每格 12B）。
    fn synth_map(width: u16, height: u16) -> Vec<u8> {
        let n = usize::from(width) * usize::from(height);
        let mut buf = vec![0u8; HEADER_BYTES + n * CELL_BYTES];
        buf[0..2].copy_from_slice(&width.to_le_bytes());
        buf[2..4].copy_from_slice(&height.to_le_bytes());
        buf
    }

    struct TempEnv {
        dir: PathBuf,
    }

    impl TempEnv {
        /// `[0 town]`（逻辑名 = 文件名）与 `[0129|0100 zuma]`（逻辑名 ≠ 文件名）。
        /// 描述用 **ASCII**：`decode_text` 按 GBK 解码，UTF-8 中文会把后面的 `]` 当双字节尾吃掉
        /// （探针踩坑）——本用例只关心名字/文件名，不引入该干扰。
        fn new(tag: &str) -> Self {
            let dir = unique_temp(tag);
            std::fs::remove_dir_all(&dir).ok();
            std::fs::create_dir_all(dir.join("Envir")).unwrap();
            std::fs::create_dir_all(dir.join("Map")).unwrap();
            std::fs::write(
                dir.join("Envir").join("MapInfo.txt"),
                "[0 town]\n[0129|0100 zuma]\n",
            )
            .unwrap();
            std::fs::write(dir.join("Map").join("0.map"), synth_map(3, 3)).unwrap();
            std::fs::write(dir.join("Map").join("0100.map"), synth_map(5, 4)).unwrap();
            Self { dir }
        }
    }

    impl Drop for TempEnv {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.dir).ok();
        }
    }

    fn registry(env: &TempEnv) -> MapRegistry {
        match load_registry(&env.dir, "Envir", "Map") {
            Maps::Envir(reg) => reg,
            Maps::Placeholder => panic!("合成 Envir 不该是 Placeholder"),
            Maps::Broken(why) => panic!("合成 Envir 不该是 Broken：{why}"),
        }
    }

    #[test]
    fn loads_registry_and_maps_logical_name_to_id() {
        let env = TempEnv::new("ok");
        let reg = registry(&env);
        assert_eq!(reg.len(), 2, "两张都能入表");
        // `map_id` = MapInfo.txt 的顺序。
        assert_eq!(resolve_map_id(Some(&reg), "0").unwrap(), 0);
        assert_eq!(resolve_map_id(Some(&reg), "0129").unwrap(), 1);
        // 逻辑名 ≠ 文件名时，id 认的是**逻辑名**。
        assert_eq!(reg.name_of(1), Some("0129"));
        assert_eq!(sizes(&reg), vec![(3, 3), (5, 4)]);
    }

    /// 找不到逻辑名 ⇒ `Err`（`require_id`），**不许**静默退回 0 号图（这是集成点 ④ 的硬要求）。
    #[test]
    fn unknown_cur_map_is_rejected() {
        let env = TempEnv::new("reject");
        let reg = registry(&env);
        assert!(
            resolve_map_id(Some(&reg), "不存在的地图").is_err(),
            "注册表内没有 ⇒ 必须 Err"
        );
        assert!(
            resolve_map_id(Some(&reg), "0100").is_err(),
            "文件名不是逻辑名 ⇒ 也不能命中（MapManager 的键是逻辑名）"
        );
    }

    /// **issue `8d07694d` ①**：`resolve_enter_map` 先 `primary`，失败且有非空 `fallback` 时按
    /// oracle `FindMap==null ⇒ MapName=HomeMap`（`WorldServer.cs:319-341`）回落。
    /// 红检：去掉回落支（只 `resolve_map_id(primary)`）⇒ 本用例的 `("0", 0)` 与 `("0129",1)` 断言必红。
    #[test]
    fn enter_map_falls_back_to_home_map() {
        let env = TempEnv::new("homefall");
        let reg = registry(&env);
        // primary 空（新角色 CurMap 空）⇒ 用 fallback（记录 HomeMap）。
        assert_eq!(
            resolve_enter_map(Some(&reg), "", "0").unwrap(),
            ("0".to_string(), 0)
        );
        // primary 非法 ⇒ 也回落。
        assert_eq!(
            resolve_enter_map(Some(&reg), "不存在", "0129").unwrap(),
            ("0129".to_string(), 1)
        );
        // primary 命中 ⇒ 用 primary（不走 fallback）。
        assert_eq!(
            resolve_enter_map(Some(&reg), "0129", "0").unwrap(),
            ("0129".to_string(), 1)
        );
        // 两者都找不到 ⇒ Err（fail-closed，不静默退回 0 号图）。
        assert!(resolve_enter_map(Some(&reg), "", "也不存在").is_err());
        // fallback 为空 ⇒ 与单发 resolve_map_id 同（Err）。
        assert!(resolve_enter_map(Some(&reg), "", "").is_err());
    }

    /// 无 Envir ⇒ `Placeholder`；`resolve_map_id` 走 S1 占位回退（`Ok(0)`，由调用方记 WRN）。
    #[test]
    fn missing_envir_falls_back_to_placeholder() {
        let dir = unique_temp("none");
        std::fs::remove_dir_all(&dir).ok();
        assert!(matches!(
            load_registry(&dir, "Envir", "Map"),
            Maps::Placeholder
        ));
        assert_eq!(resolve_map_id(None, "随便").unwrap(), 0);
    }

    /// **红检 ①**：真 `Server.conf` **没有** `WorldMaps`（`config` 解析出空表）时，只要 `Envir/MapInfo.txt`
    /// 可读，就必须用真注册表建世界 ⇒ 空 `WorldMaps` 不阻塞启动。
    #[test]
    fn envir_registry_covers_missing_worldmaps() {
        let env = TempEnv::new("nowm");
        let reg = registry(&env);
        let maps: Vec<(i32, i32)> = Vec::new(); // = 配置里没写 WorldMaps
        assert!(
            placeholder_sizes(&maps).is_err(),
            "无 Envir + 无 WorldMaps 才 fail-closed；这里 Envir 在，不该走该支"
        );
        assert_eq!(sizes(&reg), vec![(3, 3), (5, 4)], "真注册表尺寸取代占位");
        assert_eq!(resolve_map_id(Some(&reg), "0").unwrap(), 0);
    }

    /// **红检 ②**：无 Envir（`Placeholder`）且没写 `WorldMaps` ⇒ *必须* fail-closed（非零退出），
    /// 不许静默起空世界。改回"静默放行"/"`WorldMaps` 必需键"两种方向都应被这一组用例判红。
    #[test]
    fn no_envir_and_no_worldmaps_is_fail_closed() {
        let dir = unique_temp("nowm");
        std::fs::remove_dir_all(&dir).ok();
        assert!(matches!(
            load_registry(&dir, "Envir", "Map"),
            Maps::Placeholder
        ));
        let err = placeholder_sizes(&[]).expect_err("无 Envir + 无 WorldMaps 必须 Err");
        assert!(err.contains("fail-closed"), "诊断：{err}");
        // 有 WorldMaps（无 Envir）⇒ 允许走占位尺寸（开发/测试形态，调用方记 WRN）。
        assert_eq!(placeholder_sizes(&[(64, 64)]).unwrap(), vec![(64, 64)]);
    }

    /// **R3-1**：`MapInfo.txt` **在**、但 0 条有效 section（空表/全注释）⇒ `Broken`，不是 `Placeholder`；
    /// `Placeholder` 只留给 `MapInfo.txt` **不存在**。
    #[test]
    fn empty_map_info_file_is_broken_not_placeholder() {
        let env = TempEnv::new("empty");
        std::fs::write(
            env.dir.join("Envir").join("MapInfo.txt"),
            "; 只有注释，没有 section\n\n",
        )
        .unwrap();
        match load_registry(&env.dir, "Envir", "Map") {
            Maps::Broken(why) => assert!(why.contains("0 条有效 section"), "诊断：{why}"),
            Maps::Placeholder => {
                panic!("MapInfo.txt 在但空表 ⇒ 必须是 Broken，不许当 Placeholder 静默占位")
            }
            Maps::Envir(_) => panic!("空 MapInfo 却加载成功？"),
        }
    }

    /// **R3-3**：显式相对 `--config`（`Server.conf`）时 base 必须规范到**当前目录**，不是空路径。
    #[test]
    fn relative_conf_base_is_normalized_to_cwd() {
        let cwd = std::env::current_dir().unwrap();
        assert_eq!(
            base_path(Path::new("Server.conf")),
            cwd,
            "裸相对置文件名 ⇒ base = cwd（不是 Some(\"\") 推出的空路径）"
        );
        assert_eq!(base_path(Path::new("sub/Server.conf")), cwd.join("sub"));
    }

    /// `MapInfo.txt` 在但一张都没加载成功 ⇒ `Broken`（**不是** `Placeholder`）——
    /// 这样 `main` 会拒绝启动，不会静默退回 0 号占位图。
    #[test]
    fn all_maps_failing_is_broken_not_placeholder() {
        let env = TempEnv::new("broken");
        std::fs::remove_file(env.dir.join("Map").join("0.map")).unwrap();
        std::fs::remove_file(env.dir.join("Map").join("0100.map")).unwrap();
        match load_registry(&env.dir, "Envir", "Map") {
            Maps::Broken(why) => assert!(why.contains("一张都没加载成功"), "诊断：{why}"),
            Maps::Placeholder => panic!("有 section 但全失败 ⇒ 必须是 Broken，不许当 Placeholder"),
            Maps::Envir(_) => panic!("没有 .map 文件却加载成功？"),
        }
    }
}
