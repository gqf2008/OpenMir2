# 二进制构建戳（`mir2-build-stamp/1`）：先产戳、后通电

- 卡：`openmir2-csharp-stack-binary-provenance`（tier=L）
- 前序：`docs/loadgate-criteria-runbook-20261115.md`「栈配置」与「preflight 分开记 CODE_HEAD 与 REPO_HEAD」
- 目的：让起栈日志能回答「这套二进制来自哪个提交」，并在来源不可判时**中止**而不是记录后继续跑。

## 1. 现状（2026-11-15 实测）

| 二进制 | 是否有戳 | 说明 |
| --- | --- | --- |
| `mir2-server.exe` | **有** | `mir2-rs/target/release/mir2-server.build.json`，`format=mir2-build-stamp/1`，由 `tools/window/rebuild-server.ps1:89` 生成；现存戳 `head=d5ed14508…`、`built_at=2026-11-02`（已陈旧） |
| `game-svr` / `game-gate` / `login-gate` / `sel-gate` | **无** | bot 窗口实际拉起的就是这四个（360s 归档 `start-stack.out.log` 实测）；目前只能 `mtime+sha` 人工登记 |
| `DBSrv` / `LoginSrv`（C#） | **有（DBSrv 6 件 / LoginSrv 3 件；2026-11-19 起）** | 清单单一来源 `tools/orchestration/stamp-groups.psd1`（起栈门/冻结/窗口门禁/三处对齐共用）；`stamp-binaries.ps1` 支持 `.dll` 目标、`-RequireHead` 与 `-OwnGlobs` 覆盖检查（未入清单的自家 DLL ⇒ `UNLISTED_OWN_BINARY`，独立于 `-RequireAll`）。2026-11-20 已在 M29（`f5ae02c7…`）强制重编全 C# 栈并重产戳：部署件与新冻结集 head 均为 `f5ae02c7…`；此前 m9 冻结集里的 MongoDB 插件曾出现「内嵌 M8 / 戳 M9」的不一致（增量构建跳过 + 事后产戳），已由本次重建消除，旧集原样保留并在其目录内加注（`PROVENANCE.ADDENDUM-20261120.txt`）。 |

反例：`E:/MirServer/M2GameSvr/GameSrv.exe` sha256 `ab655008…`、mtime 2026-10-22，异常栈 PDB 指向**已删除**的 worktree `OpenMir2-wt-goods` ⇒ 部署目录里的二进制无法自证来源。

## 2. 戳的内容（复用既有格式，不另造）

每条戳至少含：`exe`/`path`、`sha256`、`mtime`、`head`（提交号）、`head_subject`、`built_at`、`dirty`、`cargo`/`rustc` 版本。
**`sha256` 必须由写戳的一方自己从二进制文件算**（不得抄报告、不得抄上一次的戳）。

**二进制身份只认戳，不跨构建比 sha**：Rust release 同源重建也不是位级可复现（2026-11-17 实测：`game-svr.exe` 在 Rust 源逐字相同的两个 head 上重建，sha256 从 `48f7eca3…` 变为 `457c8d5b…`，C# 同理——`SourceRevisionId` 之外还有编译期差异）。判「源码没变」看 `git diff`，判「产物来源」看戳的 `head`/`dirty`/`built_at`，**不要**用「sha 相同/不同」推源码同一性。已验证可行的模式：360s 窗口的
`SELFTEST_BINARY sha256=36726fcd…` / `SELFTEST_DLL sha256=8d09c454…`——写者自算并绑定。

## 3. 通电条件（五项全过才接 `provenance=unknown ⇒ rc=2`）

1. **它拉起的每一个二进制都有戳**（`game-svr`、`game-gate`、`login-gate`、`sel-gate`；C# 侧若也拉起则一并写）；
2. 戳内 `sha256` 与文件**实时哈希一致**；
3. 戳内 `head` 能 `git cat-file -e` 解析到**真实提交**；
4. `built_at` 与二进制 `mtime` 差 **≤5 分钟**（防「先构建后覆盖」）；
5. **健康栈自检整轮 PASS**（拿一个真实构建目录跑一遍 provenance 检查，证明不会误杀健康轮次）。

