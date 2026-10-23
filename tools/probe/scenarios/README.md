# 协议无头对拍：场景 DSL

本目录的 JSON 是 `tools/probe/mir2_probe.py`（无头客户端驱动器）的输入。
驱动器负责把同一串 `CM_*` 打进 Rust 栈与 C# oracle 栈，并把 s2c 帧逐帧落成证据；
**比较器是另一条车道**（比 `frames.ndjson` 与数据库后态），本目录不含判定逻辑。

```sh
python tools/probe/mir2_probe.py \
  --login-gate 127.0.0.1:7000 --account test01 --password test01 \
  --server-name 热血传奇 --char-name test01 \
  --scenario tools/probe/scenarios/gm_time.json \
  --out E:/tmp/probe-rust-gm-time
```

## 文件形状

```json
{
  "name": "唯一名字",
  "description": "给人看的一句话（可选）",
  "params": {"npc_actor": 119},
  "steps": [ ... ]
}
```

* 顶层只允许 `name` / `description` / `params` / `required` / `computed` / `placement` / `blocked` / `steps`；多写一个键即 **fail-closed**（退出码 2）。
  （`blocked` = 场景被封存：形状合法、`--check-scenario` 仍 rc=0，但 `card_window.py` 见到即 rc=2、不建 out/不起栈；schema 见 `docs/probe-scenario-blocked-gate-20261023.md`。）
  （`computed` 是给 `param_discovery` 的**声明性注记**，probe 不读它；见 `move_walk.json`。）
* `required`（可选）是**必填变量名列表**：`expand_scenario` 在合并完 `--var` 后逐名检查，缺任何一个即
  `scenario-error`（退出码 2），消息点名缺哪个变量。运行期才知道的值（NPC/商人 `ActorId`、`MakeIndex`、
  物品名、自身坐标）一律走这里，**不要**在 `params` 里放 `"REQUIRED"` 之类的哨兵值。
