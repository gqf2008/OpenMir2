# L2-5 ②第 2 步验收判据草案（同格序 / 多帧 / 镜像隔离 / 消失时序）

> 状态：**判据草案**（Leader 委托；只读 + 文档，基于 `06b63c397`）。**凡标「假设·未核」的条目不得当依据**。
> 每条判据均附「证据形式」；「改坏怎么红」= 必须能红的变异方向。

## 0. 观察面（判据都挂它）

| 面 | 判什么 | 为什么不选别的 |
| --- | --- | --- |
| **P1 每收件人有序出现帧流** `(socket, recog, ident)` | 顺序 + 内容 | 玩家唯一可见面；等价于 oracle `VisibleActors` 入队序（**假设·未核**：oracle 侧未实证） |
| **P2 `VisibleActors` 入队序**（世界侧） | 扫描序 | 把「扫描序错」与「出帧发射顺序错」分开定位 |
| P3 可见集合（排序后） | 仅成员 | 只判「有没有」，不判序 |

**约定**：C1–C4 用 P1（主）+ P2（定位）；C5–C9 用对应的世界侧可观测面。

## 1. 同格序（三布局，C1–C3）

三个布局都**与单 World 基线逐帧比对 P1**；基线里实体集合/坐标/创建序完全相同。

- **C1 本地+真实（本地先落格）**：同格 = 本地实体先 Enter、远端实体后 Enter。期望：出现帧序 == 基线。
  红：镜像若按「常数追加格尾」而基线序相反 ⇒ `FIRST_DIFF` 指向这两条。
- **C2 本地+真实（远端先落格）**：基线里远端先于本地落格。期望/红同 C1——**这一条是本轮 Q2 的正例**。
- **C3 多镜像同格**：两个远端实体同格。期望：镜像间序 == 基线里两者 Enter 序。
  **假设·未核**：`snapshot_border` 的 `iter_ref` 槽位序与 owner 侧 Enter 序一致（未实证）；若不一致，本判据会红——那正是要暴露的。
- **为什么 P1 能咬住**：AOI 扫描按格（先 x 后 y）、格内按 `obj_list` 下标序（`aoi.rs` 的 `search_view_range_with_updates`），`cell.add` = `obj_list.push`（`map.rs` 的 `add`）；`drain_aoi_downlink` 按 `VisibleActors` 入队序发帧 ⇒ 同格相对序直接落进帧流。改坏方向：把镜像插入改成「插到格首」或基线序与快照序不一致 ⇒ C1/C2/C3 至少一条红。

**证据（C1–C3 共用形式）**：测试 `step2_same_cell_order_{local_first,remote_first,multi_mirror}`；命令 `cd mir2-rs && cargo test -p game-svr --lib step2_same_cell_order`；rc=0；原文 = 三条 PASS 行 + 每条打印的 `P1` 序列（含 `recog/ident`），以及一次**变异演示**（把 `upsert_mirror` 的挂格改成插格首）令 C2 红的 `FIRST_DIFF` 原文。

## 2. 多帧出现（C4）

- **C4 记法**：一次 appearance **不是一帧**——正常 = `SM_TURN` + `SM_FEATURECHANGED`（2 帧），骑乘 +1（`SM_MOUNTRIDE`），死亡 = 单帧。判据按**逐帧** `(recog, ident)` 记录；账本去重键 `(tick, owner, recipient, kind=ident)` ⇒ 同一次 appearance 两帧 kind 不同、**不算重复**。
  红：把 kind 换成常量 ⇒ 第二帧立即被判 `Duplicate`（6b 已有同款牙 `ledger_distinguishes_kinds_and_flags_true_duplicates`，第 2 步直接复用并加「镜像侧同样多帧」）。
  **证据**：测试 `step2_multi_frame_appearance_is_not_duplicate`；命令 `cd mir2-rs && cargo test -p game-svr --lib step2_multi_frame`；rc=0；原文含按序打印的 `ident=SM_TURN,SM_FEATURECHANGED` 两行 + 账本 `check()` 绿。

## 3. 镜像隔离五面（C5–C9）

当前代码事实（两条并列，**不要**只读第一条）：
① `mirror` 标志只在实体/flags 与 `World` 镜像 API 里出现，**没有任何 AI/攻击/仇恨代码读它**（`06b63c397` 的 grep：`mirror` 命中只在这些文件 + `snapshot_border` 过滤）；`is_blocking` 只看 `ghost/death/fixed_hide/ob_mode`（`world.rs` 的 `is_blocking`），`mirror` 不在其中 ⇒ **活镜像参与阻挡**。
② 但 `apply_mirrors` 按**快照里的 owner 坐标**插镜像（`region_loop.rs` 的 `apply_mirrors`：`map_id/x/y = e.map_id/e.x/e.y`），而 `HaloEntity` 只从**邻区边界带**产生 ⇒ **镜像格落在 owner 的 rect 内，不在收件人 rect 内** ⇒ 收件人区玩家的常规走位**碰不到**镜像；只有「迈出本区 rect 的那一步」才会撞上。
⇒ 因此**不能**以「镜像是只读副本所以不该挡路」为由改 `is_blocking`：oracle 单 World 里那具身体是实打实挡路的，把它从阻挡集合摘掉可能**反而制造分歧**（Leader 2026-11-22 裁决）。

