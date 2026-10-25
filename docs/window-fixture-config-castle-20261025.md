# 窗口夹具：配置覆盖通道 + 城堡前置（`openmir2-window-fixture-castle`，tier L）

- 卡：`openmir2-window-fixture-castle`（owner `openmir2-dev-1`，branch `card/window-fixture-castle`，base `e3ac7c121`）。
- 写集：`tools/conformance/card_window.py`、本文件。
- **与怪物夹具卡的合流**：master `08f2e2fd5`（卡 `openmir2-window-fixture-corpse`）已先一步把
  `profile.sides.<side>.fixtures.monsters` 做进同一文件 ⇒ 本卡把 schema **合并**为
  `fixtures = {monsters, config_overrides, castle_list, castle_files, note}`（同一通道、同一还原生命周期），
  而不是另起一套；怪物夹具的语义/校验/ZenTime 口径原样保留。
- **与 tester 影子门卡（G-1）的合流**：master `92682d3fb` 新增 `_fixture_envir_roots(sc)`（基线 +
  `M2GameSvr-hook` 影子）与 `fixtures.json.targets[]`。本卡**直接复用**该函数给
  `config_overrides`/`castle_list`/`castle_files` 选 target（不再另造一套），文件与怪物的
  target 统一进 `targets[]`（`target=baseline|shadow`）。
- 来源：castle 税场景卡（`openmir2-window-scenes-castle-tax`，master `e0ba42f66`）的 F3 真缺口——
  `card_window` 在 `restore → 起栈` 之间没有**文件**通道（只有 DB 侧 placement/inventory/assets/magic），
  W-TAX-2 需要的 `CastleGetAllNpcTax=1` 没法声明。
- 红线遵守：**不碰 `E:/MirServer`**（代码内硬守卫 + 加载期校验）、不改冻结内容、不 push、不起服务端。

## 1. 一句话

`card_window` 新增**声明式窗口夹具** `profile.fixtures`（或 `sides.<side>.fixtures` 覆盖本侧）：
① `config_overrides` 改私有根 `Server.conf`/`Server1.conf` 的白名单键；② `castle_list` 写
`Castle/List.txt`；③ `castle_files` 写 `Castle/<n>/SabukW.txt` 的 `[Setup]` 键。
三者都：**写前备份（sha256）→ 原子写 → 写后读回 → 停栈后按 sha256 还原**；`--dry-run` 只打印 diff；
未知文件/键、越界值、坏城名一律**加载期 fail-closed**。

## 2. 用法

### 2.1 声明（profile JSON）

```json
{
  "format": "card-window-profile/1",
  "fixtures": {
    "config_overrides": {
      "Server.conf": {"CastleGetAllNpcTax": 1, "CastleTaxRate": 5}
    },
    "castle_list": ["0"],
    "castle_files": {
      "0": {"OwnGuild": "", "TotalGold": 0, "TodayIncome": 0}
    },
    "note": "W-TAX-2：两侧同值开 CastleGetAllNpcTax；城堡初值归零"
  },
  "sides": { "csharp": { "...": "..." }, "rust": { "...": "..." } }
}
```

- **顶层 `fixtures` = 两侧同值**；`sides.<side>.fixtures` **整体覆盖**本侧（不做深合并——免得
  两侧声明混在一起看不出谁生效）。W-TAX-2 要求两侧同值 ⇒ 用顶层写。
- `monsters`（卡 `openmir2-window-fixture-corpse`）与文件三件套可**同时声明**：MonGen 追加与配置/城堡
  写入各用各的备份，停栈后一起还原（action=`prepended+applied`）。
- 名字注意：`card_window` 里已有一个 `apply_overrides(prof, cfg)`，它是**profile/CLI 字段**覆盖
  （account/char_name/timeout 等），与本卡的**文件**覆盖无关；所以本通道叫 `fixtures`，不叫 `overrides`。

### 2.2 白名单（fail-closed 面）

