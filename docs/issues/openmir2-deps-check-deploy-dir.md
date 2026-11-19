# openmir2-deps-check-deploy-dir

> 状态：**已合并（`cfc154b92`）**。
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

## 复审第 2 轮（opc-07 要求）：来源查清、范围补齐、判据升级

### 未声明来源（原文）

`Directory.Build.targets:8-18`：

```xml
  <Target Name="CopyStoragePluginToDbSrv"
          AfterTargets="Build"
          Condition="$(MSBuildProjectName.StartsWith('DBSvr.Storage.'))">
    <ItemGroup>
      <StoragePluginFile Include="$(OutputPath)$(AssemblyName).dll" />
    </ItemGroup>
    <Copy SourceFiles="@(StoragePluginFile)"
          DestinationFolder="$(MSBuildThisFileDirectory)src\DBSrv\bin\$(Configuration)\"
          SkipUnchangedFiles="true"
          ContinueOnError="true" />
  </Target>
```

即：编 `DBSvr.Storage.*` 时把**插件 DLL** 拷进 `src/DBSrv/bin/<Configuration>/`——这就是
`DBSrv.Storage.MySQL.dll`/`MySqlConnector.dll` 出现在 DBSrv 输出里的来源。注意它**只拷插件自身 DLL，
不拷其依赖闭包**（见下"剩余红"）。

### 范围补齐与判据升级

- `generate`/`check` 支持 `--projects "a;b;c"`（并集 manifest）；部署集由**主项目（第一个）**的还原图
  决定，后续项目只"补缺"（典型：插件的 Direct 包如 `MySqlConnector`）；同名不同版本保留主项目版本。
- manifest 顶层新增：`projects`（绝对路径）、`build_config`（Debug/Release）、`output_dirs`（每项目
  绝对输出目录）；`check` 时若探测到的输出目录与记录不一致 ⇒ `BUILD_OUTPUT_MISMATCH` 红；
  `--projects` 与 manifest 的集合不一致 ⇒ `PROJECT_SET_MISMATCH` 红。
- 部署目录判定升级：`manifest` 未覆盖、且不在自家件白名单（opc-07 给的 7 个 + **`DBSrv.Storage.MongoDB.dll`**）
  的 `.dll` ⇒ **`UNCOVERED_DEPLOYED` 红**（不再是 `WARN_EXTRA`）。
  **白名单偏离说明**：opc-07 的 8 项列表里没有 `DBSrv.Storage.MongoDB.dll`，但它在
  `runtime/DBSrv` 里且是我们自己的插件产物（同样由上面的 target 拷贝），故补进白名单；若不认可请指出。

### 第四颗牙（原文）

```
DEPS_RED UNCOVERED_DEPLOYED Stray.Third.Party.dll
```
另：`BUILD_OUTPUT_MISMATCH` 也有失败原文（记录目录 vs 实际探测目录）。自测：**18 例**全绿。

### 真实目录结果（只读）

- `runtime/LoginSrv`（单项目）：`DEPS_GREEN packages=48 files=1063`，`UNCOVERED_DEPLOYED=0`。
- `runtime/DBSrv`（并集 `src/DBSrv` + `DBSvr.Storage.MySQL`）：包数/文件数以机器产物为准 ——`E:/UserFleet/evidence-cardfix-20261119/deps-check-and-selftest.log`（`DEPS_GREEN …` 行）；**注意** `543e8e39b` 清掉 NLog 后包集与例外条目均已变化，卡面不再手抄数字。
  **（历史）当时还剩 1 条红**：`DEPS_RED MISSING_DEPLOYED NLog/5.2.8 NLog.dll`——已在 `543e8e39b` 清掉 NLog 依赖与例外条目，**现无 `DEPS_EXCEPTION`**。
  根因：`DBSvr.Storage.MySQL` 声明并随构建产出 `NLog.dll`（其 `bin/Debug` 有），但
  `Directory.Build.targets` 只把**插件 DLL 本身**拷进 DBSrv 输出 ⇒ 部署目录缺 NLog；
  现网 `runtime/DBSrv` 里 `MySqlConnector.dll` 是别的途径带进去的，`NLog.dll` 没有。
  **一件一议**：要么改 target 连依赖闭包一起拷（部署修复），要么把该插件的 NLog 依赖移除/标为不部署。
  在结论前，DBSrv 这条保持红——不降级、不默默 WARN。

