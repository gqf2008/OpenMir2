//! **`SendRefMsg` 的 500ms `VisibleHumanList` 缓存**（偏差 `T-287` 的产品实现；卡
//! `openmir2-visiblehumanlist-cache`）。
//!
//! 语义与实现在 `mir2_world_bevy::World::ref_msg_targets_from_scan`（缓存按**世界**持有，
//! 避免线程级全局状态在测试/多实例世界间串味）；本模块只提供宿主侧的三个入口：
//!
//! - [`targets`]：扫描路径（`chat::hear_targets` 算 ±range 闭区间，再走缓存两段）；
//! - [`targets_at`]：扫描路径的显式坐标版（逐格 `RM_PUSH` 等按该格算收件人）；
//! - [`targets_from_scan`]：调用方已算好扫描结果（世界侧商人消失帧的收件人快照等）。
//!
//! **隐藏发送者早退**（[`targets`] / [`targets_at`]）：oracle `SendRefMsg` 在
//! `ObMode || FixedHideMode` 时只 `SendMsg` 自己并 **return**，早于
//! `SendRefMsgTick`/`VisibleHumanList` 的刷新判定（`BaseObject.Message.cs` 的 `SendRefMsg`
//! 函数体）⇒ 隐藏期间既不发观察者帧，也不推进缓存时钟。快照入口
//! [`targets_from_scan`] **不做**这个早退：它的收件人是发帧时刻的快照
//! （`drain_merchant_disappears` 在排水时 sender 已隐藏属设计前提）。
//!
//! oracle 锚：`src/M2Server/Actor/BaseObject.Message.cs:395-466`（`>=500` 重扫 ±12 闭区间；
//! 否则复用 `VisibleHumanList` + `|Δ|<11` 严格小于）。

use mir2_world_bevy::World;

// 常量透传给测试（rig 用 `REF_MSG_CACHE_TTL_MS` 模拟 ≥500ms 间隔）。
#[allow(unused_imports)]
pub use mir2_world_bevy::{REF_MSG_CACHED_RANGE, REF_MSG_CACHE_TTL_MS};

/// 隐藏发送者早退（只服务扫描路径）：`ObMode || FixedHideMode` ⇒ 收件人只有自己，
/// **且不碰缓存**（oracle 的该分支在 `SendRefMsgTick`/`VisibleHumanList` 刷新判定之前 return）。
///
/// 返回 `None` = sender 不存在或未隐藏 ⇒ 走正常扫描；`Some(vec![sender_id])` = 命中早退。
fn hidden_sender_only(world: &World, sender_id: i32) -> Option<Vec<i32>> {
    let sender = world.entities.get(sender_id)?;
    (sender.ob_mode || sender.fixed_hide_mode).then(|| vec![sender_id])
}

/// 扫描路径：`chat::hear_targets`（±range 闭区间）→ 缓存两段。
/// 隐藏发送者按 oracle 早退（只自己、不推进缓存）。
#[must_use]
pub fn targets(world: &World, sender_id: i32, range: i32) -> Vec<i32> {
    if let Some(self_only) = hidden_sender_only(world, sender_id) {
        return self_only;
    }
    // **cache-first（M53）**：命中即按旧快照过滤返回，**不扫描**；未命中才扫描并刷新。
    if let Some(cached) = world.ref_msg_targets_cached(sender_id) {
        return cached;
    }
    let scanned = crate::chat::hear_targets(world, sender_id, range);
    world.ref_msg_targets_from_scan(sender_id, scanned)
}

/// 扫描路径的**显式坐标**版：收件人按传入的 `(map_id, x, y)` 算（见
/// [`crate::chat::hear_targets_at`]）；缓存两段与隐藏发送者早退同 [`targets`]。
#[must_use]
pub fn targets_at(
    world: &World,
    sender_id: i32,
    map_id: usize,
    x: i32,
    y: i32,
    range: i32,
) -> Vec<i32> {
    if let Some(self_only) = hidden_sender_only(world, sender_id) {
        return self_only;
    }
    // **cache-first（M53）**：同 [`targets`]；命中不扫描（显式坐标只用于刷新路径的扫描面）。
    if let Some(cached) = world.ref_msg_targets_cached(sender_id) {
        return cached;
    }
    let scanned = crate::chat::hear_targets_at(world, sender_id, map_id, x, y, range);
    world.ref_msg_targets_from_scan(sender_id, scanned)
}

/// 已扫描路径：调用方给出"本次应得的收件人"，由世界侧缓存决定刷新/过滤。
#[must_use]
pub fn targets_from_scan(world: &World, sender_id: i32, scanned: Vec<i32>) -> Vec<i32> {
    world.ref_msg_targets_from_scan(sender_id, scanned)
}

/// 清空本世界缓存（测试夹具在同一世界里跑多例时使用）。
#[cfg(test)]
pub fn reset(world: &World) {
    world.reset_ref_msg_cache();
}

