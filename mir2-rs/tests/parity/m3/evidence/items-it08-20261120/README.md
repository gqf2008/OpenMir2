# IT-08 升级支 —— 宿主级对拍 §16 行为牙证据

对象：`tests::it08_levelup_host_parity`（`bins/game-svr/src/lib.rs`）。
过滤 idents `{53,44,45,52,752}` 按 oracle 线上序逐字段比 golden `pumped`；逐字段比派生
`max_hp/max_mp/hp/mp/dc`；断言升级后 `WAbil==Abil`（exp/level/max_exp）；`body_luck`/`item_list`。

## 命令

快照：`cp lib.rs E:/UserTemp/it08-host-lib-snap.rs`（sha 见 restore-and-baseline.txt）。
逐变异：改一处 → `cargo test -p game-svr --lib it08_levelup_host_parity` → 记录红 → `cp` 快照还原 → 核 sha。
> 注意：**不得用 `-- --exact`** 配裸名（会 0 tests 假绿）；runner 断言 `passed+failed>=1`。

## 变异结果（见 teeth-red-ABCD.txt，均 ran=(0,1) 即 1 测跑、1 失败）

| 变异 | 红在哪 | 说明 |
|---|---|---|
| A 去 `recalc_session_ability`（=旧实现不重算 WAbil） | 帧序断言 | 实得 53(…,0,0,0)/45(9990,5) ≠ golden 53(…,52,32,52)/45(90,6)：recalc 承载 WAbil 追平，旧实现在此必红 |
| B 去 `send_levelup_frames` | 帧序断言 | 只剩 [53,44]，缺 45/52/752 |
| C 去 53 发射 | 帧序断言 | 首帧缺失，实得缺 53 |
| D 去 `inc_health_spell(2000,2000)` | 帧序断言 | 53 的 param/tag=(0,0)≠(52,32)：HP/MP clamp 承载 |

## 未完成

- 变异 **E**（去升级 `add_body_luck(100)` ⇒ body_luck 仅 0.2≠100.2）在编译中被**低内存看守杀掉**，未取得红原文；
  且随后收到 opc-07 构建暂停令。**待窗口放行后补跑 E**，届时才有 body_luck 断言的独立牙证据。
  （body_luck 断言现已随用例通过；A–D 已证帧序/recalc/clamp 三处承载。）
- 还原后基线已复跑绿（见 restore-and-baseline.txt）。