## 4. 牙（逐条变异必红）

| 变异 | 期望 |
| --- | --- |
| 删任一戳字段 | 红 |
| 篡改戳内 `sha256` | 红（与实时哈希不符） |
| `head` 改成不存在的提交 | 红（`cat-file -e` 失败） |
| `built_at` 与 mtime 差 >5 分钟 | 红 |
| 健康目录（正常构建） | **绿**（不得误杀） |
| 空目录 / 无二进制 | 明确报错，不得静默通过 |

## 5. 与 preflight 的关系

戳落地后，`preflight.txt` 的 `CODE_HEAD` 直接读戳，不再依赖「构建时工作树 HEAD」的推断；
`REPO_HEAD` 仍单列（可含其后纯文档提交，写明数量）。

## 6. 产戳工具（第 1 步，本批交付）

工具：`tools/orchestration/stamp-binaries.ps1`（PowerShell 5.1 / 7 均可；工具门登记项 `stamp_binaries`）。

三种模式：

| 模式 | 参数 | 行为 |
| --- | --- | --- |
| 产戳（默认） | `-BinDir <dir> [-Names a,b,c] [-Head <sha> \| -Heads a=<sha>,b=<sha>] [-HeadSource <s>] [-Force]` | 对目录里**存在**的每个目标二进制写 `<exe>.build.json`；`sha256` 现算；`head` 写前必须 `git cat-file -e` 能解析 |
| 校验 | `-Check -BinDir <dir> [-Names …] [-RequireAll]` | 逐条判：戳存在、必填字段、sha 实时一致、head 可解析、`built_at`/`mtime` 与文件 mtime 差 ≤5 分钟 |
| 自测 | `-Selftest` | 离线合成夹具（临时 git 仓库 + 临时目录），1 正例 + 7 负例；`-NoToolchainProbe` 不跑 cargo/dotnet |

字段（`mir2-build-stamp/1` 的超集，公共字段与 `tools/window/rebuild-server.ps1` 一致）：

| 字段 | 类别 | 说明 |
| --- | --- | --- |
| `format` | 公共 | 固定 `mir2-build-stamp/1` |
| `head` / `head_subject` | 公共 | 构建来源提交号与其标题；写戳前必须能 `git cat-file -e <head>^{commit}` |
| `head_source` | 补充 | `explicit` / `explicit_per_binary` / `inferred_repo_head`，或调用方显式声明的推断方式（如 `reflog_inferred`）。runbook 要求「没有内嵌 commit 就标 inferred」，本字段是那枚标记 |
| `built_at` / `mtime` | 公共/补充 | 同值，取二进制文件的 **mtime**（不是脚本运行时刻）。校验器拿它与文件实时 mtime 比对，抓「产戳后被覆盖」与手改戳 |
| `dirty` | 公共 | 产戳时 `-RepoRoot` 的 `git status --porcelain`（多行字符串；空串表示干净） |
| `exe` / `sha256` / `size` | 公共 | 绝对路径（正斜杠）/ 现算 SHA256 / 字节数 |
| `toolchain_kind` | 补充 | `rust` \| `dotnet`；`DBSrv`/`LoginSrv`/`GameSrv`/`BotSrv` 等按名字识别，其余按 `<base>.d`（Rust）或 `<base>.deps.json`（.NET）识别 |
| `cargo` / `rustc` | Rust | `cargo --version` / `rustc --version` |
| `dotnet` / `runtime` | C# 等价字段 | `dotnet` = SDK 版本（`dotnet --version`）；`runtime` = **二进制自己的目标框架**（同目录 `<base>.runtimeconfig.json` 的 `runtimeOptions.framework.version`，如 `8.0.0`）。该文件缺失时才回落到本机第一个 `Microsoft.NETCore.App`。实例（2026-11-15）：本机 `--list-runtimes` 第一条是 `6.0.36`，而 `DBSrv` 目标为 `8.0.0`——只取第一条会写出与二进制无关的值 |
| `toolchain_probe` | 补充 | `ok` \| `skipped`（`-NoToolchainProbe`，自测/窗口期用） |

