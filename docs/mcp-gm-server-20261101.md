# mir2-mcp：本地 MCP（rmcp/stdio）GM 目录、开区配置与 GM 执行（slice-1 + slice-2）

- 卡：`openmir2-mcp-gm-conf`（tier=L，slice-1）、`openmir2-mcp-gm-execute`（tier=R，slice-2，T-273）
- 基线：`walgit/master` `13112da07`（slice-1）→ `dcea680f9`（slice-2 起点）
- crate：`mir2-rs/bins/mir2-mcp`（package `mir2-mcp`，依赖 `rmcp = "3.8.0"`，stdio 传输）；
  slice-2 同时改 `mir2-rs/bins/game-svr/src/{config.rs,lib.rs}`（`[Admin]` 通道）

## 1. 一句话

`mir2-mcp` 把**GM 命令目录**（185 条，只读）与**开区配置**（`M2GameSvr/Server.conf`，只读 + 白名单写）
暴露成 6 个 MCP 工具，走**纯本地 stdio**（不监听任何端口）。`gm_execute` 在 slice-2 起经
**game-svr 本机 admin 通道**（`[Admin] AdminPort`，**缺省 0 = 关闭**）执行 GM 命令：命令解析/执行
完全在 game-svr（复用 `gmcmd::execute` + 既有 GM 收口），MCP 只负责投递与结构化透传。

## 2. 工具

| 工具 | 入参 | 行为 | 写面 |
|---|---|---|---|
| `gm_list` | `{}` | 输出 185 条命令目录：现网名、族、权限、参数个数、`implemented/unimplemented`、oracle 描述/帮助、效应/帧；条数断言 = `gm_registry.json` 的 `summary.table_total` | 无 |
| `gm_help` | `{name}` | 单条详情（现网名 `OrdinalIgnoreCase`）；registry 有该条时附 `verdict/verdict_why/script_evidence` | 无 |
| `conf_get` | `{section,key}` | 读 `Server.conf` 一个键（任意段/键，只读；`OrdinalIgnoreCase`） | 无 |
| `conf_keys` | `{}` | 白名单键清单：当前值、缺省、允许范围、语义、是否活键 | 无 |
| `conf_set` | `{section,key,value,dry_run?}` | 白名单写：备份 → 临时文件+rename 原子替换 → 回读校验；`dry_run=true` 零写 | `M2GameSvr/Server.conf` |
| `gm_execute` | `{chr,cmd}`（兼容 `name,params?`） | 经 game-svr 本机 admin 通道执行一条 `@` 命令，透传 `{ok,error,frames,note}`；通道未配置/连不上/超时/未知角色/未知命令 ⇒ 结构化错误。`name/params` 只在 `cmd` 缺失时合成 `@name params…`，**同一条执行路径** | 游戏侧状态（**不含**配置文件） |

成功回包是 MCP `structuredContent`（`{"ok":true,…}`）；拒绝/失败是 `isError=true` +
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

- **MCP 传输仍是纯本地 stdio**：不监听端口；唯一「外连」是 `gm_execute` 向
  **本机回环** admin 通道投一条命令（`MIR2_MCP_ADMIN_ADDR` 或 `127.0.0.1:<AdminPort>`）；
- 读：`<root>/M2GameSvr/Server.conf`、可选 `--registry` 指定的 registry JSON；
- 写：只有 `<root>/M2GameSvr/Server.conf` 的**白名单既有键**值区间；备份写到系统临时目录；
  `gm_execute` **不写配置文件**，它改变的是游戏世界状态（与客户端 `@` 命令同源）；
- 工具没有文件路径参数，不能借参数越出上述写面。

## 7. game-svr 本机 admin 通道（slice-2）

**配置**（`M2GameSvr/Server.conf`，新增 `[Admin]` 段；oracle 没有该段，按 `T-273` 登记）：

```ini
[Admin]
AdminPort=7946        ; 缺省 0 = 不监听（生产零行为差）
AdminBind=127.0.0.1   ; 缺省 127.0.0.1；只接受回环 IP 字面量
```

