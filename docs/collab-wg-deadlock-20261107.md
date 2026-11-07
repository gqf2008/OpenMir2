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
（`merge_result` 折叠、未知状态回落、CI-only 隐藏）。

**这里改错过四次，都记录在案**：

1. 第一版按 `(ts, oid)` 定序 ⇒ 真实仓库 22 张同 ts 的卡被判成 `已合并未收口`，我还当成了成功证据；
2. 第二版改成 `ts` 主键 + 链序破平局，仍不对——它只看链尾。最小反例
   `issue(1) → in-progress(50) → closed(40)`：wg 判 **closed**，全局最大 ts 会给 `in-progress`；
3. 第三版改成「所有 leaf state 取最大 ts」，还是不对——它漏了「**最新 state 决定走哪条支**」。
   反例：`issue1 → in-progress10 → [closed40, in-progress60 → needs-review30]`，wg 判 **待审**
   （走 in-progress60 这支的终端），「所有 leaf 取最大 ts」却给旁支的 closed40。

**第四版（最新 state → 子树终端 + tie-break）也不对**：它只是线性线程的近似。反例是 sibling fork
（`issue → 三个同级 status`）与随机 DAG——reviewer 的 DAG fuzz 在 155 张卡上错 **75** 张。

**真正的规则来自 wg 源码，不是夹具反推**（`E:/UserTemp/walgit-d1/wg/src/d1/collab.zig`）：

- **线程顺序 `thread()`（§6.2）**：先按 `(ts, actor, oid)` 升序；再**分层**发出——父为空 / 父不在
  线程内 / 父已发出的条目本轮发出，否则留到下一轮；某一轮没有进展（畸形输入）则把剩余按已排序
  顺序追加。父子分层**优先于 ts**：子节点 ts 更小也排在父之后。
- **状态 `card_status()`**：按该顺序扫一遍，最后命中的 `status` 或 `merge_result{merged 真}` 胜，
  默认 `open`。**不是最大 ts、不是叶节点、不需要任何 tie-break**——此前三版都是在猜这条简单规则。
- **owner**：同一顺序回放 `status` 条目，字符串覆盖、`null`/缺失不动、空串是有效值。
- **认领覆盖（§8.4）**：状态为 `open`/`in-progress` 且存在 live claim ⇒ 状态变 `in-progress`、
  owner 变 claim 的 actor。claim 只收 parent 为空、ttl ∈ [1, 86400] 的，按 `(ts, actor, oid)` 排序取
  **最早**的一个 live 认领（不是最新）。⚠ collab_read 不验签，wg 要求 `is_verified`，这是已知差异。
- **显示过滤**：只有 `ci_claim`/`ci_result`/`card_claim` 条目的线程不显示；命中不了任何列的线程不显示。
- **畸形 body**：不是 JSON 对象的 body 一律当空对象（源码 `body_str`/`body_bool`/`optional_body_str` 的行为），
  否则一条畸形 entry 就会让整个看板崩掉。

按源码实现后：reviewer 的 **34 组夹具 / 140 张卡**对 wg 只有 **2 处 mismatch，且两处都是未签名 claim**
（wg 要求 `is_verified` 会忽略，本工具不验签所以应用）。其余形状——DAG fuzz、同 ts、fork、owner、
claim 边界、类型严格性、畸形 body——全部一致。

这条正是 A2 要的「两条路径结论一致」。前几轮「两条都不可用所以无法比较」的表述是错的，
而「用夹具反推规则」这条路也证明不可靠——最后靠的是**读上游源码**。

## 6. 已知未验证

- `wg collab board` 是否接受 `--format json` 与 `--board <path>`：聚合故障期间无法实证。
  `collab_read` 已自行实现 `--board` 覆盖，不依赖该参数。
- 上游修复进度未知；本仓库不阻塞在它上面。