| 通道 | 允许 | 拒绝（ConfigError） |
|---|---|---|
| `config_overrides` 文件 | `Server.conf` / `Server1.conf` | 其它文件名 |
| `config_overrides` 键 | `CastleGetAllNpcTax`(0..1)、`CastleTaxRate`(0..100)、`CastleOneDayGold`、`CastleGoldMax`、`UpgradeWeaponPrice`、`ConsoleShowUserCountTime`（后四者 0..int32） | 未知键、bool/浮点/字符串、越界 |
| `castle_list` | 非空数组、每项规范十进制城名 `0..99`、无重复、≤ 8 座；**两侧必须一致**（含两侧都不声明）——加载期 `_check_fixture_symmetry` 核对 | 空数组、`"00"`、重复、`"x"`、>8、**两侧不同/一侧声明另一侧不声明** |
| `castle_files` 键 | `OwnGuild`(str，GBK ≤32 字节、无换行)、`NextWar`/`TotalGold`/`TodayIncome`(int ≥0) | 未知键、超长/含换行/不可 GBK、负数 |
| `castle_files` 城名 | 必须出现在同一 `fixtures.castle_list` 里（若声明了 `castle_list`） | 未列出的城 |
| `monsters` | 非空数组；每项 `map/x/y/name[/count]`（语义与校验见 `docs/window-fixture-corpse-20261025.md`） | 未知字段、缺字段、越界、count 0、非 GBK |
| 顶层 | `monsters` / `config_overrides` / `castle_list` / `castle_files` / `note`；实体键至少其一 | 未知键、只有 note |
| **两侧一致** | `config_overrides` / `castle_list` / `castle_files` 三者两侧必须完全相同（含两侧都不声明）；`monsters` 不在此列 | 任一侧不同 ⇒ 加载期 `ConfigError`（对拍无效/门失效，不沉默） |

其它 fail-closed：目标文件**必须已存在**（不创建）、目标键在文件里**恰好出现 1 次**且值后无空白/注释
（否则拒绝而不是猜）、`envir_root` 落在 `E:/MirServer` 下直接拒绝。

### 2.3 执行序与证据

```text
restore（DB）→ placement → inventory → assets → magic
  → fixtures.apply   （对**每个根**：主根 + `M2GameSvr-hook` 影子根（存在时）
                       写前 sha256 + 备份 → 原子写 → 写后读回校验；<side>/fixtures.json）
  → 端口预检 → 起栈 → discovery → probe → DB 快照
finally:
  stop（栈）→ fixtures.restore（按备份写回 + sha256 == 写前；备份删掉、记录 restore_sha256）
```

- `fixtures.json.targets[]`（与怪物夹具同形状，卡 `openmir2-window-preflight-envir-shadow-symmetry` 定口径）：
  怪物项 `{target,path,backup,before_sha256,after_sha256}`；文件项再加 `rel/changed/diff`。
  还原后每 target 加 `restore_sha256` 并把 `backup` 置空；**还原不完整 ⇒ 该侧 rc=2**（带原因），不假绿。
- `roots[]` 同时落证据：`[基线]` 或 `[基线, M2GameSvr-hook]`（`--rng-log` 下 oracle 从影子跑）。
- **为什么影子根也要写**：`tools/window/start-csharp.ps1` 在 `-RngLog`（`card_window --rng-log`）模式下
  让 GameSvr 从 `<Root>/M2GameSvr-hook` 跑（影子只在缺 `GameSvr.exe` 时 robocopy 一次）⇒ 只写基线
  **对本次 oracle 不生效**，还会让影子与基线失配（下一轮影子对称门必红）⇒ 两个根写同一内容、各自还原。
- 为什么还原必须在**停栈后**：服务端运行时会写 `Castle/<n>/SabukW.txt`（10 分钟存档门），栈没停就还原会被覆盖。
- **部分失败也要回滚**：`_apply_files` 逐文件写，第 N 个写/读回失败时**就地**把前 N-1 个按备份还原
  （外层 except 只能看到已返回的记录）——`restore-baseline*.sh` 只管 DB，配置文件留在被改状态会污染下一轮。
- `restore-baseline*.sh` 只回滚 DB，不覆盖配置文件 ⇒ 文件夹具完全由本通道负责还原（这是它能安全存在的前提）。

### 2.4 `--dry-run`

只做离线校验并打印 planned diff（`[side] 夹具 Server.conf: - CastleGetAllNpcTax=0` / `+ …=1`），
**不写文件、不建备份目录**；任何前置不成立（文件缺失/键重复/生产根）⇒ 预览 rc=2，与真跑同等级。

## 3. 与 `castle_list_symmetry` 门的关系（**必读**：门在写前、夹具在写后）

- 门在 `tools/window/preflight_private_stack.py`（master `9c17e8eab`）：要求两侧私有栈（含 hook 影子
  `M2GameSvr-hook/`）的 `Castle/List.txt` **内容一致**，否则 preflight FAIL。
