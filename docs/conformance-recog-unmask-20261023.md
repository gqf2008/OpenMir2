# 7 条规则被全局 `recog` 掩码吞掉语义字段：逐条定性与修复（2026-10-23）

> 卡：`openmir2-conformance-rules-recog-unmask`（tier=L）
> 产出：`openmir2-worker-66`（worktree `wt-worker-66`，branch `card/rules-recog-unmask`）
> 基线：`walgit/master` = `de7cb7779`
> 改动前 `tools/conformance/rules.json` sha256 = `e354f45fb946787e1fe3fdbc3f515243dae4725cc72265a82ccf8bdb4ef0f0ce`
> 改动后 sha256 = `012737de901daa8a79f38ee8d72f0b46db694f6d052e0e9825874d9cb79bfc70`
> 本卡**只改比较语义**：`approved_by` 一条未动（脚本逐条核过）。

---

## 0. 结论

- 7 条规则（`54` / `5008` / `1106` / `752` / `811` / `766` / `213`）标 `action=compare`，但 `head_mask`、`head_compare` **都是 None**；`global[0]` 是 `mask_fields:["recog"]` ⇒ **Recog 上的语义值根本没参与比较**。
- 其中 `213`（`SM_ATTACKMODE`）与 `766`（`SM_AREASTATE`）**无 body、其余头部全为 0** ⇒ 在该掩码下这两条规则**实际什么都不比**（帧只按 ident/顺序对齐，任何载荷差异都 PASS）。
- **逐条定性结果：7/7 都是「① 语义值」；0 条「② 会话作用域 ActorId」；0 条「判不准」。**
- 修复：7 条全部加 `head_compare:["recog"]`，并在各自 `reason` 追加「Recog 载荷 + oracle 实参 + 取回理由」。
- 双向断言：改前仅 recog 不同 ⇒ `rc 0`（并登记 `masked_head`）；改后 ⇒ `rc 1` 且 `field=recog`。
- 两颗牙真实 rc：① 删 `head_compare` ⇒ 同一对由 `rc 1` 变 `rc 0`；② 给 ActorId 类 `42` 硬加 `head_compare` ⇒ 跨栈真实 ActorId 差异（`255025409` vs `275`）由 `rc 0` 变 `rc 1`。

---

## 1. 逐条定性（oracle `file:line` + 构造消息实参）

| ident | action | `Recog` 承载什么 | oracle 依据（本卡 `grep -n` 复核） | 定性 | 修复 |
| ---: | --- | --- | --- | --- | --- |
| 54 | compare | `nMusicid` = 地图背景音乐号（无音乐为 -1） | `src/M2Server/Player/PlayObject.Base.cs:894` `SendDefMessage(SM_MAPDESCRIPTION, nMusicid, 0, 0, 0, Envir.MapDesc)`；nMusicid 见 `:889-893`（`Envir.Flag.Music` / `Envir.Flag.MusicId`） | **① 语义值**（地图配置派生，确定性） | `head_compare:["recog"]` |
| 5008 | compare | `GameGold` = 元宝数量 | `PlayObject.Base.cs:938` `SendDefMessage(SM_GAMEGOLDNAME, GameGold, LoWord(GamePoint), HiWord(GamePoint), 0, sMsg)` | **① 语义值**（金额；param/tag 是 GamePoint，本来就在比） | `head_compare:["recog"]` |
| 1106 | compare | `ClientFile1_CRC` = 版本校验 CRC1 | `PlayObject.Message.cs:1690` `MakeMessage(SM_VERSION_FAIL, SystemShare.Config.ClientFile1_CRC, LoWord(ClientFile2_CRC), HiWord(ClientFile2_CRC), 0)`（在 `case RM_LOGON` 内，`:1684-1691`） | **① 语义值**（部署配置派生） | `head_compare:["recog"]` |
| 752 | compare | `AntiMagic` = 魔法躲避（`MakeLong(MakeWord(AntiMagic,0),0)`） | `PlayObject.Message.cs:1675`（`RM_LEVELUP` 支）与 `:1935`（`RM_SUBABILITY` 支）`SendDefMessage(SM_SUBABILITY, MakeLong(MakeWord(AntiMagic,0),0), MakeWord(HitPoint,SpeedPoint), MakeWord(AntiPoison,PoisonRecover), MakeWord(HealthRecover,SpellRecover))`；**无 body** | **① 语义值**（属性派生；原 reason 写“body 逐字节比”不实） | `head_compare:["recog"]` |
| 811 | compare | `BonusPoint` = 可分配点数 | `PlayObject.Base.cs:1208` `MakeMessage(SM_ADJUST_BONUS, BonusPoint, 0, 0, 0)` + `SendSocket(ClientMsg, sSendMsg)`；body 见 `:1200-1207`（`BonusAbilofX / BonusAbil / NakedAbilofX` 三段） | **① 语义值**（属性派生） | `head_compare:["recog"]` |
| 766 | compare | `n8` = 区域状态位（`FightZone|1` / `SafeArea|2` / `InGuildWarArea|4`） | `PlayObject.Base.cs:285` `SendDefMessage(SM_AREASTATE, n8, 0, 0, 0)`；位组装 `:270-284`；**无 body、param/tag/series=0** | **① 语义值**（地图旗标+行会战状态） | `head_compare:["recog"]` |
| 213 | compare | `(byte)AttatckMode` = 攻击模式 | `PlayObject.cs:1146` `SendDefMessage(SM_ATTACKMODE, (byte)AttatckMode, 0, 0, 0)`；**无 body、param/tag/series=0** | **① 语义值**（玩家状态） | `head_compare:["recog"]` |

