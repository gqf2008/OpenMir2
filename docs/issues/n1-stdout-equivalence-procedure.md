# N=1 归一化 stdout 等价：可复跑判据（此前无可复跑命令）

> 状态：**判据 + 可复跑脚本（2026-11-22）**。卡 `openmir2-n1-stdout-equivalence`。
> **缺口声明**：此前多处引用「N=1 归一化 stdout **51 行**逐字节」，但仓里只有**规则**
> （`docs/shard-runtime-20261116.md` 的「剥 12 位时间戳后逐字节 + 多重集」），
> **没有任何一条可复跑的命令**——第三方无法复现。本页 + `tools/hygiene/n1_stdout_equivalence.ps1`
> 把规则落成命令；脚本已登记进工具门（`tool_selftest_gate` 的 `REGISTRY`）。
>
> **认树锚计数说明**：`tools/hygiene/tool_selftest_gate.py` 的认树锚文档此前停在 **71 项 / 清点 76**，
> 而 `--inventory` 实跑早已是 **73/78**——这是**历史漂移的修正**，**不是本次新增造成的**：本工具只
> 让计数 +1（73→74、78→79）。「文档里的计数」同样会漂，所以三处活锚都按 `--inventory` 实跑校准过一次。

## 1. 判据（两条都绿才算绿）

1. **顺序敏感**：两侧 stdout 每行剥掉开头的 12 位 `HH:MM:SS.mmm` 与 1 个空格（`glog!` 的墙钟
   前缀）后，按**行序**拼接的 SHA256 相等；
2. **行多重集**：同一批行按序数排序后的 SHA256 相等。
   （**顺序**一项先剔除 §1b 的噪声类；**多重集**一项用全量行。）

补充细则：
- 两侧都为空 ⇒ **PASS** 并打印 `EMPTY_BOTH`（记「空文件 sha 相同」），同时打印两侧 stderr 的
  行数与首/末行作旁证；
- **非空文件却解析出 0 行**（产物缺失、函数输出被污染等）⇒ **rc=2 判不了**，绝不判 PASS；
  `EMPTY_BOTH` 只允许在**两侧文件字节数都 = 0**时成立（2026-11-22 实测过「9600 字节被判 0 行」
  的假绿，判决矩阵 `Get-N1Verdict` 现在把它钉成 UNDECIDABLE，并有用例覆盖）；
- 一侧空一侧非空、或行数不等 ⇒ **FAIL**；
- 退出码：**0 绿 / 1 红 / 2 判不了**（用法错误、文件缺失、提前退出、优雅关服超时）。
- **不接受**：`| tail` 之类取 rc、只看某几行、或「重跑一下变绿」——判据是两条 sha 同时相等。

## 1b. 归一化与噪声类（三条规则，都不是产品差异）

1. **关服管道名里的 pid 归一化**：`...openmir2-gamesvr-shutdown-<pid>` 每次运行必然不同（进程 pid）
   ⇒ 统一替换为 `<pid>` 再比。
2. **DB/IDS 连接重试环的失败行属「顺序噪声类」**：两条边各自按墙钟退避重试，两侧**内容相同**但
   相对顺序由重试时刻决定 ⇒ **顺序比较剔除这两类行，行多重集仍用全量**（内容真漂移仍然红）。
   归一化实现见 `$N1NoisePatterns` / `Test-N1NoiseLine`；`summary.json` 记
   `baseline_order_lines / feature_order_lines / noise_lines_baseline / noise_lines_feature`。
3. **PowerShell 逐条执行：函数必须先定义后调用**（曾实测 `术语 'Assert-N1RunPath' 不会被识别`）。
   脚本 `-Selftest` 对 `Assert-N1RunPath` / `Read-N1Lines` / `Get-N1Verdict` 逐个断言
   「定义行号 < 首个调用行号」。
4. **昼夜值 `Bright=<n>` 归一化**：`世界时钟变化：Bright=<n>` 的 n 由**墙钟日期**派生（昼夜）——
   跨时段两次运行必然不同（实测 `Bright=1` vs `Bright=2`），**不是产品差异** ⇒ 归一化为
   `Bright=<n>`。不归一化时，10:50 与 11:02 两次运行会在**内容**上真差一行，判据面随墙钟漂移。

