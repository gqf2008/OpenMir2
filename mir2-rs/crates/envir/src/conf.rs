//! GameSvr 配置层输入（`Server.conf`）—— 解锁入图帧 11029 `SM_SERVERCONFIG` / 1106
//! `SM_VERSION_FAIL` / 5008 `SM_GAMEGOLDNAME` / 811 `SM_ADJUST_BONUS` 的**输入**。
//!
//! 交付卡：`openmir2-m2-envir-inputs`。**只暴露 API/数据**；接线（把值放进
//! `bins/game-svr` 的 `EnterWorldInputs`）由 m2-svr 车道做。
//!
//! 逐字段来源（行号按 master `f5170a8e`）：
//! - ini 读取语义：`src/OpenMir2/Common/ConfigFile.cs:256-340`（`Load`；`StreamReader(..., GB2312)`
//!   **带 BOM 探测** ⇒ 现网 `Server.conf` 是 UTF-8+BOM，实际按 UTF-8 解码）；
//! - 金币名：`src/Modules/SystemModule/Conf/ServerConf.cs:108-109`（`[Names] GameGold`/`GamePoint`，
//!   默认值 `src/Modules/SystemModule/Conf/Model/GameSvrConf.cs:1320-1321`）；
//! - 客户端文件名：`ServerConf.cs:116-118`（`[Setup] ClientFile1..3`，默认 `GameSvrConf.cs:1298-1300`）；
//! - `BonusAbilof*`/`NakedAbilof*`：`ServerConf.cs:165-224`（`[Setup] BonusAbilof{Warr,Wizard,Taos}{DC,MC,SC,AC,MAC,HP,MP,Hit,Speed,X2}`
//!   与 `NakedAbilof*`，`*X2` → `Reserved`；默认值 `GameSvrConf.cs:1413-1418`）；
//! - `ClientConf`：`GameSvrConf.cs:1104`（声明）+ `:1866-1878`（初值）；
//!   `ServerConf.LoadConfig` **没有**读它的任何键 ⇒ 现网恒为初值；
//! - 序列化：`src/OpenMir2/SerializerUtil.cs:10-13`（MemoryPack/Utf16）+ `EDcode.cs:211-226`
//!   （`EncodeMessage<T>` 丢成员数字节后 `Encode(..., pos: 1)`）⇒ [`ClientConf::to_bytes`] 是
//!   **17 字节**（每个 `bool` 1 字节，声明序）；`NakedAbility` 是 **20 字节**（见 [`NakedAbility::to_bytes`]）。
//!
//! **`ClientFile*_CRC`**：`GameSvrConf.cs:1148-1150` 只有声明、`PlayObject.Message.cs:1690` 只有读取，
//! **全仓无任何赋值点** ⇒ oracle 实测值恒为 `0`（金标准 #11 `Recog/Param/Tag` 全 0 印证）。
//! 本模块照实暴露 `0`，不臆造 CRC 计算。

use std::path::Path;

use mir2_data::exps::IniFile;

use crate::error::EnvirError;
use crate::text::decode_text;

