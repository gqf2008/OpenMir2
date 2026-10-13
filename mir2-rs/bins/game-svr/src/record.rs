//! 角色记录（`LoadPlayerDataPacket{ChrName, HumDataInfo{Header, Data}}`）的**全字段解析与重编码**。
//!
//! - [`parse_player_data_head`]：S2b-3 的头部窗口（`ChrName`/`CurMap`/`CurX`/`CurY`/`Dir`），进世界路径在用。
//! - [`parse_player_data`] / [`encode_player_data`]：`CharacterData` 全部 52 个成员（属性/点数/物品 13+46+50/
//!   技能 20/任务数组 128）的双向读写。
//!
//! 布局**实测**（部署件 `OpenMir2.dll` 序列化逐字段可区分的样例；基准入库到
//! `tests/fixtures/player_data_full_oracle.txt`，生成器 `E:\tmp\openmir2-recf\dbgen\`）：
//! ```text
//! class / 含 string 的 struct   先写 1 字节成员数；null 引用对象 = 单字节 0xFF
//! 集合 null                     i32(-1)；元素 null 同样写 0xFF
//! string                        i32(UTF-16 码元数) + UTF-16LE
//! byte[] / char[]               i32(元素数) + 原始字节 / 每元素 2 字节
//! 标量                          小端；bool = 1 字节；char = 1 个 UTF-16 码元 (u16)
//! ```
//! **注意**：C# `CharacterData` 末尾的 `[OnSerializing] Serialized()`（`System.Runtime.Serialization`）
//! **不会被 MemoryPack 调用**——oracle 证明把数组置 null 后序列化，长度前缀就是 i32(-1)
//! （`nulls` 样例），而不是被兜底成定长数组。本实现照抄实测行为（null 集合 = -1、null 元素 = 0xFF），
//! 不实现那套兜底；对照见 `tests/fixtures/player_data_full_oracle.txt` 的说明与 `null_arrays_*` 单测。
//!
//! 逐组推进的 oracle 基准（stage1..stage6）与边界样例（nulls/nullitems/abilnull/nullitem）都在同一个 fixture 里。

// ===================== MemoryPack 通用读写 =====================

/// 解析失败（字节耗尽、成员数/长度与 oracle 不符）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ParseError;

type PResult<T> = Result<T, ParseError>;

/// null 引用对象/数组元素在 MemoryPack 里的单字节标记。
const NULL_OBJ: u8 = 0xFF;

struct Reader<'a> {
    b: &'a [u8],
    i: usize,
}

impl<'a> Reader<'a> {
    fn new(b: &'a [u8]) -> Self {
        Self { b, i: 0 }
    }

    fn take(&mut self, n: usize) -> PResult<&'a [u8]> {
        let end = self.i.checked_add(n).ok_or(ParseError)?;
        let s = self.b.get(self.i..end).ok_or(ParseError)?;
        self.i = end;
        Ok(s)
    }

    fn u8(&mut self) -> PResult<u8> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> PResult<u16> {
        Ok(u16::from_le_bytes(
            self.take(2)?.try_into().map_err(|_| ParseError)?,
        ))
    }

    fn i16(&mut self) -> PResult<i16> {
        Ok(i16::from_le_bytes(
            self.take(2)?.try_into().map_err(|_| ParseError)?,
        ))
    }

    fn i32(&mut self) -> PResult<i32> {
        Ok(i32::from_le_bytes(
            self.take(4)?.try_into().map_err(|_| ParseError)?,
        ))
    }

    fn f64(&mut self) -> PResult<f64> {
        Ok(f64::from_le_bytes(
            self.take(8)?.try_into().map_err(|_| ParseError)?,
        ))
    }

    fn boolean(&mut self) -> PResult<bool> {
        Ok(self.u8()? != 0)
    }

    /// MemoryPack 的 `char`：1 个 UTF-16 码元。用 `u16` 存以保证逐字节可逆（代理区也保真）。
    fn utf16_char(&mut self) -> PResult<u16> {
        self.u16()
    }

    /// MemoryPack 的 `string`：`None` = null（长度 -1），`Some("")` = 空串。
    fn string(&mut self) -> PResult<Option<String>> {
        let n = self.i32()?;
        if n < 0 {
            return Ok(None);
        }
        let n = usize::try_from(n).map_err(|_| ParseError)?;
        let bytes = self.take(n.checked_mul(2).ok_or(ParseError)?)?;
        let mut units: Vec<u16> = Vec::with_capacity(n);
        for c in bytes.as_chunks::<2>().0 {
            units.push(u16::from_le_bytes(*c));
        }
        Ok(Some(String::from_utf16_lossy(&units)))
    }

    /// 集合长度前缀：`None` = null（-1）。
    fn length(&mut self) -> PResult<Option<usize>> {
        let n = self.i32()?;
        if n < 0 {
            return Ok(None);
        }
        Ok(Some(usize::try_from(n).map_err(|_| ParseError)?))
    }

    /// `byte[]`。
    fn byte_array(&mut self) -> PResult<Option<Vec<u8>>> {
        match self.length()? {
            None => Ok(None),
            Some(n) => Ok(Some(self.take(n)?.to_vec())),
        }
    }
}

/// 读对象头：`0xFF` ⇒ null（`Ok(false)`）；否则读成员数并要求等于 `expected`（`Ok(true)`）。
fn obj_header(r: &mut Reader, expected: u8) -> PResult<bool> {
    if r.b.get(r.i).copied() == Some(NULL_OBJ) {
        r.i += 1;
        return Ok(false);
    }
    if r.u8()? != expected {
        return Err(ParseError);
    }
    Ok(true)
}

/// 要求成员数等于 `expected`（用于不可为 null 的 struct / 根对象）。
fn expect_count(r: &mut Reader, expected: u8) -> PResult<()> {
    if r.u8()? != expected {
        return Err(ParseError);
    }
    Ok(())
}

#[derive(Default)]
struct Writer {
    out: Vec<u8>,
}

impl Writer {
    fn u8(&mut self, v: u8) {
        self.out.push(v);
    }

    fn u16(&mut self, v: u16) {
        self.out.extend_from_slice(&v.to_le_bytes());
    }

    fn i16(&mut self, v: i16) {
        self.out.extend_from_slice(&v.to_le_bytes());
    }

