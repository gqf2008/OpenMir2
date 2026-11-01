# whitelist「待独立 principal 复核」状态普查（2026-11-01；F1/F2 返工版）

- 卡：`openmir2-whitelist-status-audit`（tier=L）
- 快照：`5f5ec4243154ceff6074c07e794f3f402f4a8df0`（`walgit/master`，本卡分支 base）
- 工具：`tools/whitelist/wl_status_audit.py`（**只读**；`selftest` **23/23**，含 8 条 git 溯源纯逻辑用例）
- 复跑（在仓库根）：

  ```sh
  python -I -B tools/whitelist/wl_status_audit.py census --at 5f5ec4243154ceff6074c07e794f3f402f4a8df0
  ```

- 结果：**85 行**（84 行 `待独立 principal 复核` + 第 256 行散文 `（useitem.rs，待审）`）
  = **81 行已审已合（改写）** + **2 行已合未审（T-2/T-3，保持原样）** +
  **2 行引用块散文未闭合（保持原样）**。
  四类计数：**已审已合 81 / 已合未审 2 / 未合并 0 / 未闭合 2**。
  其中 82 个 T 行 = 80 已审已合 + 2 已合未审；2 行引用块散文未闭合；第 256 行散文 1 行已审已合。

## 1. 口径与三个坑

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

### 1.3 git 溯源（review d9fda1af F1 的返工核心）

**上一版把 61 行判成「证据不足」是错的**（F2 的失实断言已删）。根因：`resolve_cards()` 只认行内
`卡 openmir2-…` 与固定 hints；legacy 行的交付卡只出现在证据目录名、`本片 <shorthand>` 或 git
历史里。返工后工具走两条**可复核**的溯源（证据都可在线程/提交里验）：

1. **引入 commit（reviewer 口径）**：`git log -S "| T-N |"` 找把该行 0→1 加进去的 commit →
   该 commit 是某卡 patch 的 head（`refs.head`）或落在 first-parent merge 窗口 → 该卡 →
   要求：① 该 commit 自身的行与当前行**首列前 60 字符一致**（防 T 号复用/重编号）；
   ② 卡有非作者 approve；③ 合入 commit 是快照 base 的祖先；④ 合并当时该行仍含
   `待独立 principal 复核`（或该变体）。
2. **merge 窗口 diff（fallback）**：对每个 first-parent merge 窗口 `P..M` 做 `git diff`，
   找 `+| T-N |` 行；要求：① **实质变更**——去掉「编号」与「谁审的」两列后必须与 `-` 行有差异
   （专门排除 `openmir2-whitelist-card-link-debt` 给所有 legacy 行追加 `**legacy**：…` 的批改，
   那不是「本行被审」）；② 内容锚（首列前 60 字符一致）；③ 行仍含待复核短语；④ 卡有非作者
   approve。取**最早**满足的窗口（即该内容的引入窗口）。

**证据配对口径**：溯源行的 approve 取该卡首个非作者 approve 条目，merge 取引入 commit 落入的
merge 窗口；两者都属**同一卡线程**、都是 `verified` 条目。卡内若有多分片，approve 与 merge
可能不是同一次分片（表内已逐行给出两个 oid，可复核）。

**返工实测**：原 61 桶（59 个 T 行 + 2 行散文）中，**57 个 T 行**由上述溯源闭合并改写；
**2 个 T 行（T-2/T-3）**属「已合未审」（见 §1.4）；**2 行引用块散文**无行级卡号（见 §1.4）。

### 1.4 T-2/T-3 与两行引用块散文（保持原样，不是「未审」也不是「已审」）

- **T-2 / T-3**：卡 `openmir2-s1-oracle-defects` 有 `merge_result cc252146`（coordinator 批准）
  且该 merge 时两行仍写「待独立 principal 复核 + coordinator 批准」，但**该卡线程没有任何
  `kind=review` 条目** ⇒ 无独立 principal approve ⇒ 记 **已合未审**，原样保留。行文与事实一致。
- **`:406` / `:432`**：引用块里的块级注记（「本行**待独立 principal 复核**」「本块全部行
  **待独立 principal 复核**」），不是 T 表行、没有行级卡号 ⇒ 记 **未闭合**，原样保留。
  这不是「证据不足」的泛化断言，只是这两行没有可溯源的交付卡。

## 2. 逐行普查表（85 行）

