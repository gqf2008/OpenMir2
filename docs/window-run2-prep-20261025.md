# window-run-2 续跑准备包（run-3）— 2026-10-25

- 卡：`openmir2-window-run2-prep`（tier **L**；owner `openmir2-dev-2`；worktree `.worktrees/dev-2`；分支 `card/window-run2-prep`）。
- 基线：`walgit/master` `0ef4e3b96`（2026-10-24 三天计划）；本文件随后续提交更新，实际证据见 §7 的 commit/sha。
- 定位：**tester-1 窗口执行的操作手册 + 可执行准备件**。窗口时段由 owner 给；本包**不占用窗口**、不起栈、不占端口、不写库。
- **2026-10-25 修（reviewer-21 W-1）**：`pickup_item` 已封存为结构性不可判（T-19/T-25），从 run-3 清单移除（**18→17**）；
  `run_batch.sh` 与 preflight `roster` 均断言「17 条且不含 `pickup_item`」，加回即 rc=2（fail-closed）。
- 验收映射：① 脚本 `--list/--dry-run` + 私栈自检可验证；② 两坑（SelGate ListenPort / 525 路由 17200）写进前置检查且各自可机判；③ 证据目录与 run-1/2 逐字段兼容（§5 样例）；④ `card_window --selftest` 全绿 + 卫生门禁 + `diff --check` + merge-tree（§7）。

## 0. 一句话

`bash tools/window/run_batch.sh` = **release 重建（head/sha256 对齐）→ 私栈/两坑/pin 自检 → 17 场景续跑 → 证据 + 批次汇总**。被中断后重跑同一条命令即可：已完成的场景跳过，`UNDECIDABLE`/半截场景重跑。

## 1. 交付件

| 文件 | 作用 |
|---|---|
| `tools/window/run_batch.sh` | run-3 一键续跑（唯一入口） |
| `tools/window/run3-scenarios.json` | 17 场景清单（batch 与 preflight 共用的唯一事实源；已封存的 `pickup_item` 不在列） |
| `tools/window/rebuild-server.ps1` | release 重建 + `mir2-server.build.json`（head/sha256/dirty）+ `start-rust.ps1 -ServerExe` 同步 |
| `tools/window/preflight_private_stack.py` | 私栈完整性 / 两坑 / pin 链 / 二进制对齐 / 清单一致性自检（只读，含 `--selftest` 27 条牙） |
| `docs/window-run2-prep-20261025.md` | 本文件 |

未改动：`card_window.py`、`compare_runs.py`、`mir2_probe.py`、`dbsnap.py`、场景 JSON、`rules.json`、`.walgit/board.toml`、`E:/MirServer`、mysqld。

## 2. run-3 一键续跑

### 2.1 场景清单（顺序 = window-run-2 实跑顺序去掉已封存的 `pickup_item`，重排为 1..17）

| # | 场景 | profile | 额外参数 | 已知前置（详见 `docs/window-runbook-20261023.md` §1.8/§7.4） |
|---:|---|---|---|---|
| 1 | `login_only` | alt | — | 无手动步骤；首登 `characters_ablity.Weight` 列差是已知 R 项 |
| 2 | `npc_click_auto` | alt | — | actor 自动发现 |
| 3 | `npc_click` | alt | — | 场景 `placement` |
| 4 | `dice_chain_auto` | alt | — | 无（actor 自动发现） |
| 5 | `dice_chain` | alt | — | `params.npc_actor=119` 是旧抓包值；**跨会话失效**，优先用 `dice_chain_auto` 的结论；保留本行只为清单完整 |
| 6 | `gm_time` | alt | — | 无；ident 100 时间文本属墙钟噪声 |
| 7 | `gm_batch` | alt | — | 同上 + 可能带 `Weight` 库差 |
| 8 | `move_walk` | alt | — | 走位目标格需与部署一致 |
| 9 | `merchant_dialog` | alt | — | 商人摆位/语境 |
| 10 | `king_dialog` | alt | — | `king_actor` 发现；点错庄家会响亮超时 |
| 11 | `king_castle_war` | alt | — | 城堡战语境（castle_zone 缺口） |
| 12 | `npc_script_chain` | alt | — | 脚本链语境 |
| 13 | `equip_struck_save` | alt | — | 同屏受击者夹具 |
| 14 | `bag_equip` | alt | — | 场景自描述 `inventory`；run-2 曾因 `characters_bagitem.ChrName` 列缺失 fail-closed |
| 15 | `eat_item` | alt | — | 同上 |
| 16 | `spell_cast` | **window-magic** | — | magic 夹具 + rust 侧 `MIR2_CLIENT_ABILITY_LAYOUT=0` pin（profile 内置） |
| 17 | `upgrade_retrieve` | alt | `--timeout-ms 720000` | 第 9 步 sleep 660000；默认 20000 会 180s 杀 probe |

