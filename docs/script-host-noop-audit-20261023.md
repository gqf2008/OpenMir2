# 脚本引擎宿主缺省实现全量审计（`ScriptPlayer` / `ScriptNpc`）

- 卡：`openmir2-script-host-noop-audit`（tier=L，只审计 + 开卡，**零生产代码改动**）
- 作者：`openmir2-worker-63`（worktree `wt-worker-63` / 分支 `card/host-noop-audit`）
- **取值基线：`7b0e5955`（master + `card/frame-drain-order` 合并树）**。初版审计基线 `14864309e`；复审（`94718b41`）指出 frame-drain 给 `npc.rs` 加了 23 行导致 51 条 evidence 行号过时，本版已在合并树上重钉，并把自检门改成**按符号校验**（见 §4）。
- oracle = 本仓 `src/**`（C# 参照实现）；部署语料 = `E:/MirServer/M2GameSvr/Envir`（642 个文件、41473 行）
- 机器可读台账：`mir2-rs/tests/parity/coverage/script_host_noop_audit.json`；自检：`mir2-rs/tests/parity/coverage/script_host_noop_audit.py`

> 本卡只读生产代码：`crates/script/src/engine/mod.rs`、`crates/script/src/engine/{actions,conditions,vars,dispatch}.rs`、`bins/game-svr/src/npc.rs` 均未改动。本文引用的行号是**审计时**的实测值。

---

## 0. 结论摘要

`crates/script/src/engine/mod.rs` 共 **242** 个 `fn`；其中两个 trait 的**缺省实现**（带函数体、可被覆写）共 **168** 个：`ScriptPlayer` 165 个 + `ScriptNpc` 3 个（其余 74 个是必需方法/自由函数）。按「缺省实现是否会在正式宿主 `NpcPlayer`/`HostNpc` 上产生与 oracle 的静默差异」分三态：

| 结论 | 个数 | 含义 |
|---|---|---|
| **已有卡承接** | 7 | 已有在途卡覆盖（`openmir2-script-sendmsg-host` / `openmir2-gm-script-host-seam`），本卡只登记、不动 |
| **需开卡** | 107 | 正式宿主未覆写且缺省实现与 oracle 有静默差异（no-op / 恒值），其中 P1（有效可达 ≥100 行）12 个 |
| **安全** | 54 | 宿主已覆写 51 / 语义等价 2 / 既有近似 1（近似项在下文单独标注，不是「与 oracle 等价」） |

**有效可达计数最高的 5 条（不含已有卡承接项）**：

| # | 方法 | 部署计数（行） | 一句话后果 |
|---|---|---|---|
| 1 | `action_line_variable_text` | 1260 | 动作/条件参数里的 $ 变量一个都不展开（ProcessingBase.GetVariableText 缺省空实现），发出的文本保留字面 <$XXX>；本方法 direct 命令集合内含 `<$` 的 396 行（全树「命令行且含 <$」538 行）。 |
| 2 | `envir_root` | 607 | HostNpc 未覆写 envir_root() ⇒ 恒 None：CHECKNAMELIST/CHECKACCOUNTLIST 恒 false、ADDNAMELIST/DELACCOUNTLIST/ADDUSERDATE 等文件类动作恒按「文件不存在」记错且不写盘。宿主 NpcHost 已有 envir_root 字段，缺的是把它传进 HostNpc。 |
| 3 | `dyn_var_get` | 486 | $HUMAN(...)/$GLOBAL(...)/$ACCOUNT(...) 动态变量恒读不到（None）、写不进（false）；VAR/LOADVAR/SAVEVAR/CALCVAR 的变量分支失效。NpcPlayer 无任何 dyn_var 存储。 |
| 4 | `user_account` | 476 | UserAccount 恒空串 ⇒ CHECKACCOUNTLIST（238 行）恒 false、DELACCOUNTLIST（238 行）删不到任何行。NpcPlayer 无 account 字段。 |
| 5 | `dyn_var_set` | 352 | $HUMAN(...)/$GLOBAL(...)/$ACCOUNT(...) 动态变量恒读不到（None）、写不进（false）；VAR/LOADVAR/SAVEVAR/CALCVAR 的变量分支失效。NpcPlayer 无任何 dyn_var 存储。 |

**本卡开出的 4 张子卡**（按计数排序，签名 issue）：

| 子卡 id | issue oid | 覆盖方法 | 计数 |
|---|---|---|---|
| `openmir2-script-action-var-text` | `46efbcb26f7e783b1631e6e0165d333f62e210aa` | `action_line_variable_text` / `action_variable_text` | 1260 |
| `openmir2-script-envir-root-host` | `8b8be5e4b088d39ff8744ed1f0c5a0e944ad845b` | `envir_root`（`ScriptNpc`） | 607 |
| `openmir2-script-dyn-var-host` | `04a461726bf9255ba87fa2d673e9f79a46ae0efc` | `dyn_var_get` / `dyn_var_set` / `dyn_var_insert` | 894 |
| `openmir2-script-user-account-host` | `9bdc52f0c3009401ecdc6b890690868636c7826f` | `user_account` | 476 |

> 注：`action_line_variable_text` 的 1260 是「调用该宿主的动作/条件句柄行数」；其中**本方法 direct 命令集合内文本真含 `<$`** 的只有 **396** 行（§1.2 有复算口径）——变量展开缺省实现只在后者上产生可见差异。`envir_root`/`dyn_var_*` 的计数是**间接**（经 `check_string_list` / `get_mov_data_dynamic_var_value` 等 helper）触达。子卡表的计数是**组内各方法有效可达之和**（同一命令在多个方法上被重复计入，不是并集大小）。

---

## 1. 方法与口径

### 1.1 三态判据

- **已有卡承接**：卡片线程里已有在途卡（`card_claim`/`status: in-progress`）明确承接该宿主方法 ⇒ 本卡只登记、**绝不改**。
- **需开卡**：`NpcPlayer`/`HostNpc` **未覆写**，且缺省实现（空实现 / 恒值）与 oracle 在该方法被调用时的语义不同 ⇒ 静默走错分支或丢副作用。**可空转的 getter 也要判，不许因为「恒 0」一律算安全**（§3 逐条给 oracle 何时非零）。
- **安全**：① 正式宿主已覆写（缺省实现不会被使用）；或 ② 缺省实现本身就按 oracle 逐行实现（例如 `is_enough_bag` 委托给已覆写的 `bag_count()`），或 oracle 对应分支体已被注释（`config_renewlevel_clear_exp`）。

### 1.2 可达性计数口径（可复算）

命令 → 处理器 → 宿主方法 的映射由 `crates/script/src/handlers.rs`（C# 反射表）+ `crates/script/src/codes.rs`（命令名 → 枚举值）机械对齐：解析器产出的 `CmdCode = field_index`（S3/B-8 后不再减一），注册表键同为枚举值 ⇒ **脚本命令 `X` 命中的是 `HANDLERS[enum_value(X)]`**。

- **direct_count**（主口径）：在 `engine/actions.rs` / `engine/conditions.rs` 的 `match handler { ... }` 臂体内**直接**出现 `.<method>(` 的处理器，取其脚本命令在 `E:/MirServer/M2GameSvr/Envir` 全树中**行首命中数**之和。
- **indirect_count**（次口径）：仅当 direct 为 0 时使用——从臂体出发沿引擎内函数/方法调用做传递闭包（例如 `check_string_list` / `get_mov_data_dynamic_var_value` 这类 helper 才触达的方法）。
- **有效可达 = direct 非零取 direct，否则取 indirect**。

复算（部署计数）：

```sh
# 每个脚本命令的行首命中数（GBK 解码后按 ^[ \t]*<CMD> 计数）
python -I - <<'EOF'
import os,re,collections
ROOT="E:/MirServer/M2GameSvr/Envir"
pat=re.compile(r"^[ \t]*([A-Za-z_][A-Za-z0-9_]*)")
c=collections.Counter()
for dp,_,fns in os.walk(ROOT):
    for fn in fns:
        txt=open(os.path.join(dp,fn),"rb").read().decode("gbk","ignore")
        for line in txt.split("\n"):
            m=pat.match(line)
            if m: c[m.group(1).upper()]+=1
print(c["SENDMSG"], c["GIVE"], c["CHECKACCOUNTLIST"])  # 806 479 238
EOF
```

