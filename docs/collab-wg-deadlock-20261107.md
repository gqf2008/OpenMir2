# wg 聚合死锁：现象、根因、最小复现与处置（2026-11-07）

> 卡：`openmir2-collab-interface-wg-migration`（A1/A2）。
> 结论先写：**读路径固定用 `tools/orchestration/collab_read.py`，写路径用 `wg collab entry`**；
> 退役条件见 §5。本文是可复核的事实记录，不是推测。

## 1. 现象

`wg` 0.1.0 的读命令在本仓库（`refs/collab/inbox/*` 共 14,529 条）上一律失败或挂住：

| 命令 | 实际结果 |
| --- | --- |
| `wg collab board --repo .` | 约 2 分钟后 `wg: collab board: GitFailed` |
| `wg collab report --repo .` | 同上 |
| `wg collab ls --repo .` | 同上 |
| `wg mcp --repo . --remote walgit` → `collab_board` | `collaboration aggregation failed: GitFailed` |
| 同上 + `--no-fetch` + `fresh:false` | 仍然同样报错 ⇒ **失败在聚合，不是 fetch** |
| `wg collab entry …`（写） | 正常，能签名并推送 |
| 同一台机器、8–20 条 ref 的小夹具 | `wg collab board` **rc=0，正常返回** |

> 所以这不是「wg 完全不可用」，而是**规模阈值**：小夹具上 wg 可作 oracle，
> 大仓库上才失效。A2 因此是可判的——判据是「两条读路径的**投影**是否一致」，
> 不是「能不能跑」。

## 2. 证据链

1. **trace2**：`GIT_TRACE2_EVENT=<file> wg collab report --repo .` 显示 wg 依次起
   `for-each-ref` → `cat-file --batch` → `rev-parse` → `for-each-ref refs/collab/meta/snapshot`
   → `cat-file blob` → `for-each-ref refs/collab/inbox` → **`cat-file --batch` 只有 start、没有 exit**。
2. **排除坏数据**：全量 14,529 条 inbox 条目逐条 JSON 解析通过，`version/kind/id/actor/ts/sig` 齐全；
   分布 `status 4885 / comment 3180 / patch 2025 / review 1915 / merge_result 1303 / issue 979 / card_claim 92 / ci_* 150`。
3. **排除坏对象**：`git cat-file --batch-check` 对全部 14,529 个 ref 目标返回 blob，0 missing。
4. **排除 fetch**：`git fetch --prune walgit '+refs/collab/*:refs/collab/*'` rc=0，2.3 秒完成；
   `git ls-remote walgit 'refs/collab/*'` rc=0，1.9 秒。
5. **排除检出状态**：把同样 14,687 条 collab ref 复制到 `/tmp` 的干净仓库，`wg collab report` 同样 `GitFailed`。
6. **对照实验**：把 `git cat-file --batch` 的全部 oid **先写完再读**，进程会停在写端、不再前进；
   改成**分块写读**则 0.5 秒返回全部对象。差别只在写入方式。

## 3. 根因

wg 的批量对象读取把**全部 oid 一次写入子进程 stdin，然后才开始读 stdout**。
`14,529 × 41B ≈ 595KB`，远超管道缓冲（Windows 约 64KB）；git 的 stdout 写满后停止读 stdin，
wg 的 stdin 写满后停止读 stdout ⇒ 双向互锁，子进程永不退出，wg 超时后报 `GitFailed`。

按此推算，阈值约 `64KB / 41B ≈ 1,600` 条 inbox ref；本仓库远在阈值之上。

## 4. 最小复现

任意 git 仓库，造 2,000 条 `refs/collab/inbox/*`（指向同一个 blob），然后对照两条读路径：

```sh
mkdir -p /tmp/wg-repro && cd /tmp/wg-repro && git init -q .
printf '{"version":1,"kind":"comment","id":"x","actor":"a","ts":1,"sig":"e"}' > e.json
OID=$(git hash-object -w e.json)
for i in $(seq 1 2000); do echo "create refs/collab/inbox/a/$i $OID"; done \
  | git update-ref --stdin
mkdir -p .walgit
printf '[[column]]\nname = "全部"\n' > .walgit/board.toml
git add -A && git -c user.email=a@b -c user.name=a commit -qm init

timeout 60 wg collab board --repo .          # 期望：挂到超时 / GitFailed
python -I -B <repo>/tools/orchestration/collab_read.py board --repo .   # 期望：秒级返回
```

**2026-11-07 实测**（同一台机器、同一棵树）：2,000 条 ref 时 `wg collab board` 直接返回
`wg: collab board: GitFailed`；`collab_read board` **0.28 秒**返回同一批 2,000 条。

说明：条数降到千余以下时 wg 可能恢复正常——这正是「阈值 + 互锁」而不是「数据坏了」的判据。

## 5. 处置与退役条件

**处置**（已落地，`e4d97a8dc` 起由守牙齿强制）：

- **读**（`board` / `thread` / `report` / `ls` / `thread-heads` / `pr`）一律走 `collab_read.py`；
  守牙齿在 shell / markdown / AST 三个扫描域上判红。
- **写**（`entry` / `join` / `claim` / `principal-*` / `watch`）保留 `wg collab`。
- 兜底封装 `tools/orchestration/walgit_collab.py` 把「子命令不存在」与「聚合 GitFailed」都判 **rc=3**，
  防止调用方把失败读成空看板。

**退役条件**（全部满足才把 `collab_read` 降级为对账工具）：

1. `wg collab board --repo .` rc=0 且 **< 30 秒**（当前约 2 分钟后失败）；
2. 连续 3 次，wg 的卡片集合与 `collab_read board --json` 的卡片集合**逐 id 相同**；
3. `wg collab watch --once` 能在本仓库返回（当前同样不可用）。

任一条不满足 ⇒ 保持现状。**不要在未满足时把读路径切回 wg**——那等于把「秒级」换成「两分钟失败」。

## 5.1 投影一致性的实测（A2）

修 F1 之前，`collab_read` 与 wg 在 13 张卡的差分夹具上有 3 张错列
（`merge_result` 折叠、未知状态回落、CI-only 隐藏）。修完之后 **13 张卡逐 id、逐列完全一致**，
真实仓库的 `已合并未收口` 也从 0 变成 22、卡片数从 1,055 收敛到 980（+75 个非卡片线程隐藏）。

这条正是 A2 要的「两条路径结论一致」——它**可以判**，此前「两条都不可用所以无法比较」
的表述是错的：wg 在小规模下可用，只是不能在本仓库规模上跑。

## 6. 已知未验证

- `wg collab board` 是否接受 `--format json` 与 `--board <path>`：聚合故障期间无法实证。
  `collab_read` 已自行实现 `--board` 覆盖，不依赖该参数。
- 上游修复进度未知；本仓库不阻塞在它上面。
