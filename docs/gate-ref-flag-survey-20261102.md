# 门禁 `--ref` 参数口径普查与修复（2026-11-02）

卡：`openmir2-gate-ref-flag-asymmetry`（tier=L）。
范围：`tools/hygiene/*.py` 与 `mir2-rs/tests/parity/**/*.py` 中**带 main/argparse 的门**。
判据：**实跑**同一脚本「不带 `--ref`」与「带 `--ref HEAD`」，比较退出码与输出
（输出哈希 = `sha256(stdout + stderr)` 前 16 位，仅作「两跑是否相同」的**单次样本**证据；
输出含实时状态或行序不稳的门**不复制具体值**，只记 rc 与「相同 / 不同」。见 §2 注记。）

## 1. 结论

| 类别 | 修复前（门禁 77 个） | 修复后（门禁 77 个） |
|---|---:|---:|
| 接受并生效 | 3 | 3 |
| 接受但未用 | 0 | 0 |
| 报错（未知参数 ⇒ 非零用法错，fail-closed） | 69 | 72 |
| 静默忽略（传了与不传输出相同） | 5 | 2 |
| 不认识（没有 flag 解析，按裸位置参数走） | 0 | 0 |

- **修了 3 处**，都在 `tools/hygiene/`：`conflict_marker_gate.py`、`docs_anchor_gate.py`、
  `scenario_manifest_gate.py`。三处统一改成「未知参数 ⇒ rc=2 + 打印用法」，**没有改动任何判定逻辑**。
- 剩余 2 处静默忽略都在 `mir2-rs/tests/parity/`（`check_anchors.py`、`script_host_noop_audit.py`），
  **不在本卡 write_set**，见 §7，建议另开卡。后续卡 `openmir2-gate-ref-residuals` 已把这两处
  也改成 fail-closed，收口后「静默忽略」= **0**；上表数字保留本卡（`openmir2-gate-ref-flag-asymmetry`）
  当时的实跑快照。
- 没有发现「接受但未用」：三个定义 `--ref` 的门禁都用**不同 ref**实跑证明会改变结论（见 §3、§4）。

### 类别口径（本卡采用的判定）

| 类别 | 判定 |
|---|---|
| 接受并生效 | 解析器认识 `--ref`，且换成不同 ref 后 rc 或输出改变 |
| 接受但未用 | 解析器认识 `--ref`，但换成不同 ref 后 rc 与输出逐字节相同 |
| 报错 | 不认识 `--ref`，退出码非零并打印用法（真拒绝） |
| 静默忽略 | 不认识 `--ref`，退出码与输出与不传时相同（含 0 与「碰巧同样非零」） |
| 不认识 | 没有 flag 解析，`--ref` 落在裸位置参数语义里（本卡把这些列为非门，见 §5） |

> 「静默忽略」最危险：它让「我按 ref 跑了门」变成假绿。**碰巧 rc=2 也不算 `报错`**——
> 判据是「是否被真正拒绝」，不是「退出码是否非零」（`script_host_noop_audit.py` 是反例）。

## 2. 实跑方法与证据形态

```sh
# 不带 --ref
python -I <script>
# 带 --ref
python -I <script> --ref HEAD
```

- 对 argparse 门，`--ref` 未登记时 argparse 在 `parse_args` 末尾以 rc=2 拒绝
  （输出含 `unrecognized arguments`）；这是**真拒绝**，不执行门体。
- 对自定义 `main(argv)` 门，才需要看输出哈希是否相同——相同即静默忽略。
- **哈希只作单次样本**：`dynamic_count_stamp_gate` 输出行序不稳、`stale_pointer_gate` 含实时
  ledger 与 ref sha、`scenario_manifest_gate` 读实时 `refs/collab`，其哈希**不可复现**，
  故 §3 不再登记这些门的具体值（结论以 rc 与「两跑相同 / 不同」为准）。
- 需要参数的观察员门（如 `review_principal_check.py --card ...`、`merge_tree_helper.py --base ...`）
  用其正常参数跑基线，再追加 `--ref HEAD`。
- 对会默认读取 `E:/MirServer` 的 parity 门，本机对照命令显式给 `--mongen` / `--mapinfo`
  指向不存在路径，避免触碰真机目录。

