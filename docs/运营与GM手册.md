# OpenMir2 运营与 GM 手册

> 面向运营与 GM 的可执行手册。所有路径、命令、开关均与当前部署一致。
> 版本基线：`walgit/master = 1d2455448`，服务端 exe sha256 前缀 `a9bfcd4a…`。

## 1. 本机部署形态

| 项 | 值 |
| --- | --- |
| 服务端二进制 | `mir2-rs/target/release/mir2-server.exe`（**dispatcher**，不是 game-svr 单包产物） |
| 配置主文件 | `E:/MirServer/M2GameSvr/Server.conf`（GBK 字节流，**只能按字节改**） |
| 数据目录 | `E:/MirServer`（脚本 Envir、地图、配置） |
| 日志 | `E:/MirServer/logs/mir2-server/*.out.log`、`*.err.log`（**时间戳为 UTC**） |
| 数据库 | 本机 MariaDB/MySQL：`mir2_account`（账号）、`mir2_db`（角色）、`mir2_data`（静态模板） |
| 客户端（带坐骑/翅膀渲染） | `D:/MirClient-run/MirClientZig.exe` |

**正常健康态**：6 个 `mir2-server` 进程 + `mysqld` 存活 + 11 个端口全部 LISTEN。

### 端口表

| 端口 | 角色 |
| --- | --- |
| 5000 | DBSrv（存档/读档） |
| 5100 | LoginSrv（账号认证） |
| 5500 | LoginGate |
| 5600 | SelGate（选角） |
| 5700 | 内部 |
| 6000 | 内部 |
| 7000 | **LoginGate（客户端登录入口）** |
| 7100 | SelGate |
| 7200 / 7201 / 7202 | GameGate（游戏内流量） |

## 2. 启动 / 停止 / 重启

```powershell
# 停止
powershell -NoProfile -Command "& 'E:\Users\gxh\Documents\GitHub\OpenMir2\tools\m2-acceptance\stop_mir2_server.ps1'"

# 启动
powershell -NoProfile -Command "& 'E:\Users\gxh\Documents\GitHub\OpenMir2\tools\m2-acceptance\start_mir2_server.ps1'"
```

**启动顺序约束**：先 MySQL，再服务端（DBSrv → LoginSrv → GameSvr → 各 Gate），由起栈脚本统一负责；不要手工乱序起单个角色。

**自检**：
```powershell
Get-Process mir2-server | Measure-Object                       # 应为 6
Get-NetTCPConnection -State Listen | Where-Object {
  $_.OwningProcess -in (Get-Process mir2-server).Id
} | Select-Object -ExpandProperty LocalPort -Unique | Sort-Object
```

**改代码后的重建**：必须 `cd mir2-rs && cargo build --release -p mir2-server`。
只 `cargo build -p game-svr` **不会**更新部署用的 dispatcher exe。构建后**核对 exe 的 mtime 与 sha256** 再重启。

**真机窗口纪律**：真机跑批期间不要跑 cargo（并行构建会导致窗口失败）；先构建冻结、再开窗。

## 3. 客户端连接

- LoginGate：`127.0.0.1:7000`
- 服务器名：**热血传奇**
- 测试账号：`mir2test` / `mir2pass`，角色 `aaa`

**冻结 Delphi 客户端**（`mirbeta/MirClient`）可以连，但**坐骑/翅膀不会显示**：它把 `feature_ex` 低字节当武器特效并硬置 `m_btHorse := 0`。坐骑/翅膀渲染由 `MirClientZig` 承担。

无头验证（不需要桌面、不注入键鼠）：
```sh
python -I -B tools/probe/mir2_probe.py \
  --login-gate 127.0.0.1:7000 --server-name 热血传奇 \
  --account mir2test --password <口令> --char-name aaa \
  --scenario <场景.json> --out <输出目录>
```

## 4. 配置与开关

| 开关 | 位置 | 缺省 | 说明 |
| --- | --- | --- | --- |
| `LevelRankOnLogin` | `Server.conf` 的 `[Setup]` | **必须 0** | 置 1 会在登录批次多发 `SM_LEVELRANK(1108)`，破坏与 oracle 的逐字节基线；1060 请求也会被响应 |
| `AdminPort` | `Server.conf` 的 `[Admin]` | 0（不监听） | 只有要用 admin/GM 通道时才开（如 7946） |
| `AdminBind` | 同上 | `127.0.0.1` | **只接受回环地址**，禁止改 `0.0.0.0` |

**编辑 `Server.conf` 的硬要求**：文件含非 ASCII 且不是合法 UTF-8，**必须按字节插入/替换**。
按文本（GBK/UTF-8）整文件重写曾把文件截断成 0 字节，靠备份恢复。改前先备份：
`cp E:/MirServer/M2GameSvr/Server.conf E:/tmp/Server.conf.bak-<日期>`，改完核 sha256。

