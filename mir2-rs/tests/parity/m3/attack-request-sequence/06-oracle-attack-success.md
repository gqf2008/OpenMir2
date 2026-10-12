# M3②③ `AttackSuccess` 调用条件 —— oracle 逐行 + Rust 同形核对（owner 2026-10-12 22:35 第 ① 条）

本文件回答 owner 22:35 第 ① 条：**`AttackSuccess` 到底在什么条件下被调用**，C# 与 Rust
是否同形，并给出「C# `hit_gate 19 → attack_success 3`」的正确读法（含 `atk_power_base` 的计数归属）。

---

## 1. oracle：`AttackSuccess` 的调用点与取数条件

### 1.1 调用点（`src/M2Server/Player/PlayObject.Attack.cs`）

```csharp
24  protected void AttackDir(IActor targetObject, short wHitMode, byte nDir)
25  {
26      IActor attackTarget = targetObject ?? GetPoseCreate();      // ← 目标解析（唯一入口）
...
72      bool canHit = false;
73      int nPower = GetAttackPowerHit(wHitMode, GetBaseAttackPoewr(), attackTarget, ref canHit);
74      SkillAttackDamage(wHitMode, nPower);
75      AttackDir(attackTarget, nPower, nDir);                      // ← BaseObject.Attack.cs:11 → _Attack
76      SendAttackMsg(GetHitMode(wHitMode), Dir, CurrX, CurrY);
77      AttackSuccess(wHitMode, nPower, canHit, attackTarget);       // ← 无条件调用（实参可 null）
78  }
```

⇒ `AttackSuccess` 在**每次进入 `PlayObject.AttackDir`** 时**无条件被调用**（第 77 行），
与「是否命中」「是否过门」无关 —— 它甚至拿到的是 `attackTarget`（可能为 `null`）。

### 1.2 取数条件（`PlayObject.Attack.cs:215`）

```csharp
215 private void AttackSuccess(short wHitMode, int nPower, bool canHit, IActor targetObject)
216 {
217     if (targetObject == null)
218     {
219         return;                                                  // ← 无目标：0 抽，直接返回
220     }
...
225     ushort nWeaponDamage = (ushort)(M2Share.RandomNumber.Random(5) + 2 - AddAbil.WeaponStrong);
```

⇒ **唯一的取数门是 `targetObject != null`**：有目标 ⇒ 该行**恰好 1 次 `Random(5)`**（不分命中/躲闪）；
无目标 ⇒ 一次都不抽。`attack_success` 站点（hook PDB 归因 `PlayObject.Attack.cs:223`）就是这一次
（见 §3 行号说明）。

### 1.3 同一 `AttackDir` 里与命中门的关系

`AttackDir(attackTarget, nPower, nDir)`（第 75 行）进 `BaseObject.AttackDir` → `_Attack`
（`src/M2Server/Actor/BaseObject.Attack.cs`）：

```csharp
11  protected bool AttackDir(IActor attackTarget, int nPower, byte nDir)
12  {
13      Dir = nDir;
14      if (_Attack(nPower, attackTarget)) { SetTargetCreat(attackTarget); return true; }
15      return false;
16  }
30  internal bool _Attack(int nPower, IActor targetObject)
31  {
32      if (targetObject == null) return false;                       // ← 无目标：不抽命中门
33      bool result = false;
34      if (IsProperTarget(targetObject))                             // ← 非正当目标：不抽命中门
35      {
36          if (targetObject.HitPoint > 0)                            // ← 目标 HP<=0：不抽命中门
37          {
38              if (HitPoint < M2Share.RandomNumber.RandomByte(targetObject.SpeedPoint))  // :41 命中门
39                  nPower = 0;
40          }
41      }
42      else { nPower = 0; }
43      if (nPower > 0)
44      {
45          nPower = targetObject.GetHitStruckDamage(this, nPower);    // :48 → Base.cs:792/793
46          if (nPower > 0) { targetObject.StruckDamage(nPower); ... } // → PlayObject.cs:2957…
47      }
...
```

⇒ **命中门（`RandomByte(SpeedPoint)`）的条件 = 有目标 ∧ IsProperTarget ∧ 目标 HP>0**；
**`attack_success` 的条件 = 有目标**。因此对**玩家攻方**：

| 状态 | 抽数序列（oracle） |
|---|---|
| 无目标 | `GetBaseAttackPoewr` 的 `a`(+`b`) 两抽；`_Attack`/`AttackSuccess` 0 抽 |
| 有目标 + 非正当目标 / 目标 HP<=0 | `a,b`；**不抽门**；`succ` 1 抽 |
| 有目标 + 门内 | `a,b,gate`；命中则 `hsd[,hsd2],durab`；最后 `succ` 1 抽 |
| 有目标 + 门外（躲闪） | `a,b,gate`；`succ` 1 抽 |

