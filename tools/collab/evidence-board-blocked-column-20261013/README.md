# 证据包：`openmir2-collab-board-blocked-column` —— board.toml【受阻】列死列

- **卡**：`openmir2-collab-board-blocked-column`（issue `03a0260c`，作者 openmir2-reviewer-5，发现者 root）
- **改动**：`.walgit/board.toml` 第 5 个 `[[column]]`（【受阻】）谓词 `merge = "blocked"` → `status = "blocked"`（**一行**）
- **口径依据**：host `SKILL.md` — `status` 取值含 `blocked`（`in-progress / needs-review / blocked / needs-human / done / closed`）；
  看板投影字段 = `status` / `owner` / `worktree` / `branch` / `work`（`merge_result {"merged":true}` 另落 `merged`），
  **`merge` 不是投影字段** ⇒ 旧谓词永不命中 ⇒ 死列。
- **基线**：`origin/master` = `c8a3487b1f6a117065b0105836105b7ed3c0529b`；分支 `collab-board-blocked-column`。
- **预览机制**：`walgit collab board --board <path>` 只对**工作树里的** board 定义做投影，**不改 refs**（用于改前/改后对照）。

## 判据（逐条对应卡 DoD ①–④）

| 卡 DoD | 结果 | 证据 |
| --- | --- | --- |
| ① 改前复现：任一卡 `status=blocked` ⇒ 从**所有**列消失 | 本卡实测：SUM 131 → **130**，【受阻】=0，本卡不在任何列 | `02-board-blocked-head-board.json` |
| ② 改后：同一卡出现在【受阻】 | SUM **131**，【受阻】=**1**，本卡在【受阻】 | `03-board-blocked-fixed-board.json` |
| ③ 只需 `{"status":"blocked"}`；带 `merge` 不生效 | 复现条目 body 仅 `{"status":"blocked"}`；改坏成 `merge` 即再次消失 | `05-commands.txt`、`04-board-mutation-red.json` |
| ④ 全列计数核对：各列计数之和 = 基线 | 修后六列 = `2/36/5/0/1/87`（SUM=**131** = 基线 `3/36/5/0/0/87` 之和）；改坏 = **130** | `01`、`03`、`04` |

**集合校验**（`python`，见 `05-commands.txt` §D）：

```
ids(01)==ids(03) == True                 # 修后可见集合 == 认领前基线集合（无回归）
ids(02)==ids(01)-{openmir2-collab-board-blocked-column}   # 未修：正好吞掉本卡
ids(04)==ids(02)                         # 改坏复现同一现象
```

> **DoD④ 精确核对（顶层线程数对账）**：`refs/collab/inbox/**` 顶层 distinct thread id = **133**；修后看板六列之和 = **131**。
> 差额 **2** = `openmir2-m2-rust-server-state-dump`、`openmir2-m2-rust-state-log-plumb` —— 它们的
> 最新 `merge_result {"merged":true}` 之后**没有** `status: closed` ⇒ 投影状态为 `merged`，而 `board.toml`
> **没有承载 `merged` 的列** ⇒ 同样不显示。**这与本卡【受阻】谓词无关**（是另一独立现象：无列承载 `merged`）。
> ⇒ 本卡 DoD④ 以「修正后可见集合 == 修正前基线集合」（`ids(01)==ids(03)`，131==131，无卡因受阻谓词被吞）为真源；
> `merged` 无列一条**如实登记、不夹带**，建议 owner 另开一行/一卡处置。

## 文件

| 文件 | 内容 |
| --- | --- |
| `01-board-before.json` | 认领前（HEAD board）投影基线：`待认领 3 / 进行中 36 / 待审 5 / 需要人 0 / 受阻 0 / 已完成 87`，SUM=131 |
| `02-board-blocked-head-board.json` | 本卡写 `{"status":"blocked"}` 后，**未修** HEAD board 投影：SUM=130，本卡**不在任何列** |
| `03-board-blocked-fixed-board.json` | 同 refs，**修后** board（`--board .walgit/board.toml`）：SUM=131，【受阻】=1 |
| `04-board-mutation-red.json` | **改坏必红**：把修后 board 的 `status` 改回 `merge` 预览 ⇒ SUM=130，本卡再次消失 |
| `05-commands.txt` | 复现命令 + 原始读数 + 集合校验输出 |
| `06-diff.txt` | `git diff`（board.toml 一行 + 本证据目录） |

## 边界 / 纪律

- **零 Rust / 零 `src/**` / 零 `sql/**` / 零 `MirClient`**；未起栈、未占端口（本片全程离线）。
- 本片对看板的唯一写入 = 三条签名 `status`（claim `in-progress` / 复现 `blocked` / 交付 `needs-review`）；
  `--board` 预览不改任何 ref。
- 四门禁（隔离 `CARGO_TARGET_DIR`）在合并树复跑见 `07-gates.txt`。
