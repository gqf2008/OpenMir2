# M3②③ 攻击路径「三差」归因证据片（2026-10-12）

- 卡：`openmir2-m3-combat-skills-align`（issue `a2ef74aa`）／车道 `m3cs`／principal `openmir2-worker-5`
- 任务来源：brief 顶部 **owner 2026-10-12 19:20 裁决**（先修攻击路径站点/次序/次数，不签口径变更）
- 报告：`../M3RNG攻击路径三差归因报告-20261012.md`
- 探针：`../attack-dir-probe/`（真身向量 `../attack-dir-probe/vectors/attack_dir_vectors.{json,tsv}`）

## 文件

| 文件 | 内容 |
|---|---|
| `00-commands.txt` | 本片全部命令（探针 / 4 门禁 / 定向对拍 / WorldGolden 重基准 / 工具自检） |
| `01-per-attack-draw-ledger.txt` | oracle 真身 `PlayObject.AttackDir` 的**逐次取数账本**（4 例，含站点 `file:line`）+ 逐击结算 |
| `02-hit-gate-and-segment-counts.txt` | ①②③ 的**归因正文**（两侧计数、逐段形状、根因） |
| `02a-csharp-segment-shapes.txt` | C# 活栈 120 段形状（沿用 `evidence-rng-ledger-diff-20261012/26`） |
| `02b-rust-segment-shapes.txt` | Rust 活栈 36 段形状（沿用 `evidence-rng-ledger-diff-20261012/27`） |
| `03-gates-fmt.txt` | `cargo fmt --all -- --check` |
| `04-gates-clippy.txt` | `cargo clippy --workspace --all-targets -- -D warnings` |
| `05-gates-test.txt` | 定向对拍（`attack_dir_vectors` / `combat_skill_vectors` / `world_combat_parity`）+ 全量汇总 |
| `06-gates-msgcodegen.txt` | `cargo run -p msgcodegen -- --check` = 604 |
| `07-rng-ledger-diff-selftest.txt` | `rng_ledger_diff.py selftest` 6/6（含新 `attack_success` 站点映射检查） |

## 口径

- 真机活栈读数**未在本片重跑**（窗口不在本片）：C# = `E:/tmp/m3cs-tick/csharp-stack/rng-stream.tsv`、
  Rust = `E:/tmp/m3cs-r2/rust-rng-stream-r1.tsv`，均已在 `evidence-rng-ledger-diff-20261012` 入库。
- 真身向量 = `attack-dir-probe` 反射直调 `PlayObject.AttackDir`（不连库、不起栈、不占端口）。
- hash 口径：`SHA256SUMS.txt` 按**仓库字节（LF 归一）**；复核：
  `python tools/evidence-hash/verify_sha256sums.py mir2-rs/tests/parity/m3/evidence-attack-dir-20261012/SHA256SUMS.txt`
- 本片**未 merge、未自签 review**。