* 离线校验：`python -I -B tools/probe/mir2_probe.py --check-scenario --scenario <file.json> [--var k=v ...]`
  —— 只加载+展开，不连网；rc=0 通过、rc=2 场景错误。

  形状之外还做**语义校验**（`check_scenario()`；缺一条就不算「能真跑」，全部 fail-closed rc=2）：

  1. **R2 每个 `wait_for` 之前必须已有发帧步骤**（`send`/`say`/`click_npc`/`dlg_select`）。
     **真实覆盖口径（别误读——这条不是「删掉攻击步会红」的牙）**：R2 只保证「`wait_for` 之前
     **存在过**某个发帧步」，**不保证 producer 与 consumer 的对应关系**。只有 `wait_for` 之前
     **一个发帧步都没有**（例如把场景里的 **send 全部删掉**）才 rc=2；删掉**多个 send 中的某一个**
     （包括攻击步，如 `combat_strike` 的 `CM_HIT(3014)`、前面还留着 `CM_TURN`）**仍可能 rc=0**。
     要让「删掉攻击步」变红需要 **producer→consumer 映射**，而那是猜出来的因果：`SM_STRUCK(31)`
     既可能来自 `CM_HIT` 一族，也可能来自**挨打**（见 `docs/combat-strike-scenario-20261023.md`
     的 BLOCKED 分析：玩家挨守卫一箭也收 `SM_STRUCK`、`recog=self`）—— 硬映射会把合法场景判死。
     该能力**不在本卡范围**，已在卡 `openmir2-probe-scenario-validation-gaps` 上登记为残留
     **R2-producer-map**（建议开卡的描述见该卡 status）。
  2. **R1 至少一个发帧步或累计观察步**（`wait_idents`/`wait_frames`）——全是 `sleep`
     的场景既不发帧也不观察，什么也证不了。纯观察场景（如 `login_only`）靠驱动器的
     登录链 + 自动握手已经发帧，所以它合法。
  3. **R3 ident 必须存在于协议常量表，且方向必须对**：发帧动作（`send`/`say`/`click_npc`/
     `dlg_select`，C2S）必须是 **`CM_*`**；观察动作（`wait_for` / `wait_frames.idents` /
     `wait_idents.idents`，等的是 S2C）必须是 **`SM_*`**。方向反了与表外 ident 一样 = 永远发不出 /
     等不到（例：`send ident 4 is SM_41, expected CM_*`；`wait_for ident 80 is CM_QUERYUSERNAME,
     expected SM_*`），`--check-scenario` 直接 rc=2。表由 `mir2-rs/crates/protocol/src/messages.rs`
     解析（`msgcodegen` 与 C# `Messages.cs` 对拍），跨服 `ISM_*` **不进表**。
     **已知并集歧义**：`42/100/101/102/103/104` 这 6 个号在 CM 与 SM 两个命名空间里各有名字
     （如 `100 = CM_QUERYCHR = SM_SYSMESSAGE`），方向无法由号判定 ⇒ 两个方向都放行；
     清单与处理见 `docs/probe-ident-direction-20261023.md`。
  4. **R4 变量引用必须已声明**：`send.recog/param/tag/series`、`send.text_from`、`click_npc`、
     `dlg_select.recog` 里的变量名必须在场景的 `required` / `params` / `computed` 里；
     `--var` 是给**已声明**变量覆值的通道，不是声明通道（拿 `--var` 喂一个拼错的名字仍然红）。
  5. **R5 `placement` 与由位置派生的 `params` 必须自洽**：声明了 `placement` 且在生效 `params`
     里给了位置派生变量时（`--var` 覆值同样生效，值可以是**另一个 params 名的间接引用**），
     它们必须与 `placement` 对得上，否则 rc=2 并打印两个来源：
     * `self_pos` —— `MakeLong(X,Y) = (Y<<16)|X`（低 16 位 X、高 16 位 Y），解码出的 `(x,y)` 必须等于 `placement.x/y`；
     * `self_x` / `self_y` —— 必须分别等于 `placement.x` / `placement.y`。
     不一致意味着角色被放到 A 点、期望值按 B 点算，真跑会**静默恒超时**，`card_window --dry-run` 也看不出来（实战来自 `combat_strike` 复审）。
  6. **R6（仅警告，不改变 rc）攻击族的 `send.tag` 应是合法 Direction**：`HIT_IDENTS` 的 9 条攻击
     ident 的 `tag` = oracle 的 `nDir = (byte)Tag`，截断后应在 `0..7`（`Enums/Direction.cs`）。
     越界时只在 stderr 打一条 `WARN:`（点名 step/值），**rc 仍是 0** —— 因为 oracle **先按状态化
     Dir 取目标**（C# `PlayObject.Attack.cs:26` / Rust `bins/game-svr/src/lib.rs:8836-8848`），
     `nDir` 在取目标之后才写入（`BaseObject.Attack.cs:12`），所以越界方向**不影响本次命中**；
     它污染的是**此后**的状态 Dir（动作帧与下一次取目标），且真实客户端不会发送这种帧。
     ident 名单与方向值域都**解析 `mir2-rs/bins/game-svr/src/attackwire.rs`**（`HIT_IDENTS` / `DIR_*`），
     不写私有表；非攻击族不受这条约束。
  7. **W 至少一个等待步骤**（原有的 N-5 规则，已并入 `check_scenario()`）。

  上面 R1–R5 是硬判据（rc=2），R6 是警告（rc 不变）；这些都只在 `--check-scenario` 生效。另：本卡把 `dlg_select.recog` 也改成了**加载期解析**
  （`resolve_param` + i32 范围，与 `send.recog` 同口径/PRB-4），这条落在 `plan_step`，真跑也受益。
* `steps` 非空；每一步是对象，**恰好一个动作键**，外加可选元键 `timeout_ms` / `delay_ms` / `note`。

## `placement`：角色必须位于 map/x/y（由 `card_window.py` 应用）

```json
"placement": {"map": "0122", "x": 30, "y": 32, "note": "比奇国王 (29,32) 的邻格"}
```

* 本驱动器**只校验形状**（`map` 1..10 字符、`x`/`y` 为 0..65535 的整数），**不做预置位**：
  声明存在时会在 stderr 打印一条提醒。**别以为直接跑 `mir2_probe.py` 就落到了目标位** ——
  预置位由 `tools/conformance/card_window.py` 在**每侧 restore 之后、起栈之前**改测试库的
  `characters.MapName/CX/CY` 完成（C# = `mir2_test_db`、Rust = `mir2_test_db_r`），
  完成不了直接 rc=2，不会让 probe 去超时。详见 `docs/window-runbook-20261023.md` §1.5。
