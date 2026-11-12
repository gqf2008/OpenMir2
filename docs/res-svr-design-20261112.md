# res-svr 设计（资产补齐下载服务，2026-11-12）

> 卡 `openmir2-res-download-svc`（walgit 看板；tier R，`window_required=true`）。
> 目标来源：MirClientZig `docs/GOAL-20261112-AI-HEADLESS.md` §3（客户端缺库时自动下载补齐）。
> 实现位置：`mir2-rs/bins/res-svr`（lib + 瘦 bin）；起栈开关在 `tools/m2-acceptance/start_rust_stack.ps1`。

## 1. 目标与边界

* **独立进程**：`res-svr` 是独立 bin，默认只绑 `127.0.0.1:7300`（地址/端口可配置），
  **不并入 game-svr 进程**——下载面的任何故障不影响游戏世界宿主。
* **只读**：只读各资产根；不写盘、不改冻结客户端/C# oracle/sql/Envir。
* **配置不入库**：token 与资产根映射来自环境变量/本地配置文件（`E:\MirServer\ResSvr\config.conf`
  之类），**不进仓库、不进日志、不进 argv**。
* **不假设具体资产存在**：清单来自遍历实况。`objects256` 两侧都不存在是已知事实，
  服务只提供资产根里真实存在的文件，不硬编码任何「应该有」的条目。

## 2. 路由与状态码

| 请求 | 结果 |
| --- | --- |
| `GET/HEAD /res/index.json` | 200 + `{"files":[{"path","size","sha256"}]}`（按 path 排序） |
| `GET/HEAD /res/file/<path>` | 200 全量 / 206 单段 / 403 / 404 / 413 / 416 |
| 其它方法（POST/PUT/DELETE/…） | 405 + `Allow: GET, HEAD` |
| 其它路径 | 404 |

状态码语义（实现即判据）：

| 码 | 触发条件 |
| --- | --- |
| 403 | 缺/错 token；路径归一化拒绝；扩展名不在白名单；符号链越出映射根 |
| 404 | 未知映射前缀；文件不存在；目标不是普通文件；未知路由 |
| 405 | 方法不在白名单（鉴权先于方法闸门：无 token 的 POST 仍是 403） |
| 413 | 文件大小超过 `max_file_size` |
| 416 | Range 起点不小于文件大小（附 `Content-Range: bytes */<size>`） |
| 429 | 每 token 限流；或并发连接超限（直接回最小 429 并关闭，不排队） |
| 500 | 内部异常（清单任务 panic、打开文件非 NotFound 错误） |

未知前缀归 404 而非 403：请求的路径形状合法，只是没有对应的资产面。
扩展名拒绝归 403 而非 404：这是策略拒绝，与「文件是否存在」无关（下载面不因白名单而泄漏存在性判断）。

## 3. 鉴权与审计红线

* 所有请求必须带 `Authorization: Bearer <token>`；缺失/不符 ⇒ 403。scheme 大小写不敏感。
* **常量时间比较**：长度相等时逐字节异或累积、不按首个不等字节提前返回；长度不等直接 `false`
  （长度差会泄漏，这是业界通行取舍——token 长度不是秘密，内容才是）。行为用例覆盖
  「等长但逐字节不同」「长度不同」「非 Bearer scheme」三类。
* token 来源：`RES_SVR_TOKEN` > 配置文件 `[res-svr] token`。**没有 `--token` 参数**：
  token 绝不进进程命令行（同机其它用户可见 `Win32_Process` 命令行）。
* token 红线落地：`ResConfig` 的 `Debug` 手写为 `<redacted>(len=N)`；错误信息只报来源与规则；
  启动行/告警行只打长度；审计行见下。
* 审计日志（**只写 stderr**，一行一请求）：

  ```
  res-svr audit method=<M> path=<原始 path> status=<码> bytes=<Content-Length 或 -> dur_ms=<毫秒>
  ```

  绝不写任何请求头（`Authorization`、UA 都不写）；path 若意外包含 token 子串会被替换成
  `<redacted>`；超长 path 截断到 512 字符。e2e 用例在真实请求后回读 stderr，断言所有行
  （含启动行/跳过告警行）都不含 token 本体、也不含 `authorization` 字样。

## 4. 路径归一化（安全承重件）

入口：`/res/file/` 之后的**原始（仍百分号编码）**路径。

