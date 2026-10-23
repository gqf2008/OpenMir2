# 协议无头对拍：场景 DSL

本目录的 JSON 是 `tools/probe/mir2_probe.py`（无头客户端驱动器）的输入。
驱动器负责把同一串 `CM_*` 打进 Rust 栈与 C# oracle 栈，并把 s2c 帧逐帧落成证据；
**比较器是另一条车道**（比 `frames.ndjson` 与数据库后态），本目录不含判定逻辑。

```sh
python tools/probe/mir2_probe.py \
  --login-gate 127.0.0.1:7000 --account test01 --password test01 \
  --server-name 热血传奇 --char-name test01 \
  --scenario tools/probe/scenarios/gm_time.json \
  --out E:/tmp/probe-rust-gm-time
```

## 文件形状

```json
{
  "name": "唯一名字",
  "description": "给人看的一句话（可选）",
  "params": {"npc_actor": 119},
  "steps": [ ... ]
}
```

* 顶层只允许 `name` / `description` / `params` / `required` / `computed` / `placement` / `steps`；多写一个键即 **fail-closed**（退出码 2）。
  （`computed` 是给 `param_discovery` 的**声明性注记**，probe 不读它；见 `move_walk.json`。）
* `required`（可选）是**必填变量名列表**：`expand_scenario` 在合并完 `--var` 后逐名检查，缺任何一个即
  `scenario-error`（退出码 2），消息点名缺哪个变量。运行期才知道的值（NPC/商人 `ActorId`、`MakeIndex`、
  物品名、自身坐标）一律走这里，**不要**在 `params` 里放 `"REQUIRED"` 之类的哨兵值。
* 离线校验：`python -I -B tools/probe/mir2_probe.py --check-scenario --scenario <file.json> [--var k=v ...]`
  —— 只加载+展开，不连网；rc=0 通过、rc=2 场景错误。
* `steps` 非空；每一步是对象，**恰好一个动作键**，外加可选元键 `timeout_ms` / `delay_ms` / `note`。

## `placement`：角色必须位于 map/x/y（由 `card_window.py` 应用）

```json
"placement": {"map": "0122", "x": 30, "y": 32, "note": "比奇国王 (29,32) 的邻格"}
```

* 本驱动器**只校验形状**（`map` 1..10 字符、`x`/`y` 为 0..65535 的整数），**不做预置位**：
  声明存在时会在 stderr 打印一条提醒。**别以为直接跑 `mir2_probe.py` 就落到了目标位** ——
  预置位由 `tools/conformance/card_window.py` 在**每侧 restore 之后、起栈之前**改测试库的
  `characters.MapName/CX/CY` 完成（C# = `mir2_test_db`、Rust = `mir2_test_db_r`），
  完成不了直接 rc=2，不会让 probe 去超时。详见 `docs/window-runbook-20261023.md` §1.5。
* 坐标要选**阻挡格之外**的空格（通常取目标 NPC 的邻格）：落在墙格时 oracle 登录会随机挪位。

## 步骤动作

| 动作 | 取值 | 线上展开 |
|---|---|---|
| `send` | 对象：必填 `ident`；可选 `recog`/`param`/`tag`/`series`；`text` / `raw_hex` / `text_from` **三选一** | 12B 头 + 体，按 `#1<EDCode>!` 发出 |
| `say` | 字符串 | `CM_SAY(3030)`，`recog=0`，文本按 GBK+EDCode |
| `click_npc` | int 或 `params` 名 | `CM_CLICKNPC(1010)`，`recog=<NPC actor id>` |
| `dlg_select` | 字符串 label，或 `{"label": "...", "recog": N}` | `CM_MERCHANTDLGSELECT(1011)`；`recog` 缺省取**最近一条 `SM_MERCHANTSAY(643)` 的 recog** |
| `wait_for` | int 或非空 int 数组 | 等**下一步开始之后**到达的第一条匹配 ident |
| `wait_frames` | `{"count": N, "idents": [...], "timeout_ms": M}` | 等 gamegate 连接**累计**帧数 ≥ N 且 idents 都出现过 |
| `wait_idents` | `{"idents": [...], "timeout_ms": M}` | 等 idents **全部**在 gamegate 连接上出现过（累计口径；`timeout_ms` 不满足即步骤失败） |
| `sleep` | int 毫秒，或 `{"ms": N}` | 纯等待，不发包 |

