# OpenMir2 Leader 长期目标与循环重启手册（2026-10-23）

> owner 裁决：Leader 循环是**长期任务，目标达成前不停**；队列枯竭时必须继续拆卡，不允许空转。

## 1. 终点定义（达成即停）

同时满足以下四条，才允许把循环标为 `DONE`：

1. **脚本派发**：部署语料用到的派发目标 **100% 已实现**（当前 106/106，保持）；
2. **R 卡证据**：所有 R 级卡要么有真机/协议对拍证据，要么有**显式登记 + 明确理由**（`window_required` 或 whitelist `T-*` 且 owner 复核过）；不允许「代码已合但无证据且无登记」；
3. **B 类清零**：盘点里的 B 类（未接线/未实现）卡全部关闭或有 owner 裁决的替代方案；
4. **门禁恒绿**：`cargo fmt/clippy/test --workspace/msgcodegen` + `whitelist_anchor_gate` + `m3_whitelist_registry_audit` + **`tool_selftest_gate`（含 `--inventory` 未分类检查）** 全绿，且 `walgit` 上 master 与本地一致。

进度来源：`walgit collab board --format json`（A/B/待审/在跑/待认领 五个数）+ `docs/rewrite-remaining-inventory-20261022.md`。

### 1.0 基础设施阻塞排除项（2026-10-25，owner 裁定）

- **owner 2026-10-25 裁定「先跳过」**：`openmir2-gm-batch-5b-cross-server` 依赖的**跨服传输与拓扑**（`PlanesServer`/`PlanesClient` + 主/分服）**在仓库内从未实现**，其 6 条 GM 命令的 oracle 清单已合入 `99a4d30c4`，实现部分 blocked-by G11。⇒ 该卡列入**基础设施阻塞排除项**，条件② 不再把它计入缺口；G11 作为独立基础设施卡另行排期。
- **实现方式（显式、非静默）**：`tools/goal/goal_gate.py` 的 `MERGED_NOT_CLOSED_EXEMPT` 表，**每项必须带 `adjudication_oid`** —— 本项为裁定条目 **`ab2a1cf8d37cc3ae02dc1297d4520193fbcaffe8`**（`walgit collab thread openmir2-gm-batch-5b-cross-server`）。条件② 的 `detail` 每次运行都会打印「**另排除 N 张基础设施阻塞：<卡 id>**（owner 裁定，裁定条目 …）」；**豁免项若不再是 `merged_not_closed`，detail 会打 ⚠ 提醒同步删除该条目**（防腐烂）。
- **不影响**：R 卡总数、`window_required` 桶、tier 政策、任何其它卡的归类。**条件② 绿的语义仍只是「没有无证据无登记的卡」，不等于「R 卡都验过了」**（见下方 1.1）。

### 1.1 判读口径（2026-10-23 补充，必读）

- **「条件② PASS」= 有登记，不等于已验证。** 机判 `tools/goal/goal_gate.py` 的条件②只判断「每张 R 卡要么有真机/对拍证据，要么有显式登记 + 明确理由」。当前 ~90 张 R 卡是
  `window_required: true` —— 它们**仍欠真机窗口**。把条件②读成「R 卡都验过了」是误读，真机窗口的欠账要看各卡 `status.work` 里的窗口项。
  ⚠️ **口径已订正（2026-10-23 16:5x，卡 `openmir2-window-batch-plan` 逐条复算）**：早前用「`board` 全文 grep『已合入 master』= 114」
  得出的数字**是错的**——那 114 里含 **70 张已 closed** 与 **23 张 tier=L（本就不欠窗口）**，真欠账只有 21 张。
  正确口径（对 6793 条不可变协作条目做时点重建）：
  - 「已合入 master」字面命中 114 = 70 closed + 23 tier=L + **21 真欠账**；
  - 再加 **149 张**欠窗口但看板措辞为「已落地（tier=R 不收卡）/R 级记账补登」等、**3 张**快照后才合入 ⇒ **共 173 张**；
  - 「窗口欠项」四个字在看板条目里**从未出现**（0 命中），早前的 151 也无法复现——那两个数是 Leader 文档里自造的措辞。
  **173 张的真机窗口，是终点条件②的全部欠账。** 分批见 `docs/window-batch-plan-20261023.md`：
  第 1 批 53 张（41 用 9 个可跑场景 + 12 用既有 pair 夹具）、第 2 批 4 张、第 3 批 81 张（需新场景/新帧）、不可协议对拍 35 张。
  ⇒ **第 1 批跑完也只消 31%**，剩余要靠新场景（R1–R7）与替代判据。该数字随合并变化，引用必须带快照戳。
