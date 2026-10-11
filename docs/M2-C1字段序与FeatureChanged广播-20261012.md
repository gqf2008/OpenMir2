# M2- C-1 字段口径修复 + 装备 FeatureChanged 广播 — 2026-10-12

卡：`openmir2-m2-gamesvr-host`（owner `openmir2-worker-11`）。含两个逻辑变更（各自一个 commit）：

1. **C-1 返工**（reviewer-3 `5df38bad` request_changes；owner 裁决 `54182d61`，本片最高优先）；
2. **O-1**（reviewer-6 对 N-1 片的 finding）：装备/卸下触发对附近玩家的 `FeatureChanged(41)`。

## 1. C-1：`CM_TAKEONITEM`/`CM_TAKEOFFITEM` 字段口径

**问题**：Rust 旧实现读 `slot = tag`、`make_index = param`，与 oracle 相反 ⇒ 真客户端经 Rust 链
卸下恒不命中（`UseItems[slot].MakeIndex != make_index`）。

**oracle 口径**（owner `54182d61` 的四站点）：

| 站点 | 证据 |
| --- | --- |
| 冻结客户端 | `MirClient\Source\MirClient\ClMain.pas:7049` `MakeDefaultMsg(CM_TAKEONITEM, itmindex, where, 0, 0)`（调用点 `:4265` 传 `(where, g_EatingItem.MakeIndex, name)`）⇒ **recog=MakeIndex、param=slot** |
| C# 发送端 | `src/BotSrv/Player/RobotPlayer.cs:2643` `MakeMessage(CM_TAKEONITEM, itmindex, where, 0, 0)`；`src/OpenMir2/Messages.cs:846` `MakeMessage(msg, recog, param, tag, series)` |
| C# 转发 | `src/GameSrv/Word/WorldServer.cs:820` `SendMsg(playObject, Ident, Series, Recog, Param, Tag, sMsg)`；`BaseObject.Message.cs:98` 形参序 |
| C# 处理 | `src/M2Server/Player/PlayObject.Message.cs:821/824` `ClientTakeOnItems((byte)nParam2, nParam1, Msg)` ⇒ `slot=(byte)Param`、`make_index=nParam1=Recog` |

**修法**：

```rust
// handle_bag_cmd（新增 recog: i32 形参；sim 循环传 cmd.recog）
let slot = (param & 0x00FF) as u8; // oracle 的 (byte) 低 8 位截断，不用 try_from().unwrap_or(255)
let make_index = recog;
let _ = tag; // nParam3 未参与
```

**判据/红检**：新用例 `bag_cmd_field_order_is_recog_makeindex_param_slot`——`UseItems[0]` 放
`MakeIndex=60001`，请求 `recog=60001 / param=0 / tag=7（诱饵）` ⇒ 必须命中并卸下（出现 `619`）；
把 slot 改回从 `tag` 读 / `make_index` 改回 `param` ⇒ 该用例 **FAILED**（原始件同证据目录）。

## 2. O-1：装备 FeatureChanged 广播（`SM_FEATURECHANGED 41`）

成功换装（take-on/take-off）后按 oracle `BaseObject.FeatureChanged()`（`BaseObject.cs:1509` →
`SendRefMsg(RM_FEATURECHANGED, GetFeatureEx(), GetFeatureToLong(), 0,0)`）向**同图、周围
`SendRefMsgRange` 格（含自身格）内**的玩家广播 `SM_FEATURECHANGED`，并同步更新展示表
（`feature`/`light`，供后续 AOI `SM_TURN`）。

| 项 | oracle | Rust |
| --- | --- | --- |
| 范围 | `BaseObject.Message.cs:383-470` 按 `CurrX/Y ± Config.SendRefMsgRange` 扫描单元格 | `|dx|,|dy| <= range` 同图 |
| 帧 | `PlayObject.Message.cs:1762-1764`：`(recog=src.ActorId, param/tag=LoWord/HiWord(feature), series=GetFeatureEx())` | `smframe::sm_featurechanged` |
| 队列序 | `RM_FEATURECHANGED=10116`（低字节 68）排在 `RM_ABILITY(19)`/`RM_SUBABILITY(62)` 之后 | 发在 52/752 之后 ⇒ `200 → OK/FAIL → 52 → 752 → 41` |
| 配置 | `[Setup] SendRefMsgRange`（`GameSvrConf.cs:499` `byte` 缺省 `12`；`ServerConf.cs:280`） | `GateConfig.send_ref_msg_range`，缺键 12、非法报错 |

**登记（未建模，不静默）**：`SendRefMsg` 的 500ms `VisibleHumanList` 重建缓存、`ObMode/FixedHideMode`
的“只发自己”分支、非玩家 `WantRefMsg` 目标。以上均不产生本片判据内线上差异。

**判据/红检**：`bag_equip_broadcasts_feature_changed_to_nearby`（`hero` 线
`[200,619,620,52,752,41]`；`watcher` 线只 `[41]`（`recog=hero`））；不广播 / 过滤自己 ⇒ 必红。

## 3. 门禁（隔离 `CARGO_TARGET_DIR=E:\tmp\openmir2-m2svr-target`，`--offline`）

`fmt` 0 / `clippy --workspace --all-targets -D warnings` 0 / `test --workspace` **3 连跑** 每次
**71 suites / 525 passed / 0 failed**（基线 522 + 本片 3 条）/ `msgcodegen` 604 rc=0。
证据与红检原始件：`tools/m2-acceptance/evidence/m2svr-featurechanged-20261012/**`。

## 4. 边界与残留

- 未动 `src/**`（C# oracle）、`MirClient`、表结构/脚本语法/数据格式；未动 `crates/**`。
- `f308d248` 的 ③ 52 复查证据（`m2svr-whitelist-rerun-20261012/**`）按 owner `54182d61`(5) 原样保留。
- 本片未起栈/未占端口（无真机采集）；`see-others` 换装真机回归可随下一次窗口的 M2 验收一并采。
