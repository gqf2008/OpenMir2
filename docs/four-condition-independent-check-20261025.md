# 四条件独立复核（2026-10-25，快照 `2135f6154`）

- 卡：`openmir2-four-condition-independent-check`（tier **L**；owner `openmir2-reviewer-20`；branch `review/four-condition-check`）。
- 基线：本地 `master = 2135f6154`（2026-10-25；walgit 仍故障，本地主源）。
- 写集：**只新增本文件**；未改代码/规则/卡状态；未起服务端、未占端口；`E:/MirServer` 只读（`parse-stats` 只读 `Envir`）。
- 口径：**先自己算，再对照** `docs/three-day-report-20261025.md` 与 `docs/rcard-registration-audit-20261025.md`；
  不一致全部显式列出（见 §6）。**登记型 PASS ≠ 已有真机证据**；R 桶是活量，本文数字都带时点。

## 0. 一句话结论

四条件在本快照上的**机判结果**：① **pass**；② **need_human**（唯一 `need_human` 仍是 `openmir2-gm-batch-5b-cross-server`/G11）；
③ **pass**；④ **need_human**（`cargo_test_workspace` 未由我重跑、`master_sync` 因 walgit 500）。
**另有 4 条与「全部通过」叙事不一致的发现**（§6）：3 条是 goal_gate 的**分桶口径**会把「非严格 R 卡」计入 `closed`
（含 1 张已降级为 L、1 张无代码改动的分诊卡），1 条是**主源级的台账门红**（`script_host_noop_audit --strict-lines` / `selftest`
在 master 上 rc=1，根因 = castle-official `778f30a2c` 给 `engine/mod.rs` 插了 13 行却未重钉台账）。

## 1. 条件① 脚本派发 100%

- 命令（release 二进制在本检出**现构建**：`cargo build --release -p mir2-script-tool`，4.3s；旧 release 是 10-24 23:44 的、会漏掉之后的引擎改动）：

```sh
mir2-rs/target/release/mir2-script-tool.exe parse-stats E:/MirServer/M2GameSvr/Envir \
    --json E:/tmp/fourcond-parse.json --require-implemented
```

- 实测（rc=0）：`文件: 639 加载: 639 缺失: 0 异常: 0`；`解析错误: 427；解码异常文件: 0`；
  **`语料派发目标覆盖：已实现 106 个（11829 次调用）；未实现 0 个`**。
- goal_gate（`--script-bin` 指同一二进制）：**condition 1 = pass**，detail `parse-stats 实测：目标 106/106（去重派发目标）；调用 11829 次；未实现 0 个` ⇒ 与三天报告的值**一致**。

## 2. 条件② R 卡证据（分桶 + 抽样自证）

命令：`python -I tools/goal/goal_gate.py --skip-tests --script-bin <release tool> --json`（`generated_ts` 见 JSON；本快照 `2135f6154`）。

```text
R 卡 213 张（快照 @ 2135f6154）：已完成 12 张 / window_required 147（另未合并 2）/ whitelist 48 / 已裁定登记 3 /
merged 未 closed 且无登记(需人工) 1 / 未开工或未合并 0 / 其它需人工 0
merged 未 closed 但已凭登记 PASS：196 张（卫生清单；登记型 PASS ≠ 已有真机证据）
```

- 唯一 `need_human` = `openmir2-gm-batch-5b-cross-server`（merged 但 board=blocked，G11 未实施）。
- **抽样 10 张自证（每桶 2；`merged_not_closed` 全场只有 1 张，故补 1 张 `closed`）**：

