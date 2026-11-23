# M1b 第 5b 步：B 组接线设计（hit / spell / say 非 GM，2026-11-23）

> 状态：**只读设计稿（v2，已并入 `5b` 评审的 3 洞 / 2 牙 / 4 证据缺口与 Leader 裁决）**。
> 基线 `f7dd73857`（第 1–4 步已落地）。本片目标：把 B 组在**区 k** 从「分类为 `Local` 但处置
> 丢弃」变成「与区 0 同一实现」。**排期**：`0c` 的 M1c-1 正在改 `SharedServices`/`sim_loop`，
> 本片同样触 `lib.rs` 核心区 ⇒ **等 M1c-1 合并后从新基开工**。
>
> 评审记录：`5b` 评审 v1 抓出 ①`ChatHost` 无法在区 k 构造（洞1）②`magic_table` 其实已在
> `enter_cfg`（洞2，Leader 据此**撤销 A-1a 并记在自己账上**）③区 k 写 `saves` 无落库路径（洞3）；
> 两颗会红的牙（`zone_k_non_move_command_is_counted_and_ignored` 拿 `CM_SAY` 当非移动命令、
> `at_predicate_is_shared_between_drain_gate_and_classifier` 的共享判据）；四处证据缺口（Q4）。
> 本 v2 全部按裁决改。

## 1. 范围与放行条件

区 k 的处置唯一入口 = `handle_zone_command_on`（`handle_zone_command(` 只剩测试调用点）。
本片只加三处**与区 0 同一实现**的分派（不另写处理逻辑、不复刻）：

```text
handle_zone_command_on(...):
    if movewire::is_move_command(ident)        { handle_move_cmd(...) }              // 现状
    else if attackwire::is_hit_ident(ident)    { handle_hit_cmd(...) }               // B1
    else if magicwire::is_spell_ident(ident)   { handle_spell_cmd_with_magic(...) }  // B2
    else if ident == chat::CM_SAY              { handle_plain_or_whisper(...) }      // B3（仅 plain + `/`）
    else { note_zone_cmd_unsupported(ident) }
```

**范围外**：`!` 频道族**本片不接**（分类改 `Unsupported`，见 §6）；`@` 已是 `Process`（A6 第 4 步）。

## 2. 状态可得性（区 k 有什么、缺什么）

区 k 的 `HostLoop`：`world` / `displays` / `online` / `ground_sessions` / `saves` / `gates`；
`SharedServices`（区 k 副本）：`registry` / `enter_cfg` / `colors` / `shard_runtime` / `zone_host`。

### 2.1 hit（B1，建议第一）

`handle_hit_cmd(ident, recog, tag, chr_name, socket, &online, &mut world, &mut displays,
&mut ground_sessions, &gates, &enter_cfg.catalog)` —— **全部区 k 可得**。

- 区 0 支的 `state_log.record(&world, &displays, registry.as_ref())` 是纯诊断 dump（不出帧）：
  区 k **跳过并登记**为诊断面差异。

### 2.2 spell（B2，建议第二）—— **全部区 k 可得（A-1a 已撤销）**

`handle_spell_cmd_with_magic(...)` 的实参来源：

| 实参 | 区 k 来源 | 可得？ |
| --- | --- | --- |
| `online/world/saves/ground_sessions/displays/gates` | 区 k `HostLoop` | ✅ |
| `catalog` / `pet_cfg` / `magic_attack_rage` / 三个 AoE 半径 / `table` / `magic_cfg` / `colors` / `strings` | 区 k `SharedServices.enter_cfg` + `colors` | ✅ |
| `registry` | 区 k `SharedServices.registry` | ✅ |
| **`magic_table`** | **`&enter_cfg.magic_table`**（`EnterConfig` 的字段；区 0 调用点即用此；`ProcessServices.magic_table_arc` 只服务 PROCESS 支其他调用点） | ✅ |

- **A-1a 撤销**：**不**给 `SharedServices` 加 `Arc<MagicTable>`；不存在"因 `SharedServices` 5→7
  而必须等"的排期理由。剩的排期理由只有一条：与本片同触 `lib.rs` 核心区、与 M1c-1 冲突 ⇒ 等新基。
- 魔法等级/修炼值写 `saves`（区 k 落地区自己的存档表）——**但落库路径有洞（洞3，见 §4 D-3）**。

### 2.3 say 非 GM（B3，建议第三）：抽共享函数，**不**给 `ChatHost` 塞假进程态

**洞1**：`ChatHost` 的结构体要求 `guilds`/`party_cfg`/`filter_word`/`mission`/`spirit_mutiny_tick`
等进程态 ⇒ 区 k **连构造都做不到**。**设计（采纳 5b 建议）**：

