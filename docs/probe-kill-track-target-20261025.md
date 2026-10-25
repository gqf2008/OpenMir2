# probe kill 寻靶模式（target tracking）— 2026-10-25

- 卡：`openmir2-probe-kill-track-target`（tier **L**，owner `openmir2-dev-2`，branch `card/probe-kill-track-target`）。
- 定位：`kill` 步原来只打「自身正前方**固定格**」（`target_xy`）。怪会走位 ⇒ 固定格在 settle 后已空 ⇒
  `CM_HIT` 打到空气（`SM_HIT=0`）。本卡新增**寻靶模式**：按目标**当前**坐标转身、**出手前复核**、走开就重算，
  让 `butch_two_draws` 这类链从概率性变确定性。
- 兼容：固定格模式**保留为默认**（`target_xy` 不变、既有场景零改动）；两种模式互斥。

## 1. 两种模式

```json
{"kill": {"target_xy": "target_xy", "max_swings": 20}}                 // 旧：固定前方格
{"kill": {"track": "nearest:鸡", "self_pos": "...", "max_retries": 3}} // 新：按目标当前坐标
{"kill": {"track": "actor:12345", "self_pos": "..."}}                  // 新：按 actor id
```

| 字段 | 旧模式 | 新模式 |
|---|---|---|
| `target_xy` | **必填**（MakeLong(X,Y)，须与 `self_pos` 八向相邻） | 禁止（与 `track` 互斥，加载期拒绝） |
| `track` | 禁止 | **必填**：`nearest:<ShowName>` 或 `actor:<id>` |
| `max_retries` | — | 可选，默认 **3**（0..10）：出手前复核发现目标走开时的重算上限 |
| `self_pos` | 必填/来自 params | 同左（`CM_TURN`/`CM_HIT` 的 `recog` 都是自身坐标） |
| `max_swings` / `interval_ms` / `wait_ident` / `ident` / `turn_ident` | 同左 | 同左 |

`nearest:<名>` 取**当前可见、未死、曼哈顿距离最近**的同名对象（`SM_TURN(10)` 名尾的 `ShowName`）；
同名两只按 `(距离, actor id)` 稳定排序。目标检测复用探针的 `scan_actors`（含 `SM_DISAPPEAR(30)` 剔除）。

## 2. 为什么固定格模式天然 flaky

dev-1 第三轮的帧证据（`comment 9a53f980`）：两只鸡在 `seq20 @(331,332)`、`seq22/40 @(328,331)`、
`seq45 SM_APPEAR @(328,332)`，**1.5s settle 之后已离开固定格 `(330,331)`** ⇒ `kill` 打 `(330,331)` 得
`SM_HIT=0` 空打；同一构造在 tester 的 run5 成功、run6 `no target`。⇒ 「固定格 + 等待」不是确定性：
怪每 `RegenMonstersTime` 周期会动，是否恰好留在格上是**竞态**。加大超时只会把竞态拉长，不会消除。

## 3. 复核与重算（确定性来自帧，不是等待）

每轮：

1. 用**最新帧**选目标（当前格 + actor id）；
2. 目标不在 8 邻格 ⇒ `StepFailure`（**本步不走位**，提示手动站位；走位是另一张卡的能力）；
3. `CM_TURN(recog=自身坐标, tag=Dir)` 转身，等 `interval_ms`（≥350ms 的 ActionIntervalTime）；
4. **出手前复核**：重新选目标；若目标消失/死亡 ⇒ 报错不空打；若换了格/换了 actor ⇒ `retries += 1`，
   `retries > max_retries` ⇒ `StepFailure` 打印**目标最后已知坐标 + actor + 帧 seq**；否则回到 1；
5. 复核通过 ⇒ `CM_HIT(recog=自身坐标, tag=Dir)` 出手，等死亡帧（`wait_ident`，默认 `SM_NOWDEATH(34)`/`SM_DEATH(32)`）；
   未死且未到 `max_swings` ⇒ 回到 1（下一击重新选靶，所以连击期间走位也能跟上）。

