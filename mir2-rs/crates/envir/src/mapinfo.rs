//! `Envir/MapInfo.txt` 的地图名 + Flag + MapDesc 解析 —— 复刻 `src/GameSrv/Maps/Maps.cs:25-320`
//! 与 `src/GameSrv/Maps/MapManager.cs:64-96`（`AddMapInfo` 的名/文件名拆分）。
//!
//! 交付卡：`openmir2-m2-envir-inputs`（解锁入图帧 766 `SM_AREASTATE` / 54 `SM_MAPDESCRIPTION`
//! / 46 `SM_DAYCHANGING` 的 `Envir.Flag` 与 `Envir.MapDesc` 输入）。
//!
//! 逐字段来源（行号按 master `f5170a8e`）：
//! - section 行 + `sMapName`/`sMapDesc`/`nServerIndex`：`Maps.cs:52-80`；
//! - Flag 字段清单：`src/OpenMir2/Data/MapInfoFlag.cs:3-63`；
//! - Flag 命令解析循环：`Maps.cs:82-320`；
//! - `Envirnoment.Flag`/`MapDesc` 落地：`src/GameSrv/Maps/MapManager.cs:64-90`、
//!   `src/M2Server/Maps/Envirnoment.cs:43-63`、接口 `src/Modules/SystemModule/Maps/IEnvirnoment.cs:27`；
//! - 帧 766：`src/M2Server/Player/PlayObject.Base.cs:271-286`（`n8 = FightZone|1 | SafeArea|2 | InGuildWarArea|4`）；
//! - 帧 54：`src/M2Server/Player/PlayObject.Base.cs:887-894`（`nMusicid = Flag.Music ? Flag.MusicId : -1`；尾 = `MapDesc`）；
//! - 帧 46：`src/M2Server/Player/PlayObject.Message.cs:1741`（`DayBright()` 见 `PlayObject.Base.cs:244-266`，
//!   用 `Flag.DayLight`/`Flag.boDarkness`）。
//!
//! **跨 section 的三态写回**：`Maps.cs` 里 `sLine`/`sReConnectMap` 声明在 section 循环**之外**，
//! 因此 `ArrestStringEx(..., ref sLine)` 在「未命中闭定界符」时**不改写**，会保留**上一节**的值。
//! 本模块用 [`MapInfoParseState`] 承载这个状态（`parse_section_line` 便捷入口每次给新状态，
//! 注册表加载路径用带状态的入口，保证与 oracle 1:1）。

use mir2_shared::hutil32::{
    arrest_string_ex_ref, get_valid_str3, get_valid_str_cap, str_to_int, ArrestWrite,
};

/// `HUtil32.Separator = { ' ', ',', '\t' }`。
pub const SEPARATORS: [char; 3] = [' ', ',', '\t'];

