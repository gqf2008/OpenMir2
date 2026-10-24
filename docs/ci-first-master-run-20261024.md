# master 首次 CI 实跑记录（2026-10-24，卡 `openmir2-ci-first-master-run`）

- 卡：`openmir2-ci-first-master-run`（tier=L，只跑与记录）。
- 快照戳：`@ fd2db51e9 @ 2026-10-24 12:34 +08`（master；CI 全部 11 条都钉在这个 tip 上）。
- 本卡零配置变更：`.walgit/ci.toml`、任何代码均未改；CI 结果也未接自动合并（红线）。

## 1. 怎么跑的

```sh
KEY="$(git rev-parse --git-dir)/walgit/keys/openmir2-worker-24.ed25519"
walgit ci run --once --repo . --remote walgit --actor openmir2-worker-24 --key "$KEY"
```

- `--key` 传的是**identity 里的密钥文件路径**（帮助文本写 hex，实现按文件读——CI 卡已登记该坑）。
- `--remote walgit`：默认值是 `origin`（GitHub 镜像），不显式给会轮询错远端。
- 执行时本 worktree 的 HEAD 就是 `refs/heads/master` tip（`fd2db51e9`），任务命令（`cargo`/`python`）
  在此检出上执行；runner 自己把每条结果钉到对应 tip（`refs/heads/master @ fd2db51e`）。

## 2. 11 条任务（全部 attempt 1、success、签名者 openmir2-worker-24）

| 任务 | run id | 耗时 | 结果 |
|---|---|---:|---|
| fast-t-reservation | `ci-e592a8f5150227cc` | 88 ms | success |
| fast-conflict-marker | `ci-c46f2d683ac1ce38` | 27.3 s | success |
| fast-stale-pointer | `ci-60cdab813de6e882` | 3.3 s | success |
| fast-whitelist-anchor | `ci-71e9918f6ca9c94a` | 1.8 s | success |
| fast-m3-registry | `ci-f72543110c23dc85` | 132 ms | success |
| fast-dynamic-count-stamp | `ci-a71535d0fd83730a` | 1.2 s | success |
| fast-board-coverage | `ci-0ab1f1d26aaaf186` | 1.8 s | success |
| rust-fmt | `ci-aac328ebb741e4b8` | 4.3 s | success |
| rust-clippy | `ci-e7a113a072cce23a` | 58.9 s | success |
| rust-test | `ci-05a4b0a664dcb221` | 237.5 s | success |
| rust-msgcodegen | `ci-abd3e1d844b6ea63` | 5.4 s | success |

`walgit ci status --repo .`：master tip 上 **11/11 done / success**（另有历史 run：`93ce7503`、
`c5cc093c`、`aecf95c9` 三个 tip 的 11 条，是 CI 卡交付时的分支验证，不属于本卡）。

## 3. 日志摘要（rust-test 与两条新门 + 其余 fast 门）

- **rust-test**（`ci-05a4b0a664dcb221`）：`test result: ok` × **141** 个套件，累计 **3167 passed**，
  全部 `0 failed`；末尾 Doc-tests 亦绿。
- **fast-dynamic-count-stamp**（`ci-a71535d0fd83730a`）：`dynamic-count gate: OK（0 缺戳；6 条段落声明
  降级 INFO；扫描 146 个 markdown，path=docs，max-gap=24）`。
- **fast-board-coverage**（`ci-0ab1f1d26aaaf186`）：`board-coverage gate: OK (0 hidden of 548 card
  threads; board shows 548 ids; 71 non-card threads excluded; 1.73s)`。
- fast-stale-pointer：`OK (146 files / 35834 lines …; 0 card-status hit(s), 10 card id(s) judged,
  21 card-bearing line(s); ref=fd2db51e9…)`。
- fast-conflict-marker：`OK (0 markers across 7471 tracked files)`。
- fast-t-reservation：`GREEN（①-⑤ 与 ⓪ 全过：预约表 255 行 / whitelist 255 号）`。
- fast-whitelist-anchor：`RESULT: ALL PASS（590 条锚 …）`。
- fast-m3-registry：`WHITELIST_REGISTRY GREEN（无重复编号；夹具 token 解析唯一或不可用）`。

## 4. 与本地门禁的对照（③）

本地在**同一棵树**（HEAD=`fd2db51e9`，工作树干净）上逐条复跑：

| 门 | CI（@ fd2db51e9） | 本地 | 一致？ |
|---|---|---|---|
| stale_pointer | OK 146 files / 35834 lines / 0 card-status hit | OK 146 files / 35834 lines / 0 card-status hit | ✅ 逐字一致 |
| dynamic_count | OK 0 缺戳 / 146 md / 6 条段落 INFO | OK 0 缺戳 / 146 md / 6 条段落 INFO | ✅ 逐字一致 |
| board_coverage | 0 hidden / 548 threads | 0 hidden / **549** threads | ✅ 结论一致；线程数差 1 是**看板在线漂移**（两次执行之间又有新卡），非树差异 |
| t_reservation | GREEN 255/255 | GREEN 255/255 | ✅ |
| conflict_marker | OK 0 / 7471 | OK 0 / 7471 | ✅ |
| whitelist_anchor | ALL PASS 590 | ALL PASS 590 | ✅ |
| m3_registry | GREEN | GREEN | ✅ |
| rust-fmt / clippy / msgcodegen | success | success（fmt/msgcodegen 复跑；clippy 以 CI 为证） | ✅ |
| rust-test | 3167 passed / 0 failed | 未本地重跑（CI 即证据，237s） | — |

两点说明：① 本地复跑时 `refs/remotes/walgit/master` 已前进到 `e590e1676`（label-fix 合入），
stale_pointer 的 `--ref` 因此显示新 ref，但**扫描的树与命中数**与 CI 完全一致（工作树未变）；
② board_coverage 的线程数是**在线量**，引用必须带时点——本卡两处 548/549 就是同一小时内的前后差异。

## 5. 门禁（本卡自身的本地门禁）

```text
stale_pointer_gate   rc=0（147 files；本卡新增本文件后）
dynamic_count_gate   rc=0（147 markdown）
board_coverage_gate  rc=0（0 hidden of 549）
t_reservation_gate   GREEN（255/255）
conflict_marker_gate OK（0/7472）
git diff --check     rc=0；本文件 CRLF=0、无 BOM
```

## 6. 残留

- 未改 `.walgit/ci.toml`、任何代码、任何卡状态；未接自动合并；未起栈、未占端口、未连库。
- CI 的 rust-test 约 237s：与 CI 卡登记的 283s 同量级（机器差异）；schedule 触发的重负载提醒
  （ci-gate 卡 carried #126）仍然有效。
