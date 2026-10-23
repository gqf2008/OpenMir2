# 窗口判据补齐（首批 10/20）：把「半可执行」回填成机器可判（tier=L）

> 卡：`openmir2-window-criteria-backfill`（principal `openmir2-worker-69`，worktree `E:/tmp/wt-worker-69`，
> 分支 `card/window-criteria-backfill`）。
> 上游：卡 `openmir2-window-criteria-quality-audit`（审计返工 v2，文件 head `ed7453ac5b1dc70c5cbc8cc86c49a0f6e216ed57`，
> 合并点 `5a6ed54c1`，blob `651a29f6634e3e75d72f1b65d45773d2f6f9f730`）。
> 性质：**只补判据文本**（docs 总表 + 每张卡的签名 comment），未改代码/工具/goal_gate 判据/任何人卡片 status。
> 模板：`docs/window-criteria-template.md` 尚未合入（模板卡 `openmir2-window-criteria-template` 仍在
> `needs-review`）⇒ 按审计 §3 与卡面 6 字段：①前置 ②可复跑命令 ③期望 ④判据 ⑤不可比字段与理由 ⑥负责人与状态。

## 1. 清单先定：审计 §5.2/§5.3 的 20 张（从文件现取）

**来源**：`docs/window-criteria-quality-audit-20261024.md`（head `ed7453ac5`）§5.2 行 **119–137**（② 19 张，#11–#29）、
§5.3 行 **143**（③ 1 张，#30）。下表第一列是审计卡里的序号，为便于对照保留原编号；「行」= 该文件行号。

| 审计# | 卡 | 档 | 行 |
|---:|---|---|---:|
| 11 | `openmir2-castle-merchant-disappear-downlink` | ② | 119 |
| 12 | `openmir2-chat-channels` | ② | 120 |
| 13 | `openmir2-m3-castle-gold-tax` | ② | 121 |
| 14 | `openmir2-m3-clock-injection` | ② | 122 |
| 15 | `openmir2-m3-rng-scene-driver` | ② | 123 |
| 16 | `openmir2-m3-drop-exp-wiring` | ② | 124 |
| 17 | `openmir2-outbound-encoders-red-batch` | ② | 125 |
| 18 | `openmir2-orphan-encoder-wiring` | ② | 126 |
| 19 | `openmir2-world-tick-expansion` | ② | 127 |
| 20 | `openmir2-buff-system` | ② | 128 |
| 21 | `openmir2-gategate-keepalive` | ② | 129 |
| 22 | `openmir2-selgate-reconnect` | ② | 130 |
| 23 | `openmir2-mb7-monsters` | ② | 131 |
| 24 | `openmir2-encoding-core-e1-e2` | ② | 132 |
| 25 | `openmir2-weapon-upgrade-values` | ② | 133 |
| 26 | `openmir2-login-529-status-text` | ② | 134 |
| 27 | `openmir2-castle-guard-target` | ② | 135 |
| 28 | `openmir2-weapon-durability-frames` | ② | 136 |
| 29 | `openmir2-monster-showhp-53-broadcast` | ② | 137 |
| 30 | `openmir2-m3-monster-active-seek` | ③ | 143 |

## 2. 取样：从 20 张里取 10 张（规则 + 理由）

**规则（读卡面/夹具之前写定，可复算）**：以 `docs/window-batch-plan-20261023.md` 的「场景」列为线索，按**离线载体**分档：

1. **A 组 = 有离线载体、今日实跑 rc=0**。载体三类，逐张在表里标注：
   (i) 现成场景文件（`tools/probe/scenarios/*.json`，`--check-scenario` rc=0）；
   (ii) master 上既有的 `mir2-rs/tests/parity/m3/**/run_*_pair.ps1`（`-SelfCheckNoStack` rc=0）；
   (iii) 同卡构造级 cargo 用例（whitelist T 行写死期望，rc=0）。
   **场景文件存在但 dry-run 判 `BLOCKED` 的只算 (iii)**，并显式标 BLOCKED。
2. **B 组 = 无任何离线载体**（前置被阻塞 / 多会话 / 长跑 / 工具缺口 / ③ 无载体）⇒ 留到下一批。
3. **名额**：卡面验收是「从 20 张里取 10 张」，本轮上限 10。实测 **A 组共 11 张**（下表 10 张 + `openmir2-login-529-status-text`），
   故不是「取全部 A 组」；`login-529` 判据本身已可机判（`login_only` check rc=0），不属本卡要补的「判据不可执行」对象，
   顺延下一批（理由见未取表）。

**取样的 10 张（全部为 A 组；括号内为载体类型）**：

