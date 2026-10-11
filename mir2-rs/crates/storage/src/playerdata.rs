//! DBSrv **6000 `DataService` 面**的记录与消息类型 —— 与 C# **MemoryPack** 逐字节兼容。
//!
//! 来源：`src/OpenMir2/Packets/{ClientPackets,ServerPackets}` +
//! `src/OpenMir2/Data/RecordHeader.cs`，字段顺序/类型**逐字**对齐 C# 的属性声明顺序
//! （MemoryPack 的对象布局按声明序写成员，对象头是成员数）。
//!
//! 线上形态（DBSrv 6000，`PlayerDataFixedHeaderDataHandlingAdapter`，HeaderLength=6）：
//! `ServerDataPacket(6B)` + `MemoryPack(ServerRequestData)`；体内的 `Message`/`Packet`/`Sign`
//! 是 **EDCode 编码后的字节数组**，再各自 MemoryPack（`SerializerUtil` 用
//! `MemoryPackSerializerOptions.Utf16`，即字符串按 UTF-16LE 编码）。
//!
//! 关键 MemoryPack 事实（**实测**自 C# 真身，向量见 `tests/parity/vectors/oracle_vectors.jsonl`
//! 的 `kind:"playerdatainfo"`）：
//! - 对象头 = 成员数（1 字节，`>= 249` 走 `0xF9`+i32，本组类型不会触发）；
//! - 引用类型/`string`/数组的 `null` 编码为 **i32 `-1`**（`ff ff ff ff`），空串/空数组是 `0`；
//! - `string` = i32 UTF-16 码元数 + UTF-16LE；`char[]` 同上但元素是单个 UTF-16 码元；
//! - 数组 = i32 元素数 + 逐元素；元素是引用类型时各自带对象头或 `null` 标记；
//! - 对象头不写"null 位图"（`CharacterData` 52 个成员实测无额外字节）。
//!
//! **`Ability` 的 `AbilityFormatter` 是死代码**：C# 里从未注册，源码生成器按声明序编码
//! （实测 `Reserved2` 落在声明位置，而非 formatter 的末位）——本实现照实测。

// 字段与 C# 逐字对应，文案以 C# 侧为准；此处不逐字段重复散文（见 C# 源文件）。
#![allow(missing_docs)]
// 记录类型 1:1 镜像 C#，字段数、bool 数与函数长度由外部格式决定。
#![allow(
    clippy::struct_excessive_bools,
    clippy::too_many_lines,
    clippy::doc_markdown
)]

use crate::StorageError;

/// MemoryPack 对象头中"成员数"转义的起始值（C# `MemoryPackWriter.WriteObjectHeader`）。
const OBJ_HEADER_ESCAPE: u8 = 0xF9;
/// MemoryPack `null` 标记（i32 -1）。
const NULL_I32: [u8; 4] = [0xFF, 0xFF, 0xFF, 0xFF];

/// 编码缓冲写入器（MemoryPack，Utf16 选项）。
#[doc(hidden)]
#[derive(Default)]
pub struct Writer {
    buf: Vec<u8>,
}

impl Writer {
    fn u8(&mut self, v: u8) {
        self.buf.push(v);
    }
    fn bool(&mut self, v: bool) {
        self.buf.push(u8::from(v));
    }
    fn i16(&mut self, v: i16) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }
    fn u16(&mut self, v: u16) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }
    fn i32(&mut self, v: i32) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }
    fn f64(&mut self, v: f64) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }
    /// 非空对象的对象头（成员数）。
    fn header(&mut self, members: usize) {
        let n = u8::try_from(members).expect("member count < 256");
        assert!(
            n < OBJ_HEADER_ESCAPE,
            "member count escape path not implemented"
        );
        self.buf.push(n);
    }
    /// `null` **对象引用**（1 字节 `0xFF`：对象头只有"成员数"一种合法值，`>= 0xF9` 是转义，
    /// `0xFF` 永不是合法头）。
    fn null_ref(&mut self) {
        self.buf.push(0xFF);
    }
    /// `null` **长度前缀**（字符串/字节数组/普通数组共用 i32 `-1`）。
    fn null_len(&mut self) {
        self.buf.extend_from_slice(&NULL_I32);
    }
    /// `Option<String>`：None → null，Some → UTF-16LE。
    fn string(&mut self, v: Option<&str>) {
        match v {
            None => self.null_len(),
            Some(s) => {
                let units: Vec<u16> = s.encode_utf16().collect();
                self.i32(i32::try_from(units.len()).expect("string fits i32"));
                for u in units {
                    self.u16(u);
                }
            }
        }
    }
    /// `Option<byte[]>`。
    fn bytes(&mut self, v: Option<&[u8]>) {
        match v {
            None => self.null_len(),
            Some(b) => {
                self.i32(i32::try_from(b.len()).expect("bytes fit i32"));
                self.buf.extend_from_slice(b);
            }
        }
    }
    /// `Option<char[]>`（UTF-16 码元数组，元素 = 单个码元）。
    fn chars(&mut self, v: Option<&[u16]>) {
        match v {
            None => self.null_len(),
            Some(c) => {
                self.i32(i32::try_from(c.len()).expect("chars fit i32"));
                for u in c {
                    self.u16(*u);
                }
            }
        }
    }
    /// `Option<ushort[]>`。
    fn u16s(&mut self, v: Option<&[u16]>) {
        match v {
            None => self.null_len(),
            Some(c) => {
                self.i32(i32::try_from(c.len()).expect("u16s fit i32"));
                for u in c {
                    self.u16(*u);
                }
            }
        }
    }
}

