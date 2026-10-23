# `AddAbil.WeaponStrong` 建模与武器磨损接线（`openmir2-ability-weaponstrong`）

- 卡：`openmir2-ability-weaponstrong`（tier=R）；分支 `card/openmir2-ability-weaponstrong`；
  本轮 owner = `openmir2-worker-67`（接管声明 `52e7fd45`，原 owner `openmir2-worker-24`）。
- 基线：`walgit/master = 71420a145`（返工分支已 rebase 到该提交）。
- 交付：输入端 `AddAbility::weapon_strong` + 消费端 `attack_dir_grouped_poison_ws` +
  宿主 `CM_HIT` 灌入 + 装备口径（含 `Desc[7]`）+ 数值/RNG 用例 + 牙 + R 窗口项（§8）。

## 1. 可达性判定（先判，再决定实现/登记）

部署 MySQL（`E:/MirServer/M2GameSvr/Server.conf` 的 `[DataBase]`：
`server=127.0.0.1;uid=root;pwd=;database=mir2_data`）只读查询，脚本 `E:/tmp/ws_db_probe3.ps1`
（`Add-Type E:/MirServer/DBServer/MySql.Data.dll` + 只读 `SELECT`；**必须用 pwsh 7**，Windows PowerShell 5.1 载入该程序集失败）：

```powershell
& E:/tmp/ws_db_probe3.ps1
```

| 查询 | 结果（2026-10-24 本机实跑） |
|---|---|
| `SELECT COUNT(*) FROM stditems` | **1000** |
| `SELECT COUNT(*) FROM stditems WHERE Source <> 0` | **217** |
| `SELECT COUNT(*) FROM stditems WHERE StdMode IN (5,6,24,26) AND Source BETWEEN 1 AND 10` | **38** |
| 命中样例 | `742 狂雷护腕 StdMode=26 Source=1`、`764 王者护腕(战) 26 2`、`898 皓月护腕 26 10`、`908 毒魔双刃剑 5 3` 等 |
| 负值样例（oracle 门不写） | `246 天之龙牙 5 -1`、`251 玄天 5 -4`、`274 邪魔血刀 5 -8` 等共 8 把 |

⇒ **部署可达**，按卡面「若存在 ⇒ 必须实现」处理（不走「不可达仅登记」分支）。

## 2. oracle 逐段

| 事实 | 出处 |
|---|---|
| 唯一消费点：`nWeaponDamage = (ushort)(Random(5) + 2 - AddAbil.WeaponStrong)` | `src/M2Server/Player/PlayObject.Attack.cs:225` |
| 写入点 A：`StdMode 5/6` 支 `if (SpecialPwr >= 1 && <= 10) AddAbil.WeaponStrong = (byte)SpecialPwr` | `PlayObject.cs:4424-4426` |
| 写入点 B：`StdMode 24/26` 支同式 | `PlayObject.cs:4563-4566` |
| `Desc[7]` 升级覆盖：`GetUpgradeStdItem` 内 `if (Desc[7] >= 1 && Desc[7] <= 10) { if (clientItem.Item.SpecialPwr >= 0) SpecialPwr = (sbyte)Desc[7]; }`（**只有 `case 5: case 6:`**） | `src/M2Server/Items/GameItemSystem.cs:744-750` |
| 字段类型 `byte` | `src/OpenMir2/Data/AddAbility.cs:72` |
| 随机升级写 `Desc[7] = (byte)(1 + up / 2) ∈ 1..=7` | `GameItemSystem.cs:227-231`（`Random(10)==0` 命中支） |

**赋值不是累加**（后写覆盖）；其余 `StdMode` 不写。`Desc[7]` 覆盖以**模板** `SpecialPwr >= 0` 为门（`CopyItemToClientItem` 只搬模板值，尚无其它改写）。

## 3. 实现

### 3.1 输入端（`bins/game-svr/src/ability.rs`）

