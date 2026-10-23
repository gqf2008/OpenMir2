# `rules.json` PENDING 规则一揽子审批包（2026-10-23）

> 产出：`openmir2-worker-66`（worktree `wt-worker-66`，branch `card/rules-approval-pack`）
> 基线：`walgit/master` = `121a6b48b`（2026-10-23）
> 被审对象：`tools/conformance/rules.json`，`sha256 = e354f45fb946787e1fe3fdbc3f515243dae4725cc72265a82ccf8bdb4ef0f0ce`
> 本文件只产出**材料**：未改 `rules.json`、未改任何工具代码、未替任何人宣布批准。批准动作由 owner/coordinator 执行（见 §7）。
> 签名可核：本材料挂在卡 `openmir2-rules-pending-approval-pack` 的 `status: needs-review` 条目上（actor `openmir2-worker-66`，head oid 见线程）。

---

## 0. 一页结论

**待批条数 = 43**（`global` 4 + `rules` 38 + `db` 1），不是卡面写的“38 条规则”。卡面的 38 是 `rules[]` 的条数；`RuleSet` 还会把 `global[]` 与 `db` 的 PENDING 一起列进 `pending_review`（`tools/conformance/compare_runs.py:227/:295/:331`）。

**硬阻塞只有 1 条**：`rules[2] ident 14 action=ignore`。只有 PENDING 的 `ignore` 在**真的剔帧**时判 `UNDECIDABLE(rc=2, pending_ignore_rule)`（`compare_runs.py:1518-1524`）。其余 42 条 PENDING **不会**改变退出码，只让报告带 `rules.rules_pending_review=true`（`compare_runs.py:1402-1403`、`:1568`）——即“本次窗口是拿未复核的判据判出来的”，不阻断机械执行，但不足以作为 R 级窗口的验收证据。

**风险分级（判定标准见 §3）：**

| 分级 | 条数 | 含义 |
| --- | ---: | --- |
| ① 证据充分、可批 | **33** | oracle 逐字段锚点可核 + 构造级牙 + 掩码边界写实且不吞未登记的确定性差异 |
| ② 需先补证据 | **10** | 有实质判据缺陷：7 条被全局 `recog` 掩码吞掉语义字段、2 条残余未追踪/措辞不实、1 条有**已签名的保留条件** |

**② 的 10 条（必须先返工或由 owner 明确裁定，不能当“顺手批掉”）：**

| 位置 | ident / action | 缺什么 | 建议 |
| --- | --- | --- | --- |
| `rules[2]` | 14 `ignore` | reviewer-2 已签“在 `openmir2-castle-merchant-fixedhide-hit` 落地或工具加计数/形态比较前，`approved_by` 必须保持 PENDING”，且“本 approve 不构成对该变更的授权”（`46e479fc`、`2f4b311f`） | 改后再批 / 由 owner 显式解除条件 |
| `rules[0]` | 10 `compare+head_mask[series]` | 掩的是整个 `MakeWord(Dir,Light)`，其它 actor 的确定性转身（如怪物转向攻击）无独立判据；残余没有追踪卡/T 行；reason 写“对所有 actor（自己/怪物/NPC）”，但**自己根本不收 SM_TURN**（`PlayObject.Message.cs:1528` 自过滤） | 改后再批（订正 reason + 落追踪行） |
| `rules[5]` | 50 `mask_fields[series]` | series=MakeWord(Dir,Light)，reason 只论证了 Light 是墙钟；**登录时的 Dir 也被一并掩掉**，而 Dir 不是墙钟量 | 改后再批（写实 Dir 代价 + 独立判据或追踪行） |
| `rules[10]` | 54 `compare` | 下行 `SendDefMessage(SM_MAPDESCRIPTION, nMusicid, …)`（`PlayObject.Base.cs:894`）⇒ **recog=音乐号是语义值**，被全局 recog 掩码吞掉 | 改后再批：加 `head_compare:["recog"]` |
| `rules[11]` | 5008 `compare` | `SendDefMessage(SM_GAMEGOLDNAME, GameGold, …)`（`PlayObject.Base.cs:938`）⇒ **recog=GameGold 金额**被吞 | 改后再批：加 `head_compare:["recog"]` |
| `rules[12]` | 1106 `compare` | `MakeMessage(SM_VERSION_FAIL, ClientFile1_CRC, …)`（`PlayObject.Message.cs:1690`）⇒ **recog=CRC1** 被吞 | 改后再批：加 `head_compare:["recog"]` |
| `rules[13]` | 752 `compare` | `SendDefMessage(SM_SUBABILITY, MakeLong(MakeWord(AntiMagic,0),0), …)`（`:1675`/`:1935`）⇒ **recog=AntiMagic** 被吞；且该帧**无 body**，reason 写的“body 逐字节比”不成立 | 改后再批：加 `head_compare:["recog"]` + 订正 reason |
| `rules[14]` | 811 `compare` | `MakeMessage(SM_ADJUST_BONUS, BonusPoint, …)`（`PlayObject.Base.cs:1208`）⇒ **recog=BonusPoint** 被吞（body 的 3×20B 段仍比） | 改后再批：加 `head_compare:["recog"]` |
| `rules[15]` | 766 `compare` | `SendDefMessage(SM_AREASTATE, n8, 0,0,0)`（`PlayObject.Base.cs:285`）⇒ **recog=n8 就是区域状态位**，无 body；全局 recog 掩码后该规则**实际什么都不比** | 改后再批：加 `head_compare:["recog"]` |
| `rules[17]` | 213 `compare` | `SendDefMessage(SM_ATTACKMODE, (byte)AttatckMode, 0,0,0)`（`PlayObject.cs:1146`）⇒ **recog=攻击模式**，无 body；掩码后**实际什么都不比** | 改后再批：加 `head_compare:["recog"]` |

**批准范围有两种口径，别混：**

- **口径 A（只为解除窗口硬阻塞）**：只需要 `ident 14` 这一条。其余 PENDING 不产生 rc=2。
- **口径 B（为清掉 `rules_pending_review=true`）**：43 条全批。按本包结论 = 33 条可批 + 10 条返工后再批。
- **口径 C（推荐）**：先批 33 条 → 7 条 `head_compare` 返工（54/5008/1106/752/811/766/213）+ 2 条残余登记（10/50）→ owner 裁定 ident 14 → 最后一批收口。

---

## 1. 清单：可复算命令与输出（不许手抄）

命令（Git Bash / PowerShell 均可；单行，无 `$` 展开）：

```sh
python -I -c "import json;d=json.load(open('tools/conformance/rules.json',encoding='utf-8'));rows=[('global[%d]'%i,r) for i,r in enumerate(d.get('global',[]))]+[('rules[%d]'%i,r) for i,r in enumerate(d.get('rules',[]))]+([('db',d['db'])] if bool(d.get('db')) else []);p=[(w,r.get('ident','-'),r.get('action'),r.get('tier')) for w,r in rows if str(r.get('approved_by','')).upper().startswith('PENDING')];print('PENDING',len(p),'/',len(rows));[print(*x) for x in p]"
```

输出（2026-10-23，master `121a6b48b`，`rules.json` sha256 `e354f45f…`）：

```
PENDING 43 / 43
global[0] - mask_fields L
global[1] - mask_fields L
global[2] - mask_fields L
global[3] - mask_fields L
rules[0] 10 compare R
rules[1] 41 compare L
rules[2] 14 ignore R
rules[3] 31 mask_body_ranges L
rules[4] 46 mask_fields L
rules[5] 50 mask_fields L
rules[6] 51 mask_fields L
rules[7] 52 ability_layout L
rules[8] 658 compare L
rules[9] 42 compare L
rules[10] 54 compare L
rules[11] 5008 compare L
rules[12] 1106 compare L
rules[13] 752 compare L
rules[14] 811 compare L
rules[15] 766 compare L
rules[16] 11029 compare L
rules[17] 213 compare L
rules[18] 100 multiset_by_tick L
rules[19] 520 compare L
rules[20] 525 compare L
rules[21] 529 compare L
rules[22] 530 mask_body_ranges L
rules[23] 654 compare L
rules[24] 657 compare L
rules[25] 11 compare L
rules[26] 13 compare L
rules[27] 200 bagitems_records R
rules[28] 201 bagitems_records R
rules[29] 615 compare L
rules[30] 616 compare L
rules[31] 635 compare L
rules[32] 636 compare L
rules[33] 643 compare L
rules[34] 644 compare L
rules[35] 645 compare L
rules[36] 763 compare L
rules[37] 767 compare L
db - None L
```

说明：`db` 条目没有 `action`（它是库对账口径，不是帧规则）。

按 `approved_by` 的草稿人分组（同一命令的变体，合计 43）：`openmir2-worker-4` 28 条（global 4 + rules 23 + db 1）、`openmir2-worker-68` 1 条（31，返工后标注 `rework after review 926d1b67`）、`openmir2-worker-26` 1 条（100）、`openmir2-worker-70` 13 条（11/13/200/201/615/616/635/636/643/644/645/763/767）。

---

## 2. PENDING 到底影响什么（先把机制钉死）

