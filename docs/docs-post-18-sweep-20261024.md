# 文档旧计数/旧状态定向清扫（`openmir2-docs-post-18-sweep`）

- **卡**：`openmir2-docs-post-18-sweep`（tier=L）；worker `openmir2-worker-58`；分支 `card/docs-post-18-sweep`。
- **基线**：`walgit/master = 962d1f8a2`（开工时）；本分支从该 head 起。
- **写集**：仅 `docs/`。未改 batch-plan 与彩排表的**事实源条目**（只引用）、未改 `tools/**`、场景 JSON、`rules.json`、`whitelist.md`、任何卡片状态。
- **范围**：`spell_cast` 入表（16）与 `bag_equip`/`eat_item` 入表（18）之后，清扫 `docs/` 里残留的旧计数/旧状态断言（`11 行` / `16 场景` / `16 个场景` / `2 个假绿` / `10 行` / `15 个场景` / `尚未…inventory` / `bag_equip` 与 `不可跑` 同句）。

## 0. 唯一两个事实源（本报告只引用，不复制第三份口径）

- **计数**：`docs/window-batch-plan-20261023.md` §7.4 @ `81ecc0d28` = **12 行 / 18 场景**（`bag_equip`/`eat_item` 已移入「现在就能跑」；§7.4「仍不可跑」= **无**）。
- **彩排**：`docs/window-batch-rehearsal-20261024.md`（`e4a872e43` 合入 master）= **18 可跑 / 2 blocked / 0 假绿**（20 场景）。
- **前置口径**：`bag_equip`/`eat_item` 可跑（前置：场景自描述 `inventory`；profile 顶层 `inventory` 仍被静默忽略；真机窗口仍未跑）。

## 1. 复算命令（可原样重跑）

所有命令都加 `--exclude=docs-post-18-sweep-20261024.md`：本报告正文**逐字引用了**这些旧模式（命中表、本命令块自身），不排除会把报告算进去，无法复现 §2 的计数。计数口径见 §1.1。

```sh
cd <repo>

# 六个数字/措辞模式（已排除本报告；六模式单独并集 = 26 行，见 §1.1）
grep -rn --exclude=docs-post-18-sweep-20261024.md '11 行'      docs/
grep -rn --exclude=docs-post-18-sweep-20261024.md '16 场景'    docs/
grep -rn --exclude=docs-post-18-sweep-20261024.md '16 个场景'  docs/
grep -rn --exclude=docs-post-18-sweep-20261024.md '2 个假绿'   docs/   # 0 命中
grep -rn --exclude=docs-post-18-sweep-20261024.md '10 行'      docs/
grep -rn --exclude=docs-post-18-sweep-20261024.md '15 个场景'  docs/

# inventory 未声明（必须用「UTF-8 字符」模式；见 §1.1 的写法对照）
LC_ALL=C.UTF-8 grep -rnP --exclude=docs-post-18-sweep-20261024.md '尚未.{0,6}inventory' docs/
# 等价且可移植：grep -rnP --exclude=docs-post-18-sweep-20261024.md '(*UTF)尚未.{0,6}inventory' docs/
# 或宽匹配两段式（不依赖 -P 的 locale）：
grep -rnE --exclude=docs-post-18-sweep-20261024.md '未声明|尚未' docs/ | grep -E 'inventory|bag_equip|eat_item'

# bag_equip 与 不可跑 同句
grep -rn --exclude=docs-post-18-sweep-20261024.md 'bag_equip' docs/ | grep '不可跑'
```

### 1.1 计数口径与 `-P` 的假阴性

**为什么必须排除本报告**：本报告正文逐字占用了这些模式，且**每改一次报告，「含报告」计数就变一次**（不稳定、不可作为口径发布）。故所有复算命令统一 `--exclude` 本报告；下表只列**不含报告**（可复现）的口径。

| 模式 | 不含报告（§1 命令实测） |
|---|---|
| `11 行` | **8** |
| `16 场景` | **2** |
| `16 个场景` | **5** |
| `2 个假绿` | **0** |
| `10 行` | **10** |
| `15 个场景` | **10** |

