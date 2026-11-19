# openmir2-deps-check-deploy-dir

> 状态：**实现完成，待 opc-07 复核三条牙后合并（2026-11-19，分支 `card/deps-check-deploy-dir`）**。
> 上游卡：`openmir2-deploy-deps-manifest`（其 `check` 只核 NuGet 缓存，不核部署目录）。
> 触发：botfleet 实测把 `runtime/DBSrv/MemoryPack.Core.dll` 翻一个字节后 `check` 仍
> `DEPS_GREEN 45/1010 rc=0`——"部署出去的那个文件被换了"当时不在闸门内。

## 改动

`tools/deploy/deps_manifest.py`：

- `generate` 新增 `--output-dir`（缺省自动探测本项目最新的 `bin/<config>[/<tfm>]`，兼容
  `AppendTargetFrameworkToOutputPath=false` 的布局），按构建输出记录每包 **`shipped_assets`**
  ——即"实际装运"的运行时资产。
  **为什么不能只用 assets 的 runtime 列表**：`System.Text.Json`/`System.Text.Encoding.CodePages`
  等在 net8.0 共享框架里，assets 仍列为运行时资产但**不会复制到输出目录**；`_._` 是占位符。
  若按 assets 直接要求，真实部署会误报 5 条 `MISSING_DEPLOYED`（实测）。
- `check` 新增 `--deploy-dir`：按 **basename** 把 `shipped_assets` 映射到 `<deploy-dir>/<name>`：
  - `MISSING_DEPLOYED <pkg>/<ver> <file>` ⇒ 红；
  - `DEPLOYED_SHA_MISMATCH <pkg>/<ver> <file> expected=… actual=…` ⇒ 红；
  - `AMBIGUOUS_MAPPING <file> candidates=<pkg1@v1,pkg2@v2>`（不同包、同名、sha 不同）⇒ 红；
  - 部署目录里多余的 `.dll` ⇒ `WARN_EXTRA <file>`（自家件由 `stamp-binaries` 管，不红）；
  - `--deploy-dir` 缺失/为空 ⇒ 红；manifest 无 `shipped_assets` ⇒ `MISSING_SHIP_SET` 红（fail-closed）。
- manifest 版本仍为 1；每包新增必填字段 `runtime_assets`/`shipped_assets`（旧 manifest 会 fail-closed 红）。

## 三条牙（原文，离线合成夹具）

```
① 翻部署件末字节：
DEPS_RED DEPLOYED_SHA_MISMATCH Alpha/1.0.0 Alpha.dll expected=239f59ed55e737c77147cf55ad0c1b030b6d7ee748a7426952f9b852d5a935e5 actual=98087f841378d59278942c91da4545479d0e308fe5ef461a0b8a858aac84697b
② 删一个部署件：
DEPS_RED MISSING_DEPLOYED Beta/2.0.0 Beta.dll
③ 两包同名不同 sha：
DEPS_RED AMBIGUOUS_MAPPING Shared.dll candidates=Alpha@1.0.0,Beta@2.0.0
```
另有：多余 DLL 只 `WARN_EXTRA OurOwn.dll`（不红）；缺/空部署目录 ⇒ `MISSING_DEPLOY_DIR` /
`EMPTY_DEPLOY_DIR` 红。自测：`deps_manifest selftest: GREEN (16 cases)`。

## 真实部署目录（只读）实测与一个覆盖缺口

`generate src/DBSrv` + `check --deploy-dir E:/UserFleet/m1-stack/runtime/DBSrv`：

```
DEPS_GREEN packages=45 files=1010
WARN_EXTRA DBSrv.Storage.MongoDB.dll / DBSrv.Storage.MySQL.dll / DBSrv.Storage.dll / DBSrv.dll
WARN_EXTRA MySqlConnector.dll / OpenMir2.dll
```

- 自家件按约定只 WARN（由 `stamp-binaries` 管）。
- **`MySqlConnector.dll` 是第三方件却落在 WARN**：`src/DBSrv/DBSrv.csproj` 只引用
  `OpenMir2` 与 `DBSrv.Storage`，其 lock/assets **不含** MySqlConnector ⇒ 该运行时目录不是由
  当前 `src/DBSrv` 的还原图产出的（或部署脚本从别处拷）。**这条要么改部署来源、要么把
  MySQL 存储项目的 manifest 一并纳入**——需要 botfleet/opc-07 确认"谁产出这个目录"。在确认前，
  它按约定只 WARN，不阻断。

## 边界

- 仍只做 NuGet；`runtime` 里自家件继续由 `stamp-binaries` 管，两边不重叠。
- 不改任何二进制行为。