`send.text` 的编码口径 = C# `EDCode.EncodeString`：先 `text.encode("gbk")`，再 EDCode。
`send.raw_hex` 是**编码前**的 body 字节（十六进制），驱动器负责 EDCode。
`dlg_select` 若在收到 643 之前使用且没给 `recog`，直接 fail-closed（退出码 2），不会瞎猜。

## 等待语义（对拍必须可复现）

* `wait_for` 只匹配 **`seq` 大于本步开始时** 的帧 —— 上一步已经收到的同号帧不会被误当成本步的回包。
* `wait_frames` 计的是 gamegate 连接的**累计**帧（含进世界前收到的 `658/51`）。
* `wait_idents` 同样是**累计**口径，但要求列表里的 ident **全部**出现过；超时即步骤失败
  （退出码 1），步骤记录里写 `seen_idents` / `missing_idents` 便于定位差哪条。
* **进世界后的延迟段 dwell**：驱动器收到 `SM_LOGON(50)` 后先等 `--dwell-ms`（默认 **1200ms**）。
  Rust 首个 `SearchViewRange` 在 `enter_map + 1000ms`（`crates/world/src/entity.rs` 的
  `PLAYER_SEARCH_TIME`），dwell 不足会在延迟段帧（`10/41/654/657/100…`）到达之前
  `CM_SOFTCLOSE` 小退，对拍时被误判成「对侧独有」。`--dwell-ms 0` = 旧行为（只
  `pump --between-ms`），**只用于复现旧问题**，对拍不要用。
  `--dwell-ms` 必须 ≥ 0；负值在参数校验期 fail-closed（退出码 2，不落 `run.json`）。
* 步骤间默认间隔 `--between-ms`（默认 200ms）；单步可用 `delay_ms` 覆盖。等待本身用 `timeout_ms`。

## 参数

`text` 是字面量，**不与 params 做替换**；要把文本参数化（如物品名）请用 `text_from: "<params 名>"`——
该名字必须已在 `params` 或 `--var` 里给出，否则加载期 fail-closed。

`params` 里可放 int 或字符串；动作里的字符串会按「先查 params、再按十进制 int」解析：
`{"click_npc": "npc_actor"}`。`send` 的 `recog`/`param`/`tag`/`series` 同口径
（`ident` 只收 int）；解析失败或越界（`recog` 为 i32，其余 u16）在**加载期**就报 `scenario-error`，
不会拖到发送时才变成 `frame-parse`。命令行 `--var npc_actor=456` 覆盖场景默认值。
换一台实例跑 `dice_chain` 时，actor id 变了就只改 `--var`，文件不动。

> **改 `scenarios/*.json` 后一定要重跑 `--selftest`，但要知道它只覆盖 3 个文件**：`--selftest` 的
> `scenario-dsl` 用例只加载 `login_only.json` / `gm_time.json` / `dice_chain.json`（`mir2_probe.py:1286-1288`），
> 对这 3 个文件里的步数/idents/体做断言——动它们而不重跑 selftest 会漏回归。
> 其余场景文件 selftest **读不到**：改完必须逐个 `--check-scenario --scenario <path>`（加载期 fail-closed），
> 否则写坏了也没有任何门禁会红。两条命令都要跑，别互相替代。

> `dice_chain` 末步是**分支相关**断言：`@ya52` 持筹码包走 `#ACT` 回 `643`，无筹码包走 `#ELSEACT MESSAGEBOX` 回 `SM_MENU_OK(767)`，
> 因此该步等 `[643, 767]`——单等 `643` 会变成"无包账号必然超时"的恒失败断言。

> `npc_script_chain` 是**多步标签链**（R1）：点比奇国王(0122) 后连发 5 次 `dlg_select`
> （`@@buildguildnow` → `@buildguildexp` → `@main` → `@requestcastlewarA` → `@exit`），
> 断言 `643 → 763 → 643 → 643 → 643 → 644`。`required: ["npc_actor"]` 走 643.recog 发现；
> 链里没有 `@ask`（部署件无此标签）也没有 `@back`（无 `<.../@back>` 链接 ⇒ `LableIsCanJmp` 门控下零帧），
> 逐条 oracle 出处与理由见 `docs/npc-script-chain-scenario-20261023.md`。

