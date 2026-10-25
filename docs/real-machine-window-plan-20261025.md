# 真机窗口计划（三类动作 + 指纹，2026-10-25 夜定稿）

> **为什么有这份文件**：真机窗口要独占机器、要停 C# 六角色、要注入新编 hook DLL，细节散在五六份文档里，
> 每次都靠回忆必然会漏。这里只做**索引 + 顺序 + 已知坑**，判据正文在别处（每节都有指针）。
>
> **当前状态**：**未开窗**。前置里「夹具」已合（`96ad82a37`）、「指纹四态」已合（`d161c3ee0`），
> 「判定三档成因」尚未合（`openmir2-mirflow-verdict-cause-split`）。

## 0. 开窗前必须全绿的条目

| # | 项 | 判据 | 现状 |
|---:|---|---|---|
| 1 | 工具门 | `python -I -B tools/hygiene/tool_selftest_gate.py` ⇒ 47 项全绿；`--inventory` rc=0 | **已绿**（`8b7c9c281`：47/47、`--inventory` rc=0 清点 52=47+5+0；并用「假未分类工具在场 ⇒ `--selftest` 仍 18/18 绿、`--inventory` rc=2 点名、删掉后 rc=0」独立复验） |
| 2 | 判定三档 | `mir_flow` 的 `missing_prereq / spawn_failed / out_of_range` 可机判区分 | **未合**（`openmir2-mirflow-verdict-cause-split`） |
| 3 | 夹具 | `provision_...py --gate ...`（逐阶段求值，见下） | **已实测**：动作分布在**两张图**上 ⇒ 需两次 `--apply`，详见本节下的「夹具实测」 |
| 4 | 机器空闲 | `E:/MirServer/port-status.ps1` 十端口在监听（基线态）；mysqld 2580 在 | 当前满足 |

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
- `python -I -B tools/goal/goal_gate.py selftest` ⇒ **79 条断言 79 通过**
- `python -I -B tools/hygiene/tool_selftest_gate.py` ⇒ **47 项全绿**；`--inventory` ⇒ 清点 **52 = 47 + 5 + 0**
- `goal_gate --full` 产物里条件④ 应显示「**门禁 8 项**」（第 8 项 = 工具自测门，约 +120s）
- `cargo test --workspace` ⇒ **141 suites，running 3329 = passed 3266 + ignored 63**

> 1 与 2 任一未绿就**不要开窗**——否则那天会出现同一现象两种判法，或者跑完才发现门是红的。

## 1. 判据（8 条，正文见 `openmir2-mirflow-verdict-cause-split` 的 comment `b0c9b9c0`）

**1–3 是「能不能测」⇒ 不过记 UNDECIDABLE 并点名缺项；4–8 是「该发生没发生」⇒ 不过记 RED 并带成因 kind。**

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
