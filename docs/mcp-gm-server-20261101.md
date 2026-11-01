# mir2-mcp：本地 MCP（rmcp/stdio）GM 目录与开区配置服务（slice-1）

- 卡：`openmir2-mcp-gm-conf`（tier=L）
- 基线：`walgit/master` `13112da07`
- 新增 crate：`mir2-rs/bins/mir2-mcp`（package `mir2-mcp`，依赖 `rmcp = "3.8.0"`，stdio 传输）
- 写集：`mir2-rs/Cargo.toml`、`mir2-rs/bins/mir2-mcp/**`、本文件（`mir2-rs/Cargo.lock` 因新增依赖随之更新）

## 1. 一句话

`mir2-mcp` 把**GM 命令目录**（185 条，只读）与**开区配置**（`M2GameSvr/Server.conf`，只读 + 白名单写）
暴露成 6 个 MCP 工具，走**纯本地 stdio**（不监听任何端口）。GM 命令的**执行**不在本片：
`gm_execute` 一律返回结构化 `not_wired`，不伪造执行。

## 2. 工具

| 工具 | 入参 | 行为 | 写面 |
|---|---|---|---|
| `gm_list` | `{}` | 输出 185 条命令目录：现网名、族、权限、参数个数、`implemented/unimplemented`、oracle 描述/帮助、效应/帧；条数断言 = `gm_registry.json` 的 `summary.table_total` | 无 |
| `gm_help` | `{name}` | 单条详情（现网名 `OrdinalIgnoreCase`）；registry 有该条时附 `verdict/verdict_why/script_evidence` | 无 |
| `conf_get` | `{section,key}` | 读 `Server.conf` 一个键（任意段/键，只读；`OrdinalIgnoreCase`） | 无 |
| `conf_keys` | `{}` | 白名单键清单：当前值、缺省、允许范围、语义、是否活键 | 无 |
| `conf_set` | `{section,key,value,dry_run?}` | 白名单写：备份 → 临时文件+rename 原子替换 → 回读校验；`dry_run=true` 零写 | `M2GameSvr/Server.conf` |
| `gm_execute` | `{name,params?}` | **本片不接线**：返回结构化 `not_wired`（需要 game-svr admin 通道，留 slice-2） | 无 |

成功回包是 MCP `structuredContent`（`{"ok":true,…}`）；拒绝/未接线是 `isError=true` +
`{"ok":false,"code":…,"wrote":false,…}`。所有校验失败都返回结构化错误，**不 panic**。

## 3. 启动

先构建：

```powershell
cd E:/Users/gxh/Documents/GitHub/OpenMir2/mir2-rs
cargo build -p mir2-mcp
```

注册到 Claude Code（路径换成实际 worktree/检出里的 exe）：

```powershell
claude mcp add mir2 -- E:/Users/gxh/Documents/GitHub/OpenMir2/mir2-rs/target/debug/mir2-mcp.exe --root E:/MirServer
```

`--help` / `--version` 不启动传输，直接打印退出。

### root 解析

优先级：`--root <dir>` > env `MIR2_MCP_ROOT` > `E:/MirServer`。
配置文件固定为 `<root>/M2GameSvr/Server.conf`；工具**没有** path 参数，因此不存在写其它文件的入口。

### `--registry <gm_registry.json>`（可选）

`gm_list` 默认用**编译期嵌入**的 `mir2-rs/tests/parity/coverage/gm_registry.json`（二进制可脱离仓库运行）；
传 `--registry` 则每次调用实读该文件（返回里 `source` 区分为 `file:` 与 `embedded:`）。

## 4. gm_list 的数据来源与条数断言

`gm_registry.json` 的 `commands` 数组**只收 `Imp::Unimplemented` 的 9 条**
（`gm_registry.py` 的 rows 循环显式跳过已实现项），全量 185 条只在 `summary.table_total`
与 `crosscheck.model_total` 里。因此目录由三份源数据交叉合成（编译期 `include_str!` 嵌入）：