SENDMSG 的 `nParam1` 分档（决定命中的宿主）：`0`=72、`1`=246、`4`=31、`5`=70、`6`=176、`7`=211 ⇒ case 0-3 = **318**（`world_broadcast`）、case 4 = **31**（`send_say_msg`）、case 5-7 = **457**（`sys_msg`，已覆写）、case 8/9 = **0**。这与卡面证据「case 0-4 = 349 行零帧」一致。

`action_variable_text` 缺省空实现的**可复算影响面**（复审 `94718b41` finding-2 口径）：本方法 direct 命令集合内「命令行且含 `<$`」= **396** 行（`SENDMSG` 378 + `CHECKONLINE` 14 + `MESSAGEBOX` 4）；全树「命令行且含 `<$`」= **538** 行（含不走本路径的 `GMEXECUTE` 等，`ActionOfGmExecute` 直接拼 `sParam1..6`、不调 `GetLineVariableText`，不得计入）；全树含 `<$` 的所有行 = **694**。初版写的 534 混合了三种口径且不可复算，本版已撤换。

---

## 2. 台账主表（全量 168 行）

按结论分组：§2.1 已有卡承接（7）、§2.2 需开卡（107，按有效可达降序）、§2.3 安全（54）。

### 2.1 已有卡承接（7）

| # | 方法 | trait | 缺省语义 | oracle 对应实现 | 可达行数 | NpcPlayer 覆写 | 结论 |
|---|---|---|---|---|---|---|---|
| 1 | `guild_send_msg` | player | 空实现（no-op） | Processings/ExecutionProcessingSys.cs:3199 | 806 | **否** | 已有卡 `openmir2-script-sendmsg-host` |
| 2 | `send_group_text` | player | 空实现（no-op） | Processings/ExecutionProcessingSys.cs:3194 | 806 | **否** | 已有卡 `openmir2-script-sendmsg-host` |
| 3 | `send_say_msg` | npc | 空实现（no-op） | Processings/ExecutionProcessingSys.cs:3182 | 806 | **否** | 已有卡 `openmir2-script-sendmsg-host` |
| 4 | `world_broadcast` | player | 空实现（no-op） | Processings/ExecutionProcessingSys.cs:3170 | 806 | **否** | 已有卡 `openmir2-script-sendmsg-host` |
| 5 | `permission` | player | 恒 0 | Processings/ConditionProcessingSys.cs:3894 | 147 | **否** | 已有卡 `openmir2-gm-script-host-seam` |
| 6 | `process_user_line_msg` | player | 空实现（no-op） | Processings/ExecutionProcessingSys.cs:4942 | 139 | **否** | 已有卡 `openmir2-gm-script-host-seam` |
| 7 | `set_permission` | player | 空实现（no-op） | Processings/ExecutionProcessingSys.cs:4941 | 139 | **否** | 已有卡 `openmir2-gm-script-host-seam` |

**已知三条现状点名**：

- `world_broadcast`（`mod.rs:387`，case 0-3 = 318 行）与 `send_say_msg`（`mod.rs:742`，case 4 = 31 行）：`NpcPlayer`/`HostNpc` 均未覆写 ⇒ SENDMSG case 0-4 共 349 行零帧。卡 `openmir2-script-sendmsg-host` 已由 worker-23 认领在途。
- `send_group_text`（`mod.rs:389`）/`guild_send_msg`（`mod.rs:406`）：同为 `ActionOfLineMsg` 的 case 8/9；部署语料这两个 case **各 0 行**，但仍属同一缺口（同一张卡覆盖）。
- `process_user_line_msg`（`mod.rs:549`）/`set_permission`（`mod.rs:547`）：GMEXECUTE 抬权跳线的两个前置，卡 `openmir2-gm-script-host-seam`（worker-62）在途。`permission`（`mod.rs:185`，恒 0）是该链的读侧，一并登记在同一张卡下。

### 2.2 需开卡（107，按有效可达降序）

