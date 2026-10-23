# 对拍台「参数发现」设计（卡 `openmir2-window-param-discovery`）

- 状态：**已落 `tools/probe/param_discovery.py`**（2026-10-23 返工版，worker-64）。接线（`card_window.py --discover` 等）
  不在本卡写集，另开卡 `openmir2-window-discovery-wiring`。
- 红线（卡 acceptance）：推导不出就 fail-closed（`scenario-error`，rc=2），**绝不**用默认值/猜值；不改任何场景的 `required` 声明。
- 口径纪律（返工后）：**「推导」与「规则」严格分栏**。规则 = 部署经验假设，必须显式标注并写清代价；推导 = 帧语义 + oracle 出处。

## 1. 现状

15 个场景里 9 个声明了 `required` 运行时变量；`--check-scenario` 对它们一律 rc=2（`refusing to send guessed values`）。
`card_window.py` / `run_dual.sh` 目前不传 `--var` ⇒ 参数化 run 起跑即卡住。

## 2. 推导源（每个变量一行：类别 + 来源帧 + 字段 + oracle 出处）

| 变量 | 类别 | 来源 | oracle 出处 / 依据 |
|---|---|---|---|
| `self_x` / `self_y` | **推导** | `SM_LOGON(50)` 的 `param` / `tag` | `src/M2Server/Player/PlayObject.cs:1128-1131` `SendLogon()` 的 `Messages.MakeMessage(SM_LOGON, ActorId, CurrX, CurrY, HUtil32.MakeWord(Dir, Light))`（param=CurrX、tag=CurrY、recog=ActorId）。实测一致性（`50 recog=191 param=7 tag=12` ↔ DB `characters` `B013 CX=7 CY=12`）只作**佐证**，不是出处 |
| `merchant_actor` | **推导** | `SM_MERCHANTSAY(643)` 的 `recog` | 643 是 NPC **应答点击**才发出的说词面板：`NormNpc.Click → GotoLable(@main) → RM_MERCHANTSAY`（`src/M2Server/Npc/NormNpc.cs:68-73,1615`；`src/M2Server/Player/PlayObject.Message.cs:1721-1724`）。点击入口 `PlayObject.Base.cs:674-695 ClientClickNpc` = `FindMerchant(actorId) ?? FindNpc(actorId)` |
| `npc_actor` | **推导**（有 643 时）/ 退化为**规则** | 优先 `SM_MERCHANTSAY(643).recog`；无 643 时 `SM_TURN(10)` 首个 `recog != 自身` | 643 同上一行（实证：该 actor 应答了点击）。退化规则唯一依据是 `PlayObject.Message.cs:1542` `MakeMessage(SM_TURN, ActorId, …)`——那只说明「这个 actor 转了身」，**不说明它是什么**。代价见下方「规则代价」 |
| `king_actor` | **不可推导** | 帧里没有「该 actor 是国王」的信息 | fail-closed（若国王 NPC 进视野，`npc_actor` 的 643/规则可能先取到它，但本模块不拿它冒充 `king_actor`） |
| `make_index` / `item_name` | **不可推导（本版）** | 需 `SM_BAGITEMS(201)` 的包体 | 现网 22+ 个 run **无 201** ⇒ `bag_equip` / `eat_item` **恒红直到有 201 录制**。本版**不实现** 201 规则（无录制就无法验证解析），另卡 |
| `move_x` / `move_y` | **不可推导** | 已录帧里没有 `SM_SPACEMOVE/SM_WALK`，也没有自身 `SM_TURN` | fail-closed；需新帧或新场景 |
| `target_x` / `target_y` | **不可推导** | 帧里没有目标格坐标 | fail-closed；需新帧或新场景 |
| `takeon_where` / `hit_dir` | **不可推导** | 无来源帧 | fail-closed（可由操作者 `--var` 显式给） |

### 2.1 `merchant_actor` 为什么必须换掉 `SM_TURN` 规则（返工 blocking-1）

旧实现取「首个 `recog != 自身` 的 `SM_TURN`」当商人。这是**部署经验假设**而非帧语义，两个后果：

