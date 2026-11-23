# M1b 第 5 步：8 族对拍规划（2026-11-23）

> 状态：**规划稿（只读，不含实现）**。基线 master `f7dd7385`（第 1–4 步已落地；A6 收口于
> `2ba3776fb` 的 cherry-pick）。本文件供 `a7` 直接排窗口、供 Leader 决定"下一步补接线还是补测试"。
> 依据：`docs/issues/l25-m1b-plan-20261123.md` §2.2（归属表）与 §12.1（M1b 命令集）；
> 帧面装置 = `tools/probe/`（驱动）+ `tools/conformance/compare_runs.py`（比较器）+ 两区栈
> （`MIR2_ZONES=2` + `MIR2_ZONE_MIRROR=1` + `MIR2_ZONE_SLICING=1`，见
> `docs/issues/l25-zone-loop-m1-design-20261123.md` §两区对拍）。

## 1. 8 族清单与「区 k 今天是否已接线」

**判定口径**：区 k 的命令只有一条处置实现 `handle_zone_command_on`；它只放行
`movewire::is_move_command`（该谓词含 `is_horse_run`），其余 ident 一律
`perf_counters::note_zone_cmd_unsupported(ident)` 后 **return（丢弃）**。分类器
`zone_cmd_class` 把 8 族都归 `Local`，但**分类 ≠ 处置**——除 move 外，其余族的处置在区 k **尚未接线**。

| # | 族 | ident 判据（符号锚） | 区 k 处置现状 | 区 0 的参照实现（oracle 等价物） | 代表性帧 |
| --- | --- | --- | --- | --- | --- |
| 1 | move | `movewire::is_move_command`（`CM_TURN/WALK/RUN`）+ `movewire::is_horse_run`（3009 别名） | **已接线**（`handle_zone_command_on` → `handle_move_cmd`） | 同函数（区 0 走同一 `handle_move_cmd`） | 自身回执明文 `+GD/<tick>`；观察者 `SM_TURN(10)`/`SM_WALK(11)`/`SM_RUN(13)` |
| 2 | hit | `attackwire::is_hit_ident`（`HIT_IDENTS` 9 条） | 分类 Local、**未接线**（落 unsupported 丢弃） | 区 0 `handle_hit_cmd` | `SM_HIT(14)` / `SM_STRUCK(31)`（受击者视野内，含自己） |
| 3 | spell | `magicwire::is_spell_ident`（`CM_SPELL(3017)`） | **未接线** | 区 0 `handle_spell_cmd_with_magic` | `SM_SPELL(17)` / `SM_MAGICFIRE(638)` / `SM_STRUCK(31)`；蓝不足支 `SM_MAGICFIRE_FAIL(639)` |
| 4 | magic-key | `magicwire::is_magic_key_ident`（`CM_MAGICKEYCHANGE(1008)`） | **未接线** | 区 0 `handle_magic_key_cmd` | **零回帧**；只改 `SavedHuman.record.data.magic` 的 `Key`（落库） |
| 5 | say（非 GM） | `zone_cmd_class` 的 `CM_SAY(3030)` 且 payload **非** `@` 起（共享判据 `say_payload_is_at_command`） | **未接线** | 区 0 → `handle_say_with_saves` 普通说话支 → `smframe::sm_hear` | `SM_HEAR(40)`（AoI 收件人集合，含 500ms `RM_HEAR` 缓存） |
| 6 | query4 | `QUERY_IDENTS`（`CM_QUERYUSERNAME(80)` / `CM_QUERYUSERSTATE(82)` / `CM_QUERYUSERSET(3040)` / `CM_QUERYVAL(1065)`） | **未接线** | 区 0 `dispatch_inbound_p0` 四支（`handle_query_user_name_cmd` 等） | 80 → `SM_USERNAME/SM_GHOST`；82 → `SM_SENDUSERSTATE(751)`；3040 → **零帧**；1065 → `SM_QUERYVALUE(215)`（每条 `ValLabel` 一帧，可多帧） |
| 7 | levelrank | `enter::CM_LEVELRANK(1060)` | **未接线** | 区 0 `handle_level_rank_request_cmd` | 仅 `[Setup] LevelRankOnLogin != 0` 时回 `SM_LEVELRANK(1108)`；**oracle 无此消息号**（T-275 登记偏差）⇒ 默认两侧零帧 |
| 8 | login-notice | `mir2_protocol::messages::CM_LOGINNOTICEOK(1018)` | **未接线** | 区 0 `handle_login_notice_ok_cmd` | **零回帧**（只写 `SessionGround.client_tick`；oracle 的 SysMsg 被 RunNotice drain 吞掉） |

