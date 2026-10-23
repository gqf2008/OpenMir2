# 场景 `recog` 打包口径（CM 动作族）与既有夹具复核

卡 `openmir2-scenario-recog-packing-fix`（tier=L）。写集：`tools/probe/scenarios/`、`tools/probe/scenarios/README.md`、`docs/`。

- 原始交付：base `04366adc8`（2026-10-23，openmir2-worker-24）。
- **返工快照：`bb4317a01`（2026-10-24，openmir2-worker-63 接管）**；触发 = openmir2-reviewer-7 的
  `request_changes`（review oid `5cf6cef64d3c6db500baa886a95d83f23f82602b`）。
- **本卡只修夹具 / 文档，不跑真机窗口。**

> **与卡 `openmir2-scenario-combat-hit-waitfix` 的关系（先读）**
>
> 本卡原始交付把 `combat_hit.json` 从 `recog=target_x / param=target_y` 改成
> `recog=target_xy=(target_y<<16)|target_x`，**只修对了「X/Y 都在 `recog`」这一层**。
> `openmir2-scenario-combat-hit-waitfix`（已 closed、已合入 master）进一步订正了**值语义**：
> `CM_HIT` 的 `Recog` 装的是**攻击者自身格**，不是目标格
> （`hit_xy = (self_y<<16)|self_x`，`computed.pack {lo:self_x, hi:self_y}`）；
> 目标由 `tag` / 状态化 `Dir` 经 `GetPoseCreate()` 决定。
> **返工以 master 版为准，不回退该订正**；本文档 §2 / §5 已按自身坐标口径改写。

## 1. 口径（公式）

`TDefaultMessage` 的 `Recog` 是 32 位；凡「把格坐标塞进 `Recog`」的命令，打包公式相同：

```
recog = (y << 16) | x        # LoWord = X，HiWord = Y
param 通常不携带坐标（动作族恒不读；CM_SPELL 的 param 是目标 ActorId 的低 16 位）
tag   = 方向 / 魔号 / 其它语义（按命令）
```

**但 `(x,y)` 的语义按 ident 区分——这是本卡 R1 的核心**：

| ident 族 | `(x,y)` 是什么 | oracle 依据 |
|---|---|---|
| `CM_WALK(3011)` / `CM_RUN(3013)` | **目标格**绝对坐标 | `PlayObject.Attack.cs:1365 ClientWalkXY`；`PlayObject.Message.cs:1060` |
| `CM_TURN(3010)` / 攻击族 `CM_HIT(3014)` 等 | **自身格**坐标（`CurrX/CurrY`） | `PlayObject.Attack.cs:823 ClientHitXY`，`:883` 要求 `nX==CurrX && nY==CurrY`；`PlayObject.Operate.cs:310 ClientChangeDir`（门在 `:331`）；目标取状态化 `Dir` |
| `CM_SPELL(3017)` | **目标格**绝对坐标 | `WorldServer.cs:783-793`；`PlayObject.Message.cs:1339-1341`；`PlayObject.Attack.cs:1054 ClientSpellXY` |

oracle 证据（动作族字段位置）：

- `src/GameSrv/Word/WorldServer.cs:830-852`：`CM_HORSERUN / CM_TURN / CM_WALK / CM_SITDOWN / CM_RUN / CM_HIT /
  CM_HEAVYHIT / CM_BIGHIT / CM_POWERHIT / CM_LONGHIT / CM_CRSHIT / CM_TWINHIT / CM_WIDEHIT / CM_FIREHIT`
  同一 case 组，两支（`:846` `SendActionMsg(Ident, Tag, LoWord(Recog), HiWord(Recog), 0, "")` /
  `:850` `SendMsg(playObject, Ident, Tag, LoWord(Recog), HiWord(Recog), 0)`）**都只读 `Recog` 的 Lo/Hi，
  `defMsg.Param` 完全没有被读**。
- `src/M2Server/Actor/BaseObject.Message.cs:34/98`：`SendMsg(IActor, wIdent, wParam, nParam1, nParam2, nParam3, sMsg)`
  ⇒ 上句的实参顺序 = `wParam=Tag`、`nParam1=LoWord(Recog)`、`nParam2=HiWord(Recog)`、`nParam3=0`。
