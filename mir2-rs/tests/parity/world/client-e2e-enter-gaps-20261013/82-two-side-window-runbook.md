# 82 — ③ 两侧对照窗口运行手册（`openmir2-client-e2e-enter-gaps`）

> 面向 **root 独占窗口**（2026-10-14 00:4x：「真机窗口由 root 开；判据/capture 工具你准备」）。
> 本手册把 ③ 收口所需的两侧对照做成**可照抄的步骤**，并显式登记唯一未证风险的环节（C# 侧抓包拓扑）。
> **车道自身不起栈**（m2-svr 纪律）；窗口里由 root 执行。

## 0. 要证的两件事（与 root 00:4x 逐条对应）

| # | 判据 | 期望读数 | 机判命令 |
| --- | --- | --- | --- |
| ③a | 该点确实无怪（两侧一致） | 两侧 `appearance_pairs == 0`；Rust 服务端 `vis==0 && af==0` | `aoi_visibility_pair.py --expect none` + `check_aoi_visibility_diag.py --expect none` |
| ③b | 框内点 AOI 有算有下发（两侧一致） | 两侧 `appearance_pairs >= 1`；Rust 服务端 `vis>=1 && af>=2` | `aoi_visibility_pair.py --expect visible --min-pairs 1` |

坐标（用 `check_mongen_dense_points.py` 现场复核，别凭记忆）：

```powershell
python -I -B mir2-rs/tests/parity/world/client-e2e-enter-gaps-20261013/check_mongen_dense_points.py `
  --mongen E:/MirServer/M2GameSvr/Envir/MonGen.txt --map 0 --view-range 12 `
  --points "291,666;291,651;289,618;287,620;650,631;330,266"
```

- **用哪个「框内」点**：`(289,618)`（= `Envir/StartPoint.txt` 的 oracle 出生点，框内，也是新角色默认体验）
  或 `(287,620)`（片⑦ AFTER 已证客户端实收 13 对）。`(330,266)` 也框内（另一出生点）。
- **不要再用 `(291,666)` / `(291,651)` 当「有怪点」**：二者在 map "0" 的 **76 个生成框之外**
  （最近框 Chebyshev=30 / 29 > `ViewRange=12`）⇒ 该点同屏无怪是正确行为（见 `81-…txt` §B/§C）。
  `(291,666)` 只用来做 ③a 的**反向对照**。

## 1. 前置（任一不满足就停手）

1. `E:/MirServer/port-status.ps1`：5000/5100/5500/5600/5700/6000/7000/7100/7200 全 DOWN（仅共享 3306 可 UP）。
2. 无残留 BotSrv / game-svr / cargo / rustc（`Get-Process`）。
3. `git fetch origin && git reset --hard origin/master`（窗口基 = 跑前 master oid，写进证据）。
4. 抓包栈一律**不用管道**：起栈命令 `... > <log> 2>&1`（禁 `| tee/tail/head/grep`，会把调用挂死）。

## 2. 站位（两侧必须逐字节同一坐标）

`mir2_db.characters.aaa` 的坐标就是进世界起点（真机日志 `进世界 ChrName:aaa CurMap:0 (x,y)`）。
逐相位改：

```sql
-- 有怪点
UPDATE characters SET x=289, y=618 WHERE chrname='aaa';   -- 或 287,620
-- 无怪点（反向）
UPDATE characters SET x=291, y=666 WHERE chrname='aaa';
```

⚠ 跑完必须还原到开工前值并记录（本手册开工前 = `(291,666)`，2026-10-14 实测值；若已漂移以实测为准）。

## 3. Rust 侧（已证可跑，照抄）

```powershell
# 3.1 整栈（Rust game-svr 5000 + Rust 三网关 7000/7100/7200 + C# DBSrv/LoginSrv + MySQL 3306）
powershell -ExecutionPolicy Bypass -File tools/m2-acceptance/start_rust_stack.ps1 `
  -RustBinDir E:/tmp/openmir2-window-target/debug -Seed 42 -LogDir E:/tmp/window/cliente2e/rust/stack