- **时序陷阱（本轮实测）**：门是**开窗前独立一步**，看的是夹具写盘**之前**的文件；而夹具在
  `run_side` 里 restore 之后、起栈之前才按侧写。⇒ 只要允许两侧声明**不同**的 `castle_list`，
  门永远是绿的、夹具再把两侧写成不同城集合——**门形同虚设**。任何「门 + 夹具」组合都要先问：
  门检查的是写前还是写后？
- **本卡的口径（二选一里选了①+②都做）**：
  1. **加载期声明对称（fail-closed）**：`load_profile` 对两侧各解析一次 `fixtures`，若
     **`config_overrides` / `castle_list` / `castle_files` 三件套**任一不同（含一侧声明、
     另一侧不声明）⇒ `ConfigError` 点名两侧值（`_check_fixture_symmetry`）。推荐把三件套
     只写在**顶层**（两侧同值）；`sides.<side>.fixtures` 若要用，必须把同一份三件套一起带上。
     `monsters` 不在此列（按侧独立声明是它的既有语义）。
  2. **运行时写两侧同一内容**：夹具按「主根 + 影子根（若存在）」两份都写、各自备份、
     各自按 sha256 还原（见 §2.3）。
- **没有**把 preflight 的 `castle_list_symmetry` 挪进 `card_window`（reviewer-20 的另一建议）：
  那要改 tester 的 preflight 或把门复制进本工具，写集/职责都越界；本卡用「加载期声明对称 +
  两侧同写」达到同一防护，门继续守文件现状。

## 4. W-TAX-2 / W-TAX-3 怎么用（以及仍不能做什么）

### 4.1 W-TAX-2（`CastleGetAllNpcTax=1` 分支）现在具备条件

```json
{"fixtures": {"config_overrides": {"Server.conf": {"CastleGetAllNpcTax": 1}},
              "castle_list": ["0"], "castle_files": {"0": {"TotalGold": 0, "TodayIncome": 0}}}}
```

- 两侧同值 ⇒ `CastleMgr.IncRateGold` 的**升序遍历**在两侧看到同一城集合（前置：两侧 `Castle/List.txt` 已一致；
  当前 `csharp-root` = 1 城、`rust-root` = 2 城的历史不对称**需先由 tester 对齐**，本夹具只保证「本次运行写什么」，
  不替测试库/部署做永久性修正）。
- 升级腿本身还要求 **map 3 铁匠的第二个 placement**（单场景一个 placement）⇒ 仍需另开场景卡，本卡不含。

### 4.1b 判据口径：**别只看文件**（600s 存档门会按内存重写 `SabukW.txt`）

`SystemProcessor.ShowOnline` 每 `ConsoleShowUserCountTime=600000`ms 调 `CastleMgr.Save()`，它会用
**内存里的城堡状态**重写 `Castle/<n>/SabukW.txt` 的 `[Setup]`。⇒ 在 ≥600s 的场景（如
`castle_merchant_tax_potion` 的 620s 窗口）里，**启动时没被加载进内存的键**（未知键/额外键/格式差异）
会在存档点被静默改回或丢弃。用法口径：

- 判据优先读**帧 + DB 快照**；`SabukW.txt` 只作辅助（`TotalGold/TodayIncome/OwnGuild` 这些**被加载**的键可信）；
- 需要校验「夹具确实生效」时，读 `run.json`/`fixtures.json` 的 before/after sha256 与运行期首个
  `SM_*` 帧，而不是存档后的文件内容；
- 能用短窗口（< 600s）覆盖的判据不要硬拖过存档点。

### 4.2 城堡归属：`OwnGuild` 不等于「城主」

- `UserCastle.LoadConfig`（`UserCastle.cs:228-232`）只做 `MasterGuild = GuildMgr.FindGuild(OwnGuild)`。
  **`OwnGuild` 写了名字但 DB 里没有行会/成员行 ⇒ `MasterGuild == null`**：`$LORD` 显示「管理员」、
  `IsMasterGuild` 恒 false——这正是「非城主仍收税」的观测态，不是「城主」态。
- 要造真城主需：`guild` 表建行会 + `guildmember` 里把测试角色设为掌门（DB 事务 + 两侧同值），
  且 `GuildMgr` 的加载时机在起栈时 ⇒ 本卡**不做**（如实标 blocked，见 §5）。

### 4.3 W-TAX-3 前半（`CastleMerchant=1` 但不在攻城区 ⇒ 零税）

