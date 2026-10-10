# 命令码位移（whitelist B-8）的实证记录

日期：2026-10-10。目的：证明「脚本命令实际派发到前一个枚举成员的处理器」是 C# 参照的真实行为，
而非 Rust 侧移植错误。

## 1. 上游引入点（git 历史）

```
$ git log -S "nCMDCode = code - 1" --oneline -- src/Modules/ScriptEngine/ScriptParsers.cs
8a8a02d1 修复和实现商人接口        # 2023-06-15
```

该提交把两处 `nCMDCode = code;` 改为 `else { nCMDCode = code - 1; }`（条件解析 322 行附近、
动作解析 495 行附近），**未同时改动** ConditionCode/ExecutionCode 枚举或
ConditionProcessingSys/ExecutionProcessingSys 的注册表——即执行侧仍以枚举值为键。

## 2. 码表与注册表（反射 dump）

`handler-maps.txt`：conditionMap 111 条（键=枚举值）、executionMap 133 条（键=枚举值）。

## 3. 解析侧实测（真实 ScriptParsers）

```
[脚本] CHECKLEVEL > 30          → cond CmdCode=6   （枚举值 7；6 = CHECKUNIT）
[脚本] CHECKITEM 祈福项链 1     → cond CmdCode=9   （枚举值 10；9 = CHECKBBCOUNT）
[脚本] GOTO @x                  → act  nCmdCode=54 （枚举值 55；54 = EndQuest）
```

## 4. 执行侧端到端实测（真实 ExecutionProcessingSys.Execute + DispatchProxy 玩家探针）

```
cmd pid=1   p1=金币 p2=5   (脚本 take)      → handler=ActionOfSet        玩家侧调用: SetQuestFlagStatus(0,5)
cmd pid=2   p1=金币 p2=5   (脚本 give)      → 未注册 → ScriptEngine switch 的 ExecutionCode.Take 分支
cmd pid=4   (脚本 close)                    → 未注册 → switch 的 ExecutionCode.Takew 分支
cmd pid=54  p1=@x          (脚本 goto)      → 未注册 → switch 的 ExecutionCode.EndQuest 分支
cmd pid=9   (脚本 break)                    → handler=ActionOfResetUnit  玩家侧调用: SetQuestUnitStatus(0,0)
cmd pid=233 p1=你好        (脚本 messagebox)→ handler=ActionOfMobFireBurn
```

（探针命令：临时 ProjectReference 到 ScriptSystem.csproj 的控制台程序，用 DispatchProxy 实现
INormNpc/IPlayerActor 记录玩家侧调用；未修改任何 C# 参照代码。）

## 5. 现网部署一致性

```
E:\MirServer\M2GameSvr\ScriptSystem.dll   LastWriteTime = 2026-10-09 01:18:07
```

现网 GameSvr 的 ScriptSystem.dll 由本仓库源码构建，即线上运行的就是含位移的版本。

## 结论

B 线按位移后语义 1:1 移植（解析层已完成；求值层按同一语义派发）。
若 owner 后续决定以「位移前语义」（即 8a8a02d1 之前的行为）为目标，则属独立重构批次，
需同时改解析常数与派发表并重跑全部对拍。
