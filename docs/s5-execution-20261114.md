# S5 执行：`crates/world` 退役（26 模块并入 `world-bevy` + 原子切依赖 + 删除）

> 卡：`openmir2-s5-execute-world-retire`（tier R，`window_required = true`）。
> 车道：worker-84，分支 `card/s5-execute`。初版 base `a05260e86`；按复审 `4aa4bac1` 的
> S5-R1 **已 rebase 到 `068061346`**（master 前进 6 个提交，含 10k 容量模型）。
> 授权：Owner 飞书原文「没用就删掉」（message_id `om_x100b60494b5708a0c1f4b8cfdac8f5b`，2026-11-14 11:07），
> 项目总监 opc-49 判定为 S5 授权（`E:/OPC/docs/workboard.md` 11:10 行）。**范围仅 `crates/world` 退役**。
> 准备文档：[`s5-prep-20261114.md`](s5-prep-20261114.md)。

---

## 0. 结论摘要

1. **26 个模拟模块已并入 `mir2-rs/crates/world-bevy`**（26 个 `src/*.rs` + 20 个集成测试 +
   `src/bin/tick_bench.rs`，共 47 个文件）。搬移是**纯搬 + 改名**：已用
   「`rustfmt(HEAD 的 crates/world 源文件 + 枚举改写) == world-bevy 成品文件`」逐字节证明，
   47/47 全部相等（mismatch 0）。
2. **8 个 crate 根重名已消歧**（`World` / `Entity` / `Command` / `EntityRef` / `EntityMut` /
   `MerchantState` / `NpcState` / `DupState`）。
3. **6 个 Cargo 引用点全部翻转**；`mir2-world` 这个**包别名也一并删除**，代码里不再出现
   `mir2_world::`（`rg --glob '*.rs'` = 0）。
4. **`mir2-rs/crates/world` 已删除**（49 个受版本控制文件，`git rm -r`；删除前已核
   `git status --ignored` 为空、磁盘文件数 = 跟踪文件数 = 49）。
5. **RNG site 面已随路径同步**：12 个谓词文件逐条核过并跑绿；`magic_draws_rng_trace` 的
   **负断言原本会变空洞**（旧前缀在新布局下恒不命中），已改到新前缀并用真实 trace 证明有牙。
6. **回滚双通道可用**：tag `world-archive-20261114` 冷克隆 `cargo check --workspace` rc=0；
   仓外 tar 的 sha256 未变。
7. **五门全绿**；`cargo test --workspace --no-fail-fast` 全绿；6 份 golden 逐字节未动；
   parity 0 failed。
8. **复审返工（`4aa4bac1`）**：已 rebase 到 `068061346`（含 10k 容量模型；两处冲突按 master
   语义解，见 §11）；`castle_oracle_anchor_gate` 的旧路径已修（check 由 `FileNotFoundError`
   回到 GREEN）；P2 的三处「路径分量拼接 / 双反斜杠字面量」漏改已补齐并**逐脚本实跑**（§5.3）。

---

## 1. 搬移内容与一致性证据

### 1.1 搬了什么

| 类别 | 数量 | 去向 |
| --- | --- | --- |
| `src/*.rs`（含 `lib.rs` 之外的 26 个模块） | 26 | `mir2-rs/crates/world-bevy/src/` |
| `src/bin/tick_bench.rs` | 1 | `mir2-rs/crates/world-bevy/src/bin/` |
| 集成测试 `tests/*.rs` | 20 | `mir2-rs/crates/world-bevy/tests/` |
| 合计新增文件 | 47 | — |
| `src/lib.rs` 的 `pub mod` / `pub use` | 全量 | 追加进 `world-bevy/src/lib.rs`（`//!` 降为 `//`） |

### 1.2 逐字节一致性（A1）

搬移只用**规则化改写**，规则之外一个字符未改：

* 模块内部：`mir2_world_bevy::` → `crate::`、`mir2_world::` → `crate::`（同 crate 自引用）；
  另加两处重名消歧（见 §2）。
