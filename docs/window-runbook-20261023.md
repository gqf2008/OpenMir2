# 真机窗口 Runbook（2026-10-23 定稿）

> **本文最后核对：commit `6fe496abd`（2026-10-24 07:55:46 +0800；核对时刻 08:00 +08，卡 `openmir2-runbook-preflight-refresh`）**；上一轮审计基线 `4e0bfd2b8`（2026-10-23 22:21:57，卡 `openmir2-runbook-tool-consistency-audit`）与 `7ef49f807`（22:28:01）。
> 逐条命令的实测输出与缺口见 `docs/runbook-tool-consistency-audit-20261023.md`；改工具后**先跑 §0.5 的 60 秒清单**再动窗口。

> 面向：第一次跑窗口的人 / 下一次跑窗口的我。本文只写**跑之前的必做动作**与**读结果的口径**，
> 工具用法见 `tools/conformance/README.md`，卡的判据见各卡 `whitelist.md` 行。

## 0. 现状（**`4e0bfd2b8` @ 2026-10-23 22:21:57 +0800**，审计时刻 22:27；原写 `73c39cca` @ 15:20，已过期）

- **欠窗口的卡：173 张**（口径见 `docs/leader-goal-20261023.md` §1.1 的订正：早前「grep 已合入 master = 114」是错的，
  那 114 里 70 张已 closed、23 张是 tier=L；真欠账 21 张，加上 149 张措辞不在 grep 里的 + 3 张快照后合入 = **173**）。
  分批与逐卡落点见 `docs/window-batch-plan-20261023.md`。
- 工具链：`card_window.py`（批处理）、`compare_runs.py`（帧+库比对）、`param_discovery.py`（参数发现）。
- **值断言前置已完成**：账本 compare 诊断值断言已随 `b9765764f`（2026-10-23 16:45:18，approve `reviewer-2 d755493c`）合入
  （订正：原文写「尚未加值断言 / 仍在飞行」，那是 16:45 之前的状态）。
  参数发现已接进 `card_window.py`（卡 `openmir2-window-discovery-wiring` 已合入 `e72fb9728`）；规则表另见 §4.5。

## 0.5 开窗前 60 秒自查（**一条命令**；手工步骤保留为退路）

```sh
python -I -B tools/conformance/preflight.py \
  --profile alt \
  --root E:/tmp/conformance/rust-root \
  --log-dir E:/tmp/conformance/rust-logs \
  --ports-profile E:/tmp/window/r5/root/alt-port-profile.json
# 需要更深自查时再加：--with-dry-run（委托 card_window --dry-run）/ --with-selftest
#   （--with-selftest 就是委托 `card_window --selftest`：2026-10-24 实测 GREEN 240/240；
#    测法 `time python -I -B tools/conformance/card_window.py --selftest`；
#    耗时随机器/profile 环境而变：reviewer-3 机 22.26s、worker-58 机 45.05s/52.70s、上一轮/另一环境约 55s）
```

- 参数：`--profile` 内建名（`alt` / `csharp-default` / `window-magic` / `window-testdb`）或 profile JSON 路径；`--root` / `--log-dir` 原样传给
  `start_mir2_server.ps1`（-Root / -LogDir）；`--ports-profile` 可选；`--allow-debug` 把 `Debug-Fallback`
  从 FAIL 降级为 WARN——**只在明确要用 debug 二进制时加**，并在证据里写明原因。
- 2026-10-24 在 `6fe496abd` 实跑（带全参数）：`docs`/`ports`/`profile` 三步 OK，`binary` 步 **FAIL**
  （`SERVER_EXE_BUILD=Debug-Fallback`，rc=2）——修法就是它自己打印的那句 `cargo build --release -p mir2-server`。
  这是**预期内的门**：树里只有 debug 构建时，窗口不能开。

输出每步一行 `[OK]/[FAIL] <step>: <细节>`，最后 `PREFLIGHT PASS`（rc=0）或 `PREFLIGHT FAIL`（rc=2）；**FAIL 行直接写了修复建议**。各步含义（`docs`/`binary`/`ports`/`profile` 四步与卡 `openmir2-window-preflight-script` 一致；`bash` 步 2026-10-24 新增，卡 `openmir2-window-profile-env-hardening`）：

| 步骤 | 查什么 | 失败时该做什么 |
|---|---|---|
| `docs` | runbook 有 `probe_timeout_ms`、无 profile 的 `"timeout_ms"`（旧键会被**静默忽略**）；scenarios/README 同样提到 `probe_timeout_ms` | 按第 0 步改回 `probe_timeout_ms` |
| `binary` | 委托 `start_mir2_server.ps1 -WhatIf`，解析 `SERVER_EXE/SHA256/BUILD`；必须是 `Release` 且打印的 sha 与本地重算一致 | `cargo build --release -p mir2-server`（确要用 debug 才加 `--allow-debug`，并在证据里写明） |
| `ports` | 从 profile 的 `sides[*].ports` 逐个 connect 探测（**不写死端口表**），要求全空 | 先停上一轮栈（`preflight` 会点名端口） |
| `profile` | `probe_timeout_ms` 为正整数；开了 `require_pid_binaries` 的侧必须有 `pid_file` | 补 profile 键 |
| `bash` | restore 的 bash 不落到 WSL 启动器 stub：裸 `bash` 在 `%SystemRoot%\System32\bash.exe` 存在时必被 CreateProcess 先命中（未装发行版 rc=1，window-run-1 §1.1）；显式路径必须存在且不在 System32/Sysnative 下 | profile 里改写 Git bash 绝对路径（如 `C:/Program Files/Git/usr/bin/bash.exe`）；装 Git for Windows 或设 `CARD_WINDOW_GIT_BASH` |
| 可选 | `--with-dry-run` / `--with-selftest` 才委托 `card_window` 对应动作（默认不跑） | 同原命令 |

> 只读：不起栈、不连库、端口只做 connect 探测。`--selftest` 用桩验证失败分支（rc=2 且点名；bash 步 5 条：stub 命中红 / 真 Git bash 绿 / 显式 stub 红 / 解析不到红 / 解析到 stub 红）。

### 退路：手工步骤（命令不可用时照旧做）

> **窗口项怎么写**：补 `window_required` 判据时按 `docs/window-criteria-template.md` 的六字段写（前置 / 可复跑命令 / 期望 / 判据 / 掩码 / 负责人与状态）；自查一段 status 文本缺哪几项：`python -I -B tools/goal/window_criteria_lint.py --text <文件>`（**只告警，退出码恒 0，不参与门禁**）。

> 五步顺序：**核二进制 sha → 核端口 → 核 profile 键 → `--dry-run` → `--selftest`**；外加第 0 步（文档口径）与第 4.5 步（idents 登记）。
> 任一步与期望不符 ⇒ **不要开窗**（一轮窗口 ≈ 240G + 数十分钟）。

**第 0 步：文档口径（30 秒）**

```sh
python -I -c "
import io,re,sys
s=io.open('docs/window-runbook-20261023.md',encoding='utf-8').read()
ok = ('probe_timeout_ms' in s) and not re.search(r'\"timeout_ms\"\s*:', s)
print('DOC-KEYS', 'OK' if ok else 'RED'); sys.exit(0 if ok else 1)"
```
期望：`DOC-KEYS OK`，rc=0。失败含义：runbook 又写回了旧键 `"timeout_ms"` —— profile 会**静默忽略**它，probe 仍在 180s 被杀。

**第 1 步：二进制路径与 sha256**

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools/m2-acceptance/start_mir2_server.ps1 `
  -Root 'E:/tmp/conformance/rust-root' -LogDir 'E:/tmp/conformance/rust-logs' `
  -PortsProfile 'E:/tmp/window/r5/root/alt-port-profile.json' -WhatIf | Select-String 'SERVER_EXE'
Get-FileHash -Algorithm SHA256 <上一条打印的路径>
```
期望：路径含 `\target\release\mir2-server.exe`；记下 hash 写进 `README.txt`。
失败含义：含 `\debug\` ⇒ 你所在树没构建 release（或用的是别的检出的二进制）；路径与你预期的主检出不一致 ⇒ 见 §1 第 1 步「注意解析口径」，**以起栈后 pid 文件的 `roles[].exe` 为准**。

**第 2 步：本窗口端口全空（不是 port-status 的十个）**

```powershell
powershell -NoProfile -Command "Get-NetTCPConnection -State Listen -ErrorAction SilentlyContinue | Where-Object { 15000,15100,15500,15600,15700,16000,17000,17100,17200,17201,17202,17600 -contains $_.LocalPort } | Select-Object LocalPort,OwningProcess"
```
期望：**空输出**。失败含义：有输出 ⇒ 上一轮栈没停净；`card_window` 起栈前也会 `check_ports_free` 并 rc=2 点名 pid。
附：`powershell -File E:\MirServer\port-status.ps1` 里 **3306 必须 UP**（mysqld 不归窗口管）；该脚本查的是默认栈 5000–7200，**不是本窗口端口**。

**第 3 步：profile 键（超时键 / pid 文件 / 对账开关）**

```sh
python -I -B -c "
import sys; sys.path.insert(0,'tools/conformance')
import card_window as c
p=c.load_profile('alt')
print('probe_timeout_ms', p.get('probe_timeout_ms'))
print('pid_file', p['sides']['rust'].get('pid_file'))
print('require_pid_binaries', p['sides']['rust'].get('require_pid_binaries'))"
```
期望：`probe_timeout_ms 20000`（默认；`upgrade_retrieve` 要 720000）、
`pid_file E:/tmp/conformance/rust-logs/mir2-server.pid.json`、`require_pid_binaries True`。
失败含义：`pid_file None` ⇒ 工具版本不对（新工具加载期即 `ConfigError`）；`probe_timeout_ms` 仍是 20000 而你要跑 `upgrade_retrieve`（`sleep 660000`）⇒ 180s 必被杀。

**第 4 步：`--dry-run`（不起栈/不建目录）**

```sh
python -I -B tools/conformance/card_window.py --card <卡 id> \
  --scenario tools/probe/scenarios/<场景>.json --out E:/tmp/window/<本次 run> --profile alt --dry-run
