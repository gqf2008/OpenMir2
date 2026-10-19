# Lane A：登录期（进图/进世界）技能与装备校验面审计

- 仓库基线：`c2ac5cccb`（master）；只读车道：未建 worktree、未改仓库、未起服务、未碰 `E:/MirServer`。
- 触发证据：`E:/tmp/window/cliente2e/csharp/s2c-621b/capture.jsonl`
  - seq2 `s2c 658` → seq3 `c2s 1018(CM_LOGINNOTICEOK)` → **seq4 `s2c 212`** → seq5 `s2c 51(SM_NEWMAP)` → 50/41/213/11029/42/766/54/5008/1106 → 52/752 → seq22 621 → seq23 811 → seq24 46。
  - seq4 载荷：`ident=212, recog=1, param=0, tag=0, series=1, body_len=0`，raw_hex `236564686b6458686a6464686b6465686b21`。
  - 实测落库：登出后 `characters_magic` 里 `MagicId=1` 的行消失（`docs/window-shrink-run-20261019.md` §10）。
- 现有登记（不重复当新发现）：`docs/window-shrink-run-20261019.md` §10 已记「C# 登录删职业不可用技能并发 212 且落库，Rust 未观察到」；52/752 与 621 的出队序差异另案（`openmir2-login-frame-order-52-752`）；`ability.rs` 模块头「未做清单」已登记「套装/特殊 Shape 巨型 else-if 块（`PlayObject.cs:2538-2849`）未实现」，`T-17` 记「RecalcAbilitys 的套装尾段未建模」。

---

## 1. 结论先行（回答第 5 问）

**是，Rust 缺整条 `SM_DELMAGIC(212)` 链，也缺与之同源的 `SM_ADDMAGIC(210)` 链。**
Rust 目前只有协议常量（`mir2-rs/crates/protocol/src/messages.rs:219-221`），全仓无任何 210/212 出帧点，`ability.rs` 也完全不建 `FlameRing/RecoveryRing` 标志。

最小实现面（写集）见 §5；核心是：oracle 在 **`PlayObject.RecalcAbilitys()` 尾段**按两枚戒指 Shape 决定 `AddItemSkill`/`DelItemSkill`，登录时这条路径恰好跑一次（`PlayObject.cs:1309`），删除结果同时进 `211` 与存档。

---

## 2. Oracle 登录期校验链（WorldServer.MakeNewHuman/GetHumData → PlayObject）

调用链：
`ProcessHumans`（`src/GameSrv/Word/WorldServer.cs:484`）→ `ProcessHumansMakeNewHuman`（`:221`）
→ `GetHumData`（`:1100`）→（`RunNotice` 等 `CM_LOGINNOTICEOK`，`:653-661` / `PlayObject.cs:1088-1119`）
→ `PlayObject.UserLogon`（`src/M2Server/Player/PlayObject.cs:1152`）。

