# 攻击成功时的武器耐久磨损与其下行帧（卡 `openmir2-attack-weapon-wear-642`）

- 基线：`walgit/master` = **`bb4317a01`**；worktree `E:/tmp/wt-worker-67`，分支 `card/attack-weapon-wear-642`。
- tier = **R**（RNG 抽数面 + 持久化面 + 战斗数值面）。
- 写集：`mir2-rs/crates/world/src/combat.rs`、`mir2-rs/bins/game-svr/src/`、`mir2-rs/tests/parity/whitelist.md`、`docs/`；
  **外加已在线程声明的 additive 扩张** `mir2-rs/crates/world/src/world.rs`（3 处，见 §2.3，comment
  `e790c2459b70fd3c5f3ca9e253dc3019d8c58d5e`）与 `mir2-rs/tests/parity/T-RESERVATIONS.md`（`next_t.py` 自动追加）。

---

## 0. 结论先行

1. **oracle 的逃逸条件只有「无目标」，不是「没打中」**：`AttackSuccess`（`PlayObject.Attack.cs:215`）在
   `targetObject == null` 时 `return`（`:217-220`），否则**无条件**抽 `Random(5)`：
   `nWeaponDamage = (ushort)(Random(5) + 2 - AddAbil.WeaponStrong)`（`:225`）。
   躲闪、伤害 0、目标不还手，**都照扣攻击者的武器耐久**。
2. **本仓此前是「抽了、结果丢掉」**：`combat.rs` 的 `attack_success_weapon_damage(rng)` 只抽不用。
   本卡把它**同一次抽数**用掉（不新增、不移动取数），扣耐久并出 202 / 642。
3. **帧面**：破零支先 `SendDelItems`（`SendSocket` 直发 ⇒ `SM_DELITEM(202)`）再 `SendMsg(RM_DURACHANGE)`
   （自己队列 ⇒ `SM_DURACHANGE(642)`，`Recog=Dura / Param=槽位 / Tag,Series=DuraMax`）；
   尾桶门 `Math.Abs((nDura / 1.03)) != nDuraPoint`（`:1466`）**两侧精度不对称**（LHS 未取整
   `double`、RHS 取整 `int`）⇒ 只要磨损真的发生就**必再发一条**（非破零支恰一条、破零支
   `202 + 642(0) + 642(0)`）——连怪癖一起搬，**不做「去重/取整」修正**（reviewer-6 `5ae805d9` I-1）。
4. **RNG 账本两侧逐字节相同**：固定攻击场景的 trace `diff` 为空、`sha256` 相同（§4）。
5. **场景可跑**：`card_window.py --dry-run --scenario tools/probe/scenarios/equip_struck_save.json` **rc=0**；
   **真机窗口待授权**（本条判据见 §6、T-266）。

---

## 1. oracle 逐段（file:line，逐行核过）