1. **任意第三方 actor 会被当商人**：夹具（`SM_LOGON` + 自身 turn + 第三方 actor 777，无 643）在旧实现下得到
   `npc_actor=merchant_actor=777`、`check-all` rc=0。现已改为 fail-closed（`merchant_actor` missing，rc=2），
   并固化为一颗回归牙（牙③）。
2. **会静默通过**：`merchant_dialog` 末步 `wait_for [645,643,100]` 对「点了非商人 NPC ⇒ `@main` 回 643 / `@buy` 不存在回 643/100」
   也成立 ⇒ 场景「绿」但没验证商人语义。

**选用的修法**：`merchant_actor` 改为从 `SM_MERCHANTSAY(643).recog` 推导（Leader 裁决给出的选项 (a)），理由：

- 643 是**该 actor 真的应答了本次点击**的实证（而不是「站在那儿并转了身」），任意第三方 actor 不再可能冒充；
- 与 `npc_click`/`npc_click_auto` 已有的 `auto:last_turn` 语义兼容：agent 先点一次可见 NPC，再读 643 的 `recog`；
- **代价（必须登记）**：643 只证明「这是一个应答点击的脚本 NPC」，**不证明**「它是有 `@buy` 的商人」。
  `merchant_dialog` 末步 `[645,643,100]` 的宽松等待仍在，非商人 NPC 仍可能让它「过」。收紧该等待
  （如只等 645，或断言 643 正文含商人特征）**不属本卡写集**（不许改场景 JSON），另开卡跟进。
- **代价 2**：登录期收帧（`login_only`）里没有 643 ⇒ `merchant_actor` fail-closed。所以 discovery run 必须
  **点一次 NPC**（`auto:last_turn`）才能收 643；这条要求写进接线卡（§5）。

### 2.2 `npc_actor` 的规则代价（明标，不冒充推导）

没有 643 时 `npc_actor` 退化为 `SM_TURN` 规则，输出里 `kind="rule"`，来源帧/字段/seq 全量落盘，便于复审区分。
代价：若首个可见 actor 不是目标 NPC，`npc_click` 会**响亮失败**（`wait_for 643` 超时，
实测 `w24-csharp-npc_click` 的 `run.json` 即 `exit.code=1 step-failed`），不存在静默通过。
`dice_chain` 的 `params.npc_actor=119` 硬编码猜值属**另一个问题**（见 §6.2），本卡不改场景 JSON。

## 3. 发现阶段流程（已落 `tools/probe/param_discovery.py`）

```text
discovery run（收帧：login_only 段 + 一次 auto:last_turn 点击）
    → 读 frames.ndjson
    → derive(required_vars)：
         每个变量查「推导表」(RULES)；命中 ⇒ 记 (值, kind, rule, oracle, source{ident,field,seq})
         否则查「规则表」(RULES_FALLBACK, kind=rule)；命中 ⇒ 同上但明确标 rule
         否则查 UNDERIVABLE ⇒ 记 missing（带原因）
    → missing 非空 ⇒ 直接 fail-closed（rc=2），逐个点名
    → 全部命中 ⇒ 生成 --var 列表交给后续场景
```

- **两侧各自发现**：Rust 与 C# 的 ActorId 每会话不同 ⇒ 各自跑 discovery、各用自己的值；比较器已默认掩码 `recog`
  （`tools/conformance/compare_runs.py:25` + `rules.json` global[0]，`:321-335` 生效），不会把两侧不同的 ActorId 当成差异。
  这是卡里「选哪条口径」的答案与理由。
- **发现值要落盘**：`discovered.json`（`{var: {value, kind, rule, oracle, source:{ident, field, seq}}}`）。
  `kind`/`source` 是复审能否核「两侧点的是不是同一类目标」的关键——接线卡必须原样落盘，不许只落 value。
- 输入健壮性：坏行 / 非 JSON 对象 / 字段非整数 ⇒ `DiscoveryError`，打印 `file:lineno` 并 **rc=2**（不是 traceback、不是 rc=1）。

## 4. 构造级用例与牙（`selftest`：13 用例 + 6 颗牙，全离线）