> `--dry-run` 可用性（2026-10-24 `docs/window-scenario-dryrun-sweep-20261024.md`）：alt 16×rc=0，`spell_cast` 用 `window-magic` rc=0。**dry-run 绿 ≠ 真机已对拍**。

### 2.2 用法

```sh
cd <本检出>                      # 必须与 rebuild/preflight 同一 checkout
bash tools/window/run_batch.sh --list
bash tools/window/run_batch.sh --dry-run          # 只读：打印计划 + 跑前置自检，不起栈
bash tools/window/run_batch.sh                    # 开窗：重建 + 自检 + 17 场景
bash tools/window/run_batch.sh --only login_only,spell_cast
bash tools/window/run_batch.sh --from 9           # 中断后从第 9 个接着跑
bash tools/window/run_batch.sh --force            # 全部重跑（忽略续跑判据）
bash tools/window/run_batch.sh --skip-pin-smoke   # pin 链只做静态核对（证据登记未冒烟）
bash tools/window/run_batch.sh --run-dir E:/tmp/window/run3 --timeout-sec 2700
```

- **续跑判据**（`--force` 之外）：`card-window.json` 同时满足
  `verdict ∈ {PASS, FAIL}`、`compare_rc ∈ {0,1}`、`report_kind=compare`、`frames.compared>0`、
  `artifacts` 含 `report.json`/`README.txt`、`git_head == 当前 HEAD`。
  `UNDECIDABLE`、半截目录、head 变过的场景都会重跑；`card_window.py` 会把旧产物归档到
  `<out>/_stale-<ts>-<pid>/`，不会用旧 PASS 冒充本轮。
- **每场景后兜底清理**：只按 `E:\tmp\conformance\csharp-root\*` 路径杀进程 + `stop-rust.ps1`；
  **不碰 mysqld**（3306 全程 UP）。
- `--timeout-sec` 是外层超时（GNU `timeout`，默认 2700s）；没有 GNU timeout 时脚本打印 WARNING 并退化为不设外层超时（card_window 自己的子进程超时仍生效）。

### 2.3 rc 双列口径（run-2 已确立，run-3 修正采集 bug）

每个场景在 `batch.log` / `batch.jsonl` 里落两列：

| 列 | 来源 | 含义 |
|---|---|---|
| `outer_rc` | 本脚本看到的 `card_window.py` 进程返回码 | `0` = PASS；`2` = 有结论但不通过 / UNDECIDABLE；`124`/`137` = 外层超时 |
| `compare_rc` | `card-window.json`（`compare_runs.py` 原样） | `0` = 对拍全绿；`1` = 抓到真差异；`null` = 判不了 |

- **run-2 的 bug**：`echo "END $(date +%H:%M:%S) $name shell_rc=$?"` 里命令替换 `$(date)` 先执行并**重置 `$?`**，所以 run-2 的 `shell_rc` 恒为 0（与 `verdict=FAIL/exit=2` 矛盾）。run-3 在调用后**立即** `outer_rc=$?`，再碰任何命令替换。
- `verdict` 只有三态：`PASS` / `FAIL` / `UNDECIDABLE`；**UNDECIDABLE 不是通过**，批次 rc=2。

### 2.4 退出码

`0` = 所有选中场景都产出了 PASS/FAIL；`2` = 缺参 / 重建失败 / 前置失败 / 任一场景 UNDECIDABLE 或没有证据。
与 `card_window.py` 的 fail-closed 口径一致。

