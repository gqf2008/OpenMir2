# M1c-2c C1' 双区解包造件流水 —— 可执行运行计划（2026-11-24 · rev2）

> **角色**：交给 `a7` 在独占窗口执行；**产品代码零改动**，只动探针与栈数据夹具。
> **rev2 变更**（按 `a7` 评审 + `opc-27` 裁决）：盒模板定 **Id 41「随机传送卷包」**；产物用
> `CM_EAT` **吃掉**（`CM_DROPITEM` 在本仓/oracle 恒失败，T-19，弃用）；两图分区（GA0 区 0 /
> D701 区 1）；加 `STARTUP_ROWS==0` 门与 GuardList 夹具；`characters`/`characters_ablity` 整行备份；
> 表名全限定 `mir2_test_data_r.stditems`；补 §3.3 MonGen/GuardList 独立拷贝事实。
> **上游**：`docs/issues/l25-m1c-2-plan-20261123.md` §4.1/§4.2、
> `docs/issues/l25-m1c-2c-eat-branch-spec-20261124.md` §5；A25 见 `E:/OPC/docs/myriad-gap-list.md`。
> **锚点口径**：只用符号锚，不写行号（`bins/game-svr/**` 是高 churn 目录，prose 门对行号锚判红）。

## 0. 一句话与它证明什么

2c 已把 `CM_EAT` 的**解包支**接到区 k 本地臂（`handle_zone_command_on` 的 `CM_EAT` 臂 →
`handle_eat_cmd(proc=None)` → `game_svr::useitem::use_item` 的 `StdMode 31 / AniCount 0` 支 →
`next_item_number`）。每吃一盒造 **6 件**，走**进程唯一**的 `Arc<AtomicI32>`。

⇒ **C1' 的双区解包流水 = A25 缺的"跨区 make_index 不重复"正面证据**：区 0 与区 k 的造件在
同一计数器上交错，`itemnum_trace` 的 `zone` 列直接给出两区归属，`DISTINCT == TOTAL` 即运行内
逐值无重复（复位窗口之间例外 = 既有语义，本跑远达不到复位点）。

⚠ 措辞边界（同 2a）：只能写「**单计数器内全局唯一**」「**跨跑确定**」；
**不得**写「与 oracle 值相等」（T-72：返回值不带 `HUtil32.GetTickCount()`）。

## 1. 代码面事实（执行前自检，不改代码）

- 发号单源：`mir2_formula::drop::next_item_number`（CAS 成功 → `mir2_shared::itemnum_trace::record(v)`）。
- trace：`MIR2_ITEMNUM_TRACE=<path>` 时写 `<path>`；首行 `# itemnum_trace v1 zone<TAB>make_index`，
  其后每行 `zone<TAB>make_index`（`zone` = 线程局部，区 0 线程与各区 k 线程各自 `set_thread_zone`）。
  env 未设 ⇒ 不建文件、只多一次 relaxed 原子读（`mir2_itemnum_trace_default_off` 牙）。
- 区 k 本地执行：分类器 `classify_local_event` 对 `StdMode 31 / AniCount 0` 给 `Local`；
  本地臂读 `SharedServices.unbind_list`（同一 `Arc`）与 `SharedServices.item_number`。
- **运行 head 必须 ≥ `c2fd48497`**（区 k `CM_EAT` 本地臂是 2c 合并后才有的）：
  开窗前对上窗 exe 的源码树做符号检查——`handle_zone_command_on` 体内存在
  `if cmd.ident == game_svr::useitem::CM_EAT` 且其调用 `handle_eat_cmd` 的 `proc` 实参为 `None`；
  exe 内嵌 40-hex 与 `git merge-base --is-ancestor c2fd48497 <head>` 双证。
- **容量事实**：`game_svr::bag::MAX_BAG_ITEM`=46；解包门 `item_list.len()+6-1<=46` ⇒ 背包 ≤41 才能吃；
  DBSvr 槽数 `BAG_ITEM_SLOTS`=46、`STORAGE_ITEM_SLOTS`=50 ⇒ **单角色单登录最多 96 盒**。
  本计划用**多角色**（§3.1）一轮跑完。
