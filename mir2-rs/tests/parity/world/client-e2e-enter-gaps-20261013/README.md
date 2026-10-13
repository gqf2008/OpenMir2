# `openmir2-client-e2e-enter-gaps` 证据包（m2-svr / openmir2-worker-11，2026-10-13）

卡上两条“真机缺口”在本部署上**都不能复现为 Rust↔C# 的差异**。本目录先用真客户端 + **C# 对照组**
（同一 `D:\MirClient-run\MirClinet.exe`、同一角色 `aaa`、同一 `mir2_db`）把两侧跑成逐像素同类，
并用服务端日志证明“帧发了 / 世界有实体 / AOI 真可见”。按 §6.0-3 与项目硬约束，不单方面行为偏离；

> **本目录覆盖五片（同一卡）**：
> ① 只读可观测性片（`7c8ac2fc3`，已合入 `f6869cb26`）：`log_enter_ability` + 周期世界/AOI 快照 + 1 条用例；
> ② 能力帧布局片（`a602e8a94`，已合入 `636bdce53`）：**默认关闭**的 58B 冻结客户端布局开关
> `MIR2_CLIENT_ABILITY_LAYOUT`（`record::encode_ability_client`），证据 `21`/`22`/`23`/`24`/`25`；
> ③（`2026-10-13` 追补）：DoD③ 的**独立复算** `26`/`27`（从冻结客户端源码逐字节复核 58B `TAbility`）；
> ④（`2026-10-13` 追补）：缺口②「同屏无怪」的 AOI 窗口对齐 —— 玩家 `ViewRange=12`（`PlayObject.cs:904`），
> 证据 `30`/`31`/`32`/`33`（改前窗口 11×11 → 改后 25×25）。
> ⑤（`2026-10-13` 追补，回应 owner `acceptance_add`）：**客户端侧帧可见性**真机抓包 ——
> `mir2_proxy.py` 在 7200 透明代理 + 影子 game-gate(7600)，客户端收到的字节 ⇒ `export_s2c.ps1` 解码，
> 或 `40`/`41`（两次 run）+ `capture-client-*.jsonl` + 判据 `check_client_s2c_frames.py` + 红检 `42`
> + 截图 `shots/rust-client-wire-*.png`。
>
> ⚠ `40`/`41` 与 `capture-client-*.jsonl`、`shots/rust-client-wire-*.png` 是**真机采集件**
> （2026-10-13 19:5x，栈 = merge c59a5e754 的 Rust 二进制），不是回填的固定件。

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
5. **客户端侧帧可见性（片⑤，2026-10-13 追补）**：`40`/`41` 是 `mir2_proxy` 在 7200 抓到的
   **客户端实际收到的 s2c 字节** —— 入图序列 `658…46` 里 **52/752 都在**（Run A `52 body_len=40` = oracle 布局；
   Run B 开 `MIR2_CLIENT_ABILITY_LAYOUT=1` 后 `52 body_len=58` = 冻结客户端布局，客户端 HUD `19/19` + 血球满红）；
   Run A 还在客户端侧抓到 **12 对 `10 SM_TURN`+`41 SM_FEATURECHANGED`**（12 个怪 actor 的同屏出现帧）。
   判据 = `check_client_s2c_frames.py`（`--selftest` 自证门是活的；`42` 红检：删掉 52 ⇒ RED）。
   ⇒ 缺口①「未下发」与缺口②「同屏无怪」在**客户端侧**被直接否证。

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

## ⑤ 客户端侧帧可见性（缺口①/② 的直接证据）——`40`/`41`/`42` + `check_client_s2c_frames.py`

owner `acceptance_add`（2026-10-13）：①「进图是否真的下发 52/752」与②「同屏是否有怪」此前只有
**服务端日志**，要求补**客户端收发帧**。本片采法：

```
客户端 -> (proxy 7200) -> [影子 game-gate 7600] -> game-svr 5000
           mir2_proxy.py --map 7200=127.0.0.1:7600
```

代理 s2c 方向的字节 = **客户端收到的字节**；`export_s2c.ps1`（`segment_frames` + `GoldenExport` +
`replay` 自洽 GREEN）解码为 `capture.jsonl`。

| run | 开关 | 入图自帧序列（客户端侧） | 52 body_len | 10+41 同屏对 | 客户端 HUD |
| --- | --- | --- | --- | --- | --- |
| A | 默认（oracle 40B） | `658,51,50,41,213,11029,42,766,54,5008,1106,52,752,811,46` | **40** | **12** | `0/0`（黑血球，58B offset 误读） |
| B | `MIR2_CLIENT_ABILITY_LAYOUT=1`（58B） | 同上 | **58** | 1 | **`19/19` + `15/15`（血球满红）** |

- `40-client-s2c-default-oracle40.txt` / `41-client-s2c-switch58.txt`：两次 run 的原始帧（含 52/752
  `raw_hex`、`body_segments`、`body_sha256` 与 12 个怪 actor 的 recog）+ proxy dump / decode 的 sha256。
- `capture-client-default-oracle40.jsonl` / `capture-client-switch58.jsonl`：解码后的客户端 s2c 帧（同 `capture.jsonl` schema）。
- `shots/rust-client-wire-oracle40-292-600.png`：Run A 客户端 HUD `0/0`；
  `shots/rust-client-wire-client58-299-640.png`：Run B 客户端 HUD `19/19` 且屏幕上可见怪。
- `check_client_s2c_frames.py`：可复跑判据（`--capture <jsonl> --expect-52-body-len N --min-appearance-pairs M`；
  缺文件 ⇒ `PREREQ_MISSING` rc=2；`--selftest` 不依赖外部帧）。
- `42-client-s2c-redcheck.txt`：**改坏必红** —— 从 capture 删掉 52 ⇒ `RC=1/RED`（同时报「序列前缀不符」与「没有 52」）；
  还原 ⇒ `RC=0/GREEN`；另有 `--selftest` 5 条口径全 OK。

> 这两份 capture 是**真机原始件**（不是合成）：采集命令、栈、账号/角色与 sha256 都写在 `40`/`41` 头部；
> 采完已停栈（见报告残留物清单）。片⑤**零生产代码改动**（纯证据 + 判据脚本）。

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

## ⑥ 片⑥：跑键 CM_RUN(3013) 真机判读（DoD②「能跑」/ reviewer not_verified③）

- **采集**：`mir2_proxy.py` 7200 透明代理 → 影子 `game-gate`(7600) → game-svr(5000)；
  `export_s2c.ps1`(segment_frames + GoldenExport) 解码；客户端 `D:\MirClient-run`，账号 `mir2test`/角色 `aaa`。
  共 **7 份真机 capture**（`50-runkey-five-runs-summary.txt`）：片⑤ RunA/RunB + 本片 5 次（单击 / 按住拖动×2 /
  8 方向远点按住 / 双圈(110/210px)按住 / 渐变扫动按住×16）。
- **读数**：7 份 capture 里 `CM_RUN(3013)` **恒为 0**；R2..R7 的 `52` 尾长 = **58**（客户端 HP=19 ≥
  `Actor.pas RUN_MINHEALTH=10`）⇒ 仍只发 `CM_WALK(3011)`/`CM_TURN(3010)` ⇒ **「跑不动」不是 52 布局造成的**
  （对比 R1 尾长 40、HUD `0/0`）。
