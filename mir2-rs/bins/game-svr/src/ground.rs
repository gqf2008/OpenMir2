//! 「拾取（`CM_PICKUP=1001`）/ 丢弃（`CM_DROPITEM=1000`）」的**决策 + 状态迁移 + 回帧**纯函数接缝。
//!
//! # 这条接缝给谁用
//!
//! oracle 的 `CM_PICKUP`/`CM_DROPITEM` 落在 `PlayObject.Message.cs:811/:801`，真实实现是
//! `PlayObject.Base.cs:55`（`ClientPickUpItem`）与 `PlayObject.Operate.cs:251`（`ClientDropItem`）。
//! 两者把「地图格子上的地面物品（`MapItem`）」与「玩家背包 `ItemList` / 金币 `Gold` / `WAbil.Weight`」
//! 做一次迁移，并回 `SM_DROPITEM_SUCCESS/FAIL`（600/601）或 `SM_ADDITEM`（200），其余走 `MsgQueue`
//! 的延时通知。`openmir2-m3-bag-equip-alignment` 的 DoD（拾取→装备→卸下→丢弃→用药、两侧同序列、
//! 跨侧字段级 diff=0）里，本片补的是**拾取 / 丢弃**这两条（第 4、5 个动作）。
//!
//! # oracle 链（逐行对齐）
//!
//! ```text
//! ClientPickUpItem()                                  PlayObject.Base.cs:55
//!   ├─ Dealing ⇒ return
//!   ├─ Envir.GetItem(CurrX,CurrY, ref mapItem) 失败 ⇒ return（当前格子没有地面物品）
//!   ├─ (tick - mapItem.CanPickUpTick > Config.FloorItemCanPickUpTime) ⇒ mapItem.OfBaseObject = 0
//!   ├─ !IsSelf(OfBaseObject) && !IsOfGroup(OfBaseObject) ⇒ SysMsg(CanotPickUpItem,Red,Hint) return
//!   ├─ 金币分支（Name == Grobal2.StringGoldName，OrdinalIgnoreCase）：
//!   │    DeleteFromMap(...,Item,ItemId)==1 ?  IncGold(Count)
//!   │        ? SendRefMsg(RM_ITEMHIDE,…)（**广播**给附近玩家，不含自己）+ GoldChanged() + Dispose
//!   │        : Envir.AddItemToMap(CurrX,CurrY,mapItem)（放回）
//!   │    （DeleteFromMap != 1 ⇒ 什么也不做）
//!   └─ 普通物品分支：
//!        IsEnoughBag()（ItemList.Count < 46）? DeleteFromMap==1 ?
//!          stdItem=GetStdItem(userItem.Index)
//!            stdItem!=null && IsAddWeightAvailable(stdItem.Weight)
//!              ? SendMsg(RM_ITEMHIDE,…)（**给自己**）+ AddItemToBag(userItem)【内部 WeightChanged()】
//!                + Dispose(mapItem) + (Race==Play ? SendAddItem(userItem)⇒SM_ADDITEM)
//!              : Dispose(userItem) + Envir.AddItemToMap(CurrX,CurrY,mapItem)（放回）
//!            : Dispose(userItem) + Envir.AddItemToMap(CurrX,CurrY,mapItem)（模板缺失 ⇒ 放回）//!          : ——（DeleteFromMap != 1 ⇒ 什么也不做）
//!        : ——（背包满 ⇒ 什么也不做）
//!
//! ClientDropItem(sItemName, nItemIdx) -> bool        PlayObject.Operate.cs:251
//!   ├─ Config.InSafeDisableDrop && InSafeZone() ⇒ SendMsg(ManageNPC,RM_MENU_OK,…,ActorId,…) return false
//!   ├─ !IsCanDrop || Envir.Flag.NoThrowItem     ⇒ SendMsg(ManageNPC,RM_MENU_OK,…,ActorId,…) return false
//!   ├─ sItemName.IndexOf(' ') > 0 ⇒ HUtil32.GetValidStr3(sItemName, ref sItemName, ' ')
//!   ├─ (tick - DealLastTick) > 3000 :
//!   │    for item in ItemList with MakeIndex==nItemIdx:
//!   │      stdItem=GetStdItem(item.Index); stdItem==null ⇒ continue
//!   │      CustomItemSystem.GetItemName(item) == sItemName（OrdinalIgnoreCase）:
//!   │        Config.ControlDropItem && stdItem.Price < Config.CanDropPrice
//!   │          ⇒ Dispose + ItemList.RemoveAt(i) ⇒ result=true（**不进地图**）
//!   │        DropItemDown(item, MIN(Config.DropItemRage,3), false, 0, ActorId)
//!   │          ⇒ Dispose + ItemList.RemoveAt(i) ⇒ result=true
//!   │    若 result ⇒ WeightChanged()（重算 WAbil.Weight + RM_WEIGHTCHANGED）
//!   └─ return result（派发层据此回 SM_DROPITEM_SUCCESS/FAIL）
//!
//! DropItemDown(userItem, nScatterRange, …)            BaseObject.cs:529
//!   ├─ StdMode==40 ⇒ `ushort idura = (ushort)(Dura-2000)`（**原地**改 UserItem，失败也留下）
//!   │    ⚠️ `Dura`/`idura` 都是 `ushort` ⇒ Dura<2000 时**按 u16 回绕**（10 ⇒ 63546）；
//!   │    后面的 `if (idura <= 0) idura = 0;` 对 `ushort` **恒 false**（死代码）⇒ 不是 `max(0, …)`。
//!   │    Rust 用 `wrapping_sub(2000)` 1:1 复刻（`drop_stdmode40_dura_underflow` 用例钉住）。
//!   ├─ new MapItem { UserItem=copy, Name=GetItemName, Looks=StdItem.Looks, …; Count=1;
//!   │               OfBaseObject=itemOfCreat; CanPickUpTick=tick; DropBaseObject=dropCreat }
//!   ├─ StdMode==45 ⇒ Looks = GetRandomLook(Looks, Shape)（**本片未建模**，见「未做清单」）
//!   ├─ GetDropPosition(CurrX,CurrY,nScatterRange) → (dx,dy)（`Envir.GetItemEx` 扫格）
//!   └─ Envir.AddItemToMap(dx,dy,mapItem) ? SendRefMsg(RM_ITEMSHOW,…) true : false
//! ```
//!
//! # ⚠️ 本片实测到的 oracle 既有行为：正常 `CM_DROPITEM` 恒失败
//!
//! 全仓**没有任何** `MapItem.ItemId = …` 赋值点（`MapItem.cs:41` 的
//! `// this.ItemId = M2Share.ActorMgr.GetNextIdentity();` 是注释掉的），而
//! `Envir.AddItemToMap` 第一行就是 `if (mapItem.ItemId == 0) return false;`
//! （`Envirnoment.cs:88-92`）⇒ `DropItemDown` 造出的 `MapItem.ItemId` 恒 0 ⇒ 加地图恒失败 ⇒
//! `ClientDropItem` 对普通物品**恒**回 `SM_DROPITEM_FAIL`（背包不动）。这是 oracle 的既有形态，
//! **Rust 1:1 复刻**（不是本片引入的差异；登记见 whitelist T-19）。拾取则不受影响：地面物品是
//! 以「已存在于格子、且 `ItemId != 0`」为前提进入 `GetItem` 的。
//!
//! # 两类下行（本接缝把它们分开产出，别混成一条流）
//!
//! 1. **即时帧** `immediate_frames`：`SendDefMessage`→`SendSocket` 直接写 gate buffer：
//!    - 丢弃：恒 1 条 `SM_DROPITEM_SUCCESS`(600)/`SM_DROPITEM_FAIL`(601)，`Recog = nItemIdx`；
//!    - 拾取（普通物品成功、`Race==Play`）：1 条 `SM_ADDITEM`(200)，
//!      `Recog = ActorId`、`Series = 1`；其余 0 条。
//! 2. **延时通知** `notices`：`SendMsg`/`SendUpdateMsg` 只入 `MsgQueue`
//!    （`PriorityQueue<SendMessage,byte>`，优先级 = `(byte)wIdent`，**低值先出**）。
//!    本接缝**按 oracle 的 `MsgQueue` 出队序**产出，即按 `ident as u8` 升序稳定排序
//!    （拾取：`RM_WEIGHTCHANGED=10115`(131) 先于 `RM_ITEMHIDE=8085`(149)——
//!    **与入队序相反**，别按代码顺序写）。同一次动作里 ident 互不相同 ⇒ 顺序确定。
//!
//! ⚠️ 金币成功分支的 `SendRefMsg(RM_ITEMHIDE,…)` 是**广播给附近玩家**（`BaseObject.Message.cs:383`），
//! 生产形态（`FixedHideMode=false`）下**不含自己** ⇒ 不是 self notice；本接缝把它留给接线层
//! （与 `bag.rs` 的 `swapped_out` 同口径）。探针里已显式复位 `FixedHideMode` 以对齐生产路径。
//!
//! # 与 oracle 逐字段相同怎么保证
//!
//! 用例由 `mir2-rs/tests/parity/m3/ground-probe`（引用 `src/M2Server` 真身、反射调
//! `Operate(ProcessMessage)` 走**完整消息分支**，替身 `INetChannel` 截即时帧 + 反射读出队通知 +
//! 反射灌入 64×64 全 Walk 地图）实测产出，落 `mir2-rs/tests/parity/m3/fixtures/ground_vectors.json`；
//! 本模块用例回读逐字段核对（改坏任一映射 ⇒ 必红）。生成命令（**在仓库根运行**）：
//!
//! ```text
//! dotnet build mir2-rs/tests/parity/m3/ground-probe -c Release
//! dotnet mir2-rs/tests/parity/m3/ground-probe/bin/Release/net8.0/ground-probe.dll \
//!   mir2-rs/tests/parity/m3/fixtures/ground_vectors.json
//! cargo test -p game-svr --lib ground
//! ```
//!
//! # 未做清单（登记在卡 `openmir2-m3-bag-equip-alignment`；不是"静默跳过"）
//!
//! - **StdMode 45 掉落物的 `Looks` 随机化**（`M2Share.GetRandomLook`）：未建模；因为
//!   `AddItemToMap` 恒失败、`Looks` 不可观测，且它依赖全局 RNG（会破坏夹具确定性）。
//! - **`AddItemToMap` 的金币合并分支**（同名金币堆叠 + 容量 5 上限）：未建模（本片可达路径里
//!   不出现——金币放回时目标格子已空）。
//! - **`GetUpgradeStdItem` 的 Desc 升级差量**：同 `bag.rs`/`ability.rs`，传入模板视为「有效物品」。
//! - **`Envir.Flag.boNODRUG` 一类地图 Flag 的读取**：本接缝不建地图模型，`in_safe_zone`/
//!   `no_throw_item` 由接线层按其 `Envir` 状态传入。
//! - **`InSafeZone()` 的计算**：oracle 依赖 `Config.RedHomeMap/X/Y`、`SafeZoneSize`、`M2Share.StartPointList`；
//!   本接缝按调用方给的布尔量处理（探针用 `RedHome==当前图/坐标` 走 `else result=true` 分支）。
//! - **`ClientPickUpItemIsOfGroup` 的群组列表来源**：输入是 `group_member_actor_ids`（oracle 由
//!   `ActorMgr.Get(GroupOwner).GroupMembers` 取）；探针已构造团长演员覆盖此支。
//! - **非金币物品 `mapItem.UserItem == null`**：oracle 会 `userItem.Index` NRE（`PlayObject.Base.cs:101`）；
//!   Rust 走"放回"（不 panic）。线上不可达（非金币地面物品必带 UserItem）。登记 T-20。
//! - **收包接线**（把 `CM_PICKUP`/`CM_DROPITEM` 派发进世界循环）归 m2-svr 车道（`main.rs`/`smframe.rs`）。