* 集成测试 / bin：`mir2_world::` → `mir2_world_bevy::`。
* 路径字面量：`crates/world/src/...` → `crates/world-bevy/src/...`（RNG site 谓词与 In-repo 注释），
  以及 `tick_bench` 复现命令的 `-p mir2-world` → `-p mir2-world-bevy`。

**证明方式**：从 **git tree 的 `HEAD:mir2-rs/crates/world`**（删除后仍可取）读原文件，套上述规则，
再用 `rustfmt --edition 2021` 格式化，与 `world-bevy` 成品逐字节比对。结果：

```text
rustfmt-normalized compare (source from HEAD): modules=26 tests=20 bin=1 mismatches=0
```

即：除枚举改写与 rustfmt 的**确定性排版**（`use` 排序、换行）外，内容与 `crates/world` 完全一致。

> 为什么要过一遍 rustfmt：路径变短/变长会改变行宽，`cargo fmt` 会重排少数 `use` 行与长表达式；
> 这些是**格式化**差异，不是语义差异。上面这步把「格式化差异」与「真实内容差异」分开证明。

### 1.3 搬迁前后 blob oid（抽样留档）

`git hash-object` 前后各记一次：搬前 47 个 blob oid 见
`E:/UserTemp/w84s5/evidence/pre-move-oids.txt`；搬后成品与「源 + 规则」逐字节相等（§1.2）。

---

## 2. 8 个 crate 根重名消歧（A1）

`world-bevy` 的 crate 根原先有 `use bevy_ecs::prelude::*`，把 `World`/`Entity`/`Command`/
`EntityRef`/`EntityMut` 这五个名字占住；合并后 `crates/world` 的门面类型要在**同一 crate 根**
导出。处置如下（与准备文档 §4.3 的推荐一致）：

| 名字 | 处置 | 落点 |
| --- | --- | --- |
| `World` | crate 根显式 `pub use world::World` 压过 glob；内核代码改用全限定 `bevy_ecs::prelude::World` | `world-bevy/src/lib.rs` 的 D3 函数 |
| `Entity` | 同上：根 `pub use entity::Entity`；内核用 `bevy_ecs::prelude::Entity` | `OrderIndex` 与 D3 函数 |
| `Command` | 根显式 `pub use world::{Command, …}` 覆盖 glob（bevy `Command` trait 在合并后无人使用；编译期验证） | `world-bevy/src/lib.rs` |
| `EntityRef` / `EntityMut` | 根 `pub use entity::{EntityRef, EntityMut, …}` 覆盖 glob；消费方零改动 | `world-bevy/src/lib.rs` |
| `MerchantState` / `NpcState` | 组件型 wrapper 只经 `components::` 暴露；根保留 world 侧业务结构 | `components.rs` 的 `pub use` 列表移除两名字；`entity.rs` 的组件别名改指 `crate::components::…` |
| `DupState` | 同一定义，根只导一份（`pub use components::DupState`） | `world-bevy/src/lib.rs` |

> 消歧只有 3 处真实代码改动：`lib.rs` 的 6 处全限定、`entity.rs` 的 2 个类型别名，
> 其余靠显式 `pub use` 压 glob（Rust 名字解析：显式导入优先于 glob，不产生歧义警告）。

---

## 3. Cargo 依赖翻转（6 个点，原子）

| # | 文件 | 改动 |
| --- | --- | --- |
| 1 | `mir2-rs/Cargo.toml` | workspace `members` 删除 `"crates/world"` |
| 2 | `mir2-rs/bins/game-svr/Cargo.toml` | `mir2-world = { path = "../../crates/world" }` → `mir2-world-bevy = { path = "../../crates/world-bevy" }` |
| 3 | `mir2-rs/crates/envir/Cargo.toml` | 同上（`../world-bevy`） |
| 4 | `mir2-rs/tests/parity/Cargo.toml` | 同上（`../../crates/world-bevy`） |
| 5 | `mir2-rs/crates/world-bevy/Cargo.toml` | **删除反向 dev-dependency `mir2-world`**（连注释段一起删；`[dev-dependencies]` 空段移除） |
| 6 | `mir2-rs/crates/world/Cargo.toml` | 随 crate 一起 `git rm`（其中的 `mir2-world-bevy` 依赖自然消失） |