/// 解码读取器。
#[doc(hidden)]
pub struct Reader<'a> {
    b: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(b: &'a [u8]) -> Self {
        Self { b, pos: 0 }
    }
    fn take(&mut self, n: usize) -> Result<&'a [u8], StorageError> {
        let end = self
            .pos
            .checked_add(n)
            .ok_or_else(|| StorageError::Corrupt("memorypack: length overflow".into()))?;
        if end > self.b.len() {
            return Err(StorageError::Corrupt("memorypack: truncated".into()));
        }
        let s = &self.b[self.pos..end];
        self.pos = end;
        Ok(s)
    }
    fn u8(&mut self) -> Result<u8, StorageError> {
        Ok(self.take(1)?[0])
    }
    fn bool(&mut self) -> Result<bool, StorageError> {
        Ok(self.u8()? != 0)
    }
    fn i16(&mut self) -> Result<i16, StorageError> {
        Ok(i16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }
    fn u16(&mut self) -> Result<u16, StorageError> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }
    fn i32(&mut self) -> Result<i32, StorageError> {
        Ok(i32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn f64(&mut self) -> Result<f64, StorageError> {
        Ok(f64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }
    /// 读对象头：`Ok(None)` = null 引用（消费 1 字节 `0xFF`），`Ok(Some(n))` = 成员数。
    fn header(&mut self) -> Result<Option<u32>, StorageError> {
        let first = self.u8()?;
        if first == 0xFF {
            return Ok(None);
        }
        if first >= OBJ_HEADER_ESCAPE {
            if first != OBJ_HEADER_ESCAPE {
                return Err(StorageError::Corrupt(format!(
                    "memorypack: object header escape {first:#x} unsupported"
                )));
            }
            let n = self.i32()?;
            return u32::try_from(n)
                .map(Some)
                .map_err(|_| StorageError::Corrupt("memorypack: negative member count".into()));
        }
        Ok(Some(u32::from(first)))
    }
    /// 期望的非空对象头：成员数必须等于 `want`。
    fn header_expect(&mut self, want: u32) -> Result<(), StorageError> {
        match self.header()? {
            Some(n) if n == want => Ok(()),
            Some(n) => Err(StorageError::Corrupt(format!(
                "memorypack: member count {n} != {want}"
            ))),
            None => Err(StorageError::Corrupt(
                "memorypack: unexpected null object".into(),
            )),
        }
    }
    fn string(&mut self) -> Result<Option<String>, StorageError> {
        let n = self.i32()?;
        if n == -1 {
            return Ok(None);
        }
        let n = usize::try_from(n)
            .map_err(|_| StorageError::Corrupt("memorypack: negative string len".into()))?;
        let raw = self.take(n * 2)?;
        let mut units = Vec::with_capacity(n);
        for c in raw.as_chunks::<2>().0 {
            units.push(u16::from_le_bytes(*c));
        }
        String::from_utf16(&units)
            .map(Some)
            .map_err(|_| StorageError::Corrupt("memorypack: invalid utf16 string".into()))
    }
    fn bytes(&mut self) -> Result<Option<Vec<u8>>, StorageError> {
        let n = self.i32()?;
        if n == -1 {
            return Ok(None);
        }
        let n = usize::try_from(n)
            .map_err(|_| StorageError::Corrupt("memorypack: negative byte len".into()))?;
        Ok(Some(self.take(n)?.to_vec()))
    }
    fn chars(&mut self) -> Result<Option<Vec<u16>>, StorageError> {
        let n = self.i32()?;
        if n == -1 {
            return Ok(None);
        }
        let n = usize::try_from(n)
            .map_err(|_| StorageError::Corrupt("memorypack: negative char len".into()))?;
        let raw = self.take(n * 2)?;
        Ok(Some(
            raw.as_chunks::<2>()
                .0
                .iter()
                .map(|c| u16::from_le_bytes(*c))
                .collect(),
        ))
    }
    fn u16s(&mut self) -> Result<Option<Vec<u16>>, StorageError> {
        self.chars()
    }
    /// 元素个数（数组长度；`-1` = null 数组）。
    fn count(&mut self) -> Result<Option<usize>, StorageError> {
        let n = self.i32()?;
        if n == -1 {
            return Ok(None);
        }
        usize::try_from(n)
            .map(Some)
            .map_err(|_| StorageError::Corrupt("memorypack: negative array len".into()))
    }
    fn finish(&self) -> Result<(), StorageError> {
        if self.pos == self.b.len() {
            Ok(())
        } else {
            Err(StorageError::Corrupt(format!(
                "memorypack: {} trailing bytes",
                self.b.len() - self.pos
            )))
        }
    }
}

/// 每个记录/消息类型都实现它（`to_bytes`/`from_bytes` 由 trait 提供）。
pub trait MemPack: Sized {
    /// 写入成员体（不含对象头；对象头由 `to_bytes`/调用方负责）。
    fn write_body(&self, w: &mut Writer);
    /// 读成员体（对象头已由调用方消费）。
    fn read_body(r: &mut Reader<'_>) -> Result<Self, StorageError>;

    /// 序列化（对象头 + 成员体）。
    fn to_bytes(&self) -> Vec<u8> {
        let mut w = Writer::default();
        w.header(Self::members() as usize);
        self.write_body(&mut w);
        w.buf
    }
    /// 反序列化（校验对象头成员数 == [`MemPack::members`]，并拒绝尾部多余字节）。
    fn from_bytes(b: &[u8]) -> Result<Self, StorageError> {
        let mut r = Reader::new(b);
        r.header_expect(Self::members())?;
        let v = Self::read_body(&mut r)?;
        r.finish()?;
        Ok(v)
    }
    /// 成员数（对象头）。
    fn members() -> u32;
}

/// 写一个可选嵌套对象（`None` → null 标记；`Some` → 对象头 + 体）。
fn opt_obj<T: MemPack>(w: &mut Writer, v: Option<&T>) {
    match v {
        None => w.null_ref(),
        Some(x) => {
            w.header(T::members() as usize);
            x.write_body(w);
        }
    }
}

/// 读一个可选嵌套对象。
fn read_opt_obj<T: MemPack>(r: &mut Reader<'_>) -> Result<Option<T>, StorageError> {
    match r.header()? {
        None => Ok(None),
        Some(n) => {
            if n != T::members() {
                return Err(StorageError::Corrupt(format!(
                    "memorypack: nested member count {n} != {}",
                    T::members()
                )));
            }
            Ok(Some(T::read_body(r)?))
        }
    }
}

/// 写 `Option<Vec<Option<T>>>`（数组本身与元素都可空）。
fn opt_obj_array<T: MemPack>(w: &mut Writer, v: Option<&[Option<T>]>) {
    match v {
        None => w.null_len(),
        Some(items) => {
            w.i32(i32::try_from(items.len()).expect("array fits i32"));
            for item in items {
                opt_obj(w, item.as_ref());
            }
        }
    }
}

/// 读 `Option<Vec<Option<T>>>`。
fn read_opt_obj_array<T: MemPack>(
    r: &mut Reader<'_>,
) -> Result<Option<Vec<Option<T>>>, StorageError> {
    let Some(n) = r.count()? else {
        return Ok(None);
    };
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        out.push(read_opt_obj(r)?);
    }
    Ok(Some(out))
}

// ---------------------------------------------------------------------------
// C# 记录类型（`OpenMir2.Data` / `OpenMir2.Packets.*`）
// ---------------------------------------------------------------------------

/// C# `RecordHeader`（`src/OpenMir2/Data/RecordHeader.cs`）。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RecordHeader {
    pub s_account: Option<String>,
    pub name: Option<String>,
    pub select_id: i32,
    pub d_create_date: f64,
    pub deleted: bool,
    pub update_date: f64,
    pub create_date: f64,
}

impl RecordHeader {
    /// C# `SetName`。
    pub fn set_name(&mut self, name: Option<String>) {
        self.name = name;
    }
}

impl MemPack for RecordHeader {
    fn members() -> u32 {
        7
    }
    fn write_body(&self, w: &mut Writer) {
        w.string(self.s_account.as_deref());
        w.string(self.name.as_deref());
        w.i32(self.select_id);
        w.f64(self.d_create_date);
        w.bool(self.deleted);
        w.f64(self.update_date);
        w.f64(self.create_date);
    }
    fn read_body(r: &mut Reader<'_>) -> Result<Self, StorageError> {
        Ok(Self {
            s_account: r.string()?,
            name: r.string()?,
            select_id: r.i32()?,
            d_create_date: r.f64()?,
            deleted: r.bool()?,
            update_date: r.f64()?,
            create_date: r.f64()?,
        })
    }
}

/// C# `Ability`（`Packets/ClientPackets/Ability.cs`，**声明序**）。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Ability {
    pub level: u8,
    pub reserved1: u8,
    pub ac: u16,
    pub mac: u16,
    pub dc: u16,
    pub mc: u16,
    pub sc: u16,
    pub hp: u16,
    pub mp: u16,
    pub max_hp: u16,
    pub max_mp: u16,
    pub reserved2: i16,
    pub exp_count: u8,
    pub exp_max_count: u8,
    pub exp: i32,
    pub max_exp: i32,
    pub weight: u16,
    pub max_weight: u16,
    pub wear_weight: u8,
    pub max_wear_weight: u8,
    pub hand_weight: u8,
    pub max_hand_weight: u8,
}

