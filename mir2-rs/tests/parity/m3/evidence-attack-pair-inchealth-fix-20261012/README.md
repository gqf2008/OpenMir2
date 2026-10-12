# 证据：清 M3②③ 攻击夹具真机 blocker F-A/F-B/F-C（卡 `openmir2-m3-attack-pair-fixture`）

- 卡：`openmir2-m3-attack-pair-fixture`（issue `76cc08b8`；T-29 同族）。
- 上游 finding：m3cs tick16 真机对拍在**第一个攻击 pin 语句** `ERROR 1054` 中止，两侧序列从未跑起来；
  可复现原始件见 `mir2-rs/tests/parity/m3/evidence-attack-pair-blocker-20261012/**`（m3cs 写集）
  与 owner 路由 comment `5ce4c443`。owner 把 F-A/F-B + F-C 的断言半边派回本卡（brief 顶部 2026-10-12 11:55）。
- 性质：**夹具/仪器侧修复**；零端口、未起服务端栈、未 merge、未自签 review。写集仅
  `mir2-rs/tests/parity/m3/**` + `whitelist.md`，**零 `.rs`**；`src/**`、`crates/**`、`bins/**`、`sql/**`、`MirClient` 全 0 改。
- 交付 head：`9cc89562`（fix）+ 本证据目录（含 `SHA256SUMS.txt`）。

> **2026-10-12 续修（本证据包自身）**：首版把 `01/02` 的清单 hash 按**盘上字节**记（含 2 处 CRLF）
> ⇒ 复核器默认的**仓库字节**口径（`tools/evidence-hash/verify_sha256sums.py`，LF 归一）算 `63d3b54b/41d5a22f`
> 与清单 `b5e84ecf/d2c45546` 不符（干净检出/合并后 RED 9/11，属本项目第 5 次踩的同一族）。
> 修法：把 `01/02` 归一为 LF（去掉那 2 个 CRLF），其余 `06–09` 用本次重跑刷新，再用
> `verify_sha256sums.py write <本目录>` 把清单重记为仓库字节口径。复核命令与原始输出见
> `selfcheck-selfcontained.txt`；本目录在干净 clone 上 `verify_sha256sums.py` **11/11 GREEN**。

## F-A（Important，硬阻塞）——pin SQL 别名挂错表

`gate_guards.ps1::Get-M3AttackPinSql`（基线 blob `3587f895`）把 `IncHealth/IncSpell` 设在
`characters_ablity` 别名 `a` 上，而这两列在**冻结 schema** `sql/mir2_db.sql` 里属 `characters`（别名 `c`）
⇒ 真机 pin 抛 `ERROR 1054 (42S22) Unknown column 'a.IncHealth' in 'SET'`、`run_pair.ps1:203` 整轮 RED。

**修法（唯一改动，一行）**：`a.IncHealth=0, a.IncSpell=0` ⇒ **`c.IncHealth=0, c.IncSpell=0`**
（`gate_guards.ps1:533`；改后 blob `54952a474fa6da3f8ea2849b070b32b0b89d5e37`）。

**oracle 锚点**（`Inc*` 池是**角色**列、不是 Abil 列）：

- `src/M2Server/Player/PlayObject.Base.cs:2073-2076`（`IncHealth += stdItem.AC`；同族 `IncSpell` `:2079`）。
- `src/M2Server/Player/PlayObject.Message.cs:70`（周期 tick 门 `!Death && (IncSpell>0||IncHealth>0||IncHealing>0)`）
  与 `:127-135`（`HP==MaxHP ⇒ IncHealth=0, IncHealing=0`；`MP==MaxMP ⇒ IncSpell=0`）。
- `src/Storeages/DBSvr.Storage.MySQL/PlayDataStorage.Save.cs:86,112-114`（落库的是**角色**行）。
- 与 T-14/T-28 一致：唯一会漂移的列是 `characters.IncHealth`（`scoped_diff.py::TABLE_COLUMNS` 第 22 列）。

## F-B（Minor，同类漏网）——`-SelfCheckNoStack` 对 pin SQL 只做字符串匹配

