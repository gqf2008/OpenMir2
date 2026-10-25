# probe NPC 寻址（name: / pos:）— 2026-10-25

- 卡：`openmir2-probe-npc-discovery`（tier **L**，owner `openmir2-dev-2`，branch `card/probe-npc-discovery`，base `a60e04ad9`）。
- 定位：`click_npc` 原先只接受 **actor id** 或 `auto:last_turn`（「最近一个非己 `SM_TURN`」），
  于是 `run3 #24 castle_merchant_tax_upgrade` 与 W-CO-*（castle-official）这类**知道 NPC 是谁、却点不到**的场景卡在门口。
  本卡给 DSL 补两种**按可见对象**寻址：`name:<NPC 名>` 与 `pos:<x>,<y>`，并且**歧义一律 fail-closed**。
- 写集：只动 `tools/probe/mir2_probe.py` + 本文件。**不改** `run3-scenarios.json`（那是另一张卡）、
  `tools/conformance/card_window.py`、`tools/window/run_batch.sh`。

## 1. `click_npc` 的四种写法

| 写法 | 含义 | 命中要求 | 备注 |
|---|---|---|---|
| `click_npc: 12345` 或 params 名 | 直接给 actor id（既有行为） | 不校验 | 与 oracle 的 `CM_CLICKNPC.recog = ActorId` 同口径 |
| `click_npc: auto:last_turn` | 最近一个非己 `SM_TURN(10)` 的 `recog` | 无兜底 | 既有行为；**只在没有更精确写法时用** |
| `click_npc: name:沙巴克城堡管理` | 按**名尾 `ShowName`** 精确匹配可见对象 | **恰好 1 个** | 名尾口径见 §2 |
| `click_npc: pos:7,16` | 按**坐标**（`SM_TURN` 的 `Param=x`/`Tag=y`）匹配可见对象 | **恰好 1 个** | 空格/多对象都算不中 ⇒ 红 |

- 三种新写法都**仍发 `CM_CLICKNPC`**，`recog` 取该对象 actor id，`param/tag/series/body` 全 0 —— **不新造协议**。
- 未知写法（如 `auto:lastturn`、`foo:bar`）在**加载期**就 `ScenarioError`（退出码 2），不会拖到运行期：
  错误串写明只认 `auto:last_turn` / `name:<名>` / `pos:<x>,<y>`。
- `name:` 空名、`pos:` 非「十进制 x,y」或坐标越界（>65535）同样加载期拒绝。

## 2. 名字从哪来（oracle 口径，不是猜）

`SM_TURN(10)` 的帧体 = `Encode(8B CharDesc)` [ + `Encode("ShowName/Color")` ]，两段**各自** EDCode：

- oracle：`src/M2Server/Player/PlayObject.Message.cs:1549-1555` —— `Msg` 非空时追加 `EncodeString($"{Msg}/{nObjCount}")`；
  出现帧路径（`PlayObject.cs:3240` 的 `SendMsg(..., baseObject.GetShowName())`）**带非空 Msg**；
- Rust 同构：`mir2-rs/bins/game-svr/src/downlink.rs::turn_pair`（`Encode(CharDesc)` + `Encode("ShowName/Color")`）；
- **`CM_TURN` 下行路径无名尾**（`turn_pair_nameless`，`PlayObject.cs:1922` 的 `sMsg` 恒空）⇒ 该对象的 `name=None`，
  只能按 `pos:` / actor id 寻址。probe 的实现据此只解析「体长 > 11 字符」的帧（`SM_TURN_DESC_CHARS = 11`
  是 `Encode(8B)` 的可逆长度）。

实测样例（`E:/tmp/window/run2/cs-probe` 的一条出现帧）：帧体解码后名尾 = `〖庄〗/255` ⇒ `name=〖庄〗`。

## 3. 歧义三态（fail-closed，绝不「挑第一个」）

| 命中数 | 结果 | 错误串里给什么 |
|---:|---|---|
| 1 | 正常发帧，`recog` = 该对象 actor id | 运行记录多一条 `kind=click_npc`（`selector`/`actor`/`name`/`x`/`y`） |
| 0 | `StepFailure`（rc=1） | `name:` ⇒ 列出**全部**可见对象；`pos:` ⇒ 列出该格 **±3** 邻近可见对象（若邻域没有，退回全量） |
| ≥2 | `StepFailure`（rc=1） | 「命中 N 个对象（歧义，fail-closed，不挑第一个）」+ 每个候选的 `name/(x,y)/actor` |

候选格式统一为 `name=<名或 <无名>> (x,y) actor=<id>`，操作者可直接把 `actor=<id>` 抄回 `click_npc` 重跑。
**没有任何一步会回落到 `auto:last_turn`**：名字不存在就是不存在（selftest 牙 c 钉住这一点）。

## 4. 与 `param_discovery.py` / `--discovery-scenario` 的关系

- `tools/probe/param_discovery.py` 的 `npc_actor` 规则**在点击之后**推导：优先取 `SM_MERCHANTSAY(643).recog`，
  否则退化为「首个非己 `SM_TURN`」（**规则**，不是帧语义，不计入发现能力）。
  它解决的是「我点到的到底是谁」；`name:`/`pos:` 解决的是「我点的是谁」——**先定向，再推导**。
  用 `name:沙巴克城堡管理` 定向后，`643.recog` 才能作为**该 NPC** 的已证 actor id（`derived`），
  而不是「某个应答者」。