| 卡 | 组/载体 | 现成载体 | 今日实跑结果 |
|---|---|---|---|
| `openmir2-m3-rng-scene-driver` | A-(ii) | `m3/rng-scene/run_rng_scene_pair.ps1` + `rng_scene_scenario.json` | `-SelfCheckNoStack` **rc=0（23/23）** |
| `openmir2-m3-drop-exp-wiring` | A-(ii) | `m3/drop-exp/run_drop_exp_pair.ps1` + `drop_exp_scenario.json`（batch-plan §1F 的对应卡名是 `openmir2-m3-drop-exp-align`，非本卡 id） | `-SelfCheckNoStack` **rc=0（21/21）** |
| `openmir2-weapon-upgrade-values` | A-(i) | `tools/probe/scenarios/upgrade_retrieve.json` | check rc=0 + card_window dry-run **rc=0** |
| `openmir2-encoding-core-e1-e2` | A-(iii) | `crates/encoding/tests/cp936_vectors.rs` 等（纯离线） | `cargo test` **8/8 + 7/7 + 4/4 rc=0** |
| `openmir2-world-tick-expansion` | A-(i)+(iii) | `login_only.json` + `MIR2_RNG_LOG`；构造级 `npc_tick`/`refresh_game_time`/`line_notice` | check/dry-run **rc=0**；cargo **2/2 + 2/2 + 15/15** |
| `openmir2-buff-system` | A-(iii)；场景 **BLOCKED** | `spell_cast.json`（自带 **BLOCKED-FIXTURE**）+ `buff::tests` | check rc=0；dry-run **rc=2 BLOCKED**；cargo **16/16** |
| `openmir2-outbound-encoders-red-batch` | A-(i)+(iii) | `upgrade_retrieve.json`（R2）+ `cm_make_drug_gates`；制药链无场景 | dry-run **rc=0**；cargo **1/1** |
| `openmir2-weapon-durability-frames` | A-(iii)；**无场景文件** | T-199 精确期望 + `weapon_durability_tests`；**655/1102 真机场景待建** | cargo **13/13 + 4/4 rc=0** |
| `openmir2-mb7-monsters` | A-(iii) | T-143/T-144 + `mb7` 构造级用例；**真机场景待建** | cargo **6/6 rc=0** |
| `openmir2-castle-guard-target` | A-(iii) | T-96 + `proper_target` 构造级用例；**R6 castle_zone 场景待建** | cargo **5/5 + 1/1 + 1/1 rc=0** |

> **载体纠偏（review R5）**：`openmir2-weapon-durability-frames` **没有场景文件**（`ls tools/probe/scenarios/ | grep -i durability` = 0），
> 载体是 T-199 + 构造级 cargo；`openmir2-buff-system` 的 `spell_cast.json` 存在但 `card_window --dry-run` 判 `BLOCKED`（rc=2，缺 `characters_magic`），
> 故只算构造级；`openmir2-m3-drop-exp-wiring` 在批次计划里没有同名行，§1F 对应的是 `openmir2-m3-drop-exp-align`（`docs/window-batch-plan-20261023.md:134`）。
> 规则按「离线载体」而非「是否有场景文件」重写，每张的载体类型与 blocked 状态都已落到表里，可逐张复算。

**未取的 10 张及理由**（9 张 B 组 + 1 张顺延的 A 组；同样逐条给结论，属本批「不取/先不排」清单）：

| 卡 | 不取理由 |
|---|---|
| `openmir2-castle-merchant-disappear-downlink` | 前置被阻塞：⑧ 沙巴克攻城层未接线，`castle_under_war` 生产无写点（审计 F6）⇒ 先补 `blocked_by`，不排窗口 |
| `openmir2-chat-channels` | 多会话：三前缀收件人集合需 ≥3 个会话；无场景（T-118 的 NRE 已 1:1） |
| `openmir2-m3-castle-gold-tax` | 前置被阻塞：⑧-1 C# `GetTickCount` 注入片未落地 + owner 独占窗 |
| `openmir2-m3-clock-injection` | 前置不可用：C# 影子 hook 半待 owner 裁决（IL patch / 影子源码 / 墙钟编排） |
| `openmir2-orphan-encoder-wiring` | 单客户端验不到：② 需两真客户端、③ 需 ±12 第二观察者 ⇒ 只能构造级替代（本批不做） |
| `openmir2-gategate-keepalive` | 长跑 + `==` vs `>` 与 3min 分支可达性待 owner 裁决 |
| `openmir2-selgate-reconnect` | 工具缺口：需 DBSrv 重启、断链期间真实客户端表现 |
| `openmir2-login-529-status-text` | **A 组，判据本身已可机判；本轮名额已满 ⇒ 顺延下一批**：`login_only` 场景 `--check-scenario` rc=0（今日实跑），529 两侧 `body_hex` 逐字节比即闭环。旧结论「Rust 不上报 `SS_SERVERINFO` ⇒ 状态段恒 0」**已作废**：`185a6c8cb` 已合入且是 HEAD 祖先，`mir2-rs/bins/game-svr/src/ids.rs:17-19` 明写两处上报；`docs/window-batch-plan-20261023.md`（`80935bac9`）§7.4 第 8 行与 §4 抽样亦判「能」。（上游审计 §5.2 #26 的 ③ 分类基于旧卡面，建议下一批重分类） |
| `openmir2-monster-showhp-53-broadcast` | 多会话 ±12 帧集合 + 无场景/具名命令 |
| `openmir2-m3-monster-active-seek`（③） | 无场景、无比较对象、无期望、无命令；映射到 GUI 真实客户端 `mir_click.ps1` |

> 10 张的具体 6 字段回填**已逐张发签名 comment 到该卡线程**（见 §4 每节的卡片可见位置），
> 本文件只做总表，避免「只写 docs、卡上看不到」。

## 3. 统一口径（10 张共用）

- **判据一律 fail-closed**：命令 rc≠0、报告缺件、0 帧、缺库快照 ⇒ FAIL/UNDECIDABLE，不得记 PASS。
- **期望值来源优先级**：oracle 源码行号 > whitelist T 行 > 构造级用例 > 卡面散文。散文与 T 行冲突时以 T 行为准
  （审计 F6 撤销条的教训）。
- **不可比字段必须写死**，避免「按字面必红」：墙钟相位、`VisibleHumanList` 500ms 缓存、未实现帧、
  session/MakeIndex 变量、GUI 渲染。
- **真机未跑**：本批全部 10 张的「真机窗口」均为**未跑**；只有离线/构造级命令实跑过。
  这是如实状态，不把「判据补好」写成「窗口已过」。

## 4. 逐张回填（6 字段）

> 每节末尾「实跑证据」= 本次 2026-10-24 在 `E:/tmp/wt-worker-69` 真跑的命令与 rc。
> 卡片可见位置 = 该卡线程的签名 comment（`kind=comment`, `actor=openmir2-worker-69`, `verified=true`），oid 如下：

