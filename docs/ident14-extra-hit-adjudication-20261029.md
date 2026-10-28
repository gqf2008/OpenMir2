# ident14 额外 SM_HIT 受控裁决（T-270）

- 卡：`openmir2-ident14-extra-hit-adjudication`（tier=R，window_required=true）
- 登记：`mir2-rs/tests/parity/whitelist.md` T-270；规则表 `tools/conformance/rules.json` 的
  `rules[ident=14].oracle_extra_shapes` / `oracle_extra_status`
- 实现：`tools/conformance/compare_runs.py`（`oracle_extra_explainable` + 库比完之后的放行闸）
- 关系卡：`openmir2-ignore-rule-count-compare`（counts+shape 门槛）、
  `openmir2-real-machine-verification-master`（真机窗口项）

> **当前状态：APPROVED。** owner/coordinator 裁决条目 `7c9f9525c63402155d37b5c8e09d4ee8d4486683`
> （openmir2-coordinator，卡 `openmir2-ident14-extra-hit-adjudication`）已签，规则里
> `oracle_extra_status="APPROVED"` + `oracle_extra_decision="7c9f9525…"`，放行分支生效；
> 缺 decision 或状态非 APPROVED 时仍按 PENDING 处理（只登记 `frames.oracle_extra_candidates`、
> 保持 UNDECIDABLE、`rules_pending_review=true`）。

## 1. 结论（先给判据）

C# 侧多出的 `SM_HIT(14)` 在**逐帧验证为零可观测副作用**后，按**形状白名单 + 每形状上界 + 四条件**
受控登记；不恢复整类 `ignore`，也不放宽 `ignore_rule_census` 的 counts+shape 门槛。
任何一条不成立即保持 `UNDECIDABLE`。

## 2. A1：逐帧证据（零可观测副作用）

`SM_HIT(14)` 是**展示帧**：`BaseObject.SendRefMsg(RM_HIT, Dir, CurrX, CurrY, 0, "")`
（`src/M2Server/Actor/BaseObject.Message.cs:383` 的签名 `SendRefMsg(wIdent, wParam, nParam1, nParam2, nParam3, sMsg)`）
经 `PlayObject.Message.cs:1433-1437` 转成 `MakeMessage(SM_HIT, ActorId, nParam1, nParam2, wParam)`
⇒ 线上 `param=X / tag=Y / series=Dir`，**body 恒空**，不带 HP/伤害/序号。

| 运行 | 场景 | 额外 14 | 帧位置 | recog（actor） | param/tag（X,Y） | series（Dir） | body | 该 actor 的位置帧 | 其它帧 | 库快照 |
|---|---|---|---|---|---|---|---|---|---|---|
| run12 | softclose_relogin | 1 | 流末第 72 帧（t=11971ms） | 60597838 | 318,336 | 4 | 空（`e3b0c442…`） | ident 10 `recog=60597838` 正是 (318,336) | 其余 71 帧逐字节相同，diffs=0 | 比过，diffs=0 |
| run16 | login_only | 0 | — | — | — | — | — | — | — | 比过，diffs=0 |
| run17 | move_walk | 0 | — | — | — | — | — | — | — | 比过，diffs=0 |
| run21 | gamepoint_exchange | 2 | 第 32/34 帧（t=5800/7137ms） | 29729721 | 49,49 | 5 | 空（`e3b0c442…`） | ident 10 `recog=29729721` 正是 (49,49) | 其余 32 帧逐字节相同，diffs=0 | 比过，diffs=0 |
| run24 | gamepoint_exchange | 1 | 第 30 帧（t=3615ms） | 511670749 | 49,49 | **1** | 空（`e3b0c442…`） | ident 10 `recog=511670749` 正是 (49,49) | 其余 32 帧逐字节相同，diffs=0 | 比过，diffs=0 |
| run23 | gamepoint_exchange | 0 | — | — | — | — | — | — | — | 比过，diffs=0 |

⇒ 额外帧**只有显示语义**：actor 在自己所在坐标、朝自己的朝向「受击」；前后帧、目标 HP 类帧
（本场景无 31/65）、库后态全都不变。「C# 多发一条展示帧」不等于「Rust 少发一条真帧」——
后者会在 counts 上体现为 rust 侧多出/形状不符，本裁决一律不放行。

## 3. 规则字段（rules.json，ident 14）

```json
"oracle_extra_status": "APPROVED",
"oracle_extra_decision": "7c9f9525c63402155d37b5c8e09d4ee8d4486683",
"oracle_extra_shapes": [{"shape": "318,336,4", "max": 1, "decision": "7c9f9525c63402155d37b5c8e09d4ee8d4486683"},
                        {"shape": "49,49,5", "max": 2, "decision": "7c9f9525c63402155d37b5c8e09d4ee8d4486683"},
                        {"shape": "49,49,1", "max": 1, "decision": "f0b5734e0d67718618473bc2fb54e8d6f8ea58dd"}]
```

