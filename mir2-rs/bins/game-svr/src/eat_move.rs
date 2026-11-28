//! `EatUseItems` 的 **move 组**（IT-09 回城卷 / IT-10 随机传送 / IT-11 地牢逃脱）宿主拦截。
//!
//! # 为什么单独成模块（而不进 `useitem.rs`）
//!
//! `use_item` 是**纯决策函数**（帧/通知/背包迁移，不搬运位置）；这三条的副作用是
//! `PlayObject.Base.cs` 的 `EatUseItems → BaseObjectMove` 的**宿主移动**，要 `&mut World`
//! （改 `Envir`/`CurrX/CurrY`、抽 `world.rng`、发 `RM_SPACEMOVE_FIRE`）——纯函数拿不到。
//! 并行车道（`openmir2-worker-43` 的 IT-16）也在改 `useitem.rs`，故本组**零改 `useitem.rs`**，
//! 只由 `handle_eat_cmd` 在装 `UseItemContext` 之前调一次 [`maybe_eat_move`]（一条钩子）。
//!
//! # oracle 锚（`src/M2Server/Player/PlayObject.Base.cs`）
//!
//! | 条目 | oracle | 行为 |
//! | --- | --- | --- |
//! | IT-09 Shape1 回城卷 | `EatUseItems` case1（`:2231-2236`） | `SendRefMsg(RM_SPACEMOVE_FIRE)` + `BaseObjectMove(HomeMap,0,0)` + `true` |
//! | IT-10 Shape2 随机传送 | case2（`:2238-2246`） | `if (!Envir.Flag.boNORANDOMMOVE) { FIRE; BaseObjectMove(MapName,0,0); true }`；**置位 ⇒ 不吃** |
//! | IT-11 Shape3 地牢逃脱 | case3（`:2248-2262`） | `FIRE`；`PvpLevel()<2 ⇒ BaseObjectMove(HomeMap,HomeX,HomeY)`，否则 `Config.RedHome*`；恒 `true` |
//!
//! `BaseObjectMove(sMap,sX,sY)`（`:2312-2327`）：**空 `sMap` ⇒ `MapName`**；`sX != 0 && sY != 0`
//! ⇒ `SpaceMove`（精确点，落点可走则 **0 抽**）；否则 `MapRandomMove`（两次抽：
//! `Random(Width-nEgdey-1)`/`Random(Height-nEgdey-1)`，`nEgdey` 只看**目标图**高）。
//! `SpaceMove` 成功后 `SendMsg(RM_CLEAROBJECTS/RM_CHANGEMAP)` + `SendRefMsg(RM_SPACEMOVE_SHOW)`
//! ——本模块落成 `World::base_object_move_player` 的 `MapChange`（宿主 world tick 的
//! `drain_map_change` 出 633/634/766/54/801；不在本组对拍面，卡面登记）。
//!
//! `PvpLevel()` = `(byte)(PkPoint / 100)`（`PlayObject.cs:4352`）⇒ [`mir2_world_bevy::World::pvp_level`]。
//!
//! # 帧序（线上）
//!
//! oracle：`SM_EAT_OK(635)` 由 `ClientUseItems` **直发**（`SendDefMessage`），
//! `RM_SPACEMOVE_FIRE(8062)`/`RM_WEIGHTCHANGED(10115)` 入 `MsgQueue` 随后泵出
//! ⇒ 优先级 `(byte)8062=126 < 131` ⇒ 线上序 **635 → 800 → 622**（`SM_SPACEMOVE_HIDE` 的
//! 收件人按**移动前**位置 ±12、含自己）。本模块按同序直发：635 → 广播 800（移动前）→ 移动 →
//! 移除物品 + 重算重量 → 622。
//!
//! # `HomeState`（`PlayObject.HomeMap/HomeX/HomeY` 的会话快照）
//!
//! 语义见 [`HomeState`] 的文档；本模块**只读**它（IT-09/11 不回写）。

use std::collections::HashMap;

use tokio::sync::mpsc;

use mir2_world_bevy::HomeMoveTarget;

use crate::ability::ItemCatalog;
use crate::{OnlineRegistry, SessionGround, SimToGate};

/// `PlayObject.HomeMap/HomeX/HomeY`（`PlayObject.cs:134-142`）与 `SystemShare.Config.RedHome*`
/// 的会话快照，挂在 [`SessionGround`] 上（oracle 里 HomeMap 是 `PlayObject` 的字段）。
///
/// - `home_*`：**唯一两个写点** —— ① 登录 `session_ground_from_login`（取
///   `apply_get_start_point_on_login` **之后**的 final 记录值）；② 3s 支
///   `apply_start_point_for_fired_players`（与 `saves[chr].record.data.home_*` **同拍、同源**写回）。
///   oracle 的节奏：登录（`PlayObject.cs:1199`）与每 3s `CheckDupObjTick`
///   （`PlayObject.Message.cs:266`）各跑一次 `GetStartPoint()`（`PlayObject.Base.cs:704-721`）：
///   `PvpLevel()>=2` ⇒ `Config.RedHome*`；否则按**当前地图**命中 `StartPointList` 就覆盖。
///   ⇒ 本快照 **≤3s 陈旧**，与 oracle 同节奏（不是「吃物品时才现算」）。
/// - `red_home_*`：`SystemShare.Config.RedHomeMap/RedHomeX/RedHomeY`（`GameSvrConf.cs:1377-1379`
///   的 ctor 缺省 `3/845/674`，启动时被 `[Setup] RedHome*` 覆盖）。生产不可变 ⇒ 登录时抄一份。
///
/// **禁止**：任何 move 逻辑（含本模块）不得把 `home_*` 写回 `saves`（IT-09/11 只读）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HomeState {
    /// `PlayObject.HomeMap`（空串 = oracle 的空值；`BaseObjectMove` 会回落 `MapName`）。
    pub home_map: String,
    /// `PlayObject.HomeX`。
    pub home_x: i16,
    /// `PlayObject.HomeY`。
    pub home_y: i16,
    /// `SystemShare.Config.RedHomeMap`。
    pub red_home_map: String,
    /// `SystemShare.Config.RedHomeX`。
    pub red_home_x: i16,
    /// `SystemShare.Config.RedHomeY`。
    pub red_home_y: i16,
}

impl Default for HomeState {
    fn default() -> Self {
        Self {
            // `PlayObject` ctor（`PlayObject.cs:875`）置 `"0"`，随后登录必覆盖；会话外构造用空串
            //（= 未建模，`BaseObjectMove` 回落当前图；测试直接用空 home 的语义）。
            home_map: String::new(),
            home_x: 0,
            home_y: 0,
            // `GameSvrConf.cs:1377-1379` 的 ctor 缺省（`red_home_of` 在 `game_conf = None` 时同值）。
            red_home_map: "3".to_string(),
            red_home_x: 845,
            red_home_y: 674,
        }
    }
}

/// `SM_EAT_OK`/`SM_EAT_FAIL` 的 header-only 立即帧。
fn eat_frame(ident: u16) -> Vec<u8> {
    crate::smframe::gm_data_body(crate::smframe::command_header(0, ident, 0, 0, 0), &[])
}

