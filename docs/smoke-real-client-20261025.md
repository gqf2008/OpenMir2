# 真客户端端到端冒烟（2026-10-25 夜）

> **一句话结论**：**冻结的 Delphi 客户端连 Rust 服务端，完整走通「登录 → 选服 → 选角 → 进世界 → 公告 → 说话 → 攻击 → 走路 → 软关 → 重进」，零客户端错误、零服务端 ERR。**
> 这是项目三条硬约束（客户端冻结 / 内容冻结 / 协议逐字节兼容）的**端到端演示**，不是对拍证据——**逐字段等价仍以窗口对拍为准**。
>
> 本节数字全部可由 `E:/tmp/smoke/` 下产物复算；代码快照 `7a4c12f3b`。

## 1. 被测物

| 项 | 值 | 来源 |
|---|---|---|
| 服务端 | `mir2-rs/target/release/mir2-server.exe`，sha256 `24423c134e8805a551e2b8b992f8f5b03999e129c01ac1fc6de4483345f8039b…`（见下方注） | `--release` 构建于 22:07 |
| 客户端 | `D:\MirClient-run\MirClinet.exe`（**Delphi 真客户端**，非 `MirClientZig*`），sha256 `c3cb8f73d8872c5ad3e53f0f55a96d03139fb49397ebcacb4b3573c3e1c07996`，6,561,280 B，mtime `2026-10-09T16:55:22Z` | `E:/tmp/smoke/shots/flow-*/manifest.json` |
| 内容 | 部署件副本 `E:/tmp/smoke/rust-root`（六角色树，282 MB）；**只按字节改写 11 个 conf 的端口号**，GBK 文本逐字节未动 | `E:/tmp/smoke/build_root.py` |
| DB | `mir2_test_data_r` / `mir2_test_db_r` / `mir2_test_account_r`（与生产库隔离） | 连接串未改 |
| 端口 | game-svr 5000/6000/5600、db-srv 6000/5100/5700、login-srv 5500/5600、login-gate 7000、sel-gate 7100、game-gate 7200 —— **与 C# 部署同一套标准端口** | `E:/tmp/smoke/port-profile.json` |

> 二进制 sha256 全文：`24423c134e8805a551e2b8b992f8f5b03999e129c01ac1fc6de4483345f8039b`（`sha256sum` 现查）。

## 2. 阶段 × 证据

阶段时间戳来自客户端侧 `stages.ndjson`（**25 条**），服务端侧行号来自 `E:/tmp/smoke/logs/game-svr.out.log`（**注意：该文件时间戳为 UTC，比本地早 8 小时**，如 `14:14:28` = 本地 `22:14:28`）。