- **开窗式启用**：默认关闭。只有显式写非 0 `AdminPort` 才在回环地址监听；关闭状态下
  game-svr 的行为/帧面与 oracle 逐字节不变（`T-273` 的「默认关闭 ⇒ 生产零行为差」）。
- **非回环 = 拒绝启动**：`AdminBind` 只接受 `127.0.0.0/8` 或 `::1` 的 IP 字面量；`0.0.0.0`、
  局域网/公网 IP、主机名（含 `localhost`）都在**配置解析期**报错（负例测试见
  `config.rs::admin_section_defaults_off_and_rejects_non_loopback`）。accept 时再按
  `peer.ip().is_loopback()` 做一次纵深防御。
- **协议**：TCP 回环、**逐行 JSON**。
  - 请求：`{"chr":"<角色名>","cmd":"@..."}`（`cmd` 必须是 `@` 开头的完整命令行）；
  - 响应：`{"ok":bool,"error":?string,"frames":N,"note":?string}`；
  - `error` 形如 `"<code>: <detail>"`；`frames` = 客户端可见帧数（`SysMsg` + 未知命令回落 + 直发帧）；
  - `note` = 命令回帧文本（`@时间` 等无副作用命令在这里返回文本）。
- **执行路径**：`GateEvent::AdminGm` 投进 sim 线程（世界唯一写者），`execute_admin_gm_command`：
  1. 查 `ground_sessions[chr]`（不在线 ⇒ `unknown_character`）；
  2. 构造与 `CM_SAY` 直呼**逐字段同源**的 `GmWorld`（`pose_target`/`quests`/`caller_guild`/
     `speaker_dir`/全局 `filter_word`/`mission`/`spirit_mutiny_tick`/`m_host=true`）；
  3. **只对该次调用**把 `GmSession.permission` 抬到 10（等价脚本 `GMEXECUTE` 的 `PermissionGuard`），
     调 `gmcmd::execute` 后**立刻还原**（不写回会话持久权限）；
  4. 调 `command_event_tail_gm_drain` 与客户端/脚本**同一收口**落地 `pending_*`
     （刷怪/造物与世界同一条 RNG 流；203/709/41/… 帧与 `CM_SAY` 同序）；
  5. 回 `{ok,error,frames,note}`。
- **超时/失败**：连接建立 2s、读写 10s（MCP 侧）；sim 回包 5s、连接空闲 60s（game-svr 侧）。
  连不上/超时/响应不是约定 JSON ⇒ 结构化 `admin_connect_failed` / `admin_timeout` /
  `admin_protocol_error`；`AdminPort=0` 且未设 `MIR2_MCP_ADMIN_ADDR` ⇒ `admin_not_configured`。
- **不启动默认端口栈**：开发/测试用进程内 `127.0.0.1:0` 临时端口 + 既有 GM 测试夹具驱动
  同一执行收口；`E:\MirServer` 默认端口栈（5000/6000/7000/7100/7200…）只在真机窗口操作。

## 8. 能力矩阵（哪些 GM 命令离线/在线可达）

| 场景 | 可达性 | 说明 |
|---|---|---|
| `gm_list` / `gm_help`（185 条目录） | **离线可达** | 只读编译期目录 + registry，不需要 game-svr |
| `conf_get` / `conf_keys` / `conf_set` | **离线可达** | 只读/白名单写 `Server.conf`，不需要 game-svr；写后通常需重启生效 |
| `gm_execute`，`AdminPort=0`（默认） | **不可达**（`admin_not_configured`） | 生产默认即此状态 |
| `gm_execute`，通道已开 + 角色在线 | **在线可达** | 与客户端 `@` 命令同一权限口径；命令表 `PermissionMin` 只有 0（5 条）与 10（180 条）两档 ⇒ 抬到 10 后权限门全过，命令体自带的运行期门仍照 oracle |
| `gm_execute`，通道已开 + 角色**不在线** | **不可达**（`unknown_character`） | oracle 的命令对象必须是在线 `PlayObject`；本通道不伪造离线执行 |
| 有副作用/抽 RNG 的命令（`@刷怪`/`@清除怪物`/`@召唤`/`@怪`…） | **在线可达** | 走同一 `gmcmd::execute` + `drain_script_gm_pendings`；RNG 抽数序列与帧面的真机双栈对拍属 **R 级窗口项**（见 `T-273` 与卡 `openmir2-mcp-gm-execute` 的 `window`） |
| 未知命令 / 非 `@` 命令行 | **结构化拒绝** | `unknown_command` / `invalid_command`，不 panic、不静默 |

