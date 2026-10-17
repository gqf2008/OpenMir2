# mapcell 装载「消费点可达性」复核（只读静态调用图）

- 上游报告：`mir2-rs/tests/parity/m3/audit-valid-consumers-20261017.md`（提交 `b192bd9f3` / 父 `2ff02ac0e`）。
- 本卡任务：把上游 §5「没能判定的条目」中**共同问题 = 该路径在生产里到底可达吗**的几条查清。
- 方法：只读 `grep`/`sed`/`cat`/`git log`。**未跑 cargo**、未起服务/客户端/假人、未占端口、未写库、
  未动 `E:/MirServer/**`、**未写仓库任何文件**（本报告在 `/e/tmp/`，唯一写盘物）。
- 记号：`ok` = `get_cell_info(x,y).1`（= C# `GetCellInfo` 的 `success`）；`Valid` = `Attribute==Walk`。
- **事实 vs 推断**：每节分「我读到的事实」（带 `file:line` + 可复制命令）与「我的推断」（标 **[推断]**）。

---

## 0. 结论摘要

| # | 路径 | 可达性 | 装载后行为 | 一行证据 |
|---|---|---|---|---|
| 1 | `magic.rs::char_pushed` | **不可达**（无生产者） | 读 `get_cell_info().1` ⇒ 若接线会变 | 唯一非测试调用者在 `magic.rs:1109`（`mag_push_arround` 内），而 `mag_push_arround` 无任何生产调用者 |
| 1b | `world.rs::push_now`（生产对应物） | **不可达**（`Command::Push` 无生产者） | 读 `cell_free` ⇒ 若接线会变 | `Command::Push` 全仓非测试仅在 `world.rs:1052` match 臂；`bins/game-svr/src` 无入队点 |
| 1c | `magic.rs::mag_sace_move` | **不可达**（无生产者） | 经 `space_move_get_rand_xy` ⇒ 若接线会变（R 级） | 唯一调用者在 `crates/world/tests/map_effect_vectors.rs:294`（测试） |
| 1d | `magic.rs::mag_make_clone` | **可达** | 读 `cell.valid()` ⇒ **条件变**（施法者格非 Valid 才变，生产不可达） | `handle_spell_cmd`(main.rs:5024)→`cast_clone_spell`(5549)→`spawn_clone`(bins/.../slave.rs:529→crates/world/src/slave.rs:1176)→`mag_make_clone`(magic.rs:1259) |
| 2 | `slave.rs::World::recall_slave` | **可达（条件）** | 读 `get_cell_info().1` ⇒ **会变（R 级：RNG 流水）** | `handle_spell_cmd`→`cast_slave_spell`(5346)→`summon_from_spell`(bins/.../slave.rs:187)→`summon_slave`(crates/world/src/slave.rs:152)→`recall_slave`(slave.rs:202) |
| 3 | `ground.rs::put_item`（独立模型） | **可达** | **不变（零影响）** | `put_item`(bins/.../ground.rs:263) 只走 `idx()`(206) 界内判定；与 `world.maps` 唯一共享是 `width/height`（`pickwire.rs:141 build_ground_map`），不读任何属性 |
| 4 | `aoi.rs::search_view_range(_death)`（`BaseObject.ViewRange`） | **可达** | `ok` **恒真→按属性**（墙格跳过）⇒ **输出不变** | `aoi.rs:91`（`search_view_range`）/`:211`（`..._death`）；世界 tick 调用于 `world.rs:1190`/`:1180` |
| 5 | `cetin_near_xy` 生产调用点 | **可达** | `!ok` 掩码支「恒不进→按属性」⇒ **结果不变** | 定义 `queryuser.rs:29`（`!ok` 在 `:36`）；调用点 main.rs `2672`/`2768`/`4995`（+`spell_snap_target` 内 5 处 5154/5232/5294/5429/5516） |

> **2026-10-17 订正（来源 `77dee0813` / `8e9244388` / `e7d432189`）**：上表行号按 `e7d432189` 重测——#1 唯一非测试调用者 `magic.rs:801`（`mag_push_arround` 定义 `:778`）；#1b `Command::Push` match 臂 `world.rs:1245`（`push_now` 定义 `:2344`）；#1c 唯一调用者仍 `crates/world/tests/map_effect_vectors.rs:294`（定义 `magic.rs:853`）；#1d 链 `cast_clone_spell(main.rs:5732) → bins/game-svr/src/slave.rs:529 → crates/world/src/slave.rs:1129 → mag_make_clone(magic.rs:916)`；#2 链 `cast_slave_spell(main.rs:5529) → summon_from_spell(bins/game-svr/src/slave.rs:280) → summon_slave(crates/world/src/slave.rs:152) → recall_slave(slave.rs:202，定义 :584)`；#4 `world.rs` tick 调用于 `:1373`/`:1383`（`fn tick` 定义 `:1120`）；#5 `spell_snap_target` 生产调用现为 **6 处**：`5154/5232/5298/5477/5612/5699`（定义 `main.rs:4988`）。复核命令见 §4 订正块。

**总判**：本卡 5 条未定项中，**唯一装载后行为真的会变、且真机可达**的是 **#2 `recall_slave`（R 级：RNG 流水）**；
**#1d `mag_make_clone` 条件可达但生产不可达**（施法者格必 Valid）；**#3/#4/#5 可达但输出不变**；**#1/#1b/#1c 无生产者（不可达）**。

> **2026-10-17 订正（来源 `77dee0813`）**：总判末句「**#1/#1b/#1c 无生产者（不可达）**」已过期——现值「#1b（`push_now`，魔号 8）与 #1c 的魔号 21 效果体均**生产可达且装载后行为会变**（R 级：RNG 流水/推进格数）；#1（`magic.rs::char_pushed` 本体）仍无生产调用者」；理由 接线提交。复核：`sed -n '859p' mir2-rs/crates/world/src/world.rs` ⇒ 期望 `let pushed = self.push_now(target_id, n_dir, push);`。

---

## 1. 逐条判定

### 1.1 `magic.rs::char_pushed`（推人）——**不可达（无生产者）**

> **2026-10-17 订正（来源 `77dee0813`）**：标题原文「**不可达（无生产者）**」已过期——现值「生产推人路径经 `handle_spell_cmd` 魔号 8 → `World::cast_firewind_spell`（`world.rs:822`）→ `World::push_now`（`world.rs:2344`，直调点 `world.rs:859`）**可达**；`magic.rs::char_pushed` 函数本体仍只被 `mag_push_arround`（测试）与 `#[cfg(test)]` 调用」。理由 接线提交（卡 `openmir2-m3-firewind-spacemove-wire`，tier=R）。复核：`grep -n "pub fn cast_firewind_spell\|fn push_now" mir2-rs/crates/world/src/world.rs` ⇒ 期望 `822`/`2344`。

**我读到的事实**

- 定义：`crates/world/src/magic.rs:1001`；每步先 `if !map.get_cell_info(nx, ny).1 { break; }`（`magic.rs:1018`）。
- 全仓非测试调用者：**仅** `magic.rs:1109`（`mag_push_arround` 体内）。
- `mag_push_arround` 定义 `magic.rs:1089`；其调用者**仅**测试 `crates/world/tests/map_effect_vectors.rs:151/159`。
- 生产（`bins/game-svr/src`）**没有**任何 `char_pushed` / `mag_push_arround` 调用。
- 生产侧对应的推人实现是**另一个函数** `World::push_now`（`world.rs:2151`），由 `Command::Push` 驱动
  （`world.rs:1052` match 臂）。`Command::Push` 的**生产者**：全仓非测试仅命中 match 臂
  （`world.rs:1052`）+ `world.rs:3324/3364`（`#[cfg(test)]`）+ `bins/game-svr/tests/action_wire_e2e.rs:29`（测试）。
  `bins/game-svr/src` 内 `world.enqueue(...)` 只有 `main.rs:1718`（`Command::Butch`）——**没有 Push/Rush**。


