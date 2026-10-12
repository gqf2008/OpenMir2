# M3②③ 真机同种子攻击对拍 —— 夹具阻塞与 finding（2026-10-12，m3cs / worker-5）

**结论：本 tick 的真机对拍被 **F-A** 阻塞，未产出 `attack_diff` 判据。** 夹具
`mir2-rs/tests/parity/m3/gate_guards.ps1` 的攻击 pin SQL 把 `IncHealth/IncSpell`
挂到了**没有这两列**的 `characters_ablity` 上 ⇒ `run_pair.ps1` 在**第一个**攻击 pin 语句
就 `throw`（MariaDB `ERROR 1054`），C# 半在「预热建角 + SQL pin」相即中止，两侧序列从未跑起来。

本目录是 finding 的**可复现原始件**（另见线程 comment / patch body 的行号与命令）。仪器
`gate_guards.ps1` / `run_pair.ps1` / `run_attack_pair.ps1` / `attack_diff.py` 属 **m3bag 写集**
（brief 10:20 owner 口径：m3cs 只读不写）⇒ **本片不改仪器**，只把「位置 + 可判红/绿 + 修法」
交回，owner 转 m3bag。

---

## F-A（Important，夹具缺陷 —— 真机对拍硬阻塞）

**位置**：`mir2-rs/tests/parity/m3/gate_guards.ps1:533`（函数 `Get-M3AttackPinSql`，声明在 `:518`；
本片基线 blob `3587f895204c949273a078d19fe21541821085a5`）。

```text
UPDATE mir2_db.characters_ablity a JOIN mir2_db.characters c ON a.PlayerId=c.Id
  SET a.Level={0}, a.Hp={1}, a.MaxHP={1}, a.Mp=15, a.MAxMP=15, a.IncHealth=0, a.IncSpell=0
  WHERE c.ChrName='{2}'
```

`IncHealth` / `IncSpell` / `IncHealing` 是 **`characters` 表的列**，不是 `characters_ablity`
的列（见 `02-schema.txt`；`sql/mir2_db.sql:47-49` 在 `characters` 定义内，`characters_ablity`
定义 `:6073` 的 20 列里没有它们）。⇒ 别名 `a` 上取不到该列，MariaDB 在 **prepare 期**报
`ERROR 1054 (42S22): Unknown column 'a.IncHealth' in 'SET'`。

**实跑触发链**（`01-failing-run.txt`，C# 栈带 `m3/rng-hook`、`MIR2_RNG_SEED=42`）：

```text
SESSION .../sessions/csharp-20261012-114147
PREFLIGHT 入口 127.0.0.1:7000 在听
... PREWARM run_bots ... GREEN: 2/2 登录成功 (100%)；char_created=2/2
UPDATE mir2_db.characters_ablity a JOIN ... SET ..., a.IncHealth=0, a.IncSpell=0 WHERE c.ChrName='m3a1141440'
ERROR 1054 (42S22) at line 1: Unknown column 'a.IncHealth' in 'SET'
mysql 执行失败: UPDATE ... （run_pair.ps1:203 throw）
RED: run_pair（csharp 侧）未通过（rc=1）
```

**修法（唯一改动，一行）**：`a.IncHealth=0, a.IncSpell=0` ⇒ **`c.IncHealth=0, c.IncSpell=0`**。

**判红/判绿（实跑，见 `03-repro-sql.txt`）**：

| 变异 | 结果 |
| --- | --- |
| `a.IncHealth=0, a.IncSpell=0`（原样，WHERE 用不存在角色名） | `ERROR 1054`，mysql rc=1 ⇒ **必红** |
| `c.IncHealth=0, c.IncSpell=0`（修正，同 WHERE） | 无错误、0 行改动，mysql rc=0 ⇒ **绿** |

> 两个 UPDATE 的 `WHERE` 都用**不存在**的角色名 `__m3cs_no_such_char__` ⇒ 0 行被改，
> **未触碰任何真实角色/内容**（只验证列解析与语法）。

**oracle 锚点**（`Inc*` 池属于角色、不属于 Abil）：

- `src/M2Server/Player/PlayObject.Base.cs:2073-2076` —— `if (stdItem.AC > 0) IncHealth += stdItem.AC;`
  （`:2075` 为累加行；同族 `IncSpell` 在 `:2079`）。
- `src/M2Server/Player/PlayObject.Message.cs:70`（周期 tick 触发门 `!Death && (IncSpell>0||IncHealth>0||IncHealing>0)`）
  与 `:127-135`（`HP==MaxHP ⇒ IncHealth=0, IncHealing=0`；`MP==MaxMP ⇒ IncSpell=0`）。
- `src/Storeages/DBSvr.Storage.MySQL/PlayDataStorage.Save.cs:86`（SQL 列清单含 `IncHealth/IncSpell`）
  与 `:112-114`（`AddWithValue("@IncHealth", hd.IncHealth)` 等）——落库的是**角色**行。
- 与既有 T-14 结论一致：唯一会漂移的列是 `characters.IncHealth`（`scoped_diff.py::TABLE_COLUMNS`
  第 22 列）；T-28 修的就是这一列的周期消费。

