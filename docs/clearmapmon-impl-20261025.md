# CLEARMAPMON + MONGENEX 落地（清场 → 刷怪同一动作序队列）— 2026-10-25

- 卡：`openmir2-clearmapmon-impl`（tier **R**；owner `openmir2-dev-1`；worktree `.worktrees/dev-1`；分支 `card/clearmapmon-impl`）。
- 基线：本地 `master` `18ee8003b`（patch base）。
- 写集：`crates/world/src/{world.rs,mongen.rs,session.rs,monai.rs}`、`crates/script/src/engine/mod.rs`、
  `bins/game-svr/src/{lib.rs,npc.rs}`、`mir2-rs/tests/parity/coverage/script_host_noop_audit.json`、
  `mir2-rs/tests/parity/whitelist.md`（T-207 锚保鲜）、本文件。
- 红线：未改 C# `src/**`、未改表/脚本/数据文件；未起栈、未占端口、未连库；不 merge、不自审。

## 0. 一句话

`CLEARMAPMON` 与 `MONGENEX`（此前都是 host no-op）现在**两半边一起落地**：脚本执行期只把世界副作用按
**动作序**排进同一条队列（`WorldAction`），宿主脚本跑完后在同一队列上按序应用（`World::clear_map_monsters`
→ `guardlist::regen_monster_by_name`），因此「清场 → 刷怪同图」不会被拆成两遍应用而互删；
`SM_DISAPPEAR(30)` 走既有 `drain_departs` 帧面。

## 1. oracle 逐行（行号）

| 语义 | 出处 |
|---|---|
| 动作分派 `CLEARMAPMON` = 215 / `MONGENEX` = 214 | `ExecutionProcessingSys.cs:90`、`codes.rs:446-447` |
| `ActionOfClearMapMon`：`GetMapMonster` 收表 → 逐只 `Master != null` 跳过、`GetNoClearMonList` 判（部署恒 false）、`NoItem = true`、`MakeGhost()` | `ExecutionProcessingSys.cs:2983-3003` |
| `GetMapMonster`：`MonGenInfoThreadMap` 线程序 → 条目序 → `CertList` 序；只收 `!Death && !Ghost && Envir == envir`；`envir == null` ⇒ 0 | `WorldServer.cs:1639-1674` |
| `GetNoClearMonList` 整段注释 ⇒ **恒 false** | `M2Share.cs:1782-1794`、`SystemShare.cs:1694-1706` |
| `MakeGhost`：`CanReAlive ⇒ Invisible = true`，否则 `Ghost = true`；`GhostTick = GetTickCount()`；随后 `DisappearA()` | `BaseObject.Base.cs:484-496` |
| `DisappearA`：`DeleteFromMap` + `SendRefMsg(RM_DISAPPEAR, …)`（收件人 = ±`SendRefMsgRange`(12) 格内 `Race==Play && !Ghost`；`ObMode/FixedHideMode` 只自送） | `BaseObject.cs:1519-1524`、`BaseObject.Message.cs:385-444` |
| `ActionOfMonGenEx`：循环 `nCount` 次，每次抽 `Random(range*2+1)` 两次定 x/y，`RegenMonsterByName == null ⇒ break` | `ExecutionProcessingSys.cs:5466-5487` |
| `RegenMonsterByName`：`CreateMonster` → 合成 `MonGenInfo`（`CertList=[id]`）→ 追加线程表；**不置** `CanReAlive/ReAliveTick/MonGen` | `WorldServer.MonGen.cs:382-438`（本仓 `guardlist.rs` 同形） |

部署侧可达性（只读 `E:/MirServer/M2GameSvr/Envir`）：`CLEARMAPMON` 2 处（`Market_Def/任务NPC/武士-0114.txt:145` T118、
`老人-1.txt:49` Q001），`MONGENEX` 同样 2 处（同文件 `:146` / `:50`），都在同一 `#act` 里紧接 `map <同图>`。

## 2. 实现

### 2.1 world：`World::clear_map_monsters(map_id, &MonGenState)`

- **候选序 = oracle 遍历序**：`state.threads`（BTreeMap 升序）→ `entries` 序 → `CertList` 序，**先整表快照再逐个
  `MakeGhost`**（与 C# 的 `GetMapMonster` 填表再循环同形）。不迭代 `Query`/槽位序。
- 过滤：`e.death || e.ghost || e.map_id != map_id` 跳过；`e.master.is_some()` 跳过；`NoClearMonList` 恒 false（不设过滤）。
- 每只：清场时刻 ±12 格扫 `Race==Play && !Ghost` 观察者 → `SessionRegistry::push_depart(observer, mon)`
  （与离世广播同一条保序队列，宿主同点出 `SM_DISAPPEAR(30)`）→ `no_item = true` → `monai::make_ghost`（提 `pub(crate)`）。
