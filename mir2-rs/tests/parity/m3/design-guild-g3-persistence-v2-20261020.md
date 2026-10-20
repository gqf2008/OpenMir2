# Lane B — 行会 G3（持久化 + 10s/30s + 掉线）实施简报 v2（修正版，可开工）

- 基线：`3bdfa0888`（master，2026-10-20）。G2 已合入：`d797489cc`。
- 车道：Lane B **只读**（不建 worktree、不改仓库、不起栈、不碰 `E:/MirServer`、不进 `/e/tmp/wg2`、`/e/tmp/wslots`）。
- 输入：`mir2-rs/tests/parity/m3/design-guild-g3-persistence-20261020.md`（原文 + 评审员更正）、
  G2 commit `d797489cc`、oracle `src/**`（只读）。
- 本文件取代原文 §3.2、§2.5、§4.3 的 T1/T4/T6/T8，其余章节（§0 的 8 条口径、§1 文件格式、§2.4 变更点表、
  §3.1 掉线时序、§4.1/§4.2 写集与串行）继续有效；冲突处以本文件为准。

---

## 摘要（≤1000 字）
G3 可开工，但先做三件事。① 时钟统一为 `world.clock_ms as i32`：G2 的
`handle_guild_cmd(..., now_ms: i64)` 改为 `now_tick: i32`，调用点传 `world.clock_ms as i32`；
加载时 `guild_run_tick` 同源；所有 `dw_save_tick`/`war_tick`/落盘 `now` 只此一域。
未注入时首个 30s 保存落在 30600；窗口用 `MIR2_CLOCK_MS=3600000`（>30000、正数），禁用 epoch。
② `Compare(...)==1` 不是“严格大于”：.NET 8 实测首异字符大写码位差（b/a=1、c/a=2、z/a=25、
中/一=45），前缀则返 UTF-16 长度差（ab/a=1）；只有差恰为 1 才命中。Rust 现用
`Ordering::Greater` 是错的，须改为返回 i32 的 `dotnet_compare_ordinal_ignore_case`。
③ 四条无牙判据 T1/T4/T6/T8 重写：行为用例调生产接缝 `guild_runtime::{startup,step,on_disconnect}`
与 `handle_guild_cmd`，另加 `include_str!("main.rs")` 源码锚点（照 main.rs:17754）：删主循环调用点
锚点红，删接缝内 load/run/save/check/clear 行为红。断线共三处（Closed、drain_kicks、drain_soft_close），
先 CheckSave 落盘再按 ==1 清成员。④ C# 时钟注入未授权：构造级全可做；真机字节窗口可做
（C# 开机>30s）；绝对 tick/精确边界/回绕对拍 blocked，review 必须 window_required。
⑤ 写集 8 文件，顺序 world→data→strings/config→guild_cmd→guild_runtime/main→whitelist。
---

## 0. 对原文的关键订正（先读）

| # | 原文 | 订正 |
|---|---|---|
| 0-1 | §2.5 建议 `MIR2_CLOCK_MS` 小值（正确），但未说 G2 已用 `now_ms()` 写 `dw_save_tick` | G2 `handle_guild_cmd` 的 `now_ms: i64` 参数**必须改成 `now_tick: i32`**，调用点传 `world.clock_ms as i32`；详见 §2 |
| 0-2 | §3.2 把 `Compare(...)==1` 读成“严格大于” | `==1` 是**数值恰等于 1**；见 §3，Rust 现实现 `cmp_ignore_case(...)==Ordering::Greater` 是 parity bug |
| 0-3 | §4.3 T1/T4/T6/T8 的“牙” | 四条都只调 world/data 层函数，删 main/guild_cmd 接线仍绿 ⇒ 重写为 §1 的 W1/W4；T3/T5 并入 W2 |
| 0-4 | §3.3 只列 `Closed` 与 `drain_kicks` | 还有第三处 `drain_soft_close`（账号级顶号落地，`main.rs:11945`）；三处都要接 |
| 0-5 | §4.1 写集把 `[Guild]` 六键放 `strings.rs`，但 `EnterConfig` 注释仍写 `GuildFileSections::from_config` | `main.rs:12283-12287` 的注释必须同步改成 `from_strings`，否则下一位实现者会重新引入静默分叉 |
| 0-6 | §1.1 的插入点行号 | G2 已落 `main.rs:14075-14076`（`guild_cfg`/`guilds`）与 `15004-15021`（`handle_guild_cmd`）；G3 以函数/结构名为锚，不用旧行号 |
| 0-7 | §0 口径 1 说 `guild_chief`/`guild_member_rank` 来自 `String.conf [Guild]` | 正确，但 `GuildConfig::from_guild_castle`（`world/guild.rs:143`）目前从 `GuildCastleConfig` 取值 ⇒ G3 必须在构造后**覆盖**这两键（§2.3 S3-8） |

---

## 1. 四条不合格判据的重写（删接线必红）

### 1.0 判定标准与“保鲜”机制

原 T1/T4/T6/T8 的病根：它们直接调 `GuildStore`/`GuildManager::run`/`GuildState` 方法，**删掉
`main.rs`/`guild_cmd.rs` 里的生产调用行，测试仍然全绿**（这与 `main.rs:17745-17780` 记录过的
`drain_deal_tick` 无牙问题同型）。v2 采用该文件已有的两层机制：

