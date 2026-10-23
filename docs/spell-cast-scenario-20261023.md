# R5 场景 `spell_cast`：CM_SPELL 成功/失败两支

- 卡片：`openmir2-scenario-spell-cast`（tier=L）
- 交付物：`tools/probe/scenarios/spell_cast.json` + `tools/probe/scenarios/README.md` 段 + 本文
- 状态：**夹具已解 / 场景可 dry-run / 真机窗口待授权**（卡 `openmir2-spell-cast-unblock`，
  pin = master `bb4317a01`）。`characters_magic` 夹具由卡 `openmir2-window-magic-fixture`
  （master `512d9ecf5`）提供，**通过 profile 的 `magic` 段声明**（§7）；profile 不声明时仍
  fail-closed 必红（设计意图：夹具缺失必须显式红，不许假绿）。真机双栈对拍**尚未跑**。

---

## 1. 目标与不覆盖

**验什么**：同一串 `CM_*` 把 C# oracle 与 Rust 两侧推到同两条施法路径上——

| 支 | 触发条件 | 硬门帧 |
|---|---|---|
| 成功 | 已学魔法 ∧ 距离 ≤ `MagicAttackRage` ∧ 蓝够 ∧ 版本门开 | `SM_MAGICFIRE(638)` |
| 失败 | 同上但目标距离 > `MagicAttackRage`（**确定性**失败，零随机） | `SM_MAGICFIRE_FAIL(639)` |

**不覆盖**（如实登记）：

- **伤害数值 / RNG 抽数**：两发都打空地面（目标 `ActorId=0`）。火球术命中支里的
  `Random(10)`（`MagicManager.cs:180`）、`GetAttackPower` 抽数**一次都不发生** —— 本场景不产 RNG 证据。
- **`SM_MAGIC_LVEXP(640)` 技能升级**：只有 `Level < 3` **且** `boTrain`（命中/有效果）时才走训练尾
  （`MagicManager.cs:659-668`），且其中 `Random(3)+1` 是抽数。本场景两发都无目标 ⇒ 恒不发 640。
- **`SM_SPELL(17)`**：见 §5，**施法者自己永远收不到**，本场景不验。
- **跨图 / 回召 / 召唤 / Buff**：归各自的卡（`openmir2-recall-slave-shift-live`、
  `openmir2-buff-system` 等），本场景只提供「CM_SPELL 能出帧」的最小前置。

## 2. 选样魔法

| 项 | 值 | 出处 |
|---|---|---|
| 魔法号 | `1` | `sql/mir2_data.sql:70`（`magics` 表定义见 `sql/mir2_data.sql:42-66`；加载器 `src/GameSrv/DB/MySqlDB.cs:105` `LoadMagicDB`，SQL = `select * from magics`） |
| 名称 | 火球术 | 同上 |
| 目标职业 | `Job=1`（法师） | 同上第 12 列 |
| 是否需先学 | **是** | 服务端施法前查 `GetMagicInfo(nKey)`（`src/M2Server/Player/PlayObject.Attack.cs:1065-1069`，实现 `PlayObject.cs:4174`），查 `MagicList`（= DB `characters_magic` 按 `PlayerId` 载入，`WorldServer.cs:1210-1230`）。空 ⇒ `ClientSpellXY` 返回 false（`delayTime=0`）⇒ 只回 `SM_MOVEFAIL(28)` |
| 等级 | `0` | 单次耗蓝 = `Round(Spell/4*(Level+1)) + DefSpell` = `Round(4/4*1)+1` = **2**（`PlayObject.Base.cs:1369-1372` + `:1511-1519`） |
| 其它关键列 | `EffectType=1`、`Effect=1`（进 638 的 `MakeWord(EffectType,Effect)`）、`Delay=60`（进 `MagicAttackInterval`） | 同上 |

选它而不是 `基本剑术`(MagID 3)：后者是**战士技能**，`MagicManager.IsWarrSkill` 为真 ⇒
`PlayObject.DoSpell` 包装层整层跳过（`PlayObject.Base.cs:1510`），`ClientSpellXY` 走
`case SKILL_ERGUM`（`PlayObject.Attack.cs:1120-1135`）只发**明文** `+LNG`/`+ULNG`，
**零协议帧** —— 对「帧集合」判据无用。

