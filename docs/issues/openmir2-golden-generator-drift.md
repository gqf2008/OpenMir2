# openmir2-golden-generator-drift — 既有 golden 与当前生成器 schema/墙钟漂移

**状态**：事实登记（只读查证）；**待决策（是否整批重生成）**，非紧急。
**查证 head**：master `2692d0a26`。

## 1. 现象
仓库既有 magic golden（如 `mir2-rs/tests/parity/golden/magic/skeleton.json`）是**旧 schema** 生成的；用当前 master 的 MagicGolden 生成器**重跑会出 diff**：
- **新增字段**（结构 diff）：`caster_actor_id` / `target_actor_id` / `recog` / `due_ms_rel` / `hit_speed` / `frame_order_scope`（均已在 master `Program.cs` 的 `FireballResult`/帧记录里，属 c6 的 recog-schema 变更：`:866/867/931/944/1000`）。
- **值变化**：`delivery`（`GetTickCount` **墙钟**）。

## 2. 影响
- **任何「重生成 == 仓库现状」的判据都不成立**（旧 golden 天然 stale）；用逐字节相等当门是错判据。
- golden 含**墙钟字段**（`delivery`）⇒ **天然不可逐字节复现**。§13 允许"行为等价 + 自报身份"，但**必须明说哪些字段不可复现**。

## 3. 要求（口径）
- 列「哪些 golden 已 stale、哪些字段不可复现」的清单口径：**不可复现 = 墙钟/GetTickCount 派生字段**（至少 `delivery`；`MasterRoyaltyTick` 已按 §36 不记不比）。
- 结论化的回归判据应改为「改动不引入**语义相关**变化」：**结构 diff 的新增键 ⊆ 已登记 schema 变更集，值变化 ⊆ 墙钟集**，逐条归类（附命令）。
- 后续是否**整批重生成**所有 magic golden（一次把 schema/墙钟对齐）——决策点，非今晚解决。

## 4. 证据（本卡生成时）
对 skeleton：结构 diff `ADDED ⊆ {schema}`、`CHANGED = {delivery}`、`SinSu/recall 相关 = 空`、`未归类 = 空`。

## 5. 整类根因：离线替身「造怪 0 抽」⇒ RNG 流偏移（不止 SINSU）
`OfflineWorldEngine.RegenMonsterByName` 造怪 **0 抽**，而 Rust `create_monster_oracle` 真抽（`bins/game-svr/src/mongen.rs:498/501` 等）⇒ **凡走 `RegenMonsterByName` 的叶子，RNG 流在两实现间偏移**，后果有二（同一根因）：
- **`Rng` 长度/逐值不同源**（SINSU 实测 golden 6 抽 vs Rust 8 抽：前 5 同、Rust 多 3 造怪抽）⇒ 这类 leaf 不宜逐值比 Rng（是**替身不忠实**，非产品错）。
- **帧面 RNG-payload 偏移**（例：SINSU 640 `SM_MAGIC_LVEXP` 的 `tag`=训练 `Random(3)+1`，golden=2 vs rust=3）⇒ 与 drain 窗口无关。
⇒ 凡 `# act` 召唤/造怪类叶子，评审时先问"这个值是否来自造怪/train 抽"；是则从比较面排除并注明此根因。

## 6. 共享替身变更：预置 slave 现在带 `CurrX/Y=0` + `Envir`
`Program.cs` 的 `PreSlaveCount` 预置 slave 构造（`RunMagicCase`）现为每个占位 slave 设 `CurrX=CurrY=0` 与 `Envir=caster.Envir`（此前不设）。
- **为什么**：`RecallSlave → SlaveList[i].SpaceMove(...)` 需目标对象 `Envir` 非空才能移动（SINSU `cap_hit` 回召面；见 `openmir2-golden-harness-mapmgr`）。
- **当前为何无影响**：`PreSlaveName=="pre-slave"` 的既有用例名字构造不变、`golden` 未重出 ⇒ 现状零 diff。
- **将来重生成注意**：若某用例的期望面涉及预存 slave 的坐标/`Envir`，重生成可能出 diff（预置 slave 从旧默认变为显式 `(0,0)`+env）；届时按「共享替身变更」归类，不是产品行为变化。

