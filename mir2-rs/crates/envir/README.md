# mir2-envir —— M2 地图层（Envir `*.map` 加载器）

交付卡：`openmir2-m2-map-loading`。格式与语义规格 = 该卡 comment `862396f8` 的**九条定案**
（owner 侧用合成 `.map` 在**部署件**上逐格探针得出，不是照源码猜的）。

## 范围与不做

- **做**：`*.map` 解析（12B/格、X 主序、Low 覆盖 High、门与 Status 合并）、
  `Point/*.txt` 点位表、取图路径口径、地图名→`map_id` 注册表、转 `crates/world::MapGrid`。
- **不做**（登记给 m2-svr 车道）：改 `bins/game-svr` / 起服务 / 改 `crates/world` /
  改 `src/**`（C# oracle）；`ConnectMapInfo` 之外的 MapInfo 标志解析（SAFE/FIGHT/… 不在本卡）。

## 库 API

```rust
use mir2_envir::{MapLoader, MapRegistry, CellAttribute};

// 1) 直接加载一张图（含同名点位表 <map 路径>.txt，按源码口径）
let loader = MapLoader::new("E:\\MirServer\\M2GameSvr", "Map", "Envir");
let map = loader.load("0")?;                 // base/Map/0.map
let grid = map.to_grid();                    // crates/world::MapGrid（属性 + 空对象表）
assert_eq!(map.attribute(0, 0), CellAttribute::HighWall);

// 2) 地图名 → map_id（来源 = Envir/MapInfo.txt 的 section 先后顺序，加载成功才入表）
let reg: MapRegistry = loader.load_registry()?;
let id = reg.require_id("0")?;               // 找不到即 Err(MapNameNotFound)，不静默用 0 号占位图
```

- `MapFile`：`width/height/cells/doors/door_status/points/first_wall_index`；
  `high_count()/low_count()/walk_count()` 直接对拍部署件读数。
- `door_status: Vec<DoorStatus>` 的下标即 C# `MapDoor.Status` 的**对象身份**：两条门 `status_id`
  相同 ⇔ 同一个 `DoorStatus` 实例。
- 纯字节入口 `parse_map_bytes(&[u8])`、`parse_points(text, w, h)` 便于合成件测试。

## `bins/game-svr` 集成点（**不在本卡**）

1. `WorldMaps` 占位尺寸（`src/config.rs`）⇒ 用 `MapRegistry` 里按名取到的真图尺寸；
2. 角色记录 `CurMap` ⇒ `let map_id = registry.require_id(&head.cur_map)?;` 再
   `world.enter_map(&head.chr_name, ent, map_id)`（现在恒传 0，是占位）；
3. 进世界前按 `registry.get(map_id).unwrap().to_grid()` 建/换图；找不到地图名要报错
   （日志 + 拒绝进图），**不允许**退回占位图。

## 证据（可复现）

```text
# 合成件 + 注册表（仓内，无外部依赖）
CARGO_TARGET_DIR=E:\tmp\openmir2-mapload-target cargo test -p mir2-envir --offline

# 真图判据（真机，读 E:\MirServer；#[ignore]，见 tests/real_maps.rs）
powershell -File crates\envir\scripts\gate-real-maps.ps1
# 部署件参照：
dotnet E:\tmp\openmir2-gamegate-imp\mapgen\bin\Release\net9.0\mapgen.dll probe E:\MirServer\M2GameSvr\Map\0.map
```

真图对拍结果（与部署件逐项相等）：

| 文件 | W×H | high | low | walk | doors | status 实例 |
| --- | --- | --- | --- | --- | --- | --- |
| `0.map` | 700×700 | 162711 | 109 | 327180 | 79 | 21 |
| `0129.map` | 7×10 | 59 | 0 | 11 | 0 | 0 |
| `0100.map` | 15×18 | 0 | 223 | 47 | 0 | 0 |

## 已登记的处置/偏差

- **`ObjList` null vs 空表**：C# 只在「首个墙格之前」的格分配 `ObjList`（真图全 NULL）；
  Rust `MapGrid` 每格恒空 `Vec`。因为 `IsAvailable`/`Count` 两者一致，且 `AddMapObject`
  对 null 会**懒分配**（`Envirnoment.cs:261-264`），可观测等价 ⇒ 不引入 `Option`；
  `MapFile::obj_list_allocated_at` 仍保留该源码事实作证据。
- **点位表路径**：源码 `GetEnvirFilePath("Point", $"{绝对 map 路径}.txt")` 在 .NET
  `Path.Combine` 下会丢弃前缀 ⇒ 实际是 `<map>.txt`（不是 `Envir/Point/`）。按源码复刻。
- **MapInfo `CaptureString` 分支**：section 内容以 `"` 开头的形态真实文件不存在；
  该路径 C# 会抛异常，这里按「跳过该行」处理（已登记）。
