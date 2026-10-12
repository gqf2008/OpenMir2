# M3②③ 攻击 `AttackSuccess` 调用条件 + 每击取数请求序列定位 —— 证据包（2026-10-12，m3cs / openmir2-worker-5）

卡：`openmir2-m3-combat-skills-align`（issue `a2ef74aa`）· worktree `m3cs` · branch `worktree-m3-combat-skills`
· lane `m3cs` / principal `openmir2-worker-5`。

本片回答 owner 2026-10-12 **22:35** brief 顶部第 ①–④ 条（**离线段**：零端口、零栈、零 `.rs`）。
数据源是上一步已入库的两相真机原始件（`evidence-rng-isolate-attack-streamcompare-20261012/`），
本包把四条流水复制进 `provenance/` 以自洽可复算（sha256 见 `provenance/00-provenance.txt`）。

## 结论（逐条对 owner 22:35）

1. **① `AttackSuccess` 的调用条件 = 「有目标」**（`06-oracle-attack-success.md`）：
   oracle `PlayObject.AttackDir`（`PlayObject.Attack.cs:24`）在 `:77` **无条件调用** `AttackSuccess`；
   `AttackSuccess`（`:215`）在 `:217-220` 对 `targetObject == null` 提前返回，`:225` 的 `Random(5)` 只要
   **目标非空即恰 1 抽**（不分命中/躲闪）。Rust `combat.rs:830/848` 的 `attack_dir` 与 oracle **同形**
   （玩家攻方 + 有目标 ⇒ `attack_with` 后恰 1 抽 `attack_success_weapon_damage`；无目标走
   `World::attack_swing`，不抽 `succ`）。⇒ **Rust 不是无条件调，不是实现缺陷。**
2. **② C# `hit_gate 19 → attack_success 3` 是把 base 单元算进了玩家门**（`08-base-unit-attribution.txt`）：
   19 次门里 **16 次属非玩家攻方**（`BaseObject.GetAttackPower@BaseObject.Base.cs:892` 的 16 个 base 单元），
   **玩家门只有 3 次，恰等于 `attack_success`（3）**。两侧 `AttackSuccess` 调用关系同形、无多/漏调。
   真正不同的是**「玩家攻方有目标的击数」：C# 3/120 vs Rust 38/120**（`05-target-availability.txt`：
   C# 目标只在 k=1..3 存在，Rust k=1..38 存在到目标 HP 归零）。
   停点在**目标解析** `IActor attackTarget = targetObject ?? GetPoseCreate();`（`PlayObject.Attack.cs:26`
   → `BaseObject.cs:1253` → `Envirnoment.cs:1203` 的 `!Ghost && (!boFlag || !Death)`）：k>=4 目标格为空 ⇒
   `_Attack`（`BaseObject.Attack.cs:32`）与 `AttackSuccess`（`:217`）都提前返回 ⇒ 无门、无 `succ`。
   不是拒绝门（120 条 `CM_HIT` 全部进 `AttackDir`、`atk_power_a/b` 各 120），不是目标死亡
   （C# `death_k=None`、`min_hp=18`），不是掉线（RunGate 全程只到最后 21:57:44 才两连接断开）。
3. **③ 第一处请求分歧已定位**（`07-first-divergence.txt`）：`(kind,arg)` 逐次比分到 **1-based index=2**：
   C# 是**非玩家 base 单元**的门 `hit_gate(Next(max),8)`，Rust 是玩家单元的 `atk_power_b(Next(max),10)`
   —— **class = base_unit**。触发条件：C# 流首是非玩家 `atk_power_base(Next(max),1)`，其 `(kind,arg)` 与
   Rust 首个 `atk_power_a(Next(max),1)` **完全相同**（都是 `Random(1)`）⇒ 第 1 次不比出差异，第 2 次即因
   base 单元的 `hit_gate` 显形，此后整体推移。**玩家路径本身无分歧**：120 个 player 单元的取数模板
   两侧均 0 违规（`02/03-analyze-*`）。
   - 机判工具：`per_strike_requests.py`（`selftest` 8/8；玩家路径多抽 / 次序颠倒 / 单元数不等**必红**；
     base 单元分类为 `base_unit`，不误判为 `player_path`）。
4. **④ 因果登记（不修）**：目标 maxhp=19（`-AttackVictimHp 30000` 被 `RecalcLevelAbilitys`
   `PlayObject.cs:5295` 按 L1 重算）确实把「有目标窗口」压短（Rust 在 k=38 目标 HP 归零后即无目标），
   但**不是** C# 停在 k=4 的直接成因（C# 目标全程 `min_hp=18`、未死）。C# 为何在 k=4 起目标格为空，
   本片只能把停机点钉在**目标解析**（上面第 2 条），**其成因属夹具/场景**（建议夹具逐 k 记 `victim` 的
   `CurrX/CurrY + Ghost/Death`）——**转 m3bag**。④ 本体（pin maxhp 未生效）仍归 m3bag。

**判据口径未动、未放宽**：DoD② 仍是真机 `attack_diff.py diff` 逐 k=0；本片**不宣布达成**。

## 文件

| 文件 | 内容 |
|---|---|
| `01-selftest.txt` | `per_strike_requests.py selftest`（8/8 牙齿） |
| `02-analyze-stack2.txt` / `.json` | cA(120) vs rA(120)：站点计数 / player 单元模板 / 第一分歧 class=base_unit |
| `03-analyze-stack1.txt` / `.json` | 交叉印证（含 20 冒烟 + 两次 120；`--allow-count-mismatch` 因两侧会话构成不同） |
| `04-per-strike-table-stack2.txt` | 120 击逐 k 的**每击取数请求序列**（C#/Rust 并排，带 hit/dodge） |
| `05-target-availability.txt` | ② 的读数：C#/Rust 有目标的击号、目标消失首击、death_k/min_hp、struck/ERR、gate 断开 |
| `06-oracle-attack-success.md` | ① 的 oracle 逐行 + Rust 同形核对 + 「19→3」正确读法 |
| `07-first-divergence.txt` | ③ 的第一处分歧站点 + 触发条件 |
| `08-base-unit-attribution.txt` | ② `hit_gate=19 = base 16 + player 3` 的逐单元归属 |
| `per_strike_requests.py` | ③/① 的机判工具（`analyze` / `table` / `selftest`） |
| `session_target_availability.py` | ② 的会话件读数工具 |
| `provenance/` | 四条流水副本（C# 专用流 ×2、Rust trace ×2）+ 溯源 |
| `commands.txt` | 全部命令 |

## 溯源

- 四条流水 sha256/字节见 `provenance/00-provenance.txt`（与
  `evidence-rng-isolate-attack-streamcompare-20261012/provenance/` 同源同值）。
- `SHA256SUMS.txt` 按**仓库字节**（LF 归一）口径；`verify_sha256sums.py --check-extra` GREEN。

## 残留物 / 转出

- 本片**未起栈、未占端口**，无需「窗口已释放」；端口实拍沿用上一步
  `evidence-rng-isolate-attack-twophase-20261012/window/19-ports-after.txt`（十端口 free）。
- 转 m3bag：④（pin maxhp 未生效）+ **C# 目标在 k=4 消失的场景成因**（建议夹具逐 k 记 victim 位置/状态）。
- 转 m3bag：C# 会话件里同一次 `SM_STRUCK` 被 attacker/victim 两个客户端各记一行（`other_observer=1`），
  `attack_diff.py align` 的 `hits` 可能因此多算（cA `hits=2` 而 RNG 只有 1 次玩家命中）。