**两道互补机判护栏（都改回 `a.IncHealth` 必 RED）**：

1. **可执行性极小门（owner 指定）**：`gate_guards.ps1::Test-M3PinSqlExecutable` 把每条 pin `UPDATE`
   以**不存在角色名**（`WHERE ChrName='__m3_no_such_char__'`）**真送进 MySQL 执行一次** ⇒ 0 行改动、
   不碰任何角色/内容；但 MySQL 仍在 prepare 期校验表名/别名/列名 ⇒ 列挂错表必 `ERROR 1054`。
   - `run_attack_pair.ps1 -SelfCheckNoStack` **⑥b**：生产 SQL `rc=0`；把生产行改回 `a.IncHealth`
     ⇒ `rc=1` 且失败行含 `ERROR 1054 ... Unknown column 'a.IncHealth' in 'SET'`（见 `02-mutation-red.txt`）。
   - 真实侧运行前 **1.5)** 预检也真执行一次（`run_pair -EmitOpsArgs` 的 `attack_pin_sql_all`）⇒
     不再等 `run_pair` 半途才炸。
   - **只依赖共享 MySQL 3306**（客户端 `D:\mysql\mariadb-10.11.19-winx64\bin\mysql.exe`），不起服务端栈。
2. **无栈静态校验**：`Test-M3PinSqlColumns` 从冻结件 `sql/mir2_db.sql` 解析「表 → 列集合」，核对 pin SQL
   的 `alias.column` 赋值；`characters_ablity` 无 `IncHealth` ⇒ 历史 bug 必被检出。
   机判在 `run_attack_pair.ps1` **⑫**、`run_bag_equip_pair.ps1` **⑪**、`selftest_gate.ps1` **⑦**。

## F-C（Minor，operability）——两侧「确实注入了同一种子」无机器护栏

`gate_guards.ps1::Test-M3SeedInjected`：证据取自**进程内自证**（非脚本自述）——
C# 读 rng-hook（`MIR2_RNG_LOG`）的 `1\tINJECT_OK\t<seed>\t0` 行（`rng-hook/StartupHook.cs:96`）；
Rust 读 game-svr 启动日志的 `RNG 同种子注入：seed=<seed> 参照序列=SEED_PROBE ...` 行
（T-30，`bins/game-svr/src/main.rs:2825`），并叠加校验启动器 `RUST_SEED_INJECTED=<seed>`（若存在必须一致）。

- `run_attack_pair.ps1` 新增 `-Seed`（默认 42）、`-CsharpSeedLog`、`-RustSeedLog`
  （缺省 `<RustStackLogDir>\game-svr.out.log`，`RustStackLogDir` 默认 `E:\tmp\m2-acceptance\stack`，
  与 `tools/m2-acceptance/start_rust_stack.ps1:41` 的 `-LogDir` 同源）。
- **0.6)** 预检逐侧断言：日志缺失 / 无证据行 / 种子不符 ⇒ **fail-closed RED（exit 1）**；
  通过后把证据写入会话目录 `attack_seed.json`。
- `-SelfCheckNoStack` **⑬**（离线，7 向）与 `selftest_gate.ps1` **⑧** 机判（缺失/种子不符/空路径全红）。
- 剩余：`start_rust_stack.ps1` 的种子透传管线归 `openmir2-m3-rust-stack-seed-plumb`
  （issue `09841324`，m2-svr）；本卡只负责**断言**这一半。

## 判据原始输出（命令 + 输出 + 路径 + hash）