- `CertList` 条目**保留**（oracle 同）：`Ghost` 由 `process_cert_lists` 5 分钟释放；`Invisible`（`CanReAlive`）走 `ReAliveEx`。
- 不消费随机数。

### 2.2 script：`WorldAction` 队列

`crates/script/src/engine/mod.rs` 新增 `WorldAction::{ClearMapMonsters, RegenMonster}` 与 trait 方法
`take_world_actions()`（缺省空）。`NpcPlayer`（`bins/game-svr/src/npc.rs`）覆写：

- `clear_map_monsters(map)` / `regen_monster(map,x,y,name)`：`mon_gen_enabled` 为真时**入队**；否则保持旧行为
  （`clear` no-op、`regen` 返回 `false`）⇒ 未接线路径逐字节不变。
- `regen_monster` 接线时返回 `true`（保持 `ActionOfMonGenEx` 循环次数；见 §4 偏差 ②）。

### 2.3 host：一条队列、一遍应用

`bins/game-svr/src/lib.rs`：`ScriptWorldCtx { host, registry, table, displays }` +
`apply_script_world_actions(world, &mut Option<ctx>, actions)`：

- `ClearMapMonsters`：`MapRegistry::id_of` 解析（`FindMap == null` ⇒ 零操作）→ `World::clear_map_monsters`。
- `RegenMonster`：`guardlist::regen_monster_by_name`（同一条 `CreateMonster` 取数/移位链；合成 `MonGenInfo` 入
  `MonGenState`），`(short)` 截断照 `ExecutionProcessingSys.cs:5481-5482`。
- `world.rng` 与 `step_mon_gen` 同口径「取出 → 应用 → 放回」。

接线点：`handle_click_npc_cmd`（T118/Q001 的真实入口）与 `handle_shop_cmd`（5 个 `GotoLable` 调用点）各自在
`world.rng` 归还后立刻应用；sim 循环生产调用点构造 `ScriptWorldCtx`（`mon_gen_host` + `registry` +
`enter_cfg.mon_gen.table` + `displays`）。固定 NPC 路（`FixedNpcScriptCtx`）**未接线**（`mon_gen_enabled=false`；
部署无 `CLEARMAPMON`/`MONGENEX` 用法），登记为已知缺口。

## 3. 判据（实测）

### 3.1 world 合同测试（`crates/world`，3 条）

| 测试 | 钉什么 |
|---|---|
| `clear_map_monsters_filters_match_oracle` | 只清「同图/存活/非 Ghost/非 Master/在 CertList」；`NoItem`、`Invisible` vs `Ghost`、`GhostTick`、摘格、实体与 CertList 保留 |
| `clear_map_monsters_order_is_thread_entry_cert_not_entity_slot` | 清场序 = thread→entry→CertList（用例构造的槽位序与 oracle 序相反） |
| `clear_map_monsters_departs_follow_disappear_a_broadcast` | `drain_departs` = 逐怪 × ±12 格 `Race==Play && !Ghost`（Ghost 观察者/远处观察者不收） |

### 3.2 生产入口序列（`bins/game-svr`，3 条）

`run_clear_then_mongen_sequence` 用**真实脚本动作**（`n_cmd_code` 215 + 214）经 `handle_click_npc_cmd` /
`handle_shop_cmd` 执行，断言：老怪 `NoItem + Invisible + 摘格 + 实体保留`；`MONGENEX` 新怪**存活**
（若队列反序应用，新怪已被合成 `CertList` 纳入清场 ⇒ 必红）；合成 `MonGenInfo` 入表且 `CertList=[新怪]`。

| 测试 | 对应部署脚本 |
|---|---|
| `clearmapmon_then_mongenex_t118_sequence` | `武士-0114.txt:145-146`（`CLEARMAPMON T118` + `MONGENEX T118 14 17 命运教主 2 1 1`） |
| `clearmapmon_then_mongenex_q001_sequence` | `老人-1.txt:49-50`（`CLEARMAPMON Q001` + `MONGENEX Q001 10 10 骷髅精灵5 2 1 1`） |
| `clearmapmon_then_mongenex_via_merchant_dialog_path` | 商店对话入口同队列（防单路接线） |

### 3.3 牙（逐条实跑：变异 → 目标用例红 → 还原绿）