**并集三个数（别混口径）**：
- **26** = 上表**六个模式单独**的并集（不含报告）；
- **31** = 六模式 + 两组补充命中（`尚未.{0,6}inventory` 3 行；`bag_equip` 与 `不可跑` 同句净增 2 行）的**总并集**（不含报告，已去重）——§2 的 32 处位置 = 这个 31 + 宽匹配补的 1 行（`drift:49`）；
- **30** = 从 31 中按旧版 `grep -v '111 行'` 剔掉 §2.4 #23（`docs/Rust重写设计文档.md:760` 的 `111 行` 子串）后的数。**该过滤器已删除**：`111 行` 本就计入 31。

**`grep -P` 会静默少报**（同一 `docs/`、同样 `--exclude`，仅换写法）：

| 写法 | 命中 | 结果 |
|---|---|---|
| `grep -rnE '尚未.{0,6}inventory'` | **1/3** | 字节口径；本机只中 `upgrade-retrieve:298`（reviewer 另一 locale 实测 0/3） |
| `grep -rnP '尚未.{0,6}inventory'`（裸 `-P`） | **1/3** | 未开 UTF，仍按字节；漏 `drift:42`、`needs-human-evening:11` |
| `LC_ALL=C.UTF-8 grep -rnP …` | **3/3** | 正确 |
| `grep -rnP '(*UTF)尚未.{0,6}inventory'` | **3/3** | 正确（可移植） |
| Python `re`（unicode） | **3/3** | 正确（本报告实际采用） |

> ⚠ 字符 vs 字节：UTF-8 一个汉字 3 字节；`**尚未声明** inventory` 这类间隔是 6 个字符 / **10 字节**，字节口径的 `.{0,6}` 抓不到。**复算请用后三种写法之一，不要用裸 `-P`。**

## 2. 命中表（32 处位置：八个字面模式并集 31 + 宽匹配补 1）

判定口径：**现时** = 非引号、非返工日志、非带戳快照，且断言当前 batch-plan/彩排计数或当前可跑性 ⇒ 必须改；**历史** = 引号内原文 / 返工日志 / `〔YYYY-MM-DD 订正〕` / 自述带戳快照 ⇒ 保留（留痕）；**非本口径** = 子串误命中或另一件事的行数。

### 2.1 现时断言 —— 6 处，全部已订正（均在彩排表）

| # | 位置 | 原文（摘要） | 判定 | 处置 |
|---|---|---|---|---|
| 1 | `docs/window-batch-rehearsal-20261024.md:15` | 「batch-plan §7.4 **目前仍写** 11 行 / 16 个场景 … 需要另一张卡把它 16 → 18」 | 现时 | 改为「**当时（@ `c4b66ddb1`）** §7.4 写 11 行 / 16 个场景；该差距已由 `81ecc0d28` 闭合 —— 现见 batch-plan §7.4 @ `81ecc0d28`，12 行 / 18 场景」。18/2/0 事实源原样保留。 |
| 2 | `…rehearsal…:19` | 「§7.4 @ `c4b66ddb1` **仍是** 11 行 / 16 个场景 … §7.4 需由 16 更新到 18」 | 现时 | 改为「**当时是** 11 行 / 16 个场景 … 已由 `81ecc0d28` 闭合 ⇒ 现见 §7.4 @ `81ecc0d28`，12 行 / 18 场景」。 |
| 3 | `…rehearsal…:136` | 「注意：batch-plan §7.4 **仍是** 11 行 / 16 个场景 … 需从 16 更新到 18」 | 现时 | 改为「本表基线 `c4b66ddb1` 时仍是 …；已由 `81ecc0d28` 闭合（12 行 / 18 场景）」。 |
| 4 | `…rehearsal…:292` | 「§7.4 @ `c4b66ddb1` —— **枚举** 11 行 / 16 个场景 … §7.4 **需要**从 16 更新到 18」 | 现时 | 改为「**当时枚举** 11 行 / 16 个场景 … 已由 `81ecc0d28` 闭合 ⇒ **现以 §7.4 @ `81ecc0d28` 为准，12 行 / 18 场景**」；16 场景逐行展开保留为「当时」的历史演算，并补「行粒度定为合成 1 行 = 12 行」。 |
| 5 | `…rehearsal…:294` | 「**§7.4 现在是** 11 行 / 16 场景，但对本次实测已不完整」 | 现时 | 改为「**当时（@ `c4b66ddb1`）** §7.4 是 11 行 / 16 场景 … **已由 `81ecc0d28` 更新为 12 行 / 18 场景**」。 |
| 6 | `…rehearsal…:295` | 「§7.4 的 `bag_equip`/`eat_item` 条目已过期 … §7.4 **需从** 16 更新到 18」（该条本身是 c4b66ddb1 时的实测差异） | 现时（指向性） | 差异叙述保留，末尾追加〔**2026-10-24 订正：已由 `81ecc0d28` 移入「现在就能跑」，现见 §7.4 @ `81ecc0d28`，12 行 / 18 场景**〕。 |

