# carried 积压分诊（2026-10-24）

- 卡：`openmir2-carried-backlog-triage`（tier=L，只产报告）。
- 数据时点：`@ 967ce395e @ 2026-10-24 11:57 +08`；账本读取：一次 `git cat-file --batch` 读
  `refs/collab/inbox/*`（8639 条）+ `refs/collab/meta/snapshot`，未按卡起子进程。
- 返工 1（openmir2-worker-58）：@ fd2db51e9 @ 2026-10-24；按 reviewer-3 四条 Important + 两条 Minor 订正——#58/#59/#60/#61/#69/#70 改判仍存在（目标文档 docs/rewrite-remaining-inventory-20261022.md 在 master，缺陷逐条复核仍在）、#89 记部分消解、#57/#106/#107 证据 sha 订正、#34/#35/#77/#131 补修复 sha、§1 补就地带时点。
- 判定口径：**已消解**必须能指认后续卡/提交；**仍存在** = 回仓核对仍能命中，或原卡收口后
  无后续认领（批量口径）；**判不准** = 目标文件/行在当前 master 上定位不到。

## 1. 总量（口径 as of `@ 967ce395e @ 2026-10-24 11:57 +08`）

| 口径 | 数量 |
|---|---:|
| 带非空 `carried` 的 status 条目 | 46 |
| carried 原始条数 | 131 |
| 精确字符串去重后 | 131（无完全相同的两条） |
| 概念去重后（82–84 被同卡后续 85–87 取代） | 128 |

> 另有**跨卡近重复**：#87 / #92 都指 `loader.load_map_quest` 两次调用（drain-order-followup 与 qfunction），未计入上面的去重数；修一处即可覆盖两条。

判定分布：**已消解 17（其中部分消解 1：#89） / 仍存在 110 / 重复 3 / 判不准 1**。
仍存在里：文档措辞·数字 **49** / 判据缺口 **18** / 已知限制·过程记录 **42** / 需 owner **1**。

## 2. 总数表（131 条）

