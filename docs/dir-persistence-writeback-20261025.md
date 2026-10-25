# `characters.Dir` 存档写回（卡 `openmir2-dir-persistence-writeback`，tier R）

> 状态：**离线分析已出，机制待一次带日志的真机轮钉死**（见 §4）。本文件只写已核实的事实。

## 1. 事实（真机，`E:/tmp/window/butch-run/out`）

| 项 | C# | Rust |
|---|---|---|
| `characters` 落库 | `Map=0, CX=330, CY=330, Dir=**0**` | `Map=0, CX=330, CY=330, Dir=**3**` |
| `SM_BUTCH(637)` 的 `Dir`（头 `series`） | **0** | **0** |
| 基线 dump（`E:/tmp/conformance/baseline/mir2_test_db.sql`） | `winplay0: B013 / (7,12) / Dir=3` | 同 |

⇒ 两侧**位置都正确落库**、**帧上 `Dir` 一致且正确**；差异只在**持久化的 `Dir`**，且 Rust 写回的值
**恰等于基线/载入值 3**。同一签名在 W1 的 `move_walk` 也出现过（`Dir 2→3`，基线同为 3），当时被判 flaky，**现订正为稳定差异**。

## 2. oracle 对照：`Dir` 的「实体朝向 → 存档值」链路

| 环节 | oracle（`src/`） | Rust | 一致？ |
|---|---|---|---|
| 权威字段 | `PlayObject.Dir`（`BaseObject.Dir`） | `Entity.ai.dir` | ✅ |
| 写入点 | 转向/走跑/9 条攻击命令/`CM_SPELL` default；`CM_BUTCH` 的 `Dir = btDir`（`Operate.cs:736`，位置支内**无条件**） | 同名落点：`lib.rs:9784/9840/10242`、butch `lib.rs:3144-3148`（`out.dir_updated ⇒ e.ai.dir = bt_dir`） | ✅ |
| 登录装载 | `WorldServer.cs` 把 `CharacterData.Dir` 赋给 `PlayObject` | `apply_login_record_scalars`（`lib.rs:14181`，**唯一**生产装载点，`lib.rs:23053` 首进图调用） | ✅ |
| 存档取数 | `MakeSaveRcd`：`playerInfo.Dir = playObject.Dir`（**实体**） | `build_save_record`：`rec.data.dir = e.dir()`（**实体**，`lib.rs:9434`） | ✅ |

**⇒ 第一个分歧点不在投影**：两侧的「存档读实体」是同一形状。分歧在**投影之前**——存档时刻实体的
`ai.dir` 已经是 `3`（而帧（同一实体派生）曾是 `0`）。

## 3. 由 §1+§2 推出的约束（用来排除错误修法）

1. 帧上 `Dir=0` ⇒ `Command::Butch{dir}` 取到 `bt_dir`（`series=0`）⇒ 当次 `out.dir_updated = true`
   ⇒ `e.ai.dir` 在那一刻被同步成 0（`lib.rs:3146`）；
2. 存档读到 `e.dir() = 3`，且同一分支里 `cur_x/cur_y` 取到的是**实时** `330/330`
   （`SavedHuman.record` 在会话中**从不被修改**——全仓无 `record.cur_x =` 赋值）⇒ **实体查找是成功的**，
   读到的就是实体的 `ai.dir`；
3. ⇒ 结论：**在 `CM_BUTCH` 之后、存档之前，`ai.dir` 被某个「按记录值重装标量」的路径改回了 3**。
   唯一已知的「记录 → 实体 Dir」写入点是 `apply_login_record_scalars`（`lib.rs:14181`）。

## 4. 待钉死的机制（一次带日志的窗口即可，已排除若干假说）

候选（按可能性排序）：

- **M1 存档/重进竞态**：本会话里发生了第二次「装载标量」（重复进入/被踢后重进/`InSaveRcdList` 后的
  「角色数据保存中,稍后重新读取」支，`lib.rs:22788/22934`）——会把 `ai.dir` 覆盖回记录值。
