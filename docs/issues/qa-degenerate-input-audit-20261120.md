# 输入可追溯性审计：golden 记录值 vs 实际应用值 vs Rust 消费（2026-11-20）

> 卡：`openmir2-magic-parity`（伤害面补靶批次）。
> 基线：分支 `card/magic-damage-teeth` @ `dc5a82b20`（半成品），接手工作树 `card/magic-damage-teeth-c6`。
> 本文只做**输入面审计**，不改实现、不改清单 scope（scope 由 opc-19 统一更新）。
> 触发：P0 复核发现 MG-51 的 `caster_dc` 早就逐支显式（`dc_range` 用 `MakeWord(20,30)`），
> 交接文的「夹具没有 CasterDc」与树不符 ⇒ 把同口径扩到原 13 条。

## 1. 口径（本文唯一判据）

对每个条目、每个 case，回答三问：

1. **记录值**：golden JSON 的 `Fixture` 里写的是什么（外部可回放的输入）；
2. **实际应用值**：夹具程序在生成这些帧时真正赋给施法者的值（`Program.cs` 的逐 case 覆写）；
3. **消费值**：Rust 驱动复跑时读什么、比较器看不看得到。

「点亮」= 三者一致且区间非退化（`Hi > Lo`），并且该值进入被比较字段
（SM 帧 `ident/param/tag/series/tail`、`TargetHp`、RNG 流）。

**本文最重要的发现是第 2、3 问不闭合**：记录值不等于实际应用值，消费端也读不到。

## 2. 修正表（原 13 条）

`golden 记录` 一列取自 golden JSON 的 `Fixture`（逐 case）；`实际应用值` 取自
`MagicGolden/Program.cs` 的 `CasterMcNoClamp` 逐 case 覆写（`useMcVariant` 只对
`label == "s5" || "s13" || "hit"` 生效）。数值 `0x000F` = 15（Lo=15, Hi=0，钳制支）；
`0x0A05` = `MakeWord(5,10)`（Lo=5, Hi=10，区间 6）。

| 条目 | golden 记录（每 case） | 实际应用值 | 是否退化 | 依据（golden 文件 / case） |
|---|---|---|---|---|
| MG-10 fireball | Fixture **无 caster_mc 字段**（旧 FireballCase 格式） | miss/edge=15；hit=0x0A05 | **非退化（仅 C# 产出面）** | `fireball.json`：edge 的 SM 31 series=15，hit=9 |
| MG-11 fireball-5 | 三支全 `caster_mc=15`、`caster_sc=15` | miss/edge=15；hit=0x0A05 | **非退化（仅 C# 产出面）** | `fireball-5.json`：edge 15 / hit 9 |
| MG-19 lightning-11 | 三支全 15 | edge=15；hit=0x0A05 | **非退化（仅 C# 产出面）** | `lightning-11.json`：edge 15 / hit 9 |
| MG-54 lightning-45 | 三支全 15 | edge=15；hit=0x0A05 | **非退化（仅 C# 产出面）** | `lightning-45.json`：edge 15 / hit 9 |
| MG-53 hbfireball | 三支全 15 | edge=15；hit=0x0A05 | **非退化（仅 C# 产出面）** | `hbfireball.json`：edge 15 / hit 9 |
| MG-37 fireboom | 三支全 15 | s1=15；s5/s13=0x0A05 | **非退化（仅 C# 产出面）** | `fireboom.json`：series 15/6/10 |
| MG-56 fireboom47 | 三支全 15 | s1=15；s5/s13=0x0A05 | **非退化（仅 C# 产出面）** | `fireboom47.json`：15/6/10 |
| MG-45 snowwind | 三支全 15 | s1=15；s5/s13=0x0A05 | **非退化（仅 C# 产出面）** | `snowwind.json`：15/6/10 |
| MG-38 lightflower | 三支全 15 | s1=15；s5/s13=0x0A05 | **部分**：区间抽发生，但伤害被目标 MAC 顶到下限 | `lightflower.json`：series 1/0/1 |
| MG-27 firecharm | `caster_sc=15` 全支（`caster_mc` 也 15） | MC 被覆写成 0x0A05，但 oracle 读 **SC**，SC 仍 15 | **退化** | `firecharm.json`：edge 与 hit 的 SM 31 series 均 15 |
| MG-12 healing | `caster_sc=15`，无 SC 变体 | SC=0x000F 全支 | **退化** | `healing.json`：TargetHp 50→50 / 224→224 / 50→50 |
| MG-40 bighealing | 同上 | 同上 | **退化** | `bighealing.json`：同上 |
| MG-51 groupdeding | `caster_dc` 逐支显式：四支 5140、`dc_range` 7700 | 同记录值（`RunGroupDeDingArm` 逐支写） | **非退化** | `groupdeding.json` 的 `dc_range`（`MakeWord(20,30)`）；Rust 驱动读 `caster_dc` |

## 3. 与原表不一致的逐条原因

