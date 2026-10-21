# window-20261023 — 真机窗口场景包（G1 / G2a / G3a / G5a，tier=L 夹具）

**它是什么**：把 `docs/window-round2-runbook-20261023.md`（G1/G2a 受控登录、5 条登录判据）
与 `docs/window-run-sheet-20261022.md`（§A4 优雅停机、§B5/§B7 摆位、§D.4 G5a）里已经实跑过的
命令序列固化成**一条可执行命令**：摆位 → 起栈 → 影子 7600 + 代理 7200 → 客户端 → 导出 → 判据 → 收窗。

**它不是什么**：

- 不是新的判据口径。J1..J5 的语义逐字来自 runbook §3；G2a 的必选帧逐字来自 run sheet §D.2；
  G3a/G5a 的判据仍用既有 checker（`check_sm_hit.py` / `check_attack_observability.py` / `s2c_diff.py` /
  `check_db_paths.ps1`）。本包只做**编排 + 取证路径 + fail-closed 守卫**。
- 没有真机验证过。本车道（tier=L）**没有**起栈、占端口、写数据库；`--selftest` 只证明
  「命令序列可执行、守卫会咬人、判据逻辑对合成输入成立」。真机证据必须由窗口执行人按下面跑出来。
- 不替代 `docs/window-round2-runbook-20261023.md` 的授权/中止条件（§2.5）、E1–E4 边界与收窗清单（§5）。

| 文件 | 作用 |
| --- | --- |
| `preflight.py` | 开窗前一条命令：九游戏端口 DOWN / 7600 DOWN / 3306 UP / 镜像 provenance / 测试库已从生产克隆。**只读**。 |
| `run_scenario.ps1` | 六个场景（G1、G2A-R1/R2/R3、G3A、G5A）的阶段编排；默认 dry-run，`-Go` 执行。 |
| `scenarios.json` | 场景与期望的唯一数据源（`run_scenario.ps1` 与 `judge_login.py` 都读它）。 |
| `judge_login.py` | G1/G2a 的 J0..J5 单一入口；J1 复用 `check_client_s2c_frames.check()`，J5 复用 `s2c_diff.py --out`。 |
| `sql/*.sql` | G1 摆位（runbook §2.2）、G2a R1/R2/R3（run sheet §B5）、G3a attack（§B7）。全部只写 `mir2_test_*`。 |
| `s2c-whitelist-d9.jsonl` | runbook §3.3 的 3 条 52/D-9 白名单（每条带 reason + reviewer）。 |

复用（不重造）：`tools/loginfixture/prepare_safe_login.py`（摆位守卫）、
`tools/client/mir_flow.ps1`（客户端）、`tools/m2-acceptance/{export_s2c.ps1,start_rust_stack.ps1,s2c_diff.py,check_db_paths.ps1}`、
`mir2-rs/tests/parity/m3/start-csharp-hook.ps1`、`tools/capture/mir2_proxy.py`、`tools/dbsnap/dbsnap.py`、
`mir2-rs/tests/parity/evidence/M1/s4b-slice5-exitpaths/send_ctrlc.ps1`、
`mir2-rs/tests/parity/world/client-e2e-enter-gaps-20261013/{check_sm_hit.py,check_attack_observability.py,check_client_s2c_frames.py}`。
另复用既有两个 `run_*_pair.ps1` 的**离线自检**（`-Phase paircheck`）：它们驱动的是 bot 攻击/背包序列，
与本包 G3a 的单客户端构造（run sheet §B7）不同，故不塞进场景流，只把它们的 fail-closed 自检当开窗前离线门。

---

## 1. 开窗当天（照抄即可）

```powershell
$PKG = 'E:/Users/gxh/Documents/GitHub/OpenMir2/mir2-rs/tests/parity/m3/window-20261023'
$WIN = 'E:/tmp/window/20261023'

# 0) 全部离线自检（不占端口、不起栈、不写库）
#    可选：-Phase paircheck -Go 复用两个 run_*_pair.ps1 的 -SelfCheckNoStack（离线，约 5–10 分钟）
powershell -NoProfile -ExecutionPolicy Bypass -File "$PKG/run_scenario.ps1" -Selftest
python -I -B "$PKG/preflight.py" --selftest
python -I -B "$PKG/judge_login.py" --selftest

# 1) 开窗前一条命令（九端口/7600/3306/provenance/测试库克隆）
powershell -NoProfile -ExecutionPolicy Bypass -File "$PKG/run_scenario.ps1" `
  -Scenario G1 -Side rust -Phase preflight -Go

