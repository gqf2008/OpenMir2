# 证据包 —— 残余 observation 收口（2026-10-14）

- 卡：`openmir2-m3-rng-trace-scene-groups`
- 片：收口 reviewer-5 `c3fef2fd` O-1 / reviewer-5 `9dce8b2e` M-1 / reviewer-6 `37c322a9` M-1
- base = origin/master = `768b2bb6c4b81e774a80a2ad27fda0cccdb1a12c`（**base 前进重交**：原片 base 为 `6afa37636`；2026-10-14 `git merge origin/master` 后重交，零冲突、内容逐字节不变，见 `13-base-advance.txt`）
- code_head = `8c38c0fcd3c20fca59fcc0055c86404a95b18e02`（= 合并 master 后的内容/工具提交；内容与 `9a9d93e8b` 逐字节相同）
- evidence_head = 本 pack 所在提交（= walgit `patch` 条目的 head；落笔后钉全 oid）
- 写集：`mir2-rs/tests/parity/m3/**`，零 `.rs`；零起栈、零端口

## 件表

| 件 | 内容 |
| --- | --- |
| `00-commands.txt` | 本片命令 + 口径 |
| `01-observation-closures.md` | 三条 observation 的逐条处置 + 未收项登记 |
| `02-selfcheck-nostack.txt` | `-SelfCheckNoStack` 全转录 **37/37 rc=0**（含新增 `evidence-counts-selftest` / `evidence-counts-real-green` / `evidence-counts-head-mutation-red`） |
| `03-evidence-counts-selftest.txt` | `check_evidence_counts.py selftest` **10/10 GREEN**（含 §0 短 oid 必红 / 全 oid 绿 / 仓内不可解析必红） |
| `04-evidence-counts-real-green.txt` | 真报告按真实证据包 `check` **rc=0**（`status_oids={head:c4dd70086…, base:c127ae9a4…}`） |
| `05-evidence-counts-head-mutation-red.txt` | 改坏必红：副本报告 §0 head 改 7 位短 oid ⇒ **rc=1** 点名 `declared_head` |
| `06-mutation-strength-repro.txt` | 改坏力度复现：弱 **21/22**（1 FAIL）/ 强 **20/22**（2 FAIL）/ 还原 **22/22** |
| `07-offline-regression.txt` | 离线全链（8 个 selftest + `fixture_chain_gate check` GREEN） |
| `08..11-gates-*.txt` | 四门禁：fmt rc=0 / clippy rc=0(`-D warnings`) / workspace test **101 suites 1200 passed 0 failed 22 ignored** / msgcodegen 604 rc=0 |
| `12-boundary.txt` | 净差量 **25 路径**全在 `mir2-rs/tests/parity/m3/**`（原交付片 23 + 本 burst 新增 `13`/`14`）；`*.rs`=0；冻结面空；base == merge-base；相对声明 base 回退假差量 0 |
| `13-base-advance.txt` | base 前进重交（`6afa37636` → `768b2bb6c`，merge `8c38c0fcd`）：清除 7 条回退假差量；四门禁 + 37/37 自检在新 base 重跑读数 |
| `14-reviewer6-f1-fix.txt` | reviewer-6 F-1 收口：`trace-seed-gate/SHA256SUMS.txt` 第 14 行 README.md 陈留旧 hash ⇒ 重算后 verify **GREEN 14/14**；O-1（code_head/evidence_head 拆分）/ O-2 处置 |
| `SHA256SUMS.txt` | 仓库 LF/git-blob 口径 |

## 改了哪些件（本片）

1. `M3RNG场景DoD②组清单机判组必须真出现报告-20261013.md` —— §0 head/base 钉全 oid（reviewer-5 O-1）。
2. `rng-trace-scene/check_evidence_counts.py` —— 新增 §0 `head/base` 全 oid 机判（+ `--repo` 解析）+ 3 条 selftest。
3. `rng-trace-scene/run_rng_trace_pair.ps1` —— 把上面机判接入 `-SelfCheckNoStack`（+3 项，34→37）+ ㉚ 注释 `#2`→`#3`。
4. `rng-trace-probe/rng_consumer_groups.py` —— 段头注释 `#2`→`#3`。
5. `rng-trace-scene/README.md` —— §5/§5.1/§6 计数与说明同步（34→37、7/7→10/10）。
6. `evidence-…-trace-seed-gate/{10-mutation-a-gate-ignores-stream-seed.txt,README.md,SHA256SUMS.txt}` —— 改坏力度说明 + 重算清单。

## 未收

卡级 R 真机 DoD（同场景含怪两相 trace + `groupdiff --fold --strict`）仍阻塞于承接卡
`openmir2-m3-rng-scene-driver` 未开 + owner 独占窗口 ⇒ 卡保持 in-progress。
