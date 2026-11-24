# openmir2-own-binary-without-stamp

> 状态：**2026-11-20 已收口**：R1–R5 全部落地；C# 重建窗口完成——全 C# 栈在 M29
> （`f5ae02c7…`）强制重编，九件二进制内嵌 rev 与戳 head 一致，部署副本已更新，新冻结集
> `release-csharp-f5ae02c7` 生成并校验通过；旧 m9 冻结集保留并加注（见 §7.9）。
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

分支阶段已完成的事实 —— M7 上会重产一次，head 以 M7 为准：

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

### 7. 落地与复核（2026-11-20，R1–R4 已实施）

#### 7.1 R1 冻结计数：动态 + 自检（另修了一个既有脚本 bug）

- `w-freeze-release.ps1`：PROVENANCE 的 `files=` 改由组清单动态求和（9+9=18），并在写清单前自检
  `$lines.Count == $fileCount`；不等 ⇒ `RED COUNT_MISMATCH …`、`exit 2`。
- **实测**：以 `-OutRoot E:/UserTemp/freeze-real`（临时目录）跑 → rc=0，PROVENANCE
  `files=9 binaries + 9 .build.json (SHA256SUMS 覆盖全部 18 个文件)`，`SHA256SUMS` 18 行。
- **T3（红）**：把 `$fileCount` 伪造成 16 后跑 → **rc=2**：
  `RED COUNT_MISMATCH lines=18 expected=16`。
- **顺带修复（真实 bug，非本卡设计）**：该脚本把参数 `$Head` 与内部哈希 `$heads` 当两个变量用——
  PowerShell 变量名**大小写不敏感**，二者是同一变量；`$heads = @{}` 覆盖期望 head 后，`$uniq[0]`
  退化成字符串首字符（现场报 `戳 head 5 ≠ 期望 …`）。**改动前的备份脚本同样报此错**，说明盘上这
  份脚本当时已不可用（m9 冻结应由另一份副本产出）。已改名 `$headsByItem` 并用 `@(...)` 保证数组。

#### 7.2 R2 清单单一来源

- 新增 `tools/orchestration/stamp-groups.psd1`（随 git 版本化；sha256
  `341deb377b8d707edb6bce9d37c4b40c031c0eb134acc13b62ea4fb6de7f03e7`），导出 `Groups`
  （DBSrv 6 件 / LoginSrv 3 件）与 `OwnGlobs`。
- 消费者改为同一来源：`tools/window/start-csharp.ps1`（六件 + `-OwnGlobs`）、
  `tools/hygiene/assert_three_way_aligned.ps1`（DBSrv 6 件 / LoginSrv 3 件）、
  `E:/UserFleet/m1-stack/start-m1-stack.ps1`（`$StampGroups`）、`E:/UserFleet/w-freeze-release.ps1`
  （`$groups` + 计数）。
- 本机 `powershell` 5.1 **没有** `Import-PowerShellDataFile`、dot-source `.psd1` 也返回 null（实测）
  ⇒ 统一用 `Invoke-Expression (Get-Content -Raw)`（文件在仓库内、随 git 评审，同一信任级）。
- 实测：`runtime/LoginSrv` 按 psd1 三件 + globs ⇒ `GREEN 3 stamps verified`；
  `assert_three_way_aligned -Group LoginSrv` ⇒ 三件 `THREE_WAY_OK`；其 `-Selftest` GREEN。

#### 7.3 R3 `-OwnGlobs` 独立覆盖检查

- `stamp-binaries.ps1`：新增 `-OwnGlobs`（非 `-Check` 给参数 ⇒ rc=2）；`-Check` 主循环后、
  `$found -eq 0` 判定前独立扫描——匹配 glob 的 `.dll/.exe` 若基名不在 `-Names` ⇒
  `RED <file> UNLISTED_OWN_BINARY`，**与 `-RequireAll` 无关**；自测加 Case 13/14，meta 12→14。