附加：`mir2-rs/Cargo.lock` 由 cargo 自动重解——`mir2-world` package 条目删除，
三个消费方的依赖名改为 `mir2-world-bevy`（共 +3/−15 行）。

> **与准备文档的偏差（有意）**：准备文档建议用 `mir2-world = { package = "mir2-world-bevy" }`
> 的**别名**让消费方零改动；本次执行按派工要求**不留别名**，把 53 个 `.rs` 文件里的
> `mir2_world::` 全部改名为 `mir2_world_bevy::`（2506 处）。理由：别名会让「已退役的 crate 名」
> 继续活在依赖图与 import 里，与「引用清零」的验收口径冲突。

---

## 4. 引用清零（A3）

```text
rg 'mir2_world::' --glob '*.rs' mir2-rs   → 0 命中
rg 'mir2_world::' --glob 'Cargo.toml'     → 0 命中
```

`mir2-world` 只剩 crate 自身名 `mir2-world-bevy` 与准备文档/历史记录里的叙事文本。

**残留（均为非编译面，未改）**：全仓（不含 `.worktrees`）仍有 93 处 `mir2_world::` 字面量，
分布在历史文档与证据里——28 个 `.md`、12 个 `.txt`、1 个 `.patch`、1 个 `.py`
（`tests/parity/m3/**` 的设计/盘点报告、`tests/parity/world/client-e2e-enter-gaps-20261013/`
的检查脚本等）。这些是**当时运行的记录**，不是当前判据；其中多个目录被 `SHA256SUMS` 钉住，
改写会破坏证据完整性。另：`docs/s5-prep-20261114.md` 与更早的 `docs/*.md` 同样保留原文。

> 另：随 crate 搬来的 `tests/no_parallel.rs` 里个别 assert 文案仍写 `crates/world/src`（纯文案，只在失败时打印）；
> `tools/m2-acceptance/` 下两份 ps1（`check_isolate_plumb.ps1`、`rust_seed_plumb.ps1`）的注释与
> `tests/parity/m3/bag_equip_sequence.json` 的说明里仍有 `crates/world` 字样（纯叙述、不参与判据），
> 未改；已被 `SHA256SUMS` 钉住的 `vectors/effect_registry.*` 同样保留旧 `oracle_anchor` 文本。
>
> 结论：**编译单元内为零**；文档/证据里的历史叙述保留原样，并在此登记。

---

## 5. RNG site 面（A4）

### 5.1 12 个谓词文件逐条核对

| # | 文件 | 类型 | 结果 |
| --- | --- | --- | --- |
| 1 | `crates/world-bevy/tests/hit_dice_rng_trace.rs` | 正/负断言 + 现读源码 | 绿 |
| 2 | `crates/world-bevy/tests/magic_draws_rng_trace.rs` | 静态守卫 + 正/负断言 | 绿 |
| 3 | `tests/parity/tests/player_ctor_rng_trace.rs` | 正断言（含反斜杠分支） | 绿 |
| 4 | `bins/game-svr/tests/p3b1_rng_trace.rs` | 正/负断言 | 绿 |
| 5 | `bins/game-svr/tests/p3b2_rng_trace.rs` | 正/负断言 | 绿 |
| 6 | `bins/game-svr/tests/p3b3_rng_trace.rs` | 正/负断言 | 绿 |
| 7 | `bins/game-svr/tests/p3b4_rng_trace.rs` | 正/负断言 | 绿 |
| 8 | `bins/game-svr/tests/p3b5_rng_trace.rs` | 正/负断言 | 绿 |
| 9 | `bins/game-svr/tests/magic_effect_rng_trace.rs` | 正/负断言（分组计数） | 绿 |
| 10 | `bins/game-svr/tests/ctor_fold_rng_trace.rs` | 正断言 + 屏障 A 负断言 | 绿 |
| 11 | `bins/game-svr/tests/spell_tail_rng_trace.rs` | 正/负断言 | 绿 |
| 12 | `bins/game-svr/tests/merchant_rng_trace.rs` | 正断言（含反斜杠分支） | 绿 |

