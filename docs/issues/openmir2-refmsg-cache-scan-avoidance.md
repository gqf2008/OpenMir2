# openmir2-refmsg-cache-scan-avoidance

> 状态：**follow-up 登记卡（2026-11-20，opc-07 发布）**。触发：C2 批 2（`5b53ce3b7`）QA 签署时的**强制登记项**——性能代价已随批次放大，但合并不受阻。
> 关联：`docs/issues/openmir2-visiblehumanlist-cache.md`、`docs/issues/openmir2-refmsg-cache-residual-sites.md`、章程 §19（记录面必须进比较面）

## 1. 现状（事实，带锚）

- `ref_msg_cache::targets(world, actor_id, range)` 在**缓存命中时仍先跑** `chat::hear_targets` 的**全实体扫描**，再用旧快照过滤；oracle 的 `SendRefMsg` 在 `VisibleHumanList` 有效期内**不扫描**。
- C2 批 1/批 2 把调用频率放大：`drain_stamina` 的"目标集下沉到每份副本内"使同 actor 同 tick 扫描 **≤2–3 次**。
- 结论：**行为正确、代价增加**。QA 判定为非阻塞合并项。

## 2. 目标

提供 **cache-first** 的只读入口（例如 `World::ref_msg_targets_cached(sender) -> Option<Vec<i32>>`）：命中即按旧快照过滤返回、**不扫描**；未命中才扫描并刷新缓存。**行为必须逐字不变**。

## 3. 必须照抄的边角（缺一即错）

1. **sender 不存在且缓存新鲜** ⇒ 返回空，且**不推进 tick**；
2. 命中分支只看 `now - tick < TTL` **且** `visible` 非空；
3. 隐藏 sender（`ObMode || FixedHideMode`）走**早退**分支（`4174de804` 的前置语义），不读缓存、不刷新。

## 4. 验收

- **复跑全部 C2 站点用例**（批 1/批 2/批 3–6），不是只跑本卡自己的牙；
- 上述三条边角各一条**显式用例**；
- 必须给"命中路径**没有扫描**"的**可观测证据**（扫描计数钩子或等价断言）——**"行为不变"不等于"真的没扫"**（章程 §19）；
- 落地时机：**C2 批 3–6 全部落地并签署之后**；在此之前保持"原位替换 + 每次扫描"的形态，避免反复重基线。

## 5. 不做

- 不改 oracle 语义、不改帧面、不顺手改 `TTL` 或 `|Δ|<11` 判据（那属对拍口径，须另卡评审）。

## 6. 实现（2026-11-20；证据待 lane）

- **新入口**：`World::ref_msg_targets_cached(sender_id) -> Option<Vec<i32>>`：
  命中（`now - tick < TTL` **且** `visible` 非空）即按旧快照过滤返回（`map/!ghost/|Δ|<11`），
  **不扫描、不推进 tick**；`None` = 需扫描并刷新。
  边角①：sender 不存在且缓存新鲜 ⇒ `Some(vec![])`（不推进 tick）。
  边角③：隐藏 sender 由宿主 `ref_msg_cache` 的 `hidden_sender_only` **早退**（不读缓存/不刷新）。
- **宿主**：`ref_msg_cache::targets` / `targets_at` 在隐藏早退之后先调 `ref_msg_targets_cached`，
  命中直接返回；未命中才 `chat::hear_targets*` + `ref_msg_targets_from_scan`（刷新）。
  `targets_from_scan`（快照路径）**不变**。
- **可观测证据**：`World::ref_msg_scan_total()`（`chat::hear_targets_at` 每次 `+1`）——
  用例 `cache_first_hit_does_not_scan` 断言命中路径增量 **0**、首扫增量 **1**；
  另有 `cache_first_targets_at_hit_does_not_scan`。
- **三条边角用例**：`cache_first_missing_sender_fresh_returns_empty_without_tick`（①，含 tick 不变 + 不扫描）、
  `cache_first_empty_visible_forces_rescan`（②空快照必须重扫）、`cache_first_hidden_sender_early_exit`（③不扫描/无条目）。
- **验收（2026-11-20 证据回填）**：复跑全部 C2 站点用例（批1–7 + #19/#20），`cargo test -p game-svr --lib`
  结果 **2060 passed / 0 failed / 55 ignored**；本卡 5 条 `cache_first_*` 用例与边角三条用例全 `ok`；
  首扫扫描增量 **1** / 命中增量 **0**（用例断言实测，非人工观察）；牙 m46 / m46b / m47b / m47c / m48 /
  m49 / m49b 均红并可还原，`m47`（缺失 sender 改 `None`）判**无效牙**——变异体下仍绿 ⇒ 与现行为等价。
- **放置理由（§4.2，2026-11-20 回填）**：缓存本体 `SimState.ref_msg_cache`、TTL 与 `|Δ|<11` 谓词、
  `clock_ms` 都在 `World`；命中过滤要读 `entities`（`map_id`/`ghost`/坐标）并与
  `ref_msg_targets_from_scan` **共用同一份谓词**——放宿主层就得暴露世界状态并复制一份对拍敏感谓词
  （两份会漂）。宿主层只做转接；计数 `note_ref_msg_scan()` 必须接在 world 侧扫描入口
  `chat::hear_targets_at` 上，否则就是旁路打点。同一段文字已写在 `world.rs` 该函数头上。
