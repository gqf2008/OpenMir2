# 脚本 `SENDMSG 0..4` 的宿主实现（卡 `openmir2-script-sendmsg-host`，tier=R）

> 分支 `card/sendmsg-host`；**返工基线 = master `95b169ecc`**（首版基线 `f72517d2`，review `8a3c22d8`
> 判 `request_changes` 后已 rebase 并重钉台账行号）；whitelist 行 **T-224**。
> 本文只记录**可供复查的事实**（oracle 出处、实现落点、偏差、判据、窗口命令），不代替线程记录。
>
> **返工处置**（review `8a3c22d8`）：I-1 `@StdModeFunc*` 排水点接 `world_broadcasts`（§3/§4/§5）；
> I-2 `whitelist.md` ④ 与台账 `consequence` 的语料结论与函数名订正（§4）；I-3 W-3 补可复跑命令 +
> W-2 补冷路径口径（§6）；I-4 rebase 后按合并树重钉台账行号（`check --strict-lines` GREEN）。

## 1. 问题（改前）

`crates/script` 的 `ActionOfLineMsg`（`actions.rs:209-240`）**已按 oracle 分发**（含 case1 `"(*) "`、
case2/3 的前缀拼装、`%s`/`%d` 替换），但两个宿主钩子在 `NpcPlayer` / `HostNpc` 上**都是缺省 no-op**：

- `crates/script/src/engine/mod.rs` 的 `ScriptPlayer::world_broadcast`（case 0-3）；
- `crates/script/src/engine/mod.rs` 的 `ScriptNpc::send_say_msg`（case 4）。

⇒ 部署语料里 **349 行** `SENDMSG`（case0-3 = 318、case4 = 31）产 **0 帧**。

## 2. oracle 逐段（`src/**`，冻结面）

| 段 | 文件:行 | 行为 |
|---|---|---|
| 分发 | `src/Modules/ScriptEngine/Processings/ExecutionProcessingSys.cs:3163-3200` | 文本先 `sMsg.Replace("%s", playerActor.ChrName)`、`Replace("%d", normNpc.ChrName)`（`:3165-3167`，**全局替换**）；case0 `SendBroadCastMsg(sMsg, System)`（`:3170`）、case1 `"(*) " + sMsg`（`:3173`）、case2 `'[' + normNpc.ChrName + ']' + sMsg`（`:3176`）、case3 `'[' + playerActor.ChrName + ']' + sMsg`（`:3179`）、case4 `normNpc.SendSayMsg(sMsg)`（`:3182`） |
| 广播体 | `src/GameSrv/Word/WorldServer.cs:1738-1765` | `ShowPreFixMsg` ⇒ 按 `msgType` 拼前缀（`System` ⇒ `SysMsgPreFix`，`:1754`）；`EnableChatServer` 支体**已注释**（`:1759-1762`）；否则 `for (PlayObjectList)` 里 `if (!playObject.Ghost) playObject.SysMsg(sMsg, MsgColor.Red, msgType)`（`:1763-1769`） |
| 红字帧 | `src/M2Server/Actor/BaseObject.cs:1588-1686`（非 `Notice` 支 `:1666-1686`）+ `src/M2Server/Player/PlayObject.Message.cs:1715-1718` | `SendMsg(RM_SYSMESSAGE, 0, RedMsgFColor, RedMsgBColor, 0, sMsg)` ⇒ `SM_SYSMESSAGE(100)`、`Recog` = **收件人自己**的 `ActorId`、`Param = MakeWord(F,B)`、`Series = 1`、尾 `EncodeString` |
| NPC 喊话 | `src/M2Server/Npc/NormNpc.cs:76-79` → `src/M2Server/Actor/BaseObject.Base.cs:475-478` | `ProcessSayMsg`：`sChrName = Race == Play ? ChrName : M2Share.FilterShowName(ChrName)`；`SendRefMsg(RM_HEAR, 0, btHearMsgFColor, btHearMsgBColor, 0, sChrName + ':' + sMsg)` |
| 方格广播 | `src/M2Server/Actor/BaseObject.Message.cs:383-440` | `Envir == null` ⇒ 记 Error 后 return（`:385-389`，隐藏 NPC 零帧）；`ObMode/FixedHideMode` ⇒ 只发自己；否则扫 `CurrX/Y ± SendRefMsgRange`（`:399-402`），对 `Race == Play` 的 actor 各一发（非 `Play` 只放行 `RM_STRUCK/RM_HEAR/RM_DEATH`）、**不排除自己** |
| 喊话帧 | `src/M2Server/Player/PlayObject.Message.cs:1705-1707` | `SM_HEAR(40)`、`Recog` = `processMsg.ActorId`（= **NPC** 的，`BaseObject.Message.cs:154`）、`Param = MakeWord(nParam1, nParam2)`、`Series = 1`、尾 `EncodeString` |
| 名字过滤 | `src/M2Server/M2Share.cs:571-594` | `FilterShowName`：首个 ASCII 数字或 `-` 处截断 |

