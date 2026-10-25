# 窗口夹具：声明式怪物/尸体前置（2026-10-25）

- 卡：`openmir2-window-fixture-corpse`（tier **L**，P1；owner `openmir2-tester-1`，
  worktree `.worktrees/tester-1`，branch `card/window-fixture-corpse`）。
- 目标：给 `butch_two_draws`（W-DG-2）补「单只固定动物」前置，使它可从 `blocked` 解封。
- 红线：未改生产代码/oracle；未 push；起栈仅为私有栈验证。

## 1. 事实订正（卡片前提与 oracle 实现不一致）

卡面写「起栈前在测试库写入对应怪物/动物记录…使其一进游戏就在该格且处于可屠宰状态」。核对 oracle 后：

- `monsters` 表只是**怪物定义**（`GameSrv/DB/MySqlDB.cs:168-171` `select * from monsters`），
  不是实例；`characters` 等表也没有尸体/怪物实例行。
- 尸体是**运行时对象**：`ClientGetButchItem` 要求目标 `baseObject.Death && !Skeleton && Animal`
  （`src/M2Server/Player/PlayObject.Operate.cs:702-722`），且同图、Chebyshev ≤ 2、过速度门。
- ⇒ **DB 预置尸体不可行**。可行的声明式前置是：起栈前给**本侧私有 Envir** 的 `MonGen.txt`
  前插一条**单点刷怪**（`range=0`），让动物一进图就站在指定格；**可屠宰状态仍需场景先把它打死**
  （场景侧改动，属 `butch_two_draws` 的下一次修订，不在本卡写集）。

## 2. 接口（profile 声明，未知字段 fail-closed）

```json
"sides": {
  "csharp": {
    "envir_root": "E:/tmp/conformance/csharp-root/M2GameSvr",
    "fixtures": {"monsters": [
      {"map": "0", "x": 330, "y": 331, "name": "鸡", "count": 1}
    ]}
  },
  "rust": { "...": "同形" }
}
```

| 字段 | 规则 |
|---|---|
| `fixtures` | 非空对象；**未知字段 ⇒ ConfigError（rc=2）**；当前只支持 `monsters` |
| `monsters[]` | 非空数组；每项只允许 `map/x/y/name/count`，**未知字段 ⇒ rc=2** |
| `map` | 非空字符串；必须出现在**本侧** `Envir/MapInfo.txt`（否则 rc=2 点名） |
| `x`/`y` | 0..65535 整数（bool 拒绝） |
| `name` | 非空、**可编码 GBK**（否则 rc=2） |
| `count` | 1..100 整数；缺省 1 |

写入行形如 `map\tx\ty\tname\t0\tcount\t360`（= MonGen 的 `地图 x y 名称 范围 数量 ZenTime`；
`范围=0` 即单点）。**第 7 列 `ZenTime` 必须 > 0**（本夹具固定 `FIXTURE_MONGEN_ZEN_TIME = 360`）：
两端解析器都是 `ZenTime = 列值*60*1000` 后 `ZenTime <= 0 ⇒ 丢弃`（C# `LocalDB.cs:555-558`；
Rust `crates/envir/src/mongen.rs:147-150`），且**加载期无日志** ⇒ 写 0 会「夹具看似应用、怪根本不刷」。
首刷不受该值影响：`WorldServer.MonGen.cs:228` 在 `StartTick == 0` 时立即首刷。

### 2.5 2026-10-25 P0 订正：必须**前插**（append 在窗口内永远轮不到）

卡 `openmir2-fixture-monsters-prepend`（tier=L，P0）。dev-1 第一轮 C# 实测（`butch_two_draws`）：
MonGen 加载 **39020 行**、装备成功（`SM_TAKEON_OK 615`）、20 次 `CM_HIT` 被接受
（`SM_HIT(14)×11`），但目标格**没有任何活动帧**、无 `SM_STRUCK`、无死亡帧 ⇒
`kill: no target frames`——**不是伤害问题，是怪根本没刷**。

机制（双侧同构，已复核）：

- C# `src/GameSrv/Word/WorldServer.MonGen.cs:205-230`：闸门
  `(GetTickCount() - RegenMonstersTick) > Config.RegenMonstersTime`（`GameSvrConf.cs:1433 = 200ms`）
  成立时，只取 `mongenList[CurrMonGenIdx]` **一条**，随后 `CurrMonGenIdx++`（到末尾回 0）。
- Rust `mir2-rs/crates/world/src/mongen.rs:723-768`：同构（取当前下标那条，再推进；越界取 0）。

⇒ **追加**到单表 **3081 行**末尾：扫一轮 ≈ **616s（≈10.3 分钟）**，远长于 kill 窗口的几十秒，
**窗口内永远轮不到**（部署未配 `ProcessMonsterMultiThreadLimit`，缺省 **1**，
`GameSvrConf.cs:1896`，用在 `WorldServer.cs:1609/1790`）；**前插**到文件头 = index 0，
第一个闸门（约 200ms、`StartTick == 0`）就取到。