> **2026-10-17 订正（来源 `77dee0813` / `e7d432189`）**：原文「定义 `magic.rs:1001`」「`magic.rs:1018`」「`magic.rs:1109`」「`mag_push_arround` 定义 `magic.rs:1089`」「`world.rs:2151`」已过期——现值「定义 `magic.rs:690`、格子读 `magic.rs:707`、`mag_push_arround` 内调用 `magic.rs:801`（定义 `magic.rs:778`）、`push_now` 定义 `world.rs:2344`」；理由 接线片与方案 C 搬迁导致行号平移。复核：`grep -n "pub fn char_pushed\|fn mag_push_arround" mir2-rs/crates/world/src/magic.rs` ⇒ 期望 `690`/`778`；`grep -n "if !map.get_cell_info(nx, ny).1" mir2-rs/crates/world/src/magic.rs` ⇒ `707`；`grep -n "let steps = char_pushed" mir2-rs/crates/world/src/magic.rs` ⇒ `801`；`grep -n "fn push_now" mir2-rs/crates/world/src/world.rs` ⇒ `2344`。

> **2026-10-17 订正（来源 `77dee0813` / `e7d432189`）**：原文「由 `Command::Push` 驱动（`world.rs:1052` match 臂）」已过期——现值「`push_now`（定义 `world.rs:2344`）有两个驱动面：① `World::cast_firewind_spell` 直调（`world.rs:859`，**生产**）；② `Command::Push` match 臂（现行号 `world.rs:1245`，入队点仅测试 `world.rs:3576/3616` 与 `bins/game-svr/tests/action_wire_e2e.rs:29`）。`Command::Push` 自身仍无 `bins/game-svr/src` 生产者」；理由 接线提交。复核：`grep -n "fn push_now\|self.push_now\|Command::Push" mir2-rs/crates/world/src/world.rs` ⇒ 期望含 `2344`（定义）/`859`（生产直调）/`1245`（match 臂）。

> **2026-10-17 订正（来源 `e7d432189`）**：原文「`self.cell_free(...)`（`world.rs:2164`）」「`get_cell_info(...)`（`world.rs:2064-2072`）」已过期——现值「`cell_free` 定义 `world.rs:2257`、格读 `world.rs:2264`」。复核：`grep -n "fn cell_free" mir2-rs/crates/world/src/world.rs` ⇒ `2257`。

**装载后行为会不会变（假设接线）**：**[推断]** 会变。

- `char_pushed` 每步读 `get_cell_info(nx,ny).1`（`magic.rs:1018`）：装载前界内恒 `true` ⇒ 永不因墙 `break`；
  装载后目标格是墙 ⇒ `break`（推进步数变少）。
- `push_now`（生产同族）用 `self.cell_free(map_id, nx, ny, actor_id)`（`world.rs:2164`）；`cell_free` 读
  `get_cell_info(...).1` + `valid/is_available`（`world.rs:2064-2072`）⇒ 同样按属性变。

**复核命令**

```
grep -rn "char_pushed\|mag_push_arround\|PushAroundInput" --include=*.rs .
# 期望：magic.rs 定义 + world.rs 两处 #[cfg(test)] + crates/world/tests/map_effect_vectors.rs；bins/ 下 0 命中

grep -rn "Command::Push\|Command::Rush" --include=*.rs .
# 期望：world.rs match 臂/测试 + bins/game-svr/tests/action_wire_e2e.rs；bins/game-svr/src 下 0 命中

grep -rn "\.enqueue(" --include=*.rs bins/game-svr/src/
# 期望：main.rs:1485/1492（msgqueue）、1718（Command::Butch）；无 Push/Rush
```

**判定**：`char_pushed`（magic.rs）与其生产对应物 `push_now` **当前均无生产调用链**（无 `handle_*_cmd`、
无世界 tick 入口）⇒ 装载**不改**任何真机可见行为。**但两者都读格子属性**，一旦接线即会变。

**未能确证**：oracle 侧 `CharPushed` 的生产触发点（推人魔法 = 魔号 8 抗拒火环；本仓 `handle_spell_cmd`
未处理魔号 8）是否属「待接线」而非「永不接线」——需 M3 施法批确认。

**附带事实（魔号覆盖）**：`handle_spell_cmd`（`main.rs:5024`）静态处理的魔号 = 战士技能（`:5149` 早退）
+ 31/14/15（`:5240` switch）+ 17/30/41（召唤，`:5263`）+ 20（诱惑，`:5424`）+ 46（分身，`:5511`）；
**无魔号 8 / 无魔号 21**。（魔号↔名：`src/CloudGate/TableDef.cs:242 MaigicNameList` 第 8 项=`抗拒火环`、
第 21 项=`瞬息移动`；`src/GameGate/TableDef.cs` 注释 `08 抗拒火环`/`21 瞬息移动`。）

---

### 1.2 `magic.rs::mag_sace_move`（瞬息移动）——**不可达（无生产者）**

> **2026-10-17 订正（来源 `77dee0813` / `8e9244388` / `e7d432189`）**：标题原文「**不可达（无生产者）**」已过期——现值「魔号 21 效果体经 `World::cast_space_move_spell_with`（`world.rs:909`，经 `handle_spell_cmd`）**生产可达**；`magic.rs::mag_sace_move` 函数本体仍仅测试调用」；理由 接线提交。复核：`grep -n "pub fn cast_space_move_spell" mir2-rs/crates/world/src/world.rs` ⇒ 期望 `894`/`909`。

**我读到的事实**

- 定义：`magic.rs:1196`。全仓调用者：**仅**测试 `crates/world/tests/map_effect_vectors.rs:294`（+ `:283` 造 `SaceMoveInput`）。
- `bins/**` 无 `mag_sace_move` / `SaceMoveInput` / `SaceMoveOutcome` 命中。
- 其格子读取在私有 helper `space_move_get_rand_xy`（`magic.rs:1161`）→ `can_walk_ignore_objects`
  （`magic.rs:993-994` ⇒ `map.get_cell_info(x, y).1`）。


> **2026-10-17 订正（来源 `77dee0813` / `8e9244388` / `e7d432189`）**：原文「定义 `magic.rs:1196`」「私有 helper `space_move_get_rand_xy`（`magic.rs:1161`）→ `can_walk_ignore_objects`（`magic.rs:993-994`）」已过期——现值「定义 `magic.rs:853`；格子读取改为 `magic.rs:873` 直调 `crate::space_move::space_move_get_rand_xy`（`space_move.rs:23`），闭包 `|x, y| map.get_cell_info(x, y).1` 在 `magic.rs:879`；`magic.rs` 的私有 `space_move_get_rand_xy` 与 `can_walk_ignore_objects` 副本均已删除（后者由 `world.rs:2378` 自有版本承接）」。理由 接线去重 + 方案 C。复核：`grep -n "fn space_move_get_rand_xy" mir2-rs/crates/world/src/*.rs` ⇒ 期望仅 `space_move.rs:23`；`grep -n "pub fn mag_sace_move" mir2-rs/crates/world/src/magic.rs` ⇒ `853`。