| 卡 | comment oid（初版 = 6 字段；r2 = 补 ⑥ owner，见 §8 R2） |
|---|---|
| `openmir2-m3-rng-scene-driver` | `fc00ed3ac88a1cede6cf9b1b7397f733ffd8f8ea`（初版）/ `8a2b12f856281997f8824707927832733bbc43ee`（r2 补 ⑥） |
| `openmir2-m3-drop-exp-wiring` | `abc94e9fbd5facdaf29f92e5d5d6cfc6bbf1cb12`（初版）/ `5e89c005a86064ff7759cc996f09d8cccf5ad668`（r2 补 ⑥） |
| `openmir2-weapon-upgrade-values` | `00b9a64e0f5dfa587ae7c9defbfd908ef05b3698`（初版）/ `cda8dbc03f718b9b256bd2e36c312f7e6221effc`（r2 补 ⑥） |
| `openmir2-weapon-durability-frames` | `d20b54579d3fd7d9a168793c83838bfbd3c65301`（初版）/ `785763f327311eda8efa03945af7d0d6c2ee03d8`（r2 补 ⑥） |
| `openmir2-encoding-core-e1-e2` | `bce06ff8c53b5d522cf59bd56f11420bf9e0e1bc`（初版）/ `7ecb8b415eabbf9b409f335c8bf50cc0ea54d1fb`（r2 补 ⑥） |
| `openmir2-world-tick-expansion` | `76768fd40f40baa2cda7a652b9002cb988cd6f3c`（初版）/ `0ddf3541620ad42047c00bbe280a0a35a77687d0`（r2 补 ⑥） |
| `openmir2-mb7-monsters` | `2d2d5b3461a716577701eb0c83b41f0ed340dbd4`（初版）/ `90fd394b72a5297474a0c481262112a6b34e6df5`（r2 补 ⑥） |
| `openmir2-buff-system` | `41fcf511017a95de909f5532083e14eae59e04b9`（初版）/ `ef8fa8fc88e1f9b7408f9cca11eb0ffcd6a8c41f`（r2 补 ⑥） |
| `openmir2-castle-guard-target` | `77d1421ba61d64aa7fad7ce83afc3f41d99e7623`（初版）/ `b64f6ca7242b287fe0b2861d39ae7a202587ec6f`（r2 补 ⑥） |
| `openmir2-outbound-encoders-red-batch` | `0118efc8d767495aa88fea8009ffe52acdfe9d8f`（初版）/ `70d4e29ca41f93ef250890287b7300cfbc2d03ad`（r2 补 ⑥） |

### 4.1 `openmir2-m3-rng-scene-driver`（审计 #15，行 123）

- **W-1 RNG 序列真机两侧对拍（W9 独占窗）**
- **① 前置/夹具**：独占端口 `5000/7000/7100/7200`；两侧各一次 `-Side csharp|rust`；owner 发放的 `-Go` + `-PinScene`；
  seed=42；场景 `m3/rng-scene/rng_scene_scenario.json`（map=0137、ticks=600、固定走位/待机/单怪）；
  **前置未就绪**：BotSrv real wiring（进 0137 + pin 单只固定怪 + dump）在承接写集（`crates/world` + `bins/game-svr` + `src`），
  本脚本 header 明写「本片只交校验器 + 编排骨架 + BotSrv 固定场景序列（默认关）」。
- **② 可复跑命令**（✅=已实跑）：
  - ✅ `powershell -NoProfile -ExecutionPolicy Bypass -File mir2-rs/tests/parity/m3/rng-scene/run_rng_scene_pair.ps1 -SelfCheckNoStack`
  - ✅ 单侧形状（需真机产物）：`… -TraceCheckOnly -Side csharp -AStream <csharp-rng-stream.tsv> -Seed 42`
  - ✅ 场景门（需两侧产物）：`… -GateOnly -AScene <csharp-scene.json> -BScene <rust-scene.json> -AStream <csharp.tsv> -BStream <rust.tsv>`
  - ✅ 单一入口：`… -SceneGroupDiff -AScene … -BScene … -AStream … -BStream …`（非 GREEN 拒比）
  - 真机（**未跑**，需 owner）：`… -Side csharp -Go -PinScene` / `… -Side rust -Go -PinScene`
- **③ 期望**：每侧 `<side>-rng-stream.tsv` 头 `INJECT_OK <seed>`、**5 列**、`seq` 从 1 到 N **连续无断号**、`site` 非空；
  `<side>-scene.json` canon `m3-rng-scene-descriptor-v1`，map=0137/seed=42/ticks=600；两侧账本按声明消费者序
  **逐项 `site`/`order` 相等**；场景一致性门 identical。
- **④ 判据**：`rng_trace_diff.py` rc=0（0 行 diff）且 `-GateOnly` rc=0 且 `-SceneGroupDiff` rc=0；任一非 0 = FAIL。
  离线 `-SelfCheckNoStack` 23/23 只证「校验器形状可信」，不当真机判据。
- **⑤ 不可比**：绝对 tick/墙钟相位（Rust 200ms tick vs oracle 50ms）；跨 pass `NpcLimit` 交错（T-64）；
  BotSrv real wiring 未落地期间「无产物」不得当 FAIL。
- **⑥ 负责人/状态**：owner `openmir2-worker-22`；本批 **offline 已跑通过（23/23, rc=0）**，**真机未跑**；
  blocked_by = real wiring 未落（本卡自身 W9 窗项，非另卡）。
- **实跑证据**：`run_rng_scene_pair.ps1 -SelfCheckNoStack` → `SELFTEST ok=23/23 failures=`，`rc=0`。

### 4.2 `openmir2-m3-drop-exp-wiring`（审计 #16，行 124）

