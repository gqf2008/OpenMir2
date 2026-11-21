# 召唤/克隆 actor id 的暴露面与关系口径（2026-11-21）

> **三行结论**
> ① **装置 `idBase` 分配的 id（caster/target/obs_in/obs_out/pre-slave）进比较面且必须稳定**——`Sent[].recog`、`Rm.p3` 里都能看到它们（如 `1001/2001/1002/2050`）。
> ② **产品运行期分配的 actor id（召唤怪 `GetNextIdentity`、clone 的 `Clone.actor_id`）永不跨侧比绝对值**——生产每次启动随机，任何值相等断言都不可复现。
> ③ 三支召唤 stem（`skeleton` 17 / `sinsu` 30 / `angel` 41）当前**没有产品分配 id 进入比较面**；首个暴露点会是 **656 名字色帧的 `Sent[].recog`**。

## 1. 口径（判定，不是知识）

- **允许进比较面**：装置 `idBase` 分配的 id（`SpellFrameRig::new_layout` 的 `id_base` 约定：caster=base+1、第 i 观察者=base+2+i、预置 slave=base+50；golden 侧对应 1001/2001/…）。它们是人造稳定值，比较器比它们是对的。
- **禁止跨侧比绝对值**：产品运行期分配的 actor id（召唤怪由 `ActorMgr.GetNextIdentity()` / 随机池生成；`clone46` 的 `Clone.actor_id` 同族，见 B20）。只允许断言**关系**：存在、计数、互异、同侧引用一致。
- **不许把两者混成一句 "id 不比较"**：那会把装置 id 的真约束一起放掉（下一个人会顺手把 `recog=1001` 也当非确定处理）。

## 2. 三支的暴露面清单（当前无产品 id）

| 面 | `skeleton`(17) | `sinsu`(30) | `angel`(41) | 判定 |
| --- | --- | --- | --- | --- |
| `new_slaves[]` 逐字段 | `name/level/hp/max_hp/master_is_caster` | 同左 | 同左 | **无 id** |
| `Skeleton`/`SinSu`/`Angel` 块 | `regen_calls`/`slave_count*`/`pre_slave_count`/`stub_*` | 另含 `pre_slave_xy_*`/`recall_moved` | 同 skeleton | **无 id** |
| `Sent[].recog`（比较器严格比） | 1001 / 500 / 17 | 1001 / 100 / 30 / **2050**（装置预置 slave） | 1001 / 400 / 41 | **装置 id 或魔法 id，无召唤 id** |
| `Rm` 键（ident/wParam/p1/p2/p3/priority/late） | p3 多为魔法 id / 1002（装置 target） | 同左 | 同左 | **无召唤 id** |
| `Fixture` | `caster_actor_id/target_actor_id`（装置） | 同左 | 同左 | **无召唤 id** |

## 3. 首个暴露点（若将来加帧/加 Receivers）

1. **656 名字色帧的 `Sent[].recog`**：生产 `MakeSlave` 收尾 `RefNameColor` ⇒ `SendRefMsg(RM_CHANGENAMECOLOR)` 发给 ±12 玩家；`recog` = **召唤怪自己的产品 id**。装置现在看不到它（替身怪未绑 `Envir`，B1；`mon` 无人 drain，B2/B5），一旦补上就会随机红。
2. 把召唤怪加进 `byWho`/Receivers ⇒ 它自身发送帧的 `recog/param` 可能带自身 id。
3. `new_slaves` 将来加 `actor_id` 字段（B20 式加固很容易顺手加）。
4. `807` 回召从"预置 slave"扩展到"召唤出的 slave"（`recog=2050` 现在是装置 id）。
5. `Rm` 的 `p1/p2/p3` 若承载 actor id。

## 4. 实现（driver 侧，未动 golden/比较器/产品）

`mir2-rs/bins/game-svr/src/lib.rs` 新增 `assert_summon_id_contract(label, stable_ids, summon_ids, new_slaves_len, receivers)`，在 `skeleton_parity_drive` / `angel_parity_drive` / `sinsu_parity_drive` 三处调用：

- `summon_ids.len() == new_slaves.len()`（存在/计数一致）；
- 每个召唤 id **≠ caster/target/obs_in/obs_out/pre-slave**（与装置稳定 id 不相交）；
- 召唤 id **互异**；
- **护栏**：任何 `Sent` 帧的 `recog` 落在召唤 id 集合内 ⇒ **具名 panic**，消息含 `"产品运行期分配的召唤 id 集合 ... 不得进入比较面；若需回指，改用关系口径（B20；口径见 `docs/issues/summon-actor-id-surface.md`）"`。

**禁止**：不加"召唤 id == golden 值"的断言；不修改 golden；不修改比较器（共享面）。

## 5. 牙（2026-11-21 实测）

- **绿**：`MIR2_LEDGER_RUN_ID=b20-summon-guard-20261121` 下三支 `--exact --ignored` 各 `1 passed`（`skeleton`/`angel`/`sinsu` 的 `summon_ok`，均 `entities 4->5 / slave 0->1 / regen=1 / new=1`）。
- **红**：临时把 `receivers[0].Sent[0].recog` 改成 `slave_ids[0]`（本 run = `1005`）⇒ 护栏原文：
  `[summon_ok/caster] Sent[0] 的 recog=1005 落在产品运行期分配的召唤 id 集合 [1005] 内：此类 id 不得进入比较面；若需回指，改用关系口径（B20；口径见 docs/issues/summon-actor-id-surface.md）`
- **还原**：快照 `cp` 回写，`git hash-object` 前后同为 `e8fdcccc43bfa050d051468b0c99c10cf4d46eb8`，`grep -c MUTATION` = 0，复跑 `skeleton_parity_summon_ok` 为 `1 passed`。

## 6. 边界

- `clone46` 的 `Clone.actor_id` 是同类问题但已进观测块，归 **B20**（本卡只处理召唤三支）。
- 装置侧若要真正观测 656，需先补 B1（替身怪绑 `Envir`）与 B2/B5（怪物队列排水）；那不是本卡范围。