- 抽 **`fn handle_plain_or_whisper(online: &OnlineRegistry, world: &World, gates: &Gates,
  colors: &MsgColors, speaker: &OnlineEntry, line: &str) -> bool`**（`handle_whisper` 的形参
  已经够；plain 支只用 `ref_msg_cache::targets` + `online` + `gates` + `colors`）。
- `handle_say_with_saves` 的普通/`/` 支**改为调用这一份**；`@`/`!` 分支留在
  `handle_say_with_saves`（进程态就地可用）。
- 区 k 的 `CM_SAY` 分派调用同一 `handle_plain_or_whisper`（speaker = 区 k `online` 的条目）。
- **禁止**用占位/假 `guilds/party_cfg/...` 构造 `ChatHost`。

子支状态表：

| 子支 | 判据 | 需要状态 | 本片 |
| --- | --- | --- | --- |
| 普通说话 | 非 `/`、非 `!`、非 `@` | `colors` + `online/world/gates` + `ref_msg_cache`（**按世界持有**） | ✅ 本地接 |
| `/` 私聊 | `chat::parse_whisper` | `online/world/gates` + `colors` | ✅ 本地接 |
| `!` 频道（`!!`/`!~`/`!`） | 首字符 `!` | `GuildManager`（进程态）等 | ❌ **本片不接**（归 `Unsupported`，见 §6） |
| `@` GM | `say_payload_is_at_command` | 区 0 全量 | 已是 `Process`（A6） |

## 3. 零行为证据（每族同一套；**派发牙必须配可观测副作用**）

1. **N=1 两树**：关 `MIR2_ZONES` ⇒ 区 0 路径不变 ⇒ 两树逐字节 `frames.ndjson` 一致（`a7` 跑）。
2. **全量测试**：基线 **4725** + 本片新增单测数；每族一条派发级牙 + 一条守恒牙（未接线族仍计
   `zone_cmd_unsupported`）。
3. **派发牙的可观测副作用（Q4-⑤，不许只断言"不再计 unsupported"）**：
   - **hit**：断言 `gates` 收到自身明文 `+GD/<tick>`（`send_good_tick` 直发；`world` 的命令队列私有
     不可读，故用直发帧做可观测副作用）；
   - **spell**：断言 `saves["hero"].record.data.magic` 被写（训练/等级）**或** `gates` 收到
     `SM_MAGICFIRE(638)`/`SM_SPELL(17)`（至少一条）；
   - **say**：断言 `gates` 收到 `SM_HEAR(40)`（body 非空）。
   （这些牙的变异：把分派分支改成"直接 return" ⇒ 副作用断言必须红。）
4. **两区面**（等对拍窗口；本片 hit/say **双方同区**）：
   - hit：面1/面2（帧型/序）+ 面3（self 受击桶）；数值遮罩见 §4 D-2。
   - spell：面2（`SM_MAGICFIRE(638)` → `SM_SPELL(17)` → `SM_STRUCK(31)` 帧序）+ 面3（蓝消耗）
     + **AoE multi-target**（目标集合=范围内实体，结构面判；伤害数值遮罩）+ **miss/dodge**
     （见 D-2 的确定性夹具口径）。
   - say：面1（`SM_HEAR(40)` body 字节：GBK `ChrName:Msg` + 颜色 param）+ 面3（500ms 缓存窗口）；
     **`/` 私聊**：面1（`SM_WHISPER(103)` body）+ **收件人 = 被私聊者一人**（结构性断言）。
5. **红检**（每族一条）：hit 改伤害/收件人 ⇒ 红；spell 把 638 挪到 17 之后 ⇒ 红；say 改 GBK
   编码/绕开缓存 ⇒ 红；whisper 收件人改广播 ⇒ 红。

## 4. 裁决与已知差异

### D-1（原 A-1a）**已撤销**
`EnterConfig.magic_table` 已在 `SharedServices.enter_cfg`（区 0 spell 调用点即 `&enter_cfg.magic_table`）；
`ProcessServices.magic_table_arc` 只服务 PROCESS 支其他调用点。⇒ **不加 `Arc<MagicTable>`、不改
`SharedServices`**。Leader 记录为误批（记在 Leader 账上）。

### D-2 RNG 数值遮罩（**显式、逐字段；不许悄悄放过**）

区 k world rng = `seed ⊕ zone`；oracle 单 world 一条流 ⇒ 抽随机的数值必然不同。