| # | 卡 | goal_gate 桶 | 我的独立判断（据 `walgit collab thread`） | 一致? |
|---:|---|---|---|---|
| 1 | `openmir2-dualrun-triage-enterworld-frames` | closed | board closed（10-23 01:31）；关闭理由 = 4 类帧判为**时序差异、非缺陷、`code_changed=false`** ⇒ 「closed」成立，但它是**无代码改动的分诊卡**，不是「R 实现已验」 | 桶 ✓ / 语义见 §6-C |
| 2 | `openmir2-encoding-callsites` | closed | board closed（10-21 17:42，merge 10707b11b）；但 10-20 的 approve review **`window_required=true`**，10-21 合并记录明写 **「L 卡可 closed」** ⇒ 该卡**已降级为 L** | 桶 ✓ / 语义见 §6-A/B |
| 3 | `openmir2-gate-gmclose-race` | closed | 10-25 按 `merged-not-closed-5-sweep` 裁定关闭，证据链 = approve `window_required=false` + merge_result + 三条独立 closed 建议 ⇒ 合法 closed | ✓ |
| 4 | `openmir2-a09-upgrade-item` | window_required | approve review（10-21 18:56）`wr=true`；merge_result merged；last status in-progress ⇒ 窗口欠账真实 | ✓ |
| 5 | `openmir2-a22-castle-persistence` | window_required | approve review（10-22 00:07）`wr=true`；merged；in-progress | ✓ |
| 6 | `openmir2-active-seek-race-missing` | whitelist_t | `whitelist.md` T-252 明写「卡 `openmir2-active-seek-race-missing`（tier=R）」；merged；无 wr | ✓ |
| 7 | `openmir2-aoi-verify-maptime` | whitelist_t | `whitelist.md:924` T-235 点名该卡；merged | ✓ |
| 8 | `openmir2-m3-fixture-marker-prefix-bug` | adjudicated | 最新 merge（10-14 04:10）`tier=L` + `wr=false`；最新 approve `5f1be10c` 带免窗口理由（纯离线夹具/工具片，`*.rs=0`）⇒ 已裁定不需窗口 | ✓ |
| 9 | `openmir2-m3-pet-slave-summon` | adjudicated | 最新 approve `wr=false` + 理由「纯仓库卫生/属性口径片（只动 .gitattributes 与证据目录）」；虽有历史 `wr=true` 条目，按「最新 approve 裁定」归 adjudicated | ✓（规则口径） |
| 10 | `openmir2-gm-batch-5b-cross-server` | merged_not_closed | merge `99a4d30c4` merged=true，board=blocked，无 wr、无 approve ⇒ 唯一 need_human，合理 | ✓ |

> 结论：**10/10 的桶归属都能由线程事实复现**；不一致仅在「`closed` 桶的语义」（§6-A/B/C），不在分桶算法本身。

## 3. 条件③ B 类清零

- 命令：`python -I mir2-rs/tests/parity/coverage/recon_gates.py dispatch`（rc=0）。
- 实测末行（快照 @ 2135f6154）：`summary: {"n/a-oracle-noop": 1, "implemented": 179, "n/a-unreachable": 11, "tracked": 0}` ⇒ **tracked 桶为 0 条**。
- goal_gate condition 3 = **pass**（快照 @ 2135f6154；`recon tracked 0（rows 191 条非空、counts 一致）`）⇒ 与三天报告一致。

## 4. 条件④ 门禁恒绿

- goal_gate 的「7 项」= `fmt` / `clippy` / `msgcodegen` / `whitelist_anchor_gate` / `m3_whitelist_registry_audit` /
  `cargo_test_workspace` / `master_sync`。本次 `--skip-tests`：**绿 5 / 红 0 / 需人工 2**（`cargo_test_workspace` 未跑、`master_sync` walgit 500）。
- 我在 `2135f6154` 上亲跑：`cargo fmt --all -- --check` rc=0；`cargo clippy --workspace --all-targets --locked -- -D warnings` rc=0；
  `cargo run --locked -p msgcodegen -- --check` rc=0（604 条）；**7 项卫生门**（t_reservation 255 行 / conflict 0 / stale `--ref master` OK /
  anchor 590 ALL PASS / m3 GREEN / dynamic `--ref master` 0 缺戳 / board 618 卡）**全 rc=0**；`merge_tree_helper --selftest` 12/12。
- **`cargo test --workspace`：我未重跑**（内存政策）。引用 tester 在 **`2eb147e6e`** 的分包全量结论：
  **141 suites / 3237 passed / 0 failed / 62 ignored，rc=0、未被 reaper 终止**（`openmir2-window-run-2` 线程 comment `b518f629`）。
  其后 master 前进 7 笔（`9944d58f1` gamepoint / `b42a8701d` butch / `15ba925dd` runbook / `700d1f701` clearmapmon 场景 /
  `e872fb83c` runbook 修正 / `97eb5365d` probe-assets / `2135f6154` runbatch），逐笔核对 `git diff --name-only <m>^1 <m>`：
  **只有 `b42a8701d` 触及 `mir2-rs/bins/game-svr/src/lib.rs`，且新增的两条都是 `#[cfg(test)] mod tests` 里的 `#[test]`；其余全部是 docs/tools/tests-parity** ⇒ **无生产代码变更**，3237 的结论对该快照后仍具代表性（预计 +2 条测试，未由我复跑）。
