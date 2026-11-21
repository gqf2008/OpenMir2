# 换图簇缺 `SM_SERVERCONFIG(11029)`（登记差异）

> 发现于卡 `magic-push-batch-impl`（MG-35 瞬息移动 21）。**本卡只登记，不改产品**。
> 结论：`MG-35` 本批**不升格**，原因是本条已登记缺口；`MG-16(8)` / `MG-57(37)` 不受影响。

## 1. 现象（golden 实测）

`spacemove` 三支（s1/s5/s13）的 caster 侧 golden `Sent`（`mir2-rs/tests/parity/golden/magic/spacemove.json`）：

```text
806, 638, 633, 634(x,y,series), 766, 54, 11029, 807, 640
```

Rust 在同一位置给出 `806, 638, 633, 634, 766, 54, 807, 640` —— **缺 `11029`**。
其余字段（634 的 `param/tag/series`、807 落点、640 训练尾）两侧逐字节一致。

## 2. oracle 依据

`src/M2Server/Player/PlayObject.Message.cs:1772-1778` 的 `case RM_CHANGEMAP` 是同支**四连发**：

```text
SendDefMessage(SM_CHANGEMAP, ActorId, CurrX, CurrY, DayBright(), processMsg.Msg);  // 634
RefUserState();                                                                      // 766
SendMapDescription();                                                                // 54
SendServerConfig();                                                                  // 11029
```

`SendServerConfig()`（`src/M2Server/Player/PlayObject.Base.cs:941-971`）在 `SoftVersionDateEx == 0`
时直接 `return`，否则 `EDCode.EncodeMessage(SystemShare.Config.ClientConf)` 经
`SendDefMessage(SM_SERVERCONFIG, 0,0,0,0, sMsg)` 发出（尾是**双重编码**）。
装置四个对象都设 `SoftVersionDateEx = 20240101` ⇒ 该帧必发。

## 3. Rust 现状与缺口位置

- 换图簇装配：`game_svr::sessionwire::map_change_frames`（633/634/[801|807]/766/54，
  `Route::SelfOnly` 与 `Route::Ref` 分类）。
- 生产 drain：`drain_map_change` → `drain_map_change_at_time`（同一 tick 的下行段统一发出）。
- `smframe::sm_serverconfig(cfg)` **已在仓内**（登录路径使用），本缺口缺的不是装配函数，
  而是**输入拿不到**。

## 4. 为什么本批不做（成本核算）

`SendServerConfig` 需要两样输入：

1. `GameSvrConf`（`client_conf.to_bytes()`）——只在 `sim_loop` 的 `enter_cfg.game_conf` 里；
2. 每个会话的 `SoftVersionDateEx`——在 `SessionGround.soft_version_date`（`handle_move_cmd`
   一侧有），但 `drain_map_change_at_time` 的入参里没有会话表。

显式传参要改：

- `drain_map_change` / `drain_map_change_at_time` **2 个签名**；
- 调用点 **11 处**（生产 3 处 + 测试/driver 8 处：`eat_move` 5、`lib.rs` 单测 2、21 驱动 1）；
- `handle_move_cmd` 还要加参数、并同步它的 **9 处测试调用**。

超过卡 `magic-push-batch-impl` 的止损线（>5 调用点）⇒ 停手登记。
另：靠全局 `OnceLock` 存配置会造成"第二份配置真源"，与 `enter_cfg` 单源口径冲突，不采用。

## 5. 移除条件

满足任一即可接线，并在同一提交删除本条登记：

- 把 `GameSvrConf`（或已序列化的 `client_conf` 字节 + 会话 `SoftVersionDateEx`）显式注入
  `drain_map_change*`；或
- 让 `MapChange` 事件携带该输入（会动 `crates/world-bevy` 的公开 API，需单独立卡评估）。

接线后的验收：`spacemove` 三支的 caster 侧 `Sent` 出现 `54 → 11029 → 807`（**严格序**），
且 `SM_SERVERCONFIG` 的尾与 golden 逐字节一致；`SoftVersionDateEx == 0` 的负例不得发帧
（可用登录路径既有用例同形）。

## 6. 比较器的处置（本批）

`real_rust_ledger_spacemove` **保持未通过**，且**不得**用白名单/`keep` 集合把 `11029` 排除
（那会把"缺帧"洗成静默绿）。失败原句必须点名 `11029`：

```text
[某 label/caster] Sent 条数不等（golden=9 rust=8）：golden 有 rust 无=[(11029, 0, 0, 0, 0, 0, "<尾>")]
```

比较器的 `#[ignore]` 理由保持"需先跑驱动生成台账"，**不**加"已知缺 11029"之类跳过措辞
（默认 `cargo test` 不受影响；显式 `--ignored --exact` 才会跑到）。

## 7. 与 `634.series` 无关

`634` 的 `series` 时变（真机小时 ⇒ `DayBright()`）已在卡 `magic-push-batch-impl` §4b 处理
（golden 记 `Fixture.map_bright`、驱动注入），与本条的缺帧是两个独立问题。