    fn i32(&mut self, v: i32) {
        self.out.extend_from_slice(&v.to_le_bytes());
    }

    fn f64(&mut self, v: f64) {
        self.out.extend_from_slice(&v.to_le_bytes());
    }

    fn boolean(&mut self, v: bool) {
        self.u8(u8::from(v));
    }

    fn utf16_char(&mut self, v: u16) {
        self.u16(v);
    }

    fn string(&mut self, v: Option<&str>) {
        match v {
            None => self.i32(-1),
            Some(s) => {
                let units: Vec<u16> = s.encode_utf16().collect();
                self.i32(i32::try_from(units.len()).unwrap_or(-1));
                for u in units {
                    self.u16(u);
                }
            }
        }
    }

    fn byte_array(&mut self, v: Option<&[u8]>) {
        match v {
            None => self.i32(-1),
            Some(b) => {
                self.i32(i32::try_from(b.len()).unwrap_or(-1));
                self.out.extend_from_slice(b);
            }
        }
    }
}

// ===================== 嵌套类型 =====================

/// `RecordHeader`（struct：固定 7 个成员，无 null 标记）。
#[derive(Debug, Clone, PartialEq)]
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
    fn decode(r: &mut Reader) -> PResult<Self> {
        expect_count(r, 7)?;
        Ok(Self {
            s_account: r.string()?,
            name: r.string()?,
            select_id: r.i32()?,
            d_create_date: r.f64()?,
            deleted: r.boolean()?,
            update_date: r.f64()?,
            create_date: r.f64()?,
        })
    }

    fn encode(&self, w: &mut Writer) {
        w.u8(7);
        w.string(self.s_account.as_deref());
        w.string(self.name.as_deref());
        w.i32(self.select_id);
        w.f64(self.d_create_date);
        w.boolean(self.deleted);
        w.f64(self.update_date);
        w.f64(self.create_date);
    }
}

// `Ability` / `NakedAbility` 的定义在 lib 侧（`game_svr::ability`）：`ability.rs` 要产出/消费它们，
// 而本模块是 bin 模块（主 bin 通过 `game_svr::` 访问 lib）。线缆编解码仍留在本文件，因为它依赖这里的
// `Reader`/`Writer`（lib 侧不重复造一遍）。`pub use` 保证既有 `crate::record::{Ability, NakedAbility}` 路径不变。
pub use game_svr::ability::{Ability, NakedAbility};

fn decode_ability(r: &mut Reader) -> PResult<Option<Ability>> {
    if !obj_header(r, 22)? {
        return Ok(None);
    }
    Ok(Some(Ability {
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
    }))
}

fn encode_ability_into(abil: &Ability, w: &mut Writer) {
    w.u8(22);
    w.u8(abil.level);
    w.u8(abil.reserved1);
    w.u16(abil.ac);
    w.u16(abil.mac);
    w.u16(abil.dc);
    w.u16(abil.mc);
    w.u16(abil.sc);
    w.u16(abil.hp);
    w.u16(abil.mp);
    w.u16(abil.max_hp);
    w.u16(abil.max_mp);
    w.i16(abil.reserved2);
    w.u8(abil.exp_count);
    w.u8(abil.exp_max_count);
    w.i32(abil.exp);
    w.i32(abil.max_exp);
    w.u16(abil.weight);
    w.u16(abil.max_weight);
    w.u8(abil.wear_weight);
    w.u8(abil.max_wear_weight);
    w.u8(abil.hand_weight);
    w.u8(abil.max_hand_weight);
}

/// `Ability`（= oracle `WAbil`）的 **`EDCode.EncodeMessage` 体**：MemoryPack 序列化后**丢掉成员数首字节**。
///
/// 口径照 `src/OpenMir2/EDcode.cs:211-229`（`EncodeMessage<T>(T packet) where T : class`）：
/// `byte[] data = SerializerUtil.Serialize(packet); byte[] newData = data[1..];` 然后从 `pos: 1` 开始编码
/// ⇒ 线上尾 = `EDCode.Encode(序列化原文[1..])`。本函数返回的就是 `序列化原文[1..]`（40B），
/// 由 `smframe::sm_ability` 再 `edcode::encode` 追加。
///
/// **金标准锚定**：#12（`SM_ABILITY`）线缆 54B ⇒ `edcode::decode` 得 40B，正是本函数输出
/// （测试 `encode_ability_matches_golden_frame_12_tail` 用真字节钉住，不按公式重推）。
///
/// **接线点**：`openmir2-m2-enter-world-frames` README §5.1 登记的 52/752 输入阻塞（「`record.rs` 只有私有
/// `encode_opt_ability`」）——本函数补齐该公开接缝；生产路径尚未消费（52 的触发点在 `RM_ABILITY`/
/// `RM_LEVELUP`，待 S3 世界事件接线），故 `allow(dead_code)`（与 `parse_player_data` 同）。
#[must_use]
#[allow(dead_code)]
pub fn encode_ability(abil: &Ability) -> Vec<u8> {
    let mut w = Writer::default();
    encode_ability_into(abil, &mut w);
    // 丢首字节（`Ability` 的成员数 22）——`EncodeMessage` 的口径
    w.out[1..].to_vec()
}

