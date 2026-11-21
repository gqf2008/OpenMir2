# ServerConf 批次实现记录：MG-44(32 圣言术) / MG-48(50 魔爆)

> 分支 `card/magic-serverconf-fixture`，base `master dd55ec554`（含 8/37+21 推挤批）。
> 规划卡：[`magic-serverconf-batch-plan.md`](magic-serverconf-batch-plan.md)（本文件只记实现与证据）。
> 不碰 `src/**` oracle 代码。

## 1. 两侧装置面（全部由旋钮/显式键门控）

| 侧 | 改动 | 门控 |
| --- | --- | --- |
| C# `MagicGolden` | `ServerConfFixture.LoadOrThrow`（十键 + 生成源 sha 钉死）；`MagicSpec.ServerConf` ⇒ `Fixture.server_conf`（仅非 null 落盘） | 只在 32/50/施毒分支装载 |
| C# `MagicGolden` | `MagicSpec.TargetMonster`（**1 基**序号；0 = 玩家目标）；`doX/doY`/`DoSpell` 走 `spellTarget`；三条件（`Race>=Animal`、`LifeAttrib==LA_UNDEAD`、`!SuperMan`）不满足即 `FixtureContractViolation` | `TargetMonster>0` |
| C# `MagicGolden` | `TargetMonster` 分支把目标怪 `Animal=true` | 仅 `TargetMonster>0` |
| C# `MagicGolden` | `FireballResult.TargetMonsterHpBefore/After` | `TargetMonster>0`（`WhenWritingNull`） |
| Rust 驱动 | 读 `Fixture.target_monster`（Null/缺键=0；非整数/负/越界/三条件不满足 ⇒ 契约 panic） | 同上 |
| Rust 驱动 | `Fixture.server_conf` 存在 ⇒ `apply_serverconf_fixture`（十键，缺键/非整数/≤0 具名 panic），落在**施法前** | 同上 |
| Rust 驱动 | 台账落 `TargetMonster` / `TargetMonsterHpBefore/After` / `ServerConf`（全部 null-omit） | 同上 |
| Rust 驱动 | `Fixture.caster_level != 30` 时覆写施法者等级（实体 + 地面会话） | `!=30`（既有 stem 全是 30） |
| Rust 驱动（`lib.rs::fireball_case_assert`） | `assert_key_presence_symmetric`（存在性**对称**，golden 漏键/台账多写都红）；`server_conf` 十键**绝对值** = 部署口径；键集 == `SERVER_CONF_KEYS` | 同上 |

**判据层注（QA 2026-11-21 更正）**：`assert_key_presence_symmetric`、十键绝对值、键集相等三项都实现在**驱动**（`lib.rs::fireball_case_assert`）；比较器 `magic_fireball_parity.rs` 对 `server_conf`/`ServerConf`/`TargetMonster` 的**代码引用为 0**（仅一处注释提及（`ServerConf 十键批次` 小节标题）；`real_ledger_verify_labels` 只比既有 `Sent`/`Rm`/`Rng` 面）。**勿把它们记到比较器名下**——判据在驱动，驱动不跑/台账缺字段时比较器不会代偿。

**为什么 `Animal=true` 必须在 `TargetMonster` 分支补**：`RecordingMonster` 直接 `new`，`BaseObject.Animal`
保持 ctor 缺省 `false`；生产的怪物由 `MonsterInfo` 装载时为 `true`，Rust 的 `place_monsters` 也恒写
`ai.animal = true`。不补的话 `AnimalObject.Struck` 的 `if (Animal) MeatQuality -= Random(300)` **少一抽**，
整条 RNG 流错位（实测 lv43 金标准 3 抽 vs Rust 3 抽但取值全错、即死结果相反）。

## 2. 32/50 的 case 设计

### 32 圣言术（`turnundead.json`，单文件三支）

- 目标 = `Fixture.target_monster=1` 的不死怪（`race=50`、`life_attrib=LA_UNDEAD`、`!SuperMan`、`hp=300`）。
- 施法者 `CasterLevel=60` ⇒ `:842 Random(2)+(Level-1) > targetLevel` 两抽都真（0+59>45、1+59>45），
  比较面**只由 `:844` 的 `Level < MagTurnUndeadLevel(44)`** 决定。
- 三支：`lv43`（43 过门）/`lv44`（44 不过门，严格小于）/`lv45`（45 不过门）。
- `lv43` 的 seed 由装置**按实际取数序**扫出（`Struck` 的 `Random(300)` → `Random(2)` → `Random(100)`），
  要求即死臂真的发生（当前 seed=2）；44/45 不进入 `:847`，seed 只需固定。
- 结果（golden）：

| 支 | seed | Rng | 怪物 HP | DrainPasses |
| --- | --- | --- | --- | --- |
| lv43 | 2 | `[231, 0, 16, 2]` | 300 → **0** | 2 |
| lv44 | 5 | `[101, 0]` | 300 → 300 | 1 |
| lv45 | 13 | `[155, 1]` | 300 → 300 | 1 |

即死臂多两抽（`:847 Random(100)` + 训练尾 `Random(3)+1`），44/45 少两次 —— **抽数差本身就是判据**。

### 50 魔爆（`mabmabe.json`，s1/s5/s13）