| 规则动作 | PENDING 的实际效果 | 依据 |
| --- | --- | --- |
| `ignore` | **真的剔了帧 ⇒ `UNDECIDABLE(rc=2)`**（`pending_ignore_rule`）；没剔帧则只标 `rules_pending_review=true` | `compare_runs.py:1025-1035`（`pending_ignore` 收集）、`:1518-1524`（判 2） |
| `compare` / `mask_fields` / `mask_body_ranges` / `ability_layout` / `multiset_by_tick` / `bagitems_records` | **不改退出码**，只把条目列进 `report.rules.pending_review` 并置 `rules_pending_review=true` | `compare_runs.py:227/:295/:331`（收集）、`:1402-1403`（写报告） |
| `global[]` / `db` | 同上：只标 PENDING，不判 2 | 同上 |
| 当前 `ignore` 条数 | **只有 1 条**（`ident 14`）。`selftest` 有断言钉住“默认表只有 14 是 ignore” | `selftest_impl.py:209-211` |

⇒ **“今晚的窗口会不会 UNDECIDABLE”只取决于 ident 14 是否被剔除**。其余 PENDING 的伤害是“证据等级”，不是“机械失败”。`equip-struck-save` 复审（`8ed0da42`，reviewer-6，approve/tier=L）的 finding #2 正是这个点：约 3s 窗口、12 次 `Merchant.Run` 量级，Rust 若再发一条 14 即整次 UNDECIDABLE。

---

## 3. 风险分级：判定标准（不是只给结论）

一条规则要判“证据充分、可批”，必须同时满足 A/B/C：

- **A. 锚点可核**：reason 里的 oracle 依据必须能 `grep -n` 定位到具体文件与行；掩码类必须**逐字段/逐区间**写清（`fields`/`head_mask`/`head_compare`/`ranges`/`record_mask` 与文案一致）。纯 `compare` 类可放宽为“帧号常量 + 至少一个下行点”；若 reason 只有一句描述、没有行号，本包负责补锚（见各条“依据”）。
- **B. 构造级牙**：`selftest` 至少覆盖 (i) 两侧相同 ⇒ 0；(ii) **未掩部分**改一字节 ⇒ 1 且定位到 ident/字段/偏移。掩码类还必须覆盖 (iii) 被掩字段改 ⇒ 0 且显式登记（`masked_head`/`mask_body_ranges`/`bagitems_masked_field` 等）；(iv) 掩码边界外改动 ⇒ 1（证明掩码不外溢）。
- **C. 不掩盖真实差异**：无掩码 ⇒ 自动满足。有掩码 ⇒ 被掩部分必须满足其一：(c1) oracle 语义证明它跨进程不可复现（墙钟/RNG/会话作用域）；(c2) 有**独立判据**覆盖（另一个帧/DB 对账/构造级用例）；(c3) 残余逐字节写清并**有追踪卡或 T 行**。若掩码覆盖到确定性路径且三条都不满足 ⇒ 判 ②。

判不准 ⇒ 按 ②（保守）。

---

## 4. 清单总表（43 行）

| 位置 | ident | action | tier | 分级 | 建议 |
| --- | ---: | --- | --- | --- | --- |
| global[0] | — | mask_fields(recog) | L | ① | approve |
| global[1] | — | mask_fields(t_ms) | L | ① | approve |
| global[2] | — | mask_fields(conn) | L | ① | approve |
| global[3] | — | mask_fields(seq) | L | ① | approve |
| db | — | time_mode=presence | L | ① | approve |
| rules[0] | 10 | compare, head_mask=[series] | R | ② | 改后再批 |
| rules[1] | 41 | compare | L | ① | approve |
| rules[2] | 14 | ignore | R | ② | 改后再批 / owner 裁定 |
| rules[3] | 31 | mask_body_ranges [[10,16]], head_mask=[param] | L | ① | approve（带残余说明） |
| rules[4] | 46 | mask_fields=[param,tag] | L | ① | approve（建议补锚点） |
| rules[5] | 50 | mask_fields=[series] | L | ② | 改后再批 |
| rules[6] | 51 | mask_fields=[series] | L | ① | approve |
| rules[7] | 52 | ability_layout, head_compare=[recog] | L | ① | approve |
| rules[8] | 658 | compare | L | ① | approve |
| rules[9] | 42 | compare | L | ① | approve |
| rules[10] | 54 | compare | L | ② | 改后再批（+head_compare[recog]） |
| rules[11] | 5008 | compare | L | ② | 改后再批（+head_compare[recog]） |
| rules[12] | 1106 | compare | L | ② | 改后再批（+head_compare[recog]） |
| rules[13] | 752 | compare | L | ② | 改后再批（+head_compare[recog]） |
| rules[14] | 811 | compare | L | ② | 改后再批（+head_compare[recog]） |
| rules[15] | 766 | compare | L | ② | 改后再批（+head_compare[recog]） |
| rules[16] | 11029 | compare | L | ① | approve |
| rules[17] | 213 | compare | L | ② | 改后再批（+head_compare[recog]） |
| rules[18] | 100 | multiset_by_tick | L | ① | approve |
| rules[19] | 520 | compare, head_compare=[recog] | L | ① | approve |
| rules[20] | 525 | compare | L | ① | approve（可选加 head_compare[recog]） |
| rules[21] | 529 | compare | L | ① | approve（可选加 head_compare[recog]） |
| rules[22] | 530 | mask_body_ranges [[20,null]] | L | ① | approve |
| rules[23] | 654 | compare | L | ① | approve |
| rules[24] | 657 | compare | L | ① | approve |
| rules[25] | 11 | compare, head_mask=[series] | L | ① | approve（无真实录制验证） |
| rules[26] | 13 | compare, head_mask=[series] | L | ① | approve（无真实录制验证） |
| rules[27] | 200 | bagitems_records, record_mask=[MakeIndex] | R | ① | approve |
| rules[28] | 201 | bagitems_records, record_mask=[MakeIndex] | R | ① | approve |
| rules[29] | 615 | compare, head_compare=[recog,param] | L | ① | approve |
| rules[30] | 616 | compare, head_compare=[recog] | L | ① | approve |
| rules[31] | 635 | compare | L | ① | approve |
| rules[32] | 636 | compare | L | ① | approve |
| rules[33] | 643 | compare | L | ① | approve |
| rules[34] | 644 | compare | L | ① | approve |
| rules[35] | 645 | compare | L | ① | approve |
| rules[36] | 763 | compare, head_compare=[recog] | L | ① | approve |
| rules[37] | 767 | compare | L | ① | approve |

---

## 5. 逐条材料

> 每条给出：① ident + action；② 掩码/范围确切内容；③ 依据（oracle `file:line` + 提出卡/评审 oid）；④ 不批的后果；⑤ 建议。
> 行号均为本卡 `grep -n`/`sed -n` 复核值；与原文不符处已在 §9 记录。

### global[0] — `recog`（`mask_fields`，tier L）

1. **动作**：`mask_fields fields=["recog"]`（全局）。
2. **掩码内容**：所有帧的 `recog` 头部字段不参与判定。
3. **依据**：`CommandMessage.Recog` 多数是会话相关 ActorId（`HUtil32.Sequence()`/`EcsStore::alloc_id`）；实测同帧 C# `255025409` vs Rust `275`（`docs/window-run-sheet-20261021.md:732`，本卡复核该行确为“跨侧只能用坐标定位”段）。语义型 recog 由 per-ident `head_compare` 取回（52/520/615/616/763）。
4. **不批的后果**：不产生 rc=2；只让报告带 `rules_pending_review=true`。
5. **建议**：**approve**（策略本身正确）。**附带条件**：本轮必须同时修 §0 表里的 7 条 per-ident `head_compare`（54/5008/1106/752/811/766/213），否则全局掩码会继续吞掉它们的语义 recog。

### global[1] — `t_ms`（tier L）

1. **动作**：`mask_fields fields=["t_ms"]`。
2. **内容**：驱动器墙钟毫秒不参与判定；`multiset_by_tick` 只用它标注 tick 窗口（`rules[18]` reason、`compare_runs.py:1040-1058`）。
3. **依据**：两次独立运行墙钟不可能相等，它不是协议内容。
4. **不批**：同上，只标 pending。
5. **建议**：**approve**。

### global[2] — `conn`（tier L）

1. **动作**：`mask_fields fields=["conn"]`。
2. **内容**：驱动器会话标签（oracle/rust 命名不同）不参与判定；对齐按帧序。
3. **依据**：非协议字段，仅人读。
4. **不批**：只标 pending。
5. **建议**：**approve**。

### global[3] — `seq`（tier L）

1. **动作**：`mask_fields fields=["seq"]`。
2. **内容**：驱动器本地自增序号不参与判定。
3. **依据**：不是服务端协议字段；比较器按文件顺序对齐（`compare_runs.py` 文档头“按序对齐（不是按 seq 找键）”）。
4. **不批**：只标 pending。
5. **建议**：**approve**。

### `db` — `time_mode=presence`（tier L）

