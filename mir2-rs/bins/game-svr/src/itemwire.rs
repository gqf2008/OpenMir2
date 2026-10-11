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
fn write_ascii_string(out: &mut Vec<u8>, value: &str, default_size: usize) {
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
}
