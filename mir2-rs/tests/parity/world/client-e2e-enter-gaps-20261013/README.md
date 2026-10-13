# `openmir2-client-e2e-enter-gaps` 证据包（m2-svr / openmir2-worker-11，2026-10-13）

卡上两条“真机缺口”在本部署上**都不能复现为 Rust↔C# 的差异**。本目录先用真客户端 + **C# 对照组**
（同一 `D:\MirClient-run\MirClinet.exe`、同一角色 `aaa`、同一 `mir2_db`）把两侧跑成逐像素同类，
并用服务端日志证明“帧发了 / 世界有实体 / AOI 真可见”。按 §6.0-3 与项目硬约束，不单方面行为偏离；

> **本目录覆盖四片（同一卡）**：
> ① 只读可观测性片（`7c8ac2fc3`，已合入 `f6869cb26`）：`log_enter_ability` + 周期世界/AOI 快照 + 1 条用例；
> ② 能力帧布局片（`a602e8a94`，已合入 `636bdce53`）：**默认关闭**的 58B 冻结客户端布局开关
> `MIR2_CLIENT_ABILITY_LAYOUT`（`record::encode_ability_client`），证据 `21`/`22`/`23`/`24`/`25`；
> ③（`2026-10-13` 追补）：DoD③ 的**独立复算** `26`/`27`（从冻结客户端源码逐字节复核 58B `TAbility`）；
> ④（`2026-10-13` 追补）：缺口②「同屏无怪」的 AOI 窗口对齐 —— 玩家 `ViewRange=12`（`PlayObject.cs:904`），
> 证据 `30`/`31`/`32`/`33`（改前窗口 11×11 → 改后 25×25）。

## 结论（先看这三条）

1. **① “进图未下发 52/752” = 否**。真机 Rust 日志 `进世界帧下发：socket=… 共 15 条（输入齐备的）`
   恰好是 658,51,50,41,213,11029,42,766,54,5008,1106,**52,752**,811,46；新加的
   `52/752 入图输入` 行给出输入值：`WAbil[level=1 … hp=19 mp=15 max_hp=19 max_mp=15 …]`。
   卡里引的 `main.rs:1088/1177 的 Ability::default()` 是 `handle_deal_cmd`/`wire_of` 的**对手缺席哑元**，
   不是进图会话态（锚点见 `01-oracle-anchors.txt` §C）。
2. **② “世界 tick 无实体/无怪” = 否**。`sim: tick … 处理 N 项` 的 N 是**世界命令队列**长度
   （`TickRecord::processed.len()`），不是实体数。真机读数：`实体内 9525 展示表 9394`，
   角色所在图 `same_map=1726 nearest=2..3`；把角色放到比奇省刷怪区 `(256,572)` 后，
   客户端**屏幕上确实出现怪**（`shots/rust-monsters-visible-256-584.png`，3 个红名怪）。
3. **“角色无血 / 跑不动” = 冻结客户端 × oracle 的既有问题，非 Rust 回退**。C# 对照组同屏
   HUD 逐像素一致（`0/0` + `655360/25165829`）⇒ 52 的 40B 尾与客户端 `TAbility`（HIGHHP=True，58B）
   记录长度不匹配（客户端把 `HP` 读成尾部 offset 22 的 `Exp` 段 = 0）——**C# 也一样**，
   因此 `CanRun`（`HP < RUN_MINHEALTH(10)`）两边都拒绝跑。要不要修需 owner 决策
   （客户端冻结 ⇒ 只能改服务端布局，属与 oracle 的**故意偏离**）。锚点见 `01-oracle-anchors.txt` §A/§B。
4. **104（`CM_SELECTSERVER`）= `SS_KEEPALIVE`（IDS 链路在线数心跳）**，忽略对客户端选服/心跳无可见影响。

## 命令与原始件

```powershell
# 1) 起真栈（Rust game-svr/三网关 + C# DBSrv/LoginSrv + MariaDB；配置用现网 E:\MirServer）
powershell -ExecutionPolicy Bypass -File tools/m2-acceptance/start_rust_stack.ps1 `
  -RustBinDir E:/tmp/m2svr-target/debug