改法：两种分隔符分支都从 `crates/world/src/` 改到 `crates/world-bevy/src/`；
`hit_dice` / `magic_draws` 里「现读源码」用的是 `CARGO_MANIFEST_DIR + src/<file>.rs`，
crate 目录变了但相对路径仍成立，**代码不用改**（只改提示串）。

### 5.2 `magic_draws_rng_trace` 的负断言：本来是空洞的，已修好并证明有牙

原负断言：`!norm.contains("crates/world/src/magic.rs:")`。
搬迁后所有 site 都变成 `crates/world-bevy/src/…`，**旧串永远不命中 ⇒ 断言恒真（空洞）**。

实测（用 `M2_MAGIC_DRAWS_TRACE_PATH` 落盘真实 trace，正斜杠归一后统计）：

```text
旧串 crates/world/src/magic.rs:            0 命中   ← 留在旧串就是空洞
新串 crates/world-bevy/src/magic.rs:       0 命中   ← 真实状态下应为 0
正串 magic_effect.rs / combat.rs / merchant.rs / space_move.rs：58 / 5 / 2 / 28 命中
```

正串全部命中说明 trace 的 site 前缀**确实**是 `crates/world-bevy/src/`；因此
「在 `magic.rs` 里出现一次取数」会产出同一前缀的 site，新负断言必然命中并判红。
负对照：把一行「`crates/world-bevy/src/magic.rs` + 任意行号」喂给同一谓词 ⇒ 命中（有牙）。

`hit_dice` 的旧行号负断言同理改为新前缀；实测 trace 里 6 条 combat site 全是新前缀
（旧前缀 0），旧行号 1018 在两套前缀下都是 0（该旧行号不再出现，负断言仍有意义）。

### 5.3 账本/闸门脚本与落盘夹具

**口径修正（复审 `4aa4bac1` P2；初版这里写「全部同步」是被证伪的）**：初版只替换了
「包含 `/src/` 的字面量」，漏了两种形态——**路径分量拼接**（`os.path.join(..., "crates", "world",
"src", ...)`）与 Python 源码里的**双反斜杠字面量**（`crates\\world\\src\\...`）。
后果不是「看起来没改」，而是判据**空过**：源码读不到 ⇒ live-line 检查返回 `[]`、canonical 检查被
静默跳过。已全部修好，并**逐脚本实跑**（同一脚本在 master `068061346` 上跑一遍作基线）：

| 脚本 | 初版 head | 修后实跑 | master 基线 |
| --- | --- | --- | --- |
| `castle_oracle_anchor_gate.py check` | `FileNotFoundError` rc=1 | **GREEN rc=0**（222 锚命中冻结 `src/`） | GREEN |
| `castle_oracle_anchor_gate.py selftest` | — | GREEN | GREEN |
| `attack_rng_ledger.py selftest` | 64 项（6 条 live-line/canonical **被跳过**） | **70 项**；6 条未过 | **同 6 条未过**（1037/1174/1282）⇒ 既有漂移 |
| `rng_ledger_diff.py selftest` | 9/51 RED | **5/57 RED** | **5/57 RED**（同 5 条） |
| `check_anchors.py selftest` | 19/21（牙退化成 FILE_MISSING） | **20/21** | **20/21**（唯一红是既有 `real-doc-fresh`） |
| `stream_equiv.py selftest` | — | GREEN 10/10 | GREEN |
| `rng_trace_diff.py selftest` | — | GREEN 11/11 | GREEN |
| `analyze_rng_trace.py selftest` | — | GREEN 5/5 | GREEN |
| `drop_exp_purity_gate.py selftest` | — | GREEN 32/32 | GREEN |
| `rng_consumer_groups.py selftest` | — | GREEN 27/27 | GREEN |

