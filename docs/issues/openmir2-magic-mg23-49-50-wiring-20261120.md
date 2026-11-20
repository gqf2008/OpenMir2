# MG-23 / MG-49 / MG-50 接线（holyshield 16 / grouplightening 42 / groupamyounsul 48）

> 分支 `card/magic-mg23-49-50`；base `master 3362758fa`。
> 上游：opc-19 指派（`c6` 在世代切换验证里发现三份 golden 只有数据、没有接线）。
> 边界：**不新造 oracle 数据、不改其它 stem 的 golden**。

## 1. 评估（只读，先做的）

| 项 | **MG-23 holyshield (16)** | **MG-49 grouplightening (42)** | **MG-50 groupamyounsul (48)** |
| --- | --- | --- | --- |
| 标签 | `mon_hit_two` / `mon_blocked_level` / `mon_blocked_master` / `mon_partial` | `s1` / `s5` / `s13` | `s1` / `s5` / `s13` |
| 契约三键 | ✅ `rng_mode=cover_drain` / `login_order=true` / `reachability=reachable` | ✅ | ✅ |
| 657 | 无 | 无 | 无 |
| 53（`SM_HEALTHSPELLCHANGED`） | 无 | **有**（target） | 无 |
| 怪物 | 有（2/1/1/2） | 无 | 无 |
| RNG 抽数 | 4/2/2/4 | 8 | **0** |
| 坐标 | 与 `fireball_rig` 缺省逐值相同（100/101/105） | 同左 | 同左 |
| Rust 实现 | host `SKILL_HOLYSHIELD` 臂 + world | host `_ =>` 臂 + world | host `48 =>` 臂（共享 `cast_group_poison_spell`） |
| 结论 | 直接接线 | 直接接线 | 直接接线（但见 §4） |

**关于 53**：驱动内钉 `MIR2_CLIENT_ABILITY_LAYOUT=0` ⇒ 发的是 **oracle header-only 12B**
（不带 16B 客户端尾）⇒ 全帧比较在这里是对的；生产默认 client 布局才带尾，那是已登记偏差。

## 2. 实现

- **驱动**（`game-svr --lib`，均 `#[ignore]` + `--exact --test-threads=1`）：`holyshield_parity_{mon_hit_two,mon_blocked_level,mon_blocked_master,mon_partial}`、
  `grouplightening_parity_s{1,5,13}`、`groupamyounsul_parity_s{1,5,13}`，全部复用 `fireball_parity_drive`。
- **逐收件人帧序列断言**（`per_receiver_frame_seq_matches_golden`）：比 `(ident,param,tag,series)`
  **有序序列**、逐 `Who`。**空 `Sent` 的收件人两侧都丢掉再比**——台账只登记有帧的收件人，
  金标准对零帧对象（`holyshield` 的 `mon0/mon1`）留一条空表（`strip_empty_receivers`；
  非空帧仍逐帧严格比较，**不是**放宽判据）。
- **比较器**（`mir2-parity-tests`）：`real_rust_ledger_{holyshield,grouplightening,groupamyounsul}`，
  一律走 `real_ledger_verify_labels*`；holyshield 因上述零帧收件人走 `_opts(...,true)`，
  其余两支走原函数。

## 3. 证据（实跑）

- `cargo fmt --all -- --check`：**rc=0**
- `cargo test -p game-svr --lib`（base `3362758fa`）：**2180 passed / 0 failed / 86 ignored**（rc=0）
- 10 支驱动：`MIR2_LEDGER_RUN_ID=mg2349-b` 下**全部 1 passed**
- 3 条比较器：同 run id 下**全部 1 passed**
- **牙（改坏必红，红在该支自己的断言上）**：
  - **16**：移除 `cast_holy_curtain_spell` 的等级门（`draw + (caster_level-1) > obj.level`）⇒
    `holyshield_parity_mon_blocked_level` 红在**逐收件人帧序列**断言（帧集合退化可见）。
  - **42**：`power_get_power_plus_mc(...) + 1`（42 支内联伤害）⇒ `grouplightening_parity_s1`
    红在同一断言（`SM_STRUCK` 的 param 变）。
  - **48**：见 §4。

## 4. MG-50 的发现：这份 golden 是**退化场景**（无判别面）

实测（四次变异，全部**保持绿**）：
1. `power_poison` 的 RPow 系数 `*2 → *3`；
2. `cast_group_poison_spell` 的 `range` `+5`；
3. 移除 `is_proper_target_grouped` 门；
4. 强制 `poison_dice` 通过。

四者都不改变输出 ⇒ **逐目标循环一次都没执行**。佐证：golden 自身 `rng_total_draws = 0`、
两侧都**没有** `RM_POISON`、`TargetHp` 前后都是 100。
⇒ 本场景只覆盖"施法本身"（17/638），**完全没有覆盖 48 的效果体**。

**结论与建议**：
- 接线本身有效（帧面与金标准逐帧一致），但**不能**据此把 MG-50 升格为"效果体已验证"。
- 要拿到 48 的判别面，需要**目标真正落在 `mag_objects_in_range` 内**的场景（新 oracle case）
  —— 超出本轮"不新造 oracle 数据"的边界，需另开卡。
- 在此之前，MG-50 的 evidence 只应记为**帧面/接线覆盖**，不得写成"群体施毒已验证"。

## 5. manifest evidence 建议（供升格参考）

| stem | test | fixture | cases |
| --- | --- | --- | --- |
| `holyshield` | `holyshield_parity_mon_hit_two` / `..._mon_blocked_level` / `..._mon_blocked_master` / `..._mon_partial` + `real_rust_ledger_holyshield` | `golden/magic/holyshield.json`（4 case） | 四个标签全覆盖（含两条门禁负例 + 部分命中） |
| `grouplightening` | `grouplightening_parity_s1/s5/s13` + `real_rust_ledger_grouplightening` | `golden/magic/grouplightening.json`（3 case） | 三 seed；**含 53（oracle 12B 布局）** |
| `groupamyounsul` | `groupamyounsul_parity_s1/s5/s13` + `real_rust_ledger_groupamyounsul` | `golden/magic/groupamyounsul.json`（3 case） | 三 seed；**仅接线面（§4），效果体未覆盖** |

## 6. 禁区自证

- 未改任何 golden、未改其它 stem 的比较器/驱动、未动清单 `status`。
- 只改 `bins/game-svr/src/lib.rs`（测试模块）与 `tests/parity/tests/magic_fireball_parity.rs`
  （新增三支比较器 + `strip_empty_receivers`/`_opts` 接缝，原函数语义不变）。
