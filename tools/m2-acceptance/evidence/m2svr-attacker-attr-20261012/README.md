# M3②③ k=2 分岔（m2-svr 半）· 真机复现 + oracle 修复 · 证据包（2026-10-12）

- 卡：`openmir2-m3-attacker-attr-loading`（本片新建；owner 2026-10-12 16:45 brief 顶部转派，未指名卡）
- 车道/principal：`m2-svr` / `openmir2-worker-11`；worktree `m2-svr`；分支 `worktree-m2-gamesvr-host`
- 写集：**仅 `mir2-rs/bins/game-svr/src/main.rs`**（未动 `crates/world/**`、未动 `tests/parity/m3/**` 夹具、未动 `src/**`）
- 交付：`patch` + `status: needs-review`；**不 merge、不自签 review**

---

## 1. 结论（独立复现，未采信转述）

**m3cs 的定位成立，且机制已坐实到 oracle 行号**：真机 Rust 的**攻击方世界实体**从未装载
`RecalcAbilitys()` 产出的 `WAbil`/`SubAbility`，因此：

1. `Entity.dc = 0` ⇒ `GetBaseAttackPoewr = LoByte(WAbil.DC) = 0`
   （`BaseObject.Attack.cs:25/27`）⇒ `nPower = 0` ⇒ `if (nPower > 0)`（`AttackBase.Attack.cs:52`）不成立 ⇒ 无 `SM_STRUCK`；
2. 连带**守方** `Entity.hit_point = 0` ⇒ 命中门 `if (targetObject.HitPoint > 0)` 短路
   （`BaseObject.Attack.cs:39`）。

⇒ `attack_diff.py` 把 Rust 120/120 记成躲闪（`hits=0`）。**这是「攻方属性未装载」，不是命中/躲闪门公式差**。

### 为什么 `characters_ablity.Dc=0` 但 C# 有攻击力（关键修正）

- C# 读档：`humanRcd.DC = dr.GetUInt16("DC")`（`src/Storeages/DBSvr.Storage.MySQL/PlayDataStorage.Query.cs:280`）；
  该列在 pin 后确为 **0**（`csharp-db-ablity-dc0.txt`）。
- 但登录序是 `AbilCopyToWAbil()` → **`RecalcLevelAbilitys()`** → `RecalcAbilitys()`
  （`WorldServer.cs:351` / `PlayObject.cs:1308`），而 `RecalcLevelAbilitys()` 对战士改写
  `Abil.DC = MakeWord(MAX(Level/5-1,1), MAX(1,Level/5))`（`PlayObject.cs:5305`）：
  **L1 ⇒ `MakeWord(1,1) = 0x0101`**（`oracle-recalc-level-abilitys.txt`）。
- 真机 rng 流印证：两次玩家命中前，`PlayObject.GetAttackPower@2929` 的 `Next(max=1)->0`
  ⇒ `nPower` 增量 0，`result = nBasePower = LoByte(DC) = 1`
  （`csharp-rng-hit-context.txt`）；`_Attack@41` 命中门 `Random(15)` 未躲闪、`GetHitStruckDamage@792`
  未减免、`PlayObject.StruckDamage@2957`（目标是玩家）、`AttackSuccess@223`（攻方是玩家）。
- 旁证：`ApplyItemParameters`/`RecalcAdjusBonus` 不参与（无装备、`characters_bonusability` 无该角色行 ⇒ `BonusAbil=0`）。

## 2. 修法（in 写集，按 oracle）

`bins/game-svr/src/main.rs` 进世界路径：同一份 `enter::recalc_for_session()`（= 既有 52/752 帧的
`ability::recalc_abilitys`，**不复制第二份实现**）的输出 `out.wabil/out.sub`，在 `enter_map` 之后
**写入世界实体**（`apply_ability_to_entity`）：

| 实体字段 | ← oracle |
|---|---|
| `dc`/`ac` | `WAbil.DC`/`WAbil.AC`（`BaseObject.Attack.cs:25/27`、`BaseObject.Base.cs:789`） |
| `hit_point`/`speed_point` | `SubAbility.HitPoint`/`SpeedPoint`（`BaseObject.Attack.cs:41`） |
| `hp`/`max_hp`/`level` | `WAbil.HP`/`MaxHP`/`Level` |

## 3. 改前 / 改后（同种子 seed=42、同 C# 对照会话 `csharp-20261012-170110`、同图 0137、同 pin）