被修的形态（三类都覆盖）：`"crates", "world", "src"` 分量拼接、`crates\\world\\src\\`
双反斜杠、`crates/world/...` 正斜杠。涉及脚本：`attack_rng_ledger.py`、`rng_ledger_diff.py`、
`castle_oracle_anchor_gate.py`、`check_anchors.py`、`stream_equiv.py`、`rng_trace_diff.py`、
`analyze_rng_trace.py`、`drop_exp_purity_gate.py`、`rng_consumer_groups.py`、
`whitelist_anchor_gate.py`、`check_predicates.py`、`compare_dedicated_streams.py`、
`m3_guild_war_quirk_registry_gate.py`，以及 `consumer_groups.tsv`、`attack_sites.tsv`、
`drop_exp_consumers.tsv`、`rng_*_scenario.json`。

> `attack_rng_ledger` 的 6 条与 `rng_ledger_diff` 的 5 条、`check_anchors` 的 1 条都在 master 上
> 同样失败 ⇒ 是**既有工具漂移**（行号/文档锚在 S5 之前就漂了），不是 S5 引入；本卡只保证
> **判据真的在跑且结果与基线一致**。

**刻意未改**（`SHA256SUMS` 钉住的证据/历史记录）：`attack-request-sequence/07-first-divergence.txt`、
`provenance/rust-rng-stream-stack{1,2}.tsv`、`attack-request-sequence/per_strike_requests.py`、
`magic-effect-probe/vectors/effect_registry.{json,tsv}`、`skill-probe/vectors/*`。
复算：全仓 260 份 `SHA256SUMS` / 4217 条，改动后 mismatch 仍为**改动前的那 6 条**
（都在 `tests/parity/world/client-e2e-enter-gaps-20261013/`，属 master 既有状态，本次未触碰）。

### 5.4 守卫与工具配置

| 项 | 改动 | 验证 |
| --- | --- | --- |
| `tests/no_parallel.rs`（随 crate 搬） | 扫描根自动变成 `world-bevy/src/**`；依赖白名单仍只允许内部 crate + `bevy_ecs`/`bevy_app` | 6 passed |
| `bevy_drop_parity.rs::no_parallel_guard` | 无改动（白名单仍是 `[dependencies]` 精确比对） | 4 passed |
| `tools/hygiene/docs_anchor_registry.toml` | `[[base_dirs]]` 的 `crates/world/src/` → `crates/world-bevy/src/`（顺序仍在 game-svr 之前，同名文件解析口径不变） | 原生门与基线一致（同一条既有 UNDECIDABLE） |
| `tools/hygiene/docs_anchor_gate.py` | `HIGH_CHURN_PREFIXES` 改为 `mir2-rs/crates/world-bevy/`；自测夹具同步挪到新路径（否则「高 churn 行号锚」档会静默放宽） | `--selftest` 42/42 |
| `tests/parity/m3/whitelist_anchor_gate.py` | `HIGH_CHURN` 6 条 world 路径改名（判据 5 要求文件存在，否则 rc=2） | `check` 与基线同为 2 条**既有** RED（T-192 的 `merchant.rs` 行号漂移；`npc.rs` 的高 churn 符号锚 `guild_official_run_body` 缺失），`selftest` 10/11 与基线一致（①「真身全绿」在 master 上本就红） |
| `rng_consumer_groups` | 表改名 | `selftest` 27/27 |
| `card_window` / `mir2_probe` | 路径字面量/复现注释 | `--selftest` 474/474、29 checks OK |
| `tools/window/butch_ledger.py` | 它**硬编码读取** `mir2-rs/crates/world/src/world.rs` 给 Rust 账本对 span；随迁到 `world-bevy`（否则 selftest 的 j 直接红） | `--selftest` 12/14——**k/n 两条是既有 RED**（见下） |
| `tool_selftest_gate.py`（全量工具门） | — | 66 项：绿 65 / 红 1 / 需人工 0；红的是 `butch_ledger` 的 k/n |

