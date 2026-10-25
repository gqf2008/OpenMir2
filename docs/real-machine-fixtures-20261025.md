# 真机夹具 provisioning（2026-10-25）

卡 `openmir2-mirflow-real-machine-prereqs`（tier L，离线，不占窗口、不起任何进程）。
工具：`tools/client/provision_real_machine_fixtures.py`（自检 29 条，`--selftest`）。

## 0. 为什么需要

冒烟角色 `aaa` 的实测初始态（2026-10-25 只读 SELECT，`mir2_test_db_r`）：

```
Id=12215 ChrName=aaa MapName=0 CX=286 CY=649 Gold=0 GamePoint=0 Level=1
characters_bagitem：46 个**槽位**，其中 `StdIndex<>0` 的**真物品 3 件**
```

> 口径提醒：冒烟证据文件里的「背包 1 行」是**帧基线**（`SM_BAGITEMS(201).series`）口径，不是 DB 行数；
> DB 侧 `characters_bagitem` 恒有 46 个槽位，空槽 `StdIndex=0` **不算物品**（本工具按 `StdIndex<>0` 判）。

Gold=0 ⇒ 买不了；背包只有 1 行 ⇒ 卖不出东西；没有地板物 ⇒ 捡物无物可捡；位置不在任何商店旁边
⇒ NPC 阶段还得多走一段路。本工具把这三件事（钱、货、位置）+ 一个**会发 643 的商店 NPC** 一次备好。

## 1. 红线与守卫（A4）

**只允许**写测试库：`mir2_test_db_r` / `mir2_test_account_r` / `mir2_test_data_r`。

- 生产库名（`mir2_db` / `mir2_account` / `mir2_data`）⇒ **硬拒**，rc=3，报错里点名；
- 白名单外的任意库名、含非法字符的库名（如 `mir2_test_db_r; DROP`）⇒ 同样硬拒；
- 守卫在**连库之前**执行，拒绝时**一条 SQL 都不会发出**（自检用假 mysql 的 `executed == []` 断言）。

自检里的牙（都在 `--selftest` 里、可随时复跑）：

```
PASS A4/生产库 硬拒且零 SQL
PASS A4/C# 测试库 硬拒且零 SQL      # mir2_test_db（C# 侧）也不在白名单，只有 _r
PASS A4/注入 硬拒且零 SQL
PASS A4/白名单库放行
```

## 2. A1 金币（+ 可选可卖物品）

窗口当天一条命令（**默认 dry-run**；确认后加 `--yes-apply`）：

```sh
python -I -B tools/client/provision_real_machine_fixtures.py --apply \
    --db mir2_test_db_r --char aaa --gold 5000 --game-point 10 \
    --map 3 --npc-script "盟重土城/药店" \
    --yes-apply --report E:/tmp/fixtures-aaa.json
```

它做三件事，顺序固定：**先 SELECT 证现状**（必须恰好 1 行，否则 rc=2 且不写）→ 单事务 UPDATE
（`WHERE Id=? AND ChrName=? AND Gold=<before> AND MapName=<before>` 乐观锁）→ **再 SELECT 证结果**。
报告里同时落 `before` / `after` / `statements` / `rollback`；任何一步失败 ⇒ 发 `ROLLBACK;`、
`state=rolled_back`、rc=5（自检有对应牙）。

**登录后核对（判据是客户端帧，不是 DB）**：

```sh
python -I -B tools/client/provision_real_machine_fixtures.py --verify \
    --frames <flow-out>/proxy/frames.ndjson --expect-gold 5000
```

口径与 `tools/client/mir_flow.ps1:875-886` 一致：金币取**最后一条** s2c `SM_ABILITY(52)` /
`SM_GOLDCHANGED(653)` 的 `recog`。帧里 5000 == 期望 5000 ⇒ PASS；不等 ⇒ rc=4 并打印两侧值；
帧里一条 52/653 都没有 ⇒ PREREQ（rc=2），**不拿 0 冒充已知**。

可选可卖物品（`characters_bagitem` 插一行，缺省不塞）：`--bag-std-index 207 --bag-make-index 900001`；
写入前先 `SELECT COUNT(*) WHERE MakeIndex=?` 证空位，被占 ⇒ rc=2 不写；回滚是 `DELETE ... WHERE PlayerId AND MakeIndex`。

**回滚**：

