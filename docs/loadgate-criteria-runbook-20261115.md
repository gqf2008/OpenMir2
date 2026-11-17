# 压测判据缺口修复与三档离线复算（2026-11-15）

卡：`openmir2-loadgate-criteria-gaps`（P1，tier L）
数据：`E:/UserFleet/archive/load-tiers-20261115-0245/`（200/500/1000 三档 + `supp-hold360` 补充轮）
实现：`tools/botload/load_gate.ps1`、`tools/botload/load_report.py`（本卡不重跑真机窗口，只做离线复算）

## 0. 总监硬规矩（先写在这里，所有结论都受它约束）

1. **判据修好之前，任何一轮压测都不得给 GREEN。** 旧口径下 `internal_errors=1061` 判 GREEN、BotSrv OOM 只打
   WARN，这比没有判据更危险——会让人对着「绿」做决策。
2. **每份压测结论必须显式列出四项：`login` / `no_resp` / `internal_errors` / `OOM`。** 任何一项非零都必须单独解释，
   不得藏在均值或 tick P99 里。
3. 四项之外另列 `tick_p99_ms` 与 `world_ok` 作为上下文；`world_ok` 目前不是默认判据（见 §3.3），但它是
   「tick 样本从多少人身上量的」的覆盖基数，结论里必须一起读。

## 1. 新口径

### 1.1 `internal_errors` 进入判据（A1）+ 受评审白名单逃生口

- **来源**：BotSrv 的 `LoadMetrics.InternalError()`，只在两个 catch 块自增：
  `ClientManager.SafeRun`（单个假人异常）与 `AppService.Run`（世界循环异常）。累计值随
  `load_stats.ndjson` 每 ~5s 落盘。
- **默认：容忍度 0。** 判的是该档最后一次落盘的累计值。这个计数器不是业务失败数，是「假人走了未建模路径」
  的缺陷信号；2026-11-15 已知正常轮次（tier-500、tier-1000@60s）实测精确为 0。
- **唯一放宽通道**：仓库内白名单 `tools/botload/internal-errors-allowlist.json`（见 §1.1.1）。
  默认文件 `entries: []`，即容忍度 0。**没有任何 CLI 参数可以把容忍度整体调大**；运行时不能新增条目。
- **判定语义**（总监 2026-11-15 追加）：
  1. 未列入任何条目的类别（按 `bots.log` 样本的「类型 + 消息」）⇒ **RED**；
  2. 已列入但该类别累计次数 > 条目的 `max` ⇒ **RED**；
  3. 已列入且 ≤ `max` ⇒ 不 RED，但报告 `internal_error_allowlist.matched` 仍必须列出
     **类别 / 次数 / 上限 / 条目 id**（「豁免也要留痕」）；
  4. 白名单文件**缺失 / 为空 / JSON 坏 / 条目非法** ⇒ 容忍度 0（fail-closed），
     `available=false` / `valid=false` 与原因落盘；**不是**「全部放行」；
  5. `internal_errors > 0` 但 `bots.log` 没有样本可归因 ⇒ **RED**（无法证明无害）；
  6. `load_stats.ndjson` 缺 `internal_errors` 字段 ⇒ **RED**（不得当 0）。
- **归因口径**：BotSrv 只打前 20 条（`ShouldLogInternalError`），所以「类别/次数」是**样本**，「总量」是 ndjson。
  每个样本桶按文件顺序匹配第一条命中的条目；预算 = 命中条目 `max` 之和（同一条目去重）；
  判定用**总量**与预算比较。样本里有未命中类型 ⇒ 直接 RED，不能靠别的类别预算兜住。
- **超阈输出**：`report.json.fails` 写 `internal_errors N > 白名单预算 B（类别 count/max（条目））` 或
  `internal_errors 含未列入白名单的类别：…`；stdout 的 `FIELDS` 行给出 `allowlist_budget` / `internal_excused`。
- **兼容**：`--criteria` 仍可选择是否判 internal。压测门禁与 `load_report.py summarize` 的默认判据都含
  `internal`；M1⑤ 的 `login,conn` 口径不变。**任何人用 `-Criteria` 去掉 `internal` 的轮次不得作为压测结论引用**
  （summary 会写 `criteria_complete=false` 并打 WARN）。

#### 1.1.1 白名单格式与评审规则

`tools/botload/internal-errors-allowlist.json`（当前 `entries` 为空）：

```json
{
  "version": 1,
  "entries": [
    {
      "id": "nre-playscene-beginscene-known",
      "category": "System.NullReferenceException",
      "message_pattern": "Object reference not set",
      "max": 0,
      "reason": "为什么已知无害：现象、证据、影响面；必须能区分未知同类",
      "owner": "openmir2-worker-<n>",
      "review_ref": "openmir2-<card-or-review>"
    }
  ]
}
```

| 字段 | 必填 | 语义 |
| --- | --- | --- |
| `id` | 否（缺省 `#<序号>`） | 报告里标识这一条 |
| `category` | 是 | 异常类型，精确等于 `bots.log` 里的 `System.*` 类型 |
| `message_pattern` | 是 | 消息的 Python 正则；**必须能区分已知无害与未知**，空/非法 ⇒ 该条不生效 |
| `max` | 是 | 该类别允许的累计次数（非负整数）；超出 ⇒ RED |
| `reason` | 是 | 为什么无害（判据/证据） |
| `owner` | 是 | 提出并维护这条的人 |
| `review_ref` | 是 | 哪次 card/review 批准的 |

- **评审规则**：白名单文件是**卡面 patch 的一部分**，必须由作者之外的 principal 在钉住的 commit 上评审；
  合并后任何修改都要新卡 + 新 review。运行时只读；`report.json.internal_error_allowlist` 记录
  `path` + `sha256` + 生效条目，评审核对「运行时没有被换文件」。