---

## 6. 回滚实测（A6）

两条独立通道都验过（详见 §8 证据索引）：

1. **tag 冷克隆**（本次实测，2026-11-14）：`git clone --no-hardlinks --branch
   world-archive-20261114 --single-branch` → 5.9s；`git rev-parse HEAD` =
   `80f97231ecaaf669c33c6c3e19228f50f4d32bfe`，`git describe --tags` = `world-archive-20261114`，
   `git status --porcelain` 空 → 隔离 target 上 `cargo check --workspace` **rc=0**（冷跑 31.8s）。
   检出位置：`E:/UserFleet/rollback-test/s5-execute-restore-20261114`。
2. **仓外 tar**：`E:/UserFleet/archive/world-crate-20261114.tar.gz` sha256 =
   `f12c18627e6533a833a567d228e524562c2a5130511e3e82eb41055e5877ded0`（与准备文档一致，未变）；
   `tar --force-local` 解包到独立目录后，逐个 `git hash-object` 与
   `git ls-tree -r 80f97231e:mir2-rs/crates/world` 的 blob oid 比对：**49/49 相等，mismatch 0**。

删除后的回退路径（与准备文档 §5.2 一致）：
`git checkout world-archive-20261114 -- mir2-rs/crates/world`，再还原 6 个 Cargo 点、`Cargo.lock`、
守卫与锚配置，然后跑五门。

---

## 7. 验收结果（A5）

| 门 | 命令 | 结果 |
| --- | --- | --- |
| 1 | `cargo fmt --all -- --check` | rc=0 |
| 2 | `cargo clippy --workspace --all-targets -- -D warnings` | rc=0 |
| 3 | `cargo test --workspace --no-fail-fast`（rebase 到 `068061346` 后） | **155 个测试二进制：3883 passed / 0 failed / 69 ignored**，`EXIT=0` |
| 4 | `cargo run -p msgcodegen -- --check` | rc=0，604 条常量一致 |
| 5 | `docs_anchor_gate.py --prose --base 068061346` | rc=0（GREEN；新增行 0 条 path:line 锚） |
| golden | `tests/parity/world/golden/` 6 份（`aoi_viewrange12`/`basic`/`combat`/`combat_exp_hitter_ghost`/`combat_exp_nofix`/`session`） | `git status --porcelain` 空（逐字节未动） |
| parity | `cargo test -p mir2-parity-tests` | rc=0；各测试二进制 0 failed |

> **目标数 155 vs master 的 157**：cargo 的目标段 = 135 个 `Running` + 20 个 `Doc-tests`。
> 退休 `crates/world` 让 **`mir2_world` 的 lib 单测目标**与 **`Doc-tests mir2_world`** 两个目标
> 并入 `world-bevy`（用例与 doc-test 本身不丢；首轮复审已用「用例名称多重集对称差 0」确认）。
> `passed / ignored` 与复审期望的 `3883 / 69` 完全一致。

---

## 8. 写集与实际改动（偏差登记）

卡面写集：`mir2-rs/crates/world-bevy/**`、`mir2-rs/Cargo.toml`、`mir2-rs/bins/**`、
`mir2-rs/crates/envir/**`、`mir2-rs/tests/**`、`mir2-rs/crates/world/**`（删除）、
`docs/s5-execution-20261114.md`。

**实际改动落在写集内**的部分不再重复。**越出卡面写集**的有：

