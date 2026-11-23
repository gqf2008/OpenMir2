//! 怪物掉落公式，移植源：
//! - `src/GameSrv/Word/WorldServer.MonGen.cs:440`（`MonGetRandomItems`，创建怪物时
//!   预生成掉落，含 `itemName` 只赋值一次的 bug）
//! - `src/M2Server/Items/GameItemSystem.cs`（`CopyToUserItemFromName` / `GetUpgrade` /
//!   `RandomUpgradeItem` / `RandomSetUnknownItem` 及其全部子函数）
//! - `src/M2Server/M2Share.cs:547`（`GetItemNumber` 计数器）
//!
//! 所有随机调用顺序与 C# 逐条一致——这是对拍成立的前提。

use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::Arc;

use mir2_data::models::{MonsterDropItem, StdItem, STD_MODE_MAP};
use mir2_shared::hutil;
use mir2_shared::itemnum_trace;
use mir2_shared::rng::RandomSource;

/// `src/OpenMir2/Packets/ClientPackets/ClientUserItem.cs` 的 `UserItem` 子集
/// （掉落公式会写到的字段；`color_r/g/b`、`prefix` 掉落路径不触碰，略）。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UserItem {
    /// 唯一 ID（`M2Share.GetItemNumber` 分配）
    pub make_index: i32,
    /// 物品 ID（1 起始，对应 StdItemList 下标 + 1）
    pub index: u16,
    /// 当前持久值
    pub dura: u16,
    /// 最大持久值
    pub dura_max: u16,
    /// 附加属性（C# 为 byte[14]）
    pub desc: [u8; 14],
}

/// C# `M2Share.GetItemNumber`（`M2Share.cs:547`）的**唯一**计数器一步：CAS 版。
///
/// 逐字语义：`++ItemNumber` → `> int.MaxValue/2 - 1` 回绕到 `1`（`wrapping_add` 防溢出）。
/// **返回计数值本身、不加 `GetTickCount()`**——这是偏差 `T-72` 的**有意确定性偏离**
/// （oracle 返回 `ItemNumber + HUtil32.GetTickCount()`）：保留跨跑确定性，N=1 两树才能逐字节
/// 比对。`0c` 已核 `MakeIndex` 无 glog/sort/TSV/排序/RNG 依赖，DB/帧面掩码。
/// **若将来解 D-1 的 `MakeIndex` 掩码，必须先补 tick**（`mir2-rs/tests/parity/whitelist.md` T-72）。
///
/// **M1c-2a**：oracle 只有一个活发号器（`GetItemNumberEx` 是全仓零调用的死码）⇒ 本仓的
/// 五类造件（掉落/商店/脚本/GM/使用物品）**共用同一个计数器**；`next_item_number` 是
/// **唯一**实现（`ItemNumberCounter::next_value` 与宿主直调点都走它，禁止第二份 CAS）。
pub fn next_item_number(counter: &AtomicI32) -> i32 {
    loop {
        let old = counter.load(Ordering::Relaxed);
        let mut v = old.wrapping_add(1);
        if v > i32::MAX / 2 - 1 {
            v = 1;
        }
        if counter
            .compare_exchange_weak(old, v, Ordering::Relaxed, Ordering::Relaxed)
            .is_ok()
        {
            // **M1c-2a 诊断旁路**：`MIR2_ITEMNUM_TRACE` 未设时只是一次 relaxed 原子读。
            itemnum_trace::record(v);
            return v;
        }
        // 竞争失败（另一区/另一线程先推进）：重读旧值再试，无失败路径。
    }
}

/// 进程级共享的 `MakeIndex` 计数器句柄（**内部只是同一个 `Arc<AtomicI32>`**）。
///
/// **M1c-2a 三→一**：① 宿主 `item_number` 的 `Arc` 是**唯一**源；`World::item_number`
/// 用 [`ItemNumberCounter::from_shared`] 接同一 `Arc` ⇒ 各区的掉落/商人与本进程的
/// 商店/脚本/GM 造件共用一个号段，运行内逐值无重复（复位窗口之间例外，见 T-72）。
#[derive(Clone, Debug)]
pub struct ItemNumberCounter {
    shared: Arc<AtomicI32>,
}

impl Default for ItemNumberCounter {
    /// **测试 / legacy 专用**（新 `Arc(0)`）；**生产必须 [`ItemNumberCounter::from_shared`]**
    /// 接 `run_inner` 装配的那一个 `Arc`（否则各区/各 World 各起一份，号段会撞）。
    fn default() -> Self {
        Self::from_shared(Arc::new(AtomicI32::new(0)))
    }
}

