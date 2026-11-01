# 行会窗夹具执行单（2026-11-01）

- 卡：`openmir2-guild-window-fixture`（tier=L，纯运维夹具 + 文档，无服务端行为改动）
- 交付：四个 PowerShell 脚本 + 本执行单
  - `tools/window/guild_fixture.ps1` — 预置一个「文件形态」行会（`GuildList.txt` + `Guilds/<名>.txt`）
  - `tools/window/guild_fixture_restore.ps1` — 恢复 `GuildList.txt` 原字节、删 `<名>.txt`、删空 `Guilds/`
  - `tools/window/guild_fixture_suspend.ps1` — **B 失败形态**：把 `Guilds/<名>.txt` 改名成 `.suspended`
  - `tools/window/guild_fixture_resume.ps1` — 把 `.suspended` 改回并断言 sha 还原
- 三轮顺序：**① 成功对照 → ② B 失败形态（必跑）→ ③ A 可选（游戏内建第二行会）**

> 本卡**不执行**任何对 `E:/MirServer` 的写入；脚本自测全部在系统 tempdir。真机执行由 Leader 在窗口内完成。
> 脚本对本卡外的真实根 fail-closed：`-Selftest` 下命中 `E:\MirServer` 及子孙无条件拒绝；非 Selftest 必须显式 `-Force`。

---

## 1. Oracle 事实（脚本按此构造，勿改）

| 事实 | 证据 |
|---|---|
| 行会不落 MySQL，落 `GuildBase/` 文件 | `mir2-rs/tests/parity/m3/guild_castle_scenario.json`（"sql/ 无 guild/guildmember/castle 表"） |
| `GuildDir=GuildBase/Guilds`、`GuildFile=Guildbase/Guildlist.txt`（相对工作目录） | `E:/MirServer/M2GameSvr/Server.conf:61-62`；Rust 读同键：`mir2-rs/crates/data/src/guild_castle.rs`（`[Share] GuildDir/GuildFile`） |
| 文本一律 **GBK(cp936) + CRLF** | `guild.rs` 模块头 ⑥；C# `StringList.SaveToFile` |
| `<名>.txt` 四段头 + 段间单空格行 + 成员段 `#<RankNo> <RankName>` / `+<成员>` | `GuildInfo.cs:384-431`（`SaveGuildFile`）；`guild_store_vectors.rs:319` |
| 掌门默认 `RankNo=1` / `RankName=掌门人` | `GuildInfo.cs:468-472`（`SetGuildInfo`） |
| **坑 1**：`SaveToFile` 用 `FileMode.Open` ⇒ 目标 `<名>.txt` 不存在时静默失败，被 `try/catch` 吞掉，只记 `ERR 保存行会信息失败!!!` | `GuildInfo.cs:425-428`；Rust 对应 `guild_cmd.rs:361-374`、`lib.rs:18618` |
| **坑 2**：`<名>.ini` 只在 `GuildConf` ctor 被建成空文件，五键永不落盘 | `guild.rs` 模块头 ② |
| 装载时机：服务端启动 `LoadGuildInfo/LoadGuild`；改文件后必须**重启 game-svr** | `GuildManager.cs:78-110`；Rust `guild_runtime.rs:131` |
| 当前生产现状（2026-11-01） | `E:/MirServer/M2GameSvr/GuildBase/GuildList.txt` = **0 字节**（sha256 `e3b0c442…b855`），`Guilds/` 目录**不存在** |

> 因 `GuildList.txt` 存在但为 0 字节，预置时 `BEFORE_SHA` 是空文件的 sha（`e3b0c442…`），**不是** `ABSENT`。
> 只有在文件确实不存在（例如显式删除后）时才是 `ABSENT`。

### 1.1 已知实现事实（会影响第 ③ 步的操作选择）

- **Rust 栈 `CM_GUILDADDMEMBER(1038)` 生产恒返回 `758/5`**：`allow_guild` 唯一生产写点是 GM `@Letguild`，本仓未接 ⇒ 恒 `false`
  （`mir2-rs/bins/game-svr/src/guild_cmd.rs:30-33`，`:553-556` 的注释「生产恒走这支」）。
  ⇒ **在 Rust 栈上「加成员」无法成功**，不能拿它当保存触发器。
