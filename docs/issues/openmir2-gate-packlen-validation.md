# openmir2-gate-packlen-validation

> 状态：**已登记，未建卡**（本仓库 remote 无 `refs/collab/*`，walgit 看板不可用；按 AGENTS.md §7
> 「tracker 未就绪时先落 `docs/issues/`」过渡）。看板恢复后迁移为正式卡。
> 发现于卡 `openmir2-db-write-stall` 的 44 处 `MemoryCopy.BlockCopy` 逐条审计（E 类）。

## 现象
C# `GameGate` 的会话消息路径里，`packetLen` 直接取自**网络包头**（`packetHeader.PackLength` 的绝对值），
随后作为 `MemoryCopy.BlockCopy(data, 0, sendMsg.Buffer, 0, packetLen)` 的 count。
源 `data` 的长度没有任何显式校验；`MemoryCopy` 内部是 `Buffer.MemoryCopy`（零边界检查）。

## 风险
- 修复前：若 `packetLen` 大于 `data` 实际长度 ⇒ **越界读**；大值时跨出 GC 段 ⇒ 访问违例 ⇒
  CLR 致命错误 `0x80131506` ⇒ 卡在 `NtRaiseHardError` 报告路径（与
  `dbsrv-sendmessage-oob-root-cause` 同机理）。
- 修复后（`MemoryCopy.CheckRange` 上线）：越界变为 `ArgumentOutOfRangeException`，但抛在
  TouchSocket 的收包回调里 ⇒ **该连接停读**。对网关而言，不可信输入不应走到「抛异常」这条路上。

## 建议改动（两侧同卡）
1. C# 侧：拷贝前显式校验 `packetLen >= 0 && packetLen <= data.Length`；不满足 ⇒ 记 `WRN`（含包头字段）
   并**关闭该连接**（拒绝并断开），不进入拷贝。
2. 负例用例：`packetLen < 0`、`packetLen > data.Length`、`packetLen` 极大（如 `int.MaxValue`）各一条。
3. Rust `game-gate` 侧：C# `GameGate` 是该组件的 oracle。若 Rust 已有等价校验，登记为已有；
   若没有，同 PR 对齐；若有意不同，须登记 `D-*` 偏差并说明。
4. 审计同族：`SelGate`/`LoginGate` 的接收路径是否有同类「包头长度当 count」的写法。

## 复现
离线：构造一帧 `PackLength` 大于实际负载的会话消息（不需要真客户端），看 C# 侧行为：
修复前为 AV/卡死，修复后为异常中断接收。

## 关联
- 根因卡：`openmir2-db-write-stall`
- 同批审计：`openmir2-gate-utf8-body-length`（中文正文截断）
