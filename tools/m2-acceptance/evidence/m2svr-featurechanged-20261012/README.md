# m2svr-featurechanged-20261012 — C-1 字段口径修复 + 装备 `FeatureChanged()` 广播 证据

卡：`openmir2-m2-gamesvr-host`（owner `openmir2-worker-11`）。两个逻辑变更（两 commit）：

1. **C-1 返工（reviewer-3 `5df38bad`，owner 裁决 `54182d61`）**：`CM_TAKEONITEM`/`CM_TAKEOFFITEM`
   的字段口径改为 oracle（`Recog`(b0-3)= `MakeIndex`、`Param`(b6-7)= 槽位（`(byte)` 低 8 位）、`Tag`(b8-9) 未用）。
2. **O-1（reviewer-6 finding）**：装备/卸下成功后按 `BaseObject.FeatureChanged()` 向同图
   `SendRefMsgRange` 格内玩家（含自己）广播 `SM_FEATURECHANGED(41)`。

## C-1（字段口径）

oracle 链（owner `54182d61` 的四站点 + 复核）：

| 站点 | 证据 |
| --- | --- |
| 冻结客户端 | `MirClient\Source\MirClient\ClMain.pas:7049` `MakeDefaultMsg(CM_TAKEONITEM, itmindex, where, 0, 0)`（调用点 `:4265` 传 `(where, g_EatingItem.MakeIndex, name)`）⇒ **recog=MakeIndex、param=slot** |
| C# 发送端 | `src/BotSrv/Player/RobotPlayer.cs:2643` `MakeMessage(CM_TAKEONITEM, itmindex, where, 0, 0)`；`Messages.cs:846` `MakeMessage(msg, recog, param, tag, series)` |
| C# 转发 | `src/GameSrv/Word/WorldServer.cs:820` `SendMsg(playObject, Ident, Series, Recog, Param, Tag, sMsg)`；`BaseObject.Message.cs:98` 形参序 |
| C# 处理 | `src/M2Server/Player/PlayObject.Message.cs:821/824` `ClientTakeOnItems((byte)nParam2, nParam1, Msg)` ⇒ slot=(byte)Param、make_index=nParam1=Recog |
| Rust 旧实现（错） | `let slot = u8::try_from(tag).unwrap_or(u8::MAX); let make_index = i32::from(param);` |
| Rust 修复 | `let slot = (param & 0x00FF) as u8; let make_index = recog;`（`handle_bag_cmd` 新增 `recog` 形参，sim 循环传 `cmd.recog`） |

判据：新用例 `bag_cmd_field_order_is_recog_makeindex_param_slot`（`UseItems[0]` 放 `MakeIndex=60001`，
请求 `recog=60001 / param=0 / tag=7（诱饵）` ⇒ 必须卸下成功、出现 `SM_TAKEOFF_OK(619)`）。
`bag_takeoff_recalc_uses_prior_subability`、`bag_equip_broadcasts_feature_changed_to_nearby` 的
调用点同步改为新字段序。

## O-1（FeatureChanged 广播）

| 项 | oracle | Rust |
| --- | --- | --- |
| 触发 | `PlayObject.Operate.cs:459`/`:548` `FeatureChanged()` | `handle_bag_cmd` 的 `if out.recalc` |
| 广播 | `BaseObject.cs:1509` → `SendRefMsg`（`BaseObject.Message.cs:383-470`） | `broadcast_feature_changed` |
| 范围 | `CurrX/Y ± Config.SendRefMsgRange`（缺省 12，**含自身格**） | `|dx|,|dy| <= range` 同图 |
| 帧 | `PlayObject.Message.cs:1762-1764`：`(recog=src.ActorId, param/tag=LoWord/HiWord(feature), series=GetFeatureEx())` | `smframe::sm_featurechanged` |
| 队列序 | `RM_FEATURECHANGED=10116`（(byte)=68）排在 `RM_ABILITY(19)`/`RM_SUBABILITY(62)` 后 | 发在 52/752 之后 ⇒ `200 → OK/FAIL → 52 → 752 → 41` |

配置：新增 `[Setup] SendRefMsgRange`（`GameSvrConf.cs:499` `byte` 缺省 `12`；`ServerConf.cs:280`），
缺键取缺省、非法报错。

**登记（未建模，不静默）**：`SendRefMsg` 的 500ms `VisibleHumanList` 重建缓存、
`ObMode/FixedHideMode` 的“只发自己”分支、非玩家 `WantRefMsg` 目标。

## 判据 / 红检（原始件同目录，`* -text` 冻结字节）

```
cd mir2-rs
CARGO_TARGET_DIR=E:/tmp/openmir2-m2svr-target cargo test -p game-svr --offline tests::bag_
```

| 文件 | 内容 | sha256 |
| --- | --- | --- |
| `green.txt` | `tests::bag_` 4 passed（字段序 / N-1 / FeatureChanged / bag_items_from） | `70abf48a…` |
| `redcheck-field-order.txt` | C-1：slot 改回 tag + make_index 改回 param ⇒ **FAILED**（未卸下） | `a332807e…` |
| `redcheck-no-broadcast.txt` | O-1：`broadcast_feature_changed` 用 `if false` 包住 ⇒ **FAILED**（hero 末条缺 41） | `8e75c83b…` |
| `redcheck-no-self.txt` | O-1：广播收件人过滤掉自己 ⇒ **FAILED**（同上） | `a6302c59…` |
| `restored.txt` | FeatureChanged 还原后 `1 passed` | `3b6d972f…` |

## 门禁（隔离 `CARGO_TARGET_DIR=E:\tmp\openmir2-m2svr-target`，`--offline`）

- `cargo fmt --all -- --check` rc=0
- `cargo clippy --workspace --all-targets -- -D warnings` rc=0
- `cargo test --workspace --offline` **3 连跑**：每次 rc=0 / 71 suites / **525 passed / 0 failed**
  （master `72513e42` 基线 522；+3 = 本片新增 `bag_cmd_field_order_…`、`bag_equip_broadcasts_…` 与 config 断言）
- `cargo run -p msgcodegen -- --check` rc=0（604）

原始日志：`E:/tmp/openmir2-watch/pi-m2svr-featchanged/{fmt,clippy,test1,test2,test3,msgcodegen}.log`。

> 注：`f308d248` 的 ③ 52 复查证据（`m2svr-whitelist-rerun-20261012/**`）按 owner `54182d61`(5) 原样保留，
> 本片不改。