/// `src/OpenMir2/Data/MapInfoFlag.cs:3-63` 的 1:1 移植（字段顺序与语义都照抄）。
///
/// 注意两个 oracle 既有形态（非本层引入，刻意保留）：
/// - `NOHORSE` 命中的是 `boNOPOSITIONMOVE`（`Maps.cs:286-290`），`NoHorse` 字段**永不被解析器写**；
/// - `DECHP`/`INCHP`/`DEC*/INC*` 的 `GetValidStr3(sLine, ref sLine, '/')` 返回值（`/` 之后段）
///   赋给 `*POINT`，`ref` 后的第一段赋给 `*TIME`（`Maps.cs:200-262`）——命名与赋值**相反**。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MapInfoFlag {
    pub safe_area: bool,
    pub request_level: i32,
    pub need_seton_flag: i32,
    pub need_on_off: i32,
    pub music: bool,
    pub music_id: i32,
    pub bo_darkness: bool,
    pub day_light: bool,
    /// 行会战争地图。
    pub fight_zone: bool,
    /// 行会战争地图。
    pub fight3_zone: bool,
    pub bo_quiz: bool,
    pub bo_noreconnect: bool,
    pub s_no_reconnect_map: String,
    pub bo_exprate: bool,
    pub exp_rate: i32,
    pub bo_pkwinlevel: bool,
    pub n_pkwinlevel: i32,
    pub bo_pkwinexp: bool,
    pub n_pkwinexp: i32,
    pub bo_pklostlevel: bool,
    pub n_pklostlevel: i32,
    pub bo_pklostexp: bool,
    pub n_pklostexp: i32,
    pub bo_dechp: bool,
    pub n_dechppoint: i32,
    pub n_dechptime: i32,
    pub bo_inchp: bool,
    pub n_inchppoint: i32,
    pub n_inchptime: i32,
    pub bo_decgamegold: bool,
    pub n_decgamegold: i32,
    pub n_decgamegoldtime: i32,
    pub bo_decgamepoint: bool,
    pub n_decgamepoint: i32,
    pub n_decgamepointtime: i32,
    pub bo_incgamegold: bool,
    pub n_incgamegold: i32,
    pub n_incgamegoldtime: i32,
    pub bo_incgamepoint: bool,
    pub n_incgamepoint: i32,
    pub n_incgamepointtime: i32,
    pub run_human: bool,
    pub run_mon: bool,
    pub bo_needhole: bool,
    pub no_re_call: bool,
    pub no_guild_re_call: bool,
    pub bo_nodearrecall: bool,
    pub master_re_call: bool,
    pub bo_norandommove: bool,
    pub bo_nodrug: bool,
    pub mine: bool,
    pub bo_mine2: bool,
    pub bo_nopositionmove: bool,
    pub no_drop_item: bool,
    pub no_throw_item: bool,
    pub no_horse: bool,
    pub bo_nochat: bool,
    pub bo_killfunc: bool,
    pub n_killfuncno: i32,
    pub bo_nohumnomon: bool,
}

impl Default for MapInfoFlag {
    /// 只填 `MapInfoFlag` 的 C# 零值（bool=false / int=0 / string=""）。
    fn default() -> Self {
        MapInfoFlag {
            safe_area: false,
            request_level: 0,
            need_seton_flag: 0,
            need_on_off: 0,
            music: false,
            music_id: 0,
            bo_darkness: false,
            day_light: false,
            fight_zone: false,
            fight3_zone: false,
            bo_quiz: false,
            bo_noreconnect: false,
            s_no_reconnect_map: String::new(),
            bo_exprate: false,
            exp_rate: 0,
            bo_pkwinlevel: false,
            n_pkwinlevel: 0,
            bo_pkwinexp: false,
            n_pkwinexp: 0,
            bo_pklostlevel: false,
            n_pklostlevel: 0,
            bo_pklostexp: false,
            n_pklostexp: 0,
            bo_dechp: false,
            n_dechppoint: 0,
            n_dechptime: 0,
            bo_inchp: false,
            n_inchppoint: 0,
            n_inchptime: 0,
            bo_decgamegold: false,
            n_decgamegold: 0,
            n_decgamegoldtime: 0,
            bo_decgamepoint: false,
            n_decgamepoint: 0,
            n_decgamepointtime: 0,
            bo_incgamegold: false,
            n_incgamegold: 0,
            n_incgamegoldtime: 0,
            bo_incgamepoint: false,
            n_incgamepoint: 0,
            n_incgamepointtime: 0,
            run_human: false,
            run_mon: false,
            bo_needhole: false,
            no_re_call: false,
            no_guild_re_call: false,
            bo_nodearrecall: false,
            master_re_call: false,
            bo_norandommove: false,
            bo_nodrug: false,
            mine: false,
            bo_mine2: false,
            bo_nopositionmove: false,
            no_drop_item: false,
            no_throw_item: false,
            no_horse: false,
            bo_nochat: false,
            bo_killfunc: false,
            n_killfuncno: 0,
            bo_nohumnomon: false,
        }
    }
}

/// `Maps.cs` 里跨 section 保留的两个局部（`sLine` / `sReConnectMap`）。
#[derive(Clone, Debug, Default)]
pub struct MapInfoParseState {
    /// `string sLine = string.Empty;`（`Maps.cs:24`）。
    line: String,
    /// `string sReConnectMap = string.Empty;`（`Maps.cs:25`）。
    reconnect_map: String,
}