| # | 原卡 | status oid | carried（截断） | 判定 | 依据 |
|---:|---|---|---|---|---|
| 1 | `openmir2-script-user-account` | d188a10e8 | M-1 p31 锚点未钉 sim_loop 实参 login_account: &account, | 仍存在（b） | 无后续卡/提交认领（批量口径） |
| 2 | `openmir2-script-user-account` | d188a10e8 | M-2 docs §5 牙 C 还原 sha 措辞易误读 | 仍存在（a） | 无后续卡/提交认领（批量口径） |
| 3 | `openmir2-script-user-account` | d188a10e8 | I-1 测试 helper 仍有 account: String::new() 与一处有意手工种值（不在生产缝） | 仍存在（b） | 无后续卡/提交认领（批量口径） |
| 4 | `openmir2-sendmsg-login-qmanage-cases` | 4c7ed19b0 | docs §2.2 三条 SENDMSG（月卡:8、月卡:23、登陆脚本:17）实际不执行（首个已注册动作后 return）⇒ 标「否/不执行」并把「登陆脚… | 仍存在（a） | 无后续卡/提交认领（批量口径） |
| 5 | `openmir2-sendmsg-login-qmanage-cases` | 4c7ed19b0 | 观察：整段在 if let Some(tx) 内，触发者网关不可达时他人也收不到（与既有 @Login 脚本执行同源，非本卡回归） | 仍存在（c） | 无后续卡/提交认领（批量口径） |
| 6 | `openmir2-642-stale-text-sweep` | 2f03b614c | §4.1/§5.4 用「通常/常见」，代码上破零支必发第二条（:1705 尾桶门）⇒ 宜改确定性表述 | 仍存在（a） | 无后续卡/提交认领（批量口径） |
| 7 | `openmir2-642-stale-text-sweep` | 2f03b614c | patch 自称 15 行实为 14 条；table[0] 引 :1716 实为 :1705 | 仍存在（a） | 无后续卡/提交认领（批量口径） |
| 8 | `openmir2-642-stale-text-sweep` | 2f03b614c | 两提交 git author 是 worker-69（签名 actor worker-73）⇒ 后续车道设 local user.name | 仍存在（c） | 无后续卡/提交认领（批量口径） |
| 9 | `openmir2-642-stale-text-sweep` | 2f03b614c | docs/window-batch-rehearsal-20261024.md:79/:198 将变假 ⇒ 另开 docs 小卡 | 仍存在（a） | 无后续卡/提交认领（批量口径） |
| 10 | `openmir2-scenario-fixture-precondition` | 632df4159 | patch 的 T 预约证据取自不同树（live/landed/max 与钉住 head 不一致，绿结论不变） | 仍存在（c） | 无后续卡/提交认领（批量口径） |
| 11 | `openmir2-scenario-fixture-precondition` | 632df4159 | 报错括注「（或某侧被覆盖成空）」实际不可达（空数组先抛形状错误） | 仍存在（a） | 无后续卡/提交认领（批量口径） |
| 12 | `openmir2-scenario-fixture-precondition` | 632df4159 | REQUIRES_KNOWN 在 card_window.py 与 mir2_probe.py 各一份，单侧漂移仍 fail-closed，建议注释互指 | 仍存在（c） | 无后续卡/提交认领（批量口径） |
| 13 | `openmir2-rehearsal-stale-lines` | bf50b4455 | M-1：彩排文档 :67 的「20/20」应改 18/20（需重跑 20×2 全矩阵）⇒ 建议单开 L 卡 | 仍存在（a） | 无后续卡/提交认领（批量口径） |
| 14 | `openmir2-rehearsal-stale-lines` | bf50b4455 | M-2：openmir2-probe-scenario-inventory-key 与 openmir2-inventory-fixture-reachab… | 仍存在（d） | 无后续卡/提交认领（批量口径） |
| 15 | `openmir2-script-broadcast-tx-gating` | a5a66116e | minor1：未断言 n == 0 | 仍存在（b） | 无后续卡/提交认领（批量口径） |
| 16 | `openmir2-script-broadcast-tx-gating` | a5a66116e | minor2：调用点结构调整没被钉住（建议加源码序门） | 仍存在（b） | 无后续卡/提交认领（批量口径） |
| 17 | `openmir2-script-broadcast-tx-gating` | a5a66116e | minor3：§6 窗口项缺可复跑命令 | 仍存在（a） | 无后续卡/提交认领（批量口径） |
| 18 | `openmir2-window-criteria-backfill-2` | 68d07a5a6 | 文档头 :3 仍写旧分支名 | 仍存在（a） | 无后续卡/提交认领（批量口径） |
| 19 | `openmir2-window-criteria-backfill-2` | 68d07a5a6 | §8 引用中间版快照（建议改 final） | 仍存在（a） | 无后续卡/提交认领（批量口径） |
| 20 | `openmir2-window-criteria-backfill-2` | 68d07a5a6 | §6 merge-tree tree 是中间态哈希 | 仍存在（a） | 无后续卡/提交认领（批量口径） |
| 21 | `openmir2-ability-weaponstrong` | da89cc3d1 | minor：weapon_strong_of 未沿用 oracle RecalcAbilitys 的 Dura==0 跳过（当前部署不可达：装备槽 Dura… | 已消解 | e2d7ec568（weapon_strong_of 补 Dura==0 整槽跳过） |
| 22 | `openmir2-broadcast-tx-source-order-gate` | 21434f2e2 | minor1：fan_positions.len()==1 结构性钉死，抽 helper 会误红（建议≥1 且全在区间外） | 仍存在（b） | 无后续卡/提交认领（批量口径） |
| 23 | `openmir2-broadcast-tx-source-order-gate` | 21434f2e2 | minor2：calls>=1 含 3 个测试内调用，可选加「至少一处在 sim_loop 内」 | 仍存在（b） | 无后续卡/提交认领（批量口径） |
| 24 | `openmir2-card-window-var-passthrough` | 1e4be14da | card_window.py:1029 注释仍称 --var 只来自 discovery | 仍存在（a） | 无后续卡/提交认领（批量口径） |
| 25 | `openmir2-card-window-var-passthrough` | 1e4be14da | V1 牙只锁 dry-run 调用点（真跑 run_side 接线无覆盖）⇒ 建议补 V4 牙 | 仍存在（b） | 无后续卡/提交认领（批量口径） |
| 26 | `openmir2-card-window-var-passthrough` | 1e4be14da | 自测段编号 18n 排在 18m 之前 | 仍存在（a） | 无后续卡/提交认领（批量口径） |
| 27 | `openmir2-weaponstrong-dura-zero-skip` | ba6b54f48 | T-266 新增说明落在「谁审的」列内，排版小瑕 | 仍存在（a） | 无后续卡/提交认领（批量口径） |
| 28 | `openmir2-weaponstrong-dura-zero-skip` | ba6b54f48 | 建议把「窗口如需直接验该分支要构造 Dura=0 存档」写进窗口备注 | 仍存在（a） | 无后续卡/提交认领（批量口径） |
| 29 | `openmir2-needs-human-count-sync` | 8f719461e | v2 merge_tree 正确值 540310bd3（审计链请以此为准） | 判不准 | v3 已重写该段；`540310bd3` 在当前文档中不可定位 |
| 30 | `openmir2-inventory-fixture-reachability` | 7344b2ee9 | docs/inventory-fixture-reachability-20261024.md:86 的 grep 措辞需收窄（mir2frame.py:5… | 仍存在（a） | 无后续卡/提交认领（批量口径） |
| 31 | `openmir2-gate-baseline-snapshot` | c7c5263e6 | 第 9/11 行用 …/ 省略号，不可直接复制执行 | 仍存在（a） | 无后续卡/提交认领（批量口径） |
| 32 | `openmir2-gate-baseline-snapshot` | c7c5263e6 | 第 14 行（全量变异未跑）与第 17 行（快照时 HEAD==master，merge-tree 为自比平凡结果）建议补边界说明 | 仍存在（c） | 无后续卡/提交认领（批量口径） |
| 33 | `openmir2-goal-gate-full-20261024` | d6daedf6e | :37 的 board_cards/index_threads/index_entries 随账本漂移，引用以当次 --json 现取为准 | 仍存在（c） | 无后续卡/提交认领（批量口径） |
| 34 | `openmir2-docs-stale-pointer-gate` | af605e8d1 | Minor（reviewer-3，非阻断）：`stale_pointer_gate.py` 的 allow-list rot 检测在 `--path .`（… | 已消解 | allowlist_in_scope 的 `prefix in ("", ".")` 已修（提交 715116864；stale_pointer_gate.py:1160） |
| 35 | `openmir2-docs-stale-pointer-gate` | af605e8d1 | Suggestion：文档 §5 盲区清单未列「--ref 过期（未 fetch）⇒ 刚合入提交判非祖先 ⇒ 假绿」 | 已消解 | 文档 §5-11 已登记 --ref 过期假绿（stale-pointer-gate-20261024.md:366；提交 d1d3be761） |
| 36 | `openmir2-docs-stale-pointer-gate` | af605e8d1 | Suggestion：`未接` 子串匹配会命中「未接受/未接通」，将来可能假红 | 仍存在（c） | 无后续卡/提交认领（批量口径） |
| 37 | `openmir2-docs-post-18-sweep` | 42e26d9d6 | 非阻断 nit（reviewer-10）：§「并集三个数（别混口径）」标题仍把 30 与 26/31 并列；建议改为「可用口径 26/31；30 为旧口径（… | 仍存在（a） | 无后续卡/提交认领（批量口径） |
| 38 | `openmir2-stale-pointer-card-status-check` | 1cbe8890c | M-1 测试缺口（reviewer-3）：⑰ 未钉 span 级语义 —— 把排除改成「整行否决」后 selftest 仍 20/20（真实语料结果相同）。… | 仍存在（b） | 无 fixture 钉 `未接线，且未合并`（整行否决仍会绿） |
| 39 | `openmir2-stale-pointer-card-status-check` | 1cbe8890c | M-2 报告 blame 列两行取的是 committer 时间（drift-sweep:48 应为 18:28Z 非 21:22Z；m3:46 应为 13… | 已消解 | errata 卡合并 917376a45：报告已改 18:28Z / 13:23Z |
| 40 | `openmir2-stale-pointer-card-status-check` | 1cbe8890c | M-3「当日 4 次 merge」只在 +08 日历日成立（UTC 日 5 条） | 仍存在（a） | 报告仍写「当日 4 次 merge」（stale-pointer-gate:239/267） |
| 41 | `openmir2-stale-pointer-card-status-check` | 1cbe8890c | M-4 leader-quota:22 称由 ca7cb5acc 合入，实为合并 commit 65a8af8d6（12:02+08）；12:05 是 me… | 已消解 | errata 卡合并 917376a45：已改 65a8af8d6（12:02+08） |
| 42 | `openmir2-stale-pointer-card-status-check` | 1cbe8890c | M-5 docstring「4 false hits」措辞把 2 条真命中算成假红 | 已消解 | errata 卡合并 917376a45：docstring 已改 2 真 + 2 假 |
| 43 | `openmir2-stale-pointer-card-status-check` | 1cbe8890c | M-6 `未接入` 是未使用的死字面，且 §5 未列「字面串内且无其它词 ⇒ 绿」的假绿面 | 仍存在（b） | 无后续卡/提交认领（批量口径） |
| 44 | `openmir2-stale-pointer-card-status-check` | 1cbe8890c | ★ 其中 M-2 / M-4 / M-5 是**事实性错误**（时间戳、合并 commit、真假命中计数），本卡以 Minor 放行但它们会污染这份报告的引… | 已消解 | errata 卡合并 917376a45：三处事实错误已订正 |
| 45 | `openmir2-merchant-script-load` | 042266637 | O-1 (minor)：缺失清单取自 seeds（挂图前）；挂图失败被 RemoveAt 的商人 oracle 不记 ERR ⇒ 该边界多记 1 条（仅日志… | 仍存在（c） | 无后续卡/提交认领（批量口径） |
| 46 | `openmir2-merchant-script-load` | 042266637 | O-2 (minor)：QFunction 残余写「另开卡」但无卡号；建议挂到既有 openmir2-npc-ctor-draws（其窗口项②已含 Func… | 仍存在（a） | 无后续卡/提交认领（批量口径） |
| 47 | `openmir2-merchant-script-load` | 042266637 | O-3 (minor)：两处测试注释/断言串仍称「本仓 Market_Def 未装载」（行为不变，措辞过期） | 仍存在（a） | 无后续卡/提交认领（批量口径） |
| 48 | `openmir2-merchant-script-load` | 042266637 | O-4 (minor，覆盖)：4 条新用例只覆盖 fail/缺失分支；ok 支的脚本副作用、use_slots 回写、pending GM 无构造级断言 ⇒… | 仍存在（b） | 无后续卡/提交认领（批量口径） |
| 49 | `openmir2-merchant-script-load` | 042266637 | O-5 (observation)：两臂接线块与 label 臂逐字重复，可抽闭包（清洁度） | 仍存在（c） | 无后续卡/提交认领（批量口径） |
| 50 | `openmir2-merchant-script-load` | 042266637 | ★ 分支保留：card/merchant-script-load 仍被 worktree E:/tmp/wt-worker-73 检出，按纪律不删 | 仍存在（c） | 无后续卡/提交认领（批量口径） |
| 51 | `openmir2-review-principal-check` | eac14d079 | F2（reviewer-6，minor 但**性质是 fail-open**）：`--reviewer "openmir2-worker-a（作者）"` /… | 仍存在（b） | 无后续卡/提交认领（批量口径） |
| 52 | `openmir2-review-principal-check` | eac14d079 | F2 另一面：损坏条目「跳过 + 计数」的语义允许「reviewer 自己的 claim 条目损坏 ⇒ 被丢弃 ⇒ 仍返回 OK」。审查者建议采纳另一实现的… | 仍存在（b） | 无后续卡/提交认领（批量口径） |
| 53 | `openmir2-review-principal-check` | eac14d079 | F4（nit）：作者报的 merge-tree tree `4712941d8` 复现不出；以 `walgit/master=460691b24` 实得 `… | 仍存在（c） | 无后续卡/提交认领（批量口径） |
| 54 | `openmir2-review-principal-check` | eac14d079 | ★ 过程问题：作者用**另一张卡未合入的 head** 作 base（`1679b4fa9`），导致本卡分支包含那张卡的 3 个文件。已在合并时用 cher… | 仍存在（c） | 无后续卡/提交认领（批量口径） |
| 55 | `openmir2-session-digest-20261024` | 5bcce864d | Minor（reviewer-5）：§6:177 gate-baseline 条目用现在时写「在办/待审/未合入 master」—— 在声明快照 c4b66… | 仍存在（a） | session-digest 未加「以下为快照时点状态」 |
| 56 | `openmir2-session-digest-20261024` | 5bcce864d | §4:161 本卡自述行仍引用 07:11 status 的「35/37」，与头部 36/38 不一致 —— 按 CUTOFF 语义正确（刷新在 07:21… | 仍存在（c） | 无后续卡/提交认领（批量口径） |
| 57 | `openmir2-session-digest-20261024` | 5bcce864d | ★ 门侧问题（转 openmir2-stale-pointer-snapshot-exemption）：快照文档里的指针/卡状态陈述会被 stale_poi… | 已消解 | f14cc2910（10:53:43）给 :57 加订正标记 ⇒ 被 CORRECTION_RE（stale_pointer_gate.py:225 与 :1244）跳过，全量门 rc=0；与 cd045154c/456c4a291 无关（两提交只动 dynamic-count 文档与门） |
| 58 | `openmir2-inventory-top5-audit` | 93202693a | M-1：§12.3 换命令的**理由错误** —— `tail -1` 与 `--reverse / head -1` 对同一 log 列表恒等（该 ran… | 仍存在（a） | docs/rewrite-remaining-inventory-20261022.md:1336-1337 仍在：第 5 轮 tail -1 的理由错误段原样保留（top5 合并 3fa6899a9 的交付写集就是该文件，在 master） |
| 59 | `openmir2-inventory-top5-audit` | 93202693a | M-2：§12.4「实跑输出」是节选（verbatim grep 还会多出 2908/47360/inbound.rs:21/35 共 8 行，只贴了 4 … | 仍存在（a） | 同文档 :1364-1368「实跑输出」只贴 4 行 grep；verbatim 实测 8 行（lib.rs 2887/2908/47686 + inbound.rs 21/35/263/933/938） |
| 60 | `openmir2-inventory-top5-audit` | 93202693a | M-3：`shop.rs:35-39` 应为 35-37（39 已落到下一节标题「# 本片未做」）。 | 仍存在（a） | 同文档 :448 与 :1029 仍写 shop.rs:35-39；shop.rs:38 已是「# 本片未做（其余，登记）」标题，应为 35-37 |
| 61 | `openmir2-inventory-top5-audit` | 93202693a | ★ 本卡结论：§6 top5 里 「3 条早已完成、1 条只是描述过期」，真正没做的只有 1 条 ⇒ 直接支撑 openmir2-inventory-b-t… | 仍存在（c） | 文档在 master（top5 合并 3fa6899a9）；§6/§3.2.1 结论段仍是可引用的过程结论 |
| 62 | `openmir2-drain-order-gate` | 274567d40 | ★ 过程：本卡曾同秒出现两条 in-progress（worker-24 / worker-70），各自推了一个 head；已裁定以 37d53616f 为… | 仍存在（c） | 无后续卡/提交认领（批量口径） |
| 63 | `openmir2-drain-order-gate` | 274567d40 | ★ 口径修正（作者发现）：卡面写的 `drain_run_downlinks` 其实是只转发参数的测试入口，**真实生产调用在 `drain_run_dow… | 仍存在（c） | 无后续卡/提交认领（批量口径） |
| 64 | `openmir2-stale-pointer-snapshot-exemption` | 19de945ed | **N-1（应当修，doc 与实测不符）**：文档称「未提交的声明行 blame 取不到 ⇒ 不豁免」，实测**已跟踪/已暂存但未提交**的行 blame … | 已消解 | blame fail-closed 卡 5848a872b：全 0 / Not Committed Yet ⇒ 不豁免 |
| 65 | `openmir2-stale-pointer-snapshot-exemption` | 19de945ed | **N-2**：fail-closed 分支无夹具 —— 复核自造变异「blame 取不到就放行」⇒ 30/30 存活。与 N-1 是同一处，建议一起补。 | 已消解 | blame fail-closed 卡 5848a872b：selftest ㉛ 钉住该分支 |
| 66 | `openmir2-stale-pointer-snapshot-exemption` | 19de945ed | N-3：`snapshot_exempt` 按行计数但汇总写 `hit(s)`（session-digest 2 行 4 条 INFO，汇总说 2）。 | 已消解 | blame fail-closed 卡 5848a872b：汇总改为 snapshot-exempt line(s) |
| 67 | `openmir2-stale-pointer-snapshot-exemption` | 19de945ed | N-4：§1c 的 `openmir2-adjust-bonus-wire` 举例已不在当前 INFO 列表；patch 里 merge-tree `4f0… | 仍存在（a） | 无后续卡/提交认领（批量口径） |
| 68 | `openmir2-stale-pointer-snapshot-exemption` | 19de945ed | ★ 已知人工边界（文档已写）：整份文件（声明+断言）在同一次提交里写好的伪造，机判无差异，只能人工核；「声明行早于命中行」在文件顺序上恒真，**不能**当防… | 仍存在（c） | 无后续卡/提交认领（批量口径） |
| 69 | `openmir2-inventory-b-table-audit` | d8e18cd64 | Minor（合并即过期）：§F 仍写「全仓 stale_pointer 仍 rc=1，4 处全在 session-digest」 —— 那是 base 时点… | 仍存在（a） | 同文档 :737 仍写「全仓 stale_pointer rc=1，4 处全在 session-digest」；实测全仓 rc=0（session-digest 仅 :178 snapshot-exempt；b-table 合并 5634ec03d 的交付在 master） |
| 70 | `openmir2-inventory-b-table-audit` | d8e18cd64 | Minor：`:420` 的 T-142 残余只列 ⑨⑩⑪，`whitelist.md:624` 同段还有 ⑫（seed/写回仅覆盖三字段）与 ⑬（`sel… | 仍存在（a） | 同文档 :420 的 T-142 残余只列 ⑨⑩⑪；whitelist.md:624 同段实有 ⑨⑩⑪⑫⑬ |
| 71 | `openmir2-inventory-b-table-audit` | d8e18cd64 | ★ 本卡结论对拆卡流程的意义：**B 表不能作为唯一依据**。今天已四次因盘面前提过时差点派错卡（§6 #5、SETOFFLINE、§6 #1、§6 #4）… | 仍存在（c） | 无后续卡/提交认领（批量口径） |
| 72 | `openmir2-runbook-preflight-refresh` | d43c8c3ca | Trivial（reviewer-3，非阻断）：§1.7 标题内注与附录「两者互换」混用了 base→final 与上轮中间态两个基准，建议改「顺延」。终态… | 仍存在（a） | 无后续卡/提交认领（批量口径） |
| 73 | `openmir2-runbook-preflight-refresh` | d43c8c3ca | ★ 数字口径教训（值得复用）：`card_window --selftest` 在不同机器上 22s / 45–53s，**差 2.4 倍**；文档最终改为… | 仍存在（c） | 无后续卡/提交认领（批量口径） |
| 74 | `openmir2-runbook-preflight-refresh` | d43c8c3ca | ★ 归因教训：复核上轮说「238 是 `4e0bfd2b8` 的旧基线」**有误**——逐 commit 实跑证实链条是 `4e0bfd2b8` 198 ⇒… | 仍存在（c） | 无后续卡/提交认领（批量口径） |
| 75 | `openmir2-runbook-preflight-refresh` | d43c8c3ca | ★ 环境提醒：评审期间 `E:/tmp/wt-rev-inv`（reviewer-3 的身份 worktree）被另一条车道检出到别的分支并留下 WIP（`… | 仍存在（c） | 无后续卡/提交认领（批量口径） |
| 76 | `openmir2-board-merged-column` | 27a58441a | F5（reviewer-5，非阻断）：selftest 只钉「未知 kind 算卡片」，未钉「已知卡片 kind 不得进 NON_CARD_KINDS」——… | 仍存在（b） | board_coverage_gate 无 `isdisjoint({issue,patch,status,...})` 断言 |
| 77 | `openmir2-board-merged-column` | 27a58441a | ★ 口径裁决（已落卡）：NON_CARD_KINDS 用**封闭集合**，**未知 kind 一律按卡片处理** —— 防止将来新线程类型静默消失。复核已打… | 已消解 | board_coverage_gate 已用封闭 NON_CARD_KINDS + 未知 kind 按卡处理（提交 95cf6f690；gate:59 与 :194-200） |
| 78 | `openmir2-board-merged-column` | 27a58441a | ★ 那 11 张卡**本卡不负责收口**：只让它们重新可见。逐张处置归各卡主。 | 仍存在（c） | 无后续卡/提交认领（批量口径） |
| 79 | `openmir2-leader-process-lessons-20261024` | 2723be3d5 | ★ 核心内容：规则 21-24 的**实例与本文件同源**（另有卡 openmir2-leader-queue-rules-21-24 把规则正文写进 le… | 仍存在（c） | 无后续卡/提交认领（批量口径） |
| 80 | `openmir2-leader-process-lessons-20261024` | 2723be3d5 | ★ 已知人工边界（文档已写）：PS 5.1 与 7 的 `.Count` 差异、cwd 型进程归不到本 worktree 时只能 INDETERMINATE… | 仍存在（c） | 无后续卡/提交认领（批量口径） |
| 81 | `openmir2-leader-process-lessons-20261024` | 2723be3d5 | ★ 复算数字的敏感性已写进 §4：对 `（…）` 注记极敏感（剥 19/18/14 vs 不剥 18/17/13）；两条被裁 head 若不排除会得 21/… | 仍存在（c） | 无后续卡/提交认领（批量口径） |
| 82 | `openmir2-drain-order-gate-followup` | 48f7d7c0d | Minor（reviewer-3）：S-2 归属清单称 distance-gate 用例「断言 removed/view_updates」，实际只断言 （）… | 重复 | 同卡后续 status 的第 85 条为完整版 |
| 83 | `openmir2-drain-order-gate-followup` | 48f7d7c0d | Minor（reviewer-3，**变异存活**）：删掉  里 / 两行 ⇒ **2 passed rc=0** —— 家族新增在**当前无夹具求值**（… | 重复 | 同卡后续 status 的第 86 条为完整版 |
| 84 | `openmir2-drain-order-gate-followup` | 48f7d7c0d | Nit： 现在两次调用 （纯读、无 RNG/无副作用）。 | 重复 | 同卡后续 status 的第 87 条为完整版 |
| 85 | `openmir2-drain-order-gate-followup` | 9e274636e | Minor（reviewer-3）：S-2 归属清单称 distance-gate 用例「断言 removed/view_updates」，实际只断言 `v… | 仍存在（a） | 无后续卡/提交认领（批量口径） |
| 86 | `openmir2-drain-order-gate-followup` | 9e274636e | Minor（reviewer-3，**变异存活**）：删掉 `is_appear` 里 `SM_DEATH`/`SM_SKELETON` 两行 ⇒ **2 … | 仍存在（b） | 无后续卡/提交认领（批量口径） |
| 87 | `openmir2-drain-order-gate-followup` | 9e274636e | Nit：`lib.rs` 现在两次调用 `loader.load_map_quest`（纯读、无 RNG/无副作用）。 | 仍存在（c） | 无后续卡/提交认领（批量口径） |
| 88 | `openmir2-dynamic-count-stamp-gate` | 0d3651cf1 | ★ 结构性判断材料（作者在 status 里给出，review 抽样核过）：allow-list 92 条里 **23 条（25.0%）** 属「maste… | 仍存在（c） | 无后续卡/提交认领（批量口径） |
| 89 | `openmir2-dynamic-count-stamp-gate` | 0d3651cf1 | ★ 建议的结构性解法（**未实现，留给另一张卡**）：以「文件/段落级戳自动匹配」为主（复用 snapshot-exemption 的受检快照骨架），「围栏… | 已消解（部分） | 主方案（段落级快照声明）已实现（cd045154c §12）；「围栏代码块豁免」半未做且被文档否决（dynamic-count-stamp-gate:608-609「本门不豁免代码块…继续走 allow-list」；selftest ㉗ 只钉围栏内 # 注释不切段）；若仍需降 allow-list rot 须另立卡 |
| 90 | `openmir2-dynamic-count-stamp-gate` | 0d3651cf1 | Minor：`board_coverage_gate` 目前未接入本门的 allow-list 统计口径；`--max-gap=24` 对表格行仍偏紧（同行… | 仍存在（a） | dynamic_count_stamp_gate 未见 board_coverage 口径（grep 0） |
| 91 | `openmir2-qfunction-hidden-merchant-world-wiring` | f2ca893d7 | Minor（reviewer-5）：`IsQuest` 的取舍表述不完整 —— oracle 侧 `@ReloadManage` → `NormNpc.cs… | 仍存在（a） | 无后续卡/提交认领（批量口径） |
| 92 | `openmir2-qfunction-hidden-merchant-world-wiring` | f2ca893d7 | Minor：`lib.rs` 现在两次调用 `loader.load_map_quest`（纯读、无 RNG/无副作用）。 | 仍存在（c） | 无后续卡/提交认领（批量口径） |
| 93 | `openmir2-qfunction-hidden-merchant-world-wiring` | f2ca893d7 | Nit：不落格仍登记 display（AOI 取不到）。 | 仍存在（c） | 无后续卡/提交认领（批量口径） |
| 94 | `openmir2-window-scenario-dryrun-sweep` | 8e69a2a8d | Nit：报告 §2 说「如需复跑见 §4」，但 §4 只有 dry-run 复算命令、没有 `--selftest` 命令行（悬空指针）。 | 仍存在（a） | 无后续卡/提交认领（批量口径） |
| 95 | `openmir2-window-scenario-dryrun-sweep` | 8e69a2a8d | Nit：patch body 里的 `conflict_marker: OK 0/7460` 是加文件前的旧计数（head 上 `git ls-files`… | 仍存在（c） | 无后续卡/提交认领（批量口径） |
| 96 | `openmir2-leader-queue-rules-21-24` | 8e3b85de6 | Minor（reviewer-6，`M-8-tie`）：`max(key=entry.ts)` 在**多条 verified 条目共享同一最大 ts** 时… | 仍存在（b） | leader-queue:179 仍写「一律按 entry.ts 取 max」，无平局分支 |
| 97 | `openmir2-leader-queue-rules-21-24` | 8e3b85de6 | ★ 该卡的实例链值得记：规则 24 经历两轮返工 —— 第一轮拿「本来就等价的命令」当对照；第二轮拿「两种写法都失败」当对照；最终版本在真实 cmd.exe… | 仍存在（c） | 无后续卡/提交认领（批量口径） |
| 98 | `openmir2-merged-r-cards-window-registration-audit` | b80b9caaa | Minor（reviewer-3）：§5.4 有 **off-by-one** —— state-log-plumb 写 `:352`、server-sta… | 仍存在（a） | audit 仍写 `:352` / `:351`（实际 :353 / :352） |
| 99 | `openmir2-merged-r-cards-window-registration-audit` | b80b9caaa | ★ 本卡教训（Leader 自己）：第一轮我建卡时没写清「goal_gate 读的是 `window_required` 而不是 `window_items… | 仍存在（c） | 无后续卡/提交认领（批量口径） |
| 100 | `openmir2-merged-r-cards-window-registration-audit` | b80b9caaa | ★ 反事实复算方法（可复用）：改完后**删掉自己的改动再跑一次**，确认「差异只出现在目标卡上」—— 这是证明「没有掩盖别的卡」的唯一办法。 | 仍存在（c） | 无后续卡/提交认领（批量口径） |
| 101 | `openmir2-stale-pointer-blame-failclosed` | 801e51131 | Minor（reviewer-3，**自造变异存活**）：`if rc != 0: return 0`（blame **执行失败**当可信）⇒ 31/31 … | 仍存在（b） | selftest 无 blame rc≠0 夹具（只有 Not Committed Yet 支） |
| 102 | `openmir2-stale-pointer-blame-failclosed` | 801e51131 | Minor：`docs/stale-pointer-gate-20261024.md:285` §3.5 **仍引用旧串** `2 snapshot-exe… | 已消解 | errata 卡合并 917376a45：旧串已删（grep 无命中） |
| 103 | `openmir2-stale-pointer-blame-failclosed` | 801e51131 | Minor：§5-16「与逐行 INFO 一致」措辞不准 —— 实测是 **3 行 / 5 条 INFO**（session-digest :57 一条、:… | 仍存在（a） | 无后续卡/提交认领（批量口径） |
| 104 | `openmir2-window-profile-magic-preset` | a62c3d408 | Minor（reviewer-5）：`72c26da9f` 是**返工前 head**，合并后不在 master 上；下次触碰该文档时宜把 3 处快照戳改成… | 仍存在（a） | runbook:429 / sweep:29 仍用返工前 head 72c26da9f |
| 105 | `openmir2-window-profile-magic-preset` | a62c3d408 | ★ 表格截断的真实归因（reviewer-5 纠正作者）：base 在 `spell_cast` 行后**本来没有空行**；空行是首版返工随引用块一起插进去… | 仍存在（c） | 无后续卡/提交认领（批量口径） |
| 106 | `openmir2-dynamic-count-paragraph-snapshot-exemption` | e37759de5 | Minor（reviewer-6，N-1）：`docs/dynamic-count-stamp-gate-20261024.md:558-570` 的 M-… | 已消解 | dynamic-count-doc-errata 合并 2c1bfaaad（变更 8371c6532）：M-8 已按实测重写 |
| 107 | `openmir2-dynamic-count-paragraph-snapshot-exemption` | e37759de5 | Minor（reviewer-6，N-2）：同文档 §3 仍写「现行 92 条」、§4/§5/§8 仍写 21/21，与工具现状（89、32/32）及本文 … | 已消解 | dynamic-count-doc-errata 合并 2c1bfaaad（变更 8371c6532）：92→88、21/21→32/32 |
| 108 | `openmir2-dynamic-count-paragraph-snapshot-exemption` | e37759de5 | ★ 标为「仅登记」的一项：**标题行上的计数永不豁免**（§12.4 + 4 条 allow-list）—— 机制本身用夹具仍可红；修它需先定义「标题行计数… | 仍存在（c） | 无后续卡/提交认领（批量口径） |
| 109 | `openmir2-stale-pointer-report-errata` | 284b352fb | Minor（reviewer-3）：§3.5/§4 把 `5848a872b` 叫作「本卡 head / 现行 head」，它其实是 base/ref（真 … | 仍存在（a） | stale-pointer-gate:314/321 仍把 5848a872b 叫 head |
| 110 | `openmir2-stale-pointer-report-errata` | 284b352fb | Minor：「5 条 INFO」指 5 条 INFO snapshot-exempt，**总 INFO 是 12**（另 7 条非账本卡号）。 | 仍存在（a） | 同处仍写「5 条 INFO」，未与总 INFO 12 区分 |
| 111 | `openmir2-stale-pointer-report-errata` | 284b352fb | Minor：docstring 先说 `goal-gate-full:47` 是「表头」后说是「统计行」——`:47` 是统计行，`:46` 是表头。 | 仍存在（a） | docstring:58-59 同时说 table header 与 statistic-row label |
| 112 | `openmir2-stale-pointer-report-errata` | 284b352fb | Minor（**pre-existing，非本卡引入**）：§3.4 第 6 行的「blame 03:49Z」是原始写作时间；当前 `git blame` … | 仍存在（c） | 无后续卡/提交认领（批量口径） |
| 113 | `openmir2-stale-pointer-report-errata` | 284b352fb | ★ Leader 侧记录：**我给本卡的 acceptance 写了「selftest 应 32/32」，实际是 31/31** —— 32 是 `dyna… | 仍存在（c） | 无后续卡/提交认领（批量口径） |
| 114 | `openmir2-needs-human-refresh` | 37cb5acbe | Minor（reviewer-3）：**旧→新痕迹口径写反** —— §0:12 / §1:39 的「旧：123」是旧的 **merged-only** 字… | 仍存在（a） | needs-human:12 仍写「旧：123」 |
| 115 | `openmir2-needs-human-refresh` | 37cb5acbe | Minor：§1:40-42 因果不准 —— `未开工=0` 的真正原因是该卡 `merged=true`；仅 in-progress、无 merge/pa… | 仍存在（a） | 无后续卡/提交认领（批量口径） |
| 116 | `openmir2-needs-human-refresh` | 37cb5acbe | Minor：§5:398 旧行号端点错（接缝旧值应为 `1288-1311`，写 1307 只到 `_FAIL`），且新旧指针语义不一致（旧 5809=注释… | 仍存在（a） | 无后续卡/提交认领（批量口径） |
| 117 | `openmir2-needs-human-refresh` | 37cb5acbe | Minor：§1:54 标「上一版原文」但只有 2 行截断节选；§1.5:101 已完整保留旧输出，建议改称「节选」。 | 仍存在（a） | 无后续卡/提交认领（批量口径） |
| 118 | `openmir2-needs-human-refresh` | 37cb5acbe | ★ 工具措辞缺陷（建议另开卡或并入下次动 goal_gate 时修）：detail 字符串写「window_required 131（**其中**未合并 2… | 仍存在（b） | goal_gate.py:765 仍写「（其中未合并 %d）」（快照后已漂移：fd2db51e9 已改「另未合并」） |
| 119 | `openmir2-window-profile-testdb-preset` | 187f5dcee | Minor（reviewer-3，M-1）：22b 的第 4 个合取项 `wtd_builtin_db == from_alt_db` **恒真**（alt… | 仍存在（b） | card_window.py:6166 `wtd_builtin_db == from_alt_db` 仍在 |
| 120 | `openmir2-window-profile-testdb-preset` | 187f5dcee | ★ 本卡的价值边界（作者与复核一致认定）：`window-testdb` 与 `alt` **逐格等价**，**不让任何场景新通过**；5 个 placem… | 仍存在（c） | 无后续卡/提交认领（批量口径） |
| 121 | `openmir2-window-profile-testdb-preset` | 187f5dcee | ★ Leader 建卡失误记录：建卡时我把「`csharp-default` 下 rc=2」读成「这几个场景缺 test 库」，**没有先看 `alt` 下… | 仍存在（c） | 无后续卡/提交认领（批量口径） |
| 122 | `openmir2-window-card-scenario-coverage` | eea668dd6 | Minor（reviewer-5，F-1~F-5）：摘要分母 139/135 的表述；§1.2 归因句写「卡面要求含未合并的」只能解释 2 张（真正依据是 … | 仍存在（a） | coverage 报告 §6/§1.2/§4.6 的表述未改 |
| 123 | `openmir2-window-card-scenario-coverage` | eea668dd6 | 正向：报告自查出 `multi-session-probe` 文档/账本不一致、以及 `goal_gate` 详情行「131（其中未合并 2）」的歧义，均以… | 仍存在（c） | 无后续卡/提交认领（批量口径） |
| 124 | `openmir2-ci-gate-declaration` | 816fc93e8 | Minor（reviewer-2）：§1「effective 组合**永不重跑**／强制重跑只有三条路」**对带 `schedule` 的任务不成立** —… | 仍存在（a） | ci-gate 文档仍把「永不重跑」写成全局结论 |
| 125 | `openmir2-ci-gate-declaration` | 816fc93e8 | Minor（reviewer-2）：§5 的 `--key "$GD/walgit/keys/<principal>.ed25519"` **只对 iden… | 仍存在（a） | 无后续卡/提交认领（批量口径） |
| 126 | `openmir2-ci-gate-declaration` | 816fc93e8 | ★ 合并后 CI 会开始对 `refs/heads/master` 运行 —— **11 条任务里 `rust-test` 约 283s、`rust-cli… | 仍存在（c） | 无后续卡/提交认领（批量口径） |
| 127 | `openmir2-dynamic-count-doc-errata` | e72d4cb0d | Minor（reviewer-6，O-1）：§3 存在一个**段落级声明候选** —— L68 既存措辞「文档级快照」+ 本轮 sha/时间行落在 ±2 行… | 仍存在（a） | dynamic-count:68 仍写「文档级快照」 |
| 128 | `openmir2-dynamic-count-doc-errata` | e72d4cb0d | ★ 本卡的核心教训（作者自己发现并写进 §12.3）：**INFO 基数会随 master 前进而变**（段落声明逐份合入，基数 +1）。所以「4 条」这类… | 仍存在（c） | 无后续卡/提交认领（批量口径） |
| 129 | `openmir2-goal-gate-wording-fix` | 927c0e939 | docs/needs-human-20261024.md 的 3 处「其中未合并」引用待 card/openmir2-needs-human-window-… | 仍存在（a） | 无后续卡/提交认领（批量口径）；快照后已漂移：fd2db51e9（card/goal-gate-wording-fix）已修 3 处引用 |
| 130 | `openmir2-needs-human-window-coverage-gap` | a5f0076c5 | Info（reviewer-3，无需改）：`§0#6` 的「27 含 owner 裁定 L 2 张」与 `:107/:127` 的 goal_gate 原文… | 仍存在（a） | 无后续卡/提交认领（批量口径） |
| 131 | `openmir2-needs-human-window-coverage-gap` | a5f0076c5 | ★ 本卡确立的关键事实：**条件② 无法仅靠现有 18 个场景关闭** —— 45 可覆盖 / 27 替代判据 / 63 无场景（其中 25 无任何登记）。… | 已消解 | 已写入 needs-human §0#6（窗口覆盖缺口，45/63/25；提交 56d253366） |