- **模板事实**（`mir2_test_data_r.stditems` 实查；`StdMode=31 AND AniCount=0` 只有 5 个）：
  | 盒 Id | 名字 | Shape | UnbindList[Shape] 产物 | 产物 Id / StdMode / Shape | 可用性 |
  | --- | --- | --- | --- | --- | --- |
  | 40 | 地牢逃脱卷包 | 106 | 地牢逃脱卷 | 37 / 3 / 1 | ✗（case1 回 HomeMap 随机点，可能换图） |
  | **41** | **随机传送卷包** | **107** | **随机传送卷** | **36 / 3 / 2** | **✓ `OtherLocal`：当前图内随机移动，不跨区** |
  | 42 | 回城卷包 | 108 | 回城卷 | 38 / 3 / 3 | ✗（case3 回 Home 精确点，可能换图） |
  | 43 | 行会回城卷包 | 109 | 行会回城卷 | 39 / 3 / 5 | ✗（`MoveFamily` → `Process`，区 k 不执行） |
  | 44 | 筹码包 | 164 | **UnbindList 无 164** | — | ✗（解包 0 件 ⇒ 零分配，无证据价值） |
- **`CM_DROPITEM` 不能用**：本仓 1:1 复刻 oracle 既有形态——`MapItem.ItemId` 恒 0 ⇒
  `AddItemToMap` 恒 false ⇒ 普通丢弃**恒回 `SM_DROPITEM_FAIL(601)`、背包不动**（whitelist T-19）。
  ⇒ 产物只能**吃掉**（§2.3/§4）。
- **启动期分配源**：`generate_drops` 在**怪物生成**时预算掉落（`World::budget_monster_drops`）；
  `GuardList.txt`（测试栈实有 77 行）经 `place_guards` → `regen_monster_by_name` 生成守卫 ⇒
  **启动即分配**；`step_mon_gen` 对全图带 MonGen 条目的地图持续生成 ⇒ 持续分配。
  ⇒ 两者都必须**关**（§3.3），否则 `STARTUP_ROWS==0` 不成立、trace 也不是"解包专属"。

## 2. 装置与最小改动

### 2.1 现有脚本的现实

- `run_l25_slice.ps1` **不存在**（旧名）。双区装置范式 = `E:/OPC/tools/zone_double_send_run.ps1`
  （一次起停 + probe）+ `E:/OPC/tools/run_gm_trace.ps1`（env/位置夹具/备份还原/driver 分离启动）。
- 本跑**不用 GM**（`@s` 造盒是 `Process` ⇒ 分配记区 0，正是 A25 已证不可达形态）⇒ **不设**
  `MIR2_TEST_PERMISSION`。

### 2.2 新 runner（`E:/OPC/tools/run_c1prime_trace.ps1`，全部 fail-closed）

1. **备份整行**：10 角色的 `characters`、`characters_ablity` **整行**（含 `HungerStatus`/`BodyLuck`/
   `Weight` 等一切存档列）；`characters_bagitem`/`characters_storageitem` 全行；
   `characters_item_attr` 中这些角色背包 `MakeIndex` 的相关行。逐表存 TSV + sha256 进 `db_backup.json`。
2. **模板断言**（`mir2_test_data_r.stditems`，表名全限定）：
   `Id=41` 行必须为 `Name=随机传送卷包, StdMode=31, AniCount=0, Shape=107`；
   `UnbindList[107]` 必须解出 `随机传送卷`；产物 `Id=36` 必须 `StdMode=3, Shape=2`；
   同时断言 §1 表里 40/42/43/44 的排除理由成立（形状/无 164），任一不符 ⇒ rc=2。
3. **地图/格位预检**：布局 `0:GA0=0,0-149,149;1:D701=0,0-199,199`；
   区 0 五格 `(70,70)..(74,70)`（GA0）、区 1 五格 `(14,10)..(18,10)`（D701）——已用与
   `mir2_envir::mapfile::MapCell::attribute` 同判定（52B 头 + 12B/格、**列主序** `x*h+y`、`bk/fr` bit15）
   实测全 `Walk`；runner 每次开跑**重验**。另断言两图 `MapInfo` 无 `NORANDOMMOVE`
   （随机传送的 `case2` 门；有 ⇒ `SM_EAT_FAIL` 且**不消费** ⇒ 满包）。
