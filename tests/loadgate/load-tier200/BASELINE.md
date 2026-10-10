# C# 侧 200 假人基线（M1 ⑤ 复用件，C6 产出）

**用途**：作为「200 假人登录+选角成功率 100%」的 C# 侧基线数（设计文档 §6 **M1 ⑤**）。
M1 把 LoginGate 换成 Rust 实现后，用**同一条命令**、同一档位、同一判据跑一遍，两侧一比即是 M1 ⑤ 的判据。
**本目录的产物与命令请直接引用，不必重造。**

## 复现命令（就是跑出本目录产物的那条）

```powershell
# 前置：服务端全栈在跑（E:\MirServer\start-all.ps1），账号已批量开好
powershell -ExecutionPolicy Bypass -File tools/client/account_provision.ps1 -Prefix loadbot -Count 1000
# 200 假人档（登录+连接判据；M1 ⑤ 用这一档的口径）
powershell -ExecutionPolicy Bypass -File tools/botload/load_gate.ps1 `
  -Tiers 200 -HoldSec 90 -StaggerMs 50 -Criteria login,conn `
  -OutDir <本目录>
```

指向被测栈：`-Address <host> -Port 7000`（**默认就是 127.0.0.1:7000**；M1 时把 LoginGate 换成 Rust 版即可，
SelGate/DBSrv 仍可用 C#）。`-Criteria login,conn` 表示只判登录率与连接质量——
M1 ⑤ 的假人**只跑无状态登录链路**、不产生世界内动作，因此没有 `+GD` ack，tick 判据在这里不适用
（默认的 `login,conn,tick` 三条会把 tick 缺失算红，那是 C6 自己的口径；见下"两种读法"）。

## 实测结果（C# 栈，2026-10-10）

| 指标 | 值 |
| --- | --- |
| 档位 / 连接错峰 / 稳态时长 | 200 / 50ms 每假人 / 90s |
| **登录成功率** | **200/200 = 100%** |
| 稳态掉线（登录后 reset/timeout） | **0** |
| 连不上（ConnectionRefused） | 0 |
| 上线耗时（爬坡到满员） | 15.0s |
| 假人侧兜住的异常（`internal_errors`） | 0 |
| GameSvr 内存（WS） | 峰值 314.4MB / 均值 314.1MB / **末段斜率 +0.008 MB/s ≈ 0** |
| tick P99 | **空缺**（本档 0 个 tick 样本，原因见下） |

产物：`load_stats.ndjson`（BotSrv 每 5s 一行累计：spawned/login_ok/conn_*/tick 直方图/内部异常）、
`mem.ndjson`（各进程 WS/私有字节/CPU 时间序列）、`report.json`（归算结果）、`SUMMARY.md`（报告+曲线）。
判据与口径的单一真源：[tools/botload/README.md](../../../tools/botload/README.md)。

## 两种读法（都记在案，避免误读）

- `-Criteria login,conn`（**M1 ⑤ 用这个**）：登录 100% + 稳态掉线 0 + 爬坡完成 ⇒ **GREEN**。
- 默认 `login,conn,tick`（**C6 自己的口径**）：因 tick 样本缺失 ⇒ **RED**（诚实读数，不粉饰）。

## 为什么这一档没有 tick P99（不是测量仪的问题）

tick 口径挂在「**被接受的客户端动作 → 服务端回 `#+GD/<rtime>`**」上（详见 tools/botload/README.md）。
本档假人**进得了世界**，但产生不了"每个假人各自的动作"：BotSrv 的挂机/移动层建立在**跨假人共享的全局态**
（`MShare.MySelf` / `AutoMove` / `MapPath`…）上，只有"登录→选服→选角→进世界"这条链路是无状态的。
实测证据（20 档对照，见 `../load-smoke20/`）：进世界 7/20 ✓、无崩溃，但 `开始自动挂机`=0、`+GD` ack **0 条**。

⇒ 对 M1 ⑤ **无影响**（它不需要假人动作，判据是登录+选角）。若 M2/M5 需要 P99，走"**探针 + 负载分离**"：
动作/tick 探针用真客户端 + `tools/capture/clienthook`（口径不变，量的还是同一条线上字节），负载用假人。