> **不在 8 族内**：`CM_SAY` 的 GM `@` 支是 `Process` 类（回区 0、T+1），第 4 步 A6 已接线并已对拍；
> `CM_QUERYBAGITEMS(81)` 属物品族（M1c），**不属于** query4。
>
> **结论（一句话）**：8 族里今天**只有 move 一族能直接跑两区对拍**；其余 7 族已分类为 `Local`
> 但处置未接线（被 `note_zone_cmd_unsupported` 丢弃），先跑它们只会证明"区 k 少帧"。

## 2. 每族的对拍场景

装置：`tools/probe/mir2_probe.py` 跑 `tools/probe/scenarios/*.json`（同一串 `CM_*` 打两套栈），
`tools/conformance/compare_runs.py` 比 `frames.ndjson` + 库快照。Rust 侧 = 两区切片栈
（`MIR2_ZONES=2` + `MIR2_ZONE_SLICING=1` + `MIR2_ZONE_MIRROR=1`），被驱动角色**落区 k**
（登录路由由切片栈的 `zone_router` 决定；profile 由 `a7` 的窗口提供）；oracle 侧 = 单 World C# 栈。

| # | 族 | 场景来源 | 说明 |
| --- | --- | --- | --- |
| 1 | move | **复用** `move_walk.json` | `CM_WALK` + 回执判据已是 `+GD/`（"自身走路不回 11/13"的口径已订正）；再补一条 `CM_RUN`/`CM_HORSERUN` 步骤（同一场景加 steps 或新场景 `move_run.json`）。**为什么够**：区 k 已接线，走位是唯一有现成判据的族。 |
| 2 | hit | **复用** `combat_hit.json` / `combat_strike.json` | 现场景发 `CM_HIT`/`CM_STRIKE` 并等 `SM_STRUCK`（含 `recog=self` 的挨打歧义已在 README R2-producer-map 登记）。接线后即可用。 |
| 3 | spell | **复用** `spell_cast.json`（`requires: ["magic"]`） | 需两侧 profile 都有 `magic` 段；帧序（`SM_MAGICFIRE`→`SM_SPELL`→`SM_STRUCK`）是主判据。 |
| 4 | magic-key | **需新场景** `magic_key_change.json` | 现有场景**不发** `CM_MAGICKEYCHANGE(1008)`（`spell_cast` 只放法不改 Key）。新场景：背包/魔法表里带 ≥2 条 `MagicList`，发 1008 改第 2 条的 Key ⇒ **零回帧**判据 + **库后态** `characters_magic.Key` 判据（`--require-db`）。 |
| 5 | say（非 GM） | **需新场景** `say_plain.json` | 现有 `say` 步骤（`gm_batch`/`gm_level0_gate`/`gm_time`/`p21_unequip`）**全部以 `@` 起**，走的是 Process 支、已被第 4 步覆盖。新场景：`"say": "hello"` ⇒ 等 `SM_HEAR(40)`；再补一条「前导空格的 ` hello` 不得当 GM」负例（`say_payload_is_at_command` 严格判据）。 |
| 6 | query4 | **需新场景** `query_four.json`（可拆 4 条） | 无任何场景发 80/82/3040/1065。80/82 需要前置：目标在同屏（82 要 3×3 窗口）；1065 需要脚本 `ValLabel`（`MNSval`）已由 NPC 脚本写入 ⇒ 场景里先点 NPC 跑脚本再查。**为什么现有不够**：所有场景都没有这 4 个消息号。 |
| 7 | levelrank | **需新场景** `levelrank_1060.json` | 无场景发 1060。**oracle 无此消息号** ⇒ 两侧默认都是零帧；只有把 `LevelRankOnLogin != 0` 打开才有 `SM_LEVELRANK(1108)`——**默认配置下"零帧 == 零帧"就是判据**，别用非 0 配置去比（那是 Rust-only 扩展，oracle 侧无从产生）。 |
| 8 | login-notice | **复用**（每个场景的握手） | `mir2_probe.py` 的握手第 6 步固定发 `CM_LOGINNOTICEOK(1018)`（`recog` 固定 0，D-probe-1），**所有场景**都覆盖。判据 = **零额外帧** + 不吞后续（握手后仍能收到 `SM_LOGON` 之后的帧）。 |

## 3. 判据（沿用五面；第五面为该族专属面）

统一口径（与 A6/窗口既有五面一致）：