- **M2 周期存档先于断线存档**：`SaveHumanRcdTime` 若被测试配置调小，周期存档可能在 butch 之前写一次
  `Dir=3`；若断线存档因 DB 链路/时序没落到盘，最终库里就是 3（位置恰好在之前就已是 330 ⇒ 表现一致）。
- **M3 存档投影整块跳过**（reviewer-20 在 butch 卡 review 里补入，代码可直接指认）：`build_save_record`
  只有 `if let Some(e) = world.session(chr_name)…entities.get(id)` 解析得到实体才写
  `rec.data.dir = e.dir()`；**解析不到就整块跳过、保留载入值**（`lib.rs:9416-9437`）。
  注意这条**不能单独解释本场景**（butch 那次存档里 `cur_x/cur_y` 取到了实时 330/330 ⇒ 该次分支是执行了的），
  但可以解释「**最终值来自 butch 之前的某次存档**、而 butch 之后那次被整块跳过」的组合。
- **M4 其它**：待日志排除。

**判别要点（M3 的纹路）**：若最终 `Dir` 是基线值而 `XY` 是实时值，则两次观测必须来自**不同的存档**——
一次（执行了实体分支、当时 dir 还是基线）与一次（被跳过）。⇒ 日志里要数**「存档请求已发」的条数与时间**，
并核对每个请求之后是否有对应应答（`DBR_SAVEHUMANRCD`）。

**跨四根佐证（butch 卡 review 实测）**：`db-rerun/run{,2}/move_walk` 均 C# `2` / Rust `3`、`w1c/move_walk`
两侧都 `3` ⇒ **Rust 侧 `Dir` 恒为种子值**，与「该列从不更新」一致；C# 随场景变（`0/2/3`）。
这与 M1（重装覆盖）和 M3（整块跳过）都能相容，判别仍靠上面的存档计数。

**要采的日志行**（下一轮窗口）：

**窗口执行顺序（先做这一步，否则动作会被服务端拒）**：起栈 → 登录 → **确认日志里不再出现
「角色数据保存中,稍后重新读取」**（`InSaveRcdList` 清空）→ 再发第一条动作命令。
`move_walk` 那次 UNDECIDABLE（C# 侧 `CM_WALK` 无 `SM_WALK`、只有 `SM_TURN` + `SYSMESSAGE`）就是踩了这条：
存档在途时服务端拒动作，表现为「移动不生效」，看起来像行为差异。

```text
# 0) 存档在途门：出现即等待/重来
grep -c '角色数据保存中' game-svr.out.log
# 1) butch 决策面：确认 dir_updated=true、bt_dir=0
grep 'CM_BUTCH：' game-svr.out.log
# 2) 存档请求面：每次存档一条（含 ChrName/queryId）
grep '存档请求已发' game-svr.out.log
# 3) 重装面：是否出现「角色数据保存中,稍后重新读取」/ 重复进入
grep '角色数据保存中\|关新\|踢' game-svr.out.log
# 4) 周期存档配置与实际触发
grep -i 'SaveHumanRcdTime' !Abc.txt ; grep '周期存档' game-svr.out.log
```

判定：日志里出现 (3) ⇒ **M1**；只有 (2) 且断线存档缺失/晚于 dbsnap ⇒ **M2**。

## 5. 修法原则（待机制确认后落地，先写死红线）

- **只改 Rust**，不改 `src/**`；修法必须让「存档取实体」在**任何重装路径之后**仍成立
  （若为 M1：重装标量只在**首次进图**应用，或对已存在实体只补未建模字段，不覆盖 `Dir`）；
- **不得**把 `Dir` 加进 dbsnap 忽略列；**不得**用「该列不可对拍」开脱（两侧 XY 相等已证结构一致）。

## 6. 牙（计划）

