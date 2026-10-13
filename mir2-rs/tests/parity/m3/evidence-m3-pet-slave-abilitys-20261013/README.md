# evidence-m3-pet-slave-abilitys-20261013

卡：`openmir2-m3-pet-slave-lifecycle`（M3⑨-3）**收口片 #2** —— 落地 ⑨-2 reviewer-5 的 **M-1**
（非阻塞 Minor，但 reviewer 明确要求「⑨-3 必须真落，否则 ⑨ 家族收口时该处会成为静默缺口」）。

- base = `refs/heads/master` `c8a3487b1`（origin == walgit）；分支 `worktree-m3-pet-slave-abilitys`。
- code head = `f9db62ae2`（唯一提交）。
- 零起栈、零占端口、未 merge、未自签 review；共享主检出未读写。

## 缺口是什么（reviewer-5 ⑨-2 M-1 的实测含义）

`BaseObject.MakeSlave`（`BaseObject.cs:1179-1209`）在设完 `Master`/`IsSlave`/`MasterRoyaltyTick`/
`SlaveMakeLevel`/`SlaveExpLevel` 后调 `monObj.RecalcAbilitys()`（`:1197`），再按
`if (WAbil.HP < WAbil.MaxHP) HP += (MaxHP-HP)/2` 补半血（`:1198-1201`）。

⑨-2 曾把这一步建模为 **no-op**（理由：基类怪 `RecalcAbilitys` 只是 `WAbil = Abil.Clone()`
并保留 HP/MP）。但 `BaseObject.RecalcAbilitys`（`BaseObject.Abil.cs:15-94`）末尾有

```csharp
if (Race >= ActorRace.Animal)      // Abil.cs:85-88，Animal = 50
{
    ApplySlaveLevelAbilitys();      // Abil.cs:133-170
}
```

`ApplySlaveLevelAbilitys` 会按 `SlaveExpLevel` 改写 `WAbil.DC` / `WAbil.MaxHP`（`Master != null`
时；变异骷髅/神兽两形态走专门支）。⇒ **HP 补半用的 `MaxHP` 是改过的**，本骨架此前用原
`max_hp`，与 oracle 分叉。三个派生类 `WhiteSkeleton`/`ElfMonster`/`ElfWarriorMonster` 的
`RecalcAbilitys` 覆写还额外改 `WalkSpeed`/`NextHitTime`/`WalkTick`。

## 本片实现（`crates/world/src/slave.rs::make_slave_adopt` 第 5 步）

| oracle | Rust | 判据 |
| --- | --- | --- |
| `ApplySlaveLevelAbilitys` 三类支（`Abil.cs:139-150`）：`DC` 高字节 `= round(Hi + 3*(0.3+exp*0.1)*exp)`；`MaxHP = Abil.MaxHP + round(Abil.MaxHP*(0.3+exp*0.1))*exp`（`exp==0` 回 `Abil.MaxHP`） | `apply_slave_level_abilitys` 三类支 | `make_slave_adopt_white_skeleton_branch_and_speed` |
| `ApplySlaveLevelAbilitys` else 支（`:151-167`）：`DC` 高字节 `+= 2*exp`；`MaxHP = _MIN(Abil.MaxHP + 60*exp, Abil.MaxHP + round(Abil.MaxHP*0.15)*exp)` | 同函数 else 支（`master.is_some()`） | `..._else_branch` / `..._uses_bankers_rounding` |
| `Race >= Animal(50)` 门（`:85-88`） | `if mon.race < ACTOR_RACE_ANIMAL { return }` | `..._skips_slave_level_abilitys_below_animal` |
| `WhiteSkeleton.RecalcAbilitys`（`WhiteSkeleton.cs:21-27`） | `apply_derived_slave_recalc` race 100 | `..._white_skeleton_branch_and_speed` |
| `ElfMonster.RecalcAbilitys`→`ResetElfMon`（`ElfMonster.cs:31-41`） | 同函数 race 113 | `..._elf_monster_speed` |
| `ElfWarriorMonster.RecalcAbilitys`→`ResetElfMon`（`ElfWarriorMonster.cs:35-39,105-110`） | 同函数 race 114 | `..._elf_warrior_speed` |
| `Math.Round(double)` = 银行家舍入（`MidpointRounding.ToEven`） | `f64::round_ties_even()` | `..._uses_bankers_rounding`（110*0.15=16.5→16） |
| `HUtil32.MakeWord(ushort,ushort)`（`:66-69`，`High` 只有低 8 位生效） | `make_word_u16` | 三类支/else 支 DC 断言 |

**登记未建模**（非本片改动）：else 支尾的 `WAbil.MAC = 0`（`Abil.cs:166`）——本骨架怪物无 MAC
字段、战斗不消费；`RecalcAbilitys` 其余怪物字段（`AntiPoison`/`AntiMagic`/`Weight*`）同。

## 文件

| 文件 | 内容 |
| --- | --- |
| `00-commands.txt` | 命令、读数、门禁、边界、红检汇总 |
| `01-oracle-anchors.txt` | 每处 oracle 锚的原文（`sed` 取证） |
| `02-change-diff.txt` | `git diff` 全量（3 个 `.rs`，含新增 6 用例） |
| `03-tests.txt` | `cargo test -p mir2-world --lib slave`（40 passed）原始输出 |
| `04-boundary.txt` | 冻结面空 + merge-tree rc=0 |
| `06-gate-fmt.txt` / `07-gate-clippy.txt` / `08-gate-test.txt` / `09-gate-msgcodegen.txt` | 四门禁读数 |
| `redchecks/RC{1..4}-{mutated,restored}.rc` | 改坏必红成对读数（一行） |

## 读数

- 四门禁：fmt **0** / clippy **0** / test **99 suites · 1165 passed · 0 failed · 22 ignored** / msgcodegen **604 rc=0**。
- 判据：`cargo test -p mir2-world --lib slave` = **40 passed / 0 failed**（原 34）。
- 红检：RC1/RC2/RC3/RC4 mutated 全 **rc=101**（4/1/3/1 用例红），restored 全 **rc=0**。
- 边界：`src sql MirClient` 空；`merge-tree` rc=0。
- 真机对拍（`run_hero_pet_pair.ps1`，召唤/诱惑/叛变/幽灵各站点两侧 diff=0）仍等独占窗口，
  **不在本片**（卡上 `not_verified` 不变）。