| # | 位置 | 原文要点 |
|---|---|---|
| 1 | `src/M2Server/Player/PlayObject.Attack.cs:215` | `private void AttackSuccess(short wHitMode, int nPower, bool canHit, IActor targetObject)` |
| 2 | `:217-220` | `if (targetObject == null) { return; }` —— **唯一**逃逸条件 |
| 3 | `:225` | `ushort nWeaponDamage = (ushort)(M2Share.RandomNumber.Random(5) + 2 - AddAbil.WeaponStrong);` |
| 4 | `:226-229` | `if ((nWeaponDamage > 0) && (UseItems[ItemLocation.Weapon] != null) && (UseItems[ItemLocation.Weapon].Index > 0)) { DoDamageWeapon(nWeaponDamage); }` |
| 5 | `:77` | `AttackDir` 末尾：`SendAttackMsg(...)` 之后**无条件** `AttackSuccess(wHitMode, nPower, canHit, attackTarget)`（`attackTarget = targetObject ?? GetPoseCreate()`，`:26`） |
| 6 | `:1436-1441` | `protected void DoDamageWeapon(ushort nWeaponDamage)`；`UseItems[Weapon] == null \|\| Index <= 0 ⇒ return` |
| 7 | `:1442-1444` | `ushort nDura = Dura; int nDuraPoint = HUtil32.Round(nDura / 1.03); nDura -= nWeaponDamage;` |
| 8 | `:1445` | `if (nDura <= 0)` —— 对 `ushort` 等价 **`nDura == 0`**（不是「<=」语义上的负数） |
| 9 | `:1447-1448` | `nDura = 0; UseItems[Weapon].Dura = nDura;`（先置 0） |
| 10 | `:1451` | `this.SendDelItems(UseItems[ItemLocation.Weapon]);`（**此时 `Index` 仍 > 0**、`Dura = 0`） |
| 11 | `:1459-1460` | `UseItems[Weapon].Index = 0;` → `SendMsg(RM_DURACHANGE, ItemLocation.Weapon, nDura /*=0*/, DuraMax, 0)` |
| 12 | `:1462-1464` | 非破零支：`UseItems[Weapon].Dura = nDura;` |
| 13 | `:1466-1469` | `if (Math.Abs((nDura / 1.03)) != nDuraPoint) { SendMsg(RM_DURACHANGE, ItemLocation.Weapon, Dura, DuraMax, 0); }` —— **两侧精度不对称**：LHS 是**未取整**的 `double`（`ushort / 1.03`），RHS `nDuraPoint` 是 `int`（提升为 `double` 后精确比较）⇒ 只要磨损发生（`n_dura ≥ 1`）LHS 几乎恒非整数 ⇒ **必再发一条**；**破零支也会走到** ⇒ 常见「202 + 642(0) + 642(0)」。**不得**把两侧都 `Round`（`d = 1` 时 1909 个 `before` 值会分歧） |
| 14 | `src/M2Server/Player/PlayObject.Base.cs:1648-1665` | `SendDelItems`：`MakeMessage(SM_DELITEM, ActorId, 0, 0, 1)` + `EDCode.EncodeBuffer(clientItem)`；`StdMode == 50` 追加 `" #" + Dura`；**`SendSocket` 直发**（不经队列） |
| 15 | `src/M2Server/Player/PlayObject.Message.cs:1803-1804` | `RM_DURACHANGE` → `SendDefMessage(SM_DURACHANGE, processMsg.nParam1 /*Dura*/, processMsg.wParam /*槽位*/, LoWord(nParam2) /*DuraMax*/, HiWord(nParam2))` |
| 16 | `src/M2Server/Actor/BaseObject.Message.cs:34` | `SendMsg(int wIdent, int wParam, int nParam1, int nParam2, int nParam3, string sMsg = "")`（⇒ 第 2 实参是**槽位**、第 3 是 **Dura**） |
| 17 | `src/OpenMir2/HUtil32.cs:131-134` | `Round(double r) = (int)Math.Round(r, MidpointRounding.AwayFromZero)` |
| 18 | `src/M2Server/Player/PlayObject.cs:4424-4426` / `:4563-4566` | `AddAbil.WeaponStrong = (byte)clientItem.Item.SpecialPwr`（StdMode 5/6 与 24/26，且 `SpecialPwr ∈ 1..=10`；**赋值不是累加**） |

### 1.1 与卡面的一处订正（已在线程 comment 记录）

卡面写「随后扣攻击者武器耐久、`SendRefMsg(RM_DURACHANGE…)`」——**oracle 用的是 `SendMsg`，不是 `SendRefMsg`**
（`PlayObject.Attack.cs:1460` / `:1468`）。差别是可观测的：`SendRefMsg` 会投给 ±12 格的**其他**玩家，
`SendMsg(this)` **只投自己**。本实现按 `SendMsg`（用例 `attack_success_wears_weapon_and_sends_one_642_to_attacker_only`
断言同图旁观者零帧）。`SendDelItems` 则是 `SendSocket` 直发 ⇒ **202 先于 642**。

---

## 2. 实现落点