- 构造级：`apply/变更 e.ai.dir` 后调用 `build_save_record` ⇒ 取到**新值**；
  变异：把投影改成读 `saved.record.data.dir` ⇒ 必红。
- 若机制为 M1：再加一条「重复装载标量后 `ai.dir` 不被覆盖」的构造牙（变异：去掉防护 ⇒ 红）。

## 7. 修法边界表（oracle `GetHumData` 口径，离线已定）

oracle `WorldServer.cs:1100 GetHumData` 的调用点只有**两处**：`:248`（无 switchData 支）与 `:412`（switchData 支），
**都在「进入世界」流程里、对新建的 `PlayObject` 执行**；函数体对下表字段**一律无条件装载**（`playObject.Dir = humData.Dir` 等）。

| 字段 | oracle 装载时机 | 进图后是否被后置覆盖 | 运行期写点（oracle） | Rust 对应 |
|---|---|---|---|---|
| `Dir` | 进入世界时（`GetHumData`） | 否 | 转向/走跑/9 条攻击/`CM_SPELL` default/`CM_BUTCH` 的 `Dir=btDir` | `apply_login_record_scalars`（`lib.rs:14181`，首进图 `:23053`） |
| `AttackMode` | 同上 | 否 | `@AttackMode` 等 | `attack_mode`（登录装载） |
| `Gold` | 同上 | 否 | 交易/掉落/`CM_DROPGOLD` | `sg.state.gold`（会话态） |
| 位置 `CurrX/CurrY` | 同上 | **是**：switch 支用 `switchDataInfo` 覆盖（`:414-416`） | 走/跑/传送 | 实体 `e.x/e.y` |
| `Abil` | 同上 | **是**：switch 支用 `switchDataInfo.Abil` 覆盖 | 战斗/升级 | `sg.recovery` 等 |

**⇒ 由 oracle 推出的修法边界（判完 M1/M3 后按此落地）**：

1. oracle **不存在**「对**已在世界里**的对象重装全量标量」的路径——`GetHumData` 只在新建对象时执行一次；
   所以若判为 **M1（会话中重装）**，正确修法是**让装载只发生在首次进图/新建实体上**，
   （或只补未建模字段），**不是**给 `Dir` 打特判；
2. 若判为 **M3（存档投影整块跳过）**，修法不涉及装载口径，而是**存档分支的实体解析**（`lib.rs:9416-9437`）；
3. 无论哪种，**都不要**把 `Dir` 加进忽略列。

## 8. 「环境状态伪装成行为差异」清单（harness runbook 素材）

同类样例已有三个，全部指向同一纪律：**先确认环境状态，再解释差异**。

| # | 形态 | 观测 | 真相 | 判据化处置 |
|---|---|---|---|---|
| 1 | **存档在途** | C# `CM_WALK` 无 `SM_WALK`，只有 `SM_TURN`×N + `SYSMESSAGE`×N | 登录期间 `InSaveRcdList` 未清，服务端拒动作（日志「角色数据保存中,稍后重新读取」） | 起栈/登录后先等该行消失再动作（`grep -c '角色数据保存中'`） |
| 2 | **RNG 账本被裁** | 账本里「两抽没发生」 | `trim_rng_ledger` 超限只留 head，而抽数在后半段（实测 7.2k 行/s） | 跑完先看 `rng_log_trim.trimmed`；为 `true` ⇒ 证据作废、重跑，**不判 FAIL** |
| 3 | **检出不全** | `oracle_vectors` 假红 | 检出里缺文件/构建陈旧 | 起栈前核 pid 的 `exe` 路径与构建时间；`require_pid_binaries` fail-closed 不要绕过 |

## 9. 修法裁决（`5b0fb93a`）：本卡走 (a) 快照；(b′) 另开 `openmir2-makeghost-lifecycle`（issue `c86e86b4`）

