# evidence-m3-target-availability-20261012 —— M3②③「C# 目标格从 k=4 起为空」的定位 + 逐击状态记录 + 目标可用性 fail-closed 门

- 卡：`openmir2-m3-attack-pair-fixture`（issue `76cc08b8`）／车道 `m3bag`／principal `openmir2-worker-4`。
- 来源：owner 2026-10-12 **23:50** brief 顶部块（`E:/tmp/openmir2-watch/brief-m3-bag-equip.md`）第 ①–③ 条，
  承接 m3cs 的 `M3②③攻击success调用条件与请求序列定位报告-20261012.md`（已合 master `ba8ca1c6`，owner 接受）。
- 写集：`src/BotSrv/Player/RobotPlayer.cs`（T-29 扩展，**只新增夹具标记**，默认关路径逐字节不变）、
  `mir2-rs/tests/parity/m3/{rng-hook/StartupHook.cs, run_pair.ps1, run_attack_pair.ps1, start-csharp-hook.ps1, server_state_gate.py}`；
  **零 `.rs`**、未动 `crates/**`/`bins/**`/`sql/**`/`MirClient`；未改动 oracle 七进程（C# 侧只用既有影子 hook）。

## 0. 一句话结论

**C# 侧「目标格 (18,20) 从 k=4 起为空」不是格子空了，而是受害者的服务端 `Ghost` 标志被静默置 1**
（位置/地图/HP 都没变：DumpActors 实测 `x=18,y=20,map=0137,ghost=1,death=0`）。oracle
`Envir.GetMovingObject(nX,nY,boFlag=true)` 对 `Ghost` 直接返回 null（`src/M2Server/Maps/Envirnoment.cs:1203`）
⇒ `PlayObject.Attack.cs:26` 的 `targetObject ?? GetPoseCreate()` 取到 null ⇒ `_Attack`（`BaseObject.Attack.cs:32`）
与 `AttackSuccess`（`PlayObject.Attack.cs:217`）提前返回 ⇒ 专用流里玩家 `attack_success` 只 **3** 次。
⇒ m3cs 的「目标格从 k=4 起为空」定位**方向对、术语偏**：格上有对象，只是该对象 ghost 了。

**与 pin 的关系**：同一夹具、同一图/落点，只换 pin 的 `Level/Hp`——
- `Level=1, Hp=19`（旧口径，被 `RecalcLevelAbilitys` 重算成 MaxHP=19）⇒ **受害从 #7 起 ghost=1**；
- `Level=255, Hp=20669`（F-F pin，已合 master `74165893`）⇒ **受害 120 击全程 `ghost=0`**，
  目标可用性门 **GREEN 120/120**。

⇒ **F-F 的「Level/Job/Hp 自洽 pin」不只是把受害者 HP 抬高，它同时消掉了这个 ghost**：
两侧（C#/Rust 用同一夹具同一 pin）受害者都在场且 `!ghost && !death`，「玩家有目标的击数」结构一致。

## 1. 本片交付

| 件 | 位置 | 作用 |
| --- | --- | --- |
| 逐击状态记录（客户端） | `src/BotSrv/Player/RobotPlayer.cs::M3AttackStateJson` + `AttackSequenceTick` | 每击**发 `CM_HIT` 之前**落 `M3ATTSEQ {"role":"state","k":N,"attacker":{x,y,dir,ghost,death,hp},"victim":{…}\|null,"fx","fy","vid","others"}`（m3cs `per_strike_requests.py jointable` 的接口 schema）；只读**本假人自己**的视图（`_probeX/_probeY`/`_myActorId`/`PlayScene.ActorList`；**不用进程级共享的 `MShare.MySelf`**——多假人同进程时它被最后处理包的假人覆盖，本片实测会把攻方读成受害） |
| 逐击状态记录（服务端权威） | `rng-hook/StartupHook.cs`（`MIR2_RNG_STATE_LOG` → `SERVER_STATE <seq> <json[]>`） | 每个玩家攻击起点（`PlayObject.GetAttackPower@PlayObject.cs:2929`）把 `SystemShare.ActorMgr` 里全部玩家族对象的 `id/name/x/y/dir/ghost/death/hp/maxhp/map` 落一行；**能咬住客户端看不到的 ghost** |
| 目标可用性 fail-closed 门 | `server_state_gate.py` + `run_attack_pair.ps1 -CsharpStateLog` | 取服务端 dump 最后 `-AttackCount` 行，逐击要求攻方前方格上有非攻方、`!ghost && !death` 的玩家族对象（容 1 击起跑竞态）⇒ 否则 RED 点名 k |
| 会话留痕 | `run_pair.ps1` | 新正则 `M3ATTSEQ (\{[^\r\n]*\})`（旧 `\{[^{}]*\}` 会把嵌套 `role=state` 行整条丢掉）+ 写 `attack_state.jsonl` + `state==attack`/`k` 无洞 fail-closed |
| 起栈口 | `start-csharp-hook.ps1 -StateLog`（缺省 `<LogDir>\rng-server-state.tsv`，写 `MIR2_RNG_STATE_LOG`）；顺手修 `-Stop` 对已退出 pid 抛错中断、`-Repo` 传参时 `$Here` 为 null 两个夹具缺陷 | 让服务端 dump 可开、收工可靠 |
| 顺手修 | `run_attack_pair.ps1` 的 `-EmitOpsArgs` 探针与侧运行**漏传 `-AttackVictimLevel`** ⇒ 显式覆盖等级时（如 L1/19）探针用缺省 L255 与 `Hp=19` 不自洽而 throw（`Get-M3AttackPinSql` fail-closed） | 让 `-AttackVictimLevel` 真能覆盖 |
| 登记 | `whitelist.md` T-29 第十二轮 | BotSrv 夹具面扩展（默认关路径不变） |