**为什么没有「② 会话作用域 ActorId」**：这 7 条的构造实参里没有任何一个是 `ActorId`/`HUtil32.Sequence()`/`EcsStore::alloc_id` 形态——全部是配置、属性或状态值。表里真正属于 ActorId 的对照样例是 **42 `SM_USERNAME`**（`PlayObject.Message.cs:1663` `MakeMessage(SM_USERNAME, processMsg.ActorId, GetChrColor(baseObject), 0, 0)`），它**保持不加** `head_compare`（见 §4 牙②）。

**判不准：0 条**（7 条的实参都能在 oracle 侧逐行读到，没有需要猜的）。

---

## 2. 修复内容

`tools/conformance/rules.json`（`git diff`：14 insertions / 7 deletions，只动这 7 个对象）：

- 每条在 `"action": "compare",` 之后插入 `"head_compare": ["recog"],`；
- 每条 `reason` 原文保留，末尾追加一段：
  - `**Recog=<载荷>（语义值，非 ActorId）**：下行 <oracle file:line> `<实参>` … ⇒ head_compare:["recog"] 取回逐值比。`
  - 对 752/766/213 额外写明「本帧无 body」「不加 head_compare 时该帧实际什么都不比」。
- `tier`、`approved_by`、`ident`、`action` 均未改。

`tools/conformance/selftest_impl.py`（+108 行）：新增 `RECOG_UNMASK` 表与三个测试：

- `t_recog_unmask_rules`：前提（global 仍掩 recog）、7 条的 `head_compare==["recog"]`、仍为 `compare` 且无 `head_mask`/`ranges`、反滥用不变式（42 无 `head_compare`）。
- `t_recog_unmask_diff`：双向帧对 + 不外溢 + 牙①。
- `t_recog_unmask_actorid_abuse`：牙② + 对照。

---

## 3. 双向帧对（7 条逐个，两个方向都有断言）

做法：同一对合成帧，**只有 `recog` 不同**（其余头部与 body 完全相同）。
- **改后**：内置规则 ⇒ 必须 `rc 1` 且 `diffs[].field == "recog"`；
- **改前**：把该条规则的 `head_compare` 从**临时副本**里删掉 ⇒ 同一对必须 `rc 0`（证明原来被全局掩码吞掉；报告里会登记 `masked_head`）。

`selftest` 实测（`python -I -B tools/conformance/compare_runs.py selftest`，238/238 GREEN）对这 7 条各跑三组断言：