| # | 方法 | trait | 缺省语义 | oracle 对应实现 | 可达行数 | NpcPlayer 覆写 | 结论 |
|---|---|---|---|---|---|---|---|
| 1 | `action_line_variable_text` | player | 按 oracle 骨架展开（回调 action_variable_text） | ProcessingBase.cs:1237 | 1260 | **否** | 需开卡（P1） → `openmir2-script-action-var-text` |
| 2 | `envir_root` | npc | 恒 None | ProcessingBase.cs:1829 | 607 | **否** | 需开卡（P1） → `openmir2-script-envir-root-host` |
| 3 | `dyn_var_get` | player | 恒 None | ProcessingBase.cs:898 | 486 | **否** | 需开卡（P1） → `openmir2-script-dyn-var-host` |
| 4 | `user_account` | player | 恒空串 | ProcessingBase.cs:248 | 476 | **否** | 需开卡（P1） → `openmir2-script-user-account-host` |
| 5 | `dyn_var_set` | player | 恒 false | ProcessingBase.cs:921 | 352 | **否** | 需开卡（P1） → `openmir2-script-dyn-var-host` |
| 6 | `config_show_script_action_msg` | player | 恒 false | Processings/ExecutionProcessingSys.cs:2655 | 175 | **否** | 需开卡（P1） |
| 7 | `find_magic` | player | 恒 None | Processings/ExecutionProcessingSys.cs:2641 | 165 | **否** | 需开卡（P1） |
| 8 | `is_training_skill` | player | 恒 false | Processings/ExecutionProcessingSys.cs:2644 | 165 | **否** | 需开卡（P1） |
| 9 | `learn_magic` | player | 空实现（no-op） | Processings/ExecutionProcessingSys.cs:2653 | 165 | **否** | 需开卡（P1） |
| 10 | `use_items_upper_bound` | player | 恒 -1 | Processings/ConditionProcessingSys.cs:4378 | 133 | **否** | 需开卡（P1） |
| 11 | `worn_item_add_value` | player | 恒 None | Processings/ConditionProcessingSys.cs:414 | 128 | **否** | 需开卡（P1） |
| 12 | `space_move` | player | 空实现（no-op） | Processings/ExecutionProcessingSys.cs:2178 | 125 | **否** | 需开卡（P1） |
| 13 | `set_game_point` | player | 空实现（no-op） | Processings/ExecutionProcessingSys.cs:3110 | 72 | **否** | 需开卡（P2） |
| 14 | `dyn_var_insert` | player | 恒 false | ProcessingBase.cs:590 | 56 | **否** | 需开卡（P2） → `openmir2-script-dyn-var-host` |
| 15 | `send_update_item` | player | 空实现（no-op） | Processings/ExecutionProcessingSys.cs:4023 | 42 | **否** | 需开卡（P2） |
| 16 | `set_use_item_desc` | player | 空实现（no-op） | Processings/ExecutionProcessingSys.cs:4017 | 42 | **否** | 需开卡（P2） |
| 17 | `set_use_item_dura_max` | player | 空实现（no-op） | Processings/ExecutionProcessingSys.cs:3944 | 42 | **否** | 需开卡（P2） |
| 18 | `use_item_desc` | player | 恒 0 | Processings/ExecutionProcessingSys.cs:4017 | 42 | **否** | 需开卡（P2） |
| 19 | `use_item_dura` | player | 恒 0 | Processings/ExecutionProcessingSys.cs:3944 | 42 | **否** | 需开卡（P2） |
| 20 | `set_credit_point` | player | 空实现（no-op） | Processings/ExecutionProcessingSys.cs:2772 | 40 | **否** | 需开卡（P2） |
| 21 | `job` | player | 恒 0 | Processings/ConditionProcessingSys.cs:4069 | 26 | **否** | 需开卡（P2） |
| 22 | `map_random_move` | player | 空实现（no-op） | Processings/ExecutionProcessingSys.cs:2172 | 23 | **否** | 需开卡（P2） |
| 23 | `send_ref_msg` | player | 空实现（no-op） | Processings/ExecutionProcessingSys.cs:2171 | 23 | **否** | 需开卡（P2） |
| 24 | `send_def_message` | player | 空实现（no-op） | Processings/ExecutionProcessingSys.cs:2244 | 22 | **否** | 需开卡（P2） |
| 25 | `gender` | player | 恒「男」 | Processings/ConditionProcessingSys.cs:4137 | 14 | **否** | 需开卡（P2） |
| 26 | `is_player_online` | player | 恒 false | Processings/ConditionProcessingSys.cs:4562 | 14 | **否** | 需开卡（P2） |
| 27 | `set_kick_flags` | player | 空实现（no-op） | Processings/ExecutionProcessingSys.cs:5267 | 14 | **否** | 需开卡（P2） |
| 28 | `set_val_query` | player | 空实现（no-op） | Processings/ExecutionProcessingSys.cs:2280 | 13 | **否** | 需开卡（P2） |
| 29 | `abil_hp` | player | 恒 0 | Processings/ExecutionProcessingSys.cs:5198 | 10 | **否** | 需开卡（P2） |
| 30 | `abil_mp` | player | 恒 0 | Processings/ExecutionProcessingSys.cs:5238 | 10 | **否** | 需开卡（P2） |
| 31 | `clear_delay_goto` | player | 空实现（no-op） | Processings/ExecutionProcessingSys.cs:2127 | 10 | **否** | 需开卡（P2） |
| 32 | `clear_map_monsters` | player | 空实现（no-op） | Processings/ExecutionProcessingSys.cs:2983 | 10 | **否** | 需开卡（P2） |
| 33 | `dear_name` | player | 恒空串 | Processings/ConditionProcessingSys.cs:3158 | 10 | **否** | 需开卡（P2） |
| 34 | `map_human_count` | player | 恒 0 | Processings/ConditionProcessingSys.cs:938 | 10 | **否** | 需开卡（P2） |
| 35 | `regen_monster` | player | 恒 false | Processings/ExecutionProcessingSys.cs:5483 | 10 | **否** | 需开卡（P2） |
| 36 | `set_abil_hp` | player | 空实现（no-op） | Processings/ExecutionProcessingSys.cs:5198 | 10 | **否** | 需开卡（P2） |
| 37 | `set_abil_mp` | player | 空实现（no-op） | Processings/ExecutionProcessingSys.cs:5238 | 10 | **否** | 需开卡（P2） |
| 38 | `today_string` | player | 恒空串 | ScriptEngine.cs:382 | 10 | **否** | 需开卡（P2） |
| 39 | `wabil_hp` | player | 恒 0 | Processings/ExecutionProcessingSys.cs:5201 | 10 | **否** | 需开卡（P2） |
| 40 | `wabil_max_hp` | player | 恒 0 | Processings/ExecutionProcessingSys.cs:5212 | 10 | **否** | 需开卡（P2） |
| 41 | `wabil_max_mp` | player | 恒 0 | Processings/ExecutionProcessingSys.cs:5252 | 10 | **否** | 需开卡（P2） |
| 42 | `wabil_mp` | player | 恒 0 | Processings/ExecutionProcessingSys.cs:5241 | 10 | **否** | 需开卡（P2） |
| 43 | `bonus_point` | player | 恒 0 | Processings/ConditionProcessingSys.cs:1830 | 8 | **否** | 需开卡（P3） |
| 44 | `bonus_total_point` | player | 恒 0 | Processings/ConditionProcessingSys.cs:1829 | 8 | **否** | 需开卡（P3） |
| 45 | `inc_marry_count` | player | 空实现（no-op） | Processings/ConditionProcessingSys.cs:3181 | 8 | **否** | 需开卡（P3） |
| 46 | `inc_pose_marry_count` | player | 空实现（no-op） | Processings/ExecutionProcessingSys.cs:3642 | 8 | **否** | 需开卡（P3） |
| 47 | `is_guild_master` | player | 恒 false | ProcessingBase.cs:274 | 8 | **否** | 需开卡（P3） |
| 48 | `is_start_marry` | player | 恒 false | Processings/ExecutionProcessingSys.cs:3254 | 8 | **否** | 需开卡（P3） |
| 49 | `is_start_un_marry` | player | 恒 false | Processings/ExecutionProcessingSys.cs:3632 | 8 | **否** | 需开卡（P3） |
| 50 | `last_hiter_name` | player | 恒 None | ProcessingBase.cs:189 | 8 | **否** | 需开卡（P3） |
| 51 | `last_hiter_race` | player | 恒 None | ProcessingBase.cs:189 | 8 | **否** | 需开卡（P3） |
| 52 | `member_level` | player | 恒 0 | Processings/ConditionProcessingSys.cs:3231 | 8 | **否** | 需开卡（P3） |
| 53 | `pose_goto_label` | player | 空实现（no-op） | Processings/ExecutionProcessingSys.cs:3239 | 8 | **否** | 需开卡（P3） |
| 54 | `pose_is_start_marry` | player | 恒 false | Processings/ExecutionProcessingSys.cs:3255 | 8 | **否** | 需开卡（P3） |
| 55 | `pose_is_start_un_marry` | player | 恒 false | Processings/ExecutionProcessingSys.cs:3633 | 8 | **否** | 需开卡（P3） |
| 56 | `pose_ref_show_name` | player | 空实现（no-op） | Processings/ExecutionProcessingSys.cs:3138 | 8 | **否** | 需开卡（P3） |
| 57 | `quest_check_item` | player | 委托其它钩子 | ScriptEngine.cs:573 | 8 | **否** | 需开卡（P3） |
| 58 | `ref_show_name` | player | 空实现（no-op） | Processings/ExecutionProcessingSys.cs:3138 | 8 | **否** | 需开卡（P3） |
| 59 | `set_bonus_point` | player | 空实现（no-op） | Processings/ExecutionProcessingSys.cs:5305 | 8 | **否** | 需开卡（P3） |
| 60 | `set_dear_name` | player | 空实现（no-op） | Processings/ConditionProcessingSys.cs:3158 | 8 | **否** | 需开卡（P3） |
| 61 | `set_is_start_marry` | player | 空实现（no-op） | Processings/ExecutionProcessingSys.cs:3254 | 8 | **否** | 需开卡（P3） |
| 62 | `set_is_start_un_marry` | player | 空实现（no-op） | Processings/ExecutionProcessingSys.cs:3632 | 8 | **否** | 需开卡（P3） |
| 63 | `set_pose_dear_name` | player | 空实现（no-op） | Processings/ExecutionProcessingSys.cs:3310 | 8 | **否** | 需开卡（P3） |
| 64 | `set_pose_is_start_marry` | player | 空实现（no-op） | Processings/ExecutionProcessingSys.cs:3255 | 8 | **否** | 需开卡（P3） |
| 65 | `set_pose_is_start_un_marry` | player | 空实现（no-op） | Processings/ExecutionProcessingSys.cs:3644 | 8 | **否** | 需开卡（P3） |
| 66 | `set_re_level` | player | 空实现（no-op） | ProcessingBase.cs:292 | 8 | **否** | 需开卡（P3） |
| 67 | `target_name` | player | 恒 None | ScriptEngine.cs:343 | 8 | **否** | 需开卡（P3） |
| 68 | `abil_exp` | player | 恒 0 | ProcessingBase.cs:372 | 7 | **否** | 需开卡（P3） |
| 69 | `set_abil_exp` | player | 空实现（no-op） | Processings/ConditionProcessingSys.cs:2303 | 7 | **否** | 需开卡（P3） |
| 70 | `set_delay_call` | player | 空实现（no-op） | Processings/ExecutionProcessingSys.cs:3757 | 5 | **否** | 需开卡（P3） |
| 71 | `set_dlg_query` | player | 空实现（no-op） | Processings/ExecutionProcessingSys.cs:2236 | 5 | **否** | 需开卡（P3） |
| 72 | `set_use_item_dura` | player | 空实现（no-op） | ProcessingBase.cs:457 | 5 | **否** | 需开卡（P3） |
| 73 | `use_item_dura_max` | player | 恒 0 | Processings/ExecutionProcessingSys.cs:3944 | 5 | **否** | 需开卡（P3） |
| 74 | `has_level_up` | player | 空实现（no-op） | Processings/ExecutionProcessingSys.cs:2903 | 4 | **否** | 需开卡（P3） |
| 75 | `set_abil_level` | player | 空实现（no-op） | ProcessingBase.cs:297 | 4 | **否** | 需开卡（P3） |
| 76 | `set_bo_send_msg_flag` | player | 空实现（no-op） | Processings/ExecutionProcessingSys.cs:2323 | 4 | **否** | 需开卡（P3） |
| 77 | `admin_mode` | player | 恒 false | Processings/ExecutionProcessingSys.cs:5580 | 3 | **否** | 需开卡（P3） |
| 78 | `ob_mode` | player | 恒 false | Processings/ExecutionProcessingSys.cs:5602 | 3 | **否** | 需开卡（P3） |
| 79 | `pk_point` | player | 恒 0 | ProcessingBase.cs:382 | 3 | **否** | 需开卡（P3） |
| 80 | `pvp_level` | player | 恒 0 | Processings/ExecutionProcessingSys.cs:2952 | 3 | **否** | 需开卡（P3） |
| 81 | `ref_name_color` | player | 空实现（no-op） | Processings/ExecutionProcessingSys.cs:2104 | 3 | **否** | 需开卡（P3） |
| 82 | `set_admin_mode` | player | 空实现（no-op） | Processings/ExecutionProcessingSys.cs:5580 | 3 | **否** | 需开卡（P3） |
| 83 | `set_mission` | player | 空实现（no-op） | Processings/ExecutionProcessingSys.cs:3425 | 3 | **否** | 需开卡（P3） |
| 84 | `set_ob_mode` | player | 空实现（no-op） | Processings/ExecutionProcessingSys.cs:5602 | 3 | **否** | 需开卡（P3） |
| 85 | `set_pk_point` | player | 空实现（no-op） | Processings/ExecutionProcessingSys.cs:2077 | 3 | **否** | 需开卡（P3） |
| 86 | `set_super_man` | player | 空实现（no-op） | Processings/ExecutionProcessingSys.cs:5591 | 3 | **否** | 需开卡（P3） |
| 87 | `super_man` | player | 恒 false | Processings/ExecutionProcessingSys.cs:5591 | 3 | **否** | 需开卡（P3） |
| 88 | `die` | player | 空实现（no-op） | Processings/ExecutionProcessingSys.cs:5281 | 2 | **否** | 需开卡（P3） |
| 89 | `feature_changed` | player | 空实现（no-op） | Processings/ExecutionProcessingSys.cs:2872 | 2 | **否** | 需开卡（P3） |
| 90 | `set_bo_change_item_name_flag` | player | 空实现（no-op） | Processings/ExecutionProcessingSys.cs:3513 | 2 | **否** | 需开卡（P3） |
| 91 | `set_hair` | player | 空实现（no-op） | Processings/ExecutionProcessingSys.cs:2871 | 2 | **否** | 需开卡（P3） |
| 92 | `set_last_hiter_npc` | player | 空实现（no-op） | Processings/ExecutionProcessingSys.cs:5284 | 2 | **否** | 需开卡（P3） |
| 93 | `set_no_item` | player | 空实现（no-op） | Processings/ExecutionProcessingSys.cs:5280 | 2 | **否** | 需开卡（P3） |
| 94 | `xy_obj_count` | player | 恒 None | Processings/ConditionProcessingSys.cs:4578 | 2 | **否** | 需开卡（P3） |
| 95 | `curr_pos` | player | 恒 (0,0) | ProcessingBase.cs:258 | 1 | **否** | 需开卡（P3） |
| 96 | `in_safe_zone` | player | 恒 false | ScriptEngine.cs:599 | 1 | **否** | 需开卡（P3） |
| 97 | `is_new_human` | player | 恒 false | Processings/ConditionProcessingSys.cs:4204 | 1 | **否** | 需开卡（P3） |
| 98 | `map_name` | player | 恒空串 | ScriptEngine.cs:663 | 1 | **否** | 需开卡（P3） |
| 99 | `pose` | player | 恒 None | Processings/ConditionProcessingSys.cs:1612 | 1 | **否** | 需开卡（P3） |
| 100 | `set_is_time_recall` | player | 空实现（no-op） | Processings/ExecutionProcessingSys.cs:2109 | 1 | **否** | 需开卡（P3） |
| 101 | `set_time_recall` | player | 空实现（no-op） | Processings/ExecutionProcessingSys.cs:2135 | 1 | **否** | 需开卡（P3） |
| 102 | `action_variable_text` | player | 空实现（no-op） | ProcessingBase.cs:1463 | 0 | **否** | 需开卡（P3） → `openmir2-script-action-var-text` |
| 103 | `is_master` | player | 恒 false | Processings/ConditionProcessingSys.cs:2985 | 0 | **否** | 需开卡（P3） |
| 104 | `magic_level` | player | 恒 None | Processings/ConditionProcessingSys.cs:3602 | 0 | **否** | 需开卡（P3） |
| 105 | `master_name` | player | 恒空串 | ProcessingBase.cs:227 | 0 | **否** | 需开卡（P3） |
| 106 | `pose_actor_id` | player | 恒 0 | src/BotSrv/Player/RobotPlayer.cs:303 | 0 | **否** | 需开卡（P3） |
| 107 | `pose_dear_name` | player | 恒空串 | Processings/ExecutionProcessingSys.cs:3310 | 0 | **否** | 需开卡（P3） |

