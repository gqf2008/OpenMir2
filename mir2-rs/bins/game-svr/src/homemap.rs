//! 「空 `HomeMap` 的新角色 ⇒ 出生点地图」——oracle `WorldServer.cs:250-260` + `GetHomeInfo`
//! （`WorldServer.cs:1405-1428`）的 1:1 归因与实现。
//!
//! # 为什么需要它（缺陷现场）
//!
//! `-FixedNewChr` 建出的新角色，DBSrv 写库时 `MapName`/`HomeMap` **都是 NULL**
//! （`NewChrData` 只填 `ChrName/Account/Sex/Job/Hair`，其余用默认值；
//! `PlayDataStorage.Add.cs:94-108` 把 `hd.CurMap`/`hd.HomeMap` 原样写库）⇒
//! Rust 进图门拿 `CurMap=""` 去 `MapRegistry::require_id` 必然 `Err`，落
//! `[WRN] 拒绝进图 ... CurMap:（空）` ⇒ 客户端只收到 7 帧、无 `658`、`in_world=0`。
//!
//! oracle 的处置（`WorldServer.cs:248-260`，`GetHumData` 之后）：
//!
//! ```text
//! GetHumData(playObject, ref humanRcd);         // MapName = humData.CurMap; HomeMap = humData.HomeMap
//! if (string.IsNullOrEmpty(playObject.HomeMap)) // 新角色 HomeMap 也是空
//! {
//!     playObject.HomeMap = GetHomeInfo(playObject.Job, ref homeX, ref homeY);
//!     playObject.HomeX = homeX; playObject.HomeY = homeY;
//!     playObject.MapName = playObject.HomeMap;  // ← **忽略空 CurMap**，直接落到出生点
//!     playObject.CurrX = GetRandHomeX(playObject);  // = HomeX - 2 + Random(3)
//!     playObject.CurrY = GetRandHomeY(playObject);  // = HomeY - 2 + Random(3)
//!     if (playObject.Abil.Level == 0) { /* 新手属性；本片未接，登记在卡 */ }
//! }
//! ```
//!
//! ⇒ 空 `CurMap` **不是**拒绝条件；两侧不等价的是 Rust 缺了这条「空 `HomeMap` ⇒ 出生点」回落。
//!
//! `GetHomeInfo`（`WorldServer.cs:1405-1428`）：
//!
//! ```text
//! if (M2Share.StartPointList.Count > 0)
//! {
//!     if (Count > Config.StartPointSize) I = Random(Config.StartPointSize); else I = 0;
//!     result = M2Share.GetStartPointInfo(I, ref nX, ref nY);   // StartPointList[I] 的 MapName/X/Y
//! }
//! else { result = Config.HomeMap; nX = Config.HomeX; nX = Config.HomeY; }  // ← 笔误：第二行仍写 nX
//! ```
//!
//! ⇒ 空表时 `HomeX = Config.HomeY`、`HomeY` 保持调用方初值 `0`（调用点
//! `WorldServer.cs:252-253` 写 `short homeX = 0; short homeY = 0;`）。**硬约束③：1:1 照抄该笔误，
//! 不登记偏离**——现网 `StartPoint.txt` 非空 ⇒ 不可达，用例只作保护。
//!
//! # RNG 抽数账本（M3 固定种子两侧同序列的前提）
//!
//! oracle 在这条路径上**同序消费 3 抽**（owner `784be562` / `020acba0`）：
//! `GetHomeInfo` 在 `StartPointList.Count > StartPointSize` 时抽 **1×`Random(StartPointSize)`**
//! （否则 0 抽），随后 `GetRandHomeX`/`GetRandHomeY`（`WorldServer.cs:1409-1417`）各抽 **1×`Random(3)`**。
//! 本宿主用 `crates/world::World.rng`（`mir2_shared::rng::RandomNumber`，D2 的同种子注入口）
//! **按同序同次抽**——少抽会让**后续 RNG 流偏移 3 抽**（M3②③ 的固定种子攻击序列会对不上）。
//! 真实 `E:/MirServer/M2GameSvr/Envir/StartPoint.txt` 前 `StartPointSize`(=2) 条的 `MapName` 都是 `0`
//! ⇒ 进图地图与随机值无关，仅坐标在 `(289,618)`/`(650,631)` 两条间漂移。

use std::path::Path;

use mir2_shared::rng::RandomNumber;

