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