- `--allowlist` 只用于自测/复算；**结论轮次不得指向未评审文件**（路径会进报告）。
- **当前复算表（§2）是在空白名单（`entries: []`，容忍度 0）下得到的**；tier-200 / supp-hold360 的
  `internal_errors` 判 RED 与白名单无关，不能靠事后加条目追认。

### 1.2 BotSrv 崩溃检出（A2）

任一命中 ⇒ 该档 **RED**，证据写入档目录并进 `artifacts.sha256.json`：

| 证据 | 取数点 | 判定 |
| --- | --- | --- |
| 非 0 退出 | `Start-Process -PassThru` 的 `ExitCode`（仅限**进程自己退出**；门禁自己 `Stop-Process` 的不算） | 非 0 ⇒ RED |
| `Out of memory` | `bots.err.log` 全文，大小写不敏感 | 命中 ⇒ RED |
| Windows Application Error `0xe0434352` | `Get-WinEvent` Application 日志，`ProviderName='Application Error'`，进程名 `BotSrv.exe`，异常码 `0xe0434352`，时间窗从本档启动开始 | 命中 ⇒ RED |

- **证据归档**：每档写 `bots_runtime.json`（`self_exited` / `exit_code` / `exit_code_hex` / `err_log_oom` /
  `err_log_oom_lines` / `app_error_checked` / `app_error_events` / `oom_hits` / `crash`）；`bots.err.log` 原样保留；
  两者都进产物清单。
- **事件日志查询不可用**（无权限/无日志）不单独判 RED，但 `app_error_checked=false` 与错误原因必须落盘——
  「没查」不等于「没有」。
- **为什么以前漏了**：旧门禁只在爬坡循环里看 `$proc.HasExited`，稳态采样期间不看；`supp-hold360` 的 BotSrv
  在 02:59:39 OOM 退出后，门禁又采样了约 90s，最后用 `tick P99 201ms` 给 RED，OOM 只留在人工备注里。

### 1.3 窗口前提（A4）

- **`HoldSec ≥ 360s`**：任何要作为结论引用的压测必须满足。`<360s` 只能当冒烟；`report.json` 的
  `window.conclusion_valid=false` 会写清「只可冒烟，不得作为 GREEN 结论」。
- **进世界是串行载入，实测 ~5–7/s**：`supp-hold360` 的 `load_stats.ndjson` 里 `in_world` 从 5s 的 56
  涨到 75s 的 425（≈5.3/s）后平台。60s 窗口对 tier-1000 连「进世界」都走不完（tier-1000@60s 末 `in_world=0`）；
  tier-500 60s 也只到 404/500。窗口不足时，tick 样本来自一个尚未稳定的子集，不能当基线。
- 本卡**不把窗口塞进 `verdict`**（避免静默改变 M1⑤/冒烟的既有口径）；`verdict` 仍是测量判据，
  `window.conclusion_valid` 是结论准入。**对外结论 = `verdict` 与 `conclusion_valid` 同时为真**。

## 2. 三档离线复算对照表（第一交付物）

用 `E:/UserFleet/archive/load-tiers-20261115-0245/` 的原始产物按 §1 新口径逐档复算，**不重跑窗口**。

| 档 / 窗口 | login | no_resp | internal_errors | OOM 次数 | tick P99 | world_ok | 昨晚原判 | 新口径判定 | 原因 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| tier-200 / 60s | 100.0% | 0 | **1061** | 0 | 16ms | 198/200 | **GREEN** | **RED** | `internal_errors=1061`（稳态段线性增长：约 25s 起 71→1061）> 0；另外 60s < 360s，窗口无效 |
| tier-500 / 60s | 100.0% | 0 | 0 | 0 | 93ms | 404/500 | **GREEN** | **RED（结论级）** | 测量判据全过（`verdict=GREEN`），但 60s < 360s ⇒ `conclusion_valid=false`，不是有效结论（只到 404/500 进世界） |
| tier-1000 / 60s | 100.0% | 0 | 0 | 0 | 缺样本 | 0/1000 | RED | **RED** | 缺 tick 样本（原有判据）；60s 窗口内 bot 侧 `in_world=0`，窗口无效 |
| tier-1000 / 360s（`supp-hold360`） | 100.0% | 0 | **3875** | **1** | **201ms** | 425/1000 | RED | **RED** | tick 201ms > 100ms；`internal_errors=3875` > 0；`bots.err.log` 含 `Out of memory.`（另有 0xe0434352 记录，旧门禁未落原始事件日志） |

复算命令（不改归档、输出到临时目录）见 §5。

判定口径：「新口径判定（结论）」= `report.verdict` 与 `report.window.conclusion_valid` **同时为真**才是 GREEN。
tier-500 的测量 `verdict` 是 GREEN，但 `conclusion_valid=false`，所以结论是 RED（不是有效基线）。
`OOM 次数` 取 `report.required_fields.oom_hits`（`bots.err.log` 命中行数 + Application Error 事件数）。

### 2.1 昨晚的 GREEN 掩盖了什么

1. **tier-200 的 GREEN 盖住了 1061 次 `System.NullReferenceException`。** 异常全部发生在登录达标之后（稳态段）：
   首个非零采样在约 25s（71 次），之后每 5s 约 +70~150，末值 1061。堆栈指向
   `BotSrv.Scenes.Scene.PlayScene.BeginScene`，由 `RobotPlayer.Run` 调用——假人反复重入场景失败。
   在这条曲线下，`tick P99=16ms` 只是「还在跑的那部分假人」的样本，不能当健康证据。