- **成因（文件锚见 `51-client-run-gate-anchors.txt`）**：唯一发 `CM_RUN` 的支 `ClMain.pas:2193`
  门 = `g_boCanStartRun or (g_nRunReadyCount >= 1)`；而 `g_boCanStartRun ← ClientConf.boCanStartRun`
  （`:15034`），该字段在客户端 `TClientConf`（`Grobal2.pas:2978-3013`，**30 字段**、含 5 个 Integer）
  的偏移 **> 17**，oracle 的 11029 只给 **17 字节**（`ClientConf.cs` 17 bool；本仓 `smframe.rs:341-344` 照搬）
  ⇒ `DecodeBuffer`（`EDcode.pas:894-903`）拷贝未初始化栈 ⇒ 实测恒假；且 `FormMouseDown`（`:4740`）每次都清
  `g_nRunReadyCount := 0`。⇒ **客户端构件与 oracle 的既有不兼容**，Rust 侧多发几字节是错的（客户端冻结 +
  行为等价）⇒ 归 owner 按 §6.0-3 裁决。
- **判据脚本**：`check_client_run_frames.py`（`--capture` ① c2s 有 3011 ② 52 尾长==58 ③ CM_RUN 条数==期望；
  `--selftest` 5/5；缺件 `PREREQ rc=2`）。改坏必红与绿/红原始件见 `53-redcheck-runframes.txt`
  （`--expect-run-count 1` ⇒ RED）。头条 capture：`capture-client-run5.jsonl`。
- **缺口② before/after（`52-before-after-viewrange12.txt`）**：**after** = RunA 真机 capture 逐只怪
  Chebyshev 距离，**12 只里 10 只落在 6..12**（窗口 5 只会是 0 只）⇒ 「同屏出现 6..12 的怪」达成；
  **before** 的真机尝试（回缺省 5 重建起栈）**没抓到客户端连接**（代理 dump 空）⇒ 如实登记为 remaining，
  before 侧以窗口模型（`PlayObject.cs:904` / `BaseObject.cs:383` / `ViewRange.cs:57-74`）与已审构造级用例为准。
- **门禁**（merge tree `d8c02fd8`）：`fmt 0` / `clippy 0` / `99 suites · 1159 passed · 0 failed · 22 ignored` /
  `msgcodegen 604`（`54-gates.txt`）。**本片零生产代码改动**（仅 `tools/client/mir_flow.ps1` 驱动段 + 本目录证据/判据）；
  `55-boundary.txt`：冻结面 `src/ sql/ MirClient` + `crates/protocol` + `crates/storage` = 空。

## ⑦ 片⑦：缺口② before/after **配对真机** run（收口 §⑥ 的 before remaining）——`56`/`57`

- 配对口径（唯一变量 = `main.rs::new_player_entity` 的 `e.view_range = 12;`；其余全同）：
  `start_rust_stack.ps1 -Seed 42` → DB `mir2_db.characters.aaa` 置 (287,620) → 影子 `game-gate`(7600)
  + `mir2_proxy.py` 7200→7600 → `mir_flow.ps1 -WalkClicks 0 -AutoPlaySec 12 -SkipReenter -SkipLogout -SkipTalk`
  （两次均 `FLOW_OK`，代理均实抓到客户端连接）→ `export_s2c.ps1`。
- 读数（客户端实收 s2c 解码）：
  | 运行 | view_range | `10`/`41` 配对 | Chebyshev 6..12 | 最大 |
  |---|---|---|---|---|
  | BEFORE | 5  | 2  | **0** | 4 |
  | AFTER  | 12 | 13 | **10** | 13 |
- 服务端佐证（before `game-svr.out.log` 的 `world_aoi_diag` 采样）：`nearest` = 1/6/12/23/17
  ⇒ 6/12/17/23 的怪**确在附近**，但客户端一只都没出现（窗口 11×11 半径 5 之外）。
- 原始件：`capture-client-viewrange5-seed42.jsonl`（before）、`capture-client-viewrange12-seed42.jsonl`（after）；
  命令/坐标/hash 见 `56-before-after-viewrange-realrun.txt`。牙齿见 `57-redcheck-viewrange-realrun.txt`
  （本片配对本身即正反两面；构造级 `31` 复跑）。
- **零生产代码改动**（BEFORE 二进制是临时本地注释该行、交付前已 `git checkout` 还原并重建）；
  起过真栈做抓包，收工已 `-Stop`，十游戏端口全 DOWN、仅 3306，DB 位置还原 (291,666)。

## ⑧ 片⑧：回应 orchestrator comment 6c7ab2eb（「跑」修复）——**帧号不匹配的只读复核**（`60`）

- **发现（阻断级）**：冻结客户端 `SM_SERVERCONFIG = 20002`（`Grobal2.pas:1300`），而 oracle/Rust 发 **11029**
  （`Messages.cs:433` / `protocol/messages.rs:393`）；客户端全仓无 `11029` ⇒ `ClientGetServerConfig`
  **永不执行** ⇒ `g_boCanStartRun` 停在**缺省 True**（`MShare.pas:1998`）⇒ 跑门 `ClMain.pas:2193` 恒真。
  ⇒ **「ClientConf 长度兼容」不足以（也不能）解释 `CM_RUN=0`**；片⑥「boCanStartRun 读未初始化栈 ⇒ 恒假」不成立。
- **真正的下游门**（应查这几处）：`ClMain.pas:2190 caRun:` —— `CanRun`（`Actor.pas:2162`，HP≥10）、
  `GetDistance>=2`（`:2223`）、`IsUnLockAction`（`:2225`）、`g_PlayScene.CanRun`（`PlayScn.pas:3101`，
  两格路径都被怪占即 False ⇒ 落 `:2241 caWalk` = 只走不跑）；`g_ChrAction:=caRun` 只在点 >1 格时置
  （`ClMain.pas:5446-5463`）。
- 结论与最小方案（含「若按 orchestrator 方向修，必须 id 20002 + 52B TClientConf 体同时改，并登记
  对金标准的故意偏离」）见 `60-client-run-blocker-finding.txt`。本片**未改生产代码、未起栈**。

## ⑨ 片⑨：「跑不动」真根因 = 帧 52 能力布局（HP）＋ 生产默认翻为客户端 58B —— `61`

- **结论**：`CM_RUN(3013)` 的开关是**帧 52 的能力布局**，不是 `SM_SERVERCONFIG(11029)` / `TClientConf`。
  配对真机（同树/同角色 `(256,584)` 开阔地/同探针，唯一变量 = 52 尾布局）：

  | 52 布局 | 11029 body | 52 body | HUD | c2s CM_WALK | c2s **CM_RUN** |
  |---|---|---|---|---|---|
  | oracle-40 | 23（同 sha） | 40 | `0/0` | 32 | **0** |
  | client-58 | 23（**逐字节相同**） | 58 | `19/19` | 25 | **3** |

  11029 帧两行逐字节相同 ⇒ ClientConf 一字未动而 `CM_RUN` 0→3 ⇒ 片⑥/§⑦ 所说的「跑门 = `boCanStartRun`
  读 11029 的栈垃圾」**已订正**：`boCanStartRun` 停在缺省 `True`（`MShare.pas:1998`）；真正的门是
  `CanRun`（`Actor.pas:2162`，`HP >= 10`），而 `HP` 来自 58B `TAbility`。
- **第二必要件**：跑键探针必须在**目标点**按下左键（`FormMouseMove` 不喂 `_DXDrawMouseDown`；
  `g_ChrAction:=caRun` 只在 `ClMain.pas:5446-5463` 的「点 >1 格」支置）。旧 `Send-HoldRun` 从屏幕中心
  按下 ⇒ 目标永远设不上 ⇒ 片⑥ R5..R7 的 0 条 3013 有一部分是探针 bug。
- **生产改动**：`main.rs::mir2_client_ability_layout()` **默认翻为 client-58**（`MIR2_CLIENT_ABILITY_LAYOUT=0`
  ⇒ oracle 40B 对拍）；`tools/client/mir_flow.ps1::Send-HoldRun` 目标点按下。证据 `61-client-run-ability-layout-fix.txt`
  + `shots/rust-client-run-{before-oracle40-hud-0-0,after-client58-hud-19-19}.png`；判据脚本 `check_client_run_frames.py`
  的 ③ 改为 `CM_RUN >= --min-run-count`（默认 1）。