> **2026-10-17 订正（来源 `8e9244388` / `e7d432189`）**：「装载后行为」段原文按 `magic.rs:1177-1178` 的私有 helper 描述已过期——现值按 `crates/world/src/space_move.rs:23-53` 描述：首轮 `if can_walk(x, y) {` / `return Some((x, y));`（闭包 = `magic.rs:879` 的 `map.get_cell_info(x, y).1`，即 `cellSuccess && Valid`）；装载前起点界内恒 `true` ⇒ 首轮即返回、不抽额外随机数；装载后起点是墙 ⇒ 进移位支（`x += n18` 或 `rng.random_below(width)` / `rng.random_below(height)`）⇒ 多抽随机数 + 落点不同。`magic.rs` 已无私有副本可引用。复核：`sed -n '23,60p' mir2-rs/crates/world/src/space_move.rs` ⇒ 期望首轮 `if can_walk(x, y) {` / `return Some((x, y));`。

**装载后行为会不会变（假设接线）**：**[推断]** 会变，且是 **R 级**。

- `space_move_get_rand_xy`（`magic.rs:1177-1178`）：首轮 `if can_walk_ignore_objects(map, *x, *y) { return true }`。
  装载前起点 `(nx,ny)` 界内恒 `ok=true` ⇒ **首轮即返回**（不抽额外随机数）；装载后起点是墙 ⇒ 进移位支
  （`*x += n18` 或 `rng.random_below(width)`/`random_below(height)`）⇒ **多抽随机数 + 落点不同**。

**复核命令**

```
grep -rn "mag_sace_move\|SaceMove" --include=*.rs .
# 期望：magic.rs 定义 + crates/world/tests/map_effect_vectors.rs；bins/ 下 0 命中
sed -n '1173,1192p' crates/world/src/magic.rs
# 期望：for _ in 0..201 { if can_walk_ignore_objects(map, *x, *y) { return true } ... else { *x = rng.random_below(...) } }
```

> **2026-10-17 订正（来源 `77dee0813` / `e7d432189`）**：复核命令原文 `sed -n '1173,1192p' crates/world/src/magic.rs` 的范围已过期（该范围现落到 `mag_sace_move` 本体之后）——现值用 `sed -n '868,884p' crates/world/src/magic.rs`：期望 `crate::space_move::space_move_get_rand_xy(`（`:873`）+ `|x, y| map.get_cell_info(x, y).1,`（`:879`）+ `out.space_moved = true;`（`:883`）。理由 方案 C 行号平移。

**判定**：**不可达**（生产从未驱动）⇒ 装载不改真机行为。
**未能确证**：魔号 21 是否在运行期 magic 表内、是否玩家可学（`load_magics` 是运行期加载，静态看不到）；
即便在表内，`handle_spell_cmd` 无 21 支 ⇒ 仍不可达。需要：真机跑 `load_magics` 或查 `E:/MirServer` Magic 表。

> **2026-10-17 订正（来源 `77dee0813`）**：原文「**不可达**（生产从未驱动）」「`handle_spell_cmd` 无 21 支 ⇒ 仍不可达」已过期——现值「魔号 21 经 `handle_spell_cmd`（`main.rs:5024`）的 `SKILL_FIREWIND | SKILL_SPACEMOVE` 共用块（`main.rs:5294`）→ `cast_space_move_spell_with` **生产可达**；装载后进 R 级分支（`space_move` 移位环读 `get_cell_info().1`）；仅「玩家能否学到 21」仍属数据面（未用真机 `load_magics` 验证）」；理由 同上。复核：`grep -n "SKILL_FIREWIND | mir2_world::magic::SKILL_SPACEMOVE" mir2-rs/bins/game-svr/src/main.rs` ⇒ 期望 `5294`。

---

### 1.3 `magic.rs::mag_make_clone`（分身落点）——**可达；条件变（生产不可达）**

**我读到的事实**

- 定义：`magic.rs:1259`；落格 `map.add_map_object(inp.x, inp.y, CellType::Play, inp.clone_id, 0)`（`magic.rs:1261`）。
- 唯一非测试调用者：`crates/world/src/slave.rs:1176`（`spawn_clone`，定义 `slave.rs:1166`）。
- `spawn_clone` 唯一非测试调用者：`bins/game-svr/src/slave.rs:529`（`cast_clone_spell`，定义 `:523`）。
- `cast_clone_spell` 唯一非测试调用者：`main.rs:5549`（`handle_spell_cmd` 的 `SKILL_46` 支，`:5511`）。
- `handle_spell_cmd`（定义 `main.rs:5024`）唯一生产调用者：`main.rs:8729`（`magicwire::is_spell_ident` 支，
  位于 `fn sim_loop`（`main.rs:7895`）的入站派发）。

**具体调用链（file:line → …）**

```
main.rs:7895 fn sim_loop
  → main.rs:8729  if magicwire::is_spell_ident(cmd.ident) { handle_spell_cmd(...) }   // CM_SPELL(3017)
  → main.rs:5024 handle_spell_cmd
  → main.rs:5511 if req.magic_id == slave::SKILL_46
  → main.rs:5549 game_svr::slave::cast_clone_spell(...)
  → bins/game-svr/src/slave.rs:523 cast_clone_spell
  → bins/game-svr/src/slave.rs:529 spawn_clone(world, caster_id, caster_dir)
  → crates/world/src/slave.rs:1166 spawn_clone
  → crates/world/src/slave.rs:1176 magic::mag_make_clone(&mut world.maps[map_id], ...)
  → crates/world/src/magic.rs:1259 mag_make_clone
  → crates/world/src/magic.rs:1261 map.add_map_object(x, y, CellType::Play, clone_id, 0)
  → crates/world/src/map.rs add_cell_object: `if success && cell.valid()`   // 读 Valid
```


> **2026-10-17 订正（来源 `77dee0813` / `8e9244388` / `e7d432189`）**：原文「定义 `magic.rs:1259`」「落格 `magic.rs:1261`」「`slave.rs:1176`（`spawn_clone`，定义 `slave.rs:1166`）」「`main.rs:5549`」「`:5511`」「`main.rs:8729`」「`fn sim_loop`（`main.rs:7895`）」「`slave.rs:1194`」已过期——现值「定义 `magic.rs:916`、落格 `map.add_map_object(...)` 在 `magic.rs:918`；`slave.rs:1129`（`spawn_clone` 定义 `slave.rs:1119`）；`main.rs:5732`（`handle_spell_cmd` 的 `SKILL_46` 支 `main.rs:5694`）；`handle_spell_cmd` 定义 `main.rs:5024`、生产调用 `main.rs:8934`（`is_spell_ident` 分派臂 `:8932`）（`fn sim_loop` 定义 `main.rs:8100`）；`e.add_to_mapped` 在 `slave.rs:1147`」。理由 main.rs/slave.rs 增长 + 方案 C 搬迁。复核：`grep -n "pub fn mag_make_clone" mir2-rs/crates/world/src/magic.rs` ⇒ `916`；`grep -n "add_map_object" mir2-rs/crates/world/src/magic.rs` ⇒ `918`；`grep -n "pub fn spawn_clone\|mag_make_clone(" mir2-rs/crates/world/src/slave.rs` ⇒ `1119`/`1129`；`grep -n "cast_clone_spell(" mir2-rs/bins/game-svr/src/main.rs` ⇒ `5732`。