`max_retries=3` 的依据：每次复核至少隔一个 `interval_ms`（部署 `HitIntervalTime=850ms`，探针下限 350ms），
3 次 ≈1–2.5s，覆盖观测到的走位节奏（1.5s settle 级别）；上限 10 防止把「找不到目标」拖成长等待。

## 4. 对 `butch_two_draws` 的意义

- 击杀步（`openmir2-probe-kill-step`）已合、活体夹具与 RNG 接线已落地；剩下的是**打中会走的鸡**。
  寻靶模式把「打固定格」换成「打目标当前格 + 出手前复核」，即把**概率**换成**确定性**。
- 仍需两个外部前提：① 怪物夹具**前置插入**（`openmir2-fixture-monsters-prepend`，否则 60s 内刷不出来）；
  ② 角色站位在目标邻格（探针不走位）。
- 本卡**不改** `butch_two_draws.json`（那是另一条线的写集）；切换只需把该场景的 `kill` 步改成
  `{"kill": {"track": "nearest:<MonName>", "self_pos": "..."}}`。

## 5. 未覆盖 / 边界（诚实清单）

| 项 | 现状 |
|---|---|
| 走位 | **不做**：角色不在 8 邻格 ⇒ fail-closed 提示手动站位（`send` 步可手工移） |
| 目标类别 | `nearest:<名>` 不区分玩家/怪/NPC；`actor:<id>` 完全不做名字校验 |
| 死亡判定 | 只认 `SM_DEATH(32)`/`SM_NOWDEATH(34)` 且 `recog == actor`（同格坐标帧作为补充不用于此判定） |
| 目标彻底不见了 | 若在**第一击之前**就消失 ⇒ 直接报错（不空打）；若中途消失 ⇒ 同上（`target-gone`），不会改打别的同名对象 |
| 同名多只 | 只按最近；「相距相同」时按 actor id 稳定取小 |
| 真机 | 本卡只到构造级（合成帧）；真机由测试窗口负责，未起服务端 |

## 6. 验证

```sh
python -I -B tools/probe/mir2_probe.py --selftest
# => mir2_probe selftest: OK (19 checks; ...)   dsl-kill-track 为新增
```

`dsl-kill-track` 覆盖（构造级、不连网）：

- schema：4 个反例（`target_xy` 与 `track` 互斥、未知前缀、空名、`max_retries` 越界）+ 两种正例；
- **牙 a**：目标 t0 在 A `(331,330)`、t1 走到 B `(330,331)` ⇒ 转身/出手的 `tag` 必须是 B 的方向 4，
  且**没有任何一次**按 A 的方向 2 出手（若实现仍按首次出现格打 ⇒ 红）；
- **牙 b**：目标 `SM_DISAPPEAR(30)` ⇒ `StepFailure`「目标已消失/死亡」，且**一帧都没发**；
- **牙 c**：同名两只（距离 1 与 2）⇒ 选最近的 `(331,330)`（`tag=2`）；
- **牙 d**：复核永远失败（stub 每次换格）⇒ 用尽 `max_retries`，错误串带最后已知 `(330,331)` + `seq=`，
  且**一次 `CM_HIT` 都没发**。

变异（每条都从 HEAD 还原后重跑确认 `OK (19 checks)`）：

| 变异 | 咬哪颗牙 |
|---|---|
| `SM_TURN` 后续帧不更新坐标（按首次出现格打） | 牙 a：tag 变成 A 的方向 |
| 不剔除 `SM_DISAPPEAR` 目标 | 牙 b：对已消失目标出手 |
| 选**最远**而不是最近 | 牙 c：tag 变成另一只的方向 |
| 出手前复核整条失效 | 牙 d：不再重算/不再报「复核用尽」 |

## 7. 残留物清单

- 入库：`tools/probe/mir2_probe.py`（+ 本文件）。
- 未入库（一次性脚本）：`E:/tmp/kt_a.py`、`E:/tmp/kt_b.py`、`E:/tmp/kt_c.py`、`E:/tmp/mut_kt.py`。
- 未触碰：`card_window.py`、`run_batch.sh`、`run3-scenarios.json`、`butch_two_draws.json`、`src/**`；
  未起服务端、未占端口、未跑 cargo（测试机由 tester 的 W1 批次占用）。