| # | 校验/改写 | oracle 位置 | 帧 | 落库 | 触发条件 |
|---|---|---|---|---|---|
| V1 | 魔法模板存在性过滤：`FindMagic(MagIdx)==null` 的技能**静默丢弃**（不加入 `MagicList`） | `WorldServer.cs:1198-1213`（`FindMagic(int)` `:1430`） | 无 | 是（下次 `MakeSaveRcd` 只写内存列表） | 记录 `MagicId` 不在 `magics` 表 |
| V2 | `HumItems` 槽位映射：`UseItems[Helmet]=humItems[Necklace]`、`UseItems[Necklace]=humItems[Helmet]`（交换 bug） | `WorldServer.cs:1153-1176` | 无（影响后续 50/41/621/52） | 存档再交换一次（`:1302-1303`） | 恒 |
| V3 | 背包只装 `Index>0`；仓库同 | `WorldServer.cs:1178-1196`、`:1216-1228` | 无 | 是 | 恒 |
| V4 | 新手四件物品入包（蜡烛→金创药→木剑→布衣男/女） | `PlayObject.cs:1205-1245` | 间接进 52 的 `WAbil.Weight` | 是 | `IsNewHuman`（`HomeMap` 空且 `Abil.Level==0`） |
| V5 | 背包物品存在性：`GetStdItemName(Index)` 为空 ⇒ `Dispose` + 从 `ItemList` 删 | `PlayObject.cs:1247-1256` | 无 | 是 | `Index=0` 或模板缺失 |
| V6 | 装备槽合法性（仅槽位↔`StdMode`）：`!CheckUserItems(slot,stdItem)` ⇒ `AddItemToBag`，满包则 `ItemList.Insert(0,...)`；`Index=0`。模板为 null ⇒ 直接 `Index=0` | `PlayObject.cs:1258-1283`；`M2Share.cs:376` | 无（621 之后自然少该槽） | 是 | `Config.CheckUserItemPlace`（默认 true，`GameSvrConf.cs:1885`） |
| V7 | 背包复制品：同名 + 同 `MakeIndex` ⇒ 删除较早的那个 | `PlayObject.cs:1286-1301` | 无 | 是 | 存在重复 `MakeIndex` |
| V8 | `Initialize()`：**`MagicList[i].Level >= 4 ⇒ Level=0`**；`AddBodyLuck(0)` | `PlayObject.cs:1050-1061`（登录调用点 `:1175`） | 无（211 反映 0） | 是 | 记录里技能等级 ≥4（GM/脚本均封顶 3，仅改库可达） |
| V9 | **技能职业校验（仅两条）**：非法师删「火球术」、非道士删「治愈术」，删首条匹配并发 `SM_DELMAGIC(212)` | `PlayObject.cs:5103-5124`（Del）、`:5089-5102`（DeleteNameSkill）、`:2764-2779`（调用点）；`PlayObject.Base.cs:2218-2222`（出帧） | **212**，`Recog=MagIdx, Param=0, Tag=0, Series=1` | 是 | 见下 |
| V10 | 戒指授予技能：`Ringl/Ringr` 且 `Shape==115(火球)`/`116(治愈)` 且 `Dura>0` ⇒ 若列表无该 `MagicId`，追加 `Level=1` 并发 `SM_ADDMAGIC(210)` | `PlayObject.cs:2226-2249`（置标志）、`:3907-3935`（AddItemSkill）、`:2764-2779`；`PlayObject.Base.cs:2205-2216` | **210**，`Recog=0, Param=0, Tag=0, Series=1`，尾 `EnCodeBuffer(ClientMagic)` | 是 | 见下 |
| V11 | `Abil.MaxExp = GetLevelExp(Level)`；`WAbil.Exp/MaxExp` 同步 | `PlayObject.cs:1310-1312` | 间接进 52 | 否（下次升级/存档再落） | 恒 |
| V12 | `BtB2==0 ⇒ PkPoint=0; BtB2++`；Gold 上限钳制 | `PlayObject.cs:1313-1319` | 无 | 是 | 老记录 `BtB2==0` |
| V13 | 账号/版本/密码锁/禁登/`GetSellOffGlod`（元宝寄售结算） | `PlayObject.cs:1320-1482` | 若干 SysMsg（11004 等） | 部分 | 配置/记录状态 |

### V9/V10 的精确语义（逐字对齐要点）

1. `DelItemSkill(nIndex)`（`PlayObject.cs:5103`）：`Race != Play` 直接返回；`nIndex==1 && Job != Wizard` ⇒ `DeleteNameSkill(Config.FireBallSkill)`；`nIndex==2 && Job != Taoist` ⇒ `DeleteNameSkill(Config.HealSkill)`。
2. `DeleteNameSkill`（`:5089`）：遍历 `MagicList`，用 **`userMagic.Magic.MagicName == sSkillName`（C# `==`：序数、区分大小写）** 找**第一条**，`SendDelMagic` 后 `RemoveAt(i)`，`break`。**只删一条**；重复项要等下一次 `RecalcAbilitys` 再删。
3. `AddItemSkill`（`:3907`）：`WorldEngine.FindMagic(name)`（**`OrdinalIgnoreCase`**、线性首匹配，`WorldServer.cs:1559`）；`!IsTrainingSkill(MagicId)`（按 `MagIdx` 查，`PlayObject.Attack.cs:340`）⇒ 追加 `Level=1, Key=0, TranPoint=0` 并发 210。
4. 顺序：先火球分支（`FlameRing` 真 ⇒ Add，否则 Del），再治愈分支。`Dura==0` 的槽在 `:2105-2120` 就 `continue`，**不置戒指标志**。
5. **`FlameRing` 是粘滞锁存（oracle bug，必须照抄）**：`RecalcAbilitys` 顶部只把 `RecoveryRing=false`（`:2017`），**从不重置 `FlameRing`**；全仓唯一写入点是 `:2245` 的 `FlameRing = true`。后果：
   - 非法师一旦戴过火球戒指，本会话内**永远**走 Add 分支、不再 Del（脱下/持久归零都不删），直到**下次登录**新对象才 `FlameRing=false` ⇒ 登录期 212 反而是这条链的**正常**落点。
   - `RecoveryRing` 每次重扫归零 ⇒ 道士以外角色脱下/打碎治愈戒指，同会话下一次 Recalc 就删并 212。
   - 实现 Rust 时：若 `ability.rs` 返回的是「本次命中」，会话层必须 `flame_ring |= hit`（与 `Revival` 同法），`recovery_ring = hit`；不能两者都按本次命中直接判定。