1. **动作**：`db.time_mode="presence"`、`key_mode="pk"`、`ignore_columns=[]`、`allow_unknown_table=false`。
2. **内容**：时间列只比 NULL/非 NULL；需要逐时刻比时用 CLI `--db-time-mode exact/normalized`。
3. **依据**：两次运行墙钟不同；`selftest` 的库用例覆盖“字段值不同 ⇒ rc 1”“单侧快照 ⇒ rc 2”“两侧无快照 ⇒ rc 0+absent_both”（`selftest_impl.py:1403+`，本卡实跑 198/198 GREEN 含这些断言）。
4. **不批**：只标 pending（`pending_review` 里是 `"db"`）。
5. **建议**：**approve**。残余：时间列的“值变化”本规则不判，需要时用 CLI 提级——这是写明的口径，不是隐藏。

### rules[0] — ident 10 `SM_TURN`，`compare + head_mask:[series]`（tier R）

1. **动作**：帧仍参与按序对齐与 body/param/tag 比较；只掩 `series`。
2. **掩码确切内容**：整个 `series` word = `MakeWord(Dir, Light)`（低字节 Dir、高字节 Light）。`body`、`param`、`tag` 逐字节比；`recog` 另走全局掩码。
3. **依据（已核）**：
   - oracle：`SM_TURN=10`（`src/OpenMir2/Messages.cs:161`）；固定 NPC 的 Dir 改写 `TurnTo(RandomNumber.RandomByte(8))` 在 `src/GameSrv/Npc/GuildOfficial.cs:43`、`src/GameSrv/Npc/Merchant.cs:134`；随机源墙钟播种 `src/OpenMir2/RandomNumber.cs:35`（本卡逐行复核）。
   - 双跑现象：oracle `518(0x0206)` vs Rust `516(0x0204)`，Light 同为 2、只差 Dir（`docs/window-529-dualrun-20261023.md:102`）。
   - 提出卡：`openmir2-conformance-rng-nondeterminism` issue `0f0db14e`；patch `e931c3a8`；review `8a59cd6c`（reviewer-2，request_changes，R）；patch `e1aa9127`；review **`46e479fc`（reviewer-2，approve，tier R）**；补正 review `2f4b311f`（reviewer-2，approve，tier L）。
4. **不批的后果**：不 rc=2；但 R 级窗口若用它判绿，报告带 `rules_pending_review=true`，不能被当作已复核判据。
5. **建议**：**改后再批**（C 不满足）：
   - reason 的“对所有 actor（自己/怪物/NPC）”不实：`case Messages.RM_TURN` 的 handler 条件是 `processMsg.ActorId != ActorId`（`src/M2Server/Player/PlayObject.Message.cs:1528`）⇒ **玩家自己的 SM_TURN 不发给自己**；被掩的是“其他 actor”的 Dir。
   - 其他 actor 的确定性转身（怪物追击转向）仍会被掩掉，当前没有独立判据，也没有追踪卡/T 行（reviewer-3 在 `c3f5e854` 建议的 T-224 未落地）。
   - 改法（最小）：订正上述两句 + 用 `tools/whitelist/next_t.py` 取号落一条追踪行；若要真正判它，需给比较器加“按 actor 类型条件掩码”（工具改动，另开卡）。

### rules[1] — ident 41 `SM_FEATURECHANGED`，`compare`（tier L）

1. **动作**：无掩码，全字段 + body 逐字节比。
2. **掩码/范围**：无（`recog` 走全局）。
3. **依据（已核）**：`SM_FEATURECHANGED=41`（`src/OpenMir2/Messages.cs:202`）；downlink 把它与 SM_TURN 当“一组两帧”并有黄金断言（`mir2-rs/bins/game-svr/src/downlink.rs` 文档头，review `d280be71` CH-2 引 `Messages.cs:161/202`）。提出卡：`openmir2-conformance-harness` issue `fc388535`；patch `bddd5ecd`；review `d280be71`（reviewer-3，request_changes，L）；修复 patch `f2c7050e`；review **`58f1056b`（reviewer-3，approve，L）**。
4. **不批**：只标 pending。
5. **建议**：**approve**。纯 compare 不放松判据；CH-2 已把“心跳剔除”改为真帧比较。

### rules[2] — ident 14 `SM_HIT`，`ignore`（tier R）【硬阻塞】

1. **动作**：按序对齐前**整帧剔除**（两侧各自先剔）。
2. **掩码确切内容**：无“掩码”；整帧不参与任何比较（Recog/Param/Tag/Series/body 与收件人集合都不比）。
3. **依据（已核）**：
   - 随机支：`GuildOfficial.Run` 的 `Random(30)==0` → `SendRefMsg(RM_HIT,…)`（`src/GameSrv/Npc/GuildOfficial.cs:47`、`:49`）；`Merchant.Run` 第二个 `Random(50)==0`（`Merchant.cs:138`、`:140`）；随机源 `new Random()` 墙钟播种（`src/OpenMir2/RandomNumber.cs:35`）。
   - 确定性支：`FixedHideMode` 解除 `Merchant.cs:152-157`，`SendRefMsg(RM_HIT,…)` 在 `:156`（本卡逐行复核：`:155` 清零、`:156` 发帧）。
   - 形态/收件人：`RM_HIT` handler 带 `processMsg.ActorId != ActorId`（`PlayObject.Message.cs:1434-1436`）；Rust 编码器 `downlink::hit_frame` 与 T-154 F1 用例钉形态（`downlink.rs:489-510`、`:1196-1201`）。
   - 现象：529 双跑 Rust（seed=42）多发一条 14、oracle 0 条（`docs/window-529-dualrun-20261023.md:101`）；确认性复核 review `46e479fc` F2/F6、`2f4b311f`。
   - 提出卡：`openmir2-conformance-rng-nondeterminism` issue `0f0db14e`；patch `e931c3a8`；review `8a59cd6c` RC；patch `e1aa9127`；review **`46e479fc` approve (R)**；patch `07110f7d`；review `2f4b311f` approve (L)。
   - 确定性支追踪：`openmir2-castle-merchant-fixedhide-hit` issue `1a807804`，已合入 master `0119e47b9`（approve reviewer-1 `9a15b427`，tier=R，`window_required=true`），卡片仍 `in-progress`（窗口项⑧未跑）；构造级判据 5 条与 `whitelist.md` T-192 已落。
   - 窗口风险点名：`openmir2-scenario-equip-struck-save` review **`8ed0da42`**（reviewer-6，approve，tier=L）finding #2。
4. **不批的后果**：**最硬的一条**——只要本次运行两侧剔出任何 14，整次判 `UNDECIDABLE(rc=2, pending_ignore_rule)`。历史录制里 14 很常见（reviewer-14 在 `b63fdaed` 扫盘：`w24-csharp-bagq` 3、`chouma` 1、`kaiz` 3、`kaiz30` 1、`w24-rust-login` 1、`dice*` 各 1、`529/rust` 1）。
5. **建议**：**改后再批 / 由 owner 显式解除条件**。理由不是“证据不够”（随机支证据充分），而是 **reviewer-2 已签下保留条件**：`46e479fc` 的 `f2_judgement.conditions` 明写「`approved_by` 必须保持 PENDING，直到 `openmir2-castle-merchant-fixedhide-hit` 落地或计数/形态比较机制就位；本 review 不授权去掉 PENDING」；`2f4b311f` 再次确认「本 approve 不构成对该变更的授权」。今天该卡**代码已落地 + T-192 构造级判据**，但 **R 级真机窗口（⑧ 城堡战进入/退出）未跑**。⇒ owner 必须先裁定“落地”的口径：① 认可“合并 + T-192 构造级”即算落地 ⇒ 可批；② 要求真机窗口 ⇒ 先排窗口。**在此之前不要 de-PENDING**。另：`--ignore-idents 14` 是每次窗口可用的显式处置，但它不替代 approve（runbook `docs/window-runbook-20261023.md:74-87`）。

### rules[3] — ident 31 `SM_STRUCK`，`mask_body_ranges` + `head_mask:[param]`（tier L）

1. **动作**：`action=mask_body_ranges`；`head_mask=["param"]`；`ranges=[[10,16]]`（**线缆空间**）。
2. **掩码确切内容**：① `param`（受击者当前 HP）不比；② body 线缆字节 `[10,16)` 不比（Tag1=攻击者 ActorId 的 EDCode 足迹）。`tag`（MaxHP）、`series`（本次伤害 nDmg）、body 的其余字节、`recog` 走全局掩码。
3. **依据（已核）**：
   - 下行：`PlayObject.Message.cs:1601` `MakeMessage(SM_STRUCK, processMsg.ActorId, WAbil.HP, WAbil.MaxHP, wParam)`；body `MessageBodyWL{Param1=GetFeature, Param2=CharStatus, Tag1=nParam3, Tag2=Mag?1:0}`（`:1602-1613`）；发点 `BaseObject.Attack.cs:63`、`PlayObject.Base.cs:1479`。
   - EDCode 映射：`EncryptUtil.cs:108-140`（16B 明文 ⇒ 22B 线缆）、Rust 同式 `mir2-rs/crates/protocol/src/edcode.rs:45-71`。
   - 提出卡：`openmir2-conformance-rule-sm-struck-31` issue `b050ea8f`；patch `1782f1d1`；review `926d1b67`（reviewer-1，request_changes，L）；返工 patch `d0a5ece8`；review **`38b40efc`（reviewer-1，approve，L）**。`approved_by` 的 `rework after review 926d1b67` 与之一致。
   - 牙：selftest 4 颗——去掉 31 规则 ⇒ rc2；整帧 ignore ⇒ tag 被吞；把 recog 加回 `head_compare` ⇒ rc1；ranges 放宽成 `[[0,null]]` ⇒ Feature 差异被吞（`selftest_impl.py:703-890`，README 有逐条清单）。
