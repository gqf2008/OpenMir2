# M3 总验收汇总报告（十项系统盘点 + 300 假人 24h 长跑方案）

- **卡**：`openmir2-m3-final-acceptance`（issue `b0bb3003`）。本文件落地该卡 DoD ③（`tests/parity/M3-report.md`），
  并承载 DoD ① 的十项系统逐项盘点索引；DoD ②（300 假人 24h 长跑）本片**只写方案、不跑**（见 §2）。
- **车道**：principal `openmir2-worker-5` ／ worktree `.claude/worktrees/m3cs` ／ branch `worktree-m3-final-acceptance`。
- **base**：`origin/master da94fcb7`。本片**零 `.rs` / 零 `src/**` / 零表结构·脚本语法·数据格式改动**；
  写集 = `mir2-rs/tests/parity/**`（本报告 + 证据包）。
- **口径（照 §6.0-1/2/4 与 owner 03:20 指令）**：
  1. **只写本片亲手跑过的读数**；没有亲手跑过的，写 `not_verified`；
  2. **未推进的系统写 `not_covered` + 承接卡**，不许留白当绿、不许自己造绿；
  3. 差异必须为 0 或进 `tests/parity/whitelist.md`；产物 hash 按**仓库字节（LF 归一 = git blob）**口径；
  4. 本片**未起栈、未占端口**（九端口实拍见证据包 `window/`）。
- **证据包（本片实跑原始件）**：`mir2-rs/tests/parity/m3/evidence-m3-final-acceptance-inventory-20261013/`
  （`SHA256SUMS.txt`，仓库字节口径；复核命令见 §5）。

> **本报告不得被读成「M3 全部达成」**。① 十项里只有 3 项在本报告口径下算对齐（背包/装备、战斗、技能），
> NPC/商店 与 拍卖（⑩b，按 SellOff 口径）为 `partial`，邮件（⑩a）为 `oracle_absent`，其余 5 项为 `not_covered`
> （**计数口径**：`⑩ 邮件/拍卖` 拆成 10a + 10b **两个子行**、合计仍计作**一项** ⇒ 3 covered + 2 partial + 1 oracle_absent + 5 not_covered = **11 子行 = 十项**，⑩ 为**混合行**「10a absent + 10b partial」）；② 24h 长跑 **未跑**；④ M3 门禁 `scoped_diff` 当前有一处已知 RED
> （已由 follow-up 卡 `openmir2-m3-final-acceptance` 的 F-2 轨承接，见 §4）。

---

## 1. 十项系统逐项盘点（§6 M3 顺序）

| # | 系统 | §6 判据要点 | 本报告状态 | 承接/备注 |
| --- | --- | --- | --- | --- |
| 1 | 背包 / 装备 | 穿戴/脱下/负重/持久/属性面板逐项相同；边界用例齐 | **covered（diff=0）** | 卡 `openmir2-m3-bag-equip-alignment`（closed，merge `bbf63fb0`） |
| 2 | 战斗 | 固定种子下 100+ 次攻击伤害序列逐项相同；命中/躲闪统计一致 | **covered（逐 k=0）** | 卡 `openmir2-m3-combat-skills-align`（closed，merge `55a29144`） |
| 3 | 技能 | 每个已实装技能 ≥1 条用例：前置/MP/效果/冷却 | **covered（29 技能各 1 case）** | 同上 |
| 4 | 掉落 / 经验 | 固定种子 100 次击杀掉落逐项相同；经验曲线（含英雄）逐级相同、无 i32 截断 | **not_covered（M3 系统级）** | **已有卡** `openmir2-m3-drop-exp-align`（owner 已开，见 §4） |
| 5 | NPC / 商店 | 买/卖/修理/仓库价格与库存变化一致；失败分支一致 | **partial** | 脚本层 7/7 引 M4；服务端派发 not_covered，**已有卡** `openmir2-m3-npc-shop-align` |
| 6 | 任务 | 接/交/条件判定一致，任务状态落库一致 | **not_covered** | **已有卡** `openmir2-m3-quest-align` |
| 7 | 组队 / 交易 | 归属/分配/取消/掉线中断结果一致 | **not_covered** | **已有卡** `openmir2-m3-party-trade-align` |
| 8 | 公会 / 攻城 | 建会/宣战费用时限/城门状态/自动结束逐项一致 | **not_covered** | **已有卡** `openmir2-m3-guild-castle-align` |
| 9 | 英雄 / 宠物 | 召唤/升级/阵亡/经验分配/上限判定一致 | **not_covered** | **已有卡** `openmir2-m3-hero-pet-align` |
| 10a | 邮件 | 发送 / 到期 / 退回（§6 原文） | **`oracle_absent`（已登记 T-36）** | oracle 全仓无邮件实现（仅客户端开关 `boShowMail`）⇒ **不开实现卡**；Rust 与 oracle 一致地不实现 |
| 10b | 拍卖 | 成交 / 退回（按 oracle 实际语义重划为「元宝寄售 SellOff」） | **partial**（oracle 链与判据已盘；世界侧 `CM_SELLOFF*` 派发 / `DealOffInfo` 世界态 / 帧未实现） | **已有卡** `openmir2-m3-mail-auction-align` + 承接卡 `-selloff-consignment-world` / `-selloff-goldsales-load-wire` / `-selloff-fixture-botsrv` |