- **条件③「B 类清零」的口径**：以 `recon_gates.py dispatch` 的 tracked 清单为准；`counts.tracked == 0` **且行集非空** 才判 PASS（2026-10-23 修掉一处「B 真清零时永远判需人工」的死锁）。
- **动态量必须带快照**：board 计数、R 卡总数、FAIL 数都是实时量。写进任何文档时必须标「截至 `<commit>` @ `<时间>`」，否则复跑者会按过期数字判读（已发生三次）。
- **「查不到」不许写成「已验证」**：判据工具在无法验证时必须输出「需人工」，这是本日三次返工的共同根因（goal_gate 的 `tracked=0 ⇒ PASS`、孤儿审计的「查不到即不可达」、盘点文档的「不可复算，停用」）。


## 2. 循环职责（每次巡检至少推进一件）

优先级：**判死接手 > 合并 > 派活 > 拆卡 > NOOP 报告**。

- **判死接手**：agent transcript >20 分钟无写入 ⇒ 判死，在**原 worktree** 起 finisher，不重做；
- **合并**：patch → 不同 principal 复核 → `merge --no-ff` → 完整门禁 → push → `merge_result` + `status`；
- **派活**：在跑 < 4 且写集不重叠 ⇒ 派 1 张（worker-4/5/10 轮换，先签 status 认领）；
- **拆卡**：待认领 < 3 ⇒ 从盘点/差异/残余里拆出下一个可执行单元，建成签名 `issue` 卡（goal/write_set/oracle 行号/acceptance/forbidden/tier）。
- **清理**：只删分支（本地 + `git push walgit --delete`），**绝不删 worktree**（worktree = 身份 = Ed25519 私钥）；腾空间只清 `*-target`。
  > **磁盘现状（2026-10-23 19:5x 实测，口径已订正）**：清理前 `E:` 已用 3.1T / 余 644G（84%）；
  > `du -sm /e/tmp/*target` 实测 **target 总量 1,410,563 MB ≈ 1.41 T**（不是「每车道 15G × 十几条」的量级）。
  > 按「删除 90 分钟未修改的 target（137 个）、保留最近 90 分钟的活跃车道」清理后 ⇒ **已用 1.6T / 余 2.3T（41%），回收约 1.65 T**。
  > ⇒ **正确的量级感：一轮批量复审（4 张卡 × 多轮返工 × 每次全量测试的新 target）≈ 240G**，不做清理一天能吃掉整块盘。
  > 做法：只清 `*-target`、**绝不碰 worktree**；保留最近 90 分钟内的（在跑车道）；
  > **不要在车道跑测试时清它自己的 target**（会触发全量重编，看起来像「卡住」）。

## 3. 会话重启后如何恢复循环

cron 是**会话内**的（7 天自动过期，会话结束即消失）。换会话/重启后按下面三步恢复：

1. 读本文件 + `docs/leader-loop-20261023.md` + `docs/leader-queue-20261023.md`（镜像）；
2. 重新创建 5 分钟循环（prompt 见 §2 的职责清单，可直接复用上一版 cron 文本）；
3. 先跑一次「判死接手」与「看板同步」，再从优先级最高的一件开始。

