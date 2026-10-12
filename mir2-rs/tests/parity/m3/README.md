# M3 对拍夹具（同库同脚本）

D2 线交付（对应设计文档 §6 M3「同序列对拍」）。owner 2026-10-10 决策：**本夹具即 M3 正式门禁**
（判据 = `run_pair.ps1` + `scoped_diff.py diff --canonical`，作用域内字段级 diff = 0，且 C# 基线一条命令可复跑）。
判据纪律照 §6.0：可测、可证伪、差异要么 0 要么进 `tests/parity/whitelist.md`、证据四件套。

## M3 正式门禁：一条命令

```powershell
# ① C# 基线（含"两次运行的确定性前缀 hash 相同"这条稳定性判据）
powershell -ExecutionPolicy Bypass -File mir2-rs/tests/parity/m3/run_m3_gate.ps1 -Side csharp -VerifyRepeat

# ② M1 顶住 7000/7100/7200 之后的 Rust 侧首验（C# 会话作参照 ⇒ 跨侧 diff 必须 0）
powershell -ExecutionPolicy Bypass -File mir2-rs/tests/parity/m3/run_m3_gate.ps1 -Side rust -PeerSession csharp-20261010-164731
```

### 用修好的比较器重比既有产物（不必重跑整栈）

门禁第 3 步（跨侧作用域 diff）不依赖活服务——只要有已落地的会话目录，就能单独重跑比较。
`run_m3_gate.ps1 -PeerSession` 走的就是下面这条命令（`-FixedNewChr` 时再加 `--fixed-newchr`）：

```powershell
python mir2-rs/tests/parity/m3/scoped_diff.py diff \
  mir2-rs/tests/parity/m3/sessions/<peer会话>/scoped_after_norm.jsonl \
  mir2-rs/tests/parity/m3/sessions/<本侧会话>/scoped_after_norm.jsonl --canonical
```

例：用同一对 C# 会话验证比较器本身（旧产物直接重比，不碰 5000/7000/7100/7200）：

```powershell
python mir2-rs/tests/parity/m3/scoped_diff.py diff \
  mir2-rs/tests/parity/m3/sessions/csharp-20261010-164731/scoped_after_norm.jsonl \
  mir2-rs/tests/parity/m3/sessions/csharp-20261011-110827/scoped_after_norm.jsonl
# 未 canonical ⇒ RED 8 行（exit 1）；加 --canonical ⇒ GREEN（exit 0）
```

### `-Side rust` 首验的两条前置（否则会把别人的问题读成 A2 的问题）

1. **A2（M1 边界服务）的 Rust 服务真的顶住 7000/7100/7200**——门禁的入口护栏会拦"入口还是 C#"，
   但拦不住"入口是 Rust 但链路半通"；
2. **A3 的 s2c 多段体帧编码器修复合入 master**（2026-10-10 状态：`worktree-c-tools` @ `02ba88ef`，
   needs-review）。未合入时经 Rust 链路的 `SM_TURN` 家族（`enc(CharDesc)+enc(文本)`）会跳帧，
   产生**假差异**——那属于编码器口径，不是 A2 的服务行为差异。
   相关口径差另见 whitelist **A-9**（C2 导出器对多段帧的 `body_len/body_sha256` 与分段解码不一致）：
   跨侧 diff 若消费 C2 的 JSONL，这类字段是"生产端口径"，别当行为差异。

两条都满足后再跑 ②，此时的 diff 才有仲裁力（否则结论要么恒绿、要么是他人缺陷的投影）。

`run_m3_gate.ps1` 依次做四件事，最后落 `sessions/M3-gate-<side>-<时间>.json`（含各步骤退出码与失败项）：

0. **入口实现识别护栏**（**快速失败**）：读 7000/7100/7200 的实际监听进程，口径复用
   `tools/m2-acceptance/portclassify.ps1`（与 M2 验收① 同一分类器）。`-Side rust` 时必须三个端口
   都是**已知 Rust 入口**且挂在本端口期望入口上（`login-gate`/`sel-gate`/`game-gate`）——
   **不再只判「不是 C#」**，否则「未知进程」或「已知 Rust 入口接错线」都能蒙过去；
   `-Side csharp` 时必须是 `E:\MirServer` 的 C# 实现。不过即 RED 退出。
   失败项：`entry_ports_not_listening` / `rust_side_entry_not_known_rust` /
   `csharp_side_but_entry_is_not_csharp` / `mixed_implementations_on_entries`。
1. `run_pair.ps1`：快照 before/after（13 表 sha256）→ 操作序列 → 作用域抽取 → 基线 json。
2. RNG 基线：`csharp` 侧跑 `rng-baseline.ps1`（种子注入 → 世界 RNG 流 + 掉落/经验序列落盘 + hash）；
   `rust` 侧重放 C# 记录流（`cargo test -p mir2-parity-tests --test rng_replay_parity`）。
3. 给了 `-PeerSession` 时做跨侧比对。**先机判两条前置**：
   - `Get-M3SessionDirFault`：本侧会话目录非空（run_pair 中途失败 ⇒ `$sessionDir=null`，
     旧实现会抛异常、连 RED 报告都不落 ⇒ fail-open），否则 `session_dir_missing`；
   - `Get-M3PeerScenarioGuard`：peer 的 `baseline.json.fixed_new_chr` 与本侧一致，否则
     `peer_scenario_mismatch`（两侧不是同一输入序列，diff 无仲裁力）；peer 的 `baseline.json`
     被写坏/半写（截断、非法 JSON）时 `peer_baseline_invalid`——**结构化判红并落报告**，
     不得抛异常中断（P-1：旧写法 `Get-Content … | ConvertFrom-Json` 在 `$ErrorActionPreference='Stop'`
     下中断于落 RED 报告前，属 fail-open）。
   两条都过才跑 `scoped_diff.py diff`，**必须 0**。

### 护栏自检（离线；卡 `openmir2-m3-gate-script-hardening`）

三条机判护栏（①入口 / ②sessionDir / ③peer 场景）的唯一口径在 `gate_guards.ps1`，
`run_m3_gate.ps1` 与 `selftest_gate.ps1` 共用同一批函数（照 `portclassify.ps1` 先例），
所以「改坏必红 / 还原必绿」作用在**真正跑的生产代码**上：

```powershell
powershell -ExecutionPolicy Bypass -File mir2-rs/tests/parity/m3/selftest_gate.ps1   # 16/16 GREEN
```