### 2.3 安全（54）

「安全」分三类：**已覆写**（缺省实现不生效）/ **语义等价**（缺省实现即 oracle 语义）/ **既有近似**（宿主层已接但二者不是逐字等价）。每行理由见机器台账 `reason` 与 `safe_kind`。

| # | 方法 | trait | 缺省语义 | oracle 对应实现 | 可达行数 | NpcPlayer 覆写 | 结论 |
|---|---|---|---|---|---|---|---|
| 1 | `sys_msg` | player | 空实现（no-op） | Processings/ExecutionProcessingSys.cs:2657 | 989 | 是 | 安全（已覆写） |
| 2 | `chr_name` | player | 恒空串 | ProcessingBase.cs:183 | 973 | 是 | 安全（已覆写） |
| 3 | `guild_name` | player | 恒 None | Processings/ExecutionProcessingSys.cs:5925 | 815 | 是 | 安全（已覆写） |
| 4 | `path` | npc | 恒空串 | Processings/ConditionProcessingSys.cs:642 | 489 | 是 | 安全（已覆写） |
| 5 | `drop_item_on_full_bag` | player | 空实现（no-op） | ScriptEngine.cs:740 | 479 | 是 | 安全（已覆写） |
| 6 | `gold_changed` | player | 空实现（no-op） | ScriptEngine.cs:660 | 479 | 是 | 安全（已覆写） |
| 7 | `is_enough_bag` | player | 委托 bag_count() | ScriptEngine.cs:712 | 479 | **否** | 安全（语义等价） |
| 8 | `global_a_val` | player | 恒空串 | ProcessingBase.cs:868 | 450 | 是 | 安全（已覆写） |
| 9 | `globa_dy_mval` | player | 恒 0 | ProcessingBase.cs:850 | 436 | 是 | 安全（已覆写） |
| 10 | `global_val` | player | 恒 0 | ProcessingBase.cs:832 | 436 | 是 | 安全（已覆写） |
| 11 | `set_globa_dy_mval` | player | 空实现（no-op） | ProcessingBase.cs:661 | 364 | 是 | 安全（已覆写） |
| 12 | `set_global_val` | player | 空实现（no-op） | ProcessingBase.cs:646 | 364 | 是 | 安全（已覆写） |
| 13 | `set_mdy_val` | player | 空实现（no-op） | ProcessingBase.cs:651 | 364 | 是 | 安全（已覆写） |
| 14 | `set_mn_integer` | player | 空实现（no-op） | ProcessingBase.cs:666 | 364 | 是 | 安全（已覆写） |
| 15 | `set_mnmval` | player | 空实现（no-op） | ProcessingBase.cs:656 | 364 | 是 | 安全（已覆写） |
| 16 | `game_gold_changed` | player | 空实现（no-op） | Processings/ExecutionProcessingSys.cs:3091 | 357 | 是 | 安全（已覆写） |
| 17 | `game_gold` | player | 恒 0 | ProcessingBase.cs:432 | 314 | 是 | 安全（已覆写） |
| 18 | `set_global_a_val` | player | 空实现（no-op） | ProcessingBase.cs:676 | 297 | 是 | 安全（已覆写） |
| 19 | `set_ms_string` | player | 空实现（no-op） | ProcessingBase.cs:671 | 297 | 是 | 安全（已覆写） |
| 20 | `set_game_gold` | player | 空实现（no-op） | ProcessingBase.cs:432 | 285 | 是 | 安全（已覆写） |
| 21 | `server_str_val` | player | 恒空串 | ProcessingBase.cs:1099 | 161 | 是 | 安全（已覆写） |
| 22 | `server_int_val` | player | 恒 0 | ProcessingBase.cs:1173 | 147 | 是 | 安全（已覆写） |
| 23 | `game_point` | player | 恒 0 | ProcessingBase.cs:437 | 96 | 是 | 安全（已覆写） |
| 24 | `credit_point` | player | 恒 0 | ProcessingBase.cs:387 | 68 | 是 | 安全（已覆写） |
| 25 | `send_del_use_item` | player | 空实现（no-op） | src/M2Server/Player/PlayObject.Base.cs:2646 | 42 | 是 | 安全（已覆写） |
| 26 | `set_use_item_index` | player | 空实现（no-op） | ScriptEngine.cs:871 | 42 | 是 | 安全（已覆写） |
| 27 | `use_item_std_item_present` | player | 委托 use_item_index() | src/M2Server/Player/PlayObject.Base.cs:1026 | 42 | **否** | 安全（既有近似，见注） |
| 28 | `set_play_dice_label` | player | 空实现（no-op） | Processings/ExecutionProcessingSys.cs:2032 | 24 | 是 | 安全（已覆写） |
| 29 | `tick_count` | player | 恒 0 | Processings/ExecutionProcessingSys.cs:2138 | 10 | 是 | 安全（已覆写） |
| 30 | `config_game_gold_name` | player | 委托其它钩子 | ProcessingBase.cs:123 | 8 | 是 | 安全（已覆写） |
| 31 | `config_renewlevel_clear_exp` | player | 恒 false | Processings/ExecutionProcessingSys.cs:5417 | 8 | **否** | 安全（语义等价） |
| 32 | `group_member_jobs` | player | 恒空表 | Processings/ConditionProcessingSys.cs:287 | 8 | 是 | 安全（已覆写） |
| 33 | `group_owner` | player | 恒 0 | Processings/ConditionProcessingSys.cs:285 | 8 | 是 | 安全（已覆写） |
| 34 | `line_variable_text` | player | 委托其它钩子 | ScriptEngine.cs:989 | 8 | 是 | 安全（已覆写） |
| 35 | `quest_unit_open_status` | player | 恒 0 | Processings/ConditionProcessingSys.cs:204 | 8 | 是 | 安全（已覆写） |
| 36 | `quest_unit_status` | player | 恒 0 | Processings/ConditionProcessingSys.cs:4109 | 8 | 是 | 安全（已覆写） |
| 37 | `sale_deal` | player | 恒 false | Processings/ExecutionProcessingSys.cs:2334 | 8 | 是 | 安全（已覆写） |
| 38 | `set_bo_dec_game_gold` | player | 空实现（no-op） | Processings/ExecutionProcessingSys.cs:2744 | 8 | 是 | 安全（已覆写） |
| 39 | `set_bo_inc_game_gold` | player | 空实现（no-op） | Processings/ExecutionProcessingSys.cs:2678 | 8 | 是 | 安全（已覆写） |
| 40 | `set_kill_mon_exp_rate` | player | 空实现（no-op） | Processings/ExecutionProcessingSys.cs:5458 | 8 | 是 | 安全（已覆写） |
| 41 | `set_map_mode` | player | 空实现（no-op） | Processings/ConditionProcessingSys.cs:927 | 8 | 是 | 安全（已覆写） |
| 42 | `set_power_rate` | player | 空实现（no-op） | Processings/ExecutionProcessingSys.cs:5548 | 8 | 是 | 安全（已覆写） |
| 43 | `set_sale_deal` | player | 空实现（no-op） | Processings/ExecutionProcessingSys.cs:2334 | 8 | 是 | 安全（已覆写） |
| 44 | `use_item_index` | player | 恒 0 | ScriptEngine.cs:871 | 5 | 是 | 安全（已覆写） |
| 45 | `use_item_std_name` | player | 恒空串 | src/M2Server/Player/PlayObject.cs:5342 | 5 | 是 | 安全（已覆写） |
| 46 | `use_items_len` | player | 恒 0 | ScriptEngine.cs:865 | 5 | 是 | 安全（已覆写） |
| 47 | `envir_map_name` | player | 恒 None | ProcessingBase.cs:268 | 4 | 是 | 安全（已覆写） |
| 48 | `set_offline` | player | 空实现（no-op） | Processings/ExecutionProcessingSys.cs:2727 | 4 | 是 | 安全（已覆写） |
| 49 | `re_level` | player | 恒 0 | ProcessingBase.cs:292 | 2 | 是 | 安全（已覆写） |
| 50 | `map_exists` | player | 恒 false | Processings/ConditionProcessingSys.cs:927 | 1 | 是 | 安全（已覆写） |
| 51 | `set_auto_get_exp` | player | 空实现（no-op） | Processings/ExecutionProcessingSys.cs:2707 | 1 | 是 | 安全（已覆写） |
| 52 | `play_dice_label` | player | 恒 None | Processings/ExecutionProcessingSys.cs:2032 | 0 | 是 | 安全（已覆写） |
| 53 | `set_server_int_val` | player | 空实现（no-op） | ProcessingBase.cs:1173 | 0 | 是 | 安全（已覆写） |
| 54 | `set_server_str_val` | player | 空实现（no-op） | ProcessingBase.cs:1099 | 0 | 是 | 安全（已覆写） |