- `CM_HIT` 消费端：`PlayObject.Message.cs:1227-1230` 把 9 条攻击 ident 交给
  `ClientHitXY(wIdent, processMsg.nParam1, processMsg.nParam2, (byte)processMsg.wParam, …)`；
  `ClientHitXY`（`PlayObject.Attack.cs:823`）在 `:883` 判 `nX == CurrX && nY == CurrY` 才继续
  ⇒ **`Recog` 的两个半字必须是攻击者自己的坐标**。目标不是从 `Recog` 取的：`:910` `AttackDir(null, 0, nDir)`
  → `AttackDir`（`:22`）里 `GetPoseCreate()`（`:26` → `BaseObject.cs:1253`）按**当时的状态化 `Dir`**
  取前方格；`Dir = nDir` 要到后面的 `BaseObject.Attack.cs:12` 才写入，所以**本击的 `tag` 只影响动作帧
  与之后的朝向，救不了本击的目标选择**。
- `CM_WALK` 消费端：`PlayObject.Message.cs:1059-1060` → `ClientWalkXY`（`PlayObject.Attack.cs:1365`）；
  它的 `nX/nY` 是**目标格绝对坐标**（与 `CM_HIT` 相反）。
- `CM_SPELL`：`WorldServer.cs:783-793` 的独立 case 用
  `SendMsg(Ident, Tag, LoWord(Recog), HiWord(Recog), MakeLong(Param, Series))`
  ⇒ `nParam1/nParam2` 是**目标格**、`nParam3` 是目标 ActorId；`PlayObject.Message.cs:1339-1341`
  → `ClientSpellXY(...)`（`PlayObject.Attack.cs:1054`）。

## 2. 逐文件复核

返工快照 `bb4317a01` 的实测范围：`grep -l '"recog"' tools/probe/scenarios/*.json` = **9 个文件**，
全部列在下表；其余 11 个场景不含 `recog`。

| 文件 | ident | 返工快照写法 | oracle 口径 | 结论 | 处置 |
|---|---|---|---|---|---|
| `bag_equip.json` | 1003 `CM_TAKEONITEM` | `recog=make_index, param=takeon_where` | 该命令走 `WorldServer.cs:797-820` 组，`:820` `SendMsg(…, Series, Recog, Param, Tag)` ⇒ `nParam1=Recog`、`nParam2=Param`；`PlayObject.Message.cs:820-821` `ClientTakeOnItems((byte)nParam2=where, nParam1=MakeIndex)` | **对** | 不动 |
| `combat_hit.json` | 3014 `CM_HIT` | `recog=hit_xy`（`computed.pack {lo:self_x, hi:self_y}`）、`param=0`、`tag=hit_dir` | 动作族只读 `LoWord/HiWord(Recog)`；`ClientHitXY` 在 `PlayObject.Attack.cs:883` 要求 `nX==CurrX && nY==CurrY` ⇒ `Recog` 是**攻击者自身格**；目标由 `tag`/状态化 `Dir` 经 `GetPoseCreate()` 取 | **对**（字段位置与值语义都对；值语义由 `openmir2-scenario-combat-hit-waitfix` 订正） | 不动；场景已登记 **BLOCKED**（无目标夹具 + wait 帧不可归因）。改前 `target_x/target_y` 错两层：① Y 恒为 `HiWord(recog)=0`；② 即使打包成目标格，`Recog` 也不是目标坐标 |
| `combat_strike.json` | 3010 `CM_TURN` + 3014 `CM_HIT` | `recog=self_pos`（`params.self_pos=15204608` = `MakeLong(256,232)`）、`tag=0` | 同动作族；`ClientChangeDir`（`PlayObject.Operate.cs:331`）与 `ClientHitXY`（`:883`）都要求 `nX/nY` = 自身格 | **对** | 不动（场景 BLOCKED-FIXTURE：伤害/躲闪门槛 + wait 不可归因，与本表口径无关） |
| `eat_item.json` | 1006 `CM_EAT` | `recog=make_index` | 不在 switch ⇒ `default`（`WorldServer.cs:856-857`）：`nParam1=Recog`；`PlayObject.Message.cs:826-827` `ClientUseItems(nParam1, Msg)` | **对** | 不动 |
| `equip_struck_save.json` | 3010 `CM_TURN` + 3014 `CM_HIT` | `recog=self_pos`（`params.self_pos=1245203` = `MakeLong(19,19)`）、`tag=0` | 同 `combat_strike` | **对** | 不动（Rust 侧 `SM_DURACHANGE(642)` 未接，另卡登记） |
| `gm_time.json` | 3030 `CM_SAY` | 全 0 | `WorldServer.cs:853-854` 只取文本，参数全 0 | **对** | 不动 |
| `move_walk.json` | 3011 `CM_WALK` | `recog=move_xy`（`computed.pack {lo:move_x, hi:move_y}`）、`param=0` | 动作族只读 `LoWord/HiWord(Recog)`；`ClientWalkXY` 的 `nX/nY` 是**目标格绝对坐标**（`PlayObject.Attack.cs:1365`；`PlayObject.Message.cs:1060`） | **对**（原 `recog=move_x, param=move_y` 错：Y 恒 0） | 不动 |
| `pickup_item.json` | 1001 `CM_PICKUP` | `recog=0, param=self_x, tag=self_y` | `default`（`:856-857`）⇒ `nParam2=Param`、`nParam3=Tag`；`PlayObject.Message.cs:811-815` 要求 `CurrX==nParam2 && CurrY==nParam3` | **对** | 不动 |
| `spell_cast.json` | 3017 `CM_SPELL` | `recog=near_pos/far_pos`（`MakeLong` 目标 X,Y）、`param=0`、`series=0`、`tag=magic_id` | 独立 case（`WorldServer.cs:783-793`）：`LoWord/HiWord(Recog)` = **目标格**，`MakeLong(Param,Series)` = 目标 ActorId；`PlayObject.Message.cs:1339-1341` | **对**（`CM_SPELL` 的目标格口径与 `CM_HIT` 的自身格口径**不同**，别混用） | 不动（BLOCKED-FIXTURE：缺 `characters_magic` 夹具） |