```
期望：rc=0；打印 restore/stack/discover/probe/db/stop/compare 全序列 + `[csharp]|[rust] 预置位 … 可走` + 端口预检与就绪等待；**`<out>` 目录仍不存在**。
失败含义：rc=2 且 `scenario is BLOCKED: …` ⇒ 封存场景（不能跑）；rc=2 且点名图/坐标 ⇒ 预置位非法；rc=2 且点名端口 ⇒ 栈没停净。

**第 4.5 步：idents 登记（§1 第 5 条的自查，输出必须 `missing: []`）**

见 §1 第 5 条的片段。**截至 `4e0bfd2b8` 它不是空的**（`[202, 203, 642, 709]`）⇒ 跑 `equip_struck_save` / `upgrade_retrieve` 前必须先补 rules.json（另卡）。

**第 5 步：离线自检（工具没被改坏）**

```sh
python -I -B tools/conformance/card_window.py --selftest     # 期望 GREEN: … selftest 240/240，rc=0（2026-10-24 本机实测）
python -I -B tools/conformance/compare_runs.py selftest      # 期望 GREEN: compare_runs selftest 262/262，rc=0
```
失败含义：计数**变小** ⇒ 有判据被删（先查最近提交）；变红 ⇒ 工具坏了，别开窗。
**注**：两个自检的计数都随夹具与规则表变更而变（`compare_runs` 侧实测：`4e0bfd2b8` 198 ⇒ `b42f20a28` recog-unmask +40 = 238 ⇒ `c8473b344` 补 202/203/642/709 四条规则 +24 = 262）⇒ 以**实跑**为准，别抄本文的数字（本文的 240/240、262/262 是 2026-10-24 实测）。
> 2026-10-25 起：**全部工具自测由 `tools/hygiene/tool_selftest_gate.py` 统一跑**（见 §7），上面两条仍可用于只查单个工具。

---

## 0.6 零触碰与内容比对（2026-10-25 吸收：Castle/List.txt 每 600s 被默认 C# 栈重写）

- **事实**（卡 `openmir2-test-writes-real-envir-path`，2026-10-25 复核）：只要默认 C# GameSrv 在跑
  （PID 14040，2026-10-24 21:41:48 启动，监听 5000），SystemProcessor 的 10 分钟门会触发
  `CastleMgr.Save → SaveCastleList`，重写 **`E:/MirServer/M2GameSvr/Castle/List.txt`**。
  - **实际路径**由 `E:/MirServer/M2GameSvr/Server.conf:65/71`（`CastleDir=Castle`、
    `CastleFile=Castle/List.txt`）覆盖；`Envir/Castle/` 在本部署**不存在**（默认值
    `Envir\Castle\` 未被使用）。
  - 内容恒为 6 字节 `30 0d 0a 31 0d 0a`（文本行 `0` 与 `1`，各以 CRLF 结尾），sha256
    `9b38d284bae4dc26e593ab0d5cb50d846c3328a8c0467291a8a473cf15c1615b`；
    实测 mtime `2026-10-25 04:02:02.66 +08`（上一跳 `03:52:02`，Δ600s）。
- **口径①：比对内容，不比 mtime。** 冻结基线/零触碰核查一律用 sha256 或字节比较；
  上式 sha256 不变即视为内容未被修改。任何 `LastWriteTime` / `st_mtime` 判断都会每 10 分钟误报。
- **口径②：零触碰 = 停 C# 业务进程，不停 mysqld。**

  ```powershell
  # 停（逐进程；勿用 start-all.ps1 -Stop —— 它会把 mysqld 一起停）
  Get-Process GameSrv,GameGate,LoginGate,SelGate,DBSrv,LoginSrv -ErrorAction SilentlyContinue | Stop-Process -Force
  # 起回（mysqld 已在跑时自动跳过）
  powershell -NoProfile -ExecutionPolicy Bypass -File E:/MirServer/start-all.ps1
  # 恢复核对（期望 10 端口：3306,5000,5100,5500,5600,5700,6000,7000,7100,7200）
  powershell -NoProfile -ExecutionPolicy Bypass -File E:/MirServer/port-status.ps1
  ```

  核对口径：`port-status.ps1` 的缺失列表为空。2026-10-25 04:00 观测到 5700 未监听
  （DBSrv 声明端口之一），属既有偏差：窗口前与 owner 确认是否预期，**不要**在本卡里改配置。
- **口径③：preflight 不依赖 mtime。** 2026-10-25 重跑：
  `preflight_private_stack.py` = 6 pass / 2 fail（`binary_stamp` 缺 release 二进制、
  `start_rust_exe` 仍指 `.worktrees/dev-2`）；`preflight.py --profile alt|window-magic --with-dry-run`
  各 1 fail（`binary: Debug-Fallback`）。两类都由 `run_batch.sh` 的首步
  `rebuild-server.ps1 -SyncStartRust` 覆盖，**开窗前不必单独重建**。
  代码面 grep `st_mtime|getmtime|LastWriteTime|mtime`（`tools/window`、`tools/conformance`、
  `tools/probe`、`tools/dbsnap`）零命中 ⇒ 无其它巡检项依赖文件时间。

---

## 0.7 C# 时钟注入缺失的降级口径（2026-10-25；卡 `openmir2-window-kit-p0-unblock`）

- **事实**：Rust 侧有 `MIR2_CLOCK_MS`（`mir2-rs/bins/game-svr/src/clock.rs`）；**C# 侧零实现**
  （`grep -rIl MIR2_CLOCK src/` = 0 命中）。卡 `openmir2-m3-clock-injection` 的 C# 半边仍缺（合并态以线程为准）。
- **受影响窗口项**（见 `docs/window-kit-coverage-20261025.md` §1.1 W-TAX-4、§1.5 W-T127-2）：
  1. **W-TAX-4**（日上限/金库上限/`SaveTick` 10min 门）：改为**真实时间点复拍**——在
     `SaveTick` 相邻两个 10 分钟门附近各取一次（如 **21:50** 与 **22:00**），比较
     `TotalGold`/`TodayIncome` 的增量与「未到门不刷新」的行为；**不逐位对 `SaveTick`**
     （两侧 tick 原点不同，且 C# 无法注入）。
  2. **W-T127-2**（结束预告/超时停战）：改为**真实时间点复拍**——把 `CastleWarTime`/
     `ShowCastleWarEndMsgTime` 配到当天，取 **21:50**（`[离结束还有10分钟]`）与 **22:00**
     （`[已经结束]`）两帧；帧文本/颜色/收件人可逐字节比，**时间字段与相对序不逐位比**。
- **边界（必须写进证据）**：降级只证「帧内容与可见行为一致」，不证「同一 tick 的
  `SaveTick`/计时器位级一致」；后者留给 C# 时钟钩子落地后的补拍。
- **将来补 C# 时钟钩子的切入点（后续卡素材，本卡不实现）**：
  `mir2-rs/tests/parity/m3/rng-hook/StartupHook.cs` 的 StartupHook 模式（`DOTNET_STARTUP_HOOKS`
  + 环境变量）可替换 `GetTickCount()`/`DateTime.Now` 的调用面；C# 半落地后在窗口用
  `MIR2_CLOCK_MS` 同值把两侧压到同一时刻，再恢复逐位判据。

---

## 0.8 C# 栈 RNG 同种子接线（2026-10-25；卡 `openmir2-window-csharp-rng`）

- 窗口 C# 栈的启动器已收进仓库：**`tools/window/start-csharp.ps1`**（`card_window.py` 的 alt
  profile 已指向它，仍经 `start-stack-wmi.ps1` WMI 包装）。`E:/tmp/conformance` 副本会被外部清扫，
  **仓库为准**。
- 只有 `card_window.py --rng-log`（或手工给 `-RngLog`/env `MIR2_RNG_LOG`）时，它才部署
  `RngSeedHook.dll` 并给 GameSvr 注入 `DOTNET_STARTUP_HOOKS` + `MIR2_RNG_SEED` + `MIR2_RNG_LOG`
  + `MIR2_RNG_SITE=1`；不给则与旧脚本行为逐字节相同（不注种子）。
- 账本口径：头两行 `INJECT_OK <seed>` + `SEED_PROBE`；数据行 5 列且 `site` 非空。
  Rust 侧同 seed 由 `start_mir2_server.ps1 -RngLog` 注入。实测证据与复算命令见
  `docs/window-csharp-rng-20261025.md`。
- **台账体积上限（2026-10-25，卡 `openmir2-fixture-monsters-prepend` ③）**：C# 钩子**每次抽数都落盘**，
  20 分钟实测 **899MB** ⇒ 不裁剪会写满磁盘/拖垮传输。`card_window.py` 在**停栈之后**调用
  `trim_rng_ledger()`：超过 **64MiB** 就记录**裁剪前全量**的 `bytes/行数/sha256`
  （`original_sha256` = 裁剪前整文件哈希，不是 head 的哈希），只保留**前 20 万行**
  （`INJECT_OK`/`SEED_PROBE` 都在最前，裁剪后仍过 `check_rng_ledger`）；证据写在本侧结果
  `rng_log_trim`（`trimmed/original_bytes/original_lines/original_sha256/kept_lines/kept_bytes`）。
  未超限也记录 size/行数/sha256。窗口本身仍应**限时**（`--timeout-ms` / 场景步数），
  裁剪是兜底不是许可证。
- 提醒：不要从交互 shell 直接 `powershell -File start-csharp.ps1` 后等返回（子进程继承管道会挂）；
  窗口路径走 WMI 包装，手工排查用 `-Plan`。

---

## 0.9 两侧 `Castle/List.txt` 必须对齐（2026-10-25；卡 `openmir2-window-castle-list-symmetry`）

- **事实**：私有栈两侧不同源——`csharp-root/M2GameSvr/Castle/List.txt` 曾为 `0`（1 城），
  `rust-root/M2GameSvr/Castle/List.txt` 为 `0,1`（2 城）；C# 开 `--rng-log` 时 GameSvr 走影子
  `csharp-root/M2GameSvr-hook/`，其 `Castle/List.txt` 也要同内容。
- **为什么必须对齐**：`CastleMgr.IncRateGold` 会**升序遍历全表** ⇒ 城数不同时 W-TAX-2
  （`GetAllNpcTax=1` 两城各入账一次）**必然假红**；多城广播次序（T-127 W-T127-4）等工作项
  也依赖城表一致。
- **检查（fail-closed）**：`tools/window/preflight_private_stack.py` 新增
  `castle_list_symmetry` 步——比对两侧（含影子）内容与 sha256，不一致打印两侧 bytes/sha256
  并 rc=2；只读，不改文件。
- **对齐办法**（私有栈夹具，**不碰 `E:/MirServer`**）：
  ```powershell
  # W-TAX-2 需两城：以 rust-root 的 0,1 为基准
  Copy-Item -Force E:/tmp/conformance/rust-root/M2GameSvr/Castle/List.txt `
    E:/tmp/conformance/csharp-root/M2GameSvr/Castle/List.txt
  if (Test-Path E:/tmp/conformance/csharp-root/M2GameSvr-hook/Castle/List.txt) {
    Copy-Item -Force E:/tmp/conformance/rust-root/M2GameSvr/Castle/List.txt `
      E:/tmp/conformance/csharp-root/M2GameSvr-hook/Castle/List.txt
  }
  ```
  最小场景可反向对齐到 `0`（1 城），但 W-TAX-2 与多城项需 `0,1`；对齐后重跑 preflight，
  `castle_list_symmetry` 必须 OK。`Castle/0|1/SabukW.txt` 两侧本就逐字节同源，无需改。
- **影响面**：W-TAX-1..4（尤其 W-TAX-2）、T-127 W-T127-4（多城广播次序）、任何按城表遍历的
  城堡工作项。窗口执行单（exec sheet）前置表已同步该步。
- **影子 Envir 必须等于基线**（卡 `openmir2-preflight-envir-shadow-symmetry`）：`start-csharp.ps1`
  在 `--rng-log` 模式下 GameSrv 从 `csharp-root/M2GameSvr-hook/` 跑，且影子部署有
  `if (-not (Test-Path $shadowGs))` 守卫（`GameSvr.exe` 在就不会重新 robocopy）⇒ 手改影子 Envir
  会一直生效（内容冻结风险）。preflight 的 **`envir_shadow_symmetry`** 逐文件 sha256 比
  `csharp-root/M2GameSvr/Envir`、`M2GameSvr-hook/Envir` 与 `rust-root/M2GameSvr/Envir`，
  任一不同/多/少 ⇒ FAIL 并列差异文件+两侧 sha（fail-closed）。开窗前必须 OK（现网实测
  **642 文件、0 差异**）。
- **夹具应用目标（G-1）**：`--rng-log` 时 GameSvr 读影子，而 profile 的 `envir_root` 指基线 ⇒
  夹具（MonGen/怪物、以及任何写 Envir 的声明）**在影子存在时必须同时写基线与影子并保持 sha
  一致**；只写基线 = 对 oracle 不生效 + 下一轮本门必红。
- **门在写前、夹具在写后（G-2）**：preflight 是在夹具/起栈**之前**跑的只读检查；**不得以
  「preflight 绿」推断运行期基线/影子仍一致**。夹具应用后或收工后，应重跑 preflight（或至少
  比对基线/影子 sha），再记为开窗就绪。

---

## 0.10 跑后对称性：C# 运行期会重写 Castle 文件（2026-10-25；卡 `openmir2-postrun-symmetry-drift`）

- **事实**：C# 的 `CastleMgr.Save()`/`SaveCastleList()` 会按**内存状态**重写
  `Castle/List.txt` 与 `Castle/<n>/SabukW.txt`（与 600s 存档门同一机制）⇒ **跑后漂移是预期行为**，
  不是夹具坏了；两次实测都出现过影子 `M2GameSvr-hook/Castle/List.txt` 从 2 城变 1 城
  （mtime=运行时刻）。
- **自动复核**：`card_window.py` 在 verdict 定稿后、写产物前做一次跑后对称性复核，结果写
  `card-window.json` 的 **`post_run_symmetry_drift`**（无漂移为 `[]`；有则列 `{file, sha256:{侧:sha}}`）。
  **只标注、不改本轮 parity 结论**。`run_batch.sh` 批次末尾打印一行
  **`跑后对称性：OK`** 或 **`跑后对称性：DRIFT(n) <文件…>`**。
- **下一轮前必须还原/对齐**（不碰 `E:/MirServer`，只动私有根）：
  ```powershell
  # 以基线为准，把影子与 rust 的 Castle 目录对齐（示例：两城 0,1）
  Copy-Item -Force E:/tmp/conformance/csharp-root/M2GameSvr/Castle/List.txt `
    E:/tmp/conformance/csharp-root/M2GameSvr-hook/Castle/List.txt
  Copy-Item -Force E:/tmp/conformance/csharp-root/M2GameSvr/Castle/List.txt `
    E:/tmp/conformance/rust-root/M2GameSvr/Castle/List.txt
  # 再跑 preflight：castle_list_symmetry / envir_shadow_symmetry 必须 OK
  python -I -B tools/window/preflight_private_stack.py
  ```
  若 `SabukW` 的 `TotalGold/TodayIncome` 也被上一轮写脏，按 `window-fixture-config-castle`
  的 `castle_files` 夹具把两侧初始值显式声明（`TotalGold=0`、`TodayIncome=0`）。

---

## 0.11 怪物夹具必须**前插** MonGen.txt 头部（2026-10-25；卡 `openmir2-fixture-monsters-prepend`）

- **机制**：刷怪线程每 `RegenMonstersTime=200ms` 只前进**一条** MonGen（C#
  `WorldServer.MonGen.cs:205-230`；Rust `crates/world/src/mongen.rs:723-768` 同构）。
  单表 3081 行扫一轮 ≈ **616s（≈10.3 分钟）**（部署未配 `ProcessMonsterMultiThreadLimit`，缺省 1）
  ⇒ **追加**在末尾的夹具行窗口内永远轮不到（dev-1 那轮日志口径 39020 行 ≈130 分钟）：
  dev-1 第一轮实测 20 次 `CM_HIT` 全部被接受却无 `SM_STRUCK`、无死亡帧（`kill: no target frames`）。
- **修法**：`fixtures.monsters` 现在把行**前插**到文件头（index 0，首个闸门 ≈200ms 就刷，
  `StartTick==0`）；原 39020 行**字节序不动**；写后机判 `_mongen_fixture_layout`（append 形态直接 rc=2）；
  restore 仍是「备份字节写回 + 重读 sha256 等于写前值」。
- **判据/细节/影响面**见 `docs/window-fixture-corpse-20261025.md` §2.5。
- **受影响的卡**：`openmir2-window-fixture-corpse`（夹具语义不变，位置改为前插）、
  `openmir2-butch-two-draws-unblock`（首个真机用户，修好）、后续 drop/combat 类；
  没有场景依赖「末尾追加不刷出」。
- **RNG**：前插让夹具怪更早进入随机流（更早消耗抽数）；台账上限/裁剪见 §0.8。

---

## 1. 开跑前必做（缺一不可）

1. **重建 Rust 栈并记录 sha256**。
   **要重建的是哪个二进制**：`start-rust.ps1` 委托给 `tools/m2-acceptance/start_mir2_server.ps1`，后者的 `-ServerExe` 缺省值是
   **`mir2-rs/target/release/mir2-server.exe`（其次 debug）**——「单二进制多角色」。它**只从 `-Root` 读配置文件**，
   不执行 `rust-root` 里的任何东西。
   ⇒ **`rust-root` 目录里那堆 `.NET` 程序集（`DBSvr.dll`/`LoginSvr.dll`/`GameSrv.dll`/`TouchSocket.dll`…）与 Rust 栈无关**（证据里的 Rust sha256 必须来自 `mir2-rs/target/{release,debug}/mir2-server.exe`；`card_window.py` 已内置该解析并在起栈后用 `mir2-server.pid.json` 的 `roles[].exe` 对账，不符即 rc=2——卡 `openmir2-window-rust-binary-evidence`），
   别被它们误导成「跑的是 C#」；也别以为换掉它们就换了栈。真正决定跑的是哪份代码的，是 `SERVER_EXE` 那一行打印出来的路径。
   **本轮实测的坑**（2026-10-23 16:0x 复核）：`mir2-rs/target/release/mir2-server.exe` = **10-23 09:46**、
   `target/debug/mir2-server.exe` = **10-23 11:54**，而 master 已到同日 15:20+。缺省优先 release ⇒ **不重建就会用 09:46 那份**，
   它早于 gm-registry / window-hardening / gm-shutup / frame-drain / gm-host-seam / param-discovery / 宿主审计等今天全部合并。
   用旧二进制跑出来的对拍结论没有意义，**而且它不会报错**——只会给你一份看起来正常的报告。
   做法：**在你要起栈的那棵树里**跑 `cargo build --release -p mir2-server`，再跑一次 `-WhatIf`（命令见 §0.5 第 1 步），
   **必须看到 `\target\release\mir2-server.exe`**；看到 `\debug\` 就停下先构建（`start_mir2_server.ps1:285-290` 是 release 缺失**静默退 debug**）。
   `-WhatIf` 现在会打印 **`SERVER_EXE` / `SERVER_EXE_SHA256` / `SERVER_EXE_BUILD` 三行**（`start_mir2_server.ps1:381`）——**2026-10-24 订正**：原文写「只打印路径、不打印 sha256（`:349`）」已过期（`openmir2-start-server-whatif-sha` 之后工具升级）；把这三行写进本次 `README.txt`。
   **注意解析口径**：`SERVER_EXE` 相对**脚本所在仓库**解析（`:286` `$PSScriptRoot\..\..`），而 profile 的 `stack` 指向 `E:/tmp/conformance/start-rust.ps1`，
   后者把脚本路径**写死成主检出** ⇒ 在 worktree 里预览到的二进制可能与真窗口不同；**以起栈后 `mir2-server.pid.json` 的 `roles[].exe` 为准**（工具已对账，不符 rc=2）。
   （订正：原文写「核对它打印的 SERVER_EXE 路径与文件的 sha256——脚本不打印 sha」；**2026-10-24 复核**：脚本**已打印 sha256**（`:381`），但「`SERVER_EXE` 相对脚本所在仓库解析」这一点仍成立。）
   **下一次真窗口（`alt` profile）核对 pid 的文件就是
   `E:/tmp/conformance/rust-logs/mir2-server.pid.json`**（= `E:/tmp/conformance/start-rust.ps1` 的
   `-LogDir`，**不是** `<out>/rust/`）。该路径只由 profile 的 `sides.rust.pid_file` 声明，代码里没有兜底
   绝对路径；`card_window.py` 起栈后用它的 `roles[].exe` 与登记的 `mir2-server.exe` 对账，
   **不一致/缺文件 ⇒ rc=2 并点名角色或文件**。该文件是 Windows PowerShell 5
   `Set-Content -Encoding utf8` 写的，**带 UTF-8 BOM**，工具已按 `utf-8-sig` 读取。
   自查（只读，不用起栈）：

   ```sh
   python -I -c "import json;d=json.load(open('E:/tmp/conformance/rust-logs/mir2-server.pid.json',encoding='utf-8-sig'));print(d['roles'][0]['exe'])"
   ```
2. **端口**：`powershell -File E:\MirServer\port-status.ps1` 只查 `3306,5000,5100,5500,5600,5700,6000,7000,7100,7200`
   （脚本 `:38`）——那是**默认栈**的端口 + mysqld，**不是本窗口的端口**。正确的读法：
   - **3306 必须 UP**（mysqld 是外部依赖，工具显式不碰它）；
   - 5000–7200 若在监听 ⇒ 有默认栈在跑，先停掉（否则会占资源，但**不影响 alt 的 15xxx**）；
   - **本窗口要空的是 profile 的 `sides.<side>.ports`**：C# 11 个（15000,15100,15500,15600,15700,16000,17000,17100,17200,17201,17202）、
     Rust 9 个（15000,15100,15500,15600,15700,16000,17000,17100,17600）。`card_window.py` 起栈前 `check_ports_free`，占用即 rc=2 并点名 pid（`:1896` 判定 / `:1901` 报错）。
   （订正：原文「十端口必须全 DOWN」把默认栈的检查当成了本窗口的检查，且 3306 是必须 UP 的。）
3. **库基线复位**：C# 侧 `restore-baseline.sh`（`mir2_test_*`），Rust 侧 `restore-baseline-r.sh`（`mir2_test_*_r`）。**两套库不同名**，拿 `--db test` 给 Rust 侧拍快照会拍到 C# 的库 ⇒ 假差异。
4. **一次只跑一栈**：C# 与 Rust 用同一组备用端口，串行起停。
5. **场景用到的 ident 必须先在 `rules.json` 登记**（卡 `openmir2-conformance-rules-idents`，2026-10-23 起为硬前置）：
   比较器的口径是「未登记 ident 出现 ⇒ `UNCLASSIFIED` ⇒ **整次窗口 UNDECIDABLE(rc=2)**」——probe 跑得再绿也没用。
   开跑前自查（不起栈、不连库）：

   ```sh
   # 场景 wait_for / wait_frames / wait_idents（含 {"idents":[…]} 形态）用到的 ident
   # 与 rules.json 已登记 ident 的差集 —— 必须为空才开跑
   python -I -B -c "
   import json,glob
   ids=set()
   def walk(o):
       if isinstance(o,dict):
           for k,v in o.items():
               if k in ('wait_for','wait_frames'):
                   if isinstance(v,list): ids.update(x for x in v if isinstance(x,int))
                   elif isinstance(v,int): ids.add(v)
               elif k=='wait_idents':
                   if isinstance(v,dict): ids.update(x for x in (v.get('idents') or []) if isinstance(x,int))
                   elif isinstance(v,list): ids.update(x for x in v if isinstance(x,int))
                   elif isinstance(v,int): ids.add(v)
               else: walk(v)
       elif isinstance(o,list):
           [walk(x) for x in o]
   [walk(json.load(open(p,encoding='utf-8'))) for p in glob.glob('tools/probe/scenarios/*.json')]
   reg={r['ident'] for r in json.load(open('tools/conformance/rules.json',encoding='utf-8'))['rules']}
   print('missing:', sorted(ids-reg))"
   ```

   输出 `missing: []` 才开跑。新场景先用真实录制跑一遍 `compare_runs.py compare`（`coverage.unclassified_idents` 为空），
   再进窗口。批次 2 已补齐 `11/13/200/201/615/616/635/636/643/644/645/763/767` 共 13 个（`tools/conformance/README.md`
   「当前规则覆盖」有逐条 oracle 出处；其中 `11/13` 是 `compare + head_mask:[series]`——单客户端只看得到**怪物**走位，
   Dir 是 `RandomByte(8)` 随机，玩家自身走位方向的帧级判据**要第二观察者会话**才有）。

   **5.1 `pending_ignore_rule` 自查（同样会让整次窗口 UNDECIDABLE）**

   规则表里 `action=ignore` 且 `approved_by` 仍以 `PENDING` 开头的条目，**一旦真的剔了帧**，比较器判
   `UNDECIDABLE`（`compare_runs.py` 的 `pending_ignore_rule`）——这不是「放行」，是「这条忽略还没被批准，
   不该悄悄生效」。现网正是 `ident 14 SM_HIT`：它在真实录制里**很常见**（`w24-csharp-bagq` 3、`chouma` 1、
   `kaiz` 3、`kaiz30` 1、`w24-rust-login` 1、`w24-rust-dice*` 各 1、`529/rust` 1），**第 1 批的 `login_only`
   类窗口顺带抓到一条 14 就会整次判 UNDECIDABLE**。

   开窗前自检：看 `report.json` 的 `frames.ignored_ident_rules_pending`（是否含 14）、以及
   `undecidable[].kind == "pending_ignore_rule"`。处置只有两条：

   * **(a) owner 批准该 ignore 规则（推荐）**：它与 ident 10 的随机发点同源（`Merchant.Run` 的
     `Random(50)==0`、`GuildOfficial.Run` 的 `Random(30)==0`，墙钟播种 `src/OpenMir2/RandomNumber.cs:35`），
     529 双跑已有依据；批准后 `approved_by` 改为非 PENDING，报告即按 rule-driven ignore 处理。
   * **(b) 显式 `--ignore-idents 14`**：比较器把它当「已分类的显式处置」，报告登记 `CLI --ignore-idents`；
     可跑，但**每次窗口都要写明**，且**不替代 approve**。

   **绝不许**靠「换一次刚好没有 14 的运行」过关——14 是随机发点，那样得到的是 flake，不是结论
   （`docs/window-529-dualrun-20261023.md` 记过：最终 PASS 用的是没有 14 的那次运行）。

   **caveat（reviewer-2 F2）**：这条 ignore 也盖掉了 `Merchant.cs:152-157` 里 `FixedHideMode` 解除时**确定性**补发的
   那条 `SM_HIT`（:156）；需要判这条的场景（城堡商人在战中/战后切换）必须另找场景或构造级用例，不能指望本规则。

## 1.5 角色预置位：**声明即可**（工具已支持，2026-10-23 起）

以前这是操作者的手工步骤（`UPDATE characters SET MapName/CX/CY`），现在**不要再手工改库**：
在场景顶层声明需求，`card_window.py` 会在**每侧 restore 之后、起栈之前**自动完成。

```json
{
  "name": "npc_click_king",
  "placement": {"map": "0122", "x": 30, "y": 32, "note": "比奇国王 0122 (29,32) 的邻格"},
  "steps": ["..."]
}
```

- **为什么必须预置**：新角色出生点 = StartPoint `0 289 618`，而第 1 批依赖的四个 NPC 都不在图 0
  （`0122` 比奇国王 `(29,32)`、`GA2` `(13,27)`、`N109` `(38,43)`、`0150` `(16,8)`）；探针不发跨图移动
  ⇒ **漏了只会 click 超时、不报错**。
- **两侧各改各的库**：C# = `mir2_test_db`、Rust = `mir2_test_db_r`（`db.characters_db`）。
  拿错库会直接 rc=2（按该侧 `Server.conf` 的 `[DataBase] database=mir2_test_data*` 交叉核对）。
- **只改 `MapName`/`CX`/`CY`**：预置前后对**其余列做 MD5 指纹对比**，其它列变了即 rc=2
  （防 `characters_ablity.Weight` 那类连带写坏）。
- **fail-closed（rc=2 并点名，绝不让 probe 去超时）**：
  - 图不在 `<envir_root>/Envir/MapInfo.txt`，或该图的服务器号 ≠ 本侧 `[Server] ServerIndex`；
  - 坐标超出 `.map` 宽高，或**落在阻挡格**（oracle 登录期 `CanWalk(x,y,true)` 为假会随机挪 5 次再回
    HomeMap ⇒ 角色悄悄挪位，你只会看到点不到 NPC）；
  - 角色行不唯一 / `HomeMap` 为空 / `Deleted=1` / 表缺列；
  - 改完读回与目标不符。
- **已在目标位 ⇒ 不发 UPDATE**（`action=noop`），不会白写库。
- 证据：`<out>/<side>/placement.json`（声明值、前后值、其余列指纹、SQL），README/card-window.json 里也有。
- **不连库也能验**：`--dry-run` 会做离线内容校验（图存在、坐标在界内且可走）并打印预置命令。
- **背包夹具：声明即可**（2026-10-23 起，卡 `openmir2-window-inventory-fixture`）：场景顶层写 `inventory`，
  工具在 restore 之后、起栈之前把它写进**本侧测试库**；见 §1.6。

## 1.6 背包夹具：**声明即可**（工具已支持，2026-10-23 起）

「背包里必须已有某个 MakeIndex / 物品」这类前置**不再手工改库**：在场景 JSON 顶层写

```json
"inventory": [
  {"index": 100, "make_index": 5001, "position": 3, "std_mode": 0, "ani_count": 0, "dura": 0, "dura_max": 0}
]
```

- 必填 `index`（物品模板号）/ `make_index` / `position`（0..45 槽位）；`std_mode` / `ani_count` 声明时
  **必须与 `mir2_data.stditems` 模板一致**（不符 ⇒ 该侧 rc=2，防止「以为是药水结果造出武器」）；
- `position` 与 `make_index` 在同一声明里都必须唯一；未知键/类型错 ⇒ fail-closed；
- 写入列只有 `ChrName/LoginID/Index/MakeIndex/Position/Dura/DuraMax`；其余列出问题另判；
  **声明的槽位必须原本为空**（槽位已被占用 ⇒ rc=2：DELETE+INSERT 会把该行其它列重置成默认值，
  工具不替你做这件事）；
- 预置时机：每侧 `restore` 之后、起栈之前；两侧共用**同一份声明** ⇒ `make_index` 天然一致；
- 证据：`<out>/<side>/inventory.json`（声明、前后行数/指纹、SQL、rc）。

> 与 `placement` 同一处声明风格；不起栈的离线校验：`--dry-run` 会打印预置命令并做**离线内容校验**
> （槽位/键/std_mode），不合规直接 rc=2，不会起栈。

**已废弃的做法（2026-10-24 证伪）：不要把这个夹具写进 profile 顶层 `inventory`。**
profile 顶层没有白名单校验、该键会被**静默忽略**（跑起来像「没有夹具」，不报错）；唯一生效的位置是
**场景自描述的 `inventory`**。`--dry-run` 现在会为每侧打印一行
`背包夹具（dry-run 不连库）：db=… 声明 N 件 positions=… make_index=…`（2026-10-24 实测 `bag_equip`
两侧各 1 行、rc=0）——**看不到这一行就说明夹具没被声明**。

## 1.7 二进制与 sha256 的开窗前核对（卡 `openmir2-start-server-whatif-sha`，2026-10-23 晚；2026-10-24 由 §1.6 改为 §1.7——与「魔法夹具」互换编号，避免 §1.8 排在 §1.7 之前，见文末「已知行号漂移」）

`start_mir2_server.ps1 -WhatIf` 固定输出三行（实跑同款，只是多打印告警）：

```text
SERVER_EXE         <绝对路径>
SERVER_EXE_SHA256  <小写 hex；文件不存在时为 MISSING>
SERVER_EXE_BUILD   Release | Debug-Fallback | Explicit
```

标准核对命令（在**你将要起栈的那棵树**里跑；根解析只用脚本自身的 `$PSScriptRoot`，所以它指向**该树**的 `mir2-rs/target`，与 cwd 无关）：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools/m2-acceptance/start_mir2_server.ps1 `
  -Root 'E:/tmp/conformance/rust-root' -LogDir 'E:/tmp/conformance/rust-logs' `
  -PortsProfile 'E:/tmp/window/r5/root/alt-port-profile.json' -WhatIf
```