## 3. 实现落点

| 位置 | 内容 |
|---|---|
| `crates/script/src/engine/mod.rs` | `ScriptPlayer::world_broadcast` / `ScriptNpc::send_say_msg` 的**缺省实现保留**（后者签名改为 `-> Option<HearSay>`，缺省 `None` = no-op）；新增 `HearSay { text, f_color, b_color }` |
| `crates/script/src/engine/actions.rs` | case 4 改为 `if let Some(say) = self.npc.send_say_msg(&s_msg) { self.player.send_msg(RM_HEAR, 0, f, b, 0, Some(&text)) }` —— **本例玩家**那一发压进他自己的帧表（入队序 = 动作序） |
| `bins/game-svr/src/npc.rs` | `NpcPlayer::world_broadcast`（前缀 + `SM_SYSMESSAGE` 帧 + `world_broadcasts` 扇出清单）；`HostNpc::send_say_msg`（`FilterShowName + ':' + msg`，`has_envir == false` ⇒ `None`）；`SysMsgCfg.hear`、`BroadcastCfg`、`run_goto_label(..., has_envir)` |
| `bins/game-svr/src/pickwire.rs` | `RM_HEAR` ⇒ `SM_HEAR(40)`（`Recog = npc_actor_id`、`Param = MakeWord(n_param1, n_param2)`） |
| `bins/game-svr/src/lib.rs` | `fan_out_world_sysmessage` / `fan_out_npc_hear` / `caller_within_hear_range` / `script_hear_says` / `drain_script_frames_with_host`；接线点：点击 `@main`、1011、`@back`、修理、QFunction（`run_party_goto_label`）、**`@StdModeFunc*`（`run_std_mode_func_script`，返工 I-1）** |
| `bins/game-svr/src/guild_official.rs` | `FixedNpcScript::take_script_broadcasts`（缺省空）+ `handle_select` 的扇出 |

### 3.1 排水序（本卡的关键点）

oracle 的每条帧都进**收件人自己的** `MsgQueue`（`PriorityQueue<SendMessage, byte>`，优先级 = `(byte)wIdent`），
tick 末按 .NET 8 **四叉最小堆**出队 ⇒ 同 ident 多帧**不是 FIFO**（3 条 `[0,2,1]`、4 条 `[0,3,2,1]`；
卡 `openmir2-frame-drain-order-audit` 的结论，实现 = `crates/world/src/msgqueue.rs::NetPriorityQueue`）。

本卡的三个位置都按这条口径：

1. **本例玩家自己**：`world_broadcast` 把 `SM_SYSMESSAGE`（ident 11004 & 0xFF = **252**）与
   `send_say_msg` 的 `RM_HEAR`（11001 & 0xFF = **249**）都压进 `NpcPlayer.frames` ⇒ 与同一次脚本的
   其它帧共用**同一个** `NetPriorityQueue`（`emit_script_frames_raw`）出队，入队序 = 动作序。
2. **其余玩家（广播）**：`fan_out_world_sysmessage` **逐收件人**各建一个 `NetPriorityQueue` 灌入
   扇出正文再出队（同 ident 252）。
