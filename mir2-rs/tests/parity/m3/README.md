# M3 对拍夹具（同库同脚本）

D2 线交付（对应设计文档 §6 M3「同序列对拍」的准备项）。判据纪律照 §6.0：
可测、可证伪、差异要么 0 要么进 `tests/parity/whitelist.md`、证据四件套。

## 一次「侧运行」是什么

```
预检（入口在听 / DB 可读 / 账号名长度合规）
  → 全新前缀断言（该命名空间必须为空）
  → 快照 BEFORE（tools/dbsnap，13 张表逐表 sha256）
  → 操作序列（tools/botload 假人：注册 → 登录 → 建角 →（尽量）进世界）
  → 快照 AFTER
  → 作用域抽取（scoped_diff extract：只取本次前缀命名的行）
  → 改动清单（scoped_diff delta）
  → baseline.json（两侧快照 hash、作用域行数、改动清单、命令、git 提交）
```

命令：

```powershell
# C# 基线（当前可跑通的最小操作序列）
powershell -ExecutionPolicy Bypass -File mir2-rs/tests/parity/m3/run_pair.ps1 -Side csharp -Bots 1

# Rust 侧（把 7000/7100/7200 指向 Rust 实现后，同一条命令）
powershell -ExecutionPolicy Bypass -File mir2-rs/tests/parity/m3/run_pair.ps1 -Side rust -Bots 1

# M3 判据：作用域内字段级 diff 必须为 0
python mir2-rs/tests/parity/m3/scoped_diff.py diff \
  <csharp会话>/scoped_after_norm.jsonl <rust会话>/scoped_after_norm.jsonl
```

已产出的基线（实测，2026-10-10）：`sessions/csharp-20261010-135350/baseline.json`
—— 13 表快照 hash 齐备，作用域内 4 行变更：
`mir2_account.account +1`、`mir2_account.account_protection +1`、
`mir2_db.characters +1`、`mir2_db.characters_indexes +1`。

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