## 3. `tools/hygiene/` 全量（12 个）

| 脚本 | 无 `--ref` rc | 有 `--ref` rc | 类别 | 备注 |
|---|---:|---:|---|---|
| `board_coverage_gate.py` | 0 | 2 | 报错 | 自定义解析器，显式 `unknown argument '--ref'` |
| `conflict_marker_gate.py` | 0 | 2 | 报错（本次修复） | 修复前两跑均 rc=0 且输出相同（单次样本，未留存具体哈希）⇒ 静默忽略 |
| `docs_anchor_gate.py` | 0 | 2 | 报错（本次修复） | 修复前两跑均 rc=0 且输出相同（单次样本）；`--prose --base ...` 加 `--ref` 亦相同 |
| `docs_delivery_gate.py` | 0 | 2 | 报错 | argparse 拒绝 |
| `dynamic_count_stamp_gate.py` | 0 | 0 | 接受并生效 | 默认 ref = `refs/remotes/walgit/master`，`--ref HEAD` = HEAD；两者**不保证同一 commit**，输出相同只因 `master` 是 `HEAD` 的**祖先**（判定为祖先关系，链不变则结论不变）。`--ref <root>`（与 HEAD 无祖先关系）⇒ rc=1。输出行序不稳、哈希不可复现 ⇒ 只记 rc：0 / 0 / 1 |
| `merge_tree_helper.py` | 0 | 2 | 报错 | `--base HEAD --json` 基线；加 `--ref` ⇒ 用法 rc=2 |
| `review_principal_check.py` | 0 | 2 | 报错 | `--card <id>` 基线；加 `--ref` ⇒ 用法 rc=2 |
| `scenario_manifest_gate.py` | 0 | 2 | 报错（本次修复） | 修复前两跑均 rc=0 且输出相同（单次样本；该门读实时 `refs/collab`，哈希不可复现，故不复制）⇒ 静默忽略 |
| `scenario_undecidable_audit.py` | 0 | 2 | 报错 | argparse 拒绝 |
| `stale_pointer_gate.py` | 0 | 0 | 接受并生效 | 默认 ref = `refs/remotes/walgit/master`，`--ref HEAD` = HEAD；两者**不保证同一 commit**，输出相同只因 `master` 是 `HEAD` 的**祖先**。`--ref <root>` ⇒ rc=1。输出含实时 ledger 与 ref sha、哈希不可复现 ⇒ 只记 rc：0 / 0 / 1 |
| `tool_selftest_gate.py` | 0 | 2 | 报错 | 基线 `--inventory`；加 `--ref` ⇒ 用法 rc=2 |
| `wallclock_scan.py` | 0 | 2 | 报错 | 只读清点器，非判红门；argparse 拒绝 |

修复前小计：静默忽略 3 / 报错 7 / 接受并生效 2。修复后：静默忽略 0 / 报错 10 / 接受并生效 2。

## 4. `mir2-rs/tests/parity/` 门（65 个）

> 全部实跑。`2 / 2` 表示两跑都是用法错；`0 / 2` 表示不带 `--ref` 能跑、带 `--ref` 被拒绝。
> 10 个非门脚本（生成器/runner/抓包/变异器）见 §5，不计入本表。

