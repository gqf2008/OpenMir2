# 真机窗口 Runbook（2026-10-23 定稿）

> 面向：第一次跑窗口的人 / 下一次跑窗口的我。本文只写**跑之前的必做动作**与**读结果的口径**，
> 工具用法见 `tools/conformance/README.md`，卡的判据见各卡 `whitelist.md` 行。

## 0. 现状（master `73c39cca` @ 2026-10-23 15:20 +0800）

- **欠窗口的卡：173 张**（口径见 `docs/leader-goal-20261023.md` §1.1 的订正：早前「grep 已合入 master = 114」是错的，
  那 114 里 70 张已 closed、23 张是 tier=L；真欠账 21 张，加上 149 张措辞不在 grep 里的 + 3 张快照后合入 = **173**）。
  分批与逐卡落点见 `docs/window-batch-plan-20261023.md`。
- 工具链：`card_window.py`（批处理）、`compare_runs.py`（帧+库比对）、`param_discovery.py`（参数发现）。
- **开跑前还剩一个前置没到位**：账本的 compare 诊断值**尚未加值断言**（卡 `openmir2-card-window-value-assertions`）。
  参数发现已接进 `card_window.py`（卡 `openmir2-window-discovery-wiring` 已合入 `e72fb9728`）；规则表另见 §4.5。
  账本值断言不落地就跑，账本可能写下错值（`openmir2-card-window-value-assertions` 仍在飞行）。

## 1. 开跑前必做（缺一不可）

1. **重建 Rust 栈并记录 sha256**。
   **要重建的是哪个二进制**：`start-rust.ps1` 委托给 `tools/m2-acceptance/start_mir2_server.ps1`，后者的 `-ServerExe` 缺省值是
   **`mir2-rs/target/release/mir2-server.exe`（其次 debug）**——「单二进制多角色」。它**只从 `-Root` 读配置文件**，
   不执行 `rust-root` 里的任何东西。
   ⇒ **`rust-root` 目录里那堆 `.NET` 程序集（`DBSvr.dll`/`LoginSvr.dll`/`GameSrv.dll`/`TouchSocket.dll`…）与 Rust 栈无关**，
   别被它们误导成「跑的是 C#」；也别以为换掉它们就换了栈。真正决定跑的是哪份代码的，是 `SERVER_EXE` 那一行打印出来的路径。
   **本轮实测的坑**（2026-10-23 16:0x 复核）：`mir2-rs/target/release/mir2-server.exe` = **10-23 09:46**、
   `target/debug/mir2-server.exe` = **10-23 11:54**，而 master 已到同日 15:20+。缺省优先 release ⇒ **不重建就会用 09:46 那份**，
   它早于 gm-registry / window-hardening / gm-shutup / frame-drain / gm-host-seam / param-discovery / 宿主审计等今天全部合并。
   用旧二进制跑出来的对拍结论没有意义，**而且它不会报错**——只会给你一份看起来正常的报告。
   做法：`cargo build --release -p mir2-server`；起栈时先跑一次 `start_mir2_server.ps1 -WhatIf`，
   **核对它打印的 `SERVER_EXE` 路径与文件的 sha256**，再把这个 sha256 写进本次窗口的 `README.txt`。