> **§7–§11 由 QA `openmir2-3b` 追加**（来源 `docs/issues/qa-golden-drift-inventory-20261120.md`；清单查证 head = master `01da212ce`）。与既有 §5（整类根因）/§6（共享替身）不重复，不覆盖任何原正文。

## 7. 逐文件字段表（51 份；来源：QA `openmir2-3b`）
| 文件 | actor_id | recog | due_ms_rel | delivery(墙钟) |
| --- | --- | --- | --- | --- |
| `bigcloak.json` | — | — | — | Y |
| `bighealing.json` | — | — | — | Y |
| `cloak.json` | — | — | — | Y |
| `dejiwonho.json` | — | — | — | Y |
| `fire.json` | Y | Y | Y | Y |
| `fireball-5.json` | — | — | — | Y |
| `fireball.json` | — | — | — | Y |
| `fireboom.json` | — | — | — | Y |
| `fireboom47.json` | — | — | — | Y |
| `firecharm.json` | — | — | — | Y |
| `groupamyounsul.json` | — | — | — | Y |
| `groupdeding.json` | — | — | — | Y |
| `grouplightening.json` | — | — | — | Y |
| `hangmajinbub.json` | — | — | — | Y |
| `hbfireball.json` | — | — | — | Y |
| `healing.json` | — | — | — | Y |
| `holyshield.json` | — | — | — | Y |
| `legacy/bigcloak.json` | — | — | — | Y |
| `legacy/bighealing.json` | — | — | — | Y |
| `legacy/cloak.json` | — | — | — | Y |
| `legacy/dejiwonho.json` | — | — | — | Y |
| `legacy/fireball-5.json` | — | — | — | Y |
| `legacy/fireball.json` | — | — | — | Y |
| `legacy/fireboom.json` | — | — | — | Y |
| `legacy/firecharm.json` | — | — | — | Y |
| `legacy/hangmajinbub.json` | — | — | — | Y |
| `legacy/hbfireball.json` | — | — | — | Y |
| `legacy/healing.json` | — | — | — | Y |
| `legacy/lightning-11.json` | — | — | — | Y |
| `legacy/lightning-45.json` | — | — | — | Y |
| `legacy/shield.json` | — | — | — | Y |
| `legacy/showhp.json` | — | — | — | Y |
| `legacy/snowwind.json` | — | — | — | Y |
| `lightflower.json` | — | — | — | Y |
| `lightning-11.json` | — | — | — | Y |
| `lightning-45.json` | — | — | — | Y |
| `shield.json` | — | — | — | Y |
| `showhp.json` | — | — | — | Y |
| `skeleton.json` | — | — | — | Y |
| `snowwind.json` | — | — | — | Y |
| `uenhancer.json` | — | — | — | Y |
| `unamyounsul.json` | — | — | — | Y |
| `warr-12.json` | — | — | — | Y |
| `warr-25.json` | — | — | — | Y |
| `warr-26.json` | — | — | — | Y |
| `warr-27.json` | — | — | — | Y |
| `warr-3.json` | — | — | — | Y |
| `warr-34.json` | — | — | — | Y |
| `warr-38.json` | — | — | — | Y |
| `warr-4.json` | — | — | — | Y |
| `warr-7.json` | — | — | — | Y |

## 8. 分组结论
- **完全新 schema（actor_id + recog + due_ms_rel 全有）= 1 份**：`fire.json`
- **完全旧 schema（三项全无）= 50 份**（即上表除 `fire.json` 外的全部文件，含 9 份 `warr-*` 与 17 份 `legacy/*`）
- **混合（部分有）= 0 份** —— **本批为零，无需点名**。
- 附：`hit_speed` 键：**18 份有 / 34 份无**（`git grep -l '"hit_speed"' master -- <dir> | wc -l` = 18）；`frame_order_scope` 仅 `fire.json` 3 处。
- **`delivery`（墙钟）在 51 份 JSON 中全部存在**（`Rm` 记录里）。