| 脚本（相对 `mir2-rs/tests/parity/`） | 无 `--ref` rc | 有 `--ref` rc | 类别 | 备注 |
|---|---:|---:|---|---|
| `M4/evidence_hashes.py` | 0 | 2 | 报错 | argparse 不识别 `--ref`（rc=2，usage） |
| `M4/m4_parity.py` | 0 | 2 | 报错 | argparse 不识别 `--ref`（rc=2，usage） |
| `M4/npc_window_readiness.py` | 0 | 2 | 报错 | argparse 不识别 `--ref`（rc=2，usage） |
| `coverage/gm_registry.py` | 2 | 2 | 报错 | argparse 不识别 `--ref`（rc=2，usage） |
| `coverage/recon_gates.py` | 2 | 2 | 报错 | argparse 不识别 `--ref`（rc=2，usage） |
| `coverage/script_host_noop_audit.py` | 0 | 2 | 静默忽略（后续卡已修） | 修复前 `--ref=HEAD` / 裸 `--ref` 静默忽略且 rc=0，而 `--ref HEAD` 的 rc=2 只是值 `HEAD` 被当子命令（**不是**参数校验）。后续卡 `openmir2-gate-ref-residuals` 改为未知参数 ⇒ 三形态均 rc=2 + usage 并点名（见 §7） |
| `m3/attack_diff.py` | 2 | 2 | 报错 | argparse 不识别 `--ref`（rc=2，usage） |
| `m3/attack_rng_ledger.py` | 2 | 2 | 报错 | argparse 不识别 `--ref`（rc=2，usage） |
| `m3/attack_stream_gate.py` | 2 | 2 | 报错 | argparse 不识别 `--ref`（rc=2，usage） |
| `m3/castle_oracle_anchor_gate.py` | 2 | 2 | 报错 | argparse 不识别 `--ref`（rc=2，usage） |
| `m3/drop-exp/drop_exp_diff.py` | 2 | 2 | 报错 | argparse 不识别 `--ref`（rc=2，usage） |
| `m3/drop-exp/drop_exp_purity_gate.py` | 2 | 2 | 报错 | argparse 不识别 `--ref`（rc=2，usage） |
| `m3/fixture_chain_gate.py` | 2 | 2 | 报错 | argparse 不识别 `--ref`（rc=2，usage） |
| `m3/fixture_matrix.py` | 2 | 2 | 报错 | argparse 不识别 `--ref`（rc=2，usage） |
| `m3/guild_castle_anchor_gate.py` | 2 | 2 | 报错 | argparse 不识别 `--ref`（rc=2，usage） |
| `m3/guild_castle_diff.py` | 2 | 2 | 报错 | argparse 不识别 `--ref`（rc=2，usage） |
| `m3/hero_pet_anchor_gate.py` | 2 | 2 | 报错 | argparse 不识别 `--ref`（rc=2，usage） |
| `m3/hero_pet_diff.py` | 2 | 2 | 报错 | argparse 不识别 `--ref`（rc=2，usage） |
| `m3/m3_fixture_fmt_evidence_gate.py` | 2 | 2 | 报错 | argparse 不识别 `--ref`（rc=2，usage） |
| `m3/m3_guild_war_quirk_registry_gate.py` | 2 | 2 | 报错 | argparse 不识别 `--ref`（rc=2，usage） |
| `m3/m3_pet_slave_registry_gate.py` | 2 | 2 | 报错 | argparse 不识别 `--ref`（rc=2，usage） |
| `m3/m3_whitelist_registry_audit.py` | 2 | 2 | 报错 | argparse 不识别 `--ref`（rc=2，usage） |
| `m3/marker_prefix_gate.py` | 2 | 2 | 报错 | argparse 不识别 `--ref`（rc=2，usage） |
| `m3/npc_shop_diff.py` | 2 | 2 | 报错 | argparse 不识别 `--ref`（rc=2，usage） |
| `m3/party_trade_anchor_gate.py` | 2 | 2 | 报错 | argparse 不识别 `--ref`（rc=2，usage） |
| `m3/party_trade_diff.py` | 2 | 2 | 报错 | argparse 不识别 `--ref`（rc=2，usage） |
| `m3/quest_anchor_gate.py` | 2 | 2 | 报错 | argparse 不识别 `--ref`（rc=2，usage） |
| `m3/quest_diff.py` | 2 | 2 | 报错 | argparse 不识别 `--ref`（rc=2，usage） |
| `m3/rng-isolate-attack/check_predicates.py` | 2 | 2 | 报错 | argparse 不识别 `--ref`（rc=2，usage） |
| `m3/rng-isolate-attack/compare_dedicated_streams.py` | 2 | 2 | 报错 | argparse 不识别 `--ref`（rc=2，usage） |
| `m3/rng-scene/rng_trace_diff.py` | 2 | 2 | 报错 | argparse 不识别 `--ref`（rc=2，usage） |
| `m3/rng-trace-probe/analyze_rng_trace.py` | 2 | 2 | 报错 | argparse 不识别 `--ref`（rc=2，usage） |
| `m3/rng-trace-probe/rng_consumer_groups.py` | 0 | 2 | 报错 | 自定义 argv：首参当模式，`--ref` 不是 `selftest` ⇒ 用法 rc=2 |
| `m3/rng-trace-probe/rng_ledger_diff.py` | 2 | 2 | 报错 | argparse 不识别 `--ref`（rc=2，usage） |
| `m3/rng-trace-scene/check_anchors.py` | 1 | 1 | 静默忽略（后续卡已修） | 修复前裸跑与 `--ref HEAD` 都是 rc=1、输出相同（单次样本，不复制哈希）。后续卡 `openmir2-gate-ref-residuals` 改为未知参数 ⇒ rc=2 + usage；裸跑仍 rc=1，是**既有锚点漂移**（§7.1），不在该卡修 |
| `m3/rng-trace-scene/check_evidence_counts.py` | 2 | 2 | 报错 | argparse 不识别 `--ref`（rc=2，usage） |
| `m3/rng-trace-scene/check_evidence_head.py` | 2 | 2 | 报错 | argparse 不识别 `--ref`（rc=2，usage） |
| `m3/rng-trace-scene/check_window_batch.py` | 2 | 2 | 报错 | argparse 不识别 `--ref`（rc=2，usage） |
| `m3/rng-trace-scene/rng_scene_gate.py` | 2 | 2 | 报错 | argparse 不识别 `--ref`（rc=2，usage） |
| `m3/scoped_diff.py` | 2 | 2 | 报错 | argparse 不识别 `--ref`（rc=2，usage） |
| `m3/selloff_anchor_gate.py` | 2 | 2 | 报错 | argparse 不识别 `--ref`（rc=2，usage） |
| `m3/selloff_diff.py` | 2 | 2 | 报错 | argparse 不识别 `--ref`（rc=2，usage） |
| `m3/server_state_gate.py` | 2 | 2 | 报错 | argparse 不识别 `--ref`（rc=2，usage） |
| `m3/whitelist_anchor_gate.py` | 2 | 2 | 报错 | argparse 不识别 `--ref`（rc=2，usage） |
| `m3/window-20261023/judge_login.py` | 2 | 2 | 报错 | argparse 不识别 `--ref`（rc=2，usage） |
| `m3/window-20261023/preflight.py` | 1 | 2 | 报错 | argparse 不识别 `--ref`（rc=2，usage） |
| `world/client-e2e-enter-gaps-20261013/aoi_visibility_pair.py` | 2 | 2 | 报错 | argparse 不识别 `--ref`（rc=2，usage） |
| `world/client-e2e-enter-gaps-20261013/check_aoi_visibility_diag.py` | 2 | 2 | 报错 | argparse 不识别 `--ref`（rc=2，usage） |
| `world/client-e2e-enter-gaps-20261013/check_appearance_pairs.py` | 2 | 2 | 报错 | argparse 不识别 `--ref`（rc=2，usage） |
| `world/client-e2e-enter-gaps-20261013/check_attack_observability.py` | 2 | 2 | 报错 | argparse 不识别 `--ref`（rc=2，usage） |
| `world/client-e2e-enter-gaps-20261013/check_client_run_frames.py` | 2 | 2 | 报错 | argparse 不识别 `--ref`（rc=2，usage） |
| `world/client-e2e-enter-gaps-20261013/check_client_s2c_frames.py` | 2 | 2 | 报错 | argparse 不识别 `--ref`（rc=2，usage） |
| `world/client-e2e-enter-gaps-20261013/check_csharp_capture_preflight.py` | 2 | 2 | 报错 | argparse 不识别 `--ref`（rc=2，usage） |
| `world/client-e2e-enter-gaps-20261013/check_healthspellchanged_trigger.py` | 2 | 2 | 报错 | argparse 不识别 `--ref`（rc=2，usage） |
| `world/client-e2e-enter-gaps-20261013/check_mongen_dense_points.py` | 2 | 2 | 报错 | argparse 不识别 `--ref`（rc=2，usage） |
| `world/client-e2e-enter-gaps-20261013/check_reverse_point_clean.py` | 2 | 2 | 报错 | argparse 不识别 `--ref`（rc=2，usage） |
| `world/client-e2e-enter-gaps-20261013/check_sm_hit.py` | 2 | 2 | 报错 | argparse 不识别 `--ref`（rc=2，usage） |
| `world/client-e2e-enter-gaps-20261013/check_timing.py` | 2 | 2 | 报错 | argparse 不识别 `--ref`（rc=2，usage） |
| `world/client-e2e-enter-gaps-20261013/verify_client_ability_layout.py` | 0 | 2 | 报错 | argparse 不识别 `--ref`（rc=2，usage） |
| `world/client-e2e-enter-gaps-20261013/verify_client_highhp_family.py` | 0 | 2 | 报错 | argparse 不识别 `--ref`（rc=2，usage） |
| `m3/attack-request-sequence/per_strike_requests.py` | 2 | 2 | 报错 | argparse 不识别 `--ref`，两跑均用法 rc=2 |
| `m3/attack-request-sequence/session_target_availability.py` | 2 | 2 | 报错 | argparse 不识别 `--ref`，两跑均用法 rc=2 |
| `m3/attack-request-sequence/stream_equiv.py` | 2 | 2 | 报错 | argparse 不识别 `--ref`，两跑均用法 rc=2 |
| `m3/evidence-m3-isolate-twophase-fn1-20261013/stream_equiv.py` | 2 | 2 | 报错 | argparse 不识别 `--ref`，两跑均用法 rc=2 |
| `world/client-e2e-enter-gaps-20261013/check_rust_window_preflight.py` | 1 | 1 | 接受并生效 | 明确 args.ref 参与祖先判定：`--ref HEAD` 全 OK；`--ref <root>` ⇒ `RED R1 … 不含 d16b5199d9` |