* 坐标要选**阻挡格之外**的空格（通常取目标 NPC 的邻格）：落在墙格时 oracle 登录会随机挪位。

## 步骤动作

| 动作 | 取值 | 线上展开 |
|---|---|---|
| `send` | 对象：必填 `ident`；可选 `recog`/`param`/`tag`/`series`；`text` / `raw_hex` / `text_from` **三选一** | 12B 头 + 体，按 `#1<EDCode>!` 发出 |
| `say` | 字符串 | `CM_SAY(3030)`，`recog=0`，文本按 GBK+EDCode |
| `click_npc` | int 或 `params` 名 | `CM_CLICKNPC(1010)`，`recog=<NPC actor id>` |
| `dlg_select` | 字符串 label，或 `{"label": "...", "recog": N}` | `CM_MERCHANTDLGSELECT(1011)`；`recog` 缺省取**最近一条 `SM_MERCHANTSAY(643)` 的 recog**，也可写 `params` 名（加载期解析，与 `send.recog` 同口径） |
| `wait_for` | int 或非空 int 数组 | 等**下一步开始之后**到达的第一条匹配 ident |
| `wait_frames` | `{"count": N, "idents": [...], "timeout_ms": M}` | 等 gamegate 连接**累计**帧数 ≥ N 且 idents 都出现过 |
| `wait_idents` | `{"idents": [...], "timeout_ms": M}` | 等 idents **全部**在 gamegate 连接上出现过（累计口径；`timeout_ms` 不满足即步骤失败） |
| `sleep` | int 毫秒，或 `{"ms": N}` | 纯等待，不发包 |

`send.text` 的编码口径 = C# `EDCode.EncodeString`：先 `text.encode("gbk")`，再 EDCode。
`send.raw_hex` 是**编码前**的 body 字节（十六进制），驱动器负责 EDCode。
`dlg_select` 若在收到 643 之前使用且没给 `recog`，直接 fail-closed（退出码 2），不会瞎猜。

## 等待语义（对拍必须可复现）

* `wait_for` 只匹配 **`seq` 大于本步开始时** 的帧 —— 上一步已经收到的同号帧不会被误当成本步的回包。
* `wait_frames` 计的是 gamegate 连接的**累计**帧（含进世界前收到的 `658/51`）。
* `wait_idents` 同样是**累计**口径，但要求列表里的 ident **全部**出现过；超时即步骤失败
  （退出码 1），步骤记录里写 `seen_idents` / `missing_idents` 便于定位差哪条。
* **进世界后的延迟段 dwell**：驱动器收到 `SM_LOGON(50)` 后先等 `--dwell-ms`（默认 **1200ms**）。
  Rust 首个 `SearchViewRange` 在 `enter_map + 1000ms`（`crates/world/src/entity.rs` 的
  `PLAYER_SEARCH_TIME`），dwell 不足会在延迟段帧（`10/41/654/657/100…`）到达之前
  `CM_SOFTCLOSE` 小退，对拍时被误判成「对侧独有」。`--dwell-ms 0` = 旧行为（只
  `pump --between-ms`），**只用于复现旧问题**，对拍不要用。
  `--dwell-ms` 必须 ≥ 0；负值在参数校验期 fail-closed（退出码 2，不落 `run.json`）。
* 步骤间默认间隔 `--between-ms`（默认 200ms）；单步可用 `delay_ms` 覆盖。等待本身用 `timeout_ms`。

## 参数

`text` 是字面量，**不与 params 做替换**；要把文本参数化（如物品名）请用 `text_from: "<params 名>"`——
该名字必须已在 `params` 或 `--var` 里给出，否则加载期 fail-closed。

`params` 里可放 int 或字符串；动作里的字符串会按「先查 params、再按十进制 int」解析：
`{"click_npc": "npc_actor"}`。`send` 的 `recog`/`param`/`tag`/`series` 同口径
（`ident` 只收 int）；解析失败或越界（`recog` 为 i32，其余 u16）在**加载期**就报 `scenario-error`，
不会拖到发送时才变成 `frame-parse`。命令行 `--var npc_actor=456` 覆盖场景默认值。
换一台实例跑 `dice_chain` 时，actor id 变了就只改 `--var`，文件不动。