- **但**：仓内另有**台账门** `mir2-rs/tests/parity/coverage/script_host_noop_audit.py`：
  - `check`（宽容行号）rc=0；
  - **`check --strict-lines` rc=1（229 处不一致）**，形态全部是 `engine/mod.rs` 的 `mod_line` 与 `fn` 声明行**差 +13 行**；
  - **`selftest` rc=1**（其内部断言真台账 strict-lines 必须为 0）。
  - 根因：`778f30a2c`（castle-official R 卡）给 `engine/mod.rs` 插入 `take_say_abort`（含注释 13 行）**没有重钉台账**
    （台账最后一次更新是 `4ed9fdf72`）。⇒ 见 §6-D。

## 5. 与两份既有文档对照

| 文档 | 快照 | ① | ② | ③ | ④ | 与本快照的关系 |
|---|---|---|---|---|---|---|
| `docs/three-day-report-20261025.md` | `1c605ca4a` | pass 106/106 | 213 = closed 12 / wr **145**（另未合并 2）/ whitelist 48 / adjudicated 3 / merged_not_closed 1 / **未开工 2** | pass | 绿 5 / 需人工 2 | 口径与结论**一致**；我只是快照晚、**桶内活量漂移**（wr 145→147、未开工 2→0，总数仍 213） |
| `docs/rcard-registration-audit-20261025.md`（§8 后续变更） | `1c605ca4a` | — | 213 = 12 / wr 145（另未合并 **3**）/ 48 / 3 / 1 / **not_merged 1** | — | — | **§8 的预测被证实**：`castle-official-actions`（not_merged→wr）与 `script-handler-msgqueue`（unmerged→wr）合并后，正是 145+3+1 → **147+2+0** |

- 两份文档的「登记型 PASS ≠ 真机证据」表述与本复核一致；三天报告的 194 张 hygiene 与现在的 196 张属同一口径的活量。

## 6. 不一致 / 需要登记的四条（**本文最有价值的部分**）

### A. `closed` 桶先于 `window_required` 判定（goal_gate `r_category` 顺序）
`r_category` 的第一条就是 `if ev["board_closed"]: return "closed"` ⇒ **board 已 closed 的卡即使带 `window_required=true` 也计入 `closed`**。
样本 `openmir2-encoding-callsites`：10-20 approve review `wr=true`，10-21 直接以 `tier=L` 收卡并 closed ⇒ 现落 `closed`。
⇒ 「已完成 12」**不含「该卡窗口已验」的承诺**；若将来有卡在 `wr=true` 未验时被 closed，条件②会把欠账藏进 `closed`。
**建议**：`global_gate` 增加一条 `closed_without_window` 卫生清单（board closed ∧ 历史 `wr=true` ∧ 无免窗口裁定），或至少报告里把「closed」拆成「closed 且无 wr 历史」/「closed 但曾有 wr」。

### B. R 卡集合 = 「任一条目写过 `tier=R`」
`collect_r_cards` 的注释本就说明这是为防止「新 R 卡漏写 tier 造成假 PASS」；副作用是**后来降级为 L 的卡仍留在 213 里**
（`encoding-callsites`：issue/review 是 R，10-21 合并记录明写「L 卡可 closed」）。
⇒ 213 不是「严格 R 卡数」。**建议**：报告把「R 总体」与「当前仍判 R 的卡」分开计数（按最后一条带 tier 的条目不等于 R 过滤一次），或在 detail 里注明差量。

### C. `closed` 里含无代码改动的分诊卡
`openmir2-dualrun-triage-enterworld-frames` 关闭时明写 `code_changed=false`（时序差异、非缺陷）；它作为 R 计入 `closed 12`。
语义上没问题（分诊卡就应该 closed），但**不能用「closed 12」推断「12 个 R 实现已完成且可验收」**。**建议**同上（在 detail 里单列类型）。

### D. **主源级台账门红**：`script_host_noop_audit --strict-lines` / `selftest` = rc=1
- 实测：`check` rc=0；`check --strict-lines` **rc=1（229 处）**；`selftest` **rc=1**。
- 根因：`778f30a2c` 给 `mir2-rs/crates/script/src/engine/mod.rs` 插了 `take_say_abort`（+13 行）**未重钉** `script_host_noop_audit.json`
  （该 JSON 最后一次更新是 `4ed9fdf72`）；所有 `mod_line` 与 `fn` 声明行都差 **+13**，与插入行数吻合。