> `combat_strike` 是**唯一一条主动发起攻击**的场景（R3）：`placement` 到 map 4 封魔谷 (256,232) →
> `CM_TURN(3010)` 定状态化朝向 → `CM_HIT(3014)` → `wait_for 31`（`SM_STRUCK`）；目标是正上方格
> (256,231) 的固定刷怪点 `恶魔弓箭手`（`MonGen.txt:90`，Range=0/Count=1；Race=112 `ArcherGuard`，
> **非 FixedHideMode**）。
> 三个容易写错的点，逐条 oracle 出处见 `docs/combat-strike-scenario-20261023.md`：
> ① **`CM_TURN`/`CM_HIT` 的 X/Y 打包在 `recog` 里**（`WorldServer.cs:831-844`：
> `nParam1=LoWord(Recog)`、`nParam2=HiWord(Recog)`、`wParam=Tag=Dir`，`param` 不被读）
> ⇒ `recog` 必须写 `(Y<<16)|X`，不是「X 放 recog、Y 放 param」；
> ② **必须先 `CM_TURN` 定朝向**——`CM_HIT` 取目标走 `GetPoseCreate()` 的**状态化 `Dir`**，
> 报文里的 `tag` 只管动作帧（`PlayObject.Attack.cs:26` 早于 `BaseObject.Attack.cs:12` 的 `Dir=nDir`）；
> ③ **本方看不到自己的 `SM_HIT(14)`**（`PlayObject.Message.cs:1433-1437` 自我过滤），
> 命中只能等受击者的 `SM_STRUCK(31)`。
> **本条现在是 BLOCKED-FIXTURE，不许当窗口输入**：目标必须既「非 FixedHide」又「打得动」——
> Race=95 的 `僵尸2`/`石墓尸王` 是 `DigOutZombi`（出生即 `FixedHideMode=true`，按 oracle 的隐藏视野门
> 永不出土、`SendRefMsg` 也不广播 ⇒ 恒超时），而名字不在 `monsters` 表里的 `弓箭护卫` 等根本不刷。
> 剩下能刷的非隐藏目标只有 112/102/115，AC 都 ≥18，而基线角色没有一档能同时满足
> 「DC > AC」与「HitPoint ≥ SPEED-1」（最好的一对 `m3st0 × 恶魔弓箭手` 单击也只有 33%）
> ⇒ 现在跑必然 `wait_for` 超时。缺的夹具与证明见文档 §6；另两条登记：
> 「没打中」支零协议帧（只有明文 `+GD` ack、`ident=null`，`wait_for` 匹配不到，扩展建议见文档 §4）、
> `rules.json` 缺 ident 31（另卡 `openmir2-conformance-rule-sm-struck-31` 在办）。

## 驱动器的自动握手（对拍不要关）

照 `src/BotSrv/Player/RobotPlayer.Operate.cs`：

* 收到 `SM_SENDNOTICE(658)` -> 回 `CM_LOGINNOTICEOK(1018)`；
* 收到 `SM_LOGON(50)` -> 发 `CM_QUERYBAGITEMS(81, recog=1)`；
* 服务端单字节 `'*'` 心跳 -> 回 `'*'`。

`CM_LOGINNOTICEOK` 的 `recog` 在 BotSrv 里是客户端 tick（1284 行），驱动器固定 **0**：
服务端不读该字段，固定值才能让 Rust / C# 两次运行的字节可比。登记为 **D-probe-1**。
调试可用 `--no-auto-handshake` 关掉，但那样两侧会缺同样的应答，不适合对拍。

## 证据文件

### `frames.ndjson`（一行一条 s2c 帧）

固定字段：`seq`、`t_ms`（相对本次运行起点）、`phase`（`login`/`selgate`/`gamegate`）、
`conn`、`ident`、`recog`、`param`、`tag`、`series`、`body_len`、`body_hex`、`body_sha256`。
附加：`frame_class`、`tail`（`!` 或 `!$`）、`raw_len`、`body_text`（GBK 展示，仅供人看）。