> **改 `scenarios/*.json` 后一定要重跑 `--selftest`，但要知道它只覆盖 3 个文件**：`--selftest` 的
> `scenario-dsl` 用例只加载 `login_only.json` / `gm_time.json` / `dice_chain.json`（`mir2_probe.py:1286-1288`），
> 对这 3 个文件里的步数/idents/体做断言——动它们而不重跑 selftest 会漏回归。
> 其余场景文件 selftest **读不到**：改完必须逐个 `--check-scenario --scenario <path>`（加载期 fail-closed），
> 否则写坏了也没有任何门禁会红。两条命令都要跑，别互相替代。

> `dice_chain` 末步是**分支相关**断言：`@ya52` 持筹码包走 `#ACT` 回 `643`，无筹码包走 `#ELSEACT MESSAGEBOX` 回 `SM_MENU_OK(767)`，
> 因此该步等 `[643, 767]`——单等 `643` 会变成"无包账号必然超时"的恒失败断言。

> `npc_script_chain` 是**多步标签链**（R1）：点比奇国王(0122) 后连发 5 次 `dlg_select`
> （`@@buildguildnow` → `@buildguildexp` → `@main` → `@requestcastlewarA` → `@exit`），
> 断言 `643 → 763 → 643 → 643 → 643 → 644`。`required: ["npc_actor"]` 走 643.recog 发现；
> 链里没有 `@ask`（部署件无此标签）也没有 `@back`（无 `<.../@back>` 链接 ⇒ `LableIsCanJmp` 门控下零帧），
> 逐条 oracle 出处与理由见 `docs/npc-script-chain-scenario-20261023.md`。

> `combat_strike` 是**唯一一条主动发起攻击**的场景（R3）：`placement` 到 map 4 封魔谷 (256,232) →
> `CM_TURN(3010)` 定状态化朝向 → `CM_HIT(3014)` → `wait_for 31`（`SM_STRUCK`）；目标是正上方格
> (256,231) 的固定刷怪点 `恶魔弓箭手`（`MonGen.txt:90`，Range=0/Count=1；Race=112 `ArcherGuard`，
> **非 FixedHideMode**）。
> 三个容易写错的点，逐条 oracle 出处见 `docs/combat-strike-scenario-20261023.md`：
> ① **`CM_TURN`/`CM_HIT` 的 X/Y 打包在 `recog` 里**（`WorldServer.cs:831-844`：
> `nParam1=LoWord(Recog)`、`nParam2=HiWord(Recog)`、`wParam=Tag=Dir`，`param` 不被读）
> ⇒ `recog` 必须写 `(Y<<16)|X`，不是「X 放 recog、Y 放 param」；
> ② **必须先 `CM_TURN` 定朝向**——`CM_HIT` 取目标走 `GetPoseCreate()` 的**状态化 `Dir`**，
> 报文里的 `tag` 只管动作帧（`PlayObject.Attack.cs:26` 早于 `BaseObject.Attack.cs:12` 的 `Dir=nDir`）；
> ③ **本方看不到自己的 `SM_HIT(14)`**（`PlayObject.Message.cs:1433-1437` 自我过滤），
> 命中只能等受击者的 `SM_STRUCK(31)`。
> **本条现在是 BLOCKED-FIXTURE，不许当窗口输入，且「跑绿 ≠ 玩家命中」。** 目标必须既「非 FixedHide」又「打得动」——
> Race=95 的 `僵尸2`/`石墓尸王` 是 `DigOutZombi`（出生即 `FixedHideMode=true`，按 oracle 的隐藏视野门
> 永不出土、`SendRefMsg` 也不广播 ⇒ 恒超时），而名字不在 `monsters` 表里的 `弓箭护卫` 等根本不刷。
> 剩下能刷的非隐藏目标只有 112/102/115，AC 都 ≥18；伤害扣的是 **[AC, 2AC-1] 的随机护甲**
> （`BaseObject.Base.cs:789-806`，怪物 `AC=MakeWord(AC,AC)`，`WorldServer.MonGen.cs:898`），
> 所以「必伤害」门槛是 `DC_min > 2*AC-1`（AC30 ⇒ ≥60），再叠加 `HitPoint ≥ SPEED-1`（SPEED18 ⇒ ≥17）；
> 基线角色没有一档同时满足（最好的一对 joint 只有 24%，精确矩阵见文档 §6）。
> ⇒ 现在跑必然 `wait_for` 超时。**另外这是敌对守卫**（`GuardUnit.cs:120-124` + `ArcherGuard.cs:39-81`），
> 玩家挨打也会收到 `SM_STRUCK(31)` 且 `recog=self`，而 `wait_for` 只按 ident 匹配（`mir2_probe.py:291-303`）
> ⇒ **即使跑绿也证明不了「玩家主动一击命中」**。**该封口已在工具层强制**：场景顶层带 `blocked`
> （`combat_strike.json` 已补；schema 见 `docs/probe-scenario-blocked-gate-20261023.md`），`card_window.py`
> 见到即 rc=2、不建 out/不起栈；原先「窗口批次不得消费本场景的任何 PASS」的人读约定降为解释。
> 根治需 `wait_for` 支持 recog 过滤，归 `openmir2-probe-scenario-validation-gaps`）。
> 另两条登记：「没打中」支零协议帧（只有明文 `+GD` ack、`ident=null`，扩展建议见文档 §4）、
> `rules.json` 缺 ident 31（另卡 `openmir2-conformance-rule-sm-struck-31` 在办）。