1. **行为层**：测试只调生产接缝（新文件 `guild_runtime.rs` 的 `startup`/`step`/`on_disconnect`，
   以及 G2 已有的 `handle_guild_cmd`）。接缝内部每一行接线（加载、`run`、`save_guild`、`check_save`、
   清成员）被删都会改变**影子目录里的文件字节**或**返回的 save/event 列表** ⇒ 红。
2. **保鲜层**：`main.rs` 测试模块内的源码锚点用例，用 `include_str!("main.rs")` 断言
   “调用行存在、trim 后以 `guild_runtime::xxx(` 开头、实参形状匹配、未被注释”
   （照 `drain_deal_tick_production_call_is_wired_in_sim_loop` 的写法；`.lines()` 对 LF/CRLF 都可，
   但按项目要求先确认 `eol=lf` 再跑源码断言类用例；本车道已核
   `git ls-files --eol` ⇒ `main.rs`/`guild.rs`/本简报均为 `i/lf w/lf`）。删/注释/改坏主循环调用点 ⇒ 红。

> 说明：`sim_loop` 本身无法在进程内直达（原文 §4.3 的 T8 也没说清怎么驱到 `GateEvent::Closed`）。
> 因此“删 main 调用点必红”由**锚点**承担，“删接缝内部接线必红”由**行为**承担；两者缺一不可。
> 这不降低判据——它把原文含糊的“驱动 Closed”变成可执行的两条腿。

### W1（替换 T1）`g3_startup_wiring` — 启动加载接线

**驱动**：`guild_runtime::startup(store: Option<&GuildStore>, sections: &GuildFileSections,
rank_name_len: u8, now_tick: i32, server_index: i32) -> GuildManager`（main 的 `sim_loop`
在时钟注入之后调用它；`store=None` ⇒ 空表）。影子目录：
- `GuildList.txt` = GBK `A\r\nB\r\nC\r\n`；
- `Guilds/A.txt`、`Guilds/B.txt` 合法；`Guilds/C.txt` 缺失；
- A 的战表引用 C，B 的战表引用 B；
- `String.conf` fixture 的 `[Guild]` 六键写成**非缺省**值（如 `GuildNotice=公告X`、
  `GuildWar=敌对X`、`GuildAll=联盟X`、`GuildMember=成员X`、`GuildMemberRank=成员X`、`GuildChief=掌门X`）。

**断言**：
1. 会表 = `[A, B]`（名单序，非字母序）；
2. `GuildList.txt` 回写为 GBK `A\r\nB\r\n`；
3. A/B/C 的 `.ini` 都存在且 **0 字节**（C 的 `.ini` 在其 `.txt` 缺失前已建）；
4. A 引用 C 的战条**不**入表；B 的战条存在且 `war_tick == now_tick`、`war_time == 0`；
5. `MissingListFile` ⇒ 空表 + ERR 日志、**不 panic、不拒启**；
6. 段头来自 `from_strings`（fixture 的“公告X/敌对X/…”生效），不是 `from_config`。

**具体变异点（逐条注入 → 必红）**：
- m1 `main.rs` 里 `guild_runtime::startup(` 调用行删除/注释 ⇒ **锚点红**；
- m2 `startup` 内 `store.load_guild_info(...)` 删除 ⇒ 会表空 ⇒ 断言 1 红；
- m3 传 `GuildFileSections::from_config(&guild_castle_cfg)` ⇒ 段头是缺省值 ⇒ 断言 6 红；
- m4 `guild_exists` 用**初始名单快照** ⇒ A 的战条出现 ⇒ 断言 4 红；
- m5 不为名单建 `.ini` ⇒ 断言 3 红；
- m6 `MissingListFile` 当致命错误 ⇒ 断言 5 红；
- m7 名单写回漏最后一个 CRLF ⇒ 断言 2 红。

### W2（替换 T4，吸收 T3/T5）`g3_tick_gate_and_30s_wiring` — 10s 门 / 战争到期 / 30s 门

**驱动**：`guild_runtime::step(guilds, store, sections, server_index, now_tick, run_tick: &mut i32)
-> StepOutcome`。`step` 的生产语义（逐行照 `SystemProcessor.cs:70-77` + `GuildManager.cs:145-167`）：

```
if now_tick.wrapping_sub(*run_tick) > 10_000 {
    *run_tick = now_tick;
    let events = guilds.run(now_tick);          // WarEnded* → Save{UpdateGuildFile} → Save{CheckSaveGuildFile}
    for ev in events { apply: 落盘 / 返回 104 目标 }
}
```

`StepOutcome { ran: bool, war_ended: Vec<...>, saves: Vec<(String, GuildSaveReason, Result<SaveStatus, _>)> }`
只用于判据可读，**不改变** oracle 语义。`save_guild` 的 `MissingSaveTarget` 必须吞掉（oracle catch），
其它错误记 `行会信息保存失败!!!`。

**用例 A（origin=0，未注入）**：加载后 `bo_changed=true, dw_save_tick=0`，`run_tick=0`；
先把 `notices` 改成 `["N2"]`（`N1` 在文件里）以便观测落盘。`step` 依次在 `10000 / 10200 /
20400 / 30600`：
- `10000`：`ran=false`（严格 `>`，不是 `>=`）；
- `10200`：`ran=true`，`war_ended=[]`，`saves=[]`（`10200 > 30000` 假）；
- `20400`：`saves=[]`；
- `30600`：`saves=[(会, CheckSaveGuildFile)]`，文件里 `N2` 出现；`bo_changed=false`。

