//! Envir 真地图注册表接入（`openmir2-m2-map-loading` 的集成点 ④：`CurMap` → `map_id`）。
//!
//! ## 装载把「`Valid` 判据」全面激活（2026-10-17 独立复验 must_fix ③；**tier=R 窗口必核项**）
//!
//! 本片之前每格皆 `Walk` ⇒ 所有读 `get_cell_info(...).ok` / `Valid` 的门在真机**恒真**（形同虚设）。
//! 装载后**真机可达**的变化面（门形与 oracle 逐条抽核同构，**未穷尽** 30 余处消费点）：
//! - **刷怪（最重）**：`bins/game-svr/src/mongen.rs` 的地形移位环（oracle `WorldServer.MonGen.cs` 的
//!   31 次 `CanWalk` 环）由 `cell_walkable_ignore_actors`/`can_walk` 驱动，此前恒真 ⇒ 环从不进入。
//!   装载后真机 `Envir/MonGen.txt` **2827 条里 1072 条**声明出生格是墙格（755 LowWall、306 HighWall、
//!   11 越界；`D001(200,200)`=HighWall、`3(339,313)`=LowWall、`Q011(75,75)`=LowWall 三点经 `.map`
//!   二进制独立复核），其中 **40 条首轮就进入抽随机数的边界支**（`Random(Width/2)`/`Random(Height/2)`
//!   ）⇒ **刷怪位置与 RNG 流水都会变**（向 oracle 收敛，但确为本片引入的、真机可达的 RNG/时序面变化）。
//! - route 落格 **1658 → 1649**（9 条落墙格 = 7 HighWall + 2 LowWall；oracle `AddMapRoute` 的
//!   `cellSuccess && Valid` 门同样丢）。
//! - 其余同族消费点：进图/落格（`add_map_object`）、地面物品（`AddItemToMap`）、AOI 视距
//!   （`BaseObject.ViewRange`）、`CanWalk(...,true)` 冲刺、`cetin_near_xy`（spell 路径 snap）。
//!
//! ## 真机静态普查（2026-10-17，卡 `openmir2-mapcell-consumer-teeth`；**只读部署件**）
//!
//! 用 `mir2_envir` 解析器（`MapLoader`/`MapRegistry`/`MapFile::attribute`/`npcs`/`merchant`）普查
//! `E:/MirServer/M2GameSvr`。复核命令 = `crates/envir/tests/real_maps.rs` 的 `deployment_wall_census`（`#[ignore]` **只读**
//! 工具，不进常驻测试）：
//! `cargo test -p mir2-envir --test real_maps -- --ignored --nocapture`（`MIR2_MAP_DIR` 默认 `E:/MirServer/M2GameSvr/Map`）。
//! 独立复核（python 裸读 `.map`、无 cargo）：见审计报告
//! `tests/parity/m3/audit-valid-consumers-20261017.md` §6 R8。⚠ R8 的 glob 扫 `Map/` 全部
//! **572** 张 `.map`；**322** 条 section 指向 **314** 张不同文件（8 条是 `a|b` 别名）、
//! 其余 **258** 张未注册、不进世界 ⇒ R8 的「Walk cell0」是 **14** 张 = 本表 **5** 张注册图 +
//! **9** 张未注册图（`613`/`T1361`/`T1362`/`T2362`/`T3361`/`T3362`/`W0`/`W1`/`W2`），不是分歧。
//!
//! ### ① `(0,0)` 属性普查 —— 防把 `move_object` 的 cell0 耦合误读为回归
//!
//! 真机 `MapInfo.txt` 注册 **322** 张图：`(0,0)` 为**墙** **317** 张，为 `Walk` 仅 **5** 张
//! （`T2361` id=13 / `E702` 118 / `D10031` 226 / `4` 236 / `H009` 300；含字面 `0.map`=id26 ⇒ `HighWall`，
//! 与 `b192bd9f3` 正文④的锚一致）。Rust `MapGrid` 列主序 `index(0,0)==0` ⇒ `get_cell_info` 失败回落的
//! `cells[0]` **就是** `(0,0)`。`map.rs::move_object` 的
//! `let (cell,_) = get_cell_info(to_x,to_y); if !cell.valid() { return false; }` 与 oracle
//! `Envirnoment.MoveToMovingObject` 的 `if (canMove && cellInfo.Valid)`（`Envirnoment.cs:380`，
//! 目标非 `Valid` 时 `cellInfo` 同为 `CellArray[0]`）**逐行同形** ⇒
//! - `(0,0)` 是墙（317/322）：目标墙格 ⇒ `cells[0].valid()==false` ⇒ 早退、**不摘源格**；
//! - `(0,0)` 是 `Walk`（5/322）：目标墙格 ⇒ 摘源格、不落目标格（`add_map_object` 因墙返回 `false`，
//!   `move_object` 整体仍返回 `false`）。
//!
//! **两种都与 oracle 同形；且真机走位路径都够不到该分支**：生产走位 `handle_move_cmd` 走
//! `World::step_with_route`→`try_step_now`→`cell_free`（`world.rs`：墙目标在 `cell_free` 即被拒），
//! 无门控的 `Command::Walk`→`try_step`（`world.rs`）**生产无调用方**（`World::walk` 只被测试用）。
//! ⇒ 真机不触发；此段登记以免后人误读为 mapcell 片引入的回归。
//!
//! ### ② NPC / 商人「配置格是墙 ⇒ 静默移除」的逐条普查
//!
//! 真机 `Envir/Npcs.txt` **3** 条、落墙格 **0** 条；`Envir/Merchant.txt`（盘上 `MerChant.txt`）
//! **131** 条、落墙格 **0** 条 ⇒ **真机不发生任何移除**。Rust `World::attach_merchant_to_map` /
//! `attach_npc_to_map`（`merchant.rs`/`npc.rs`）与 oracle `BaseObject.Initialize`
//! （`BaseObject.Base.cs:13-21` 的 `CanWalk(CurrX,CurrY,true) && AddToMap()`）以及
//! `MerchantInitialize`/`NpCinitialize`（`WorldServer.Merchant.cs`）的静默 `RemoveAt` **逐条同形**。
//! **登记一处潜在日志差异（真机 0 移除 ⇒ 两侧都无该日志、当前不可观测）**：oracle 逐条
//! `LogService.Warn("Merchant/Npc Initalize fail..."+名+图+坐标)`；宿主只打**一条** `INF` 汇总
//! （`bins/game-svr/src/main.rs` 的「挂图失败 {} 条」），**无逐条 WRN**。语料若变化须复核本行。
//!
//! ### ③ 走位碰撞牙（进程级）
//!
//! 装载让 `Valid==Walk` 真机生效 ⇒ 玩家走位碰撞（`world.rs::cell_free`，oracle `Envirnoment.CanWalk`）
//! 对墙格由「恒可走」变「不可走」。构造级钉死：`bins/game-svr` 的
//! `tests::move_cmd_blocked_by_wall_cell_sends_no_move_frame`（走生产入口 `handle_move_cmd`；
//! 两条变异牙：摘 `cell_free` 属性门 ⇒ 源格 orphan 必红；`MapCellInfo::valid()` 恒真 ⇒ 走上墙格必红）。
//! 窗口必核项见 W0 runbook。
//!
//! oracle 路径：`M2Share.BasePath`（可执行体目录）+ `Server.conf` 的 `[Share] EnvirDir`/`MapDir`
//! （`GameSvrConf.EnvirDir/MapDir` 的默认值分别是 `Envir`/`Map`，`GameSvrConf.cs:1295-1296`）
//! → `Envir/MapInfo.txt` 的 section 先后顺序就是 `map_id`（见 `mir2_envir::MapRegistry` 的头注释）。
//! 角色记录里的 `CurMap` 是**逻辑地图名**，必须经 `MapRegistry::require_id` 换成 `map_id`；
//! **找不到即拒绝进图**（返回 `Err`），不许静默退回 0 号占位图。
//!
//! 本模块做「加载 + 取尺寸 + 换 id」三件事，**外加**静态装载的最后一个搬运步骤
//! （[`apply_map_cells`]：把 `mir2_envir` 已解析的每格属性灌进 `world.maps`——本仓
//! `crates/world` 不依赖 `crates/envir`，跨 crate 的搬运只能落在宿主）。不碰 `crates/envir`
//! 内部，也不建世界（那是 `sim_loop` 的事）。`WorldMaps`（S1 占位尺寸）作为**没有 Envir**（开发/测试）时的回退保留并登记：
//! 真机验收（始终有 `Envir/MapInfo.txt`）之后应删掉占位键。三种加载结果的处置见 [`Maps`]。