impl ItemNumberCounter {
    /// 接一个已存在的共享原子（生产唯一注入形态）。
    pub fn from_shared(shared: Arc<AtomicI32>) -> Self {
        Self { shared }
    }

    /// 共享句柄（`Arc::ptr_eq` 断言「同一计数器」用）。
    pub fn shared(&self) -> &Arc<AtomicI32> {
        &self.shared
    }

    /// 当前计数值（诊断/测试）。
    pub fn load(&self) -> i32 {
        self.shared.load(Ordering::Relaxed)
    }

    /// `M2Share.GetItemNumber()` 的一步（[`next_item_number`] 的同实现）。
    pub fn next_value(&self) -> i32 {
        next_item_number(&self.shared)
    }
}

/// 对应 `SystemShare.ItemSystem` 在掉落路径上用到的子集。
pub trait ItemCatalog {
    /// C# `GameItemSystem.CopyToUserItemFromName`（`GameItemSystem.cs:113`）：
    /// 按名（OrdinalIgnoreCase）找第一个物品，生成 `UserItem`
    /// （`index = 列表下标 + 1`、`dura = dura_max = StdItem.DuraMax`）。
    fn copy_to_user_item_from_name(&self, name: &str, make_index: i32) -> Option<UserItem>;

    /// C# `GameItemSystem.GetStdItem(ushort)`（`GameItemSystem.cs:20`）：
    /// `index - 1` 越界或条目名为空 → None（C# 返回 null）。
    fn get_std_item(&self, index: u16) -> Option<&StdItem>;
}

/// 基于 `Vec<StdItem>` 的目录（`StdItemList` 的直接对应物）。
pub struct VecItemCatalog<'a> {
    pub items: &'a [StdItem],
}

/// C# `string.Equals(a, b, OrdinalIgnoreCase)` 的近似：
/// 双方做 Unicode 大写折叠后按序比较。物品名基本是中文/ASCII，差异面见
/// `tests/parity/whitelist.md`。
fn equals_ordinal_ignore_case(a: &str, b: &str) -> bool {
    a.chars()
        .flat_map(char::to_uppercase)
        .eq(b.chars().flat_map(char::to_uppercase))
}

impl ItemCatalog for VecItemCatalog<'_> {
    fn copy_to_user_item_from_name(&self, name: &str, make_index: i32) -> Option<UserItem> {
        if name.is_empty() {
            return None;
        }
        for (i, std_item) in self.items.iter().enumerate() {
            if !equals_ordinal_ignore_case(&std_item.name, name) {
                continue;
            }
            return Some(UserItem {
                make_index,
                index: (i + 1) as u16,
                dura: std_item.dura_max,
                dura_max: std_item.dura_max,
                desc: [0; 14],
            });
        }
        None
    }

    fn get_std_item(&self, index: u16) -> Option<&StdItem> {
        // C#: nItemIdx -= 1（ushort 回绕）；0 → 65535 → 越界 → null
        let idx = index.wrapping_sub(1);
        let item = self.items.get(usize::from(idx))?;
        if item.name.is_empty() {
            return None;
        }
        Some(item)
    }
}

/// C# `GameItemSystem.GetUpgrade`（`GameItemSystem.cs:142`）：
/// 连续成功计数，失败即停。
fn get_upgrade(rng: &mut impl RandomSource, count: i32, ran: i32) -> i32 {
    let mut result = 0;
    for _ in 0..count {
        if rng.random_below(ran) == 0 {
            result += 1;
        } else {
            break;
        }
    }
    result
}

/// C# `UpgradeRandomWeapon`（`GameItemSystem.cs:182`）。
fn upgrade_random_weapon(pu: &mut UserItem, rng: &mut impl RandomSource) {
    let mut up = get_upgrade(rng, 12, 15);
    if rng.random_below(15) == 0 {
        pu.desc[0] = (1 + up) as u8; // DC
    }
    up = get_upgrade(rng, 12, 15);
    if rng.random_below(20) == 0 {
        let incp = (1 + up) / 3;
        if incp > 0 {
            if rng.random_below(3) != 0 {
                pu.desc[6] = incp as u8;
            } else {
                pu.desc[6] = (10 + incp) as u8;
            }
        }
    }
    up = get_upgrade(rng, 12, 15);
    if rng.random_below(15) == 0 {
        pu.desc[1] = (1 + up) as u8; // MC
    }
    up = get_upgrade(rng, 12, 15);
    if rng.random_below(15) == 0 {
        pu.desc[2] = (1 + up) as u8; // SC
    }
    up = get_upgrade(rng, 12, 15);
    if rng.random_below(24) == 0 {
        pu.desc[5] = (1 + up / 2) as u8;
    }
    up = get_upgrade(rng, 12, 12);
    if rng.random_below(3) < 2 {
        let n = (1 + up) * 2000;
        pu.dura_max = hutil::min_i32(65000, i32::from(pu.dura_max) + n) as u16;
        pu.dura = hutil::min_i32(65000, i32::from(pu.dura) + n) as u16;
    }
    up = get_upgrade(rng, 12, 15);
    if rng.random_below(10) == 0 {
        pu.desc[7] = (1 + up / 2) as u8;
    }
}

