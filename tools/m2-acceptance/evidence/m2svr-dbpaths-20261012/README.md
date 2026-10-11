# M2 验收③c「三路径 DB 字段级 diff」真机证据（2026-10-12）＋ 两条 harness 修复

卡：`openmir2-m2-gamesvr-host`（M2 验收①②③④）。
本目录是 `check_db_paths.ps1 -Mode diff -Canonical` 在**真机双侧**（C# 对照栈 + Rust 栈）上的产物。

## 结论：**GREEN**（三条路径、11 张表逐行一致）

```powershell
powershell -File tools/m2-acceptance/check_db_paths.ps1 -Mode diff `
  -CsharpDir E:\tmp\m2acc\csharp4 -RustDir E:\tmp\m2acc\rust4 `
  -Canonical -OutFile tools/m2-acceptance/evidence/m2svr-dbpaths-20261012/db-paths.json
# => softclose/disconnect/shutdown 各 GREEN；exit 0
```

`db-paths.json`：`pass=true`，三路径 `state=GREEN/rc=0`。

## 采法（两侧同前缀 `mir2test`；每步 `snap` before/after）

- **Rust 组**：`start_rust_stack.ps1 -RustBinDir E:\tmp\openmir2-m2svr-target\debug`（MySQL + C# DBSrv/LoginSrv
  下盘 oracle + Rust game-svr(5000)/三网关(7000/7100/7200)）⇒ `tools/client/mir_flow.ps1` ⇒ 停栈。
- **C# 组**：`E:\MirServer\start-all.ps1` ⇒ 同一 `mir_flow.ps1` ⇒ 停服务端（保留共享 MySQL 以便取 after 快照）。
- 三条退出路径：
  - `softclose`（小退）：`mir_flow.ps1` 默认流程走到 `CM_SOFTCLOSE(1009)`（Rust 日志 `console/rust-game-svr-exit.txt:119`）。
  - `disconnect`（掉线）：`mir_flow.ps1 -SkipLogout -LeaveClientRunning` 进世界后**杀客户端进程**（Rust 日志 `:163` 无 1009，只有"用户关闭/断线清世界"）。
  - `shutdown`（服务关闭）：同上进世界后**停整栈**（Rust：`start_rust_stack.ps1 -Stop`；C#：停 game 组件、保留 MySQL）。
- 快照 = `check_db_paths.ps1 -Mode snap`（内部 `dbsnap.py` 13 表 + `scoped_diff.py extract --normalize`，只取本前缀
  `mir2test` 的 235 行 / 3 个角色）。产物：`rust/*.jsonl`、`csharp/*.jsonl`。

## 牙齿（"改坏必红"）

- `db-paths-nocanonical.json`：同一批真机数据**不带 `--canonical`** ⇒ 三路径全 **RED/rc=1**。
  差异**全部落在易变列**（`account.LastLoginTime`、`characters.LASTUPDATE`、`characters_indexes.ModifyDate`），
  即两次侧运行的**墙钟**不同——正是必须 canonical 的原因，也证明判据不是恒绿的空壳。
- harness 层：`selftest.ps1` 新增 3 条（total 36/36 GREEN）——
  ① 不 canonical 仅易变列 ⇒ RED；② canonical 仅易变列 ⇒ GREEN；③ canonical 下行为列（`SelectID`）变 ⇒ 仍 RED。

## 本片修掉的两条 harness 真缺陷（不改判据、只让 ③c 能跑）

1. **`check_db_paths.ps1` 的空口令参数被 PS 5.1 吞掉**：`--password $MysqlPassword`（默认 `""`）退化成
   `--password snap …` ⇒ `dbsnap.py` 报 `invalid choice: 'E:\tmp\...'` ⇒ **④ 直接 RED**（假红）。
   改为 `--password=<值>`（`=` 形式，空值也成立）。
2. **`check_db_paths.ps1` 的 diff 漏了 `--canonical`**：M3 对拍本体一直用 `--canonical`
   （`run_m3_gate.ps1:179`、`run_bag_equip_pair.ps1:121`）；漏了它，真机双侧的墙钟列必然不同 ⇒ **④ 恒 RED**（假红）。
   新增 `-Canonical` 开关（默认关以兼容 selftest 合成短行夹具），`run_acceptance.ps1` 的真实 diff 步骤已带上。

## 复现（一条链路）

```powershell
# Rust 侧（示例：softclose）
powershell -File tools/m2-acceptance/check_db_paths.ps1 -Mode snap -Path softclose -Phase before -Prefix mir2test -SnapDir E:\tmp\m2acc\rust4\snap-softclose-before -OutDir E:\tmp\m2acc\rust4
powershell -File tools/client/mir_flow.ps1 -Account mir2test -Password mir2pass -CharSlot 1 -OutDir E:\tmp\m2acc\rust4\shots -SkipTalk -SkipReenter -WalkClicks 0
powershell -File tools/m2-acceptance/check_db_paths.ps1 -Mode snap -Path softclose -Phase after  -Prefix mir2test -SnapDir E:\tmp\m2acc\rust4\snap-softclose-after  -OutDir E:\tmp\m2acc\rust4
# C# 侧同前缀再跑一遍 -> E:\tmp\m2acc\csharp4
# 对拍
powershell -File tools/m2-acceptance/check_db_paths.ps1 -Mode diff -CsharpDir E:\tmp\m2acc\csharp4 -RustDir E:\tmp\m2acc\rust4 -Canonical
```

## 残留物

- `E:\tmp\m2acc\{rust4,csharp4}`（快照目录 + jsonl，本目录已归档关键件；其余可删）。
- `E:\tmp\m2-acceptance\stack\game-svr.out.log`（Rust 退出路径原始日志，已摘录 `console/`）。
- 收工端口：仅共享 MySQL `3306` 在听；5000/6000/7000/7100/7200 全空。