// ── 出生点解析：**唯一事实源 = `crates/envir/src/startpoint.rs`**
// （1:1 `LocalDB.LoadStartPoint`，`LocalDB.cs:822-867`）。
//
// 卡 `openmir2-loader-fields` 残余接线：此前本模块私有一份只取 `MapName/X/Y` 三列的解析，
// 丢弃 oracle 逐行读取的 `NotAllowSay/Range/Type/PkZone/PkFire` 五列（`LocalDB.cs:844-862`）。
// 现在直接复用 envir 的解析器（`parse_start_points`/`load_start_points_if_exists`）；
// 本接缝只用 `map_name/x/y`，其余五列随 `StartPoint` 类型一起承载，供后续
// `MakeSafePkZone`/`InSafeZone` 接线。
pub use mir2_envir::startpoint::StartPoint;

/// 「空 `HomeMap` ⇒ 出生点」的静态输入（进程启动时读一次）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HomeFallback {
    /// `Envir/StartPoint.txt`（`M2Share.StartPointList`）。
    pub start_points: Vec<StartPoint>,
    /// `[Setup] StartPointSize`（`Config.StartPointSize`，oracle 缺省 2）。
    pub start_point_size: u8,
    /// `[Setup] HomeMap`（`Config.HomeMap`，oracle 缺省 `"0"`）。
    pub home_map: String,
    /// `[Setup] HomeX`（`Config.HomeX`，oracle 缺省 289）。
    pub home_x: i16,
    /// `[Setup] HomeY`（`Config.HomeY`，oracle 缺省 618）。
    pub home_y: i16,
}

impl HomeFallback {
    /// `WorldServer.GetHomeInfo(job, ref x, ref y)`（`WorldServer.cs:1405-1428`）——**同序消费 RNG**。
    ///
    /// `StartPointList` 非空且 `Count > StartPointSize` ⇒ 抽 1×`Random(StartPointSize)` 定索引；
    /// 非空但 `Count <= StartPointSize` ⇒ `I = 0`（**0 抽**）；空 ⇒ `Config.HomeMap` + 照抄笔误后的 HomeX/HomeY（详见下，0 抽）。
    ///
    /// 注意 oracle 形参 `Job` **完全未被使用**（不是按职业取）。空表分支照抄 oracle 笔误
    /// （`WorldServer.cs:1425-1426`：`nX = Config.HomeX; nX = Config.HomeY;`）⇒ `HomeX = Config.HomeY`、
    /// `HomeY` 保持调用方初值 `0`（`:252-253` 的 `short homeY = 0`）。
    #[must_use]
    pub fn get_home_info(&self, rng: &mut RandomNumber) -> (String, i16, i16) {
        if self.start_points.is_empty() {
            // 硬约束③（行为等价优先）：oracle `WorldServer.cs:1425-1426` 是
            // `nX = Config.HomeX; nX = Config.HomeY;`（第二行笔误仍写 `nX`）⇒ 最终
            // `nX = Config.HomeY`，`nY` 保持调用方初值 0（`:252-253` 的 `short homeY = 0`）。
            // 照抄该笔误，**不登记偏离**（现网 `StartPoint.txt` 非空，不可达；用例只作保护）。
            return (self.home_map.clone(), self.home_y, 0);
        }
        let count = self.start_points.len();
        let idx = if count > usize::from(self.start_point_size) {
            // C# `Random(Config.StartPointSize)`：`I ∈ [0, size)`。`size==0` 时 C# `Next(0)` 回 0（仍抽 1 次）。
            let n = i32::from(self.start_point_size).max(1);
            usize::try_from(rng.random_below(n))
                .unwrap_or(0)
                .min(count - 1)
        } else {
            0
        };
        let sp = &self.start_points[idx];
        (sp.map_name.clone(), sp.x, sp.y)
    }

    /// oracle `WorldServer.cs:254-259` 的**整条新角色出生点路径**（**同序 3 抽**）：
    /// `GetHomeInfo`（`Count > StartPointSize` 时 1 抽）+ `GetRandHomeX`/`GetRandHomeY`（各 1×`Random(3)`）。
    /// 坐标为 `(short)(Random(3) + Home(X|Y) - 2)`（`WorldServer.cs:1409-1417`）。
    /// 仅返回 `(map, CurrX, CurrY)` 的便捷包装（原生产路径）；存档要 `HomeX/HomeY`，生产改调
    /// [`Self::new_human_spawn_with_home`]。本函数保留给单测（抽数账本）。
    #[must_use]
    #[cfg(test)]
    pub fn new_human_spawn(&self, rng: &mut RandomNumber) -> (String, i16, i16) {
        let (map, _hx, _hy, x, y) = self.new_human_spawn_with_home(rng);
        (map, x, y)
    }