6. 战士同时有两条火球且无戒指 ⇒ 每次 `RecalcAbilitys` 只删一条（`break`）。
6. **除火球/治愈外，oracle 登录期不校验技能职业**：战士带雷电术、法师带施毒术等都会原样保留并进 211。

### 明确不存在的登录期校验（不要臆造）

- **Need/等级/职业/性别/负重**：只在 `CheckTakeOnItems`（`PlayObject.Base.cs:1730-1830`，装备动作 `PlayObject.Operate.cs:408`）里校验；登录期**完全不查**。登录期只查 V6 的「槽位↔StdMode」。
- **过期物品/限时物品**：本 oracle 登录链**没有**任何物品时限校验。唯一的「过期」是账号游戏时长（`PlayObject.cs:3937-3963`），由消息循环驱动，不是登录校验；`GetSellOffGlod` 只是元宝寄售结算。
- **技能等级/职业/修炼点**：登录期不校验（除 V8 的 ≥4 归零）。

---

## 3. Rust 现状对照

| oracle 项 | Rust 位置/现状 | 判定 |
|---|---|---|
| V2 头盔/项链交换 | `main.rs::use_items_from`（4609-4616）+ `human_items_for_save`（4670-4683）已交换、存档再换回 | ✅ 已实现 |
| V3 背包 `Index>0` | `main.rs::bag_items_from`（4561） | ✅ |
| V4 新手四件 | `main.rs::add_newbie_items`（2767-2810） | ⚠ 已实现但**顺序错**：在 `ability_inputs`/`recalc_for_session` **之后**加入（14552-14575），52/752 的 `WAbil.Weight` 不含新手物品；oracle 是先入包再 `RecalcAbilitys` |
| V1 魔法模板过滤 | **未实现**：`enter.rs::send_my_magic`（582-611）遇到 `table.get(mag_idx)==None` 直接 `Err` 并**跳过整条 211**；且不滤 `MagIdx==0` 的空槽 | ❌ 未实现（且见 §6 的 211 缺陷） |
| V5 背包非法物品删除 | **未实现**（无 `GetStdItemName` 校验；`bag_items_from` 只滤 `index>0`） | ❌ |
| V6 登录期槽位检查 | **未实现**；`bag.rs::check_user_items` 只服务 `CM_TAKEONITEM`（`bag.rs:305`） | ❌ |
| V7 复制品删除 | **未实现** | ❌ |
| V8 技能等级≥4 归零 | **未实现** | ❌ |
| V9 非本职业火球/治愈 + 212 | **未实现**；`ability.rs` 的 `RecalcOutput` 无戒指标志；无 `sm_delmagic` 构造器 | ❌（本 lane 主项） |
| V10 戒指授予技能 + 210 | **未实现**；`ability.rs` 只建模 Shape 114/133/107/108 等少数分支（模块头「未做清单」已登记巨型 else-if 块） | ❌（同源缺件） |
| V11 `Abil.MaxExp=GetLevelExp` | 存档侧有重算（`main.rs:4911-4917`），但登录 52 用的是记录 `Abil`；`recalc_abilitys` 不覆盖 `max_exp` | ⚠ 差异（次要，需另判） |
| 52/752 与 621 的出队序 | 已知差异：真机 `52,752,621,...`，模型 C′ 在 621/211 出帧时预测 `52,621,211,752,...` | 已登记（`docs/window-shrink-run-20261019.md` §10、`openmir2-login-frame-order-52-752`），本 lane 不重复 |

---

## 4. 可达性

