# whitelist「待独立 principal 复核」状态普查（2026-11-01）

- 卡：`openmir2-whitelist-status-audit`（tier=L）
- 快照：`5f5ec4243154ceff6074c07e794f3f402f4a8df0`（`walgit/master`，本卡分支 base）
- 工具：`tools/whitelist/wl_status_audit.py`（**只读**：只产出普查表，不改 whitelist / 不改卡片状态；
  `selftest` 14/14）
- 复跑（在仓库根）：

  ```sh
  python -I -B tools/whitelist/wl_status_audit.py census --at 5f5ec4243154ceff6074c07e794f3f402f4a8df0
  ```

- 结果：**85 行**（84 行 `待独立 principal 复核` + 第 256 行散文 `（useitem.rs，待审）`）
  = **24 行已审已合（改写）** + **61 行证据不足（保持原样）**。
  其中带该字样的 84 行 = **82 个 T 行 + 2 行引用块散文**；82 个 T 行 = 23 已审已合 +
  59 证据不足（2 行散文均证据不足）；另第 256 行散文 1 行已审已合。
  分布按四类记：**已审已合 24 / 已合未审 0 / 未合并 0 / 证据不足 61**。

> 快照口径：本表针对**改写前**的 base（`5f5ec4243`）。改写后工作树只剩 61 行该字样
> （`census` 不带 `--at` 时实测 `合计 61 行：NO_CARD=61`），两份读数不矛盾。

## 1. 口径与三个已知坑

### 1.1 approve 有两种字段名（必须都认）

早期 review 条目用 `body.verdict`，近期用 `body.decision`。只认一个会把「有审的卡」误判成
「没审」。本表两种都认，并逐行标注用的是哪个字段：

- `decision`：T-12（reviewer-4 `627575c0`）、T-50（reviewer-5 `8198aec1`）、
  T-57/T-58（reviewer-5 `8fcd4392`）、prose useitem（reviewer-6 `f069955a`）、
  T-142 的近期复核（reviewer-21 `54eeabc3`）。
- `verdict`：T-119、T-125..T-132、T-134/T-135/T-148、T-138、T-139、T-142、T-145、
  T-152/T-153、T-155、T-156 的 reviewer-5 approve。

「独立」的判据：approve 的 actor **不在**该卡 `kind=patch` 的 actor 集合里。上表所有 approve
均为 reviewer（作者为 `openmir2-worker-*` 或 `openmir2-orchestrator`），无自审。

### 1.2 行内引用的卡不一定是**交付卡**

这是本卡最容易踩的坑：`walgit collab thread` 里，行内出现的 `openmir2-…` 可能是**承接卡**
（未来才做），不是本行的交付卡。按行内卡号猜会把证据算到别人头上：

| 行 | 行内引用 | 实际交付卡 | 依据 |
|---|---|---|---|
| T-50 | `openmir2-m3-rng-scene-driver`（承接卡） | `openmir2-m3-drop-exp-fixture-botsrv` | 本行证据目录 `m3/drop-exp/evidence-drop-exp-fixture-botsrv-20261014/**`；该卡 patch `2c1d8d24` 自述「whitelist.md 的 M3④ 段新增 T-50」 |
| T-58 | `openmir2-m3-quest-state-dump`（**该卡号无对应线程**，是承接卡） | `openmir2-m3-drop-exp-wiring` | 该卡 patch `220019d7` 自述「whitelist T-57/T-58（12 文件）」 |
| T-138/T-139 | 行内只写 `audit #3 / 车道 guildofficial` | T-138=`openmir2-t138-request-castle-war`；T-139=`openmir2-guild-official-npc` | T-138 卡标题即「T-138 GuildOfficial @requestcastlewarnowN」；T-139 卡标题含「（audit #3）」 |
| T-142/T-145 | 行内只写车道名 `scriptpd`/`marketdef` | `openmir2-script-playdice-openybdeal` / `openmir2-market-def-impl` | patch `b04975f6` 自述「登记 T-142」；patch `66a8a4c3` 自述「T-145 登记」 |
| T-155 | 行内两个卡 `npc-ctor`（主）+ `guard-ctor`（分项） | `openmir2-npc-ctor-constants` + `openmir2-guard-ctor-constants` | 两卡 patch 均登记 T-155 |

