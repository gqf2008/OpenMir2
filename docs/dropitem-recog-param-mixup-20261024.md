# CM_DROPITEM 字段错读修复：`nParam1` 取线缆 `Recog`（= `MakeIndex`）

- 卡片：`openmir2-dropitem-recog-param-mixup`（tier **R**，生产协议字段修复）
- 分支：`card/dropitem-recog-param-mixup`（base `walgit/master` = `f2ad4f6bb`）
- 证据源头：`openmir2-compare-ident600-registration` 的越界发现段（`docs/compare-ident600-registration-20261024.md` §7）
- 结论：Rust 派发层把 `CM_DROPITEM` 的 `nParam1` 读成 12B 头的 `param`，oracle 读的是**线缆 `Recog`**。
  已按 oracle 改取 `Recog`，并同步修正 `lib.rs` 的误导注释。

## 1. oracle 证据链（逐环，600 成功 / 601 失败两支同源）

| # | 位置 | 事实 |
|---|------|------|
| ① | `src/M2Server/Player/PlayObject.Message.cs:801-810` | `case Messages.CM_DROPITEM:` ⇒ `:802` `ClientDropItem(processMsg.Msg, processMsg.nParam1)`；成功支 `:804` `SendDefMessage(Messages.SM_DROPITEM_SUCCESS, processMsg.nParam1, 0, 0, 0, processMsg.Msg)`；失败支 `:808` 同形（`SM_DROPITEM_FAIL`）。两支的 `Recog` 都是 `processMsg.nParam1`。常量：`src/OpenMir2/Messages.cs:277` `SM_DROPITEM_SUCCESS = 600`、`:278` `SM_DROPITEM_FAIL = 601`。 |
| ② | `src/GameSrv/Word/WorldServer.cs:796-820` | `case Messages.CM_DROPITEM:`（与 `CM_TAKEONITEM`/`CM_TAKEOFFITEM`/`CM_DEAL*` 等同组，`:820`）⇒ `playObject.SendMsg(playObject, defMsg.Ident, defMsg.Series, defMsg.Recog, defMsg.Param, defMsg.Tag, sMsg)`。按 `SendMsg` 形参序，实参位 `nParam1 = defMsg.Recog`（= **线缆 Recog**），`nParam2 = defMsg.Param`、`nParam3 = defMsg.Tag`。 |
| ③ | `src/M2Server/Actor/BaseObject.Message.cs:98`（构造）；`:357-375`（`GetMessage`） | `SendMsg(IActor baseObject, int wIdent, int wParam, int nParam1, int nParam2, int nParam3, string sMsg)` ⇒ `GetMessage` 把 `sendMessage.nParam1` **原样**写入 `ProcessMessage.nParam1` ⇒ ①的 `processMsg.nParam1 == defMsg.Recog`。 |
| ④ | `src/M2Server/Player/PlayObject.Socket.cs:27-38` | `SendDefMessage(short wIdent, int nRecog, int nParam, int nTag, int nSeries, string sMsg)` ⇒ `Messages.MakeMessage(wIdent, nRecog, nParam, nTag, nSeries)`（`src/OpenMir2/Messages.cs:846`）。 |
| ⑤ | `src/OpenMir2/Packets/ClientPackets/ClientMesaagePacket.cs:11-25`（`CommandMessage`） | `Recog` 是 **`int`（4B）**，`Ident/Param/Tag/Series` 是 `ushort`（`MakeMessage` 对后三者做 `(ushort)` 截断，`Recog` 不截断）。⇒ 600/601 的 `Recog` 必须承载 `MakeIndex` 的 **i32 全宽**。 |

闭环：`CM_DROPITEM` 线缆 `Recog` ⇒ `defMsg.Recog` ⇒ `SendMsg.nParam1` ⇒ `processMsg.nParam1` ⇒
`ClientDropItem(..., nParam1)` 与 `SendDefMessage(600/601, nParam1, …)` 的 `Recog`。
`param`（线缆 `Param`）在本链路里**只**落 `processMsg.nParam2`，oracle 的 drop 支不读它。