| # | 阶段 | 客户端证据 | 服务端证据 | 结果 |
|---:|---|---|---|---|
| 1 | 登录（7000） | `conn-0001` c2s 72 B / s2c 79 B | `登录网关` 连接 | **PASS** |
| 2 | 选服（7100） | `conn-0002` 68/89 B | `[选服] account=[mir2test] server=[热血传奇] gateIP=[127.0.0.1]` | **PASS** |
| 3 | 选角 | `04_charsel.png` | `[QueryChr] 候选 account=mir2test i=0 name=[aaa]` | **PASS** |
| 4 | 进世界（7200） | `conn-0003` open | L328 `进世界 ChrName=aaa CurMap:0 (272,654) Dir:6 entity=6496`；L330 帧下发 15 条 | **PASS** |
| 5 | 公告 | `05c_ingame_ready.png` 弹窗 | Notice 320 B 已载入 → 通过 `SM_*` 下发 | **PASS** |
| 6 | 说话 | 聊天区 `aaa:talk ok` | c2s 34 B → s2c 33 B 成对 | **PASS** |
| 7 | GM 命令 | 聊天区 `攻击模式: 全体攻击` | L361/364 `GM 命令 ChrName=aaa ActorId=6496 tail="@AttackMode" found=true 帧数=1` | **PASS** |
| 8 | 走路 | `06_attack.png` 显示 `比奇省 272:657`（起 654） | `Ident=3011/3013` 走步命令 + 位置更新 | **PASS** |
| 9 | 软关 | `07b_logout_confirm_dlg.png` | L382 `CM_SOFTCLOSE … BoReconnection=true/BoSoftClose=true` | **PASS** |
| 10 | 存档 | — | L385 `存档请求已发 ChrName=aaa queryId=4 记录 4832 字节` → L388 `存档应答 Ident=1102` | **PASS** |
| 11 | 软关后回选角、重连选服（7100） | `conn-0004` c2s 68 B / s2c 89 B | 新一轮 `[QueryChr]` 角色查询 | **PASS** |
| 12 | 退出菜单 / 确认 | `07a_logout_btn_hover.png`、`07b_logout_confirm_dlg.png` | —（纯客户端 UI） | **PASS** |
| 13 | 重进游戏（7200） | `back_charsel` / `reenter` / `reentered`；`conn-0005` | L394–400 新会话 `SessionId=9165` → `进世界 ChrName=aaa (273,656) Dir:0 entity=7604`（**即上一次存档的值**） | **PASS** |
| 14 | 关服 | 客户端退出（`done`） | L436–440 存档 `queryId=6` 应答；L476–480 `关服完成：在册 0 / 超时 false` | **PASS** |

客户端 hook **75 个事件、10 个 bin**，覆盖**五条连接**：`7000`（登录）、`7100`×2（首进选服 + 软关后重选）、`7200`×2（两段游戏会话），首段游戏会话 **c2s 416 B / s2c 2483 B**，原始字节落 `E:/tmp/smoke/hook/conn-000{1..5}.{c2s,s2c}.bin`。
截图 **19 张**；`05b_after_notice.png` 非黑像素占比 **97.68%**（黑屏门阈值 1%）。

### 2b. ERR 面必须分窗说，不能只报一个「0」

| 面 | 实测 |
|---|---|
| 六个 `.err.log` | **0 字节（全部）** |
| `[ERR]` 在 `*.out.log` 中 | **共 4 条，全部落在关服窗口**：`game-svr` L467/469/470 `网关连接断开。`（14:16:18.844–845，SHUTDOWN 请求在 14:16:18.918）、`db-srv` L40 `账号服务器[127.0.0.1:5600]断开链接.`（22:16:19.160） |
| 功能期（登录→重进结束）`[ERR]`/`panic` | **0** |

> **这一条是本轮发现的第二类假绿**：执行车道报告「12 份日志 ERR|panic 计数全 0」，实际 `grep -cE '\[ERR\]|panic' logs/*.log` 是 `game-svr.out.log:3`、`db-srv.out.log:1`。**差异不在事实，在水位线的范围与正则**——只扫 `*.err.log`、或只扫功能期窗口，都会得到 0。结论仍然成立（4 条全是关服断链，非行为错误），但**「0」这个数字本身不可复用**，必须写清扫的是哪些文件、哪个时间窗。

### 2c. 本轮执行车道自己撞上的两类工具假绿（自报，已修）

| # | 形态 | 表现 | 修法 |
|---:|---|---|---|
| A | **glob 没通配 ⇒ 扫了 0 个文件** | 第一轮 ERR 水位用 `Get-ChildItem $LogDir -Include *.err.log,*.out.log`（缺 `\*`）⇒ 匹配 0 个文件，「0 ERR」成立但**什么也没看** | 先列目录再按后缀过滤；修后自检 12 个文件 |
| B | **返回值被输出污染** | `Test-NotBlackScreen` 用 `Write-Output` 返回黑屏比 ⇒ 调用方拿到的是 `System.Object[]`，`render=System.Object[]` 却仍走真分支 | 改用 `Write-Host`；判据另用 System.Drawing 独立复核 |