自检覆盖：已知 Rust 入口=绿 / C#=红 / **未知进程=红（旧护栏放行）** /
**已知 Rust 接错端口=红（旧护栏放行）** / 端口没听=红 / sessionDir=null ⇒ `session_dir_missing` /
peer 场景不一致 ⇒ `peer_scenario_mismatch` / 缺 `baseline.json` / 缺 `fixed_new_chr` /
**peer `baseline.json` 坏 JSON ⇒ `peer_baseline_invalid`（不抛异常中断，P-1）**。
全程离线：不起栈、不占端口、不碰 DB。

首次 GREEN 报告（2026-10-10，`-Side csharp -VerifyRepeat`）：

```
GREEN: M3 门禁通过
  stable=True            prefix_sha256 == run2（两次运行取数 35139 / 36974 条，格式前缀 hash 相同）
  common_prefix_calls=1842   prefix_calls=1500（hash 只覆盖保证稳定的前 1500 次，留余量）
  drop_100kills_sha256=e40d78c0…  exp_table_sha256=5c1c45ce…（与 D 线入库 golden 逐位相同）
  pack_unchanged=True（E:\MirServer 5 个关键文件 sha256 前后一致）
```

### 规范化口径（2026-10-11，owner 裁决 A；本判据的组成部分）

`row_sha256` 覆盖 `SELECT *` 全列，其中一部分列**每次运行必变**（自增 PK/FK、墙钟
`datetime`/unix 秒），另有一列是**夹具自身随机**（`src/BotSrv/Scenes/Scene/SelectChrScene.cs:174`
`sJob = Random(2)`，`RandomNumber` 无种子）。不排除这些列，**两次都 GREEN 的 C# 会话互 diff
也判 RED**（实测 8 行），判据无仲裁力。因此正式判据改为 `diff --canonical`：

- 只对 `scoped_diff.py` 里**逐列登记（含每列理由）**的易变列置 `<VOLATILE>`，其余列逐字节比；
- **不做整表 ignore**；遇到未登记的表 fail-closed（exit 4），不许悄悄不比；
- `characters.Hair/Sex/Job` 是夹具建角输入——默认（不固定）时 `Random(2)` 使 `Job` 在 0/1 间随机、
  `Hair/Sex` 因 `Random(1)` 恒 0 而固定为 2/0，故在**默认场景**里这三列置占位；
  **固定建角场景**用 `--fixed-newchr` 让它们**不**置占位、逐字节比（见下节，卡
  `openmir2-m3-botsrv-fixed-newchr`）；
- 红检：`python scoped_diff.py selftest` **14/14**，含「仅易变列不同必须判绿」「被比对列被改坏必须判红」
  「固定建角场景下 Job 被改坏必须判红（不被占位吃掉）」「重复行 `A=[x,x]` vs `B=[x]` 必须判红」
  「未登记表 fail-closed」「`characters.MapName/CX/CY` 逐字节比（仅 CX 不同必红）」。
- **多重集/计数式比对（2026-10-11，卡 `openmir2-m3-scopeddiff-multiset`）**：`diff` 计 `problems`
  与 `delta` 计增删时，按 **(表, 行 key) 的出现次数** 做多重集差（`collections.Counter`），
  **不用 set 差集**。旧实现下 `A=[x,x]` vs `B=[x]` 的 `仅A=0 仅B=0` 会 **exit 0（假绿）**；
  现在「重复行数」本身是判据的一部分（多出一行计 1，判红）。同表两侧重复行数相同仍判绿。

### 作用域覆盖角色族表（物品/Abil/Status；卡 `openmir2-m3-bag-equip-alignment`，2026-10-11）

旧 `extract` 的归属规则是「行内出现 prefix」——它只能命中 `account*` / `characters` / `characters_indexes`
（行里有账号名/角色名）。**物品行、Abil 行、Status 行、Bonus 行的键是 `PlayerId`（bigint 外键），
行里没有名字** ⇒ 它们**从来没进过作用域**：背包/装备对齐就算真的跑起来，也会在这一层被静默漏比。

现在改成两遍：先从 `mir2_db.characters` 里解出本次运行建出的 `Id` 集合，再以 `PlayerId ∈ 该集合`
归属族表（`scoped_diff.py` 的 `FAMILY_TABLES`）。覆盖的表：

```
mir2_db.characters_item          # 身上装备行（Position/MakeIndex/StdIndex/Dura/DuraMax 逐字节比）
mir2_db.characters_bagitem       # 背包行
mir2_db.characters_storageitem   # 仓库行
mir2_db.characters_item_attr     # 装备附加属性（MakeIndex + Value0..13 逐字节比）
mir2_db.characters_ablity        # 角色 Abil（Level/Ac/Mac/Dc/Mc/Sc/Hp/Mp/Exp/Weight…）
mir2_db.characters_bonusability  # Bonus 模板
mir2_db.characters_status        # Status0..15
mir2_db.characters_magic         # 技能行（MagicId/Level/UseKey/CurrTrain 逐字节比）
mir2_db.characters_quest         # 任务标志（QUESTOPENINDEX/QUESTFININDEX/QUEST 逐字节比）
```

上面 9 张 + `account` / `account_protection` / `characters` / `characters_indexes` 正好是
`tools/dbsnap/dbsnap.py` 的 `DEFAULT_TABLES` **13 张真实快照表**；`selftest` 的 14/14 直接读仓内
真实 `sessions/csharp-20261011-142347/snap_after.manifest.json` 跑一次 `extract` —— 只要有一张表
没登记就 exit 4（F-1 的牙齿）。

口径纪律（不变）：易变列只有「自增 PK `Id` / 外键 `PlayerId`（`bonusability`/`quest` 是 `PLAYERID`）」
与墙钟 `ModifyTime`，**逐列登记理由**；`Position`/`MakeIndex`/`StdIndex`/`Dura`/`DuraMax`/Abil 各字段
一律**逐字节比**——其中 `MakeIndex` **故意不登记为易变**（它是否跨侧确定属被比对行为，
要白名单必须另给实测理由）。未登记表仍 fail-closed（exit 4）。

真实 13 表快照上的重抽证据（F-1/O-1）：`sessions/reextract-20261011-1828/`——C#/Rust 两前缀
各 115 行（族表 111 行），族表子集跨侧 `--canonical --fixed-newchr` **GREEN（5 表）**；
全量只剩 `characters` 的 `CX/CY` 一行差异（活库快照/落库时机，见该目录 README「已知观察」）。

单侧基线不必带 `--canonical`（`extract`/`delta` 不变）；只在**跨侧比对**时用。

### 固定建角场景（卡 `openmir2-m3-botsrv-fixed-newchr`；T-6）

默认场景下 `Job` 是夹具自身随机（`RandomNumber` 无种子）⇒ 两侧不是同一输入序列，拿它做 diff
只能得到噪声，所以 `--canonical` 对这 3 列置占位——但**覆盖会丢**。本场景把它补回来：

