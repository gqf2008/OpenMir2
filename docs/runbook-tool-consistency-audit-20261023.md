# Runbook × 工具一致性审计（2026-10-23 晚场）

> **快照**：master **`4e0bfd2b8`** @ **2026-10-23 22:27 +0800**；工具 = 同一 commit 的 `tools/`。
> 审计对象是**改前**的 runbook（`git show 4e0bfd2b8:docs/window-runbook-20261023.md`），审计后 runbook 已就地订正（见 §4 的 sha）。
> 卡：`openmir2-runbook-tool-consistency-audit`（tier=L，写集 `docs/`）。
> **每条都实跑或 `--help` 验证过**；没实跑的（会写库/起栈/耗时构建）逐条注明「未实跑 + 为什么」。

## 1. 逐条命令清单

状态三分：**仍有效** / **已过期需改**（命令能跑但按它的结论做会错/会白跑）/ **已不存在**。
「验证」列给的是**本机实际输出要素**，不是读码推断。

| # | 章节 | 命令原文（runbook 里怎么写） | 状态 | 验证（实跑/--help 要点） | 正确写法 / 备注 |
|---|---|---|---|---|---|
| 1 | §1.1 | `cargo build --release -p mir2-server` | 仍有效 | `mir2-rs/bins/mir2-server/Cargo.toml:2 name="mir2-server"`（未实跑：构建耗时，且不属于审计写集） | 补一句：**在你要启动的那棵树里 build**（见 #2 的解析口径） |
| 2 | §1.1 | `start_mir2_server.ps1 -WhatIf`（核对打印的 `SERVER_EXE` 与 sha256） | **已过期需改** | 实跑 rc=0；输出首行 `SERVER_EXE E:\tmp\wt-worker-67\mir2-rs\target\debug\mir2-server.exe`；脚本 `:285-290` 以 `$PSScriptRoot\..\..` 为仓根、**release 缺失就静默退 debug**；`:349` 只打印路径，**不打印 sha256** | 见 §2.1：完整命令 + 「必须看到 release」+ 自己算 sha256 + 以起栈后的 pid `roles[].exe` 为准 |
| 3 | §1.1 | `python -I -c "import json;d=json.load(open('E:/tmp/conformance/rust-logs/mir2-server.pid.json',encoding='utf-8-sig'));print(d['roles'][0]['exe'])"` | 仍有效 | 实跑 rc=0，输出 `E:\Users\gxh\Documents\GitHub\OpenMir2\mir2-rs\target\release\mir2-server.exe` | 仅在**真起过栈**之后才有文件；`utf-8-sig` 是必需的（BOM） |
| 4 | §1.2 | `powershell -File E:\MirServer\port-status.ps1`（「十端口必须全 DOWN」） | **已过期需改** | 实跑 rc=0；`:38` 的端口表是 `3306,5000,5100,5500,5600,5700,6000,7000,7100,7200` —— **默认栈** + mysqld，**不含 alt 的 15xxx/17xxx**；全部监听时打印「十个端口全部在监听」 | 见 §2.3：3306 必须 **UP**；alt 的端口用 profile 的 `ports` 自查（或直接信 `card_window` 的预检） |
| 5 | §1.3 | `restore-baseline.sh` / `restore-baseline-r.sh` | 仍有效 | 两个脚本都在（367B / 489B），`bash -n` 通过；**未实跑**（会 `drop database` 后重建，属破坏性） | `-r.sh` 的重建源是 `E:/tmp/conformance/baseline/<库>.sql`，库名带 `_r` 后缀 |
| 6 | §1.5(第 5 条) | 「场景用到的 ident 必须先在 `rules.json` 登记」的 python 自查片段（输出 `missing: []` 才开跑） | **已过期需改** | 实跑输出 `missing: [202, 203, 642]`（按原片段）；片段还有一个 bug：`wait_idents` 写成 `{"idents":[...]}` 时**取不到**，补上后真实缺口是 `[202, 203, 642, 709]` | 见 §2.4：换成正确片段（含 dict 形态）+ 当前缺口两条已合入场景 |
| 7 | §1.5 | 场景 `placement` JSON 示例 + 「声明即可」 | 仍有效 | `--dry-run` 实跑打印 `[rust] 预置位（dry-run 不连库）：map=3 x=320 y=328 db=mir2_test_db_r 图=3.map 1000x800 可走` | 无 |
| 8 | §2 | `card_window.py --card <卡 id> --scenario <场景> --out <目录> --profile alt` | 仍有效 | `--help` rc=0；`--dry-run` 实跑 rc=0（全链命令序列齐全） | 补：`--timeout-ms` 覆盖的是 **profile 的 `probe_timeout_ms`**（见 §2.2） |
| 9 | §2 | `--post-comment` | 仍有效 | `--help` 有该 flag（「跑完在该卡发一条签名 comment…不改卡状态」） | 需要真跑才会用；`--dry-run` 不触发 |
| 10 | §1.5 / §2 / §4.5.1 | `--dry-run` | 仍有效 | 实跑 rc=0，不建目录（验证方式：`ls` 目标目录 ⇒ 不存在） | 预置位/端口/就绪等待/各步命令都会预览 |
| 11 | §4.5.1 | `--dry-run --discovery-frames E:/tmp/conformance/runs/w24-csharp-npc_auto/frames.ndjson` | 仍有效 | 实跑 rc=0；两侧 probe 命令都带真实值 `--var npc_actor=647278663` | 文件确实存在（14 KB，10-23 05:11） |
| 12 | §4.5 | `--allow-missing=3` / `--allow-missing=1` | 仍有效（语境已变） | 实跑 `check-all`：`scenarios=19 ok=16 rc2=3 derived=5 rule=2 computed=1 no_required=5 params_default=3 missing=3 error=0`，**rc=2**；`--allow-missing=1` 也是 **rc=2**（真实录制无 201 ⇒ bag_equip/eat_item 也缺） | 这条只能对「追加了合成 201 帧的录制副本」成立；对真实录制 `--allow-missing=3` 才可能 0 |
| 13 | §4.5.1 | `card_window --selftest`（DISC-9） | 仍有效 | 实跑 `GREEN: card_window.py selftest 186/186`，**rc=0** | 期望计数写死 **186**（见 §2.5） |
| 14 | §4.5.1 / §1.5 | 「`card_window.py` 本期未接背包预置」 | 仍有效 | `grep -rn inventory tools/ --include=*.py` ⇒ 与背包预置相关的命中 **0**（只有两个无关文件名的 `inventory` 字样） | 结论仍对：背包夹具还得手工 DB；`openmir2-window-inventory-fixture` 的实现**没有**在 master 上 |
| 15 | §4 | `grep -n 'ident=' docs/<卡相关文档>` | 仍有效 | 通用命令，实跑可用 | 无 |
| 16 | §0 | 工具链三件套 `card_window.py` / `compare_runs.py` / `param_discovery.py` | 仍有效 | 三者 `--help` 均 rc=0；`compare_runs.py selftest` = `GREEN 198/198` rc=0 | `param_discovery --help` 只有一行 usage（`selftest | <frames> <vars> | check-all …`） |
| 17 | §0 | 「开跑前还剩一个前置没到位：账本 compare 诊断值**尚未加值断言**（卡 `openmir2-card-window-value-assertions`）」 | **已过期需改** | 该卡已合入：`b9765764f` @ 2026-10-23 16:45:18，approve `reviewer-2 d755493c`（+69 行 card_window + mutation 用例） | 改成「已完成」并给 oid；不要再当前置 |
| 18 | §1.1 | 「`rust-root` 里的 .NET 程序集与 Rust 栈无关」 | 仍有效 | 起栈命令的 `-Root` 只读配置文件；`SERVER_EXE` 列证明跑的是 `mir2-server.exe` | 无 |
| 19 | §1.1 | 「pid 文件路径只由 profile 的 `sides.rust.pid_file` 声明，代码里不写死」 | 仍有效 | `card_window.py:237-244`（`pid_file_path` 只读 `sc["pid_file"]`）、`:329`（alt 的值）、`:664-672`（缺 `pid_file` ⇒ `ConfigError`） | 无 |
| 20 | §0 | 「欠窗口的卡 173 张」 | 需带戳 | 口径见 `docs/window-batch-plan-20261023.md` §1；今日已两轮复核仍在变 | 保留数字但**必须带快照戳**（runbook 已改） |

