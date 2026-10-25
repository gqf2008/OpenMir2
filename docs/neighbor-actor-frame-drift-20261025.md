# 邻怪帧漂移取证（F 类）：`SM_TURN(10)` / `SM_FEATURECHANGED(41)` 单侧多帧（卡 `openmir2-neighbor-actor-frame-drift`，tier R）

- 卡：`openmir2-neighbor-actor-frame-drift`（issue `da641efe`；owner `openmir2-dev-1`；
  branch `card/neighbor-actor-frame-drift`；base = 本 patch 的 `--base`）。
  写集：`tools/window/frame_drift_report.py`（新增）、本文件。
- **本卡只定位与分类**：不改 `src/**`、不用掩码/过滤抹差异；「flaky vs 稳定差异」的结论必须等
  **同一场景重复跑 ≥3 次**（机器由 tester 排期，未获窗口前不跑）。

## 1. 取证工具

```
python -I -B tools/window/frame_drift_report.py <batch-dir> [--json out.json]
python -I -B tools/window/frame_drift_report.py --selftest     # 6/6 GREEN
```

对批次里每个场景读 `<scenario>/{csharp,rust}/frames.ndjson`，给每侧 ident 计数、**单侧多出的
ident 与条数**、按序对齐的**首个错位下标**，并分类：

| 分类 | 判据 | 含义 |
|---|---|---|
| `extra_tail` | 有单侧多帧 **且** 前 `min(len)` 帧完全一致 | 多出帧全在尾部（对齐不分叉） |
| `extra_mid` | 有单侧多帧 **且** 对齐在某个下标分叉 | 多出帧在中间，**后续整段受影响** |
| `order_only` | 两侧计数相同、顺序不同 | 不是多帧，是**同刻多队列的出队顺序差** |
| `clean` | 计数与顺序都一致 | — |
| `missing_frames` | 缺一侧 `frames.ndjson` | 未跑到 probe / 该侧未完成，不参与结论 |

`extra_side` ∈ `oracle` / `rust` / **`both`**（两侧都多帧时必须报 `both`；review F3）。

控制帧（`ident` 为 null 且 `frame_class=server_ack_time`）与 `compare_runs.py` 同口径先剔除。

## 2. W1 批次实测（`E:/tmp/window/w1c`，2026-10-25）

> **取数时点**：2026-10-25 **09:40:20 +08**（批次 summary `generated_at`；构建 head=`4b4405bf5`，tester-1 worktree；帧数来自各场景 `frames.ndjson`，由 `tools/window/frame_drift_report.py` 逐条统计）。

| 场景 | csharp | rust | 多出(csharp) | 多出(rust) | 首个错位 | 分类 |
|---|---:|---:|---|---|---:|---|
| login_only | 31 | 29 | `{10:1, 41:1}` | — | — | extra_tail |
| gm_batch | 34 | 36 | — | `{10:1, 41:1}` | — | extra_tail |
| merchant_dialog | 34 | 32 | `{10:1, 41:1}` | — | 29 | extra_mid |
| move_walk | 40 | 39 | `{10:1, 41:1}` | `{14:1}` | 38 | extra_mid |
| npc_click | 30 | 30 | — | — | 19 | **order_only** |

分流（验收①）：

- **oracle 多**：`login_only`、`merchant_dialog`、`move_walk`（各多 1 对 `10/41`）；
- **rust 多**：`gm_batch`（多 1 对 `10/41`）；`move_walk` 的 rust 侧另多 1 帧 `SM_HIT(14)`；
- **不是多帧**：`npc_click` 两侧计数相同，差的是**顺序**——`643` 在 oracle 侧排在 `10/41` 之后、
  在 rust 侧排在之前（同刻两条发送队列的出队顺序）。
- **`move_walk` 第 38 位的错位是两条不同来源的帧撞在同一位置，二者无因果**（review F1）：
  csharp 那条是 `SM_TURN(10) recog=996282343`（`10/41` 对的**起点**），rust 那条是
  `SM_HIT(14) recog=117`（**受击广播**）。对齐只看「同一下标上的 ident 不同」，
  所以不能把 rust 的 `14` 并进「oracle 多 `10/41`」的同一因果里——它们各自独立，
  要分别判定（`10/41` 看 H1–H3，`14` 看它是否每次都出现）。
- **两侧都多帧时工具报 `extra_side=both`**（`move_walk` 就是：oracle `{10:1,41:1}` + rust `{14:1}`）。