**T-58 的额外事实**：行内引用的 `openmir2-m3-quest-state-dump` 在 `refs/collab/inbox/*` 里
**没有任何线程**（85 个目标行里唯一一个卡号不存在的情况）。若只按行内卡号查，会得出
「卡不存在 ⇒ 证据不足」；实际交付卡是 `openmir2-m3-drop-exp-wiring`，其 patch 明确登记
T-57/T-58，且已有 reviewer-5 `8fcd4392` approve + merge `07dd178c`。

### 1.3 无卡 / 无线程 = 证据不足，不许美化

61 行属于「行内无 `openmir2-…` 卡号、看板无对应工作项」的旧轮次登记（`**legacy**` 后缀）。
前卡 `openmir2-whitelist-card-link-debt`（approve reviewer-21 `6a705a37`）已明确裁决：
**不许拿一个卡号套多行**，T-50/T-58 两处「不补链」的克制是对的。本卡沿用该裁决：
这些行无法从可复核的行级证据判定审查/合并状态 ⇒ **原样保留**，本表标「证据不足」。

> 注意：这不等于「它们一定没被审」。有些旧行可能确实被后开的卡间接覆盖，但**没有行级证据**
> 就不许写成「已审已合」——把「未审」写成「已审」比留着旧措辞更坏（卡面 A2）。

## 2. 逐行普查表（85 行）