use crate::ability::{Ability, ItemCatalog};
use crate::bag::{item_display_name, BagItem, Frame, MAX_BAG_ITEM};
use crate::useitem::QueuedNotice;

/// `Messages.CM_PICKUP`（`src/OpenMir2/Messages.cs:46`）。
pub const CM_PICKUP: u16 = 1001;
/// `Messages.CM_DROPITEM`（`:42`）。
pub const CM_DROPITEM: u16 = 1000;
/// `Messages.SM_ADDITEM`（`:220`）——拾取入包后发给自己的帧。
pub const SM_ADDITEM: u16 = 200;
/// `Messages.SM_DROPITEM_SUCCESS`（`:277`）。
pub const SM_DROPITEM_SUCCESS: u16 = 600;
/// `Messages.SM_DROPITEM_FAIL`（`:278`）。
pub const SM_DROPITEM_FAIL: u16 = 601;
/// `Messages.RM_ITEMHIDE`（`:688`）——地面物品消失（拾取成功给自己）。
pub const RM_ITEMHIDE: i32 = 8085;
/// `Messages.RM_WEIGHTCHANGED`（`:579`）。
pub const RM_WEIGHTCHANGED: i32 = 10115;
/// `Messages.RM_GOLDCHANGED`（`:596`）。
pub const RM_GOLDCHANGED: i32 = 10136;
/// `Messages.RM_MENU_OK`（`:625`）——丢弃守卫拒绝时的对话框提示。
pub const RM_MENU_OK: i32 = 10309;
/// `Messages.RM_SYSMESSAGE`（`:646`）——拾取归属不符时的红字提示。
pub const RM_SYSMESSAGE: i32 = 11004;

/// `Grobal2.StringGoldName`（`src/OpenMir2/Grobal2.cs:11`，原文是 GBK「金币」两字）。
pub const GOLD_NAME: &str = "金币";

/// `HUtil32.GetTickCount()` 的口径：单调毫秒；本接缝只用差值（`now_tick - x`），具体基点不重要。
///
/// `MapItem` 的 Rust 等价物（oracle `SystemModule.Data.MapItem`，只保留本片判据读到的字段）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroundItem {
    /// `MapItem.ItemId`（oracle 造物时**恒 0**；地面已有的物品由其它路径赋予非 0 值）。
    pub item_id: i32,
    pub name: String,
    pub looks: u16,
    pub ani_count: u8,
    pub reserved: i32,
    pub count: i32,
    pub drop_base_object: i32,
    pub of_base_object: i32,
    pub can_pick_up_tick: i32,
    pub user_item: Option<BagItem>,
}

impl GroundItem {
    /// 金币地面物（无 `UserItem`）。
    #[must_use]
    pub fn gold(item_id: i32, count: i32, of_base_object: i32, can_pick_up_tick: i32) -> Self {
        Self {
            item_id,
            name: GOLD_NAME.to_string(),
            looks: 110,
            ani_count: 0,
            reserved: 0,
            count,
            drop_base_object: 0,
            of_base_object,
            can_pick_up_tick,
            user_item: None,
        }
    }
}

/// 地图格子（`MapCellInfo.ObjList` 的极简等价物：一格一个 `Vec<GroundItem>`）。
///
/// 只实现本接缝需要的语义（`GetItem`/`DeleteFromMap`/`AddItemToMap`/`GetItemEx`/`CellMatch`）。
/// **不含**地图文件解析、门、事件、行走判定——那些属 `crates/envir`/`crates/world`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroundMap {
    pub width: i32,
    pub height: i32,
    cells: Vec<Vec<GroundItem>>,
}

impl GroundMap {
    #[must_use]
    pub fn new(width: i32, height: i32) -> Self {
        let n = (width.max(0) * height.max(0)) as usize;
        Self {
            width,
            height,
            cells: vec![Vec::new(); n],
        }
    }