> 说明：§6 的 M3 判据表把「战斗」与「技能」列为两项；本表的 2/3 与卡 `openmir2-m3-combat-skills-align`
> 的 ②③ 一一对应。§6「顺序」原文把两者并列写作「战斗/技能」，故十项计数以本表为准。
> §6「⑩ 邮件/拍卖」原为一项；owner 2026-10-13 06:24 按 §6.0-3 重划口径后拆为 **10a（邮件 = `oracle_absent`）/ 10b（拍卖 = SellOff 口径）** 两行（**计数仍作一项**；即 ⑩ 为**混合行** = 10a `oracle_absent` + 10b `partial`，两子行合计计作一项），
> 其判据与 DB 落库现状见 [`m3/M3⑩邮件拍卖现状盘点报告-20261013.md`](m3/M3⑩邮件拍卖现状盘点报告-20261013.md)、登记见 `whitelist.md` 的 T-36。

### 1.1 背包 / 装备（covered）

- **判据（§6 原文）**：穿戴、脱下、负重上限、持久消耗、属性面板数值 —— 同一操作序列下数值逐项相同；边界用例（负重满、等级不够、性别不符）都要有。
- **本片实跑命令**（在 `mir2-rs/` 下）：

  ```powershell
  python -I tests/parity/m3/scoped_diff.py diff `
    tests/parity/m3/sessions/csharp-20261012-085856/scoped_after_norm.jsonl `
    tests/parity/m3/sessions/rust-20261012-090020/scoped_after_norm.jsonl --canonical --fixed-newchr
  ```

- **差异报告**：`GREEN: 作用域内 9 张表逐行一致`，`rc=0` ⇒ **diff = 0**。
- **原始件路径**：`mir2-rs/tests/parity/m3/evidence-t14-rerun-20261012/`（`verify_sha256sums.py` **GREEN 18/18**）；
  关键件 `diff-scoped.txt` sha256 `6db667fb20c2828810c52e049170de5ea489a26529c698a7925e62b8f7e2096f`、
  `inc-health.txt` sha256 `467f69b8421114efc3434ddbc0e06c8a6704df0435ca53cd490b0cfedc866d76`。
- **会话**：C# `sessions/csharp-20261012-085856`（prefix `m3cbag4`） vs Rust `sessions/rust-20261012-090020`（prefix `m3rbag4`）；
  同一固定起点、五步同序列 `CM_PICKUP → CM_TAKEONITEM → CM_TAKEOFFITEM → CM_DROPITEM → CM_EAT`。
- **边界用例**：`characters.Hair/Sex/Job` 在 `--fixed-newchr` 下不置占位、逐字节比（改坏必红已由 reviewer 复现：
  Rust `characters.CX 289→290` ⇒ RED）；负重/等级/性别边界见卡 `openmir2-m3-bag-equip-alignment` 的探针向量（本片不重跑）。
- **本片读数文件**：`evidence-m3-final-acceptance-inventory-20261013/diff/20-bag-scope-canonical-fixednewchr.txt`。
- **收口依据**：`merge_result 818745d9`（merge `bbf63fb0`，head `3516271c`）；`status: closed` `e1ec2305`（收口人 orchestrator；
  两位独立 reviewer 各复现一次）。

### 1.2 战斗（covered，逐 k = 0）

- **判据（§6 原文）**：固定随机种子下 **100 次攻击的伤害序列逐项相同**；命中/躲闪统计与 C# 一致。
- **口径**：`MIR2_RNG_SEED=42` + `MIR2_RNG_ISOLATE_ATTACK=1`（`attack_seed=40545`）、图 `0137`（零 AI 消费者）、
  attacker `(17,20,dir=2)` / victim `(18,20) L255 Hp20669`（F-F pin）、`-AttackPinPair -IsolateAttack`；真机两相。