**Dir 族与逐形状裁决**：`(49,49)` 下已观测到 Dir=5（run21，2 条）与 Dir=1（run24，1 条），
两者都零副作用但**分开登记**——每个 Dir 一个 `max`，且条目可自带 `decision`：
**键缺失、空串或非 40-hex 一律按 PENDING**——没有「继承规则级 approval」这条路。
**每个形状必须自带 `decision`**（reviewer-21 F1）：键**缺失**与空串/非法 oid 一视同仁按 PENDING，
**不允许继承规则级批准**——否则删掉一个 `decision` 键就能让新形状蹭旧批准。`49,49,1` 已由 owner 单独裁决
（comment `f0b5734e0d67718618473bc2fb54e8d6f8ea58dd`，APPROVED，仅该形状、仅 oracle 侧），
放行记录的 `approved_by` 只写**各形状自己的裁决 oid**（多形状时并列去重）。

- `shape` = `param,tag,series`（坐标 + 朝向）；`max` = 该形状允许的 oracle 额外帧**上界**
  （reviewer-20 R20-X2：同形状不能无界放行；上限取 A1 证据里实际出现过的条数）。
- **形状键不含 recog（ActorId）**（R20-X3，登记口径）：`SM_HIT` 的 recog 只决定客户端给谁放
  受击动画；ActorId 是会话内分配的，跨运行不稳定，硬绑只会制造假红。actor 差异本身没有服务端
  后果，而「其它帧全等 + 库全等」已经把可观测状态锁死。若将来要绑 actor，另加
  `oracle_extra_recogs` 白名单并由裁决条目说明（selftest 有 `recog 不计入形状` 的登记牙）。
- `oracle_extra_status` / `oracle_extra_decision`：**只有显式 `APPROVED` 且带非空裁决 oid 才放行**；
  缺省/未知值、以及「写了 APPROVED 但没写裁决 oid / oid 不是 40-hex」一律按 PENDING
  （fail-closed，reviewer-20 R20-Y1/R20-Z1，selftest 各有牙）。`oracle_extra_shapes` 的每个形状**必须带 `max`**，bare string 视为规则非法
  （R20-Y4，同样有牙——否则新规则能用旧写法绕开上界）。

## 4. 判定流程（compare_runs.py）

两段式：帧面先判「是否够格当候选」（`oracle_extra_explainable`），放行与否等库比完再定。

候选的必要条件（缺一即不是候选，`adjudication_blocked_by` 写明原因）：

1. 规则声明了 `oracle_extra_shapes`，且没有解析不了的条目；
2. **rust 侧没有 oracle 没有的形状，也没有 rust 计数 > oracle 计数的形状**（Rust 多发不算）；
3. 所有计数不等的形状都在白名单里，且 `body_sha256` 是空体哈希（有载荷不放行）；
4. 每个形状的 oracle 额外帧数 **≤ 该形状 `max`**。

库比完之后的放行闸（全部满足才放行）：

1. 帧面除该 ignore 差异外**没有别的 diffs**；
2. 两侧库快照都读到且比过（`state=compared`）；
3. 库无 diffs；4. 库无 undecidable；5. `oracle_extra_status == "APPROVED"`。

未放行时：该 ignore 差异**保持 `UNDECIDABLE`**，并写 `adjudication: "T-270"` +
`adjudication_blocked_by`（点明被哪条挡住），同时把候选写进 `frames.oracle_extra_candidates`
（含每形状两侧计数 / `max` / `status` / `blockers` / 实际 `verified` 数据）。
放行时写 `frames.oracle_extra_hits[]` 并进 `whitelisted`（`kind=oracle_extra_hit`）——
**显式登记，不是静默掩码**；`reason` 与 `verified` 按**本次实际条件**生成
（`frame_diffs` / `db_state` / `db_diffs` / `db_undecidable`，reviewer-21 F2）。
PENDING 期间报告 `rules.rules_pending_review=true`，`rules.pending_review` 里出现
`T-270: rules[ident=14].oracle_extra_shapes`（R20-X1）。

## 5. 牙与变异（`compare_runs selftest`，离线合成输入）

放行闸的机制牙用**合成的 APPROVED 规则副本**测（仓库规则保持 PENDING）；PENDING 行为单独一颗牙。