用例：

1. 含 643 + 含自身坐标 ⇒ `self_x/self_y/npc_actor/merchant_actor` 的**值与来源帧**（ident/field/seq）逐项断言，`--var` 渲染有序；
2. 缺目标 NPC（只有自身 turn）⇒ `npc_actor` missing ⇒ fail-closed；
3. 缺自身坐标（无 `SM_LOGON`）⇒ `self_x/self_y` missing ⇒ fail-closed；
4. 只有自己的 `SM_TURN` ⇒ 不得把自己当 NPC；
5. 无 `SM_BAGITEMS(201)` ⇒ `make_index/item_name` **恒** missing（本版不实现 201 规则）；
6. 同夹具两次跑 ⇒ 同结果（确定性）；
7. 混合：可推导的照样给出、不可推导的逐个点名；
8. `check-all` 接线口径：可推导场景 rc=0（`cls=derived`）/ 缺变量场景 rc=2 且 detail 由**本模块**报 `missing:`（`cls=missing`）；
9. 分类诚实：只有 `params` 默认值的场景（`dice_chain` 形态）算 `params_default`，**不计入**「发现能力已验证」；
10. `--allow-missing`：只放行 `missing`；`error` 行仍 rc=2；`--allow-missing=N` 与实际 missing 数不符也 rc=2；
11. 坏行 ⇒ 干净 rc=2（`DiscoveryError`）；
12. 单场景推导路径的空判据：空 `--var` 列表 / 只有分隔符 ⇒ rc=2（「一个变量都没要求」不算通过）；空 frames + 有 required ⇒ rc=2；
13. 场景分类的 `rule` 分栏（见牙⑥）。

牙（逐字节还原 + sha256 见 §6.3）：

- ① `self_x` 错取 `tag` ⇒ 值从 7 变 12，与用例①断言冲突 ⇒ 红；
- ② **`check_all` 行为断言**：断言缺变量行 `cls == "missing"` 且 detail 以 `missing:` 开头。
  把 `check_all` 的 `if missing:` 改成 `if False:` ⇒ 该行变成 `cls=error` / `scenario-error:` ⇒ 红。
  （旧版这颗牙只断言 `derive()` 的 `missing`，是**空牙**：变异后 selftest 仍全绿。）
- ③ **reviewer blocking-1 回归牙**：只有第三方 actor 转身、**无 643** 的帧 ⇒ `merchant_actor` 必须 missing、
  `check-all` 该场景 rc=2（旧规则此处 rc=0 且值 777）。
- ④ **零场景 fail-open（reviewer C-1）**：`scenarios_dir` 不存在 / 空目录 / 加 `--allow-missing` / `--allow-missing=0`
  ⇒ 一律 rc=2。删掉 `check_all` 的 `isdir` 检查与 `check_all_main` 的 `if not rows: return 2` ⇒ 红。
- ⑤ **场景文件坏掉（reviewer C-2）**：坏 JSON / 顶层数组 / `required` 是字符串 / `required: [123]`
  ⇒ 每行 `cls=error`、detail 以 `scenario-load:` 开头、整体 rc=2（`--allow-missing` 也不放行）。
  把 `_load_scenario` 退回裸 `json.load` ⇒ 红（traceback / 按字符拆 missing）。
- ⑥ **推导与规则分栏（reviewer A-1）**：无 643 时 `npc_actor` 走 `SM_TURN` 规则 ⇒ `cls=rule`；有 643 ⇒ `cls=derived`。
  把 `kind == "rule"` 判定改坏 ⇒ 红。

## 5. 与 `card_window.py` 的接线（另卡 `openmir2-window-discovery-wiring`，不在本卡写集）

- `card_window.py` 增加 `--discover`：先跑 discovery，再把派生值拼进每个场景的 `--var`；
- `run_dual.sh` 两侧各自 discovery（不共享 ActorId），各自落 `discovered.json`；
- **discovery 场景必须点击一次**（`click_npc: "auto:last_turn"`）+ 收 643，否则 `merchant_actor` 永远 fail-closed；
- 接线完成后，`--check-scenario` 的结果必须能用 `check-all ... --allow-missing=N` 机判（N = 不可推导清单长度）。

