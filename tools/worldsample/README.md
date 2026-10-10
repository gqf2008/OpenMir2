# tools/worldsample/ —— 世界态采样器（C5）

把一份客户端抓包（`GoldenExport` 的 JSONL）还原成**可对拍的世界态时间线**，
补上 M0 之外缺的那层观测：不只对字节/字段，还能对"世界态"。

```powershell
# 采样（输出 NDJSON：event 流 + tick 键的快照流）
python tools/worldsample/world_sampler.py sample `
  --capture mir2-rs/tests/golden/c-line-baseline-c3-20261010.jsonl `
  --out tests/golden/session-20261010-c3-hook/world.ndjson

# 对拍两条时间线（C# 基线 vs Rust 候选）——M2 的"N tick 后位置/可见集相同"用它自动判
python tools/worldsample/world_sampler.py compare baseline.ndjson candidate.ndjson

# 自测（确定性 + 改坏必红）
python tools/worldsample/world_sampler.py selftest
```

## 可比字段与其来源（都锚在 C# 发送点，不从字节反推）

| 字段 | 来源 | 说明 |
| --- | --- | --- |
| `tick` | 明文动作帧 `#+GD/<rtime>!` | 协议层 `frame::parse_act_frame`；发送模板 `src/Modules/SystemModule/MessageSettings.cs:6` |
| `self_srv` | `SM_NEWMAP(51)` / `SM_LOGON(50)` 的 `Param=X, Tag=Y`, `Series=LoByte(Dir)` | **服务端权威**的"自己"位置 —— 只在进图/纠偏时下发 |
| `aoi` | `SM_TURN(10)`/`SM_WALK(11)`/`SM_RUN(13)`/`SM_HIT(14)`/`SM_STRUCK(31)`/`SM_FEATURECHANGED(41)` 把 actor 加入；`SM_DISAPPEAR(30)` 移出 | 头字段语义见 `src/M2Server/Player/PlayObject.Message.cs:1406`(SM_WALK)/`:1542`(SM_TURN)：`Recog=actorId, Param=X, Tag=Y, Series=MakeWord(Dir,Light)` |
| `bag` | `SM_ADDITEM(200)`/`SM_DELITEM(202)`/`SM_UPDATEITEM(203)` 计数 | 背包变化次数 |
| `gold_changed` / `hpmp_changed` | `SM_GOLDCHANGED(653)` / `SM_HEALTHSPELLCHANGED(53)` | 变化次数（值本身在 body，未解） |
| `move_inputs` | 上行 `CM_TURN(3010)`/`CM_WALK(3011)`/`CM_RUN(3013)` 计数 | 客户端**输入意图** |

## 两条实测得到的协议事实（别当成 bug，写进来免得重踩）

1. **玩家自己的逐步位置在本协议里不回显**：C3 抓包 382 帧里，`SM_TURN/SM_WALK/SM_RUN` 带的
   `Recog` 全是**其它** actor（NPC/怪物）的 id；自己的坐标只在 `SM_NEWMAP/SM_LOGON`（进图）出现一次。
   实证：进图时 `self_srv=(287,618)`，而 10 秒后客户端截图上的「坐标」读数是 `339,627`
   —— 差距由**客户端本地预测**造成，服务端没发纠正。
   ⇒ M2 判"N tick 后位置相同"时，**可比的"世界态"是 AOI（其它 actor 的位置/可见集）+ 背包/金币/tick**；
   要比"自己的位置"，得另找信号（读客户端内存 / 识别屏幕上「坐标」读数 / 让服务端在纠偏时也回一步）。
2. **上行的移动帧里没有坐标**：`CM_TURN/CM_WALK/CM_RUN` 实测 `Param=0`、`Tag=方向样值`、
   `Recog` 是 65536 递增的时间戳样值 ⇒ 只能当"输入计数/方向"，不能当"自己的位置"。

## 判据（C5 卡）

- **同序列两次采样逐字节相同** ✓（`selftest` 第 1 项；工具无时间戳/无随机，输出确定）。
- **改坏必红** ✓（`selftest` 第 3 项：把"权威自身位置"那一帧的 X 改一格，`compare` 立刻报 RED）。
- **能自动判 M2 的"N tick 后位置/可见集相同"** ✓：`compare` 按 tick 键的快照逐点比
  `self_srv / aoi / bag / gold / hpmp / move_inputs`，差异点直接打印（含两边的值），有差异即 exit 1。