### 2.1 `crates/world/src/combat.rs`（本卡独占段）

| 项 | 说明 |
|---|---|
| `weapon_damage_with_strong(raw, weapon_strong)` | `(ushort)(raw - WeaponStrong)` 的 unchecked 回绕（**不** saturating） |
| `do_damage_weapon(&mut UseItemSlot, n)` → `WeaponWearOutcome` | `DoDamageWeapon` 的逐行等价：回绕减、`== 0` 破零、`Round(Dura/1.03)` 桶位门；产出**按发帧序**的 `WeaponWearFrame::DelItem` / `::DuraChange` |
| `attack_success_weapon_wear(rng, &mut Entity)` | 当前 head 的生产口径（`weapon_strong = 0`；见 T-266 与后继卡 `openmir2-ability-weaponstrong`）：**只**调 `attack_success_weapon_damage` 一次，随后纯数值/写槽 |
| `attack_success_weapon_wear_with_strong(…)` | 带 `WeaponStrong` 的同一逻辑（用例钉住回绕语义） |
| `attack_dir_grouped_poison(…) -> (AttackOutcome, bool, Option<WeaponWearOutcome>)` | 第三个返回值 = 本次磨损。**取数位置与次数逐字不变**：原来那行 `let _ = attack_success_weapon_damage(rng);` 原地换成 `weapon_wear = attack_success_weapon_wear(rng, attacker);`（**同一行、同一位置、同一次抽取**） |

### 2.2 帧序不变量

- 世界侧只产**帧序列**（`Vec<WeaponWearFrame>`），**不编码字节**；
- 宿主按序列发：`DelItem` ⇒ `route_to_gate` 直发（202，不经队列）；`DuraChange` ⇒ `SpellFrameBuf`
  按 `(byte)RM_DURACHANGE = 141` 入队，末了 `flush`；
- ⇒ 同一击 **202 先于 642**；两者都只投攻击者自己。

### 2.3 `crates/world/src/world.rs`（**已声明的 additive 扩张**，逐字照抄 `weapon_breaks` 模式）

1. `TickRecord` 新增 `pub weapon_wears: Vec<WeaponWearEvent>`（`Default` 派生；28 处测试字面量走 `..Default::default()`）；
2. `World` 新增 `pub(crate) pending_weapon_wears` + ctor 初始化 + `tick_tail` 转抄；
3. `attack_once`：调用点改收三元组；`weapon_wear.is_some()` 时**无条件**写回 `attacker.use_items`
   （既有 `weapon_outcome` 的写回是条件式，普通武器写不回去）并 push 事件。

`monai.rs` / `entity.rs` / `ability.rs` **未改**；`AttackOutcome` **不扩字段**（monai 的三处是完整字面量，
扩字段会编译红）——改用 `TickRecord` 事件通道。

### 2.4 `bins/game-svr/src/lib.rs`

- `drain_attack_weapon_wear(rec, online, gates, catalog)`：202 直发 + 642 入 `SpellFrameBuf`；
- `weapon_broken_body`：复用 `game_svr::slave::deleted_amulet_body`（oracle 同一个 `SendDelItems`；
  含 `StdMode == 50` 的 `" #"+Dura` 名尾）；
- `sim_loop` 里紧跟 `drain_weapon_durability` 之前调用（同属武器状态帧族）。

---

## 3. 用例（≥4）与牙（≥2）

### 3.1 世界侧（`crates/world/src/combat.rs` 的 `mod tests`）

