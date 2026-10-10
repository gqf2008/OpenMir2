# M2 世界/AOI 对拍（同地图同实体 → 位置与可见集逐项相同）

对应设计文档 §6 M2 判据②与 §4.2（世界模拟单线程串行）。夹具与金标准生成器在
`tests/parity/csharp/WorldGolden/`，场景在 `world/scenarios/`，金标准在 `world/golden/`。

## 一条命令

```bash
# 1) C# oracle 产出金标准（函数体逐字拷贝；路径自解析，不依赖 cwd）
dotnet run --project mir2-rs/tests/parity/csharp/WorldGolden -c Release -- basic

# 2) Rust 侧对拍（位置 + 可见集，逐行）
cargo test -p mir2-parity-tests --test world_aoi_parity
```

## 顺序契约（两侧必须一致，写死在各自实现里）

1. 实体按场景数组顺序插入 ⇒ **槽位顺序 = AOI 处理顺序**；
2. 每个实体插入时入队一条 `Enter`；
3. 每 tick：`clock += 200` → 追加本 tick 的命令（按场景顺序）→ FIFO 处理队列 → 按插入顺序做 AOI；
4. `Walk`：界内 → 目标格阻挡者（`!Ghost && !Death && !FixedHideMode && !ObMode`）→ `MoveToMovingObject` → 更新坐标；
5. `Leave`：从格子摘除 + 从仓库/ActorMgr 移除；
6. **虚拟时钟**：两侧都不读墙钟（C# 侧把 `HUtil32.GetTickCount()` 桩成由驱动推进的时钟）。

场景 `basic.json` 刻意覆盖：视野边界（dx=5 可见 / dx=6 不可见）、扫描序（X 外/Y 内）、
过滤器（Ghost/Invisible/FixedHideMode/ObMode 各自被排除）、主人规则（master 距离 ≤3 才可见）、
死亡路径（`SearchViewRangeDeath`）、阻挡反直觉项（Ghost/ObMode/FixedHideMode **不阻挡**，
Invisible **阻挡**）、离开世界。

## 与 C# 对齐时确认/踩到的两件事

1. **C# 的 AOI 不排除观察者自己**：`BaseObject.ViewRange.cs:108-116` 的过滤器没有
   `baseObject == this` 判断，自己格里的自己对象会被 `UpdateVisibleGay` 加进 `VisibleActors`
   （金标准第 1 tick 的 `ent 1 30 30 vis 1:2 …` 就是自证）。**照抄不修**（行为等价优先），
   由金标准钉住。
2. **桩的错误会伪装成"两侧不一致"**：第一次对拍差异是 `11:2` 只出现在 Rust 侧——
   根因是我在 C# 桩里**漏了给 `Master` 字段赋值**（Rust 侧主人规则其实是对的）。
   教训：对拍红了先查夹具自身（这次是夹具漏字段），别直接改被测实现。
