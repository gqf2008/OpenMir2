# M3⑥-实现 3 第②片 —— 会话接线（@Login + SM_MERCHANTSAY(643) + NPC 脚本宿主装载）

- 卡：`openmir2-m3-quest-dialog-wiring`（M3⑥-实现 3；owner_hint = openmir2-worker-11）。
- 分支：`quest-dialog-wire`（base = 第①片 head `8c158d26`）。
- 上一片：第①片（离线核心：`NpcHost`/`NpcPlayer`/`HostNpc` + 说词行）已在【待审】`8c158d26`。

## 本片做什么（逐条对 oracle 锚，见 `01-oracle-anchors.txt`）

1. **进世界 @Login**（`PlayObject.cs:1438-1441`）：`sim_loop` 在进世界完成点（`enter::apply` 下发帧 +
   `MarkInWorld` 之后）调 `NpcHost::run_login`（QManage）。守卫 = `has_manage_npc()`（= oracle
   `SystemShare.ManageNPC != null`），未装载（脚本文件缺）⇒ 不跑。
2. **说词帧 `SM_MERCHANTSAY(643)`**（`PlayObject.Message.cs:1722-1726`）：
   `smframe::sm_merchantsay(recog, n_param1, n_param2, msg)`，尾 = `EncodeString(ChrName + '/' + sMsg)`。
   `recog` 按入队路径取人（`priority ⇒ 玩家 ActorId`；否则 ⇒ NPC ActorId，隐藏 NPC 恒 0）。
3. **出队序**（`BaseObject.Base.cs:25-32` + `MessagePriority`）：`High=0` 组先、`SendMsg` 组
   （`(byte)RM_MERCHANTSAY = 1`）后；`npc::dequeue_order` 只做这两个优先级的分组，组内取入队序。
4. **NPC 脚本宿主装载**：`sim_loop` 起动时消费 `MapLoader::load_map_quest`（隐藏 NPC：`QManage`/
   `QFunction`/`RobotManage`），`rename_rand` 走**同一条** `world.rng`（`ScriptParsers.cs:857`）。
5. **会话态 → `NpcPlayer` 快照**：`CharacterData` 的 `chr_name/actor_id/map/level/gold/bag/quest_flag`
   （`npc_player_from`）。脚本取数走临时移出的 `world.rng`（与 `step_mon_gen` 同法），用完归位。
6. **回写**：`Gold`（`SavedHuman.record.data.gold` + `SessionGround.state.gold`）与 `QuestFlag`
   （按记录原长度按位写回）。
7. **附带订正（第①片事实错误）**：`NpcPlayer.quest_flag` 由 `[u8;512]` 订正为 `[u8;128]`
   （`PlayObject.cs:1040` 是 `new byte[128]`）。
8. **配置**：`[Setup] ScriptGotoCountLimit`（`ServerConf.cs:320`；缺省 30，现网 10000）——
   `GameSvrConf.cs:542/1527`，`ScriptEngine.cs:629` 的 goto 守卫；`config.rs` 单独预扫该键
   （不并入大 `"SETUP"` match，避免 rustfmt 无关重排）。

## 判据与牙齿

- 用例 6 条：`npc::dequeue_order_puts_priority_high_first`、`npc::manage_and_function_npc_gates_require_loaded`、
  `smframe::sm_merchantsay_header_and_tail`、`main::emit_npc_says_priority_first_and_recog_by_path`、
  `main::npc_player_snapshot_maps_record_fields`、`config::setup_script_goto_count_limit_and_default`
  （另 5 条第①片既有用例仍绿）。
- **改坏必红 5 条**（`04-redchecks.txt`）：去掉出队分组 / recog 恒取玩家 / 去掉 `loaded` 守卫 /
  `Series` 1→0 / `ScriptGotoCountLimit` 缺省 30→0，逐条必红；还原后文件与备份 diff 为空。
- 真机两侧同序列（⑥-5 `run_quest_pair.ps1`）**本片未跑**（要窗口）⇒ 登记。

## 本片登记未做（不留暗债）

- `@StdModeFunc<N>`（`PlayObject.Base.cs:1072` + `useitem.rs` 的 `function_npc_present`）**仍是下一分片**：
  设 `function_npc_present=true` 而不跑脚本会让 StdMode 31 的物品「被吃掉但无脚本效果」⇒ 本片
  **故意保持 false**，待 `@StdModeFunc` 一起接（同 `CM_EAT` 帧序的混排）。
- `CM_CLICKNPC(1010)` / `CM_MERCHANTDLGSELECT(1011)` 派发与商店帧归 **⑤-4 `-shop-dialog-wiring`**
  （同派发表、同帧族）；本片只落隐藏 NPC 侧（@Login + 说词）。
- 回写只覆盖 `Gold`/`QuestFlag`；`QuestUnit/QuestUnitOpen/ItemList/LastNpc/Script/ScriptGotoCount`
  的写回登记为后续分片（`NpcPlayer` 的 `quest_unit*` 现为 `HashMap`，与 oracle `byte[128]` 的语义
  待随 `CM_CLICKNPC` 一起对齐）。
- `playerActor.GetScriptLabel(sMsg)`（`ScriptEngine.cs:990`）未建模（第①片已登记）。

## 窗口 / 端口

- **未起栈 / 未占端口**（判据全在构造级）。收工十端口保持开工前状态（仅共享 3306）。