- **玩家目标**（oracle 只有 `Race == Play` 才发 STONE 中毒帧；套 `TargetMonster` 会丢该分支）。
- `CasterMcNoClamp=MakeWord(5,10)` ⇒ `GetAttackPower` 区间非退化；`StepDrainMs=100`（RM_DELAYMAGIC 600ms + 训练尾 1000ms）。
- 结果：target HP 100→95(s1)/98(s5)/100(s13 miss)，抽数 10/11/5，`DrainPasses` 4/4/1（实测） —— 三 seed 覆盖命中/边缘/未中。
- **配置值面**：四项 `MabMabe*` 部署值 == ctor 缺省（100/10/21/20），行为面**无法**区分"读了配置"与
  "吃了缺省"；本批的判据是**绝对值断言 + 生成源 sha 钉死**（驱动断 `server_conf` 十键 == 部署口径，
  装置断生成源整文件 sha）。
  **follow-up**：用非缺省配置值（部署副本）重出至少一支，以点亮"读配置 vs 吃缺省"的行为面——当前部署值等于缺省，
  s1/s5/s13 无法区分二者。

## 3. 旧 golden 零变化（`TargetMonster=0` 路径）

方法：把 `HEAD` 版生成器快照（`git archive HEAD -- .../MagicGolden`）放在同深度目录
`MagicGoldenBase/`，与新生成器对**同一批 9 个既有 stem**（fireball、fireball5、shield、firewind、
spacemove、amyounsul、groupamyounsul-shape2、snowwind、groupdeding）各生成一份，**只剔除墙钟字段
`delivery`/`due_ms_rel`** 后逐字段比较：

```
fireball True / fireball5 True / shield True / firewind True / spacemove True /
amyounsul True / groupamyounsul-shape2 True / snowwind True / groupdeding True
ALL_ZERO_CHANGE True
```

同时记录：`new==committed` 与 `base==committed` 逐 stem 同真同假（fireball、fireball5、shield、
snowwind、groupdeding 两侧都 ≠ 入库件；firewind/spacemove/amyounsul 两侧都 == 入库件）⇒
入库件的差异是**既有装置/世代漂移**（见 `openmir2-golden-fixture-drift.md`），不是本批引入。
`MagicGoldenBase/` 是临时对照物，验证后已删除。

**双向口径（QA 2026-11-21 补强）**：零变化判据采用 `new==committed` **且** `base==committed`（`base` = `dd55ec554` 生成器、`new` = `0e2eb1316` 生成器，均只剔 `delivery`/`due_ms_rel`）。只写「重出 == 入库件」是单向的：若新生成器有变化、而入库件恰好等于 base，也会通过。实测两支独立复现：
- `firewind`：`new==committed` / `base==committed` / `new==base` 均为 **True**（Rng 3/3/3、`DrainPasses` 2）；
- `amyounsul`：同上三项均为 **True**（Rng 3/4/4、`DrainPasses` 3）。

## 4. 牙

| 变异 | 期望 | 实得 |
| --- | --- | --- |
| `apply_serverconf_fixture` 无视 golden，`set_mag_turn_undead_level(50)` | lv43 绿、lv44/lv45 红 | lv43 rc=0；lv44 `目标怪物术后 HP 与金标准不符`；lv45 `施法段+交付段 RNG 抽数 != 金标准` |
| 台账不写 `ServerConf` | 存在性对称判红 | 驱动 `[mabmabe/s1] Fixture.server_conf/ServerConf 存在性不对称（golden=true 台账=false）` |

两次变异均在**本 worktree** 内完成，还原后 `sha256` 与变异前逐字节相等、`grep MUTATION` = 0，并复跑基线绿。

## 5. 门禁（最终代码）

| 命令 | 结果 |
| --- | --- |
| `cargo fmt --all -- --check` | 0 |
| `cargo clippy -p game-svr -p mir2-parity-tests --all-targets -- -D warnings` | 0（`parity_serverconf` 已 `#[cfg(test)]`：消费方全在测试构建） |
| `cargo test -p game-svr --lib` | 2191 passed / 0 failed / 133 ignored |
| `cargo test -p game-svr --test serverconf_parse_parity` | 2 passed |
| 驱动 `tests::turnundead_parity_{lv43,lv44,lv45}`、`tests::mabmabe_parity_{s1,s5,s13}` | 6/6 rc=0（`MIR2_LEDGER_RUN_ID=tu-mb-20261121-c`） |
| 比较器 `real_rust_ledger_{turnundead,mabmabe}` | 2/2 rc=0（同一 run_id） |

## 6. 夹具更名（随本批）

`tests/parity/fixtures/serverconf/missing_mab_mabe_hit_success_rate.conf` →
`missing_mab_mabe_hit_sucess_rate.conf`：键名口径是 **一个 c**（oracle ASCII 拼写 `Sucess`），
测试按 `missing_<KEY>.conf` 拼文件名，旧名会让 `synthetic_serverconf_parses_identically` 找不到夹具
（首跑实测红）。文件内容未改（仅 `git mv`）。

## 7. 已知边界

- `mag_tamming_*` 四键只进 golden/快照，**不落 `World`**（消费者是 `PetSlaveConfig`，`World` 不持有）；
  MG-34（收服批）必须接到读 `PetSlaveConfig` 的那一侧。
- 施毒四支仍只把 `LoadOrThrow` 当**校验副作用**用，不落 `Fixture.server_conf`（既有施毒 golden 不重生成）。
- Rust 驱动的 `caster_level` 覆写只在 `!=30` 时生效；这是对 `fireball_rig` 硬编码 30 的最小扩展点，
  若将来装置给出其它等级，两侧都读同一个 golden 字段。