# 2) 看计划（默认 dry-run，不执行任何一条命令）
powershell -NoProfile -ExecutionPolicy Bypass -File "$PKG/run_scenario.ps1" `
  -Scenario G1 -Side rust -Win $WIN

# 3) 执行一侧（写测试库需 -Yes；先 C# 再 Rust，侧间 restore 由 -Phase close 做）
powershell -NoProfile -ExecutionPolicy Bypass -File "$PKG/run_scenario.ps1" `
  -Scenario G1 -Side csharp -Win $WIN -Go -Yes
powershell -NoProfile -ExecutionPolicy Bypass -File "$PKG/run_scenario.ps1" `
  -Scenario G1 -Side rust   -Win $WIN -Go -Yes
```

**安全默认**：不加 `-Go` 只打印 `CMD ...`（自检里有一条断言钉死「未加 -Go 时不执行任何命令」）。
写 `mir2_test_*` 需要同时给 `-Yes`；`prepare_safe_login.py` 与 `dbsnap restore` 在没有 `-Yes` 时只做
dry-run。SQL 文件在执行前先过守卫：出现 `mir2_db|mir2_account|mir2_data`、`USE`、DDL、`INTO OUTFILE`
或没有 `mir2_test_*` 目标 ⇒ 当场拒绝（不执行）。

C# 侧前置（runbook §1.2）不由本包代做：`<WIN>/csharp/{pack,repo}` 必须先按 runbook 复制好；
`<WIN>/db/base-test` 必须先按 runbook §4.1.3 拍好，否则 `close` 阶段的 restore 会报缺目录。

## 2. 场景、期望、证据

| 场景 | 构造 | 机器判据 | 证据（`{WIN}` = 窗口目录） |
| --- | --- | --- | --- |
| **G1** | `winplay0`，map 0 `(311,666)`，满血；`-WalkClicks 0 -SkipTalk -SkipLogout -SkipReenter` | J1 15 帧 + J2 `654` 在 46 后恰 1 条 + J3 `42`×2/`657`×2 次序 + J4 第二 42 后 `100×4` + J5 s2c_diff | `{WIN}/{side}/s2c/capture.jsonl`、`{WIN}/judge/g1-{side}.json` |
| **G2A-R1** | `aaa`，无技能 + 戒指 354(Slot7) ⇒ `210` | `--expect-enter-seq 658,210,51,...,46`（D.2 必选帧） | `{WIN}/{side}/g2a-r1/s2c/capture.jsonl`、`{WIN}/judge/g2a-r1-{side}.json` |
| **G2A-R2** | 无技能 + 无戒指 ⇒ `212` | `658,212,51,...,46`；粘滞看 characters_magic 仍 0 行 | `{WIN}/{side}/g2a-r2/s2c/capture.jsonl`、`{WIN}/judge/g2a-r2-{side}.json` |
| **G2A-R3** | `MagicId=3` + 木剑 207 ⇒ `621`+`211` | `658,51,...,752,621,211,811,46` | `{WIN}/{side}/g2a-r3/s2c/capture.jsonl`、`{WIN}/judge/g2a-r3-{side}.json` |
| **G3A** | 三个 subrun：`attack (605,578)` / `guardlist (291,666)` / `reverse (311,666)`；每 subrun 独立 `{WIN}/{side}/g3a-{sub}/` | `check_sm_hit.py --expect count,order --min-count 1 --lead-ms 250 --expect-recipients count=1`；`check_attack_observability.py --expect incoming --min-struck 1`；GuardList/反向点用 `s2c_diff --start-ident 10 --upto-ident 41 --from-x/--from-y` | `{WIN}/{side}/g3a-{sub}/s2c/capture.jsonl`、`{WIN}/judge/g3a-{sub}-*.json` |
| **G5A** | `aaa` 在世界中（`-LeaveClientRunning`）⇒ `send_ctrlc.ps1 -Event 1` 优雅停 ⇒ before/after | 关键值：MapName/CX/CY/Level/MaxHP/MAxMP 不得漂移；`LASTUPDATE` 是否推进记 `G5_WRITE_OBSERVED`；E1-b 才跑字段级 `check_db_paths -Canonical` | `{WIN}/db/g5-{side}/key-{before,after}.tsv`、`{WIN}/judge/g5-{side}.json` |