- **本片实跑命令**（在 `mir2-rs/tests/parity/m3/` 下；会话取自**入库证据包**，可干净检出复算）：

  ```powershell
  $D = 'evidence-m3-isolate-twophase-dod2-20261013/sessions'
  python -I attack_diff.py diff $D/csharp-20261013-024050/attack_seq.jsonl $D/csharp-20261013-024249/attack_seq.jsonl   # c1 vs c2 异栈同侧
  python -I attack_diff.py diff $D/csharp-20261013-024249/attack_seq.jsonl $D/rust-20261013-024604/attack_seq.jsonl     # c2 vs r1 跨侧（判据本体）
  python -I attack_diff.py diff $D/csharp-20261013-024050/attack_seq.jsonl $D/rust-20261013-024604/attack_seq.jsonl     # c1 vs r1 跨侧
  ```

- **差异报告**：三对全部 `NONTRIVIAL a=True() b=True()` + `GREEN: 跨侧攻击序列 diff = 0（attacked=120 hits=57 dodges=63 sum=2425）`，
  `rc=0` ⇒ **逐 k = 0**（120 ≥ 判据要求的 100）。
- **产物 hash**：三份 diff 原始件 `diffs/31-diff-c1c2.txt`/`32-diff-c2r1.txt`/`33-diff-c1r1.txt` 三者
  sha256 均为 `a44e3f2106f92cd2c8978f549d67d76d1c617f3da71c0863abe8def770e3344c`（内容 = GREEN 单行）。
- **原始件路径**：`mir2-rs/tests/parity/m3/evidence-m3-isolate-twophase-dod2-20261013/`
  （`verify_sha256sums.py` **GREEN 69/69**）。
- **命中/躲闪统计一致性**：两侧同一序列 + 同一汇总（`hits=57 dodges=63 sum=2425` + `NONTRIVIAL` 非平凡门两侧均真）；
  `jointable`（bot 命中/躲闪 × 服务端 RNG 权威命中）**GREEN 0 mismatch**（`requests/42-jointable.txt`）。
- **取数流等价（攻击路径 651/651）**：`stream_equiv.py gate` GREEN（详见 §1.3）。
- **纯度门**：C# 专用流 `player=120 base=0`（`attack_stream_gate`，F-N1）；两侧目标可用性门 `120/120`（`server_state_gate`）。
- **收口依据**：`merge_result` merge `55a29144`（head `e62e144a`）；owner 自核三对 diff + 两侧 `SERVER_STATE` 逐击 HP 同步下降。
- **本片读数文件**：`.../diff/21-attack-per-k-three-pairs.txt`。

### 1.3 技能（covered，已实装 29 技能各 ≥1 条用例）

- **判据（§6 原文）**：每个已实装技能各 ≥1 条用例：施法前置、MP 消耗、效果、冷却，逐项一致。
- **本片实跑命令**（在 `mir2-rs/` 下）：

  ```powershell
  cargo test --workspace --offline          # 含 -p mir2-world --test combat_skill_vectors
  python -I tools/evidence-hash/verify_sha256sums.py mir2-rs/tests/parity/m3/skill-probe      # GREEN 5/5
  python -I tools/evidence-hash/verify_sha256sums.py mir2-rs/tests/parity/m3/combat-probe     # GREEN 4/4
  ```

- **差异报告 / 读数**（本片实读工件）：
  - `skill-probe/vectors/skills_registry.json`：`total_magic_rows=108`、`implemented=29`、未实装 `79`（逐条 `dospell_class` + reason 登记）。
    `dospell_class` 直方图：`implemented 29 / magicid_reject 35 / no_switch_case 20 / context_required 10 / warr_reject 8 / no_effect_case 6`。
  - `skill-probe/vectors/skill_vectors.json`：`seed=20261012`、`cases=29`（= 已实装技能数），`oracle` 面含
    `mp_cost_entry`/`prereq_entry`/`rng`/`attack_power`/`dospell_class` ⇒ 每技能的四件事（前置/MP/效果/冷却）逐项可查。
    > 注：`implemented` 28 个 distinct `magic_id` / 29 行 —— `magic_id=48` 在魔表里有两行（`FindMagic` 重复 MagID 首匹配口径，
    > 见卡 `openmir2-m3-combat-skills-align` 的「重复魔号首匹配」片）；计数以**行**为准，此重复是 oracle 事实、非漏登记。
  - `cargo test` 读数：`combat_skill_vectors` **29 passed / 0 failed**；workspace 合计 **84 suites / 687 passed / 0 failed / 16 ignored**。