# 2) 同一真客户端跑同一角色（D:\MirClient-run）
powershell -ExecutionPolicy Bypass -File tools/client/mir_flow.ps1 `
  -Account mir2test -Password mir2pass -OutDir E:/tmp/m2svr-gaps/run5 -WalkClicks 0 -AutoPlaySec 70
# 3) C# 对照组：start-all.ps1 起 C# 栈，同命令、同角色再跑一遍（见 12-csharp-control-flow.txt）
# 4) 收工停栈；DB 位置已还原 (291,666)；MySQL 3306 保持运行（开工前只剩它）
```

- `10-rust-real-run-52-752-and-tick.txt`：Rust 真机 `game-svr.out.log` 摘录（52/752 输入 + 周期观测）。
- `12-csharp-control-flow.txt` / `13-rust-flow-flow.txt`：两侧 `mir_flow.ps1` 的阶段回执（均 `FLOW_OK`）。
- `shots/rust-monsters-visible-256-584.png`：Rust 侧角色位于比奇省 (256,584)，屏上 3 只怪。
- `shots/rust-city-spawn-288-654.png` / `shots/csharp-control-city-289-669.png`：两侧同城出生点，
  HP/MP HUD 逐像素同类（`0/0` + `655360/25165829`；`MaxMP` 的精确推导见 `26` §5）。
- `21-client-ability-layout.md`：58B `TAbility` 的偏移表 + 默认关布局开关；
  `22-before-after-realrun.txt`（改前 40B/`0/0` → 改后 58B/`19/19`）、`23-redcheck-client-layout.txt`。
- `26-frozen-client-layout-verify.txt`：**DoD③ 独立复算**（从冻结客户端源码解析 `{$IF HIGHHP}`/`TAbility`
  逐字段算偏移；pin commit `c61d912d`）+ `verify_client_ability_layout.py`（可复跑）+
  `27-redcheck-layout-verify.txt`（两条改坏必红）。
- `04-gates.txt` / `05-boundary.txt` / `20-redcheck-world-aoi-diag.txt` / `24-gates.txt` / `25-boundary.txt`。

## 本目录代码改动（只读可观测性 + 默认关的布局开关）

- `mir2-rs/bins/game-svr/src/main.rs`
  - `log_enter_ability`（进图时打印实际写入 52/752 的输入值、**实际布局**与**实际尾长**；只读）。
  - `WorldDiag`/`SessionAoiDiag`/`world_aoi_diag`：周期 `sim:` 行追加
    `实体内 N 展示表 M 在世界会话 K` 与每会话 `[name(id) map=… vis=… same_map=… nearest=…]`，
    从根本上消除“`处理 0 项` ⇒ 无实体”的误读。
  - `mir2_client_ability_layout()`：读 `MIR2_CLIENT_ABILITY_LAYOUT`（`1/true/TRUE/True` ⇒ 开；**未设 ⇒ 关**）。
  - [`new_player_entity`]（片④）：生产宿主唯一的玩家构造点，对齐 `PlayObject` 构造器 —— `WantRefMsg=true`
    （`PlayObject.cs:889`，片①）+ **`ViewRange=12`**（`PlayObject.cs:904`）。后者决定 AOI 扫描窗口
    （边长 `2*ViewRange+1`，`BaseObject.ViewRange.cs:71-74`）：缺它只有缺省 5（`entity.rs:402` = `BaseObject.cs:383`）
    的 11×11 ⇒ 距离 6..12 的怪不进 `visible_actors`、`drain_aoi_downlink` 不发 ⇒ 客户端同屏少怪。
    用例 `tests::player_aoi_window_covers_view_range_12` + `tests::new_player_entity_sets_want_ref_msg` 的
    `ViewRange==12` 断言；改坏必红见 `31`。
- `mir2-rs/bins/game-svr/src/record.rs`：`encode_ability_client`（冻结客户端 58B `TAbility` 布局；
  oracle `encode_ability` 40B 原样保留）。`enter.rs::AbilityInputs.client_ability_layout` 决定 52 尾二选一。
