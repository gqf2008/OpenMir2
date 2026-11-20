# openmir2-runtime-sysmsg-text-frames

> 状态：**登记卡（2026-11-20，opc-07 发布）**。触发：IT-14（修复油）设计时确认——夹具记录面能"对拍通过"，但**运行期宿主不会按 oracle 发出带文本的系统消息帧**，玩家看到的提示缺失。
> 关联：`docs/issues/openmir2-items-parity.md`（IT-14/IT-15）、C2 之前的 `StdMode1` 食物同类缺口

## 1. 现状（事实，带锚）

- oracle：`SysMsg(<文本>, Green, Hint)` → `SendMsg(RM_SYSMESSAGE, 0, GreenF, GreenB, 0, "<文本>")`（队列通知）。
- 宿主：`QueuedNotice` 是**5 字段定长**（ident + 4 个数字参数），`notice_frame` 的 `RM_SYSMESSAGE` 走 `session.sys_msg`（**静态文本**）。
- ⇒ 夹具层面：`notices` 只记 `ident + 颜色参数`，**不含文本** ⇒ **对拍能通过**；运行期层面：**这句话不会按 oracle 发**，玩家看不到。
- 已确认同类：**StdMode1 食物** 的 `RM_SYSMESSAGE` / `RM_CHARSTATUSCHANGED`、**Shape9 修复油** 的 `WeaponRepairSuccess` 绿字。

## 2. 目标

让"带文本的系统消息帧"在**运行期**与 oracle 一致：送达同一收件人、带同一文本与颜色，且**进了比较面**（不能只有夹具层面通过）。

## 3. 可选路径（需评审取舍）

1. **扩 `QueuedNotice` 携带字符串**：破"5 字段定长"不变量，牵动 `pickwire` 字面量与多处构造点——影响面大但语义直接；
2. **旁路表**：`QueuedNotice` 只带 `ident + 消息 ID`，文本由宿主按 ID 从配置/常量表还原——不改定长结构，但要求 oracle 文本与宿主表逐字一致（需落表并机检）；
3. **只登记不实现**：明确"运行期缺文本帧"，覆盖率口径中该条**不计入 verified**（宁少不多）。

## 4. 验收

- 任一方案落地后，**至少一条"运行期帧"证据**：同一场景下宿主实际下行帧含 oracle 的同文本与颜色（不是只看夹具 notices）；
- 若选路径 3，则把受影响的条目从 verified 降级并在报告里给数字变化（**代价接受**）。

## 5. 不做

- 不在比较器里"忽略文本"当作已覆盖；
- 不把"夹具对拍通过"表述成"运行期一致"。