小计：报错 62 / 静默忽略 2 / 接受并生效 1。

- **接受并生效 1**：`world/client-e2e-enter-gaps-20261013/check_rust_window_preflight.py`。
  `--ref HEAD` 五项 R1 全 OK；`--ref <root>` 同一命令输出 `RED R1 … 不含 d16b5199d9`，
  证明 ref 真的进了 `git merge-base --is-ancestor` 判定。
- **静默忽略 2（后续卡 `openmir2-gate-ref-residuals` 已收口，见 §7）**：
  - `m3/rng-trace-scene/check_anchors.py`：修复前裸跑与 `--ref HEAD` 都是 rc=1、输出相同
    （单次样本，不复制具体哈希；哈希相同是关键字面证据）；未知 flag 被 `argv` 手工解析逻辑忽略。
    该门裸跑现仍 rc=1，属**既有锚点漂移**，与参数处理无关（§7.1）。
  - `coverage/script_host_noop_audit.py`：修复前 `--ref=HEAD` 与裸 `--ref` 都是 rc=0 且输出与裸跑相同；
    只有两 token 形态 `--ref HEAD` 会 rc=2，但原因是值 `HEAD` 被当成子命令，**不是参数校验**。

## 5. 非门脚本（不计入门禁计数）

这些脚本带 main/argparse 或裸 `sys.argv`，但语义是生成器 / runner / 抓包 / 变异器，
不是「判红」的门；本卡只记录、不修。

