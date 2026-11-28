# MG-12 墙钟相位立卡（②档，openmir2-a0 分诊批 2026-11-28）

## 缺口
`SKILL_HEALLING`（MG-12）恢复节奏的**真实周期 tick 墙钟相位**未验：现装置在 fixture 给的
**显式虚拟 ms 列表**上直接调生产 `PlayObject.Run()` / `recovery_tick`（tick_hp_* /
tick_lag_rearm / tick_gate_edge_20 三牙已验：重挂公式 `IncHealthSpellTick = now +
min(200, now-old-dw)` 与 600−min(level*10,400) 门、PerHealing=5 消费、清零不重进——
oracle `PlayObject.Message.cs:113-130`，重挂写点 `:76`/`:141`）。

## 需要的机制面
**真实定时器 × tick 网格对齐**的验证面：不注入虚拟 ms，改由真实 `GetTickCount` 语义
（或等价的真实时钟源注入）驱动周期恢复段，验证施法时刻与宿主 tick 网格的相位组合
（相位差 0/1/半格）下 due/重挂/消费序与 oracle 一致。

## 判据草案
1. golden（C# 侧同真实时钟口径）与 Rust 逐 tick `TargetHp` 轨迹 + `IncHealthSpellTick`
   写点时刻对拍；
2. 相位差扫描（0/+1/+half-grid）三支；
3. 牙：重挂公式常数或门常数变异 ⇒ 红。

## 撤销条件
若装置裁定「虚拟 ms 列表 + 公式断言」已等价覆盖相位面（需总监确认），本卡关闭并回写
MG-12 scope。