**装载后行为会不会变**：**条件变，且条件在生产不可达**。

- `add_map_object` 门 = `cell_match(x,y)`（界内）&& `cell.valid()`（`map.rs add_cell_object`；上游 §1A #7）。
  `(x,y)` = **施法者自己的坐标**（`spawn_clone` 取主人的 `x/y`，`slave.rs:1168-1171`）。
- 装载前：施法者界内 ⇒ `created=true` 恒；装载后：仅当**施法者那格是墙**才 `created=false`。
- 玩家只能站 Valid 格（`cell_free` 挡墙 + `session.rs enter_map` 的 `CellRejected`）⇒
  **施法者格必 Valid ⇒ `created=true` 不变**。
  **2026-10-17 订正（本提交）**：`session.rs::enter_map` 已改为 **oracle 同形**（落格失败**不移除角色**，
  角色进世界但不入格表；见同批提交与 oracle `BaseObject.Base.cs:14-23`）⇒ 上述「必 Valid」前提在
  **病态存档（存档 `CurX/CurY` 指向墙格）/ 切图缩图** 场景**不再成立**：`mag_make_clone` 的 `created=false`
  支从「不可达」曾升为 **边缘可达**（仍与 oracle 同构 —— oracle 该支也不校验返回值，见 §2 #19）。
  **同日后续片 `openmir2-enter-shift-loop` 再收窄**：宿主进图路径已按 oracle `WorldServer.cs:371-395`
  跑地形移位环（墙格逐轮 `-3+Random(6)`、至多 5 轮；仍不可走 ⇒ 回落 `[Setup] HomeMap/HomeX/HomeY`）
  ⇒ **「墙格出生」在宿主路径不可达**（oracle 同因），上述「边缘可达」的来源收窄为**切图缩图**
  （`World::switch_map` 目标格非 Valid）等非登录路径；登录出生不再产生；仍与 oracle 同构。
- 与 oracle 的差异（上游 §2 #19 已登记）：oracle `MagMakeClone` 恒返回 `true`、**不看** `AddMapObject`
  返回值；本仓把「格子能否放下」落成 `created`，`spawn_clone` 用它置 `e.add_to_mapped`（`slave.rs:1194`）。
  ⇒ 差异**仅在施法者格非 Valid 时**可见（生产不可达），**与装载无关**（同一门）。

**复核命令**

```
grep -rn "mag_make_clone\|spawn_clone\|cast_clone_spell" --include=*.rs .
# 期望：crates/world/src/magic.rs:1259 / slave.rs:1166,1176 / bins/game-svr/src/slave.rs:523,529 / main.rs:5549
sed -n '1249,1272p' crates/world/src/magic.rs
sed -n '1166,1200p' crates/world/src/slave.rs
sed -n '5505,5555p' bins/game-svr/src/main.rs
```

> **2026-10-17 订正（来源 `77dee0813` / `e7d432189`）**：本段复核命令原文 `sed -n '1249,1272p' crates/world/src/magic.rs` / `sed -n '1166,1200p' crates/world/src/slave.rs` / `sed -n '5505,5555p' bins/game-svr/src/main.rs` 的范围已过期——现值用 `sed -n '916,922p' crates/world/src/magic.rs`（期望 `pub fn mag_make_clone` + `let created = map.add_map_object(inp.x, inp.y, CellType::Play, inp.clone_id, 0);` + `MakeCloneOutcome {`）、`sed -n '1119,1150p' crates/world/src/slave.rs`（期望 `pub fn spawn_clone` → `mag_make_clone(` → `e.add_to_mapped = out.created;`）、`sed -n '5689,5735p' bins/game-svr/src/main.rs`（期望 `SKILL_46` 支 → `cast_clone_spell`）。grep 期望改为：`magic.rs:916,918` / `slave.rs:1119,1129` / `bins/game-svr/src/slave.rs:523,529` / `main.rs:5732`。

**判定**：**可达**（玩家施放魔号 46）；**装载引入的行为变化 = 0**（施法者格必 Valid）。**未能确证**：无。

---

### 1.4 `slave.rs::World::recall_slave`（召回到主人身边）——**可达（条件）；装载后 RNG 会变（R 级）**

**我读到的事实**

- 定义：`crates/world/src/slave.rs:631`。
- 唯一非测试调用者：`crates/world/src/slave.rs:202`（`summon_slave`，定义 `slave.rs:152`；
  仅在 `MakeSlave` 返回 null 且 `magic_id == SKILL_SINSU` 的 else 支）。
- `summon_slave` 唯一非测试调用者：`bins/game-svr/src/slave.rs:187`（`summon_from_spell`，定义 `:171`）。
- `summon_from_spell` 唯一非测试调用者：`bins/game-svr/src/slave.rs:280`（`cast_slave_spell`，定义 `:254`）。
- `cast_slave_spell` 唯一非测试调用者：`main.rs:5346`（`handle_spell_cmd` 的 17/30/41 支，`main.rs:5284`（`matches!`；`SKILL_SKELLETON` 在 `5286`））。
- 格子读取：`slave.rs:656-657` 闭包 `|x, y| map.get_cell_info(x, y).1`（喂给 `space_move_get_rand_xy`，`slave.rs:396`）。

**具体调用链（file:line → …）**

```
main.rs:7895 fn sim_loop
  → main.rs:8729 handle_spell_cmd(...)                 // CM_SPELL(3017)
  → main.rs:5024 handle_spell_cmd
  → main.rs:5284（`matches!`）支 SKILL_SKELLETON(5286)|SKILL_SINSU|SKILL_ANGEL
  → main.rs:5346 game_svr::slave::cast_slave_spell(...)
  → bins/game-svr/src/slave.rs:254 cast_slave_spell
  → bins/game-svr/src/slave.rs:280 summon_from_spell(...)
  → bins/game-svr/src/slave.rs:171 summon_from_spell → crates/world::summon_slave
  → crates/world/src/slave.rs:152 summon_slave
     [ gate_ok==false 或 spawner.spawn 返回 None ]
  → crates/world/src/slave.rs:202 world.recall_slave(...)   // 仅 magic_id==SKILL_SINSU(30)
  → crates/world/src/slave.rs:631 World::recall_slave
  → crates/world/src/slave.rs:656 space_move_get_rand_xy(..., |x,y| map.get_cell_info(x,y).1)  // 读 ok
  → crates/world/src/slave.rs:663 map.move_object(...)                                      // 读 Valid
```


> **2026-10-17 订正（来源 `8e9244388` / `e7d432189`）**：文中 `8e9244388` 平移后的行号按 `e7d432189` 实测订正——`World::recall_slave` 定义 **`slave.rs:584`**（原文 `:631`）；唯一非测试调用 **`slave.rs:202`**（不变）；闭包读格 **`slave.rs:609-610`**（原文 `:656-657`）；`map.move_object` **`slave.rs:616`**（原文 `:663`）；`space_move_get_rand_xy` 由 `slave.rs:391` `pub use crate::space_move::space_move_get_rand_xy;` 再导出、定义在 **`space_move.rs:23`**（原文写 `slave.rs:396`）；`summon_slave` 定义 `slave.rs:152`（不变）、`summon_from_spell` 定义 `bins/game-svr/src/slave.rs:171` / 调用 `:280`、`cast_slave_spell` 定义 `:254` / 生产调用 `main.rs:5529`；17/30/41 的 `matches!` 在 `main.rs:5468-5472`。理由 `8e9244388` 把 `space_move_get_rand_xy` 拆出独立模块 + 后续片行号增长。复核：`sed -n '584p;609p;616p' mir2-rs/crates/world/src/slave.rs` ⇒ 期望 `pub fn recall_slave(` / `let found = space_move_get_rand_xy(...` / `map.move_object(`。