| 族 | 被遮罩字段 | 依赖依据（一句） |
| --- | --- | --- |
| hit | `SM_HEALTHSPELLCHANGED(53)` 的 **HP** 字段 | 伤害 = `group_melee_damage`：①`atk_power/get_attack_power(rng, base, range)` 抽攻击力区间 ②`rng.random_below(target.speed_point) >= attacker.hit_point ⇒ power=0`（**闪避也抽随机**）③`get_hit_struck_damage` 的伤害链 |
| hit | 派生死亡/经验/掉落链（帧是否出现/条数） | 同一次抽取决定"是否致死" ⇒ 结构面会变；**场景固定"必不致死"**（目标 HP > 最大可能伤害） |
| spell | 同 53 的 HP（目标）及 AoE 各目标的 HP | 魔法伤害走同一 `group_melee_damage`（`AttackPowerMode::Player`）链 |
| move / say / magic-key / query4 / levelrank / login-notice | 无 | 不抽随机（`+GD` 的 tick 由侧车归一，见规划 §5） |

**确定性夹具口径（替换"实施时须核"）**：
- **闪避支**：`speed_point` 是**派生量**（`ability::hit_speed_base`：
  `DEF_SPEED(15) + BonusAbil.Speed/除数 + Σ装备 Speed`，**无负数项 ⇒ 恒 ≥ 15**）⇒
  **`speed_point = 0` 不可达**。确定性条件 = **`attacker.hit_point ≥ target.speed_point`**
  （代码 `rng.random_below(speed) >= hit ⇒ power=0`；`hit ≥ speed` ⇒ 恒命中）。
  落地：目标不戴 Speed 装备、不加 `BonusAbil.Speed` ⇒ speed=15；攻方用
  `characters_bonusability.Hit = 240`（job 0/1：`5 + 240/20 = 17`）或 `Accurate ≥ 10` 的武器
  （注意 job 2 的道士 bonus 分支不加 hit）。
  **注意该确定性夹具下"闪避分支本身不被覆盖"**——它只证"不闪避路径"两侧一致，报告必须原样写
  这句，不得读成"闪避已验"；要覆盖闪避分支需另设可复现的随机源（当前未做，列为未覆盖）。
- **必伤害/致死支**：目标 `characters_ablity.AC = 0` ⇒ 护甲随机区间 `[0,0]`、伤害 = power；
  攻方 `DC_min ≥ 1` 保 `power > 0`；`目标 HP > 攻方 DC 上界` ⇒ 必不致死（结构面可比）。
  已知坑：法师 1 级 `DC = 0-1`（`DC_min = 0`）⇒ 不可用；战士 ≥5 级才有 `DC_min ≥ 1`。
- **AoE**：目标集合与逐目标帧数按结构面判（不遮罩）；每个目标的 HP 数值遮罩。
- **遮罩掉的是数值不是整帧**：hit 仍判帧类（ident 集合）、条数、收件人集合、非数值字段
  （recog/tag/series/方向/文本）、以及「数值字段存在且非零」的结构性断言。
- **遮罩表进报告与缺口清单**（Leader 立偏离项）；hit 的死亡/经验/掉落链在报告写明
  **"有意未覆盖"**（Q4-③）。

### D-3（洞3）**区 k 写 `saves` 无落库路径**
`M1B_ZONE_PROD` 不含 `PERSIST`；区 k 只有断线时的 `drain_zone_saves`（`save_outbox` → 区 0 DB 链）。
⇒ spell 的魔法等级/修炼值（以及后续 magic-key 的 `Key`）在区 k 只落内存，**直到断线才存**。
要求：
- **登记为已知差异**（报告写明：oracle 是周期 `SaveHumanRcd`，我们是断线保存）；
- **补一条窗口判据**：spell 改等级 → **断线** → 重登 ⇒ 值保持（证明 `drain_zone_saves` 路径覆盖
  该写）；在线期间不做"落库时点"判据。

### D-4（原 A-3）跨区观察者/听者投递缺口
`SM_HIT/SM_STRUCK/SM_HEAR` 走 direct online 查找、⑤ 七面无对应回退 ⇒ 边界 12 格内跨区收件人少帧
（光环 12 == 听觉 12 ⇒ 可见性够、**投递**缺口）。裁决：本片 hit/say 两区跑**双方同区**，报告写明
缺口未覆盖；Leader 立卡 **A18**（关门判据 = 跨区 12 格内观察者收帧与同区一致）。

## 5. 排期与顺序

