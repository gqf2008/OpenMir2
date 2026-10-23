# R1 场景 `npc_script_chain`：多步 NPC 脚本链（2026-10-23）

- 卡：`openmir2-scenario-npc-script-chain`（tier=L；owner `openmir2-worker-71`）
- 交付物：`tools/probe/scenarios/npc_script_chain.json`（12 步）+ 本文件
- 基线：master `4518ec045`；分支 `card/npc-chain-scenario`
- 覆盖目标（批量计划 `docs/window-batch-plan-20261023.md` §5.3 的 R1 模板）：`npc_click`
  **只发一次 `CM_CLICKNPC` 然后等 643**，表达不了「从菜单里选一项再进下一层」，R1 就是补这条链。

## 0. 一句话

点一次比奇国王（0122，`GuildOfficial`）之后，用 **5 次 `CM_MERCHANTDLGSELECT(1011)`**
走完 `@@buildguildnow → @buildguildexp → @main → @requestcastlewarA → @exit`，
依次断言 `643 → 763 → 643 → 643 → 643 → 644` 六帧；NPC actor id **不写死**，
走 `param_discovery` 的 `npc_actor` 规则（`SM_MERCHANTSAY(643).recog`）。

## 1. 交互链与 oracle 出处（逐条）

消息号（`src/OpenMir2/Messages.cs`）：`CM_CLICKNPC=1010`（:62）、`CM_MERCHANTDLGSELECT=1011`（:63）、
`SM_MERCHANTSAY=643`（:304）、`SM_MERCHANTDLGCLOSE=644`（:306）、`SM_BUILDGUILD_OK=762`（:375）、
`SM_BUILDGUILD_FAIL=763`（:376）。

| # | 线上动作 | 期望回帧 | oracle 出处 |
|---:|---|---|---|
| 1 | `CM_CLICKNPC(1010)` recog=NPC actor | — | `PlayObject` 的 `CM_CLICKNPC` 臂 → `NormNpc.Click`（`src/M2Server/Npc/NormNpc.cs:68-74`）：清 `ScriptGotoCount/ScriptGoBackLable/ScriptCurrLable` 后 `GotoLable(@main, false)` |
| 2 | wait | `643` | `NormNpc.SendMsgToUser`（`NormNpc.cs:1613-1616`）`RM_MERCHANTSAY` → `SM_MERCHANTSAY(643)`；说词正文由 `ScriptEngine.GotoLable`（`src/Modules/ScriptEngine/ScriptEngine.cs:38-129`）拼出，发送在 `GotoLableSendMerChantSayMsg`（`:987-993`） |
| 3 | `CM_MERCHANTDLGSELECT(1011)` recog=643.recog、body=`@@buildguildnow` | `763` | `PlayObject.Message.cs:888-890` → `ClientMerchantDlgSelect`（`src/M2Server/Player/PlayObject.Operate.cs:114-128`，`npc.UserSelect(sMsg.Trim())`）→ `GuildOfficial.UserSelect`（`src/GameSrv/Npc/GuildOfficial.cs:55-104`）。`@@buildguildnow` 是 `ScriptFlagConst.sBUILDGUILDNOW` 的字面值（`src/Modules/SystemModule/Const/ScriptFlagConst.cs:68`）⇒ `ReQuestBuildGuild`（`GuildOfficial.cs:112-172`）。空行会名/无沃玛号角/金币不足 ⇒ `result<0` ⇒ `SendMsg(RM_BUILDGUILD_FAIL, result)`（`:167-170`）⇒ `SM_BUILDGUILD_FAIL(763)`，**Recog = 失败码、Param = 0**（7 参 `SendMsg` 把 `result` 放 nParam1，泵 `SendDefMessage(SM_BUILDGUILD_FAIL, processMsg.nParam1, 0, 0, 0)` 的第 2 参是 nRecog；`BaseObject.Message.cs:98` / `PlayObject.Message.cs:1940-1942` / `PlayObject.Socket.cs:17`）；失败码随账号状态变（无行会+钱够+无号角 -3、钱不够 -2、已有行会 -1、其余兜底 -4），**永不为 0**。**不是 643**：`[@main]` 里没有 `[@@buildguildnow]` 标签，`GotoLable` 零帧 |
| 4 | `dlg_select @buildguildexp` | `643` | 标签逐字取自部署件 `Envir/Npc_Def/比奇国王-0122.txt` 的 `<如何建立行会./@buildguildexp>`；`[@buildguildexp]` 是纯说词（无 `#IF`），末尾 `<返回/@main>` |
| 5 | `dlg_select @main` | `643` | 回退腿：`LableIsCanJmp` 对 `@main` **硬编码 true**（`src/M2Server/Player/PlayObject.Base.cs:2816-2819`），不依赖上一句说词里的链接 |
| 6 | `dlg_select @requestcastlewarA` | `643` | 部署脚本 `[@requestcastlewarA]` 说词含 `<关闭/@exit>`；前缀分支 `CompareLStr(sLabel, sREQUESTCASTLEWAR)` 只匹配 `@requestcastlewarnow`（`GuildOfficial.cs:82`），本标签不触发 `ReQuestCastleWar` |
| 7 | `dlg_select @exit` | `644` | `@exit` 只有在**上一句说词**含该链接时才可跳（见 §2 的门控）；`GuildOfficial.cs:86-89` `SendMsg(RM_MERCHANTDLGCLOSE, 0, ActorId, 0, 0)` ⇒ `SM_MERCHANTDLGCLOSE(644)`。国王脚本里没有 `[@exit]` 标签，644 只来自这个分支 |