**用例 B（origin=3_600_000）**：`run_tick=3_600_000`，`dw_save_tick=0` ⇒ `3_610_200 > 30000` 真，
`10200`（相对）首个 `step` 即 `Save{CheckSaveGuildFile}`；`bo_changed=false`，下一跳不再 Save。

**用例 C（战争到期 + 事件序）**：一会两战，`E1{war_tick:100, war_time:50}`、`E2{war_tick:1000, war_time:50}`；
`now=150` ⇒ 无 `WarEnded`；`now=151` ⇒ `WarEnded(E1)` 在前、`Save{UpdateGuildFile}` 在后，
`CheckSaveGuildFile` 不再命中（`DwSaveTick` 刚被置 151）；保存体里 E1 行消失、E2 保留。

**具体变异点**：
- m1 `main.rs` 的 `guild_runtime::step(` 调用行删除/注释 ⇒ **锚点红**；
- m2 `step` 内 `>` 改 `>=` ⇒ 用例 A 的 `10000` 变 `ran=true` ⇒ 红；
- m3 `step` 内 `guilds.run` 调用删除 ⇒ 用例 C 无 `WarEnded`、文件保留 E1 ⇒ 红；
- m4 `step` 内 `apply`/`save_one` 删除 ⇒ 用例 A 的 `N2` 不出现 ⇒ 红；
- m5 调用点传 `now_ms()`（`i64 as i32`，未注入时回绕成负数）⇒ **锚点**（实参形状含
  `world.clock_ms as i32`）红；行为层另注入 `1_700_000_000_000i64 as i32` ⇒ 30s 门永不触发 ⇒ 红；
- m6 `Save{UpdateGuildFile}` 排到 `WarEnded` 之前 ⇒ 用例 C 事件序红；
- m7 把 `bo_changed=false` 提前到 `run` 之前 ⇒ 下一次 `CheckSaveGuildFile` 不再命中 ⇒ 用例 B 红。

### W3（替换 T6）`g3_change_point_saves_wired` — 变更点立即落盘

**驱动**：`handle_guild_cmd`（G2 已有的生产派发入口，`guild_cmd.rs:198`）新增
`store: Option<&GuildStore>`、`sections: &GuildFileSections`、`server_index: i32`、`now_tick: i32`；
测试用手写的 `HashMap<String, GuildSession>`（G2 已有）驱动 9 条 CM，影子目录读文件字节。

**逐点断言**：
| CM | 断言（文件字节 + 状态） |
|---|---|
| 1038 成功 | `<名>.txt` 增加 `+target\r\n`；`bo_changed=true`、`dw_save_tick == now_tick` |
| 1039 删他人 | 文件少 `+target\r\n`；`bo_changed=true`、`dw_save_tick == now_tick` |
| 1039 自删解散 | `<名>.txt` = 空会 7 行体；`GuildList.txt` 不再含该名 |
| 1040 公告 | 公告段逐字节更新；`bo_changed` **保持 false**、`dw_save_tick` 不变 |
| 1041 成功 | ranks 已清 ⇒ 文件里无 `#` 行；落盘且 `bo_changed=true` |
| 1044/1045 | 生产恒失败（769/-4、769/0）；无状态变化 ⇒ 不产生新写（钩子存在即可） |

**具体变异点**：
- m1 1038 臂的 `save_guild` 删除 ⇒ 文件不加 `+target` ⇒ 红；
- m2 1039 臂的 `save_guild` 删除 ⇒ 文件仍含 `+target` ⇒ 红；
- m3 自删解散漏 `save_guild_list` ⇒ 名单仍含该名 ⇒ 红；
- m4 1040 改用 `update_guild_file`（会翻 `bo_changed`/推 `dw_save_tick`）⇒ 红；
- m5 任何臂用 `now_ms()` 而不是传入的 `now_tick` ⇒ `dw_save_tick` 不等 ⇒ 红；
- m6 写出 `.bak` 或创建 `<GuildDir>/<名>/` 子目录 ⇒ 红（`.bak` 永不落盘是 oracle 事实）；
- m7 新建会路径创建 `<名>.txt` ⇒ 红（`FileMode.Open` 吞异常）。

> 写调用次数的口径：oracle 自删解散会写空体**两次**（`CancelGuld` + `DelGuild` 各一次
> `BackupGuildFile`），但两次字节相同 ⇒ 文件层面不可观测。判据只断字节与名单；写次数若要用
> 测试替身计数，需给 `GuildStore` 加 trait（**不推荐**，扩大写集）。登记为 T-9x 不可观测项。

### W4（替换 T8）`g3_disconnect_wiring` — 三处断线接线

**驱动**：`guild_runtime::on_disconnect(guilds, store, sections, server_index,
guild_name: Option<&str>, chr_name: &str, now_tick: i32)`：

```rust
let Some(g) = guild_name.and_then(|n| guilds.find_guild_mut(n)) else { return };
if g.check_save_guild_file(now_tick) { save_one(...); }   // 先落盘
let _ = g.del_human_obj_member(chr_name);                  // 再按 Compare==1 清
```

