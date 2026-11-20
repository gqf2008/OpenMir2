# 片① 逐帧快照装置（卡 `openmir2-tick-frame-priority-queue`）

本目录是片①（`TickFrameQueue` 抽取）的**复跑装置与两棵树的输出**。装置本身是临时 test 模块，
**未随交付代码入库**；这里放的是它的源码快照，保证「改造前/后逐帧字节相同」这条判据可被重放。

| 文件 | 内容 |
|---|---|
| `tickframe_snapshot.rs` | 测试模块源码（`#[cfg(test)]`）；5 个场景，把下行帧按 `gate/socket/ident/body-hex` 写成 NDJSON |
| `before.ndjson` | 改造前树（master `bb160e2ad`）的输出 |
| `after.ndjson` | 改造后树（代码 head `db60e9f32`）的输出 |

## 复跑步骤

1. 把 `tickframe_snapshot.rs` 复制到 `mir2-rs/bins/game-svr/src/`；
2. 在 `mir2-rs/bins/game-svr/src/lib.rs` 的测试模块声明区（`#[cfg(test)] mod aoi_golden;` 旁）加一行
   `#[cfg(test)] mod tickframe_snapshot;`；
3. 运行（PowerShell）：

   ```powershell
   cd mir2-rs
   $env:TICKFRAME_SNAPSHOT_OUT = "E:\UserTemp\tickframe-slice1\out.ndjson"
   cargo test -p game-svr --lib -- tickframe_slice1_snapshot
   ```

   未设 `TICKFRAME_SNAPSHOT_OUT` 时该测试直接返回（什么都不做）。

## 判据

- `castle_fanout` / `castle_refshow` / `eat_stdmode1` / `bag_takeoff`：与 `after.ndjson` **逐字节相同**；
- `t290_chain`：只允许 902/903/905 三个 socket 的 657 顺序按「每收件人自己的入队序」不同
  （帧数每 socket 相同）；见卡 §15b。

## 场景覆盖

| 场景 | 覆盖的接线点 |
|---|---|
| `castle_fanout` | `castle.rs::fan_out_sysmessage_to_all`（252；两条网关线**共用 socket=901**，验证 `(gate_idx, socket)` 键） |
| `castle_refshow` | `castle.rs::refresh_show_name_in_area`（165） |
| `eat_stdmode1` | `handle_eat_cmd`（635/622/657；观察者 +5/+11/+12 各 1 条、+13 零帧） |
| `bag_takeoff` | `handle_bag_cmd_with_magic`（200→615/620→752→52→657） |
| `t290_chain` | `drain_kill_exp_chain_with_magic`（3 人同 tick 升级 + 子集观察者） |

## 已知边界

- 装置是快照时的源码（对应交付 head `ef32a7e9e`）；若宿主 API 后续变动，需要跟着改。
- 它只证「帧面逐字节」；收件人集合/帧数由同一份 NDJSON 的 `sock` 列一并可见。
