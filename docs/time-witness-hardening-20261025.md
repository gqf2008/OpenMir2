# 时间列见证加固：fail-closed 默认 + 限定到本角色行（卡 `openmir2-time-witness-hardening`，tier R）

> **取数时点**：2026-10-25（本卡 head 与 selftest 输出；快照/窗口数据来自 `E:/tmp/window/harness-proof/`，无实时数值依赖）。

- 卡：`openmir2-time-witness-hardening`（TZ-1 的 follow-up，来源 review `df349949`；
  owner `openmir2-dev-1`；branch `card/time-witness-hardening`；base = 本 patch 的 `--base`）。
  写集：`tools/conformance/compare_runs.py`、`tools/conformance/rules.json`、
  `tools/conformance/selftest_impl.py`、`tools/db-parity/dbsnap_pair.py`、
  `mir2-rs/bins/db-srv/Cargo.toml`、本文件。

## 1. G1：缺 `db.time_columns` 键 = 静默关掉整层见证（fail-open → fail-closed）

原来 `db.get("time_columns", {})` 的默认值让「没登记任何时间列」等价于「不做任何见证」并跑出
**rc 0 PASS**——与本日另外两次同源（`int(summary.get('exit_code') or 2)`、
`characters.ModifyTime` 不存在到 12 条牙全绿）：**看不见 ≠ 没问题**。

修法（双保险）：

1. **加载期**：`db` 里没有 `time_columns` 键 ⇒ `InputError`（rc 2），不再有隐式默认；
2. **运行期**：见证层用 `if time_witness is not None` 而不是真值判断——**空登记表**
   （`{}`）也必须跑这一层，于是其中出现的任何时间列都会判 `time_witness_unregistered`。

牙：`牙TZ-l`（缺键 ⇒ rc 2）、`牙TZ-k`（空登记表 + 存在时间列 ⇒ rc 2 unregistered）。
变异「去掉缺键拒绝」⇒ 红。

## 2. F1：max-only 的误绿/误红

reviewer-20 构造的三例（旧写法只看「表里最大值」）：

| 例 | 情形 | 旧写法 | 现在 |
|---|---|---|---|
| **A** | 本次没写，但表里有**别人** 5 分钟前写的行 | 绿（误绿） | 红（`time_witness_stale`） |
| **C** | 本角色行是**上一次运行**写的（start 前 300s） | 绿（误绿） | 红 |
| **B** | 本次写了，但表里有**更晚**的行（窗口外） | 红（误红） | 绿 |

收敛方案（两者都做了）：

1. **限定到本角色行**：角色名取自 `run.json` 的 `argv` 里的 `--char-name`
   （probe 产物里恒有；实测 r1/r2 两侧都有 `--char-name winplay0`）。
   `characters` / `characters_indexes` 按 `ChrName` 过滤，`characters_ablity` 按
   `PlayerId = 该角色 Id` 过滤（Id 从同一快照的 `characters` 表解析）。
   **拿不到角色名/角色行 ⇒ `time_witness_unscoped`（判不了，fail-closed）**——
   不许退回「看全表最大值」。
2. **窗口内「至少一行」而不是「最大值」**：本次写入的那一行只要落在窗口里就算过，
   同行后面还有更晚的行（B）不会把本次判红。

窗口 = `[start_wall − time_lead_s, end_wall + time_tolerance_s]`：

- `time_lead_s`（新增，默认 **30s**）：**下界 slack 要小**，否则「上一次运行写的行」
  （C，start 前 300s）会落进窗口 ⇒ 误绿。30s 只用于吸收 DB/主机之间的秒级时钟偏差。
- `time_tolerance_s`（默认 600s）：上界 slack，照顾长场景（#23 带 620s sleep）与
  存档在窗口末尾才落库的情形。

牙：`牙TZ-h`（A）、`牙TZ-i`（C）、`牙TZ-j`（B）、`牙TZ-g`（历史行 + 窗口内一行 ⇒ 绿）、
`牙TZ-e`（620s 长场景运行开头写 ⇒ 绿）、`牙TZ-f`（比 start 早 660s ⇒ 红）。
变异：「把 `lead` 放大到 600」⇒ 牙TZ-i 红；「退回按最大值判」⇒ 牙TZ-j 红；
「去掉 unscoped 的 fail-closed」⇒ 牙TZ-m 红。

**语义边界（与 TZ-1 一致）**：见证判的是「**本次运行有没有写这个角色**」，
不是「表里每一行的时间都对」。所以既不逐行判（历史行永远 stale），
也不看全表最大值（别人的写入会污染）。

## 3. 其它

- **F4（已由 reviewer-20 独立复跑，`52f67e5831d9`）**：`cargo test -p db-srv --lib
  session_time_zone_and_local_wall_clock -- --ignored --nocapture` ⇒ **1 passed**（含负向对照
  「不带本卡选项的 sqlx 默认会话时区 == `+00:00`」）。
- **CMD-1（reviewer-20）已修**：文档/注释里写的 `cargo test -p db-srv --bins …` **跑不到**这些牙
  ——db-srv 是「lib + 瘦 bin」，测试编进 **lib** 目标，`--bins` 会输出 `running 0 tests … ok` 的假绿。
  已把 `db-srv` 下全部 **12 处** `--bins` 改成 `--lib`（`db.rs` 4 / `data.rs` 2 / `handler.rs` 1 /
  `lb08_tests.rs` 1 / `playdata.rs` 3 / `testsupport.rs` 1）与本文件、`docs/dbsrv-session-timezone-20261025.md`
  的同类写法。
- **CMD-1′（reviewer-20）**：`testsupport.rs` 原来写的过滤名 `load_fault_tests` **全仓不存在**
  （既无 `mod` 也无 `fn`）——照它跑同样是 `running 0 tests / ok` 的假绿。已改成
  **全量 ignored**：`cargo test -p db-srv --lib -- --ignored --nocapture`。
  **口径**：文档里写的每个过滤名都必须能在目标里解析到（`mod`/`fn` 至少一个）；共享夹具文件
  没有自己的模块时，写全量而不是编一个名字。
- **依赖对齐**：`url = "2.5"`（与 lock 一致；它本来就是 sqlx 的传递依赖，本条只是提成直接边，无新增 crate）。
- `docs/dbsrv-session-timezone-20261025.md` §3b 的反向对照在本改动下更强：r1 的 rust 侧
  本角色行 `LASTUPDATE=02:10:25` 会落在窗口外 ⇒ 仍 `time_witness_stale`；csharp 侧本角色行
  `10:09:21` 在窗口内 ⇒ 通过。（该结论依赖 `--char-name`；旧证据里有。）

## 4. 门禁与未做

- `compare_runs selftest` **335/335 GREEN**；`dbsnap_pair selftest` 30/30；`check-spec` GREEN；
  7 项卫生门 + `git diff --check <base> <head>` + `merge_tree_helper` 见 patch body。
- **未跑 cargo**（本期机器归 dev-2）：改了 `db-srv/Cargo.toml` 一行（`url = "2.5"`），
  需要在窗口里补 `cargo fmt/clippy/test --workspace` + 那条 `--ignored` 牙（F4）。
- **真机复拍**（TZ-1 遗留）：两侧时间列相等且新判据 rc 0，仍未做。
