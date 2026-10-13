# ⑧-6 第②片（实体挂图）：`UserCastle.Initialize` —— 证据包

- 卡：`openmir2-m3-castle-file-load`（M3⑧-实现 6，第②片）
- 分支：`worktree-m3-castle-file-load-f2`
- base：`origin/master c8a3487b1f6a117065b0105836105b7ed3c0529b`
- 写集：`mir2-rs/crates/world/src/castle.rs`（+ `lib.rs` 导出行）+ 本证据目录。零 `bins/`、零 `crates/envir`、零 `src/ sql/ MirClient`。

> **2026-10-13 追加（reviewer-6 M-1/M-2/O-1 收口，见 §6）**：本文件 §1/§2 的 oracle 行号
> 已逐个按**冻结 `src/`** 订正，并由新增门 `castle_oracle_anchor_gate.py` 机判锁死（改错必红）；
> `09-hash-check.txt` 已重写为**最终 GREEN**；§3 用例计数已订正为 **8**（并补列漏项）。

## 1. 本片做了什么

按 oracle `UserCastle.Initialize()`（`src/M2Server/Castle/UserCastle.cs:108-226`）在
`mir2-rs/crates/world/src/castle.rs` 落 [`UserCastle::initialize`]：

1. `UserCastle.cs:113` `GetMapOfServerIndex(MapName) == M2Share.ServerIndex` 门（宿主 trait `map_owner_is_self`）；
2. `UserCastle.cs:115-125` 皇宫 / 密道 `FindMap` 失败告警（不阻断）；
3. `UserCastle.cs:127-128` `CastleEnvir == null` ⇒ 告警「城堡所在地图不存在」并整体跳过；
4. `UserCastle.cs:130-138` 主门 `RegenMonsterByName` → `WAbil.HP=nHP` → `Castle=this` → `MainDoor.nStatus && Open()`；
5. `UserCastle.cs:144-173` 左/中/右墙（写 `HP` + `Castle`）；
6. `UserCastle.cs:174-193` 弓手 `i=0..12`：`nHP<=0` 跳过；成功 ⇒ `HP` + `Castle` + `GuardDirection=3`；
7. `UserCastle.cs:194-211` 守卫 `i=0..12`：`nHP<=0` 跳过；成功 ⇒ **只**写 `HP`（oracle 不设 `Castle`）。

宿主面抽成 trait `CastleInitializeEnv`（`map_owner_is_self` / `find_map` /
`regen_monster_by_name` / `warn`），调用序被文档钉死为 oracle 遍历序；本模块零 RNG、零墙钟。

新增可观测副作用 `CastleEvent::SpawnDefenseUnit(DefenseSpawn)`（角色 / 坐标 / `hp` /
`castle_owner` / `guard_direction`），门 `Open()` 产生的帧与掩码仍走既有的
`CastleEvent::Structure(StructureEvent)`（⑧-8）。

## 2. 照搬的 oracle 怪行为（不是差异）

- **墙的失败告警引用上一单元的名字**（`UserCastle.cs:152/162/172` 的 copy-paste 怪癖）：
  左墙失败报 `MainDoor.sName`、中墙失败报 `LeftWall.sName`、右墙失败报 `CenterWall.sName`；
- 主门造怪失败用英文文案 `[Error] UserCastle.Initialize MainDoor.UnitObj = nil`（`UserCastle.cs:142`）；
- 守卫造怪**不**绑 `Castle`（`UserCastle.cs:194-211` 内无 `Castle = this`），与门/墙/弓手不同。

## 3. 判据与读数

- 构造级用例：本片提交时 **49 passed / 0 failed**（base `c8a3487b1`）；本 M-1/M-2/O-1 收口片在
  master `16ae2607f` 上复跑 `castle` = **50 passed / 0 failed**（+1 = ⑧-8 `compare_lstr_*` 向量合并后新增），
  见 `08-castle-lib-test.txt`。其中本片**新增 8 条**（逐条，卡上 `patch` body 的计数以此为准）：

  1. `initialize_spawns_1_door_3_walls_12_archers_12_guards_in_oracle_order`
  2. `initialize_door_status_false_does_not_open`
  3. `initialize_skips_zero_hp_archers_and_guards`
  4. `initialize_non_owner_map_is_noop`
  5. `initialize_missing_castle_map_warns_and_skips_all_spawns`
  6. `initialize_warns_palace_and_secret_map_but_still_spawns`
  7. `initialize_wall_failure_warnings_use_previous_unit_name_quirk`
  8. `initialize_missing_main_door_uses_english_warning`

- 四门禁：本片原提交时（合并到 `origin/master c8a3487b1` 的树）`fmt rc=0` / `clippy -D warnings rc=0` /
  `99 suites 1167 passed 0 failed 22 ignored` / `msgcodegen 604 rc=0`；本 M-1/M-2/O-1 收口片在
  `origin/master 16ae2607f` 上复跑 `fmt rc=0` / `clippy rc=0` / `101 suites 1198 passed 0 failed 22 ignored` /
  `msgcodegen 604 rc=0`（`01`–`04`）。
- 改坏必红 3 条（`06-redcheck.txt`）：RC1 弓手失绑 Castle、RC2 “修正”左墙告警名、RC3 删除
  `nHP<=0` 跳过 ⇒ 各自点名一条用例 FAILED；还原后源 sha256 一致、回 49/49。
- oracle 锚一致性门（本片新增，`10-anchor-gate.txt`）：`castle_oracle_anchor_gate.py check`
  GREEN（本目录/castle.rs 声明的每条锚都在冻结 `src/` 对应行命中期望 token）+ `selftest`
  GREEN + 改坏必红（把 `UserCastle.cs:113` 写回 `:111` ⇒ RED）。

