# ⑧-6 第②片（实体挂图）：`UserCastle.Initialize` —— 证据包

- 卡：`openmir2-m3-castle-file-load`（M3⑧-实现 6，第②片）
- 分支：`worktree-m3-castle-file-load-f2`
- base：`origin/master c8a3487b1f6a117065b0105836105b7ed3c0529b`
- 写集：`mir2-rs/crates/world/src/castle.rs`（+ `lib.rs` 导出行）+ 本证据目录。零 `bins/`、零 `crates/envir`、零 `src/ sql/ MirClient`。

## 1. 本片做了什么

按 oracle `UserCastle.Initialize()`（`src/M2Server/Castle/UserCastle.cs:108-226`）在
`mir2-rs/crates/world/src/castle.rs` 落 [`UserCastle::initialize`]：

1. `:112` `GetMapOfServerIndex(MapName) == M2Share.ServerIndex` 门（宿主 trait `map_owner_is_self`）；
2. `:114-123` 皇宫 / 密道 `FindMap` 失败告警（不阻断）；
3. `:125` `CastleEnvir == null` ⇒ 告警「城堡所在地图不存在」并整体跳过；
4. `:128-137` 主门 `RegenMonsterByName` → `WAbil.HP=nHP` → `Castle=this` → `MainDoor.nStatus && Open()`；
5. `:139-168` 左/中/右墙（写 `HP` + `Castle`）；
6. `:170-185` 弓手 `i=0..12`：`nHP<=0` 跳过；成功 ⇒ `HP` + `Castle` + `GuardDirection=3`；
7. `:186-207` 守卫 `i=0..12`：`nHP<=0` 跳过；成功 ⇒ **只**写 `HP`（oracle 不设 `Castle`）。

宿主面抽成 trait `CastleInitializeEnv`（`map_owner_is_self` / `find_map` /
`regen_monster_by_name` / `warn`），调用序被文档钉死为 oracle 遍历序；本模块零 RNG、零墙钟。

新增可观测副作用 `CastleEvent::SpawnDefenseUnit(DefenseSpawn)`（角色 / 坐标 / `hp` /
`castle_owner` / `guard_direction`），门 `Open()` 产生的帧与掩码仍走既有的
`CastleEvent::Structure(StructureEvent)`（⑧-8）。

## 2. 照搬的 oracle 怪行为（不是差异）

- **墙的失败告警引用上一单元的名字**（`UserCastle.cs:152/162/172` 的 copy-paste 怪癖）：
  左墙失败报 `MainDoor.sName`、中墙失败报 `LeftWall.sName`、右墙失败报 `CenterWall.sName`；
- 主门造怪失败用英文文案 `[Error] UserCastle.Initialize MainDoor.UnitObj = nil`（`:142`）；
- 守卫造怪**不**绑 `Castle`（`:186-207` 无 `Castle = this`），与门/墙/弓手不同。

## 3. 判据与读数

- 构造级用例 **49 passed / 0 failed**（`-p mir2-world --lib castle`，见 `08-castle-lib-test.txt`），
  其中本片新增 8 条：调用序 1+3+12+12、`GuardDirection=3`、守卫不绑 Castle、
  `MainDoorOpen=false` 不 `Open()`、`nHP=0` 跳过、非本服地图 no-op、缺城图告警并跳过、
  告警 copy-paste 怪癖、主门缺怪英文告警。
- 四门禁（合并到 `origin/master c8a3487b1` 的树上）：`fmt rc=0` / `clippy -D warnings rc=0` /
  `99 suites 1167 passed 0 failed 22 ignored` / `msgcodegen 604 rc=0`（`01`–`04`）。
- 改坏必红 3 条（`06-redcheck.txt`）：RC1 弓手失绑 Castle、RC2 “修正”左墙告警名、RC3 删除
  `nHP<=0` 跳过 ⇒ 各自点名一条用例 FAILED；还原后源 sha256 一致、回 49/49。

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