    /// 与 [`Self::new_human_spawn`] 同一条路径，但**同时返回出生点原坐标** `(map, HomeX, HomeY, CurrX, CurrY)`：
    /// `MakeSaveRcd`（`WorldServer.cs:1256-1258`）要写 `HomeMap/HomeX/HomeY`，而不只是 `CurrX/CurrY`。
    /// 抽数次序/次数与上面完全一致（只能调 `get_home_info` 一次）。
    #[must_use]
    pub fn new_human_spawn_with_home(
        &self,
        rng: &mut RandomNumber,
    ) -> (String, i16, i16, i16, i16) {
        let (map, hx, hy) = self.get_home_info(rng);
        // 顺序只能先是 X 再是 Y（`GetRandHomeX` 在 `:258` / `GetRandHomeY` 在 `:259`）。
        let x = (i32::from(hx) - 2 + rng.random_below(3)) as i16;
        let y = (i32::from(hy) - 2 + rng.random_below(3)) as i16;
        (map, hx, hy, x, y)
    }
}

/// 读 `<base>/<envir_dir>/StartPoint.txt`，**消费 `crates/envir::startpoint` 的新加载器**。
///
/// oracle `LocalDB.LoadStartPoint`（`LocalDB.cs:832-867`）：
/// - `File.Exists` 假 ⇒ 整段跳过（返回空表，**不**报错），`GetHomeInfo` 落到 `Config.HomeMap`；
/// - 存在但读取失败 ⇒ `LoadFromFile` 抛出、调用点无 try/catch ⇒ **启动期 fail-closed**。
///   旧实现 `Err(_) => Vec::new()` 会静默吞掉读取失败（与 oracle 不符）；接线后改为 `Err` 上抛。
///
/// 现网该文件是 **GBK**（首行注释 `;安全区/出生点`），解码口径由 envir 加载器统一
/// （`StringList.LoadFromFile`：BOM/UTF-16/GBK；不用 `read_to_string`——2026-10-12 真机跑出的坑）。
///
/// # Errors
/// 文件存在但读取失败 ⇒ [`mir2_envir::EnvirError::Io`]。
pub fn load_start_points(
    base: &Path,
    envir_dir: &str,
) -> Result<Vec<StartPoint>, mir2_envir::EnvirError> {
    let path = mir2_envir::startpoint::start_point_path(base, envir_dir);
    Ok(mir2_envir::startpoint::load_start_points_if_exists(&path)?.unwrap_or_default())
}

/// `PlayObject.MoveToHome` → `BaseObjectMove`（`PlayObject.Base.cs:2306-2333`）的**目标解析**。
///
/// 逐行：
/// - `2309 BaseObjectMove(HomeMap, HomeX, HomeY)`；
/// - `2314 if (string.IsNullOrEmpty(sMap)) sMap = MapName;`（空 `HomeMap` ⇒ **当前**图名）；
/// - `2318 if (sX != 0 && sY != 0) SpaceMove(sMap, sX, sY, 0); else MapRandomMove(sMap, 0);`
///   （**两轴都非 0** 才走定坐标；任一轴为 0 走随机落点）。
///
/// **不含** `FindMap`（注册表在宿主）：`fixed == false` ⇒ `MapRandomMove`，即便某轴非 0。
///
/// 恒假死分支 `IEnvirnoment envir = Envir; if (envir != Envir …) IsTimeRecall = false;`
/// （`:2328-2332`：赋值在移动**之后** ⇒ 恒 false）不必实现（登记，见设计件 §0.5）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MoveHomeTarget {
    /// 解析后的目标图名（空 `HomeMap` 时回落到当前图名）。
    pub map_name: String,
    pub x: i16,
    pub y: i16,
    /// `sX != 0 && sY != 0`（**两轴都非 0**；写成 `||` 是错的）。
    pub fixed: bool,
}