impl MemPack for Ability {
    fn members() -> u32 {
        22
    }
    fn write_body(&self, w: &mut Writer) {
        w.u8(self.level);
        w.u8(self.reserved1);
        w.u16(self.ac);
        w.u16(self.mac);
        w.u16(self.dc);
        w.u16(self.mc);
        w.u16(self.sc);
        w.u16(self.hp);
        w.u16(self.mp);
        w.u16(self.max_hp);
        w.u16(self.max_mp);
        w.i16(self.reserved2);
        w.u8(self.exp_count);
        w.u8(self.exp_max_count);
        w.i32(self.exp);
        w.i32(self.max_exp);
        w.u16(self.weight);
        w.u16(self.max_weight);
        w.u8(self.wear_weight);
        w.u8(self.max_wear_weight);
        w.u8(self.hand_weight);
        w.u8(self.max_hand_weight);
    }
    fn read_body(r: &mut Reader<'_>) -> Result<Self, StorageError> {
        Ok(Self {
            level: r.u8()?,
            reserved1: r.u8()?,
            ac: r.u16()?,
            mac: r.u16()?,
            dc: r.u16()?,
            mc: r.u16()?,
            sc: r.u16()?,
            hp: r.u16()?,
            mp: r.u16()?,
            max_hp: r.u16()?,
            max_mp: r.u16()?,
            reserved2: r.i16()?,
            exp_count: r.u8()?,
            exp_max_count: r.u8()?,
            exp: r.i32()?,
            max_exp: r.i32()?,
            weight: r.u16()?,
            max_weight: r.u16()?,
            wear_weight: r.u8()?,
            max_wear_weight: r.u8()?,
            hand_weight: r.u8()?,
            max_hand_weight: r.u8()?,
        })
    }
}