**三处宿主接线（缺一红）**：
1. `GateEvent::Closed`（`main.rs:15057` 起）：在 `ground_sessions.remove(&chr_name)` **之前**取
   `let guild_name = ground_sessions.get(&chr_name).and_then(|sg| sg.guild.guild_name.clone());`；
   在 party 块之后、`world.disconnect` 之前调 `on_disconnect`（oracle `Disappear()`：退组 → 行会 → 摘世界）。
2. `drain_kicks`（`main.rs:11812`）：`world.disconnect` 之前调；`guild_name` 在
   `ground_sessions.remove` 之前取。
3. `drain_soft_close`（`main.rs:11945`）：同 2。**原简报漏了这处**；oracle 的
   `BoSoftClose → MakeGhost → Disappear` 同样走 `DelHumanObj`。
4. 断线**零行会帧**（不发 750），会话字段随 `ground_sessions.remove` 消失。

**断言**：
- 成员 `[b,c]`、`bo_changed=true, dw_save_tick=0`、`now=40000`，断 `"a"`：
  `check_save` 命中并**先**落盘（文件里仍有 `+b`），随后 `b` 槽变空串；
  `count()==2` 不变、`is_member("b")==false`、`is_member("")==false`、`c` 不动；
  下一次保存写出 `+\r\n`；
- 成员 `[b]` 断 `"c"` ⇒ 不动；成员 `[c]` 断 `"a"` ⇒ **不动**（Compare=2，见 §3）；
- `guild_name=None` ⇒ no-op；
- 三处调用点各有一条源码锚点。

**具体变异点**：
- m1 `Closed` 的 `on_disconnect` 调用删除 ⇒ 锚点红；
- m2 `guild_name` 取在 `ground_sessions.remove` 之后 ⇒ 恒 `None`（行为红；锚点也钉取用顺序）；
- m3 `on_disconnect` 内先 `del_human_obj_member` 后 `check_save` ⇒ 文件缺 `+b` ⇒ 红；
- m4 `check_save_guild_file` 的返回值忽略/不落盘 ⇒ `now=40000` 时文件仍是旧体 ⇒ 红；
- m5 `drain_kicks`/`drain_soft_close` 任一漏调 ⇒ 对应锚点红；
- m6 用 `cmp_ignore_case(...)==Ordering::Greater` ⇒ `[c]` 断 `"a"` 被清 ⇒ 红（§3 的另一条牙）。

### 其余判据的去向

| 原编号 | 去向 |
|---|---|
| T2 文件字节 | 保留为 C2；落盘统一走 `guild_runtime::save_one`，主循环/变更点各有一条锚点 |
| T3 10s 严格 > | 并入 W2 用例 A/C |
| T5 战争到期事件序 | 并入 W2 用例 C |
| T7 Compare | 重写为 §3 的 C7（数值语义 + 矩阵） |
| T9 绑定非在线 | 保留为 C9；必须经 `handle_guild_cmd` 的 104 扇出与 `ref_member_name`，并加“104 在 `step` 的 WarEnded 分支发出”的锚点 |
| T10 104 线形 | 保留为 C10；`Recog=ActorId`、`Param=MakeWord(F,B)`、`Series=1`、尾 GBK |
| T11 非 0 区 | 保留为可选 |

---

## 2. 时钟域统一方案

### 2.1 现状（G2 实测）

- `guild_cmd.rs:198` `handle_guild_cmd(..., now_ms: i64, ...)`；`:243` `now_tick: now_ms as i32`；
  `:515/:581/:688` 用它调 `add_member`/`del_member`/`update_rank`，后三者经
  `update_guild_file(now_tick)` 写 `dw_save_tick`。
- 调用点 `main.rs:15021` 传 `now_ms()`；`now_ms()` = `clock::now_ms()`（`main.rs:16803-16806`）：
  未注入 = **墙钟 epoch 毫秒**（约 `1.76e12`），`as i32` 回绕成任意/负数。
- G3 原方案用 `world.clock_ms as i32`：虚拟毫秒，未注入从 **0** 起、每 tick **+200**
  （`world-bevy/src/tick.rs:60`）。
- 两者不同域 ⇒ G2 写入的 `dw_save_tick` 与 G3 的 `now - dw_save_tick` 完全不同基，
  30s 门要么永不触发、要么乱触发。

### 2.2 唯一时钟：`world.clock_ms as i32`

理由（按优先级）：
1. oracle 只有一个时钟 `HUtil32.GetTickCount()`（`Environment.TickCount`，i32 毫秒、24.9 天回绕）；
   `world.clock_ms` 是项目既定的模拟时钟，`wrapping_sub` 与 i32 回绕语义天然一致。
2. `now_ms()` 未注入时是 epoch，`as i32` 后 30s 门永不触发（正是原文 §0.6 警告的坑）；
   `world.clock_ms` 从 0 起，门至少能工作。
3. 判据可构造：测试直接给 `now_tick` 值即可，无需墙钟/睡眠。
4. 与 G2 的写点统一后，`dw_save_tick` 只有一个写域，30s 门可判。

