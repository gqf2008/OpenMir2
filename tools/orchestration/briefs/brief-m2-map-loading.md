# 任务：openmir2-m2-map-loading —— Envir *.map 加载器（**格式已由实验定案**）+ 地图名→map_id

仓库：`E:\Users\gxh\Documents\GitHub\OpenMir2`。卡（唯一事实来源）：`openmir2-m2-map-loading`（**待认领**）。
身份：**openmir2-worker-8**（空闲；种子 `C:\Users\gxh\.walgit\keys\openmir2-worker-8.ed25519`）。

## 0. 先读后写

1. `git fetch walgit '+refs/collab/*:refs/collab/*'`；`walgit collab thread openmir2-m2-map-loading`。
   **本卡的 comment `862396f8` 就是规格**：owner 侧用合成 .map 在**部署件**上做的定案实验（九条 + 证据路径 + sha256）；
   comment `1e282568` 里「部署件与源码不一致」的结论**已被 `862396f8` 推翻**（那是探针假象，别照它实现）。
2. 读 `AGENTS.md`、`docs/Rust重写设计文档.md` §6（M2）、§12.4（写集/并行）、§14（协作）、§6.0（证据四件套与红检）。
3. 建独立 worktree：`git worktree add .claude\worktrees\mapload -b worktree-m2-map-loading refs/heads/master`，
   再 `walgit collab join --repo <worktree> --principal openmir2-worker-8 --key %USERPROFILE%\.walgit\keys\openmir2-worker-8.ed25519 --push walgit`，
   然后在卡上发 `status: in-progress`（parent = 该线程最后一条 entry 的 oid）再动手。**若已有人认领 ⇒ 停手回报。**

## 1. 规格（照做，别再自己猜）

以卡内 `862396f8` 的九条为准，摘要：

- 头 52 字节；`LoadMapData` 只读偏移 0..3 的 `int16 Width/Height`（小端），随后 seek 到 52。文件长度 ≤ 52 ⇒ 加载失败。
- 每格 **12 字节**：`wBkImg u16`、`wMidImg u16`、`wFrImg u16`、`btDoorIndex u8`、`btDoorOffset u8`、
  `btAniFrame u8`、`btAniTick u8`、`btArea u8`、`btLight u8`。
- 文件序↔坐标：`index = x * Height + y`（x 主序；`nW` 外层、`nH` 内层）。
- 属性：`wBkImg` bit15 ⇒ `HighWall`；`wFrImg` bit15 ⇒ `LowWall`；**`wMidImg` 忽略**；
  **先判 High 再判 Low ⇒ Low 覆盖 High**（两者都置位 ⇒ `LowWall`）。
- 门：`btDoorIndex` bit7 置位且 `(btDoorIndex and 0x7F) > 0` ⇒ 记一条门 `{x, y, n08 = btDoorIndex and 0x7F}`；
  `0x80`（point=0）不记。同图内 `|x1-x2| <= 10 && |y1-y2| <= 10 && n08 相同` 的门**共享同一个 DoorStatus 实例**
  （但 DoorList 里仍是各自独立的条目）。
- `ObjList`：部署件只在「首个墙格之前」的格分配对象表（源码 `isInitialize` 只置 false 不复位）；
  **真图 0.map / 0129.map 全部为 NULL**。⇒ Rust 侧别假设每格都有对象表，也别把「空表」与「无表」当两回事
  （`IsAvailable` 两种都是 false）——这条按可观测性在卡上写清你怎么处置。
- `Point/*.txt` 点位表：源码 `Envirnoment.cs:957-1027`（`;` 注释、`,`/`\t` 分隔、越界丢弃）。
- 地图名→map_id：按 M2Share 的取图路径口径实现，并把「哪张图对应哪个 id」的来源讲清（找不到就报错，别静默用占位图）。

## 2. 写集（§0b，**必须**守住）

- **允许**：新建 `crates/envir/**`（或你论证更合适的**新** crate）+ 在 `mir2-rs/Cargo.toml` **只追加你自己那一行** members
  （设计文档 §12.4：不改别人的行）+ 你自己的 `tests/`、`docs/`。
- **禁止**：`bins/game-svr/**`、`bins/game-gate/**`（m2-svr 车道正在写那里）、`src/**`（C# oracle）、
  `crates/world/**`（world-bevy 迁移线在动）、`MirClient`、表结构/脚本语法/数据文件格式。
- 集成到 `bins/game-svr`（替掉占位图 + 地图名→id）**不在本卡**：把库 API 与集成点写清、在卡上登记，
  由 owner 排到 m2-svr 车道之后。

## 3. 判据（自己重跑，贴原始输出）

- **合成件判据（入库、可复现）**：在测试里按上面的布局**生成** 3×3 字节（别依赖仓外文件），
  断言：`(0,0)` 只有 `wBkImg` bit15 ⇒ HighWall；只有 `wMidImg` ⇒ Walk；只有 `wFrImg` ⇒ LowWall；
  `wBkImg|wFrImg` ⇒ LowWall；全置位 ⇒ LowWall；`k=1` 单点 ⇒ 落在 `(0,1)`；门字节 9 种取值与 doormerge 三门的共享关系。
- **红检**：把 Low 的覆盖顺序写反（或把 `wMidImg` 当属性）⇒ 必须红。
- **真图判据（**不要**在常驻测试里读 `E:\MirServer`**——本仓有教训：测试读仓外目录会在 worktree 恒红）：
  做成一个 gate 脚本 / `#[ignore]` 用例，由审查者在真机跑：
  `0.map` ⇒ `700x700 high=162711 low=109 walk=327180`；`0129.map` ⇒ `7x10 high=59 low=0 walk=11`；
  再抽 1~2 张（如 `0100.map` 15x18）与部署件逐项相等。部署件读数可用
  `E:\tmp\openmir2-gamegate-imp\mapgen\`（`dotnet mapgen.dll probe <map>`）。
- 硬门禁（自己跑，RED 不许交）：`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets --offline -- -D warnings`、
  `cargo test --workspace --offline`、`cargo run -p msgcodegen -- --check`。
  **用隔离 target**（例如 `CARGO_TARGET_DIR=E:\tmp\openmir2-mapload-target`）。

## 4. 红线

- 不 merge、不签 review、不动别人的卡状态；不改 `src/**` 行为；不起服务栈（5000/7000/7100/7200 常被审查线占用）。
- **别在 `E:\tmp\openmir2-watch\rev2-*` 有 load 脚本在跑时做重编译**（会污染审查者的时序判据）。

## 5. 交付

`patch --base refs/heads/master --head refs/heads/worktree-m2-map-loading`（body 带 files/why/evidence/remaining）
→ `status: needs-review`；收工写 `E:\tmp\openmir2-watch\last-mapload.md`（改了哪些文件、判据原始输出、卡在哪、剩余物清单）。
