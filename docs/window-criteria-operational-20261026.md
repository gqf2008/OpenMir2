# 真机窗口 8 条判据操作单（2026-10-26）

> 卡：`openmir2-window-criteria-operational`（tier **L**，纯文档/离线，不占窗口）。
> 判据基线 = `openmir2-mirflow-verdict-cause-split` 的 comment `b0c9b9c0`（Leader 采纳的 8 条原文）。
> 本单**只落命令，不改口径**：把每条落成「① 命令 → ② 看哪个字段 → ③ 通过阈值 → ④ 不符时的 kind/rc」。
> 基线本身落不成命令的地方**不自行放宽**，逐条列在 §8 交 Leader 裁决；受影响的判据在正文里标「部分机判」。
>
> **不在本轮**：pickup（`96ad82a37` 裁决结构性不可达；G3 只把该缺项**显式记账**，不跑 pickup 阶段）。

---

## 0. 先读这六条

### 0.1 rc 契约（**只对 `mir_flow.ps1` 与 `provision --gate` 成立**）

`mir_flow.ps1` 的退出码（`tools/client/mir_flow.ps1:205-220`，`Get-VerdictRc` `:988`，`Stop-Usage` `:1103`）：

| rc | 含义 | kind |
|---:|---|---|
| 0 | PASS / 全 GREEN | `ok` |
| 1 | 其它 RED（旧口径） | `action_missing` / `no_dialogue` / `no_success` / `no_gold_change` / `aim_mismatch` / `wrong_cell` / `no_position` / `kind_mismatch` … |
| 2 | **USAGE**（参数名/取值/输入路径错，脚本没按意图跑） | 不落任何 verdict |
| 6 | **UNDECIDABLE** | `missing_prereq`（点名 `missing=<项>`） |
| 7 | RED | `spawn_failed` |
| 8 | RED | `out_of_range` |

多类同跑的聚合优先级（`Get-RcAggregate` `:1092`）：`out_of_range(8) > spawn_failed(7) > 其它 RED(1) > missing_prereq(6) > PASS(0)`。

**不同工具的 rc 语义不同，禁止串读**（这是本单最容易踩的坑）：

| 工具 | rc 语义 |
|---|---|
| `provision_real_machine_fixtures.py` | `0`=OK/PASS；`2`=**PREREQ/输入错**（不是 USAGE：角色命中行数≠1、落点不可走、`--char-x` 单给等）；`3`=库名被拒；`4`=MISMATCH；`5`=ROLLED_BACK；`6`=UNDECIDABLE(`missing_prereq`) |
| `verify_chunks.ps1` | `0`=GREEN；`1`=RED（指纹被证伪）；`2`=USAGE；**`3`=ERROR**（校验没跑完，超出 0/1/2/6/7/8 契约） |
| `dbsnap_pair.py diff` | `0`=GREEN；`1`=RED；`2`=输入/用法错 |
| `port-status.ps1` | 只读巡检，rc 不承载判据（按输出文本判） |

### 0.2 三属性图例

每条命令下都标：**需服务端**（Rust 栈是否在跑）/ **读 DB**（是否连 `mir2_test_db_r`）/ **写入**（写到哪里）。

### 0.3 变量（一次设好；后续命令原样复制）

```powershell
# 窗口实际跑的检出/worktree（不要用别的车道的旧二进制）
$REPO = "E:/Users/gxh/Documents/GitHub/OpenMir2"
$ROOT = "E:/tmp/smoke/rust-root"          # 六角色部署根（含 DBSrv/LoginSrv/M2GameSvr/... 六份 conf）
$PORTS = "E:/tmp/smoke/port-profile.json" # 计划 §2 第 3 步的端口剖面（注意 §8-P2）
$W     = "E:/tmp/window-20261026"
$LOGS  = "$W/logs"                        # 传给 start_mir2_server.ps1 -LogDir，也给 mir_flow -LogDir
$DB    = "mir2_test_db_r"; $CHAR = "aaa"
$GATE_A = "$W/gate-A.json"; $GATE_B = "$W/gate-B.json"
$gateACmd = "python -I -B tools/client/provision_real_machine_fixtures.py --gate --db mir2_test_db_r --char aaa --map 3 --npc-script 盟重土城/药店 --need gold --need sellable_item --need npc --min-gold 500"
$gateBCmd = "python -I -B tools/client/provision_real_machine_fixtures.py --gate --db mir2_test_db_r --char aaa --map 0 --profile tools/window/profiles/butch_monsters.json --need monsters"
# 每次 mir_flow 一条：独立的 proxy 输出目录与 hook 目录（避免串场）
$PROXY_A1 = "$W/proxy-A1"; $HOOK_A1 = "$W/hook-A1"
$PROXY_A2 = "$W/proxy-A2"; $HOOK_A2 = "$W/hook-A2"
$PROXY_A3 = "$W/proxy-A3"; $HOOK_A3 = "$W/hook-A3"
$PROXY_A4 = "$W/proxy-A4"; $HOOK_A4 = "$W/hook-A4"
New-Item -ItemType Directory -Force -Path $W,$LOGS,$HOOK_A1,$HOOK_A2,$HOOK_A3,$HOOK_A4 | Out-Null
# 像素只给「旧投影起点」，不是标定值：见 §5 与 §8-P10
$TradeNpcPixel   = "560,336"   # 阶段 A：char(360,334) -> 药店(361,335)，旧投影 (512+48,304+32)
$TradeBuyMenuPixel  = "600,300" # 设计示例值；必须按当次「买」菜单重标
$TradeSellMenuPixel = "600,300" # 设计示例值；卖菜单与买菜单不同，必须重标
$NpcDialogPixel  = "560,336"
$CombatTargetPixel = "464,272" # 阶段 B：char(330,330) -> 靶(329,329)，旧投影 (512-48,304-32)
```

### 0.4 本单核对过的入口参数（没读到的不写进命令）

| 入口 | 实际参数（读自 `--help` / param 块 / 源码） |
|---|---|
| `tools/client/mir_flow.ps1` | `-OutDir -RunDir -ProxyDir -CapturePath -EvidenceSinceMs -ExpectPos -LogDir -RequireLogEvidence -InjectDll -InjectExe -HookOut -WalkClicks -SkipTalk -SkipLogout -SkipReenter -LeaveClientRunning -StageEvidenceCheck -PickupStage -PickupPixel -NpcDialogStage -NpcDialogPixel -TradeStage -TradeNpcPixel -TradeMenuPixel -TradeKind(buy\|sell) -CombatStage -Profile -PlayerMap -AttackRangeCells -CombatTargetPixel -ActionWaitSec -TradeDialogMaxAgeSec -PrereqGateJson -PrereqGateCmd -EvidenceOut`（param 块 `:28-200`；`-StageEvidenceCheck` 离线分支 `:1980/:2013`；live 四阶段 `:2818/:2833/:2849/:2870/:2932`） |
| `tools/client/provision_real_machine_fixtures.py` | `--check --apply --rollback REPORT --verify --a2-evidence --gate --selftest`；`--db --char --map --char-x --char-y --npc-script --gold --game-point --profile --need {gold,sellable_item,npc,floor_item,monsters} --min-gold --report --yes-apply --yes --frames --expect-gold --expect-rows`（`argparse` `:880-929`；`--apply` 默认 dry-run，真写要 `--yes-apply`） |
| `tools/capture/clienthook/verify_chunks.ps1` | `-CaptureDir <dir> [-Quiet]`；四态 rc `0/1/2/3`（头部注释 + `:173-215`）。`-Quiet` 已实现为**真静默**（只打摘要，逐事件明细不出屏；退出码不变） |
| `tools/db-parity/dbsnap_pair.py` | `diff A B`；`--tables --scope --require-tables --key-mode --time-mode --null-token --ignore-columns --allow-unknown-table --baseline-a --baseline-b --masked-columns --jsonl --summary-out --quiet`；另有 `check-spec` / `selftest`（`build_parser` `:1720-1742`） |
| `E:/MirServer/port-status.ps1` | `-Out <file>`；第 3 节输出 `- <进程> pid=… port=…`，缺失时打 `- **缺失端口**: …`，全在时打 `- 十个端口全部在监听`（脚本源码；本机实测） |

### 0.5 执行顺序总表（**只读 → 有写**）

| # | 动作 | 判据 | 需服务端 | 读 DB | 写入 |
|---:|---|---|---|---|---|
| 1 | `port-status.ps1` 存开窗前基线 | 第 1 步前置 | 否 | 否 | 临时文件 |
| 2 | `start_mir2_server.ps1 -WhatIf`（核 exe/sha/build） | G2 | 否 | 否 | 临时文件 |
| 3 | 停六个 C# 角色；`port-status.ps1` 核九端口 DOWN / 3306 UP | 第 2–3 步前置 | 否 | 否 | 临时文件 |
| 3b | `provision --gate` A **apply 前基线**（只读；期望 rc=6 点名 `gold`） | G3 **负例** | 否 | 是 | 临时文件 |
| 4 | `provision --apply` 阶段 A（置金币/定位到药店旁） | G3 前置 | **否（必须停栈）** | 是 | **写测试库** |
| 5 | `provision --gate` A / B / floor_item，存 JSON | **G3** | 否 | 是 | 临时文件 |
| 6 | 起 Rust 栈（`start_mir2_server.ps1`） | 第 3 步 | 是 | 否 | 进程 |
| 7 | 读 `mir2-server.pid.json` 核 `roles[].exe`/mtime；`build.ps1` + 记 DLL sha | **G2** | 是（pid 部分） | 否 | 临时文件 |
| 8 | 起 `mir2_proxy`（见 §8-P2）→ 跑 `mir_flow` 阶段 A（买/卖/NPC） | **A1/A2/A3** | 是 | 否 | client dump/截图 |
| 9 | `verify_chunks.ps1` 逐 run 校验 hook 产物 | **F1** | 否 | 否 | 无 |
| 10 | `grep 角色数据保存中` | **G1** | 是（读日志） | 否 | 无 |
| 11 | 停 Rust 栈 → `provision --apply` 阶段 B（定位到靶子旁）→ `--gate` B → 再起 Rust | G3 前置 | 否（apply 时） | 是 | **写测试库** |
| 12 | 跑 `mir_flow` 阶段 B（战斗）→ `verify_chunks` → G1 | **A4/F1/G1** | 是 | 否 | client dump/截图 |
| 13 | 停 Rust 栈 → `provision --rollback` **B 再 A** | 收尾 | 否 | 是 | **写测试库** |
| 14 | 恢复 C# 栈 → `port-status.ps1` 核十端口回原样 | 第 7 步 | 否 | 否 | 临时文件 |

