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

## 六、本轮（2026-10-11，M3-Rust 首验）实际残留清单

- **入库（证据，随 patch 提交）**：
  - `m3/sessions/M3-gate-csharp-20261011-142345.json`（C# 基线 GREEN，含 RNG `-VerifyRepeat`）
  - `m3/sessions/M3-gate-rust-20261011-143420.json`（Rust 固定建角 GREEN）、
    `m3/sessions/M3-gate-rust-20261011-143444.json`（Rust 默认 GREEN）
  - `m3/sessions/M3-gate-rust-20261011-143312.json`（缺陷 2 的红报告，作修复前后对照）
  - `m3/sessions/csharp-20261011-142347/`（C# 基线会话，~53KB）、
    `m3/sessions/rust-20261011-143315|143423|143447/`（Rust 三个会话，各 ~50KB）
  - `m3/sessions/m3rng-gate-20261011-142345/`（`product.json` + `drop_100kills.txt` + `exp_table.txt`）
- **不入库（已登记，`m3/.gitignore`）**：`sessions/m3rng-gate-*/rng-run{1,2}.tsv`（~2.2MB×2，可由
  `rng-baseline.ps1` 再生）、`sessions/rust-gates/`（Rust 三网关日志）、`sessions/rust-gates.pid.json`（本机 PID）。
- **无影子副本残留**：`rng-baseline.ps1 -VerifyRepeat` 的 `shadow-run*` 已自动清理。
- **冻结基线**：`pack_unchanged=true`（`E:\MirServer\M2GameSvr` 5 个关键文件 sha256 前后一致）；
  `src/**` 一行未改。收工停栈：`start_rust_gates.ps1 -Stop` + `E:\MirServer\start-all.ps1 -Stop`。
- **测试账号/角色**：本轮新增 `m3142347 / m3143315 / m3143423 / m3143447` 四个命名空间各 4 行
  （account / account_protection / characters / characters_indexes），清理 SQL 见 `README.md` 末尾（不自动执行）。


## 七、本片（2026-10-11，worktree `m3bag`：装备/能力接缝）实际残留清单

- **入库（证据，随 patch 提交）**：
  - `m3/fixtures/ability_vectors.json`（oracle 真身探针产出的 `WAbil`/`SubAbility`/feature 向量，
    ~19 模板 / 23 用例；Rust 用例回读逐字段核对）
  - `m3/ability-probe/`（探针源码 + csproj；不连 MySQL、不起服务、不写现网文件）
- **未入库/临时物**：探针的 `bin/`、`obj/`（构建产物，已由 `m3/.gitignore` 忽略；可用上面的命令再生）；
  隔离构建目录 `E:/tmp/openmir2-m3bag-target`（登记位置，可删）。
- **未起栈、未占端口**（5000/7000/7100/7200 未使用）；`E:/MirServer` 十端口状态与开工前一致。
- **冻结基线**：`src/**`（C# oracle）一行未改；表结构/脚本/数据文件格式未改。