## 3. release 二进制重建（对齐 head + sha256）

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools/window/rebuild-server.ps1 -SyncStartRust -RequireClean
```

做的事：

1. `git rev-parse HEAD` + `git status --porcelain`；`-RequireClean` 下脏树直接 RED（证据必须能追溯到 HEAD）。
2. `cargo build --release -p mir2-server`（cwd = `mir2-rs/`）。
3. 写 `mir2-rs/target/release/mir2-server.build.json`（`format=mir2-build-stamp/1`）：
   `head / head_subject / built_at / dirty / exe / sha256 / size / cargo / rustc`。stamp 在 target 下，不入库。
4. `-SyncStartRust`：把 `E:/tmp/conformance/start-rust.ps1` 的默认 `-ServerExe` 改成**本检出**的 release 二进制；
   原文件先备份 `start-rust.ps1.bak-<时间戳>`。**为什么必须**：run-2 的默认值指向已删除的
   `E:\tmp\wt-worker-73\...`；`card_window.py` 起栈后用 `mir2-server.pid.json` 的 `roles[].exe`
   与 profile `binaries` 对账，不一致直接 rc=2。
5. 打印机器可读行：`BUILD_HEAD / BUILD_DIRTY / SERVER_EXE / SERVER_EXE_SHA256 / BUILD_STAMP / START_RUST_SERVER_EXE`。

`run_batch.sh` 默认在开跑前自动执行这一步（`--no-rebuild` 可跳过，但证据里必须写明二进制来源）。
**窗口里 Rust 证据的 sha256 一律以 `batch-summary.json.rust_binary.sha256` 为准**（与每个场景 `README.txt` 里的 rust sha 应一致）。
C# oracle 侧 sha256 由 `card_window.py` 逐场景记录（`csharp-root/M2GameSvr/GameSrv.exe`）。

## 4. 前置自检

```sh
python -I -B tools/window/preflight_private_stack.py          # 私栈 + 两坑 + pin + 二进制
python -I -B tools/conformance/preflight.py --profile alt --with-dry-run
python -I -B tools/conformance/preflight.py --profile window-magic --with-dry-run
```

`run_batch.sh` 在开跑前自动跑这三条；任一步 FAIL ⇒ 不开跑（`--no-preflight` 仅调试，证据须登记）。

### 4.1 `preflight_private_stack.py` 八步

| step | 判什么 | FAIL 的典型现象 |
|---|---|---|
| `inventory` | 私栈 22 个部署件 + `baseline/` 非空 | 外部清理后只剩半套；restore 无基线 |
| `selgate_listenport` | **坑 A**：`ListenPort=17100` + SelGate.exe 与 T-172 构建同 sha256 + `AddrTable→:17100` | stack 步只报 `ready ports missing: 17100,17200`，看不出根因 |
| `route_525` | **坑 B**：两侧 `ServerInfo.txt` 首行端口含 17200；C# RunGate 17200 / Rust 17600 | C# 探针按 525 连网关连空；或 Rust 525 与 oracle 逐字节必红 |
| `pin_static` | `window-magic` rust env + `-PinChecks/-PinLog` 接线；alt 未被污染 | pin 静默丢失（布局跑错却不报错） |
| `pin_smoke` | `start-stack-wmi.ps1 -CheckPinsOnly`（**不派生进程**）读上一轮 game-svr 日志自证 | 日志不存在 / 未见 `MIR2_CLIENT_ABILITY_LAYOUT=0` + `layout=oracle-40` |
| `binary_stamp` | stamp.head == 当前 HEAD；stamp.sha256 == 文件实际；dirty 为空 | 旧二进制假绿 / 构建时脏树 |
| `start_rust_exe` | `start-rust.ps1 -ServerExe` == 本检出 release | run-2 遗留的 `wt-worker-73` 路径 |
| `roster` | 17 场景文件、**不含已封存 `pickup_item`**、profile 只允许 alt/window-magic、rules.json 在位 | 清单漂移 / 已封存场景混入 |

> 私栈根可用 `--root` / `WINDOW_PRIVATE_ROOT` 覆盖；默认 `E:/tmp/conformance`。
> `--skip-pin-smoke` / `--skip-binary` 只降级为 WARN，**必须**在证据里登记。

### 4.2 坑 A：SelGate ListenPort（重建期）

- **背景**：C# `SelGate` 源码把 `GatePort = 7100` 写死；owner 2026-10-23 批准的 T-172
  （`src/SelGate/GateShare.cs:21` 改 `public static int`，`Conf/ConfigManager.cs:71` 读
  `[SelGate] ListenPort`）让私有栈能起在 17100。**只有 T-172 构建**会读 `ListenPort`。
- **机判**：`csharp-root/SelGate/config.conf` 的 `ListenPort == 17100`
  **且** `sha256(csharp-root/SelGate/SelGate.exe) == sha256(selgate-build/SelGate.exe)`
  **且** `LoginSrv/AddrTable.txt` 含 `:17100`。
- **失败现象**：`start-stack-wmi` 报 `ready ports missing: 17100,17200`；登录到选服后无
  `SM_SELECTSERVER_OK(530)`，看起来像「Rust 协议错」，实际是**选服网关没起来**。
- **修复**：用 `E:/tmp/conformance/selgate-build/` 的构建覆盖 `csharp-root/SelGate/`；确认
  `config.conf` 的 `[SelGate] ListenPort=17100`（stock 二进制不认这个键）。

### 4.3 坑 B：525 路由 17200（重建期）

- **背景**：`SM_STARTPLAY(525)` body = `GameGate ip/port`；C# 探针**用它**决定连哪个游戏网关
  （`mir2_probe.py` `run.chain["gamegate"]`）。`DBSrv/ServerInfo.txt` 默认行是
  `127.0.0.1 127.0.0.1 7200`；私有栈网关在 17200 ⇒ 不改就连空。
- **机判**：
  - `csharp-root/DBSrv/ServerInfo.txt` 首行 = `127.0.0.1 127.0.0.1 17200`；
  - `rust-root/DBSrv/ServerInfo.txt` **也必须** = `127.0.0.1 127.0.0.1 17200`；
  - `csharp-root/RunGate/config.conf` `GatePort1=17200`、`rust-root/RunGate` `GatePort1=17600`。
- **反直觉点（别「修」错）**：Rust 实际连的是 **17600**，由 profile 的
  `--game-gate 127.0.0.1:17600` 覆盖；但 `rules.json` 对 525 是 `action=compare`
  （body 逐字节比），所以 **rust-root 的 ServerInfo 必须保持 17200** 才能与 oracle 相同。
  把它改成 17600 ⇒ 525 必红（preflight 有专门的牙钉住这一点）。
- **失败现象**：C# 侧 probe 在 `SM_STARTPLAY` 之后连不上网关；或 Rust 525 与 oracle 差
  `.../17600` vs `.../17200`。

### 4.4 pin 链 smoke

`window-magic` 的 rust 侧 `sides.rust.env.MIR2_CLIENT_ABILITY_LAYOUT=0` +
stack `-PinChecks MIR2_CLIENT_ABILITY_LAYOUT=0 -PinLog E:/tmp/conformance/rust-logs/game-svr.out.log`。
`start-stack-wmi.ps1 -CheckPinsOnly` **只校验调用方 env + 日志自证行，不派生进程**，
因此可以在开窗前任意跑。日志不存在 ⇒ FAIL（自证无从谈起）；确实没有上一轮日志时用
`--skip-pin-smoke` 并在证据里登记未冒烟。

## 5. 证据目录规范（`E:/tmp/window/run3/`）

### 5.1 目录树

```text
E:/tmp/window/run3/
  batch.log                      # 人读：START/RESULT/SKIP/END（rc 双列）
  batch.jsonl                    # 机器读：每场景一行（增量写，中断也保留）
  batch-summary.json             # 批次汇总（每次重算；字段见 5.3）
  rebuild.log                    # release 重建输出（含 SERVER_EXE_SHA256）
  preflight/
    preflight-private.log/.json  # 私栈自检逐项
    preflight-alt.log
    preflight-window-magic.log
  <scenario>/                    # ← 与 run-1/run-2 **逐字段相同**（card_window.py 原样产物）
    report.json
    card-window.json
    card-window.log
    README.txt
    csharp/{frames.ndjson,run.json,discovered.json,db/…,placement.json,inventory.json,magic.json}
    rust/{同上}
  <scenario>.log                 # card_window 的 stdout/stderr（run-2 同口径）