- **未做**：brief 指定的 ClientConf 体编码（客户端不读 11029，做了也不可能满足「改回 17B ⇒ CM_RUN 回归 0」）；
  58B 白名单 / owner_actions ④ 待 owner 签字。

## ⑩ 片⑩：「跑」配对原始件**入库** + 配对判据可离线复跑（收口 reviewer not_verified③）—— `62`

- **为什么**：片⑨的真机配对（52 尾 40↔58 ⇒ `CM_RUN` 0→3、HUD 0/0→19/19）**原始 capture 只在
  `E:/tmp/m2svr-run4/**`（未入库）**，reviewer-5/6 均记「中央读数无法在检出内独立复算」。本片把两份原始件**逐字节入库**
  （sha256 与 `61-*` §F 完全相同）：
  `capture-run-ability-before-oracle40.jsonl`（`8d140852…`）/ `capture-run-ability-after-client58.jsonl`（`753a7566…`）。
- **判据**：`check_client_run_frames.py` 增 `--pair-before/--pair-after` 配对模式（**不需要起栈**）：
  A) 两侧 `11029` `raw_hex` 逐字节相同 ⇒ ClientConf 未动（把片⑧ 的口径变成机器可判）；
  B) BEFORE `52==40` 且 `CM_RUN==0`；C) AFTER `52==58` 且 `CM_RUN>=--min-run-count`（默认 1）。
  实跑 GREEN；对调 → RED；单件 BEFORE 默认 → RED（= root 判据「改回 ⇒ CM_RUN 归 0」）；`--selftest` 单件 6/6 + 配对 7/7。
  原文见 `62-run-ability-pair-inrepo.txt`。
- **零生产代码改动**；边界/门禁见 `63-gates.txt` / `64-boundary.txt`；本片未起栈、未占端口。
- **仍归 owner/窗口**（不因本片收卡）：58B 默认值的 §6.0-3 签字；`SM_SERVERCONFIG id 20002 + 52B TClientConf`
  属另案故意偏离且已证与 `CM_RUN` 无关；默认翻 58B 后跨侧 s2c 逐字节对拍需 Rust 显式 `MIR2_CLIENT_ABILITY_LAYOUT=0`。

## ⑪ 片⑪：缺口②「同屏没怪」root 判读误伤的根因 = **诊断 `vis` 语义错**（2026-10-14）—— `70`

- **root 2026-10-14 00:4x 实测**（`E:/tmp/client-e2e/verify1/` + `stack/game-svr.out.log`）：①血 ✓（HUD 19/19、15/15）、
  ②跑 ✓（`Ident=3013` 17 次）、③怪未闭环（`实体内 27030`、`same_map=1726`、`nearest=23..34`，但 `vis=0` 11/11）。
- **判定**：③「该点确实无怪」**成立**（真机位置 `map 0 (291,666)` 在 `MonGen.txt` 的 map 0 **全部生成框之外**，
  最近框 Chebyshev = 30 > `ViewRange` 12；见 `71-mongen-dense-points-map0.txt`）；④「AOI 未算/未下发」**不成立**。
- **根因（诊断口径，不是 AOI）**：`visible_actors` 的 `VisibleFlag` 有两种「看见」——
  `Show`=**本轮新增**、`Invisible`=**已存在且本轮又看见**（`BaseObject.ViewRange.cs:25/36`，Rust `aoi.rs:39-48`）。
  旧 `world_aoi_diag` 只数 `Show` ⇒ 怪停留 >1 tick 后 `vis` 恒 0。真机反例：`game-svr-AFTER.log`（vr=12、
  `nearest=2..9`）`vis=0`，而客户端实收 **13 对** `10 SM_TURN + 41 SM_FEATURECHANGED`
  （`check_client_s2c_frames.py --capture capture-client-viewrange12-seed42.jsonl` = GREEN, appearance_pairs=13）。
- **本片改动**（`bins/game-svr/src/main.rs`，只读诊断）：`vis` 改为「当前可见集」（`Show|Invisible`，**不含自身**）；
  新增 `vr=`（view_range）、`new=`（本轮新增）、`af=`（**累计实发**出现帧数，`drain_aoi_downlink` 返回值累加）；
  周期行变成 `[aaa(22017) map=26 vr=12 vis=13 new=0 af=26 same_map=1726 nearest=2]`。
  新用例 `world_aoi_diag_vis_is_current_view_not_new_appearances`（红检：改回只数 `Show` ⇒ `vis:0` 必红）。
- **判据脚本**：`check_aoi_visibility_diag.py`（`--expect none|visible --min-vis N`，老版式 fail-closed ⇒ `PREREQ_RED`），
  自检 9/9。原文见 `70-aoi-visibility-verdict.txt`；红检 `72-redcheck-vis-metric.txt`；老日志 fail-closed `73-prereq-old-log.txt`。
- **仍归 owner/窗口（R 级，不因本片收卡）**：两侧**同位置同 `viewrange`** 对照（Rust 侧 `--expect visible`
  + C# 侧同一 `check_client_s2c_frames.py` 配对数；反向「无怪」用 `--expect none`）。recipe 见 `70-…` §E。

## ⑫ 片⑫：③「两侧对照」判据工具 + 两条随卡 Minor 收口（2026-10-14）—— `77`/`78`

- **背景**：root 00:4x ③ 要求「用两侧对照（同一位置、同一 viewrange）给证据并收口」，并注明
  「判据/capture 工具你准备」。片⑪ 已给服务端读数判据（`check_aoi_visibility_diag.py`）与客户端帧判据
  （`check_client_s2c_frames.py`），但**两侧对照**缺一个把两侧 capture 放一起机判的工具。
- **新工具 `aoi_visibility_pair.py`**：消费两侧 `capture.jsonl`，抽 `SM_TURN(10)+SM_FEATURECHANGED(41)`
  相邻出现对（**直接 import 复用** `check_client_s2c_frames.py` 的抽取函数，口径不漂移），机判：
  `--expect none` ⇒ 两侧都为 0 = ③「该点确实无怪」的两侧证据；`--expect visible --min-pairs N` ⇒
  两侧都 ≥N = ④「AOI 有算有下发」的反证；`--strict-recogs`（默认关）把 `70 §E step3` 的
  「两侧 ActorId 相同」假设升级为强判据。退出码 0/1/2，`--selftest` 11 条不依赖外部件。
  真机既有 capture 上的绿/红演示见 `77-two-side-comparator.txt` §C（**真两侧对照**需窗口采 C# 侧后跑，
  命令写在 `77` 末尾）。
- **Minor M-1**（reviewer-5）：`check_aoi_visibility_diag.py` 的 INFO 行恒说「vis==0 与窗口自洽」，
  即使 `vis=13`。改为按 `vis` 实际值与窗口内外关系输出（四个分支：`vis==0/nearest>vr`、
  `vis==0/nearest<=vr`、`vis>0/nearest<=vr`、`vis>0/nearest>vr`），判绿红逻辑未动；自检仍 9/9。
- **Minor M-2**（reviewer-5）：本目录 `SHA256SUMS.txt` 8 行用「两空格 + *」⇒ GNU `sha256sum -c`
  报 No such file、权威复核器静默跳过。本片触碰该目录时用 `verify_sha256sums.py write` 重生成清单，
  分隔符统一为单空格（8 个 hash 复算未变）。
- **零生产代码改动**（仅 `.py` 判据工具 + 证据）；本片**未起栈、未占端口**；两侧同框真机读数仍归 owner 窗口。
- 证据：`77-two-side-comparator.txt`（工具自证 + 真 capture 演示）、`78-m1-m2-minor-fix.txt`
  （M-1 措辞 before/after + M-2 归一）、`79-gates.txt`、`80-boundary.txt`。

