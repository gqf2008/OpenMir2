# 随机分支成对帧的对拍口径（2026-10-26）

卡 `openmir2-rules-random-idle-frames`（tier L）。实现：`tools/conformance/rules.json` 的
`random_branch_pairs` + `tools/conformance/compare_runs.py` 的实现/自检。

## 0. 一句话

**无名 `SM_TURN(10)` + 紧随同 recog 的 `SM_FEATURECHANGED(41)`** 这对帧由**随机分支**产生
（oracle 墙钟播种 vs rust `with_seed(42)`）⇒ 同一二进制同一场景下这对帧的次数在 **0/1/2** 之间跳。
它**不再当帧差**：按**多重集比形状**，次数差只登记、不判红；形状差（尾、位置、Light、配对）仍判红。

## 1. 为什么叫「随机分支」而不是「纯 idle 动画」

- 固定 NPC / 商人：`Merchant.cs:132-141`、`GuildOfficial.cs:39-53` 的
  `Random(50)==0 ⇒ TurnTo(RandomByte(8))`。
- 机器人**走路卡住**分支：`RobotPlayer.Base.cs:982/:997` 的 `Random(2)⇒TurnTo else WalkTo`
  ——reviewer-21 的反例，写「纯 idle」会漏掉这一支。
- 收件人侧 `RM_TURN` 支**无条件**补发 41：`PlayObject.Message.cs:1524-1560`。
- 随机源：oracle `new Random()` 墙钟播种（`src/OpenMir2/RandomNumber.cs:35`）；
  rust `with_seed(42)`（`mir2-rs/crates/world/src/world.rs:1301`）⇒ 跨进程不可复现。

## 2. 形状合同（`rules.json` → `random_branch_pairs`）

| 项 | 值 | 依据 |
|---|---|---|
| 识别 | `ident=10` 且 `body_len==11`（**无名尾**）**紧跟** `ident=41` 且 `body_len==0`，且两帧 `recog` 相同 | `npc_random_idle_frames_through_production_drain_run`（`mir2-rs/bins/game-svr/src/lib.rs`，行为牙） |
| 比的字段 | `(param, tag, Light, phase)` 的**多重集** | 位置与相位是确定性的；`Light = series 高字节`（固定 NPC/NormNpc ctor `Light=2`） |
| 不比的字段 | `series` 低字节 = `Dir` = `RandomByte(8)` 的结果 | 这正是随机分量；比它就会把随机噪声重新变成红 |
| 次数差 | **不判红**，登记进 `whitelisted(random_branch_pairs)` 与报告 `random_branch_pairs` | 卡 A1「按多重集比，不判存在性」 |
| 一侧 0 对 | **不判红**（没有形状合同可比） | 同上；实测 move_walk 是 oracle 1 / rust 0 |
| 带名尾 TURN | **不在本规则内**，照常逐帧比 | 登录期出现帧是确定性的（body_len>11） |
| 禁止 | 不许 `ident 10` 整类 `ignore`、不许整类 `ignore 41` | 两者都有确定性实例（见 `rules.json` 里 ident 10/41 的既有条目） |

实现位置（`compare_runs.py`）：`extract_random_pairs()`（按形状摘出、其余帧保持原序）与
`compare_random_pairs()`（多重集比形状）；摘对发生在 `ignore` 过滤之后、`_build_blocks()` 之前；
`kept_frames` 仍按**摘对之前**的长度算（空运行判据不变）。

## 3. 牙

常驻牙：`python -I -B tools/conformance/compare_runs.py selftest`（新增 8 条，既有 343 条不放松）。

```
PASS random_pairs: 规则表已登记 random_branch_pairs
PASS random_pairs: 一侧 0 对 ⇒ 不判红
PASS random_pairs: 少一对不判红
PASS random_pairs: 次数差被如实登记
PASS random_pairs: TURN 尾改坏 ⇒ 判红
PASS random_pairs: Light 改坏 ⇒ 判红（形状合同）
PASS random_pairs: Dir 变化不判红（随机分量）
PASS random_pairs: 删确定性带名尾 TURN ⇒ 判红
```

对应卡 A2 的三条：①「删一对 ⇒ 不判红」（第二、三条）、②「尾/Series 改坏 ⇒ 必红」（第四、五条，
Series 的高字节 Light 判、低字节 Dir 不判）、③「删确定性带名尾 TURN ⇒ 必红」（第八条；fixture 用**真实形态**：带名尾 TURN 后面也跟一条 41，这样若把「无名尾」判定删掉，这一对会被误当随机对摘走 ⇒ 该牙变红）。