# 3.2 把真 game-gate(7200) 停掉，起**影子** game-gate(7600)（同 `RunGate/config.conf`，只把 GatePort* 改 7600）
#     再让代理占官方 7200：
python tools/capture/mir2_proxy.py --out E:/tmp/window/cliente2e/rust/proxy --map 7200=127.0.0.1:7600

# 3.3 真客户端（同一 MirClinet.exe / 同一账号 mir2test / 角色 aaa）
powershell -ExecutionPolicy Bypass -File tools/client/mir_flow.ps1 `
  -Account mir2test -Password mir2pass -OutDir E:/tmp/window/cliente2e/rust/shots `
  -WalkClicks 0 -AutoPlaySec 12 -SkipReenter -SkipLogout -SkipTalk

# 3.4 解码客户端收到的 s2c 字节
powershell -ExecutionPolicy Bypass -File tools/m2-acceptance/export_s2c.ps1 `
  -ProxyDir E:/tmp/window/cliente2e/rust/proxy -OutDir E:/tmp/window/cliente2e/rust/s2c
```

Rust 侧判据（三条）：

```powershell
cd mir2-rs/tests/parity/world/client-e2e-enter-gaps-20261013
# 服务端：新读数 vr/new/af（老二进制没有这三个字段 ⇒ PREREQ_RED，属正确 fail-closed）
python -I -B check_aoi_visibility_diag.py --log E:/tmp/window/cliente2e/rust/stack/game-svr.out.log `
  --expect visible --min-vis 1
# 客户端：出现帧对 >= 1
python -I -B check_client_s2c_frames.py --capture E:/tmp/window/cliente2e/rust/s2c/capture.jsonl `
  --min-appearance-pairs 1
# 截图留证（屏幕上要能看到怪）
```

## 4. C# 侧（先读 `85-csharp-capture-topology-rootcause.md` 再动手）

C 线（2026-10-10）把「`CM_LOGINNOTICEOK` 之后世界数据下不来」归因于**影子网关端口被挪**；
`85-…md` 按当前源码逐路径复核**推翻了该归因**：

- `!servertable.txt` 只在**多机转移**路径被消费（`GameShare.GetMultiServerAddrPort` →
  `AppService.cs:221` 关机转移 / `WorldServer.cs:1063-1074` `SM_RECONNECT`）；单机进世界不读它。
- GameSvr 对网关按**槽位**寻址（`TCPNetChannel` 的 `_gateSlotByConnection`/`_gameGates`），
  世界侧 `GateIdx` = 网关在认证串里自报的 `ServiceId`（`ClientSession.cs:1203`），**与端口无关**。
- 同症状的确定根因是 **S1 缺陷①**（槽位按 accept 序 + `CloseGate` 不释放槽位），
  修复 `607e9b773`（2026-10-10 17:01）**已进部署件**（`!deployed-from.json` = master `628d8024`）。

### 4.0 前置门（起 C# 栈之前先跑；离线、零端口、fail-closed）

```powershell
cd mir2-rs/tests/parity/world/client-e2e-enter-gaps-20261013
python -I -B check_csharp_capture_preflight.py `
  --repo E:/Users/gxh/Documents/GitHub/OpenMir2/.claude/worktrees/m2-svr `
  --mirserver E:/MirServer --shadow-config <影子 RunGate/config.conf>
```

退出码 `0`=GREEN / `1`=RED / `2`=PREREQ_RED。它咬五条：部署件含 `607e9b773`；真实与影子
`ServerWorkThread=1`（S1 §2.1 多网关错配边界）；真实 `GatePort1=7200`；`!servertable.txt`
GameGate 端口 `=7200`；上游 `127.0.0.1:5000`。**红灯先修，不要带着红灯起栈。**

### 4.1 影子 RunGate 配置

拷 `E:/MirServer/RunGate/config.conf` 一份，**只改 `GatePort1=7600`**（绑定地址、上游地址、
`ServerWorkThread` 一律不动）；**不要**引入第 2/3 个活动网关（本部署本就 `ServerWorkThread=1`）。

