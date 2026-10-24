# PlayDice / GAMEPOINT 残余收口（T-142 ⑨⑩⑪，tier R）

- 卡：`openmir2-script-playdice-openybdeal`（既有 R 卡残余，owner `openmir2-dev-2`，branch `card/playdice-gamapoint-residual`）。
- 基线：本检出 `master` `a6fc02d0a`。
- 写集：`mir2-rs/bins/game-svr/src/{npc.rs,lib.rs,shop.rs}`、`mir2-rs/crates/script/src/engine/mod.rs`、`mir2-rs/crates/script/tests/engine_tests.rs`、`mir2-rs/tests/parity/whitelist.md`（T-142 末段）、本文档。
- **未改**：C# `src/**`、场景 JSON、协议号表。
- tier **R**（脚本语义 + 可观测帧 + 角色属性 `GamePoint`）：合并后保持 in-progress，窗口复拍后 closed。

## 1. ⑨ GAMEPOINT 半边（已收口）

### 1.1 oracle 证据

- `ActionOfGamePoint`（`src/Modules/ScriptEngine/Processings/ExecutionProcessingSys.cs:3095-3122`）：
  `nOldGamePoint = playerActor.GamePoint`；`StrToInt(sParam2,-1)` <0 ⇒ `ScriptActionError`；
  `'='` 直接赋值、`'-'`/`'+'` 用 `HUtil32._MAX(0, …)` 钳 0；`old != new` ⇒ `playerActor.GameGoldChanged()`。
- `GameGoldChanged()`（`src/M2Server/Actor/BaseObject.cs:593-599`）：`SendUpdateMsg(RM_GAMEGOLDCHANGED, …)`。
- `SendUpdateMsg`（`BaseObject.Message.cs:283-303`）：**队首-only** 去重（见 §2.2）。
- 帧泵（`PlayObject.Message.cs:1836-1837`）：`RM_GAMEGOLDCHANGED` ⇒ `SendGoldInfo(false)` ⇒
  `SM_GAMEGOLDNAME(5008)`，`Param = LoWord(GamePoint)`、`Tag = HiWord(GamePoint)`
  （`PlayObject.Base.cs:927-939`）。
- 落库（`src/GameSrv/Word/WorldServer.cs:1293`）：`playerInfo.GamePoint = playObject.GamePoint`；
  进世界读回（同文件 `:1143`）。`characters.GamePoint` 列已存在。
- 部署可达：`Envir/Market_Def/功能NPC/真爱-G002.txt`、`真爱666-G002.txt` 共 72 行
  `GAMEPOINT - N`（各 36 行），经商人对话可达（T-142 残段原记录）。

### 1.2 实现

| 位置 | 改动 |
|---|---|
| `bins/game-svr/src/npc.rs` | 新增 `impl ScriptPlayer for NpcPlayer::set_game_point(v)`（原先落到 trait 默认 no-op，写值被丢） |
| `bins/game-svr/src/lib.rs::write_back_player_gold_deal` | 新增 `sg.game_point = p.game_point;`（`GotoLable` 结束回写会话） |
| `bins/game-svr/src/lib.rs::build_save_record` | 新增 `rec.data.game_point = sg.game_point;`（存档/落库，`WorldServer.cs:1293`） |

`npc_player_from_session` 原本已 seed `p.game_point = sg.game_point`，缺口只在写回与落档；
现三处闭环：会话 → 快照（已有）→ 动作写 → 写回 → 下一条命令可见 / 存档可见。

## 2. ⑩ 帧排水（可观测子集：② + ③ 已收，① 仍登记）

### 2.1 oracle 证据

- `MsgQueue` 优先级 = `(byte)wIdent`（`src/M2Server/Actor/BaseObject.Message.cs:181`；
  `TryDequeue` `:334`），.NET 四叉最小堆 ⇒ 同优先级**不是** FIFO
  （本仓 `crates/world/src/msgqueue.rs` 已逐位复刻 `[0,2,1]` / `[0,3,2,1]`）。
- `@back`（`src/GameSrv/Npc/Merchant.cs:922-929`）：**同一个 handler 内两次 `GotoLable`**
  （第一次是 `label` 本身，第二次是 `ScriptGoBackLable`/`@main`）⇒ 两轮所有 `SendMsg`
  进**同一个** `MsgQueue`，handler 末尾才出队。说词 `RM_MERCHANTSAY(11009&0xFF=1)`
  先于脚本帧 `RM_GAMEGOLDCHANGED(8084&0xFF=148)`。
- `SendUpdateMsg`（`BaseObject.Message.cs:283-303`）：循环里 `TryPeek` 永远看**队首**
  （最小优先级）；只有队首 ident 命中才 `TryDequeue`，随后 `i++`。说词（1）插在两条
  GAMEGOLD（148）之间时队首是说词 ⇒ 旧 GAMEGOLD **保留**（不是全删）。

### 2.2 实现

| 残段 | 修法 | 状态 |
|---|---|---|
| ② `@back` 两次 `GotoLable` 各自排水 | `handle_shop_cmd` 的 1011 分支：`@back` 的第一次输出**延后**，与第二次合并后一次 `emit_merchant_goto_result`（说词按出现序 → 两轮说词都先于脚本帧；帧内部仍由 `NetPriorityQueue` 按 `(byte)wIdent` 出队） | **已收口** |
| ③ `game_gold_changed` 用 `retain` 全删 | `NpcPlayer::game_gold_changed` 改为复刻 `SendUpdateMsg` 的**队首-only**：把待发帧按 `NetPriorityQueue` 建堆、只看队首 ident，命中才删一条，再入队新帧 | **已收口** |
| ① 1011 脚本帧（148）先于 `out` 面板/关闭帧（`RM_MENU_OK` 69 / `RM_MERCHANTDLGCLOSE` 143 / `RM_SENDGOODSLIST` 144 / `RM_SENDUSERSELL` 145） | 需要把 `out` 的每一帧按 oracle 的 **`SendSocket` 直连 vs `SendMsg` 入队**分类，再与脚本帧共用一条队列；本卡未做 | **仍登记**（部署不可达：T-142 原记录「扫 `Market_Def` 无『面板标签 + GAMEGOLD』组合、无 `[@back]`」；本卡不改变该结论） |