1. **BotSrv 配置固定建角参数**（`AppSetting.json` 的 `BotPlay` 段；默认关，行为与改动前一致）：

   ```json
   { "BotPlay": { "FixedNewChr": true, "NewChrJob": 1, "NewChrHair": 2, "NewChrSex": 0 } }
   ```

   配置由 `RobotOptions.FixedNewChr`/`NewChrJob`/`NewChrHair`/`NewChrSex` 承载，
   `SelectChrScene.SelectChrCreateNewChr` 在固定模式下直接发这些值（原随机路径逐字节未改）。

2. **夹具命令**（`run_pair.ps1`/`run_m3_gate.ps1` 直接透传 `-FixedNewChr`）：

   ```powershell
   # 同一命令连跑两次（各自新前缀）⇒ characters 的 Job/Hair/Sex 两次相同
   powershell -ExecutionPolicy Bypass -File mir2-rs/tests/parity/m3/run_pair.ps1 -Side csharp -FixedNewChr -NewChrJob 1 -NewChrHair 2 -NewChrSex 0
   # 跨侧判据：--canonical 再叠 --fixed-newchr ⇒ Hair/Sex/Job 不置占位、逐字节比
   python mir2-rs/tests/parity/m3/scoped_diff.py diff <a>/scoped_after_norm.jsonl <b>/scoped_after_norm.jsonl --canonical --fixed-newchr
   ```

   `--fixed-newchr` 只影响 `mir2_db.characters` 的 `Hair/Sex/Job` 三列（从易变白名单里摘掉）；
   自增 PK / 墙钟列照旧置占位。未登记列仍 fail-closed。

### 背包/装备「两侧同一序列」（卡 `openmir2-m3-bag-equip-alignment`；T-14 + T-22）

卡内 DoD ②：拾取 → 装备 → 卸下 → 丢弃 → 使用药品，**两侧同一序列、跨侧作用域字段级 diff = 0**。
序列定义 = `bag_equip_sequence.json`（每步的 `CM_*` 编码、oracle 派发锚点、出包观测面）。

驱动 = BotSrv 的固定背包/装备序列（`RobotOptions.BagEquipSequence`，默认 **false = 原行为**；
白名单 T-22）：本假人收到自己的 `SM_LOGON` 后，按固定顺序各发一条 `CM_PICKUP/CM_TAKEONITEM/`
`CM_TAKEOFFITEM/CM_DROPITEM/CM_EAT`，参数只从**本假人的客户端视图**取（两侧服务端行为相同时
字节逐条相同）。每步落 `M3BAGSEQ step=N …`、收尾 `M3BAGSEQ done`，`run_pair.ps1` 写进
`baseline.json.bag_equip_markers`——**缺整串即 RED**（空序列恒绿是禁止的）。

一条命令（一条命令 = 一侧；两侧栈需要人工换栈，除 `-DiffOnly`/`-SelfCheckNoStack` 外会连真栈）：

```powershell
# ① C# 基线（C# 栈在听）
powershell -ExecutionPolicy Bypass -File mir2-rs/tests/parity/m3/run_bag_equip_pair.ps1 -Side csharp -Bots 1
# ② 换 Rust 栈后跑 Rust 侧 + 跨侧 diff（= 本卡正式判据）
powershell -ExecutionPolicy Bypass -File mir2-rs/tests/parity/m3/run_bag_equip_pair.ps1 -Side rust -PeerSession <上面打的 SESSION_CSHARP 名>
# ③ 只重比既有产物（不碰端口）
powershell -ExecutionPolicy Bypass -File mir2-rs/tests/parity/m3/run_bag_equip_pair.ps1 -DiffOnly -CsharpSession <c#会话> -RustSession <rust会话>
# ④ 无栈自检（所有路径必须 fail-closed；不连真栈）
powershell -ExecutionPolicy Bypass -File mir2-rs/tests/parity/m3/run_bag_equip_pair.ps1 -SelfCheckNoStack
```

逐步判红口径（详见脚本头部注释）：
0）入口护栏：`-Side rust` 而入口监听进程仍在 `E:\MirServer\*` ⇒ RED（拿 C# 比 C# 恒绿）；
1）预检：入口端口不全在听 / MySQL 3306 不可达 / 前缀不清新 ⇒ **PREREQ（exit 2）**，不静默通过；
2）同序列：`bot.log` 必须出现整串 `M3BAGSEQ`（含 `done`）⇒ 否则 RED；
3）跨侧：`scoped_diff.py diff --canonical --fixed-newchr` ⇒ 非 0 即 RED。

窗口纪律：起栈前看 `E:\MirServer\port-status.ps1`，与 M2 真机验收**严格串行**；
本脚本不启动/不停止任何服务。

### T-14 复跑口径（关挂机 + 同口径 peer；owner 2026-10-12 06:36）

背包/装备序列要**两侧同一组开关**才可比。`run_pair.ps1` 的 `-BagEquipSequence` 分支会**自动**附带
`-DisableAutoPlay`（并写进 `baseline.json.bag_equip_sequence`，形如
`on step_ms=1000 start_delay_ms=3000 disable_autoplay=True`）：

- **为什么要关挂机**：挂机 = 客户端无种子 RNG 随机走位（`ClientManager.RunAutoPlay`）。
  `scoped_diff.py` 的 `characters.MapName/CX/CY` 是**逐字节比**列（不在易变白名单；selftest 14/14 有一条
  「仅 CX 不同必红」钉住）。开着挂机 ⇒ 两侧各走各的，`CX/CY` **必然**出 diff——**这不是实现 bug**；
  关掉后坐标才只由服务端行为决定。
- **peer 口径护栏**：跨侧 diff 前 `gate_guards.ps1::Get-M3PeerBagEquipScopeGuard` 机判两侧
  `bag_equip_sequence` 同开关（`run_m3_gate.ps1 -PeerSession` 与 `run_bag_equip_pair.ps1`
  共用）。入库的 `csharp-20261012-054849` 是**开着挂机**跑的旧产物（只有
  `on step_ms=1000 start_delay_ms=3000`、无 `disable_autoplay`）⇒ 被判 `peer_bag_equip_autoplay_on`
  （PREREQ exit 2），**不能当同口径 peer** ⇒ C# 基线必须按新开关重跑一次。
- **默认关两向自检**：`run_pair.ps1 -EmitOpsArgs`（无副作用）证明「带 `-BagEquipSequence` ⇒ 传
  `-DisableAutoPlay`」「不带 ⇒ 不传」；`run_bag_equip_pair.ps1 -SelfCheckNoStack` 还断言 run_bots
  配置透传有/无两向、旧挂机 peer 必红、以及 `scoped_diff.py selftest` 14/14。