use std::path::{Path, PathBuf};

use mir2_envir::{MapFile, MapLoader, MapRegistry};
use mir2_world::{CellAttribute, MapGrid, World};

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

/// **静态装载**：把**一张**已解析的 `.map`（`mir2_envir::MapFile`）的格子属性灌进同尺寸的
/// `MapGrid`——oracle `Envirnoment.LoadMapData`（`src/M2Server/Maps/Envirnoment.cs:903`/`943-951`）
/// 在加载时就把 `CellArray[nW * Height + nH].Attribute` 写死，本函数是它在世界侧的落点。
///
/// **判据来源**：属性只从 `mir2_envir` 已解析的那份取（[`MapFile::attribute`]，
/// 其判据是 `wBkImg` bit15 ⇒ `HighWall`、`wFrImg` bit15 ⇒ `LowWall`、**先判 High 再判 Low
/// ⇒ Low 覆盖 High**，见 `crates/envir/src/mapfile.rs` 模块头第 4/5 条）——**不另写一套解析**。
///
/// 顺序：逐 `x` 外层、逐 `y` 内层填 `k = x * Height + y`，与 [`MapGrid::set_cell_attributes`]
/// 的列主序**同序**（也是 `.map` 的文件序）。
///
/// 尺寸不符 ⇒ `MapGrid::set_cell_attributes` 的 fail-closed 生效（返回 0、整图保持 `Walk`）。
/// 返回实际写入的格子数。
pub fn apply_map_file_cells(f: &MapFile, grid: &mut MapGrid) -> usize {
    let capacity = (f.width as usize).saturating_mul(f.height as usize);
    let mut attrs: Vec<CellAttribute> = Vec::with_capacity(capacity);
    for x in 0..f.width {
        for y in 0..f.height {
            attrs.push(f.attribute(x, y));
        }
    }
    grid.set_cell_attributes(&attrs)
}