| 行 | T 号 | 交付卡 | 独立 approve（字段 + 条目 oid） | merge oid | 当前文字 | 判定 | 溯源 |
|---:|---|---|---|---|---|---|---|
| 135 | T-60 | openmir2-whitelist-stale-corrections | openmir2-reviewer-5 `d6f58a7d`（decision） tier=L | `46f1586a59566c3433049c6c952bde03712b17be` | 待独立 principal 复核 | 已审已合 ⇒ 改写 | git 溯源：merge 窗口 `46f1586a` 加了本行（内容锚 + 实质变更 + 短语） |
| 136 | T-61 | openmir2-whitelist-stale-corrections | openmir2-reviewer-5 `d6f58a7d`（decision） tier=L | `46f1586a59566c3433049c6c952bde03712b17be` | 待独立 principal 复核 | 已审已合 ⇒ 改写 | git 溯源：merge 窗口 `46f1586a` 加了本行（内容锚 + 实质变更 + 短语） |
| 137 | T-62 | openmir2-whitelist-stale-corrections | openmir2-reviewer-5 `d6f58a7d`（decision） tier=L | `46f1586a59566c3433049c6c952bde03712b17be` | 待独立 principal 复核 | 已审已合 ⇒ 改写 | git 溯源：merge 窗口 `46f1586a` 加了本行（内容锚 + 实质变更 + 短语） |
| 138 | T-63 | openmir2-whitelist-stale-corrections | openmir2-reviewer-5 `d6f58a7d`（decision） tier=L | `46f1586a59566c3433049c6c952bde03712b17be` | 待独立 principal 复核 | 已审已合 ⇒ 改写 | git 溯源：merge 窗口 `46f1586a` 加了本行（内容锚 + 实质变更 + 短语） |
| 146 | T-64 | openmir2-world-tick-expansion | openmir2-reviewer-5 `b8cd2d89`（decision） tier=R window_required | `a071631482492b4e777ced21b3819fca60f2ca16` | 待独立 principal 复核 | 已审已合 ⇒ 改写 | git 溯源：merge 窗口 `a0716314` 加了本行（内容锚 + 实质变更 + 短语） |
| 147 | T-66 | openmir2-post-audit2-followups | openmir2-reviewer-5 `9c4eac6b`（decision） tier=L | `5b118f6572abf1d43a122479df7d194f42577923` | 待独立 principal 复核 | 已审已合 ⇒ 改写 | git 溯源：merge 窗口 `5b118f65` 加了本行（内容锚 + 实质变更 + 短语） |
| 148 | T-67 | openmir2-post-audit2-followups | openmir2-reviewer-5 `9c4eac6b`（decision） tier=L | `5b118f6572abf1d43a122479df7d194f42577923` | 待独立 principal 复核 | 已审已合 ⇒ 改写 | git 溯源：merge 窗口 `5b118f65` 加了本行（内容锚 + 实质变更 + 短语） |
| 149 | T-68 | openmir2-whitelist-anchor-gate | openmir2-reviewer-5 `a79de344`（decision） tier=L | `2a04d6e29b75f2db0f7325e68d2e0a3eedf428f6` | 待独立 principal 复核 | 已审已合 ⇒ 改写 | git 溯源：merge 窗口 `2a04d6e2` 加了本行（内容锚 + 实质变更 + 短语） |
| 227 | T-2 | openmir2-s1-oracle-defects | — | `cc25214649fca9806fa7e4480d896bbf31040bab` | 待独立 principal 复核 | 已合无独立 approve ⇒ 保持原样 | git 溯源：merge 窗口 `cc252146` 加了本行，但该卡无线程 review |
| 228 | T-3 | openmir2-s1-oracle-defects | — | `cc25214649fca9806fa7e4480d896bbf31040bab` | 待独立 principal 复核 | 已合无独立 approve ⇒ 保持原样 | git 溯源：merge 窗口 `cc252146` 加了本行，但该卡无线程 review |
| 246 | T-8 | openmir2-c6-loadgate-idempotent | openmir2-reviewer-3 `03238f5c`（decision） | `6f2856dad7dd8aac872a736739a9043d13785c84` | 待独立 principal 复核 | 已审已合 ⇒ 改写 | git 溯源：merge 窗口 `6f2856da` 加了本行（内容锚 + 实质变更 + 短语） |
| 247 | T-9 | openmir2-c6-loadgate-idempotent | openmir2-reviewer-3 `03238f5c`（decision） | `6f2856dad7dd8aac872a736739a9043d13785c84` | 待独立 principal 复核 | 已审已合 ⇒ 改写 | git 溯源：merge 窗口 `6f2856da` 加了本行（内容锚 + 实质变更 + 短语） |
| 248 | T-10 | openmir2-c6-loadgate-idempotent | openmir2-reviewer-3 `03238f5c`（decision） | `6f2856dad7dd8aac872a736739a9043d13785c84` | 待独立 principal 复核 | 已审已合 ⇒ 改写 | git 溯源：merge 窗口 `6f2856da` 加了本行（内容锚 + 实质变更 + 短语） |
| 249 | T-11 | openmir2-c6-loadgate-idempotent | openmir2-reviewer-3 `03238f5c`（decision） | `6f2856dad7dd8aac872a736739a9043d13785c84` | 待独立 principal 复核 | 已审已合 ⇒ 改写 | git 溯源：merge 窗口 `6f2856da` 加了本行（内容锚 + 实质变更 + 短语） |
| 250 | T-12 | openmir2-botsrv-world-evidence | openmir2-reviewer-4 `627575c0`（decision） | `7a87809503a7e93e2f6c07027249139fcce2de4e` | 待独立 principal 复核 | 已审已合 ⇒ 改写 | 每个卡都有非作者 approve 且 merged |
| 256 | PROSE-useitem | openmir2-m3-bag-equip-alignment | openmir2-reviewer-6 `f069955a`（decision） | `60095f6523f0980e02323d3adb85c060ff2efc9f` | （`useitem.rs`，待审） | 已审已合 ⇒ 改写 | 每个卡都有非作者 approve 且 merged |
| 264 | T-13 | openmir2-m3-bag-equip-alignment | openmir2-reviewer-5 `d0b9f014`（decision） | `40ac925876a8c8f5e09c49065f37ce9b27353b26` | 待独立 principal 复核 | 已审已合 ⇒ 改写 | git 溯源：merge 窗口 `40ac9258` 加了本行（内容锚 + 实质变更 + 短语） |
| 265 | T-14 | openmir2-m3-bag-equip-alignment | openmir2-reviewer-5 `d0b9f014`（decision） | `ce18a1338a8c0c834d36e7beecd98abaae6da867` | 待独立 principal 复核 | 已审已合 ⇒ 改写 | git 溯源：merge 窗口 `ce18a133` 加了本行（内容锚 + 实质变更 + 短语） |
| 266 | T-15 | openmir2-whitelist-stale-corrections | openmir2-reviewer-5 `d6f58a7d`（decision） tier=L | `46f1586a59566c3433049c6c952bde03712b17be` | 待独立 principal 复核 | 已审已合 ⇒ 改写 | git 溯源：merge 窗口 `46f1586a` 加了本行（内容锚 + 实质变更 + 短语） |
| 267 | T-16 | openmir2-m3-bag-equip-alignment | openmir2-reviewer-5 `d0b9f014`（decision） | `ce18a1338a8c0c834d36e7beecd98abaae6da867` | 待独立 principal 复核 | 已审已合 ⇒ 改写 | git 溯源：merge 窗口 `ce18a133` 加了本行（内容锚 + 实质变更 + 短语） |
| 268 | T-17 | openmir2-m3-bag-equip-alignment | openmir2-reviewer-5 `d0b9f014`（decision） | `a3ac1c092910aa829c9f6f1500db39e0c8d68ea8` | 待独立 principal 复核 | 已审已合 ⇒ 改写 | git 溯源：merge 窗口 `a3ac1c09` 加了本行（内容锚 + 实质变更 + 短语） |
| 269 | T-18 | openmir2-m3-bag-equip-alignment | openmir2-reviewer-5 `d0b9f014`（decision） | `ce18a1338a8c0c834d36e7beecd98abaae6da867` | 待独立 principal 复核 | 已审已合 ⇒ 改写 | git 溯源：merge 窗口 `ce18a133` 加了本行（内容锚 + 实质变更 + 短语） |
| 270 | T-19 | openmir2-m3-bag-equip-alignment | openmir2-reviewer-5 `d0b9f014`（decision） | `ce18a1338a8c0c834d36e7beecd98abaae6da867` | 待独立 principal 复核 | 已审已合 ⇒ 改写 | git 溯源：merge 窗口 `ce18a133` 加了本行（内容锚 + 实质变更 + 短语） |
| 271 | T-20 | openmir2-m3-bag-equip-alignment | openmir2-reviewer-5 `d0b9f014`（decision） | `ce18a1338a8c0c834d36e7beecd98abaae6da867` | 待独立 principal 复核 | 已审已合 ⇒ 改写 | git 溯源：merge 窗口 `ce18a133` 加了本行（内容锚 + 实质变更 + 短语） |
| 272 | T-21 | openmir2-m3-bag-equip-alignment | openmir2-reviewer-5 `d0b9f014`（decision） | `ce18a1338a8c0c834d36e7beecd98abaae6da867` | 待独立 principal 复核 | 已审已合 ⇒ 改写 | git 溯源：merge 窗口 `ce18a133` 加了本行（内容锚 + 实质变更 + 短语） |
| 274 | T-23 | openmir2-m3-bag-equip-alignment | openmir2-reviewer-5 `d0b9f014`（decision） | `fe538f09df7332e592a48539865a35e517f45d54` | 待独立 principal 复核 | 已审已合 ⇒ 改写 | git 溯源：merge 窗口 `fe538f09` 加了本行（内容锚 + 实质变更 + 短语） |
| 275 | T-24 | openmir2-smframe-minor-gaps | openmir2-reviewer-5 `638535ab`（decision） tier=R | `27d22a9f38aa31919de4e0e39e826178539b94e9` | 待独立 principal 复核 | 已审已合 ⇒ 改写 | git 溯源：merge 窗口 `27d22a9f` 加了本行（内容锚 + 实质变更 + 短语） |
| 276 | T-25 | openmir2-m2-gamesvr-host | openmir2-reviewer-3 `0d0da1ed`（decision） | `73fd521be7c308d4710939501ba9fa6a2dd368a3` | 待独立 principal 复核 | 已审已合 ⇒ 改写 | git 溯源：merge 窗口 `73fd521b` 加了本行（内容锚 + 实质变更 + 短语） |
| 298 | T-33 | openmir2-m3-drop-exp-align | openmir2-reviewer-5 `63c4b45f`（decision） | `b7fef4a7685f06ab38d3e8a296bd3d373c6a0c2c` | 待独立 principal 复核 | 已审已合 ⇒ 改写 | git 溯源：merge 窗口 `b7fef4a7` 加了本行（内容锚 + 实质变更 + 短语） |
| 299 | T-34 | openmir2-m3-drop-exp-align | openmir2-reviewer-5 `63c4b45f`（decision） | `b7fef4a7685f06ab38d3e8a296bd3d373c6a0c2c` | 待独立 principal 复核 | 已审已合 ⇒ 改写 | git 溯源：merge 窗口 `b7fef4a7` 加了本行（内容锚 + 实质变更 + 短语） |
| 300 | T-35 | openmir2-m3-drop-exp-align | openmir2-reviewer-5 `63c4b45f`（decision） | `d99f596e6185cec9c5981cb8dcb25b67bd5bcac4` | 待独立 principal 复核 | 已审已合 ⇒ 改写 | git 溯源：merge 窗口 `d99f596e` 加了本行（内容锚 + 实质变更 + 短语） |
| 309 | T-50 | openmir2-m3-drop-exp-fixture-botsrv | openmir2-reviewer-5 `8198aec1`（decision） tier=L | `30c52f85df2b08170537f83a2113f520e364f679` | 待独立 principal 复核 | 已审已合 ⇒ 改写 | 每个卡都有非作者 approve 且 merged |
| 311 | T-57 | openmir2-m3-drop-exp-wiring | openmir2-reviewer-5 `8fcd4392`（decision） tier=L | `07dd178c37963c7c9e7e2dd5ea1c29d581ad4b04` | 待独立 principal 复核 | 已审已合 ⇒ 改写 | 每个卡都有非作者 approve 且 merged |
| 312 | T-58 | openmir2-m3-drop-exp-wiring | openmir2-reviewer-5 `8fcd4392`（decision） tier=L | `07dd178c37963c7c9e7e2dd5ea1c29d581ad4b04` | 待独立 principal 复核 | 已审已合 ⇒ 改写 | 每个卡都有非作者 approve 且 merged |
| 327 | T-37 | openmir2-m3-quest-state | openmir2-reviewer-5 `e55c3d76`（decision） | `a443296ea58083ea7968854591cb24f03165c30a` | 待独立 principal 复核 | 已审已合 ⇒ 改写 | git 溯源：merge 窗口 `a443296e` 加了本行（内容锚 + 实质变更 + 短语） |
| 348 | T-41 | openmir2-m3-shop-fixture-botsrv | openmir2-reviewer-5 `33fc3c88`（decision） | `7df340fe786c6519f54aa9a5978a8b355037fdb6` | 待独立 principal 复核 | 已审已合 ⇒ 改写 | git 溯源：merge 窗口 `7df340fe` 加了本行（内容锚 + 实质变更 + 短语） |
| 350 | T-42 | openmir2-m3-merchant-entity-tick | openmir2-reviewer-6 `d5011c64`（decision） | `0caf5786e02afec15351ecc96cba0bbcfd49511b` | 待独立 principal 复核 | 已审已合 ⇒ 改写 | git 溯源：merge 窗口 `0caf5786` 加了本行（内容锚 + 实质变更 + 短语） |
| 363 | T-69 | openmir2-whitelist-anchor-gate | openmir2-reviewer-5 `a79de344`（decision） tier=L | `2a04d6e29b75f2db0f7325e68d2e0a3eedf428f6` | 待独立 principal 复核 | 已审已合 ⇒ 改写 | git 溯源：merge 窗口 `2a04d6e2` 加了本行（内容锚 + 实质变更 + 短语） |
| 364 | T-43 | openmir2-m3-party-trade-fixture-botsrv | openmir2-reviewer-5 `4901e222`（decision） | `12039f6063160ffe00d85cf185ba5da3f83f7e34` | 待独立 principal 复核 | 已审已合 ⇒ 改写 | git 溯源：merge 窗口 `12039f60` 加了本行（内容锚 + 实质变更 + 短语） |
| 368 | T-44 | openmir2-m3-selloff-fixture-botsrv | openmir2-reviewer-5 `7a575abe`（decision） | `78ae6319060b121a30ba21ce9cf8c1786cc8d574` | 待独立 principal 复核 | 已审已合 ⇒ 改写 | git 溯源：merge 窗口 `78ae6319` 加了本行（内容锚 + 实质变更 + 短语） |
| 394 | T-46 | openmir2-m3-guild-castle-fixture-botsrv | openmir2-reviewer-5 `351f1488`（decision） | `9c776129c30280ddc692d88d7507192dd94f20a0` | 待独立 principal 复核 | 已审已合 ⇒ 改写 | git 溯源：merge 窗口 `9c776129` 加了本行（内容锚 + 实质变更 + 短语） |
| 406 | （非 T 行） | — | — | — | 待独立 principal 复核 | 非 T 表行（引用块散文）⇒ 保持原样 | 非 T 表行（引用块散文），无行级卡号 |
| 421 | T-45 | openmir2-m3-hero-pet-fixture-botsrv | openmir2-reviewer-5 `b60285d7`（decision） | `f0b333c96cedc7b733e0a03fca126f14ac89007c` | 待独立 principal 复核 | 已审已合 ⇒ 改写 | git 溯源：merge 窗口 `f0b333c9` 加了本行（内容锚 + 实质变更 + 短语） |
| 432 | （非 T 行） | — | — | — | 待独立 principal 复核 | 非 T 表行（引用块散文）⇒ 保持原样 | 非 T 表行（引用块散文），无行级卡号 |
| 436 | T-49 | openmir2-m3-pet-slave-lifecycle | openmir2-reviewer-5 `7cecfb77`（decision） tier=R window_required | `c8a3487b1f6a117065b0105836105b7ed3c0529b` | 待独立 principal 复核 | 已审已合 ⇒ 改写 | git 溯源：merge 窗口 `c8a3487b` 加了本行（内容锚 + 实质变更 + 短语） |
| 451 | T-53 | openmir2-m3-party-recall-ext | openmir2-reviewer-5 `fd2be227`（decision） tier=L | `2d2426f5be60d715c68c4df8ac1596295fccd9ad` | 待独立 principal 复核 | 已审已合 ⇒ 改写 | git 溯源：merge 窗口 `2d2426f5` 加了本行（内容锚 + 实质变更 + 短语） |
| 452 | T-55 | openmir2-m3-party-gotolabel-ext | openmir2-reviewer-6 `bf81bc8f`（decision） tier=L | `17389f05f6cc9862a86aa3443fac10654f0d26ce` | 待独立 principal 复核 | 已审已合 ⇒ 改写 | git 溯源：merge 窗口 `17389f05` 加了本行（内容锚 + 实质变更 + 短语） |
| 524 | T-86 | openmir2-run-gap-pl2 | openmir2-reviewer-5 `aca468b2`（verdict） tier=R window_required | `41eb7fa5445f3f1a3412a1600bd0290641f8d22b` | 待独立 principal 复核 | 已审已合 ⇒ 改写 | git 溯源：merge 窗口 `41eb7fa5` 加了本行（内容锚 + 实质变更 + 短语） |
| 531 | T-87 | openmir2-gate-idle-timeout | openmir2-reviewer-5 `f45272ac`（verdict） tier=R window_required | `d7dfc1055dc0f9eb05e5935a547d07dd34449fdf` | 待独立 principal 复核 | 已审已合 ⇒ 改写 | git 溯源：merge 窗口 `d7dfc105` 加了本行（内容锚 + 实质变更 + 短语） |
| 532 | T-88 | openmir2-gate-idle-timeout | openmir2-reviewer-5 `f45272ac`（verdict） tier=R window_required | `d7dfc1055dc0f9eb05e5935a547d07dd34449fdf` | 待独立 principal 复核 | 已审已合 ⇒ 改写 | git 溯源：merge 窗口 `d7dfc105` 加了本行（内容锚 + 实质变更 + 短语） |
| 533 | T-89 | openmir2-run-gap-pl4 | openmir2-reviewer-5 `6c55b991`（verdict） tier=R window_required | `bd5dc1bc3c5320731009f454a0c0d85b81268dc8` | 待独立 principal 复核 | 已审已合 ⇒ 改写 | git 溯源：merge 窗口 `bd5dc1bc` 加了本行（内容锚 + 实质变更 + 短语） |
| 534 | T-90 | openmir2-run-gap-pl4 | openmir2-reviewer-5 `6c55b991`（verdict） tier=R window_required | `bd5dc1bc3c5320731009f454a0c0d85b81268dc8` | 待独立 principal 复核 | 已审已合 ⇒ 改写 | git 溯源：merge 窗口 `bd5dc1bc` 加了本行（内容锚 + 实质变更 + 短语） |
| 535 | T-91 | openmir2-run-gap-pl4 | openmir2-reviewer-5 `6c55b991`（verdict） tier=R window_required | `bd5dc1bc3c5320731009f454a0c0d85b81268dc8` | 待独立 principal 复核 | 已审已合 ⇒ 改写 | git 溯源：merge 窗口 `bd5dc1bc` 加了本行（内容锚 + 实质变更 + 短语） |
| 536 | T-92 | openmir2-run-gap-pl4 | openmir2-reviewer-5 `6c55b991`（verdict） tier=R window_required | `bd5dc1bc3c5320731009f454a0c0d85b81268dc8` | 待独立 principal 复核 | 已审已合 ⇒ 改写 | git 溯源：merge 窗口 `bd5dc1bc` 加了本行（内容锚 + 实质变更 + 短语） |
| 537 | T-93 | openmir2-run-gap-pl4 | openmir2-reviewer-5 `6c55b991`（verdict） tier=R window_required | `bd5dc1bc3c5320731009f454a0c0d85b81268dc8` | 待独立 principal 复核 | 已审已合 ⇒ 改写 | git 溯源：merge 窗口 `bd5dc1bc` 加了本行（内容锚 + 实质变更 + 短语） |
| 538 | T-94 | openmir2-run-gap-pl4 | openmir2-reviewer-5 `6c55b991`（verdict） tier=R window_required | `bd5dc1bc3c5320731009f454a0c0d85b81268dc8` | 待独立 principal 复核 | 已审已合 ⇒ 改写 | git 溯源：merge 窗口 `bd5dc1bc` 加了本行（内容锚 + 实质变更 + 短语） |
| 562 | T-97 | openmir2-guild-g3-persistence | openmir2-reviewer-5 `9cb8c535`（verdict） tier=L | `e9c1e865470ed9988b5fa3cedb1cbfa0307f9cde` | 待独立 principal 复核 | 已审已合 ⇒ 改写 | git 溯源：merge 窗口 `e9c1e865` 加了本行（内容锚 + 实质变更 + 短语） |
| 563 | T-98 | openmir2-guild-g3-persistence | openmir2-reviewer-5 `9cb8c535`（verdict） tier=L | `e9c1e865470ed9988b5fa3cedb1cbfa0307f9cde` | 待独立 principal 复核 | 已审已合 ⇒ 改写 | git 溯源：merge 窗口 `e9c1e865` 加了本行（内容锚 + 实质变更 + 短语） |
| 564 | T-99 | openmir2-guild-g3-persistence | openmir2-reviewer-5 `9cb8c535`（verdict） tier=L | `e9c1e865470ed9988b5fa3cedb1cbfa0307f9cde` | 待独立 principal 复核 | 已审已合 ⇒ 改写 | git 溯源：merge 窗口 `e9c1e865` 加了本行（内容锚 + 实质变更 + 短语） |
| 565 | T-100 | openmir2-guild-g3-persistence | openmir2-reviewer-5 `9cb8c535`（verdict） tier=L | `e9c1e865470ed9988b5fa3cedb1cbfa0307f9cde` | 待独立 principal 复核 | 已审已合 ⇒ 改写 | git 溯源：merge 窗口 `e9c1e865` 加了本行（内容锚 + 实质变更 + 短语） |
| 567 | T-102 | openmir2-single-binary-roles | openmir2-reviewer-5 `4ba2e1f2`（verdict） tier=R window_required | `606b32cf55950be15925e46307123ebc7629278b` | 待独立 principal 复核 | 已审已合 ⇒ 改写 | git 溯源：merge 窗口 `606b32cf` 加了本行（内容锚 + 实质变更 + 短语） |
| 568 | T-103 | openmir2-guild-g3-persistence | openmir2-reviewer-5 `9cb8c535`（verdict） tier=L | `e9c1e865470ed9988b5fa3cedb1cbfa0307f9cde` | 待独立 principal 复核 | 已审已合 ⇒ 改写 | git 溯源：merge 窗口 `e9c1e865` 加了本行（内容锚 + 实质变更 + 短语） |
| 584 | T-119 | openmir2-gategate-keepalive | openmir2-reviewer-5 `79709ef8`（verdict） tier=R window_required | `381e78fdb` | 待独立 principal 复核 | 已审已合 ⇒ 改写 | 每个卡都有非作者 approve 且 merged |
| 595 | T-125 | openmir2-castle-war-area-impl | openmir2-reviewer-5 `a49984c2`（verdict） tier=R window_required | `9b1986173` | 待独立 principal 复核 | 已审已合 ⇒ 改写 | 每个卡都有非作者 approve 且 merged |
| 596 | T-126 | openmir2-castle-war-area-impl | openmir2-reviewer-5 `a49984c2`（verdict） tier=R window_required | `9b1986173` | 待独立 principal 复核 | 已审已合 ⇒ 改写 | 每个卡都有非作者 approve 且 merged |
| 598 | T-128 | openmir2-castle-war-area-impl | openmir2-reviewer-5 `a49984c2`（verdict） tier=R window_required | `9b1986173` | 待独立 principal 复核 | 已审已合 ⇒ 改写 | 每个卡都有非作者 approve 且 merged |
| 599 | T-129 | openmir2-castle-war-area-impl | openmir2-reviewer-5 `a49984c2`（verdict） tier=R window_required | `9b1986173` | 待独立 principal 复核 | 已审已合 ⇒ 改写 | 每个卡都有非作者 approve 且 merged |
| 600 | T-130 | openmir2-castle-war-area-impl | openmir2-reviewer-5 `a49984c2`（verdict） tier=R window_required | `9b1986173` | 待独立 principal 复核 | 已审已合 ⇒ 改写 | 每个卡都有非作者 approve 且 merged |
| 601 | T-131 | openmir2-castle-war-area-impl | openmir2-reviewer-5 `a49984c2`（verdict） tier=R window_required | `9b1986173` | 待独立 principal 复核 | 已审已合 ⇒ 改写 | 每个卡都有非作者 approve 且 merged |
| 602 | T-132 | openmir2-castle-war-area-impl | openmir2-reviewer-5 `a49984c2`（verdict） tier=R window_required | `9b1986173` | 待独立 principal 复核 | 已审已合 ⇒ 改写 | 每个卡都有非作者 approve 且 merged |
| 604 | T-134 | openmir2-monster-ref-drains | openmir2-reviewer-5 `1a073c11`（verdict） tier=R window_required | `641d50782` | 待独立 principal 复核 | 已审已合 ⇒ 改写 | 每个卡都有非作者 approve 且 merged |
| 605 | T-135 | openmir2-monster-ref-drains | openmir2-reviewer-5 `1a073c11`（verdict） tier=R window_required | `641d50782` | 待独立 principal 复核 | 已审已合 ⇒ 改写 | 每个卡都有非作者 approve 且 merged |
| 617 | T-138 | openmir2-t138-request-castle-war | openmir2-reviewer-5 `30c79447`（verdict） tier=R window_required | `4d547cb74` | 待独立 principal 复核 | 已审已合 ⇒ 改写 | 每个卡都有非作者 approve 且 merged |
| 618 | T-139 | openmir2-guild-official-npc | openmir2-reviewer-5 `12f3e5ac`（verdict） tier=R window_required | `8761bd68c` | 待独立 principal 复核 | 已审已合 ⇒ 改写 | 每个卡都有非作者 approve 且 merged |
| 620 | T-142 | openmir2-script-playdice-openybdeal | openmir2-reviewer-5 `ae1e7697`（verdict） tier=R window_required<br>openmir2-reviewer-21 `54eeabc3`（decision） tier=R window_required<br>openmir2-reviewer-20 `e0ff0b68`（verdict） tier=R window_required | `13769d591`<br>`bff927e11` | 待独立 principal 复核 | 已审已合 ⇒ 改写 | 每个卡都有非作者 approve 且 merged |
| 621 | T-145 | openmir2-market-def-impl | openmir2-reviewer-5 `59c396a6`（verdict） tier=R window_required | `23748419d` | 待独立 principal 复核 | 已审已合 ⇒ 改写 | 每个卡都有非作者 approve 且 merged |
| 629 | T-141 | openmir2-t141-hit-site | openmir2-reviewer-5 `a1d20592`（verdict） tier=L | `48d8840a846ce6faa5bf3fadbdcc361b3f214d06` | 待独立 principal 复核 | 已审已合 ⇒ 改写 | git 溯源：merge 窗口 `48d8840a` 加了本行（内容锚 + 实质变更 + 短语） |
| 657 | T-148 | openmir2-monster-ref-drains | openmir2-reviewer-5 `1a073c11`（verdict） tier=R window_required | `641d50782` | 待独立 principal 复核 | 已审已合 ⇒ 改写 | 每个卡都有非作者 approve 且 merged |
| 669 | T-149 | openmir2-encoding-callsites | openmir2-reviewer-5 `b1417305`（verdict） tier=R window_required | `10707b11b8d3a29e7966bab4c4706bfcb010a9f2` | 待独立 principal 复核 | 已审已合 ⇒ 改写 | git 溯源：merge 窗口 `10707b11` 加了本行（内容锚 + 实质变更 + 短语） |
| 670 | T-150 | openmir2-encoding-callsites | openmir2-reviewer-5 `b1417305`（verdict） tier=R window_required | `10707b11b8d3a29e7966bab4c4706bfcb010a9f2` | 待独立 principal 复核 | 已审已合 ⇒ 改写 | git 溯源：merge 窗口 `10707b11` 加了本行（内容锚 + 实质变更 + 短语） |
| 671 | T-151 | openmir2-encoding-callsites | openmir2-reviewer-5 `b1417305`（verdict） tier=R window_required | `10707b11b8d3a29e7966bab4c4706bfcb010a9f2` | 待独立 principal 复核 | 已审已合 ⇒ 改写 | git 溯源：merge 窗口 `10707b11` 加了本行（内容锚 + 实质变更 + 短语） |
| 677 | T-152 | openmir2-a22-castle-persistence | openmir2-reviewer-5 `03d54984`（verdict） tier=R window_required | `ca12cbf58` | 待独立 principal 复核 | 已审已合 ⇒ 改写 | 每个卡都有非作者 approve 且 merged |
| 683 | T-153 | openmir2-a22-castle-persistence | openmir2-reviewer-5 `03d54984`（verdict） tier=R window_required | `ca12cbf58` | 待独立 principal 复核 | 已审已合 ⇒ 改写 | 每个卡都有非作者 approve 且 merged |
| 685 | T-155 | openmir2-npc-ctor-constants<br>openmir2-guard-ctor-constants | openmir2-reviewer-5 `a7de06c2`（verdict） tier=R window_required<br>openmir2-reviewer-5 `fe666219`（verdict） tier=R window_required | `1f5132b2c`<br>`c17b18c2e` | 待独立 principal 复核 | 已审已合 ⇒ 改写 | 每个卡都有非作者 approve 且 merged |
| 691 | T-156 | openmir2-gm-commands-b1 | openmir2-reviewer-5 `0bbbd515`（verdict） tier=L | `7eb281fac` | 待独立 principal 复核 | 已审已合 ⇒ 改写 | 每个卡都有非作者 approve 且 merged |