多出帧的**形态**：`SM_TURN(10)` 与 `SM_FEATURECHANGED(41)` **成对**出现、`recog` 相同，且该
`recog` 在本侧**其余帧里从未出现**（例：login_only 的 csharp 尾部 `recog=(46212,14120)`；
gm_batch 的 rust 尾部 `recog=(120,0)`）⇒ 是**一个新进视野/刚变化过的 actor 的转身广播**，
不是主角自己的动作。

## 3. 机制假设（每条都给可证伪观察）

| # | 假设 | 可证伪观察（重复跑 ≥3 次时看） |
|---|---|---|
| H1 | **AOI 可见集刷新**：邻怪进/出视野时服务端补发 `SM_TURN+SM_FEATURECHANGED`；两侧进程的刷新 tick 相对场景尾部的相位不同 ⇒ 谁先断连谁能收到这一对 | **正向**：尾部这对帧**每次跑都在变**（有时无、有时换 recog）⇒ flaky；出现与否与「尾部那几百毫秒里有没有新的 actor id」相关。**反向（同样可证伪）**：若 ≥3 轮里这对帧**稳定出现或稳定缺席**（不随 run 变）⇒ **否定 H1 的「相位随机」半句**，应转 H2/H3 或按 §4 的「稳定差异」另开卡 |
| H2 | **怪物随机转身**：怪物 `Random` 触发转身 ⇒ 广播 `SM_TURN+SM_FEATURECHANGED`；触发时刻相对 probe 结束是随机的 | 扫描账本/帧：多出的 `recog` 是否恒为一个**活着的怪**；重复跑时是否出现「同一怪在两侧都转、但时刻不同」 |
| H3 | **断连时 flush**：`10/41` 已入队，但只有在 socket 尚未关闭时才送达；probe 结束即 close ⇒ 尾部存在竞态窗口 | 在场景尾部加确定性 settle（等某个可观测帧）后，这对帧**是否稳定出现/消失**；若稳定消失 ⇒ 判定为「harness 收尾竞态」，应改场景而非改实现 |
| H4 | **同刻多队列出队顺序**（`npc_click` 的 643） | 同一场景重复跑：`643` 与 `10/41` 的相对顺序**是否每次不同**；若每轮都在同一位置 ⇒ **稳定顺序差异**，按真差异另开卡（不得归 flaky） |

**注**：两侧是**分别起栈、分别跑**（`run_side` 串行），所以 `t_ms` 只能在同一侧内比较，
不能拿两侧的绝对 `t_ms` 相减当证据（本轮已按此口径解读）。

## 4. 判定规则（结论必须两分，验收③）

- **flaky**：同侧多轮里，多出帧的**存在性/位置/recog 至少有一项在变** ⇒ 记「该场景的帧面判据对
  这几帧不敏感」，在台账/场景说明登记（§5），**不得用掩码抹掉**（掩码会连同真差异一起吞）；
- **稳定差异**：同侧每轮都在**同一位置**多发/少发（ident、相对位置、recog 形态一致）⇒ **真差异**，
  **不得归为 flaky**，另开实现修卡（本卡只定位）。

## 5. 待真机（window_items）

1. 同一场景（至少 `login_only`、`gm_batch`、`merchant_dialog`、`move_walk`、`npc_click`）
   **重复跑 ≥3 次**，每轮填本工具的输出表（帧数、多出 ident、首个错位、分类）；
2. 每次跑完核三份 Envir 与 `Castle/List.txt`（含影子）；
3. 按 §4 给出**两分结论**；flaky 项登记，稳定项另开卡。

## 6. 顺带发现（不属本卡，登记防丢）

- **`dice_chain_auto`**（w1c）：csharp=34 / rust=33，`extra(csharp)={14:1}`，首错位 32，
  分类 **`extra_mid`** —— 形态与 `10/41` 不同（多的是 `SM_HIT(14)`），且该场景另有
  `pending_ignore_rule` 的整轮 UNDECIDABLE（卡 `openmir2-ignore-rule-count-compare` 的计数/形态判据
  会把它记为 counted 或 mismatch）。登记在此防丢，**不并入**本卡的 `10/41` 结论。
- `castle_merchant_tax_potion` 在 w1c 里 csharp=139 / rust=33 帧，多出 `{10:37, 14:27, 41:37, 53:1,
99:1, 100:1, 200:1, 622:1, 650:1}` —— 量级远大于本卡的「尾部一对」，**不是同一形态**，
应单独开卡 triage（可能是 C# 侧在场景期间持续刷怪广播，或 rust 侧提前结束）。本卡不处理。

## 7. 门禁

`frame_drift_report.py --selftest` 6/6（六分类各一颗牙，含「两侧都多帧 ⇒ both」）；7 项卫生门 +
`git diff --check <base> <head>` + `merge_tree_helper` 见 patch body。**未跑 cargo、未起栈、未占端口。**