- **C# 栈**可以用 GM `@Letguild` 打开 `AllowGuild`（`LetGuildCommand.cs:12`），再走 1038；但 GM 权限本身可能是 0
  （见 `docs` 中 GM 权限 oracle 事实），是否可用由对方在窗口内确认。
- **不依赖 GM、两栈都可达的保存触发器：`CM_GUILDUPDATENOTICE(1040)`（掌门 RankNo=1 改公告）**
  —— 直接调 `SaveGuildInfoFile()`（`guild_cmd.rs:724-741`），落盘路径与加成员完全相同；
  且 753 面板会回显公告（可判据）。**建议本轮优先用 1040**；「加成员」作为对方侧可用时的等价触发。
- 加成员成功时**不打印成功日志**（只有失败打印 `ERR 保存行会信息失败!!!`，`guild_cmd.rs:361-374`）；
  成功旁证是 `INF 行会断线 check_save：Ok(…) ChrName=<角色>`（`lib.rs:18618`，断线路径）。
  因此 **① 的主判据是「磁盘字节 + 753/756 回读」**，INF 行只作旁证（窗口内若没有断线可以不出现）。

---

## 2. 窗口前置与纪律

1. **零客户端**：停止所有客户端 / 假人 / 探针；确认目标栈端口全空（窗口端口按本轮 profile，不是 `5000–7200`）。
   `E:/MirServer/port-status.ps1` 里 **3306 必须 UP**（mysqld 不归本窗口管，**不要停**）。
2. **先停 game-svr 再改文件**：运行中的栈会在运行期 `SaveGuildList/SaveGuildFile` 覆盖夹具写入。
   起停命令按本轮窗口 runbook（C# 参考：`tools/window/stop-csharp-graceful.ps1` / `tools/window/start-csharp.ps1`；
   Rust 参考：本轮 profile 的 start 脚本或 `tools/window/start-stack-wmi.ps1`）。
3. **备份目录**：放在仓库外、生产根外，并在收尾报告登记残留物。建议：
   `E:/tmp/guild-window-20261101/backup`（fixture/restore 用）与
   `E:/tmp/guild-window-20261101/suspend`（suspend/resume 用）。**两个 state 文件不要混用目录**。
4. **部署根 `-Root`**：包含 `GuildBase/` 的目录。C# 默认是 `E:/MirServer/M2GameSvr`（`Server.conf` 同目录）。
   Rust 栈同布局；若 `[Share] GuildDir/GuildFile` 被改过，`-Root` 要指向实际工作目录。
5. **所有真机写入必须带 `-Force`**（不带会被脚本拒绝，rc=2）。先跑一遍 `-WhatIf` 看计划。
6. **二进制/装载确认（loaded/stamp）**：
   - Rust：运行中二进制的 `mir2-server.build.json` 的 `head` 必须等于本轮窗口钉住的 head；
     启动日志**不得**出现 `ERR 行会加载失败：… ⇒ 会表为空`（`guild_runtime.rs:138`）。
   - C#：启动日志应出现 `已读取 [N] 个行会信息...`（`GuildManager.cs:104`；预置成功后 N 含 `e2e-guild`），
     且**不得**出现 `行会信息文件未找到!!!`。
   - 端口回落/延迟就绪属正常；以脚本/探针的「端口就绪」等待为准，不要用固定 sleep 判失败。
7. **取证目录**：每轮单独目录（如 `E:/tmp/guild-window-20261101/round1/`），放脚本输出、日志切片、frames.ndjson、sha 清单。
8. **窗口期禁止 cargo 构建**（会抢内存/干扰真机）；窗口前先确认没有构建在跑。

---

## 3. 脚本速查

### 3.1 `guild_fixture.ps1`（预置）

```powershell
pwsh -NoProfile -File tools/window/guild_fixture.ps1 -Root <部署根> -GuildName e2e-guild -Member <角色名> `
     -BackupDir E:/tmp/guild-window-20261101/backup -Force