## 3. `CM_SPELL` 的线上字段口径（与 `CM_TURN`/`CM_HIT` 不同，最容易写错）

`src/GameSrv/Word/WorldServer.cs:783-793`：

```text
SendMsg(playObject, CM_SPELL, wParam = defMsg.Tag,
        nParam1 = LoWord(defMsg.Recog), nParam2 = HiWord(defMsg.Recog),
        nParam3 = MakeLong(defMsg.Param, defMsg.Series))
```

处理器 `src/M2Server/Player/PlayObject.Message.cs:1339-1341`：

```text
ClientSpellXY(wIdent, wParam, (short)nParam1, (short)nParam2,
              ActorMgr.Get(nParam3), LateDelivery, ref delayTime)
```

⇒ 结论：

| 线字段 | 语义 |
|---|---|
| `tag` | **魔号**（`nKey`，`GetMagicInfo(nKey)` 的入参） |
| `recog` | **目标坐标** `MakeLong(X,Y) = (Y<<16)|X`（不是「X 放 recog、Y 放 param」——那是 `CM_TURN`/`CM_HIT` 的口径） |
| `param` + `series` | 拼成**目标 ActorId** = `MakeLong(Param, Series)`；无目标时两字段都写 0 |

## 4. 两支的因果链（逐行）

### 4.1 成功支 → `SM_MAGICFIRE(638)`

```text
PlayObject.Message.cs:1339      case CM_SPELL ⇒ ClientSpellXY(...)
PlayObject.Attack.cs:1054-1069  IsCanSpell / Death / STONE / GetMagicInfo(nKey)  （已学 = 前置）
PlayObject.Attack.cs:1070-1118  战士技能跳过限速；非战士走 CheckActionStatus + MagicAttackInterval
PlayObject.Attack.cs:1269-1282  default 支：Dir=GetNextDirection(...) ⇒ DoSpell(...)
                                false ⇒ SendRefMsg(RM_MAGICFIREFAIL)（失败支，见 4.2）
PlayObject.Base.cs:1505-1531    包装层：!IsWarrSkill ⇒ GetSpellPoint ⇒ MP 门 ⇒ 扣蓝 + RM_HEALTHSPELLCHANGED
MagicManager.cs:151-154         战士门（火球术：false）
MagicManager.cs:155-158         **距离门**：|CurrX-nTargetX| > MagicAttackRage ⇒ return false
MagicManager.cs:159             SendRefMsg(RM_SPELL, …)   ← 无条件、在距离门之后（只给**别人**看）
MagicManager.cs:166-168         boSpellFire=true；版本门 SoftVersionDateEx==0 && ClientTick==0 || MagicId>80
MagicManager.cs:172-643         switch(MagicId)：火球术 case（:174-203）在 targetObject==null 时什么都不做
MagicManager.cs:648-658         共享尾：boSpellFire ⇒ SendRefMsg(RM_MAGICFIRE, 0, MakeWord(EffType,Eff), MakeLong(X,Y), ActorId)
PlayObject.Message.cs:1788-1793 RM_MAGICFIRE ⇒ SM_MAGICFIRE（**无** self 守卫）⇒ 本连接收到 638
```

体：`SendSocket(ClientMsg, EDCode.EncodeBuffer(BitConverter.GetBytes(nParam3)))` —— `nParam3` 是
`targetObject.ActorId`（本场景 = 0）⇒ 4 字节小端全 0 再 EDCode。

**版本门对本场景恒开**：`SoftVersionDateEx = LoadUser.SoftVersionDate`（`WorldServer.cs:469`），
值 = GameGate 登录串里的客户端版本（`ChannelMessageHandler.cs:329`
`SoftVersionDate = nClientVersion`）；探针 `--client-version` 默认 `1200409180`
（`tools/probe/mir2_probe.py:99` = `src/OpenMir2/Grobal2.cs:5`）≠ 0 ⇒ 门不拦。
**`ClientTick` 恒 0** 不是问题：门是 `A==0 && B==0`，A 非 0 即放行。