1. **V9 登录期 212（非本职业火球/治愈）**
   - **火球：正常玩法直接可达（且登录期是主落点）**。战士/道士戴 `Shape=115` 火球戒指 ⇒ `AddItemSkill` 把火球写进 `MagicList` 并存档；`FlameRing` 是粘滞锁存（见 §2-5），脱下/打碎戒指的**本会话内不会删**，技能随存档留在 DB ⇒ 下一次登录（新对象 `FlameRing=false`、无戒指）就在登录期删技能、发 212、落库删除。这正是本次实测观察到的链。
   - **治愈：正常玩法可达，但落点在会话内**。`RecoveryRing` 每次 Recalc 归零 ⇒ 脱下/打碎戒指的同一次 `RecalcAbilitys` 就删并 212；登录期触发需要「DB 里有治愈但当前没戴戒指」（崩溃未回写、脚本/GM/改库）。
   - GM/脚本入口**不校验职业**（`TrainingMagicCommand.cs:24-57`、`ExecutionProcessingSys.cs:2635-2663`）；普通角色靠技能书学不会跨职业技能（`ReadBook` 有 `magic.Job == 99 || magic.Job == Job` 门，`PlayObject.Base.cs:2172-2195`）。
   - 注意：**除火球/治愈外的跨职业技能会原样保留**（战士带雷电术不会在登录期被删）。
2. **V10 戒指 210**：正常玩法直接可达（打到/买到 115/116 戒指戴上即触发；116 每次 Recalc 都可能重新 Add，若列表里没有）。
3. **V1 魔法模板缺失**：改 `magics` 表/表未加载时才可达；部署内容一般稳定。
4. **V5/V6/V7 物品面**：改 `stditems`/改库/复制道具事故时可达；正常掉落不会产生未知 `Index`。
5. **V8 技能等级≥4**：GM 与脚本都封顶 3，仅改库可达。
6. **Need/等级/职业/负重、物品过期**：登录期**没有**这些校验，所以不存在「Rust 漏了」的问题；不要在验收里伪造这些场景。

---

## 5. 最小实现面（写集）——仅补 212/210 链

按依赖序：

1. `mir2-rs/crates/envir/src/conf.rs`（+ 单测）
   - 新增 `fire_ball_skill: String`、`heal_skill: String`，读 `Server.conf [Names] FireBallSkill/HealSkill`（`ServerConf.cs:80-81`），缺省 `火球术/治愈术`（`GameSvrConf.cs:1407-1408`）。
2. `mir2-rs/bins/game-svr/src/magics.rs`
   - 新增 `MagicTable::find_by_name(&str) -> Option<&MagicInfo>`：线性首匹配 + 等价 `OrdinalIgnoreCase`（供 AddItemSkill 用）；Del 侧按记录的 `MagIdx` 查 `get()` 取 `magic_name` 做**区分大小写**比较（照 `DeleteNameSkill`）。
3. `mir2-rs/bins/game-svr/src/ability.rs`
   - `RecalcOutput` 增 `flame_ring: bool`、`recovery_ring: bool`（语义 = **本次 Recalc 是否命中**）；
   - 在现有 `for (slot, cell) in inputs.use_items` 循环（`:503-552`）里、`inst.dura == 0` 的 `continue` **之后**，加 Shape 115/116 判定（只认 `SLOT_RING_L/SLOT_RING_R`）；
   - 会话层消费时必须照 oracle：`flame_ring` 是**粘滞锁存**（`|=`，永不重置；`PlayObject.cs` 全仓只有 `FlameRing = true`），`recovery_ring` 是每次重算（直接取本次值）。登录是新对象 ⇒ 首次值即本次命中。
4. 新模块 `mir2-rs/bins/game-svr/src/magicaudit.rs`（建议，避免再往 `enter.rs` 堆）
   - 纯函数 `audit_login_magic(job, magics: &mut Vec<Option<MagicRcd>>, table, fire_name, heal_name, flags) -> Vec<MagicEvent>`；
   - 严格照 V9/V10 的「先火球后治愈、Del 只删首条、Add 查重 by MagIdx、比较口径不同」；
   - 返回事件序（`Del(MagIdx)` / `Add(MagicInfo)`）供出帧，并在函数内就地改列表（等价 `MagicList`）。
