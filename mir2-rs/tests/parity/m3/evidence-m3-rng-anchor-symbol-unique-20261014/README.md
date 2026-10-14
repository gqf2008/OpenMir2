# evidence-m3-rng-anchor-symbol-unique-20261014

卡 `openmir2-m3-rng-trace-scene-groups`（M3 夹具卫生片）——**承接卡规格锚点保鲜的根因收口**：
把行号锚从多车道共享的增长文件 `bins/game-svr/src/main.rs` 上撤下，改做「符号存在且唯一」机判。

> 写集：`mir2-rs/tests/parity/m3/**`；**零 `.rs`**；零起栈 / 零端口；共享主检出未动；
> 未 merge 被审以外的分支、未自签 review。
> 首次交付 base = `41e61029e`；**再交付（合并 origin/master 后）base = `9edfc745d`，head = 见 `10-boundary.txt` 的声明值**。

## 结论

1. **真身 RED，且病因复发**：`rng-trace-scene/check_anchors.py` 是 `DRIVER-CONTRACT.md` 的源码锚点
   保鲜门。该门把宿主 AI 调用点钉成 `main.rs:<line>` 行号锚；`main.rs` 被多条车道反复插代码，行号已漂
   **7 次** `5229 → 5440 → 5488 → 5598 → 5665 → 5795 → 5817`。
   - 首次交付 base `41e61029e`：`check` RED 1 项、`selftest` RED `12/13`，并把 `fixture_chain_gate check`
     拖成 **RED `red=3`**（`02-base-red.txt`）。
   - 链门车道把同一锚「再钉一次」（`:5665`→`:5795`）后，**同一轮 master** 又被 `ca10f4a87` 推到
     `:5817` ⇒ 再交付 base `9edfc745d` **仍 RED**（`05-base-newmaster-red.txt`）——band-aid 不到一轮即失效。
2. **本片修法（根因）**：`check_anchors.py` 新增 **符号唯一性锚**（`symbol_unique=True`）——文档只引用
   文件路径、**不得**写 `:line`，机判 = 符号在该文件里**恰出现 1 次**（0 次 ⇒ `ANCHOR_SYMBOL_MISSING`，
   >1 次 ⇒ `ANCHOR_AMBIGUOUS`）。把 `main.rs` 这条锚由行号锚改过来。**判据不降级**：调用点被删 /
   被复制仍必红（与行号漂移同样是 fail-closed），只是把不稳定的行号换成稳定的唯一性；其它锚点
   （稳定文件）仍维持更强的行号锚。
3. `selftest` **13/13 → 16/16**（换牙：删「行号改回历史值」，加 4 条 —— 写回行号 / 删文件引用 /
   符号重复 / 符号缺失）；`fixture_chain_gate check` **RED → GREEN（`runner_ok=10 red=0`）**；
   `run_rng_trace_pair -SelfCheckNoStack` **37/37**（项数不变），`run_drop_exp_pair` **20/20**。
4. **再交付的合并解**：本片与链门车道的 band-aid 在 `DRIVER-CONTRACT.md` 同一行冲突；取根因侧，
   并把链门车道新增的 fmt 证据门 / `.py` 下界 `≥23` 原样保留（`13-merge-resolution.txt`）。

## 判据 → 证据映射

| 判据 | 证据 |
| --- | --- |
| 首次 base `41e61029e`：`check` RED / `selftest` 12/13 / chain RED red=3 | `02-base-red.txt` |
| 再交付 base `9edfc745d`：第 7 次漂移 `:5795`→`:5817`，仍 RED | `05-base-newmaster-red.txt` |
| after：`check` GREEN 7/7 / `selftest` 16/16 / chain GREEN red=0 / chain selftest 40/40 | `03-after-green.txt` |
| 合并冲突与解（同物理行两套改法 ⇒ 取根因 + 并集保留） | `13-merge-resolution.txt` |
| 4 条新牙 + 真身文档改坏（`ANCHOR_LINE_NOT_ALLOWED` / `ANCHOR_MISSING`） | `04-mutation-red.txt` |
| 两类锚点口径（行号锚 / 符号唯一性锚）写入承接卡规格与 README | `01-change-summary.txt` 的改动表 |
| 四门禁 fmt/clippy/test/msgcodegen | `06`–`09`（test = 101 suites 1219 passed / 0 failed / 22 ignored） |
| 两侧驱动无栈自检 37/37、20/20 | `11`、`12` |
| 边界 / 冻结面 / 零 `.rs` / 声明 head | `10-boundary.txt` |

## 改坏必红原始输出（摘，见 `04`）

    改坏 A：把符号唯一性锚写回 `main.rs:5795`（行号锚）
      RED ANCHOR_LINE_NOT_ALLOWED …… `mir2-rs/bins/game-svr/src/main.rs` 是**符号唯一性锚**
          （共享增长文件不做行号锚，行号必漂）⇒ 文档不得写 `…:<line>`，只写文件路径
      rc=1
    改坏 B：删掉该锚的文件引用 ⇒ ANCHOR_MISSING（rc=1）
    selftest 内联 4 牙：mutation-symbol-line-ref-red / -ref-removed-red /
          -ambiguous-red（符号重复）/ -missing-red（符号缺失）全 ok（即改坏必红）

## 如实登记

- **「治标 vs 根因」的分界**：链门车道把行号改对（`:5665`→`:5795`），但没消除漂移的**成因**；实测
  该值在同一轮 master 内即再次失准（`:5817`）。本片消除成因（不再对共享增长文件做行号锚）。
- **残留局限**：其它**行号锚**（`mongen.rs` / `monai.rs` / oracle `AnimalObject.cs` /
  `rng_consumer_groups.py` / `consumer_groups.tsv`）仍按「行号 == 首个匹配行」判。它们要么是冻结面，
  要么在本卡写集内、改动远少于 main.rs ⇒ 维持行号锚（更强）。若将来这些文件也变成多车道热点，
  同一 `symbol_unique` 牙可直接复用。
- **更强的一步（需 owner 裁写集，本片未做）**：`mir2-rs/tests/parity` 是 workspace crate
  `mir2-parity-tests`（有 `tests/*.rs`，随四门禁的 `cargo test --workspace` 跑）。把
  `check_anchors.py selftest`（或 `fixture_chain_gate check`）接成一个 Rust 集成测试，即可让锚点漂移
  **在合并门禁**上就红 —— 那才是「不静默」的完全体。但该文件在 `mir2-rs/tests/parity/tests/**`，
  **超出本卡写集**（本卡只许 `m3/**`）⇒ 只登记建议，不在本片夹带。
- 卡级 R 真机 DoD（同场景含怪两相 trace + `groupdiff --fold --strict` site/order/折叠计数 diff=0）
  仍待承接卡 `openmir2-m3-rng-scene-driver` + owner 独占窗口，不据此收卡。
- EOL/清单口径：按**仓库字节（git blob / LF 归一）**记；`verify --check-extra` / `eolcheck` /
  `check_evidence_head check-pack` 读数见 `00-commands.txt` §5 与 `10-boundary.txt`。

## 复核（本片实跑；作者 worktree；零起栈 / 零端口）

    $ python -I tools/evidence-hash/verify_sha256sums.py verify --check-extra <EV>   # GREEN
    $ python -I tools/evidence-hash/verify_sha256sums.py eolcheck <EV>               # GREEN
    $ python -I mir2-rs/tests/parity/m3/rng-trace-scene/check_evidence_head.py \
        check-pack --dir <EV> --since 9edfc745d                                       # GREEN