## 3. 改写清单（81 行）

- **本卡上一版（2026-11-01，23 T + 1 散文 = 24 行）**：行 250, 256, 309, 311, 312, 584, 595, 596, 598, 599, 600, 601, 602, 604, 605, 617, 618, 620, 621, 657, 677, 683, 685, 691。
- **本次 F1 返工新增（57 行）**：行 135, 136, 137, 138, 146, 147, 148, 149, 246, 247, 248, 249, 264, 265, 266, 267, 268, 269, 270, 271, 272, 274, 275, 276, 298, 299, 300, 327, 348, 350, 363, 364, 368, 394, 421, 436, 451, 452, 524, 531, 532, 533, 534, 535, 536, 537, 538, 562, 563, 564, 565, 567, 568, 629, 669, 670, 671。
- 合计 **81 行**改写；`git diff --numstat`（相对 base）为 `81 81`（上一版 24 + 本次 57）。
  每行只替换「谁审的」列的状态短语；抽检 57 行做「还原旧文本」逐行比对与 `path:line`
  token 多重集比对，**零差异**。

## 4. 保持原样的 4 行

- `:227` T-2、`:228` T-3：**已合未审**（`openmir2-s1-oracle-defects` 无线程 review，见 §1.4）。
- `:406`、`:432`：引用块散文，无行级卡号（见 §1.4）。