**装载后行为会不会变**：**会变（R 级：RNG 流水 + 落点）**。

- `recall_slave` 先算 `front_position(cx, cy, caster_dir, ...)`（主人正前方 `(fx,fy)`，`slave.rs:655`），
  再 `space_move_get_rand_xy(..., fx, fy, |x,y| get_cell_info(x,y).1)`。
- `space_move_get_rand_xy`（`slave.rs:396`）首轮 `if can_walk(...) { return true }`：
  - 装载前：`(fx,fy)` 界内恒 `ok=true` ⇒ **立即返回 `(fx,fy)`，不抽任何随机数**。
  - 装载后：`(fx,fy)` 若是墙 ⇒ 进移位支 ⇒ 每轮 `*x += n18` 或 `rng.random_below(...)` ⇒
    **消耗 `world.rng`** + 落点变。⇒ **R 级**（上游分级：RNG 抽数序列）。
- 触发条件：**魔号 30（召唤神兽）** 且 `MakeSlave` 侧返回 null（`gate_ok==false` = 宝宝数已满 `nMaxMob`，
  或 `MonsterTableSpawner` 失败）**且主人正前方是墙格** ⇒ **条件可达**，非常态路径。

**复核命令**

```
grep -rn "recall_slave" --include=*.rs .
# 期望：crates/world/src/slave.rs:147(注释),202(调用),631(定义),648 + 测试
sed -n '196,210p;631,690p' crates/world/src/slave.rs
sed -n '396,445p' crates/world/src/slave.rs
# 期望：pub fn space_move_get_rand_xy<F>(...) 首轮 if can_walk(...) { return true }
```

> **2026-10-17 订正（来源 `8e9244388` / `e7d432189`）**：本段复核命令的 `sed` 范围已过期——现值 `sed -n '196,210p;584,625p' crates/world/src/slave.rs`（期望：`:202` `world.recall_slave(`；`:584` `pub fn recall_slave(`；`:609` `let found = space_move_get_rand_xy(...`）；`sed -n '23,60p' crates/world/src/space_move.rs`（原 `sed -n '396,445p' crates/world/src/slave.rs`；期望首轮 `if can_walk(x, y) {` / `return Some((x, y));`）。

**判定**：**可达（条件：魔号 30 + 宝宝满 + 主人正前方墙）**；装载后 **RNG 流水会变**（**R 级**）。
**未能确证**：真机上「魔号 30 且宝宝数已达 `nMaxMob` 且主人正前方是墙」的频率——需真机/造场景或断点。
**建议**：若窗口核 RNG，把本路径列为与刷怪环同族「必核候选」，构造用例 = 主人站墙前 + 宝宝满。

---

### 1.5 `ground.rs` 的地面物品落点（`put_item`）——**可达；装载后零影响（结论成立）**

**我读到的事实**

- 文件是 **`bins/game-svr/src/ground.rs`**（上游写作 `ground.rs:259`，实为 **`:263`**）。
- 模型：`GroundMap { width, height, cells: Vec<Vec<GroundItem>> }`（`ground.rs:189-193`），**无 `Attribute`/`Valid` 字段**。
- `put_item`（`ground.rs:263`）：`item_id==0 ⇒ false`；否则 `match self.idx(x, y)`——`idx()`（`ground.rs:206`）
  **只做界内判定**（`x∈[0,width) && y∈[0,height)`）；界内 ⇒ `push` 并 `true`。
- `cell_match`（`ground.rs:216`）= `self.idx(x,y).is_some()`（纯几何）。
- 与 `world.maps` 的**唯一**连接点：`bins/game-svr/src/pickwire.rs:141 build_ground_map(view, map_id, width, height, catalog)`
  ——`width/height` 取自 `World::maps[map_id]`（**尺寸**），物品来自 `FloorItemView`（world 的**只读地面视图**），
  逐项 `map.put_item(v.x, v.y, item)`。⇒ 共享的只有**尺寸**与**物品清单**，**没有属性**。

**装载后会不会变**：**不会**（结论成立）。

- 装载 `mapwire::apply_map_cells` 只写 `world.maps` 的 `MapCell.Attribute`（上游 §4 总判 + `map.rs set_cell_attributes`），
  **不碰** `GroundMap.cells`。
- `GroundMap` 的任何读取（`get_item`/`get_item_ex`/`delete_item`/`put_item`）都不读属性，也不调 `world.maps` 的
  `get_cell_info`。⇒ **零影响**。
- 上游 §2 #25 提到的既有差异（oracle `AddItemToMap` 有 `cellSuccess && cellInfo.Valid` 门）仍成立，但那是
  **既存简化**（非装载引入）；且生产 `MapItem.ItemId` 恒 0 ⇒ oracle 首行即 `false`（`ground.rs` 模块头
  「正常 CM_DROPITEM 恒失败」）。**与装载无关**。

**复核命令**

```
grep -n "fn idx\|fn put_item\|fn cell_match\|struct GroundMap\|cells:" bins/game-svr/src/ground.rs
# 期望：206 fn idx / 216 fn cell_match / 263 fn put_item / 189 struct GroundMap / 192 cells: Vec<Vec<GroundItem>>
sed -n '186,200p;204,220p;259,278p' bins/game-svr/src/ground.rs
grep -rn "GroundMap" --include=*.rs bins/game-svr/src/ | grep -v "ground.rs:"
# 期望：只 pickwire.rs:37,136,142-148（build_ground_map）与 main.rs:2080 注释
sed -n '136,170p' bins/game-svr/src/pickwire.rs
```

**判定**：**可达**（CM_PICKUP/CM_DROPITEM 经 `main.rs:8454-8455` 派发）；**装载后零影响**（结论成立）。**未能确证**：无。

---

### 1.6 `ViewRange`/AOI（`BaseObject.ViewRange.cs`）——**可达；`ok` 恒真→按属性，但输出不变**

**我读到的事实**

- 本仓读格子属性处（上游 §2 行 15 对应）：
  - `crates/world/src/aoi.rs:91`（函数 `search_view_range`，定义 `:62`）：
    ```
    let (cell_len, cell_ok) = { let (cell, success) = map.get_cell_info(x, y); (cell.count(), success) };
    if !cell_ok || cell_len == 0 { continue; }        // aoi.rs:93-95
    ```
  - `crates/world/src/aoi.rs:211`（`search_view_range_death`，定义 `:182`）——同形。
- oracle 对应（`src/M2Server/Actor/BaseObject.ViewRange.cs:79-80`；death 在 `:183-184`）：
  ```
  ref MapCellInfo cellInfo = ref Envir.GetCellInfo(nX, nY, out bool cellSuccess);
  if (cellSuccess && cellInfo.IsAvailable) { ... }
  ```
- 真机可达性：世界 tick（`world.rs:927 fn tick`）在 AOI 刷新段调 `search_view_range_death`（`world.rs:1180`）
  与 `search_view_range`（`world.rs:1190`），逐实体、逐图。