- **产物 hash**：`skills_registry.json` sha256 `7ff716d30b139379d11c8ee7e56a85c2b2756d6a3adc1b2e930ab9b00f08cdcb`；
  `skill_vectors.json` sha256 `011d8ea5e576bcd83b30f5b3fc6bc751938dd0016f234bd50420a7ff8c4c3ca7`；
  `combat_vectors.json` sha256 `529c5271539a9fa75033d1c1a76cee216d935c0992be408ae2e3961f4a069112`。
- **口径诚实说明**：卡的第一片报告（`M3战斗技能对齐报告-20261012.md`）当时记「11 条已实装技能」；后续批次把
  已实装数推进到 `skills_registry.json` 现读的 **29**。本报告以 **HEAD 工件实测**为准，不以卡上历史措辞为准。
- **未实装 79 条**逐条登记 reason（不静默漏），承接 = 后续 M3 技能批次（`context_required` 中 3 条 `c_real_summon` 17/30/41
  缺前置 `RegenMonsterByName` / 真栈探针，见卡 `openmir2-m3-combat-skills-align` 的 deferred 余额）。
- **本片读数文件**：`.../inventory/30-skills-combat-registry.txt`、`.../gates/52-test-workspace.txt`。

### 1.4 掉落 / 经验（not_covered，M3 系统级）

- **判据（§6 原文）**：固定种子下同一怪 100 次击杀的掉落列表逐项相同；经验曲线（含英雄）逐级相同，不得出现 i32 截断。
- **本报告状态：`not_covered`（未推进 M3 系统级对齐）**——按 owner 2026-10-13 03:20 指令登记，**不自行判绿**。
- **相关先例（不等于本项达成）**：D 线离线对拍（`mir2-rs/tests/parity/D线报告.md` §证据②③④，2026-10-10）报过
  「固定种子 RNG 序列 diff=0 / 经验曲线 1→255 全表 diff=0（含英雄）/ 真实怪 + 合成 17 分支 ×100 杀掉落逐项 diff=0」，
  以及白名单 `D-1..D-6`（掉落相关豁免）。**但那是 D 线离线 golden 口径，不是 M3 的两侧同库同脚本系统级门禁**，
  故本表不据此判 covered。
- **承接**：**已有卡** `openmir2-m3-drop-exp-align`（owner 已开；判据 = §6 原文 + `run_pair.ps1`/`scoped_diff.py`/RNG 账本口径）。
- **本片状态**：`not_verified`（本片未跑任何掉落/经验对拍）。

### 1.5 NPC / 商店（partial）

- **判据（§6 原文）**：买 / 卖 / 修理 / 仓库的价格与库存变化一致；钱不够、负重满等失败分支也要一致。
- **本片实跑命令**（在 `mir2-rs/` 下，port-free，不起栈）：

  ```powershell
  python -I tests/parity/M4/run_npc_cases.py            # → == M4② 结果：可跑用例 7（GREEN） / 等窗口用例 0 ==
  python -I tests/parity/M4/evidence_hashes.py verify   # → GREEN: 证据 hash 复核 19/19 项一致
  ```

- **差异报告**：**7/7 GREEN，rc=0**，其中与 §6「NPC / 商店」直接相关的 3 条：商店-货币兑换（`@changeGold`）、
  修理-RepairAll（`@StdModeFunc26`）、仓库-捆绑卷轴（`@zum_bind1`）。产物 `tests/parity/evidence/M4/npc-cases.txt`
  （本片跑后 `git status` 干净 ⇒ 与入库件逐字节一致）。
- **与 §6 的对表（如实写缺口）**：
  - 覆盖到的：**脚本层**逐行行为（take/give/messagebox 序列）差 0；3 条商人用例含**商店/修理/仓库**。
  - **对不上的缺口（⇒ 本项只算 `partial`）**：
    1. 上述 M4 用例是 **port-free 脚本 harness** 口径（C# `script-parity-cs` vs Rust `script-tool flow-run`），
       **不是** §6 要求的「两侧真栈下价格/库存变化 + DB 作用域字段级 diff」；
    2. **Rust 服务侧 NPC/商人派发（`bins/game-svr`）未覆盖**——M4 README 明写「属 M3 NPC/商店系统对齐」，
       而该服务侧对齐至今没有卡；
    3. **失败分支**（钱不够、负重满）与**库存变化**（买卖后的物品行增减）未被这 7 条用例覆盖。