    fn idx(&self, x: i32, y: i32) -> Option<usize> {
        if x >= 0 && x < self.width && y >= 0 && y < self.height {
            Some((x * self.height + y) as usize)
        } else {
            None
        }
    }

    /// `Envirnoment.CellMatch(nX, nY)`（`Envirnoment.cs:316`）。
    #[must_use]
    pub fn cell_match(&self, x: i32, y: i32) -> bool {
        self.idx(x, y).is_some()
    }

    /// `Envirnoment.GetItem(nX, nY, ref mapItem)`（`:705`）——返回该格**第一个**物品。
    #[must_use]
    pub fn get_item(&self, x: i32, y: i32) -> Option<&GroundItem> {
        let i = self.idx(x, y)?;
        self.cells[i].first()
    }

    /// `Envirnoment.GetItemEx`（`:723`）→ `(CellObjId, Count, ChFlag)`。
    ///
    /// `ChFlag` 在格子越界/不可用/有活的演员时为 false；本模型格子恒可用、且不放演员。
    #[must_use]
    pub fn get_item_ex(&self, x: i32, y: i32) -> (i32, i32, bool) {
        match self.idx(x, y) {
            None => (0, 0, false),
            Some(i) => {
                let cell = &self.cells[i];
                let mut id = 0;
                for it in cell {
                    id = it.item_id;
                }
                (id, cell.len() as i32, true)
            }
        }
    }

    /// `Envirnoment.DeleteFromMap(..., CellType.Item, itemId, null)`（`:635`）——返回值只有 `==1` 有意义。
    pub fn delete_item(&mut self, x: i32, y: i32, item_id: i32) -> i32 {
        if let Some(i) = self.idx(x, y) {
            if let Some(pos) = self.cells[i]
                .iter()
                .position(|it| it.item_id != 0 && it.item_id == item_id)
            {
                self.cells[i].remove(pos);
                return 1;
            }
        }
        0
    }

    /// `Envirnoment.AddItemToMap(nX, nY, mapItem)`（`:88`）。
    ///
    /// oracle 第一行就是 `if (mapItem.ItemId == 0) return false;`——**丢弃路径永远命中它**
    /// （见模块头「正常 CM_DROPITEM 恒失败」）。金币合并/容量上限分支未建模（见未做清单）。
    pub fn put_item(&mut self, x: i32, y: i32, item: GroundItem) -> bool {
        if item.item_id == 0 {
            return false;
        }
        match self.idx(x, y) {
            None => false,
            Some(i) => {
                self.cells[i].push(item);
                true
            }
        }
    }

    /// 全图物品数（判据用：证明丢弃没有凭空进地图、拾取放回/删除的净效果）。
    #[must_use]
    pub fn item_count(&self) -> usize {
        self.cells.iter().map(Vec::len).sum()
    }
}

/// [`pick_up`] 会原地改的演员侧状态（`PlayObject` 上分散的字段；接缝把它们打包传入传出）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroundState {
    /// `PlayObject.ItemList`（背包，顺序敏感）。
    pub item_list: Vec<BagItem>,
    /// `BaseObject.Gold`。
    pub gold: i32,
    /// `BaseObject.WAbil`（本片只读 `Weight`/`MaxWeight`，写 `Weight`）。
    pub wabil: Ability,
}

/// [`pick_up`] 的观测输入（`ClientPickUpItem` + `ClientPickUpItemIsSelf/IsOfGroup` 读到的外部状态）。
#[derive(Clone, Copy)]
pub struct PickUpContext<'a> {
    /// `SystemShare.ItemSystem.GetStdItem`。
    pub catalog: &'a dyn ItemCatalog,
    /// `PlayObject.ActorId`。
    pub actor_id: i32,
    /// `BaseObject.CurrX`。
    pub curr_x: i32,
    /// `BaseObject.CurrY`。
    pub curr_y: i32,
    /// `HUtil32.GetTickCount()` 的当轮值。
    pub now_tick: i32,
    /// `PlayObject.Dealing`。
    pub dealing: bool,
    /// `SystemShare.Config.FloorItemCanPickUpTime`（默认 `2*60*1000`）。
    pub floor_item_can_pick_up_time: i32,
    /// `SystemShare.Config.HumanMaxGold`。
    pub human_max_gold: i32,
    /// `BaseObject.Race == ActorRace.Play`（`SendAddItem` 仅在玩家时发）。
    pub race_is_play: bool,
    /// `PlayObject.GroupOwner`（0 = 无组 ⇒ 群组分支恒 false）。
    pub group_owner: i32,
    /// `ActorMgr.Get(GroupOwner).GroupMembers` 的 `ActorId` 列表（团长自身/成员都算）。
    pub group_member_actor_ids: &'a [i32],
    /// `Config.RedMsgFColor`（`SysMsg(...,Red,Hint)` 的 `nParam1`）。
    pub red_msg_f_color: u16,
    /// `Config.RedMsgBColor`（同上的 `nParam2`）。
    pub red_msg_b_color: u16,
}

/// [`pick_up`] 的结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PickUpOutcome {
    /// 立即下发的帧（普通物品成功且 `Race==Play` ⇒ 1 条 `SM_ADDITEM`；否则 0 条）。
    pub immediate_frames: Vec<Frame>,
    /// 成功拾取并入包的物品（`SM_ADDITEM` 的 `ClientItem` 尾输入）。
    /// `None` = 非普通物品成功路径（金币/失败/放回）。
    pub added_item: Option<BagItem>,
    /// 进 `MsgQueue` 的延时通知（**按出队序**，见模块头）。
    pub notices: Vec<QueuedNotice>,
}

/// `ClientPickUpItemIsSelf(actorId)`（`PlayObject.Base.cs:24`）。
#[must_use]
pub fn is_self(self_actor_id: i32, actor_id: i32) -> bool {
    actor_id == 0 || self_actor_id == actor_id
}

/// `ClientPickUpItemIsOfGroup(actorId)`（`PlayObject.Base.cs:38`）。
///
/// oracle 取 `GroupOwner` 的 `GroupMembers` 逐个比 `ActorId`；本接缝拿的是那份列表的 ActorId。
#[must_use]
pub fn is_of_group(group_owner: i32, group_member_actor_ids: &[i32], actor_id: i32) -> bool {
    if group_owner == 0 {
        return false;
    }
    group_member_actor_ids.contains(&actor_id)
}

/// `PlayObject.IncGold(int tGold)`（`PlayObject.Base.cs:168`）。
#[must_use]
pub fn inc_gold(gold: i32, t_gold: i32, human_max_gold: i32) -> bool {
    gold + t_gold <= human_max_gold
}

/// `BaseObject.IsAddWeightAvailable(int nWeight)`（`:193`）。
#[must_use]
pub fn is_add_weight_available(wabil: &Ability, n_weight: i32) -> bool {
    i32::from(wabil.weight) + n_weight <= i32::from(wabil.max_weight)
}

/// `BaseObject.RecalcBagWeight()`（`BaseObject.cs:1356`）：Σ `GetStdItem(bag[i].Index).Weight`，`ushort` 加。
#[must_use]
pub fn recalc_bag_weight(catalog: &dyn ItemCatalog, item_list: &[BagItem]) -> u16 {
    let mut result: u16 = 0;
    for it in item_list {
        if let Some(tpl) = catalog.get(it.index) {
            result = result.wrapping_add(u16::from(tpl.weight));
        }
    }
    result
}

fn notice(ident: i32, w_param: i32, n_param1: i32, n_param2: i32, n_param3: i32) -> QueuedNotice {
    QueuedNotice {
        ident,
        w_param,
        n_param1,
        n_param2,
        n_param3,
    }
}

/// 把一条 `SendMsg(this, ident, 0,0,0, "")` 入队（无 `SendUpdateMsg` 的去重语义）。
fn plain(ident: i32) -> QueuedNotice {
    notice(ident, 0, 0, 0, 0)
}