2. **端口全空**：`powershell -File E:\MirServer\port-status.ps1`，十端口必须全 DOWN。`card_window.py` 起栈前会预检并在占用时 rc=2。
3. **库基线复位**：C# 侧 `restore-baseline.sh`（`mir2_test_*`），Rust 侧 `restore-baseline-r.sh`（`mir2_test_*_r`）。**两套库不同名**，拿 `--db test` 给 Rust 侧拍快照会拍到 C# 的库 ⇒ 假差异。
4. **一次只跑一栈**：C# 与 Rust 用同一组备用端口，串行起停。
5. **场景用到的 ident 必须先在 `rules.json` 登记**（卡 `openmir2-conformance-rules-idents`，2026-10-23 起为硬前置）：
   比较器的口径是「未登记 ident 出现 ⇒ `UNCLASSIFIED` ⇒ **整次窗口 UNDECIDABLE(rc=2)**」——probe 跑得再绿也没用。
   开跑前自查（不起栈、不连库）：

   ```sh
   # 场景 wait_for/wait_frames/wait_idents 用到的 ident – rules.json 已登记 = 差集必须为空
   python -I -B -c "
   import json,glob
   ids=set()
   def walk(o):
       if isinstance(o,dict):
           for k,v in o.items():
               if k in ('wait_for','wait_frames','wait_idents'):
                   (ids.update(x for x in v if isinstance(x,int)) if isinstance(v,list)
                    else ids.add(v) if isinstance(v,int) else None)
               else: walk(v)
       elif isinstance(o,list):
           [walk(x) for x in o]
   [walk(json.load(open(p,encoding='utf-8'))) for p in glob.glob('tools/probe/scenarios/*.json')]
   reg={r['ident'] for r in json.load(open('tools/conformance/rules.json',encoding='utf-8'))['rules']}
   print('missing:', sorted(ids-reg))"
   ```

   输出 `missing: []` 才开跑。新场景先用真实录制跑一遍 `compare_runs.py compare`（`coverage.unclassified_idents` 为空），
   再进窗口。批次 2 已补齐 `11/13/200/201/615/616/635/636/643/644/645/763/767` 共 13 个（`tools/conformance/README.md`
   「当前规则覆盖」有逐条 oracle 出处）。

## 1.5 角色预置位：**声明即可**（工具已支持，2026-10-23 起）

以前这是操作者的手工步骤（`UPDATE characters SET MapName/CX/CY`），现在**不要再手工改库**：
在场景顶层声明需求，`card_window.py` 会在**每侧 restore 之后、起栈之前**自动完成。

```json
{
  "name": "npc_click_king",
  "placement": {"map": "0122", "x": 30, "y": 32, "note": "比奇国王 0122 (29,32) 的邻格"},
  "steps": ["..."]
}
```

- **为什么必须预置**：新角色出生点 = StartPoint `0 289 618`，而第 1 批依赖的四个 NPC 都不在图 0
  （`0122` 比奇国王 `(29,32)`、`GA2` `(13,27)`、`N109` `(38,43)`、`0150` `(16,8)`）；探针不发跨图移动
  ⇒ **漏了只会 click 超时、不报错**。
- **两侧各改各的库**：C# = `mir2_test_db`、Rust = `mir2_test_db_r`（`db.characters_db`）。
  拿错库会直接 rc=2（按该侧 `Server.conf` 的 `[DataBase] database=mir2_test_data*` 交叉核对）。
- **只改 `MapName`/`CX`/`CY`**：预置前后对**其余列做 MD5 指纹对比**，其它列变了即 rc=2
  （防 `characters_ablity.Weight` 那类连带写坏）。
- **fail-closed（rc=2 并点名，绝不让 probe 去超时）**：
  - 图不在 `<envir_root>/Envir/MapInfo.txt`，或该图的服务器号 ≠ 本侧 `[Server] ServerIndex`；
  - 坐标超出 `.map` 宽高，或**落在阻挡格**（oracle 登录期 `CanWalk(x,y,true)` 为假会随机挪 5 次再回
    HomeMap ⇒ 角色悄悄挪位，你只会看到点不到 NPC）；
  - 角色行不唯一 / `HomeMap` 为空 / `Deleted=1` / 表缺列；
  - 改完读回与目标不符。
- **已在目标位 ⇒ 不发 UPDATE**（`action=noop`），不会白写库。
- 证据：`<out>/<side>/placement.json`（声明值、前后值、其余列指纹、SQL），README/card-window.json 里也有。
- **不连库也能验**：`--dry-run` 会做离线内容校验（图存在、坐标在界内且可走）并打印预置命令。
- **仍不支持背包夹具**：「背包里必须已有某个 MakeIndex/物品」这类前置仍走手工 DB 夹具，
  由卡 `openmir2-discovery-bagitems-201` 承接（本工具预留了同一处声明点，落地时直接接）。

## 2. 跑的时候