**新帧的基线影响**
- `SM_MOUNTRIDE(1118)`：**无开关**，但只在角色背包持有马牌（`StdMode=18 && Shape∈1..12`）时于登录批次与 AOI 出现帧下发。Delphi 客户端对未知 ident 走默认空支、无视觉影响；**逐字节对拍时请用背包无马牌的角色，或先回滚马牌迁移**。
- 翅膀（`feature_ex` 高字节）：仅当 Dress 槽装备 `AniCount != 0` 的衣服时非 0；无翅膀角色帧与基线逐字节一致。

## 5. 数据与迁移

迁移脚本统一放 `sql/migrations/`，要求**幂等 + 可回滚**。

当前生产已应用：
- `T-277-mount-plates.sql`：马牌 12 行（Id 1001..1012，StdMode 18，Shape 1..12）。
  备份 `E:/tmp/t277-prod-migration/stditems-backup-20261105-1135.sql`。

**翅膀与英雄排行不需要任何数据变更**：翅膀只读既有 `stditems.AniCount`；英雄排行本仓无英雄数据源，按设计发空表。

**关键纪律**：`stditems` 由 game-svr **启动时一次性加载**成 catalog。
**新增或修改物品模板后必须重启栈**，否则 `catalog.get(index)` 返回空，相关功能静默不生效（坐骑首次未生效即此原因，非实现缺陷）。

**备份 / 回滚**
```sh
# 备份单表
mysqldump -uroot --single-transaction mir2_data stditems > E:/tmp/stditems-backup.sql
# 回滚（脚本方式）
mysql -uroot mir2_data < sql/migrations/T-277-mount-plates-rollback.sql
# 或整表恢复
mysql -uroot mir2_data < E:/tmp/stditems-backup.sql
```
回滚后同样需要**重启栈**重载 catalog。

## 6. GM 怎么管理游戏

### 6.1 先拿权限（最容易踩的坑）

本项目 1:1 复刻 oracle：**角色默认 GM 权限 = 0**。此时直接发 `@OnHorse` 等命令只会收到
`ident=100` 的「权限不够」，**根本进不了功能分支**。

授权（写目标会话，取值 0..10）：
```
@VV给予权限 <目标角色名> <级别>
```
拿到权限后再用对应的 `@` 命令。

不想走权限门，用 **`GMEXECUTE` 脚本路径**——这是 oracle 原样可达的触发方式。
注意：**持牌自动骑乘不经过 `@OnHorse`**，因此那条路径没有权限门；`@OnHorse/@OffHorse` 只是手动切换。

### 6.2 服务端 GM 工具（mir2-mcp）

六个工具：`gm_list`（列出 GM 命令）、`gm_help`（单命令用法）、`gm_execute`（执行 GM 命令）、
`conf_keys`（配置键清单）、`conf_get`（读配置）、`conf_set`（改配置）。

- **默认 stdio**（不监听任何端口）：
  ```
  claude mcp add mir2 -- E:/.../mir2-rs/target/debug/mir2-mcp.exe --root E:/MirServer
  ```
- **可选 HTTP**（长驻/多客户端）：
  ```
  mir2-mcp.exe --root E:/MirServer --transport http --bind 127.0.0.1:7950 --token-file <token 文件>
  ```
  **只绑回环 + 必须 Bearer token**，缺 token 直接拒绝启动且不监听；token 不得进命令行/日志/文档。

`gm_execute` 依赖 game-svr 的 **admin 通道**：若 `Server.conf [Admin] AdminPort` 为 0（缺省不监听），
该工具不可用；`gm_list/gm_help/conf_*` 不依赖它。

`conf_set` 带**跨进程锁 + 备份 + 临时文件替换 + 回读校验 + 失败回滚**，但改生产配置仍请先手工备份。

### 6.3 安全边界

- admin 通道与 HTTP 传输**只允许回环**，禁止对外暴露；
- token 一律走文件；
- 生产改动（配置、迁移、二进制）一律「先备份 → 执行 → 回读核对 → 必要时重启 → 记台账」。

## 7. 验收与台账

- 真机验收记录统一追加到协作台账 `openmir2-real-machine-verification-master`。
- 本批已验收：翅膀（T-279）、坐骑（T-277，含同屏可见）、英雄排行（T-278）。
- 里程碑机判：`python -I -B tools/goal/goal_gate.py run --full`（四条件）。
- 合并前门禁：`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、
  `cargo test --workspace`、`cargo run -q -p msgcodegen -- --check` 与 `tools/hygiene/` 下各门。