## 4. 已知的长期阻塞（要靠 owner 或窗口）

- **GM 132 条未实现**：现网 `Permission` 恒 0，命令体不可达（设计件 §2.4）；除非 owner 批准 `SETPERMISSION` 夹具。
- **GUI-only 证据**：渲染/点击几何/输入法类卡无法用协议对拍替代，需真机窗口。
- **C# 真值栈**：私有 root `E:/tmp/conformance/csharp-root`（15xxx/17xxx、`mir2_test_*`）可与 peer 的默认端口栈并存；对拍时与 Rust 串行、同端口。
- **whitelist 待复核**：T-161..T-173 需 owner 逐条复核留痕。

---

## 5. 机判用法（`tools/goal/goal_gate.py`）

§1 的四条终点条件已落成**一条只读、零副作用、可复算**命令。任何人都能用它把
「目标是否达成」从人读变成可复现的三态表；**它不修东西、不改仓库、不起栈、不连库、不占端口**。

### 5.1 三条命令

```sh
# 快速模式（默认）：跳过 cargo test 与 parse-stats 构建；暖缓存约 20–40s（冷构建约 1m44s）
python tools/goal/goal_gate.py

# 机器可读（stdout 只有 JSON，适合巡检/cron/CI 读）
python tools/goal/goal_gate.py --json

# 完整模式：= --tests --script，四条件全部实测（cargo test 很慢，见 5.3）
python tools/goal/goal_gate.py --full
```

可选开关：

| 开关 | 作用 |
| --- | --- |
| `--tests` / `--skip-tests` | 跑 / 显式跳过 `cargo test --workspace`（快速模式默认跳过） |
| `--script` / `--skip-script` | 构建并跑 parse-stats / 显式跳过（快速模式默认跳过） |
| `--full` | `--tests --script` 的简写 |
| `--script-bin <path>` | 用**预编译**的 `mir2-script-tool` 二进制复算条件1（跳过 cargo 构建，仍读实时输出） |
| `--envir <dir>` | 脚本语料目录，默认 `E:/MirServer/M2GameSvr/Envir` |
| `--repo <dir>` | 仓库根，默认脚本所在仓库 |
| `--timeout N` / `--test-timeout N` | 单命令超时（默认 1800s）/ cargo test 超时（默认 7200s） |
| `--allow-verdict-grep` | 条件2 采信 comment/review 里的 `VERDICT=PASS` 文本标记（默认关闭） |
| `selftest` | 合成输入自检（64 条断言，含 6 颗牙） |

退出码：`0` 四条全 PASS；`1` 有 FAIL；`2` 无 FAIL 但有「需人工」；`3` 脚本自身错误（参数/路径）。

### 5.2 三态怎么看

| 状态 | 含义 | 处置 |
| --- | --- | --- |
| **PASS** | 判据**机判成立**，有实测输出作证据（不是「看起来一样」） | 无需动作 |
| **FAIL** | 判据**机判不成立**；表格下方的「缺口明细」逐条点名（卡 id / 消息名 / 门禁项） | 派卡或转 `needs-human` |
| **需人工** | 脚本**无法机判**：缺输入（未跑 `--tests`/`--script`）、证据链不可信（未验签、board 缺失、弱关联 `T-*`）、命令超时 | **不许**当成 PASS；要么补跑完整模式，要么人判并留痕 |

判定顺序：**任一 FAIL ⇒ 总体 FAIL**；否则任一「需人工」⇒ 总体「需人工」；全 PASS 才 PASS。
**「查不到」永远是「需人工」，绝不默认 PASS**。

### 5.3 四条件各自怎么算（复算口径）

