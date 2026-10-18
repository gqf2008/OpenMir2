//! `CM_QUERYBAGITEMS(81)` → `SM_BAGITEMS(201)` 的**编码层**（owner 追加的卡项 ②，issue `8d07694d`）。
//!
//! # oracle 链（逐行对齐）
//!
//! ```text
//! PlayObject.Message.cs:784  case CM_QUERYBAGITEMS:
//!   if ((GetTickCount() - QueryBagItemsTick) > 30*1000) { QueryBagItemsTick = GetTickCount(); ClientQueryBagItems(); }
//!   else SysMsg(MessageSettings.QUERYBAGITEMS, Red, Hint);
//!
//! PlayObject.Operate.cs:37  public void ClientQueryBagItems()
//!   for i in 0..ItemList.Count:
//!     userItem = ItemList[i]; item = GetStdItem(userItem.Index);
//!     if (item != null) {
//!       ClientItem clientItem = new();
//!       GetUpgradeStdItem(item, userItem, ref clientItem);       // desc 升级差量
//!       clientItem.Item.Name = CustomItemSystem.GetItemName(userItem);
//!       clientItem.Dura = userItem.Dura; clientItem.DuraMax = userItem.DuraMax;
//!       clientItem.MakeIndex = userItem.MakeIndex;
//!       if (item.StdMode == 50) clientItem.Item.Name += " #" + userItem.Dura;
//!       sSendMsg += EDCode.EncodeBuffer(clientItem) + '/';
//!     }
//!   if (!string.IsNullOrEmpty(sSendMsg))
//!     { ClientMsg = MakeMessage(SM_BAGITEMS, ActorId, 0, 0, ItemList.Count); SendSocket(ClientMsg, sSendMsg); }
//! ```
//!
//! `ClientItem.WritePacket`（`src/OpenMir2/Packets/ClientPackets/ClientItem.cs:22-84`）与
//! `EDCode.EncodeBuffer(ClientItem)`（`src/OpenMir2/EDCode.cs:187`）= `EncryptUtil.Encode(GetBuffer())`。
//! `WriteAsciiString(v, 14)`（`BinaryWriterExtension.cs`）= 1 字节长度 + 定长 14B（GB2312）。
//!
//! # 本片口径与登记
//!
//! - **`GetUpgradeStdItem` 的 desc 升级差量未实现**（`GameItemSystem.cs:731-...` 的大 switch）：
//!   新角色默认 4 件物品 `Desc` 全 0 ⇒ 各分支退化为「原值」（`MakeWord(LoByte(x), min(255, HiByte(x)+0)) = x`）
//!   ⇒ **本接缝对 desc=0 与 oracle 逐字节一致**；带升级物品走 `TODO: T-*` 登记。
//! - `CustomItemSystem.GetItemName`（`CustomItemSystem.cs:122`）：`Desc[13]==1` 才取自定义名，
//!   默认物品 `Desc[13]==0` ⇒ 模板名。本接缝只走模板名（自定义名未建模，登记）。
//! - `StdMode == 50` 的名字后缀 `" #" + Dura` 照抄（`Operate.cs:52-55`）。

use encoding_rs::GBK;
use mir2_data::StdItem;
use mir2_protocol::edcode;

use crate::smframe;

/// `ClientPacketConst.ItemNameLen`（`src/OpenMir2/Packets/ClientPackets/ClientPacketConst.cs:8`）。
pub const ITEM_NAME_LEN: usize = 14;

/// 一个背包物品的**客户端视图输入**：模板 + `UserItem` 三字段 + `Desc`。
pub struct ClientItem<'a> {
    /// `SystemShare.ItemSystem.GetStdItem(userItem.Index)`（模板缺失 ⇒ 调用方不产出该项）。
    pub tpl: &'a StdItem,
    /// `CustomItemSystem.GetItemName(userItem)`（本接缝 = 模板名；`StdMode==50` 会加 `" #" + Dura`）。
    pub name: String,
    pub make_index: i32,
    pub dura: u16,
    pub dura_max: u16,
    pub desc: [u8; ITEM_NAME_LEN],
}