## ⑬ 片⑬：③「该点确实无怪」判据**工具化** + 窗口运行手册（2026-10-14）—— `81`/`82`

- **背景**：片⑪/⑫ 已给服务端 `vis/af` 判据、客户端帧判据、两侧对照器，但「该点有没有生成框」的读数
  只留在 `71-mongen-dense-points-map0.txt` 那份**一次性转录**里（不可复跑、无牙齿），而 root 的 ③ 收口
  与窗口选点都依赖它；C# 侧的 capture 拓扑也只在 `tools/README.md` / `C线-交付说明.md` 里有历史告警。
- **新工具 `check_mongen_dense_points.py`**：解析 `Envir/MonGen.txt`（GBK，逻辑地图名），按 oracle
  `WorldServer.MonGen.cs:825-826` 的**正方形**生成框语义（Chebyshev）判 `inside/outside`，打印到最近框的
  距离与 `view_range` 关系；`--selftest` 10/10、**两条改坏必红**（Chebyshev→Manhattan、丢掉 `Range`）、
  缺件 `PREREQ_RED` rc=2。原文见 `81-mongen-dense-points-checker.txt`。
- **③ 离线收口（复算）**：root verify1 `(291,666)` / 复进 `(291,651)` 在 map "0" **76 个生成框之外**
  （最近框 Chebyshev=30 / 29 > `ViewRange=12`）⇒ ③「该点确实无怪」成立；`(287,620)`/`(256,572)`/`(188,439)`
  框内 ⇒ ④ 不成立（客户端实收 13 对出现帧）。**新增**：`StartPoint.txt` 的 oracle 出生点
  `(289,618)`/`(330,266)` 框内、`(650,631)` 距框 3 格 ⇒ **新角色默认体验本来就该同屏见怪**，
  窗口里应把 `aaa` 置 `(289,618)`（或 `(287,620)`），别再拿夹具残留点 `(291,666)` 复测。
- **窗口运行手册 `82-two-side-window-runbook.md`**：③a/③b 逐条判据、坐标选择（含 `check_mongen_dense_points.py`
  现场复核）、两侧起栈/影子 game-gate+代理/客户端/解码/比较器的**照抄步骤**、收工与还原，
  并**显式登记唯一未证环节**：C# 侧抓包拓扑（`tools/README.md` 的「进世界数据拿不到」历史 +
  若复现时按 `C线-交付说明.md`「解除判据」#1 走 owner 裁决），以及**降级判据**（截图级两侧对照，须在卡上写明）。
- **零生产代码改动**（仅新增一个 `.py` 判据 + 两份文档）；本片**未起栈、未占端口**；真机两侧同框读数仍归 owner 窗口。
- 证据：`81-mongen-dense-points-checker.txt`（工具 selftest 10/10 + 真 MonGen 读数 + 两条改坏必红）、
  `82-two-side-window-runbook.md`（两侧窗口照抄步骤 + C# 抓包拓扑风险与降级）、`83-gates.txt`、`84-boundary.txt`。

## ⑭ 片⑭：③「C# 侧抓包拓扑」旧归因**订正** + 离线前置门（2026-10-14）—— `85`/`86`

- **背景**：片⑬ 的手册 §4 把「C# 世界数据下不来」登记为**唯一未证环节**，沿用的是 C 线（2026-10-10）
  「影子网关端口被挪 ⇒ GameSvr 按端口对齐」的归因。本片按**当前源码**逐路径复核，推翻该归因。
- **订正（`85-csharp-capture-topology-rootcause.md`）**：`!servertable.txt` 只在**多机转移**路径被消费
  （`GameShare.GetMultiServerAddrPort` → `AppService.cs:221` / `WorldServer.cs:1063-1074` `SM_RECONNECT`），
  单机进世界不读；GameSvr 对网关按**槽位**寻址（`TCPNetChannel._gateSlotByConnection`/`_gameGates`），
  世界侧 `GateIdx` = 网关认证串里自报的 `ServiceId`（`ClientSession.cs:1203`），**与端口无关**。
  同症状的确定根因是 **S1 缺陷①**（槽位按 accept 序 + `CloseGate` 不释放槽位，
  `evidence/S1/S1-报告.md` §1），修复 `607e9b773`（2026-10-10 17:01）**已进部署件**
  （`!deployed-from.json` `git_head=628d8024`，2026-10-10 23:54，`merge-base --is-ancestor` = YES）。
- **新工具 `check_csharp_capture_preflight.py`**（只读、零端口）：起 C# 栈前机判五条前置 —— 部署件含
  `607e9b773`；真实与影子 `ServerWorkThread=1`（S1 §2.1 多网关错配边界；本部署本就为 1）；
  真实 `GatePort1=7200`；`!servertable.txt` GameGate 端口 `=7200`；上游 `127.0.0.1:5000`；
  另可 `--shadow-config` 检查影子配置。`--selftest` 12/12；**五条改坏必红**（GatePort1→7201、
  `ServerWorkThread→3`、servertable 端口→7201、fix 提交换非祖先、影子多网关）后还原复绿。
- **手册 `82` §4 重写**：加入 `§4.0 前置门`（命令 + 退出码）与 `§4.1 影子 RunGate 配置`
  （只改 `GatePort1=7600`；不要引入第 2/3 个活动网关）；保留降级判据与「不要现造 oracle 配置」。
- **仍未证（不静默）**：本片只做静态复核，**世界数据能否经代理下来仍须窗口实拍**；三处过期注释
  （`capture_baseline.ps1:75-83`、`tools/README.md:213-225`、`C线-交付说明.md`「已知缺口」）**未改**
  （非本卡写集）⇒ 登记待订正。
- **零生产代码改动**（仅一个 `.py` 判据 + 两份文档）；本片**未起栈、未占端口**。
- 证据：`85-csharp-capture-topology-rootcause.md`（源码锚点 + 部署件包含关系）、
  `86-csharp-capture-preflight.txt`（selftest 12/12 + 真实 GREEN + 五条改坏必红 + 还原复绿）、
  `87-gates.txt`、`88-boundary.txt`。

## ⑮ 片⑮：③ 窗口 **Rust 侧前置门** —— 构建源/读数标记/map_id 对齐（2026-10-14）—— `89`/`90`

- **背景（实测到的真缺陷）**：root 的窗口脚本 `E:/tmp/openmir2-watch/client-e2e-window.sh`
  里 `repo=/e/Users/gxh/Documents/GitHub/OpenMir2`，即**构建源 = 共享主检出**。2026-10-14 实测该检出
  停在 `ad0592b19`（`origin/master` 的祖先，但**不含**片⑪ `d16b5199d`）⇒ `main.rs` 仍是旧版式
  `[{name}({aid}) map={} vis={} same_map={} nearest={}]`，而片⑪ 的 `check_aoi_visibility_diag.py`
  ① 版式门**要求** `vr=/new=/af=` ⇒ 直接开 ③ 窗口会 `PREREQ_RED`，白烧一个独占窗口。
- **新工具 `check_rust_window_preflight.py`**（只读、零端口、零起栈）—— 片⑭ C# 侧前置门的对称件。
  五组机判（fail-closed，`0` GREEN / `1` RED / `2` PREREQ_RED）：
  R1 构建源含 `d16b5199d`（`git merge-base --is-ancestor`）；R2 源码含版式字面量
  `vr={} vis={} new={} af={}` 与可见集判据 `!= VisibleFlag::Hidden`（旧 `== Show` 只数「本轮新增」，
  是 root 误读的根因）；R3 `Envir/MapInfo.txt` 第 26 个 section 的逻辑名 == `0`
  （钉住真机周期行 `map=26` ↔ MonGen 判据用的逻辑名 `0`；**本片新补的判据**）；
  R4 窗口坐标与 `MonGen.txt` 生成框一致（调既有 `check_mongen_dense_points.py`，不复制几何）；
  R5 四个判据工具齐备。`--selftest` 17/17。
