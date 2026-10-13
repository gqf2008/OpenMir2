# evidence — M3 夹具 markers 前缀门：④ drop-exp 族漏登记收口 + check 反向 fail-closed + 接入链深检门

> 卡：`openmir2-m3-fixture-marker-prefix-bug`（owner `openmir2-worker-5`，worktree `m3cs`）。
> 本片 **tier=L**（纯离线夹具门/工具；零 `.rs`；零起栈/零端口；不 merge、未自签 review）。
> base = `origin/master` `ea4cd8c65`（本片唯一代码改动 = `mir2-rs/tests/parity/m3/**` 两个 `.py`）。
> 修前复现（`01`/`07`）在 base 的祖先 `266bdbd17` 上取（两版对 `mir2-rs/tests/parity/m3` 逐字节相同；
> `266bdbd17..ea4cd8c65` 的全部差量只在 `mir2-rs/tests/parity/world/**`，与本片无关）。

## 1. 问题（既存 RED，离线可复现）

在 master 上直接跑 `marker_prefix_gate.py selftest` ⇒ **rc=1 / 23/24**，失败牙 = `real-guards-wired`：

```
$ python -I marker_prefix_gate.py selftest
...
SELFTEST FAIL real-guards-wired
SELFTEST ok=23/24         # rc=1
```

根因：④ 夹具片（commit `6ab9b419d`，随 `openmir2-m3-drop-exp-fixture-botsrv` 合入 master）往
`run_pair.ps1` 的 `-DropExpSequence` 块加了第 7 个生产牙：

```
Assert-M3MarkerPrefix -Path $dropExpMarkersPath -Marker 'M3DROPEXPSEQ'
```

但没有把 `M3DROPEXPSEQ` 登记进 `marker_prefix_gate.py`。门的 `real-guards-wired` 要求
「`run_pair.ps1` 的牙集合 == 比较器集合」⇒ 7 个牙 ≠ 6 个比较器 ⇒ 红。

**为什么 `check` 仍是绿的（这才是真问题）**：`check` 只单向核对「每个 `COMPARATORS` 都有牙」，
**不检查反向的「每个牙都已登记」** ⇒ 新夹具族漏登记时 `check` 仍 GREEN，只有手工 `selftest`
才咬到。且 `marker_prefix_gate.py` **没有被 `fixture_chain_gate.py` 常跑** ⇒ 这条红既不在四门禁、
也不在链深检门里（修前链深检门 `check` 仍 `GREEN runner_ok=10`，见 `07`），只能靠人肉发现。

## 2. 改法（2 文件，不动产品代码语义）

### 2.1 `marker_prefix_gate.py`
1. 新增 **`GUARD_ONLY_MARKERS = ["M3DROPEXPSEQ"]`** 例外清单 + 注释：该族的 markers 流牙在
   `run_pair.ps1`，但其比较器 `drop-exp/drop_exp_diff.py` 比的是 BotSrv `-DropExpOut`
   **直写的 `drop_exp_raw.jsonl`**，不走 `line.find("<M3*SEQ> ")` 的 markers 流，故不进
   `COMPARATORS`。例外必须逐条登记。
2. **`check` 改 fail-closed**：`_evaluate` 新增反向核对 —— `run_pair.ps1` 出现的任何牙 Marker
   不在 `COMPARATORS ∪ GUARD_ONLY_MARKERS` ⇒ finding（`check` rc=1）。「新族漏登记」不再只靠
   `selftest` 才发现。
3. `selftest` 的 `real-guards-wired` 走新 helper `guards_wired()`，并新增两条牙：
   `guards-extra-unregistered-red`（多一个未登记族 ⇒ RED）与 `guards-guard-only-ok`（已登记例外 ⇒ 放行）。
4. `check` 的 GREEN 文案带出 guard-only 例外数。

### 2.2 `fixture_chain_gate.py`
把 markers 前缀门登记进 `SHARED_CHECKS`（`marker_prefix_selftest` + `marker_prefix_check`），与既有
castle / 行会攻城 / 英雄宠物三组主机门同形。它判的正是「多张夹具卡反复扩展 `run_pair.ps1` / BotSrv
后有没有互相踩坏」—— 本 bug 就是这类踩坏的实例。并加防回退牙「markers 前缀门已接入共享管线
（selftest + check 两向）」。

## 3. 判据读数（本片钉住的树 = `ea4cd8c65` + 本片）