> **既有近似**只有 1 条：`use_item_std_item_present` 缺省 `use_item_index(i) > 0`，oracle 是 `GetStdItem(UseItems[i].Index) != null`（`PlayObject.Base.cs:1026`）；Index 非零但物品表查不到时两者不同。这不是本卡的缺口（宿主层不需要新存储），残余归 StdItem/物品表卡。

---

## 3. `#IF` 恒值 getter 专章（80 个 const 缺省实现逐条）

要求：**不许把「getter 恒 0」一律判安全**。下表对每个恒值 getter 给出「oracle 什么情况下会返回非缺省值」+「当前 Rust 是否可能在该情况下被调用」。

| 方法 | 缺省值 | oracle 何时非缺省 + 当前 Rust 调用情形（一句话） | 结论 |
|---|---|---|---|
| `chr_name` | 恒空串 | oracle 侧同名字段的读写写法见 `sValue = playerActor.ChrName;`；只要真实玩家/世界状态非缺省即为非零/非空。Rust 当前恒空串，部署可达 973 行。 | 安全（已覆写） |
| `guild_name` | 恒 None | oracle 侧同名字段的读写写法见 `playerActor.MyGuild.GuildName`；只要真实玩家/世界状态非缺省即为非零/非空。Rust 当前恒 None，部署可达 815 行。 | 安全（已覆写） |
| `envir_root` | 恒 None | oracle 侧根目录由宿主注入（`GetEnvirFilePath`），恒非 None。Rust 恒 None（`HostNpc` 未覆写）⇒ 文件列表类条件/动作全失效。 | 需开卡 P1 → `openmir2-script-envir-root-host` |
| `path` | 恒空串 | oracle 侧同名字段的读写写法见 `string sListFileName = SystemShare.GetEnvirFilePath(normNpc.Path, questConditionInfo.sParam1);`；只要真实玩家/世界状态非缺省即为非零/非空。Rust 当前恒空串，部署可达 489 行。 | 安全（已覆写） |
| `dyn_var_get` | 恒 None | oracle 取 `DynamicVarMap`（`$HUMAN/$GLOBAL/$ACCOUNT`）：变量声明过则非 None。Rust 恒 None ⇒ 动态变量永读不到（`CALCVAR` 68 / `VAR` 24 / `LOADVAR` 24 等）。 | 需开卡 P1 → `openmir2-script-dyn-var-host` |
| `user_account` | 恒空串 | oracle 取 `playerActor.UserAccount`，任何真实玩家都非空。Rust 恒空 ⇒ `CHECKACCOUNTLIST`(238) 恒 false、`DELACCOUNTLIST`(238) 删不到行；**真实玩家确实会走到**。 | 需开卡 P1 → `openmir2-script-user-account-host` |
| `global_a_val` | 恒空串 | oracle 侧同名字段的读写写法见 `sValue = SystemShare.Config.GlobalAVal[n100 - 700];`；只要真实玩家/世界状态非缺省即为非零/非空。Rust 当前恒空串，部署可达 450 行。 | 安全（已覆写） |
| `globa_dy_mval` | 恒 0 | oracle 侧同名字段的读写写法见 `nValue = SystemShare.Config.GlobaDyMval[n100 - 400];`；只要真实玩家/世界状态非缺省即为非零/非空。Rust 当前恒 0，部署可达 436 行。 | 安全（已覆写） |
| `global_val` | 恒 0 | oracle 侧同名字段的读写写法见 `nValue = SystemShare.Config.GlobalVal[n100 - 100];`；只要真实玩家/世界状态非缺省即为非零/非空。Rust 当前恒 0，部署可达 436 行。 | 安全（已覆写） |
| `dyn_var_set` | 恒 false | oracle 写回已存在且类型匹配的变量并返回 true。Rust 恒 false ⇒ 写点静默失败。 | 需开卡 P1 → `openmir2-script-dyn-var-host` |
| `game_gold` | 恒 0 | oracle 侧同名字段的读写写法见 `nValue = playerActor.GameGold;`；只要真实玩家/世界状态非缺省即为非零/非空。Rust 当前恒 0，部署可达 314 行。 | 安全（已覆写） |
| `config_show_script_action_msg` | 恒 false | 不是玩家值，是配置读：oracle 取 `SystemShare.Config.ShowScriptActionMsg`，**部署 `Server.conf:753` 为 1（true）**。Rust 恒 false ⇒ `ADDSKILL`(165) 等分支里 oracle 会发的提示帧当前全不发；部署路径**确实**会走到。 | 需开卡 P1 |
| `find_magic` | 恒 None | oracle 查 `WorldEngine.FindMagic(name)`：名字命中魔法表时返回 Some。Rust 恒 None ⇒ `ADDSKILL`(165) 一律走「找不到技能」错误支；**确实**会走到。 | 需开卡 P1 |
| `is_training_skill` | 恒 false | oracle 查 `playerActor.IsTrainingSkill(id)`：已学该技能 true。Rust 恒 false ⇒ 已学技能会被重复学习/或走错支。 | 需开卡 P1 |
| `server_str_val` | 恒空串 | oracle 侧同名字段的读写写法见 `sValue = playerActor.ServerStrVal[n01 - 1600];`；只要真实玩家/世界状态非缺省即为非零/非空。Rust 当前恒空串，部署可达 161 行。 | 安全（已覆写） |
| `permission` | 恒 0 | oracle 取 `playerActor.Permission`；GM/管理员非 0。Rust 恒 0 ⇒ `GMEXECUTE`/`ISADMIN`/`ISSYSOP` 分支永不成立；**真实 GM 账号确实会走到**（139/8 行，已有卡 `openmir2-gm-script-host-seam` 承接）。 | 已有卡 `openmir2-gm-script-host-seam` |
| `server_int_val` | 恒 0 | oracle 侧同名字段的读写写法见 `nValue = playerActor.ServerIntVal[n01 - 1700];`；只要真实玩家/世界状态非缺省即为非零/非空。Rust 当前恒 0，部署可达 147 行。 | 安全（已覆写） |
| `use_items_upper_bound` | 恒 -1 | oracle 取 `UseItems.GetUpperBound(0)`（13 槽 ⇒ 12，恒非 -1）。Rust 恒 -1 ⇒ `CHECKUSEITEM`/`CHECKITEMADDVALUE` 的越界判定恒成立、直接报错返回；**确实**会走到。 | 需开卡 P1 |
| `worn_item_add_value` | 恒 None | oracle 取 `UseItems[nWhere].Desc[...]` 之和：装备有附加属性时非 None。Rust 恒 None ⇒ `CHECKITEMADDVALUE` 判假。 | 需开卡 P1 |
| `game_point` | 恒 0 | oracle 侧同名字段的读写写法见 `nValue = playerActor.GamePoint;`；只要真实玩家/世界状态非缺省即为非零/非空。Rust 当前恒 0，部署可达 96 行。 | 安全（已覆写） |
| `credit_point` | 恒 0 | oracle 侧同名字段的读写写法见 `nValue = playerActor.CreditPoint;`；只要真实玩家/世界状态非缺省即为非零/非空。Rust 当前恒 0，部署可达 68 行。 | 安全（已覆写） |
| `dyn_var_insert` | 恒 false | oracle 在变量不存在时新增并返回 true。Rust 恒 false ⇒ `VAR` 声明恒失败。 | 需开卡 P2 → `openmir2-script-dyn-var-host` |
| `use_item_desc` | 恒 0 | oracle 取 `UseItems[i].Desc[attr]`：装备有该属性时非 0。Rust 恒 0 ⇒ `UPGRADEITEMEX`/`CHECKITEMADDVALUE` 读到的属性恒 0。 | 需开卡 P2 |
| `use_item_dura` | 恒 0 | oracle 取 `UseItems[i].Dura`：装备有持久时非 0。Rust 恒 0 ⇒ `REPAIRALL`/`CHECKDURA`(0 行) 读到 0。 | 需开卡 P2 |
| `job` | 恒 0 | oracle 取 `playerActor.Job`：法师=1/道士=2。Rust 恒「战士」⇒ `CHECKJOB`(18)/`MARRY`/`UNMARRY` 对法师/道士走错分支；**确实**会走到。 | 需开卡 P2 |
| `gender` | 恒「男」 | oracle 取 `playerActor.Gender`：女玩家为 1。Rust 恒「男」⇒ `GENDER`(5)/`CHECKPOSEDIR`/`MARRY`/`UNMARRY` 对女玩家走错分支；女玩家**确实**会走到。 | 需开卡 P2 |
| `is_player_online` | 恒 false | oracle `GetPlayObject(name) != null`：玩家在线即 true。Rust 恒 false ⇒ `CHECKONLINE`(14) 把在线玩家判成离线；真实在线玩家**确实**会走到。 | 需开卡 P2 |
| `abil_hp` | 恒 0 | oracle 取 `Abil.HP`：受伤后非满。Rust 恒 0 ⇒ `HUMANHP`/`HUMANMP`(各 5) 的 `-` 分支恒判下溢并写 0（写回本身也是空实现）。 | 需开卡 P2 |
| `abil_mp` | 恒 0 | oracle 取 `Abil.MP`。Rust 恒 0 ⇒ 与 `abil_hp` 同因，`HUMANMP` 减蓝恒归零。 | 需开卡 P2 |
| `dear_name` | 恒空串 | oracle 取 `playerActor.DearName`：已婚玩家非空。Rust 恒空 ⇒ `CHECKMARRY`(10) 对已婚玩家判假；已婚玩家**确实**会走到。 | 需开卡 P2 |
| `map_human_count` | 恒 0 | oracle `GetMapHuman(map)`：地图有人时 >0。Rust 恒 0 ⇒ `CHECKHUM` 类条件判假；部署 `CHECKHUM` 0 行。 | 需开卡 P2 |
| `regen_monster` | 恒 false | oracle 调 `WorldEngine.RegenMonsterByName` 并返回是否成功（成功后 true）。Rust 恒 false ⇒ `MONGENEX` 不生成怪。 | 需开卡 P2 |
| `tick_count` | 恒 0 | oracle 侧同名字段的读写写法见 `playerActor.TimeRecallTick = HUtil32.GetTickCount() + (questActionInfo.nParam1 * 60 * 1000);`；只要真实玩家/世界状态非缺省即为非零/非空。Rust 当前恒 0，部署可达 10 行。 | 安全（已覆写） |
| `today_string` | 恒空串 | oracle `DateTime.Today.ToString()` 恒非空。Rust 恒空 ⇒ `ADDUSERDATE`(2)/`DELUSERDATE`(2) 写入的日期列为空串；**确实**会走到。 | 需开卡 P2 |
| `wabil_hp` | 恒 0 | oracle 取 `WAbil.HP`（当前上限应用后的现值）。Rust 恒 0 ⇒ `HUMANHP`(5) 的 `-` 下溢判定恒走「归零」支。 | 需开卡 P2 |
| `wabil_max_hp` | 恒 0 | oracle 取 `WAbil.MaxHP`（角色属性，恒 >0）。Rust 恒 0 ⇒ `HUMANHP` 的上限钳制失效。 | 需开卡 P2 |
| `wabil_max_mp` | 恒 0 | oracle 取 `WAbil.MaxMP`。Rust 恒 0 ⇒ `HUMANMP` 的上限钳制失效。 | 需开卡 P2 |
| `wabil_mp` | 恒 0 | oracle 取 `WAbil.MP`。Rust 恒 0 ⇒ `HUMANMP`(5) 同上。 | 需开卡 P2 |
| `bonus_point` | 恒 0 | oracle 取 `playerActor.BonusPoint`：洗点后 >0。Rust 恒 0 ⇒ `RENEWLEVEL` 的加点累加从 0 起（且写回也是空实现）。语料 `RENEWLEVEL` 0 行。 | 需开卡 P3 |
| `bonus_total_point` | 恒 0 | oracle 取 `BonusAbil` 九项和；洗点后 >0。Rust 恒 0 ⇒ `CHECKBONUSPOINT` 类条件判假。 | 需开卡 P3 |
| `config_renewlevel_clear_exp` | 恒 false | oracle 侧同名字段的读写写法见 `if (SystemShare.Config.ReNewLevelClearExp)`；只要真实玩家/世界状态非缺省即为非零/非空。Rust 当前恒 false，部署可达 8 行。 | 安全（语义等价） |
| `group_member_jobs` | 恒空表 | oracle 侧同名字段的读写写法见 `for (int i = 0; i < playerActor.GroupMembers.Count; i++)`；只要真实玩家/世界状态非缺省即为非零/非空。Rust 当前恒空表，部署可达 8 行。 | 安全（已覆写） |
| `group_owner` | 恒 0 | oracle 侧同名字段的读写写法见 `if (playerActor.GroupOwner != 0)`；只要真实玩家/世界状态非缺省即为非零/非空。Rust 当前恒 0，部署可达 8 行。 | 安全（已覆写） |
| `is_guild_master` | 恒 false | oracle `MyGuild != null && IsGuildMaster()`：会长 true。语料 `ISGUILDMASTER` 0 行 ⇒ 当前不会被调用。 | 需开卡 P3 |
| `is_start_marry` | 恒 false | oracle 读会话「求婚进行中」标志：求婚期间 true。语料 0 行 ⇒ 当前不会被调用。 | 需开卡 P3 |
| `is_start_un_marry` | 恒 false | oracle 读会话「离婚进行中」标志。语料 0 行。 | 需开卡 P3 |
| `last_hiter_name` | 恒 None | oracle 取最后攻击者名（被攻击过则非 None）。语料 0 行。 | 需开卡 P3 |
| `last_hiter_race` | 恒 None | oracle 取最后攻击者 race（被攻击过则非 None）。语料 0 行。 | 需开卡 P3 |
| `member_level` | 恒 0 | oracle 取 `playerActor.MemberLevel`：会员 >0、非会员 0。部署语料 `CHECKMEMBERLEVEL` 0 行 ⇒ 当前不会被调用。 | 需开卡 P3 |
| `pose_is_start_marry` | 恒 false | oracle 读**面向对象**的求婚标志：对方求婚中 true。语料 0 行。 | 需开卡 P3 |
| `pose_is_start_un_marry` | 恒 false | oracle 读面向对象的离婚标志。语料 0 行。 | 需开卡 P3 |
| `quest_unit_open_status` | 恒 0 | oracle 侧同名字段的读写写法见 `int n10 = playerActor.GetQuestUnitOpenStatus(n14);`；只要真实玩家/世界状态非缺省即为非零/非空。Rust 当前恒 0，部署可达 8 行。 | 安全（已覆写） |
| `quest_unit_status` | 恒 0 | oracle 侧同名字段的读写写法见 `int n10 = playerActor.GetQuestUnitStatus(n14);`；只要真实玩家/世界状态非缺省即为非零/非空。Rust 当前恒 0，部署可达 8 行。 | 安全（已覆写） |
| `sale_deal` | 恒 false | oracle 侧同名字段的读写写法见 `if (playerActor.SaleDeal)`；只要真实玩家/世界状态非缺省即为非零/非空。Rust 当前恒 false，部署可达 8 行。 | 安全（已覆写） |
| `target_name` | 恒 None | oracle 取目标角色名（有目标时非 None）。语料 0 行 ⇒ 当前不会被调用。 | 需开卡 P3 |
| `abil_exp` | 恒 0 | oracle 取 `Abil.Exp`：有经验玩家非 0。Rust 恒 0 ⇒ `CHANGEEXP`(7) 的按比例分支（`$EXP`/百分比）算错；**确实**会走到。 | 需开卡 P3 |
| `use_item_dura_max` | 恒 0 | oracle 取 `UseItems[i].DuraMax`：装备有最大持久时非 0。Rust 恒 0 ⇒ `REPAIRALL`(5) 写回的持久恒 0。 | 需开卡 P3 |
| `use_item_index` | 恒 0 | oracle 侧同名字段的读写写法见 `if (playerActor.UseItems[i].Index > 0)`；只要真实玩家/世界状态非缺省即为非零/非空。Rust 当前恒 0，部署可达 5 行。 | 安全（已覆写） |
| `use_item_std_name` | 恒空串 | oracle 侧同名字段的读写写法见 `string sName = SystemShare.ItemSystem.GetStdItemName(UseItems[i].Index);`；只要真实玩家/世界状态非缺省即为非零/非空。Rust 当前恒空串，部署可达 5 行。 | 安全（已覆写） |
| `use_items_len` | 恒 0 | oracle 侧同名字段的读写写法见 `for (int i = 0; i < playerActor.UseItems.Length; i++)`；只要真实玩家/世界状态非缺省即为非零/非空。Rust 当前恒 0，部署可达 5 行。 | 安全（已覆写） |
| `envir_map_name` | 恒 None | oracle 侧同名字段的读写写法见 `sValue = playerActor.Envir.MapName;`；只要真实玩家/世界状态非缺省即为非零/非空。Rust 当前恒 None，部署可达 4 行。 | 安全（已覆写） |
| `admin_mode` | 恒 false | oracle 侧同名字段的读写写法见 `playerActor.AdminMode = boOpen;`；只要真实玩家/世界状态非缺省即为非零/非空。Rust 当前恒 false，部署可达 3 行。 | 需开卡 P3 |
| `ob_mode` | 恒 false | oracle 侧同名字段的读写写法见 `playerActor.ObMode = boOpen;`；只要真实玩家/世界状态非缺省即为非零/非空。Rust 当前恒 false，部署可达 3 行。 | 需开卡 P3 |
| `pk_point` | 恒 0 | oracle 侧同名字段的读写写法见 `nValue = playerActor.PkPoint;`；只要真实玩家/世界状态非缺省即为非零/非空。Rust 当前恒 0，部署可达 3 行。 | 需开卡 P3 |
| `pvp_level` | 恒 0 | oracle 侧同名字段的读写写法见 `byte nOldPkLevel = playerActor.PvpLevel();`；只要真实玩家/世界状态非缺省即为非零/非空。Rust 当前恒 0，部署可达 3 行。 | 需开卡 P3 |
| `super_man` | 恒 false | oracle 侧同名字段的读写写法见 `playerActor.SuperMan = boOpen;`；只要真实玩家/世界状态非缺省即为非零/非空。Rust 当前恒 false，部署可达 3 行。 | 需开卡 P3 |
| `re_level` | 恒 0 | oracle 侧同名字段的读写写法见 `nValue = playerActor.ReLevel;`；只要真实玩家/世界状态非缺省即为非零/非空。Rust 当前恒 0，部署可达 2 行。 | 安全（已覆写） |
| `xy_obj_count` | 恒 None | oracle `Envir.GetXYObjCount(x,y)`：同格对象数，>1 时 `ISDUPMODE`(2) 成立。Rust 恒 None ⇒ 永不成立；站在多对象格的玩家会走到。 | 需开卡 P3 |
| `curr_pos` | 恒 (0,0) | oracle 取 `playerActor.CurrX/CurrY`。Rust 恒 (0,0) ⇒ `TIMERECALL`(1) 记录的回城点错到 (0,0)；被召回玩家**确实**会走到。 | 需开卡 P3 |
| `in_safe_zone` | 恒 false | oracle `InSafeZone()`：安全区 true。`INSAFEZONE` 1 行；落点在安全区的玩家会被判假。 | 需开卡 P3 |
| `is_new_human` | 恒 false | oracle 取 `playerActor.IsNewHuman`：新建角色首次登录为 true。Rust 恒 false ⇒ `#IF ISNEWHUMAN` 分支永不成立；新号**确实**会走到（1 行）。 | 需开卡 P3 |
| `map_exists` | 恒 false | oracle 侧同名字段的读写写法见 `IEnvirnoment envir = SystemShare.MapMgr.FindMap(sMapName);`；只要真实玩家/世界状态非缺省即为非零/非空。Rust 当前恒 false，部署可达 1 行。 | 安全（已覆写） |
| `map_name` | 恒空串 | oracle 取 `playerActor.MapName`。Rust 恒空 ⇒ `TIMERECALL`(1) 记录的回城地图为空；被召回玩家**确实**会走到。 | 需开卡 P3 |
| `pose` | 恒 None | oracle `GetPoseCreate()`：面向前方有对象时非 None。Rust 恒 None ⇒ `CHECKPOSEMARRY`(1) 对面向配偶的玩家判假。 | 需开卡 P3 |
| `is_master` | 恒 false | oracle 取 `IsMaster`：收徒过的玩家 true。语料 0 行。 | 需开卡 P3 |
| `magic_level` | 恒 None | oracle `GetMagicInfo(name)?.Level`：已学技能非 None。语料 `MAGICLEVEL` 0 行 ⇒ 当前不会被调用。 | 需开卡 P3 |
| `master_name` | 恒空串 | oracle 取 `MasterName`：有师傅的玩家非空。语料 0 行。 | 需开卡 P3 |
| `play_dice_label` | 恒 None | oracle 侧同名字段的读写写法见 `playerActor.PlayDiceLabel = questActionInfo.sParam2;`；只要真实玩家/世界状态非缺省即为非零/非空。Rust 当前恒 None，部署语料 0 行 ⇒ 当前不会被调用。 | 安全（已覆写） |
| `pose_actor_id` | 恒 0 | oracle 取面向对象 ActorId：有面向对象时非 0。语料 0 行。 | 需开卡 P3 |
| `pose_dear_name` | 恒空串 | oracle 取 `poseHuman.DearName`：面向对象已婚则非空。语料 0 行。 | 需开卡 P3 |