4. **背包夹具**：每角色 46 行：Position 0..40 = 盒（`StdIndex=41`；`MakeIndex` 预置段**每角色不重叠**，
   如 `10_000_000 + i*100 + k`），41..45 = 空槽（`MakeIndex=0,StdIndex=0`）。写后逐行读回；
   其它未声明槽位指纹不得变。
5. **起栈**：`MIR2_ZONES=<布局>`、`MIR2_ZONE_SLICING=1`、`MIR2_ZONE_MIRROR=1`、
   `MIR2_PERF_COUNTERS=<run_dir>/perf_counters.tsv`、`MIR2_LOGIN_DB=<测试账号库>`、
   `MIR2_ITEMNUM_TRACE=<run_dir>/itemnum_trace.tsv`；`MIR2_TEST_PERMISSION` **不设**。
6. **STARTUP_ROWS 门**：栈就绪（端口 LISTENING、进世界闸 Ready）后、起 probe **之前**读 trace：
   行数必须 **== 0**；非 0 ⇒ 停手逐源排查（守卫/MonGen/城堡等），给源做文件夹具后重跑；
   **禁止**把启动行从 `TOTAL` 里减掉凑数。
7. **跑 probe**：10 个 probe 并发（5 个在区 0/GA0，5 个在区 1/D701），动作流见 §4。
8. **断言 + 收尾**（§5/§7）。

### 2.3 probe 最小改动（`tools/probe/mir2_probe.py`，先过工具门）

新增动作 **`eat_additems`**（解包产物是新分配的 `make_index`，夹具无法预知；只能从
`SM_ADDITEM(200)` body 尾部解出）：

```json
{ "eat_additems": { "count": 6, "idents": [200], "interval_ms": 900, "timeout_ms": 4000 } }
```

语义：自当前游标起（最多 `timeout_ms`）收集 `count` 条 `SM_ADDITEM`；逐条 `EDCode.Decode` body 尾部 →
定长 124B `ClientItem`（`Name`@0、`MakeIndex`@100 i32 LE；布局见 `tools/probe/param_discovery.py`
的 `BAG_RECORD_LEN`）→ 按序 `CM_EAT(1006, recog=<make_index>, param=0, tag=0)`，步间 `interval_ms`。
**fail-closed（scenario-error ⇒ rc=2）**：条数不足、解码失败、`MakeIndex<=0`。
**不要**实现任何 drop 逻辑（§1：恒失败）。装置改动带 selftest（合成 2 条 200 ⇒ 解出两个
`make_index` 并按序发送；少一条 ⇒ 拒绝）。

## 3. 夹具

### 3.1 角色（10 个，5/区）

- 预检：`mir2_test_db_r.characters` 取 `loadbot10..loadbot19`（未删除）且
  `mir2_test_account_r.account.Account` 存在；缺失 ⇒ rc=2（不自建角色，避免建角造件面）。
- 位置：区 0 五个放 GA0 `(70..74,70)`；区 1 五个放 D701 `(14..18,10)`。
- 两图**整图各属一个区**（布局 ± 全图矩形）⇒ 随机传送只在当前图内 ⇒ **不跨区**（§1 表 41 行）。

### 3.2 地图

- 区 0 = **GA0**（`Envir/MapInfo.txt`：`[GA0 庄园] SAFE DAY STALL`，150×150，**无 MonGen 条目**）；
- 区 1 = **D701**（`[D701 秘密通道] DAY`，200×200，**无 MonGen 条目**；`y=100` 那排是墙，勿用）。
- 布局串：`0:GA0=0,0-149,149;1:D701=0,0-199,199`；未提到的图由 `default_zone` 兜底（本跑 MonGen 已关，
  无其它分配源，兜底不影响断言）。

### 3.3 关启动期/持续分配源（文件夹具，**只在 teststack 副本**）

**路径事实（只读实测，2026-11-24）**：`E:/OPC/teststack/rust-root/M2GameSvr/Envir` 是**真实目录**
（非 reparse point/软链）；其 `MonGen.txt` 与 `E:/MirServer/M2GameSvr/Envir/MonGen.txt`
**File ID 不同、各自只有自身一条 hardlink、当前内容同 sha** ⇒ **独立拷贝**。
⇒ 夹具只动 teststack 这份；`E:/MirServer/...` **不碰**（N=1/真机口径）。

