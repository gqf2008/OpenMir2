# 排水次序门：`SM_DISAPPEAR` 必须先于出现帧（卡 `openmir2-drain-order-gate`）

- 卡：`openmir2-drain-order-gate`（tier **L**：只加测试与静态断言，零生产行为变更、零帧变更）
- 代码：`mir2-rs/bins/game-svr/src/lib.rs`
  - ① 构造级：`tests::drain_run_downlinks_sends_disappear_before_appear_frames`
  - ①b 构造级（死亡支，卡 `openmir2-drain-order-gate-followup` S-1）：
    `tests::drain_run_downlinks_sends_disappear_before_death_appearance`
  - ② 静态级：`drain_order_gate_tests::disappear_call_precedes_downlink_inside_frame_drain_body`

## 1. 为什么这条序需要判据

oracle `PlayObject.Run()` 用**单趟** `VisibleActors` 循环按下标处理：`Hidden` 发
`RM_DISAPPEAR`、`Show` 发 `RM_TURN`/`RM_DEATH`。`UpdateVisibleGay` 只把**本轮新见**对象
`VisibleActors.Add(...)` **追加到表尾**，旧条目原地保留 ⇒ 不变式「所有 `Hidden` 下标 < 所有
`Show` 下标」⇒ 出帧次序**必为「先整批消失、后整批出现」**。

本宿主在 `drain_run_downlinks_with_magic` 里把 `drain_aoi_disappear(...)` 排在
`drain_aoi_downlink(...)` 之前，与 oracle 同序。但在此卡之前：

- 序只靠**注释宣称**（原 `lib.rs:15547-15551` 一带），**没有任何断言**钉住；
- 上一轮复审实测：**把排水挪到出现帧之后仍全绿**（卡 `openmir2-castle-disappear` review M-1）；
- 既有用例的边界：`aoi_disappear_frame_sent_when_target_leaves_view` **直调** helper（doc 自认
  「删生产调用也不红」）；`aoi_disappear_reachable_through_drain_run_downlinks` 走生产路径但
  **只有 hides、没有 shows** ⇒ 只证明「生产调用存在」。**两者之间的相对次序**是空白。

序是**协议可观测**的（客户端按到达序处理），只测「发了没发」是典型假绿通道，故补两档判据。

## 2. 两条断言各抓什么 / 抓不到什么

### ① 构造级（主判据，`drain_run_downlinks_sends_disappear_before_appear_frames`）

- 夹具：同一 tick 里 `rec.hides = [(p, vanished)]` **且** 观察者 `p` 可见集钉一条
  `VisibleFlag::Show` 的真实体（`spawn_clone` 造实体 + 展示表登记，`search_tick == clock_ms`），
  然后走**生产入口** `drain_run_downlinks`，按 `rx` 的 FIFO 序（= 共享 gate `tx` 的发送序 = 调用序）
  收集帧。
- 抓：**所有 `SM_DISAPPEAR(30)` 的位置都小于所有出现帧**；出现帧家族按 oracle
  `src/M2Server/Player/PlayObject.cs:3222-3238` 的 `Show` 分支取全：`SM_TURN(10)` /
  `SM_FEATURECHANGED(41)`（`!Death`）、`SM_DEATH(32)`（`Death && !Skeleton`，`:3235`）、
  `SM_SKELETON(33)`（`Death && Skeleton`，`:3231`）。失败信息直接打印**实际序（ident, recog）**、
  期望序、第一个违约位置。
- **死亡支单独一条（S-1）**：`drain_run_downlinks_sends_disappear_before_death_appearance` 用
  同一夹具但把目标 `death = true` ⇒ `appear_frames` 只发 `SM_DEATH(32)`；断言消失帧早于死亡帧、
  且本 tick 恰一条死亡帧。牙：把该用例 `is_death` 改恒假 ⇒ `deaths.len()==1` 断言红
  （实测实际序 `[(30,4242),(32,2)]`）。