**`body_hex` 是线上原样字节**，不是「解码后的明文」：GameSvr 有的消息把多段
`EDCode.EncodeMessage(...)` 用**原样 `'/'`** 拼起来（如 `SM_ADJUST_BONUS(811)`），
所以 body 里允许出现不在 `0x3C..0x7B` 的字节。`body_text` 只是尽力解码的预览，
比较器请比 `body_hex` / `body_sha256`。

`frame_class` 取值：

* `protocol`：正常 12B 头协议帧；
* `server_ack_time`：服务端明文动作 ack `#+GD/<rtime>!`
  （`src/Modules/SystemModule/MessageSettings.cs` 的 `sSTATUS_GOOD`）。它不是协议帧，
  `ident/recog/param/tag/series` 为 `null`，`body_hex` 是 `+GD/<rtime>` 明文。
  其中的 `rtime` 是服务端时间（C# 口径 = `Environment.TickCount`，**≤10 位十进制、可为负**；
  Rust 侧是 13 位 epoch-ms），**不能逐字节对拍**，比较器应按 `frame_class` 排除或只比前缀。
* `server_action_ack`：`PlayObject.Attack.cs` 的 `SendSocket("=DIG")` / `("+PWR")` 一族短明文
  （`DIG`/`PWR`/`LNG`/`ULNG`/`WID`/`UWID`/`FIR`/`UFIR`/`CRS`/`UCRS`/`TWN`/`UTWN`/`CID`/`UCID`，
  前缀 `'='` 或 `'+'`）。同样不是协议帧，`ident/recog/...` 为 `null`，`body_hex` 是明文标签。

明文帧的判据是**文法**（固定前缀 + 纯十进制 / 标签全集），不是「EDCode 解不开就当明文」：
`+GD/` 后不是 `-?[0-9]+`、或未知的短明文，仍然 fail-closed（退出码 2），不会被当成帧吞掉。
两者都在 12B 头长度闸**之前**分类 —— C# 的 `#+GD/1792667727!` 内层只有 14B，先过长度闸就会被拒。

### `run.json`

`argv`（`--password` 已打码）、`env`、起止墙钟、`chain`（登录四步的 ident/网关地址/session/角色列表/
`SM_LOGON` 的 actor id 与落点）、`dwell`（进世界后延迟段：请求/实际时长、窗口前后见过的 ident、
新到的 ident；**加性字段**，`schema_version` 仍为 1，旧比较器忽略）、`steps`（每条步骤的发送帧
十六进制与等待结果）、`auto`（自动握手）、
`frames_total` / `frames_by_conn` / `ident_hist`、`unclassified`、`exit`。

## 退出码

* `0` 全绿；
* `1` 链路或场景失败（网关不可达、登录被拒、`wait_for` / `wait_idents` 超时、角色不存在等）；
* `2` UNCLASSIFIED：帧解析失败（越界字节/未闭合帧）、规则没覆盖（`531` 改密、
  `dlg_select` 无 643 锚点）、场景 DSL 非法、必需参数缺失（参数校验在建证据目录之前，
  因此不落 `run.json`，只在 stderr 报缺哪几个参数）、内部错误。

工具是 fail-closed 的：解析不了就退出 2 并写进 `unclassified`，**绝不静默当绿**。

## 自测

```sh
python tools/probe/mir2frame.py selftest
python tools/probe/mir2_probe.py --selftest
```

`--selftest` 用 `../fixtures/r9/` 的离线 s2c 抓包跑「切帧 -> 解析 -> 落 ndjson」全链路，
不连网、不起进程；含「改坏必红」的负向用例（载荷字节改成越界值必须报错）。

## fixtures 来源

`../fixtures/r9/` 是 r9 真机窗口的代理抓包（`E:/tmp/window/r9/proxy/`）原样复制：
`conn-0001.s2c.bin` 44 帧、`conn-0002.s2c.bin` 106 帧（含 4 条 `SM_MERCHANTSAY(643)`，
即赌场骰子链的服务端回包）。`*.c2s.bin` 是同一窗口的客户端字节，用于核对帧构造的已知答案向量。
`sha256` 见提交说明；不要在 fixture 上做任何改写（自测里的 tamper 是内存副本）。