/// C# `UpgradeRandomDress`（`GameItemSystem.cs:234`）。
fn upgrade_random_dress(pu: &mut UserItem, rng: &mut impl RandomSource) {
    let mut up = get_upgrade(rng, 6, 15);
    if rng.random_below(30) == 0 {
        pu.desc[0] = (1 + up) as u8; // AC
    }
    up = get_upgrade(rng, 6, 15);
    if rng.random_below(30) == 0 {
        pu.desc[1] = (1 + up) as u8; // MAC
    }
    up = get_upgrade(rng, 6, 20);
    if rng.random_below(40) == 0 {
        pu.desc[2] = (1 + up) as u8; // DC
    }
    up = get_upgrade(rng, 6, 20);
    if rng.random_below(40) == 0 {
        pu.desc[3] = (1 + up) as u8; // MC
    }
    up = get_upgrade(rng, 6, 20);
    if rng.random_below(40) == 0 {
        pu.desc[4] = (1 + up) as u8; // SC
    }
    up = get_upgrade(rng, 6, 10);
    if rng.random_below(8) < 6 {
        let n = (1 + up) * 2000;
        pu.dura_max = hutil::min_i32(65000, i32::from(pu.dura_max) + n) as u16;
        pu.dura = hutil::min_i32(65000, i32::from(pu.dura) + n) as u16;
    }
}

/// C# `UpgradeRandomNecklace`（`GameItemSystem.cs:270`）。
fn upgrade_random_necklace(pu: &mut UserItem, rng: &mut impl RandomSource) {
    let mut up = get_upgrade(rng, 6, 30);
    if rng.random_below(60) == 0 {
        pu.desc[0] = (1 + up) as u8; // AC(HIT)
    }
    up = get_upgrade(rng, 6, 30);
    if rng.random_below(60) == 0 {
        pu.desc[1] = (1 + up) as u8; // MAC(SPEED)
    }
    up = get_upgrade(rng, 6, 20);
    if rng.random_below(30) == 0 {
        pu.desc[2] = (1 + up) as u8; // DC
    }
    up = get_upgrade(rng, 6, 20);
    if rng.random_below(30) == 0 {
        pu.desc[3] = (1 + up) as u8; // MC
    }
    up = get_upgrade(rng, 6, 20);
    if rng.random_below(30) == 0 {
        pu.desc[4] = (1 + up) as u8; // SC
    }
    up = get_upgrade(rng, 6, 12);
    if rng.random_below(20) < 15 {
        let n = (1 + up) * 1000;
        pu.dura_max = hutil::min_i32(65000, i32::from(pu.dura_max) + n) as u16;
        pu.dura = hutil::min_i32(65000, i32::from(pu.dura) + n) as u16;
    }
}

/// C# `UpgradeRandomBarcelet`（`GameItemSystem.cs:306`，原文拼写如此）。
fn upgrade_random_barcelet(pu: &mut UserItem, rng: &mut impl RandomSource) {
    let mut up = get_upgrade(rng, 6, 20);
    if rng.random_below(20) == 0 {
        pu.desc[0] = (1 + up) as u8; // AC
    }
    up = get_upgrade(rng, 6, 20);
    if rng.random_below(20) == 0 {
        pu.desc[1] = (1 + up) as u8; // MAC
    }
    up = get_upgrade(rng, 6, 20);
    if rng.random_below(30) == 0 {
        pu.desc[2] = (1 + up) as u8; // DC
    }
    up = get_upgrade(rng, 6, 20);
    if rng.random_below(30) == 0 {
        pu.desc[3] = (1 + up) as u8; // MC
    }
    up = get_upgrade(rng, 6, 20);
    if rng.random_below(30) == 0 {
        pu.desc[4] = (1 + up) as u8; // SC
    }
    up = get_upgrade(rng, 6, 12);
    if rng.random_below(20) < 15 {
        let n = (1 + up) * 1000;
        pu.dura_max = hutil::min_i32(65000, i32::from(pu.dura_max) + n) as u16;
        pu.dura = hutil::min_i32(65000, i32::from(pu.dura) + n) as u16;
    }
}