- **preflight（T-27，owner 2026-10-12 06:40）**：跑 `-Side rust` 前先确认 Rust game-svr 是否已实现
  断线/进世界存档（承接卡 `openmir2-m3-gamesvr-save`，issue `f6893ea2`）：`bins/game-svr` 目前 **0 处**
  请求常量 `DB_SAVEHUMANRCD(101)`（`dbclient.rs` 只有 load 构造器）⇒ 断线后 after 快照恒为建角状态、
  跨侧 diff **必非 0**（与 T-26 三条正交）。`run_bag_equip_pair.ps1 -Side rust` 已内置该机判
  （未实现 ⇒ PREREQ exit 2，`-SkipSavePreflight` 仅调试用）。
- **RNG 抽数账本仍是前置**：`openmir2-m3-enter-eat-wiring` 的 `homemap.rs` 少抽 3 次（owner 裁决
  `784be562`）未落地前，出生点 `Random(3)+Home-2` 两侧仍可能不同——那属 RNG 接线，与挂机无关。
- **固定起点 + 固定初始背包变体（T-14；owner 86478c55/d7798370/643459a7）**：只关挂机**不够**——
  `HomeMap` 空时 oracle 走出生点分支（`WorldServer.cs:249` + `Random(3)+Home-2`，`RandomNumber.cs:20`
  无种子）⇒ 两侧起点天然不同。**不许**把 `CX/CY` 挪进易变白名单（14/14 自检钉住）；改用**两相夹具**
  把角色初始态整体交给 fixture：① 预热建角（`run_bots -RequireCharCreated`，只建号）→ ② SQL pin
  角色的 `MapName/HomeMap/HomeX/Y/CX/CY/Level/Dir` **+ `characters_bagitem` 初始 4 件**（蜡烛/布衣/木剑/
  金创药(小量)）+ `characters_item` 清空 + `characters_ablity`（Level=1）→ ③ pin 后再取 BEFORE 快照 →
  ④ 计分跑走「选角」（不带 `-FixedNewChr/-NewAccount`，仍带 `-BagEquipSequence -DisableAutoPlay`）。
  `run_pair.ps1 -PinStartPose`（与 `-FixedNewChr -BagEquipSequence` 同用）已实现两相；两侧
  `baseline.json.start_pose` 必须逐字节相同（`gate_guards.ps1::Get-M3PeerStartPoseGuard`；标签含
  `bag=48@0,97@1,207@2,3@3 abil=1`）。pin 默认值 `MapName=0, HomeMap=0, HomeX/Y=289/618, CX/CY=289/618,
  Level=1, Dir=0`（`HomeMap` 非空 ⇒ oracle 跳过新角色分支，也就不会发新手 4 件、不走无种子出生点）。
  **分工**：真·新角色路径（空 `HomeMap`）由 `openmir2-m3-enter-eat-wiring` 的真机证据覆盖（进图 `(287,616)`、
  `bag_view=4`、4 件齐）；T-14 变体覆盖「序列对拍」。
  ✅ **已在真机验证（第十四格，2026-10-12）**：pin **确实被进图路径采用**（C# `CM_PICKUP x=289 y=618`、
  `make=90000002`，与 Rust 逐条相同）。实跑同时修出两处夹具竞态（已登记 T-22 第十四格）：
  - `run_bots.ps1 -RequireCharCreated` 的收工条件加 `login_ok`（`load_stats` 每 5s 采样 vs 建角标记即时落日志 ⇒ 旧条件恒假红）；
  - `run_pair.ps1 -PinStartPose` 的 pin 加 **settle + hold 循环**（预热断线存档异步，实测晚于 pin 落库并把 pin 覆盖；
    pin → 等 3s → 复验 → 被覆盖则重 pin，最多 5 轮）。
    ⚙ **2026-10-12 08:20 补强（owner 要求的可判红自检）**：settle+hold 判据抽到
    `gate_guards.ps1::Get-M3PinHoldVerdict` / `Invoke-M3PinSettleHold`（唯一口径，`run_pair.ps1` 真实 pin 循环与
    自检共用），并把 hold 条件从「某轮三项到位」收紧为「**至少一轮真实 settle + 连续两轮** settle 复验都到位」——
    单次观测挡不住「pin 早于异步落库、存档随后静默覆盖」的假 GREEN。`run_bag_equip_pair.ps1 -SelfCheckNoStack`
    新增 **⑩**（故障注入驱动同一函数）：跳过 settle、pin 早于落库、存档反复覆盖均 ⇒ 必判红；正常路径 ⇒ attempt=2；
    并机判 `run_pair -EmitOpsArgs` 的 `pin_instrument`（`settle_seconds>=1` 且 `consecutive>=2`）证明生产接线未被削弱。
    原始输出与三条变异红检：`evidence-t14-pinhold-20261012/**`（`SHA256SUMS.txt` 8/8 GREEN）。
    注：判据轮跑在补强前，故报告里是 `PIN_HOLD attempt=1`；T-28 修好后的重跑会因连续两轮要求显示 `attempt>=2`（预期）。
  ⚠ **当前唯一剩余跨侧差异 = `characters.IncHealth`**（C#=0 / Rust=20），根因 = Rust 未建模 oracle 周期
  `HealthSpellChanged` 恢复 tick（把吃药得到的 `IncHealth` 消费回 HP，满血时归零）；登记 **T-28**，待 owner 裁决。
  报告 `T14-对照报告-20261012.md`；判据会话 `sessions/{csharp-20261012-080312,rust-20261012-080417}`。

## 一次「侧运行」是什么


```
预检（入口在听 / DB 可读 / 账号名长度合规）
  → 运行态记录（7 进程实际 exe 路径 + 启动时间 → baseline.json）
  → 全新前缀断言（该命名空间必须为空）
  → 快照 BEFORE（tools/dbsnap，13 张表逐表 sha256）
  → 操作序列（tools/botload 假人：注册 → 登录 → 建角 →（尽量）进世界）
  → 快照 AFTER
  → 作用域抽取（scoped_diff extract：只取本次前缀命名的行）
  → 改动清单（scoped_diff delta）
  → baseline.json（两侧快照 hash、作用域行数、改动清单、命令、git 提交）
```

单跑夹具（排查用；门禁请用 `run_m3_gate.ps1`）：

```powershell
powershell -ExecutionPolicy Bypass -File mir2-rs/tests/parity/m3/run_pair.ps1 -Side csharp -Bots 1
python mir2-rs/tests/parity/m3/scoped_diff.py diff <csharp会话>/scoped_after_norm.jsonl <rust会话>/scoped_after_norm.jsonl --canonical
```

## RNG 基线：一条命令（种子注入 → 序列落盘 + hash 稳定）