- `card_window.py --card <卡 id> --scenario <场景> --out <目录> --profile alt`；R 卡窗口**必须** `--post-comment` 记账（在解禁之后）。
- **参数发现**：9 个场景声明了 `required`（`npc_actor`/`merchant_actor`/`make_index`/自身坐标…）。两侧**各自发现各自的值**，某侧推不出 ⇒ fail-closed，不许拿一侧的值跑另一侧。
- **手动步骤不要漏**：`login_only.json` 这类**不含发客户端命令的步骤**。若卡面要求「客户端直呼 `@禁言 <名> 1`」之类的探针，必须由操作者在真实客户端手动输入，脚本不会替你做。漏了不会报错，只会让这条判据看起来没红。
  **角色预置位不再是手动步骤**：场景声明 `placement` 即可（见 §1.5），`card_window.py` 在起栈前自动改库并在 `placement.json` 留证；预置不成就 rc=2，不会带着错位去跑。

## 3. 读结果（最容易错的地方）

- **只看顶层 `verdict`，不要扫 body 里出现过的 `PASS`**。三态：`PASS` / `FAIL` / `UNDECIDABLE`；`UNDECIDABLE` **不是通过**。
- `verdict_reason` 与 `blockers` 写明为什么不是 PASS（如 `evidence_incomplete: README.txt write failed`、`compare_fail`）。
- compare 的帧数/库后态/差异计数在**嵌套 `compare{}`** 里，顶层没有这些字段。
- 证据包必须齐：`report.json` + `README.txt` + `card-window.json` + 两侧 `frames.ndjson`。缺件时工具 rc=2、comment 记 `UNDECIDABLE`。
- **`compare_rc` 的 2 是 UNDECIDABLE，不是绿**。

## 4. 窗口项本身的自检（跑之前 30 秒）

- `grep -n 'ident=' docs/ <卡相关文档>`：**帧号写错没有机器守卫**——今天出过 `ident=769`（实为 `SM_GUILDMAKEALLY_FAIL`）而真帧是 `767`（`SM_MENU_OK`）的事故。写窗口项时**帧号必须同时给 `SM_` 名字**。
- 场景文件里 `--var` 的值必须来自**发现**，不是 `params` 里的默认值（`dice_chain` 的 `npc_actor=119` 就是硬编码默认值，不计入发现能力）。
- **规则表覆盖**：场景 `wait_for` 里出现的每个 ident 都必须在 `rules.json` 有条目（§1 第 5 条的自查），
  否则跑出来的不是 FAIL 而是 `UNDECIDABLE`——**UNDECIDABLE 不是通过**（§3）。

## 4.5 参数发现覆盖 14/15 —— 决定窗口战役能先推多少

> 2026-10-23 更新（卡 `openmir2-window-discovery-rules`）：规则表 4 条 → **5 条规则**（+ `npc_actor` 一条退化规则），另加 **1 类 computed**。
>
> 2026-10-23 晚更新（卡 `openmir2-discovery-bagitems-201`）：新增 **`SM_BAGITEMS(201)` 规则族**
> （`make_index` / `item_name` / `takeon_where`，三变量取自**同一个**物品槽）⇒ 覆盖 12/15 → **14/15**。
> **注意**：该规则目前只有**构造级**证据（合成 201 帧 + oracle 布局），现有 **798 份** `frames.ndjson`（`E:/tmp` + `E:/MirServer` 全扫）里**一条 201 都没有**
> （登录期背包为空 ⇒ oracle `ClientQueryBagItems` 不发帧），真机验证要等一次**非空背包**的录制。

`param_discovery.py` 的规则表有 **5 条规则**：`self_x` / `self_y`（SM_LOGON 的 param/tag）、
`merchant_actor` / `npc_actor` / `king_actor`（SM_MERCHANTSAY(643).recog；`npc_actor` 另有一条 `SM_TURN` 退化规则），
外加第三类 **`computed`**（`move_x` / `move_y`：由 `self_x`/`self_y` + **场景 JSON 里显式声明的 `delta`** 算出）。
三栏**分开计数**，`kind` 分别是 `derived` / `rule` / `computed`：

