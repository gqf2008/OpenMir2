# tools/oracle/ —— oracle（C# 服务端）维护线工具（S1）

> 设计文档：`docs/Rust重写设计文档.md` §2.1（谁是 oracle：部署件契约与版本对账）、§16（S1 处置）。
> 本目录只放工具。与 `tools/` 下 C 线工具的分工：C 线做抓包/假人/E2E **观测**，
> 本目录做 oracle 的**部署与修复**（改 `src/**` 走 `whitelist.md` 的 `T-*` 登记）。

| 工具 | 作用 | 红检（改坏必红） |
| --- | --- | --- |
| `reconcile_deployment.ps1` | 部署件 ↔ 源码构建输出逐字节对账；判 `SINGLE-BUILD`/`MIXED`/`FOREIGN`/`SELF-HOSTED`；另核对"未声明为部署件的旧目录"的血统（PE 还是 Mach-O、`deps.json` 的 runtimeTarget） | `-SelfTestRed`：复制一组产物改坏一个字节，比较器必须判 DRIFT |
| `deploy_gamesvr.ps1` | 把**同一份构建**的 GameSvr 组件集整体铺到 `E:\MirServer\M2GameSvr`（带备份、只铺本仓库自有文件、铺后逐文件复核哈希）；`-WhatIf` 只列将要替换的文件 | 铺后哈希复核（`OK`/`RED` 逐文件） |
| `verify_gate_reconnect.ps1` | **实机门禁**：只重启 GameGate 制造一次网关重连，然后跑真实客户端进世界；断言 ① 流程走到 `ingame` 且截图画出画面 ② 抖动后没有新增 `SetGateUserList` NRE ③ GameSvr 日志出现第二条"网关已打开" | `-SelfTestRed`：断言一个不存在的阶段，必须判 RED |
| `gate-slot-probe/` | 不起整栈、不碰 oracle 进程的**确定性探针**：直接跑生产类型 `TCPNetChannel`，用"连接#1 → 断开 → 连接#2"复现/守护"网关重连后槽位失配"。输出槽位表与四项判据 | 先跑未修版本必须 1 GREEN / 3 RED（见交付说明） |

## 常用命令

```powershell
# ① 对账（判部署件是不是这份源码编的）
powershell -ExecutionPolicy Bypass -File tools/oracle/reconcile_deployment.ps1
powershell -ExecutionPolicy Bypass -File tools/oracle/reconcile_deployment.ps1 -SelfTestRed

# ② 铺部署（默认取 <repo>/src/GameSrv/bin/Release）
powershell -ExecutionPolicy Bypass -File tools/oracle/deploy_gamesvr.ps1 -WhatIf
powershell -ExecutionPolicy Bypass -File tools/oracle/deploy_gamesvr.ps1

# ③ 实机门禁（要求整栈在跑；只动 GameGate，不动 GameSvr/MySQL/LoginSrv/DBSrv）
powershell -ExecutionPolicy Bypass -File tools/oracle/verify_gate_reconnect.ps1

# ④ 确定性探针（不依赖整栈，用 15000 当临时网关口）
dotnet build tools/oracle/gate-slot-probe/GateSlotProbe.csproj -c Release
tools/oracle/gate-slot-probe/bin/Release/GateSlotProbe.exe --port 15000    # 退出码 0 = 行为符合修复后契约
```

## 已知边界

- `verify_gate_reconnect.ps1` 会重启 GameGate 并占用整栈约 2~3 分钟：**别与其他线正在跑的服务端任务并行执行**
  （与 `tools/capture/capture_baseline.ps1` 同一条纪律）。
- 它只等"进世界"这一段就提前结束流程（`mir_flow.ps1` 后面的小退段有十来次固定等待，与本门禁判据无关）。
- `gate-slot-probe` 启动时会把 `E:\MirServer\M2GameSvr` 的 `*.conf`/`*.txt` 复制到自己的输出目录
  （`M2Share`/`SystemShare` 静态构造从 `AppContext.BaseDirectory` 读配置），可用 `--oracle-dir` 改。
- 探针**不是** `tools/e2e/run_e2e.ps1` 的替代：前者只判"网关重连后槽位是否还指向活连接"这一条不变量。