## 2. 五条重点（卡面点名）

### 2.1 `start_mir2_server.ps1 -WhatIf` 的 `SERVER_EXE` 与 sha256

实跑（与 wrapper `E:/tmp/conformance/start-rust.ps1` 同参、只加 `-WhatIf`）：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools/m2-acceptance/start_mir2_server.ps1 `
  -Root 'E:/tmp/conformance/rust-root' -LogDir 'E:/tmp/conformance/rust-logs' `
  -PortsProfile 'E:/tmp/window/r5/root/alt-port-profile.json' -WhatIf
```

输出（本 worktree 实测）：`SERVER_EXE E:\tmp\wt-worker-67\mir2-rs\target\debug\mir2-server.exe`，rc=0。

两条**会让人白跑**的事实：

1. **`-WhatIf` 不打印 sha256**（`:349` 只有 `SERVER_EXE {0}`）⇒ runbook 原句「核对它打印的 SERVER_EXE 路径与文件的 sha256」只能核对**路径**，sha 要自己算：
   `Get-FileHash -Algorithm SHA256 <SERVER_EXE>`（PowerShell）或 `sha256sum <path>`。
2. **解析口径是「脚本所在仓库」而不是部署根**：`:286` `$RepoRoot = $PSScriptRoot\..\..`，`:288-290` release 存在就用 release，**否则静默退 debug**。
   ⇒ 在缺 release 的 worktree 里跑 `-WhatIf` 会看到 **debug**；而真窗口走的是 profile 的 `stack` 命令（`E:/tmp/conformance/start-rust.ps1`，它把脚本路径**写死成主检出** `E:/Users/gxh/Documents/GitHub/OpenMir2/tools/...`）⇒ **在 worktree 里预览到的二进制 ≠ 真窗口将启动的二进制**。
   以真起栈后的 `mir2-server.pid.json` 的 `roles[].exe` 为准（工具已对账，不符 rc=2）。

**正确做法**：① 在**主检出**（或你真要起栈的那棵树）里 `cargo build --release -p mir2-server`；② 用上面命令 `-WhatIf`，**必须看到 `\target\release\mir2-server.exe`**；看到 `\debug\` ⇒ 停，先构建；③ 对那个路径算 sha256 写进本次 `README.txt`。

### 2.2 超时键的真实名称

| 位置 | 事实 | 位置 |
|---|---|---|
| **profile 键** | `probe_timeout_ms`（**不是** `timeout_ms`） | 内置 alt `card_window.py:297`、csharp-default `:345` |
| 传给 probe | `--timeout-ms <probe_timeout_ms>`（每次等待的默认超时） | `:973-974` |
| **子进程总超时** | `max(180, probe_timeout_ms/1000 + 120)` 秒 —— 到点就杀 probe | `:1949` |
| CLI 覆盖 | `--timeout-ms` → 写回 `prof["probe_timeout_ms"]` | `:696-697` |
| 未知键 | profile 顶层**没有白名单校验** ⇒ 在 profile JSON 里写 `"timeout_ms": 720000` 会被**静默忽略**，仍是 20000 ⇒ 180s 被杀 | `load_profile` 只校验已知键（`:600-680`），不拒未知键 |

**`upgrade_retrieve` 该写多少**：该场景第 9 步是 `{"sleep": {"ms": 660000}}`（`tools/probe/scenarios/upgrade_retrieve.json` 的 steps[9]）⇒ 总时长 ≥ 660s。
按公式 `max(180, X/1000+120)`，要活下来必须 **`probe_timeout_ms` ≥ 660000**；**建议 720000**（子进程 840s，留 60s 余量）。
写法二选一：profile JSON 顶层 `"probe_timeout_ms": 720000`，或命令行 `--timeout-ms 720000`（两者等价，后者不用改 profile）。

### 2.3 内置 `alt` profile 的 `pid_file`

`card_window.py:329`：`"pid_file": "E:/tmp/conformance/rust-logs/mir2-server.pid.json"`（= wrapper `-LogDir`）。
代码侧 `:237-244` 只认 `pid_file`（支持 `{side_dir}` 模板），`:664-672` 缺 `pid_file` ⇒ 加载期 `ConfigError`。
rust-default 的 `pid_file` 是 `{side_dir}/mir2-server.pid.json`，**但它的 wrapper `E:/tmp/conformance/start-rust-default.ps1` 本机不存在**（`ls` ⇒ No such file）⇒ 该 profile 目前不可用。

### 2.4 `--check-scenario` 的全部 flag / 校验 / 退出码

- flag：`--check-scenario`（开关）＋ `--scenario <path>`＋ `--var NAME=VALUE`（可重复）。
- 退出码：**rc=0 = 形状与语义通过；rc=2 = 场景错误**。`rc=2` **有两种完全不同的原因**，看 stderr 首行区分：
  1. `scenario-error: <规则名> …` —— 真的坏了（R1–R5 或 schema/blocked 形状）；
  2. `scenario-error: scenario 'x' requires --var a=<value>, …` —— **只是没给 required 变量**（场景没坏）。
- 校验项（`mir2_probe.py`）：
  - schema/fail-closed：未知键、类型、`required` 重复（`:345-407`）；
  - **R2** wait 之前必须有过发帧步（`:817`）；**R1** 至少一个发帧/累计观察步（`:831`）；
  - **R3** ident 必须在 `messages.rs` 常量表、**且方向对**：发帧动作必须 `CM_*`、观察动作必须 `SM_*`（`:839-871`；6 个同值 ident 42/100/101/102/103/104 两向放行）；
  - **R4** 变量引用必须先声明（`:872`）；**R5** `placement` 与 `self_pos` 等派生参数自洽（`:881`）；
  - **R6**（**WARN 级，不改 rc**）攻击族 `send.tag` 越界只警告（`:904`）；
  - **`blocked`**：顶层 `{reason, missing_fixture?}` 是**合法形状** ⇒ `--check-scenario` 仍 **rc=0**；写错形状才 rc=2。真正拦跑的是 `card_window`（见下）。
- 实测：19 个场景逐个跑，8 个 rc=0、11 个 rc=2（全是 `requires --var`）。**带 `required` 的场景只有给了 `--var` 才会 rc=0**。
- 另：`combat_strike.json` 有合法 `blocked` ⇒ `--check-scenario` rc=0；`card_window --dry-run` **rc=2**、打印 `scenario is BLOCKED: …`、**不建 out 目录**（本机实测 `real_rc=2`，目标目录不存在）。

### 2.5 `--selftest` 期望计数

`python -I -B tools/conformance/card_window.py --selftest` ⇒ `GREEN: card_window.py selftest 186/186`，**rc=0**（本机 2026-10-23 22:2x 实跑）。
同族：`compare_runs.py selftest` = `198/198` rc=0。**这两个数是「代码没被改坏」的门槛，runbook 里要写死**（改动后第一件事就是跑）。

## 3. 审计发现的**窗口级缺口**（不在本卡写集，必须另卡）

1. **两个已合入的场景引用了未登记 ident**（`rules.json` 现有 38 条，无 202/203/642/709）：
   - `equip_struck_save.json`（合并 `121a6b48b`）：wait `203`、`642`；
   - `upgrade_retrieve.json`（合并 `d5cbb7abf`）：wait `202`、`709`（709 在 `wait_idents` 的 dict 形态里，旧自查片段抓不到）。
   ⇒ 按 runbook 的判据（未登记 ident 出现 ⇒ `UNCLASSIFIED` ⇒ 整次 **UNDECIDABLE(rc=2)**），这两张卡今天开窗**必 UNDECIDABLE**，不是代码问题。
   建议：另开「给 rules.json 补 202/203/642/709」的卡（`rules.json` 改动 = 改判据，要走评审）。
2. **`port-status.ps1` 不覆盖 alt 窗口端口**（见 §1 表 #4 与 runbook §1.2 的订正）；runbook 的「十端口全 DOWN」是把默认栈的检查当成了本窗口的检查。
3. **`start-rust.ps1` 把脚本路径写死成主检出** ⇒ 在 worktree 里 `-WhatIf` 预览到的二进制与真窗口不同（见 §2.1）。

## 4. 牙（把命令改回旧写法 ⇒ 断言变红）

断言（写进 runbook 的 60 秒清单，也可单独跑）：

```sh
python -I -c "
import io,re,sys
s=io.open('docs/window-runbook-20261023.md',encoding='utf-8').read()
ok = ('probe_timeout_ms' in s) and not re.search(r'\"timeout_ms\"\s*:', s)
print('DOC-KEYS', 'OK' if ok else 'RED'); sys.exit(0 if ok else 1)"
```

- **基线（订正后的 runbook）**：rc=0，`DOC-KEYS OK`；sha256 = `7e08004d338cb0bbe5fe0b0b1b89a5665d668807bb2adc2f2bff428964ed1cfb`。
- **变红**：把 runbook §2 的超时写法改回旧形态（在 profile JSON 段落里写 `"timeout_ms": 720000`）⇒ 同一命令 **rc=1、`DOC-KEYS RED`**（实测见下方记录）。
- **还原**：逐字节还原后 sha256 必须回到上面的值。

## 5. 残留物清单

- 本审计产生的临时产物只在 `E:/tmp/w67-scratch/`（复跑副本、`r6_tag99.json` 等，见上一张 docs 卡登记），仓库内只改 `docs/`。
- **没有**起栈、没有连库、没有跑 `restore-baseline*.sh`、没有改 `rules.json` / `scenarios/*.json` / 任何工具代码。

### 4.1 本次牙的实测记录（2026-10-23 22:3x）

1. 备份：`cp docs/window-runbook-20261023.md E:/tmp/w67-scratch/runbook-base.md`
2. 基线：sha256 = `7e08004d338cb0bbe5fe0b0b1b89a5665d668807bb2adc2f2bff428964ed1cfb`；断言 `DOC-KEYS OK`，**rc=0**
3. 变异（把 §2 的超时写法改回旧形态，模拟旧 runbook）：把
   `` `probe_timeout_ms` 设到 **720000** `` 换成 `"timeout_ms": 720000` ⇒ 断言 **`DOC-KEYS RED`，rc=1**
4. 还原：从备份 `cp` 回 ⇒ sha256 回到 `7e08004d…`（与基线逐字节一致），断言 `DOC-KEYS OK`，**rc=0**

> 牙的含义：只要有人在 runbook 里把 profile 键写回 `timeout_ms`，这条 30 秒断言就会拦下来；
> 而它是**真的会白跑窗口**的那类错（profile 顶层无白名单 ⇒ 静默忽略 ⇒ probe 180s 被杀）。
