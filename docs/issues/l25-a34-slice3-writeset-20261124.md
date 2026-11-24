> 原路径 E:/UserTemp/a34-slice3-writeset.md（2026-11-24 入仓，内容未改）
> 入仓时锚已规范化（oracle `src/**` 行号保留、本仓高 churn 文件改符号锚），结论与事实未改；原文锚对应 `250c8a97d`。
# A34 片 3（交易 6 / 组队 4，仅同区）——写集与依赖地图

> 只读准备件（不写码、不编译）。配套：`opc-27` 2026-11-24 开工前置。
> 基准：master `828dc273b`（A33 已合）；本文件只在 `E:/UserTemp`，不入库。
> oracle = 主检出 `E:/Users/gxh/Documents/GitHub/OpenMir2/src`（C# 参照实现）；
> Rust = `mir2-rs/`（当前树 = a33-fanout 分支，与 master 一致于相关文件）。

---

## 0. 现状（区 k 行为）

- 分类器 `zone_cmd_class`（`bins/game-svr/src/lib.rs`，函数在 `~28904`）对表外 ident 一律
  `Unsupported`；`CM_DEALTRY` 在注释里被点名当例子。
- 区 k 执行体 `handle_zone_command_on`（`~1970`）尾部 `note_zone_cmd_unsupported(cmd.ident)`
  + `ZoneCmdOutcome::Unsupported` ⇒ 命令**丢弃 + 计数**（`zone_cmd_unsupported`），无帧。
- 需要进程态的族（脚本/商店等）走 A6 控制邮箱；交易/组队**目前既不 Local 也不 Process**。

---

## 1. 逐命令归属表（10 行）

字段口径：`CM_*` 的 12B 头 `recog/param/tag/series`；组队名字在尾 `Msg`（`.Trim()`），
`CM_GROUPMODE` 判 `nParam2` = 12B 头的 **Param** 字段。