**「装载前恒 X → 装载后按属性」的具体一行**

```
装载前：get_cell_info(x,y).1（= cellSuccess）对界内每格恒 true ⇒ !cell_ok 恒 false
装载后：cellSuccess = (Attribute == Walk) ⇒ 墙格 false ⇒ continue（跳过该格）
```

**装载后行为如何变 / 是否真机可见**：**输出不变**（**与上游 §4-3 一致**）。

**支撑「不变」的事实**：`add_cell_object`（`map.rs`）有 `success && cell.valid()` 门 ⇒ **对象只能落 Valid 格**；
`.map` 静态装载发生在任何实体落图**之前**（`main.rs:7959 apply_map_cells` 在 `sim_loop` 的世界构建段，
早于 `main.rs:7990 add_map_routes`、早在任何 `enter_world`/刷怪之前；本卡复核
`awk 'NR>=7895 && NR<=7960' bins/game-svr/src/main.rs | grep -n "add_entity"` ⇒ 空）。

⇒ 墙格上**不可能**有已注册对象 ⇒ AOI 跳过墙格 ⇒ `visible_actors` 与装载前**逐条相同**。

> **2026-10-17 订正（来源 `e7d432189`）**：本段原文「`main.rs:7959 apply_map_cells`」「`main.rs:7990 add_map_routes`」「`world.rs:927 fn tick`」「`world.rs:1180`/`:1190`」已过期——现值「`apply_map_cells` 在 `main.rs:8164`、`add_map_routes` 在 `main.rs:8195`；`World::tick` 定义 `world.rs:1120`，AOI 调用 `search_view_range_death` 在 `world.rs:1373`、`search_view_range` 在 `world.rs:1383`」。复核：`grep -n "apply_map_cells\|add_map_routes" mir2-rs/bins/game-svr/src/main.rs` ⇒ `8164`/`8195`；`grep -n "search_view_range" mir2-rs/crates/world/src/world.rs` ⇒ `1373`/`1383`；`awk 'NR>=8100 && NR<=8195' mir2-rs/bins/game-svr/src/main.rs | grep -n "add_entity\|enter_world\|spawn"` ⇒ 空。

**复核命令**

```
sed -n '86,96p;205,215p' crates/world/src/aoi.rs
sed -n '55,82p;155,187p' src/M2Server/Actor/BaseObject.ViewRange.cs
grep -n "search_view_range" crates/world/src/world.rs
# 期望：19(import) 1180(death) 1190(主体)，均在 fn tick(927) 内
```

**判定**：**可达**（每 tick、每实体）；**装载后输出不变**。**未能确证**：若将来**运行期**掩码
（`castle.rs apply_structure_events` 的 `set_map_xy_flag`，归卡 M3⑧）把某实体所在的 Valid 格标成墙 ⇒
该格被 AOI 跳过。本卡不判 M3⑧（无生产调用方，上游 §1B #36 已述）。

---

### 1.7 `cetin_near_xy` 的全部生产调用者——**可达；`!ok` 支「恒不进→按属性」，结果不变**

**我读到的事实**

- 定义：`bins/game-svr/src/queryuser.rs:29`；`!ok` 支在 `:36`：
  ```
  let (cell, ok) = map.get_cell_info(cx, cy);      // queryuser.rs:35
  if !ok || !cell.is_available() { continue; }     // queryuser.rs:36
  ```
- **全部生产调用者（3 个函数、7 处）**：
  1. `main.rs:2672`（`handle_query_user_name_cmd`，定义 `:2628`）——`CM_QUERYUSERNAME`；
  2. `main.rs:2768`（`handle_query_user_state_cmd`，定义 `:2720`）——`CM_QUERYUSERSTATE`；
  3. `main.rs:4995`（`spell_snap_target`，定义 `:4988`）——被 `handle_spell_cmd` **5 处**调用：
     `main.rs:5154 / 5232 / 5294 / 5429 / 5516`（= 上游所说「spell 路径 5 处」）。

**装载前 → 装载后（逐调用者）**

| 调用者 | 装载前 | 装载后 | 结果变否 |
|---|---|---|---|
| `main.rs:2672` CM_QUERYUSERNAME | 界内每格 `ok=true` ⇒ `!ok` 恒 false ⇒ 9 格全扫 | 窗口内墙格 `ok=false` ⇒ 跳过 | **不变** |
| `main.rs:2768` CM_QUERYUSERSTATE | 同上 | 同上 | **不变** |
| `main.rs:4995`（`spell_snap_target`）×5 处 | 同上 | 同上 | **不变** |

**前两处此前是否也因「每格 Walk」而恒命中？**：**`!ok` 支确曾在界内恒不进**（即每格都进入扫描）。
**但命中与否不取决于此**——`cetin_near_xy` 是**逐格扫 `ObjList` 找 `target_id`**（`queryuser.rs:39-47`），
而非「坐标窗内即命中」。对象只在 Valid 格注册 ⇒ 目标所在格 `ok=true` ⇒ 该格照扫 ⇒ 命中结果与装载前**逐条相同**。

**我读到的佐证**：本仓已有牙 `bins/game-svr/src/mapwire.rs:674 loaded_low_wall_cell_is_skipped_by_cetin_near_xy`
（tier=R）——它**构造**「目标站在墙格」的等价态来证明掩码面生效，用例头明确写：

> `.map` 静态装载本身发生在任何实体落图**之前**（故生产次序上本牙的「装载前后」两半不会同时真实发生，
> 这里是把它当**等价状态**构造出来）

⇒ 该牙的「装载后不命中」是**构造态**、非生产可达态。（阴性对照 `mapwire.rs:622
all_walk_map_stays_walkable_and_hittable_after_load` 证明可走格上目标仍命中。）

**复核命令**

```
grep -rn "cetin_near_xy" --include=*.rs bins/game-svr/src/main.rs
# 期望：2672 / 2768 / 4995（+ doc 注释 4977/5151/5229/5292/5427/5514）
grep -n "spell_snap_target(" bins/game-svr/src/main.rs
# 期望：4988(定义) 5154 5232 5294 5429 5516（5 个调用点）
sed -n '29,51p' bins/game-svr/src/queryuser.rs
sed -n '659,705p' bins/game-svr/src/mapwire.rs
```

**判定**：**可达**（三条客户端指令）；装载后 `!ok` 支「恒不进 → 按属性」但其**输出不变**。**未能确证**：无。

---

## 2. 装载后行为**会变**的路径清单（按真机可达性排序）