```sh
python -I -B tools/client/provision_real_machine_fixtures.py --rollback E:/tmp/fixtures-aaa.json --yes
```

回滚后重新 SELECT 五个字段（Gold/GamePoint/MapName/CX/CY），与 `before` 全等才算 restored；不等 rc=4。

## 3. A2 地板物品：**结构性不可达（BLOCKED）**

结论：两侧都**造不出**地面物，因此 pickup 阶段（`CM_PICKUP(1001)` → `SM_ADDITEM(200)` /
`SM_GOLDCHANGED(653)`）在真机上无物可捡。这与 `docs/pickup-item-reachability-20261024.md` 同结论。

三条可机判的事实（`--a2-evidence`，任一事实变了 ⇒ `REEVALUATE` rc=2，逼人重新评估而不是静默绿）：

| 事实 | 位置 |
|---|---|
| oracle `MapItem` 构造器里 `ItemId` 赋值**被注释** | `src/Modules/SystemModule/Data/MapItem.cs:41` |
| oracle `AddItemToMap` 首行 `if (mapItem.ItemId == 0) return false;` | `src/M2Server/Maps/Envirnoment.cs:88-92` |
| Rust 1:1 复刻同一门 `if item.item_id == 0` | `mir2-rs/bins/game-svr/src/ground.rs:276` |

补充事实：

- oracle 全仓 `ItemId =` 的赋值点**只有那一行注释**；四条 `new MapItem` 路径（普通掉落
  `BaseObject.cs:551`、金币 `BaseObject.cs:740`、脚本 `@throwitem` `ExecutionProcessingSys.cs:5694/5730`、
  机器人 `RobotPlayer.cs:508-559`）都不赋 `ItemId` ⇒ 全部命中上面的门。
- DB 侧也没有地物表：`sql/*.sql` 里只有 `characters_item`（穿戴）与 `characters_bagitem`（包裹）；
  Rust 侧 `MapGrid::add_floor_item` 的调用点**只有单测**，服务端启动不种地物。

因此本工具**拒绝伪造地面物**：它生成的 SQL 里不存在 `mapitem`/`ground` 字样（自检有断言）。
若将来有人先在 oracle/Rust 里补上 `ItemId` 赋值，`--a2-evidence` 会立刻转 `REEVALUATE`，
届时再补「地物夹具」才是正确做法。

## 4. A3 可响应 NPC（643）

选择规则（确定性）：`MerChant.txt` 文件序，**商店脚本优先**（脚本头 `(@buy @sell)` 或存在 `[goods]`），
且 `[@main]` 下**确有说词行**（`#IF/#ACT` 之后是命令体，不算说词）。

实测（2026-10-25，只读）：

| 用途 | 脚本 | map / x,y | 脚本文件:行号 | 依据 |
|---|---|---|---|---|
| 默认（肉店，有买有卖） | `盟重土城/肉店` | 3 / (355,330) | `Envir/Market_Def/盟重土城/肉店-3.txt:5-7` | `[@main]` 说词 4 行 + `<买/@buy>` `<卖/@sell>` |
| **trade 推荐（药店）** | `盟重土城/药店` | 3 / (361,335) | `Envir/Market_Def/盟重土城/药店-3.txt:4`（`[@main]`）、`:1` `(@buy @sell)`、`:16` `[goods]`（金创药(小量) 在 `:17`） | 金创药(小量) 100 金币/个 ⇒ 5000 够买 |
| 只要说词 | `举证` | 3 / (345,338) | `Envir/Market_Def/举证-3.txt:2` 起 | 12 行说词 |

判据：点击 NPC 后 s2c 出现 `SM_MERCHANTSAY(643)`，正文与该脚本 `[@main]` 的说词逐字对应
（`mir_flow.ps1` 的 `npc_dialog` 阶段就是按这个判）。

**落点**：工具不写死坐标，而是从 NPC 的 8 邻格起逐圈找**可走**格（`.map` 口径同
`tools/conformance/card_window.py:1831` `read_map_cell`：越界或 `bk`/`fr` 的 bit15 ⇒ 不可走）；
找不到可走格或地图文件缺失 ⇒ PREREQ，**不猜**。

## 4b. A6 动作前的前置门（`--gate`，Leader 裁决新增）

