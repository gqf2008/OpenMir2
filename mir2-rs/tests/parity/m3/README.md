# M3 对拍夹具（同库同脚本）

D2 线交付（对应设计文档 §6 M3「同序列对拍」）。owner 2026-10-10 决策：**本夹具即 M3 正式门禁**
（判据 = `run_pair.ps1` + `scoped_diff.py`，作用域内字段级 diff = 0，且 C# 基线一条命令可复跑）。
判据纪律照 §6.0：可测、可证伪、差异要么 0 要么进 `tests/parity/whitelist.md`、证据四件套。

## M3 正式门禁：一条命令

```powershell
# ① C# 基线（含"两次运行的确定性前缀 hash 相同"这条稳定性判据）
powershell -ExecutionPolicy Bypass -File mir2-rs/tests/parity/m3/run_m3_gate.ps1 -Side csharp -VerifyRepeat

# ② M1 顶住 7000/7100/7200 之后的 Rust 侧首验（C# 会话作参照 ⇒ 跨侧 diff 必须 0）
powershell -ExecutionPolicy Bypass -File mir2-rs/tests/parity/m3/run_m3_gate.ps1 -Side rust -PeerSession csharp-20261010-164731
```

### `-Side rust` 首验的两条前置（否则会把别人的问题读成 A2 的问题）

1. **A2（M1 边界服务）的 Rust 服务真的顶住 7000/7100/7200**——门禁的入口护栏会拦"入口还是 C#"，
   但拦不住"入口是 Rust 但链路半通"；
2. **A3 的 s2c 多段体帧编码器修复合入 master**（2026-10-10 状态：`worktree-c-tools` @ `02ba88ef`，
   needs-review）。未合入时经 Rust 链路的 `SM_TURN` 家族（`enc(CharDesc)+enc(文本)`）会跳帧，
   产生**假差异**——那属于编码器口径，不是 A2 的服务行为差异。
   相关口径差另见 whitelist **A-9**（C2 导出器对多段帧的 `body_len/body_sha256` 与分段解码不一致）：
   跨侧 diff 若消费 C2 的 JSONL，这类字段是"生产端口径"，别当行为差异。

两条都满足后再跑 ②，此时的 diff 才有仲裁力（否则结论要么恒绿、要么是他人缺陷的投影）。

`run_m3_gate.ps1` 依次做四件事，最后落 `sessions/M3-gate-<side>-<时间>.json`（含各步骤退出码与失败项）：

0. **入口实现识别护栏**（**快速失败**）：读 7000/7100/7200 的实际监听进程，
   `-Side rust` 时若入口仍是 `E:\MirServer` 的 C# 实现 ⇒ 立即 RED 退出
   ——否则就是"拿 C# 跟自己比"，门禁会恒绿。**实测**（2026-10-10，M1 未顶住端口时）：
   `RED: 入口护栏未通过，快速失败: rust_side_but_csharp_holds_entry`（退出码 1）。
1. `run_pair.ps1`：快照 before/after（13 表 sha256）→ 操作序列 → 作用域抽取 → 基线 json。
2. RNG 基线：`csharp` 侧跑 `rng-baseline.ps1`（种子注入 → 世界 RNG 流 + 掉落/经验序列落盘 + hash）；
   `rust` 侧重放 C# 记录流（`cargo test -p mir2-parity-tests --test rng_replay_parity`）。
3. 给了 `-PeerSession` 时做跨侧比对：`scoped_diff.py diff` **必须 0**。

首次 GREEN 报告（2026-10-10，`-Side csharp -VerifyRepeat`）：

```
GREEN: M3 门禁通过
  stable=True            prefix_sha256 == run2（两次运行取数 35139 / 36974 条，格式前缀 hash 相同）
  common_prefix_calls=1842   prefix_calls=1500（hash 只覆盖保证稳定的前 1500 次，留余量）
  drop_100kills_sha256=e40d78c0…  exp_table_sha256=5c1c45ce…（与 D 线入库 golden 逐位相同）
  pack_unchanged=True（E:\MirServer 5 个关键文件 sha256 前后一致）
```