---

## 4. 牙（≥2）与自检

自检脚本：`python -I mir2-rs/tests/parity/coverage/script_host_noop_audit.py check`（rc=0 绿）；`selftest` 跑两颗牙。

### 牙①（证据必须可核：不存在的 file:line 必须红）

台账里「安全」结论的证据若换成**不存在/不合法**的形式（不存在的 oracle 文件、越界行号、或行上不含引用文本），`check` 必须红。`selftest` 实测：把 `sys_msg` 的 oracle 改成 `src/Modules/ScriptEngine/__no_such_file__.cs:999999` ⇒

```text
RED（期望）: 牙①：sys_msg 的 oracle 改成 src/.../__no_such_file__.cs:999999 ⇒ 1 处不一致
  首条: `sys_msg` oracle 文件不存在: src/Modules/ScriptEngine/__no_such_file__.cs
```

### 牙②（防把已实现的方法列成缺口：谎报「已覆写」必须红）

`check` 会现场解析 `bins/game-svr/src/npc.rs` 的 `impl ScriptPlayer for NpcPlayer` / `impl ScriptNpc for HostNpc` 块：`needs_card` 行的方法名**不得**在块内；`safe` 里 `host_overridden=true` 的行**必须**在块内且 evidence 行含 `fn <name>`。反向牙把 `space_move`（实测 `NpcPlayer` 确无覆写）谎报成 `safe/已覆写` ⇒