| 文件 | 基线 sha256（2026-11-24 实测） | 动作 |
| --- | --- | --- |
| `E:/OPC/teststack/rust-root/M2GameSvr/Envir/MonGen.txt` | `E5C39FB7EDA66B70F82A61A5C2CEA25DF9CF0B6892B916FB6D579F88FF43748F`（126,292B） | 备份 `.c1prime.bak` → 写只含注释的空表 |
| `E:/OPC/teststack/rust-root/M2GameSvr/Envir/GuardList.txt` | 77 行（实测行数；sha 开窗时现取并记入 summary） | 同法：备份 → 空表（守卫生成走 `generate_drops`，启动即分配） |

跑完**逐一还原**并断言 sha256 与备份相等；任一不等 ⇒ 整跑作废。

### 3.4 还原面（停栈后，整行读回相等才写 `RESTORE_OK=True`）

`characters` 整行、`characters_ablity` 整行、`characters_bagitem` 全行、`characters_storageitem` 全行、
`characters_item_attr`：先删**本次 trace 中出现过的 make_index** 对应的行（产物 attr），再按备份恢复。

## 4. 每角色动作流（场景生成器产出，落 `run_dir/scen/`，不入仓）

1. 登录 / 进世界 / settle。
2. 重复 **41 轮**：
   - `send {ident: 1006, recog: <盒的预置 make_index>}`（`CM_EAT`，12B 头 `Recog` = `make_index`）；
   - `wait_for {idents: [635]}`（`SM_EAT_OK`；出现 `636` ⇒ 立即失败并报"解包门拒绝"）；
   - `eat_additems {count: 6}`（吃掉 6 件「随机传送卷」；每次吃会把自己随机传送到当前图另一格，
     包内净 −6）；
   - `sleep 900`。
3. 无 GM、无丢弃、无商人、无手动移动。

**动作间隔依据（`opc-27` 指定措辞）**：850/900ms 只是沿用 oracle 场景的口径**形式**；
**本栈是 Rust-only，服务端对 `CM_EAT` 无间隔依赖**——`Server.conf` 未配 `CloseSpeedHackCheck`
⇒ 取缺省 true（跳过限速），`movewire`/`attackwire` 模块头登记"限速/`CheckActionStatus` 未接"。
报告必须写「**Rust-only，无服务端间隔依赖，不与 C# 场景等价**」。默认 900ms；窗口紧可 350ms。

## 5. 断言（写进 `run_dir/summary.txt`，缺一不判绿）

**trace 口径**（去首行注释；行 = `zone<TAB>value`）：

| 断言 | 值 |
| --- | --- |
| `STARTUP_ROWS` | **== 0**（§2.2-6 的门；非 0 ⇒ 停手排源，不许减数） |
| `TRACE_TOTAL_ROWS` | **== 2460**（10 角色 × 41 盒 × 6 件） |
| `TRACE_ZONE_0_ROWS` / `TRACE_ZONE_1_ROWS` | **各 == 1230**，且均 **≥ 1002** |
| `TRACE_DISTINCT_VALUES` | **== TRACE_TOTAL_ROWS** |
| `TRACE_DUP` | **== 0** |
| `CROSS_ZONE_DUP` | **== 0**（同一 value 同时出现在 zone 0 与 zone 1 的行数） |
| `TRACE_TOTAL_ROWS >= 2004` | 是（样本量口径；容量案见 §1） |

**探针/场景口径**：每角色 `BOX_EATS == 41`、`PRODUCT_EATS == 246`、`SM_EAT_FAIL(636) == 0`、
`probe rc == 0`；10 个 probe 登录 `InWorld` 无踢线；`MONGEN_DISABLED=1` 与 `GUARDLIST_DISABLED=1`
（两夹具 sha 前后核对）；无商人/丢弃动作。