1. **面1 帧多重集**：该族代表性 ident 的帧按规则表（`tools/conformance/rules.json` 的
   `multiset_by_tick`/逐帧）比较。**绿了说明**：帧数量与关键字段（ident/recog/param/tag/series/body
   的可比部分）两侧一致，没有多发/少发/字段漂移。
2. **面2 相对序**：按**收件人**过滤后的子序列（per-recipient subsequence）比较。**绿了说明**：
   同一收件人看到的该族帧相对顺序与 oracle 一致（对多帧族：query 的多帧、spell 的
   `SM_MAGICFIRE→SM_SPELL→SM_STRUCK`）。
3. **面3 因果链（c1 / self 桶）**：同 tick 的成组帧与"发给自己的那一帧"按 tick 归档对账。**绿了说明**：
   该族"同一玩家的因果帧"没跨 tick 拆分、没把应答发给错的收件人（例如 move 的 `+GD/` 只回自己、
   hit 的 `SM_STRUCK` 自身那条在）。
4. **面4 侧车账本（sidecar）**：`run.json`（时钟/`start_wall`/`end_wall`）与库快照
   （`--require-db` 时）比较。**绿了说明**：帧以外的副作用（magic-key 的 `characters_magic.Key`、
   levelrank 的零帧、login-notice 的 `client_tick` 不外泄）与 oracle 一致。
5. **面5 不可判定面（ambiguous）**：`compare_runs.py` 的 `UNCLASSIFIED`/`UNDECIDABLE` 必须为 **0**
   （rc=2 不算绿）。**绿了说明**：该族用到的每个 ident 都在规则表里有覆盖、每个字段都能判定，
   不存在"没比"的面。

**族专属面（第五面之后每族加一条）**：

| 族 | 专属面 | 绿了说明什么 |
| --- | --- | --- |
| move | 自身回执明文字节（`+GD/<tick>`，tick 由侧车归一） | 走位受理（而非"没走"）与 oracle 同形 |
| hit | **伤害值**（受击者 HP/DB 前后差） | 抽攻击力的数值链（WAbil/随机流）与 oracle 同值 |
| spell | **魔法值消耗与魔法帧序**（`SM_MAGICFIRE` 先于 `SM_SPELL`） | 施法受理/扣蓝/帧序三件事与 oracle 一致 |
| magic-key | **库后态** `characters_magic.Key`（按 `MagicList` 序） | 零帧副作用改的是同一条魔法记录、同一位 |
| say（非 GM） | **`SM_HEAR` body 字节**（GBK `ChrName:Msg` + 颜色 param） | 说话文本编码/颜色与 oracle 逐字节一致 |
| query4 | **每条 `ValLabel` 一帧**（1065 多帧）与 82 的 3×3 窗口命中 | 多帧应答的条数/顺序/窗口过滤与 oracle 一致 |
| levelrank | **默认配置零帧**（oracle 无 1060） | 不把 Rust-only 扩展泄漏给对拍（默认关） |
| login-notice | **零帧 + 后续握手帧不受影响** | 1018 只写会话态、不吞后续 |

## 4. 分批（today / 接线后 / 物品族）

- **A 批（现在就能跑）**：**move**（区 k 已接线）。建议 `a7` 先跑 `move_walk`（+可选 `CM_RUN`/
  `CM_HORSERUN` 步骤）两区对拍；这也是唯一能验证"区 k 自己处置命令"的族。
- **B 批（需「8 族接线片」）**：**hit / spell / say（非 GM）**。这三族帧丰富、价值最高；
  接线内容 = 在 `handle_zone_command_on` 里按 `zone_cmd_class` 之外的**族谓词**分派到各自 `handle_*`
  （与区 0 同一实现；hit/spell 需要 `displays`/`saves` 等区 k 已有字段）。
- **C 批（接线后、低帧量）**：**query4 / magic-key / login-notice / levelrank**。前三者帧少或零帧，
  主要靠面4/sidecar 与面5；levelrank 在默认配置下是"零帧 == 零帧"（oracle 无此消息号）。
- **不在本规划**：`CM_QUERYBAGITEMS(81)` 与物品族（M1c-1 在建 `MakeIndex` 共享计数器）；
  GM `@`（Process）已被第 4 步 A6 覆盖。

## 5. 风险（每族最可能出差异的点）