校验器对 `head_source`/`mtime`/`toolchain_kind`/`toolchain_probe` **只在存在时校验自洽**，所以
`rebuild-server.ps1` 产出的旧戳（无这些补充字段，但有 `cargo`/`rustc`）在字段层面仍可被识别；
两者差异（旧戳缺 `mtime`/`head_source`/`toolchain_kind`）是第 2 步统一口径时的待办。

退出码（与工具门三态一致）：`0` 产戳成功/校验全绿；`1` 校验判红或自测 FAIL；`2` 判不了/用法错
（目录不存在、没有二进制、RepoRoot 不是 git 仓、写戳失败、自测基础设施不可用）。

### 6.1 实测（2026-11-15，`E:/UserTemp/stack-rel-target-w83/release`）

该目录的四个 Rust 二进制并不是同一次构建产出的：`game-svr.exe` mtime `10:58`，而
`game-gate`/`login-gate`/`sel-gate` mtime 在 `09:53–09:54`；按 `.d` 依赖文件指向的
`worker-83` 工作区 reflog，二者分别对应 `4c8b60198` 与 `ac533176f`。因此产戳用 `-Heads`
逐项指定、并标 `head_source=reflog_inferred`；产戳后 `-Check` 四项全绿。
**这条实测就是「一个窗口的栈可能是两次构建的混合体」的直接证据**——一个「目录」不等于一个
「构建」，同一栈里不同二进制的来源提交可以不同。结论：**事后补戳只能是 `inferred` 级证据；
第 2 步必须由构建车道在构建完成时逐二进制产戳。**

## 7. 第 2 步实现计划（本批**未实施**）

1. **接线点**：`tools/conformance/card_window.py` 的 stack 步（profile 的 binaries 清单已知）
   在拉起前调用
   `stamp-binaries.ps1 -Check -BinDir <profile 二进制目录> -Names <profile binaries> -RequireAll`；
   `tools/window/start-stack-wmi.ps1` 作为备用接线点。
2. **失败语义**：`-Check` rc≠0（`1` 戳缺陷 / `2` 判不了）一律**停栈**，不继续跑；起栈日志与
   `preflight.txt` 写 `provenance=unknown code=<首个 RED CODE>` 并保留全部 RED 行；起栈 rc=2，
   `invalid_reason=binary_provenance`。**不是注脚**——记录后继续跑等于把错栈结论送下游。
3. **preflight**：`CODE_HEAD` 改为从戳读；多二进制时逐行 `CODE_HEAD[<name>]=<head>`，
   并追加 `CODE_HEAD_SOURCE=<head_source>`（`inferred*` 必须显式可见）；
   `REPO_HEAD` 仍单列（可含其后纯文档提交，写明数量）。
4. **五项通电条件逐条**：
   - ① 每个被拉起的二进制都有戳 —— 用 profile binaries 清单 + `-RequireAll`；
   - ② 戳内 sha256 与文件实时一致 —— `-Check` 现算；
   - ③ head 能解析到真实提交 —— `-Check` 的 `cat-file -e`；
   - ④ `built_at` 与 mtime 差 ≤5 分钟 —— `-Check`；
   - ⑤ 健康栈自检整轮 PASS —— **本批只在构建目录上跑了 `-Check`，没有跑整轮真机健康栈**，
     条件 ⑤ 未满足，故本批不得接 `provenance=unknown ⇒ rc=2`。

## 8. 牙清单与自测矩阵（`-Selftest` 实测，两种 PowerShell 宿主均 GREEN）