/// 一条 MapInfo section 解析结果。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MapInfoEntry {
    /// 逻辑地图名（`MapManager._mapList` 的键；角色记录 `CurMap`）。
    pub map_name: String,
    /// 取图文件名（不带 `.map`）。
    pub map_file_name: String,
    /// `Envirnoment.MapDesc`（`Maps.cs:81-95` 的 `sMapDesc`；帧 54 尾巴）。
    pub desc: String,
    /// `(byte)str_to_int(s4C, 0)`。
    pub server_index: u8,
    /// `Envirnoment.Flag`。
    pub flag: MapInfoFlag,
}

/// 解析一行 MapInfo；不是 section 行返回 `None`（用一次性状态，等价于单行场景）。
///
/// `content` 首字符为 `"` 时 `GetValidStrCap` 走 `CaptureString`（C# 可能抛异常）——
/// 真实 MapInfo.txt 无此形态，这里按「跳过该行」处理并交给调用方登记。
#[must_use]
pub fn parse_section_line(line: &str) -> Option<MapInfoEntry> {
    parse_section_line_with_state(line, &mut MapInfoParseState::default())
}

/// 同 [`parse_section_line`]，但复用调用方的跨行状态（注册表加载路径用）。
#[must_use]
pub fn parse_section_line_with_state(
    line: &str,
    state: &mut MapInfoParseState,
) -> Option<MapInfoEntry> {
    // `Maps.cs:55-56`：`sFlag[0] == '['`（**未 trim**，首字符白空格即不是 section）。
    if !line.starts_with('[') {
        return None;
    }
    // `sFlag = ArrestStringEx(sFlag, "[", "]", ref sMapName)`。
    let (write, rest) = arrest_string_ex_ref(line, '[', ']');
    let raw_name = match write {
        ArrestWrite::Value(v) => v,
        ArrestWrite::Unchanged => return None,
    };
    // `string sMapDesc = GetValidStrCap(sMapName, ref sMapName, Separator)`：
    // dest(sMapName)=第一段，返回(sMapDesc)=其余。
    let (map_name, mut map_desc) = get_valid_str_cap(&raw_name, &SEPARATORS).ok()?;
    // `if (sMapDesc[0] == '"') ArrestStringEx(sMapDesc, "\"", "\"", ref sMapDesc);`
    if !map_desc.is_empty() && map_desc.starts_with('"') {
        if let (ArrestWrite::Value(v), _) = arrest_string_ex_ref(&map_desc, '"', '"') {
            map_desc = v;
        }
    }
    // `string s4C = GetValidStr3(sMapDesc, ref sMapDesc, Separator).Trim();`
    // dest(sMapDesc)=第一段（= Envir.MapDesc），返回(s4C)=其余（= 服务器号）。
    let (desc_first, s4c_raw) = get_valid_str3(&map_desc, &SEPARATORS);
    map_desc = desc_first;
    let s4c = s4c_raw.trim();
    let server_index = str_to_int(s4c, 0) as u8;
    if map_name.is_empty() {
        return None;
    }
    // `Maps.cs:82-320` 的 Flag 命令循环。
    let flag = parse_flag_commands(&rest, state);
    // `MapManager.AddMapInfo` 的名/文件名拆分（`MapManager.cs:64-86`）。
    let (map_name, map_file_name) = split_map_name_and_file(&map_name);
    Some(MapInfoEntry {
        map_name,
        map_file_name,
        desc: map_desc,
        server_index,
        flag,
    })
}