impl Ability {
    /// C# `Ability.Clone`（逐字段拷贝）。
    #[must_use]
    pub fn clone_ability(&self) -> Self {
        self.clone()
    }
}

/// C# `NakedAbility`。
#[derive(Debug, Clone, PartialEq, Default)]
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

impl MemPack for NakedAbility {
    fn members() -> u32 {
        10
    }
    fn write_body(&self, w: &mut Writer) {
        w.u16(self.dc);
        w.u16(self.mc);
        w.u16(self.sc);
        w.u16(self.ac);
        w.u16(self.mac);
        w.u16(self.hp);
        w.u16(self.mp);
        w.u8(self.hit);
        w.i32(self.speed);
        w.u8(self.reserved);
    }
    fn read_body(r: &mut Reader<'_>) -> Result<Self, StorageError> {
        Ok(Self {
            dc: r.u16()?,
            mc: r.u16()?,
            sc: r.u16()?,
            ac: r.u16()?,
            mac: r.u16()?,
            hp: r.u16()?,
            mp: r.u16()?,
            hit: r.u8()?,
            speed: r.i32()?,
            reserved: r.u8()?,
        })
    }
}

/// C# `MagicRcd`（`char MagicKey` 是单个 UTF-16 码元）。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MagicRcd {
    pub mag_idx: u16,
    pub level: u8,
    pub magic_key: u16,
    pub tran_point: i32,
}

impl MemPack for MagicRcd {
    fn members() -> u32 {
        4
    }
    fn write_body(&self, w: &mut Writer) {
        w.u16(self.mag_idx);
        w.u8(self.level);
        w.u16(self.magic_key);
        w.i32(self.tran_point);
    }
    fn read_body(r: &mut Reader<'_>) -> Result<Self, StorageError> {
        Ok(Self {
            mag_idx: r.u16()?,
            level: r.u8()?,
            magic_key: r.u16()?,
            tran_point: r.i32()?,
        })
    }
}

/// C# `ServerUserItem`（`Desc` = byte[14]，`Prefix` = char[13]）。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ServerUserItem {
    pub make_index: i32,
    pub index: u16,
    pub dura: u16,
    pub dura_max: u16,
    pub desc: Option<Vec<u8>>,
    pub color_r: u8,
    pub color_g: u8,
    pub color_b: u8,
    pub prefix: Option<Vec<u16>>,
}

impl ServerUserItem {
    /// C# 构造函数默认：`Desc = new byte[14]`、`Prefix = new char[13]`。
    #[must_use]
    pub fn csharp_new() -> Self {
        Self {
            desc: Some(vec![0; 14]),
            prefix: Some(vec![0; 13]),
            ..Self::default()
        }
    }
}

impl MemPack for ServerUserItem {
    fn members() -> u32 {
        9
    }
    fn write_body(&self, w: &mut Writer) {
        w.i32(self.make_index);
        w.u16(self.index);
        w.u16(self.dura);
        w.u16(self.dura_max);
        w.bytes(self.desc.as_deref());
        w.u8(self.color_r);
        w.u8(self.color_g);
        w.u8(self.color_b);
        w.chars(self.prefix.as_deref());
    }
    fn read_body(r: &mut Reader<'_>) -> Result<Self, StorageError> {
        Ok(Self {
            make_index: r.i32()?,
            index: r.u16()?,
            dura: r.u16()?,
            dura_max: r.u16()?,
            desc: r.bytes()?,
            color_r: r.u8()?,
            color_g: r.u8()?,
            color_b: r.u8()?,
            prefix: r.chars()?,
        })
    }
}