- **W-1 真机 100 杀两相窗口；W-2 quest 真机窗口（另判）**
- **① 前置/夹具**：单 attacker（`Bots=1`）、`Kills=100`、`Seed=42`、`DropExpStepMs=500`、`DropExpStartDelayMs=3000`；
  端口 `5000/7000/7100/7200`；owner `-Go`；**仍缺**：0137 上运行时 pin 出单只固定怪
  （`run_drop_exp_pair.ps1` header 明写归 shared 承接卡 **`openmir2-m3-rng-scene-driver`**）。
- **② 可复跑命令**：
  - ✅ `powershell -NoProfile -ExecutionPolicy Bypass -File mir2-rs/tests/parity/m3/drop-exp/run_drop_exp_pair.ps1 -SelfCheckNoStack`
  - ✅ 只比较两份既有原始件（离线）：`… -DiffOnly -CsharpRaw <f> -RustRaw <f>`
  - ✅ 纯度门（离线）：`… -PurityOnly -PuritySide csharp -PurityStream <rng-stream.tsv> -PurityRaw <drop_exp_raw.jsonl> -ExpectedKills 100`
  - 真机（**未跑**，需 owner + pin 怪）：`… -Side csharp -Go` / `… -Side rust -Go`
- **③ 期望**：每侧 `drop_exp_raw.jsonl` **恰好 100 行**（一次击杀一行，schema 见 `drop_exp_scenario.json`）；
  `drop_exp_diff.py` 比较 **掉落列表多重集/逐槽位 + 金币 + 经验增量**，diff=0；
  纯度门：除「1 只 pin 怪 + 击杀→掉落/经验取数」外 **RNG 消费者数 = 0**。
- **④ 判据**：`-DiffOnly` rc=0 且纯度门 rc=0 且行数=100；未知 RNG 站点 ⇒ 点名 RED（fail-closed）。
- **⑤ 不可比**：Wondering/墙钟须 pin 抑制；组队 `GainExp` 分支未建模（T-32①）⇒ 显式不比对；quest 窗口另判。
- **⑥ 负责人/状态**：owner `openmir2-worker-22`；本批 **offline 已跑通过（21/21, rc=0）**，**真机未跑**；
  blocked_by = `openmir2-m3-rng-scene-driver` 的 pin 单怪 real wiring。
- **实跑证据**：`run_drop_exp_pair.ps1 -SelfCheckNoStack` → `SELFTEST ok=16/16` + `SELFTEST ok=21/21`，`rc=0`。

### 4.3 `openmir2-weapon-upgrade-values`（审计 #25，行 133）

- **W-1 @upgradenow 四字段 + 657；W-2 取回 Dura 档/n90 逐抽；W-3 202/200 ClientItem 字节；W-4 零 RNG 探针；W-5 多件首饰三列独立**
- **① 前置/夹具**：`tools/probe/scenarios/upgrade_retrieve.json`（沙巴克铁匠，placement map=3 x=320 y=328；
  merchant_actor 由 `card_window.py` 参数发现解析，不得猜值）；alt profile 测试库 `mir2_test_*`；
  取回时效门 `UPgradeWeaponGetBackTime=600000`，场景 sleep 660000ms。
- **② 可复跑命令**：
  - ✅ `python -I -B tools/probe/mir2_probe.py --check-scenario --scenario tools/probe/scenarios/upgrade_retrieve.json --var merchant_actor=0`
  - ✅ `python -I -B tools/conformance/card_window.py --dry-run --scenario tools/probe/scenarios/upgrade_retrieve.json`
  - 真机（**未跑**，需 owner 窗口）：`python -I -B tools/conformance/card_window.py --scenario tools/probe/scenarios/upgrade_retrieve.json --out E:/tmp/window/upgrade-retrieve-<ts> --card openmir2-weapon-upgrade-values`
- **③ 期望**：成功首帧 `SM_DELITEM(202)`，紧跟 52/41/653/657×2/709；对拍 **Dc/Sc/Mc/Dura 四字段 + 657 帧**；
  取回分支 `n10>63`/`n10>79`（`Merchant.cs:677-733`）逐抽；`SM_DELITEM(202)`/`SM_ADDITEM(200)` 的 `ClientItem`
  字段/字节（线上 `Desc` 恒 0、统一套差量）；首饰 `DC/SC/MC` 经 `_MIN(255,..)` 三列独立。
- **④ 判据**：`report.json` 中上述帧/列 diff=0；RNG trace 逐抽一致；DB `characters_item`/`characters_item_attr` 字段级相同。
- **⑤ 不可比**：657 近邻收件人集合受 `VisibleHumanList` 500ms 缓存影响（T-201 ④/D-13）⇒ 静止玩家、按帧集合比；
  C2S 的 MakeIndex/session 变量；660s 墙钟等待不参与判定；T-95① 破零链（1102/203/52/752）归 T-199，本卡不比对。
- **⑥ 负责人/状态**：owner `openmir2-orchestrator`；本批 **check + dry-run 已跑通过（rc=0）**，**真机未跑**；blocked_by = none。
- **实跑证据**：`--check-scenario`（带 `--var merchant_actor=0`）rc=0，`scenario OK: upgrade_retrieve (15 steps, params=merchant_actor=0)`；`card_window --dry-run` rc=0（含两侧 discover/probe/db/stop 全计划）。

### 4.4 `openmir2-weapon-durability-frames`（审计 #28，行 136）

- **W-1 `SM_LAMPCHANGEDURA(655)` 真机帧面；W-2 `SM_BREAKWEAPON(1102)` 真机 + 落库**
- **① 前置/夹具**：W-1 角色装备火把（`SpecialPwr==0`）站桩 ≥3s，部署 `[Setup] DecLampDura=1`；
  W-2 经升级 NPC `@getbackupgnow` 取得 `Desc[10]` 标记武器，有目标打一次 / 空挥各一次，部署 `UpgradeWeaponMaxPoint=7`；
  静止玩家 + ±12 观察者；**真机场景待建**（window-batch-plan 第 77 行「新：combat_hit + 装备耐久」；承接卡未指定）。
