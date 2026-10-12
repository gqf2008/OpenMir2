# M3②③ 攻击 `AttackSuccess` 调用条件 + 每击取数请求序列定位 —— 证据包（2026-10-12，m3cs / openmir2-worker-5）

卡：`openmir2-m3-combat-skills-align`（issue `a2ef74aa`）· worktree `m3cs` · branch `worktree-m3-combat-skills`
· lane `m3cs` / principal `openmir2-worker-5`。

本片回答 owner 2026-10-12 **22:35** brief 顶部第 ①–④ 条（**离线段**：零端口、零栈、零 `.rs`）。
数据源是上一步已入库的两相真机原始件（`evidence-rng-isolate-attack-streamcompare-20261012/`），
本包把四条流水复制进 `provenance/` 以自洽可复算（sha256 见 `provenance/00-provenance.txt`）。

## 结论（逐条对 owner 22:35）

1. **① `AttackSuccess` 的调用条件 = 「有目标」**（`06-oracle-attack-success.md`）：
   oracle `PlayObject.AttackDir`（`PlayObject.Attack.cs:24`）在 `:77` **无条件调用** `AttackSuccess`；
   `AttackSuccess`（`:215`）在 `:217-220` 对 `targetObject == null` 提前返回，`:225` 的 `Random(5)` 只要
   **目标非空即恰 1 抽**（不分命中/躲闪）。Rust `combat.rs:830/848` 的 `attack_dir` 与 oracle **同形**
   （玩家攻方 + 有目标 ⇒ `attack_with` 后恰 1 抽 `attack_success_weapon_damage`；无目标走
   `World::attack_swing`，不抽 `succ`）。⇒ **Rust 不是无条件调，不是实现缺陷。**
2. **② C# `hit_gate 19 → attack_success 3` 是把 base 单元算进了玩家门**（`08-base-unit-attribution.txt`）：
   19 次门里 **16 次属非玩家攻方**（`BaseObject.GetAttackPower@BaseObject.Base.cs:892` 的 16 个 base 单元），
   **玩家门只有 3 次，恰等于 `attack_success`（3）**。两侧 `AttackSuccess` 调用关系同形、无多/漏调。
   真正不同的是**「玩家攻方有目标的击数」：C# 3/120 vs Rust 38/120**（`05-target-availability.txt`：
   C# 目标只在 k=1..3 存在，Rust k=1..38 存在到目标 HP 归零）。
   停点在**目标解析** `IActor attackTarget = targetObject ?? GetPoseCreate();`（`PlayObject.Attack.cs:26`
   → `BaseObject.cs:1253` → `Envirnoment.cs:1203` 的 `!Ghost && (!boFlag || !Death)`）：k>=4 目标格为空 ⇒
   `_Attack`（`BaseObject.Attack.cs:32`）与 `AttackSuccess`（`:217`）都提前返回 ⇒ 无门、无 `succ`。
   不是拒绝门（120 条 `CM_HIT` 全部进 `AttackDir`、`atk_power_a/b` 各 120），不是目标死亡
   （C# `death_k=None`、`min_hp=18`），不是掉线（RunGate 全程只到最后 21:57:44 才两连接断开）。
3. **③ 第一处请求分歧已定位**（`07-first-divergence.txt`）：`(kind,arg)` 逐次比分到 **1-based index=2**：
   C# 是**非玩家 base 单元**的门 `hit_gate(Next(max),8)`，Rust 是玩家单元的 `atk_power_b(Next(max),10)`
   —— **class = base_unit**。触发条件：C# 流首是非玩家 `atk_power_base(Next(max),1)`，其 `(kind,arg)` 与
   Rust 首个 `atk_power_a(Next(max),1)` **完全相同**（都是 `Random(1)`）⇒ 第 1 次不比出差异，第 2 次即因
   base 单元的 `hit_gate` 显形，此后整体推移。**玩家路径本身无分歧**：120 个 player 单元的取数模板
   两侧均 0 违规（`02/03-analyze-*`）。
   - 机判工具：`per_strike_requests.py`（`selftest` 16/16；玩家路径多抽 / 次序颠倒 / 单元数不等**必红**；
     base 单元分类为 `base_unit`，不误判为 `player_path`）。同文件的 `jointable` 子命令把本表接到夹具
     **逐击状态记录**上（见下节）。