菜单回包的消息号核实结果：**是 `CM_MERCHANTDLGSELECT(1011)`**，不是别的。服务端在
`PlayObject.Message.cs:888-890` 收 `processMsg.nParam1`（=recog=NPC actor）与 `processMsg.Msg`（=标签原文），
转 `ClientMerchantDlgSelect(nParam1, Msg)`。驱动器 `dlg_select` 的 `recog` 缺省取**最近一条 643 的 recog**，
body 按 GBK 编码后整帧 EDCode（与真实客户端同形）。

## 2. 为什么链里没有 `@ask` / `@back`（先给设计说明，不硬凑）

**`@ask`：部署件里根本不存在这个标签。** 对两侧部署根各跑一次：

```sh
grep -rIn "@ask" E:/tmp/conformance/csharp-root/M2GameSvr/Envir      # 0 命中
grep -rIn "@ask" E:/tmp/conformance/rust-root/M2GameSvr/Envir        # 0 命中
grep -rIn "@ask" E:/MirServer/Mir200/Envir                           # 0 命中
```

没有 `[@ask]` ⇒ `GotoLable` 查不到 saying ⇒ 无论发不发都不会有帧。硬写进 `wait_for` 只会得到
「两侧同样超时」的恒失败断言，证明不了任何东西，因此**不放进链**，在此登记。

**`@back`：能发，但在这套部署里是零帧。** `GuildOfficial.UserSelect` 的顺序是：

```csharp
bool boCanJmp = PlayObject.LableIsCanJmp(sLabel);          // GuildOfficial.cs:64
M2Share.ScriptEngine.GotoLable(PlayObject, this.ActorId, sLabel, !boCanJmp);  // :65
if (!boCanJmp) { return; }                                  // :66-69 —— 分支都在这儿之后
```

`LableIsCanJmp`（`PlayObject.Base.cs:2814-2830`）只放行三类：`@main`、**上一句说词里出现过的标签**
（`GetScriptLabel` 从 `GotoLableSendMerChantSayMsg` 发出去的说词里抽 `<文本/标签>`，`PlayObject.Base.cs:2775-2811`）、
以及当次的 `PlayDiceLabel`。部署件里**没有任何 `<文本/@back>` 链接**（`@back2` 是另一个标签）：

```sh
grep -rIn "@back" E:/tmp/conformance/csharp-root/M2GameSvr/Envir
# 只有 真爱-G002.txt / 真爱666-G002.txt 的 @back2，没有 @back
```

⇒ 发 `@back` 时 `boCanJmp=false`，`GotoLable(..., boExtJmp=true)` 什么都不发，紧接着 `return`，
`ScriptGoBackLable` 那条回跳分支（`GuildOfficial.cs:90-95`）**不可达**。要断言它只能等一个不存在的帧。

