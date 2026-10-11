# m2svr-featurechanged-20261012 — 装备/卸下 `FeatureChanged()`（SM_FEATURECHANGED 广播）证据

卡：`openmir2-m2-gamesvr-host`（owner `openmir2-worker-11`）。片：reviewer-6 对 N-1 片的
finding **O-1**（“装备触发对附近玩家的 `FeatureChanged(41)` 未建模”）+ owner `b8f40ac4`/§26 的后续候选。

## 改动

- `mir2-rs/bins/game-svr/src/main.rs`：`handle_bag_cmd` 成功分支（`out.recalc`）在 `SM_ABILITY(52)`/
  `SM_SUBABILITY(752)` 之后按 oracle `BaseObject.FeatureChanged()`（`BaseObject.cs:1509` →
  `SendRefMsg(RM_FEATURECHANGED, GetFeatureEx(), GetFeatureToLong(), 0,0)`）向**同图、周围
  `SendRefMsgRange` 格（含自身格）内**的玩家广播 `SM_FEATURECHANGED(41)`；并同步更新展示表
  （`FeatureToLong` + `GetMyLight`）。
- `mir2-rs/bins/game-svr/src/config.rs`：新增 `[Setup] SendRefMsgRange`（oracle `GameSvrConf.cs:499`
  缺省 `12`；`ServerConf.cs:280` 的 `ReadWriteInteger`），缺键取缺省、非法报错。

## oracle 口径（逐行）

| 项 | oracle 位置 | Rust |
| --- | --- | --- |
| 触发点 | `PlayObject.Operate.cs:459`（take-on 成功）/`:548`（take-off 成功） | `main.rs::handle_bag_cmd` 的 `if out.recalc` |
| 广播 | `BaseObject.cs:1509` → `SendRefMsg`（`BaseObject.Message.cs:383-470`） | `main.rs::broadcast_feature_changed` |
| 范围 | `CurrX/Y ± Config.SendRefMsgRange`（缺省 12，**含自身格**） | `(t.x-sx).abs() <= range && (t.y-sy).abs() <= range` 同图 |
| 帧 | `PlayObject.Message.cs:1762-1764`：`(recog=src.ActorId, param/tag=LoWord/HiWord(feature), series=GetFeatureEx())` | `smframe::sm_featurechanged` |
| 队列序 | `RM_FEATURECHANGED=10116` ⇒ `(byte)=68`，排在 `RM_ABILITY(19)`/`RM_SUBABILITY(62)` 后 | 发在 52/752 之后 ⇒ `200 → OK/FAIL → 52 → 752 → 41` |

**登记（未建模，不静默）**：`SendRefMsg` 的 `VisibleHumanList` 500ms 重建缓存、`ObMode/FixedHideMode`
的“只发自己”分支、非玩家目标的 `WantRefMsg`（本宿主只承载玩家）。对装备路径（低频）不影响首刷。

## 判据

```
cd mir2-rs
CARGO_TARGET_DIR=E:/tmp/openmir2-m2svr-target cargo test -p game-svr --offline bag_equip_broadcasts_feature_changed_to_nearby
```

用例造 `hero`(10,10) / `watcher`(12,10) 同图，`hero` 卸下 `UseItems[0]` 的木剑 ⇒
- `hero` 线出帧序 `[200, 619, 620, 52, 752, 41]`；
- `watcher` 线只收 `[41]`，`recog=hero`、`param/tag=LoWord/HiWord(GetFeatureToLong())`；
- 展示表 `feature` 用换装后的值更新（不等于换装前的 111）。

## 红检（改坏必红，原始件同目录）

| 文件 | 变异 | 结果 | sha256 |
| --- | --- | --- | --- |
| `green.txt` | —（基线） | `1 passed; 0 failed` | `bbbf219a…` |
| `redcheck-no-broadcast.txt` | `broadcast_feature_changed(...)` 用 `if false { … }` 包住 | **FAILED** `left:[200,619,620,52,752] right:[…,41]` | `8e75c83b…` |
| `redcheck-no-self.txt` | 广播收件人过滤掉自己 | **FAILED** 同上（`hero` 末条不再为 `41`） | `a6302c59…` |
| `restored.txt` | 还原后 | `1 passed; 0 failed` | `3b6d972f…` |

## 门禁（隔离 `CARGO_TARGET_DIR=E:\tmp\openmir2-m2svr-target`，`--offline`）

- `cargo fmt --all -- --check` rc=0
- `cargo clippy --workspace --all-targets -- -D warnings` rc=0
- `cargo test --workspace --offline` **3 连跑**：每次 rc=0 / 71 suites / **524 passed / 0 failed**
  （基线 master `72513e42` 为 522；+2 = 本片新增 `bag_equip_broadcasts_feature_changed_to_nearby`
  与 `config` 断言）
- `cargo run -p msgcodegen -- --check` rc=0（604）

原始日志：`E:/tmp/openmir2-watch/pi-m2svr-featchanged/{fmt,clippy,test1,test2,test3,msgcodegen}.log`。