4. **不批**：只标 pending；R3 combat_strike 窗口的证据会用未复核判据。
5. **建议**：**approve**，残余写清：连续区间 `[10,16)` 连带掩掉 CharStatus 明文[6]/[7] 的 bits 2,3（与明文[8] 共用线缆[11]）——已在 reason 登记为“过掩边界”，其余位与其余字段仍逐字节比。这是 EDCode 位共享导致的**有界**过掩，不是整帧放行。

### rules[4] — ident 46 `SM_DAYCHANGING`，`mask_fields:[param,tag]`（tier L）

1. **动作**：`fields=["param","tag"]`（param=Bright，tag=DayBright）。
2. **内容**：只掩亮度两字段；`body`、`recog`（全局）、`series` 照常比。
3. **依据（已核）**：下行 `SendDefMessage(SM_DAYCHANGING, 0, Bright, DayBright(), 0)`（`src/M2Server/Player/PlayObject.Message.cs:1741`；reason 原文未给行号，本卡补）。`SM_DAYCHANGING=46`（`Messages.cs:207`）。提出卡：`openmir2-conformance-harness`（issue `fc388535`，review `d280be71`→`58f1056b`）。
4. **不批**：只标 pending。
5. **建议**：**approve**；批时建议把 `PlayObject.Message.cs:1741` 补进 reason（纯文案，不改行为）。亮度是墙钟派生（两次运行启动时刻不同），掩掉是必要代价；body 仍逐字节比。

### rules[5] — ident 50 `SM_LOGON`，`mask_fields:[series]`（tier L）

1. **动作**：`fields=["series"]`；`series=MakeWord(Dir, Light)`。
2. **掩码内容**：整个 series word（Dir + Light）；body 与 param/tag 仍比。
3. **依据（已核）**：`SendLogon()` 的 `MakeMessage(SM_LOGON, ActorId, CurrX, CurrY, MakeWord(Dir, Light))`（`src/M2Server/Player/PlayObject.cs:1131`）。提出卡：`openmir2-conformance-harness`（初始表即含 50，review `d280be71`/`58f1056b`）。
4. **不批**：只标 pending。
5. **建议**：**改后再批**。reason 只论证了 `Light` 是墙钟/光照相关，**没有论证 Dir**；而系列字把 Dir 一并掩掉。SM_LOGON 是**发给玩家自己**的帧（`SendLogon` 直接 `SendSocket`），登录朝向是确定性值，掩掉它没有任何独立判据（玩家自己的 SM_TURN 被 `:1528` 的自过滤挡住）。改法（最小）：在 reason 写实“Dir 也被掩、登录 Dir 无帧级判据”，并用 `next_t.py` 落追踪行；要真判 Dir，需要另立一个能拿到登录朝向的判据（构造级或第二观察者）。

### rules[6] — ident 51 `SM_NEWMAP`，`mask_fields:[series]`（tier L）

1. **动作**：`fields=["series"]`；`series=DayBright()`。
2. **内容**：只掩亮度；body（地图文件名）、param/tag 仍比。
3. **依据（已核）**：`MakeMessage(SM_NEWMAP, ActorId, CurrX, CurrY, DayBright())` + `EDCode.EncodeString(MapFileName)`（`PlayObject.Message.cs:1681-1682`）。`SM_NEWMAP=51`（`Messages.cs:209`）。提出卡同上。
4. **不批**：只标 pending。
5. **建议**：**approve**。series 只有亮度，墙钟派生；body/坐标仍逐字节比。

### rules[7] — ident 52 `SM_ABILITY`，`ability_layout` + `head_compare:[recog]`（tier L）

1. **动作**：按 18 个语义字段比 40B(oracle)/58B(client) 布局对；`recog=Gold` 从全局掩码取回。
2. **内容**：无“掩码”；布局表从 `tools/loginfixture/capture_login_compare.py:74`（`ABILITY_LAYOUTS`）/`:96`（`ABILITY_FIELDS`）导入，不在比较器抄第二份。
3. **依据（已核）**：`SM_ABILITY` 发点 `MakeMessage(SM_ABILITY, Gold, MakeWord((byte)Job,99), LoWord(GameGold), HiWord(GameGold))` + `EDCode.EncodeMessage(WAbil)`（`PlayObject.Message.cs:1673-1674`）。提出卡：`openmir2-conformance-harness`（初始表即含 52，review `d280be71`/`58f1056b`）。
4. **不批**：只标 pending。
5. **建议**：**approve**。牙：语义字段改 hp ⇒ rc1；线缆 54/78 归一后语义全等 ⇒ rc0 且登记 `ability_layout`；未登记长度对 ⇒ rc2（fail-closed）。

### rules[8] — ident 658 `SM_SENDNOTICE`，`compare`（tier L）

1. **动作**：无掩码。
2. **内容**：recog 走全局；param/tag/series/body 全比。
3. **依据（已核）**：`SM_SENDNOTICE=658`（`Messages.cs:320`）；下行 `SendDefMessage(SM_SENDNOTICE, 2000, 0,0,0, sNoticeMsg…)`（`PlayObject.cs:1076`）⇒ recog 是常量 2000，无语义被全局掩码吞掉。提出卡 `openmir2-conformance-harness`；`selftest` 的“改一个字节 ⇒ rc1 且定位 ident 658 + 偏移”用例就是它（`selftest_impl.py:217-243`）。
4. **不批**：只标 pending。
5. **建议**：**approve**。

### rules[9] — ident 42 `SM_USERNAME`，`compare`（tier L）

1. **动作**：无掩码。
2. **内容**：recog=ActorId（全局掩）；param=颜色码、body=名字逐字节比。
3. **依据（已核）**：`SM_USERNAME=42`（`Messages.cs:203`）；下行 `MakeMessage(SM_USERNAME, processMsg.ActorId, GetChrColor(baseObject), 0, 0)`（`PlayObject.Message.cs:1663`）。提出卡 `openmir2-conformance-harness`。
4. **不批**：只标 pending。
5. **建议**：**approve**（recog 是 ActorId，不需要 head_compare；颜色/名字仍比）。

### rules[10] — ident 54 `SM_MAPDESCRIPTION`，`compare`（tier L）

1. **动作**：无掩码（`recog` 走全局）。
2. **内容**：`recog=nMusicid`、`body=MapDesc`、param/tag/series=0。
3. **依据（已核）**：`SM_MAPDESCRIPTION=54`（`Messages.cs:212`）；下行 `SendDefMessage(SM_MAPDESCRIPTION, nMusicid, 0,0,0, Envir.MapDesc)`（`PlayObject.Base.cs:894`），nMusicid 见 `:889-893`（`Envir.Flag.MusicId`，无音乐为 -1）。提出卡 `openmir2-conformance-harness`。
4. **不批**：只标 pending。**不批不会 rc2，但按现规则判绿会吞掉音乐号差异**。
5. **建议**：**改后再批**。`recog` 是地图音乐号（确定性内容），全局 recog 掩码会把它吞掉；加 `head_compare:["recog"]` 后重新跑 selftest（52/520/615 同型先例）。这是本包抓到的 7 条系统性缺陷之一。

### rules[11] — ident 5008 `SM_GAMEGOLDNAME`，`compare`（tier L）

1. **动作**：无掩码。
2. **内容**：`recog=GameGold`、`param/tag=GamePoint 低/高字`、`body=名称文本`。
3. **依据（已核）**：`SM_GAMEGOLDNAME=5008`（`Messages.cs:434`）；下行 `SendDefMessage(SM_GAMEGOLDNAME, GameGold, LoWord(GamePoint), HiWord(GamePoint), 0, sMsg)`（`PlayObject.Base.cs:938`）。提出卡 `openmir2-conformance-harness`。
4. **不批**：只标 pending；按现规则判绿会吞掉 **GameGold 金额**。
5. **建议**：**改后再批**，加 `head_compare:["recog"]`（param/tag 已在比，body 文本已在比）。

### rules[12] — ident 1106 `SM_VERSION_FAIL`，`compare`（tier L）

1. **动作**：无掩码。
2. **内容**：`recog=ClientFile1_CRC`、`param/tag=ClientFile2_CRC 低/高字`、body=`"<<<<<<"`。
3. **依据（已核）**：`SM_VERSION_FAIL=1106`（`Messages.cs:400`）；下行 `MakeMessage(SM_VERSION_FAIL, ClientFile1_CRC, LoWord(ClientFile2_CRC), HiWord(ClientFile2_CRC), 0)` + `SendSocket(ClientMsg, "<<<<<<")`（`PlayObject.Message.cs:1690-1691`）。提出卡 `openmir2-conformance-harness`。
4. **不批**：只标 pending；按现规则会吞掉 **CRC1**。
5. **建议**：**改后再批**，加 `head_compare:["recog"]`。