| # | ident / 名字 | oracle 处理器（`src/**`） | 区 k 现状 | 读写的区状态 | 仅同区成立？ |
|---|---|---|---|---|---|
| T1 | `CM_DEALTRY(1025)` | `PlayObject.Message.cs:945` → `src/M2Server/Player/PlayObject.Operate.cs:911` `ClientDealTry` | Unsupported（丢弃 + 计数） | `trade_sessions[me]`（dealing/deal_last_tick/is_can_deal）、**前排实体**（`GetPoseCreate()`，`world.entities`）、**对方** `trade_sessions`/实体（AllowDeal/IsCanDeal/Dealing）、`online`（id→chr）、`gates`；写：双方 dealing/deal_creature/deal_last_tick | **有条件**。oracle：`poseObject.Race == Play` 才继续；`GetPoseCreate()==null` ⇒ NRE 被吞（零帧）；`poseObject==this` ⇒ 零帧；非玩家 ⇒ `SM_DEALTRY_FAIL`（`src/M2Server/Player/PlayObject.Operate.cs:958-960`）。区 k 风险：前排是**镜像**（他区玩家）时 `is_play_actor` 为真、而本地 `online` 无其人 ⇒ 现在会拿 **dummy DealState（默认 `allow_deal=true/is_can_deal=true`）** 误开交易并写 mirror 实体。**必须加守卫**（见 §2 写集）。 |
| T2 | `CM_DEALADDITEM(1026)` | `src/M2Server/Player/PlayObject.Message.cs:948` → `src/M2Server/Player/PlayObject.Operate.cs:964` | 同上 | 双方 `trade_sessions`（deal_item_list/deal_last_tick）、双方 `ground_sessions`（item_list）、`catalog`；写：本侧 item_list 移入 deal_item_list | **是**：只作用 `DealCreat`（T1 建立的本区对手）；无 DealCreat/非 dealing ⇒ 零帧（`trade.rs` 门）。 |
| T3 | `CM_DEALDELITEM(1027)` | `src/M2Server/Player/PlayObject.Message.cs:951` → `src/M2Server/Player/PlayObject.Operate.cs:1001` | 同上 | 同 T2（反向：deal_item_list 退回 item_list） | **是**（同上）。 |
| T4 | `CM_DEALCANCEL(1028)` | `src/M2Server/Player/PlayObject.Message.cs:954` → `src/M2Server/Player/PlayObject.Operate.cs:1044` `ClientCancelDeal` | 同上 | 双方 `trade_sessions`（dealing→false、deal_creature→None、退物品、deal_last_tick）、`ground_sessions` 双方；帧 `SM_DEALCANCEL`（每侧一条）+ SysMsg | **是**（递归取消双方，均本区）。A35-b 跨区前取消已复用此路径（`deal_cancel_between`）。 |
| T5 | `CM_DEALCHGGOLD(1029)` | `src/M2Server/Player/PlayObject.Message.cs:957` → `src/M2Server/Player/PlayObject.Operate.cs:1049` | 同上 | 双方 `trade_sessions.deal_golds`、`ground_sessions.gold` 上限检查（`cfg.human_max_gold`）；帧 双方 | **是**。 |
| T6 | `CM_DEALEND(1030)` | `src/M2Server/Player/PlayObject.Message.cs:960` → `src/M2Server/Player/PlayObject.Operate.cs:1085` | 同上 | 容量四检（`MaxBagItem`/背包/负重）、**物品与金币在双方 `ground_sessions` 间转移**、双方 dealing 复位；帧 OK/FAIL + SysMsg。**无 DB 请求**（存档仍走周期/断线） | **是**。 |
| P1 | `CM_GROUPMODE(1019)` | `src/M2Server/Player/PlayObject.Message.cs:912` → `src/M2Server/Player/PlayObject.Operate.cs:787` `ClientGroupClose`（param2==0）/ 就地置 `AllowGroup=true` | Unsupported | 本区实体的 `allow_group`/`group_owner`/`group_members`（world-bevy `Entity`）、`world.party_cfg`；帧 `SM_GROUPMODECHANGED`；`FunctionNPC.GotoLable("@GroupClose")` | **是**（不涉及他区）。区 k 的 GotoLable 见限制 §4。 |
| P2 | `CM_CREATEGROUP(1020)` | `src/M2Server/Player/PlayObject.Message.cs:930` → `src/M2Server/Player/PlayObject.Operate.cs:810` `ClientCreateGroup` | Unsupported | `GetPlayObject(名字)`（**本区实体按名**）、双方 group_owner/group_members/allow_group、`party_cfg.group_members_max`；帧 `SM_CREATEGROUP_OK/FAIL(-1..-4)`、双方成员表；GotoLable("@GroupCreate") | **有条件**：oracle 单世界总能按名找到；区 k 按名只在**本区世界**找。若目标是**镜像**，`party_play_object_by_name`（`mir2-rs/crates/world-bevy/src/party.rs` `party_play_object_by_name`）现在会命中（`race==PLAY && !ghost`，未排除 mirror）⇒ 要么错误帧（-4，因为 mirror 的 allow_group 恒 false），要么更糟（若将来 mirror 带 allow_group，会往镜像写 group 状态）。**必须加 `!e.mirror` 过滤**；过滤后找不到 ⇒ 与 oracle「GetPlayObject null」同形：`-2`。 |
| P3 | `CM_ADDGROUPMEMBER(1021)` | `src/M2Server/Player/PlayObject.Message.cs:933` → `src/M2Server/Player/PlayObject.Operate.cs:847` | Unsupported | 同 P2（加成员）；`GroupMembers.Count > cfg.group_members_max` 检查 | **有条件**：同 P2（`-2` + 必须排除镜像）。 |
| P4 | `CM_DELGROUPMEMBER(1022)` | `src/M2Server/Player/PlayObject.Message.cs:936` → `src/M2Server/Player/PlayObject.Operate.cs:885` | Unsupported | 同 P2（移除成员；`IsGroupMember` 门） | **有条件**：同 P2（`-2` + 必须排除镜像）。 |