## 一次「侧运行」是什么

```
预检（入口在听 / DB 可读 / 账号名长度合规）
  → 运行态记录（7 进程实际 exe 路径 + 启动时间 → baseline.json）
  → 全新前缀断言（该命名空间必须为空）
  → 快照 BEFORE（tools/dbsnap，13 张表逐表 sha256）
  → 操作序列（tools/botload 假人：注册 → 登录 → 建角 →（尽量）进世界）
  → 快照 AFTER
  → 作用域抽取（scoped_diff extract：只取本次前缀命名的行）
  → 改动清单（scoped_diff delta）
  → baseline.json（两侧快照 hash、作用域行数、改动清单、命令、git 提交）
```

单跑夹具（排查用；门禁请用 `run_m3_gate.ps1`）：

```powershell
powershell -ExecutionPolicy Bypass -File mir2-rs/tests/parity/m3/run_pair.ps1 -Side csharp -Bots 1
python mir2-rs/tests/parity/m3/scoped_diff.py diff <csharp会话>/scoped_after_norm.jsonl <rust会话>/scoped_after_norm.jsonl
```

## RNG 基线：一条命令（种子注入 → 序列落盘 + hash 稳定）

```powershell
powershell -ExecutionPolicy Bypass -File mir2-rs/tests/parity/m3/rng-baseline.ps1 -VerifyRepeat
```

- 影子副本建在**会话目录内**，改的是副本的 `runtimeconfig`/`Server.conf`；运行前后对比
  `E:\MirServer\M2GameSvr` 5 个关键文件 sha256（`pack_unchanged`），证明冻结基线未被本线改动。
- `-VerifyRepeat` 跑两次同种子运行并断言**确定性前缀 hash 相同**（当前 `b677fe32…`）。
- **hash 语义（重要）**：只对前 `PrefixCalls`（默认 1500）次取数取 hash——实测跨运行确定性区段
  ≈1828 次（见 `RNG同种子与记录回放方案.md` 的边界实测），留了余量。整条流的 hash 不可作判据
  （公共前缀之后受 NPC 遍历顺序/时钟门控影响）。
- 同一命令产出掉落/经验序列（`drop_100kills.txt` / `exp_table.txt`）及其 sha256。

已产出的基线（实测，2026-10-10）：`sessions/csharp-20261010-140632/baseline.json`
—— 13 表快照 hash 齐备，`runtime_procs`（产出该基线的进程表）齐备，
作用域内 4 行变更：`mir2_account.account +1`、`mir2_account.account_protection +1`、
`mir2_db.characters +1`、`mir2_db.characters_indexes +1`。
**同一命令连跑两次，改动清单的形状完全一致**（同 4 表各 +1，仅行内容里的自增 Id/时间戳不同）
——这是"夹具本身可复现"的证据。

产物与临时物的位置/去向登记见 `ARTIFACTS.md`（§14.3 收尾清单要求）。

## 为什么"同一初始状态"不靠破坏性还原

`mir2_db` 是**活库**（既有数千角色，且并行线也在跑假人）。做法是**每次运行用全新前缀**
（账号/角色名 = `<前缀><序号>`），并断言该前缀此前 0 行；于是：
- 不需要还原快照（不碰既有角色，风险为 0）；
- 比对只取本次命名空间内的行 ⇒ 并行线的干扰自动排除。

`scoped_diff.py` 提供 `extract / delta / diff / selftest`（selftest 含"改坏必红"两项）。
`--normalize` 把前缀归一化为 `<RUN>`，使两次运行的前缀差异不影响比对。

## 账号名 10 字符上限（踩过，务必先读）