/// `Maps.cs:82-320` 的 Flag 命令循环（`sFlag` = `]` 之后的剩余串）。
///
/// `state` 承载 `sLine` / `sReConnectMap`（跨 section 保留，见模块头）。
#[must_use]
pub fn parse_flag_commands(rest: &str, state: &mut MapInfoParseState) -> MapInfoFlag {
    let mut flag = MapInfoFlag {
        // `Maps.cs:74-78` 的三条初值 + `SafeArea = false`。
        safe_area: false,
        request_level: 1,
        need_seton_flag: -1,
        need_on_off: -1,
        music_id: -1,
        ..MapInfoFlag::default()
    };
    let mut s_flag = rest.to_string();
    loop {
        // `if (string.IsNullOrEmpty(sFlag)) break;`
        if s_flag.is_empty() {
            break;
        }
        // `sFlag = GetValidStr3(sFlag, ref sCommand, Separator);`
        let (command, remaining) = get_valid_str3(&s_flag, &SEPARATORS);
        s_flag = remaining;
        if command.is_empty() {
            break;
        }
        if command.eq_ignore_ascii_case("SAFE") {
            flag.safe_area = true;
            continue;
        }
        if command.eq_ignore_ascii_case("DARK") {
            flag.bo_darkness = true;
            continue;
        }
        if command.eq_ignore_ascii_case("FIGHT") {
            flag.fight_zone = true;
            continue;
        }
        if command.eq_ignore_ascii_case("FIGHT3") {
            flag.fight3_zone = true;
            continue;
        }
        if command.eq_ignore_ascii_case("DAY") {
            flag.day_light = true;
            continue;
        }
        if command.eq_ignore_ascii_case("QUIZ") {
            flag.bo_quiz = true;
            continue;
        }
        if mir2_shared::hutil32::compare_lstr(&command, "NORECONNECT") {
            flag.bo_noreconnect = true;
            write_arrest(&mut state.reconnect_map, &command, '(', ')');
            flag.s_no_reconnect_map = state.reconnect_map.clone();
            continue;
        }
        if mir2_shared::hutil32::compare_lstr(&command, "CHECKQUEST") {
            write_arrest(&mut state.line, &command, '(', ')');
            // `QuestNPC = LoadMapQuest(sLine)`：本层不建 NPC（集成点由 M2 车道做）。
            continue;
        }
        if mir2_shared::hutil32::compare_lstr(&command, "NEEDSET_ON") {
            flag.need_on_off = 1;
            write_arrest(&mut state.line, &command, '(', ')');
            flag.need_seton_flag = str_to_int(&state.line, -1);
            continue;
        }
        if mir2_shared::hutil32::compare_lstr(&command, "NEEDSET_OFF") {
            flag.need_on_off = 0;
            write_arrest(&mut state.line, &command, '(', ')');
            flag.need_seton_flag = str_to_int(&state.line, -1);
            continue;
        }
        if mir2_shared::hutil32::compare_lstr(&command, "MUSIC") {
            flag.music = true;
            write_arrest(&mut state.line, &command, '(', ')');
            flag.music_id = str_to_int(&state.line, -1);
            continue;
        }
        if mir2_shared::hutil32::compare_lstr(&command, "EXPRATE") {
            flag.bo_exprate = true;
            write_arrest(&mut state.line, &command, '(', ')');
            flag.exp_rate = str_to_int(&state.line, -1);
            continue;
        }
        if mir2_shared::hutil32::compare_lstr(&command, "PKWINLEVEL") {
            flag.bo_pkwinlevel = true;
            write_arrest(&mut state.line, &command, '(', ')');
            flag.n_pkwinlevel = str_to_int(&state.line, -1);
            continue;
        }
        if mir2_shared::hutil32::compare_lstr(&command, "PKWINEXP") {
            flag.bo_pkwinexp = true;
            write_arrest(&mut state.line, &command, '(', ')');
            flag.n_pkwinexp = str_to_int(&state.line, -1);
            continue;
        }
        if mir2_shared::hutil32::compare_lstr(&command, "PKLOSTLEVEL") {
            flag.bo_pklostlevel = true;
            write_arrest(&mut state.line, &command, '(', ')');
            flag.n_pklostlevel = str_to_int(&state.line, -1);
            continue;
        }
        if mir2_shared::hutil32::compare_lstr(&command, "PKLOSTEXP") {
            flag.bo_pklostexp = true;
            write_arrest(&mut state.line, &command, '(', ')');
            flag.n_pklostexp = str_to_int(&state.line, -1);
            continue;
        }
        if mir2_shared::hutil32::compare_lstr(&command, "DECHP") {
            flag.bo_dechp = true;
            write_arrest(&mut state.line, &command, '(', ')');
            split_slash_point_time(
                &mut state.line,
                &mut flag.n_dechppoint,
                &mut flag.n_dechptime,
            );
            continue;
        }
        if mir2_shared::hutil32::compare_lstr(&command, "INCHP") {
            flag.bo_inchp = true;
            write_arrest(&mut state.line, &command, '(', ')');
            split_slash_point_time(
                &mut state.line,
                &mut flag.n_inchppoint,
                &mut flag.n_inchptime,
            );
            continue;
        }
        if mir2_shared::hutil32::compare_lstr(&command, "DECGAMEGOLD") {
            flag.bo_decgamegold = true;
            write_arrest(&mut state.line, &command, '(', ')');
            split_slash_point_time(
                &mut state.line,
                &mut flag.n_decgamegold,
                &mut flag.n_decgamegoldtime,
            );
            continue;
        }
        if mir2_shared::hutil32::compare_lstr(&command, "DECGAMEPOINT") {
            flag.bo_decgamepoint = true;
            write_arrest(&mut state.line, &command, '(', ')');
            split_slash_point_time(
                &mut state.line,
                &mut flag.n_decgamepoint,
                &mut flag.n_decgamepointtime,
            );
            continue;
        }
        if mir2_shared::hutil32::compare_lstr(&command, "INCGAMEGOLD") {
            flag.bo_incgamegold = true;
            write_arrest(&mut state.line, &command, '(', ')');
            split_slash_point_time(
                &mut state.line,
                &mut flag.n_incgamegold,
                &mut flag.n_incgamegoldtime,
            );
            continue;
        }
        if mir2_shared::hutil32::compare_lstr(&command, "INCGAMEPOINT") {
            flag.bo_incgamepoint = true;
            write_arrest(&mut state.line, &command, '(', ')');
            split_slash_point_time(
                &mut state.line,
                &mut flag.n_incgamepoint,
                &mut flag.n_incgamepointtime,
            );
            continue;
        }
        if command.eq_ignore_ascii_case("RUNHUMAN") {
            flag.run_human = true;
            continue;
        }
        if command.eq_ignore_ascii_case("RUNMON") {
            flag.run_mon = true;
            continue;
        }
        if command.eq_ignore_ascii_case("NEEDHOLE") {
            flag.bo_needhole = true;
            continue;
        }
        if command.eq_ignore_ascii_case("NORECALL") {
            flag.no_re_call = true;
            continue;
        }
        if command.eq_ignore_ascii_case("NOGUILDRECALL") {
            flag.no_guild_re_call = true;
            continue;
        }
        if command.eq_ignore_ascii_case("NODEARRECALL") {
            flag.bo_nodearrecall = true;
            continue;
        }
        if command.eq_ignore_ascii_case("NOMASTERRECALL") {
            flag.master_re_call = true;
            continue;
        }
        if command.eq_ignore_ascii_case("NORANDOMMOVE") {
            flag.bo_norandommove = true;
            continue;
        }
        if command.eq_ignore_ascii_case("NODRUG") {
            flag.bo_nodrug = true;
            continue;
        }
        if command.eq_ignore_ascii_case("MINE") {
            flag.mine = true;
            continue;
        }
        if command.eq_ignore_ascii_case("MINE2") {
            flag.bo_mine2 = true;
            continue;
        }
        if command.eq_ignore_ascii_case("NOTHROWITEM") {
            flag.no_throw_item = true;
            continue;
        }
        if command.eq_ignore_ascii_case("NODROPITEM") {
            flag.no_drop_item = true;
            continue;
        }
        if command.eq_ignore_ascii_case("NOPOSITIONMOVE") {
            flag.bo_nopositionmove = true;
            continue;
        }
        // oracle 既有形态：`NOHORSE` 命中的是 `boNOPOSITIONMOVE`（`Maps.cs:286-290`）。
        if command.eq_ignore_ascii_case("NOHORSE") {
            flag.bo_nopositionmove = true;
            continue;
        }
        if command.eq_ignore_ascii_case("NOCHAT") {
            flag.bo_nochat = true;
            continue;
        }
        if mir2_shared::hutil32::compare_lstr(&command, "KILLFUNC") {
            flag.bo_killfunc = true;
            write_arrest(&mut state.line, &command, '(', ')');
            flag.n_killfuncno = str_to_int(&state.line, -1);
            continue;
        }
        if mir2_shared::hutil32::compare_lstr(&command, "NOHUMNOMON") {
            // 有人才开始刷怪
            flag.bo_nohumnomon = true;
            continue;
        }
        // `if (sCommand[0] == 'L') RequestLevel = str_to_int(sCommand[1..], 1);`
        if let Some(rest) = command.strip_prefix('L') {
            flag.request_level = str_to_int(rest, 1);
        }
    }
    flag
}