## 6. 返工实测账目（2026-10-23，worker-64，master `14864309e`）

### 6.1 两条真实录制

```text
$ python -I -B tools/probe/param_discovery.py check-all E:/tmp/conformance/runs/w24-rust-login/frames.ndjson
param_discovery: 7 scenario(s) fail-closed on missing vars （登记后可 --allow-missing=N 放行）
bag_equip.json               rc=2  missing        missing: make_index,takeon_where,item_name
combat_hit.json              rc=2  missing        missing: target_x,target_y,hit_dir
dice_chain.json              rc=0  params_default scenario OK: dice_chain (8 steps, params=npc_actor=119)
dice_chain_auto.json         rc=0  no_required    scenario OK: dice_chain_auto (8 steps, params=-)
eat_item.json                rc=2  missing        missing: make_index,item_name
gm_batch.json                rc=0  no_required    scenario OK: gm_batch (10 steps, params=-)
gm_time.json                 rc=0  no_required    scenario OK: gm_time (2 steps, params=-)
king_castle_war.json         rc=2  missing        missing: king_actor
king_dialog.json             rc=2  missing        missing: king_actor
login_only.json              rc=0  no_required    scenario OK: login_only (1 steps, params=-)
merchant_dialog.json         rc=2  missing        missing: merchant_actor
move_walk.json               rc=2  missing        missing: move_x,move_y
npc_click.json               rc=0  rule           scenario OK: npc_click (2 steps, params=npc_actor=119)
npc_click_auto.json          rc=0  no_required    scenario OK: npc_click_auto (2 steps, params=-)
pickup_item.json             rc=0  derived        scenario OK: pickup_item (2 steps, params=self_x=7,self_y=12)
SUMMARY scenarios=15 ok=8 rc2=7 derived=1 rule=1 no_required=5 params_default=1 missing=7 error=0
rc=2（默认 fail-closed）；`--allow-missing=7` ⇒ rc=0
```

```text
$ python -I -B tools/probe/param_discovery.py check-all E:/tmp/conformance/runs/w24-rust-npc_auto/frames.ndjson
param_discovery: 6 scenario(s) fail-closed on missing vars （登记后可 --allow-missing=N 放行）
merchant_dialog.json         rc=0  derived        scenario OK: merchant_dialog (4 steps, params=merchant_actor=119)
SUMMARY scenarios=15 ok=9 rc2=6 derived=3 rule=0 no_required=5 params_default=1 missing=6 error=0
rc=2（默认）；`--allow-missing=6` ⇒ rc=0
```

**读法**：登录期收帧没有 643 ⇒ `merchant_actor` 正确 fail-closed（这也是旧版拿第三方 actor 蒙过去的那个位置）；
点过一次 NPC 的 run 里，`merchant_actor=119` **由 643 推导**（`source.ident=643, seq=30`），`merchant_dialog` 转绿。

### 6.2 「绿」的分类（`cls`），指标名实相符

| cls | 含义 | 登录期 / 点击 run |
|---|---|---|
| `derived` | required 全部由本模块**推导**（帧语义，**唯一**算「发现能力已验证」的绿） | 1 / 3 |
| `rule` | required 里有变量来自退化**规则**（`SM_TURN` 部署假设，`kind=rule`）⇒ **不计入**发现能力 | 1 / 0 |
| `no_required` | 场景本就不需要变量 | 5 / 5 |
| `params_default` | **绿来自场景 `params` 硬编码默认值，不是发现值** | 1 / 1（`dice_chain`） |
| `missing` | 本模块 fail-closed（不可推导，已登记） | 7 / 6 |
| `error` | 场景本身坏了（`--allow-missing` 也不放行） | 0 / 0 |

`dice_chain` 的绿来自 `params.npc_actor=119`（e0ddbe200 起就有），**不能**计入「发现能力已验证」；
同目录 `dice_chain_auto.json` 自己的注释也写着「庄家 ActorId 每会话不同，用 auto」。把该默认值改成
`required` + 发现值属另卡（本卡禁止改场景 `required`）。