/// `GameSvrConf` 里本卡用到的字段（读侧；键名照 `ServerConf.cs`，**不发明**）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GameSvrConf {
    /// `[Names] GameGold`（帧 5008 的元宝名）。
    pub game_gold_name: String,
    /// `[Names] GamePoint`（帧 5008 的金币名）。
    pub game_point_name: String,
    /// `[Names] FireBallSkill`（`ServerConf.cs:80-81`；缺省 `GameSvrConf.cs:1407` 的 `火球术`）——
    /// `DelItemSkill(1)` 的 `DeleteNameSkill`（区分大小写）与 `AddItemSkill(1)` 的
    /// `FindMagic(name)`（忽略大小写）共用该配置名。
    pub fire_ball_skill: String,
    /// `[Names] HealSkill`（`ServerConf.cs:80-81`；缺省 `GameSvrConf.cs:1408` 的 `治愈术`）。
    pub heal_skill: String,
    /// **MB-3（R34）**：`[Names] Zuma1..4`（`ServerConf.cs:96-99`；`GameSvrConf.cs:159/1315`
    /// 缺省 `祖玛卫士/祖玛雕像/祖玛弓箭手/楔蛾`）——`ScultureKingMonster.CallSlave` 的
    /// `Config.Zuma[Random(4)]` 怪名表。
    pub zuma: [String; 4],
    /// `[Setup] ClientFile1`（帧 1106 的 `ClientFile1_CRC` 来源文件；见模块头）。
    pub client_file1: String,
    /// `[Setup] ClientFile2`。
    pub client_file2: String,
    /// `[Setup] ClientFile3`。
    pub client_file3: String,
    /// `ClientFile1_CRC`（oracle 无赋值点 ⇒ 恒 0）。
    pub client_file1_crc: i32,
    /// `ClientFile2_CRC`（恒 0）。
    pub client_file2_crc: i32,
    /// `ClientFile3_CRC`（恒 0）。
    pub client_file3_crc: i32,
    /// 帧 11029 的序列化输入。
    pub client_conf: ClientConf,
    /// 帧 811 的职业 A 段模板（`[Setup] BonusAbilofWarr*`）。
    pub bonus_abil_warr: NakedAbility,
    /// 帧 811 的职业 A 段模板（`[Setup] BonusAbilofWizard*`）。
    pub bonus_abil_wizard: NakedAbility,
    /// 帧 811 的职业 A 段模板（`[Setup] BonusAbilofTaos*`）。
    pub bonus_abil_taos: NakedAbility,
    /// 帧 811 的职业 C 段模板（`[Setup] NakedAbilofWarr*`）。
    pub naked_abil_warr: NakedAbility,
    /// 帧 811 的职业 C 段模板（`[Setup] NakedAbilofWizard*`）。
    pub naked_abil_wizard: NakedAbility,
    /// 帧 811 的职业 C 段模板（`[Setup] NakedAbilofTaos*`）。
    pub naked_abil_taos: NakedAbility,
    /// `[Setup] FireBoomRage`（`GameSvrConf.cs:903/1770`，缺省 1）——
    /// 23（爆裂火焰）/47（地狱烈焰）的 `MagBigExplosion` 半径。
    pub fire_boom_rage: u8,
    /// `[Setup] SnowWindRange`（`GameSvrConf.cs:904/1771`，缺省 1）—— 33（冰咆哮）的半径。
    pub snow_wind_range: u8,
    /// `[Setup] ElecBlizzardRange`（`GameSvrConf.cs:908/1772`，缺省 2）—— 24（地狱雷光）的半径。
    pub elec_blizzard_range: u8,
    /// **PL-1 / P32**：`[Setup] RevivalTime`（`GameSvrConf.cs:804/1725`，缺省 `60*1000`ms）——
    /// 复活戒指的二次复活间隔（`PlayObject.Message.cs:753`）。读取点 `ServerConf.cs:503`。
    pub revival_time: i32,
}

/// `src/OpenMir2/Packets/ClientPackets/ClientConf.cs:6-24`（17 个 `bool`，声明序即序列化序）。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ClientConf {
    pub bo_game_assist: bool,
    pub bo_whisper_record: bool,
    pub bo_maket_system: bool,
    pub bo_no_fog: bool,
    pub bo_stall_system: bool,
    pub bo_show_hp_bar: bool,
    pub bo_show_hp_number: bool,
    pub bo_no_struck: bool,
    pub bo_fast_move: bool,
    pub bo_no_weight: bool,
    pub bo_show_friend: bool,
    pub bo_show_relationship: bool,
    pub bo_show_mail: bool,
    pub bo_show_recharging: bool,
    pub bo_show_help: bool,
    pub bo_show_game_shop: bool,
    pub bo_gamepath: bool,
}