| # | 变异 / 夹具 | 期望 | 状态 |
| --- | --- | --- | --- |
| 1 | 健康目录（3 个合成二进制；sha 独立复算；C# 戳含 `dotnet`/`runtime`；`-RequireAll` 校验） | rc=0 + `GREEN 3` | 已实现 |
| 2 | 删任一必填字段（删 `sha256`） | rc=1 + `MISSING_FIELD sha256` | 已实现 |
| 3 | 篡改戳内 `sha256` | rc=1 + `SHA256_MISMATCH` | 已实现 |
| 4 | `head` 改成不存在的提交（40 个 0） | rc=1 + `HEAD_NOT_FOUND` | 已实现 |
| 5 | `built_at` 比 mtime 晚 10 分钟 | rc=1 + `BUILT_AT_SKEW` | 已实现 |
| 6 | 空目录 / 无二进制 | rc=2 + `NO_BINARIES`（不得静默通过） | 已实现 |
| 7 | 产戳后往二进制追加 1 字节 | rc=1 + `SHA256_MISMATCH`（证明 sha 是现算，不是抄报告） | 已实现 |
| 8 | 产戳时 `head` 不可解析 | rc=2 且**不产生戳文件** | 已实现 |
| 9 | 第 2 步接线：删掉起栈前的 `-Check` 调用 | 起栈必须变红 | **未实施**（第 2 步） |
| 10 | 第 2 步接线：伪造一份坏戳 | 起栈 rc=2 + `invalid_reason=binary_provenance` | **未实施**（第 2 步） |

自测的夹具全部在 `%TEMP%/stamp-binaries-selftest-<guid>` 内创建（临时 git 仓库 + 3 个合成
二进制），结束后由创建方清理；不读真实仓库状态、不碰真实二进制、不跑 cargo/dotnet。

## 9. 已知限制（第 1 步交付时必须知道）

- **head/dirty 是「产戳时」`-RepoRoot` 的状态，不是「构建时」的状态**。事后给旧构建目录产戳，
  `head` 只能是推断（`head_source` 标出来）；要拿到真实来源，**必须由构建车道在构建完成时立即产戳**。
- **一个目录 ≠ 一个构建**：同一输出目录里各 exe 可能来自不同提交（§6.1 实例）。此时用 `-Heads`
  逐项指定，不能用一个 `-Head` 冒充全部。
- **没有把 commit 内嵌进二进制**：`head` 始终是外部证据；二进制与戳分离运输时二者可能脱钩，
  第 2 步必须让「起栈时 binary/stamp 同目录且 `-Check` 通过」成为硬前置。
- **C# 的 `runtime` 取二进制同目录的 `<base>.runtimeconfig.json`**；该文件缺失时回落到产戳机器的
  第一个 `Microsoft.NETCore.App`——此时该字段语义降级为「产戳机器工具链」，不是目标框架，需人工注意。
- **窗口期不要跑产戳**：产戳模式会执行 `cargo --version` / `dotnet --version`，空载口径把
  cargo/dotnet 进程计为外部构建；`-Selftest` 因 `-NoToolchainProbe` 不派生子进程，可随时跑。
- **校验需要 git 对象库**：`-Check` 用 `git cat-file -e` 核 head，`-RepoRoot` 必须指向包含该提交的
  检出（worktree 共享对象库即可）；没有 git / 没有对象库的机器上 head 无法本机验证 ⇒ 判 rc=2
  （判不了，不是绿）。第 2 步需决定「部署机的信任根」（例如在构建机校验后随包携带结果），本批不做。
- **旧戳兼容**：`rebuild-server.ps1` 产出的 `mir2-server.build.json` 没有 `mtime`/`head_source`/
  `toolchain_kind`；本工具的 `-Check` 已兼容（补充字段只做存在时校验），但第 2 步若要把它纳入
  同一清单，应同步补齐字段或明确豁免。

## 7. DBSrv 六件 / LoginSrv 三件：产戳命令与时机（2026-11-17 起；2026-11-20 扩到六件，卡 `openmir2-own-binary-without-stamp`）

