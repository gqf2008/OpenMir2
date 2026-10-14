# evidence-m3-drop-exp-oracle-ref-tooth-20261014

卡 `openmir2-m3-rng-trace-scene-groups`（M3 夹具卫生片）——**④ 消费者表 `oracle_ref` 保鲜牙 + 真身错锚修正**。

> 写集：`mir2-rs/tests/parity/m3/**`；**零 `.rs`**；base = `origin/master` `6dba8719d`（`merge --ff-only` 前进）。
> 零起栈 / 零端口；共享主检出未动；未 merge、未自签 review。

## 结论

- **同族缺口补牙**：`drop-exp/drop_exp_consumers.tsv` 的 `oracle_ref` 列此前**只读不校验**（同仓
  `rng_consumer_groups.py` 早已 fail-closed，本表没有）。本片在 `drop_exp_purity_gate.py` 补齐：
  ① 存在性 + 行号范围；② **C# 行 ref 必须落在其 `pattern` 点名的方法体内**（语义牙）。
- **真身错锚**：第 46 行 `RegenMonsters` 的 ref 原写 `WorldServer.MonGen.cs:238`（该行是 `}`，
  落在 `ProcessMonsters` 里）⇒ 修正为方法声明 `:811`。**语义牙是承重的**：只看存在性/范围抓不到
  238（它在文件范围内）。
- **判据全绿**：`python -I drop_exp_purity_gate.py selftest` = **28/28 rc=0**（原 23 + 新 5 条）；
  真表 `oracle-refs-fresh` = `[]`。
- **改坏必红**（真表）：`:811`→`:238` ⇒ `oracle-refs-fresh` FAIL 且点名，rc=1；还原 ⇒ 28/28 rc=0。
- **四门禁**：fmt 0 / clippy 0(`-D warnings`) / **101 suites 1205 passed 0 failed 22 ignored** / msgcodegen 604。

## 判据→证据映射

| 判据 | 证据 |
| --- | --- |
| 门 28/28 GREEN | `02-selftest.txt` |
| 改坏必红（真表真身 :238） | `03-mut-real-table-red.txt` |
| 新增 4 条改坏/真身牙 | `04-mut-synthetic-teeth.txt` |
| oracle_ref 全表审计（15 条 csharp 行方法体归属） | `05-oracle-ref-audit.txt` |
| 四门禁 | `06`–`09` |
| 边界/冻结面/零 .rs | `10-boundary.txt` |

## 改坏必红原始输出（摘）

    ### 2) MUTATED（真身错值 :238，改坏）
    SELFTEST FAIL oracle-refs-fresh expect=[] got=['行46(csharp/wallclock): oracle_ref 行 238 不在
    `RegenMonsters` 方法体内（range=[(811, 878)]；ref=src/GameSrv/Word/WorldServer.MonGen.cs:238）']
    SELFTEST ok=27/28
    rc=1

## 边界（如实）

- Rust 侧 `measured`/`setup` 行仍是**文件级**前缀锚（`#[track_caller]` 世界层调用点）；收紧到
  `attack_sites.tsv` 式行级锚属 reviewer-5 `108fc72b` O-1 明示的**窗口期/④ 侧**工作，本片不动。
- 卡级 R 真机 DoD（同场景含怪两相 trace + `groupdiff --fold --strict` site/order diff=0）仍待
  承接卡 `openmir2-m3-rng-scene-driver` + owner 独占窗口。
- EOL/清单口径：按**仓库字节（git blob / LF 归一）**记。复核（本片实跑，全程在作者 worktree；未起栈）：

      $ python -I tools/evidence-hash/verify_sha256sums.py verify --check-extra <EV>
      GREEN mir2-rs\tests\parity\m3\evidence-m3-drop-exp-oracle-ref-tooth-20261014\SHA256SUMS.txt  [repo 口径]  OK 12/12

      $ python -I tools/evidence-hash/verify_sha256sums.py eolcheck <EV>
      EOLCHECK GREEN 13/13 件文本证据与仓库口径（LF）在 EOL 上一致

  > 注：`02-selftest.txt` / `05-oracle-ref-audit.txt` 初写时为 Python 文本模式 stdout 重定向（Windows 下 CRLF），
  > `eolcheck` 当场判红 2/13（同族第 N 次）；已按 LF 归一重写后 GREEN。清单 `write` 用 repo 口径，故口径自洽。