/// C# `CharacterData`（存档主体，52 个成员）。
#[derive(Debug, Clone, PartialEq)]
pub struct CharacterData {
    pub server_index: u8,
    pub chr_name: Option<String>,
    pub cur_map: Option<String>,
    pub cur_x: i16,
    pub cur_y: i16,
    pub dir: u8,
    pub hair: u8,
    pub sex: u8,
    pub job: u8,
    pub gold: i32,
    pub abil: Option<Ability>,
    pub status_time_arr: Option<Vec<u16>>,
    pub home_map: Option<String>,
    pub home_x: i16,
    pub home_y: i16,
    pub bonus_abil: Option<NakedAbility>,
    pub bonus_point: i32,
    pub credit_point: u8,
    pub re_level: u8,
    pub master_name: Option<String>,
    pub is_master: bool,
    pub dear_name: Option<String>,
    pub storage_pwd: Option<String>,
    pub game_gold: i32,
    pub game_point: i32,
    pub pay_ment_point: i32,
    pub pk_point: i32,
    pub allow_group: u8,
    pub bt_f9: u8,
    pub attatck_mode: u8,
    pub inc_health: u8,
    pub inc_spell: u8,
    pub inc_healing: u8,
    pub fight_zone_die_count: u8,
    pub bt_ee: u8,
    pub bt_ef: u8,
    pub account: Option<String>,
    pub lock_logon: bool,
    pub contribution: i16,
    pub hunger_status: i32,
    pub allow_guild_re_call: bool,
    pub group_rcall_time: i16,
    pub body_luck: f64,
    pub allow_group_re_call: bool,
    pub quest_unit_open: Option<Vec<u8>>,
    pub quest_unit: Option<Vec<u8>>,
    pub quest_flag: Option<Vec<u8>>,
    pub marry_count: u8,
    pub hum_items: Option<Vec<Option<ServerUserItem>>>,
    pub bag_items: Option<Vec<Option<ServerUserItem>>>,
    pub storage_items: Option<Vec<Option<ServerUserItem>>>,
    pub magic: Option<Vec<Option<MagicRcd>>>,
}

impl Default for CharacterData {
    /// 复刻 C# `CharacterData` 构造函数：`Abil`/`BonusAbil` 非空；三个 128 字节标记数组、
    /// 13/46/50 物品槽、20 技能槽、15 状态槽全部**分配**（元素为 null 引用）。
    fn default() -> Self {
        Self {
            server_index: 0,
            chr_name: None,
            cur_map: None,
            cur_x: 0,
            cur_y: 0,
            dir: 0,
            hair: 0,
            sex: 0,
            job: 0,
            gold: 0,
            abil: Some(Ability::default()),
            status_time_arr: Some(vec![0; 15]),
            home_map: None,
            home_x: 0,
            home_y: 0,
            bonus_abil: Some(NakedAbility::default()),
            bonus_point: 0,
            credit_point: 0,
            re_level: 0,
            master_name: None,
            is_master: false,
            dear_name: None,
            storage_pwd: None,
            game_gold: 0,
            game_point: 0,
            pay_ment_point: 0,
            pk_point: 0,
            allow_group: 0,
            bt_f9: 0,
            attatck_mode: 0,
            inc_health: 0,
            inc_spell: 0,
            inc_healing: 0,
            fight_zone_die_count: 0,
            bt_ee: 0,
            bt_ef: 0,
            account: None,
            lock_logon: false,
            contribution: 0,
            hunger_status: 0,
            allow_guild_re_call: false,
            group_rcall_time: 0,
            body_luck: 0.0,
            allow_group_re_call: false,
            quest_unit_open: Some(vec![0; 128]),
            quest_unit: Some(vec![0; 128]),
            quest_flag: Some(vec![0; 128]),
            marry_count: 0,
            hum_items: Some(vec![None; 13]),
            bag_items: Some(vec![None; 46]),
            storage_items: Some(vec![None; 50]),
            magic: Some(vec![None; 20]),
        }
    }
}