```

`--dry-run` 的日志写到 **`E:/tmp/window/run3-dry-run/`**（旁边，不污染真证据目录）。

### 5.2 与 run-1/run-2 的兼容性

- `<scenario>/` 下所有文件由 `card_window.py` 生成，**字段/布局未改**；run-1/run-2 的
  `report.json` / `card-window.json` / `README.txt` / 两侧 `frames.ndjson` 可直接与 run-3 对照。
- 本轮新增的只有**批次层**（`batch.*`、`preflight/`、`rebuild.log`），不修改任何单场景字段。
- `card-window.json` 关键字段（续跑判据与汇总都读这些）：
  `verdict`、`compare_rc`、`exit_code`、`report_kind`、`frames{oracle_total,rust_total,compared}`、
  `artifacts{README.txt,report.json,...}`、`git_head`、`binaries{csharp[],rust[]}`、`sides{...}`。

### 5.3 `batch-summary.json` 字段与样例

| 字段 | 说明 |
|---|---|
| `format` | `window-batch-summary/1` |
| `card` / `run_dir` / `repo` / `git_head` / `generated_at` | 批次标识 |
| `rust_binary.{path,sha256,build_stamp}` | 本轮 Rust 二进制（来自 `mir2-server.build.json`） |
| `totals.{PASS,FAIL,UNDECIDABLE,NOT_RUN}` | 计数（`NOT_RUN` = 中断时还没轮到的场景） |
| `scenarios[]` | 按清单顺序，每项：`n/name/profile/extra/verdict/compare_rc/exit_code/outer_rc/frames{oracle,rust,compared}/duration_s/skipped/skip_reason/dir/log/evidence{card_window_json,report_json,readme}` |

样例（真实值以本轮 `E:/tmp/window/run3/batch-summary.json` 为准）：

```json
{
  "format": "window-batch-summary/1",
  "card": "openmir2-window-run-2",
  "run_dir": "E:/tmp/window/run3",
  "git_head": "<本轮 HEAD>",
  "rust_binary": {
    "path": "E:/…/mir2-rs/target/release/mir2-server.exe",
    "sha256": "<本轮 release sha256>",
    "build_stamp": {"format": "mir2-build-stamp/1", "head": "<本轮 HEAD>", "dirty": ""}
  },
  "totals": {"PASS": 1, "FAIL": 0, "UNDECIDABLE": 0, "NOT_RUN": 16},
  "scenarios": [
    {
      "n": 1, "name": "login_only", "profile": "alt", "extra": [],
      "verdict": "PASS", "compare_rc": 0, "exit_code": 0, "outer_rc": 0,
      "frames": {"oracle": 29, "rust": 29, "compared": 29},
      "duration_s": 185, "skipped": false, "skip_reason": null,
      "dir": "E:/tmp/window/run3/login_only",
      "log": "E:/tmp/window/run3/login_only.log",
      "evidence": {"card_window_json": "…/card-window.json", "report_json": "…/report.json",
                   "readme": "…/README.txt"}
    },
    {"n": 17, "name": "upgrade_retrieve", "verdict": "NOT_RUN", "skipped": false}
  ]
}
```

`batch.jsonl` 每行就是一个 `scenarios[]` 元素（同一场景重跑时后一行覆盖前一行；汇总取最后一行）。
`batch.log` 的 `RESULT` 行示例：

```text
RESULT n=07 name=gm_batch verdict=FAIL outer_rc=2 compare_rc=1 exit_code=2 frames=36/36/35 duration=150s
```

## 6. 开窗前 10 分钟 checklist（tester-1）

1. `git status --porcelain` 为空；`git rev-parse HEAD` 记入本轮快照戳。
2. `bash tools/window/run_batch.sh --list`（17 行）与 `--dry-run` 跑通；确认 preflight 全 OK。
3. 确认窗口独占：默认 C# 栈的 `E:/MirServer` 六进程已停（**不要**用 `start-all.ps1 -Stop`，会停 mysqld）；3306 仍 UP。
4. `bash tools/window/run_batch.sh`；中断就重跑同一条命令（续跑）。
5. 跑完：`batch-summary.json` 的 `totals` 与 `batch.log` 的 RESULT 行逐场景核对；把证据路径 + 两侧二进制 sha256 + HEAD 贴到 `openmir2-window-run-2` 的 `status`/`comment`。
6. 恢复默认栈（`start-all.ps1`），`port-status.ps1` 复核十端口；清理只清本轮起的栈，不动 mysqld。

## 7. 本次 dry-run 验证证据

（以下命令均在本检出执行；**全程未起栈、未占端口、未连库**。）
> 修后（`openmir2-window-run3-scenarios-blocked`）：`--list`（17 行）与 `check_roster()`（`17 场景…不含 pickup_item`）已实跑；
> 下方其余输出仍是修前基线 `3d6c85595` 的原始记录（未重跑全量 preflight：私栈 `start-rust.ps1 -ServerExe` 仍指 dev-2，按设计会 FAIL）。

```text
快照：`3d6c85595` @ 2026-10-24 23:48 +08（**证据采集点**；此后本文件只有 §7 文档提交，
Rust 源码未变，二进制 sha256 不变）。
- Rust release：`mir2-rs/target/release/mir2-server.exe`
  sha256 `b505fc4c5b568d47d039829c64f66ae9c4556e904836d748dcc92ff5c4f3e2a4`
  （`mir2-server.build.json`: `head=3d6c85595`, `dirty=""`）。
