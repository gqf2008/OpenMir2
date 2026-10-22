# 协议对拍台：首次双栈真机对拍（2026-10-23）

- 目的：用**无头协议**替代 GUI 窗口取证 —— 同一串 `CM_*` 打进 C# 真值栈与 Rust 栈，比 s2c 帧 + 比数据库后态。
- 工具：`tools/probe/mir2_probe.py`（驱动器）+ `tools/conformance/compare_runs.py`（比较器），
  均经独立复核 approve（selftest 11+6 / 44 项）。
- 场景：`tools/probe/scenarios/login_only.json`（登录→选服→选角→进世界→等 15 帧后断开）。
  **round2 起改为 `wait_idents` 等 `50/52/658/654/657/100` 全部到齐（另有 `--dwell-ms` 默认 1200ms 兜底）**，
  见卡 `openmir2-probe-dwell-after-enterworld`：原来的固定帧数会在延迟段帧到达前小退。

## 1. 环境（两栈串行、**同一组端口**）

| 角色 | 端口 |
| --- | --- |
| DBSrv | 16000 / 15100 / 15700 |
| LoginSrv | 15500 / 15600 |
| GameSvr | 15000 |
| LoginGate | 17000 |
| SelGate | 17100 |
| GameGate(RunGate) | 17200 |

- **C# 真值栈**：私有 root `E:/tmp/conformance/csharp-root`（部署件副本 + 仓库构建的 DBSrv/LoginSrv），
  配置逐项重映射到上表端口、连接串指向 `mir2_test_*`（**绝不碰生产库**）。
  为使其能与 peer 的默认端口栈并存，`src/SelGate` 的 `GatePort` 已改为可配置（whitelist **T-172**，缺省 7100 不变）。
- **Rust 栈**：`E:/tmp/conformance/rust-root`（r5 覆盖层副本），同一组端口、同一 `mir2_test_*`。
- 两侧跑前都从**同一份基线**恢复（`mysqldump` 快照 → drop/create → 回灌）。

## 2. 结果：19 帧逐字节对齐，只剩 2 条差异 + 8 条规则缺口

```
compared=19  oracle_total=29  rust_total=19
diffs=2
  [0] ident=529 body_len  C#=19B  Rust=15B
  [19] frame_count         C# 多出前几个 ident: [10, 41, 654, 657, 657, 4…]
UNDECIDABLE(8): ident 100/520/525/529/530/654/657 无规则；52 的 body 长度对 (54,78) 不在已知布局表 [40,58]
db: state=compared（13 表逐行逐列已比，未报差异）
```

### 2.1 差异 1：`529 SM_PASSOK_SELECTSERVER` 的第二段不同

| 侧 | body（解码后） | 长度 |
| --- | --- | --- |
| C# oracle | `热血传奇/Idle/` | 19 B |
| Rust | `热血传奇/0/` | 15 B |

⇒ 服务器状态段 C# 送的是字面 **`Idle`**，Rust 送的是数字 `0`。**这是真差异，需按 oracle 修正**（下一步立卡）。

### 2.2 差异 2：进入世界阶段 C# 多出 4 类帧

| ident | 含义 | C# | Rust |
| --- | --- | --- | --- |
| 10 | `SM_TURN` | 有 | 无 |
| 100 | `SM_SYSMESSAGE` | 有 | 无 |
| 654 | 待确认 | 有 | 无 |
| 657 | `SM_CHARSTATUSCHANGED` | 有（×2） | 无 |

`530/525` 两侧**逐字节相同**（`127.0.0.1/17100/2`、`127.0.0.1/17200`），证明同端口方案消除了地址串噪声。

### 2.3 `52 SM_ABILITY` 布局对 (54,78)

两侧都不在既有白名单 `[40,58]`（C# 54 / Rust 78）。此前的 40/58 结论来自另一场景；
说明该帧长度随**客户端版本/ability 位集**变化，规则表需要按场景登记真实长度对，而不是写死 40/58。

## 3. 结论与下一步

- **对拍台可用**：同端口 + 同库基线 + 逐字节帧比对，能自动定位到「第几帧、哪个 ident、第几字节」。
- 待办（按优先级）：
  1. 立卡修 `529` 状态段（`Idle` vs `0`）；
  2. 规则表补齐 `100/520/525/529/530/654/657`，并为 `52` 增加 (54,78) 布局（附 reason/tier）；
  3. 用同一套流程把 `gm_time` / `dice_chain` 也纳入，逐步替代剩余 GUI 窗口卡；
  4. 把「同端口串行 + 基线重灌」固化成一条脚本（`tools/conformance/run_dual.sh`）。