#[cfg(test)]
mod tests {
    use super::*;
    use mir2_world_bevy::{EntitySpawn, ACTOR_RACE_PLAY};

    fn world_with_sender_and_players() -> (World, i32, Vec<i32>) {
        let mut w = World::with_maps(vec![(64, 64)]);
        let mut npc = EntitySpawn::new(0, "npc", 10, 10);
        npc.race = mir2_world_bevy::ACTOR_RACE_NPC;
        let sender = w.add_entity(npc);
        let mut ids = Vec::new();
        for (i, (x, y)) in [(10, 10), (15, 10), (22, 10), (23, 10)]
            .into_iter()
            .enumerate()
        {
            let mut e = EntitySpawn::new(0, format!("p{i}"), x, y);
            e.race = ACTOR_RACE_PLAY;
            ids.push(w.add_entity(e));
        }
        (w, sender, ids)
    }

    /// 首扫全量（`+12` 闭区间含 22）；同窗口第二次只剩 `|Δ|<11`（22/23 被挡）。
    /// 牙：**删掉二次过滤**（缓存命中时直接返回整份快照）⇒ 第二帧含 +12 ⇒ 红
    /// （`< → <=` 不咬：`12 <= 11` 仍为假，两边行为相同——该写法已弃用）。
    #[test]
    fn ref_msg_cache_first_scan_then_strict_11_on_reuse() {
        let (mut w, sender, ids) = world_with_sender_and_players();
        let first = targets(&w, sender, 12);
        assert_eq!(
            first,
            vec![ids[0], ids[1], ids[2]],
            "首扫：±12 闭区间 ⇒ +0/+5/+12 进列表，+13 在范围外"
        );
        w.clock_ms += 100; // 仍在 500ms 窗口内
        let second = targets(&w, sender, 12);
        assert_eq!(
            second,
            vec![ids[0], ids[1]],
            "缓存命中：只剩 |Δ|<11 的 +0/+5（+12/+13 被严格小于挡掉）"
        );
    }

    /// 500ms 到期后重新扫格全量恢复。牙：把 TTL 判据改成永不过期 ⇒ 本段红。
    #[test]
    fn ref_msg_cache_expires_after_500ms() {
        let (mut w, sender, ids) = world_with_sender_and_players();
        let _ = targets(&w, sender, 12);
        w.clock_ms += 500;
        let refreshed = targets(&w, sender, 12);
        assert_eq!(
            refreshed,
            vec![ids[0], ids[1], ids[2]],
            ">=500 ⇒ 重扫，+12 收件人回到列表"
        );
    }

    /// **隐藏发送者早退**（oracle `SendRefMsg` 的 `ObMode || FixedHideMode` 分支在
    /// `SendRefMsgTick`/`VisibleHumanList` 刷新**之前** return，只 `SendMsg` 自己）：
    /// 隐藏期间收件人只有自己，**且缓存不被改写**。
    ///
    /// 牙（双向）：① 去掉 `hidden_sender_only` 早退 ⇒ 隐藏调用命中旧快照 ⇒ 拿到 +0/+5
    /// 观察者 ⇒ 第二段红；② 早退保留但把 `[self]` 写进缓存 ⇒ 解除隐身后读到 `[self]`
    /// ⇒ +5 收不到 ⇒ 第三段红（专钉「缓存不刷新」）。
    #[test]
    fn hidden_sender_returns_self_only_without_refreshing_cache() {
        let (mut w, sender, ids) = world_with_sender_and_players();
        let first = targets(&w, sender, 12);
        assert_eq!(
            first,
            vec![ids[0], ids[1], ids[2]],
            "预热缓存（+0/+5/+12 进 VisibleHumanList）"
        );
        w.entities.flags_mut(sender).unwrap().ob_mode = true;
        let hidden = targets(&w, sender, 12);
        assert_eq!(hidden, vec![sender], "隐藏发送者只发自己（oracle 早退）");
        w.entities.flags_mut(sender).unwrap().ob_mode = false;
        let after = targets(&w, sender, 12);
        assert_eq!(
            after,
            vec![ids[0], ids[1]],
            "解除隐身后仍在 500ms 窗口 ⇒ 复用隐身前的快照 + |Δ|<11（+0/+5；+12 被挡）\
             ⇒ 冻结「隐藏期间不刷新缓存」"
        );
    }