4. **④ 因果登记（不修）**：目标 maxhp=19（`-AttackVictimHp 30000` 被 `RecalcLevelAbilitys`
   `PlayObject.cs:5295` 按 L1 重算）确实把「有目标窗口」压短（Rust 在 k=38 目标 HP 归零后即无目标），
   但**不是** C# 停在 k=4 的直接成因（C# 目标全程 `min_hp=18`、未死）。C# 为何在 k=4 起目标格为空，
   本片只能把停机点钉在**目标解析**（上面第 2 条），**其成因属夹具/场景**（建议夹具逐 k 记 `victim` 的
   `CurrX/CurrY + Ghost/Death`）——**转 m3bag**。④ 本体（pin maxhp 未生效）仍归 m3bag。

**判据口径未动、未放宽**：DoD② 仍是真机 `attack_diff.py diff` 逐 k=0；本片**不宣布达成**。

## jointable：逐击请求序列 × 夹具逐击状态记录（owner 2026-10-12 23:50 第 ① 条）

owner 已把「查清 C# 目标格 (18,20) 从 k=4 起为空」转 m3bag（加**逐击状态记录**）。本片把本工具的
逐击请求序列接到那份记录上——**两侧同表**（长表：一行 = 一侧的一击，列两侧完全同名）：

    python -I per_strike_requests.py jointable \
      --csharp-stream <csharp-attack-stream.tsv> --rust-stream <rust-rng-stream.tsv> \
      --csharp-seq <attack_seq.jsonl> --rust-seq <attack_seq.jsonl> \
      ( --csharp-state <attack_state.jsonl> --rust-state <attack_state.jsonl> \
        | --derive-state --csharp-raw <attack_raw.jsonl> --rust-raw <attack_raw.jsonl> )

列：`side k state hit rng dmg reqs atk_{x,y,dir,ghost,death,hp} vic vic_{x,y,dir,ghost,death,hp}`。
`hit` = `attack_seq.jsonl`（SM_STRUCK 观察）；`rng` = 该击的取数模板（有无 `hit_struck_damage`）；
`state` = `fixture`（权威夹具记录）或 `derived`（预览）。

**机判（fail-closed）**：状态记录必须覆盖 RNG 流水切出的**每一个**玩家单元且 `k` 无洞 ⇒ 否则 RED；
缺字段 / 缺键 / 非法 JSON ⇒ **PREREQ 点名 k+字段**；`victim:null` 合法（目标格空，正是要看的东西）。
`hit` 与 `rng` 不一致时打 `info:`（不判红）——本次真机 C# `k=3` 就命中，是会话件跨观察者重复计数
（`attack_raw.jsonl` 里同一次 `SM_STRUCK` 被 attacker/victim 两个客户端各记一行，`attack_diff.py align`
因此把 k=3 算成 hit 而 RNG 只有一次玩家命中）⇒ 已登记转 m3bag，**不是**本工具或判据的缺口。

### 需要 m3bag 夹具产出的接口（owner 转）

每侧一个 JSONL（建议落在会话目录 `attack_state.jsonl`，或往 `attack_raw.jsonl` 追加 `role=state` 行），
一行一击，`k` 从 1 起、与 `M3ATTSEQ role=attack` 的 `k` **一一对应**（120 击 ⇒ 120 行）：

```json
{"role":"state","k":1,
 "attacker":{"x":17,"y":20,"dir":2,"ghost":0,"death":0,"hp":80},
 "victim":{"x":18,"y":20,"dir":6,"ghost":0,"death":0,"hp":18}}
```

* 6 个字段必须齐全（缺一 ⇒ 本工具 PREREQ 点名 `k`+字段）；无目标写 `"victim":null`。
* 快照时刻 = oracle `PlayObject.AttackDir` 取 `targetObject ?? GetPoseCreate()` **之前**（该击入 `AttackDir` 那一刻）的双方状态。
* 两侧同 schema（C# 与 Rust 都产）；攻方/受害键别名 `attacker|atk`、`victim|vic`（本工具容错读）。

夹具记录就位前，`--derive-state` 从既有 `attack_raw.jsonl` 反推可得子集（攻方 `x/y/dir` ← `role=attack`；
受害 `present/hp` ← `role=struck`；其余 `-`），列内 `state=derived` 明示——**不替代**权威记录。

## SERVER_STATE：jointable 直接吃**服务端权威 dump**（owner 2026-10-13 01:05 第 ② 条）

