# 帧 52 尾：冻结客户端 58B `TAbility` 布局（`openmir2-client-e2e-enter-gaps` ③ 收口）

- 卡：`openmir2-client-e2e-enter-gaps`（tier R）
- 车道：m2-svr / `openmir2-worker-11`；分支 `client-e2e-enter-gaps`
- 上一片：`patch 5c7bc55c` @ `7c8ac2fc3`（只读可观测性 + 卡上 ①② 不可复现结论）
- 本片：**把 ③ 的根因钉到字节级，并交付一个默认关闭的修复开关**（不改默认行为）

## 0. 结论（先说三条）

1. **卡上 ① 的前提不成立**：进图路径**已经**下发 52/752（15 条入图帧含 52、752）；卡里引的
   `main.rs` 的 `Ability::default()` 是 `handle_deal_cmd`/`wire_of` 的哑元，不是进图会话态
   （上一片已给真机日志）。
2. **“角色无血 / 跑不动”的真根因 = 冻结客户端与 C# oracle 的能力帧布局不匹配**（C# 与 Rust 同病）：
   - 客户端 `TAbility` 在 `HIGHHP=True` 下是 **58B**（`Grobal2.pas:39` + `:2167-2193`），
     `SM_ABILITY` 按 `SizeOf(TAbility)` 整块拷（`ClMain.pas:10102`），HUD 取 `m_Abil.HP/MaxHP`
     （`FState.pas:8887`），`CanRun` 要求 `HP >= RUN_MINHEALTH(10)`（`Actor.pas:2165`）；
   - oracle 的 40B `EncodeMessage(WAbil)` 把 `HP` 放在 offset 12（2B），客户端却按 offset 22 读
     `HP`(DWORD) ⇒ 读到 oracle 的 `Exp`（本部署 = 0）⇒ HUD `0/0`、`CanRun` 返回 -1。
3. **修复 = 让 52 尾按客户端 58B 记录布局编码（同一 `WAbil`，只换布局）**。本片以
   `MIR2_CLIENT_ABILITY_LAYOUT` 开关提供，**默认关**（未设 ⇒ 与 oracle 逐字节相同，金标准/四门禁不回归）。

## 1. 字节级证据（真机）

**改前**（默认 oracle 布局，开关未设）：

```
10:32:23.917 [DBG] 52/752 入图输入 ChrName=aaa layout=oracle-40 tail_len=40 WAbil[level=1 ac=0 mac=0 dc=257 mc=0 sc=0 hp=19 mp=15 max_hp=19 max_mp=15 exp=0 max_exp=10 weight=0 max_weight=50 wear_weight=0 max_wear_weight=15 hand_weight=0 max_hand_weight=12] SubAbility[anti_magic=1 hit_point=5 speed_point=15 anti_poison=0 poison_recover=0 health_recover=0 spell_recover=0] ability_tail_hex=01000000000001010000000013000f0013000f0000000000000000000a00000000003200000f000c
```

HUD：`0/0` + `655360/25165829`（截图 `shots/rust-oracle-40-hud-0-0.png`，血球暗）——客户端把
offset 22 读成 `HP`(DWORD)=`Exp`=0；offset 26 读成 `MP`(DWORD)=错位的相邻字节。

**改后**（`MIR2_CLIENT_ABILITY_LAYOUT=1`）：

```
10:34:53.847 [DBG] 52/752 入图输入 ChrName=aaa layout=client-58 tail_len=58 WAbil[level=1 ac=0 mac=0 dc=257 mc=0 sc=0 hp=19 mp=15 max_hp=19 max_mp=15 exp=0 max_exp=10 weight=0 max_weight=50 wear_weight=0 max_wear_weight=15 hand_weight=0 max_hand_weight=12] SubAbility[anti_magic=1 hit_point=5 speed_point=15 anti_poison=0 poison_recover=0 health_recover=0 spell_recover=0] ability_tail_hex=01000000000000000000010100000000000000000000130000000f000000130000000f000000000000000a0000000000320000000f0000000c00
```

HUD：`19/19` + `15/15`，血球满红（截图 `shots/rust-client-58-hud-19-19.png`）。

58B 尾逐偏移（与客户端 `TAbility` 对位；脚本复算见 `22-before-after-realrun.txt`）：

