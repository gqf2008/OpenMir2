# run3 三场景前置分诊（2026-10-27，卡 `openmir2-window-run3-prereq-triage`）

> 只读分诊 + 离线修复；**未真实复跑**（窗口仍被本轮批次占用）。A4 待窗口。

## n12 `npc_script_chain`：discovery 找「老人」而角色被放在国王图

**现象**：`csharp/discover rc=1`，`没有对象匹配 name=老人`，可见对象只有 `比奇国王 (29,32)`。

**根因（地图/坐标错配，不是寻址机制问题）**：
- `tools/probe/scenarios/npc_script_chain.json:5` 的 placement 是 **map `0122` (29,33)**（比奇国王 (29,32) 的南邻格）；
- 而 `tools/window/run3-scenarios.json` 给 n12 的 discovery 是 `tools/probe/scenarios/npc_click_elder.json`，
  它 `click_npc: name:老人`；
- 「老人」是 **map `3`（盟重省）(669,338)** 的固定 NPC：`E:/MirServer/Mir200/Envir/Npcs.txt:5`
  （GBK：`老人 0 3 669 338 0 1`）。**0122 上根本没有老人** ⇒ 名字寻址 fail-closed，与日志一致。

**修复方向（需 owner 定，二选一）**：
1. **改 discovery 为 `npc_click_king.json`**——place 在 0122 就只该找国王；但国王是 `Npcs.txt` type=1
   ⇒ `GuildOfficial` 不实现 `IMerchant`，点它 **0 帧**（`docs/window-runbook-20261023.md` §10，
   `king_dialog`/`king_castle_war` 因此封存）⇒ 这条路会把场景变成「恒 0 帧」的 blocked 形态；
2. **改 placement 到 map 3 (669,339)**（老人的正南邻格）并保留 `npc_click_elder`——但 `npc_script_chain`
   后续步骤走的是 `@@buildguildnow`（建行会链），**必须先确认老人的脚本里有该标签**，否则场景语义整体错位。

⇒ 我**不擅自改**：两条路都不是纯夹具修复（一条把场景变成 blocked，一条可能改掉场景语义）。
卡上请 owner 裁决后再落；本文件记录证据与两条路的后果。

## n13 `equip_struck_save`：`save_settle no_change_observed (changes=0, 907ms)`

**现象**：probe 在 `CM_TURN`/`CM_HIT` 后等 `[642,203]` 3000ms 超时；随后 `save_settle` 判定
「require_changes=true 但 907ms 内无任何变化」⇒ UNDECIDABLE。

**根因**：两个判据是**同一条因果链**——没有 642（`SM_DURACHANGE`）就说明 `DoDamageWeapon` 没跑，
DB 自然无变化。`equip_struck_save.json` 的设计前提是「打身前商店 NPC 卫家店（map 0103 (19,18)）」，
而日志显示目标帧一条都没有 ⇒ 要么 placement 没落到 0103 (19,19)，要么 `Dir=Up` 的前方格不是目标。
**需要窗口期取 `frames.ndjson` 的 c2s 段**确认 `CM_TURN` 的 dir 与 `CM_HIT` 的 recog 是否指向该格；
离线只能确认「642 缺失 ⇒ DB 无变化」这条因果，无法确认落点。

## n17 `upgrade_retrieve`：背包夹具槽位 [40,41] 已被基线占用

**现象**：`inventory: 声明槽位里已有 2 行（[40,41]）——DELETE+INSERT 会把该行其它列重置，请改用空槽或先清空`。

**根因**：夹具声明的是**固定槽位**，而这两行在基线库里已有数据；`card_window` 的 inventory 准备
只做 DELETE+INSERT，会重置该行其它列 ⇒ 工具主动 fail-closed（**这是正确行为，不是 bug**）。

**修复方向**：把 profile/夹具里的槽位改成**基线里为空的槽**，或显式写「先清空该槽再插入」的流程。
两者都需要**读基线库确认哪些槽为空**——本轮窗口被占用、且卡片禁止连库写数据 ⇒ **待窗口**。
（离线能做的只有：把该约束写进 profile 注释，防止有人用「覆盖写」绕过。）