```text
RED（期望）: 牙②：把 space_move 改判「safe：已覆写」（npc.rs:1072） ⇒ 2 处不一致
  首条: `space_move` 标「已覆写」但 ScriptPlayer for NpcPlayer impl 块里没有该方法
```

### 牙③（行号仍要落在 impl 块内：防「换个乱行号糊弄」）

符号核不是「完全不看行号」：`evidence.line` 必须落在**同一 impl 块**的范围内。把任一条「已覆写」行的 evidence.line 改成 1（块外）⇒

```text
RED（期望）: 牙③：sys_msg 的 evidence.line 改成 1（impl 块外） ⇒ 1 处不一致
  首条: `sys_msg` evidence.line 1 不在 impl 块 [1072,1605] 内
```

> 三颗牙都在 `selftest` 内用**内存副本**跑，跑完校验真实台账 sha256 与运行前一致：
> `a8d95cc786d68bd8b8a869ea67858ed2f4c1f1391fd932275abaae827594805d`（前后同值 = 还原证明；`selftest` 自身 rc=0）。

**行号纪律**：本版 `npc.rs` / `mod.rs` 行号钉在合并树 `7b0e5955`；门按**符号**核（impl 块成员 + mod.rs 内 `fn` 声明），所以别的分支改这两个热文件后**不会误红**，只会在 `check` 里打 `WARN: 行号已漂移`。需要逐行严格时跑 `check --strict-lines`。oracle（`src/**`，冻结面）仍逐行精确核。