```powershell
powershell -ExecutionPolicy Bypass -File mir2-rs/tests/parity/m3/rng-baseline.ps1 -VerifyRepeat
```

- 影子副本建在**会话目录内**，改的是副本的 `runtimeconfig`/`Server.conf`；运行前后对比
  `E:\MirServer\M2GameSvr` 5 个关键文件 sha256（`pack_unchanged`），证明冻结基线未被本线改动。
- `-VerifyRepeat` 跑两次同种子运行并断言**确定性前缀 hash 相同**（当前 `b677fe32…`）。
- **hash 语义（重要）**：只对前 `PrefixCalls`（默认 1500）次取数取 hash——实测跨运行确定性区段
  ≈1828 次（见 `RNG同种子与记录回放方案.md` 的边界实测），留了余量。整条流的 hash 不可作判据
  （公共前缀之后受 NPC 遍历顺序/时钟门控影响）。
- 同一命令产出掉落/经验序列（`drop_100kills.txt` / `exp_table.txt`）及其 sha256。

已产出的基线（实测，2026-10-10）：`sessions/csharp-20261010-140632/baseline.json`
—— 13 表快照 hash 齐备，`runtime_procs`（产出该基线的进程表）齐备，
作用域内 4 行变更：`mir2_account.account +1`、`mir2_account.account_protection +1`、
`mir2_db.characters +1`、`mir2_db.characters_indexes +1`。
**同一命令连跑两次，改动清单的形状完全一致**（同 4 表各 +1，仅行内容里的自增 Id/时间戳不同）
——这是"夹具本身可复现"的证据。

产物与临时物的位置/去向登记见 `ARTIFACTS.md`（§14.3 收尾清单要求）。

## 为什么"同一初始状态"不靠破坏性还原

`mir2_db` 是**活库**（既有数千角色，且并行线也在跑假人）。做法是**每次运行用全新前缀**
（账号/角色名 = `<前缀><序号>`），并断言该前缀此前 0 行；于是：
- 不需要还原快照（不碰既有角色，风险为 0）；
- 比对只取本次命名空间内的行 ⇒ 并行线的干扰自动排除。

`scoped_diff.py` 提供 `extract / delta / diff / selftest`（selftest 14/14，含"改坏必红"与"重复行多重集"）。
`diff`/`delta` 的差异计数是**多重集/计数式**（同 `(表, row_sha256)` 的出现次数纳入判定）。
`--normalize` 把前缀归一化为 `<RUN>`，使两次运行的前缀差异不影响比对。

## 账号名 10 字符上限（踩过，务必先读）

协议层账号字段上限 **10 字符**，超长会被**静默截断**，表现为：
「帐号创建成功」→ 立刻登录「此帐号不存在」（截断后的名字与创建时不一致）。
夹具已内置护栏：`前缀长度 + 序号位数 > 10` 直接判红并退出。
现状默认前缀 `m3` + `HHMMSS`（8 字符），留 2 位给序号（≤ 99 只假人）。

## LoginSrv 的内存账号表（SQL 开号的坑）

`AccountStorage.Index()` 查的是**启动时加载的内存账号表**。因此：
- **推荐**：让假人走 `-NewAccount`（服务器自建号）——账号表在线更新，
  `account_protection` 行也由服务端一并写入，实测可直接登录；
- `-ProvisionSql`（SQL 直开号，`tools/client/account_provision.ps1`）**必须重启 LoginSrv**
  后才可登录；只适合"开号 → 重启 → 跑操作序列"的编排。

## 运行态 exe 归属（重启/改配置前必读）

`baseline.json` 的 `runtime_procs` 会记录产出基线的进程表（实测，2026-10-10）：

| 进程 | 实际 exe |
| --- | --- |
| mysqld | `D:/mysql/mariadb-10.11.19-winx64/bin/mysqld.exe` |
| DBSrv | `src/DBSrv/bin/Release/DBSrv.exe`（**仓库构建输出**） |
| LoginSrv | `src/LoginSrv/bin/Release/LoginSrv.exe`（**仓库构建输出**） |
| GameSvr | `E:/MirServer/M2GameSvr/GameSrv.exe` |
| LoginGate / SelGate / GameGate | `E:/MirServer/{LoginGate,SelGate,RunGate}/*.exe` |

⇒ 需要"整栈重启"时**以这张表为准**（重启错那份等于没重启）；
LoginSrv 读的配置是 `src/LoginSrv/bin/Release/logsrv.conf`
（`LogSrv.ini` / `config.conf` 是遗留副本，不读）。

## 已知阻塞（影响操作序列能覆盖到哪一步）

1. **BotSrv 进世界后崩溃**：`TMap.CanMove`（`src/BotSrv/Maps/TMap.cs:343`）抛异常并带走整个进程，
   崩溃点与时序相关（有时停在"确认游戏公告"）。⇒ 今日可用的操作序列 = **注册 → 登录 → 建角(→ 尽量进世界)**，
   判据取可靠段（`帐号创建成功`/`帐号登录成功` = N），`进世界` 只记录不判红。
   M3 要跑「走路/打怪/掉落/小退」必须**先修该崩溃**（工具侧，C 线领域）。
2. 世界侧的**非 RNG 熵源**已实测会扰动逐次一致（见 `RNG同种子与记录回放方案.md`）：
   同种子两次运行在第 1825 次取数处分岔（调用点从 `Merchant.Run` 变成 `GuildOfficial.Run`）。

## 红检（改坏必红）

```powershell
powershell -File run_pair.ps1 -SelfTestRed      # 入口不通 → 判失败（实测 exit 0 = 红检成立）
python scoped_diff.py selftest                 # 14/14 实测（含重复行多重集：A=[x,x] vs B=[x] 必红；族表 PlayerId 归属；真实 13 表快照 extract；MapName/CX/CY 逐字节比）
```

另外两条内置护栏本身就是门禁（实测触发过）：
账号名超 10 字符 → 红；前缀已存在同名账号/角色 → 红。

## 清理本次运行留下的账号/角色（不自动执行）

```sql
-- 先看命中范围
SELECT Id, Account FROM mir2_account.account WHERE Account LIKE 'm3%';
SELECT Id, ChrName FROM mir2_db.characters WHERE ChrName LIKE 'm3%';
-- 确认后再删（夹具不自动删：误删既有角色不可逆）
DELETE p FROM mir2_account.account_protection p JOIN mir2_account.account a ON p.AccountId=a.Id
  WHERE a.Account LIKE 'm3%';
DELETE FROM mir2_account.account WHERE Account LIKE 'm3%';
```

## 装备/能力接缝（`bins/game-svr/src/ability.rs`，**M2 车道接线点**）

