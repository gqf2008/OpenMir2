# 卡 `openmir2-itemnumber-persist-writeback` 判据：oracle 不写回，卡前提不成立

- 车道：implementation（worktree `/e/tmp/impl-persist`，分支 `persist`，base `b192bd9f3`）
- 日期：2026-10-17
- 结论：**不实现写回**；本卡判 **blocked / 前提不成立**（不是「实现太大」的超范围问题，是**前提本身为假**）。

## 0. TL;DR

卡的前提是「oracle 每 60s 把 `ItemNumber` 累积写回 `Server.conf`，Rust 不写 ⇒ 跨重启分叉」。逐行核对
oracle（`b192bd9f3`）后该前提**不成立**：

1. 60s 定时器**确实**调 `SaveItemNumber()` → `ServerConf.SaveVariable()`；
2. 但 `SaveVariable()` 只对 10 个键调 `ConfigFile.WriteInteger(...)`，那只是**内存缓存**（`iniCahce`）赋值；
3. 真正把缓存落盘的 `ConfigFile.Save()`（`FileMode.Truncate` 重写整个文件）是 `protected`，**没有任何调用点落在 `ServerConf` 实例上**（注：`ConfigFile` 的**其它子类**确有调用点——GameGate/LoginGate 的 `Conf/ConfigManager.cs` 与 `M2Server/Castle/CastleConf.cs:318` 都在自己的写回路径里调继承来的 `Save()`；`CloudGate/Conf/ConfigManager.cs` 的基类其实是 `IniFile`、不是 `ConfigFile`。与本判定相关的只是：`ServerConf` 自身无 `Save()` 定义、`ServerConf.` 全仓仅 2 处引用且均**不落盘**。**独立复验 2026-10-17 订正了原措辞，结论不变**）
   （历史全分支亦然）⇒ **oracle 从不把 `ItemNumber`/`ItemNumberEx` 写回 `Server.conf`**。

⇒ 两侧跨重启行为**本来就一致**（都只读 conf 静态值，再各加屏障 A 的 1 抽）。若按卡的字面去实现写回，
反而会**破坏**与 oracle 的逐字段等价（违反 AGENTS.md 硬约束 3「行为等价优先于修 bug」——这里的「等价物」
就是「不落盘」）。

**交付**：本判据件 + `mir2-rs/bins/game-svr/src/config.rs` 的 `item_number` 就地登记；**不改任何功能代码**。

## 1. oracle 三处逐条对照（任务第 1 项）

### 1.1 60s 定时器（周期 / 启动时机 / 触发条件）

`src/GameSrv/TimedService.cs`（`GameSrv` 的 `BackgroundService`）：

- 周期：`PeriodicTimer` **1s** 一跳（`:23`）；每跳 `ExecuteInternal()`（`:43`）。
- 触发条件：`if (!M2Share.StartReady) return;`（`:61-64`）——服务**未就绪就不跑**。
- 保存分支：`(currentTick - SaveIntervalTime) > 60 * 1000` ⇒ `SaveItemNumber(); ProcessGameNotice();`（`:73-78`）；
  同一方法内 `SaveIntervalTime = GetTickCount()`（`:75`，**在 `:76` 调用之前**）⇒ 周期 = **60s**。（独立复验订正排序笔误，结论不变）
- 启动时机：`StartAsync` 把 `SaveIntervalTime = GetTickCount()`（`:30`）⇒ 首次触发在**启动后 60s**，非立刻。
- `SaveItemNumber()` 本体：`SystemShare.ServerConf.SaveVariable();`（`:255-258`）——空壳，无后续落盘。

### 1.2 `SaveVariable()` 的实现（`src/Modules/SystemModule/Conf/ServerConf.cs:668-680`）

```csharp
public void SaveVariable()
{
    WriteInteger("Setup", "ItemNumber", SystemShare.Config.ItemNumber);
    WriteInteger("Setup", "ItemNumberEx", SystemShare.Config.ItemNumberEx);
    WriteInteger("Setup", "WinLotteryCount", …);
    … WinLotteryLevel1..6 …
}   // ← 到此为止：没有 Save() / 没有文件 IO
```

`WriteInteger` 来自基类 `ConfigFile`（`src/OpenMir2/Common/ConfigFile.cs:419-434`）：

```csharp
protected void WriteInteger(string section, string key, object val)
{
    if (val == null) return;
    if (!CheckSectionExists(section)) iniCahce.Add(section, new Dictionary<string, string>());
    if (!string.IsNullOrEmpty(key))
    {
        Dictionary<string, string> secTbl = iniCahce[section];
        secTbl[key] = val.ToString();   // ← 纯内存 HashSet/Dictionary 赋值
    }
}
```

**这是内存缓存的键赋值，不碰磁盘。**

### 1.3 `ConfigFile.WriteInteger` 的「文件形态」——由 `Save()` 决定（`ConfigFile.cs:348-381`）

写文件只发生在 `Save()`（`protected`）：