> 为什么 `--apply` 夹在「停 C#」与「起 Rust」之间：见 §1.2。

### 0.6 两条通用判读规则

1. **只认工具打印的结构化行**：`STAGE_EVIDENCE category=… verdict=… kind=… rc=… missing=…`、`PREREQ_GATE …`、`CHUNKS_VERIFY …`、gate JSON 的 `verdict/missing`。不要扫 body 里出现过的 `PASS` 字样。
2. **任一动作非 GREEN 会立刻中止本次 `mir_flow`**（`Stop-StageEvidence` `:1109` 直接 `exit <rc>`），所以四类动作**分次跑**；每次只开一类阶段开关，失败才能归因到该类。

---

## 1. 与七步流程逐条对齐

### 1.1 对齐表

`docs/real-machine-window-plan-20261025.md` §2 的七步 → 8 条判据落点：

| 七步 | 本单落点 |
|---|---|
| 1 开窗前存基线（`port-status.ps1`） | 执行顺序 #1；同时核 `3306` 在 |
| 2 只停六个 C# 角色 | #3；**#4 的 apply 必须在其后** |
| 3 确认九端口 DOWN、3306 UP → 起 Rust | #3 核端口；#6 起栈；**G2 的 pid 部分在 #7** |
| 4 重编并注入 hook | #7 的 `build.ps1` + sha；**注入由 #8 的 `mir_flow -InjectDll/-HookOut` 完成**（脚本 `:2320-2335` 等 `hook-ready.flag`，打 `HOOK_READY`） |
| 5 跑 `mir_flow` → `verify_chunks` | #8/#12 跑动作；#9/#12 跑 F1；**G1 在 #10**；A1/A2/A3 在阶段 A（#8），A4 在阶段 B（#12） |
| 6 存 `aaa` before/after、`report.json`、日志、截图 | 本窗口的等价件是 `provision --apply` 报告（`before/after`）+ `flow-stages-evidence.json` + `stages.ndjson` + `mir_flow` 截图 + `verify_chunks` 输出；`report.json` 是 `card_window` 的产物，本流程不产（§8-P12） |
| 7 恢复：停 Rust → `start-all.ps1` → `port-status.ps1` | #13 先 **rollback（写库）** 再 #14 恢复 C#；顺序不能反（见 §1.2） |

**G1/G2/G3 与七步的对应一句话**：
G2 横跨「第 2 步之后（`-WhatIf`）/ 第 3 步之后（pid）/ 第 4 步之后（hook sha）」；
G3 在「第 2 步之后、第 3 步之前」（apply 之后、起栈之前）；
G1 在「第 3 步之后（登录后）/ 第 5 步之后（整窗日志复核）」。

### 1.2 `--apply` 在 G1 之前——核实结论

前一位设计者说「`--apply` 之前必须先跑 G1 存档在途门」。**按实际工具行为，这个顺序是反的。** 理由是两条互相独立的硬事实：

1. **`--apply` 必须发生在没有服务端持有该角色内存态的时候**：它在 `mir2_test_db_r.characters` 上直接 `UPDATE Gold/GamePoint/MapName/CX/CY`（`apply_plan` `:486`）。若 C# 或 Rust 栈正在运行且该角色已登录，服务端内存里的旧值会在登出/断线/关服时写回（Rust `world.disconnect` / `GateEvent::Closed` 存档臂），把夹具覆盖掉；而 apply 的乐观锁谓词只有 `Id+ChrName+Gold+MapName`（`:493-498`），**无法识别「写后又被服务端覆盖」**——`COMMIT` 还早于写后核对（文档已登记：rc=4 `MISMATCH` 时库其实已经改了）。也就是说，运行中 apply 的失败与「被覆盖」不可分。
2. **G1 读的是 Rust 侧 `game-svr.out.log` 的「角色数据保存中」**（Rust 生产点 `mir2-rs/bins/game-svr/src/lib.rs:23134/23282`），这个日志只可能在 Rust 栈起来、角色登录之后存在。它不可能排在 apply 之前。

**正确顺序**（也是本单 §0.5 采用的）：

```
停 C# 六角色 → --apply（夹具写入）→ --gate（读库确认夹具）→ 起 Rust 栈
   → 登录后 G1（grep=0）→ 动作阶段 → verify_chunks → 停机 → --rollback → 恢复 C#
```

**两条都成立的那句话**：`--apply` 之前要确认的是「栈已停、没有人会覆盖」，不是「G1 已过」；G1 是**登录后、动作前**的门。前一版把「栈已停」误写成了「G1 已过」。
另外，阶段 A/阶段 B 的两次 `--apply` 都必须在 **Rust 停栈期间**做（计划 §1 已要求两次 apply）；收尾的 `--rollback` 同理，必须在停 Rust 之后、恢复 C# 之前，否则回滚值会再次被服务端存档覆盖。

### 1.3 夹具写入与回滚（G3 的前置，不在 8 条判据内）

```powershell
# 阶段 A：停 C# 之后、起 Rust 之前
python -I -B "$REPO/tools/client/provision_real_machine_fixtures.py" --apply `
  --db $DB --char $CHAR --gold 5000 --game-point 10 --map 3 `
  --npc-script "盟重土城/药店" --yes-apply --report "$W/fixtures-A.json"
"APPLY_A rc=$LASTEXITCODE"

# 阶段 B：停 Rust 之后、再起 Rust 之前（落点已用 --check 验过可走：(330,330)）
python -I -B "$REPO/tools/client/provision_real_machine_fixtures.py" --apply `
  --db $DB --char $CHAR --gold 5000 --game-point 10 --map 0 `
  --char-x 330 --char-y 330 --npc-script "比奇城/夏家" --yes-apply --report "$W/fixtures-B.json"
"APPLY_B rc=$LASTEXITCODE"

# 收尾：停 Rust 之后、恢复 C# 之前；**先 B 后 A**（B.before = A 之后的状态，A.before = 原始状态）
python -I -B "$REPO/tools/client/provision_real_machine_fixtures.py" --rollback "$W/fixtures-B.json" --yes
"ROLLBACK_B rc=$LASTEXITCODE"
python -I -B "$REPO/tools/client/provision_real_machine_fixtures.py" --rollback "$W/fixtures-A.json" --yes
"ROLLBACK_A rc=$LASTEXITCODE"
```

- **看哪个字段**：报告 JSON 的 `state` / `before` / `after` / `rollback`；回滚报告的 `state`/`want`/`got`。
- **通过阈值**：apply `rc=0` 且 `state="applied"` 且 `after` 的 `Gold=5000`、`GamePoint=10`、`MapName`/`CX`/`CY` 等于计划值（A：`360,334`；B：`330,330`）。回滚 `rc=0` 且 `state="restored"` 且 `want==got`。
- **不符时**：`rc=4 state=MISMATCH` ⇒ **库已经被改过**（`COMMIT` 早于写后核对），必须**手动 `--rollback <report>`**，不许当成已回滚；`rc=5` ⇒ 已回滚，停窗；`rc=2` ⇒ 输入/前置错；`rc=3` ⇒ 库名被拒（红线）。这些都不是 8 条判据的 kind。
- **三属性**：需服务端=**否（且必须停栈）**；读 DB=是（先 SELECT 证现状）；写入=**写 `mir2_test_db_r`**。
- **注意（夹具残留）**：apply 的回滚只恢复 `characters` 的 `Gold/GamePoint/MapName/CX/CY`（以及用 `--bag-std-index` 显式插入的那一件）；**A1 买进来的物品不会被回滚**。所以 A1/A2 必须尽量对同一件物品净额为零；否则把多出的背包物品登记成残留物（§9）。

---

## 2. 门（G1 / G2 / G3）

> 基线原则（`b0c9b9c0`）：**1–3 是「能不能测」——不过 ⇒ UNDECIDABLE + 点名缺项，不记 FAIL。**
> 其中 G3 有工具产出的 rc/kind；G1/G2 没有工具产出 verdict，它们的失败是**环境门失败**，按 UNDECIDABLE 记账并停窗，**不得记 RED**。

### G1 存档在途门

- **基线原文**：两侧 `角色数据保存中` = 0 且 `save_settle.changes ≥ 1`。
- **① 命令**（本窗口只有 Rust 侧；`save_settle` 半条见 §8-P9）：

```powershell
$svrLog = Join-Path $LOGS "game-svr.out.log"
$txt = if (Test-Path -LiteralPath $svrLog) { Get-Content -LiteralPath $svrLog -Raw -Encoding UTF8 } else { $null }
$n = if ($null -ne $txt) { ([regex]::Matches($txt, '角色数据保存中')).Count } else { -1 }
"G1 save_in_flight=$n log=$svrLog" | Tee-Object "$W/g1.txt"
```

