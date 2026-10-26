# 真机窗口计划（三类动作 + 指纹，2026-10-25 夜定稿）

> **为什么有这份文件**：真机窗口要独占机器、要停 C# 六角色、要注入新编 hook DLL，细节散在五六份文档里，
> 每次都靠回忆必然会漏。这里只做**索引 + 顺序 + 已知坑**，判据正文在别处（每节都有指针）。
>
> **当前状态**：**未开窗**。前置里「夹具」已合（`96ad82a37`）、「指纹四态」已合（`d161c3ee0`），
> 「判定三档成因」**已合**（`openmir2-mirflow-verdict-cause-split` → `e7fb3e78d`）⇒ **四条前置已全绿**，只剩两件具体操作（置金币、定位到靶子旁）。

## 0. 开窗前必须全绿的条目

> **认树锚（开窗当天照抄）**：下文第 1 行与「开窗当天用它认树」里的工具门数字（**52 项 / 清点 57 = 52 + 5 + 0**）是**故意保留的实数值**——它们用于确认「开的就是预期那棵树」。**加/删工具时必须同步更新**（同时更新 `--inventory` 清点差），不要改成「详见实跑」而失去认树能力。

| # | 项 | 判据 | 现状 |
|---:|---|---|---|
| 1 | 工具门 | `python -I -B tools/hygiene/tool_selftest_gate.py` ⇒ **52 项全绿**；`--inventory` rc=0 | **已绿**（历史：`8b7c9c281` 当时 47/47；`26ba230ea` 起 48 项；2026-10-27 起 **52 项**（含 C7 的 `mir_flow.selftest.ps1` 与 run_batch 的 `run_batch_selftest.py`），`--inventory` **57=52+5+0**。原始记录：`--inventory` rc=0 清点 52=47+5+0；并用「假未分类工具在场 ⇒ `--selftest` 仍 18/18 绿、`--inventory` rc=2 点名、删掉后 rc=0」独立复验） |
| 2 | 判定三档 | `mir_flow` 的 `missing_prereq / spawn_failed / out_of_range` 可机判区分 | **已绿**（`e7fb3e78d`：三档 kind/rc=6/7/8，`selftest 148/148`；用法错 rc=2 与判据彻底分家） |
| 3 | 夹具 | `provision_...py --gate ...`（逐阶段求值，见下） | **已实测**：动作分布在**两张图**上 ⇒ 需两次 `--apply`，详见本节下的「夹具实测」 |
| 4 | 机器空闲 | `E:/MirServer/port-status.ps1` 十端口在监听（基线态）；**3306 UP**（pid 现查，**不要写死**） | 当前满足。**注**：`mysqld` pid 会变（2026-10-26 实测由 2580 → 3308，22:04:18 起重启过）⇒ **pid 变化说明 MySQL 重启过，之前取的 DB 基线可能失效，需重取**；判据只看「3306 UP + 能连」，不看 pid |

> **操作口径（卡 `openmir2-mirserver-graceful-stop`）**：停栈时 **mysqld 必须优雅关闭**（`tools/window/stop-mysql-graceful.ps1`，超时也**不强杀**）；启动/停止常驻服务的调用**一律输出落盘再读文件**（`tools/window/invoke-bounded.ps1`），**禁止** `| Select-Object -Last N` 这类管道截法——2026-10-26 有车道因此挂了 10h12m。细节与 `start-all.ps1 -Stop` 的 diff 建议见 `docs/window-runbook-20261023.md` §0.13。
> **DB 实例边界（2026-10-26 22:04:18）**：`mysqld` 曾被 `start-all.ps1 -Stop` 的 `Stop-Process -Force` **强杀**，新实例带 `InnoDB: Starting crash recovery from checkpoint LSN=…` 启动。⇒ **该时刻之前取的 DB 证据一律打折/重取**；判据是 `E:/MirServer/logs/MySQL.err.log` 里**有没有 crash recovery 行**，不是 pid 变没变。详见卡 `openmir2-mirserver-graceful-stop`。
| 5 | **批次自检** | `bash tools/window/run_batch.sh --dry-run` ⇒ **rc=0** | **这是「每次开窗前」的动作，不是常驻属性**：窗口跑过一轮之后必然又有 2 项变红 —— ① `castle_list_symmetry`（C# GameSrv 每 600s 重写 `Castle/List.txt`，跑完就与 rust 侧不一致）② `pin_smoke`（最后一个 Rust 栈日志若来自 alt 剖面则无 `layout=oracle-40` 自证行）。⇒ 开窗第 0 步（停完 C# 之后）三侧对齐 castle 文件，并**先起一次带 pin 的 window-magic 私栈**把自证行跑出来，再 dry-run。**dry-run 不需要 `CONFORMANCE_PASSWORD`**：缺它时 rc=0 + stdout 一条 `WRN: 缺 CONFORMANCE_PASSWORD`（真跑才 fail-closed rc=2）——卡 `openmir2-runbatch-dryrun-password` |

