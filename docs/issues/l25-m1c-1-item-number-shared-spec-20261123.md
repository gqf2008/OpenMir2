# M1c-1 · `item_number` 进程级共享 —— 实施规格（只读规格；不写码）

> 基线 master `7e54d4f9b`（本规格只规划、不改 `lib.rs`）。作者：车道 0c（复核者）。目标：把
> `item_number` 计数器改成进程级共享（`Arc<AtomicI32>`），使多区并发造件不再撞号，并给出可直接照做的改写与证据。
> **范围（opc-27 2026-11-23 裁决，本稿据此修订）：本片实施 ① Host `item_number` + ② `item_number_ex`（同形态、并入本片）；③ `World.item_number`（per-zone 掉落/商人）**不在本片**、开卡 `openmir2-world-item-number-per-zone`。§0 硬话：①② 是**必要非充分**，③ 未关门之前不得宣称 make_index 全局唯一。**

---

> **M1c-2a 覆盖声明（2026-11-23）**：本规格的 ② 段落**已被 M1c-2a 覆盖**——② `item_number_ex` **已删**
> （oracle `GetItemNumberEx` 是死码），GM-M 改走统一计数器 `mir2_formula::drop::next_item_number`
> （复位取 **oracle 主计数器语义** `> i32::MAX/2-1 ⇒ 1`，② 的 `MAX/2` 下限 clamp 不再存在）；
> ③ `World.item_number` **已并入同一 `Arc`**（`ItemNumberCounter::from_shared`，经 `sim_loop` 与
> `new_zone_state` 双注入）。计数器的**值不带 tick**（有意确定性偏离，T-72）。
> 下文「①② 必要非充分」「③ 未共享前不得称全局唯一」「7 处 fetch_add」均为 M1c-1 当时口径，
> 现以 `docs/issues/l25-m1c-2-plan-20261123.md` §3/§8 为准。

## 0. 范围与缺口（**必读，先看这条**）

`make_index` 的分配**不止一个计数器**。对着 `7e54d4f9b` 代码事实，至少有 **3 个独立计数器**在产 `make_index`：

| # | 计数器 | 类型/复位 | 归属 | 分配点 | A9 §3 裁决覆盖？ |
|---|---|---|---|---|---|
| **①** | Host `item_number` | `i32`，**不**复位 | `ProcessServices.item_number`（`lib.rs:34810`）| 7 处（§1）| **是**（本片）|
| **②** | Host `item_number_ex` | `i32`，`next_item_number_ex` **复位**(clamp `[MAX/2, MAX-1]`→`MAX/2`) | `ProcessServices.item_number_ex`（`lib.rs:34811`）| 1 处：GM-M `lib.rs:27186` | **是**（**并入本片**，opc-27 裁决）|
| **③** | `World.item_number` | `ItemNumberCounter`，`next_value` **复位**(`> MAX/2-1 ⇒ 1`) | **`World` 字段**（`crates/world-bevy/src/world.rs:1638`），**per-zone** | 怪物掉落 `generate_drops`（`world.rs:10385/10486`）+ 商人 `merchant.rs:1906` | **否**（**开卡 `openmir2-world-item-number-per-zone`**，不并入本片）|

**关键结论（opc-27 2026-11-23 裁决）**：
- **【硬话·必写】`item_number` 共享（①②）是必要非充分；③ 未共享前，跨区 `make_index` 全局唯一不成立；任何声称「号码已全局唯一」的报告，在 ③ 关门之前都不许写。**
- **① ② = 本片**；② 与 ① 同形态同代价（`Arc<AtomicI32>`+CAS），不并会让「共享」只覆盖一半造件路径、给人「已解决」的错觉 ⇒ **必须并入**。
- **③ = 开卡 `openmir2-world-item-number-per-zone`**（内容：`world.rs:1638` per-zone、`next_value` 复位、供 `generate_drops` 与 `merchant.rs:1906`）——③ 是**每区 `World` 一份**的独立复位计数器，N 区各一份会**照样撞号**（掉落遍布全区，是最常见造件源）。
- ⇒ **证据②（2K 件无重复）只能覆盖 ①② 的路径**，**不得混入怪物掉落或商人造件**（走 ③、本片未共享，会误报红）；这是**已知且已立卡的缺口，不是本片失败**。