- **② 看哪个字段**：`G1 save_in_flight=<n>`；日志文件必须存在（`n=-1` 说明 `-LogDir` 与起栈时不一致，先修 G2/路径）。
- **③ 通过阈值**：`n -eq 0`。可选定位（不改变判据）：用 `stages.ndjson` 里 `ingame` 的 `t_wall_ms` 对齐日志时间，确认动作窗口内也没有该行。

  **若必须在动作前判**：先跑一次「只登录、不开四类阶段开关」的 `mir_flow`（`-SkipTalk -WalkClicks 0 -SkipLogout -LeaveClientRunning`），检查 `n=0` 后再跑正式动作 run；代价是多一次客户端启动。本单默认用**整窗日志 grep**（与基线 `grep -c` 同口径、保守）。
- **④ 不符时**：`n -gt 0` ⇒ **UNDECIDABLE**（环境伪差异，不记 FAIL）；记账码用 `6`（沿用「判不了」桶），**但注意没有工具产出这个 rc**。等 `InSaveRcdList` 清空后重跑；不得因为「walk 没生效」去改场景/判据（T1 的教训）。`n=-1` ⇒ 先修 `-LogDir` 路径，不是判据失败。
- **三属性**：需服务端=**是**（读 Rust 日志）；读 DB=否；写入=临时文件。
- **落点**：第 3 步之后（登录后）；正式判读在整次 `mir_flow` 结束后对同一份 `game-svr.out.log` 复核（整窗 `grep -c=0`，与基线一致）。

### G2 构建 / 注入门

- **基线原文**：`mir2-server.pid.json` 的 `roles[].exe` 为本检出产物且 mtime ≥ 当前 head；**hook DLL 的 sha256 与本次构建一致并记入证据**。
- **① 命令**（三段，全部只读）：

```powershell
# (a) 起栈前：-WhatIf 只打印计划，不建进程/不建 LogDir（脚本 :424-443）
powershell -NoProfile -ExecutionPolicy Bypass -File "$REPO/tools/m2-acceptance/start_mir2_server.ps1" `
  -Root $ROOT -LogDir $LOGS -PortsProfile $PORTS -WhatIf | Tee-Object "$W/g2-whatif.txt"

# (b) 起栈后：pid 里的 roles[].exe 必须唯一、是本检出 release、mtime >= head 提交时刻
$pj    = Get-Content -LiteralPath (Join-Path $LOGS "mir2-server.pid.json") -Raw | ConvertFrom-Json
$head  = [int](git -C $REPO log -1 --format=%ct)
$headSha = (git -C $REPO rev-parse HEAD).Trim()
$exes  = @($pj.roles | ForEach-Object { [string]$_.exe } | Sort-Object -Unique)
$exe0  = if ($exes.Count -gt 0) { $exes[0] } else { "" }
$exeNorm = ($exe0 -replace '\\','/')
$mtime = if ($exe0 -and (Test-Path -LiteralPath $exe0)) {
  [int]([DateTimeOffset](Get-Item -LiteralPath $exe0).LastWriteTimeUtc).ToUnixTimeSeconds()
} else { 0 }
$pass  = ($pj.roles.Count -eq 6) -and ($exes.Count -eq 1) -and
         ($exeNorm -eq "$REPO/mir2-rs/target/release/mir2-server.exe") -and ($mtime -ge $head)
[ordered]@{ head=$headSha; head_commit_ts=$head; roles=$pj.roles.Count; exes=$exes;
            exe_mtime=$mtime; pass=$pass } | ConvertTo-Json | Tee-Object "$W/g2-pid.json"

# (c) hook DLL sha：构建后记一次，注入后（mir_flow 打出 HOOK_READY 之后）再记一次，必须相同
function Get-FileSha256([string]$p) {
  $s = [Security.Cryptography.SHA256]::Create(); $f = [IO.File]::OpenRead($p)
  try { ([BitConverter]::ToString($s.ComputeHash($f)) -replace '-','').ToLowerInvariant() }
  finally { $f.Dispose(); $s.Dispose() }
}
$dll = Join-Path $REPO "tools/capture/clienthook/bin/Mir2ClientHook.dll"
$pre = Get-FileSha256 $dll
"pre=$pre" | Set-Content -LiteralPath "$W/hook-dll-sha256.txt" -Encoding ascii
# 跑完 mir_flow（见 A1/A2/A3/A4）后：
$post = Get-FileSha256 $dll
"post=$post" | Add-Content -LiteralPath "$W/hook-dll-sha256.txt"
$hookOk = ($pre -eq $post) -and ($pre -match '^[0-9a-f]{64}$') -and
          (Test-Path -LiteralPath (Join-Path $HOOK_A1 "hook-ready.flag"))
"G2 hook_sha_pass=$hookOk pre=$pre post=$post"
```

- **② 看哪个字段**：
  - (a) `SERVER_EXE <path>`、`SERVER_EXE_BUILD Release`、`SERVER_EXE_SHA256 <64 hex>`；
  - (b) `g2-pid.json` 的 `roles=6` / `exes` 唯一 / `pass=true`；
  - (c) `G2 hook_sha_pass=True`（pre==post 且 64 hex），以及 `mir_flow` 日志里的 `HOOK_READY`。
- **③ 通过阈值**：三段的 `pass` 全真；`SERVER_EXE_BUILD` 必须 `Release`（`Debug-Fallback` 直接停窗）。
- **④ 不符时**：**环境门失败 ⇒ 停窗，记 UNDECIDABLE**；没有工具产出 kind/rc。路径对但 mtime 早于 head ⇒ 先 `cargo build --release -p mir2-server` 再起栈；`pre!=post` ⇒ DLL 在构建与注入之间被换过，重编重注入；`Debug-Fallback` ⇒ 不得用 debug 证据。
- **三属性**：(a)/(c) 不需要服务端；(b) 需要服务端刚起完；均不读 DB；写临时文件。
- **落点**：(a) 第 3 步之前；(b) 第 3 步之后；(c) 第 4 步之后 / 第 5 步的 `mir_flow` 打出 `HOOK_READY` 之后。
- **注意（实测坑）**：`start_mir2_server.ps1` 在**显式给 `-ServerExe`** 时把 `SERVER_EXE_BUILD` 标成 `Explicit`，此时再加 `-RequireRelease` 会因为 `BuildType -ne 'Release'` 直接 RED（`:407-410`）。所以 (a) 不要同时给 `-ServerExe` 与 `-RequireRelease`；要钉死本检出产物就用默认 release 解析。

### G3 前提门（金币 / 可卖物品 / NPC / 地板物 / 怪物声明）

- **基线原文**：金币 ≥ 阈值 / 可卖物品在约定槽（DB 查）/ 地板可捡物在约定坐标 / NPC 能发 643 或 767。**地板物一项结构性不可达 ⇒ 显式记 `missing_prereq(structural)`，不是失败。**
- **① 命令**（`--gate` 默认**全查含 `floor_item`**，会恒 rc=6；必须逐项 `--need`）：

```powershell
# 统一的捕获方式：只取 stdout；stderr 留在屏上（失败时是 PREREQ/REFUSED）
$prevOutEnc = [Console]::OutputEncoding
[Console]::OutputEncoding = New-Object Text.UTF8Encoding($false)
$env:PYTHONIOENCODING = "utf-8"

# 阶段 A：买卖/NPC（map 3 药店）
$outA = & python -I -B "$REPO/tools/client/provision_real_machine_fixtures.py" --gate `
  --db $DB --char $CHAR --map 3 --npc-script "盟重土城/药店" `
  --need gold --need sellable_item --need npc --min-gold 500
$rcA = $LASTEXITCODE
if ($outA) { [IO.File]::WriteAllText($GATE_A, (($outA -join "`n") + "`n"), (New-Object Text.UTF8Encoding($false))) }
"G3_A rc=$rcA json=$GATE_A"

# 阶段 B：战斗（map 0，profile 的 fixtures.monsters）
$outB = & python -I -B "$REPO/tools/client/provision_real_machine_fixtures.py" --gate `
  --db $DB --char $CHAR --map 0 --profile "$REPO/tools/window/profiles/butch_monsters.json" `
  --need monsters
$rcB = $LASTEXITCODE
if ($outB) { [IO.File]::WriteAllText($GATE_B, (($outB -join "`n") + "`n"), (New-Object Text.UTF8Encoding($false))) }
"G3_B rc=$rcB json=$GATE_B"

# 地板物：结构性缺项的**显式记账**（期望 rc=6；这是记录，不是通过门；不要传给 mir_flow）
$outF = & python -I -B "$REPO/tools/client/provision_real_machine_fixtures.py" --gate `
  --db $DB --char $CHAR --map 3 --npc-script "盟重土城/药店" --need floor_item
$rcF = $LASTEXITCODE
if ($outF) { [IO.File]::WriteAllText("$W/gate-floor.json", (($outF -join "`n") + "`n"), (New-Object Text.UTF8Encoding($false))) }
"G3_floor rc=$rcF json=$W/gate-floor.json"

# 恢复控制台编码（这一段只影响 PowerShell 如何解码子进程 stdout）
[Console]::OutputEncoding = $prevOutEnc
Remove-Item Env:PYTHONIOENCODING -ErrorAction SilentlyContinue
```

**apply 之前的负例**（只读；必须在 §1.3 的 apply 之前跑；**不要写 `$GATE_A`**，只记账）：