> `upgrade_retrieve` 是**唯一一条武器升级 NPC 链**（R2）：`placement` 到盟重省 map 3 (320,328) → `click_npc` 沙巴克铁匠 → `@upgrade`/`@confirmupgrade`/`@upgradenow` 提交 → `sleep 660000` 过取回时效门 → `@main`/`@getbackupgnow` 取回（`Merchant.cs:529-590 / 594-759`）。四个容易踩的点：① **placement (320,328) 的理由与刷怪无关**——`MonGen.txt:51/58/59` 那三行的怪名是 `弓箭护卫`，**不在 `monsters` 表** ⇒ 根本不刷（`GetMonRace=-1`、`if (monGen.Race > 0)` 跳过）；选它的理由是「`.map` 可走 + 到铁匠 (12,11) 在 `CM_CLICKNPC` 15 格门内 + x=332 最大使 discovery 的 `last_turn` 落在铁匠」；② 取回要过 `UPgradeWeaponGetBackTime=600000`（`Server.conf:331`），Permission 恒 0 ⇒ 只能等，**窗口 profile 必须把 `probe_timeout_ms` 抬到 ≥ 720000**（`card_window.py:1949` 的进程超时 = `max(180, probe_timeout_ms/1000+120)` 秒）；**写错键名（例如 `timeout_ms`）会被静默忽略**（profile 顶层无未知键校验），probe 仍在 180 秒被杀 ⇒ 白跑一轮；也可以用 CLI `--timeout-ms 720000`；③ 提交/取回的成功硬门是**唯一 ident** `709`/`200`（`wait_idents`），`643` 只作成败判别（否则 643 会把失败分支蒙混过去）；④ 本场景要求背包 < 46 件（部署脚本没有 `~@getbackupgnow_bagfull` 标签 ⇒ 包满支零帧，等不到）。逐条 oracle 出处、夹具前置与登记缺口见 `docs/upgrade-retrieve-scenario-20261023.md`。

