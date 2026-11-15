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
- **一个窗口只跑一件事：跨会话和会话内都算**（2026-11-15 教训）。窗口开启期间，**任何**
  cargo/rustc/dotnet/zig/msbuild 构建都禁用——不管它来自别的会话、还是**同一条会话里另一个车道**。
  实例：同会话的 profiler 车道起 release 构建（56s），把池卡 500/90 窗口打成 `heavy_max=5`、
  `cpu_peak=100%`，`in_world` 恒 22/500、Gen2 GC=0 ⇒ **整个 Run A 作废**。
  落地：开窗前先在会话内广播「构建停」，各车道回一条「无进程/已停」确认；`heavy_max=0` 且
  起跑前 CPU<20% 才允许起跑；跑批期间任何车道要编译，**先申请窗口**。