- `File.Open(FileName, FileMode.Truncate, …)` **原地截断重写**；GB2312 编码（`:364`）；
- 遍历 `iniCahce`：逐节写 `[节名]`、逐键写 `键=值`，每节尾补一个空行（`:365-374`）；
- 只读文件临时解除只读后再恢复（`:355-363/377-380`）；
- **注意**：注释（`;` / `/* */`）在 `Load()` 时**不进入** `iniCahce`，所以 `Save()` 重写会**丢注释**；
  只有进过缓存的节/键会被写回（重复节被 `goto Label_02AE` 截断，`ConfigFile.cs:302-306`）。

⇒ 若真有写回：形态是「整文件重写为 `[节]`+`键=值`+空行、GB2312、丢注释、非原子」。

## 2. 为什么「没有写回」——决定性证据（可复跑）

```sh
# (a) ServerConf 里没有任何 void Save( ，SaveVariable 体里没有 Save()
grep -n "void Save(" src/Modules/SystemModule/Conf/ServerConf.cs
#   → （无输出）

# (b) 全仓 .Save(); 的调用点：只有网关配置与城池，没有 ServerConf
grep -rn "Save();" src/ --include=*.cs
#   → CloudGate/Conf/ConfigManager.cs:140 / GameGate/Conf/ConfigManager.cs:174 /
#     GameSrv/Npc/CastleOfficial.cs:145 / GameSrv/Word/Threads/SystemProcessor.cs:61 /
#     LoginGate/Conf/ConfigManager.cs:90 / M2Server/Castle/CastleConf.cs:318 /
#     M2Server/Castle/CastleManager.cs:77,150 / Castles/IUserCastle.cs:150 / SubSystem/ICastleSystem.cs:22
#     （ConfigFile.Save 的调用者 = GameGate/LoginGate/CloudGate 的 ConfigManager.SaveConfig() 与 CastleConf；
#      都不是 ServerConf）

# (c) ServerConf 的全部外部用法只有两处：装载与「伪保存」
grep -rn "ServerConf\." src/ --include=*.cs
#   → GameShare.cs:90:  SystemShare.ServerConf.LoadConfig();
#   → TimedService.cs:257: SystemShare.ServerConf.SaveVariable();

# (d) 历史全分支：SaveVariable 从未伴随过 Save()
git log --all -p -S 'SaveVariable' -- '*.cs' | grep -c "Save();"
#   → 0
```

> (c) 的 `SaveVariable()` 名字带「Save」，但**没有匹配的落盘调用**——「Save」只是方法名。

## 3. 为什么不走「最小忠实实现」那条路

任务给了两条路：① 实现一个最小忠实写回；② 只出设计件 + 登记 + blocked。选 ②，理由不是「实现会牵出
配置写回框架」这种工程量问题，而是：

- **实现写回 = 主动制造与 oracle 的差异**（oracle 不写）。这与本仓第一判据「逐字段/逐字节等价」直接冲突；
- 卡的「跨重启初值分叉」现象**在本 oracle 上不存在**：两侧都从 conf 静态值起算。Rust 侧现状
  （`config.rs` 只读、`main.rs` 的 `item_number` 计数器只在内存里 `+=`）**已经等价**，无需改动；
- 部署件 `ItemNumber=406897` / `ItemNumberEx=1073741872`（`E:/MirServer/M2GameSvr/Server.conf:281/379`）
  **不是本代码路径产生的累积**：其 mtime `2026-10-09 23:38` **早于**同目录 `GameSrv.dll` 的 `2026-10-10 23:54`，
  且 `ItemNumberEx` 恰为 `int.MaxValue/2 (=1073741823) + 49`——与 `GameApp.cs:76` 的 `int.MaxValue/2` 初值同族，
  更像**手工设定或更早的非本仓构建**留下的值，而非现行 60s 定时器写回的结果。（本项为只读观测，结论不依赖它。）

## 4. 使本判据可证伪的对照（反例）

出现下列任一，本判据作废、需回到「实现写回」：

1. `src/Modules/SystemModule/Conf/ServerConf.cs`（或任一 `ServerConf` 分部/子类）出现 `Save()` / `base.Save()`
   / `SaveConfig()` 调用；
2. `src/OpenMir2/Common/ConfigFile.cs` 的 `WriteInteger` 改成带文件 IO；
3. `grep -rn "ServerConf\." src/` 出现第三处用法且其会落盘；
4. 真机窗口实测（R 级）发现 oracle 运行的 `Server.conf` 在 60s 后被改写（mtime 前进）。

## 5. tier 与窗口

- 本判定本身是 **L 级**（纯代码事实核对：静态 `grep`/`git log` 即可证伪，不需真机）；
- 被它挡下的**实现**才会是 R 级（跨重启持久化落库，AGENTS.md 判据分级③）；
- 因此本卡**不需要真机窗口**——因为没有要落库的实现。**不宣称任何真机对拍结论。**

## 6. 只读核查（本车道未触碰任何冻结面/部署件）

见交付说明（`Server.conf` mtime/hash 未变、主检出 `git status --porcelain` 为空）。