- **真机读数**：构建源 = 本车道 worktree（含片⑪）⇒ **GREEN rc=0**；
  构建源 = 共享主检出 ⇒ **RED rc=1**，R1 + R2 同时点名（`89` §C 原文）。
- **7 条改坏必红**（`90`）：map_id 26→27、逻辑名 0→9、框内点误标 outside、
  fix-commit 换非祖先、坏 ref、删源码版式标记、判据工具名写错；每条还原后 sha256 逐字节一致。
- **零生产代码改动**（仅一个 `.py` 判据 + 三份文档）；本片**未起栈、未占端口**，未动共享主检出。
- 证据：`89-rust-window-preflight.txt`、`90-redcheck-rust-window-preflight.txt`、
  `91-gates.txt`、`92-boundary.txt`。

## ⑯ 片⑯：③①「换怪密点」**选点判据** —— MonGen 窗口内期望出生怪数排行（2026-10-14）—— `93`/`94`

- **背景**：root 00:4x 的 ③① 要「换怪密点（比奇省郊外 / MonGen 里密度高的图）复测」。片⑬ 只给了
  「框内/框外 + 到最近框距离」；选点仍靠人工在若干个已知点里挑。本片把「密」变成**可排行、可机判**的定量读数。
- **工具扩展 `check_mongen_dense_points.py`**（只读、零端口；在片⑬ 既有工具上加，不新造第二份几何）：
  - `expected_in_window(boxes,x,y,vr)` = `Σ_box Count * |窗口∩框| / |框|`（整数格点，纯几何无 RNG；
    窗口 = 半径 `view_range` 的方格）。**只算出生**（游走怪另算，`70` §C 已登记）。
  - `--rank N` / `--best-point`：候选 = 去重后的生成框中心，按 E 降序（并列按 x,y 升序，确定性）。
  - `--min-expected X` / `--max-expected X`：逐点机判 E ≥ / ≤ X（缺 `--view-range` ⇒ `PREREQ_RED`）。
    旧 `--expect inside|outside`、`--point/--points/--start-point` 口径**未动**（片⑮ R4 同款 args 仍 GREEN，`93` §E）。
  - `--selftest` 10 → **19/19**（新增 E 的 4 条解析解 + rank 序 + min/max 绿红 + 缺 vr 的 PREREQ）。
- **本次测算（map 0，vr=12）**：root verify1 `(291,666)` **E=0.000**（③a 定量成立；改判 `--min-expected 1` ⇒ RED）；
  `(289,618)` E=6.754、`(287,620)` E=5.617（③b 框内点）；**最密 `(605,578)` E=12.681**
  （5 个重叠框中心：`鹿/稻草人` r=50 count=50 ×2 + `半兽人/多钩猫/钉耙猫` r=30 count=10 ×3），
  次密 `(211,629)`/`(256,572)` E=12.225。⇒ 想一次拿最多同屏怪用 `(605,578)`；想复用既有新角色出生点证据用 `(289,618)`。
- **5 条改坏必红**（`94`）：去格点 `+1` / 忽略 `Count` / 忽略重叠比 / `min` 写成 `<=` / `max` 写成 `>=`；
  每条 rc=1 + 还原后 sha256 逐字节一致。
- **零 .rs、零生产语义改动**（仅判据工具 + 一份手册引用 + 文档/证据）；本片**未起栈、未占端口**，未动共享主检出。
- 证据：`93-mongen-dense-rank.txt`、`94-redcheck-mongen-dense-rank.txt`、`95-gates.txt`、`96-boundary.txt`。

## ⑰ 片⑰：③ 收口的**窗口就绪聚合门 + 一键两侧 capture 计划**（2026-10-14）—— `97`/`98`

- **背景**：③ 的离线判决已成立（`70`：`(291,666)` 在 map 0 全部生成框之外 ⇒ `vis=0` 正确；
  框内点 `(287,620)` 客户端实收 13 对出现帧 ⇒ AOI 有算有下发），**唯一未闭环的 R 级判据 = 两侧同框
  真机对照**，归 root 独占窗口。窗口是一次性资源；片⑮/⑯ 已分别把「构建源是否含片⑪」「怪密点排行」
  做成机判，但**没有一个「一条命令的 GO/NO-GO」**——root 开窗前仍要手跑 4 个工具并自行解读。
- **新工具 `window_handoff.py`**（只读、零起栈、零端口；不复制任何判据，只聚合）：
  - **G1** = 片⑮ `check_rust_window_preflight.py`（R1..R5）解析 `RESULT:`；R1/R2 点住窗口脚本
    `repo=` 指向的检出是否含片⑪（2026-10-14 实测共享主检出停在 `ad0592b19` ⇒ RED）。
  - **G2** = 证据目录 6 件判据脚本的 `--selftest` 必须全过（缺一件 ⇒ PREREQ；某件 FAILED ⇒ RED）。
  - **G3**（可选 `--with-csharp`）= 片⑭ `check_csharp_capture_preflight.py`。
  - **INFO** = `check_mongen_dense_points.py --rank`（选点用，见 ⑯）。
  - `--plan` = ③a/③b 两侧对照的**可粘贴命令序列**（Rust 起栈/代理/客户端/解码 + C# 侧 +
    三条判据 + 收工），与 `82` 手册同口径；当 G1 RED 时自动附「改用专用 worktree 从 `origin/master`
    构建」修复块（**不动共享主检出**，只把窗口脚本 `repo=`/`tgt=` 两行指向新树）。
  - 总判决 `WINDOW_READY=YES|NO`；退出码 `0`/`1`/`2`。`--selftest` **27/27**（含 4 种聚合端到端）。
- **真机读数**：构建源 = 本车道 worktree（含片⑪）⇒ `WINDOW_READY=YES` rc=0；
  构建源 = 共享主检出 ⇒ `WINDOW_READY=NO` rc=1（`97` §B/§C 原文）。
- **3 条改坏必红**（`98`）：吞 G1 RED / 吞某判据脚本 `--selftest` FAILED / 总判决恒 0 ——
  各自都产出**假绿**（`WINDOW_READY=YES`），且都被工具内建 `--selftest` 判红（24/27、25/27、22/27）；
  还原后 sha256 逐字节一致。
- **零生产代码改动**（仅一个 `.py` 判据 + 四份证据 + README/清单）；本片**未起栈、未占端口**，未动共享主检出。
- 证据：`97-window-handoff-gate.txt`、`98-redcheck-window-handoff.txt`、`99-gates.txt`、`100-boundary.txt`。

## ⑱ 片⑱：③ 窗口计划的 **O-2 收口 + 换怪密点自动选点**（2026-10-14）—— `101`/`102`

- **背景①（reviewer-5 O-2，非阻塞已登记）**：`window_handoff.py --plan` 的 ③a 反向段当时写成
  「同上加 `--expect none`」的半句，不是**完整可粘贴命令** ⇒ root 开窗时要自己补两条命令，易漏。
- **背景②（root 00:4x ③①「换怪密点」）**：片⑯ 已把「窗口内期望出生怪数 E」做成排行，但片⑰ 计划的
  ③b `--site` **缺省恒为 `287,620`（E=5.617）**，与「最密点 `(605,578)` E=12.681」脱节 ⇒ 选点仍靠人工记忆。