> `equip_struck_save` 是**唯一一条装备耐久变化**场景（R7）：`placement` 到铁器店 map 0103 (19,19) → `CM_TURN(3010)` 定朝向 Up → `sleep 400` → `CM_HIT(3014)` → `wait_for [642,203]` → `wait_idents [642]`。**选「攻击方武器磨损」而不是「受击方扣耐久」**：`AttackSuccess`（`PlayObject.Attack.cs:215`）只在 `targetObject==null` 时逃逸（`:217-220`），有目标即无条件抽 `Random(5)+2-WeaponStrong`（`:225-229`）——**命中/躲闪/伤害 0 都扣**；受击方要先被打中（概率路径，正是 combat_strike 的坑）。目标 = map 0103 的商店 NPC「卫家店」（`MerChant.txt:26`，坐标 (19,18)）：`GetMovingObject` 返回格内任何 `ActorObject`（`Envirnoment.cs:1203-1239`），且玩家侧的 `IsAttackTarget` 对 NPC 返回 false（`PlayObject.Attack.cs:627-753`：默认 `HAM_ALL` 下只有 `Race<NPC || Race>PeaceNpc` 才可打）⇒ 不掉血、不还手、不触发 `StruckTime`。**平台必须是「零刷怪」的图**：`MonGen.txt` 里 map 0103 **一行都没有**（部署件与两侧窗口 Envir 的 `MonGen.txt` 逐字节相同）⇒ 没有刷怪箱覆盖；此前选 map 0 (288,610) 是错的——`MonGen.txt:112/113/124/125` 的 鹿/稻草人 `Range=50` 箱体覆盖它，`GenMonRate=10` 下每行刷 50 只，稻草人 Race=83 `SlowAtMonster` 会主动攻击。**`642` 才是磨损帧**（`Recog=Dura/Param=槽/Tag,Series=DuraMax`；破零先 `202` 再 `642(Recog=0)`）；**`203` 普通磨损不发**，只在 `CheckWeaponUpgrade` 成功支（武器 `Desc[10]>0`）⇒ 集合里带它只为换夹具不恒超时，硬门只认 642。注意 **Rust 侧这条耐磨帧还没接**（T-199 相邻登记 + `wear_durability` 无事件通道）⇒ 现在跑会在「Rust 缺 642」上红，不是场景问题。逐条 oracle 出处、零怪证明表、存档侧判据与登记见 `docs/equip-struck-save-scenario-20261023.md`。


> **R6 `castle_zone` 没有场景文件（2026-10-23 可达性登记）**：城堡战/城堡区在部署里**单会话探针不可达**——`@requestcastlewarnow` 的申请链命中 `UserCastle.AddAttackerInfo(IGuild)` 的 `throw new NotImplementedException()`（`UserCastle.cs:670-673`，白名单 **T-38**）；GM 的 `@ForcedWallconquestWar` 卡在 `PermissionMin=10`（部署是 Release）；`UnderWar` 只在墙钟 `StartCastlewarTime=20` 那一小时开（`Server.conf:380`）且 `Castle/0/AttackSabukWall.txt` 为空 ⇒ `766 SM_AREASTATE` 的 bit4（`RefUserState` 的 `|=4`，`PlayObject.Base.cs:281-284`）永远点不亮。依赖它的 4 张 R 卡（castle-guard-target / castle-war-area-impl / castle-merchant-fixedhide-hit / castle-merchant-disappear-downlink）窗口**当前不可跑**；可达性表、`766` 置位链与夹具规格见 `docs/castle-zone-unreachable-20261023.md`。

> `spell_cast` 是**唯一一条主动施法**的场景（R5）：`placement` 到 map 0122 (29,33) → `CM_SPELL(3017)` 打空地面 (29,28) → 等 `SM_MAGICFIRE(638)`；`sleep 1600` 过 `MagicAttackInterval`（`Delay 60 + MagicHitIntervalTime 1350`）→ 第二发打 (29,42)（|dy|=9 > `MagicAttackRage=8`）→ 等 `SM_MAGICFIRE_FAIL(639)`。
> 四个容易写错的点，逐条 oracle 出处见 `docs/spell-cast-scenario-20261023.md`：
> ① **`CM_SPELL` 的字段口径与 `CM_TURN`/`CM_HIT` 不同**——`WorldServer.cs:783-793` 是 `SendMsg(Ident, Tag, LoWord(Recog), HiWord(Recog), MakeLong(Param,Series))` ⇒ `tag=魔号`、`recog=MakeLong(目标X,目标Y)`、`param+series` 拼成目标 ActorId（无目标就都写 0）；
> ② **`SM_SPELL(17)` 永远不会到自己这条连接**——`PlayObject.Message.cs:1455` 的 `if (processMsg.ActorId != ActorId)` 把自己滤掉 ⇒ 等待集合里写 17 必然恒超时（卡面原本写了 17，是错的）；自己只收 638/639（`:1788-1793`/`:1794-1800` 无 self 守卫）；
> ③ **两条支路都以「角色已学该魔法」为前提**——`GetMagicInfo(nKey)`（`PlayObject.Attack.cs:1065`）为空时直接 `false`、只回 `SM_MOVEFAIL(28)`，638/639 一枚都不发；基线库 `characters_magic` 里没有窗口角色（winplay0 = PlayerId 6228）的任何一行 ⇒ **本场景现在是 BLOCKED-FIXTURE，缺夹具必红**（fail-closed，不会假绿）；
> ④ **两次施法之间必须 ≥1410ms**（`MagicAttackInterval = Magic.DelayTime + Config.MagicHitIntervalTime`，`PlayObject.Attack.cs:1112-1118`；部署 60+1350），否则走「魔法忙」早退，第二发既不立刻出帧也打乱两支判定。