- **承接**：**已有卡** `openmir2-m3-npc-shop-align`（判据 = 两侧真栈买/卖/修理/仓库 + `characters_item`/
  `characters_bagitem`/`characters_storageitem` 作用域 diff=0 + 失败分支）。
- **本片读数文件**：`.../m4/40-npc-cases.txt`、`.../m4/41-evidence-hashes.txt`。

### 1.6 任务（not_covered）

- **判据（§6 原文）**：用现存脚本任务跑：接/交/条件判定一致，任务状态落库一致。
- **状态：`not_covered`**。相关先例：M4 的 `Q001@main`（地图出矿提示）只覆盖脚本层恒定输出（gold/items），
  **不覆盖**任务状态落库（`characters_quest` 表）与接/交流程。
- **承接**：**已有卡** `openmir2-m3-quest-align`（判据 = 现存脚本任务 + `characters_quest` 作用域 diff=0）。

### 1.7 组队 / 交易（not_covered）

- **判据（§6 原文）**：两人及以上场景：归属、分配、取消、掉线中断的结果一致。
- **状态：`not_covered`**（本片未跑，仓内未见承接卡）。
- **承接**：**已有卡** `openmir2-m3-party-trade-align`。

### 1.8 公会 / 攻城（not_covered）

- **判据（§6 原文）**：建会 / 宣战（费用、时限）/ 城门状态 / 自动结束，逐项一致。
- **状态：`not_covered`**。相关先例：M4 的行会脚本用例（`@requestcastlewar` 缴金条）只到**脚本层**扣金条/说词，
  **不覆盖**攻城费用/时限/城门状态/自动结束的世界逻辑。
- **承接**：**已有卡** `openmir2-m3-guild-castle-align`。

### 1.9 英雄 / 宠物（not_covered）

- **判据（§6 原文）**：召唤 / 升级 / 阵亡 / 经验分配 / 上限判定一致。
- **状态：`not_covered`**。M3 技能侧有 3 条 `c_real_summon`（17/30/41）因缺
  `SystemShare.WorldEngine.RegenMonsterByName` / 真栈探针而 deferred，属**技能召唤**而非系统级英雄/宠物对齐。
- **承接**：**已有卡** `openmir2-m3-hero-pet-align`。

### 1.10 邮件 / 拍卖（邮件 = `oracle_absent`；拍卖 = `partial`，按 SellOff 口径）

- **判据（§6 原文）**：发送 / 到期 / 成交 / 退回，DB 落库字段级 diff = 0。
- **邮件：`oracle_absent`（已登记 `whitelist.md` T-36）**。oracle 全仓无邮件实现（只有客户端开关 `boShowMail`，
  无实体 / 无 `CM_*`·`SM_*` / 无表 / 无调用链）⇒ 「发送 / 到期 / 退回」无行为可对齐；owner 2026-10-13 06:24 裁决：
  **不开实现卡**，Rust 与 oracle 一致地不实现（发相关包同样无响应/忽略）。
- **拍卖：`partial`**。oracle 实为**只读且坏的 P2P 元宝寄售（SellOff）**（`M2Share.SellOffItemList`/`DealOffInfo` +
  `SaleDeal`/`SellOffConfirm` + 3 个 `RM_*` + 7 个 `CM_SELLOFF*` 派发；`goldsales` 加载只读、读错列名、`save` 调用点全注释）；
  5700 `MarketSystem` 是**死接线**（Rust M1 S4b 已 1:1，非缺口）。Rust **世界侧派发 / 世界态 / 帧 = 未实现**
  （`rg -n 'CM_SELLOFF|SellOffItem|DealOffInfo' mir2-rs/{bins/game-svr/src,crates/world/src,crates/shared/src}` = no match）。
- **承接**：**已有卡** `openmir2-m3-mail-auction-align`；实现/夹具已拆 3 张承接卡
  `-selloff-consignment-world` / `-selloff-goldsales-load-wire` / `-selloff-fixture-botsrv`。
- **盘点报告**：[`m3/M3⑩邮件拍卖现状盘点报告-20261013.md`](m3/M3⑩邮件拍卖现状盘点报告-20261013.md)。

---