impl MemPack for CharacterData {
    fn members() -> u32 {
        52
    }
    fn write_body(&self, w: &mut Writer) {
        w.u8(self.server_index);
        w.string(self.chr_name.as_deref());
        w.string(self.cur_map.as_deref());
        w.i16(self.cur_x);
        w.i16(self.cur_y);
        w.u8(self.dir);
        w.u8(self.hair);
        w.u8(self.sex);
        w.u8(self.job);
        w.i32(self.gold);
        opt_obj(w, self.abil.as_ref());
        w.u16s(self.status_time_arr.as_deref());
        w.string(self.home_map.as_deref());
        w.i16(self.home_x);
        w.i16(self.home_y);
        opt_obj(w, self.bonus_abil.as_ref());
        w.i32(self.bonus_point);
        w.u8(self.credit_point);
        w.u8(self.re_level);
        w.string(self.master_name.as_deref());
        w.bool(self.is_master);
        w.string(self.dear_name.as_deref());
        w.string(self.storage_pwd.as_deref());
        w.i32(self.game_gold);
        w.i32(self.game_point);
        w.i32(self.pay_ment_point);
        w.i32(self.pk_point);
        w.u8(self.allow_group);
        w.u8(self.bt_f9);
        w.u8(self.attatck_mode);
        w.u8(self.inc_health);
        w.u8(self.inc_spell);
        w.u8(self.inc_healing);
        w.u8(self.fight_zone_die_count);
        w.u8(self.bt_ee);
        w.u8(self.bt_ef);
        w.string(self.account.as_deref());
        w.bool(self.lock_logon);
        w.i16(self.contribution);
        w.i32(self.hunger_status);
        w.bool(self.allow_guild_re_call);
        w.i16(self.group_rcall_time);
        w.f64(self.body_luck);
        w.bool(self.allow_group_re_call);
        w.bytes(self.quest_unit_open.as_deref());
        w.bytes(self.quest_unit.as_deref());
        w.bytes(self.quest_flag.as_deref());
        w.u8(self.marry_count);
        opt_obj_array(w, self.hum_items.as_deref());
        opt_obj_array(w, self.bag_items.as_deref());
        opt_obj_array(w, self.storage_items.as_deref());
        opt_obj_array(w, self.magic.as_deref());
    }
    fn read_body(r: &mut Reader<'_>) -> Result<Self, StorageError> {
        Ok(Self {
            server_index: r.u8()?,
            chr_name: r.string()?,
            cur_map: r.string()?,
            cur_x: r.i16()?,
            cur_y: r.i16()?,
            dir: r.u8()?,
            hair: r.u8()?,
            sex: r.u8()?,
            job: r.u8()?,
            gold: r.i32()?,
            abil: read_opt_obj(r)?,
            status_time_arr: r.u16s()?,
            home_map: r.string()?,
            home_x: r.i16()?,
            home_y: r.i16()?,
            bonus_abil: read_opt_obj(r)?,
            bonus_point: r.i32()?,
            credit_point: r.u8()?,
            re_level: r.u8()?,
            master_name: r.string()?,
            is_master: r.bool()?,
            dear_name: r.string()?,
            storage_pwd: r.string()?,
            game_gold: r.i32()?,
            game_point: r.i32()?,
            pay_ment_point: r.i32()?,
            pk_point: r.i32()?,
            allow_group: r.u8()?,
            bt_f9: r.u8()?,
            attatck_mode: r.u8()?,
            inc_health: r.u8()?,
            inc_spell: r.u8()?,
            inc_healing: r.u8()?,
            fight_zone_die_count: r.u8()?,
            bt_ee: r.u8()?,
            bt_ef: r.u8()?,
            account: r.string()?,
            lock_logon: r.bool()?,
            contribution: r.i16()?,
            hunger_status: r.i32()?,
            allow_guild_re_call: r.bool()?,
            group_rcall_time: r.i16()?,
            body_luck: r.f64()?,
            allow_group_re_call: r.bool()?,
            quest_unit_open: r.bytes()?,
            quest_unit: r.bytes()?,
            quest_flag: r.bytes()?,
            marry_count: r.u8()?,
            hum_items: read_opt_obj_array(r)?,
            bag_items: read_opt_obj_array(r)?,
            storage_items: read_opt_obj_array(r)?,
            magic: read_opt_obj_array(r)?,
        })
    }
}

/// C# `CharacterDataInfo`（`Header` + `Data`）。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CharacterDataInfo {
    pub header: RecordHeader,
    pub data: Option<CharacterData>,
}

impl CharacterDataInfo {
    /// C# 构造函数：`Data = new CharacterData()`（非空）。
    #[must_use]
    pub fn csharp_new() -> Self {
        Self {
            header: RecordHeader::default(),
            data: Some(CharacterData::default()),
        }
    }
}

impl MemPack for CharacterDataInfo {
    fn members() -> u32 {
        2
    }
    fn write_body(&self, w: &mut Writer) {
        w.header(RecordHeader::members() as usize);
        self.header.write_body(w);
        opt_obj(w, self.data.as_ref());
    }
    fn read_body(r: &mut Reader<'_>) -> Result<Self, StorageError> {
        // Header 是**值结构**（非空），直接读体（对象头紧跟在外层对象头之后）。
        r.header_expect(RecordHeader::members())?;
        let header = RecordHeader::read_body(r)?;
        let data = read_opt_obj(r)?;
        Ok(Self { header, data })
    }
}

/// C# `PlayerRecordData`（`Storeages/DBSrv.Storage/Model/HumRecordData.cs`）：
/// `characters_indexes` 的一行（角色索引表条目）。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PlayerRecordData {
    pub id: i32,
    pub header: RecordHeader,
    pub chr_name: Option<String>,
    pub account: Option<String>,
    pub deleted: bool,
    pub selected: u8,
}