### 4.2 失败支 → `SM_MAGICFIRE_FAIL(639)`

```text
MagicManager.cs:155-158        距离门失败 ⇒ return false（**在 MP 扣减之后**，蓝已掉 2 点）
PlayObject.Attack.cs:1278-1282 if (!DoSpell(...)) SendRefMsg(RM_MAGICFIREFAIL, 0,0,0,0,"")
PlayObject.Message.cs:1794-1800 RM_MAGICFIREFAIL ⇒ SendDefMessage(SM_MAGICFIRE_FAIL, ActorId, 0,0,0)（无 self 守卫）
```

选**距离门**做失败支的理由：与随机、与目标、与等级全无关，且是唯一在「已学 + 蓝够 + 版本门开」
前提下**必然成立**的失败路径。其它两条失败路径不进场景：

- **蓝不足**（`PlayObject.Base.cs:1515`）：取决于角色 MP，基线角色值不可控，且它**在距离门之前** ——
  用它做第二发会把「距离门」这一支的判据污染掉。
- **魔号 > 80**（`MagicManager.cs:167`）：`magics` 表里有 > 80 的行，但要额外一份夹具，
  且 `Attack.cs` 的 8 个显式 case 会绕开。

## 5. 为什么 `SM_SPELL(17)` 不能进等待集合

卡片原文写「帧集合要同时覆盖成功与失败两支（`SM_SPELL(17)`/`SM_MAGICFIRE(638)` 与
`SM_MAGICFIRE_FAIL(639)`）」。**其中 17 这一半是错的**：`MagicManager.cs:159` 发的 `RM_SPELL`
到 `PlayObject.Message.cs:1454-1458` 时首句是

```text
case Messages.RM_SPELL:
    if (processMsg.ActorId != ActorId)   // ← 自己发的 RM_SPELL，ActorId == 自己
        ClientMsg = Messages.MakeMessage(Messages.SM_SPELL, …);
```

⇒ 施法者自己那条连接**永远**收不到自己的 `SM_SPELL(17)`。把 17 写进 `wait_for` / `wait_idents`
会让本场景**恒超时**，而且看起来像「服务端不发帧」，极难定位。
`RM_MAGICFIRE` 与 `RM_MAGICFIREFAIL` 的处理器**没有**这个守卫（`:1788-1793` / `:1794-1800`），
所以**只有 638 / 639 是自己可见的**。Rust 侧现状与此一致（`lib.rs` 的
`broadcast_ref_msg` vs `broadcast_ref_msg_with_self` 两个函数的分工注释）。

> 旁注：17 不是完全不可验——**旁观者视角**能看到。但那需要第二条探针连接（多会话能力归
> `openmir2-multi-session-probe`），本卡写集不含，登记在 §9。

`SM_MOVEFAIL(28)` 同理**可以**进集合（它是「未学魔法」那支的可见帧，
`PlayObject.Message.cs:1468-1474`，无 self 守卫），但本场景按「已学」设计，不把 28 当判据：
一旦 28 出现就说明夹具没生效，而 `wait_for 638/639` 的写法会把它当「没等到」，语义更清楚。

## 6. 可达性矩阵（部署实测值）

部署常量（`E:/tmp/conformance/{csharp,rust}-root/M2GameSvr/Server.conf`）：
`SoftVersionDate=20020522`（:316）、`ActionIntervalTime=300`（:330）、
`MagicHitIntervalTime=1350`（:326）、`MagicAttackRage=8`（:755）、`SpellSendUpdateMsg=1`（:838）。