## 2. 300 假人 24h 长跑方案（**本片只写方案，未跑**）

> owner 2026-10-13 03:20 指令：**24h 长跑这轮不跑**——按卡 `sequencing`，长跑期**必须独占机器**
> （期间无其它 lane 编译/压测），窗口由 owner 收拢发放。**本片状态 = `not_run`；我在等窗口**
> （已在卡上 `status` 登记）。

### 2.1 判据（§6 M3 总验收 + issue DoD② 原文，三件，机判）

| # | 判据 | 机判口径（命令 → 看什么数 → 阈值） |
| --- | --- | --- |
| a | **无 panic / 无未捕获异常** | 全窗内 Rust 各服务进程存活（按 pid 实拍）；各进程日志 `panicked` 命中数 **= 0**；BotSrv `load_stats.ndjson` 的 `internal_errors` **= 0**；无进程提前退出（`rc` 只在收工时出现）。 |
| b | **无掉线** | `load_report.py`：`conn_lost_steady` **= 0**；**且** `in_world` 曲线在稳态段（`[ramp_end, end-300s]`）按 5 分钟桶的均值 ≥ `0.95 × 桶峰值均值`、无单调持续跌落；连接数曲线稳态段同样无跌落。（爬坡期掉线单列 `conn_lost_ramp`，不与稳态混算。） |
| c | **登录 + 选角成功率 100%** | ① 登录：BotSrv `login_ok` **= 300（100%）**；② 选角：协议探针 `select_chr` **300/300**（M1⑤ 口径 `tools/m1-acceptance/probe_m1_chain.py`）。若「选角成功但未进世界」，另按 (b) 的 `in_world` 曲线**分开如实报**，不许把选角与进世界合成一个成功率。 |

**红检（可判伪）**：长跑中人为停掉一个网关（或 kill 一个 game-svr）⇒ (a)/(b) 必 RED；还原 ⇒ 绿（记录命令与前后读数）。

### 2.2 命令草案（全 Rust 链；起栈命令一律 `> log 2>&1`，禁 `| tee/tail/head/grep`）

```powershell
# 0) 窗口前置（owner 收拢后）：登记 lane-status.log + 进程实况 + 本机负载探针三点齐，确认无其它车道编译/压测
# 1) 隔离 target 重建（长跑二进制冻结）
$env:CARGO_TARGET_DIR = 'E:\tmp\openmir2-m3final-target'
cargo build --offline -p game-svr -p game-gate -p login-srv -p login-gate -p sel-gate -p db-srv
# 2) 起全 Rust 栈（含 GameGate 7200 / game-svr）
powershell -NoProfile -ExecutionPolicy Bypass -File tools/m2-acceptance/start_rust_stack.ps1 `
  -RustBinDir E:\tmp\openmir2-m3final-target\debug -LogDir E:\tmp\m3final\stack -Seed 42 > E:\tmp\m3final\stack-start.txt 2>&1
# 3) 端口所有权 / 入口门（Rust 入口接对端口）
powershell -NoProfile -ExecutionPolicy Bypass -File tools/m2-acceptance/preflight.ps1 > E:\tmp\m3final\preflight.txt 2>&1
# 4) 300 假人 24h（= 86400s hold；Steady 段由 load_report 按 hold 收口）
powershell -NoProfile -ExecutionPolicy Bypass -File tools/botload/load_gate.ps1 `
  -Tiers 300 -HoldSec 86400 -Criteria "login,conn" -Prefix m3final -StaggerMs 20 -MemSampleSec 2 `
  -OutDir E:\tmp\m3final > E:\tmp\m3final\load-gate.txt 2>&1
# 5) 曲线/分档表（判据 (b)(c) 的机判本体）
python -I tools/botload/load_report.py --stats E:\tmp\m3final\bots\load_stats.ndjson `
  --mem E:\tmp\m3final\mem.ndjson --count 300 --hold-sec 86400 --criteria login,conn `
  --hostload E:\tmp\m3final\hostload.txt --out E:\tmp\m3final\report.json