| 用例 | 断言 |
|---|---|
| `do_damage_weapon_deducts_and_emits_642_when_bucket_changes` | `5000 → 4995` 恰一条 642（`Recog=4995`、`Tag=9000`、`Param=1`） |
| `do_damage_weapon_tail_gate_is_unrounded_double_vs_int` | `52 → 51`（`d = 1`）：oracle 的尾桶门 `51 / 1.03 = 49.514… ≠ Round(52/1.03) = 50` ⇒ **发一条 642**（「两侧都 Round」的错实现会漏发；reviewer-6 `5ae805d9` I-1） |
| `do_damage_weapon_break_emits_delitem_then_two_642` | 破零：`[DelItem(Index 快照>0, Dura=0), 642(0), 642(0)]`，`Index=0`、`DuraMax/Desc/MakeIndex` 保留 |
| `weapon_strong_greater_than_wear_wraps_like_unchecked_ushort` | `WeaponStrong=9 > 磨损`：`(ushort)` 回绕成大正数、`Dura` 反而变大、**不 panic** |
| `zero_weapon_damage_still_draws_but_does_not_wear` | `nWeaponDamage == 0`：**仍抽一次** `Random(5)`，不写槽、不发帧 |
| `swing_draws_no_weapon_dice_but_targeted_attack_draws_exactly_one` | 空挥流水**无** `Random(5)`；有目标恰 1 次 |

### 3.2 宿主侧（`bins/game-svr/src/lib.rs` 的 `attack_weapon_wear_tests`）

| 用例 | 断言 |
|---|---|
| `attack_success_wears_weapon_and_sends_one_642_to_attacker_only` | 有目标攻击 ⇒ 耐久 `-2..=6`；恰一条 642 只投攻击者（同图旁观者零帧）；`(Recog,Param,Tag,Series) = (Dura,1,9000,0)`；12B 无尾；逐字节等于 `smframe::sm_durachange` |
| `swing_without_target_wears_nothing_and_sends_no_frame` | `attack_swing` ⇒ `rec.weapon_wears` 空、耐久不变、零帧 |
| `broken_weapon_sends_202_before_642_recog_zero` | 帧序 `202 → 642(0) → 642(0)`；202 头 `(ActorId, 202, 0, 0, 1)`、`ClientItem.Dura=0`/`DuraMax=9000`/`MakeIndex=7`/`Desc` 恒 0 |

### 3.3 牙（实测）

| 牙 | 改坏点 | 变红用例 | rc | 还原 |
|---|---|---|---|---|
| 牙 1 | `combat.rs` 的 `attack_success_weapon_wear_with_strong` 门改成 `if false && …`（不结算） | `attack_success_wears_weapon_and_sends_one_642_to_attacker_only`（`rec.weapon_wears.len()==1` 失败） | **101** | `git checkout -- mir2-rs/crates/world/src/combat.rs`；`sha256sum` = `git show HEAD:<path> \| sha256sum` = **`77c2dd8d053d6ba68722578415ad6186f091c6fac22eca91acd004efbc2cdf56`**；`git status` 干净 |
| 牙 2 | `bins/game-svr/src/lib.rs` 的 `drain_attack_weapon_wear` 在 `DuraChange` 臂首行插入 `continue`（642 不发） | `attack_success_wears_weapon_and_sends_one_642_to_attacker_only` + `broken_weapon_sends_202_before_642_recog_zero`（帧数/次序失败） | **101** | `git checkout -- mir2-rs/bins/game-svr/src/lib.rs`；`sha256sum` 两侧同为 **`15aaa20ffd143c779eb70a05afa09d2ec2aebe31c6325f373dd2bdb5b4048448`**；`git status` 干净 |

两牙均**只用 `git checkout -- <file>` 还原**（无文本写回），还原后两文件 sha256 与 `git show HEAD:<path>` 逐字相同、`git status` 空。

---

## 4. RNG 流水不变（本卡最硬的一条）

**做法**：`hit_dice_rng_trace` 的固定攻击场景（玩家攻方 → 怪物守方，含毒门）在**改前**（`bb4317a01` 的
`E:/tmp/wt-wear-base`）与**改后**（本分支）各跑一次，产物即账本（`site` 逐次 + `order` 逐次）：