| ident | recog 取值（oracle→rust） | 改后 | 改前（删 head_compare） | 不外溢（改 param） |
| ---: | --- | --- | --- | --- |
| 54 | 7 → 8 | `rc 1` / `field=recog` | `rc 0` | `rc 1` / `field=param` |
| 5008 | 100 → 200 | `rc 1` / `field=recog` | `rc 0` | `rc 1` / `field=param` |
| 1106 | 0x1234 → 0x5678 | `rc 1` / `field=recog` | `rc 0` | `rc 1` / `field=param` |
| 752 | 3 → 4 | `rc 1` / `field=recog` | `rc 0` | `rc 1` / `field=param` |
| 811 | 10 → 11 | `rc 1` / `field=recog` | `rc 0` | `rc 1` / `field=param` |
| 766 | 0 → 5 | `rc 1` / `field=recog` | `rc 0` | `rc 1` / `field=param` |
| 213 | 0 → 2 | `rc 1` / `field=recog` | `rc 0` | `rc 1` / `field=param` |

**独立命令行复核（不依赖 selftest，ident 213）**：

```sh
python -I -B tools/conformance/compare_runs.py \
  --oracle E:/tmp/worker66-scratch/recog-demo/oracle \
  --rust   E:/tmp/worker66-scratch/recog-demo/rust \
  --out    E:/tmp/worker66-scratch/recog-demo/report-after.json
# => VERDICT: FAIL (exit 1)
#    [frame] idx=0 ident=213 field=recog offset=None oracle=0 rust=2
#    rc=1

python -I -B tools/conformance/compare_runs.py \
  --oracle E:/tmp/worker66-scratch/recog-demo/oracle \
  --rust   E:/tmp/worker66-scratch/recog-demo/rust \
  --rules  E:/tmp/worker66-scratch/recog-demo/rules-pre.json \
  --out    E:/tmp/worker66-scratch/recog-demo/report-before.json
# => VERDICT: PASS (exit 0)
#    DIFFS: 0; whitelisted: masked_head ident=213 field=recog x1
#    rc=0
```

即：**修复前这条帧“看起来一样”（PASS）**，修复后同样的输入判 FAIL。这正是本次要消除的假绿形态。

---

## 4. 两颗牙（真实 rc）

### 牙①：删掉某条 `head_compare` ⇒ 对应用例红

对 ident 213 的临时副本删掉 `head_compare`（`E:/tmp/worker66-scratch/recog-demo/rules-pre.json`），同一对帧：
**内置规则 `rc 1`（FAIL, field=recog） → 删除后 `rc 0`（PASS, masked_head）**。
`selftest` 里另有聚合断言：一次性删掉 7 条的 `head_compare` 后，7 对 recog 差异**全部 `rc 0`**——即上面 7 条「改后 rc 1」断言会全红，证明它们真的在咬。

### 牙②：给 ActorId 类加 `head_compare` ⇒ 跨栈真跑会红

取 ident **42 `SM_USERNAME`**（`Recog=processMsg.ActorId`），在临时副本里硬加 `head_compare:["recog"]`，用**真实跨栈形态**做帧对：C# `HUtil32.Sequence()` 实测 `255025409` vs Rust `EcsStore::alloc_id` `275`（`docs/window-run-sheet-20261021.md:732`）。

```sh
# 内置规则（42 的 recog 走全局掩码）
python -I -B tools/conformance/compare_runs.py --oracle .../o42 --rust .../r42 --out .../report-42-real.json
# => PASS (exit 0)，rc=0

# 滥用副本（42 加 head_compare:["recog"]）
python -I -B tools/conformance/compare_runs.py --oracle .../o42 --rust .../r42 \
  --rules .../rules-abuse-42.json --out .../report-42-abuse.json
# => FAIL (exit 1)，diffs=[(42, recog, 255025409, 275)]，rc=1
```

⇒ ActorId 是会话作用域标识，两侧命名空间必然不同；把它取回比较会把**必然差异**判成 FAIL。所以本卡只取回语义值，**没有给任何 ActorId 类字段加 `head_compare`**；`selftest` 里还有一条不变式断言钉住「42 不加 `head_compare`」。