| 门 | 位置 | 本场景取值 | 结果 |
|---|---|---|---|
| `GetMagicInfo(nKey)` | `PlayObject.Attack.cs:1065` | 火球术；**需 profile `magic` 段**（§7） | ✅ 夹具已落地（离线 rc=0；真机待窗口） |
| 蓝门 | `PlayObject.Base.cs:1515` | 2 点/发，两发 4 点 | 需角色 MP ≥ 4（基线角色满足，见 §9-5） |
| 距离门 | `MagicManager.cs:155` | 第一发 5 格、第二发 9 格（阈值 8） | 第一发过、第二发必挂 |
| 版本门 | `MagicManager.cs:167` | `SoftVersionDateEx=1200409180` | 恒开 |
| 战士门 | `MagicManager.cs:151` | 火球术非战士技能 | 不触发 |
| 施法间隔 | `PlayObject.Attack.cs:1112-1118` | `sleep 1600` ≥ 1410 | 过 |

地段清场：`0122.map` 为 51×55，`(29,33)/(29,28)/(29,42)` 三格 `bk=0/fr=0` 可走
（口径 = `card_window.read_map_cell`，与 oracle 登录期 `CanWalk(x,y,true)` 一致）；
`MonGen.txt` 无 `0122` 行（零刷怪）、`GuardList.txt` 的卫士全在 map 0、`MerChant.txt` 无 `0122` 行
⇒ 两次施法之间不会被敌对目标打断（对照 `upgrade_retrieve` 踩过的 ArcherGuard 坑）。

## 7. 夹具（profile `magic` 段；缺声明则场景 fail-closed）

**状态（2026-10-24，卡 `openmir2-spell-cast-unblock`）：夹具已解。** 夹具实现随卡
`openmir2-window-magic-fixture` 合入 master `512d9ecf5`（本文 pin = master `bb4317a01`）：
`tools/conformance/card_window.py` 的 `profile_magic`（:1863）、`run_magic`（:1943）、
`preview_magic`（:2059）；声明形状在加载期 fail-closed 校验（`_validate_magic_list`，:1817，
`check_profile` 对两侧各调一次，:687-689）。

**profile 必须声明**（顶层 `magic` 同时应用到两侧；`sides.csharp.magic` /
`sides.rust.magic` 可覆盖本侧）：

```json
"magic": [{"magic_id": 1, "level": 0, "use_key": "1", "curr_train": 0}]
```

- 四列必填，键名是 **snake_case**（`magic_id`/`level`/`use_key`/`curr_train`），不是 DB 列名
  `MagicId`/`Level`/`UseKey`/`CurrTrain`；未知键 ⇒ rc=2（fail-closed）。
- `use_key` 必须是单个 ASCII 可见字符（本场景 `"1"`）；`level` 0..3；`magic_id` 1..65535。

**预置确实在起栈之前**：真跑 `restore → placement → magic → 端口检查 → stack`
（`card_window.py:2181` restore、`:2191` placement、`:2208-2219` magic、`:2229` 端口、`:2241` stack）；
`--dry-run` 同一顺序（`:2864` placement、`:2881` magic、`:2897` stack）。`run_magic` 写的是各侧
**测试库**（`mir2_test_db` / `mir2_test_db_r`）的 `characters_magic`，写后读回校验
`MagicId/Level/UseKey/CurrTrain`，并校验未声明行的行数/MD5 指纹不变。

**等价的手工 SQL**（两侧各一份；走 profile 时不需要手工执行，仅供核对）：

```sql
INSERT INTO characters_magic (PlayerId, MagicId, Level, UseKey, CurrTrain)
VALUES (6228, 1, 0, '1', 0);
```

（`PlayerId` 由夹具按 profile 的 `char_name`/`account` 在 `characters` 反查 `Id`，不写死 6228。）

- 列义见 `sql/mir2_db.sql:371912-371922`：`PlayerId` / `MagicId` / `Level`（0..3）/ `UseKey`（char(2)）/ `CurrTrain`。
- `Level=0` 是刻意的：耗蓝 2 点；`Level<3` 只影响**训练尾**（本场景无目标 ⇒ 不触发），不引入 RNG。
- 基线 `E:/tmp/conformance/baseline/mir2_test_db.sql:88` 的 `characters_magic` 段只有
  PlayerId 6226/1/13339 三组，没有 6228 ⇒ **profile 不声明夹具时本场景必红**（fail-closed，不假绿）。
