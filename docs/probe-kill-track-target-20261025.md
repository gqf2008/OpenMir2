# probe kill 寻靶模式（target tracking）— 2026-10-25

- 卡：`openmir2-probe-kill-track-target`（tier **L**，owner `openmir2-dev-2`，branch `card/probe-kill-track-target`）。
- 定位：`kill` 步原来只打「自身正前方**固定格**」（`target_xy`）。怪会走位 ⇒ 固定格在 settle 后已空 ⇒
  `CM_HIT` 打到空气（`SM_HIT=0`）。本卡新增**寻靶模式**：以**最新已收帧**为基准取目标当前坐标、出手前复核、
  走开就重算。它把「打固定格」换成「打最近已知格 + 复核」，**显著降低** flaky 概率；**不是确定性保证**——
  复核通过到 `CM_HIT` 生效之间仍有残余窗口（见 §3.1）。
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

`nearest:<名>` 取**当前可见、未死、且八向相邻（cheb==1）**的同名对象里曼哈顿距离最近的那个（`SM_TURN(10)` 名尾的 `ShowName`）；**同格（cheb=0）不选**；没有相邻目标时**只等不追**（最多 `max_retries` 轮，不注入移动帧），等不到则 fail-closed 并列出坐标与 cheb 距离（卡 `openmir2-probe-track-target-adjacency`）；
同名两只按 `(距离, actor id)` 稳定排序。目标检测复用探针的 `scan_actors`（含 `SM_DISAPPEAR(30)` 剔除）。

## 2. 为什么固定格模式天然 flaky

dev-1 第三轮的帧证据（`comment 9a53f980`）：两只鸡在 `seq20 @(331,332)`、`seq22/40 @(328,331)`、
`seq45 SM_APPEAR @(328,332)`，**1.5s settle 之后已离开固定格 `(330,331)`** ⇒ `kill` 打 `(330,331)` 得
`SM_HIT=0` 空打；同一构造在 tester 的 run5 成功、run6 `no target`。⇒ 「固定格 + 等待」不是确定性：
怪每 `RegenMonstersTime` 周期会动，是否恰好留在格上是**竞态**。加大超时只会把竞态拉长，不会消除。

## 3. 复核与重算（机制描述，不是确定性保证）

每轮：

1. 用**最新帧**选目标（当前格 + actor id）；
2. 目标不在 8 邻格 ⇒ `StepFailure`（**本步不走位**，提示手动站位；走位是另一张卡的能力）；
3. `CM_TURN(recog=自身坐标, tag=Dir)` 转身，等 `interval_ms`（≥350ms 的 ActionIntervalTime）；
4. **出手前复核**：重新选目标；若目标消失/死亡 ⇒ 报错不空打；若换了格/换了 actor ⇒ `retries += 1`，
   `retries > max_retries` ⇒ `StepFailure` 打印**目标最后已知坐标 + actor + 帧 seq**；否则回到 1；
5. 复核通过 ⇒ `CM_HIT(recog=自身坐标, tag=Dir)` 出手，等死亡帧（`wait_ident`，默认 `SM_NOWDEATH(34)`/`SM_DEATH(32)`）；
   未死且未到 `max_swings` ⇒ 回到 1（下一击重新选靶，所以连击期间走位也能跟上）。

### 3.1 残余窗口（如实描述）

复核读的是**已经收到的帧**：转身后的 `conn.pump(interval_ms)` 会把这段时间到达的帧收进来，复核基于那一刻的最新状态；
复核通过后**不再等待**，立即发 `CM_HIT`。因此存在**残余窗口** = 「复核所用的最新帧」到「`CM_HIT` 在服务端生效」之间
（网络往返 + 服务端处理）。目标若恰好在这个窗口内再次走开，这一击仍会打空——下一轮（下一次挥击）会重新选靶。

所以本卡的收益是**概率性**的：把「固定格」的持续错位（settle 后格已空，之后每一击都打空）换成「只在残余窗口内错位」
（最多单次打空，下一击跟上）。dev-1 的帧证据正是前者：两只鸡 `seq20 @(331,332)`、`seq22/40 @(328,331)`、
`seq45 SM_APPEAR @(328,332)`，1.5s settle 后固定格 `(330,331)` 已空 ⇒ 固定格模式每一击都 `SM_HIT=0`；
tester 的 run5 成功 / run6 `no target` 是同一竞态。寻靶模式在这两段录制上会先重算到 `(328,331)` 再出手，
但它**不消除**残余窗口，也不能保证「一次就中」。

**出手间隔口径（钉死）**：track 模式每轮 = 选靶 → `CM_TURN` → `pump(interval_ms)` → 复核 → `CM_HIT` →
`pump(interval_ms)` ⇒ 相邻两次 `CM_HIT` 的实际间隔 ≈ **2×interval_ms**。必须 ≥ 部署 `HitIntervalTime=850`
（`src/M2Server/Player/PlayObject.Attack.cs:850`），否则一部分挥击会被「攻击忙」节流吃掉。
⇒ **track 的 `interval_ms` 下限 = 450**（加载期校验，报错串直接给出 2× 关系与 850）；默认 1000 ⇒ 实际 2000ms，安全。
旧的固定格模式不受影响（每轮只 `pump(interval)` 一次，下限仍是 350）。

