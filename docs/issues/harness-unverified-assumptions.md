# 装置对生产的「未核实假设」总表

> **五列**：假设 / 装置怎么做 / 生产实际是什么 / 已核实 / 影响哪些判据。
>
> **表头铁律（务必先读）**：**每条都标明它落在哪条轴**——
> **`realism` 轴 = 装置↔生产**（我们是否复刻了生产）；**`对拍` 轴 = 装置↔Rust**（同一驱动下两栈是否一致）。
> **realism 轴上发现的差，不得拿去改对拍轴**（本项目多次踩坑的根因：在两条轴上换着问问题）。
>
> 立表 2026-11-22。**已排除的也列**（标「已排除 + 覆盖范围」），否则下一个人会重做一遍。

| # | 假设内容 | 轴 | 装置怎么做 | 生产实际是什么 | 已核实? | 影响哪些判据 |
|---|---|---|---|---|---|---|
| ① | tick 节拍 | realism | 100ms 步进（`StepDrainMs` 共享驱动） | 待 89 指定数值/是否可变；**B23 契约=保留 100ms 共享驱动**，时间类结论须带 `@100ms 共享驱动` 限定 | 契约化（B23） | 所有多延迟 stem 的时序 / `due` |
| ② | 初始相位 `Random(1500)` | 对拍+realism | cast 前抽 `Random(1500)` 定初相 | **`RunTick = GetTickCount() + RandomNumber.Random(1500)`（`BaseObject.cs`，opc-19 亲核，属 B23）** | **已锚定（B23 内）** | RNG 流 / `due` 初相 |
| ③ | 跨 actor 处理顺序 | 对拍 | 当前固定 `byWho`（caster→target→obs_in→obs_out），**待改** | Rust = `SelfDelayQueue.order` **首入队序**（非 byWho；bigcloak/groupdeding 恰好 caster 先、吻合） | **契约改「首入队序」（B24）**；装置待改 | 帧序 / 跨 actor 交付时序 |
| ④ | 堆 arity | 对拍 | .NET `PriorityQueue` = **4-ary** | 同 .NET = 4-ary；Rust `SelfDelayQueue` 是其 **4-ary 忠实镜像** | **是（已排除）** | 帧序（**排除了**）；覆盖：13 种 prio / len 2–30 / 300k 随机 + 8 手选 / 88% tie / 63% 混 255+非255；4-ary 镜像 vs native **0/300k** 分歧，binary 镜像 **78%** 分歧 |
| ⑤ | RNG 流（次数/上界/序/值） | 对拍 | `RngSeam` 记录 .NET `System.Random` | 生产=`System.Random`(Knuth)；Rust 复算同一 .NET Knuth | **是（已排除）** | Rng 判据（**排除了**）；覆盖：fireball/amyounsul/groupdeding 共 11 case，`Rng+DeliverRng` 拼接**逐值一致** |
| ⑥ | `due` 基准 tick | realism（差在装置↔生产；对拍无影响） | 统一 `castTick`(DoSpell 前) + 虚拟钟（DoSpell 不 Advance⇒ε=0 理想化） | 每 msg 自 `GetTickCount()`，**16ms 粒度** | **是（已在容差内）** | `due`；实测粒度 16ms，差 ≤16ms ＜ 640 的 ±32ms 容差；装置↔Rust 都理想化 ⇒ 对拍**不受影响** |
| ⑦ | AOI / 可见集 | realism | ***不跑 AOI***（`MagicGolden/Program.cs:1538` 明注 `SearchViewRange`/`UpdateVisibleGay` 都不调）；可见性 = 4 个固定玩家（caster/target/obs_in/obs_out 手设坐标）+ push magic（firewind/energyrepulsor）的 `MakeVisible` 回填 `VisibleActors` | 真实 AOI 按距离/范围动态算 `VisibleActors` | **否**（89 在做） | AOI 依赖 magic（push/散布/向"全可见"发帧）的收件人集合 |
| ⑧ | `prio=(byte)wIdent` 语义 | 对拍+realism | 照生产：非延迟 `Enqueue(sm,(byte)wIdent)`、延时 `Lowest` | 同：`BaseObject.Message.cs` 非延迟 `(byte)wIdent`、延时 `Lowest` | **是（生产源码已锚）**；装置各处是否都照做待核 | 队列序 / 帧序 |

## 备注
- ④⑤ 是「用错的对照组打脸错的假设」（④ 证 .NET `PriorityQueue` 是 4-ary 非二叉；⑤ 证两侧同 .NET Knuth）；⑥ 是「把 realism 轴的已知粒度指回它本就在的容差里」。
- 可复用工具：`E:\UserTemp\b17arity`（堆序三方对照：binary-mirror / quad4ary-mirror / native + fuzz）、`E:\UserTemp\b17tick`（`Environment.TickCount` 粒度）。