2. **tier-500 的 GREEN 盖住了窗口不足与 96 个未进世界的假人。** 60s 时 `world_ok=404/500`，
   `in_world` 在 45s 就平台；按「进世界 ~5–7/s」的前提，500 档至少需要 ~90s 才走完载入。旧判据不看 `world_ok`，
   也不看 `HoldSec`，所以「404 人在世界的 60s」被当成了「500 档稳态」。
3. **tier-1000 的 RED 只归因于「缺 tick 样本」，掩盖了真正原因。** 60s 时 server 还在串行处理进世界，
   bot 侧 `in_world=0`；同一份报告里 `login=100%`，说明只看登录完全看不到这个问题。
4. **`supp-hold360` 的 RED 只归因于 `tick P99 201ms`，BotSrv OOM 没有任何判据痕迹。** `bots.err.log` 只有一行
   `Out of memory.`；BotSrv 在 02:59:39（窗口结束前约 90s）就退出，门禁继续采样并归算，
   OOM 只写进了人工 `results.json` 备注。新口径下这一档同时命中 tick / internal_errors / OOM 三条。

## 3. 判定入口（不可静默绕过）

### 3.1 默认判据

- `tools/botload/load_gate.ps1` 默认 `-Criteria "login,conn,tick,internal"`。
- `load_report.py summarize` 默认 `--criteria "login,conn,tick,internal"`，白名单默认
  `tools/botload/internal-errors-allowlist.json`（当前为空，容忍度 0）。
- 「缺 tick 样本」仍是独立开关 `--allow-missing-tick`，`--criteria` 不能替代它（2026-10-10 的恒绿回归项）。
- `internal_errors` 的放宽只走白名单（§1.1.1）；没有整体阈值参数，字段缺失 ⇒ RED（fail-closed）。

### 3.2 BotSrv 崩溃

- 门禁在**爬坡与稳态采样两个循环里都检查** `$proc.HasExited`；进程自己退出 ⇒ 取证、判 RED、停止本档采样。
- 门禁自己 `Stop-Process` 的退出码不参与「非 0 退出」判定（`stopped_by_gate=true`），但 `bots.err.log` 与
  Application Error 仍照查。

### 3.3 不进默认 `verdict` 的观测

- `world_ok` / `world_ok_pct`：默认不判（三档实测平台只有 198/404/425），结论里必须列出。
- 内存「峰值-均值」：只标注（M5 长跑另判），不并入 `verdict`。
- 本机负载窗（CPU≥40% 或 heavy≥4）：只标注，`loaded_window=true` 时 tick P99 未归属。

## 4. Runbook

### 4.1 跑一轮

```powershell
# 结论级窗口：HoldSec >= 360；多档连跑必须每档换干净栈
powershell -ExecutionPolicy Bypass -File tools/botload/load_gate.ps1 `
  -Tiers '200,500,1000' -HoldSec 360 -RestartStackPerTier -OutDir 'E:\UserFleet\load-<时间戳>'
```

### 4.2 写结论时必须带的四项

| 字段 | 来源 | 说明 |
| --- | --- | --- |
| `login` | `report.json.login_success_pct` | 稳态窗内 `login_ok` 峰值 ÷ 档位人数 |
| `no_resp` | `report.json.login_conn_diag.no_resp` | 发出 `CM_IDPASSWORD` 后没拿到 529 的连接数；`None` = 证据缺，不得当 0 |
| `internal_errors` | `report.json.internal_errors`（另看 `internal_errors_steady` / `internal_error_diag.types`） | 超 0 必须给 top 异常类型与归属；若走白名单，必须列 `internal_error_allowlist.matched`（类别/次数/上限/条目）与 `path`/`sha256` |
| `OOM` | `report.json.required_fields.oom_hits` + `report.json.botsrv` | 非 0 必须给 `bots.err.log` 行与 `bots_runtime.json` |

另列：`tick_p99_ms`、`world_ok`、`hold_sec`、`window.conclusion_valid`、`internal_errors_excused` /
`allowlist_budget`。任何一项非零/无效/被豁免都要单独解释。

### 4.3 红检（改坏必红）

```powershell
python -I -B tools/botload/load_report.py selftest
powershell -ExecutionPolicy Bypass -File tools/botload/load_gate.ps1 -SelfTestRed
powershell -ExecutionPolicy Bypass -File tools/botload/load_gate.ps1 -SelfTestCriteria
```

`-SelfTestCriteria` 不启动服务、不需要 BotSrv：它伪造 `load_stats.ndjson` / `bots.log` / `bots.err.log` /
`bots_runtime.json` / 白名单，走真实 `load_report.py summarize` CLI，断言：
OOM 伪造 ⇒ RED；NRE（`internal_errors>0` + 堆栈）⇒ RED；伪造非 0 退出 ⇒ RED；缺 `internal_errors` 字段 ⇒ RED；
干净对照 ⇒ GREEN；白名单四牙——**≤max ⇒ GREEN 且报告列出类别/次数/上限**、**>max ⇒ RED**、
**未列入类别 ⇒ RED**、**缺文件/为空/条目非法 ⇒ 容忍度 0**。

### 4.4 结论有效性

- `verdict=GREEN` 且 `window.conclusion_valid=true` 才是 GREEN 结论。
- `HoldSec < 360s`：冒烟可以，结论无效。
- `loaded_window=true`：tick P99 未归属，必须空载复测。
- 判据修好前的历史 GREEN 一律作废，按本表重判。

## 5. 离线复算命令

```python
# python -I -B tools/botload/offline_recalc.py 不存在；下面是自包含复算（只读归档，输出到临时目录）：
import json, os, subprocess, sys, tempfile
ARCH = r"E:/UserFleet/archive/load-tiers-20261115-0245"
OUT = os.path.join(tempfile.gettempdir(), "loadgate-recalc-20261115"); os.makedirs(OUT, exist_ok=True)
base = {"source": "offline-recalc", "self_exited": None, "stopped_by_gate": None,
        "err_log_oom": False, "err_log_oom_lines": [], "app_error_events": [], "oom_hits": 0,
        "app_error_checked": False, "app_error_check_error": "old gate did not record exit code / event log"}
