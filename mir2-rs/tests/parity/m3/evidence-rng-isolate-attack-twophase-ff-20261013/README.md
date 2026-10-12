# M3②③ 活栈隔离口径两相实跑（F-F 后）+ 请求序列逐段定位 —— 证据包（2026-10-13, m3cs / openmir2-worker-5）

卡：`openmir2-m3-combat-skills-align`（issue `a2ef74aa`）· worktree `m3cs` · branch `worktree-m3-combat-skills`
· lane `m3cs` / principal `openmir2-worker-5`。

## 三条正面结论（复读区；owner 2026-10-13 01:05 第 ① 条，供后来者复用）

1. **F-F 生效**：受害 pin 自洽的 L255（`Hp=MaxHP=20669`）后不再被静默 ghost ⇒ 两侧「玩家有目标击数」
   **120/120**（对比修复前 C# 3/120 vs Rust 38/120）；目标可用性门真机 `120/120 !ghost && !death`。
   见 §3、`window/12`、`window/21`、`requests/40-analyze-clean.txt`。
2. **Rust 逐 k 可复现**：两条独立新栈干净首跑 `diff = 0`、`rng-stream.tsv` 逐字节相同（`cmp` 无输出）。
   见 §4、`diffs/32`。
3. **C# 不可复现：专用流按调用点共享（F-N1）**——**事实**：C# 影子 hook 的隔离按**调用点**分流、
   进程级共享一条 `Random(attack_seed)`，所以流里混进了 **11 次 `atk_power_base`（base unit）** 抽取，
   第一分歧 `index=0, class=base_unit`。**假设**（m3bag 正在查证）：这 11 次来自**并发非玩家攻方**
   （同进程其它地图的怪物/守卫）——本片只能证「多出 base_unit 取数 ⇒ 序列分岔」，不能单独证「并发」。
   见 §5、`requests/40`。⇒ 结构已对齐，分歧在夹具共享流；DoD② 未达成、未放宽（F-N1 已转 m3bag）。

追加（owner 01:05 第 ② 条）：`attack-request-sequence/per_strike_requests.py jointable` 已能直接吃
**两侧 `SERVER_STATE` 服务端 dump**（C# 真实件 GREEN；Rust 用 m2-svr 待审片真实样件直出）——
即 m2-svr 的 Rust dump 合入后 **jointable 两侧都能填**（详见该目录 README 的「SERVER_STATE」节）。

## 0. 口径

- **图/场景**：`0137`（竞技场，零刷怪 / 零 AI 消费者，`source_verified=True`）、`-AttackPinPair`、`-Bots 2`、
  attacker `(17,20) dir=2`、victim `(18,20)`、victim pin = **F-F L255/战士 ⇒ Hp/MaxHP=20669**（master 已含）。
- **种子**：两侧 `MIR2_RNG_SEED=42` ⇒ 专用流 `attack_seed=40545`（`42 + 0x9E37`）。
- **隔离**：Rust `World.rng_attack`（`MIR2_RNG_ISOLATE_ATTACK=1`）/ C# 影子 hook 按调用点分流
  （`start-csharp-hook.ps1 -IsolateAttack`）。
- **判据（DoD②，未放宽）**：`attack_diff.py diff <csharp seq> <rust seq>` **逐 k = 0**。
- **合并基线**：master = `0f4e141b`（含 m3bag owner 23:50 夹具：逐击状态记录 + 目标可用性门）。
  本包后半段（c1/c2/r1/cA2/rA2）即在合入 `0f4e141b` 后的树上跑。

## 1. 会话一览

| 会话 | 侧/栈 | 用途 | attacked | hits | dodges | sum |
|---|---|---|---|---|---|---|
| `csharp-20261013-003132` | C# (fixture) | 20 冒烟 | 20 | 11 | 9 | 609 |
| `csharp-20261013-004311` | C# (fixture) | **120 c1（同栈）** | 120 | 67 | 53 | 3055 |
| `csharp-20261013-004504` | C# (fixture) | **120 c2（同栈）** | 120 | 66 | 54 | 3241 |
| `rust-20261013-004711` | Rust | 20 冒烟 | 20 | 12 | 8 | 497 |
| `rust-20261013-004811` | Rust | **120 r1（-PeerSession c1）** | 120 | 55 | 65 | 2327 |
| `csharp-20261013-005038` | C# (fixture, 新栈) | **120 cA2（单跑，无冒烟）** | 120 | 73 | 47 | 3684 |
| `rust-20261013-005240` | Rust (新栈) | **120 rA2（单跑，无冒烟）** | 120 | 57 | 63 | 2425 |
| `csharp-20261013-001016` | C# (旧脚本, 新栈) | **120 cA（干净首跑）** | 120 | 73 | 47 | 3813 |
| `csharp-20261013-001225` | C# (旧脚本, 新栈) | **120 cB（干净首跑）** | 120 | 70 | 50 | 3601 |
| `rust-20261013-001443` | Rust (新栈) | **120 rA（干净首跑）** | 120 | 57 | 63 | 2425 |
| `rust-20261013-001647` | Rust (新栈) | **120 rB（干净首跑）** | 120 | 57 | 63 | 2425 |