真机锚（不改夹具，只引用）：`mir2-rs/tests/parity/m3/fixtures/ground_vectors.json`
（`oracle_source_sha256 = cff86a3e71d3a4480880d0a4894446e3bc83fd2a7b0e4a0815c83ed24229a8f1`）
的 `drop_ok_control_price` ⇒ `{ident:600, recog:52, param:0, tag:0, series:0}`，
`drop_fail_*` 全为 601 同形；即 oracle 的 600/601 `Recog` = 请求 `MakeIndex`。

## 2. 新旧行为对照

Rust 旧实现（`handle_ground_cmd` 的 drop 支）：

```rust
make_index: i32::from(param),   // 12B 头的 Param（u16→i32）
```

新实现：

```rust
make_index: n_param1,           // oracle processMsg.nParam1 = 线缆 Recog（i32）
```

| 场景（客户端线缆 `Recog=X`/`Param=P`） | 旧行为 | 新行为（= oracle） |
|---|---|---|
| `P=0`，背包有 `MakeIndex=X` 的物品 | 按 `MakeIndex=0` 查包 ⇒ 查不到 ⇒ **601**，帧 `Recog=0` | 按 `MakeIndex=X` 查包 ⇒ 该成功的照常 600 / 该失败的 601，帧 `Recog=X` |
| `P` 恰等于**另一件**物品的 `MakeIndex` | 可能丢弃/扣耐久的不是请求的物品 | 只匹配 `X` |
| `X > 65535` | 即便传 `param` 也装不下（`Param` 是 u16） | `Recog` 按 i32 全宽承载（④⑤） |
| `CM_PICKUP` | 位置门用 `param/tag`（= `nParam2/nParam3`） | **不变**（位置门仍读 `param/tag`） |

`CM_PICKUP` 语义未受影响：`PlayObject.Message.cs:811-816` 的位置门读的是
`nParam2/nParam3`，对应 12B 头的 `param/tag`；本卡只把 drop 支的 `nParam1` 来源纠正为 `Recog`。

## 3. 修法

`mir2-rs/bins/game-svr/src/lib.rs`：

1. `handle_ground_cmd` 形参加 `n_param1: i32`（= oracle `processMsg.nParam1`），drop 支改
   `make_index: n_param1`；`param`/`tag` 仍只用于 pickup 位置门。
2. 生产调用点（`sim_loop` 的 `CM_PICKUP`/`CM_DROPITEM` 派发）传 `cmd.recog`。
3. 修正 `handle_ground_cmd` 文档注释（原写「`nParam1`（`param`）= 要丢的 `MakeIndex`」），
   改为「`nParam1` = 线缆 `Recog`」并附 ②③ 的来源。
4. 更新 6 处既有测试调用点（4 处 `CM_PICKUP` 传 `0` 占位；2 处 drop 传 `60001` 作为
   `n_param1`，其 `param` 补 `0`）。

未改：C# 侧、`ground.rs` 纯函数（本来就正确）、`pickwire`/`smframe` 编码、600 规则表（在审卡）、
`pickup_item` 场景 JSON。

## 4. 合同测试（新增）

`mir2-rs/bins/game-svr/src/lib.rs` ⇒ `dropitem_reply_recog_is_wire_recog_not_param`：

- **① 向量驱动（8 例）**：对 `ground_vectors.json` 中派发层可复现的全部 drop 用例
  （`drop_fail_item_id_zero`、`drop_ok_control_price`、`drop_fail_control_price_too_high`、
  `drop_fail_deal_tick_guard`、`drop_fail_name_with_space`、`drop_fail_makeindex_mismatch`、
  `drop_stdmode40_dura_normal`、`drop_stdmode40_dura_underflow`），走生产入口
  `handle_ground_cmd(CM_DROPITEM, …)`：线缆 `recog = request.make_index`、
  `param/tag = 诱饵（≠Recog）` ⇒ 断言 600/601 帧
  `(ident, recog, param, tag, series)` 与 oracle 向量逐字段一致、`body[0..4] == recog.to_le_bytes()`、
  整帧等于 `sm_dropitem_reply(ident==600, recog, EncodeString(name))`。