    /// **隐藏发送者早退（显式坐标入口 [`targets_at`]）**：与 [`targets`] 同一条 oracle
    /// 分支（逐格 `RM_PUSH` 等按该格算收件人的调用走这里）。
    ///
    /// 牙：只删 `targets_at` 里的早退、保留 `targets` 的 ⇒ 本用例红（钉住两个入口各自接线）。
    #[test]
    fn hidden_sender_returns_self_only_through_targets_at() {
        let (mut w, sender, ids) = world_with_sender_and_players();
        let first = targets_at(&w, sender, 0, 10, 10, 12);
        assert_eq!(
            first,
            vec![ids[0], ids[1], ids[2]],
            "预热：显式坐标 (10,10) 与实体坐标一致"
        );
        w.entities.flags_mut(sender).unwrap().fixed_hide_mode = true;
        assert_eq!(
            targets_at(&w, sender, 0, 10, 10, 12),
            vec![sender],
            "FixedHideMode ⇒ 只自己"
        );
        w.entities.flags_mut(sender).unwrap().fixed_hide_mode = false;
        assert_eq!(
            targets_at(&w, sender, 0, 10, 10, 12),
            vec![ids[0], ids[1]],
            "解除后 <500ms 复用预热快照（+0/+5；+12 被 |Δ|<11 挡）"
        );
    }

    /// **M53 边角②+可观测**：命中分支只看 `now-tick<TTL && visible 非空`，且**命中路径不扫描**
    /// （`ref_msg_scan_total` 增量 = 0）；首扫路径增量 = 1。
    #[test]
    fn cache_first_hit_does_not_scan() {
        let (mut w, sender, ids) = world_with_sender_and_players();
        let before = w.ref_msg_scan_total();
        let first = targets(&w, sender, 12);
        assert_eq!(first, vec![ids[0], ids[1], ids[2]], "首扫");
        assert_eq!(w.ref_msg_scan_total() - before, 1, "首扫路径恰好扫描 1 次");
        w.clock_ms += 100;
        let since_hit = w.ref_msg_scan_total();
        let second = targets(&w, sender, 12);
        assert_eq!(second, vec![ids[0], ids[1]], "命中：旧快照 + |Δ|<11");
        assert_eq!(
            w.ref_msg_scan_total() - since_hit,
            0,
            "命中路径必须**不扫描**（M53 可观测证据）"
        );
    }

    /// **M53 边角①**：sender 不存在且缓存新鲜 ⇒ 返回空，且**不推进 tick**（也不扫描）。
    #[test]
    fn cache_first_missing_sender_fresh_returns_empty_without_tick() {
        let (mut w, sender, _ids) = world_with_sender_and_players();
        let _ = targets(&w, sender, 12);
        let tick_before = w.ref_msg_cache_tick(sender).expect("已刷新");
        w.entities.remove(sender);
        let scans_before = w.ref_msg_scan_total();
        assert!(targets(&w, sender, 12).is_empty(), "sender 缺失 ⇒ 空");
        assert_eq!(
            w.ref_msg_cache_tick(sender),
            Some(tick_before),
            "命中分支不推进 tick"
        );
        assert_eq!(w.ref_msg_scan_total() - scans_before, 0, "也不扫描");
    }

    /// **M53 边角②（visible 为空必须刷新）**：首扫无收件人 ⇒ `visible` 空；随后加入玩家，
    /// 同窗口内再调用**必须重新扫格**（而不是把「空快照」当命中返回空）。
    #[test]
    fn cache_first_empty_visible_forces_rescan() {
        let mut w = World::with_maps(vec![(64, 64)]);
        let mut src = EntitySpawn::new(0, "src".to_string(), 10, 10);
        src.race = mir2_world_bevy::ACTOR_RACE_NPC;
        let sender = w.add_entity(src);
        assert!(targets(&w, sender, 12).is_empty(), "无玩家 ⇒ 空");
        let mut p = EntitySpawn::new(0, "p".to_string(), 15, 10);
        p.race = ACTOR_RACE_PLAY;
        let pid = w.add_entity(p);
        assert_eq!(
            targets(&w, sender, 12),
            vec![pid],
            "visible 为空必须重扫（不能把空快照当命中）"
        );
    }

    /// **M53 边角③**：隐藏 sender 早退——不扫描、不刷新（无缓存条目）、只返回自己。
    #[test]
    fn cache_first_hidden_sender_early_exit() {
        let (mut w, sender, _ids) = world_with_sender_and_players();
        w.entities.flags_mut(sender).unwrap().ob_mode = true;
        let scans = w.ref_msg_scan_total();
        assert_eq!(targets(&w, sender, 12), vec![sender], "隐藏 ⇒ 只自己");
        assert_eq!(w.ref_msg_scan_total() - scans, 0, "早退不扫描");
        assert_eq!(w.ref_msg_cache_tick(sender), None, "早退不刷新缓存");
    }

    /// **M53**：显式坐标入口 `targets_at` 同样 cache-first（命中不扫描）。
    #[test]
    fn cache_first_targets_at_hit_does_not_scan() {
        let (mut w, sender, ids) = world_with_sender_and_players();
        let _ = targets_at(&w, sender, 0, 10, 10, 12);
        w.clock_ms += 100;
        let scans = w.ref_msg_scan_total();
        assert_eq!(
            targets_at(&w, sender, 0, 10, 10, 12),
            vec![ids[0], ids[1]],
            "命中：旧快照 + |Δ|<11"
        );
        assert_eq!(w.ref_msg_scan_total() - scans, 0, "命中不扫描");
    }
}