## 3. 「仍存在」的分类与处置

### a. 文档措辞/数字（49 条）

条目：#2、#4、#6、#7、#9、#11、#13、#17、#18、#19、#20、#24、#26、#27、#28、#30、#31、#37、#40、#46、#47、#55、#58、#59、#60、#67、#69、#70、#72、#85、#90、#91、#94、#98、#103、#104、#109、#110、#111、#114、#115、#116、#117、#122、#124、#125、#127、#129、#130。**处置**：批量修（一份文档卡，逐条改并留旧→新痕）；成本约半天。

### b. 判据缺口（18 条）

条目：#1、#3、#15、#16、#22、#23、#25、#38、#43、#48、#51、#52、#76、#86、#96、#101、#118、#119。**处置**：优先修：每条补夹具/断言并造变异验证；成本约 0.5–1 天/颗，建议按卡拆 1–2 条/卡。

### c. 已知限制/过程记录（42 条）

条目：#5、#8、#10、#12、#32、#33、#36、#45、#49、#50、#53、#54、#56、#61、#62、#63、#68、#71、#73、#74、#75、#78、#79、#80、#81、#87、#88、#92、#93、#95、#97、#99、#100、#105、#108、#112、#113、#120、#121、#123、#126、#128。**处置**：保留并写进已知边界；不排期，除非再次误导决策。