判据：**能不能测 = UNDECIDABLE（`kind=missing_prereq`，点名缺项）；该发生却没发生 = RED**。
本卡负责把「金币 / 可卖物品 / NPC / 地板物 / 怪物夹具**是否备齐**」变成动作前可机判的一道门，
**不把「缺夹具」记成 RED**（真机那轮同一现象只能有一种判法）。

```sh
python -I -B tools/client/provision_real_machine_fixtures.py --gate     --db mir2_test_db_r --char aaa --map 3 --npc-script "盟重土城/药店" [--profile <profile.json>]
```

输出（实测，2026-10-25，只读）：

```json
{ "kind": "missing_prereq", "verdict": "UNDECIDABLE",
  "missing": ["gold", "floor_item", "monsters"],
  "reasons": {
    "gold": "characters.Gold=0 < 要求的 500（买卖无钱）",
    "floor_item": "地面物结构性不可达（…）⇒ pickup 阶段这一轮不能测",
    "monsters": "未给 --profile（无法证明该图有靶子夹具）" },
  "details": { "char": {...}, "gold": 0, "bag_rows": 46, "bag_items": 3, "npc": {...} } }
```

- `rc=0` + `verdict=PASS`（`missing` 为空）⇒ 动作可跑；
- `rc=6` + `kind=missing_prereq` ⇒ **UNDECIDABLE**，调用方必须把 `missing` 写进证据，不许静默 skip；
- 与 RED（rc≠0 且 `verdict` 非 UNDECIDABLE）**是两种 kind**，不许混。
- 检查项可用 `--need`（`gold`/`sellable_item`/`npc`/`floor_item`/`monsters`）逐项点；
  `--min-gold` 默认 500。
- `floor_item` **永远**报缺（结构性不可达，见 §3），直到 §3 的三条事实有一条改变。

## 5. A5 窗口当天 runbook

```sh
# ⓪ 动作前的前置门（只读）：PASS 才往下跑；UNDECIDABLE 就把 missing 记进证据
python -I -B tools/client/provision_real_machine_fixtures.py --gate --db mir2_test_db_r     --char aaa --map 3 --npc-script "盟重土城/药店" --profile <profile.json>

# ① 备夹具（只写 mir2_test_db_r）
python -I -B tools/client/provision_real_machine_fixtures.py --apply --db mir2_test_db_r \
    --char aaa --gold 5000 --game-point 10 --map 3 --npc-script "盟重土城/药店" \
    --yes-apply --report E:/tmp/fixtures-aaa.json

# ② 起栈 → 跑 mir_flow（本卡不跑；pickup 阶段**保持关闭**：无物可捡，开了只会 RED）
# ③ 登录后核对金币（帧 ↔ DB）
python -I -B tools/client/provision_real_machine_fixtures.py --verify \
    --frames <flow-out>/proxy/frames.ndjson --expect-gold 5000

# ④ 收尾回滚
python -I -B tools/client/provision_real_machine_fixtures.py --rollback E:/tmp/fixtures-aaa.json --yes
```

`--check`（只读、不连库）随时可看计划；`--a2-evidence`（只读）随时可复核 §3 的三条事实。

## 6. 自检

```sh
python -I -B tools/client/provision_real_machine_fixtures.py --selftest
```

29 条 PASS，覆盖：A4 三颗拒绝牙 + 白名单放行；A1 事务提交 / before 登记 / 失败 ROLLBACK /
MakeIndex 占用拦截 / 帧一致与不一致 / 无帧 PREREQ；A3 商店优先与指定脚本 / 文件:行号 /
只有 `#ACT` 的脚本不算 643 / 缺脚本不选 / 落点跳过墙格 / 缺地图 PREREQ；A6 缺项 ⇒ UNDECIDABLE
且点名 / 备齐 ⇒ PASS / 46 个空槽不算物品 / 地板物永远报缺 / profile 无靶子报缺；A2 三事实 BLOCKED +
事实被改 ⇒ REEVALUATE + SQL 无 `mapitem/ground`。

## 7. 残留物

- `E:/tmp/dev-2/*.py`：本卡的临时编辑脚本（会话结束可删）。
- `E:/tmp/fixtures-aaa.json`：窗口当天由 `--apply` 生成（含 before/after 与回滚语句），核对完保留到回滚后。
- 未起任何进程、未改 `E:/tmp/smoke/rust-root` 的 Envir/Map（全部只读）。