1. **hit** → 2. **spell** → 3. **say（plain + `/`）**；每族一个独立提交，便于逐族回退。
2. 三族都**等 M1c-1 合并后从新基开工**（同触 `lib.rs` 核心区；`SharedServices` 5→7 已是新基）。
3. `!` 频道族**单开一片**：要么给区 k 可得的 guild 状态（A9 类问题），要么走**不阻塞本地命令**的
   独立控制通道并显式登记序偏离。卡由 Leader 立；本片报告写明「频道族未接线、无证据」（Q4-①）。

## 6. 分类器与牙的爆炸半径（Q2：六处必须一起改）

即使本片不接 `!`，分类规则里 `!` 要归 **`Unsupported`**（不是 `Local`，否则会被本地执行）：

1. **`zone_cmd_class` 的 `CM_SAY` 三态**：`@` 起 ⇒ `Process`；`!` 起 ⇒ **`Unsupported`**；其余 ⇒
   `Local`。
2. **共享判据**：新增 `say_payload_is_channel(line)`（严格 `starts_with('!')`，与 `@` 判据同款），
   分类器**只调共享判据**（不再出现裸 `starts_with`）。
3. **牙（六处）**：
   - `at_predicate_is_shared_between_drain_gate_and_classifier`：扩成"分类器走共享 process 判据、
     判据内部复用 at 判据、两处都无裸 `starts_with`（`@` 与 `!` 都不许裸写）"；
   - `zone_k_non_move_command_is_counted_and_ignored`：**把 `CM_SAY` 换成真正未接线的 ident**
     （如 `CM_QUERYBAGITEMS`）——plain say 接线后原牙必红；
   - `zone_cmd_class_cm_say_payload_exception_is_strict_at`：补 `!x`/`!!x`/`!~x`/` !x` 四态
     （`!` ⇒ Unsupported；` !x` 前导空格 ⇒ Local）；
   - `zone_cmd_class_m1b_families_are_local`：补 `!` 例外（`CM_SAY` + `!…` **不**在 Local 族）；
   - 唯一性牙改名/扩三态（`payload_exception_is_unique_to_cm_say` → 内容改为"payload 例外只此
     **一族**，族内三态"）；
   - 分类器 doc 块与 `say_payload_is_at_command` 的 doc 重写（"唯一例外"→"唯一例外族、三态"）。
4. **文档同步**：`docs/issues/l25-m1b-plan-20261123.md` §2.1「唯一 payload 例外」措辞随牙一起改。

## 7. 证据缺口清单（Q4，报告逐条写）

| # | 缺口 | 处置 |
| --- | --- | --- |
| ① | `!` 频道族无面 | 本片不接：报告写明「频道族未接线、无证据」；Leader 立卡 |
| ② | `/` 私聊面 | 本片补：`SM_WHISPER(103)` body + 收件人 = 被私聊者一人 |
| ③ | hit 死亡/经验/掉落链 | RNG 派生结构面：报告写明**有意未覆盖**（场景固定必不致死） |
| ④ | spell AoE 多目标 / miss-dodge | 已给判据：AoE 目标集合=结构面、伤害遮罩；闪避用 `speed_point=0` 确定性夹具，否则列未覆盖 |
| ⑤ | 派发牙的副作用 | 每族配可观测副作用（hit 攻击队列非空 / spell saves 写或帧出 / say gates 收 `SM_HEAR`） |

## 8. 裁决记录（2026-11-23，Leader opc-27，经 `5b` 评审修订）

| 编号 | 裁决 | 备注 |
| --- | --- | --- |
| 洞1 | 抽 `handle_plain_or_whisper` 共享；不给 `ChatHost` 塞假进程态 | 本稿 §2.3 |
| 洞2 / 原 A-1a | **撤销 A-1a**：`magic_table` 取自 `enter_cfg`，不加 `SharedServices` 字段 | Leader 记为误批 |
| 洞3 | 区 k saves 无落库路径：登记差异 + 断线重登窗口判据 | 本稿 D-3 |
| Q3 / 原 A-1b | **改判：本片不接 `!`**（A6 会让每条频道消息暂缓该玩家本地命令 ≤1 tick、可撑爆 deferred/挤占邮箱；oracle 聊天不阻塞移动）；`!` 归 `Unsupported`，单开一片 | 本稿 §5.3、§6 |
| Q2 | 六处分类器/牙/文档一起改 | 本稿 §6 |
| D-2 | 数值遮罩逐字段 + 确定性夹具；hit 仍判结构面 | 本稿 D-2 |
| D-4 / A-3 | 本片双方同区 + 立卡 **A18** | 本稿 D-4 |

**状态**：设计稿只读、未提交（`card/m1-0e` 上未跟踪文件）；规划 `8e252b7cd` 已提交、待与 M1c-1
一起推。工作树停手等新基。
