# M1c-2c C1' 双区解包造件流水 —— 可执行运行计划（2026-11-24）

> **角色**：交给 `a7` 在独占窗口执行；**只有装置侧改动，产品代码零改动**。
> **上游**：`docs/issues/l25-m1c-2-plan-20261123.md` §4.1/§4.2（trace 旁路 + C1' 断言）、
> `docs/issues/l25-m1c-2c-eat-branch-spec-20261124.md` §5 分支门；A25 登记见
> `E:/OPC/docs/myriad-gap-list.md`（2026-11-24 新增④：`CM_EAT` 盒装解包 = 区 k 生产造件路径）。
> **锚点口径**：本文只用符号锚，不写行号（`bins/game-svr/**` 是高 churn 目录，prose 门会对行号锚判红）。

## 0. 一句话与它证明什么

2c 已把 `CM_EAT` 的**解包支**接到区 k 本地臂（`handle_zone_command_on` 的 `CM_EAT` 臂 →
`handle_eat_cmd(proc=None)` → `game_svr::useitem::use_item` 的 `StdMode 31 / AniCount 0` 支 →
`next_item_number`）。每吃一盒造 **6 件**，走**进程唯一**的 `Arc<AtomicI32>`。

⇒ **C1' 的双区解包流水 = A25 缺的"跨区 make_index 不重复"正面证据**：区 0 与区 k 的造件在
同一计数器上交错，`itemnum_trace` 的 `zone` 列直接给出两区归属，`DISTINCT == TOTAL` 即运行内
逐值无重复（复位窗口之间例外 = 既有语义，本跑远达不到复位点）。

⚠ 措辞边界（同 2a）：只能写「**单计数器内全局唯一**」「**跨跑确定**」；
**不得**写「与 oracle 值相等」（T-72：返回值不带 `HUtil32.GetTickCount()`）。

## 1. 代码面已具备的事实（执行前自检，不改代码）

- 发号单源：`mir2_formula::drop::next_item_number`（CAS 成功 → `mir2_shared::itemnum_trace::record(v)`）。
- trace：`MIR2_ITEMNUM_TRACE=<path>` 时写 `<path>`；首行 `# itemnum_trace v1 zone<TAB>make_index`，
  其后每行 `zone<TAB>make_index`（`zone` = 线程局部，区 0 线程与各区 k 线程各自 `set_thread_zone`）。
  env 未设 ⇒ 不建文件、只多一次 relaxed 原子读（`mir2_itemnum_trace_default_off` 牙）。
- 区 k 本地执行：分类器 `classify_local_event` 对 `StdMode 31 / AniCount 0` 给 `Local`；
  本地臂读 `SharedServices.unbind_list`（同一 `Arc` 实例）与 `SharedServices.item_number`。
- **容量事实（决定夹具形状，别按 167 盒硬塞）**：
  - `game_svr::bag::MAX_BAG_ITEM` = 46；`use_item` 解包门 = `item_list.len() + 6 - 1 <= MAX_BAG_ITEM`
    ⇒ **只有背包 ≤ 41 件时才能解包**。
  - DBSvr 读档槽数：`characters_bagitem` 46、`characters_storageitem` 50
    （`mir2-rs/bins/db-srv/src/playdata.rs` 的 `BAG_ITEM_SLOTS` / `STORAGE_ITEM_SLOTS`）⇒ 单角色单登录
    最多经手 96 盒；**167 盒/区必须分角色或分轮**。本计划用**多角色**（见 §3），一轮跑完、不改产品。
- **怪物会污染 trace**：`step_mon_gen` 对全图（不止玩家所在图）带 MonGen 条目的地图刷怪并预生成掉落，
  每次掉落都走同一计数器（2a 实测同型跑 5.5 万行）。⇒ 本跑必须**关 MonGen**（§3.3 的文件夹具；
  仓内无 env 开关，服务端只按 `Envir/MonGen.txt` 建宿主）。

## 2. 装置与最小改动

### 2.1 现有脚本的现实

- `run_l25_slice.ps1` **在盘上不存在**（计划 §4.2 的旧名）；双区实测装置 = `E:/OPC/tools/zone_double_send_run.ps1`
  （一次起停一套栈 + 两个 probe），env/位置夹具的封装范式 = `E:/OPC/tools/run_gm_trace.ps1`
  （`MIR2_TEST_PERMISSION` + `MIR2_ITEMNUM_TRACE` + 备份/还原位置 + driver 分离启动）。
