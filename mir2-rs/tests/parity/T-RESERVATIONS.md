# `T-*` 编号预约表（tests/parity）

> **唯一取号入口**：`python -I tools/whitelist/next_t.py --card <卡 id>`
> **合并前门禁**：`python -I tools/whitelist/t_reservation_gate.py check`（**在合并结果树上跑**，
> 见 `docs/leader-queue-20261023.md` 第 9 条）
> 背景：2026-10-23 一天撞号 4 次——并行车道各在**开工时** grep 一次「当前最大号」，
> 并行时必然读到旧值。从本表起：**先预约、再登记**，不要再用 `grep` 现算最大号。

## 规则

1. **写任何 `T-<n>` 之前先取号**：`python -I tools/whitelist/next_t.py --card <卡 id>`。
   它在一把跨进程锁内读「共享 journal + 本表 + `whitelist.md` + 远端**当前** `walgit/master`」，
   返回 `max(已占用号) + 1` 并**原子地**追加一行「在用」。同 card 重复调用幂等（不会烧号）。
2. **跨 worktree 真互斥靠共享 journal**：认领先写进
   `$(git rev-parse --git-common-dir)/walgit/t-reservations.journal`（所有 worktree 共用该目录），
   再写本表。journal 不进仓库、不影响 diff；崩溃最多浪费一个号，不会重号。
3. **远端新鲜度 fail-closed**：取号时 `git ls-remote` 比对 `walgit/master`，落后就 `git fetch`
   后再算；**远端不可达 ⇒ 拒绝发号（rc≠0）**，除非显式 `--allow-stale`（大声告警）。
4. **号只增不减**：`已释放` 的号不回收——历史 commit / 文档里可能已有引用，回收即制造歧义。
5. **落地后改状态**：登记行随卡片合并进 `whitelist.md` 后，把本表该行状态改成 `已落地`。
6. **撤销 / 换号**：放弃某号改成 `已释放`，**不要删行**（删行会让 ① 判红）；换号用
   `--force-new`（脚本会把该卡旧「在用」行自动改 `已释放`，不会留下同卡两个「在用」）。
7. **不追溯、不重排**：本表首次落地时，`whitelist.md` 已存在的 186 条历史登记由
   `next_t.py --bootstrap` 一次性批量登记为 `已落地`（卡 id = `historical-pre-reservation`），
   不逐条补卡片归属、不改历史编号语义。
8. **扫描口径**：只认**行首** `| T-<n> |` 的登记行；正文里的 `T-<n>` 引用（如「见 T-154 残余④」）
   **不查**——否则纯引用会假红。``` 围栏代码块里的行不算登记行；**围栏不成对 ⇒ 门禁红 / 取号器拒绝
   发号**（否则围栏后的号会被静默跳过）。

## 门禁（`t_reservation_gate.py check`，任一不满足 ⇒ rc=1 并点名）

| 规则 | 检查 | 变红意味着 |
|---|---|---|
| ① | `whitelist.md` 的每个 `T-<n>`（行首登记行）在预约表有「在用」或「已落地」行 | 有人绕过取号器直接写了号 |
| ② | 同一号不得有两个「在用」认领，也不得同号多行 | 两条车道拿到了同一个号 |
| ③ | 预约表的「已落地」号必须真的在 `whitelist.md` 出现 | 登记了却没落地，号被占死 |
| ④ | `whitelist.md` 内部同一号出现多行 | 表内撞号（T-4 那一类） |
| ⑤ | 同一卡 id 不得有多个「在用」认领 | `--force-new` 没释放旧行 |
| ⓪ | 坏行 / 未闭合 ``` 围栏 | 输入坏了，会静默漏号 |

缺文件 / 空表 / 空 whitelist ⇒ rc=2（fail-closed）。
牙齿（`teeth`）在真实表副本上做五种破坏，必须变红且逐字节还原 + sha256。

**注意**：在**自己的分支 worktree** 上跑永远是绿的（只见到自己的号）——必须在**合并结果树**上跑。