## 5. selftest 与变异自证

`wl_status_audit.py selftest` = **23/23**（新增 8 条 git 溯源纯逻辑：内容锚正/负例、
实质变更正/负例、approve/无 approve、merge 非祖先、内容锚不符、空候选）。三条变异各在临时副本
上跑（原文件 sha 前后一致 `90c2dfec…`）：

| 变异 | 结果 |
|---|---|
| M1 关掉内容锚（`content_matches` 恒 True） | **rc=2**，`SELFTEST FAIL: 内容锚：不同行（T 号复用）不成立` |
| M2 关掉实质变更守卫（`is_substantive` 恒 True） | **rc=2**，`SELFTEST FAIL: 实质变更：只改最后一列不成立` |
| M3 删掉溯源分支（`choose_evidence` 恒空） | **rc=2**，`SELFTEST FAIL: 溯源：内容+短语+approve ⇒ 已审已合, 溯源：无 approve ⇒ 已合未审` |

（M1/M2/M3 各自删掉的正是「把漏判修回来」的三条分支；任一被删，对应用例立刻红。）

## 6. 残留物清单

- 临时文件（已删）：`.census_new.txt`/`.census_new.json`/`.census_new_err.txt`/
  `.apply57.py`/`.mut_tmp.py` 以及全部 `.trace*.txt`、`.audit*.txt`。
- 未起服务/未占端口；未跑 cargo；未改 `src/`、`sql/`、`MirClient`。
- 未改任何卡片状态（除本卡认领/返工 status）。
- 已知边界：溯源行的 approve 与 merge 可能来自同卡的不同分片（见 §1.3 的配对口径）；
  T-2/T-3 若要变「已审已合」，须先补该卡的独立 review 条目（另卡）。