- **改前**（master `8ac62fb8`）：`rust-20261012-171710` → `attacked=120 hits=0 sum=0`，**非平凡门 RED**（复现 k=2 现象）。
- **改后**（本 patch）：`rust-20261012-171957` → `attacked=120 hits=19 sum=19`，**非平凡门绿**。

原始件：`rerun-before-after.txt` + `sessions/{csharp-20261012-170110,rust-20261012-171710,rust-20261012-171957}/`。

红检（改坏必红）：注释掉 `e.dc = i32::from(wabil.dc);` ⇒ 用例
`tests::apply_ability_to_entity_loads_wabil_and_sub` **FAILED**（`left: 0 right: 257`），随还原
（`redcheck-dc-assignment-removed.txt`）。

## 4. 如实登记：**DoD「跨侧逐 k 归零（diff = 0）」本片未达成**（本片不改判据）

改后 `attack_diff.py diff` 仍有 **80 项**差异（改前 1 项，但改前是「Rust 恒 0 击」的假象；
非平凡门才是本片真正修复的判据）。残差有两条**在属性路径之外**的成因：

1. **RNG 流位置未对齐**（m3cs 已登记，属夹具 m3bag/m3cs 写集）：C# 全局 `RandomNumber` 被
   **非 0137 图**持续消耗（本次 C# 流 597,954 行里攻击只占 120×2 次 `PlayObject.GetAttackPower`，
   余为 `AnimalObject.Wondering`/`WorldServer.MonGen`/`Merchant.Run` 等），Rust 半并不消费同一批
   ⇒ 同一「第 k 次」的 `RandomByte(SpeedPoint)` 不同 ⇒ 命中/躲闪位置整体错位。
2. **目标解析口径差（也可能是 oracle「状态化 Dir」的真实语义）**：本 C# 流里
   `PlayObject.AttackSuccess@223`（= `GetPoseCreate()` 非空）只出现 **3 次**，而 Rust
   `handle_hit_cmd` 按消息 `Tag` 求前方格、**120/120** 都能找到 victim。oracle 的
   `GetPoseCreate()` 用**当前 `Dir`**（`AttackDir` 在 `GetPoseCreate()` 之后才 `Dir = nDir`），
   与本宿主的取法不同 —— 该差在 `bins/game-svr`（本写集），但**改它会动真机攻击语义**，
   本片按「一片一逻辑」不动它，**登记回去**由 owner 决定是否单开片。

另有一条**夹具假象**（m3bag 写集）：`attack_raw.jsonl` 合并了两个 bot 的日志，而生产的
`role=attack` 行**没有 `my` 字段** ⇒ `attack_diff.py` 的 `my=None` 过滤失效 ⇒ 同一条
`SM_STRUCK` 被广播给两个 bot、各记一次，`hits/sum` 被**重复计数**（本次 C# 的
`hits=2 sum=4` 实为 1 次 `dmg=1` 的事件被记 4 次）。这会让「跨侧逐 k」判据失真，属 m3bag。

⇒ 本片的 DoD：**攻方属性装载缺口已闭合（真机 Rust 从 0 击 → 19 击、非平凡门绿）**；
**`diff=0` 仍待第 1/2 条（夹具/owner 裁决）**。

## 5. 门禁（隔离 `CARGO_TARGET_DIR=E:/tmp/m3cs-b1b2-target`，`--offline`）

- `cargo fmt --all -- --check` rc=0
- `cargo clippy --workspace --all-targets -- -D warnings` rc=0
- `cargo test --workspace` **79 suites / 657 passed / 0 failed / 16 ignored**（+1 = 本片新用例）
- `cargo run -p msgcodegen -- --check` **604**

见 `gates.txt`（全量 `gate-clippy-full.txt` / `gate-test-full.txt`）。

## 6. 窗口纪律

起栈前实拍端口：仅共享 `3306` 在听；C# 栈与两轮 Rust 栈均 `-Stop`，收工后
5000/5100/5500/5600/5700/6000/7000/7100/7200 全部 DOWN。**窗口已释放。**

## 7. 残留物清单

- 保留：`E:/tmp/m2svr-k2/`（本片原始日志、pre/post 二进制、gate 全量输出）；`E:/tmp/m3cs-b1b2-target/`（隔离 target，热缓存）。
- 清理：`mir2-rs/tests/parity/m3/sessions/{csharp-20261012-170110,rust-20261012-171710,rust-20261012-171957,rust-20261012-170802}`（未跟踪的夹具输出，已择要入库本证据包后删除）。
- 未动：`src/**`、表结构/脚本语法/数据格式、`whitelist.md`、`crates/world/**`、`tests/parity/m3/**` 夹具。