| 序 | 路径 | 真机可达性 | 变化语义 | 级别 | 证据（file:line） |
|---|---|---|---|---|---|
| 1 | `recall_slave`（宝宝召回到身边，魔号 30） | **可达（条件）**：宝宝满 + 主人正前方墙 | 读 `get_cell_info().1` ⇒ 进移位支 ⇒ **RNG 流水 + 落点变** | **R** | `crates/world/src/slave.rs:202,631,656-657` |
| 2 | `mag_make_clone`（魔号 46 分身） | 可达；但**变的条件生产不可达**（施法者格必 Valid） | 读 `cell.valid()` ⇒ `created` 变 | L（条件不可达） | `crates/world/src/magic.rs:1259-1261`；`slave.rs:1166-1191` |
| 3 | `push_now` / `char_pushed`（推人） | **不可达**（`Command::Push` 无生产者） | 读 `get_cell_info().1`/`cell_free` ⇒ 若接线会变 | （未接线） | `world.rs:2151,2064`；`magic.rs:1018` |
| 4 | `mag_sace_move`（魔号 21） | **不可达**（无生产者） | 读 `ok` ⇒ 若接线会变（RNG） | （未接线） | `magic.rs:1177-1178,1196` |
| 5 | `aoi.rs::search_view_range(_death)` | 可达（每 tick） | `ok` 恒真→按属性 ⇒ **输出不变** | L | `aoi.rs:91,211`；`world.rs:1180,1190` |
| 6 | `cetin_near_xy`（2 条查询指令 + spell snap ×5） | 可达 | `!ok` 恒不进→按属性 ⇒ **输出不变** | L | `queryuser.rs:35-36`；`main.rs:2672,2768,4995` |
| 7 | `ground.rs::put_item` | 可达 | **不变（零影响）** | L | `ground.rs:206,263`；`pickwire.rs:141` |
| 附 | `merchant_run` →（`map_random_move` → `move_object` → `cell.valid`）**（`npc_run` 不可达：固定 NPC `BoCanMove` 恒 false）** | **可达（条件）**（`main.rs:8049 enable_merchant_tick`；商人 DB `CanMove!=0 && MoveTime>0`） | 读 `move_object` 的 `cell.valid()` ⇒ 落点为墙 ⇒ 早退（**不移动/不再抽数**）；**oracle 则进移位环继续抽数** ⇒ **不同构** | **R（真缺口：落点 + RNG 流水双分歧）** | `merchant.rs:584,748,854,873`；`map.rs move_object`；`world.rs:1149`；oracle `BaseObject.cs:1214/1103/1040` |

> **2026-10-17 订正（来源 `77dee0813` / `8e9244388` / `e7d432189`）**：本表行号按 `e7d432189` 重测——第 1 行 `slave.rs:202,584,609-610`；第 2 行 `magic.rs:916-918`、`slave.rs:1119,1129,1147`；第 3 行 `world.rs:2344,2257`、`magic.rs:707`（`char_pushed` 本体），生产驱动 `world.rs:859`（`push_now`）；第 4 行 `magic.rs:853,873,879` + `space_move.rs:23`（生产入口 `world.rs:909`）；第 5 行 `aoi.rs:91,211`、`world.rs:1373,1383`；第 6 行 `queryuser.rs:35-36`、`main.rs:2672,2768,4995`、`spell_snap_target` **6 处**（`5154/5232/5298/5477/5612/5699`）；第 7 行 `ground.rs:206,263`、`pickwire.rs:141`；附行 `merchant.rs:661,748,905,929`、`world.rs:1342`（`merchant_tick` 调用）、生产开关 `main.rs:8254`（`enable_merchant_tick`）。理由 接线/拆分/方案 C 行号平移。复核：`grep -n "fn cell_free\|fn push_now" mir2-rs/crates/world/src/world.rs` ⇒ `2257`/`2344`；`grep -n "pub fn merchant_run\|fn map_random_move_xy\|pub(crate) fn map_random_move\|move_object(" mir2-rs/crates/world/src/merchant.rs` ⇒ `661`/`855`/`905`/`929`；`grep -n "enable_merchant_tick" mir2-rs/bins/game-svr/src/main.rs` ⇒ `8254`。

**附表（附行）说明（2026-10-17 独立复验**否掉**原「同构」判定）**：`merchant_run`（`merchant.rs:661`）→
`map_random_move`（`:748`/`:854`）已抽 2 次 `random_below`，随后 `move_object(ignore_block=true)`：装载后若落点是墙，
`move_object` 的 `if !cell.valid() { return false }` ⇒ **不移动、不再抽数**。而 oracle 的 `MapRandomMove`
（`BaseObject.cs:1214-1237`）→ `SpaceMove`（`:1103-1166`）→ `SpaceMoveGetRandXY`（`:1040-1096`）在落点不可走时
**进移位环、继续抽 `Random(Width)`/`Random(Height)`**，最后 `AddMapObject`。⇒ **落点与 RNG 流水都会分歧**（原稿引的
`Envirnoment.cs:380` 属 `MoveToMovingObject`（`:337`），**不是**这条路径）。**这是真缺口而非「同构」**：已立卡
`openmir2-map-random-move-shift-loop`（R）承接实现；本片只登记（窗口项见 `82-two-side-window-runbook.md` §2e）。
**可达性收窄**：`npc_run` 的该支**不可达**（固定 NPC `BoCanMove` 恒 false，`npc.rs` 已登记），只有 `merchant_run`
条件可达（DB `CanMove!=0 && MoveTime>0`）。

---

## 3. 不可确证条目与所需证据

| 条目 | 为何不可确证（静态） | 需要什么 |
|---|---|---|
| ~~魔号 8/21 是否「待接线」而非「永不接线」~~ **（已确证，2026-10-17）** | **静态可证**（原稿判「不可确证」是深度不足）：`MagicConst.cs:36 SKILL_FIREWIND=8`、`:88 SKILL_SPACEMOVE=21`；oracle `MagicManager.cs:259`（及 `:602` ENERGYREPULSOR）`case SKILL_FIREWIND`→`MagPushArround`、`:403 case SKILL_SPACEMOVE`→`MagSaceMove` ⇒ **oracle 有生产分支**，本仓 `handle_spell_cmd` 缺 8/21 属**待接线** | 仅剩「玩家能否学到」属数据面（真机/查 Magic 表） |
| `recall_slave` 的触发频率（宝宝满 + 主人正前方墙） | 需真机玩家态（宝宝数 `nMaxMob`、朝向、地图墙分布） | 真机造场景：主人站墙前 + 宝宝数满；或进程级对拍 |
| ~~`mag_make_clone` 是否会在「施法者格非 Valid」时被触发~~ **（已关闭，2026-10-17）** | 上游那条差异**已修**：`session.rs::enter_map` 改为 oracle 同形（不移除角色）⇒ 病态存档（墙格出生）/切图缩图下角色可「在世界上但不入格表」⇒ 该支曾 **边缘可达**。**同日后续片 `openmir2-enter-shift-loop`** 在宿主进图路径接入 oracle `WorldServer.cs:371-395` 移位环 ⇒ **宿主路径的「墙格出生」不可达**（oracle 同因）；该支仍可由切图缩图/直接调用 `World::enter_map` 产生，登录出生不再产生 | — |
| ~~`map_random_move` 消费点的级别~~ **（已定级，2026-10-17）** | 独立复验**否掉**「与 oracle 同构」：本仓 `move_object` 早退、oracle 进 `SpaceMoveGetRandXY` 移位环继续抽数 ⇒ **落点 + RNG 双分歧** | **R**，已立卡 `openmir2-map-random-move-shift-loop` |
| ~~各图 `(0,0)` 是否 Walk~~ **（已确证，2026-10-17）** | **静态可解**：`.map` 二进制逐图读 cell0 即可（`b192bd9f3` 的复验者已示范解原始二进制）。**已解**：322 注册图中 `(0,0)` 为墙 **317**、为 Walk **5**（清单与复核命令见 `mapwire.rs` 模块头「真机静态普查」段与 `audit-valid-consumers-20261017.md` §6 R8 的更正） | — |