| 脚本（相对 `mir2-rs/tests/parity/`） | 角色 | `--ref` 形态 |
|---|---|---|
| `M4/run_npc_cases.py` | 用例 runner | 非门；不适用 |
| `loginsrv/oracle_capture.py` | 抓包器 | 非门；不适用 |
| `m3/evidence-m3-pet-slave-amulet-delitem-20261014/redchecks/mutate.py` | 变异器 | 裸 `argv[1]` 模式 |
| `m3/evidence-m3-pet-slave-mutiny-frames-20261014/redchecks/mutate.py` | 变异器 | 裸 `argv[1]` 模式 |
| `m3/evidence-m3-tiered-acceptance-20261013/collect.py` | 证据收集器 | 无 argv 处理 |
| `m3/fixtures/extract_map_ai_consumers.py` | 夹具提取器 | 两个位置参数 |
| `m3/fixtures/extract_mongen.py` | 夹具提取器 | 两个位置参数 |
| `m3/quest_state_producer.py` | 证据生产者 | 非门；不适用 |
| `m3/rebuild_attack_my.py` | 重建器 | 裸 `sys.argv[1:2]` |
| `world/client-e2e-enter-gaps-20261013/window_handoff.py` | 交接/计划生成器 | 明确解析 `--ref` 并用于计划文本（非判据门） |

