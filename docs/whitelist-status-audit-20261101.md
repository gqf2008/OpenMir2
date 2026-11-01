# whitelist「待独立 principal 复核」状态普查（2026-11-01；F2–F6 返工版）

- 卡：`openmir2-whitelist-status-audit`（tier=L）
- 快照：`5f5ec4243154ceff6074c07e794f3f402f4a8df0`（`walgit/master`，本卡分支 base）
- 工具：`tools/whitelist/wl_status_audit.py`（**只读**；`selftest` **24/24**，含 9 条 git 溯源纯逻辑用例）
- 复跑（在仓库根）：

  ```sh
  python -I -B tools/whitelist/wl_status_audit.py census --at 5f5ec4243154ceff6074c07e794f3f402f4a8df0
  ```

- 结果：**85 行**（84 行 `待独立 principal 复核` + 第 256 行散文 `（useitem.rs，待审）`）
  = **81 行已审已合（改写）** + **2 行已合未审（T-2/T-3，保持原样）** +
  **2 行引用块散文（`PROSE_UNCLOSED`，保持原样）**。
  四类计数：**已审已合 81 / 已合未审 2 / 未合并 0 / `PROSE_UNCLOSED` 2**；
  **`TRACE_UNCLOSED` = 0**（没有「行内无卡号且 git 溯源未闭合」的 T 行）。
  其中 82 个 T 行 = 80 已审已合 + 2 已合未审；第 256 行散文 1 行已审已合。

## 1. 口径与坑

### 1.1 approve 有两种字段名（必须都认）

早期 review 用 `body.verdict`，近期用 `body.decision`。只认一个会把「有审的卡」误判成「没审」。
本表两种都认并逐行标注：`decision` 见 T-8..T-11、T-13..T-25、T-33..T-55、T-60..T-68、
T-119、T-125..T-132、T-134/T-135、T-138/T-139、T-142、T-145、T-148、T-152/T-153、T-156 等
（reviewer-3/5/6 的 approve）；`verdict` 见 T-86..T-94、T-97..T-103、T-141、T-149..T-151 等
（reviewer-5 的 approve）。prose useitem 用 `decision`（reviewer-6 `f069955a`）。

「独立」的判据：approve 的 actor **不在**该卡 `kind=patch` 的 actor 集合里。本表所有 approve
均为 reviewer（作者为 `openmir2-worker-*` 或 `openmir2-orchestrator`），无自审。

### 1.2 行内引用的卡不一定是**交付卡**

| 行 | 行内引用 | 实际交付卡 | 依据 |
|---|---|---|---|
| T-50 | `openmir2-m3-rng-scene-driver`（承接卡） | `openmir2-m3-drop-exp-fixture-botsrv` | 本行证据目录 `m3/drop-exp/evidence-drop-exp-fixture-botsrv-20261014/**`；该卡 patch `2c1d8d24` 自述「whitelist.md 的 M3④ 段新增 T-50」 |
| T-58 | `openmir2-m3-quest-state-dump`（**该卡号无对应线程**，承接卡） | `openmir2-m3-drop-exp-wiring` | 该卡 patch `220019d7` 自述「whitelist T-57/T-58（12 文件）」 |
| T-138/T-139 | 行内只写 `audit #3 / 车道 guildofficial` | T-138=`openmir2-t138-request-castle-war`；T-139=`openmir2-guild-official-npc` | T-138 卡标题即「T-138 GuildOfficial @requestcastlewarnowN」；T-139 卡标题含「（audit #3）」 |
| T-142/T-145 | 行内只写车道名 `scriptpd`/`marketdef` | `openmir2-script-playdice-openybdeal` / `openmir2-market-def-impl` | patch `b04975f6` 自述「登记 T-142」；patch `66a8a4c3` 自述「T-145 登记」 |
| T-155 | 行内两个卡 `npc-ctor`（主）+ `guard-ctor`（分项） | `openmir2-npc-ctor-constants` + `openmir2-guard-ctor-constants` | 两卡 patch 均登记 T-155 |

### 1.3 git 溯源：两条路径、两种证据串（review 9581ae98 F2 的返工核心）

**上一版把两条路径写成同一句「merge 窗口 X 加了本行」，对 22/59 行失实**：路径①
（`_trace_intro`）只核「引入 commit 的行」与「X 处的行」内容锚+短语，**从未核 X 的 diff
是否加了本行**（X 是 `merge_result.oid` 账本值，可能=patch head、也可能是后一个 merge）。
返工后两条路径输出**不同**的证据串：

1. **路径①：引入 commit（reviewer 口径）**
   `git log -S "| T-N |"` 找 0→1 加进该行的 commit `C` → 该 commit 是某卡 patch 的 head
   （`refs.head`）或落在 first-parent merge 窗口 → 卡 → **`merge_result.oid` 账本值 `X`**。
   证据串固定为 **「引入 commit `C` → 卡 `card` → merge_result oid `X`（账本值；实际 merge `M`）」**，
   **不再声称 X 的窗口加了本行**。要求：① `C` 自身的行与当前行首列前 60 字符一致（防 T 号复用）；
   ② 卡有非作者 approve；③ `X` 是快照 base 祖先；④ 合并当时该行仍含待复核短语。
2. **路径②：merge 窗口 diff（fallback）**
   逐 first-parent merge 窗口 `P..M` 做 `git diff`，找 `+| T-N |` 且**实质变更**（去掉「编号」与
   「谁审的」两列后有差异——排除 `card-link-debt` 的 legacy 后缀批改）+ 内容锚 + 短语 + approve。
   **只有这条路径**可以写「窗口 `M` 加了本行」；表内该措辞即表示 `git diff P..M` 真的含该 `+` 行。

**`merge_result oid` 列的类型标记**（review F2）：

- `实际 merge M（引入）；账本 merge_result oid X`：先按 first-parent 链二分找**首次包含 C 的
  commit**（即真实合并提交），与账本值不同时并列给出。