impl MemPack for PlayerRecordData {
    fn members() -> u32 {
        6
    }
    fn write_body(&self, w: &mut Writer) {
        w.i32(self.id);
        w.header(RecordHeader::members() as usize);
        self.header.write_body(w);
        w.string(self.chr_name.as_deref());
        w.string(self.account.as_deref());
        w.bool(self.deleted);
        w.u8(self.selected);
    }
    fn read_body(r: &mut Reader<'_>) -> Result<Self, StorageError> {
        Ok(Self {
            id: r.i32()?,
            header: {
                r.header_expect(RecordHeader::members())?;
                RecordHeader::read_body(r)?
            },
            chr_name: r.string()?,
            account: r.string()?,
            deleted: r.bool()?,
            selected: r.u8()?,
        })
    }
}

// ---------------------------------------------------------------------------
// DBSrv 6000 消息（`OpenMir2.Packets.ServerPackets`）
// ---------------------------------------------------------------------------

/// C# `LoadCharacterData`（struct）：GameSvr 请求取档。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct LoadCharacterData {
    pub account: Option<String>,
    pub chr_name: Option<String>,
    pub user_addr: Option<String>,
    pub session_id: i32,
}

impl MemPack for LoadCharacterData {
    fn members() -> u32 {
        4
    }
    fn write_body(&self, w: &mut Writer) {
        w.string(self.account.as_deref());
        w.string(self.chr_name.as_deref());
        w.string(self.user_addr.as_deref());
        w.i32(self.session_id);
    }
    fn read_body(r: &mut Reader<'_>) -> Result<Self, StorageError> {
        Ok(Self {
            account: r.string()?,
            chr_name: r.string()?,
            user_addr: r.string()?,
            session_id: r.i32()?,
        })
    }
}

/// C# `SaveCharacterData`：GameSvr 存档。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SaveCharacterData {
    pub account: Option<String>,
    pub chr_name: Option<String>,
    pub character_data: Option<CharacterDataInfo>,
}

impl MemPack for SaveCharacterData {
    fn members() -> u32 {
        3
    }
    fn write_body(&self, w: &mut Writer) {
        w.string(self.account.as_deref());
        w.string(self.chr_name.as_deref());
        opt_obj(w, self.character_data.as_ref());
    }
    fn read_body(r: &mut Reader<'_>) -> Result<Self, StorageError> {
        Ok(Self {
            account: r.string()?,
            chr_name: r.string()?,
            character_data: read_opt_obj(r)?,
        })
    }
}

/// C# `LoadPlayerDataPacket`：DBSrv 回给 GameSvr 的取档应答体。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct LoadPlayerDataPacket {
    /// C# 侧这里存的是 `EDCode.EncodeString(chrName)` 的**编码后 ASCII 串**。
    pub chr_name: Option<String>,
    pub hum_data_info: Option<CharacterDataInfo>,
}

impl MemPack for LoadPlayerDataPacket {
    fn members() -> u32 {
        2
    }
    fn write_body(&self, w: &mut Writer) {
        w.string(self.chr_name.as_deref());
        opt_obj(w, self.hum_data_info.as_ref());
    }
    fn read_body(r: &mut Reader<'_>) -> Result<Self, StorageError> {
        Ok(Self {
            chr_name: r.string()?,
            hum_data_info: read_opt_obj(r)?,
        })
    }
}

/// C# `ServerRequestMessage`：DBSrv 内部 12 字节"消息头"（经 EDCode 包裹后进 `ServerRequestData`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ServerRequestMessage {
    pub recog: i32,
    pub ident: u16,
    pub param: u16,
    pub tag: u16,
    pub series: u16,
}

impl ServerRequestMessage {
    /// C# `new ServerRequestMessage(ident, recog, param, tag, series)`。
    #[must_use]
    pub fn new(ident: u16, recog: i32, param: u16, tag: u16, series: u16) -> Self {
        Self {
            recog,
            ident,
            param,
            tag,
            series,
        }
    }
}

impl MemPack for ServerRequestMessage {
    fn members() -> u32 {
        5
    }
    fn write_body(&self, w: &mut Writer) {
        w.i32(self.recog);
        w.u16(self.ident);
        w.u16(self.param);
        w.u16(self.tag);
        w.u16(self.series);
    }
    fn read_body(r: &mut Reader<'_>) -> Result<Self, StorageError> {
        Ok(Self {
            recog: r.i32()?,
            ident: r.u16()?,
            param: r.u16()?,
            tag: r.u16()?,
            series: r.u16()?,
        })
    }
}

/// C# `ServerRequestData`：6000 面的外层信封（2200/2201 请求与应答共用）。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ServerRequestData {
    pub query_id: i32,
    /// EDCode 编码后的 `ServerRequestMessage` MemoryPack 字节（null 允许）。
    pub message: Option<Vec<u8>>,
    /// EDCode 编码后的业务体 MemoryPack 字节（null 允许）。
    pub packet: Option<Vec<u8>>,
    /// EDCode 编码后的 4 字节校验码（null 允许）。
    pub sign: Option<Vec<u8>>,
}