### 2.2 历史 / 带戳快照 / 返工日志 / 订正 —— 15 处，保留

| # | 位置 | 原文（摘要） | 判定依据 |
|---|---|---|---|
| 7 | `docs/docs-state-drift-sweep-20261024.md:42` | `eat_item` 场景**尚未声明** `inventory` ⇒ 仍不可跑 | 该文自述「快照 `2e3b8500a` … 本报告自己也会过期」，§5 命中表为带戳历史 |
| 8 | `…drift…:49`〔扩展命中〕 | 「`card_window.py` 已支持 `inventory`，但 `bag_equip`/`eat_item` 场景未声明 …『跑不了』的结论不变」 | 同上（快照 §5 命中表）；不由两个字面模式命中，由宽匹配发现 |
| 9 | `…drift…:51` | 「15 个场景文件『尚未进 master `be9c5ac6b`』」 | 快照命中表，且同行已给 `33b40970` 订正；指**场景文件数**，非 batch 可跑数 |
| 10 | `…drift…:133` | 「v5 起按逐行枚举为 **10 行 / 15 个场景** … 均原样保留」 | 描述 v5 历史原状 + 该卡「不修判据」范围声明 |
| 11 | `…drift…:142` | **Leader 裁决（计数）**：「§7.4 现为 **10 行 / 15 个场景** … `bag_equip`/`eat_item`（场景未声明 `inventory`）… 仍在『仍不可跑』」 | §8「返工 v5 … 含 Leader 裁决」= 返工日志，按卡规保留 |
| 12 | `docs/needs-human-20261023-evening.md:11` | 「`eat_item` **仍阻塞** … **尚未声明** `inventory` ⇒ 仍不可跑」（**2026-10-24 订正** 内文） | 文首「快照：master `ff88bdf2`」+ 该句本身是 dated 订正 |
| 13 | `docs/upgrade-retrieve-scenario-20261023.md:298` | 「本场景**尚未写** `inventory` 声明」（**2026-10-24 订正**） | dated 订正；且**仍准确**——已核 `tools/probe/scenarios/upgrade_retrieve.json` 无 `inventory` |
| 14 | `docs/npc-script-chain-scenario-20261023.md:112` | 「工具把新场景判绿（16 个场景，ok=13 + 3 个既有的 missing）」 | 该场景卡落地日志；指**场景文件数**，非 batch 可跑数 |
| 15 | `docs/npc-script-chain-scenario-20261023.md:161` | 「§4.5 的表是『15 个场景』口径；本卡合入后为 16 个」 | 场景文件数演进日志（历史） |
| 16 | `docs/param-discovery-design-20261023.md:14` | 「15 个场景里 9 个声明了 `required`」 | 设计文档写作时的场景文件数快照 |
| 17 | `docs/param-discovery-design-20261023.md:364` | 「acceptance[6]『15 个场景全绿』不可达」 | 设计文档的历史验收讨论 |
| 18 | `docs/rewrite-remaining-inventory-20261022.md:752` | 「15 个场景文件 … **尚未进 master**」+ **2026-10-24 订正**（现有 20 个 JSON） | 已带 dated 订正 |
| 19 | `docs/window-debt-reclass-20261023.md:21` | 「15 个场景文件 … 尚未进 master」+ **2026-10-24 订正** | 已带 dated 订正 |
| 20 | `docs/window-debt-reclass-20261023.md:203` | 「现有 15 个场景里没有任何一条发 `CM_RUN`」 | 场景文件数快照（历史设计文档） |
| 21 | `docs/window-runbook-20261023.md:390` | 「对照 15 个场景的 `required`（2026-10-23 实测…）」 | 带实测日期的历史快照 |