/// C# `UpgradeRandomNecklace19`（`GameItemSystem.cs:342`）。
fn upgrade_random_necklace19(pu: &mut UserItem, rng: &mut impl RandomSource) {
    let mut up = get_upgrade(rng, 6, 20);
    if rng.random_below(40) == 0 {
        pu.desc[0] = (1 + up) as u8;
    }
    up = get_upgrade(rng, 6, 20);
    if rng.random_below(40) == 0 {
        pu.desc[1] = (1 + up) as u8;
    }
    up = get_upgrade(rng, 6, 20);
    if rng.random_below(30) == 0 {
        pu.desc[2] = (1 + up) as u8; // DC
    }
    up = get_upgrade(rng, 6, 20);
    if rng.random_below(30) == 0 {
        pu.desc[3] = (1 + up) as u8; // MC
    }
    up = get_upgrade(rng, 6, 20);
    if rng.random_below(30) == 0 {
        pu.desc[4] = (1 + up) as u8; // SC
    }
    up = get_upgrade(rng, 6, 10);
    if rng.random_below(4) < 3 {
        let n = (1 + up) * 1000;
        pu.dura_max = hutil::min_i32(65000, i32::from(pu.dura_max) + n) as u16;
        pu.dura = hutil::min_i32(65000, i32::from(pu.dura) + n) as u16;
    }
}

/// C# `UpgradeRandomRings`（`GameItemSystem.cs:378`）。
fn upgrade_random_rings(pu: &mut UserItem, rng: &mut impl RandomSource) {
    let mut up = get_upgrade(rng, 6, 20);
    if rng.random_below(30) == 0 {
        pu.desc[2] = (1 + up) as u8; // DC
    }
    up = get_upgrade(rng, 6, 20);
    if rng.random_below(30) == 0 {
        pu.desc[3] = (1 + up) as u8; // MC
    }
    up = get_upgrade(rng, 6, 20);
    if rng.random_below(30) == 0 {
        pu.desc[4] = (1 + up) as u8; // SC
    }
    up = get_upgrade(rng, 6, 12);
    if rng.random_below(4) < 3 {
        let n = (1 + up) * 1000;
        pu.dura_max = hutil::min_i32(65000, i32::from(pu.dura_max) + n) as u16;
        pu.dura = hutil::min_i32(65000, i32::from(pu.dura) + n) as u16;
    }
}

/// C# `UpgradeRandomRings23`（`GameItemSystem.cs:404`）。
fn upgrade_random_rings23(pu: &mut UserItem, rng: &mut impl RandomSource) {
    let mut up = get_upgrade(rng, 6, 20);
    if rng.random_below(40) == 0 {
        pu.desc[0] = (1 + up) as u8;
    }
    up = get_upgrade(rng, 6, 20);
    if rng.random_below(40) == 0 {
        pu.desc[1] = (1 + up) as u8;
    }
    up = get_upgrade(rng, 6, 20);
    if rng.random_below(30) == 0 {
        pu.desc[2] = (1 + up) as u8; // DC
    }
    up = get_upgrade(rng, 6, 20);
    if rng.random_below(30) == 0 {
        pu.desc[3] = (1 + up) as u8; // MC
    }
    up = get_upgrade(rng, 6, 20);
    if rng.random_below(30) == 0 {
        pu.desc[4] = (1 + up) as u8; // SC
    }
    up = get_upgrade(rng, 6, 12);
    if rng.random_below(4) < 3 {
        let n = (1 + up) * 1000;
        pu.dura_max = hutil::min_i32(65000, i32::from(pu.dura_max) + n) as u16;
        pu.dura = hutil::min_i32(65000, i32::from(pu.dura) + n) as u16;
    }
}

/// C# `UpgradeRandomHelmet`（`GameItemSystem.cs:440`）。
fn upgrade_random_helmet(pu: &mut UserItem, rng: &mut impl RandomSource) {
    let mut up = get_upgrade(rng, 6, 20);
    if rng.random_below(40) == 0 {
        pu.desc[0] = (1 + up) as u8; // AC
    }
    up = get_upgrade(rng, 6, 20);
    if rng.random_below(30) == 0 {
        pu.desc[1] = (1 + up) as u8; // MAC
    }
    up = get_upgrade(rng, 6, 20);
    if rng.random_below(30) == 0 {
        pu.desc[2] = (1 + up) as u8; // DC
    }
    up = get_upgrade(rng, 6, 20);
    if rng.random_below(30) == 0 {
        pu.desc[3] = (1 + up) as u8; // MC
    }
    up = get_upgrade(rng, 6, 20);
    if rng.random_below(30) == 0 {
        pu.desc[4] = (1 + up) as u8; // SC
    }
    up = get_upgrade(rng, 6, 12);
    if rng.random_below(4) < 3 {
        let n = (1 + up) * 1000;
        pu.dura_max = hutil::min_i32(65000, i32::from(pu.dura_max) + n) as u16;
        pu.dura = hutil::min_i32(65000, i32::from(pu.dura) + n) as u16;
    }
}