```powershell
$pre = & python -I -B "$REPO/tools/client/provision_real_machine_fixtures.py" --gate `
  --db $DB --char $CHAR --map 3 --npc-script "盟重土城/药店" `
  --need gold --need sellable_item --need npc --min-gold 500
$preTxt = ($pre -join "`n")
"G3_A_PREFIX rc=$LASTEXITCODE has_gold=$($preTxt -match '"gold"') has_undecidable=$($preTxt -match 'UNDECIDABLE')"
```

期望 `rc=6`、`has_gold=True`、`has_undecidable=True`——证明门对「Gold=0」不是恒绿。这一条只做负例记账，不阻断。

- **② 看哪个字段**：gate JSON 的 `verdict` / `kind` / `missing[]` / `reasons{}` / `details{}`（金币值在 `details.gold`，背包真物品数在 `details.bag_items`，NPC 在 `details.npc`，地板物在 `details.floor_item`）。
- **③ 通过阈值**：
  - **负例（apply 之前的基线，只读）**：阶段 A 的 gate 应 `rc=6` 且 `missing` 含 `gold`（角色 Gold=0）——这一步证明门不是恒绿；只记账，不阻断。
  - 阶段 A/B 的**通过门**（apply 之后复跑）：`rc=0` 且 `verdict="PASS"` 且 `missing=[]`。
  - **结构性记账**：`rc=6`、`missing` 含 `floor_item`、`reasons.floor_item` 含「结构性不可达」、`details.floor_item="BLOCKED"` ⇒ **这是预期的绿**（本轮 pickup 不跑）。三条事实一旦变化，工具会转 `REEVALUATE`，届时重评估而不是硬记缺。
- **④ 不符时**：
  - `rc=6` 且 `missing` 非空 ⇒ **UNDECIDABLE + `kind=missing_prereq` + rc 6**，点名缺项。把**同一份 JSON** 传给 `mir_flow -PrereqGateJson`，动作没生效时就会落 `rc=6` 而不是 RED（`Get-FirstMissingPrereq` `:1073`）。
  - `rc∈{2,3,4,5}` ⇒ **不是 gate verdict**：2=输入/前置错（角色行数≠1、落点不可走、单给 `--char-x`），3=库名被拒（红线），4=MISMATCH，5=ROLLED_BACK。停窗修正，不得当 PASS/UNDECIDABLE。
  - 注意 `provision` 的 `2` **不是** `mir_flow` 的 USAGE（§0.1）。
- **三属性**：需服务端=**否**（直连测试库/读 Envir）；读 DB=**是**（只读 `mir2_test_db_r`）；写入=临时 JSON。**必须在 `--apply` 之后跑**，否则金币还是旧的。
- **落点**：第 2 步之后、第 3 步之前（apply 之后、起栈之前）。阶段 B 的 gate 在第二次 apply 之后。
- **基线覆盖度（部分机判）**：
  - 「金币 ≥ 阈值」：机判（`details.gold` + `--min-gold`）。
  - 「NPC 能发 643 或 767」：gate 只判**脚本确有 `[@main]` 说词（能力）**；是否真的发 643 由 A3 判。
  - 「可卖物品在约定槽」：gate 只数 `StdIndex<>0` 的行数（`details.bag_items`），**不核对具体槽位/StdIndex** ⇒ 见 §8-P5。
  - 「地板可捡物在约定坐标」：结构性，按上面的记账处理。
  - 「怪物声明」：gate 只判 profile 里有同图声明；**服务端是否真的刷出不在 gate 里** ⇒ 见 §8-P1（本轮的关键缺口）。

---

## 3. 动作（A1 / A2 / A3 / A4）

### 3.0 四类动作的公共形态

每条动作都是一次**独立的** `mir_flow` 进程（一次只开一类开关，失败才能归因；且非 GREEN 会中止本次后续阶段）：

```powershell
# 公共参数片段（每条命令在此基础上加自己的阶段开关）
#   -OutDir <run 目录> -RunDir "D:\MirClient-run"
#   -ProxyDir <本次 proxy 输出目录> -HookOut <本次 hook 目录>
#   -InjectDll <本检出 hook DLL> -LogDir $LOGS
#   -ExpectPos <阶段 A "360,334" / 阶段 B "330,330">
#   -PrereqGateJson <gate-A 或 gate-B> -PrereqGateCmd "<生成该 JSON 的实际命令>"
#   -WalkClicks 0 -SkipTalk -SkipLogout -SkipReenter -LeaveClientRunning
#   -ActionWaitSec 8
```

参数说明（都是实际读到的）：
- `-ProxyDir`：**live 四类动作强制要求**，且它必须是 `mir2_proxy --out` 目录——脚本在启动客户端前校验 `session.json` + `ready.flag` + `pid` 存活（`Assert-ProxyLive` `:672-681`）。**hook 目录不能当 `-ProxyDir`**（实测：rc=1 + `proxy session.json 不存在` 堆栈，见 §8-P2）。
- `-HookOut` + `-InjectDll`：`mir_flow` 自己注入 hook 并等 `hook-ready.flag`（`:2320-2335`），不需要手工注入。
- `-PrereqGateCmd`：把生成 gate 的命令原样写进证据（`generated_by_source=operator`）；不给则记模板并标 `template`（N3）。
- `-ExpectPos` 的必要性：apply 后角色坐标是确定的（阶段 A 实测 `(360,334)`；阶段 B 实测 `(330,330)`），坐标门不过会 `POSITION_GATE RED` 并中止（`:2602-2615`）。
- `-WalkClicks 0`：默认 2 次走位会把角色带离 NPC/靶子，后续阶段与下一次 run 的 `-ExpectPos` 都会红。
- `-SkipTalk/-SkipLogout/-SkipReenter/-LeaveClientRunning`：本单只判四类动作；保留客户端便于截图，也避免 logout/reenter 改变位置。下一次 run 启动时会先杀掉旧客户端（`:2301`），服务端在断线存档臂落库。
- `-LogDir` 不给 `-RequireLogEvidence`：后者会把「日志里有 `[ERR]/panic`」折成 `rc=1`，与阶段判定混在一起；日志水位另用 G1/单独扫描。

**每次 run 之后都做 F1**（§4），再读 `STAGE_EVIDENCE` 与证据 JSON。

### 3.1 A1 买

- **基线原文**：金币余额**差 = 价格**（DB 前后差）且目标槽位**出现**同一 `StdIndex`。
- **① 命令**：

```powershell
$runA1 = "$W/flow-A1-buy"
$outA1 = & powershell -NoProfile -ExecutionPolicy Bypass -File "$REPO/tools/client/mir_flow.ps1" `
  -OutDir $runA1 -RunDir "D:\MirClient-run" `
  -ProxyDir $PROXY_A1 -HookOut $HOOK_A1 `
  -InjectDll "$REPO/tools/capture/clienthook/bin/Mir2ClientHook.dll" `
  -LogDir $LOGS -ExpectPos "360,334" `
  -PrereqGateJson $GATE_A -PrereqGateCmd $gateACmd `
  -TradeStage -TradeKind buy -TradeNpcPixel $TradeNpcPixel -TradeMenuPixel $TradeBuyMenuPixel `
  -WalkClicks 0 -SkipTalk -SkipLogout -SkipReenter -LeaveClientRunning -ActionWaitSec 8 2>&1
$rcA1 = $LASTEXITCODE
$outA1 | Tee-Object "$W/flow-A1-buy.log"
"FLOW_A1_RC=$rcA1" | Add-Content -LiteralPath "$W/flow-A1-buy.log"
```

- **② 看哪个字段**：
  - stdout：`PREREQ_GATE source=provision_gate_json … sha256=… evaluated_at_wall_ms=…`、`STAGE_EVIDENCE category=trade verdict=GREEN kind=ok rc=0`、`FLOW_OK`；
  - 证据 JSON `<runA1>/flow-stages-evidence.json` 的 `categories.trade`：`verdict` / `verdict_kind` / `kind`（方向）/ `success{ident,ts_ms}`（**`ident` 是 `-TradeKind` 的常量回显；证据面看 `ts_ms`**）/ `gold_delta` / `dialogue` / `item_effect`，以及 `prereq_gate.evaluated_before_actions` / `sha256`。
- **③ 通过阈值**（工具实际口径）：

```powershell
$ev = Get-Content -LiteralPath "$runA1/flow-stages-evidence.json" -Raw -Encoding UTF8 | ConvertFrom-Json
$t  = $ev.categories.trade
# F2（`openmir2-window-criteria-doc-followups`）：不再判 `success.ident` 的等值——该字段是
# `-TradeKind` 选出的**常量回显**（`mir_flow.ps1:1289`），恒真、不构成证据。改用**实测帧时间**
# 与请求时间的先后关系；产物里没有 `success` 对象（伪造/缺帧）⇒ RED。
$passA1 = ($t.verdict -eq 'GREEN') -and ($t.verdict_kind -eq 'ok') -and ($t.kind -eq 'buy') -and
          ($null -ne $t.success) -and ([long]$t.success.ts_ms -gt 0) -and
          ([long]$t.success.ts_ms -ge [long]$t.request.ts_ms) -and ($t.gold_delta -lt 0) -and
          ($t.dialogue.opens -ge 1) -and ($t.dialogue.dialogue_frames -ge 1) -and
          ($ev.prereq_gate.evaluated_before_actions -eq $true) -and
          ($ev.prereq_gate.sha256 -match '^[0-9a-f]{64}$')
"A1 PASS=$passA1 gold_delta=$($t.gold_delta) item=$($t.item_effect.ident)x$($t.item_effect.count)"
```

  另需**单独记账**（工具不过门、基线要求）：`item_effect.ident=200` 且 `count>=1`（对应「目标槽出现物品」）；若 `count=0`，即使工具 GREEN 也**不满足基线**，见 §8-P4。
- **④ 不符时**（`Get-TradeEvidence` `:1226-1325`）：