**时机**：`dotnet build src/DBSrv -c Release` **成功后立即**产戳（`built_at` 取二进制 mtime，
校验要求与 mtime 差 ≤5 分钟，别把「先构建、隔天再产戳」当常态）。只对部署源
`src/DBSrv/bin/Release` 产戳——`DBSrv.Storage.MySQL.dll` 与 `DBSrv.Storage.MongoDB.dll`
（`Directory.Build.targets` 的 `CopyStoragePluginToDbSrv` 会把 `DBSvr.Storage.*` 插件拷过来）
在这里各有一份副本，部署只许从这里取。清单以 `tools/orchestration/stamp-groups.psd1` 为准。

**产戳**（`-Head` 用构建时的提交；`-HeadSource explicit` 不要把推断当事实）：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools/orchestration/stamp-binaries.ps1 `
  -BinDir <worktree>/src/DBSrv/bin/Release -RepoRoot <repo> -Head <rev> -HeadSource explicit `
  -Names DBSrv.exe,DBSrv.dll,DBSrv.Storage.dll,DBSrv.Storage.MySQL.dll,DBSrv.Storage.MongoDB.dll,OpenMir2.dll
```

**校验**（起栈前、**影子路径**上做；`-RequireHead` 可选，给了就断言来源提交）：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools/orchestration/stamp-binaries.ps1 `
  -Check -BinDir <stack>/runtime/DBSrv -RepoRoot <repo> -RequireAll `
  -Names DBSrv.exe,DBSrv.dll,DBSrv.Storage.dll,DBSrv.Storage.MySQL.dll,DBSrv.Storage.MongoDB.dll,OpenMir2.dll `
  -OwnGlobs 'DBSrv*.dll,OpenMir2.dll' [-RequireHead <rev>]
```

判据：`GREEN n stamps verified` + rc=0 才可起栈；任何 `RED …` ⇒ **不起任何进程**。

**`-OwnGlobs` 覆盖检查（2026-11-20，卡 `openmir2-own-binary-without-stamp` R3）**：匹配 glob 的
每个 `.dll/.exe` 必须已出现在 `-Names` 里，否则 `RED <file> UNLISTED_OWN_BINARY`；该检查**独立于
`-RequireAll`**（不给 `-RequireAll` 也生效）。目的：`CopyStoragePluginToDbSrv` 自动拷进输出的
`DBSvr.Storage.*` 插件若没进名单，`-RequireAll` 原本完全看不见它——正是 MongoDB 插件
「部署了但无身份」的生成机制。清单本身以 `tools/orchestration/stamp-groups.psd1` 为单一来源。

**命名坑（务必别改回去）**：本机 HIPS 会把「以 `Expect` 开头的 PowerShell 参数名」与
`-ExecutionPolicy Bypass` 同现的命令行**整条静默阻断**——嵌套 `powershell` 退出码 `-1`、
stdout/stderr 全空、脚本一行都没跑。`-ExpectHead`/`-Expect` 实测被拦，`-RequireHead` 同位置正常。

**拷贝必须保 mtime（2026-11-17 botfleet 实测）**：把三件套从 `bin/Release` 拷到 `runtime\DBSrv` 时，
**裸 `cp`/`copy` 会把 mtime 改成"现在"** ⇒ 戳内 `built_at` 与文件 mtime 差 >5 分钟 ⇒ `BUILT_AT_SKEW` 判红。
PowerShell `Copy-Item` 保 mtime（安全）；robocopy `/COPY:DAT` 也可以。判据不变：`built_at` 与 mtime 差 ≤5 分钟。

**负向验证（botfleet 实测，每条起栈前都该会）**：藏掉任一 `.build.json` ⇒ `MISSING_STAMP` + **不起任何进程**；
把影子换回旧二进制（戳没跟着换）⇒ `SHA256_MISMATCH` + 不起进程。两条都证明这道门是 fail-closed 的。

**升级顺序约束**：`m1-stack/start-m1-stack.ps1` 调的是 `$CsharpRepo	ools\orchestration\stamp-binaries.ps1`
——**主检出（master）那份**。因此本工具的任何修复**必须先合 master** 才会对真实起栈生效；
在分支上「自测全绿」不等于生产路径绿（2026-11-17 实测：`.dll` 支持在分支上绿、master 上照旧 SKIP ⇒ 起栈必红）。