协议层账号字段上限 **10 字符**，超长会被**静默截断**，表现为：
「帐号创建成功」→ 立刻登录「此帐号不存在」（截断后的名字与创建时不一致）。
夹具已内置护栏：`前缀长度 + 序号位数 > 10` 直接判红并退出。
现状默认前缀 `m3` + `HHMMSS`（8 字符），留 2 位给序号（≤ 99 只假人）。

## LoginSrv 的内存账号表（SQL 开号的坑）

`AccountStorage.Index()` 查的是**启动时加载的内存账号表**。因此：
- **推荐**：让假人走 `-NewAccount`（服务器自建号）——账号表在线更新，
  `account_protection` 行也由服务端一并写入，实测可直接登录；
- `-ProvisionSql`（SQL 直开号，`tools/client/account_provision.ps1`）**必须重启 LoginSrv**
  后才可登录；只适合"开号 → 重启 → 跑操作序列"的编排。

## 运行态 exe 归属（重启/改配置前必读）

`baseline.json` 的 `runtime_procs` 会记录产出基线的进程表（实测，2026-10-10）：

| 进程 | 实际 exe |
| --- | --- |
| mysqld | `D:/mysql/mariadb-10.11.19-winx64/bin/mysqld.exe` |
| DBSrv | `src/DBSrv/bin/Release/DBSrv.exe`（**仓库构建输出**） |
| LoginSrv | `src/LoginSrv/bin/Release/LoginSrv.exe`（**仓库构建输出**） |
| GameSvr | `E:/MirServer/M2GameSvr/GameSrv.exe` |
| LoginGate / SelGate / GameGate | `E:/MirServer/{LoginGate,SelGate,RunGate}/*.exe` |

⇒ 需要"整栈重启"时**以这张表为准**（重启错那份等于没重启）；
LoginSrv 读的配置是 `src/LoginSrv/bin/Release/logsrv.conf`
（`LogSrv.ini` / `config.conf` 是遗留副本，不读）。

## 已知阻塞（影响操作序列能覆盖到哪一步）

1. **BotSrv 进世界后崩溃**：`TMap.CanMove`（`src/BotSrv/Maps/TMap.cs:343`）抛异常并带走整个进程，
   崩溃点与时序相关（有时停在"确认游戏公告"）。⇒ 今日可用的操作序列 = **注册 → 登录 → 建角(→ 尽量进世界)**，
   判据取可靠段（`帐号创建成功`/`帐号登录成功` = N），`进世界` 只记录不判红。
   M3 要跑「走路/打怪/掉落/小退」必须**先修该崩溃**（工具侧，C 线领域）。
2. 世界侧的**非 RNG 熵源**已实测会扰动逐次一致（见 `RNG同种子与记录回放方案.md`）：
   同种子两次运行在第 1825 次取数处分岔（调用点从 `Merchant.Run` 变成 `GuildOfficial.Run`）。

## 红检（改坏必红）

```powershell
powershell -File run_pair.ps1 -SelfTestRed      # 入口不通 → 判失败（实测 exit 0 = 红检成立）
python scoped_diff.py selftest                 # 相同→绿、多一行→红（2/2 实测）
```

另外两条内置护栏本身就是门禁（实测触发过）：
账号名超 10 字符 → 红；前缀已存在同名账号/角色 → 红。

## 清理本次运行留下的账号/角色（不自动执行）

```sql
-- 先看命中范围
SELECT Id, Account FROM mir2_account.account WHERE Account LIKE 'm3%';
SELECT Id, ChrName FROM mir2_db.characters WHERE ChrName LIKE 'm3%';
-- 确认后再删（夹具不自动删：误删既有角色不可逆）
DELETE p FROM mir2_account.account_protection p JOIN mir2_account.account a ON p.AccountId=a.Id
  WHERE a.Account LIKE 'm3%';
DELETE FROM mir2_account.account WHERE Account LIKE 'm3%';
```