- `AddAbility` 新增 `pub weapon_strong: u8`（doc 写清 oracle 出处、赋值语义、`Desc[7]` 口径）。
- `apply_item_parameters`：在 `5 | 6`、`24`、`26` 三支里按 `(1..=10).contains(&item.special_pwr)` **赋值**。
- `weapon_strong_of(use_items, catalog)`：逐件复用 `apply_item_parameters`，并在 `StdMode 5/6`
  且 `Desc[7] ∈ 1..=10` 且模板 `SpecialPwr >= 0` 时用 `Desc[7]` 覆盖（oracle `:744-750`）。
- 模块 doc 原写「`WeaponStrong` 略」已订正。

### 3.2 消费端（`crates/world/src/combat.rs`、`world.rs` + 宿主 `lib.rs`）

- `combat.rs`：**修法 (a)**——旧函数 `attack_dir_grouped_poison` **原地改名** `attack_dir_grouped_poison_ws`，
  参数行末尾加 `weapon_strong: u16`；函数体（含毒门 `random_below`）**不移动**，尾行改调
  `attack_success_weapon_wear_with_strong(rng, attacker, weapon_strong)`；函数体之后追加薄包装
  `attack_dir_grouped_poison(...)`（恒传 0，保留历史签名给 `attack_dir_grouped` 与旧调用方）。
  **行号中性化**：参数行与 `#[allow(clippy::too_many_arguments)]` 各 +1 行，与函数 doc 里删去的两条空白 `///` 行相抵 ⇒ 毒门
  `random_below` 仍在 `combat.rs:1260`（`attack_rng_ledger.py` / `rng_ledger_diff.py` 现读源码钉住的 canonical 行）。
- `world.rs`：`attack_weapon_strong: HashMap<i32, u16>` + `set_attack_weapon_strong`；
  `attack_once` 以 `_ws` 末位实参传该值（缺省 0）。
- 宿主 `bins/game-svr/src/lib.rs`：`handle_hit_cmd` 在 `world.attack_cmd(...)` **之前**恰一次
  `set_actor_weapon_strong(world, sessions, catalog, actor_id)`，按当前 `UseItems` 重算。

> **为何不采用返工 v2 的 `Entity.weapon_strong` 字段**：v2（head `7a1badb7a`）虽然也能保持 1260，
> 但改了 `crates/world/src/entity.rs`，超出 Leader 追认的写集（`d856b978` 只授权
> `crates/world/src/{combat,world}.rs`）。本轮按 review 的修法 (a) 落回授权写集内。

## 4. I-1 RNG 硬门证据（base vs head）

```sh
# base（master 8b40443b6，独立 worktree E:/tmp/wt-ws67-base）
cd E:/tmp/wt-ws67-base/mir2-rs
M2_HIT_DICE_TRACE_PATH=E:/tmp/ws67-base.tsv cargo test -p mir2-world --test hit_dice_rng_trace

# head（本分支）
cd E:/tmp/wt-worker-67/mir2-rs
M2_HIT_DICE_TRACE_PATH=E:/tmp/ws67-head.tsv cargo test -p mir2-world --test hit_dice_rng_trace

diff -u E:/tmp/ws67-base.tsv E:/tmp/ws67-head.tsv
# ⇒ rc=0（逐字节相同）

python -I -B mir2-rs/tests/parity/m3/rng-trace-probe/rng_ledger_diff.py \
  diff E:/tmp/ws67-base.tsv E:/tmp/ws67-head.tsv --a-side rust --b-side rust
# ⇒ VERDICT IDENTICAL（attack_success=2，ONLY_IN_A/ONLY_IN_B 均空，FIRST_DIVERGENCE none，len=6）
```

实测：`diff` **rc=0**；两侧 canonical 均为
`{'attack_success': 2, 'atk_power_a': 1, 'atk_power_b': 1, 'hit_gate': 1, 'hit_struck_damage': 1}`。