- **MG-51**：原表只拿 `CasterMc=15` / `CasterSc=15` 两个缺省判，没看 `CasterDC` 的逐支显式值。
  `GroupDeDingBatch` 里 `dcFlat = MakeWord(20,20)`、`dcRange = MakeWord(20,30)`，且 golden
  的 `Fixture.caster_dc` 逐 case 落盘（5140×4、7700×1）。⇒ **原表错在没看 spec 覆写 + golden 记录值**。
- **8 条 MC（MG-10/11/19/54/53/37/56/45）**：原表判「退化」是 `dc5a82b20` 之前的基线；
  该提交把 C# 侧点亮了，原表没同步 ⇒ 结论过期。**但见 §4：点亮只到 C# 产出面。**
- **MG-27 firecharm**：结论「退化」对，机制写反了——不是「没设区间」，而是区间设到了 MC、
  而 oracle 的伤害式读 SC（`src/M2Server/Magic/MagicManager.cs:337`，`GetAttackPower(... LoByte(WAbil.SC), HiByte(WAbil.SC) - LoByte(WAbil.SC) + 1)`）。
- **MG-38 lightflower**：原表「下限钳制」对。oracle 读 MC（`src/M2Server/Magic/MagicManager.cs:425`）。
- **MG-12 / MG-40**：原表「SC 退化」对；实测被治疗者 TargetHp 完全不动（50→50 / 224→224），
  符合「治疗量在现夹具下不可观测」分支。

## 4. 系统性发现（比单条结论更要紧）

### 4.1 记录缺失：golden 的 `caster_mc` 不是产出这些帧时用的值

`MagicGolden/Program.cs` 的 `RunMagicCase` 在写 `Fixture` 时取 `spec.CasterMc`（缺省 15），
`EmitGolden` 的 `fixture_spec.caster_mc` 同样取 `spec.CasterMc`；`CasterMcNoClamp` 这个
**逐 case 覆写**从未写进 golden。结果：9 条 MC 变体的 hit/s5/s13 帧是用 `0x0A05` 产出的，
但 golden 声称输入是 15。**这正是 P0 要消灭的失效模式（实际输入不可追溯），只是换了字段。**
`fireball.json`（MG-10）更早：Fixture 里连 `caster_mc` 字段都没有。

### 4.2 消费缺失：Rust 侧根本不读 `caster_mc`

- `mir2-rs` 全树 grep `caster_mc` **零命中**；
- `fireball_rig` 把施法者会话的 `wabil.mc` 硬编码为 15（`sg.state.wabil.mc = 15`）；
- `fireball_parity_drive` 只从 golden 读 `hit_speed` / `target_*` / `anti_magic` / `status_time_arr`，
  **不读** `caster_mc` / `caster_sc`；
- 对照：`groupdeding_parity_drive` 用 `golden_fixture_opt(stem, label, "caster_dc")` 真读 DC。

⇒ 即使 golden 的帧分叉，Rust 复跑仍按 MC=15 算，SM 31 series 应为 15 ≠ golden 的 9，
**比较器应当红**。也就是说，「8 条已分叉」只证明了 **C# 产出面**，**比较面没变**。

### 4.3 结论

13 条里三者全齐（记录值显式 + Rust 消费 + 进入比较面）的只有 **MG-51 的 `dc_range`** 一支。
8 条 MC 只能记 `lit-in-oracle-only / not-replayable`，**不能写 verified**。

## 5. MG-38 lightflower 的口径（写死）

`lightflower` 的可观测伤害是 **1 / 0 / 1**。其中 **s5 的伤害 0 仍然是「打到下限」，不是
「区间被观测」**：`0x0A05` 的区间抽确实发生了，但目标 MAC 把结果扣到 0/1。
**即使记录端与消费端都补齐，MG-38 也只能算「部分」，不得写「已点亮」。**

## 5.1 golden 行尾口径（CRLF，省得每个人重查）

`mir2-rs/tests/parity/golden/magic/*.json` **一直是 CRLF**（核过 `HEAD~2/HEAD~1/HEAD` 三版的
blob：fireball-5 / groupdeding / bigcloak 都是全 CRLF；工作树同）。因此重出 golden 后
`git diff --check` 会把新增行报 trailing whitespace——**这是既有口径，不是缺陷**；
改成 LF 会让整份文件全行变动、淹没语义 diff。**不要为了消这条提示去改行尾。**

## 6. 复现命令（只读）

```sh
cd mir2-rs/tests/parity
python -c "import json; d=json.load(open('golden/magic/fireball-5.json',encoding='utf-8')); [print(c['Label'], c['Fixture'].get('caster_mc'), c['Fixture'].get('caster_sc')) for c in d['cases']]"
python -c "import json; d=json.load(open('golden/magic/groupdeding.json',encoding='utf-8')); [print(c['Label'], c['Fixture'].get('caster_dc')) for c in d['cases']]"
python -c "import json; d=json.load(open('golden/magic/fireball-5.json',encoding='utf-8')); [print(c['Label'], [(s['ident'],s.get('series')) for r in c['Receivers'] for s in r['Sent'] if s['ident']==31]) for c in d['cases']]"
```

## 7. 待办（按 opc-19 指令）

1. 记录端：9 条 MC 变体的 golden `Fixture.caster_mc` / `caster_sc` 写**实际应用值**；
   判据 = golden 记录值等于产出这些帧时真正用的值。