| 牙 | 断言 |
|---|---|
| PENDING（仓库规则） | 形状全对也只登记候选、`rc 2`、无 `oracle_extra_hits`、`blocked_by` 含「PENDING」、`rules_pending_review=true`（R20-X1） |
| 正向（合成 APPROVED） | 白名单形状空体 14、无其它帧差、库全等 ⇒ `rc 0`，`oracle_extra_hits` 非空、`whitelisted` 含 `oracle_extra_hit` |
| 伴随库差 | rust 侧 Level 7→8 ⇒ `rc 2`，`blocked_by` 点明库 |
| 伴随帧差 | 41 的 body 改一字节 ⇒ `rc 2`，`blocked_by` 点明帧面 |
| 形状不符 | 额外帧 param/tag=7,7 ⇒ `rc 2` |
| body 非空 | 额外帧带 `b"x"` ⇒ `rc 2` |
| Rust 多发 | 同形帧放到 rust 侧 ⇒ `rc 2` |
| 库未比（F1） | 两侧都没有库快照（`absent_both`）⇒ `rc 2`，`blocked_by` 点明「库未比」 |
| 库判不了（F1） | `state=compared`、diffs=0 但库侧有 undecidable ⇒ `rc 2`，`blocked_by` 点明「判不了」 |
| 超上界（X2） | 同形状 3 条 > `max=2` ⇒ `rc 2`，`blocked_by` 点明 `max` |
| actor 口径（X3） | 换 recog、同坐标/朝向/空体 ⇒ 同判（登记「actor 不参与形状」） |
| APPROVED 缺 decision（Y1） | status=APPROVED 但删掉 `oracle_extra_decision` ⇒ 视同 PENDING、`rc 2`、`blocked_by` 点明缺 decision |
| verified 实际条数（Y2） | 伴随帧差时候选的 `verified.frame_diffs` 必须是实际条数（≥1），不能写死 0 |
| 影响面（Y3） | PENDING 且本次有候选 ⇒ `rules.pending_review_used` 含 T-270 |
| bare shape（Y4） | 形状写成 bare string（无 `max`）⇒ 规则非法、不入候选、`rc 2` |
| verified.db_*（正向） | 放行记录 `verified` 带实际 `db_state=compared` / `db_diffs=0` / `db_undecidable=0` / `frame_diffs=0` |
| verified.db_diffs（库差） | 伴随库差时候选 `verified.db_diffs >= 1`（不能写死 0） |
| verified.db_undecidable（库判不了） | 库判不了时候选 `verified.db_undecidable >= 1` |
| decision 非 oid（Z1） | `oracle_extra_decision="not-an-oid"` ⇒ 视同 PENDING、`rc 2`、`blocked_by` 点明「不是合法 oid」 |
| Dir=1（仓库规则） | `49,49,1` 带独立裁决 `f0b5734e…` ⇒ `rc 0`，`approved_by` 为该 oid |
| 删形状 decision 键（F1） | 把某形状的 `decision` 键删掉 ⇒ 视同 PENDING、只登记候选、`rc 2`（不允许继承） |
| Dir=1 裁决清空 | 临时副本把该形状 `decision` 置空 ⇒ 只登记候选、`rc 2`、`blocked_by` 点明形状裁决未落地 |
| max 边界相等（W1） | `delta == cap`（1==1）⇒ 放行；`>`→`>=` 的变异必须红 |

变异（逐次恢复并核 sha256，无 MUTATION 残留；恢复后
`compare_runs.py` sha256=`185e78aae0565e6655d0d021faad31f20d5147f79866497061f139d51931c40f`）：

| 变异 | 结果 |
|---|---|
| 删白名单形状检查 | 形状不符牙红 |
| 删空体检查 | body 非空牙红 |
| 删「rust 多发」检查 | Rust 多发牙红 |
| 删「帧面其它差异」条件 | 伴随帧差牙红 |
| 删「库有差异」条件 | 伴随库差牙红 |
| 删「库未比」条件 | 库未比牙红（rc 0 / `absent_both` / 无 hits） |
| 删「库判不了」条件 | 库判不了牙红 |
| 删 `max` 上界检查 | 超上界牙红（rc 由 2 变 0） |
| `oracle_extra_status` 恒返回 APPROVED | PENDING 牙红 |
| 删「APPROVED 必须有 decision」 | Y1 牙红（rc 由 2 变 0） |
| `verified.frame_diffs` 写死 0 | Y2 牙红 |
| 不把 T-270 计入 `pending_review_used` | Y3 牙红 |
| 允许 bare shape 无 max | Y4 牙红（rc 由 2 变 0、候选出现） |
| `verified.db_diffs` 写死 0 | 库差 verified 牙红 |
| `verified.db_undecidable` 写死 0 | 库判不了 verified 牙红 |
| 删 decision 的 40-hex 校验 | Z1 牙红（rc 由 2 变 0） |
| `>` 改成 `>=`（max 边界） | max 边界牙红（连带 Dir=1 两颗也红） |
| 忽略条目自带 decision（一律继承） | Dir=1 裁决清空牙红（rc 由 2 变 0） |
| `approved_by` 不用形状自己的 oid | Dir=1 正向牙红 |
| 缺键回落到 INHERIT（两处同时改） | 删键牙红（rc 由 2 变 0）；只改一处时放行闸仍兜住（纵深防御） |