- `X（真实合并，引入）`：账本值本身就是引入 merge。
- `X（账本值/单亲；引入 merge 未解析）`：账本值是单亲 patch/delivery commit（可能=patch head），
  且未能定位引入 merge。
- `X（真实合并；引入 merge 未解析）` / `X（未解析）`：其余情况如实标注。

**证据配对口径**：溯源行的 approve 取该卡首个非作者 approve 条目；`merge_result.oid` 是卡线程
账本值；真实引入 merge 单独给出。三者都可在线程/提交里复核。

### 1.4 T-2/T-3 与两行引用块散文

- **T-2 / T-3**：卡 `openmir2-s1-oracle-defects` 线程**没有 `kind=review` 条目**（9 条：issue/
  comment/status×5/patch/merge_result）⇒ 无独立 principal approve ⇒ **已合未审**，原样保留。
  **实际引入 merge = `1894ccd50e54caf173c8dd9a8888a41093b034d1`**（Merge branch
  `worktree-s1-oracle-fixes`，父 `a978da5c` + `607e9b77`；`607e9b77` 即该卡 patch 交付提交）。
  线程账本 `merge_result.oid = cc252146…` 是**部署对账时的 master tip**（其首父即 `1894ccd50`），
  不是本行的引入 merge；`git diff 1894ccd50 cc252146 -- mir2-rs/tests/parity/whitelist.md` 零 T 行。
  §2 行 227/228 已按此写「实际 merge 1894ccd5 → 账本 cc252146」。
- **`:406` / `:432`**：引用块里的块级注记，工具 kind = **`PROSE_UNCLOSED`**（不是
  `TRACE_UNCLOSED`）。它们**可反解**：`:406`「本行待复核」紧邻并指代 **T-54**（其行已写
  「已复核本登记行（openmir2-dev-2 独立验证…）」）；`:432`「本块全部行待复核」指代其后含
  **T-49** 的块（T-49 已改「已审已合」）。本轮按卡面 A2「证据不足宁可留原样」**保守保留**，
  不把块级注记硬改成行级结论。

## 2. 逐行普查表（85 行）