/// 帧 52 尾的**冻结客户端布局**（备选；默认不启用，见 `MIR2_CLIENT_ABILITY_LAYOUT`）。
///
/// 真机实测（2026-10-13，`D:\MirClient-run\MirClinet.exe`）：`MirClient` 的 `TAbility` 在
/// `HIGHHP = True`（`Source/Common/Grobal2.pas:39`）下是 **58B** 记录，字段为
/// `Level: Word`、`AC/MAC/DC/MC/SC: Integer`(4B)、`HP/MP/MaxHP/MaxMP: DWORD`(4B)、
/// `Exp/MaxExp: DWORD`(4B)、`Weight/MaxWeight/WearWeight/MaxWearWeight/HandWeight/MaxHandWeight: Word`
/// （`:2167-2193`）；`SM_ABILITY` 处理器按 `SizeOf(TAbility)` 整块拷（`ClMain.pas:10102`），HUD 取
/// `m_Abil.HP/MaxHP`（`FState.pas:8887`），`CanRun` 要求 `HP >= RUN_MINHEALTH(10)`（`Actor.pas:2165`）。
///
/// oracle 的 40B `EncodeMessage(WAbil)`（[`encode_ability`]）与该记录**布局不匹配**：客户端把
/// offset 22 读成 `HP`(DWORD) = oracle 的 `Exp`（本部署 = 0）⇒ HUD `0/0`、`CanRun` 拒绝跑。
/// C# 对照组同病（真机逐像素一致，见
/// `tests/parity/world/client-e2e-enter-gaps-20261013/README.md`）。
///
/// **值取自同一 `WAbil`，只有布局不同**：`Level@0`(Word)、`AC@2/MAC@6/DC@10/MC@14/SC@18`(Integer)、
/// `HP@22/MP@26/MaxHP@30/MaxMP@34`(DWORD)、`Exp@38/MaxExp@42`(DWORD)、
/// `Weight@46/MaxWeight@48/WearWeight@50/MaxWearWeight@52/HandWeight@54/MaxHandWeight@56`(Word)。
///
/// **本函数不改默认行为**：生产路径只有在 `MIR2_CLIENT_ABILITY_LAYOUT` 打开时才用它；默认仍走
/// [`encode_ability`]（与 oracle 逐字节相同）。要走它属 §6.0-3「与 oracle 的故意偏离」，
/// 需 owner 裁决 + 白名单登记。
#[must_use]
#[allow(dead_code)]
pub fn encode_ability_client(abil: &Ability) -> Vec<u8> {
    let mut w = Writer::default();
    // `Level: Word`（oracle 用 `level + reserved1` 两字节占同一槽；`reserved1` 恒 0）。
    w.u16(u16::from(abil.level));
    // `AC/MAC/DC/MC/SC: Integer`（零扩展）。
    w.i32(i32::from(abil.ac));
    w.i32(i32::from(abil.mac));
    w.i32(i32::from(abil.dc));
    w.i32(i32::from(abil.mc));
    w.i32(i32::from(abil.sc));
    // `HP/MP/MaxHP/MaxMP: DWORD`。
    w.i32(i32::from(abil.hp));
    w.i32(i32::from(abil.mp));
    w.i32(i32::from(abil.max_hp));
    w.i32(i32::from(abil.max_mp));
    // `Exp/MaxExp: DWORD`。
    w.i32(abil.exp);
    w.i32(abil.max_exp);
    // `Weight/MaxWeight/WearWeight/MaxWearWeight/HandWeight/MaxHandWeight: Word`。
    w.u16(abil.weight);
    w.u16(abil.max_weight);
    w.u16(u16::from(abil.wear_weight));
    w.u16(u16::from(abil.max_wear_weight));
    w.u16(u16::from(abil.hand_weight));
    w.u16(u16::from(abil.max_hand_weight));
    w.out
}

fn decode_naked(r: &mut Reader) -> PResult<Option<NakedAbility>> {
    if !obj_header(r, 10)? {
        return Ok(None);
    }
    Ok(Some(NakedAbility {
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
    }))
}

fn encode_naked(n: &NakedAbility, w: &mut Writer) {
    w.u8(10);
    w.u16(n.dc);
    w.u16(n.mc);
    w.u16(n.sc);
    w.u16(n.ac);
    w.u16(n.mac);
    w.u16(n.hp);
    w.u16(n.mp);
    w.u8(n.hit);
    w.i32(n.speed);
    w.u8(n.reserved);
}

/// `ServerUserItem`（class：9 个成员）。`double`/`Prefix` 可为 null。
#[derive(Debug, Clone, PartialEq)]
pub struct ServerUserItem {
    pub make_index: i32,
    pub index: u16,
    pub dura: u16,
    pub dura_max: u16,
    pub desc: Option<Vec<u8>>,
    pub color_r: u8,
    pub color_g: u8,
    pub color_b: u8,
    /// C# `char[] Prefix`：每元素 1 个 UTF-16 码元。
    pub prefix: Option<Vec<u16>>,
}

impl ServerUserItem {
    fn decode(r: &mut Reader) -> PResult<Option<Self>> {
        if !obj_header(r, 9)? {
            return Ok(None);
        }
        let make_index = r.i32()?;
        let index = r.u16()?;
        let dura = r.u16()?;
        let dura_max = r.u16()?;
        let desc = r.byte_array()?;
        let color_r = r.u8()?;
        let color_g = r.u8()?;
        let color_b = r.u8()?;
        let prefix = match r.length()? {
            None => None,
            Some(n) => {
                let mut v = Vec::with_capacity(n);
                for _ in 0..n {
                    v.push(r.utf16_char()?);
                }
                Some(v)
            }
        };
        Ok(Some(Self {
            make_index,
            index,
            dura,
            dura_max,
            desc,
            color_r,
            color_g,
            color_b,
            prefix,
        }))
    }

    fn encode(&self, w: &mut Writer) {
        w.u8(9);
        w.i32(self.make_index);
        w.u16(self.index);
        w.u16(self.dura);
        w.u16(self.dura_max);
        w.byte_array(self.desc.as_deref());
        w.u8(self.color_r);
        w.u8(self.color_g);
        w.u8(self.color_b);
        match &self.prefix {
            None => w.i32(-1),
            Some(p) => {
                w.i32(i32::try_from(p.len()).unwrap_or(-1));
                for u in p {
                    w.utf16_char(*u);
                }
            }
        }
    }
}

/// `MagicRcd`（class：4 个成员）。
#[derive(Debug, Clone, PartialEq)]
pub struct MagicRcd {
    pub mag_idx: u16,
    pub level: u8,
    /// C# `char MagicKey`：1 个 UTF-16 码元。
    pub magic_key: u16,
    pub tran_point: i32,
}

impl MagicRcd {
    fn decode(r: &mut Reader) -> PResult<Option<Self>> {
        if !obj_header(r, 4)? {
            return Ok(None);
        }
        Ok(Some(Self {
            mag_idx: r.u16()?,
            level: r.u8()?,
            magic_key: r.utf16_char()?,
            tran_point: r.i32()?,
        }))
    }

    fn encode(&self, w: &mut Writer) {
        w.u8(4);
        w.u16(self.mag_idx);
        w.u8(self.level);
        w.utf16_char(self.magic_key);
        w.i32(self.tran_point);
    }
}

// ===================== CharacterData（52 个成员） =====================

/// `CharacterData`（class：52 个成员，顺序 = `PlayerDataInfo.cs` 声明顺序）。
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

