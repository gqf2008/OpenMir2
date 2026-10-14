# m2svr-o1-closeout-20261014 — reviewer-5 O-1 收口（非阻塞 Minor）

卡：`openmir2-client-e2e-enter-gaps`（分支 `enter-gaps-window-dense-menu`，作者 `openmir2-worker-11`）。
来源：walgit review `a1b16f3d`（`openmir2-reviewer-5` @ `reviewed_head aff1f7c17`）O-1。
分级：`tier=L`（纯证据叙述订正 + 一个静态文本门读数，零 `.rs`/协议/存储/出帧/模拟改动；断网可判红/绿）。

## 改了什么

| 文件（`tools/m2-acceptance/evidence/m2svr-client-ability-layout-plumb-20261014/`） | 改前 | 改后 |
| --- | --- | --- |
| `07-four-gates-closeout.txt` 末行 | `GREEN 8/8（写完清单后复跑，见下）` | `GREEN 9/9 rc=0（写完 06/07/08 + 清单后复跑）` |
| `08-boundary.txt` 全表 | 6 文件**中间态**快照（写 08 自身之前） | 10 路径**最终态**（`base 567c4e390 → head 0c3561629`） |
| `SHA256SUMS.txt` | 随 07/08 变化的 2 行 hash 过期 | 重录（9 行，repo 口径） |

未改：`README.md` / `check_client_ability_layout_doc_counts.py` / `check_client_ability_layout_plumb.ps1` / `02` / `03` / `06`。

## 证据件

- `01-o1-closeout.txt` — 来源锚 + 改前后 + 未改范围。
- `02-four-gates.txt` — 本片四门禁（cwd `mir2-rs`，`CARGO_TARGET_DIR=E:/tmp/m2svr-target`）。
- `03-verify-and-docgate.txt` — 修复后 `verify_sha256sums.py`（9/9）+ `eolcheck` + 文档漂移门。
- `04-boundary.txt` — 边界（base `0c3561629`）+ 冻结面空 + `.rs` NONE。
- `SHA256SUMS.txt` — 本包 hash（repo 口径）。

## 归属

本片**不据此收卡**：卡级剩余 DoD 仍是 root 独占窗口的 R 真机判据（①②压血进图 HUD/`CM_RUN` +
③两侧同点对照），按 `.../client-e2e-enter-gaps-20261013/82-two-side-window-runbook.md §2b` 复跑。