/// `ArrestStringEx(source, "(", ")", ref dest)` 的三态写回（Unchanged = 保留旧值）。
fn write_arrest(dest: &mut String, source: &str, after: char, before: char) {
    if let (ArrestWrite::Value(v), _) = arrest_string_ex_ref(source, after, before) {
        *dest = v;
    }
}

/// `str_to_int(GetValidStr3(sLine, ref sLine, '/'), -1)` + `str_to_int(sLine, -1)` 的 1:1 移植。
///
/// 注意 oracle 的赋值方向与字段名**相反**：`point` = `/` 之后段，`time` = 之前段（`Maps.cs:200-262`）。
fn split_slash_point_time(line: &mut String, point: &mut i32, time: &mut i32) {
    let (first, rest) = get_valid_str3(line, &['/']);
    *line = first;
    *point = str_to_int(&rest, -1);
    *time = str_to_int(line, -1);
}

/// `MapManager.AddMapInfo` 里逻辑名/文件名的拆分（`MapManager.cs:64-86`）。
#[must_use]
pub fn split_map_name_and_file(name: &str) -> (String, String) {
    if name.contains('|') {
        let (before, after) = get_valid_str3(name, &['|']);
        return (before, after);
    }
    match arrest_string_ex_ref(name, '<', '>') {
        (ArrestWrite::Value(file_name), rest) if !file_name.is_empty() => (rest, file_name),
        _ => (name.to_string(), name.to_string()),
    }
}