| 判据 | 命令（cwd `mir2-rs/tests/parity/m3`） | 读数 |
|---|---|---|
| 门 check | `python -I marker_prefix_gate.py check` | GREEN：7 生产牙（含 1 guard-only 例外）+ 6 比较器按前缀扫描 |
| 门 selftest | `python -I marker_prefix_gate.py selftest` | **26/26 GREEN**（原 24 → 26） |
| 门 mutation | `python -I marker_prefix_gate.py mutation` | GREEN（PS 真身牙 good rc=0 / bad rc=1 + 三条变异体红） |
| 链深检门 selftest | `python -I fixture_chain_gate.py selftest` | **32/32 GREEN**（原 31 → 32） |
| 链深检门 check | `python -I fixture_chain_gate.py check --require-all` | **FIXTURE_CHAIN GREEN runner_ok=10 red=0**，含 `marker_prefix_selftest`/`check` 两项 |
| 四门禁 | fmt / clippy -D warnings / test / msgcodegen | fmt rc=0 / clippy rc=0 / **101 suites 1200 passed 0 failed 22 ignored** / 604 rc=0 |

## 4. 改坏必红（fail-closed 牙）

- **A（漏登记）**：把 `M3DROPEXPSEQ` 从 `GUARD_ONLY_MARKERS` 删掉（模拟新族漏登记）⇒ `check`
  **rc=1** 点名 `run_pair.ps1 生产牙 Marker 'M3DROPEXPSEQ'（$dropExpMarkersPath）未登记`；
  `selftest` **24/26**（`real-guards-wired` + `check-green` 双红）。见 `03`。
- **B（新族未登记）**：给 `run_pair.ps1` 追加一行未登记牙 `M3FUTURESEQ` ⇒ `check` **rc=1**
  点名 `M3FUTURESEQ`。见 `04`。两次改坏后均按字节还原（`run_pair.ps1` / `marker_prefix_gate.py`
  sha256 前后一致）。

## 5. 修前为什么没被自动化门抓住（登记）

- 修前链深检门 `check --require-all` = **GREEN runner_ok=10**，且输出里**没有任何 `marker_prefix`
  条目** ⇒ 该 bug 对链深检门完全不可见（`07`）。
- 修前 `marker_prefix_gate.py check` = **GREEN**（单向核对看不到漏登记），只有手工 `selftest` 红（`01`）。
- ④ 夹具片本身把 ④ 系统判为 `READY`/`RUN-OK`（它的 `run_drop_exp_pair.ps1 -SelfCheckNoStack` 不含
  markers 门的 selftest），所以链深检门逐系统一路绿。

⇒ 本片的两条修复（`check` 反向 fail-closed + 门接入链深检门）正好把这两个盲区都堵上。

## 6. 边界 / 纪律

- 净差量（`git diff --name-status ea4cd8c65 HEAD`）= `M mir2-rs/tests/parity/m3/fixture_chain_gate.py`、
  `M mir2-rs/tests/parity/m3/marker_prefix_gate.py` + 本证据目录 14 件。冻结面
  `src/ sql/ MirClient/ mir2-rs/crates/protocol/ mir2-rs/crates/storage/` **空**；`*.rs` = 0；
  `src/BotSrv/**` 未动。
- 只在本 worktree 写/提交/推；共享主检出 `E:/Users/gxh/Documents/GitHub/OpenMir2` 未动。
- 编译用 `CARGO_TARGET_DIR=E:/tmp/openmir2-m3cs-target`（检出外）。
- 零起栈、零占端口；未 merge、未自签 review。

## 7. 文件

| 文件 | 内容 |
|---|---|
| `00-commands.txt` | 生成/复核本证据包的命令序列 |
| `01-pre-fix-red.txt` | 修前：`check` GREEN（单向） + `selftest` 23/24 rc=1 |
| `02-post-fix-green.txt` | 修后：check GREEN + selftest 26/26 + mutation GREEN |
| `03-redcheck-unregistered-guard.txt` | 改坏 A（漏登记）⇒ check rc=1 / selftest 24/26 |
| `04-redcheck-unregistered-new-guard.txt` | 改坏 B（新族未登记）⇒ check rc=1 |
| `05-chain-gate-selftest.txt` | 链深检门 selftest 32/32 |
| `06-chain-gate-check.txt` | 链深检门 check（+本片）⇒ FIXTURE_CHAIN GREEN，含 marker_prefix 两项 |
| `07-chain-gate-check-pre-fix.txt` | 链深检门 check（修前）⇒ GREEN 但无 marker_prefix 条目（盲区） |
| `08-gate-fmt.txt` / `09-gate-clippy.txt` / `10-gate-test.txt` / `11-gate-msgcodegen.txt` | 四门禁 |
| `12-boundary.txt` | 净差量 + 冻结面 + sha256 |
| `README.md` | 本文件 |
| `SHA256SUMS.txt` | 仓库 LF/git-blob 口径清单 |