| 客户端偏移 | 字段 | 本片字节 | 值 |
| --- | --- | --- | --- |
| 0 | `Level: Word` | `01 00` | 1 |
| 2 / 6 / 10 / 14 / 18 | `AC/MAC/DC/MC/SC: Integer` | … | 0 / 0 / 257 / 0 / 0 |
| 22 | `HP: DWORD` | `13 00 00 00` | 19 |
| 26 | `MP: DWORD` | `0f 00 00 00` | 15 |
| 30 | `MaxHP: DWORD` | `13 00 00 00` | 19 |
| 34 | `MaxMP: DWORD` | `0f 00 00 00` | 15 |
| 38 / 42 | `Exp/MaxExp: DWORD` | `00…` / `0a 00 00 00` | 0 / 10 |
| 46…56 | `Weight…MaxHandWeight: Word` | … | 50 / 0 / 0 / 15 / 0 / 12 |

⇒ `CanRun` 门槛（`HP >= 10`）由 `HP=19` 满足；真机 `mir_flow` 报告 `FLOW_OK`，
自动挂机（`-AutoPlaySec 15`）期间角色移动。**未单独合成“跑键”对 `CM_RUN(3013)` 抓包**
（`CanRun` 是唯一门槛，源码锚见上），登记为真机复验项。

## 2. 本片代码（`bins/game-svr`，默认零行为变化）

- `record.rs::encode_ability_client(&Ability) -> Vec<u8>`：58B 客户端记录布局（新函数；
  `encode_ability`（oracle 40B）原样保留）。
- `enter.rs::AbilityInputs.client_ability_layout: bool`：`apply` 据此在 52 尾二选一。
- `main.rs::mir2_client_ability_layout()`：读 `MIR2_CLIENT_ABILITY_LAYOUT`
  （`1`/`true`/`TRUE`/`True` ⇒ 开；未设/其它 ⇒ 关）。
- `main.rs::log_enter_ability`：日志改为打印**实际**布局与尾字节（`layout=… tail_len=…`），
  修掉“开关打开但日志仍写 40B 尾”的误导。

**默认关**：未设 `MIR2_CLIENT_ABILITY_LAYOUT` 时，52 尾仍 = oracle `EncodeMessage(WAbil)` 40B
（金标准锚 `#12` 与 `enter::tests::ability_frames_follow_recalc_abilitys` 默认分支不回归）。

## 3. 判据 / 改坏必红

- `record::tests::encode_ability_client_matches_highhp_record`：58B + `HP@22 / MP@26 / MaxHP@30 / MaxMP@34` 逐偏移。
- `enter::tests::ability_frames_follow_recalc_abilitys`：默认分支尾 = `encode_ability`；
  开关分支尾 = `encode_ability_client` 且 `HP@22 == out.wabil.hp`。
- **改坏必红**（`23-redcheck-client-layout.txt`）：把 `encode_ability_client` 的 `HP` 槽写成 `max_hp`
  ⇒ enter 用例 FAILED（`left: 19 right: 15`，rc=101）；还原 ⇒ 绿。
- 真机改坏（真客户端）见 §1 的改前读数（`0/0`）。

## 4. 为什么默认关 + 需要 owner 裁决

- oracle 与冻结客户端在此**不可同时满足**：oracle 40B、客户端 58B。
- 项目硬约束 #1（客户端冻结 + 协议逐字节兼容）与 #3（行为等价优先于修 bug）在此冲突；
  打开这个开关属 §6.0-3 的「与 oracle 的故意偏离」，**需 owner 裁决 + 白名单登记**
  （纪律：白名单条目 owner 签字，作者不自加）。
- 建议：**把默认改成 `client-58`**（让现存客户端原样可用；oracle 40B 留给需要逐字节对拍的夹具），
  并登记一条 `B-*`/`T-*`（差异点 = 52 尾布局 / 为什么无害 = 客户端冻结、C# 同病且无血不可玩 /
  谁审的 = owner）。若 owner 选择保留 oracle 默认，则本开关作为“真机联调可选项”保留；
  此时卡上 ① 的“客户端血条非 0”**只能在开关打开时成立**（如实登记）。

## 5. 边界与窗口

- 边界：`git diff --name-only <base> -- src sql MirClient crates/protocol crates/storage` = 空。
- 窗口：真机两相（改前/改后）各起栈一次，收工 `start_rust_stack.ps1 -Stop`；
  5000/7000/7100/7200 全 DOWN，仅 3306 保留（开工前即只有它）。
