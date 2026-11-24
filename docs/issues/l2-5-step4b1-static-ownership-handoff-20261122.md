# L2-5 ④b-1 交接：静态实体按区归属（2026-11-22）

> **交接边界**：本文档只交接、不含未完成的半成品代码。撰写会话 ④b-0 已完成（已合 master
> `f19e19ab0`：ActorId 段撞车修复 + 遮蔽两列拆分 + 外来 id 归属断言）并把 ④b 主项的**接缝调查与
> 分段**写进计划页 §12.7（提交 `8f308ab0a`，分支 `card/l25-step4b-static-ownership`）。
> ④b-1 的产品代码**尚未开始**——按总监批准的方案直接续做即可。

## 0. 已批准的范围（总监 2026-11-22）

**④b-1（本片）**：`MonGen 首刷 + 商人 + 固定 NPC + 守卫` 按 `zone_of` 归属过滤；
**④b-2（下一片）**：ctor-fold 的 per-map 站点（`MapCheckQuest`/`MapQuestRow`）、隐藏 NPC 实体、
城堡实体在区 k 的归属/可见性；④d 的「全局产生 + 分发」解除条件。

**语义写死（计划页 §12.7，照做）**
1. `slicing=0` / N=1 **不调用**新过滤/装载路径（不是"结果相同"）⇒ N=1 两树 sha 必须与
   ④a/④b-0 **同一对**（`order_sha=b78156a7…` / `multiset_sha=bd4b13ba…`）。
2. 2 区与 N=1 的 MonGen 抽数**不等价**（被过滤 entry 不消耗抽数）——有意偏离，解除=④d。
3. **正面断言**：2 区下区 0 的抽数计数 **<** N=1（禁"先全量抽再丢"）。
4. `MonGenHost` **每区一份**，用各区自己的 `seed ⊕ zone` RNG（不共享流）。
5. 未被任何 rect 覆盖的 321 张图**显式归区 0**（代码注释 + 文档写死，禁隐式默认）。
6. **脚本宿主保持进程级单实例共享**：区 k 装载只**创建实体并登记**，不新建宿主、不重复初始化
   脚本 VM；装载函数签名显式带宿主依赖（`&mut merchant_book` / `&mut npc_host`），不许在函数内
   去全局取；若发现必须按区隔离的脚本副作用，**先列清单上卡**再改。

## 1. 接缝事实（已调查，直接可用）

`sim_loop` 启动段的静态装载序列（与 RNG 屏障严格耦合）：
`ctor fold 段① → 屏障 A(1 抽) → 段② → MonGenHost::start（屏障 B）→ 段③ → LoadMerchant →
NpCinitialize（固定 NPC + 脚本）→ 隐藏 NPC → GuardList → 城堡（宿主限定）`。

- **MonGen**：`MonGenHost::start(entries, table, sentinel, config, &mut world.rng)` 的 `entries`
  即过滤点——**过滤 entries 再 start** 即"先过滤后抽"；区 k 传入自己的 `entries` + 自己的 rng。
- **固定 NPC**：`enter_cfg.npcs`（`Vec`）→ `npcs::place_fixed_npcs_with_scripts(&mut world, &npcs, …)`
  ——按 NPC 的 `(map_name,x,y)` 先过滤 `Vec` 再调用；区 k 装载时脚本登记仍走共享宿主（见 §0.6）。
- **守卫**：`enter_cfg.guards`（`Vec`）→ `guardlist::place_guards(&guards, …, host, …)`——
  同样先按 `(map_name,x,y)` 过滤；守卫放置依赖 `mon_gen_host`（区 k 必须用**自己的** host）。
- **商人（有 API 缺口，见 §2）**：`merchant::load_merchant_spawns(loader, resolve_map, &mut rng)`。

## 2. 商人装载的 API 缺口（④b-1 必做的前置）

`merchant::load_merchant_spawns` 的过滤闭包只按**地图名**解析（`resolve_map(&str) -> Option<usize>`），
而 `MapLoader::load_merchants_loaded(&mut rename_rand)` **在解析每条记录时就抽 `rename_rand`**
（`ScriptParsers.cs:857` 的 `GetRandomNumber(1,200)`）。要满足「被过滤 entry 不消耗抽数」，
必须在**抽数之前**按 `(map_name,x,y)` 过滤：

- 修法：给 `mir2_envir` 的商家解析入口加一个 `skip: impl Fn(&record) -> bool`（或等价谓词），
  在 `rename_rand` 调用**之前**判区；`load_merchant_spawns` 透传该谓词。
- 判据：谓词恒假时商人装载**零抽数**（用 `rng_trace`/已抽计数断言；对照 = 现网 `MIR2_RNG_SEED`）。
- 不要在 `load_merchant_spawns` 之后过滤 `spawns`（抽数已发生）。

## 3. 区 k 的装载落点与构造

- `new_zone_state`（`run_inner` 建区处，早于区线程）已有：`ZoneState::new` + 边界 + 静态**地图**数据。
  ④b-1 在此**追加**：`spawn_statics_for_zone(&mut state.world, &mut state.displays, zone 谓词, 输入, 宿主)`。
- 输入准备：在 `run_inner` 里按 `layout` 为每区预过滤 `mon_gen.entries` / `npcs` / `guards`（与
  shard 过滤**叠加**；`MIR2_SHARDS=1` 时只多一层 zone 谓词）；重表用 `Arc` 共享，不复制。
- 区 0 的 `sim_loop`：现有装载段改为用 HostZone 谓词（同一谓词函数），并把未被任何 rect 覆盖的
  图显式归 HostZone。
- `slicing=0`：谓词参数为 `None`，**不构造**过滤/新路径（N=1 逐字节不变）。

## 4. 「同 id 恰一份」装置（总监指定三条）

合成 2 区布局 + 合成 `.map`/`MapInfo` 注册表；同一装载函数分别跑两个世界（区 0=HostZone、
区 1=rect），收集全静态 `(id, name, map, x, y)`：
1. 两集合**交集 = 0**；
2. **并集 == N=1 全量装载集合**（无丢无重；**含"未覆盖 321 图→区 0"那一块**）；
3. 每条落点在其区 rects 内（或显式命中"未覆盖→区 0"规则）。

变异（各自只红一条）：谓词恒真 ⇒ ①红；删"未覆盖→区 0" ⇒ ②红；先全量抽再过滤 ⇒ RNG 正面断言红。

## 5. 1k 判据（走廊铺位，起栈前报窗口）

`mirror_shadowed_static==0`、`handoff_refused==0`、`cross_recipient_missing` 不变差、
`ΔRSS(2区−1区)` **方向性预期下降**（静态不再由区 0 双份模拟——若未降要解释）。
同轮报三缺口计数与 `mirror_shadowed_static/handoff` 两分量。

## 6. 开工前必读

- 计划页 §12.6（④b-0 根因/修法/牙）与 §12.7（接缝与语义写死）。
- 变异还原**禁用 `git checkout`**（未提交工作会退回 HEAD）：用内存原件写回 + sha 相等断言。
- 起栈跑 1k 前先报总监窗口；门与 push 分两步。
