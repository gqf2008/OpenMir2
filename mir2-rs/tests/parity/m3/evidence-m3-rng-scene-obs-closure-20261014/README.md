# 证据包 —— 残余 observation 收口（2026-10-14）

- 卡：`openmir2-m3-rng-trace-scene-groups`
- 片：收口 reviewer-5 `c3fef2fd` O-1 / reviewer-5 `9dce8b2e` M-1 / reviewer-6 `37c322a9` M-1
- base = origin/master = `6afa376363d3378933d95c26c45cd8eb7c9738f3`
- head = `d4274cdaf0fa51f2d5fc946e77f371b260b8837a`（= 本片代码/文档提交；其后只追加本证据目录）
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
| `12-boundary.txt` | 净差量 8 路径全在 `mir2-rs/tests/parity/m3/**`；`*.rs`=0；冻结面空；base 是 head 祖先 |
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
