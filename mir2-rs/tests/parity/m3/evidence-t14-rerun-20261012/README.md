# T-14 两相 pin 对拍复跑（T-28 修好后）——M3① 背包/装备跨侧字段级 diff = 0

- **卡**：`openmir2-m3-bag-equip-alignment`（issue `c28cea21`）；owner 2026-10-12 08:50 **条件式 GO**（`T28_MERGED`）。
- **判据 head**：`376ea0042c0636e724dabf5b6c978f8a7923c091`（= master tip；含 T-28 `505f8d16`「周期恢复 tick」+ move-wiring/m3cs/bag15）。
- **窗口**：起栈前实拍十端口只剩共享 3306 ⇒ 本轮归 m3bag；收工后十端口实拍仍只剩 3306（`ports.txt`）。
- **纪律**：不 merge、不自签 review；起栈命令一律重定向到文件（无 `| tee/tail/head/grep`）。

## 结果（判据 = `scoped_diff.py diff --canonical --fixed-newchr`）

```
GREEN: 作用域内 9 张表逐行一致
RC=0
```

- 会话：C# `csharp-20261012-085856`（prefix `m3cbag4`） vs Rust `rust-20261012-090020`（prefix `m3rbag4`）。
- 两侧**同一组开关**：`on step_ms=1000 start_delay_ms=3000 disable_autoplay=True`（`BAGEQUIP_SCOPE_OK`）。
- 两侧**同一固定起点**：`map=0 home=0,289,618 pos=289,618 level=1 dir=0 bag=48@0,97@1,207@2,3@3 abil=1`（`STARTPOSE_OK`）。
- 两侧同序列完整跑完：`M3BAGSEQ` 9 条（`arm` / `SM_BAGITEMS bag_view=4` / `snapshot` / `step=0..4` / `done`），
  五步 = `CM_PICKUP → CM_TAKEONITEM → CM_TAKEOFFITEM → CM_DROPITEM → CM_EAT`。
- **`characters.IncHealth` 两侧均为 0**（十四格唯一剩余差异列，由 T-28 修复）：见 `inc-health.txt`。
  - 十四格 RED：C#=0 / Rust=20；T-28 在 Rust 侧建了周期恢复 tick，吃药后满血 ⇒ 首轮 tick 即把 `IncHealth` 归零。
- settle+hold 仪器真机生效：C# `PIN_HOLD attempt=3`（前两轮被异步掉线存档覆盖）、Rust `PIN_HOLD attempt=2`
  （`attempt>=2` = 连续两轮 settle 复验都到位，与 §15 补强后的判据一致）。

## 文件

| 文件 | 内容 |
|---|---|
| `commands.txt` | 原始命令清单（含窗口纪律与 head） |
| `run-csharp.txt` | `run_bag_equip_pair.ps1 -Side csharp -Bots 1 -Prefix m3cbag4 -PinStartPose` 全量输出（RC=0） |
| `run-rust.txt` | `... -Side rust -Bots 1 -Prefix m3rbag4 -PinStartPose -PeerSession csharp-20261012-085856` 全量输出（RC=0，尾段为判据） |
| `diff-scoped.txt` | 判据显式重放：`python scoped_diff.py diff --canonical --fixed-newchr ...` ⇒ GREEN，RC=0 |
| `inc-health.txt` | 两侧 `characters` 行（按 `scoped_diff.TABLE_COLUMNS` 唯一列序）的 `IncHealth/IncSpell/IncHealing`，均 0 |
| `ports.txt` | 起栈前 / C# 栈中 / Rust 栈中 / 收工后 四段十端口实拍 |
| `save-req-resp.txt` | Rust game-svr 存档请求/应答（`DB_SAVEHUMANRCD` → `Ident=1102`）两相各一次 |
| `rust-game-svr.out.log` | Rust game-svr 运行日志全量（含进世界 / CM_QUERYBAGITEMS / 五步 CM_* / 恢复 tick） |
| `gates-fmt.txt` / `gates-clippy.txt` / `gates-msgcodegen.txt` / `gates-test-workspace.txt` | 四门禁在 head `376ea004` 的回归输出（本片零 `.rs`）：fmt/clippy/msgcodegen rc=0；test = 73 suites / 589 passed / 0 failed / 16 ignored |
| `csharp-start.txt` / `csharp-stop.txt` | `E:\MirServer\start-all.ps1` 起停输出 |
| `rust-start.txt` / `rust-stop.txt` | `tools/m2-acceptance/start_rust_stack.ps1` 起停输出 |

## Rust 侧二进制

`E:\tmp\openmir2-m3bag-target\debug\{game-svr,login-gate,sel-gate,game-gate}.exe`，在 head `376ea004` 重建
（`CARGO_TARGET_DIR=E:/tmp/openmir2-m3bag-target cargo build -p game-svr -p login-gate -p sel-gate -p game-gate`）。
存档 preflight 判 `DB_SAVEHUMANRCD` 已实现（T-27 已由存档片收敛）。

## 残留物清单

- 入库：`mir2-rs/tests/parity/m3/evidence-t14-rerun-20261012/**` +
  `mir2-rs/tests/parity/m3/README.md`（T-14 段：8/8→12/12 + O-2 树对应注释 + T-28 复跑 GREEN）+ 
  `evidence-t14-pinhold-20261012/{commands.txt,SHA256SUMS.txt}`（O-2 注释 / 清单重算 12/12）。
- 会话目录（按 `.gitignore`，`*.log` 不入库；`sessions/{csharp-20261012-085856,rust-20261012-090020}/` 保留本机供复查）。
- 临时物（保留，证据）：`E:/tmp/m3bag4-*.log`（起栈/侧运行/停止）、`E:/tmp/m3bag4-scoped-diff.txt`；
  Rust 栈日志 `E:/tmp/m2-acceptance/stack/**`。
- 端口/进程：收工后十端口只剩 3306（共享 MySQL，pid 26652）；无 BotSrv/GameSrv/网关残留。
