# openmir2-nlog-using-cleanup

> 状态：**实现完成（2026-11-19，分支 `card/m7-small-cards`，与 `openmir2-own-binary-without-stamp`
> 一并合并为 M7）**。来源：卡 `openmir2-deps-check-deploy-dir` 复审（opc-07）：NLog 当时走
> 「显式例外」（决策 b），并明确「10k 之后再清 using」。10k 窗口已结束，本卡做清理。

## 本卡要解决的问题

`DBSvr.Storage.MySQL` 声明了 `NLog`（包 + 两条 `using NLog;`），但**没有任何 NLog API 调用**，
实际日志全部走仓库自己的 `LogService` 门面。因此：

- 依赖图里多一个**运行时不需要**的第三方件；
- 部署目录没有 `NLog.dll`（拷贝目标只拷插件自身 DLL，不带依赖闭包），于是 manifest 要靠
  `deps.exceptions.json` 一条例外把 `MISSING_DEPLOYED` 压成绿——「例外」本身就是债；
- 例外条目一旦不再命中（例如依赖被清理），工具会打印 `DEPS_EXCEPTION_UNUSED`，同样算债。

## 改动（5 个文件）

| 文件 | 改动 |
|---|---|
| `src/Storeages/DBSvr.Storage.MySQL/DBContext.cs` | 删第 2 行 `using NLog;` |
| `src/Storeages/DBSvr.Storage.MySQL/PlayRecordStorage.cs` | 删第 4 行 `using NLog;` |
| `src/Storeages/DBSvr.Storage.MySQL/DBSvr.Storage.MySQL.csproj` | 删 `PackageReference Include="NLog" Version="5.2.8"` |
| `src/Storeages/DBSvr.Storage.MySQL/packages.lock.json` | `dotnet restore --force-evaluate` 更新（NLog 条目 -6 行） |
| `src/DBSrv/deps.exceptions.json` | 例外条目整条删除，内容变为 `[]` |

## 为什么安全（行为等价）

- 两条 `using` 之外**没有任何 NLog 符号**：删除后 `dotnet build -c Release` 0 错误
  即编译级证明（若存在隐藏使用点会直接报 `CS0103`/`CS0246`）。
- 日志路径未动：`LogService.Error(...)` 等调用原样保留。
- 不改任何运行时判定、协议、配置。

## 证据（分支 head 上实跑）

```
# 1) 项目内已无 NLog 引用
grep -rn NLog src/Storeages/DBSvr.Storage.MySQL --include=*.cs --include=*.csproj   -> 无输出（rc=1）

# 2) 重新生成 manifest：NLog 消失、例外为空
DEPS_WRITE .../DBSrv.deps.json packages=46 files=1030
  exceptions = []     （旧：1 条 NLog）
  packages   = 46     （旧：47）

# 3) 对现网部署目录核验：绿，且**没有** DEPS_EXCEPTION 行
DEPS_GREEN packages=46 files=1030      (rc=0)

# 4) 负对照（证明「无 DEPS_EXCEPTION 行」不是空过）：换回旧 manifest（含 NLog 例外）
DEPS_EXCEPTION NLog/5.2.8 declared by DBSvr.Storage.MySQL, zero API usage (only using-directives), not on any runtime path
DEPS_GREEN packages=47 files=1045      (rc=0)
```

- 插件 DLL 字节确实变了：旧 `9be1af2d...`（64512B）→ 新 `82ceb8d4...`（64512B），
  重新产侧车戳在 M7 上做（与 ① 同一批）。
- 本卡使**依赖图与实际部署一致**：部署集里不再有「声明了但从不部署」的第三方包，
  也不再需要任何例外条目。

## 边界与残留

- 不动 `DBSrv.Storage.MongoDB` 插件：它的 csproj 里也有一条 `PackageReference NLog`，但同样
  零 API 使用，且该插件不在 manifest 工程集里（`SELF_BUILT` 名单），不产生例外。
  **残留项**：同一把「死声明」还留在 MongoDB 插件里，建议随下一次触碰该插件时一并清掉
  （本卡不扩大写集）。
- 10k 冻结件（`release-ctrl-96422fd5` / `release-exp-c3d3c271`）不动；部署副本在 M7 上刷新。