- **② 可复跑命令**：
  - ✅ `cargo test -p mir2-world weapon_durability_tests`
  - ✅ `cargo test -p game-svr --lib weapon_durability_downlink_tests`
  - 真机（**未跑**）：场景待建 + 承接卡 = 需 owner/coordinator 派场景卡（同 T-249..T-253 的登记口径）。
- **③ 期望**（T-199 原文）：655 **子集**两侧逐字节一致（条数；燃尽支 `Recog=0`；跨千位支 `Recog=Dura`；12B 头）；
  **654（`RM_CHANGELIGHT`）与 709（`SendDelItems`）本窗口显式不比对**（Rust 未实现，T-79⑥/T-95① 承接）；
  1102 帧面 + 收件人集合（±12 格边界，含自己），断后 `characters_item` 的 `Index`/`Dura`/`Desc` 逐字段相同。
- **④ 判据**：以上帧/字段 diff=0；构造级 13/13 + 4/4 全绿作为回归下限；任一缺件 = FAIL。
- **⑤ 不可比**：654/709（明写不比对）；655 相位（oracle 每对象 `DecLightItemDrugTick` vs Rust 世界级 `lamp_tick`，≤1 周期）
  ⇒ 按帧集合/相对次序比，不按绝对 tick；`VisibleHumanList` 500ms 缓存 ⇒ 静止玩家。
- **⑥ 负责人/状态**：owner `openmir2-worker-41`；本批 **构造级已跑通过（13/13 + 4/4, rc=0）**，**真机未跑**；blocked_by = 场景待建。
- **实跑证据**：world 13 passed / game-svr 4 passed。

### 4.5 `openmir2-encoding-core-e1-e2`（审计 #24，行 132）

- **W-1 真机非 ASCII 说词/物品名逐字节；W-2 `StringList` 与 `ConfigFile` 两条 BOM 路径**
- **① 前置/夹具**：W-1 `npc_click_auto`（或 `merchant_dialog`）打到 GBK 说词，比 643/767 帧体；
  W-2 **纯离线**，无需起栈（`crates/encoding` 向量 + `script`/`envir` 调用面）。
- **② 可复跑命令**：
  - ✅ `cargo test -p mir2-encoding`（BOM 两路 + cp936 向量）
  - ✅ `cargo test -p mir2-script --test cp936_encode`
  - ✅ `cargo test -p mir2-envir --test text_encoding`
  - ✅ `python -I -B tools/probe/mir2_probe.py --check-scenario --scenario tools/probe/scenarios/npc_click_auto.json`
  - ✅ `python -I -B tools/conformance/card_window.py --dry-run --scenario tools/probe/scenarios/npc_click_auto.json`
  - 真机 W-1（**未跑**）：`card_window.py --scenario …/npc_click_auto.json --out … --card openmir2-encoding-core-e1-e2`
- **③ 期望**：W-2 `StringList` = UTF-16LE、`ConfigFile` = UTF-32LE（preamble 决定），不可映射 → `3F`，
  非 BMP → `3F 3F`，绝不产出 `&#N;`；W-1 两侧 643/767 **body 字节相等**（GBK 原字节）。
- **④ 判据**：三条 cargo 命令 rc=0（8/8 + 7/7 + 4/4）即为 W-2 的机判通过；W-1 以 `report.json` 帧体字节 diff=0 判定。
- **⑤ 不可比**：客户端字形渲染/字体映射（GUI）；`&#N;` 字符串（两侧都不得出现）。
- **⑥ 负责人/状态**：owner `openmir2-orchestrator`；本批 **W-2 离线已跑通过（8/8 + 7/7 + 4/4, rc=0）**，**W-1 真机未跑**；
  **结论：W-2 不需要真机窗口，离线即可判；只有 W-1（线上非 ASCII 字节）需窗口**。
- **实跑证据**：`mir2-encoding` 8 passed；`cp936_encode` 7 passed；`text_encoding` 4 passed；`npc_click_auto` check rc=0、dry-run rc=0。

### 4.6 `openmir2-world-tick-expansion`（审计 #19，行 127）

- **W-1 RNG `site/order` 账本；W-2 `SM_DAYCHANGING(46)`；W-3 滚动公告节拍；W-4 Ghost 60s + `RunTime` 门**
- **① 前置/夹具**：`login_only.json` + `MIR2_RNG_LOG`（C# 侧 hook + `MIR2_RNG_SITE=1`）；
  公告文件 `Notice/LineNotice.txt`（7 行/832B）；小时边界需真机墙钟。
- **② 可复跑命令**：
  - ✅ `python -I -B tools/probe/mir2_probe.py --check-scenario --scenario tools/probe/scenarios/login_only.json`
  - ✅ `python -I -B tools/conformance/card_window.py --dry-run --scenario tools/probe/scenarios/login_only.json`
  - ✅ `cargo test -p mir2-world npc_tick`；✅ `cargo test -p mir2-world refresh_game_time`；✅ `cargo test -p game-svr line_notice`
  - 真机（**未跑**）：`MIR2_RNG_LOG=<path> MIR2_RNG_SEED=42` + `card_window.py --scenario …/login_only.json --out …`
- **③ 期望**：① 两侧账本按声明消费者序逐项 `site`/`order` 相等（npc_tick 次序 = `npc_tick_runs_in_slot_order` 的 slot 序）；
  ② 桶变化时**每个在线会话各一条** 46，`Series`=桶值（1/2/0/3）；③ 公告按 `LineNotice.txt` 顺序逐玩家滚行、
  `ShowLineNoticeTime` 节拍、`Idx` 回绕；④ 固定 NPC/商人 ghost **60s** 后移除，`RunTime` 门控制周期。