/// `SendUpdateMsg(wIdent, …)`：先移除队列里同 `wIdent` 的旧消息，再入队。
///
/// 本接缝的 `notices` 是「出队后」的形态，故先在同 ident 上做去重（`MsgQueue` 的可见语义）。
fn update(notices: &mut Vec<QueuedNotice>, ident: i32) {
    notices.retain(|n| n.ident != ident);
    notices.push(plain(ident));
}

/// 按 oracle `MsgQueue` 的**出队序**（优先级 = `(byte)wIdent`，低值先出）稳定排序。
fn dequeue_order(mut notices: Vec<QueuedNotice>) -> Vec<QueuedNotice> {
    notices.sort_by_key(|n| (n.ident as u8) as u16);
    notices
}

/// `PlayObject.ClientPickUpItem()`（`PlayObject.Base.cs:55`）的纯函数等价物。
///
/// 原地改 `state`（`item_list`/`gold`/`wabil`）与 `map`（删除/放回地面物品）。
pub fn pick_up(
    ctx: &PickUpContext<'_>,
    map: &mut GroundMap,
    state: &mut GroundState,
) -> PickUpOutcome {
    let mut notices: Vec<QueuedNotice> = Vec::new();
    let mut immediate_frames: Vec<Frame> = Vec::new();
    let mut added_item: Option<BagItem> = None;

    if ctx.dealing {
        return PickUpOutcome {
            immediate_frames,
            notices,
            added_item: None,
        };
    }

    let Some(mut ground) = map.get_item(ctx.curr_x, ctx.curr_y).cloned() else {
        return PickUpOutcome {
            immediate_frames,
            notices,
            added_item: None,
        };
    };

    if ctx.now_tick - ground.can_pick_up_tick > ctx.floor_item_can_pick_up_time {
        ground.of_base_object = 0;
    }

    if !is_self(ctx.actor_id, ground.of_base_object)
        && !is_of_group(
            ctx.group_owner,
            ctx.group_member_actor_ids,
            ground.of_base_object,
        )
    {
        notices.push(notice(
            RM_SYSMESSAGE,
            0,
            i32::from(ctx.red_msg_f_color),
            i32::from(ctx.red_msg_b_color),
            0,
        ));
        return PickUpOutcome {
            immediate_frames,
            notices: dequeue_order(notices),
            added_item: None,
        };
    }

    if ground.name.eq_ignore_ascii_case(GOLD_NAME) {
        if map.delete_item(ctx.curr_x, ctx.curr_y, ground.item_id) == 1 {
            if inc_gold(state.gold, ground.count, ctx.human_max_gold) {
                state.gold += ground.count;
                // `SendRefMsg(RM_ITEMHIDE, …)` 是广播（不含自己），留给接线层。
                update(&mut notices, RM_GOLDCHANGED);
            } else {
                let _ = map.put_item(ctx.curr_x, ctx.curr_y, ground);
            }
        }
        return PickUpOutcome {
            immediate_frames,
            notices: dequeue_order(notices),
            added_item: None,
        };
    }

    if state.item_list.len() < MAX_BAG_ITEM
        && map.delete_item(ctx.curr_x, ctx.curr_y, ground.item_id) == 1
    {
        // oracle：`UserItem userItem = mapItem.UserItem; StdItem stdItem = GetStdItem(userItem.Index);`
        // 非金币地面物品必带 UserItem；缺失时 oracle 会 NRE，Rust 走"放回"（登记 T-20）。
        let user_item = ground.user_item.clone();
        let is_addable = user_item
            .as_ref()
            .and_then(|u| ctx.catalog.get(u.index))
            .is_some_and(|tpl| is_add_weight_available(&state.wabil, i32::from(tpl.weight)));

        if is_addable {
            let u = user_item.expect("is_addable 蕴含 user_item 为 Some");
            notices.push(notice(
                RM_ITEMHIDE,
                0,
                ground.item_id,
                ctx.curr_x,
                ctx.curr_y,
            ));
            // `AddItemToBag`：满包不追加、也不 WeightChanged；本分支已过 `IsEnoughBag`。
            if state.item_list.len() < MAX_BAG_ITEM {
                // `SendAddItem(userItem)` 的尾需要**实际入包**的 `UserItem`（G-1）。
                if ctx.race_is_play {
                    added_item = Some(u.clone());
                }
                state.item_list.push(u);
                state.wabil.weight = recalc_bag_weight(ctx.catalog, &state.item_list);
                update(&mut notices, RM_WEIGHTCHANGED);
            }
            if ctx.race_is_play {
                immediate_frames.push(Frame {
                    ident: SM_ADDITEM,
                    recog: ctx.actor_id,
                    param: 0,
                    tag: 0,
                    series: 1,
                });
            }
        } else {
            let _ = map.put_item(ctx.curr_x, ctx.curr_y, ground);
        }
    }

    PickUpOutcome {
        immediate_frames,
        notices: dequeue_order(notices),
        added_item,
    }
}

/// [`drop_item`] 的观测输入（`ClientDropItem`/`DropItemDown`/`GetDropPosition` 读到的外部状态）。
#[derive(Clone, Copy)]
pub struct DropContext<'a> {
    /// `SystemShare.ItemSystem.GetStdItem`。
    pub catalog: &'a dyn ItemCatalog,
    /// `PlayObject.ActorId`（`DropItemDown(..., this.ActorId)` 的 `itemOfCreat` 与日志用）。
    pub actor_id: i32,
    /// `BaseObject.CurrX`。
    pub curr_x: i32,
    /// `BaseObject.CurrY`。
    pub curr_y: i32,
    /// `HUtil32.GetTickCount()` 的当轮值。
    pub now_tick: i32,
    /// `PlayObject.DealLastTick`。
    pub deal_last_tick: i32,
    /// `CM_DROPITEM` 的 `nParam1`（要丢的物品 `MakeIndex`）。
    pub make_index: i32,
    /// `CM_DROPITEM` 的 `Msg`（物品名；含空格时按 `GetValidStr3` 折分）。
    pub item_name: &'a str,
    /// `SystemShare.Config.InSafeDisableDrop`。
    pub in_safe_disable_drop: bool,
    /// `InSafeZone()` 的结果（由接线层按其 `Envir`/`Config` 算）。
    pub in_safe_zone: bool,
    /// `PlayObject.IsCanDrop`。
    pub is_can_drop: bool,
    /// `Envir.Flag.NoThrowItem`。
    pub no_throw_item: bool,
    /// `SystemShare.Config.ControlDropItem`。
    pub control_drop_item: bool,
    /// `SystemShare.Config.CanDropPrice`。
    pub can_drop_price: i32,
    /// `SystemShare.Config.DropItemRage`（实际散布半径取 `MIN(…,3)`）。
    pub drop_item_rage: u8,
}

/// [`drop_item`] 的结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DropOutcome {
    /// oracle `ClientDropItem` 的返回值（派发层据此选 `SM_DROPITEM_SUCCESS/FAIL`）。
    pub success: bool,
    /// 立即下发的帧（恒 1 条）。
    pub immediate_frames: Vec<Frame>,
    /// 进 `MsgQueue` 的延时通知（按出队序）。
    pub notices: Vec<QueuedNotice>,
}

/// `BaseObject.GetDropPosition(nOrgX, nOrgY, nRange, ref pX, ref pY)`（`BaseObject.cs:470`）。
fn get_drop_position(map: &GroundMap, org_x: i32, org_y: i32, range: i32) -> (i32, i32) {
    let mut n24 = 999i32;
    let (mut n28, mut n2c) = (0i32, 0i32);
    for i in 0..range {
        for ii in -i..=i {
            for iii in -i..=i {
                let px = org_x + iii + 1;
                let py = org_y + ii + 1;
                let (id, count, ch_flag) = map.get_item_ex(px, py);
                if id == 0 {
                    if ch_flag {
                        return (px, py);
                    }
                } else if ch_flag && n24 > count {
                    n24 = count;
                    n28 = px;
                    n2c = py;
                }
            }
        }
    }
    if n24 < 8 {
        (n28, n2c)
    } else {
        (org_x, org_y)
    }
}