| 现象 | kind | rc |
|---|---|---:|
| 没有 c2s `1014`，且 gate 点名 `gold/npc` 缺 | `missing_prereq`（`missing=gold` 或 `npc`） | **6** |
| 没有 c2s `1014`，门全过 | `action_missing` | 1 |
| 请求 ident 与 `-TradeKind` 不符 | `kind_mismatch` | 1 |
| 交易前没有商人对话（1010/1011 + 643/767 body>0） | `no_dialogue`（门缺则 6） | 1 / 6 |
| 没有成功帧 `SM_BUYITEM_SUCCESS(650)` | `no_success`（门缺则 6） | 1 / 6 |
| 没有金币帧（52/653） | `no_gold_evidence`（门缺则 6） | 1 / 6 |
| 金币没减少 | `no_gold_change`（门缺则 6） | 1 / 6 |
| 副作用成立 | `ok` | 0 |

- **三属性**：需服务端=**是**；读 DB=否（纯协议证据）；写入=客户端 dump/截图/`flow-stages-evidence.json`（不写库）。
- **落点**：第 5 步的 `mir_flow` 阶段 A。
- **部分机判**：基线「差 = 价格」「同一 `StdIndex`」工具**都不判**（只判方向与成功帧；物品帧只记 count）⇒ §8-P4。另「一次跑两方向、与 A1 共用同一物品」工具不支持（`-TradeKind` 单值）⇒ §8-P3。

### 3.2 A2 卖

- **基线原文**：金币余额**差 = 售价**且该槽**消失**（与 A1 共用同一物品，一次跑两方向）。
- **① 命令**：把 A1 的 `-TradeKind buy` 换成 `-TradeKind sell`、`-TradeMenuPixel $TradeSellMenuPixel`，其余相同（独立 run 目录/proxy/hook）：

```powershell
$runA2 = "$W/flow-A2-sell"
$outA2 = & powershell -NoProfile -ExecutionPolicy Bypass -File "$REPO/tools/client/mir_flow.ps1" `
  -OutDir $runA2 -RunDir "D:\MirClient-run" `
  -ProxyDir $PROXY_A2 -HookOut $HOOK_A2 `
  -InjectDll "$REPO/tools/capture/clienthook/bin/Mir2ClientHook.dll" `
  -LogDir $LOGS -ExpectPos "360,334" `
  -PrereqGateJson $GATE_A -PrereqGateCmd $gateACmd `
  -TradeStage -TradeKind sell -TradeNpcPixel $TradeNpcPixel -TradeMenuPixel $TradeSellMenuPixel `
  -WalkClicks 0 -SkipTalk -SkipLogout -SkipReenter -LeaveClientRunning -ActionWaitSec 8 2>&1
$rcA2 = $LASTEXITCODE
$outA2 | Tee-Object "$W/flow-A2-sell.log"
"FLOW_A2_RC=$rcA2" | Add-Content -LiteralPath "$W/flow-A2-sell.log"
```

- **② 看哪个字段**：同 A1，`categories.trade`（`kind="sell"`）、`success{ident,ts_ms}`（`ident` 是常量回显）、`gold_delta`、`item_effect`。
- **③ 通过阈值**：

```powershell
$ev2 = Get-Content -LiteralPath "$runA2/flow-stages-evidence.json" -Raw -Encoding UTF8 | ConvertFrom-Json
$t2  = $ev2.categories.trade
# F2：同 A1——`success.ident` 是常量回显（这里恒为 648），改判实测 `ts_ms` 与请求的先后。
$passA2 = ($t2.verdict -eq 'GREEN') -and ($t2.verdict_kind -eq 'ok') -and ($t2.kind -eq 'sell') -and
          ($null -ne $t2.success) -and ([long]$t2.success.ts_ms -gt 0) -and
          ([long]$t2.success.ts_ms -ge [long]$t2.request.ts_ms) -and ($t2.gold_delta -gt 0) -and
          ($t2.dialogue.opens -ge 1) -and ($t2.dialogue.dialogue_frames -ge 1) -and
          ($ev2.prereq_gate.evaluated_before_actions -eq $true) -and
          ($ev2.prereq_gate.sha256 -match '^[0-9a-f]{64}$')
"A2 PASS=$passA2 gold_delta=$($t2.gold_delta) item=$($t2.item_effect.ident)x$($t2.item_effect.count)"
```

  **单独记账**（基线要求、工具不判）：`item_effect.ident=202` 且 count>=1 只是「有卖出应答帧」；「该槽消失」需要在 DB/背包侧核，见 §8-P4。
- **④ 不符时**：同 A1 的表（成功帧换成 `SM_USERSELLITEM_OK(648)`；金币方向反过来）。`missing_prereq` 的门项是 `sellable_item` 或 `npc`。
- **三属性 / 落点**：同 A1（阶段 A，第二条独立 run）。
- **部分机判**：售价相等、槽消失、与 A1 共用同一物品 ⇒ §8-P3/P4。

### 3.3 A3 NPC 对话进入与退出

- **基线原文**：对话态**进入与退出**（收 643 → 支线推进 → 退出后不再收）；**不以 525/520 出现为判据**。
- **① 命令**：

```powershell
$runA3 = "$W/flow-A3-npc"
$outA3 = & powershell -NoProfile -ExecutionPolicy Bypass -File "$REPO/tools/client/mir_flow.ps1" `
  -OutDir $runA3 -RunDir "D:\MirClient-run" `
  -ProxyDir $PROXY_A3 -HookOut $HOOK_A3 `
  -InjectDll "$REPO/tools/capture/clienthook/bin/Mir2ClientHook.dll" `
  -LogDir $LOGS -ExpectPos "360,334" `
  -PrereqGateJson $GATE_A -PrereqGateCmd $gateACmd `
  -NpcDialogStage -NpcDialogPixel $NpcDialogPixel `
  -WalkClicks 0 -SkipTalk -SkipLogout -SkipReenter -LeaveClientRunning -ActionWaitSec 8 2>&1
$rcA3 = $LASTEXITCODE
$outA3 | Tee-Object "$W/flow-A3-npc.log"
"FLOW_A3_RC=$rcA3" | Add-Content -LiteralPath "$W/flow-A3-npc.log"
```

- **② 看哪个字段**：`STAGE_EVIDENCE category=npcdialog …`；证据 `categories.npcdialog` 的 `click.ident/ts_ms`、`dialogue.say_frames` / `menu_frames` / `max_body_len` / `first_ts_ms`。
- **③ 通过阈值**（**进入**部分）：

```powershell
$ev3 = Get-Content -LiteralPath "$runA3/flow-stages-evidence.json" -Raw -Encoding UTF8 | ConvertFrom-Json
$d   = $ev3.categories.npcdialog
$passA3 = ($d.verdict -eq 'GREEN') -and ($d.verdict_kind -eq 'ok') -and
          (($d.dialogue.say_frames + $d.dialogue.menu_frames) -ge 1) -and ($d.dialogue.max_body_len -gt 0)
"A3_ENTRY PASS=$passA3 say=$($d.dialogue.say_frames) menu=$($d.dialogue.menu_frames) body=$($d.dialogue.max_body_len)"
```

  该阶段**不看 525/520**（脚本内也没有任何 525/520 判据；只有公告按钮坐标用到 `522,525`）。`SM_MENU_OK(767)` 只当「对话内容帧」（body 非空），不当成功帧。
- **④ 不符时**（`Get-NpcDialogEvidence` `:1178-1224`）：

| 现象 | kind | rc |
|---|---|---:|
| 没有 c2s `CM_CLICKNPC(1010)`，gate 点名 `npc` 缺 | `missing_prereq`（`missing=npc`） | **6** |
| 没有 1010，门全过 | `action_missing` | 1 |
| 点击后没有 body>0 的 643/767 | `no_dialogue`（门缺则 6） | 1 / 6 |
| 进入成立 | `ok` | 0 |

- **三属性**：需服务端=是；读 DB=否；写入=dump/截图/证据 JSON。
- **落点**：第 5 步的 `mir_flow` 阶段 A（独立 run）。
- **部分机判（明确不可机判的部分）**：基线要求「**退出**（退出后不再收）」。工具只判进入，没有退出动作、也没有「不再收」的机判信号（客户端关框是本地行为，服务端无对应 ack）。**退出 = 只有人能判**（或后续另开卡实现）。见 §8-P6。

### 3.4 A4 有目标战斗

- **基线原文**：靶子 **HP 递减或死亡帧**且尸体仍在 AP 范围内；声明夹具却刷不出 ⇒ RED(`spawn_failed`)；刷出但超 AP ⇒ RED(`out_of_range`)。
- **① 命令**：

```powershell
$runA4 = "$W/flow-A4-combat"
$outA4 = & powershell -NoProfile -ExecutionPolicy Bypass -File "$REPO/tools/client/mir_flow.ps1" `
  -OutDir $runA4 -RunDir "D:\MirClient-run" `
  -ProxyDir $PROXY_A4 -HookOut $HOOK_A4 `
  -InjectDll "$REPO/tools/capture/clienthook/bin/Mir2ClientHook.dll" `
  -LogDir $LOGS -ExpectPos "330,330" `
  -PrereqGateJson $GATE_B -PrereqGateCmd $gateBCmd `
  -CombatStage -Profile "$REPO/tools/window/profiles/butch_monsters.json" -PlayerMap "0" `
  -CombatTargetPixel $CombatTargetPixel -AttackRangeCells 1 `
  -WalkClicks 0 -SkipTalk -SkipLogout -SkipReenter -LeaveClientRunning -ActionWaitSec 8 2>&1
$rcA4 = $LASTEXITCODE
$outA4 | Tee-Object "$W/flow-A4-combat.log"
"FLOW_A4_RC=$rcA4" | Add-Content -LiteralPath "$W/flow-A4-combat.log"

# F1（`openmir2-window-criteria-doc-followups`）：A4 的「同图」**不是线上证据**——经典协议没有图名，
# 工具拿 -PlayerMap 与 profile 的 fixtures.monsters[].map 比较，而这个值是**操作者断言**。
# 必须同跑一份来源记录；缺来源 ⇒ 该项 UNDECIDABLE（见下面 ③ 与 §5 / §8-P14）。
$pmaA4 = [ordered]@{
  map          = "0"       # 必须与上面的 -PlayerMap 逐字相同
  source_kind  = "db"      # db | screenshot | other
  source_cmd   = "SELECT MapName FROM mir2_test_db_r.characters WHERE ChrName='aaa'"
  source_value = "0"       # 上面命令/截图**实际读到的值**
  by           = $env:USERNAME
  ts_wall_ms   = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
}
$pmaA4 | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath "$W/player-map-assertion-A4.json" -Encoding UTF8
```