- C# oracle：`csharp-root/M2GameSvr/GameSrv.exe` sha256
  `ab6550080736e6610c86de226751c66a319e61e6be2176630a2374ea5819392f`；
  私有栈 SelGate（T-172 构建）sha256 `bc8cf4d708e0d88cd261ffcfca4b8ea47b85a30b12dd1ded12b94439d00c1a27`
  （与 `selgate-build/SelGate.exe` 相同）。
- 红线：全程未起栈、未占端口（`preflight.py` 的 `ports` 步只做 connect 探测，12 端口全空）、
  未连库/写库；pin smoke 走 `-CheckPinsOnly`（不派生进程）。

$ bash -n tools/window/run_batch.sh && echo SYNTAX_OK
SYNTAX_OK

$ bash tools/window/run_batch.sh --list          # rc=0（17 行；修后实跑）
#   scenario             profile       extra
1   login_only           alt
2   npc_click_auto       alt
3   npc_click            alt
4   dice_chain_auto      alt
5   dice_chain           alt
6   gm_time              alt
7   gm_batch             alt
8   move_walk            alt
9   merchant_dialog      alt
10  king_dialog          alt
11  king_castle_war      alt
12  npc_script_chain     alt
13  equip_struck_save    alt
14  bag_equip            alt
15  eat_item             alt
16  spell_cast           window-magic
17  upgrade_retrieve     alt           --timeout-ms 720000