# 只打印不写：
pwsh -NoProfile -File tools/window/guild_fixture.ps1 -Root <部署根> -GuildName e2e-guild -Member <角色名> -BackupDir ... -WhatIf
# 自测（tempdir）：
pwsh -NoProfile -File tools/window/guild_fixture.ps1 -Selftest
```

行为：登记 `GuildList.txt` 原字节+sha → 备份到 `-BackupDir` → 建 `GuildBase/Guilds/` → 写 `Guilds/<名>.txt`
（GBK+CRLF：`公告 / 空格行 / 敌对行会 / 空格行 / 联盟行会 / 空格行 / 行会成员 / #1 掌门人 / +<角色名>`）→
在 `GuildList.txt` 追加 `<名>`+CRLF（内存一次构造，避免半写）→ 打印 before/after sha。
**fail-closed**：目标 `<名>.txt`/`<名>.ini` 已存在、名单已有同名、参数含路径分隔符/空格/逗号 ⇒ rc=2。
`-WhatIf` 零写（连备份目录都不建）。

### 3.2 `guild_fixture_restore.ps1`（恢复）

```powershell
pwsh -NoProfile -File tools/window/guild_fixture_restore.ps1 -Root <部署根> -GuildName e2e-guild `
     -BackupDir E:/tmp/guild-window-20261101/backup -Force
pwsh -NoProfile -File tools/window/guild_fixture_restore.ps1 -Selftest
```

行为：读同一 `-BackupDir/guild-fixture-state.json` → 删 `Guilds/<名>.txt`（以及服务端装载时生成的 `<名>.ini`，仅当预置前不存在）→
`GuildList.txt` 原字节拷回（原 `ABSENT` 则删除）→ **断言 `restored_sha == before_sha`**（备份先自校验 sha；不符即 rc=2）→
删空 `Guilds/`（仅当预置前不存在）与空 `GuildBase/`（仅当预置前不存在）。
输出：`RESTORED_SHA / BEFORE_SHA / MATCH / DELETED …`。

### 3.3 `guild_fixture_suspend.ps1` / `guild_fixture_resume.ps1`（B 失败形态）

```powershell
# 制造 B（文件不存在）：改名为 <名>.txt.suspended
pwsh -NoProfile -File tools/window/guild_fixture_suspend.ps1 -Root <部署根> -GuildName e2e-guild `
     -BackupDir E:/tmp/guild-window-20261101/suspend -Force
# 归还
pwsh -NoProfile -File tools/window/guild_fixture_resume.ps1 -Root <部署根> -GuildName e2e-guild `
     -BackupDir E:/tmp/guild-window-20261101/suspend -Force
pwsh -NoProfile -File tools/window/guild_fixture_suspend.ps1 -Selftest
pwsh -NoProfile -File tools/window/guild_fixture_resume.ps1 -Selftest
```

suspend：记录 `before_sha` → 改名 `<名>.txt` → `<名>.txt.suspended` → 打印 `BEFORE_SHA`/`SUSPENDED_SHA`（必须相等）。
resume：读 state → 断言 suspended 的 sha == `before_sha` → 断言目标 `<名>.txt` 不存在（存在则拒绝覆盖）→ 改回 →
打印 `RESTORED_SHA`/`MATCH`。两者都 fail-closed。

---

## 4. 轮 ①：成功对照（必跑）

**目标**：证明「预置的 `<名>.txt` 存在 ⇒ 保存成功」，并把改动持久化到文件、重启后可回读（作为 ② 的基线）。

> 触发操作二选一（见 §1.1）：
> - **推荐（两栈可达）**：掌门改公告 `CM_GUILDUPDATENOTICE(1040)`，如把公告改为 `round1-notice`。
> - 对方侧可用 GM 时：`CM_GUILDADDMEMBER(1038)` 加一个成员（C# 需先 `@Letguild`；Rust 恒 758/5 不可用）。
> 两种操作最终都走 `SaveGuildInfoFile()`，落盘路径相同。

步骤：

1. 停 game-svr（记录起停命令与时间）。
2. `-WhatIf` 预览，再真跑 fixture：
   ```powershell
   pwsh -NoProfile -File tools/window/guild_fixture.ps1 -Root E:/MirServer/M2GameSvr -GuildName e2e-guild `
        -Member <掌门角色名> -BackupDir E:/tmp/guild-window-20261101/backup -Force
   ```
   记录：`BEFORE_SHA`（现网应为空文件 sha `e3b0c442…`）、`AFTER_SHA`、`STATE_FILE`、`LIST_FILE`、`GUILD_FILE`。
