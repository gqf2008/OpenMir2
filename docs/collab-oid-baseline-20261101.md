# 历史坏 parent 逐条豁免登记（2026-11-01）

卡：`openmir2-oid-baseline-night-report-parent`（tier=L）。只改**数据文件**（豁免清单）与本文档，
不动 `oid_gate.py` 判定逻辑、不回写任何 collab 历史条目 / inbox ref。

## 1. 事实（1 条）

| 项 | 值 |
|---|---|
| principal | `openmir2-orchestrator` |
| thread | `openmir2-night-report-morning-append` |
| kind | `merge_result` |
| inbox ref | `refs/collab/inbox/openmir2-orchestrator/64b5d7a631b406fa6715b67275a5a3b4` |
| 条目 blob | `d1e7214d89a026aee5746a8739e0f86e154ccee2` |
| 字段 | `parent` |
| 坏值 | `2113986cd0e8e1b9c1e3a8ac7a3f0d8b3e6c8c7a`（对象不存在，`git cat-file -t` 失败） |
| 正确值 | `2113986c0a89…`（当时被手打成形近的臆造 40-hex） |

来源：2026-11-01 Leader 签 `merge_result` 时手打了形近 oid 当 `--parent`。历史条目是**签名事实**，
不回写；用另一条更正条目取代后，坏条目本身的 parent 依旧解析不到 ⇒ 按既有机制登记**逐条豁免**。

## 2. 处置

在 `tools/collab/oid_gate_baseline.json` 的 `entries` 末尾**新增恰好一条**（9 → 10 条），
`git diff --numstat` 为 `11 0`（纯新增，既有条目一字未改）：

- 豁免键 = `(principal, thread, entry_oid, field, value)` = 上表五项；
- `reason` / `added_at=2026-11-01` / `added_by=openmir2-dev-2` 齐备；
- `load_baseline` 的校验：`entry_oid` 必须是 40 位小写 hex blob oid，`value` 必须是 7–40 位 hex，
  **不许通配**（`*` / `…` / `...`）、不许在必需字段上留空 —— 本条目按此写。

## 3. 为什么不是通配

`load_baseline` 对通配/空值/格式直接抛 `ValueError`（rc=1），而且豁免键包含 `entry_oid`：
**同 principal、同 thread、同字段、同值，但条目不同 ⇒ 不命中豁免，仍判红**。因此这条豁免只覆盖
`d1e7214d…` 这一条历史条目，不构成对 `parent` 字段（或 orchestrator 所有条目）的整体放过。

## 4. 命令与 rc（改动前 / 改动后）

| # | 命令 | 改动前 | 改动后 |
|---|---|---|---|
| 1 | `python -I tools/collab/oid_gate.py check --limit 400`（**不带** `--baseline`；**该行是 2026-11-01 当时状态**，见下方 as-of 注） | rc=1 | rc=1 |
| 2 | `python -I tools/collab/oid_gate.py check --limit 400 --baseline tools/collab/oid_gate_baseline.json` | rc=1，未豁免 | **rc=0** |
| 3 | `python -I tools/collab/oid_gate.py gate --baseline tools/collab/oid_gate_baseline.json`（`tool_selftest_gate` 的实际调用形态） | rc=1 | **rc=0** |
| 4 | `python tools/hygiene/tool_selftest_gate.py --inventory` | rc=0 | rc=0 |
| 5 | `python tools/hygiene/tool_selftest_gate.py`（全量） | **rc=2**，`FAIL oid_gate rc=1` | **rc=0**，`TOTAL 59 项：绿 59 / 红 0` |
| 6 | `python -I tools/collab/oid_gate.py check --audit-baseline --limit 400` | — | rc=0，`AUDIT-OK 10 条` |

**关于第 1 行的说明（重要，别误读成没修好）**：`main()` 只在**显式传 `--baseline`** 时才
`load_baseline`；不传 ⇒ 豁免清单根本不在判定里（`已豁免 0`）。所以卡面 A2 字面写的
`check --limit 400` 在加 baseline **之前和之后都是 rc=1** —— 这是工具的既定语义，不是本条豁免没生效。
真正会被 goal_gate 条件④触发的是 `tool_selftest_gate`，它调用的是 `gate --baseline …`（第 3/5 行），
改动后已 rc=0。想让「裸 check」也绿，需要改 `oid_gate.py` 的默认加载逻辑 —— 本卡禁止改该文件，
故**不在本卡范围**，作为发现登记。

**as-of 注（2026-11-01 之后补，不改写历史数字）**：上表第 1 行的 `rc=1` 是 **2026-11-01 当时**
（`81c8fd6a2` 之前）的快照。`81c8fd6a2` 起 `check` / `gate` 在**不传 `--baseline`** 时
会缺省加载仓库内 `tools/collab/oid_gate_baseline.json`，因此裸 `check` 现在 **rc=0**；
只有显式 `--no-baseline` 才恢复严格判定、回到 **rc=1**。上表「改动前 / 改动后」两列（第 3/4 列）
保持当时事实，**不改写**。

改动后第 2 行关键输出（原文）：

```
PASS （库内共 13747 条；范围 最近 400 条，实扫 400 条）豁免 1 处（逐条）：豁免 openmir2-orchestrator / openmir2-night-report-morning-append / 条目 d1e7214d8 / parent=2113986cd0e8e1b9c1e3a8ac7a3f0d8b3e6c8c7a || 扫描 400 条条目 / 758 个 oid：可解析 757（其中短 oid 13）、非 commit 跳过 0、空值 0、解析不到 1（其中已豁免 1）、省略号缩写跳过 0、说明性片段跳过 0；baseline 共 10 条（命中 1，未命中 9）
```

口径：`解析不到 1（其中已豁免 1）` ⇒ **未豁免 0**；`baseline 共 10 条（命中 1，未命中 9）` 里的
「未命中 9」是**本扫描窗口（最近 400 条）没用到**的旧豁免条目（它们的坏条目更早），不是坏条目。

## 5. 负例对照（豁免精确到条目 ⇔ 换一个键就重新判红）

副本放在 `E:/tmp/dev2-oid-neg/`（**未提交**），用 `--baseline` 指向副本跑同一份真仓库 check，
每次只改新增条目的一个键：

| 变异 | rc | 结果 |
|---|---|---|
| `entry_oid` → `0123456789abcdef0123456789abcdef01234567` | 1 | 命中 0 / 未命中 10，坏条目重新点名 |
| `value` 末位 a→b（`…3e6c8c7b`） | 1 | 同上 |
| `thread` 加尾缀 `-x` | 1 | 同上 |
| `principal` → `openmir2-dev-2` | 1 | 同上 |
| `field` → `body.head` | 1 | 同上 |

5 个键逐个变异全部 rc=1 ⇒ 豁免不是「整体放过 parent」，而是精确到这条条目。

## 6. 边界

- **新写入的坏 parent 仍判红**：即便值与这条完全相同，只要 `entry_oid` 不同（新条目）、线程不同，
  就不命中本豁免。想豁免必须另开条目、写明 reason 并有人签字。
- 本卡**没有**增加任何字段级/principal 级/线程级豁免；`oid_gate.py` 判定逻辑零改动。

## 7. 残留物清单

- `E:/tmp/dev2-oid-neg/baseline-{oid,value,thread,principal,field}-swapped.json` —— 负例副本，
  不提交，卡关闭后可删。
- 无其它临时目录/分支/进程；本线未起服务端/客户端。