impl ClientConf {
    /// `GameSvrConf.cs:1866-1878` 的初值（`ServerConf.LoadConfig` 不读任何 `ClientConf` 键 ⇒
    /// 现网恒为这一组）。
    #[must_use]
    pub fn game_svr_default() -> Self {
        ClientConf {
            bo_game_assist: true,
            bo_whisper_record: true,
            bo_maket_system: true,
            bo_no_fog: false,
            bo_stall_system: true,
            bo_show_hp_bar: true,
            bo_show_hp_number: true,
            bo_no_struck: false,
            bo_fast_move: false,
            bo_no_weight: false,
            bo_show_friend: false,
            bo_show_relationship: false,
            bo_show_mail: false,
            bo_show_recharging: false,
            bo_show_help: false,
            bo_show_game_shop: true,
            bo_gamepath: false,
        }
    }

    /// MemoryPack 序列化原文（`EDCode.EncodeMessage<T>` 实际编码的那 17 字节，见模块头）。
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        [
            self.bo_game_assist,
            self.bo_whisper_record,
            self.bo_maket_system,
            self.bo_no_fog,
            self.bo_stall_system,
            self.bo_show_hp_bar,
            self.bo_show_hp_number,
            self.bo_no_struck,
            self.bo_fast_move,
            self.bo_no_weight,
            self.bo_show_friend,
            self.bo_show_relationship,
            self.bo_show_mail,
            self.bo_show_recharging,
            self.bo_show_help,
            self.bo_show_game_shop,
            self.bo_gamepath,
        ]
        .iter()
        .map(|b| u8::from(*b))
        .collect()
    }
}

/// `src/OpenMir2/Packets/ClientPackets/NakedAbility.cs:6-18` 的 1:1 移植。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NakedAbility {
    pub dc: u16,
    pub mc: u16,
    pub sc: u16,
    pub ac: u16,
    pub mac: u16,
    pub hp: u16,
    pub mp: u16,
    pub hit: u8,
    pub speed: i32,
    pub reserved: u8,
}

impl NakedAbility {
    /// MemoryPack 序列化原文（`usize`=字段序，`u16` LE；`Speed` 是 `int` LE）—— 20 字节。
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut v = Vec::with_capacity(20);
        v.extend_from_slice(&self.dc.to_le_bytes());
        v.extend_from_slice(&self.mc.to_le_bytes());
        v.extend_from_slice(&self.sc.to_le_bytes());
        v.extend_from_slice(&self.ac.to_le_bytes());
        v.extend_from_slice(&self.mac.to_le_bytes());
        v.extend_from_slice(&self.hp.to_le_bytes());
        v.extend_from_slice(&self.mp.to_le_bytes());
        v.push(self.hit);
        v.extend_from_slice(&self.speed.to_le_bytes());
        v.push(self.reserved);
        v
    }
}

impl GameSvrConf {
    /// 从 `Server.conf` 字节加载（用现网语料；只读）。
    ///
    /// 编码口径照 `ConfigFile.Load` 的 `StreamReader(..., GB2312)`：**带 BOM 探测**
    /// ⇒ 现网 UTF-8+BOM 的 `Server.conf` 按 UTF-8 解出 `元宝`/`成长`（不是 GBK 乱码）。
    /// 解码后用 [`IniFile::from_gbk_bytes`] 解析（`crates/data` 的既有入口）。
    #[must_use]
    pub fn from_bytes(bytes: &[u8]) -> Self {
        let text = decode_text(bytes);
        // `IniFile::from_gbk_bytes` 是 `crates/data` 的 GBK 入口；这里先把 BOM 探测后的文本
        // 重新编码为 GBK 再喂它，保证 ini 结构与值都与 `ConfigFile.Load` 一致。
        let (gbk, _, _) = encoding_rs::GBK.encode(&text);
        let ini = IniFile::from_gbk_bytes(&gbk);
        Self::from_ini(&ini)
    }