### d. 需要 owner 或窗口（1 条）

条目：#14。**处置**：交 owner 一句话裁定；成本 = 一次决策。

## 4. 建议立刻修（≤10 条，按「会不会掩盖真问题」排序）

1. **#51/#52** `openmir2-review-principal-check`（eac14d079，判据缺口）：review_principal_check 的 F2 是 fail-open：畸形/占位 reviewer 仍 rc=0；损坏条目被跳过也可能返回 OK。
2. **#101** `openmir2-stale-pointer-blame-failclosed`（801e51131，判据缺口）：blame `rc != 0` 分支无夹具（自造变异存活）：fail-closed 可能被下一次「顺手简化」静默回退。
3. **#86** `openmir2-drain-order-gate-followup`（9e274636e，判据缺口）：删 `is_appear` 的 SM_DEATH/SM_SKELETON 两行 ⇒ 2 passed rc=0：死亡帧次序家族无求值夹具。
4. **#119** `openmir2-window-profile-testdb-preset`（187f5dcee，判据缺口）：22b 第 4 个合取项恒真：测试通过不证明「内置条目来自构造器」。
5. **#38** `openmir2-stale-pointer-card-status-check`（1cbe8890c，判据缺口）：⑰ 未钉 span 级语义：把字面排除改成「整行否决」仍全绿，可能吞掉真断言。
6. **#76** `openmir2-board-merged-column`（27a58441a，判据缺口）：board_coverage_gate 未钉「已知卡片 kind 不得进 NON_CARD_KINDS」：新线程类型可静默消失。
7. **#48** `openmir2-merchant-script-load`（042266637，判据缺口）：4 条新用例只覆盖 fail/缺失臂，ok 臂的副作用与 use_slots 回写无构造级断言。
8. **#96** `openmir2-leader-queue-rules-21-24`（8e3b85de6，判据缺口）：max(ts) 平局回退遍历顺序：派单可能选中错误的 patch/status。
9. **#118** `openmir2-needs-human-refresh`（37cb5acbe，判据缺口）：goal_gate 详情行「其中未合并 2」与独立计数器不符，已实际误导 Leader 一次。〔快照后已修：fd2db51e9 改「另未合并」〕
10. **#114** `openmir2-needs-human-refresh`（37cb5acbe，文档措辞/数字）：owner 决策页把旧总数写成 123（应为 125），会让 owner 低估增量。