## 9. 整批重生成的代价（必然 diff 的字段）
用当前 master 生成器重跑：
1. **50 份旧 schema 文件**：结构 diff 必然包含
   - `Fixture` 增 `caster_actor_id` / `target_actor_id`（`MagicGolden/Program.cs` 的 `caster_actor_id` / `target_actor_id` 赋值，**无条件写**）；
   - **每一条 `Sent` 帧**增 `recog` 与 `due_ms_rel`（同文件帧记录构造处的 `recog` / `due_ms_rel` 字段，**无条件写**）；
   - `Fixture` 增 `hit_speed`（同文件 `hit_speed` 字段，**无条件写**）。
2. **全部 51 份**：`delivery` 是 `GetTickCount` 墙钟 ⇒ **重生成值必变、逐字节复现不可能**（含已是新 schema 的 `fire.json`）。
⇒ 判据不能是"重生成 == 仓库现状"，只能是卡 `openmir2-golden-generator-drift.md` §3 的"结构 diff ⊆ 已登记 schema 变更集、值变化 ⊆ 墙钟集"。

## 10. `hit_speed` 键的 18 份 = 哪一代
口径更正：按 **JSON 51 份**统计 —— **18 份有该键 / 33 份无**；此前"34 无"是把 `legacy/README.md`（非 golden）也算进去了。

| 分布 | 文件 | 判定 |
| --- | --- | --- |
| `hit_speed` **值 = 7**（6 份） | `cloak` / `bigcloak` / `shield` / `uenhancer` / `hangmajinbub` / `dejiwonho` | **7 代点亮件**（657 `series` 旋钮生效的那六份） |
| `hit_speed` **值 = 0**（12 份） | `fire` / `fireball-5` / `fireboom` / `fireboom47` / `firecharm` / `hbfireball` / `healing` / `bighealing` / `lightflower` / `lightning-11` / `lightning-45` / `snowwind` | **有键但退化值**（生成器已带该字段，夹具未设 7） |
| **无该键**（33 份） | 其余全部（含 9 份 `warr-*`、17 份 `legacy/*`、`groupdeding`/`grouplightening`/`groupamyounsul`/`holyshield`/`skeleton`/`showhp`/`unamyounsul`） | **0 代 schema** |

⇒ 同一目录混着**两代（甚至三种形态）**夹具；重生成时 `hit_speed` 是**第二个必然 diff 源**（对外层 33 份为"新增键"，对值 0 的 12 份为"值可能变"，对 6 份 7 代件为"值应不变但键序/格式可能变"）。

## 11. 不可复现字段：`delivery`（全部 51 份）
**"`delivery` 在全部 51 份 golden 中都存在（`Rm` 记录的墙钟）⇒ 不是'某些文件旧了'，而是'所有 golden 都含不可复现字段'"**；
按 §13 口径：行为等价 + 自报身份，不要求字节级，但**必须逐字段写明不可复现集合**（至少 `delivery`）。

## 12. 缺键 ⇒ 整段判据静默失效（`due_ms_rel` 实例）

比较器只在 **golden 第一帧带该键**时才把字段纳入比较面（`magic_fireball_parity.rs` 的
`sent_fields.push("due_ms_rel")`）；而**收件人级过滤**（如 `sent_filter_sinsu` 的 `keep` 白名单）
还会把某些 ident 整条移出比较面。两种机制叠加时会出现「金标准与实现不一致、判据却全绿」：

- **实例 A（缺键）**：`skeleton.json` 属旧世代（`Sent` 无 `due_ms_rel`）⇒ 640 的时序整段不判；
  用现行装置重生成可得 `640 due=1000`，而 Rust 在施法批发（`due=0`）——旧件下看不出来。
- **实例 B（keep 过滤）**：`sinsu.json` 的 caster 640 是 `due=1000`，Rust 台账全帧 `due=0`，
  但 `sent_filter_sinsu` 只留 `17/638/642` ⇒ 640 被过滤面吃掉，**当前绿≠已验**。
  **已收口（2026-11-21）**：`keep` 不动 + 补「只判 `due_ms_rel`」的不对称断言（见 sweep 卡 #6）。

**防御**：golden 与实现任一侧缺键 ⇒ 该字段判「未验」并显式登记（不得默认通过）；过滤白名单
（`keep`）必须在卡里列出**被排除的 ident 及其理由**。对应收口：`openmir2-magic-640-delay-sweep`。