| 栏 | 含义 | 计入「发现能力已验证」 |
|---|---|---|
| `derived` | 帧语义直接推出 | ✅ |
| `rule` | 退化规则 = 部署假设（点错就响亮超时） | ❌ |
| `computed` | 场景声明 + 已推导量的**确定性算术**（不是猜值） | ❌（另外记录，见 `discovered.json` 的 `kind`） |

对照 15 个场景的 `required`（2026-10-23 实测；201 规则落地前 `--allow-missing=3` 时 rc=0，
201 规则落地后构造级帧上 **`--allow-missing=1` 时 rc=0**，只剩 `combat_hit`）：

| 场景 | required | 发现能否覆盖 |
|---|---|---|
| `merchant_dialog` | merchant_actor | ✅（derived，643） |
| `npc_click` | npc_actor | ✅（derived，643） |
| `pickup_item` | self_x, self_y | ✅（derived，SM_LOGON） |
| `king_dialog` / `king_castle_war` | king_actor | ⚠️ **rule（部署假设，不计入发现能力）**：643 只证明「该 actor 应答了点击」，**不证明是国王**——仓库里全部 643 录制都是赌场庄家（`〖庄〗…@kaiz`），且默认 discovery 点的是「最近的非自己 `SM_TURN`」（录制里就是庄家），**没有可执行步骤能让 discovery 点中国王**。点错的后果是末步响亮超时（`@@buildguildnow` 只存在于`比奇国王-0122.txt`，查不到 label ⇒ 零帧） |
| `move_walk` | move_x, move_y | ✅（**computed**：`self_x+1` / `self_y+0`，delta 写在场景 JSON 的 `computed` 里） |
| `bag_equip` | make_index, takeon_where, item_name | ✅（**derived，201**）：三变量取自同一槽——`MakeIndex`@100 / `Name`@0（帧里就有，不查 StdItems）/ `StdMode`@15 → `ItemLocation`（`M2Share.CheckUserItems` 逆表） |
| `eat_item` | make_index, item_name | ✅（**derived，201**）：同一个「首个可食用槽」的 `MakeIndex`@100 + `Name`@0 |
| `combat_hit` | target_x, target_y, hit_dir | ❌ **需新帧/新场景**：目标是**实体**，当前录制里没有任何一帧报出「附近怪物在第几格」。已知的唯一近似来源 `SM_TURN(10).param/tag` 是**转身者自己**的坐标（`BaseObject.cs:1375 SendRefMsg(RM_TURN, Dir, CurrX, CurrY, …)`），且帧里不区分怪物/NPC ⇒ 不能拿它冒充。缺的是一帧「AOI 出现/实体位置」且带种族或对象类型的字段；`hit_dir` 还要能算 `GetNextDirection(self, target)`（`M2Share.cs:299`） |
| `dice_chain` / `dice_chain_auto` / `gm_batch` / `gm_time` / `login_only` / `npc_click_auto` | 无 required | 直接可跑（6 个，已实测 `--check-scenario` 全 rc=0；`dice_chain` 绿来自 `params` 硬编码默认值，**不算发现**） |

⇒ **规则表落地后，`check-all` 能覆盖 14 个场景**（5 derived + 2 rule + 1 computed + 5 no_required + 1 params_default）；
其中 `king_*`（2 个）与 `move_walk`（1 个）由 `openmir2-window-discovery-rules` 解锁，
`bag_equip` / `eat_item`（2 个）由 `openmir2-discovery-bagitems-201` 解锁（构造级，见 §4.5.1）。`move_walk` 的 `computed` 已接进 `card_window.py`（见下方「接线」）；`king_*` **能跑但不该算绿**——`king_actor` 是部署假设（rule），
默认 discovery 推出来的是庄家，真跑会在末步超时（响亮失败），要让它真绿必须先有「点中国王」的 discovery 场景/步骤。
只剩 `combat_hit`（1 个）要等「AOI 实体位置 + 对象类型」的新帧。
所以「②45 次参数化 run」**第一批按这 14 个推**，
其余按「缺哪个变量 → 补哪条规则/哪个帧」逐条开卡，不要笼统地指望发现工具。

`computed` 的判据形态（复审要能复算）：

