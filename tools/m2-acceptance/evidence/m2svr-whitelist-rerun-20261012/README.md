# M2 验收③ 52 复查（owner `f963c1b3`）——`52 SM_ABILITY` 能力层修复后的真机对账

- 卡：`openmir2-m2-gamesvr-host`（M2 验收①②③）
- 本片写集：`tools/m2-acceptance/**`（证据）、`docs/**`、`mir2-rs/bins/game-svr/**`（同批 N-1 片，分交）
- **未改**：`crates/**`、`src/**`（C# oracle）、`MirClient`、表结构/脚本语法/数据格式；**未 merge、未签 review**
- 栈：`start_rust_stack.ps1`（MySQL 3306 + **C# DBSrv/LoginSrv** 下盘 + **Rust** `game-svr`(5000)/三网关）
- 二进制：`E:\tmp\openmir2-m2svr-target\debug`（当前分支 head，含 m3bag 的 `52` 修复）
- 采集：影子 `game-gate`(7600) + `tools/capture/mir2_proxy.py --map 7200=127.0.0.1:7600`；真客户端 `D:\MirClient-run`，账号 `mir2test` / 角色 `aaa`

## 1. 结论

| 项 | 结果 |
| --- | --- |
| `s2c_diff.py --start-ident 658 --upto-ident 46 --whitelist tools/m2-acceptance/whitelist.jsonl` | **GREEN：同序 + 等长 + 逐字节 diff = 0**（归一化仅 `recog`） |
| 15/15 ident 同序 | `658,51,50,41,213,11029,42,766,54,5008,1106,52,752,811,46` |
| 未登记差异 | **0** |
| 白名单命中 | 仅 `46.param` / `46.raw_hex`（夜间 `Bright=3` vs 金标准白天 `1`，owner §25.3 已登记） |
| `52` 能力层 | **修复生效**：`max_hp`/`max_mp`/`dc` 已与金标准逐字段相同（m3bag `3434f219`+`511d575b`，随 master 合入） |

命令（在本仓根运行）：

```powershell
python tools/m2-acceptance/s2c_diff.py `
  --rust tools/m2-acceptance/evidence/m2svr-whitelist-rerun-20261012/capture.jsonl `
  --golden mir2-rs/tests/golden/c-line-baseline-c3-20261010.jsonl `
  --start-ident 658 --upto-ident 46 `
  --whitelist tools/m2-acceptance/whitelist.jsonl `
  --out tools/m2-acceptance/evidence/m2svr-whitelist-rerun-20261012/s2c-diff.json