1. 百分号解码（`%2e%2e` ⇒ `..`；`%2f` ⇒ `/`）；解码失败/非 UTF-8 ⇒ 403。
2. 含任何控制字符（含 `NUL`、换行）⇒ 403。
3. `\` 视为分隔符（Windows 口径），统一成 `/` 后切分；**任何空组件 ⇒ 403**——这一条同时
   拦住绝对路径（`/res/file//etc/passwd`）、UNC（`//host/share`）、双斜杠与尾斜杠。
4. 每段（**含第一段**）逐段校验：非空、不是 `.`/`..`、不以点/空格结尾（Windows 会静默剥掉
   尾点/尾空格）、不含 `<>:"|?*`、不是 Windows 保留设备名（`CON`/`PRN`/`AUX`/`NUL`/`COM1..9`/
   `LPT1..9`/`CONIN$`/`CONOUT$`，`NUL.txt` 这类带扩展名形式同样拒绝）。盘符形态的 `C:` 首段在
   这一步就 403，而不是因为「前缀没命中」落到 404。
5. 第一段必须**大小写不敏感**命中某个映射前缀（`Data`/`Map`/`Wav`…）；命中不了 ⇒ 404。
6. 末段扩展名（小写化）必须命中白名单，否则 ⇒ 403。
7. 拼到映射根后 `canonicalize`（解析符号链接/8.3 短名），**必须仍以该根的 canonical 路径为
   前缀**，否则 ⇒ 403（`..` 逃逸与符号链逃逸在这里兜底）；`canonicalize` 失败 ⇒ 404。
8. 目标必须是普通文件（目录 ⇒ 404）。

**不变量**：任何 2xx 的下载响应，其真实路径必然以某个映射根的 canonical 前缀开头。

顺序是刻意的：`Data/../Data/a.wzl` 这类「逃出去又回来」的路径在**第 4 步**就 403，
不依赖第 7 步兜底（两处判据互为纵深；M2 变异钉的是第 4 步）。TOCTOU 说明见 §12。

## 5. 清单与哈希缓存

* 遍历每个映射根，按扩展白名单收集；对外路径为 `<前缀>/<相对路径>`（`/` 分隔），
  最终按 path 排序（同输入同输出）。
* **哈希缓存键 = 文件路径，值 = `(size, mtime) → sha256` + `computed_at`**：指纹变化或 `computed_at` 超出 `index_cache_ttl_ms`（默认 5000，0=每次重算）即重算并写回；相同指纹且 TTL 内直接复用。首次清单会全量哈希（3G+ 资产一次性成本），其后只对变化文件/TTL 过期条目重算。
  缓存只锁 map 存取，哈希计算在锁外（并发清单请求不会把整个服务串行化）。
* 遍历是**元数据级**操作（不读内容）；哈希只在缓存失效时发生。
* 符号链接：只有解析后仍在映射根内、且是普通文件的才纳入清单（与下载面包含检查同口径）；
  越根符号链接与无法解析的链接逐条记 stderr 后跳过；目录符号链接不跟随（避免环路）。
* 目录递归深度上限 32，超限记 stderr 并截断；不可读目录/条目、不可哈希文件逐条记 stderr
  （可观测性红线：丢弃必须留痕）。
* 清单不受 `max_file_size` 影响：列出全部白名单文件，超限只在下载面回 413。

## 6. Range 语义

| 请求形态 | 语义 |
| --- | --- |
| `bytes=a-b` | 206；`Content-Range: bytes a-b/size`；`b` 越界饱和到 `size-1` |
| `bytes=a-` | 206；到文件尾 |
| 起点 `a >= size` | 416 + `Content-Range: bytes */size` |
| 多段 `a-b,c-d` | **忽略**（回 200 全量；不实现 multipart/byteranges） |
| 后缀 `-N` | **忽略**（回 200 全量） |
| 语法非法 / `a > b` / 非 `bytes` 单位 | 忽略（回 200 全量） |

`Content-Length` 一律显式写出（GET/HEAD/Range/错误体都是）；GET 流式体按 64 KiB/块读取
（满足「≤1 MiB/块」上限），块数×块长严格等于声明的长度。HEAD 与 GET 同头、无体。

## 7. 限额（默认收紧）

| 配置项 | 默认 | 行为 |
| --- | --- | --- |
| `max_file_size` | 1 GiB（1073741824） | 超过 ⇒ 413 |
| `max_connections` | 8 | 超过 ⇒ 新连接直接回最小 429 并关闭（不排队） |
| `rate_limit_rps` / `rate_limit_burst` | 20 / 40 | 每 token 令牌桶；超限 ⇒ 429 + `Retry-After: 1` |
| `header_timeout_ms` | 15000 | 请求头读取超时（hyper `header_read_timeout`） |
| `idle_timeout_ms` | 30000 | 已建连接读/写/flush 连续无进展超时；超时释放并发名额（0 非法） |
| `index_cache_ttl_ms` | 5000 | index sha 缓存 TTL；0 = 每次清单都重算；超 TTL 必重算 |
| 流式块 | 64 KiB（代码常量） | ≤1 MiB/块要求 |