期望与判读：

| 输出 | 含义 | 该做什么 |
|---|---|---|
| `SERVER_EXE_BUILD Release` | 用的是 release 构建 | 手工比对 `SERVER_EXE_SHA256` 后，把该值写进本次 `README.txt` |
| `SERVER_EXE_BUILD Debug-Fallback` + `WARNING: release 二进制不存在…` | **release 缺失，已退到 debug** | 默认先停：在**这棵树**里 `cargo build --release -p mir2-server`；确实要跑 debug 才继续，并在证据里如实写 debug |
| `SERVER_EXE_BUILD Explicit` | 用 `-ServerExe` 指定 | 自己确认这就是你要的那份二进制 |
| `SERVER_EXE_SHA256 MISSING` + `RED: ServerExe 不存在`（rc=2） | 路径解析到了、文件不在 | 先构建，或用 `-ServerExe` 指到实际文件 |

**手工比对 sha256（任选其一）**：

```sh
sha256sum <SERVER_EXE 打印的路径>          # Git Bash / WSL
```

```powershell
# 本机 PowerShell 5.1 实测**没有 Get-FileHash**（CommandNotFound）⇒ 用 .NET：
$p = '<SERVER_EXE 打印的路径>'
$h = [System.Security.Cryptography.SHA256]::Create(); $fs = [System.IO.File]::OpenRead($p)
([System.BitConverter]::ToString($h.ComputeHash($fs)) -replace '-','').ToLowerInvariant()
```