### rules[13] — ident 752 `SM_SUBABILITY`，`compare`（tier L）

1. **动作**：无掩码。
2. **内容**：**无 body**；`recog=MakeLong(MakeWord(AntiMagic,0),0)`、`param=MakeWord(HitPoint,SpeedPoint)`、`tag=MakeWord(AntiPoison,PoisonRecover)`、`series=MakeWord(HealthRecover,SpellRecover)`。
3. **依据（已核）**：`SM_SUBABILITY=752`（`Messages.cs:366`）；下行 `SendDefMessage(SM_SUBABILITY, MakeLong(MakeWord(AntiMagic,0),0), MakeWord(HitPoint,SpeedPoint), MakeWord(AntiPoison,PoisonRecover), MakeWord(HealthRecover,SpellRecover))`（`PlayObject.Message.cs:1675`、`:1935`）。提出卡 `openmir2-conformance-harness`。
4. **不批**：只标 pending；按现规则会吞掉 **AntiMagic**，且 reason 说的“body 逐字节比”**不成立**（此帧无 body）。
5. **建议**：**改后再批**，加 `head_compare:["recog"]` 并把 reason 改成“header-only，AntiMagic 取回比、其余三头部字段比”。

### rules[14] — ident 811 `SM_ADJUST_BONUS`，`compare`（tier L）

1. **动作**：无掩码。
2. **内容**：`recog=BonusPoint`、body=3×20B 分段（`BonusAbil / 目标 BonusAbil / NakedAbil`，`PlayObject.Base.cs:1200-1208`）。
3. **依据（已核）**：`SM_ADJUST_BONUS=811`（`Messages.cs:394`）；下行 `MakeMessage(SM_ADJUST_BONUS, BonusPoint, 0,0,0)` + `SendSocket(ClientMsg, sSendMsg)`（`PlayObject.Base.cs:1208-1209`）。提出卡 `openmir2-conformance-harness`。
4. **不批**：只标 pending；按现规则会吞掉 **BonusPoint**。
5. **建议**：**改后再批**，加 `head_compare:["recog"]`（body 分段仍逐字节比）。

### rules[15] — ident 766 `SM_AREASTATE`，`compare`（tier L）

1. **动作**：无掩码。
2. **内容**：**无 body**；`recog=n8` 就是区域状态位（`FightZone` bit0、`SafeArea` bit1、`InGuildWarArea` bit2，`PlayObject.Base.cs:274-285`），param/tag/series=0。
3. **依据（已核）**：`SM_AREASTATE=766`（`Messages.cs:355`）；下行 `SendDefMessage(SM_AREASTATE, n8, 0,0,0)`（`PlayObject.Base.cs:285`）。提出卡 `openmir2-conformance-harness`。
4. **不批**：只标 pending；**按现规则该帧实际“什么都不比”**（非 recog 字段全 0），区域状态差异会假绿。
5. **建议**：**改后再批**，加 `head_compare:["recog"]`。这是本包最严重的“空判据”之一。

### rules[16] — ident 11029 `SM_SERVERCONFIG`，`compare`（tier L）

1. **动作**：无掩码。
2. **内容**：`recog=0`、body=服务端配置文本。
3. **依据（已核）**：`SM_SERVERCONFIG=11029`（`Messages.cs:433`）；下行 `SendDefMessage(SM_SERVERCONFIG, 0,0,0,0, sMsg)`（`PlayObject.Base.cs:969`）。提出卡 `openmir2-conformance-harness`。
4. **不批**：只标 pending。
5. **建议**：**approve**（recog 常量 0，body 文本全比）。

### rules[17] — ident 213 `SM_ATTACKMODE`，`compare`（tier L）

1. **动作**：无掩码。
2. **内容**：**无 body**；`recog=(byte)AttatckMode`，param/tag/series=0。
3. **依据（已核）**：`SM_ATTACKMODE=213`（`Messages.cs:227`）；下行 `SendDefMessage(SM_ATTACKMODE, (byte)AttatckMode, 0,0,0)`（`PlayObject.cs:1146`）。提出卡 `openmir2-conformance-harness`。
4. **不批**：只标 pending；**按现规则该帧实际“什么都不比”**，攻击模式差异会假绿。
5. **建议**：**改后再批**，加 `head_compare:["recog"]`。

### rules[18] — ident 100 `SM_SYSMESSAGE`，`multiset_by_tick`（tier L）

1. **动作**：同 ident 连续段内按 `(param,tag,series,body_hex,phase)` 多重集比；顺序不同集合相同 ⇒ rc0 并登记 `masked_order`；集合不同 ⇒ rc1 并逐条给缺失/多余帧。
2. **掩码内容**：只放松“段内先后顺序”，**帧不剔除**；`tick_tolerance_ms=0`，`t_ms` 只作证据不作判定边界。
3. **依据（已核）**：Queue 入队 `MsgQueue.Enqueue(sendMessage, (byte)wIdent)`（`src/M2Server/Actor/BaseObject.Message.cs:94`）；`SM_SYSMESSAGE=100`（`Messages.cs:215`）；提示发点 `PlayObject.cs:1347-1372`（7 个攻击模式提示 + `:1369` StartChangeAttackModeHelp + `:1372` StartNoticeMsg）；`.NET PriorityQueue` 同优先级出队序未定义；首跑 4 条实测无墙钟（`docs/conformance-first-dual-run-20261023.md` §2.2）。提出卡：`openmir2-conformance-denoise-v2` issue `a7a276b3`；patch `3f84802d`；review `b641c71d`（reviewer-2，request_changes）；返工 patch `a66b6795`；review **`5e9d5132`（reviewer-2，approve，L）**。
4. **不批**：只标 pending。
5. **建议**：**approve**。牙：顺序颠倒 ⇒ 0 + `masked_order`；少一条 ⇒ 1 + 缺失帧；跨别的 ident 绝不合并；缺 `t_ms` ⇒ 2；只此一条用它。

### rules[19] — ident 520 `SM_QUERYCHR`，`compare + head_compare:[recog]`（tier L）

1. **动作**：无掩码；recog 取回比较。
2. **内容**：`recog=nChrCount`（协议内容）、body=角色列表文本；tag/series 照常比。
3. **依据（已核）**：`SM_QUERYCHR` 下行 `MakeMessage(SM_QUERYCHR, nChrCount, 0, 1, 0)`（`src/DBSrv/Services/Impl/UserService.cs:488`）；Rust 同形 `mir2-rs/bins/db-srv/src/handler.rs:365`。提出卡：`openmir2-conformance-harness` 第二批（patch `9f03bd77`，review **`a49618a5`**（reviewer-3，approve，L））。
4. **不批**：只标 pending。
5. **建议**：**approve**（牙：recog 不同 ⇒ rc1，证明 `head_compare` 真取回）。

### rules[20] — ident 525 `SM_STARTPLAY`，`compare`（tier L）

1. **动作**：无掩码。
2. **内容**：body=GameGate ip/port 文本（同端口方案必须逐字节相同）；recog/param/tag/series 全 0。
3. **依据（已核）**：`MakeMessage(SM_STARTPLAY, 0, 0, 0, 0)`（`UserService.cs:719`）。提出卡同上（`a7993b0d5`/review `a49618a5`）。
4. **不批**：只标 pending。
5. **建议**：**approve**（可选按 reviewer-3 `a49618a5` 的 nit 加 `head_compare:["recog"]`，更严；不加也不掩盖真差异）。

### rules[21] — ident 529 `SM_PASSOK_SELECTSERVER`，`compare`（tier L）

1. **动作**：无掩码。
2. **内容**：body=服务器名/状态；series=列表条数；recog 走全局。
3. **依据（已核）**：`MakeMessage(SM_PASSOK_SELECTSERVER, 0, 0, 0, ServerNameList.Count)`（`src/LoginSrv/Services/ClientSession.cs:271`）；首跑真差异 `热血传奇/Idle/` vs `热血传奇/0/`（`docs/conformance-first-dual-run-20261023.md` §2.1）**故意保留**为 body 差异。提出卡同上。
4. **不批**：只标 pending。
5. **建议**：**approve**。这条的价值正是“不掩真差异”；可选加 `head_compare:["recog"]`（当前两侧恒 0）。

### rules[22] — ident 530 `SM_SELECTSERVER_OK`，`mask_body_ranges [[20,null]]`（tier L）