| 行 | T 号 | 交付卡 | 独立 approve（字段 + 条目 oid） | merge_result oid（类型） | 当前文字 | 判定 | 溯源 |
|---:|---|---|---|---|---|---|---|
| 135 | T-60 | openmir2-whitelist-stale-corrections | openmir2-reviewer-5 `d6f58a7d`（decision） tier=L | `46f1586a59566c3433049c6c952bde03712b17be`（账本值/单亲；引入 merge 未解析） | 待独立 principal 复核 | 已审已合 ⇒ 改写 | 引入 commit `46f1586a` → 卡 `openmir2-whitelist-stale-corrections` → merge_result oid `46f1586a`（账本值；引入 merge 未解析） |
| 136 | T-61 | openmir2-whitelist-stale-corrections | openmir2-reviewer-5 `d6f58a7d`（decision） tier=L | `46f1586a59566c3433049c6c952bde03712b17be`（账本值/单亲；引入 merge 未解析） | 待独立 principal 复核 | 已审已合 ⇒ 改写 | 引入 commit `46f1586a` → 卡 `openmir2-whitelist-stale-corrections` → merge_result oid `46f1586a`（账本值；引入 merge 未解析） |
| 137 | T-62 | openmir2-whitelist-stale-corrections | openmir2-reviewer-5 `d6f58a7d`（decision） tier=L | `46f1586a59566c3433049c6c952bde03712b17be`（账本值/单亲；引入 merge 未解析） | 待独立 principal 复核 | 已审已合 ⇒ 改写 | 引入 commit `46f1586a` → 卡 `openmir2-whitelist-stale-corrections` → merge_result oid `46f1586a`（账本值；引入 merge 未解析） |
| 138 | T-63 | openmir2-whitelist-stale-corrections | openmir2-reviewer-5 `d6f58a7d`（decision） tier=L | `46f1586a59566c3433049c6c952bde03712b17be`（账本值/单亲；引入 merge 未解析） | 待独立 principal 复核 | 已审已合 ⇒ 改写 | 引入 commit `46f1586a` → 卡 `openmir2-whitelist-stale-corrections` → merge_result oid `46f1586a`（账本值；引入 merge 未解析） |
| 146 | T-64 | openmir2-world-tick-expansion | openmir2-reviewer-5 `b8cd2d89`（decision） tier=R window_required | `a071631482492b4e777ced21b3819fca60f2ca16`（账本值/单亲；引入 merge 未解析） | 待独立 principal 复核 | 已审已合 ⇒ 改写 | 引入 commit `a0716314` → 卡 `openmir2-world-tick-expansion` → merge_result oid `a0716314`（账本值；引入 merge 未解析） |
| 147 | T-66 | openmir2-post-audit2-followups | openmir2-reviewer-5 `9c4eac6b`（decision） tier=L | `5b118f6572abf1d43a122479df7d194f42577923`（账本值/单亲；引入 merge 未解析） | 待独立 principal 复核 | 已审已合 ⇒ 改写 | 引入 commit `5b118f65` → 卡 `openmir2-post-audit2-followups` → merge_result oid `5b118f65`（账本值；引入 merge 未解析） |
| 148 | T-67 | openmir2-post-audit2-followups | openmir2-reviewer-5 `9c4eac6b`（decision） tier=L | `5b118f6572abf1d43a122479df7d194f42577923`（账本值/单亲；引入 merge 未解析） | 待独立 principal 复核 | 已审已合 ⇒ 改写 | 引入 commit `5b118f65` → 卡 `openmir2-post-audit2-followups` → merge_result oid `5b118f65`（账本值；引入 merge 未解析） |
| 149 | T-68 | openmir2-whitelist-anchor-gate | openmir2-reviewer-5 `a79de344`（decision） tier=L | `2a04d6e29b75f2db0f7325e68d2e0a3eedf428f6`（账本值/单亲；引入 merge 未解析） | 待独立 principal 复核 | 已审已合 ⇒ 改写 | 引入 commit `2a04d6e2` → 卡 `openmir2-whitelist-anchor-gate` → merge_result oid `2a04d6e2`（账本值；引入 merge 未解析） |
| 227 | T-2 | openmir2-s1-oracle-defects | — | 实际 merge `1894ccd5`（引入）；账本 merge_result oid `cc252146` | 待独立 principal 复核 | 已合无独立 approve ⇒ 保持原样 | 引入 commit `607e9b77` → 卡 `openmir2-s1-oracle-defects` → merge_result oid `cc252146`（账本值；实际 merge `1894ccd5`）；该卡无线程 review |
| 228 | T-3 | openmir2-s1-oracle-defects | — | 实际 merge `1894ccd5`（引入）；账本 merge_result oid `cc252146` | 待独立 principal 复核 | 已合无独立 approve ⇒ 保持原样 | 引入 commit `607e9b77` → 卡 `openmir2-s1-oracle-defects` → merge_result oid `cc252146`（账本值；实际 merge `1894ccd5`）；该卡无线程 review |
| 246 | T-8 | openmir2-c6-loadgate-idempotent | openmir2-reviewer-3 `03238f5c`（decision） | `6f2856da`（真实合并，引入） | 待独立 principal 复核 | 已审已合 ⇒ 改写 | 引入 commit `81e387d8` → 卡 `openmir2-c6-loadgate-idempotent` → merge_result oid `6f2856da`（账本值） |
| 247 | T-9 | openmir2-c6-loadgate-idempotent | openmir2-reviewer-3 `03238f5c`（decision） | `6f2856da`（真实合并，引入） | 待独立 principal 复核 | 已审已合 ⇒ 改写 | 引入 commit `81e387d8` → 卡 `openmir2-c6-loadgate-idempotent` → merge_result oid `6f2856da`（账本值） |
| 248 | T-10 | openmir2-c6-loadgate-idempotent | openmir2-reviewer-3 `03238f5c`（decision） | `6f2856da`（真实合并，引入） | 待独立 principal 复核 | 已审已合 ⇒ 改写 | 引入 commit `81e387d8` → 卡 `openmir2-c6-loadgate-idempotent` → merge_result oid `6f2856da`（账本值） |
| 249 | T-11 | openmir2-c6-loadgate-idempotent | openmir2-reviewer-3 `03238f5c`（decision） | `6f2856da`（真实合并，引入） | 待独立 principal 复核 | 已审已合 ⇒ 改写 | 引入 commit `99f80be0` → 卡 `openmir2-c6-loadgate-idempotent` → merge_result oid `6f2856da`（账本值） |
| 250 | T-12 | openmir2-botsrv-world-evidence | openmir2-reviewer-4 `627575c0`（decision） | `7a878095`（真实合并；引入 merge 未解析） | 待独立 principal 复核 | 已审已合 ⇒ 改写 | 每个卡都有非作者 approve 且 merged |
| 256 | PROSE-useitem | openmir2-m3-bag-equip-alignment | openmir2-reviewer-6 `f069955a`（decision） | `60095f65`（真实合并；引入 merge 未解析） | （`useitem.rs`，待审） | 已审已合 ⇒ 改写 | 每个卡都有非作者 approve 且 merged |
| 264 | T-13 | openmir2-m3-bag-equip-alignment | openmir2-reviewer-5 `d0b9f014`（decision） | `40ac9258`（真实合并；引入 merge 未解析） | 待独立 principal 复核 | 已审已合 ⇒ 改写 | merge 窗口 `40ac9258` 加了本行（内容锚 + 实质变更 + 短语；真实合并） |
| 265 | T-14 | openmir2-m3-bag-equip-alignment | openmir2-reviewer-5 `d0b9f014`（decision） | 实际 merge `1df92efe`（引入）；账本 merge_result oid `ce18a133` | 待独立 principal 复核 | 已审已合 ⇒ 改写 | 引入 commit `156ddba1` → 卡 `openmir2-m3-bag-equip-alignment` → merge_result oid `ce18a133`（账本值；实际 merge `1df92efe`） |
| 266 | T-15 | openmir2-whitelist-stale-corrections | openmir2-reviewer-5 `d6f58a7d`（decision） tier=L | `46f1586a59566c3433049c6c952bde03712b17be`（账本值/单亲） | 待独立 principal 复核 | 已审已合 ⇒ 改写 | merge 窗口 `46f1586a` 加了本行（内容锚 + 实质变更 + 短语；账本值/单亲） |
| 267 | T-16 | openmir2-m3-bag-equip-alignment | openmir2-reviewer-5 `d0b9f014`（decision） | 实际 merge `40ac9258`（引入）；账本 merge_result oid `ce18a133` | 待独立 principal 复核 | 已审已合 ⇒ 改写 | 引入 commit `7b139963` → 卡 `openmir2-m3-bag-equip-alignment` → merge_result oid `ce18a133`（账本值；实际 merge `40ac9258`） |
| 268 | T-17 | openmir2-m3-bag-equip-alignment | openmir2-reviewer-5 `d0b9f014`（decision） | `a3ac1c09`（真实合并；引入 merge 未解析） | 待独立 principal 复核 | 已审已合 ⇒ 改写 | merge 窗口 `a3ac1c09` 加了本行（内容锚 + 实质变更 + 短语；真实合并） |
| 269 | T-18 | openmir2-m3-bag-equip-alignment | openmir2-reviewer-5 `d0b9f014`（decision） | 实际 merge `60095f65`（引入）；账本 merge_result oid `ce18a133` | 待独立 principal 复核 | 已审已合 ⇒ 改写 | 引入 commit `f6650f05` → 卡 `openmir2-m3-bag-equip-alignment` → merge_result oid `ce18a133`（账本值；实际 merge `60095f65`） |
| 270 | T-19 | openmir2-m3-bag-equip-alignment | openmir2-reviewer-5 `d0b9f014`（decision） | 实际 merge `2a2247c5`（引入）；账本 merge_result oid `ce18a133` | 待独立 principal 复核 | 已审已合 ⇒ 改写 | 引入 commit `d0f1d4b9` → 卡 `openmir2-m3-bag-equip-alignment` → merge_result oid `ce18a133`（账本值；实际 merge `2a2247c5`） |
| 271 | T-20 | openmir2-m3-bag-equip-alignment | openmir2-reviewer-5 `d0b9f014`（decision） | 实际 merge `2a2247c5`（引入）；账本 merge_result oid `ce18a133` | 待独立 principal 复核 | 已审已合 ⇒ 改写 | 引入 commit `d0f1d4b9` → 卡 `openmir2-m3-bag-equip-alignment` → merge_result oid `ce18a133`（账本值；实际 merge `2a2247c5`） |
| 272 | T-21 | openmir2-m3-bag-equip-alignment | openmir2-reviewer-5 `d0b9f014`（decision） | 实际 merge `2a2247c5`（引入）；账本 merge_result oid `ce18a133` | 待独立 principal 复核 | 已审已合 ⇒ 改写 | 引入 commit `d0f1d4b9` → 卡 `openmir2-m3-bag-equip-alignment` → merge_result oid `ce18a133`（账本值；实际 merge `2a2247c5`） |
| 274 | T-23 | openmir2-m3-bag-equip-alignment | openmir2-reviewer-5 `d0b9f014`（decision） | `fe538f09`（真实合并，引入） | 待独立 principal 复核 | 已审已合 ⇒ 改写 | 引入 commit `3434f219` → 卡 `openmir2-m3-bag-equip-alignment` → merge_result oid `fe538f09`（账本值） |
| 275 | T-24 | openmir2-smframe-minor-gaps | openmir2-reviewer-5 `638535ab`（decision） tier=R | `27d22a9f38aa31919de4e0e39e826178539b94e9`（账本值/单亲） | 待独立 principal 复核 | 已审已合 ⇒ 改写 | merge 窗口 `27d22a9f` 加了本行（内容锚 + 实质变更 + 短语；账本值/单亲） |
| 276 | T-25 | openmir2-m2-gamesvr-host | openmir2-reviewer-3 `0d0da1ed`（decision） | 实际 merge `46d2df7d`（引入）；账本 merge_result oid `73fd521b` | 待独立 principal 复核 | 已审已合 ⇒ 改写 | 引入 commit `19882a39` → 卡 `openmir2-m2-gamesvr-host` → merge_result oid `73fd521b`（账本值；实际 merge `46d2df7d`） |
| 298 | T-33 | openmir2-m3-drop-exp-align | openmir2-reviewer-5 `63c4b45f`（decision） | 实际 merge `fa82187a`（引入）；账本 merge_result oid `b7fef4a7` | 待独立 principal 复核 | 已审已合 ⇒ 改写 | 引入 commit `103995c1` → 卡 `openmir2-m3-drop-exp-align` → merge_result oid `b7fef4a7`（账本值；实际 merge `fa82187a`） |
| 299 | T-34 | openmir2-m3-drop-exp-align | openmir2-reviewer-5 `63c4b45f`（decision） | 实际 merge `fa82187a`（引入）；账本 merge_result oid `b7fef4a7` | 待独立 principal 复核 | 已审已合 ⇒ 改写 | 引入 commit `103995c1` → 卡 `openmir2-m3-drop-exp-align` → merge_result oid `b7fef4a7`（账本值；实际 merge `fa82187a`） |
| 300 | T-35 | openmir2-m3-drop-exp-align | openmir2-reviewer-5 `63c4b45f`（decision） | `d99f596e`（真实合并；引入 merge 未解析） | 待独立 principal 复核 | 已审已合 ⇒ 改写 | merge 窗口 `d99f596e` 加了本行（内容锚 + 实质变更 + 短语；真实合并） |
| 309 | T-50 | openmir2-m3-drop-exp-fixture-botsrv | openmir2-reviewer-5 `8198aec1`（decision） tier=L | `30c52f85`（真实合并；引入 merge 未解析） | 待独立 principal 复核 | 已审已合 ⇒ 改写 | 每个卡都有非作者 approve 且 merged |
| 311 | T-57 | openmir2-m3-drop-exp-wiring | openmir2-reviewer-5 `8fcd4392`（decision） tier=L | `07dd178c`（真实合并；引入 merge 未解析） | 待独立 principal 复核 | 已审已合 ⇒ 改写 | 每个卡都有非作者 approve 且 merged |
| 312 | T-58 | openmir2-m3-drop-exp-wiring | openmir2-reviewer-5 `8fcd4392`（decision） tier=L | `07dd178c`（真实合并；引入 merge 未解析） | 待独立 principal 复核 | 已审已合 ⇒ 改写 | 每个卡都有非作者 approve 且 merged |
| 327 | T-37 | openmir2-m3-quest-state | openmir2-reviewer-5 `e55c3d76`（decision） | `a443296e`（真实合并，引入） | 待独立 principal 复核 | 已审已合 ⇒ 改写 | 引入 commit `261688a6` → 卡 `openmir2-m3-quest-state` → merge_result oid `a443296e`（账本值） |
| 348 | T-41 | openmir2-m3-shop-fixture-botsrv | openmir2-reviewer-5 `33fc3c88`（decision） | 实际 merge `12a3bb5f`（引入）；账本 merge_result oid `7df340fe` | 待独立 principal 复核 | 已审已合 ⇒ 改写 | 引入 commit `c2af796c` → 卡 `openmir2-m3-shop-fixture-botsrv` → merge_result oid `7df340fe`（账本值；实际 merge `12a3bb5f`） |
| 350 | T-42 | openmir2-m3-merchant-entity-tick | openmir2-reviewer-6 `d5011c64`（decision） | `0caf5786`（真实合并，引入） | 待独立 principal 复核 | 已审已合 ⇒ 改写 | 引入 commit `097fbb66` → 卡 `openmir2-m3-merchant-entity-tick` → merge_result oid `0caf5786`（账本值） |
| 363 | T-69 | openmir2-whitelist-anchor-gate | openmir2-reviewer-5 `a79de344`（decision） tier=L | `2a04d6e29b75f2db0f7325e68d2e0a3eedf428f6`（账本值/单亲；引入 merge 未解析） | 待独立 principal 复核 | 已审已合 ⇒ 改写 | 引入 commit `2a04d6e2` → 卡 `openmir2-whitelist-anchor-gate` → merge_result oid `2a04d6e2`（账本值；引入 merge 未解析） |
| 364 | T-43 | openmir2-m3-party-trade-fixture-botsrv | openmir2-reviewer-5 `4901e222`（decision） | `12039f60`（真实合并，引入） | 待独立 principal 复核 | 已审已合 ⇒ 改写 | 引入 commit `d29d017b` → 卡 `openmir2-m3-party-trade-fixture-botsrv` → merge_result oid `12039f60`（账本值） |
| 368 | T-44 | openmir2-m3-selloff-fixture-botsrv | openmir2-reviewer-5 `7a575abe`（decision） | `78ae6319`（真实合并，引入） | 待独立 principal 复核 | 已审已合 ⇒ 改写 | 引入 commit `39f270c0` → 卡 `openmir2-m3-selloff-fixture-botsrv` → merge_result oid `78ae6319`（账本值） |
| 394 | T-46 | openmir2-m3-guild-castle-fixture-botsrv | openmir2-reviewer-5 `351f1488`（decision） | `9c776129`（真实合并，引入） | 待独立 principal 复核 | 已审已合 ⇒ 改写 | 引入 commit `37dda4ee` → 卡 `openmir2-m3-guild-castle-fixture-botsrv` → merge_result oid `9c776129`（账本值） |
| 406 | （非 T 行） | — | — | — | 待独立 principal 复核 | 非 T 表行（引用块散文）⇒ 保持原样 | 非 T 表行（引用块散文），无行级卡号 |
| 421 | T-45 | openmir2-m3-hero-pet-fixture-botsrv | openmir2-reviewer-5 `b60285d7`（decision） | 实际 merge `cf3f81c3`（引入）；账本 merge_result oid `f0b333c9` | 待独立 principal 复核 | 已审已合 ⇒ 改写 | 引入 commit `e4d5fbb7` → 卡 `openmir2-m3-hero-pet-fixture-botsrv` → merge_result oid `f0b333c9`（账本值；实际 merge `cf3f81c3`） |
| 432 | （非 T 行） | — | — | — | 待独立 principal 复核 | 非 T 表行（引用块散文）⇒ 保持原样 | 非 T 表行（引用块散文），无行级卡号 |
| 436 | T-49 | openmir2-m3-pet-slave-lifecycle | openmir2-reviewer-5 `7cecfb77`（decision） tier=R window_required | `c8a3487b`（真实合并，引入） | 待独立 principal 复核 | 已审已合 ⇒ 改写 | 引入 commit `5406d640` → 卡 `openmir2-m3-pet-slave-lifecycle` → merge_result oid `c8a3487b`（账本值） |
| 451 | T-53 | openmir2-m3-party-recall-ext | openmir2-reviewer-5 `fd2be227`（decision） tier=L | `2d2426f5`（真实合并，引入） | 待独立 principal 复核 | 已审已合 ⇒ 改写 | 引入 commit `0273b230` → 卡 `openmir2-m3-party-recall-ext` → merge_result oid `2d2426f5`（账本值） |
| 452 | T-55 | openmir2-m3-party-gotolabel-ext | openmir2-reviewer-6 `bf81bc8f`（decision） tier=L | `17389f05`（真实合并，引入） | 待独立 principal 复核 | 已审已合 ⇒ 改写 | 引入 commit `c5625c1a` → 卡 `openmir2-m3-party-gotolabel-ext` → merge_result oid `17389f05`（账本值） |
| 524 | T-86 | openmir2-run-gap-pl2 | openmir2-reviewer-5 `aca468b2`（verdict） tier=R window_required | `41eb7fa5445f3f1a3412a1600bd0290641f8d22b`（账本值/单亲；引入 merge 未解析） | 待独立 principal 复核 | 已审已合 ⇒ 改写 | 引入 commit `9fe31ac5` → 卡 `openmir2-run-gap-pl2` → merge_result oid `41eb7fa5`（账本值；引入 merge 未解析） |
| 531 | T-87 | openmir2-gate-idle-timeout | openmir2-reviewer-5 `f45272ac`（verdict） tier=R window_required | `d7dfc1055dc0f9eb05e5935a547d07dd34449fdf`（账本值/单亲；引入 merge 未解析） | 待独立 principal 复核 | 已审已合 ⇒ 改写 | 引入 commit `d7dfc105` → 卡 `openmir2-gate-idle-timeout` → merge_result oid `d7dfc105`（账本值；引入 merge 未解析） |
| 532 | T-88 | openmir2-gate-idle-timeout | openmir2-reviewer-5 `f45272ac`（verdict） tier=R window_required | `d7dfc1055dc0f9eb05e5935a547d07dd34449fdf`（账本值/单亲；引入 merge 未解析） | 待独立 principal 复核 | 已审已合 ⇒ 改写 | 引入 commit `d7dfc105` → 卡 `openmir2-gate-idle-timeout` → merge_result oid `d7dfc105`（账本值；引入 merge 未解析） |
| 533 | T-89 | openmir2-run-gap-pl4 | openmir2-reviewer-5 `6c55b991`（verdict） tier=R window_required | `bd5dc1bc3c5320731009f454a0c0d85b81268dc8`（账本值/单亲；引入 merge 未解析） | 待独立 principal 复核 | 已审已合 ⇒ 改写 | 引入 commit `7a7945cb` → 卡 `openmir2-run-gap-pl4` → merge_result oid `bd5dc1bc`（账本值；引入 merge 未解析） |
| 534 | T-90 | openmir2-run-gap-pl4 | openmir2-reviewer-5 `6c55b991`（verdict） tier=R window_required | `bd5dc1bc3c5320731009f454a0c0d85b81268dc8`（账本值/单亲；引入 merge 未解析） | 待独立 principal 复核 | 已审已合 ⇒ 改写 | 引入 commit `7a7945cb` → 卡 `openmir2-run-gap-pl4` → merge_result oid `bd5dc1bc`（账本值；引入 merge 未解析） |
| 535 | T-91 | openmir2-run-gap-pl4 | openmir2-reviewer-5 `6c55b991`（verdict） tier=R window_required | `bd5dc1bc3c5320731009f454a0c0d85b81268dc8`（账本值/单亲；引入 merge 未解析） | 待独立 principal 复核 | 已审已合 ⇒ 改写 | 引入 commit `7a7945cb` → 卡 `openmir2-run-gap-pl4` → merge_result oid `bd5dc1bc`（账本值；引入 merge 未解析） |
| 536 | T-92 | openmir2-run-gap-pl4 | openmir2-reviewer-5 `6c55b991`（verdict） tier=R window_required | `bd5dc1bc3c5320731009f454a0c0d85b81268dc8`（账本值/单亲；引入 merge 未解析） | 待独立 principal 复核 | 已审已合 ⇒ 改写 | 引入 commit `7a7945cb` → 卡 `openmir2-run-gap-pl4` → merge_result oid `bd5dc1bc`（账本值；引入 merge 未解析） |
| 537 | T-93 | openmir2-run-gap-pl4 | openmir2-reviewer-5 `6c55b991`（verdict） tier=R window_required | `bd5dc1bc3c5320731009f454a0c0d85b81268dc8`（账本值/单亲；引入 merge 未解析） | 待独立 principal 复核 | 已审已合 ⇒ 改写 | 引入 commit `7a7945cb` → 卡 `openmir2-run-gap-pl4` → merge_result oid `bd5dc1bc`（账本值；引入 merge 未解析） |
| 538 | T-94 | openmir2-run-gap-pl4 | openmir2-reviewer-5 `6c55b991`（verdict） tier=R window_required | `bd5dc1bc3c5320731009f454a0c0d85b81268dc8`（账本值/单亲；引入 merge 未解析） | 待独立 principal 复核 | 已审已合 ⇒ 改写 | 引入 commit `63f5daff` → 卡 `openmir2-run-gap-pl4` → merge_result oid `bd5dc1bc`（账本值；引入 merge 未解析） |
| 562 | T-97 | openmir2-guild-g3-persistence | openmir2-reviewer-5 `9cb8c535`（verdict） tier=L | `e9c1e865470ed9988b5fa3cedb1cbfa0307f9cde`（账本值/单亲；引入 merge 未解析） | 待独立 principal 复核 | 已审已合 ⇒ 改写 | 引入 commit `c02428b8` → 卡 `openmir2-guild-g3-persistence` → merge_result oid `e9c1e865`（账本值；引入 merge 未解析） |
| 563 | T-98 | openmir2-guild-g3-persistence | openmir2-reviewer-5 `9cb8c535`（verdict） tier=L | `e9c1e865470ed9988b5fa3cedb1cbfa0307f9cde`（账本值/单亲；引入 merge 未解析） | 待独立 principal 复核 | 已审已合 ⇒ 改写 | 引入 commit `c02428b8` → 卡 `openmir2-guild-g3-persistence` → merge_result oid `e9c1e865`（账本值；引入 merge 未解析） |
| 564 | T-99 | openmir2-guild-g3-persistence | openmir2-reviewer-5 `9cb8c535`（verdict） tier=L | `e9c1e865470ed9988b5fa3cedb1cbfa0307f9cde`（账本值/单亲；引入 merge 未解析） | 待独立 principal 复核 | 已审已合 ⇒ 改写 | 引入 commit `c02428b8` → 卡 `openmir2-guild-g3-persistence` → merge_result oid `e9c1e865`（账本值；引入 merge 未解析） |
| 565 | T-100 | openmir2-guild-g3-persistence | openmir2-reviewer-5 `9cb8c535`（verdict） tier=L | `e9c1e865470ed9988b5fa3cedb1cbfa0307f9cde`（账本值/单亲；引入 merge 未解析） | 待独立 principal 复核 | 已审已合 ⇒ 改写 | 引入 commit `c02428b8` → 卡 `openmir2-guild-g3-persistence` → merge_result oid `e9c1e865`（账本值；引入 merge 未解析） |
| 567 | T-102 | openmir2-single-binary-roles | openmir2-reviewer-5 `4ba2e1f2`（verdict） tier=R window_required | `606b32cf55950be15925e46307123ebc7629278b`（账本值/单亲） | 待独立 principal 复核 | 已审已合 ⇒ 改写 | merge 窗口 `606b32cf` 加了本行（内容锚 + 实质变更 + 短语；账本值/单亲） |
| 568 | T-103 | openmir2-guild-g3-persistence | openmir2-reviewer-5 `9cb8c535`（verdict） tier=L | `e9c1e865470ed9988b5fa3cedb1cbfa0307f9cde`（账本值/单亲；引入 merge 未解析） | 待独立 principal 复核 | 已审已合 ⇒ 改写 | 引入 commit `c02428b8` → 卡 `openmir2-guild-g3-persistence` → merge_result oid `e9c1e865`（账本值；引入 merge 未解析） |
| 584 | T-119 | openmir2-gategate-keepalive | openmir2-reviewer-5 `79709ef8`（verdict） tier=R window_required | `381e78fdb`（账本值/单亲） | 待独立 principal 复核 | 已审已合 ⇒ 改写 | 每个卡都有非作者 approve 且 merged |
| 595 | T-125 | openmir2-castle-war-area-impl | openmir2-reviewer-5 `a49984c2`（verdict） tier=R window_required | `9b1986173`（账本值/单亲） | 待独立 principal 复核 | 已审已合 ⇒ 改写 | 每个卡都有非作者 approve 且 merged |
| 596 | T-126 | openmir2-castle-war-area-impl | openmir2-reviewer-5 `a49984c2`（verdict） tier=R window_required | `9b1986173`（账本值/单亲） | 待独立 principal 复核 | 已审已合 ⇒ 改写 | 每个卡都有非作者 approve 且 merged |
| 598 | T-128 | openmir2-castle-war-area-impl | openmir2-reviewer-5 `a49984c2`（verdict） tier=R window_required | `9b1986173`（账本值/单亲） | 待独立 principal 复核 | 已审已合 ⇒ 改写 | 每个卡都有非作者 approve 且 merged |
| 599 | T-129 | openmir2-castle-war-area-impl | openmir2-reviewer-5 `a49984c2`（verdict） tier=R window_required | `9b1986173`（账本值/单亲） | 待独立 principal 复核 | 已审已合 ⇒ 改写 | 每个卡都有非作者 approve 且 merged |
| 600 | T-130 | openmir2-castle-war-area-impl | openmir2-reviewer-5 `a49984c2`（verdict） tier=R window_required | `9b1986173`（账本值/单亲） | 待独立 principal 复核 | 已审已合 ⇒ 改写 | 每个卡都有非作者 approve 且 merged |
| 601 | T-131 | openmir2-castle-war-area-impl | openmir2-reviewer-5 `a49984c2`（verdict） tier=R window_required | `9b1986173`（账本值/单亲） | 待独立 principal 复核 | 已审已合 ⇒ 改写 | 每个卡都有非作者 approve 且 merged |
| 602 | T-132 | openmir2-castle-war-area-impl | openmir2-reviewer-5 `a49984c2`（verdict） tier=R window_required | `9b1986173`（账本值/单亲） | 待独立 principal 复核 | 已审已合 ⇒ 改写 | 每个卡都有非作者 approve 且 merged |
| 604 | T-134 | openmir2-monster-ref-drains | openmir2-reviewer-5 `1a073c11`（verdict） tier=R window_required | `641d50782`（账本值/单亲） | 待独立 principal 复核 | 已审已合 ⇒ 改写 | 每个卡都有非作者 approve 且 merged |
| 605 | T-135 | openmir2-monster-ref-drains | openmir2-reviewer-5 `1a073c11`（verdict） tier=R window_required | `641d50782`（账本值/单亲） | 待独立 principal 复核 | 已审已合 ⇒ 改写 | 每个卡都有非作者 approve 且 merged |
| 617 | T-138 | openmir2-t138-request-castle-war | openmir2-reviewer-5 `30c79447`（verdict） tier=R window_required | `4d547cb74`（账本值/单亲） | 待独立 principal 复核 | 已审已合 ⇒ 改写 | 每个卡都有非作者 approve 且 merged |
| 618 | T-139 | openmir2-guild-official-npc | openmir2-reviewer-5 `12f3e5ac`（verdict） tier=R window_required | `8761bd68c`（账本值/单亲） | 待独立 principal 复核 | 已审已合 ⇒ 改写 | 每个卡都有非作者 approve 且 merged |
| 620 | T-142 | openmir2-script-playdice-openybdeal | openmir2-reviewer-5 `ae1e7697`（verdict） tier=R window_required<br>openmir2-reviewer-21 `54eeabc3`（decision） tier=R window_required<br>openmir2-reviewer-20 `e0ff0b68`（verdict） tier=R window_required | `13769d591`（账本值/单亲）<br>`bff927e1`（真实合并；引入 merge 未解析） | 待独立 principal 复核 | 已审已合 ⇒ 改写 | 每个卡都有非作者 approve 且 merged |
| 621 | T-145 | openmir2-market-def-impl | openmir2-reviewer-5 `59c396a6`（verdict） tier=R window_required | `23748419d`（账本值/单亲） | 待独立 principal 复核 | 已审已合 ⇒ 改写 | 每个卡都有非作者 approve 且 merged |
| 629 | T-141 | openmir2-t141-hit-site | openmir2-reviewer-5 `a1d20592`（verdict） tier=L | `48d8840a846ce6faa5bf3fadbdcc361b3f214d06`（账本值/单亲） | 待独立 principal 复核 | 已审已合 ⇒ 改写 | merge 窗口 `48d8840a` 加了本行（内容锚 + 实质变更 + 短语；账本值/单亲） |
| 657 | T-148 | openmir2-monster-ref-drains | openmir2-reviewer-5 `1a073c11`（verdict） tier=R window_required | `641d50782`（账本值/单亲） | 待独立 principal 复核 | 已审已合 ⇒ 改写 | 每个卡都有非作者 approve 且 merged |
| 669 | T-149 | openmir2-encoding-callsites | openmir2-reviewer-5 `b1417305`（verdict） tier=R window_required | `10707b11b8d3a29e7966bab4c4706bfcb010a9f2`（账本值/单亲；引入 merge 未解析） | 待独立 principal 复核 | 已审已合 ⇒ 改写 | 引入 commit `af80a9da` → 卡 `openmir2-encoding-callsites` → merge_result oid `10707b11`（账本值；引入 merge 未解析） |
| 670 | T-150 | openmir2-encoding-callsites | openmir2-reviewer-5 `b1417305`（verdict） tier=R window_required | `10707b11b8d3a29e7966bab4c4706bfcb010a9f2`（账本值/单亲；引入 merge 未解析） | 待独立 principal 复核 | 已审已合 ⇒ 改写 | 引入 commit `af80a9da` → 卡 `openmir2-encoding-callsites` → merge_result oid `10707b11`（账本值；引入 merge 未解析） |
| 671 | T-151 | openmir2-encoding-callsites | openmir2-reviewer-5 `b1417305`（verdict） tier=R window_required | `10707b11b8d3a29e7966bab4c4706bfcb010a9f2`（账本值/单亲；引入 merge 未解析） | 待独立 principal 复核 | 已审已合 ⇒ 改写 | 引入 commit `af80a9da` → 卡 `openmir2-encoding-callsites` → merge_result oid `10707b11`（账本值；引入 merge 未解析） |
| 677 | T-152 | openmir2-a22-castle-persistence | openmir2-reviewer-5 `03d54984`（verdict） tier=R window_required | `ca12cbf58`（账本值/单亲） | 待独立 principal 复核 | 已审已合 ⇒ 改写 | 每个卡都有非作者 approve 且 merged |
| 683 | T-153 | openmir2-a22-castle-persistence | openmir2-reviewer-5 `03d54984`（verdict） tier=R window_required | `ca12cbf58`（账本值/单亲） | 待独立 principal 复核 | 已审已合 ⇒ 改写 | 每个卡都有非作者 approve 且 merged |
| 685 | T-155 | openmir2-npc-ctor-constants<br>openmir2-guard-ctor-constants | openmir2-reviewer-5 `a7de06c2`（verdict） tier=R window_required<br>openmir2-reviewer-5 `fe666219`（verdict） tier=R window_required | `1f5132b2c`（账本值/单亲）<br>`c17b18c2e`（账本值/单亲） | 待独立 principal 复核 | 已审已合 ⇒ 改写 | 每个卡都有非作者 approve 且 merged |
| 691 | T-156 | openmir2-gm-commands-b1 | openmir2-reviewer-5 `0bbbd515`（verdict） tier=L | `7eb281fac`（账本值/单亲） | 待独立 principal 复核 | 已审已合 ⇒ 改写 | 每个卡都有非作者 approve 且 merged |