$ bash tools/window/run_batch.sh --dry-run       # rc=0（17/17 计划；未起栈）
run_batch: start card=openmir2-window-run-2 run_dir=E:/tmp/window/run3 head=3d6c855951ac selected=17 dry_run=1
run_batch: PLAN rebuild: powershell … tools/window/rebuild-server.ps1 -SyncStartRust -RequireClean
run_batch: == preflight private（tools/window/preflight_private_stack.py）
  PREFLIGHT_PRIVATE step=inventory status=OK detail=22 个部署件齐全，baseline 非空
  PREFLIGHT_PRIVATE step=selgate_listenport status=OK detail=ListenPort=17100 + SelGate.exe 与 T-172 构建同 sha256 + AddrTable→:17100
  PREFLIGHT_PRIVATE step=route_525 status=OK detail=C#/Rust ServerInfo→17200；C# RunGate=(17200, 15000)、Rust RunGate=(17600, 15000)
  PREFLIGHT_PRIVATE step=pin_static status=OK detail=window-magic: rust env MIR2_CLIENT_ABILITY_LAYOUT=0 + -PinChecks/-PinLog 已接线；alt 未污染
  PREFLIGHT_PRIVATE step=pin_smoke status=OK detail=pin 链 smoke rc=0（-CheckPinsOnly，不派生/不等端口）
  PREFLIGHT_PRIVATE step=binary_stamp status=OK detail=head=3d6c855951ac sha256=b505fc4c5b56 built_at=2026-10-24T23:48:33+08:00
  PREFLIGHT_PRIVATE step=start_rust_exe status=OK detail=-ServerExe 已对齐：…/dev-2/mir2-rs/target/release/mir2-server.exe
  PREFLIGHT_PRIVATE step=roster status=OK detail=17 场景（alt×16 + window-magic×1：spell_cast），不含 pickup_item，rules.json 在位
  PREFLIGHT_PRIVATE SUMMARY pass=8 warn=0 fail=0
  PREFLIGHT_PRIVATE RESULT PASS
  [OK] binary: …/mir2-rs/target/release/mir2-server.exe（sha256=b505fc4c5b56…）
  [OK] ports: 12 个端口全空（15000,15100,15500,15600,15700,16000…）
  [OK] pin: pin 接线一致（1 侧已接线；未声明 pin 的侧不适用）   # window-magic
  PREFLIGHT PASS（private + alt + window-magic）
  PLAN n=1 name=login_only profile=alt extra=[] out=E:/tmp/window/run3/login_only
  …
  PLAN n=17 name=upgrade_retrieve profile=alt extra=[--timeout-ms 720000]