## 2. 判据与证据

| 文件 | 内容 | 结果 |
| --- | --- | --- |
| `01-server-state-L255-green.txt` | L255/20669 服务端 dump（每击受害 `ghost/death/hp/x/y/map`） | 120 击全 `ghost=0 death=0 @18,20 map=0137` |
| `02-server-state-L1-red.txt` | L1/19 服务端 dump | 受害从 #7 起 `ghost=1`（位置/地图/HP 不变） |
| `03-client-vs-server.txt` | 客户端视图 vs 服务端视图逐击对照 | 客户端 119/120 击都「看见」受害；服务端 #7 起已 ghost ⇒ **只靠客户端记录会漏判** |
| `04-gate-selftest.txt` | `server_state_gate.py --selftest` | **GREEN 6/6**（生产形态绿 / ghost ⇒ 红点名 k / 移格 ⇒ 红 / 行数不足 ⇒ PREREQ 2 / 非法 JSON ⇒ PREREQ 2 / tolerance=0 首击缺 ⇒ 红） |
| `05-gate-live.txt` | 真机 L255 GREEN + L1 RED 两次 `run_attack_pair` 输出 | L255 rc=0 `目标可用性门通过（120/120）`；L1 rc=1 `RED: 目标在 114/120 击不可用 … #7…` |
| `06-selfcheck-nostack.txt` | `run_attack_pair.ps1 -SelfCheckNoStack` | `SELFTEST_RED GREEN`（含 ⑦b） |
| `07-gates.txt` | 四门禁（隔离 `CARGO_TARGET_DIR`） | fmt/clippy/test/msgcodegen |

### 2.1 oracle 链路（唯一目标解析）

```
PlayObject.AttackDir(null, 0, nDir)                 # ClientHitXY（PlayObject.Attack.cs:909）
  attackTarget = targetObject ?? GetPoseCreate()    # :26  ← 唯一目标解析
      GetPoseCreate() → Envir.GetMovingObject(fx,fy,true)   # BaseObject.cs:1253 → Envirnoment.cs:1203
          # 遍历该格 ObjList，要求 baseObject != null && !baseObject.Ghost
          #   （boFlag=true ⇒ 还要 !baseObject.Death）
  GetAttackPowerHit(..., GetBaseAttackPoewr(), attackTarget, ref canHit)  # :73（攻击力照抽，与目标无关）
  AttackDir(attackTarget, nPower, nDir) → _Attack      # BaseObject.Attack.cs:32
      IsProperTarget && target.HitPoint>0 ⇒ RandomByte(SpeedPoint)     # :41 玩家门只 3 次
  AttackSuccess(wHitMode, nPower, canHit, attackTarget) # :77/:217：targetObject==null ⇒ return（无 Random(5)）
```

⇒ 受害 `Ghost=1` 时：无玩家门、无 `attack_success`，但 `atk_power_a/b` 仍 120 次（攻击力照抽）——
与 m3cs 观测的数值结构逐条吻合。

## 3. 未做 / 边界（如实登记）

1. **触发 `MakeGhost()` 的精确 oracle 分支未钉死（candidate，不宣称已定因）**：服务端 `Ghost` 只能由
   `BaseObject.Base.cs:492`（`MakeGhost`）置 1；`GameSvr.out.log` 在该时刻**无任何异常行**，dump 显示
   `death=0` ⇒ 排除 `KickException`（会先置 `Death`）与「死亡后 `MakeGhostTime`」。剩下静默路径是
   `PlayObject.Message.cs:375` / `PlayObject.cs:1088` 的 `if (BoEmergencyClose || BoKickFlag || BoSoftClose) MakeGhost()`。
   本片**不改冻结 `src/**`**，故未在 oracle 内插桩追栈；触发点与 pin 的 `Level/Hp` **高度相关且可复现**
   （L1 必 ghost、L255 必不 ghost），但「为什么 L1 会走到该分支」仍待后续（或由 owner 决定是否接受
   F-F 的「抬高等级避开」为最终口径）。已登记的关联线索：登录内容脚本链
   `QManage.txt [@Login] → 月卡/代练/假人系统`，以及 `SETOFFLINEPLAY ON`（`ExecutionProcessingSys.cs:2716`）。
2. **客户端视图看不到 ghost**：受害被 `MakeGhost` 后，观察者（攻方）客户端仍保留该 actor（实测 119/120 击
   都在 `ActorList` 里）⇒ 纯客户端状态记录**不能**当目标可用性判据；本片因此以**服务端 dump** 为门。
   这是 m3cs `jointable` 需要注意的口径（它拿 `attack_state.jsonl` 只能看客户端面）。
3. **Rust 侧无同形服务端 dump**：`bins/game-svr` 归 m2-svr；本片的目标可用性门只在 `-Side csharp` 强制。
   两侧「玩家有目标击数一致」由 **F-F pin 的自洽 Level/Job/Hp** 保证（同一夹具同 pin）；Rust 侧若要同等
   强度的门，需 m2-svr 补一个同 schema 的 dump（建议另开卡，不在本片写集）。
4. **F-F 修复本片未改**：F-F（`gate_guards.ps1`/`run_pair.ps1`）已合 master `74165893`；本片只**证明**它
   同时消掉了 ghost + 目标可用性回升到 120/120，并把它变成 fail-closed。
3. **`-Side rust` 的目标可用性门不做**（无 dump）；本片不降低任何既有判据。