代价：命令处理发生在事件循环里，`world.clock_ms` 停在上一 tick ⇒ 与 oracle 的“当场
GetTickCount”最多差 **1 tick = 200ms**；登记 T-9x（与 10s/30s 量化同族）。

### 2.3 具体改动点

| # | 位置 | 改动 |
|---|---|---|
| S3-1 | `guild_cmd.rs:198` | 参数 `now_ms: i64` → `now_tick: i32`；删 `now_ms as i32`（`:243`）；`Ctx.now_tick` 直接接收 |
| S3-2 | `main.rs:15021` | 实参 `now_ms()` → `world.clock_ms as i32`（该臂 `world` 是 `&World`，只读，无借用冲突） |
| S3-3 | `guild_cmd.rs` 13 条 G2 测试 | 全部改传 i32（`g2_add_member_codes_and_order`、`g2_del_member_*`、`g2_notice_*`、`g2_update_rank_*` 等） |
| S3-4 | `guild_runtime::startup` 调用点 | `guild_run_tick = world.clock_ms as i32`（加载时，不是 0） |
| S3-5 | `guild_runtime::step` | 门与落盘 `now` 全用传入的 `now_tick`；不再引用 `now_ms()` |
| S3-6 | `guild_runtime::on_disconnect` | 三处调用点传 `world.clock_ms as i32`（事件循环里即上一 tick） |
| S3-7 | 源码锚点 | 断言 `handle_guild_cmd(` 的实参含 `world.clock_ms as i32`，且 `guild_cmd.rs` 无 `now_ms` 标记 |
| S3-8 | `config.rs` / `strings.rs` | 与时钟无关；但 `GuildConfig::from_guild_castle` 之后覆盖 `guild_chief`/`guild_member_rank`（§0-7） |

### 2.4 `MIR2_CLOCK_MS` 与首跳

- Rust 半已授权：`main.rs:14106-14110` 把 `world.clock_ms` 起点设成与 `now_ms()` **同一个 origin**；
  此后仍 +200ms/tick。
- 行会窗口要求 origin 满足 `30_000 < origin <= i32::MAX`（推荐 `3_600_000`）。
  `tests/clock_inject.rs:24` 的 `1_700_000_000_000` 只验证探针，**不可**用于行会窗口。
- 首跳：
  - 未注入（origin=0）：`10200/20400` 无 Save，`30600` 首个 `CheckSaveGuildFile` Save；
  - origin=3_600_000：`3_610_200`（相对 +10200）首次 Save。
- C# 侧不读 `MIR2_CLOCK_MS`；该变量只控 Rust。C# 时钟注入是**另一项 owner 授权**（§4）。

### 2.5 对齐判据（构造级）

1. 在 `world.clock_ms = T` 时走 1038：`dw_save_tick == T`（不是 epoch/负值）；
2. 同一会随后 `step(T + 30_200)`：`CheckSaveGuildFile` 命中；`step(T + 30_100)` 不命中（严格 >）；
3. `T = 3_600_000` 时加载后首个 `step(T+10_200)` 落盘；`T = 0` 时不落盘（30600 才落）；
4. 变异：把 S3-2 改回 `now_ms()` ⇒ 1/2 红（`dw_save_tick` 是回绕值）。

---

## 3. `Compare(...)==1` 的正确语义

### 3.1 oracle 与 .NET 8 实测

`GuildInfo.cs:534-550`：

```csharp
if (string.Compare(guildRank.MemberList[j].MemberName, playObject.ChrName,
                   StringComparison.OrdinalIgnoreCase) == 1)
{
    guildRank.MemberList[j] = default(GuildMember);   // 不是 RemoveAt
    return;
}
```

`string.Compare(a, b, OrdinalIgnoreCase)` 的返回**不是** -1/0/1：逐 UTF-16 码元比较，首个大小写折叠后
不同的码元返回 `ToUpperInvariant(a[i]) - ToUpperInvariant(b[i])`；若折叠后全等，返回 `len(a) - len(b)`。
本机 **net8.0 实跑**（`E:/tmp/laneB-cmp-net8`，`dotnet run -c Release`）：

```
b|a => 1      c|a => 2      c|b => 1      z|a => 25
ab|a => 1     a|ab => -1    aa|a => 1     B|a => 1
b|B => 0      a|a => 0      丁|一 => 1     中|一 => 45
会乙|会甲 => -9945           ""|a => -1    a|"" => 1
```

### 3.2 语义表述（供实现与用例直接引用）

`==1` 命中当且仅当：
- **(A) 首异码元恰大 1**：从左起首个大小写折叠后不同的码元 `ca`、`cb` 满足 `ca - cb == 1`；或
- **(B) 前缀且长 1**：`a` 以 `b` 为前缀（忽略大小写）且 `a` 的 UTF-16 码元数比 `b` 多 **1**。

不命中：相等（0）、负数、以及任何 `>=2` 的差（含所有“确实更大但差不是 1”的名字）。
因此**不是**“第一个比断线者大的成员被清”，而是“第一个与断线者名字差恰为 1 的成员被清”；
同名（0）永不删自己。清空是 `default(GuildMember)`：条目留在表里、名字变 null、`PlayObject=null`；
Rust 用 `""` 占位，序列化为 `+\r\n`，`IsMember` 跳过空名（`GuildInfo.cs:169-171`）。

### 3.3 Rust 实现