> **表后注记的适用范围（R2 订正）**：返工快照上含 `"recog"` 的 9 个文件就是上表 9 行；其余 11 个场景
> （`dice_chain*`、`king_*`、`merchant_dialog`、`npc_click*`、`npc_script_chain`、`login_only`、`gm_batch`、
> `upgrade_retrieve`）不含 `recog`。原始 base `04366adc8` 时只有 6 个：`combat_strike`、
> `equip_struck_save`、`spell_cast` 是经 merge master 进入本分支的（不是原始 patch 的改动）。
> 因此原文「其余场景不发送本族 `CM_*`」**只在 base `04366adc8` 成立，在返工快照上不成立**。

## 3. DSL 现状（返工快照：`computed.pack` 已落地）

- `mir2_probe` 的取值只有两种：int，或 `params`/`--var` 里同名变量的十进制 int（`resolve_param`）。
  **probe 本身不读 `computed`**；`computed` 是给 `param_discovery` / `card_window` 的声明。
- `computed` 现在支持两种声明：`derived_from` + `delta`（加法），以及
  `{"pack": {"lo": <var>, "hi": <var>}}`（值 = `(hi<<16)|lo`；`lo/hi` 可引用其它 `computed`，按不动点求解；
  成环、缺字段、与 `derived_from/delta` 混用都 fail-closed）。卡 `openmir2-probe-computed-pack`
  已合入 master（`b94040c08`），所以 §3 旧文里「DSL 没有打包表达方式、建议后续卡加 pack」已经过期。
- 实测（合成一帧 `SM_LOGON`：`{"ident":50,"recog":191,"param":7,"tag":12}`）：
  `param_discovery.py check-all` 对 `move_walk.json` ⇒ `rc=0 computed`，
  `params=move_x=8,move_xy=786440,move_y=12`；对 `combat_hit.json` ⇒ `rc=2 missing: hit_dir`
  （`hit_xy` 已可由 `self_x/self_y` 经 pack 推出，**方向没有推导来源**，必须操作者给；且该场景本就 BLOCKED）。
- 结论：原始交付留下的「两条场景因 `*_xy` 推不出而 fail-closed」在返工快照上**只剩 `combat_hit` 的 `hit_dir`**；
  `move_walk` 的 `move_xy` 断链已由 computed-pack 闭环。

## 4. 影响面（改动前必读）

- `move_walk.json` / `combat_hit.json` 的**线上字节变了**（这正是修复点）：`Recog` 从单坐标 / 目标坐标
  改成「X/Y 打包」与「自身格」，`param` 归 0。真跑时值由 `--var` 传入；`card_window` 用发现值填。
  `combat_hit` 现在是 **BLOCKED**（`card_window` 在起栈前即 rc=2）；`move_walk` 的 `move_xy` 可自动发现。