## 复审第 3 轮：显式例外（NLog）与身份缺口登记

### NLog 处置 = (b) 显式例外（不改 DLL、不改 target；10k 冻结件不动）

- 复核"零 API 使用"：`grep -rn "LogManager|Logger|Log\.|using NLog" src/Storeages/DBSvr.Storage.MySQL --include=*.cs`
  → 只有 `src/Storeages/DBSvr.Storage.MySQL/DBContext.cs`（第 2 行）、`src/Storeages/DBSvr.Storage.MySQL/PlayRecordStorage.cs`（第 4 行） 两行 `using NLog;`，**没有任何调用** ⇒ 不在运行时引用。
- 例外文件：`src/DBSrv/deps.exceptions.json`
  `[{"pkg":"NLog","version":"5.2.8","reason":"declared by DBSvr.Storage.MySQL, zero API usage (only using-directives), not on any runtime path"}]`
- `check` 语义：被例外覆盖的 `MISSING_DEPLOYED` 打印 `DEPS_EXCEPTION <pkg>/<ver> <reason>` 且**仍绿**；
  未被用到的例外打印 `DEPS_EXCEPTION_UNUSED`（不红）；理由为空/缺字段 ⇒ 生成/读 manifest 阶段红
  （禁止空口豁免）。例外只压制 `MISSING_DEPLOYED`，不影响 sha/覆盖判定。

### 真实目录结果（只读，最终）

```
DBSrv（并集 47 包 / 1045 文件）：
DEPS_EXCEPTION NLog/5.2.8 declared by DBSvr.Storage.MySQL, zero API usage (only using-directives), not on any runtime path
DEPS_GREEN packages=47 files=1045

LoginSrv（48 包 / 1063 文件）：
DEPS_GREEN packages=48 files=1063
```
`UNCOVERED_DEPLOYED=0`；**显式例外共 1 笔**（NLog）。第四颗牙原文保持；自测 **21 例**。

### 顺带登记的身份缺口

`DBSrv.Storage.MongoDB.dll` 在 `runtime/DBSrv` 里**被部署但无戳**、也不在 `release-1af56267` 的 8 件里
⇒ 另卡 `docs/issues/openmir2-own-binary-without-stamp.md`（重构建补戳 / 从部署集移除，**10k 之后**）。

### 锚必须可解析（红 → 改锚 → 绿，范例）

合并 head M5 上 anchor gate **首跑红**（2026-11-19）：

```
PROSE_ANCHOR_PATH_MISSING      docs/issues/openmir2-deps-check-deploy-dir.md  DBContext.cs:2  base_dirs 都解析不到
PROSE_ANCHOR_PATH_MISSING      docs/issues/openmir2-deps-check-deploy-dir.md  PlayRecordStorage.cs:4  base_dirs 都解析不到
prose-anchor gate：检查 2 / 悬空 2 / 越界 0 / 符号缺失 0 / 高churn 0
RESULT: RED
```

改法：裸文件名锚改成**全路径 + 「第 N 行」**叙述（不写 `path:line`，避免行号漂移）：

> `src/Storeages/DBSvr.Storage.MySQL/DBContext.cs`（第 2 行）、
> `src/Storeages/DBSvr.Storage.MySQL/PlayRecordStorage.cs`（第 4 行）

复跑：

```
prose-anchor gate：检查 0 / 悬空 0 / 越界 0（新增行里没有任何 path:line 锚；仅反引号内 .rs/.cs，检查 0 ≠ 全部锚干净）
RESULT: GREEN
```

**没有改 gate 规则**——红是 gate 在正常工作，修的是文档锚。

## QA 整改（2026-11-19）

> 统一口径：**卡面不再手抄测试数字**，一律引用机器产物路径；状态写"已合并（<sha>）"。

- 机器产物：`E:/UserFleet/evidence-cardfix-20261119/`（`packlen-test.log`、`utf8-test.log`、`deps-check-and-selftest.log`）；
- 实跑命令与 `git status --porcelain` 原文：见上述日志（本次采集时 `git status --porcelain` = 0 行）；
- **源码文本/`grep` 型断言**：仅作"接线提示"，**不作行为证据**（QA 章程 §12）；行为判据以
  实跑帧/JSON 产物为准。
> 来源：QA 签署摘要 —— 公共路径 `E:/UserFleet/qa/qa-merged-cards-signoff-20261119.md`（QA 已放置）。