/// `BaseObjectMove`（`PlayObject.Base.cs:2312-2327`）的目标解析（纯逻辑，`FindMap` 由宿主做）。
#[must_use]
pub fn resolve_move_home(
    home_map: &str,
    current_map_name: &str,
    home_x: i16,
    home_y: i16,
) -> MoveHomeTarget {
    // `string.IsNullOrEmpty(sMap)`：C# 的 null 与 "" 在这里都落 `&str` 的空串。
    let map_name = if home_map.is_empty() {
        current_map_name
    } else {
        home_map
    };
    MoveHomeTarget {
        map_name: map_name.to_string(),
        x: home_x,
        y: home_y,
        fixed: home_x != 0 && home_y != 0,
    }
}

/// `GetStartPoint()`（oracle `PlayObject.Base.cs:704-721`）的纯函数等价物。
///
/// `PvpLevel() >= 2`（`PkPoint / 100`，`PlayObject.cs:4352-4355`）⇒ 红名回城点；
/// 否则在 `StartPointList` 里按 **`Envir.MapName`** 匹配（`OrdinalIgnoreCase`）——
/// 命中即**覆盖**调用方 Home*，无匹配 ⇒ `None`（调用方保留原值）。
pub fn get_start_point(
    pvp_level: u8,
    current_map: &str,
    start_points: &[StartPoint],
    red_home_map: &str,
    red_home_x: i16,
    red_home_y: i16,
) -> Option<(String, i16, i16)> {
    if pvp_level >= 2 {
        return Some((red_home_map.to_string(), red_home_x, red_home_y));
    }
    start_points
        .iter()
        .find(|p| p.map_name.eq_ignore_ascii_case(current_map))
        .map(|p| (p.map_name.clone(), p.x, p.y))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mir2_envir::startpoint::parse_start_points;

    /// 现网 `E:/MirServer/M2GameSvr/Envir/StartPoint.txt` 的前几行（逐字）。
    const REAL_HEAD: &str = ";安全区/出生点\n0    289  618\n0    650  631\n0    330  266\n2    503  483\n3    330  330\n4    240  200\n5    141  337\n11   178  326\n";

    #[test]
    fn parse_real_head_matches_oracle_tokens() {
        let sp = parse_start_points(REAL_HEAD);
        assert_eq!(sp.len(), 8, "首行注释跳过：{sp:?}");
        // 旧三列解析的逐位值（本用例同时是「换加载器后逐位不变」的合同钉）。
        for (i, (map, x, y)) in [("0", 289, 618), ("0", 650, 631), ("11", 178, 326)]
            .into_iter()
            .enumerate()
        {
            let idx = if i < 2 { i } else { 7 };
            assert_eq!(
                (sp[idx].map_name.as_str(), sp[idx].x, sp[idx].y),
                (map, x, y)
            );
        }
        // 三列部署件：后五列必须是默认值（`LocalDB.cs:862-863`）。
        for p in &sp {
            assert!(!p.not_allow_say);
            assert_eq!((p.range, p.r#type, p.pk_zone, p.pk_fire), (0, 0, 0, 0));
            assert_eq!(p.shape, 0);
        }
    }

    /// 红检：把分隔符从 `{ ' ', '\t' }` 改成只认 `','` ⇒ 现网（空格分隔）解析出 0 条 ⇒ 必红。
    /// 这里用「显式喂逗号文本」做**正向对照**：同一行若只有逗号也应能分（oracle 的 `GetValidStr3`
    /// 只认给定分隔符，行内空格属于 token），从而把「解析器确实按 `' '/\t'` 分」钉住。
    #[test]
    fn parser_splits_on_space_and_tab_only() {
        let text = "0\t289 618\n";
        let sp = parse_start_points(text);
        assert_eq!(sp.len(), 1);
        assert_eq!((sp[0].map_name.as_str(), sp[0].x, sp[0].y), ("0", 289, 618));
    }

    #[test]
    fn skips_blank_comment_and_incomplete_lines() {
        let text = "\n; c\n0 289\n   \n1 2 3\n";
        let sp = parse_start_points(text);
        // `0 289` 缺 CY ⇒ 丢；`1 2 3` 合法。
        assert_eq!(sp.len(), 1);
        assert_eq!((sp[0].map_name.as_str(), sp[0].x, sp[0].y), ("1", 2, 3));
    }

    #[test]
    fn get_home_info_consumes_one_draw_when_count_exceeds_size() {
        let fb = HomeFallback {
            start_points: parse_start_points(REAL_HEAD),
            start_point_size: 2,
            home_map: "3".into(),
            home_x: 839,
            home_y: 668,
        };
        let mut rng = RandomNumber::with_seed(42);
        let (map, x, y) = fb.get_home_info(&mut rng);
        // 同种子独立重放：第 1 抽（`Random(2)`）就是索引。
        let idx = usize::try_from(RandomNumber::with_seed(42).random_below(2)).unwrap();
        assert_eq!(map, "0", "前两条 MapName 都是 0");
        assert_eq!((x, y), (fb.start_points[idx].x, fb.start_points[idx].y));
        // 后续抽与「只抽 1 次」的同种子重放一致 ⇒ 确实只消费 1 抽。
        let next = rng.random_below(1000);
        let mut replay = RandomNumber::with_seed(42);
        replay.random_below(2);
        assert_eq!(next, replay.random_below(1000));
    }

    #[test]
    fn get_home_info_falls_back_to_config_when_no_start_points() {
        let fb = HomeFallback {
            start_points: Vec::new(),
            start_point_size: 2,
            home_map: "3".into(),
            home_x: 839,
            home_y: 668,
        };
        let mut rng = RandomNumber::with_seed(42);
        // 空表 ⇒ `Config.HomeMap` + oracle 笔误坐标（`HomeX = Config.HomeY`、`HomeY = 0`）。
        assert_eq!(fb.get_home_info(&mut rng), ("3".to_string(), 668, 0));
        // 空表 ⇒ **0 抽**：下一抽与同种子首抽一致。
        assert_eq!(
            rng.random_below(1000),
            RandomNumber::with_seed(42).random_below(1000)
        );
    }

    /// **硬约束③**：空 `StartPointList` 时照抄 oracle 的笔误（`WorldServer.cs:1425-1426`）——
    /// `nX` 先取 `Config.HomeX` 随即被 `Config.HomeY` 覆盖，`nY` 保持调用方初值 `0`
    /// ⇒ 返回 `(HomeMap, Config.HomeY, 0)`，`Config.HomeX` **被丢弃**。
    ///
    /// 现网 `StartPoint.txt` 非空 ⇒ 不可达；本用例是保护（防「顺手修正笔误」）。
    /// 牙：把空支改回「X/Y 各自取」⇒ 坐标断言红（必红）。
    #[test]
    fn get_home_info_copies_oracle_typo_when_no_start_points() {
        let fb = HomeFallback {
            start_points: Vec::new(),
            start_point_size: 2,
            home_map: "7".into(),
            home_x: 111,
            home_y: 222,
        };
        let mut rng = RandomNumber::with_seed(7);
        assert_eq!(
            fb.get_home_info(&mut rng),
            ("7".to_string(), 222, 0),
            "oracle 笔误：HomeX = Config.HomeY(222) 而不是 Config.HomeX(111)；HomeY 保持初值 0"
        );
        // 空表 ⇒ 0 抽（坐标来自配置，不抽数）。
        assert_eq!(
            rng.random_below(1000),
            RandomNumber::with_seed(7).random_below(1000)
        );
    }

    /// **owner `784be562`/`020acba0` 的抽数账本**：`new_human_spawn` 必须按 oracle 同序抽 **3 次**
    /// （`Random(StartPointSize)` → `Random(3)`(X) → `Random(3)`(Y)）。
    /// 红检：去掉任一抽（回到 `I=0`/坐标下界）⇒ 第 4 抽与手放的同种子重放不一致 ⇒ 必红。
    #[test]
    fn new_human_spawn_consumes_three_draws_in_oracle_order() {
        let fb = HomeFallback {
            start_points: parse_start_points(REAL_HEAD),
            start_point_size: 2,
            home_map: "3".into(),
            home_x: 839,
            home_y: 668,
        };
        let mut a = RandomNumber::with_seed(42);
        let (map, x, y) = fb.new_human_spawn(&mut a);
        let after = a.random_below(1000);

        let mut b = RandomNumber::with_seed(42);
        let i = usize::try_from(b.random_below(2)).unwrap();
        let rx = b.random_below(3);
        let ry = b.random_below(3);
        assert_eq!(
            after,
            b.random_below(1000),
            "必须恰好抽 3 次（少抽 ⇒ 后续 RNG 流偏移）"
        );
        let sp = &fb.start_points[i];
        assert_eq!(map, sp.map_name);
        assert_eq!(
            (x, y),
            (
                (i32::from(sp.x) - 2 + rx) as i16,
                (i32::from(sp.y) - 2 + ry) as i16
            ),
            "坐标为 Random(3) + Home(X|Y) - 2，且 X 先于 Y"
        );
    }

    /// 现网 `StartPoint.txt` 是 **GBK**（首行 `;安全区/出生点`）——`load_start_points` 必须解码成
    /// 8 条，否则空表 ⇒ 落到 `Config.HomeMap=3` 而不是夹具实测的 `StartPoint[0]=("0",289,618)`。
    /// 红检：把 `std::fs::read` + `decode_text` 改回 `read_to_string` ⇒ 本用例必红（真实 GBK 文件读失败）。
    #[test]
    fn load_start_points_decodes_gbk_real_file() {
        let dir = std::env::temp_dir().join(format!("gs-homemap-gbk-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("Envir")).unwrap();
        // 首行 GBK `;安全区/出生点`（`b0 b2 c8 ab c7 f8`）+ 两条出生点。
        let mut bytes: Vec<u8> = vec![0x3B, 0xB0, 0xB2, 0xC8, 0xAB, 0xC7, 0xF8, 0x0D, 0x0A];
        bytes.extend_from_slice(b"0    289  618\r\n0    650  631\r\n");
        std::fs::write(dir.join("Envir").join("StartPoint.txt"), &bytes).unwrap();
        let sp = load_start_points(&dir, "Envir").expect("读取成功");
        assert_eq!(
            sp.len(),
            2,
            "GBK 文件必须解出 2 条（read_to_string 会得 0 条）：{sp:?}"
        );
        assert_eq!((sp[0].map_name.as_str(), sp[0].x, sp[0].y), ("0", 289, 618));
        std::fs::remove_dir_all(&dir).ok();
    }

    /// 判据 7（R）：`BaseObjectMove` 的分支选择——空 `HomeMap` 回落当前图名；**两轴都非 0**
    /// 才走 `SpaceMove`（定坐标），任一轴为 0 ⇒ `MapRandomMove`。
    ///
    /// 牙：`&&` 写成 `||` ⇒ `home_x=0, home_y=5` 那条红；不回落当前图名 ⇒ 空 `HomeMap` 那条红。
    #[test]
    fn resolve_move_home_branches() {
        assert_eq!(
            resolve_move_home("3", "0", 839, 668),
            MoveHomeTarget {
                map_name: "3".into(),
                x: 839,
                y: 668,
                fixed: true
            }
        );
        // 空 HomeMap ⇒ 当前图名。
        assert_eq!(
            resolve_move_home("", "0", 839, 668),
            MoveHomeTarget {
                map_name: "0".into(),
                x: 839,
                y: 668,
                fixed: true
            }
        );
        // 任一轴为 0 ⇒ 随机落点（`fixed == false`）。
        for (x, y) in [(0, 668), (839, 0), (0, 0)] {
            let t = resolve_move_home("3", "0", x, y);
            assert!(!t.fixed, "({x},{y}) 必须走 MapRandomMove");
            assert_eq!(t.map_name, "3");
        }
        // 负坐标也是「非 0」（照抄 `sX != 0`，不 clamp）。
        assert!(resolve_move_home("3", "0", -1, -1).fixed);
    }
    // --------------------------------------------------------------------
    // 卡 openmir2-loader-fields 残余接线：合同测试 + 牙（≥2）
    // --------------------------------------------------------------------

    /// **合同（逐位）**：换加载器后，现网三列形态的 `map_name/x/y` 与旧三列解析**逐位相同**；
    /// 差异只有「新类型多承载 5 列（三列部署件全为默认）」与「读取失败改为 fail-closed」。
    ///
    /// 红检（改坏必红）：把 `parse_start_points` 换回旧的三列私有实现（或改 `x` 的默认值）
    /// ⇒ 本用例必红。
    #[test]
    fn wiring_matches_legacy_three_column_view() {
        let legacy: [(&str, i16, i16); 8] = [
            ("0", 289, 618),
            ("0", 650, 631),
            ("0", 330, 266),
            ("2", 503, 483),
            ("3", 330, 330),
            ("4", 240, 200),
            ("5", 141, 337),
            ("11", 178, 326),
        ];
        // 旧解析器（本文件接线前）对这些行的输出：map/x/y 完全相同。
        let sp = parse_start_points(REAL_HEAD);
        assert_eq!(sp.len(), legacy.len());
        for (i, want) in legacy.iter().enumerate() {
            assert_eq!(
                (sp[i].map_name.as_str(), sp[i].x, sp[i].y),
                *want,
                "第 {i} 条与旧三列解析逐位不同"
            );
            // 三列部署件：新列必须显式承载默认值。
            assert_eq!(
                (sp[i].not_allow_say, sp[i].range, sp[i].r#type),
                (false, 0, 0)
            );
            assert_eq!((sp[i].pk_zone, sp[i].pk_fire, sp[i].shape), (0, 0, 0));
        }
    }

    /// **合同（旧三列语义缺什么）**：8 列行必须把 oracle 读到的 5 列承载下来——旧三列解析
    /// 会丢弃它们。若有人把接线回退成三列解析 ⇒ 本用例必红。
    #[test]
    fn wiring_carries_columns_beyond_legacy_three() {
        let sp = parse_start_points("3 644 290 1 7 2 13 5\n");
        assert_eq!(sp.len(), 1);
        assert_eq!((sp[0].map_name.as_str(), sp[0].x, sp[0].y), ("3", 644, 290));
        assert!(sp[0].not_allow_say, "allSay=1 ⇒ Convert.ToBoolean(1)");
        assert_eq!(
            (sp[0].range, sp[0].r#type, sp[0].pk_zone, sp[0].pk_fire),
            (7, 2, 13, 5)
        );
    }

    /// **牙（缺列/非法值）**：缺 `MapName/x/y` 任一并列 ⇒ 丢弃；非法数值 ⇒ `StrToInt16` 取默认 0
    /// 但行仍入库；`NotAllowSay` 是「非 0 即 true」（负数也为 true）。
    ///
    /// 红检（改坏必红）：放宽入库门（空列也收）⇒ `len()==2` 断言必红；
    /// 把 `x` 的默认值从 0 改成 1 ⇒ `(0, 0)` 断言必红；把 `!= 0` 改成 `> 0` ⇒ 负数断言必红。
    #[test]
    fn wiring_gate_and_illegal_values() {
        let sp = parse_start_points("1 2\n3\n289 618\n4 abc def\n5 -1 -1 -1\n");
        assert_eq!(sp.len(), 2, "缺列三行丢弃；非法数值行仍入库");
        assert_eq!((sp[0].map_name.as_str(), sp[0].x, sp[0].y), ("4", 0, 0));
        assert_eq!((sp[1].map_name.as_str(), sp[1].x, sp[1].y), ("5", -1, -1));
        assert!(
            sp[1].not_allow_say,
            "allSay=-1 非 0 ⇒ true（Convert.ToBoolean）"
        );
    }
    /// 卡 `openmir2-home-coords-divergence`：`GetStartPoint()` 三向牙。
    /// 变异：把 `pvp_level >= 2` 常量删掉 / 把匹配改成忽略 map（永远命中）⇒ 前两条红。
    #[test]
    fn get_start_point_matches_current_map_or_red_home() {
        let sp = parse_start_points("0 289 618
3 330 330
");
        // ① 命中当前地图 ⇒ 覆盖成该条目（map 3 ⇒ 330/330）
        assert_eq!(
            get_start_point(0, "3", &sp, "3", 845, 674),
            Some(("3".to_string(), 330, 330)),
            "当前地图在 StartPoint 表内 ⇒ 用该条目覆盖"
        );
        // ② 无匹配 ⇒ 保留原值（None）
        assert_eq!(
            get_start_point(0, "B013", &sp, "3", 845, 674),
            None,
            "当前地图不在表内 ⇒ 不写（保留 DB 原值）"
        );
        // ③ PvP>=2 ⇒ 红名回城点（与当前地图无关）
        assert_eq!(
            get_start_point(2, "B013", &sp, "3", 845, 674),
            Some(("3".to_string(), 845, 674)),
            "PvpLevel>=2 ⇒ RedHome*"
        );
        // 大小写不敏感（oracle `StringComparison.OrdinalIgnoreCase`）
        assert_eq!(
            get_start_point(0, "b013", &parse_start_points("b013 7 9
"), "3", 845, 674),
            Some(("b013".to_string(), 7, 9))
        );
    }
}