新增（放 `world/guild.rs`，或 `shared` 的字符串原语旁）：

```rust
/// .NET 8 `string.Compare(a, b, StringComparison.OrdinalIgnoreCase)` 的数值返回。
fn dotnet_compare_ordinal_ignore_case(a: &str, b: &str) -> i32 {
    let a: Vec<u16> = a.encode_utf16().collect();
    let b: Vec<u16> = b.encode_utf16().collect();
    let n = a.len().min(b.len());
    for i in 0..n {
        if a[i] == b[i] { continue; }
        let ca = dotnet_upper_invariant(a[i]);
        let cb = dotnet_upper_invariant(b[i]);
        if ca != cb { return i32::from(ca) - i32::from(cb); }
    }
    i32::try_from(a.len()).unwrap_or(i32::MAX) - i32::try_from(b.len()).unwrap_or(i32::MAX)
}
```

- `dotnet_upper_invariant(u16)`：复用现有 `to_upper_char`（`shared/hutil32.rs:22`，一对多保持原字符）。
  已知分叉：`char.ToUpperInvariant('ı')='ı'`、`'ß'='ß'`，而 Rust `to_uppercase` 会把 `ı→I`；
  这些字符 GBK 不可编码 ⇒ 部署内容不可观测，登记 T-9x；不要为此扩大全局 `to_upper` 的口径。
- `del_human_obj_member(&mut self, name: &str) -> bool` 内用
  `dotnet_compare_ordinal_ignore_case(&members[j].name, name) == 1`；
  **禁止**用 `cmp_ignore_case(...).cmp == Ordering::Greater`。
- 空槽/null 等价：oracle `Compare(null, name) = -1`，Rust `Compare("", name) = -len`；两者都 ≠1（同为空时都 0）⇒ 行为等价。
- `cmp_ignore_case` 仍供 `CancelGuld`/`GetRankName` 等**只判等**的点使用，不要改其语义。

### 3.4 用例矩阵（C7）

| 成员 | 断线者 | Compare | 期望 |
|---|---|---|---|
| b | a | 1 | 清空（A） |
| **c** | **a** | **2** | **不动**（区分 `==1` 与 `Greater`） |
| z | a | 25 | 不动 |
| ab | a | 1 | 清空（B） |
| a | ab | -1 | 不动 |
| aa | a | 1 | 清空（B） |
| B | a | 1 | 清空（A，大小写折叠） |
| b | b | 0 | 不动 |
| 丁 | 一 | 1 | 清空（A，CJK 相邻码位） |
| 中 | 一 | 45 | 不动 |
| 会乙 | 会甲 | -9945 | 不动 |
| （空槽） | a | -1 | 不动 |

附加断言：命中后条目仍在该 rank、`count()` 不变、`is_member("")==false`、下一次保存出现 `+\r\n`。

**变异点**：`==1`→`!=0`/`>0` ⇒ c/a、z/a、中/一 红；`==1`→`==0` ⇒ b/a 红；
去掉长度分支 ⇒ ab/a、aa/a 红；用 `Ordering::Greater` ⇒ c/a 红；按字节而非 UTF-16 码元（CJK 在两处一致，
但 ASCII 前缀分支会红）。

---

## 4. C# 时钟注入授权边界（owner 项）

### 4.1 构造级（**无需** C# 授权，随时可做）

W1/W2/W3/W4 的进程内行为 + C2/C7/C9/C10 + 全部源码锚点 + 变异注入。这些只用 Rust 侧
`world.clock_ms`/显式 `now_tick` 与影子目录，不依赖 C# 进程。

### 4.2 真机窗口 A：字节窗口（需 owner 窗口，**不需** C# 时钟注入）

- 前置：C# 机器**开机 >30s**（`Environment.TickCount > 30000`，保证首个 10s Run 就能过 30s 门）；
  Rust 侧 `MIR2_CLOCK_MS=3600000`；两侧影子 `GuildBase/`（`GuildList.txt` + 两会 `.txt`）。
- 断言：启动后首个 10s Run（C# ~+10.05s、Rust ~+10.2s）两会 `<名>.txt` 各被重写一次；
  战表行消失、公告/成员段逐字节不变；`GuildList.txt` 字节不变；`.ini` 仍 0 字节；无 `.bak`。
- 掉线：构造“成员名与断线名差为 1”与“差为 2”两组，观察下一次保存的 `+\r\n` 是否出现。
- 这一半可做，但 review 仍按 `tier=R` 处理；只完成 A 不算全绿。

### 4.3 真机窗口 B：必须 C# 时钟注入（**blocked**，owner 未授权）

以下任一断言出现，就必须先拿到 owner 对 C# `MIR2_CLOCK_MS`（或等价钩子）的授权：
- B1 断言 C# 的 `dw_save_tick` **绝对数值**（或与 Rust 注入 origin 相等）；
- B2 断言 10s/30s **精确边界**（`10000`/`30000` 的严格 `>`）跨越 50ms 扫描点；
- B3 断言 i32 回绕（`dw_save_tick` 近 `i32::MAX`）行为；
- B4 把战争到期（50ms 量级）与 Rust 的 200ms 量化做逐毫秒对拍。

未授权前的正确写法：这些项写 `not_verified` / `window_required:true`，**不得**用“语义等价”放行。

### 4.4 review 措辞