**替代**：链里用 `@main` 作回退腿——同一套门控里它是唯一**无条件可跳**的标签，语义上就是部署件里
「返回上级」的写法（`[@buildguildexp]` / `[@rw02]` 等都写 `<返回/@main>`）。

> 若将来要真验 `@back` 与 `@ask`，前置是**内容侧**有对应标签/链接（内容冻结，不能自己加），
> 或在窗口用 `QuestDiary`/`QFunction` 类脚本站点。这不是 DSL 扩展能解决的问题，本卡不改 DSL。

## 3. 运行期变量与预置位

- `required: ["npc_actor"]` —— 走 `tools/probe/param_discovery.py` 的 `npc_actor` 规则：
  有 643 取 `643.recog`（`kind=derived`），没有则退化到「首个非 self 的 `SM_TURN(10)`」（`kind=rule`，
  部署假设，点错就响亮超时）。**场景里没有任何 actor id 常量**（不像 `dice_chain` 的 `params.npc_actor=119`）。
- `placement`：`{"map":"0122","x":29,"y":33}` —— 比奇国王在 `Npcs.txt`（`比奇国王 1 0122 29 32`），角色站**南邻格**。
  ① **为什么不用 (29,32)**：那一格被国王**实体**占着，而 `card_window.py` 的预置位只校验**地形**可走
  （`validate_placement_content` 只读 MapInfo / `.map` 阻挡位，不校验实体占用）⇒ 把角色放在同格不是合法站立位。
  ② **这个预置位同时决定了 discovery 会点到谁**：发现 run 用的是 `npc_click_auto`（`click_npc: "auto:last_turn"`，
  取最近一条非 self 的 `SM_TURN`）；角色站在国王南邻格时，最近的可点 NPC 才是国王。**没有这个预置位**时
  discovery 会点到最近的其它 NPC（示例录制里就是赌场庄家），`npc_actor` 就变成庄家，整条链会在第一个
  `wait_for` 之后响亮超时 ⇒ placement 不是可选装饰，是这条链能跑的前置。
  离线校验实测：`0122.map 51x55 可走`（两侧同一份图）。
- 两侧**各自发现各自的值**（`--dry-run` 实录：csharp `--var npc_actor=647278663`、rust `--var npc_actor=119`，
  来自已录的 `w24-{side}-npc_auto/frames.ndjson`；那两份录制是赌场庄家，只用来演示接线，不是本场景真跑值）。

## 4. 复现（零起栈、零连库）

```sh
# 1) 加载期 fail-closed：12 步、required 生效
python -I -B tools/probe/mir2_probe.py --check-scenario \
  --scenario tools/probe/scenarios/npc_script_chain.json --var npc_actor=119
# => scenario OK: npc_script_chain (12 steps, params=npc_actor=119)      rc=0

# 2) 多步 send/wait 序列（展开后的计划；驱动器每次发送都带 --var 的值）
python -I -c "import sys;sys.path.insert(0,'tools/probe');import mir2_probe as m;d=m.load_scenario('tools/probe/scenarios/npc_script_chain.json');p,v=m.expand_scenario(d,{'npc_actor':119});[print(x['index'],x['kind'],x.get('idents') or x.get('body')) for x in p]"

# 3) 编排层离线预览（带真实 --var 的两侧 probe 命令 + 预置位校验）
python -I -B tools/conformance/card_window.py --card openmir2-npc-def-loading \
  --scenario tools/probe/scenarios/npc_script_chain.json --out E:/tmp/cw-dry-npc-chain --dry-run \
  --discovery-frames 'E:/tmp/conformance/runs/w24-{side}-npc_auto/frames.ndjson'
# => rc=0；两侧 probe 命令分别带 --var npc_actor=647278663 / --var npc_actor=119

# 4) probe 自测仍 8 checks 绿（本卡**没有**改 mir2_probe.py，selftest 仍只覆盖 3 个场景文件）
python -I -B tools/probe/mir2_probe.py --selftest   # => OK (8 checks)

# 5) 发现工具把新场景判绿（16 个场景，ok=13 + 3 个既有的 missing）
python -I -B tools/probe/param_discovery.py check-all \
  E:/tmp/conformance/runs/w24-csharp-npc_auto/frames.ndjson tools/probe/scenarios --allow-missing=3
# => npc_script_chain.json rc=0 derived ; SUMMARY scenarios=16 ok=13 rc2=3 derived=4 rule=2 computed=1 ...
```