/// **静态装载**：把注册表里每张 `.map` 的格子属性灌进 `world.maps`（`map_id` = 下标，
/// 与 [`sizes`]/[`resolve_map_id`] 同源；尺寸也同源 ⇒ 逐图长度必然等于 `W*H`）。
///
/// **调用时机（oracle 次序，别调换）**：必须在 `World::add_map_routes` **之前**。oracle 先
/// `Envirnoment.LoadMapData`（写属性，`Envirnoment.cs:903`/`943-951`）、后 `Maps.LoadMapInfo` 尾段
/// 的 `MapManager.AddMapRoute`（`src/GameSrv/Maps/Maps.cs:361-379`）；而 `AddMapRoute` 有
/// `cellSuccess && cellInfo.Valid` 门（`Envirnoment.cs:176`）⇒ **墙格上的连接点在 oracle 侧本来
/// 就落不下**。属性灌入前每格都是 `Walk`，`crates/world/src/map.rs` 的 `add_map_route` 那道具
/// 形同虚设；本函数把次序摆正后，那道门才真正生效（墙格 route 被丢 ⇒ `added < routes.len()`）。
///
/// 返回实际写入的格子总数（各图 `W*H` 之和；有图长度不符时会更小 ⇒ 调用方打印该值自证）。
///
/// **静默边界（登记）**：若 `world.maps` 比注册表**短**，本函数 `break` 掉后续图（生产里
/// `sizes(reg)` 与 `reg.maps()` 同源 ⇒ 不可达；日志的 `cells` 变小是唯一线索）。同理
/// `MapGrid::set_cell_attributes` 遇长度不符会**整图保持 Walk** 并返 0（fail-closed，不半灌）。
pub fn apply_map_cells(reg: &MapRegistry, world: &mut World) -> usize {
    let mut total = 0usize;
    for (map_id, f) in reg.maps().iter().enumerate() {
        let Some(grid) = world.maps.get_mut(map_id) else {
            break;
        };
        total += apply_map_file_cells(f, grid);
    }
    total
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

    // ============================ 片 `openmir2-mapcell-attributes-loading` ============================
    // 静态装载（`.map` 格属性 → `world.maps` 的 `MapGrid`）的判据与牙。判据来源恒为
    // `mir2_envir` 已解析的那份（`MapFile::attribute`），**不另写解析**。

    /// 合成 `.map` 一格的三态墙（`wBkImg`/`wFrImg` 的 bit15）。
    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    enum WallKind {
        /// `wBkImg` bit15 ⇒ `HighWall`。
        High,
        /// `wFrImg` bit15 ⇒ `LowWall`。
        Low,
        /// **两位都置** ⇒ oracle 先判 High 再判 Low ⇒ `LowWall`（`crates/envir/src/mapfile.rs` 第 5 条）。
        Both,
    }

    /// 生成一张 `width×height`、按 `walls` 置位的合成 `.map`（其余格全零 ⇒ `Walk`）。
    ///
    /// 每格 12B：`wBkImg u16`、`wMidImg u16`、`wFrImg u16`、6×u8；文件序 `k = x * Height + y`。
    fn synth_map_walls(width: u16, height: u16, walls: &[(u16, u16, WallKind)]) -> Vec<u8> {
        let n = usize::from(width) * usize::from(height);
        let mut buf = vec![0u8; HEADER_BYTES + n * CELL_BYTES];
        buf[0..2].copy_from_slice(&width.to_le_bytes());
        buf[2..4].copy_from_slice(&height.to_le_bytes());
        for &(x, y, kind) in walls {
            let k = usize::from(x) * usize::from(height) + usize::from(y);
            let off = HEADER_BYTES + k * CELL_BYTES;
            if matches!(kind, WallKind::High | WallKind::Both) {
                buf[off..off + 2].copy_from_slice(&0x8000u16.to_le_bytes());
            }
            if matches!(kind, WallKind::Low | WallKind::Both) {
                buf[off + 4..off + 6].copy_from_slice(&0x8000u16.to_le_bytes());
            }
        }
        buf
    }

    /// 装载夹具：`0.map` 6×6（1 `High` + 2 `Low` + 1 **Both**⇒`Low`）+ 2 条 route
    /// （`(2,2)` 落**墙格**、`(5,5)` 落可走格）；`0100.map` 5×4 全 `Walk`（阴性对照图）。
    struct WallsEnv {
        dir: PathBuf,
    }

    impl WallsEnv {
        fn new() -> Self {
            let dir = unique_temp("walls");
            std::fs::remove_dir_all(&dir).ok();
            std::fs::create_dir_all(dir.join("Envir")).unwrap();
            std::fs::create_dir_all(dir.join("Map")).unwrap();
            // 段 0 = 逻辑名 `0`（文件 `0.map`）；段 1 = 逻辑名 `1`（文件 `0100.map`）。
            // 两条非 section 行 = route（`Maps.cs:361-379` 的 `源图 源x,源y 目标图 目标x,目标y`）。
            std::fs::write(
                dir.join("Envir").join("MapInfo.txt"),
                "[0 town]\n[1|0100 zuma]\n0 2,2 -> 1 1,1\n0 5,5 -> 1 1,2\n",
            )
            .unwrap();
            std::fs::write(
                dir.join("Map").join("0.map"),
                synth_map_walls(
                    6,
                    6,
                    &[
                        (1, 0, WallKind::High),
                        (2, 2, WallKind::Low),
                        (4, 4, WallKind::Both),
                        (0, 5, WallKind::Low),
                    ],
                ),
            )
            .unwrap();
            std::fs::write(dir.join("Map").join("0100.map"), synth_map(5, 4)).unwrap();
            Self { dir }
        }
    }

    impl Drop for WallsEnv {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.dir).ok();
        }
    }

    fn registry_dir(dir: &Path) -> MapRegistry {
        match load_registry(dir, "Envir", "Map") {
            Maps::Envir(reg) => reg,
            Maps::Placeholder => panic!("夹具 Envir 不该是 Placeholder"),
            Maps::Broken(why) => panic!("夹具 Envir 不该是 Broken：{why}"),
        }
    }

    /// **本片主判据（直方图逐值相等 + 3 格抽查，tier=R）**：`apply_map_cells` 之后，每张图的
    /// `Walk`/`LowWall`/`HighWall` 计数必须与 `mir2_envir` 解析**同一张** `.map` 得到的三计数
    /// **逐值相等**；并与 oracle 定案导出的**硬编码期望值**一致（后一半才钉得住「Low 覆盖 High」：
    /// 若只在 envir 侧把次序判反，两侧会"一起错"，world↔envir 的相等断言仍绿）。
    ///
    /// 夹具硬编码期望（oracle `Envirnoment.cs:945-951`：先 `wBkImg` 再 `wFrImg`）：
    /// - `0.map` 6×6=36 格：`(1,0)`=High、`(2,2)`=Low、`(4,4)`=Both⇒**Low**、`(0,5)`=Low
    ///   ⇒ `walk=32 low=3 high=1`；
    /// - `0100.map` 5×4=20 格：全 `Walk` ⇒ `walk=20 low=0 high=0`。
    ///
    /// 红检（变异牙，实测见补丁说明）：
    /// ① 把 `apply_map_cells` 改成直接 `return 0`（= 装载整段删掉）⇒ world 侧恒全 `Walk`
    ///    ⇒ 本测的计数断言与 `get_cell_info` 的 `ok=false` 断言必红；
    /// ② 把 `MapCell::attribute` 的 High/Low 判定**对调**（`high_wall()` 先判）⇒ `(4,4)` 变 `HighWall`
    ///    ⇒ `low=3/high=1` 硬编码断言必红；
    /// ③ 只灌 `HighWall` 不灌 `LowWall`（去掉 `low_wall()` 支）⇒ `(2,2)`/`(4,4)` 变 `Walk`
    ///    ⇒ `walk=32` 断言必红。
    #[test]
    fn loaded_cell_histogram_matches_envir_and_oracle_constants() {
        let env = WallsEnv::new();
        let reg = registry_dir(&env.dir);
        let mut world = World::with_maps(sizes(&reg));
        let total = apply_map_cells(&reg, &mut world);

        // ① world ↔ envir 逐图逐值相等（三计数 + 和 = W*H）。
        for (map_id, f) in reg.maps().iter().enumerate() {
            let g = &world.maps[map_id];
            assert_eq!(
                (g.walk_count(), g.low_wall_count(), g.high_wall_count()),
                (f.walk_count(), f.low_count(), f.high_count()),
                "map_id={map_id}（{}）装载后直方图必须与 envir 解析值逐值相等",
                f.map_name
            );
            assert_eq!(
                g.walk_count() + g.low_wall_count() + g.high_wall_count(),
                (f.width as usize) * (f.height as usize),
                "三态计数之和 = W*H（不许有第四态/漏格）"
            );
        }

        // ② 硬编码期望值（oracle 定案；钉住 Low 覆盖 High + `wMidImg` 不参与）。
        let m0 = &world.maps[0];
        assert_eq!(
            (m0.walk_count(), m0.low_wall_count(), m0.high_wall_count()),
            (32, 3, 1),
            "0.map 6×6：1 High + 2 Low + 1 Both(⇒Low) ⇒ walk=32 low=3 high=1"
        );
        let m1 = &world.maps[1];
        assert_eq!(
            (m1.walk_count(), m1.low_wall_count(), m1.high_wall_count()),
            (20, 0, 0),
            "0100.map 5×4 全 Walk ⇒ walk=20"
        );

        // ③ 抽查 3 格（属性 + `ok`）+ 1 个 Both 格（Low 覆盖 High 的单格证据）。
        let spot = |x: i32, y: i32| {
            (
                world.maps[0].cell_attribute(x, y),
                world.maps[0].get_cell_info(x, y).1,
            )
        };
        assert_eq!(
            spot(1, 0),
            (Some(CellAttribute::HighWall), false),
            "抽查①：wBkImg bit15 ⇒ HighWall 且 ok=false"
        );
        assert_eq!(
            spot(2, 2),
            (Some(CellAttribute::LowWall), false),
            "抽查②：wFrImg bit15 ⇒ LowWall 且 ok=false"
        );
        assert_eq!(
            spot(0, 0),
            (Some(CellAttribute::Walk), true),
            "抽查③：无墙位 ⇒ Walk 且 ok=true"
        );
        assert_eq!(
            spot(4, 4),
            (Some(CellAttribute::LowWall), false),
            "两位都置 ⇒ Low 覆盖 High（先判 High 再判 Low）"
        );
        // 写入格数自证：各图 W*H 之和。
        let expect_total: usize = reg
            .maps()
            .iter()
            .map(|m| (m.width as usize) * (m.height as usize))
            .sum();
        assert_eq!(total, expect_total, "写入格数 = 各图 W*H 之和");
        assert_eq!(expect_total, 36 + 20);

        // 与 envir 侧同格读数逐值相等（spot 判据的双侧一致性）。
        for (x, y) in [(1, 0), (2, 2), (0, 0), (4, 4)] {
            assert_eq!(
                world.maps[0].cell_attribute(x, y),
                Some(reg.maps()[0].attribute(x, y)),
                "({x},{y}) world↔envir 属性必须逐值相等"
            );
        }
    }

    /// **阴性对照（防「一律不可用」）**：全 `Walk` 的图装载后仍 `ok=true`、可落对象、
    /// `cetin_near_xy` 仍可命中。
    ///
    /// 红检：把 `set_cell_attributes` 改成「无视 `attrs` 一律置 `LowWall`」⇒ 本测必红。
    #[test]
    fn all_walk_map_stays_walkable_and_hittable_after_load() {
        let env = WallsEnv::new();
        let reg = registry_dir(&env.dir);
        let mut world = World::with_maps(sizes(&reg));
        apply_map_cells(&reg, &mut world);

        // `0100.map`（map_id=1）全 Walk：逐格 `ok=true`。
        for x in 0..world.maps[1].width {
            for y in 0..world.maps[1].height {
                assert!(
                    world.maps[1].get_cell_info(x, y).1,
                    "全 Walk 图 ({x},{y}) 必须 ok=true"
                );
                assert!(world.maps[1].cell_valid(x, y));
            }
        }
        // 可落对象（`AddMapObject` 的 `Valid` 门放行）。
        assert!(
            world.maps[1].add_map_object(2, 1, mir2_world::CellType::Play, 7, 0),
            "全 Walk 格仍可落对象"
        );
        // `cetin_near_xy` 仍可命中（同一格上的实体）。
        let target = world.add_entity(mir2_world::Entity::new(0, "t", 2, 1));
        world.tick();
        assert!(
            crate::queryuser::cetin_near_xy(&world, 0, 2, 1, target),
            "阴性对照：可走格上的目标必须仍被 `cetin_near_xy` 命中"
        );
        // `0.map` 的可走格 (0,0) 同理。
        assert!(world.maps[0].get_cell_info(0, 0).1);
        assert!(world.maps[0].cell_valid(0, 0));
    }

    /// **牙 ④（`cetin_near_xy` × 装载掩码，tier=R）**：`.map` 装载出的 `LowWall` 格必须让
    /// `cetin_near_xy` 的 `!ok` 支跳过它——即**掩码来源是 `.map` 文件本身**（不是
    /// `MapGrid::set_map_xy_flag` 的运行期掩码；既有掩码牙 `spell_cmd_snap_masked_cell_does_not_snap`
    /// 用的是后者）。布置沿用该牙：目标站在 3×3 窗口内、窗口中心 (1,2)、目标 (2,2)。
    ///
    /// **夹具次序说明（必须写清）**：先落实体、后灌属性——因为 `add_map_object` 有 `cell.Valid`
    /// 门，实体**进不了**墙格。生产里「实体已在场上、其格随后被掩码」只可能来自**运行期**掩码
    /// （城门/城墙结构事件 `crates/world/src/castle.rs::apply_structure_events`，归卡 `M3⑧`）；
    /// `.map` 静态装载本身发生在任何实体落图**之前**（故生产次序上本牙的「装载前后」两半不会同时
    /// 真实发生，这里是把它当**等价状态**构造出来）。两侧共同点：`get_cell_info` 对 `LowWall` 格
    /// 返回 `ok=false` ⇒ `cetin_near_xy` 跳过 ⇒ 下游 `spell_snap_target` 不 snap。
    ///
    /// 红检（变异牙）：把 `apply_map_cells` 改成 no-op（`return 0`）⇒ 装载后 (2,2) 仍 `Walk`
    /// ⇒「装载后不命中」断言必红（这正是「装载整段删掉」的必红面）。
    #[test]
    fn loaded_low_wall_cell_is_skipped_by_cetin_near_xy() {
        let env = WallsEnv::new();
        let reg = registry_dir(&env.dir);
        let mut world = World::with_maps(sizes(&reg));
        // 先在**未灌属性**（全 Walk）的世界上落实体（见用例头的次序说明）。
        let target = world.add_entity(mir2_world::Entity::new(0, "t", 2, 2));
        world.tick();
        assert!(
            crate::queryuser::cetin_near_xy(&world, 0, 1, 2, target),
            "装载前：(2,2) 是 Walk ⇒ 目标在 3×3 窗口内 ⇒ 命中"
        );

        apply_map_cells(&reg, &mut world);

        assert_eq!(
            world.maps[0].cell_attribute(2, 2),
            Some(CellAttribute::LowWall),
            "装载把 (2,2) 落成 LowWall"
        );
        assert!(
            !world.maps[0].get_cell_info(2, 2).1,
            "装载后 (2,2) 的 `ok` 必须是 false（`Valid == Attribute == Walk`）"
        );
        assert!(
            !crate::queryuser::cetin_near_xy(&world, 0, 1, 2, target),
            "装载后：目标所在格被 `.map` 掩码 ⇒ `cetin_near_xy` 跳过该格 ⇒ 不命中"
        );
        // 同窗口内的可走格不受影响（阴性面：窗口里还有可走格）。
        assert!(
            world.maps[0].get_cell_info(1, 2).1,
            "(1,2) 仍是可走格（装载不是「整窗一律不可用」）"
        );
    }

    /// **装配序牙**：oracle 先 `Envirnoment.LoadMapData`（写属性）后 `Maps.LoadMapInfo` 尾段的
    /// `MapManager.AddMapRoute`（`src/GameSrv/Maps/Maps.cs:361-379`），而 `AddMapRoute` 有
    /// `cellSuccess && cellInfo.Valid` 门（`Envirnoment.cs:176`）⇒ **落在墙格上的连接点被丢**。
    ///
    /// 本测把两种次序都跑一遍：反序（属性未灌 ⇒ 每格 Walk）**两条都落**，正序**只落可走格那条**
    /// ——前者就是本片之前的实际行为（那道门形同虚设）。
    ///
    /// 红检（变异牙）：把 `apply_map_cells` 与 `add_map_routes` 的调用序对调（或把属性灌到 route
    /// 之后）⇒ `assert_eq!(added, 1)` 必红（会变 2）。
    #[test]
    fn wall_cell_route_is_dropped_only_after_attributes_are_loaded() {
        let env = WallsEnv::new();
        let reg = registry_dir(&env.dir);
        assert_eq!(reg.routes().len(), 2, "夹具两条 route 都解析成功");
        let specs: Vec<mir2_world::map::MapRouteSpec> = reg
            .routes()
            .iter()
            .map(|r| mir2_world::map::MapRouteSpec {
                map_id: r.src_map_id,
                x: r.src_x,
                y: r.src_y,
                route_id: r.route_id,
                hop: mir2_world::map::RouteHop {
                    dst_map_id: r.dst_map_id,
                    dst_x: r.dst_x,
                    dst_y: r.dst_y,
                },
            })
            .collect();

        // 反序（本片之前的实际行为）：属性未灌 ⇒ 墙格也是 Walk ⇒ 两条都落。
        let mut wrong = World::with_maps(sizes(&reg));
        assert_eq!(
            wrong.add_map_routes(specs.clone()),
            2,
            "属性未灌 ⇒ 连落在墙格 (2,2) 的 route 也落（oracle 不该如此）"
        );
        apply_map_cells(&reg, &mut wrong); // 补救：属性晚到（route 已在墙上）

        // 正序（oracle 次序）：先灌属性 ⇒ 墙格那条被 `cell.Valid` 门丢掉。
        let mut right = World::with_maps(sizes(&reg));
        apply_map_cells(&reg, &mut right);
        assert_eq!(
            right.add_map_routes(specs),
            1,
            "先灌属性 ⇒ 落在墙格 (2,2) 的 route 被丢；可走格 (5,5) 那条照落"
        );
        assert!(!right.maps[0].get_cell_info(2, 2).1, "(2,2) 是墙格（前置）");
        assert_eq!(right.maps[0].cell(5, 5).count(), 1, "可走格 route 已落格");
    }

    /// **生产调用点保鲜（源码锚）**：`sim_loop` 必须在 `world.add_map_routes(specs)` **之前**
    /// 调用 `mapwire::apply_map_cells(reg, &mut world)`，且两者都在 `if let Some(reg) = registry.as_ref()`
    /// 支内。
    ///
    /// 本仓既有同类样板（`bins/game-svr/src/main.rs:10927` 的 `include_str!("main.rs")` 锚点保鲜）。
    /// 作用：把「生产路径真的灌了、且次序对」钉进常驻测试——单测覆盖不到 `sim_loop`（巨型函数、
    /// 起真宿主），源码锚是这条**调用点**在套件内唯一可判的形态。删掉调用点/把两条调用对调 ⇒ 必红。
    #[test]
    fn production_sim_loop_loads_cells_before_routes() {
        let src = include_str!("main.rs");
        let branch = src
            .find("if let Some(reg) = registry.as_ref()")
            .expect("sim_loop 的注册表支");
        let cells = src
            .find("mapwire::apply_map_cells(reg, &mut world)")
            .expect("生产装载调用点必须存在（sim_loop）");
        let routes = src
            .find("let added = world.add_map_routes(specs);")
            .expect("route 装载调用点");
        assert!(
            branch < cells,
            "装载调用必须在注册表支内（有注册表才灌；无注册表 ⇒ 全 Walk 的开发形态）"
        );
        assert!(
            cells < routes,
            "装载必须在 route 落格之前（oracle：先 LoadMapData 写属性，后 AddMapRoute；Maps.cs:361-379）"
        );
    }

    /// **真机部署件向量（`#[ignore]`，需 `E:/MirServer/M2GameSvr`）**：真 `.map` 的 world 侧直方图
    /// 必须与 `mir2_envir` 解析的同一张三计数**逐值相等**，且与部署件锚值一致；并抽查 3 格。
    ///
    /// ```text
    /// cargo test -p game-svr --bin game-svr -- --ignored --nocapture loaded_real_map_histogram
    /// ```
    #[test]
    #[ignore = "真机部署件向量：需 E:/MirServer/M2GameSvr（证据/owner 跑）"]
    fn loaded_real_map_histogram_matches_deployment() {
        let base = PathBuf::from(
            std::env::var("MIR2_BASE_DIR").unwrap_or_else(|_| "E:/MirServer/M2GameSvr".into()),
        );
        let loader = MapLoader::new(&base, "Map", "Envir");
        // (文件名, W, H, walk, low, high) —— 锚值取自 `crates/envir/tests/real_maps.rs`（部署件 probe）。
        let vectors: [(&str, i32, i32, usize, usize, usize); 3] = [
            ("0.map", 700, 700, 327_180, 109, 162_711),
            ("0100.map", 15, 18, 47, 223, 0),
            ("0129.map", 7, 10, 11, 0, 59),
        ];
        for (name, w, h, walk, low, high) in vectors {
            let f = loader
                .load_path(&base.join("Map").join(name))
                .unwrap_or_else(|e| panic!("加载 {name} 失败：{e}"));
            let mut grid = MapGrid::new(f.width, f.height);
            let n = apply_map_file_cells(&f, &mut grid);
            println!(
                "{name}: W={} H={} cells={n} | envir(walk={} low={} high={}) | world(walk={} low={} high={})",
                f.width,
                f.height,
                f.walk_count(),
                f.low_count(),
                f.high_count(),
                grid.walk_count(),
                grid.low_wall_count(),
                grid.high_wall_count(),
            );
            assert_eq!((f.width, f.height), (w, h), "{name} 尺寸锚");
            assert_eq!(n, (w * h) as usize, "{name} 写入格数");
            assert_eq!(
                (f.walk_count(), f.low_count(), f.high_count()),
                (walk, low, high),
                "{name} envir 侧部署件锚值"
            );
            assert_eq!(
                (
                    grid.walk_count(),
                    grid.low_wall_count(),
                    grid.high_wall_count()
                ),
                (f.walk_count(), f.low_count(), f.high_count()),
                "{name} world↔envir 直方图逐值相等"
            );
            // 抽查 3 格（互不相同）：(0,0) + 首个 LowWall 格 + 首个 HighWall 格。
            let mut picks: Vec<(i32, i32)> = vec![(0, 0)];
            let push_first = |attr: CellAttribute, picks: &mut Vec<(i32, i32)>| {
                if picks.iter().any(|(x, y)| f.attribute(*x, *y) == attr) {
                    return;
                }
                for x in 0..f.width {
                    for y in 0..f.height {
                        if f.attribute(x, y) == attr {
                            picks.push((x, y));
                            return;
                        }
                    }
                }
            };
            push_first(CellAttribute::LowWall, &mut picks);
            push_first(CellAttribute::HighWall, &mut picks);
            for (x, y) in picks {
                println!(
                    "  {name} ({x},{y}): envir={:?} world={:?} ok={}",
                    f.attribute(x, y),
                    grid.cell_attribute(x, y),
                    grid.get_cell_info(x, y).1
                );
                assert_eq!(
                    grid.cell_attribute(x, y),
                    Some(f.attribute(x, y)),
                    "{name} ({x},{y}) 属性逐值相等"
                );
                assert_eq!(
                    grid.get_cell_info(x, y).1,
                    f.attribute(x, y) == CellAttribute::Walk,
                    "{name} ({x},{y}) `ok` 必须 == (属性 == Walk)"
                );
            }
        }
    }

    /// **真机影响面（`#[ignore]`）**：真注册表逐图装载后的三计数 / 格数 + 「有多少条 route 落在
    /// **墙格**上」（= 本片之前那条 route 会落、oracle 侧本该被丢 ⇒ 本片会改变 `added` 的行数）。
    ///
    /// 打印的值就是生产 `sim_loop` 那条 `INF 地图格子属性已灌入` 日志（`cells/walk/low/high`）与
    /// `INF 地图连接点已灌入（MapRoute）routes=… added=…` 日志的**等价读数**（本测不建 world 网格：
    /// 只累计 `MapFile` 侧计数，避免 17M 格的 `MapGrid` 常驻）。
    ///
    /// ```text
    /// cargo test -p game-svr --bin game-svr -- --ignored --nocapture real_registry_route_cells_on_wall
    /// ```
    #[test]
    #[ignore = "真机部署件向量：需 E:/MirServer/M2GameSvr（证据/owner 跑）"]
    fn real_registry_route_cells_on_wall() {
        use std::collections::HashMap;
        let base = PathBuf::from(
            std::env::var("MIR2_BASE_DIR").unwrap_or_else(|_| "E:/MirServer/M2GameSvr".into()),
        );
        let loader = MapLoader::new(&base, "Map", "Envir");
        let t0 = std::time::Instant::now();
        let entries = loader.load_map_info_entries().expect("MapInfo 加载");
        // **键归一化**：oracle 侧 `MapManager` 的表是 `StringComparer.OrdinalIgnoreCase`、本仓注册表用
        // `normalize_name = to_ascii_lowercase` ⇒ 本测也必须小写化，否则 `h006` 匹配不到 `H006`，会凭空
        // 多出一条「未加载」并让 `added` 少算 1（独立复验 2026-10-17 实测到的伪影）。
        let name_to_file: HashMap<String, String> = entries
            .iter()
            .map(|e| (e.map_name.to_ascii_lowercase(), e.map_file_name.clone()))
            .collect();
        let routes = loader.load_map_routes().expect("route 加载");
        let mut cache: HashMap<String, Option<MapFile>> = HashMap::new();
        let (mut on_wall, mut unresolved) = (0usize, 0usize);
        for r in &routes {
            let Some(fname) = name_to_file.get(&r.src_map_name.to_ascii_lowercase()) else {
                unresolved += 1;
                continue;
            };
            let f = cache
                .entry(fname.clone())
                .or_insert_with(|| loader.load(fname).ok());
            let Some(f) = f else {
                unresolved += 1;
                continue;
            };
            if f.attribute(r.src_x, r.src_y) != CellAttribute::Walk {
                on_wall += 1;
                println!(
                    "  墙格 route：{} ({},{}) -> {} ({},{}) 属性={:?}",
                    r.src_map_name,
                    r.src_x,
                    r.src_y,
                    r.dst_map_name,
                    r.dst_x,
                    r.dst_y,
                    f.attribute(r.src_x, r.src_y)
                );
            }
        }
        // 真机实测（2026-10-17；独立复验复算 + 本测）：`routes=1658 / unresolved=0 / added=1649`，
        // 与**父提交**（无属性装载）的 `added=1658` 相比少 9 条——**不是**「1657 → 1648」。
        println!(
            "route 总数={} 落在墙格={on_wall} 源图不可解析/未加载={unresolved} ⇒ 生产 added 应为 {}（父提交 = {}）",
            routes.len(),
            routes.len() - unresolved - on_wall,
            routes.len() - unresolved
        );

        // 全注册表（含未被 route 引用的图）的三计数 / 格数：生产日志等价值。
        let reg = MapLoader::new(&base, "Map", "Envir")
            .load_registry()
            .expect("全注册表加载");
        let (mut cells, mut walk, mut low, mut high) = (0usize, 0usize, 0usize, 0usize);
        for f in reg.maps() {
            cells += (f.width as usize) * (f.height as usize);
            walk += f.walk_count();
            low += f.low_count();
            high += f.high_count();
        }
        println!(
            "注册表 maps={} cells={cells} walk={walk} low={low} high={high} route 源图缓存={} 用时={:?}",
            reg.maps().len(),
            cache.len(),
            t0.elapsed()
        );
        assert_eq!(walk + low + high, cells, "三态计数之和 == 总格数");
    }
}