3. **其余玩家（NPC 喊话）**：`fan_out_npc_hear` 同上（同 ident 249），收件人 = `chat::hear_targets`
   （±`SendRefMsgRange`、`Race == Play && !Ghost`）。

**本例玩家的边界**：`SENDMSG 4` 时若他不在 NPC 的 ±12 方格内（`CM_CLICKNPC` 允许 ±15），
从帧表里**摘掉** `RM_HEAR`（`caller_within_hear_range`）——oracle 的方格扫描不会给他入队。

## 4. 部署可达性计数（可复跑）

```sh
cd /e/MirServer/M2GameSvr/Envir
grep -ahoiE '^[[:space:]]*SENDMSG[[:space:]]+[0-9]' -r . --include='*.txt' | wc -l          # 806
grep -ahoiE '^[[:space:]]*SENDMSG[[:space:]]+[0-9]' -r . --include='*.txt' \
  | grep -oiE '[0-9]+$' | tr -d ' ' | sort | uniq -c | sort -rn                             # 1=246 7=211 6=176 0=72 5=70 4=31
grep -ahoiE '^[[:space:]]*SENDMSG[[:space:]]+[0-3][[:space:]]' -r . --include='*.txt' | wc -l  # 318（case0-3）
grep -ahoiE '^[[:space:]]*SENDMSG[[:space:]]+4[[:space:]]' -r . --include='*.txt' | wc -l      # 31（case4）
```

实测（2026-10-23）：总 **806**；**case0-3 = 318**（走 `world_broadcast`）、**case4 = 31**（走 `send_say_msg`）
⇒ 本卡覆盖 **349 行**；case5/6/7（457 行）走 `sys_msg`（此前已实现）。

**调用点分布**（决定接线范围）：

- `SENDMSG 4`：`Market_Def/新手接待员-0.txt`、`Market_Def/赌博NPC/赌场-B013.txt`、
  `QuestDiary/游戏触发/{一,五,十}筹码.txt` ⇒ **商人漏斗**（Envir 非 null）✓ 已接线。
- `SENDMSG 0-3`：`Market_Def/QFunction-0.txt`（`@PlayDie` 4 条 case1）、各 `Market_Def/功能NPC/*`、
  `QuestDiary/*` ⇒ 商人/固定 NPC/QFunction 漏斗 ✓ 已接线。
- `MapQuest_def/QManage.txt` 的 10 条 SENDMSG = case5×9 + case7×1（`[@Resume]`/`[@Member]`）⇒ `@Login`
  漏斗今日无 0-4 调用点（**仍未接线**的排水点，登记 T-224 ④）。
- `@StdModeFunc*`：**语料订正**（返工 I-2）——`Market_Def/QFunction-0.txt:107` 的 `[@StdModeFunc14]`
  的 `#ACT` 是**无条件 `SENDMSG 0`**（不是 6/7），经 `handle_eat_cmd` 的
  `std_mode == 31 && ani_count != 0`（`PlayObject.Base.cs:1072`）**可达** ⇒ 返工 I-1 已接线该排水点
  （**改前**：触发者收到、其余在线玩家 **0 帧**；**改后**：全服每个 `!Ghost` 玩家各一发）。
  同文件 `:388` 的 `[@UserCmd6]` 的 `SENDMSG 0` 不可达（oracle `UserCmdCommand` 三段 `GotoLable` 全注释）。
  可复跑：`awk 'BEGIN{lbl=""} /^\[/{lbl=$0} /^[[:space:]]*SENDMSG[[:space:]]+[0-4]([[:space:]]|$)/{print NR": "lbl" -> "$0}' Market_Def/QFunction-0.txt`

## 5. 判据（用例 9 / 牙 4）

`cargo test -p game-svr --lib sendmsg`（9 条，全绿）：