口径注记（2026-10-25 实测）：仓库内私有副本是 **3081 行**（`csharp-root` 与影子同为 3081 行 /
126314 B / sha `8ad1826a88ad713d…`；`rust-root` 3080 行 / sha `e5c39fb7eda66b70…`）。
dev-1 那轮 **GameSvr 日志**报「MonGen 加载 **39020 行**」——那是他那轮的日志口径
（≈130 分钟），不是部署树行数；**两种口径下 append 都轮不到，结论不变。**

实现与判据（`tools/conformance/card_window.py`）：

- `FIXTURE_MONGEN_LAYOUT = "prepend"`；写入 `block + 原文件字节`，**原 39020 行字节序整体后移、一个字节不改**；
- 写后立即用 `_mongen_fixture_layout(raw_before, raw_after, lines)` **机判**：
  头块必须是夹具行；若形态是「块在末尾」⇒ 直接报 **append 布局**（rc=2，并写清 200ms/条、130 分钟）；
  原内容字节不一致也 rc=2；
- restore 口径不变：备份原文件字节 → 停栈后写回 → **重读磁盘比对 sha256 等于写前值**；
- 牙（自测里做反向验证，不提交反向实现）：构造 append 布局 ⇒ 机判必须红（`MERGE 牙1`）。

**副作用面清点**（所有用 `fixtures.monsters` 的卡/场景）：

| 使用方 | 影响 |
|---|---|
| `openmir2-window-fixture-corpse`（本夹具原始卡） | 语义不变（单点刷怪）；「写后位置」从末尾改到头部，证据里 `layout=prepend`、`action=prepended` |
| `openmir2-butch-two-draws-unblock`（dev-1，首个真机用户） | **修好**：怪会在首个刷怪闸门出现，kill 链才有 target frames |
| 后续 drop / combat 类窗口卡 | 同上：凡依赖「夹具怪真的刷出来」的都受益；**没有**任何场景依赖「末尾追加不刷出」（已 grep：仓库内仅文档示例，无 profile/场景断言） |
| `castle_merchant_tax*`（taxleg） | 声明里 `monsters: null`，不受影响 |

- **残留风险与硬断言（review F1，reviewer-20 与 reviewer-21 独立复现）**：`restore-baseline*.sh`
  **只重置 DB、不覆盖 Envir** ⇒ 夹具一旦失败留下半改的 `MonGen.txt`，基线/影子可能被**同样写脏**，
  而 `envir_shadow_symmetry` 只比三方一致 ⇒ **脏得一样反而会绿（自证清白）**。因此本夹具把
  「**先登记 target（带 `.bak`）再写**、失败按备份回滚、失败后文件字节 == 写前」做成硬断言
  （selftest `MERGE 牙6` / `MERGE 牙7` 把写盘变异成 append 复验）；冒烟收尾逐 target 核
  `before_sha256 == restore 后 sha256`，不只比三方相等。
- **RNG 台账**：前插会让夹具怪更早进入刷怪随机流（比 append 更早消耗 RNG）；台账体积策略见
runbook §0.8（默认 64MiB / 20 万行，超限跑后裁剪并记录原始 size/行数/sha256）。

## 3. 生命周期与证据

1. **应用时机**：`run_side` 在 assets 之后、端口检查与起栈之前（`profile_fixtures()` →
   `run_fixtures()`）；写前保存 `before_sha256` 与原文件到
   `<out>/<side>/fixtures/MonGen.txt.{baseline,shadow}.bak`，写后记 `after_sha256`、`layout=prepend`
   与前插行清单（`fixtures.json` 的 `targets[]`）。
   **G-1（review）应用目标**：`--rng-log` 时 C# GameSvr 从 `M2GameSvr-hook/`（影子）跑，
   而 profile 的 `envir_root` 指基线 ⇒ 夹具**在影子存在时必须同时写基线与影子**，两侧内容
   逐字节相同（否则①对 oracle 不生效、②下一轮 `envir_shadow_symmetry` 必红）。
2. **还原**：`run_side` 的 `finally` 在停栈之后调用 `restore_fixtures()`，把备份写回并**校验
   sha256 等于写前值**；失败写 `restore_error`（不改 rc，但证据可见）。
3. **dry-run**：只打印 `[side] 怪物夹具（dry-run 不写 Envir，布局=prepend）：… ⇤ <行>`，不落盘。
4. **失败面（全部 rc=2，fail-closed）**：未知字段、空数组、缺字段、坐标越界、count 越界、
   非 GBK 名、MapInfo 不存在/解析不到、MonGen.txt 缺失/不可写。

实测：