3. 落盘核对（只读）：`GuildBase/GuildList.txt` 含 `e2e-guild`（GBK+CRLF）；
   `Guilds/e2e-guild.txt` 字节 = 四段头 + `#1 掌门人` + `+<掌门角色名>`，无 UTF-8 BOM、无裸 LF。
   留 `Get-FileHash -Algorithm SHA256`（用 `pwsh`；Windows PowerShell 5.1 本机可能没有该 cmdlet）与 hex 首尾。
4. 启动 game-svr，按 §2.6 确认 loaded/stamp；确认无 `行会加载失败` / `行会信息文件未找到!!!`。
5. 对方在窗口内执行触发操作（1040 改公告 或 1038 加成员）；记录角色名/新值。
6. **重启前**取证：
   - 客户端帧：`753`（面板，含新公告/新成员）、`756`（成员列表）；
     —— 加成员路径另有 `757/758`（成功/失败码）。
   - 磁盘：立即读 `Guilds/e2e-guild.txt`（加成员/改公告是**当场直写**，`guild_cmd.rs:734-735`、`:578-579`），
     确认新值已落盘；留文件 sha。
   - 日志：无 `ERR 保存行会信息失败!!!`；若有断线，允许出现 `INF 行会断线 check_save：Ok(…) ChrName=…`（旁证）。
7. **停 game-svr → 重启 game-svr**。
8. **重启后**取证：
   - `753/756` 必须**回读到改动后的值**（新公告/新成员仍在）——这是轮 ① 的主判据。
   - 磁盘 `<名>.txt` 仍含新值（重启前后 sha 一致或仅 tick 无关差异）。
9. 通过判据（全部满足）：
   - 重启前 753/756 可见改动；重启后 753/756 回读同一改动；
   - 磁盘 `<名>.txt` 在重启前已含改动（证明保存成功）；
   - 本轮无 `ERR 保存行会信息失败!!!`（有则说明预置/权限/路径有问题，**停轮排查**）。

---

## 5. 轮 ②：B 失败形态（必跑）

**目标**：证明「`<名>.txt` 不存在 ⇒ 保存静默失败（只记 ERR）」且重启后回退到预置值。

1. 在轮 ① 结束、game-svr **已停止**的前提下（或先停服），制造 B：
   ```powershell
   pwsh -NoProfile -File tools/window/guild_fixture_suspend.ps1 -Root E:/MirServer/M2GameSvr -GuildName e2e-guild `
        -BackupDir E:/tmp/guild-window-20261101/suspend -Force
   ```
   记录 `BEFORE_SHA` 与 `SUSPENDED_SHA`（**必须相等**，证明只改名不改内容）；确认 `Guilds/` 里只剩 `e2e-guild.txt.suspended`。
2. 启动 game-svr，确认装载正常（B 形态下 `<名>.txt` 缺失，行会会**装载失败并被移除**——
   这是 oracle 行为；若本轮观察点需要行会在内存中，见下方「注意」）。
3. 对方执行**同一种**触发操作（1040 或 1038）。
4. 观察并取证：
   - **日志恰好 1 条 `ERR 保存行会信息失败!!!`**（C# `GuildInfo.cs:427`；Rust `guild_cmd.rs:372` / `lib.rs:18618`），
     来自这次触发操作；**没有**对应的成功保存。
   - 客户端帧：加成员路径的 `757` 不会出现（应为 `758`）；1040 路径仍会回 `753`（面板），但磁盘未变。
   - 磁盘：`Guilds/e2e-guild.txt` **仍不存在**（只有 `.suspended`），`GuildList.txt` 未变。
5. **尽快**归还文件，限制后续自动保存把 ERR 数放大：
   ```powershell
   pwsh -NoProfile -File tools/window/guild_fixture_resume.ps1 -Root E:/MirServer/M2GameSvr -GuildName e2e-guild `
        -BackupDir E:/tmp/guild-window-20261101/suspend -Force
   ```
   断言 `RESTORED_SHA == BEFORE_SHA`（即预置原值）且 `MATCH true`。
6. 停 game-svr → 重启 → 取 `753/756`：成员/公告**回退到预置原值**（加成员的那个成员不在；公告回到预置公告）。
7. 通过判据：日志 1 条 ERR 且无成功行；磁盘在 B 期间未产生 `<名>.txt`；归还后 sha 还原；重启后回读为预置原值。