fn decode_items(r: &mut Reader) -> PResult<Option<Vec<Option<ServerUserItem>>>> {
    match r.length()? {
        None => Ok(None),
        Some(n) => {
            let mut v = Vec::with_capacity(n);
            for _ in 0..n {
                v.push(ServerUserItem::decode(r)?);
            }
            Ok(Some(v))
        }
    }
}

fn decode_magics(r: &mut Reader) -> PResult<Option<Vec<Option<MagicRcd>>>> {
    match r.length()? {
        None => Ok(None),
        Some(n) => {
            let mut v = Vec::with_capacity(n);
            for _ in 0..n {
                v.push(MagicRcd::decode(r)?);
            }
            Ok(Some(v))
        }
    }
}

fn decode_u16_vec(r: &mut Reader) -> PResult<Option<Vec<u16>>> {
    match r.length()? {
        None => Ok(None),
        Some(n) => {
            let mut v = Vec::with_capacity(n);
            for _ in 0..n {
                v.push(r.u16()?);
            }
            Ok(Some(v))
        }
    }
}

fn decode_u8_vec(r: &mut Reader) -> PResult<Option<Vec<u8>>> {
    r.byte_array()
}

impl CharacterData {
    fn decode(r: &mut Reader) -> PResult<Self> {
        expect_count(r, 52)?;
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
            abil: decode_ability(r)?,
            status_time_arr: decode_u16_vec(r)?,
            home_map: r.string()?,
            home_x: r.i16()?,
            home_y: r.i16()?,
            bonus_abil: decode_naked(r)?,
            bonus_point: r.i32()?,
            credit_point: r.u8()?,
            re_level: r.u8()?,
            master_name: r.string()?,
            is_master: r.boolean()?,
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
            lock_logon: r.boolean()?,
            contribution: r.i16()?,
            hunger_status: r.i32()?,
            allow_guild_re_call: r.boolean()?,
            group_rcall_time: r.i16()?,
            body_luck: r.f64()?,
            allow_group_re_call: r.boolean()?,
            quest_unit_open: decode_u8_vec(r)?,
            quest_unit: decode_u8_vec(r)?,
            quest_flag: decode_u8_vec(r)?,
            marry_count: r.u8()?,
            hum_items: decode_items(r)?,
            bag_items: decode_items(r)?,
            storage_items: decode_items(r)?,
            magic: decode_magics(r)?,
        })
    }

    fn encode(&self, w: &mut Writer) {
        w.u8(52);
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
        encode_opt_ability(w, self.abil.as_ref());
        encode_u16_vec(w, self.status_time_arr.as_deref());
        w.string(self.home_map.as_deref());
        w.i16(self.home_x);
        w.i16(self.home_y);
        encode_opt_naked(w, self.bonus_abil.as_ref());
        w.i32(self.bonus_point);
        w.u8(self.credit_point);
        w.u8(self.re_level);
        w.string(self.master_name.as_deref());
        w.boolean(self.is_master);
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
        w.boolean(self.lock_logon);
        w.i16(self.contribution);
        w.i32(self.hunger_status);
        w.boolean(self.allow_guild_re_call);
        w.i16(self.group_rcall_time);
        w.f64(self.body_luck);
        w.boolean(self.allow_group_re_call);
        w.byte_array(self.quest_unit_open.as_deref());
        w.byte_array(self.quest_unit.as_deref());
        w.byte_array(self.quest_flag.as_deref());
        w.u8(self.marry_count);
        encode_items(w, self.hum_items.as_deref());
        encode_items(w, self.bag_items.as_deref());
        encode_items(w, self.storage_items.as_deref());
        encode_magics(w, self.magic.as_deref());
    }
}

fn encode_opt_ability(w: &mut Writer, v: Option<&Ability>) {
    match v {
        None => w.u8(NULL_OBJ),
        Some(a) => encode_ability_into(a, w),
    }
}

fn encode_opt_naked(w: &mut Writer, v: Option<&NakedAbility>) {
    match v {
        None => w.u8(NULL_OBJ),
        Some(n) => encode_naked(n, w),
    }
}

fn encode_u16_vec(w: &mut Writer, v: Option<&[u16]>) {
    match v {
        None => w.i32(-1),
        Some(a) => {
            w.i32(i32::try_from(a.len()).unwrap_or(-1));
            for x in a {
                w.u16(*x);
            }
        }
    }
}

fn encode_items(w: &mut Writer, v: Option<&[Option<ServerUserItem>]>) {
    match v {
        None => w.i32(-1),
        Some(a) => {
            w.i32(i32::try_from(a.len()).unwrap_or(-1));
            for it in a {
                match it {
                    None => w.u8(NULL_OBJ),
                    Some(item) => item.encode(w),
                }
            }
        }
    }
}

fn encode_magics(w: &mut Writer, v: Option<&[Option<MagicRcd>]>) {
    match v {
        None => w.i32(-1),
        Some(a) => {
            w.i32(i32::try_from(a.len()).unwrap_or(-1));
            for m in a {
                match m {
                    None => w.u8(NULL_OBJ),
                    Some(magic) => magic.encode(w),
                }
            }
        }
    }
}

// ===================== 顶层：LoadPlayerDataPacket =====================

/// 角色记录的头部窗口（`PlayerDataHead` 的完整版载体）。
#[derive(Debug, Clone, PartialEq)]
pub struct PlayerDataHead {
    pub chr_name: String,
    pub cur_map: String,
    pub cur_x: i16,
    pub cur_y: i16,
    pub dir: u8,
    /// `Header.sAccount`（账号）——世界侧暂不需要，留着日志用。
    pub account: String,
}

/// 全字段角色记录：`LoadPlayerDataPacket{ChrName, HumDataInfo{Header, Data}}`。
#[derive(Debug, Clone, PartialEq)]
pub struct PlayerRecord {
    pub chr_name: Option<String>,
    pub header: RecordHeader,
    pub data: CharacterData,
}