- **② 看哪个字段**：stdout `COMBAT_TARGET map=… x=… y=… own=… dist=…`、`COMBAT_ACTION x=… y=… pixel_source=explicit|projected`、`STAGE_EVIDENCE category=combat …`；证据 `categories.combat` 的 `target{distance}` / `hit{own_x,own_y,dir,front_x,front_y}` / `effect` / `hp_before,hp_after,max_hp,damage` / `actor` / `pixel_source` / `pixel{x,y}`。
- **③ 通过阈值**：

```powershell
$ev4 = Get-Content -LiteralPath "$runA4/flow-stages-evidence.json" -Raw -Encoding UTF8 | ConvertFrom-Json
$c   = $ev4.categories.combat
$passA4 = ($c.verdict -eq 'GREEN') -and ($c.verdict_kind -eq 'ok') -and
          ($c.target.distance -le $c.range_cells) -and
          ($c.effect -in @('hp','death')) -and
          ($c.pixel_source -in @('explicit','projected')) -and ($null -ne $c.pixel.x) -and
          ($c.hit.front_x -eq $c.target.x) -and ($c.hit.front_y -eq $c.target.y)
if ($c.effect -eq 'hp') { $passA4 = $passA4 -and ($c.damage -gt 0) }
# F1：同图前提 = 操作者断言。缺断言文件 / 缺来源字段 / 断言的 map 与证据里的 -PlayerMap 不同
# ⇒ **UNDECIDABLE**（这一项本轮没测：既不是 GREEN，也不是 RED）。
$pmaPath = "$W/player-map-assertion-A4.json"
$pma = if (Test-Path -LiteralPath $pmaPath) { Get-Content -LiteralPath $pmaPath -Raw -Encoding UTF8 | ConvertFrom-Json } else { $null }
$pmaOk = ($null -ne $pma) -and
         (-not [string]::IsNullOrWhiteSpace([string]$pma.map)) -and
         (-not [string]::IsNullOrWhiteSpace([string]$pma.source_cmd)) -and
         (-not [string]::IsNullOrWhiteSpace([string]$pma.source_value)) -and
         ([string]$pma.map -eq [string]$ev4.profile.player_map)
$a4Verdict = if (-not $pmaOk) { 'UNDECIDABLE' } elseif ($passA4) { 'PASS' } else { 'RED' }
"A4 VERDICT=$a4Verdict PASS=$passA4 effect=$($c.effect) dmg=$($c.damage) pixel_source=$($c.pixel_source) dist=$($c.target.distance) player_map=$($ev4.profile.player_map) pma_ok=$pmaOk"
```

  `effect=hp` 时 `hp_before = param + series`、`hp_after = param`，工具已保证 `series>0`；`effect=death` 时必须有非自己的 `SM_DEATH(32)`。
- **④ 不符时**（`Get-CombatEvidence` `:1329-1489` + live 前置 `:2870-2931`）：

| 现象 | kind | rc |
|---|---|---:|
| `-PlayerMap` 未给 / profile 读不出 / `fixtures.monsters` 为空 | `missing_prereq`（`missing=monster_fixture`） | **6** |
| 证据里没有自身坐标（50/51） | `no_position` | 1 |
| 靶子只在别的图；或最近同图靶子距离 > AP | `out_of_range` | **8** |
| 没有 c2s `CM_HIT(3014)`（点击落成走/跑） | `action_missing` | 1 |
| 前方格与靶子不匹配，但最近靶子在 AP 内 | `aim_mismatch` | 1 |
| 已声明、同图、AP 内、前方格就是靶子，但无 `SM_STRUCK(31)`/`SM_DEATH(32)` | `spawn_failed` | **7** |
| 靶子与角色同格 | 无 kind，`throw` 堆栈 | 1 |
| 成立 | `ok` | 0 |

- **三属性**：需服务端=是；读 DB=否；写入=dump/截图/证据 JSON。
- **落点**：第 5 步的 `mir_flow` 阶段 B（第二次起栈后）。
- **部分机判 / 不可机判**：
  - 基线「**尸体仍在 AP 范围内**」：证据里没有尸体坐标，工具不判 ⇒ **只有人能判**（看截图/帧）。
  - `spawn_failed` 的**可达性前提**是客户端先发了 `CM_HIT`。如果靶子根本没刷出/没同屏，客户端点击会落成 `CM_WALK/RUN`，于是先看到 `action_missing rc=1`，而不是 `rc=7`。这正是已知的 attack-vs-walk 根因（`docs/client-flow-stages-20261025.md` §7.2）——**从「没有 3014」的帧里无法区分「没刷出」「点歪」「转向错」**，工具不判 ⇒ 只有人能判（需先录「点击坐标 + 客户端落点」）。见 §8-P7。
  - **「同图」= 操作者断言**：`out_of_range` / `spawn_failed` 的同图判定用 `-PlayerMap`（`mir_flow.ps1:1381/:2899`），
    而它由操作者给（`:182` 注释原文「经典协议线上没有图名；必须显式给」）⇒ 这一半不是机判。
    本单要求同跑 `player-map-assertion-A4.json` 记 `map` + 来源；缺来源 ⇒ 该项 **UNDECIDABLE**。见 §5 / §8-P14。
  - 像素（`-CombatTargetPixel`）：依赖窗口位置/尺寸/DPI，48×32 投影未被证实 ⇒ 只有人能标定，见 §5。

---

## 4. F1 四态指纹