### 3b. 实现层变异（都在已提交 head 上做，还原后 `hash-object == HEAD`）

| 变异 | 结果 |
|---|---|
| 去掉「一侧 0 对」豁免（`if rc else []`） | 只红「一侧 0 对 ⇒ 不判红」 |
| 从 `shape_fields` 去掉 `light` | 只红「Light 改坏 ⇒ 判红」 |
| 删掉「无名尾」判定（`body_len == 11`） | 红 3 条：预期的「删确定性带名尾 TURN」＋连带「TURN 尾改坏」＋共享夹具的「控制帧 ⑤ 34 条全部比较」（带名尾对被误当随机对摘走） |

## 4. run2 重算（**同一批产物**，旧代码 vs 新代码）

方法：把 `ae69e4e44` 的 `compare_runs.py` + `rules.json` 连同其依赖（`tools/probe/mir2frame.py`、
`tools/loginfixture/`、`tools/db-parity/`）复制到 `E:/tmp/dev-2/oldconf/`，与当前工具**跑同一份**
`E:/tmp/window-20261026/run2/<场景>/{csharp,rust}/frames.ndjson`。

| 场景 | 旧 verdict | 新 verdict | diffs 旧→新 | undecidable 旧→新 | 随机对 (oracle/rust) |
|---|---|---|---|---:|---|
| bag_equip | FAIL | FAIL | 13→10 | 0→0 | 0/1 |
| butch_two_draws | UNDECIDABLE | UNDECIDABLE | 72→67 | 6→6 | 4/0 |
| castle_merchant_tax_potion | UNDECIDABLE | UNDECIDABLE | 5→1 | 1→1 | 24/27 |
| **dice_chain_auto** | **FAIL** | **PASS** | **5→0** | 0→0 | 2/0 |
| eat_item | FAIL | FAIL | 13→9 | 0→0 | 1/1 |
| gm_batch | PASS | PASS | 0→0 | 0→0 | 0/0 |
| gm_time | PASS | PASS | 0→0 | 0→0 | 0/0 |
| login_only | PASS | PASS | 0→0 | 0→0 | 0/0 |
| merchant_dialog | UNDECIDABLE | UNDECIDABLE | 0→0 | 1→1 | 0/0 |
| move_walk | UNDECIDABLE | UNDECIDABLE | 12→11 | 1→1 | 1/0 |
| npc_click | UNDECIDABLE | UNDECIDABLE | 1→0 | 1→1 | 0/1 |
| npc_click_auto | PASS | PASS | 0→0 | 0→0 | 0/0 |
| spell_cast | PASS | PASS | 0→0 | 0→0 | 0/0 |

**verdict 变化只有 1 条**：

- `dice_chain_auto` **FAIL → PASS**：旧的 5 条 diff 全部是那 2 对随机帧造成的错位
  （oracle 2 对、rust 0 对）；现在按形状摘出、次数差只登记 ⇒ 0 diff。其余帧逐字节相同。

diff 数下降但 verdict 不变的那些，都是同一个原因（随机对造成的错位被去掉），但因为还有
**与随机无关**的差异，verdict 仍是 FAIL / UNDECIDABLE：

- `castle_merchant_tax_potion` 5→1：24/27 对都按形状摘掉，剩下 1 条是 `body_len`（别的差异，非随机帧）；
- `move_walk` 12→11：剩下的是 4 组**带名尾** TURN 的**顺序/位置**差异（确定性帧，规则不覆盖，
  应当判）；
- `npc_click` 1→0：rust 的 1 对随机帧不再造成错位；verdict 仍 UNDECIDABLE 是别的原因（ident 14 的
  PENDING ignore 计数不齐）。
- `dice_chain_auto` 之外两处 `diffs` 里仍含 `random_pair_shape` 吗？——没有：`shape_mismatch`
  全部为 `false`（一侧 0 对时不比形状；两侧都有对时共享形状集）。

## 5. 边界与残余

- 只覆盖「无名尾 TURN + 41」这一对；**带名尾** TURN（登录期出现帧）与单独出现的 41 仍逐帧比。
- `Dir`（series 低字节）**没有任何数值判据**——它是随机分量；其**编码形态**由
  `npc_random_idle_frames_through_production_drain_run` 行为牙钉死（`Series=MakeWord(Dir,Light=2)`）。
- 规则 `approved_by` 仍是 **PENDING**：报告会带 `rules_pending_review=true`，等 owner/coordinator 复核。
- 与 ident 14 的 PENDING ignore 相乘：如果某轮 rust 多了一条确定性支的 14，仍会由
  `ignored_rule_census` 的计数不齐判 UNDECIDABLE（本卡没动 ident 14）。
