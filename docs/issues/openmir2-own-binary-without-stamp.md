# openmir2-own-binary-without-stamp

> 状态：**2026-11-20 只读复核：原缺陷已在 2026-11-19 的 M9 收口（有戳、在 9 件冻结集里）；本卡转为
> 「结构性闸门补齐 + 陈旧文案清理」**，执行窗口由 Leader 排（详见文末「现状核实与准备」）。
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

## 结论与实施（2026-11-19，opc-07 裁决：选项 1「重构建 + 补戳」）

> 状态：**已定案（重构建 + 补戳），在 M7 上执行收口**（分支 `card/m7-small-cards`，与
> `openmir2-nlog-using-cleanup` 合并为 M7；戳的 head 必须是 M7 本身，故产戳放在合并之后）。

### 为什么选 1 而不是 2/3

- 选 2（从部署集移除）需要先证明运行时永不加载该插件；插件是按名字动态加载的，
  「现在没启用」不等于「部署集里不该有」，证明成本高于补戳。
- 选 3（显式登记无身份）等于把「部署了但无身份」制度化，与 `stamp-binaries` 的口径相反。
- 补戳后该件与同一目录的 `DBSrv.Storage.MySQL.dll` 同规格：有 `mir2-build-stamp/1` 侧车戳，
  构建来源可回答。

### 原件的来源**不可考**（这是本卡的前提事实）

部署在 `m1-stack/runtime/DBSrv/DBSrv.Storage.MongoDB.dll` 的那一份：mtime `2026-10-10`，
无侧车戳，不在 `release-1af56267` 冻结的 8 件里，也没有任何构建记录能把它绑到某个提交。
换言之，**无法回答它来自哪个提交**，也无法证明它与当前源码等价。因此本卡重建的这一份是它的
**正式替代件**（不是「顺手更新」），原件只作对照保留在备份目录。

### 实施步骤与证据

1. **在 M7 上重建**（源码同一份，M7 即合并后的主线 head）：
   `dotnet build src/Storeages/DBSvr.Storage.MongoDB/DBSrv.Storage.MongoDB.csproj -c Release`
   → 0 错误；产物 `src/Storeages/DBSvr.Storage.MongoDB/bin/Release/DBSrv.Storage.MongoDB.dll`。
2. **产侧车戳**：`tools/orchestration/stamp-binaries.ps1 -BinDir <插件输出目录>
   -Names DBSrv.Storage.MongoDB.dll -Head <M7> -HeadSource explicit`
   → `head_source=explicit`、`dirty=""`（M7 干净树）、`head=` 为 40 位 M7。
3. **替换部署副本**：先把原件备份到 `E:/UserFleet/m1-stack/backup-unstamped-mongodb-<ts>/`
   （记录 sha256 与 mtime），再把新 DLL + `.build.json` 放到
   `m1-stack/runtime/DBSrv/`，并同步放到 `src/DBSrv/bin/Release/`
   （否则下次 `-RefreshRuntime` 会把它覆盖回没有戳的状态）。
4. **纳入闸门分组**：DBSrv 戳组 5 件 → 6 件，见下节。

分支阶段已完成的事实（M7 上会重产一次，head 以 M7 为准）：

- 旧部署件：`a35b27e7...`，12288 B，无戳；
- 分支重建件：`70143345...`，12288 B（同尺寸、不同字节；工具链/时间不同）；
- `.build.json` 的 `head` 只认 **40 位**、且与启动器传入的期望 head 严格相等，
  所以戳必须在 M7（不是分支 head）上产。

### 外部闸门分组（botfleet 侧）

DBSrv 的戳组**不在** `w-rust-gate.ps1`（那个脚本只管 Rust 四件 exe，分组在 `$Names`
默认值里）。真正的位置是启动器：

- `E:/UserFleet/m1-stack/start-m1-stack.ps1` 的 `$StampGroups`，`DBSrv` 组当前 5 件：
  `DBSrv.exe, DBSrv.dll, DBSrv.Storage.MySQL.dll, DBSrv.Storage.dll, OpenMir2.dll`，
  **需加第 6 件 `DBSrv.Storage.MongoDB.dll`**（脚本随后用 `-Names (...) -join ','`
  调 `stamp-binaries.ps1 -Check -RequireAll`，加名字即生效，无其它逻辑改动）。