/// `BinaryWriterExtension.WriteAsciiString(value, defaultSize)` 的字节产出（1B 长度 + 定长，GB2312）。
///
/// 口径：空串 ⇒ 长度字节 0；`buffer.len() > defaultSize` ⇒ 长度字节 `defaultSize`、内容截断；
/// 否则长度字节 = `buffer.len()`、内容补零到 `defaultSize`。
pub(crate) fn write_ascii_string(out: &mut Vec<u8>, value: &str, default_size: usize) {
    let (bytes, _, _) = GBK.encode(value);
    let mut buf = bytes.into_owned();
    if buf.len() > default_size {
        buf.truncate(default_size);
        out.push(default_size as u8);
    } else if value.is_empty() {
        buf.resize(default_size, 0);
        out.push(0);
    } else {
        let len = buf.len();
        buf.resize(default_size, 0);
        out.push(len as u8);
    }
    out.extend_from_slice(&buf);
}

/// `ClientItem.WritePacket` 的 1:1 编码（字段序照 `ClientItem.cs:22-84`）。
#[must_use]
pub fn encode_client_item(ci: &ClientItem<'_>) -> Vec<u8> {
    let t = ci.tpl;
    let mut out = Vec::with_capacity(128);
    write_ascii_string(&mut out, &ci.name, ITEM_NAME_LEN);
    out.push(t.std_mode);
    out.push(t.shape);
    out.push(t.weight);
    out.push(t.ani_count);
    out.push(t.special_pwr as u8);
    out.push(t.item_desc);
    out.push(0);
    out.extend_from_slice(&t.looks.to_le_bytes());
    out.extend_from_slice(&t.dura_max.to_le_bytes());
    out.extend_from_slice(&t.ac.to_le_bytes());
    out.extend_from_slice(&t.mac.to_le_bytes());
    out.extend_from_slice(&t.dc.to_le_bytes());
    out.extend_from_slice(&t.mc.to_le_bytes());
    out.extend_from_slice(&t.sc.to_le_bytes());
    out.push(t.need);
    out.push(t.need_level);
    out.push(t.need_identify);
    out.push(0);
    out.extend_from_slice(&t.price.to_le_bytes());
    out.extend_from_slice(&t.stock.to_le_bytes());
    out.push(t.atk_spd);
    out.push(t.agility);
    out.push(t.accurate);
    out.push(t.mg_avoid);
    out.push(t.strong);
    out.push(t.undead);
    out.push(0);
    out.push(0);
    out.extend_from_slice(&t.hp_add.to_le_bytes());
    out.extend_from_slice(&t.mp_add.to_le_bytes());
    out.extend_from_slice(&t.exp_add.to_le_bytes());
    out.push(t.eff_type1);
    out.push(t.eff_rate1);
    out.push(t.eff_value1);
    out.push(t.eff_type2);
    out.push(t.eff_rate2);
    out.push(t.eff_value2);
    out.push(t.slowdown);
    out.push(t.tox);
    out.push(t.tox_avoid);
    out.push(t.unique_item);
    out.push(t.overlap_item);
    out.push(t.light);
    out.push(t.item_type);
    out.push(0);
    out.extend_from_slice(&t.item_set.to_le_bytes());
    write_ascii_string(&mut out, &t.reference, ITEM_NAME_LEN);
    out.push(0);
    out.extend_from_slice(&ci.make_index.to_le_bytes());
    out.extend_from_slice(&ci.dura.to_le_bytes());
    out.extend_from_slice(&ci.dura_max.to_le_bytes());
    out.extend_from_slice(&ci.desc);
    out.push(0);
    out.push(0);
    out
}

/// `ClientMagic.WritePacket`（`src/OpenMir2/Packets/ClientPackets/ClientMagic.cs:29-40`）：
/// `Key(byte) + Level + 0 + 0 + CurTrain(i32) + MagicInfo.GetBuffer()`。
///
/// `MagicInfo.WritePacket`（`src/OpenMir2/Data/MagicInfo.cs:79-123`）= 76B：
/// `MagicId(u16) + WriteAsciiString(Name,14) + EffectType + Effect + 0 + Spell(u16) + Power(u16)
///  + TrainLevel[4] + MaxTrain[4]×i32 + TrainLv + Job + 0 + 0 + DelayTime(i32)
///  + DefSpell + DefPower + MaxPower(u16) + DefMaxPower + WriteAsciiString(Desc,15) + 0×3`。
/// ⇒ `ClientMagic` 固定 84B。
#[must_use]
pub fn encode_client_magic(
    key: u16,
    level: u8,
    cur_train: i32,
    def: &mir2_data::MagicInfo,
) -> Vec<u8> {
    let mut out = Vec::with_capacity(84);
    // `writer.Write((byte)Key)`：C# `char` 的**低 8 位**。
    out.push(key as u8);
    out.push(level);
    out.push(0);
    out.push(0);
    out.extend_from_slice(&cur_train.to_le_bytes());
    out.extend_from_slice(&encode_magic_info(def));
    out
}

