# castle-war T-131/T-132：MasterGuild 装载相位 + CastleNameColors 配置源（L 片）— 2026-10-25

- 卡：`openmir2-castle-war-config-slice`（tier **L**；owner `openmir2-dev-1`；worktree `.worktrees/dev-1`；分支 `card/castle-war-config-slice`）。
- 基线：本地 `master` `c703f6a03`（patch base；承接 `openmir2-castle-war-area-impl` 的 L 片）。
- 写集：`crates/world/src/{world.rs,castle.rs 注释}`、`bins/game-svr/src/{config.rs,castle.rs,lib.rs,castle/tests.rs}`、本文件。
- 红线：未触 RNG/时序面；未改 C# `src/**`；未起栈、未占端口、未连库。

## 0. 一句话

T-131 用**合同测试钉住启动相位**（`MasterGuild` 在启动装载时恒 `None`，`is_master_guild_name` 不得用
`OwnGuild` 名字顶替）；T-132 把 `Server.conf [Setup]` 的**四个名字色**真正读进 `GateConfig` 并灌进
`World`，`query_user_name_color` 不再硬编码 `CastleNameColors::default()`。两处各配牙（实测 3/3 红后还原绿）。

## 1. T-131：`MasterGuild` 装载相位

### 1.1 oracle 证据（行号）

- `UserCastle.LoadConfig`（`src/M2Server/Castle/UserCastle.cs:229-232`）：
  `MasterGuild = SystemShare.GuildMgr.FindGuild(OwnGuild);`
- `UserCastle.GetCastle`（`:543-556`）：占领时 `MasterGuild = Guild; OwnGuild = Guild.GuildName;`。
- `IsMasterGuild`（`:639-642`）：`MasterGuild != null && MasterGuild == guild`。
- 装载顺序：`CastleMgr.Initialize()`（`src/GameSrv/GameApp.cs:213`）**早于**
  `GuildMgr.LoadGuildInfo()`（`:245`）/`CastleMgr.LoadCastleList()`（`:246`）⇒ 初始化时 `FindGuild` 恒
  `null` ⇒ 启动态 `MasterGuild == None`；只有 `GetCastle`（真实占领）才会挂上。

### 1.2 本仓处置

- `UserCastle::initialize`（`crates/world/src/castle.rs:1207+`）**不解析** `master_guild`（保持 `None`），
  只搬 `OwnGuild` 名字；`is_master_guild_name`（`:1658-1670`）在 `master_guild == None` 时**恒 false**，
  不拿 `OwnGuild` 顶替城主身份。`GetCastle` 生产未接线 ⇒ 生产恒 `None`（与 oracle 启动态一致）。
- **装载序差异（登记，不在本 L 片改）**：本宿主 guild 装载（`lib.rs` G3 启动块，约 `:20192`）在
  城堡初始化（约 `:20533`）**之前**，而 oracle 是 castle init（213）在前。因为 `initialize` 从不解析
  `MasterGuild`，**相位结果相同**（`None`）；重排启动序会移动 `world.rng` 的消费位置（城堡造怪抽数）
  ⇒ 属 RNG/时序面，按卡面红线**不在此片改**。已在 `lib.rs` 城堡调用点加相位注释。
- 合同测试（新增，`bins/game-svr/src/castle/tests.rs`）：
  `initialize_leaves_master_guild_none_phase_contract` —— 生产 `initialize_castle` 后
  `own_guild == Some("沙巴克")`、`master_guild == None`、`is_master_guild_name(Some("沙巴克")) == false`。
- 既有 `crates/world/src/castle.rs::master_guild_name_requires_master_guild_loaded` 继续覆盖
  「已挂 `MasterGuild` 后才判城主」的语义。

## 2. T-132：`CastleNameColors` 配置源

### 2.1 oracle 证据（行号）

- `ServerConf.cs:509-512`：`[Setup] PKFlagNameColor / AllyAndGuildNameColor / WarGuildNameColor /
  InFreePKAreaNameColor` 四个 `ReadWriteByte`。
- 缺省：`GameSvrConf.cs:1731-1736` = `0x2F`(47) / `0xB4`(180) / `0x45`(69) / `0xDD`(221)；
  部署 `E:/MirServer/M2GameSvr/Server.conf:740-743` 四值相同。