- **仍写旧口径的文档（本卡写集外/不属本卡范围，只登记；不会造成假绿——真跑 fail-closed / BLOCKED 是响亮红，
  但说明书会误导操作者）**：
  - `docs/window-runbook-20261023.md:397`（`move_walk` 仍写 `move_x / move_y`）、
    `:400`（`combat_hit` 仍写 `target_x/target_y/hit_dir`）、`:404-407`（发现链描述）；
  - `docs/window-debt-reclass-20261023.md:128`（`CM_WALK` / `CM_HIT` 旧字段口径）、
    `:72`、`:74`、`:300`、`:397`（`move_walk` 参数）；
  - `docs/conformance-a-card-scenario-map-20261023.md:320-321`、`:328`；
  - `docs/param-discovery-design-20261023.md:8`、`:263`、`:273`、`:368`；
  - `docs/probe-ident-direction-20261023.md:106`；
  - `docs/window-batch-plan-20261023.md:152`、`:317`（`:483` 已更新成 `move_x/move_y/move_xy` +
    `computed.pack`，无需改）。
- **本卡写集外的残留（R4，已如实登记，不阻塞）**：
  1. `tools/probe/param_discovery.py:138` 的 `COMPUTED_ORACLES["move_y"]` 文案仍写
     「`nParam1=X → recog，nParam2=Y → param`」，与 oracle 不符（X/Y 都在 `Recog`，`Param` 不读）。
     **板面无现成承接卡**（`openmir2-probe-computed-pack` 只加了 `pack` 支持、未改文案）；建议卡 id
     **`openmir2-param-discovery-oracle-text-fix`**（写集 `tools/probe/param_discovery.py` + 对应 selftest 期望）。
     本卡**不开卡**。
  2. `card_window` 里 `combat_hit` 仍 **BLOCKED**（无目标夹具 + `wait_for` 无来源过滤）——承接卡
     `openmir2-scenario-combat-hit-waitfix`（closed）已在 `blocked.missing_fixture` 登记；
     `move_walk` 的 `*_xy` 断链已由 `openmir2-probe-computed-pack`（closed）解除。

## 5. 牙

工具：`probe` 自己的 `load_scenario` / `expand_scenario`（只读，`E:/tmp/w63-gates/expand_send.py`；
`card_window --dry-run` 就是用同一套展开拼 `--var`）。变异用 `git show <旧 commit>:<path>` 写回，
还原一律 `git checkout -- <path>`。

### 5.1 `combat_hit.json`：值语义（自身格 vs 目标格）

```
# 正确（返工快照 HEAD，sha256=5ae04ae01d5d83854087bf04b68133fe87802c14ed34ebcf97917fe6910321ae）
ident=3014 recog=2097182 (0x20001E) param=0 tag=2 => LoWord(X)=30 HiWord(Y)=32
# 服务端 nX/nY=(30,32) = 自身格；目标交给 tag/Dir

# 牙：换成 base 04366adc8 的错误写法（recog=target_x, param=target_y）
ident=3014 recog=30 (0x1E) param=32 tag=2 => LoWord(X)=30 HiWord(Y)=0
# 服务端 nX/nY=(30,0) ⇒ nX==CurrX && nY==CurrY 不可能成立 ⇒ 无攻击、无 SM_HIT

# git checkout -- tools/probe/scenarios/combat_hit.json
sha256sum = 5ae04ae01d5d83854087bf04b68133fe87802c14ed34ebcf97917fe6910321ae
          = git show HEAD:tools/probe/scenarios/combat_hit.json | sha256sum   ← 一致
```

### 5.2 `move_walk.json`：`card_window --dry-run` 的可见差异

```
# 正确（返工快照 HEAD，sha256=239d4c289d7c5b20219ec5d40400065f6870558f40ede3791b042868f5db6bca）
python -I -B tools/conformance/card_window.py --scenario tools/probe/scenarios/move_walk.json \
    --dry-run --out E:/tmp/w63-gates/dry-move-ok
rc=0
--var "move_x=<discovered:move_x>"
--var "move_xy=<discovered:move_xy>"      ← pack 后的目标格
--var "move_y=<discovered:move_y>"
expand: ident=3011 recog=65537 (0x10001) param=0 tag=0 => (X,Y)=(1,1)

# 牙：换成 base 04366adc8 的错误写法（recog=move_x, param=move_y）
python -I -B tools/conformance/card_window.py --scenario tools/probe/scenarios/move_walk.json \
    --dry-run --out E:/tmp/w63-gates/dry-move-wrong
rc=0
--var "move_x=<discovered:move_x>"
--var "move_y=<discovered:move_y>"        ← 没有 move_xy（可见差异）
expand: ident=3011 recog=1 (0x1) param=1 tag=0 => (X,Y)=(1,0)   ← Y 恒 0（错）

# git checkout -- tools/probe/scenarios/move_walk.json
sha256sum = 239d4c289d7c5b20219ec5d40400065f6870558f40ede3791b042868f5db6bca
          = git show HEAD:tools/probe/scenarios/move_walk.json | sha256sum   ← 一致
```