2. 消费端：Rust rig 从 golden 读 `caster_mc` / `caster_sc` / `caster_dc`，去掉硬编码 15；
   缺字段**报错**，不得默认 15。
3. 实跑：改消费端前跑一次同类比较器（预期红，证明比较面真在看伤害值）；改完再跑（预期绿）。
   若改完仍红，按「实现与 oracle 的真实差异」上报。

## 8. 世代切换（657 `series`）与剩余 4 条红的产品侧定位（2026-11-20，opc-19 裁定）

### 8.1 两代 golden 不可混比

- **0 世代**（master，`3886e297b`）：golden 无 `hit_speed` 键，657 `series` 恒 0。
- **7 世代**（本分支，承接 `b3844fd22`）：`MagicSpec.HitSpeed` 旋钮 + 6 份 golden 重出，
  657 `series` 出现 7/0 的逐接收者分布。
- 拿 7 世代的装置/Rust 配 0 世代的 golden（或反之）会产出**假差异**——不是任何一侧的缺陷。

### 8.2 装置侧（本卡完成）

- `fireball_parity_drive` 的 `hit_speed` 注入从"只写会话"改为**会话 + 实体**
  （`sg.hit_speed` + `vitals.hit_speed`；`EntityRef.hit_speed` 是后者的视图）。
  写实体那段是 `apply_ability_to_entity` 在装置侧的对应；只写会话会让观察者副本恒 0。
- `cloak_assert` / `big_cloak_assert` 的两处 657 断言原本写死 `(0,0,0)`（期望取自被测实现
  自身输出、注释谎称"与 golden 同源"），已改为**从 golden 读对应接收者的三元组再比**。
- 结果：`cloak(18)` / `bigcloak(19)` 比较器 rc=0（三人 657 series 均为 7）；
  牙：去掉"写实体"那半 ⇒ cloak 必红（9 条，golden=7 rust=0），恢复后回绿。

### 8.3 剩余 4 条红 = **产品侧**缺陷（夹具/装置无责）

观测（同一 run_id，`MIR2_CLIENT_ABILITY_LAYOUT` 未导出；括号内为 golden 期望）：

| 条目 | stem | caster | target | obs_in | 比较器 |
|---|---|---|---|---|---|
| MG-28 | hangmajinbub(14) | **0 (7)** | 0 (0) | **0 (7)** | rc=101 |
| MG-29 | dejiwonho(15) | **0 (7)** | 0 (0) | **0 (7)** | rc=101 |
| MG-43 | shield(31) | **0 (7)** | **0 (7)** | **0 (7)** | rc=101 |
| MG-59 | uenhancer(36) | 0 (0) | **0 (7)** | **0 (7)** | rc=101 |

**定位（符号锚；`bins/game-svr/**` 属卫生门 HIGH_CHURN，禁写行号锚）**：

- 宿主 buff-apply 段（`take_buff_applies` 的循环）用
  `sm_charstatuschanged(ev.actor_id, ev.char_status, 0)` —— series **写死 0**，且**一条 body
  广播给所有接收者**。这是 MG-28/29/43 的直接原因。
- oracle 语义（`src/M2Server/Player/PlayObject.Message.cs` 的 `case Messages.RM_CHARSTATUSCHANGED`）：
  `SendDefMessage(SM_CHARSTATUSCHANGED, ActorId, LoWord(nParam1), HiWord(nParam1), this.HitSpeed)`
  ⇒ **657 的 series = 处理这条 RM 的每个接收者自己的 `this.HitSpeed`**（不是发送者的、也不是常数）。
- 因此 golden 的形状完全自洽：shield 三人应 7；14/15 的 caster/obs_in 保持 7、**target 因自身
  `RecalcAbilitys` 的 `HitSpeed = AddAbil.HitSpeed`（`RecalcHitSpeed` 无被动 ⇒ 0）被重算回 0**；
  36 的 caster 被重算成 0、旁观者保持 7。
- `uenhancer` 的 657 走 ability 同步路径：`recalc_session_ability` 先把 `sg.hit_speed` 置 0，
  再按单 body 发 —— 与上一条同根：**要按接收者取 `HitSpeed` 逐人生成 body**。

**影响面**：MG-28 / MG-29 / MG-43 / MG-59 的 657 `series` 仍不可判绿；MG-32 / MG-33
（cloak / bigcloak）在装置侧修好后已绿。

### 8.4 合并顺序依赖（必须写明）

世代替换不是孤立改动：**先由产品侧卡修好"buff-apply 路径 657 series 逐接收者取值"，
再合本卡（7 世代）**；否则 master 上 MG-28/29/43/59 四条比较器会立刻红。
本卡分支冻结在 `20376762e`，不合并。

### 8.5 `recalc_hit_speed` 的准确表述（原样）

> `world-bevy` 的 `recalc_hit_speed` 与 wg 本体无生产调用点（只剩 re-export 与测试）；
> 宿主持有另一份等价实现 `skill_ability::recalc_hit_speed_skills`。

**不要**写成"死代码"——这会导致下一任误删。