每次变异只打红对应那颗牙（其余仍绿）。

**F1 的两半（reviewer-21 原意）**：① 不删条件时，删 `db_snapshot.json` + `db/` ⇒
`rc 2 / state=absent_both / blocked_by「库未比」/ 无 hits`；只删一侧 ⇒ `one_sided` 同样 `rc 2`；
去掉 run.json 的 `--char-name` ⇒ `rc 2 /「库有 N 条判不了」`。② **短路 state/undecidable 检查（变异）后**，
无库快照的 run12 会变 PASS —— 这正是 F1 说的「缺牙」；本 head 已补两颗牙，删条件即**命名红**
（变异 M6/M7）。两句同时成立；此前版本只写了前半，读起来像在否认 F1，已按此更正。

## 6. 离线重算（A4 的离线部分）

用当前分支工具对既有窗口产物重算（`compare_runs.py compare --oracle <run>/<sc>/csharp
--rust <run>/<sc>/rust --out …`）：

| 运行 | 判定 | 说明 |
|---|---|---|
| run5 softclose_relogin | **UNDECIDABLE**（rc 2） | 额外帧形状 `318,332,7` 不在白名单 ⇒ 保持原判 |
| run12 softclose_relogin | **PASS**（rc 0） | `oracle_extra_hits` 1 条，`approved_by=7c9f9525…`，`verified={compared,0,0,0}` |
| run16 login_only | PASS | 无 14 帧 |
| run17 move_walk | PASS | 无 14 帧 |
| run21 gamepoint_exchange | **PASS**（rc 0） | `oracle_extra_hits` 1 条（含 2 帧），`approved_by=7c9f9525…` |
| run23 gamepoint_exchange | PASS | 无 14 帧（未触发白名单） |
| run24 gamepoint_exchange | **PASS**（rc 0） | Dir=1 形状经 `f0b5734e…` 裁决 ⇒ `oracle_extra_hits` 1 条，`approved_by` 为该 oid |

⇒ 裁决条目（`7c9f9525…`）落地后，只有**A1 证据覆盖的两个形状**放行；run5 等未登记形状仍 UNDECIDABLE，
仓库里其它窗口运行（run3/run4/run5-upgrade）同样不放行。

仓库里其它窗口运行（run3 castle/dice/move_walk、run4 butch、run5 upgrade_retrieve 等）的 14 形状
都**不在**白名单，仍按 `ignore_rule_mismatch` 判 `UNDECIDABLE`——本裁决不是「整类 14 放行」。

## 7. 残余风险 / 未完成

- **白名单是坐标级的**：`318,336,4`（max=1）与 `49,49,5`（max=2）来自这两次运行；换地图/换站位或
  同场景多跑出第 3 条，都需要按同样证据逐个登记（不会自动放行）。
- **只证明「这几条额外帧零副作用」**：不能推出「所有 C# 额外 14 都零副作用」。确定性支（城堡商人
  `FixedHideMode` 解除补发，`Merchant.cs:152-157`）与攻击命中仍走原路径，没有被本裁决覆盖。
- **Dir=1 已裁决**：`f0b5734e…`（APPROVED，scope=(49,49,1) max=1、仅 oracle 侧）。新 Dir 仍需按
  同样证据 + 单独裁决登记，未裁决的 Dir 只登记候选。
- **裁决 oid 未与本工具交叉核对（R20-DIR3）**：规则级与逐形状 `decision` 都只校验「40-hex 的
  collab 条目 oid」，不核对该 oid 是否真是 coordinator 的 APPROVED 条目；任意 40-hex（别的 commit/
  别的卡）都能启用。当前两处 oid 都是人工核过的正确条目，属登记在案的残余。
- **裁决条目已落地**：`7c9f9525…`（APPROVED，scope=两形状 max1/max2、oracle 侧、零副作用）。
  新增形状仍需新的证据与裁决；`oracle_extra_status` 改回 PENDING（或 decision 清空）即退回候选模式。
- **真机复跑未做（A4 的窗口部分）**：窗口受 Leader 排期，本卡不自行起栈；待窗口内在当前 master
  复跑至少一条场景并把证据入卡。
- 父卡的真机项（ident14「不该出现的一侧不发」）仍在 `openmir2-real-machine-verification-master`
  （`fdc4bbb27`）。