- `tier: R`（持久化字节 + 时序），`tier_why` 点名 `SaveGuildInfoFile`/`CheckSaveGuildFile`/
  `DelHumanObj` + 10s/30s 门；
- 只完成 §4.1 ⇒ `approve + window_required:true`，缺 A/B 列清单；
- 完成 A 未完成 B ⇒ 仍 `window_required:true`，明写“缺 C# 时钟注入授权”；
- A+B 都绿才可 `approve` 且不带 window 标记。

---

## 5. 更新后的 G3 DoD

### 5.1 写集（文件级）

| # | 文件 | 改动 |
|---|---|---|
| 1 | `mir2-rs/crates/world/src/guild.rs` | `GuildMember{name, actor_id: Option<i32>}`（全读写点）；`dotnet_compare_ordinal_ignore_case`；`del_human_obj_member`/`del_human_obj` 拆两步；`GuildRunEvent{WarEnded,Save}` + `GuildSaveReason`；`run -> Vec<GuildRunEvent>`；`to_record`；`names`；`del_guild -> Option<GuildRecord>`；`ref_member_name`/`send_guild_msg_targets` 改按绑定；改 3 条既有单测（`:1372`、`:1423`、`:1820`） |
| 2 | `mir2-rs/crates/data/src/guild.rs` | 新增 `GuildFileSections::from_strings(notice, war, all, member)`（保留 `from_config` 但 G3 不用）；**可选**非 0 区路径 `.<idx>`→`<idx>`（不可观测，建议另卡） |
| 3 | `mir2-rs/bins/game-svr/src/guild_runtime.rs`（**新建**） | `startup`/`step`/`on_disconnect`/`save_one`/`StepOutcome`；全部可进程内驱动；只做接线，不含新语义 |
| 4 | `mir2-rs/bins/game-svr/src/guild_cmd.rs` | `now_tick: i32`；`Ctx` 增 `store/sections/server_index`；§2.4 各变更点落盘；104 扇出按绑定；13 条 G2 测试改签名 |
| 5 | `mir2-rs/bins/game-svr/src/main.rs` | `EnterConfig` 增 `guild_store/guild_sections/guild_chief/guild_member_rank`（注释同步改 `from_strings`）；`main()` 建 store；`sim_loop` 加载 + `guild_run_tick` + `step`；三处断线；`handle_guild_cmd` 传 `world.clock_ms as i32`；源码锚点用例 |
| 6 | `mir2-rs/bins/game-svr/src/strings.rs` | `String.conf [Guild]` 六键 + `[String] GuildMsgPreFix`（缺省 `〖行会〗`） |
| 7 | `mir2-rs/bins/game-svr/src/config.rs` | `[Setup] GuildMsgFColor/GuildMsgBColor`（缺省 `0xDB/0xFF`；`GuildRecallTime` 可同批） |
| 8 | `mir2-rs/tests/parity/whitelist.md` | T-9x 登记（§5.4） |

**不碰**：`src/**`、`sql/**`、`MirClient/`、`crates/protocol`、`crates/storage`、`E:/MirServer`、
`guild_castle_scenario.json`。`E:/tmp/wg2`、`E:/tmp/wslots` 两条车道不进入。

### 5.2 串行顺序（同一 worktree 内）

```
S1 world/guild.rs（纯状态 + Compare + 事件）           → cargo test -p mir2-world
S2 data/guild.rs（from_strings）                        → cargo test -p mir2-data
S3 strings.rs / config.rs（解析）                       → cargo test -p game-svr strings/config
S4 guild_runtime.rs（接缝 + 单测，暂 allow(dead_code)） → cargo test -p game-svr guild_runtime
S5 guild_cmd.rs（时钟 + 落盘 + 104）                    → 13 条 G2 用例全绿
S6 main.rs（接线 + 三处断线 + 源码锚点）                → cargo test --workspace + 锚点
S7 whitelist.md + 证据四件套                            → 四门禁
```

理由：S1/S2 是 S3-S6 的 API 基础；S4 先于 S5/S6 是为了让“生产接缝”在接线前就有行为覆盖；
S6 最后动 `main.rs`，把与 `/e/tmp/wg2`、`/e/tmp/wslots` 的合并冲突面压到一次 rebase。

### 5.3 DoD 清单（逐条勾）

