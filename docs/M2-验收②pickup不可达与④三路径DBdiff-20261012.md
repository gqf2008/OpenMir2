# M2 验收：② `pickup` 不可达判决 + ④ 三路径 DB diff 真机 GREEN（2026-10-12）

- 卡：`openmir2-m2-gamesvr-host`（M2 验收①②③④）+ `openmir2-m2-enter-world-frames`（顺序判据②）。
- 分支：`worktree-m2-gamesvr-host`；写集：`tools/m2-acceptance/**`、`tools/client/mir_flow.ps1`、`docs/**`。
- **未改**：`crates/**`、`src/**`（C# oracle）、`MirClient`、表结构/脚本语法/数据格式；**未 merge、未签 review**。

## 1. 本片要解决的问题（brief §24/§26 的最后两个真机项）

owner 排期要求 M2 ②（补 `pickup`）与 ④（三路径 DB diff，两侧 + `dbsnap`）。本片就这两项取证：

- ② 的 6/7 已由既存证据覆盖（`evidence/m2svr-run-20261012*`）；剩 `m2-06-pickup`。
- ④ 此前一直"未跑"（`m2svr-run-20261012-2/README.md` §4 如实登记 PREREQ）。

## 2. 结论一：`m2-06-pickup` 在 oracle 上**物理不可达**（② 判据不可满足）

### 2.1 证明（`tools/m2-acceptance/check_pickup_unreachable.ps1`，exit 0）

真实 oracle（`src/**`）里**地面物从来没有被创建过**：

1. `MapItem` 是 `record struct`，`ItemId` 默认 0；其构造器唯一一处赋值是**注释**
   （`src/Modules/SystemModule/Data/MapItem.cs:41`：`// this.ItemId = M2Share.ActorMgr.GetNextIdentity();`）。
2. 全仓 **895** 个 `.cs` 里**零个**未注释的 `ItemId = …` 赋值点（对象初始化器也没有）。
3. `Envirnoment.AddItemToMap` 首行 `if (mapItem.ItemId == 0) return false;`（`Envirnoment.cs:88-93`）。
4. 4 个 `new MapItem`（掉落 `BaseObject.cs:551/740`、脚本 `ExecutionProcessingSys.cs:5694/5730`）都不设 `ItemId`。

⇒ `AddItemToMap` **恒 false** ⇒ 地面无物 ⇒ `ClientPickUpItem`（`PlayObject.Base.cs:55`）的
`Envir.GetItem` 永远取不到 ⇒ **`CM_PICKUP` 无物可拾**。这与 `m3bag` 已登记的 **T-19**
（正常 `CM_DROPITEM` 恒 `SM_DROPITEM_FAIL`）同源——**是 oracle 的既有形态，不是 Rust 缺件**。

### 2.2 红检（改坏必红）

在 `MapItem()` 里插入 `this.ItemId = 123;` ⇒ 门禁 **exit 1**（RED，打出 `MapItem.cs:41`）；
`MapItem.cs` 以 `git HEAD` 字节还原后 **exit 0**（GREEN），`git status --short src/` 空。
原始输出：`evidence/m2svr-pickup-unreachable-20261012/redcheck-{mutated,restored}.txt`。

### 2.3 owner 裁决（`073f0dac`，2026-10-12）= **branch A**（oracle 不可达）

- **不批** `crates/world` 写分片（回写会让 Rust 删掉一件 oracle 从未落到地上的物，反而制造偏离）；
- ② 第 7 格判据**改写为**「**无地面物**这一唯一可达形态下，两侧对 `CM_PICKUP` 的处理一致：
  不产生任何下行帧、不进包、不崩、不改变会话态势」；
- 登记 **T-25**（`mir2-rs/tests/parity/whitelist.md`）。

落地实现：`check_screenshots.ps1` 默认**不要求** `m2-06-pickup`（新增 `-RequirePickup` 保留 legacy 7 张口径）；
可达面两侧逐字段一致由 `m3bag` 的 `ground-probe`（23 用例，`ItemId` 由夹具显式给值）+
`ground.rs::matches_oracle_ground_fixture` 钉住；harness 自检 38/38 GREEN（含 pickup 可选/legacy 两向）。

### 2.4 部署 C# 参照栈上的实拍（owner `evidence_required`）

- 起 `E:\MirServer\start-all.ps1`（C# 全套） + `tools/botload` 固定背包/装备序列（T-22）直跑 `BotSrv.exe`：
  bot 在部署栈上真的发了一条 `CM_PICKUP x=281 y=617`（`M3BAGSEQ step=0`）——服务端处理后
  **无任何地面物产生、无 `SM_ADDITEM`/`RM_ITEMSHOW`**，会话照常。
- 如实登记限制：本次 bot 的背包视图为空（步 1/3/4 全 skip）⇒ **未在部署栈上实跑出 `CM_DROPITEM` 的
  `SM_DROPITEM_FAIL`**；丢弃恒失败由真身 oracle 代码的 `ground-probe`（`drop_fail_item_id_zero`）钉住（T-19）。
- 残余风险：部署 `GameSrv.exe` 与 `src/` 的二进制级 provenance 未校验（本仓以 `src/**` 为 oracle）。
- 原始件：`evidence/m2svr-pickup-unreachable-20261012/deployed-csharp/**`。