/// C# `GameItemSystem.RandomUpgradeItem`（`GameItemSystem.cs:1184`）。
pub fn random_upgrade_item(std_item: &StdItem, pu: &mut UserItem, rng: &mut impl RandomSource) {
    match std_item.std_mode {
        5 | 6 => upgrade_random_weapon(pu, rng),
        10 | 11 => upgrade_random_dress(pu, rng),
        19 => upgrade_random_necklace19(pu, rng),
        20 | 21 | 24 => upgrade_random_necklace(pu, rng),
        26 => upgrade_random_barcelet(pu, rng),
        22 => upgrade_random_rings(pu, rng),
        23 => upgrade_random_rings23(pu, rng),
        15 => upgrade_random_helmet(pu, rng),
        _ => {}
    }
}

/// C# `RandomSetUnknownHelmet`（`GameItemSystem.cs:476`）。
fn random_set_unknown_helmet(pu: &mut UserItem, rng: &mut impl RandomSource) {
    let mut up = get_upgrade(rng, 4, 3) + get_upgrade(rng, 4, 8) + get_upgrade(rng, 4, 20);
    if up > 0 {
        pu.desc[0] = up as u8; // AC
    }
    let mut sum = up;
    up = get_upgrade(rng, 4, 3) + get_upgrade(rng, 4, 8) + get_upgrade(rng, 4, 20);
    if up > 0 {
        pu.desc[1] = up as u8; // MAC
    }
    sum += up;
    up = get_upgrade(rng, 3, 15) + get_upgrade(rng, 3, 30);
    if up > 0 {
        pu.desc[2] = up as u8; // DC
    }
    sum += up;
    up = get_upgrade(rng, 3, 15) + get_upgrade(rng, 3, 30);
    if up > 0 {
        pu.desc[3] = up as u8; // MC
    }
    sum += up;
    up = get_upgrade(rng, 3, 15) + get_upgrade(rng, 3, 30);
    if up > 0 {
        pu.desc[4] = up as u8; // SC
    }
    sum += up;
    up = get_upgrade(rng, 6, 30);
    if up > 0 {
        let n = (1 + up) * 1000;
        pu.dura_max = hutil::min_i32(65000, i32::from(pu.dura_max) + n) as u16;
        pu.dura = hutil::min_i32(65000, i32::from(pu.dura) + n) as u16;
    }
    if rng.random_below(30) == 0 {
        pu.desc[7] = 1;
    }
    pu.desc[8] = 1;
    if sum >= 3 {
        if pu.desc[0] >= 5 {
            pu.desc[5] = 1;
            pu.desc[6] = (25 + i32::from(pu.desc[0]) * 3) as u8;
            return;
        }
        if pu.desc[2] >= 2 {
            pu.desc[5] = 1;
            pu.desc[6] = (35 + i32::from(pu.desc[2]) * 4) as u8;
            return;
        }
        if pu.desc[3] >= 2 {
            pu.desc[5] = 2;
            pu.desc[6] = (18 + i32::from(pu.desc[3]) * 2) as u8;
            return;
        }
        if pu.desc[4] >= 2 {
            pu.desc[5] = 3;
            pu.desc[6] = (18 + i32::from(pu.desc[4]) * 2) as u8;
            return;
        }
        pu.desc[6] = (18 + sum * 2) as u8;
    }
}

/// C# `RandomSetUnknownRing`（`GameItemSystem.cs:550`）。
fn random_set_unknown_ring(pu: &mut UserItem, rng: &mut impl RandomSource) {
    let mut up = get_upgrade(rng, 3, 4) + get_upgrade(rng, 3, 8) + get_upgrade(rng, 6, 20);
    if up > 0 {
        pu.desc[2] = up as u8; // DC
    }
    let mut sum = up;
    up = get_upgrade(rng, 3, 4) + get_upgrade(rng, 3, 8) + get_upgrade(rng, 6, 20);
    if up > 0 {
        pu.desc[3] = up as u8; // MC
    }
    sum += up;
    up = get_upgrade(rng, 3, 4) + get_upgrade(rng, 3, 8) + get_upgrade(rng, 6, 20);
    if up > 0 {
        pu.desc[4] = up as u8; // SC
    }
    sum += up;
    up = get_upgrade(rng, 6, 30);
    if up > 0 {
        let n = (1 + up) * 1000;
        pu.dura_max = hutil::min_i32(65000, i32::from(pu.dura_max) + n) as u16;
        pu.dura = hutil::min_i32(65000, i32::from(pu.dura) + n) as u16;
    }
    if rng.random_below(30) == 0 {
        pu.desc[7] = 1;
    }
    pu.desc[8] = 1;
    if sum >= 3 {
        if pu.desc[2] >= 3 {
            pu.desc[5] = 1;
            pu.desc[6] = (25 + i32::from(pu.desc[2]) * 3) as u8;
            return;
        }
        if pu.desc[3] >= 3 {
            pu.desc[5] = 2;
            pu.desc[6] = (18 + i32::from(pu.desc[3]) * 2) as u8;
            return;
        }
        if pu.desc[4] >= 3 {
            pu.desc[5] = 3;
            pu.desc[6] = (18 + i32::from(pu.desc[4]) * 2) as u8;
            return;
        }
        pu.desc[6] = (18 + sum * 2) as u8;
    }
}