- 声明在场景 JSON 顶层：`"computed": {"move_x": {"derived_from": "self_x", "delta": 1}}`；
  **delta 必须显式写出**——缺 `delta` / 类型不对 / 键不在 `required` / `derived_from` 指向另一个 computed
  一律按**场景坏掉**处理（`cls=error`，rc=2），不是「缺变量」。
- 基准量推不出 ⇒ **连锁 fail-closed**（`move_x`/`move_y` 一起进 `missing`），绝不拿 0 顶替。
- 没有 `computed` 声明（单变量推导入口）⇒ 该类变量照旧 missing。
- `mir2_probe` 不读 `computed`（声明性注记，不是步骤 DSL），但顶层可选键放行，否则带声明的场景会被 `unknown key(s)` 拒绝。

### 4.5.1 `SM_BAGITEMS(201)` 规则族（卡 `openmir2-discovery-bagitems-201`）

**oracle 帧体**（`src/M2Server/Player/PlayObject.Operate.cs:38-64 ClientQueryBagItems()`）：

```
header: MakeMessage(SM_BAGITEMS(201), ActorId, 0, 0, ItemList.Count)   // series = 背包物品数
body  : EDCode.EncodeBuffer(ClientItem) + '/'  × ItemList.Count        // 逐项、无分隔前缀
```

每条 `ClientItem` 记录是**定长 124B**（`src/OpenMir2/Packets/ClientPackets/ClientItem.cs WritePacket`），
关键偏移：`Name`@0（`WriteAsciiString(ItemNameLen=14)` = 1B 长度 + 14B 定长 GBK）、`StdMode`@15、
`MakeIndex`@100（i32 LE）。**帧里没有 StdItem 的 `Index`**（客户端靠 Name 识别物品；服务端
`ClientTakeOnItems`/`ClientUseItems` 也是 MakeIndex + GetItemName 双匹配）⇒ 「MakeIndex 取成 Index」
在线上不可能发生，`item_name` **不需要查 `StdItems` 表**。旁证：M3 实测 `SM_BAGITEMS body_len=668`
= 4 × (166B 编码 + 1B `'/'`)，与 124B 记录一致。

**取哪个物品（显式、确定、不随机）**：只看**首条** 201（槽序 = `ItemList` 序）：
`required` 含 `takeon_where` ⇒ **首个可装备槽**（`StdMode` 在 `M2Share.CheckUserItems` 槽表里）；
否则 ⇒ **可食用槽**（`StdMode ∈ {0,1,2,3,4,31}`，`ClientUseItems` 的 switch 分支），并按**固定价值次序**
`StdMode31 & AniCount!=0`（QFunction `@StdModeFunc<N>` 有脚本，exec-seams ② 路的判据包）→
`StdMode31 & AniCount==0`（解包）→ 药（0/1/2/3）→ 书（4），同档次序取最小槽。
**`AniCount` 在帧里**（记录偏移 18），所以这条优先级不需要额外信息。
`make_index`/`item_name`/`takeon_where` 一次 derive 只选一次 ⇒ **三个 `--var` 必然同槽**。
无候选 ⇒ missing（不退回「随便取一个」）。

**`takeon_where` 的来源**：`StdMode` → `ItemLocation` 逆表（`M2Share.CheckUserItems`，
`src/M2Server/M2Share.cs:376-458`）；同 StdMode 多槽时取**最小槽号**
（24/25/26 → ArmRingl(5)，25 虽然也能进 Bujuk(9)）。CM_TAKEONITEM 的 `nParam2` 就是这个槽号
（`PlayObject.Message.cs:820-821`）。

**局限（真机前必读）**：
1. **待真实录制验证**：现有录制无 201 ⇒ 规则只有构造级证据；真机需要一个**非空背包**的 discovery run
   （背包夹具仍走手工 DB 或后续卡，`card_window.py` 本期未接背包预置）。