/// `EatUseItems` move 组的宿主拦截（IT-09/10/11）。返回 `true` = 已接管（调用方直接 `return`）。
///
/// 只处理 **首件** `MakeIndex == make_index` 且模板 `StdMode==3 && Shape∈{1,2,3}` 的条目；
/// 其余（含模板缺失、`dead`、`no_drug`）一律 `false` ⇒ 交回 `use_item` 的既有分支
/// （`no_drug` 的红字 `RM_SYSMESSAGE` + `SM_EAT_FAIL` 仍由 `use_item` 出，避免两份真相）。
///
/// `can_use_item == false`（`RM_MENU_OK`）在宿主恒 `true`（登记），不在本函数判定。
#[allow(clippy::too_many_arguments)]
pub(crate) fn maybe_eat_move(
    world: &mut mir2_world_bevy::World,
    sg: &mut SessionGround,
    actor_id: i32,
    make_index: i32,
    chr_name: &str,
    socket: i32,
    online: &OnlineRegistry,
    gates: &HashMap<u8, mpsc::UnboundedSender<SimToGate>>,
    catalog: &dyn ItemCatalog,
    registry: Option<&mir2_envir::MapRegistry>,
    dead: bool,
    no_drug: bool,
    // **IT-13 行会回城**：`PlayObject.InGuildWarArea` 的每 actor 会话位（宿主 `castle_war_states`）。
    // **M1c-2c 段 5**：`None` = 区 k 的本地臂（无进程态）——只有 **Shape 5** 读它（Shape 1/2/3
    // 不需要）；Shape 5 在 `None` 下不执行（交回 `use_item` 的失败帧，零副作用）。
    castle_war_states: Option<&HashMap<i32, mir2_world_bevy::castle::CastleWarState>>,
    // 红字 `SM_SYSMESSAGE` 的颜色（`[Setup] RedMsgFColor/BColor`）。
    colors: &crate::MsgColors,
) -> bool {
    // `ClientUseItems`：`if (BoCanUseItem) { if (!Death) { … } }`；`EatItems` 首判 `boNODRUG`。
    if dead || no_drug {
        return false;
    }
    let Some(pos) = sg
        .state
        .item_list
        .iter()
        .position(|it| it.make_index == make_index)
    else {
        return false;
    };
    let Some(tpl) = catalog.get(sg.state.item_list[pos].index) else {
        return false;
    };
    if tpl.std_mode != 3 || !matches!(tpl.shape, 1..=3 | 5) {
        return false;
    }

    let gate_idx = online
        .iter()
        .find(|e| e.actor_id == actor_id)
        .map_or(0, |e| e.gate_idx);
    let tx = crate::route_to_gate(gates, gate_idx, socket, "吃 move 物品回帧");

    // `Envir.MapName`（当前图）与 `Envir.Flag`：都按实体 `map_id` 走注册表（唯一名源）。
    let cur_map = world
        .entities
        .get(actor_id)
        .and_then(|e| registry.and_then(|r| r.name_of(e.map_id)))
        .unwrap_or_default()
        .to_string();
    let pvp = world
        .entities
        .get(actor_id)
        .map_or(0, |e| mir2_world_bevy::World::pvp_level(e.pk_point));

    // `EatUseItems` 的目标（`sMap` + 精确/随机点）；空 `sMap` 的回落留给下面的 `BaseObjectMove` 口径。
    let (s_map, s_x, s_y) = match tpl.shape {
        // case1：`BaseObjectMove(HomeMap, 0, 0)` ⇒ HomeMap 上随机点。
        1 => (sg.home.home_map.clone(), 0, 0),
        // case2：`if (!Envir.Flag.boNORANDOMMOVE) { FIRE; BaseObjectMove(MapName,0,0); true }`
        2 => {
            let norandom = world
                .entities
                .get(actor_id)
                .and_then(|e| registry.and_then(|r| r.flag_of(e.map_id)))
                .is_some_and(|f| f.bo_norandommove);
            if norandom {
                // 不吃：只落 `SM_EAT_FAIL(636)`（无 FIRE、无移动、物品保留）。
                if let Some(tx) = &tx {
                    let _ = tx.send(SimToGate::SendFrame {
                        socket,
                        body: eat_frame(crate::useitem::SM_EAT_FAIL),
                    });
                }
                return true;
            }
            (cur_map.clone(), 0, 0)
        }
        // case5（IT-13 行会回城，`PlayObject.Base.cs:2264-2286`）：**不对称**——
        // `MyGuild == null` ⇒ 整段不做（不吃）；战区 ⇒ 红字「此处无法使用」**不吃**（636）；
        // 非战区但无 `MasterGuild == MyGuild` 的城 ⇒ 红字「无效」但**吃**（635，位置不变）；
        // 有效城堡 ⇒ `BaseObjectMove(castle.HomeMap, GetHomeX(), GetHomeY())`（先 X 后 Y 各抽 `Random(9)`）。
        5 => {
            // **M1c-2c 段 5**：区 k 本地臂（`proc=None`）不持有进程态 ⇒ Shape 5 不本地执行：
            // 交回 `use_item`（StdMode3/Shape5 无臂 ⇒ `SM_EAT_FAIL(636)`，零副作用）。
            // 生产不可达（分类器已把 Shape5 归 `MoveFamily` → 区 0）；这是安全默认。
            let Some(castle_war_states) = castle_war_states else {
                return false;
            };
            // `MyGuild == null` ⇒ `case 5` 整段不做、`result=false` ⇒ 交回 `use_item` 落 636。
            let Some(guild) = world.actor_guild(actor_id) else {
                return false;
            };
            let in_war = castle_war_states
                .get(&actor_id)
                .is_some_and(|s| s.in_guild_war_area);
            if in_war {
                // 不吃：直发 636 + 红字「此处无法使用」（`PlayObject.Base.cs:2283`）。
                if let Some(tx) = &tx {
                    let _ = tx.send(SimToGate::SendFrame {
                        socket,
                        body: eat_frame(crate::useitem::SM_EAT_FAIL),
                    });
                    let body = crate::smframe::sm_sysmessage(
                        actor_id,
                        u16::from(colors.red_f),
                        u16::from(colors.red_b),
                        &crate::enter::gbk("此处无法使用"),
                    );
                    let _ = tx.send(SimToGate::SendFrame { socket, body });
                }
                return true;
            }
            // `CastleMgr.IsCastleMember(this)` = 首座 `IsMember(MyGuild)` 的城；而 `IsMember` 本身
            // 就是 `IsMasterGuild(MyGuild)`（`UserCastle.cs:480-483`）⇒ 找到 ⇔ 有效。
            let castle = world
                .castles
                .iter()
                .find(|c| c.master_guild == Some(guild))
                .map(|c| (c.home_map.clone(), c.home_x, c.home_y));
            let Some((home_map, home_x, home_y)) = castle else {
                // 红字「无效」但**吃**（`PlayObject.Base.cs:2276-2279`）：位置不变、0 抽；吃的尾巴照走。
                if let Some(tx) = &tx {
                    let _ = tx.send(SimToGate::SendFrame {
                        socket,
                        body: eat_frame(crate::useitem::SM_EAT_OK),
                    });
                }
                sg.state.item_list.remove(pos);
                sg.state.wabil.weight =
                    crate::ground::recalc_bag_weight(catalog, &sg.state.item_list);
                if let Some(tx) = &tx {
                    // 泵序：`RM_WEIGHTCHANGED(10115→131)` < `RM_SYSMESSAGE(11004→252)`。
                    let body = crate::smframe::sm_weightchanged(
                        sg.state.wabil.weight,
                        sg.state.wabil.wear_weight,
                        sg.state.wabil.hand_weight,
                    );
                    let _ = tx.send(SimToGate::SendFrame { socket, body });
                    let body = crate::smframe::sm_sysmessage(
                        actor_id,
                        u16::from(colors.red_f),
                        u16::from(colors.red_b),
                        &crate::enter::gbk("无效"),
                    );
                    let _ = tx.send(SimToGate::SendFrame { socket, body });
                }
                return true;
            };
            // `GetHomeX/Y()` = `(short)(HomeX - 4 + Random(9))`，实参从左到右 ⇒ **先 X 后 Y**。
            let dx = world.rng.random_below(9);
            let dy = world.rng.random_below(9);
            (
                home_map,
                i32::from(home_x) - 4 + dx,
                i32::from(home_y) - 4 + dy,
            )
        }
        // case3：`PvpLevel()<2 ⇒ BaseObjectMove(HomeMap,HomeX,HomeY)`，否则 `Config.RedHome*`。
        _ => {
            if pvp >= 2 {
                (
                    sg.home.red_home_map.clone(),
                    i32::from(sg.home.red_home_x),
                    i32::from(sg.home.red_home_y),
                )
            } else {
                (
                    sg.home.home_map.clone(),
                    i32::from(sg.home.home_x),
                    i32::from(sg.home.home_y),
                )
            }
        }
    };
    // `BaseObjectMove`：`if (string.IsNullOrEmpty(sMap)) sMap = MapName;`
    let s_map = if s_map.is_empty() { cur_map } else { s_map };
    // `if (sX != 0 && sY != 0) SpaceMove(精确) else MapRandomMove(随机)`；`FindMap == null` ⇒ 不动。
    let target = registry.and_then(|r| r.id_of(&s_map)).map(|map_id| {
        if s_x != 0 && s_y != 0 {
            HomeMoveTarget::Fixed {
                map_id,
                x: s_x,
                y: s_y,
            }
        } else {
            HomeMoveTarget::Random { map_id }
        }
    });

    // ① 即时帧 `SM_EAT_OK(635)`（oracle 直发，线序最前）。
    if let Some(tx) = &tx {
        let _ = tx.send(SimToGate::SendFrame {
            socket,
            body: eat_frame(crate::useitem::SM_EAT_OK),
        });
    }
    // ② `RM_SPACEMOVE_FIRE`（**移动前**位置 ±12、含自己）⇒ 各收件人 `SM_SPACEMOVE_HIDE(800)`。
    //    失败（图缺失/201 轮不可走）也照发：oracle 的 FIRE 先于 `SpaceMove` 的成功判定。
    //    **IT-13 Shape5 例外**：oracle `case 5` 直接 `BaseObjectMove`，没有 `SendRefMsg(FIRE)`
    //（FIRE 只由 case1/2/3 显式发）⇒ 夹具 `pumped` 无 800；此处按 shape 门控。
    if tpl.shape != 5 {
        let hide =
            crate::smframe::sm_spacemove_hide(mir2_protocol::messages::SM_SPACEMOVE_HIDE, actor_id);
        crate::broadcast_ref_msg_with_self(world, online, gates, actor_id, &[hide]);
    }
    // ③ `BaseObjectMove`：落图/落点（跨图时记一条 `MapChange{space_move_show: Some(false)}`，
    //    宿主 world tick 的 `drain_map_change` 出 633/634/766/54/801——不在本组对拍面）。
    let _ = world.base_object_move_player(chr_name, actor_id, target);
    // ④ 吃尾巴（`ClientUseItems` 的 `eatSuccess` 支）：移除本件 + `WeightChanged()`
    //    （`WAbil.Weight = RecalcBagWeight()` + `RM_WEIGHTCHANGED` ⇒ `SM_WEIGHTCHANGED(622)`）。
    sg.state.item_list.remove(pos);
    sg.state.wabil.weight = crate::ground::recalc_bag_weight(catalog, &sg.state.item_list);
    if let Some(tx) = &tx {
        let body = crate::smframe::sm_weightchanged(
            sg.state.wabil.weight,
            sg.state.wabil.wear_weight,
            sg.state.wabil.hand_weight,
        );
        let _ = tx.send(SimToGate::SendFrame { socket, body });
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ability::StdItemCatalog;
    use crate::bag::BagItem;
    use crate::handle_eat_cmd;
    use crate::EatProcessEnv;
    use crate::{online_set, MsgColors, OnlineEntry, SessionGround};
    use mir2_protocol::frame::CommandMessage;
    use mir2_shared::rng::RandomNumber;
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};

    use tokio::sync::mpsc;

    fn fixture() -> serde_json::Value {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/parity/m3/fixtures/move_eat_vectors.json"
        );
        let raw = std::fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("oracle 向量夹具缺失/不可读：{path}（{e}）—— 不许静默跳过"));
        serde_json::from_str(&raw).unwrap_or_else(|e| panic!("{path} 不是合法 JSON：{e}"))
    }

    /// 合成一份真格式 `.map`（52B 头 + 12B/格，列主序；`wBk=0` ⇒ 全可走）。
    fn write_walkable_map(dir: &Path, name: &str, width: i32, height: i32) -> PathBuf {
        let mut bytes = vec![0u8; 52 + (width * height) as usize * 12];
        bytes[0..2].copy_from_slice(&(width as u16).to_le_bytes());
        bytes[2..4].copy_from_slice(&(height as u16).to_le_bytes());
        let path = dir.join(format!("{name}.map"));
        std::fs::write(&path, &bytes).expect("写 .map 夹具失败");
        path
    }

    /// 夹具 `item_list` → `Vec<BagItem>`（move/guild 夹具记录完整 5 字段，含 `desc[14]`）。
    fn golden_bag_items(v: &serde_json::Value) -> Vec<BagItem> {
        v.as_array()
            .unwrap()
            .iter()
            .map(|it| BagItem {
                make_index: it["make_index"].as_i64().unwrap() as i32,
                index: it["index"].as_u64().unwrap() as u16,
                dura: it["dura"].as_u64().unwrap() as u16,
                dura_max: it["dura_max"].as_u64().unwrap() as u16,
                desc: std::array::from_fn(|i| it["desc"][i].as_u64().unwrap() as u8),
            })
            .collect()
    }

    fn registry_from(dir: &Path, maps: &[(&str, bool, &str)]) -> mir2_envir::MapRegistry {
        for (name, _, _) in maps {
            write_walkable_map(dir, name, 60, 60);
        }
        let entries = maps
            .iter()
            .map(|(name, norandom, desc)| mir2_envir::mapinfo::MapInfoEntry {
                map_name: (*name).to_string(),
                map_file_name: (*name).to_string(),
                // 夹具字段点亮（① 档 IT-09/10/11/13，2026-11-28）：逐图非空 `MapDesc`（与探针同值，
                // 见 golden `ctx.map_desc`）——54 尾 = EDCode(GBK(目标图 MapDesc))，逐图不同才能证明
                // 尾取自目标图 Envir 而非常量。
                desc: (*desc).to_string(),
                server_index: 0,
                flag: mir2_envir::mapinfo::MapInfoFlag {
                    bo_norandommove: *norandom,
                    // 夹具字段点亮：`DARK`（boDarkness）⇒ `DayBright()=1`（`enter::day_bright` 的
                    // DARK 臂在 game_time 之前 ⇒ 与墙钟解耦、两侧确定值；时间三桶的覆盖见
                    // `map_change_634_day_bright_buckets`，章程 §36）。
                    bo_darkness: true,
                    ..Default::default()
                },
            })
            .collect::<Vec<_>>();
        mir2_envir::MapRegistry::from_entries(mir2_envir::MapLoader::new(dir, "", ""), entries)
    }

    /// 带 **`[名|文件]` alias** 的注册表：`alias` 的逻辑名与文件主干（`stem`）不同。
    fn registry_with_alias(dir: &Path) -> mir2_envir::MapRegistry {
        write_walkable_map(dir, "plain", 60, 60);
        write_walkable_map(dir, "stem", 60, 60);
        let mk = |name: &str, file: &str, flag: mir2_envir::mapinfo::MapInfoFlag| {
            mir2_envir::mapinfo::MapInfoEntry {
                map_name: name.to_string(),
                map_file_name: file.to_string(),
                desc: "别名图".to_string(),
                server_index: 0,
                flag,
            }
        };
        let entries = vec![
            mk(
                "plain",
                "plain",
                mir2_envir::mapinfo::MapInfoFlag {
                    day_light: true,
                    ..Default::default()
                },
            ),
            mk(
                "alias",
                "stem",
                mir2_envir::mapinfo::MapInfoFlag {
                    safe_area: true,
                    music: true,
                    music_id: 7,
                    day_light: true,
                    ..Default::default()
                },
            ),
        ];
        mir2_envir::MapRegistry::from_entries(mir2_envir::MapLoader::new(dir, "", ""), entries)
    }

    /// **`[名|文件]` alias + 换图簇连带帧**：
    /// - 634 尾 = **取图文件主干**（oracle `Envir.MapFileName`，`MapManager.cs:66-96`），不是逻辑名；
    /// - 766/54（oracle `RM_CHANGEMAP` 处理器连带帧）序在 634 之后、只发自己（`SendDefMessage`）。
    /// - 两半都断：alias 图尾 = `stem`；无 alias 图尾 = 逻辑名（逐字节不变）。
    #[test]
    fn map_change_634_tail_uses_alias_file_stem() {
        let dir = std::env::temp_dir().join(format!(
            "mir2-alias-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).expect("tempdir");
        let reg = registry_with_alias(&dir);
        assert_eq!(reg.name_of(1), Some("alias"));
        assert_eq!(reg.map_file_name_of(1), Some("stem"));

        let mut world = mir2_world_bevy::World::with_maps(crate::mapwire::sizes(&reg));
        world.open_session("hero");
        let mut e = mir2_world_bevy::EntitySpawn::new(0, "hero", 5, 5);
        e.race = mir2_world_bevy::ACTOR_RACE_PLAY;
        let actor_id = world.enter_map("hero", e, 0).expect("进图");
        let _ = world.sessions.drain_map_changes();
        let mut displays = crate::downlink::DisplayTable::new();
        displays.set(
            actor_id,
            crate::downlink::ActorDisplay::new(4, 0, 0, 0, 0, 255, b"hero".to_vec()),
        );
        let mut online: crate::OnlineRegistry = crate::OnlineRegistry::new();
        crate::online_set(
            &mut online,
            crate::OnlineEntry {
                account: "ua".into(),
                chr_name: "hero".into(),
                session_id: 1,
                gate_idx: 0,
                socket: 901,
                actor_id,
            },
        );
        let (tx, mut rx) = mpsc::unbounded_channel::<crate::SimToGate>();
        let mut gates: HashMap<u8, mpsc::UnboundedSender<crate::SimToGate>> = HashMap::new();
        gates.insert(0, tx);

        // (a) 换进 alias 图（逻辑名 alias / 文件主干 stem）⇒ 634 尾 = EncodeString("stem")，
        //     并出 766（safe_area ⇒ n8=2）与 54（music_id=7，尾 = MapDesc）。
        world.switch_map("hero", 1, 7, 9).expect("换进 alias 图");
        crate::drain_map_change(&mut world, &online, &gates, &displays, Some(&reg));
        let mut frames: Vec<(u16, i32, Vec<u8>)> = Vec::new();
        while let Ok(crate::SimToGate::SendFrame { body, .. }) = rx.try_recv() {
            let m = CommandMessage::from_bytes(&body).expect(">=12B");
            frames.push((m.ident, m.recog, body[CommandMessage::SIZE..].to_vec()));
        }
        let ids: Vec<u16> = frames.iter().map(|f| f.0).collect();
        assert_eq!(
            ids,
            vec![
                mir2_protocol::messages::SM_CLEAROBJECTS,
                mir2_protocol::messages::SM_CHANGEMAP,
                mir2_protocol::messages::SM_AREASTATE,
                mir2_protocol::messages::SM_MAPDESCRIPTION
            ],
            "EnterAnotherMap 支：633 → 634 → 766 → 54（无 801）"
        );
        assert_eq!(
            frames[1].2,
            mir2_protocol::edcode::encode(&crate::enter::gbk("stem")),
            "634 尾必须是取图文件主干（不是逻辑名 alias）"
        );
        assert_eq!(frames[2].1, 2, "766 Recog = SafeArea|2");
        assert_eq!(frames[3].1, 7, "54 Recog = MusicId");
        assert_eq!(
            frames[3].2,
            mir2_protocol::edcode::encode(&crate::enter::gbk("别名图")),
            "54 尾 = EDCode(GBK(MapDesc))"
        );

        // (b) 对照：换进无 alias 的图（逻辑名 plain == 文件主干）⇒ 634 尾 = "plain"。
        world.switch_map("hero", 0, 5, 5).expect("换进 plain 图");
        crate::drain_map_change(&mut world, &online, &gates, &displays, Some(&reg));
        let mut frames: Vec<(u16, i32, Vec<u8>)> = Vec::new();
        while let Ok(crate::SimToGate::SendFrame { body, .. }) = rx.try_recv() {
            let m = CommandMessage::from_bytes(&body).expect(">=12B");
            frames.push((m.ident, m.recog, body[CommandMessage::SIZE..].to_vec()));
        }
        assert_eq!(
            frames[1].2,
            mir2_protocol::edcode::encode(&crate::enter::gbk("plain")),
            "无 alias ⇒ 634 尾 == 逻辑名（逐字节不变）"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    fn case_ctx(c: &serde_json::Value) -> &serde_json::Value {
        &c["ctx"]
    }

    /// **IT-09 宿主级对拍**（回城卷 Shape1 两支）：驱动 `handle_eat_cmd`，比
    /// ① **线上帧**（真实帧，不是标志位代理）：635 → 800 → 622 → 633 → 634 → 801，逐字段；
    ///    633/634/766/54/801 由**生产共用**的 `drain_map_change`（world tick 的换图簇）产出——本用例在
    ///    `handle_eat_cmd` 之后按生产次序驱动它，从而与 oracle 泵出的帧逐帧比。`766(SM_AREASTATE)`/
    ///    `54(SM_MAPDESCRIPTION)`（oracle `RM_CHANGEMAP` 处理器的连带帧）已由换图簇卡接线；
    ///    本用例断言**剔除集为空**（防再次静默漏帧）。
    /// ② 终态 `MapName/X/Y`（实体 + 会话）③ `ItemList` 清空 + `WAbil.Weight`
    /// ④ `Rng` 逐次（同 seed 的 legacy `System.Random` 序列）。
    ///
    /// **尾也逐字节比**：634 尾 = `EDCode(Envir.MapFileName)`（`Envir.MapFileName` 是 MapInfo 的**取图
    /// 文件主干**，`MapManager.AddMapInfo`；本夹具逻辑名 == 主干 ⇒ 两侧同值）；801 尾 = `EDCode(CharDesc)`
    /// （`Feature=0/Status=0`）。801 的 `Series = MakeWord(Dir,Light)` 在夹具固定 `DayLight=true`
    /// （DayBright=0）、实体 `Dir=4`（`BaseObject.cs:378` ctor）、`Light=0` 下逐值成立——这三个前提
    /// 记在 golden 的 `scenario_note` 与 `ctx.presentation` 里。
    /// **`[名|文件]` 形态**：oracle 发**文件主干**；本夹具无 alias 形态（`map_file_name == map_name`），
    /// 故 634 尾两者同值。alias 覆盖另由 `map_change_634_tail_uses_alias_file_stem` 证明。
    ///
    /// 红检：hook 返回值恒改 `false`（回 `_ => {}` ⇒ 636）⇒ 帧序必红；把 hook 里的
    /// `base_object_move_player` 换成不动（不出换图簇）⇒ 帧序/终态必红；HomeMap 空回落删掉 ⇒
    /// 第二例终态图红。
    fn run_move_parity(case_index: u16, expected_cases: usize, only: Option<&str>) {
        let doc = fixture();
        let mut cases: Vec<&serde_json::Value> = doc["cases"]
            .as_array()
            .expect("cases")
            .iter()
            .filter(|c| case_ctx(c)["index"].as_i64() == Some(i64::from(case_index)))
            .collect();
        if let Some(name) = only {
            cases.retain(|c| c["name"].as_str() == Some(name));
            assert_eq!(cases.len(), 1, "单例筛选 {name} 未命中（夹具被改？）");
        } else {
            assert_eq!(
                cases.len(),
                expected_cases,
                "IT 条目 {case_index} 的用例数不符（夹具被改？）"
            );
        }
        // 对拍面内/外的 ident 清单（golden 的 pumped 必须恰是这两集合之并）。
        // 766/54（oracle `RM_CHANGEMAP` 处理器的连带帧）已由换图簇卡接线 ⇒ 进比较面。
        let modeled: [i64; 7] = [800, 622, 633, 634, 766, 54, 801];
        // 已无登记缺口：剔除集必须是空集（保留变量名只为下方断言可读）。
        let registered_gap: [i64; 0] = [];

        let mut catalog = StdItemCatalog::new();
        for (k, v) in doc["templates"].as_object().expect("templates") {
            catalog.insert(
                k.parse::<u16>().unwrap(),
                mir2_data::StdItem {
                    name: v["name"].as_str().unwrap().to_string(),
                    std_mode: v["std_mode"].as_u64().unwrap() as u8,
                    shape: v["shape"].as_u64().unwrap() as u8,
                    weight: v["weight"].as_u64().unwrap() as u8,
                    dura_max: 20,
                    ..Default::default()
                },
            );
        }

        // 每个测试调用一个独立目录（同进程内多个 `#[test]` 并行跑；共享目录会被彼此
        // 的 `remove_dir_all` 删掉 ⇒ 随机红）。
        static DIR_SEQ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let seq = DIR_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("mir2-move-host-{}-{seq}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("tempdir");
        for c in cases {
            let ctx = case_ctx(c);
            let norandom = ctx["norandom"].as_bool().unwrap_or(false);
            // 注册表顺序 = map_id：move_map=0 / home_map=1 / red_map=2；`boNORANDOMMOVE` 逐例（IT-10 两支）。
            // 逐图 `MapDesc` 从夹具 ctx 读（与探针同源）。
            let map_desc = |n: &str| -> String {
                ctx["map_desc"][n]
                    .as_str()
                    .expect("ctx.map_desc 缺键")
                    .to_string()
            };
            let reg = registry_from(
                &dir,
                &[
                    ("move_map", norandom, map_desc("move_map").as_str()),
                    ("home_map", false, map_desc("home_map").as_str()),
                    ("red_map", false, map_desc("red_map").as_str()),
                ],
            );
            assert_eq!(reg.map_names().len(), 3);
            let expect = &c["expect"];
            let cur_name = ctx["current"]["map"].as_str().unwrap();
            let cur_map_id = reg.id_of(cur_name).expect("当前图在注册表");
            let seed = ctx["seed"].as_i64().unwrap() as i32;
            let make_index = ctx["make_index"].as_i64().unwrap() as i32;
            let item_index = ctx["index"].as_i64().unwrap() as u16;

            let mut world = mir2_world_bevy::World::with_maps(crate::mapwire::sizes(&reg));
            world.open_session("hero");
            let mut e = mir2_world_bevy::EntitySpawn::new(
                0,
                "hero",
                ctx["current"]["x"].as_i64().unwrap() as i32,
                ctx["current"]["y"].as_i64().unwrap() as i32,
            );
            e.race = mir2_world_bevy::ACTOR_RACE_PLAY;
            e.pk_point = ctx["pvp_level"].as_i64().unwrap() as i32 * 100;
            let actor_id = world.enter_map("hero", e, cur_map_id).expect("进图");
            // 801 的 Series 低字节 = 实体 `Dir` **活值**（旋钮值来自夹具 `ctx.presentation.dir`=5，
            // 非 `BaseObject.cs:378` ctor 常量 4）；展示表 dir 保留 ctor 值 4 作陈旧投影，
            // `drain_map_change_at_time` 必须取实体值 ⇒ 若产品误取展示表投影则对拍红。
            let dir_knob = ctx["presentation"]["dir"]
                .as_u64()
                .expect("ctx.presentation.dir") as u8;
            if let Some(ai) = world.entities.ai_mut(actor_id) {
                ai.dir = dir_knob;
            } else {
                panic!("实体缺 ai 组件");
            }
            let _ = world.sessions.drain_map_changes();

            let mut sessions: HashMap<String, SessionGround> = HashMap::new();
            let mut sg = crate::tests::test_session_ground(actor_id);
            sg.state.item_list = vec![BagItem {
                make_index,
                index: item_index,
                dura: 10,
                dura_max: 20,
                desc: [0; 14],
            }];
            // `SM_WEIGHTCHANGED(622)` 的 Recog/Param/Tag 取 `WAbil.Weight/WearWeight/HandWeight`；
            // 前值与夹具一致（golden `ctx.wabil_before`）——只改 Weight 是 `WeightChanged` 的语义。
            sg.state.wabil.weight = ctx["wabil_before"]["weight"].as_i64().unwrap() as u16;
            sg.state.wabil.wear_weight = ctx["wabil_before"]["wear_weight"].as_i64().unwrap() as u8;
            sg.state.wabil.hand_weight = ctx["wabil_before"]["hand_weight"].as_i64().unwrap() as u8;
            sg.home = HomeState {
                home_map: ctx["home"]["map"].as_str().unwrap().to_string(),
                home_x: ctx["home"]["x"].as_i64().unwrap() as i16,
                home_y: ctx["home"]["y"].as_i64().unwrap() as i16,
                red_home_map: ctx["red_home"]["map"].as_str().unwrap().to_string(),
                red_home_x: ctx["red_home"]["x"].as_i64().unwrap() as i16,
                red_home_y: ctx["red_home"]["y"].as_i64().unwrap() as i16,
            };
            sessions.insert("hero".into(), sg);
            let mut online: OnlineRegistry = OnlineRegistry::new();
            online_set(
                &mut online,
                OnlineEntry {
                    account: "ua".into(),
                    chr_name: "hero".into(),
                    session_id: 1,
                    gate_idx: 0,
                    socket: 901,
                    actor_id,
                },
            );
            let (tx, mut rx) = mpsc::unbounded_channel::<SimToGate>();
            let mut gates: HashMap<u8, mpsc::UnboundedSender<SimToGate>> = HashMap::new();
            gates.insert(0, tx);
            let colors = MsgColors {
                red_f: 255,
                red_b: 56,
                hear_f: 0,
                hear_b: 255,
                whisper_f: 0,
                whisper_b: 0,
                green_f: 0xDB,
                green_b: 0xFF,
                blue_f: 0xFF,
                blue_b: 0xFC,
            };
            let unbind = game_svr::unbindlist::UnbindList::default();
            let item_number = std::sync::atomic::AtomicI32::new(0);
            world.rng = RandomNumber::with_seed(seed);

            handle_eat_cmd(
                make_index,
                0,
                0,
                None,
                "hero",
                901,
                &online,
                &mut sessions,
                &gates,
                &catalog,
                &mut world,
                &colors,
                90,
                // master 的 `strings` 形参（IT-17a 读本用）；本组不吃书，默认文案即可。
                &game_svr::strings::StringConf::default(),
                Some(&reg),
                &unbind,
                &item_number,
                Some(EatProcessEnv {
                    npc_host: None,
                    npc_vars: &game_svr::npc::NpcVars::default(),
                    gm: None,
                    // IT-16 尾参（本组用例不走彩票支；传进程级缺省句柄/配置，抽数不发生）。
                    lottery: &crate::winlottery::WinLotteryShared::new(
                        crate::config::WinLotteryCfg::default().init_state(),
                    ),
                    castle_war_states: &std::collections::HashMap::new(),
                }),
                crate::config::WinLotteryCfg::default(),
                10_000_000,
                crate::config::WeaponMakeLuckCfg::default(),
            );

            // ① 生产换图簇：与 world tick 同一条路径（`drain_map_change`），展示表给 Light=0/Feature=0/
            //    Status=0（只影响 801 的 Series 高字节与尾，见函数头字段边界）。
            let mut displays = crate::downlink::DisplayTable::new();
            // 展示表 dir 留 ctor 值 4（陈旧投影）：产品 `drain_map_change_at_time` 的 801 Series
            // 低字节必须取**实体** `ai.dir`（夹具旋钮 5）⇒ 误取展示表投影则 4≠5，对拍红。
            displays.set(
                actor_id,
                crate::downlink::ActorDisplay::new(4, 0, 0, 0, 0, 255, b"hero".to_vec()),
            );
            crate::drain_map_change(&mut world, &online, &gates, &displays, Some(&reg));

            // 帧 = `(ident, recog, param, tag, series, tail_hex)`；`SimToGate::SendFrame.body` 是
            // 12B `CommandMessage` 头 + 载荷（网关再包 32B 信封），故载荷 = `body[12..]`。
            let hex =
                |b: &[u8]| -> String { b.iter().map(|x| format!("{x:02x}")).collect::<String>() };
            let mut got: Vec<(i64, i64, i64, i64, i64, String)> = Vec::new();
            while let Ok(SimToGate::SendFrame { body, .. }) = rx.try_recv() {
                let m = CommandMessage::from_bytes(&body).expect(">=12B");
                got.push((
                    m.ident as i64,
                    m.recog as i64,
                    m.param as i64,
                    m.tag as i64,
                    m.series as i64,
                    hex(&body[CommandMessage::SIZE..]),
                ));
            }
            // 期望 = immediate_frames ++ pumped（pumped 先按 modeled 过滤，并断言剔除集恰为登记缺口）。
            let mut want: Vec<(i64, i64, i64, i64, i64, String)> = Vec::new();
            // oracle 的 `ActorId`（golden ctx）映射到本引擎分配的 actor id：Recog 是**夹具身份**而非行为。
            let golden_actor = ctx["actor_id"].as_i64().unwrap();
            let five = |f: &serde_json::Value| {
                let recog = f["recog"].as_i64().unwrap();
                (
                    f["ident"].as_i64().unwrap(),
                    if recog == golden_actor {
                        i64::from(actor_id)
                    } else {
                        recog
                    },
                    f["param"].as_i64().unwrap(),
                    f["tag"].as_i64().unwrap(),
                    f["series"].as_i64().unwrap(),
                    f["tail"].as_str().unwrap_or("").to_string(),
                )
            };
            for f in expect["immediate_frames"].as_array().unwrap() {
                want.push(five(f));
            }
            let mut dropped: Vec<i64> = Vec::new();
            for f in expect["pumped"].as_array().unwrap() {
                let (ident, ..) = five(f);
                if modeled.contains(&ident) {
                    want.push(five(f));
                } else {
                    dropped.push(ident);
                }
            }
            let want_gap: Vec<i64> = if norandom {
                Vec::new()
            } else {
                registered_gap.to_vec()
            };
            assert_eq!(
                dropped, want_gap,
                "{}: 剔除集必须为空（766/54 已接线，不应再被剔除）",
                c["name"]
            );
            assert_eq!(
                got, want,
                "{}: 线上帧严格序必须 635→800→622→633→634→766→54→801（逐字段）",
                c["name"]
            );

            // ② 终态 MapName/X/Y（实体 + 会话，两侧同值）。
            let final_map = expect["final"]["map"].as_str().unwrap();
            let final_x = expect["final"]["x"].as_i64().unwrap() as i32;
            let final_y = expect["final"]["y"].as_i64().unwrap() as i32;
            let want_map_id = reg.id_of(final_map).expect("终态图在注册表");
            let ent = world.entities.get(actor_id).expect("实体还在");
            assert_eq!(
                (ent.map_id, ent.x, ent.y),
                (want_map_id, final_x, final_y),
                "{}: 终态 MapName/X/Y（实体）",
                c["name"]
            );
            let sess = world.session("hero").expect("会话");
            assert_eq!(sess.map_id, want_map_id, "{}: 终态图（会话）", c["name"]);

            // ③ ItemList / WAbil.Weight：**逐件整 `BagItem` 比**（这些路径无解包/无墙钟
            // `make_index` 生成 ⇒ 不需要排除项；夹具记录完整 5 字段含 `desc`）。
            let want_items = golden_bag_items(&expect["item_list"]);
            assert_eq!(
                sessions["hero"].state.item_list, want_items,
                "{}: 背包终态（逐件整 BagItem）",
                c["name"]
            );
            assert_eq!(
                i64::from(sessions["hero"].state.wabil.weight),
                expect["weight"].as_i64().unwrap()
            );

            // ④ Rng 逐次：按 golden 的 `(arg, value)` 重放同 seed 流，再核「没有多抽」。
            //    golden 的 `rng_draw_count` 是**oracle 记录器自证的抽数**（不是「没看到数」）；
            //    禁随机支必须为 0，且 Rust 侧的「0 抽」由下一步的流位置等式独立证明。
            let golden_draws = expect["rng"].as_array().unwrap();
            assert_eq!(
                expect["rng_draw_count"].as_i64().unwrap(),
                golden_draws.len() as i64,
                "{}: 记录器计数与逐次账本不一致（夹具坏了）",
                c["name"]
            );
            let mut replay = RandomNumber::with_seed(seed);
            for d in golden_draws {
                let arg = d["arg"].as_i64().unwrap() as i32;
                let value = d["value"].as_i64().unwrap() as i32;
                assert_eq!(replay.random_below(arg), value, "{}: 逐次取数", c["name"]);
            }
            assert_eq!(
                world.rng.random_below(1000),
                replay.random_below(1000),
                "{}: 移动后不得多抽（流位置对齐）",
                c["name"]
            );
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    /// **IT-09 回城卷 Shape1** 宿主级对拍（判据清单见 `run_move_parity` 的文档）。
    ///
    /// 红检：hook 返回值恒改 `false`（回 `_ => {}` ⇒ 636）⇒ 帧序必红；把 hook 里的
    /// `base_object_move_player` 换成不动（不出换图簇）⇒ 帧序/终态必红；HomeMap 空回落删掉 ⇒
    /// 第二例终态图红。
    #[test]
    fn it09_town_scroll_host_parity() {
        run_move_parity(3001, 2, None);
    }

    /// **IT-10 随机传送 Shape2** 宿主级对拍（两支）：
    /// - 未置位：吃 + 本图随机点（`BaseObjectMove(MapName,0,0)`）+ 全套换图簇；
    /// - **`boNORANDOMMOVE` 置位：不吃**（`EatUseItems` 返回 false）⇒ 只 `SM_EAT_FAIL(636)`，
    ///   **不移动、不消耗、0 抽**（golden `pumped=[]`/`rng=[]`/`item_list` 保留 1 件）。
    ///
    /// 红检：把 `bo_norandommove` 判读改反（`if norandom` → `if !norandom`）⇒ 置位支会吃+移动、
    /// 未置位支变 636 ⇒ 两例同时红；把 Shape2 的目标图换成 HomeMap ⇒ 未置位支终态图红。
    #[test]
    fn it10_random_scroll_host_parity() {
        run_move_parity(3002, 2, None);
    }

    /// **IT-10 禁随机支单跑**（变异牙要「一支一支红」的原文，而不是两例一起挂）。
    #[test]
    fn it10_norandom_branch_no_eat() {
        run_move_parity(3002, 2, Some("eat_fail_shape2_norandommap_no_eat"));
    }

    /// **IT-10 未置位支单跑**（同上）。
    #[test]
    fn it10_random_branch_moves() {
        run_move_parity(3002, 2, Some("eat_ok_shape2_random_scroll_current_map"));
    }

    /// **IT-11 地牢逃脱 Shape3** 宿主级对拍（三支）：
    /// - `PvpLevel<2` ⇒ `BaseObjectMove(HomeMap,HomeX,HomeY)` 精确点（0 抽）；
    /// - `PvpLevel>=2` ⇒ `BaseObjectMove(Config.RedHomeMap,RedHomeX,RedHomeY)` 精确点（0 抽）；
    /// - `HomeX/HomeY` 任一为 0 ⇒ `MapRandomMove(HomeMap)`（两抽）——`BaseObjectMove` 的坐标门。
    ///
    /// 红检：把 `pvp >= 2` 改 `pvp > 2`（边界错）⇒ 红名支红；把 home/red 目标互换 ⇒ 红名支红；
    /// 把坐标 0 门去掉（恒 Fixed）⇒ 零坐标支的落点/抽数红。
    #[test]
    fn it11_dungeon_escape_host_parity() {
        run_move_parity(3003, 5, None);
    }

    /// **IT-11 单跑**（变异牙要逐支原文）。
    #[test]
    fn it11_home_branch_exact() {
        run_move_parity(3003, 5, Some("eat_ok_shape3_dungeon_home_exact"));
    }

    /// **IT-11 单跑**：`PvpLevel>=2` ⇒ RedHome 精确点。
    #[test]
    fn it11_red_home_branch_exact() {
        run_move_parity(3003, 5, Some("eat_ok_shape3_dungeon_red_home_exact"));
    }

    /// **IT-11 单跑**：另一组 RedHome 值（25/35）⇒ 证明目标来自快照、不是写死 20/30。
    #[test]
    fn it11_red_home_alt_values() {
        run_move_parity(3003, 5, Some("eat_ok_shape3_dungeon_red_home_alt_values"));
    }

    /// **IT-11 单跑**：只缺一维（HomeX=0、HomeY=6）⇒ 随机点——证明门是 `sX!=0 && sY!=0`（`&&`，不是 `||`）。
    #[test]
    fn it11_home_x_zero_y_nonzero_random() {
        run_move_parity(3003, 5, Some("eat_ok_shape3_home_x_zero_y_nonzero_random"));
    }

    /// **IT-11 单跑**：HomeX/HomeY 都为 0 ⇒ 随机点（`BaseObjectMove` 的 `sX!=0 && sY!=0` 门）。
    #[test]
    fn it11_home_zero_coords_random() {
        run_move_parity(
            3003,
            5,
            Some("eat_ok_shape3_dungeon_home_coords_zero_random"),
        );
    }

    fn guild_fixture() -> serde_json::Value {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/parity/m3/fixtures/guild_vectors.json"
        );
        let raw = std::fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("guild 夹具缺失/不可读：{path}（{e}）"));
        serde_json::from_str(&raw).unwrap_or_else(|e| panic!("{path} 不是合法 JSON：{e}"))
    }

    /// **IT-13 行会回城 Shape5** 宿主级对拍（五支，`guild_vectors.json`）。
    ///
    /// 判据面与 IT-09/10/11 同构（帧逐字段 / 终态 X-Y / 背包 / 重量 / RNG 逐次 + 无多抽），
    /// 另加两条本组专有：
    /// - **Shape5 不发 `RM_SPACEMOVE_FIRE`**（oracle `case 5` 只调 `BaseObjectMove`；夹具 `pumped` 无 800）；
    /// - **红字 `SM_SYSMESSAGE(100)` 逐字节**：`in_war_area` ⇒「此处无法使用」、`invalid` ⇒「无效」，
    ///   尾 = `EDCode(gbk(文案))`，且线序 635/636 → 622 → 100（`131 < 252`）。
    ///
    /// **部署不可达（§18.1）**：位移两支靠 `castle.master_guild = Some(玩家行会)` 的 synthetic 构造
    /// （部署态 `MasterGuild` 恒 null）⇒ 该两支证据**非生产路径**，不作生产已验证。
    ///
    /// 红检（逐支）：①`if in_war` 改 `!in_war` ⇒ war/invalid 两支互换红；②`master_guild` 判据改
    /// `!=` ⇒ 位移两支与 invalid 互换红；③去掉 `-4` ⇒ 终态 X/Y 红；④抽序对调（先 Y 后 X）
    /// ⇒ 逐次 RNG 红；⑤删掉 `tpl.shape != 5` 的 FIRE 门 ⇒ 位移两支多一帧 800 红。
    fn run_guild_parity(only: Option<&str>) {
        let doc = guild_fixture();
        let mut cases: Vec<&serde_json::Value> =
            doc["cases"].as_array().expect("cases").iter().collect();
        if let Some(name) = only {
            cases.retain(|c| c["name"].as_str() == Some(name));
            assert_eq!(cases.len(), 1, "单例筛选 {name} 未命中（夹具被改？）");
        } else {
            assert_eq!(cases.len(), 5, "IT-13 用例数固定 5（夹具被改？）");
        }
        // 766/54（oracle `RM_CHANGEMAP` 处理器的连带帧）已由换图簇卡接线 ⇒ 进比较面。
        let modeled: [i64; 7] = [800, 622, 633, 634, 766, 54, 801];
        // 已无登记缺口：剔除集必须是空集（保留变量名只为下方断言可读）。
        let registered_gap: [i64; 0] = [];

        let mut catalog = StdItemCatalog::new();
        for (k, v) in doc["templates"].as_object().expect("templates") {
            catalog.insert(
                k.parse::<u16>().unwrap(),
                mir2_data::StdItem {
                    name: v["name"].as_str().unwrap().to_string(),
                    std_mode: v["std_mode"].as_u64().unwrap() as u8,
                    shape: v["shape"].as_u64().unwrap() as u8,
                    weight: v["weight"].as_u64().unwrap() as u8,
                    dura_max: 20,
                    ..Default::default()
                },
            );
        }

        static DIR_SEQ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let seq = DIR_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("mir2-guild-host-{}-{seq}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("tempdir");
        for c in cases {
            let ctx = &c["ctx"];
            // 逐图 `MapDesc` 从夹具 ctx 读（与探针同源；IT-13 同簇点亮笔）。
            let map_desc = |n: &str| -> String {
                ctx["map_desc"][n]
                    .as_str()
                    .expect("ctx.map_desc 缺键")
                    .to_string()
            };
            let reg = registry_from(
                &dir,
                &[
                    ("move_map", false, map_desc("move_map").as_str()),
                    ("home_map", false, map_desc("home_map").as_str()),
                    ("red_map", false, map_desc("red_map").as_str()),
                    ("castle_home", false, map_desc("castle_home").as_str()),
                ],
            );
            assert_eq!(reg.map_names().len(), 4);
            let expect = &c["expect"];
            let cur_name = ctx["current"]["map"].as_str().unwrap();
            let cur_map_id = reg.id_of(cur_name).expect("当前图在注册表");
            let seed = ctx["seed"].as_i64().unwrap() as i32;
            let make_index = ctx["make_index"].as_i64().unwrap() as i32;
            let item_index = ctx["index"].as_u64().unwrap() as u16;

            let mut world = mir2_world_bevy::World::with_maps(crate::mapwire::sizes(&reg));
            world.open_session("hero");
            let mut e = mir2_world_bevy::EntitySpawn::new(
                0,
                "hero",
                ctx["current"]["x"].as_i64().unwrap() as i32,
                ctx["current"]["y"].as_i64().unwrap() as i32,
            );
            e.race = mir2_world_bevy::ACTOR_RACE_PLAY;
            e.pk_point = ctx["pvp_level"].as_i64().unwrap_or(0) as i32 * 100;
            let actor_id = world.enter_map("hero", e, cur_map_id).expect("进图");
            // 801 的 Series 低字节 = 实体 `Dir` **活值**（旋钮值来自夹具 `ctx.presentation.dir`=5，
            // 非 ctor 常量 4；展示表 dir 留 4 作陈旧投影 ⇒ 产品误取展示表则对拍红）。
            let dir_knob = ctx["presentation"]["dir"]
                .as_u64()
                .expect("ctx.presentation.dir") as u8;
            if let Some(ai) = world.entities.ai_mut(actor_id) {
                ai.dir = dir_knob;
            } else {
                panic!("实体缺 ai 组件");
            }
            let _ = world.sessions.drain_map_changes();

            // 行会/城堡：探针私有的 synthetic 构造（部署态 `MasterGuild` 恒 null）。
            let guild = mir2_world_bevy::castle::GuildId(1);
            if ctx["has_guild"].as_bool().unwrap_or(false) {
                world.set_actor_guild(actor_id, guild);
            }
            let ch = &ctx["castle_home"];
            let mut castle = mir2_world_bevy::castle::UserCastle::new(
                "castle",
                mir2_world_bevy::castle::CastleSiegeConfig::default(),
            );
            castle.home_map = ch["map"].as_str().unwrap().to_string();
            castle.home_x = ch["x"].as_i64().unwrap() as i16;
            castle.home_y = ch["y"].as_i64().unwrap() as i16;
            if ctx["castle_member"].as_bool().unwrap_or(false) {
                castle.master_guild = Some(guild);
            }
            world.castles.push(castle);
            let mut war_states: HashMap<i32, mir2_world_bevy::castle::CastleWarState> =
                HashMap::new();
            if ctx["in_war_area"].as_bool().unwrap_or(false) {
                war_states.insert(actor_id, war_state_in_war());
            }

            let mut sessions: HashMap<String, SessionGround> = HashMap::new();
            let mut sg = crate::tests::test_session_ground(actor_id);
            sg.state.item_list = vec![BagItem {
                make_index,
                index: item_index,
                dura: 10,
                dura_max: 20,
                desc: [0; 14],
            }];
            sg.state.wabil.weight = ctx["wabil_before"]["weight"].as_i64().unwrap() as u16;
            sg.state.wabil.wear_weight = ctx["wabil_before"]["wear_weight"].as_i64().unwrap() as u8;
            sg.state.wabil.hand_weight = ctx["wabil_before"]["hand_weight"].as_i64().unwrap() as u8;
            sg.home = HomeState {
                home_map: ctx["home"]["map"].as_str().unwrap_or("").to_string(),
                home_x: ctx["home"]["x"].as_i64().unwrap() as i16,
                home_y: ctx["home"]["y"].as_i64().unwrap() as i16,
                red_home_map: ctx["red_home"]["map"].as_str().unwrap_or("").to_string(),
                red_home_x: ctx["red_home"]["x"].as_i64().unwrap() as i16,
                red_home_y: ctx["red_home"]["y"].as_i64().unwrap() as i16,
            };
            sessions.insert("hero".into(), sg);
            let mut online: OnlineRegistry = OnlineRegistry::new();
            online_set(
                &mut online,
                OnlineEntry {
                    account: "ua".into(),
                    chr_name: "hero".into(),
                    session_id: 1,
                    gate_idx: 0,
                    socket: 901,
                    actor_id,
                },
            );
            let (tx, mut rx) = mpsc::unbounded_channel::<SimToGate>();
            let mut gates: HashMap<u8, mpsc::UnboundedSender<SimToGate>> = HashMap::new();
            gates.insert(0, tx);
            let colors = MsgColors {
                red_f: 255,
                red_b: 56,
                hear_f: 0,
                hear_b: 255,
                whisper_f: 0,
                whisper_b: 0,
                green_f: 0xDB,
                green_b: 0xFF,
                blue_f: 0xFF,
                blue_b: 0xFC,
            };
            let unbind = game_svr::unbindlist::UnbindList::default();
            let item_number = std::sync::atomic::AtomicI32::new(0);
            world.rng = RandomNumber::with_seed(seed);

            handle_eat_cmd(
                make_index,
                0,
                0,
                None,
                "hero",
                901,
                &online,
                &mut sessions,
                &gates,
                &catalog,
                &mut world,
                &colors,
                90,
                &game_svr::strings::StringConf::default(),
                Some(&reg),
                &unbind,
                &item_number,
                Some(EatProcessEnv {
                    npc_host: None,
                    npc_vars: &game_svr::npc::NpcVars::default(),
                    gm: None,
                    lottery: &crate::winlottery::WinLotteryShared::new(
                        crate::config::WinLotteryCfg::default().init_state(),
                    ),
                    castle_war_states: &war_states,
                }),
                crate::config::WinLotteryCfg::default(),
                10_000_000,
                crate::config::WeaponMakeLuckCfg::default(),
            );

            let mut displays = crate::downlink::DisplayTable::new();
            // 展示表 dir 留 ctor 值 4（陈旧投影）：产品 `drain_map_change_at_time` 的 801 Series
            // 低字节必须取**实体** `ai.dir`（夹具旋钮 5）⇒ 误取展示表投影则 4≠5，对拍红。
            displays.set(
                actor_id,
                crate::downlink::ActorDisplay::new(4, 0, 0, 0, 0, 255, b"hero".to_vec()),
            );
            crate::drain_map_change(&mut world, &online, &gates, &displays, Some(&reg));

            let hex =
                |b: &[u8]| -> String { b.iter().map(|x| format!("{x:02x}")).collect::<String>() };
            let mut got: Vec<(i64, i64, i64, i64, i64, String)> = Vec::new();
            while let Ok(SimToGate::SendFrame { body, .. }) = rx.try_recv() {
                let m = CommandMessage::from_bytes(&body).expect(">=12B");
                got.push((
                    m.ident as i64,
                    m.recog as i64,
                    m.param as i64,
                    m.tag as i64,
                    m.series as i64,
                    hex(&body[CommandMessage::SIZE..]),
                ));
            }

            // 期望 = immediate ++ pumped（modeled 内）+ 逐支的 SysMsg 帧（逐字节文本）。
            let golden_actor = ctx["actor_id"].as_i64().unwrap();
            let five = |f: &serde_json::Value| {
                let recog = f["recog"].as_i64().unwrap();
                (
                    f["ident"].as_i64().unwrap(),
                    if recog == golden_actor {
                        i64::from(actor_id)
                    } else {
                        recog
                    },
                    f["param"].as_i64().unwrap(),
                    f["tag"].as_i64().unwrap(),
                    f["series"].as_i64().unwrap(),
                    f["tail"].as_str().unwrap_or("").to_string(),
                )
            };
            let name = c["name"].as_str().unwrap();
            let sysmsg = |text: &str| -> (i64, i64, i64, i64, i64, String) {
                let body =
                    crate::smframe::sm_sysmessage(actor_id, 255, 56, &crate::enter::gbk(text));
                let m = CommandMessage::from_bytes(&body).expect("SysMsg 可解析");
                (
                    m.ident as i64,
                    m.recog as i64,
                    m.param as i64,
                    m.tag as i64,
                    m.series as i64,
                    hex(&body[CommandMessage::SIZE..]),
                )
            };
            let mut want: Vec<(i64, i64, i64, i64, i64, String)> = Vec::new();
            for f in expect["immediate_frames"].as_array().unwrap() {
                want.push(five(f));
            }
            let mut dropped: Vec<i64> = Vec::new();
            for f in expect["pumped"].as_array().unwrap() {
                let (ident, ..) = five(f);
                if modeled.contains(&ident) {
                    want.push(five(f));
                } else {
                    dropped.push(ident);
                }
            }
            // 只有**真的换图**的两支才会带 766/54（`RM_CHANGEMAP` 处理器的连带帧）；
            // 不吃/无效/战区三支 `pumped` 为空或只有 622 ⇒ 剔除集必须为空。
            let want_gap: Vec<i64> = if expect["pumped"]
                .as_array()
                .unwrap()
                .iter()
                .any(|p| p["ident"].as_i64() == Some(766))
            {
                registered_gap.to_vec()
            } else {
                Vec::new()
            };
            assert_eq!(dropped, want_gap, "{name}: 剔除集必须为空（766/54 已接线）");
            if name == "eat_fail_shape5_in_war_area" {
                want.push(sysmsg("此处无法使用"));
            }
            if name == "eat_ok_shape5_invalid_castle_eaten" {
                want.push(sysmsg("无效"));
            }
            assert_eq!(got, want, "{name}: 线上帧严格序（逐字段 + 红字文本尾）");

            // 终态 X/Y（实体 + 会话）。
            let final_map = expect["final"]["map"].as_str().unwrap();
            let want_map_id = reg.id_of(final_map).expect("终态图在注册表");
            let ent = world.entities.get(actor_id).expect("实体还在");
            assert_eq!(
                (ent.map_id, ent.x, ent.y),
                (
                    want_map_id,
                    expect["final"]["x"].as_i64().unwrap() as i32,
                    expect["final"]["y"].as_i64().unwrap() as i32
                ),
                "{name}: 终态 MapName/X/Y（实体）"
            );
            let sess = world.session("hero").expect("会话");
            assert_eq!(sess.map_id, want_map_id, "{name}: 终态图（会话）");

            // 背包 / 重量：**逐件整 `BagItem` 比**（无解包/无墙钟 `make_index`）。
            assert_eq!(
                sessions["hero"].state.item_list,
                golden_bag_items(&expect["item_list"]),
                "{name}: 背包终态（逐件整 BagItem）"
            );
            assert_eq!(
                i64::from(sessions["hero"].state.wabil.weight),
                expect["weight"].as_i64().unwrap(),
                "{name}: WAbil.Weight"
            );

            // RNG 逐次 + 无多抽。
            let golden_draws = expect["rng"].as_array().unwrap();
            assert_eq!(
                expect["rng_draw_count"].as_i64().unwrap(),
                golden_draws.len() as i64,
                "{name}: 记录器计数与账本不一致（夹具坏了）"
            );
            let mut replay = RandomNumber::with_seed(seed);
            for d in golden_draws {
                let arg = d["arg"].as_i64().unwrap() as i32;
                let value = d["value"].as_i64().unwrap() as i32;
                assert_eq!(replay.random_below(arg), value, "{name}: 逐次取数");
            }
            assert_eq!(
                world.rng.random_below(1000),
                replay.random_below(1000),
                "{name}: 移动后不得多抽（流位置对齐）"
            );
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    fn war_state_in_war() -> mir2_world_bevy::castle::CastleWarState {
        let mut st = mir2_world_bevy::castle::CastleWarState::new(0);
        st.in_guild_war_area = true;
        st
    }

    /// **IT-13 五支总跑**。
    #[test]
    fn it13_guild_return_host_parity() {
        run_guild_parity(None);
    }

    /// **IT-13 逐支单跑**（变异牙要「一支一支红」的原文）。
    #[test]
    fn it13_no_guild_no_eat() {
        run_guild_parity(Some("eat_fail_shape5_no_guild"));
    }

    #[test]
    fn it13_in_war_area_red_no_eat() {
        run_guild_parity(Some("eat_fail_shape5_in_war_area"));
    }

    #[test]
    fn it13_invalid_castle_eaten_red() {
        run_guild_parity(Some("eat_ok_shape5_invalid_castle_eaten"));
    }

    #[test]
    fn it13_castle_home_exact() {
        run_guild_parity(Some("eat_ok_shape5_castle_home_exact"));
    }

    #[test]
    fn it13_castle_home_zero_coord_random() {
        run_guild_parity(Some("eat_ok_shape5_castle_home_zero_coord_random"));
    }

    fn registry_with_flag(
        dir: &Path,
        name: &str,
        flag: mir2_envir::mapinfo::MapInfoFlag,
    ) -> mir2_envir::MapRegistry {
        write_walkable_map(dir, name, 60, 60);
        let entries = vec![mir2_envir::mapinfo::MapInfoEntry {
            map_name: name.to_string(),
            map_file_name: name.to_string(),
            desc: String::new(),
            server_index: 0,
            flag,
        }];
        mir2_envir::MapRegistry::from_entries(mir2_envir::MapLoader::new(dir, "", ""), entries)
    }

    /// **章程 §36**：`SM_CHANGEMAP(634)` 的 `Series = DayBright()` 由 `game_time` 桶映射
    /// （`enter::day_bright`：`{1→0, 3→1, 其它→2}`；`DayLight` 恒 0；`boDarkness` 恒 1）。
    /// 生产入口读真实小时，本用例走 `drain_map_change_at_time` **注入**时钟，覆盖被
    /// 「夹具置 `DayLight=true`」避开的三个时间分支。
    ///
    /// 红检：把 `day_bright` 的 `1=>0 / 3=>1 / _=>2` 任一改错 ⇒ 对应用例红；删 `DayLight`/`boDarkness`
    /// 短路 ⇒ 对应红。
    #[test]
    fn map_change_634_day_bright_buckets() {
        use mir2_world_bevy::HomeMoveTarget;
        static SEQ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("mir2-daybright-{}-{seq}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("tempdir");

        let default_flag = || mir2_envir::mapinfo::MapInfoFlag::default();
        let cases: Vec<(mir2_envir::mapinfo::MapInfoFlag, u8, u16, &str)> = vec![
            (default_flag(), 1, 0, "game_time=1 ⇒ DayBright=0"),
            (default_flag(), 3, 1, "game_time=3 ⇒ DayBright=1"),
            (default_flag(), 2, 2, "game_time=其它 ⇒ DayBright=2"),
            (
                mir2_envir::mapinfo::MapInfoFlag {
                    day_light: true,
                    ..Default::default()
                },
                2,
                0,
                "DayLight ⇒ 恒 0",
            ),
            (
                mir2_envir::mapinfo::MapInfoFlag {
                    bo_darkness: true,
                    ..Default::default()
                },
                2,
                1,
                "boDarkness ⇒ 恒 1",
            ),
        ];
        for (flag, bucket, want, label) in cases {
            let reg = registry_with_flag(&dir, "move_map", flag);
            let mut world = mir2_world_bevy::World::with_maps(crate::mapwire::sizes(&reg));
            world.open_session("hero");
            let mut e = mir2_world_bevy::EntitySpawn::new(0, "hero", 5, 5);
            e.race = mir2_world_bevy::ACTOR_RACE_PLAY;
            let actor_id = world.enter_map("hero", e, 0).expect("进图");
            // 记一条 `MapChange{space_move_show: Some(false)}`（同图移动也走 switch_map_space）。
            let _ = world.base_object_move_player(
                "hero",
                actor_id,
                Some(HomeMoveTarget::Fixed {
                    map_id: 0,
                    x: 6,
                    y: 6,
                }),
            );
            let mut online: OnlineRegistry = OnlineRegistry::new();
            online_set(
                &mut online,
                OnlineEntry {
                    account: "ua".into(),
                    chr_name: "hero".into(),
                    session_id: 1,
                    gate_idx: 0,
                    socket: 901,
                    actor_id,
                },
            );
            let (tx, mut rx) = mpsc::unbounded_channel::<SimToGate>();
            let mut gates: HashMap<u8, mpsc::UnboundedSender<SimToGate>> = HashMap::new();
            gates.insert(0, tx);
            let displays = crate::downlink::DisplayTable::new();
            crate::drain_map_change_at_time(
                &mut world,
                &online,
                &gates,
                &displays,
                Some(&reg),
                bucket,
            );
            let mut got: Option<u16> = None;
            while let Ok(ev) = rx.try_recv() {
                if let SimToGate::SendFrame { body, .. } = ev {
                    let m = CommandMessage::from_bytes(&body).expect(">=12B");
                    if m.ident == 634 {
                        got = Some(m.series);
                    }
                }
            }
            assert_eq!(got, Some(want), "{label}");
        }
        std::fs::remove_dir_all(&dir).ok();
    }
}