runs = [("tier-200", 200, 60), ("tier-500", 500, 60),
        ("tier-1000", 1000, 60), ("supp-hold360/tier-1000", 1000, 360)]
for rel, count, hold in runs:
    d = os.path.join(ARCH, *rel.split("/"))
    rt = dict(base)
    err = open(os.path.join(d, "bots.err.log"), "rb").read().decode("utf-8-sig")
    if "out of memory" in err.lower():
        rt.update(err_log_oom=True, err_log_oom_lines=[err.strip()], oom_hits=err.lower().count("out of memory"))
    rtp = os.path.join(OUT, rel.replace("/", "_") + ".bots_runtime.json")
    json.dump(rt, open(rtp, "w", encoding="utf-8"), ensure_ascii=False)
    out = os.path.join(OUT, rel.replace("/", "_") + ".report.json")
    cmd = [sys.executable, "-I", "-B", "tools/botload/load_report.py", "summarize",
           "--stats", os.path.join(d, "load_stats.ndjson"), "--count", str(count),
           "--hold-sec", str(hold), "--criteria", "login,conn,tick,internal",
           "--bots-log", os.path.join(d, "bots.log"), "--bots-runtime", rtp, "--out", out]
    for flag, fn in (("--mem", "mem.ndjson"), ("--login-conns", "login_conns.ndjson"),
                     ("--hostload", "hostload.ndjson")):
        if os.path.exists(os.path.join(d, fn)): cmd += [flag, os.path.join(d, fn)]
    subprocess.run(cmd, check=False, stdout=subprocess.DEVNULL)
    rep = json.load(open(out, encoding="utf-8"))
    rf = rep["required_fields"]
    print(rel, "| login", rf["login_success_pct"], "| no_resp", rf["login_conn_no_resp"],
          "| internal_errors", rf["internal_errors"], "| OOM", rf["oom_hits"],
          "| verdict", rep["verdict"], "| conclusion_valid", rep["window"]["conclusion_valid"],
          "| fails", rep["fails"])