G2a 的每一轮有独立目录 `<WIN>/<side>/g2a-rN/`，J5 的 `--golden` 取同轮的
`<WIN>/csharp/g2a-rN/s2c/capture.jsonl`，不会被 R1/R2/R3 互相覆盖。

摆位（`-Phase pin`）对每个场景都显式传 `--level/--hp/--max-hp`（`prepare_safe_login.py` 缺任一即 rc=2）：
`scenarios.json` 只钉会漂移的项（G1 的 16/16、各轮 Level），其余用夹具常量 `level=1, hp=max_hp=19`
补齐，两侧同值。G3a 的抓包/导出都在 `<WIN>/<side>/g3a-{sub}/`：代理按 `g3a-{sub}/proxy` 落
`chunks.ndjson`，`-Phase export` 读**同一目录**（不再是 `<WIN>/<side>/proxy`）。

`-Phase judge -Go` 会真正写出 `{WIN}/judge/g5-{side}.json`（含 `key_before` / `key_after` /
`delta` / `write_observed` / `result`，写完用 `Test-Path` 自证）；`-E1b` 通过后另写
`{WIN}/judge/g5-e1b-{side}.json`（`check_db_paths.ps1` 的逐路径结果）。

`--52-layout` 由 side 决定：C# = `oracle40`（40B），Rust = `client58`（58B），与 runbook §3.1/§3.3 同口径。
G2a **不使用** `auto` 预设（runbook §D.2：`auto` 会把 210/212 写成可选，不能证明本轮真的出了）。

**golden 缺失 = PREREQ，不是降级**：`-Side rust` 且 `-Go` 时，若同轮
`<WIN>/csharp/[g2a-rN/]s2c/capture.jsonl` 不存在，`run_scenario.ps1` 直接
`PREREQ_RED`（rc=2）并打印缺失路径；**不会**把 Rust 判据降级成 `--no-j5` 后放行。

### J5 的两种通过状态

- `GREEN`：同序列 `diff=0` ⇒ 上一轮 50/51 的 `param` 差异是**输入漂移**（侧间没 restore）。
- `KNOWN_PENDING`：恰好是已登记的 4 条（50/51 的 `param`+`raw_hex`）且 3 条 52/D-9 白名单全部命中 ⇒
  **整体仍判 RED（待查）**（runbook §3.3 明令「不得写成 GREEN」）。任何其它差异、白名单缺 `reason/reviewer`、
  或次序分叉 ⇒ RED。

## 3. 退出码

| 工具 | 0 | 1 | 2 |
| --- | --- | --- | --- |
| `preflight.py` | 全绿 | RED（端口在听/清单与源不符/克隆漂移） | PREREQ_RED（缺 git/mysql/provenance） |
| `judge_login.py` | GREEN | RED（含 J5 `KNOWN_PENDING`） | PREREQ_RED（缺 capture/导出未做/golden 缺失） |
| `run_scenario.ps1 -Selftest` | 全绿 | 任一断言红 | — |
| `run_scenario.ps1 -Phase ... -Go` | 全绿 | RED（命令失败/判据不成立） | PREREQ_RED（J5 缺本轮 C# golden；`-E1b` 缺 §C5 快照） |

末行统一 `RESULT: GREEN|RED|PREREQ_RED`；证据 JSON 写 `--out`。

## 4. 离线自检（本车道实际跑过）

```text
python -I -B preflight.py --selftest            => SELFTEST GREEN (25 assertions), rc=0
python -I -B judge_login.py --selftest          => upstream 28 + 35 assertions, rc=0
powershell -File run_scenario.ps1 -Selftest     => SELFTEST GREEN (116 assertions), rc=0
```

另：`run_scenario.ps1 -Scenario G1 -Side rust -Phase paircheck -Go`（复用两个 `run_*_pair.ps1` 的
`-SelfCheckNoStack`）实跑 rc=0（两段均 `SELFTEST_RED GREEN`；离线，约 5–10 分钟）。

`judge_login.py --selftest` 还先跑上游 `check_client_s2c_frames.py --selftest`（28 条，SELFTEST GREEN），
证明复用的布局表/判据未被本包改坏。