- 本跑**不用 GM**（盒装解包是 `Local` 臂；用 `@s` 造盒会把分配记到区 0，正是 A25 已证不可达的形态）
  ⇒ **不设** `MIR2_TEST_PERMISSION`。

### 2.2 新 runner（装置，`E:/OPC/tools/run_c1prime_trace.ps1`）

职责（全部 fail-closed，任一不满足 ⇒ rc=2 并点名）：

1. **备份**：10 个角色的 `characters` 位置列、`characters_bagitem` 全行、`characters_storageitem` 全行、
   `characters_ablity` 的 `Hp/Mp`、`characters_item_attr` 中与这些角色背包 `MakeIndex` 相关的行；
   计算 sha256 存 `run_dir/db_backup.json`。
2. **模板发现**（记录进 `run_identity.json`）：
   `SELECT Id,Name,StdMode,Shape,AniCount FROM stditems WHERE StdMode=31 AND AniCount=0 AND Shape BETWEEN 100 AND 125 ORDER BY Id;`
   与栈内 `Envir/UnbindList.txt`（GBK；`段号 名字`）交叉：盒的 `Shape` 必须在表内，且解出名字在
   `stditems.Name` 里命中；**优选用药水类产物**（`金创药/魔法药/太阳水` 一类，见 UnbindList 100–105/114），
   这样 6 件产物能被下一动作吃掉、不需要扔地面；命中 0 或多于 1 类可选时按最小 `Id` 定一份并记录理由。
3. **可走性预检**：读地图文件（52B 头 + 12B/格，**列主序** `idx = x*height + y`），
   用与 `mir2_envir::mapfile::MapCell::attribute` 同一判定（`HighWall`/`LowWall` 为墙）断言 10 个格位全 `Walk`；
   任一为墙 ⇒ 换格位或换图，**不许**用"登录没被弹走"代替。
4. **背包夹具**：每角色写 46 行：Position 0..40 = 盒（`StdIndex`=盒 `Id`；`MakeIndex` 预置段
   **每角色不重叠**，如 `10_000_000 + i*100 + k`，避免 `characters_item_attr` 键冲突），41..45 = 空槽
   （`MakeIndex=0,StdIndex=0`）。写后逐行读回核对；同时快照**其它未声明槽位**的指纹（它们不得变）。
5. **起栈**：env 与 `zone_double_send_run.ps1` 同源——`MIR2_ZONES="0:<图>=0,0-<Xb-1>,<H-1>;1:<图>=<Xb>,0-<W-1>,<H-1>"`
   `MIR2_ZONE_SLICING=1`、`MIR2_ZONE_MIRROR=1`、`MIR2_PERF_COUNTERS=<run_dir>/perf_counters.tsv`、
   `MIR2_LOGIN_DB=<测试账号库>`、`MIR2_ITEMNUM_TRACE=<run_dir>/itemnum_trace.tsv`；`MIR2_TEST_PERMISSION` **不设**。
6. **跑 probe**：10 个 probe 进程并发（5 个落区 0、5 个落区 1），见 §4。
7. **断言 + 收尾**（§5/§6）。

### 2.3 probe 最小改动（装置，`tools/probe/mir2_probe.py`，先过工具门再入 master）

新增一个动作 **`eat_additems`**（为什么必须加：解包产物是**新分配**的 `make_index`，夹具无法预知；
只能从 `SM_ADDITEM(200)` 的 body 尾部解出）：

```json
{ "eat_additems": { "count": 6, "idents": [200], "interval_ms": 900, "timeout_ms": 4000 } }
```

语义：自当前游标起（最多 `timeout_ms`）收集 `count` 条 `SM_ADDITEM`；逐条 `EDCode.Decode` body 尾部 →
定长 124B `ClientItem`（`MakeIndex` @100，i32 LE；布局见 `tools/probe/param_discovery.py` 的模块文档与
`BAG_RECORD_LEN`）；按序 `CM_EAT(1006, recog=<make_index>, param=0, tag=0)` 发送，步间 `interval_ms`。
**fail-closed（scenario-error ⇒ rc=2）**：条数不足、解码失败、`MakeIndex <= 0`。
装置改动必须带 selftest（合成帧：2 条 200 → 解出两个 `make_index` 并断言发送序；少一条 ⇒ 拒绝）。

## 3. 夹具

### 3.1 角色（10 个，5/区）

- 预检：`mir2_test_db_r.characters` 里取 `ChrName IN ('loadbot10'..'loadbot19')` 且未删除、对应
  `mir2_test_account_r.account.Account` 存在（账号密码惯例 `loadbotN/loadbotN`）；缺失 ⇒ rc=2（不自建角色，
  避免建角路径的造件与清理面）。