fn parse_head_inner(bytes: &[u8]) -> PResult<PlayerDataHead> {
    let mut r = Reader::new(bytes);
    expect_count(&mut r, 2)?; // LoadPlayerDataPacket 成员数
    let packet_chr_name = r.string()?;
    expect_count(&mut r, 2)?; // CharacterDataInfo 成员数
    expect_count(&mut r, 7)?; // RecordHeader 成员数
    let account = r.string()?.unwrap_or_default(); // sAccount
    let header_name = r.string()?.unwrap_or_default(); // Name
    r.i32()?; // SelectID
    r.f64()?; // dCreateDate
    r.u8()?; // Deleted
    r.f64()?; // UpdateDate
    r.f64()?; // CreateDate
    expect_count(&mut r, 52)?; // CharacterData 成员数
    r.u8()?; // ServerIndex
    let data_chr_name = r.string()?.unwrap_or_default(); // Data.ChrName
    let cur_map = r.string()?.unwrap_or_default();
    let cur_x = r.i16()?;
    let cur_y = r.i16()?;
    let dir = r.u8()?;
    Ok(PlayerDataHead {
        // `LoadPlayerDataPacket.ChrName` 与 `Data.ChrName` 都记了角色名；不一致时以 Data 为准并交给调用方判断
        chr_name: if data_chr_name.is_empty() {
            packet_chr_name.unwrap_or_default()
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

/// 解析 `LoadPlayerDataPacket` 的头部窗口（不足 ⇒ `None`）。进世界路径在用（S2b-3）。
#[must_use]
pub fn parse_player_data_head(bytes: &[u8]) -> Option<PlayerDataHead> {
    parse_head_inner(bytes).ok()
}

fn parse_player_data_inner(bytes: &[u8]) -> PResult<PlayerRecord> {
    let mut r = Reader::new(bytes);
    expect_count(&mut r, 2)?; // LoadPlayerDataPacket 成员数
    let chr_name = r.string()?;
    if !obj_header(&mut r, 2)? {
        return Err(ParseError); // CharacterDataInfo 为 null：无角色数据
    }
    let header = RecordHeader::decode(&mut r)?;
    let data = CharacterData::decode(&mut r)?;
    Ok(PlayerRecord {
        chr_name,
        header,
        data,
    })
}

/// 解析 `LoadPlayerDataPacket` 的**全部 52 个 `CharacterData` 成员**（不足/成员数不符 ⇒ `None`）。
///
/// **接线点（留给 m2-svr 车道，本卡不动 `main.rs`）**：DB 回包解出后用本函数取全字段，
/// 替代 `parse_player_data_head` 只取头部；`encode_player_data` 是其逆。生产路径尚未调用，
/// 故 `allow(dead_code)`——本文件的单测已全量覆盖。
#[must_use]
#[allow(dead_code)]
pub fn parse_player_data(bytes: &[u8]) -> Option<PlayerRecord> {
    parse_player_data_inner(bytes).ok()
}

/// 把 [`PlayerRecord`] 重新编码回 MemoryPack 字节（与 C# `SerializerUtil.Serialize` 逐字节相同）。
#[must_use]
#[allow(dead_code)]
pub fn encode_player_data(rec: &PlayerRecord) -> Vec<u8> {
    let mut w = Writer::default();
    w.u8(2); // LoadPlayerDataPacket 成员数
    w.string(rec.chr_name.as_deref());
    w.u8(2); // CharacterDataInfo 成员数
    rec.header.encode(&mut w);
    rec.data.encode(&mut w);
    w.out
}

/// `SaveCharacterData`（GameSvr → DBSvr 的**存档请求体**，`PlayerDataInfo.cs`）的 MemoryPack 编码：
/// `3` 个成员 = `Account`、`ChrName`、`CharacterDataInfo{ Header, Data }`。
///
/// 逐行照 `WorldServer.MakeSaveRcd`（`:1231-1360`）产出的 `CharacterDataInfo` + `DataService.SaveHumanRcd`
/// 的反序列化端（`crates/storage` 的 `SaveCharacterData`，两侧同布局）：DBSvr 收到后用
/// `PlayDataStorage.Save.cs` 写 `characters`/`characters_ablity` 与物品族表。
///
/// `opt_obj(CharacterDataInfo)` 的对象头写 `CharacterDataInfo::members() == 2`（与 `encode_player_data` 一致）。
#[must_use]
pub fn encode_save_character_data(account: &str, chr_name: &str, rec: &PlayerRecord) -> Vec<u8> {
    let mut w = Writer::default();
    w.u8(3); // SaveCharacterData 成员数
    w.string(Some(account));
    w.string(Some(chr_name));
    w.u8(2); // CharacterDataInfo 成员数（`opt_obj` 的 Some 对象头）
    rec.header.encode(&mut w);
    rec.data.encode(&mut w);
    w.out
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

    /// 全字段 oracle（生成器 `E:\tmp\openmir2-recf\dbgen\`，生成命令与 sha256 见文件头注释）。
    const FULL_ORACLE: &str = include_str!("../tests/fixtures/player_data_full_oracle.txt");

    fn oracle(name: &str) -> Vec<u8> {
        let prefix = format!("{name}=");
        for line in FULL_ORACLE.lines() {
            for tok in line.split_whitespace() {
                if let Some(h) = tok.strip_prefix(&prefix) {
                    return hex(h.trim());
                }
            }
        }
        panic!("fixture 里找不到样例 {name}");
    }

    /// 判据③：Rust 解出的值 → 重编码 == C# 字节（逐字节）。
    fn assert_roundtrip(name: &str) -> PlayerRecord {
        let bytes = oracle(name);
        let rec = parse_player_data(&bytes).unwrap_or_else(|| panic!("{name} 必须能解析"));
        assert_eq!(
            encode_player_data(&rec),
            bytes,
            "{name} 重编码必须与 oracle 逐字节相同"
        );
        rec
    }

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

    /// 52/752 的输入接缝：`encode_ability` 必须逐字节等于 oracle `EncodeMessage(WAbil)` 的体。
    ///
    /// 用 M0 金标准 #12（`SM_ABILITY`，7200/s2c）的真字节锚定：线缆 54B ⇒ `edcode::decode` 得 40B；
    /// 补回被丢掉的成员数首字节（22）后按 [`decode_ability`] 解出 WAbil，再 [`encode_ability`]
    /// 必须回到那 40B（防“按直觉另写一个 40B”）。
    #[test]
    fn encode_ability_matches_golden_frame_12_tail() {
        fn between<'a>(s: &'a str, a: &str, b: &str) -> &'a str {
            let i = s.find(a).expect("缺少左标记") + a.len();
            let j = s[i..].find(b).expect("缺少右标记") + i;
            &s[i..j]
        }
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../tests/golden/session-20261010-c3-hook/capture.jsonl"
        );
        let capture =
            std::fs::read_to_string(path).unwrap_or_else(|e| panic!("金标准不存在：{path}（{e}）"));
        let mut tail = None;
        for line in capture.lines() {
            if !line.contains("\"dir\":\"s2c\"") || !line.contains("\"port\":7200") {
                continue;
            }
            if !line.contains("\"ident\":52,") {
                continue;
            }
            let raw = hex(between(line, "\"raw_hex\":\"", "\""));
            // 客户端线缆：'#' + Encode(头 12B) + 尾 + '!'（oracle GameGate ClientSession.cs:897-920）
            assert_eq!(raw[0], b'#');
            assert_eq!(raw[raw.len() - 1], b'!');
            tail = Some(mir2_protocol::edcode::decode(&raw[17..raw.len() - 1]));
            break;
        }
        let tail = tail.expect("金标准 #12（ident 52）必须在 7200/s2c 里");
        assert_eq!(
            tail.len(),
            40,
            "EncodeMessage(WAbil) 体 = 40B（41B 序列化丢首字节）"
        );
        let mut with_count = Vec::with_capacity(41);
        with_count.push(22u8); // 补回被 EncodeMessage 丢掉的成员数首字节
        with_count.extend_from_slice(&tail);
        let mut r = Reader::new(&with_count);
        let abil = decode_ability(&mut r)
            .expect("40B 体补首字节后必须能按 Ability 解码")
            .expect("WAbil 非 null");
        assert_eq!(abil.level, 1, "金标准 #12 解出的 level");
        assert_eq!(
            encode_ability(&abil),
            tail,
            "encode_ability 必须逐字节等于金标准 #12 的 EncodeMessage 体"
        );
    }

    /// 冻结客户端布局（`HIGHHP=True`，`Grobal2.pas:2167-2193`）：58B，`HP` 在 offset 22（DWORD）。
    ///
    /// 与真机读数对齐：oracle 40B 尾在本部署下被客户端读成 `HP=offset22=Exp=0`、
    /// `MP=offset26=MaxExp 错位`（截图 `0/0` + `655360/251658290`）；本函数把同一 `WAbil` 的
    /// `hp/mp/max_hp/max_mp` 放到客户端实际读取的偏移上。
    #[test]
    fn encode_ability_client_matches_highhp_record() {
        let abil = Ability {
            level: 1,
            ac: 3,
            mac: 4,
            dc: 257,
            mc: 5,
            sc: 6,
            hp: 19,
            mp: 15,
            max_hp: 19,
            max_mp: 15,
            exp: 10,
            max_exp: 20,
            weight: 50,
            max_weight: 60,
            wear_weight: 7,
            max_wear_weight: 8,
            hand_weight: 9,
            max_hand_weight: 12,
            ..Ability::default()
        };
        let b = encode_ability_client(&abil);
        assert_eq!(b.len(), 58, "客户端 TAbility（HIGHHP=True）= 58B");
        let u16a = |o: usize| u16::from_le_bytes([b[o], b[o + 1]]);
        let i32a = |o: usize| i32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]);
        assert_eq!(u16a(0), 1, "Level@0");
        assert_eq!(i32a(2), 3, "AC@2");
        assert_eq!(i32a(6), 4, "MAC@6");
        assert_eq!(i32a(10), 257, "DC@10");
        assert_eq!(i32a(14), 5, "MC@14");
        assert_eq!(i32a(18), 6, "SC@18");
        assert_eq!(i32a(22), 19, "HP@22（客户端 HUD 的第一对数）");
        assert_eq!(i32a(26), 15, "MP@26");
        assert_eq!(i32a(30), 19, "MaxHP@30");
        assert_eq!(i32a(34), 15, "MaxMP@34");
        assert_eq!(i32a(38), 10, "Exp@38");
        assert_eq!(i32a(42), 20, "MaxExp@42");
        assert_eq!(u16a(46), 50, "Weight@46");
        assert_eq!(u16a(48), 60, "MaxWeight@48");
        assert_eq!(u16a(50), 7, "WearWeight@50");
        assert_eq!(u16a(52), 8, "MaxWearWeight@52");
        assert_eq!(u16a(54), 9, "HandWeight@54");
        assert_eq!(u16a(56), 12, "MaxHandWeight@56");
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

    // ----- 逐组推进（stage1..stage6）-----

    #[test]
    fn stage1_header_and_ability_group() {
        let rec = assert_roundtrip("stage1");
        let d = &rec.data;
        assert_eq!(d.server_index, 3);
        assert_eq!(d.chr_name.as_deref(), Some("recf-alpha"));
        assert_eq!(d.cur_map.as_deref(), Some("map7"));
        assert_eq!((d.cur_x, d.cur_y, d.dir), (-1234, 4321, 5));
        assert_eq!((d.hair, d.sex, d.job), (6, 1, 3));
        assert_eq!(d.gold, 0x0a0b_0c0d);
        let ab = d.abil.as_ref().expect("Abil");
        assert_eq!(ab.level, 0x11);
        assert_eq!(ab.reserved1, 0x12);
        assert_eq!(ab.ac, 0x1314);
        assert_eq!(ab.mac, 0x1516);
        assert_eq!(ab.dc, 0x1718);
        assert_eq!(ab.mc, 0x191a);
        assert_eq!(ab.sc, 0x1b1c);
        assert_eq!(ab.hp, 0x1d1e);
        assert_eq!(ab.mp, 0x1f20);
        assert_eq!(ab.max_hp, 0x2122);
        assert_eq!(ab.max_mp, 0x2324);
        assert_eq!(ab.reserved2, 0x2526);
        assert_eq!((ab.exp_count, ab.exp_max_count), (0x27, 0x28));
        assert_eq!((ab.exp, ab.max_exp), (0x292a_2b2c, 0x2d2e_2f30));
        assert_eq!((ab.weight, ab.max_weight), (0x3132, 0x3334));
        assert_eq!(
            (
                ab.wear_weight,
                ab.max_wear_weight,
                ab.hand_weight,
                ab.max_hand_weight
            ),
            (0x35, 0x36, 0x37, 0x38)
        );
        let na = d.bonus_abil.as_ref().expect("BonusAbil");
        assert_eq!(
            (
                na.dc,
                na.mc,
                na.sc,
                na.ac,
                na.mac,
                na.hp,
                na.mp,
                na.hit,
                na.speed,
                na.reserved
            ),
            (
                0x1011,
                0x1213,
                0x1415,
                0x1617,
                0x1819,
                0x1a1b,
                0x1c1d,
                0x1e,
                0x1f20_2122,
                0x23
            )
        );
        // 本阶段未填充的组：其数组仍是"定长 + null 元素"（C# 构造器的默认）
        assert_eq!(d.hum_items.as_deref().map(<[_]>::len), Some(13));
        assert!(d.hum_items.as_ref().unwrap().iter().all(Option::is_none));
    }

    #[test]
    fn stage2_point_fields() {
        let rec = assert_roundtrip("stage2");
        let d = &rec.data;
        assert_eq!(d.home_map.as_deref(), Some("home9"));
        assert_eq!((d.home_x, d.home_y), (-55, -66));
        assert_eq!(d.bonus_point, 0x2425_2627);
        assert_eq!(d.credit_point, 0x28);
        assert_eq!(d.re_level, 0x29);
        assert_eq!(d.master_name.as_deref(), Some("master-x"));
        assert!(d.is_master);
        assert_eq!(d.dear_name.as_deref(), Some("dear-y"));
        assert_eq!(d.storage_pwd.as_deref(), Some("pwd-z"));
        assert_eq!(d.game_gold, 0x2a2b_2c2d);
        assert_eq!(d.game_point, 0x2e2f_3031);
        assert_eq!(d.pay_ment_point, 0x3233_3435);
        assert_eq!(d.pk_point, 0x3637_3839);
        assert_eq!(
            (
                d.allow_group,
                d.bt_f9,
                d.attatck_mode,
                d.inc_health,
                d.inc_spell,
                d.inc_healing,
                d.fight_zone_die_count,
                d.bt_ee,
                d.bt_ef
            ),
            (0x3a, 0x3b, 0x3c, 0x3d, 0x3e, 0x3f, 0x40, 0x41, 0x42)
        );
        assert_eq!(d.account.as_deref(), Some("acct-42"));
        assert!(d.lock_logon);
        assert_eq!(d.contribution, 0x4344);
        assert_eq!(d.hunger_status, 0x4546_4748);
        assert!(d.allow_guild_re_call);
        assert_eq!(d.group_rcall_time, 0x494a);
        assert_eq!(d.body_luck.to_bits(), std::f64::consts::PI.to_bits());
        assert!(!d.allow_group_re_call);
        assert_eq!(d.marry_count, 0x77);
    }

    #[test]
    fn stage3_status_time_arr() {
        let rec = assert_roundtrip("stage3");
        let arr = rec.data.status_time_arr.as_ref().expect("StatusTimeArr");
        assert_eq!(arr.len(), 15);
        assert_eq!(*arr, (1000u16..1015).collect::<Vec<_>>());
    }

    #[test]
    fn stage4_items_13_46_50() {
        let rec = assert_roundtrip("stage4");
        let d = &rec.data;
        let hum = d.hum_items.as_ref().expect("HumItems");
        let bag = d.bag_items.as_ref().expect("BagItems");
        let sto = d.storage_items.as_ref().expect("StorageItems");
        assert_eq!((hum.len(), bag.len(), sto.len()), (13, 46, 50));
        let h0 = hum[0].as_ref().expect("HumItems[0]");
        assert_eq!(h0.make_index, 0x0102_0300);
        assert_eq!((h0.index, h0.dura, h0.dura_max), (56, 1056, 1056));
        assert_eq!((h0.color_r, h0.color_g, h0.color_b), (0x11, 0x22, 0x33));
        assert_eq!(
            h0.desc.as_deref(),
            Some((0u8..=13).collect::<Vec<_>>().as_slice())
        );
        assert_eq!(
            h0.prefix.as_deref(),
            Some(('A'..='M').map(|c| c as u16).collect::<Vec<_>>().as_slice())
        );
        let last = hum[12].as_ref().expect("HumItems[12]");
        assert_eq!(last.make_index, 0x0102_0300 + 12);
        assert_eq!(last.index, 68);
        assert_eq!(last.desc.as_ref().unwrap()[0], 12 * 14);
        assert_eq!(last.desc.as_ref().unwrap()[13], 12 * 14 + 13);
        let bag0 = bag[0].as_ref().unwrap();
        assert_eq!(bag0.make_index, 0x1111_1100);
        assert_eq!((bag0.index, bag0.dura, bag0.dura_max), (136, 1136, 2136));
        assert_eq!(bag[45].as_ref().unwrap().index, 181);
        let sto0 = sto[0].as_ref().unwrap();
        assert_eq!(sto0.make_index, 0x2222_2200);
        assert_eq!((sto0.index, sto0.dura, sto0.dura_max), (272, 272, 1272));
        assert_eq!(sto[49].as_ref().unwrap().index, 321);
        assert!(d.magic.as_ref().unwrap().iter().all(Option::is_none));
    }

    #[test]
    fn stage5_magic_20() {
        let rec = assert_roundtrip("stage5");
        let m = rec.data.magic.as_ref().expect("Magic");
        assert_eq!(m.len(), 20);
        let m0 = m[0].as_ref().expect("Magic[0]");
        assert_eq!(m0.mag_idx, 3000);
        assert_eq!(m0.level, 40);
        assert_eq!(m0.magic_key, u16::from(b'm'));
        assert_eq!(m0.tran_point, 0x3031_3200);
        let m19 = m[19].as_ref().expect("Magic[19]");
        assert_eq!(m19.mag_idx, 3019);
        assert_eq!(m19.level, 59);
        assert_eq!(m19.magic_key, 0x80); // 'm' + 19 = U+0080，仍逐字节保真
        assert_eq!(m19.tran_point, 0x3031_3200 + 19);
    }

    #[test]
    fn stage6_quest_arrays_128() {
        let rec = assert_roundtrip("stage6");
        let d = &rec.data;
        let qo = d.quest_unit_open.as_ref().expect("QuestUnitOpen");
        let qu = d.quest_unit.as_ref().expect("QuestUnit");
        let qf = d.quest_flag.as_ref().expect("QuestFlag");
        assert_eq!((qo.len(), qu.len(), qf.len()), (128, 128, 128));
        assert_eq!(qo[0], 0x50);
        assert_eq!(qo[127], 0x50 + 127);
        assert_eq!(qu[0], 0x90);
        assert_eq!(qf[0], 0xd0);
        assert_eq!(qf[127], 0xd0u8.wrapping_add(127));
        assert_eq!(oracle("stage6").len(), 7742, "全字段样例长度应固定为 7742");
    }

    // ----- 边界：null 集合 / null 元素 / null 子对象 / null string -----

    #[test]
    fn null_arrays_round_trip() {
        let rec = assert_roundtrip("nulls");
        let d = &rec.data;
        // oracle 证明 [OnSerializing] 兜底未被 MemoryPack 调用：四个数组的长度前缀都是 -1。
        assert!(d.hum_items.is_none(), "null HumItems 必须解成 None");
        assert!(d.bag_items.is_none());
        assert!(d.storage_items.is_none());
        assert!(d.magic.is_none());
        // null string 与空串可区分：MasterName/DearName/StoragePwd/HomeMap/Account 均为 None
        assert!(d.master_name.is_none());
        assert!(d.dear_name.is_none());
        assert!(d.storage_pwd.is_none());
        assert!(d.home_map.is_none());
        assert!(d.account.is_none());
        // 构造器默认的定长数组仍存在
        assert_eq!(d.quest_unit_open.as_deref().map(<[_]>::len), Some(128));
        assert_eq!(d.status_time_arr.as_deref().map(<[_]>::len), Some(15));
        // 长度是 722（若实现了 [OnSerializing] 兜底，这里会远大于 722）
        assert_eq!(oracle("nulls").len(), 722);
    }

    #[test]
    fn null_array_elements_round_trip() {
        let rec = assert_roundtrip("nullitems");
        let d = &rec.data;
        for (name, arr) in [
            ("HumItems", &d.hum_items),
            ("BagItems", &d.bag_items),
            ("StorageItems", &d.storage_items),
        ] {
            let arr = arr.as_ref().unwrap_or_else(|| panic!("{name}"));
            assert!(arr.iter().all(Option::is_none), "{name} 元素全为 null");
        }
        let m = d.magic.as_ref().expect("Magic");
        assert!(m.iter().all(Option::is_none));
        assert_eq!(
            (
                d.hum_items.as_ref().unwrap().len(),
                d.bag_items.as_ref().unwrap().len(),
                d.storage_items.as_ref().unwrap().len(),
                m.len()
            ),
            (13, 46, 50, 20)
        );
        // 每个 null 元素 = 单字节 0xFF：nulls 的 4 个 -1 长度前缀（16B）换成 4 个长度 + 129 个 0xFF
        assert_eq!(oracle("nullitems").len(), 722 - 16 + 16 + 129);
    }

    #[test]
    fn null_subobjects_round_trip() {
        let rec = assert_roundtrip("abilnull");
        let d = &rec.data;
        assert!(d.abil.is_none(), "null Abil = 单字节 0xFF");
        assert!(d.bonus_abil.is_none());
        assert!(d.status_time_arr.is_none());
        // 同阶段的其它字段仍在
        assert_eq!(d.cur_map.as_deref(), Some("map7"));
        assert_eq!(d.home_map.as_deref(), Some("home9"));
    }

    #[test]
    fn null_item_desc_prefix_round_trip() {
        let bytes = oracle("nullitem");
        let mut r = Reader::new(&bytes);
        let item = ServerUserItem::decode(&mut r).unwrap().unwrap();
        assert!(item.desc.is_none());
        assert!(item.prefix.is_none());
        assert_eq!(r.i, bytes.len(), "必须恰好消费完");
        let mut w = Writer::default();
        item.encode(&mut w);
        assert_eq!(w.out, bytes, "nullitem 重编码必须与 oracle 相同");
    }

    #[test]
    fn standalone_subtype_encodings_match() {
        // abil / naked / item0 / magic0：子类型独立序列化（成员数 + 字段）逐字节对齐
        let full = oracle("stage6");
        let rec = parse_player_data(&full).unwrap();
        let mut w = Writer::default();
        encode_ability_into(rec.data.abil.as_ref().unwrap(), &mut w);
        assert_eq!(w.out, oracle("abil"));

        let mut w = Writer::default();
        encode_naked(rec.data.bonus_abil.as_ref().unwrap(), &mut w);
        assert_eq!(w.out, oracle("naked"));

        let mut w = Writer::default();
        rec.data.hum_items.as_ref().unwrap()[0]
            .as_ref()
            .unwrap()
            .encode(&mut w);
        assert_eq!(w.out, oracle("item0"));

        let mut w = Writer::default();
        rec.data.magic.as_ref().unwrap()[0]
            .as_ref()
            .unwrap()
            .encode(&mut w);
        assert_eq!(w.out, oracle("magic0"));
    }

    /// 存档请求体 `SaveCharacterData`：必须能被 **DBSvr 侧的同一类型**
    /// （`crates/storage::SaveCharacterData`，`bins/db-srv/src/data.rs` 用的就是它）解出，
    /// 且重编码回**同一字节**——这是「GameSvr 的 `record::Writer` 与 DBSvr 的 MemoryPack 同布局」
    /// 的机械证明（不是按公式重推）。
    #[test]
    fn save_character_data_roundtrips_through_dbsrv_type() {
        use mir2_storage::MemPack;
        let rec = assert_roundtrip("stage6");
        let bytes = encode_save_character_data("acct", "aaa", &rec);
        let back = mir2_storage::SaveCharacterData::from_bytes(&bytes)
            .expect("DBSvr 侧 SaveCharacterData 必须能解存档体");
        assert_eq!(back.account.as_deref(), Some("acct"));
        assert_eq!(back.chr_name.as_deref(), Some("aaa"));
        let cdi = back
            .character_data
            .as_ref()
            .expect("CharacterDataInfo 非 null");
        assert_eq!(
            cdi.data.as_ref().and_then(|d| d.chr_name.as_deref()),
            rec.data.chr_name.as_deref(),
            "嵌套 CharacterData 必须原样携带"
        );
        assert_eq!(
            mir2_storage::SaveCharacterData::to_bytes(&back),
            bytes,
            "重编码必须逐字节相同（两侧同 MemoryPack 布局）"
        );
    }
}