冻结内容里**没有**这种 NPC（0151–0155 走 `EnvirList`；map 3 的 `沙巴克/小贩` 在 `WarRange` 内），
本卡不改变该结论——只能靠构造级合同测试。

### 4.4 W-TAX-4（`SaveTick` 10 分钟门）

仍 blocked：`SaveTick` 不落盘、C# 无 `MIR2_CLOCK_*`。本卡**不**提供时钟注入。

## 5. 仍 blocked 的项与解封条件（如实登记）

| 项 | 为什么本卡不解 | 解封条件 |
|---|---|---|
| 真城主（`MasterGuild` != null）夹具 | 需要 DB 事务（guild/guildmember）+ 起栈前加载顺序；本卡只写文件 | 新卡：`guild`/`guildmember` 声明式夹具（写前指纹、两库同值、`GuildMgr` 加载后核 `MasterGuild`） |
| 升级腿（第二 placement，map 3） | `card_window` 单场景单 placement | 新场景卡或 kit 扩展多段 placement |
| W-TAX-4 同 tick | C# 无时钟注入 | ⑧-1 的 C# `MIR2_CLOCK_*` 片 |
| 两侧 `Castle/List.txt` 历史不对称 | 属部署/夹具现状（preflight 已守卫） | tester 对齐两侧私有根（或用本夹具在顶层声明同值并保持） |

## 6. 测试与牙

- `python tools/conformance/card_window.py --selftest` ⇒ **349/349 GREEN**（含怪物夹具卡的 `FIX1..FIX4` 与影子门卡的 `FIX2c/FIX3c`）；本卡新增 `FX1..FX49`（去掉被 master 影子用例覆盖的组合）+ 1 牙，共 46 条：
  - 声明校验：未知文件/键、越界、坏城名（`"00"`）、重复、空列表、castle_files 城不在 list、OwnGuild 超 32 字节/含换行、只有 note、顶层未知键；
  - dry-run：不写文件、不建备份、diff 行精确；
  - apply：只改目标行（CRLF/其余字节逐字节不变）、写前/写后 sha256、SabukW 的 GBK 值、List.txt 同集合 → changed=False；
  - restore：**期间被第三方改写**后仍按备份还原到写前字节、备份清理、`restore_sha256` 记录；
  - 生产根守卫、目标文件缺失、键出现 2 次；
  - 源码级接线：`run_side` 在起栈前应用、`finally` 停栈后还原、`load_profile` 加载期校验；
  - 合并语义：`monsters` + 文件三件套同一份声明（action=`prepended+applied`），两类备份互不干扰、双双还原；
  - 合并语义（FX46/FX47）：`monsters` + 文件三件套同一份声明（action=`prepended+applied`），`targets[]` 同时含怪物与文件，双双还原；
  - 影子根（FX43–FX45）：基线 + `M2GameSvr-hook` 写同一内容、都进 `targets[]`、都回到写前字节；dry-run 两个根都不落盘；
  - execute 级（dry-run）：`summary.fixtures` 有该侧证据、文件未被改写。
- 既有怪物夹具自测唯一改动：`FIX1` 的断言从「整对象相等」收敛到「`decl["monsters"]` 相等 + 文件侧为空」
  （合并 schema 后归一化结果多了三个空集字段），语义不变。
- 外部牙（实跑变异 → 红 → 还原，本卡实测条数）：① 关掉还原写入 ⇒ **11 条红**（FIX3c/FX9/FX10/FX11/FX44…）；
  ② 去掉生产根守卫 ⇒ FX27 红（1 条）；③ dry-run 分支也写文件 ⇒ **8 条红**（FX2 起）；
  ④ 静默接受未知配置键 ⇒ FX14 + 牙 红（2 条）；⑤ `load_profile` 不调对称性检查 ⇒ FX41 红（1 条）；
  ⑥ 对称性检查直接放行 ⇒ FX36/FX38/FX40/FX48/FX49 红（5 条）；⑦ 不写影子根 ⇒ FIX2c/FX43 红（3 条）；
  ⑧ 关掉就地回滚 ⇒ FX42 红（2 条）。还原后 349/349 且字节与变异前一致。

## 7. 门禁

`cargo` 面不涉（纯 Python 工具）；卫生四门（conflict/stale `--ref master`/dynamic `--ref master`/board）+
`git diff --check <base> <head>` + `merge_tree_helper` 见 patch body。**未改** `E:/MirServer`、未起栈、未连库。