**oracle 原文（目标不存在时的语义，用于「仅同区」判据）**：
- 组队（`src/M2Server/Player/PlayObject.Operate.cs:810/847/885`）：`IPlayerActor playObject = SystemShare.WorldEngine.GetPlayObject(sHumName);`
  随后 `if (playObject == null || playObject == this || playObject.Death || playObject.Ghost) { SendDefMessage(..._FAIL, -2, ...); return; }`
  ⇒ **找不到目标 ⇒ 失败帧 -2**，不跨图、不搜索其它世界。
- 交易 TRY（`src/M2Server/Player/PlayObject.Operate.cs:929-960`）：`IActor poseObject = GetPoseCreate();`
  `if (poseObject.Race == ActorRace.Play) { ... if (target.GetPoseCreate() == this && !target.Dealing) { if (target.AllowDeal && target.IsCanDeal) { 开窗 } else { SysMsg(PoseDisableDeal) } } else { SendDefMessage(SM_DEALTRY_FAIL) } } else { SendDefMessage(SM_DEALTRY_FAIL); }`
  ⇒ **前排非玩家/背对 ⇒ SM_DEALTRY_FAIL**；前排 null ⇒ NRE 被 `WorldServer.cs:732-735` 吞掉（零帧）。
- 交易后续 5 条：全部以 `DealCreat` 为对手（`src/M2Server/Player/PlayObject.Operate.cs:964/1001/1044/1049/1085`），无对手时早退零帧。

---

## 2. 写集文件清单（含与 5b 片 2 的冲突点）

### R1 `mir2-rs/bins/game-svr/src/lib.rs`（主要）

| 位置 | 改动 | 与 5b 片 2（商店 handlers） |
|---|---|---|
| `zone_cmd_class`（`~28904`） | `ident_level` 链尾追加 10 个 ident（Local） | **同函数**：5b 也往这条链尾加商店 ident ⇒ **冲突点**（append 区，机械冲突；按「只增不改」双向追加即可） |
| `handle_zone_command_on`（`~1970`） | ① 新增交易 6 臂（调 `handle_deal_cmd`，实参区本地：`&mut world`、`&*displays`、`&*online`、`&mut ground_sessions`、`&mut trade_sessions`、`&*gates`、`env.enter_cfg.trade`、`&env.enter_cfg.catalog`）；② 新增组队 4 臂（调 `handle_party_cmd`，`npc_host=None`、`gm=None`、`npc_vars=&NpcVars::default()`、`saves` 可变、`party_cfg=env.party_cfg`）；③ **参数 `trade_sessions: &HashMap` → `&mut HashMap`** | **同函数**：5b 也在此加商店臂 ⇒ **冲突点**；③的签名变更波及 5b 臂（它们多半只读 `&*trade_sessions`，兼容） |
| A6 执行闭包（`tick_phase_a` 的 `INBOUND_LOCAL`，`~38455`） | `&*trade_sessions` → `&mut *trade_sessions`（逐次重借） | 可能同函数（5b 若动该闭包传参）⇒ 小冲突点 |
| `handle_zone_command_with_tail_and_shared`（`~1784`）与 `#[cfg(test)]` 包装 `handle_zone_command` / `_with_tail` | 同步 `trade_sessions` 可变性；`shared` 已够（`party_cfg` 若加进 `SharedServices`） | 间接（调用面） |
| `handle_deal_cmd`（`~8103`） | **仅同区守卫**：`other_id` 是玩家实体但 `other_chr == None`（本地 `online` 无此人，含镜像）⇒ 对 `CM_DEALTRY` 归 `DealTryTarget::NotPlayer`（`SM_DEALTRY_FAIL`）；其余 5 条无 DealCreat 早退，天然不串区 | 5b 不动（商店） |
| `ZoneCmdEnv`（`~1833`）+ 两处构造 | 增 `party_cfg: &'a mir2_world_bevy::PartyConfig` | 若 5b 也扩 env ⇒ 同函数小冲突 |
| `SharedServices`（`~36960` 附近） | 增 `party_cfg`（同源值；见下「单源方案」） | 5b 不动 |
| `ProcessServices` 字段/构造（`~37740`） | 若采用「迁移」方案：`party_cfg` 从 `ProcessServices` 移到 `SharedServices`，改 `svc.party_cfg` 读取点（`~38507` 一带） | 5b 不动（商店不用 party_cfg） |
| `new_zone_state` / `HostLoop::for_zone`（`~1103`/`~37026`） | 区 k 的 `World` 注入**真实** `party_cfg`（与 `need_exps` 同一注入点；否则区 k 的 `world.party_cfg` 是 `PartyConfig::default()`，组队前缀/颜色/上限与区 0 不一致 ⇒ 帧字节漂移） | 5b 不动 |
| `zone_cmd_class_*` 单测 + `handle_zone_command_on` 源码扫描测试（`~136810`） | 追加断言/同步扫描区间 | **同测试模块**，append 冲突可能 |
| 新增单测模块 | 10 条命令的「同区成功 / 对方不在本区失败」牙（见 §3） | 新增文件/模块，不冲突 |