- **本片改动（`window_handoff.py`，只读、零起栈、零端口；不新增第二份判据）**：
  1. **O-2**：③a 反向输出**两条完整命令**（服务端 `check_aoi_visibility_diag.py --expect none` +
     两侧 `aoi_visibility_pair.py --expect none`），并显式标注「③a 是第二趟：把 aaa 改到 site_none 重跑 §2/§3」；
     计划文本里**不再出现**「同上加」。
  2. **`--best-site`**：③b 站点由片⑯ `check_mongen_dense_points.py --best-point` **自动选取**（解析 `BEST (x,y) E=`），
     计划头部打印 `# ③b 最密点自动选取 BEST=… E=…`。缺工具/缺 `MonGen.txt`/解析不到 ⇒ **fail-soft**
     （回落 `--site` 缺省 + 打印「选点跳过：…」），**绝不静默假装选过点**（假点必红，见 `102` MUT-C′）。
- **真读数**：
  - `--selftest` **27/27 → 35/35**（新增：`parse-best-point` 三条 + `resolve-best-site` fail-soft + O-2 完整命令 +
    第二趟说明 + `site_note` 必含/必不含）。
  - `--plan-only --best-site` ⇒ `site=605,578 … # ③b 最密点自动选取 BEST=605,578 E=12.681`（`101` §B）。
  - 聚合门（构建源 = 本车道 worktree @ `origin/master`）⇒ `WINDOW_READY=YES` rc=0（`101` §D）。
- **3 条改坏必红**（`102`，均在 `/tmp` 复制件上做，仓内零改动、还原后 sha256 一致）：
  ① 把 ③a 退回「同上加」半句 ⇒ `SELFTEST FAILED (33/35)`；
  ② `parse_best_point` 恒 `None` ⇒ `SELFTEST FAILED (33/35)`；
  ③ 选点 fail-soft 被吞成假点（缺脚本时返回 `"0,0"`）⇒ `SELFTEST FAILED (34/35)`。
- **零生产代码改动**（仅一个 `.py` + 四份证据 + README/清单）；本片**未起栈、未占端口**，未动共享主检出。
- 证据：`101-window-plan-o2-bestsite.txt`、`102-redcheck-window-plan-o2.txt`、`103-gates.txt`、`104-boundary.txt`。

## ⑲ 片⑲：root verify1 **真机原始件入库** + ③ 收口判定书（2026-10-14）—— `105`

- **背景**：③ 的离线判决（`70`：`(291,666)` 在 map 0 全部生成框之外 ⇒ `vis=0` 正确；框内点
  `(287,620)` 实收 13 对出现帧 ⇒ AOI 有算有下发）**已成立**，但 root verify1 的真机原始件
  （`E:/tmp/client-e2e/stack/game-svr.out.log` + `verify1/stages.ndjson` + 34 张截图）**一直只在检出外**，
  卡 DoD⑤「证据四件套」与 AGENTS「证据要能重放」都要求入库。
- **本片新增（零生产代码改动，只把真机原始件按字节入库 + 一份判定书）**：
  - `capture-root-verify1-game-svr.out.txt`（36371 B，源 `game-svr.out.log`，`*.log`→`*.txt` 避 gitignore，内容一字未改）；
  - `capture-root-verify1-stages.ndjson.txt`（1785 B，20 阶段）；
  - `capture-root-verify1-shots.manifest.txt`（34 张 PNG 的 sha256；PNG ~34MB 不入库）；
  - `105-aoi-gap3-closure-root-verify1.txt`（判定书：读数逐条 + ③/④ 判定 + 窗口 GO + 剩余项）。
- **从入库件复算的读数**（不采信散文）：`52/752 入图输入` = 2 条（`hp=19 mp=15`，layout=client-58）；
  `Ident=3013` = **17** 条；`sim: tick` 周期行 86 条，其中玩家行 **11** 条全 `vis=0 same_map=1726 nearest=23..34`；
  `实体内` 22283→27030（峰值 28152）。⇒ 与 root 判读逐值吻合。
- **窗口就绪 GO（当前 master 重跑）**：`origin/master = c517ce4fc`，
  `window_handoff.py --repo <本 worktree> --ref HEAD --check-dir …` ⇒ `WINDOW_READY=YES` / `RESULT: GREEN` rc=0。
  ⚠ root 的 `client-e2e-window.sh` `repo=` 指共享主检出（现为 rev 车道 worktree）⇒ 开窗前按 `--plan` 的修复块换
  专用 worktree（`origin/master`）。
- **仍未闭环（归 root 独占窗口，不因本片收卡）**：DoD③ 的 R 级「两侧同框真机对照」+「屏幕见怪并能攻击造成伤害」。
- **零生产代码改动**；本片**未起栈、未占端口**，未动共享主检出。
- 证据：`105-aoi-gap3-closure-root-verify1.txt`、`capture-root-verify1-*.txt`、`106-gates.txt`、`107-boundary.txt`。

## ⑳ 片⑳：③①「换怪密点」扩到**跨图** + 片⑲ 两条随卡 Minor 收口（2026-10-14）—— `108`/`109`

- **背景（root 00:4x ①）**：root 让「换怪密点（例如比奇省郊外，**或 MonGen 里密度高的图**）复测」；
  片⑯ 的选点判据此前**只支持单图**（`--map`，窗口用 `--map 0`）⇒ 本片补**跨全部地图**排行。
- **本片改动**：
  1. `check_mongen_dense_points.py` 新增 `--all-maps`（`rank_all_maps` + `all_map_names`，确定性序
     `(-E, 图名, x, y)`）；**既有 `--map`/`--rank`/`--best-point` 输出逐字节不变**（`--map 0 --best-point`
     仍 `BEST (605,578) E=12.681`）；`--selftest` **新增 3 例**（跨图序、并列图名序、单调性）⇒ **22/22**。
  2. `108-window-dense-site-menu.txt`：**map 0 推荐点**（`605,578` E=12.681，可进入性有先例）+ map 0 Top-10
     + **全局 Top-20**（`D10071 (17,21) E=59.533`）候选菜单，并显式标注「跨图候选**可进入性未证**，
     需窗口实拍；不拿未证点换掉已证 map 0 点」。
  3. 片⑲ 的 **reviewer-5 两条非阻塞 Minor 收口**：O-1 = `105` §A 表加「源件字节 vs 入库后 LF 归一字节」两列
     （1785→1749 / 2797→2763；「内容一字未改」限定到语义/字符序列）；O-2 = `106` 的快照时点注（见下）。
- **红检（`109`，2 颗牙，变异只在 `E:/tmp` 复制件、仓内 sha256 前后一致）**：
  ① `rank_all_maps` 只看首图 ⇒ `SELFTEST FAILED (21/22)` rc=1；
  ② 全局排序降序翻升序 ⇒ `SELFTEST FAILED (20/22)` rc=1。
- **零生产代码改动**（仅一个 `.py` + 两件证据 + `105`/`106`/README/清单）；**未起栈、未占端口**，未动共享主检出。
- 证据：`108-window-dense-site-menu.txt`、`109-redcheck-dense-all-maps.txt`、`110-gates.txt`、`111-boundary.txt`。

## ㉑ 片㉑：③① 跨图候选的**可进入性机判**（MonGen × MapInfo，root ①「密度高的图」）（2026-10-14）—— `112`

- **背景**：片⑳ 的跨图 Top-20 只能标「可进入性未证」⇒ root 换图时可能选中一张 MapInfo 不加载的图而烧窗。
  本片把它变成**机判**：按 oracle `MapMgr.FindMap` 口径（逻辑名 = `|` 前）查 `Envir/MapInfo.txt`。
- **本片改动**（仅 `check_mongen_dense_points.py`）：新增 `parse_mapinfo`/`is_enterable`/`enterable_suffix`/
  `require_enterable_check`；CLI 新增 `--mapinfo` 与 `--require-enterable`（四条路径都接入）；
  **不给 `--mapinfo` 时旧输出逐字节不变**；`--selftest` **22 → 30**。
- **真机部署实测**：MonGen **207** 张图 **全部** `enterable=yes`（MapInfo 逻辑名 322 个）；
  跨图 Top-5（`D10071`/`D10061`/`D10062`/`D2002`）全 `enterable=yes` ⇒「图是否存在」这一层
  **不存在**可进入性缺口，换图候选可放心进入窗口实测（坐标落点仍须实拍）。