- 同一份列表还在冻结脚本 `E:/UserFleet/w-freeze-release.ps1`（`'DBSrv'` 行，同为 5 件）里；
  若冻结集也要覆盖 MongoDB 插件，应同步加第 6 件并把「8 件」的表述改成「9 件」——
  这一条属于**发布集口径**，需 opc-07 裁决，本卡不擅自改。

**两侧都改完才算完成**：仓库侧（新件 + 戳 + 部署副本）由本卡完成；启动器组的第 6 件由 botfleet
改并回证，缺一侧则戳组仍只校验 5 件、本卡「部署了但无身份」的问题只解决一半。

### 验收

- `runtime/DBSrv/DBSrv.Storage.MongoDB.dll.build.json` 存在，`head` = M7（40 位）、
  `head_source=explicit`、`dirty=""`、`sha256`/`size` 与实物一致；
- 用加过第 6 件的组跑 `stamp-binaries.ps1 -Check -Names <6 件> -RequireAll -RequireHead M7` → 全绿；
- `deps_manifest check --deploy-dir runtime/DBSrv` 仍 `DEPS_GREEN`（`dbsrv.storage.mongodb.dll`
  在 `SELF_BUILT` 名单内，不参与 NuGet 映射）；
- 10k 冻结件（`release-ctrl-96422fd5` / `release-exp-c3d3c271`）互不影响：那两组是窗口期快照。

## 现状核实与准备（2026-11-20，只读；未动构建/部署/冻结件）

> 结论先行：原卡描述的具体缺陷（MongoDB 插件无戳、不在冻结/闸门清单）**已在 2026-11-19 的
> M9（`572a0b8a`）收口**——实物与戳、冻结副本、两组 E:/UserFleet 脚本三者一致。本次复核的
> 产出是**剩余结构性缺口**与改法/牙/计划；不重建、不替换任何部署件。

### 1. 现状核实（逐件；命令与原文）

命令：
- `ls -la E:/UserFleet/m1-stack/runtime/DBSrv/`
- `sha256sum <file>` / `cat <file>.build.json`
- `powershell -File tools/orchestration/stamp-binaries.ps1 -Check -BinDir E:/UserFleet/m1-stack/runtime/DBSrv -RepoRoot <repo> -Names DBSrv.exe,DBSrv.dll,DBSrv.Storage.MySQL.dll,DBSrv.Storage.dll,DBSrv.Storage.MongoDB.dll,OpenMir2.dll -RequireAll`
- `python -I -B tools/deploy/deps_manifest.py check --projects "src/DBSrv/DBSrv.csproj;src/Storeages/DBSvr.Storage.MySQL/DBSvr.Storage.MySQL.csproj" --manifest E:/UserFleet/m1-stack/deps/DBSrv.deps.json --deploy-dir E:/UserFleet/m1-stack/runtime/DBSrv`

自家 DLL（6 件；其余为第三方，按设计无戳）：

| 件 | size | 侧车戳 | head | 冻结副本（`release-csharp-m9-572a0b8a`） |
|---|---|---|---|---|
| `DBSrv.exe` | 151552 | 有 | `572a0b8a…` | 有（sha 一致） |
| `DBSrv.dll` | 67584 | 有 | `572a0b8a…` | 有 |
| `DBSrv.Storage.dll` | 12288 | 有 | `572a0b8a…` | 有 |
| `DBSrv.Storage.MySQL.dll` | 64512 | 有 | `572a0b8a…` | 有 |
| `DBSrv.Storage.MongoDB.dll` | 11776 | **有**（2026-11-19 18:31 产） | `572a0b8a…` | **有**（sha `38c7a440…` 与部署件相同） |
| `OpenMir2.dll` | 245760 | 有 | `572a0b8a…` | 有 |

原文：
- `stamp-binaries -Check … -RequireAll` ⇒ `GREEN 6 stamps verified`（rc=0；六件逐个 `STAMP_OK`）。
- `deps_manifest check --deploy-dir runtime/DBSrv` ⇒ `DEPS_GREEN packages=46 files=1030`（rc=0；
  `dbsrv.storage.mongodb.dll` 在 `SELF_BUILT` 名单内，不参与 NuGet 映射）。