# GREEN: 同序列逐字节 diff = 0（归一化仅 recog）⇒ exit 0
```

## 2. `52` 的逐字段变化（`field-diff-52.txt`）

旧的 5 处差异（`dc=0 / max_hp=0 / max_mp=0` 是真缺陷；`hp/weight` 是夹具记录差）在 m3bag 修复后：

| 阶段 | `dc` | `hp` | `max_hp` | `max_mp` | `weight` |
| --- | --- | --- | --- | --- | --- |
| 金标准 | 257 | 15 | 19 | 15 | 14 |
| 修复前（01:28 旧采） | 0 | 19 | 0 | 0 | 0 |
| 修复后（夹具未对齐） | 257 | 19 | 19 | 15 | 0 |
| 修复后（夹具对齐，本片） | 257 | 15 | 19 | 15 | 14 |

**根因（逐条到 oracle 行号）**：

- `max_hp/max_mp/dc`：oracle 登录链 `AbilCopyToWAbil()` → `RecalcLevelAbilitys()` → `RecalcAbilitys()`
  （`WorldServer.cs:351` / `PlayObject.cs:1308`），`DBSrv.GetAbilGetRecord` **不读** `MaxHP/MAxMP`
  ⇒ 记录里为 0，必须由等级派生补上。Rust 现在按同序补（m3bag 修复），已与金标准相同。
- `hp`：`WAbil.HP = wOldHP`（`BaseObject.Abil.cs:21-24`，保留**调用前**的当前值）⇒ 线上值 = 角色记录
  `characters_ablity.Hp`。金标准角色记录 `Hp=15`；本机 `mir2test/aaa` 记录 `Hp=19`。
- `weight`：`WAbil.Weight = RecalcBagWeight()`（`BaseObject.cs:1356`；求和 `ItemList` 模板重量）
  ⇒ 金标准角色背包有 1 件模板重 14 的物品；本机 `aaa` 背包为空。

⇒ 后两项是**夹具记录状态**差异，不是 Rust 偏差（同 DB 下 oracle 也会得出 19/0）。

## 3. 夹具对齐（为把 golden↔Rust 对比控制在同一输入）

对账要求「同一操作序列」，因此把 `aaa` 的**可见帧字段所依赖的记录**对齐到金标准采集时的状态；
**收工已全部还原**（对齐项、起点值、还原值如下）：

| 项 | 对齐前（原始） | 对齐为（金标准） | oracle 出处 |
| --- | --- | --- | --- |
| `characters.CX/CY` | `291,666` | `287,618` | `SM_NEWMAP(51)` 的 `param/tag` |
| `characters.Dir` | `4` | `0` | `SM_LOGON(50)` 的 `series = MakeWord(Dir, Light)` |
| `characters_ablity.Hp` | `19` | `15` | `SM_ABILITY(52)` 尾 `WAbil.HP` |
| `characters_bagitem`（Position 0 槽，id=275771） | `StdIndex=0` | `StdIndex=15`（金创药(中)包，`stditems.Weight=14`）/`MakeIndex=15`/`Dura=DuraMax=100` | `SM_ABILITY(52)` 尾 `WAbil.Weight`（`RecalcBagWeight`） |

> 对齐只影响恰好读这两项的字段（`WAbil.HP`/`WAbil.Weight`），不改判据、不外扩白名单。
> 未对齐时的证据同目录保留：`s2c-diff-fixture-unaligned.{json,txt}`（RED：2 处 `hp/weight`）。

## 4. 复现（真机）

```powershell
# 1) 起栈（Rust 链；本片账号已存在，未新建/provision）
powershell -File tools/m2-acceptance/start_rust_stack.ps1 -RustBinDir E:\tmp\openmir2-m2svr-target\debug
# 2) 影子网关(7600) + 代理(7200→7600)（先停标准 game-gate 让出 7200）
#    game-gate.exe E:\tmp\m2-acceptance\stack\rungate-proxy.conf
#    python tools/capture/mir2_proxy.py --out <dump> --map 7200=127.0.0.1:7600
# 3) 真客户端进图（对齐夹具后）
powershell -File tools/client/mir_flow.ps1 -Account mir2test -Password mir2pass `
  -OutDir E:\tmp\m2acc\run5b\shots -SkipTalk -SkipReenter -WalkClicks 0
# 4) 导出 + 对账（上一节的命令）
powershell -File tools/m2-acceptance/export_s2c.ps1 -ProxyDir <dump> -OutDir <out>
# 5) 收工停栈
powershell -File tools/m2-acceptance/start_rust_stack.ps1 -Stop
```

## 5. 失败怎么判红

- 未登记差异 ≠ 0 ⇒ `s2c_diff.py` `exit 1`；缺 capture/金标准 ⇒ `exit 2`（PREREQ）。
- 白名单条目缺 `reason`/`reviewer` ⇒ 脚本 `exit 1`（不静默放行）。
- `52` 的 `max_hp/max_mp` 再变回 0（= 修复被回退）⇒ 本片 `s2c-diff.json` 的 `pass` 必为 false。

## 6. 残留物清单

- 本目录（证据，入库）：`capture.jsonl`（Rust 侧 15 条入图帧导出）、`s2c-diff.{json,txt}`（GREEN）、
  `s2c-diff-fixture-unaligned.{json,txt}`（未对齐对照 RED）、`field-diff-52.txt`、本 README。
- 仓库外（保留作原始件，不入库）：`E:\tmp\m2acc\run5\**`、`E:\tmp\m2acc\run5b\**`（proxy dump / 导出 / 截图 / 栈日志）。
- DB：`mir2test/aaa` 的 `CX/CY/Dir`、`characters_ablity.Hp`、`characters_bagitem(id=275771)` 已按 §3 还原。
- 端口：收工 `-Stop`，仅共享 MySQL `3306` 在听；未改 `E:\MirServer` 配置，未起客户端残留。