**背景**：m3bag 已定位「客户端夹具记录**看不到 victim 的服务端 `Ghost`**」（受害被 ghost 后观察者仍保留
actor）⇒ 目标可用性只能由**服务端权威 dump** 咬住。C# 影子 hook（`StartupHook.cs::DumpActors`）与
m2-svr 待审片（`bins/game-svr/src/state_log.rs`）**两侧同形**：每行
`SERVER_STATE <seq> <json 数组>`，一**玩家攻击单元**一行，数组元素 = 全部玩家族对象的
`{id,name,x,y,dir,ghost,death,hp,maxhp,map}`。

⇒ 本工具 `jointable` 新增侧别参数，**两侧都能吃**（C#/Rust 走同一解析器）：

    python -I per_strike_requests.py jointable \
      --csharp-stream <csharp-attack-stream.tsv> --rust-stream <rust-rng-stream.tsv> \
      --csharp-server-state <SERVER_STATE.tsv> --rust-server-state <SERVER_STATE.tsv> \
      --csharp-attacker-id <id> --rust-attacker-id <id> --victim-x 18 --victim-y 20

口径（与夹具侧 `server_state_gate.py` 一致）：

* 每侧取该 dump 的**最后 N 行**（N = 本侧 RNG 流水切出的**玩家攻击单元数**）；
* `attacker` = 该行 `id == --<side>-attacker-id` 的玩家族对象（`attack_raw.jsonl` 的 `role=attack.my`）；
* `victim` = 该行站在 `(--victim-x,--victim-y)` 的**另一个**玩家族对象（**不筛 `ghost/death`**——
  正是要列出来看见的那一列 `vic_ghost`/`vic_death`；`victim:null` = 该格无人）；
* 两侧同时给 `--*-server-state` 与 `--*-state` ⇒ 额外记 `info:client_vs_server_state_diff`
  （客户端说在场且 `ghost=0`，服务端同 k `ghost=1` ⇒ 点名 `k:ghost(client=…,server=…)`，不判红）。

**fail-closed（真机件驱动，原始件 `18-serverstate-failclosed.txt`）**：dump 行数 < 玩家单元数 /
行非法 JSON / 找不到 attacker id ⇒ **PREREQ rc=2**；给了 `--*-server-state` 却缺选择参数
（`attacker-id`/`victim-x`/`victim-y`）⇒ PREREQ（不猜）。**绝不静默出表。**

**验证（非合成）**：

* C# 真实产出：`sessions/csharp-20261013-005038/server_state_segment.tsv`（120 行）+ cA2 流水 ⇒
  `jointable` **GREEN**，`state=server`，120 行 `vic_ghost=0`（`15-serverstate-jointable-csharp-real.txt`）。
* Rust 真实产出：m2-svr 待审片 `4ba36368` 的 `rust-server-state.tsv` 原样存 `fixtures/`，本解析器
  对其直出 5 个 `k` 的 attacker/victim（`16-serverstate-rust-sample-parse.txt`）⇒ **Rust 侧 dump 合入后
  两侧都能填**（不需要再改本工具）。
* 牙齿：`selftest` **24/24**（新增 SERVER_STATE 两侧 / ghost 列 / 客户端 vs 服务端 / 缺选择器/短/缺 id/坏 JSON）。

## 文件