## 5. 牙（证明第二步的等待是承重的）

把 `steps[1]` 的 `wait_for` 键删掉（保留 `timeout_ms`/`note`），其余逐字节不动：

```sh
python -I -B tools/probe/mir2_probe.py --check-scenario \
  --scenario E:/tmp/npc_script_chain.tooth.json --var npc_actor=119
# => scenario-error: steps[1] must have exactly one action key [...], got []   rc=2
```

还原后取**提交树**的哈希：`git show HEAD:tools/probe/scenarios/npc_script_chain.json | sha256sum`
（与工作区 `sha256sum` 相同，见 patch 证据）。

## 6. 边界、残余与**消费窗口的前置**

**前置 A（important）：763 的失败码在 `Recog`，而且默认被 global 掩码吃掉。**

- 线上帧是 `Recog = result(<0)`、`Param = 0`（见 §1 第 3 行）。
- `tools/conformance/rules.json` 的 `global[0]` 把 `recog` 一律 `mask_fields`（理由写的是「多数消息 recog 是会话相关
  ActorId」，对 763 不成立）⇒ **按现规则失败码根本不参与比对**。要让窗口判这个字段，必须给 `ident=763` 补一条
  `head_compare: ["recog"]` 的规则（照 `ident=52` 的先例：52 的 recog=Gold 就是这样从全局掩码里取回的）；
  改规则 = 改判据，走评审。
- 失败码**随账号状态变**（无行会+钱够+无号角 -3、钱不够 -2、已有行会 -1、其余兜底 -4），窗口若不钉死账号状态，
  就**只断言 ident 出现、不要钉具体码**。

**前置 B（important）：643 / 644 / 763 都不在 `rules.json` 里**（当前 24 条规则：10/14/41/42/46/50/51/52/54/100/
213/520/525/529/530/654/657/658/752/766/811/1106/5008/11029）。

- 比较器口径：某 ident 在任一侧任何一帧出现、规则表却没有条目 ⇒ **UNCLASSIFIED ⇒ rc=2（UNDECIDABLE）**
  （`compare_runs.py` 模块头第 4 条）；`alt` profile 的 `compare_extra` 为空 ⇒ **任何卡拿本场景跑窗口都会直接
  UNDECIDABLE**，不是链本身的问题却会白占一个窗口。已有先例：`npc_click_auto` 因 `ident 643 未登记` 被判 UNDECIDABLE。
- 两个可接受的处置（二选一，都要写进该窗口的报告）：
  1. **补规则**：给 643 / 644 / 763 各写一条 `action=compare` 的规则（带 reason/tier/approved_by，走评审），
     其中 763 用 `head_compare: ["recog"]` 取回失败码；
  2. **显式放行**：在 profile 的 `compare_extra` 里加 `["--allow-unclassified", "643,644,763"]`
     （`card_window.py` 会把它拼进 compare argv），并在报告里接受「这三帧只登记、不判定」。
- 两条都没做之前，**不要把本场景的结果当 PASS 判据**。

- 本卡**不跑真机**：场景只是窗口输入；`763` 的 `Recog`（-1/-2/-3/-4）两侧是否一致、以及说词逐字节，
  由真机对拍判（`openmir2-guild-official-npc` / `openmir2-npc-def-loading`）。
- 建会**成功支**（762 `SM_BUILDGUILD_OK` + 750/622/653/42）需要「金币 ≥ BuildGuildPrice + 背包有沃玛号角」
  的 DB 夹具，本场景按空名/无号角走**失败支**；成功支留给该卡自带的夹具步。
- 链里刻意不用 `sleep` 掩盖时序：每一步都靠 `wait_for` 的帧序推进，缺帧就是 rc=1（响亮失败）。
- `docs/window-runbook-20261023.md` §4.5 的表是「15 个场景」口径；本卡合入后为 **16 个**
  （`npc_script_chain` = derived），该表未改（避免与其它车道抢写集）。