`attack_rng_ledger.py selftest` / `rng_ledger_diff.py selftest` 均 **GREEN**（70/70、57/57）。

## 5. I-2 `Desc[7]` 随机升级路径

- oracle：`GetUpgradeStdItem`（`GameItemSystem.cs:744-750`）在 `StdMode 5/6` 且
  `Desc[7] ∈ 1..=10` 且模板 `SpecialPwr >= 0` 时把有效物品的 `SpecialPwr` 覆盖为 `Desc[7]`；
  `ApplyItemParameters` 随后读**有效物品**。
- 本仓：`weapon_strong_of` 先按模板跑 `apply_item_parameters`，再按上述**同一门**用 `Desc[7]`
  覆盖 `add.weapon_strong`（只对 `5/6`；`24/26` 不受影响）。可达性：`world.rs` 的掉落链
  `generate_drops → formula/drop.rs:611 random_upgrade_item → :155 desc[7]=(1+up/2) ∈ 1..=7`。
- 用例 `weapon_strong_of_consumes_upgrade_desc7_for_weapons` 五档：
  无升级用模板 / `Desc[7]=5` 覆盖模板 3 / 模板 0 + `Desc[7]=7` ⇒ 7 / `Desc[7]=11` 越界不覆盖 /
  `StdMode 24` 不走 `Desc[7]`。
- 牙（本轮实跑）：把覆盖赋值改成空操作 ⇒ 该用例 **FAILED rc=101**；备份还原，
  `sha256sum ability.rs = d7bd0129…` == `git show HEAD:...`，还原后用例 **ok**。

## 6. 数值用例（`weapon_strong_numeric_matrix_matches_oracle`）

同种子（42）下四种口径：

| 场景 | 断言 |
|---|---|
| `WS=0` | `raw = Random(5)+2 ∈ 2..=6` |
| `WS=3` | 磨损量 `(ushort)(raw-3)`（`raw=2` 时回绕） |
| `WS=9 > raw` | `(ushort)(raw-9)` 回绕成大正数 ⇒ 耐久反向增加、不破零、不 panic（例：`raw=2, WS=9` ⇒ 5000→5007） |
| `WS == raw` | `nWeaponDamage == 0` ⇒ 门不过、零帧、耐久不变 |

另有 `weapon_strong_does_not_change_rng_draws`：同种子对比 `_ws(...,0)` 与 `_ws(...,9)` 的
`random_below` **入参序列逐项相同**（同位置、同次数）。

## 7. 牙（本轮全部自己复现，还原后 sha 对齐）

| # | 变异 | 预期 | 实测 | 还原 |
|---|---|---|---|---|
| 1 | `combat.rs` 毒门前插一空行（把 `random_below` 挤到 `:1261`） | ledger RED | `attack_rng_ledger.py selftest` **rc=1**（点名 `:1261`） | 删除空行，行号回 1260 |
| 2 | 删除宿主 `set_actor_weapon_strong(...)` 调用 | 接线牙红 | `weapon_strong_is_injected_before_attack_dispatch` **FAILED rc=101** | 备份还原，`sha256sum lib.rs = 260b4eb5…` == `git show HEAD:...` |
| 3 | `wrapping_sub` → `saturating_sub` | 数值矩阵红 | `weapon_strong_numeric_matrix_matches_oracle` **FAILED rc=101** | 备份还原，`sha256sum combat.rs = c0bff684…` == `git show HEAD:...` |
| 4 | 删除 `Desc[7]` 覆盖赋值 | I-2 用例红 | `weapon_strong_of_consumes_upgrade_desc7_for_weapons` **FAILED rc=101** | 备份还原，`sha256sum ability.rs = d7bd0129…` == `git show HEAD:...` |

## 8. R 级真机窗口项（`window_required: true`）

