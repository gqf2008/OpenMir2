# evidence-m3-rng-anchor-symbol-unique-20261014

卡 `openmir2-m3-rng-trace-scene-groups`（M3 夹具卫生片）——**承接卡规格锚点保鲜的根因收口**：
把行号锚从多车道共享的增长文件 `bins/game-svr/src/main.rs` 上撤下，改做「符号存在且唯一」机判。

> 写集：`mir2-rs/tests/parity/m3/**`；**零 `.rs`**；零起栈 / 零端口；共享主检出未动；
> 未 merge、未自签 review。base = `origin/master` `41e61029e`。

## 结论

1. **真身 RED（在本片 base 的干净检出上可复现）**：`rng-trace-scene/check_anchors.py` 是
   `DRIVER-CONTRACT.md` 的源码锚点保鲜门。base 上 `check` RED 1 项（`ANCHOR_DRIFT`：
   文档写 `main.rs:5665`，实际首个匹配 `:5795`）、`selftest` RED `12/13`，并因此把
   `fixture_chain_gate check` 拖成 **RED `red=3`**（`rng_scene_anchor_selftest` /
   `rng_scene_anchor_check` / runner `anchors-freshness-selftest` 三项）。
2. **根因＝行号锚钉在共享增长文件上**：`bins/game-svr/src/main.rs` 被 ④-D / client-e2e / ⑧-10 / ⑦
   等多条车道反复插入代码，该调用点行号已漂 **6 次** `5229 → 5440 → 5488 → 5598 → 5665 → 5795`。
   上一片（`1b88c9d3c`）只把 `:5598` 改成 `:5665`，随即被 `17389f05f` 合并推到 `:5795` ⇒ **只治标**。
3. **本片修法**：`check_anchors.py` 新增 **符号唯一性锚**（`symbol_unique=True`）——文档只引用文件
   路径、**不得**写 `:line`，机判 = 符号在该文件里**恰出现 1 次**。把 main.rs 这条锚由行号锚改过来。
   **判据不降级**：调用点被删 / 被复制仍必红（0 次 ⇒ `ANCHOR_SYMBOL_MISSING`，>1 次 ⇒
   `ANCHOR_AMBIGUOUS`）；只是把不稳定的行号换成稳定的唯一性。
4. `selftest` **13/13 → 16/16**（换牙：删「行号改回历史值」，加 4 条 —— 写回行号 / 删文件引用 /
   符号重复 / 符号缺失）；`fixture_chain_gate check` **RED → GREEN（`runner_ok=10 red=0`）**；
   `run_rng_trace_pair -SelfCheckNoStack` **37/37**（项数不变）。

## 判据 → 证据映射

| 判据 | 证据 |
| --- | --- |
| base 干净检出：`check` RED 1 项 / `selftest` 12/13 / chain RED red=3 | `02-base-red.txt` |
| after：`check` GREEN 7/7 / `selftest` 16/16 / chain GREEN red=0 | `03-after-green.txt` |
| 4 条新牙 + 真身文档改坏（`ANCHOR_LINE_NOT_ALLOWED` / `ANCHOR_MISSING`） | `04-mutation-red.txt` |
| 两类锚点口径（行号锚 / 符号唯一性锚）写入承接卡规格与 README | `01-change-summary.txt` 的改动表 |
| 四门禁 fmt/clippy/test/msgcodegen | `06`–`09`（test = 1214 passed / 0 failed / 22 ignored） |
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

- **这是「只治标 vs 根因」的分界**：上一片把行号改对，但没消除漂移的**成因**；本片消除成因
  （不再对共享增长文件做行号锚）。文档因此不再写 main.rs 的行号，读者按文件名 + 符号 grep 定位。
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
        check-pack --dir <EV> --since 41e61029e                                       # GREEN