5. `mir2-rs/bins/game-svr/src/smframe.rs`
   - `sm_delmagic(mag_idx: u16)`：`command_header(mag_idx, SM_DELMAGIC, 0, 0, 1)`，无尾；
   - `sm_addmagic(def: &MagicInfo)`：`command_header(0, SM_ADDMAGIC, 0, 0, 1)` + `edcode::encode(encode_client_magic(0, 1, 0, def))`；
   - `EnterWorldInputs` 增 `magic_journal: Vec<MagicEvent>`，在 `frames()` 的 **`sm_sendnotice(658)` 之后、队列出队之前**依次产出（真机序：658 → 212 → 51）。
6. `mir2-rs/bins/game-svr/src/main.rs`（进世界块，约 14270-14550）
   - 在 `let data = full.as_ref()...` / 构造 `ability_inputs` **之前**：
     a) 先做 V1 过滤（丢弃 `MagIdx==0` 与模板缺失项，并同步删掉记录里的槽，才能让存档删行）；
     b) 用 `enter::recalc_for_session` 的输出去取 ring flags（或单独调一次 ability）；
     c) `audit_login_magic` 就地改 `full` 的 `data.magic`；
     d) 把事件序塞进 `EnterWorldInputs.magic_journal`；
   - `ability_inputs.magics` 自然引用改后的列表（211 用）；
   - 存档无需改：`build_save_record` 的 `rec.data.magic` 即内存态；db-srv `save_magics` 已是 `DELETE + INSERT(MagIdx>0)`（`playdata.rs:687-711`），行会消失。
7. 测试
   - `ability.rs` 单测：115/116 命中与 `Dura==0` 不命中；
   - `magicaudit` 单测：§7 的 A/B 组；
   - `smframe` 字节单测：`sm_delmagic(1)` 对照 `capture.jsonl` seq4 的 `raw_hex`（注意经 `client_frame` 后与网关编码比对）；
   - `enter`/`main` 集成：帧序 `658,212,51,...` 与 `characters_magic` 落库。
8. （同批建议带上）211 空槽修复：见 §6，否则「有效技能」场景仍出不了 211。

**不建议**把 212 简化成「登录时无条件按职业过滤两条技能」：会漏掉戒指 Add 分支、重复技能只删一条、以及 `Dura==0` 的戒指门。

---

## 6. 额外发现（本 lane 顺带，需另立/并卡）

1. **211 在真实记录上很可能永远不发（新发现，置信高）**
   - db-srv `get_magic_record` 把 20 个槽全部填成 `Some(MagicRcd::default())`（`playdata.rs:206`，`mag_idx=0`）；wire 上不是 NULL 对象（`storage::playerdata` 的 `MagicRcd::default().to_bytes()` = `04 0000...`）。
   - game-svr `send_my_magic` 遍历 `magics.iter().flatten()`，遇到 `mag_idx=0` 时 `table.get(0)` 为 `None`（`sql/mir2_data.sql:70` 起 MagicId 从 1 开始、无 0 行）⇒ 返回 `Err`，**整条 211 被跳过**（`enter.rs:590-611`）。
   - 结论：只要记录里有一个空槽（所有真实记录都有），211 就不发；「有武器+有效技能」的真机样本预计仍缺 211。这与 oracle「只发非空 `MagicList`、`Count`=实际条数」不同。
   - 修法：211 前过滤 `mag_idx != 0` 与模板缺失项（同时这也是 V1 的实现点）。
2. **新手物品重量顺序**：见 §3 V4，52 的 `WAbil.Weight` 会少四件新手物品，且 ground 会话的负重判定同源。
3. **`Abil.MaxExp` 登录重算**：52 的 `MaxExp` 需要按 `GetLevelExp(Level)` 覆盖（`PlayObject.cs:1310`），Rust 的 `recalc_abilitys` 不写 `max_exp`；记录里的值过期时 52/升级阈值会差。建议与 V11 一起判。
4. **`AddItemSkill` 的重复添加 bug + `FlameRing` 粘滞要照抄**：`FlameRing` 真时只 Add、不 Del；若列表已有同 `MagIdx` 则 `IsTrainingSkill` 挡住。并且 `FlameRing` 一旦置真在本会话内**永不回落**（`RecoveryRing` 则每轮重置）——不要把「戒指 + 已有技能」写成替换/去重删除，也不要在会话内加「卸下即删火球」。
5. **`DeleteNameSkill` 大小写敏感、`FindMagic(name)` 忽略大小写**：两侧口径不同，Rust 必须分别实现，不能共用一个比较器。

---