    /// 按 `ServerConf.cs:108-118/165-224` 的读取点取值（缺键 ⇒ oracle 初值，不写回）。
    #[must_use]
    pub fn from_ini(ini: &IniFile) -> Self {
        let (bw, bwz, bt) = bonus_defaults();
        let (nw, nwz, nt) = naked_defaults();
        GameSvrConf {
            game_gold_name: ini.read_string("Names", "GameGold", "元宝").to_string(),
            game_point_name: ini.read_string("Names", "GamePoint", "游戏点").to_string(),
            fire_ball_skill: ini
                .read_string("Names", "FireBallSkill", "火球术")
                .to_string(),
            heal_skill: ini.read_string("Names", "HealSkill", "治愈术").to_string(),
            zuma: [
                ini.read_string("Names", "Zuma1", "祖玛卫士").to_string(),
                ini.read_string("Names", "Zuma2", "祖玛雕像").to_string(),
                ini.read_string("Names", "Zuma3", "祖玛弓箭手").to_string(),
                ini.read_string("Names", "Zuma4", "楔蛾").to_string(),
            ],
            client_file1: ini.read_string("Setup", "ClientFile1", "mir.1").to_string(),
            client_file2: ini
                .read_string("Setup", "ClientFile2", "mir.Dat")
                .to_string(),
            client_file3: ini.read_string("Setup", "ClientFile3", "mir.3").to_string(),
            // 恒 0：oracle 无赋值点（见模块头）。
            client_file1_crc: 0,
            client_file2_crc: 0,
            client_file3_crc: 0,
            client_conf: ClientConf::game_svr_default(),
            bonus_abil_warr: read_ability(ini, "BonusAbilofWarr", &bw),
            bonus_abil_wizard: read_ability(ini, "BonusAbilofWizard", &bwz),
            bonus_abil_taos: read_ability(ini, "BonusAbilofTaos", &bt),
            naked_abil_warr: read_ability(ini, "NakedAbilofWarr", &nw),
            naked_abil_wizard: read_ability(ini, "NakedAbilofWizard", &nwz),
            naked_abil_taos: read_ability(ini, "NakedAbilofTaos", &nt),
            // `ServerConf.cs:548-550`：`ReadWriteByte("Setup", <key>, <default>)`。
            fire_boom_rage: ini.read_integer("Setup", "FireBoomRage", 1) as u8,
            snow_wind_range: ini.read_integer("Setup", "SnowWindRange", 1) as u8,
            elec_blizzard_range: ini.read_integer("Setup", "ElecBlizzardRange", 2) as u8,
            revival_time: ini.read_integer("Setup", "RevivalTime", 60 * 1000),
        }
    }