### 旧 2.3（已由 owner 裁决覆盖，保留作历史）

## 3. 结论二：④ 三路径 DB diff **真机双侧 GREEN**（11 张表逐行一致）

```powershell
powershell -File tools/m2-acceptance/check_db_paths.ps1 -Mode diff `
  -CsharpDir E:\tmp\m2acc\csharp4 -RustDir E:\tmp\m2acc\rust4 -Canonical
# softclose / disconnect / shutdown 各 GREEN；exit 0；db-paths.json pass=true
```

- 两侧同前缀 `mir2test`（235 行 / 3 个角色，13 表快照 → 作用域抽取）。
- **C# 对照栈**：`E:\MirServer\start-all.ps1`（真客户端同一 `mir_flow.ps1` 同一动作）；
  **Rust 栈**：`start_rust_stack.ps1`（MySQL + C# DBSrv/LoginSrv 下盘 + Rust 5000/7000/7100/7200）。
- 三条退出路径均落到真实事件：`softclose` = `CM_SOFTCLOSE(1009)`；`disconnect` = 杀客户端（无 1009）；
  `shutdown` = 停整栈。Rust 侧日志摘录 `evidence/m2svr-dbpaths-20261012/console/rust-game-svr-exit.txt`。
- **牙齿**：同批真机数据**不带** `-Canonical` ⇒ 三路径全 **RED**，差异**全在墙钟列**
  （`account.LastLoginTime` / `characters.LASTUPDATE` / `characters_indexes.ModifyDate`）
  ⇒ 判据不是恒绿空壳。证据 `db-paths-nocanonical.json`。

## 4. 本片修掉的两条 harness 真缺陷（④ 此前不可能跑绿的根因）

1. **空口令实参被 PowerShell 5.1 吞掉**：`check_db_paths.ps1` 原用 `--password $MysqlPassword`（默认 `""`）
   ⇒ 命令行退化为 `--password snap …` ⇒ `dbsnap.py: error: invalid choice: 'E:\tmp\…'` ⇒ **④ 假红**。
   修为 `--password=<值>`（`=` 形式，空值也成立）。
2. **diff 漏 `--canonical`**：M3 对拍本体一直用 `--canonical`
   （`run_m3_gate.ps1:179`、`run_bag_equip_pair.ps1:121`）；漏了它，真机双侧墙钟列必然不同 ⇒ **④ 恒 RED**。
   新增 `-Canonical` 开关（默认关，兼容 `selftest.ps1` 的合成短行夹具），`run_acceptance.ps1` 的真实 diff 步骤已带上。

两条都补了 `selftest.ps1` 用例（`total=36 failed=0`）：不 canonical 仅易变列=RED / canonical 仅易变列=GREEN /
canonical 下行为列（`SelectID`）变=RED。

## 5. 驱动新增：`mir_flow.ps1 -SkipLogout`

三路径里 `disconnect`/`shutdown` 需要客户端**留在世界里**（不触发 `CM_SOFTCLOSE`）。新增 `-SkipLogout`
跳过所有小退/顶号动作并落一张 `07b_skip_logout_inworld.png`（`-LeaveClientRunning` 保留进程供外部杀/停栈）。

## 6. 门禁

| 项 | 命令 | 结果 |
| --- | --- | --- |
| fmt | `cargo fmt --all -- --check` | （见下）|
| clippy | `cargo clippy --workspace --all-targets -- -D warnings` | |
| test | `cargo test --workspace --offline` | |
| msgcodegen | `cargo run -p msgcodegen -- --check` | |
| harness 自检 | `selftest.ps1` | **36/36 GREEN** |
| pickup 门禁 | `check_pickup_unreachable.ps1` | GREEN（红检必红）|
| ④ 真机 | `check_db_paths.ps1 -Mode diff -Canonical` | GREEN（无 canonical 必红）|

> 本片无 Rust/C# 代码改动，仅工具与文档；四门禁仍按合并树复核（见交付说明）。

## 7. 未做 / 登记

- ② 剩 `m2-06-pickup`：**oracle 不可达**（见 §2），判据待 owner 签字。
- ② 的 ④ 截图里 `see-others` 已达成（既存 `m2svr-run-20261012-2`）；`talk` 已达成（`m2svr-run-20261012`）。
- `52 WAbil.max_hp/max_mp=0`：归 `m3bag` 修，本 lane 待其合入后复查 ③。
- ④ 的**行为增强**（先走位/拾取再退出，使保存字段非平凡）未做：本片三条路径的 `characters` 行为列本就无变化，
  故 diff=0 主要证"无多余写"；要证"保存正确"需给退出前加可观测状态变化（可作后续片）。

## 8. 残留物清单

- `E:\tmp\m2acc\{rust4,csharp4}`：快照目录 + 12 个 jsonl（关键件已归档本仓 `evidence/m2svr-dbpaths-20261012/`）。
- `E:\tmp\m2-acceptance\stack\game-svr.out.log`：Rust 退出路径原始日志（已摘录）。
- `E:\MirServer\logs\GameSvr.out.log`：C# 对照栈日志（已摘录）。
- `E:\tmp\m2acc\pickup-proof*`、`ps-argtest*`：探针残留。
- **端口**：收工仅共享 MySQL `3306` 在听，5000/6000/7000/7100/7200 全空；客户端已退。
