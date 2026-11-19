//! **`SendRefMsg` 的 500ms `VisibleHumanList` 缓存**（偏差 `T-287` 的产品实现；卡
//! `openmir2-visiblehumanlist-cache`）。
//!
//! 语义与实现在 `mir2_world_bevy::World::ref_msg_targets_from_scan`（缓存按**世界**持有，
//! 避免线程级全局状态在测试/多实例世界间串味）；本模块只提供宿主侧的两个入口：
//!
//! - [`targets`]：扫描路径（`chat::hear_targets` 算 ±range 闭区间，再走缓存两段）；
//! - [`targets_from_scan`]：调用方已算好扫描结果（世界侧商人消失帧的收件人快照等）。
//!
//! oracle 锚：`src/M2Server/Actor/BaseObject.Message.cs:395-466`（`>=500` 重扫 ±12 闭区间；
//! 否则复用 `VisibleHumanList` + `|Δ|<11` 严格小于）。

use mir2_world_bevy::World;

// 常量透传给测试（rig 用 `REF_MSG_CACHE_TTL_MS` 模拟 ≥500ms 间隔）。
#[allow(unused_imports)]
pub use mir2_world_bevy::{REF_MSG_CACHED_RANGE, REF_MSG_CACHE_TTL_MS};

/// 扫描路径：`chat::hear_targets`（±range 闭区间）→ 缓存两段。
#[must_use]
pub fn targets(world: &World, sender_id: i32, range: i32) -> Vec<i32> {
    let scanned = crate::chat::hear_targets(world, sender_id, range);
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
    /// 牙：把 `< REF_MSG_CACHED_RANGE` 放宽成 `<=` ⇒ 22 进第二帧 ⇒ 红。
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
}
