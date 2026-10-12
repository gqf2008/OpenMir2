# 证据包 — M3 总验收盘点片：① 十项系统索引 + ③ 报告骨架 + ② 长跑方案（2026-10-13）

- 卡：`openmir2-m3-final-acceptance`（issue `b0bb3003`）；本片 = 卡上「不占窗口部分」。
- 车道：principal `openmir2-worker-5` ／ worktree `.claude/worktrees/m3cs` ／ branch `worktree-m3-final-acceptance`。
- base：`origin/master da94fcb7`。**零 `.rs` / 零 `src/**` / 零表结构·脚本语法·数据格式改动**。
- 本片**未起栈、未占端口**（九端口实拍 `window/01-ports-9.txt` 全 FREE）；写集 = `mir2-rs/tests/parity/**`。
- hash 口径：**仓库字节（LF 归一 = git blob）**，清单 `SHA256SUMS.txt`；复核命令见 `commands.txt` §8。

## 结论（本片实跑读数）

| 系统 | 本片读数 | 判定 |
| --- | --- | --- |
| ① 背包/装备 | `scoped_diff.py diff --canonical --fixed-newchr` = `GREEN 作用域内 9 张表逐行一致` rc=0 | **covered（diff=0）** |
| ② 战斗 | 三对 `attack_diff.py diff` 全 `GREEN diff=0（attacked=120 hits=57 dodges=63 sum=2425）` rc=0；`stream_equiv gate` 651/651 | **covered（逐 k=0）** |
| ③ 技能 | `combat_skill_vectors` 29 passed；registry `implemented=29/108`、cases=29；未实装 79 条逐条登记 | **covered** |
| 掉落/经验、任务、组队/交易、公会/攻城、英雄/宠物、邮件/拍卖 | 本片未跑 | **not_covered**（逐项给承接卡：掉落/经验·NPC/商店·组队/交易·公会/攻城·英雄/宠物·邮件/拍卖 = owner 已开卡；任务 = 建议卡） |
| NPC/商店 | M4 7/7 脚本层 GREEN（含商店/修理/仓库 3 条）；服务端派发未覆盖 | **partial** |
| ④ M3 门禁 `scoped_diff --canonical` | 真机隔离两相后 DB 侧 `characters_ablity` 一处 RED（F-2 轨承接） | 未达成（非本片轴） |
| ② 300 假人 24h 长跑 | **未跑**（等 owner 收拢窗口） | **not_run**（方案见 `M3-report.md` §2） |

## 工件索引

| 路径 | 内容 |
| --- | --- |
| `commands.txt` | 本片命令清单（含窗口纪律） |
| `window/01-ports-9.txt` | 九端口监听实拍（全 FREE ⇒ 未占端口） |
| `verify/10-evidence-verify.txt` | 上游证据包 hash 复核（t14-rerun 18/18、dod2 69/69、skill-probe 5/5、combat-probe 4/4、attack-request-sequence 39/39，全 GREEN） |
| `diff/20-bag-scope-canonical-fixednewchr.txt` | ① 判据重放（GREEN 9 表 rc=0） |
| `diff/21-attack-per-k-three-pairs.txt` | ② 三对逐 k（全 GREEN 0） |
| `diff/22-stream-equiv.txt` | ② `stream_equiv` selftest 10/10 + gate 651/651 |
| `diff/23-scope-m3-gate-canonical.txt` | §6 M3 门禁实跑：跨侧 `--canonical` RED 4 行 / `--fixed-newchr` RED 8 行；同侧 C# 自比 GREEN |
| `inventory/30-skills-combat-registry.txt` | ③ 技能/战斗工件计数（只读；生成脚本内联于 `commands.txt` §4） |
| `inventory/31-in-world-stability-selftest.txt` | DoD② 判据 (b) 机判器 `tools/botload/in_world_stability.py` 自测 7/7 |
| `inventory/32-in-world-stability-redcheck.txt` | 判据 (b) 实跑演示：稳态 ⇒ GREEN、改坏一桶 ⇒ RED、缺 `in_world` ⇒ PREREQ（fail-closed） |
| `inventory/iw_steady.jsonl` / `iw_dropped.jsonl` | 上面演示的合成 `load_stats` fixture（GREEN / RED） |
| `m4/40-npc-cases.txt` | M4 7/7 port-free NPC 用例 stdout（GREEN） |
| `m4/41-evidence-hashes.txt` | M4 证据 hash 复核（GREEN 19/19） |
| `gates/50-fmt.txt` `51-clippy.txt` `52-test-workspace.txt` `53-msgcodegen.txt` | 四门禁（全 rc=0；test 84 suites/687 passed/0 failed/16 ignored） |
| `SHA256SUMS.txt` | 本包清单（仓库字节口径） |

## 复现（全离线，不起栈）

见 `commands.txt`。所有命令只读仓库内既有件；唯一写盘是 `run_npc_cases.py` 写
`tests/parity/evidence/M4/npc-cases.txt`（确定性、重跑字节不变、`git status` 干净）。

## 残留物清单

- 本目录全部入库；`M3-report.md` 入库。
- 无临时目录/临时库/进程；未起栈、未占端口。
- 本 worktree 另有 5 个未跟踪会话目录（上一片 ②③ 真机跑，非本片产生）：见 `M3-report.md` §3。