M3 第一片（`feature.rs`）+ 本片（`ability.rs`）把"**装备物品集 ⇒ `50/41/10/213` 帧的 feature、
`52 SM_ABILITY` 的 `WAbil`、`752 SM_SUBABILITY` 的 7 字段**"做成无 I/O 的纯函数接缝。
`openmir2-m2-enter-world-frames` 的入图帧序里有 `52/752`，接线直接用这两个模块即可，**不要复制第二份实现**。

| 接缝 | 位置 | 输入 → 输出 |
| --- | --- | --- |
| 外形 feature | `feature.rs`（已合入 `1df92efe`） | `StdItem.Shape`(Dress/Weapon) + Gender + Hair → `i32` |
| 模板查表 | `ability::ItemCatalog` / `StdItemCatalog` | 物品 `Index` → `&StdItem`（`StdItems` 模板） |
| 装备集取值 | `ability::equipped_feature` | `UseItems[13]` + 目录 → feature `i32` |
| 派生能力 | `ability::recalc_abilitys` | `Abil`+`BonusAbil`+装备+背包 → `WAbil`(22 字段) + `SubAbility`(7) |

设备/状态的**源**：`Abil`/`BonusAbil` 来自 `record.rs`（角色记录，已全字段解析）；
`UseItems`/背包 Index 来自角色记录 `hum_items` / `bag_items`。

**未做（登记在卡，不阻塞接线）**：有效物品的 Desc 升级差量（`GetUpgradeStdItem`）、
套装/特殊 Shape 的巨型 else-if 块、buff、`MagicList` 对 HitPoint 的加成、`ApplyItemParametersByJob` 龙装分支。
⇒ 本片直接收"**已升级的有效 `StdItem`**"；普通（非套装/非龙装/无升级）物品与 oracle 逐字段相同。

### 复现 oracle 向量（真身探针，改坏必红）

```powershell
# 在**仓库根**运行（源锚点用相对路径）
dotnet build mir2-rs/tests/parity/m3/ability-probe -c Release
dotnet mir2-rs/tests/parity/m3/ability-probe/bin/Release/net8.0/ability-probe.dll `
  mir2-rs/tests/parity/m3/fixtures/ability_vectors.json
cargo test -p game-svr --lib        # 回读夹具逐字段核对
```

向量覆盖：裸装（复现 M0 金标准 52/752）、`BonusAbil` 非 0、`ApplyItemParameters` 的每个
`StdMode` 分支（5/6、10/11、15、19、20、21、22、23、24、26、52/54、53、default）、`Dura==0` 只称重、
满装多槽、`Index==0`/缺模板忽略、Dress/Weapon 的 feature。

## 装备/卸下 handler 核心（`bins/game-svr/src/bag.rs`，卡内第二片）

`bag.rs` 把 `CM_TAKEONITEM`(1003) / `CM_TAKEOFFITEM`(1004) 的**决策 + 状态迁移**做成无 I/O 的纯函数接缝
（`take_on`/`take_off` + `CheckUserItems`/`CheckTakeOnItems`/三条不可卸守卫 + `GetFeatureToLong` 回帧）：
输入 = `ItemList`（背包）+ `UseItems[13]`（身上）+ 守卫上下文；输出 = 错误码（`n18`/`n10`）、
`SM_TAKEON_OK/FAIL`(615/616) / `SM_TAKEOFF_OK/FAIL`(619/620) 的**帧规格**、换下物品（接线层据此发 `SM_ADDITEM`）。

oracle 链与**必须原样复刻**的口径（`PlayObject.Operate.cs:377`/`:499`）：

| 口径 | 说明 |
| --- | --- |
| `InDisableTakeOffList` 分支 | oracle `goto FailExit` 前**不设**错误码 ⇒ `SM_TAKEON_FAIL` 的 `Recog = 0`（不是 `-4`） |
| take-off **成功** | oracle 成功分支**没有** `n10 = 1` ⇒ 先发 `SM_TAKEOFF_OK` 再落回 `FailExit` 发一条 `SM_TAKEOFF_FAIL(Recog=0)`（既有形态，**照 bug 搬**） |
| `NeedLevel` 是 `byte` | `HUtil32.HiByte(byte)` 经 int 隐式转换后恒 0 ⇒ `Need` 10-13/40-44/81/82 的 Hi 比较恒真 |
| `MaxBagItem = 46` | `AddItemToBag` 的满包判定是 `Count >= 46`（take-on 忽略返回值；take-off 走 `n10 = -3` 且不清槽） |
| take-on 成功 | 若 `StdMode ∈ {15,19,20,21,22,23,24,26}` 则 `userItem.Desc[8] = 0` |

### 复现 oracle 向量（真身探针，改坏必红）

```powershell
# 在**仓库根**运行（源锚点用相对路径）
dotnet build mir2-rs/tests/parity/m3/bag-probe -c Release
dotnet mir2-rs/tests/parity/m3/bag-probe/bin/Release/net8.0/bag-probe.dll `
  mir2-rs/tests/parity/m3/fixtures/bag_vectors.json
cargo test -p game-svr --lib        # 回读夹具逐字段核对
```

向量覆盖：装备/换装成功、按槽位/名称/等级/腕力的失败、已占槽三条不可卸守卫、`InDisableTakeOffList`、
性别不符、卸下成功、空槽/`MakeIndex` 不符/名称不符/背包满(46)/交易中、`btWhere >= 13`。

**未做（登记在卡，见 `tests/parity/whitelist.md` 的 T-13/T-16）**：`CM_PICKUP`/`CM_DROPITEM`/`CM_EAT`
已分别在 `ground.rs`/`useitem.rs` 落地（见下两节）；接线（`main.rs`/`smframe.rs` 归 m2-svr）、双侧同序列驱动未做。

## 使用药品 handler（`bins/game-svr/src/useitem.rs`，卡内第二片·第三格）

`useitem.rs` 把 `CM_EAT`(1006) 的**决策 + 状态迁移 + 回帧**做成无 I/O 的纯函数接缝 `use_item`：
输入 = `ItemList`（背包）+ 守卫（`BoCanUseItem`/`Death`）+ `ActorId` + 目录；输出 = 是否吃掉、
被吃下标、`WAbil`/`IncHealth`/`IncSpell`/`UserUnLockDurg` 的新值、**即时帧**与**延时通知**。
oracle 链 = `PlayObject.Message.cs:826` → `ClientUseItems`（`PlayObject.Operate.cs:601`）→
`EatItems`（`PlayObject.Base.cs:2047`）。**本片做 `boNODRUG` 守卫 + StdMode 0/1/2**：StdMode 0 的
Shape 1（固定量）、Shape 2（解锁）、Shape 3（百分比）、default（进 `IncHealth`/`IncSpell` 池），
StdMode 1（食物：`HungerStatus += DuraMax/10`、夹 5000、跨档 `RefMyStatus()`），以及 StdMode 2
（`result=true`、无任何状态变更）；地图禁药（`Envir.Flag.boNODRUG`）时
`SysMsg(Red, Hint)` → `RM_SYSMESSAGE` 并拒绝（不吃、只落 `SM_EAT_FAIL`）。

