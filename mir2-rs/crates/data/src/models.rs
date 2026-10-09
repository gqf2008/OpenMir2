//! 表/文件的数据模型，字段与 C# 结构一一对应。

use serde::{Deserialize, Serialize};

/// `src/OpenMir2/Data/StdItem.cs`。
///
/// `ac/mac/dc/mc/sc` 在 C# 加载时就是 `MakeWord(下限, 上限)` 合成的一个 u16，
/// 这里保存合成后的值，与 C# 内存形态一致。
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StdItem {
    pub name: String,
    pub std_mode: u8,
    pub shape: u8,
    /// 物品重量
    pub weight: u8,
    pub ani_count: u8,
    pub special_pwr: i8,
    pub item_desc: u8,
    pub looks: u16,
    pub dura_max: u16,
    pub ac: u16,
    pub mac: u16,
    pub dc: u16,
    pub mc: u16,
    pub sc: u16,
    pub need: u8,
    pub need_level: u8,
    pub need_identify: u8,
    pub price: i32,
    pub stock: i32,
    pub atk_spd: u8,
    pub agility: u8,
    pub accurate: u8,
    pub mg_avoid: u8,
    pub strong: u8,
    pub undead: u8,
    pub hp_add: i32,
    pub mp_add: i32,
    pub exp_add: i32,
    pub eff_type1: u8,
    pub eff_rate1: u8,
    pub eff_value1: u8,
    pub eff_type2: u8,
    pub eff_rate2: u8,
    pub eff_value2: u8,
    pub slowdown: u8,
    pub tox: u8,
    pub tox_avoid: u8,
    pub unique_item: u8,
    pub overlap_item: u8,
    pub light: u8,
    pub item_type: u8,
    pub item_set: u16,
    pub reference: String,
}

/// `src/OpenMir2/Data/MagicInfo.cs`。
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MagicInfo {
    pub magic_id: u16,
    pub magic_name: String,
    /// 动作效果
    pub effect_type: u8,
    /// 魔法效果
    pub effect: u8,
    /// 魔法消耗
    pub spell: u16,
    /// 基本威力
    pub power: u16,
    pub max_power: u16,
    /// 职业 0-战 1-法 2-道
    pub job: u8,
    /// 技能等级（C# 为 byte[4]）
    pub train_level: [u8; 4],
    /// 技能等级最高修炼点（C# 为 int[4]）
    pub max_train: [i32; 4],
    /// 修炼等级（C# 加载时恒为 3）
    pub train_lv: u8,
    /// 技能使用延时
    pub delay_time: i32,
    pub def_spell: u8,
    pub def_power: u8,
    pub def_max_power: u8,
    pub desc: String,
}

/// `src/OpenMir2/Data/MonsterInfo.cs`（掉落列表单独由 `monitems` 模块解析）。
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MonsterInfo {
    pub name: String,
    /// 种族
    pub race: u8,
    /// 种族图像
    pub race_img: u8,
    /// 形像代码
    pub appr: u16,
    /// 怪物等级
    pub level: u8,
    /// 不死系
    pub life_attrib: u8,
    /// 视线范围
    pub cool_eye: u8,
    /// 经验点数
    pub exp: i32,
    pub hp: u16,
    pub mp: u16,
    pub ac: u16,
    pub mac: u16,
    pub dc: u16,
    pub max_dc: u16,
    pub mc: u16,
    pub sc: u16,
    pub speed: u8,
    /// 命中率
    pub hit_point: u8,
    pub walk_speed: u16,
    pub walk_step: u16,
    pub walk_wait: u16,
    pub attack_speed: u16,
}

/// `src/OpenMir2/Data/MonsterDropItem.cs`。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MonsterDropItem {
    pub max_point: i32,
    pub sel_point: i32,
    pub item_name: String,
    pub count: i32,
}

/// `src/OpenMir2/Data/DealOffInfo.cs`（寄售）。
///
/// C# 把 `UseItems` 反序列化为 `UserItem[]`；D 线只做「加载条数与过滤条件」
/// 的行为等价，完整 `UserItem` 解码归 M1 storage 线，这里保留原始 JSON。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DealOffInfo {
    pub deal_chr_name: String,
    pub buy_chr_name: String,
    pub sell_date_time: chrono::NaiveDateTime,
    pub sell_gold: i16,
    pub use_items: serde_json::Value,
    pub flag: u8,
}

/// C# `ActorRace.SabukDoor`（`src/OpenMir2/Enums/Race.cs:48`）。
pub const ACTOR_RACE_SABUK_DOOR: u8 = 110;
/// C# `ActorRace.SabukWall`（`src/OpenMir2/Enums/Race.cs:52`）。
pub const ACTOR_RACE_SABUK_WALL: u8 = 111;

/// C# `Grobal2.StringGoldName`（`src/OpenMir2/Grobal2.cs:11`）。
pub const STRING_GOLD_NAME: &str = "金币";

/// C# `Grobal2.MaxLevel`（`byte.MaxValue`）。
pub const MAX_LEVEL: i32 = 255;

/// C# `Grobal2.MaxChangeLevel`（`NeedExps` 表长）。
pub const MAX_CHANGE_LEVEL: usize = 1000;

/// C# `M2Share.StdModeMap`（`src/M2Server/M2Share.cs:188`）。
pub const STD_MODE_MAP: [u8; 8] = [15, 19, 20, 21, 22, 23, 24, 26];