## 驱动器的自动握手（对拍不要关）

照 `src/BotSrv/Player/RobotPlayer.Operate.cs`：

* 收到 `SM_SENDNOTICE(658)` -> 回 `CM_LOGINNOTICEOK(1018)`；
* 收到 `SM_LOGON(50)` -> 发 `CM_QUERYBAGITEMS(81, recog=1)`；
* 服务端单字节 `'*'` 心跳 -> 回 `'*'`。

`CM_LOGINNOTICEOK` 的 `recog` 在 BotSrv 里是客户端 tick（1284 行），驱动器固定 **0**：
服务端不读该字段，固定值才能让 Rust / C# 两次运行的字节可比。登记为 **D-probe-1**。
调试可用 `--no-auto-handshake` 关掉，但那样两侧会缺同样的应答，不适合对拍。

## 证据文件

### `frames.ndjson`（一行一条 s2c 帧）

固定字段：`seq`、`t_ms`（相对本次运行起点）、`phase`（`login`/`selgate`/`gamegate`）、
`conn`、`ident`、`recog`、`param`、`tag`、`series`、`body_len`、`body_hex`、`body_sha256`。
附加：`frame_class`、`tail`（`!` 或 `!$`）、`raw_len`、`body_text`（GBK 展示，仅供人看）。

**`body_hex` 是线上原样字节**，不是「解码后的明文」：GameSvr 有的消息把多段
`EDCode.EncodeMessage(...)` 用**原样 `'/'`** 拼起来（如 `SM_ADJUST_BONUS(811)`），
所以 body 里允许出现不在 `0x3C..0x7B` 的字节。`body_text` 只是尽力解码的预览，
比较器请比 `body_hex` / `body_sha256`。

`frame_class` 取值：

* `protocol`：正常 12B 头协议帧；
* `server_ack_time`：服务端明文动作 ack `#+GD/<rtime>!`
  （`src/Modules/SystemModule/MessageSettings.cs` 的 `sSTATUS_GOOD`）。它不是协议帧，
  `ident/recog/param/tag/series` 为 `null`，`body_hex` 是 `+GD/<rtime>` 明文。
  其中的 `rtime` 是服务端时间（C# 口径 = `Environment.TickCount`，**≤10 位十进制、可为负**；
  Rust 侧是 13 位 epoch-ms），**不能逐字节对拍**，比较器应按 `frame_class` 排除或只比前缀。
* `server_action_ack`：`PlayObject.Attack.cs` 的 `SendSocket("=DIG")` / `("+PWR")` 一族短明文
  （`DIG`/`PWR`/`LNG`/`ULNG`/`WID`/`UWID`/`FIR`/`UFIR`/`CRS`/`UCRS`/`TWN`/`UTWN`/`CID`/`UCID`，
  前缀 `'='` 或 `'+'`）。同样不是协议帧，`ident/recog/...` 为 `null`，`body_hex` 是明文标签。

明文帧的判据是**文法**（固定前缀 + 纯十进制 / 标签全集），不是「EDCode 解不开就当明文」：
`+GD/` 后不是 `-?[0-9]+`、或未知的短明文，仍然 fail-closed（退出码 2），不会被当成帧吞掉。
两者都在 12B 头长度闸**之前**分类 —— C# 的 `#+GD/1792667727!` 内层只有 14B，先过长度闸就会被拒。

### `run.json`

`argv`（`--password` 已打码）、`env`、起止墙钟、`chain`（登录四步的 ident/网关地址/session/角色列表/
`SM_LOGON` 的 actor id 与落点）、`dwell`（进世界后延迟段：请求/实际时长、窗口前后见过的 ident、
新到的 ident；**加性字段**，`schema_version` 仍为 1，旧比较器忽略）、`steps`（每条步骤的发送帧
十六进制与等待结果）、`auto`（自动握手）、
`frames_total` / `frames_by_conn` / `ident_hist`、`unclassified`、`exit`。

