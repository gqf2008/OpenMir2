//! 角色记录（`LoadPlayerDataPacket{ChrName, HumDataInfo{Header, Data}}`）的**头部解析**。
//!
//! 本切片只解析到世界入口要用的字段（`ChrName` / `CurMap` / `CurX` / `CurY` / `Dir`）；
//! 后面的物品/技能/任务数组（`HumItems[13]`/`BagItems[46]`/`StorageItems[50]`/`Magic[20]`/`QuestUnit*[128]`）
//! 属后续切片（登记）。
//!
//! 布局**实测**（部署件 `OpenMir2.dll` 序列化一个样例，见 `tests/fixtures/player_data_oracle.txt`）：
//! MemoryPack 对 class / 含 string 的 struct 先写**成员数字节**，标量按裸小端、`string` = `i32(字符数)` + UTF-16LE、
//! `bool` = 1 字节、`f64` = 8 字节。逐段：
//! ```text
//! 02                      LoadPlayerDataPacket 成员数 2
//! 04 000000 "lb1a"        ChrName
//! 02                      CharacterDataInfo 成员数 2
//! 07                      RecordHeader 成员数 7（sAccount, Name, SelectID, dCreateDate, Deleted, UpdateDate, CreateDate）
//! … 七个字段 …
//! 34                      CharacterData 成员数 **52**（0x34；与 PlayerDataInfo.cs:59-129 的字段数一致）
//! 01                      ServerIndex
//! 04 000000 "lb1a"        ChrName
//! 01 000000 "0"           CurMap
//! 6400                    CurX = 100
//! c800                    CurY = 200
//! 03                      Dir = 3
//! ```

/// 角色记录里本切片用到的头部字段。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerDataHead {
    pub chr_name: String,
    pub cur_map: String,
    pub cur_x: i16,
    pub cur_y: i16,
    pub dir: u8,
    /// `Header.sAccount`（账号）——世界侧暂不需要，留着日志用。
    pub account: String,
}

struct Cursor<'a> {
    b: &'a [u8],
    i: usize,
}

impl<'a> Cursor<'a> {
    fn new(b: &'a [u8]) -> Self {
        Self { b, i: 0 }
    }

    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        if self.i + n > self.b.len() {
            return None;
        }
        let s = &self.b[self.i..self.i + n];
        self.i += n;
        Some(s)
    }

    fn u8(&mut self) -> Option<u8> {
        Some(self.take(1)?[0])
    }

    fn i16(&mut self) -> Option<i16> {
        Some(i16::from_le_bytes(self.take(2)?.try_into().ok()?))
    }

    fn i32(&mut self) -> Option<i32> {
        Some(i32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }

    fn f64(&mut self) -> Option<()> {
        self.take(8)?;
        Some(())
    }

    /// MemoryPack 的 `string`：`i32(字符数)` + UTF-16LE。
    fn string(&mut self) -> Option<String> {
        let n = usize::try_from(self.i32()?).ok()?;
        let bytes = self.take(n.checked_mul(2)?)?;
        let mut units: Vec<u16> = Vec::with_capacity(n);
        for c in bytes.as_chunks::<2>().0 {
            units.push(u16::from_le_bytes(*c));
        }
        Some(String::from_utf16_lossy(&units))
    }
}

/// 解析 `LoadPlayerDataPacket` 的头部窗口（不足 ⇒ `None`）。
#[must_use]
pub fn parse_player_data_head(bytes: &[u8]) -> Option<PlayerDataHead> {
    let mut c = Cursor::new(bytes);
    if c.u8()? != 2 {
        return None; // LoadPlayerDataPacket 成员数
    }
    let packet_chr_name = c.string()?;
    if c.u8()? != 2 {
        return None; // CharacterDataInfo 成员数
    }
    // RecordHeader（struct，含 string ⇒ 有成员数前缀）
    if c.u8()? != 7 {
        return None;
    }
    let account = c.string()?; // sAccount
    let header_name = c.string()?; // Name
    c.i32()?; // SelectID
    c.f64()?; // dCreateDate
    c.u8()?; // Deleted
    c.f64()?; // UpdateDate
    c.f64()?; // CreateDate
              // CharacterData（class）：52 个成员
    if c.u8()? != 52 {
        return None;
    }
    c.u8()?; // ServerIndex
    let data_chr_name = c.string()?; // Data.ChrName
    let cur_map = c.string()?;
    let cur_x = c.i16()?;
    let cur_y = c.i16()?;
    let dir = c.u8()?;
    Some(PlayerDataHead {
        // `LoadPlayerDataPacket.ChrName` 与 `Data.ChrName` 都记了角色名；不一致时以 Data 为准并交给调用方判断
        chr_name: if data_chr_name.is_empty() {
            packet_chr_name
        } else {
            data_chr_name
        },
        cur_map,
        cur_x,
        cur_y,
        dir,
        account: if account.is_empty() {
            header_name
        } else {
            account
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(s: &str) -> Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }

    /// oracle 实测头部窗口（部署件序列化的样例：ChrName=lb1a / sAccount=loadbot1 / CurMap="0" /
    /// CurX=100 / CurY=200 / Dir=3 / Gold=12345；见 `tests/fixtures/player_data_oracle.txt`）。
    const ORACLE_HEAD: &str = "02040000006c006200310061000207080000006c006f006100640062006f0074003100040000006c0062003100610005000000000000000000f83f0000000000000004400000000000000c403401040000006c006200310061000100000030006400c8000302010039300000160000000000000000000000";

    #[test]
    fn parses_oracle_head() {
        let head = parse_player_data_head(&hex(ORACLE_HEAD)).expect("oracle 头部窗口必须能解析");
        assert_eq!(head.chr_name, "lb1a");
        assert_eq!(head.account, "loadbot1");
        assert_eq!(head.cur_map, "0");
        assert_eq!(head.cur_x, 100);
        assert_eq!(head.cur_y, 200);
        assert_eq!(head.dir, 3);
    }

    #[test]
    fn truncated_or_mismatched_prefix_is_rejected() {
        let full = hex(ORACLE_HEAD);
        assert!(parse_player_data_head(&full[..20]).is_none(), "不足 ⇒ None");
        let mut bad = full.clone();
        bad[0] = 3; // LoadPlayerDataPacket 成员数不对
        assert!(parse_player_data_head(&bad).is_none());
        let mut bad2 = full;
        // CharacterData 的成员数（0x34=52）在**偏移 76**（逐段手算见 fixture 的说明；不是随便搜 0x34——那会命中字符串/日期里的字节）
        bad2[76] = 0x33;
        assert!(parse_player_data_head(&bad2).is_none());
    }
}