- **实现前（盲区，两条都 rc=0）**：临时副本放无戳 `DBSrv.Storage.LocalFile.dll`，
  带/不带 `-RequireAll` 跑 6 件名单 ⇒ 均 `GREEN 6 stamps verified`。
- **实现后（两条都红）**：同一目录加 `-OwnGlobs 'DBSrv*.dll,OpenMir2.dll,LoginSrv*'` ⇒
  **rc=1**，`RED …\DBSrv.Storage.LocalFile.dll UNLISTED_OWN_BINARY`（带/不带 `-RequireAll` 各一次）。
- 真目录回归：`runtime/DBSrv` 用 6 件 + globs ⇒ `GREEN 6 stamps verified`；
  `-Selftest` ⇒ `GREEN (14 cases)`。
- **T1（红）**：临时副本删 `DBSrv.Storage.MongoDB.dll.build.json` ⇒ rc=1，
  `RED … MISSING_STAMP`，`RED 1 problem(s) across 6 binaries`。

#### 7.4 R4 文档

- `docs/binary-provenance-stamp-20261115.md`：DBSrv 6 件 / LoginSrv 3 件；清单单一来源；
  `-OwnGlobs` 语义；并写明**部署件 head = `572a0b8a`，落后当前 master 是已知且可解释的状态**。

#### 7.5 **新发现（待裁决）：MongoDB 插件的戳与内嵌 head 不符**

- 二进制内嵌 `SourceRevisionId`（ASCII 与 UTF-16LE 各一处）＝ **`a5643481aded4d93a7a47867b7094097df6850f5`**
  （M8：`merge: AOI 增量切成默认路径…`）；
- 侧车戳 `head` ＝ **`572a0b8a…`**（M9：`merge: deps_manifest 构建口径显式化…`），
  `built_at` = 文件 mtime（18:31，早于其余五件 19:03）；`src/DBSrv/bin/Release` 副本内嵌同为 `a5643481`。
- ⇒ 这正是「MSBuild 增量构建跳过源码未变项目 + 事后按 M9 产戳」的典型：**二进制是 M8 产物、戳写成 M9**。
  M9 的冻结之所以成功，是因为冻结脚本只断言**戳 head 一致**、不看二进制内嵌。
- 把 MongoDB 纳入三处对齐清单后，`assert_three_way_aligned -Group DBSrv` 报
  `THREE_WAY_RED DBSrv.Storage.MongoDB.dll reason=embedded_mismatch`（main/shadow/build 三处一致地红）。
- **可选处置（Leader 裁决）**：
  1. **只改戳**：用 `-Head a5643481… -HeadSource explicit` 重产该件戳（不动二进制/部署），
     好处是身份立刻为真；代价是冻结脚本的「全组同一 head」断言不再成立（需改为逐件 head 可解析 +
     记录混合 head），且要同步更新部署副本与既有冻结副本的戳（涉及"改部署/冻结件"，需授权）。
  2. **重建**：在指定 head 上 `dotnet build` MongoDB 插件 + 产戳 + 替换部署 + 重冻结（需 C# 构建窗口；
     与本轮"不重建 C#、不动冻结件"的口径冲突，需另排）。
  3. **登记为已知坏件**：保持现状并在交付注记里写明"该件戳不可信、真实来源 M8"，闸门对该件显式
     豁免（不推荐：等于把"部署了但身份错"制度化）。

### 7.6 R5：`-Check` 增加 C# 内嵌 rev 与戳 head 的一致性（根因牙，已落地）

- **读内嵌 rev 的方法（实测命令 + 原文）**：
  ```text
  python - <<'PY'
  import re
  b = open(r'E:/UserFleet/m1-stack/runtime/DBSrv/DBSrv.Storage.MongoDB.dll','rb').read()
  print([m.group().decode() for m in re.finditer(rb'[0-9a-f]{40}', b)])
  PY
  → ['a5643481aded4d93a7a47867b7094097df6850f5']
  ```
  工具内实现为 `Get-EmbeddedRevisions`：ASCII（ISO-8859-1/28591 字节→字符映射）与 UTF-16LE
  各扫一遍 `[0-9a-f]{40}`；与 `tools/hygiene/assert_three_way_aligned.ps1` 同一实现
  （PS 5.1 没有 `[Text.Encoding]::Latin1`，故用 28591）。