## 3. 改写清单（81 行，本轮 F2–F6 未改 whitelist）

- **本卡上一版（2026-11-01，23 T + 1 散文 = 24 行）**：行 250, 256, 309, 311, 312, 584, 595, 596, 598, 599, 600, 601, 602, 604, 605, 617, 618, 620, 621, 657, 677, 683, 685, 691。
- **F1 返工新增（57 行）**：行 135, 136, 137, 138, 146, 147, 148, 149, 246, 247, 248, 249, 264, 265, 266, 267, 268, 269, 270, 271, 272, 274, 275, 276, 298, 299, 300, 327, 348, 350, 363, 364, 368, 394, 421, 436, 451, 452, 524, 531, 532, 533, 534, 535, 536, 537, 538, 562, 563, 564, 565, 567, 568, 629, 669, 670, 671。
- 合计 **81 行**；`git diff --numstat`（相对 base）为 `81 81`。本轮 F2–F6 只改工具证据串与
  审计文档，**whitelist 零改动**（相对上一交付 head `af314ccda` 为 0/0）。
- whitelist 行内的「merge <8 位 oid>（git 溯源）」是卡线程的 **`merge_result.oid` 账本值**
  （可能=patch head，不一定是 git merge commit）；真实引入 merge 与类型见 §2 的
  「merge_result oid（类型）」列。