1. **动作**：`ranges=[[20,null]]`（线缆空间）；两侧 body 等长才生效，长度一变退回整段 diff。
2. **内容**：只掩线缆 `[20, end)` = 明文“最后一个 `/` + SessionID”（+ 尾组余字节）；ip 与端口每一位仍逐字节比；recog 走全局。
3. **依据（已核）**：`SendGateMsg(... EDCode.EncodeMessage(defMsg) + EDCode.EncodeString(sSelGateIp + "/" + nSelGatePort + "/" + userInfo.SessionID))`（`ClientSession.cs:520-522`；reason 引 `:520` 指的是 `MakeMessage`，`EncodeString` 实际在 `:522`，见 §9）；EDCode 映射 `EncryptUtil.cs:108-140`；双跑实测 SessionID `…/17100/3` vs `…/17100/2`（`docs/window-529-dualrun-20261023.md:103`）。提出/返工：card `openmir2-conformance-rng-nondeterminism`（patch `e931c3a8`；review `8a59cd6c` F1 blocking：初版 `[[16,null]]` 掩掉端口后三位、全假绿；修后 review `46e479fc` approve R；文本订正 `2f4b311f` approve L）。
4. **不批**：只标 pending。
5. **建议**：**approve**。牙：端口任一位改 ⇒ rc1（offset 13/14/16/17/18）、ip 首末字节改 ⇒ rc1、SessionID 改 ⇒ rc0 + `mask_body_ranges(first_offset=21)`、长度变 ⇒ rc1。

### rules[23] — ident 654 `SM_CHANGELIGHT`，`compare`（tier L）

1. **动作**：无掩码。
2. **内容**：header-only；param=Light、tag=nClientKey、series=0；recog=ActorId 走全局。
3. **依据（已核）**：`SendDefMessage(SM_CHANGELIGHT, processMsg.ActorId, baseObject.Light, (short)Config.nClientKey, 0)`（`PlayObject.Message.cs:1840`）；Rust `enter.rs:711` 注释同形。提出卡同上。
4. **不批**：只标 pending。
5. **建议**：**approve**（Light 目前真机未见差异；若出现差异，按 46/50/51 样式窄掩并补依据，不得整帧 ignore——reason 已写）。

### rules[24] — ident 657 `SM_CHARSTATUSCHANGED`，`compare`（tier L）

1. **动作**：无掩码。
2. **内容**：header-only；param/tag=状态位低/高字、series=HitSpeed；recog 走全局。
3. **依据（已核）**：`SendDefMessage(SM_CHARSTATUSCHANGED, processMsg.ActorId, LoWord(nParam1), HiWord(nParam1), HitSpeed)`（`PlayObject.Message.cs:1846`）。提出卡同上。
4. **不批**：只标 pending。
5. **建议**：**approve**（同一库基线两次应相同；差异出现时窄掩并补依据）。

### rules[25] — ident 11 `SM_WALK`，`compare + head_mask:[series]`（tier L）

1. **动作**：掩 `series`（`MakeWord(Dir, Light)`）；param/tag（目的地 x/y）与 body 逐字节比。
2. **内容**：见上。
3. **依据（已核）**：`SM_WALK=11`（`Messages.cs:165`）；下行 `case Messages.RM_WALK: if (processMsg.ActorId != ActorId) MakeMessage(SM_WALK, ActorId, nParam1, nParam2, MakeWord(wParam, Light))`（`PlayObject.Message.cs:1403-1410`，`if` 实际在 `:1405`）；怪物游走 Dir `RandomByte(8)`（`MonsterObject.cs:210` 等）；随机源 `RandomNumber.cs:35`；Light 无时间成分（`PlayObject.cs:5404-5430`）。提出/返工：card `openmir2-conformance-rules-idents` issue `e4c2eaff`；patch `7f909d9b`；review `b63fdaed`（reviewer-14，request_changes，L）；返工 patch `65a0d43d`；review **`c3f5e854`（reviewer-3，approve，L）**。
4. **不批**：只标 pending；场景 `move_walk` 会因缺规则 UNCLASSIFIED（规则存在则不会）。
5. **建议**：**approve**。残余如实：单客户端窗口里 11/13 **只可能是别人**（自过滤），玩家自身走位方向没有帧级判据（需第二观察者）；历史录制 0 条含 11/13 ⇒ 尚无真实录制验证（reviewer-14/3 均确认）。Light 另有 654 的 param 独立判据。

### rules[26] — ident 13 `SM_RUN`，`compare + head_mask:[series]`（tier L）

1. **动作**：同 11。
2. **内容**：同 11（`SM_RUN=13`）。
3. **依据（已核）**：`Messages.cs:167`；`PlayObject.Message.cs:1423-1430`（`if` 在 `:1425`）；其余同 11。提出/评审同 rules[25]。
4. **不批**：同 rules[25]。
5. **建议**：**approve**（残余同 11）。

### rules[27] — ident 200 `SM_ADDITEM`，`bagitems_records`（tier R）

1. **动作**：按 124B `ClientItem` 记录逐条比；`record_mask=["MakeIndex"]`（偏移 100..104）。
2. **内容**：只掩 MakeIndex；Name@0、StdMode@15、AniCount@18、Dura@104、DuraMax@106、Desc@108 等逐字节比；解码失败/长度非 124 ⇒ UNDECIDABLE。
3. **依据（已核）**：`SM_ADDITEM=200`（`Messages.cs:220`）；下行 `MakeMessage(SM_ADDITEM, ActorId, 0, 0, 1)` + `SendSocket(ClientMsg, EDCode.EncodeBuffer(clientItem))`（`PlayObject.Base.cs:226-227`；reason 写 `:225-227` 的区间包含它）；`GetItemNumber() = ItemNumber++ + GetTickCount()`（`M2Share.cs:547-555`）；新角色初始四件 `PlayObject.cs:1206-1245`（reason 写 `:1201-1240`，IsNewHuman 实际在 `:1206`）；DB 恢复 `PlayDataStorage.Query.cs:404-425`（MakeIndex 在 `:416` 读库）。布局：`ClientItem.cs:25-88` + `ItemNameLen=14` ⇒ MakeIndex@100、总长 124B（reviewer-14 在 `b63fdaed` 独立核过）。提出/返工：card `openmir2-conformance-rules-idents`（patch `7f909d9b`；review `b63fdaed` RC；返工 `65a0d43d`；approve `c3f5e854`）。
4. **不批**：只标 pending；R 级窗口的背包判据用未复核规则。
5. **建议**：**approve**。牙：只 MakeIndex 不同 ⇒ 0+`bagitems_masked_field`；Name 改一字节 ⇒ 1；条数不同 ⇒ 1；解码失败 ⇒ 2。独立判据：DB 对账覆盖 `characters_bagitem.MakeIndex`（持久化路径）。

### rules[28] — ident 201 `SM_BAGITEMS`，`bagitems_records`（tier R）

1. **动作**：逐项 124B 记录 + `'/'`；`record_mask=["MakeIndex"]`；series=背包数照常比。
2. **内容**：同 200；条数不同 ⇒ rc1。
3. **依据（已核）**：`SM_BAGITEMS=201`（`Messages.cs:221`）；下行 `ClientQueryBagItems()`（`PlayObject.Operate.cs:38-64`；`MakeMessage(SM_BAGITEMS, ActorId, 0, 0, ItemList.Count)` 在 `:61`）；MakeIndex 不可复现性同 200。布局同 200。提出/评审同 rules[27]。
4. **不批**：只标 pending。
5. **建议**：**approve**（同 200）。

### rules[29] — ident 615 `SM_TAKEON_OK`，`compare + head_compare:[recog,param]`（tier L）

1. **动作**：无掩码；recog/param 取回比。
2. **内容**：recog=外观 long、param=FeatureEx（装备派生确定性值）、tag/series=0、无 body。
3. **依据（已核）**：`SM_TAKEON_OK=615`（`Messages.cs:284`）；下行 `SendDefMessage(SM_TAKEON_OK, GetFeatureToLong(), GetFeatureEx(), 0, 0)`（`PlayObject.Operate.cs:458`）。提出/评审同 rules[27]。
4. **不批**：只标 pending。
5. **建议**：**approve**（若不加 `head_compare`，全局 recog 会吞掉外观 long——这条做对了）。

### rules[30] — ident 616 `SM_TAKEON_FAIL`，`compare + head_compare:[recog]`（tier L）

1. **动作**：无掩码；recog 取回比。
2. **内容**：recog=n18=失败码（-1/-4 等）、其余 0、无 body。
3. **依据（已核）**：`SM_TAKEON_FAIL=616`（`Messages.cs:285`）；下行 `if (n18 <= 0) SendDefMessage(SM_TAKEON_FAIL, n18, 0,0,0)`（`PlayObject.Operate.cs:493-496`，`SendDefMessage` 在 `:495`）。提出/评审同 rules[27]。
4. **不批**：只标 pending。
5. **建议**：**approve**（失败码取回是必须的；selftest 有“recog 不同 ⇒ rc1”的牙）。

### rules[31] — ident 635 `SM_EAT_OK`，`compare`（tier L）

1. **动作**：无掩码。
2. **内容**：四头部全 0、无 body；出现与否即判据。
3. **依据（已核）**：`SM_EAT_OK=635`（`Messages.cs:297`）；下行 `SendDefMessage(SM_EAT_OK, 0,0,0,0)`（`PlayObject.Operate.cs:690`）。提出/评审同 rules[27]。
4. **不批**：只标 pending。
5. **建议**：**approve**。

