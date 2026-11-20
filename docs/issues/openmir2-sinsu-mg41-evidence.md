# openmir2-sinsu-mg41-evidence — MG-41 SINSU(30) summon_ok 三颗牙红证据

**状态**：证据（只读驱动+变异重放）。**MG-41 本批不升格**。
**base**：master `2692d0a26`；装置 golden `sinsu.json`（`head` 见分支）；run_id `sinsu-*`。

## 三颗牙（每颗：变异点 → 断言名 → left/right → 还原）
> 均为 **Rust 侧**变异（`bins/game-svr/src/lib.rs` 的 `sinsu_parity_drive`），还原后 `sha=EQUAL`、golden sha 不变。

### 牙 A — race 113→100（证明「神兽=113」替身参数真进被测路径）
- 变异点：`sinsu_parity_drive` 里 `let mon = MonsterInfo { name:"神兽", race: 113, … }` 的 `race: 113 → 100`（`mon_race` 由 `mon.race` 派生入账本 `stub_monster_race`）。
- 红原文：`assertion \`left == right\` failed: [summon_ok] SinSu.stub_monster_race 不等`
- left/right（变异后一侧 vs golden）：`left = 100`（Rust，变异后） / `right = 113`（golden）。

### 牙 B — 表名 神兽→变异骷髅（**Rust 侧 `MonsterTable`** 变异）
- 变异点：`sinsu_parity_drive` 传入的 `MonsterInfo { name:"神兽" → "变异骷髅", race:113 }`（即 Rust 侧 `MonsterTable` 条目名）。**不是 C# 离线替身**（替身对任何名字都造得出）。
- 机制：召唤计划 `sMonName` 仍是 `Config.Dragon="神兽"` 去 `table.race_of("神兽")`，表已被改名 ⇒ `race_of=-1 ⇒ create_monster_oracle=None ⇒ 无实体`。
- 红原文：`assertion \`left == right\` failed: [summon_ok] SinSu.slave_count_after 不等`
- left/right：`left = 0`（Rust，变异后召不出） / `right = 1`（golden）。
- **B 证明不了 `new_slaves[0].name` 的比较**（它根本没走到实体名）——那是牙 C 的职责。

### 牙 C — 发出实体名 `神兽 → 神兽X`（**断言自检牙**，§12 行为牙）
- 变异点：`sinsu_parity_drive` 的 `new_slaves` 里 `e.entity().name` → `format!("{}X", e.entity().name)`（**只改发出的观测值**，不动表、不动召唤计划）。
- 红原文：`assertion \`left == right\` failed: [summon_ok] new_slaves[0].name 不等`
- left/right：`left = "神兽X"`（Rust 发出） / `right = "神兽"`（golden）。
- ⇒ 证明 `new_slaves[0].name` 的比较**不是恒真**。

## 还原证据
三颗牙各自：变异文件 `cp` 快照还原后 `sha256` 与变异前 **EQUAL**；`sinsu.json` golden sha 全程不变（`0e93c381…`），`git status` 无变异残留。