### 开窗第 0 步（castle 三侧对齐 + 跑前基线，卡 `openmir2-castle-symmetry-baseline`）

停完 C# 之后、起私栈之前：把**部署件当时的值**复制到三侧（csharp / csharp-hook / rust）的
`Castle/List.txt` 与 `Castle/*/SabukW.txt`，再落跑前基线；`run_batch.sh` 会自动写
`<run_dir>/castle-before.txt`（起栈前）与 `<run_dir>/castle-after.txt`（跑完），并把差异分成
**「本轮造成」/「开窗前已存在」**两档。命令、文件格式、参考值（`List.txt`=`0\r\n1\r\n`；
SabukW 部署件 9260B→实测 9290B，会变，别写死）见 runbook **§0.9b**。

### 开窗当天必读：本机 PowerShell 5.1 **缺 `Get-FileHash`**

`tools/window/rebuild-server.ps1` 在 `powershell`（5.1）下直接报 `Get-FileHash is not recognized`（`PSModulePath` 里 `Microsoft.PowerShell.Utility` 未加载）⇒ **窗口相关脚本一律用 `pwsh`（PowerShell 7）跑**：

```
"/c/Program Files/PowerShell/7/pwsh" -NoProfile -ExecutionPolicy Bypass -File tools/window/rebuild-server.ps1 -SyncStartRust
```

2026-10-26 02:02 用 pwsh 跑通：`SERVER_EXE_SHA256=24423c13…`、`BUILD_STAMP` 生成、`START_RUST_SERVEREXE` 已从 `.worktrees/dev-2/...` 对齐到本检出。


### 夹具实测（2026-10-26 00:50 / 01:02，只读 `--gate`）

| 跑法 | 结果 |
|---|---|
| `--map 3 --npc-script 盟重土城/药店` | `missing = [gold, monsters]`：`characters.Gold=0 < 500`；该图无靶子 |
| `--map 0 --profile tools/window/profiles/butch_monsters.json` | `missing = [gold, npc]`：**靶子有了**（该 profile 在 map 0 的 (330,331) 刷 1 只鸡），但「药店」脚本在 map 3 |

⇒ **G3 必须逐阶段求值**：
- **阶段 A（买卖 / NPC 对话）**：`--map 3`，药店 NPC 在 (361,335)
- **阶段 B（有目标战斗）**：`--map 0`，靶子在 (330,331)，角色需定位到其相邻格

开窗前要跑**两次** `--apply`（置金币 + 分别定位），**每次各自保存一份 gate JSON**——其 sha256 会由 `mir_flow` 的证据记录下来，事后可对账（`-PrereqGateCmd` 记生成命令）。
侧证：`bag_rows=46 / bag_items=3`，与此前订正的「46 槽位、3 件真物品」一致。

**开窗当天用它认树**（数字对不上就说明不在预期 master 上）：
- `python -I -B tools/goal/goal_gate.py selftest` ⇒ **83 条断言 83 通过**（`f0d44ceeb` 实测；`80a820762` 的 rc=3⇒NEED 加了 4 条，此前 79）
- `python -I -B tools/hygiene/tool_selftest_gate.py` ⇒ **52 项全绿**；`--inventory` ⇒ 清点 **57 = 52 + 5 + 0**（2026-10-27 起，含 C7 与 run_batch；此前 51/56、50/55、48/53、47/52）
- `goal_gate --full` 产物里条件④ 应显示「**门禁 8 项**」（第 8 项 = 工具自测门，约 +120s）
- `cargo test --workspace` ⇒ **141 suites，running 3329 = passed 3266 + ignored 63**

> 1 与 2 任一未绿就**不要开窗**——否则那天会出现同一现象两种判法，或者跑完才发现门是红的。

## 1. 判据（8 条，正文见 `openmir2-mirflow-verdict-cause-split` 的 comment `b0c9b9c0`）

**1–3 是「能不能测」⇒ 不过记 UNDECIDABLE 并点名缺项；4–8 是「该发生没发生」⇒ 不过记 RED 并带成因 kind。**

> **可执行操作单**：每条的命令 / 看哪个字段 / 通过阈值 / kind·rc，以及三属性（需否服务端、读不读 DB、是否写入）与七步对齐，见 `docs/window-criteria-operational-20261026.md`；本表只留一句话摘要。