### 6.3 牙的变异实测与逐字节还原

三颗牙都用「源变异 → `selftest` 必须红 → 还原 → sha256 逐字节一致」验证：

| 牙 | 变异 | selftest | 红的用例 |
|---|---|---|---|
| ① | `RULES["self_x"].field`: `param` → `tag` | FAIL rc=1 | ① / ⑦ / 牙① |
| ② | `check_all` 的 `if missing:` → `if False:`（reviewer 的变异 M2） | FAIL rc=1 | ⑧ / ⑩ / 牙②（`cls=error`，detail 变 `scenario-error:`）/ 牙③ |
| ③ | `if var == "npc_actor"` → `if var in ("npc_actor","merchant_actor")`（退回旧规则） | FAIL rc=1 | 牙③（`merchant_actor=777`、`missing=[]` 被抓） |
| ④ | 删掉 `check_all` 的 `isdir` 检查 + `check_all_main` 的 `if not rows: return 2`（reviewer C-1） | FAIL rc=1 | 牙④（空目录/缺目录返回 0） |
| ⑤ | `_load_scenario` 退回裸 `json.load`（reviewer C-2） | FAIL rc=1（`AttributeError: 'list' object has no attribute 'get'`） | 牙⑤（顶层数组/非字符串 required 不再落 `cls=error`） |
| ⑥ | `v["kind"] == "rule"` → `"ruleX"`（reviewer A-1） | FAIL rc=1 | 牙⑥（规则绿被算成 `derived`） |

还原后 `python -I -B tools/probe/param_discovery.py selftest` ⇒ `OK (13 cases + 6 teeth)` rc=0；
`sha256sum tools/probe/param_discovery.py` = `87291f3686057930ce4beed1778833f6979bfb566a1215c5aaa74bad9c7f55ce`，
与提交树 blob 逐字节一致（`git show HEAD:tools/probe/param_discovery.py | sha256sum` 复核）。

### 6.4 与旧版的差异清单（返工）

| # | 项 | 旧 | 新 |
|---|---|---|---|
| 1 | `merchant_actor` | `SM_TURN` 首个非自身（部署假设，任意第三方 actor 可冒充） | `SM_MERCHANTSAY(643).recog`（真应答过点击的 NPC）；无 643 ⇒ missing |
| 2 | 牙② | 空牙（只断言 `derive` 的 missing，变异后 selftest 仍绿） | 对 `check_all` 行为断言（`cls=missing` + `missing:` 前缀） |
| 3 | `self_x/self_y` oracle | 只写实测 ↔ DB | `PlayObject.cs:1128-1131 SendLogon()` |
| 4 | 201 / `make_index` | 设计 §4 承诺「有 201 就能推」，代码硬编码 UNDERIVABLE（矛盾） | 统一为「本版不实现 201 规则；`bag_equip`/`eat_item` 恒红直到有 201 录制」 |
| 5 | `dice_chain` 的绿 | 混在「9 绿」里 | 单列 `params_default`，不计入发现能力 |
| 6 | 坏行 | traceback / rc=1 | `DiscoveryError`，`file:lineno` + rc=2 |
| 7 | `check-all` 门禁 | 任一 fail-closed ⇒ 恒 rc=2，无法机判 | `--allow-missing[=N]`（默认仍 rc=2；`error` 行永不放行；钉住的 N 不符也 rc=2） |
| 8 | 输出 | `{value, source(字符串), seq, ident}` | `{value, kind, rule, oracle, source:{ident, field, seq}}`（`kind` 区分推导/规则） |
| 9 | 零场景（reviewer C-1） | 缺目录 / 空目录 ⇒ `rows=[]` ⇒ **rc=0**（「检查了零个场景」判绿，fail-open） | `scenarios_dir` 不是目录 ⇒ `DiscoveryError`；`rows` 为空 ⇒ 点名 + rc=2；`--allow-missing`/`=0` 也不放行 |
| 10 | 场景文件坏掉（reviewer C-2） | 坏 JSON / 顶层数组 / `required:[123]` ⇒ traceback **rc=1**；`required:"npc_actor"` ⇒ 按字符拆成 `missing: n,p,c,…` | `_load_scenario` 统一校验 ⇒ 该行 `cls=error`、`scenario-load: …`、整体 rc=2 |
| 11 | `rule` 分栏（reviewer A-1） | 登录期 `npc_click` 的规则绿被算进 `derived=2`，与 §6.2 自己的定义矛盾 | 新增 `cls=rule` / `rule` 计数；登录期 `derived=1 rule=1`，点击期 `derived=3 rule=0` |