`ClientUseItems` 产出**两条通道**，接缝分开返回（别混成一条流）：

| 通道 | 产生点 | 本片帧/通知 |
| --- | --- | --- |
| 即时帧 `immediate_frames` | `SendDefMessage` → `SendSocket` 直接写 gate buffer | `SM_EAT_OK=635` / `SM_EAT_FAIL=636`（恒 1 条） |
| 延时通知 `notices` | `SendMsg` 入 `MsgQueue`（`PriorityQueue`，优先级 = `(byte)wIdent`） | `RM_HEALTHSPELLCHANGED=10052`（`IncHealthSpell` 尾部）、`RM_WEIGHTCHANGED=10115`（成功尾部，同时 `WAbil.Weight = RecalcBagWeight()`）、`RM_MENU_OK=10309`（`BoCanUseItem==false`，`nParam1=ActorId`）、`RM_SYSMESSAGE=11004`（`boNODRUG`，`nParam1/2 = RedMsgFColor/BColor`）、`RM_CHARSTATUSCHANGED=10139`（StdMode 1 跨档 → `RefMyStatus` 里 `RecalcAbilitys` 尾部）、`RM_MYSTATUS=8102`（`RefMyStatus`） |

**必须原样复刻**的口径：

| 口径 | 说明 |
| --- | --- |
| 首个 `MakeIndex` 命中后**无条件 break** | `break` 在 `if (stdItem != null)` 块**之外** ⇒ 同 `MakeIndex` 重复项只动第一个 |
| 模板缺失 / StdMode 非 0/1/2/31 | 什么都不吃（`SM_EAT_FAIL`、无通知、背包不变）；`sItemName` 形参 oracle **未使用** |
| StdMode 31（`AniCount != 0`） | `UseStdModeFunItem`（`PlayObject.Base.cs:1072`）= **只看 `SystemShare.FunctionNPC != null`**：存在 ⇒ `GotoLable("@StdModeFunc"+AniCount)` 后 `result=true` ⇒ 吃 + `SM_EAT_OK` + `RM_WEIGHTCHANGED`；缺失 ⇒ 不吃 |
| StdMode 31（`AniCount == 0`，解包） | oracle 在 `ItemList.Count + 6 - 1 <= Grobal2.MaxBagItem`（46）时解包出 6 件（`UnbindList`）；**未实现**（容量不足时两侧一致地不吃，容量够时是已证差异，T-17） |
| `boNODRUG` 守卫 | 仅 StdMode 0/1/2/3 进 `EatItems`（4/31 走 `ReadBook`/`UseStdModeFunItem`，不受约束）；恒 `SysMsg(Red)` 一条 + 拒绝（未禁药时 3/4 与 31 的解包分支仍落 `SM_EAT_FAIL`，见 T-17） |
| StdMode 1（食物） | `nOldStatus = GetMyStatus()`（`min(HungerStatus/1000,4)`）→ `HungerStatus += DuraMax/10`（整除）→ `_MIN(5000, ·)` → 跨档才 `RefMyStatus()`（`RecalcAbilitys`+`RM_MYSTATUS`）；`HungerStatus` 是 `int`、**不**回绕 |
| `notices` 顺序 | oracle `MsgQueue` 是 `PriorityQueue<SendMessage,byte>`，优先级 = `(byte)wIdent` ⇒ 消息泵按**优先级升序**出队（跨档时 `RM_WEIGHTCHANGED`(131) 反而先于 `RM_CHARSTATUSCHANGED`(155)/`RM_MYSTATUS`(166)）；接缝据此排序 |
| Shape 3 百分比 | `HUtil32.Round` = `Math.Round(..., AwayFromZero)`（半数**远离零**）；`max_hp=101/ac=5`→5、`max_mp=55/mac=10`→6 |
| default Shape | `AC>0` 才 `IncHealth += AC`、`MAC>0` 才 `IncSpell += MAC`（`ushort` 回绕） |
| `IncHealthSpell` | `nHp<0 || nMp<0` 直接 return（**不**发通知）；否则 `HP+nHp >= MaxHP` 夹 `MaxHP`，否则 `ushort` 回绕 |

### 复现 oracle 向量（真身探针，改坏必红）

```powershell
# 在**仓库根**运行（源锚点用相对路径）
dotnet build mir2-rs/tests/parity/m3/eat-probe -c Release
dotnet mir2-rs/tests/parity/m3/eat-probe/bin/Release/net8.0/eat-probe.dll `
  mir2-rs/tests/parity/m3/fixtures/eat_vectors.json