| 字段 | 来源 |
|---|---|
| 现网名 / 权限 / 参数个数 / 实现态 | `mir2-rs/bins/game-svr/src/gmcmd.rs` 的 `COMMANDS`（185 行） |
| 族 / 代码类 / 出厂名 / 效应 / 主帧 | `mir2-rs/tests/parity/m3/design-gm-commands-20261023.md` §4.3（185 行） |
| 中文描述 / 帮助 | `src/Modules/GameCommand/**/*.cs` 的 `[Command]` 属性（build.rs 提取到 `OUT_DIR`） |
| 总条数 / 违例明细 | `gm_registry.json` |

加载时强校验：gmcmd 表 185 条 ⇔ 设计件 §4.3 185 行（名字集合相等）、每条都有 C# 属性；
调用 `gm_list` 时再断言 `catalog_total == summary.table_total`，不等则返回
`catalog_count_mismatch`（不输出假目录）。

## 5. 开区配置白名单

写面只认 `[Setup]` 下这三个键（`section`/`key` 均 `OrdinalIgnoreCase`）：

| 键 | 活键 | 缺省 | 允许范围 | 语义（改动何时生效） |
|---|---|---|---|---|
| `GenMonRate` | 是 | 10 | 1..=1000 | 刷怪目标数除数：`目标数 = max(1, round(max(1,Count)/(rate/10)))`，值越大刷得越少；**启动装载时生效** |
| `MonsterPowerRate` | 是 | 10 | 1..=1000 | 怪物模板能力缩放（HP/MP/AC/MAC/DC/DCMAX/MC/SC × rate/10；`SabukWall`/`SabukDoor` 的 HP 例外不乘）；**装载怪物库时生效，重启才变** |
| `ExpRate` | **否** | — | — | **oracle 没有这个键**：`Server.conf` 无 `ExpRate`，全服经验倍率不存在；每玩家倍率走脚本 `KILLMONEXPRATE`（`docs/gm-operations-guide-20261031.md` §4.4）。本工具列出但**拒绝写入**（写进去也没有任何代码读取） |

> **`ExpRate` 偏差说明（卡面白名单写了它，代码按 oracle 真相处理）**：卡面要求白名单「至少包含
> GenMonRate、ExpRate、MonsterPowerRate」。oracle 事实是没有 `ExpRate` 键（部署文件里只有
> `ItemExpRate`/`LimitExpRate*` 等死键，两端源码都不消费）。按「行为等价优先于修 bug」「不伪造」
> 原则，`conf_keys` 仍列出该键并给出真相，`conf_set` 对它返回 `not_live_key` 且**零写**。
> 若确需一个运行期经验倍率，应开独立卡（脚本通道或 D-* 偏差），不在本片私造配置键。

### conf_set 写流水（成功路径）

1. 白名单 / 活键 / 值域校验（任一不过 ⇒ 结构化错误，**零写**）；
2. `before_sha` = 写前文件 sha256；
3. 备份原文件到系统临时目录 `%TEMP%/mir2-mcp-backups/Server.conf.<ms>.<pid>.<n>.bak`，
   **路径随回包返回**（`backup_path`）；
4. 同目录临时文件 `.<name>.mir2-mcp.<pid>.<ms>.<n>.tmp` + `rename` 原子替换（Windows
   `MoveFileEx(REPLACE_EXISTING)`）；
5. 写后回读：文件 sha 与目标值都要一致；不一致则用备份回滚并返回 `readback_mismatch`。
6. 回包：`before_sha` / `after_sha` / `backup_path` / `readback_value` / 变更前后整行。

`dry_run=true`：只返回 `before_sha`、预览 `after_sha`、变更前后整行；**不建备份、不落盘**
（调用前后目录内容 sha 相等）。

备份保留期：本片**不做自动清理**（`%TEMP%/mir2-mcp-backups/` 由调用方 / 系统临时目录策略负责；
每次真实写入都会留一份 `.bak`）。如需保留期/轮转策略另开卡（F3 登记不改）。

### 拒绝码（`code` 字段，全部零写）