- **改坏必红**（三层）：① 数据侧（复制 MapInfo 注释掉 `[D10071`）⇒ `RED --require-enterable … ['D10071']` rc=1；
  ② `is_enterable` 恒 True ⇒ `SELFTEST FAILED (27/30)`；③ `enterable_suffix` 未给 mapinfo 也吐后缀 ⇒ `(28/30)`。
- **边界**：只判「图在 MapInfo 里」（= oracle `FindMap != null`），不判 MapInfo 属性（`NORECONNECT`/`SAFE`…）、
  坐标阻挡、Rust 侧该图是否已加载；换图窗口仍按 runbook §1/§4 实拍。**未起栈、未占端口**，未动共享主检出。
- 证据：`112-mapinfo-enterability.txt`、`113-gates.txt`、`114-boundary.txt`。

## ㉒ 片㉒：③ 窗口两趟证据不互相覆盖 + `--judge` 五条判据一键聚合（root ③「判据/capture 工具你准备」）（2026-10-14）—— `115`

- **背景（root 2026-10-14 00:4x ③）**：③ 的离线判决已成立、工具已备，但片⑰ 的窗口计划把 **③a 反向第二趟**
  指到**与 ③b 同一批路径**；而 `tools/m2-acceptance/start_rust_stack.ps1` 的 `Start-One` 每次起栈先
  `Remove-Item $out, $err`（`:276-278`）⇒ `game-svr.out.log` 每次被删重建 ⇒ **③a 一开跑就覆盖 ③b 读数**，
  一次性窗口事后不可复核。
- **本片改动（`window_handoff.py`，只读、零起栈、零端口；不新增第二份判据）**：
  1. **③a 独立目录树** `w_a = <work-dir>-a`（③b 的 `<work-dir>/{rust,csharp}` 路径逐字节不变）；
     `--plan` 的 §4 给出 ③a 的目录替换与两条完整判据命令，并显式写「不独立就覆盖 ③b 读数」。
  2. **`--judge`（J1）**：把 ③ 的**五条**判据（③b 服务端 `vis>=1` / ③b 客户端 `pairs>=1` /
     ③b 两侧 `visible` / ③a 服务端 `vis==0 && af==0` / ③a 两侧 `none`）聚合为一个 rc + 一行 `AOI_GAP3=`；
     **只调既有判据脚本，不复制判据逻辑**（同片⑰「只聚合不复制」）。fail-closed：任一证据缺失 ⇒
     `PREREQ_RED`（rc=2），**绝不静默当绿**。
- **真读数**：
  - `--selftest` **35/35 → 45/45**（计划 4 条：③a 独立目录 / ③a 判据参数 / `--judge` 命令 / 覆盖注；
    judge 6 条：全绿 rc0 / 五条 / 一条 RED rc1 / 2 红 3 绿 / 缺证据 PREREQ / 缺件点名）。
  - `--judge` 用**本目录真判据脚本** + 合成产物：全绿相 ⇒ `AOI_GAP3=GREEN` rc=0；③a 被 ③b 产物污染 ⇒
    `③a 服务端`/`③a 两侧` 双 RED、`AOI_GAP3=RED` rc=1；证据不存在 ⇒ 五条全 PREREQ、rc=2（`115` §C）。
  - `--plan-only --best-site` 仍 `BEST=605,578 E=12.681`；③a 判据命令改读 `<work-dir>-a/**`（`115` §B）。
- **4 颗改坏必红**（`116`）：MUT-A（去掉 ③a 独立目录）⇒ `SELFTEST FAILED (42/45)`；MUT-B（`run_window_judges`
  把 rc=1 当绿）⇒ `(43/45)`；③a 污染 ⇒ 双 RED；缺证据 ⇒ 五条 PREREQ。变异只在 `E:/tmp` 复制件、仓内
  `window_handoff.py` sha256 前后一致。
- **零生产代码改动**（仅一个 `.py` + 四份证据 + README/清单）；**未起栈、未占端口**，未动共享主检出。
- **仍未闭环（归 root 独占窗口，不因本片收卡）**：DoD③ 的 R 级「③b/③a 两趟真机同框对照」+「屏幕见怪并
  能攻击造成伤害」。
- 证据：`115-aoi-gap3-window-judge.txt`、`116-redcheck-window-judge.txt`、`117-gates.txt`、`118-boundary.txt`。

## ㉓ 片㉓：③ 窗口计划**补构建步骤** + 构建产物缺失提示（root ③「判据/capture 工具你准备」）（2026-10-14）—— `119`

- **背景（源码实读、真机实证的烧窗路径）**：片⑰/⑱/㉒ 的计划把两条 `start_rust_stack.ps1` 命令与
  `-RustBinDir <target-dir>/debug` 写成可粘贴文本，但**全计划没有任何构建步骤**；而 `start_rust_stack.ps1:196-206`
  对 `game-svr.exe`/`login-gate.exe`/`sel-gate.exe`/`game-gate.exe` 逐个 `Test-Path`，缺一即
  `PREREQ_MISSING … exit 2`。计划默认 `<target-dir>` = `E:/tmp/m2svr-window-target` **本机不存在**
  （root 的窗口脚本另用 `E:\tmp\openmir2-merge-target`）⇒ 从 §0 粘到 §5 会在第 2 步直接失败。
- **本片改动（`window_handoff.py`，零起栈/零端口；不新增第二份判据）**：
  1. **§0a 构建块**：`plan_text` 在 §0 后、§1 前插入与窗口同一个 `repo`/`target-dir` 的构建命令
     （`git rev-parse HEAD` 记构建源 + `CARGO_TARGET_DIR=<tgt> cargo build --offline -p game-svr -p game-gate
     -p login-srv -p login-gate -p sel-gate -p db-srv`，与 `client-e2e-window.sh` 第 2 步逐 package 同集）。
  2. **bin-missing 提示**：`main()` 实测 `<target-dir>/debug/game-svr.exe`，缺则打
     `⚠ 构建产物不存在：…`（存在则**不打**）——把「构建产物不在」提前到开窗前一眼可见。
- **真读数**：
  - `--selftest` **45/45 → 49/49**（新增 `plan-has-build-step` / `plan-build-step-repo` /
    `plan-build-before-start` / `plan-bin-missing-warning`；既有 45 条逐条不动）。
  - `--plan-only --best-site`：默认 target 缺件 ⇒ 打出 `⚠ 构建产物不存在：E:/tmp/m2svr-window-target/debug/game-svr.exe`；
    `--target-dir E:/tmp/m2svr-target`（已构建）⇒ 该行消失。
  - `start_rust_stack.ps1 -RustBinDir E:/tmp/no-such-target/debug` ⇒ `PREREQ_MISSING …` **rc=2**（burn-mode 实证）。
  - G1：构建源=本车道 worktree ⇒ `WINDOW_READY=YES`/`GREEN`；构建源=共享主检出（`ad0592b19`）⇒ `RED` + 修复块，
    修复块 + §0a + §2 构成「换树 → 构建 → 起栈」完整序列。
- **改坏必红**（`120`，2 颗牙，变异只在 `E:/tmp/m2svr-burst10/mut/` 复制件、仓内 sha256 前后一致）：
  ① 删 §0a 构建命令 ⇒ `SELFTEST FAILED (46/49)`；② 吞 `bin_missing` 提示 ⇒ `(48/49)`。
- **零生产代码改动**（仅一个 `.py` + 四份证据 + README/清单）；**未起栈、未占端口**，未动共享主检出。
- **仍未闭环（归 root 独占窗口，不因本片收卡）**：DoD③ 的 R 级「③b/③a 两趟真机同框对照」+「屏幕见怪并
  能攻击造成伤害」。