/// C# `RandomSetUnknownBracelet`（`GameItemSystem.cs:606`）。
fn random_set_unknown_bracelet(pu: &mut UserItem, rng: &mut impl RandomSource) {
    let mut up = get_upgrade(rng, 3, 5) + get_upgrade(rng, 5, 20);
    if up > 0 {
        pu.desc[0] = up as u8; // AC
    }
    let mut sum = up;
    up = get_upgrade(rng, 3, 5) + get_upgrade(rng, 5, 20);
    if up > 0 {
        pu.desc[1] = up as u8; // MAC
    }
    sum += up;
    up = get_upgrade(rng, 3, 15) + get_upgrade(rng, 5, 30);
    if up > 0 {
        pu.desc[2] = up as u8; // DC
    }
    sum += up;
    up = get_upgrade(rng, 3, 15) + get_upgrade(rng, 5, 30);
    if up > 0 {
        pu.desc[3] = up as u8; // MC
    }
    sum += up;
    up = get_upgrade(rng, 3, 15) + get_upgrade(rng, 5, 30);
    if up > 0 {
        pu.desc[4] = up as u8; // SC
    }
    sum += up;
    up = get_upgrade(rng, 6, 30);
    if up > 0 {
        let n = (1 + up) * 1000;
        pu.dura_max = hutil::min_i32(65000, i32::from(pu.dura_max) + n) as u16;
        pu.dura = hutil::min_i32(65000, i32::from(pu.dura) + n) as u16;
    }
    if rng.random_below(30) == 0 {
        pu.desc[7] = 1;
    }
    pu.desc[8] = 1;
    if sum >= 2 {
        if pu.desc[0] >= 3 {
            pu.desc[5] = 1;
            pu.desc[6] = (25 + i32::from(pu.desc[0]) * 3) as u8;
            return;
        }
        if pu.desc[2] >= 2 {
            pu.desc[5] = 1;
            pu.desc[6] = (30 + i32::from(pu.desc[2]) * 3) as u8;
            return;
        }
        if pu.desc[3] >= 2 {
            pu.desc[5] = 2;
            pu.desc[6] = (20 + i32::from(pu.desc[3]) * 2) as u8;
            return;
        }
        if pu.desc[4] >= 2 {
            pu.desc[5] = 3;
            pu.desc[6] = (20 + i32::from(pu.desc[4]) * 2) as u8;
            return;
        }
        pu.desc[6] = (18 + sum * 2) as u8;
    }
}

/// C# `GameItemSystem.RandomSetUnknownItem`（`GameItemSystem.cs:1222`）。
pub fn random_set_unknown_item(std_item: &StdItem, pu: &mut UserItem, rng: &mut impl RandomSource) {
    match std_item.std_mode {
        15 => random_set_unknown_helmet(pu, rng),
        22 | 23 => random_set_unknown_ring(pu, rng),
        24 | 26 => random_set_unknown_bracelet(pu, rng),
        _ => {}
    }
}

/// `MonGetRandomItems` 的产出：金币 + 物品列表（对应 `mon.Gold` 与 `mon.ItemList`）。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DropOutcome {
    pub gold: i32,
    pub items: Vec<UserItem>,
}

