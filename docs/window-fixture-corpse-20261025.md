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
  追加一条**单点刷怪**（`range=0`），让动物一进图就站在指定格；**可屠宰状态仍需场景先把它打死**
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

## 3. 生命周期与证据

1. **应用时机**：`run_side` 在 assets 之后、端口检查与起栈之前（`profile_fixtures()` →
   `run_fixtures()`）；写前保存 `before_sha256` 与原文件到
   `<out>/<side>/fixtures/MonGen.txt.{baseline,shadow}.bak`，写后记 `after_sha256` 与追加行清单
   （`fixtures.json` 的 `targets[]`）。
   **G-1（review）应用目标**：`--rng-log` 时 C# GameSvr 从 `M2GameSvr-hook/`（影子）跑，
   而 profile 的 `envir_root` 指基线 ⇒ 夹具**在影子存在时必须同时写基线与影子**，两侧内容
   逐字节相同（否则①对 oracle 不生效、②下一轮 `envir_shadow_symmetry` 必红）。
2. **还原**：`run_side` 的 `finally` 在停栈之后调用 `restore_fixtures()`，把备份写回并**校验
   sha256 等于写前值**；失败写 `restore_error`（不改 rc，但证据可见）。
3. **dry-run**：只打印 `[side] 怪物夹具（dry-run 不写 Envir）：…`，不落盘。
4. **失败面（全部 rc=2，fail-closed）**：未知字段、空数组、缺字段、坐标越界、count 越界、
   非 GBK 名、MapInfo 不存在/解析不到、MonGen.txt 缺失/不可写。

实测：

```text
$ python -I -B tools/conformance/card_window.py --selftest
PASS FIX1 合法声明（map/x/y/name，count 缺省 1）=> 解析透传
PASS FIX1b 未声明 fixtures => None（不使用夹具的场景行为不变）
PASS FIX 牙1 未知字段/空数组/缺字段/越界/count 0/非 GBK => ConfigError 点名
PASS FIX2 写 Envir/MonGen.txt：追加单点刷怪行、备份存在、rc=0
PASS FIX2b 夹具行第 7 列 ZenTime>0（360 分钟 => 21600000 ms）——按两端解析器口径（zen_time<=0 即丢弃且无日志）不会被静默丢行
PASS FIX3 停栈后还原：sha256 与写前一致（重读磁盘后比对）
PASS FIX2c G-1：影子存在时同一夹具同时写基线与影子（两侧内容逐字节相同）
PASS FIX3c G-1：基线与影子都按 sha256 还原（重读磁盘）
PASS FIX 牙2 dry-run：只打印预览、不写文件
PASS FIX 牙3 地图不在本侧 MapInfo => rc=2 点名（fail-closed）
PASS FIX4 接线：run_side 起栈前应用夹具、finally 还原
GREEN: card_window.py selftest 303/303
```

真实文件演练（`E:/tmp/conformance/rust-root/M2GameSvr/Envir/MonGen.txt`，不 no 起栈）：

```text
[rust] 怪物夹具（起栈前写 Envir/MonGen.txt）：追加 1 行；地图=0；sha256 e5c39fb7eda6 -> 5effe4954158；备份=…/MonGen.txt.bak
[rust]   + 0	330	331	鸡	0	1	360
rc=0 action=appended；restore_err=None；还原后 sha256 == 写前（True）
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

`card_window --selftest` 300/300；7 项卫生门禁（`stale_pointer --ref master`）+
`git diff --check <base> <head>` + `merge_tree_helper` 值见 patch body。未起正式服务端；未 push。

## 6. 残留

- 真实文件演练的备份副本：`E:/tmp/scriptcov/fixture-side/fixtures/MonGen.txt.bak`（可删）。
- 私有 Envir 的 `MonGen.txt` 已按 sha256 还原（演练输出 `restored True`）。
