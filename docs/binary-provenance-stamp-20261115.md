# 二进制构建戳（`mir2-build-stamp/1`）：先产戳、后通电

- 卡：`openmir2-csharp-stack-binary-provenance`（tier=L）
- 前序：`docs/loadgate-criteria-runbook-20261115.md`「栈配置」与「preflight 分开记 CODE_HEAD 与 REPO_HEAD」
- 目的：让起栈日志能回答「这套二进制来自哪个提交」，并在来源不可判时**中止**而不是记录后继续跑。

## 1. 现状（2026-11-15 实测）

| 二进制 | 是否有戳 | 说明 |
| --- | --- | --- |
| `mir2-server.exe` | **有** | `mir2-rs/target/release/mir2-server.build.json`，`format=mir2-build-stamp/1`，由 `tools/window/rebuild-server.ps1:89` 生成；现存戳 `head=d5ed14508…`、`built_at=2026-11-02`（已陈旧） |
| `game-svr` / `game-gate` / `login-gate` / `sel-gate` | **无** | bot 窗口实际拉起的就是这四个（360s 归档 `start-stack.out.log` 实测）；目前只能 `mtime+sha` 人工登记 |
| `DBSrv` / `LoginSrv`（C#） | **无** | 同上 |

反例：`E:/MirServer/M2GameSvr/GameSrv.exe` sha256 `ab655008…`、mtime 2026-10-22，异常栈 PDB 指向**已删除**的 worktree `OpenMir2-wt-goods` ⇒ 部署目录里的二进制无法自证来源。

## 2. 戳的内容（复用既有格式，不另造）

每条戳至少含：`exe`/`path`、`sha256`、`mtime`、`head`（提交号）、`head_subject`、`built_at`、`dirty`、`cargo`/`rustc` 版本。
**`sha256` 必须由写戳的一方自己从二进制文件算**（不得抄报告、不得抄上一次的戳）。已验证可行的模式：360s 窗口的
`SELFTEST_BINARY sha256=36726fcd…` / `SELFTEST_DLL sha256=8d09c454…`——写者自算并绑定。

## 3. 通电条件（五项全过才接 `provenance=unknown ⇒ rc=2`）

1. **它拉起的每一个二进制都有戳**（`game-svr`、`game-gate`、`login-gate`、`sel-gate`；C# 侧若也拉起则一并写）；
2. 戳内 `sha256` 与文件**实时哈希一致**；
3. 戳内 `head` 能 `git cat-file -e` 解析到**真实提交**；
4. `built_at` 与二进制 `mtime` 差 **≤5 分钟**（防「先构建后覆盖」）；
5. **健康栈自检整轮 PASS**（拿一个真实构建目录跑一遍 provenance 检查，证明不会误杀健康轮次）。

## 4. 牙（逐条变异必红）

| 变异 | 期望 |
| --- | --- |
| 删任一戳字段 | 红 |
| 篡改戳内 `sha256` | 红（与实时哈希不符） |
| `head` 改成不存在的提交 | 红（`cat-file -e` 失败） |
| `built_at` 与 mtime 差 >5 分钟 | 红 |
| 健康目录（正常构建） | **绿**（不得误杀） |
| 空目录 / 无二进制 | 明确报错，不得静默通过 |

## 5. 与 preflight 的关系

戳落地后，`preflight.txt` 的 `CODE_HEAD` 直接读戳，不再依赖「构建时工作树 HEAD」的推断；
`REPO_HEAD` 仍单列（可含其后纯文档提交，写明数量）。