> **注意**：B 形态下 `<名>.txt` 缺失，`LoadGuildInfo` 会加载失败并 `RemoveAt` + `SaveGuildList()`
> （`GuildManager.cs:96-103`），即**行会可能根本不在内存**，此时 1038/1040 可能因「找不到行会」提前返回而不触发保存。
> 若窗口观察到这种情况，B 的正确形态应是「**运行中改名**」：先按轮 ① 正常启动并装载行会，再在执行触发操作**之前**
> 于服务器运行时刻改名（文件缺失），这样内存里仍有行会、保存才会走到 `FileMode.Open` 失败。**推荐用运行中改名**：
> 停 → 起 → 确认行会已装载 → suspend → 触发 → 观察 1 条 ERR → resume → 重启看回退。

---

## 6. 轮 ③：A 可选（游戏内建第二个行会）

- 前提：需要 NPC 等级/金币等游戏内前置；由 **Leader 决定是否做**，并在窗口前用 DB/脚本预置前置。
- **本卡脚本不实现**这一轮（不写 DB、不建会）。
- 执行单口径：若窗口时间允许，对方在游戏内建第二个行会，我方重启后确认其落盘（`GuildList.txt` 多一行 + 新 `<名>.txt`），
  并与轮 ① 的预置行会并存；否则记为「本轮跳过（Leader 决定）」，不影响 ①② 的验收。

---

## 7. 收尾：restore 与证明

顺序：**停 game-svr → restore → 启动 game-svr → 确认空态 → 端口就绪**。

1. 停 game-svr（与开窗同一套 stop 脚本，确认进程已退出）。
2. 执行恢复：
   ```powershell
   pwsh -NoProfile -File tools/window/guild_fixture_restore.ps1 -Root E:/MirServer/M2GameSvr -GuildName e2e-guild `
        -BackupDir E:/tmp/guild-window-20261101/backup -Force
   ```
   必须看到：
   - `RESTORED_SHA <before_sha>`、`BEFORE_SHA <before_sha>`、`MATCH true`（**`restored_sha == before_sha`** 是硬断言）；
   - `DELETED …/Guilds/e2e-guild.txt`、`DELETED …/Guilds`（空目录）；
   - 现网基线：`GuildList.txt` 回到 **0 字节**（sha256 `e3b0c442…b855`），`Guilds/` 不存在。
3. 启动 game-svr，确认：
   - 装载日志无 `行会信息文件未找到!!!`（C# 空名单是正常的：`已读取 [0] 个行会信息...`）；
     无 `行会加载失败`（Rust）。
   - `GuildList.txt` 仍为 0 字节 / `Guilds/` 已删（重启不会重建夹具内容）。
   - 端口就绪（本轮 profile 口径）；`E:/MirServer/port-status.ps1` 十端口与开窗基线一致（3306 UP）。
4. 若中途异常中断（suspend 后没 resume、fixture 后没 restore）：按 `suspend`→`resume`、`fixture`→`restore` 的顺序补做；
   state 文件丢了就手工恢复，但**必须**用 `-WhatIf` 先看计划并用 sha 核对（不得凭记忆覆盖）。
5. 残留物登记：`E:/tmp/guild-window-20261101/`（backup/suspend/state/取证）在报告末段列明，窗口结束清理或注明保留原因。

---

## 8. 每轮证据取样清单

| 面 | 取什么 | 怎么取 |
|---|---|---|
| 客户端帧 | `753`（面板）、`756`（成员列表）；加成员另取 `757/758` | 从本轮 `frames.ndjson` 按 `ident` 过滤，保留时间序（**以帧文件为准**，不以源码顺序推断） |
| 服务端日志 | `ERR 保存行会信息失败!!!`、`INF 行会断线 check_save：`、`行会加载失败`、`已读取 [N] 个行会信息` | `Select-String` / `grep -E` 日志切片，保留上下文 3 行 |
| 文件 sha | `GuildList.txt`、`Guilds/e2e-guild.txt`、`.suspended` | `pwsh` 下 `Get-FileHash -Algorithm SHA256`；或脚本打印的 `BEFORE_SHA/AFTER_SHA/RESTORED_SHA/SUSPENDED_SHA` |
| 文件字节 | 四段头/GBK/CRLF/无 BOM | hexdump 首尾 + GBK 解码（脚本自测 T4 已内置同款断言） |
| 脚本输出 | 四个脚本的完整 stdout + rc | 每轮重定向到 `roundN/<script>.log`；rc 单独记录 |

每轮日志建议的 grep：

```powershell
Select-String -Path <game-svr.log> -Pattern '保存行会信息失败|行会断线 check_save|行会加载失败|已读取 \[' |
  ForEach-Object { $_.Line }