### 改坏必红（本车道实际执行的变异 + 逐字节还原）

基线 sha256（本车道提交时；每个变异后 `cp` 还原并用 `sha256sum` 逐字节复核，全部 OK）：

```text
5802070430c3ba22905544006cba528c93a10b234e83fbc532e469d67ccfe1d3  preflight.py
8731543d49144f4379fba4ca07bfe4b17e0389ff74102174502c8868c4932ffb  judge_login.py
e63c09d0137fc6b19cc5648ea92f9450f1306244c672a0fb6c5600716e9a8be2  run_scenario.ps1
```

| # | 工具 | 变异（生产路径被改坏） | 指定断言变红 |
| --- | --- | --- | --- |
| M1 | preflight | `SHADOW_PORT = 7600` → `7601` | `7600 在听 => RED` |
| M2 | preflight | 克隆行数比较 `elif n != nt:` → `elif False and n != nt:` | `克隆行数漂移 => RED` |
| M3 | preflight | `elif src != head:` → `elif False and src != head:` | `source_head 不符 => RED` |
| M4 | judge_login | `if len(i657) != 2:` → `if False and ...` | `删一条 657 => J3 红` |
| M5 | judge_login | `if len(i42) != 2:` → `if False and len(i42) != 2:` | `删第二 42 => J3 红` |
| M6 | judge_login | `keys == KNOWN` / `diff_count == 4` → `KNOWN.issubset(keys)` / `>= 4` | `J5 多一条差异 => RED` |
| M7 | run_scenario | SQL 守卫生产库名正则 `mir2_db` → `mir2_dbX`（测试改为断言错误文本含「生产库名」） | `守卫拒绝生产库名` |
| M8 | run_scenario | `if ($s -notmatch '...mir2_test_...')` → `if ($false)` | `守卫拒绝无测试库目标` |
| M9 | run_scenario | `Compare-G5KeyValues` MaxHP 比较 → `if ($false)` | `G5 MaxHP 漂移 => RED` |
| M10 | run_scenario | `Invoke-Step` 的 `if (-not $Go) { return $false }` → `if ($false) ...` | `未加 -Go 时不执行任何命令` |

每个变异都走「`cp` 备份 → 改坏 → `--selftest` 期望 rc=1 且点名断言 → `cp` 还原 → `sha256` 逐字节相同」。
M1–M10 为 2026-10-23 场景包提交时的历史结果（10/10 命中指定断言，10/10 还原逐字节相同）；
2026-10-21 的 reviewer 修复（Critical 1/2、Important 1–4）只重跑了三份自检与相关 dry-run，
未重跑变异套件——上表 sha256 已按修复后代码重算。

## 5. 未做 / 已知边界

- **没有真机窗口证据**：没起栈、没占端口、没连库、没抓包。`-Go` 路径的真机行为以 runbook 的实测为准。
- **G3a 的 `s2c_diff` 需要两侧 capture**：只跑一侧时 `judge` 会打 `SKIP`（不静默 GREEN）。
- **G5a 的 E1-a 口径**：默认只做行数/关键值断言；`-E1b` 才调 `check_db_paths -Mode diff -Canonical`。
  `-E1b` 是**结构性前置**：`check_db_paths -Mode diff` 只读
  `<dir>/{softclose,disconnect,shutdown}-after.jsonl`，因此必须先在两侧各采齐三条 after 快照
  （run sheet §C5 的六条 `-Mode snap` 命令）；缺任一件 ⇒ `run_scenario.ps1` 直接 `PREREQ_RED`（rc=2）
  并列出缺失文件，不会静默通过，也不会被误记成字段级 RED。
- **`LASTUPDATE` 未推进不是 RED**：它说明本轮 shutdown 没观察到落库（M1 曾记录两侧都没落库），
  证据里记 `G5_WRITE_OBSERVED=false`，由窗口执行人按 runbook §D.4 判卡片。
- **G3b/G3c、G4、G6、G7 不在本包**（run sheet §D.5 / §B7）。
- **E2 影子内容**不在本包：非 TestServer/行会/特殊怪另需 owner 签名（runbook §4.2）。
- `run_scenario.ps1` 的 C# 起栈要求 `<WIN>/csharp/{pack,repo}` 已按 runbook §1.2 备好；
  它不替你复制 `E:/MirServer`（避免误写冻结源）。