| 路径 | 为什么必须改 |
| --- | --- |
| `mir2-rs/Cargo.lock` | 删 crate + 改依赖名后 cargo 自动重解；不改则锁文件与清单不一致 |
| `mir2-rs/.gitignore` | 注释里的 `tick_bench` 复现命令写死 `-p mir2-world` |
| `tools/hygiene/docs_anchor_gate.py`、`tools/hygiene/docs_anchor_registry.toml` | 判据配置；不改则锚解析失败/高 churn 档静默放宽（准备文档 §3.3 / §4.5-6 已预告） |
| `docs/needs-human-20261024.md` | 文中两条证据命令引用已删路径；CI 的 `docs_delivery_gate` 判 `path-missing`（RED），
随迁到 `crates/world-bevy`（内容逐字节一致） |
| `tools/conformance/card_window.py`、`tools/conformance/rules.json`、`tools/probe/mir2_probe.py`、`tools/probe/scenarios/login_only.json`、`tools/m2-acceptance/start_rust_stack.ps1`、`tools/window/equip_struck_dura_triage.py`、`tools/window/rules-butch-census-ident-triage.draft.json`、`tools/window/butch_ledger.py` | 判据面/复现命令里的 `crates/world/src/…` 路径与 `mir2_world::` 名字（纯文本同步，无逻辑改动） |

依据：准备文档 §4.5 第 8 条「写集必须扩写」已预告 `tools/hygiene/**` 与 `docs/**`；
派工单也要求「任何 `mir2_world` 的 dev-dep/dev-target 引用都要一并处理」。请审者按此口径裁定。

---

## 9. 残留物

| 路径 | 说明 |
| --- | --- |
| `E:/UserTemp/w84s5x-target` | 本车道的隔离 cargo target（可随时删，纯构建缓存） |
| `E:/UserTemp/w84s5/` | 迁移脚本 + 证据（pre-move oid、full-test.log、真实 trace、回滚日志） |
| `E:/UserFleet/archive/world-crate-20261114.tar.gz` | **归档本体，必须长期保留** |
| `E:/UserFleet/rollback-test/*` | 准备卡与本次回滚实测用的仓外检出 |
| 仓库内 | 无临时文件；`git status` 只有本卡的改动 |

---

## 10. 未测面 / 风险

1. **未跑真机窗口**：本卡的 `window_required = true`，但派工范围是代码迁移 + 五门 + 全量测试；
   DB 面与真机对拍留给窗口（准备文档 §4.5-9 同口径）。
2. **`tick_bench` P99 三档未复跑**：`-p` 已改为 `mir2-world-bevy`，bin 可编译；性能门属窗口项。
3. **既有 RED 不是本卡引入**：`whitelist_anchor_gate check` 的 2 条 RED、
   `dynamic_count_stamp_gate` 的 1 条缺戳（`docs/world-ecs-migration-20261113.md`）、
   6 条 `SHA256SUMS` mismatch 均在本卡改动前就存在（见 §5.3 / §5.4 的基线对照）。
4. **`butch_ledger` 的 k/n 是既有 RED，不是本卡引入**：同一套 `fn_span` / `_strip_code` 逻辑分别喂
   「tag 归档里的 `world.rs`」与「搬移后的 `world.rs`」，得到完全相同的数字
   （span 8483–8500；`random_below(16)/(201)` 全文件各 1 处、**函数内 0 处**；
   剥注释前后行数 18267→16393 / 18268→16394）⇒ 该工具的函数区间与取数位置在 S5 之前就已漂移。
   本卡只把它的源码路径随迁（j 由红转绿），k/n 维持原状，建议单独开卡修。
5. **`no_parallel.rs` 依赖白名单里的 `mir2-world-bevy` 已成死项**（crate 不能依赖自己，
   cargo 会报循环依赖）——不影响判据，但建议后续卡片收紧。
6. **历史文档里的 `mir2_world::` 保留原样**（§4 残留）。
7. **三个 RNG 账本脚本的 selftest 在 master 上就是 RED**（`attack_rng_ledger` 6/70、
   `rng_ledger_diff` 5/57、`check_anchors` 20/21）：初版漏改路径时它们是**空过**（假绿/跳过），
   复审判定「判据空过 = 假绿」；现在判据真的在跑，结果与 master 基线逐条一致（§5.3）。
   修这三条工具的行号/文档锚属另一张卡，本卡不夹带。