- **④ 判据**：账本 diff=0；46 帧数 = 在线会话数 × 桶变化；公告帧序/节拍符合；ghost 在 60s 边界消失。任一不符 = FAIL。
- **⑤ 不可比**：墙钟边界探测延迟 ≤150ms（T-66，oracle 50ms vs Rust 200ms）；跨 pass `NpcLimit` 交错（T-64）；
  `line_notice` 的 `<$…>` 变量替换未建模（T-67①）⇒ 显式不比对；颜色前缀剥离/`[`/`<`/`{` 出帧按 T-67 口径。
- **⑥ 负责人/状态**：owner `openmir2-coordinator`；本批 **offline check/dry-run + 构造级全跑通过（2/2 + 2/2 + 15/15）**，
  **真机未跑**；blocked_by = none（前置已就绪）。
- **实跑证据**：`login_only` check rc=0；dry-run rc=0；`npc_tick` 2 passed；`refresh_game_time` 2 passed；`line_notice` 15 passed。

### 4.7 `openmir2-mb7-monsters`（审计 #23，行 131）

- **W-1 race 113/114/134 的 Run/克隆/召唤（部署不可达时以构造级为准）；W-2 T-143 展示色与排水相位**
- **① 前置/夹具**：race 113/114/134（`ElfMonster`/`ElfWarriorMonster`）；`[Names] Dragon1=神兽`（部署 `Server.conf:130`）；
  `flush_sculture_spawns` 排水；**真机场景待建**：window-batch-plan 第 59 行「新：world_observe race 113/114/134」，
  承接卡未指定 ⇒ 需 owner/coordinator 派场景卡。
- **② 可复跑命令**：
  - ✅ `cargo test -p mir2-world mb7`
  - 真机（**未跑 / 场景待建 + 承接卡 = 待派场景卡**）
- **③ 期望**：`MakeClone` 继承与 appear-now、2s 后 ghost；变脸范围与 clone 帧（mb7_r16）；死 Elf 经 class run ghost；
  T-143 展示色；排水相位（oracle `RegenMonsterByName` 在 `Run` 内同步 vs Rust `flush_sculture_spawns` 在 tick 后）。
- **④ 判据**：构造级 6/6 全绿；真机按 race 113/114/134 的**帧集合**相等（不按绝对 tick）。
- **⑤ 不可比**：排水相位的绝对 tick（T-143；按帧集合比）；部署数据不可达 ⇒ 真机只能验构造面；
  展示色若部署无对应数据则显式标注「不可达，不比对」。
- **⑥ 负责人/状态**：owner `openmir2-orchestrator`；本批 **构造级已跑通过（6/6, rc=0）**；
  **真机部分判不了**（部署数据可达性 + 场景未建）；blocked_by = 场景卡未派。
- **实跑证据**：`cargo test -p mir2-world mb7` → `6 passed`，rc=0。

### 4.8 `openmir2-buff-system`（审计 #20，行 128）

- **W-1 三条怪癖；W-2 毒伤 DoT 流水；W-3 `CharStatus` 位序；W-4 魔法盾 MP + `LastHiter` 守卫**
- **① 前置/夹具**：`spell_cast.json` **已声明 BLOCKED**（缺 `characters_magic` 夹具：两侧测试库各加
  `(PlayerId=6228, MagicId=1, Level=0, UseKey='1', CurrTrain=0)`；建议新卡 `openmir2-window-magic-fixture`）；
  且 W-1 的「递减 50ms vs 1s 两读法」**待窗口 A-R2 裁定**。`characters_magic` 夹具本身已在 `512d9ecf5`；
  场景封口由卡 `openmir2-spell-cast-unblock` 处理（head `923a98a5c` **待审**，本文件不预设合并结果）。
- **② 可复跑命令**：
  - ✅ `python -I -B tools/probe/mir2_probe.py --check-scenario --scenario tools/probe/scenarios/spell_cast.json`（形状合法 rc=0）
  - ✅ `python -I -B tools/conformance/card_window.py --dry-run --scenario tools/probe/scenarios/spell_cast.json` → **rc=2 BLOCKED**
  - ✅ `cargo test -p mir2-world buff::tests`
  - 真机（**不可执行**）：场景封存，**不写可跑命令**。
- **③ 期望**：① `break` 跳 `for(j)` 的 SysMsg 与递减、怪物 buff 永不到期（两读法待 A-R2）；② 毒伤 DoT 逐次 `MIR2_RNG_LOG`；
  ③ 走/跑/转向/受击/死亡帧尾 `Status` 位序跨帧一致；④ 魔法盾 MP 抵扣与 `LastHiter` 守卫分支。
- **④ 判据**：**判不了**——① 判据未定（A-R2 未裁决）、场景封存（dry-run rc=2 BLOCKED）；只有构造级可判。
- **⑤ 不可比**：A-R2 未裁决前不比对递减节奏；缺 `characters_magic` 前任何施法帧（637/638/639/17）都不比。
- **⑥ 负责人/状态**：owner `openmir2-coordinator`；本批 **构造级已跑通过（16/16, rc=0）**；
  **窗口判据判不了**（判据未定 + 夹具缺失）；blocked_by = 窗口 A-R2 裁定 + `openmir2-window-magic-fixture`。
- **实跑证据**：check rc=0；dry-run `VERDICT: BLOCKED … exit=2`；`buff::tests` 16 passed。

### 4.9 `openmir2-castle-guard-target`（审计 #27，行 135）

- **W-1 同城/盟友/UnderWar 的守卫目标选择；W-2 `Castle!=null` 受害者 `AnimalObject.Struck` 合法性**
- **① 前置/夹具**：城堡归属/同盟/UnderWar 三态（真机）；W-2 归 T-96⑥④「另开卡」。
  **真机场景待建**：window-batch-plan 第 53 行「新：R6 castle_zone」，承接卡未指定。