---

## 1. 分配点全表（① Host `item_number` 7 处 + ② Host `item_number_ex` 1 处；①② 均并入本片；全部「先增后用」）

**语义统一**：7 处都是 `*item_number += 1;` **之后**才 `make_index: *item_number` ⇒ **「先增后用」**（用后增后的新值）。
**等价改写**（全部一致）：
```text
旧： *item_number += 1;  …  make_index: *item_number
新： let make_index = item_number.fetch_add(1, Ordering::Relaxed) + 1;  …  make_index
```
`fetch_add(1)` 返回**旧值**，`+1` 即**新值** = 原 `+=1` 后再读的值。7 处同一形式，无「取值后增」。

| # | 符号锚 | 所在函数 | 触发/段 | N 区同时走到？ | 改写 |
|---|---|---|---|---|---|
| 1 | `lib.rs:7442` `+=1` → `7447` `make_index` | `add_newbie_items`（`lib.rs:7419`，被登录路径调 @`7484`）| 登录新人礼包 | **待核**（今天 zone 0；M1c 若登录落区则随区）| `fetch_add(1)+1` |
| 2 | `lib.rs:10787` → `10792` | `write_back_player_bag`（`lib.rs:10697`，脚本/NPC 交易 `give` 写回）| NPC 交易 / 脚本 give | **是**（区 k 的 NPC 交易）| `fetch_add(1)+1` |
| 3 | `lib.rs:10843` → `10845` | `drop_full_bag_items`（`lib.rs:10821`，背包满/掉落包）| 背包满掉落 | **是**（per-zone）| `fetch_add(1)+1` |
| 4 | `lib.rs:27103` → `27105` | `apply_pending_gm_m_commands`（`lib.rs:26911`）| GM-M 给件 | **待核**（GM-M：zone 0 / 或下放区）| `fetch_add(1)+1` |
| 5 | `lib.rs:27168` → `27170`（`for _ in 0..req.count` 循环）| 同上 | GM-M 批量 | 同上 | `fetch_add(1)+1` |
| 6 | `shop.rs:730` → `732` | 商店购买 | 玩家商店购买 | **是**（per-zone）| `fetch_add(1)+1` |
| 7 | `useitem.rs:582` → `584` | 使用物品（开盒等）| 玩家用物品 | **是**（per-zone）| `fetch_add(1)+1` |

- **一眼可见 N 区并发点 = #2 #3 #6 #7**（NPC 交易 / 背包掉落 / 商店 / 使用物品）。#1 #4 #5 取决于 M1c 段集把「登录落区 / GM-M」放哪（待核）。
- **溢出等价性**：`i32` 起始约 4e5（`[Setup] ItemNumber=406897`），到 `i32::MAX` 需 ~2.1e9 次自增 ⇒ **单次运行不可达**（whitelist T-72：复位阈值 ≈1.07e9 亦不可达）。故 debug 下 `+=1` 的溢出 panic 不可达；`AtomicI32::fetch_add` 的 wrap 语义与 release `+=1` 一致 ⇒ **等价成立**。
- **不改值语义**：本片**不**引入 `GetTickCount`、**不**引入复位（与原 Rust host 模型一致；oracle `GetItemNumber` 的 `+GetTickCount`/复位本就不逐字节对拍，D-1 掩码）。`item_number_ex`（②）与 `World.item_number`（③）的复位语义**本片不动**（② 只是把复位原子化，逻辑不变）。

### 1.1 ② `item_number_ex`（1 处，并入本片；复位型）