## 4. 未做（如实登记，不作绿）

- **`bins/game-svr` 接线**：把 `crates/envir` 的 `SabukWConfig` 适配成 `CastleDefenseSpecs`、
  在建城时调用 `initialize`（真造怪 = `RegenMonsterByName` 的宿主实现）——属第②片的 bins 面；
  本刻 `bins/game-svr` 正被其它车道占用，且 owner 尚未对 bins 串行序裁决（comment `018d7be2`）⇒ 另片交付。
- **真机两侧 `run_guild_castle_pair.ps1` diff=0**：需 ⑧ 全链接线 + `openmir2-m3-clock-injection` +
  owner 独占窗口。
- 本片不在 `crates/envir` 建 `SabukWConfig → CastleDefenseSpecs` 的转换（避免引入
  `mir2-world -> mir2-envir` 新依赖；依赖方向保持 `bins -> {envir, world}`）。

## 5. 残留物清单

- 本目录（证据件）为本片专属，保留。
- 临时目录 `E:/tmp/m3cs-cf2/`（改坏必红用的源快照 + RC patch 文本）为一次性中间物，不入库；
  隔离编译产物在 `E:/tmp/openmir2-m3cs-target/`（车道共享，非本片专属）。
- 收口片门 `castle_oracle_anchor_gate.py mutation` 的可选 `--out` 目录（默认落系统临时目录
  `castle-anchor-mut-*`）为一次性中间物，不入库；`10-anchor-gate.txt` 记录的是 stdout 读数。

## 6. reviewer-6 M-1/M-2/O-1 收口（2026-10-13，tier=L）

reviewer-6 的 approve（`4b568b85` @`62fb829134f9`）携带 3 条**非阻塞** findings；本片逐条收口：

### M-1 新增代码 doc-comment 的 oracle 行号漂移（`castle.rs` + 本文件 §1/§2）

根因：⑧-6 第②片新增块的锚行号按另一版本抄写，与**冻结** `src/M2Server/Castle/{UserCastle,CastleConf}.cs`
实际行号差 1–13 行，而同一 patch 的 `05-oracle-anchors.txt` 行号是正确的（自相矛盾）。
逐处订正（`代码引用 → 冻结 src/ 实际`）：

| 引用处 | 旧 | 新 |
|---|---|---|
| `CastleConf` `CastleMap` | `:36` | `:39` |
| `CastlePlaceMap` | `:44` | `:45` |
| `CastleSecretMap` | `:45` | `:46` |
| `MainDoor` | `:47-54` | `:49-55` |
| `LeftWall` | `:55-60` | `:56-61` |
| `CenterWall` | `:61-66` | `:62-67` |
| `RightWall` | `:67-72` | `:68-73` |
| `Archers[12]` | `:73-83` | `:74-83` |
| `Initialize` 地图门 | `:112` | `:113` |
| 皇宫/密道告警 | `:114-123` | `:115-125` |
| `CastleEnvir == null` | `:125` | `:127-128` |
| 主门块 | `:128-137` | `:130-138` |
| 三面墙 | `:139-158` | `:144-173` |
| 弓手循环 | `:161-184` | `:174-193` |
| 守卫循环 | `:186-206` | `:194-211` |
| `MainDoor.nStatus && Open()` | `:134-137` | `:135-138` |
| 弓手/守卫 `nHP<=0` | `:164/190` | `:177/197` |
| `Castle = this`（门/墙/弓手） | `:131/146/154/161/181` | `:134/148/158/168/186` |
| 守卫块（不设 `Castle`） | `:197` | `:202-206` |
| `GetMapOfServerIndex`（trait） | `:111` | `:113` |
| `FindMap`（trait） | `:114/119/125` | `:115/121/127` |
| `Initialize` 覆盖 HP 域 | `:131-220` | `:133-211` |
| 墙告警 copy-paste 怪癖 | `:148/156/166` | `:152/162/172` |
| 主门英文告警 | `:144` | `:142` |

同族两处（`06-redcheck.txt` RC1/RC3 的引用）一并订正：`:181`→`UserCastle.cs:186`、`:164`→`UserCastle.cs:177`。
本文件 §1/§2 同步（含把散落的裸 `:NNN` 统一为 `UserCastle.cs:NNN`，便于机判）。

### M-2 `09-hash-check.txt` 记的是中间态 RED

该件原记的是修复前快照（`verify --check-extra OK 10/10 + 多件 09`、`eolcheck RED 1/12`），
与 patch body/README 声称的 GREEN 矛盾。**本片重写 `09-hash-check.txt` 为收口后的最终读数**
（`verify --check-extra GREEN` + `eolcheck GREEN`，并把 `09` 自身纳入清单）。

### O-1 本文件 §3 用例计数与枚举不符

原写「新增 8 条」却枚举 9 项（且漏列 `initialize_warns_palace_and_secret_map_but_still_spawns`）。
现按实现（`castle.rs` 的 8 个 `initialize_*` 用例）逐条列全，计数 = **8**。

### 新增机判门（防同族再漂）

`mir2-rs/tests/parity/m3/castle_oracle_anchor_gate.py`：把本片新块的每条锚（`UserCastle.cs:NNN` /
`CastleConf.cs:NNN` 等）双向绑死——① 该锚文本必须出现在 `castle.rs` 或本 `README.md`
（写侧漂移 ⇒ RED）；② 冻结 `src/` 对应行必须命中期望 token（读侧/表漂移 ⇒ RED）。
`check` / `selftest` / 改坏必红见 `10-anchor-gate.txt`。
