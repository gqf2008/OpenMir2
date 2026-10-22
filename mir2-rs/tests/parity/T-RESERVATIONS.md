# `T-*` 编号预约表（tests/parity）

> **唯一取号入口**：`python -I tools/whitelist/next_t.py --card <卡 id>`
> **合并前门禁**：`python -I tools/whitelist/t_reservation_gate.py check`（见 `docs/leader-queue-20261023.md`）
> 背景：2026-10-23 一天撞号 4 次——并行车道各在**开工时** grep 一次「当前最大号」，
> 并行时必然读到旧值。从本表起：**先预约、再登记**，不要再用 `grep` 现算最大号。

## 规则

1. **写任何 `T-<n>` 之前先取号**：`python -I tools/whitelist/next_t.py --card <卡 id>`。
   它在锁内读本表 + `whitelist.md` +（若可达）`walgit/master` 的同名文件，返回
   `max(已占用号) + 1` 并**原子地**追加一行「在用」。同 card 重复调用幂等（不会烧号）。
2. **号只增不减**：`已释放` 的号不回收——历史 commit / 文档里可能已有引用，回收即制造歧义。
3. **落地后改状态**：登记行随卡片合并进 `whitelist.md` 后，把本表该行状态改成 `已落地`。
4. **撤销**：放弃某号时把状态改 `已释放`，**不要删行**（删行会让门禁把「whitelist 有登记、
   预约表没有」判红）。
5. **不追溯、不重排**：本表首次落地时，`whitelist.md` 已存在的 186 条历史登记由
   `next_t.py --bootstrap` 一次性批量登记为 `已落地`（卡 id = `historical-pre-reservation`），
   不逐条补卡片归属、不改历史编号语义。

## 三向门禁（`t_reservation_gate.py`）

| 规则 | 检查 | 变红意味着 |
|---|---|---|
| ① | `whitelist.md` 的每个 `T-<n>` 在预约表里有「在用」或「已落地」行 | 有人绕过取号器直接写了号 |
| ② | 同一号不得有两个「在用」认领，也不得同号多行 | 两条车道拿到了同一个号 |
| ③ | 预约表的「已落地」号必须真的在 `whitelist.md` 出现 | 登记了却没落地，号被占死 |

另有 ⓪：预约表里以 `| T-` 开头但解析不成四列的行 = 坏行（fail-closed）。
不可读 / 缺文件 ⇒ rc=2。牙齿（`teeth`）在真实表的副本上做三种破坏，必须变红且逐字节还原。

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
| T-195 | historical-pre-reservation | 2026-10-23T00:00:00Z | 已落地 |
