# openmir2-deploy-deps-manifest

> 状态：**已登记，未建卡**（本仓库 remote 无 `refs/collab/*`，walgit 看板不可用；按 AGENTS.md §7 过渡）。
> 看板恢复后迁移为正式卡。tier 建议 **L**。

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