两侧 `attack_isolation.json`：`isolate_attack=true`、`attack_seed=40545`；C# 流水头
`ISOLATE_ATTACK 40545`、Rust 流水头 `ISOLATE_ATTACK 40545`；`ISOLATE_PREDICATES_OK table-vs-csharp GREEN`。

## 2. 判据结果（DoD② 未达成、未放宽）

| 对比 | 侧 | 差异项 | 结论 |
|---|---|---|---|
| `rA vs rB` | Rust 两新栈干净首跑 | **0** | **GREEN：Rust 侧逐 k 完全可复现** |
| `rA vs rB` 原始流 `cmp` | Rust | 无输出 | 两条 Rust 专用流水**逐字节相同** |
| `cA vs cB` | C# 两新栈干净首跑 | **242** | C# **不可复现** |
| `c1 vs c2` | C# **同栈**两次 120 | **328** | C# 同栈**不可复现** |
| `cA2 vs rA2` | C#/Rust 干净首跑 | **311** | 跨侧逐 k **≠ 0** |
| `c1 vs r1` | C#/Rust（含冒烟） | **334** | 跨侧逐 k **≠ 0** |

⇒ **DoD② 未达成**；本包**不宣布达成、不放宽判据**（差异原始件在 `diffs/`）。

## 3. 正面结论①：F-F 生效，「玩家有目标击数」两侧已一致

- 目标可用性门真机 GREEN：C# `120 击里 120 击受害在场且 !ghost && !death（tolerance=1）`
  （`window/12`、`window/21`），`VICTIM_HP_POSTPIN ok expected=20669`。
- 攻击站点取数（干净首跑，`requests/40-analyze-clean.txt`）：

| 站点 | C# cA2 | Rust rA2 |
|---|---|---|
| `atk_power_a` | 120 | 120 |
| `atk_power_b` | 120 | 120 |
| `hit_gate` | **131** | **120** |
| `attack_success` | 120 | 120 |
| `atk_power_base` | **11** | **0** |
| `hit_struck_damage` | 66 | 57 |
| `hit_struck_damage2` | 53 | 57 |
| `struck_durability` | 55 | 57 |
| 合计 | 676 | 651 |

⇒ 修复前 `hit_gate` C# 19 / Rust 38、`attack_success` C# 3 / Rust 38（玩家有目标击数 3 vs 38）；
**现在两侧 player unit 都是 120**，`atk_power_a/b`、`attack_success` 都是 120 ⇒ 「玩家有目标击数」已一致
（这正是 owner 23:50 要的前置）。

## 4. 正面结论②：Rust 侧确定性成立

- Rust 两条独立新栈的干净首跑（`rust-20261013-001443` vs `rust-20261013-001647`）：
  `attack_diff.py diff = 0`；且两条 `rng-stream.tsv` **逐字节相同**（`cmp` 无输出）。
- 第三栈 `rust-20261013-005240`（rA2）汇总与上两条完全一致（hits 57 / dodges 63 / sum 2425）。
⇒ Rust 侧「同种子 ⇒ 逐 k 相同」在本隔离口径下**成立**。

## 5. 根因定位：第一分歧在流水 index=0，class=`base_unit`（C# 专用流按**调用点**共享）

`requests/40-analyze-clean.txt`（`per_strike_requests.py analyze`，cA2 vs rA2）：

```
csharp: 攻击取数 676（player unit=120, base unit=11）
rust  : 攻击取数 651（player unit=120, base unit=0）
第一处请求分歧 index(0based)=0  class=base_unit
  csharp: atk_power_base(1)->0        # BaseObject.GetAttackPower@BaseObject.Base.cs:892（非 PlayObject 攻方）
  rust  : atk_power_a(2)->0           # combat.rs:109（玩家攻方）
```

- **C# 专用流的第一条取数就是 `atk_power_base`**（非玩家攻方，`PlayObject` 的 override 之外的
  `BaseObject.GetAttackPower`）——即**在玩家攻方第一次出手之前**，进程里另一个实体已经往**同一条专用流**
  抽了一发。
- 数量上 C# 比 Rust 多：`atk_power_base +11`、`hit_gate +11`（131 vs 120）——同一并发攻方那一族的取数。
- **机制（事实）**：C# 影子 hook 的隔离是**按调用点**分流（`divert-sites.txt` = `attack_sites.tsv` 的
  `divert=1` 行），**不按攻方实体** ⇒ 进程里任何实体走这些站点都会抽**同一条 `Random(attack_seed)`**。
  本片能直接证的：**C# 流里混入 11 次 `atk_power_base`（base unit）**，第一分歧即落在 base unit。
- **并发（假设，未单独证）**：这 11 次来自「同一 `GameSvr` 进程其它地图上的怪物/守卫在测量窗口内出手」，
 `0137` 本身零刷怪（owner 已核）。m3bag 正在查证那个非玩家攻方是谁；若证伪，则改用「按攻方实体分流」。