- 位置：全部移到同一张无 MonGen 的图（§3.2）；5 个在区 0 一侧、5 个在区 1 一侧。

### 3.2 地图与格位

- **首选 GA0**（`秘密通道`=D701 为备选）：`Envir/MapInfo.txt` 有 `[GA0 庄园] SAFE DAY STALL`，
  `.map` 150×150（52B 头实测），`Envir/MonGen.txt` **无 GA0 条目** ⇒ 玩家所在图零刷怪。
- 区界 `Xb=75`：区 0 格 `(70,70..74)`，区 1 格 `(80,70..74)`（可走性预检不过 ⇒ 平移，记录最终格位）。
- 布局串：`0:GA0=0,0-74,149;1:GA0=75,0-149,149`。未提到的图由 `default_zone` 兜底（本跑统一关 MonGen，
  无其它分配源，兜底不影响断言）。

### 3.3 关 MonGen（文件夹具，必须）

运行前把 `E:/OPC/teststack/rust-root/M2GameSvr/Envir/MonGen.txt` 备份为 `MonGen.txt.c1prime.bak`，
写入只含注释的等价空表（`; c1prime: mon-gen disabled`），记录 sha256；**跑完立即还原并核对 sha256 相等**。
不关 ⇒ trace 里全是掉落分配，"样本只来自解包路径"不成立（§1 末条），本跑判废。

### 3.4 还原面（停栈后）

位置列、`characters_bagitem`/`characters_storageitem` 全行、`characters_ablity.Hp/Mp`、
`characters_item_attr`：删除**本次 trace 里出现过的 make_index** 对应的行（产物 attr），再按备份恢复；
逐项读回相等才写 `RESTORE_OK=True`，任一不等 ⇒ 整跑作废并点名。

## 4. 每角色动作流（场景生成器产出，生成物落 `run_dir/scen/`，不入仓）

1. 登录 / 进世界 / settle（沿用 probe 既有登录帧与 `dwell`）。
2. 重复 **41 轮**：
   - `send {ident: 1006, recog: <盒的预置 make_index>}`（`CM_EAT`，12B 头 `Recog` = `make_index`）；
   - `wait_for {idents: [635]}`（`SM_EAT_OK`；出现 `636` ⇒ 立即失败并报"解包门拒绝"）；
   - `eat_additems {count: 6}`（吃掉 6 件产物；见 §2.3）；
   - `sleep 900`。
3. 收尾：无 GM、无丢弃、无商人交互、无移动（除登录落点）。

**动作间隔依据（peer 指定要核）**：
- 850ms 的来源 = oracle 场景口径（`CheckActionStatus` + `AttackInterval`/`MagicAttackInterval`，
  见 `E:/OPC/tools/scenarios/zone_b_hit_B.json` 的"动作间隔守门"注释），用于**将来与 C# 同场景复跑**。
- **本栈服务端无此门**：`Server.conf` 未配 `CloseSpeedHackCheck` ⇒ 启动取缺省 true（跳过限速）；
  `movewire`/`attackwire` 模块头明确登记"限速/超速/`CheckActionStatus` **未接**"。
  ⇒ `CM_EAT`/`SM_ADDITEM` 之间没有服务端限速依赖；间隔只决定窗口时长与保序余量。
- 结论：**默认 900ms**（沿用 oracle 口径、留富余）；窗口紧时允许降到 350ms，但报告必须写明
  「本轮为 Rust-only、无 oracle 对照，间隔不与 C# 场景等价」——**不得**把 350ms 的跑当成 oracle 可比证据。

## 5. 断言（全部写进 `run_dir/summary.txt`，缺一不判绿）

**trace 口径**（`itemnum_trace.tsv`，去首行注释；每行 `zone<TAB>value`）：

| 断言 | 值 |
| --- | --- |
| `TRACE_TOTAL_ROWS` | **= 2460**（10 角色 × 41 盒 × 6 件；不等 ⇒ 有其它分配源，先查再判） |
| `TRACE_ZONE_0_ROWS` / `TRACE_ZONE_1_ROWS` | **各 = 1230**，且均 **≥ 1002** |
| `TRACE_DISTINCT_VALUES` | **= TRACE_TOTAL_ROWS** |
| `TRACE_DUP` | **= 0**（`TRACE_TOTAL_ROWS - TRACE_DISTINCT_VALUES`） |
| `CROSS_ZONE_DUP` | **= 0**（同一 value 同时出现在 zone 0 与 zone 1 的行数） |
| `TRACE_TOTAL_ROWS >= 2004` | 是（样本量口径） |