## 预约表

| 号 | 卡 id | 认领时间 | 状态 |
| --- | --- | --- | --- |
| T-1 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-2 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-3 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-4 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-5 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-6 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-7 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-8 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-9 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-10 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-11 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-12 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-13 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-14 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-15 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-16 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-17 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-18 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-19 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-20 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-21 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-22 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-23 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-24 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-25 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-26 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-27 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-28 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-29 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-30 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-31 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-32 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-33 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-34 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-35 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-36 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-37 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-38 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-39 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-40 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-41 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-42 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-43 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-44 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-45 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-46 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-47 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-49 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-50 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-51 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-52 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-53 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-54 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-55 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-56 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-57 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-58 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-59 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-60 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-61 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-62 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-63 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-64 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-65 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-66 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-67 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-68 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-69 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-70 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-71 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-72 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-73 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-75 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-76 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-77 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-78 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-79 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-80 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-81 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-82 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-83 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-84 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-85 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-86 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-87 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-88 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-89 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-90 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-91 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-92 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-93 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-94 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-95 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-96 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-97 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-98 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-99 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-100 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-101 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-102 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-103 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-104 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-105 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-106 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-107 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-108 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-109 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-110 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-111 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-112 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-113 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-114 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-115 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-116 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-117 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-118 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-119 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-120 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-125 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-126 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-127 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-128 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-129 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-130 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-131 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-132 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-133 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-134 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-135 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-137 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-138 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-139 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-140 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-141 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-142 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-143 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-144 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-145 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-146 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-147 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-148 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-149 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-150 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-151 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-152 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-153 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-154 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-155 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-156 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-157 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-158 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-159 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-160 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-161 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-162 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-163 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-164 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-165 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-166 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-167 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-168 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-169 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-170 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-171 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-172 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-173 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-174 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-175 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-176 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-177 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-178 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-179 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-180 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-181 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-182 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-183 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-184 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-186 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-187 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-188 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-189 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-190 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-191 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-192 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-193 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-194 | openmir2-menu-ok-unify | 2026-10-23T08:25:00Z | 已落地 |
| T-195 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
| T-196 | openmir2-gm-admin-mode-entity-seam | 2026-10-23T08:45:00Z | 已落地 |
| T-199 | openmir2-weapon-durability-frames | 2026-10-23T00:16:58Z | 在用 |
| T-201 | openmir2-cm-adjust-bonus | 2026-10-23T00:21:36Z | 在用 |
| T-202 | openmir2-sm-reconnect-wiring | 2026-10-23T00:23:10Z | 在用 |
| T-203 | openmir2-ybdeal-frames | 2026-10-23T00:35:09Z | 已落地 |
| T-204 | openmir2-sendrefmsg-range-config | 2026-10-23T00:45:07Z | 已落地 |
| T-205 | openmir2-map-event-frames | 2026-10-23T01:02:40Z | 已落地 |
| T-206 | openmir2-sm-skeleton-wiring | 2026-10-23T04:00:16Z | 在用 |
| T-207 | openmir2-pickwire-remaining-frames | 2026-10-23T04:07:57Z | 在用 |
| T-208 | openmir2-frame-drain-order-audit | 2026-10-23T05:23:46Z | 在用 |
| T-209 | openmir2-gm-impl-shutup | 2026-10-23T05:36:07Z | 在用 |
| T-210 | openmir2-gm-script-host-seam | 2026-10-23T06:18:40Z | 在用 |
| T-211 | openmir2-gm-script-exec-seams | 2026-10-23T07:31:30Z | 在用 |
| T-212 | openmir2-gm-impl-shutup-release | 2026-10-23T08:17:00Z | 在用 |
| T-215 | openmir2-gm-impl-showeffect | 2026-10-23T08:37:07Z | 在用 |
| T-219 | openmir2-gm-impl-startquest | 2026-10-23T09:19:34Z | 在用 |
| T-222 | openmir2-script-dyn-var-host | 2026-10-23T09:56:25Z | 在用 |