#   ⇒ login 判据 = login_success_pct（阈值 100）；conn 判据 = conn_lost_steady == 0
# 5b) 选角成功率 300/300（协议探针，M1⑤ 口径）
python -I tools/m1-acceptance/probe_m1_chain.py --count 300 --out E:\tmp\m3final\probe.json
# 5c) in_world 曲线稳态段无跌落（Ramp=1h；5min 桶；判据 = 桶均值 min ≥ 0.95 × 桶均值 max）
python -I tools/botload/in_world_stability.py E:\tmp\m3final\bots\load_stats.ndjson `
  --ramp-ms 3600000 --bucket-ms 300000 --tail-ms 300000 --min-ratio 0.95 --json E:\tmp\m3final\in-world-stability.json
# 6) 收工：停栈 + 十端口实拍 + 卡上写「窗口已释放」
powershell -NoProfile -ExecutionPolicy Bypass -File tools/m2-acceptance/start_rust_stack.ps1 -Stop
```

> 说明：`5c` 的 `tools/botload/in_world_stability.py` 已随本片入库（`--selftest` 7/7：丢一桶⇒RED、缺 `in_world`/坏 JSON⇒PREREQ、无稳态段⇒PREREQ；
> 实跑演示见证据包 `inventory/31`/`32`）；`5b` 复用 M1⑤ 的 `probe_m1_chain.py`（`--count 300`，口径 = 登录→选服→选角四步的 `select_chr` 计数）。

### 2.3 采集窗口登记（§17.1 要求，跑前必写）

长跑期**必须独占机器**；采集前由 owner 收拢窗口并登记三点：
① `lane-status.log`（无其它 lane 在编译/压测）；
② 进程实况（无并发 `cargo`/`rustc`/`cargo-clippy`/`msbuild`/第三套栈）；
③ 本机负载探针（CPU / 内存 / 磁盘；`load_gate.ps1` 采 `hostLoadLines`，见 §17.1-2「C6 load gate 没有本机负载字段」的教训）。
**采集起止时间 + 三点读数**随原始件入库；这三点缺一，长跑结论**不可归属**。

### 2.4 曲线 / 分档表口径

- 原始：BotSrv 每 5s 一行 `load_stats.ndjson`（`login_ok`/`conn_lost`/`in_world`/`internal_errors`/`tick_*`）；
  每 `MemSampleSec` 一行 `mem.ndjson`（RSS/活跃字节）。
- 汇总：`load_report.py` 出「档位 × 登录成功率 / 稳态掉线 / in_world / tick P50·P90·P99·max / 稳态样本 / CPU 峰」表；
  300 档为单行；若同时采 tick，则附 tick 分档曲线（M5 的 P99 三档另属卡 `openmir2-m5-convergence-drills`）。
- **稳定性曲线**：`in_world` 与连接数各自画全窗曲线，标注 ramp 结束点与稳态段；判据 (b) 的 5 分钟桶表随报告入库。

### 2.5 本片状态

- **`not_run`**：窗口未发放，本片**未起任何栈、未跑任何假人**；只在卡上登记「我在等窗口」。
- 开跑前置：owner 收拢窗口 + §2.3 三点登记 + 本方案经 owner 认可。

---

## 3. 残留物清单

- **本片入库（新增）**：
  - `mir2-rs/tests/parity/M3-report.md`（本报告）；
  - `mir2-rs/tests/parity/m3/evidence-m3-final-acceptance-inventory-20261013/**`
    （`README.md` + `commands.txt` + `verify/` + `diff/` + `inventory/` + `m4/` + `gates/` + `window/` + `SHA256SUMS.txt`）；
  - `tools/botload/in_world_stability.py`（owner 87e84b4f 要求「预置」：DoD② 判据 (b) 的 `in_world` 稳态无跌落机判器；
    `--selftest` 7/7，改坏必红；实跑演示见证据包 `inventory/31`/`32`）。
- **本片未新增任何临时目录/临时库/进程**；**未起栈、未占端口**（九端口实拍 FREE，见证据包 `window/01-ports-9.txt`）。
- **仓外临时物**：无（本片所有命令均读仓库内既有件；`run_npc_cases.py` 只写仓库内 `evidence/M4/npc-cases.txt`，
  且重跑字节不变、`git status` 干净）。
- **本 worktree 其它未跟踪件**（非本片产生，属上一片 ②③ 真机跑会话）：
  `mir2-rs/tests/parity/m3/sessions/{csharp-20261013-023941,csharp-20261013-024050,csharp-20261013-024249,
  rust-20261013-024454,rust-20261013-024604}/` —— 已在 ②③ 证据包含 3 个会话（`-024050/-024249/-024604`），
  其余 2 个（`-023941/-024454`）为冒烟轮会话，**本片不动**（归属 ②③ 卡收口人）。

## 4. 未收口项（含承接卡）