```text
# 改前：在 bb4317a01 的独立 worktree 里跑（本卡收尾时已清理 ⇒ 复现时重建）
git worktree add --detach E:/tmp/wt-wear-base bb4317a01
(cd E:/tmp/wt-wear-base/mir2-rs && M2_HIT_DICE_TRACE_PATH=E:/tmp/wear-trace-base.tsv \
   cargo test -p mir2-world --test hit_dice_rng_trace)
# 改后：本分支（返工后复跑；发帧门改动不影响抽数）
(cd mir2-rs && M2_HIT_DICE_TRACE_PATH=E:/tmp/wear-trace-fix.tsv \
   cargo test -p mir2-world --test hit_dice_rng_trace)
diff -u E:/tmp/wear-trace-base.tsv E:/tmp/wear-trace-fix.tsv            # rc=0，无输出
python -I -B mir2-rs/tests/parity/m3/rng-trace-probe/rng_ledger_diff.py diff \
  E:/tmp/wear-trace-base.tsv E:/tmp/wear-trace-fix.tsv                  # VERDICT IDENTICAL
```

两侧 `sha256` 相同：`41e2a59c98faac716c9e9cf9a28493f5891e3de31c242932bae07ad0d37c6dfa`。

流水（6 条，站点行号逐字未动）：`combat.rs:109` / `:111` / `:1016` / `:856` / `:1260`（毒门）/ `:1152`（`Random(5)`）。

**为什么行号也没漂**：新代码全部追加在 `combat.rs` 第 1609 行之后的**新段**；对既有函数的改动是**行数中性**的
（返回类型改写在同一行；`let mut poisoned` 合并成 `let (mut poisoned, mut weapon_wear)`；抽取行原位替换）——
所以六个攻击取数站点连 `file:line` 都与改前逐字相同。

---

## 5. 持久化面

世界侧把磨损写回 `Entity.use_items`；宿主 `sim_loop` 的既有 `sync_use_items_to_session`（G-06）在本 tick
全部排水之后、周期存档之前把实体槽整表回写会话 ⇒ `DB_SAVEHUMANRCD` 读到磨损后的 `Dura`（与既有受击方
护甲磨损同一条通道，本卡不新增写库路径）。

---

## 6. 残余与窗口项（登记 **T-266**）

1. **`AddAbil.WeaponStrong` 输入端未接（算术已 1:1）**——本跳 head 的 `bins/game-svr/src/ability.rs`
   尚无 `weapon_strong` 字段（`ability.rs:79` 原登记），`attack_success_weapon_wear` 生产口径传 **0**。
   **后继接线卡**：`openmir2-ability-weaponstrong`（认领车道，分支 `card/openmir2-ability-weaponstrong`
   head `f5a2e485b`，**未合入**）已加 `AddAbility::weapon_strong`（`ability.rs:373` 字段，`:847/914/925`
   在 StdMode 5/6/24/26 且 `SpecialPwr ∈ 1..=10` 时赋值）。**待该卡合入后接线，届时复用其字段**：
   把攻方 `AddAbility.weapon_strong` 作为 [`attack_success_weapon_wear_with_strong`] 的实参即可
   （本卡的 `weapon_wear` 通道与用例已就位）。
   **分叉面（补充登记，reviewer-6 `5ae805d9` I-2）**：
   - **(a) 模板 `SpecialPwr`**：部署 `stditems` 里 `StdMode ∈ {5,6,24,26} ∧ Source ∈ 1..=10` 共 **38 条**
     （`Source → StdItem.SpecialPwr`：`src/GameSrv/DB/MySqlDB.cs:36`；Rust 同源
     `crates/data/src/loaders.rs:149`），其中 StdMode 5/6 武器 4 条：毒魔双刃剑(3)/祖玛银蛇(3)/
     祖玛无极棍(3)/祖玛龙纹剑(5)（`stditems.Source`）。复跑：
     `mysql -h127.0.0.1 -uroot mir2_data -e "SELECT Id,Name,StdMode,Source FROM stditems WHERE StdMode IN (5,6,24,26) AND Source BETWEEN 1 AND 10"`。
   - **(b) `Desc[7]` 随机强化路径（更常见）**：`GameItemSystem.UpgradeRandomWeapon`（`GameItemSystem.cs:225-231`）
     以 `Random(10)==0` 写 `Desc[7] = (1 + up/2)`，`up = GetUpgrade(12,15) ∈ 0..12` ⇒ `Desc[7] ∈ 1..=7`；
     `GetUpgradeStdItem`（`:744-750`）在模板 `SpecialPwr >= 0`（几乎所有武器）时把 SpecialPwr **覆盖**为
     `Desc[7]`。Rust 侧同源已实现：`crates/formula/src/drop.rs:155`（写 `desc[7]`）与
     `bins/game-svr/src/bag.rs:581-583`（`(1..=10).contains(&d[7]) && t.special_pwr >= 0` ⇒ 覆盖）。
   - **后果不是「磨损量偏大」，而是常常相反**：`(ushort)(Random(5) + 2 - WeaponStrong)` 在
     `WeaponStrong > raw` 时**回绕成大正数**（如 65529），`DoDamageWeapon` 于是**给耐久加值、且不破零**
     （oracle 同款 unchecked 语义）。本实现当前恒减 2..6 —— 该分叉在接线前**真实存在**。
   - 算术语义已按 unchecked `ushort` 实现，并用 `weapon_strong_greater_than_wear_wraps_like_unchecked_ushort`
     钉住（`WeaponStrong=9 > raw` ⇒ 回绕、`Dura` 变大、不 panic）。