```text
$ python -I -B tools/conformance/card_window.py --selftest
PASS FIX1 合法声明（map/x/y/name，count 缺省 1）=> 解析透传
PASS FIX1b 未声明 fixtures => None（不使用夹具的场景行为不变）
PASS FIX 牙1 未知字段/空数组/缺字段/越界/count 0/非 GBK => ConfigError 点名
PASS FIX2 写 Envir/MonGen.txt：单点刷怪行、备份存在、rc=0（**2026-10-25 起为前插**）
PASS FIX2b 夹具行第 7 列 ZenTime>0（360 分钟 => 21600000 ms）——按两端解析器口径（zen_time<=0 即丢弃且无日志）不会被静默丢行
PASS FIX3 停栈后还原：sha256 与写前一致（重读磁盘后比对）
PASS FIX2c G-1：影子存在时同一夹具同时写基线与影子（两侧内容逐字节相同）
PASS FIX3c G-1：基线与影子都按 sha256 还原（重读磁盘）
PASS FIX 牙2 dry-run：只打印预览、不写文件
PASS FIX 牙3 地图不在本侧 MapInfo => rc=2 点名（fail-closed）
PASS FIX4 接线：run_side 起栈前应用夹具、finally 还原
GREEN: card_window.py selftest 303/303   # 2026-10-25 夹具卡当时的快照
```

2026-10-25 P0 订正后的新增自测（同一 `--selftest`，review F1/F2 修复后全套 396/396（rebase 到 `a60e04ad9` 时 393/393）；其中含 inventory 卡的 INV 16 条）：

```text
PASS MERGE1 前插布局 => 机判通过（夹具行在头，原内容字节后移）
PASS MERGE 牙1 追加布局 => 机判红（点名 append：200ms/条，窗口内轮不到）
PASS MERGE 牙2 插在文件中间 => 机判红
PASS MERGE 牙3 前插但原内容被改一个字节 => 机判红
PASS MERGE 牙4 什么都没写 => 机判红
PASS MERGE2 端到端：MonGen.txt 头部即夹具行、tail 为原 39020 行字节、action=prepended
PASS MERGE3 restore 逐字节复原（sha256 与写前一致）
PASS MERGE4 未超限 => 不裁剪，但仍记录 size/行数/sha256
PASS MERGE5 超限 => 只留前 3 行 + 原始 size/行数/sha256；head 仍过账本校验
PASS MERGE 牙5 台账不存在 => 记 error、不抛异常
PASS MERGE6 接线：停栈之后才裁剪台账（进程退出前台账仍在增长）
GREEN: card_window.py selftest 396/396
```

真实文件演练（`E:/tmp/conformance/rust-root/M2GameSvr/Envir/MonGen.txt`，不 no 起栈）：

```text
# 旧版（append）日志，仅存档；2026-10-25 起为前插
[rust] 怪物夹具（起栈前写 Envir/MonGen.txt）：追加 1 行；地图=0；sha256 e5c39fb7eda6 -> 5effe4954158；备份=…/MonGen.txt.bak
[rust]   + 0	330	331	鸡	0	1	360
rc=0 action=appended；restore_err=None；还原后 sha256 == 写前（True）

# 当前口径（prepend）
[rust] 怪物夹具（起栈前**前插** baseline/Envir/MonGen.txt）：1 行位于文件头；sha256 <before> -> <after>；备份=…
rc=0 action=prepended；restore_err=None；还原后 sha256 == 写前（True）
```

## 4. `butch_two_draws` 解封步骤（四件事，缺一不可）

1. **本卡夹具**：给两侧 profile 声明 `fixtures.monsters`（同一只动物、同一格）。
2. **场景侧击杀**：`butch_two_draws` 需先攻击该动物至死亡（并把它所在格填进 `target_xy`），
   再发 `CM_BUTCH`；该改动属场景卡（dev-2 写集），**本卡不改场景 JSON**。
3. **RNG 同种子**：C# hook 接线已合 `15dbbe843`（`tools/window/start-csharp.ps1`），窗口用
   `--rng-log` 即可两侧同 seed + `MIR2_RNG_SITE=1`；证据见 `docs/window-csharp-rng-20261025.md`。
4. **两处解除 blocked**（见 `openmir2-blocked-two-sources-doc`）：`butch_two_draws.json` 顶层
   `blocked` 与 `run3-scenarios.json` 的 #21 行 `blocked` **同时删除**，否则批次排除与
   `card_window` 起栈前 BLOCKED 只解除一半。

> F3（review info）：`butch_two_draws.json` 的 `blocked.reason` 仍写「无尸体夹具」，属**场景侧写集**，
> 随场景修订（含 run3 行两处同时删 blocked）一起改；本卡不改场景 JSON。

> 注意：MonGen 单点刷怪在**每侧各自的私有 Envir**（`csharp-root` / `rust-root`）副本上生效；
> `E:/MirServer` 冻结基线一字未动。若将来私有 Envir 被外部清扫（`E:/tmp` 已知问题），夹具会
> 在起栈前 rc=2 点名缺 `MapInfo.txt`/`MonGen.txt`，不会静默跑空。

## 5. 门禁

`card_window --selftest` 396/396（2026-10-25 P0 订正 + review F1/F2 修复后；本条历史值 300/300）；7 项卫生门禁（`stale_pointer --ref master`）+
`git diff --check <base> <head>` + `merge_tree_helper` 值见 patch body。未起正式服务端；未 push。

## 6. 残留

- 真实文件演练的备份副本：`E:/tmp/scriptcov/fixture-side/fixtures/MonGen.txt.bak`（可删）。
- 私有 Envir 的 `MonGen.txt` 已按 sha256 还原（演练输出 `restored True`）。