| 用例 | 钉住什么 |
|---|---|
| `sendmsg_case0_world_broadcast_reaches_every_online_player` | 全服每个 `!Ghost` 玩家各一条 `SM_SYSMESSAGE`，`Recog` **各取自己**；逐字节 |
| `sendmsg_case1_prefix_is_star_paren_space_byte_exact` | 前缀 `"(*) "` 逐字节 |
| `sendmsg_case2_prefix_is_npc_name_in_brackets` | `[寄售商]`（NPC 名，不是玩家名） |
| `sendmsg_case3_prefix_is_player_name_in_brackets` | `[hero]`（玩家名） |
| `sendmsg_case4_npc_say_is_sm_hear_around_npc_only` | `SM_HEAR(40)`、`Recog` = **NPC** 的 `ActorId`、`Param = 0x00FF`；±12 格内（本例玩家 + near）各一条，**方格外的 far 零帧** |
| `sendmsg_text_replaces_percent_s_and_percent_d` | `%s` → ChrName、`%d` → normNpc.ChrName（多处全替换） |
| `sendmsg_same_ident_frames_dequeue_in_four_ary_heap_order` | 3 条同 ident 出队 = **`[0,2,1]`**（`a,c,b`），不是 FIFO |
| `sendmsg_case4_hidden_npc_emits_nothing` | 隐藏 NPC（`Envir == null`）⇒ `SendSayMsg` 零帧 |
| `sendmsg_stdmodefunc14_broadcast_reaches_other_online_player` | **返工 I-1**：`@StdModeFunc14` 的 `SENDMSG 0` 经 `handle_eat_cmd` 触发 ⇒ **另一个在线玩家**也收到 `SM_SYSMESSAGE`（`Recog` 各取自己），触发者收到的字节与之一致 |

牙（逐字节还原 + 提交后树的 sha256 见 patch body）：

1. `actions.rs` case1 的 `"(*) "` 改错 ⇒ case1 红；
2. `NpcPlayer::world_broadcast` 覆写删回缺省 no-op ⇒ case0 红；
3. `HostNpc::send_say_msg` 的 `if !self.has_envir { return None }` 删掉 ⇒ 隐藏 NPC 用例红；
4. **返工 I-1 牙**：把 `run_std_mode_func_script` 里 `fan_out_world_sysmessage(...)` 的正文实参换成
   `&[]`（= 关掉扇出）⇒ `sendmsg_stdmodefunc14_broadcast_reaches_other_online_player`
   **0 passed / 1 failed**（`left: 0, right: 1`）；`git checkout --` 还原后 sha256 与提交树一致。

## 6. 窗口项（R，**未跑**；可复跑命令）

前置照 `docs/window-runbook-20261023.md` §1（重建 `mir2-server` 并记 sha256、十端口全 DOWN、
两侧库各自复位、ident 先在 `tools/conformance/rules.json` 登记：本卡用 **100 / 40**）。

**W-1（case0-3 全服广播）**：双栈各一次，同一角色在有第二观察者在线时触发带 `SENDMSG 1` 的脚本。

```powershell
# 每侧各跑一次（<side> = csharp | rust），影子 SelGate 抓下行
python -I -B tools/gw-capture/gate_shadow_proxy.py --out E:/tmp/w224/<side>/sel `
  --map 7101=127.0.0.1:7100 --gate sel --direction s2c
# 客户端登录端口指向 7101，触发脚本后停，再对账
python -I -B tools/gw-capture/gate_shadow_proxy.py --verify E:/tmp/w224/<side>/sel
```

判读：两侧 `frames.ndjson` 里 `ident=100` 的帧，`Recog`（12B 头 `Recog` 字段）= **收件人自己**、
`Param` = `MakeWord(RedMsgFColor, RedMsgBColor)`（现网 `0xFF/0x38`）、`Series = 1`、尾 `DecodeString`
= 脚本正文（case1 带 `"(*) "` 前缀）；**收件人集合** = 全服所有非 Ghost 在线玩家（含触发者）。
场景：部署 `Market_Def/QFunction-0.txt [@PlayDie]` 的 4 条 `SENDMSG 1` 在 Rust 无 `@PlayDie` 触发点 ⇒
用**构造级**场景（临时 NPC 脚本 `SENDMSG 1 <$KILLER> 测试`，两侧同脚本）替代，并在报告里写明是构造场景。

> **§4.6 提醒**：`SENDMSG` 的正文走 `GetLineVariableText`，但命名变量分支是死代码（whitelist B-9）
> ⇒ 部署行里的 `<$KILLER>`/`<$USERNAME>` 在两侧**都是字面量**，不要期待被替换；`%s`/`%d` **会**替换。

**W-2（case4 NPC 喊话）**：同一场景触发 `SENDMSG 4`，在 NPC 的 11 / 12 / 13 格各放一个静止号。

```powershell
python -I -B tools/gw-capture/gate_shadow_proxy.py --out E:/tmp/w224/<side>/sel4 `
  --map 7101=127.0.0.1:7100 --gate sel --direction s2c
python -I -B tools/gw-capture/gate_shadow_proxy.py --verify E:/tmp/w224/<side>/sel4
```