**单源方案（推荐）**：`party_cfg` 迁到 `SharedServices`（纯数据：u8/String，`Clone` 便宜），
`ProcessServices` 不再持有；区 0 的 `SayEnv`/`handle_party_cmd` 读同一份。避免「两份配置漂移」。
若 `opc-27` 倾向最小 diff，也可在 `SharedServices` **克隆一份**、`ProcessServices` 保留（登记为
双份只读快照）；我倾向前者。

### R2 `mir2-rs/crates/world-bevy/src/party.rs`

| 位置 | 改动 | 理由 |
|---|---|---|
| `party_play_object_by_name`（`:434`） | 过滤加 `&& !e.mirror` | 镜像不是 oracle `PlayObjectList` 的成员；不加则按名命中他区玩家的镜像（错误帧 -4/潜在写镜像状态） |

### R3 不改

- `crates/game-svr/src/trade.rs`（纯状态机，按 `DealSide` 工作，无区感知）。
- `handoff.rs`（A35-b 的跨区前交易取消已覆盖：区 k 作为源区时 `book=None` 仅跳过寄售，交易取消照常）。
- `partywire`/`party` 其余函数（`party_group_mode/create/add/del` 都按本区实体 id 工作）。

### 依赖/前置

1. **A35-b 已合**：区 k 有真实会话/`trade_sessions`/`saves`（迁移会带上），本片才能谈「同区交易/组队」。
2. **A35-b 不搬 entity 的组队字段**：`MirrorSpec`/`promote_mirror_to_real` 不携带
   `group_owner`/`group_members`，跨区后目标区实体没有原组关系（源区实体已释放）。本片不做
   跨区组队，但这是**已知数据缺口**，须在卡残余里写清（见 §4）。
3. **5b 片 2 合并序**：本片与 5b 的同函数冲突点共 3–4 处（分类器链尾、执行臂区、闭包传参、
   env 扩字段），均 append 型；建议先合 5b，再合本片（谁后合谁用 merge 解，不 rebase）。

---

## 3. 牙草案（每命令一条「同区成功 / 对方不在本区失败」+ 变异）

统一夹具：`ZoneState::new(zone_k, …)` + `SharedServices`（测试注入 `party_cfg`/`catalog`），
`gates` 单通道；两名同区玩家 A/B（`online` + `world.add_entity` + `SessionGround`）；
第三名「他区镜像」C：在区 k world 里 `handoff_accept(MirrorSpec{id=C, race=PLAY})`（无 `online`）。

**交易**

1. `deal_try_same_zone_opens_both_dialogs`：A 面向 B、B 面向 A ⇒ A/B 各收 `RM_MENU_OK`(OpenedDeal)
   与开窗帧，双方 `trade_sessions.dealing=true`、`deal_creature` 互为对方。
   变异：删 `deal_try_target_of` 的 `faces_me` 计算 ⇒ 红。
2. `deal_try_facing_mirror_is_refused`（仅同区失败面）：A 面向 **C（镜像）** ⇒ **恰一条
   `SM_DEALTRY_FAIL`**，A 不入 trading、C 实体 `group_*`/会话不被写。
   变异：删 `handle_deal_cmd` 的「玩家但无本地 online ⇒ NotPlayer」守卫 ⇒ 误开窗（红）。
