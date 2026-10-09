# 对拍差异白名单（tests/parity/whitelist.md）

判据（设计文档 §6.0-3）：**差异要么 0，要么进本文件；没登记 = 不过。**

每条登记格式：

```
## <差异点标识>
- 现象：<哪条包/哪个字段，两侧各是什么>
- 为什么无害：<行为等价的论证>
- 触发条件：<什么配置/时序下出现>
- 登记人 / 审查人：<谁审的>
- 登记日期：<YYYY-MM-DD>
```

当前登记数：**0**。

<!-- 已知待观察项（不是白名单，是 C# 侧既有口径差，M1/M2 移植时逐条处置）：
1. GameGate SM_EAT_FAIL 限频分支按 CommandFixedLength=16 编码 12 字节头（C# 越界风险路径，
   仅在 IsEatInterval 开启且超速吃粮时触发）；
2. GameGate 聊天过滤命令分支 Encode 的 dstOffset=0 会覆盖帧首 '#'（Config 开启时才触发）；
3. LoginGate.SendDefMessage 带 sMsg 时 Array.Copy 目标偏移 13 > 缓冲 12+len（必然抛异常的路径，
   说明该分支从未被真实触发）。
-->