---

## 11. 复审返工（review `4aa4bac1`，request_changes）

### 11.1 S5-R1：分支基太旧 → 已 rebase 到 `068061346`

- 初版 base `a05260e86`；master 已前进 6 个提交（10k 容量模型 + T10 Top2/Top4 + P6b2 取数回归修复）。
- `git rebase walgit/master` 后**只有 2 处冲突**（与复审试合并一致）：`world-bevy/src/aoi.rs` 的
  import、`world-bevy/src/world.rs` 的 `MapsRes` 插入点。两处都**按 master 语义**解：
  - `aoi.rs`：master（T10 Top2）删掉了 `use std::collections::HashMap;` ⇒ 冲突块整段删除；
    `crate::MapsRes` 已在上一行导入。
  - `world.rs`：保留 master 的 `world.insert_resource(crate::MapsRes(maps));`（只做 crate 路径改写）。
- **关于 `AoiIds` 的事实核对（与复审描述的差异，需审者确认）**：master 的 T10 Top2 **已经删除**
  `AoiIds` 资源——`walgit/master:mir2-rs/crates/world/src/aoi.rs` 里 `AoiIds` 只剩一条注释
  （“此前是 facade 每 tick `slot_order()` 克隆进 `AoiIds` 资源”），`output_aoi_system` 改为直接
  借用 `SlotTable`。所以：
  - 若保留 `crate::aoi::AoiIds(ids)`，结果不是“运行时错”，而是**编译错**（类型已不存在）；
  - 因此按“以 master 的容量模型改动为准”解冲突 = **删掉**这行；容量模型的行为由 master 的
    `aoi.rs` / `tick.rs` / `ecs.rs` 改动承担（已随 rebase 进来）。
- 顺带修掉 rebase 自动合并带来的**自引用残留**：master 新行里的 `mir2_world_bevy::…` 在合并后的
  crate 内是自引用（`aoi.rs` 3 处、`entity.rs` 3 处），已统一改成 `crate::`。
- 正确性证明（对 **master 源**重做一次“纯搬 + 改名”证明）：
  `rustfmt(walgit/master:mir2-rs/crates/world/src/X.rs + 枚举改写) == world-bevy/src/X.rs`，
  **26 模块 / 20 测试 / 1 bin，mismatches 0**。
- master 对 `world-bevy/src/{tick.rs,ecs.rs}` 的改动也核过：`tick.rs` 与 master **逐字节相同**；
  `ecs.rs` 只差本卡的注释路径改写。

### 11.2 S5-R2：`castle_oracle_anchor_gate` 崩溃 → 已修且 GREEN

`CASTLE_RS` 的路径分量拼接里 `"world"` 未随迁 ⇒ `open` 抛 `FileNotFoundError`。
改为 `"world-bevy"` 后：

```text
GREEN: castle oracle 锚门通过（222 锚命中冻结 src/ + 写侧在位 + 覆盖区间无未登记/裸锚）  rc=0
selftest: SELFTEST GREEN                                                   rc=0
```

### 11.3 P2 四条

见 §5.3 的表：`attack_rng_ledger` / `rng_ledger_diff` / `check_anchors` 的实际结果与 master 基线
逐条一致（判据真的在跑，不再空过）；交付文档里“live 判据脚本全部同步”的失实表述已改写。
另外把同类的 `stream_equiv.py`、`rng_trace_diff.py`、`analyze_rng_trace.py`、
`drop_exp_purity_gate.py` 一并修好并实跑（全 GREEN）。

### 11.4 P3（登记，本卡不处理）

两个 ps1 preflight 的旧目录注释、约 41 处纯注释文案、`no_parallel.rs` 依赖白名单里的
`mir2-world-bevy` 惰性自名——均为注释/死项，不影响判据，随下一张收敛卡处理。