## 退出码

* `0` 全绿；
* `1` 链路或场景失败（网关不可达、登录被拒、`wait_for` / `wait_idents` 超时、角色不存在等）；
* `2` UNCLASSIFIED：帧解析失败（越界字节/未闭合帧）、规则没覆盖（`531` 改密、
  `dlg_select` 无 643 锚点）、场景 DSL 非法、必需参数缺失（参数校验在建证据目录之前，
  因此不落 `run.json`，只在 stderr 报缺哪几个参数）、内部错误。

工具是 fail-closed 的：解析不了就退出 2 并写进 `unclassified`，**绝不静默当绿**。

## 自测

```sh
python tools/probe/mir2frame.py selftest
python tools/probe/mir2_probe.py --selftest
```

`--selftest` 用 `../fixtures/r9/` 的离线 s2c 抓包跑「切帧 -> 解析 -> 落 ndjson」全链路，
不连网、不起进程；含「改坏必红」的负向用例（载荷字节改成越界值必须报错）。
当前共 **9 checks**；其中 `scenario-validation` 覆盖 `--check-scenario` 的语义牙（R1–R6：
删 `send` 步 / `wait_for` 写表外 ident / `send.ident` 表外 / 引用未声明变量 / `wait_for` 先于 `send` /
全 `sleep` / 注释掉的常量不得进表 / `placement` 与 `self_pos`·`self_x`·`self_y` 不一致 / 间接 params 名不假红 /
R6 取表五种 drift / 攻击族越界方向只 WARN 不判死），并断言 `login_only`、`dice_chain` 与非攻击族 `tag` 不被误伤。

## fixtures 来源

`../fixtures/r9/` 是 r9 真机窗口的代理抓包（`E:/tmp/window/r9/proxy/`）原样复制：
`conn-0001.s2c.bin` 44 帧、`conn-0002.s2c.bin` 106 帧（含 4 条 `SM_MERCHANTSAY(643)`，
即赌场骰子链的服务端回包）。`*.c2s.bin` 是同一窗口的客户端字节，用于核对帧构造的已知答案向量。
`sha256` 见提交说明；不要在 fixture 上做任何改写（自测里的 tamper 是内存副本）。

## `recog` 打包口径（动作族 `CM_HIT`/`CM_WALK`/`CM_RUN`/`CM_TURN`…）

这些命令的 **X/Y 都打包在 `recog` 里**：`recog = (y << 16) | x`（`LoWord=X`、`HiWord=Y`），
**`param` 服务端完全不读**（`src/GameSrv/Word/WorldServer.cs:831-851`，两支都只取 `LoWord/HiWord(Recog)`）。

* `combat_hit.json` 用 `target_xy`、`move_walk.json` 用 `move_xy` 承载打包值；`target_x/target_y`、`move_x/move_y`
  保留为「意图/推导量」。
* **打包可以用 `computed.pack` 声明**（卡 `openmir2-probe-computed-pack`）：`param_discovery` 会算出
  `值 = (hi<<16)|lo`，`card_window` 的自动发现据此直接拿到 `*_xy`：

  ```json
  "computed": {
    "move_x": {"derived_from": "self_x", "delta": 1},
    "move_y": {"derived_from": "self_y", "delta": 0},
    "move_xy": {"pack": {"lo": "move_x", "hi": "move_y"}}
  }
  ```

  `pack.lo`/`pack.hi` 可写任意可推导变量（**允许引用其它 `computed`**，按不动点求解；成环、缺字段、
  与 `derived_from/delta` 混用都 fail-closed）。`mir2_probe` 仍不读 `computed`——真跑时值由
  `--var` 传入，`card_window` 用发现值填。
* `combat_hit.json` 的 `hit_xy` 同样用 `computed.pack {lo: self_x, hi: self_y}`：**CM_HIT 的 Recog 是攻击者自己的坐标**（`PlayObject.Attack.cs:883` 只在 `nX==CurrX && nY==CurrY` 时执行），方向在 `tag`。但该场景**已登记 BLOCKED**（攻击者收不到自己的 `SM_HIT(14)`，见卡 `openmir2-scenario-combat-hit-waitfix`）。
* 详细逐文件复核表、oracle 行号与影响面：`docs/scenario-recog-packing-20261023.md`。