- **分配点**：`apply_pending_gm_m_commands`（`lib.rs:26911`）内 `item.make_index = next_item_number_ex(item_number_ex);`（`lib.rs:27186`）。`next_item_number_ex`（`lib.rs:26781`）语义：`v = counter.wrapping_add(1); if v < MAX/2 { v = MAX/2 } if v > MAX-1 { v = MAX/2 }; counter = v; 返回 v`（**先增后用 + clamp/reset**）。
- **取触发/段**：GM-M 给件（与 ① 的 #4 #5 同函数），**待核**是否下放区；但**并入本片**与「M1c 该族是否入 LOCAL」解耦——只要它是 Host 计数、与 ① 同形态，就一起原子化。
- **等价改写**：`fetch_add` 不能直接表达 clamp/reset，须 **CAS 循环**（见 §3），返回新值 `v` 即原 `next_item_number_ex` 返回值。
  - **★ 澄清（防误写）**：现 `next_item_number_ex`（`lib.rs:26781`）是**普通 `&mut i32` 读-改-写**（`v = counter.wrapping_add(1); clamp; *counter = v`），**不是 CAS**；但它的**新值依赖旧值**（clamp/reset），所以**原子化不能是纯 `fetch_add`、必须是 CAS 循环**（`compare_exchange_weak`/`fetch_update`）。**写成纯 `fetch_add` 会静默丢掉 clamp 语义（不报错、只是行为变了）**——这是最难在测试里发现的一类。**① 才是纯 `fetch_add`，② 是 CAS**，两者有别。
- **搬迁**：与 ① 一起去 `SharedServices`（两个独立 `Arc<AtomicI32>`）。

---

## 2. 类型与放置

**结论（①② 同一处置）**：① `item_number`（`ProcessServices.item_number: i32`，`lib.rs:34810`）与 ② `item_number_ex`（`ProcessServices.item_number_ex: i32`，`lib.rs:34811`）**都移出 `ProcessServices`**，改为两个独立
`SharedServices` 字段 **`item_number: Arc<AtomicI32>`** 与 **`item_number_ex: Arc<AtomicI32>`**（加在 `SharedServices` 定义 `lib.rs:34784` 的 5 字段之后）。二者**独立**（不复用同一原子，避免 ①② 语义互串）。

**依据（为什么不是 `ProcessServices`）**：
1. **区 k 需要它，但区 k 拿不到 `ProcessServices`**：`ProcessServices` 是 `Option`，区 k（`spawn_minimal_zone`）传 `None`；任何 `PROCESS_ONLY` 段进入前 `proc_svc!` 断言 `process.is_some()`（`lib.rs:35154/35287/36616/...`）。造件路径（#2 #3 #6 #7）在 M1c 要移到 **`INBOUND_LOCAL`（区 k 执行）**，那时区 k 必须能取到 `item_number` ⇒ 它**不能**留在 `ProcessServices`。
2. **`SharedServices` 是唯一全段可达的共享上下文**：它以 `&mut SharedServices` 传入 `tick_phase_a/b/rest` 的**所有**段（区 0 与区 k 都拿）；其现有字段（`registry`/`enter_cfg`/`shard_runtime`/`zone_host`/`colors`）就是「全区可达」的定位。把 `Arc<AtomicI32>` 放这里，区 k 的 `INBOUND_LOCAL` 段即可在造件时 `fetch_add`。
3. **需同值共享 ⇒ `Arc`**：`SharedServices` 每区**各构造一份**（区 0 @`lib.rs:34515`、每区 k @`lib.rs:3461`），是副本；要让所有区用**同一个**计数器，必须持 `Arc` 克隆。