- **不建议**在场景里用 `@TrainingMagic` 代替 DB 夹具：`TrainingMagicCommand`
  权限 10（`src/Modules/GameCommand/Commands/TrainingMagicCommand.cs:12` 的 `[Command(..., 10)]`），
  而部署 `Envir/AdminList.txt` 只有 `*	阿瑞斯乐园` 一条 ⇒ 探针角色不是 GM，命令会被拒；
  即便放开，Rust 侧 `gmcmd::training_magic` 的落库路径与 oracle 是否逐字节一致也**未验**，
  那会把「夹具问题」变成「GM 命令 parity 问题」，判据更糊。

**内置 `alt` profile 没有 `magic` 段**（pin `bb4317a01` 的 `BUILTIN_PROFILES["alt"]`，
`card_window.py:287-355`）：要看到 magic 预置步骤，必须用
`--profile <声明了 magic 的 profile.json>`。本卡证据用的 profile 是
`E:/tmp/spellcast-profile.json`（= 内置 alt + 上面的 `magic` 段；临时物，见 §9-6）。
**只写 `spell_cast.json` 不能让 magic 预置出现** —— 场景文件不承载夹具，夹具声明在 profile。

### 7.1 离线证据（本卡实跑，2026-10-24）

```text
$ python -I -B tools/conformance/card_window.py --selftest
GREEN: card_window.py selftest 216/216          # MG-1..MG-7 全 PASS

$ python -I -B tools/probe/mir2_probe.py --check-scenario     --scenario tools/probe/scenarios/spell_cast.json
scenario OK: spell_cast (5 steps, params=far_pos=2752541,magic_id=1,near_pos=1835037,self_pos=2162717)   # rc=0

$ python -I -B tools/conformance/card_window.py --dry-run     --scenario tools/probe/scenarios/spell_cast.json     --card openmir2-spell-cast-unblock --out E:/tmp/spellcast-dry
[csharp] restore: bash E:/tmp/conformance/restore-baseline.sh
[csharp] 预置位（dry-run 不连库）：map=0122 x=29 y=33 db=mir2_test_db 图=0122.map 51x55 可走
[csharp] 魔法夹具（dry-run 不连库）：db=mir2_test_db magic_ids=1
[csharp] 端口预检：[15000, ...]
[csharp] stack: powershell ... start-csharp3.ps1
[rust]   restore: bash E:/tmp/conformance/restore-baseline-r.sh
[rust]   预置位（dry-run 不连库）：map=0122 x=29 y=33 db=mir2_test_db_r 图=0122.map 51x55 可走
[rust]   魔法夹具（dry-run 不连库）：db=mir2_test_db_r magic_ids=1
[rust]   端口预检：[15000, ...]
[rust]   stack: powershell ... start-rust.ps1
VERDICT: DRY-RUN ...                             # rc=0
```

注意：上面 `魔法夹具` 两行来自 `--profile E:/tmp/spellcast-profile.json`；用默认 `alt`
profile 跑同一条命令也是 **rc=0**，但**不会**打印这两行（alt 没声明 `magic`，夹具被静默跳过）。
这不是 bug，是「夹具只由 profile 声明」的直接后果；文档必须把这一点写死，否则别人照命令跑会以为
夹具没生效。

### 7.2 Rust 侧 638/639 的现有用例（pin = master `bb4317a01`，五个用例实跑全绿）

| 覆盖 | 用例（`cargo test -p game-svr --lib '<名>'`） | 断言行（`mir2-rs/bins/game-svr/src/lib.rs`） |
|---|---|---|
| 成功支 638（魔号 1、无目标、零抽） | `tests::spell_cmd_version_gate_double_zero_then_1018_recovers`（:50493） | 调用 `:50566-50584`（`tag=1`、`param=series=0` ⇒ target=0）；帧面 `:50589-50603` 断言自己+观察者各 1 条 `SM_MAGICFIRE`；`:50606-50608` 零抽 |
| 成功支 638（魔号 1/5、带目标） | `tests::spell_p3b1_fireball_family_draw_order_and_frames`（:49719） | `:49742` 观察者恰 1 条、`:49747` 自己恰 1 条；`:49759-49765` ActorId/坐标 |
| 距离门失败 639 | `tests::spell_cmd_distance_gate_failure_emits_exactly_one_639`（:47726） | `:47736-47737` 恰 1 帧、`:47741` 发给自己、`:47744-47745` ident=639 且逐字节、`:47766-47770` 零抽 |
| 距离门失败 639（P1 魔号集） | `tests::spell_tail_p1_out_of_range_sends_only_639`（:49447） | 越界 ⇒ 只 639、无尾帧 |
| `SM_SPELL` 自身不可见 | `tests::spell_frame_never_reaches_caster_connection`（:46848） | 施法者那条连接收不到自己的 17 |