并发限额按**连接**（keep-alive 连接在关闭前一直占一个名额）；这是防慢速占坑的取舍。
令牌桶当前按「每 token 一桶」建模；当前部署只有一个 token ⇒ 等价每服务一桶，桶内不存 token 值。
限流在鉴权之后：未鉴权请求不值得消耗 token 的配额，也避免未认证洪泛把合法客户端挤掉。

## 8. 配置项与启停

优先级（高 → 低）：

| 项 | 来源链 |
| --- | --- |
| 配置文件路径 | `--conf` > `RES_SVR_CONF` > 可执行文件同目录 `config.conf`（存在才用） |
| 监听地址 | `--bind` > `RES_SVR_BIND` > `[res-svr] bind` > `127.0.0.1:7300` |
| token | `RES_SVR_TOKEN` > `[res-svr] token`（**无 CLI 参数**） |
| 资产根 | `RES_SVR_MAP_<名称>` > `[map]` > 内置默认 `Data`/`Map`/`Wav` 三根 |
| 限额/白名单 | `[res-svr]` / `[ext]` > 内置默认 |

配置文件样例（token 用占位符；真实文件放 `E:\MirServer\ResSvr\config.conf`，不进仓库）：

```ini
[res-svr]
bind=127.0.0.1:7300
token=<token>
max_file_size=1073741824
max_connections=8
rate_limit_rps=20
rate_limit_burst=40
header_timeout_ms=15000
idle_timeout_ms=30000
index_cache_ttl_ms=5000

[map]
Data=D:/MirClient-run/Data
Map=E:/MirServer/M2GameSvr/Map
Wav=D:/MirClient-run/Wav

[ext]
whitelist=.wzl,.wzx,.wav,.mp3,.map,.mex,.wil,.wick
```

* 配置文件按 .NET `StreamReader` 口径解码（BOM 探测 + GB2312 回退，复用 `mir2-encoding` 的
  `decode_config_text`）；`[section]` 名大小写不敏感；`;`/`#` 行注释与值后 `;` 行内注释；
  `[map]` 的键**保留原拼写**（它就是对外前缀名）。
* 硬失败（rc=2，不起服务）：token 缺失/空/含空白或非可打印 ASCII/超长；bind 解析失败；
  映射根不存在或不是目录/canonicalize 失败；映射名重复（大小写不敏感）；白名单为空或含非法项；
  限额非正数。
* token 长度低于 16 只**告警**（告警行只打长度），不拒绝——默认回环绑定时不宜再制造部署摩擦；
  跨机部署请用 ≥32 字节随机值。

起栈开关 `start_rust_stack.ps1 -ResSvr`（默认**不**起）：`-ResSvr` 时额外起 `res-svr.exe`
（conf 默认 `<MirServer>\ResSvr\config.conf`，可用 `-ResSvrExe`/`-ResSvrConf` 覆盖；`-ResSvrPort`
只用于端口占用检查/就绪等待，真实 bind 以 conf 为准），pid 进同一 pid 文件，`-Stop` 统一回收。
未给 `-ResSvr` 时端口检查、前置检查、`-EmitPlan` 计划与启动序列**逐字节不变**
（默认 EmitPlan 输出与基线 diff 为空，证据见 patch body）。

## 9. 日志

* 只写 stderr；stdout 保持干净（二进制没有 stdout 输出）。
* 启动行：监听地址、并发/限流/单文件上限（**不含 token**）。
* 审计行：见 §3；每次请求一行（含 403/404/405/413/416/429）。
* 跳过/截断/拒绝必须留痕：不可读目录、不可哈希文件、越根符号链接、路径拒绝理由（静态文案）。
* 并发超限的直接 429 也写一行固定形状的审计行。

## 10. 依赖

全部复用 workspace `Cargo.lock` 已锁定的版本（`cargo build -p res-svr` 后新增锁条目为零）：