## F-B（Minor，自检缺乏「SQL 可执行」牙齿 —— 本缺陷能穿过 78/638 门禁的原因）

`run_attack_pair.ps1 -SelfCheckNoStack` ⑥（`run_attack_pair.ps1` 内，未改）对攻击 pin SQL 只做
**字符串匹配**（`$joined -match 'a\.Hp=100000, a\.MaxHP=100000'` 等），**从不执行**该 SQL ⇒
列名挂错表这种错误永远判不出来（本片 gate 全绿、自检全绿，而真机一跑即 1054）。

**建议**（m3bag）：⑥ 增加一条**可执行性极小门**——把 `attack_pin_sql_all` 的每条 `UPDATE`
以 `WHERE` 指向一个不存在角色名执行（或 `EXPLAIN`），断言 `mysql` rc=0；把 `a.IncHealth`
改回 ⇒ 该门必红（就是本片 `03-repro-sql.txt` 的第 ③ 节）。

## F-C（Minor，operability —— Rust 半的「同一种子」无机器护栏）

brief 步骤 2 写「起 `start_rust_stack.ps1` ⇒ `-Side rust`（**同一种子**）」，但
`tools/m2-acceptance/start_rust_stack.ps1` **没有任何 `MIR2_RNG_SEED` 管线**（该文件与
`run_acceptance.ps1` 里 grep 不到 `SEED`）；它用 `Start-Process` 起 `game-svr.exe`，只**继承**
调用方进程环境。⇒ 只有 operator **手工**在起栈那个 shell 里 `$env:MIR2_RNG_SEED=42`，
Rust half 才会同种子；否则 Rust 侧不注入（T-30 生产路径 fail-soft：未设变数 ⇒ 保持默认源）。
`run_attack_pair.ps1` 也**不校验**两侧确实注入了种子（C# 侧有 `rng-stream.tsv` 的
`INJECT_OK 42`，Rust 侧无对应断言）⇒ 未设种子的跑法只会得到**非可比的红**，且要靠人记得。

**建议**（m3bag / owner）：在 `run_attack_pair.ps1` 里加**双侧种子护栏**（读 C# `rng-stream.tsv`
的 `INJECT_OK`，并与 Rust game-svr 启动日志/`game-svr` 的种子自证比对；不一致 ⇒ PREREQ）。
若要一条命令，`start_rust_stack.ps1` 应把 `MIR2_RNG_SEED` 显式透传给 `game-svr`。

---

## 本 tick 事实（窗口 / 端口 / 副作用）

- **窗口**：tick16 `status` 已 `window: CLAIMED`（本地端口起栈前实拍全空，`ports-before.txt`）。
  起栈 → 命中 F-A → 停栈。**窗口本 tick 结束时释放**（十端口实拍 + 3306 恢复为开工前的 DOWN；
  开工时 3306 亦为 DOWN，本片为复现临时起过 mysqld，收工已停）。
- **未做**：Rust 半（`start_rust_stack.ps1`）、`attack_diff.py align/diff` —— F-A 使 C# 半拿不到
  `attack_raw.jsonl`，两侧序列无对象可比。**不假装可比、不用白名单盖**。
- **未改**：`src/**`、`sql/**`、`MirClient`、以及 m3bag 写集全部仪器文件；本片只新增本证据目录。

## 复现命令

```powershell
# 1) 起 MySQL（本片环境里收工已停；复现时需要）
D:\mysql\mariadb-10.11.19-winx64\bin\mysqld.exe --datadir=D:\mysql\data --port=3306 --bind-address=127.0.0.1 --console
# 2) 活库 schema 判定（期望：ablity 空集、characters 三列）
mysql -h 127.0.0.1 -P 3306 -u root --password= --batch -e "SHOW COLUMNS FROM mir2_db.characters_ablity LIKE 'Inc%';"
mysql -h 127.0.0.1 -P 3306 -u root --password= --batch -e "SHOW COLUMNS FROM mir2_db.characters LIKE 'Inc%';"
# 3) 原 SQL 必红 / 修正必绿（两张原始输出见 03-repro-sql.txt）
```

## 原始件

| 文件 | 内容 |
| --- | --- |
| `00-csharp-stack-start.txt` | C# 栈（带 rng-hook 影子 GameSvr）起栈实拍：7 进程 + 10 端口 |
| `01-failing-run.txt` | `run_attack_pair.ps1 -Side csharp ... -AttackPinPair` 的完整 RED 输出（含 1054 与 run_pair:203） |
| `02-schema.txt` | `sql/mir2_db.sql` 行号 + 活库 `SHOW COLUMNS` 事实（`Inc*` 在 `characters`） |
| `03-repro-sql.txt` | 活库复现：schema 实拍 + 原 SQL `ERROR 1054` + 修正 SQL rc=0（0 行改动） |
| `04-original-vs-fixed-sql.txt` | 原 SQL / 修正 SQL 对照 + oracle 锚点行号 |

**本片不 merge、不自签 review。** F-A 修好后请 owner 重发窗口，本 lane 按 brief 的
`T30_MERGED` 路径重跑（C# 半 → Rust 半 → `attack_diff`）。