---

## 5. 门禁（全部实跑）

| 门禁 | 命令 | 结果 |
| --- | --- | --- |
| 比较器自检 | `python -I -B tools/conformance/compare_runs.py selftest` | **GREEN 238/238**，rc=0（改前 198/198 ⇒ **计数未减少，+40**） |
| 窗口工具自检 | `python -I -B tools/conformance/card_window.py --selftest` | **GREEN 186/186**，rc=0 |
| T 预约表（合并树） | `python -I tools/whitelist/t_reservation_gate.py check` | **GREEN**：rows=208 whitelist=208 live=17 landed=191 malformed=0 fence_ok=True/True max=T-222，rc=0 |
| T 预约表自检 | `python -I tools/whitelist/t_reservation_gate.py selftest` | **GREEN 18/18**，rc=0 |
| 冲突标记 | `python -I tools/hygiene/conflict_marker_gate.py` | **OK（0 markers across 7398 tracked files）**，rc=0 |
| 写集 | `git diff --stat de7cb7779..HEAD` | `tools/conformance/rules.json`、`tools/conformance/selftest_impl.py`、`docs/`；未碰 `mir2_probe.py`/`param_discovery.py`/`src/**` |
| 批准状态 | 脚本比对 `approved_by` | 7 条仍全部 `PENDING …`（未动） |

---

## 6. 影响：哪些判据变严了，窗口要注意什么

修复后，下列字段从「永远不比」变成「逐值比」：

| ident | 新增判据 | 可能的真差异来源 |
| ---: | --- | --- |
| 54 | 地图音乐号 | 两侧 `Envir` 地图 Flag 不一致 |
| 5008 | 元宝数量 GameGold | 基线库 `characters.GameGold` 不一致 |
| 1106 | 客户端文件 CRC1 | 两侧部署的客户端/版本配置不一致（该帧在 `RM_LOGON` 每登录都发） |
| 752 | AntiMagic | 属性重算差异（tier 分类属战斗数值面） |
| 811 | BonusPoint | 加点/属性派生差异 |
| 766 | 区域状态位 | 地图 `FightZone/SafeArea` 或行会战状态不一致 |
| 213 | 攻击模式 | 玩家 `AttatckMode` 不一致 |

⇒ **这不是回归风险，而是把已有的假绿改成真判据**。若窗口因此冒出 FAIL，应当先核实两侧部署/基线是否一致（正是该比的内容），**不得**用 `--allow-*` 或重新加掩码盖掉。R 级面上，`752` 的新判据属战斗数值，窗口判级仍按 `AGENTS.md` 的 tier 策略。

---

## 7. 行号核验与未改清单

- 本卡新增/引用的 oracle 行号均 `grep -n`/`sed -n` 复核：`PlayObject.Base.cs:894`（54）、`:938`（5008）、`:1208`（811）、`:285`（766，位组装 `:270-284`）；`PlayObject.Message.cs:1690`（1106，`case RM_LOGON` 内）、`:1675`/`:1935`（752）；`PlayObject.cs:1146`（213）；`PlayObject.Message.cs:1663`（42 对照）。
- 修正的旧文案：752 原 reason 写“body 逐字节比”不实（该帧无 body），已在 reason 与本文写实。
- **未改**：`approved_by`、`tier`、`action`、任何 `head_mask`/`ranges`；`tools/probe/mir2_probe.py`、`tools/probe/param_discovery.py`、`src/**`、`crates/**`；`compare_runs.py` 的 `mask_for` 语义（`head_compare` 是既有机制，本卡只使用它）。

---

## 8. 残留物

- `E:/tmp/worker66-scratch/recog-demo/`：改前/改后与牙②的合成产物、报告与临时规则副本（仓库外；可整目录删除）。
- 未起栈、未连库、未占端口。
- 分支 `card/rules-recog-unmask` 已推 walgit；未合并、未自签 review、未删 worktree。