> 说明：本卡把「每次 `GotoLable` 立即发」里**可观测的两条**（② 跨轮次序、③ 同 ident 合并）
> 收口；① 的整 handler 队列需要逐帧分类 `SendSocket`/`SendMsg`，属独立重构输入，未在本卡
> 冒充完成。

## 3. ⑪ `lable_is_can_jmp` 单一实现（已收口）

- 引擎 `crates/script/src/engine/mod.rs` 的签名改为 `(can_jmp_labels: &[String],
  play_dice_label: &mut Option<String>, label: &str)`（原 trait getter/setter 版仅自测使用）。
- 宿主 `bins/game-svr/src/shop.rs` 删除第二份实现，改为
  `pub use mir2_script::engine::lable_is_can_jmp;` ⇒ `shop::lable_is_can_jmp` 路径与所有调用点不变。
- 语义不变：`@main` 恒可跳且不消费；表命中（OrdinalIgnoreCase）；`PlayDiceLabel` 命中即清
  `Some("")`（一次性），`None` 永不匹配。
- 源码序门：`t142_lable_is_can_jmp_has_single_definition`（`shop.rs` 不含 `pub fn`，引擎恰好 1 份）。

## 4. 合同测试（逐位）

| 测试 | 断言 |
|---|---|
| `t142_game_point_action_writes_back_save_and_emits_5008` | `GAMEPOINT - 3`：`65538 → 65535`；`SessionGround.game_point == 0xFFFF`；存档 `rec.data.game_point == 0xFFFF`；`SM_GAMEGOLDNAME` 的 `Param/Tag = (0xFFFF, 0)` |
| `t142_back_merges_two_goto_batches_into_one_handler_drain` | `@back`（两轮 `GAMEGOLD +1` + 各一条说词）：帧序 = `643, 643, 5008, 5008`（第二条说词 `say_positions[1] < gold_positions[0]`）；旧实现为 `643,5008,643,5008` |
| `t142_lable_is_can_jmp_has_single_definition` | 单一实现（源码序门） |
| 既有 `t142_*`（7 条） | 全部保持绿（`cargo test -p game-svr --lib t142` ⇒ 10 passed） |

## 5. 牙（≥2，各还原）

| 牙 | 变异 | 结果 |
|---|---|---|
| T1（⑨） | 注释 `write_back_player_gold_deal` 的 `sg.game_point = p.game_point;` | **RED**：`t142_game_point_action_*` 失败（`0 passed; 1 failed`），还原后绿 |
| T2（⑩②） | `let is_back = false;`（让 `@back` 两轮各自排水） | **RED**：`t142_back_merges_*` 失败（`0 passed; 1 failed`），还原后绿 |

两次均 `cp` 备份 + 还原，最终 `grep TOOTH` = 0。

## 6. 门禁

| 检查 | 结果 |
|---|---|
| `cargo fmt --all -- --check` | rc=0 |
| `cargo clippy --workspace --all-targets -- -D warnings` | rc=0 |
| `cargo test --workspace` | **3177 passed / 0 failed**（基线 3174 + 本卡 3，其中 1 条属源码序门） |
| `cargo run -p msgcodegen -- --check` | rc=0（604 条） |
| `conflict_marker` / `stale_pointer` / `dynamic_count_stamp` / `t_reservation` / `whitelist_anchor` / `m3_registry` / `board_coverage` | rc=0 |
| `git diff --check` | rc=0 |
| `git merge-tree --write-tree master HEAD` | 提交后复跑，见 patch body |
| C# `src/**` / 场景 JSON | 零变更 |

## 7. R 级窗口项（`window_required: true`）

1. **GAMEPOINT 真机**：赌博/功能 NPC（`真爱-G002` 等）执行 `GAMEPOINT - N` 后，两侧
   `characters.GamePoint` 落库 diff=0；`SM_GAMEGOLDNAME(5008)` 的 `Param/Tag` 逐字节同；
   断线重连后 `$GAMEPOINT` 与 5008 仍为新值。
2. **@back 排水**：真机商人 `@back`（Market_Def 有该标签的脚本）两轮说词均先于脚本帧；
   帧序与 oracle diff=0。
3. **① 残余**：若部署出现「面板标签 + GAMEGOLD」组合，需先补整 handler 队列（`SendSocket`
   直连 vs `SendMsg` 入队分类），再复拍；当前部署 0 处，不作为本卡窗口项。
4. `SendUpdateMsg` 同 ident 合并（③）：构造级测试已逐位；真机若出现「说词夹两条 GAMEGOLD」
   场景，复核帧数（2 条 5008）。

## 8. 残留

- ① 整 handler 级队列（`out`/直连帧与脚本帧统一）未做，见 §2.2；需要逐帧
  `SendSocket`/`SendMsg` 分类，建议另立 L 卡。
- `GAMEPOINT` 的 `characters.GamePoint` 列在 DBSrv 侧已有读写（`playdata.rs`），本卡只补
  game-svr 写点；真机落库仍需窗口。
- 临时物：`E:/tmp/lib-t142.bak`（牙备份，已还原；可删）、`E:/tmp/t142-*.log`、`/tmp/t1-9.log`、
  `/tmp/t2-10.log`。
- 未自批：审查归独立 principal；不 merge、不自签 review。