- **判据**：dotnet 目标（`toolchain_kind=dotnet`、旧戳带 `dotnet` 字段、`.dll`、或基名在
  `DotnetNames`）⇒ 二进制必须含内嵌 40-hex 且**以戳 head 为前缀**；无 ⇒ `EMBEDDED_REV_MISSING`；
  有但不匹配 ⇒ `EMBEDDED_REV_MISMATCH stamp=… embedded=…`。Rust 目标不走此支（已有二进制自报的
  `SELF_REV_MISMATCH`）。
- **现场原文**（真实部署目录，6 件 + `-OwnGlobs 'DBSrv*.dll,OpenMir2.dll'`，rc=1）：
  ```text
  STAMP_OK …\DBSrv.exe
  STAMP_OK …\DBSrv.dll
  STAMP_OK …\DBSrv.Storage.dll
  STAMP_OK …\DBSrv.Storage.MySQL.dll
  RED …\DBSrv.Storage.MongoDB.dll EMBEDDED_REV_MISMATCH stamp=572a0b8a3ea244351a6633fe6af0d0fe4a20643a embedded=a5643481aded4d93a7a47867b7094097df6850f5
  STAMP_OK …\OpenMir2.dll
  RED 1 problem(s) across 6 binaries
  ```
  `runtime/LoginSrv` 同口径 ⇒ `GREEN 3 stamps verified`（rc=0）。
- **牙（自测）**：Case 15（内嵌 `deadbeef…` ≠ 戳 head）⇒ `EMBEDDED_REV_MISMATCH` rc=1；
  Case 16（无内嵌）⇒ `EMBEDDED_REV_MISSING` rc=1；`-Selftest` ⇒ `GREEN (16 cases)`。
- **互不掩盖**：T1（删戳）仍只报 `MISSING_STAMP`；T2（未列自家 DLL）仍报 `UNLISTED_OWN_BINARY`
  （若同目录 MongoDB 也在，会同时报它的 `EMBEDDED_REV_MISMATCH`，两条各自成立）。

### 7.7 裁决与后续（2026-11-20，opc-07）

- 裁定 **选项 2：重建该插件**。不改戳（事后改戳 = 构建期之外写身份；且 m9 冻结件不可变）。
- 待执行：C# 构建窗口（与 `d4`/`bd`/`2d` 错峰）→ 重建 `DBSvr.Storage.MongoDB` → 产戳 → 替换部署副本
  → 生成**新冻结集**；旧 `release-csharp-m9-572a0b8a` **保留**，并在其 `PROVENANCE`/文档里标注
  「含一件内嵌 rev 与戳不一致的 MongoDB 插件（见本卡）」。
- 当前口径：重建完成前，该件在 `stamp-binaries -Check` 下如实红、起栈门 fail-closed；UserFleet 两个
  脚本已指向仓库内 psd1，**合并本分支前**若需起栈，用 `.bak-20261120-022849` 回滚后再应用。

### 7.8 交付说明里需要单列的两点

1. **`$Head` / `$heads` 大小写撞名（既有 bug，非本卡设计）**：PowerShell 变量名大小写不敏感 ⇒
   参数 `$Head` 与内部哈希 `$heads` 是同一变量；`w-freeze-release.ps1` 旧版本运行即报
   `戳 head 5 ≠ 期望 …`（`$uniq[0]` 退化为字符串首字符）。**改动前的备份脚本同样报此错**，
   说明盘上那份当时已不可用，m9 冻结应由另一份副本产出。已改名 `$headsByItem` 并用 `@(...)`。
2. **M8→M9 增量跳过证据链**：内嵌 rev `a5643481`（M8：AOI 增量切默认）与戳 head `572a0b8a`（M9：
   deps_manifest 构建口径）并存；MongoDB 件 mtime 18:31、其余五件 19:03；`src/DBSrv/bin/Release`
   副本内嵌同为 M8 ⇒ 不是"部署拷贝错"，而是**构建期就没重建**、事后按 M9 产戳。