- **② 同值帧**：`drop_ok_control_price` 传 `recog == param == 52` ⇒ 仍断言整帧 shape
  （`ident=600`、`Recog=52`、`Param/Tag/Series ≡ 0`），防「恰好相等」假绿。
- **③ i32 全宽**：自造 `MakeIndex = 100_001`（> u16）的背包物品，线缆 `recog=100_001`、
  `param=0` ⇒ 断言 600 帧 `Recog=100_001` 且前 4 字节为 `100_001i32` LE。

派发层不建模的 3 例（不在此层复现，纯函数面已由 `ground.rs` 覆盖）：
`drop_fail_safe_zone`（`Flag.SafeArea`，本层无 registry）、`drop_fail_no_throw_item`
（`Flag.NoThrowItem`）、`drop_fail_is_can_drop_false`（`IsCanDrop=false` 是状态面，派发层按生产恒
`is_can_drop: true`）。

## 5. 牙（实际跑过的变异）

| 牙 | 变异 | 复跑结果 |
|---|---|---|
| ① 改回 `cmd.param` | `make_index: n_param1` → `i32::from(param)` | **RED**：`drop_fail_item_id_zero: 600/601 帧头须与 oracle 向量逐字段一致` — `left (601, 52, 0,0,0)` vs `right (601, 51, 0,0,0)`（52 = 诱饵 `param`） |
| ② i32 截断 | `make_index: n_param1` → `i32::from(n_param1 as u16)` | **RED**：`Recog 必须承载 > u16 的 MakeIndex（i32 全宽）` — `left (601, 34465, 0,0,0)` vs `right (600, 100001, 0,0,0)` |
| 同值 shape | 见测试 ②；`recog==param` 时仍断言 `param/tag/series ≡ 0` | GREEN（该断言在①变异下也通过——正因如此才需要诱饵算例；两者合起来才不留「恰好相等」假绿） |

两次变异均已还原后复跑 GREEN（`1 passed; 0 failed`）。

## 6. 门禁（本分支实跑）

| 命令 | 结果 |
|---|---|
| `cargo fmt --all -- --check` | rc=0 |
| `cargo clippy --workspace --all-targets -- -D warnings` | rc=0 |
| `cargo test --workspace` | rc=0（3174 passed / 0 failed / 62 ignored） |
| `cargo run -p msgcodegen -- --check` | rc=0（604 条常量与 C# 源一致） |
| `python -I -B tools/whitelist/t_reservation_gate.py check` | rc=0 |
| `python -I tools/hygiene/conflict_marker_gate.py` | rc=0 |
| `python -I -B tools/hygiene/stale_pointer_gate.py` | rc=0 |
| `python -I -B tools/hygiene/dynamic_count_stamp_gate.py` | rc=0 |
| `git diff --check` | rc=0 |
| `git merge-tree`（vs `walgit/master`） | 无冲突（无输出） |

R 级窗口登记：本卡改的是**生产协议字段语义**（`CM_DROPITEM` 的 `Recog`），按分级属 R；
真机判据（pickup 链的「丢→走→拾」端到端 + 600/601 帧 Recog 对拍）留待下轮窗口。
合并后卡片保持 `in-progress`（`window_required`）。

## 7. 残留物清单

- 无临时文件/进程/端口（本卡只跑 cargo 与只读门禁；未起服务端/客户端）。
- 未修的相关面（登记，非本卡写集）：`CommandMessage` 的 `param/tag/series` 在 `MakeMessage`
  中按 `(ushort)` 截断是 oracle 既有行为，未动；`ground.rs` 纯函数无需改。
