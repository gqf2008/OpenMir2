# openmir2-deploy-deps-manifest

> 状态：**已合并（`e7de2fbd9`；后续跟随 `543e8e39b` 清 NLog、`cfc154b92` 部署目录核验）**——生成器 + 两级检查器 +
> 自测用例数以 `python -I -B tools/deploy/deps_manifest.py selftest` 实跑为准（见 `E:/UserFleet/evidence-cardfix-20261119/deps-check-and-selftest.log`），已入仓并登记工具门（TOTAL 69 → **70**）。已登记（walgit 看板不可用，按 AGENTS.md §7
> 过渡）。看板恢复后迁移为正式卡。tier 建议 **L**。

## 背景
`STAMP_GATE` 的判据是「产物由本仓库某次提交构建」——戳里的 `head` 与源码同源。第三方 NuGet 包
（`MemoryPack.Core.dll`、`TouchSocket*.dll` 等）**没有我们的 head**，把它们塞进 `STAMP_GATE`
只会制造假绿/假红，也会稀释「与源码同源」这个唯一含义。**裁决：不进 `STAMP_GATE`，另走依赖清单。**

## 范围
- 目录：`m1-stackuntime\<服务>\` 下所有被部署的二进制（DBSrv / LoginSrv / 将来的其它 C# 服务）。
- 扩展名：`*.dll`（含 `*.exe` 里属于第三方的部分若有）。
- **归属判定**：能产 `<name>.build.json` 戳的（= 本仓构建产物，如 `DBSrv.dll`、`OpenMir2.dll`）
  归 **stamp**；其余归 **manifest**。两边集合必须**互斥且覆盖**（同一文件不得两边都管或都不管）。

## 生成时机
- 构建车道的 `--no-incremental` 构建结束后、**部署前**生成 `deps.manifest`（与产戳同一批）；
- 记录内容（每行一条）：`文件名 | 声明版本 | 实际版本 | sha256 | 来源(lock/assets) | 构建 head`；
- 声明版本来源：**优先 `packages.lock.json`**（本仓目前**未启用**锁文件 —— 第一步先加
  `RestorePackagesWithLockFile=true`，否则「声明版本」缺权威来源）；未启用期间退而取
  `obj/project.assets.json` 里解析出的版本，但要标明来源，并在卡面写明这是临时口径。

## 两级校验（都要，缺一不可）
1. **版本一致性**：`deps.manifest` 里每个包的「实际版本」必须等于 lock（或 assets）声明的版本
   —— 防「悄悄换包」（例如手工把某个 dll 换成另一个版本）。
2. **内容一致性**：部署目录里每个包的 sha256 必须等于 `deps.manifest` 记录的 sha256
   —— 防「同版本文件被替换/损坏」。
两条各管一类失效：只做版本 → 同版本换文件看不出来；只做 sha → 先换包再改 manifest 就混过去了
（manifest 是构建时产物，改它需要重跑构建车道并留痕）。

## 失败输出（必须有，便于现场定责）
`DEPS_RED <file> declared=<ver> actual=<ver> sha=<实际> expected=<记录>`，并给出清单差异条数；
rc=2（判不了）与 rc=1（判红）分开，不静默通过。

## 与其它机制的关系
- 与 `STAMP_GATE`（本仓产物、按目录 5/3/… 件）**并列**，都在起栈前跑；
- 与 `docs/issues/openmir2-gate-packlen-validation.md` 等的"部署件覆盖面"问题同源：
  本条解决**第三方依赖**的面，`STAMP_GATE` 解决**本仓产物**的面，两者合起来才覆盖"部署目录里的每个二进制都被某种机制管住"。

## 验收
1. 生成器 selftest：合成目录里放 1 个本仓产物 + 1 个第三方 dll，断言归属判定互斥且覆盖；
2. 负例①：改第三方 dll 的版本号（换包）⇒ 版本一致性红；
3. 负例②：同版本替换文件内容 ⇒ sha 红；
4. 旧部署（当前 8 件 + 第三方）在**启用前**应给出「manifest 缺失」的明确提示，而不是静默绿。

## 实现与证据（2026-11-19）

### 范围落定（两句话）

- **本期只做 NuGet**：部署目录（`m1-stack/runtime/{DBSrv,LoginSrv}`）里的第三方 DLL 都由 NuGet 还原；
  权威输入 = `packages.lock.json`（`src/DBSrv`、`src/LoginSrv` 已启用 `RestorePackagesWithLockFile=true`
  并入库）。
- **Rust 依赖暂不在闸门内**，理由：Rust 件静态链接、部署目录里没有 Rust 第三方 DLL，其来源由
  `build-rev`/侧车戳覆盖；`Cargo.lock` 的供应链核对属构建车道（另卡）。**不是忘记。**

### 工具（`tools/deploy/deps_manifest.py`）

```
python -I -B tools/deploy/deps_manifest.py generate --project <csproj> --out <manifest.json>
python -I -B tools/deploy/deps_manifest.py check    --project <csproj> --manifest <manifest.json>
python -I -B tools/deploy/deps_manifest.py selftest
```

- **generate**：读 lock + csproj 声明 + 已还原包（默认 `~/.nuget/packages`，**只读，绝不改缓存**），
  输出 `deps.manifest.json`（`manifest_version=1`；每包含
  `name/type/requested/resolved/source/toolchain/content_hash/nupkg_sha512/files{rel:sha256}`）。
  nupkg 的 SHA512 与缓存侧车 `.nupkg.sha512` 不一致 ⇒ 红（**不自动重下**）。
- **check**：① 版本一致性（csproj `PackageReference` 范围**可满足** lock `resolved`；NuGet 范围语义，
  非字符串比较；csproj↔lock 的 Direct 集合双向核对）；② 内容一致性（manifest 每包文件 sha256 与磁盘
  逐项相等——缺/多/变各报 CODE；nupkg SHA512 一致）。
- **fail-closed**：缺 lock / 缺 manifest / manifest 空或字段缺失 ⇒ 红（rc=1），绝不当"没有依赖"。
- 退出码：0 绿；1 判红（逐条 `DEPS_RED <CODE> <detail>`）；2 用法/判不了。
- **contentHash 口径**：lock 的 `contentHash` 是 NuGet 对**包内容**的规范化哈希，**不是** .nupkg 的
  SHA512；manifest 里分开记录、分别核验（前者记录来源，后者比对缓存实物）。

### 证据

- 真实项目（本机还原缓存）：
```
generate src/DBSrv    → DEPS_WRITE …DBSrv.deps.manifest.json packages=45 files=1010
check    src/DBSrv    → DEPS_GREEN packages=45 files=1010
generate src/LoginSrv → DEPS_WRITE …LoginSrv.deps.manifest.json packages=48 files=1063
check    src/LoginSrv → DEPS_GREEN packages=48 files=1063
```
- 三颗牙（**离线合成夹具**，不动真实缓存；原文）：
  - 牙①：`DEPS_RED RANGE_UNSATISFIED Alpha range=[1.0.0] resolved=9.9.9（resolved 9.9.9 != 1.0.0）`（rc=1）；
  - 牙②：`DEPS_RED FILE_SHA_MISMATCH Alpha/1.0.0 lib/net8.0/Alpha.dll expected=239f59ed… actual=2a8a2779…`（rc=1）；
  - 牙③：`DEPS_RED BAD_MANIFEST_VERSION manifest_version=None` + `DEPS_RED EMPTY_MANIFEST`；
    `DEPS_RED MISSING_FIELD Alpha.nupkg_sha512`（均 rc=1）。
  - 另两条 fail-closed：缺 manifest / 缺 lock ⇒ `MISSING_MANIFEST` / `MISSING_LOCK`（rc=1）。
  - 汇总：以机器产物为准 —— `E:/UserFleet/evidence-cardfix-20261119/deps-check-and-selftest.log`（`python -I -B tools/deploy/deps_manifest.py selftest` 的最后一行）。
- 锁文件不破坏构建（干净 clone）：`git clone --local` 后在分支 head 上 `dotnet build`
  DBSrv/LoginSrv/GameGate/LoginGate/SelGate **0 error**；`dotnet test TestProject1` →
  **通过 40 / 失败 0 / 跳过 0**。
- 工具门：登记 `deps_manifest` 后 **TOTAL 70 项：绿 69 / 红 0 / 需人工 1 / 跳过 0**；
  `--inventory` 清点 **75 = 70 + 5 + 0**；NEED=`oid_gate`（本检出无 collab 条目）。

### 与 botfleet 的边界（你出工具、它接）

- 挂点由 botfleet 决定：建议构建车道（部署前）跑 `generate`，目标机（部署后/起栈前）跑 `check`。
- 红格式示例：`DEPS_RED FILE_SHA_MISMATCH <pkg>/<ver> <file> expected=… actual=…`（rc=1）；
  判不了（缺目录等）rc=2，不静默通过。两边都不改对方仓库。

## QA 整改（2026-11-19）

> 来源：QA 签署摘要 `docs/issues/qa-merged-cards-signoff-20261119.md`。

> 统一口径：**卡面不再手抄测试数字**，一律引用机器产物路径；状态写"已合并（<sha>）"。

- 机器产物：`E:/UserFleet/evidence-cardfix-20261119/`（`packlen-test.log`、`utf8-test.log`、`deps-check-and-selftest.log`）；
- 实跑命令与 `git status --porcelain` 原文：见上述日志（本次采集时 `git status --porcelain` = 0 行）；
- **源码文本/`grep` 型断言**：仅作"接线提示"，**不作行为证据**（QA 章程 §12）；行为判据以
  实跑帧/JSON 产物为准。
- 可独立重放的篡改分组（`packlen T1–T4` / `utf8 M1–M3` / `deps T1–T5`）：**待 QA 提供定义后
  原样写入本卡**（本批未凭空补写）。
### 统一整改（C4–C6）

- **C4 `built_at`**：本族交付件的 `built_at` 口径**已由 `docs/issues/openmir2-stamp-built-at-single-source.md`
  承接**——戳的 `built_at` 必须等于二进制自报的 `built_at`（单一来源），mtime 只作元数据、不参与比对；
  M9 四件按 `CONDITIONAL`（身份可证、语义待统一）。
- **C5 `SHA256SUMS`**：冻结件采用**双清单口径**——`SHA256SUMS.txt`（历史格式，带 `N bytes`，不改写）+
  `SHA256SUMS.sha256`（`sha256sum -c` 兼容，`<sha>  <相对路径>`）；两者用途差异写在 `PROVENANCE.txt`。
- **C6 章程引用**：权威 QA 章程在 **`E:\OPC\docs\qa-charter.md`（§10–§12）**，不在本仓库；
  卡面凡引"章程 §N"均以此文件为准。