## 4. 保持原样的 4 行

- `:227` T-2、`:228` T-3：**已合未审**（`openmir2-s1-oracle-defects` 无线程 review；
  实际 merge `1894ccd50`，账本 `cc252146`，见 §1.4）。
- `:406`、`:432`：引用块散文（`PROSE_UNCLOSED`，可反解到 T-54 / T-49 块，按 A2 保留）。

## 5. selftest 与变异自证

`wl_status_audit.py selftest` = **24/24**（新增 9 条 git 溯源用例：内容锚正/负例、
实质变更正/负例、approve/无 approve、merge 非祖先、内容锚不符、空候选、**主路径
`_trace_intro` 合成假 Trace**）。四条变异各在临时副本上跑（原文件 sha 前后一致
`d35d3046…`）：

| 变异 | 结果 |
|---|---|
| M1 关掉内容锚（`content_matches` 恒 True） | **rc=2**，`SELFTEST FAIL: 内容锚：不同行（T 号复用）不成立` |
| M2 关掉实质变更守卫（`is_substantive` 恒 True） | **rc=2**，`SELFTEST FAIL: 实质变更：只改最后一列不成立` |
| M3 删掉溯源分支（`choose_evidence` 恒空） | **rc=2**，`SELFTEST FAIL: 溯源：内容+短语+approve ⇒ 已审已合；溯源：无 approve ⇒ 已合未审` |
| **M4 置空主路径 `_trace_intro`**（review F4） | **rc=2**，`SELFTEST FAIL: 主路径 _trace_intro 覆盖（置空该分支必红）` |

## 6. 残留物清单

- 临时文件（已删）：`.census_new.txt`/`.census_new.json`/`.census_new_err.txt`/
  `.mut_tmp.py` 等。
- 未起服务/未占端口；未跑 cargo；未改 `src/`、`sql/`、`MirClient`。
- 未改任何卡片状态（除本卡认领/返工 status）。
- 本轮只改 `tools/whitelist/wl_status_audit.py` 与 `docs/whitelist-status-audit-20261101.md`；
  whitelist 与 `tool_selftest_gate.py` 相对上一交付 head 零改动。