- **首选（需现场验证）**：照 §3 **只挪 game-gate**（真 LoginGate/SelGate 保持 7000/7100 不代理，
  game-gate 影子挪到 7600，代理占 7200）。Rust 侧同拓扑能拿到世界数据（§3 已证），
  C# 侧**仍须在窗口里实拍确认**（看代理 `chunks.ndjson` 里 7200 跳在公告之后是否还有 s2c 字节）——
  本片只把「端口=根因」这个旧归因订正掉，**不把「应该能」写成「已经能」**。
- **若 C# 世界数据仍下不来**（老现象复现，且 §4.0 前置门已 GREEN）⇒ 按 `C线-交付说明.md`「解除判据」#1：
  让 C# 自己把网关挂在可插桩端口，并把 `!servertable.txt` 与 LoginSrv `AddrTable.txt` 一起指向该端口。
  **不要在窗口里现造**——这是 oracle 侧配置改动，属 §6.0-3 登记项，先记 `comment` 请 owner 裁决。
- **降级判据（不依赖抓包）**：C# 与 Rust 各在**同一坐标**起一次真客户端，各留 `mir_flow.ps1` 截图
  （`-OutDir`），人眼/像素比对「屏幕上是否有怪」。这仍满足 root 的「同一位置、同一 viewrange 两侧对照」，
  但没有逐帧计数；若走降级路，必须在卡上写明「两侧对照 = 截图级（抓包拓扑受限）」。

## 5. 两侧对照（机判）

```powershell
cd mir2-rs/tests/parity/world/client-e2e-enter-gaps-20261013
# ③b 有怪点：两侧都 >=1 对 ⇒ AOI 有算有下发（④ 反证）
python -I -B aoi_visibility_pair.py `
  --left E:/tmp/window/cliente2e/rust/s2c/capture.jsonl `
  --right E:/tmp/window/cliente2e/csharp/s2c/capture.jsonl `
  --expect visible --min-pairs 1
# ③a 无怪点（291,666）：两侧都 0 对 ⇒ 「该点确实无怪」
python -I -B aoi_visibility_pair.py `
  --left E:/tmp/window/cliente2e/rust/s2c/capture.jsonl `
  --right E:/tmp/window/cliente2e/csharp/s2c/capture.jsonl --expect none
```

工具口径：默认按**计数**判（`--min-pairs`），`--strict-recogs`（默认关）会额外要求两侧 actor 集合逐项相等
（同图同种子才成立；C# 与 Rust 同种子由 `-Seed`/C# 侧 hook 注入保证，见 `openmir2-m3-rust-stack-seed-plumb`）。

## 6. 收工（DoD）

1. 停栈：`start_rust_stack.ps1 -Stop`；C# 侧按既有 `-Stop`；确认 5000/5100/5500/5600/5700/6000/7000/7100/7200 全 DOWN
   （仅共享 3306 可 UP）——实拍 `E:/MirServer/port-status.ps1`。
2. 还原 `characters.aaa` 到开工前坐标，记录 before/after。
3. 证据四件套（命令 + 输出 + 路径 + sha256；`SHA256SUMS` 按 LF/git-blob 口径用
   `python -I tools/evidence-hash/verify_sha256sums.py write <dir>` 生成并 `verify` 自检）；
   证据入 `mir2-rs/tests/parity/world/client-e2e-enter-gaps-20261013/`。
4. 卡上 `patch`（base=跑前 master、head=你的提交）+ `status: in-progress`（work 写 ③a/③b 结论）。
5. 若走降级（截图级）⇒ 证据里如实写清，不许把截图级写成帧级。

## 7. 登记的风险 / 未做

- **C# 抓包拓扑**：旧归因（端口被挪）已由 `85-…md` 订正为 S1 缺陷①（已修 + 已部署）；
  §4.0 前置门可离线机判。**但世界数据能否经代理下来仍须窗口实拍**；未实拍前不得写「两侧帧级一致」。
- `RM_DISAPPEAR(48)` 无编码器（怪离视野无消失帧，`bins/game-svr/src/downlink.rs` 已登记）与 ③ 无因果，
  本手册不含；归属见 `70-…txt` §F（等 owner 指派）。
- 本手册**零生产代码改动**；只补判据工具（`check_mongen_dense_points.py`、`check_csharp_capture_preflight.py`）与运行步骤。