- 用例 `world_aoi_diag_reports_visible_and_nearest`（构造级；改坏必红见 `20-…`）、
  `record::tests::encode_ability_client_matches_highhp_record` + `enter::tests::ability_frames_follow_recalc_abilitys`
  （改坏必红见 `23-…`）。
- **默认关**：未设 `MIR2_CLIENT_ABILITY_LAYOUT` 时，52 尾仍 = oracle `EncodeMessage(WAbil)` 40B
  （金标准 #12 与 `enter` 默认分支不回归）。
- **不改**任何模拟/出帧/取数语义；`src/ sql/ MirClient` 与 `crates/protocol`/`crates/storage` 边界为空。

## ④ 玩家 `ViewRange=12` 对齐（缺口②「同屏无怪」）——`30`/`31`/`32`/`33`

oracle `PlayObject` 构造器置 `ViewRange = 12`（`PlayObject.cs:904`），而 Rust 生产宿主 `new_player_entity`
只对齐了 `WantRefMsg`、**从未置 `ViewRange`** ⇒ 玩家 AOI 窗口停在 `BaseObject` 缺省 5（`BaseObject.cs:383`）
的 **11×11**，不是 oracle 的 **25×25**。`drain_aoi_downlink` 只对 `visible_actors` 里 `Show` 的对象发
`SM_TURN(10)`+`SM_FEATURECHANGED(41)` ⇒ 距离 6..12 的怪**从不出现**（出生点/城墙外尤其明显）。

本片只改**一行**（+ 两条用例/断言），不动模拟/取数/RNG 语义：

- `30-oracle-anchors-viewrange.txt`：oracle 锚点（`PlayObject.cs:904` / `BaseObject.cs:383` /
  `BaseObject.ViewRange.cs:57-74` / `:112`）与 Rust 现状（`entity.rs:402` / `main.rs:3106` / 调用点 `:5311`）。
- `31-redcheck-viewrange.txt`：**改坏必红** —— 删 `e.view_range = 12;` ⇒ 两条用例各 rc=101
  （`距离 12 的怪必须在玩家 25×25 窗口内…：[1]`、`left: 5 / right: 12`）；还原 rc=0。
- `32-gates.txt`：合并树四门禁 `fmt 0` / `clippy 0` / `98 suites · 1097 passed · 0 failed · 21 ignored` /
  `msgcodegen 604`。
- `33-boundary.txt`：边界 `src sql MirClient protocol storage` 空。

> 未做（如实登记）：真机两侧同序列对拍（窗口归 `window` 车道/owner）；怪物侧 `ViewRange` 的各子类覆盖
> 仍是**未登记差异**（`30` §D），交 owner 裁决归属，本片不擅自扩写集。

## 验证（隔离 `CARGO_TARGET_DIR=E:/tmp/m2svr-target`，cwd=`mir2-rs/`）

- 四门禁（当前 head `a602e8a94` 合并树）：`fmt rc=0` / `clippy --workspace --all-targets -D warnings rc=0` /
  `test --workspace --offline` **98 suites · 1040 passed · 0 failed · 21 ignored** / `msgcodegen 604`
  （早前只读片为 97/988，见 `04-gates.txt`；布局片重跑见 `24-gates.txt`）。
- 真机：两侧 `mir_flow` 均 `FLOW_OK`；端口实拍见栈脚本输出（停在 `RUST_STACK_OK`），收工 `-Stop` 后
  5000/7000/7100/7200 全 DOWN，仅 3306 保留。

## 残留物清单

- `E:\tmp\m2svr-gaps\`（两侧截图/日志/门禁原始件，部分已入库到本目录）；`E:\tmp\m2svr-target`（本片构建目标）。
- `E:\tmp\m2-acceptance\stack\`（最近一次 Rust 栈日志/pid 文件；进程已 `-Stop`）。
- MySQL 3306 常驻（开工前即有）；`mir2_db.characters.aaa` 的位置已还原为 `(291,666)`。