- 消费：`PlayObject.GetChrColor`（`PlayObject.cs:3588-3656`，`PKFlagNameColor` 在 `:3595`）。
- `ReadWriteByte`（`ConfigFile.cs:45-53`）：缺键 ⇒ 写回缺省并返回缺省；键在 ⇒ `Read<T>` →
  `Convert.ChangeType`（非法值**抛**）；空串 ⇒ 缺省。

### 2.2 本仓改动

| 文件 | 改动 |
|---|---|
| `bins/game-svr/src/config.rs` | `GateConfig` 新增 `pk_flag_name_color` / `ally_and_guild_name_color` / `war_guild_name_color` / `in_free_pk_area_name_color`；`[Setup]` 解析四键（缺键 ⇒ 47/180/69/221；非法 ⇒ fail-closed）；新增 `GateConfig::castle_name_colors()`（**唯一映射点**）。 |
| `crates/world/src/world.rs` | `World` 新增 `castle_name_colors` 字段（缺省 `CastleNameColors::default()`）+ `set_castle_name_colors`；合成 golden 不调 ⇒ 逐字节不变。 |
| `bins/game-svr/src/lib.rs` | `EnterConfig` 携带四个名字色（生产从 `cfg.castle_name_colors()`，`for_test` 用缺省）；`sim_loop` 起世界时 `world.set_castle_name_colors(...)`。 |
| `bins/game-svr/src/castle.rs` | `query_user_name_color` 由 `CastleNameColors::default()` 改为 `world.castle_name_colors`（配置覆盖生效）。 |

- **登记偏差（不落盘）**：oracle `ReadWriteByte` 在缺键时会把缺省**写回** `Server.conf`；本仓配置层只读
  （内容冻结/不改部署件），部署件四键都在 ⇒ 当前不可观测。

## 3. 合同测试 + 牙（实测 3/3）

| 测试 | 钉什么 |
|---|---|
| `config::tests::parses_setup_castle_name_colors_with_oracle_defaults` | 缺键 ⇒ 47/180/69/221；写了 ⇒ 逐字段读；非法（`300`/`abc`）⇒ `load` 报错（fail-closed）；`castle_name_colors()` 映射 |
| `castle::tests::initialize_leaves_master_guild_none_phase_contract` | 启动相位 `master_guild == None`、名字不得顶替城主身份 |
| `castle::tests::query_user_name_color_reads_world_castle_name_colors` | **F3**：配置源→World→消费者全链——缺省 World 取 47、`set_castle_name_colors(pk_flag=200)` 取 200 |

| 变异 | 结果 |
|---|---|
| T-131：去掉 `is_master_guild_name` 的 `master_guild.is_none()` 相位门 | 相位合同测试 **FAILED** → 还原 ok |
| T-132：`castle_name_colors()` 的 ally/war 两字段对调 | 配置测试 **FAILED** → 还原 ok |
| T-132：`PKFlagNameColor` 缺省 `0x2F → 0x30` | 配置测试 **FAILED** → 还原 ok |
| **F3**：`query_user_name_color` 改回 `CastleNameColors::default()`（原始状态） | 全链测试 **FAILED**（缺省 47 仍绿、200 红）→ 还原 ok |

## 4. 门禁

fmt / clippy / `cargo test --workspace` / msgcodegen 全 rc=0；7 项卫生门禁全 rc=0；
`git diff --check` rc=0；`git merge-tree` 无冲突——review head `372af4c5a` vs 当时 master `d36532a7c` = `02b511d0…`、vs base `c703f6a03` = 分支 tree `8625051d…`（返工后 head 的 vs_base / vs_master 由 `tools/hygiene/merge_tree_helper.py` 生成，值见 patch body）。未改 C# `src/**`。

## 5. 判不准 / 残余

- 启动序（guild load 在 castle init 之前）与 oracle 相反，但相位结果等价；重排属 RNG/时序面，
  不在本 L 片（已在 `lib.rs` 注释与本节登记）。
- `ReadWriteByte` 的缺键写回副作用未建模（部署件四键齐全，不可观测）。
- T-128 的 N1/盟友关系（`GuildId` 映射）仍按原登记；本片只接配置源，不改变关系判定。