## 9. 安全边界

- admin 通道**只绑定回环**（配置层拒绝 + accept 层复核），不做网络暴露；
- 通道**无独立鉴权**：安全边界 = 「本机回环」+ 「默认关闭」+ 「显式开窗」。任何本机进程都能连
  `127.0.0.1:<AdminPort>`，因此**不要在生产常开**；需要时开窗、用完关闭（改 `AdminPort=0` 并重启）。
- 抬权只作用于**单次调用**（执行前 10 / 执行后立即还原），不写会话持久权限、不改 `AdminList` 判定；
- 不新增客户端可见帧面（除命令本身的效果）；不改客户端协议、DB schema、Envir 脚本格式；
- `T-273`：新增 `[Admin]` 管理面与 MCP 执行能力是 oracle 没有的能力，按「默认关闭 ⇒ 生产零行为差」
  登记（`mir2-rs/tests/parity/whitelist.md` / `T-RESERVATIONS.md`）。

## 10. 自测

```powershell
cd E:/Users/gxh/Documents/GitHub/OpenMir2/mir2-rs
cargo test -p mir2-mcp
cargo test -p game-svr --lib admin_channel_tests
cargo test -p game-svr --lib config::tests::admin_section_defaults_off_and_rejects_non_loopback
```

- **mir2-mcp 单元（27 条）**：目录 185 条 / 176 implemented / 9 unimplemented / perm0=5；目录条数 =
  registry `table_total`；`gm_help` 取到 oracle 描述（`Tingwa`→`TingCommand`、`清理垃圾`→`清理包裹物品`）；
  `conf_get` 大小写/注释/重复段；`conf_set` 往返（before/after sha、备份逐字节、回读、备份恢复 sha
  相等）；`dry_run` 零写；非白名单/非活键/非法值/缺键拒绝且零写；同值 no-op；无 `.tmp` 残留；
  CLI 参数/`--help`；`[Admin] AdminPort` 读侧（缺段/空值 ⇒ 0、显式端口、越界报错）；
  admin 客户端连不上/地址非法 ⇒ 结构化错误。
- **mir2-mcp 端到端（4 条，真起子进程走 stdio JSON-RPC）**：
  1. 主链路：`initialize` → `tools/list`（6 个工具）→ `gm_list`（`catalog_total=table_total=185`）→
     `conf_get` → `gm_help` → `conf_set dry_run`（零写）→ `conf_set` 真写（备份/回读 25）→
     `gm_execute`（未配置通道 ⇒ `admin_not_configured`）。
  2. root 解析：`--root` 压过 env `MIR2_MCP_ROOT`；无 `--root` 时读 env。
  3. `gm_execute` 经 `[Admin] AdminPort` 的**默认地址**投递到进程内假通道：请求 `{chr,cmd}` 原样、
     响应 `{ok,frames,note}` 原样透传。
  4. 失败面：缺 `chr` ⇒ `missing_chr`；非 `@` ⇒ `invalid_command`；通道连不上 ⇒ `admin_connect_failed`。
- **game-svr（6 条）**：`@时间` ⇒ `ok` + 文本 + 1 条 `SM_SYSMESSAGE(100)`；`@LoadAdmin`
  （`PermissionMin=10`）经抬权成功且**执行后 `permission` 仍为 0**；未知角色/未知命令/非 `@`
  ⇒ 结构化错误；`127.0.0.1:0` 传输往返（真实 `run_admin_listener` + 同一执行收口）。
- **配置负例**：`AdminBind=0.0.0.0 / 192.168.1.10 / localhost` ⇒ 拒绝启动。
- **真机窗口（R 级，合并后回登台账）**：① `@刷怪`/`@清除怪物`/`@召唤` 经 admin 通道与脚本
  `GMEXECUTE` 路径的**抽数序列与帧面**等价（需真机双栈对拍）；② `AdminPort` 开/关两种配置下
  客户端帧面无差异。