### 7.9 重建完成（2026-11-20，C# 窗口）

- **强制重编**（`--no-incremental`，dotnet `9.0.313`）：`src/DBSrv/DBSrv.csproj`、
  `src/LoginSrv/LoginSrv.csproj`、`src/Storeages/DBSvr.Storage.MySQL`、
  `src/Storeages/DBSrv.Storage.MongoDB` 四个工程，**均 0 错误**；主检在 M29（`f5ae02c7…`）干净树。
- **内嵌 `SourceRevisionId` 实测**（ASCII + UTF-16LE 双编码扫描）全部 =
  `f5ae02c7e595002a982e44ebe1ab7eeffaf975f9`；注意 exe 只在 **UTF-16LE** 里带，dll 在 ASCII 里带，
  工具两种编码都扫（缺一侧就会假红/假绿）。
- **重产戳**：`-Head f5ae02c7… -HeadSource explicit -Force`；`src/DBSrv/bin/Release` 6 件、
  `src/LoginSrv/bin/Release` 3 件。
- **部署副本更新**（`Copy-Item` 保 mtime）：DBSrv 6 件 + 6 戳、LoginSrv 3 件 + 3 戳；
  替换前备份 `E:/UserFleet/m1-stack/backup-20261120-023546/`（18 个文件的 sha256 已记录）。
- **校验**：runtime 两组 `-Check -RequireAll -RequireHead f5ae02c7… -OwnGlobs …` ⇒
  `GREEN 6 stamps verified` / `GREEN 3 stamps verified`；`assert_three_way_aligned -Group DBSrv`
  与 `-Group LoginSrv` ⇒ 9 条 `THREE_WAY_OK`（`embedded=ok`）。
- **新冻结集**：`E:/UserFleet/release-csharp-f5ae02c7`（9 binaries + 9 `.build.json`；
  `SHA256SUMS.sha256` 18 条，`sha256sum -c` 全 `OK`）；旧 `release-csharp-m9-572a0b8a` 原样保留，
  新增 `PROVENANCE.ADDENDUM-20261120.txt` 标注 MongoDB 件的内嵌/戳不一致与 supersede 关系。

### 7.10 a2 只读评审整改（2026-11-20）

- **A1**：`start-m1-stack.ps1` 的 `$srcGroups`（源目录门）改为从 `tools/orchestration/stamp-groups.psd1`
  取名单（此前同文件内联 6+3，与影子门两套清单会静默漂移）；源目录戳校验同时带 `-OwnGlobs`。
- **D5**：`tools/window/start-csharp.ps1` 的戳门从「只 DBSrv」扩到「DBSrv 6 + LoginSrv 3」——
  该脚本的 `$components` 本来就会拉起 LoginSrv。带 `-GateOnly` 只跑闸门不起进程；正/反例在
  **夹具根**（`E:/UserTemp/d5-gate-root`，仅含 6+3 自有件与戳）上真跑，**未在真实栈根演示**
  （真实栈根会起进程）。
- **C4**：`start-m1-stack.ps1` 注释「两目录 8 件」→「两目录 9 件（DBSrv 6 + LoginSrv 3）」。
- **B1–B4 / C2 / C3 / D1 / D2 / D4**：`docs/binary-provenance-stamp-20261115.md` 补 R5 判据与 §11；
  改写「没有把 commit 内嵌进二进制」；登记 Rust 条件性判据的**静默跳过**风险；补短 head 前缀匹配弱化；
  自测矩阵由 10 行改为 16 例（Case 9–16 补齐）；写入 `$Head`/`$heads` 大小写撞名与 PS 5.1 无
  `Import-PowerShellDataFile` 的坑；`dist` 在仓库与 E:/UserFleet 均查无出处——**此前未登记**，已写明
  发布目录不使用 `dist`。