- 自检：本 tick 必须**真的同时**含两类帧（缺一类/多出未知帧都单独报错），防止用例退化成只测一类。
- **抓不到**：如果将来出现帧改走**另一条通道**（不经 `drain_aoi_downlink` 的同一 `tx`），
  `appears.len() == 2` 自检会报「需同步更新」，届时本判据**会失效而不是静默变绿**，必须同步改夹具。
  另外它抓的是**本夹具路径**的序，不是所有会话/所有分支的全局序。

### ② 静态级（第二判据，`disappear_call_precedes_downlink_inside_frame_drain_body`）

- 手法复用同文件 `mod broadcast_tx_source_order_tests` 的 `blank_non_code()`：把**注释与
  字符串/字符字面量**涂白（保留换行 ⇒ 行号不变），再在只剩代码的文本上做字节级定位。
- 抓：`drain_run_downlinks_with_magic` 体内 `drain_aoi_disappear(` **恰一处**、`drain_aoi_downlink(`
  **恰一处**，且前者字节位置 < 后者；并断言两处调用的**最内层函数**仍是该帧面函数（防止把它们
  整体挪到别的函数里，让构造级判据失去生产可观测位置）。
- 说明：卡面写作「`drain_run_downlinks` 函数体」——生产 `sim_loop` 实际调用的是
  `drain_run_downlinks_with_magic`，`#[cfg(test)] fn drain_run_downlinks` 只是**转发参数**的测试
  入口，两者对应同一段生产体；静态门钉的是后者。
- **抓不到**：它只看**调用序**。若将来改成「先收集、再按别的键排序后发送」，静态调用序仍绿，
  只有 ① 会响；它也不证明序**为何**必须如此（那是 oracle 口径，见 §1）。

### ②b 夹具覆盖边界（S-2，**明确登记**，二选一取 b）

本构造级夹具**手工 `push` 一条 `VisibleFlag::Show`**（并钉 `search_tick == clock_ms`），
**不覆盖**真实 `search_view_range` 扫描产生 hides/shows 的路径。真实扫描的正确性由世界侧用例负责：

- `mir2_rs::aoi::tests::player_scan_view_update_distance_gate_exact_boundary`
  （`mir2-rs/crates/world/src/aoi.rs`，驱动 `search_view_range_with_updates` 并断言 `removed`/`view_updates`）；
- `mir2_rs::aoi::tests::view_range_update_only_emitted_by_player_viewer`（同文件，真扫描的来源过滤）。

⇒ 本文件的序判据只回答「**同一 tick 内两类帧的相对顺序**」，不回答「扫描是否产出正确的 hides/shows」。
补端到端夹具（S-2 选项 a）未做，理由：世界侧已有上述用例且本卡写集只允许测试区与 docs；
若将来出现帧改走另一通道，① 的自检会先红并要求同步更新。

## 3. 三条牙（串行跑，`git checkout --` 还原；均实测）

| 变异 | ① 构造级 | ② 静态级 |
| --- | --- | --- |
| (a) 删 `let _disappeared = drain_aoi_disappear(rec, online, gates);` | 红（无 `SM_DISAPPEAR`，实际序 `[(10,2),(41,2)]`） | 红（体内计数 0） |
| (b) 交换两条 drain 调用 | 红（序违约，首帧出现帧） | 红（`disappear` 行号 > `downlink` 行号） |
| (c) `drain_aoi_disappear` 函数体改空 | 红（无 `SM_DISAPPEAR`） | 绿（调用序未变，属**预期**：序的**内容**由 ① 负责） |

结论：本卡钉住的「相对次序」在两档判据下都有牙；没有哪种变异在**两条**新增断言下仍全绿。
(c) 对 ② 不红是设计使然（静态门不执行语义），已在上面写明。

## 4. 残留物

无：只改 `mir2-rs/bins/game-svr/src/lib.rs`（`#[cfg(test)]` 区域）并新增本页；未起栈、未连库、未占端口。
