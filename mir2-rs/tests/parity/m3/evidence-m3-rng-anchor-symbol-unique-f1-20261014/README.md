# evidence-m3-rng-anchor-symbol-unique-f1-20261014

卡 `openmir2-m3-rng-trace-scene-groups`（M3 夹具卫生片）——**reviewer-5 F-1 收口**：
符号唯一性锚的计数口径由「匹配**行**数」改为「匹配**总次数**」。

> 写集：`mir2-rs/tests/parity/m3/**`；**零 `.rs`**、零 `.toml`；零起栈 / 零端口；共享主检出未动；
> 未 merge 被审以外的分支、未自签 review。
> base = `origin/master 515c6b5ea`；**声明 head（code head）= `15fac97ac`**（见 `10-boundary.txt`）。

## 结论

1. **F-1 是真缺陷（假绿通道）**：上一片把 `main.rs` 的锚从行号锚改成符号唯一性锚（reviewer-5
   `256e79553a` approve 时携带 F-1）。旧实现 `n = sum(1 for line in lines if re.search(a["rx"], line))`
   数的是**匹配行数** —— 若期望符号在同一物理行写两遍（`spawn(); spawn();`），只计 1 次 ⇒
   `n == 1` 放行、`ANCHOR_AMBIGUOUS` 不触发。现实 Rust 形状不可达（调用点独占一行），但这是门里
   一条**真实的静默通道**，且修复成本极低。
2. **本片修法**：改为 `n = sum(len(re.findall(a["rx"], line)) for line in lines)` ⇒ 计**匹配总次数**；
   同一物理行两遍符号 ⇒ n = 2 ⇒ `ANCHOR_AMBIGUOUS` 必红（0 次仍 ⇒ `ANCHOR_SYMBOL_MISSING`）。
3. **改坏必红有牙**：新增 `mutation-symbol-ambiguous-same-line-red`（临时源文件写 `spawn(); spawn();`，
   断言 `ANCHOR_AMBIGUOUS` 点名该文件）。把修法**还原成按行计数**后跑同一 selftest ⇒
   **RED 16/17**，恰点名该用例（`04-mutation-red.txt`）——即「旧实现对本用例假绿」被同一条牙当场证明。
4. `selftest` **16/16 → 17/17**（含 15→16 条改坏必红）；`fixture_chain_gate check` 仍 GREEN
   `runner_ok=10 red=0`、`selftest 40/40`；`run_rng_trace_pair -SelfCheckNoStack` 仍 **37/37**
   （该项只断言 `check_anchors selftest` rc=0 与 a/a 相等，不锁死数字 ⇒ 未被本次计数改动绊倒）。

## 判据 → 证据映射

| 判据 | 证据 |
| --- | --- |
| base（旧口径）selftest GREEN 16/16 —— 但对同行两遍符号假绿 | `02-before-selftest.txt` |
| after：`check` GREEN 7/7 / `selftest` 17/17（含 16 条改坏必红） | `03-after-green.txt` |
| 改坏必红：把修法还原成按行计数 ⇒ RED 16/17，点名 `mutation-symbol-ambiguous-same-line-red` | `04-mutation-red.txt` |
| 夹具链深检门不被绊倒：`selftest 40/40` + `check GREEN runner_ok=10 red=0` | `05-fixture-chain.txt` |
| 两侧驱动无栈自检 37/37（`anchors-freshness-selftest` 绿） | `11-rng-runner-selfcheck.txt` |
| 四门禁 fmt/clippy/test/msgcodegen | `06`–`09`（test = 101 suites 1219 passed / 0 failed / 22 ignored） |
| 边界 / 冻结面 / 零 `.rs` / 声明 head | `10-boundary.txt` |

## 改坏必红原始输出（摘，见 `04-mutation-red.txt`）

    改坏：把 n = sum(len(re.findall(...)) ...) 还原成 n = sum(1 for line ... if re.search(...))
      RED mutation-symbol-ambiguous-same-line-red
      SELFTEST RED: check_anchors selftest 16/17
      rc=1

    修法在位时（03-after-green.txt）：
      ok  mutation-symbol-ambiguous-same-line-red（ANCHOR_AMBIGUOUS 改坏：符号在同一物理行出现两次：
          crates/world/src/dup_same_line.rs 里 /spawn\(\)/ 出现 2 次 ⇒ 无法判定）
      SELFTEST GREEN: check_anchors selftest 17/17

## 如实登记

- **现实可达性**：本片修的是一个**当前形状不可达**的假绿通道（真实 `main.rs` 的调用点独占一行）。
  按 reviewer-5 F-1 的建议「下次动此文件时改为统计匹配总次数」执行；修法与牙都是小、可判的。
- **未动** `run_rng_trace_pair.ps1` 的断言逻辑（只订正一处注释里的旧数字），故 `-SelfCheckNoStack`
  项数与结果不变（37/37）。
- 卡级 R 真机 DoD（同场景含怪两相 trace + `groupdiff --fold --strict` site/order/折叠计数 diff=0）
  仍待承接卡 `openmir2-m3-rng-scene-driver` + owner 独占窗口，**不据此收卡**。
- EOL/清单口径：按**仓库字节（git blob / LF 归一）**记；`verify --check-extra` / `eolcheck` /
  `check_evidence_head check-pack` 读数见 `00-commands.txt` §5。

## 复核（本片实跑；作者 worktree；零起栈 / 零端口）

    $ python -I tools/evidence-hash/verify_sha256sums.py verify --check-extra <EV>   # GREEN
    $ python -I tools/evidence-hash/verify_sha256sums.py eolcheck <EV>               # GREEN
    $ python -I mir2-rs/tests/parity/m3/rng-trace-scene/check_evidence_head.py \
        check-pack --dir <EV> --since 515c6b5ea                                      # GREEN