- 影响：goal_gate 的 7 项**不含**这个台账门，所以条件④的机判仍是「绿 5 / 需人工 2」；但多张卡（含 CLEARMAPMON、castle-official 自己）
  把 `check --strict-lines` + `selftest` 写进了 DoD ⇒ **口径不一致**：一项被判据依赖的门在 master 上是红的。
- **建议**：开一张 L 卡（台账重钉，改 `script_host_noop_audit.json` 的行号，重跑 `check --strict-lines` + `selftest` 至 rc=0）；
  并决定这个台账门是否要进 goal_gate 的 7 项（若进，当前 master 的条件④会变红）。

## 7. 复算命令（可直接执行）

```sh
cd <本检出>
git rev-parse HEAD                                   # 期望 2135f6154…

# ① 覆盖率（先按本 head 重建 release 工具）
(cd mir2-rs && cargo build --release -p mir2-script-tool)
mir2-rs/target/release/mir2-script-tool.exe parse-stats E:/MirServer/M2GameSvr/Envir \
    --json "$TEMP/parse.json" --require-implemented | grep -E '文件:|语料派发目标覆盖'

# ② 四条件（含 parse-stats）+ 分桶
python -I tools/goal/goal_gate.py --skip-tests \
    --script-bin "$(pwd)/mir2-rs/target/release/mir2-script-tool.exe" --json > "$TEMP/gate.json"
python -I -c "import json,os;d=json.load(open(os.path.join(os.environ['TEMP'],'gate.json')));print(d['overall']);[print(c['id'],c['state'],c['detail'][:160]) for c in d['conditions']]"

# ② 抽样：逐卡桶归属（直接调用 goal_gate 自己的分类函数）
python -I - <<'PY'
import importlib.util, pathlib
spec = importlib.util.spec_from_file_location("gg", "tools/goal/goal_gate.py")
gg = importlib.util.module_from_spec(spec); spec.loader.exec_module(gg)
repo = pathlib.Path(".").resolve()
board,_ = gg.load_board(repo); index,_ = gg.load_entry_index(repo)
wl = gg.build_whitelist(repo/"mir2-rs/tests/parity/whitelist.md", exclude_ids=gg.load_principals(repo))
for c in ["openmir2-encoding-callsites","openmir2-a09-upgrade-item","openmir2-active-seek-race-missing",
          "openmir2-m3-pet-slave-summon","openmir2-gm-batch-5b-cross-server"]:
    ev = gg.collect_r_evidence(c, index, board, wl)
    print(c, gg.r_category(ev))
PY

# ③ recon
python -I mir2-rs/tests/parity/coverage/recon_gates.py dispatch | tail -1

# ④ 门禁 / 台账门
cargo fmt --all -- --check ; cargo clippy --workspace --all-targets --locked -- -D warnings
cargo run --locked -p msgcodegen -- --check
python -I tools/whitelist/t_reservation_gate.py check
python -I tools/hygiene/conflict_marker_gate.py
python -I -B tools/hygiene/stale_pointer_gate.py --ref master
python -I mir2-rs/tests/parity/m3/whitelist_anchor_gate.py check
python -I mir2-rs/tests/parity/m3/m3_whitelist_registry_audit.py audit
python -I -B tools/hygiene/dynamic_count_stamp_gate.py --ref master
python -I -B tools/hygiene/board_coverage_gate.py --board .walgit/board.toml
python -I -B tools/hygiene/merge_tree_helper.py --selftest
python -I mir2-rs/tests/parity/coverage/script_host_noop_audit.py check              # rc=0
python -I mir2-rs/tests/parity/coverage/script_host_noop_audit.py check --strict-lines  # 期望 0，**实测 rc=1（§6-D）**
python -I mir2-rs/tests/parity/coverage/script_host_noop_audit.py selftest              # **实测 rc=1（§6-D）**
```

## 8. 本卡门禁 / 残留

- 本卡只新增本文档：`git diff --check <base> <head>` rc=0；7 项卫生门 + `merge_tree_helper --selftest` 见 patch body。
- 临时物：`E:/tmp/fourcond-parse.json` / `E:/tmp/fourcond-parse.log` / `E:/tmp/fourcond-gate.json`（复算产物，可删）。
- 全量 `cargo test --workspace` 未由我重跑（内存政策）；引用 tester `2eb147e6e` 的 3237 与增量分析（§4）。