### 2.3 事实源 —— 1 处，保留（只引用，不改）

| # | 位置 | 原文（摘要） | 判定 |
|---|---|---|---|
| 22 | `docs/window-batch-plan-20261023.md:491` | 「**仍不可跑**：**无**（上表 18 个场景全部可跑；`bag_equip`/`eat_item` 已于 2026-10-24 移入上表）」 | 即事实源本身，已正确；红线禁止改 |

### 2.4 子串误命中 / 非本口径 —— 10 处，保留

| # | 位置 | 命中片段 | 判定 |
|---|---|---|---|
| 23 | `docs/Rust重写设计文档.md:760` | 「5 表/**111 行**×2」 | 子串（`111 行`） |
| 24 | `docs/envir-root-host-20261023.md:114` | 「在第 **610 行**」 | 子串（`10 行`） |
| 25 | `docs/gm-script-exec-seams-20261023.md:135` | 「T-**211 行**与 §8」 | 子串（`11 行`） |
| 26 | `docs/gm-script-host-seam-20261023.md:141` | 「T-**210 行**」 | 子串（`10 行`） |
| 27 | `docs/gm-script-host-seam-20261023.md:144` | 「已在 T-**210 行**登记」 | 子串（`10 行`） |
| 28 | `docs/remaining-plan-v2-20261021.md:165` | 「宿主队列 H1–H3 共 **11 行**」 | 另一个口径（实现/队列行数） |
| 29 | `docs/r-window-registry-batch-3-20261023.md:6` | 「`T-RESERVATIONS.md`（+**10 行**预约）」 | 另一个口径（预约行数） |
| 30 | `docs/rules-pending-approval-pack-20261023.md:606` | 「这 **10 行**恰好是 `rules[…]`」 | 另一个口径（rules 行数） |
| 31 | `docs/script-host-noop-audit-20261023.md:325` | 「部署可达 **10 行**」 | 另一个口径（代码行数） |
| 32 | `docs/whitelist-review-status-audit-20261024.md:16` | 「`S4B-1`…`S4B-10` **10 行**」 | 子串 + 另一个口径（whitelist 行数） |

## 3. 处置汇总与一致性

- **位置总数**：32 = 6 现时（全改）+ 15 历史/快照/返工日志（保留）+ 1 事实源（保留）+ 10 非本口径/子串（保留）。
- **只改了 1 个文件**：`docs/window-batch-rehearsal-20261024.md`，`git diff --numstat` = `6  6`，与「现时命中 6 处」逐一对应。
- **事实源数字未动**：彩排表的 18 / 2 / 0、batch-plan §7.4 的 12 / 18 与枚举均未被改写；只把**指向 §7.4 的过期指针**改成「当时（@ `c4b66ddb1`）… / 已由 `81ecc0d28` 更新为 …」。
- **未新增第三份口径**：文中所有数字只引用上述两个事实源。

## 4. 判不准 / 边界

- **无「判不准」项**。唯一边界是 `docs/docs-state-drift-sweep-20261024.md`：其 §5/§6/§8 的 `10 行 / 15 个场景` 与「`eat_item` 未声明 `inventory`」现在是旧话，但该文**自述为带戳快照**（「本报告自己也会过期，引用前先带上这个戳」），且 §8 明确是返工日志 ⇒ 判**历史、保留**。若 owner 要求「现态压过留痕」，需另开卡（不在本卡写集判断内）。
- `docs/upgrade-retrieve-scenario-20261023.md:298` 的「尚未写 `inventory` 声明」**现仍准确**（`upgrade_retrieve.json` 无 `inventory` 键），不是陈旧断言。

## 5. 残留物

- 无临时脚本入库；扫描只在本地终端进行，未留中间文件。仅新增本报告与上述 1 文件订正。

## 6. 复算证据（本卡收集）

```sh
git rev-parse HEAD                                  # 交付 head
git diff --numstat 962d1f8a2 -- docs/               # 期望：rehearsal 6/6 + 本报告 (added)
git diff --check 962d1f8a2 -- docs/                 # 期望 rc=0
python -c "d=open('docs/window-batch-rehearsal-20261024.md','rb').read(); print(d.count(b'\r\n'))"   # 期望 0
```
