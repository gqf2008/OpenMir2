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


## M2 下半程（最小切片）：会话进世界 + 掉落系统

范围（刻意收窄，见 `M2骨架报告.md`）：**会话进世界**（进图/切图/掉线/小退/再进）+ **一个系统（掉落）**；
战斗结算（`_Attack` 的 RNG 面）不在本切片的 golden 里——击杀由脚本化 `kill` 触发，
因此随机流只由"掉落预算"消耗，先把逐行一致钉死。

```bash
# 金标准（两个场景）
dotnet run --project mir2-rs/tests/parity/csharp/WorldGolden -c Release -- basic
dotnet run --project mir2-rs/tests/parity/csharp/WorldGolden -c Release -- session
# Rust 对拍
cargo test -p mir2-parity-tests --test world_aoi_parity --test world_session_parity
```

场景 `session.json` 覆盖：进图 → 移动（含被怪物挡住的一步）→ 怪物生成（掉落预算在此消耗 RNG）
→ 脚本化击杀（掉落落地 + 金币）→ 切图 → 小退 → 再进（新 `ActorId`）。

**统一输出格式**（两个场景一致，两侧逐行比对）：

```
tick <t> clock <ms>
ent <id> <map> <x> <y> <hp> <level> <exp> vis <目标id>:<flag> ...
death <id> <落地件数> <金币> <击杀者|-1> <获得经验>     # 仅 extended 场景
floor <map> <x> <y> <obj_id>                          # 仅 extended 场景
```

对齐过程中修掉的一处真差异：**切图后"再进"必须用会话当前地图**——Rust 测试驱动最初用会话
初始地图（map 0），而 C# 把当前地图记在会话上（切图时更新）⇒ 首个差异就是 `ent 3 0 …` vs `ent 3 1 …`。
两侧驱动统一为"会话当前地图"后逐行一致（`session.txt`）。
