# M3②③ 活栈隔离口径 —— Rust 侧实现证据（2026-10-12）

> owner 2026-10-12 21:15 裁决（§6.0-3 owner 签字）：DoD② 改为**活栈隔离口径** —— 两侧把**攻击路径
> 取数**接到各自一条**专用种子流**，其余消费者走原全局流；判据仍是真机 `attack_diff.py diff` **逐 k = 0**。
> 本片 = **Rust 侧专用流 + 两侧分流谓词接口**（C# 影子 hook 由 m3bag 实现；本片**未取窗**）。

交付树 = master `e2470c9d`（= `2700d709` + 已合并的 `ed0c4c64`）+ 本片。**零 `src/**`、零 `sql/**`、
零 `MirClient`、零 `bins/**`、零窗口（未起任何栈）。**

## 一、改了什么

| 文件 | 改动 |
| --- | --- |
| `crates/shared/src/rng.rs` | 新增 `ISOLATE_ATTACK_ENV`(`MIR2_RNG_ISOLATE_ATTACK`)、`ATTACK_SEED_DELTA`(`0x9E37`)、`RNG_SEED_ENV`、`attack_seed()`、`attack_isolation_seed_from_env()`（解析 + fail-closed）；2 条单测 |
| `crates/shared/src/rng_trace.rs` | 隔离开启时 trace 头部补 `ISOLATE_ATTACK <attack_seed>`（空格分隔，旧解析器跳过） |
| `crates/world/src/world.rs` | `World.rng_attack`（专用流）+ `isolate_attack` + `enable_attack_isolation()` + `attack_isolation_enabled()` + `attack_rng()`；`attack_once`（`attack_dir`）与 `attack_swing` 按开关选流；`with_maps` 从 env 开启 |
| `crates/world/tests/rng_attack_isolation.rs`（新） | 5 条路由判据（关时不变 / 开时专用 / swing 也走 / 同种子可复现 / 种子同公式） |
| `tests/parity/m3/rng-isolate-attack/**`（新） | 分流谓词表 `attack_sites.tsv` + 机判 `check_predicates.py` + C# 静态 dump 格式示例 + 接口 README |

核心契约（两侧逐字一致）：开关 `MIR2_RNG_ISOLATE_ATTACK`（空/0/false/off/no = 关；1/true/on/yes = 开；
其它非空 fail-closed）；种子 `attack_seed = MIR2_RNG_SEED + 0x9E37`（i32 回绕）；关闭时逐字节不变；
开启时只有攻击路径走专用流。站点表 = 账本 vocabulary（`atk_power_a/b`、`hit_gate`、`hit_struck_damage(2)`、
`struck_durability(_slot)`、`attack_success` 等 11 个 diver canon）。

## 二、复制命令

```sh
cd mir2-rs
cargo fmt --all -- --check
CARGO_TARGET_DIR=E:/tmp/m3cs-r3-target cargo clippy --workspace --all-targets --offline -- -D warnings
CARGO_TARGET_DIR=E:/tmp/m3cs-r3-target cargo test --workspace --offline
CARGO_TARGET_DIR=E:/tmp/m3cs-r3-target cargo run -p msgcodegen --offline -- --check
CARGO_TARGET_DIR=E:/tmp/m3cs-r3-target cargo test -p mir2-world --test rng_attack_isolation --offline
CARGO_TARGET_DIR=E:/tmp/m3cs-r3-target cargo test -p mir2-shared --lib rng::tests --offline
cd tests/parity/m3/rng-isolate-attack
python check_predicates.py selftest
python check_predicates.py static
python check_predicates.py table-vs-csharp --csharp csharp_predicate_dump.example.txt
```

## 三、判据读数

| 判据 | 读数 | 件 |
| --- | --- | --- |
| 四门禁（合并树） | fmt rc=0 / clippy rc=0 / **84 suites 682 passed 0 failed 16 ignored** / msgcodegen **604** | `70a-70d`、`01-gates.txt` |
| 路由判据（新用例） | **5 passed**（隔离关=全局流不变 + 专用流未被取；隔离开=专用流逐项对 + 全局流未动；swing 同；两次可复现；种子同公式） | `02-new-tests.txt` |
| 共享 rng 单测 | `rng::tests` 全绿（含 `attack_seed` 公式 / 专用流与全局流可分辨） | `02b` |
| 谓词机判 | selftest **7/7**、`static` GREEN（11 canon 两侧齐备）、`table-vs-csharp` GREEN（示例 dump 11/11） | `05/06/07` |
| 改坏必红① | `attack_rng` 恒走全局流 ⇒ **2 FAILED**（开时专用流值不符 / swing 不符），关时用例仍绿 | `03`、hunks 见 `10` |
| 改坏必红② | `attack_seed` 恒等 ⇒ **2 FAILED**（公式 / 专用流可分辨）；还原后全绿 | `04`、hunks 见 `10` |
| 真机同形 trace（离线） | 头部含 `INJECT_OK 20261012` / `SEED_PROBE …` / **`ISOLATE_ATTACK 20301515`**（= 20261012+0x9E37）；攻击路径落 `combat.rs:109/111/628/831` | `08*.tsv/txt` |
| 窗口 | **本片未起栈**（无 C# 半无法跑两相）；实拍九端口 5000/5100/5500/5600/5700/6000/7000/7100/7200 全空 | `09-ports-no-stack.txt` |

## 四、与 m3bag 的接口（owner 转发）

1. `mir2-rs/tests/parity/m3/rng-isolate-attack/attack_sites.tsv` = **唯一事实源**：
   `canon / side / pattern / draw / oracle_ref / divert`。
2. C# 影子 hook：开启时按 `side=csharp, divert=1` 的调用点把取数委托给 `Random(attack_seed)`；
   并落**静态分流站点 dump**（每行一个 `Type.Method@file:line`，格式见 `csharp_predicate_dump.example.txt`）。
3. 机判：`check_predicates.py table-vs-csharp --csharp <dump>` ⇒ 缺一条 / 有未登记 site **fail-closed**；
   真机用 `check_predicates.py align --csharp <tsv> --rust <log>` 逐位比 canonical 序列。
4. **待校准**：C# 行号为 PDB/JIT 归因值（`PlayObject.cs:2934` / `BaseObject.Base.cs:793` /
   `PlayObject.Attack.cs:223` 是 F-1 权威流水实测），`Luck>0` 与 `struck_durability_slot` 夹具未触发，
   由 hook 实测后订正表内 pattern。

## 五、夹具 finding（m3bag 写集，只登记）

- `attack_rng_ledger.py` 的 `RUST_ATTACK_RE` 仍是**旧行号**（`67|109|111|579|587|606|608|736`），
  不含 `628/630/758/831` ⇒ 它把 `combat.rs:628` 与 `:831` 误判为噪声（本片实测：`attack_path=8/48`，
  而 `rng_ledger_diff.py` 同 trace 报 `16/48`）。本片用 `rng_ledger_diff.py`（行号已更新）。

## 六、残留物清单

- `E:/tmp/m3cs-iso/**`（本片中间件：四门禁/trace/变异输出）、`E:/tmp/m3cs-r3-target`（隔离 target）：
  **仓外**临时物，非本线专属也可留作复算；不随交付入库。
- 交付树内仅本目录 + `tests/parity/m3/rng-isolate-attack/**` + 上述 `.rs` 改动。