| 文件 | 命令 | 结果 |
|------|------|------|
| `01-selfcheck-nostack-green.txt` | `powershell -ExecutionPolicy Bypass -File mir2-rs/tests/parity/m3/run_attack_pair.ps1 -SelfCheckNoStack` | **GREEN（rc=0）**；⑥b 生产 `rc=0 failures=0`、⑫ 生产 0 violation、⑬ 7/7 |
| `02-mutation-red.txt` | 同上，但先把 `gate_guards.ps1:533` 改回 `a.IncHealth`（blob `5244e576`） | **RED（rc=1）**；⑥b `rc=1` 含真 `ERROR 1054 (42S22) Unknown column 'a.IncHealth' in 'SET'`；⑫ 2 violation；`SELFTEST_RED` |
| `03-repro-sql.txt` | 活库复现：`mysql ... -e "<原/修 SQL，WHERE 指向不存在角色名>"` | 原 SQL `ERROR 1054` rc=1；修正 SQL rc=0（0 行改动）；schema 实拍 `Inc*` 在 `characters` |
| `04-selftest-gate-47of47.txt` | `powershell ... -File mir2-rs/tests/parity/m3/selftest_gate.ps1` | **47/0 GREEN（rc=0）**；⑦ pin 列校验（含改坏 a.IncHealth=红）、⑧ 种子断言全绿 |
| `05-bagequip-selfcheck-green.txt` | `powershell ... -File mir2-rs/tests/parity/m3/run_bag_equip_pair.ps1 -SelfCheckNoStack` | **GREEN（rc=0）**；⑪ 生产 0 violation、改坏 2 violation |
| `06-gates-fmt.txt` | `CD mir2-rs && CARGO_TARGET_DIR=E:/tmp/m3bag-attack-fix-target cargo fmt --all -- --check` | rc=0 |
| `07-gates-clippy.txt` | `cargo clippy --workspace --all-targets --offline -- -D warnings` | rc=0 |
| `08-gates-test.txt` | `cargo test --workspace --offline` | **79 suites / 648 passed / 0 failed / 16 ignored**，rc=0 |
| `09-gates-msgcodegen.txt` | `cargo run -p msgcodegen --offline -- --check` | 604 条一致，rc=0 |

## 复现命令（本目录，无需起服务端栈）

```powershell
# 0) MySQL 3306 须在听（共享服务；本片未起未停）
# 1) 生产 GREEN
powershell -ExecutionPolicy Bypass -File mir2-rs/tests/parity/m3/run_attack_pair.ps1 -SelfCheckNoStack
# 2) 改坏必红（复现后用 git checkout -- 还原）
sed -i 's/c\.IncHealth=0, c\.IncSpell=0/a.IncHealth=0, a.IncSpell=0/' mir2-rs/tests/parity/m3/gate_guards.ps1
powershell -ExecutionPolicy Bypass -File mir2-rs/tests/parity/m3/run_attack_pair.ps1 -SelfCheckNoStack   # rc=1
git checkout -- mir2-rs/tests/parity/m3/gate_guards.ps1
# 3) 其它两条无栈自检
powershell -ExecutionPolicy Bypass -File mir2-rs/tests/parity/m3/selftest_gate.ps1
powershell -ExecutionPolicy Bypass -File mir2-rs/tests/parity/m3/run_bag_equip_pair.ps1 -SelfCheckNoStack
# 4) 活库复现
& 'D:\mysql\mariadb-10.11.19-winx64\bin\mysql.exe' -h 127.0.0.1 -P 3306 -u root --password= --batch -e "<03-repro-sql.txt 中的 SQL>"
```

## 残留物清单

- 入库：本目录 + `gate_guards.ps1` / `run_attack_pair.ps1` / `run_bag_equip_pair.ps1` / `selftest_gate.ps1`
  / `README.md`（m3）/ `whitelist.md`。
- 仓外临时物（可删）：`E:/tmp/m3bag-fabfix/**`（本次自检/门禁中间输出）、`E:/tmp/m3bag-attack-fix-target`（隔离 cargo target）。
- 端口/进程：本片**未起任何服务**；十端口状态与开工前一致（只剩共享 3306）。
- source-mutation 已还原：`gate_guards.ps1` blob = `54952a474fa6da3f8ea2849b070b32b0b89d5e37`（= 代码 head `9cc89562` 版本）。
- 证据字节：`01/02` 归一为 LF；`06–09` 为本次重跑原始输出；`SHA256SUMS.txt` 按**仓库字节**口径（`verify_sha256sums.py write`）重记。