| 行 | T 号 | 交付卡 | 独立 approve（字段 + 条目 oid） | merge oid | 当前文字 | 判定 |
|---:|---|---|---|---|---|---|
| 135 | T-60 | — | — | — | 待独立 principal 复核 | 无交付卡号 ⇒ 保持原样（证据不足） |
| 136 | T-61 | — | — | — | 待独立 principal 复核 | 无交付卡号 ⇒ 保持原样（证据不足） |
| 137 | T-62 | — | — | — | 待独立 principal 复核 | 无交付卡号 ⇒ 保持原样（证据不足） |
| 138 | T-63 | — | — | — | 待独立 principal 复核 | 无交付卡号 ⇒ 保持原样（证据不足） |
| 146 | T-64 | — | — | — | 待独立 principal 复核 | 无交付卡号 ⇒ 保持原样（证据不足） |
| 147 | T-66 | — | — | — | 待独立 principal 复核 | 无交付卡号 ⇒ 保持原样（证据不足） |
| 148 | T-67 | — | — | — | 待独立 principal 复核 | 无交付卡号 ⇒ 保持原样（证据不足） |
| 149 | T-68 | — | — | — | 待独立 principal 复核 | 无交付卡号 ⇒ 保持原样（证据不足） |
| 227 | T-2 | — | — | — | 待独立 principal 复核 | 无交付卡号 ⇒ 保持原样（证据不足） |
| 228 | T-3 | — | — | — | 待独立 principal 复核 | 无交付卡号 ⇒ 保持原样（证据不足） |
| 246 | T-8 | — | — | — | 待独立 principal 复核 | 无交付卡号 ⇒ 保持原样（证据不足） |
| 247 | T-9 | — | — | — | 待独立 principal 复核 | 无交付卡号 ⇒ 保持原样（证据不足） |
| 248 | T-10 | — | — | — | 待独立 principal 复核 | 无交付卡号 ⇒ 保持原样（证据不足） |
| 249 | T-11 | — | — | — | 待独立 principal 复核 | 无交付卡号 ⇒ 保持原样（证据不足） |
| 250 | T-12 | `openmir2-botsrv-world-evidence` | reviewer-4 `627575c0`（decision） | `7a87809503a7e93e2f6c07027249139fcce2de4e` | 待独立 principal 复核 | 已审已合 ⇒ 改写 |
| 256 | PROSE-useitem | `openmir2-m3-bag-equip-alignment` | reviewer-6 `f069955a`（decision） | `60095f6523f0980e02323d3adb85c060ff2efc9f` | （`useitem.rs`，待审） | 已审已合 ⇒ 改写 |
| 264 | T-13 | — | — | — | 待独立 principal 复核 | 无交付卡号 ⇒ 保持原样（证据不足） |
| 265 | T-14 | — | — | — | 待独立 principal 复核 | 无交付卡号 ⇒ 保持原样（证据不足） |
| 266 | T-15 | — | — | — | 待独立 principal 复核 | 无交付卡号 ⇒ 保持原样（证据不足） |
| 267 | T-16 | — | — | — | 待独立 principal 复核 | 无交付卡号 ⇒ 保持原样（证据不足） |
| 268 | T-17 | — | — | — | 待独立 principal 复核 | 无交付卡号 ⇒ 保持原样（证据不足） |
| 269 | T-18 | — | — | — | 待独立 principal 复核 | 无交付卡号 ⇒ 保持原样（证据不足） |
| 270 | T-19 | — | — | — | 待独立 principal 复核 | 无交付卡号 ⇒ 保持原样（证据不足） |
| 271 | T-20 | — | — | — | 待独立 principal 复核 | 无交付卡号 ⇒ 保持原样（证据不足） |
| 272 | T-21 | — | — | — | 待独立 principal 复核 | 无交付卡号 ⇒ 保持原样（证据不足） |
| 274 | T-23 | — | — | — | 待独立 principal 复核 | 无交付卡号 ⇒ 保持原样（证据不足） |
| 275 | T-24 | — | — | — | 待独立 principal 复核 | 无交付卡号 ⇒ 保持原样（证据不足） |
| 276 | T-25 | — | — | — | 待独立 principal 复核 | 无交付卡号 ⇒ 保持原样（证据不足） |
| 298 | T-33 | — | — | — | 待独立 principal 复核 | 无交付卡号 ⇒ 保持原样（证据不足） |
| 299 | T-34 | — | — | — | 待独立 principal 复核 | 无交付卡号 ⇒ 保持原样（证据不足） |
| 300 | T-35 | — | — | — | 待独立 principal 复核 | 无交付卡号 ⇒ 保持原样（证据不足） |
| 309 | T-50 | `openmir2-m3-drop-exp-fixture-botsrv` | reviewer-5 `8198aec1`（decision） | `30c52f85df2b08170537f83a2113f520e364f679` | 待独立 principal 复核 | 已审已合 ⇒ 改写 |
| 311 | T-57 | `openmir2-m3-drop-exp-wiring` | reviewer-5 `8fcd4392`（decision） | `07dd178c37963c7c9e7e2dd5ea1c29d581ad4b04` | 待独立 principal 复核 | 已审已合 ⇒ 改写 |
| 312 | T-58 | `openmir2-m3-drop-exp-wiring` | reviewer-5 `8fcd4392`（decision） | `07dd178c37963c7c9e7e2dd5ea1c29d581ad4b04` | 待独立 principal 复核 | 已审已合 ⇒ 改写 |
| 327 | T-37 | — | — | — | 待独立 principal 复核 | 无交付卡号 ⇒ 保持原样（证据不足） |
| 348 | T-41 | — | — | — | 待独立 principal 复核 | 无交付卡号 ⇒ 保持原样（证据不足） |
| 350 | T-42 | — | — | — | 待独立 principal 复核 | 无交付卡号 ⇒ 保持原样（证据不足） |
| 363 | T-69 | — | — | — | 待独立 principal 复核 | 无交付卡号 ⇒ 保持原样（证据不足） |
| 364 | T-43 | — | — | — | 待独立 principal 复核 | 无交付卡号 ⇒ 保持原样（证据不足） |
| 368 | T-44 | — | — | — | 待独立 principal 复核 | 无交付卡号 ⇒ 保持原样（证据不足） |
| 394 | T-46 | — | — | — | 待独立 principal 复核 | 无交付卡号 ⇒ 保持原样（证据不足） |
| 406 | （非 T 行） | — | — | — | 待独立 principal 复核 | 无交付卡号 ⇒ 保持原样（证据不足） |
| 421 | T-45 | — | — | — | 待独立 principal 复核 | 无交付卡号 ⇒ 保持原样（证据不足） |
| 432 | （非 T 行） | — | — | — | 待独立 principal 复核 | 无交付卡号 ⇒ 保持原样（证据不足） |
| 436 | T-49 | — | — | — | 待独立 principal 复核 | 无交付卡号 ⇒ 保持原样（证据不足） |
| 451 | T-53 | — | — | — | 待独立 principal 复核 | 无交付卡号 ⇒ 保持原样（证据不足） |
| 452 | T-55 | — | — | — | 待独立 principal 复核 | 无交付卡号 ⇒ 保持原样（证据不足） |
| 524 | T-86 | — | — | — | 待独立 principal 复核 | 无交付卡号 ⇒ 保持原样（证据不足） |
| 531 | T-87 | — | — | — | 待独立 principal 复核 | 无交付卡号 ⇒ 保持原样（证据不足） |
| 532 | T-88 | — | — | — | 待独立 principal 复核 | 无交付卡号 ⇒ 保持原样（证据不足） |
| 533 | T-89 | — | — | — | 待独立 principal 复核 | 无交付卡号 ⇒ 保持原样（证据不足） |
| 534 | T-90 | — | — | — | 待独立 principal 复核 | 无交付卡号 ⇒ 保持原样（证据不足） |
| 535 | T-91 | — | — | — | 待独立 principal 复核 | 无交付卡号 ⇒ 保持原样（证据不足） |
| 536 | T-92 | — | — | — | 待独立 principal 复核 | 无交付卡号 ⇒ 保持原样（证据不足） |
| 537 | T-93 | — | — | — | 待独立 principal 复核 | 无交付卡号 ⇒ 保持原样（证据不足） |
| 538 | T-94 | — | — | — | 待独立 principal 复核 | 无交付卡号 ⇒ 保持原样（证据不足） |
| 562 | T-97 | — | — | — | 待独立 principal 复核 | 无交付卡号 ⇒ 保持原样（证据不足） |
| 563 | T-98 | — | — | — | 待独立 principal 复核 | 无交付卡号 ⇒ 保持原样（证据不足） |
| 564 | T-99 | — | — | — | 待独立 principal 复核 | 无交付卡号 ⇒ 保持原样（证据不足） |
| 565 | T-100 | — | — | — | 待独立 principal 复核 | 无交付卡号 ⇒ 保持原样（证据不足） |
| 567 | T-102 | — | — | — | 待独立 principal 复核 | 无交付卡号 ⇒ 保持原样（证据不足） |
| 568 | T-103 | — | — | — | 待独立 principal 复核 | 无交付卡号 ⇒ 保持原样（证据不足） |
| 584 | T-119 | `openmir2-gategate-keepalive` | reviewer-5 `79709ef8`（verdict） | `381e78fdb` | 待独立 principal 复核 | 已审已合 ⇒ 改写 |
| 595 | T-125 | `openmir2-castle-war-area-impl` | reviewer-5 `a49984c2`（verdict） | `9b1986173` | 待独立 principal 复核 | 已审已合 ⇒ 改写 |
| 596 | T-126 | `openmir2-castle-war-area-impl` | reviewer-5 `a49984c2`（verdict） | `9b1986173` | 待独立 principal 复核 | 已审已合 ⇒ 改写 |
| 598 | T-128 | `openmir2-castle-war-area-impl` | reviewer-5 `a49984c2`（verdict） | `9b1986173` | 待独立 principal 复核 | 已审已合 ⇒ 改写 |
| 599 | T-129 | `openmir2-castle-war-area-impl` | reviewer-5 `a49984c2`（verdict） | `9b1986173` | 待独立 principal 复核 | 已审已合 ⇒ 改写 |
| 600 | T-130 | `openmir2-castle-war-area-impl` | reviewer-5 `a49984c2`（verdict） | `9b1986173` | 待独立 principal 复核 | 已审已合 ⇒ 改写 |
| 601 | T-131 | `openmir2-castle-war-area-impl` | reviewer-5 `a49984c2`（verdict） | `9b1986173` | 待独立 principal 复核 | 已审已合 ⇒ 改写 |
| 602 | T-132 | `openmir2-castle-war-area-impl` | reviewer-5 `a49984c2`（verdict） | `9b1986173` | 待独立 principal 复核 | 已审已合 ⇒ 改写 |
| 604 | T-134 | `openmir2-monster-ref-drains` | reviewer-5 `1a073c11`（verdict） | `641d50782` | 待独立 principal 复核 | 已审已合 ⇒ 改写 |
| 605 | T-135 | `openmir2-monster-ref-drains` | reviewer-5 `1a073c11`（verdict） | `641d50782` | 待独立 principal 复核 | 已审已合 ⇒ 改写 |
| 617 | T-138 | `openmir2-t138-request-castle-war` | reviewer-5 `30c79447`（verdict） | `4d547cb74` | 待独立 principal 复核 | 已审已合 ⇒ 改写 |
| 618 | T-139 | `openmir2-guild-official-npc` | reviewer-5 `12f3e5ac`（verdict） | `8761bd68c` | 待独立 principal 复核 | 已审已合 ⇒ 改写 |
| 620 | T-142 | `openmir2-script-playdice-openybdeal` | reviewer-5 `ae1e7697`（verdict）；后续 reviewer-21 `54eeabc3`（decision）、reviewer-20 `e0ff0b68`（verdict） | `13769d591`（同卡后续 `bff927e11`） | 待独立 principal 复核 | 已审已合 ⇒ 改写 |
| 621 | T-145 | `openmir2-market-def-impl` | reviewer-5 `59c396a6`（verdict） | `23748419d` | 待独立 principal 复核 | 已审已合 ⇒ 改写 |
| 629 | T-141 | — | — | — | 待独立 principal 复核 | 无交付卡号 ⇒ 保持原样（证据不足） |
| 657 | T-148 | `openmir2-monster-ref-drains` | reviewer-5 `1a073c11`（verdict） | `641d50782` | 待独立 principal 复核 | 已审已合 ⇒ 改写 |
| 669 | T-149 | — | — | — | 待独立 principal 复核 | 无交付卡号 ⇒ 保持原样（证据不足） |
| 670 | T-150 | — | — | — | 待独立 principal 复核 | 无交付卡号 ⇒ 保持原样（证据不足） |
| 671 | T-151 | — | — | — | 待独立 principal 复核 | 无交付卡号 ⇒ 保持原样（证据不足） |
| 677 | T-152 | `openmir2-a22-castle-persistence` | reviewer-5 `03d54984`（verdict） | `ca12cbf58` | 待独立 principal 复核 | 已审已合 ⇒ 改写 |
| 683 | T-153 | `openmir2-a22-castle-persistence` | reviewer-5 `03d54984`（verdict） | `ca12cbf58` | 待独立 principal 复核 | 已审已合 ⇒ 改写 |
| 685 | T-155 | `openmir2-npc-ctor-constants`<br>`openmir2-guard-ctor-constants` | reviewer-5 `a7de06c2`（verdict）<br>reviewer-5 `fe666219`（verdict） | `1f5132b2c`<br>`c17b18c2e` | 待独立 principal 复核 | 已审已合 ⇒ 改写 |
| 691 | T-156 | `openmir2-gm-commands-b1` | reviewer-5 `0bbbd515`（verdict） | `7eb281fac` | 待独立 principal 复核 | 已审已合 ⇒ 改写 |

