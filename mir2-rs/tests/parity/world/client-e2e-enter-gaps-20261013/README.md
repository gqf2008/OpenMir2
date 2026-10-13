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