/// `MagicInfo.WritePacket` 的字节体（见 [`encode_client_magic`]；固定 76B）。
#[must_use]
pub fn encode_magic_info(m: &mir2_data::MagicInfo) -> Vec<u8> {
    let mut out = Vec::with_capacity(76);
    out.extend_from_slice(&m.magic_id.to_le_bytes());
    write_ascii_string(&mut out, &m.magic_name, ITEM_NAME_LEN);
    out.push(m.effect_type);
    out.push(m.effect);
    out.push(0);
    out.extend_from_slice(&m.spell.to_le_bytes());
    out.extend_from_slice(&m.power.to_le_bytes());
    out.extend_from_slice(&m.train_level);
    for v in m.max_train {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out.push(m.train_lv);
    out.push(m.job);
    out.push(0);
    out.push(0);
    out.extend_from_slice(&m.delay_time.to_le_bytes());
    out.push(m.def_spell);
    out.push(m.def_power);
    out.extend_from_slice(&m.max_power.to_le_bytes());
    out.push(m.def_max_power);
    write_ascii_string(&mut out, &m.desc, 15);
    out.push(0);
    out.push(0);
    out.push(0);
    out
}

// ---------------------------------------------------------------------------
// `ChangeItemWithLevel` / `ChangeItemByJob`（`ClientItem` 的展示层变换）
// ---------------------------------------------------------------------------

const DRAGON_RING_SHAPE: u8 = 198;
const DRAGON_BRACELET_SHAPE: u8 = 199;
const DRAGON_NECKLACE_SHAPE: u8 = 200;
const DRAGON_DRESS_SHAPE: u8 = 10;
const DRAGON_HELMET_SHAPE: u8 = 201;
const DRAGON_WEAPON_SHAPE: u8 = 37;
const DRESS_SHAPE_WING: u8 = 9;
const LOLLIPOP_SHAPE: u8 = 1;
const GOLDMEDAL_SHAPE: u8 = 2;
const SILVERMEDAL_SHAPE: u8 = 3;
const BRONZEMEDAL_SHAPE: u8 = 4;

fn lo_byte(v: u16) -> u16 {
    v & 0xFF
}

fn hi_byte(v: u16) -> u16 {
    v >> 8
}

/// `HUtil32.MakeWord(ushort, ushort)`（`HUtil32.cs:66-69`）：`(ushort)(low | (high << 8))`。
///
/// **不是** byte 重载：`low` 的第 8 位会串进高字节（`MakeWord(256, h)` ⇒ 高字节 `|1`），
/// `low == 0xFFFE` 时整值就是 `0xFFFE`（`MakeWord(65534, 0)`）。`ChangeItemWithLevel/ByJob`
/// 里的 `(ushort)(LoByte+1)` / `(ushort)(LoByte-2)` 正是这两类边界。
fn make_word16(low: u16, high: u16) -> u16 {
    low | (high << 8)
}

/// `BaseObject.Abil.ChangeItemWithLevel`（`BaseObject.Abil.cs:99-129`）：**只**对
/// `Shape==DRESS_SHAPE_WING(9)` 且 `StdMode 10/11` 的衣服按等级加 AC/MAC/DC/MC/SC。
pub fn change_item_with_level(t: &mut mir2_data::StdItem, level: u8) {
    if t.shape != DRESS_SHAPE_WING || (t.std_mode != 10 && t.std_mode != 11) {
        return;
    }
    if level > 20 {
        let (dc, mc, sc, ac, mac) = if level < 40 {
            (
                make_word16(0, 1),
                make_word16(0, 2),
                make_word16(0, 2),
                make_word16(2, 3),
                make_word16(0, 2),
            )
        } else if level < 50 {
            (
                make_word16(0, 3),
                make_word16(0, 4),
                make_word16(0, 4),
                make_word16(5, 5),
                make_word16(1, 2),
            )
        } else {
            (
                make_word16(0, 5),
                make_word16(0, 6),
                make_word16(0, 6),
                make_word16(9, 7),
                make_word16(2, 4),
            )
        };
        t.dc = t.dc.wrapping_add(dc);
        t.mc = t.mc.wrapping_add(mc);
        t.sc = t.sc.wrapping_add(sc);
        t.ac = t.ac.wrapping_add(ac);
        t.mac = t.mac.wrapping_add(mac);
    }
}

/// `PlayObject.ChangeItemByJob`（`PlayObject.cs:4711-4878`）的 1:1 移植。
///
/// 只在龙系外观（`Shape 198/199/200/201/37`）与 `StdMode 53` 的棒棒糖/奖牌上按职业裁剪数值；
/// `Job` 取 `PlayerJob`（0 战士 / 1 法师 / 2 道士），其它值不命中任何 `switch` 支。
pub fn change_item_by_job(t: &mut mir2_data::StdItem, job: u8) {
    if t.std_mode == 22 && t.shape == DRAGON_RING_SHAPE {
        match job {
            0 => {
                t.dc = make_word16(lo_byte(t.dc), 255.min(hi_byte(t.dc) + 4));
                t.mc = 0;
                t.sc = 0;
            }
            1 => {
                t.dc = 0;
                t.sc = 0;
            }
            2 => t.mc = 0,
            _ => {}
        }
    } else if t.std_mode == 26 && t.shape == DRAGON_BRACELET_SHAPE {
        match job {
            0 => {
                t.dc = make_word16(lo_byte(t.dc) + 1, 255.min(hi_byte(t.dc) + 2));
                t.mc = 0;
                t.sc = 0;
                t.ac = make_word16(lo_byte(t.ac), 255.min(hi_byte(t.ac) + 1));
            }
            1 => {
                t.dc = 0;
                t.sc = 0;
                t.ac = make_word16(lo_byte(t.ac), 255.min(hi_byte(t.ac) + 1));
            }
            2 => t.mc = 0,
            _ => {}
        }
    } else if (t.std_mode == 19 && t.shape == DRAGON_NECKLACE_SHAPE)
        || ((t.std_mode == 10 || t.std_mode == 11) && t.shape == DRAGON_DRESS_SHAPE)
        || (t.std_mode == 15 && t.shape == DRAGON_HELMET_SHAPE)
    {
        // oracle 的三个分支（项链/衣服/头盔）**逐字相同** ⇒ 合并（行为不变）。
        match job {
            0 => {
                t.mc = 0;
                t.sc = 0;
            }
            1 => {
                t.dc = 0;
                t.sc = 0;
            }
            2 => {
                t.dc = 0;
                t.mc = 0;
            }
            _ => {}
        }
    } else if (t.std_mode == 5 || t.std_mode == 6) && t.shape == DRAGON_WEAPON_SHAPE {
        match job {
            0 => {
                t.dc = make_word16(lo_byte(t.dc) + 1, 255.min(hi_byte(t.dc) + 28));
                t.mc = 0;
                t.sc = 0;
                // `(ushort)(LoByte(AC) - 2)`：byte→int→ushort 回绕。
                t.ac = make_word16(lo_byte(t.ac).wrapping_sub(2), hi_byte(t.ac));
            }
            1 => {
                t.sc = 0;
                t.mac = if hi_byte(t.mac) > 12 {
                    make_word16(lo_byte(t.mac), hi_byte(t.mac) - 12)
                } else {
                    make_word16(lo_byte(t.mac), 0)
                };
            }
            2 => {
                t.dc = make_word16(lo_byte(t.dc) + 2, 255.min(hi_byte(t.dc) + 10));
                t.mc = 0;
                t.ac = make_word16(lo_byte(t.ac).wrapping_sub(2), hi_byte(t.ac));
            }
            _ => {}
        }
    } else if t.std_mode == 53 {
        if t.shape == LOLLIPOP_SHAPE {
            match job {
                0 => {
                    t.dc = make_word16(lo_byte(t.dc), 255.min(hi_byte(t.dc) + 2));
                    t.mc = 0;
                    t.sc = 0;
                }
                1 => {
                    t.dc = 0;
                    t.mc = make_word16(lo_byte(t.mc), 255.min(hi_byte(t.mc) + 2));
                    t.sc = 0;
                }
                2 => {
                    t.dc = 0;
                    t.mc = 0;
                    t.sc = make_word16(lo_byte(t.sc), 255.min(hi_byte(t.sc) + 2));
                }
                _ => {}
            }
        } else if t.shape == GOLDMEDAL_SHAPE
            || t.shape == SILVERMEDAL_SHAPE
            || t.shape == BRONZEMEDAL_SHAPE
        {
            match job {
                0 => {
                    t.dc = make_word16(lo_byte(t.dc), 255.min(hi_byte(t.dc)));
                    t.mc = 0;
                    t.sc = 0;
                }
                1 => {
                    t.dc = 0;
                    t.mc = make_word16(lo_byte(t.mc), 255.min(hi_byte(t.mc)));
                    t.sc = 0;
                }
                2 => {
                    t.dc = 0;
                    t.mc = 0;
                    t.sc = make_word16(lo_byte(t.sc), 255.min(hi_byte(t.sc)));
                }
                _ => {}
            }
        }
    }
}

/// `ClientQueryBagItems` 的**尾**：每件 `EncodeBuffer(clientItem) + '/'`（`Operate.cs:56`）。
#[must_use]
pub fn bagitems_tail(items: &[ClientItem<'_>]) -> Vec<u8> {
    let mut out = Vec::new();
    for ci in items {
        out.extend_from_slice(&edcode::encode(&encode_client_item(ci)));
        out.push(b'/');
    }
    out
}

/// `SM_BAGITEMS`（`Operate.cs:59-62`）：`MakeMessage(SM_BAGITEMS, ActorId, 0, 0, ItemList.Count)` + 尾原样。
#[must_use]
pub fn sm_bagitems(actor_id: i32, count: usize, tail: &[u8]) -> Vec<u8> {
    let series = u16::try_from(count).unwrap_or(u16::MAX);
    let header =
        smframe::command_header(actor_id, mir2_protocol::messages::SM_BAGITEMS, 0, 0, series);
    smframe::gm_data_body(header, tail)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tpl() -> StdItem {
        StdItem {
            name: "木剑".into(),
            std_mode: 5,
            shape: 1,
            weight: 3,
            ani_count: 0,
            looks: 100,
            dura_max: 20,
            ac: 0,
            mac: 0,
            dc: 3,
            mc: 0,
            sc: 0,
            price: 10,
            reference: String::new(),
            ..StdItem::default()
        }
    }

    /// `WriteAsciiString` 三态：定长 14 + 1B 长度；空串长度字节 0；超长截断且长度字节 = 14。
    #[test]
    fn write_ascii_string_matches_oracle_semantics() {
        let mut out = Vec::new();
        write_ascii_string(&mut out, "木剑", 14);
        assert_eq!(out.len(), 15, "1B 长度 + 14B 定长");
        assert_eq!(out[0], 4, "GBK 两字 = 4 字节");
        assert_eq!(&out[1..5], &GBK.encode("木剑").0[..]);
        assert!(out[5..].iter().all(|&b| b == 0), "补零到 14");

        let mut empty = Vec::new();
        write_ascii_string(&mut empty, "", 14);
        assert_eq!(empty.len(), 15);
        assert_eq!(empty[0], 0);
        assert!(empty[1..].iter().all(|&b| b == 0));

        let mut long = Vec::new();
        write_ascii_string(&mut long, "0123456789ABCDEFGH", 14);
        assert_eq!(long.len(), 15);
        assert_eq!(long[0], 14, "超长 ⇒ 长度字节 = 14（截断）");
        assert_eq!(&long[1..], b"0123456789ABCD");
    }

    /// `encode_client_item` 的总长固定 124B（`ClientItem.WritePacket` 字段序逐项），并校验几处关键偏移。
    #[test]
    fn encode_client_item_layout_is_stable() {
        let t = tpl();
        let ci = ClientItem {
            tpl: &t,
            name: t.name.clone(),
            make_index: 29_033_301,
            dura: 20,
            dura_max: 20,
            desc: [0; 14],
        };
        let b = encode_client_item(&ci);
        assert_eq!(b.len(), 124, "ClientItem 定长 124B");
        // Name(15B) → StdMode@15 / Shape@16 / Weight@17 / AniCount@18 / SpecialPwr@19 / ItemDesc@20 / pad@21
        // → Looks@22..24 → DuraMax@24..26.
        assert_eq!(b[15], 5, "StdMode");
        assert_eq!(b[22..24], 100u16.to_le_bytes(), "Looks");
        assert_eq!(b[24..26], 20u16.to_le_bytes(), "DuraMax");
        // 尾部：MakeIndex@100 / Dura@104 / DuraMax@106 / Desc@108..122 / 2B pad。
        assert_eq!(b[100..104], 29_033_301i32.to_le_bytes(), "MakeIndex");
        assert_eq!(b[104..106], 20u16.to_le_bytes(), "Dura");
        assert_eq!(b[106..108], 20u16.to_le_bytes(), "DuraMax");
        assert!(b[108..].iter().all(|&x| x == 0), "Desc+2 pad 全 0");
    }

    /// 尾：每件 `Encode(ClientItem) + '/'`，件数与长度随 `ItemList.Count`。
    #[test]
    fn bagitems_tail_joins_encoded_items_with_slash() {
        let t = tpl();
        let one = ClientItem {
            tpl: &t,
            name: t.name.clone(),
            make_index: 1,
            dura: 5,
            dura_max: 20,
            desc: [0; 14],
        };
        let tail = bagitems_tail(&[one]);
        assert_eq!(tail.last(), Some(&b'/'));
        // 去掉末尾 '/' 后应等于 `edcode::encode(encode_client_item(...))`。
        let expect = edcode::encode(&encode_client_item(&ClientItem {
            tpl: &t,
            name: t.name.clone(),
            make_index: 1,
            dura: 5,
            dura_max: 20,
            desc: [0; 14],
        }));
        assert_eq!(&tail[..tail.len() - 1], &expect[..]);
    }

    /// `ClientMagic` 固定 84B（`ClientMagic.WritePacket` 4 + `MagicInfo.GetBuffer` 76 + i32 4）。
    /// 偏移逐项钉死（G-3）。
    #[test]
    fn encode_client_magic_is_84_bytes_with_oracle_offsets() {
        let m = mir2_data::MagicInfo {
            magic_id: 17,
            magic_name: "heal".into(),
            effect_type: 3,
            effect: 4,
            spell: 5,
            power: 6,
            train_level: [1, 2, 3, 4],
            max_train: [10, 20, 30, 40],
            train_lv: 3,
            job: 1,
            delay_time: 100,
            def_spell: 7,
            def_power: 8,
            max_power: 9,
            def_max_power: 10,
            desc: "d".into(),
        };
        let b = encode_client_magic(0x41, 2, 100, &m);
        assert_eq!(b.len(), 84);
        assert_eq!(
            &b[0..8],
            &[0x41, 2, 0, 0, 100, 0, 0, 0],
            "Key/Level/pad/CurTrain"
        );
        assert_eq!(u16::from_le_bytes([b[8], b[9]]), 17, "MagicId");
        assert_eq!(b[10], 4, "名前缀长度");
        assert_eq!(&b[11..15], b"heal");
        assert_eq!(&b[15..25], &[0u8; 10], "定长 14 补零");
        assert_eq!((b[25], b[26], b[27]), (3, 4, 0));
        assert_eq!(u16::from_le_bytes([b[28], b[29]]), 5, "Spell");
        assert_eq!(u16::from_le_bytes([b[30], b[31]]), 6, "Power");
        assert_eq!(&b[32..36], &[1, 2, 3, 4], "TrainLevel[4]");
        assert_eq!(
            i32::from_le_bytes(b[36..40].try_into().unwrap()),
            10,
            "MaxTrain[0]"
        );
        assert_eq!((b[52], b[53], b[54], b[55]), (3, 1, 0, 0));
        assert_eq!(i32::from_le_bytes(b[56..60].try_into().unwrap()), 100);
        assert_eq!((b[60], b[61]), (7, 8));
        assert_eq!(u16::from_le_bytes([b[62], b[63]]), 9, "MaxPower");
        assert_eq!(b[64], 10);
        assert_eq!(b[65], 1, "Desc 前缀长度");
        assert_eq!(b[66], b'd');
        assert_eq!(&b[81..84], &[0, 0, 0]);
        assert_eq!(encode_magic_info(&m).len(), 76);
    }

    /// `ChangeItemWithLevel`（G-2）：`Shape 9` + `StdMode 10/11` 且 `level>20` 才加值。
    #[test]
    fn change_item_with_level_only_for_wing_dress() {
        let mut t = mir2_data::StdItem {
            std_mode: 10,
            shape: 9,
            dc: 0x0100,
            mc: 0,
            sc: 0,
            ac: 0x0202,
            mac: 0,
            ..mir2_data::StdItem::default()
        };
        change_item_with_level(&mut t, 45); // 40..50 档
        assert_eq!(t.dc, 0x0100 + 0x0300);
        assert_eq!(t.mc, 0x0400);
        assert_eq!(t.sc, 0x0400);
        assert_eq!(t.ac, 0x0202 + 0x0505);
        assert_eq!(t.mac, 0x0201);
        // 等级 <=20：不变。
        let mut t2 = t.clone();
        change_item_with_level(&mut t2, 20);
        assert_eq!(t2, t);
        // 非 wing / 非衣服：不变。
        let mut t3 = mir2_data::StdItem {
            std_mode: 10,
            shape: 8,
            ..mir2_data::StdItem::default()
        };
        change_item_with_level(&mut t3, 60);
        assert_eq!(
            t3,
            mir2_data::StdItem {
                std_mode: 10,
                shape: 8,
                ..Default::default()
            }
        );
    }

    /// `ChangeItemByJob`（G-2）：龙系外观按职业裁剪；武器战士支的 AC 低位 `0-2` 按 ushort 回绕。
    #[test]
    fn change_item_by_job_dragon_branches() {
        let mut ring = mir2_data::StdItem {
            std_mode: 22,
            shape: 198,
            dc: 0x0503,
            mc: 0x1111,
            sc: 0x2222,
            ..mir2_data::StdItem::default()
        };
        change_item_by_job(&mut ring, 0);
        assert_eq!(ring.dc, 0x0903, "Lo=3, Hi=5+4");
        assert_eq!((ring.mc, ring.sc), (0, 0));

        let mut weapon = mir2_data::StdItem {
            std_mode: 5,
            shape: 37,
            dc: 0x0100,
            ac: 0x0000,
            ..mir2_data::StdItem::default()
        };
        change_item_by_job(&mut weapon, 0);
        assert_eq!(weapon.dc, 0x1D01, "Lo=0+1, Hi=1+28");
        // `(ushort)(LoByte(AC) - 2)`：0-2 回绕成 0xFFFE；`MakeWord(0xFFFE, Hi)` = 0xFFFE
        // （bit8 串进高字节，但高字节本就是 0）。
        assert_eq!(weapon.ac, 0xFFFE);

        // 边界（ushort 重载）：`LoByte(DC)=255` ⇒ `(ushort)(255+1)=0x0100` ⇒ 高字节 `|1`。
        let mut brace = mir2_data::StdItem {
            std_mode: 26,
            shape: 199,
            dc: 0x04FF,
            ac: 0,
            ..mir2_data::StdItem::default()
        };
        change_item_by_job(&mut brace, 0);
        // low=(0xFF+1)=0x0100；high=min(255, 4+2)=6 ⇒ `6<<8 | 0x0100` = 0x0700（bit8 串入）。
        assert_eq!(brace.dc, 0x0700, "ushort 重载：low 的 bit8 串进高字节");

        let mut dress = mir2_data::StdItem {
            std_mode: 11,
            shape: 10,
            dc: 0x1111,
            mc: 0x2222,
            sc: 0x3333,
            ..mir2_data::StdItem::default()
        };
        change_item_by_job(&mut dress, 1);
        assert_eq!((dress.dc, dress.mc, dress.sc), (0, 0x2222, 0));
    }
}