cargo test -p game-svr --lib        # 回读夹具逐字段核对
```

向量覆盖：Shape 1 固定量/溢出夹取、Shape 3 百分比（含 .5 进位）、Shape 2 解锁、default 进池/双零守卫、
按移除后背包重算 `WAbil.Weight`、首个 `MakeIndex` 命中、`MakeIndex` 未找到、StdMode 非 0/2、
模板缺失、`BoCanUseItem==false`、`Death`、**StdMode 2（无效果但吃）**、**`boNODRUG` 禁药（不消耗）**、
**StdMode 1（不跨档 / 跨档 `RefMyStatus` / 夹档 / 禁药）**、**StdMode 31（`AniCount!=0` 有/无 `FunctionNPC`；`AniCount==0` 满包）**。
红检：StdMode 2 改回「不吃」⇒ `stdmode2_eats_without_effect` 必红；`boNODRUG` 去掉守卫⇒ `nodrug_guard_does_not_consume` 必红、颜色 f/b 互换⇒该用例与夹具回读必红；
StdMode 1 夹档改 `min(6000)` ⇒ `eat_ok_stdmode1_hunger_clamp` 必红（`5150 != 5000`）；去掉跨档 `RM_CHARSTATUSCHANGED` ⇒ `eat_ok_stdmode1_hunger_recalc` 通知序必红；
StdMode 31 把 `if tpl.ani_count != 0 && ctx.function_npc_present` 改成 `if tpl.ani_count != 0`（删 `FunctionNPC` 判据）⇒ `eat_fail_stdmode31_no_functionnpc` 与 `stdmode31_funitem_eats_only_when_function_npc_present` 必红；把 `!= 0` 改 `== 0` ⇒ `eat_ok_stdmode31_funitem` 必红。

**未做（登记在卡，见 `tests/parity/whitelist.md` 的 T-17）**：
StdMode 3/4 与 StdMode 31 的**解包**分支（`ExtraAbil` buff+`GetExp`/`ReadBook`/`UnbindList`）——**不是**"与 oracle 一致地不吃"，而是**已证差异**
（oracle 在这些 StdMode 上有成功路径：StdMode 3 的 Shape 13 无条件吃掉并发 `SM_EAT_OK`，本接缝同输入落 `SM_EAT_FAIL`），
只因 `use_item` 尚未接线而不可达；接线前必须补齐。
StdMode 1 已实现主体，但 `RefMyStatus()` 里的 `RecalcAbilitys()` 只用 `ability::recalc_abilitys` 的最小等价物
（`ctx.recalc`）复算 `WAbil`，`PlayObject.RecalcAbilitys` 的套装/`ExtraAbil`/`GetCharStatus` 尾段未建模 ⇒ 差异收窄为
**仅「有装备/buff 且跨档」**；无装备/buff 的跨档场景已逐字段相同（`eat_ok_stdmode1_hunger_recalc`）。
夹具 `eat_fail_stdmode_other` 只覆盖 `StdMode=5`（`case` 名单外）。
接线（`main.rs`/`smframe.rs` 归 m2-svr）、双侧同序列驱动仍未做。

## 拾取/丢弃 handler（`bins/game-svr/src/ground.rs`，卡内第二片·第四格）

`ground.rs` 把 `CM_PICKUP`(1001) / `CM_DROPITEM`(1000) 的**决策 + 状态迁移 + 回帧**做成无 I/O 的纯函数接缝
`pick_up`/`drop_item`，并自带**最小地面物品模型** `GroundMap`（一格多物；`GetItem`/`DeleteFromMap`/
`AddItemToMap`/`GetItemEx`/`CellMatch` 的等价物）。输入 = `ItemList`/`Gold`/`WAbil` + `GroundMap` + 守卫上下文；
输出 = **即时帧**（`SM_ADDITEM=200` / `SM_DROPITEM_SUCCESS=600` / `SM_DROPITEM_FAIL=601`）+ **延时通知**（按
`MsgQueue` 出队序）+ 迁移后的背包/金币/负重与地面格子。oracle 链 = `PlayObject.Message.cs:801/:811` →
`ClientDropItem`（`PlayObject.Operate.cs:251`）/ `ClientPickUpItem`（`PlayObject.Base.cs:55`）→
`BaseObject.DropItemDown/AddItemToBag/RecalcBagWeight` + `Envirnoment.GetItem/DeleteFromMap/AddItemToMap/GetItemEx`。

**必须原样复刻**的口径：

| 口径 | 说明 |
| --- | --- |
| **正常 `CM_DROPITEM` 恒失败** | 全仓无 `MapItem.ItemId = …` 赋值点（`MapItem.cs:41` 的赋值是注释）⇒ `AddItemToMap` 首行 `if (ItemId == 0) return false` 恒命中 ⇒ 普通丢弃恒 `SM_DROPITEM_FAIL`、背包不动（**照 bug 搬**，登记 T-19） |
| 通知出队序 | `MsgQueue` 是 `PriorityQueue`，优先级 = `(byte)wIdent`（低值先出）⇒ `RM_WEIGHTCHANGED=10115`(131) **先于** `RM_ITEMHIDE=8085`(149)，与入队序相反 |
| 金币拾取 | `IncGold` 超 `HumanMaxGold` ⇒ 放回地面、金币不变、无通知；成功只发自身 `RM_GOLDCHANGED=10136`（`SendRefMsg` 对自身不投递） |
| `DealLastTick` 守卫 | `(now - DealLastTick) > 3000` 才扫描背包；不满足则整段跳过 ⇒ FAIL |
| 拾取放回 | 背包满（`Count >= 46`）、超重（`IsAddWeightAvailable`）、模板缺失、金币溢出都走 `AddItemToMap` 放回 ⇒ 地面不动 |
| 空槽/无主 | `Dealing` ⇒ 直接 return；`!IsSelf && !IsOfGroup` ⇒ `RM_SYSMESSAGE=11004`（CanotPickUpItem）；超时 ⇒ `OfBaseObject = 0` |

### 复现 oracle 向量（真身探针，改坏必红）

```powershell
# 在**仓库根**运行（源锚点用相对路径）
dotnet build mir2-rs/tests/parity/m3/ground-probe -c Release
dotnet mir2-rs/tests/parity/m3/ground-probe/bin/Release/net8.0/ground-probe.dll `
  mir2-rs/tests/parity/m3/fixtures/ground_vectors.json
cargo test -p game-svr --lib        # 回读夹具逐字段核对
```

向量覆盖（6 模板 / 23 用例）：拾取成功（无主/自己/超时清主/同组）、`Dealing`/背包满/超重/模板缺失/
空格子失败、金币成功与超上限、丢弃 `ItemId=0` 恒失败、`ControlDropItem` 便宜直丢、安全区/`NoThrowItem`/
`IsCanDrop`/`DealLastTick` 守卫、名称带空格折分、`MakeIndex` 不命中、StdMode 40 耐久 u16 回绕
（`DropItemDown` 的 `ushort idura = (ushort)(Dura-2000)`：2500⇒500、10⇒63546）。夹具记录的 tick
（`now_tick`/`can_pick_up_tick`/`deal_last_tick`）都平移到固定基点 `1_200_000`（真实 tick 只驱动 oracle 判定）
⇒ 探针产物**逐字节可复现**。

红检（改坏 ⇒ 必红）：① `put_item` 的 `item_id == 0` 守卫改成 `== -1` ⇒ T-19 的 2 条用例红；
② `is_add_weight_available` 恒真 ⇒ 超重用例红；③ `dequeue_order` 改成降序 ⇒ 出队序用例红；
④ `DealLastTick` 守卫 `> 3000` 改成 `> 0` ⇒ 夹具用例红；
⑤ `drop_item` 的 StdMode 40 耐久 `wrapping_sub(2000)` 改成 `saturating_sub(2000)`（clamp 到 0）⇒
`stdmode40_drop_dura_wraps_like_oracle` 红（oracle 实测 underflow：Dura=10 ⇒ 63546，不是 0）。

**未做（登记在卡，见 `tests/parity/whitelist.md` 的 T-15/T-19/T-20）**：地面物品模型只在 `bins/game-svr`
侧的最小切片里（`crates/world` 尚无地图地面物品与地图 `Flag`）、`DropItemDown` 的 StdMode 45 随机外观、
`SendRefMsg` 广播、接线（`main.rs`/`smframe.rs` 归 m2-svr）、双侧同序列驱动。