3. `deal_add_del_item_roundtrip`：TRY 成功后 ADD 一件 ⇒ 自己 `SM_DEALADDITEM_OK` + 对手
   `SM_DEALREMOTEADDITEM`；DEL ⇒ 物品回本侧 `item_list`。变异：不写回 trade_sessions ⇒ 红。
4. `deal_cancel_emits_one_each_side`：双方各恰一条 `SM_DEALCANCEL`、dealing 复位、物品回包。
   变异：只取消自己一侧 ⇒ 对手 0 条 ⇒ 红。
5. `deal_change_gold_caps_and_echo`：超 `human_max_gold` ⇒ 拒绝帧；正常 ⇒ 双方 `deal_golds` 同步。
   变异：跳过上限检查 ⇒ 红。
6. `deal_end_swaps_items_and_gold`：结算后物品/金币在双方 `ground_sessions` 间转移，dealing 复位；
   容量不足 ⇒ FAIL 帧且双方不变。变异：删容量四检 ⇒ 红。

**组队**

7. `groupmode_toggles_and_replies_once`：`param2!=0` ⇒ `allow_group=true` + 恰一条
   `SM_GROUPMODECHANGED(param=1)`；`param2==0` 且无组 ⇒ `param=0`。变异：不发帧 ⇒ 红。
8. `create_group_same_zone_ok`：A 邀 B（B `allow_group=true`）⇒ A 收 `SM_CREATEGROUP_OK(0)`、
   双方成员表；B 的 `group_owner==A`。变异：不写 B 的 group_owner ⇒ 红。
9. `create_group_missing_or_mirror_fails_minus_two`：目标名不存在 **或** 只命中 C 的镜像 ⇒
   **`SM_CREATEGROUP_FAIL(-2)`**（与 oracle GetPlayObject null 同形），镜像不被写。
   变异：删 `party_play_object_by_name` 的 `!e.mirror` ⇒ 命中镜像、帧变 -4 或写镜像（红）。
10. `add_del_member_same_zone_and_missing`：ADD 成功置成员/帧 OK，DEL 成功移除；目标缺失 ⇒
    对应 `_FAIL(-2)`。变异：DEL 不校验 `IsGroupMember` ⇒ 红。

每条牙同时断言 `zone_cmd_unsupported` **不增**（该 ident 已归 Local）。

---

## 4. 已知限制（必须原句进卡残余）

> **跨区交易/组队本片不做：区 k 玩家与区 0 玩家不能交易、不能组队。**
> - 交易：前排是**镜像**（他区玩家）时，区 k 回 `SM_DEALTRY_FAIL`（不发交易请求、不开窗）；
>   与 oracle 单世界的「同区即可交易」不同。
> - 组队：按名只在本区世界找**真实玩家**（镜像被排除），找不到 ⇒ `-2` 失败帧（与 oracle
>   `GetPlayObject` 为 null 同形）。
> - 区 k 的组队 `GotoLable`（`@GroupCreate/@GroupAddMember/@GroupDelMember/@GroupClose`）**不执行**
>   （区 k 无 `NpcHost`/`NpcVars`/`ScriptGmEnv`；oracle 的 `FunctionNPC != null` 支无对应物）。
> - A35-b 迁移**不携带**实体上的 `group_owner`/`group_members`：玩家跨区后原组关系在目标区不可见。
> 以上四条不得含糊；A36/后续卡若要补跨区交易/组队，需另立卡（涉及跨区状态同步与双方区锁纪律）。

---

## 5. 门禁与交付（照本仓规范）

- 三数：`cargo fmt --all -- --check`；`cargo clippy --workspace --all-targets -- -D warnings`；
  `cargo test -p game-svr --lib`（另按需 `-p mir2-world-bevy --lib`，因 R2 动了 `party.rs`）。
- 每条牙：快照 → 改 → 复跑红 → 还原（sha256 逐字节相等、无 MUTATION 残留）。
- 双推 origin+walgit，报 head + `git merge-base master HEAD`；不 push master、不 rebase。
- 合并序由 `opc-27` 定（建议 5b 片 2 先于本片）。