## 3. 改写清单（24 行）

统一写法（只替换「谁审的」列里的状态短语，**不动任何 `path:line` 锚**）：

- 23 个 T 行：`**待独立 principal 复核**` → `**已审已合：approve <principal> \`<8 位>\`（<字段>）+ merge \`<merge oid>\`**`
- T-12（原为 `→ 待独立 principal 复核`，无粗体）：`→ 待独立 principal 复核` →
  `；已审已合：approve reviewer-4 \`627575c0\`（decision）+ merge \`7a878095\``
- T-148（原短语含窗口裁定）：`**待独立 principal 复核 + 窗口/owner 裁决**` →
  `**已审已合：approve reviewer-5 \`1a073c11\`（verdict）+ merge \`641d50782\`；窗口/owner 裁决见台账**`
- 第 256 行散文：`（\`useitem.rs\`，待审）` →
  `（\`useitem.rs\`，已审已合：approve reviewer-6 \`f069955a\`（decision）+ merge \`60095f65\`）`

改了 **24 个物理行**（`git diff --numstat` = `24 24`，`--word-diff` 逐行确认只有状态短语变化）。
T 号清单：`T-12`、`T-50`、`T-57`、`T-58`、`T-119`、`T-125`、`T-126`、`T-128`、`T-129`、
`T-130`、`T-131`、`T-132`、`T-134`、`T-135`、`T-138`、`T-139`、`T-142`、`T-145`、`T-148`、
`T-152`、`T-153`、`T-155`、`T-156`，加第 256 行散文。