- **② 可复跑命令**：
  - ✅ `cargo test -p mir2-world proper_target`
  - ✅ `cargo test -p mir2-world animal_struck_proper_target_gate`
  - 真机（**未跑 / 场景待建 + 承接卡 = R6 castle_zone 场景卡（待派）+ W-2 的 T-96⑥④ 卡**）
- **③ 期望**：同城/盟友/UnderWar 三态下守卫选中/不选中的目标集合；`Castle!=null` 受害者经 `AnimalObject.Struck`
  的合法性判定与 oracle 一致。
- **④ 判据**：构造级 `proper_target` 5/5 + `animal_struck_proper_target_gate` 1/1 全绿；真机按三态目标集合 diff=0。
- **⑤ 不可比**：W-2 另开卡未就绪 ⇒ 显式不比对；城堡初始化/归属布置须与 oracle 同源（否则目标集合无意义）。
- **⑥ 负责人/状态**：owner `openmir2-orchestrator`；本批 **构造级已跑通过（5/5 + 1/1 + 1/1, rc=0）**；
  **真机部分判不了**（场景未建 + W-2 前置未就绪）；blocked_by = R6 castle_zone 场景 + T-96⑥④。
- **实跑证据**：`proper_target` 5 passed（+ 另 1 passed）；`animal_struck_proper_target_gate` 1 passed；rc=0。

### 4.10 `openmir2-outbound-encoders-red-batch`（审计 #17，行 125）

- **W-1 时序/RNG 真机对拍；W-2 654 入队相对序；W-3 制药链抽数/帧序/`MYSTATUS` 落库**
- **① 前置/夹具**：W-2/W-3 需两侧同场景真机；`upgrade_retrieve.json`（R2）覆盖 202/200 与 657 部分；
  **制药链（`Merchant.ClientMakeDrugItem`）无场景** ⇒ 场景待建（承接卡未指定，需 owner/coordinator 派场景卡）。
- **② 可复跑命令**：
  - ✅ `python -I -B tools/probe/mir2_probe.py --check-scenario --scenario tools/probe/scenarios/upgrade_retrieve.json --var merchant_actor=0`
  - ✅ `python -I -B tools/conformance/card_window.py --dry-run --scenario tools/probe/scenarios/upgrade_retrieve.json`
  - ✅ `cargo test -p game-svr cm_make_drug_gates`
  - 真机（**部分未跑**）：654/202/200 借 `card_window --scenario …/upgrade_retrieve.json`；制药链场景待建。
- **③ 期望**：654 入队相对序（两侧同序列）；制药链 `Random` 次第、`SM_USERMAKEDRUGITEM`/`ADDITEM`/`DELITEMS`
  与结果帧的相对序、`MYSTATUS` 派生字段落库前后一致（字段级）。
- **④ 判据**：帧相对序表逐项相等 + RNG trace 逐抽一致 + DB 字段级 diff=0；制药链在场景落地前**判不了**。
- **⑤ 不可比**：制药链无场景 ⇒ 本窗口不覆盖（显式）；C2S 的 MakeIndex/session 变量；墙钟。
- **⑥ 负责人/状态**：owner `openmir2-coordinator`；本批 **654/202/200 部分离线可跑通（dry-run rc=0 + drug gates 1/1）**；
  **制药链真机判不了**（场景待建）；blocked_by = 制药链场景卡未派。
- **实跑证据**：check（`--var merchant_actor=0`）rc=0；dry-run rc=0；`cm_make_drug_gates` 1 passed。

## 5. 汇总表（10 张）

| 卡 | 判据形态 | 今日实跑 | 结论 |
|---|---|---|---|
| m3-rng-scene-driver | 账本逐项 + 5 列/seq 连续 + 场景门 | SelfCheck 23/23 rc=0 | 离线可判；真机待 real wiring |
| m3-drop-exp-wiring | 100 行 raw + 多重集/金币/经验 diff + 纯度门 | SelfCheck 21/21 rc=0 | 离线可判；真机待 pin 单怪 |
| weapon-upgrade-values | 四字段 + 657 + 202/200 + RNG 逐抽 | check/dry-run rc=0 | 场景可跑；真机未跑 |
| weapon-durability-frames | 655 子集逐字节 + 1102 + DB 字段 | cargo 13/13 + 4/4 | 构造级可判；真机场景待建 |
| encoding-core-e1-e2 | BOM 两路 + 非 ASCII 字节 | cargo 8/8 + 7/7 + 4/4 | **W-2 不需窗口**；W-1 未跑 |
| world-tick-expansion | 账本 + 46 + 公告节拍 + ghost 60s | check/dry-run rc=0；cargo 2/2+2/2+15/15 | 离线/构造级可判；真机未跑 |
| mb7-monsters | race 113/114/134 帧集合 + T-143 | cargo 6/6 | 构造级可判；**真机判不了**（数据不可达/场景待建） |
| buff-system | 三怪癖/DoT/位序/魔法盾 | check rc=0；dry-run **rc=2 BLOCKED**；cargo 16/16 | **判不了**（A-R2 未定 + 夹具缺） |
| castle-guard-target | 三态目标集合 + Struck 合法性 | cargo 5/5+1/1+1/1 | 构造级可判；**真机判不了**（场景待建 + W-2 另卡） |
| outbound-encoders-red-batch | 654 相对序 + 制药链流水/帧序/落库 | dry-run rc=0；cargo 1/1 | 部分可判；**制药链判不了**（场景待建） |

**计数（互斥分桶，总和 = 10）**：

- **完全可机判 6 张**（判据文本齐备，剩下只是排窗口/离线执行）：`m3-rng-scene-driver`、`m3-drop-exp-wiring`、
  `weapon-upgrade-values`、`weapon-durability-frames`、`encoding-core-e1-e2`、`world-tick-expansion`；