**创建点与克隆**：在多区装配作用域（`senders`/`receivers`/`host` 同级，约 `lib.rs:3285..3360`）创建**唯一** `let item_number: Arc<AtomicI32> = Arc::new(AtomicI32::new(item_number_counter_init(enter_cfg.newbie.item_number, &barrier_a)?))`（初值仍取 `item_number_counter_init` @`lib.rs:34344`，即 `[Setup]ItemNumber + 屏障A 抽数`）；区 0 的 `SharedServices`（`34515`）与每区 k 的 `SharedServices`（`3461`）都 `item_number: Arc::clone(&item_number)`。
- **N=1（无多区）**：`SharedServices` 只构造一次（`34515`），该 `Arc` 只有一份引用 ⇒ **单写者、零竞争**，序列与旧 `i32` 逐条相等。
- **② 同法**：`let item_number_ex: Arc<AtomicI32> = Arc::new(AtomicI32::new(cfg.item_number_ex))`（初值同现 `ProcessServices.item_number_ex` = `[Setup]ItemNumberEx`，缺省 `i32::MAX/2`，见 `config.rs`）；区 0（`34515`）与每区 k（`3461`）两份 `SharedServices` 各 `item_number_ex: Arc::clone(&item_number_ex)`。两个 `Arc` 都从同一装配作用域建、独立克隆。

**`&mut i32` 波及面**（类型改动的传导，必须一起改）：
- 所有 `item_number: &mut i32` 形参（`lib.rs` 约 15 处：`7424/7481/8095/8503/9148/10703/10825/11182/11560/11853/12542/26926/27637/27813/27883`）改为 `&AtomicI32`（或 `&Arc<AtomicI32>`，取一致口径）。
- `LoginCtx.item_number: &'a mut i32`（`lib.rs:31438`）改为持 `&'a AtomicI32`；**删** `finish_login` 里 `let mut item_number = **item_number;`（`lib.rs:31491`）的**拷入 + 末尾写回**模式——改后直接用 `AtomicI32`（不再需要把 `&mut` 拷成局部再写回，这是原「不能长持 `&mut`」的绕法）。
- 分配点（§1 的 7 处）按上表改成 `fetch_add(1, Relaxed)+1`。

**段集含义（这决定 M1c 段集怎么排）**：`item_number` 走 `SharedServices` ⇒ 造件命令族（新手/背包写回/掉落包/商店/使用物品/GM-M）**必须排在 `INBOUND_LOCAL`（区 k 跑），而非 `INBOUND_PROCESS`（`PROCESS_ONLY`）**。若留在 `PROCESS_ONLY`，区 k 仍造不了件，共享无意义。

---

## 3. 内存序

**结论**：`Ordering::Relaxed`。

**理由（一句）**：`make_index` 的唯一性**只依赖 `fetch_add` 的原子性**——单个 `AtomicI32` 有全修改序，**两次 `fetch_add` 必返回不同旧值**，与跨线程可见的先后/happens-before **无关**（`make_index` 只作唯一标记，不用于发布其它带序数据）⇒ `Relaxed` 足够；`SeqCst` 是多余的同步开销（高并发造件下更明显）。

- **② `item_number_ex`（并入本片）**：`next_item_number_ex` 的复位是**读-改-写**，须用 `compare_exchange_weak` 循环保持「clamp/reset」原子化，同样 `Relaxed`（唯一性论证同上）：
  ```text
  loop { let old = item_number_ex.load(Relaxed);
         let mut v = old.wrapping_add(1); if v < MAX/2 { v = MAX/2 } if v > MAX-1 { v = MAX/2 }
         if item_number_ex.compare_exchange_weak(old, v, Relaxed, Relaxed).is_ok() { break v } }
  ```
  - **① vs ② 的形态别混**：① 是纯 `fetch_add(1)+1`（值语义与 `+=1` 逐条相等）；② **必须 CAS**——因 clamp/reset 依赖旧值，用纯 `fetch_add` 会**静默丢掉 clamp 语义**（不报错、只是行为变了），实施时不要为「统一成 fetch_add」而改错 ②。
