# 证据：预热相门正证收紧（reviewer-5 N-1）

- 卡：`openmir2-m3-attack-pair-fixture`（worktree `m3bag` / `openmir2-worker-4`）
- 来源：reviewer-5 在 approve `63d3eec5`（reviewed_head `1ceb5ad3`）里的 finding **N-1（Minor）**：
  `Test-M3PrewarmNoWorldEntry` 的正证 `创建角色` 是**子串匹配**，会命中同日志里的建角失败串
  `创建角色名称已被其他人使用！`/`创建角色时出现错误！`/`创建角色时出现未知错误！`
  （`src/BotSrv/Player/RobotPlayer.Operate.cs:185/191/194`）⇒ 在「建角失败」路径上正证仍为真、门**放行（fail-open）**。
- 本片修法（零 `.rs`；再动 `gate_guards.ps1` + `selftest_gate.ps1` + `run_pair.ps1` + `run_attack_pair.ps1`）：
  1. 正证锚到**生产整行** `创建角色 <name>`（`src/BotSrv/Scenes/Scene/SelectChrScene.cs:200`）——`创建角色` 后
     **带一个空格**，与上面三条失败串逐字互斥；`预热建角完成`（`SelectChrScene.cs:146`，`-StopAfterCharCreated` 短路点）保留。
  2. 新增 `failures` 收集；**只有失败串、无成功正证 ⇒ `prewarm_char_create_failed` fail-closed**（比 `prewarm_marker_missing` 更准确）。
  3. `run_pair.ps1` 预热门按 reason 分派新文案；`run_attack_pair.ps1` 文档串同步。
  4. `selftest_gate.ps1` ⑩ 增第 5 向「仅有建角失败串=红（N-1）」，`total` 64 → **65**。
  5. `whitelist.md` T-29 **第九轮**登记。

## 证据文件

| 文件 | 内容 |
| --- | --- |
| `01-selftest-gate.txt` | `selftest_gate.ps1` 全量输出：`SUMMARY total=65 failed=0`（⑩ 含 5 向） |
| `02-prewarm-gate-before-after.txt` | 改前（master `de0ea592` 的 `gate_guards.ps1`，裸子串）/ 改后（本片）在同一组构造日志上的对照 |
| `03-mutation-teeth.txt` | 改坏必红：正证改回裸子串 ⇒ ⑩ 第 5 向 `FAIL`、`total=65 failed=1`；还原后 `cmp` 逐字节相同 |
| `04-commands.txt` | 全部复现命令 |
| `05-attack-pair-selfcheck.txt` | `run_attack_pair.ps1 -SelfCheckNoStack` 全量输出（`SELFTEST_RED GREEN`，rc=0） |
| `06-four-gates.txt` | 四门禁（fmt/clippy/msgcodegen/test）——**交付前已合 master `de0ea592`** |
| `SHA256SUMS.txt` | 本目录各件 sha256（仓库字节口径） |

## 判据（改前 → 改后）

同一组构造日志（逐字节见 `02-*`）：

- `logs/create-failed.log`（**只有**失败串 `创建角色名称已被其他人使用！`）：
  - **改前**：`ok=True`（正证 `创建角色` 命中失败串）——**N-1 缺陷（fail-open）**；
  - **改后**：`ok=False reason=prewarm_char_create_failed failures=[创建角色名称已被其他人使用]`。
- `logs/noworld.log`（`创建角色 m3x0` + `预热建角完成…`）：改前/改后均 `ok=True`（正证 `[创建角色 , 预热建角完成]`）。
- `logs/world.log`（建角 + `进入游戏`）：改前/改后均 `ok=False reason=prewarm_entered_world`。

## 改坏必红（牙齿）

把正证 `'创建角色 '`（带尾空格）改回裸子串 `'创建角色'`（= master `de0ea592` 的旧行为）⇒
`selftest_gate.ps1` ⑩ 第 5 向 `FAIL`、`SUMMARY total=65 failed=1`；`cp` 还原后 `cmp` 与原文件**逐字节相同**、
`selftest` 回 `GREEN`（见 `03-*`）。

## 门禁 / 纪律

- 四门禁（`06-*`）：`fmt` rc=0 / `clippy -D warnings` rc=0 / `test` **83 suites / 674 passed / 0 failed / 16 ignored** / `msgcodegen --check` **604**。
- 零 `.rs`、零 `src/**`；**未起栈、未占端口**（5000..7200 未起监听，仅共享 3306）；未 merge、未自签 review。
- 解释器：本片在 **pwsh 7** 下取得判据。`reviewer-5 N-2`（PS 5.1 下 m3 脚本 `Get-FileHash` 不可用）**非本片差量**，按 reviewer 建议另开卡，未在本片处理。

## 残留物清单

- 无入库残留。临时件：`E:/tmp/m3bag-n1/`（construct logs / harness / 改坏副本）、
  `E:/tmp/openmir2-m3gs/selftest-n1{,-mut}/`（自检输出）——本片用毕即删或登记为临时；
  保留 `E:/tmp/openmir2-m3bag-target/`（隔离 cargo target，后续门禁复用）。

## hash 口径

`SHA256SUMS.txt` 用仓库字节口径（LF 归一 / git blob 字节）计算。验证方式（干净 clone 或本目录）：
`python tools/evidence-hash/verify_sha256sums.py ...` 或
`sha256sum -c SHA256SUMS.txt`（本目录）。