| crate | 锁内版本 | 用途 |
| --- | --- | --- |
| `tokio` | 1.53.2 | 运行时/监听/文件 IO |
| `hyper` | 1.12.0 | HTTP/1.1 服务端（与 `bins/mir2-mcp` 同口径，不引入 axum） |
| `hyper-util` | 0.1.21 | `TokioIo` / `TokioTimer` |
| `http-body-util` | 0.1.5 | `Full` / `StreamBody` / `BoxBody` |
| `futures-util` | 0.3.34 | `stream::unfold` 做有界流式读取 |
| `sha2` | 0.10.9 | 清单 `sha256` |
| `hex` | 0.4.3 | 十六进制编码 |
| `serde_json` | 1.0.151 | 清单 JSON |
| `mir2-encoding` | 本仓 path | 配置文本解码（BOM + GB2312 回退） |
| `percent-encoding` | 2.3.2 | **唯一新增直接依赖**；此前已作为 `url` 2.5.8 的传递依赖在锁内，无新增供应链面 |

## 11. 测试与牙

单测（30 条，`cargo test -p res-svr --lib`）：`auth`（等长逐字节不同/长度不同/scheme）、
`range`（解析矩阵/饱和/416/空文件）、`pathmap`（穿越/绝对/UNC/盘符/尾点尾空格/NUL/保留名/ADS/
`%2e%2e`/`%2f` 分隔符/符号链逃逸）、`index`（排序/sha/缓存失效/越根符号链不入清单）、
`ratelimit`（突发/回填/容量封顶，注入时刻不依赖墙钟）、`config`（优先级/默认值/非法值 fail-closed/
Debug 不含 token/Sources 解析/非 Unicode env）、`http`（审计脱敏/错误响应头/空闲超时包装）。

e2e（13 条，`cargo test -p res-svr --test res_svr_e2e`，真起子进程 + std TcpStream 手写 HTTP）：
鉴权矩阵、方法闸门、穿越矩阵（含 `Data/../Data/a.wzl`「逃出去又回来」与中间组件尾点/尾空格）、
重复 Authorization 首值优先、下载逐字节一致、HEAD、Range 六形态、清单 + 内容变更后缓存失效、
TTL 端到端、413、限流 429、并发 429 与名额恢复、空闲超时释放名额、审计日志不含 token。

变异牙（在交付 head 重放并恢复，证据见 patch body）：

* **M1 删 Bearer 校验**（请求路径上的 `auth::authorized` 判断）⇒ 无 token 用例必须红；
* **M2 放行 `..`**（删 pathmap 第 4 步的 `..` 组件拒绝）⇒ `Data/../Data/a.wzl` 用例必须红
  （该路径 canonical 后仍在根内，只有组件层判据能拦住它——这就是该用例存在的理由）；
* **M3 删限流中间件**（请求路径上的 `limiter.allow()` 判断）⇒ 429 用例必须红。
* **M4 只校验末段/跳过中间组件尾点** ⇒ `Data/sub%20/b.wzx` 用例必须红；**M5 TTL 失效** ⇒
  同尺寸同 mtime 重写用例必须红（对应 `ressvr-hardening` 卡）。

## 12. 已知取舍与风险

* **无 TLS**：默认只绑回环。跨机部署必须置于内网或反向代理之后（token 明文过网的风险由部署方承担）。
* **Range 后缀段/多段忽略**：按 HTTP 语义回 200 全量，客户端（断点续传）需用 `a-`/`a-b` 形态。
* **TOCTOU**：`canonicalize` 校验与后续 `open` 之间资产根内容可能被换。资产根由运维控制、
  非攻击者可控输入；如未来资产根可被低权限进程写，需要在打开后按文件句柄复核。
  （Windows 上不存在 `openat` 式 API 的简单替代，暂以「资产根可信」为前提并在此登记。）
* **保留名/非法字符是黑名单近似**：任何未列出的怪异名字会被文件系统自身拒绝（404/500），
  方向只会更严（fail-closed），不会更松。
* **符号链接用例**在无 `SeCreateSymbolicLinkPrivilege` 的机器上会跳过并打印原因；containment
  判据本身由单测（根外链接）与代码路径保证。
* **清单首次全量哈希**是 3G+ 的一次性成本（其后按 `(size, mtime)` + TTL 缓存）；若资产规模继续增长，
  可加持久化缓存文件（当前不做，避免引入新的写入面）。TTL 过期时全量重算的成本在 3G+ 真资产上
  未量化，可通过调大 `index_cache_ttl_ms` 换陈旧上界。
* **A7 + T3 真机窗口已通过（2026-11-12 22:55:34–22:59:04 +0800）**：403/200/404/穿越 403、
  Range 206、HEAD 同头零 body；客户端缺库 prguse 下载 + sha256 校验 + 原子落盘通过；审计
  token 出现 0。证据见卡 `openmir2-res-download-svc` comment。
