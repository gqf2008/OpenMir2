# M2-装备 FeatureChanged 广播（SM_FEATURECHANGED 41）— 2026-10-12

卡：`openmir2-m2-gamesvr-host`（owner `openmir2-worker-11`）。
动机：reviewer-6 对 N-1 片的 finding **O-1**（“装备触发的 `FeatureChanged(41)` 给附近玩家未建模”，
原为如实登记项）与 owner 的后续候选。**行为等价优先**：照 oracle 1:1 接线，不引入新偏差。

## 1. 改了什么

`mir2-rs/bins/game-svr/src/main.rs`

- `handle_bag_cmd`（`CM_TAKEONITEM` 1003 / `CM_TAKEOFFITEM` 1004）**成功分支**（`out.recalc`）在
  `SM_ABILITY(52)`、`SM_SUBABILITY(752)` 之后广播 `SM_FEATURECHANGED(41)`：
  - 半径 = `[Setup] SendRefMsgRange`（同图、`|dx| <= range && |dy| <= range`，**含自身格**）；
  - `recog = actor_id`、`param/tag = LoWord/HiWord(GetFeatureToLong())`、`series = GetFeatureEx()`；
  - 同时把展示表 `ActorDisplay` 的 `feature`/`light` 更新为换装后的值（供后续 AOI `SM_TURN` 用）。
- 新增纯函数 `broadcast_feature_changed(...)`（按 `[GateIdx,Socket]` 路由，失败记 WRN，不静默挑线）。

`mir2-rs/bins/game-svr/src/config.rs`

- 新增 `[Setup] SendRefMsgRange`（`GateConfig.send_ref_msg_range: u8`）：
  oracle `GameSvrConf.cs:499` 是 `byte`、缺省 `12`（`GameSvrConf.cs:1503`），
  `ServerConf.cs:280` 用 `ReadWriteInteger` 读；**缺键取缺省、写了但非法报错**（与本仓既有约定一致）。

## 2. oracle 依据（逐行）

| 项 | oracle | Rust |
| --- | --- | --- |
| 触发 | `PlayObject.Operate.cs:459`（take-on 成功）/`:548`（take-off 成功）调 `FeatureChanged()` | `handle_bag_cmd` 的 `if out.recalc` |
| 实现 | `BaseObject.cs:1509` `FeatureChanged()` → `SendRefMsg(RM_FEATURECHANGED, GetFeatureEx(), GetFeatureToLong(), 0,0)` | `broadcast_feature_changed` |
| 广播范围 | `BaseObject.Message.cs:383-470`：按 `CurrX/Y ± Config.SendRefMsgRange` 扫描单元格，命中**玩家**逐一下发；自身格在范围内 ⇒ **自己也收** | 同图 + `|dx|,|dy| <= range` |
| 帧字段 | `PlayObject.Message.cs:1762-1764`：`SendDefMessage(SM_FEATURECHANGED, processMsg.ActorId, LoWord(nParam1), HiWord(nParam1), wParam)` | `smframe::sm_featurechanged(actor, feature, feature_ex)` |
| 线上序 | `RM_ABILITY=10051(19)`、`RM_SUBABILITY=10302(62)`、`RM_FEATURECHANGED=10116(68)` 同入 `MsgQueue`（优先级 = 低字节）⇒ `52 → 752 → 41` | 发在 52/752 之后 |
| 特征值 | `GetFeatureToLong()` 读换装后的 `UseItems[Dress/Weapon]`；`GetFeatureEx()` = `MakeWord(OnHorse?HorseType:0, DressEffType)` | `ability::equipped_feature(..., &use_items, gender, hair)`；`feature_ex=false,0,0` |

**登记（未建模，不静默）**：

1. `SendRefMsg` 的 `VisibleHumanList` 有 **500ms 重建缓存**（`BaseObject.Message.cs:395-397/455`）——
   本实现每次重建 12 格集合；装备是低频操作，首刷口径一致。该缓存对所有 `RM_*` 广播同源，
   留待需要时统一按 oracle 接。
2. `ObMode/FixedHideMode` 的“只发自己”分支未建模（现网玩家常规形态不进该分支）。
3. 非玩家目标（`WantRefMsg` 的 `RM_STRUCK/HEAR/DEATH`）不在本实现内（本宿主生产路径只承载玩家）。

以上三项**均不产生本片判据内的线上差异**，故不新增 `T-*` 白名单；如后续真机对拍出现差异再登记。

## 3. 判据与红检

- 判据：进程/单元级用例 `bag_equip_broadcasts_feature_changed_to_nearby`
  （`hero`(10,10)/`watcher`(12,10) 同图；`hero` 卸装 ⇒ `hero` 线 `[200,619,620,52,752,41]`，
  `watcher` 线只 `[41]`（`recog=hero`））。
- 红检：① 不广播 ⇒ 必红；② 收件人过滤掉自己 ⇒ 必红。原始件与 sha256 见
  `tools/m2-acceptance/evidence/m2svr-featurechanged-20261012/README.md`。

## 4. 门禁（隔离 `CARGO_TARGET_DIR=E:\tmp\openmir2-m2svr-target`，`--offline`）

`fmt` 0 / `clippy --workspace --all-targets -D warnings` 0 / `test --workspace` **3 连跑** 每次
**71 suites / 524 passed / 0 failed**（基线 522 + 本片 2 条）/ `msgcodegen` 604 rc=0。

## 5. 残留 / 后续

- **本片未跑真机**：`m2-04-see-others` 的真机回归（第二个角色换装 ⇒ 观察者收到 `41`）可随下一次
  窗口的 M2 验收一并采；本片只给进程级判据（不占端口）。
- 入图 16 条自帧的台账（`docs/M2顺序判据②-对账台账.md`）不受本片影响（`41` 已是入图自帧，
  AOI 侧的 `41` 按 owner §22 单独判）。