- [ ] S1-S7 完成，`git status` 干净；`git diff --stat <基线> -- src/ sql/ MirClient/ crates/protocol crates/storage` 为空；
- [ ] `cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、
      `cargo test --workspace`、`cargo run -p msgcodegen -- --check` 全绿（用本车道 `CARGO_TARGET_DIR`）；
- [ ] W1/W2/W3/W4 行为用例 + 源码锚点全绿；锚点覆盖 `startup/step/on_disconnect` 与
      `handle_guild_cmd` 的 `world.clock_ms as i32` 实参；
- [ ] 变异日志：§1 每条变异“注入 → 指定用例红 → 还原（sha256）”有记录；
- [ ] C7 Compare 矩阵 12 行全绿（含 c/a、中/一、ab/a）；
- [ ] §4.1 构造级全绿；§4.2 窗口 A 由 owner 执行并留证据；
- [ ] §4.3 B1-B4 若未授权：`review` 写 `window_required:true` + 缺项清单；
- [ ] T-9x 已写 `whitelist.md`；`review` 带 `tier=R` + `tier_why`。

### 5.4 白名单（T-9x，编号避让其他车道）

1. **节拍量化**：world tick 200ms vs oracle 50ms 扫描；10s 门首跳 +10200（oracle +10050）、
   30s 边界最多晚 150ms、战争到期最多晚 200ms；命令处理读上一 tick 的 `world.clock_ms`（≤200ms 偏差）。
2. **未注入首存 30600**：`world.clock_ms` 从 0 起，`dw_save_tick=0` 的首次 30s 保存落在 30600
   （oracle 开机 >30s ⇒ 10200）；窗口强制 `MIR2_CLOCK_MS=3600000`，epoch 量级会 i32 回绕。
3. **顶号同 tick 清理**：oracle `MakeGhost` → 下一次 `ProcessPlayObjectData`（≤250ms）才 `DelHumanObj`；
   Rust 在 `drain_kicks`/`drain_soft_close` 同 tick；仅写盘相位不同；`drain_kicks` 目前不建模 party
   `DelMember`（既有缺口，不本卡补）。
4. **保存编码**：Rust 恒 GBK；oracle 按 BOM 嗅探（UTF-8/UTF-16 文件会被原编码重写）；部署件无 BOM。
5. **加载失败降级**：`.ini` 创建失败 oracle 抛到 `InitializeWorld` catch；Rust 记 ERR、空表（fail-closed）。
6. **绑定是加载期快照**：`actor_id` 可能在重登后复用；oracle 发向旧 `PlayObject` 指针，Rust 按
   `actor_id` 找当前会话；极端情况下收件人不同。部署单区小规模不可观测。
7. **`to_upper_char` 与 .NET 的少数分叉**（`ı`/`ß`）：GBK 不可编码 ⇒ 部署不可观测。
8. **自删解散写空体两次**：oracle 两次 `BackupGuildFile`；Rust 可只写一次（字节相同），写次数不可观测。

### 5.5 收尾

- 删除本车道 `CARGO_TARGET_DIR`；worktree `git worktree remove`；分支已合则删、未合写明；
- 本文件残留：仅 `E:/tmp/laneB-guild-g3-v2.md`（与本车道核验用的 `E:/tmp/laneB-cmp-net8/` 只读探针，
  已删除编译产物）；无进程、无端口、无 `E:/MirServer` 改动。

---

## 6. 复核命令（只读）

```bash
git rev-parse HEAD                                   # 3bdfa0888
sed -n '145,167p' src/M2Server/Guild/GuildManager.cs
sed -n '525,550p;584,621p' src/M2Server/Guild/GuildInfo.cs
sed -n '70,77p'   src/GameSrv/Word/Threads/SystemProcessor.cs
sed -n '169,180p' src/M2Server/Guild/GuildInfo.cs     # IsMember 跳过空名
sed -n '463,485p;640,660p' src/M2Server/Guild/GuildInfo.cs   # 绑定赋值点
grep -n "now_ms as i32\|now_tick" mir2-rs/bins/game-svr/src/guild_cmd.rs
grep -n "fn handle_guild_cmd\|handle_guild_cmd(" mir2-rs/bins/game-svr/src/main.rs
grep -n "fn drain_kicks\|fn drain_soft_close\|GateEvent::Closed" mir2-rs/bins/game-svr/src/main.rs
sed -n '17745,17780p' mir2-rs/bins/game-svr/src/main.rs   # 源码锚点既有写法
# .NET 8 Compare 探针（临时目录，跑完删）：
#   dotnet run -c Release  in E:/tmp/laneB-cmp-net8
```

---

## 7. 残留物

- 仅 `E:/tmp/laneB-guild-g3-v2.md`。
- 探针目录 `E:/tmp/laneB-cmp-net8/` 用完即删（`bin/`、`obj/` 一并删）；不留在 `E:/tmp` 顶层。
- 无 worktree、无 `CARGO_TARGET_DIR`、无编译产物入仓、无进程、无端口。


---

## 评审员更正（2026-10-20，**v2 仍不可直接开工**）

- **2 处 parity 漏洞**：断线 104 的退出提示被漏；`CM_SOFTCLOSE` 的生命路径未入清单。
- **6 处假绿/不可执行**：W2 用例 C 与 10s 门矛盾（`now=150/151` 时 `151-0>10000` 为假，须显式把 `run_tick` 设到 `now-10001`）；
  W1 的顺序牙不存在（fixture `A,B,C` 与字母序重合，改 `B,A,C`）；W1 只测了 4 个段头（`GuildChief`/`GuildMemberRank` 无断言）；
  W3 丢了 `add_guild` 用例（m7 不可执行）；W3 缺「自删解散漏 `save_one`」的变异点；
  W4 的「下一次保存写出 `+
`」是错的（`CheckSaveGuildFile` 只置 `BoChanged=false`、不更新 `DwSaveTick`）。
- 成立并已核实：Compare 返回**码位差**（net8.0 实跑：b/a=1、c/a=2、z/a=25、会乙/会甲=-9945），Rust 用 `Ordering::Greater` 是真实 parity bug；
  时钟域统一为 `world.clock_ms as i32`；断线有三处（`Closed`/`drain_kicks`/`drain_soft_close`）。