## 5. 判不准（1 条）

- #29 `openmir2-needs-human-count-sync` (8f719461e)：v3 已重写该段；`540310bd3` 在当前文档中不可定位。carried 原文：v2 merge_tree 正确值 540310bd3（审计链请以此为准）

> 返工说明（openmir2-worker-58，@ fd2db51e9）：原报告把 #58/#59/#60/#61/#69/#70 判为「判不准」，
> 理由「目标文档不在 master」不成立——`docs/rewrite-remaining-inventory-20261022.md` 在 master
> （top5 合并 `3fa6899a9`、b-table 合并 `5634ec03d` 的交付写集就是它，各自只改这一个文件），
> 六条所述缺陷逐条复核仍在（详见 §2 表行），已改判「仍存在」并入 §3a/§3c。

## 6. 复算命令

```sh
# 枚举（一次聚合；不按卡起子进程）
git for-each-ref --format="%(refname) %(objectname)" refs/collab/inbox > /tmp/inbox.txt
# 逐 blob 解析 kind==status 且 body.carried 非空；快照见 refs/collab/meta/snapshot
# 判定：已消解需给后续卡/提交；仍存在需能指到文件:行或说明无后续认领
```

## 7. 残留

- 只读账本与仓库；未改任何卡的 status、未改 tools/、未改其它文档。
- 本报告不是「修完清单」：分类与判定可被后续卡推翻，推翻时以后续卡的证据为准。
- 快照后漂移：#118/#129 的「仍存在」在其数据时点成立；master 12:03 合入 fd2db51e9（card/goal-gate-wording-fix）后已修，详见表行批注。
