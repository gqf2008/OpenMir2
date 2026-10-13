# `openmir2-client-e2e-enter-gaps` 证据包（m2-svr / openmir2-worker-11，2026-10-13）

卡上两条“真机缺口”在本部署上**都不能复现为 Rust↔C# 的差异**。本片用真客户端 + **C# 对照组**
（同一 `D:\MirClient-run\MirClinet.exe`、同一角色 `aaa`、同一 `mir2_db`）把两侧跑成逐像素同类，
并用服务端日志证明“帧发了 / 世界有实体 / AOI 真可见”。按 §6.0-3 与项目硬约束，本片不做单方面行为偏离；
仅落地一处**只读可观测性**改进 + 一条构造级用例（带改坏必红），供后续真机定位不再误读。

## 结论（先看这三条）

1. **① “进图未下发 52/752” = 否**。真机 Rust 日志 `进世界帧下发：socket=… 共 15 条（输入齐备的）`
   恰好是 658,51,50,41,213,11029,42,766,54,5008,1106,**52,752**,811,46；新加的
   `52/752 入图输入` 行给出输入值：`WAbil[level=1 … hp=19 mp=15 max_hp=19 max_mp=15 …]`。
   卡里引的 `main.rs:1088/1177 的 Ability::default()` 是 `handle_deal_cmd`/`wire_of` 的**对手缺席哑元**，
   不是进图会话态（锚点见 `01-oracle-anchors.txt` §C）。
2. **② “世界 tick 无实体/无怪” = 否**。`sim: tick … 处理 N 项` 的 N 是**世界命令队列**长度
   （`TickRecord::processed.len()`），不是实体数。真机读数：`实体内 9525 展示表 9394`，
   角色所在图 `same_map=1726 nearest=2..3`；把角色放到比奇省刷怪区 `(256,572)` 后，
   客户端**屏幕上确实出现怪**（`shots/rust-monsters-visible-256-584.png`，3 个红名怪）。
3. **“角色无血 / 跑不动” = 冻结客户端 × oracle 的既有问题，非 Rust 回退**。C# 对照组同屏
   HUD 逐像素一致（`0/0` + `655360/25165829`）⇒ 52 的 40B 尾与客户端 `TAbility`（HIGHHP=True，58B）
   记录长度不匹配（客户端把 `HP` 读成尾部 offset 22 的 `Exp` 段 = 0）——**C# 也一样**，
   因此 `CanRun`（`HP < RUN_MINHEALTH(10)`）两边都拒绝跑。要不要修需 owner 决策
   （客户端冻结 ⇒ 只能改服务端布局，属与 oracle 的**故意偏离**）。锚点见 `01-oracle-anchors.txt` §A/§B。
4. **104（`CM_SELECTSERVER`）= `SS_KEEPALIVE`（IDS 链路在线数心跳）**，忽略对客户端选服/心跳无可见影响。

## 命令与原始件

```powershell
# 1) 起真栈（Rust game-svr/三网关 + C# DBSrv/LoginSrv + MariaDB；配置用现网 E:\MirServer）
powershell -ExecutionPolicy Bypass -File tools/m2-acceptance/start_rust_stack.ps1 `
  -RustBinDir E:/tmp/m2svr-target/debug
# 2) 同一真客户端跑同一角色（D:\MirClient-run）
powershell -ExecutionPolicy Bypass -File tools/client/mir_flow.ps1 `
  -Account mir2test -Password mir2pass -OutDir E:/tmp/m2svr-gaps/run5 -WalkClicks 0 -AutoPlaySec 70
# 3) C# 对照组：start-all.ps1 起 C# 栈，同命令、同角色再跑一遍（见 12-csharp-control-flow.txt）
# 4) 收工停栈；DB 位置已还原 (291,666)；MySQL 3306 保持运行（开工前只剩它）
```

- `10-rust-real-run-52-752-and-tick.txt`：Rust 真机 `game-svr.out.log` 摘录（52/752 输入 + 周期观测）。
- `12-csharp-control-flow.txt` / `13-rust-flow-flow.txt`：两侧 `mir_flow.ps1` 的阶段回执（均 `FLOW_OK`）。
- `shots/rust-monsters-visible-256-584.png`：Rust 侧角色位于比奇省 (256,584)，屏上 3 只怪。
- `shots/rust-city-spawn-288-654.png` / `shots/csharp-control-city-289-669.png`：两侧同城出生点，
  HP/MP HUD 逐像素同类（`0/0` + `655360/25165829`）。
- `04-gates.txt` / `05-boundary.txt` / `20-redcheck-world-aoi-diag.txt`。

## 本片代码改动（只读可观测性 + 1 条用例）

- `mir2-rs/bins/game-svr/src/main.rs`
  - `log_enter_ability`（进图时打印实际写入 52/752 的输入值与 40B 尾；只读）。
  - `WorldDiag`/`SessionAoiDiag`/`world_aoi_diag`：周期 `sim:` 行追加
    `实体内 N 展示表 M 在世界会话 K` 与每会话 `[name(id) map=… vis=… same_map=… nearest=…]`，
    从根本上消除“`处理 0 项` ⇒ 无实体”的误读。
- 新增用例 `world_aoi_diag_reports_visible_and_nearest`（构造级）：
  近怪必须出现在 `vis`，`same_map`/`nearest` 计数正确；改坏必红见 `20-redcheck-world-aoi-diag.txt`。
- **不改**任何模拟/出帧/取数语义；`src/ sql/ MirClient` 与 `crates/protocol`/`crates/storage` 边界为空。

## 验证（隔离 `CARGO_TARGET_DIR=E:/tmp/m2svr-target`，cwd=`mir2-rs/`）

- 四门禁：`fmt rc=0` / `clippy --workspace --all-targets -D warnings rc=0` /
  `test --workspace --offline` **97 suites · 988 passed · 0 failed · 21 ignored** / `msgcodegen 604`。
- 真机：两侧 `mir_flow` 均 `FLOW_OK`；端口实拍见栈脚本输出（停在 `RUST_STACK_OK`），收工 `-Stop` 后
  5000/7000/7100/7200 全 DOWN，仅 3306 保留。

## 残留物清单

- `E:\tmp\m2svr-gaps\`（两侧截图/日志/门禁原始件，部分已入库到本目录）；`E:\tmp\m2svr-target`（本片构建目标）。
- `E:\tmp\m2-acceptance\stack\`（最近一次 Rust 栈日志/pid 文件；进程已 `-Stop`）。
- MySQL 3306 常驻（开工前即有）；`mir2_db.characters.aaa` 的位置已还原为 `(291,666)`。