| 族 | 风险 |
| --- | --- |
| move | ①自身 `+GD/<tick>` 的 tick 是**墙钟**（`GetGoodTick`）⇒ 规则必须把明文里的 tick 归一，否则永远差；②观察者收 `SM_WALK` 的**按格序**（同格多实体）在拆区后依赖 `obj_list` 序；③镜像观察者的 `SM_WALK` 必须**只由源区发一次**（第 1 步 A13 已修，回归要守）。 |
| hit | ①**伤害值**依赖 `WAbil`/随机流：两侧每次攻击的 RNG 消耗次序必须一致（`RollAbility` 链）；②`SM_STRUCK` 收件人集合 = 受击者视野（**含自己**），拆区后边缘观察者极易差 1 个；③9 条 `HIT_IDENTS` 的 `tag` 方向截断语义（R6 警告只覆盖取值域）。 |
| spell | ①**帧序**：`SM_MAGICFIRE(638)` 必须在 `SM_SPELL(17)` 之前（oracle `case RM_SPELL` 的回执链）；②蓝不足支必须只发 `SM_MAGICFIRE_FAIL(639)`、**无** 17；③魔法等级/修炼值副作用落库（`MagicList.Level/Train`）。 |
| magic-key | ①零帧但**必须改到正确的那条**魔法（按 `MagicList` 下标，不是按 id 排序）；②存档编码（`build_save_record` 整条记录克隆）——错位会在 DB 面表现为整行不同；③该族**没有帧**，判据全在 sidecar/DB，**别**用"帧相等"当绿。 |
| say（非 GM） | ①`SM_HEAR` body 是 **GBK**（`ChrName + ':' + Msg`），走 `cp936` 编码；②颜色取 `MsgColors.hear_f/b`；③收件人集合受 `RM_HEAR` **500ms 缓存**影响（T-287 残余站点）——同 tick 连说两句时观察者只收首句，拆区后缓存键/时钟要对齐。 |
| query4 | ①**多帧**：1065 每条 `ValLabel` 一帧，顺序必须与脚本写入序一致；②82 的 3×3 窗口（`CretInNearXy`）在拆区后边缘目标会落到邻区镜像 ⇒ 需明确"镜像可被查询"的口径；③80 的回执 ident 是 `SM_USERNAME/SM_GHOST` 两选一（oracle `SendDefMessage`），别只等一种；④3040 **零帧**，别把它当少帧。 |
| levelrank | ①oracle **没有 1060** ⇒ 默认必须零帧；若窗口 profile 误开 `LevelRankOnLogin`，Rust 会多发 `SM_LEVELRANK(1108)`，一侧独有 ⇒ 判红（这本身是好的牙，但要在 profile 里显式关）；②排序 = 在线登记序（非等级序），若将来要比"排序正确"需另立判据。 |
| login-notice | ①零帧；若在 1018 之后多发任何帧（例如错误地触发 `SM_SENDNOTICE` 回环）⇒ 面1 红；②`client_tick` 只写会话态、**不落库**——DB 面不得出现该字段。 |

## 6. 装置与前置（给 `a7` 的窗口清单）

1. **两区 profile**：`MIR2_ZONES=2`、`MIR2_ZONE_SLICING=1`、`MIR2_ZONE_MIRROR=1`（slicing 强制
   mirror=1）、**不设** `MIR2_SHARDS>1`；登录路由使被测角色落区 k（沿用第 3/4 步窗口 profile）。
2. **场景**：`tools/probe/scenarios/<name>.json`；新场景用 `--check-scenario` 离线校验（rc=0 才算
   "能真跑"，`required`/`placement` 自洽见 README R1–R5）。
3. **比较器**：`tools/conformance/compare_runs.py` + `rules.json`；**该族 ident 必须先有规则**
   （否则 rc=2 UNDECIDABLE，面5 不绿）；需要 DB 后态的族（magic-key/query/levelrank）加
   `--require-db` 与 `db-key-mode` 口径。
4. **证据四件套**：两侧 `frames.ndjson`/`run.json`/`db_snapshot.json` + `report.json`；
   每族至少一条**红检**（改坏该族独有的面 ⇒ rc=1；例如 hit 改伤害公式、spell 把 638 挪到 17 后）。
5. **构造级辅助（非替代）**：`bins/game-svr/tests/l2_two_zone.rs` 只证 AOI 两阶段/halo 序，
   **不覆盖命令帧**；不要拿它当族对拍证据。

## 7. 一句话结论

**8 族里今天能直接跑两区对拍的只有 `move` 一族**（区 k 的 `handle_zone_command_on` 只实现 move，
其余 7 族被 `note_zone_cmd_unsupported` 丢弃）；下一步应先补「8 族接线片」，接线到位后按 §4 的
B/C 批分两轮跑，其中 hit/spell/say 价值最高、query4/magic-key/login-notice/levelrank 主要靠
sidecar/DB 与"零帧相等"。