2. **跨侧一致性**：`make_index` **不在** `card_window.py` 的 `SESSION_SCOPED_VARS` 里 ⇒ 两侧 `MakeIndex`
   不同会被判 `value_mismatch` rc=2。两侧库由同一基线克隆且 `characters_bagitem.MakeIndex` 是从库读回的
   （`PlayDataStorage.Query.cs:404-425`）时两侧相同；若某侧在会话里**新造**物品（GM/掉落）则两侧必然不同，
   那时要么把 `make_index` 加进会话作用域（改 `card_window.py`，另卡），要么先做背包夹具对齐。

**接线（`card_window.py`，已完成）**：`scenario_computed()` 读场景的 `computed` 声明 → `discovery_cfg`/`side_ctx`
携带 → `derive_from_frames(frames, required, computed)` → 两处调用点（真跑 `run_side` + `--dry-run` 回放）透传。
证据：`card_window --selftest` 的 DISC-9（两侧 `self_x` 钉 7 ⇒ 两侧 `--var move_x=8 move_y=12`、
`discovered.json` 记 `kind=computed`、缺 `delta` 加载期 `ConfigError`）；以及离线预览
`--dry-run --discovery-frames E:/tmp/conformance/runs/w24-csharp-npc_auto/frames.ndjson` 两侧 probe 命令都带
`--var move_x=8 --var move_y=12`（不起栈、不建目录）。

> 计数订正：本表初稿写「9 个场景（3 可发现 + 6 无 required）」，是规则表 4 条时的口径；
> 规则扩到 5 条 + computed 后实测为 **12 个**（`SUMMARY ... ok=12 rc2=3 derived=3 rule=2 computed=1
> no_required=5 params_default=1 missing=3 error=0`）。复审 `1e9f6315` 指出 king 记 derived 与本文件的分栏定义冲突，已改判 **rule**（`derived` 5→3、`rule` 0→2），`king_*` 不再计入「发现能力已验证」。
>
> 再订正（`openmir2-discovery-bagitems-201`）：201 规则族落地后，**同一个真实录制**
> （`E:/tmp/conformance/runs/w24-csharp-npc_auto/frames.ndjson`，**无 201**）仍是
> `ok=12 rc2=3 missing=3`——这是**正确**的 fail-closed（没有 201 就推不出）；
> 把一条按 oracle 布局合成的 201 帧（4 项背包同理，`body_len=334` 对应 2 项）追加到同一录制的**副本**后，
> 实测 `SUMMARY ... ok=14 rc2=1 derived=5 rule=2 computed=1 no_required=5 params_default=1 missing=1 error=0`，
> `bag_equip` 得 `item_name=屠龙,make_index=123456,takeon_where=1`、`eat_item` 得
> `item_name=金创药,make_index=9001`。**真实录制仍待补**（见 §4.5.1 局限 1）。

## 4.6 脚本动作参数里的 `<$…>` **不会被展开**（2026-10-23 晚实测，写窗口项前必读）

oracle 的 `ActionOfGmExecute`（以及多数脚本动作）**直接拼 `sParam1..6`，不调 `GetLineVariableText`** ⇒
脚本里写 `Gmexecute 踢人 <$STR(S6)>` 时，命令收到的是**字面量 `<$STR(S6)>`**，不是展开值。

当天三张 GM 卡的窗口判据全部建立在这个错误前提上，已全部打回重写。**写任何 GM/脚本动作窗口项前，先算出"命令实际收到什么"**：
- 若部署调用点传的是 `<$STR(Sn)>` / `<$USERNAME>` ⇒ 真机看到的是**检测失败**（如「<$STR(S6)> 现在不在线」红字），不是成功分支；
- 只有调用点传**字面量**（如 `Gmexecute 改属性 78`）时，命令才拿到真值。

同理：`SENDMSG` 的文本走的是 `GetLineVariableText`（**会**展开命名变量？**不会**——`IsStringNumber` 的 `||` 缺陷让命名变量分支全是死代码，见 whitelist B-9；只有数字码 `<$N>` 可达）。

## 5. 跑完

- 写 `--post-comment`（解禁后）+ 卡的 `status` 里写明：哪几个窗口项已验、证据路径、两侧二进制 sha256、以及**没验到的项和原因**。
- R 卡窗口全验完才 `closed`；只验了一部分就保持 `in-progress`，并在 `work` 里写明剩余项。
