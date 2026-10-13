# 证据包：M3⑩ 元宝寄售动作序列夹具骨架（`openmir2-m3-selloff-fixture-botsrv`）

> 卡：`openmir2-m3-selloff-fixture-botsrv`（M3 逐系统对齐 ⑩ 的承接卡 3；父卡 `openmir2-m3-mail-auction-align`）。
> 本片 = **夹具骨架（夹具先行，同 ⑤-6/⑥-5/⑦-5 口径）**：比较器 + 场景 + 入口 + BotSrv 最小驱动 + 无栈自检 + 改坏必红。
> **真机两侧同序列跑（DoD②③）未做**：等 ⑩-1 `openmir2-m3-selloff-consignment-world`（世界侧 `bins/game-svr` 收包派发 + 帧）落地 + owner 发独占窗口。
> 分支 `worktree-m3-selloff-fixture-botsrv` @ `.claude/worktrees/m3cs`；base（本片提交时）= ⑦-5 head `d29d017b`（交付前 `git merge master`）。
> 输入 = 盘点报告 [`../M3⑩邮件拍卖现状盘点报告-20261013.md`](../M3⑩邮件拍卖现状盘点报告-20261013.md) §4。

## 文件 → 读数

| 文件 | 命令 | 读数 |
| --- | --- | --- |
| `01-selloff-diff-selftest.txt` | `python -I mir2-rs/tests/parity/m3/selloff_diff.py selftest` | **33/33** rc=0 |
| `02-run-selloff-pair-selfcheck.txt` | `run_selloff_pair.ps1 -SelfCheckNoStack` | **19/19** rc=0 |
| `03-botsrv-selloffseq-selftest.txt` | `BotSrv.exe --selftest-selloff` | GREEN 29/29 rc=0 |
| `04-run-bots-selfcheck.txt` | `tools/botload/run_bots.ps1 -SelfCheckNoStack` | GREEN（含 selloff 收尾计数牙齿）rc=0 |
| `05-selftest-gate.txt` | `mir2-rs/tests/parity/m3/selftest_gate.ps1` | `SUMMARY total=65 failed=0` |
| `06..09-reg-*.txt` | `run_{bag_equip,npc_shop,quest,party_trade}_pair.ps1 -SelfCheckNoStack` | 全 GREEN（共享件回归） |
| `10-external-break-comparator.txt` | 删 `compare()` 的 `"branch"` 比较后跑 `selftest` | **32/33** rc=1（`mutate-branch-red` FAIL）⇒ 外部改坏必红 |
| `11..14-gate-*.txt` | 四门禁（cwd=`mir2-rs`，`CARGO_TARGET_DIR=E:/tmp/m3cs-so-target`，`--offline`） | fmt 0 / clippy 0 / **93 suites 847 passed 0 failed 20 ignored** / msgcodegen 604 |
| `15-boundary.txt` | `git diff --name-only d29d017b -- src sql MirClient mir2-rs/crates mir2-rs/bins` | 仅 `src/BotSrv/**`（登记例外 T-44；BotSrv 不在 oracle 七进程内，默认关逐字节不变） |

## 判据（比较器 `selloff_diff.py` 的机器口径）

- 逐 `(bot,k)` 比：`op` / `role` / `ran` / `param` / 帧**种类序列**（严格顺序）/ 声明的标量帧参数 /
  `SM_MENU_OK(767)` 文本逐字节 / `gamegold_before/after`（元宝 `GameGold`）/ `gold_before/after`（金币 `Gold`）/
  背包**多重集** / `branch`；两侧 bot 集合必须相同、每 bot 的 k 集合必须相同。
- fail-closed：缺件 / 缺字段 / 类型错 / 非法 JSON / k 断档 / bot 集合不一致 ⇒ rc=2（`PREREQ_RED`），绝不静默 GREEN。
- 落库口径 = oracle「寄售不落库」（`SaveSellOffItemList` 调用点全注释、读列名写错 ⇒ 两侧都不写、都加载 0 行）：
  由 `run_selloff_pair.ps1` 的 `scoped_diff --canonical` + `goldsales` 表增量 = 0 承担，本比较器不在记录里造跨重登保留。

## 未做 / 下一步（DoD②③）

- 真机两侧同序列 diff=0：前置 = ⑩-1 世界侧派发；本片 `-Side rust` preflight fail-closed，未起栈、未占端口。
- 窗口期若发现 `dSellDateTime` 需逐字段比 ⇒ 依赖横切前置 `openmir2-m3-clock-injection`（本片只登记；当前判据不含该展示字段）。
