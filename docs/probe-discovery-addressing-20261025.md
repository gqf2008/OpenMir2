# discovery 确定性寻址（国王/老人）— 2026-10-25

- 卡：`openmir2-discovery-deterministic-addressing`（tier **L**，owner `openmir2-dev-2`，branch `card/discovery-deterministic-addressing`）。
- 定位：窗口 discovery 原来是「`click_npc: auto:last_turn` + 等转身帧」，而国王的转身是
  `GuildOfficial.Run` 里的 `Random(40)==0`（`src/GameSrv/Npc/GuildOfficial.cs:36-47`）⇒ 期望 ≈10s > 场景 5s，
  run3 的 n=10/11/12 三条因此长期 UNDECIDABLE。本卡改用已合入的 `name:` 寻址，做到**0 等待、不依赖 RNG**。

## 1. 机制与修法

`auto:last_turn` 要求进世界后**先看到一条非己 `SM_TURN(10)`** 才能取 recog；固定 NPC 的转身由 Run 周期 +
RNG 决定，所以「有没有可点的 actor」是概率事件。`name:` 寻址不依赖转身帧：它直接按可见对象的 `ShowName`
匹配（`SM_TURN` 出现帧的名尾），**点击前不发任何等待帧**。

新增两个专用 discovery 场景（`--discovery-scenario` 用）：

| 文件 | 点的谁 | 用法 |
|---|---|---|
| `tools/probe/scenarios/npc_click_king.json` | `click_npc: name:比奇国王` | run3 #10 `king_dialog` / #11 `king_castle_war` |
| `tools/probe/scenarios/npc_click_elder.json` | `click_npc: name:老人` | run3 #12 `npc_script_chain` |
| `tools/probe/scenarios/npc_click_king_slow.json` | **兜底变体**：先 `wait_for SM_TURN(10)` 15s 再 `auto:last_turn` | 仅在部署命名不一致、name: 不可用时 |

run3 接线（`run3-scenarios.json` 的 `extra`，`--discovery-scenario` 是 `card_window.py:10101` 的既有参数）：

```json
{"n": 10, "name": "king_dialog", "extra": ["--discovery-scenario", "tools/probe/scenarios/npc_click_king.json"]}
```

**兜底不是解法**：15s 只是把 flaky 概率压低（期望 10s 附近仍会失败），文档与场景 `description` 都写明了这一点。

## 2. 排查：哪些 discovery 仍是概率性的

按 run3 里 `required` 参数场景逐个核（`tools/probe/param_discovery.py` 的 discovery 走同一个默认场景）：

| run3 场景 | required | 发现方式 | 概率性？ | 处置 |
|---|---|---|---|---|
| #10 `king_dialog` / #11 `king_castle_war` | `king_actor` | 默认 `npc_click_auto`（auto:last_turn） | **是**（国王转身 `Random(40)`） | 改 `npc_click_king.json` |
| #12 `npc_script_chain` | `npc_actor`（老人） | 默认 `npc_click_auto` | **是**（同上，且最近对象未必是老人） | 改 `npc_click_elder.json` |
| #9 `merchant_dialog` / #17 `upgrade_retrieve` | `merchant_actor` | 默认 `npc_click_auto` | 同类形态（最近商人转身也是 Run+RNG），W1 目前未报 | **暂不改**，等实证；若 flaky 再按名字点具体商人 |
| #18 `gamepoint_exchange` / #19 `clearmapmon_q001` / #23–#24 castle 税 | `npc_actor` / `merchant_actor` | 默认 `npc_click_auto` | 同上 | 暂不改（同上） |
| #14 `bag_equip` / #15 `eat_item` | `make_index`/`item_name`/`takeon_where` | **不走点击 discovery**（param_discovery 从 `SM_BAGITEMS(201)` 帧推导） | 否 | 不在本类 |
| #8 `move_walk` / #16 `spell_cast` / #20 `dropgold_zero_draw` / #21 `butch_two_draws` | 无 `required` 或非点击类 | — | 否 | 不在本类 |

**不特判**：没有把「国王」写进代码；两个场景文件只是各自声明要点的名字，探针侧仍是通用 `name:` 机制。
同类场景只要换成对应的 `name:`/`pos:` 即可，不需要改代码。

## 3. 验证

```sh
python -I -B tools/probe/mir2_probe.py --selftest
# => mir2_probe selftest: OK (20 checks; ...)   新增 discovery-addressing
```

`discovery-addressing` 三颗牙（构造级、不连网）：

- **牙 a**：`npc_click_king.json` 在目标格有固定 NPC 时**零等待**直接点——只发 1 帧 `CM_CLICKNPC(recog=王)`，
  **没有任何 `CM_TURN`**；
- **牙 b**：目标不存在（但另有别的可见对象）⇒ `StepFailure`「没有对象匹配」，**不静默回落到**别的对象或
  `auto:last_turn`，一帧不发；
- **牙 c**：既有 `npc_click_auto.json`（`auto:last_turn`）仍能正常点（通用路径不破坏）。

变异：把零命中分支改成「回落取第一个可见对象」⇒ 牙 b 红（同时既有「空格坐标」牙也红），还原后 20/20 绿。

## 4. 边界与未覆盖

| 项 | 现状 |
|---|---|
| 部署命名 | `name:比奇国王` / `name:老人` 依赖部署 `Npcs.txt`/`Npc_def` 的名字；部署改名则场景要同步（名字与卡 id 都写在场景 `description` 里） |
| 同名多只 | `name:` 走既有歧义门：多命中 ⇒ fail-closed（不会随便挑一个） |
| `pos:` 替代 | 也可写 `pos:29,32`（玩家在 0122 正南邻格），但要过 `pos:` 的静态对象门；本卡用 `name:` 更直接 |
| 真机 | **未做真机**（W1 窗口由 tester 排期）；本卡只做构造级 + 场景静态校验（`--check-scenario`） |
| 其它 discovery | 见 §2 表：`merchant_actor` 类保持默认，等失败实证再逐个收口 |

## 5. 残留物清单

- 入库：`tools/probe/scenarios/npc_click_king.json`、`npc_click_elder.json`、`npc_click_king_slow.json`、
  `tools/probe/mir2_probe.py`（selftest）、`tools/window/run3-scenarios.json`（10/11/12 的 `extra`）、本文件。
- 未入库：`E:/tmp/king.json`、`E:/tmp/elder.json`、`E:/tmp/slow.json`、`E:/tmp/probe_wip.py`（rebase 时的临时物）。
- 未触碰：`card_window.py`、`run_batch.sh` 执行逻辑、`src/**`、mysqld；未起服务端、未占端口、未跑 cargo。
