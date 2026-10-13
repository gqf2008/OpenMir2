# evidence-m3-marker-prefix-20261013

卡 **`openmir2-m3-fixture-marker-prefix-bug`**（issue `13993f9447f8e0559b00fe6a18b59e0102d8deb0`，
发现者 = 真机窗口车道 `openmir2-svc-1`）的证据包。worktree `m3cs`，分支 `worktree-m3-marker-prefix`，
base = `origin/master d4d5b89c73e77ed995188c2668dc3eda9a926dc0`。**零 `.rs`、零 `src/**`、未起栈、未占端口**
（写集只在 `mir2-rs/tests/parity/m3/**`）。

## 缺陷与口径

- **写侧**：`run_pair.ps1` 原来的 4 处 markers 写入把 `M3*SEQ {...}` 事件**去掉前缀**再落 `*_markers.jsonl`
  （对捕获行做 `Substring('<M3*SEQ> '.Length)`）。
- **读侧**：4 个比较器（`npc_shop_diff.py` / `quest_diff.py` / `party_trade_diff.py` / `selloff_diff.py`）的
  `_marker_events` **只认带前缀行**（`line.find("M3*SEQ ")`）⇒ 裸 JSON 收集 0 条 ⇒ `assemble` 恒
  `PREREQ_RED`(rc=2) ⇒ ⑤商店/⑥任务等**已实现**系统的真机判决被夹具阻断（不是实现差异）。
- **owner 裁决口径（写侧统一为带前缀）**：markers 文件 = `bot.log` 内**带前缀**的原始事件行。
  理由：① 前缀是同一份事件流在 `bot.log` / 会话目录两处的**唯一稳定标识**；② 只改 `run_pair.ps1` 一处即可
  与 4 个比较器（及 ⑧/⑨ 的同族比较器）对齐，不动比较器判据、不放宽 fail-closed；③ 历史残留的裸 JSON 会
  被 fail-closed 牙直接判红，不会静默混入跨侧比较。

## 修法

1. `run_pair.ps1`：4 处 `$*Events = @($*SeqMarkers)` **保留前缀**（删去 `Substring(...)` 截断）。
2. `run_pair.ps1`：新增 `Assert-M3MarkerPrefix` **生产 fail-closed 牙**，4 个序列写后各调用一次 ——
   任何非空行缺 `'<M3*SEQ> '` 前缀 ⇒ 当场 `exit 1`（不给去前缀的 markers 进比较器的机会）。
3. `marker_prefix_gate.py`：**机判门 + 自检 + 真实变异**，防止该写法/牙被后续片静默改回。

## 文件

| 文件 | 内容 | 生成命令 |
| --- | --- | --- |
| `01-code-change-diff.txt` | `run_pair.ps1` 相对 master 的完整改动 | `git diff -U3 master -- mir2-rs/tests/parity/m3/run_pair.ps1` |
| `02-gate-selftest.txt` | 前缀口径门自检 **15/15**（含真实变异两条红） | `python -I marker_prefix_gate.py selftest` |
| `03-gate-check-green.txt` | 前缀口径门 `check` GREEN | `python -I marker_prefix_gate.py check` |
| `04-gate-mutation-red.txt` | **改坏必红**：两个真实变异体 + PowerShell 真身牙 | `python -I marker_prefix_gate.py mutation --out <dir>/mutation` |
| `05-comparator-selftest-npc_shop_diff.txt` | `npc_shop_diff.py` selftest **27/27** | `python -I npc_shop_diff.py selftest` |
| `06-comparator-selftest-quest_diff.txt` | `quest_diff.py` selftest **36/36** | `python -I quest_diff.py selftest` |
| `07-comparator-selftest-party_trade_diff.txt` | `party_trade_diff.py` selftest **27/27** | `python -I party_trade_diff.py selftest` |
| `08-comparator-selftest-selloff_diff.txt` | `selloff_diff.py` selftest **33/33** | `python -I selloff_diff.py selftest` |
| `09-real-artifact-bare-prereq.txt` | 真机原始件（去前缀形态）⇒ `assemble` `PREREQ_RED` rc=2 | 见文件内 `$` 行 |
| `10-real-artifact-prefixed-assembled.txt` | 同一事件流补回前缀 ⇒ `assemble` ok rc=0 | 见文件内 `$` 行 |
| `11-gate-fmt.txt` | 门禁 fmt rc=0 | `cargo fmt --all -- --check` |
| `12-gate-clippy.txt` | 门禁 clippy rc=0 | `cargo clippy --offline --workspace --all-targets -- -D warnings` |
| `13-gate-test.txt` | 门禁 test **94 suites / 896 passed / 0 failed / 21 ignored** rc=0 | `cargo test --offline --workspace` |
| `14-gate-msgcodegen.txt` | 门禁 msgcodegen **604** rc=0 | `cargo run --offline -p msgcodegen -- --check` |
| `mutation/` | `04` 命令写出的两个真实变异体 + PowerShell 牙脚本与样例 markers | 产物，供复核 hash |
| `real-artifact/` | 真机原始件（`shop-bare.jsonl`）与补前缀件（`shop-prefixed.jsonl`）+ 来源说明 | 见 `real-artifact/provenance.txt` |
| `SHA256SUMS.txt` | 23 件证据的**仓库字节口径**（LF 归一 / git blob）sha256 | `verify_sha256sums.py write` |

## 复现（离线，零端口）