## 4. 保持原样的 61 行

行号：135、136、137、138、146、147、148、149、227、228、246、247、248、249、264、265、266、
267、268、269、270、271、272、274、275、276、298、299、300、327、348、350、363、364、368、
394、406、421、432、436、451、452、524、531、532、533、534、535、536、537、538、562、563、
564、565、567、568、629、669、670、671。
（406/432 是引用块里的两行散文，不是 T 表行；其余 59 行是 T 行。）

T 号：`T-60`、`T-61`、`T-62`、`T-63`、`T-64`、`T-66`、`T-67`、`T-68`、`T-2`、`T-3`、
`T-8`、`T-9`、`T-10`、`T-11`、`T-13`、`T-14`、`T-15`、`T-16`、`T-17`、`T-18`、`T-19`、
`T-20`、`T-21`、`T-23`、`T-24`、`T-25`、`T-33`、`T-34`、`T-35`、`T-37`、`T-41`、`T-42`、
`T-69`、`T-43`、`T-44`、`T-46`、`T-45`、`T-49`、`T-53`、`T-55`、`T-86`、`T-87`、`T-88`、
`T-89`、`T-90`、`T-91`、`T-92`、`T-93`、`T-94`、`T-97`、`T-98`、`T-99`、`T-100`、
`T-102`、`T-103`、`T-141`、`T-149`、`T-150`、`T-151`。

这些行**没有任何可复核的行级证据**（行内无交付卡号 / 旧轮次 legacy）；按卡面 A2/FORBIDDEN，
**不美化、不改写**。

## 5. 残留物清单

- 临时文件（已删）：`.audit_tmp.txt`、`.audit_evidence.txt`、`.census_out.txt`、
  `.census_err.txt`、`.census_base.txt`、`.census_base_err.txt`、`.census_head.txt`、
  `.apply_edits.py`。
- 未起任何服务/未占端口；未跑 cargo；未改 `src/`、`sql/`、`MirClient`。
- 未改任何卡片状态（本卡只交付 patch + status，合并由 coordinator 做）。
- 已知未决（不在本卡写集）：61 行 legacy 行若要变成「已审已合」，必须先有行级证据
  （逐行开卡或补行内 review 引用）；本卡不猜、不批量套卡号。