- 证据：`119-window-plan-build-step.txt`、`120-redcheck-window-plan-build.txt`、`121-gates.txt`、`122-boundary.txt`。

## ㉔ 片㉔：③ 窗口计划**补抓包拓扑**（影子 game-gate 7600 + 代理占 7200）（2026-10-14）—— `123`

- **背景（源码实读 + 湿测的烧窗路径）**：片㉓ 补了构建步骤后，计划仍把 `mir2_proxy.py --map 7200=…` 直接接在
  起栈命令后，没有「先让出官方 7200、再起影子 game-gate(7600)」。`start_rust_stack.ps1:323` 起的标准
  Rust `game-gate` 必然占着 7200，而 `mir2_proxy.py:137-145` 的 `asyncio.start_server` 在端口被占时抛
  `OSError(10048)`、不写 `ready.flag` ⇒ **无 capture ⇒ ③b 客户端 pairs=0/PREREQ，白烧窗口**。
- **本片改动（`window_handoff.py`，零起栈/零游戏端口）**：`plan_text()` 在 §2 段内新增 **§2a 抓包拓扑**——
  Rust 侧照已证步骤（`tools/m2-acceptance/evidence/m2svr-whitelist-rerun-20261012/README.md` §4：停标准
  game-gate 让出 7200 → 起影子 game-gate(7600，`game-gate.exe <影子 conf>`，conf 最小集 5 键) → 代理占 7200）；
  C# 侧同口径并指向 `82-two-side-window-runbook.md` §4.1。两块都排在各自 `mir2_proxy.py` 之前。
- **真读数**：`--selftest` **49/49 → 53/53**（新增 `plan-has-capture-topology` / `plan-topology-before-proxy` /
  `plan-topology-shadow-conf` / `plan-topology-csharp-pointer`；既有 49 条逐条不动）。
- **burn-mode 湿测**（非游戏端口 57999/57997，绕开十端口）：代理监听端口被占 ⇒ `rc=1` 且无 `ready.flag`；
  让出后 ⇒ `LISTEN` + `ready.flag=True`。
- **改坏必红**（`124`，2 颗牙，变异只在 `E:/tmp/m2svr-burst11/mut/` 复制件、仓内 sha256 前后一致）：
  ① 删 §2a 4 行 ⇒ `SELFTEST FAILED (50/53)`；② 删 C# 指针 ⇒ `(52/53)`。
- **零生产代码改动**（仅一个 `.py` + 四份证据 + README/清单）；**未起栈、未占任何游戏端口**，未动共享主检出。
- **仍未闭环（归 root 独占窗口，不因本片收卡）**：DoD③ 的 R 级「③b/③a 两趟真机同框对照」+「屏幕见怪并
  能攻击造成伤害」；C# 侧世界数据能否经代理下来须窗口实拍（`85-…md` §E）。
- 证据：`123-window-plan-capture-topology.txt`、`124-redcheck-window-plan-capture-topology.txt`、`125-gates.txt`、`126-boundary.txt`。

## ㉕ 片㉕：③ 窗口计划补 **teardown/清端口段** + 计划↔工具面 **G4 机判**（2026-10-14）—— `127`

- **背景（源码实读，与片㉔ 同类的烧窗路径）**：片㉔ 把「**起**代理前的抓包拓扑」补全后，计划仍**没有补「收」**：
  ① C# 趟收栈写的是**不存在**的 `{mirserver}/stop-all.ps1`（实测 `E:/MirServer` 只有
  `start-all.ps1`/`port-status.ps1`/`restart-gamesvr.ps1`/`build-client-104.ps1`）；② 手工起的
  **影子 game-gate(7600)** 不在 `start_rust_stack.ps1` 的 pid 文件里 ⇒ `-Stop` 只停 pid 文件 entries
  （`start_rust_stack.ps1:150-157`），影子不收；③ 代理 `mir2_proxy.py` 常驻（`:148 serve_forever()`、pid 只在
  `ready.flag`），计划不收 ⇒ 下一趟 §0 预检 `RED: 目标端口已被监听 7200`（`:171-177`，`$wanted` 含 7200）。
  三者都会让下一趟（C# 趟 / ③a 反向趟）起不来 ⇒ **一次性窗口再白烧**。另 `start-all.ps1 -Stop` 的 targets
  含 `mysqld`（`start-all.ps1:117`）⇒ **不能**拿它收 C# 栈（会停共享 3306）。
- **本片改动（`window_handoff.py`，只读规划器；不新造第二份判据）**：
  1. **§2b 清端口段（Rust 趟）**：用 `<out>/ready.flag` 的 pid 停代理 → 按绝对路径停影子 game-gate →
     `start_rust_stack.ps1 -Stop` 停 pid 文件其余 → `port-status.ps1` 断言 7200/7600 都 DOWN。
  2. **§2b 清端口段（C# 趟）**：同法停代理 + 按绝对路径停 C# 影子（`{mirserver}/RunGate/GameGate.exe`）+
     `Get-Process GameSvr,DBSrv,LoginSrv,LoginGate,SelGate,GameGate | Stop-Process -Force`（**保留 3306**）；
     删除不存在的 `stop-all.ps1`，写明**不要**用 `start-all.ps1 -Stop` 的理由。
  3. **占位替换**：§2a 的 `<bin_dir>` 代入实际 `-RustBinDir`；`port-status.ps1` 改随 `{mirserver}` 走。
  4. **G4 `plan_conformance`**（静态、零起栈/零端口）：计划里每个被调 `.ps1/.py` 必须存在；每个命令里
     **脚本路径之后**的 `-Flag`/`--flag` 必须在该脚本的声明集合里（`.ps1` 取 `param(...)` 的 `$name`、
     `.py` 取 `add_argument('--x')`；宿主 powershell 开关排除）；必须含 6 条 teardown/清端口标记；
     不得出现 `stop-all.ps1`、不得残留 `<bin_dir>`。`--plan`/`--plan-only` 打印 `PLAN_CONFORMANCE=`，
     RED 逐条点名并把 rc 抬红（fail-closed）。
- **真读数**：
  - 计划 before/after：C# 收栈 `E:/MirServer/stop-all.ps1` → **消失**；新增 Rust/C# 两段 §2b；`<bin_dir>` 占位 0 命中（`127` §C-1）。
  - 真计划 G4：`PLAN_CONFORMANCE=GREEN` rc=0（`--json`：`{"plan_conformance": "GREEN", "problems": [], "rc": 0}`）；
    就绪门 `--repo <本 worktree> --ref HEAD --plan --best-site` ⇒ `WINDOW_READY=YES` / `RESULT: GREEN` rc=0（`127` §C-2）。
  - `--selftest` **53/53 → 63/63**（6 条 teardown 结构 + `plan-conf-green` + 3 条 G4 改坏必红；既有 53 条不动）。
- **改坏必红**（`128`，3 颗牙，变异只在 `E:/tmp/m2svr-burst12/mut/` 复制件、仓内 sha256 前后一致）：
  MUT-A 写回 `stop-all.ps1` ⇒ `SELFTEST FAILED (61/63)`；MUT-B 删 §2b 首行 ⇒ `(61/63)`；
  MUT-C `plan_conformance` 恒绿 ⇒ `(60/63)`。
- **零生产代码改动**（仅一个 `.py` + 四份证据 + README/清单）；**未起栈、未占任何游戏端口**，未动共享主检出。
- **仍未闭环（归 root 独占窗口，不因本片收卡）**：DoD③ 的 R 级「③b/③a 两趟真机同框对照」+「屏幕见怪并
  能攻击造成伤害」。
- 证据：`127-window-plan-teardown.txt`、`128-redcheck-window-plan-teardown.txt`、`129-gates.txt`、`130-boundary.txt`。