- **基线原文**：真机只取 `ok` 态（`verify_chunks -CaptureDir <本次根>` rc=0 且块数/`data` sha256 非空串）；空串/篡改/类型非法三态离线复验不占窗口；`pixel_source` 在战斗与对话阶段被记账且非空。
- **① 命令**（每次 `mir_flow` 之后，对本次 `-HookOut` 目录）：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File "$REPO/tools/capture/clienthook/verify_chunks.ps1" -CaptureDir $HOOK_A1
$f1rc = $LASTEXITCODE
"CHUNKS_RC=$f1rc" | Tee-Object "$W/f1-A1.txt"
```

- **② 看哪个字段**：stdout 首个 token + 计数行：
  - `CHUNKS_VERIFY GREEN events=<N> bins=<M> bytes=<B> dir=…`
  - `CHUNKS_VERIFY RED bad=<k> events=<N> bins=<M> bytes=<B> …`
- **③ 通过阈值**：`rc=0` **且** `CHUNKS_VERIFY GREEN` **且** `events>=1`、`bins>=1`、`bytes>0`。GREEN 已内含「每个 data 事件的 sha256 非空且按 `.bin` 切片复算一致」；空串/越界/0 条都会是 RED。
- **④ 不符时**（`verify_chunks.ps1` 头部四态）：

| rc | 输出 | 含义 | 归属 |
|---:|---|---|---|
| 0 | `GREEN` | 指纹已证明 | PASS |
| 1 | `RED` | 证据被证伪（空串/不符/越界/0 条/字段类型非法） | **其它 RED（rc=1）**；不是 6/7/8，也不是行为判据；停窗查 hook 产物 |
| 2 | `USAGE` | 用法错，压根没校验（错参数名/缺值/目录不存在/缺 `chunks.ndjson`） | **不落 verdict**；按提示重跑，**这不是 RED** |
| 3 | `ERROR` | 校验没跑完（内部异常） | **不可判**（超出 8 条契约）；修权限/完整性后重跑，不许当 PASS/RED |

- **三属性**：需服务端=否；读 DB=否；**只读，不写任何文件**。
- **落点**：第 5 步每跑完一次 `mir_flow` 立刻做（基线明确要求它是显式步骤，不能省）。
- **部分机判**：
  - 「`pixel_source` 在战斗与对话阶段被记账且非空」：**战斗有**（`COMBAT_ACTION … pixel_source=` + 证据字段）；**对话没有**——`Get-NpcDialogEvidence`/live npc 阶段根本不产出 `pixel_source` 字段（只在 combat 里出现，`:1348/:1437/:2933/:2947`）⇒ 这一半**不可机判**（不是本轮能补的），见 §8-P8。
  - 空串/篡改/类型非法三态的离线复验由 `tools/capture/clienthook/selftest_verify_chunks.ps1`（13 条）覆盖；它**不在这五个入口里**，本单不在窗口内跑。若 Leader 要求窗口内也复验三态，需要把该 selftest 加入允许入口。

---

## 5. 哪几条仍然只有人能判（本单的诚实边界）

| 判据 | 机判部分 | 只有人能判 / 本轮判不了 |
|---|---|---|
| G1 | Rust 日志 `角色数据保存中` 计数=0 | `save_settle.changes≥1` 与「两侧」（本窗口只有 Rust 侧）⇒ §8-P9 |
| G2 | pid `roles[].exe`/mtime、`-WhatIf` 的 build/sha、hook DLL sha、`HOOK_READY` | 无（「本检出产物」只到 path+mtime，不含源码内容一致性；见 §8-P13） |
| G3 | 金币、真物品数、NPC 脚本能力、怪物声明、地板物结构性 | 「可卖物品在**约定槽**」；「怪物**真的刷出**」⇒ §8-P5/P1 |
| A1 | 成功帧 650 + 金币**方向** + 对话前置 + 物品帧计数 | 金币差**等于价格**、目标槽**同一 StdIndex**；像素/DPI ⇒ §8-P4/P10 |
| A2 | 成功帧 648 + 金币方向 + 物品帧计数 | 金币差**等于售价**、该槽**消失**、与 A1 共用同一物品（两次 run）；像素/DPI ⇒ §8-P3/P4/P10 |
| A3 | **进入**（1010 + body>0 的 643/767） | **退出**（退出后不再收）；像素/DPI ⇒ §8-P6/P10 |
| A4 | HP 递减/死亡帧 + 三档 kind（6/7/8）+ `pixel_source` 记账 | **尸体在 AP 内**；attack-vs-walk 根因；**「同图」= 操作者断言 `-PlayerMap`（须带来源，缺 ⇒ UNDECIDABLE）**；像素/DPI ⇒ §8-P7/P10/P14 |
| F1 | chunks 逐事件 sha256 + 块数/bytes | **对话阶段的 `pixel_source`**（字段不存在）⇒ §8-P8 |

---

## 6. 本单实测过的命令（离线，未起服务端/未连库/未占端口）

| 探针 | 命令要点 | 实测结果 |
|---|---|---|
| USAGE 契约 | `mir_flow -StageEvidenceCheck -ProxyDir E:/tmp/dev1/nope -PickupStage` | `MIRFLOW USAGE: proxy 输出目录不存在…`，rc=2，无 `kind=` |
| 离线阶段证据格式 | `mir_flow -StageEvidenceCheck -ProxyDir <空 chunks 目录> -NpcDialogStage -TradeStage` | 两条 `STAGE_EVIDENCE … kind=action_missing rc=1`，`STAGE_EVIDENCE_TOTAL rc=1`，证据文件 `schema=openmir2.mirflow-stages/1` |
| hook 目录当 `-ProxyDir` | `mir_flow -FixtureDryRun -FixturePreflight -ProxyDir <hook 目录> -NoticeEvidenceCheck` | `proxy session.json 不存在…` + 堆栈，**rc=1**（不是 USAGE）⇒ §8-P2 |
| verify_chunks 四态 | `-CaptureDir` 不存在 / 空 chunks | rc=2 USAGE / rc=1 `RED data 事件 0 条` |
| G3 落点可走性 | `provision --check --map 0 --char-x 330 --char-y 330 --profile butch_monsters.json` | rc=0；`char_x=330,char_y=330` 可走 |
| G3 阶段 A 落点 | `provision --check --map 3 --npc-script 盟重土城/药店` | rc=0；`char_x=360,char_y=334`，NPC=(361,335) |
| 已部署 MonGen | `grep -P "^0\t" <ROOT>/M2GameSvr/Envir/MonGen.txt` | 53 条 map 0 行，**没有 (329..331,329..331) 的鸡** ⇒ §8-P1 |

**本单未做**：未连 `mir2_test_db_r`/生产库、未起任何服务端/客户端/proxy、未停栈、未占端口、未写 `E:/MirServer`。所有命令里的像素是**旧投影起点**，不是标定值。

---

## 7. 读到但没用的 flag（以及为什么不用）

| 入口 | flag | 为什么不用 |
|---|---|---|
| `mir_flow.ps1` | `-CapturePath` | 四类动作强制 `-ProxyDir`（capture.jsonl 无编码后 body 长度，判不了 643/767 前置；`:1988/1991`），且两源互斥 |
| | `-EvidenceSinceMs` | 默认取 proxy `session.json` 的 `start_wall_ms`；手工覆盖可能把旧帧算进来 |
| | `-RequireLogEvidence` | 会把「日志有 `[ERR]/panic`」折成 `rc=1`，与阶段 verdict 混在一起；日志水位单独看 |
| | `-PickupStage` / `-PickupPixel` | pickup 结构性不可达，本轮不跑 |
| | `-AttackClicks` / `-AttackRing` / `-AttackTurn` / `-AttackTurnMap` / `-AttackTurnCalibrate` / `-AttackTurnDryRun` | 旧战斗/标定路径，不用于 A4；A4 用 `-CombatStage` |
| | `-CenterX` / `-CenterY` | 只影响旧攻击环与跑动拖拽；A4 用显式像素 |
| | `-ClickNpc` / `-NpcClick` | A3 用 `-NpcDialogPixel`；固定 NPC 点击是别的场景 |
| | `-AutoPlaySec` / `-RunHoldMs` / `-WalkPath` / `-SeeOthersWaitSec` / `-TalkText` / `-KickVia` | 不在本轮 8 条内 |
| | `-CreateChar` / `-CharName` / `-NewChrBtn`… | 建角不在本轮 |
| | `-FixtureDryRun` / `-FixturePreflight` / `-NoticePollSim` / `-NoticeEvidenceCheck` / `-NoticePixelProbe` / `-NoticeWindowProbe` / `-NoticeProbe` | 离线自检/公告探针；本单只用了它们做**离线探针**，不进窗口 |
| | `-TradeDialogMaxAgeSec` | 默认 60s 足够；不是判据变量 |
| `provision...py` | `--a2-evidence` | 只用于解释地板物结构性；G3 的记账用 `--gate --need floor_item` 已足够 |
| | `--check` | 只在 §6 用来验证落点可走，不是判据 |
| | `--selftest` | 卡级自检，不占窗口 |
| | `--bag-std-index/--bag-dura/--bag-dura-max/--bag-make-index/--bag-position` | 角色已有 3 件真物品；加物品会改夹具并要求额外回滚，本轮不需要 |
| | `--game-point` | 随 apply 一起写（10），不是 G3 判据 |
| | `--frames` / `--expect-gold` / `--expect-rows`（`--verify`） | **本窗口没有合法输入**：它要 `frames.ndjson`，而 `mir2_proxy` 只产 `chunks.ndjson`+`conn-*.bin`；转换工具不在五入口内 ⇒ §8-P4 |
| | `--map-root/--envir-root/--repo-root/--mysql/--host/--port/--user/--password` | 默认已解析到本机测试栈；窗口不写生产库口令 |
| `verify_chunks.ps1` | `-Quiet` | 本单已把它改成**真静默**（只打摘要行，逐事件明细不出屏；退出码不变）。窗口默认不加，排障降噪时才加；自检有 6 颗牙（rc 不变 / 明细消失 / 行数减少 / 摘要仍在 / GREEN 侧 / USAGE 串） |
| `dbsnap_pair.py` | `--baseline-a/--baseline-b`、`--key-mode`、`--time-mode`、`--tables/--require-tables/--scope`、`--ignore-columns`、`--masked-columns`、`--jsonl`、`--summary-out`、`check-spec`、`selftest` | 全部读过；但**没有任何允许入口能产出它的输入快照目录**（`dbsnap.py` 不在五入口内，`provision` 不产快照）⇒ 本窗口用不上，见 §8-P4 |
| `start_mir2_server.ps1` | `-RequireRelease` | 与显式 `-ServerExe` 同给会因 `BuildType=Explicit` 直接 RED（§2-G2 注意） |
| | `-Roles/-WaitSeconds/-DbName/-RngLog/-Ports/-Seed` | G2 不需要；`-Seed 42` 属于第 3 步起栈参数 |
| `build.ps1` | `-Bds` | 默认 `D:\Program Files (x86)\Embarcadero\Studio\21.0`；仅当 BDS 装在别处才改 |
| | `-Force` | 每次都重编；不需要额外语义 |

---

## 8. 口径问题清单（**交 Leader 裁决；本单未自行改口径**）

> 排序按影响：P1–P3 会让窗口当天**测不到/测错**；P4–P8 是「基线比工具强」；P9–P13 是记账/流程细节。

### P1（关键）A4 的怪物夹具在七步流程里**没有真正的应用步骤**

- 基线要求「声明夹具 ⇒ 真刷出；刷不出 ⇒ spawn_failed」。但 `fixtures.monsters` 的**应用**是 `tools/conformance/card_window.py` 在**起栈前把行前插到 `<root>/Envir/MonGen.txt`**（`card_window.py:4968-5033`，`FIXTURE_MONGEN_LAYOUT`）。七步流程直接 `start_mir2_server.ps1` 起栈，**没有任何一步读 `tools/window/profiles/butch_monsters.json` 去改 MonGen**；`provision` 也只检查 profile 声明，不刷怪。
- 实测：`E:/tmp/smoke/rust-root/M2GameSvr/Envir/MonGen.txt` 有 53 条 map 0 行，**没有 329..331 的鸡**。
- 影响：A4 在窗口当天很可能不是「测到 spawn_failed」，而是「客户端看不到靶子 ⇒ 点击落成走/跑 ⇒ 无 3014 ⇒ rc=1 action_missing」；这与基线「刷不出 ⇒ 7」的归类不一致（见 P7）。
- 需要裁决：是否允许在窗口流程里加一步「MonGen 前插」；若允许，用哪个入口（`card_window` 的 fixture 应用 / 手工前插 / 另开小卡）。**本单不写这一步的命令**，因为它超出五入口且会改部署根。

### P2（关键）`mir_flow` live 四类动作要求 `-ProxyDir`，但七步流程没有 `mir2_proxy` 步骤，端口剖面也对不上

- 事实：live 动作强制 `-ProxyDir`，且 `Assert-ProxyLive` 要求目录里有 `session.json` + `ready.flag` + pid 存活（`mir_flow.ps1:672-681`）。hook 目录只有 `chunks.ndjson`/`conn-*.bin`/`hook-ready.flag`，**没有 session.json/ready.flag**。
- 实测：把 hook 目录当 `-ProxyDir`（`-FixtureDryRun -FixturePreflight`，不启客户端）⇒ `proxy session.json 不存在…` + 堆栈，**rc=1**（输入/接线错误被当成了 RED，属 B1 同类残留）。
- 另一层：计划 §2 第 3 步用 `E:/tmp/smoke/port-profile.json`（标准端口 5000/7000/7200）；若同时要 `mir2_proxy` 监听 7000/7100/7200，就必须把 Rust 栈平移到 17xxx（`tools/window/alt_port_profile.py` 的剖面）并给客户端对应端口。两者不能都用标准端口。
- 需要裁决：七步流程是否补一步「起 `mir2_proxy` + alt 端口剖面 + 客户端路由」；`mir2_proxy.py` 不在五入口内，本单不写它的命令，只把 `-ProxyDir $PROXY_*` 作为**前置已在跑**。

### P3 A1/A2 的「一次跑两方向、共用同一物品」在工具上不成立

- `-TradeKind` 是单值（`buy|sell`），`TradeStage` 只点一次菜单（`:2849-2868`）；一次进程只能判一个方向。本单只能拆成两次 run（A1、A2），并且「同一物品」要靠操作者在卖菜单里选同一件（人判）。
- 需要裁决：接受「两次 run + 人保证同一物品」，还是另开卡实现双方向。

### P4 A1/A2 的 DB 面（差=价格 / StdIndex / 槽消失）在本窗口不可机判

- `Get-TradeEvidence` 只判：成功帧（650/648）+ 金币**方向**（`gold_delta` 正负）+ 对话前置；物品帧只记 `item_effect.count`，**不核 StdIndex/槽位**。
- 能读 DB 的入口只有 `provision --gate`（`--need gold|sellable_item` 会给出**整行角色** `details.char`
  —— `Id/ChrName/MapName/CX/CY/Gold/GamePoint/Level` —— 以及 `bag_rows`/`bag_items` 计数；仍**不给**
  `Position/StdIndex`）和 `dbsnap_pair.py`（只 diff 已有快照目录）。**没有任何五入口内的工具能产生 dbsnap 快照**（`dbsnap.py` 不在清单；`provision --verify` 要 `frames.ndjson`，`mir2_proxy` 不产）。
- 影响：基线「差 = 价格」「目标槽出现同一 StdIndex」「该槽消失」**本轮不可机判**；工具 GREEN 只是必要不充分。`docs/real-machine-window-plan-20261025.md` §4 已声明「R 卡收口走 `card_window` 两侧口径」，那才是这些 DB 子句的归宿。
- 需要裁决：接受「本窗口只判方向/成功帧，DB 子句留给 card_window」，或把 `tools/dbsnap/dbsnap.py` 加入允许入口（同时补快照的 before/after 时点与 G1 的 save settle）。

### P5 G3「可卖物品在约定槽」只被数成「有 N 件」

- `prereq_gate` 的 `sellable_item` 只查 `COUNT(*) WHERE StdIndex<>0 >= 1`（`provision...py:393-405`），不返回 `Position/StdIndex`。基线要求「在约定槽」。
- 影响：G3 能做到「有可卖物品」，做不到「在约定槽」。本单按「部分机判」标注。

### P6 A3 的「退出」没有机判载体

- 工具只判**进入**（1010 + body>0 的 643/767），没有退出动作/退出帧判据；基线要求「进入与退出（退出后不再收）」。
- 影响：A3 只有「进入」是机判；退出只有人能判（观察/截图）。**不以 525/520 出现为判据**这一条已满足（脚本根本不看 525/520）。

### P7 A4「声明了却刷不出 ⇒ spawn_failed」在真机可能先落 rc=1

- `spawn_failed` 只在「已有 `CM_HIT(3014)`、前方格命中声明靶子、AP 内、但无 31/32」时成立（`:1329-1489`）。若靶子没刷出/没同屏，客户端把点击当走路，**没有 3014** ⇒ `action_missing rc=1`。
- 这正是已知 attack-vs-walk 根因（`docs/client-flow-stages-20261025.md` §7.2）：工具无法从「没有 3014」区分「没刷出 / 点歪 / 转向错 / 坐标偏移」。基线把它归 `spawn_failed(7)`，真机可能归 1。
- 需要裁决：是否接受「先证明 `CM_HIT` 已发出，才允许用 7」；否则「刷不出」在本轮是**不可机判**的成因。

### P8 F1 的「对话阶段也记 pixel_source」实现里不存在

- `pixel_source` 只在 combat 路径产生（`Get-CombatEvidence` `:1348/:1437`，live `:2933/:2947`，stdout `COMBAT_PIXEL`/`COMBAT_ACTION`）。`Get-NpcDialogEvidence`、`Get-TradeEvidence` 与 live npc/trade 阶段**没有这个字段**。
- 影响：F1 的「战斗与对话阶段都记账且非空」只能满足战斗一半；对话一半**不可机判**（不是像素值问题，是字段不存在）。

### P9 G1 的 `save_settle.changes ≥ 1` 不在本窗口

- `save_settle` 只由 `tools/conformance/card_window.py` 的 `run_side` 产出（`save_settle.json`，`card_window.py:3521/5441`），属 `card_window` 两侧 harness；七步冒烟流程没有任何一步调用它。
- 「两侧」在本窗口也不成立：C# 六角色是停掉的，只有 Rust 侧 `game-svr.out.log`。
- 影响：G1 本窗口只能判「Rust 日志 `角色数据保存中` = 0」；`save_settle` 与「两侧」两半**不可机判**，留给 `card_window` 收口（计划 §4 已有此口径）。

### P10 像素/DPI 不可机判（四类动作都受影响）

- 所有 `-*Pixel` 都是**绝对屏幕坐标**，依赖：窗口固定在标定位置、缩放 100%（无 DPI 虚拟化）、分辨率不变、48×32 步长成立（该步长未证实，`docs/window-shrink-run-20261019.md` §F2 有反例）。
- 工具只能判「点击后有没有出现预期帧」；**无法判「像素是否点对了格」**。点错 ⇒ `action_missing/no_dialogue/aim_mismatch rc=1`，与「真没发生」同码。
- 影响：A1–A4 的像素参数属于**人判/当场标定**；本单给的 `560,336`/`600,300`/`464,272` 只是旧投影起点，**不是通过阈值的一部分**。窗口当天必须先按截图标定（或先跑一次只截图不判的 run）。

### P11 `start_mir2_server.ps1` 的 `-RequireRelease` 与显式 `-ServerExe` 互斥

- 显式 `-ServerExe` ⇒ `$BuildType='Explicit'`；`-RequireRelease` 判 `$BuildType -ne 'Release'` ⇒ 直接 RED（`:407-410`）。计划 §2 第 3 步写的正是 `-ServerExe <本检出 release 产物>`，若再加 `-RequireRelease` 会假红。
- 处置：本单 (a) 不给 `-ServerExe`（用默认 release 解析）；或用显式 exe 但不加 `-RequireRelease`，用 pid 的 `roles[].exe` + mtime 判。无需改工具。

### P12 第 6 步的 `report.json` 在本流程没有生产者

- `report.json` 是 `card_window` 的证据包件（`docs/window-runbook-20261023.md` §3：`report.json`+`README.txt`+`card-window.json`+两侧 `frames.ndjson`）。七步冒烟流程不跑 `card_window`，本窗口的等价件是 `flow-stages-evidence.json` + `stages.ndjson` + `provision` apply 报告 + 截图。
- 影响：第 6 步的「存 `report.json`」在本流程落空；本单按等价件记账，不伪造 `report.json`。

### P14 A4 的「同图」是操作者断言，不是线上/DB 证据

- `mir_flow.ps1` 判 `out_of_range` 的「同图」= profile `fixtures.monsters[].map` vs **`-PlayerMap`**
  （`:1381` / `:2899`）；`-PlayerMap` 由操作者给（`:182` 注释原文「经典协议线上没有图名；必须显式给」）。
- 影响：`-PlayerMap` 给错会让「靶子在别的图」「真的刷不出」两种归因互相串台；这一半不是机判。
- 处置（本单已落地）：§3.4 要求同跑 `player-map-assertion-A4.json`（`map` + `source_kind/source_cmd/source_value/by`），
  ③ 阈值在**缺来源**或**断言的 map 与证据 `profile.player_map` 不一致**时判 **UNDECIDABLE**；§5 已点名。
- 备注：`card_window` 那条路不走 `-PlayerMap`，不受此影响。

### P13 G2 的「本检出产物」只到 path+mtime，不含内容一致性

- 基线只要求 `roles[].exe` 是本检出 release 且 mtime ≥ head。脏树构建、换了依赖但没改源码、或把旧 release 的 mtime 刷新，都能满足这条。
- 影响：这是基线的强度上限，不是本单能加的判据；如需更强，另开卡（例如记录 `SERVER_EXE_SHA256` 并与 CI 产物对账）。

---

## 9. 残留物

- 本卡只改两个文档文件：本文件 + `docs/real-machine-window-plan-20261025.md` §1 的一行指针。
- 离线探针产物在 `E:/tmp/dev1/`（`vcs.json`、`hook/`、`o2/`、`o3/`、`port-probe.md`、`chk-b.json`）；未入库，会话结束可删。
- **窗口当天要登记的残留**：`$W`（日志/dump/截图/gate JSON/apply 报告）；若 A1 买进的物品未被 A2 卖掉，`characters_bagitem` 会多一行——按 §1.3 的注意登记并手工清理/下轮 `--apply` 前核对。
- 未起服务端/客户端/proxy，未占端口，未连 DB，未写 `E:/MirServer`。