2. **R 级真机窗口（待授权）**：`tools/probe/scenarios/equip_struck_save.json` 的硬门是
   `wait_for [642,203]` + `wait_idents [642]`，本卡正是它的前置。**场景可跑**：离线 dry-run
   `python -I -B tools/conformance/card_window.py --dry-run --scenario tools/probe/scenarios/equip_struck_save.json`
   ⇒ **rc=0**；**真机窗口未跑**（不得写成「真机已对拍」）。可复跑命令：

   ```text
   python -I -B tools/conformance/card_window.py --card openmir2-attack-weapon-wear-642 \
     --scenario tools/probe/scenarios/equip_struck_save.json --out E:/tmp/win-attack-weapon-wear-642
   ```

   窗口判据：① Rust 侧收到**至少一条** `SM_DURACHANGE(642)`，`Recog = 扣后 Dura`、`Param = 1`、
   `Tag/Series = DuraMax`，且**只有自己收到**；② 与 C# 侧同场景的 642 载荷逐字节一致；
   ③ 若夹具耐久调到 2..=5 触发破零，则 `SM_DELITEM(202)` **先于** `Recog=0` 的 642，且随后武器槽
   `Index` 清零（存档侧）；④ 两侧 RNG 流水（`MIR2_RNG_LOG`）在同场景下逐项一致。
3. **`SendDelItems` 的「扣除道具」伴随帧**：oracle 的 `NeedIdentify == 1` 事件日志在 DoDamageWeapon 里是
   **注释掉的**（`PlayObject.Attack.cs:1452-1457`）⇒ 本卡无该帧；无需登记。
4. **帧族批处理口径（reviewer-6 `5ae805d9` Minor，登记）**：oracle 的 `CheckWeaponUpgrade`（`PlayObject.Attack.cs:31`，
   `AttackDir` 开头）**先于** `AttackSuccess`（`:77`）⇒ 同一 tick 的 `SM_BREAKWEAPON(1102)` 应先于
   202/642。本宿主按**帧族**批量排水（`drain_attack_weapon_wear` 在 `drain_weapon_durability` 之前，
   各自 flush）⇒ 跨 actor 且互相在 ±12 视野内时，1102 与 202/642 的相对序可能与 oracle 不同。
   **同 actor 不可观测**：`CheckWeaponUpgrade` 破零会清 `Index` ⇒ 随后 `AttackSuccess` 的
   `Index > 0` 门不过，两者不会共存。要跨 actor 逐字节，需把两个 drain 合成按事件序的单一队列（另卡）。