/// `Maps.cs` 第一遍扫描的 `ConnectMapInfo` 指令行：返回其后第一个 token（子文件名）。
#[must_use]
pub fn connect_map_info_arg(line: &str) -> Option<String> {
    if !mir2_shared::hutil32::compare_lstr(line, "ConnectMapInfo") {
        return None;
    }
    let div = [' ', '\t'];
    // `HUtil32.GetValidStr3(loadList[count], ref sFlag, …)` 的**返回值**即子文件名。
    let (_, s_map_info_file) = get_valid_str3(line, &div);
    if s_map_info_file.is_empty() {
        None
    } else {
        Some(s_map_info_file)
    }
}

/// 逐行解析出所有 section（不做 `ConnectMapInfo` 展开；展开见 `MapLoader::load_map_info_entries`）。
#[must_use]
pub fn parse_map_info_lines(lines: &[String]) -> Vec<MapInfoEntry> {
    let mut state = MapInfoParseState::default();
    lines
        .iter()
        .filter_map(|line| parse_section_line_with_state(line, &mut state))
        .collect()
}

/// `Maps.cs:361-379` 的「加载地图连接点」行（非 section 行）分词结果：
/// `源图 源X,源Y -> 目标图 目标X,目标Y`。
///
/// 本结构只是**原始分词结果**（坐标可能落到 0、图名可能不存在）——「两侧都能 `FindMap` 才入表」
/// 的门在注册表层（[`crate::registry::MapRegistry::routes`]，对应 `MapManager.AddMapRoute`）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MapRouteEntry {
    pub src_map_name: String,
    pub src_x: i32,
    pub src_y: i32,
    pub dst_map_name: String,
    pub dst_x: i32,
    pub dst_y: i32,
}