1. **脚本派发 100%（数字来源已查明，不硬编码）**
   来源 = `docs/rewrite-remaining-inventory-20261022.md` §1.2 的 parse-stats 覆盖行。脚本执行：

   ```sh
   cd mir2-rs && cargo run --release --locked -p mir2-script-tool -- \
     parse-stats E:/MirServer/M2GameSvr/Envir --json <临时文件> --require-implemented
   ```

   然后解析两段 stdout：
   * 覆盖行 `语料派发目标覆盖：已实现 N 个（M 次调用）；未实现 K 个`：
     `K == 0 且 rc == 0` ⇒ PASS；`K > 0` ⇒ FAIL；**`K == 0 但 rc != 0` ⇒ FAIL**
     （`--require-implemented` 与覆盖行是同一判据的两条腿，rc 非 0 不许当 PASS）。
   * 语料完整性计数 `文件/加载/缺失/异常` 与 `解码异常文件`：缺文件或解码异常会让
     **参与统计的目标变少**，此时即使打印「未实现 0 个」也不算数 ⇒ 「需人工」。
   * 抽不到覆盖行 / 超时 / 语料目录不存在 ⇒「需人工」。

   **数字从不写死**：detail 打印 `目标 N/（N+K）（去重派发目标）；调用 M 次`，
   「106/106」= 已实现目标/语料用到的目标总数；「106/11829」= 目标数/调用次数，两者分母不同。