| # | 项 | 一句话 |
|---:|---|---|
| G1 | 存档在途门 | 两侧 `角色数据保存中` = 0 且 `save_settle.changes ≥ 1` |
| G2 | 构建/注入门 | `roles[].exe` 是本检出产物且 mtime ≥ head；**hook DLL sha256 记入证据** |
| G3 | 前提门 | 金币/可卖物品/NPC 发 643；**地板物恒缺 ⇒ 记 `missing_prereq(structural)`，不是失败** |
| A1 | 买 | 金币差 = 价格 且目标槽出现同一 `StdIndex` |
| A2 | 卖 | 金币差 = 售价 且该槽消失（与 A1 共用同一物品） |
| A3 | NPC 对话 | 对话态**进入与退出**；**不以 525/520 出现为判据** |
| A4 | 有目标战斗 | 靶子 HP 递减或死亡帧且尸体在 AP 内；刷不出 ⇒ `spawn_failed`；超 AP ⇒ `out_of_range` |
| F1 | 指纹 | 真机只取 `ok` 态（rc=0、块数、`data` sha256 非空串）；另三态离线复验 |

**pickup 不在本轮**：按 `96ad82a37` 裁决为结构不可达（三条独立代码事实 + `REEVALUATE rc=2` 触发器），
见 `docs/pickup-item-reachability-20261024.md` 与 `docs/real-machine-fixtures-20261025.md` §4。

## 2. 机器操作顺序（每一步都要留输出）

1. **开窗前存基线**：`powershell -File E:/MirServer/port-status.ps1 > E:/tmp/window-<date>/port-before.txt`
2. **只停六个 C# 角色**（**不要用 `start-all.ps1 -Stop`**，也**绝不动 mysqld**）：
   `Stop-Process -Id <GameSrv,DBSrv,LoginSrv,LoginGate,SelGate,GameGate> -Force`
3. 确认九端口 DOWN、3306 UP，再 `start_mir2_server.ps1 -Root E:/tmp/smoke/rust-root -PortsProfile E:/tmp/smoke/port-profile.json -ServerExe <本检出 release 产物> -Seed 42`
4. **重新编译并注入 hook**：`tools/capture/clienthook/build.ps1` → 记 DLL sha256 → 注入真客户端
5. 跑 `mir_flow`（三类动作）→ 跑 `verify_chunks.ps1 -CaptureDir <本次 hook 目录>`（**必须 GREEN**）
6. 存 `aaa` before/after、`report.json`、日志、截图
7. **恢复**：停 Rust 栈 → `E:/MirServer/start-all.ps1`（3306 已在跑会自动跳过 mysqld）→ `port-status.ps1` 证明十端口回原样

## 3. 已知坑（每条都是今天付过学费的）

| 坑 | 表现 | 正确做法 |
|---|---|---|
| ERR 水位只看一个「0」 | 换个栈后恒 0（假绿） | 只认 `E:/tmp/smoke/logs/*.err.log` + 时间窗；**同时报扫描文件数** |
| `verify_chunks` 用法错与校验 RED 同码 | 错参数名也 rc=1 | 现在已分：用法错 **rc=2 + USAGE**，篡改 **rc=1 + RED**，字段类型非法也 rc=1（第三来源） |
| `--gate` 默认含 `floor_item` | 恒 rc=6 | 用 `--need gold --need sellable_item --need npc --need monsters` |
| `--apply` 后 MISMATCH | `COMMIT` 早于核对 ⇒ 库已改 | rc=4 时要**手动 `--rollback <报告>`**，别当成已回滚 |
| 存档在途 | 服务端拒动作，看起来像行为差异 | 动作前过 G1 门，不过记 UNDECIDABLE |
| 攻击 vs 走路 | 三次点击全落走/跑，**一个 `3014 CM_HIT` 都没有** | 必须有靶子且点在靶子上；`pixel_source` 记账用来区分「靶子没刷出」与「投影点歪」 |
| 新增带 selftest 的脚本 | 工具门变红 | **必须同时登记 REGISTRY** |

## 4. 产出与记录

- 证据根：`E:/tmp/window-<date>/`（每次开窗新建，结束后把路径登记进交付报告 §5 临时物表）
- 结论表：**最多 8 行**（对应 §1），每行写明 kind/verdict + 判据命令 + 原始输出片段
- **R 卡不得凭此关账**：这是**冒烟驱动**证据；R 卡的收口仍走 `card_window` 两侧口径