判读：`ident=40`（`SM_HEAR`）的 `Recog` = **NPC 的 ActorId**、`Param = MakeWord(HearMsgFColor,
HearMsgBColor)`（现网 `0x00/0xFF`）、尾 = `FilterShowName(NPC名) + ':' + 正文`；
收件人 = |dx| ≤ 12 且 |dy| ≤ 12（oracle `<=`，11/12 格**收**、13 格**不收**）。
场景：`Market_Def/新手接待员-0.txt` 的 `SendMsg 4 <$USERNAME>，欢迎来到[<$SERVERNAME>]`。

> **口径（冷路径）**：「11/12 收、13 不收」只在 oracle 的 `VisibleHumanList` **冷路径**成立；
> 缓存命中支的边界是 `|dx| < 11 && |dy| < 11`（`BaseObject.Message.cs:395,453-460`）。
> 本仓 `chat::hear_targets` / `hear_targets_at` **恒走 ±12 冷路径**（全仓已登记偏差 T-154 ④、D-13、
> `crates/world/src/session.rs:40`）⇒ 窗口对账必须取**该 NPC 的首次发话**（同一 NPC 500ms 内第二次
> 起 C# 侧会命中缓存、Rust 侧仍是冷路径，11/12 格的边界帧会不一致，属已登记偏差、不是本卡回归）。

**W-3（同 ident 多帧的到达序）**：构造脚本连发 3 条 `SENDMSG 0 a|b|c`，两侧比 `ident=100` 的
出现次序必须是 `a, c, b`（四叉堆 `[0,2,1]`）；4 条是 `[0,3,2,1]`。

构造脚本（窗口期临时件，跑完删）：`E:/tmp/w224/script/ZZ-W224-3.txt`（装进 `Market_Def` 临时 NPC，
`[@main]` 的 `#ACT` 依次三行 `SENDMSG 0 a` / `SENDMSG 0 b` / `SENDMSG 0 c`；两侧**同一份脚本**）。

```powershell
# 每侧各跑一次（<side> = csharp | rust），影子 SelGate 抓下行
python -I -B tools/gw-capture/gate_shadow_proxy.py --out E:/tmp/w224/<side>/sel3 `
  --map 7101=127.0.0.1:7100 --gate sel --direction s2c
# 触发脚本（CM_CLICKNPC → @main）后停，再对账
python -I -B tools/gw-capture/gate_shadow_proxy.py --verify E:/tmp/w224/<side>/sel3
# 对账过滤：只取 ident=100（SM_SYSMESSAGE）的帧，按 frames.ndjson 的到达序打印帧指纹
python -I -c "import json,sys;rows=[json.loads(l) for l in open(sys.argv[1],encoding='utf-8') if l.strip()];print([r['sha256'][:12] for r in rows if r.get('ident')==100])" E:/tmp/w224/<side>/sel3/frames.ndjson
```

判读：两侧 `ident=100` 的指纹序列**逐项相等**；把三帧尾部（帧偏移 12 起的 EDCode 串）依次解码
必须得到 `a, c, b`（**不是** `a, b, c`）⇒ 四叉堆 `[0,2,1]` 与 oracle `NetPriorityQueue` 一致。

## 7. 残余物清单

- 临时脚本：`E:/tmp/wt-worker-73-*.py`（实现期用的打补丁脚本，非仓库物，窗口跑完后可删）；
- 本卡**未起栈、未占端口、未连库**；`src/**` 未改。