| # | 变异 | 实际红 |
|---:|---|---|
| 1 | `apply_script_world_actions` 改为倒序应用（`actions.into_iter().rev()`） | 3 条序列用例红（「新怪必须存活」） |
| 2 | 删 world 清场的 `e.master.is_some()` 跳过 | `..._filters_match_oracle` 红（宠物被清） |
| 3 | 候选表 `ids.sort()`（= 槽位序）替换 oracle 遍历序 | `..._order_is_thread_entry_cert...` 红 |
| 4 | 清场帧收件人范围 `SEND_REF_MSG_RANGE + 30` | `..._departs_follow...` 红（远处观察者收到） |
| 5 | `NpcPlayer::regen_monster` 恒 `false` | 3 条序列用例红（「MONGENEX 必须刷出 1 只」） |

### 3.4 噪声面 / 审计台账

- `script_host_noop_audit.json`：`regen_monster` / `clear_map_monsters` 由 `needs_card` → `safe/overridden`
  （evidence 行 = `npc.rs` 当前 `fn` 行）；顺手订正 `set_game_point`（master 已覆写但台账落后一版）。
  因本卡给 `engine/mod.rs` 与 `npc.rs` 加行，台账 `line/mod_line/mod_ref/evidence.line` 按当前文件
  重钉（纯行号数据，无语义改动）。`check` / `check --strict-lines` / `selftest` 均 rc=0。
- `whitelist.md` T-207 的 `npc.rs:1370-1373` 锚随 `npc.rs` 行号漂移重钉为 `1387-1390`（`whitelist_anchor_gate` rc=0）。

## 4. 已知偏差（R 级窗口项，`window_required: true`）

1. **RNG 流水位置**：oracle 在动作内同步执行 `RegenMonsterByName`（`CreateMonster` 抽数插在 MONGENEX 的 x/y
   抽数之间、且与后续脚本抽数交织）；本实现把创建抽数挪到脚本尾（同队列按动作序）。**可观测面** = 脚本在
   `MONGENEX` 之后还有抽数动作时，后续随机结果整体移位。部署两处（T118/Q001）`MONGENEX` 之后只有
   `map`/`sendmsg`/`TimeRecall`（均不抽数）⇒ 当前不可观测。
2. **`regen_monster` 返回门**：排队时恒 `true`；oracle 在 `RegenMonsterByName == null`（未知地图/模板/31 次
   移位失败）时最先一轮就 `break`。失败输入下本实现会多走后续迭代的 x/y 抽数。部署模板/地图都存在。
3. **玩家 `map <同图>` 与本队列的相对序**：oracle 是「清场 → 刷怪 → 玩家切图」；本实现脚本先把玩家切图，
   再应用队列。当前 T118/Q001 落点与刷怪点不同格，不可观测；若将来落点与刷怪点重合，`CanWalk` 的阻挡
   判定会因玩家先占格而不同。
4. **`Invisible` 语义**：MonGen 刷出的怪 `CanReAlive = true` ⇒ 清场后是 `Invisible`（可被 `ReAliveEx` 复活），
   与 oracle 一致；`MONGENEX` 合成的怪不置 `CanReAlive` ⇒ `Ghost`（5 分钟后释放）。两分支均有用例钉住。
5. 固定 NPC 路（`Npcs.txt` 脚本）未接世界副作用；部署无用法。

**窗口项（真机，R 级）**：
- T118：任务两步 —— 进 T118 前地图上有怪（含 ≥2 只同名 + 1 只宠物/召唤物），触发 `CLEARMAPMON T118`
  ⇒ 两侧实体集合/AOI 差集/`SM_DISAPPEAR(30)` 帧序逐字节对拍；随后 `MONGENEX` 的刷怪位置（±2 格随机）与数量 diff=0。
- Q001：同上，怪物名 `骷髅精灵5`、刷怪点 `(10,10)`，并覆盖 `TimeRecall 5` 之后的回收时序。
- 命令模板：`python -I -B tools/gw-capture/gate_shadow_proxy.py --out E:/tmp/clmon/sel --map 7101=127.0.0.1:7100 --gate sel --direction s2c`，
  跑完 `--verify` 复核；同一条命令在 C# 栈重跑，按帧 sha256 对拍。

## 5. 门禁

fmt / clippy（`-D warnings`）/ `cargo test --workspace`（**3206 passed**，0 failed）/ `msgcodegen --check` 全 rc=0；
7 项卫生门禁全 rc=0（`t_reservation` / `conflict_marker` / `stale_pointer` / `whitelist_anchor` / `m3_registry` /
`dynamic_count_stamp` / `board_coverage`）；`script_host_noop_audit.py check`（含 `--strict-lines`）与 `selftest` rc=0；
`git diff --check` rc=0；`git merge-tree` 见 patch body（`tools/hygiene/merge_tree_helper.py`）。

## 6. 残余

- 真机窗口（§4）未跑 ⇒ 按 tier 政策合并后保持 `in-progress`。
- 固定 NPC 路世界副作用未接线（部署不可达）。
- 无临时物；`E:/MirServer` 未改动。