- 设计文档 `docs/param-discovery-design-20261023.md:117` 提到的 `--discovery-scenario` 是**尚未实现**的设想
  （仓库里 grep 不到该选项）。本卡的 `name:`/`pos:` **不依赖**它：定向能力直接落在 `click_npc` 上，
  现有 discovery 场景（`npc_click_auto`）只要把 `click_npc: auto:last_turn` 改成 `click_npc: name:<名>`
  即可，无需新的 discovery 机制。它也不是 `--discovery-scenario` 的替代品：后者（若做）是多场景编排，
  本卡只给单步寻址。

## 5. 在 #24 与 W-CO-* 上怎么用

- **#24 `castle_merchant_tax_upgrade`**：部署里城堡商人是固定 NPC（`Npcs.txt` 的 `0150 沙巴克城堡管理`；
  `MerChant.txt:83` 的 `0151 武器商行` 是升级腿的买卖人）。场景里把
  `click_npc: auto:last_turn` 换成 `click_npc: name:沙巴克城堡管理`（或该 NPC 的实际坐标 `pos:<x>,<y>`），
  就能**确定**点到的是城堡管理而不是旁边恰好最近的那个。注意：**本卡不改 run3 的 #24 `blocked` 文案**，
  解封是另一张卡（场景 JSON + 清单行两处同步）。
- **W-CO-*（castle-official）**：`@main` 说词逐字节（mask `$DATETIME`）要求先点中 `沙巴克城堡管理`；
  用 `name:` 定向后，`643` 的应答者身份不再靠「最近」假设，W-CO-1 的前置从**规则**变**已证**。
  动作段（W-CO-6）仍受「测试专用城主注入」限制，与本卡无关。

示例片段（`<-` 为新增写法）：

```json
{
  "name": "castle_merchant_tax_upgrade",
  "steps": [
    { "click_npc": "name:沙巴克城堡管理" },
    { "wait_for": 643, "timeout_ms": 3000 },
    { "dlg_select": "@buy" },
    { "wait_for": 645, "timeout_ms": 3000 }
  ]
}
```

## 6. 尚未覆盖（诚实清单）

| 缺口 | 现状 | 影响 |
|---|---|---|
| 无名尾对象（`CM_TURN` 路径的 `SM_TURN`） | `name=None`，`name:` 匹配不到 | 该类对象只能用 `pos:`/actor id |
| 还没收到 `SM_TURN` | 红（「还没看到任何非己对象…先 wait_for」），不猜 | 场景需在 `click_npc` 前等到出现帧（进图 AOI 内通常已有） |
| 同一 actor 的多条出现帧 | 按 actor id **去重**（保留最后坐标/名字） | 不会把「同一个人转身两次」误判成歧义 |
| 怪物/玩家也有名尾 | `name:` 不区分对象类别 | 是特性不是缺陷：按名点怪也成立；要限定 NPC 由调用方保证 |
| 坐标语义 | `pos:` 只匹配 `SM_TURN` 的 `Param/Tag`，不做寻路/相邻/朝向校验 | 点到之后是否够得着由 oracle 的点击门决定（超时响亮失败） |
| `pos:` 与 params | 只收字面十进制坐标，不收 `--var` 名 | 需要动态坐标时先用 `send` 或扩展（未做，避免半吊子语义） |
| 真机 | **未做真机**（本轮只做到构造级 selftest；宿主内存紧张、私栈在 tester 手里） | 需要在窗口轮次里抽样冒烟一次（见卡面 ④） |

## 7. 验证

```sh
python -I -B tools/probe/mir2_probe.py --selftest
# => mir2_probe selftest: OK (18 checks; ...)   新增 dsl-npc-discovery
```

新增自测覆盖：schema 2 正 6 反、`check_scenario` 不把 `name:/pos:` 当 params 名、运行期 5 例（牙 a–d + 同格多对象）。
**4 条变异逐条变红再还原**（每条都从 HEAD 还原后重跑确认 `OK (18 checks)`）：

| 变异 | 红在哪 |
|---|---|
| 多命中 `if len(hits)==1` → `if hits`（挑第一个） | 「同名两处未被拒（会挑第一个）」 |
| 零命中不再早报（`if not hits and False`） | 牙 b：错误串不再给邻近对象 |
| 运行期不解析选择器（`if False and …_npc_selector`） | 牙 d：`FrameError: recog must be int, got NoneType` |
| 名字匹配恒真（`hits = list(actors)`） | 牙 c：「不存在的名字未被拒」 |

## 8. 残留物清单

- 入库：`tools/probe/mir2_probe.py`（+ 本文件）。
- 未入库：`E:/tmp/npc_a.py`、`E:/tmp/npc_b.py`、`E:/tmp/npc_c1.py`、`E:/tmp/mutate_npc.py`（一次性补丁/变异脚本）。
- 未触碰：`run3-scenarios.json`、`card_window.py`、`run_batch.sh`、`src/**`、mysqld；
  未起服务端、未占端口、未申请 `stack-claim`。
