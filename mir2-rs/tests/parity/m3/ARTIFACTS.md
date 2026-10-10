# M3 产物与临时物登记（D2 线）

按设计文档 §14.3（每批收尾清单第 5 条）：本线产生的快照/临时目录/临时库逐个登记位置与去向。
**结论：无长期临时库；临时目录只有"影子副本"，由基线命令自建于会话目录内并被自动清理；
快照 TSV 体积大、不入库但登记位置（可再生）。**

## 一、入库的产物（证据，随 patch 提交）

| 路径 | 内容 | 大小 |
| --- | --- | --- |
| `m3/sessions/csharp-*/baseline.json` | 单侧基线：13 表快照 sha256、作用域行数、改动清单、**runtime_procs**（产出该基线的 7 个进程实际 exe 路径与启动时间）、git 提交 | ~12KB |
| `m3/sessions/csharp-*/snap_{before,after}.manifest.json` | 快照逐表行数 + sha256（快照本体不入库，见下） | ~4KB |
| `m3/sessions/csharp-*/scoped_{before,after,after_norm}.jsonl` | 作用域内的行（按前缀抽取；行数少，含全部判据数据） | <4KB |
| `m3/sessions/csharp-*/{delta,ops,provision,extract_*,dbsnap_*}.json/log` | 判据的原始输出（命令 + 输出） | <20KB |
| `m3/sessions/csharp-*/bot.log` | 假人日志（操作序列判据的原始依据） | <20KB |
| `m3/sessions/m3rng-gate-*/product.json` | RNG 基线：种子、两次运行的取数条数与**确定性前缀 sha256**、公共前缀长度、hook/pack 指纹、掉落与经验序列 sha256、`pack_unchanged` | ~2KB |
| `m3/sessions/m3rng-gate-*/rng-run{1,2}.tsv` | 真实进程的 RNG 取数流（含调用点） | **不入库**（~1.5MB×2，可被 `rng-baseline.ps1` 再生；判据本体是 product.json 里的 `prefix_sha256`） |
| `m3/sessions/m3rng-gate-*/{drop_100kills.txt,exp_table.txt}` | 掉落/经验序列（C# oracle 公式，同种子） | <100KB |
| `m3/sessions/M3-gate-*.json` | 门禁报告：入口实现识别、各步骤退出码、失败项清单、pass | ~4KB |
| `m3/fixtures/rng_stream_seed42_*.tsv` | RNG 记录流夹具（回放门禁用） | ~350KB |

## 二、不入库但需登记位置的产物

| 路径 | 为什么不入库 | 去向 |
| --- | --- | --- |
| `m3/sessions/*/snap_before/`、`snap_after/` | 活库全表导出（单次 ~36MB，含 30 万行 storageitem） | `.gitignore` 已忽略；**可随时用 `run_pair.ps1` 重生成**；manifest 里的 sha256 是判据本体 |
| `m3/sessions/*/shadow-run*/` | 影子副本（217MB/次，含 Map/Envir） | 基线脚本结束时自动 `Remove-Item`（`-KeepShadow` 才留） |

## 三、无临时库

本线**不建临时库**：比对全部落在既有 `mir2_db`/`mir2_account` 上，用"每次运行全新前缀 +
作用域抽取"隔离（详见 `README.md`「为什么同一初始状态不靠破坏性还原」）。
测试账号/角色是**新增行**，清理 SQL 在 `README.md` 末尾（不自动执行）。

## 四、本轮（2026-10-10，worktree worktree-d2-m3-fixtures）实际残留清单

- 已清理：`E:\tmp\m3-rng-shadow\`（上一轮的影子副本与记录，脚本化后不再需要常驻）
- 已清理：本线起的影子 `GameSrv.exe` 进程（按 `$_.Path -like '<会话目录>*'` 过滤结束，线上 oracle 未动）
- 保留并登记：`m3/sessions/m3rng-gate-20261010-164729/`（门禁 GREEN 的完整基线；快照已按默认瘦身为 manifest）
  与 `M3-gate-csharp-20261010-164729.json`（门禁 GREEN 报告）
- 保留并登记：RNG 记录流夹具 `m3/fixtures/rng_stream_seed42_*.tsv`（回放门禁的输入）
- 冻结基线校验：每次基线运行都对比 `E:\MirServer\M2GameSvr` 的 5 个关键文件 sha256
  （`pack_fingerprint` / `pack_unchanged`），本轮结果 `true`

## 五、清理命令（需要时）

```powershell
# 快照与影子（可再生）
Remove-Item -Recurse -Force mir2-rs/tests/parity/m3/sessions/*/snap_before, mir2-rs/tests/parity/m3/sessions/*/snap_after -ErrorAction SilentlyContinue
Remove-Item -Recurse -Force mir2-rs/tests/parity/m3/sessions/*/shadow-run* -ErrorAction SilentlyContinue
# 测试账号/角色（见 README 的清理 SQL，需先人工确认命中范围）
```