⇒ **恒有 `attack_success` 的击数 = 「玩家攻方有目标」的击数 ≥ 玩家 `hit_gate` 的击数。**
这是本条的判定式：若某侧出现 `attack_success < 玩家 hit_gate`，才说明它漏调/多调了 `AttackSuccess`。

---

## 2. Rust 侧同形核对

`mir2-rs/crates/world/src/combat.rs`：

```rust
830  pub fn attack_success_weapon_damage(rng: &mut impl RandomSource) -> u16 {
831      (rng.random_below(5) + 2) as u16                 // = PlayObject.Attack.cs:225 的 Random(5)
832  }
...
848  pub fn attack_dir(rng, attacker: &mut Entity, target: &mut Entity, last_hitter, config) -> AttackOutcome {
849      let mode = AttackPowerMode::for_attacker(attacker);
850      let outcome = attack_with(rng, attacker, target, last_hitter, config, mode);   // = _Attack
...
857      if matches!(mode, AttackPowerMode::Player { .. }) {
858          // AttackSuccess(..., targetObject)：目标非空 ⇒ 抽 Random(5)
859          let _ = attack_success_weapon_damage(rng);
860      }
861      outcome
862  }
```

- **调用结构同形**：`attack_with`（= `_Attack`）之后，**玩家攻方**再补 `attack_success_weapon_damage`
  （= `AttackSuccess` 的 `Random(5)`）；怪物攻方不补（`AttackSuccess` 是 `PlayObject` 私有方法）。
- **取数条件同形**：Rust `attack_dir` 的签名是 `target: &mut Entity`（**非 Option**）——它只在
  「已解析到目标」时被调用（`world.rs:703`）；无目标的空打走
  `World::attack_swing`（`world.rs:243`，只抽攻击力、不碰门/不碰 `succ`）。
  ⇒ Rust 的「无目标 ⇒ 不抽 `succ`」与 C# 的「`targetObject == null ⇒ return`」**行为等价**。
- **门同形**：`combat.rs:758` `if target.hit_point > 0 && attacker.hit_point < rng.random_byte(target.speed_point)`
  —— 与 `BaseObject.Attack.cs:41` 同条件（且 `random_byte` 是 `&&` 右操作数，只要左条件成立就必抽，同 C#）。

**结论：Rust `combat.rs:831` 不是「无条件调」** —— 它的调用点是 `attack_dir`，而 `attack_dir` 只在有目标
时进入；玩家攻方有目标时**恰 1 抽**。这与 oracle 第 77/217/225 行同形。⇒ **不是实现缺陷。**

---

## 3. 「C# `hit_gate` 19 → `attack_success` 3」的正确读法（owner 22:35 第 ② 条的一半）

**`hit_gate` 次数不等于「玩家到达命中门的击数」**：C# 侧那 19 次 `_Attack@41` 里，
**16 次属于非玩家攻方**（`BaseObject.GetAttackPower@892` 的 16 个 base 单元），
只有 **3 次是玩家攻方**；而这 3 次与 `attack_success`（3）**完全相等**。

| 站点 | C# 总 | 其中 base 单元（非玩家） | 其中 player 单元（玩家） | Rust（全为 player） |
|---|---|---|---|---|
| `atk_power_base` / `atk_power_a` | 16 / 120 | 16 | 120 | 0 / 120 |
| `hit_gate` | 19 | **16** | **3** | 38 |
| `attack_success` | 3 | 0（`AttackSuccess` 是 `PlayObject` 私有） | **3** | 38 |
| `hit_struck_damage`(+2) | 17(+4) | 16(+4) | 1(+0) | 19(+0) |
| `struck_durability` | 1 | 0 | 1 | 19 |

⇒ 玩家侧 `hit_gate(3) == attack_success(3)`，**`AttackSuccess` 的调用关系两侧同形、无多/漏调**。
owner 22:35 观察到的「19 → 3」是把**非玩家 base 单元的 16 次门**算进了玩家到达门数。

真正两侧不同的是**「玩家攻方有目标的击数」**：C# **3/120**、Rust **38/120**（见 `05-target-availability.txt`：
C# 只有 k=1..3 有目标，Rust k=1..38 有目标直到目标 HP 归零）。

### 3.1 行号说明（PDB 归因）

hook 记录的 `attack_success` 站点是 `PlayObject.Attack.cs:223`，源码里 `Random(5)` 在 `:225`
（`:221` 是麻痹分支 `Random(AntiPoison+AttackPosionRate)`，属 `divert=0`）。JIT/PDB 把该调用归到相邻行；
`rng-isolate-attack/attack_sites.tsv` 已按实测把 `attack_success` 绑定到 `:223`（与 `:221` 区分）。