### 可独立重放的篡改（T1–T5；**都在副本 `qa-deps/deploy` 上做，不碰真实目录**）

| 编号 | 改什么 | 怎么改 | 期望红在哪 | 还原 |
|---|---|---|---|---|
| T1 | 副本 `qa-deps/deploy/MemoryPack.Core.dll`（包 `MemoryPack.Core/1.10.0`，size 207872） | **只翻最后一个字节** `0x00 → 0xFF`（`b[-1] ^= 0xFF`） | `DEPS_RED DEPLOYED_SHA_MISMATCH MemoryPack.Core/1.10.0 MemoryPack.Core.dll expected=101f0cfbf844d0277b41cdff67bbb33f90e75dbe094387c321300e1434576bee actual=8e137a6fe19278b52d95fe1c319d0adb48f50423c4b702b66040862e0cab9bea` | 复原后全绿 |
| T2 | 部署副本 `Microsoft.Extensions.Logging.dll` | 删除 | `DEPS_RED MISSING_DEPLOYED Microsoft.Extensions.Logging/8.0.0` | 复原后全绿 |
| T3 | 部署副本 | 新增 `Stray.Third.Party.dll` | `DEPS_RED UNCOVERED_DEPLOYED Stray.Third.Party.dll` | 复原后全绿 |
| T4a | 副本删 `Microsoft.Extensions.Logging.dll`；manifest 用副本 `qa-deps/dbsrv.exc.manifest.json`（`exceptions` 含 `{"pkg":"Microsoft.Extensions.Logging","version":"8.0.0"}`） | 见左 | `DEPS_EXCEPTION …` + `DEPS_GREEN packages=46 files=1030`（rc=0；证明例外**只**压制 `MISSING_DEPLOYED`） | 复原后全绿 |
| T4b | 先 `cp` 原件回副本，再对 `qa-deps/deploy/Microsoft.Extensions.Logging.dll`（包 `Microsoft.Extensions.Logging/8.0.0`，size 50848）**只翻最后一个字节** `0x00 → 0xFF`；manifest 仍为 `qa-deps/dbsrv.exc.manifest.json` | 见左 | `DEPS_EXCEPTION_UNUSED` + `DEPS_RED DEPLOYED_SHA_MISMATCH Microsoft.Extensions.Logging/8.0.0 Microsoft.Extensions.Logging.dll expected=4330a5efe9d110afb7f8e567d5b43eee976e0f7f6802d13a211b65d747529179 actual=719a47903edabc4b715221b7bfc0c72ccf2a0347ce17e5170aaf7c42cc4a9637`（证明**不**压制 sha） | 复原后全绿 |
| T5 | 部署目录 | 改成缺目录 / 空目录 | `MISSING_DEPLOY_DIR` / `EMPTY_DEPLOY_DIR` | 复原后全绿 |
### 统一整改（C4–C6）

- **C4 `built_at`（裁定正文，引用不指向未合并分支）**：戳的 `built_at` 必须等于**二进制自报**的
  `built_at`（单一来源）；文件 mtime 只作元数据、**不参与比对**；闸门断言"自报 == 戳"。
  M9 四件因自报与戳相差 5 秒判 `CONDITIONAL`（身份可证、语义待统一），不算 FAIL。
- **C5 `SHA256SUMS`（已补实）**：两份冻结目录 `E:/UserFleet/release-csharp-m9-572a0b8a/` 与
  `E:/UserFleet/release-m9-572a0b8a/` 均为双清单；`sha256sum -c` 的结果与 rc **见验证日志**
  `E:/UserFleet/evidence-cardfix-20261119/sums-verify.log`（卡面不写行数/OK 数）；历史
  `SHA256SUMS.txt` 不改写；用途差异见各自 `PROVENANCE.txt` 的 `sums_files=` 字段。
- **C3 构建命令与仓库状态（已补实）**：两目录的 `PROVENANCE.txt` 均含 `build_cmd=` / `build_cwd=` /
  `toolchain=` / `git_status_porcelain_measure=` 字段（**字段值照抄，卡面不写数字**）；
  C# 侧实跑命令与仓库状态见 `E:/UserFleet/evidence-cardfix-20261119/RUNS.txt`。
- **C6 章程引用**：权威 QA 章程在 **`E:\OPC\docs\qa-charter.md`（§10–§12）**，不在本仓库；
  卡面凡引"章程 §N"均以此文件为准。