**成因订正**：发存档的是 `GateEvent::Closed` 臂（日志 L149 `06:38:54.986 用户关闭：socket=400 index=1 在线=0 槽=1` ⇒ 已摘），
**不是** `drain_soft_close`；同一角色 402ms 内走了**两条退出路径**，先走的那条已把实体销毁。
(b)「把存档挪到 `soft_close` 之前」被**否决**：Closed 臂会再存一次，二次存档在 DB 上可观测。

**实施契约（每条都要有牙）**：

1. **单一接缝**：快照只由一个函数 `entity_derived_snapshot(&Entity)` 产出，禁止在 soft-close 处散写（散写必然漂移）；
2. **五项全取**：`x / y / dir / attack_mode / use_items(Index, Dura)`——只覆盖 `Dir` 等于只修今天现形的那一个；
3. **优先级**：**实体 → 快照 → 记录**；第三档**必须 WRN**，且只在「会话存在却既无实体也无快照」时发（不让普通断线/未进世界刷屏）；
4. **牙**：① 逐字段：walk 后 `CX/CY`、turn 后 `Dir`、`@AttackMode` 后 `attack_mode`、磨损后 `Dura` 各一条；
   ② 变异：优先级改成 `记录→快照` ⇒ 必红；③ 删 WRN ⇒ 必红；④ **负例**：普通 `disconnect` 路径同样成立（只修 soft_close 支不算完）。

## 10. 卡面补注（两条，供审阅与后续人）

**10.1 T-266 不并入本卡的依据（两轮符号相反）**

| 轮次 | 物品 | C# `Dura` | Rust `Dura` | 解释 |
|---|---|---|---|---|
| butch 真机轮（13:53–13:56） | `Id` 见该轮快照 | `3993` | `3989` | Rust 差分更大（两侧 RNG 分叉，磨损量不同） |
| Dir 诊断轮（本文件 §7/§8 那轮，14:38） | `Id=106 StdIndex=5001`（夹具注入 `dura=4000`） | `3984` | `3985` | Rust 差分更小 |

两轮**符号相反** ⇒ 不是「Rust 恒静默回落到载入值」（那会恒为 `4000`/`∞`）；且本卡若同根，Rust 应停在注入值。
⇒ `characters_item.Dura` 的差异属 **T-266（每刀磨损 `Random(5)+2-AddAbil.WeaponStrong`，两侧 RNG 流分叉）**，**不并入本卡**。

**10.2 为什么本轮只有 `Dir` 现形（不是只有 `Dir` 会坏）**

测试库的 placement 已把角色写成 `map 0 / (330,330)`（基线是 `B013 / (7,12)`），且 `attack_mode=0`、全程未移动
⇒ 其余字段的「回落载入值」**恰好等于实时值**，所以只有 `Dir`（基线 `3`、butch 后 `0`）显形。
**结论：存档面所有实体派生字段都会在「先离世后存档」路径上回落**，`Dir` 只是这次的显影剂。

## 11. 契约细化的两处（编排方 `6fdf…` 答复，写入供接手者）

**11.1 `use_items` 的档位降级（别与 `Dir`/坐标/`attack_mode` 同级）**

`build_save_record` 里 `use_items` 的**回落源是会话克隆**（`let mut save_use_items = sg.use_items.clone();`），
实体在才按实体覆盖（全零视为「未同步」不覆盖）⇒ 实体缺失时它回落 `sg.use_items`（**每 tick 回写的实时值**），
**只丢最多一个 tick**。与 `dir/坐标/attack_mode`（直接回落到**载入记录**）不同。
⇒ 契约：`use_items` **仍进快照**（要「离世那一刻的精确值」），但**优先级排在四个标量之后**，
定性为「覆盖一个 tick 的漂移」，**改动面不得被读成「必须整块拷实体」**。

**11.2 `SavedHuman.derived` 与 WRN 判定点**