| 文件 | 内容 |
|---|---|
| `01-selftest.txt` | `per_strike_requests.py selftest`（原 16/16 牙齿；SERVER_STATE 后 **24/24**） |
| `02-analyze-stack2.txt` / `.json` | cA(120) vs rA(120)：站点计数 / player 单元模板 / 第一分歧 class=base_unit |
| `03-analyze-stack1.txt` / `.json` | 交叉印证（含 20 冒烟 + 两次 120；`--allow-count-mismatch` 因两侧会话构成不同） |
| `04-per-strike-table-stack2.txt` | 120 击逐 k 的**每击取数请求序列**（C#/Rust 并排，带 hit/dodge） |
| `05-target-availability.txt` | ② 的读数：C#/Rust 有目标的击号、目标消失首击、death_k/min_hp、struck/ERR、gate 断开 |
| `06-oracle-attack-success.md` | ① 的 oracle 逐行 + Rust 同形核对 + 「19→3」正确读法 |
| `07-first-divergence.txt` | ③ 的第一处分歧站点 + 触发条件 |
| `08-base-unit-attribution.txt` | ② `hit_gate=19 = base 16 + player 3` 的逐单元归属 |
| `09-gates.txt` | 四门禁（fmt 0 / clippy 0 / 84 suites 682 passed 0 failed 16 ignored / msgcodegen 604） |
| `10-i1-twophase-verify.txt` | I-1 返工原始件：merge master `4408868d` 后 twophase `GREEN 75/75`、streamcompare `GREEN 31/31`（改前/改后写清） |
| `11-jointable-twophase-derived.txt` | `jointable --derive-state` 真机预览（两侧同表 240 行）：C# 受害 `present` 只在 k=2,3、Rust 到 k=38；C# `k=3` 的 `hit≠rng`（观察者重复计数，登记） |
| `12-jointable-prereq-failclosed.txt` | 缺状态记录且未 `--derive-state` ⇒ `PREREQ`（rc=2）原始件：**绝不静默出表** |
| `13-gates-jointable.txt` | 本片四门禁（零 .rs；`CARGO_TARGET_DIR=E:/tmp/m3cs-r2-target`）：fmt 0 / clippy 0 / 84 suites 682 passed 0 failed 16 ignored / msgcodegen 604 |
| `15-serverstate-jointable-csharp-real.txt` | owner 01:05 ②：`jointable` 吃 C# 真实 `SERVER_STATE`（120 行）⇒ GREEN（`state=server`） |
| `16-serverstate-rust-sample-parse.txt` | owner 01:05 ②：本解析器对**Rust 真实产出**（m2-svr `rust-server-state.tsv`）直出 5 个 k |
| `17-serverstate-selftest.txt` | SERVER_STATE 后 `selftest` **24/24** 原始件 |
| `18-serverstate-failclosed.txt` | 真机件驱动的 fail-closed：短 dump / 缺 attacker id / 缺选择器 ⇒ PREREQ rc=2 |
| `19-serverstate-gates.txt` | 本片四门禁（零 .rs）：fmt 0 / clippy 0 / 84 suites 682 passed 0 failed 16 ignored / 604 |
| `20-e1-verify-green.txt` | owner 01:15 E-1 返工：`window/00-selfcheck-nostack.txt` 清单按仓库字节重记 ⇒ verify **RED 81/82 → GREEN 82/82** |
| `21-cleancheck-verify.txt` | E-1 的**干净检出**复核（`git worktree add --detach`）：两包 verify GREEN 82/82 + 32/32、selftest 24/24 |
| `fixtures/rust-server-state-m2svr-sample.tsv` | m2-svr 待审片 `4ba36368` 的 Rust `SERVER_STATE` 原样样本（5 行） |
| `per_strike_requests.py` | ③/① 的机判工具（`analyze` / `table` / `jointable` / `selftest`） |
| `session_target_availability.py` | ② 的会话件读数工具 |
| `provenance/` | 四条流水副本（C# 专用流 ×2、Rust trace ×2）+ 溯源 |
| `commands.txt` | 全部命令 |

## 溯源

- 四条流水 sha256/字节见 `provenance/00-provenance.txt`（与
  `evidence-rng-isolate-attack-streamcompare-20261012/provenance/` 同源同值）。
- `SHA256SUMS.txt` 按**仓库字节**（LF 归一）口径；`verify_sha256sums.py --check-extra` GREEN 21/21。

## I-1 返工（owner 23:30）

reviewer-5 对①–④离线段判 `request_changes` 的 **I-1**：twophase 包清单记了**原始 CRLF 字节**哈希
`a9ca0b7c…` 而仓库口径（LF 归一）实算 `68593747…` ⇒ `RED 74/75`。本片按 owner 裁决 (a)：`merge master`
（`4408868d`，含该 CRLF→LF 修复）后 `window/00` blob 已 LF（`== 4408868d`），重算清单 ⇒ **GREEN 75/75**
（原件 `10-i1-twophase-verify.txt`）。**M-1**：twophase 包的 hash 判据**依赖 `4408868d` 的 CRLF→LF 修复**（现已在 base 内）。

## 残留物 / 转出

- 本片**未起栈、未占端口**，无需「窗口已释放」；端口实拍沿用上一步
  `evidence-rng-isolate-attack-twophase-20261012/window/19-ports-after.txt`（十端口 free）。
- 转 m3bag：④（pin maxhp 未生效）+ **C# 目标在 k=4 消失的场景成因**（建议夹具逐 k 记 victim 位置/状态）。
- 转 m3bag：C# 会话件里同一次 `SM_STRUCK` 被 attacker/victim 两个客户端各记一行（`other_observer=1`），
  `attack_diff.py align` 的 `hits` 可能因此多算（cA `hits=2` 而 RNG 只有 1 次玩家命中）。