`spell_cmd_distance_gate_failure_emits_exactly_one_639` 用的魔号是 14/8 而非 1：**距离门在
`switch (MagicId)` 之前**（`MagicManager.cs:155` 早于 `:171` 的 switch），对魔号无依赖；
魔号 1 的可用性由上面两行 638 用例覆盖。

## 8. 牙（mutation，全部实跑）

`--check-scenario` 是加载期门禁；下表是把它咬红的实际手段（`E:/tmp/mut-spell/m*.json` 为就地生成的副本）：

| # | 改法 | rc | 判据来源 |
|---|---|---|---|
| M1 | 删掉**所有** `send` 步 | **2** | R2「wait_for 之前必须已有发帧步骤」 |
| M2 | 只删**第二发** `send` | 0 | **已知缺口**：R2 只要求「之前存在过某个发帧步」，不做 producer→consumer 映射（`README` 的 R2 条已登记，归 `openmir2-probe-scenario-validation-gaps` 的 R2-producer-map） |
| M3 | `wait_for 638` → `4095`（协议表外 ident） | **2** | R3 |
| M4 | `wait_for 638` → `17`（表内 ident，但被 self 守卫滤掉） | 0 | **静态门禁抓不到**：这是语义错，靠 §5 与本文挡住；若有人改回 17，真窗口会恒超时 |
| M5 | `self_pos` 改成与 `placement` 不一致 | **2** | R5 |
| M6 | 把 `blocked` 加回（本卡反证牙，实跑） | **2** | card_window BL 硬门（selftest BL-1：`scenario is BLOCKED: <reason>`） |

⇒ 牙 ≥ 1 成立（M1/M3/M5/M6 四颗）。M2/M4 是**登记在案的缺口**，不是本卡范围。

## 9. 残留与登记

1. **夹具契约**（§7）：profile 未声明 `magic` 段时两发都只回 `SM_MOVEFAIL(28)`，本场景 fail-closed 必红；
   夹具已随 `openmir2-window-magic-fixture` 落地，**声明位置是 profile 不是场景**。
2. **`SM_SPELL(17)` 自己不可见**（§5）：要验需第二条连接 ⇒ `openmir2-multi-session-probe`。
3. **R2-producer-map 缺口**（M2）：删单个 `send` 不红，归 `openmir2-probe-scenario-validation-gaps`。
4. **RNG 盲区**：本场景零抽数，**不能**作为伤害/命中/技能升级（640）的对拍输入；
   这些仍需 `openmir2-buff-system` / `openmir2-healthspell-emission-model` 等 R 卡在窗口里跑并开
   `MIR2_RNG_LOG` 双开对拍。
5. **蓝门下界**：耗蓝 2 点/发，靠「基线角色 MP ≥ 4」这个未显式断言的假设。若后续换更弱的夹具角色，
   第一发会在蓝门（`PlayObject.Base.cs:1515`）就挂、且**同样回 639** —— 与距离门那一支同帧不同因，
   届时需在场景 `note` 里补一条 MP 断言（当前无 DSL 表达，登记）。
6. **临时物（本卡）**：`E:/tmp/spellcast-profile.json`（= 内置 alt + `magic` 段，
   仅用于 dry-run 取证，可删）、`/tmp/dryrun-default.txt` / `/tmp/dryrun-magic.txt`（dry-run stdout）。
   `--dry-run` 不建 `E:/tmp/spellcast-dry`（该目录本卡跑完仍不存在）。