> 与 §2b 同类：**判据写得再对，取样空集也会报绿。** ⇒ 任何「0 个问题」的结论都必须附带「扫描了多少个对象」。

## 3. 一条顺带取到的真机观察：`characters.Dir` 写回的是**实时值**

| 时点 | 观测 |
|---|---|
| 载入（进世界） | `(272,654) Dir:6` —— 即存档里的旧值 |
| 第 1 次存档后重进 | `(273,656) **Dir:0**` |
| 第 2 次存档（`E:/tmp/smoke/aaa-after.txt`） | `CX=286 CY=649 **Dir=1**` |

载入值 6 → 两次落库分别是 0 和 1，**都不是载入值**，且与客户端最后朝向一致。
⇒ `openmir2-dir-persistence-writeback` 的修复（`574a49e7f`，离世前取实体快照 + 实体→快照→记录三档）在**真客户端路径**上生效。

**这不是该卡收口证据**，原因写清楚：① 只有 Rust 一侧，没有同场景的 C# 对拍；② 不是 `card_window` 口径（无 before/after 双库快照对）。
卡仍须按 §5 的窗口队列跑一次两侧比对。

## 4. 本轮**没有**覆盖的（缺口，不是失败）

1. **捡物 / 交易 / NPC 对话**：`tools/client/mir_flow.ps1` 只有 `-ClickNpc`，没有 pickup / trade 阶段；本轮未跑 NPC 对话。
2. **战斗有目标**：三次攻击发出去了（服务端收到走步/攻击命令），但**视野内没有怪** ⇒ **没有验证「怪物被打、掉血、死亡、尸体、挖肉」**。
3. **`chunks.ndjson` 的 `sha256` 字段全为空串**：原始 bin 在，但逐块内容指纹缺失 ⇒ 拿它做「字节未变」断言的人会假绿。
4. **`mir2_probe` 与客户端连的是同一个角色 `aaa`**，两次会话共用存档 ⇒ 快照前后差异同时包含「探针的一次小移动」与「客户端走路」；本轮结论不依赖位置差值的具体大小，但**谁移动了哪一段无法从快照单独区分**（日志可区分）。
5. 服务端 WRN 一条：L373 `消失帧：观察者 actor=290 无在线网关坐标 ⇒ 跳过 SM_DISAPPEAR(30)`——**未定性**，待查是否与 C# 同形。

## 5. 机器状态恢复（DoD）

| 项 | 结果 |
|---|---|
| Rust 栈 | 已停（`stop_mir2_server.ps1`，`关服完成：在册 0`） |
| C# 栈 | 六角色已重启；`port-status.ps1` 报 **十个端口全部在监听** |
| **mysqld** | **pid 2580，与开跑前完全相同 ⇒ 全程未动** |
| 客户端 | `MirClinet.exe` 已退出 |
| 仓库 | `git status` 干净 |

> C# 六角色 PID 与开跑前不同（重起所致）；比对基线是「十端口在监听 + mysqld pid 不变」。

## 6. 对「能不能发版」的回答

**能，但要把话说准**：

- **可以发的是「客户端与内容冻结条件下的可用性」**：冻结的 Delphi 客户端在 Rust 服务端上完成登录到重进的完整链路，客户端无报错，服务端无 ERR —— 这是最外层、也最容易被用户直观感知的判据。
- **不能把这次冒烟当行为等价的证据**：它没有逐帧/逐字段比对，也没有覆盖捡物、交易、NPC 对话、有目标战斗、DB 面差异。R 卡仍欠真机窗口（`docs/needs-human-20261025.md` §2），`window_required` 卡不得据此关闭。
- 三条已知缺口里，**第 3 条（hook `sha256` 空）是工具缺陷**，建议单独开卡修；**第 1、2 条是覆盖缺口**，属下一轮冒烟扩项。