/// `Maps.cs:361-379` 的 1:1 移植（`loadList` 里的非 `[`/`;` 行 → 地图连接点）。
///
/// 逐 token 照 oracle 的六次 `HUtil32.GetValidStr3`：
/// 1. `sMapName` ← `HUtil32.Separator = {' ', ',', '\t'}`；
/// 2. `nX`      ← `Separator`；
/// 3. `n18`     ← `Separator`；
/// 4. `s44`（目标图）← `{' ', ',', '-', '>', '\t'}`（**与前几步分隔符集不同**——正是它吃掉 `->`）；
/// 5. `n1C`     ← `Separator`；
/// 6. `n20`     ← `{' ', ',', ';', '\t'}`（**目标 y 用分号集**，又与前面不同）。
///
/// 空行 / `[` 开头（section）/ `;` 开头（注释）跳过（`Maps.cs:364` 的首字符判据）。
/// 源坐标 `29,81` 被 `Separator` 的逗号拆成 `29` / `81` 两段（不是当整串一个 token）——别"顺手修"。
///
/// **红检（变异牙）**：把第 4 步的分隔符集换成 `SEPARATORS`（去掉 `-`/`>`）⇒ 对
/// `M100A\t29,81\t->\tM100B\t287,282` 会得到 `dst_map_name == "->"` ⇒ 下面的单测必红。
#[must_use]
pub fn parse_map_routes(lines: &[String]) -> Vec<MapRouteEntry> {
    let mut out = Vec::new();
    for line in lines {
        // `if (!string.IsNullOrEmpty(sFlag) && sFlag[0] != '[' && sFlag[0] != ';')`
        if line.is_empty() || line.starts_with('[') || line.starts_with(';') {
            continue;
        }
        let (src_map_name, rest) = get_valid_str3(line, &SEPARATORS);
        let (tok, rest) = get_valid_str3(&rest, &SEPARATORS);
        let src_x = str_to_int(&tok, 0);
        let (tok, rest) = get_valid_str3(&rest, &SEPARATORS);
        let src_y = str_to_int(&tok, 0);
        // `new[] { ' ', ',', '-', '>', '\t' }` —— 目标图段。
        let (dst_map_name, rest) = get_valid_str3(&rest, &[' ', ',', '-', '>', '\t']);
        let (tok, rest) = get_valid_str3(&rest, &SEPARATORS);
        let dst_x = str_to_int(&tok, 0);
        // `new[] { ' ', ',', ';', '\t' }` —— 目标 y 段。
        let (tok, _) = get_valid_str3(&rest, &[' ', ',', ';', '\t']);
        let dst_y = str_to_int(&tok, 0);
        out.push(MapRouteEntry {
            src_map_name,
            src_x,
            src_y,
            dst_map_name,
            dst_x,
            dst_y,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// oracle 既有形态 1：`NOHORSE` 命中的是 `boNOPOSITIONMOVE`（`Maps.cs:286-290`），`no_horse` 恒 false。
    #[test]
    fn nohorse_sets_nopositionmove_not_nohorse() {
        let e = parse_section_line("[X|fx 某地] NOHORSE").unwrap();
        assert!(e.flag.bo_nopositionmove);
        assert!(!e.flag.no_horse, "oracle 从不写 NoHorse");
    }

    /// oracle 既有形态 2：`DECHP(a/b)` 的 `*POINT` 拿到 `/` 之后段、`*TIME` 拿到之前段（`Maps.cs:200-205`）。
    #[test]
    fn dechp_point_and_time_are_swapped() {
        let e = parse_section_line("[X|fx 某地] DECHP(100/3)").unwrap();
        assert!(e.flag.bo_dechp);
        assert_eq!(e.flag.n_dechppoint, 3, "point = '/' 之后段");
        assert_eq!(e.flag.n_dechptime, 100, "time = '/' 之前段");
    }

    /// 默认值：`request_level=1`、`need_seton_flag=-1`、`need_on_off=-1`、`music_id=-1`（`Maps.cs:74-78`）。
    #[test]
    fn flag_defaults_match_maps_cs() {
        let e = parse_section_line("[X|fx 某地]").unwrap();
        assert_eq!(e.flag.request_level, 1);
        assert_eq!(e.flag.need_seton_flag, -1);
        assert_eq!(e.flag.need_on_off, -1);
        assert_eq!(e.flag.music_id, -1);
        assert!(!e.flag.music);
        assert!(!e.flag.safe_area);
    }

    /// `L<n>` 前缀只认大写 `L`；`Fight2`/`Fight5`/`STALL` 等未知命令被忽略（不误判成 FIGHT）。
    #[test]
    fn l_prefix_and_unknown_commands() {
        let e = parse_section_line("[X|fx 某地] Fight2 Fight5 STALL L7").unwrap();
        assert!(!e.flag.fight_zone && !e.flag.fight3_zone);
        assert_eq!(e.flag.request_level, 7);
    }

    /// 跨 section 三态写回：`ArrestStringEx` 未命中闭括号时**不改写**，沿用上一节的值。
    #[test]
    fn arrest_unchanged_leaks_across_sections() {
        let lines = vec![
            "[A|fa 甲] NORECONNECT(G002) NEEDSET_ON(5)".to_string(),
            "[B|fb 乙] PKWINLEVEL(6".to_string(), // 无 ')' ⇒ sLine 沿用 "5"
            "[C|fc 丙] NORECONNECT(5".to_string(), // 无 ')' ⇒ sReConnectMap 沿用 "G002"
        ];
        let entries = parse_map_info_lines(&lines);
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[1].flag.n_pkwinlevel, 5, "sLine 跨节沿用");
        assert_eq!(
            entries[2].flag.s_no_reconnect_map, "G002",
            "sReConnectMap 跨节沿用"
        );
    }

    /// `[X10 "桃源" 1]`：desc 的引号先被剥掉，`1`（服务器号位）随之丢失（oracle 行为）。
    #[test]
    fn quoted_desc_and_map_name_split() {
        let e = parse_section_line("[X10 \"桃源\" 1] DAY").unwrap();
        assert_eq!(e.map_name, "X10");
        assert_eq!(e.map_file_name, "X10");
        assert_eq!(e.desc, "桃源");
        assert_eq!(e.server_index, 0);
    }

    /// **L1 TransMap 数据面**：`parse_map_routes` 只取非 `[`/`;`/空 行，且源坐标 `29,81`
    /// 被逗号拆成 `29`/`81`（`Maps.cs:361-379`）。
    ///
    /// 红检：见 [`parse_map_routes`] 的变异牙说明（第 4 步分隔符集改错 ⇒ 本测 `dst_map_name` 断言红）。
    #[test]
    fn parse_map_routes_tab_line_skips_sections_and_comments() {
        let lines = vec![
            "[X|fx 某地] DAY".to_string(),
            "; 注释".to_string(),
            String::new(),
            "M100A\t29,81\t->\tM100B\t287,282".to_string(),
        ];
        assert_eq!(
            parse_map_routes(&lines),
            vec![MapRouteEntry {
                src_map_name: "M100A".into(),
                src_x: 29,
                src_y: 81,
                dst_map_name: "M100B".into(),
                dst_x: 287,
                dst_y: 282,
            }]
        );
    }

    /// 真实文件另一种分隔（空格串而非单个制表符，`MapInfo.txt:307`）：目标图段靠
    /// `{' ',',','-','>','\t'}` 吃出。
    #[test]
    fn parse_map_routes_space_separated_line() {
        let lines = vec!["0          346,186              ->         0122       11,41".to_string()];
        assert_eq!(
            parse_map_routes(&lines),
            vec![MapRouteEntry {
                src_map_name: "0".into(),
                src_x: 346,
                src_y: 186,
                dst_map_name: "0122".into(),
                dst_x: 11,
                dst_y: 41,
            }]
        );
    }

    /// 自环 route（`Q012 -> Q012`，真实文件 37-45 行）：源/目标同名、坐标不同，照常解析保序。
    #[test]
    fn parse_map_routes_self_loop_and_order() {
        let lines = vec![
            "Q012\t14,15\t->\tQ012\t30,15".to_string(),
            "Q012\t18,15\t->\tQ012\t34,15".to_string(),
        ];
        let r = parse_map_routes(&lines);
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].dst_map_name, "Q012");
        assert_eq!(
            (r[0].src_x, r[0].src_y, r[0].dst_x, r[0].dst_y),
            (14, 15, 30, 15)
        );
        assert_eq!(
            (r[1].src_x, r[1].src_y, r[1].dst_x, r[1].dst_y),
            (18, 15, 34, 15)
        );
    }
}