---

## 5. 复现与门禁

| 项 | 命令 | 结果（本卡实跑） |
|---|---|---|
| fmt | `cargo fmt --all -- --check` | **rc=0** |
| clippy | `cargo clippy --workspace --all-targets -- -D warnings` | **rc=0**（合并树 32s，Finished） |
| test | `cargo test --workspace` | **rc=0**（141 suites / 3030 passed / 0 failed / 62 ignored） |
| msgcodegen | `cargo run -p msgcodegen -- --check` | **rc=0**（604 条常量与 C# 源一致） |
| anchor | `python -I -B mir2-rs/tests/parity/m3/whitelist_anchor_gate.py check` | **rc=0**（498 条锚 ALL PASS） |
| registry | `python -I mir2-rs/tests/parity/m3/m3_whitelist_registry_audit.py audit` | **rc=0**（WHITELIST_REGISTRY GREEN，无重复编号） |
| T 预约 | `python -I tools/whitelist/t_reservation_gate.py check` | **rc=0**（198 行 / 198 号，GREEN） |
| 台账自检 | `python -I mir2-rs/tests/parity/coverage/script_host_noop_audit.py check` | **rc=0**（168 行全部可核；`--strict-lines` 亦 rc=0） |
| 台账牙 | `python -I mir2-rs/tests/parity/coverage/script_host_noop_audit.py selftest` | **rc=0**（check 绿 + 牙①②③各红 + sha 还原） |

本卡**零 `.rs` 改动**（只写 `docs/` + `mir2-rs/tests/parity/coverage/`），clippy/test 各跑一次留证据；未起栈、未占端口、未连库。

---

## 6. 残留物清单

- 临时分析脚本/中间产物全部在 `E:/tmp/wt-worker-63-audit/`（含 `traits.json` / `funcs.json` / `arms.json` / `envir_counts.json` / `ledger.json`），**不入库**；可随时删除。
- 仓库内仅新增 3 个文件：本报告、`mir2-rs/tests/parity/coverage/script_host_noop_audit.json`、`.../script_host_noop_audit.py`。
- 未触碰 `whitelist.md` / `T-RESERVATIONS.md`（零 `.rs` 改动，无需 T 号）。

---

## 7. 复审返工记录（review `94718b41`，reviewer-3）

| finding | 级别 | 处理 |
|---|---|---|
| F-1 自检门在合并树上 RED 51 处（全是 evidence 行号，npc.rs 因 frame-drain +23 行漂移） | important | **同时做了 (a)+(b)**：① 把台账与文档行号**重钉到合并树 `7b0e5955`**（`check --strict-lines` rc=0）；② 把门改成**按符号核**——`evidence` 记录 `{file, impl, method, line}`，只要求「方法在该 impl 块内 + 行号落在块范围内」，`mod.rs` 同理只要求存在 `fn <name>`。别的分支再改热文件只会打 `WARN: 行号已漂移`，不再误红。行号基线写进台账 `line_baseline` / `line_policy`。 |
| F-2 「534 行影响面」不可复算且混入不走本路径的命令 | minor | 撤换为复审口径：本方法 direct 集合内「命令行且含 `<$`」**396**（SENDMSG 378 + CHECKONLINE 14 + MESSAGEBOX 4）；全树「命令行且含 `<$`」**538**；全树含 `<$` 的所有行 **694**。并注明 `GMEXECUTE` 不调 `GetLineVariableText`。子卡上的旧数已用签名 comment 订正（`fe909fbf`）。 |
| F-3 `user_account`(P1/476) 无子卡，envir-root 卡引用的「另一子卡」不存在 | minor | 新开 `openmir2-script-user-account-host`（issue oid `9bdc52f0c3009401ecdc6b890690868636c7826f`），并在 envir-root 卡签名 comment 里交叉链接（`4dc44bea`）。台账 / 本文子卡表同步为 **4 张**。 |
| F-4 `use_item_std_item_present` 标「语义等价」易被读成等价 | minor | 新增 `safe_kind` 三分类（overridden / equivalent / approximation）；该行改标 **既有近似**，理由写明 `Index>0` 与 `GetStdItem(...)!=null` 的差异与残余归属。§2.3 增加专门注记。 |
| F-5（info）全量性/计数/覆写一致性复核通过 | info | 无动作。 |