| # | 面 | 能观察？观察什么 | 当前靠什么保证 | 判据与红检 |
| --- | --- | --- | --- | --- |
| C5 | AI | 能：镜像坐标/dir 跨 tick | **无**（AI 不读 `mirror`） | 镜像 10 tick 内坐标只随快照变；红：AI 驱动镜像走位 ⇒ 与快照坐标不符 |
| C6 | 仇恨/攻击 | 能：镜像与 owner 的 HP | **无**（攻击按 id 取实体，未过滤镜像；**假设·未核**攻击解析路径细节） | 朝镜像 id 攻击 ⇒ 两侧 HP 不变 + `cross_hit_delayed` 计数；红：镜像 HP 下降 |
| C7 | **跨界一步的阻挡归属** | 能：边界格 `CanWalk` 被谁挡（阻挡者 id） | 走位阻挡经 `map.rs` 的 `move_object` → `is_blocking`（见 §3 两条事实）；第二个 `move_object` 调用点的阻挡集合来源**假设·未核** | **判据 = 两侧同结论**：先量 oracle（单 World）在边界格 `CanWalk` 被谁挡，再量拆区后谁挡，二者必须一致（允许按 handoff 语义「拒绝跨界」，但**不得**因镜像而拒）。红：把镜像从阻挡集合摘掉导致拆区可走而基线被挡（或反之）。**在拿到这对测量前，任何改 `is_blocking` 的改动都无依据** |
| C8 | MonGen | 弱：怪物总数 | 不读实体列表（**假设·未核**） | 固定 seed 下怪物总数与基线相同；红：镜像被计入刷怪上限 |
| C9 | 死亡/掉落 | 能：`death` 翻转的副作用 | **无**：`update_mirror_frame_fields` 直写 `death`，绕过 `Die()`（§9.1 新增条目；掉落/经验链关系**假设·未核**） | 镜像 `death` 翻转不产生掉落/经验/尸体帧；红：出现掉落或经验链帧 |

**证据（C5–C9）**：测试 `step2_mirror_isolation_{ai,attack,mongen,death}` + `step2_boundary_step_block_attribution`；命令 `cd mir2-rs && cargo test -p mir2-world-bevy step2_mirror_isolation` 与 `cargo test -p game-svr --lib step2_boundary_step_block_attribution`；rc=0；原文 = 每条 PASS 行 + 注入演示（对镜像 `attack_cmd` 后打印两侧 HP 不变）+ **C7 的「oracle 阻挡者 / 拆区阻挡者」两侧原文**（阻挡者 id 必须相同）。

## 4. 消失帧与刷新时序（C10）

- 时序事实：`after_phase1`（barrier1 后）→ `apply_mirrors` → `tick_phase2`（AOI）⇒ 本 tick 扫描已含刷新后的镜像；`remove_mirror` 后由下一次 AOI 的 `hides` 自然产生消失帧。
- **C10**：边界带走出的 tick 上，消失帧多重集 == 基线；首个消失帧的 tick ≤ 基线 + 1（镜像可见性由 owner 快照驱动，**允许晚 1 tick**）。
  红：把 `apply_mirrors` 挪到 `tick_phase2` 之后 ⇒ 消失帧晚 2 tick ⇒ 超窗即红。
- **§9 已知不对称（不算缺陷，逐条引用）**：①镜像插入不经 `Command::Enter`（本 tick 消费 Enter 的系统看不到）；②换位/移除/`AddTime` 续期/`death` 直写（6b 正在补进 §9.1）；③镜像刷新晚于本区 phase1（本 tick 的 AI/碰撞看到的是刷新后状态）。
  **证据**：测试 `step2_disappear_tick_window`；命令 `cd mir2-rs && cargo test -p game-svr --lib step2_disappear_tick_window`；rc=0；原文 = 每 tick 的 `hides` 打印 + 与基线的差值（0 或 1）。

## 5. 建议顺序与门

C4（记法）→ C1–C3（格序）→ C7（**跨界一步的阻挡归属**：先测量两侧再判同结论）→ C6/C9（直写副作用）→ C5/C8 → C10。
每步合并门：五门 + `-Mode Both`（Window 内）+ 本草案逐条 PASS；`MIR2_ZONES` 未设时零变化仍须成立。