- 这解释了 C# 侧**不可复现**（在「并发非玩家攻方」假设下）：其抽取时机每次不同 ⇒ 同种子下玩家攻方
  每击拿到的 draw 不同 ⇒ `cA vs cB = 242`、`c1 vs c2 = 328`。
- 逐击请求**模板形状**两侧一致（`requests/42-jointable-clean.txt` 的 `reqs` 列）：
  miss = `atk_power_a,atk_power_b,hit_gate,attack_success`；hit = `...,hit_struck_damage,hit_struck_damage2,struck_durability,...`。
  即**结构已对齐，分歧是「抽到的值不同 ⇒ 命中/躲闪序列不同」**，不是结构缺站点。

> 结论：本隔离口径的**必需补强** = 把专用流按**攻方实体**分流（或让测量窗口内除被测量攻方外零攻击者）。
> 这不是判据问题，也不是统计/集合降级；属夹具/接线**设计**层（m3bag + owner 定归属）。

## 6. 逐击状态表（两侧同表，`requests/42-jointable-clean.txt`）

- `per_strike_requests.py jointable`：**GREEN**，240 行 = 两侧各 120，`state=fixture`（权威夹具记录）。
- 受害在场：**C# 与 Rust 各 120 击**，唯一差异 `k=1`（C# `vic=absent`、Rust `present`）——
  目标可用性门以 `tolerance=1` 覆盖（`window/12` 已记）。
- C# 有 2 击命中模板缺 `hit_struck_damage2`（`...hit_struck_damage,struck_durability...`），Rust 每击命中都含两者；
  C# `hit_struck_damage2 53` vs `hit_struck_damage 66`（Rust 57/57）——**样本**，待与并发攻方污染一起判（只登记）。
- 计数不等（把多跑流水喂给 jointable）⇒ **fail-closed RED**（`requests/43-...`），工具牙齿有效。

## 7. 夹具/工具 finding（作者侧只登记，m3bag 写集）

- **F-N1（Important）**：C# 隔离按**调用点**分流却**进程级共享**一条 `Random(attack_seed)`，
  被 `0137` 之外的并发非玩家攻方污染（§5）⇒ 隔离后 C# 仍不可复现。建议：分流键加攻方实体
  （ActorId / 对象引用），或保证测量窗口内除被测量攻方零攻击者。
- **F-N2（Important，脚本回归）**：`mir2-rs/tests/parity/m3/start-csharp-hook.ps1:48`
  `. (Join-Path $Here 'm3hashutil.ps1')`，而 `$Here` **只在 `-Repo` 为空时**赋值（`:37`）
  ⇒ 传 `-Repo` 时 `$Here=null`，起栈脚本抛 `Cannot bind argument to parameter 'Path' because it is null`。
  由 F1 修复 `44849fa3` 引入（把 SHA256 收进 `m3hashutil.ps1` 时）。本次规避 = 不传 `-Repo`、
  从含 C# 构建产物的检出起栈。建议把 `$Here = Split-Path -Parent $MyInvocation.MyCommand.Path`
  移出 `if`。
- **观察（非阻塞）**：`csharp k=1 vic=absent` vs Rust `present`（setup 时序），门 `tolerance=1` 已允许；
  若要求逐 k 严格一致，夹具需把双 bot 落点/进世界时序钉齐。

## 8. 窗口 / 清理

- 起栈前实拍：`window/01-ports-before.txt`（5000..7200 全 free，仅共享 3306）。
- 收工：`window/18`/`window/25`（-Stop）+ `window/26-ports-after.txt` ⇒ 九端口全 free，仅共享 3306
  ⇒ **窗口已释放**。
- 大流水（C# 全局 `rng-stream` 25MB / Rust 33KB）只入攻击专用流（`streams/`，见 SHA256SUMS）；
  全局大流水原件在 `E:/tmp/m3cs-2phase2/**`，不入库。

## 9. 文件

| 路径 | 内容 |
|---|---|
| `window/00-selfcheck-nostack.txt` | 合并树 `-SelfCheckNoStack`（31 项 SELFTEST，GREEN） |
| `window/10..18` | C#/Rust（含冒烟）起停 + c1/c2/r1 原始输出 |
| `window/20..25` | C#/Rust 单跑干净首跑（cA2/rA2）原始输出 |
| `diffs/30..35` | 同侧 / 跨侧逐 k diff 原始件 |
| `requests/40-41` | `per_strike_requests.py analyze`（站点计数 + index=0 第一分歧） |
| `requests/42` | `jointable` 两侧同表（240 行，state=fixture）GREEN |
| `requests/43` | `jointable` 计数不等 fail-closed RED |
| `sessions/**` | 各会话 `attack_seq/state/raw/isolation` + C# `server_state_segment` |
| `streams/**` | 两侧攻击专用流原件（C# cA2/cB、Rust rA2/rA/rB） |