/// `BaseObject.DropItemDown(userItem, nScatterRange, boDieDrop, itemOfCreat, dropCreat)`（`BaseObject.cs:529`）。
///
/// 返回 false ⇔ `Envir.AddItemToMap` 失败。**oracle 造出的 `MapItem.ItemId` 恒 0**（模块头）⇒
/// 本函数在本仓当前 oracle 下**恒返回 false**；`looks`/`get_drop_position` 仍按真身走一遍，
/// 以便 oracle 一旦补上 `ItemId` 赋值时这里的语义自然跟上。
fn drop_item_down(
    ctx: &DropContext<'_>,
    map: &mut GroundMap,
    user_item: &BagItem,
    item_of_creat: i32,
    drop_creat: i32,
    scatter_range: i32,
) -> bool {
    let Some(tpl) = ctx.catalog.get(user_item.index) else {
        return false;
    };
    // `StdMode 45` 的 `GetRandomLook` 未建模（未做清单）；`AddItemToMap` 恒失败使其不可观测。
    let map_item = GroundItem {
        // ⚠️ oracle 无任何 `MapItem.ItemId = …` 赋值点 ⇒ 恒 0（`MapItem.cs:41` 的赋值被注释）。
        item_id: 0,
        name: item_display_name(ctx.catalog, user_item),
        looks: tpl.looks,
        ani_count: tpl.ani_count,
        reserved: 0,
        count: 1,
        drop_base_object: drop_creat,
        of_base_object: item_of_creat,
        can_pick_up_tick: ctx.now_tick,
        user_item: Some(user_item.clone()),
    };
    let (dx, dy) = get_drop_position(map, ctx.curr_x, ctx.curr_y, scatter_range);
    if map.put_item(dx, dy, map_item) {
        // oracle 成功分支 `SendRefMsg(RM_ITEMSHOW, …)` 是广播（不含自己）⇒ 不是 self notice。
        true
    } else {
        false
    }
}

/// `PlayObject.ClientDropItem(sItemName, nItemIdx)`（`PlayObject.Operate.cs:251`）的纯函数等价物。
///
/// 原地改 `state.item_list`（成功时移除）与 `state.wabil.weight`（成功时 `WeightChanged`）。
pub fn drop_item(
    ctx: &DropContext<'_>,
    map: &mut GroundMap,
    state: &mut GroundState,
) -> DropOutcome {
    let mut notices: Vec<QueuedNotice> = Vec::new();

    if ctx.in_safe_disable_drop && ctx.in_safe_zone {
        notices.push(notice(RM_MENU_OK, 0, ctx.actor_id, 0, 0));
        return DropOutcome {
            success: false,
            immediate_frames: vec![fail_frame(ctx.make_index)],
            notices: dequeue_order(notices),
        };
    }
    if !ctx.is_can_drop || ctx.no_throw_item {
        notices.push(notice(RM_MENU_OK, 0, ctx.actor_id, 0, 0));
        return DropOutcome {
            success: false,
            immediate_frames: vec![fail_frame(ctx.make_index)],
            notices: dequeue_order(notices),
        };
    }

    let mut s_item_name = ctx.item_name;
    if let Some(pos) = ctx.item_name.find(' ') {
        if pos > 0 {
            // `HUtil32.GetValidStr3(source, ref dest, ' ')`：dest = Split(' ',2,RemoveEmptyEntries)[0]
            s_item_name = ctx
                .item_name
                .split(' ')
                .find(|s| !s.is_empty())
                .unwrap_or("");
        }
    }

    let mut result = false;
    if ctx.now_tick - ctx.deal_last_tick > 3000 {
        let mut i = 0usize;
        while i < state.item_list.len() {
            let mut item = state.item_list[i].clone();
            if item.make_index == ctx.make_index {
                if let Some(tpl) = ctx.catalog.get(item.index) {
                    if item_display_name(ctx.catalog, &item).eq_ignore_ascii_case(s_item_name) {
                        if ctx.control_drop_item && tpl.price < ctx.can_drop_price {
                            state.item_list.remove(i);
                            result = true;
                            break;
                        }
                        // `DropItemDown` 的 StdMode 40 耐久扣减作用在 ItemList 的**同一引用**上
                        //（失败也留下）。`BaseObject.cs:541-548`：`ushort idura = (ushort)(Dura-2000)`
                        // ⇒ **按 u16 回绕**（Dura<2000 ⇒ 大正数），后面的 `idura <= 0` 对 `ushort`
                        // **恒 false**（死代码）⇒ 不是 `max(0, Dura-2000)`。
                        if tpl.std_mode == 40 {
                            let nd = item.dura.wrapping_sub(2000);
                            item.dura = nd;
                            state.item_list[i].dura = nd;
                        }
                        let scatter = i32::from(ctx.drop_item_rage).min(3);
                        // `PlayObject.Operate.cs:251`：`DropItemDown(userItem, dropWide, false, 0, ActorId)`
                        // ⇒ itemOfCreat=0（`mapItem.OfBaseObject`），dropCreat=ActorId。
                        if drop_item_down(ctx, map, &item, 0, ctx.actor_id, scatter) {
                            state.item_list.remove(i);
                            result = true;
                            break;
                        }
                    }
                }
            }
            i += 1;
        }
        if result {
            state.wabil.weight = recalc_bag_weight(ctx.catalog, &state.item_list);
            update(&mut notices, RM_WEIGHTCHANGED);
        }
    }

    let frame = if result {
        Frame {
            ident: SM_DROPITEM_SUCCESS,
            recog: ctx.make_index,
            param: 0,
            tag: 0,
            series: 0,
        }
    } else {
        fail_frame(ctx.make_index)
    };
    DropOutcome {
        success: result,
        immediate_frames: vec![frame],
        notices: dequeue_order(notices),
    }
}

fn fail_frame(make_index: i32) -> Frame {
    Frame {
        ident: SM_DROPITEM_FAIL,
        recog: make_index,
        param: 0,
        tag: 0,
        series: 0,
    }
}