```

三档 + `supp-hold360` 的复算结果见 §2；`required_fields` 块逐档给出四项必列字段。

## 6. 残留物与后续

- 本卡只修判据，不修 `PlayScene.BeginScene` 的 NRE；该 NRE 是 BotSrv 侧真缺陷，按卡外另行处置。
  **当前白名单为空，不追认这个已知 NRE**——在它被修复或另开卡论证「已知无害」之前，它必须继续判 RED。
- 白名单是逃生口而不是事后追认工具：`entries: []` 是当前状态；任何新增条目都要单独评审
  `category`/`message_pattern`/`max`/`reason`，并在报告里留下 `sha256`。
- 归档里没有原始 Windows 事件日志，`supp-hold360` 的 0xe0434352 只能引 `results.json` 备注；新门禁会把原始事件
  证据写进 `bots_runtime.json`。
- 本卡未重跑真机窗口；§2 的判定是对既有产物的离线复算，不替代换机后的 360s 复测；§2 表是在**空白名单**
  （容忍度 0）下得到的。

## 7. 2026-11-15 起的新口径：错误类别普查（卡 `openmir2-botsrv-recv-thread-nre`）

- `internal_errors` 只是总量；新判据要求 **枚举出的每一个 fault 类别都为 0**
  （或按 `tools/botload/error-census-allowlist.json` 登记），见 `docs/botsrv-error-census-20261115.md`。
- **旧归档（本文件 §2 复算的那些 `load_stats.ndjson`）没有 `error_census` 字段**，按新口径会被
  fail-closed 判 RED（「缺 error_census」）。这是**有意**的：旧证据不得再用来宣布「全绿」；
  §2 的离线复算只能当「口径变更前」的对照，新结论必须用带 census 的新构建重跑。
- 每份新结论额外必列：`error_census_complete` / `error_census_fault_total` /
  `error_census_nonzero` / `scene_null_actor_skipped`（`report.json.required_fields`）。

### 7.1 父卡 scene_null 口径（master `39d418816` 起）

- **`scene_null_actor_skipped` 每份报告必须显式列出**（是 0 也要列，不许不提）。
- **上界判据**：稳态窗口内 `scene_null_actor_skipped / scene_rounds < 0.5%` 轮次；超界必须用
  `load_report.py summarize --scene-null-explanation "<单独解释>"` 显式解释，否则 RED。
- **语义**：并发下读到正被清尾的槽位（收包线程 `RemoveAt` 的瞬时空位），**安全跳过、不读不删不登记**；
  这是「已知且受限」，**不是「无错」**。
- `scene_list_race_skipped` / `scene_index_race_skipped` 是护栏触发的竞态证据，必须为 0；
  缺 `scene_rounds`（上界分母）⇒ RED。

### 7.2 报告措辞（硬规则）

- **不得写「夹具无错」**。census 落地后 **200 档会变红**（`recv_packet_null ≈ 10,968`）——**那是正确的红**。
- 500 档若为 0/很低，必须写「**500 为 0 是丢弃率 39.5%、到达帧少把竞态压下去了，不是修好**」。
- 正确表述：「本卡消除了 NRE@120/AOORE@125 两类；收包线程 NRE 尚未进入计数，由 census 卡承接，
  并在 200 档以 `recv_packet_null` 显形（正确的红）」。

## 空载判据（2026-11-15 总监定义）

- 语义：**「起跑前 CPU <20% 且全程无外部构建进程」**。
- 「外部」显式：cargo/rustc/dotnet/zig/msbuild 在窗口全程为 0；**被测栈自身负载造成的 CPU 峰值不算违规**（起跑前满足即可）。
- 每份窗口报告须记录：起跑前 CPU、全程 `heavy_samples`（进程数/n/names）、CPU min/mean/max，并区分「外部构建」与「被测负载」。
- 换档规则：**每档新栈、单档**；同栈只跑单档（前一档收尾的离世广播风暴会压住后一档起跑，实测进世界慢 4×）。
- **起跑后 60 秒 `in_world` 闸门（2026-11-15 总监规则）**：60s 时 `in_world` 低于档位人数的 90%
  （500 档即 <450）⇒ **就地判本轮无效**：停栈、不采数据、不进任何模型。理由：`in_world` 低意味着后面所有
  tick/延迟/丢弃数都是「只有 N 个真玩家」的量，而报告上它仍带着 `login 100%` 的伪装。
  实例：池卡 r2 `login 500/500`、`tick P99 16ms`、`internal_errors 0`，但 `in_world` 从 t≈30s 起恒 21/500
  ⇒ 整轮作废。历史教训：500 档 471/500（94%）曾被误读成 29 个失败，`login` 全绿也完全看不出。
- **每轮必报 `in_world` 峰值与稳态值，且写在结论句旁边**（不只在表格字段里）。任何人读结论时应能
  立刻看到「500 个里有多少真进去了」；缺这两个值的报告不得作为结论引用。

### 栈配置：bot 窗口只许用 Rust `game-svr`（2026-11-15 事故）

- 起栈后**必须检查** `start-stack.out.log`：要有 `START game-svr`，且**不得**出现 `GameSvr.exe`。
  **检测到 `GameSvr.exe` ⇒ 立即停栈、判本轮无效（`invalid_reason=csharp_game_svr`）、rc=2**——与 `in_world`
  闸门同等级，**不是注脚**：自查发现问题却不中止，等于把错栈的结论继续往下游送。
- 原因：C# oracle 的玩家数据拉取路径**本来就没实现**（`src/M2Server/Net/TCPNetChannel.cs:400 LogService.Info("待实现")`），
  500 bot 登录会在 `WorldServer::ProcessHumans` 里 `RemoveAt` 越界抛异常，只有 ~21 个能进世界。
  实例：2026-11-15 池卡 r2 用 `E:/MirServer/M2GameSvr/GameSrv.exe`（mtime 2026-10-22、来自已删 worktree `OpenMir2-wt-goods`）
  ⇒ 487 条 `ProcessHumans` 异常、`in_world` 恒 21/500 ⇒ 整轮作废。
- 健康栈形态（360s 窗口实测）：`DBSrv`(C#) + `LoginSrv`(C#) + **`game-svr`(Rust, 5000)** + `login-gate`/`sel-gate`/`game-gate`(Rust)。
- **以 `internal_errors` 为判据的窗口，起跑前必须跑一次 `BotSrv.exe --selftest-error-census`**，
  输出存进归档 `errorstack-selftest.txt`（并记录被测二进制 sha256）。理由：这条自检用真实
  `AppServer.LogOutputTemplate` 渲染真实 `NullReferenceException` 并断言「消息行 + 类型行 + `   at ` 帧」，
  把「诊断链路通」从「等某轮恰好复现 NRE」变成**确定性检查**。变异（`:32` 改回 `Logger.Error(ex.Message, ex)`）
  必须让它变红。**零异常的窗口不能证明诊断链路通**，只能写「未触发，诊断链路未获验证」。
  **这条前置检查是门禁，不是仪式（fail-closed）**：
  - 文件缺失 / selftest rc≠0 / 输出里缺少三条断言（消息行、类型行、`   at ` 帧）任一 ⇒ **本轮无效**
    （`invalid_reason=errorstack_selftest`，rc=2），不得「记录了就继续跑」。
  - **必须跑本轮实际使用的那个 `BotSrv.exe`**，并在同一份输出里记录该二进制的 **sha256 + mtime**；
    跑另一份二进制只能证明「某个构建诊断链路通」，不能证明「这一轮通」。
  - **反例可复核**：把 `src/OpenMir2/LogService.cs:32` 改回 `Logger.Error(ex.Message, ex)` ⇒ 三条断言必红、
    本门禁必拦。谁改回坏序，门禁先于窗口拦住它（对应自检 3 条 FAIL：`log-error-event-has-exception`、
    `-type-present`、`-stack-present`）。
- **门禁运行的三态：PASS / FAIL / INCOMPLETE**（2026-11-15 定）。凡出现下列任一形态，一律判
  **INCOMPLETE**——**既不得计入 PASS，也不得归咎于代码**：
  rustc/cargo 在编译期被外部或系统回收器杀死；进程异常退出且**无任何 `error[...]`/`could not` 诊断行**；
  日志里**没有 `test result:` 行**（⇒ 0 个测试执行）。
  INCOMPLETE 的证据必须齐：**被杀时刻的退出码、日志末 40 行、`test result:` 行是否存在、被采样进程与空闲内存快照**（采不到就写「未采到」）。
  实例：2026-11-15 池卡分支 `cargo test --workspace` 编译 26 crate 后被系统 OOM 回收器整体 kill，
  rustc exit 1 且无诊断行、零 `test result:` ⇒ 判 INCOMPLETE，重跑用 `--jobs 2` 降并发。
  **任何只报「第 N 个 crate 编译失败」而不给上述证据的门禁结论，不得作为合并依据。**
- **preflight 必须分开写 `CODE_HEAD` 与 `REPO_HEAD`**（2026-11-15 定）：
  - `CODE_HEAD=<commit>`：**被测二进制实际来源**（首选二进制内嵌 commit；没有就写构建时的工作树 HEAD 并标 `inferred`）；
  - `REPO_HEAD=<commit>`：起跑时的仓库 HEAD（可能包含其后若干**纯文档**提交，写明数量）；
  理由：把两者混成一个 `head=`，审计时无法回答「这轮跑的到底是哪份代码」。实例：360s 轮口径写「master `05a0e4815`」，
  但 `preflight.txt` 记的是 `head=e9b807e1b`——两者之间恰有 **1 个 docs-only 提交**（`src/` diff 为空），
  二进制 `dll 8d09c454…` 由 `05a0e4815` 构建。**`.build.json` 戳落地后 preflight 直接读戳即可。**
- **戳的产出与校验工具（2026-11-15，卡 `openmir2-csharp-stack-binary-provenance` 第 1 步）**：
  `tools/orchestration/stamp-binaries.ps1` 为 `game-svr` / `game-gate` / `login-gate` / `sel-gate`
  （以及 C# 侧 `DBSrv` / `LoginSrv` 等）写 `mir2-build-stamp/1` 戳；`-Check` 逐条判「戳存在 /
  sha256 实时一致 / head 可解析 / built_at 与 mtime 差 ≤5 分钟」，空目录或无二进制判 rc=2
  （不静默通过）；`-Selftest` 已登记进工具门。**第 2 步尚未实施**：把
  `provenance=unknown ⇒ rc=2` 接进起栈脚本、跑通五项通电条件；实现计划与牙清单见
  `docs/binary-provenance-stamp-20261115.md` 的 §7–§8。
- **`dotnet-counters` 采集必须记录目标进程**：落 `counters_target=<process> pid=<pid>` 到 `preflight.txt`
  与 `report.json`。**缺这项的采集，其数字不得出现在报告结论里**（只能进附录并标「未证」）。
  实例：2026-11-15 修复树 360s 的 `Allocation Rate≈418MB/s` 因归档未记 counters PID，只能降级为「疑为 BotSrv、未证」。
- **CPU 采样一律用 `typeperf`，不要用 `Get-CimInstance Win32_Processor` 循环**（2026-11-15 实测）：
  CIM 连续采样时 top consumer 就是**发起采样的 `powershell` 进程本身**，会把空载读数打到 38–62%，
  从而误判「机器不空载」；同机 typeperf 6 采样为 11.8–18.7%（max 18.7 / mean 14.2）。
  归属分析用**单次** CIM/Get-Process 快照（不要循环），并把 `cpu_attrib` 落归档。
  ⇒ 此前报告中「cpu_max=62%」类数字须标注「CIM 自污染，实测以 typeperf 为准」。
- **能预判会无效的运行，宁可等也不跑**（2026-11-15 总监标准）。判据已经会判它无效的运行
  （外部构建在跑、`heavy≠0`、栈形态可疑、60s 闸门必然不过），跑下去得到的不是「数据略差」，
  而是**归档里一份看起来像数据的东西**——将来有人翻到会当成有效轮次引用。实例：r2（错栈 + 
  `heavy=5`）与 471/500 的误读，都是这样产生或放大的。等条件满足再跑，成本永远低于事后作废。
- **一个窗口只跑一件事：跨会话和会话内都算**（2026-11-15 教训）。窗口开启期间，**任何**
  cargo/rustc/dotnet/zig/msbuild 构建都禁用——不管它来自别的会话、还是**同一条会话里另一个车道**。
  实例：同会话的 profiler 车道起 release 构建（56s），把池卡 500/90 窗口打成 `heavy_max=5`、
  `cpu_peak=100%`，`in_world` 恒 22/500、Gen2 GC=0 ⇒ **整个 Run A 作废**。
  落地：开窗前先在会话内广播「构建停」，各车道回一条「无进程/已停」确认；`heavy_max=0` 且
  起跑前 CPU<20% 才允许起跑；跑批期间任何车道要编译，**先申请窗口**。
- **`heavy=0` 不等于机器没被占用：preflight 必须能回答「这点 CPU 是谁的」**（2026-11-16 新增）。
  现有 preflight 只看构建名单（rustc/cargo/dotnet/msbuild/VBCSCompiler/zig），**名单外进程完全不可见**。
  实例：D1 对照轮 preflight 第 1 轮 `heavy=0` 但 `cpu_mean=29.26 / cpu_max=51.57`，第 2 轮才降到 18.72；
  当时采集器**没有 per-process CPU 采样**，这 29% 至今**归不了因**（同会话当时只有建库/390KB 导表，
  且都在采样前 30 s 结束）⇒ 只能作为「基线混杂项」写进对比，不能当干净基线。
  落地要求：**每轮 preflight 除 heavy 名单外，还要取一次进程 CPU 快照并点名 >5% 的进程**（单次采样，
  不许 CIM 循环——见上一条自污染），写进归档 `preflight.txt`；点名不出 ⇒ 该轮 CPU 基线**不可引用**。
- **牙（断言）锚定结构或语义，不锚定偏移量/阈值常量**（2026-11-16 新增，两次实例）。
  「往前 N 字节必须出现某串」这类窗口断言会在**排版/注释变化**后误红或误绿：
  实例一：接线牙用 400 B 窗口，实际条件行在 410+ B 外 ⇒ 实现正确却红（改成「取最近的 `host_owns(` 条件」后稳定）；
  实例二：某用例必须把缓冲缩到 1 B 才不假绿，说明它锚的其实是实现细节而不是行为。
  落地：断言优先用「最近的前驱/后继匹配」「AST/结构判据」「语义值」；确需 Z 窗口时，
  必须同时给出**负例**（把被钉的东西去掉后必红），否则窗口本身就是假绿源。
- **每份窗口报告必须记 DB 行数**（2026-11-16 新增）：`mir2_db` 的 `characters` / `characters_bagitem` /
  `characters_storageitem` / `characters_item` 行数（一条 SQL 即可）。原因：C# DBSrv 取档对这 4 张表里的
  3 张**没有 PlayerId 索引**（`EXPLAIN` 为 `type=ALL`），**每次进世界都全表扫**；角色数增长会按 1/行数
  压低进世界速率。实例：1500 档留下的 1498 个角色把 bagitem 从 337,733 撑到 406,687（**1.204×**），
  同期 1000 档进世界速率 13.0→10.5/s（**1.238×**）。⇒ 不记行数的跨轮比较**不是同口径**。
- **文本类/偏移类的牙，先确认「跑的是不是你以为的那份字节」**（2026-11-16 三次同族事故合并成一条）：
  ① 断言别锚**偏移量/阈值常量**（400B 窗口实测在 410+B 处误红；改用「最近的匹配臂/结构锚」）；
  ② 改文件后的"恢复"别用会重写换行的 API（`write_text` 按平台重写 ⇒ 用 `write_bytes` 恢复并断言 sha）；
  ③ Windows 车道的 `.rs` 工作文件若是 CRLF，`include_str!` 读到的就是 CRLF ⇒ 6 条源码文本断言
  假红（2026-11-16 worker-100 实例）；按 `.gitattributes`（`*.rs text eol=lf`）转 LF 后全绿、提交不变。
  ⇒ **跑任何基于文本/偏移的断言前，先核行尾与文件字节**；红灯先问"红的是不是我以为的那条"。
- **改写脚本必须幂等**：同一脚本第二次运行不能因为"已经改过"而判红。实例：`AddrTable.txt`
  的 `:7100 → :7400` 首次实现只在找到 `:7100` 时成功，第二次运行直接 fail-closed，把正常起栈挡掉。
  判据：已是目标态 ⇒ 跳过并打印 `already`；两种态都没有才失败。
- **检出/开工第一步：行尾体检（Windows 车道硬规矩，2026-11-16 第三次踩到后固化）**。
  新 worktree / 新 clone 的 `.rs` 若被检出成 CRLF，`include_str!` 读到的就是 CRLF ⇒ 源码文本断言假红，
  且提交会把整文件改动带进 diff。开工前跑：

  ```bash
  # 只列「含 CRLF 的 .rs」；空输出才是干净
  grep -rlI $'
' --include='*.rs' mir2-rs | head
  # 逐个转 LF（只转需要转的），然后**验证 git diff 只显示预期文件**
  git diff --numstat
  ```

  判据：转换后 `git diff` 只应出现你真正改动的文件与行数；出现整文件行数 ⇒ 说明转行尾没生效或转错了文件。
  与「断言锚偏移量」「`write_text` 重写换行」合并看：**凡基于文本/偏移的判据，先确认跑的是你以为的那份字节。**
- **建角/删角有「1 秒守卫」，且它是**按会话**计的**（2026-11-16 实测，M1 建角被挡的直接原因）。
  C# DBSrv `src/DBSrv/Services/Impl/UserService.cs`：`CM_NEWCHR` / `CM_DELCHR` 分支都要
  `(GetTickCount() - userInfo.dwChrTick) > 1000` 才处理，否则只打
  `[Hacker Attack] NEWCHR <account>/<ip>` 并**静默丢弃（不回 521/522）**；而 **`CM_QUERYCHR` 分支会先刷新
  `dwChrTick`** ⇒ 「查到空角色列表后立刻发建角」必中。
  **`dwChrTick` 是 `userInfo`（= 单会话）字段** ⇒
  · **每个账号自己的 query→newchr 之间必须 >1000 ms**（建议 1.2 s，别贴 1000）；
  · **不同账号可以并行**（各自一条会话），所以 N 个角色的总时长**不是** N×1 s——
    **单连接串行**才是 1/s。真正的批量约束是 sel-gate 的 60 s 排队超时 + DBSrv 吞吐。
  同一账号的 DELCHR 重试也一样要 >1 s。
- **门禁跑之前先看内存；被系统杀掉的一律判 INCOMPLETE**（2026-11-16 夜间两次事故后固化）。
  本机 **31.1 GB 总内存**；`cargo test --workspace` + `clippy --workspace` 同时跑会把机器压到临界，
  2026-11-16 就发生过「后台门禁被系统因内存不足整体 kill」。
  落地三条：
  1. **开跑前查空闲内存**（`Get-CimInstance Win32_OperatingSystem` 的 `FreePhysicalMemory`）：
     **< 8 GB 不开**；两包以上并行编排时按 8 GB/包估。
  2. **一律 `-j 2`**（`cargo test --workspace -j 2` / `cargo clippy ... -j 2`），不要用默认并发。
  3. **被杀 ⇒ 判 INCOMPLETE**（三态：不算绿、不算红、不许用"定向窄检查"顶替全量门禁——
     窄检查只能证明那几处不红，证明不了全量绿）。要恢复绿灯必须**整条门禁重跑**。
- **产物来源判据（2026-11-17 起硬规矩，一晚三次「看起来对」之后固化）**。判据只认**字节**，不认观感：
  1. **多产物栈逐个核 sha**：运行中进程的 exe **路径** + 该文件 **sha** + **本次构建产物 sha** 三者相等；
     `DBSrv.exe` / `DBSrv.dll` / `DBSrv.Storage.MySQL.dll` 缺任何一件，就是「新 exe + 旧业务代码」
     （2026-11-17 实测：只换 exe 与策略 DLL 时外层 warn 不出现，差点误判『改的代码没生效』）。
  2. **产物内查特征字节**：.NET 程序集里的字符串常量在 `#US` 堆是 **UTF-16LE**；Rust 可执行文件是 **UTF-8**。
     交付前列一张「应含 / 不应含」表逐条跑（例：`DBSrv.dll` 应含 `[EntryTelemetry]`，storage dll 应含
     `GetQryChar 异常`，`game-gate.exe` 应含 `ENTRY_TELEMETRY`），一条不真就不算换上了。
  3. **mtime、文件大小、路径、「我记得改过」都不算证据。**
  4. **起栈脚本的默认行为要读代码确认**：`Copy-Runtime` 这类「只在目标不存在时复制」的实现，会让影子目录
     **永远是旧的**；而就绪断言若只核 exe 路径不核内容 sha ⇒ 路径对、内容是旧的、断言全绿。实例：M1 栈
     shadow 仍是 2026-10-09 的三产物（`8006c895… / 1763a55b… / bcd7781a…`），加 `-RefreshRuntime`
     后才是当前构建（`44fd7a61… / facd51e6… / 1295963f…`）。同类登记卡：
     `openmir2-dbsrv-build-deps`（构建产物缺依赖）、`openmir2-acceptance-stack-bin-provenance`
     （两张 acceptance 栈脚本 `-RustBinDir` 默认指仓外共享 target 的 debug 口径、只判存在性）。
  5. **印章自洽 ≠ 由当前 master 构建**（2026-11-17 实例，比计数本身更值钱）：`m1-bin/*.stamp.json`
     四件与文件逐个 sha 相等（4/4）只证明「**同一份产物被一致引用**」；它不证明「这份产物由当前
     master 构建」。来源要看 stamp 里的 `head`（与 `dirty`）：当晚 4/4 自洽，但 stamp 记的是
     `head=2c1a3338 dirty=4 built=2026-11-16T23:22`，而 master 已是 `aa5267632` ⇒ **印章产物比 master 旧**。
     要「由目标 rev 构建」必须**比对 `stamp.head` 与目标 rev**（且 `dirty == 0`）；只核 sha 相等会
     把「旧但自洽」读成「就是我要的那份」。
     **DBSrv 三产物长期不在印章体系内**（历史：`m1-bin` 只有 game-gate/game-svr/login-gate/sel-gate
     四件 `.stamp.json`，**没有任何 `DBSrv*`**）⇒ 说「印章态 DBSrv」是错的说法，它的来源只能靠
     构建记录 + 运行中进程 sha + 产物内字面量自证。**2026-11-17 起已纳入**：
     `tools/orchestration/stamp-binaries.ps1` 对 `DBSrv.exe` / `DBSrv.dll` / `DBSrv.Storage.MySQL.dll`
     产 `.build.json` 戳，起栈脚本在**影子路径**上 `-Check -RequireAll` 后才起进程（卡 `openmir2-dbsrv-build-stamp`）。
  6. **release 产物不可位级复现（2026-11-17 实测，不是缺陷、是必须记住的前提）**：同一份源码
     （`git diff --stat <old> <new> -- mir2-rs` 为空）在不同 worktree 重建后，
     `game-svr.exe` 的 sha **会不同**（`ec125457…` → `92b2e6af…`）。本项目 Rust release 未做
     reproducible-build 配置（嵌入路径/时间戳类差异）⇒ **sha 只能回答「这个文件是不是刚才那一份」，
     不能回答「它是不是由同一份源码构建的」**。判源码同一性看 `git diff`/rev；判产物体在位看
     **sha + 产物内字面量**。任何「同源应当同 sha」的断言都是错的，写判据前先记住这条。
- **本机 HIPS 会静默阻断「以 `Expect` 开头的 PowerShell 参数名」（2026-11-17 实测）**：与
  `-ExecutionPolicy Bypass` 同现、且从 PowerShell 父进程**嵌套**启动时，整条命令行被吞——
  子进程退出码 `-1`、stdout/stderr **全空**、脚本一行都没执行（无事件日志、无提示）。实测
  `-ExpectHead` / `-ExpectedHead` / `-Expect` 被拦，`-RequireHead` / `-HeadExpect` / `-Other` 正常。
  ⇒ 工具参数名避开 `Expect*`（本仓 `stamp-binaries.ps1` 用 `-RequireHead`）；脚本里对子进程 rc
  **必须判 `-ne 0` 即 RED**（`-1` 不是「没跑」的免罪符，它就是失败）。
- **停服务进程用 `CTRL_BREAK_EVENT`，不要用 `CTRL_C_EVENT`（2026-11-17 实测，一轮白跑 120s）**：
  本环境里子进程会**继承「忽略 Ctrl+C」的属性** ⇒ 向它发 `CTRL_C_EVENT` 会被内核**静默丢弃**
  （无效果、无错误、无日志）；`Program.Main` 的 `Console.CancelKeyPress` 根本不会触发。
  停 DBSrv 一律发 **`CTRL_BREAK_EVENT`**；**判据不是命令返回码**，而是两条同时成立：
  ①日志出现 `Ctrl+C pressed`（`Program.Main` 的 `CancelKeyPress` 处理器打了这一行）；
  ②**`FINAL_LINES >= 1`**（`[EntryTelemetry] final …` 存在）。缺任一条 = **没走优雅关服**，
  终值不可用（只能拿到 ≤10s 旧的心跳快照）。这是与「产物看似对」「去重吞终值」同族的**静默失败**：
  命令"成功返回"了，但程序根本没收到停止信号。