```

---

## 9. 自测记录（本卡交付时实跑，均在 tempdir）

```
pwsh -NoProfile -File tools/window/guild_fixture.ps1         -Selftest   # rc=0, SELFTEST PASS
pwsh -NoProfile -File tools/window/guild_fixture_restore.ps1 -Selftest   # rc=0, SELFTEST PASS
pwsh -NoProfile -File tools/window/guild_fixture_suspend.ps1 -Selftest   # rc=0, SELFTEST PASS
pwsh -NoProfile -File tools/window/guild_fixture_resume.ps1  -Selftest   # rc=0, SELFTEST PASS
```

覆盖：prep→restore 往返字节一致（含原文件带/不带尾 CRLF 两种）、目标已存在拒绝、GBK+CRLF+无 BOM 字节断言、
`-WhatIf` 零写、suspend→resume sha 还原、篡改 `.suspended`/备份拒绝、真实根护栏；
每个自测前后对 `E:/MirServer/M2GameSvr/GuildBase` 做只读树摘要并断言一致（零写入证明）。

CLI 护栏实测（不写任何文件，rc=2）：

```
guild_fixture.ps1 -Selftest -Root E:/MirServer                          → RED: Selftest 禁止使用真实部署根 …
guild_fixture.ps1         -Root E:/MirServer/M2GameSvr …（无 -Force）    → RED: 根命中受保护的真实部署根 …
guild_fixture_restore.ps1 / _suspend / _resume 同上                     → RED: 根命中受保护的真实部署根 …
```

预置产物字节样例（`-Member 测试角色`，sha256 `282b71a4…4fd8`，9 个 CRLF / 0 个裸 LF / 无 BOM）：

```
b9 ab b8 e6 0d 0a 20 0d 0a  b5 d0 b6 d4 d0 d0 bb e1 0d 0a 20 0d 0a
c1 aa c3 cb d0 d0 bb e1 0d 0a 20 0d 0a  d0 d0 bb e1 b3 c9 d4 b1 0d 0a
23 31 20 d5 c6 c3 c5 c8 cb 0d 0a  2b b2 e2 ca d4 bd c7 c9 ab 0d 0a
```

（`公告<CRLF> <CRLF>敌对行会<CRLF> <CRLF>联盟行会<CRLF> <CRLF>行会成员<CRLF>#1 掌门人<CRLF>+测试角色<CRLF>`）

---

## 10. 风险与待确认

1. **Rust 栈加成员恒 758/5**（§1.1）：若窗口跑 Rust，轮 ①/② 的触发操作请改用 `1040 改公告`（或先用合法路径打开 `allow_guild`，
   但那需要偏差/新卡）。本执行单已给出 1040 口径，判据不变（都是 `SaveGuildInfoFile`）。
2. **B 形态的装载时机**（§5 注意）：运行中改名 vs 停服改名会影响行会是否在内存；推荐运行中改名。
3. **成功日志**：加成员/改公告成功不打印成功行；`INF 行会断线 check_save：Ok(…)` 只在断线路径出现，只作旁证。
   主判据始终是「磁盘字节 + 753/756 回读」。
4. **`GuildList.txt` 不存在 vs 0 字节**：现网是 0 字节，`BEFORE_SHA = e3b0c442…`；不要把它误记为 `ABSENT`。
5. 脚本以 UTF-8 no BOM 入仓（仓库惯例）；功能字符串（四段头/掌门人）用 Unicode 码点构造，
   因此即使被 Windows PowerShell 5.1 按 ANSI 解析，写出的 GBK 字节仍正确。**建议用 `pwsh` 执行**以获得正确的中文日志输出。