**探针/场景口径**：
- 每角色 `BOX_EATS == 41`、`PRODUCT_EATS == 246`、`SM_EAT_FAIL(636) == 0`、`probe rc == 0`；
- 10 个 probe 的登录都 `InWorld`，无踢线；
- **场景无怪无商人**：`summary` 里记 `MONGEN_DISABLED=1`（文件夹具 sha 核对）+ 服务端日志
  `grep -c` 刷怪行为 0（若日志无该字样，以"MonGen 空表 + trace 严格等值"两项合证）+ 场景无商人动作。

**装置/运行身份**（防陈旧台账假绿）：`run_identity.json` 记 `run_id`、exe 的 sha256 与内嵌 40-hex、
`start_wall/end_wall`（+0800）、`MIR2_ZONES`/布局/格位、盒模板 `Id/Shape`、UnbindList 产品名、
MonGen 夹具 sha256（前后）、`ARM_BUCKETS`。

## 6. 反例（必做；变异只在**临时树**，不入 master）

- 临时树：`git clone --shared` 合入 head 到 `E:/UserTemp/c1prime-mut-<sha8>`；
- 补丁（照 2a 的"每区独立计数"形态）：在 `sim_loop` 的 `item_number.store(item_number_init, …)` 之后，
  把 `zone_host.zone_states` 里每个区的 `host.world.item_number` 换成
  `Arc::new(AtomicI32::new(item_number_init))`（**同初值**，保证两区序列相撞）；
  记录补丁 diff + 构建产物 sha（release 构建须在开窗前完成）。
- 用同一 runner 跑**一轮**（5 角色/区即可）：预期 `TRACE_DUP > 0` 且 `CROSS_ZONE_DUP > 0` ⇒ 检查器红。
  把红输出原文入档；还原临时树 `git status` 空。

## 7. 收尾清单（每项写进 `summary.txt`）

- [ ] 停栈：`mir2-server`/`game-svr`/gate 进程 0；六端口 `LISTENING=0`；
- [ ] `MonGen.txt` 已还原且 sha256 与备份相等；文件里无 `c1prime` 残留；
- [ ] 10 角色 DB 还原：位置 / `characters_bagitem` / `characters_storageitem` / `characters_ablity` /
      `characters_item_attr`（法：§3.4），`RESTORE_OK=True`；
- [ ] trace、`run_dir`、场景生成物、变异对比件全在 `E:/UserTemp/`（或 `E:/OPC`）下，**不入仓**；
- [ ] 主检出与本 worktree `git status` 空；probe 改动若已合 master，其树同样空；
- [ ] 无新增 cargo 构建残留（release 构建在开窗前完成；窗口期禁 cargo）；
- [ ] 临时物清单（路径 + 大小）写进 `summary.txt`。

## 8. 报给 Leader 的字段

`RUN_ID` / exe sha256 + 内嵌 head / `TRACE_TOTAL_ROWS` / `TRACE_ZONE_{0,1}_ROWS` /
`TRACE_DISTINCT_VALUES` / `TRACE_DUP` / `CROSS_ZONE_DUP` / 每角色 `BOX_EATS`、`PRODUCT_EATS` /
`RESTORE_OK` / `MONGEN_DISABLED` + 夹具 sha / 变异跑 `DUP/CROSS_ZONE_DUP` 原文 / 收尾四项（进程/端口/仓/临时物）。

## 9. 风险与已知边界

- **覆盖面**：只覆盖**解包路径**（脚本/商店/GM 造件仍留区 0）；报告必须写清，不得外推"所有造件跨区唯一"。
- **容量**：46 槽 + 解包门 ≤41 是硬事实；本计划用 5 角色/区绕开，**不改产品**。
  若将来要"单角色 167 盒"，需要产品侧改容量或做轮次 refill（另卡，不在本跑）。
- **D-1 掩码**：帧/DB 对拍掩 `MakeIndex`；本跑的绿**不证**帧面编号，只证计数器流水唯一性。
- **镜像开启**：`MIR2_ZONE_MIRROR=1` 是驱动缺省（跨区收件人路径）；本跑不产生镜像收件人，不影响 trace；
  若窗口对镜像有异议，可关（`MIR2_ZONE_SLICING=1` 要求 mirror=1，见 `zone::ENV_ZONE_SLICING` 的 fail-closed）。
- **复位窗口**：run 内分配 ~2.5k 次，离 `i32::MAX/2-1` 极远，复位不参与；若 trace 出现回落值 ⇒ 判废。