```bash
# 前缀口径门（扫 run_pair/比较器 + 运行时牙）
python -I mir2-rs/tests/parity/m3/marker_prefix_gate.py check
python -I mir2-rs/tests/parity/m3/marker_prefix_gate.py selftest
# 改坏必红（真实变异 run_pair.ps1 副本 + PowerShell 真身牙；变异体写 --out）
python -I mir2-rs/tests/parity/m3/marker_prefix_gate.py mutation --out /tmp/m3-marker-prefix-mutation
# 4 个比较器自检
python -I mir2-rs/tests/parity/m3/npc_shop_diff.py selftest
python -I mir2-rs/tests/parity/m3/quest_diff.py selftest
python -I mir2-rs/tests/parity/m3/party_trade_diff.py selftest
python -I mir2-rs/tests/parity/m3/selloff_diff.py selftest
# 真机原始件的 assemble 两态
python -I mir2-rs/tests/parity/m3/npc_shop_diff.py assemble <EV>/real-artifact/shop-bare.jsonl --side csharp   # ⇒ rc=2
python -I mir2-rs/tests/parity/m3/npc_shop_diff.py assemble <EV>/real-artifact/shop-prefixed.jsonl --side csharp # ⇒ rc=0

# 四门禁（隔离 target，--offline）
CARGO_TARGET_DIR=E:/tmp/m3cs-marker-target cargo fmt --all -- --check
CARGO_TARGET_DIR=E:/tmp/m3cs-marker-target cargo clippy --offline --workspace --all-targets -- -D warnings
CARGO_TARGET_DIR=E:/tmp/m3cs-marker-target cargo test --offline --workspace
CARGO_TARGET_DIR=E:/tmp/m3cs-marker-target cargo run --offline -p msgcodegen -- --check

# 证据清单（仓库字节口径）与 EOL 门
python -I tools/evidence-hash/verify_sha256sums.py --check-extra --exclude README.md <EV>
python -I tools/evidence-hash/verify_sha256sums.py eolcheck <EV>
```

`<EV>` = `mir2-rs/tests/parity/m3/evidence-m3-marker-prefix-20261013`。实测：verify **GREEN 23/23**、eolcheck **GREEN 23/23**。

## 关键读数

- **改坏必红（③）**：`run_pair_strip.ps1` 变体 ⇒ 门 RED 且点名 `run_pair.ps1:709 仍有去前缀截断`；
  `run_pair_noguard.ps1` 变体 ⇒ 门 RED 点名缺 `M3SHOPSEQ` 牙；PowerShell 真身牙 bad ⇒ rc=1 且打印「缺前缀」。
- **运行时牙（②）**：同一合法事件流 带前缀 ⇒ `assemble` rc=0；裸 JSON ⇒ rc=2；空文件 ⇒ rc=2
  （证明「前缀」是 assemble 成立的必要条件，且 4 个比较器的 PREREQ 语义未被削弱）。
- **真机原始件（09/10）**：窗口车道 commit `b25e595d` 的 11 件（去前缀，14 行）⇒ `PREREQ_RED`；
  逐行补回 `M3SHOPSEQ ` 前缀（内容字段一字未改）⇒ `ASSEMBLED ok ops=5`。⇒ **前缀是唯一的阻断变量**。

## 边界与登记（不在本片修，交 owner 转派）

窗口车道同卡 comment（`cf3334343550f62dc5719de1bf1afe5177b2ab06`）另报三项，**与本片同源但不是前缀**，
本片**如实登记、不自行开工**（按 svc-1 建议：本卡修 ①，其余另开/承接）：

1. **defect_2（同前缀混入非 JSON 诊断行）**：Rust 侧 `M3SHOPSEQ op=sell skip=no_npc_in_view` /
   `M3QUESTSEQ skip=... step=...` 与事件行同前缀，比较器 `_marker_events` 一律 `json.loads` ⇒ 非法 JSON。
   修法二选一（写侧换独立前缀如 `M3SHOPSEQ_SKIP`，或读侧跳过非 `{` 行）——**写侧在 BotSrv，读侧在
   比较器**；本片未动，请 owner 定承接卡。
2. **defect_3（C# 侧任务 k=0 frame JSON 截断）**：`{=2/}=1`、len=457，疑长 NPC 说词写入截断（写侧在 BotSrv）。
3. **`quest_state.jsonl` 无生产者**（`WRITER_FOUND=0`）⇒ ⑥ 仍会 `PREREQ_MISSING`（`run_quest_pair.ps1:526`）。
4. ⑧（公会/攻城）与 ⑨（英雄/宠物）的夹具比较器**尚未合入**；一旦合入，把它们加进
   `marker_prefix_gate.py` 的 `COMPARATORS` 表即可被同一道门咬住（同族口径）。

> 本片**未起栈、未占窗口**，因此**未**在真机上重跑 `run_pair.ps1 -Side csharp`（⑤/⑥ 端到端）。
> svc-1 已在真机上用临时改法验证前缀修复方向（见同卡 comment 的 `verified_prefix_fix`）；
> 端到端复跑等下一轮真机窗口。

**残留物清单**：本目录 14 件 `.txt` + `real-artifact/`（5 件）+ `mutation/`（5 件）+ `SHA256SUMS.txt`
（23 行）+ 本 README；`marker_prefix_gate.py` 1 件新工具、`run_pair.ps1` 1 件改动。未起栈、未占端口、
未动 `crates/**` / `bins/**` / `src/**`。
