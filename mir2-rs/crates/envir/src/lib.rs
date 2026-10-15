//! M2 地图层 —— Envir `*.map` 加载器 + `Point/*.txt` 点位表 + 地图名→`map_id`。
//!
//! 交付卡：`openmir2-m2-map-loading`；格式与语义规格 = 该卡 comment `862396f8` 的九条定案
//! （在**部署件**上用合成 `.map` 逐格探针得出）。本模块只读 Envir 数据文件，
//! 不做任何写回；不改表结构/脚本语法/数据文件格式。
//!
//! 与既有 crate 的关系：
//! - `crates/world` 的 `MapGrid`（属性 + 对象表）由 [`mapfile::MapFile::to_grid`] 生成；
//! - `crates/shared` 的 `HUtil32` 等价物用于 MapInfo / 点位表的分词。
//!
//! 集成点（**不在本卡**，登记给 m2-svr 车道）：
//! - `bins/game-svr` 的 `WorldMaps` 占位尺寸 ⇒ 换成 [`registry::MapRegistry`] 里按名取到的真地图；
//! - 角色记录 `CurMap` ⇒ `registry.require_id(cur_map)?`（找不到即报错，禁止静默 0 号占位图）。
#![allow(clippy::doc_markdown)]

pub mod castle;
pub mod conf;
pub mod error;
pub mod guardlist;
pub mod mapfile;
pub mod mapinfo;
pub mod mapquest;
pub mod merchant;
pub mod mongen;
pub mod npcs;
pub mod paths;
pub mod points;
pub mod registry;
pub mod text;

pub use castle::{
    attack_sabuk_wall_path, castle_config_dir, castle_list_path, parse_attack_sabuk_wall_line,
    parse_castle_list, parse_castle_map_list, parse_sabuk_w_bytes, sabuk_w_path,
    serialize_attack_sabuk_wall, serialize_castle_list, AttackSabukWallEntry, DefenseUnit,
    SabukWConfig, ATTACK_SABUK_WALL_FILE_NAME, CASTLE_TEXT_SPLIT, GUARD_ARRAY_LEN,
    MAX_CASTLE_ARCHER, MAX_CASTLE_GUARD, SABUK_W_FILE_NAME,
};
pub use conf::{ClientConf, GameSvrConf, NakedAbility};
pub use error::EnvirError;
pub use mapfile::{
    attach_points, load_map_file, parse_map_bytes, DoorStatus, MapCell, MapDoor, MapFile,
    CELL_BYTES, DOOR_BIT, DOOR_MERGE_RANGE, DOOR_POINT_MASK, HEADER_BYTES, WALL_BIT,
};
pub use mapinfo::{
    parse_flag_commands, parse_map_info_lines, parse_section_line, parse_section_line_with_state,
    MapInfoEntry, MapInfoFlag, MapInfoParseState,
};
pub use mapquest::{
    create_quest, parse_map_quest_line, HiddenFileCreation, HiddenNpcTarget, MapQuestInfo,
    MapQuestLine, MapQuestLoad, QuestNpcRecord, TEXT_SPLIT_CONST as MAP_QUEST_TEXT_SPLIT_CONST,
};
pub use merchant::{
    parse_merchant_line, MerchantRecord, MERCHANT_DEFAULT_PRICE_RATE, MERCHANT_PRICE_RATE_MIN,
    MERCHANT_SCRIPT_DIR, MERCHANT_TEXT_SPLIT,
};
pub use mongen::{
    mon_gen_count, parse_mon_gen_line, MonGenInfo, MON_GEN_THREAD_MON_LIMIT, TEXT_SPLIT_CONST,
};
pub use npcs::{load_npcs, parse_npc_line, NpcInfo, NpcType, NPC_TEXT_SPLIT};
pub use paths::{
    envir_file_path, map_file_path, path_combine, point_file_path, DEFAULT_ENVIR_DIR,
    DEFAULT_MAP_DIR, POINT_DIR,
};
pub use points::{parse_points, PointInfo};
pub use registry::{MapLoader, MapRegistry, SkippedMap};
pub use text::decode_text;

// 便于调用方/测试直接引用属性类型（`MapFile::attribute` 的返回类型）。
pub use mir2_world::CellAttribute;