## 7. 判据草案（构造级；每条 ≥2 例 + 牙；tier 按 AGENTS.md）

> 约定：A 组 = 212 链；B 组 = 210 链；C = 物品面；D = 其他登录期改写。tier 判定：凡改动 `characters_magic/characters_item*` 落库或 52 数值者记 **R**（必须真机窗口）；纯输入→字节可判的片记 **L**（等价证明 + 1 条抽样冒烟）。判不准按 R。

### A. 非本职业技能删除 + 212（tier **R**）
- A1：warrior L1，记录 `magic=[{MagIdx:1,Level:0}]`，无戒指 ⇒ 帧序含 `212(recog=1,series=1)` 且位于 658 与 51 之间；不得发 211（删后为空）；登出后 `characters_magic` 无 MagicId=1。
- A2：taoist，记录 `magic=[{MagIdx:2}]` ⇒ `212(recog=2)`；再以 wizard + `[{MagIdx:2}]` ⇒ 同样 `212(recog=2)`（治愈同样受职业门）。
- A3（反向）：wizard + `[{MagIdx:1}]`、taoist + `[{MagIdx:2}]` ⇒ **无 212**；211 含该条且 `Count=1`。
- A4（只删首条）：warrior + 两条 `[1,1]` 无戒指 ⇒ 本次登录只发 1 条 212，211 含 1 条；再触发一次 `RecalcAbilitys`（如登录后一次装备变更探针）才删第二条。
- A5（名字比较口径）：记录 `MagIdx=1`（模板名 `火球术`）而 `Server.conf FireBallSkill=火球术` ⇒ 删；把配置改成 `火球術` ⇒ 不删（C# `==` 区分大小写）；把模板名改大小写（`FireBall`）而配置 `fireball` ⇒ Del **不**命中、但 Add 会命中（两个比较口径分别钉住）。
- A6（模板缺失）：记录 `MagicId=9999`（表无）⇒ 无 212、无 211 条目、存档删行。
- 牙1：把 `DelItemSkill` 改成 `job != Wizard` 恒删 ⇒ A3 红。
- 牙2：把 `break` 改成删全部 ⇒ A4 的 211 Count 红。
- 牙3：把 `series` 写成 0 ⇒ A1 字节/帧头红；把 recog 写成 0 ⇒ A2 红。
- 牙4：把名字比较改成忽略大小写 ⇒ A5 红。

### B. 戒指 210 / 212（tier **R**）
- B1：warrior + Ringl `{Index=火球戒指(Shape115),Dura>0}`、记录无火球 ⇒ 追加 `{MagIdx:1,Level:1,Key:0,TranPoint:0}`，发 `210(recog=0,series=1)`，211 含该条；登出后 DB 多一行 MagicId=1。
- B2：同 B1 但记录已有 `MagIdx=1`（Level=3）⇒ **不**发 210、不覆盖等级；211 保持原 Level=3。
- B3：taoist + Ringr `{Shape116,Dura>0}` ⇒ 发 210 且 211 含 MagicId=2；wizard + Shape116 ⇒ 210（Add 不校验职业）。
- B4：warrior + `Shape115,Dura=0`（戒指持久归零）+ 记录有火球 ⇒ Dura==0 不置 `FlameRing` ⇒ 发 212 并删；`Dura>0` 同数据 ⇒ 不删。
- B5：warrior + 记录有火球 + 戒指 `Shape115,Dura>0` ⇒ 不删（Add 分支优先），211 含火球。
- B6（粘滞锁存，tier **R**）：warrior 登录时戴 115 戒指（火球在列表）；会话内卸下戒指后触发一次 `RecalcAbilitys` ⇒ 火球**仍在**、无 212（`FlameRing` 未重置）；同场景换成 116 戒指 + 治愈 ⇒ 卸下即删并 212（`RecoveryRing` 每轮重置）；再重登（无戒指）⇒ 火球才被删并 212。
- 牙1：把 `Dura==0` 前的 `continue` 挪到戒指判断之后 ⇒ B4 红。
- 牙2：Add 前不查 `IsTrainingSkill` ⇒ B2 的 211 Count/等级红。
- 牙3：戒指 Shape 常量写错（115↔114）⇒ B1 红、复活戒指用例不受影响（区分度）。
- 牙4：把 `flame_ring` 当每次重算（每次 `RecalcAbilitys` 直接覆盖 false）⇒ B6 前半红；把 `recovery_ring` 做成粘滞 ⇒ B6 中段红。
- 帧字节牙：`sm_addmagic` 尾若漏 `edcode` 或漏 `encode_client_magic` 的 76B `Def` ⇒ B1 与 oracle capture 字节比对红。

