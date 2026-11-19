# openmir2-own-binary-without-stamp

> 状态：**已登记，10k 窗口之后再做**（涉及重构建/补戳或调整部署集，会改变冻结件）。
> 来源：卡 `openmir2-deps-check-deploy-dir` 复审（opc-07，2026-11-19）。

## 现象

`E:/UserFleet/m1-stack/runtime/DBSrv/` 里有 `DBSrv.Storage.MongoDB.dll`（我们自己的插件产物），
但它：
- **不在** `release-1af56267` 冻结的 8 件里；
- **没有** 侧车戳（`DBSrv.Storage.MongoDB.dll.build.json` 不存在），
  而同目录的 `DBSrv.Storage.MySQL.dll` 有戳。

来源：`Directory.Build.targets` 的 `CopyStoragePluginToDbSrv`（AfterTargets=Build）把
`DBSvr.Storage.*` 的插件 DLL 拷进 `src/DBSrv/bin/<Configuration>/`——MongoDB 插件也被拷进去了，
但部署/冻结清单只覆盖了 MySQL 插件。

## 风险

- "部署了但无身份"：无法回答"这份 MongoDB 插件来自哪个提交"，与 `stamp-binaries` 的口径不一致；
- 若该插件当前不被使用，却仍在部署集里，会增加无谓的攻击面/维护面。

## 选项（10k 之后定）

1. **重构建 + 补戳**：从当前 master 构建 `src/Storeages/DBSvr.Storage.MongoDB` 并产
   `DBSrv.Storage.MongoDB.dll.build.json`，与 MySQL 插件同规格纳入冻结清单；
2. **从部署集移除**：确认运行时不加载 MongoDB 插件（配置未启用）后，把它从
   `runtime/DBSrv` 与部署脚本里去掉，并在 `Directory.Build.targets` 的拷贝范围里排除；
3. 若保留但暂不补戳：必须在部署 `ANNOTATION` 里显式登记"该件无身份"，不让它变成默认遗忘。

## 边界

- 10k 窗口的两组冻结件不能动；本卡的所有选项都在窗口之后执行。
- 不改任何二进制行为。