## 6. 修复详情

### 6.1 `tools/hygiene/conflict_marker_gate.py`（选 A2 方案②：显式拒绝）

- 新增 `unknown_argument(argv)`：只放行 `-h` / `--help` / `--selftest`；其余任意 token
  （含 `--ref`、`--ref=HEAD`）⇒ stderr 打印 `unknown argument '<token>'` + 一行 usage，rc=2。
- 为什么不实现 `--ref`：本门判据是「跟踪文件里有没有冲突标记」，读的是
  `git ls-files -z` + **工作树文件**。改成按 ref blob 读会改变判定对象（且 `--intent-to-add`
  的覆盖语义需要重新定义），超出「只改参数处理」的边界；fail-closed 改动最小、语义不变。
- 牙（selftest 从 6 条增至 9 条）：
  - `unknown --ref is rejected: rc=2 + usage on stderr`
  - `any unknown flag is rejected (fail-closed)`
  - `--help still works (known flag, rc=0)`
- 变异自证：删掉 `if bad is not None:` 拒绝分支后 selftest rc=1，两条「rejected」用例变红；
  恢复后文件 sha256 与变异前相等。

### 6.2 `tools/hygiene/docs_anchor_gate.py`

- 新增 `unknown_argument(argv)`：布尔 flag 集合 `-h/--help/--selftest/--prose/--list`；
  带值 flag 集合 `--base/--repo/--registry/--paths`（**非 flag 值 token 被跳过**；
  `--repo --ref` 这种「值位上是 flag」形态仍判 `--ref` 为未知，避免把未知 flag 当成值吞掉）。
  未知 token ⇒ rc=2 + 两行 usage。
- 顺带补 `-h/--help` 打印文档并 rc=0（旧版会静默落回判定路径）。
- 牙（selftest 从 31 条增至 36 条）：
  - 未知 `--ref` ⇒ rc=2 且 stderr 点名；
  - `--prose --base HEAD --ref HEAD` ⇒ 仍拒（证明 `--base` 的值没有被当成 flag）；
  - 任意未知 flag ⇒ rc=2；
  - `--repo --ref` ⇒ `--ref` 不被当成值吞掉，仍 rc=2；
  - `--help` 仍可用。
- 变异自证：删拒绝分支后 selftest rc=1，四条未知参数用例变红；恢复后 sha256 相等。

### 6.3 `tools/hygiene/scenario_manifest_gate.py`

- 新增 `unknown_argument(argv)`：布尔 `-h/--help/--selftest/--audit-exempt/--json`，
  带值 `--repo`（值位是 flag 时不吞）；未知 token ⇒ rc=2 + usage。同样补 `-h/--help`。
- 牙（selftest 从 34 条增至 39 条）：
  - 未知 `--ref` ⇒ rc=2 且点名；
  - `--repo . --ref HEAD` ⇒ 值不被误判，`--ref` 仍被拒；
  - 任意未知 flag ⇒ rc=2；
  - `--repo --ref` ⇒ `--ref` 不被当成值吞掉，仍 rc=2；
  - `--help` 仍可用。
- 变异自证：删拒绝分支后 selftest rc=1，四条未知参数用例变红；恢复后 sha256 相等。

## 7. 后续收口（卡 `openmir2-gate-ref-residuals`）

原「未修 / 后续」的两处已由后续卡收口。收口口径与本卡一致：**只改参数处理，判定逻辑零改动**；
未知参数 ⇒ rc=2 + stderr `unknown argument '<token>'` + 一行 usage。