- E:/UserFleet 侧两处清单均已含 MongoDB：`start-m1-stack.ps1` 的 `$StampGroups` = 6 件；
  `w-freeze-release.ps1` 的 `'DBSrv'` 组 = 6 件（6+3=9 件，两组 `LoginSrv` 3 件已全部有戳）。
- 冻结产物 `release-csharp-m9-572a0b8a`：9 binaries + 9 `.build.json`（`SHA256SUMS` 18 行、
  `SHA256SUMS.sha256` 18 条），`PROVENANCE.txt` 记录构建命令（含 MongoDB 插件）与
  `git status --porcelain ⇒ 0`。
- 旧冻结 `release-1af56267/DBSrv` 只有 5 件——那是历史口径，保留不动。

### 2. 剩余缺口（本次复核发现，均未修）

- **R1 冻结脚本计数硬编码**：`E:/UserFleet/w-freeze-release.ps1` 的 PROVENANCE 文案仍写
  「8 binaries + 8 .build.json（16 个文件）」，而组已是 9 件、m9 实际产出 18 行。下次冻结会
  写出**错误计数**（内容拷对了，自述错）。
- **R2 仓库内窗口/卫生脚本仍是 3 件名单**：`tools/window/start-csharp.ps1` 与
  `tools/hygiene/assert_three_way_aligned.ps1` 只点名 `DBSrv.exe / DBSrv.dll /
  DBSrv.Storage.MySQL.dll`，不含 `DBSrv.Storage.dll` / `DBSrv.Storage.MongoDB.dll` /
  `OpenMir2.dll`。仓库侧窗口门禁与起栈门（6 件）覆盖不一致。
- **R3 未入清单的自家 DLL 完全不可见（实测）**：在临时副本放入无戳
  `DBSrv.Storage.LocalFile.dll`，用 6 件名单跑 `-Check -RequireAll` 仍 `GREEN 6 stamps
  verified`（rc=0）；只有把文件名列进 `-Names` 才会 `MISSING_STAMP`。这正是原缺陷的生成
  机制（名单靠人维护），未修。
- **R4 文档陈旧**：`docs/binary-provenance-stamp-20261115.md` 仍写「DBSrv 三件套」与
  「LoginSrv 同法可纳入，尚未做」，与现状（DBSrv 6 件、LoginSrv 3 件、均已冻结）不符。

### 3. 改法设计（不实施）

① **stamp-binaries 的清单扩展与覆盖**
- 新增 `-OwnGlobs`（或 `-RequireStampedGlob`）：对 BinDir 里匹配自家模式（`DBSrv*.dll`、
  `DBSrv.exe`、`OpenMir2.dll`、`LoginSrv*.dll` 等）的每个文件，要求同时满足「在 `-Names`
  里」且「戳自洽」；未列/无戳 ⇒ `RED UNLISTED_OWN_BINARY`（rc=1）。这是 R3 的根治：把
  「名单覆盖」从人工记忆变成机判。
- **清单单一来源**：把 DBSrv/LoginSrv 两组抽到共享 `stamp-groups.psd1`（E:/UserFleet），
  `start-m1-stack.ps1` 与 `w-freeze-release.ps1` dot-source 同一份；仓库侧
  `tools/window/start-csharp.ps1` 用同一份或 `-OwnGlobs` 等价覆盖（消 R2）。
② **冻结清单与自述**
- 组内容已 9 件；把 `PROVENANCE` 的文件计数改为**动态计算**（`$groups.Values.Count` 求和 ×2），
  并加自检：`PROVENANCE files=N` 必须等于本次实际拷贝条数，否则 rc=2（消 R1）。
- 「8→9」的文字口径按动态计数输出，不再手写。
③ **分工（谁管什么，避免重叠）**
- `stamp-binaries` = **身份与覆盖**：来源提交 / sha / size / mtime / 全套自家件是否都在册；
- `deps_manifest --deploy-dir` = **依赖闭包**：NuGet 映射、`UNCOVERED_DEPLOYED`、
  `SELF_BUILT` 豁免；**不**承担 provenance，也不要求有戳；