### 6.5 空判据 / 空输入审计（reviewer 提醒的同族形状）

「空集合判绿」是本轮反复堵的病根（goal_gate 缺 `counts` 键、recon 唯一 `tracked`、`--post-comment` 假 PASS）。
逐条清点本工具的入口，结论：

| 入口 | 空输入 | 行为 | 兜底 |
|---|---|---|---|
| `check-all` 目录不存在 / 不是目录 | `rows=[]` | **rc=2**（`scenarios dir does not exist or is not a directory`） | 牙④ |
| `check-all` 目录存在但 0 个 `.json` | `rows=[]` | **rc=2**（`no scenarios found under …`） | 牙④ |
| `check-all --allow-missing`（不带 N） | 同上 | **rc=2**（先判空，再谈放行） | 牙④ |
| `check-all --allow-missing=0` | 同上 | **rc=2**（先判空：零场景永远红，`=0` 不能把它变绿） | 牙④ |
| `check-all` 场景文件坏 | 该行无法校验 | 该行 `cls=error` ⇒ 整体 **rc=2** | 牙⑤ |
| 单场景 `derive`：空/空白 `--var` 列表 | 一个变量都没要求 | **rc=2**（「空判据不算通过」） | 用例⑫ |
| 单场景 `derive`：空 `frames` + 非空 required | 全部 missing | **rc=2** | 用例⑫（回归） |
| frames 坏行 / 行非对象 / 字段非整数 | 输入坏 | `DiscoveryError` + `file:lineno` ⇒ **rc=2** | 用例⑪ |
| required 含规则表外的变量名 | 推不出 | 记 missing ⇒ **rc=2**（不猜） | 用例②③ |

仍存在但**不是** fail-open 的一处：`check-all` 只匹配 `*.json`——若场景文件改成别的扩展名，
表现为 `scenarios=0` ⇒ **rc=2（红）**，不会静默判绿。

## 7. 验收口径改判（Leader 裁决 `ed60a40f`）

acceptance[6]「15 个场景全绿」**不可达**，改判为「**可推导的全绿 + 不可推导的逐条登记原因**」。必须同时承认：

1. `bag_equip` / `eat_item` 的 `make_index`/`item_name` 在拿到 `SM_BAGITEMS(201)` 录制前**恒红**（本版未实现 201 规则）；
2. `combat_hit`（`target_x/target_y/hit_dir`）、`move_walk`（`move_x/move_y`）、`king_dialog`/`king_castle_war`（`king_actor`）
   需要新帧或新场景才可能推导；
3. `dice_chain` 的绿来自 `params.npc_actor=119` 硬编码默认值，**不是**发现值。

据此，机判口径：

```sh
# 登录期收帧：1 derived（pickup_item）+ 1 rule（npc_click 的 npc_actor 走 SM_TURN 退化规则）
#             + 5 no_required；7 条已登记 fail-closed
python -I -B tools/probe/param_discovery.py check-all <login frames> --allow-missing=7   # rc=0
# 点击收帧：3 derived（pickup_item / npc_click / merchant_dialog，全部来自 643+SM_LOGON 帧语义）
#           + 5 no_required；6 条已登记 fail-closed
python -I -B tools/probe/param_discovery.py check-all <click frames>  --allow-missing=6   # rc=0
```

「发现能力已验证」只认 `derived`（`rule` / `params_default` / `no_required` 都不算）；
规范 discovery run 必须**点一次 NPC 收 643**，这样 `merchant_actor`/`npc_actor` 才是推导而非规则。