**运行身份**：`run_identity.json` 记 `run_id`、exe sha256 + 内嵌 40-hex（并断言
`merge-base --is-ancestor c2fd48497 <head>`）、`start_wall/end_wall`（+0800）、布局/格位、盒与产物
`Id/Shape`、两夹具 sha（前后）、`ARM_BUCKETS`。

## 6. 反例（必做；变异只在**临时树**，不入 master）

- 临时树：`git clone --shared` 运行 head 到 `E:/UserTemp/c1prime-mut-<sha8>`；
- 补丁（"每区独立计数器、同初值"的最小等价形态，绕开 init 时序）：
  在 `handle_zone_command_on` 的 `CM_EAT` 本地臂里，把传给 `handle_eat_cmd` 的 `item_number`
  从 `env.item_number` 换成**线程局部** `thread_local! { static MUT_COUNTER: AtomicI32 =
  AtomicI32::new(412_071) }` 的引用（每个区线程各一份、同初值 ⇒ 两区序列相同）。
  记补丁 diff。
- **release 构建必须在开窗前完成**（约 6–10 分钟、峰值内存 ~1.6G，窗口期禁 cargo）；
  产物 sha256 + 内嵌 40-hex（含 dirty 标记）记进 summary。
- 用同一 runner 跑**一次起停**、每区 1 角色 × 5 盒（每区 30 件）：预期 `TRACE_DUP == 30`、
  `CROSS_ZONE_DUP == 30` ⇒ 检查器红；红输出原文入档。跑完临时树 `git status` 空。

## 7. 收尾清单（每项进 `summary.txt`）

- [ ] 停栈：`mir2-server`/`game-svr`/gate 进程 0；六端口 `LISTENING=0`；
- [ ] `MonGen.txt`、`GuardList.txt` 已还原且 sha256 与备份相等，无 `c1prime` 残留；
- [ ] 10 角色：`characters`/`characters_ablity` 整行、bag/storage/item_attr 按 §3.4 还原，
      `RESTORE_OK=True`；
- [ ] trace、`run_dir`、场景生成物、变异件全在 `E:/UserTemp/`（或 `E:/OPC`）下，**不入仓**；
- [ ] 主检出与本 worktree `git status` 空；
- [ ] 无窗口期 cargo 构建残留（正向/反例两个 exe 均开窗前构建，记 sha）；
- [ ] 临时物清单（路径 + 大小）写进 `summary.txt`。

## 8. 报给 Leader 的字段

`RUN_ID` / 正向 exe sha256 + 内嵌 head / `STARTUP_ROWS` / `TRACE_TOTAL_ROWS` /
`TRACE_ZONE_{0,1}_ROWS` / `TRACE_DISTINCT_VALUES` / `TRACE_DUP` / `CROSS_ZONE_DUP` /
每角色 `BOX_EATS`、`PRODUCT_EATS` / `RESTORE_OK` / `MONGEN_DISABLED`、`GUARDLIST_DISABLED` + 模板断言原文 /
反例 exe sha + `DUP/CROSS_ZONE_DUP` 红原文 / 收尾四项（进程/端口/仓/临时物）。

## 9. 风险与已知边界

- **覆盖面**：只覆盖**解包路径**（脚本/商店/GM 造件仍留区 0）；报告必须写清，不得外推"所有造件跨区唯一"。
- **容量**：46 槽 + 解包门 ≤41 是硬事实；本跑用 5 角色/区绕开，**不改产品**。
- **D-1 掩码**：帧/DB 对拍掩 `MakeIndex`；本跑的绿**不证**帧面编号，只证计数器流水唯一性。
- **镜像**：`MIR2_ZONE_SLICING=1` 要求 `MIR2_ZONE_MIRROR=1`（`zone::ENV_ZONE_SLICING` 的 fail-closed）；
  本跑不产生镜像收件人，不影响 trace。
- **随机传送**：`case2` 需要当前图无 `NORANDOMMOVE`（§2.2-3 预检）；传送只改位置、不改会话所属区，
  且两图整图各属一区 ⇒ 不跨区；若落点不可走，移动失败但**物品照常被吃掉**（`maybe_eat_move`
  先吃后移），不影响流水。
- **复位窗口**：run 内分配 ~2.5k 次，离 `i32::MAX/2-1` 极远；trace 出现回落值 ⇒ 判废。