> **真机窗口未采 ⇒ 本卡合并后保持 `in-progress`，不得 `closed`**（tier=R）。
> 场景 `tools/probe/scenarios/equip_struck_save.json`（硬门 `wait_for [642,203]`、`wait_idents [642]`）。

可复跑命令：

```sh
python -I -B tools/conformance/card_window.py \
  --card openmir2-ability-weaponstrong \
  --scenario tools/probe/scenarios/equip_struck_save.json \
  --out E:/tmp/win-ability-weaponstrong
```

窗口判据（review `f3be880d1` 的 ⓐ–ⓓ，抄录）：

- **ⓐ 642 载荷逐字节且只投自己**：同场景 Rust/C# 的 `SM_DURACHANGE(642)`（`Recog=扣后 Dura`、
  `Param=1`、`Tag/Series=DuraMax`）逐字节一致，收件人只有攻击者本人。
- **ⓑ `SpecialPwr=k>0` 时磨损量 = `(ushort)(Random(5)+2-k)`**：覆盖 `k > raw` 的 ushort 回绕
  （耐久增加、不破零、不 panic）。夹具建议用部署命中行：`908 毒魔双刃剑`（`StdMode=5, Source=3`）、
  `898 皓月护腕`（`26, 10`）。
- **ⓒ `k == Random(5)+2` ⇒ 零 642、耐久不变**（`nWeaponDamage == 0` 门）。
- **ⓓ 存档 `characters_item.Dura` 与帧一致**：窗口后查库，耐久 = 帧里的 `Recog`（含回绕/破零支）。

窗口注意：`Desc[7]` 随机升级武器（`Desc[7] ∈ 1..=7`）比模板命中**更常见**，窗口夹具应同时覆盖
「模板 `Source>0`」与「模板 0 + `Desc[7]=k`」两类，否则只验到一半。

## 9. 门禁（本轮 head 实跑，base master `71420a145`）

| 门禁 | 结果 |
|---|---|
| `cargo fmt --all -- --check` | rc=0 |
| `cargo clippy --workspace --all-targets -- -D warnings` | rc=0 |
| `cargo test --workspace` | **141 suites / 3149 passed / 0 failed / 62 ignored**，rc=0 |
| `cargo run -p msgcodegen -- --check` | **604 OK** |
| `whitelist_anchor_gate.py check` | **ALL PASS 588** |
| `gm_registry.py gate` | **GREEN 74** |
| `script_host_noop_audit.py check --strict-lines` | **GREEN 168** |
| `t_reservation_gate.py check` | **GREEN rows=255 / whitelist=255 / max=T-267** |
| 冲突标记 / `git diff --check` | 0 / rc=0 |
| `git merge-tree --write-tree refs/remotes/walgit/master HEAD` | rc=0（tree `43c1a348f`） |
| `attack_rng_ledger.py selftest` / `rng_ledger_diff.py selftest` | GREEN 70/70、57/57 |
| 窗口 dry-run `card_window.py --dry-run` | rc=0 |

## 10. 残留（登记，不隐瞒）

1. **R 级真机窗口未采**（§8）——构造级全绿不等于 R 面已验证；合并后卡保持 `in-progress`。
2. **`Desc[7]` 只覆盖 `WeaponStrong` 的输入口径**：oracle `GetUpgradeStdItem` 还会用
   `Desc[0..6]` 改写 DC/MC/SC/AC/MAC/HitSpeed 等；`ability.rs` 的整段 `ApplyItemParameters`
   目前只对 `WeaponStrong` 消费 `Desc[7]`，其余差量仍属既有偏差（不在本卡写集/范围）。
3. **生产注入时机**：宿主每次 `CM_HIT` 前按当前 `UseItems` 重算并灌表（与 oracle `RecalcAbilitys`
   后的 `AddAbil` 口径一致）；若装备在两次攻击间变化而 `RecalcAbilitys` 未跑，本仓会比 oracle
   更早看到新值——该窗口由现有装备/重算接线保证，本卡不改。
