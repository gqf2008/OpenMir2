# 证据包：M3 受击耐久——衣服位（`Index 0`）特判与逐格 `Random(8)` 取数对齐（2026-10-12）

- 卡：`openmir2-m3-combat-skills-align`（issue `a2ef74aa`）／principal `openmir2-worker-5`
- worktree：`.claude/worktrees/m3cs`／branch `worktree-m3-combat-skills`
- 来源：reviewer-5 对 B-1/B-2 片（已合 master `8ac62fb8`）的 **Minor ①**（brief 顶部 owner 2026-10-12 16:55 块）
- 报告：`mir2-rs/tests/parity/m3/M3受击耐久衣服位与逐格取数对齐报告-20261012.md`
- 窗口：未占端口、未起栈（全离线：探针 + 单测）；DoD①② 真机对拍仍未达成（沿用第九片登记）。

## oracle（已按行读全，owner 已亲核 `GetAttackPower` 段）

`src/M2Server/Player/PlayObject.cs:2954-3036` 的 `PlayObject.StruckDamage` override，在 `base.StruckDamage`
之后：

1. `:2957` **无条件**抽一次 `Random(10)`（`nDam = Random(10)+5`）；
2. `:2965-2996` **衣服位特判**（`ItemLocation.Dress = 0`）：**不抽数**，只扣耐久；但 `ushort nDura -= nDam`
   是 **unchecked 回绕**（`:2969`），`if (nDura <= 0)` 对 `ushort` 等价于 `== 0`（`:2970`）⇒ 只有 `Dura`
   **恰好等于** `nDam` 才破零，破零先 `Index = 0`（`:2982`）+ `Dura = 0`（`:2985`）；
3. `:2998-3039` **逐格循环**：`Index > 0` 的每格**各抽一次 `Random(8)`**（`:3000`），`== 0` 才扣耐久，
   破零同样清 `Index`（`:3008`/`:3012`）。

⇒ 衣服位在特判里破零会**少抽一次** `Random(8)`（特判在循环之前）⇒ 全局 RNG 流从该击起错位。
旧的 Rust 侧（B-2 片）只建模了 ③ 且用 `saturating_sub`，两处都不忠实。

## 本片改动

- `mir2-rs/crates/world/src/combat.rs`：新增 `ITEM_LOCATION_DRESS` / `wear_durability` /
  `pub fn struck_damage_durability`；`attack_with` 对玩家目标改调该函数。
  - `wrapping_sub` 复刻 unchecked `ushort`；破零清 `index` 且 `dura = 0`。
- `mir2-rs/crates/world/tests/combat_skill_vectors.rs`：+3 单测（`TargetedRng` 定向取数）
  `dress_break_clears_index_and_skips_random8_in_same_attack` /
  `dress_durability_below_damage_wraps_without_breaking` /
  `loop_break_clears_index_and_affects_next_attack_draws`。
- `mir2-rs/crates/world/tests/struck_durability_vectors.rs`（新）：回放真身取数账本 + 逐字段核状态。
- `mir2-rs/tests/parity/m3/struck-durability-probe/**`（新）：C# 真身探针，5 条用例产
  `vectors/struck_durability_vectors.{json,tsv}`。

本片不动夹具/仪器（`run_attack_pair.ps1` / `attack_diff.py` / `gate_guards.ps1` / `run_pair.ps1` /
`whitelist.md` 只读），不含 `bins/game-svr/**`、不含 `src/**`。

## commands（命令 + 原始输出）

| 文件 | 命令 | 结果 |
| --- | --- | --- |
| `01-baseline-tests.txt` | `cargo test -p mir2-world --test combat_skill_vectors --test struck_durability_vectors --offline --no-fail-fast` | rc=0；27 passed + 1 passed |
| `02-redcheck-drop-index-clear.txt` | 删掉 `wear_durability` 的 `slot.index = 0` 后重跑 | rc=101；`dress_break_*` / `loop_break_*` / `struck_durability_matches_oracle_vectors` 均 FAILED |
| `03-redcheck-saturating-sub.txt` | 把 `wrapping_sub` 改成 `saturating_sub` 后重跑 | rc=101；`dress_durability_below_damage_wraps_without_breaking` / `struck_durability_matches_oracle_vectors` FAILED |
| `04-probe-reproducibility.txt` | 探针连跑两次 + diff + sha256 | 逐字节相同（`e72f703d…`） |
| `05-fmt.txt` | `cargo fmt --all -- --check` | rc=0 |
| `06-clippy.txt` | `CARGO_TARGET_DIR=… cargo clippy --workspace --all-targets --offline -- -D warnings` | rc=0 |
| `07-workspace-tests.txt` | `CARGO_TARGET_DIR=… cargo test --workspace --offline --no-fail-fast` | rc=0；**81 suites / 667 passed / 0 failed / 16 ignored** |
| `08-msgcodegen.txt` | `cargo run -p msgcodegen --offline -- --check` | rc=0；604 |
| `09-verify-sha256sums.txt` | `python -I tools/evidence-hash/verify_sha256sums.py --check-extra --exclude 'bin/*' --exclude 'obj/*' …/struck-durability-probe` | GREEN 4/4；改坏 1 处必红、还原 GREEN |

两次红检后均已还原（`crates/world/src/combat.rs` 现为 `wrapping_sub` + 破零清 `index`）。

## 真身向量（`struck-durability-probe/vectors/struck_durability_vectors.tsv`）

| 用例 | nDam | 输入（衣服/其它） | 衣服结果 | 逐格 `Random(8)` 抽数 | 判读 |
| --- | --- | --- | --- | --- | --- |
| `dress_break` | 7 | 衣服 `Dura=7`（==nDam）；slot1/slot7 `Dura=nDam+10000` | **破零**（`Index=0`/`Dura=0`） | **2**（slot0 被跳过） | 特判破零 ⇒ 同击少抽一次 |
| `dress_wrap` | 13 | 衣服 `Dura=11`（<nDam） | `Dura=65534`（回绕）、`Index` 保留 | 2（slot0+slot1） | 回绕不破零 ⇒ 照抽 |
| `dress_normal` | 8 | 衣服 `Dura=1008` | `Dura=1000`、`Index` 保留 | 2 | 正常扣耐久 |
| `dress_empty` | 13 | 衣服空；slot1 `Dura=13` | — | 1（slot1） | 特判整段跳过 |
| `loop_break` | 7 | 衣服空；slot1 `Dura=7`、`Random(8)=0` | slot1 **破零**（`Index=0`） | 1 | 循环破零清位（影响后续击取数） |

Rust `struck_damage_durability` 用 `ReplayRandom` 回放这 5 条账本：取数零分歧、恰好消费完、
`UseItems` 状态逐字段相同（`cargo test -p mir2-world --test struck_durability_vectors`）。

## 结论

- reviewer-5 Minor ① **已按 oracle 忠实建模，不是白名单挂号**：破零清位 / unchecked 回绕 / 同击跳抽
  三件事都由真身向量 + 3 条单测 + 2 次「改坏必红」坐实。
- 未声明白名单：Rust 侧可等价（真身向量逐字段相同）。

## 残留物

- 隔离 target：`E:/tmp/m3cs-cloth-target`（保留，供复算；不进仓库）。
- 探针构建产物 `struck-durability-probe/bin|obj`（`.gitignore` 覆盖，不入库）。
- 仓库内 `git status` 仅含本片交付路径。