impl MemPack for ServerRequestData {
    fn members() -> u32 {
        4
    }
    fn write_body(&self, w: &mut Writer) {
        w.i32(self.query_id);
        w.bytes(self.message.as_deref());
        w.bytes(self.packet.as_deref());
        w.bytes(self.sign.as_deref());
    }
    fn read_body(r: &mut Reader<'_>) -> Result<Self, StorageError> {
        Ok(Self {
            query_id: r.i32()?,
            message: r.bytes()?,
            packet: r.bytes()?,
            sign: r.bytes()?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(b: &[u8]) -> String {
        use std::fmt::Write as _;
        let mut s = String::with_capacity(b.len() * 2);
        for x in b {
            let _ = write!(s, "{x:02x}");
        }
        s
    }

    fn unhex(s: &str) -> Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }

    /// 金标准字面量（C# 真身生成的向量，`kind:"playerdatainfo"`）：
    /// 小结构逐个钉住对象头/声明序/null 编码。
    #[test]
    fn small_structs_match_csharp_golden() {
        let ab = Ability {
            level: 3,
            reserved1: 0,
            ac: 1,
            mac: 2,
            dc: 3,
            mc: 4,
            sc: 5,
            hp: 6,
            mp: 7,
            max_hp: 8,
            max_mp: 9,
            reserved2: -2,
            exp_count: 1,
            exp_max_count: 2,
            exp: 100,
            max_exp: 200,
            weight: 10,
            max_weight: 11,
            wear_weight: 12,
            max_wear_weight: 13,
            hand_weight: 14,
            max_hand_weight: 15,
        };
        assert_eq!(
            hex(&ab.to_bytes()),
            "160300010002000300040005000600070008000900feff010264000000c80000000a000b000c0d0e0f"
        );
        assert_eq!(Ability::from_bytes(&ab.to_bytes()).unwrap(), ab);

        let na = NakedAbility {
            dc: 1,
            mc: 2,
            sc: 3,
            ac: 4,
            mac: 5,
            hp: 6,
            mp: 7,
            hit: 8,
            speed: 9,
            reserved: 10,
        };
        assert_eq!(
            hex(&na.to_bytes()),
            "0a010002000300040005000600070008090000000a"
        );

        let m = MagicRcd {
            mag_idx: 0x1234,
            level: 5,
            magic_key: u16::from(b'x'),
            tran_point: 999,
        };
        assert_eq!(hex(&m.to_bytes()), "043412057800e7030000");

        let it = ServerUserItem {
            make_index: 0x0102_0304,
            index: 0x0506,
            dura: 0x0708,
            dura_max: 0x090A,
            desc: {
                let mut d = vec![0u8; 14];
                d[0] = 0xAA;
                d[13] = 0xBB;
                Some(d)
            },
            color_r: 1,
            color_g: 2,
            color_b: 3,
            prefix: {
                let mut p = vec![0u16; 13];
                p[0] = u16::from(b'p');
                p[12] = u16::from(b'q');
                Some(p)
            },
        };
        assert_eq!(
            hex(&it.to_bytes()),
            "0904030201060508070a090e000000aa000000000000000000000000bb010203\
             0d0000007000000000000000000000000000000000000000000000007100"
        );
        assert_eq!(ServerUserItem::from_bytes(&it.to_bytes()).unwrap(), it);

        let rh = RecordHeader {
            s_account: Some("acct".into()),
            name: Some("aaa".into()),
            select_id: 7,
            d_create_date: 1.5,
            deleted: false,
            update_date: 2.5,
            create_date: 3.5,
        };
        assert_eq!(
            hex(&rh.to_bytes()),
            "070400000061006300630074000300000061006100610007000000000000000000f83f\
             0000000000000004400000000000000c40"
        );
    }

    /// 信封与请求头的小结构。
    #[test]
    fn envelopes_match_csharp_golden() {
        let lcd = LoadCharacterData {
            account: Some("acct".into()),
            chr_name: Some("aaa".into()),
            user_addr: Some("127.0.0.1".into()),
            session_id: 42,
        };
        assert_eq!(
            hex(&lcd.to_bytes()),
            "0404000000610063006300740003000000610061006100090000003100320037002e0030002e\
             0030002e0031002a000000"
        );

        let srd = ServerRequestData {
            query_id: 5,
            message: Some(vec![1, 2, 3]),
            packet: Some(vec![4, 5]),
            sign: Some(vec![6]),
        };
        assert_eq!(
            hex(&srd.to_bytes()),
            "0405000000030000000102030200000004050100000006"
        );

        let srm = ServerRequestMessage::new(0x1234, 5, 6, 7, 8);
        assert_eq!(hex(&srm.to_bytes()), "05050000003412060007000800");
    }

    /// 破坏者自检：改一个字段，字节必须变（防"自洽往返对格式改动盲"）。
    #[test]
    fn mutation_changes_bytes() {
        let a = ServerRequestMessage::new(0x1234, 5, 6, 7, 8);
        let mut b = a;
        b.tag = 9;
        assert_ne!(a.to_bytes(), b.to_bytes());
        assert_eq!(
            ServerRequestMessage::from_bytes(&unhex("05050000003412060007000800")).unwrap(),
            a
        );
    }
}