    /// 从文件路径加载。
    ///
    /// # Errors
    /// 读文件失败 ⇒ [`EnvirError::Io`]。
    pub fn load(path: &Path) -> Result<Self, EnvirError> {
        let bytes = std::fs::read(path).map_err(|source| EnvirError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        Ok(Self::from_bytes(&bytes))
    }
}

fn read_ability(ini: &IniFile, stem: &str, def: &NakedAbility) -> NakedAbility {
    NakedAbility {
        dc: read_u16(ini, "Setup", &format!("{stem}DC"), def.dc),
        mc: read_u16(ini, "Setup", &format!("{stem}MC"), def.mc),
        sc: read_u16(ini, "Setup", &format!("{stem}SC"), def.sc),
        ac: read_u16(ini, "Setup", &format!("{stem}AC"), def.ac),
        mac: read_u16(ini, "Setup", &format!("{stem}MAC"), def.mac),
        hp: read_u16(ini, "Setup", &format!("{stem}HP"), def.hp),
        mp: read_u16(ini, "Setup", &format!("{stem}MP"), def.mp),
        hit: read_u8(ini, "Setup", &format!("{stem}Hit"), def.hit),
        speed: ini.read_integer("Setup", &format!("{stem}Speed"), def.speed),
        reserved: read_u8(ini, "Setup", &format!("{stem}X2"), def.reserved),
    }
}

fn read_u16(ini: &IniFile, section: &str, key: &str, def: u16) -> u16 {
    // `ConfigFile.Read<ushort>`（`Convert.ChangeType`）；非数字时 oracle 抛异常，这里给 def。
    ini.read_integer(section, key, i32::from(def)) as u16
}

fn read_u8(ini: &IniFile, section: &str, key: &str, def: u8) -> u8 {
    ini.read_integer(section, key, i32::from(def)) as u8
}

fn bonus_defaults() -> (NakedAbility, NakedAbility, NakedAbility) {
    (
        NakedAbility {
            dc: 17,
            mc: 20,
            sc: 20,
            ac: 20,
            mac: 20,
            hp: 1,
            mp: 3,
            hit: 20,
            speed: 35,
            reserved: 0,
        },
        NakedAbility {
            dc: 17,
            mc: 25,
            sc: 30,
            ac: 20,
            mac: 15,
            hp: 2,
            mp: 1,
            hit: 25,
            speed: 35,
            reserved: 0,
        },
        NakedAbility {
            dc: 20,
            mc: 30,
            sc: 17,
            ac: 20,
            mac: 15,
            hp: 2,
            mp: 1,
            hit: 30,
            speed: 30,
            reserved: 0,
        },
    )
}

fn naked_defaults() -> (NakedAbility, NakedAbility, NakedAbility) {
    (
        NakedAbility {
            dc: 512,
            mc: 2560,
            sc: 20,
            ac: 768,
            mac: 1280,
            hp: 0,
            mp: 0,
            hit: 0,
            speed: 0,
            reserved: 0,
        },
        NakedAbility {
            dc: 512,
            mc: 512,
            sc: 2560,
            ac: 1280,
            mac: 768,
            hp: 0,
            mp: 0,
            hit: 5,
            speed: 0,
            reserved: 0,
        },
        NakedAbility {
            dc: 20,
            mc: 30,
            sc: 17,
            ac: 20,
            mac: 15,
            hp: 2,
            mp: 1,
            hit: 30,
            speed: 30,
            reserved: 0,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 把文本按 GBK 编码（现网 Envir/Server.conf 的历史编码）——夹具用。
    fn gbk(text: &str) -> Vec<u8> {
        let (b, _, _) = encoding_rs::GBK.encode(text);
        b.into_owned()
    }

    /// 缺键一律回落到 oracle 初值（`GameSvrConf.cs:1320-1321/1298-1300/1413-1418`），
    /// 不静默乱给默认。
    #[test]
    fn missing_keys_fall_back_to_game_svr_conf_defaults() {
        let c = GameSvrConf::from_bytes(b"[Server]\nGatePort=5000\n");
        assert_eq!(c.game_gold_name, "元宝");
        assert_eq!(c.game_point_name, "游戏点");
        assert_eq!(c.client_file1, "mir.1");
        assert_eq!(c.client_file2, "mir.Dat");
        assert_eq!(c.client_file3, "mir.3");
        assert_eq!(c.client_file1_crc, 0);
        assert_eq!(c.client_file2_crc, 0);
        assert_eq!(c.client_file3_crc, 0);
        assert_eq!(
            c.bonus_abil_warr.mp, 3,
            "初值 MP=3（现网 Server.conf 覆盖为 1）"
        );
        assert_eq!(c.naked_abil_wizard.hit, 5);
        assert_eq!(c.naked_abil_taos.dc, 20);
    }

    /// 现网 `Server.conf` 的读取形态（键存在 ⇒ 取文件值，`*X2` → `Reserved`）。
    #[test]
    fn reads_deployed_shaped_keys() {
        let bytes = gbk(concat!(
            "[Names]\nGameGold=元宝\nGamePoint=成长\n",
            "[Setup]\nClientFile1=mir.dat\nClientFile2=mir.2\nClientFile3=mir.3\n",
            "BonusAbilofWarrDC=17\nBonusAbilofWarrMC=20\nBonusAbilofWarrSC=20\n",
            "BonusAbilofWarrAC=20\nBonusAbilofWarrMAC=20\nBonusAbilofWarrHP=1\n",
            "BonusAbilofWarrMP=1\nBonusAbilofWarrHit=20\nBonusAbilofWarrSpeed=35\n",
            "BonusAbilofWarrX2=0\n",
            "NakedAbilofTaosDC=513\nNakedAbilofTaosMC=2560\nNakedAbilofTaosSC=512\n",
            "NakedAbilofTaosAC=512\nNakedAbilofTaosMAC=512\nNakedAbilofTaosHP=0\n",
            "NakedAbilofTaosMP=0\nNakedAbilofTaosHit=0\nNakedAbilofTaosSpeed=0\n",
            "NakedAbilofTaosX2=0\n",
        ));
        let c = GameSvrConf::from_bytes(&bytes);
        assert_eq!(c.game_gold_name, "元宝");
        assert_eq!(c.game_point_name, "成长");
        assert_eq!(c.client_file1, "mir.dat");
        assert_eq!(c.client_file2, "mir.2");
        assert_eq!(c.client_file3, "mir.3");
        assert_eq!(c.bonus_abil_warr.mp, 1, "文件值覆盖初值 MP=3");
        assert_eq!(c.bonus_abil_warr.speed, 35);
        assert_eq!(c.naked_abil_taos.dc, 513);
    }

    /// **BOM 口径**：现网 `Server.conf` 是 UTF-8+BOM，oracle `StreamReader(..., GB2312)` 会
    /// **探测 BOM 改用 UTF-8**（`ConfigFile.cs:263`）⇒ 取到的汉字是 `元宝`，不是 GBK 乱码。
    #[test]
    fn utf8_bom_server_conf_is_decoded_as_utf8() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&[0xEF, 0xBB, 0xBF]); // UTF-8 BOM
        bytes.extend_from_slice("[Names]\nGameGold=元宝\n".as_bytes());
        let c = GameSvrConf::from_bytes(&bytes);
        assert_eq!(c.game_gold_name, "元宝");
    }

    /// `ClientConf` 序列化原文 = 17 字节（声明序，每个 bool 1 字节）——
    /// 与 C# 探针 `clientconf` 输出、金标准 #6 的单解结果逐字节相等。
    #[test]
    fn client_conf_serializes_to_17_declaration_order_bytes() {
        let bytes = ClientConf::game_svr_default().to_bytes();
        assert_eq!(bytes.len(), 17);
        assert_eq!(
            bytes,
            vec![1, 1, 1, 0, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0]
        );
    }

    /// `[Names] FireBallSkill` / `[Names] HealSkill`（`ServerConf.cs:80-81`）：
    /// 缺省 = `GameSvrConf.cs:1407-1408` 的 `火球术` / `治愈术`；文件值覆盖。
    #[test]
    fn skill_name_config_defaults_and_override() {
        let c = GameSvrConf::from_bytes(
            b"[Server]
GatePort=5000
",
        );
        assert_eq!(c.fire_ball_skill, "火球术");
        assert_eq!(c.heal_skill, "治愈术");
        let c2 = GameSvrConf::from_bytes(
            "[Names]
FireBallSkill=FireBall
HealSkill=Heal
"
            .as_bytes(),
        );
        assert_eq!(c2.fire_ball_skill, "FireBall");
        assert_eq!(c2.heal_skill, "Heal");
    }

    /// `NakedAbility` 序列化原文 = 20 字节（`DC..MP` 7×u16 LE + `Hit` u8 + `Speed` i32 LE + `Reserved` u8）。
    #[test]
    fn naked_ability_serializes_to_20_bytes() {
        let a = NakedAbility {
            dc: 17,
            mc: 20,
            sc: 20,
            ac: 20,
            mac: 20,
            hp: 1,
            mp: 1,
            hit: 20,
            speed: 35,
            reserved: 0,
        };
        assert_eq!(a.to_bytes().len(), 20);
        assert_eq!(
            a.to_bytes(),
            vec![
                0x11, 0x00, 0x14, 0x00, 0x14, 0x00, 0x14, 0x00, 0x14, 0x00, 0x01, 0x00, 0x01, 0x00,
                0x14, 0x23, 0x00, 0x00, 0x00, 0x00
            ]
        );
    }
}