**跨时段稳定性证据（2026-11-22，本机）**：`E:/tmp/m2-acceptance/n1-stdout/` 下 10:50–11:02 的
**7 个含两侧 stdout 的产物目录**（含早前被判 `EMPTY_BOTH`/`RED` 的那几次捕获），按上述四条规则
逐目录复算，全部得到同一组数字：
`61/61 行`、`53/53 顺序行`、`8/8 噪声行`、`order_sha=ffe13210…`、`multiset_sha=a87b66f0…`
⇒ 判据面不再随墙钟漂移（与 Leader 侧 6 次完成运行/9 个目录的结论一致）。

## 2. 怎么跑

1. **两棵 worktree**：基线树 = 合并目标（如 master 的合并点），特性树 = 待合并分支。
   两棵树各自 `cargo build -p game-svr`（**同一 profile**，例 `debug`），产物路径
   `<树>/mir2-rs/target/debug/game-svr.exe`。
2. **一份临时配置副本**（两棵树共用，内容逐字节相同）：从生产 `Server.conf` 拷贝，
   **只改** `[Server] GatePort` 为**非 5000** 的空闲端口（例 5077），并可去掉 Envir 依赖走
   **占位 `WorldMaps`**。脚本会**校验**该文件：必须有 `[Server]` 段、`GatePort` 存在且等于
   `-Port`、且**不得为 5000**。
3. **同一启动命令**：两棵树都以「同一个 conf 路径」作为唯一参数启动 game-svr；脚本负责
   固定时长（默认 20s）→ **优雅关服**（连 `\\.\pipe\openmir2-gamesvr-shutdown-<pid>` 写 **ASCII `SHUTDOWN`（8 字节；产品只认这一串，其它 8 字节会被 `WRN 未知请求` 忽略）**）
   → 等待退出（默认 30s 上界；超时判 2，不静默 kill 冒充通过）。
4. 脚本输出：`baseline.normalized.txt`、`feature.normalized.txt`、`summary.json`
   （含两侧行数、两条 sha、结果）；红时打印 `FIRST_DIFF line=N baseline='…' feature='…'`。

```powershell
pwsh -NoProfile -ExecutionPolicy Bypass -File tools/hygiene/n1_stdout_equivalence.ps1 `
  -BaselineExe E:\wt\baseline\mir2-rs\target\debug\game-svr.exe `
  -FeatureExe  E:\wt\feature\mir2-rs\target\debug\game-svr.exe `
  -Conf E:\tmp\m2-acceptance\n1-stdout\Server.conf -Port 5077
```

自测（离线、不起进程、不需要 exe/conf）：

```powershell
pwsh -NoProfile -ExecutionPolicy Bypass -File tools/hygiene/n1_stdout_equivalence.ps1 -Selftest
```

## 3. 覆盖的启动路径（与 P2-3b Step 4 同口径）

**覆盖**：配置文件解析（`[Server]` 必填键集校验）、地图注册（占位 `WorldMaps`，无 Envir）、
`World` 构造与区布局装配、脚本/NPC 装载（若给真 Envir）、固定时长内的 tick、**优雅关服**
（命名管道 → 存档收尾 → `perf_counters::finish`）、进程退出码。

**不覆盖**（明确出域，不许拿它冒充证据）：真实客户端/网关交互、登录与会话、帧生成与顺序、
DB 读写内容、`-Mode Both` 真机对拍（另有其门）、以及**由外部依赖时序决定的行**（DB 重试/退避、
端口占用重试）——这些行不在可对比域，必须用「同一可达 DB 端点 + 同一端口」让它们不出现；
若目标机没有可达 DB，本判据**判不了（rc=2）**而不是红。

## 4. 证据怎么记

合并提交里记：`baseline_lines / feature_lines`、两条 sha、`summary.json` 路径、以及两棵树
的 commit。**空文件**必须写 `EMPTY_BOTH` 与 stderr 旁证，不许只写「通过」。

## 5. 为什么两条 sha 一起用

只比顺序 sha 时，一条被删、另一条被加会红（对）；只比多重集时，换序会绿（错）。两条一起：
换序 ⇒ 顺序 sha 红、多重集绿——**定位是「顺序变了」**；换内容 ⇒ 两条都红——**定位是「内容变了」**。
脚本按这个组合给出 `order_equal/multiset_equal` 两个布尔，便于判读。