| 脚本 | 收口前 | 收口后 |
|---|---|---|
| `mir2-rs/tests/parity/m3/rng-trace-scene/check_anchors.py` | 未知 flag 被手工 argv 解析静默忽略（裸跑与 `--ref HEAD` 输出相同） | `--ref` / `--ref HEAD` / `--ref=HEAD` 均 rc=2 + usage；`check` / `selftest` / `--doc <path>` / `-h/--help` 照旧 |
| `mir2-rs/tests/parity/coverage/script_host_noop_audit.py` | `--ref=HEAD` / 裸 `--ref` 静默忽略（rc=0）；`--ref HEAD` 的 rc=2 来自值 `HEAD` 被当子命令，**不是参数校验** | `--ref` / `--ref HEAD` / `--ref=HEAD` 均 rc=2 + usage 且**点名 `--ref`**；`check [--strict-lines]` / `selftest` / `-h/--help` 照旧 |

两门的 selftest 各新增 4 条 CLI 用例（同形：`--ref HEAD`、`--ref=HEAD`、裸 `--ref`、`--help`）。
断言**不是**「rc 非零」，而是「stderr 点名 `unknown argument '<token>'` 且含 `usage:`」——
否则 `script_host_noop_audit.py` 旧行为（值被当子命令）的 rc=2 会冒充真拒绝。
变异自证：删掉拒绝分支后两门 selftest 均变红（`check_anchors` 3 条 CLI 用例、
`script_host_noop_audit` 3 条 CLI 用例）；恢复后文件 sha256 与变异前相等。

### 7.1 既有问题（登记，不在本卡修）

`check_anchors.py check` 在本 tip 仍 **rc=1**，原因是 `DRIVER-CONTRACT.md` 的两处源码锚点漂移：

- `mir2-rs/crates/world/src/monai.rs`：文档写 `:5227`，`pub fn monster_ai_tick(` 的首个匹配在 `mir2-rs/crates/world/src/monai.rs:5259`；
- 同一文件：文档写 `:234`，`pub fn wondering(` 的首个匹配在 `mir2-rs/crates/world/src/monai.rs:236`。

这是**既有锚点漂移**，与本卡（参数处理）无关；按卡面要求只登记不修（另案重钉）。

`window_handoff.py` 虽然是生成器而非门，但其 `--ref` 是**接受并生效**的（参与计划文本与检查），
无需处理。

## 8. 收口命令（合并前必跑）

```sh
python -I tools/hygiene/tool_selftest_gate.py --inventory
python -I tools/hygiene/tool_selftest_gate.py
python -I tools/hygiene/docs_delivery_gate.py
python -I tools/hygiene/docs_anchor_gate.py
python -I tools/hygiene/docs_anchor_gate.py --prose --base walgit/master
python -I tools/hygiene/dynamic_count_stamp_gate.py --ref HEAD
python -I tools/hygiene/stale_pointer_gate.py --ref HEAD
python -I tools/whitelist/t_reservation_gate.py check
python -I mir2-rs/tests/parity/m3/rng-trace-scene/check_anchors.py --ref HEAD      # 期望 rc=2（fail-closed）
python -I mir2-rs/tests/parity/coverage/script_host_noop_audit.py selftest        # 期望 rc=0
git diff --check <merge-base> <head>
```

> `check_anchors.py selftest` 现为 20/21：唯一红是 §7.1 的既有锚点漂移（`real-doc-fresh`），
> 属另案；新增的 4 条 CLI fail-closed 用例全绿。`script_host_noop_audit.py selftest` 全绿（含 CLI 牙）。

## 9. 残留物

- 无临时目录 / 无新进程 / 未启动任何栈。普查实跑全部在工作树内只读完成。
- 一次副作用：裸跑 `M4/m4_parity.py` 会重写
  `mir2-rs/tests/parity/evidence/M4/parse-stats-cs.json` 与
  `mir2-rs/tests/parity/evidence/M4/macro/parse-stats-cs.json`（内容相同、仅行尾差异）。
  已用 `git restore` 还原，**两个文件不在本卡补丁内**；后续普查未再裸跑该脚本。
- 后续卡 `openmir2-gate-ref-residuals`：无新增临时物 / 进程 / 栈。两门变异自证只在自身
  worktree 内改-跑-还原，恢复后 sha256 与变异前相等、无 `MUTATION` 残留。