2. **R 卡证据**：`walgit collab board --format json` 提供卡片列表/状态列/未验签计数；
   条目正文（`tier`/`window_required`/`merge_result`/comment）由只读 git 对象
   （`refs/collab/meta/snapshot` 折叠区 + `refs/collab/inbox/*` tail）按 `git cat-file --batch` 一次读出。
   **R 卡枚举 = 任一条目的 `body.tier` 归一为 R**（不只 issue：86/161 张卡的 tier 只写在
   review/patch/status 里，只认 issue 会形成「新 R 卡漏写 tier ⇒ 不进 R 总体」的假 PASS 路径）。
   每张 R 卡按下列**优先级**定态：

   | 序 | 判据 | 状态 |
   |---|---|---|
   | 1 | board 未列出该卡 | 需人工（视图未同步/线程状态异常） |
   | 2 | board 报未验签条目 | 需人工 |
   | 3 | **board 列 = 已完成 / status = closed** | **PASS（已合并；不要求线程有 merge_result）** |
   | 3.5 | **未开工/未合并**：无 merge_result、无 patch、无 review、无 comment，且 board 未 closed | **未合并**（不计 PASS 也不计 FAIL，只列清单） |
   | 4 | **已裁定登记**：最新一条带 `window_required` 的 approve review 说 `true` ⇒ window_required；说 `false` 且**专用字段 `why_not_window`/`window_waiver` 非空**（或 `tier_why` 命中免窗口短语白名单且否决词均被否定）⇒ 已裁定不需窗口 | PASS |
   | 5 | `window_required=true`（无复核裁定时，作者/状态里的登记） | PASS |
   | 6 | whitelist `T-*` 登记行**点名本卡** | PASS（弱关联不算） |
   | 7 | comment/review 内有 `VERDICT=PASS`（**仅 `--allow-verdict-grep` 时**） | PASS（默认关闭） |
   | 8 | 线程有 `merge_result merged=true` 但 board 非 closed，**且以上登记都没有** | **需人工**（可能已重开且新工作无证据） |
   | 9 | 线程提到 `T-*` 但登记行未点名本卡 | 需人工（弱关联） |
   | 10 | 以上皆无 | **FAIL（三者皆无，列卡 id）** |

   * **条件②只约束「已开工/已合并」的卡**（I-5）：一张刚建、线程里只有 issue + claim 的卡既不需要证据
     也不需要登记，判成 FAIL 是假阳性。机器代理：`merge_result` / board closed / `patch` / `review` / `comment` 任一存在 ⇒ 判证据；
     只有 issue + claim（或只有 status）的新卡归 `not_merged`（不计 PASS/FAIL，控制台单列前 20 条）。
     该代理同时覆盖「Leader 直接合入但没有 merge_result 条目」的记账缺口卡。
   * **「已裁定登记」必须排在 merged 之前**：否则任何 merged 的卡都会进需人工，把「有显式登记」
     误报成缺口（H-1）。review 的结论字段 `decision` 与 `verdict` 两种写法都认；同一卡多次 review
     的 `window_required` 会不一致 ⇒ **取最新一条带该字段的 approve**，不能对历史做 `any(false)`。
   * **「不需要窗口」必须有免窗口语义，不能只看「理由非空」**（I-1 实证：`tier_why` =「时序/竞态…」
     描述的是改动性质，却被当成免窗口）：专用字段 `why_not_window` / `window_waiver` 优先；回退
     `tier_why` 时必须命中白名单短语（`不需要窗口/无需窗口/不占窗口/不可能只在真机暴露/零 .rs/
     零生产代码/纯文档/纯夹具/纯仓库卫生/生产不可达/等价即放行` 等），且 `时序/竞态/RNG/落库/
     战斗数值/数值/持久化/随机/tick` 若出现即否决（除非处于「不触/无/零/未」等否定语境）。
   * **merged 但 board 非 closed 且已凭登记 PASS** 的卡进 `evidence.merged_open_hygiene`（卫生清单，
     带 `last_status_ts`），**不算条件2 缺口**，供 owner 确认是否重开；张数同时打印在条件2 的 detail 里。
   * **登记型 PASS ≠ 已有真机证据**：`window_required=true` 只代表「登记 + 明确理由」，不等于真机窗口
     已跑（本轮约 95 张此类卡仍欠窗口）。条件② 绿只说明「没有无证据无登记的卡」，不得解读为「R 面已验」。
   * `need_items` 每项带 `last_status_ts`（线程内 `status` 条目的最大 ts），**按升序（最旧先）**排；
     控制台按条件各列前 20 条（条件2 即最旧的 20 张），其余见 `--json`。FAIL 项顺序不动。

   * `VERDICT=PASS` 是文本子串匹配，可能被「计划/预期」误命中 ⇒ **默认不采信**，要显式加
     `--allow-verdict-grep`（当前 0 张卡依赖它）。
   * `whitelist.md` 同一编号出现多行时**取并集**（卡集合合并、行文本拼接），不让覆盖写吃掉卡↔T 关联。
   * 卡 id 匹配会排掉 `refs/collab/meta/principals/*` 里的 principal 名（`openmir2-worker-5` 等不是卡）。
   * 判定 tier 时把散文式 tier（如 `R（战斗数值/时序/持久化面）…`）归一为 R；线程里**任一**条目
     标了 R 就按 R 卡处理（偏向保守：宁可多查一张，不漏一张）。

3. **B 类清零**：跑 `cd mir2-rs && python -I -B tests/parity/coverage/recon_gates.py dispatch --repo .. --json <临时文件>`，
   取 `status == "tracked"` 的行，逐条判：条目名出现在某条 `T-*` 登记行 ⇒「有 T-* 登记」；
   否则其 `card` 在 board 里存在 ⇒「有卡」；都没有 ⇒ FAIL。判定顺序（fail-closed **且不判死终点**）：
   1. rows 为空 / 非 list ⇒ 需人工；
   2. 行缺 `status`（schema 漂移）⇒ 需人工；
   3. 缺 `counts.tracked` 或与实际 tracked 行数不一致 ⇒ 需人工；
   4. **`counts.tracked == 0` 且行集非空 ⇒ PASS（B 类清零）**——否则条件3 永远无法在 DONE 时转 PASS；
   5. 否则按条目判定（**`tracked > 0` 不等于 FAIL**：每个 tracked 条目只要有 `T-*` 登记或看板上有卡，就判 PASS）。
      ⚠️ 2026-10-23 实测教训：某张卡以「tracked 永远 ≥1 ⇒ 条件③ 被卡死」为前提立项，实际 `tracked=1` 时
      条件③ **本来就是 PASS**（该条目有 `T-*` 登记）。此后把该条目正确归类为 implemented、`tracked` 变 0，
      反而因 `counts` 丢 `tracked` 键（`summarize()` 只统计出现过的 status）而退回「需人工」。
      ⇒ **`counts` 必须恒带 `tracked:int`（含 0）**，且「`tracked>0` 是否算缺口」要看条目登记，不看这个数字本身。

   「查不到」与「确认清零」必须分开：recon 给不出结构 ⇒ 需人工；recon 明确 `tracked=0` 且计数自洽 ⇒ PASS。