`max_retries` 的语义（钉死）：**N = 允许的重算次数**；第 N+1 次复核仍不一致才报错。所以 `max_retries=3`
意味着一轮里最多重算 3 次、第 4 次失败才 `StepFailure`（报错串会同时写出「允许 N 次重算，第 N+1 次仍不一致」）。

取值依据：每次复核至少隔一个 `interval_ms`（部署 `HitIntervalTime=850ms`，探针下限 350ms），
3 次 ≈1–2.5s，覆盖观测到的走位节奏（1.5s settle 级别）；上限 10 防止把「找不到目标」拖成长等待。

## 4. 对 `butch_two_draws` 的意义

- 击杀步（`openmir2-probe-kill-step`）已合、活体夹具与 RNG 接线已落地；剩下的是**打中会走的鸡**。
  寻靶模式把「打固定格」换成「以最新已收帧为基准 + 出手前复核 + 逐击重选靶」，把「持续打空」压缩成
  「残余窗口内的单次打空」；**它降低 flaky 概率，但不消除**（见 §3.1）。真机是否稳定仍要窗口复拍。
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
| 同名多只 | **相邻优先**（cheb==1），同层按曼哈顿距离最近；「相距相同」时按 actor id 稳定取小；同格不选 |
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
- **牙 e**：两只**等距**同名（帧序 702 在前）⇒ 稳定按 actor id 选 701（`tag=2`）；
- **牙 f**：目标只发 `SM_NOWDEATH(34)`、不发 `SM_DISAPPEAR` ⇒ 不得再被选中、一帧不发；
- **牙 g**：`max_retries=1` 且目标只走开一次后稳定 ⇒ 应继续出手（`alive`），不得报「复核用尽」（`>`↔`>=` 分界）；
- **牙 h（T7）**：近者 actor=702、远者 actor=701 ⇒ 必须选**近的 702**（`tag=2`）——「距离优先于 actor」；
- **牙 i（T8）**：hook `Conn.send`，在 `CM_HIT` 后注入 `SM_NOWDEATH(34)` ⇒ 必须返回 `result=dead`、`swings=1`、
  **不再重算**（成功路径的牙；此前把 `death` 集合清空 selftest 仍全绿）；
- **牙 j**：track 模式的 `interval_ms` 下限（350 ⇒ 加载期拒；450 ⇒ 通过；旧模式 350 不受影响）；
- **牙 f 扩展**：target-gone 的可见集对「已死未消失」的对象加 `[已死]` 标注；
- **牙 d**：复核永远失败（stub 每次换格）⇒ 用尽 `max_retries`，错误串带最后已知 `(330,331)` + `seq=`，
  且**一次 `CM_HIT` 都没发**。

变异（每条都从 HEAD 还原后重跑确认 `OK (19 checks)`）：

| 变异 | 咬哪颗牙 |
|---|---|
| `SM_TURN` 后续帧不更新坐标（按首次出现格打） | 牙 a：tag 变成 A 的方向 |
| 不剔除 `SM_DISAPPEAR` 目标 | 牙 b：对已消失目标出手 |
| 选**最远**而不是最近 | 牙 c：tag 变成另一只的方向 |
| 出手前复核整条失效 | 牙 d：不再重算/不再报「复核用尽」 |
| 去掉 actor 决胜键（等距平手） | 牙 e：等距两只的选中对象随帧序漂移 |
| 已死目标仍可选（只看 `disappeared`） | 牙 f：对只发死亡帧的目标出手 |
| `retries > max_retries` 改成 `>=`（差一） | 牙 g：允许的一次重算被误报成「复核用尽」 |
| `death = set(plan["wait_idents"])` 清空（T8） | 牙 i：成功路径不再认死亡帧（`result` 变 `alive`） |
| 排序键 `(dist, actor)` 改成 `(actor, dist)`（T7） | 牙 h：选了远的那只 |
| 去掉 track 的 `interval_ms ≥ 450` 加载期校验 | 牙 j：350 被放行 |
| 去掉 `[已死]` 标注 | 牙 f：错误串看不出目标是死的 |

## 7. 残留物清单

- 入库：`tools/probe/mir2_probe.py`（+ 本文件）。
- 未入库（一次性脚本）：`E:/tmp/kt_a.py`、`E:/tmp/kt_b.py`、`E:/tmp/kt_c.py`、`E:/tmp/mut_kt.py`。
- 未触碰：`card_window.py`、`run_batch.sh`、`run3-scenarios.json`、`butch_two_draws.json`、`src/**`；
  未起服务端、未占端口、未跑 cargo（测试机由 tester 的 W1 批次占用）。
