# M3②③ 真机同种子攻击对拍夹具（骨架片）——证据包

- **卡**：`openmir2-m3-attack-pair-fixture`（issue `76cc08b8`），related `openmir2-m3-combat-skills-align`。
- **本片范围**：**不占端口**的那半——`run_attack_pair.ps1` 骨架 + BotSrv 固定攻击序列（白名单 T-22 扩展）+
  度量口径 + `-SelfCheckNoStack` + **改坏必红** + 四门禁。**真机两侧跑等 owner 窗口**（起栈前实拍端口）。
- **纪律**：未起任何服务、未占端口、未 merge、未自签 review；`src/**` 仅按 T-22 扩展登记地改 BotSrv（夹具专用、默认关）。

## 1. 交付物（路径 + 说明）

| 路径 | 说明 |
| --- | --- |
| `mir2-rs/tests/parity/m3/run_attack_pair.ps1` | 攻击对拍一条命令：复用 `run_pair.ps1` 两相 pin/会话/`scoped_diff`；加 `attack_diff.py align/diff`；`-SelfCheckNoStack` |
| `mir2-rs/tests/parity/m3/attack_diff.py` | 逐次伤害序列 + 命中/躲闪对齐与跨侧比对（`align`/`diff`/`mutate`/`selftest`）；**非平凡门**防恒 0 比较器 |
| `mir2-rs/tests/parity/m3/run_pair.ps1`（改） | 增 `-AttackSequence/-AttackCount/-AttackStepMs/-AttackStartDelayMs/-AttackPinPair/...`；`-EmitOpsArgs` 暴露 `attack_sequence`/`attack_pose`/`attack_pin_sql_all`；baseline 写 `attack_*`；`M3ATTSEQ` 完整性机判 |
| `mir2-rs/tests/parity/m3/gate_guards.ps1`（改） | 增 `Format/Convert/Get-M3PeerAttackScopeGuard`、`Get-M3PeerAttackPoseGuard`、`Get-M3AttackPinSql`（唯一口径，自检共用） |
| `tools/botload/run_bots.ps1`（改） | 增 `-AttackSequence/-AttackCount/-AttackStepMs/-AttackStartDelayMs` 透传到 `AppSetting.json` |
| `src/BotSrv/{RobotOptions,MShare,AppService}.cs`、`Player/RobotPlayer.cs`、`Player/RobotPlayer.Operate.cs`（改） | 固定攻击序列驱动 + `SM_STRUCK` 落 `M3ATTSEQ` 标记（**仅 `g_boAttackSequence=true` 时生效**；默认关路径逐字节不变） |

## 2. 度量口径（详见卡 comment；判据认它）

- **伤害来源** = 受击方收到的 `SM_STRUCK` 的 `Series`（oracle `MakeMessage(SM_STRUCK, ActorId, HP, MaxHP, wParam)`
  的 series=本次伤害，`src=MessageBodyWL.Tag1`=命中者）。不用 attacker 的 `SM_HIT`（动作帧无伤害数）、
  不用 DB 快照（怪物不入库、玩家 HP 只在断线存档落库）。
- **命中/躲闪** = 按「第 k 次」：attacker 第 k 步发出后、下一步之前收到 `src==attacker` 的 `SM_STRUCK` ⇒ hit。
  比 `attacked/hits/dodges/sum` 四项。
- **非平凡门**（防恒 0/恒绿）：两侧都必须 `attacked>=1 && hits>=1 && sum>0`，否则 RED。
- **目标接法**：`-AttackPinPair` 把 attacker（账号序号 0）钉 (289,618) 面向 dir=2，victim（序号 1）钉 (290,618)，
  victim `Hp=MaxHP=100000` 防序列内死亡。
- **前置（登记，不在本卡写集）**：Rust `game-svr` 侧同种子注入（C# 用既有 `m3/rng-hook`）；
  未接前真机两侧伤害数值不可比（本片不假装可比）。

## 3. `-SelfCheckNoStack`（无栈、机判、fail-closed）

命令：

```
powershell -ExecutionPolicy Bypass -File mir2-rs/tests/parity/m3/run_attack_pair.ps1 -SelfCheckNoStack
```

原始输出见 `attack-pair-selfcheck-nostack.txt`（rc=0，GREEN）。覆盖：
① 攻击配置透传 on/off 两向；② 入口护栏 fail-closed（rc=2）；③ `-DiffOnly` 缺会话 fail-closed（rc=2）；
④ `run_pair -AttackSequence` 必带 `-DisableAutoPlay` 两向；⑤ `-AttackPinPair` 的 pose + 每 bot pin SQL；
⑥ `attack_diff selftest`；⑦ **改坏必红**；⑧ `scoped_diff selftest 14/14`。

同批回归：`run_bag_equip_pair.ps1 -SelfCheckNoStack` 仍 GREEN（见 `bag-equip-selfcheck-nostack-regression.txt`）——
证明本片对 `run_pair/gate_guards/run_bots` 的改动没有削弱 T-14 既有语义。

## 4. 改坏必红（生产函数级）

不靠字符串替换：合成两侧同序列 raw → 走生产的 `attack_diff.py align/diff` ⇒ GREEN；
再用生产的 `attack_diff.py mutate` 改坏 `k=5` 的一次 **dmg**（20→999）⇒ RED；还原后改坏一次 **hit**（hit→dodge）⇒ RED。
原始输出（含命令与退出码）见 `mutation-red.txt`。

## 5. 四门禁（隔离 `CARGO_TARGET_DIR=E:/tmp/m3bag-attack-target`）

| 门禁 | 原始输出 | 结果 |
| --- | --- | --- |
| `cargo fmt --all -- --check` | `gates-fmt.txt` | rc=0 |
| `cargo clippy --workspace --all-targets --offline -- -D warnings` | `gates-clippy.txt` | rc=0 |
| `cargo test --workspace --offline` | `gates-test.txt` | rc=0（无 FAILED） |
| `cargo run -p msgcodegen --offline -- --check` | `gates-msgcodegen.txt` | rc=0（604 条常量一致） |

另：`dotnet build src/BotSrv/BotSrv.csproj -c Release`（验证 T-22 扩展能编译）见 `botsrv-build.txt`（0 错误）。

## 6. 改前/改后行为（T-22 扩展登记口径）

- 新增 BotSrv 开关 `AttackSequence`（默认 `false`）⇒ **默认关时不再新增任何发送**，原路径逐字节不变
  （两向自检 + `AppSetting.json` 开关位钉住）。
- 打开时只写**本假人自己的 socket**（attacker 每 `AttackStepMs` 发一条 `CM_HIT`），`SM_STRUCK` 只**读**不写世界态；
  不读不写跨假人共享世界态（同 T-22 纪律）。

## 7. 文件

见 `SHA256SUMS.txt`（按仓库字节/git-blob 口径；`selfcheck-selfcontained.txt` 记录自包含性自检）。

**未做（如实登记）**：真机两侧跑（等 owner 窗口）；Rust game-svr 同种子注入（前置，属 m2-svr 写集）。