- ③ `World.item_number` 若共享（§0）：`next_value` 的 `wrapping_add(1)+复位` 同样改 CAS 循环；但它是 `World` 字段（`world.rs:1638`），改动面在 `World`/`generate_drops`/`merchant`，**不在本片**。

---

## 4. 两条证据的具体跑法

### 证据① N=1 两树逐字节
- **脚本位置（工作区工具，非仓库文件）**：`n1_master_equiv.ps1` **不在仓库内**——它在**协调工作区** `E:/OPC/tools/n1_master_equiv.ps1`（`a7` 编写，sha `a1ecf17c…`；用法说明 `E:/OPC/tools/README-m1b-n1-command.md`）。引用时一律写「工作区工具 `E:/OPC/tools/n1_master_equiv.ps1`」，**不得**写成仓内路径或 `tools/...`。
- **为什么单区序列不变（必写清）**：N=1 ⇒ 只有一个写者线程 ⇒ 对单 `AtomicI32` 的 `fetch_add(1,Relaxed)` 与旧 `i32 +=1; 读` 有**同一总序、同值序列**（没有第二个写者能插队）⇒ `make_index` 流逐条相等。
- **★ 口径警告（关键，防误读）**：`MakeIndex` 受 **D-1 掩码**（帧比较掩 `MakeIndex`、DB `characters_bagitem.MakeIndex` 在 `db.masked_columns`），且**未必出现在 stdout** ⇒ **N=1 两树逐字节证明的是「行为零变化」，不证明号码本身**；号码的正确性/唯一性**由证据②承担**。谁把「两树绿」当成「编号正确」都是误读。

### 证据② 并发 2K 件无重复
- **现有装置够不够 → 不够**。`run_l25_slice.ps1 -Zones 2` 能起两区并把两区的 `perf_counters.tsv`/`run.json` 落盘，但：
  - **不捕帧**（全文无 frames/ndjson 捕获步骤；只有 `perf_counters.tsv`/`rss.ndjson`/`run.json`/`summary.json`）；
  - **perf 是聚合计数、非逐值**（拿不到每个 `make_index` 的值）；
  - 无「物品布局 → make_index」的解码器现成可用。
  ⇒ **缺 make_index 逐值口径**。**口径（opc-27 2026-11-23 裁决：env-gated 分配旁路）**：
  - 实施切片加 **env-gated 分配旁路**（默认关、零参与）：每次成功 `fetch_add`（①）与 `next_item_number_ex` CAS 成功（②）各写一行 `(zone, value, counter)` 到侧路 TSV/日志，仅 `MIR2_ITEMNUM_TRACE=1` 时开。
  - 证据断言：**全局无重复**（按 `(counter,value)` 或直接 value，视 ①② 共享口径——两计数器独立标号，须分别验无重复）**且 样本数 ≥ 2×1000**。
  - （备选口径，若不想改码：探针捕 `SM_BAGITEMS(201)`/`SM_ADDITEM(200)` 帧按物品布局 `124B/名块15B/MakeIndex@100` 解，或 DB 后查 `characters_bagitem.MakeIndex` 重复；但需补解帧器，裁决已定用旁路。）
- **★★ 边界（回到 §0，写死）**：2K 件**只能**由 ①② 的路径产生——新手/背包写回/掉落包/商店购买/使用物品/GM-M 中**不属于 ③ 的那些**。**混入怪物掉落或商人造件**（走 ③ 的 per-zone `World.item_number`，本片未共享）**会误报重复**——这是**已知且已立卡（`openmir2-world-item-number-per-zone`）的缺口，不是本片失败**；证据脚本须显式限定造件来源，并在报告里援引该卡。

---

## 5. 回滚