/// C# `WorldServer.MonGetRandomItems`（`WorldServer.MonGen.cs:440`）逐条移植。
///
/// 入参对应：
/// - `item_list`：`MonsterInfo.ItemList`（由 `mir2_data::monitems` 解析）；
/// - `catalog`：`SystemShare.ItemSystem`；
/// - `rng`：`M2Share.RandomNumber`；
/// - `mon_random_add_value`：`SystemShare.Config.MonRandomAddValue`（极品掉落几率）；
/// - `gold_name`：`Grobal2.StringGoldName`（"金币"）；
/// - `item_number`：`M2Share.GetItemNumber` 的全局计数器。
///
/// BUG-REPLICA：`itemName` 在循环外只初始化一次，第一个掉落的非金币物品
/// 确定后，后续所有非金币掉落都复用这个名字（C# 原样，见
/// `WorldServer.MonGen.cs:453-456`）。
pub fn mon_get_random_items<C: ItemCatalog>(
    item_list: &[MonsterDropItem],
    catalog: &C,
    rng: &mut impl RandomSource,
    mon_random_add_value: i32,
    gold_name: &str,
    item_number: &ItemNumberCounter,
) -> DropOutcome {
    let mut item_name = String::new(); // BUG-REPLICA：C# 在循环外声明且只赋值一次
    let mut outcome = DropOutcome {
        gold: 0,
        items: Vec::with_capacity(item_list.len()),
    };
    for mon_item in item_list {
        if rng.random_below(mon_item.max_point) <= mon_item.sel_point {
            if equals_ordinal_ignore_case(&mon_item.item_name, gold_name) {
                outcome.gold = outcome.gold + mon_item.count / 2 + rng.random_below(mon_item.count);
            } else {
                if item_name.is_empty() {
                    item_name = mon_item.item_name.clone();
                }
                if let Some(mut user_item) =
                    catalog.copy_to_user_item_from_name(&item_name, item_number.next_value())
                {
                    user_item.dura = hutil::round(
                        f64::from(user_item.dura_max) / 100.0
                            * f64::from(20 + rng.random_below(80)),
                    ) as u16;
                    // C#: GetStdItem 为 null 时 continue（注意 dura 的随机数已消耗）
                    let Some(std_item) = catalog.get_std_item(user_item.index) else {
                        continue;
                    };
                    if std_item.std_mode > 0 && rng.random_below(mon_random_add_value) == 0 {
                        // 极品掉落几率
                        random_upgrade_item(std_item, &mut user_item, rng);
                    }
                    if STD_MODE_MAP.contains(&std_item.std_mode)
                        && matches!(std_item.shape, 130..=132)
                    {
                        random_set_unknown_item(std_item, &mut user_item, rng);
                    }
                    outcome.items.push(user_item);
                }
            }
        }
    }
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;
    use mir2_shared::rng::RandomNumber;

    fn catalog(items: &[StdItem]) -> VecItemCatalog<'_> {
        VecItemCatalog { items }
    }

    fn std_item(name: &str, std_mode: u8, shape: u8, dura_max: u16) -> StdItem {
        StdItem {
            name: name.to_string(),
            std_mode,
            shape,
            dura_max,
            ..Default::default()
        }
    }

    #[test]
    fn gold_uses_half_plus_random() {
        let items = [std_item("金币", 0, 0, 0)];
        let drops = vec![MonsterDropItem {
            sel_point: 9,
            max_point: 10,
            item_name: "金币".to_string(),
            count: 300,
        }];
        let mut rng = RandomNumber::with_seed(42);
        let counter = ItemNumberCounter::default();
        let out = mon_get_random_items(&drops, &catalog(&items), &mut rng, 10, "金币", &counter);
        // 固定种子下 C# 对拍验证具体值；这里验证结构：gold = 150 + Random(300) ∈ [150, 450)
        assert!(out.items.is_empty());
        assert!((150..450).contains(&out.gold));
    }

    #[test]
    fn item_name_bug_replica() {
        // BUG-REPLICA：两件必掉的不同物品，第二件实际复制的是第一件的名字。
        let items = [
            std_item("乌木剑", 5, 1, 1000),
            std_item("布衣(男)", 10, 2, 2000),
        ];
        let drops = vec![
            MonsterDropItem {
                sel_point: 9,
                max_point: 10,
                item_name: "乌木剑".to_string(),
                count: 1,
            },
            MonsterDropItem {
                sel_point: 9,
                max_point: 10,
                item_name: "布衣(男)".to_string(),
                count: 1,
            },
        ];
        let mut rng = RandomNumber::with_seed(1);
        let counter = ItemNumberCounter::default();
        let out = mon_get_random_items(&drops, &catalog(&items), &mut rng, 10, "金币", &counter);
        assert_eq!(out.items.len(), 2);
        // 两件都是 index=1（乌木剑）——第二件的名字被第一件覆盖
        assert_eq!(out.items[0].index, 1);
        assert_eq!(out.items[1].index, 1);
    }

    #[test]
    fn unknown_item_sets_desc8() {
        // StdMode=15 ∈ StdModeMap 且 Shape=130 → 走 RandomSetUnknownItem，desc[8] 恒置 1
        let items = [std_item("神秘头盔", 15, 130, 1000)];
        let drops = vec![MonsterDropItem {
            sel_point: 9,
            max_point: 10,
            item_name: "神秘头盔".to_string(),
            count: 1,
        }];
        let mut rng = RandomNumber::with_seed(5);
        let counter = ItemNumberCounter::default();
        let out = mon_get_random_items(&drops, &catalog(&items), &mut rng, 10, "金币", &counter);
        assert_eq!(out.items.len(), 1);
        assert_eq!(out.items[0].desc[8], 1);
    }

    #[test]
    fn item_number_counter_wraps() {
        let c = ItemNumberCounter::from_shared(Arc::new(AtomicI32::new(i32::MAX / 2 - 1)));
        assert_eq!(c.next_value(), 1);
    }

    /// **M1c-2a 牙 1【行为】`unified_counter_shared_arc_interleaved`**：两个
    /// `from_shared(同一 Arc)` 交错取号 ⇒ `[1,2]`、无重复。
    ///
    /// 红因：`from_shared` 各起一份新 `Arc` ⇒ 两边都从 1 起 ⇒ 相等断言红。
    #[test]
    fn unified_counter_shared_arc_interleaved() {
        let shared = Arc::new(AtomicI32::new(0));
        let a = ItemNumberCounter::from_shared(Arc::clone(&shared));
        let b = ItemNumberCounter::from_shared(Arc::clone(&shared));
        let first = a.next_value();
        let second = b.next_value();
        assert_eq!((first, second), (1, 2), "同一 Arc 交错取号必须连号");
        assert_eq!(shared.load(Ordering::Relaxed), 2);
        assert!(
            Arc::ptr_eq(a.shared(), b.shared()),
            "两个句柄必须指向同一 Arc"
        );
    }

    /// **M1c-2a 牙 2【行为】`unified_counter_reset_semantics`**：置 `MAX/2-1` ⇒ 下一次 = 1；
    /// `MAX/2` 也 = 1（**oracle 主计数器语义**：`> MAX/2-1 ⇒ 1`；② 的 `MAX/2` 下限 clamp 已随
    /// `GetItemNumberEx`（死码）删除，**不得**复活）。
    #[test]
    fn unified_counter_reset_semantics() {
        for (start, expected) in [
            (i32::MAX / 2 - 1, 1),
            (i32::MAX / 2, 1),
            (i32::MAX / 2 + 1, 1),
        ] {
            let counter = AtomicI32::new(start);
            assert_eq!(
                next_item_number(&counter),
                expected,
                "start={start} ⇒ 复位到 1（oracle 语义）"
            );
            assert_eq!(counter.load(Ordering::Relaxed), expected);
        }
        // 未到阈值 ⇒ 照常 +1（防止「无条件置 1」的过度复位）。
        let counter = AtomicI32::new(i32::MAX / 2 - 2);
        assert_eq!(next_item_number(&counter), i32::MAX / 2 - 1);
    }

    /// **M1c-2a 牙 3【行为】`unified_counter_no_tick`**：返回值 == CAS 后的计数值本身
    /// （**不叠时钟**；oracle 返回 `ItemNumber + GetTickCount()`，T-72 有意偏离）。
    ///
    /// 红因：返回值里加 `good_tick_i32()`/`GetTickCount()` ⇒ 与 `load()` 不等 ⇒ 红。
    #[test]
    fn unified_counter_no_tick() {
        let counter = AtomicI32::new(1_000);
        for _ in 0..3 {
            let v = next_item_number(&counter);
            assert_eq!(
                v,
                counter.load(Ordering::Relaxed),
                "返回的必须就是计数值（不带 tick）"
            );
        }
        assert_eq!(counter.load(Ordering::Relaxed), 1_003);
    }

    /// **M1c-2a 牙 9【行为】`mir2_itemnum_trace_default_off`**：旁路默认关 ⇒ **跑过造件、
    /// 写盘 0 行**、数值与未加旁路逐项相同（拿到的是 1,2,3，不是别的）。
    ///
    /// 红因：`record` 无条件写盘/默认开 ⇒ 写盘行数 > 0。
    #[test]
    fn mir2_itemnum_trace_default_off() {
        // 以「写盘行数不增」为判据，不依赖全局是否曾被开启（开启类用例自带复位）。
        let written_before = itemnum_trace::written();
        let counter = ItemNumberCounter::default();
        let got = [
            counter.next_value(),
            counter.next_value(),
            counter.next_value(),
        ];
        assert_eq!(got, [1, 2, 3], "默认关时取号行为逐项不变");
        assert_eq!(
            itemnum_trace::written(),
            written_before,
            "旁路默认关：不得写盘（MIR2_ITEMNUM_TRACE 未设）"
        );
    }
}