- 落点：`SavedHuman`（`lib.rs:9082`）加 `derived: Option<EntityDerived>`，不新开结构；
- `build_save_record`（`:9404`）优先级：**实体 → `saved.derived` → `rec`**；第三档 WRN；
- WRN 判定放在 `world.session(chr_name)` 判定**之后**：「session 在 ∧ 实体无 ∧ derived 无」才发；
  普通 disconnect 必走实体支 ⇒ 不刷屏（牙④的可注入点）。

## 12. 接手者最短路径（体力活清单）

```bash
git status                                   # 必须干净；若脏先查，勿覆盖
git log --oneline -1                         # 期望 a9309b5ba（或其后）
git fetch walgit master && git rebase master # 需要时
# 1) 写 EntityDerived + entity_derived_snapshot(&Entity, &SessionGround)
# 2) SavedHuman.derived；build_save_record 三档优先级 + WRN
# 3) 四颗牙：walk 后 CX/CY、turn 后 Dir、@AttackMode 后 attack_mode、磨损后 Dura；
#    变异①优先级改 记录→快照 ⇒ 必红；变异②删 WRN ⇒ 必红；牙③普通 disconnect 负例
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p game-svr --lib <新测试过滤名>   # 先局部，再全量
cargo test --workspace
cargo run -p msgcodegen -- --check
# 7 卫生门 + git diff --check <base> <head> + merge_tree_helper
```

**真机复拍**（R 卡必须）：窗口内跑「walk+turn 后 soft_close」与普通 disconnect 各一次，
判 `characters.Dir/CX/CY/attack_mode` 两侧相等（`characters_item.Dura` 按 T-266 不判）。

## 13. 落地结果回填（dev 交接执行记录，2026-10-25）

> 本节是 §12 的**实际执行结果**（谁做了、做到哪、证据在哪、还缺什么）。判据仍未完成的项显式留白，
> 不许读成「已完成」。

### 13.1 代码改动（只改 Rust，`src/**` 零改动）

| 位置 | 改动 |
|---|---|
| `mir2-rs/bins/game-svr/src/lib.rs` `EntityDerived` / `entity_derived_snapshot` | 新增**单一接缝**：五项 `x / y / dir / attack_mode / use_items` 一次取全。**签名是 `entity_derived_snapshot(&Entity)`**（不是 §12 写的 `(&Entity, &SessionGround)`——五项全在实体上，会话输入是多余的；以 Leader 派单口径为准）。 |
| `SavedHuman`（`lib.rs` 的 `struct SavedHuman`） | 新增 `derived: Option<EntityDerived>`；43 处构造点补 `derived: None`（生产 1 处 + 测试 42 处）。 |
| `handle_soft_close_cmd` | 在 `world.soft_close(chr_name)` **之前**调 `entity_derived_snapshot` 写入 `saves[chr].derived`；函数签名新增 `saves` 形参（`dispatch_inbound_p0` 同步透传，9 个测试调用点补空表）。 |
| `build_save_record` | 四标量改「实体 → `saved.derived` → `rec`」，第三档调 `warn_save_fallback`；`use_items` 改「实体（且装载过）→ `saved.derived`（且装载过）→ 会话克隆」，**仍按 §11.1 定性为「覆盖 ≤1 tick 漂移」，未扩大改动面**。 |
| `warn_save_fallback` / `save_fallback_warns` | WRN 的触发门控在 `build_save_record`：**只在 `world.session(chr_name)` 存在**时发（普通断线存档在 `world.disconnect` 之前、实体仍在；从未进世界无会话）。为让「删 WRN ⇒ 牙红」可判，测试构建里有一个线程局部计数器，**计数点就写在 `glog!` 的格式实参里**（只删打印行也会让计数消失）。 |

**未做（明确留给别的卡）**：`displays.remove` / `party_del_member` 那块 `GateEvent::Closed` 清理在
`CM_SOFTCLOSE` 之后被跳过的问题**不在本卡**（Leader 已单开 `openmir2-softclose-cleanup-residue`，
issue `f83966d1`）；本卡**没有**动 `displays`、组队清理，也**没有**把 `actor_id` 改成「软关时保留」
（那等于做 (b′) 半成品，与 `openmir2-makeghost-lifecycle` 冲突）。