### rules[32] — ident 636 `SM_EAT_FAIL`，`compare`（tier L）

1. **动作**：无掩码。
2. **内容**：同 635（全 0、无 body）。
3. **依据（已核）**：`SM_EAT_FAIL=636`（`Messages.cs:298`）；下行 `PlayObject.Operate.cs:698`。提出/评审同 rules[27]。
4. **不批**：只标 pending。
5. **建议**：**approve**。

### rules[33] — ident 643 `SM_MERCHANTSAY`，`compare`（tier L）

1. **动作**：无掩码。
2. **内容**：recog=ActorId（全局掩）、param=0、tag=0、series=1、body=脚本文本。
3. **依据（已核）**：`SM_MERCHANTSAY=643`（`Messages.cs:304`）；`NormNpc.cs:1615` `PlayObject.SendMsg(this, RM_MERCHANTSAY, …)` → `PlayObject.Message.cs:1723-1729` `MakeMessage(SM_MERCHANTSAY, ActorId, MakeWord(nParam1,nParam2), 0, 1)` + `EDCode.EncodeString(Msg)`。真实录制先例 `E:/tmp/conformance/runs/dice767-fix`（两侧各 3 条）。提出/评审同 rules[27]。
4. **不批**：只标 pending。
5. **建议**：**approve**。

### rules[34] — ident 644 `SM_MERCHANTDLGCLOSE`，`compare`（tier L）

1. **动作**：无掩码。
2. **内容**：recog=商人 ActorId（全局掩）、其余 0、无 body。
3. **依据（已核）**：`SM_MERCHANTDLGCLOSE=644`（`Messages.cs:306`）；`PlayObject.Message.cs:1806-1808` `SendDefMessage(SM_MERCHANTDLGCLOSE, nParam1, 0,0,0)`。提出/评审同 rules[27]。
4. **不批**：只标 pending。
5. **建议**：**approve**（recog 是 ActorId，无被吞语义）。

### rules[35] — ident 645 `SM_SENDGOODSLIST`，`compare`（tier L）

1. **动作**：无掩码。
2. **内容**：recog=商人 ActorId（掩）、param=n10=商品条目数、tag/series=0、body=商品列表文本。
3. **依据（已核）**：`SM_SENDGOODSLIST=645`（`Messages.cs:307`）；`Merchant.cs:980-992` 拼 `名字/子菜单/价格/库存/` 并 `n10++`，`:992 user.SendMsg(RM_SENDGOODSLIST, 0, ActorId, n10, 0, sSendMsg)` → `PlayObject.Message.cs:1809-1811`。提出/评审同 rules[27]。
4. **不批**：只标 pending。
5. **建议**：**approve**。

### rules[36] — ident 763 `SM_BUILDGUILD_FAIL`，`compare + head_compare:[recog]`（tier L）

1. **动作**：无掩码；recog 取回比。
2. **内容**：recog=失败码、其余 0、无 body。
3. **依据（已核）**：`SM_BUILDGUILD_FAIL=763`（`Messages.cs:376`）；`PlayObject.Message.cs:1940-1942` `SendDefMessage(SM_BUILDGUILD_FAIL, nParam1, 0,0,0)`。提出/评审同 rules[27]。
4. **不批**：只标 pending。
5. **建议**：**approve**（与 616 同型，失败码必须取回）。

### rules[37] — ident 767 `SM_MENU_OK`，`compare`（tier L）

1. **动作**：无掩码。
2. **内容**：recog=nParam1=ActorId（全局掩）、param/tag/series=0、body=菜单文本。
3. **依据（已核）**：`SM_MENU_OK=767`（`Messages.cs:379`）；`PlayObject.Message.cs:1952-1954` `SendDefMessage(SM_MENU_OK, nParam1, 0,0,0, Msg)`。真实录制 `dice767-fix` 两侧各 1 条。注意 767=SM_MENU_OK，**不是** 769=SM_GUILDMAKEALLY_FAIL（runbook 记过 ident 769 事故，reason 已写）。提出/评审同 rules[27]。
4. **不批**：只标 pending。
5. **建议**：**approve**。

---

## 6. 证据汇总：每条规则的提出卡与评审 oid

| 规则组 | 提出卡（issue oid） | patch | 评审 oid（verdict/tier） | merge |
| --- | --- | --- | --- | --- |
| 初始 15 条（10/41/42/46/50/51/52/54/213/658/752/766/811/1106/5008/11029） | `openmir2-conformance-harness`（`fc388535`） | `bddd5ecd`；修复 `f2c7050e` | `d280be71`（reviewer-3，request_changes，L）→ `58f1056b`（reviewer-3，approve，L） | `9e72e2287` |
| 首跑 7 条（100/520/525/529/530/654/657，含 52 线缆） | 同上（第二批） | `9f03bd77` | `a49618a5`（reviewer-3，approve，L） | `5d7e3556a` |
| 14 / 10.series / 530 末段 | `openmir2-conformance-rng-nondeterminism`（`0f0db14e`） | `e931c3a8`；返工 `e1aa9127`；文本 `07110f7d` | `8a59cd6c`（reviewer-2，request_changes，R）→ `46e479fc`（reviewer-2，approve，**R**）→ `2f4b311f`（reviewer-2，approve，L） | `017352a60`、`8d53ff544` |
| 100 multiset | `openmir2-conformance-denoise-v2`（`a7a276b3`） | `3f84802d`；返工 `a66b6795` | `b641c71d`（reviewer-2，request_changes，L）→ `5e9d5132`（reviewer-2，approve，L） | `be9c5ac6b` |
| 13 条（11/13/200/201/615/616/635/636/643/644/645/763/767） | `openmir2-conformance-rules-idents`（`e4c2eaff`） | `7f909d9b`；返工 `65a0d43d` | `b63fdaed`（reviewer-14，request_changes，L）→ `c3f5e854`（reviewer-3，approve，L） | `6665e1178` |
| 31 | `openmir2-conformance-rule-sm-struck-31`（`b050ea8f`） | `1782f1d1`；返工 `d0a5ece8` | `926d1b67`（reviewer-1，request_changes，L）→ `38b40efc`（reviewer-1，approve，L） | `57302d3df` |

**关键**：以上 review 批准的是**草稿/工具片**，没有任何一条把 `approved_by` 从 PENDING 改掉。reviewer-3 在 `c3f5e854` 的 `merge_conditions` 明写：合并后由 coordinator/owner 收口「① 13 条新规则的 `approved_by` 复核；② ident 14 的 ignore 批准（或按 §5.1(b) 显式 `--ignore-idents` 并每次记录）」。reviewer-2 在 `46e479fc`/`2f4b311f` 明写「本 approve 不构成对该变更的授权」。

---

## 7. 批准权限与流程（不替任何人宣布已批准）

**权限结论：`approved_by` 只能由 owner/coordinator 级签署；reviewer 的 `approve` 是技术评审，不是规则批准。**

依据（均为已签名条目，可重放）：

1. `rules.json` 的 `note`：「`approved_by` 以 PENDING 开头的条目尚未经 **owner/coordinator** 复核」。
2. reviewer-2 `46e479fc`：`f2_judgement.conditions` ——「`approved_by` 必须保持 PENDING，直到 `openmir2-castle-merchant-fixedhide-hit` 落地或计数/形态比较机制就位；**本 review 不授权去掉 PENDING**」。
3. reviewer-2 `2f4b311f`：`approve_conditions_confirmed.ident_14_pending` ——「**本 approve 不构成对该变更的授权**」。
4. reviewer-3 `c3f5e854`：`open_items_judgement` ——「13 条批准 / ident 14 裁决 … 是 **owner 放行决定**（本卡只登记）」；`merge_conditions` 要求 coordinator/owner 收口。
5. tier 策略（`AGENTS.md`「判据分级」）：R 级片必须真机全量对拍；`rules.json` 里 tier=R 的规则是 **10 / 14 / 200 / 201**。批它们时要么附真机窗口证据，要么由 owner 显式登记“接受残余/待窗口”。

**批准动作的最小形态（建议，不是本卡能执行的）：**

1. 有权限的 principal 在 `tools/conformance/rules.json` 把目标条目的 `approved_by` 改成可核的署名串（如 `owner <principal> 2026-10-23: approved by <basis>`）；
2. **同一次提交**必须同步改 `tools/conformance/selftest_impl.py` 的 PENDING 断言（见 §8.2），否则 selftest 会红；
3. 先跑 §8 的机判命令 + `selftest`；
4. 走 review（规则 = 判据，`tools/conformance/README.md:221`）；R 规则附窗口证据；
5. 用签名 `comment`/`status` 把“批了哪些、依据哪条 review、残余是什么”写进本卡线程（线程是唯一共享记忆）。

---

## 8. 批完后的机判

### 8.1 主判据：`approved_by` 不再含 PENDING

命令（与 §1 同一条）：