> **2026-10-17 订正（来源 `e7d432189`）**：本表 `map_random_move` 行的实现位置按实测为 `merchant.rs:905`（`map_random_move`）/`:929`（`move_object` 调用）/`:855`（`map_random_move_xy`）；`merchant_run` 在 `:661`、其调用在 `:748`。Oracle 侧 `BaseObject.cs:1214/1103/1040` 不变；附表说明中的 `:854` 现为 `:855`（`map_random_move_xy` 定义）。复核：`grep -n "pub fn merchant_run\|fn map_random_move_xy\|pub(crate) fn map_random_move\|move_object(" mir2-rs/crates/world/src/merchant.rs` ⇒ `661`/`855`/`905`/`929`。

**未用「大概/应该」**：以上凡静态确证不到的，一律标「不可确证 + 需要 X」。

---

## 4. 复核命令汇总（可直接复制）

```
# C1 char_pushed / push_now 无生产者
grep -rn "char_pushed\|mag_push_arround" --include=*.rs .            # bins/ 下 0
grep -rn "Command::Push" --include=*.rs .                            # bins/game-svr/src 下 0
grep -rn "\.enqueue(" --include=*.rs bins/game-svr/src/              # 只有 Butch(1718)

# C2 mag_sace_move 无生产者
grep -rn "mag_sace_move\|SaceMove" --include=*.rs .                  # 只有 tests/map_effect_vectors.rs

# C3 mag_make_clone 生产链
grep -rn "mag_make_clone\|spawn_clone\|cast_clone_spell" --include=*.rs .

# C4 recall_slave 生产链
grep -rn "recall_slave\|summon_from_spell\|cast_slave_spell" --include=*.rs .
sed -n '196,210p;631,690p' crates/world/src/slave.rs

# C5 ground.rs 独立模型
grep -n "fn idx\|fn put_item\|fn cell_match\|struct GroundMap" bins/game-svr/src/ground.rs
sed -n '136,170p' bins/game-svr/src/pickwire.rs                       # build_ground_map：只共享 width/height

# C6 AOI 门
sed -n '86,96p;205,215p' crates/world/src/aoi.rs
grep -n "search_view_range" crates/world/src/world.rs                 # 1180/1190 在 fn tick(927)

# C7 cetin_near_xy
grep -rn "cetin_near_xy" --include=*.rs bins/game-svr/src/main.rs     # 2672/2768/4995
grep -n "spell_snap_target(" bins/game-svr/src/main.rs                # 5154/5232/5294/5429/5516
sed -n '29,51p' bins/game-svr/src/queryuser.rs

# C8 装载次序（早于任何实体落图）
grep -n "apply_map_cells\|add_map_routes\|enable_merchant_tick\|enable_npc_tick\|^fn sim_loop" bins/game-svr/src/main.rs
awk 'NR>=7895 && NR<=7960' bins/game-svr/src/main.rs | grep -n "add_entity\|enter_world\|spawn"   # 空

# C9 魔号表（8 抗拒火环 / 21 瞬息移动）
grep -n "MaigicNameList" src/CloudGate/TableDef.cs
grep -n "抗拒火环\|瞬息移动" src/GameGate/TableDef.cs

# C10 oracle 对读锚
sed -n '1850,1876p;1103,1168p;1040,1084p;854,901p' src/M2Server/Actor/BaseObject.cs
sed -n '55,82p;155,187p' src/M2Server/Actor/BaseObject.ViewRange.cs
sed -n '1337,1341p' src/M2Server/Magic/MagicManager.cs
```

> **2026-10-17 订正（来源 `77dee0813` / `8e9244388` / `e7d432189`）**：§4 命令的期望按 `e7d432189` 重测：
> - **C1**：`grep -rn "char_pushed\|mag_push_arround" --include=*.rs .` 在 `crates/world/src/magic.rs` 命中 `690`（`char_pushed` 定义）/`:778`（`mag_push_arround` 定义）/`:801`（内部调用）；`crates/world/src/world.rs` 的测试另命中 `:3610/:3613/:3630`；`bins/game-svr/src/main.rs` 的 `#[cfg(test)]` 有 2 处 `mir2_world::magic::char_pushed`（`:22648` 注释 / `:22716` 调用）；生产直调是 `world.rs:859` `self.push_now(...)`。`Command::Push` 在 `world.rs:1245`（match 臂）、`:3576`/`:3616`（测试）、`bins/game-svr/tests/action_wire_e2e.rs:29`。`grep -rn "\.enqueue(" --include=*.rs bins/game-svr/src/` ⇒ `main.rs:1485/1492`（msgqueue）、`1718`（`Command::Butch`）；无 Push/Rush。
> - **C2**：`grep -rn "mag_sace_move\|SaceMove" --include=*.rs .` 在 `crates/world/src/magic.rs` 命中 `823/825/841/843/852/853`（`SaceMoveInput`/`Outcome`/`mag_sace_move`），`crates/world/src/magic_effect.rs` 有 `:52` 等注释命中，`crates/world/tests/map_effect_vectors.rs:294` 为调用；`bins/` 下只有 `main.rs` 的注释/效果体（`MagSaceMove` 字样，不是 `mag_sace_move` 调用）。
> - **C3**：`grep -rn "mag_make_clone\|spawn_clone\|cast_clone_spell" --include=*.rs .` ⇒ `magic.rs:916/918`；`crates/world/src/slave.rs:1119`（定义）/`:1129`（调用）；`bins/game-svr/src/slave.rs:523`（定义）/`:529`（`spawn_clone` 调用）；`main.rs:5732`（`cast_clone_spell` 调用）。
> - **C4**：`grep -rn "recall_slave\|summon_from_spell\|cast_slave_spell" --include=*.rs .` ⇒ `crates/world/src/slave.rs:362`（`recall_slave_target`）/`:584`（定义）/`:202`（调用）；`bins/game-svr/src/slave.rs:171/254/280`；`main.rs:5529`。`sed -n '196,210p;584,625p' crates/world/src/slave.rs` ⇒ 见 §1.4 订正。
> - **C5**：不变（`ground.rs:206/263`、`pickwire.rs:141`）。
> - **C6**：`grep -n "search_view_range" crates/world/src/world.rs` ⇒ `19`（import）/`1373`（death）/`1383`（主体），均在 `fn tick`（`1120`）内。
> - **C7**：`grep -rn "cetin_near_xy" --include=*.rs bins/game-svr/src/main.rs` ⇒ 生产 `2672/2768/4995`（+ doc 注释 `4977/5151/5229/5475/5610/5697`）；`grep -n "spell_snap_target(" bins/game-svr/src/main.rs` ⇒ `4988`（定义）+ **6 个调用点** `5154/5232/5298/5477/5612/5699`。
> - **C8**：`grep -n "apply_map_cells\|add_map_routes\|enable_merchant_tick\|^fn sim_loop" bins/game-svr/src/main.rs` ⇒ `8100`（`sim_loop`）/`8164`（`apply_map_cells`）/`8195`（`add_map_routes`）/`8254`（`enable_merchant_tick`）；`awk 'NR>=8100 && NR<=8195' bins/game-svr/src/main.rs | grep -n "add_entity\|enter_world\|spawn"` ⇒ 空。
> - **C9/C10**：oracle 行号不变。

---

## 5. 残留物清单

- 本报告：`/e/tmp/reachability-audit.md`（唯一写盘物）。
- 仓库：**零改动**（未写任何文件、未建 worktree、未 `git add`/`commit`）。
- 未起任何服务/客户端/假人；未占端口；未写库；未动 `E:/MirServer/**`。
- **未跑 cargo**（全程静态调用图 + `grep`/`sed`/`cat`）。