/// **PL-1 / P21**：`PlayObject.Message.cs:421-426` 的
/// `DropItemDown(UseItems[i], MIN(SystemShare.Config.DropItemRage, 3), false, 0, ActorId)`。
///
/// 与 [`drop_item`] 的丢弃路径共用 [`drop_item_down`] 的核心：
/// 1. `StdMode == 40` 先做**原地**耐久扣减（`BaseObject.cs:541-548`，`ushort` 回绕；失败也留下）；
/// 2. 返回值照 oracle 的 `Envir.AddItemToMap` —— `MapItem.ItemId` 恒 0（`T-19` 登记）
///    ⇒ **恒 false**，即 P21 的「背包满 ⇒ 掉落」分支在当前 oracle 树上**永远失败**，
///    物品留在装备位（P21 的 `boTakeItem` 也因此不置位）。
///
/// 这里传一张 0×0 的 [`GroundMap`]：`get_drop_position` 全部越界 ⇒ 落回原坐标，
/// `put_item` 在 `item_id == 0` 的第一行即返回 false——与真实地图上的结果相同，
/// 且不依赖宿主地图尺寸。
#[must_use]
pub fn drop_item_down_p21(
    catalog: &dyn ItemCatalog,
    item: &mut BagItem,
    std_mode: u8,
    drop_item_rage: u8,
    actor_id: i32,
) -> bool {
    // `BaseObject.cs:539-548`：`ushort idura = (ushort)(Dura - 2000)`；`if (idura <= 0)` 对
    // `ushort` 是 `== 0`（死代码分支），净效果 = `wrapping_sub(2000)`。
    if std_mode == 40 {
        item.dura = item.dura.wrapping_sub(2000);
    }
    let ctx = DropContext {
        catalog,
        actor_id,
        curr_x: 0,
        curr_y: 0,
        now_tick: 0,
        deal_last_tick: 0,
        make_index: item.make_index,
        item_name: "",
        in_safe_disable_drop: false,
        in_safe_zone: false,
        is_can_drop: true,
        no_throw_item: false,
        control_drop_item: false,
        can_drop_price: 0,
        drop_item_rage,
    };
    let mut map = GroundMap::new(0, 0);
    drop_item_down(
        &ctx,
        &mut map,
        item,
        0,
        actor_id,
        i32::from(drop_item_rage).min(3),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ability::{Ability, StdItemCatalog};
    use mir2_data::StdItem;
    use serde_json::Value;

    fn fixture() -> Value {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/parity/m3/fixtures/ground_vectors.json"
        );
        let raw = std::fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("oracle 向量夹具缺失/不可读：{path}（{e}）—— 不许静默跳过"));
        serde_json::from_str(&raw).unwrap_or_else(|e| panic!("{path} 不是合法 JSON：{e}"))
    }

    fn u8v(v: &Value) -> u8 {
        v.as_u64().unwrap() as u8
    }
    fn u16v(v: &Value) -> u16 {
        v.as_u64().unwrap() as u16
    }
    fn i32v(v: &Value) -> i32 {
        v.as_i64().unwrap() as i32
    }

    fn template(v: &Value) -> StdItem {
        StdItem {
            name: v["name"].as_str().unwrap().to_string(),
            std_mode: u8v(&v["std_mode"]),
            shape: u8v(&v["shape"]),
            weight: u8v(&v["weight"]),
            price: i32v(&v["price"]),
            looks: u16v(&v["looks"]),
            ani_count: u8v(&v["ani_count"]),
            dura_max: u16v(&v["dura_max"]),
            ..StdItem::default()
        }
    }

    fn catalog(doc: &Value) -> StdItemCatalog {
        let mut c = StdItemCatalog::new();
        for (k, v) in doc["templates"].as_object().unwrap() {
            c.insert(k.parse::<u16>().unwrap(), template(v));
        }
        c
    }

    fn ability(v: &Value) -> Ability {
        Ability {
            level: u8v(&v["level"]),
            reserved1: u8v(&v["reserved1"]),
            ac: u16v(&v["ac"]),
            mac: u16v(&v["mac"]),
            dc: u16v(&v["dc"]),
            mc: u16v(&v["mc"]),
            sc: u16v(&v["sc"]),
            hp: u16v(&v["hp"]),
            mp: u16v(&v["mp"]),
            max_hp: u16v(&v["max_hp"]),
            max_mp: u16v(&v["max_mp"]),
            reserved2: v["reserved2"].as_i64().unwrap() as i16,
            exp_count: u8v(&v["exp_count"]),
            exp_max_count: u8v(&v["exp_max_count"]),
            exp: i32v(&v["exp"]),
            max_exp: i32v(&v["max_exp"]),
            weight: u16v(&v["weight"]),
            max_weight: u16v(&v["max_weight"]),
            wear_weight: u8v(&v["wear_weight"]),
            max_wear_weight: u8v(&v["max_wear_weight"]),
            hand_weight: u8v(&v["hand_weight"]),
            max_hand_weight: u8v(&v["max_hand_weight"]),
        }
    }

    fn bag_item(v: &Value) -> BagItem {
        let mut desc = [0u8; 14];
        for (i, d) in v["desc"].as_array().unwrap().iter().enumerate() {
            desc[i] = u8v(d);
        }
        BagItem {
            make_index: i32v(&v["make_index"]),
            index: u16v(&v["index"]),
            dura: u16v(&v["dura"]),
            dura_max: u16v(&v["dura_max"]),
            desc,
        }
    }

    fn bag_items(v: &Value) -> Vec<BagItem> {
        v.as_array().unwrap().iter().map(bag_item).collect()
    }

    fn frame(v: &Value) -> Frame {
        Frame {
            ident: u16v(&v["ident"]),
            recog: i32v(&v["recog"]),
            param: u16v(&v["param"]),
            tag: u16v(&v["tag"]),
            series: u16v(&v["series"]),
        }
    }

    fn frames(v: &Value) -> Vec<Frame> {
        v.as_array().unwrap().iter().map(frame).collect()
    }

    fn notice(v: &Value) -> QueuedNotice {
        QueuedNotice {
            ident: i32v(&v["ident"]),
            w_param: i32v(&v["w_param"]),
            n_param1: i32v(&v["n_param1"]),
            n_param2: i32v(&v["n_param2"]),
            n_param3: i32v(&v["n_param3"]),
        }
    }

    fn notices(v: &Value) -> Vec<QueuedNotice> {
        v.as_array().unwrap().iter().map(notice).collect()
    }

    fn ground(v: &Value) -> Option<GroundItem> {
        if v.is_null() {
            return None;
        }
        Some(GroundItem {
            item_id: i32v(&v["item_id"]),
            name: v["name"].as_str().unwrap().to_string(),
            looks: u16v(&v["looks"]),
            ani_count: u8v(&v["ani_count"]),
            reserved: i32v(&v["reserved"]),
            count: i32v(&v["count"]),
            drop_base_object: i32v(&v["drop_base_object"]),
            of_base_object: i32v(&v["of_base_object"]),
            can_pick_up_tick: i32v(&v["can_pick_up_tick"]),
            user_item: if v["user_item"].is_null() {
                None
            } else {
                Some(bag_item(&v["user_item"]))
            },
        })
    }

    fn build_map(case: &Value) -> GroundMap {
        let c = &case["ctx"];
        let mut map = GroundMap::new(64, 64);
        if let Some(g) = ground(&case["ground_before"]) {
            assert!(
                map.put_item(i32v(&c["curr_x"]), i32v(&c["curr_y"]), g),
                "夹具地面物品必须能放进地图"
            );
        }
        map
    }

    /// 每条用例：即时帧、延时通知（含**出队序**）、`ItemList`、`Gold`、`WAbil`、格子前后状态、
    /// 全图物品数都必须与 oracle 逐字段相同。
    #[test]
    fn matches_oracle_ground_fixture() {
        let doc = fixture();
        let cat = catalog(&doc);
        let cases = doc["cases"].as_array().expect("cases 数组");
        assert!(cases.len() >= 18, "夹具用例数异常：{}", cases.len());
        for case in cases {
            let name = case["name"].as_str().unwrap();
            let c = &case["ctx"];
            let member_ids: Vec<i32> = c["group_member_actor_ids"]
                .as_array()
                .map(|a| a.iter().map(i32v).collect())
                .unwrap_or_default();
            let mut map = build_map(case);
            let mut state = GroundState {
                item_list: bag_items(&case["item_list_before"]),
                gold: i32v(&case["gold_before"]),
                wabil: ability(&case["wabil_before"]),
            };
            let expect = &case["expect"];

            let (got_frames, got_notices) = match case["action"].as_str().unwrap() {
                "pick_up" => {
                    let ctx = PickUpContext {
                        catalog: &cat,
                        actor_id: i32v(&c["actor_id"]),
                        curr_x: i32v(&c["curr_x"]),
                        curr_y: i32v(&c["curr_y"]),
                        now_tick: i32v(&case["now_tick"]),
                        dealing: c["dealing"].as_bool().unwrap(),
                        floor_item_can_pick_up_time: i32v(&c["floor_item_can_pick_up_time"]),
                        human_max_gold: i32v(&c["human_max_gold"]),
                        race_is_play: true,
                        group_owner: i32v(&c["group_owner"]),
                        group_member_actor_ids: &member_ids,
                        red_msg_f_color: u16v(&c["red_msg_f_color"]),
                        red_msg_b_color: u16v(&c["red_msg_b_color"]),
                    };
                    let out = pick_up(&ctx, &mut map, &mut state);
                    (out.immediate_frames, out.notices)
                }
                "drop_item" => {
                    let ctx = DropContext {
                        catalog: &cat,
                        actor_id: i32v(&c["actor_id"]),
                        curr_x: i32v(&c["curr_x"]),
                        curr_y: i32v(&c["curr_y"]),
                        now_tick: i32v(&case["now_tick"]),
                        deal_last_tick: i32v(&c["deal_last_tick"]),
                        make_index: i32v(&case["request"]["make_index"]),
                        in_safe_disable_drop: c["in_safe_disable_drop"].as_bool().unwrap(),
                        // 探针把 `RedHomeMap/X/Y/SafeZoneSize` 配成「当前图/坐标」⇒ 只要
                        // `Flag.SafeArea` 为真，`InSafeZone()` 就为真。
                        in_safe_zone: c["safe_area"].as_bool().unwrap(),
                        is_can_drop: c["is_can_drop"].as_bool().unwrap(),
                        no_throw_item: c["no_throw_item"].as_bool().unwrap(),
                        control_drop_item: c["control_drop_item"].as_bool().unwrap(),
                        can_drop_price: i32v(&c["can_drop_price"]),
                        drop_item_rage: u8v(&c["drop_item_rage"]),
                        item_name: case["request"]["name"].as_str().unwrap(),
                    };
                    let out = drop_item(&ctx, &mut map, &mut state);
                    (out.immediate_frames, out.notices)
                }
                other => panic!("未知 action：{other}"),
            };

            assert_eq!(
                got_frames,
                frames(&expect["immediate_frames"]),
                "{name}: 即时帧与 oracle 不符"
            );
            assert_eq!(
                got_notices,
                notices(&expect["notices"]),
                "{name}: 延时通知（出队序）与 oracle 不符"
            );
            assert_eq!(
                state.item_list,
                bag_items(&expect["item_list"]),
                "{name}: ItemList 与 oracle 不符"
            );
            assert_eq!(state.gold, i32v(&expect["gold"]), "{name}: Gold");
            assert_eq!(
                state.wabil,
                ability(&expect["wabil"]),
                "{name}: WAbil 与 oracle 不符"
            );
            assert_eq!(
                map.get_item(i32v(&c["curr_x"]), i32v(&c["curr_y"])),
                ground(&expect["ground_after"]).as_ref(),
                "{name}: 地图格子前后状态与 oracle 不符"
            );
            assert_eq!(
                map.item_count(),
                expect["map_item_count"].as_u64().unwrap() as usize,
                "{name}: 全图物品数与 oracle 不符"
            );
        }
    }

    /// **oracle 既有行为钉住**：`MapItem.ItemId` 从无赋值点 ⇒ `AddItemToMap` 首行即 false ⇒
    /// 普通丢弃恒 `SM_DROPITEM_FAIL`、物品留在背包。改坏 `put_item` 的 `item_id == 0` 守卫，
    /// 或给 `drop_item_down` 的 `item_id` 填非 0，本用例必红。
    #[test]
    fn drop_fails_because_oracle_map_item_id_is_always_zero() {
        let doc = fixture();
        let cat = catalog(&doc);
        let case = doc["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["name"] == "drop_fail_item_id_zero")
            .expect("夹具必须含 drop_fail_item_id_zero");
        let c = &case["ctx"];
        let mut map = build_map(case);
        let mut state = GroundState {
            item_list: bag_items(&case["item_list_before"]),
            gold: i32v(&case["gold_before"]),
            wabil: ability(&case["wabil_before"]),
        };
        let before = state.item_list.clone();
        let ctx = DropContext {
            catalog: &cat,
            actor_id: i32v(&c["actor_id"]),
            curr_x: i32v(&c["curr_x"]),
            curr_y: i32v(&c["curr_y"]),
            now_tick: i32v(&case["now_tick"]),
            deal_last_tick: i32v(&c["deal_last_tick"]),
            make_index: 51,
            in_safe_disable_drop: false,
            in_safe_zone: false,
            is_can_drop: true,
            no_throw_item: false,
            control_drop_item: false,
            can_drop_price: 500,
            drop_item_rage: 3,
            item_name: "ring",
        };
        let out = drop_item(&ctx, &mut map, &mut state);
        assert!(!out.success, "ItemId 恒 0 ⇒ 丢弃必失败");
        assert_eq!(out.immediate_frames[0].ident, SM_DROPITEM_FAIL);
        assert_eq!(out.immediate_frames[0].recog, 51);
        assert_eq!(state.item_list, before, "失败不许动背包");
        assert_eq!(map.item_count(), 0, "失败不许在地图上留物");
        // 直接钉 `put_item` 的守卫。
        let mut m2 = GroundMap::new(8, 8);
        assert!(!m2.put_item(1, 1, GroundItem::gold(0, 100, 0, 0)));
        assert!(m2.put_item(1, 1, GroundItem::gold(7, 100, 0, 0)));
    }

    /// **StdMode 40 的耐久语义钉住**（reviewer-6 `8060a856` 的 P-1）：oracle `BaseObject.cs:541-548`
    /// 是 `ushort idura = (ushort)(Dura-2000)` ⇒ **u16 回绕**，后面的 `if (idura <= 0) idura = 0;`
    /// 对 `ushort` 恒 false（死代码）。`drop_stdmode40_dura_underflow`（Dura=10 ⇒ 63546）与
    /// `drop_stdmode40_dura_normal`（Dura=2500 ⇒ 500）取自 oracle 真身探针实测。
    /// 把 `wrapping_sub` 改回 `max(0, Dura-2000)` ⇒ underflow 用例必红。
    #[test]
    fn stdmode40_drop_dura_wraps_like_oracle() {
        let doc = fixture();
        let cat = catalog(&doc);
        let cases = doc["cases"].as_array().unwrap();
        for (name, want) in [
            ("drop_stdmode40_dura_normal", 500u16),
            ("drop_stdmode40_dura_underflow", 63546u16),
        ] {
            let case = cases
                .iter()
                .find(|c| c["name"] == name)
                .unwrap_or_else(|| panic!("夹具必须含 {name}"));
            let c = &case["ctx"];
            let mut map = build_map(case);
            let mut state = GroundState {
                item_list: bag_items(&case["item_list_before"]),
                gold: i32v(&case["gold_before"]),
                wabil: ability(&case["wabil_before"]),
            };
            let ctx = DropContext {
                catalog: &cat,
                actor_id: i32v(&c["actor_id"]),
                curr_x: i32v(&c["curr_x"]),
                curr_y: i32v(&c["curr_y"]),
                now_tick: i32v(&case["now_tick"]),
                deal_last_tick: i32v(&c["deal_last_tick"]),
                make_index: i32v(&case["request"]["make_index"]),
                in_safe_disable_drop: false,
                in_safe_zone: false,
                is_can_drop: true,
                no_throw_item: false,
                control_drop_item: false,
                can_drop_price: 500,
                drop_item_rage: 3,
                item_name: case["request"]["name"].as_str().unwrap(),
            };
            let out = drop_item(&ctx, &mut map, &mut state);
            // T-19：ItemId 恒 0 ⇒ 丢弃仍失败、物品留在背包（但 Dura 已被原地扣减）。
            assert!(!out.success, "{name}: ItemId 恒 0 ⇒ 丢弃必失败");
            assert_eq!(out.immediate_frames[0].ident, SM_DROPITEM_FAIL);
            assert_eq!(state.item_list.len(), 1, "{name}: 物品应留在背包");
            assert_eq!(
                state.item_list[0].dura, want,
                "{name}: StdMode 40 耐久须与 oracle u16 回绕一致"
            );
        }
    }

    /// 拾取的延时通知是**出队序**（优先级 = `(byte)wIdent`，低值先出），**不等于**代码顺序：
    /// `SendMsg(RM_ITEMHIDE)` 先入队，但 `(byte)10115 < (byte)8085` ⇒ `RM_WEIGHTCHANGED` 先出。
    /// 改成"入队序"本用例必红。
    #[test]
    fn pickup_notices_are_in_priority_dequeue_order() {
        let doc = fixture();
        let cat = catalog(&doc);
        let case = doc["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["name"] == "pickup_ok_item")
            .expect("夹具必须含 pickup_ok_item");
        let c = &case["ctx"];
        let mut map = build_map(case);
        let mut state = GroundState {
            item_list: Vec::new(),
            gold: 1000,
            wabil: ability(&case["wabil_before"]),
        };
        let ctx = PickUpContext {
            catalog: &cat,
            actor_id: i32v(&c["actor_id"]),
            curr_x: i32v(&c["curr_x"]),
            curr_y: i32v(&c["curr_y"]),
            now_tick: i32v(&case["now_tick"]),
            dealing: false,
            floor_item_can_pick_up_time: i32v(&c["floor_item_can_pick_up_time"]),
            human_max_gold: i32v(&c["human_max_gold"]),
            race_is_play: true,
            group_owner: 0,
            group_member_actor_ids: &[],
            red_msg_f_color: 255,
            red_msg_b_color: 56,
        };
        let out = pick_up(&ctx, &mut map, &mut state);
        let ids: Vec<i32> = out.notices.iter().map(|n| n.ident).collect();
        assert_eq!(ids, vec![RM_WEIGHTCHANGED, RM_ITEMHIDE]);
        assert!((RM_WEIGHTCHANGED as u8) < (RM_ITEMHIDE as u8));
        assert_eq!(out.immediate_frames.len(), 1);
        assert_eq!(out.immediate_frames[0].ident, SM_ADDITEM);
        assert_eq!(out.immediate_frames[0].recog, i32v(&c["actor_id"]));
        assert_eq!(out.immediate_frames[0].series, 1);
        assert_eq!(state.item_list.len(), 1);
        assert_eq!(state.wabil.weight, 3, "重算背包重量 = 模板 Weight");
    }

    /// 负重不足 ⇒ `DeleteFromMap` 成功后**放回**原格：地面保留、背包不变、无帧无通知。
    /// 改坏 `is_add_weight_available` 分支本用例必红。
    #[test]
    fn pickup_puts_back_when_overweight() {
        let doc = fixture();
        let cat = catalog(&doc);
        let case = doc["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["name"] == "pickup_fail_weight")
            .expect("夹具必须含 pickup_fail_weight");
        let c = &case["ctx"];
        let mut map = build_map(case);
        let mut state = GroundState {
            item_list: Vec::new(),
            gold: 1000,
            wabil: ability(&case["wabil_before"]),
        };
        let ctx = PickUpContext {
            catalog: &cat,
            actor_id: i32v(&c["actor_id"]),
            curr_x: i32v(&c["curr_x"]),
            curr_y: i32v(&c["curr_y"]),
            now_tick: i32v(&case["now_tick"]),
            dealing: false,
            floor_item_can_pick_up_time: i32v(&c["floor_item_can_pick_up_time"]),
            human_max_gold: i32v(&c["human_max_gold"]),
            race_is_play: true,
            group_owner: 0,
            group_member_actor_ids: &[],
            red_msg_f_color: 255,
            red_msg_b_color: 56,
        };
        let before = state.item_list.clone();
        let out = pick_up(&ctx, &mut map, &mut state);
        assert!(out.immediate_frames.is_empty());
        assert!(out.notices.is_empty());
        assert_eq!(state.item_list, before);
        assert_eq!(map.item_count(), 1, "超重 ⇒ 物品必须放回地面");
        assert_eq!(map.get_item(ctx.curr_x, ctx.curr_y).unwrap().item_id, 70007);
    }

    /// 金币拾取：`IncGold` 成功 ⇒ 删格 + `RM_GOLDCHANGED`；失败（超 `HumanMaxGold`）⇒ 放回、金币不变。
    #[test]
    fn gold_pickup_inc_and_clamp() {
        let doc = fixture();
        let cat = catalog(&doc);
        let cases = doc["cases"].as_array().unwrap();

        let ok = cases
            .iter()
            .find(|c| c["name"] == "pickup_gold_ok")
            .unwrap();
        let c = &ok["ctx"];
        let mut map = build_map(ok);
        let mut state = GroundState {
            item_list: Vec::new(),
            gold: 1000,
            wabil: ability(&ok["wabil_before"]),
        };
        let ctx = PickUpContext {
            catalog: &cat,
            actor_id: i32v(&c["actor_id"]),
            curr_x: i32v(&c["curr_x"]),
            curr_y: i32v(&c["curr_y"]),
            now_tick: i32v(&ok["now_tick"]),
            dealing: false,
            floor_item_can_pick_up_time: i32v(&c["floor_item_can_pick_up_time"]),
            human_max_gold: i32v(&c["human_max_gold"]),
            race_is_play: true,
            group_owner: 0,
            group_member_actor_ids: &[],
            red_msg_f_color: 255,
            red_msg_b_color: 56,
        };
        let out = pick_up(&ctx, &mut map, &mut state);
        assert_eq!(state.gold, 1500);
        assert!(out.immediate_frames.is_empty());
        assert_eq!(out.notices.len(), 1);
        assert_eq!(out.notices[0].ident, RM_GOLDCHANGED);
        assert_eq!(map.item_count(), 0);

        let fail = cases
            .iter()
            .find(|c| c["name"] == "pickup_gold_fail_max")
            .unwrap();
        let c = &fail["ctx"];
        let mut map = build_map(fail);
        let mut state = GroundState {
            item_list: Vec::new(),
            gold: 1000,
            wabil: ability(&fail["wabil_before"]),
        };
        let ctx = PickUpContext {
            catalog: &cat,
            actor_id: i32v(&c["actor_id"]),
            curr_x: i32v(&c["curr_x"]),
            curr_y: i32v(&c["curr_y"]),
            now_tick: i32v(&fail["now_tick"]),
            dealing: false,
            floor_item_can_pick_up_time: i32v(&c["floor_item_can_pick_up_time"]),
            human_max_gold: i32v(&c["human_max_gold"]),
            race_is_play: true,
            group_owner: 0,
            group_member_actor_ids: &[],
            red_msg_f_color: 255,
            red_msg_b_color: 56,
        };
        let out = pick_up(&ctx, &mut map, &mut state);
        assert_eq!(state.gold, 1000);
        assert!(out.notices.is_empty());
        assert_eq!(map.item_count(), 1);
    }

    /// **PL-1 / P21**：`drop_item_down_p21` 的 StdMode 40 耐久回绕 + 恒 false（T-19）。
    /// 红检：把返回改成 `true` ⇒ 第二条断言必红；把 `wrapping_sub` 改成 `saturating_sub`
    /// ⇒ 第一条断言（Dura=10 ⇒ 63546）必红。
    #[test]
    fn p21_drop_item_down_keeps_item_and_wraps_stdmode40_dura() {
        let cat = StdItemCatalog::from_items([(
            1u16,
            StdItem {
                name: "torch".to_string(),
                std_mode: 40,
                ..StdItem::default()
            },
        )]);
        let mut item = BagItem {
            make_index: 9,
            index: 1,
            dura: 10,
            dura_max: 100,
            desc: [0; 14],
        };
        assert!(
            !drop_item_down_p21(&cat, &mut item, 40, 3, 42),
            "MapItem.ItemId 恒 0 ⇒ AddItemToMap 必失败 ⇒ false"
        );
        assert_eq!(item.dura, 63546, "StdMode 40：10 - 2000 按 ushort 回绕");
        assert_eq!(item.index, 1, "失败时槽位物品原样保留");
    }
}