### 13.2 牙与变异（构造级，全部已跑）

夹具 `SoftCloseRig`：世界 `(100,100)`，角色在 `(11,22)`、`dir=0`、`attack_mode=0`，佩戴武器
`Index=207/Dura=4000`；载入记录故意写成 **`cur=(7,12) / dir=3 / attatck_mode=1`**（与实时值全不同）。

| 牙 | 内容 | 绿 | 删对应实现 ⇒ 红 |
|---|---|---|---|
| T1 | `CM_WALK` 到 `(12,22)` → `CM_SOFTCLOSE` → 存档 ⇒ `CX/CY=(12,22)` | ✓ | ✓（优先级变异后 `(7,12)`） |
| T2 | `CM_TURN` 到 2 → soft_close → 存档 ⇒ `Dir=2` | ✓ | ✓（回落 3） |
| T3 | `@AttackMode`（生产入口 `handle_say`）0→2 → soft_close → 存档 ⇒ `attatck_mode=2` | ✓ | ✓（回落 1） |
| T4 | 实体磨损 `Dura=1234`（会话仍 4000）→ soft_close → 存档 ⇒ `Index=207/Dura=1234` | ✓ | ✓（回落 4000） |
| T5 | 变异：`let derived = live.as_ref();`（快照降到记录之后） | — | ✓ **T1/T2/T3 三条红**（T4 走独立的 `use_items` 支，另由 T4 自身变异覆盖） |
| T6 | 变异：删 `warn_save_fallback(chr_name)` ⇒ 计数断言红（`left: 0, right: 1`） | ✓ 三门控（实体在 / 无会话 / 二缺一） | ✓ |
| T7 | **负例**：普通断线路径（不经 `CM_SOFTCLOSE`，存档在 `world.disconnect` 之前）同样取实时 `(12,22)/dir=2`，`derived` 保持 `None` | ✓ | ✓（删实体优先支 ⇒ 回落 `(7,12)`） |

补充：**删掉 `handle_soft_close_cmd` 里的 `saved.derived = Some(d)` 一行 ⇒ T1–T4 四条全红**（已实测）。

### 13.3 门禁（本地，单次顺序执行）

```
cargo fmt --all -- --check                            rc=0
cargo clippy --workspace --all-targets -- -D warnings rc=0
cargo test -p game-svr --lib                          1508 passed / 0 failed / 4 ignored（含本卡 6 条新牙）
cargo test --workspace                                rc=0（全绿）
cargo run -p msgcodegen -- --check                    rc=0（604 条常量一致）
7 卫生门（t_reservation / conflict_marker / stale_pointer / whitelist_anchor /
        m3_registry / dynamic_count_stamp / board_coverage）+ merge_tree_helper --selftest  全 rc=0
```

> 工具链记录：clippy 0.1.98 在 `thread_local!` 宏调用处误报 `missing_const_for_thread_local`
>（初值已是 `const { … }`，同 `chat.rs:50` 的既有登记）；逐项 `#[allow]` 会被判 `unused-attributes`，
> 故把计数器收进 `#[cfg(test)] mod save_fallback_warns` 并在模块内放行。

### 13.4 仍未完成（R 卡不走掉）

- **真机复拍（本卡的判据主体）**：窗口内跑「walk + turn 后 `CM_SOFTCLOSE`」与**普通 disconnect** 各一次，
  两侧比 `characters.CX/CY/Dir/AttatckMode`；`characters_item.Dura` 按 **T-266** 不判。
  未跑 ⇒ 本卡 `window_required=true` 继续保持，不得 `closed`。
- `use_items` 的**同 tick 残余**（最后一刀磨损与 soft_close 落在同一 tick 且会话回写先于 inbound）仍按
  §11.1 登记为「最多丢 1 tick」，本卡未覆盖。
