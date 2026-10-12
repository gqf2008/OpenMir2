# M3②③ 活栈隔离口径 —— 攻击路径专用种子流（Rust 侧 + 两侧分流谓词接口）

> owner 2026-10-12 21:15 裁决（§6.0-3 owner 签字）：真机 C# 侧在「全服共享随机源」下**自己不可复现**
> （同种子同配置 fresh stack 两次差 15 项；噪声占 99.95%）⇒「同种子 ⇒ 逐 k 相同」**原则不可达成**。
> DoD② 口径改为**「活栈隔离口径」**：两侧把**攻击路径的取数**接到各自一条**专用种子流**，其余消费者
> 走原全局流；判据仍是真机 `attack_diff.py diff` **逐 k = 0**。全服消费者一致性（Claim B）另立卡
> （`openmir2-rng-consumer-structure-parity`，判据 = 账本 site/order diff，不是逐 k）。

本目录是**两侧分流谓词的对接口径**（Rust 侧已实现；C# 影子 hook 由 m3bag 实现）。

## 1. 契约（两侧必须逐字一致）

| 项 | 口径 |
| --- | --- |
| 开关变量 | `MIR2_RNG_ISOLATE_ATTACK`：空/`0`/`false`/`off`/`no`（大小写不敏感）⇒ **关**；`1`/`true`/`on`/`yes` ⇒ **开**；其它非空值 ⇒ **fail-closed**（两侧都不许猜） |
| 专用流种子 | `attack_seed = (MIR2_RNG_SEED + 0x9E37)`（`i32` 回绕加法；与 .NET `int + int` unchecked 同口径） |
| 关闭时 | **逐字节不变**（专用流不构造/不被取数；既有 golden 与生产路径一字不改） |
| 开启时 | 只有**攻击路径**取数走专用流；其余消费者（怪物 AI/掉落/商人/刷新/构造期…）仍走原全局流 |
| 站点集合/次序/次数 | 必须与本目录 [`attack_sites.tsv`](attack_sites.tsv) 的 `divert=1` 行逐条一致（缺一条 fail-closed） |

Rust 实现落点（本片）：
- `mir2-rs/crates/shared/src/rng.rs`：`ISOLATE_ATTACK_ENV` / `ATTACK_SEED_DELTA` / `attack_seed()` /
  `attack_isolation_seed_from_env()`（解析 + fail-closed）。
- `mir2-rs/crates/world/src/world.rs`：`World::rng_attack`（专用流）+ `isolate_attack` 开关 +
  `enable_attack_isolation()` + `attack_rng()`；`attack_once`（`attack_dir`）与 `attack_swing` 按开关选流。
- `mir2-rs/crates/shared/src/rng_trace.rs`：隔离开启时流水头部补一行 `ISOLATE_ATTACK <attack_seed>`
  （空格分隔，既有解析器按「首列非 int」跳过），让会话产物自证「这次跑的是隔离口径」。

## 2. 分流谓词表 `attack_sites.tsv`

制表符分隔；列：`canon / side(csharp|rust) / pattern(正则) / draw / oracle_ref / divert(1|0) / note`。
匹配前把 site 的 `\` 归一成 `/`。`divert=1` 的行 = 必须分流到专用流的站点；`divert=0` 用于登记
「oracle 有、Rust 未建模、故意不分流」的站点（如 `attack_paralysis`）。

站点来自**账本 vocabulary**（`rng_ledger_diff.py` 的 canonical 名）：`atk_power_a`/`atk_power_b`
（玩家攻方 `PlayObject.GetAttackPower` 两抽）、`atk_power_luck_a`/`atk_power_luck_b`（`Luck>0` 分支，
夹具未触发）、`atk_power_base`（非玩家攻方基版）、`hit_gate`（`_Attack` 命中门）、
`hit_struck_damage`/`hit_struck_damage2`（护甲减免两抽）、`struck_durability`/`struck_durability_slot`
（玩家目标受击的 `Random(10)` + 逐格 `Random(8)`）、`attack_success`（`Random(5)`）。

> C# 行号是 **PDB/JIT 归因值**（F-1 权威流水实测：`PlayObject.cs:2929/2934`，`BaseObject.Base.cs:792/793`，
> `PlayObject.Attack.cs:223`），与源码行（2932/795/225）不同属正常；Rust 行号是 `#[track_caller]`
> 的世界层调用点。

## 3. 要求 m3bag 的 C# 影子 hook（owner 转发）

1. **按调用点分流**：`MIR2_RNG_ISOLATE_ATTACK` 开启时，把命中 `attack_sites.tsv` 中 `side=csharp, divert=1`
   的取数委托给一条**独立的 `Random(attack_seed)`**（`attack_seed` 见 §1），其余取数走原全局流；
   未设开关 ⇒ 行为与现在逐字节相同（用例 + 改坏必红）。
2. **静态谓词 dump**（机判输入）：开启时把**它登记的全部分流站点**（不是运行期命中的子集）写成
   **每行一个 site 字符串**（`Type.Method@file:line`），落一个文件（建议 env `MIR2_RNG_ISOLATE_SITES=<path>`，
   格式即本目录示例 `csharp_predicate_dump.example.txt`）。
3. **会话产物**：记「隔离是否开启 + 专用流 seed + dump 文件 sha256」（m3bag 夹具片职责）。

## 4. 机判

```sh
# ① Rust 侧谓词表自洽（每个 divert canon 两侧齐备）
python mir2-rs/tests/parity/m3/rng-isolate-attack/check_predicates.py static

# ② C# hook 的静态分流站点点表 vs Rust 谓词表（缺一条 / 有未登记 site ⇒ RED，fail-closed）
python mir2-rs/tests/parity/m3/rng-isolate-attack/check_predicates.py table-vs-csharp \
  --csharp <csharp_hook 的静态 dump>

# ③ 两侧隔离后实测 trace 的攻击路径 canonical 序列逐位比（顺序敏感）
python mir2-rs/tests/parity/m3/rng-isolate-attack/check_predicates.py align \
  --csharp <csharp rng-stream.tsv> --rust <rust MIR2_RNG_LOG>

# 自检（7 向；改坏必红）
python mir2-rs/tests/parity/m3/rng-isolate-attack/check_predicates.py selftest
```

`align` 只保留表内攻击路径站点（噪声消费者不计入），报第一处分歧；`--set-only` 只比集合。

## 5. 限制 / 未核（登记，不静默）

- `attack_paralysis`（`PlayObject.Attack.cs:221`，依赖 `Paralysis` 状态）Rust **未建模** ⇒ `divert=0`
  （仍走全局流；夹具无麻痹不触发）。若 hook 分流了它，机判会报未登记 site fail-closed。
- `atk_power_luck_a/b`、`struck_durability_slot` 在现有夹具（`Luck=0`、无装备）**未触发**；行号待
  隔离口径真机实测校准（本表按源码行给出，C# 侧需按 PWD/JIT 实测值订正）。
- 本片**未取窗**（C# 半未就绪）；两相真机复跑在 m3bag 的 C# 片合入后进行。