| code | 触发 |
|---|---|
| `not_whitelisted` | 非白名单键 / 非 `[Setup]` 段 |
| `not_live_key` | `ExpRate`（活键=false） |
| `invalid_value` | 非十进制整数 / 越界（`GenMonRate`、`MonsterPowerRate` = 1..=1000） |
| `conf_missing` | `<root>/M2GameSvr/Server.conf` 不存在 |
| `key_absent` | 文件里没有该生效键（**只改既有键，绝不插入新键**） |
| `unsupported_encoding` | UTF-16 BOM |
| `io_error` | 读/写/rename 失败 |
| `readback_mismatch` | 写后回读不符（已尝试回滚） |

### ini 读侧口径（与 oracle `ConfigFile.Load` 一致）

- 段名/键名 `OrdinalIgnoreCase`（C# 内外两层字典都传了 `StringComparer.OrdinalIgnoreCase`）；
- 整行 `Trim`；`;` 整行注释；`/* … */` 段注释；行内 `;;` 之后截断为注释；
- 键按**第一个** `=` 切分，键值各自 `Trim`；
- 同段同名键**首个生效**；**重复段名 ⇒ 整个加载中断**（C# `goto Label_02AE`），其后的键视为未装载。

写侧**不用** `ConfigFile.Save()`：oracle 的 Save 会按内存字典整文件重写（丢注释、重排），
本工具只在原始字节上替换目标行的**值区间**，其余字节（BOM/CRLF/注释/顺序）逐字节保留。

- `conf_get` 的**展示层**对非 ASCII 值按 UTF-8 lossy 解码（只影响显示）；`conf_set` 的写路径在原始
  字节上替换值区间，**逐字节保留**（含 GBK/BOM/CRLF）——F4 登记不改。

## 6. 权限面

- **纯本地 stdio**：不监听端口、不发起网络连接、不读环境外数据；
- 读：只有 `<root>/M2GameSvr/Server.conf` 与（可选 `--registry` 指定的）registry JSON；
- 写：只有 `<root>/M2GameSvr/Server.conf` 的**白名单既有键**值区间；备份写到系统临时目录；
- `gm_execute` 不执行任何游戏侧动作，只回 `not_wired`；
- 工具没有文件路径参数，不能借参数越出上述写面。

## 7. slice-2（未做）

`gm_execute` 需要 game-svr 的 admin 通道（把命令送进运行中的 GameSvr 并回帧/结果）。
本片边界：

- 不接 game-svr 代码、不监听 socket、不伪造执行结果；
- 待 slice-2 卡定义 admin 通道（鉴权、命令白名单、超时、审计）后再接线；
- 在此之前 `gm_execute` 固定返回 `{"status":"not_wired","slice":"slice-2"}`（`isError=true`）。

## 8. 自测

```powershell
cd E:/Users/gxh/Documents/GitHub/OpenMir2/mir2-rs
cargo test -p mir2-mcp
```

- 单元（21 条）：目录 185 条 / 176 implemented / 9 unimplemented / perm0=5；目录条数 = registry
  `table_total`；`gm_help` 取到 oracle 描述（`Tingwa`→`TingCommand`、`清理垃圾`→`清理包裹物品`）；
  `conf_get` 大小写/注释/重复段；`conf_set` 往返（before/after sha、备份逐字节、回读、备份恢复 sha
  相等）；`dry_run` 零写（目录 sha 前后相等）；非白名单/非活键/非法值/缺键拒绝且零写；
  同值 no-op；无 `.tmp` 残留；CLI 参数/`--help`。
- 端到端（2 条，真起子进程走 stdio JSON-RPC）：
  1. 主链路：`initialize` → `notifications/initialized` → `tools/list`（6 个工具）→
     `gm_list`（`catalog_total=table_total=185`、`commands.len()=185`）→ `conf_get`（tempdir 假 root）→
     `gm_help` → `conf_set dry_run`（零写）→ `conf_set` 真写（备份存在且等于写前内容、回读 25）→
     `gm_execute`（`isError=true` + `status=not_wired`）。
  2. root 解析：`--root` 压过 env `MIR2_MCP_ROOT`；无 `--root` 时读 env。