```sh
python -I -c "import json;d=json.load(open('tools/conformance/rules.json',encoding='utf-8'));rows=[('global[%d]'%i,r) for i,r in enumerate(d.get('global',[]))]+[('rules[%d]'%i,r) for i,r in enumerate(d.get('rules',[]))]+([('db',d['db'])] if bool(d.get('db')) else []);p=[(w,r.get('ident','-'),r.get('action'),r.get('tier')) for w,r in rows if str(r.get('approved_by','')).upper().startswith('PENDING')];print('PENDING',len(p),'/',len(rows));[print(*x) for x in p]"
```

期望输出（全部批完时）：

```
PENDING 0 / 43
```

若只批 §0 的 33 条（口径 C 的第一批），期望 `PENDING 10 / 43`，且这 10 行恰好是 `rules[0] 10`、`rules[2] 14`、`rules[5] 50`、`rules[10] 54`、`rules[11] 5008`、`rules[12] 1106`、`rules[13] 752`、`rules[14] 811`、`rules[15] 766`、`rules[17] 213`。

### 8.2 配套判据：`selftest` —— 注意它会因为“改了 `approved_by`”而红

```sh
python -I -B tools/conformance/compare_runs.py selftest
```

**现状（未批）**：`GREEN: compare_runs selftest 198/198`，rc=0（本卡实跑）。

**批准后（本卡实测模拟：把全部 43 条改成 `owner 2026-10-23 (approval simulation)`，在临时副本上跑；下面只摘录首尾，实际输出是 22 条 FAILED）**：

```
RED: compare_runs selftest 176/198
  FAILED: ident=100 规则带 reason/tier/approved_by=PENDING
  …（7 条首跑规则）
  FAILED: ident=11/13/200/201/615/616/635/636/643/644/645/763/767 带 reason/tier/approved_by=PENDING
  FAILED: 忽略牙：内置 14 是 PENDING ignore ⇒ 真剔帧时 rc 2（pending_ignore_rule 兜底）
  FAILED: 31 的 reason/tier/approved_by=PENDING 齐全
```

原因：`selftest_impl.py` 有三处显式断言 PENDING —— `:313-317`（7 条首跑规则）、`:480-484`（13 条批次 2）、`:715-718`（31）；另 `:653` 断言“内置 14 是 PENDING ignore ⇒ 真剔帧 rc2”。共 22 条断言会红。

⇒ **批准规则必须与 selftest 断言同步更新同一提交**（改法：把 `startswith("PENDING")` 断言换成“`approved_by` 非空”或显式期望值；`:653` 的 ignore 牙改用临时规则副本构造 PENDING）。这是本包发现的**操作性前置**，不写清会让“批完以后 selftest 假红”。

### 8.3 规则表可解析（任何改动后都要跑）

```sh
python -I -B tools/conformance/compare_runs.py selftest
```

批完 + 同步改断言后，期望仍是 `GREEN: compare_runs selftest 198/198`（或断言数按新增调整后的 GREEN），rc=0。

---

## 9. 行号核验记录（`grep -n` 复核，差异逐条列出）

**总览**：reason 中出现的 57 处 `file:line` 引用，56 处文件存在且行号落在文件范围内；1 处写成简写 `src/downlink.rs`（实际 `mir2-rs/bins/game-svr/src/downlink.rs`）。逐条内容核对后，下列引用有偏移或文案不符：

| 位置 | reason 写法 | 实际 | 性质 |
| --- | --- | --- | --- |
| `rules[22]`（530） | `ClientSession.cs:520` 标 `EDCode.EncodeString(...)` | `:520` 是 `MakeMessage(SM_SELECTSERVER_OK, …)`；`SendGateMsg(... EDCode.EncodeString(sSelGateIp + "/" + nSelGatePort + "/" + userInfo.SessionID))` 在 **`:522`** | 行号偏 2，语义正确 |
| `rules[25]`（11） | “handler 带 `if (processMsg.ActorId != ActorId)`（本文件 `:1403`）” | `:1403` 是 `case Messages.RM_WALK:`；`if` 在 **`:1405`**（SM_RUN `:1423`/`:1425` 同理） | 引用 case 行，`if` 偏移 2 |
| `rules[25]`（11） | “Rust 侧 `bin/game-svr/src/downlink.rs:377-380` 照抄” | `:377-380` 是**文档注释**描述 oracle 口径，不是代码；实际编码器在 `downlink.rs` 的 `walk/run` 帧函数 | 引注释不引代码 |
| `rules[27]`（200） | `PlayObject.Base.cs:225-227` 标 `MakeMessage(SM_ADDITEM,…)` | `MakeMessage` 在 **`:226`**（`:225` 是 `}`） | 区间包含，锚点早 1 行 |
| `rules[27]`（200） | `PlayObject.cs:1201-1240` IsNewHuman 分支 | `if (IsNewHuman)` 在 **`:1206`**，四件初始物品块 `:1206-1245` | 区间偏 5 行 |
| `rules[30]`（616） | `PlayObject.Operate.cs:493-496` 标 `SendDefMessage(SM_TAKEON_FAIL,…)` | `SendDefMessage` 在 **`:495`**，`if (n18 <= 0)` 在 `:493` | 区间包含 |
| `rules[4]`（46） | 只写 “C# `PlayObject.Message.cs` RM_DAYCHANGING”，无行号 | 下行在 **`PlayObject.Message.cs:1741`** | 缺锚点（本包补） |
| 规则 10/14 | “对**所有 actor**（自己/怪物/NPC）生效” | `case Messages.RM_TURN` 的 handler 有 `processMsg.ActorId != ActorId`（`PlayObject.Message.cs:1528`）⇒ **自己不发给自己**；SM_HIT 同（`:1434-1436`） | 文案不实（见 §0） |
| `rules[13]`（752） | “body 逐字节比” | 该帧**无 body**；`recog=AntiMagic` 被全局掩码吞掉 | 文案不实（见 §0） |
| `rules[15]`（766）/`rules[17]`（213） | “body 逐字节比”/“逐项比” | 均**无 body**，`recog` 才是载荷且被掩 | 文案不实（见 §0） |
| `rules[10]`（54）/`rules[11]`（5008）/`rules[12]`（1106）/`rules[14]`（811） | 只写 body/文本逐字节比 | `recog` 也是语义载荷（音乐号/GameGold/CRC1/BonusPoint），被全局掩码吞掉 | 覆盖缺口（见 §0） |

**未找到/无法核实的引用**：无（`src/downlink.rs` 是简写，按 `mir2-rs/bins/game-svr/src/downlink.rs` 核到）。

---

## 10. 与卡面不一致之处 / 需要 owner 知道的事

1. **条数**：卡面写“`rules.json` 现有 38 条规则，其中至少 ident 14、31 等是 PENDING”。实际待批 **43 条** = `rules` 38 + `global` 4 + `db` 1；`rules` 38 条**全部**是 PENDING，不是“至少”。
2. **“PENDING 的 ignore 会直接判 rc=2”**：成立，但只对 `action=ignore` 且**真的剔了帧**时成立（`compare_runs.py:1518-1524`）。全表只有 1 条 ignore（14）；其余 PENDING 只标 `rules_pending_review=true`，不改退出码。
3. **ident 14 不能“顺手批”**：reviewer-2 已在两个签名条目里写下保留条件（`46e479fc`、`2f4b311f`），「本 approve 不构成对该变更的授权」。`openmir2-castle-merchant-fixedhide-hit` 已合入 master `0119e47b9` 且落了 T-192 五条构造级判据，但 R 级真机窗口（⑧ 城堡战切换）未跑、卡片仍 `in-progress`。⇒ 需要 owner 先裁定“落地”口径，或由 owner 明确覆盖 reviewer-2 的条件。
4. **equip-struck-save 复审确实点名了 14**（`8ed0da42` finding #2），但该 review 把它判为 **minor、不阻塞、`window_required=false`**：它说的是“排窗口前先推动 approve 或显式登记”，不是“本卡必须现在批”。
5. **7 条早期 compare 规则有实质判据缺陷**（54/5008/1106/752/811/766/213）：全局 `recog` 掩码吞掉语义字段；其中 213/766 在该掩码下“什么都不比”。这是本包最重要的发现，卡面没有提到。
6. **批完会红 selftest**：`selftest_impl.py` 有三处断言“规则必须是 PENDING”，批准后 `selftest` 从 198/198 变 176/198（本卡实测）。批准提交必须同步改断言，否则门禁假红。
7. **规则 reason 的“对所有 actor（自己/…）”不实**：SM_TURN/SM_HIT/SM_WALK/SM_RUN 的 handler 都带 `processMsg.ActorId != ActorId`，玩家自己收不到这些帧；rules[0] 的残余分析需要据此订正。

---

## 11. 残留物清单（本卡）

- 本卡在工作树 `E:/tmp/wt-worker-66` 内只新增本文档；未改 `rules.json`、`tools/**`、`src/**`、`crates/**`。
- 临时目录：`E:/tmp/worker66-scratch/`（行号/牙模拟用；含 `repo/` 的模拟审批副本与 `citations.txt`）。不属于仓库，未清理，登记在此；如需清理可整目录删除。
- 未起任何服务端/客户端/代理，未连库，未占端口。
- 门禁：`python -I tools/hygiene/conflict_marker_gate.py` rc=0；`git status` 干净（见线程 status 条目）。