- **部分可机判 3 张**（构造级可判、真机面判不了）：`mb7-monsters`（部署数据不可达 + 场景待建）、
  `castle-guard-target`（场景待建 + W-2 另卡）、`outbound-encoders-red-batch`（制药链无场景；654/202/200 部分可判）；
- **判不了 1 张**：`buff-system`（窗口 A-R2 未裁 + 场景 BLOCKED，只有构造级 16/16）。

> 上一版把 mb7/castle-guard 的「真机半」重复计入最后两档、且 encoding 的 W-1/W-2 跨档（review R3），已改为互斥分桶；
> 「判不了也算完成」的口径不变。mb7/castle-guard/outbound 的真机残缺只在「部分可机判」一档计入一次。

## 6. 门禁前后对照

| 项 | 改前 | 改后 |
|---|---|---|
| `t_reservation_gate` | GREEN（rows=250, whitelist=250, max=T-264） | 未改 whitelist/预约表 ⇒ 同值 |
| `whitelist_anchor_gate` | ALL PASS（548 条锚） | 未改 whitelist ⇒ 同值 |
| `conflict_marker_gate` | OK（0 markers / 7426 tracked files，`5a6ed54c1`） | **OK（0 markers / 7430 tracked files，返工 head）**（base 7426 + master `19009c6a0` 带入 3 个文件 + 本 patch 的 1 个 docs 文件 = 7430；`git ls-tree -r --name-only` 实测） |
| `goal_gate --json` 条件② | `state=need_human`；r_cards **194**：`window_required=116` / `window_required_unmerged=5` / `whitelist_t=48` / `closed=11` / `not_in_board=5` / `merged_not_closed=5` / `adjudicated=3` / `weak_t=1` | **`state=need_human`；r_cards 194：同上 8 类计数逐一相同；逐卡 category diff 为空集**（`index_entries` 因本批新增 comment 增长，不影响分类；改前基线取合并提交 `d971f318e`，已含并行卡合并） |

> 返工 r2 实测（2026-10-24）：`t_reservation_gate` rc=0 GREEN；`conflict_marker_gate` rc=0；
> `whitelist_anchor_gate` rc=0 `ALL PASS（548 条锚）`；`goal_gate --json` 快照
> `E:/tmp/worker69-backfill-r2/gg-after.json` 的 `conditions[id=2].state` 仍为 `need_human`，`r_cards` 的
> `{card: category}` 映射与改前**逐项相等**（diff = `{}`）。
> 改前基线快照：`E:/tmp/worker69-backfill-r2/gg-before.json`（取合并提交 `d971f318e`，`generated_ts` 见文件；
> 本批 r2 新增的 10 条 ⑥ 补登 comment 只增长 `index_entries`，不改变任何卡的 `category`）。
> 本批**未改 `goal_gate` 判据、未改 whitelist、未改任何卡片 status**。

## 7. 残留物清单

- 仓库内：仅本文件 `docs/window-criteria-backfill-20261024.md`。
- 仓库外（`E:/tmp/worker69-backfill-r2/`，本线专属，交付后可删）：`gg-before.json`、`gg-after.json`、`cat-before.json`、`cat-after.json`、门禁输出。
- 未起任何服务端/客户端/假人/代理进程；未占端口；未改 `E:/MirServer`。

## 8. 返工对照（review `ef710b00f`，request_changes）

| # | 处置 | 落点 / 证据 |
|---|---|---|
| R1 | `login-529` 不再是「不可能达成」：改判 **A 组、判据已可机判**，因本轮名额 10 张已满而顺延；删除「SS_SERVERINFO 未接 ⇒ 状态段恒 0」。 | 本文 §2 未取表；证据：`git merge-base --is-ancestor 185a6c8cb HEAD` → YES；`sed -n '17,20p' mir2-rs/bins/game-svr/src/ids.rs` →「**上报**：SS_SERVERINFO … 由两处产生」；`mir2_probe --check-scenario --scenario tools/probe/scenarios/login_only.json` → rc=0 `scenario OK: login_only (1 steps)`。10 条 comment 内无同一表述（逐条 grep 0 命中）。上游审计 §5.2 #26 仍带旧分类，属另一卡文件，未改，建议下一批重分类。 |
| R2 | 10 张卡各补一条签名 comment，`items[].status` 写 `owner=<principal>；<状态>` 并新增显式 `owner` 字段；owner 取自该卡最近一条 `status` 的 `owner`（非本卡作者）。 | 10 条 r2 comment 全部 `rc=0`、`verified=true`；oid 逐条见 §4 开头 oid 表的「r2 补 ⑥」列。 |
| R3 | 汇总计数改**互斥分桶**：完全可机判 6 + 部分可机判 3 + 判不了 1 = **10**。 | 本文 §5「计数（互斥分桶）」 |
| R4 | tracked files 按返工 head 实测写 **7430**（base 7426 + master 合并 3 + 本 patch 1）；goal_gate 六类改为实测 **8 类 194 张**（改前基线 = 合并提交 `d971f318e`）。 | 本文 §6；`git ls-tree -r --name-only <head> | wc -l`；`tools/hygiene/conflict_marker_gate.py` |
| R5 | A 组定义改为「离线载体（场景/RunPair/构造级 cargo）且今日 rc=0」，blocked 场景只算构造级；10 张逐张标载体类型；`drop-exp-wiring` 注明 batch-plan 对应行是 `m3-drop-exp-align`。 | 本文 §2 规则 + 表 + 「载体纠偏」注 |

> batch-plan 漂移（review「另发现」）：`docs/window-batch-plan-20261023.md` §7.1/§7.2/§7.3 的 `spell_cast` 行改为
> 「夹具已在 `512d9ecf5`；场景封口由卡 `openmir2-spell-cast-unblock` 处理（head `923a98a5c` 待审）」，
> **不预设合并结果**；未改 `tools/probe/**`。