| 未收口项 | 现状 | 承接卡 / 建议卡名 |
| --- | --- | --- |
| §6 M3 门禁 `scoped_diff.py diff --canonical` 有一处 RED | **本片实跑确认**：`--canonical` ⇒ `RED 作用域内 4 行不一致`（全在 `mir2_db.characters_ablity`；`rc=1`）；`--canonical --fixed-newchr` ⇒ `RED 8 行`（+ `characters.Hair`，夹具随机输入）；同侧 C# 自比 `--canonical --fixed-newchr` = GREEN 9 表。⇒ 跨侧差异落在 `characters_ablity`（`Ac/MAxMP/MaxWeight/MaxWearWeight/MaxHandWeight/Hp`），疑持久化/断线存档轴；**非** DoD② 攻击 RNG 轴（后者已逐 k=0）。原始件 `diff/23-scope-m3-gate-canonical.txt` | **已有卡** `openmir2-m3-scoped-diff-ability`（即原 F-2 follow-up） |
| 掉落 / 经验（§6 顺序 ④） | `not_covered`（D 线离线先例存在，非 M3 系统级） | **已有卡** `openmir2-m3-drop-exp-align` |
| NPC / 商店（§6 顺序 ⑤）服务端派发 + 失败分支 | `partial`（脚本层 7/7 引 M4；服务端 not_covered） | **已有卡** `openmir2-m3-npc-shop-align` |
| 任务（§6 顺序 ⑥） | `not_covered` | **已有卡** `openmir2-m3-quest-align` |
| 组队 / 交易（§6 顺序 ⑦） | `not_covered` | **已有卡** `openmir2-m3-party-trade-align` |
| 公会 / 攻城（§6 顺序 ⑧） | `not_covered` | **已有卡** `openmir2-m3-guild-castle-align` |
| 英雄 / 宠物（§6 顺序 ⑨） | `not_covered` | **已有卡** `openmir2-m3-hero-pet-align` |
| 邮件（§6 顺序 ⑩a） | **`oracle_absent`**（已登记 T-36） | **不开实现卡**；Rust 与 oracle 一致地不实现（oracle 无此功能） |
| 拍卖（§6 顺序 ⑩b，元宝寄售 SellOff） | `partial`（oracle 链/判据已盘；世界侧派发未实现） | **已有卡** `openmir2-m3-mail-auction-align` + 承接卡 `-selloff-consignment-world` / `-selloff-goldsales-load-wire` / `-selloff-fixture-botsrv` |
| **300 假人 24h 长跑**（DoD②） | **`not_run`**（等 owner 收拢窗口；方案见 §2） | 本卡（`openmir2-m3-final-acceptance`）自身，窗口发放后开跑 |
| §6 M3 十项「全绿」总判 | **未达成**（3 covered / 2 partial / 1 oracle_absent / 5 not_covered = 11 子行；⑩ 为**混合行** 10a `oracle_absent` + 10b `partial`，两子行合计计作一项 ⇒ 仍为十项） | 上述各承接卡收敛后回本卡复核 |

## 5. 本片命令与证据四件套

- **命令**：见证据包 `commands.txt`（各命令的完整串 + 本报告引用的行）。
- **原始输出**：见证据包 `verify/` `diff/` `inventory/` `m4/` `gates/` `window/`（逐件可复核）。
- **路径**：`mir2-rs/tests/parity/m3/evidence-m3-final-acceptance-inventory-20261013/`。
- **hash**：`SHA256SUMS.txt`（仓库字节 = LF 归一 / git blob 口径）。复核命令：

  ```powershell
  python -I tools/evidence-hash/verify_sha256sums.py mir2-rs/tests/parity/m3/evidence-m3-final-acceptance-inventory-20261013
  python -I tools/evidence-hash/verify_sha256sums.py --check-extra mir2-rs/tests/parity/m3/evidence-m3-final-acceptance-inventory-20261013
  ```

- **四门禁（本片交付树，零 `.rs`）**：
  - `cargo fmt --all -- --check` → `rc=0`（`gates/50-fmt.txt`）
  - `cargo clippy --workspace --all-targets --offline -- -D warnings` → `rc=0`（`gates/51-clippy.txt`）
  - `cargo test --workspace --offline` → `84 suites / 687 passed / 0 failed / 16 ignored`，`rc=0`（`gates/52-test-workspace.txt`）
  - `cargo run --offline -p msgcodegen -- --check` → `604 条常量`，`rc=0`（`gates/53-msgcodegen.txt`）