run_batch: dry-run 完成：未起栈、未占端口、未写场景；计划 E:/tmp/window/run3-dry-run/batch.log

$ python -I -B tools/window/preflight_private_stack.py --selftest
GREEN: preflight_private_stack selftest 27/27

$ python -I -B tools/conformance/card_window.py --selftest
GREEN: card_window.py selftest 279/279

```

### 7.1 门禁与冲突检查（本检出，2026-10-24 23:50 +08）

| 检查 | 结果 |
|---|---|
| `card_window.py --selftest` | GREEN **279/279** |
| `preflight_private_stack.py --selftest` | GREEN **27/27**（新增 3 条 roster 牙：17 条 OK / 加回 pickup 18 条红 / 17 条含 pickup 红） |
| `cargo fmt --all -- --check` | rc=0 |
| `cargo clippy --workspace --all-targets -- -D warnings` | rc=0 |
| `cargo test --workspace` | rc=0 |
| `git diff --check` | 通过（提交时） |
| `t_reservation_gate` / `conflict_marker_gate` / `stale_pointer_gate` / `whitelist_anchor_gate` / `m3_whitelist_registry_audit` / `board_coverage_gate` | 全 rc=0 |
| `dynamic_count_stamp_gate` | **基线 `0ef4e3b96` 即 rc=1**：`docs/plan-3day-20261024.md:11-12` 的「R 卡 206 张 / 已完成 11 / tracked=0」缺快照戳（该文件不在本卡写集，本卡未改）。本卡新增文档单跑该门 ⇒ rc=0（`--path docs/window-run2-prep-20261025.md`）。已在 patch/status 点名，交 orchestrator 补戳或裁定 |
| `git merge-tree`（vs `walgit/master`） | 无冲突（结果见 patch body） |

## 8. 残留物与禁止事项

- 残留：`E:/tmp/window/run3-dry-run/`（本次 dry-run 日志）；`E:/tmp/conformance/start-rust.ps1.bak-*`
  （同步 `-ServerExe` 前的备份）；`mir2-rs/target/release/mir2-server.build.json`（构建戳，target 内，不入库）。
- 禁止：启动任何服务端/客户端、占端口、写生产库、改场景 JSON、`--allow-production-db`、
  停 mysqld、用 `--no-preflight` 出正式证据。
- **已封存场景**：`pickup_item` 结构性不可判（T-19/T-25，见 `docs/pickup-item-reachability-20261024.md`），
  已从 run-3 清单移除、不进入窗口；把它加回清单（或清单不是 17 条）会被 `run_batch.sh` 与 preflight `roster`
  双双拒绝（rc=2）。本卡（`openmir2-window-run3-scenarios-blocked`）的牙即此二条。
- 已知限制：`pin_smoke` 依赖**上一轮** game-svr 日志（本机 `E:/tmp/conformance/rust-logs/game-svr.out.log`）；
  全新机器/日志被清时只能 `--skip-pin-smoke` 并登记。
- run-2 证据仍在 `E:/tmp/window/run2/`（其 README 里的 rust sha 指向已删除的 `wt-worker-73` 构建，
  **不可**直接当作 run-3 证据；run-3 必须用本轮重建的二进制重跑）。run-1 证据目录已不在本机，
  以 `docs/window-run-1-20261024.md` 的文字记录为准。