- `w-freeze-release.ps1` = **交付副本**：只读拷贝 + 断 head 一致 + 计数自检；
- `start-m1-stack.ps1` = **起栈门**：调 `stamp-binaries -Check -RequireAll`（fail-closed）。
- 明确不重叠：deps 不看戳；stamp 不看 NuGet 映射；冻结只看 head 一致与计数。

### 4. 牙设计

- **T1（删戳必红，已实测）**：临时副本删 `DBSrv.Storage.MongoDB.dll.build.json` ⇒
  `stamp-binaries -Check -RequireAll` rc=1，原文：
  `RED …\DBSrv.Storage.MongoDB.dll MISSING_STAMP` / `RED 1 problem(s) across 6 binaries`。
- **T2（未入清单必须红，设计）**：加 `-OwnGlobs DBSrv*.dll,OpenMir2.dll,LoginSrv*` 后，
  在临时副本放无戳 `DBSrv.Storage.LocalFile.dll` ⇒ rc=1、`UNLISTED_OWN_BINARY`。
  **对照（已实测）**：同一目录在**加参数前** rc=0、`GREEN 6 stamps verified`——证明当前
  盲区真实存在；把该件列进 `-Names` 时才 `MISSING_STAMP`。
- **T3（计数自检必红，设计）**：把冻结脚本的 PROVENANCE 计数写死回 8，冻到临时目录 ⇒
  自检 `COUNT_MISMATCH 8 != 18`（rc=2），不产出「自述正确」的冻结目录。
- **T4（保留回证）**：改一个 `.build.json` 的 head ⇒ 冻结脚本「戳 head 不一致」rc=2。

### 5. 执行计划（待 Leader 排窗口）

- **窗口**：小型工具窗口（单车道 15–20 分钟）。**不需要重建 C# 产物**——实物与戳已在 M9
  收口；本次只改工具/清单/文案。若 Leader 要求戳头跟到当前 master，则需另排 `dotnet build`
  + 产戳 + 部署替换 + 重冻结（本卡默认不做）。
- **会动的件**：仓库内 `tools/orchestration/stamp-binaries.ps1`、`tools/window/start-csharp.ps1`、
  `tools/hygiene/assert_three_way_aligned.ps1`、文档；E:/UserFleet 的
  `start-m1-stack.ps1`、`w-freeze-release.ps1`、新增 `stamp-groups.psd1`。
- **不原地改冻结件**：既有 `release-1af56267`、`release-csharp-m9-572a0b8a`、10k 两组
  （`release-ctrl-96422fd5` / `release-exp-c3d3c271`）保持原样；若需新冻结，产出**新目录**
  `release-csharp-<head8>`。
- **回滚**：仓库内改动走 git（独立分支，不合并即回滚）；E:/UserFleet 脚本改动前保存
  `.bak-<ts>` 副本；部署目录不触碰。
- **验证顺序**：先让 T2/T3 红（证明牙能咬）→ 实现 → T1/T2/T3 绿 → `stamp-binaries -Selftest`
  → `start-m1-stack.ps1` 的 GateOnly 路径（若可用）全绿 → 下一次冻结的计数自检通过。

### 6. 裁定与落地口径（2026-11-20，opc-07）

- **戳头保持 `572a0b8a`，不重建**：身份链只需回答「这份件来自哪个提交」，且部署件与冻结件
  sha 一致；为「跟到当前 master」而重建 + 部署替换 + 重冻结，等于在交付前夜引入一次
  不必要的生产变更，风险大于收益。
  **R4 文档必须写明**：部署件 head = `572a0b8a`，落后当前 master 是**已知且可解释**的状态，
  不得被误读为构建缺失或漂移。
- **窗口**：在 `a2` 的装载改动与 `dinglao` 出包两条交付落地之后，由 Leader 排给本卡。
- **落地顺序**：R1 动态计数 + 自检（T3）→ R2 三件名单升级为六件、共享 `stamp-groups.psd1`
  → R3 `-OwnGlobs` → R4 文档。**不动冻结件、不重建 C#**。
- **R3 硬要求**：`-OwnGlobs` 必须在 `-RequireAll` **之外独立生效**——「未列入 `-Names`、
  但匹配 own globs 且无戳」⇒ 红（`UNLISTED_OWN_BINARY`）；验收原文要求该场景在**不加**
  `-RequireAll` 时同样 rc=1。