- **实现（未来代码提交）单提交 revert 是否干净：是**。改动自包含（①② 两字段从 `ProcessServices` 移出 + `SharedServices` 加两个 `Arc` + `&mut i32`→`&AtomicI32` 传导 + ① 7 处 `fetch_add` + ② 1 处 CAS + 删 `LoginCtx` 拷入/写回）。revert 回到「每实例 `i32`」即恢复原状。
- **持久化副作用**：**无新增**。`make_index` **不是**持久全局序列——每次启动计数器从 `[Setup]ItemNumber + 屏障A 抽数` **重新 init**（`lib.rs:34344`），**不**从「已存的最大 MakeIndex」恢复。⇒ 回滚后新运行仍从配置起点号，可能与历史已发号重叠——**但这是现状本就如此**（跨重启 make_index 不保证全局唯一），revert 不使其更糟。
- **已发号码**：随下次启动失效；`MakeIndex` 在帧比较与 DB 比较里**均被 D-1 掩码**，不作对拍真相。**需确认**：无持久层把 `make_index` 当**跨运行全局唯一键**（若有，revert+号复用会撞键）；按现状它是**per-run 唯一标识**（in-memory 物品标识 per-run 唯一即可）。

---

## 6. 明确不做（非范围）

- **不改 `lib.rs`**（本文件只规划；实施是后续代码提交）。
- **不设计 M1c 其他部分**（段集整体排布、kick/AdminGm、控制邮箱等）。
- **不碰 kick/AdminGm**（A9 另一裁）。
- **② 并入本片、③ 不在本片**：② `item_number_ex` 已并入（§1.1/§2/§3）；③ `World.item_number`（掉落/商人，per-zone）**不在本片**，开卡 `openmir2-world-item-number-per-zone`（归 opc-27 建卡），本片不碰 `world.rs`/`generate_drops`/`merchant`。

---

## 7. 裁决落定（opc-27 2026-11-23；本稿已据此修订）

1. **② 并入本片、③ 开卡**：② `item_number_ex`（1 处 GM-M）并入（同 `Arc<AtomicI32>`+CAS）；③ `World.item_number` **开卡 `openmir2-world-item-number-per-zone`、不并入**，卡内容按 §0 表（`world.rs:1638` per-zone、`next_value` 复位、供 `generate_drops` 与 `merchant.rs:1906`）。
2. **证据②口径**：env-gated 分配旁路（默认关、零参与），断言全局无重复 + 样本 ≥2×1000；样本限 ①② 路径（排除 ③，③ 未共享属已知已立卡缺口，非本片失败）。
3. **证据①脚本**：用**工作区工具** `E:/OPC/tools/n1_master_equiv.ps1`（非仓库文件；`a7` 写，示例见 `E:/OPC/tools/README-m1b-n1-command.md`）；两树逐字节证「行为零变化」，号码正确性由证据②承担（§4 D-1 警告）。

---

## 8. 交付前自查（实施时）

- 7 处分配点全部 `fetch_add(1,Relaxed)+1`，无遗漏「取值后增」（本仓无此形态）。
- 所有 `item_number: &mut i32` 形参改 `&AtomicI32`；`LoginCtx` 拷入/写回删除；无 `&mut` 别名残留在原子下。
- 唯一 `Arc` 在装配作用域建、区 0 与各区 k 的 `SharedServices` 均 `Arc::clone`；N=1 只一份引用。
- 段集：造件族在 `INBOUND_LOCAL`；`proc_svc!` 的 `PROCESS_ONLY` 段**不再**引用 `item_number`（否则区 k 取不到）。
- 证据①② 按 §4 跑、② 限 ①② 路径（排除 ③）、① 注明 D-1 掩码口径（两树证行为零变化、不证号码）。
- ①② 两个 `Arc` 独立、都从装配作用域建并 clone 进区 0/各区 k 的 `SharedServices`；`item_number_ex` 的 CAS 保留 clamp/reset 语义。
- ③ 缺口按 §7：开卡 `openmir2-world-item-number-per-zone`、在报告援引；**③ 未关门不得宣称「make_index 全局唯一」**。