> 归零残留：两次变异后 `git status --short` 均为空；变异体 sha256（`combat_hit` 旧写法
> `ac92562cd73f63e4f341acff5a27462b3e9075ccbc8baed9e8cd6b3d021b5bfe`、
> `move_walk` 旧写法 `c92a445776a83bcb2703c35ad2d7decc7a83865e077dd8aef1b08d8a3c2fe43d`）
> 只出现在日志里，未提交。

## 6. 残留物清单

- `E:/tmp/w63-gates/`：`check_all.py`（20 场景 `--check-scenario` 批量跑）、`expand_send.py`（牙的展开打印）、
  `logon.ndjson`（合成 `SM_LOGON` 帧，供 `param_discovery check-all`）、
  `dry-move-ok/`、`dry-move-wrong/`、`dry-combat/`、`dry-combat-wrong/`（`card_window --dry-run` 预演目录，
  未起栈）、`combat_hit_wrong.json`（旧写法副本）、`*.log`。均只读或未起栈，保留供审查复算。
- 原始作者（worker-24）的 `E:/tmp/w24-dry-combat/`、`E:/tmp/w24-dry-move/`、`E:/tmp/w24_recog_tooth.py`
  未清理，登记沿用。
- 未起任何服务端/客户端进程；未占端口；未连库。

## 7. 返工记录（reviewer-7 `request_changes` → worker-63）

| # | reviewer-7 发现 | 返工处置 | 证据 |
|---|---|---|---|
| R1 | `combat_hit` 装的是**目标格**语义；`ClientHitXY` 把 Lo/Hi 当**攻击者自身** `nX/nY`，`nX==CurrX && nY==CurrY`（`PlayObject.Attack.cs:883`）不成立即 false；docs §2/§5 误读「目标=(30,32)」 | **不改 master 夹具**（`combat-hit-waitfix` 已订正为 `hit_xy=computed.pack{lo:self_x,hi:self_y}`）；§1 新增「同一公式、语义按 ident 区分」表，§2 的 `combat_hit` 行与 §5.1 改为**自身坐标**口径，并写明与 `combat-hit-waitfix` 的关系 | §1 / §2 / §5.1；§5.1 展开值 (30,32) vs 旧写法 (30,0) |
| R2 | head 上 `combat_strike` / `equip_struck_save` 未入 §2 表；「其余场景不发送本族 CM_*」在 head 上不成立 | §2 补齐两行（并补 master 新增的 `spell_cast`，共 9 行全覆盖）；表后注记改成「返工快照 9 个文件全在表内，其余 11 个不含 recog」，并说明这三条是经 merge master 进入、非原始 patch | `grep -l '"recog"' tools/probe/scenarios/*.json` = 9；`combat_strike self_pos=15204608⇒(256,232)`、`equip self_pos=1245203⇒(19,19)` 均 `--check-scenario rc=0` |
| R3 | §4 影响面漏 `window-runbook` 的 `move_walk` 行等引用 | §4 按返工快照的实际行号补全：window-runbook `:397/:400/:404-407`、window-debt-reclass `:72/:74/:128/:300/:397`、conformance map `:320-321/:328`、param-discovery-design `:8/:263/:273/:368`、probe-ident-direction `:106`、batch-plan `:152/:317`；并说明它们不会造成假绿 | §4 清单逐条来自 `grep -n`（2026-10-24，HEAD `bb4317a01`） |
| R4 | 两条残留是否如实登记 + 承接卡 | ① `param_discovery.py:138` 文案残留：确认仍在 master；板面无现成承接卡 ⇒ **建议** id `openmir2-param-discovery-oracle-text-fix`（本卡不开卡）；② `*_xy` fail-closed：`move_walk` 已由 `openmir2-probe-computed-pack` 解除，`combat_hit` 仍 BLOCKED 且由 `openmir2-scenario-combat-hit-waitfix` 登记 | §4「本卡写集外的残留」；`check-all` 实测 `move_walk rc=0 computed` / `combat_hit rc=2 missing: hit_dir` |