**`-RequireRelease`（建议写进自动化）**：只要构建类型不是 `Release` 就 `RED` + 退出 2，把「跑错二进制」变成硬失败：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools/m2-acceptance/start_mir2_server.ps1 `
  -Root 'E:/tmp/conformance/rust-root' -LogDir 'E:/tmp/conformance/rust-logs' `
  -PortsProfile 'E:/tmp/window/r5/root/alt-port-profile.json' -WhatIf -RequireRelease
```

**Debug-Fallback 意味着什么**：优化关闭、构建时间未知，且**很可能不是当前 master**（本机 10-23 实测：release = 09:46、debug = 20:49，两者都不是「刚构建」）。默认批次仍以 **release** 为准；要拿 debug 当证据，必须在卡上写明「本次对拍用的是 debug 构建 + 该 sha256」，否则这份证据的可比性没有保证。

**不改真实启动行为**：上述告警与三行输出在非 `-WhatIf` 路径上同样打印；`-RequireRelease` 是**可选**开关，不加时行为与之前一致。

**注意（脚本能力之外）**：`E:/tmp/conformance/start-rust.ps1` 把脚本路径写死成主检出，因此**在 worktree 里 `-WhatIf` 看到的二进制，可能不是真窗口启动的那份**；真窗口以起栈后 `mir2-server.pid.json` 的 `roles[].exe` 为准（`card_window.py` 已对账，不符 rc=2）。

## 1.8 魔法夹具：**声明即可**（工具已支持，2026-10-23 晚；卡 `openmir2-window-magic-fixture`；2026-10-24 由 §1.7 改为 §1.8——与「二进制与 sha256」互换编号）

`spell_cast` 场景的 `blocked.missing_fixture` 要的「角色已学魔法」现在由 `card_window.py` 在
**每侧 restore（与预置位）之后、起栈之前**自动写进该侧测试库 `characters_magic`。
**不要再手工改库**（技能不是脚本动作、GM 权限恒 0 ⇒ 只能 DB 预置）。

**声明位置是 profile（不是场景）**——顶层 `magic` 应用到两侧；`sides.<side>.magic` 可覆盖本侧：

```json
{
  "format": "card-window-profile/1",
  "magic": [{"magic_id": 1, "level": 0, "use_key": "1", "curr_train": 0}],
  "sides": { "...": {} }
}
```

- 四列必填：`magic_id`（1..65535）、`level`（0..3）、`use_key`（单个 ASCII 可见字符）、
  `curr_train`（>=0）；未知键 / 类型错 / 越界 ⇒ **加载期 rc=2 点名**，一步都不执行。
- **跨栈不同的字段不进判据**：`PlayerId`（= 本侧 `characters.Id`）与行 `Id` 各侧自己定位；
  读回只比 `MagicId / Level / UseKey / CurrTrain`。
- **fail-closed**（每条都 rc=2 且**不起栈、不让 probe 去超时**）：角色不存在 / 角色行不唯一 /
  `Deleted=1`、同一 `MagicId` 有多行、写后读回不符、**未声明的魔法行**指纹（行数 + MD5）变化。
- 每侧证据落 `<out>/<side>/magic.json`：`declared` / `before` / `after` / `sql` /
  `action = noop | written`（一致就不写库；缺行或不一致才 `INSERT`/`UPDATE`）。
- `--dry-run`：只做离线校验（声明 + 库配对）并预览 SQL，**不连库**。
- 已知限制：`level` 上限按技能等级 0..3 校验；`use_key` 只允许单字符（线上 `char(2)`，
  经典客户端只用 1 字节）；`magic_id` 只做数值域校验，**不查 `mir2_data.magic` 表**是否存在该魔号
  ——写一个不存在的魔号不会报错，但场景也等不到施法帧（窗口里看 `GetMagicInfo` 的行为即可发现）。

## 1.9 内置 profile `window-testdb`：**5 个带预置位场景的测试库句柄**（2026-10-24；卡 `openmir2-window-profile-testdb-preset`）

`csharp-default` 的 C# 侧 DB 指向**生产库**（`preset=prod` / `characters_db=mir2_db`），因此下面
5 个**带 `placement` 预置位**的场景在 `--dry-run` 下就被生产库守卫 fail-closed（rc=2，**这是设计内的门，
不是 bug**）：`upgrade_retrieve` / `equip_struck_save` / `bag_equip` / `eat_item` / `npc_script_chain`。
报错原文：

```text
db 规格展开后命中生产库：prod(sides.csharp.db.preset), mir2_db(sides.csharp.db.characters_db)
```

`window-testdb` 是 `alt` 的深拷贝 + **C# 侧显式指向测试库**（`db.preset=test`、
`db.characters_db=mir2_test_db`；Rust 侧沿用 `mir2_test_db_r`）。它只让**输入不再命中**守卫：
**注意：今天就结果而言 `window-testdb` 与 `alt` 等价**——`alt` 的 C# 侧本来就写着同一对值
（`preset=test` / `mir2_test_db`），所以它**不会让任何场景「新通过」**，5 个预置位场景在 `alt`
下本来也是 rc=0。本 profile 的价值只是把「test-DB 契约」固化成独立、可命名的句柄（并把两键
显式写在构造器里，由 selftest 22b 钉住），不是改变任何行为；选 `alt` 还是它，结果一样。

- **守卫一字未动**：`check_profile_safety` 的 `prod(...)` 代码、报错文案、触发条件全部保持原样；
  本 profile 既没有 `--allow-production-db`，也没有 `production_db` 标记。
- **只加数据、不加判定**：改动局限于**模块级构造器与赋值**——`def _derived_testdb_profile(...)`
  （`copy.deepcopy` 基线 + 显式写 C# 侧两键）加 `BUILTIN_PROFILES["window-testdb"] =
  _derived_testdb_profile(...)`，与既有的 `window-magic` 同形；**不是**只往
  `BUILTIN_PROFILES` 里塞一段 dict 字面量，但也不改任何判定逻辑。
- **仅测试库，绝不可用于生产**：`db.characters_db=mir2_test_db` / `mir2_test_db_r` 是测试库；
  真跑会写测试库，**不要**把它当生产窗口 profile，也**不要**在它基础上改回 `mir2_db`。
- 用法：`--profile window-testdb`。`--dry-run` 下这 5 个场景 rc=0；`alt` / `csharp-default`
  的 18 场景 rc 逐格不变（`csharp-default` 下那 5 个**仍然 rc=2**，守卫还在）。
- 实测快照戳：`@ 7cc81fbfa @ 2026-10-24 11:04 +0800`（原测于 feature commit `633e2d8a1`，同一组数字在 tip `7cc81fbfa` 上复验一致；18 格矩阵与 5 个 rc=0 证据见
  `docs/window-scenario-dryrun-sweep-20261024.md` §1）。

## 2. 跑的时候

- **命令模板**（所有场景共用，只有前置不同）：

  ```sh
  python -I -B tools/conformance/card_window.py --card <卡 id> \
    --scenario tools/probe/scenarios/<场景>.json --out E:/tmp/window/<本次 run> \
    --profile <本次 profile> [--var NAME=VALUE ...] [--timeout-ms <ms>] [--post-comment]
  ```

  **不存在「所有场景都用 `--profile alt`」**：`spell_cast` 在 `alt` 下**加载期就 rc=2**
  （实测原文：`要求 magic 夹具，但 profile alt 未声明 magic 段`）；`combat_hit` / `combat_strike` 是封存场景，
  任何 profile 都 rc=2 `BLOCKED`（实测）。`--var` 可重复、两侧**原样**透传（实测 `--var npc_actor=12345`
  同时出现在两侧 probe 命令行）；非法形式（缺 `=` / 空名 / 空值 / 同名不同值）**加载期** rc=2 且点名。
  **口径**：`--var` 只**覆盖场景已声明**的变量（不新增变量）；值必须来自本次参数发现/会话，
  **不得沿用 `params` 旧默认值**（如 `dice_chain` 的 `npc_actor=119` 是旧抓包值）；两侧同值 ≠ 可复用。
  **注意 `csharp-default` 的定位是「核对生产库守卫」，不是「跑场景的默认 profile」**：带 `placement` 的
  5 个场景（`upgrade_retrieve` / `equip_struck_save` / `bag_equip` / `eat_item` / `npc_script_chain`）
  在它下面 `--dry-run` 即 rc=2（设计内 fail-closed）。要预览这 5 个，用 `--profile window-testdb`（§1.9，
  **仅测试库，绝不可用于生产**）；**今天它与 `alt` 等价**（同一份深拷贝 + 把 `alt` 里本就相同的
  C# 库名再显式写一遍），选哪个都行。
- **本节点到名字的场景，前置如下**（其余场景以场景 JSON 的 `requires` / `inventory` / `required` 为准，
  全量枚举见 `docs/window-batch-plan-20261023.md` §7.4（当前 head：**11 行 / 17 场景**；`81ecc0d28` 当时为 12 行 / 18 场景，pickup_item 封存后推算 11/17）；
  能不能跑的矩阵见 `docs/window-batch-rehearsal-20261024.md`：**17 可跑 / 3 blocked / 0 假绿**（该文档为当时快照 18 可跑/2 blocked，pickup_item 封存后推算 17/3）——本文不复写矩阵）：
  > **2026-10-25 勾注**：`pickup_item` 已封存（T-19/T-25 ⇒ 地面结构性无物，见 batch-plan §7.4 勾注）⇒
  > 场景数 **18→17**；run-3 执行清单 = `tools/window/run3-scenarios.json`（17 场景 + `tools/window/run_batch.sh`）。
  > **CLEARMAPMON 两处脚本**：`Market_Def/任务NPC/武士-0114.txt:145`（T118）与 `老人-1.txt:49`（Q001）
  > 仍走 host no-op（部署 2 处使用；需实现，建议 R）——真机跑这两张图时**清场不会发生**，
  > 判据不得假设怪已被清；证据 `docs/clearmapmon-hook-audit-20261025.md`。

  > **`blocked` 有两个来源，两处都要写才闭环（2026-10-25；卡 `openmir2-blocked-two-sources-doc`）**：
  > ① `tools/window/run3-scenarios.json` 的行级 `blocked` —— 管**批次排除**（`run_batch.sh --list`
  > 的 blocked 计数与入批选择都读它）；② 场景 JSON（`tools/probe/scenarios/<name>.json`）的顶层
  > `blocked` —— 管**直接跑 `card_window.py` 时的起栈前 BLOCKED（rc=2）**（`scenario_blocked` 读它）。
  > 只写 JSON：批次不会排除（`--list` 仍显示 blocked 0，真跑仍入批）；只写清单行：直接跑
  > `card_window` 仍会执行。现有例子：`clearmapmon_t118` 只在场景 JSON（且不在 run3 清单）；
  > `butch_two_draws` 两处都有（run3 #21 行级 blocked + 场景 JSON blocked）。

  | 场景 | 必带前置 | 不满足时（实测） |
  |---|---|---|
  | `dice_chain` | `--var npc_actor=<本次抓到的 actor id>`（场景 `params.npc_actor=119` 是旧抓包值，不在 `required` 里、**不会自动发现**） | 不带就点旧 actor（点空）；或改用 `dice_chain_auto`（自动发现） |
  | `dice_chain_auto` | 无（actor 自动发现） | — |
  | `bag_equip` / `eat_item` | **场景自描述 `inventory`**（已在场景 JSON 里，见 §1.6「背包夹具」）；场景 `required` 会自动触发 discovery（`bag_equip`：`item_name` / `make_index` / `takeon_where`；`eat_item`：`item_name` / `make_index`，**无** `takeon_where`） | 写 profile 顶层 `inventory` 无效（静默忽略） |
  | `upgrade_retrieve` | `probe_timeout_ms=720000`（profile 顶层）或 `--timeout-ms 720000`（实测两侧 probe 命令行均为 720000） | 默认 20000 ⇒ 180s 杀 probe，白跑一整轮 |
  | `spell_cast` | **可直接用内置 `window-magic`**（= `alt` + 两侧 `magic` + D-9 pin，见下）；或自定义 profile 两侧都写 `magic` 段 + `sides.*.env.MIR2_CLIENT_ABILITY_LAYOUT="0"` | `alt`/`csharp-default` ⇒ 加载期 rc=2（缺 magic）；不 pin D-9 ⇒ 53 的 `body_len` 0B vs 16B FAIL |
  | `combat_hit` / `combat_strike` | 无——**封存**（缺同屏怪物夹具与伤害夹具） | rc=2 `BLOCKED`；这是设计如此，不要当「工具坏了」去改 |
  | `login_only` | 探针不替操作者发客户端命令；卡面要求的手动输入（如 `@禁言 <名> 1`）必须由人在真实客户端做 | 漏了**不报错**，只会让判据看起来没红 |

  > **内置 `window-magic`（2026-10-24 @ `72c26da9f`）**：`--profile window-magic` = `alt` 的副本 + 顶层
  > `magic:[{magic_id:1,level:0,use_key:"1",curr_train:0}]` + 两侧 `env.MIR2_CLIENT_ABILITY_LAYOUT="0"`。
  > 实测：`spell_cast --dry-run --profile window-magic` ⇒ **rc=0**，两侧各打印 `魔法夹具（dry-run 不连库）… magic_ids=1`；
  > 既有 `alt`/`csharp-default` 下 18 个场景的 rc 逐格不变（alt 17×0 + spell 2；csharp-default 12×0 + 5×生产库守卫 + spell 2）。
  > `alt` 默认值**未被修改**（它是 18 场景的既有基线）。

- **两条硬提醒（照做，别省）**：
  1. **`--dry-run` 绿 ≠ 真机已对拍**：dry-run 不连库、不起栈，只预览命令序列；判据只看顶层 `verdict`
     （`PASS` / `FAIL` / `UNDECIDABLE`，见 §3），`UNDECIDABLE` 不是通过。
  2. **失败/中止时的停栈口径**：只停**本次窗口起的**两个栈进程；`card_window` 的停栈命令禁含
     `mysqld` / `Stop-Service` / `net stop` / `mysqladmin`，**不碰生产库、不停 mysqld**（3306 必须一直是 UP）。
     restore 只对 `mir2_test_db*` 测试库做，不要把测试库名改成生产库。
- R 卡窗口**必须** `--post-comment` 记账（在解禁之后）。
- **超时**：profile 键是 **`probe_timeout_ms`**（内置 alt 默认 `20000`，`card_window.py:297`），子进程总超时 = `max(180, probe_timeout_ms/1000 + 120)` 秒（`:1949`）。
  ⇒ **默认 180s 就杀 probe**。`tools/probe/scenarios/upgrade_retrieve.json` 第 9 步是 `{"sleep": {"ms": 660000}}` ⇒ 该场景必须把
  `probe_timeout_ms` 设到 **720000**（profile JSON 顶层，或命令行 `--timeout-ms 720000`，`:696-697` 等价）。
  **在 profile 里写 `"timeout_ms"` 会被静默忽略**（profile 顶层没有白名单校验）——这正是「白跑一轮」的经典成因。
- **参数发现**：11 个场景声明了 `required`（`npc_actor`/`merchant_actor`/`make_index`/自身坐标…；2026-10-24 实测：`bag_equip`、`combat_hit`、`eat_item`、`king_castle_war`、`king_dialog`、`merchant_dialog`、`move_walk`、`npc_click`、`npc_script_chain`、`pickup_item`、`upgrade_retrieve`）。两侧**各自发现各自的值**，某侧推不出 ⇒ fail-closed，不许拿一侧的值跑另一侧。
- **封存提醒（2026-10-24，卡 `openmir2-pickup-item-reachability`）**：`pickup_item` 的参数发现能力仍在，但场景已因 T-19/T-25（`MapItem.ItemId` 恒 0 ⇒ 地面永远无物）封存为「结构性不可判」，`--dry-run` 按设计 rc=2 BLOCKED（不起栈）；不要把它排进窗口，除非 oracle 出现新的、可复跑证明的 `ItemId` 赋值生产路径。
- **手动步骤不要漏**：`login_only.json` 这类**不含发客户端命令的步骤**。若卡面要求「客户端直呼 `@禁言 <名> 1`」之类的探针，必须由操作者在真实客户端手动输入，脚本不会替你做。漏了不会报错，只会让这条判据看起来没红。
  **角色预置位不再是手动步骤**：场景声明 `placement` 即可（见 §1.5），`card_window.py` 在起栈前自动改库并在 `placement.json` 留证；预置不成就 rc=2，不会带着错位去跑。

## 3. 读结果（最容易错的地方）

- **只看顶层 `verdict`，不要扫 body 里出现过的 `PASS`**。三态：`PASS` / `FAIL` / `UNDECIDABLE`；`UNDECIDABLE` **不是通过**。
- `verdict_reason` 与 `blockers` 写明为什么不是 PASS（如 `evidence_incomplete: README.txt write failed`、`compare_fail`）。
- compare 的帧数/库后态/差异计数在**嵌套 `compare{}`** 里，顶层没有这些字段。
- 证据包必须齐：`report.json` + `README.txt` + `card-window.json` + 两侧 `frames.ndjson`。缺件时工具 rc=2、comment 记 `UNDECIDABLE`。
- **`compare_rc` 的 2 是 UNDECIDABLE，不是绿**。
- **外层 rc 与 `compare_rc` 是两个口径（run-2 误读教训，2026-10-25 勾注）**：
  - **外层 rc** = `card_window.py` 进程退出码：`0` = 完整 PASS；`2` = 有结论但不通过 / UNDECIDABLE（超时被杀为 `124`/`137`）。它**不区分** FAIL 与 UNDECIDABLE。
  - **`compare_rc`** = `compare_runs.py` 的结论：`0` = 对拍全绿 / `1` = 抓到真差异（FAIL）/ `2` = 判不了（UNDECIDABLE）。
  - 读法：外层 rc `0` 只代表 PASS；外层 `2` 必须再看 `compare_rc`（`1`=FAIL、`2`/`null`=UNDECIDABLE）；
    **绝不能用外层 rc 0 掩盖 `compare_rc` 非 0**。`tools/window/run_batch.sh` 的 `outer_rc` / `compare_rc`
    双列同此口径（`batch.log` / `batch.jsonl`）。

## 4. 窗口项本身的自检（跑之前 30 秒）

- `grep -n 'ident=' docs/ <卡相关文档>`：**帧号写错没有机器守卫**——今天出过 `ident=769`（实为 `SM_GUILDMAKEALLY_FAIL`）而真帧是 `767`（`SM_MENU_OK`）的事故。写窗口项时**帧号必须同时给 `SM_` 名字**。
- 场景文件里 `--var` 的值必须来自**发现**，不是 `params` 里的默认值（`dice_chain` 的 `npc_actor=119` 就是硬编码默认值，不计入发现能力）。
- **规则表覆盖**：场景 `wait_for` 里出现的每个 ident 都必须在 `rules.json` 有条目（§1 第 5 条的自查），
  否则跑出来的不是 FAIL 而是 `UNDECIDABLE`——**UNDECIDABLE 不是通过**（§3）。

## 4.5 参数发现覆盖 14/15 —— 决定窗口战役能先推多少

> 2026-10-23 更新（卡 `openmir2-window-discovery-rules`）：规则表 4 条 → **5 条规则**（+ `npc_actor` 一条退化规则），另加 **1 类 computed**。
>
> 2026-10-23 晚更新（卡 `openmir2-discovery-bagitems-201`）：新增 **`SM_BAGITEMS(201)` 规则族**
> （`make_index` / `item_name` / `takeon_where`，三变量取自**同一个**物品槽）⇒ 覆盖 12/15 → **14/15**。
> **注意**：该规则目前只有**构造级**证据（合成 201 帧 + oracle 布局），现有 **798 份** `frames.ndjson`（`E:/tmp` + `E:/MirServer` 全扫）里**一条 201 都没有**
> （登录期背包为空 ⇒ oracle `ClientQueryBagItems` 不发帧），真机验证要等一次**非空背包**的录制。

`param_discovery.py` 的规则表有 **5 条规则**：`self_x` / `self_y`（SM_LOGON 的 param/tag）、
`merchant_actor` / `npc_actor` / `king_actor`（SM_MERCHANTSAY(643).recog；`npc_actor` 另有一条 `SM_TURN` 退化规则），
外加第三类 **`computed`**（`move_x` / `move_y`：由 `self_x`/`self_y` + **场景 JSON 里显式声明的 `delta`** 算出）。
三栏**分开计数**，`kind` 分别是 `derived` / `rule` / `computed`：

| 栏 | 含义 | 计入「发现能力已验证」 |
|---|---|---|
| `derived` | 帧语义直接推出 | ✅ |
| `rule` | 退化规则 = 部署假设（点错就响亮超时） | ❌ |
| `computed` | 场景声明 + 已推导量的**确定性算术**（不是猜值） | ❌（另外记录，见 `discovered.json` 的 `kind`） |

对照 15 个场景的 `required`（2026-10-23 实测；201 规则落地前 `--allow-missing=3` 时 rc=0，
201 规则落地后构造级帧上 **`--allow-missing=1` 时 rc=0**，只剩 `combat_hit`）：

| 场景 | required | 发现能否覆盖 |
|---|---|---|
| `merchant_dialog` | merchant_actor | ✅（derived，643） |
| `npc_click` | npc_actor | ✅（derived，643） |
| `pickup_item` | self_x, self_y | ✅（derived，SM_LOGON）〔2026-10-24 勾注，卡 `openmir2-pickup-item-reachability`：场景已封存为结构性不可判（T-19/T-25），`card_window --dry-run` 按设计 rc=2 BLOCKED，不再作为窗口输入；见 `docs/pickup-item-reachability-20261024.md`〕 |
| `king_dialog` / `king_castle_war` | king_actor | ⚠️ **rule（部署假设，不计入发现能力）**：643 只证明「该 actor 应答了点击」，**不证明是国王**——仓库里全部 643 录制都是赌场庄家（`〖庄〗…@kaiz`），且默认 discovery 点的是「最近的非自己 `SM_TURN`」（录制里就是庄家），**没有可执行步骤能让 discovery 点中国王**。点错的后果是末步响亮超时（`@@buildguildnow` 只存在于`比奇国王-0122.txt`，查不到 label ⇒ 零帧）。**〔2026-10-24 勾注，卡 `openmir2-window-deploy-prereq-batch2`〕「点中国王」已由 placement 闭合，不需要新 discovery 步骤**：discovery probe 与真场景共用同一角色（restore→placement→起栈→discovery→probe，`card_window.py` run_side 同栈两跑），场景声明 `placement(0122,29,33)` 后 discovery 也落在王宫 (29,33)；0122 全图唯一 actor=国王（Npcs.txt 仅 :3 一行 0122、MerChant/MonGen 零行，两 Envir 逐字节同源）⇒ 点出的 643.recog 必然是国王，rule 从「赌运气」降为「可离线机判的部署假设」。场景侧同步更新：`king_dialog` 末步期望改 [763]（GuildOfficial.ReQuestBuildGuild 失败支，旧 [643,100] 无生产点）、`king_castle_war` 改走 @requestcastlewarA→@requestcastlewar 缴金条门（644+767） |
| `move_walk` | move_x, move_y | ✅（**computed**：`self_x+1` / `self_y+0`，delta 写在场景 JSON 的 `computed` 里） |
| `bag_equip` | make_index, takeon_where, item_name | ✅（**derived，201**）：三变量取自同一槽——`MakeIndex`@100 / `Name`@0（帧里就有，不查 StdItems）/ `StdMode`@15 → `ItemLocation`（`M2Share.CheckUserItems` 逆表） |
| `eat_item` | make_index, item_name | ✅（**derived，201**）：同一个「首个可食用槽」的 `MakeIndex`@100 + `Name`@0 |
| `combat_hit` | target_x, target_y, hit_dir | ❌ **需新帧/新场景**：目标是**实体**，当前录制里没有任何一帧报出「附近怪物在第几格」。已知的唯一近似来源 `SM_TURN(10).param/tag` 是**转身者自己**的坐标（`BaseObject.cs:1375 SendRefMsg(RM_TURN, Dir, CurrX, CurrY, …)`），且帧里不区分怪物/NPC ⇒ 不能拿它冒充。缺的是一帧「AOI 出现/实体位置」且带种族或对象类型的字段；`hit_dir` 还要能算 `GetNextDirection(self, target)`（`M2Share.cs:299`） |
| `dice_chain` / `dice_chain_auto` / `gm_batch` / `gm_time` / `login_only` / `npc_click_auto` | 无 required | 直接可跑（6 个，已实测 `--check-scenario` 全 rc=0；`dice_chain` 绿来自 `params` 硬编码默认值，**不算发现**） |

⇒ **规则表落地后，`check-all` 能覆盖 14 个场景**（5 derived + 2 rule + 1 computed + 5 no_required + 1 params_default）；
其中 `king_*`（2 个）与 `move_walk`（1 个）由 `openmir2-window-discovery-rules` 解锁，
`bag_equip` / `eat_item`（2 个）由 `openmir2-discovery-bagitems-201` 解锁（构造级，见 §4.5.1）。`move_walk` 的 `computed` 已接进 `card_window.py`（见下方「接线」）；`king_*` **能跑但不该算绿**——`king_actor` 是部署假设（rule），
默认 discovery 推出来的是庄家，真跑会在末步超时（响亮失败），要让它真绿必须先有「点中国王」的 discovery 场景/步骤。〔2026-10-24 勾注：该前提已由批 2 的 `placement(0122,29,33)` + 0122 单 actor 证明闭合——discovery 与真场景同角色同摆位，见 §4.5 表内勾注与卡 `openmir2-window-deploy-prereq-batch2`。〕
只剩 `combat_hit`（1 个）要等「AOI 实体位置 + 对象类型」的新帧。
所以「②45 次参数化 run」**第一批按这 14 个推**，
其余按「缺哪个变量 → 补哪条规则/哪个帧」逐条开卡，不要笼统地指望发现工具。

`computed` 的判据形态（复审要能复算）：

- 声明在场景 JSON 顶层：`"computed": {"move_x": {"derived_from": "self_x", "delta": 1}}`；
  **delta 必须显式写出**——缺 `delta` / 类型不对 / 键不在 `required` / `derived_from` 指向另一个 computed
  一律按**场景坏掉**处理（`cls=error`，rc=2），不是「缺变量」。
- 基准量推不出 ⇒ **连锁 fail-closed**（`move_x`/`move_y` 一起进 `missing`），绝不拿 0 顶替。
- 没有 `computed` 声明（单变量推导入口）⇒ 该类变量照旧 missing。
- `mir2_probe` 不读 `computed`（声明性注记，不是步骤 DSL），但顶层可选键放行，否则带声明的场景会被 `unknown key(s)` 拒绝。

> **计数快照（`4e0bfd2b8` @ 22:21:57；实跑于 22:2x）**：场景已增至 **19** 个，
> `check-all` ⇒ `SUMMARY scenarios=19 ok=16 rc2=3 derived=5 rule=2 computed=1 no_required=5 params_default=3 missing=3 error=0`（**rc=2**）。
> 真实录制（`E:/tmp/conformance/runs/w24-csharp-npc_auto/frames.ndjson`）**没有 201** ⇒ `bag_equip`/`eat_item` 仍 missing，`--allow-missing=1` 也是 rc=2（`--allow-missing=3` 才可能 0）。
> 本节正文里的「12 个 / 14 个 / 15 个 / params_default=1」都是**更早的口径**，保留作对照，不要当现值。

### 4.5.1 `SM_BAGITEMS(201)` 规则族（卡 `openmir2-discovery-bagitems-201`）

**oracle 帧体**（`src/M2Server/Player/PlayObject.Operate.cs:38-64 ClientQueryBagItems()`）：

```
header: MakeMessage(SM_BAGITEMS(201), ActorId, 0, 0, ItemList.Count)   // series = 背包物品数
body  : EDCode.EncodeBuffer(ClientItem) + '/'  × ItemList.Count        // 逐项、无分隔前缀
```

每条 `ClientItem` 记录是**定长 124B**（`src/OpenMir2/Packets/ClientPackets/ClientItem.cs WritePacket`），
关键偏移：`Name`@0（`WriteAsciiString(ItemNameLen=14)` = 1B 长度 + 14B 定长 GBK）、`StdMode`@15、
`MakeIndex`@100（i32 LE）。**帧里没有 StdItem 的 `Index`**（客户端靠 Name 识别物品；服务端
`ClientTakeOnItems`/`ClientUseItems` 也是 MakeIndex + GetItemName 双匹配）⇒ 「MakeIndex 取成 Index」
在线上不可能发生，`item_name` **不需要查 `StdItems` 表**。旁证：M3 实测 `SM_BAGITEMS body_len=668`
= 4 × (166B 编码 + 1B `'/'`)，与 124B 记录一致。

**取哪个物品（显式、确定、不随机）**：只看**首条** 201（槽序 = `ItemList` 序）：
`required` 含 `takeon_where` ⇒ **首个可装备槽**（`StdMode` 在 `M2Share.CheckUserItems` 槽表里）；
否则 ⇒ **可食用槽**（`StdMode ∈ {0,1,2,3,4,31}`，`ClientUseItems` 的 switch 分支），并按**固定价值次序**
`StdMode31 & AniCount!=0`（QFunction `@StdModeFunc<N>` 有脚本，exec-seams ② 路的判据包）→
`StdMode31 & AniCount==0`（解包）→ 药（0/1/2/3）→ 书（4），同档次序取最小槽。
**`AniCount` 在帧里**（记录偏移 18），所以这条优先级不需要额外信息。
`make_index`/`item_name`/`takeon_where` 一次 derive 只选一次 ⇒ **三个 `--var` 必然同槽**。
无候选 ⇒ missing（不退回「随便取一个」）。

**`takeon_where` 的来源**：`StdMode` → `ItemLocation` 逆表（`M2Share.CheckUserItems`，
`src/M2Server/M2Share.cs:376-458`）；同 StdMode 多槽时取**最小槽号**
（24/25/26 → ArmRingl(5)，25 虽然也能进 Bujuk(9)）。CM_TAKEONITEM 的 `nParam2` 就是这个槽号
（`PlayObject.Message.cs:820-821`）。

**局限（真机前必读）**：
1. **待真实录制验证**：现有录制无 201 ⇒ 规则只有构造级证据；真机需要一个**非空背包**的 discovery run
   （背包夹具：场景声明 `inventory` 即可，`card_window.py` 已接，见 §1.6）。
2. **跨侧一致性**：`make_index` **不在** `card_window.py` 的 `SESSION_SCOPED_VARS` 里 ⇒ 两侧 `MakeIndex`
   不同会被判 `value_mismatch` rc=2。两侧库由同一基线克隆且 `characters_bagitem.MakeIndex` 是从库读回的
   （`PlayDataStorage.Query.cs:404-425`）时两侧相同；若某侧在会话里**新造**物品（GM/掉落）则两侧必然不同，
   那时要么把 `make_index` 加进会话作用域（改 `card_window.py`，另卡），要么先做背包夹具对齐。

**接线（`card_window.py`，已完成）**：`scenario_computed()` 读场景的 `computed` 声明 → `discovery_cfg`/`side_ctx`
携带 → `derive_from_frames(frames, required, computed)` → 两处调用点（真跑 `run_side` + `--dry-run` 回放）透传。
证据：`card_window --selftest` 的 DISC-9（两侧 `self_x` 钉 7 ⇒ 两侧 `--var move_x=8 move_y=12`、
`discovered.json` 记 `kind=computed`、缺 `delta` 加载期 `ConfigError`）；以及离线预览
`--dry-run --discovery-frames E:/tmp/conformance/runs/w24-csharp-npc_auto/frames.ndjson` 两侧 probe 命令都带
`--var move_x=8 --var move_y=12`（不起栈、不建目录）。

> 计数订正：本表初稿写「9 个场景（3 可发现 + 6 无 required）」，是规则表 4 条时的口径；
> 规则扩到 5 条 + computed 后实测为 **12 个**（`SUMMARY ... ok=12 rc2=3 derived=3 rule=2 computed=1
> no_required=5 params_default=1 missing=3 error=0`）。复审 `1e9f6315` 指出 king 记 derived 与本文件的分栏定义冲突，已改判 **rule**（`derived` 5→3、`rule` 0→2），`king_*` 不再计入「发现能力已验证」。
>
> 再订正（`openmir2-discovery-bagitems-201`）：201 规则族落地后，**同一个真实录制**
> （`E:/tmp/conformance/runs/w24-csharp-npc_auto/frames.ndjson`，**无 201**）仍是
> `ok=12 rc2=3 missing=3`——这是**正确**的 fail-closed（没有 201 就推不出）；
> 把一条按 oracle 布局合成的 201 帧（4 项背包同理，`body_len=334` 对应 2 项）追加到同一录制的**副本**后，
> 实测 `SUMMARY ... ok=14 rc2=1 derived=5 rule=2 computed=1 no_required=5 params_default=1 missing=1 error=0`，
> `bag_equip` 得 `item_name=屠龙,make_index=123456,takeon_where=1`、`eat_item` 得
> `item_name=金创药,make_index=9001`。**真实录制仍待补**（见 §4.5.1 局限 1）。

## 4.6 脚本动作参数里的 `<$…>` **不会被展开**（2026-10-23 晚实测，写窗口项前必读）

oracle 的 `ActionOfGmExecute`（以及多数脚本动作）**直接拼 `sParam1..6`，不调 `GetLineVariableText`** ⇒
脚本里写 `Gmexecute 踢人 <$STR(S6)>` 时，命令收到的是**字面量 `<$STR(S6)>`**，不是展开值。

当天三张 GM 卡的窗口判据全部建立在这个错误前提上，已全部打回重写。**写任何 GM/脚本动作窗口项前，先算出"命令实际收到什么"**：
- 若部署调用点传的是 `<$STR(Sn)>` / `<$USERNAME>` ⇒ 真机看到的是**检测失败**（如「<$STR(S6)> 现在不在线」红字），不是成功分支；
- 只有调用点传**字面量**（如 `Gmexecute 改属性 78`）时，命令才拿到真值。

同理：`SENDMSG` 的文本走的是 `GetLineVariableText`（**会**展开命名变量？**不会**——`IsStringNumber` 的 `||` 缺陷让命名变量分支全是死代码，见 whitelist B-9；只有数字码 `<$N>` 可达）。

## 5. 跑完

- 写 `--post-comment`（解禁后）+ 卡的 `status` 里写明：哪几个窗口项已验、证据路径、两侧二进制 sha256、以及**没验到的项和原因**。
- R 卡窗口全验完才 `closed`；只验了一部分就保持 `in-progress`，并在 `work` 里写明剩余项。

## 附录：已知行号漂移（2026-10-24 登记；正文既有行号**本卡不改**）

> 下表是 `card_window.py` 的**既有**行号引用（非本卡引入），正文保持原样；按行号去找代码前先查本表。
> 行号随工具提交持续漂移，**优先按函数/键名检索**（右列锚点）。

| 正文里的旧行号 | 当前实际行号 | 锚点（grep 这个名字/片段） |
|---|---|---|
| `:297` | `:310` | `BUILTIN_PROFILES['alt']` 的 `"probe_timeout_ms": 20000` |
| `:1949` | `:2814` | `probe_timeout = max(180, int(prof.get("probe_timeout_ms", 20000) / 1000) + 120)` |
| `:696-697` | `:713` | `apply_overrides()` 里的 `prof["probe_timeout_ms"] = cfg["timeout_ms"]` |
| `:1896`（判定） | `:528` 定义、`:2761` 调用 | `def check_ports_free(ports)` / `busy = check_ports_free(sc.get("ports") or [])` |
| `:1901`（报错） | `:2766` | `res["error"] = ("起栈前端口已被占用…` |

> **节号也漂过**：原「二进制与 sha256」节与本文件「背包夹具」节同为 `## 1.6`；2026-10-24 起「背包夹具」保留
> §1.6，「二进制与 sha256」改为 **§1.7**、原「魔法夹具」由 §1.7 改为 **§1.8**（两者互换，避免 §1.8 排在 §1.7 之前；
> `docs/docs-state-drift-sweep-20261024.md:65` 所称的「§1.6 三行」= 该节）。

> **悬空引用（不算行号漂移）**：§0.5 第 1 步原写「见 §1.1 的解析口径」，但本文件**没有** `## 1.1`（也无 `### 1.1`）；
> 被指的「注意解析口径」实际在 **§1 第 1 步**（`## 1. 开跑前必做` 下第 1 条的加粗小标题）。2026-10-24 已就地
> 改为「见 §1 第 1 步『注意解析口径』」——这是**悬空引用**，不是行号漂移，故单列在此。

## 6. 故障期卫生门禁口径：**带 ref 的两项必须显式 `--ref master`**（2026-10-25；卡 `openmir2-runbook-stale-ref-master`）

> 背景（tester-1 2026-10-25 实证）：walgit 故障期 `refs/remotes/walgit/master` **冻结**在 `4ae3dc810`。
> `stale_pointer_gate` 默认取这个远端 ref 做祖先判定 ⇒ 会把**当前本地 head** 的快照声明误拒
> （`f08496192` 上实测 1 处红；同一命令加 `--ref master` 即绿）。

**① 口径（按 `--ref` 行为分三类；7 项卫生门禁 + 1 项台账门）**

| 门 | `--ref` 行为 | 故障期口径 |
|---|---|---|
| `tools/hygiene/stale_pointer_gate.py` | **接受**（默认 `refs/remotes/walgit/master`） | 显式 `--ref master` |
| `tools/hygiene/dynamic_count_stamp_gate.py` | **接受** | 显式 `--ref master` |
| `tools/whitelist/t_reservation_gate.py` | 硬传 **rc=2**（argparse 子命令 `unrecognized arguments`） | 不传 |
| `mir2-rs/tests/parity/m3/whitelist_anchor_gate.py` | 硬传 **rc=2**（同上） | 不传 |
| `mir2-rs/tests/parity/m3/m3_whitelist_registry_audit.py` | 硬传 **rc=2**（同上） | 不传 |
| `tools/hygiene/board_coverage_gate.py` | 硬传 **rc=2**（`unknown argument '--ref'`） | 不传 |
| `tools/hygiene/conflict_marker_gate.py` | **静默忽略（rc=0 假绿）**：只认 `-h/--help`/`--selftest`，其余 argv 直接忽略 | 不传；默认 ref 口径靠 ② 确认 |
| `mir2-rs/tests/parity/coverage/script_host_noop_audit.py`（台账门） | **静默忽略（rc=0 假绿）**：`check` 前先丢弃所有 `--` 开头的 token | 不传；同上 |

⇒ 三类行为**不能互相替代**：① 2 项接受 `--ref`；② 4 项硬传 rc=2（能看见报错）；③ **2 项静默忽略**——
传了照样 rc=0，是**假绿**（比报错危险：会让人误以为 ref 已钉到本地 master）。故对 ②③ 两类，
故障期的唯一正确做法是 **不传 `--ref`**，并用 ② 的 `git rev-parse` 对照确认**默认 ref 已指向本地
`master`**（CI 的 `refs` 字段只决定任务对哪些 ref 生效，**不改变**门的 `--ref` 默认解析——仍是冻结的 `refs/remotes/walgit/master` ⇒ 判定建立在冻结 ref 上，结论可能失真（可能翻红也可能误绿），仍需按本地 master 手动复核）。恢复默认口径后（③）不要给这两门补 `--ref`，它不会生效。

**② 可复算对照（跑门之前 1 秒判一次）**

```sh
git rev-parse master refs/remotes/walgit/master
```

- 两 sha **相同** ⇒ 默认口径与 `--ref master` 等价，按 CI 原样跑（无需显式 ref）；
- 两 sha **不同**（故障期常态）⇒ 两项 ref 门必须带 `--ref master`，否则判定基准是冻结的旧远端。

**③ 恢复后回归默认**

`git fetch walgit master` 成功且 `refs/remotes/walgit/master` 与本地 `master` 同 sha 之后，**去掉**
`--ref master`，回到 CI（`.walgit/ci.toml`）的默认口径；不要把 `--ref master` 长期写进脚本/文档命令。

**故障期整套（可直接复制；`--ref` 只出现在前两条）**

```sh
python -I tools/whitelist/t_reservation_gate.py check
python -I tools/hygiene/conflict_marker_gate.py
python -I -B tools/hygiene/stale_pointer_gate.py --ref master
python -I mir2-rs/tests/parity/m3/whitelist_anchor_gate.py check
python -I mir2-rs/tests/parity/m3/m3_whitelist_registry_audit.py audit
python -I -B tools/hygiene/dynamic_count_stamp_gate.py --ref master
python -I -B tools/hygiene/board_coverage_gate.py --board .walgit/board.toml
python -I mir2-rs/tests/parity/coverage/script_host_noop_audit.py check   # 台账门（静默忽略 --ref）
python -I -B tools/hygiene/merge_tree_helper.py --selftest               # CI 第 8 项：不吃 --ref，resolve_master 自解到本地 master
git diff --check
```

**判据（本卡实测，master=`263e77d7e`）**：`stale_pointer_gate --ref master` ⇒ rc=0（`ref=263e77d7e…`，
0 处红）；`dynamic_count_stamp_gate --ref master` ⇒ rc=0（0 缺戳）；`conflict_marker_gate` ⇒ rc=0；
`git diff --check` ⇒ rc=0。

---

## 7. 工具自测也是合并门禁的一部分（2026-10-25；卡 `openmir2-tool-selftest-gate`）

**由来（真实漏网）**：`openmir2-db-makeindex-mask` 合入后，master 上 `card_window.py --selftest` 是
**412/435**，而 `goal_gate` 七项全绿——因为**工具自测从来不在合并门禁里**。工具自己坏了，后面所有
「对拍全绿 / 门禁全绿」都可能是假绿，所以工具自测与 fmt / clippy / msgcodegen / cargo test 同级。

**一条命令**：

```sh
python -I -B tools/hygiene/tool_selftest_gate.py                       # 全量；rc=0 全绿，rc=2 有红/用法错
python -I -B tools/hygiene/tool_selftest_gate.py --only card_window    # 只跑一项（排障/变异自证）
python -I -B tools/hygiene/tool_selftest_gate.py --skip <name,...>     # 显式跳过（必打印 SKIP）
python -I -B tools/hygiene/tool_selftest_gate.py --inventory           # 清点：谁在门内/出域，出域带理由
```

**判定域**（写清判定域才不会假绿）：

- 扫什么：脚本内 `REGISTRY` 的**固定 44 项**（conformance 驱动器、probe、db 对拍、窗口工具、网关抓包、
  M1/M2 验收、日志差分、世界采样、botload、本仓库 hygiene 门自身，以及本门自己）；
- 在哪跑：仓库根（`--repo`，默认本脚本上溯两级），`python -I -B`，**每项独立子进程**；
- rc 从哪取：**子进程退出码**（不经管道/`tail`，否则拿到的是 `tail` 的 rc）；
- 判红：任一注册项 rc≠0 ⇒ 本门 rc=2；全 0 ⇒ rc=0；**用法错（未知条目 / 参数）⇒ rc=2**；
- `--skip <name,...>` **必须打印 `SKIP`**（显式跳过，不许静默放水）；`--only` 只跑指定项；
- **门自己必须先能超时退出**（2026-10-25 实证 pid 1772 挂住 30+ 分钟）：
  - 单项超时 ⇒ 按**进程树**终止（Windows `taskkill /T /F`，POSIX `killpg`）再 `communicate()` 兜底 10s；
    只 kill 直接子进程时，孙进程继承 stdout ⇒ 超时路径的第二次 `communicate()` **永远等 EOF**；
  - **总墙钟** `--total-timeout`（默认 **420s** < CI 480s）：超时后未跑的项判 `NOT-RUN/rc=124` 并**点名**；
  - 牙：**TSG7**（挂住项 ⇒ 2.2s rc=2 点名，断言 <8s）、**TSG9**（隐藏开关 `--selftest-no-tree-kill`
    只杀直接子进程 ⇒ 断言 10s ≤ 耗时 <30s 返回 rc=2、孙进程已清理）、**TSG8**（总墙钟 3s ⇒ 点名滞留项）、
    **TSG10**（隐藏开关在非自测路径被忽略 + WARN，防止「牙自己变成漏洞」）；
- **不是 hermetic（F2，reviewer-21）**：`dbsnap` 项的 selftest 会连**本机 MySQL**，读
  `mir2_account.*` / `mir2_db.*` 共 13 张表，`mysql.exe` 路径硬编码（`D:\mysql\…`）。因此
  **「找不到 mysql 客户端 / 连不上 127.0.0.1:3306」导致的红属于环境，不是工具回归**；CI 每小时在
  master 上跑也继承这个依赖——runner 机器没有这套 MySQL 时，该项必红，要么给 runner 配库、要么后续
  改合成夹具（另开卡）。判定方法：看该行 tail 是 `mysql`/连接类错误还是 `RED: … selftest` 断言类错误。

**收哪些、不收哪些**（`--inventory` 可看全量）：**只要自带 selftest 且能离线判红，就收**——不按
「文件在哪个目录 / 是不是别域 / 是不是一次性脚本」筛。Leader 2026-10-25 裁定：
**「一次性 / 人工使用 / 别域」都不是合法出域理由**，只有「**需要交互式输入、外部产物或真实环境**」才算。
出域仅 **5** 项（49 = 44 门内 + 5 出域 + 0 未分类），逐项有实跑证据：

| 出域项 | 真实原因（实跑证据） |
|---|---|
| `client/dlg_links.py` | selftest 需外部截图 `E:/tmp/window/r7/shot-dice5.png`（人产出）；缺失时打印 **SKIP 且 rc=0** ⇒ 接入会永远假绿 |
| `window/alt_port_profile.py` | `selftest` 需要**主检出构建产物/部署配置** `src/DBSrv/bin/Release/dbsvr.conf`；缺失 ⇒ `RED: db-srv: 找不到源配置…` rc=1（环境相关，不是「只打印」） |
| `capture/golden_fresh_check.py` | 需要先 `dotnet build` 出 `GoldenExport.exe`（实测 rc=2「前置不满足」） |
| `conformance/selftest_impl.py` | 无独立 CLI 入口（`--selftest` 空转 rc=0、无输出）；它是 `compare_runs selftest` 的实现模块，已被 compare_runs 覆盖 |
| `db-parity/newchr_delchr_selchr_probe.py` | 需要真实 MySQL 与建/删/选角色状态（真机） |

**本机实测（2026-10-25，44 项）**：

| 项 | 结果 |
|---|---|
| 全量 | **绿 44 / 红 0 / 跳过 0，rc=0，111s**（`E:/tmp/final-recheck/gate-full-44.json`；上一版 43 项本机 89s、reviewer-21 冷跑 101s ⇒ 机器/冷热相关，按 **100–120s** 记；`card_window` 36.7–40.3s、`mir2_probe` 29.4s、本门自测 17.7s，其余 ≤2s） |
| 本门自测 | `--selftest` **13/13**（假·绿/假·红/一绿一红/`--skip` 打印 `SKIP`/`--only`/未知条目 rc=2 ×2/挂住项/总墙钟/兜底/隐藏开关） |
| 清点 | 49 个带 selftest 的工具：门内 **44** / 已分类出域 **5** / **未分类 0** |
| 变异自证 | **45/45**（44 项 + 门自身进程树变异；逐项「改坏被测实现 ⇒ 该门必须红」） |

**能力扩容（两轮共 20 项）**：第二轮 19 项（`review_principal_check` / `scenario_undecidable_audit` /
`rules_approve` / `capture_login_compare` / `s2c_diff` / `error_curve` / `timeline` / `world_sampler` /
`preflight` / `gate_shadow_proxy` / `gate_timer_gate` / `count_m1_logs` / `selftest_proxy` /
`selftest_gate_shadow` / `load_report` / `prepare_safe_login` / `preflight_private_stack` /
`selftest_mutations` / `startup_param`）+ 第三轮 `in_world_stability`（reviewer-21 实跑 7/7——
我先前写「无 selftest 入口」是**事实错误**，第三轮被打回后订正）。

**同型缺陷（变异自证/AST 扫描发现，比新增清单更重要）**：`sys.exit(N)` 写在含 `except SystemExit`
的 `try` 体内 ⇒ 失败分支被自己的处理器当成「预期判红」接住，**改坏必绿**。
- `dbsnap.py` `selftest`（旧写法实测：同一输出里既打印 `RED: 篡改快照后仍判绿` 又打印
  `PASS 2/2` / `GREEN: dbsnap selftest 2/2`，**进程 rc=0**）→ 改 `raise AssertionError` 后同变异 rc=1 + traceback 点名；
- AST 扫出 3 处：`timeline.py`、`world_sampler.py`（已改 + 变异自证 rc=2）、`dbsnap.py restore-selftest`
  （需真实 MySQL，**仅同类修正、未实证**）。**何时能实证**：需要一个能连真实 MySQL 的 runner——当前没有；
  有库环境时用 `python -I -B tools/dbsnap/dbsnap.py restore-selftest --mysql <mysql.exe>`，按本卡变异
  （把 `cmd_diff` 改成恒不判红）跑，预期 rc=1 + traceback 点名 `AssertionError`；**在此之前按未验证对待**。

**变异自证怎么复现**：对每一项注入一处看似无害的改动（让 `sha256_bytes` 恒返回 0、让 `first_mismatch`
恒 `None`、让 `--skip` 不打印、让 `roundtrip_check` 恒失败…），跑 `tool_selftest_gate --only <name>`，
**必须 rc=2**；跑完按原字节还原，再用 `git status --porcelain -- <path>` 复核干净。证据：
`E:/tmp/final-recheck/mutate_gate.py`（45 条定义）、`mutations.json`（逐条 rc）、`mutations-proof.log`。

> **方法论（写进 `mutate_gate.py` 顶部）**：**绿只证明现在没坏，变异才证明门会咬。**
> 初选锚点可能**不在自测路径上**：`capture_login_compare`（`encoded_len`）、`timeline`（`parse_ndjson`）
> 改坏后门仍绿 ⇒ 换成真正被判据覆盖的 `first_body_diff` / `cmd_diff` 才转红。加新项时先确认锚点在 selftest 路径上。
> （`capture_login_compare` 的最终锚点是**判据失效** `elif False and …`，避免 TypeError 崩出来的红。）

**外部子进程的上限（F3）**：44 个注册文件里 `subprocess.*` 一律要有上限或显式终止路径——
已补 **13 处非 git 调用**（`param_discovery:779` 120s 且超时⇒该行 rc=2、`rc_attribution:152`、
`card_window:2688`、`load_report:546`、`preflight:121/332/341`、`selftest_gate_shadow:137`、
`stale_pointer:862/1995/2225/2226/2305`）；**30 处 git 调用保持原样**（本地只读 `rev-parse/log/cat-file/show/
for-each-ref`，且门内已被单项超时+进程树终止兜住、整轮被总墙钟兜住，不依赖网络）；**5 处 `Popen` 均有显式
终止路径**（2 个抓包代理 `terminate/wait(5)/kill`、`next_t` 并发 4 进程 `communicate(120)`、本门 run_one 与
自测 `_bounded`）。`param_discovery` 另有用例 ㉙ 钉住「超时⇒rc=2/cls=error」。

**接入合并**：合并前与「本地门禁」四条一起跑（见 `AGENTS.md`「本地门禁」；reviewer 自己重跑，不采信作者
自述）。全量 ~110s：快循环用 `--only <改动的工具>`，交付前跑全量（**不许 `--skip` 掉慢项了事**）。

**CI 已接**：`.walgit/ci.toml` 第 9 个 `fast-*` 任务 `fast-tool-selftest`
（`command = "python -I -B tools/hygiene/tool_selftest_gate.py"`，`@hourly`，`refs=[refs/heads/master]`，
`timeout = "8m"` = 120s 上界 + 门内单项上限 300s + 余量；门自身总墙钟 420s < 480s；`walgit ci validate` rc=0）。
**注意 F2**：runner 机器没有本机 MySQL 时 `dbsnap` 项会红——那是环境，不是工具回归。

**与开窗自检的关系**：§0.5 / §2 的 `card_window --selftest` 单跑仍然有用（快、只查一个工具），但
「计数变小」这类回归现在由本门统一兜住。门里的数字以**实跑**为准——本节上方旧段落里的 240/240、
262/262 是 2026-10-24 的历史实测，不是当前期望值。