4. **门禁恒绿**：逐项跑并在 `--json` 里留 rc/耗时——`cargo fmt --all -- --check`、
   `cargo clippy --workspace --all-targets --locked -- -D warnings`、
   `cargo run --locked -p msgcodegen -- --check`、
   `python -I -B tests/parity/m3/whitelist_anchor_gate.py check`、
   `python -I -B tests/parity/m3/m3_whitelist_registry_audit.py audit`、
   **`python -I -B tools/hygiene/tool_selftest_gate.py`（2026-10-26 起，第 8 项；普通全跑已含
   `--inventory` 未分类检查）**，
   加上 `git ls-remote walgit refs/heads/master` 与本地 `master` 的 sha 比对。
   > **为什么之前是「假绿面」**：工具门此前**不在条件④ 内**（`goal_gate` 全文件 grep `tool_selftest` 命中 0），
   > 所以一份 `overall=PASS` 的权威产物可以在工具门红着（52 个工具里 1 个未分类、rc=2）时照样产出。
   > 卡 `openmir2-goal-gate-include-tool-selftest` 把第 8 项接进条件④；卡 `openmir2-goalgate-need-rc3`
   > 按工具门的**退出码语义**改成四档判定：rc=0 ⇒ PASS；**rc=3（工具门自报「只有需人工」，即某个
   > 工具自述判不了/rc=2）⇒ NEED，`need_items` 点名**（不能折叠成红）；rc=2 / 其它 ⇒ FAIL 且
   > `fail_items` 点名（哪个工具未分类/哪条自测红）；工具缺失/无法启动/超时 ⇒ **NEED，不是 PASS**。
   > 口径一句话：**「判不了」既不是绿也不是红**——绿会假绿，红会假红，两种都会毁掉门禁可信度。
   `cargo test --workspace --locked` **只在 `--tests`/`--full` 时跑**；跳过时条件4 =「需人工」，不冒充 PASS。

### 5.4 `--skip-tests` 与完整模式的区别（务必分清）

| | 快速（默认 / `--skip-tests`） | 完整（`--full` 或 `--tests --script`） |
| --- | --- | --- |
| fmt / clippy / msgcodegen / anchor / registry / tool_selftest / master 比对 | 跑（工具门约 +120s） | 跑（同左） |
| `cargo test --workspace` | **跳过** ⇒ 条件4 「需人工」 | 跑（数千用例；冷 target 实测 433.5s，全量 `--full` 约 10.5 分钟） |
| parse-stats 覆盖率 | **跳过** ⇒ 条件1 「需人工」 | 跑（首次 `--release` 构建较慢；可用 `--script-bin` 复用已有二进制） |
| 适用 | 巡检、每次合并前快手 | **宣告终点 `DONE` 前必须跑一次**（四条都要有实测结论） |

> 换句话说：**快速模式永远不可能给出总体 PASS**（条件1、4 至少各有一处「需人工」）。
> 这是刻意的：没跑过的门不许算绿。要看到真正的「四条全绿」，请用完整模式。

### 5.5 前置条件与已知边界