### C. 登录期物品面（tier **R**）
- C1：背包 `Index=9999`（模板缺失）⇒ 登录后 621 不受影响、存档该槽写空（`Index=0`），且不再出现在任何物品帧。
- C2：装备槽 Weapons 放 `StdMode=10`（衣服）`Index` 合法 ⇒ 被移入背包（`AddItemToBag`），621 不含 Weapons；背包满 46 时按 `Insert(0)` 语义（47 条，存档截断到 46）。牙：把 `CheckUserItems` 的 Weapons 白名单加 `10` ⇒ C2 红。
- C3：武器槽 `Index=9999` ⇒ `Index=0`（物品消失），621 不含该槽；牙：模板 null 时保留 Index ⇒ C3 红。
- C4：背包两件同名同 `MakeIndex` ⇒ 只删较早一件；同名不同 `MakeIndex` 或同 `MakeIndex` 不同名 ⇒ 两件都留。牙：去掉名字相等条件 ⇒ 后半例红。
- C5：`CheckUserItemPlace=false` ⇒ 非法槽位**不**搬移（配置门）。牙：无视配置恒检查 ⇒ C5 红。

### D. 其他登录期改写（tier **R**）
- D1：记录技能 Level=4 ⇒ 登录后 211 的 Level=0 且存档 Level=0；Level=3 ⇒ 保持。牙：去掉 `>=4` 归零 ⇒ D1 红。
- D2：`Abil.MaxExp`（记录故意写 1）⇒ 52 的 `MaxExp = GetLevelExp(Level)`（对照 `Exps.conf` 真表）；牙：沿用记录值 ⇒ D2 红。
- D3：新手角色（HomeMap 空 / Level 0）⇒ 四件物品入包后 52 的 `WAbil.Weight` = 四件重量和；牙：把 `add_newbie_items` 留在 recalc 之后 ⇒ D3 红（当前 Rust 即为此态）。
- D4（L，纯帧）：任意真实记录（20 槽含 `MagIdx=0`）⇒ 211 `Count` = 非零条数、尾只含非零条目；牙：把空槽重新纳入遍历 ⇒ D4 红。真机抽 1 例（有基本剑术）冒烟。

---

## 8. 残留物清单

- 本 lane 唯一产物：`E:/tmp/laneA-login-validation.md`（本文件）。
- 未创建 worktree/分支；`git status` 未改仓库；未启动任何服务/客户端；未读写 `E:/MirServer`；未产生临时库/日志。


---

## 评审员更正（2026-10-19，**以本节为准**）

- **A2 自相矛盾**：`DelItemSkill(2)`（治愈术）只在 `Job != Taoist` 时删（`PlayObject.cs:5116-5121`）
  ⇒ 「taoist + MagIdx=2 ⇒ 212」错误，应改为 warrior/wizard + MagIdx=2 发 212，taoist 作反向。
- **GM/脚本可达性错误**：`TrainingMagicCommand.cs:46-54` 与 `ExecutionProcessingSys.cs:2654-2664` 在 `Add` 后
  立即 `RecalcAbilitys()` ⇒ 同一会话内就删技能并发 212，技能不会留在 DB ⇒ **不能**作为登录期 212 的来源；
  登录期治愈删除的正常来源只剩 DB/崩溃/耐久 0 的治愈戒指载入。
- **写集不完整**：B6/A4/V10 的「戴上即触发」要求会话内 `RecalcAbilitys` 也走 210/212
  （`main.rs:5381 recalc_session_ability` 及各调用点、`useitem.rs:343`）；211 空槽修复在 `enter.rs:582-611`，均未列入。
- **C/D 组超出写集**：V5/V6/V7/V8/V11/V4 不在「仅补 212/210」范围，需另卡或扩写集。
- **A5 需加 115 戒指用例**（`AddItemSkill` 只在 `FlameRing` 真时调用）。
- 其余（Rust 缺整条 210/212 链、FlameRing 粘滞、RecoveryRing 每轮重置、211 空槽导致整帧跳过、
  新手物品与 recalc 的顺序、`Abil.MaxExp` 不重算）**支持**。