* **协作视图新鲜度由调用方负责**：脚本**不 `git fetch`**（fetch 会写 refs，违反零副作用）。
  跑条件2 前先同步本地视图：

  ```sh
  git fetch walgit '+refs/collab/inbox/*:refs/collab/inbox/*' '+refs/collab/meta/*:refs/collab/meta/*'
  ```

  视图为空 ⇒ 条件2/3 输出「需人工」并提示这条命令，不会空表 PASS。
* **工作区/二进制**：cargo 命令在 `mir2-rs/` 下执行，构建产物落 `target/`（gitignored）；全程 `--locked`，
  不改 `Cargo.lock`。parse-stats 需要 `E:/MirServer/M2GameSvr/Envir`（不存在则「需人工」并说明）。
* **board 未列出的卡**：本仓看板只显示命中列谓词的线程；索引里有 `tier=R`、board 却不显示 ⇒「需人工」
  （本地视图未同步，或线程条目链/状态异常），并列入缺口明细。
* **merged 但 board 未 closed**：若已有显式登记（window_required / whitelist / 已裁定）⇒ PASS，并进
  `evidence.merged_open_hygiene` 卫生清单（带 `last_status_ts`），**不算条件2 缺口**；
* **未开工/未合并的卡**（无 `merge_result`/`patch`/`review`/`comment`）归 `not_merged`：**不计 FAIL 也不计 PASS**，
  控制台单列「未开工/在飞」前 20 条，全量在 `--json` 的 `not_merged_items`；
  没有任何登记的才判「需人工」，供 owner 确认「是否已重开且新工作有证据」。
* **whitelist 重复编号**：`build_whitelist` 取并集（不会丢卡↔T 关联），但仍以「提示」列出重复号，
  登记号唯一性归 `m3_whitelist_registry_audit.py`（该门当前 `scope=T-36+`，T-4/T-6 这类低号重复覆盖不到）。
* **代理的认知边界**：若某卡代码已合但线程零痕迹（无 `merge_result`/`patch`/`review`/`comment`），任何代理都发现不了它——
  本门看不到这种卡。当前无此例，但记为已知盲区（勿当成「已覆盖」）。
* **JSON 结构**：`{schema, generated_ts, repo, envir, mode, conditions[4]{id,name,state,label,detail,evidence},
  overall, overall_label, fail_items[], need_items[], warnings[]}`；`state ∈ {pass, fail, need_human}`；
  条件2 的 `evidence.r_cards[]` 含每卡 `category/board_column/merged/window_required/strong_t/weak_t/comment_evidence`。

### 5.6 自检与牙

```sh
python tools/goal/goal_gate.py selftest --verbose
# selftest：64 条断言，64 通过，0 失败
```

覆盖四条件各自的 PASS / FAIL / 需人工 分支（含覆盖行解析、语料完整性计数、**rc=1 但未实现 0 ⇒ FAIL**
的 fail-closed 耦合、任一条目 tier 枚举、board 已完成、已裁定登记（decision/verdict 两种写法、取最新 approve）、
merged 未 closed、VERDICT 子串默认不认、need_items 按 last_status_ts 升序、弱关联 T 不冒认、未验签、
board 缺失、master 漂移、ls-remote 不可用、tracked 空/schema 漂移/counts 不一致、**tracked=0 且行集非空 ⇒ PASS**、
**approve+false+tier_why=「时序/竞态」⇒ 需人工**、**否定不跨中文逗号/转折词（「不涉及时序，但涉及 RNG」⇒ 拒）**、顿号列表仍被统辖（「不触 RNG、时序、落库」⇒ 放行）、专用 `why_not_window` 优先、未开工卡 ⇒ 未合并、状态优先级），并内置**六颗牙**：忽略未实现计数、删掉 rc 守卫、把三者皆无判成 PASS、把空 tracked 判成 PASS、
把 tracked=0 一律判需人工（H-3 旧口径）、把未开工卡判成 FAIL（I-5）—— 任一退化都会被断言判红。改判定逻辑后先跑 selftest 再跑快速模式。
